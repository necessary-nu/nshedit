#!/usr/bin/env bash
#
# conformance-abi-shape — the drop-in claim, stated as a test.
#
# A drop-in replacement is first of all a set of symbols. This compares four
# dynamic symbol tables:
#
#   port     target/debug/libnshedit.so   what we ship
#   oracle   the in-tree C, built by us   what we translated
#   debian   libedit.so.2                 what a deployed consumer links today
#   gnu      libreadline.so.8             the reference for the readline layer
#
# The oracle is the primary comparison, because it is the same source at the
# same version. The Debian and GNU columns say what is actually reachable on a
# real machine, which is a different question and sometimes a different answer.
#
#   ./conformance/abi-shape.sh          # report, exit 1 on an unexplained gap
#   ./conformance/abi-shape.sh --report # print the full three-way table only
#
# Failure looks like a named symbol under "MISSING" or "EXTRA", not a count.
#
# Two sets of symbols the oracle exports are missing on purpose, and each is
# split out by name so it can never stand in for a real gap: the vis family,
# for the build-configuration reason argued below, and the two libc
# gap-fillers under "DECIDED", whose reasons this script prints in full. A
# name may only join those sets by being decided, on the record, in a rule or
# a plan decision — never by being observed to be missing.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

mkdir -p -- "$REPORTS"
SYM=$REPORTS/symbols
mkdir -p -- "$SYM"

[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build' first"
[ -f "$ORACLE_PREFIX/lib/libedit.so" ] || die "no oracle — run ./conformance/build-oracle.sh first"

# Defined, exported, dynamic symbols. Version suffixes are stripped so
# strvis@LIBBSD_0.0 and strvis compare equal; the version is reported
# separately where it matters.
extract() {
    nm -D --defined-only --extern-only "$1" 2>/dev/null \
        | awk '$2 ~ /^[TBDRWiV]$/ { print $3 }' \
        | sed 's/@.*//' | sort -u
}

extract "$PORT_LIB"                                     > "$SYM/port"
extract "$(readlink -f "$ORACLE_PREFIX/lib/libedit.so")" > "$SYM/oracle"
extract "$DEBIAN_LIBEDIT"                               > "$SYM/debian"
extract "$GNU_READLINE"                                 > "$SYM/gnu"

# The vis family. libedit's own vis.c/unvis.c are compiled in only when
# configure finds no system vis (src/Makefile.am, if !HAVE_VIS / !HAVE_UNVIS),
# so whether these are exported is a property of the build host, not of the
# source. Listed explicitly so the report can say which of the three
# libraries has them and why that is the right answer.
cat > "$SYM/vis-family" <<'EOF'
nvis
snvis
stravis
strenvisx
strnunvis
strnunvisx
strnvis
strnvisx
strsenvisx
strsnvis
strsnvisx
strsvis
strsvisx
strunvis
strunvisx
strvis
strvisx
svis
unvis
vis
EOF
sort -u -o "$SYM/vis-family" "$SYM/vis-family"

# The libc gap-fillers the C exports and the port deliberately does not.
# `dec:libedit:posix-only-scope` names both by name — "strlcpy, strlcat,
# getline, wcsdup and reallocarr leave scope; they are libc gap-fillers with
# no observable surface" — and `why_not_exported` below carries the argument
# for each, including the part of that clause the symbol table disproves.
#
# This list is a decision, not an observation. Adding a name to it without a
# rule or a plan decision behind it turns this stage into a rubber stamp.
cat > "$SYM/not-exported" <<'EOF'
reallocarr
wcsdup
EOF
sort -u -o "$SYM/not-exported" "$SYM/not-exported"

why_not_exported() {
    case $1 in
    reallocarr)
        cat <<'EOF'
  reallocarr — NetBSD's overflow-checked array realloc.

    Out of scope by dec:libedit:posix-only-scope, which names it. glibc has no
    reallocarr, so nothing is shadowed by its absence and nothing is shadowed
    by its presence either; Debian's libedit.so.2 exports it for the same
    reason our oracle does, which is that configure found the platform lacks
    it and src/reallocarr.c was compiled in.

    It is not an interface. No installed header declares it — src/Makefile.am
    installs histedit.h and editline/readline.h and nothing else, and its
    prototype lives in src/sys.h, which is private. It has exactly one caller
    anywhere in libedit, src/wcsdup.c:38, and on a glibc host that file now
    compiles to nothing. So it is an internal helper with default visibility
    rather than a symbol a consumer was offered.

    And it cannot be honoured in Rust. reallocarr reallocates a block the
    CALLER allocated, so its size is not known here, and std::alloc::System
    cannot be given the Layout that GlobalAlloc::realloc requires; only libc's
    own realloc/free would do. That would be a fourth site on
    dec:libedit:no-c-ffi's closed enumeration, and its two-part test refuses
    it: the second half asks for a rule in the corpus that specifies the
    behaviour, and there is none — .config/nspec/config.styx excludes
    src/reallocarr.c, so the function has no def or sem rule at all. Reaching
    for System::realloc with a fabricated Layout would be exactly the "route
    to a nearby question" that decision rejects by name.

    Residual risk, stated rather than hidden: a consumer that declared
    reallocarr itself and resolved it through libedit.so.2 gets an unresolved
    symbol against us, and on glibc there is no other provider. That is the
    cost of holding both decisions, and it is the one gap in the drop-in claim
    this stage now passes with.
EOF
        ;;
    wcsdup)
        cat <<'EOF'
  wcsdup — a libc gap-filler, and on this platform not even that.

    glibc supplies wcsdup@GLIBC_2.2.5, and Debian's libedit.so.2 IMPORTS it
    from there rather than exporting one of its own. Out of scope by
    dec:libedit:posix-only-scope and by sem:histedit.wcsdup-fn, which spells
    the whole case out.

    Exporting it would be actively harmful, not merely redundant: our artifact
    is installed with libedit.so.0 symlinked onto it, ELF interposition binds
    by load order, and a process that loaded us ahead of libc would resolve
    every caller's wcsdup — libc's own included — to ours. That is the hazard
    ERR-readline-53 records for completion_matches and the abi-surface node
    records for the vis family.

    The in-tree C no longer exports it either. src/wcsdup.c used to evaluate
    `#ifndef HAVE_WCSDUP` two lines BEFORE `#include "config.h"`, so the macro
    did not exist yet, the guard was unconditionally true, and the bundled
    copy was compiled into every build. With the include moved above the
    guard, config.h's `#define HAVE_WCSDUP 1` is visible and the file compiles
    to nothing. The name is kept on this list so the decision survives a host
    where HAVE_WCSDUP is genuinely 0.
