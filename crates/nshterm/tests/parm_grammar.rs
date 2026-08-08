// Copyright 2026 Necessary Innovations AB.
//
// This file is not derived from `term` 1.2.1 — upstream shipped no test
// coverage for `parm::expand` beyond the eight cases inside `src/parm.rs`.
// It is licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option, to
// match the crate it tests.

//! The terminfo parameterised-string grammar: what each `%` sequence in a
//! string capability does.
//!
//! Covers parameter pushes, the output conversions, literals, `%i`, the
//! conditional forms and the operator set — everything `terminfo(5)` lists
//! except the printf-style conversion specifiers, which are in
//! `parm_format.rs`, and behaviour on malformed input, which is in
//! `parm_robustness.rs`.
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

pub mod common;

use common::{cap, ex, ex_err, fixture, nums, s};
use nshterm::parm::{Error, Param, Variables, expand};

// ---------------------------------------------------------------------------
// %p — parameter push
// ---------------------------------------------------------------------------

#[test]
fn push_reaches_all_nine_parameters() {
    // terminfo(5): `%p[1-9]` push i'th parameter. Nine is the whole range.
    assert_eq!(
        s(
            b"%p1%d%p2%d%p3%d%p4%d%p5%d%p6%d%p7%d%p8%d%p9%d",
            &nums(&[1, 2, 3, 4, 5, 6, 7, 8, 9])
        ),
        "123456789"
    );
}

#[test]
fn unsupplied_parameters_read_as_zero() {
    // tparm takes nine slots whatever the caller passed; the tail is zero.
    assert_eq!(s(b"%p1%d%p2%d%p3%d", &nums(&[1, 2])), "120");
    assert_eq!(s(b"%p9%d", &[]), "0");
}

#[test]
fn push_of_a_non_digit_is_rejected() {
    assert_eq!(
        ex_err(b"%pa", &[]),
        Error::InvalidParameterIndex('a'),
        "`%p` must be followed by a digit"
    );
}

#[test]
fn parameters_may_be_strings() {
    assert_eq!(s(b"%p1%s", &[Param::Words("xterm".to_owned())]), "xterm");
}

// ---------------------------------------------------------------------------
// %d %s %c — output
// ---------------------------------------------------------------------------

#[test]
fn char_output_truncates_to_a_byte() {
    // ncurses casts and truncates rather than bounds-checking; 300 & 0xff is
    // 0x2c, a comma. Matching that matters because terminfo entries in the
    // wild rely on it (see `rep`, below).
    assert_eq!(s(b"%p1%c", &nums(&[65])), "A");
    assert_eq!(ex(b"%p1%c", &nums(&[300])), b",");
}

#[test]
fn char_output_of_zero_becomes_0200() {
    // A NUL cannot travel down a terminal line, so ncurses substitutes 0200.
    assert_eq!(ex(b"%p1%c", &nums(&[0])), b"\x80");
}

#[test]
fn output_operators_reject_the_wrong_stack_type() {
    assert_eq!(
        ex_err(b"%p1%d", &[Param::Words("x".to_owned())]),
        Error::TypeMismatch
    );
    assert_eq!(ex_err(b"%p1%s", &nums(&[1])), Error::TypeMismatch);
    assert_eq!(
        ex_err(b"%p1%c", &[Param::Words("x".to_owned())]),
        Error::TypeMismatch
    );
}

#[test]
fn percent_percent_emits_one_percent() {
    assert_eq!(s(b"%%literal%%", &[]), "%literal%");
}

// ---------------------------------------------------------------------------
// %'c' and %{n} — literals
// ---------------------------------------------------------------------------

#[test]
fn char_constant_pushes_its_byte_value() {
    assert_eq!(s(b"%'a'%d", &[]), "97");
    assert_eq!(s(b"%'0'%d", &[]), "48");
    // The quoted byte is taken verbatim, including `%` itself.
    assert_eq!(s(b"%'%'%d", &[]), "37");
}

#[test]
fn unterminated_char_constant_is_rejected() {
    assert_eq!(
        ex_err(b"%'ab'%d", &[]),
        Error::MalformedCharacterConstant,
        "a char constant holds exactly one byte"
    );
}

