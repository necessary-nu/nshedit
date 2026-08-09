# nshedit

`nshedit` is a safe Rust reimplementation of
[libedit](https://www.thrysoee.dk/editline/): line editing, history,
tokenization, completion, terminal rendering, and the `histedit.h` C API.

The Rust-native editor is the implementation. C representations, callbacks,
and lifetime obligations are isolated in a separate ABI adapter; operating
system calls are isolated in a small platform crate. The detailed
compatibility corpus is retained under `docs/spec/port`; there is no second C
implementation in the repository.

## Status

| Target | Rust API | C compatibility |
| --- | --- | --- |
| Linux | Supported and conformance-gated | ELF shared/static library, generated headers, `libedit` compatibility names, and end-to-end loader tests |
| macOS (`x86_64` and Apple silicon) | Cross-compiles | Darwin layouts and runtime symbols are implemented; native Mach-O install, link, and runtime acceptance is still pending |
| Windows | Planned | The native Rust surface is planned; a libedit C ABI is not |

The exported Readline functions are **libedit's Readline compatibility
surface**, not a complete implementation of GNU Readline. In particular,
nshedit does not install itself as `libreadline.so.8`.

The project is pre-release (`0.0.0`). Pin a Git revision when using it as a
dependency.

## Quick start

The repository pins the Rust toolchain it needs, including the nightly support
currently required by the C ABI crate's variadic functions. With
[`rustup`](https://rustup.rs/) installed:

```sh
git clone https://github.com/necessary-nu/nshedit.git
cd nshedit
cargo run -p nshedit --example repl
```

The example is a complete safe Rust consumer with editing, history,
completion, terminal resize handling, and explicit terminal restoration. See
[`crates/nshedit/examples/repl.rs`](crates/nshedit/examples/repl.rs).

To use the native crate directly from Git:

```toml
[dependencies]
nshedit = { git = "https://github.com/necessary-nu/nshedit" }
```

The native API is intentionally host-driven:

- `Editor<TerminalControl>` owns line state and the terminal lifecycle.
- `ReadDriver` yields typed `ReadStep` effects for input, prompts, history,
  completion, signals, and external commands.
- The embedding application performs those effects and resumes the driver.
- `Text` preserves Unicode scalar values, raw bytes, and opaque code points
  without forcing them through a C string representation.

## C library

Build the shared and static ABI artifacts with:

```sh
cargo build -p nshedit-abi --release
```

On Linux this produces `target/release/libnshedit.so` and
`target/release/libnshedit.a`. The committed, generated headers are:

- [`crates/nshedit-abi/include/histedit.h`](crates/nshedit-abi/include/histedit.h)
- [`crates/nshedit-abi/include/editline/readline.h`](crates/nshedit-abi/include/editline/readline.h)

The committed ELF export contract is
[`crates/nshedit-abi/exports.txt`](crates/nshedit-abi/exports.txt).

The Linux installer lays out the versioned library, headers, `pkg-config`
metadata, and—unless `--no-compat` is supplied—the `libedit.so`,
`libedit.so.0`, and `libedit.so.2` compatibility names. Preview a staged
installation before writing it:

```sh
./packaging/install.sh \
  --prefix "$PWD/target/stage" \
  --profile release \
  --dry-run

./packaging/install.sh \
  --prefix "$PWD/target/stage" \
  --profile release
```

Installing the compatibility names into a system library directory is meant
to shadow the system libedit for newly started processes. Use a staging prefix
first, inspect the printed links, and use `--no-compat` when only
`libnshedit` should be visible.

## Workspace

| Crate | Purpose |
| --- | --- |
| [`nshedit`](crates/nshedit) | Safe Rust editor, history, tokenizer, completion, and rendering API |
| [`nshedit-abi`](crates/nshedit-abi) | Opaque C adapter exporting the libedit and libedit-provided Readline ABIs |
| [`nshedit-plat`](crates/nshedit-plat) | Typed platform boundary for terminal, signal, and user database operations |
| [`nshterm`](crates/nshterm) | Pure-Rust terminfo discovery, parsing, capability lookup, and parameter expansion |

The detailed libedit compatibility rules live in
[`docs/spec/port`](docs/spec/port). They document the contract implemented by
the Rust crates and exercised through the generated C interface.

## Testing

The ordinary Rust quality gates are:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace
cargo test --workspace
```

The Linux ABI tests run by default. They compare the built symbol table with
the committed export contract, compile direct C consumers against the
generated headers, exercise defined handling of historically unsafe inputs,
and verify the staged installer and loader layout.

For the same checks with a stage-by-stage report:

```sh
./conformance/run.sh
```

The full conformance harness is currently Linux-oriented. It expects a C
compiler, `pkg-config`, standard ELF/binutils tools, and installed terminfo
entries. It does not require Autotools or a system libedit.

From a Linux development host, compile every workspace crate and test target
for both supported Darwin architectures with:

```sh
./ci/darwin-cross-check.sh
```

This proves the Darwin-gated code compiles; it does not replace the pending
native macOS acceptance run.

## Compatibility policy

The source of truth is the detailed compatibility corpus plus the actual Rust
implementation. Generated headers and the export manifest freeze the C-facing
shape; Rust tests and direct C consumers exercise maintained behaviour.
Reference-defined no-ops and unsupported Readline operations remain compatible
no-ops. Deliberate safety fixes and divergences are recorded in
[`docs/errata.md`](docs/errata.md).

The architecture and compatibility decisions are kept under
[`plan/decisions`](plan/decisions), and the executable specification is under
[`docs/spec`](docs/spec).

## License

`nshedit`, `nshedit-abi`, `nshedit-plat`, and the libedit-derived Rust code are
available under the [BSD 3-Clause License](COPYING).

`nshterm` is derived from Rust's `term` crate and remains dual-licensed under
[MIT](crates/nshterm/LICENSE-MIT) or
[Apache-2.0](crates/nshterm/LICENSE-APACHE).
