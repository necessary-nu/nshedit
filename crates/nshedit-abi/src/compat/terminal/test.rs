use std::collections::HashMap;
use std::io::{Read, Seek};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::el::blank_editline;
use crate::fcns::ED_INSERT;

/// An editor with the capability tables allocated and nothing loaded into
/// them, so each test installs exactly the capabilities it is about.
///
/// `terminal_init` cannot stand in for this: it reaches `terminal_set`,
/// which reads `TERM` out of the environment and loads whatever database
/// the machine happens to have. None of these tests are about that, and a
/// test that changes its answer with the host's terminfo is not a test.
/// `el_outfd` starts at -1 — [`blank_editline`] leaves it 0, which is a
/// real descriptor — so an emit reaching output that was not captured
/// writes nowhere instead of into the test runner's stdin.
fn bare_terminal() -> EditLine {
    let mut el = blank_editline();
    el.el_terminal.t_str = vec![None; T_STR];
    el.el_terminal.t_val = vec![0; T_VAL];
    el.el_terminal.t_size = CoordT { h: 80, v: 24 };
    el.el_outfd = -1;
    el.el_errfd = -1;
    el
}

/// Everything `f` writes to the editor's output descriptor.
///
/// The C writes through a `FILE *` a test could replace with a memory
/// stream; this port writes straight to a descriptor, so the capture has
/// to be a real file. It is opened read-write and rewound rather than
/// reopened, because `write_fd` shares the offset with this handle.
fn emitted(el: &mut EditLine, f: impl FnOnce(&mut EditLine)) -> Vec<u8> {
    let (mut file, path) = scratch();
    el.el_outfd = file.as_raw_fd();
    f(el);
    el.el_outfd = -1;
    let mut bytes = Vec::new();
    file.rewind().unwrap();
    file.read_to_end(&mut bytes).unwrap();
    drop(file);
    let _ = std::fs::remove_file(path);
    bytes
}

fn scratch() -> (std::fs::File, std::path::PathBuf) {
    static N: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "nshedit-terminal-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    (file, path)
}

/// B9600, as `el_tty.t_speed` carries it: the encoded `B*` constant, not a
/// baud number. Padding is only visible at a known line speed.
const B9600: SpeedT = 13;

// [spec:libedit:sem:terminal.tputs-fn/test]
/// Padding is realised as transmitted characters, so what comes out of a
/// `$<...>` run is a count derived from the line speed — and the `*` and
/// `/` suffixes decide whether the count is scaled and whether it survives
/// a flow-controlled terminal at all.
#[test]
fn a_padding_run_becomes_as_many_characters_as_the_line_takes_to_send() {
    let pad = |cap: &[u8], affcnt: i32, entry: Option<&TermInfo>| {
        let mut out = Vec::new();
        assert_eq!(tputs(&mut out, cap, affcnt, entry, B9600), 0);
        out
    };

    // Ten bits per character: 5 ms at 9600 baud is 4.8 characters, and the
    // count truncates. The pad character is terminfo `pad_char`, NUL when
    // the entry does not supply one — which is why this is not the empty
    // string.
    assert_eq!(pad(b"CE$<5>", 1, None), b"CE\0\0\0\0");
    // A tenth of a millisecond is the resolution, and 12.5 ms is exactly
    // twelve characters.
    assert_eq!(pad(b"$<12.5>", 1, None), [0u8; 12]);
    // `*` multiplies by the caller's affected-line count.
    assert_eq!(pad(b"$<5*>", 3, None), [0u8; 14]);
    // ...and takes it as given, so a caller passing 0 — which
    // `terminal_echotc`'s two-argument form can — zeroes the delay.
    assert_eq!(pad(b"$<5*>", 0, None), b"");
    // Both suffixes, in either order.
    assert_eq!(pad(b"$<5*/>", 2, None), [0u8; 9]);
    assert_eq!(pad(b"$<5/*>", 2, None), [0u8; 9]);
}

