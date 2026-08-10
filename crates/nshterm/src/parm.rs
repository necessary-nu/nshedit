// Copyright 2019 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Parameterized string expansion

use self::Param::*;
use self::States::*;

use std::iter::repeat_n;

#[derive(Clone, Copy, PartialEq)]
enum States {
    Nothing,
    Percent,
    SetVar,
    GetVar,
    PushParam,
    CharConstant,
    CharClose,
    IntConstant(i32),
    FormatPattern(Flags, FormatState),
    SeekIfElse(usize),
    SeekIfElsePercent(usize),
    SeekIfEnd(usize),
    SeekIfEndPercent(usize),
}

#[derive(Copy, PartialEq, Clone)]
enum FormatState {
    Flags,
    Width,
    Precision,
}

/// Types of parameters a capability can use
#[derive(Clone)]
pub enum Param {
    /// An integer terminfo parameter.
    Number(i32),
    /// A string terminfo parameter.
    Words(String),
}

impl Default for Param {
    fn default() -> Self {
        Param::Number(0)
    }
}

/// An error from interpreting a parameterized string.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// Data was requested from the stack, but the stack didn't have enough elements.
    StackUnderflow,
    /// The type of the element(s) on top of the stack did not match the type that the operator
    /// wanted.
    TypeMismatch,
    /// An unrecognized format option was used.
    UnrecognizedFormatOption(char),
    /// An invalid variable name was used.
    InvalidVariableName(char),
    /// An invalid parameter index was used.
    InvalidParameterIndex(char),
    /// A malformed character constant was used.
    MalformedCharacterConstant,
    /// An integer constant was too large (overflowed an i32)
    IntegerConstantOverflow,
    /// A malformed integer constant was used.
    MalformedIntegerConstant,
    /// A format width was larger than 10000, the same bound ncurses enforces.
    FormatWidthOverflow,
    /// A format precision was larger than 10000, the same bound ncurses
    /// enforces.
    FormatPrecisionOverflow,
}

impl ::std::fmt::Display for Error {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        use self::Error::*;
        match self {
            StackUnderflow => f.write_str("not enough elements on the stack"),
            TypeMismatch => f.write_str("type mismatch"),
            UnrecognizedFormatOption(_) => f.write_str("unrecognized format option"),
            InvalidVariableName(_) => f.write_str("invalid variable name"),
            InvalidParameterIndex(_) => f.write_str("invalid parameter index"),
            MalformedCharacterConstant => f.write_str("malformed character constant"),
            IntegerConstantOverflow => f.write_str("integer constant computation overflowed"),
            MalformedIntegerConstant => f.write_str("malformed integer constant"),
            FormatWidthOverflow => f.write_str("format width is larger than 10000"),
            FormatPrecisionOverflow => f.write_str("format precision is larger than 10000"),
        }
    }
}

impl ::std::error::Error for Error {}

/// Container for static and dynamic variable arrays
#[derive(Clone, Default)]
pub struct Variables {
    /// Static variables A-Z
    sta_vars: [Param; 26],
    /// Dynamic variables a-z
    dyn_vars: [Param; 26],
}

impl Variables {
    /// Return a new zero-initialized Variables
    pub fn new() -> Variables {
        Default::default()
    }
}

