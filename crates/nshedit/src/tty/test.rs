use std::io::{Read, Seek};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::el::{CoordT, blank_editline};

/// An editor with the mode tables loaded, as `tty_init` leaves them, and
/// **no terminal on any descriptor**.
///
/// `blank_editline` leaves `el_infd` at 0, which under `cargo test` is
/// quite likely the developer's own terminal: a test that reached
/// `tcsetattr` would then reconfigure it for real. Every descriptor is
/// pushed below zero so the two syscall wrappers fail the way they do on a
/// pipe, which is also what makes "returned 0" proof that a guard fired
/// before the syscall.
fn tty_el() -> EditLine {
    let mut el = blank_editline();
    el.el_tty.t_t = ttyperm();
    el.el_tty.t_c = TTYCHAR;
    el.el_tty.t_vdisable = plat::VDISABLE;
    el.el_terminal.t_size = CoordT { h: 1000, v: 24 };
    el.el_infd = -1;
    el.el_outfd = -1;
    el.el_errfd = -1;
    el
}

fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

/// Everything `f` writes to one of the editor's descriptors.
///
/// The C writes through a `FILE *`; this port writes straight to a
/// descriptor, so the capture is a real file, opened read-write and
/// rewound rather than reopened because `write_fd` shares its offset.
fn captured(
    el: &mut EditLine,
    pick: fn(&mut EditLine) -> &mut i32,
    f: impl FnOnce(&mut EditLine),
) -> Vec<u8> {
    static N: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "nshedit-tty-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    *pick(el) = file.as_raw_fd();
    f(el);
    *pick(el) = -1;
    let mut bytes = Vec::new();
    file.rewind().unwrap();
    file.read_to_end(&mut bytes).unwrap();
    drop(file);
    let _ = std::fs::remove_file(path);
    bytes
}

fn on_out(el: &mut EditLine, f: impl FnOnce(&mut EditLine)) -> Vec<u8> {
    captured(el, |el| &mut el.el_outfd, f)
}

fn on_err(el: &mut EditLine, f: impl FnOnce(&mut EditLine)) -> Vec<u8> {
    captured(el, |el| &mut el.el_errfd, f)
}

// [spec:libedit:sem:tty.tty-getcharindex-fn/test]
/// libedit's `C_*` indices and the termios `V*` subscripts are two
/// different numberings, and this is the only thing that converts between
/// them. That they coincide for the first two is an accident of glibc's
/// header — it is the reason ERR-terminal-37 goes unnoticed — so the
/// interesting pairs are the ones that do not.
#[test]
fn the_control_character_index_is_not_the_termios_subscript() {
    assert_eq!(tty__getcharindex(C_INTR as i32), plat::VINTR as i32);
    assert_eq!(tty__getcharindex(C_QUIT as i32), plat::VQUIT as i32);
    // Where the two numberings part company.
    assert_eq!(tty__getcharindex(C_SUSP as i32), plat::VSUSP as i32);
    assert_ne!(tty__getcharindex(C_SUSP as i32), C_SUSP as i32);
    assert_eq!(tty__getcharindex(C_MIN as i32), plat::VMIN as i32);
    assert_eq!(tty__getcharindex(C_TIME as i32), plat::VTIME as i32);
    assert_eq!(tty__getcharindex(C_EOF as i32), plat::VEOF as i32);
    assert_eq!(tty__getcharindex(C_START as i32), plat::VSTART as i32);
}

// [spec:libedit:sem:tty.tty-getcharindex-fn/test]
/// The -1 arms, which are what callers must test for. `C_BRK` is the one
/// that matters: the C has no case for it on *any* platform, even one
/// defining `VBRK`, and `tty_stty`'s `brk=` form then writes
/// `c_cc[-1]` once the guarding assert is compiled out (ERR-terminal-05).
#[test]
fn an_index_with_no_subscript_answers_minus_one_including_brk() {
    assert_eq!(tty__getcharindex(C_BRK as i32), -1);
    // Absent on this platform, so also -1 here.
    for c in [C_SWTCH, C_DSWTCH, C_ERASE2, C_DSUSP, C_STATUS, C_PAGE] {
        assert_eq!(tty__getcharindex(c as i32), -1, "C_* index {c}");
    }
    // `C_NCC` is a count, not an index.
    assert_eq!(tty__getcharindex(C_NCC as i32), -1);
    assert_eq!(tty__getcharindex(-1), -1);
    assert_eq!(tty__getcharindex(i32::MAX), -1);
}

