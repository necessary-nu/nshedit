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
| Linux (`x86_64-unknown-linux-gnu`) | Supported and conformance-gated | ELF shared/static library, generated headers, `libedit` compatibility names, and end-to-end loader tests |
| Linux (`x86_64-unknown-linux-musl`) | Supported and cross-check-gated | The same ELF product, built with `crt-static` off and gated on a musl host |
| macOS (`x86_64` and Apple silicon) | Supported and native-acceptance-gated | Mach-O shared/static library, generated headers, `libedit` compatibility names, and native install, link, and runtime tests |
| Windows | Supported and native-acceptance-gated | Not provided; the libedit C ABI remains POSIX-only |

The Linux rows are an exact target contract, not an `any Linux` promise.
Other architectures, x32, and Android are unsupported and rejected at compile
time; adding one requires its own platform layouts and acceptance evidence.

The two Linux rows are both x86-64 and differ only in C library. Every record
layout the platform crate transcribes is asserted separately under each, and
the checks that read expected values out of C headers read the headers of the
library the *target* links. musl needs one build setting the glibc target does
not: `crt-static` is on by default there, and rustc emits no `cdylib` at all
while it is, so the C compatibility product is built with
`RUSTFLAGS='-C target-feature=-crt-static'`. See
[`plan/decisions/linux-musl-target.md`](plan/decisions/linux-musl-target.md).

The exported Readline functions are **libedit's Readline compatibility
surface**, not a complete implementation of GNU Readline. In particular,
nshedit does not install itself as `libreadline.so.8`.

The project is pre-release (`0.0.0`). Pin a Git revision when using it as a
dependency.

## Quick start

With a current stable Rust toolchain installed:

```sh
git clone https://github.com/necessary-nu/nshedit.git
cd nshedit
cargo run -p nshedit --example repl
```

The repository deliberately has no toolchain override. The native crates
(`nshedit`, `nshedit-plat`, and `nshterm`) build on current stable Rust and do
not declare an MSRV. Only `nshedit-abi` declares an MSRV: Rust 1.99, for its
C-variadic exports. On a host whose default stable toolchain is older than
1.99, use an installed current nightly explicitly for commands that select the
ABI crate or the whole workspace. Once the selected stable compiler is 1.99 or
newer, the corresponding unqualified `cargo` commands work as well.

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
cargo +nightly build -p nshedit-abi --release
```

On either supported Linux target this produces
`target/release/libnshedit.so`; on macOS it produces
`target/release/libnshedit.dylib`. Every supported POSIX target also produces
`target/release/libnshedit.a`. On musl, add
`RUSTFLAGS='-C target-feature=-crt-static'`: without it rustc drops the
`cdylib` crate type and the build succeeds having produced only the static
library. The committed, generated headers are:

- [`crates/nshedit-abi/include/histedit.h`](crates/nshedit-abi/include/histedit.h)
- [`crates/nshedit-abi/include/editline/readline.h`](crates/nshedit-abi/include/editline/readline.h)

The committed C export contract, shared by ELF and Mach-O builds, is
[`crates/nshedit-abi/exports.txt`](crates/nshedit-abi/exports.txt).

The installer lays out the platform's versioned library, headers, `pkg-config`
metadata, and libedit compatibility names. Linux receives `libedit.so`,
`libedit.so.0`, and `libedit.so.2`; macOS receives `libedit.dylib` and
`libedit.3.dylib`. Pass `--no-compat` to install only the `libnshedit` names.
Preview a staged installation before writing it:

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

The native crates' ordinary stable quality gates are:

```sh
cargo fmt --all -- --check
cargo clippy -p nshedit -p nshedit-plat -p nshterm --all-targets -- -D warnings
cargo build -p nshedit -p nshedit-plat -p nshterm
cargo test -p nshedit -p nshedit-plat -p nshterm --all-targets
```

The full workspace adds `nshedit-abi`. While the default stable toolchain is
older than its Rust 1.99 MSRV, run those gates with an installed nightly:

```sh
cargo +nightly clippy --workspace --all-targets -- -D warnings
cargo +nightly build --workspace
cargo +nightly test --workspace
```

Generate the native Rust API documentation with every rustdoc warning denied:

```sh
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
```

The `nshedit-abi` library target is intentionally omitted from rustdoc. It is
a C-only adapter whose public documentation is the generated headers and
export manifest above; the native `nshedit` crate owns the Rust API docs.

The ELF ABI tests run by default on a Linux host that builds a shared object.
They compare the built symbol table with the committed export contract,
compile direct C consumers against the generated headers, exercise defined
handling of historically unsafe inputs, and verify the staged installer and
loader layout.

For the same checks with a stage-by-stage report:

```sh
rustup run nightly ./conformance/run.sh
```

The full conformance harness requires a supported Linux host. It expects a C
compiler, `pkg-config`, standard ELF/binutils tools, and installed terminfo
entries. It does not require Autotools or a system libedit. The `rustup run`
wrapper selects nightly for the unqualified workspace Cargo command inside the
script; it is unnecessary when the default compiler is Rust 1.99 or newer.
Set `NSHEDIT_TARGET` to a triple to point every stage — the installer
included — at a cross build in `target/<triple>/debug` instead.

From a glibc development host, compile every crate and test target for musl
and run the suite, which `crt-static` makes static binaries that any Linux
host executes:

```sh
rustup run nightly ./ci/musl-cross-check.sh
```

The C compatibility product needs a musl host, because the object is
dynamically linked against that system's own musl. On one — an Alpine
container is enough — this builds it with `crt-static` off, checks that it
depends on musl and on no glibc name, and runs the same conformance stages:

```sh
./ci/musl-acceptance.sh
```

From a Linux development host, compile every workspace crate and test target
for both supported Darwin architectures with:

```sh
rustup run nightly ./ci/darwin-cross-check.sh
```

Native macOS acceptance builds and tests the workspace, checks the Mach-O
export set and install name, stages both installer modes, and compiles, links,
and runs the unchanged C consumer through `-ledit`:

```sh
rustup run nightly ./ci/macos-acceptance.sh
```

CI runs that script on both Apple silicon and Intel macOS hosts. Windows CI
likewise exercises the native editor against a real console, ConPTY, and
redirected streams with:

```sh
./ci/windows-acceptance.sh
```

The Windows script requires a Windows host and `NSHEDIT_REPL_EXE` pointing to
the built `repl.exe`, as configured by the workflow.

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
available under the [BSD 3-Clause License](LICENSE).

`nshterm` is derived from Rust's `term` crate and remains dual-licensed under
[MIT](crates/nshterm/LICENSE-MIT) or
[Apache-2.0](crates/nshterm/LICENSE-APACHE).
