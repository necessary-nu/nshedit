use std::io::{Read, Seek};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::el::CoordT;
use crate::literal::{EL_LITERAL, literal_get};
use crate::terminal::{T_CE, T_STR};
use crate::testkit::headless_editor;

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

/// An editor with a `DLEN`-column, three-row screen.
///
/// Narrow on purpose: every wrap, margin and rotation case here is reached by
/// running off the end of a row, which an 80-column screen would put out of
/// reach of a readable test string. [`emitted`] swaps a real file in over the
/// closed descriptors where a test needs to read the byte stream back.
fn screen() -> EditLine {
    headless_editor(DLEN, 3)
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
    let mut el = screen();
    el.el_vdisplay = vec![cells("Aa"), cells("Bb"), cells("Cc")];
    el.el_display = vec![cells("Dd"), cells("Ee"), cells("Ff")];

    // Room below: advance, and only the column resets.
    el.el_refresh.r_cursor = CoordT { h: 5, v: 1 };
    re_nextline(&mut el);
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 0, v: 2 });
    assert_eq!(el.el_vdisplay[0][0], u32::from('A'), "no rotation yet");

    // On the last row: rotate up, and stay put vertically.
    el.el_refresh.r_cursor.h = 7;
    re_nextline(&mut el);
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 0, v: 2 });
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

/// The dump goes to the error descriptor — never the output one, so it cannot
/// be mistaken for screen content — as the label, a colon, the region in
/// double quotes and a CR/LF. Every cell is masked with `0177`, which is what
/// makes it legible for ASCII and meaningless for anything else:
/// `MB_FILL_CHAR` is `(wint_t)-1` and prints as `\177`, and an embedded NUL is
/// dumped like any other cell rather than ending the region.
///
/// Nothing in the port calls it — the twelve call sites in `re_update_line`
/// are `DEBUG_REFRESH`-only and are not ported (ERR-terminal-65) — so it
/// reaches a terminal only where someone wires it up, and it disturbs neither
/// image nor cursor when they do.
// [spec:libedit:sem:refresh.re-printstr-fn/test]
#[test]
fn the_debug_dump_folds_the_region_into_ascii_between_quotes() {
    let mut el = screen();
    el.el_refresh.r_cursor = CoordT { h: 3, v: 1 };
    el.el_cursor = CoordT { h: 5, v: 2 };
    el.el_vdisplay[1] = row("live");
    let before = (el.el_vdisplay.clone(), el.el_display.clone());

    let out = emitted(&mut el, |el| {
        re_printstr(el, "new", &cells("live"));
        re_printstr(el, "fill", &[MB_FILL_CHAR, 0, 0x1b, u32::from('~')]);
        re_printstr(el, "", &[]);
    });

    assert_eq!(out, b"new:\"live\"\r\nfill:\"\x7f\0\x1b~\"\r\n:\"\"\r\n");
    assert_eq!((el.el_vdisplay.clone(), el.el_display.clone()), before);
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 3, v: 1 });
    assert_eq!(el.el_cursor, CoordT { h: 5, v: 2 });
}

/// With no error descriptor the dump goes nowhere at all — it does not fall
/// back to the output one, where it would land in the middle of the screen
/// image. An unset `el_errfd` is what an application that never asked for
/// diagnostics leaves behind, and the failed write is discarded exactly as the
/// C discards `fprintf`'s result.
// [spec:libedit:sem:refresh.re-printstr-fn/test]
#[test]
fn the_debug_dump_never_falls_back_to_the_output_descriptor() {
    let mut el = screen();
    let out = emitted(&mut el, |el| {
        el.el_errfd = -1;
        re_printstr(el, "new", &cells("live"));
    });
    assert_eq!(out, b"");
}

// ---------------------------------------------------------------------------
// re_putliteral
// ---------------------------------------------------------------------------