// [spec:libedit:sem:tty.tty-quotemode-fn/test]
/// Quote mode copies the *edit* termios rather than the terminal's current
/// state, applies the `QU_IO` masks to it, and writes it out — and its
/// destination, `t_qu`, is `#define`d to `t_ts`, so the copy destroys the
/// terminal-snapshot scratch. Both are reproduced. The write fails here
/// because there is no terminal, which is what makes the intermediate
/// state visible: the masks have already been applied to `t_ts` and the
/// recorded mode has not moved.
#[test]
fn quoting_masks_a_copy_of_the_edit_termios_over_the_snapshot_scratch() {
    let mut el = tty_el();
    el.el_tty.t_mode = EX_IO as u8;
    // Everything quote mode is meant to clear, plus one bit it must not.
    el.el_tty.t_ed.c_iflag = plat::IXON | plat::IXOFF | plat::INLCR | plat::ICRNL | plat::ISTRIP;
    el.el_tty.t_ed.c_lflag = plat::ISIG | plat::IEXTEN | plat::ECHO;
    el.el_tty.t_ed.c_oflag = plat::OPOST;
    el.el_tty.t_ed.c_cc[plat::VMIN] = 1;
    // A sentinel the snapshot scratch is about to lose.
    el.el_tty.t_ts.c_iflag = plat::IGNBRK;

    assert_eq!(tty_quotemode(&mut el), -1, "no terminal to write to");
    assert_eq!(
        el.el_tty.t_mode, EX_IO as u8,
        "the recorded mode does not move on failure"
    );

    let qu = &el.el_tty.t_ts;
    assert_eq!(qu.c_iflag, plat::ISTRIP, "flow control and translation off");
    assert_eq!(
        qu.c_lflag,
        plat::ECHO,
        "signals and extended processing off"
    );
    assert_eq!(qu.c_oflag, plat::OPOST, "the output word is not masked");
    assert_eq!(
        qu.c_cc[plat::VMIN],
        1,
        "the control characters stay as edit mode left them"
    );
}

// [spec:libedit:sem:tty.tty-quotemode-fn/test]
/// The mode test is the only guard — there is no `EDIT_DISABLED` check
/// here, unlike `tty_rawmode` and `tty_cookedmode`. Returning 0 with no
/// terminal on the input descriptor is what proves it returned before the
/// syscall rather than through it.
#[test]
fn quoting_twice_is_a_no_op_even_with_editing_disabled() {
    let mut el = tty_el();
    el.el_tty.t_mode = QU_IO as u8;
    el.el_tty.t_ts.c_iflag = plat::IGNBRK;
    el.el_tty.t_ed.c_iflag = plat::IXON;

    assert_eq!(tty_quotemode(&mut el), 0);
    assert_eq!(el.el_tty.t_ts.c_iflag, plat::IGNBRK, "nothing was copied");

    // Not a guard: with the mode anywhere else the copy happens whether or
    // not editing is disabled.
    el.el_flags |= EDIT_DISABLED;
    el.el_tty.t_mode = ED_IO as u8;
    assert_eq!(tty_quotemode(&mut el), -1);
    assert_eq!(el.el_tty.t_ts.c_iflag, 0, "t_ed's IXON was masked away");
}

// [spec:libedit:sem:tty.tty-noquotemode-fn/test]
/// Leaving quote mode is guarded only by the recorded mode, and on a
/// failed write it stays `QU_IO` — the terminal keeps the quote-mode flags
/// and libedit knows it. The snapshot scratch that `tty_quotemode`
/// clobbered is deliberately not repaired: `tty_rawmode` re-reads it from
/// the terminal before use.
#[test]
fn leaving_quote_mode_keeps_the_mode_when_the_terminal_write_fails() {
    let mut el = tty_el();
    el.el_tty.t_mode = QU_IO as u8;
    el.el_tty.t_ts.c_iflag = plat::IGNBRK;

    assert_eq!(tty_noquotemode(&mut el), -1);
    assert_eq!(el.el_tty.t_mode, QU_IO as u8);
    assert_eq!(el.el_tty.t_ts.c_iflag, plat::IGNBRK, "not repaired");
}

