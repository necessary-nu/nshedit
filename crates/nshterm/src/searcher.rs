// Copyright 2019 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Ncurses-compatible filesystem database discovery.
//!
//! The supported Linux package matrix installs directory trees. Ncurses'
//! opt-in Berkeley DB layout is intentionally outside that matrix; see
//! `plan/decisions/terminal-caps-via-term-crate.md`.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::Result;

/// Whether the environment may steer terminfo database discovery.
//
// [spec:nshedit:req:terminal.typed-api]
///
/// `TERMINFO`, `TERMINFO_DIRS` and `HOME` each name a directory a compiled
/// terminfo entry will be loaded from, and such an entry is a set of escape
/// sequences that get written to a terminal. Honouring them in a set-uid
/// process lets whoever started it choose those bytes, so the decision is a
/// value the caller passes rather than something discovery assumes.
///
/// The policy and its process query live at the platform boundary; this
/// re-export preserves `nshterm`'s typed discovery API without duplicating
/// privilege classification here.
pub use nshedit_plat::EnvironmentTrust;

/// A terminal database name that is safe to use as one path component.
///
/// The invariant covers every supported host's path syntax. Both Unix and
/// Windows separators are forbidden, as are Windows volume or alternate-data
/// stream markers. This lets discovery construct `<root>/<first>/<name>`
/// without a caller-controlled component changing the directory being read.
// [spec:nshedit:req:terminal.typed-api]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerminalName<'a> {
    value: &'a str,
    first: char,
}

impl<'a> TerminalName<'a> {
    /// Validate a terminal database name without allocating or changing it.
    pub fn new(name: &'a str) -> std::result::Result<Self, InvalidTerminalName> {
        let first = name.chars().next().ok_or(InvalidTerminalName)?;
        let portable = !name.contains(['\\', ':', '\0']);
        let mut components = Path::new(name).components();
        let single_normal_component = matches!(
            (components.next(), components.next()),
            (Some(Component::Normal(component)), None) if component == OsStr::new(name)
        );

        (portable && single_normal_component)
            .then_some(Self { value: name, first })
            .ok_or(InvalidTerminalName)
    }

    /// The validated name, byte-for-byte as supplied by the caller.
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        self.value
    }

    fn first_char(self) -> char {
        self.first
    }
}

impl<'a> TryFrom<&'a str> for TerminalName<'a> {
    type Error = InvalidTerminalName;

    fn try_from(name: &'a str) -> std::result::Result<Self, Self::Error> {
        Self::new(name)
    }
}

impl AsRef<str> for TerminalName<'_> {
    fn as_ref(&self) -> &str {
        self.value
    }
}

impl fmt::Display for TerminalName<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.value)
    }
}

/// The supplied terminal database name was not one ordinary path component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct InvalidTerminalName;

impl fmt::Display for InvalidTerminalName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("terminal database name must be one ordinary path component")
    }
}

impl std::error::Error for InvalidTerminalName {}

// The default terminfo location should be /usr/lib/terminfo but that's not guaranteed, so we check
// a few more locations. See https://tldp.org/HOWTO/Text-Terminal-HOWTO-16.html#ss16.2
const DEFAULT_LOCATIONS: &[&str] = &[
    "/etc/terminfo",
    "/usr/share/terminfo",
    "/usr/lib/terminfo",
    "/lib/terminfo",
];

// [spec:nshedit:req:terminal.typed-api]
/// The database entry for `term`, if the search path holds one.
///
/// `Ok(None)` is a terminal the readable parts of the database do not
/// describe. A directory or entry that exists and cannot be read is an
/// [`Io`][crate::Error::Io] failure instead: it is a database the caller was
/// meant to be able to read, and reporting it as an absent terminal would
/// blame the terminal type for a permission problem. Requiring a
/// [`TerminalName`] here makes it impossible to construct a filesystem path
/// before validation; [`TermInfo::from_name`][crate::TermInfo::from_name] is
/// the convenient boundary for callers that hold an unvalidated string.
pub fn database_path(
    term: TerminalName<'_>,
    environment: EnvironmentTrust,
) -> Result<Option<PathBuf>> {
    search(term, environment, |name| env::var_os(name))
}

