//! Linux acceptance for the shipped C ABI.
//!
//! The maintained contract is owned by this repository: generated headers,
//! the committed export manifest, direct C consumers, and the installer. The
//! scripts under conformance/ exercise those artifacts without building a
//! second implementation.

//! # Why `crt-static` compiles this away
//!
//! Every stage below inspects the shipped `cdylib`, and rustc emits no
//! `cdylib` at all for a target whose `crt-static` feature is on — it drops
//! the crate type with a warning, which is the default arrangement for
//! `x86_64-unknown-linux-musl`. There is then no artifact to gate, so these
//! tests state their precondition in a `cfg` rather than failing on an
//! absence that is a build configuration and not a defect. The musl C ABI is
//! built with `crt-static` off and gated by `ci/musl-acceptance.sh`, which
//! runs these same `conformance/` stages against that build.
#![cfg(all(target_os = "linux", not(target_feature = "crt-static")))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

static STAGES: Mutex<()> = Mutex::new(());

fn stage_lock() -> MutexGuard<'static, ()> {
    STAGES.lock().unwrap_or_else(|error| error.into_inner())
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/nshedit-abi is two levels below the repository root")
        .to_path_buf()
}

/// Where `cargo build` put the artifact: `target/debug`, or
/// `target/<triple>/debug` when `NSHEDIT_TARGET` names a cross build.
/// `conformance/lib.sh` reads the same variable and derives the same
/// directory, so the scripts these tests invoke inspect the object this
/// function checked for.
fn library_dir() -> PathBuf {
    let mut dir = repo_root().join("target");
    if let Some(triple) = std::env::var_os("NSHEDIT_TARGET").filter(|t| !t.is_empty()) {
        dir.push(triple);
    }
    dir.join("debug")
}

fn require_cdylib() {
    let library = library_dir().join("libnshedit.so");
    assert!(
        library.is_file(),
        "no {} — run cargo build first",
        library.display()
    );
}

fn run_script(name: &str) {
    let root = repo_root();
    let script = root.join("conformance").join(name);
    let output = Command::new(&script)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|error| panic!("could not execute {}: {error}", script.display()));

    assert!(
        output.status.success(),
        "conformance/{name} failed ({})\n----- stdout -----\n{}\n----- stderr -----\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// [spec:nshedit:req:abi.surface-stability/test]
#[test]
fn exported_abi_matches_the_contract() {
    let _stage = stage_lock();
    require_cdylib();
    run_script("abi-shape.sh");
}

// [spec:nshedit:req:abi.behavioural-conformance+1/test]
// [spec:nshedit:req:abi.observational-coverage+1/test/integration]
#[test]
fn generated_headers_work_for_c() {
    let _stage = stage_lock();
    require_cdylib();
    run_script("c-abi.sh");
}

#[test]
fn installed_library_works_for_c() {
    let _stage = stage_lock();
    require_cdylib();
    run_script("soname.sh");
}

#[test]
fn unsafe_c_inputs_have_defined_results() {
    let _stage = stage_lock();
    require_cdylib();
    run_script("ub.sh");
}