// [spec:libedit:sem:tty.tty-noquotemode-fn/test]
/// From any other mode it is a no-op that issues no syscall at all — which
/// is why it answers 0 here, where a `tcsetattr` would have failed. Note
/// the asymmetry the pair leaves behind: quote mode can be entered from
/// `EX_IO`, but leaving it always lands in `ED_IO`.
#[test]
fn leaving_quote_mode_from_any_other_mode_touches_nothing() {
    let mut el = tty_el();
    for mode in [EX_IO, ED_IO] {
        el.el_tty.t_mode = mode as u8;
        assert_eq!(tty_noquotemode(&mut el), 0);
        assert_eq!(el.el_tty.t_mode, mode as u8);
    }
}

// [spec:libedit:sem:tty.tty-stty-fn/test]
/// The display form walks `ttymodes[]` in table order, starting a new
/// labelled group each time `m_type` changes — which is why the table's
/// grouping is load-bearing — and prints only the modes the selected row
/// has an opinion about. Clear wins over set in the sign.
#[test]
fn setty_with_no_arguments_prints_the_masks_for_one_mode() {
    let mut el = tty_el();
    let out = on_out(&mut el, |el| {
        assert_eq!(tty_stty(el, 0, &[&wide("setty")]), 0)
    });
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "iflag:-inlcr -igncr +icrnl \n\
         oflag:+opost +onlcr -onlret \n\
         cflag:\n\
         lflag:+isig +icanon +echo +echoe -echonl -noflsh +echoctl -flusho +iexten -extproc \n\
         chars:\n"
    );

    // `-d` selects the edit row instead, where the input flags differ.
    let out = on_out(&mut el, |el| {
        assert_eq!(tty_stty(el, 0, &[&wide("setty"), &wide("-d")]), 0);
    });
    let out = String::from_utf8(out).unwrap();
    assert!(out.starts_with("iflag:+inlcr -igncr +icrnl \n"), "{out}");
}

// [spec:libedit:sem:tty.tty-stty-fn/test]
/// `-a` shows every name in the table rather than only the signed ones, so
/// the empty `cflag:` group above fills in.
#[test]
fn setty_dash_a_shows_the_modes_with_no_opinion_too() {
    let mut el = tty_el();
    let out = on_out(&mut el, |el| {
        assert_eq!(tty_stty(el, 0, &[&wide("setty"), &wide("-a")]), 0);
    });
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("cflag:cbaud cstopb cread "), "{out}");
    // Unsigned entries print bare; the signs still appear where they apply.
    assert!(out.contains("ignbrk brkint "), "{out}");
    assert!(out.contains("-inlcr "), "{out}");
    assert!(out.contains("chars:intr quit erase "), "{out}");
}

// [spec:libedit:sem:tty.tty-stty-fn/test]
/// The wrap: an entry that would reach the terminal width starts a new
/// line indented to the width of the group label.
///
/// The test is `len + cu >= width`, not `>`, and `len` already counts the
/// label — so at a width of 20 a six-character label plus two eight-column
/// entries does not fit, and the second entry wraps at column 14. The
/// wrapped line then starts at `st + cu` rather than at `st`, which is why
/// the third entry wraps again where a fresh line would have held it.
#[test]
fn setty_wraps_its_output_at_the_terminal_width() {
    let mut el = tty_el();
    el.el_terminal.t_size.h = 20;
    let out = on_out(&mut el, |el| {
        assert_eq!(tty_stty(el, 0, &[&wide("setty")]), 0);
    });
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "iflag:-inlcr \n      -igncr \n      +icrnl \n\
         oflag:+opost \n      +onlcr \n      -onlret \n\
         cflag:\n\
         lflag:+isig \n      +icanon \n      +echo +echoe \n      -echonl \n      \
         -noflsh \n      +echoctl \n      -flusho \n      +iexten \n      -extproc \n\
         chars:\n"
    );
}

