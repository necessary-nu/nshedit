#!/usr/bin/env bash
#
# conformance-header-diff — the COMPILE-time half of the drop-in claim.
#
# A drop-in replacement is two things. `abi-shape.sh` tests the first: the
# symbols a consumer links. This tests the second: the header a consumer
# compiles against — and with it, the struct layouts nothing else can see.
#
# THE GAP THIS CLOSES. `abi-shape.sh` compares exported symbol NAMES. Nothing
# else in the harness checks a signature and nothing checks a LAYOUT. A field
# in the wrong order inside `LineInfo`, `LineInfoW`, `HistEvent` or
# `HistEventW` breaks every C consumer that reads it, leaves the symbol table
# byte-identical, and passes all five other stages. That is the worst shape a
# defect can have here, and it is not theoretical: this stage found
# `KeymapEntry` missing its `#[repr(C)]`, which had Rust ordering the exported
# `emacs_standard_keymap` entries as it pleased.
#
# THE DIRECTION, which is the opposite of the obvious one. The GENERATED
# header is the SHIPPED header — `crates/nshedit-abi/include/`, produced by
# cbindgen from our Rust and committed. libedit's own `src/histedit.h` and
# `src/editline/readline.h` are what we diff AGAINST, and a difference means
# OUR RUST IS WRONG, or it is a divergence someone decided. It never means the
# generated header wants editing, because nothing edits it. A hand-maintained
# header would be a second artifact obliged to agree with the implementation
# and maintained apart from it, which is the failure mode this project keeps
# finding. See the `abi-headers` and `conformance-header-diff` plan nodes.
#
# One consequence follows and is worth stating: a RENDERING difference is a
# bug in `crates/nshedit-abi/cbindgen/*.toml`, not an entry on the DECIDED
# list below. The list holds only differences that no configuration can
# remove, each with its argument, printed in full at every run. A long list
# would mean the config is wrong.
#
#   ./conformance/header-diff.sh          # report, exit 1 on an unexplained gap
#   ./conformance/header-diff.sh --report # print the full comparison only
#
# Four questions are asked of each header, and only a C compiler can answer
# three of them (see conformance/header-abi.py):
#
#   1. Is the committed header what the generator produces?  (no drift)
#   2. Is every declaration there, and no extra ones?        (inventory)
#   3. Is every type the same type?                          (compat)
#   4. Are the bytes in the same places?                     (layout)
#
# and then the claim itself: a C program that includes ours and links
# libnshedit.so compiles, with -Werror, and runs.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

HDR=$REPORTS/headers          # inventories, layouts and diffs a human reads
GEN=$OUT/headers-generated    # a fresh generation, compared with the committed one
SHIPPED=$ROOT/crates/nshedit-abi/include
ABI=$CONF_DIR/header-abi.py

mkdir -p -- "$HDR" "$GEN"

