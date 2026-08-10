// Copyright 2026 Necessary Innovations AB.
//
// This file is not derived from `term` 1.2.1 — upstream shipped no test
// coverage for `parm::expand` beyond the eight cases inside `src/parm.rs`.
// It is licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option, to
// match the crate it tests.

//! The printf-style conversion specifiers: `%[[:]flags][width[.precision]]`
//! followed by one of `doxXs`.
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
use nshterm::parm::{Error, Param};

// ---------------------------------------------------------------------------
// printf-style width, precision and flags
// ---------------------------------------------------------------------------

#[test]
fn width_right_justifies_by_default() {
    assert_eq!(s(b"%p1%9d|", &nums(&[7])), "        7|");
    assert_eq!(s(b"%p1%2s|", &[Param::Words("f".to_owned())]), " f|");
}

#[test]
fn the_colon_escape_admits_a_minus_flag() {
    // terminfo(5): "Use a ':' to allow the next character to be a '-' flag,
    // avoiding interpreting '%-' as an operator." Without it `%-` subtracts.
    assert_eq!(s(b"%p1%:-9d|", &nums(&[7])), "7        |");
    assert_eq!(ex_err(b"%p1%-9d", &nums(&[7])), Error::StackUnderflow);
}

#[test]
fn space_and_sign_flags() {
    assert_eq!(s(b"%p1% 5d|", &nums(&[7])), "    7|");
    assert_eq!(s(b"%p1%:+d", &nums(&[7])), "+7");
    // A sign always wins over the space flag, as in C.
    assert_eq!(s(b"%p1%:+ d", &nums(&[7])), "+7");
}

#[test]
fn precision_pads_integers_and_truncates_strings() {
    assert_eq!(s(b"%p1%.5d", &nums(&[15])), "00015");
    assert_eq!(s(b"%p1%.5d", &nums(&[-15])), "-00015");
    assert_eq!(s(b"%p1%.3s", &[Param::Words("xterm".to_owned())]), "xte");
    // A precision at or above the length leaves the string whole.
    assert_eq!(s(b"%p1%.9s", &[Param::Words("xterm".to_owned())]), "xterm");
}

#[test]
fn omitted_and_zero_precision_differ() {
    assert_eq!(s(b"%p1%d", &nums(&[0])), "0");
    assert_eq!(s(b"%p1%.d", &nums(&[0])), "");
    assert_eq!(s(b"%p1%.0d", &nums(&[0])), "");
    assert_eq!(s(b"%p1%.0d", &nums(&[7])), "7");

    let word = [Param::Words("xterm".to_owned())];
    assert_eq!(s(b"%p1%s", &word), "xterm");
    assert_eq!(s(b"%p1%.s", &word), "");
    assert_eq!(s(b"%p1%.0s", &word), "");
}

#[test]
fn zero_precision_obeys_format_flags() {
    assert_eq!(s(b"%p1%5.0d|", &nums(&[0])), "     |");
    assert_eq!(s(b"%p1%:+5.0d|", &nums(&[0])), "    +|");
    assert_eq!(s(b"%p1% 5.0d|", &nums(&[0])), "     |");
    assert_eq!(s(b"%p1%05.0d|", &nums(&[0])), "     |");
    assert_eq!(
        s(b"%p1%5.0s|", &[Param::Words("xterm".to_owned())]),
        "     |"
    );

    assert_eq!(s(b"%p1%#.0o", &nums(&[0])), "0");
    assert_eq!(s(b"%p1%#5.0o|", &nums(&[0])), "    0|");
    assert_eq!(s(b"%p1%#.0x", &nums(&[0])), "");
    assert_eq!(s(b"%p1%#.0X", &nums(&[0])), "");
}

#[test]
fn width_and_precision_combine() {
    assert_eq!(s(b"%p1%5.3d", &nums(&[7])), "  007");
    assert_eq!(s(b"%p1%:-5.3d|", &nums(&[7])), "007  |");
}

#[test]
fn alternate_form_prefixes_octal_and_hex() {
    assert_eq!(s(b"%p1%#o", &nums(&[255])), "0377");
    assert_eq!(s(b"%p1%#x", &nums(&[255])), "0xff");
    assert_eq!(s(b"%p1%#X", &nums(&[255])), "0XFF");
    // C suppresses the `0x` prefix for a zero value; so does this.
    assert_eq!(s(b"%p1%#x", &nums(&[0])), "0");
    assert_eq!(s(b"%p1%#X", &nums(&[0])), "0");
}

#[test]
fn hex_width_precision_drives_the_256_colour_palette() {
    // xterm-256color `initc`: `%2.2X` per channel, over an OSC 4 payload.
    // 1000/1000 -> 255 -> FF, 500/1000 -> 127 -> 7F, 0 -> 00. The precision
    // is what keeps a single-digit channel from shortening the sequence.
    let initc = cap(&fixture("xterm-256color"), "initc");
    assert_eq!(
        ex(&initc, &nums(&[7, 1000, 500, 0])),
        b"\x1b]4;7;rgb:FF/7F/00\x1b\\"
    );
}

#[test]
fn oversized_width_and_precision_are_rejected() {
    assert_eq!(
        ex_err(b"%p1%99999999999999999999d", &nums(&[1])),
        Error::FormatWidthOverflow
    );
    assert_eq!(
        ex_err(b"%p1%.99999999999999999999d", &nums(&[1])),
        Error::FormatPrecisionOverflow
    );
}

#[test]
fn an_unknown_conversion_after_a_flag_is_rejected() {
    assert_eq!(
        ex_err(b"%p1%1z", &nums(&[7])),
        Error::UnrecognizedFormatOption('z')
    );
}