fn search(
    term: TerminalName<'_>,
    trust_env: EnvironmentTrust,
    environment: impl Fn(&str) -> Option<OsString>,
) -> Result<Option<PathBuf>> {
    let first_char = term.first_char();
    let term = term.as_str();
    let trust_env = trust_env.permits_environment();
    let mut dirs_to_search = Vec::new();
    let mut default_locations = DEFAULT_LOCATIONS.iter().map(PathBuf::from);

    // From the manual.
    //
    // > The  environment  variable TERMINFO is checked first, for a terminal
    // > database containing the terminal description.
    if let Some(dir) = trust_env.then(|| environment("TERMINFO")).flatten() {
        dirs_to_search.push(PathBuf::from(dir));
    }

    // > Next, ncurses looks in $HOME/.terminfo for a compiled description.
    if let Some(home) = trust_env.then(|| environment("HOME")).flatten() {
        let mut homedir = PathBuf::from(home);
        homedir.push(".terminfo");
        dirs_to_search.push(homedir)
    }

    // > Next, if the environment variable TERMINFO_DIRS is set, ncurses interprets
    // > the contents of that variable as a list of colon-separated pathnames of
    // > terminal databases to be searched.
    // >
    // > An  empty  pathname  (i.e.,  if  the  variable begins or ends with a
    // > colon, or contains adjacent colons) is interpreted as the system location
    // > /usr/share/terminfo.
    if let Some(dirs) = trust_env
        .then(|| environment("TERMINFO_DIRS"))
        .flatten()
        .and_then(|value| value.into_string().ok())
    {
        for i in dirs.split(':') {
            if i.is_empty() {
                dirs_to_search.extend(&mut default_locations);
            } else {
                dirs_to_search.push(PathBuf::from(i));
            }
        }
    }

    // > Finally, ncurses searches these compiled-in locations...
    //
    // NOTE: We only append these to `dirs_to_search` once. If we've already added these
    // directories as specified in `TERMINFO_DIRS`, this operation will be a no-op.
    dirs_to_search.extend(&mut default_locations);

    // Look for the terminal in all of the search directories
    for mut p in dirs_to_search {
        if !exists(&p)? {
            continue;
        }
        p.push(first_char.to_string());
        p.push(term);
        if exists(&p)? {
            return Ok(Some(p));
        }
        p.pop();
        p.pop();

        // on some installations the dir is named after the hex of the char
        // (e.g. OS X)
        p.push(format!("{:x}", first_char as usize));
        p.push(term);
        if exists(&p)? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// Whether `path` names something, reporting anything other than its absence.
///
/// A search path is a list of places an entry may be; a directory that is not
/// there is the ordinary case and not a failure. A directory that is there
/// and refuses to answer is one, and it is the one that must not be silently
/// read as "this terminal does not exist".
fn exists(path: &Path) -> Result<bool> {
    match fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(crate::Error::Io(error)),
    }
}

#[cfg(test)]
mod test {
    #[cfg(unix)]
    use super::search;
    use super::{
        DEFAULT_LOCATIONS, EnvironmentTrust, InvalidTerminalName, TerminalName, database_path,
    };

    /// Search-path policy is an explicit input here; platform classification
    /// is tested at the platform boundary.
    #[cfg(unix)]
    #[test]
    fn explicit_trust_honours_terminfo() {
        let dir = tempdir();
        // ncurses' layout: $TERMINFO/<first character of the name>/<name>.
        let sub = dir.join("f");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("fakevt100"), b"not a real entry").unwrap();

        let environment = |name: &str| (name == "TERMINFO").then(|| dir.clone().into_os_string());
        let term = TerminalName::new("fakevt100").unwrap();
        let found = search(term, EnvironmentTrust::Honoured, environment);
        let untrusted = search(term, EnvironmentTrust::Ignored, environment);
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(
            found.unwrap().as_deref(),
            Some(sub.join("fakevt100").as_path())
        );
        assert_eq!(untrusted.unwrap(), None);
    }

    /// A terminal that exists nowhere resolves to nothing rather than to
    /// whatever happened to be next in the search path.
    #[test]
    fn an_unknown_terminal_finds_nothing() {
        let trust = EnvironmentTrust::for_process();
        let term = TerminalName::new("nshedit-no-such-terminal").unwrap();
        assert_eq!(database_path(term, trust).unwrap(), None);
    }

    /// Names accepted here are unchanged ordinary components on Unix, macOS,
    /// and Windows; discovery may safely use them under a database root.
    // [spec:nshedit:req:terminal.typed-api/test]
    #[test]
    fn terminal_names_are_portable_single_components() {
        for valid in [
            "xterm",
            "xterm-256color",
            "screen.xterm-256color",
            ".private-terminal",
            "terminal name",
            "λ-terminal",
        ] {
            assert_eq!(TerminalName::new(valid).unwrap().as_str(), valid);
        }

        for invalid in [
            "",
            ".",
            "..",
            "/xterm",
            "../xterm",
            "xterm/../vt100",
            r"\xterm",
            r"..\xterm",
            r"xterm\..\vt100",
            r"C:\xterm",
            "C:xterm",
            "xterm:alternate-stream",
            "xterm\0suffix",
        ] {
            assert_eq!(TerminalName::new(invalid), Err(InvalidTerminalName));
        }
    }

    /// The compiled-in list is what an elevated process is left with, so it
    /// must be non-empty and absolute — a relative entry would be resolved
    /// against a working directory the caller chose, which is the same class
    /// of hole the guard closes.
    #[test]
    fn the_fallback_locations_are_absolute() {
        assert!(!DEFAULT_LOCATIONS.is_empty());
        for loc in DEFAULT_LOCATIONS {
            assert!(loc.starts_with('/'), "{loc} is not absolute");
        }
    }

    /// A directory that is on the search path and refuses to answer is a
    /// database the caller was meant to read. Reporting it as an absent
    /// terminal would blame the terminal type for a permission problem, and
    /// would leave the caller with no way to learn what actually happened.
    // [spec:nshedit:req:terminal.typed-api/test]
    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_is_reported() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().join("blocked");
        let sub = dir.join("f");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("fakevt100"), b"not a real entry").unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();

        let readable_anyway = std::fs::read_dir(&dir).is_ok();
        let environment = |name: &str| (name == "TERMINFO").then(|| dir.clone().into_os_string());
        let found = search(
            TerminalName::new("fakevt100").unwrap(),
            EnvironmentTrust::Honoured,
            environment,
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        if readable_anyway {
            // The permission bits do not apply to this user, so the search
            // simply found the entry; there is nothing to assert.
            assert_eq!(found.unwrap(), Some(sub.join("fakevt100")));
            return;
        }
        assert!(
            matches!(found, Err(crate::Error::Io(_))),
            "expected the permission failure, got {found:?}"
        );
    }

    #[cfg(unix)]
    fn tempdir() -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "nshterm-searcher-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }
}
