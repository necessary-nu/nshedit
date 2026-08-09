//! Generate the shipped C headers from this crate.
//!
//!     cargo run -p nshedit-abi --example gen-headers [--] [OUTDIR]
//!
//! With no argument the headers are written where they are committed,
//! `crates/nshedit-abi/include/`. With one, they are written under that
//! directory instead.
//!
//! The generated header is the shipped header. Nothing hand-edits
//! `include/`; regenerate it from the ABI declarations instead.
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
//! Committing them has one cost: a generated file can go stale.
//! `tests/headers.rs` fails if the committed copy is not what this program
//! produces, and the direct C acceptance fixture proves consumers can use it.
//!
//! cbindgen is a `[dev-dependencies]` entry rather than something to
//! `cargo install`, so its version is pinned by `Cargo.lock` and the same
//! bytes come out on every machine.

#[cfg(not(test))]
include!("../cbindgen/generate.rs");

#[cfg(not(test))]
fn main() {
    let out_root = match std::env::args().nth(1) {
        Some(dir) => PathBuf::from(dir),
        None => shipped_dir(),
    };
    write_headers(&out_root);
}

#[cfg(test)]
fn main() {}
