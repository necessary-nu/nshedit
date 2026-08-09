//! Linux acceptance for the shipped C ABI.
//!
//! The maintained contract is owned by this repository: generated headers,
//! the committed export manifest, direct C consumers, and the installer. The
//! scripts under conformance/ exercise those artifacts without building a
//! second implementation.

#![cfg(target_os = "linux")]

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

fn require_cdylib() {
    let library = repo_root().join("target/debug/libnshedit.so");
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
