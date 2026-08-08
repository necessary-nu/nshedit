//! The format gate names its packages; this fails when that list rots.
//!
//! `cargo fmt --all` formats workspace members *and their local path-based
//! dependencies*, which is documented behaviour and wrong for us: the
//! optional `bsd` dependency is a path into a sibling checkout. Under
//! `--all`, the format check reported diffs in somebody else's repository —
//! making the gate depend on when they last saved — and `cargo fmt --all` as
//! the "fix" command would have rewritten their files.
//!
//! `cargo fmt` has no `--exclude`, so the boundary is an explicit `-p` list
//! in `.config/nplan/config.styx`. An explicit list is a list that can go
//! stale: add a crate, forget the config, and it is silently never checked.
//! This is the thing that notices.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/nshedit-abi is two levels below the repo root")
        .to_path_buf()
}

/// The `members = [...]` list from the workspace manifest, by crate name.
fn workspace_members(root: &Path) -> Vec<String> {
    let toml = fs::read_to_string(root.join("Cargo.toml")).expect("workspace Cargo.toml");
    let list = toml
        .split_once("members = [")
        .expect("workspace has no members list")
        .1
        .split_once(']')
        .expect("unterminated members list")
        .0;
    let mut names: Vec<String> = list
        .split(',')
        .filter_map(|entry| {
            let path = entry.trim().trim_matches('"');
            (!path.is_empty()).then(|| {
                path.rsplit('/')
                    .next()
                    .expect("a member path has a last component")
                    .to_owned()
            })
        })
        .collect();
    names.sort();
    names
}

/// The `-p NAME` arguments in the configured format command.
fn format_gate_packages(root: &Path) -> Vec<String> {
    let cfg = fs::read_to_string(root.join(".config/nplan/config.styx"))
        .expect(".config/nplan/config.styx");
    let cmd = cfg
        .lines()
        .find(|l| l.contains("cmd \"cargo fmt"))
        .expect("no format check configured");
    let words: Vec<&str> = cmd.split_whitespace().collect();
    let mut names: Vec<String> = words
        .windows(2)
        .filter(|w| w[0] == "-p")
        .map(|w| w[1].to_owned())
        .collect();
    names.sort();
    names
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

#[test]
fn the_format_gate_covers_every_workspace_member() {
    let root = repo_root();
    let members = workspace_members(&root);
    let gated = format_gate_packages(&root);

    assert!(
        !members.is_empty() && !gated.is_empty(),
        "parsed nothing: members={members:?} gated={gated:?}"
    );
    assert_eq!(
        gated,
        members,
        "the format gate's -p list has drifted from the workspace members.\n\
         Update the `format` check in .config/nplan/config.styx.\n\
         Not covered: {:?}\n\
         Named but not a member: {:?}",
        members
            .iter()
            .filter(|m| !gated.contains(m))
            .collect::<Vec<_>>(),
        gated
            .iter()
            .filter(|g| !members.contains(g))
            .collect::<Vec<_>>(),
    );
}

/// `--all` must not come back. It is the exact thing this boundary exists to
/// prevent, and it looks more correct than what replaced it.
#[test]
fn the_format_gate_does_not_use_all() {
    let cfg = fs::read_to_string(repo_root().join(".config/nplan/config.styx"))
        .expect(".config/nplan/config.styx");
    // Command lines only. The comment above the check explains why `--all`
    // is wrong and therefore contains it, which is not a violation.
    for line in cfg
        .lines()
        .map(str::trim)
        .filter(|l| (l.starts_with("cmd \"") || l.starts_with("fix \"")) && l.contains("cargo fmt"))
    {
        assert!(
            !line.contains("--all"),
            "`cargo fmt --all` reaches path dependencies outside this \
             workspace — see the header of this file: {line}"
        );
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
