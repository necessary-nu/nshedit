//! Conformance: execute the C and execute the port, then compare.
//!
//! # What this is
//!
//! libedit ships no test suite — `Makefile.am:9` is `SUBDIRS = src
//! $(EXAMPLES_DIR) doc`, with no `TESTS` and no `check_PROGRAMS` — so there is
//! nothing to port and a harness had to be built. It lives in `conformance/`
//! at the repo root, as a handful of shell scripts and a C driver, and these
//! tests are thin wrappers so `cargo test` can reach it.
//!
//! The oracle is **the C in this tree, built by us**, never the system
//! libedit. Debian ships 3.1-20250104 as `libedit.so.2.0.75`; this tree is
//! `libedit-20260512-3.1` at `LT_VERSION 0:78:0`. Diffing against Debian's
//! build would blame the port for upstream's sixteen months of changes.
//! `conformance/build-oracle.sh` builds `src/*.c` out of tree into
//! `target/conformance/oracle` and refuses to fall back to anything else.
//!
//! # How to run it
//!
//! ```text
//! ./conformance/run.sh                        # everything, with a report
//! cargo test -p nshedit-abi -- --ignored      # the same, through cargo
//! ./conformance/build-oracle.sh [--clean]     # just the oracle
//! ./conformance/abi-shape.sh                  # just the symbol comparison
//! ./conformance/differential.sh [driver]      # just the trace diff
//! ./conformance/header-diff.sh                # just the header comparison
//! ```
//!
//! # Why `#[ignore]`
//!
//! Two reasons, and neither is that the tests are flaky.
//!
//! 1. They need a C toolchain and about a minute of wall clock for the
//!    autotools build. `cargo test --workspace` on a machine without `gcc`
//!    would fail for a reason that has nothing to do with the port.
//! 2. Historically they **reported open gaps**, and closing those is a
//!    decision, not a code change a test should force. Making them
//!    non-ignored while a gap was open would have meant either a red `cargo
//!    test --workspace` or a baseline file that quietly blessed whatever the
//!    port happened to do — and blessing is exactly what a conformance
//!    harness must not do. Every stage passes as of the `abi-missing-exports`
//!    node, so only reason 1 still holds; run `./conformance/run.sh` in CI
//!    where a toolchain is guaranteed.
//!
//! # How to read a failure
//!
//! `abi_shape` names symbols. `differential_traces` names operations:
//!
//! ```text
//!   [0146] H_NSAVE_FP 40
//!       oracle: rc=20 num=0 str=<OK>
//!       port  : rc=-1 num=11 str=<can't write history>
//! ```
//!
//! Full traces are left in `target/conformance/reports/` so a wider `diff -u`
//! is always available.
//!
//! A divergence is not automatically a port bug. `docs/errata.md` records
//! which defects of the C the port reproduces on purpose
//! (`plan/decisions/conformance-policy.md`), and a divergence that matches a
//! registered entry means the harness is working. Do not change the Rust to
//! make one of these pass without deciding, on the record, which way the
//! defect goes.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/nshedit-abi is two levels below the repo root")
        .to_path_buf()
}

