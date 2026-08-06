#!/usr/bin/env bash
#
# conformance-driver-ub — the calls the C has no defined answer for.
#
#   ./conformance/ub.sh
#
# The other three stages are differentials: the port and the oracle must
# agree. This one is not, and could not be. Every case here comes from an
# entry in `docs/errata.md` whose disposition is `define`, which says in so
# many words that the C is undefined and the port is not — so agreement is
# the wrong test, and a diff would report every success as a failure.
#
# What is asserted:
#
#   the PORT must survive every case.
#
# What is reported but not asserted:
#
#   whatever the ORACLE does.
#
# Running the oracle matters even though it cannot fail this stage. A case
# the C also survives proves nothing about the port — it may be a hazard
# somebody imagined rather than one that exists — so the report says which
# cases actually kill the C, and the corpus can be judged on that rather than
# taken on trust.
#
# Both sides are built from the same source, and each case runs in a forked
# child with an alarm, so a crash or a hang costs one case and not the run.

set -uo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

SRC=$CONF_DIR/aux/ub_corpus.c
OUT_DIR=$OUT/ub
REPORT=$REPORTS/ub

[ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
[ -f "$ORACLE_PREFIX/lib/libedit.so" ] || die "no oracle — run ./conformance/build-oracle.sh"
stage_port_soname

rm -rf -- "$OUT_DIR"
mkdir -p -- "$OUT_DIR" "$REPORT"

build() {
    local which=$1 libdir=$2 libname=$3
    gcc -std=c11 -O0 -g -Wall -Wextra -Wno-unused-parameter \
        -I"$ORACLE_PREFIX/include" \
        "$SRC" -o "$OUT_DIR/ub.$which" \
        -L"$libdir" -l"$libname" -Wl,-rpath,"$libdir" \
        || die "failed to compile ub_corpus.c against the $which"
}

build oracle "$ORACLE_PREFIX/lib" edit
build port   "$PORT_LIB_DIR"       nshedit

prepare_work
mkdir -p -- "$WORK/data"

# stdin is /dev/null, not the harness's. `el_gets` blocks until it has a line,
# so an inherited terminal or pipe makes it wait forever — both sides hung on
# ERR-core-api-11 until this was added, which was the harness's fault and not
# a finding. At EOF it returns immediately, which is the state that erratum is
# actually about.
#
# The whole run is bounded as well as each case: a driver that wedges before
# reaching its own alarms would otherwise hang the harness.
for which in oracle port; do
    timeout 300 env -i \
        LC_ALL=C TERM=dumb "TERMINFO=$WORK/terminfo" "HOME=$WORK/home" \
        "TMPDIR=$WORK/tmp" COLUMNS=80 LINES=24 PATH=/usr/bin:/bin \
        "$OUT_DIR/ub.$which" "$WORK/data" \
        < /dev/null > "$REPORT/$which.trace" 2>&1
    printf 'conformance: %s driver exited %d\n' "$which" "$?" >&2
done

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------

printf '\n%-18s %-34s %-22s %s\n' "ERRATUM" "CASE" "ORACLE (the C)" "PORT"
printf '%.0s-' {1..96}; printf '\n'

status=0
survived_both=0
killed_oracle=0
killed_port=0

# Join the two traces by sequence number. Both drivers run the same cases in
# the same order, so line N of one is line N of the other; the case label is
# carried from the port's line and the two verdicts sit side by side.
while IFS= read -r pline && IFS= read -r oline <&3; do
    case "$pline" in *" done "*) continue ;; esac

    err=$(printf '%s' "$pline" | awk '{print $2}')
    # The case label is everything between the erratum id and the verdict.
    label=$(printf '%s' "$pline" | sed -E 's/^[0-9]+ +[^ ]+ +(.*[^ ]) +(survived|KILLED|unknown).*/\1/')
    pv=$(printf '%s' "$pline" | grep -oE '(survived exit=[0-9-]+|KILLED signal=[0-9]+|unknown.*)')
    ov=$(printf '%s' "$oline" | grep -oE '(survived exit=[0-9-]+|KILLED signal=[0-9]+|unknown.*)')

    printf '%-18s %-34s %-22s %s\n' "$err" "$label" "${ov:-<missing>}" "${pv:-<missing>}"

    case "$pv" in
        survived*) ;;
        *) status=1; killed_port=$((killed_port + 1)) ;;
    esac
    case "$ov" in
        KILLED*) killed_oracle=$((killed_oracle + 1)) ;;
        survived*) case "$pv" in survived*) survived_both=$((survived_both + 1)) ;; esac ;;
    esac
done < "$REPORT/port.trace" 3< "$REPORT/oracle.trace"

total=$(grep -c '^[0-9]' "$REPORT/port.trace")
printf '\n'
if [ "$killed_port" -eq 0 ]; then
    printf '%d cases. The C dies on %d of them; the port dies on none.\n' \
        "$((total - 1))" "$killed_oracle"
else
    printf '%d cases. The C dies on %d of them, the port on %d.\n' \
        "$((total - 1))" "$killed_oracle" "$killed_port"
fi
if [ "$survived_both" -gt 0 ]; then
    printf '\n%d case(s) the C also survives. Those prove nothing on their own —\n' "$survived_both"
    printf 'the erratum they cite may describe a hazard that needs a different\n'
    printf 'input to reach, or the disposition may already be met by accident.\n'
    printf 'Read them against docs/errata.md rather than counting them as wins.\n'
fi
if [ "$status" -ne 0 ]; then
    printf '\nFAIL: the port died on a case whose erratum says the disposition is\n'
    printf '"define". Either the disposition was not carried out, or the entry is\n'
    printf 'wrong about what the call does. Traces: %s\n' "$REPORT"
fi
printf '\nreports: %s\n' "$REPORT"
exit "$status"
