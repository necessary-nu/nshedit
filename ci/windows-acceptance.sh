#!/usr/bin/env bash
# Run the Windows-only real-console, ConPTY, and redirected-stream acceptance.

set -euo pipefail

if [[ ${OS:-} != Windows_NT ]]; then
    printf 'windows acceptance: this command requires a Windows host\n' >&2
    exit 1
fi

: "${NSHEDIT_REPL_EXE:?set NSHEDIT_REPL_EXE to the absolute repl.exe path}"

cargo build --package nshedit --example repl

# [spec:nshedit:req:workspace.windows-acceptance]
cargo test \
    --package nshedit \
    --package nshedit-plat \
    --package nshterm \
    --all-targets \
    -- \
    --test-threads=1