#[test]
fn integer_constant_pushes_its_value() {
    assert_eq!(s(b"%{255}%d", &[]), "255");
    assert_eq!(s(b"%{0}%d", &[]), "0");
    // Empty braces read as zero, as they do in ncurses.
    assert_eq!(s(b"%{}%d", &[]), "0");
}

#[test]
fn malformed_integer_constants_are_rejected() {
    assert_eq!(ex_err(b"%{1a}%d", &[]), Error::MalformedIntegerConstant);
    // terminfo(5) gives no sign in `%{nn}`; ncurses rejects it too.
    assert_eq!(ex_err(b"%{-3}%d", &[]), Error::MalformedIntegerConstant);
    assert_eq!(
        ex_err(b"%{99999999999}%d", &[]),
        Error::IntegerConstantOverflow
    );
}

// ---------------------------------------------------------------------------
// %i — one-base the first two parameters
// ---------------------------------------------------------------------------

#[test]
fn increment_applies_to_parameters_not_the_stack() {
    // `%i` rewrites p1 and p2 in place; anything already pushed is untouched.
    assert_eq!(s(b"%p1%d%i%p1%d", &nums(&[5])), "56");
}

#[test]
fn increment_drives_real_cursor_addressing() {
    // xterm(5) `cup` — terminfo is row-first, column-second, and both are
    // zero-based until `%i` makes them one-based for the ANSI CUP sequence.
    let cup = cap(&fixture("xterm"), "cup");
    assert_eq!(ex(&cup, &nums(&[4, 12])), b"\x1b[5;13H");
    assert_eq!(ex(&cup, &nums(&[0, 0])), b"\x1b[1;1H");

    // linux `hpa` — a one-parameter capability; `%i` still touches p2, which
    // this capability never reads.
    let hpa = cap(&fixture("linux"), "hpa");
    assert_eq!(ex(&hpa, &nums(&[0])), b"\x1b[1G");
    assert_eq!(ex(&hpa, &nums(&[41])), b"\x1b[42G");
}

#[test]
fn increment_rejects_string_parameters() {
    assert_eq!(
        ex_err(b"%i", &[Param::Words("x".to_owned())]),
        Error::TypeMismatch
    );
}

// ---------------------------------------------------------------------------
// %? %t %e %; — conditionals
// ---------------------------------------------------------------------------

#[test]
fn conditional_selects_the_taken_branch() {
    assert_eq!(s(b"%?%p1%tYES%eNO%;", &nums(&[1])), "YES");
    assert_eq!(s(b"%?%p1%tYES%eNO%;", &nums(&[0])), "NO");
    // Text after `%;` belongs to neither branch.
    assert_eq!(s(b"A%?%p1%tB%;C", &nums(&[0])), "AC");
    assert_eq!(s(b"A%?%p1%tB%;C", &nums(&[1])), "ABC");
}

#[test]
fn conditionals_nest() {
    // The skip that `%t` performs on a false condition has to count nested
    // `%?`/`%;` pairs, or an inner `%e` steals the outer else-branch.
    let cap = b"%?%p1%t%?%p2%tBOTH%eFIRST%;%e%?%p2%tSECOND%eNEITHER%;%;";
    assert_eq!(s(cap, &nums(&[1, 1])), "BOTH");
    assert_eq!(s(cap, &nums(&[1, 0])), "FIRST");
    assert_eq!(s(cap, &nums(&[0, 1])), "SECOND");
    assert_eq!(s(cap, &nums(&[0, 0])), "NEITHER");
}

#[test]
fn else_if_chains_walk_to_the_matching_arm() {
    // xterm `setf`: five arms, each `%e` opening a fresh test. The skip after
    // a taken branch has to run to the *last* `%;`, not the first.
    let setf = cap(&fixture("xterm"), "setf");
    assert_eq!(ex(&setf, &nums(&[1])), b"\x1b[34m"); // first arm
    assert_eq!(ex(&setf, &nums(&[4])), b"\x1b[31m"); // third arm
    assert_eq!(ex(&setf, &nums(&[6])), b"\x1b[33m"); // fourth arm
    assert_eq!(ex(&setf, &nums(&[2])), b"\x1b[32m"); // fallthrough
}