// [spec:libedit:sem:terminal.tputs-fn/test]
/// The two halves of the grammar that are not about arithmetic: a
/// flow-controlled terminal throttles itself, so an *advisory* delay is
/// dropped and only a mandatory `/` one survives; and anything that is not
/// a well-formed run is text.
#[test]
fn an_advisory_delay_is_skipped_on_a_terminal_that_throttles_itself() {
    let xon = TermInfo {
        names: vec!["test".to_string()],
        bools: HashMap::from([("xon", true)]),
        numbers: HashMap::new(),
        // The pad character comes from the entry too, so this one pads
        // with a visible byte rather than with NUL.
        strings: HashMap::from([("pad", b".".to_vec())]),
    };
    let pad = |cap: &[u8]| {
        let mut out = Vec::new();
        tputs(&mut out, cap, 1, Some(&xon), B9600);
        out
    };
    assert_eq!(pad(b"X$<5>"), b"X", "advisory, and the terminal throttles");
    assert_eq!(pad(b"X$<5/>"), b"X....", "mandatory, so it is emitted");

    // Everything outside a run is verbatim, and a run that is not a delay
    // is not a run: a `$` that opens nothing, an unterminated `$<`, and a
    // body that is not a number all come out as they went in.
    assert_eq!(pad(b"a$b"), b"a$b");
    assert_eq!(pad(b"a$<5"), b"a$<5");
    assert_eq!(pad(b"a$<oops>b"), b"a$<oops>b");
    // A zero or unrecognised line speed emits no padding at all, which is
    // also what ncurses does with an unset `ospeed`.
    let mut out = Vec::new();
    tputs(&mut out, b"X$<50>", 1, None, 0);
    assert_eq!(out, b"X");
}

// [spec:libedit:sem:terminal.tgoto-fn/test]
/// The C's `tgoto` takes (column, row) and substitutes the row first;
/// terminfo's `cursor_address` is `%p1` = row, `%p2` = column. The port
/// resolves that by exposing terminfo's convention — ERR-terminal-63 — so
/// the *second* argument is what `%p1` sees. Only `terminal_echotc`'s
/// two-argument form can tell, because every internal caller passes one
/// value twice.
#[test]
fn the_two_parameters_go_in_terminfo_order_not_tgotos() {
    // xterm's `cup`, with the `%i` that makes the coordinates 1-based.
    let cup = b"\x1b[%i%p1%d;%p2%dH";
    assert_eq!(tgoto(cup, 5, 10), b"\x1b[11;6H");
    // The one-parameter capabilities every internal caller uses consume
    // `%p1` only, which is why they pass the same value twice.
    assert_eq!(tgoto(b"\x1b[%p1%dP", 3, 3), b"\x1b[3P");
}

// [spec:libedit:sem:terminal.tgoto-fn/test]
/// Expansion must not eat the padding: `tputs` is where a `$<...>` run
/// becomes real delay, and it only ever sees what came out of here. A
/// capability that is not expandable at all is passed through rather than
/// dropped, which is what the C does with anything it does not understand.
#[test]
fn expansion_preserves_the_padding_runs_it_walks_past() {
    assert_eq!(tgoto(b"\x1b[%p1%dP$<5*>", 4, 4), b"\x1b[4P$<5*>");
    // A run at the front, and one in the middle of two expandable pieces.
    assert_eq!(tgoto(b"$<2>%p1%d$<3>%p2%d", 7, 9), b"$<2>9$<3>7");
    // A `$` that opens nothing stays a `$`.
    assert_eq!(tgoto(b"a$b", 1, 2), b"a$b");
}

// [spec:libedit:sem:terminal.terminal-alloc-fn/test]
/// A capability slot holds its own string and nothing else. The C interns
/// into a 2048-byte pool whose append bound ignores the string's length
/// (ERR-terminal-01) and whose compaction rewrites the buffer without
/// repointing a single slot (ERR-terminal-02); the rule directs the port
/// not to reproduce the pool, so the interesting property is that far more
/// than 2048 bytes of capability survive intact.
#[test]
fn every_capability_slot_keeps_its_own_string_past_the_cs_pool_size() {
    let mut el = bare_terminal();
    let value = |t: usize| format!("{}-{t}", "x".repeat(100));
    for t in 0..T_STR {
        terminal_alloc(&mut el, t, Some(&value(t)));
    }
    // The premise: what was just stored does not fit in the C's arena.
    const { assert!(T_STR * 100 > TC_BUFSIZE) };
    for t in 0..T_STR {
        assert_eq!(
            cap_str(&el, t),
            Some(value(t).as_bytes()),
            "slot {t} was corrupted by a neighbour"
        );
    }
}

