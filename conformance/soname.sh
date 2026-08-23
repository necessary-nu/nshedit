#!/usr/bin/env bash
#
# Verify nshedit's ELF install layout with a freshly compiled C consumer.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

PREFIX=$OUT/prefix
BARE=$OUT/prefix-bare
BIN=$OUT/soname
REPORT=$REPORTS/soname
status=0

pass() {
    printf '  ok    %s\n' "$*"
}

fail() {
    printf '  FAIL  %s\n' "$*"
    status=1
}

soname_of() {
    readelf -d "$1" 2>/dev/null |
        sed -n 's/.*Library soname: \[\(.*\)\].*/\1/p'
}

needed_of() {
    readelf -d "$1" 2>/dev/null |
        sed -n 's/.*Shared library: \[\(.*\)\].*/\1/p'
}

[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
command -v "$CC" >/dev/null || die "C compiler '$CC' is required"
command -v readelf >/dev/null || die "readelf is required"
command -v pkg-config >/dev/null || die "pkg-config is required"

rm -rf -- "$PREFIX" "$BARE" "$BIN"
mkdir -p -- "$BIN" "$REPORT"

if [ "$(soname_of "$PORT_LIB")" = "$PORT_SONAME" ]; then
    pass "the cdylib carries SONAME $PORT_SONAME"
else
    fail "the cdylib does not carry SONAME $PORT_SONAME"
fi

if "$ROOT/packaging/install.sh" --prefix "$PREFIX" --profile debug \
        > "$REPORT/install.log" 2>&1; then
    pass "installed into $PREFIX"
else
    fail "packaging/install.sh failed; see $REPORT/install.log"
fi

object=$(readlink -f -- "$PREFIX/lib/$PORT_SONAME" 2>/dev/null)
for name in \
    "$PORT_SONAME" \
    libnshedit.so \
    libedit.so \
    libedit.so.0 \
    libedit.so.2
do
    target=$(readlink -f -- "$PREFIX/lib/$name" 2>/dev/null)
    if [ -n "$object" ] && [ "$target" = "$object" ]; then
        pass "$name resolves to the installed object"
    else
        fail "$name does not resolve to the installed object"
    fi
done

pkg_flags=$(
    PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" \
        pkg-config --cflags --libs libedit 2> "$REPORT/pkg-config.stderr"
)
pkg_status=$?
if [ "$pkg_status" -eq 0 ]; then
    read -r -a args <<< "$pkg_flags"
    if "$CC" -std=c11 -O0 -g -Wall -Wextra -Werror \
            "$CONF_DIR/fixtures/soname_consumer.c" "${args[@]}" \
            -Wl,-rpath,"$PREFIX/lib" \
            -o "$BIN/consumer" > "$REPORT/build.log" 2>&1; then
        needed=$(
            needed_of "$BIN/consumer" |
                grep -E '^(libedit|libnshedit)' || true
        )
        if [ "$needed" = "$PORT_SONAME" ]; then
            pass "pkg-config consumer records DT_NEEDED $PORT_SONAME"
        else
            fail "consumer records '${needed:-no library}', expected $PORT_SONAME"
        fi

        if output=$(LD_LIBRARY_PATH="$PREFIX/lib" "$BIN/consumer" 2>&1); then
            pass "installed headers and library run: $output"
        else
            fail "installed consumer failed: $output"
        fi
    else
        fail "consumer compilation failed; see $REPORT/build.log"
    fi
else
    fail "pkg-config cannot resolve the installed libedit.pc"
fi

if "$ROOT/packaging/install.sh" --prefix "$BARE" --profile debug --no-compat \
        > "$REPORT/install-bare.log" 2>&1; then
    # Not `find -printf`: that is a GNU extension, and the BusyBox find on a
    # musl distribution rejects it — which would leave `stray` empty and turn
    # this check into an unconditional pass.
    stray=$(cd -- "$BARE" && find . -name 'libedit*' | sed 's|^\./||' | LC_ALL=C sort)
    if [ -z "$stray" ]; then
        pass "--no-compat installs no libedit names"
    else
        fail "--no-compat still installed: $stray"
    fi
    if [ -e "$BARE/lib/$PORT_SONAME" ]; then
        pass "--no-compat still installs $PORT_SONAME"
    else
        fail "--no-compat dropped $PORT_SONAME"
    fi
else
    fail "packaging/install.sh --no-compat failed"
fi

printf '\nreport: %s\n' "$REPORT"
exit "$status"
