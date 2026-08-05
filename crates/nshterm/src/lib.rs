// Copyright 2012-2019 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Terminfo database access: ncurses-compatible entry discovery, compiled
//! `term(5)` parsing, and parameterised capability expansion.
//!
//! A terminal type resolves to a [`TermInfo`] — three maps keyed by terminfo
//! capname, one each for booleans, numbers and strings — through
//! [`TermInfo::from_env`], [`TermInfo::from_name`] or [`TermInfo::from_path`].
//! String capabilities are stored raw; [`parm::expand`] substitutes their
//! parameters.
//!
//! ```no_run
//! use nshterm::TermInfo;
//! use nshterm::parm::{Param, Variables, expand};
//!
//! let ti = TermInfo::from_env()?;
//! let cup = ti.strings.get("cup").expect("no cursor_address");
//! let bytes = expand(cup, &[Param::Number(4), Param::Number(12)], &mut Variables::new())?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Derivation
//!
//! This crate is a derived work of the [`term`] crate, version **1.2.1**,
//! copyright The Rust Project Developers and Steven Allen, dual-licensed
//! `MIT OR Apache-2.0`. Both licence texts travel with it as `LICENSE-MIT`
//! and `LICENSE-APACHE`, and every file carried over keeps its upstream
//! copyright header. It is **not** relicensed: `nshterm` is
//! `MIT OR Apache-2.0` too, and differs from the rest of this workspace,
//! which is BSD-3-Clause.
//!
//! `term` is a terminal *formatting* library that happens to contain a
//! terminfo parser. Upstream has been unmaintained since 2018
//! ([Stebalien/term#93], "[LFM] Looking For Maintainer"), and the ecosystem's
//! successor for its users, `termcolor`, does not read terminfo at all. The
//! terminfo half is what we depend on, so we took it.
//!
//! What changed, relative to `term` 1.2.1:
//!
//! * **Kept**: `terminfo::TermInfo` and its constructors, `terminfo::parm`,
//!   `terminfo::parser::compiled`, `terminfo::parser::names`,
//!   `terminfo::searcher`.
//! * **Dropped**: the `Terminal` trait, `TerminfoTerminal`, `Attr`, the
//!   `color` module, the `stdout`/`stderr` constructors, and the Windows
//!   console backend (`win`) with the `windows-sys` dependency it carried.
//!   The `#[cfg(windows)]` probe in `TermInfo::from_env` that called into
//!   that backend went with it; the `MSYSCON` check beside it, which is pure
//!   environment inspection, stayed.
//! * **Flattened**: everything sat under a `terminfo` module of a crate whose
//!   root was the formatting API. With the formatting API gone the module was
//!   pure stutter, so its contents are the crate root here —
//!   `term::terminfo::parm` is [`parm`], `term::terminfo::TermInfo` is
//!   [`TermInfo`].
//! * **Merged**: `term` had two types named `Error`, one at the crate root
//!   and one for parse failures under `terminfo`. Flattening collided them,
//!   so this crate has a single [`Error`]; see its docs.
//!
//! [`term`]: https://crates.io/crates/term/1.2.1
//! [Stebalien/term#93]: https://github.com/Stebalien/term/issues/93

#![deny(missing_docs)]
#![deny(rust_2018_idioms)]

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::io::prelude::*;
use std::path::Path;

use self::Error::*;
use self::parm::{Param, Variables, expand};
use self::parser::compiled::parse;
use self::searcher::get_dbpath_for_term;

pub mod parm;
pub mod searcher;

/// `TermInfo` format parsing.
pub mod parser {
    /// ncurses-compatible compiled terminfo format parsing (term(5))
    pub mod compiled;
    mod names;
}

/// Returns true if the named terminal supports basic ANSI escape codes.
fn is_ansi(name: &str) -> bool {
    // SORTED! We binary search this.
    static ANSI_TERM_PREFIX: &[&str] = &[
        "Eterm", "ansi", "eterm", "iterm", "konsole", "linux", "mrxvt", "msyscon", "rxvt",
        "screen", "tmux", "xterm",
    ];
    match ANSI_TERM_PREFIX.binary_search(&name) {
        Ok(_) => true,
        Err(0) => false,
        Err(idx) => name.starts_with(ANSI_TERM_PREFIX[idx - 1]),
    }
}