/// One magic cell stands for the whole bracketed sequence plus the visible
/// character glued to it, so the image charges the sequence no columns while
/// printing the cell replays every byte of it. The closing delimiter is not
/// part of either: the sequence and the visible character are what get
/// encoded, and the visible character alone decides the width.
// [spec:libedit:sem:refresh.re-putliteral-fn/test]
#[test]
fn a_literal_becomes_one_cell_that_replays_its_whole_byte_string() {
    let mut el = screen();
    re_putliteral(&mut el, &cells("\u{1b}[1m"), u32::from('X'));

    let c = el.el_vdisplay[0][0];
    assert_eq!(c, EL_LITERAL, "the first index, with the marker bit");
    assert_eq!(literal_get(&mut el, c), b"\x1b[1mX");
    assert_eq!(
        el.el_refresh.r_cursor,
        CoordT { h: 1, v: 0 },
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

    re_putliteral(&mut el, &cells("\u{1b}k"), 0);

    assert_eq!(el.el_vdisplay[0][0], EL_LITERAL);
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 1, v: 0 });
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
    re_putliteral(&mut el, &cells("\u{1b}["), 0xD800);
    assert_eq!(el.el_vdisplay, before);
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 2, v: 1 });
    assert_eq!(el.el_literal.l_idx, 0, "nothing was interned either");

    // The other refusal: the port bounds the table index where the C wraps
    // into the marker bit, and past that bound `literal_add` returns 0 with
    // the visible character's real width still in `w`.
    el.el_literal.l_idx = usize::MAX;
    re_putliteral(&mut el, &cells("\u{1b}["), u32::from('X'));
    assert_eq!(el.el_vdisplay, before);
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 2, v: 1 });
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

    re_putliteral(&mut el, &cells("\u{1b}["), u32::from('X'));

    assert_eq!(el.el_vdisplay[0][DLEN as usize - 1], EL_LITERAL);
    assert_eq!(
        el.el_vdisplay[0][DLEN as usize], 0,
        "the reserved terminator slot, not the sentinel it held"
    );
    assert_eq!(el.el_refresh.r_cursor, CoordT { h: 0, v: 1 });
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
    re_putliteral(&mut el, &cells("\u{1b}["), CJK);
    if wide {
        assert_eq!(el.el_vdisplay[0][0], EL_LITERAL);
        assert_eq!(el.el_vdisplay[0][1], MB_FILL_CHAR, "the second column");
        assert_eq!(el.el_refresh.r_cursor, CoordT { h: 2, v: 0 });
    } else {
        // C/POSIX: unprintable, so `literal_add` declines it outright.
        assert_eq!(el.el_vdisplay[0][0], 0);
        assert_eq!(el.el_refresh.r_cursor, CoordT { h: 0, v: 0 });
    }

    // At the last column there is only one column left for a two-column
    // character, and with no pre-padding loop nothing is pushed off it.
    let mut el = screen();
    el.el_vdisplay[0] = row("abcdefgh");
    el.el_refresh.r_cursor = CoordT { h: DLEN - 1, v: 0 };
    re_putliteral(&mut el, &cells("\u{1b}["), CJK);
    if wide {
        assert_eq!(el.el_vdisplay[0][DLEN as usize - 1], EL_LITERAL);
        assert_eq!(
            el.el_vdisplay[0][DLEN as usize], 0,
            "the terminator, not a fill cell: the clamp emptied the fill loop"
        );
        assert_eq!(el.el_refresh.r_cursor, CoordT { h: 0, v: 1 });
    } else {
        assert_eq!(el.el_vdisplay[0][DLEN as usize - 1], u32::from('h'));
        assert_eq!(el.el_refresh.r_cursor, CoordT { h: DLEN - 1, v: 0 });
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
        el.el_cursor,
        CoordT { h: 0, v: 0 },
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
    assert_eq!(el.el_cursor, CoordT { h: 0, v: 2 });
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
        el.el_cursor,
        CoordT { h: DLEN, v: 0 },
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