#[test]
fn conditional_drives_the_256_colour_selector() {
    // xterm-256color `setaf` is the genuine workout: two comparisons, an
    // arithmetic else-branch, and a bare else, over three disjoint ranges.
    let setaf = cap(&fixture("xterm-256color"), "setaf");
    assert_eq!(ex(&setaf, &nums(&[1])), b"\x1b[31m");
    assert_eq!(ex(&setaf, &nums(&[7])), b"\x1b[37m");
    assert_eq!(ex(&setaf, &nums(&[8])), b"\x1b[90m");
    assert_eq!(ex(&setaf, &nums(&[15])), b"\x1b[97m");
    assert_eq!(ex(&setaf, &nums(&[42])), b"\x1b[38;5;42m");

    // The `setab` twin, so the pair cannot drift apart unnoticed.
    let setab = cap(&fixture("xterm-256color"), "setab");
    assert_eq!(ex(&setab, &nums(&[3])), b"\x1b[43m");
    assert_eq!(ex(&setab, &nums(&[200])), b"\x1b[48;5;200m");
}

#[test]
fn conditional_bodies_may_be_empty() {
    // linux-16color `setaf` folds the bright-colour suffix into a conditional
    // whose false arm emits a different literal, both non-empty; rxvt `sgr`
    // instead leaves whole arms out. Both shapes have to survive.
    let setaf = cap(&fixture("linux-16color"), "setaf");
    assert_eq!(ex(&setaf, &nums(&[3])), b"\x1b[33;21m");
    assert_eq!(ex(&setaf, &nums(&[7])), b"\x1b[37;21m");
    assert_eq!(ex(&setaf, &nums(&[9])), b"\x1b[31;1m");

    let sgr = cap(&fixture("rxvt"), "sgr");
    assert_eq!(ex(&sgr, &nums(&[0; 9])), b"\x1b[0m\x0f");
    assert_eq!(
        ex(&sgr, &nums(&[0, 0, 0, 0, 0, 1, 0, 0, 0])),
        b"\x1b[0;1m\x0f"
    );
}

#[test]
fn conditional_operands_are_checked() {
    assert_eq!(ex_err(b"%t", &[]), Error::StackUnderflow);
    assert_eq!(
        ex_err(b"%p1%tX%;", &[Param::Words("x".to_owned())]),
        Error::TypeMismatch
    );
}

// ---------------------------------------------------------------------------
// arithmetic, bitwise, comparison and logic
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_operators_pop_right_then_left() {
    // Order matters: `%{10}%{3}%-` is 10 - 3, not 3 - 10.
    assert_eq!(s(b"%{10}%{3}%+%d", &[]), "13");
    assert_eq!(s(b"%{10}%{3}%-%d", &[]), "7");
    assert_eq!(s(b"%{10}%{3}%*%d", &[]), "30");
    assert_eq!(s(b"%{10}%{3}%/%d", &[]), "3");
    assert_eq!(s(b"%{10}%{3}%m%d", &[]), "1");
}

#[test]
fn bitwise_operators() {
    assert_eq!(s(b"%{6}%{3}%&%d", &[]), "2");
    assert_eq!(s(b"%{6}%{3}%|%d", &[]), "7");
    assert_eq!(s(b"%{6}%{3}%^%d", &[]), "5");
}

#[test]
fn unary_operators() {
    // `%!` is logical negation of "greater than zero"; `%~` is bit complement.
    assert_eq!(s(b"%{0}%!%d", &[]), "1");
    assert_eq!(s(b"%{5}%!%d", &[]), "0");
    assert_eq!(s(b"%{0}%~%d", &[]), "-1");
    assert_eq!(s(b"%{5}%~%d", &[]), "-6");
}

#[test]
fn logical_and_or_test_positivity() {
    assert_eq!(s(b"%{1}%{1}%A%d", &[]), "1");
    assert_eq!(s(b"%{1}%{0}%A%d", &[]), "0");
    assert_eq!(s(b"%{1}%{0}%O%d", &[]), "1");
    assert_eq!(s(b"%{0}%{0}%O%d", &[]), "0");
}

#[test]
fn arithmetic_drives_real_colour_capabilities() {
    // linux-16color `setaf`: `%{8}%m` folds 0-15 into 0-7, and `%{7}%>`
    // picks the bright suffix.
    let setaf = cap(&fixture("linux-16color"), "setaf");
    assert_eq!(ex(&setaf, &nums(&[11])), b"\x1b[33;1m");

    // xterm-256color `rep`: `%{1}%-` decrements the repeat count.
    let rep = cap(&fixture("xterm-256color"), "rep");
    assert_eq!(ex(&rep, &nums(&[65, 4])), b"A\x1b[3b");
}