// [spec:libedit:sem:terminal.terminal-alloc-fn/test]
/// Step 1 of the rule, which is the whole of what "capability absent"
/// means downstream: NULL and the empty string both clear the slot, and
/// `GoodStr` then reads false. A capability is also a C string, so an
/// embedded NUL ends it.
#[test]
fn an_empty_capability_clears_the_slot_rather_than_storing_nothing() {
    let mut el = bare_terminal();
    terminal_alloc(&mut el, T_BL, Some("\x07"));
    assert!(good_str(&el, T_BL));

    terminal_alloc(&mut el, T_BL, Some(""));
    assert_eq!(cap_str(&el, T_BL), None);
    assert!(!good_str(&el, T_BL));

    terminal_alloc(&mut el, T_BL, Some("\x07"));
    terminal_alloc(&mut el, T_BL, None);
    assert_eq!(cap_str(&el, T_BL), None);

    // The C reaches for `strlen`, so the stored value stops at the NUL.
    terminal_alloc(&mut el, T_BL, Some("ab\0cd"));
    assert_eq!(cap_str(&el, T_BL), Some(&b"ab"[..]));
    // A shorter value replaces a longer one outright. The C would have
    // copied it over the old bytes in place and left the slack behind.
    terminal_alloc(&mut el, T_BL, Some("z"));
    assert_eq!(cap_str(&el, T_BL), Some(&b"z"[..]));
}

// [spec:libedit:sem:terminal.terminal-deletechars-fn/test]
/// Three guards, in the order the rule fixes, and none of them emits
/// anything. The `num > t_size.h` one matters because it is a bound on a
/// count the caller derived from the screen, not on the capability.
#[test]
fn deleting_declines_a_nonsensical_count_without_emitting() {
    let mut el = bare_terminal();
    terminal_alloc(&mut el, T_DC1, Some("dc"));
    el.el_terminal.t_flags = TERM_CAN_DELETE;

    assert_eq!(emitted(&mut el, |el| terminal_deletechars(el, 0)), b"");
    assert_eq!(emitted(&mut el, |el| terminal_deletechars(el, -3)), b"");
    assert_eq!(emitted(&mut el, |el| terminal_deletechars(el, 81)), b"");

    // The flag is the gate, not the presence of the capability: with
    // TERM_CAN_DELETE clear the same call writes nothing.
    el.el_terminal.t_flags = 0;
    assert_eq!(emitted(&mut el, |el| terminal_deletechars(el, 2)), b"");
}

// [spec:libedit:sem:terminal.terminal-deletechars-fn/test]
/// The cost heuristic: the parameterised form wins for a run, the
/// one-character form wins for a single deletion, and when only the
/// one-character form exists it is wrapped in delete mode.
#[test]
fn deleting_picks_the_parameterised_form_only_when_it_is_cheaper() {
    let mut el = bare_terminal();
    el.el_terminal.t_flags = TERM_CAN_DELETE;
    terminal_alloc(&mut el, T_DC, Some("\x1b[%p1%dP"));
    terminal_alloc(&mut el, T_DC1, Some("\x1b[P"));

    assert_eq!(
        emitted(&mut el, |el| terminal_deletechars(el, 3)),
        b"\x1b[3P"
    );
    // One deletion, and the one-character form exists: it is assumed
    // cheaper, so the parameterised capability is not used.
    assert_eq!(
        emitted(&mut el, |el| terminal_deletechars(el, 1)),
        b"\x1b[P"
    );
    // With no one-character form the parameterised one is used even for a
    // single deletion.
    terminal_alloc(&mut el, T_DC1, None);
    assert_eq!(
        emitted(&mut el, |el| terminal_deletechars(el, 1)),
        b"\x1b[1P"
    );

    // Delete mode brackets the run, and the one-character capability is
    // repeated `num` times inside it.
    let mut el = bare_terminal();
    el.el_terminal.t_flags = TERM_CAN_DELETE;
    terminal_alloc(&mut el, T_DM, Some("<dm>"));
    terminal_alloc(&mut el, T_DC1, Some("d"));
    terminal_alloc(&mut el, T_ED, Some("<ed>"));
    assert_eq!(
        emitted(&mut el, |el| terminal_deletechars(el, 3)),
        b"<dm>ddd<ed>"
    );
    // Nothing here touches the screen model; the caller owns it.
    assert_eq!(el.el_cursor, CoordT { h: 0, v: 0 });
}