// [spec:libedit:sem:tty.tty-stty-fn/test]
/// The edit form changes the mask table for one I/O mode and re-derives
/// that mode's termios from it. Nothing is pushed to the terminal unless
/// the edited mode is the one currently installed, so editing `-d` from
/// execute mode is silent — which is what makes an `.editrc` `setty` work
/// at all, since none of the modes are live while it is read.
#[test]
fn setty_edits_the_mask_table_for_the_mode_it_was_given() {
    let mut el = tty_el();
    el.el_tty.t_mode = EX_IO as u8;
    assert_eq!(
        tty_stty(&mut el, 0, &[&wide("setty"), &wide("-d"), &wide("+ixon")]),
        0
    );

    let e = &el.el_tty.t_t[ED_IO][MD_INP];
    assert_eq!(e.t_setmask & plat::IXON, plat::IXON);
    assert_eq!(e.t_clrmask & plat::IXON, 0);
    // Step 7 re-derives the mode's flag word from the just-edited masks.
    assert_eq!(el.el_tty.t_ed.c_iflag & plat::IXON, plat::IXON);
    assert_eq!(el.el_tty.t_ed.c_iflag & plat::IGNCR, 0, "still cleared");
    // The execute row is untouched.
    assert_eq!(el.el_tty.t_t[EX_IO][MD_INP].t_setmask & plat::IXON, 0);

    // A bare name clears both masks: the bit is inherited from whatever
    // the terminal has rather than forced either way.
    assert_eq!(
        tty_stty(&mut el, 0, &[&wide("setty"), &wide("-d"), &wide("icrnl")]),
        0
    );
    let e = &el.el_tty.t_t[ED_IO][MD_INP];
    assert_eq!((e.t_setmask | e.t_clrmask) & plat::ICRNL, 0);
}

// [spec:libedit:sem:tty.tty-stty-fn/test]
/// The `name=value` form writes the control character into the selected
/// `struct termios` and **not** into `el_tty.t_c`, so the next
/// `tty_rawmode` that notices a control-character change reverts it — this
/// is why `setty erase=^H` in an `.editrc` does not stick (ERR-terminal-38,
/// reproduced). The same entry reproduces the name match's prefix
/// behaviour: `er=` finds `erase`.
#[test]
fn setty_assigns_a_control_character_to_the_termios_but_not_the_table() {
    let mut el = tty_el();
    let before = el.el_tty.t_c[ED_IO][C_ERASE];
    assert_eq!(
        tty_stty(&mut el, 0, &[&wide("setty"), &wide("-d"), &wide("er=^H")]),
        0
    );
    assert_eq!(el.el_tty.t_ed.c_cc[plat::VERASE], 8);
    assert_eq!(
        el.el_tty.t_c[ED_IO][C_ERASE], before,
        "the table libedit re-applies from is not updated"
    );

    // `name=` with nothing after it disables the character.
    assert_eq!(
        tty_stty(&mut el, 0, &[&wide("setty"), &wide("-d"), &wide("erase=")]),
        0
    );
    assert_eq!(el.el_tty.t_ed.c_cc[plat::VERASE], plat::VDISABLE);

    // A malformed escape is `parse__escape`'s -1 stored unchecked, which
    // is why `setty erase=X` yields 0xFF where `erase=^H` works.
    assert_eq!(
        tty_stty(&mut el, 0, &[&wide("setty"), &wide("-d"), &wide("erase=X")]),
        0
    );
    assert_eq!(el.el_tty.t_ed.c_cc[plat::VERASE], 0xFF);
}

// [spec:libedit:sem:tty.tty-stty-fn/test]
/// The three refusals. The last is ERR-terminal-06: the C decides whether
/// a token is an option by reading `argv[0][2]`, one element past the end
/// of the one-character string `"-"`; the port tests the length instead,
/// so `-` is not an option and falls through to the argument loop, where
/// it is an empty name that matches nothing.
#[test]
fn setty_refuses_an_empty_vector_an_unknown_switch_and_a_bare_dash() {
    let mut el = tty_el();
    assert_eq!(tty_stty(&mut el, 0, &[]), -1);

    let err = on_err(&mut el, |el| {
        assert_eq!(tty_stty(el, 0, &[&wide("setty"), &wide("-z")]), -1);
    });
    assert_eq!(err, b"setty: Unknown switch `z'.\n");

    let err = on_err(&mut el, |el| {
        assert_eq!(tty_stty(el, 0, &[&wide("setty"), &wide("-")]), -1);
    });
    assert_eq!(err, b"setty: Invalid argument `'.\n");

    // An unknown mode name, after an argument that was applied: the
    // earlier one keeps its effect.
    let err = on_err(&mut el, |el| {
        assert_eq!(
            tty_stty(
                el,
                0,
                &[&wide("setty"), &wide("-d"), &wide("+ixon"), &wide("nope")]
            ),
            -1
        );
    });
    assert_eq!(err, b"setty: Invalid argument `nope'.\n");
    assert_eq!(
        el.el_tty.t_t[ED_IO][MD_INP].t_setmask & plat::IXON,
        plat::IXON
    );
}