command -v clang >/dev/null || die "no clang: this stage compares types by asking a C front end"
command -v python3 >/dev/null || die "no python3: header-abi.py drives clang and cpp"
[ -f "$ORACLE_PREFIX/include/histedit.h" ] || die "no oracle headers — run ./conformance/build-oracle.sh first"
[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build' first"

# The two headers libedit installs: `src/Makefile.am` line 55 is
# `nobase_include_HEADERS = histedit.h editline/readline.h` and nothing else.
# Debian additionally ships `editline/history.h`, which is a byte-for-byte
# copy of `readline.h` under a second name and is a packaging convenience of
# Debian's, not part of the upstream contract.
HEADERS=(histedit.h editline/readline.h)

# ---------------------------------------------------------------------------
# The DECIDED list: differences that are neither a bug in our Rust nor a bug
# in the cbindgen config, because no configuration can remove them.
#
# Keyed by the exact `header :: inventory line` this script prints, so a name
# cannot join by drifting into the neighbourhood of one that is already here.
# `why_decided` below carries the argument for each, in full, and this script
# FAILS if an entry ever reaches the list without one — the same discipline as
# `abi-shape.sh`'s not-exported list, and for the same reason: a list that can
# absorb a new divergence silently is a rubber stamp.
# ---------------------------------------------------------------------------
cat > "$HDR/decided" <<'EOF'
histedit.h :: MISSING :: func wcsdup
editline/readline.h :: EXTRA :: struct _history_state fields: length
EOF
sort -o "$HDR/decided" "$HDR/decided"

why_decided() {
    case $1 in
    "histedit.h :: MISSING :: func wcsdup")
        cat <<'EOF'
  wcsdup — declared by the original, not declared and not exported by us.

    `histedit.h` guards it with `#ifndef HAVE_WCSDUP`, and a consumer never
    defines HAVE_WCSDUP, so the installed header always declares
    `wchar_t *wcsdup(const wchar_t *)`.

    We do not export the symbol, and must not. `dec:libedit:posix-only-scope`
    puts it out of scope by name; `sem:histedit.wcsdup-fn` spells the case
    out; and `conformance/abi-shape.sh` carries the full argument on its own
    not-exported list — glibc supplies `wcsdup@GLIBC_2.2.5`, Debian's
    `libedit.so.2` IMPORTS it rather than exporting one, and exporting ours
    under an interposing `libedit.so.0` would resolve every caller's wcsdup,
    libc's own included, to ours.

    Declaring in a header what the library does not export would be a link
    error waiting for the first consumer to use it. And nothing is lost:
    `<wchar.h>`, which this header includes, declares the identical function.
    The declaration goes with the symbol because it is the same decision, not
    a second one.
EOF
        ;;
    "editline/readline.h :: EXTRA :: struct _history_state fields: length")
        cat <<'EOF'
  struct _history_state — a tag on a record the original leaves anonymous.

    The C writes `typedef struct { int length; } HISTORY_STATE;`. cbindgen
    prints a Rust type's name, and every Rust type has one, so ours is
    `typedef struct _history_state { int length; } HISTORY_STATE;` — the
    `_history_state` spelling chosen to match `_hist_entry` and
    `_keymap_entry`, which is how the same header tags its other two records.

    Purely additive, and checked rather than asserted: `layout` reports the
    same size, the same alignment and the same offset; `inventory` reports the
    same field under the same typedef; and every signature that mentions the
    type is spelled `HISTORY_STATE *` in both headers, so the 172 type
    assertions pass. The only thing a consumer gains is the ability to write
    `struct _history_state`, which the original does not offer and which no
    consumer can already be using.

    Not fixable by configuration: C has no way to declare a record with no
    tag from a generator that names its types. That is why this is one entry
    rather than a class of them.
EOF
        ;;
    *)
        printf '  %s\n' "$1"
        printf '    On the DECIDED list with no reason recorded. That is a bug in\n'
        printf '    this script: the list is a decision and every entry has to\n'
        printf '    carry its argument.\n'
        return 1
        ;;
    esac
}

section() { printf '\n=== %s ===\n' "$*"; }

# ---------------------------------------------------------------------------
# 1. Drift. The committed header must be what the generator produces.
# ---------------------------------------------------------------------------
printf 'Generating headers from crates/nshedit-abi\n'
if ! (cd -- "$ROOT" && cargo run -q -p nshedit-abi --example gen-headers -- "$GEN" 2>&1); then
    die "the header generator failed; nothing to compare"
fi

status=0
section "committed vs freshly generated"
drift=0
for h in "${HEADERS[@]}"; do
    if diff -u "$SHIPPED/$h" "$GEN/$h" > "$HDR/drift.$(basename "$h").diff"; then
        printf '  same  %s\n' "$h"
    else
        printf '  DRIFT %s\n' "$h"
        sed 's/^/    /' "$HDR/drift.$(basename "$h").diff"
        drift=1
    fi