/// Runs one of the `conformance/` scripts and fails the test with its whole
/// report on a non-zero exit. The report is the error message — a test that
/// said only "script failed" would throw away the part that names what
/// diverged.
fn run_script(name: &str, args: &[&str]) {
    let root = repo_root();
    let script = root.join("conformance").join(name);
    assert!(
        script.is_file(),
        "missing harness script: {}",
        script.display()
    );

    let out = Command::new(&script)
        .args(args)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("could not execute {}: {e}", script.display()));

    if !out.status.success() {
        panic!(
            "conformance/{name} {args:?} failed ({})\n\
             ----- stdout -----\n{}\n\
             ----- stderr -----\n{}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

/// The port's `cdylib`, which the differential driver links against. Built by
/// `cargo build`; this only checks it is there, because invoking cargo from
/// inside a cargo test would contend for the same target directory.
fn require_port_cdylib() {
    let lib = repo_root().join("target/debug/libnshedit.so");
    assert!(
        lib.is_file(),
        "no {} — run `cargo build` (or ./conformance/run.sh, which does)",
        lib.display()
    );
}

/// `conformance-oracle`: the reference artifact builds, reproducibly and
/// offline, from the C in this tree.
#[test]
#[ignore = "needs a C toolchain; run ./conformance/run.sh or cargo test -- --ignored"]
fn oracle_builds() {
    run_script("build-oracle.sh", &[]);
    // Idempotent: a second invocation must be a no-op, not a rebuild.
    run_script("build-oracle.sh", &[]);
}

/// `conformance-abi-shape`: the drop-in claim, stated as a test.
///
/// Passes. The port exports 205 symbols; the oracle exports 226, and the
/// difference is two decided sets and nothing else.
///
/// Twenty are the `vis` family, correct because Debian's libedit imports
/// `strvis`/`strunvis`/`vis` from `libbsd` and exports none of them, so
/// *matching* means not exporting them; our oracle exports them only because
/// `libbsd-dev` is absent on this host.
///
/// The twenty-first is `reallocarr`, a libc gap-filler
/// `dec:libedit:posix-only-scope` puts out of scope and
/// `dec:libedit:no-c-ffi` gives no route to — it reallocates a block the
/// caller allocated, so there is no `Layout` for `std::alloc::System` and only
/// libc's `realloc` would do. `abi-shape.sh` prints the whole argument, and
/// the residual risk with it: a consumer that declared it itself and reached
/// it through `libedit.so.2` finds nothing here.
///
/// `wcsdup` was on this list until `src/wcsdup.c` was corrected to include
/// `config.h` above its `#ifndef HAVE_WCSDUP` guard; the C now agrees with
/// Debian, which imports it from glibc.
///
/// The seven that were real gaps — `ct_encode_string`, `ct_decode_string`,
/// `fn_complete`, `fn_complete2`, `fn_display_match_list`,
/// `fn_filename_completion_function` and `fn_tilde_expand` — are exported now,
/// from [`nshedit_abi::chartype`] and [`nshedit_abi::filecomplete`]. Debian
/// exports all seven, so a consumer deployed today reaches them.
#[test]
#[ignore = "needs a C toolchain; run ./conformance/run.sh or cargo test -- --ignored"]
fn abi_shape() {
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("abi-shape.sh", &[]);
}

/// `conformance-differential`: identical scripted sequences, diffed traces.
///
/// Driver 1 is history + tokenizer + the history-file round trip: 247
/// operations under `LC_ALL=C.UTF-8` and 246 under `LC_ALL=C`, no terminal
/// involved.
///
/// Passes: every operation agrees under both codesets, including every byte
/// of the saved history file.
///
/// The last two divergences were `history_init` and `tok_init` aborting with
/// `SIGABRT` — all eight narrow entry points routed to a `core_gap()` in
/// `crates/nshedit-abi/src/histedit.rs`, because `nshedit` had no narrow
/// instantiation of `historyn.c` or `tokenizern.c`. Both are live now:
/// `nshedit::history` and `nshedit::tokenizer` are generic over the character
/// type and instantiated at `u32` and `c_char`, so the narrow entry points
/// call the same source the wide ones do. They are still probed in a forked
/// child, which is what would let the rest of a run survive a regression.
#[test]
#[ignore = "needs a C toolchain; run ./conformance/run.sh or cargo test -- --ignored"]
fn differential_traces() {
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("differential.sh", &[]);
}

/// The traces have to be byte-identical run to run, or a diff between them
/// means nothing. This runs every driver three times against each library
/// under each locale, from a freshly wiped work directory, and compares.
#[test]
#[ignore = "needs a C toolchain; run ./conformance/run.sh or cargo test -- --ignored"]
fn traces_are_deterministic() {
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("determinism.sh", &[]);
}

/// `conformance-header-diff`: the COMPILE-time half of the drop-in claim.
///
/// `abi_shape` above compares exported symbol NAMES; nothing else in the
/// harness checks a signature and nothing checks a struct LAYOUT. A field in
/// the wrong order inside `LineInfo`, `LineInfoW`, `HistEvent` or
/// `HistEventW` breaks every consumer that reads it and leaves the symbol
/// table byte-identical.
///
/// The headers this compares are OURS: `crates/nshedit-abi/include/` is
/// generated from this crate by cbindgen and committed, and libedit's own
/// `src/histedit.h` and `src/editline/readline.h` are what it is diffed
/// against — so a difference means our Rust is wrong, never that the header
/// wants editing.
///
/// Passes. 238 type assertions, 19 record measurements and every declaration
/// accounted for, against two decided divergences: `wcsdup`, which the
/// original declares and we deliberately neither export nor declare, and
/// `struct _history_state`, a tag on a record the original leaves anonymous.
/// It also builds and runs a C consumer against the generated headers with
/// `-Werror`, which is the claim itself rather than a proxy for it.
///
/// The staleness question — is the committed header what the generator
/// produces — is `tests/headers.rs`, which needs no toolchain and is not
/// ignored.
#[test]
#[ignore = "needs a C toolchain; run ./conformance/run.sh or cargo test -- --ignored"]
fn header_diff() {
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("header-diff.sh", &[]);
}

/// Whether the history file format the port reproduces is the same one
/// already on disk.
///
/// Neither side of this is the port: it compares `src/vis.c` (NetBSD-derived,
/// what the port translated) against `libbsd`'s `strvis` (what Debian's
/// `libedit.so.2` imports, and therefore what wrote every history file on a
/// Debian machine). They agree byte for byte on the corpus, under both
/// `C.UTF-8` and `C`, so a history file written by either is readable by the
/// other.
#[test]
#[ignore = "needs a C toolchain; run ./conformance/run.sh or cargo test -- --ignored"]
fn vis_matches_libbsd() {
    run_script("build-oracle.sh", &[]);
    run_script("vis-cross.sh", &[]);
}
