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
fn the_core_surface_is_safe() {
    let root = repo_root();
    let source = fs::read_to_string(root.join("crates/nshedit/src/lib.rs"))
        .expect("read the editor core facade");

    assert!(
        source.contains("#![forbid(unsafe_code)]"),
        "the editor core must reject unsafe declarations and implementations"
    );
    assert!(
        !source.contains("#[path"),
        "the core facade must not compile C-boundary sources by path"
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
        "the core facade exposed a module outside the semantic Rust API"
    );
}

// [spec:nshedit:req:core.no-compat-internals/test]
#[test]
fn the_core_has_no_c_boundary() {
    let root = repo_root();
    let core = root.join("crates/nshedit/src");
    let abi = root.join("crates/nshedit-abi/src");

    assert!(
        !abi.join("compat.rs").exists() && !abi.join("compat").exists(),
        "a C-shaped implementation must not remain disconnected behind the ABI"
    );

    let mut sources = Vec::new();
    rust_sources_below(&core, &mut sources);
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
        let source = fs::read_to_string(&path).expect("read an editor core source");
        for spelling in forbidden {
            assert!(
                !source.contains(spelling),
                "{} contains the C-boundary spelling {spelling:?}",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
        }
    }
}

// [spec:nshedit:req:workspace.no-legacy-allows/test]
// [spec:nshedit:req:workspace.lint-policy+1/test]
#[test]
fn first_party_rust_rejects_suppressions() {
    let root = repo_root();
    let mut sources = Vec::new();
    rust_sources_below(&root.join("crates"), &mut sources);
    sources.sort();

    // Assembled so this test does not report its own search strings.
    // Whitespace and line comments are discarded first, so a split or
    // formatted attribute cannot evade the check.
    let allow = ["allow", "("].concat();
    let expect = ["expect", "("].concat();
    let item = |attribute: &str| ["#", "[", attribute].concat();
    let inner = |attribute: &str| ["#", "!", "[", attribute].concat();
    let conditional = ["#", "[", "cfg_attr", "("].concat();

    for path in sources {
        let source = fs::read_to_string(&path).expect("read first-party Rust source");
        let code: String = source
            .lines()
            .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
            .flat_map(str::chars)
            .filter(|character| !character.is_whitespace())
            .collect();

        for suppression in [&allow, &expect] {
            assert!(
                !code.contains(&item(suppression)) && !code.contains(&inner(suppression)),
                "{} suppresses a lint; represent the constraint so the lint does not arise, or move it outside first-party source",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
        }

        let mut conditional_attributes = code.as_str();
        while let Some(start) = conditional_attributes.find(&conditional) {
            conditional_attributes = &conditional_attributes[start + conditional.len()..];
            let end = conditional_attributes
                .find(")]")
                .expect("a Rust cfg_attr attribute has no closing delimiter");
            let attribute = &conditional_attributes[..end];
            assert!(
                !attribute.contains(&allow) && !attribute.contains(&expect),
                "{} conditionally suppresses a lint; conditional code must stay warning-clean too",
                path.strip_prefix(&root).unwrap_or(&path).display()
            );
            conditional_attributes = &conditional_attributes[end + 2..];
        }
    }
}

/// The lints the workspace selects for every crate, which is what makes the
/// suppression ban enforceable rather than aspirational: with these denied
/// and no suppression allowed, the only way past one is to fix the code.
// [spec:nshedit:req:workspace.lint-policy+1/test]
#[test]
fn every_crate_denies_the_workspace_lints() {
    let root = repo_root();
    let workspace =
        fs::read_to_string(root.join("Cargo.toml")).expect("read the workspace manifest");
    for lint in [
        "dead_code",
        "unused",
        "nonstandard_style",
        "unsafe_op_in_unsafe_fn",
        "missing_safety_doc",
        "allow_attributes",
    ] {
        assert!(
            workspace.contains(&format!("{lint} = \"deny\"")),
            "the workspace stopped denying {lint}"
        );
    }

    for crate_directory in fs::read_dir(root.join("crates")).expect("read the crate directory") {
        let path = crate_directory.expect("read a crate entry").path();
        if !path.is_dir() {
            continue;
        }
        let manifest = fs::read_to_string(path.join("Cargo.toml")).expect("read a crate manifest");
        assert!(
            manifest.contains("workspace = true"),
            "{} does not take the workspace lints",
            path.strip_prefix(&root).unwrap_or(&path).display()
        );
    }
}

/// Code — not prose — carrying one of the labels a migration uses to say
/// "the one that came after". They name nothing on their own: every
/// implementation here is the only one, so `native` distinguishes it from
/// nothing, and a reader cannot tell what a `legacy` value holds.
const MIGRATION_LABELS: [&str; 4] = ["native", "legacy", "compat", "translated"];

/// `line` with its comment and its string literals removed, leaving the code
/// a name would have to be declared in.
fn code_of(line: &str) -> String {
    let line = line.split_once("//").map_or(line, |(code, _)| code);
    let mut code = String::with_capacity(line.len());
    let mut quoted = false;
    let mut escaped = false;
    for character in line.chars() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            _ if quoted => {}
            _ => code.push(character),
        }
    }
    code
}

// [spec:nshedit:req:workspace.semantic-naming/test]
#[test]
fn no_identifier_carries_a_migration_label() {
    let root = repo_root();
    let mut sources = Vec::new();
    rust_sources_below(&root.join("crates"), &mut sources);
    sources.sort();

    for path in sources {
        // This file names the labels in order to look for them.
        if path.ends_with("workspace_hygiene.rs") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("read first-party Rust source");
        for (number, line) in source.lines().enumerate() {
            let code = code_of(line).to_lowercase();
            for label in MIGRATION_LABELS {
                assert!(
                    !code.contains(label),
                    "{}:{} names something {label:?}; name it for what it is responsible for",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    number + 1
                );
            }
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
