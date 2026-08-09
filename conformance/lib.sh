# Shared paths for the Linux ABI acceptance scripts.

set -u

CONF_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd -- "$CONF_DIR/.." && pwd)

OUT=$ROOT/target/conformance
WORK=$OUT/work
REPORTS=$OUT/reports

PORT_LIB_DIR=$ROOT/target/debug
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
