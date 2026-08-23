#!/usr/bin/env bash
#
# Install nshedit, with the platform's libedit-compatible link names.
#
#   packaging/install.sh [--prefix DIR] [--libdir DIR] [--includedir DIR]
#                        [--profile debug|release] [--target TRIPLE]
#                        [--no-compat] [--dry-run]
#
# --target names the triple a cross build was made for, so the object is taken
# from target/TRIPLE/PROFILE instead of target/PROFILE. It defaults to
# $NSHEDIT_TARGET, which is how the conformance stages point the installer at
# the build they are inspecting. It says where the object came from and not
# what this host can run.
#
# Linux installs one object under these names:
#
#   libnshedit.so.0.0.0                    the object
#   libnshedit.so.0    -> libnshedit.so.0.0.0   the SONAME
#   libnshedit.so      -> libnshedit.so.0       the -lnshedit link name
#   libedit.so         -> libnshedit.so.0   ] compat, --no-compat skips these
#   libedit.so.0       -> libnshedit.so.0   ]
#   libedit.so.2       -> libnshedit.so.0   ]
#
# macOS installs the same arrangement using Mach-O names:
#
#   libnshedit.0.0.0.dylib
#   libnshedit.0.dylib -> libnshedit.0.0.0.dylib
#   libnshedit.dylib   -> libnshedit.0.dylib
#   libedit.dylib      -> libnshedit.0.dylib   ] compat
#   libedit.3.dylib    -> libnshedit.0.dylib   ]
#
# WHY BOTH libedit.so.0 AND libedit.so.2. A shared object's SONAME is copied
# into every program that links it, as DT_NEEDED, and the loader then looks for
# a file with exactly that name — so the compat set has to cover every name
# libedit has presented itself under. Upstream and several distributions use
# libedit.so.0; Debian and Ubuntu use libedit.so.2. Shipping only one would
# leave the other population unable to load us at all.
#
# WHY NOT libreadline.so.8. We export 149 of its 736 symbols. A program needing
# any of the other 587 would fail at LOAD time with an unresolved symbol, which
# is strictly worse than our not being installed: the program worked with real
# readline and now does not start. The readline surface here is a compatibility
# layer over libedit's, not a reimplementation of GNU readline.
#
# The compat names are also filenames another libedit installation may claim.
# That is not something to create by accident, so every link is printed as it
# is made and --no-compat turns all of them off.

set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

prefix=/usr/local
libdir=
includedir=
profile=release
target=${NSHEDIT_TARGET:-}
compat=1
dry=0

die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case $1 in
        --prefix)     prefix=${2:?--prefix needs a directory}; shift 2 ;;
        --prefix=*)   prefix=${1#*=}; shift ;;
        --libdir)     libdir=${2:?--libdir needs a directory}; shift 2 ;;
        --libdir=*)   libdir=${1#*=}; shift ;;
        --includedir) includedir=${2:?--includedir needs a directory}; shift 2 ;;
        --includedir=*) includedir=${1#*=}; shift ;;
        --profile)    profile=${2:?--profile needs debug or release}; shift 2 ;;
        --profile=*)  profile=${1#*=}; shift ;;
        --target)     target=${2:?--target needs a triple}; shift 2 ;;
        --target=*)   target=${1#*=}; shift ;;
        --no-compat)  compat=0; shift ;;
        --dry-run)    dry=1; shift ;;
        -h|--help)    sed -n '2,53p' "${BASH_SOURCE[0]}" | sed 's/^#\{1,2\} \{0,1\}//'; exit 0 ;;
        *)            die "unknown argument: $1" ;;
    esac
done

libdir=${libdir:-$prefix/lib}
includedir=${includedir:-$prefix/include}

case $profile in
    debug|release) ;;
    *) die "--profile must be debug or release, not $profile" ;;
esac

# ---------------------------------------------------------------------------
# What we are installing, and whether it is what we think it is.
# ---------------------------------------------------------------------------

host=$(uname -s)
case $host in
    Linux)
        RUNTIME_NAME=libnshedit.so.0
        REAL=libnshedit.so.0.0.0
        LINK_NAME=libnshedit.so
        built=$ROOT/target${target:+/$target}/$profile/libnshedit.so
        compat_names=(libedit.so libedit.so.0 libedit.so.2)
        ;;
    Darwin)
        RUNTIME_NAME=libnshedit.0.dylib
        REAL=libnshedit.0.0.0.dylib
        LINK_NAME=libnshedit.dylib
        built=$ROOT/target${target:+/$target}/$profile/libnshedit.dylib
        compat_names=(libedit.dylib libedit.3.dylib)
        ;;
    *)
        die "unsupported installation host: $host"
        ;;
