//! Generate the shipped C headers from this crate.
//!
//!     cargo run -p nshedit-abi --example gen-headers [--] [OUTDIR]
//!
//! With no argument the headers are written where they are committed,
//! `crates/nshedit-abi/include/`. With one, they are written under that
//! directory instead, which is what `conformance/header-diff.sh` does when it
//! checks the committed copies against a fresh generation.
//!
//! The generated header IS the shipped header. libedit's own `src/histedit.h`
//! and `src/editline/readline.h` are what the harness diffs it against, and
//! that diff proves the Rust is right rather than proving the header is.
//! Nothing hand-edits `include/`; regenerate instead.
//!
//! # Generated and committed, not generated at build time
//!
//! A distribution packaging this must be able to install `histedit.h` and
//! `editline/readline.h` with no Rust toolchain in the picture at all, let
//! alone cbindgen — the headers are consumed by C programs, by pkg-config
//! recipes and by build systems that have never heard of cargo. A `build.rs`
//! would also make `cargo build` write into the source tree and dirty the
//! working copy on every build.
//!
//! Committing them has one cost, that a generated file can go stale, and it
//! is paid twice: `tests/headers.rs` fails if the committed copy is not what
//! this program produces — with no C toolchain needed, so it runs in the
//! ordinary `cargo test` — and `conformance/header-diff.sh` regenerates
//! before it compares.
//!
//! cbindgen is a `[dev-dependencies]` entry rather than something to
//! `cargo install`, so its version is pinned by `Cargo.lock` and the same
//! bytes come out on every machine.

include!("../cbindgen/generate.rs");

fn main() {
    let out_root = match std::env::args().nth(1) {
        Some(dir) => PathBuf::from(dir),
        None => shipped_dir(),
    };
    write_headers(&out_root);
}