EOF
        ;;
    *)
        printf '  %s — on the not-exported list with no reason recorded.\n' "$1"
        printf '    That is a bug in this script: the list is a decision and\n'
        printf '    every entry has to carry its argument.\n'
        return 1
        ;;
    esac
}

section() { printf '\n=== %s ===\n' "$*"; }
count() { wc -l < "$1" | tr -d ' '; }

printf 'ABI shape: exported dynamic symbols\n'
printf '  port    %5s  %s\n' "$(count "$SYM/port")"   "$PORT_LIB"
printf '  oracle  %5s  %s\n' "$(count "$SYM/oracle")" "$(readlink -f "$ORACLE_PREFIX/lib/libedit.so")"
printf '  debian  %5s  %s\n' "$(count "$SYM/debian")" "$DEBIAN_LIBEDIT"
printf '  gnu     %5s  %s\n' "$(count "$SYM/gnu")"    "$GNU_READLINE"

# ---------------------------------------------------------------------------
# Primary: port vs oracle.
# ---------------------------------------------------------------------------
comm -23 "$SYM/oracle" "$SYM/port" > "$SYM/missing-vs-oracle"
comm -13 "$SYM/oracle" "$SYM/port" > "$SYM/extra-vs-oracle"

# Split the missing set three ways: the vis family is missing on purpose (see
# below), the decided list is missing on purpose (reasons printed in full),
# and whatever is left is a real gap.
comm -12 "$SYM/missing-vs-oracle" "$SYM/vis-family" > "$SYM/missing-vis"
comm -23 "$SYM/missing-vs-oracle" "$SYM/vis-family" > "$SYM/missing-other"
comm -12 "$SYM/missing-other" "$SYM/not-exported" > "$SYM/missing-decided"
comm -23 "$SYM/missing-other" "$SYM/not-exported" > "$SYM/missing-real"

section "port vs oracle: MISSING, vis family (expected — see below)"
cat "$SYM/missing-vis" || true
section "port vs oracle: MISSING, decided (expected — reasons below)"
cat "$SYM/missing-decided" || true
section "port vs oracle: MISSING, everything else"
cat "$SYM/missing-real" || true
section "port vs oracle: EXTRA (symbols the C does not export)"
cat "$SYM/extra-vs-oracle" || true

if [ -s "$SYM/missing-decided" ]; then
    section "why those are not exported"
    decided_ok=0
    while read -r sym; do
        why_not_exported "$sym" || decided_ok=1
        printf '\n'
    done < "$SYM/missing-decided"
else
    decided_ok=0
fi

