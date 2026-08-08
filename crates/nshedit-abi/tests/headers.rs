//! The committed headers are what the generator produces.
//!
//! `crates/nshedit-abi/include/histedit.h` and `include/editline/readline.h`
//! are generated from this crate by cbindgen and committed, because a
//! distribution installing them cannot be asked to have a Rust toolchain.
//! The cost of committing a generated file is that it can go stale, and this
//! is the gate for that: it regenerates in memory and compares.
//!
//! Not `#[ignore]`d, unlike `conformance.rs`. Nothing here needs a C compiler
//! or the oracle build — cbindgen reads Rust source and that is all — so it
//! runs in the ordinary `cargo test --workspace`, which is where a stale
//! header should be caught. The *comparison against libedit's own headers* is
//! a different question and lives in `conformance/header-diff.sh`, which does
//! need a toolchain.
//!
//! On failure: regenerate, never edit.
//!
//!     cargo run -p nshedit-abi --example gen-headers

include!("../cbindgen/generate.rs");

/// Every committed header is byte-identical to a fresh generation.
// [spec:nshedit:req:abi.surface-stability/test]
#[test]
fn committed_headers_are_freshly_generated() {
    let shipped = shipped_dir();
    for (rel, expected) in generate() {
        let path = shipped.join(rel);
        let found = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} is missing or unreadable: {e}", path.display()));
        assert!(
            found == expected,
            "{} is not what the generator produces.\n\
             Regenerate it — never edit it:\n\
             \x20   cargo run -p nshedit-abi --example gen-headers\n\
             (committed {} bytes, generated {} bytes)",
            path.display(),
            found.len(),
            expected.len(),
        );
    }
}

/// The generator's own inputs exist and are the ones a reader would expect:
/// a header is generated from the modules that own its surface, and from no
/// others. A source file silently dropped from the list would take its
/// declarations out of the shipped header, and `header-diff.sh` would then
/// report them missing against libedit's — but only where a C toolchain is
/// available, which is why the list is checked here too.
#[test]
fn every_generator_input_exists() {
    let workspace = crate_dir().join("../..");
    for header in HEADERS {
        assert!(
            crate_dir().join(header.config).is_file(),
            "missing cbindgen config {}",
            header.config
        );
        assert!(
            !header.srcs.is_empty(),
            "{} is generated from nothing",
            header.out
        );
        for src in header.srcs {
            let path = workspace.join(src);
            assert!(path.is_file(), "missing generator input {}", path.display());
        }
    }
}