// [spec:libedit:sem:terminal.terminal-insertwrite-fn/test]
/// Strategy A opens the hole with the parameterised capability and then
/// hands the characters to `terminal_overwrite`, which is what advances
/// the recorded column.
#[test]
fn inserting_a_run_opens_it_with_one_capability_then_overwrites() {
    let mut el = bare_terminal();
    el.el_terminal.t_flags = TERM_CAN_INSERT;
    terminal_alloc(&mut el, T_IC, Some("\x1b[%p1%d@"));
    let cp = [u32::from(b'a'), u32::from(b'b')];

    assert_eq!(
        emitted(&mut el, |el| terminal_insertwrite(el, &cp, 2)),
        b"\x1b[2@ab"
    );
    assert_eq!(el.el_cursor.h, 2);
}

// [spec:libedit:sem:terminal.terminal-insertwrite-fn/test]
/// Strategy B counts the columns up front and then writes them, which is
/// why it applies no margin rule at all: the recorded column is allowed
/// straight past the screen width (ERR-terminal-27, reproduced). The
/// insert padding is emitted once for the whole run here, and once per
/// character in strategy C — the same capability, two different meanings.
#[test]
fn insert_mode_pads_once_and_lets_the_column_run_off_the_screen() {
    let mut el = bare_terminal();
    el.el_terminal.t_flags = TERM_CAN_INSERT;
    el.el_terminal.t_size.h = 3;
    terminal_alloc(&mut el, T_IM, Some("<im>"));
    terminal_alloc(&mut el, T_EI, Some("<ei>"));
    terminal_alloc(&mut el, T_IP, Some("<ip>"));
    let cp = [u32::from(b'a'), u32::from(b'b'), u32::from(b'c')];

    el.el_cursor.h = 2;
    assert_eq!(
        emitted(&mut el, |el| terminal_insertwrite(el, &cp, 3)),
        b"<im>abc<ip><ei>"
    );
    assert_eq!(
        el.el_cursor.h, 5,
        "no wrap handling: the column runs past t_size.h"
    );

    // Strategy C, reached because insert mode is incomplete. The padding
    // now follows every character.
    let mut el = bare_terminal();
    el.el_terminal.t_flags = TERM_CAN_INSERT;
    terminal_alloc(&mut el, T_IC1, Some("<ic>"));
    terminal_alloc(&mut el, T_IP, Some("<ip>"));
    assert_eq!(
        emitted(&mut el, |el| terminal_insertwrite(el, &cp, 2)),
        b"<ic>a<ip><ic>b<ip>"
    );
    assert_eq!(el.el_cursor.h, 2);
}

// [spec:libedit:sem:terminal.terminal-insertwrite-fn/test]
/// ERR-terminal-28, reproduced: `TERM_CAN_INSERT` can be set by
/// enter-insert-mode alone, and with no matching exit-insert-mode and no
/// one-character insert, strategy C emits no insert capability whatsoever.
/// The characters overwrite what was there instead of pushing it right —
/// a silently wrong screen, not a diagnostic.
#[test]
fn an_incomplete_insert_mode_degenerates_into_a_plain_overwrite() {
    let mut el = bare_terminal();
    // As `terminal_setflags` would set it from enter-insert-mode alone.
    el.el_terminal.t_flags = TERM_CAN_INSERT;
    terminal_alloc(&mut el, T_IM, Some("<im>"));
    let cp = [u32::from(b'a'), u32::from(b'b')];

    assert_eq!(
        emitted(&mut el, |el| terminal_insertwrite(el, &cp, 2)),
        b"ab",
        "no insert capability reached the terminal"
    );
    assert_eq!(el.el_cursor.h, 2);
}

// [spec:libedit:sem:terminal.terminal-clear-screen-fn/test]
/// The affected-line count is the recorded *line* count, not 1, and it is
/// observable: a per-affected-line padding delay on the clear capability
/// is computed for a whole screen's worth of work.
#[test]
fn clearing_the_screen_pads_for_every_line_it_claims_to_affect() {
    let mut el = bare_terminal();
    el.el_tty.t_speed = B9600;
    terminal_alloc(&mut el, T_CL, Some("CLS$<5*>"));

    set_val(&mut el, T_LI, 3);
    assert_eq!(
        emitted(&mut el, terminal_clear_screen),
        [b"CLS".as_slice(), &[0u8; 14]].concat()
    );
    set_val(&mut el, T_LI, 1);
    assert_eq!(
        emitted(&mut el, terminal_clear_screen),
        [b"CLS".as_slice(), &[0u8; 4]].concat()
    );
}

