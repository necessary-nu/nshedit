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
//! cargo test -p nshedit-abi --test conformance # the same, through cargo
//! ./conformance/build-oracle.sh [--clean]     # just the oracle
//! ./conformance/abi-shape.sh                  # just the symbol comparison
//! ./conformance/differential.sh [driver]      # just the trace diff
//! ./conformance/header-diff.sh                # just the header comparison
//! ```
//!
//! # These run by default
//!
//! They are not `#[ignore]`d. A repository whose entire subject is porting a
//! C library, whose oracle is built from `src/*.c` in this tree, is not one
//! where a C toolchain can go missing — and the cost is 4.7s warm, paid once
//! more on a cold `target/` for the autotools build.
//!
//! They **must** run single-threaded, which [`STAGES`] enforces rather than
//! leaving to an invocation flag. Every stage shares one work directory under
//! `target/conformance/`, wiping and rebuilding it; `conformance/run.sh` runs
//! them in sequence, and cargo would otherwise run them in parallel and let
//! `differential_traces` read a tree `traces_are_deterministic` was midway
//! through replacing. Serialising here keeps `cargo test --workspace` honest
//! without asking anyone to remember `--test-threads=1`.
//!
//! Historically they were ignored for a second reason worth recording: they
//! **reported open gaps**, and closing those is a decision, not something a
//! red test should force. A temporary known-gap fixture is now permitted only
//! as the exact unified diff from the reviewed oracle trace to the port trace.
//! Any changed byte fails, and equality also fails until the stale fixture is
//! removed. That gives a short baseline-fix node a green gate without quietly
//! blessing subsequent drift.
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
use std::sync::{Mutex, MutexGuard};

/// Serialises the stages against each other.
///
/// Every one of them wipes and rebuilds the same work directory under
/// `target/conformance/`, so two running at once corrupt each other's inputs.
/// Cargo runs tests in parallel by default, so the guard has to live here —
/// relying on `--test-threads=1` would mean a bare `cargo test --workspace`
/// fails intermittently and blames the port for it.
static STAGES: Mutex<()> = Mutex::new(());

/// Takes the stage lock, ignoring poisoning: a panicking stage means that
/// stage failed, not that the work directory is unusable for the next one.
fn stage_lock() -> MutexGuard<'static, ()> {
    STAGES.lock().unwrap_or_else(|e| e.into_inner())
}

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
fn oracle_builds() {
    let _stages = stage_lock();
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
fn abi_shape() {
    let _stages = stage_lock();
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
/// history retains one translated generic implementation during its cutover,
/// while both tokenizer families convert at the ABI boundary and use the
/// same native owned parser. They are still probed in a forked child, which
/// is what would let the rest of a run survive a regression.
#[test]
fn differential_traces() {
    let _stages = stage_lock();
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("differential.sh", &[]);
}

/// The traces have to be byte-identical run to run, or a diff between them
/// means nothing. This runs every driver three times against each library
/// under each locale, from a freshly wiped work directory, and compares.
#[test]
fn traces_are_deterministic() {
    let _stages = stage_lock();
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
fn header_diff() {
    let _stages = stage_lock();
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("header-diff.sh", &[]);
}

/// `conformance-soname`: the LOADER's half of the drop-in claim.
///
/// [`abi_shape`] compares exported symbol names and [`header_diff`] compiles a
/// consumer against our headers. Neither asks what decides whether a binary
/// already on someone's disk starts: does the loader find us under the name
/// that binary recorded as `DT_NEEDED`?
///
/// Passes. `conformance/soname.sh` installs into `target/conformance/prefix`
/// and then runs a consumer built against Debian's `libedit.so.2`, and another
/// built against the in-tree oracle's `libedit.so.0`, with nothing but that
/// install on the library path. Both load and both work, which is the compat
/// symlinks doing their job — neither binary knows anything about us.
///
/// Both names are needed: `libedit.so.0` is libedit's own, from
/// `configure.ac`'s `LT_VERSION 0:75:0`, and `libedit.so.2` exists only
/// because `debian/patches/update-soname.diff` changes that line to `2:75:0`.
/// A newly linked consumer records `libnshedit.so.0` instead, which is the
/// point — `ldd` names what actually loaded.
#[test]
fn soname_and_compat_symlinks() {
    let _stages = stage_lock();
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("soname.sh", &[]);
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
fn vis_matches_libbsd() {
    let _stages = stage_lock();
    run_script("build-oracle.sh", &[]);
    run_script("vis-cross.sh", &[]);
}

/// `conformance-driver-ub`: the calls the C has no defined answer for.
///
/// The other stages are differentials. This one cannot be: every case comes
/// from an entry in `docs/errata.md` whose disposition is `define`, which
/// says the C is undefined and the port is not — so agreement is the wrong
/// test and a diff would report every success as a failure.
///
/// Asserted: the port survives all 20 cases. Reported but not asserted: what
/// the oracle does, which matters because a case the C also survives proves
/// nothing about the port. It currently dies on 11 of the 20, each in a
/// forked child, so the corpus can be judged rather than trusted.
///
/// Passes. Two of the cases had no erratum at all and were found by running
/// it — `history_expand(line, NULL)` and `tilde_expand(NULL)`, both of which
/// the C stores through or reads without checking.
#[test]
fn undefined_behaviour_is_defined() {
    let _stages = stage_lock();
    require_port_cdylib();
    run_script("build-oracle.sh", &[]);
    run_script("ub.sh", &[]);
}
