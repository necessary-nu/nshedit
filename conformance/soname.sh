#!/usr/bin/env bash
#
# conformance-soname — the LOADER's half of the drop-in claim.
#
#   ./conformance/soname.sh
#
# `abi-shape.sh` asks whether we export the right symbol names and
# `header-diff.sh` asks whether a consumer can be compiled against us. Neither
# asks the question that decides whether an already-installed binary starts:
# does the dynamic loader find us under the name that binary recorded?
#
# A shared object's SONAME is copied verbatim into every program that links
# it, as DT_NEEDED, and the loader then searches for a file with exactly that
# name. Three sonames are in play:
#
#   libnshedit.so.0   ours, from crates/nshedit-abi/build.rs
#   libedit.so.0      libedit's own, from configure.ac LT_VERSION 0:75:0 --
#                     a from-source build, Fedora, Arch, the BSDs
#   libedit.so.2      Debian's, from debian/patches/update-soname.diff, which
#                     changes that one line to 2:75:0 -- Debian and Ubuntu
#
# So the stage builds one consumer program three times, against three
# different libraries, and then runs all three against nothing but our own
# install. The two libedit-linked binaries are the interesting ones: they are
# what is already on a real machine, and they were built without any knowledge
# of us.
#
# This is the only stage that installs. It installs into target/ and never
# touches a system directory.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

PREFIX=$OUT/prefix
BIN=$OUT/soname
REPORT=$REPORTS/soname
status=0

section() { printf '\n--- %s ---\n' "$*"; }
pass()    { printf '  ok    %s\n' "$*"; }
fail()    { printf '  FAIL  %s\n' "$*"; status=1; }

