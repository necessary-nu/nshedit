// Copyright 2026 Necessary Innovations AB.
//
// This file is not derived from `term` 1.2.1 — upstream shipped no test
// coverage for `parm::expand` beyond the eight cases inside `src/parm.rs`.
// It is licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option, to
// match the crate it tests.

//! What `expand` does with input the grammar does not bless.
//!
//! A terminfo entry is a file on disk that this crate did not write, so every
//! byte sequence in it reaches the expander. Nothing here may panic; an
//! `Err` is a fine answer, a crash is not.
//!
//! The `defects` module at the bottom holds tests written to the behaviour
//! `terminfo(5)` and `tparm(3)` specify but this crate does not yet produce.
//! They are `#[ignore]`d rather than deleted or weakened. A test graduates out
//! of that module, unchanged, when the defect it describes is fixed — the two
//! padding tests above did.
//!
//! Expected values come from one of two places: a run of the real ncurses
//! `tparm(3)` against the same capability and parameters, or the grammar as
//! `terminfo(5)` states it, where ncurses is quirkier than its own
//! documentation (`%:+` is the notable case — the man page admits the `:`
//! escape before any flag, ncurses only honours it before `-`).
//!
//! Capability strings are quoted from the 30 compiled fixtures in
//! `tests/data/` wherever a real terminal ships one that exercises the
//! production under test. A comment names the fixture and capname so the
//! claim can be checked with `infocmp`.

mod common;

use common::{cap, ex, ex_err, fixture, fixture_names, nums, s};
use nshterm::parm::{Error, Param, Variables, expand};

// ---------------------------------------------------------------------------
// malformed input — a terminfo file may contain anything
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_percent_sequence_is_rejected() {
    assert_eq!(ex_err(b"%y", &[]), Error::UnrecognizedFormatOption('y'));
    assert_eq!(ex_err(b"%Q", &[]), Error::UnrecognizedFormatOption('Q'));
}

#[test]
fn every_operator_reports_stack_underflow_rather_than_reaching_past_the_stack() {
    for op in [
        &b"%d"[..],
        b"%s",
        b"%c",
        b"%o",
        b"%x",
        b"%X",
        b"%l",
        b"%!",
        b"%~",
        b"%t",
        b"%Pa",
        b"%PA",
        b"%5d",
    ] {
        assert_eq!(
            ex_err(op, &[]),
            Error::StackUnderflow,
            "{:?} on an empty stack",
            String::from_utf8_lossy(op)
        );
    }
    for op in [
        &b"%+"[..],
        b"%-",
        b"%*",
        b"%/",
        b"%m",
        b"%&",
        b"%|",
        b"%^",
        b"%=",
        b"%<",
        b"%>",
        b"%A",
        b"%O",
    ] {
        assert_eq!(
            ex_err(op, &[]),
            Error::StackUnderflow,
            "{:?} on an empty stack",
            String::from_utf8_lossy(op)
        );
        let mut one = b"%{1}".to_vec();
        one.extend_from_slice(op);
        assert_eq!(
            ex_err(&one, &[]),
            Error::StackUnderflow,
            "{:?} with one operand",
            String::from_utf8_lossy(op)
        );
    }
}

#[test]
fn truncated_sequences_end_the_expansion_without_panicking() {
    // Each of these runs off the end of the capability mid-construct. None of
    // them may panic; emitting the prefix already produced is enough.
    for cap in [
        &b"A%"[..],
        b"A%p",
        b"A%{12",
        b"A%'x",
        b"A%P",
        b"A%g",
        b"A%5",
        b"A%?%{1}%t",
    ] {
        let out = expand(cap, &[], &mut Variables::new());
        assert_eq!(
            out,
            Ok(b"A".to_vec()),
            "{:?} should emit its literal prefix and stop",
            String::from_utf8_lossy(cap)
        );
    }
}

#[test]
fn an_unterminated_conditional_swallows_the_rest() {
    // No `%;` closes this, so the skip for a false condition runs to the end
    // of the capability. ncurses does the same; the point is that it
    // terminates rather than looping or panicking.
    assert_eq!(s(b"A%?%p1%tB", &nums(&[0])), "A");
    assert_eq!(s(b"A%?%p1%tB", &nums(&[1])), "AB");
    // An `%e` with no `%?` skips to a `%;` that never arrives.
    assert_eq!(s(b"A%eB", &[]), "A");
}