// [spec:libedit:sem:terminal.terminal-clear-screen-fn/test]
/// The three strategies in order. The last is not a capability at all: on
/// a terminal that cannot clear, the best available is to scroll one line,
/// and the recorded cursor is left wrong on every path for the caller to
/// resynchronise.
#[test]
fn clearing_falls_back_to_home_and_clear_to_bottom_then_to_a_newline() {
    let mut el = bare_terminal();
    terminal_alloc(&mut el, T_HO, Some("<ho>"));
    terminal_alloc(&mut el, T_CD, Some("<cd>"));
    assert_eq!(emitted(&mut el, terminal_clear_screen), b"<ho><cd>");

    // Home alone is not enough — both must be present.
    terminal_alloc(&mut el, T_CD, None);
    assert_eq!(emitted(&mut el, terminal_clear_screen), b"\r\n");

    let mut el = bare_terminal();
    el.el_cursor = CoordT { h: 7, v: 2 };
    terminal_alloc(&mut el, T_CL, Some("<cl>"));
    assert_eq!(emitted(&mut el, terminal_clear_screen), b"<cl>");
    assert_eq!(
        el.el_cursor,
        CoordT { h: 7, v: 2 },
        "the cursor model is the caller's to fix"
    );
}

// [spec:libedit:sem:terminal.terminal-beep-fn/test]
/// The capability if there is one, a literal BEL if there is not — and an
/// empty capability counts as absent, which is what makes the fallback
/// reachable on a terminal whose entry defines `bel` as nothing.
#[test]
fn the_bell_falls_back_to_a_literal_bel_byte() {
    let mut el = bare_terminal();
    assert_eq!(emitted(&mut el, terminal_beep), b"\x07");

    terminal_alloc(&mut el, T_BL, Some("<bl>"));
    assert_eq!(emitted(&mut el, terminal_beep), b"<bl>");

    terminal_alloc(&mut el, T_BL, Some(""));
    assert_eq!(emitted(&mut el, terminal_beep), b"\x07");
    assert_eq!(el.el_cursor, CoordT { h: 0, v: 0 });
}

/// The function-key table as `terminal_init` leaves it, defaults filled
/// in. The names are what `set`/`clear` match on and they only exist once
/// `terminal_init_arrow` has run.
fn with_fkeys() -> EditLine {
    let mut el = bare_terminal();
    el.el_terminal.t_fkey = (0..A_K_NKEYS)
        .map(|_| FunckeyT {
            name: None,
            key: 0,
            fun: KeymacroValueT::Cmd(0),
            r#type: XK_CMD,
        })
        .collect();
    terminal_init_arrow(&mut el);
    el
}

fn bound_cmd(el: &EditLine, slot: usize) -> ElActionT {
    match &el.el_terminal.t_fkey[slot].fun {
        KeymacroValueT::Cmd(c) => *c,
        KeymacroValueT::Str(_) => panic!("slot {slot} holds a string, not a command"),
    }
}

