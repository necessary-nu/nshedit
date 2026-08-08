// How the shipped headers are generated. `include!`d by
// `examples/gen-headers.rs`, which writes them, and by `tests/headers.rs`,
// which checks the committed copies against a fresh generation.
//
// Shared rather than written twice on purpose: a generator and a test of the
// generator that each carried their own idea of what to generate would be
// two artifacts obliged to agree, which is the thing this whole approach
// exists to avoid.
//
// # Why a file list and not a crate
//
// cbindgen is pointed at *source files*, not at the crate. libedit installs
// two headers and this crate exports three surfaces — the third being
// `chartype.h` and `filecomplete.h`, which libedit compiles in and does not
// install, so their seven exported symbols are reachable by a consumer that
// declares them and appear in no header. Parsing per file is what splits the
// output the way libedit splits it, with no list of names to keep in step:
// each header is generated from exactly the modules that own its surface,
// and `crates/nshedit-abi/src/{chartype,filecomplete}.rs` are simply not
// parsed.

#[cfg(not(test))]
use std::path::Path;
use std::path::PathBuf;

/// One generated header: where it goes, how it is configured, and the source
/// files it is generated from.
struct Header {
    /// Path of the generated file, relative to the output directory.
    out: &'static str,
    /// The cbindgen config, relative to this crate.
    config: &'static str,
    /// Sources, relative to the workspace root. Order decides declaration
    /// order in the output and nothing else; the comparison in
    /// `conformance/header-diff.sh` is a set.
    srcs: &'static [&'static str],
}

const HEADERS: &[Header] = &[
    Header {
        out: "histedit.h",
        config: "cbindgen/histedit.toml",
        srcs: &[
            // The incomplete handle tags.
            "crates/nshedit-abi/src/cdecl/handles.rs",
            // Every completed record, callback spelling, and `CC_*`/`H_*`
            // value. Header generation deliberately reads no core source.
            "crates/nshedit-abi/src/cdecl/histedit.rs",
            // The exported functions, and the `EL_*` opcodes.
            "crates/nshedit-abi/src/histedit.rs",
            // The nine `histedit.h` entry points `eln.c` defines, which Rust
            // cannot define twice and so keeps in a module of their own.
            "crates/nshedit-abi/src/eln.rs",
        ],
    },
    Header {
        out: "editline/readline.h",
        config: "cbindgen/readline.toml",
        srcs: &[
            "crates/nshedit/src/editline/readline.rs",
            "crates/nshedit-abi/src/cdecl/readline.rs",
            "crates/nshedit-abi/src/readline.rs",
            "crates/nshedit-abi/src/readline/history_io.rs",
        ],
    },
];

/// This crate's directory, and the workspace root above it.
fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Where the committed headers live, and what a consumer's `-I` points at.
fn shipped_dir() -> PathBuf {
    crate_dir().join("include")
}

/// Generates every header, in memory. Returns `(relative path, contents)`.
///
/// Nothing here touches the filesystem, so the test can compare without
/// writing and the writer below cannot produce anything the test did not see.
fn generate() -> Vec<(&'static str, String)> {
    let crate_dir = crate_dir();
    let workspace = crate_dir.join("../..");

    HEADERS
        .iter()
        .map(|header| {
            let config = cbindgen::Config::from_file(crate_dir.join(header.config))
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", header.config));

            let mut builder = cbindgen::Builder::new().with_config(config);
            for src in header.srcs {
                let path = workspace.join(src);
                assert!(path.exists(), "no such source file: {}", path.display());
                builder = builder.with_src(path);
            }

            let bindings = builder
                .generate()
                .unwrap_or_else(|e| panic!("cbindgen failed for {}: {e}", header.out));

            let mut buf = Vec::new();
            bindings.write(&mut buf);
            let text = String::from_utf8(buf).expect("cbindgen emitted invalid UTF-8");
            (header.out, text)
        })
        .collect()
}

/// Writes what [`generate`] produced under `out_root`, creating directories.
// `tests/headers.rs` includes this file and compares without writing.
#[cfg(not(test))]
fn write_headers(out_root: &Path) {
    for (rel, text) in generate() {
        let out = out_root.join(rel);
        std::fs::create_dir_all(out.parent().expect("header path has a parent"))
            .expect("could not create the output directory");
        std::fs::write(&out, text).unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
        eprintln!("wrote {}", out.display());
    }
}
