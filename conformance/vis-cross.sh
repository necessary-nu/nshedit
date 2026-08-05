#!/usr/bin/env bash
#
# The history file format, cross-checked against libbsd.
#
# Neither side of this comparison is the port. The differential already
# proves the port's H_SAVE output matches the in-tree C's byte for byte; this
# asks the separate question of whether the in-tree C's vis(3) agrees with
# the vis(3) that actually wrote the history files on a Debian disk.
#
#   oracle  src/vis.c, NetBSD-derived, compiled in because configure found
#           no system vis (src/Makefile.am, if !HAVE_VIS)
#   libbsd  what Debian's libedit.so.2 imports — strvis@LIBBSD_0.0
#
#   ./conformance/vis-cross.sh
#
# A difference here is not a port bug. It is a statement about what "drop-in"
# means for existing data, and it belongs in a decision rather than a code
# change.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

LOCALES=("C.UTF-8" "C")
SRC=$CONF_DIR/aux/vis_corpus.c

[ -f "$ORACLE_PREFIX/lib/libedit.so" ] || die "no oracle — run ./conformance/build-oracle.sh"
if [ ! -e /usr/lib/x86_64-linux-gnu/libbsd.so ] && [ ! -e /usr/lib/x86_64-linux-gnu/libbsd.so.0 ]; then
    note "libbsd is not installed; skipping the cross-check"
    exit 0
fi

mkdir -p -- "$DRIVERS" "$REPORTS"

gcc -std=c11 -O0 -g "$SRC" -o "$DRIVERS/vis_corpus.oracle" \
    -L"$ORACLE_PREFIX/lib" -ledit -Wl,-rpath,"$ORACLE_PREFIX/lib"

# libbsd's .so symlink needs libbsd-dev; fall back to naming the versioned
# object directly so the check still runs on a runtime-only host.
if [ -e /usr/lib/x86_64-linux-gnu/libbsd.so ]; then
    gcc -std=c11 -O0 -g "$SRC" -o "$DRIVERS/vis_corpus.libbsd" -lbsd
else
    gcc -std=c11 -O0 -g "$SRC" -o "$DRIVERS/vis_corpus.libbsd" \
        /usr/lib/x86_64-linux-gnu/libbsd.so.0
fi

status=0
for locale in "${LOCALES[@]}"; do
    tag=vis_corpus.${locale//./_}
    prepare_work
    run_pinned "$locale" "$DRIVERS/vis_corpus.oracle" > "$REPORTS/$tag.oracle.trace"
    run_pinned "$locale" "$DRIVERS/vis_corpus.libbsd" > "$REPORTS/$tag.libbsd.trace"

    printf '\n===== vis: in-tree (NetBSD) vs libbsd, LC_ALL=%s =====\n' "$locale"
    if cmp -s "$REPORTS/$tag.oracle.trace" "$REPORTS/$tag.libbsd.trace"; then
        printf 'IDENTICAL: the two vis implementations agree on this corpus.\n'
        continue
    fi
    status=1
    diff "$REPORTS/$tag.oracle.trace" "$REPORTS/$tag.libbsd.trace" \
        | sed 's/^</  in-tree: /; s/^>/  libbsd : /'
done

if [ "$status" -eq 0 ]; then
    printf '\nAGREE: history files written by Debian'"'"'s libedit and by this tree'"'"'s\n'
    printf 'are byte-identical for this corpus.\n'
else
    printf '\nDIFFER: the in-tree vis and libbsd'"'"'s do not agree. The port matches\n'
    printf 'the in-tree one (see the differential), so this is a statement about\n'
    printf 'existing on-disk data, not a port defect. Decide it, do not patch it.\n'
fi
exit "$status"
