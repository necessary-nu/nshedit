#!/usr/bin/env bash
#
# Native macOS acceptance for the Rust workspace and shipped C interface.

set -euo pipefail

if [[ $(uname -s) != Darwin ]]; then
    printf 'macOS acceptance: this command requires a macOS host\n' >&2
    exit 1
fi

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

expected=$root/crates/nshedit-abi/exports.txt
dylib=$root/target/debug/libnshedit.dylib
out=$root/target/macos-acceptance
actual=$out/exports.actual
prefix=$out/prefix
bare=$out/prefix-bare
consumer=$out/header-consumer
runtime_name=libnshedit.0.dylib
install_name=@rpath/$runtime_name

fail() {
    printf 'macOS acceptance: %s\n' "$*" >&2
    exit 1
}

note() {
    printf 'macOS acceptance: %s\n' "$*"
}

# [spec:nshedit:req:abi.darwin-drop-in]
# [spec:nshedit:req:abi.darwin-drop-in/test]
cargo build --workspace
cargo test --workspace

[ -f "$dylib" ] || fail "cargo did not produce $dylib"
command -v nm >/dev/null || fail "nm is required"
command -v otool >/dev/null || fail "otool is required"
command -v cc >/dev/null || fail "a C compiler is required"

mkdir -p -- "$out"
LC_ALL=C nm -gjU "$dylib" | sed 's/^_//' | LC_ALL=C sort -u > "$actual"
LC_ALL=C sort -c "$expected" || fail "$expected must remain sorted"
diff -u -- "$expected" "$actual" || fail "the Mach-O export set differs from the contract"
note "$(wc -l < "$actual" | tr -d ' ') Mach-O exports match"

have_install_name=$(otool -D "$dylib" | awk 'NR == 2 { print $1 }')
[ "$have_install_name" = "$install_name" ] ||
    fail "install name is ${have_install_name:-<none>}, expected $install_name"
note "install name is $install_name"

rm -rf -- "$prefix" "$bare" "$consumer"
./packaging/install.sh --prefix "$prefix" --profile debug

[ "$(readlink "$prefix/lib/$runtime_name")" = libnshedit.0.0.0.dylib ] ||
    fail "$runtime_name does not point at the installed object"
[ "$(readlink "$prefix/lib/libnshedit.dylib")" = "$runtime_name" ] ||
    fail "libnshedit.dylib does not point at $runtime_name"
for name in libedit.dylib libedit.3.dylib; do
    [ "$(readlink "$prefix/lib/$name")" = "$runtime_name" ] ||
        fail "$name does not point at $runtime_name"
done
note "the native and libedit compatibility link names resolve to one object"

cc -std=c11 -O0 -g -Wall -Wextra -Werror \
    -I"$prefix/include" \
    "$root/conformance/fixtures/header_consumer.c" \
    -L"$prefix/lib" -Wl,-rpath,"$prefix/lib" -ledit \
    -o "$consumer"

consumer_dependency=$(
    otool -L "$consumer" |
        awk 'NR > 1 { print $1 }' |
        grep -E 'lib(nsh)?edit' || true
)
[ "$consumer_dependency" = "$install_name" ] ||
    fail "consumer records ${consumer_dependency:-no nshedit dependency}, expected $install_name"
"$consumer" "$out"
note "the unchanged generated headers compile, link through -ledit, and run"

./packaging/install.sh --prefix "$bare" --profile debug --no-compat
[ -e "$bare/lib/$runtime_name" ] || fail "--no-compat dropped $runtime_name"
for name in libedit.dylib libedit.3.dylib; do
    [ ! -e "$bare/lib/$name" ] || fail "--no-compat installed $name"
done
note "--no-compat installs no libedit link names"