#[test]
fn binary_operators_reject_string_operands() {
    let words = [Param::Words("x".to_owned()), Param::Words("y".to_owned())];
    assert_eq!(ex_err(b"%p1%p2%+", &words), Error::TypeMismatch);
    assert_eq!(ex_err(b"%p1%p2%=", &words), Error::TypeMismatch);
    assert_eq!(ex_err(b"%p1%!", &words), Error::TypeMismatch);
}

#[test]
fn strlen_measures_a_string() {
    assert_eq!(s(b"%p1%l%d", &[Param::Words("xterm".to_owned())]), "5");
    assert_eq!(ex_err(b"%p1%l%d", &nums(&[1])), Error::TypeMismatch);
}

// ---------------------------------------------------------------------------
// %P / %g — static (A-Z) and dynamic (a-z) variables
// ---------------------------------------------------------------------------

#[test]
fn variables_round_trip_within_one_expansion() {
    assert_eq!(s(b"%p1%Pa%ga%ga%+%d", &nums(&[21])), "42");
    assert_eq!(s(b"%p1%PZ%gZ%gZ%*%d", &nums(&[7])), "49");
}

#[test]
fn unset_variables_read_as_zero() {
    assert_eq!(s(b"%gq%d", &[]), "0");
    assert_eq!(s(b"%gQ%d", &[]), "0");
}

#[test]
fn static_variables_persist_between_expansions() {
    // terminfo(5): static variables are "an array in the TERMINAL structure",
    // outliving any one tparm call. `expand` models that with the caller's
    // `Variables`, which is why the docs say to share it across capabilities
    // for one terminal.
    let mut vars = Variables::new();
    assert_eq!(
        expand(b"%p1%PA", &nums(&[7]), &mut vars),
        Ok(Vec::new()),
        "%P consumes the stack and emits nothing"
    );
    assert_eq!(expand(b"%gA%d", &[], &mut vars), Ok(b"7".to_vec()));
}

#[test]
fn dynamic_variables_also_persist_between_expansions() {
    // A deliberate divergence, pinned so it cannot change by accident.
    // terminfo(5) records that ncurses from 6.3 scopes the a-z set to a
    // single tparm call, while "before version 6.3, ncurses stores both
    // dynamic and static variables in persistent storage" — and Solaris XPG4
    // never distinguished them at all. This crate carries both in the
    // caller's `Variables`, so it matches pre-6.3 ncurses and XPG4, not
    // current ncurses. The man page's own advice is that no capability may
    // rely on either choice, so this is a documented difference rather than a
    // defect; the fixtures back that up, since every `%g[a-z]` among the 30
    // is preceded by a `%P` for the same letter in the same capability.
    let mut vars = Variables::new();
    assert_eq!(expand(b"%p1%Pz", &nums(&[9]), &mut vars), Ok(Vec::new()));
    assert_eq!(expand(b"%gz%d", &[], &mut vars), Ok(b"9".to_vec()));
}

#[test]
fn variable_names_outside_a_z_are_rejected() {
    assert_eq!(
        ex_err(b"%p1%P0", &nums(&[1])),
        Error::InvalidVariableName('0')
    );
    assert_eq!(ex_err(b"%g0", &[]), Error::InvalidVariableName('0'));
}

#[test]
fn variables_drive_the_linux_c_palette_capability() {
    // linux-c `initc` is the only fixture capability that really uses the
    // dynamic set: it stores a scaled channel in `r`, derives the high nibble
    // into `x`, prints it, then masks `r` for the low nibble. Two
    // conditionals pick hex digit vs letter. 1000/1000 * 255 = 255 = 0xff,
    // which is the only channel value that exercises the letter arm twice.
    let initc = cap(&fixture("linux-c"), "initc");
    assert_eq!(ex(&initc, &nums(&[0, 1000, 0, 0])), b"\x1b]P0ff0000");
    assert_eq!(ex(&initc, &nums(&[10, 0, 1000, 0])), b"\x1b]Pa00ff00");
}
