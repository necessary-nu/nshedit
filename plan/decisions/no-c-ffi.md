---
id [dec:libedit:no-c-ffi]
epitome "The workspace links no optional C library; enumerated libc facilities are isolated in nshedit-plat and nshedit-abi, never nshedit."
state @decided
category @ban
scope {
    elements ([arch:libedit:c-abi] [arch:libedit:platform] [arch:libedit:core])
    rules (
        [spec:libedit:sem:el.el-init-fn]
        [spec:libedit:sem:el.el-wset-fn]
        [spec:libedit:sem:history.history-save-fp-fn]
        [spec:libedit:sem:readline.rl-initialize-fn]
        [spec:nshedit:req:core.rust-io]
        [spec:nshedit:req:core.unsafe-free]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Link libtinfo, ncurses, or the reference libedit for difficult facilities."
        rejected_because "The project would cease to be a Rust implementation and would inherit the foreign library's global state, allocation, and deployment constraints."
    }
    {
        option "Ban every libc symbol, including those required to interoperate with caller-owned C objects."
        rejected_because "A C caller observes errno and passes opaque FILE objects whose buffering and standard-stream identity cannot be reconstructed from a descriptor. Refusing those facilities breaks specified ABI behaviour without removing the libc already linked by Rust std."
    }
    {
        option "Permit libc anywhere a wrapper is convenient."
        rejected_because "An unbounded exception would spread C representation and unsafe code into the native core. The facility and owning boundary must remain enumerable."
    }
)
consequences {
    accepted (
        "No build script searches for or links an optional C library. Terminal capabilities and other former library facilities are implemented in Rust."
        "nshedit contains no foreign declarations or unsafe code. It returns typed errors and operates on safe platform and I/O interfaces."
        "nshedit-plat owns the platform libc facilities for which no sound pure-Rust route exists: the signal family and NSS passwd family. It exposes safe operations to its consumers."
        "nshedit-abi owns C interoperability facilities: the thread-local errno accessor and one cstdio module for fileno, position, buffered writes, formatting, flushing when the reference requires it, and access to the process's actual standard streams."
        "stdin, stdout, and stderr are part of the existing cstdio exception, not a fourth category. The cstdio module uses target-specific functions, data symbols, or documented accessors behind cfg while presenting one internal interface."
        "A caller-owned FILE object remains entirely in nshedit-abi. The core receives only safe semantic read, write, flush, and descriptor operations and never learns the object's representation."
        "Every new foreign declaration requires this decision to be amended with both the missing pure-Rust route and the spec rule that makes the facility observable."
    )
    deferred (
        "Targets whose libc exposes standard streams only through a different documented accessor require a target-specific cstdio implementation before that target is supported."
    )
}
edges {
    related_to ([dec:libedit:platform-layer] [dec:libedit:opaque-abi-adapter])
}
codifies (
    [spec:nshedit:req:core.rust-io]
    [spec:nshedit:req:core.unsafe-free]
)
establishes ([arch:libedit:c-abi])
---

## Rationale

The ban is about dependencies and ownership, not pretending a `cdylib` can
interoperate with C without touching the platform libc already used by Rust's
standard library. Most facilities have a sound Rust route and must take it.
The remaining cases are narrow and dictated by an observable contract.

Signals and NSS belong to the platform boundary because native Rust consumers
need them too and rustix deliberately does not replace them. Errno and stdio
belong to the ABI because they exist only as C caller state. In particular, a
`FILE *` is not its descriptor: buffering, position, write ordering, and the
identity of the process standard streams reside in the C library's opaque
object. Converting it to a raw descriptor would implement a nearby but
different contract.

The core therefore sees neither family of representation. `nshedit-plat`
contains the platform unsafety behind safe functions, while `nshedit-abi`
contains the foreign objects and converts typed core failures to errno. The
enumeration keeps that exception reviewable and prevents compatibility
mechanics from leaking into the native API.