#[test]
fn no_capability_in_any_fixture_panics_the_expander() {
    // The real reason this matters: `expand` runs over bytes that came out of
    // a file on disk, and terminfo entries contain user-defined capabilities
    // (`u6` and friends) that are not tparm strings at all. Every string in
    // all 30 fixtures, against several parameter shapes, must return — Ok or
    // Err, but never panic and never diverge.
    let shapes: [Vec<Param>; 4] = [
        nums(&[]),
        nums(&[0; 9]),
        nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9]),
        nums(&[-1, 1000, 255, 65, 0, 0, 0, 0, 0]),
    ];
    let mut expanded = 0usize;
    for name in fixture_names() {
        let term = fixture(&name);
        for (capname, value) in &term.strings {
            for params in &shapes {
                // A capability may legitimately fail (`u6` underflows the
                // stack); it may not take the process down with it.
                let outcome = expand(value, params, &mut Variables::new());
                expanded += 1;

                // The `u6`-`u9` slots hold scanf templates for querying the
                // terminal, not capabilities to expand — xterm-256color's
                // `u8` is `\E[?%[;0123456789]c`, whose `%[` is a scanf class.
                // Everything else in the database is a tparm string, so an
                // unrecognized `%` there would mean this crate is missing a
                // production terminfo(5) defines.
                if matches!(capname, &"u6" | &"u7" | &"u8" | &"u9") {
                    continue;
                }
                if let Err(e @ Error::UnrecognizedFormatOption(_)) = outcome {
                    panic!("{name}/{capname}: {e}");
                }
            }
        }
    }
    assert!(
        expanded > 5_000,
        "expected the fixture sweep to cover the whole database, ran {expanded}"
    );
}

#[test]
fn a_non_tparm_capability_errors_rather_than_panicking() {
    // `u6` is a scanf-style user string, not a parameterised capability: it
    // has `%d` conversions with nothing pushing operands for them.
    let u6 = cap(&fixture("xterm"), "u6");
    assert_eq!(
        expand(&u6, &[], &mut Variables::new()),
        Err(Error::StackUnderflow)
    );
}

// ---------------------------------------------------------------------------
// padding — timing, not text, so `expand` must leave it alone
// ---------------------------------------------------------------------------

#[test]
fn padding_delays_survive_expansion() {
    // sem:terminal.tgoto-fn is explicit: "`term`'s expander recognises
    // `$<...>` delay markers and **discards them**, so a capability that
    // carried padding loses it on expansion... the port's expansion
    // routine must preserve the `$<...>` runs in its output". ncurses'
    // tparm passes them through untouched, because realising the delay is
    // tputs' job, not tparm's.
    //
    // These three are shipped capabilities from tests/data, covering
    // padding at the end of a parameterised string, at the end of a plain
    // one, and in the middle of one.
    let cup = cap(&fixture("vt100"), "cup");
    assert_eq!(ex(&cup, &nums(&[4, 12])), b"\x1b[5;13H$<5>");

    let flash = cap(&fixture("linux"), "flash");
    assert_eq!(ex(&flash, &[]), b"\x1b[?5h\x1b[?5l$<200/>");

    let flash = cap(&fixture("xterm"), "flash");
    assert_eq!(ex(&flash, &[]), b"\x1b[?5h$<100/>\x1b[?5l");
}

#[test]
fn a_dollar_that_opens_no_delay_is_literal() {
    // sem:terminal.tputs-fn: "A `$` not followed by `<`, and an
    // unterminated `$<`, are emitted verbatim." `term`'s expander entered a
    // Delay state on a bare `$` rather than on `$<`, so text between an
    // incidental `$` and the next `>` disappeared — and a trailing `$`
    // disappeared with it. ncurses leaves both alone.
    assert_eq!(s(b"a$b>c", &[]), "a$b>c");
    assert_eq!(s(b"cost: $", &[]), "cost: $");
    assert_eq!(s(b"$<unterminated", &[]), "$<unterminated");
}

// ---------------------------------------------------------------------------
// defects
// ---------------------------------------------------------------------------