/// Expand a parameterized capability
///
/// # Arguments
/// * `cap`    - string to expand
/// * `params` - vector of params for %p1 etc
/// * `vars`   - Variables struct for %Pa etc
///
/// To be compatible with ncurses, `vars` should be the same between calls to `expand` for
/// multiple capabilities for the same terminal.
///
/// # Padding
///
/// A `$<...>` padding run is **passed through verbatim**, exactly as ncurses'
/// `tparm(3)` does. Padding is timing, not text: realising it means emitting
/// pad characters at the tty's output speed, which only the routine that owns
/// the tty — `tputs(3)`, or its equivalent in the caller — can do. So this
/// function's job is to leave the marker intact, not to interpret it.
///
/// Keeping it in the returned bytes rather than reporting it out of band is
/// what makes the two kinds of capability interchangeable: a capability with
/// no `%` sequences is used raw, straight from [`TermInfo::strings`], with its
/// `$<...>` still in it. Were the delay returned alongside the output instead,
/// a caller would have to know whether the string had been through `expand`
/// before it knew where the padding lived.
///
/// This crate's ancestor, `term` 1.2.1, instead entered a delay state on a
/// bare `$` and dropped everything up to the next `>`, which both destroyed
/// the timing and swallowed any incidental `$`.
///
/// [`TermInfo::strings`]: crate::TermInfo::strings
pub fn expand(cap: &[u8], params: &[Param], vars: &mut Variables) -> Result<Vec<u8>, Error> {
    let mut state = Nothing;

    // expanded cap will only rarely be larger than the cap itself
    let mut output = Vec::with_capacity(cap.len());

    let mut stack: Vec<Param> = Vec::new();

    // Copy parameters into a local vector for mutability
    let mut mparams = [
        Number(0),
        Number(0),
        Number(0),
        Number(0),
        Number(0),
        Number(0),
        Number(0),
        Number(0),
        Number(0),
    ];
    for (dst, src) in mparams.iter_mut().zip(params.iter()) {
        *dst = (*src).clone();
    }

    for &c in cap.iter() {
        let cur = c as char;
        let mut old_state = state;
        match state {
            Nothing => {
                if cur == '%' {
                    state = Percent;
                } else {
                    // `$` is an ordinary byte here: a `$<...>` padding run
                    // contains no `%`, so copying it out bytewise preserves it
                    // for the caller's `tputs` to consume. See the note on
                    // `expand` for why the delay is not parsed here.
                    output.push(c);
                }
            }
            Percent => {
                match cur {
                    '%' => {
                        output.push(c);
                        state = Nothing
                    }
                    'c' => {
                        match stack.pop() {
                            // if c is 0, use 0200 (128) for ncurses compatibility
                            Some(Number(0)) => output.push(128u8),
                            // Don't check bounds. ncurses just casts and truncates.
                            Some(Number(c)) => output.push(c as u8),
                            Some(_) => return Err(Error::TypeMismatch),
                            None => return Err(Error::StackUnderflow),
                        }
                    }
                    'p' => state = PushParam,
                    'P' => state = SetVar,
                    'g' => state = GetVar,
                    '\'' => state = CharConstant,
                    '{' => state = IntConstant(0),
                    'l' => match stack.pop() {
                        Some(Words(s)) => stack.push(Number(s.len() as i32)),
                        Some(_) => return Err(Error::TypeMismatch),
                        None => return Err(Error::StackUnderflow),
                    },
                    '+' | '-' | '/' | '*' | '^' | '&' | '|' | 'm' => {
                        match (stack.pop(), stack.pop()) {
                            (Some(Number(y)), Some(Number(x))) => {
                                stack.push(Number(binary_op(cur, x, y)))
                            }
                            (Some(_), Some(_)) => return Err(Error::TypeMismatch),
                            _ => return Err(Error::StackUnderflow),
                        }
                    }
                    '=' | '>' | '<' | 'A' | 'O' => match (stack.pop(), stack.pop()) {
                        (Some(Number(y)), Some(Number(x))) => stack.push(Number(
                            if match cur {
                                '=' => x == y,
                                '<' => x < y,
                                '>' => x > y,
                                'A' => x > 0 && y > 0,
                                'O' => x > 0 || y > 0,
                                _ => unreachable!("logic error"),
                            } {
                                1
                            } else {
                                0
                            },
                        )),
                        (Some(_), Some(_)) => return Err(Error::TypeMismatch),
                        _ => return Err(Error::StackUnderflow),
                    },
                    '!' | '~' => match stack.pop() {
                        Some(Number(x)) => stack.push(Number(match cur {
                            '!' if x > 0 => 0,
                            '!' => 1,
                            '~' => !x,
                            _ => unreachable!("logic error"),
                        })),
                        Some(_) => return Err(Error::TypeMismatch),
                        None => return Err(Error::StackUnderflow),
                    },
                    'i' => match (&mparams[0], &mparams[1]) {
                        (&Number(x), &Number(y)) => {
                            mparams[0] = Number(x.wrapping_add(1));
                            mparams[1] = Number(y.wrapping_add(1));
                        }
                        (_, _) => return Err(Error::TypeMismatch),
                    },

                    // printf-style support for %doxXs
                    'd' | 'o' | 'x' | 'X' | 's' => {
                        if let Some(arg) = stack.pop() {
                            let flags = Flags::default();
                            let res = format(arg, FormatOp::from_char(cur), flags)?;
                            output.extend(res);
                        } else {
                            return Err(Error::StackUnderflow);
                        }
                    }
                    ':' | '#' | ' ' | '.' | '0'..='9' => {
                        let mut flags = Flags::default();
                        let mut fstate = FormatState::Flags;
                        read_format_byte(&mut flags, &mut fstate, cur)?;
                        state = FormatPattern(flags, fstate);
                    }

                    // conditionals
                    '?' | ';' => (),
                    't' => match stack.pop() {
                        Some(Number(0)) => state = SeekIfElse(0),
                        Some(Number(_)) => (),
                        Some(_) => return Err(Error::TypeMismatch),
                        None => return Err(Error::StackUnderflow),
                    },
                    'e' => state = SeekIfEnd(0),
                    c => return Err(Error::UnrecognizedFormatOption(c)),
                }
            }
            PushParam => {
                // params are 1-indexed, and `terminfo(5)` gives the range as
                // `%p[1-9]`, so `%p0` is as malformed as `%pa`. Taking the
                // digit on trust indexed `mparams` at `0usize - 1`.
                match cur.to_digit(10) {
                    Some(d @ 1..=9) => stack.push(mparams[d as usize - 1].clone()),
                    _ => return Err(Error::InvalidParameterIndex(cur)),
                }
            }
            SetVar => match cur {
                'A'..='Z' => {
                    if let Some(arg) = stack.pop() {
                        let idx = (cur as u8) - b'A';
                        vars.sta_vars[idx as usize] = arg;
                    } else {
                        return Err(Error::StackUnderflow);
                    }
                }
                'a'..='z' => {
                    if let Some(arg) = stack.pop() {
                        let idx = (cur as u8) - b'a';
                        vars.dyn_vars[idx as usize] = arg;
                    } else {
                        return Err(Error::StackUnderflow);
                    }
                }
                _ => {
                    return Err(Error::InvalidVariableName(cur));
                }
            },
            GetVar => match cur {
                'A'..='Z' => {
                    let idx = (cur as u8) - b'A';
                    stack.push(vars.sta_vars[idx as usize].clone());
                }
                'a'..='z' => {
                    let idx = (cur as u8) - b'a';
                    stack.push(vars.dyn_vars[idx as usize].clone());
                }
                _ => {
                    return Err(Error::InvalidVariableName(cur));
                }
            },
            CharConstant => {
                stack.push(Number(i32::from(c)));
                state = CharClose;
            }
            CharClose => {
                if cur != '\'' {
                    return Err(Error::MalformedCharacterConstant);
                }
            }
            IntConstant(i) => {
                if cur == '}' {
                    stack.push(Number(i));
                    state = Nothing;
                } else if let Some(digit) = cur.to_digit(10) {
                    match i
                        .checked_mul(10)
                        .and_then(|i_ten| i_ten.checked_add(digit as i32))
                    {
                        Some(i) => {
                            state = IntConstant(i);
                            old_state = Nothing;
                        }
                        None => return Err(Error::IntegerConstantOverflow),
                    }
                } else {
                    return Err(Error::MalformedIntegerConstant);
                }
            }
            FormatPattern(ref mut flags, ref mut fstate) => {
                old_state = Nothing;
                match cur {
                    'd' | 'o' | 'x' | 'X' | 's' => {
                        if let Some(arg) = stack.pop() {
                            let res = format(arg, FormatOp::from_char(cur), *flags)?;
                            output.extend(res);
                            // will cause state to go to Nothing
                            old_state = FormatPattern(*flags, *fstate);
                        } else {
                            return Err(Error::StackUnderflow);
                        }
                    }
                    _ => read_format_byte(flags, fstate, cur)?,
                }
            }
            SeekIfElse(level) => {
                if cur == '%' {
                    state = SeekIfElsePercent(level);
                }
                old_state = Nothing;
            }
            SeekIfElsePercent(level) => {
                if cur == ';' {
                    if level == 0 {
                        state = Nothing;
                    } else {
                        state = SeekIfElse(level - 1);
                    }
                } else if cur == 'e' && level == 0 {
                    state = Nothing;
                } else if cur == '?' {
                    state = SeekIfElse(level + 1);
                } else {
                    state = SeekIfElse(level);
                }
            }
            SeekIfEnd(level) => {
                if cur == '%' {
                    state = SeekIfEndPercent(level);
                }
                old_state = Nothing;
            }
            SeekIfEndPercent(level) => {
                if cur == ';' {
                    if level == 0 {
                        state = Nothing;
                    } else {
                        state = SeekIfEnd(level - 1);
                    }
                } else if cur == '?' {
                    state = SeekIfEnd(level + 1);
                } else {
                    state = SeekIfEnd(level);
                }
            }
        }
        if state == old_state {
            state = Nothing;
        }
    }
    Ok(output)
}

