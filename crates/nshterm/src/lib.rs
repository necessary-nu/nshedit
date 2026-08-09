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
//! A terminal type resolves to a [`TermInfo`] — the booleans, numbers and
//! strings one database entry defines — through [`TermInfo::from_env`],
//! [`TermInfo::from_name`] or [`TermInfo::from_path`]. Capabilities are read
//! back by name through [`TermInfo::string`] and its siblings, naming each
//! one in the vocabulary it is being asked for; string capabilities are
//! stored raw, and [`parm::expand`] substitutes their parameters.
//!
//! ```no_run
//! use nshterm::{CapabilityName, EnvironmentTrust, TermInfo};
//! use nshterm::parm::{Param, Variables, expand};
//!
//! let ti = TermInfo::from_env(EnvironmentTrust::for_process())?;
//! let cup = ti.string(CapabilityName::Terminfo("cup")).expect("no cursor_address");
//! let bytes = expand(&cup, &[Param::Number(4), Param::Number(12)], &mut Variables::new())?;
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
//! * **Fixed**: `term`'s expander recognised `$<...>` padding delays and threw
//!   them away, so every parameterised capability silently lost its timing —
//!   and it entered that delay state on a bare `$` rather than on `$<`, so an
//!   incidental `$` swallowed the text up to the next `>` and a trailing one
//!   vanished. Padding now survives expansion verbatim, which is what ncurses'
//!   `tparm(3)` does and what leaves it available to a `tputs(3)`; see
//!   [`parm::expand`].
//! * **Fixed**: six ways a terminfo entry could panic the process. A compiled
//!   entry is a file this crate did not write — [`TermInfo::from_env`] opens
//!   whatever the terminfo search path resolves to — so every one of these was
//!   reachable input, and a panic in a library is a denial of service in every
//!   consumer of it. In [`parm::expand`]: `%p0` indexed a nine-element array
//!   at `0usize - 1`; `%/` and `%m` divided without guarding a zero divisor,
//!   where ncurses writes `npush(y ? (x / y) : 0)`; and `%+`, `%-`, `%*` and
//!   `%i` overflowed, where C's `int` wraps and ncurses duly reports
//!   `-2147483648` for `INT_MAX + 1`. `%p0` is now an
//!   [`InvalidParameterIndex`][parm::Error::InvalidParameterIndex], the
//!   divisions yield zero, and the arithmetic wraps — including `i32::MIN /
//!   -1`, where ncurses is no guide because the real `tparm` takes `SIGFPE`.
//!   In [`parser::compiled`]: the string table was read with `read_to_end` and
//!   then sliced against the length the header *declared*, so a file ending
//!   inside its own string table went out of range, as did any offset pointing
//!   past the table. The table is now read with `read_exact`, making a short
//!   one the same clean [`Error::Io`] as every other short read in the format,
//!   and a stray offset is [`Error::StringOffsetOutOfRange`].
//! * **Fixed**: two printf-style conversions that `terminfo(5)` defines by
//!   reference to `printf(3)` — ncurses hands the whole specification to
//!   `sprintf`, so C is the spec. The `0` flag did not exist: `Flags` had no
//!   member for it and the leading zero folded into the width, so `%02x` of 15
//!   padded with a space. That is live on the `linux` console, whose shipped
//!   `initc` is `…%{1000}%/%02x…`; a colour channel below `0x10` put a space
//!   inside an OSC payload. And `%#o` of 0 rendered as `00`, because the
//!   alternate form prepended a zero unconditionally rather than only when the
//!   value did not already start with one.
//!
//! [`term`]: https://crates.io/crates/term/1.2.1
//! [Stebalien/term#93]: https://github.com/Stebalien/term/issues/93

// Every lint this crate and the workspace select is answered in the source
// rather than suppressed at the item that raised it.
// [spec:nshedit:req:workspace.lint-policy+1]
#![deny(missing_docs)]
#![deny(rust_2018_idioms)]