/// Tests written to the behaviour `terminfo(5)` and `tparm(3)` specify, which
/// `expand` does not currently produce.
///
/// Each is `#[ignore]`d rather than deleted or weakened: a known-failing test
/// that states the contract is worth more than a passing one that pins the
/// defect in place. Run them with `cargo test -p nshterm -- --ignored`.
///
/// Fixing one means moving its test up into the body of this file with its
/// assertions untouched, not editing it in place.
mod defects {
    use super::{cap, ex, ex_err, fixture, nums, s};
    use nshterm::parm::Error;

    #[test]
    #[ignore = "expand panics on %p0 (subtract-with-overflow indexing mparams)"]
    fn parameter_index_zero_is_rejected_not_panicked_on() {
        // terminfo(5) gives the range as `%p[1-9]`, so `%p0` is malformed and
        // belongs with `%pa` as InvalidParameterIndex. Instead `expand`
        // computes `0usize - 1` to index a nine-element array: a panic in a
        // debug build, an out-of-bounds index in a release one. A terminfo
        // file is untrusted input, so this is reachable.
        assert_eq!(
            ex_err(b"%p0%d", &nums(&[5])),
            Error::InvalidParameterIndex('0')
        );
    }

    #[test]
    #[ignore = "expand panics on division by zero; ncurses pushes 0"]
    fn division_by_zero_yields_zero() {
        // ncurses' tparm guards both: `npush(y ? (x / y) : 0)`. `expand`
        // evaluates `x / y` directly, so a capability whose divisor comes
        // from a parameter takes the process down.
        assert_eq!(s(b"%p1%{0}%/%d", &nums(&[5])), "0");
        assert_eq!(s(b"%p1%p2%/%d", &nums(&[5, 0])), "0");
    }

    #[test]
    #[ignore = "expand panics on modulo by zero; ncurses pushes 0"]
    fn modulo_by_zero_yields_zero() {
        assert_eq!(s(b"%p1%{0}%m%d", &nums(&[5])), "0");
    }

    #[test]
    #[ignore = "expand panics on integer overflow in %+ %- %* and on i32::MIN / -1"]
    fn arithmetic_overflow_wraps_rather_than_panicking() {
        // C's int arithmetic wraps in every implementation terminfo targets,
        // and ncurses reports -2147483648 here. Whatever this crate chooses —
        // wrapping, saturating, or an Error variant — it must not panic on
        // bytes that came out of a file.
        assert_eq!(s(b"%p1%{2147483647}%+%d", &nums(&[1])), "-2147483648");
    }

    #[test]
    #[ignore = "expand has no zero-pad flag: %02x renders as ' f' instead of '0f'"]
    fn a_leading_zero_in_the_width_pads_with_zeros() {
        // terminfo(5) says the conversion works "as in printf", and ncurses
        // hands the whole spec to sprintf, so `%02x` zero-pads. `expand`
        // folds the leading 0 into the width and pads with spaces; `Flags`
        // has no zero-pad member at all.
        //
        // This is not academic. The linux console's `initc` — shipped, and in
        // tests/data — is
        //   \E]P%p1%x%p2%{255}%*%{1000}%/%02x%p3...%02x%p4...%02x
        // so any channel below 0x10 emits a space inside an OSC payload
        // instead of a leading zero, corrupting the sequence.
        assert_eq!(s(b"%p1%02d", &nums(&[7])), "07");
        assert_eq!(s(b"%p1%03d", &nums(&[7])), "007");
        assert_eq!(s(b"%p1%02x", &nums(&[15])), "0f");

        let initc = cap(&fixture("linux"), "initc");
        assert_eq!(ex(&initc, &nums(&[1, 1000, 60, 0])), b"\x1b]P1ff0f00");
    }

    #[test]
    #[ignore = "expand renders %#o of 0 as '00'; C and ncurses give '0'"]
    fn alternate_octal_of_zero_is_a_single_zero() {
        // The alternate form guarantees a leading 0, it does not add one to a
        // value that is already 0. `expand` special-cases `#` for hex by
        // testing `d != 0` but unconditionally prepends for octal.
        assert_eq!(s(b"%p1%#o", &nums(&[0])), "0");
    }
}
