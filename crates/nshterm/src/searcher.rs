// Copyright 2019 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! ncurses-compatible database discovery
//!
//! Does not support hashed database, only filesystem!

use std::env;
use std::fs;
use std::path::PathBuf;

// The default terminfo location should be /usr/lib/terminfo but that's not guaranteed, so we check
// a few more locations. See https://tldp.org/HOWTO/Text-Terminal-HOWTO-16.html#ss16.2
const DEFAULT_LOCATIONS: &[&str] = &[
    "/etc/terminfo",
    "/usr/share/terminfo",
    "/usr/lib/terminfo",
    "/lib/terminfo",
    #[cfg(target_os = "haiku")]
    "/boot/system/data/terminfo",
];

/// Return path to database entry for `term`
///
/// # The environment is not trusted when the process is elevated
///
/// `TERMINFO`, `TERMINFO_DIRS` and `HOME` each name a directory this function
/// will load a compiled terminfo entry from, and a terminfo entry is a set of
/// escape sequences that get written to a terminal. Honouring them in a
/// set-uid process lets whoever started it choose those bytes.
///
/// So they are read only when [`nshedit_plat::is_elevated`] says the process
/// is running with the privileges of its invoker. Otherwise the search is the
/// compiled-in [`DEFAULT_LOCATIONS`] alone, which is exactly what ncurses does
/// through `use_terminfo_vars()` — see `ncurses/tinfo/db_iterator.c:226,327`
/// and `home_terminfo.c:52`, each of which gates the same three sources.
///
/// libedit already routed `TERM` through `secure_getenv`, so before this the
/// terminal *type* was guarded and the *database it was looked up in* was not,
/// which is the wrong half.
///
// TODO(nshterm): filesystem layout only — a hashed `terminfo.db` is not read at
// all, so on an ncurses built `--with-hashed-db` every search below misses and
// the terminal resolves to dumb. Not Debian's build. See `nshterm-hashed-db`.
pub fn get_dbpath_for_term(term: &str) -> Option<PathBuf> {
    let mut dirs_to_search = Vec::new();
    let mut default_locations = DEFAULT_LOCATIONS.iter().map(PathBuf::from);
    let first_char = term.chars().next()?;

    // One test for all three sources, as ncurses has one `use_terminfo_vars()`
    // for all three of its call sites. Splitting it per variable would make it
    // possible to guard two and forget the third.
    let trust_env = !nshedit_plat::is_elevated();

    // From the manual.
    //
    // > The  environment  variable TERMINFO is checked first, for a terminal
    // > database containing the terminal description.
    if let Some(dir) = trust_env.then(|| env::var_os("TERMINFO")).flatten() {
        dirs_to_search.push(PathBuf::from(dir));
    }

    // > Next, ncurses looks in $HOME/.terminfo for a compiled description.
    if let Some(home) = trust_env.then(|| env::var_os("HOME")).flatten() {
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
    if let Some(Ok(dirs)) = trust_env.then(|| env::var("TERMINFO_DIRS")) {
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
        if fs::metadata(&p).is_ok() {
            p.push(first_char.to_string());
            p.push(term);
            if fs::metadata(&p).is_ok() {
                return Some(p);
            }
            p.pop();
            p.pop();

            // on some installations the dir is named after the hex of the char
            // (e.g. OS X)
            p.push(format!("{:x}", first_char as usize));
            p.push(term);
            if fs::metadata(&p).is_ok() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod test {
    use super::{DEFAULT_LOCATIONS, get_dbpath_for_term};

    /// The guard is a property of the process, and this test process is not
    /// elevated, so what can be asserted here is that the trusted path still
    /// works — `TERMINFO` is honoured — and that the untrusted path is
    /// reachable at all.
    ///
    /// Proving the refusal needs a set-uid binary, which a unit test cannot
    /// make. `nshedit_plat::is_elevated` is the whole of the decision and it
    /// is three comparisons; keeping it that small is what makes this
    /// inspectable instead of testable.
    #[test]
    fn terminfo_is_honoured_when_the_process_is_not_elevated() {
        assert!(
            !nshedit_plat::is_elevated(),
            "the test runner is set-uid; this test cannot say anything"
        );

        let dir = tempdir();
        // ncurses' layout: $TERMINFO/<first character of the name>/<name>.
        let sub = dir.join("f");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("fakevt100"), b"not a real entry").unwrap();

        // SAFETY: single-threaded test, and the variable is restored below.
        let saved = std::env::var_os("TERMINFO");
        unsafe { std::env::set_var("TERMINFO", &dir) };
        let found = get_dbpath_for_term("fakevt100");
        match saved {
            Some(v) => unsafe { std::env::set_var("TERMINFO", v) },
            None => unsafe { std::env::remove_var("TERMINFO") },
        }
        std::fs::remove_dir_all(&dir).ok();

        assert_eq!(found.as_deref(), Some(sub.join("fakevt100").as_path()));
    }

    /// A terminal that exists nowhere resolves to nothing rather than to
    /// whatever happened to be next in the search path.
    #[test]
    fn an_unknown_terminal_finds_nothing() {
        assert_eq!(get_dbpath_for_term("nshedit-no-such-terminal"), None);
        assert_eq!(get_dbpath_for_term(""), None);
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
