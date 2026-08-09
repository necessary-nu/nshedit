//! Structural checks for the maintained Rust source boundary.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/nshedit-abi is two levels below the repo root")
        .to_path_buf()
}

fn rust_sources_below(directory: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read first-party source directory") {
        let path = entry.expect("read first-party source entry").path();
        if path.is_dir() {
            rust_sources_below(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

// [spec:nshedit:req:core.public-surface/test]
// [spec:nshedit:req:core.unsafe-free/test]
#[test]
fn native_core_surface_is_safe() {
    let root = repo_root();
    let source = fs::read_to_string(root.join("crates/nshedit/src/lib.rs"))
        .expect("read the native core facade");

    assert!(
        source.contains("#![forbid(unsafe_code)]"),
        "the native core must reject unsafe declarations and implementations"
    );
    assert!(
        !source.contains("#[path"),
        "the native facade must not compile compatibility sources by path"
    );

    let public_modules: Vec<&str> = source
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("pub mod ")
                .and_then(|module| module.strip_suffix(';'))
        })
        .collect();
    assert_eq!(
        public_modules,
        ["domain", "editor", "history", "history_file", "tokenizer"],
        "the native facade exposed a module outside the semantic Rust API"
    );
}

// [spec:nshedit:req:core.no-compat-internals/test]
#[test]
fn translated_core_and_facade_are_absent() {
    let root = repo_root();
    let native = root.join("crates/nshedit/src");
    let abi = root.join("crates/nshedit-abi/src");

    assert!(
        !abi.join("compat.rs").exists() && !abi.join("compat").exists(),
        "the retired translated implementation must not remain disconnected behind the ABI"
    );

    let mut sources = Vec::new();
    rust_sources_below(&native, &mut sources);
    sources.sort();
    let forbidden = [
        "extern \"C\"",
        "core::ffi::c_",
        "std::ffi::c_",
        "VaList",
        "CFile",
        "errno::",
        "#[path",
    ];
    for path in sources {
        let source = fs::read_to_string(&path).expect("read native core source");
        for spelling in forbidden {
            assert!(
                !source.contains(spelling),
                "{} contains retired compatibility spelling {spelling:?}",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
        }
    }
}

// [spec:nshedit:req:workspace.no-legacy-allows/test]
#[test]
fn first_party_rust_rejects_allow_attributes() {
    let root = repo_root();
    let mut sources = Vec::new();
    rust_sources_below(&root.join("crates"), &mut sources);
    sources.sort();

    // Assemble the spellings so this test does not report its own search
    // strings. Whitespace and line comments are discarded so a split or
    // formatted attribute cannot evade the check.
    let item_allow = ["#", "[", "allow", "("].concat();
    let inner_allow = ["allow", "("].concat();
    let crate_allow = ["#", "!", "[", "allow", "("].concat();
    let cfg_attr = ["#", "[", "cfg_attr", "("].concat();
    let expect = ["#", "[", "expect", "("].concat();

    for path in sources {
        let source = fs::read_to_string(&path).expect("read first-party Rust source");
        let code: String = source
            .lines()
            .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
            .flat_map(str::chars)
            .filter(|character| !character.is_whitespace())
            .collect();

        assert!(
            !code.contains(&item_allow) && !code.contains(&crate_allow),
            "{} contains a lint allow attribute; remove the exception or use the narrowest fulfilled expect with a reason",
            path.strip_prefix(&root).unwrap_or(&path).display()
        );

        let mut conditional_attributes = code.as_str();
        while let Some(start) = conditional_attributes.find(&cfg_attr) {
            conditional_attributes = &conditional_attributes[start + cfg_attr.len()..];
            let end = conditional_attributes
                .find(")]")
                .expect("a Rust cfg_attr attribute has no closing delimiter");
            let attribute = &conditional_attributes[..end];
            assert!(
                !attribute.contains(&inner_allow),
                "{} conditionally enables a lint allow; conditional code must remain warning-clean too",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
            conditional_attributes = &conditional_attributes[end + 2..];
        }

        let mut remainder = code.as_str();
        while let Some(start) = remainder.find(&expect) {
            remainder = &remainder[start + expect.len()..];
            let end = remainder
                .find(")]")
                .expect("a Rust expect attribute has no closing delimiter");
            let attribute = &remainder[..end];
            assert!(
                attribute.contains("reason="),
                "{} contains an expect attribute without a reason",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
            remainder = &remainder[end + 2..];
        }
    }
}

/// The C symbol a `#[unsafe(no_mangle)]` item defines, if the line declares
/// one.
fn exported_symbol(declaration: &str) -> Option<&str> {
    let item = declaration.strip_prefix("pub ")?;
    let name = item
        .strip_prefix("unsafe extern \"C\" fn ")
        .or_else(|| item.strip_prefix("extern \"C\" fn "))
        .or_else(|| item.strip_prefix("static mut "))
        .or_else(|| item.strip_prefix("static "))?;
    name.split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .next()
        .filter(|name| !name.is_empty())
}

/// Every C symbol the ABI crate defines.
fn exported_symbols(abi_source: &Path) -> BTreeSet<String> {
    let mut sources = Vec::new();
    rust_sources_below(abi_source, &mut sources);
    let mut symbols = BTreeSet::new();
    for path in sources {
        let source = fs::read_to_string(&path).expect("read an ABI crate source");
        let mut exported = false;
        for line in source.lines() {
            let line = line.trim();
            if line == "#[unsafe(no_mangle)]" {
                exported = true;
                continue;
            }
            if !exported || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            exported = false;
            if let Some(name) = exported_symbol(line) {
                symbols.insert(name.to_owned());
            }
        }
    }
    symbols
}

/// Every C symbol `source` declares as coming from somewhere else, whether by
/// its Rust name or through a linkage name.
fn foreign_declarations(source: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        let line = line.trim();
        if !inside {
            inside = line.ends_with("extern \"C\" {");
            continue;
        }
        if let Some(rest) = line.strip_prefix("#[link_name = \"") {
            if let Some(name) = rest.split('"').next() {
                declarations.push(name.to_owned());
            }
            continue;
        }
        let declared = line
            .strip_prefix("fn ")
            .or_else(|| line.strip_prefix("unsafe fn "))
            .or_else(|| line.strip_prefix("pub fn "))
            .or_else(|| line.strip_prefix("static "))
            .or_else(|| line.strip_prefix("static mut "));
        if let Some(name) = declared {
            declarations.extend(
                name.split(|character: char| !(character.is_alphanumeric() || character == '_'))
                    .next()
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned),
            );
            continue;
        }
        if line.starts_with('}') {
            inside = false;
        }
    }
    declarations
}

/// A symbol this workspace defines must never also be declared as foreign:
/// that would make private code re-enter the library through the linker
/// rather than call the implementation it is compiled with, and would carry
/// the C's operation codes and variadic tails inwards with it.
// [spec:nshedit:req:abi.rust-internals/test]
#[test]
fn no_exported_symbol_is_declared_foreign() {
    let root = repo_root();
    let exported = exported_symbols(&root.join("crates/nshedit-abi/src"));
    for expected in ["el_set", "el_get", "history", "readline", "rl_line_buffer"] {
        assert!(
            exported.contains(expected),
            "the exported ABI surface was not recognised: {expected} is missing"
        );
    }

    let mut sources = Vec::new();
    rust_sources_below(&root.join("crates"), &mut sources);
    sources.sort();
    for path in sources {
        let source = fs::read_to_string(&path).expect("read first-party Rust source");
        for declared in foreign_declarations(&source) {
            assert!(
                !exported.contains(&declared),
                "{} declares {declared}, which this workspace defines; private code must call the typed implementation instead of re-entering the exported symbol",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
        }
    }
}
