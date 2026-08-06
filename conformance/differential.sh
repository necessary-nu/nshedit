#!/usr/bin/env bash
#
# conformance-differential — drive both libraries through the same C driver
# and diff the traces.
#
# One driver source, compiled twice against the same header (the oracle's
# histedit.h) and linked against two different libraries. Both binaries run
# the identical scripted sequence under an identical, pinned environment and
# emit a numbered, labelled trace. The traces are then diffed.
#
#   ./conformance/differential.sh              # every driver, every locale
#   ./conformance/differential.sh hist_tok     # one driver
#
# A failure names the operation. Each trace line is
#
#   NNNN <operation label>       <result fields>
#
# so the report below prints the first differing lines side by side with
# their labels — "H_SAVE bytes", "tok backslash in dq" — rather than a byte
# offset. Full traces are left in target/conformance/reports/ for a wider
# look, and `diff -u` on the two files is always available.
#
# A divergence is not automatically a bug. Read it against docs/errata.md:
# the register says which defects the port reproduces on purpose, and a
# divergence that matches a registered entry means the harness is working.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

# The two codesets crates/nshedit/src/locale.rs implements, which are also
# the two the C's ct_encode_string behaves differently under. The history
# file's bytes depend on this, so both are exercised.
LOCALES=("C.UTF-8" "C")

