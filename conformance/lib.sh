# Shared paths for the Linux ABI acceptance scripts.

set -u

CONF_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$CONF_DIR/.." && pwd)

# Which build these stages inspect. Empty means the host build in
# target/debug, which is where `cargo build` writes; NSHEDIT_TARGET names a
# triple when the artifacts came from `cargo build --target`, and every stage
# — packaging/install.sh included, which reads the same variable — then
# inspects that directory instead. It selects a directory; it is not a claim
# that this host can run what it finds there.
NSHEDIT_TARGET=${NSHEDIT_TARGET:-}
export NSHEDIT_TARGET

OUT=$ROOT/target/conformance${NSHEDIT_TARGET:+-$NSHEDIT_TARGET}
WORK=$OUT/work
REPORTS=$OUT/reports

PORT_LIB_DIR=$ROOT/target${NSHEDIT_TARGET:+/$NSHEDIT_TARGET}/debug
PORT_LIB=$PORT_LIB_DIR/libnshedit.so
PORT_SONAME=libnshedit.so.0
CC=${CC:-cc}

die() {
    printf 'conformance: %s\n' "$*" >&2
    exit 1
}

note() {
    printf 'conformance: %s\n' "$*" >&2
}

# Cargo writes the link name. The installer writes the SONAME link; acceptance
# stages use the same two-name arrangement in target/debug.
stage_port_soname() {
    [ -f "$PORT_LIB" ] || die "no $PORT_LIB — run 'cargo build'"
    ln -sfn -- "$(basename -- "$PORT_LIB")" "$PORT_LIB_DIR/$PORT_SONAME"
}
