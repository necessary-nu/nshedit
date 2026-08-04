---
id [dec:libedit:no-c-ffi]
epitome "The port links no C libraries; a separate crate exports the C ABI so we drop in for libedit."
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
)
consequences {
    accepted (
        "Anything libedit got from a C library must be reimplemented in Rust or sourced from a pure-Rust crate."
        "The C ABI surface is both histedit.h and editline/readline.h, so the readline compatibility layer stays in scope."
        "Observable behaviour that crosses the ABI is frozen, including on-disk formats."
        "Two crates: nshedit is the library nsh links, nshedit-abi exports the C symbols. See [dec:libedit:idiomatic-core]."
        "One artifact, libnshedit.so, carries both exported surfaces. Compatibility names are symlinks onto it — libedit.so.0 and libreadline.so.8 — which is how the readline ABI becomes a supported target rather than an accident."
    )
    deferred (
        "Claiming the readline soname makes libedit's incomplete readline emulation our problem: real consumers exercise more of that API than its header admits, so the compatibility tests must target programs rather than the header."
    )
}
establishes ([arch:libedit:c-abi])
---

## Rationale

The port is pure Rust end to end. No `extern "C"` calls out to system
libraries, no `build.rs` that hunts for a C library to link against.

The reason is not purity. libedit's value is that it is already linked
into a great deal of software, reached through `histedit.h` or through
the GNU readline compatibility header. A rewrite that cannot be dropped
into that socket is a different project. So the deliverable is a Rust
core plus a crate that exports the C ABI, and the ABI is what pins the
behaviour: whatever a C caller can observe, we must reproduce.

That constraint reaches further than the function signatures. It covers
formats that cross the boundary — most sharply the history file, which
is why [[posix-only-scope]] keeps `vis`/`unvis` even though they are
BSD in origin.

Choosing no FFI is what forces the terminal capability question:
libedit's one hard library dependency was a termcap provider, and
banning FFI means replacing it rather than calling it. See
[[terminal-caps-via-term-crate]].
