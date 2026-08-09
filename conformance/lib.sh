# Shared paths and the pinned environment, sourced by every script in this
# directory. Not executable on its own.
#
# Everything the harness builds lands under $OUT, which is inside target/ and
# therefore gitignored. Nothing is written into the source tree.

set -u

# Repo root, derived from this file's own location so the scripts work from
# any cwd.
CONF_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$CONF_DIR/.." && pwd)

OUT=$ROOT/target/conformance
ORACLE_PREFIX=$OUT/oracle          # installed reference: lib/ and include/
ORACLE_BUILD=$OUT/oracle-build     # autotools VPATH build tree
DRIVERS=$OUT/drivers               # compiled differential drivers
WORK=$OUT/work                     # per-run scratch: $HOME, $TMPDIR, traces
REPORTS=$OUT/reports               # trace + diff output a human reads

# The port's artifact. `cargo build` writes it here; the harness never
# guesses at a system library.
PORT_LIB_DIR=$ROOT/target/debug
PORT_LIB=$PORT_LIB_DIR/libnshedit.so

# The SONAME `crates/nshedit-abi/build.rs` stamps onto the cdylib, which is
# therefore what every binary linked against it records as DT_NEEDED and what
# the loader then goes looking for.
PORT_SONAME=libnshedit.so.0

# Cargo writes only `libnshedit.so`, so nothing the harness links can start
# until a file named for the SONAME sits beside it. `packaging/install.sh`
# creates that link at install time; this creates it in target/ so the stages
# exercise the same two-name arrangement rather than a special case.
stage_port_soname() {
    [ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
    ln -sfn -- "$(basename -- "$PORT_LIB")" "$PORT_LIB_DIR/$PORT_SONAME"
}

# The two system libraries the ABI comparison uses, and only it. They are
# never an oracle: see plan/main.styx, the `conformance` node's log.
DEBIAN_LIBEDIT=/usr/lib/x86_64-linux-gnu/libedit.so.2
GNU_READLINE=/usr/lib/x86_64-linux-gnu/libreadline.so.8

die() { printf 'conformance: %s\n' "$*" >&2; exit 1; }
note() { printf 'conformance: %s\n' "$*" >&2; }

# The pinned environment for a driver run.
#
# libedit reads exactly four variables through its `el_getenv` hook — EDITRC,
# HOME, TERM and EDITOR (src/el.c:610,614, src/terminal.c:906, src/vi.c:1112)
# — plus LC_CTYPE through setlocale. All five are pinned here. LANG, LANGUAGE
# and the per-category LC_* are cleared so that LC_ALL is the only thing
# deciding the codeset: the C reaches it through setlocale(LC_ALL, "") and the
# port reads LC_ALL/LC_CTYPE/LANG itself (crates/nshedit/src/locale.rs), and
# they must be looking at the same answer.
#
# $1 is the locale to pin. HOME points at an empty directory so no real
# ~/.editrc is ever read.
pinned_env() {
    local locale=$1
    printf '%s\n' \
        "LC_ALL=$locale" \
        "TERM=dumb" \
        "TERMINFO=$WORK/terminfo" \
        "HOME=$WORK/home" \
        "TMPDIR=$WORK/tmp" \
        "COLUMNS=80" \
        "LINES=24" \
        "SHELL=/bin/sh" \
        "PATH=/usr/bin:/bin"
}

# Creates the controlled directories the pinned environment names, wiping any
# previous run so a stale history file cannot make a load succeed.
prepare_work() {
    rm -rf -- "$WORK"
    mkdir -p -- "$WORK/home" "$WORK/tmp" "$WORK/data" \
        "$WORK/terminfo/d" "$WORK/terminfo/x"
    # Pin the terminfo database rather than inheriting the system's. TERM is
    # `dumb`, whose entry is tiny and stable; copying it means the harness
    # reads one known file instead of whatever /usr/share/terminfo holds.
    local src
    for src in /usr/share/terminfo/d/dumb /lib/terminfo/d/dumb /etc/terminfo/d/dumb; do
        if [ -f "$src" ]; then cp -- "$src" "$WORK/terminfo/d/dumb"; break; fi
    done
    [ -f "$WORK/terminfo/d/dumb" ] || die "no terminfo entry for TERM=dumb found on this system"

    # xterm exercises parameterised strings and ncurses' termcap-visible
    # projection of `sgr0` to `me`; `dumb` deliberately contains neither.
    for src in /usr/share/terminfo/x/xterm /lib/terminfo/x/xterm /etc/terminfo/x/xterm; do
        if [ -f "$src" ]; then cp -- "$src" "$WORK/terminfo/x/xterm"; break; fi
    done
    [ -f "$WORK/terminfo/x/xterm" ] || die "no terminfo entry for TERM=xterm found on this system"
}

# Runs $2... under the pinned environment for locale $1, with the environment
# otherwise emptied. `env -i` is what makes this a pin rather than an
# overlay: an inherited EDITRC or LC_CTYPE cannot leak in.
run_pinned() {
    local locale=$1; shift
    local -a vars=()
    mapfile -t vars < <(pinned_env "$locale")
    env -i "${vars[@]}" "$@"
}