/// A parsed terminfo database entry.
#[derive(Debug, Clone)]
pub struct TermInfo {
    /// Names for the terminal
    pub names: Vec<String>,
    /// Map of capability name to boolean value
    pub bools: HashMap<&'static str, bool>,
    /// Map of capability name to numeric value
    pub numbers: HashMap<&'static str, u32>,
    /// Map of capability name to raw (unexpanded) string
    pub strings: HashMap<&'static str, Vec<u8>>,
}

impl TermInfo {
    /// Create a `TermInfo` based on current environment.
    pub fn from_env() -> Result<TermInfo> {
        let term_var = env::var("TERM").ok();
        let term_name = term_var.as_deref().or_else(|| {
            env::var("MSYSCON").ok().and_then(|s| {
                if s == "mintty.exe" {
                    Some("msyscon")
                } else {
                    None
                }
            })
        });

        if let Some(term_name) = term_name {
            TermInfo::from_name(term_name)
        } else {
            Err(TermUnset)
        }
    }

    /// Create a `TermInfo` for the named terminal.
    pub fn from_name(name: &str) -> Result<TermInfo> {
        if let Some(path) = get_dbpath_for_term(name) {
            match TermInfo::from_path(path) {
                Ok(term) => return Ok(term),
                // Skip IO Errors (e.g., permission denied).
                Err(Io(_)) => {}
                // Don't ignore malformed terminfo databases.
                Err(e) => return Err(e),
            }
        }
        // Basic ANSI fallback terminal.
        if is_ansi(name) {
            let mut strings = HashMap::new();
            strings.insert("sgr0", b"\x1B[0m".to_vec());
            strings.insert("bold", b"\x1B[1m".to_vec());
            strings.insert("setaf", b"\x1B[3%p1%dm".to_vec());
            strings.insert("setab", b"\x1B[4%p1%dm".to_vec());

            let mut numbers = HashMap::new();
            numbers.insert("colors", 8);

            Ok(TermInfo {
                names: vec![name.to_owned()],
                bools: HashMap::new(),
                numbers,
                strings,
            })
        } else {
            Err(TerminfoEntryNotFound)
        }
    }

    /// Parse the given `TermInfo`.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<TermInfo> {
        Self::_from_path(path.as_ref())
    }
    // Keep the metadata small
    // (That is, this uses a &Path so that this function need not be instantiated
    // for every type
    // which implements AsRef<Path>. One day, if/when rustc is a bit smarter, it
    // might do this for
    // us. Alas. )
    fn _from_path(path: &Path) -> Result<TermInfo> {
        let file = File::open(path).map_err(Io)?;
        let mut reader = BufReader::new(file);
        parse(&mut reader, false)
    }

    /// Read a `TermInfo` out of an already-open compiled entry.
    pub fn from_reader<R: Read>(mut reader: R) -> Result<TermInfo> {
        parse(&mut reader, false)
    }

    /// Retrieve a capability `cmd` and expand it with `params`, writing result to `out`.
    pub fn apply_cap(&self, cmd: &str, params: &[Param], out: &mut dyn io::Write) -> Result<()> {
        match self.strings.get(cmd) {
            Some(cmd) => match expand(cmd, params, &mut Variables::new()) {
                Ok(s) => {
                    out.write_all(&s)?;
                    Ok(())
                }
                Err(e) => Err(e.into()),
            },
            None => Err(NotSupported),
        }
    }

    /// Write the reset string to `out`.
    pub fn reset(&self, out: &mut dyn io::Write) -> Result<()> {
        // are there any terminals that have color/attrs and not sgr0?
        // Try falling back to sgr, then op
        let cmd = match [
            ("sgr0", &[] as &[Param]),
            ("sgr", &[Param::Number(0)]),
            ("op", &[]),
        ]
        .iter()
        .filter_map(|&(cap, params)| self.strings.get(cap).map(|c| (c, params)))
        .next()
        {
            Some((op, params)) => expand(op, params, &mut Variables::new())?,
            None => return Err(NotSupported),
        };
        out.write_all(&cmd)?;
        Ok(())
    }
}

