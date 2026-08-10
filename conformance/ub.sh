#!/usr/bin/env bash
#
# Assert that public C inputs which were historically unsafe are now defined.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

SRC=$CONF_DIR/fixtures/ub_corpus.c
OUT_DIR=$OUT/ub
REPORT=$REPORTS/ub
BIN=$OUT_DIR/ub

[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
command -v "$CC" >/dev/null || die "C compiler '$CC' is required"
command -v timeout >/dev/null || die "timeout is required"

stage_port_soname
rm -rf -- "$OUT_DIR" "$WORK"
mkdir -p -- "$OUT_DIR" "$REPORT" "$WORK/home" "$WORK/tmp" "$WORK/data"

"$CC" -std=c11 -O0 -g -Wall -Wextra -Wno-unused-parameter \
    -I"$ROOT/crates/nshedit-abi/include" \
    "$SRC" \
    -L"$PORT_LIB_DIR" -Wl,-rpath,"$PORT_LIB_DIR" -lnshedit \
    -o "$BIN"

if ! timeout 300 env -i \
        LC_ALL=C TERM=dumb HOME="$WORK/home" TMPDIR="$WORK/tmp" \
        COLUMNS=80 LINES=24 PATH=/usr/bin:/bin \
        LD_LIBRARY_PATH="$PORT_LIB_DIR" \
        "$BIN" "$WORK/data" \
        </dev/null >"$REPORT/port.trace" 2>"$REPORT/port.stderr"; then
    die "the undefined-input driver failed or timed out; see $REPORT"
fi

cat "$REPORT/port.trace"

status=0
cases=0
while IFS= read -r line; do
    case "$line" in
        *" done "*) continue ;;
        [0-9]*)
            cases=$((cases + 1))
            case "$line" in
                *" survived exit="*) ;;
                *)
                    printf 'FAIL: %s\n' "$line" >&2
                    status=1
                    ;;
            esac
            ;;
    esac
done < "$REPORT/port.trace"

[ "$cases" -gt 0 ] || die "the undefined-input corpus ran no cases"
if [ "$status" -eq 0 ]; then
    note "$cases unsafe-input cases completed without a signal or hang"
fi
exit "$status"
