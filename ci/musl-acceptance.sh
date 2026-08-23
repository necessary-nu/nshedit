#!/usr/bin/env bash
#
# Native musl acceptance for the Rust workspace and shipped C interface.
#
# This is the musl counterpart of ci/macos-acceptance.sh: it runs on a host
# whose own C library is musl — Alpine, or any other musl distribution — and
# gates the artifact a musl system would actually install.
#
# ci/musl-cross-check.sh already compiles every crate for the musl target from
# a glibc development host and runs the test suite there as static binaries.
# The one thing it cannot cover is the C compatibility product: a musl target
# links crt-static by default, and rustc emits no cdylib at all while that is
# on. So the shared library is built here with crt-static off, which is what a
# libnshedit.so on a musl system is — an ELF object dynamically linked against
# the system's own musl — and then handed to the same conformance stages the
# glibc target is gated on.

set -euo pipefail

fail() {
    printf 'musl acceptance: %s\n' "$*" >&2
    exit 1
}

note() {
    printf 'musl acceptance: %s\n' "$*"
}

host=$(rustc -vV | sed -n 's/^host: //p')
case $host in
    *-linux-musl) ;;
    *) fail "this command requires a musl host, not ${host:-unknown}" ;;
esac

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

lib=$root/target/debug/libnshedit.so

command -v readelf > /dev/null || fail "readelf is required"
command -v cc > /dev/null || fail "a C compiler is required"

# Off for every target in this run, so the workspace, its tests and the
# cdylib are one consistent dynamically linked build.
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=-crt-static"

# [spec:nshedit:req:abi.musl-drop-in]
# [spec:nshedit:req:abi.musl-drop-in/test]
cargo build --workspace
cargo test --workspace

[ -f "$lib" ] || fail "cargo did not produce $lib
(rustc drops the cdylib when crt-static is on)"

# What the object depends on is the whole claim being made here: this is a
# musl library. A glibc name among its DT_NEEDED entries would mean the build
# resolved -lc against the wrong C library, which produces an object that
# links and then fails to load on the system it was built for.
needed=$(readelf -d "$lib" | sed -n 's/.*Shared library: \[\(.*\)\].*/\1/p')
for entry in $needed; do
    case $entry in
        libc.so.6 | ld-linux-*.so.* | libm.so.6 | libpthread.so.0 | libdl.so.2)
            fail "$lib records the glibc dependency $entry"
            ;;
    esac
done
printf '%s\n' "$needed" | grep -q -E '^(libc\.musl-.*\.so\.[0-9]+|libc\.so)$' ||
    fail "$lib records no musl C library: ${needed:-no dependencies at all}"
note "the cdylib depends on musl: $(printf '%s' "$needed" | tr '\n' ' ')"

# The same stages the glibc target is gated on: the export contract and
# SONAME, the generated headers against a real C compiler, the installer,
# compatibility names, pkg-config metadata and a linked C consumer, and the
# defined results for unsafe C inputs.
#
# `cargo test` above already ran these through the crate's conformance test,
# which is how they are gated on an ordinary `cargo test` run. Running them
# again here is deliberate: this is the stage-by-stage report a person reads
# when the gate fails, rather than one assertion's captured output.
./conformance/run.sh