/// An error from looking up, reading or applying a terminfo entry.
///
/// `term` split this in two: a crate-root `Error` for lookup and I/O, and a
/// `terminfo::Error` for compiled-format parse failures, reached through the
/// root's `TerminfoParsing` variant. Flattening the `terminfo` module put both
/// at the crate root, so they are one enum here and the parse failures are
/// variants in their own right rather than a nested type. `term`'s
/// `CursorDestinationInvalid` and `ColorOutOfRange` are gone with the
/// `Terminal` implementations that were their only source.
///
/// [`parm::Error`] stays separate: it is the error of a pure function over
/// bytes, it is `Eq`, and nothing about expanding a `%` sequence needs to
/// know about files. It reaches this enum through `ParameterizedExpansion`.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Indicates an error from any underlying IO
    Io(io::Error),
    /// Indicates an error expanding a parameterized string from the terminfo database
    ParameterizedExpansion(parm::Error),
    /// Indicates that the terminal does not support the requested operation.
    NotSupported,
    /// Indicates that the `TERM` environment variable was unset, and thus we were unable to detect
    /// which terminal we should be using.
    TermUnset,
    /// Indicates that we were unable to find a terminfo entry for the requested terminal.
    TerminfoEntryNotFound,
    /// The "magic" number at the start of the file was wrong.
    ///
    /// It should be `0x11A` (16bit numbers) or `0x21e` (32bit numbers)
    BadMagic(u16),
    /// The names in the file were not valid UTF-8.
    ///
    /// In theory these should only be ASCII, but to work with the Rust `str` type, we treat them
    /// as UTF-8. This is valid, except when a terminfo file decides to be invalid. This hasn't
    /// been encountered in the wild.
    NotUtf8(::std::str::Utf8Error),
    /// The names section of the file was empty
    ShortNames,
    /// More boolean parameters are present in the file than this crate knows how to interpret.
    TooManyBools,
    /// More number parameters are present in the file than this crate knows how to interpret.
    TooManyNumbers,
    /// More string parameters are present in the file than this crate knows how to interpret.
    TooManyStrings,
    /// The length of some field was not >= -1.
    InvalidLength,
    /// The names table was missing a trailing null terminator.
    NamesMissingNull,
    /// The strings table was missing a trailing null terminator.
    StringsMissingNull,
}

// manually implemented because std::io::Error does not implement Eq/PartialEq
impl std::cmp::PartialEq for Error {
    fn eq(&self, other: &Error) -> bool {
        match self {
            Io(_) => false,
            ParameterizedExpansion(a) => matches!(other, ParameterizedExpansion(b) if a == b),
            NotSupported => matches!(other, NotSupported),
            TermUnset => matches!(other, TermUnset),
            TerminfoEntryNotFound => matches!(other, TerminfoEntryNotFound),
            BadMagic(a) => matches!(other, BadMagic(b) if a == b),
            NotUtf8(a) => matches!(other, NotUtf8(b) if a == b),
            ShortNames => matches!(other, ShortNames),
            TooManyBools => matches!(other, TooManyBools),
            TooManyNumbers => matches!(other, TooManyNumbers),
            TooManyStrings => matches!(other, TooManyStrings),
            InvalidLength => matches!(other, InvalidLength),
            NamesMissingNull => matches!(other, NamesMissingNull),
            StringsMissingNull => matches!(other, StringsMissingNull),
        }
    }
}

/// The canonical `Result` type using this crate's Error type.
pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Io(io) => io.fmt(f),
            ParameterizedExpansion(e) => e.fmt(f),
            NotSupported => f.write_str("operation not supported by the terminal"),
            TermUnset => {
                f.write_str("TERM environment variable unset, unable to detect a terminal")
            }
            TerminfoEntryNotFound => {
                f.write_str("could not find a terminfo entry for this terminal")
            }
            BadMagic(v) => write!(f, "bad magic number {v:x} in terminfo header"),
            NotUtf8(e) => e.fmt(f),
            ShortNames => f.write_str("no names exposed, need at least one"),
            TooManyBools => f.write_str("more boolean properties than nshterm knows about"),
            TooManyNumbers => f.write_str("more number properties than nshterm knows about"),
            TooManyStrings => f.write_str("more string properties than nshterm knows about"),
            InvalidLength => f.write_str("invalid length field value, must be >= -1"),
            NamesMissingNull => f.write_str("names table missing NUL terminator"),
            StringsMissingNull => f.write_str("string table missing NUL terminator"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Io(io) => Some(io),
            ParameterizedExpansion(e) => Some(e),
            NotUtf8(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Error> for io::Error {
    fn from(err: Error) -> io::Error {
        let kind = match &err {
            Io(e) => e.kind(),
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, err)
    }
}

impl std::convert::From<io::Error> for Error {
    fn from(val: io::Error) -> Self {
        Io(val)
    }
}

impl std::convert::From<parm::Error> for Error {
    fn from(val: parm::Error) -> Self {
        ParameterizedExpansion(val)
    }
}

impl std::convert::From<::std::string::FromUtf8Error> for Error {
    fn from(v: ::std::string::FromUtf8Error) -> Self {
        NotUtf8(v.utf8_error())
    }
}
