#!/usr/bin/env bash
#
# The conformance harness, end to end. This is the entry point; everything
# else in this directory is a stage of it.
#
#   ./conformance/run.sh
#
# Stages, in dependency order:
#
#   1. cargo build            the port's libnshedit.so, which stage 3 links
#   2. build-oracle.sh        the in-tree C, built out of tree as the reference
#   3. abi-shape.sh           exported symbols: port vs oracle vs libedit.so.2
#                             vs libreadline.so.8
#   4. determinism.sh         every trace is byte-identical across runs
#   5. differential.sh        both libraries driven through one C driver,
#                             traces diffed per operation
#   6. vis-cross.sh           the in-tree vis(3) against libbsd's, which is
#                             what wrote the history files already on disk
#   7. header-diff.sh         the headers we generate from our own Rust
#                             against libedit's, which is the only check in
#                             the harness that sees a struct LAYOUT
#
# Each stage prints its own report and the summary at the end says which
# passed. A non-zero exit means at least one stage found something; read the
# stage's report for what, and read it against docs/errata.md before
# concluding it is a port bug — the register says which defects of the C the
# port reproduces deliberately.
#
# WHY THE ORACLE IS THE C IN THIS TREE. Debian ships libedit 3.1-20250104 as
# libedit.so.2.0.75. This tree is libedit-20260512-3.1 at LT_VERSION 0:78:0,
# about sixteen months newer. Measuring against the system library would
# attribute upstream's changes to the port, so the reference is built from
# the same source the port was translated from. The system libraries are used
# only in stage 3, and only to answer the different question of what a
# deployed consumer can actually reach.
#
# NOTHING HERE NEEDS A TERMINAL. Every stage runs headless.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

declare -a names=() results=()
overall=0

stage() {
    local label=$1; shift
    printf '\n########## %s ##########\n' "$label"
    "$@"
    local rc=$?
    names+=("$label")
    if [ "$rc" -eq 0 ]; then results+=("PASS"); else results+=("FAIL"); overall=1; fi
    return 0
}

printf '########## build the port ##########\n'
if ! (cd -- "$ROOT" && cargo build --workspace); then
    die "cargo build failed; the differential has nothing to link against"
fi

stage "conformance-oracle"       "$CONF_DIR/build-oracle.sh"
stage "conformance-abi-shape"    "$CONF_DIR/abi-shape.sh"
stage "determinism"              "$CONF_DIR/determinism.sh"
stage "conformance-differential" "$CONF_DIR/differential.sh"
stage "vis vs libbsd"            "$CONF_DIR/vis-cross.sh"
stage "conformance-header-diff"  "$CONF_DIR/header-diff.sh"

printf '\n########## summary ##########\n'
for i in "${!names[@]}"; do
    printf '  %-4s %s\n' "${results[$i]}" "${names[$i]}"
done
printf '\nreports: %s\n' "$REPORTS"
if [ "$overall" -ne 0 ]; then
    printf '\nAt least one stage found a difference. Read it against docs/errata.md\n'
    printf 'before changing any Rust: a divergence that matches a registered\n'
    printf 'defect is the harness working, not a bug to fix.\n'
fi
exit "$overall"
