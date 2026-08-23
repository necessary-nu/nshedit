#!/usr/bin/env bash
#
# Linux C ABI acceptance, end to end.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

host=$(rustc -vV | sed -n 's/^host: //p')
target=${NSHEDIT_TARGET:-$host}
case $target in
    x86_64-unknown-linux-gnu | x86_64-unknown-linux-musl) ;;
    *)
        die "the supported Linux targets are x86_64-unknown-linux-gnu and \
x86_64-unknown-linux-musl, not ${target:-unknown}"
        ;;
esac

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
if [ -n "$NSHEDIT_TARGET" ]; then
    # Someone else built these. A musl cdylib exists only with crt-static
    # turned off and a musl-capable linker selected, which is
    # ci/musl-acceptance.sh's business rather than a flag this script can
    # guess on the caller's behalf.
    [ -f "$PORT_LIB" ] ||
        die "no $PORT_LIB — build for $NSHEDIT_TARGET before running this"
    note "inspecting the $NSHEDIT_TARGET build in $PORT_LIB_DIR"
else
    (cd -- "$ROOT" && cargo build --workspace) ||
        die "cargo build failed"
fi

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
