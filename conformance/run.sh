#!/usr/bin/env bash
#
# Linux C ABI acceptance, end to end.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

declare -a names=()
declare -a results=()
overall=0

stage() {
    local label=$1
    shift
    printf '\n########## %s ##########\n' "$label"
    names+=("$label")
    if "$@"; then
        results+=("PASS")
    else
        results+=("FAIL")
        overall=1
    fi
}

printf '########## build ##########\n'
(cd -- "$ROOT" && cargo build --workspace) ||
    die "cargo build failed"

stage "export contract" "$CONF_DIR/abi-shape.sh"
stage "generated C headers" "$CONF_DIR/c-abi.sh"
stage "install and loader" "$CONF_DIR/soname.sh"
stage "defined unsafe inputs" "$CONF_DIR/ub.sh"

printf '\n########## summary ##########\n'
for i in "${!names[@]}"; do
    printf '  %-4s %s\n' "${results[$i]}" "${names[$i]}"
done
printf '\nreports: %s\n' "$REPORTS"
exit "$overall"
