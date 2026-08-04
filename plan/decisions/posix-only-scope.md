---
id [dec:libedit:posix-only-scope]
epitome "Port only what POSIX needs: portability shims and platform carve-outs are dropped, but vis/unvis stays because it is the history file format."
state @decided
category @ban
scope {
    elements ([arch:libedit:compat-shims])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Port the whole C tree symbol for symbol, shims included."
        rejected_because "Most of the shim layer exists to paper over pre-POSIX and non-POSIX libc gaps. In Rust it has nothing to paper over, so porting it would produce code with no callers and no meaning."
    }
    {
        option "Drop everything of BSD origin, vis and unvis among it."
        rejected_because "Origin is the wrong test. vis/unvis is not a portability shim, it is how history entries are escaped on disk, so dropping it breaks the drop-in guarantee."
    }
)
consequences {
    accepted (
        "src/sys.h leaves the port scope entirely: typedefs, __arraycount, _DIAGASSERT, __RCSID, u_int32_t, SIZE_MAX, the REGEX define, the libc prototypes and its copy of the Solaris termcap externs."
        "strlcpy, strlcat, getline, wcsdup and reallocarr leave scope; they are libc gap-fillers with no observable surface."
        "vis, unvis and vis.h stay in scope and get ported properly."
        "Search uses POSIX regcomp/regexec semantics; the BSD regexp and V7 re_comp branches are dead under the C's own REGEX define and are not ported."
    )
    deferred (
        "Platform carve-outs inside files that stay in scope, such as the __sun blocks in terminal.c, are judged per site rather than by a blanket rule."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
}
---

## Rationale

The target is POSIX. Code that exists only to survive systems that are
not POSIX, or to supply a libc function some platform lacks, has no
counterpart in Rust and is not ported.

`src/sys.h` is the clearest case: read end to end it is nothing but
shim — compiler attribute defines, `ptr_t` and `ioctl_t` typedefs,
`_DIAGASSERT` and `__RCSID` stubs, a `u_int32_t` fallback, a `SIZE_MAX`
fallback, prototypes for libc functions some platforms are missing, and
a Solaris-only redeclaration of the termcap externs. Not one line
survives translation. The same reasoning takes out `strlcpy.c`,
`strlcat.c`, `getline.c`, `wcsdup.c` and `reallocarr.c`, which the C
build itself compiles only when the host libc comes up short.

`vis`/`unvis` looks like it belongs on that list and does not. It is BSD
in origin, but origin is not the test — observability is. History
entries are written `strvis`-escaped and read back with `strunvis`, so
the encoding is the on-disk format of the history file. A libedit
replacement that cannot read a history file written by libedit is not a
replacement, which is the guarantee [[no-c-ffi]] exists to protect. It
stays, and it gets ported as carefully as anything else.

Regular expression matching resolves the same way by a different route.
The C offers three implementations, but `sys.h` hardcodes `REGEX`, so
POSIX `regcomp`/`regexec` is the only branch any modern build compiles.
The BSD `regexp` and V7 `re_comp`/`re_exec` paths are dead code and are
not ported. Note the semantics that survive are POSIX BRE, which is not
what a Rust regex crate gives by default.