/// Apply one binary operator to the two numbers popped for it.
///
/// Three guards live here rather than in the caller's match, all of them
/// about bytes that came out of a terminfo file rather than out of this
/// crate:
///
/// * Division and modulo by zero yield zero. ncurses' `tparm` writes exactly
///   that — `npush(y ? (x / y) : 0)` — because a capability is free to divide
///   by a parameter the caller left at its default.
/// * The arithmetic wraps. C's `int` wraps on every machine terminfo targets
///   and ncurses reports `-2147483648` for `INT_MAX + 1`; a Rust debug build
///   would panic instead.
/// * `i32::MIN / -1` wraps to `i32::MIN`. Here ncurses is no guide: C leaves
///   the case undefined and the real `tparm` takes `SIGFPE` on x86, which is
///   not a behaviour a library can copy.
fn binary_op(op: char, x: i32, y: i32) -> i32 {
    match op {
        '+' => x.wrapping_add(y),
        '-' => x.wrapping_sub(y),
        '*' => x.wrapping_mul(y),
        '/' if y == 0 => 0,
        '/' => x.wrapping_div(y),
        'm' if y == 0 => 0,
        'm' => x.wrapping_rem(y),
        '|' => x | y,
        '&' => x & y,
        '^' => x ^ y,
        _ => unreachable!("logic error"),
    }
}