# ---------------------------------------------------------------------------
# The vis question. libedit does not have one answer to it; the build host
# decides. Report which library exports the family and which imports it.
# ---------------------------------------------------------------------------
section "the vis family: exported, or imported from libbsd?"
for pair in "port:$PORT_LIB" "oracle:$(readlink -f "$ORACLE_PREFIX/lib/libedit.so")" "debian:$DEBIAN_LIBEDIT"; do
    name=${pair%%:*}; lib=${pair#*:}
    exported=$(comm -12 "$SYM/$name" "$SYM/vis-family" | wc -l | tr -d ' ')
    imported=$(nm -D --undefined-only "$lib" | sed 's/@.*//' | awk '{print $2}' | sort -u \
               | comm -12 - "$SYM/vis-family" | wc -l | tr -d ' ')
    needs_bsd=$(readelf -d "$lib" | grep -c 'NEEDED.*libbsd' || true)
    printf '  %-7s exports %2s   imports %2s   NEEDED libbsd: %s\n' \
        "$name" "$exported" "$imported" "$([ "$needs_bsd" -gt 0 ] && echo yes || echo no)"
done
printf '\n  Matching Debian means NOT exporting these: Debian builds libedit against\n'
printf '  libbsd, so its libedit.so.2 imports strvis/strunvis/vis from LIBBSD_0.0\n'
printf '  and exports none of the family. Our oracle exports all twenty only\n'
printf '  because libbsd-dev is absent on this host, so configure compiled\n'
printf '  src/vis.c and src/unvis.c in. That is a build-configuration\n'
printf '  difference in the oracle, not a defect in the port.\n'

# ---------------------------------------------------------------------------
# Three-way: what a real consumer can reach.
# ---------------------------------------------------------------------------
section "port vs debian (libedit.so.2 — what a deployed consumer links)"
printf -- '-- in debian, not in port (a consumer calling these breaks):\n'
comm -13 "$SYM/port" "$SYM/debian" || true
printf -- '-- in port, not in debian (newer than 3.1-20250104, or ours):\n'
comm -23 "$SYM/port" "$SYM/debian" || true

section "oracle vs debian (pure version skew, 0:78:0 vs 3.1-20250104)"
printf -- '-- in oracle, not in debian:\n'
comm -23 "$SYM/oracle" "$SYM/debian" | grep -vxF -f "$SYM/vis-family" || true
printf -- '   (plus the %s vis-family symbols above)\n' "$(comm -23 "$SYM/oracle" "$SYM/debian" | grep -cxF -f "$SYM/vis-family" || true)"
printf -- '-- in debian, not in oracle:\n'
comm -13 "$SYM/oracle" "$SYM/debian" || true

section "readline coverage (libreadline.so.8)"
for name in port debian oracle; do
    have=$(comm -12 "$SYM/$name" "$SYM/gnu" | wc -l | tr -d ' ')
    lack=$(comm -13 "$SYM/$name" "$SYM/gnu" | wc -l | tr -d ' ')
    printf '  %-7s shares %3s of GNU readline'"'"'s %s exported symbols; %s unimplemented\n' \
        "$name" "$have" "$(count "$SYM/gnu")" "$lack"
done
comm -13 "$SYM/port" "$SYM/debian" | grep -xF -f "$SYM/gnu" > "$SYM/readline-gap" || true
if [ -s "$SYM/readline-gap" ]; then
    printf '  readline symbols debian libedit reaches and the port does not:\n'
    sed 's/^/    /' "$SYM/readline-gap"
fi

# ---------------------------------------------------------------------------
# SONAME. A consumer's DT_NEEDED names a soname, so a drop-in has to carry
# the right one — a cdylib carries none at all.
# ---------------------------------------------------------------------------
section "SONAME (what a consumer's DT_NEEDED has to match)"
for pair in "port:$PORT_LIB" "oracle:$(readlink -f "$ORACLE_PREFIX/lib/libedit.so")" "debian:$DEBIAN_LIBEDIT" "gnu:$GNU_READLINE"; do
    name=${pair%%:*}; lib=${pair#*:}
    so=$(readelf -d "$lib" | sed -n 's/.*SONAME.*\[\(.*\)\].*/\1/p')
    printf '  %-7s %s\n' "$name" "${so:-<none>}"
done

[ "${1:-}" = "--report" ] && exit 0

# ---------------------------------------------------------------------------
# Verdict.
# ---------------------------------------------------------------------------
status=0
if [ -s "$SYM/missing-real" ]; then
    printf '\nFAIL: %s symbol(s) the oracle exports and the port does not, outside the\n' \
        "$(count "$SYM/missing-real")"
    printf '      vis family and the decided list:\n'
    sed 's/^/        /' "$SYM/missing-real"
    status=1
fi
if [ "$decided_ok" -ne 0 ]; then
    printf '\nFAIL: a name on the not-exported list carries no recorded reason.\n'
    status=1
fi
if [ -s "$SYM/extra-vs-oracle" ]; then
    printf '\nFAIL: %s symbol(s) the port exports and the oracle does not.\n' \
        "$(count "$SYM/extra-vs-oracle")"
    status=1
fi
if [ "$status" -eq 0 ]; then
    printf '\nPASS: the port exports exactly the oracle'"'"'s symbols, less the vis family\n'
    printf '      and the %s decided above.\n' "$(count "$SYM/missing-decided")"
fi
exit "$status"
