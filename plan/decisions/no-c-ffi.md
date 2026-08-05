---
id [dec:libedit:no-c-ffi]
epitome "The port links no C libraries; a separate crate exports the C ABI so we drop in for libedit, and is the only place allowed to name a libc symbol."
state @decided
category @ban
scope {
    elements ([arch:libedit:c-abi])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Link libtinfo/libncurses via FFI for the terminal capability functions."
        rejected_because "Reintroduces the C dependency the port exists to remove, and drags in ncurses' allocation and global-state behaviour we would then have to match."
    }
    {
        option "Ship only a Rust-native API and let callers migrate."
        rejected_because "Drop-in replacement is the point: existing consumers link libedit and include histedit.h or editline/readline.h. Without the C ABI they cannot adopt this at all."
    }
    {
        option "Hold the ban absolute, so not even errno is written, and export a reader such as el_errno() for the values the sem rules promise."
        rejected_because "A C caller reads errno, not a symbol only this library publishes. The rules promise ENOSPC, ENOMEM, EINVAL and ERANGE at named entry points, and a reader nobody calls does not deliver them. The cost of the exception is nil: a cdylib already links the platform libc through std, exactly as libedit.so does, so declaring the accessor adds no dependency."
    }
)
consequences {
    accepted (
        "Anything libedit got from a C library must be reimplemented in Rust or sourced from a pure-Rust crate."
        "The C ABI surface is both histedit.h and editline/readline.h, so the readline compatibility layer stays in scope."
        "Observable behaviour that crosses the ABI is frozen, including on-disk formats."
        "Two crates: nshedit is the library nsh links, nshedit-abi exports the C symbols. See [dec:libedit:idiomatic-core]."
        "One artifact, libnshedit.so, carries both exported surfaces. Compatibility names are symlinks onto it — libedit.so.0 and libreadline.so.8 — which is how the readline ABI becomes a supported target rather than an accident."
        "The ban is on linking C libraries, not on naming a symbol of the libc the artifact already links. The ABI crate may declare a libc symbol where the C ABI cannot otherwise be honoured; the core crate may not, and no build.rs may hunt for a library either way."
        "That exception is currently spent exactly once, on the thread-local errno accessor (__errno_location on Linux). errno is caller-observable state the sem rules promise values in, and no pure-Rust API can write it. The core records into nshedit::errno and the ABI crate copies that into the C's errno at the entry point whose rule promises it."
    )
    deferred (
        "Claiming the readline soname makes libedit's incomplete readline emulation our problem: real consumers exercise more of that API than its header admits, so the compatibility tests must target programs rather than the header."
    )
}
establishes ([arch:libedit:c-abi])
---

## Rationale

The port is pure Rust end to end. No C library is linked, no `build.rs`
hunts for one to link against, and the only symbol the tree declares out
to the platform's libc is the single one named below, in the ABI crate.

The reason is not purity, which is also why the ban has a boundary
rather than being absolute. libedit's value is that it is already linked
into a great deal of software, reached through `histedit.h` or through
the GNU readline compatibility header. A rewrite that cannot be dropped
into that socket is a different project. So the deliverable is a Rust
core plus a crate that exports the C ABI, and the ABI is what pins the
behaviour: whatever a C caller can observe, we must reproduce.

That constraint reaches further than the function signatures. It covers
formats that cross the boundary — most sharply the history file, which
is why [[posix-only-scope]] keeps `vis`/`unvis` even though they are
BSD in origin. It also covers `errno`, and that is where the ban stops
being absolute.

`errno` is state a C caller reads after the call returns, and the `sem`
rules promise values in it: `ENOSPC` and `ENOMEM` from the `vis`
engine, `EINVAL` from `unvis`, `ERANGE` from `el_getc`, the failing
read's own value restored by `el_wgets`. Rust can read `errno` and
cannot write it, so honouring those rules means naming one libc symbol,
the thread-local accessor. Refusing would not remove a dependency —
`libnshedit.so` links the platform libc through `std` already, exactly
as `libedit.so` does — it would only make the library lie to its
callers about why a call failed.

So the ban is read for what it is for: no C *library* is linked, no
`build.rs` looks for one, and nothing that libedit got from a C library
is obtained by calling that library. Naming a symbol of the libc the
artifact already carries is a different act, and it is confined to the
ABI crate, which is where every other C-shaped thing lives. The core
stays ordinary Rust: it records the value in `nshedit::errno` and the
ABI crate copies it into the C's `errno` at the entry point whose rule
promises it. Anything else wanting an exception argues for it here
first; the test is that the C ABI cannot otherwise be honoured, not
that a libc call would be convenient.

Choosing no FFI is what forces the terminal capability question:
libedit's one hard library dependency was a termcap provider, and
banning FFI means replacing it rather than calling it. See
[[terminal-caps-via-term-crate]].