/// Consume one byte of a printf-style conversion specification, updating the
/// flags and which part of the specification the next byte belongs to.
///
/// The grammar is `printf(3)`'s: `%[flags][width][.precision]` before the
/// conversion character. A leading `0` is therefore the zero-pad *flag*, not
/// the first digit of the width — `%02x` means "width 2, padded with zeros".
/// ncurses gets this for free by copying the whole specification into a
/// `sprintf` format string; this reimplements it.
fn read_format_byte(flags: &mut Flags, fstate: &mut FormatState, cur: char) -> Result<(), Error> {
    match (*fstate, cur) {
        // `terminfo(5)`: "Use a ':' to allow the next character to be a '-'
        // flag, avoiding interpreting '%-' as an operator."
        (FormatState::Flags, ':') => (),
        (FormatState::Flags, '#') => flags.alternate = true,
        (FormatState::Flags, '-') => flags.left = true,
        (FormatState::Flags, '+') => flags.sign = true,
        (FormatState::Flags, ' ') => flags.space = true,
        (FormatState::Flags, '0') => flags.zero = true,
        (FormatState::Flags, '1'..='9') => {
            flags.width = cur as usize - '0' as usize;
            *fstate = FormatState::Width;
        }
        (FormatState::Width, '0'..='9') => {
            flags.width = push_format_digit(flags.width, cur).ok_or(Error::FormatWidthOverflow)?;
        }
        (FormatState::Flags | FormatState::Width, '.') => {
            flags.precision = Some(0);
            *fstate = FormatState::Precision;
        }
        (FormatState::Precision, '0'..='9') => {
            let precision = flags.precision.get_or_insert_default();
            *precision =
                push_format_digit(*precision, cur).ok_or(Error::FormatPrecisionOverflow)?;
        }
        _ => return Err(Error::UnrecognizedFormatOption(cur)),
    }
    Ok(())
}