[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
[ -f "$ORACLE_PREFIX/lib/libedit.so" ] || die "no oracle — run ./conformance/build-oracle.sh"

rm -rf -- "$PREFIX" "$BIN"
mkdir -p -- "$BIN" "$REPORT"

# The name a shared object presents itself under, or empty if it has none.
soname_of() {
    readelf -d "$1" 2>/dev/null | sed -n 's/.*Library soname: \[\(.*\)\].*/\1/p'
}

# Every DT_NEEDED of a binary, one per line.
needed_of() {
    readelf -d "$1" 2>/dev/null | sed -n 's/.*Shared library: \[\(.*\)\].*/\1/p'
}

# ---------------------------------------------------------------------------
section "what each library calls itself"
# ---------------------------------------------------------------------------

printf '  %-28s %s\n' \
    "port"   "$(soname_of "$PORT_LIB")" \
    "oracle" "$(soname_of "$(readlink -f "$ORACLE_PREFIX/lib/libedit.so")")" \
    "debian" "$(soname_of "$DEBIAN_LIBEDIT")"

if [ "$(soname_of "$PORT_LIB")" = "$PORT_SONAME" ]; then
    pass "the cdylib carries SONAME $PORT_SONAME"
else
    fail "the cdylib carries SONAME '$(soname_of "$PORT_LIB")', not $PORT_SONAME
        crates/nshedit-abi/build.rs is what stamps it. Without one, a program
        linked against us records the path it was handed, so moving the file
        breaks the binary and no compat symlink can help."
fi

# The two names the compat symlinks exist to serve, read off the libraries
# themselves rather than hardcoded, so the set cannot silently go stale.
oracle_soname=$(soname_of "$(readlink -f "$ORACLE_PREFIX/lib/libedit.so")")
debian_soname=$(soname_of "$DEBIAN_LIBEDIT")

# ---------------------------------------------------------------------------
section "packaging/install.sh lays down the chain"
# ---------------------------------------------------------------------------

if "$ROOT/packaging/install.sh" --prefix "$PREFIX" --profile debug \
        > "$REPORT/install.log" 2>&1; then
    pass "installed into $PREFIX"
else
    fail "packaging/install.sh failed:"
    sed 's/^/        /' "$REPORT/install.log"
    printf '\nreport: %s\n' "$REPORT"
    exit 1
fi

object=$(readlink -f -- "$PREFIX/lib/$PORT_SONAME")
for name in "$PORT_SONAME" libnshedit.so libedit.so "$oracle_soname" "$debian_soname"; do
    target=$(readlink -f -- "$PREFIX/lib/$name" 2>/dev/null)
    if [ -z "$target" ] || [ ! -f "$target" ]; then
        fail "$name resolves to nothing"
    elif [ "$target" != "$object" ]; then
        fail "$name resolves to $target, not the one installed object"
    else
        pass "$name -> $(basename -- "$object")"
    fi
done

# ---------------------------------------------------------------------------
section "a newly linked consumer records what it actually loaded"
# ---------------------------------------------------------------------------

# Built against our install, through the compat `libedit.so` and `-ledit` --
# which is what a build system that has always said `-ledit` will do. The
# linker reads the SONAME off the object the symlink resolves to, so the
# binary should record OUR name, not libedit's. That is the honesty half of
# the decision on `abi-soname`: ldd names us.
if gcc -std=c11 -O0 -g -I"$PREFIX/include" \
        "$CONF_DIR/aux/soname_consumer.c" -o "$BIN/consumer.fresh" \
        -L"$PREFIX/lib" -ledit \
        > "$REPORT/build.fresh" 2>&1; then
    needed=$(needed_of "$BIN/consumer.fresh" | grep -E 'libedit|libnshedit')
    if [ "$needed" = "$PORT_SONAME" ]; then
        pass "-ledit against our install records DT_NEEDED $PORT_SONAME"
    else
        fail "-ledit recorded DT_NEEDED '$needed', expected $PORT_SONAME"
    fi
else
    fail "cannot build against the install with -ledit:"
    sed 's/^/        /' "$REPORT/build.fresh"
fi

# pkg-config is how most build systems actually find libedit. We install a
# libedit.pc, so it has to answer.
if flags=$(PKG_CONFIG_PATH=$PREFIX/lib/pkgconfig pkg-config --libs libedit 2>&1); then
    pass "pkg-config --libs libedit -> $flags"
else
    fail "pkg-config cannot see the installed libedit.pc: $flags"
fi

# ---------------------------------------------------------------------------
section "a consumer linked before we existed still starts"
# ---------------------------------------------------------------------------

# This is the claim the compat symlinks are for. Each of these is built
# against a real libedit -- the oracle's, and Debian's -- so DT_NEEDED holds
# libedit's name and nothing about us. Then each is run with ONLY our install
# on the library path.
build_against() {
    local label=$1 out=$2; shift 2
    if gcc -std=c11 -O0 -g -I"$ORACLE_PREFIX/include" \
            "$CONF_DIR/aux/soname_consumer.c" -o "$out" "$@" \
            > "$REPORT/build.$label" 2>&1; then
        return 0
    fi
    fail "cannot build the consumer against $label:"
    sed 's/^/        /' "$REPORT/build.$label"
    return 1
}

# Runs $2 with $PREFIX/lib as the only library path, and checks that it both
# resolved libedit inside our prefix and then did its work.
run_against_us() {
    local label=$1 bin=$2 want=$3
    local line resolved out rc

    line=$(LD_LIBRARY_PATH=$PREFIX/lib ldd "$bin" | grep -E "$want" | head -1)
    case "$line" in
        "") fail "$label: the binary does not record $want at all"; return ;;
        *"not found"*)
            fail "$label: $want is not found under our install -- the compat
        symlink for it is missing, and this binary would not start"
            return ;;
    esac
    resolved=$(printf '%s\n' "$line" | awk '{ print $3 }')
    case "$resolved" in
        "$PREFIX"/*) ;;
        *) fail "$label: $want resolved to $resolved, outside our install --
        refusing to call that a pass, it is measuring some other library"
           return ;;
    esac

    out=$(LD_LIBRARY_PATH=$PREFIX/lib "$bin" 2>&1); rc=$?
    if [ "$rc" -eq 0 ]; then
        pass "$label: DT_NEEDED $want -> our install, and it ran: $out"
    else
        fail "$label: loaded from our install but failed (exit $rc): $out"
    fi
}

if build_against "the oracle" "$BIN/consumer.oracle" \
        -L"$ORACLE_PREFIX/lib" -ledit; then
    needed=$(needed_of "$BIN/consumer.oracle" | grep -E 'libedit')
    printf '  %s\n' "built against the in-tree C, DT_NEEDED $needed"
    run_against_us "from-source libedit" "$BIN/consumer.oracle" "$oracle_soname"
fi

if build_against "Debian's libedit" "$BIN/consumer.debian" "$DEBIAN_LIBEDIT"; then
    needed=$(needed_of "$BIN/consumer.debian" | grep -E 'libedit')
    printf '  %s\n' "built against $DEBIAN_LIBEDIT, DT_NEEDED $needed"
    run_against_us "Debian libedit" "$BIN/consumer.debian" "$debian_soname"
fi

# ---------------------------------------------------------------------------
section "--no-compat means no compat"
# ---------------------------------------------------------------------------

# The flag exists because the compat names claim filenames a distribution's
# package manager also claims, so it has to actually work. A --no-compat
# install that still dropped libedit.so.2 would be worse than not offering
# the flag.
BARE=$OUT/prefix-bare
rm -rf -- "$BARE"
if "$ROOT/packaging/install.sh" --prefix "$BARE" --profile debug --no-compat \
        > "$REPORT/install.bare.log" 2>&1; then
    # The pkgconfig file claims libedit's identity as much as the symlinks do,
    # and it lives one directory down, so look for it by name rather than
    # listing lib/ and calling that the whole answer.
    stray=$(find "$BARE" -name 'libedit*' -printf '%P\n' | sort || true)
    if [ -z "$stray" ]; then
        pass "--no-compat installed nothing named libedit"
    else
        fail "--no-compat still installed: $(printf '%s ' $stray)"
    fi
    if [ -f "$BARE/lib/$PORT_SONAME" ] || [ -L "$BARE/lib/$PORT_SONAME" ]; then
        pass "--no-compat still installs $PORT_SONAME"
    else
        fail "--no-compat dropped $PORT_SONAME, which is not a compat name"
    fi
else
    fail "packaging/install.sh --no-compat failed:"
    sed 's/^/        /' "$REPORT/install.bare.log"
fi

printf '\nreport: %s\n' "$REPORT"
if [ "$status" -ne 0 ]; then
    printf 'A binary that was linked against libedit before we existed may not\n'
    printf 'start against this install. Read the failures above; a missing compat\n'
    printf 'symlink is a packaging bug, not a port bug.\n'
fi
exit "$status"
