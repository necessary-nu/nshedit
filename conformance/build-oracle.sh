#!/usr/bin/env bash
#
# conformance-oracle — build the in-tree C as the reference artifact.
#
# The oracle is *this tree's* src/*.c, configured and built out of tree, never
# the system libedit. Debian ships 3.1-20250104 as libedit.so.2.0.75; this
# tree is libedit-20260512-3.1 at LT_VERSION 0:78:0, about sixteen months
# newer. Diffing the port against Debian's build would blame the port for
# upstream's changes, so the reference is compiled from the exact source the
# port was translated from. The [spec:libedit:...] annotations in src/ are
# comments and change nothing.
#
#   ./conformance/build-oracle.sh          # build if stale, else no-op
#   ./conformance/build-oracle.sh --clean  # discard and rebuild from scratch
#
# Idempotent: a second run does nothing if the installed library is newer than
# every C source. Offline: configure and make are told nothing about the
# network, and no dependency is fetched. Loud on failure: if the build cannot
# be produced this exits non-zero and says why. It never falls back to the
# system library, because a harness that silently measures Debian's libedit
# instead of ours is worse than no harness at all.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

if [ "${1:-}" = "--clean" ]; then
    rm -rf -- "$ORACLE_BUILD" "$ORACLE_PREFIX"
fi

ORACLE_LIB=$ORACLE_PREFIX/lib/libedit.so
ORACLE_HDR=$ORACLE_PREFIX/include/histedit.h

# Up to date? Every .c and .h under src/, plus the two build inputs that
# decide what gets compiled in.
if [ -f "$ORACLE_LIB" ] && [ -f "$ORACLE_HDR" ]; then
    newest=$(find "$ROOT/src" -name '*.c' -o -name '*.h' -o -name 'Makefile.am' -o -name 'makelist' \
             | xargs ls -t 2>/dev/null | head -1 || true)
    if [ -n "$newest" ] && [ "$ORACLE_LIB" -nt "$newest" ] \
       && [ "$ORACLE_LIB" -nt "$ROOT/configure.ac" ]; then
        note "oracle up to date at $ORACLE_PREFIX"
        exit 0
    fi
fi

command -v gcc >/dev/null || die "no C compiler: the oracle cannot be built (install gcc)"
command -v make >/dev/null || die "no make: the oracle cannot be built"

mkdir -p -- "$ORACLE_BUILD"
cd -- "$ORACLE_BUILD"

# A VPATH build. The source tree stays clean — the six generated headers
# (vi.h, emacs.h, common.h, fcns.h, help.h, func.h) land here, not in src/.
if [ ! -f config.status ]; then
    note "configuring the oracle (out of tree, prefix $ORACLE_PREFIX)"
    "$ROOT/configure" --prefix="$ORACLE_PREFIX" --enable-shared --enable-static \
        > configure.log 2>&1 \
        || { tail -40 configure.log >&2; die "configure failed; see $ORACLE_BUILD/configure.log"; }
fi

note "building the oracle"
# src only. The doc/ subdirectory needs an nroff and builds nothing the
# harness links against.
make -C src > build.log 2>&1 \
    || { tail -40 build.log >&2; die "build failed; see $ORACLE_BUILD/build.log"; }

make -C src install > install.log 2>&1 \
    || { tail -40 install.log >&2; die "install failed; see $ORACLE_BUILD/install.log"; }

# Loud rather than silent. If any of these is missing the harness must stop,
# not proceed against whatever else happens to be on the link path.
[ -f "$ORACLE_LIB" ] || die "oracle build produced no $ORACLE_LIB"
[ -f "$ORACLE_PREFIX/lib/libedit.a" ] || die "oracle build produced no static libedit.a"
[ -f "$ORACLE_HDR" ] || die "oracle build installed no histedit.h"
[ -f "$ORACLE_PREFIX/include/editline/readline.h" ] || die "oracle build installed no readline.h"

# The artifact must be ours, not a symlink into /usr/lib. Check the soname
# carries this tree's version-info rather than Debian's.
soname=$(readelf -d "$(readlink -f "$ORACLE_LIB")" | sed -n 's/.*SONAME.*\[\(.*\)\].*/\1/p')
[ "$soname" = "libedit.so.0" ] \
    || die "oracle soname is '$soname', expected libedit.so.0 from LT_VERSION 0:78:0 — this is not our build"

note "oracle ready: $(readlink -f "$ORACLE_LIB") (soname $soname)"