done
if [ "$drift" -ne 0 ]; then
    printf '\n  crates/nshedit-abi/include/ is not what the generator produces.\n'
    printf '  Regenerate it — never edit it:\n'
    printf '      cargo run -p nshedit-abi --example gen-headers\n'
    status=1
fi

# ---------------------------------------------------------------------------
# 2..4. Against the original, one header at a time.
# ---------------------------------------------------------------------------
undecided=0
decided_seen=$HDR/decided-seen
: > "$decided_seen"

for h in "${HEADERS[@]}"; do
    tag=$(basename "$h" .h)
    ours=$SHIPPED/$h
    theirs=$ORACLE_PREFIX/include/$h

    section "$h — what is declared"
    # LC_ALL=C so `comm` and python agree on order; a locale collation
    # difference here would silently report every line as both missing and
    # extra.
    LC_ALL=C python3 "$ABI" inventory "$theirs" -I"$ORACLE_PREFIX/include" \
        | LC_ALL=C sort > "$HDR/$tag.original" || die "could not read $theirs"
    LC_ALL=C python3 "$ABI" inventory "$ours" -I"$SHIPPED" \
        | LC_ALL=C sort > "$HDR/$tag.ours" || die "could not read $ours"

    printf '  original %4s declarations   %s\n' "$(wc -l < "$HDR/$tag.original")" "$theirs"
    printf '  ours     %4s declarations   %s\n' "$(wc -l < "$HDR/$tag.ours")" "$ours"

    LC_ALL=C comm -23 "$HDR/$tag.original" "$HDR/$tag.ours" | sed "s|^|$h :: MISSING :: |" > "$HDR/$tag.missing"
    LC_ALL=C comm -13 "$HDR/$tag.original" "$HDR/$tag.ours" | sed "s|^|$h :: EXTRA :: |" > "$HDR/$tag.extra"
    cat "$HDR/$tag.missing" "$HDR/$tag.extra" | LC_ALL=C sort > "$HDR/$tag.diff"

    LC_ALL=C comm -12 "$HDR/$tag.diff" "$HDR/decided" > "$HDR/$tag.diff-decided"
    LC_ALL=C comm -23 "$HDR/$tag.diff" "$HDR/decided" > "$HDR/$tag.diff-real"
    cat "$HDR/$tag.diff-decided" >> "$decided_seen"

    printf -- '-- decided (expected — reasons at the end):\n'
    sed 's/^/     /' "$HDR/$tag.diff-decided"
    printf -- '-- everything else:\n'
    sed 's/^/     /' "$HDR/$tag.diff-real"
    [ -s "$HDR/$tag.diff-real" ] && undecided=1

    section "$h — are the types the same type"
    printf '  Each assertion is written with the type as the ORIGINAL spells it and\n'
    printf '  compiled against OURS, so the question asked is the consumer'"'"'s: does a\n'
    printf '  program that learned the API from libedit'"'"'s header type-check against\n'
    printf '  this one?\n\n'
    if LC_ALL=C python3 "$ABI" compat "$ours" "$theirs" \
            -I"$SHIPPED" -I"$ORACLE_PREFIX/include" > "$HDR/$tag.compat" 2>&1; then
        sed 's/^/  /' "$HDR/$tag.compat"
    else
        sed 's/^/  /' "$HDR/$tag.compat"
        status=1
    fi

    section "$h — are the bytes in the same places"
    LC_ALL=C python3 "$ABI" layout "$theirs" -I"$ORACLE_PREFIX/include" > "$HDR/$tag.layout.original"
    LC_ALL=C python3 "$ABI" layout "$ours" -I"$SHIPPED" > "$HDR/$tag.layout.ours"
    if diff -u "$HDR/$tag.layout.original" "$HDR/$tag.layout.ours" > "$HDR/$tag.layout.diff"; then
        printf '  %s record measurement(s), all identical\n' \
            "$(wc -l < "$HDR/$tag.layout.original")"
    else
        printf '  LAYOUT DIFFERS — a consumer reading these records gets garbage:\n'
        sed 's/^/    /' "$HDR/$tag.layout.diff"
        status=1
    fi