/// The largest width or precision a conversion specification may carry.
///
/// This is ncurses' own bound, from `parse_format` in
/// `ncurses/tinfo/lib_tparm.c`:
///
/// ```c
/// value = (value * 10) + (*s - '0');
/// if (value > 10000)
///     err = TRUE;
/// ```
///
/// Confirmed against `libtinfo` 6.5+20250216: `tparm("%p1%10000d", 42)` yields
/// 10000 bytes and `tparm("%p1%10001d", 42)` yields `42`.
///
/// Bounding this bounds the whole expansion. A capability's own length caps how
/// many conversions it can contain — a string capability is reachable only
/// through a `u16` offset, so it is at most 32767 bytes — and each conversion
/// now contributes at most 10001 bytes. That is the same product ncurses
/// arrives at, since it grows its output buffer under exactly these two limits.
const MAX_FORMAT_VALUE: usize = 10_000;

/// Accumulate one decimal digit into a width or precision, rejecting a value
/// past [`MAX_FORMAT_VALUE`].
///
/// Unchecked arithmetic is sufficient *because* of the bound: `value` is at
/// most `MAX_FORMAT_VALUE` on entry, so the product cannot exceed 100_009.
///
/// The bound is load-bearing, not cosmetic. A width or precision below
/// `usize::MAX` — which is all the previous `checked_mul` asked for — still
/// reached two places that cannot take one:
///
/// - The numeric formatter panics with "Formatting argument out of range"
///   from a precision of 65536, because Rust packs a formatting width into a
///   `u16`. `%p1%.65536d` is eleven bytes.
/// - [`pad`] hands `flags.width` to `Vec::with_capacity`, so `%p1%99999999999d`
///   aborts the process on a failed 99999999999-byte allocation.
///
/// Both inputs are a compiled terminfo entry, which is a file we did not write,
/// found wherever the search path led.
///
/// Past the bound we diverge from ncurses deliberately. It discards the flags
/// and renders a bare `%d`, emitting a control sequence the database did not
/// describe; `tparm(3)` has no error channel its callers check, so degrading is
/// the only move available to it. [`expand`] returns a `Result` that this
/// crate's caller does check, so it reports the capability as malformed rather
/// than quietly writing the wrong bytes to a terminal.
fn push_format_digit(value: usize, cur: char) -> Option<usize> {
    let next = value * 10 + (cur as usize - '0' as usize);
    (next <= MAX_FORMAT_VALUE).then_some(next)
}

#[derive(Copy, PartialEq, Clone, Default)]
struct Flags {
    width: usize,
    /// `None` when no dot appeared; `Some(0)` for both `.` and `.0`.
    precision: Option<usize>,
    alternate: bool,
    left: bool,
    sign: bool,
    space: bool,
    /// `printf(3)`'s `0` flag: fill the width with zeros rather than spaces.
    zero: bool,
}

#[derive(Copy, Clone)]
enum FormatOp {
    Digit,
    Octal,
    Hex,
    UpperHex,
    String,
}

impl FormatOp {
    fn from_char(c: char) -> FormatOp {
        use self::FormatOp::*;
        match c {
            'd' => Digit,
            'o' => Octal,
            'x' => Hex,
            'X' => UpperHex,
            's' => String,
            _ => panic!("bad FormatOp char"),
        }
    }
}

