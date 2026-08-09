#!/usr/bin/env bash
#
# Verify the ELF artifact against nshedit's committed C export contract.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

EXPECTED=$ROOT/crates/nshedit-abi/exports.txt
REPORT=$REPORTS/abi-shape
ACTUAL=$REPORT/exports.actual

[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
[ -f "$EXPECTED" ] || die "missing export contract: $EXPECTED"
command -v nm >/dev/null || die "nm is required"
command -v readelf >/dev/null || die "readelf is required"

mkdir -p -- "$REPORT"

LC_ALL=C sort -c "$EXPECTED" ||
    die "$EXPECTED must remain sorted"

LC_ALL=C nm -D --defined-only "$PORT_LIB" |
    awk 'NF >= 3 { print $3 }' |
    LC_ALL=C sort -u > "$ACTUAL"

if ! diff -u -- "$EXPECTED" "$ACTUAL"; then
    die "the exported ABI differs from the committed contract"
fi

soname=$(
    readelf -d "$PORT_LIB" 2>/dev/null |
        sed -n 's/.*Library soname: \[\(.*\)\].*/\1/p'
)
[ "$soname" = "$PORT_SONAME" ] ||
    die "$PORT_LIB carries SONAME '${soname:-<none>}', expected $PORT_SONAME"

note "$(wc -l < "$ACTUAL") exports match; SONAME is $PORT_SONAME"
