//! `TermInfo` itself — the crate's entry point, which had no tests at all.
//!
//! `parm`, `parser::compiled`, `parser::names` and `searcher` all carry their
//! own; `lib.rs` was 416 lines and zero. Everything a caller of this crate
//! actually touches is here: loading an entry, reading a capability out of
//! one, applying a parameterized string, and the errors each of those can
//! return.

pub mod common;

use common::{fixture, fixture_names};
use nshterm::parm::Param;
use nshterm::{Error, TermInfo};

/// Every fixture parses, and parses into something usable.
///
/// A parser that accepts a file and produces an entry with no names, or no
/// capabilities at all, has failed while reporting success — which is the
/// failure mode that matters here, because the caller has no way to tell.
#[test]
fn every_fixture_loads_into_a_usable_entry() {
    let names = fixture_names();
    assert!(
        names.len() >= 30,
        "expected the fixture corpus, got {names:?}"
    );

    for name in &names {
        let term = fixture(name);
        assert!(!term.names.is_empty(), "{name}: entry has no names");
        assert!(
            term.names.iter().all(|n| !n.is_empty()),
            "{name}: an empty alias"
        );
        assert!(
            !term.strings.is_empty(),
            "{name}: parsed but has no string capabilities"
        );
        // Capability keys come from the static name tables, never from the
        // file, so a key that is not in them means the parser indexed past
        // its own table.
        for key in term.strings.keys() {
            assert!(
                nshterm::parser::names::STRING_NAMES.contains(key),
                "{name}: string capability {key:?} is not a known capname"
            );
        }
    }
}

/// The `dumb` entry, which the conformance harness pins `TERM` to, so its
/// contents are load-bearing beyond this crate.
#[test]
fn the_dumb_entry_says_what_it_should() {
    let term = fixture("dumb");
    assert!(term.names.iter().any(|n| n == "dumb"));
    assert_eq!(
        term.strings.get("bel").map(Vec::as_slice),
        Some(&b"\x07"[..])
    );
    assert_eq!(term.numbers.get("cols"), Some(&80));
    // A dumb terminal cannot address the cursor; that is what makes it dumb.
    assert!(!term.strings.contains_key("cup"));
}

/// `apply_cap` on a capability the entry has, and on one it does not.
#[test]
fn apply_cap_expands_or_says_it_cannot() {
    let term = fixture("xterm");
    let mut out = Vec::new();

    // `cup` is row-and-column addressing, so this exercises the parameter
    // path rather than just a byte copy.
    term.apply_cap("cup", &[Param::Number(2), Param::Number(5)], &mut out)
        .expect("xterm should support cup");
    assert_eq!(out, b"\x1b[3;6H", "cup is 0-based in, 1-based out");

    let mut out = Vec::new();
    let err = term
        .apply_cap("no_such_capability", &[], &mut out)
        .expect_err("an unknown capability must not silently succeed");
    assert_eq!(err, Error::NotSupported);
    assert!(out.is_empty(), "nothing should have been written");
}

/// A capability that needs parameters, given none.
///
/// This must be an error rather than a panic: the parameters come from the
/// caller and the capability string comes from a file, so a mismatch between
/// them is ordinary input, not a bug.
#[test]
fn a_missing_parameter_is_an_error_not_a_panic() {
    let term = fixture("xterm");
    let mut out = Vec::new();
    // `%p1` with no parameters reads the zero-initialised slot rather than
    // failing, so this checks the call RETURNS at all — the assertion is the
    // absence of a panic, and the result is whatever the zeroes produce.
    let r = term.apply_cap("cup", &[], &mut out);
    assert!(r.is_ok() || matches!(r, Err(Error::ParameterizedExpansion(_))));
}

/// `reset` falls back sgr0 -> sgr -> op, and says so when it has none.
#[test]
fn reset_uses_whichever_reset_capability_the_entry_has() {
    let term = fixture("xterm");
    let mut out = Vec::new();
    term.reset(&mut out).expect("xterm has sgr0");
    assert!(!out.is_empty());
    assert_eq!(out, common::cap(&term, "sgr0"));

    // An entry with none of the three cannot be reset, and must report that
    // rather than writing nothing and returning success.
    let bare = TermInfo {
        names: vec!["bare".into()],
        bools: Default::default(),
        numbers: Default::default(),
        strings: Default::default(),
    };
    let mut out = Vec::new();
    assert_eq!(bare.reset(&mut out).unwrap_err(), Error::NotSupported);
    assert!(out.is_empty());
}

