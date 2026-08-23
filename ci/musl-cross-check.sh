#!/usr/bin/env bash
#
# Compile and run the native Rust product for x86_64-unknown-linux-musl from
# the Linux development host:
#
#   ./ci/musl-cross-check.sh
#
# musl is the second supported Linux ABI, and the layouts nshedit-plat spells
# for it — struct passwd, struct sigaction, sigset_t, the V* subscripts and
# the terminal flag words — are asserted by the compiler and checked against
# musl's own headers. Both only happen when musl is the target, so this is the
# gate that keeps work done on glibc from breaking it.
#
# A musl target links crt-static by default, so what `cargo test` builds here
# is a statically linked binary that this host runs natively: these are real
# executions on musl, not a compile-only cross-check. The one thing crt-static
# excludes is the cdylib — rustc drops that crate type outright — so the C
# compatibility product is not covered here at all. That needs a musl host and
# is ci/musl-acceptance.sh's job.

set -euo pipefail

if [[ $(uname -s) != Linux ]]; then
    printf 'musl cross-check: this command is for the Linux development host\n' >&2
    exit 1
fi

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

target=x86_64-unknown-linux-musl

if ! rustc --print target-libdir --target "$target" > /dev/null 2>&1; then
    printf 'musl cross-check: install the target first: rustup target add %s\n' \
        "$target" >&2
    exit 1
fi

# [spec:nshedit:req:workspace.musl-cross-check]
printf 'musl cross-check: compiling every crate and test target for %s\n' "$target"
cargo check --workspace --all-targets --target "$target"

printf 'musl cross-check: running the test suite as static musl binaries\n'
cargo test --workspace --target "$target"
