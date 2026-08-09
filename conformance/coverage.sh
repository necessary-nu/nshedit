#!/usr/bin/env bash
#
# Which functions do the conformance drivers actually execute?
#
#   ./conformance/coverage.sh            # measure, and rewrite the claim file
#   ./conformance/coverage.sh --check    # measure, and fail if it is stale
#
# `nplan port status` reports how many of the port's 572 functions are
# verified. The plan's `conformance` node is explicit about how that question
# may be answered:
#
#     the drivers may claim only what they provably drive — hist_tok.c
#     reaches history and tokenizer, not chared. Annotating a function no
#     driver executes would make the gate report coverage that does not
#     exist, which is worse than a low number.
#
# "Provably" is the operative word, so this measures rather than judges. The
# cdylib is rebuilt with `-C instrument-coverage`, each driver is run against
# it separately, and `llvm-cov` says which functions ran. A `sem` annotation
# is claimed only when the function it labels is inside a region that
# executed — and the claim records WHICH driver executed it, so a driver that
# is later narrowed takes its claims with it.
#
# The output is `crates/nshedit-abi/tests/driven.rs`, which is where the
# annotations have to live: the port gate reads the `include` globs from
# `.config/nspec/config.styx`, so a `/test` facet only counts inside
# `crates/**/*.rs`. Measured, not assumed — a `/test` annotation in a test
# file moves the count and a bare `sem` annotation does not.
#
# This is NOT part of `run.sh`. It rebuilds the whole workspace under
# instrumentation, which costs about half a minute and a second target
# directory, and its answer changes only when a driver changes. Run it then,
# and `--check` in the same breath as reviewing a driver.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

MODE=${1:-write}
COV=$OUT/coverage
COV_TARGET=$ROOT/target/cov-build
CLAIMS=$ROOT/crates/nshedit-abi/tests/driven.rs

DRIVERS=(hist_tok el_api readline_api pty_edit binding_dispatch abi_gaps ub_corpus)
src_of() {
    case $1 in
        ub_corpus) printf '%s\n' "$CONF_DIR/aux/ub_corpus.c" ;;
        *) printf '%s\n' "$CONF_DIR/driver/$1.c" ;;
    esac
}

# The llvm tools must be the ones this rustc emits for; Debian's are several
# major versions behind and cannot read the profraw at all.
LLVM=$(rustc --print target-libdir 2>/dev/null)/../bin
[ -x "$LLVM/llvm-profdata" ] || die "no llvm-profdata beside rustc — 'rustup component add llvm-tools'"

[ -f "$ORACLE_PREFIX/include/histedit.h" ] || die "no oracle — run ./conformance/build-oracle.sh"

rm -rf -- "$COV"
mkdir -p -- "$COV/bin"

note "building the cdylib under -C instrument-coverage"
( cd -- "$ROOT" && RUSTFLAGS="-C instrument-coverage" CARGO_TARGET_DIR="$COV_TARGET" \
    cargo build --workspace ) > "$COV/build.log" 2>&1 \
    || { sed 's/^/  /' "$COV/build.log"; die "instrumented build failed"; }

# The SONAME the cdylib carries has to resolve, exactly as it does for the
# uninstrumented harness.
ln -sfn libnshedit.so "$COV_TARGET/debug/$PORT_SONAME"

prepare_work

for name in "${DRIVERS[@]}"; do
    gcc -std=c11 -O0 -g -I"$ORACLE_PREFIX/include" "$(src_of "$name")" -o "$COV/bin/$name" \
        -L"$COV_TARGET/debug" -lnshedit -Wl,-rpath,"$COV_TARGET/debug" \
        || die "failed to build $name against the instrumented library"

    mkdir -p -- "$COV/raw/$name"
    rm -rf -- "$WORK/data"; mkdir -p -- "$WORK/data"
    # Same pinned environment the differential uses, so a driver reaches the
    # same code here as it does there.
    env -i "LLVM_PROFILE_FILE=$COV/raw/$name/%p.profraw" \
        LC_ALL=C.UTF-8 TERM=dumb "TERMINFO=$WORK/terminfo" "HOME=$WORK/home" \
        "TMPDIR=$WORK/tmp" COLUMNS=80 LINES=24 PATH=/usr/bin:/bin \
        "$COV/bin/$name" "$WORK/data" < /dev/null > /dev/null 2>&1
    rc=$?
    note "$name exited $rc"

    "$LLVM/llvm-profdata" merge -sparse "$COV/raw/$name"/*.profraw \
        -o "$COV/$name.profdata" 2> /dev/null \
        || die "$name produced no profile — did it run?"
    "$LLVM/llvm-cov" export --instr-profile="$COV/$name.profdata" \
        "$COV_TARGET/debug/libnshedit.so" --format=text > "$COV/$name.json" 2> /dev/null \
        || die "llvm-cov export failed for $name"
done

exec python3 "$CONF_DIR/driven.py" "$MODE" "$COV" "$ROOT" "$CLAIMS" "${DRIVERS[@]}"