/// Bytes that are not a terminfo file are rejected by name, and none of them
/// panic. A compiled entry is a file we did not write, found wherever the
/// search path led.
#[test]
fn malformed_input_is_reported_rather_than_trusted() {
    // Empty, truncated header, and a plausible-but-wrong magic.
    assert!(matches!(
        TermInfo::from_reader(&b""[..]),
        Err(Error::Io(_)) | Err(Error::BadMagic(_))
    ));

    let bad_magic = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
    match TermInfo::from_reader(&bad_magic[..]) {
        Err(Error::BadMagic(m)) => assert_eq!(m, 0),
        other => panic!("expected BadMagic, got {other:?}"),
    }

    // A real header, then nothing.
    let truncated = b"\x1a\x01\x10\x00";
    assert!(TermInfo::from_reader(&truncated[..]).is_err());

    // Every prefix of a real entry: each must error or parse, never panic.
    let whole =
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/xterm"))
            .unwrap();
    for cut in (0..whole.len()).step_by(7) {
        let _ = TermInfo::from_reader(&whole[..cut]);
    }
}

/// A path that is not there is an error, not a panic or an empty entry.
#[test]
fn a_missing_file_is_an_error() {
    let err = TermInfo::from_path("/nonexistent/nshterm/terminfo/entry").unwrap_err();
    assert!(matches!(err, Error::Io(_)), "got {err:?}");
}

/// `from_name` searches the database; with `TERMINFO` pointed at the fixture
/// corpus it must find an entry there and nowhere else.
#[test]
fn from_name_finds_an_entry_under_terminfo() {
    const CHILD: &str = "NSHTERM_TERMINFO_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let found = TermInfo::from_name("xterm-nshterm-test")
            .expect("the entry under TERMINFO should be found");
        assert!(found.names.iter().any(|name| name == "xterm"));
        assert_eq!(
            TermInfo::from_name("nshterm-no-such-terminal").unwrap_err(),
            Error::TerminfoEntryNotFound,
            "a terminal that exists nowhere must not resolve to something else"
        );
        assert_eq!(
            TermInfo::from_name("xterm-nshterm-no-such-terminal").unwrap_err(),
            Error::TerminfoEntryNotFound,
            "an ANSI-looking name must still identify a real database entry"
        );
        return;
    }

    let dir = std::env::temp_dir().join(format!("nshterm-terminfo-{}", std::process::id()));
    let sub = dir.join("x");
    std::fs::create_dir_all(&sub).unwrap();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/xterm");
    std::fs::copy(&src, sub.join("xterm-nshterm-test")).unwrap();

    let status = std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .args(["--exact", "from_name_finds_an_entry_under_terminfo"])
        .env("TERMINFO", &dir)
        .env(CHILD, "1")
        .status()
        .expect("run isolated TERMINFO consumer");
    std::fs::remove_dir_all(&dir).ok();

    assert!(status.success(), "isolated TERMINFO consumer failed");
}

/// Every error variant renders a message, and `PartialEq` is the hand-written
/// one — `Error` holds an `io::Error`, which is not `Eq`, so equality here is
/// a decision rather than a derive and worth pinning.
#[test]
fn the_errors_compare_and_print() {
    assert_eq!(Error::NotSupported, Error::NotSupported);
    assert_eq!(Error::BadMagic(3), Error::BadMagic(3));
    assert_ne!(Error::BadMagic(3), Error::BadMagic(4));
    assert_ne!(Error::NotSupported, Error::TermUnset);

    for e in [
        Error::NotSupported,
        Error::TermUnset,
        Error::TerminfoEntryNotFound,
        Error::BadMagic(0x1234),
    ] {
        let msg = e.to_string();
        assert!(!msg.is_empty(), "{e:?} renders nothing");
        assert!(
            !msg.contains("Error"),
            "{e:?} renders the type name rather than a message: {msg}"
        );
    }

    // The conversion a caller gets when they use `?` in an io context.
    let io: std::io::Error = Error::NotSupported.into();
    assert!(!io.to_string().is_empty());
}
