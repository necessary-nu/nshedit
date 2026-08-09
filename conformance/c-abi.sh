#!/usr/bin/env bash
#
# Compile and run a real C consumer against the committed generated headers.

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/lib.sh"

BIN=$OUT/c-abi/header-consumer
HEADERS=$ROOT/crates/nshedit-abi/include

[ -f "$HEADERS/histedit.h" ] || die "missing generated histedit.h"
[ -f "$HEADERS/editline/readline.h" ] || die "missing generated readline.h"
command -v "$CC" >/dev/null || die "C compiler '$CC' is required"

stage_port_soname
mkdir -p -- "$(dirname -- "$BIN")" "$WORK"

"$CC" -std=c11 -O0 -g -Wall -Wextra -Werror \
    -I"$HEADERS" \
    "$CONF_DIR/aux/header_consumer.c" \
    -L"$PORT_LIB_DIR" -Wl,-rpath,"$PORT_LIB_DIR" -lnshedit \
    -o "$BIN"

LD_LIBRARY_PATH="$PORT_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
    "$BIN" "$WORK"

note "generated headers compile and the C consumer passes"