DRIVER_SRCS=()
if [ $# -gt 0 ]; then
    for name in "$@"; do DRIVER_SRCS+=("$CONF_DIR/driver/$name.c"); done
else
    for f in "$CONF_DIR"/driver/*.c; do DRIVER_SRCS+=("$f"); done
fi

[ -f "$ORACLE_PREFIX/lib/libedit.so" ] || die "no oracle — run ./conformance/build-oracle.sh"
[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
stage_port_soname

mkdir -p -- "$DRIVERS" "$REPORTS"

# Both libraries are linked by -l name with an rpath, and both carry a SONAME
# — libnshedit.so.0 for the port, libedit.so.0 for the oracle — so DT_NEEDED
# records the soname and the rpath is what finds it. Neither link line can
# reach /usr/lib: -L comes first and the rpath is absolute, and the check
# after each build confirms which library the binary actually resolved to.
compile_driver() {
    local src=$1 which=$2 out=$3 libdir=$4 libname=$5
    gcc -std=c11 -O0 -g -Wall -Wextra -Wno-unused-parameter \
        -I"$ORACLE_PREFIX/include" \
        "$src" -o "$out" \
        -L"$libdir" -l"$libname" -Wl,-rpath,"$libdir" \
        || die "failed to compile $(basename "$src") against the $which"
}

# Confirms at run time — not link time — which shared object the binary will
# use. This is the check that stops the harness quietly measuring Debian's
# libedit: if ldd resolves libedit to /usr/lib, we stop.
verify_link() {
    local bin=$1 which=$2 expect=$3
    local line resolved
    line=$(ldd "$bin" | grep -E 'libedit|libnshedit' | head -1)
    [ -n "$line" ] || die "$which driver links no libedit/libnshedit at all"
    # A DT_NEEDED naming a file the loader cannot find prints "=> not found",
    # whose third field is the word "not". Say so, rather than reporting a
    # missing library as one that resolved somewhere surprising.
    case "$line" in
        *"not found"*) die "$which driver needs ${line%% *}, which is not on the search path" ;;
    esac
    resolved=$(printf '%s\n' "$line" | awk '{ print $3 }')
    [ -n "$resolved" ] || die "$which driver: cannot read a path out of: $line"
    case "$resolved" in
        "$expect"*) : ;;
        *) die "$which driver resolves to $resolved, not $expect — refusing to run" ;;
    esac
}

overall=0

for src in "${DRIVER_SRCS[@]}"; do
    name=$(basename "$src" .c)
    note "driver: $name"

    compile_driver "$src" "oracle" "$DRIVERS/$name.oracle" "$ORACLE_PREFIX/lib" edit
    compile_driver "$src" "port"   "$DRIVERS/$name.port"   "$PORT_LIB_DIR"     nshedit
    verify_link "$DRIVERS/$name.oracle" oracle "$ORACLE_PREFIX/lib/"
    verify_link "$DRIVERS/$name.port"   port   "$PORT_LIB_DIR/"

    for locale in "${LOCALES[@]}"; do
        tag="$name.${locale//./_}"
        prepare_work
        mkdir -p -- "$WORK/data/oracle" "$WORK/data/port"

        set +e
        run_pinned "$locale" "$DRIVERS/$name.oracle" "$WORK/data/oracle" \
            > "$REPORTS/$tag.oracle.trace" 2> "$REPORTS/$tag.oracle.stderr"
        rc_oracle=$?
        run_pinned "$locale" "$DRIVERS/$name.port" "$WORK/data/port" \
            > "$REPORTS/$tag.port.trace" 2> "$REPORTS/$tag.port.stderr"
        rc_port=$?
        set -e

        printf '\n===== %s, LC_ALL=%s =====\n' "$name" "$locale"
        printf 'exit: oracle=%d port=%d   lines: oracle=%d port=%d\n' \
            "$rc_oracle" "$rc_port" \
            "$(wc -l < "$REPORTS/$tag.oracle.trace")" \
            "$(wc -l < "$REPORTS/$tag.port.trace")"

        if [ "$rc_oracle" -ne 0 ]; then
            printf 'ORACLE EXITED %d. stderr:\n' "$rc_oracle"
            sed 's/^/  | /' "$REPORTS/$tag.oracle.stderr" | head -20
            overall=1
        fi
        if [ "$rc_port" -ne 0 ]; then
            printf 'PORT EXITED %d. stderr:\n' "$rc_port"
            sed 's/^/  | /' "$REPORTS/$tag.port.stderr" | head -20
            overall=1
        fi

        if cmp -s "$REPORTS/$tag.oracle.trace" "$REPORTS/$tag.port.trace"; then
            printf 'IDENTICAL: %s operations agree.\n' \
                "$(wc -l < "$REPORTS/$tag.oracle.trace")"
            continue
        fi

        overall=1
        # The per-operation report. Field 2..n of a trace line is the
        # operation label, so a differing line can name what diverged.
        ndiff=$(diff <(cut -c1-4 "$REPORTS/$tag.oracle.trace") \
                     <(cut -c1-4 "$REPORTS/$tag.port.trace") >/dev/null 2>&1 \
                && echo aligned || echo shifted)
        printf 'DIVERGED (line numbering %s). Operations that differ:\n' "$ndiff"

        # Walk both traces in lockstep while the sequence numbers agree; that
        # covers every case where neither side skipped or added a line, which
        # is the common one. Where they stop agreeing, say so and stop —
        # everything after a desync is noise.
        awk -v ORACLE="$REPORTS/$tag.oracle.trace" -v PORT="$REPORTS/$tag.port.trace" '
        BEGIN {
            shown = 0; n = 0;
            while ((getline a < ORACLE) > 0) {
                if ((getline b < PORT) <= 0) {
                    printf("  line %d: port trace ends here; oracle continues with:\n    %s\n", n + 1, a);
                    exit;
                }
                n++;
                if (a == b) continue;
                seq_a = substr(a, 1, 4); seq_b = substr(b, 1, 4);
                if (seq_a != seq_b) {
                    printf("  line %d: traces desynchronised (oracle seq %s, port seq %s); stopping\n", n, seq_a, seq_b);
                    exit;
                }
                label = substr(a, 6, 26); sub(/ +$/, "", label);
                printf("  [%s] %s\n", seq_a, label);
                printf("      oracle: %s\n", substr(a, 33));
                printf("      port  : %s\n", substr(b, 33));
                if (++shown >= 40) { printf("  ... (40 shown; see the full traces)\n"); exit; }
            }
            if ((getline b < PORT) > 0)
                printf("  line %d: oracle trace ends here; port continues with:\n    %s\n", n + 1, b);
        }'
        printf 'full traces:\n  %s\n  %s\n' \
            "$REPORTS/$tag.oracle.trace" "$REPORTS/$tag.port.trace"
    done
done

if [ "$overall" -eq 0 ]; then
    printf '\nPASS: every driver produced identical traces under every locale.\n'
else
    printf '\nFAIL: see the per-operation report above. A divergence is not\n'
    printf 'automatically a port bug — check docs/errata.md for a registered\n'
    printf 'defect the port reproduces on purpose.\n'
fi
exit "$overall"