// [spec:libedit:sem:terminal.terminal-set-arrow-fn/test]
/// Rebinding a named key changes the table and nothing else — the key map
/// is not touched until `terminal_bind_arrow` next runs, which is what
/// re-derives both the hard-coded sequences and the terminal's own key
/// capabilities from it.
#[test]
fn setting_an_arrow_rewrites_one_table_row_and_leaves_the_map_alone() {
    let mut el = with_fkeys();
    assert!(el.el_map.key.is_empty());

    assert_eq!(
        terminal_set_arrow(&mut el, A_NAME_UP, KeymacroValueT::Cmd(ED_INSERT), XK_CMD),
        0
    );
    assert_eq!(bound_cmd(&el, A_K_UP), ED_INSERT);
    assert_eq!(el.el_terminal.t_fkey[A_K_UP].r#type, XK_CMD);
    // Its neighbours keep the defaults `terminal_init_arrow` installed.
    assert_eq!(bound_cmd(&el, A_K_DN), ED_NEXT_HISTORY);
    assert!(el.el_map.key.is_empty(), "nothing reached the key map");

    // An unrecognised name changes nothing and says so.
    let nosuch: Vec<u32> = "pgup".chars().map(u32::from).collect();
    assert_eq!(
        terminal_set_arrow(&mut el, &nosuch, KeymacroValueT::Cmd(ED_INSERT), XK_CMD),
        -1
    );
    assert_eq!(bound_cmd(&el, A_K_UP), ED_INSERT);
}

// [spec:libedit:sem:terminal.terminal-set-arrow-fn/test]
/// The table `terminal_init` allocates has NULL names until
/// `terminal_init_arrow` fills them; the C compares them with `wcscmp` and
/// would dereference NULL. Defined here as "never matches", so every name
/// is simply unknown.
#[test]
fn an_unfilled_arrow_table_matches_no_name_at_all() {
    let mut el = bare_terminal();
    el.el_terminal.t_fkey = (0..A_K_NKEYS)
        .map(|_| FunckeyT {
            name: None,
            key: 0,
            fun: KeymacroValueT::Cmd(0),
            r#type: XK_CMD,
        })
        .collect();
    assert_eq!(
        terminal_set_arrow(&mut el, A_NAME_UP, KeymacroValueT::Cmd(ED_INSERT), XK_CMD),
        -1
    );
    assert_eq!(terminal_clear_arrow(&mut el, A_NAME_UP), -1);
}

// [spec:libedit:sem:terminal.terminal-clear-arrow-fn/test]
/// Clearing sets the type to `XK_NOD` and deliberately leaves the bound
/// function where it was. That matters because `terminal_reset_arrow`
/// binds every sequence to the slot's *current* value and type, so the
/// stale function is what an `XK_NOD` entry carries into `keymacro_add`;
/// only the type tells `terminal_bind_arrow` to clear the key instead.
#[test]
fn clearing_an_arrow_marks_the_row_without_forgetting_its_function() {
    let mut el = with_fkeys();
    assert_eq!(bound_cmd(&el, A_K_DE), ED_DELETE_NEXT_CHAR);

    assert_eq!(terminal_clear_arrow(&mut el, A_NAME_DE), 0);
    assert_eq!(el.el_terminal.t_fkey[A_K_DE].r#type, XK_NOD);
    assert_eq!(
        bound_cmd(&el, A_K_DE),
        ED_DELETE_NEXT_CHAR,
        "the function value is untouched"
    );
    // Every other row keeps its type.
    assert_eq!(el.el_terminal.t_fkey[A_K_HO].r#type, XK_CMD);

    let nosuch: Vec<u32> = "insert".chars().map(u32::from).collect();
    assert_eq!(terminal_clear_arrow(&mut el, &nosuch), -1);
}

// [spec:libedit:sem:terminal.terminal-tputs-fn/test]
/// ERR-terminal-34, fixed: the C parks the destination `FILE *` in a
/// file-static and serialises the whole emit behind one file-static mutex,
/// because its `tputs` callback takes no user data. Two `EditLine`s on
/// different streams therefore could not emit concurrently. Here the
/// destination, the pad source and the line speed all come off the
/// instance, so interleaved emits stay separate — and the same capability
/// pads differently for each.
#[test]
fn two_editors_emit_to_their_own_streams_with_their_own_padding() {
    let (mut fa, pa) = scratch();
    let (mut fb, pb) = scratch();
    let mut a = bare_terminal();
    let mut b = bare_terminal();
    a.el_outfd = fa.as_raw_fd();
    b.el_outfd = fb.as_raw_fd();
    a.el_tty.t_speed = B9600;
    // Line speed unknown, so `b` emits no padding for the same string.
    b.el_tty.t_speed = 0;

    terminal_tputs(&mut a, "A$<5>", 1);
    terminal_tputs(&mut b, "B$<5>", 1);
    terminal_tputs(&mut a, "A", 1);

    let read = |f: &mut std::fs::File| {
        let mut v = Vec::new();
        f.rewind().unwrap();
        f.read_to_end(&mut v).unwrap();
        v
    };
    assert_eq!(read(&mut fa), b"A\0\0\0\0A");
    assert_eq!(read(&mut fb), b"B");

    // The C's one-byte callback returns -1 when the destination `FILE *`
    // is NULL and `tputs`'s result is discarded, so a missing stream is
    // silent. A descriptor below zero stands in for that NULL, and the
    // emit has to reach nothing at all — not the descriptor this editor
    // held a moment ago, and not its neighbour's.
    a.el_outfd = -1;
    terminal_tputs(&mut a, "A", 1);
    assert_eq!(read(&mut fa), b"A\0\0\0\0A");
    assert_eq!(read(&mut fb), b"B");

    drop((fa, fb));
    let _ = std::fs::remove_file(pa);
    let _ = std::fs::remove_file(pb);
}
