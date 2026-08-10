#!/usr/bin/env bash
#
# Compile the native Rust product for both supported 64-bit Windows toolchains
# from a Linux development host:
#
#   ./ci/windows-cross-check.sh

set -euo pipefail

if [[ $(uname -s) != Linux ]]; then
    printf 'windows cross-check: this command is for the Linux development host\n' >&2
    exit 1
fi

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

packages=(--package nshedit --package nshedit-plat --package nshterm)

# [spec:nshedit:req:workspace.windows-native-build]
printf 'windows cross-check: x86_64-pc-windows-msvc via cargo-xwin\n'
cargo xwin check "${packages[@]}" --all-targets --target x86_64-pc-windows-msvc

printf 'windows cross-check: x86_64-pc-windows-gnu\n'
cargo check "${packages[@]}" --all-targets --target x86_64-pc-windows-gnu