esac

[ -f "$built" ] || die "no $built — run 'cargo build --$profile${target:+ --target $target}' first
(for the debug profile, 'cargo build' and pass --profile debug)"

# The whole layout below is derived from the runtime name, so a mismatch means
# every symlink would point somewhere nothing will look. Check it when the
# platform's object inspection tool is available.
if [ "$host" = Linux ] && command -v readelf > /dev/null 2>&1; then
    have=$(readelf -d "$built" 2>/dev/null | sed -n 's/.*Library soname: \[\(.*\)\].*/\1/p')
    [ -n "$have" ] || die "$built carries no SONAME — crates/nshedit-abi/build.rs did not run"
    [ "$have" = "$RUNTIME_NAME" ] || die "$built carries SONAME $have, not $RUNTIME_NAME"
elif [ "$host" = Darwin ] && command -v otool > /dev/null 2>&1; then
    have=$(otool -D "$built" 2>/dev/null | awk 'NR == 2 { print $1 }')
    [ -n "$have" ] || die "$built carries no install name — crates/nshedit-abi/build.rs did not run"
    expected=@rpath/$RUNTIME_NAME
    [ "$have" = "$expected" ] || die "$built carries install name $have, not $expected"
fi

headers=$ROOT/crates/nshedit-abi/include
[ -f "$headers/histedit.h" ] || die "no $headers/histedit.h — regenerate with
'cargo run -p nshedit-abi --example gen-headers'"

# ---------------------------------------------------------------------------
# Do it, narrating.
# ---------------------------------------------------------------------------

run() {
    printf '  %s\n' "$*"
    [ "$dry" -eq 1 ] || "$@"
}

printf 'nshedit -> %s (profile %s%s)\n' "$prefix" "$profile" \
    "$([ "$dry" -eq 1 ] && printf ', dry run')"

run mkdir -p -- "$libdir" "$includedir/editline" "$libdir/pkgconfig"
run install -m 755 -- "$built" "$libdir/$REAL"
# -f so a reinstall replaces the previous link rather than failing, -n so a
# link that already points at a directory is replaced rather than followed
# into it.
run ln -sfn -- "$REAL" "$libdir/$RUNTIME_NAME"
run ln -sfn -- "$RUNTIME_NAME" "$libdir/$LINK_NAME"

run install -m 644 -- "$headers/histedit.h" "$includedir/histedit.h"
run install -m 644 -- "$headers/editline/readline.h" "$includedir/editline/readline.h"

if [ "$compat" -eq 1 ]; then
    printf '\ncompat names, each claiming a filename libedit would also install:\n'
    for name in "${compat_names[@]}"; do
        run ln -sfn -- "$RUNTIME_NAME" "$libdir/$name"
    done
    # So that `pkg-config --cflags --libs libedit` resolves, which is how most
    # build systems find libedit rather than by guessing at -ledit.
    if [ "$dry" -eq 1 ]; then
        printf '  write %s\n' "$libdir/pkgconfig/libedit.pc"
    else
        cat > "$libdir/pkgconfig/libedit.pc" <<EOF
prefix=$prefix
libdir=$libdir
includedir=$includedir

Name: libedit
Description: Command line editing, history and tokenization. Provided by nshedit.
Version: 3.1
Requires:
Libs: -L\${libdir} -ledit
Cflags: -I\${includedir} -I\${includedir}/editline
EOF
        printf '  write %s\n' "$libdir/pkgconfig/libedit.pc"
    fi
else
    printf '\nno compat names (--no-compat): -ledit will not select this install.\n'
fi

if [ "$host" = Linux ]; then
    printf '\ndone. A newly linked program will record DT_NEEDED %s, which is what\n' "$RUNTIME_NAME"
    printf 'it actually loaded; the libedit names carry binaries linked before us.\n'
else
    printf '\ndone. A newly linked program will record @rpath/%s.\n' "$RUNTIME_NAME"
    printf 'The libedit names let -ledit select that object at link time.\n'
fi
if [ "$host" = Linux ] && [ "$compat" -eq 1 ] && [ "$dry" -eq 0 ]; then
    printf '\nIf %s is on the default search path, this now shadows the\n' "$libdir"
    printf "system libedit for every process that starts. That is the intent, but run\n"
    printf 'ldconfig and check `ldd` on something that matters before assuming it worked.\n'
fi
