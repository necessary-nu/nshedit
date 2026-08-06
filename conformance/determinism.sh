#!/usr/bin/env bash
#
# Determinism: a trace that varies run to run makes the differential
# meaningless, so this checks the property directly rather than assuming it.
#
# Each driver is run three times against each library under each locale, from
# a freshly wiped work directory, and the three traces must be byte-identical.
# Three rather than two because a first-run/second-run difference (a file left
# behind, a lazily initialised cache) and a run-to-run difference (an address,
# a timestamp) are different bugs and both should be caught.
#
#   ./conformance/determinism.sh
#
# A failure prints the differing lines. The usual causes are a printed
# pointer, a printed path, or a fixture left over from the previous run.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

LOCALES=("C.UTF-8" "C")
REPEATS=3

[ -f "$ORACLE_PREFIX/lib/libedit.so" ] || die "no oracle — run ./conformance/build-oracle.sh"
[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
stage_port_soname

mkdir -p -- "$DRIVERS" "$REPORTS/determinism"
status=0

for src in "$CONF_DIR"/driver/*.c; do
    name=$(basename "$src" .c)

    gcc -std=c11 -O0 -g -I"$ORACLE_PREFIX/include" "$src" -o "$DRIVERS/$name.oracle" \
        -L"$ORACLE_PREFIX/lib" -ledit -Wl,-rpath,"$ORACLE_PREFIX/lib"
    gcc -std=c11 -O0 -g -I"$ORACLE_PREFIX/include" "$src" -o "$DRIVERS/$name.port" \
        -L"$PORT_LIB_DIR" -lnshedit -Wl,-rpath,"$PORT_LIB_DIR"

    for side in oracle port; do
        for locale in "${LOCALES[@]}"; do
            tag="$name.$side.${locale//./_}"
            for i in $(seq 1 "$REPEATS"); do
                prepare_work
                mkdir -p -- "$WORK/data"
                run_pinned "$locale" "$DRIVERS/$name.$side" "$WORK/data" \
                    > "$REPORTS/determinism/$tag.$i" 2>/dev/null || true
            done
            same=yes
            for i in $(seq 2 "$REPEATS"); do
                if ! cmp -s "$REPORTS/determinism/$tag.1" "$REPORTS/determinism/$tag.$i"; then
                    same=no
                    printf 'NON-DETERMINISTIC: %s, run 1 vs run %d\n' "$tag" "$i"
                    diff "$REPORTS/determinism/$tag.1" "$REPORTS/determinism/$tag.$i" \
                        | head -20 | sed 's/^/  /'
                    status=1
                fi
            done
            [ "$same" = yes ] && printf 'stable: %-28s %s runs identical\n' "$tag" "$REPEATS"
        done
    done
done

if [ "$status" -eq 0 ]; then
    printf '\nPASS: every trace is byte-identical across %s runs.\n' "$REPEATS"
else
    printf '\nFAIL: a trace varies between runs. Until this is fixed a diff\n'
    printf 'between the two libraries cannot be trusted.\n'
fi
exit "$status"