use std::borrow::Cow;
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
use self::searcher::database_path;

pub use self::searcher::EnvironmentTrust;

pub mod parm;
pub mod searcher;
mod termcap;

/// `TermInfo` format parsing.
pub mod parser {
    /// ncurses-compatible compiled terminfo format parsing (term(5))
    pub mod compiled;
    /// The capability name tables, and the termcap-to-terminfo lookup.
    ///
    /// Public because the lookup is the point: `settc`, `echotc` and
    /// `EL_SETTC` take a name a user typed, and what users type is termcap.
    /// A crate-private table could not answer that.
    pub mod names;
}

// [spec:nshedit:req:terminal.typed-api]
/// Which of terminfo's two capability-name tables a compiled entry is read
/// with.
///
/// The compiled format numbers its capabilities; the table decides what those
/// numbers are called afterwards, and every lookup on the resulting entry is
/// in the vocabulary the table chose.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NameTable {
    /// The short capnames `terminfo(5)` documents: `bw`, `cup`, `xenl`.
    Capnames,
    /// The long C variable names: `auto_left_margin`, `cursor_address`.
    VariableNames,
}

// [spec:nshedit:req:terminal.typed-api]
/// The vocabulary a capability is being named in at a lookup.
///
/// terminfo and termcap name the same capabilities differently, and a name
/// alone does not say which of the two it is — `cr` is a terminfo capname and
/// also a termcap code, for different capabilities. Callers say which they
/// hold, and a termcap lookup gets termcap's projections as well as its names.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CapabilityName<'a> {
    /// A terminfo capname, as the database spells it.
    Terminfo(&'a str),
    /// A termcap two-letter code, as a `.editrc` or a user spells it.
    Termcap(&'a str),
}

// [spec:nshedit:req:terminal.typed-api]
/// A parsed terminfo database entry.
///
/// The capabilities are private: an entry answers what a terminal can do, and
/// a caller that could reach into the tables could also invent a capability
/// the database never defined, or key one under a name no table knows.
#[derive(Debug, Clone)]
pub struct TermInfo {
    names: Vec<String>,
    bools: HashMap<&'static str, bool>,
    numbers: HashMap<&'static str, u32>,
    strings: HashMap<&'static str, Vec<u8>>,
}

/// Assembles a [`TermInfo`] from capabilities a caller already holds.
///
/// The parsers build an entry out of a compiled file; this is the other
/// source — a caller that resolved capabilities by some other route and needs
/// them in the same shape. Capability names are `'static` because they are
/// the ones the name tables define.
#[derive(Debug, Clone, Default)]
pub struct TermInfoBuilder {
    names: Vec<String>,
    bools: HashMap<&'static str, bool>,
    numbers: HashMap<&'static str, u32>,
    strings: HashMap<&'static str, Vec<u8>>,
}

impl TermInfoBuilder {
    /// Add one name for the terminal, longest-lived first.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.names.push(name.into());
        self
    }

    /// Define one boolean capability.
    #[must_use]
    pub fn boolean(mut self, capname: &'static str, value: bool) -> Self {
        self.bools.insert(capname, value);
        self
    }

    /// Define one numeric capability.
    #[must_use]
    pub fn number(mut self, capname: &'static str, value: u32) -> Self {
        self.numbers.insert(capname, value);
        self
    }

    /// Define one string capability, raw and unexpanded.
    #[must_use]
    pub fn string(mut self, capname: &'static str, value: impl Into<Vec<u8>>) -> Self {
        self.strings.insert(capname, value.into());
        self
    }

    /// The assembled entry.
    #[must_use]
    pub fn build(self) -> TermInfo {
        let Self {
            names,
            bools,
            numbers,
            strings,
        } = self;
        TermInfo {
            names,
            bools,
            numbers,
            strings,
        }
    }
}

