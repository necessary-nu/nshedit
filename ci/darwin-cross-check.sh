#!/usr/bin/env bash
#
# Compile every workspace crate and test target for both supported Darwin
# architectures from the Linux development host:
#
#   ./ci/darwin-cross-check.sh

set -euo pipefail

if [[ $(uname -s) != Linux ]]; then
    printf 'darwin cross-check: this command is for the Linux development host\n' >&2
    exit 1
fi

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"

# [spec:nshedit:req:workspace.darwin-cross-check]
for target in aarch64-apple-darwin x86_64-apple-darwin; do
    printf 'darwin cross-check: %s\n' "$target"
    cargo check --workspace --all-targets --target "$target"
done
