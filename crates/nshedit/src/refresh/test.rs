use std::io::{Read, Seek};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::el::blank_editline;
use crate::literal::{EL_LITERAL, literal_get};
use crate::terminal::{T_CE, T_STR};

/// A screen row of `dlen` cells plus the reserved terminator slot at
/// `d[dlen]` that both `re_insert` and `re_delete` write, holding `s` and
/// NUL-padded. The sentinel goes in the terminator slot so a test can tell
/// "wrote the terminator" from "returned before reaching it".
const DLEN: i32 = 8;
const SENTINEL: u32 = 0xFFFF;

fn row(s: &str) -> Vec<u32> {
    let mut d = vec![0u32; DLEN as usize + 1];
    for (i, c) in s.chars().enumerate() {
        d[i] = u32::from(c);
    }
    d[DLEN as usize] = SENTINEL;
    d
}

/// The row read as the terminal would: up to the first NUL.
fn shown(d: &[u32]) -> String {
    d[..DLEN as usize]
        .iter()
        .take_while(|&&c| c != 0)
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

fn cells(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

/// An editor with a `DLEN`-column, three-row screen and no descriptors.
///
/// Both images are allocated with the extra terminator cell at `[t_size.h]`
/// that `re_putc` and `re_putliteral` write before every wrap, because
/// `terminal_alloc_buffer` allocates `t_size.h + 1`. Descriptor 0 is the test
/// runner's own stdout, hence the -1s; [`emitted`] swaps in a real file where
/// a test needs to read the byte stream back.
fn screen() -> EditLine {
    let mut el = blank_editline();
    el.el_infd = -1;
    el.el_outfd = -1;
    el.el_errfd = -1;
    el.el_terminal.t_size = CoordT { h: DLEN, v: 3 };
    el.el_display = vec![vec![0u32; DLEN as usize + 1]; 3];
    el.el_vdisplay = vec![vec![0u32; DLEN as usize + 1]; 3];
    el
}

/// Everything `f` writes to either of the editor's output descriptors.
///
/// The C writes through `FILE *`s a test could replace with memory streams;
/// this port writes straight to descriptors, so the capture has to be a real
/// file. Both `el_outfd` and `el_errfd` point at it, so a write to the wrong
/// one still shows up rather than vanishing.
fn emitted(el: &mut EditLine, f: impl FnOnce(&mut EditLine)) -> Vec<u8> {
    static N: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "nshedit-refresh-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    el.el_outfd = file.as_raw_fd();
    el.el_errfd = file.as_raw_fd();
    f(el);
    el.el_outfd = -1;
    el.el_errfd = -1;
    let mut bytes = Vec::new();
    file.rewind().unwrap();
    file.read_to_end(&mut bytes).unwrap();
    drop(file);
    let _ = std::fs::remove_file(path);
    bytes
}

/// `CoordT` is a literal translation of the C struct and derives nothing, so
/// a position is compared as the pair it is.
fn at(c: &CoordT) -> (i32, i32) {
    (c.h, c.v)
}

/// The `(buf, end)` pair `re_putliteral` takes, laid out as `prompt_print`
/// lays it out: the bracketed sequence, then the closing delimiter at `end`,
/// then the visible character the sequence decorates at `end + 1`.
fn literal(seq: &[u32], visible: u32) -> (Vec<u32>, usize) {
    let mut buf = seq.to_vec();
    let end = buf.len();
    buf.push(u32::from(b'%'));
    buf.push(visible);
    (buf, end)
}

/// `re_insert` opens the gap right-to-left and the cells that fall off the
/// end of the row are discarded rather than growing it — the row is a
/// fixed-width screen line, not a string.
// [spec:libedit:sem:refresh.re-insert-fn/test]
#[test]
fn inserting_into_a_row_shifts_the_tail_right_and_drops_what_overflows() {
    let mut d = row("abcd");
    re_insert(&mut d, 2, DLEN, &cells("XY"), 2);
    assert_eq!(shown(&d), "abXYcd");
    assert_eq!(d[DLEN as usize], 0, "the terminator slot is always written");

    // A full row: 'g' and 'h' are pushed past the end and lost.
    let mut d = row("abcdefgh");
    re_insert(&mut d, 2, DLEN, &cells("XY"), 2);
    assert_eq!(shown(&d), "abXYcdef");
}

/// The clamp is to what fits from `dat` onwards, and it is observable in
/// the shift: two cells at column 6 of an eight-cell row have nowhere to
/// move to, so the row's existing content is left exactly where it was.
// [spec:libedit:sem:refresh.re-insert-fn/test]
#[test]
fn inserting_clamps_to_the_room_left_in_the_row() {
    let mut d = row("abcdefgh");
    re_insert(&mut d, 6, DLEN, &cells("XY"), 5);
    assert_eq!(shown(&d), "abcdefXY");
}

/// Both guards return before the terminator write, so a non-positive count
/// and the negative count a `dat` past `dlen` manufactures leave the row
/// completely alone. Nothing here may index outside it.
// [spec:libedit:sem:refresh.re-insert-fn/test]
#[test]
fn inserting_nothing_touches_no_cell_at_all() {
    let mut d = row("abcd");
    let before = d.clone();
    re_insert(&mut d, 2, DLEN, &cells("XY"), 0);
    assert_eq!(d, before);
    re_insert(&mut d, 2, DLEN, &cells("XY"), -1);
    assert_eq!(d, before);
    // `dat > dlen` clamps `num` negative, which every later step skips.
    re_insert(&mut d, DLEN + 1, DLEN, &cells("XY"), 3);
    assert_eq!(d, before, "including the terminator slot");
}

/// `re_delete` slides the tail down and terminates the row, but the cells
/// it vacated keep stale copies — the row still reads correctly only
/// because the string's own NUL slides down with it.
// [spec:libedit:sem:refresh.re-delete-fn/test]
#[test]
fn deleting_from_a_row_slides_the_tail_down_over_stale_cells() {
    let mut d = row("abcdef");
    re_delete(&mut d, 2, DLEN, 3);
    assert_eq!(shown(&d), "abf");
    assert_eq!(
        d[5],
        u32::from('f'),
        "the vacated cell keeps its stale copy"
    );
    assert_eq!(d[DLEN as usize], 0);
}

/// A deletion reaching the end of the row is a truncation: the row is cut
/// at `dat` with no shifting, and the early return means even the
/// terminator slot is left as it was.
// [spec:libedit:sem:refresh.re-delete-fn/test]
#[test]
fn deleting_to_the_end_of_the_row_just_truncates_it() {
    let mut d = row("abcdef");
    re_delete(&mut d, 4, DLEN, 10);
    assert_eq!(shown(&d), "abcd");
    assert_eq!(d[5], u32::from('f'), "nothing was shifted");
    assert_eq!(d[DLEN as usize], SENTINEL, "returned before the terminator");

    let mut d = row("abcdef");
    let before = d.clone();
    re_delete(&mut d, 2, DLEN, 0);
    assert_eq!(d, before);
}

/// `re_nextline` moves to column 0 of the next row, and on the last row
/// emulates a scroll by rotating the virtual rows instead. ERR-terminal-47
/// is that `el_display` — the image of what is physically on screen — is
/// deliberately NOT rotated with it.
// [spec:libedit:sem:refresh.re-nextline-fn/test]
#[test]
fn the_next_line_rotates_only_the_virtual_rows_at_the_bottom() {
    let mut el = blank_editline();
    el.el_terminal.t_size.v = 3;
    el.el_vdisplay = vec![cells("Aa"), cells("Bb"), cells("Cc")];
    el.el_display = vec![cells("Dd"), cells("Ee"), cells("Ff")];

    // Room below: advance, and only the column resets.
    el.el_refresh.r_cursor = CoordT { h: 5, v: 1 };
    re_nextline(&mut el);
    assert_eq!((el.el_refresh.r_cursor.h, el.el_refresh.r_cursor.v), (0, 2));
    assert_eq!(el.el_vdisplay[0][0], u32::from('A'), "no rotation yet");

    // On the last row: rotate up, and stay put vertically.
    el.el_refresh.r_cursor.h = 7;
    re_nextline(&mut el);
    assert_eq!((el.el_refresh.r_cursor.h, el.el_refresh.r_cursor.v), (0, 2));
    assert_eq!(el.el_vdisplay[0][0], u32::from('B'));
    assert_eq!(el.el_vdisplay[1][0], u32::from('C'));
    assert_eq!(el.el_vdisplay[2][0], 0, "only the first cell is cleared");
    assert_eq!(
        el.el_vdisplay[2][1],
        u32::from('a'),
        "the recycled row keeps the rest of its stale content"
    );
    assert_eq!(
        el.el_display.iter().map(|r| r[0]).collect::<Vec<_>>(),
        cells("DEF"),
        "ERR-terminal-47: the real image is left behind"
    );
}

// ---------------------------------------------------------------------------
// re_printstr
// ---------------------------------------------------------------------------

/// The debug dump is omitted, not emulated. Its destination in the C is
/// `el_errfile`, a borrowed `FILE *` the port carries as an opaque pointer
/// with no writer behind it, and the whole function plus its twelve call sites
/// in `re_update_line` live behind `DEBUG_REFRESH`, which no shipped build
/// defines (ERR-terminal-65). So the contract is that it writes nothing and
/// reads nothing — including for the `MB_FILL_CHAR` cells the C's `& 0177`
/// mask would have printed as `\177`, and for a range that runs past the
/// row's terminator.
// [spec:libedit:sem:refresh.re-printstr-fn/test]
#[test]
fn the_debug_dump_emits_nothing_and_disturbs_nothing() {
    let mut el = screen();
    el.el_refresh.r_cursor = CoordT { h: 3, v: 1 };
    el.el_cursor = CoordT { h: 5, v: 2 };
    el.el_vdisplay[1] = row("live");
    let before = (el.el_vdisplay.clone(), el.el_display.clone());

    let out = emitted(&mut el, |el| {
        let d = el.el_vdisplay[1].clone();
        re_printstr(el, "d", &d);
        re_printstr(el, "MB_FILL_CHAR", &[MB_FILL_CHAR, 0, 0x1b, u32::from('~')]);
        re_printstr(el, "", &[]);
    });

    assert_eq!(out, b"", "nothing reaches either descriptor");
    assert_eq!((el.el_vdisplay.clone(), el.el_display.clone()), before);
    assert_eq!(at(&el.el_refresh.r_cursor), (3, 1));
    assert_eq!(at(&el.el_cursor), (5, 2));
}

// ---------------------------------------------------------------------------
// re_putliteral
// ---------------------------------------------------------------------------

/// One magic cell stands for the whole bracketed sequence plus the visible
/// character glued to it, so the image charges the sequence no columns while
/// printing the cell replays every byte of it. The closing delimiter at `end`
/// is skipped; `end + 1` is what gets encoded and what decides the width.
// [spec:libedit:sem:refresh.re-putliteral-fn/test]
#[test]
fn a_literal_becomes_one_cell_that_replays_its_whole_byte_string() {
    let mut el = screen();
    let (buf, end) = literal(&cells("\u{1b}[1m"), u32::from('X'));
    re_putliteral(&mut el, &buf, end);

    let c = el.el_vdisplay[0][0];
    assert_eq!(c, EL_LITERAL, "the first index, with the marker bit");
    assert_eq!(literal_get(&mut el, c), b"\x1b[1mX");
    assert_eq!(
        at(&el.el_refresh.r_cursor),
        (1, 0),
        "one column, the visible character's own width"
    );
    assert_eq!(el.el_vdisplay[0][1], 0, "no fill cell for a single column");
}

/// ERR-terminal-45, the same rule `re_putc` follows: a visible character
/// `wcwidth` calls zero-width still costs a column here, while
/// `re_refresh_cursor` charges it none. The cell is written and the column
/// advances by one, not by zero.
// [spec:libedit:sem:refresh.re-putliteral-fn/test]
#[test]
fn a_zero_width_visible_character_still_costs_a_column() {
    let mut el = screen();
    // NUL is the one character `wcwidth` answers 0 for in every charset, so
    // this does not depend on the test host's locale.
    assert_eq!(locale::wcwidth(locale::charset(), 0), 0);

    let (buf, end) = literal(&cells("\u{1b}k"), 0);
    re_putliteral(&mut el, &buf, end);

    assert_eq!(el.el_vdisplay[0][0], EL_LITERAL);
    assert_eq!(at(&el.el_refresh.r_cursor), (1, 0));
}

/// Both of `literal_add`'s refusals abandon the sequence without touching the
/// image or the cursor, and they are distinguished by the width: a negative
/// one means the visible character is not printable, a zero return with a
/// non-negative width means the table could not take it.
// [spec:libedit:sem:refresh.re-putliteral-fn/test]
#[test]
fn a_refused_literal_leaves_the_image_and_the_cursor_alone() {
    let mut el = screen();
    el.el_refresh.r_cursor = CoordT { h: 2, v: 1 };
    let before = el.el_vdisplay.clone();

    // A lone surrogate is unprintable in every charset, so `wcwidth` is -1.
    let (buf, end) = literal(&cells("\u{1b}["), 0xD800);
    re_putliteral(&mut el, &buf, end);
    assert_eq!(el.el_vdisplay, before);
    assert_eq!(at(&el.el_refresh.r_cursor), (2, 1));
    assert_eq!(el.el_literal.l_idx, 0, "nothing was interned either");

    // The other refusal: the port bounds the table index where the C wraps
    // into the marker bit, and past that bound `literal_add` returns 0 with
    // the visible character's real width still in `w`.
    el.el_literal.l_idx = usize::MAX;
    let (buf, end) = literal(&cells("\u{1b}["), u32::from('X'));
    re_putliteral(&mut el, &buf, end);
    assert_eq!(el.el_vdisplay, before);
    assert_eq!(at(&el.el_refresh.r_cursor), (2, 1));
}

/// Reaching the right margin terminates the virtual row at `t_size.h` — the
/// extra cell every row is allocated with — and wraps. The terminator goes in
/// at the margin even though the magic cell went in one column earlier, which
/// is what keeps the row readable as a string.
// [spec:libedit:sem:refresh.re-putliteral-fn/test]
#[test]
fn a_literal_in_the_last_column_terminates_the_row_and_wraps() {
    let mut el = screen();
    el.el_vdisplay[0] = row("abcdefgh");
    el.el_refresh.r_cursor = CoordT { h: DLEN - 1, v: 0 };

    let (buf, end) = literal(&cells("\u{1b}["), u32::from('X'));
    re_putliteral(&mut el, &buf, end);

    assert_eq!(el.el_vdisplay[0][DLEN as usize - 1], EL_LITERAL);
    assert_eq!(
        el.el_vdisplay[0][DLEN as usize], 0,
        "the reserved terminator slot, not the sentinel it held"
    );
    assert_eq!(at(&el.el_refresh.r_cursor), (0, 1));
}

/// A double-width literal is where `re_putliteral` and `re_putc` part company:
/// there is no pre-padding loop here, so the sequence is not pushed to the
/// next row to keep it whole. Its magic cell is written where it falls, the
/// fill is clamped at the margin, and `r_cursor.h` overshoots `t_size.h`
/// before `re_nextline` resets it.
///
/// Whether a character has two columns at all is the locale's decision, and
/// the C locale calls everything above U+007E unprintable — so this pins both
/// answers rather than assuming the host's.
// [spec:libedit:sem:refresh.re-putliteral-fn/test]
#[test]
fn a_double_width_literal_is_truncated_at_the_margin_not_moved_off_it() {
    const CJK: u32 = 0x4E00;
    let wide = locale::wcwidth(locale::charset(), CJK) == 2;

    let mut el = screen();
    let (buf, end) = literal(&cells("\u{1b}["), CJK);
    re_putliteral(&mut el, &buf, end);
    if wide {
        assert_eq!(el.el_vdisplay[0][0], EL_LITERAL);
        assert_eq!(el.el_vdisplay[0][1], MB_FILL_CHAR, "the second column");
        assert_eq!(at(&el.el_refresh.r_cursor), (2, 0));
    } else {
        // C/POSIX: unprintable, so `literal_add` declines it outright.
        assert_eq!(el.el_vdisplay[0][0], 0);
        assert_eq!(at(&el.el_refresh.r_cursor), (0, 0));
    }

    // At the last column there is only one column left for a two-column
    // character, and with no pre-padding loop nothing is pushed off it.
    let mut el = screen();
    el.el_vdisplay[0] = row("abcdefgh");
    el.el_refresh.r_cursor = CoordT { h: DLEN - 1, v: 0 };
    let (buf, end) = literal(&cells("\u{1b}["), CJK);
    re_putliteral(&mut el, &buf, end);
    if wide {
        assert_eq!(el.el_vdisplay[0][DLEN as usize - 1], EL_LITERAL);
        assert_eq!(
            el.el_vdisplay[0][DLEN as usize], 0,
            "the terminator, not a fill cell: the clamp emptied the fill loop"
        );
        assert_eq!(at(&el.el_refresh.r_cursor), (0, 1));
    } else {
        assert_eq!(el.el_vdisplay[0][DLEN as usize - 1], u32::from('h'));
        assert_eq!(at(&el.el_refresh.r_cursor), (DLEN - 1, 0));
    }
}

// ---------------------------------------------------------------------------
// re_clear_lines
// ---------------------------------------------------------------------------

/// With the clear-to-end-of-line capability the rows are walked from the
/// bottom up, `r_oldcv` down to 0 **inclusive** — so row 0 is cleared even
/// when the previous line occupied nothing else — and the bare `\r\n` pair is
/// skipped for that last iteration alone.
///
/// ERR-terminal-52 is visible in the byte stream: the bare pairs move the real
/// terminal without telling `el_cursor`, so the `terminal_move_to_line` that
/// follows the first one computes its motion from row 0 and emits two more
/// newlines on top of the one already sent. The recorded cursor ends back at
/// the home position while the terminal is four rows lower.
// [spec:libedit:sem:refresh.re-clear-lines-fn/test]
#[test]
fn clearing_walks_the_rows_upwards_and_loses_track_of_the_cursor() {
    let mut el = screen();
    el.el_terminal.t_str = vec![None; T_STR];
    el.el_terminal.t_str[T_CE] = Some(b"ce\0".to_vec());
    el.el_terminal.t_flags = TERM_CAN_CEOL;
    el.el_refresh.r_oldcv = 2;

    assert_eq!(emitted(&mut el, re_clear_lines), b"\r\n\n\nce\r\ncece");
    assert_eq!(
        at(&el.el_cursor),
        (0, 0),
        "ERR-terminal-52: the model never saw the bare newlines"
    );
}

/// Without the capability nothing is erased at all: the path just scrolls
/// `r_oldcv` newline pairs to push the old text up, moves to what it believes
/// is the last row, and sends one more pair. The two extra newlines in the
/// middle are ERR-terminal-52 again — `terminal_move_to_line(2)` from a
/// recorded row 0, after two bare pairs have already moved the terminal there.
// [spec:libedit:sem:refresh.re-clear-lines-fn/test]
#[test]
fn without_the_clear_capability_the_old_text_is_only_scrolled_away() {
    let mut el = screen();
    el.el_terminal.t_flags = 0;
    el.el_refresh.r_oldcv = 2;

    assert_eq!(emitted(&mut el, re_clear_lines), b"\r\n\r\n\n\n\r\n");
    assert_eq!(at(&el.el_cursor), (0, 2));
}

/// The flag is the gate, not the capability. With `TERM_CAN_CEOL` set but no
/// `ce` string installed, `terminal_clear_EOL` falls back to writing
/// `t_size.h` spaces and — unlike the capability — advances the recorded
/// column by that many, which is what makes the *next* row's
/// `terminal_move_to_char(0)` emit a carriage return the capability path never
/// needs.
// [spec:libedit:sem:refresh.re-clear-lines-fn/test]
#[test]
fn a_missing_clear_capability_blanks_the_row_with_spaces_instead() {
    let mut el = screen();
    el.el_terminal.t_str = vec![None; T_STR];
    el.el_terminal.t_flags = TERM_CAN_CEOL;
    el.el_refresh.r_oldcv = 1;

    assert_eq!(
        emitted(&mut el, re_clear_lines),
        b"\r\n\n        \r        ",
    );
    assert_eq!(
        at(&el.el_cursor),
        (DLEN, 0),
        "the fallback leaves the column one past the last one it wrote"
    );
}

/// `r_oldcv == 0` — a previous line that fitted on one row — is not a no-op on
/// either path, and the two disagree about what "nothing to clear" means. The
/// capability path clears row 0 and moves nothing; the fallback path erases
/// nothing and still scrolls a line.
// [spec:libedit:sem:refresh.re-clear-lines-fn/test]
#[test]
fn a_single_row_of_history_still_costs_output_on_both_paths() {
    let mut el = screen();
    el.el_terminal.t_str = vec![None; T_STR];
    el.el_terminal.t_str[T_CE] = Some(b"ce\0".to_vec());
    el.el_terminal.t_flags = TERM_CAN_CEOL;
    el.el_refresh.r_oldcv = 0;
    assert_eq!(emitted(&mut el, re_clear_lines), b"ce");

    let mut el = screen();
    el.el_terminal.t_flags = 0;
    el.el_refresh.r_oldcv = 0;
    assert_eq!(emitted(&mut el, re_clear_lines), b"\r\n");
}