impl TermInfo {
    /// Create a `TermInfo` for the terminal type the environment names.
    ///
    /// `environment` decides whether the environment may be read at all: a
    /// process running with privileges its invoker does not have takes its
    /// terminal type from nowhere, because the type selects the escape
    /// sequences that will be written to the terminal.
    pub fn from_env(environment: EnvironmentTrust) -> Result<TermInfo> {
        if environment == EnvironmentTrust::Ignored {
            return Err(TermUnset);
        }
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
    ///
    /// A database directory or entry that exists but cannot be read is
    /// reported as the [`Io`][Error::Io] failure it is, rather than as a
    /// terminal the database does not describe.
    pub fn from_name(name: &str) -> Result<TermInfo> {
        match database_path(name, EnvironmentTrust::for_process())? {
            Some(path) => TermInfo::from_path(path),
            None => Err(TerminfoEntryNotFound),
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
        parse(&mut reader, NameTable::Capnames)
    }

    /// Read a `TermInfo` out of an already-open compiled entry.
    pub fn from_reader<R: Read>(mut reader: R) -> Result<TermInfo> {
        parse(&mut reader, NameTable::Capnames)
    }

    /// The names this entry answers to, longest-lived first.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Whether this terminal has the named boolean capability.
    ///
    /// A capability the entry does not define is absent, which for a boolean
    /// is the same as false — the distinction is kept so a caller can tell an
    /// unknown name from a defined one.
    #[must_use]
    pub fn boolean(&self, name: CapabilityName<'_>) -> Option<bool> {
        self.bools.get(self.capname(name)?).copied()
    }

    /// The value of the named numeric capability.
    #[must_use]
    pub fn number(&self, name: CapabilityName<'_>) -> Option<u32> {
        self.numbers.get(self.capname(name)?).copied()
    }

    /// The raw, unexpanded bytes of the named string capability.
    ///
    /// Most termcap codes are a plain namespace translation, and the answer is
    /// borrowed. `me` is not: termcap has no `sgr` operation and therefore
    /// requires its reset string to preserve alternate-character-set state, so
    /// a termcap lookup for it is a projection ncurses makes as well, and it
    /// is owned.
    #[must_use]
    pub fn string(&self, name: CapabilityName<'_>) -> Option<Cow<'_, [u8]>> {
        match name {
            CapabilityName::Terminfo(capname) => self
                .strings
                .get(capname)
                .map(|value| Cow::Borrowed(&value[..])),
            CapabilityName::Termcap(code) => termcap::string(self, code).map(Cow::Owned),
        }
    }

    /// Every boolean capability this entry defines.
    pub fn booleans(&self) -> impl Iterator<Item = (&'static str, bool)> + '_ {
        self.bools.iter().map(|(&capname, &value)| (capname, value))
    }

    /// Every numeric capability this entry defines.
    pub fn numbers(&self) -> impl Iterator<Item = (&'static str, u32)> + '_ {
        self.numbers
            .iter()
            .map(|(&capname, &value)| (capname, value))
    }

    /// Every string capability this entry defines, raw and unexpanded.
    pub fn strings(&self) -> impl Iterator<Item = (&'static str, &[u8])> + '_ {
        self.strings
            .iter()
            .map(|(&capname, value)| (capname, &value[..]))
    }

    /// The terminfo capname `name` selects, in whichever vocabulary it is
    /// written.
    fn capname<'a>(&self, name: CapabilityName<'a>) -> Option<&'a str> {
        match name {
            CapabilityName::Terminfo(capname) => Some(capname),
            CapabilityName::Termcap(code) => parser::names::capname_for_termcap(code),
        }
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
    /// A string capability's offset pointed outside the string table.
    ///
    /// The offset table and the string table are sized independently in the
    /// header, so an entry can name a start that the table it belongs to does
    /// not reach.
    StringOffsetOutOfRange,
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
            StringOffsetOutOfRange => matches!(other, StringOffsetOutOfRange),
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
            StringOffsetOutOfRange => {
                f.write_str("string capability offset lies outside the string table")
            }
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