fn format(val: Param, op: FormatOp, flags: Flags) -> Result<Vec<u8>, Error> {
    match val {
        Number(d) => {
            let s = match op {
                FormatOp::Digit => {
                    if d == 0 && flags.precision == Some(0) {
                        if flags.sign {
                            "+".to_owned()
                        } else if flags.space {
                            " ".to_owned()
                        } else {
                            String::new()
                        }
                    } else {
                        let precision = flags.precision.unwrap_or_default();
                        // C doesn't take sign into account in precision calculation.
                        if flags.sign {
                            format!("{:+01$}", d, precision + 1)
                        } else if d < 0 {
                            format!("{:01$}", d, precision + 1)
                        } else if flags.space {
                            format!(" {:01$}", d, precision)
                        } else {
                            format!("{:01$}", d, precision)
                        }
                    }
                }
                FormatOp::Octal => {
                    let s = if d == 0 && flags.precision == Some(0) {
                        String::new()
                    } else {
                        format!("{:01$o}", d, flags.precision.unwrap_or_default())
                    };
                    // The alternate form guarantees a leading zero, it does
                    // not add one to a value that already has it: C and
                    // ncurses render `%#o` of 0 as "0", not "00".
                    if flags.alternate && !s.starts_with('0') {
                        format!("0{s}")
                    } else {
                        s
                    }
                }
                FormatOp::Hex => {
                    if d == 0 && flags.precision == Some(0) {
                        String::new()
                    } else if flags.alternate && d != 0 {
                        format!("0x{:01$x}", d, flags.precision.unwrap_or_default())
                    } else {
                        format!("{:01$x}", d, flags.precision.unwrap_or_default())
                    }
                }
                FormatOp::UpperHex => {
                    if d == 0 && flags.precision == Some(0) {
                        String::new()
                    } else if flags.alternate && d != 0 {
                        format!("0X{:01$X}", d, flags.precision.unwrap_or_default())
                    } else {
                        format!("{:01$X}", d, flags.precision.unwrap_or_default())
                    }
                }
                FormatOp::String => return Err(Error::TypeMismatch),
            }
            .into_bytes();
            // C ignores the `0` flag once a precision is given, since the
            // precision has already decided how many digits there are.
            let zero_after = (flags.zero && flags.precision.is_none()).then(|| number_prefix(&s));
            Ok(pad(s, flags, zero_after))
        }
        Words(s) => match op {
            FormatOp::String => {
                let mut s = s.into_bytes();
                if let Some(precision) = flags.precision {
                    s.truncate(precision);
                }
                // `0` is defined for the numeric conversions only, and glibc
                // pads `%05s` with spaces, so a string never zero-pads.
                Ok(pad(s, flags, None))
            }
            _ => Err(Error::TypeMismatch),
        },
    }
}

/// How many leading bytes of an already-formatted number are its sign or base
/// prefix, which zero padding has to go *after* rather than before: C renders
/// `%#010x` of 15 as `0x0000000f` and `%05d` of -7 as `-0007`.
///
/// Only [`format`]'s own output reaches this, so the two-byte case cannot be
/// anything but the alternate form's `0x`/`0X` — no other conversion here
/// emits an `x` at all, and the octal form's forced leading zero is a digit
/// rather than a prefix, which is exactly how C pads it.
fn number_prefix(s: &[u8]) -> usize {
    match s {
        [b'+' | b'-' | b' ', ..] => 1,
        [b'0', b'x' | b'X', ..] => 2,
        _ => 0,
    }
}

/// Bring `s` up to `flags.width`, in `printf(3)`'s order.
///
/// `zero_after` is `Some(prefix)` when the `0` flag applies, `prefix` being
/// the length of the sign or base prefix the zeros go behind. A left
/// justification wins over it, as it does in C.
fn pad(s: Vec<u8>, flags: Flags, zero_after: Option<usize>) -> Vec<u8> {
    if flags.width <= s.len() {
        return s;
    }
    let n = flags.width - s.len();
    let mut out = Vec::with_capacity(flags.width);
    if flags.left {
        out.extend(s);
        out.extend(repeat_n(b' ', n));
    } else if let Some(prefix) = zero_after {
        out.extend_from_slice(&s[..prefix]);
        out.extend(repeat_n(b'0', n));
        out.extend_from_slice(&s[prefix..]);
    } else {
        out.extend(repeat_n(b' ', n));
        out.extend(s);
    }
    out
}

#[cfg(test)]
mod test {
    use super::Param::{self, Number, Words};
    use super::{Error, Variables, expand};
    use std::result::Result::Ok;

