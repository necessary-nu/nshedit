// Copyright 2026 Necessary Innovations AB.
//
// This file is not derived from `term` 1.2.1 — upstream shipped no test
// coverage for `parm::expand` beyond the eight cases inside `src/parm.rs`.
// It is licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option, to
// match the crate it tests.

//! Helpers shared by the `parm_*` integration tests.
//!
//! Each test binary exposes this shared module publicly so its full helper API
//! remains reachable even when that binary uses only a subset.

use std::path::PathBuf;

use nshterm::parm::{Error, Param, Variables, expand};
use nshterm::{CapabilityName, TermInfo};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Expand `cap` with fresh variables, asserting success.
pub fn ex(cap: &[u8], params: &[Param]) -> Vec<u8> {
    match expand(cap, params, &mut Variables::new()) {
        Ok(out) => out,
        Err(e) => panic!("expanding {:?} failed: {e}", String::from_utf8_lossy(cap)),
    }
}

/// Expand `cap` and return the output as a string, for readable assertions.
pub fn s(cap: &[u8], params: &[Param]) -> String {
    String::from_utf8(ex(cap, params)).expect("expansion produced invalid UTF-8")
}

/// Expand `cap` with fresh variables, asserting failure, and return the error.
pub fn ex_err(cap: &[u8], params: &[Param]) -> Error {
    match expand(cap, params, &mut Variables::new()) {
        Ok(out) => panic!(
            "expanding {:?} unexpectedly succeeded with {:?}",
            String::from_utf8_lossy(cap),
            String::from_utf8_lossy(&out)
        ),
        Err(e) => e,
    }
}

/// `Param::Number` for each element, for capabilities that take integers.
pub fn nums(ns: &[i32]) -> Vec<Param> {
    ns.iter().copied().map(Param::Number).collect()
}

/// Load one of the compiled fixtures in `tests/data/`.
pub fn fixture(name: &str) -> TermInfo {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data", name]
        .iter()
        .collect();
    TermInfo::from_path(&path).unwrap_or_else(|e| panic!("parsing fixture {name}: {e}"))
}

/// Every fixture in `tests/data/`, by name.
pub fn fixture_names() -> Vec<String> {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data"]
        .iter()
        .collect();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("tests/data is missing")
        .map(|e| e.expect("reading tests/data").file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Fetch a capability from a fixture, failing loudly if the entry lacks it —
/// a silently-absent capability would turn a real assertion into a no-op.
pub fn cap(term: &TermInfo, capname: &str) -> Vec<u8> {
    term.string(CapabilityName::Terminfo(capname))
        .unwrap_or_else(|| panic!("fixture {:?} has no {capname}", term.names()[0]))
        .into_owned()
}