done

# ---------------------------------------------------------------------------
# The reasons, printed at every run rather than filed somewhere.
# ---------------------------------------------------------------------------
sort -u -o "$decided_seen" "$decided_seen"
decided_ok=0
if [ -s "$decided_seen" ]; then
    section "why those differences are decided"
    while read -r entry; do
        why_decided "$entry" || decided_ok=1
        printf '\n'
    done < "$decided_seen"
fi

# An entry that stops being a difference is as much a signal as a new one: it
# means the argument for it is stale and nobody noticed.
if unused=$(LC_ALL=C comm -23 "$HDR/decided" "$decided_seen") && [ -n "$unused" ]; then
    section "DECIDED entries that are no longer differences"
    printf '%s\n' "$unused" | sed 's/^/  /'
    printf '\n  These describe a divergence that is not there any more. Delete them,\n'
    printf '  so the list keeps meaning what it says.\n'
    status=1
fi

# ---------------------------------------------------------------------------
# The claim itself: a consumer compiles and runs against OUR header.
# ---------------------------------------------------------------------------
section "a C consumer, compiled against our headers and linked to our library"
consumer=$OUT/drivers/header_consumer
mkdir -p -- "$(dirname -- "$consumer")"
if gcc -std=c11 -O0 -g -Wall -Wextra -Werror \
        -I"$SHIPPED" \
        "$CONF_DIR/aux/header_consumer.c" -o "$consumer" \
        -L"$PORT_LIB_DIR" -lnshedit -Wl,-rpath,"$PORT_LIB_DIR" \
        > "$HDR/consumer.build" 2>&1; then
    printf '  built with -Wall -Wextra -Werror\n'
    resolved=$(ldd "$consumer" | awk '/libnshedit/ { print $3 }' | head -1)
    if [ "$resolved" != "$PORT_LIB" ]; then
        printf '  FAIL: resolves to %s, not %s — refusing to trust the run\n' \
            "${resolved:-<nothing>}" "$PORT_LIB"
        status=1
    elif "$consumer" > "$HDR/consumer.out" 2>&1; then
        sed 's/^/  /' "$HDR/consumer.out"
    else
        printf '  FAIL: the consumer ran and reported failures:\n'
        sed 's/^/    /' "$HDR/consumer.out"
        status=1
    fi
else
    printf '  FAIL: a C program cannot be built against the generated headers:\n'
    sed 's/^/    /' "$HDR/consumer.build"
    status=1
fi

[ "${1:-}" = "--report" ] && exit 0

# ---------------------------------------------------------------------------
# Verdict.
# ---------------------------------------------------------------------------
if [ "$undecided" -ne 0 ]; then
    printf '\nFAIL: declaration(s) present in one header and not the other, outside\n'
    printf '      the DECIDED list:\n'
    cat "$HDR"/*.diff-real | sed 's/^/        /'
    printf '\n      Read this as a bug in the Rust or in\n'
    printf '      crates/nshedit-abi/cbindgen/*.toml before reading it as a\n'
    printf '      divergence to decide. The generated header is the shipped one.\n'
    status=1
fi
if [ "$decided_ok" -ne 0 ]; then
    printf '\nFAIL: an entry on the DECIDED list carries no recorded reason.\n'
    status=1
fi
if [ "$status" -eq 0 ]; then
    printf '\nPASS: the generated headers declare what libedit'"'"'s declare, with the\n'
    printf '      same types and the same layouts, less the %s decided above; and a\n' \
        "$(wc -l < "$decided_seen")"
    printf '      C consumer builds and runs against them.\n'
fi
exit "$status"