    #[test]
    fn test_basic_setabf() {
        let s = b"\\E[48;5;%p1%dm";
        assert_eq!(
            expand(s, &[Number(1)], &mut Variables::new()).unwrap(),
            "\\E[48;5;1m".bytes().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_multiple_int_constants() {
        assert_eq!(
            expand(b"%{1}%{2}%d%d", &[], &mut Variables::new()).unwrap(),
            "21".bytes().collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_op_i() {
        let mut vars = Variables::new();
        assert_eq!(
            expand(
                b"%p1%d%p2%d%p3%d%i%p1%d%p2%d%p3%d",
                &[Number(1), Number(2), Number(3)],
                &mut vars
            ),
            Ok("123233".bytes().collect::<Vec<_>>())
        );
        assert_eq!(
            expand(b"%p1%d%p2%d%i%p1%d%p2%d", &[], &mut vars),
            Ok("0011".bytes().collect::<Vec<_>>())
        );
    }

    #[test]
    fn test_param_stack_failure_conditions() {
        let mut varstruct = Variables::new();
        let vars = &mut varstruct;
        fn get_res(
            fmt: &str,
            cap: &str,
            params: &[Param],
            vars: &mut Variables,
        ) -> Result<Vec<u8>, super::Error> {
            let mut u8v: Vec<_> = fmt.bytes().collect();
            u8v.extend(cap.as_bytes().iter().cloned());
            expand(&u8v, params, vars)
        }

        let caps = ["%d", "%c", "%s", "%Pa", "%l", "%!", "%~"];
        for &cap in &caps {
            let res = get_res("", cap, &[], vars);
            assert!(
                res.is_err(),
                "Op {} succeeded incorrectly with 0 stack entries",
                cap
            );
            let p = if cap == "%s" || cap == "%l" {
                Words("foo".to_owned())
            } else {
                Number(97)
            };
            let res = get_res("%p1", cap, &[p], vars);
            assert!(
                res.is_ok(),
                "Op {} failed with 1 stack entry: {}",
                cap,
                res.err().unwrap()
            );
        }
        let caps = ["%+", "%-", "%*", "%/", "%m", "%&", "%|", "%A", "%O"];
        for &cap in &caps {
            let res = expand(cap.as_bytes(), &[], vars);
            assert!(
                res.is_err(),
                "Binop {} succeeded incorrectly with 0 stack entries",
                cap
            );
            let res = get_res("%{1}", cap, &[], vars);
            assert!(
                res.is_err(),
                "Binop {} succeeded incorrectly with 1 stack entry",
                cap
            );
            let res = get_res("%{1}%{2}", cap, &[], vars);
            assert!(
                res.is_ok(),
                "Binop {} failed with 2 stack entries: {}",
                cap,
                res.err().unwrap()
            );
        }
    }

    #[test]
    fn test_push_bad_param() {
        assert!(expand(b"%pa", &[], &mut Variables::new()).is_err());
    }

    #[test]
    fn test_comparison_ops() {
        let v = [
            ('<', [1u8, 0u8, 0u8]),
            ('=', [0u8, 1u8, 0u8]),
            ('>', [0u8, 0u8, 1u8]),
        ];
        for &(op, bs) in &v {
            let s = format!("%{{1}}%{{2}}%{}%d", op);
            let res = expand(s.as_bytes(), &[], &mut Variables::new());
            assert!(res.is_ok(), "{}", res.err().unwrap());
            assert_eq!(res.unwrap(), vec![b'0' + bs[0]]);
            let s = format!("%{{1}}%{{1}}%{}%d", op);
            let res = expand(s.as_bytes(), &[], &mut Variables::new());
            assert!(res.is_ok(), "{}", res.err().unwrap());
            assert_eq!(res.unwrap(), vec![b'0' + bs[1]]);
            let s = format!("%{{2}}%{{1}}%{}%d", op);
            let res = expand(s.as_bytes(), &[], &mut Variables::new());
            assert!(res.is_ok(), "{}", res.err().unwrap());
            assert_eq!(res.unwrap(), vec![b'0' + bs[2]]);
        }
    }

    #[test]
    fn test_conditionals() {
        let mut vars = Variables::new();
        let s = b"\\E[%?%p1%{8}%<%t3%p1%d%e%p1%{16}%<%t9%p1%{8}%-%d%e38;5;%p1%d%;m";
        let res = expand(s, &[Number(1)], &mut vars);
        assert!(res.is_ok(), "{}", res.err().unwrap());
        assert_eq!(res.unwrap(), "\\E[31m".bytes().collect::<Vec<_>>());
        let res = expand(s, &[Number(8)], &mut vars);
        assert!(res.is_ok(), "{}", res.err().unwrap());
        assert_eq!(res.unwrap(), "\\E[90m".bytes().collect::<Vec<_>>());
        let res = expand(s, &[Number(42)], &mut vars);
        assert!(res.is_ok(), "{}", res.err().unwrap());
        assert_eq!(res.unwrap(), "\\E[38;5;42m".bytes().collect::<Vec<_>>());
    }

    #[test]
    fn test_format() {
        let mut varstruct = Variables::new();
        let vars = &mut varstruct;
        assert_eq!(
            expand(
                b"%p1%s%p2%2s%p3%2s%p4%.2s",
                &[
                    Words("foo".to_owned()),
                    Words("foo".to_owned()),
                    Words("f".to_owned()),
                    Words("foo".to_owned())
                ],
                vars
            ),
            Ok("foofoo ffo".bytes().collect::<Vec<_>>())
        );
        assert_eq!(
            expand(b"%p1%:-4.2s", &[Words("foo".to_owned())], vars),
            Ok("fo  ".bytes().collect::<Vec<_>>())
        );

        assert_eq!(
            expand(b"%p1%d%p1%.3d%p1%5d%p1%:+d", &[Number(1)], vars),
            Ok("1001    1+1".bytes().collect::<Vec<_>>())
        );
        assert_eq!(
            expand(
                b"%p1%o%p1%#o%p2%6.4x%p2%#6.4X",
                &[Number(15), Number(27)],
                vars
            ),
            Ok("17017  001b0X001B".bytes().collect::<Vec<_>>())
        );
        assert_eq!(
            expand(
                b"%p1%.5d%p1% .5d%p1%:+.5d%p2%.5d%p2% .5d%p2%:+.5d",
                &[Number(15), Number(-15)],
                vars
            ),
            Ok("00015 00015+00015-00015-00015-00015"
                .bytes()
                .collect::<Vec<_>>())
        );
    }

    /// The width and precision bound sits exactly where ncurses puts it.
    ///
    /// Every expectation here is what `libtinfo` 6.5+20250216 printed for the
    /// same capability, so the two agree byte for byte right up to the edge.
    #[test]
    fn test_format_size_boundary_matches_ncurses() {
        let vars = &mut Variables::new();

        let at = expand(b"%p1%10000d", &[Number(42)], vars).unwrap();
        assert_eq!(at.len(), 10000);
        assert_eq!(&at[9998..], b"42");
        assert!(at[..9998].iter().all(|&b| b == b' '));

        let at = expand(b"%p1%.10000d", &[Number(42)], vars).unwrap();
        assert_eq!(at.len(), 10000);
        assert_eq!(&at[9998..], b"42");
        assert!(at[..9998].iter().all(|&b| b == b'0'));

        // One past it, ncurses discards the flags and renders `42`. We report
        // the capability instead, for the reason given on `push_format_digit`.
        assert!(matches!(
            expand(b"%p1%10001d", &[Number(42)], vars),
            Err(Error::FormatWidthOverflow)
        ));
        assert!(matches!(
            expand(b"%p1%.10001d", &[Number(42)], vars),
            Err(Error::FormatPrecisionOverflow)
        ));
    }

    /// A width or precision that fits in a `usize` is not thereby safe.
    ///
    /// Before the bound, `%p1%.65536d` panicked inside `format!` — Rust packs a
    /// formatting width into a `u16` — and `%p1%99999999999d` aborted the
    /// process on a failed 99999999999-byte `Vec::with_capacity`. Each is a
    /// dozen bytes of a compiled terminfo entry.
    #[test]
    fn test_format_size_is_bounded() {
        let vars = &mut Variables::new();
        let caps: [&[u8]; 6] = [
            b"%p1%.65536d",
            b"%p1%65536d",
            b"%p1%99999999999d",
            b"%p1%.99999999999d",
            b"%p1%18446744073709551615d",
            // More digits than a usize holds, which the previous `checked_mul`
            // caught and nothing else did.
            b"%p1%99999999999999999999999999d",
        ];
        for cap in caps {
            assert!(
                expand(cap, &[Number(42)], vars).is_err(),
                "{} was accepted",
                String::from_utf8_lossy(cap),
            );
        }
    }
}