// [spec:libedit:sem:tty.tty-printchar-fn/test]
/// The C's version is behind `#ifdef notyet`, has no callers and does not
/// compile (ERR-terminal-65); the rule says to write it fresh from the
/// stated intent, quirks included. Both quirks are here: the newline fires
/// on `i == 0`, so the grouping is offset by one rather than being rows of
/// five, and the indices with no `ttymodes` entry silently print nothing
/// while still counting towards the grouping.
#[test]
fn the_debug_dump_groups_its_rows_one_position_out() {
    let mut el = tty_el();
    let mut row = [0u8; C_NCC];
    for (i, slot) in row.iter_mut().enumerate() {
        *slot = i as u8 + 1;
    }
    let out = on_err(&mut el, |el| tty_printchar(el, &row));
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "intr ^A \n\
         quit ^B erase ^C kill ^D eof ^E eol ^F \n\
         eol2 ^G start ^K \n\
         stop ^L werase ^M susp ^N reprint ^P \n\
         discard ^Q lnext ^R \n\
         min ^X time ^Y \n"
    );
}

// [spec:libedit:sem:tty.tty-printchar-fn/test]
/// The caret rendering is `s[i] + 'A' - 1` through `fprintf`'s `%c`, so it
/// is only meaningful for a byte in 1..26. Everything else — 0, and the
/// disable byte in particular — renders as whatever that arithmetic
/// happens to produce.
#[test]
fn the_debug_dump_renders_a_non_control_byte_as_garbage() {
    let mut el = tty_el();
    let mut row = [0u8; C_NCC];
    row[C_INTR] = 0;
    row[C_QUIT] = 0xff;
    let out = on_err(&mut el, |el| tty_printchar(el, &row));
    let out = String::from_utf8_lossy(&out).into_owned();
    assert!(out.starts_with("intr ^@ \n"), "{out}");
    assert!(out.contains("quit ^? "), "{out}");
}

// [spec:libedit:sem:tty.tty-get-signal-character-fn/test]
/// ERR-terminal-36, reproduced: the guard tests `ECHOCTL` — a `c_lflag`
/// bit — against `c_iflag`. On glibc `ECHOCTL` has the same value as the
/// input flag `IUCLC`, which libedit never sets and which is off on a
/// normal terminal, so this always answers -1 and `rl_echo_signal_char` is
/// a silent no-op. That is the observable a caller must not read anything
/// into.
#[test]
fn the_signal_character_is_always_absent_on_this_platform() {
    let mut el = tty_el();
    el.el_tty.t_ed.c_lflag = plat::ECHOCTL;
    el.el_tty.t_c[ED_IO][C_INTR] = 3;

    assert_eq!(tty_get_signal_character(&mut el, signo::SIGINT), -1);
    assert_eq!(
        plat::ECHOCTL,
        plat::IUCLC,
        "the bit the guard actually tests"
    );
}

// [spec:libedit:sem:tty.tty-get-signal-character-fn/test]
/// Past the guard — reachable only by setting the input flag the guard
/// really tests — ERR-terminal-37 is live: the rows of `t_c` are keyed by
/// libedit's `C_*` constants and the switch subscripts them with termios
/// `V*` values. `VINTR` and `VQUIT` coincide with `C_INTR` and `C_QUIT` by
/// accident, which is what hides it; `VSUSP` is 10 and `C_SUSP` is 13, so
/// `SIGTSTP` answers with the flow-control start character.
#[test]
fn a_reachable_signal_character_answers_from_the_wrong_column() {
    let mut el = tty_el();
    el.el_tty.t_ed.c_iflag = plat::IUCLC;
    el.el_tty.t_c[ED_IO][C_INTR] = 3;
    el.el_tty.t_c[ED_IO][C_QUIT] = 28;
    el.el_tty.t_c[ED_IO][C_SUSP] = 26;
    el.el_tty.t_c[ED_IO][C_START] = 17;

    assert_eq!(tty_get_signal_character(&mut el, signo::SIGINT), 3);
    assert_eq!(tty_get_signal_character(&mut el, signo::SIGQUIT), 28);
    assert_eq!(
        tty_get_signal_character(&mut el, signo::SIGTSTP),
        17,
        "C_START, not C_SUSP: VSUSP is 10 and that is C_START's row"
    );
    // No arm is compiled for anything else, `SIGINFO` included — it is
    // BSD-only, as is `VSTATUS`.
    assert_eq!(tty_get_signal_character(&mut el, signo::SIGHUP), -1);
    // The edit-mode row answers whatever the current mode is.
    el.el_tty.t_mode = EX_IO as u8;
    assert_eq!(tty_get_signal_character(&mut el, signo::SIGINT), 3);
}
