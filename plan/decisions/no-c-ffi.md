---
id [dec:libedit:no-c-ffi]
epitome "The port links no C libraries; a separate crate exports the C ABI so we drop in for libedit, and naming a libc symbol is a rationed exception spent in three enumerated places, never in the core."
state @decided
category @ban
scope {
    elements ([arch:libedit:c-abi] [arch:libedit:platform])
    rules (
        [spec:libedit:sem:el.el-init-fn]
        [spec:libedit:sem:el.el-wset-fn]
        [spec:libedit:sem:history.history-save-fp-fn]
        [spec:libedit:sem:readline.rl-initialize-fn]
    )
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
        "Three crates: nshedit is the library nsh links, nshedit-abi exports the C symbols, and nshedit-plat owns the syscalls. See [dec:libedit:idiomatic-core] and [dec:libedit:platform-layer]."
        "One artifact, libnshedit.so, carries both exported surfaces. Compatibility names are symlinks onto it — libedit.so.0 and libreadline.so.8 — which is how the readline ABI becomes a supported target rather than an accident."
        "The ban is on linking C libraries, not on naming a symbol of the libc the artifact already links. nshedit-abi and nshedit-plat may declare such a symbol; the core crate nshedit may not, whatever the argument; and no build.rs anywhere in the workspace may hunt for a library."
        "The test has two halves and an argument must carry both: no pure-Rust route delivers the facility, and a rule in the corpus specifies behaviour that cannot be implemented without it. A route that answers a nearby question — an /etc/passwd parse standing in for NSS — is a divergence to register, not a route; neither is a hand-rolled syscall the platform documents as undefined behaviour in a process that has a libc. Convenience, brevity, a shorter build, and 'it is what the C called' are not the test."
        "The exception is spent in three enumerated places, and the enumeration is the rule: a fourth site amends this decision before it is written. (1) The thread-local errno accessor (__errno_location on Linux) in nshedit-abi. errno is caller-observable state the sem rules promise values in, and no pure-Rust API can write it; the core records into nshedit::errno and the ABI crate copies that into the C's errno at the entry point whose rule promises it. (2) The signal family — sigaction, sigprocmask, and the self-signal (raise, or pthread_kill on the calling thread) — and the passwd family — getpwnam_r, getpwuid_r, setpwent/getpwent/endpwent — in nshedit-plat. rustix declines the first on principle and NSS backends are dlopened C objects with no pure-Rust route at all. See [dec:libedit:platform-layer]. (3) The caller's stdio stream — fileno, ftell and fputs — in nshedit-abi, and nowhere but its cstdio module. A caller's FILE * arrives on the public ABI and four rules are written in terms of what stdio does with it."
        "The second site is a widening. This decision first confined the exception to nshedit-abi on the test 'the C ABI cannot otherwise be honoured', which does not reach nsh: it links the core and never includes histedit.h, so it could not satisfy that test while still needing EL_SIGNAL and needing ~user to expand on a directory-joined host. The two-part test above replaces it and covers both consumers; the errno accessor passes it unchanged."
        "The third site carries both halves of the test. No pure-Rust route: a FILE * is an opaque handle into the C library's own object, whose buffer, position bookkeeping and descriptor live in memory that library owns and lays out differently in glibc, musl and the BSDs, so no crate in the ecosystem can read one it did not create — and the streams here are the application's, created before it called us. A rule specifies behaviour that needs it, four of them: sem:el.el-init-fn is nothing but el_init_fd(prog, fin, fout, ferr, fileno(fin), fileno(fout), fileno(ferr)); sem:el.el-wset-fn's EL_SETFP arm stores fileno(fp) beside the stream and has no el_init_fd-shaped escape hatch; sem:readline.rl-initialize-fn steps 4 and 5 take three more fileno calls; and sem:history.history-save-fp-fn decides its header on ftell(fp) == 0, writes with fputs and fprintf, and closes with 'the stream is neither flushed nor closed — the caller owns it'. All four were transcribed from the C before this question came up, which is what the second half is for."
        "fileno alone was considered and rejected, which would have held this site to one symbol: once a descriptor is in hand, std::fs::File::from_raw_fd carries the write with no further libc name. It answers a different question. A stream owns a userspace buffer the descriptor knows nothing about, so ftell counts bytes still pending in it and lseek does not — which flips sem:history.history-save-fp-fn's step-1 cookie decision on any stream the caller has already written to and not flushed — a write to the descriptor reaches the file ahead of bytes the caller wrote through the stream earlier, and the rule's closing clause is a positive statement that when the call returns the bytes are in the caller's buffer, not on the file. Flushing first would repair the first two and not the third, and is itself a stdio call, so it does not save the symbol either. That is the shape this decision already names: a route to a nearby question is a divergence to register, not a route. The descriptor route survives as nshedit::history::history_save_fd, for Rust callers that have a descriptor and no stream; H_SAVE_FP does not take it."
    )
    deferred (
        "Claiming the readline soname makes libedit's incomplete readline emulation our problem: real consumers exercise more of that API than its header admits, so the compatibility tests must target programs rather than the header."
        "stdin, stdout and stderr are NOT on the enumeration, and the third site does not reach them. They are data objects rather than functions — and on the BSDs macros over a static array, so not linkable as data at all — and the only rule that wants them is sem:readline.rl-initialize-fn step 3, which rewrites a NULL rl_instream/rl_outstream to the C's own stream object. The port instead falls back to descriptors 0, 1 and 2, which is what those three streams carry at process start; an application that reads rl_instream back after rl_initialize sees NULL where the C shows it stdin. That is a divergence for the register, and adding the objects to the enumeration would be a fourth site with its own argument."
        "The third site closes group 12 of [dec:libedit:platform-layer]'s divergence register except for one half: sem:readline.rl-initialize-fn's tcgetattr ECHO test needs both fileno and tcgetattr, and only the first is answered here. Until the platform layer lands, that test never succeeds, editmode is never cleared, and libedit edits on a non-echoing input where the C would not — the one entry in the whole register that fails open."
    )
}
establishes ([arch:libedit:c-abi])
---

## Rationale

The port is pure Rust end to end. No C library is linked, no `build.rs`
hunts for one to link against, and the symbols the tree declares out to
the platform's libc are the enumerated few named below — in two crates,
and never in the core.

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
artifact already carries is a different act, and it is rationed rather
than free. Anything wanting the ration argues for it here first, and the
argument has to carry two halves: **no pure-Rust route delivers the
facility**, and **a rule in the corpus specifies behaviour that cannot be
implemented without it**.

Each half stops a different drift. The first is a checkable claim about
the ecosystem rather than a preference — `rustix` documents the calls it
declines and why, and `dlopen`ed NSS modules have a C ABI and nothing
else — so *I would rather write one line than forty* is not a missing
route. It is not satisfied by a route to a nearby question either:
parsing `/etc/passwd` is pure Rust and is not a route to NSS, it is a
divergence, and the register is where divergences go. Nor by a route
that is unsound: a hand-rolled `rt_sigaction` inside a process that has
a libc is documented undefined behaviour, which is not a route at any
price. The second half ties the exception to something outside the
author's control. The `sem` and `def` rules were transcribed from the C
before any of these questions came up, so *the rule needs it* cannot be
arranged after the fact — and where no rule needs it, the alternative to
the libc call is a slightly longer implementation rather than a
permanent entry in the divergence register.

Then the sites are enumerated, which is the part that actually holds.
A test can be argued around; a list cannot be extended without editing
this file. Three entries stand: the `errno` accessor in `nshedit-abi`,
the signal and passwd families in `nshedit-plat`, and the caller's
`FILE *` in `nshedit-abi`. The core stays ordinary Rust — for `errno` it
records the value in `nshedit::errno` and the ABI crate copies it into
the C's `errno` at the entry point whose rule promises it; for the
stream it takes the two answers the ABI crate read off it and never
learns there was a `FILE *`.

The second entry is a widening, and the reason belongs on the record,
because the first pass of this rule got it wrong. The exception was
originally confined to `nshedit-abi` on the test *the C ABI cannot
otherwise be honoured*, and [[platform-layer]] honoured that by routing
the core's signal and passwd needs through a process-global hook. Both
rested on a false premise about who consumes what: nsh links `nshedit`,
the core, and never touches the C ABI. Under the old test nsh is not the
C ABI, so it could not benefit from the exception at all — and to get
`EL_SIGNAL`, or `~alice` on a host whose accounts live in LDAP, it would
have had to write its own libc-backed shim, a duplicate of the one
`nshedit-abi` already ships, imposed on the consumer this port exists
for. Two consumers needed one facility and the crate they share declined
to provide it. The test above is the repair: it asks whether the
facility is reachable and whether the behaviour is specified, which is
answerable for both consumers, instead of asking which header the caller
included.

The third entry is the caller's `FILE *`, and it is the first one added
under the two-part test rather than in spite of it. The first half is a
fact about the object: a `FILE *` is a handle into the C library's own
allocation. Its buffer, its position bookkeeping and its descriptor sit
in a struct glibc, musl and the BSDs each lay out differently and none
of them documents, and the streams that arrive here were created by the
application before it called us, so there is no Rust that owns them and
none that can be made to. The second half is four rules, all of them
transcribed from the C long before this question arose:
`sem:el.el-init-fn` **is** `fileno`, its entire body being one tail call
that computes three of them; `sem:el.el-wset-fn`'s `EL_SETFP` stores a
`fileno` beside the stream and, unlike `el_init`, has no `el_init_fd`
escape hatch; `sem:readline.rl-initialize-fn` steps 4 and 5 take three
more; and `sem:history.history-save-fp-fn` decides whether to write the
history cookie on `ftell(fp) == 0` and then writes with `fputs` and
`fprintf`.

The interesting part is what was rejected, because it looked like a way
to keep the site to a single symbol. Given `fileno`, the descriptor
behind the stream is an ordinary `RawFd` and `File::from_raw_fd` carries
every write without naming anything further. It does not work, and the
reason is that a stream is not its descriptor. The stream owns a
userspace buffer the descriptor knows nothing about, so `ftell` counts
what is still pending in that buffer and `lseek` does not — which turns
`history_save_fp`'s cookie test into a different test the moment the
caller has written to the stream and not flushed — a raw write reaches
the file *ahead* of bytes the caller wrote through the stream earlier,
inverting their order; and the rule's last line, *the stream is neither
flushed nor closed — the caller owns it*, is a positive claim about
where the bytes are when the call returns. Through the stream they are
in the caller's buffer, which is what the C promises and what a caller
that then `_exit`s or inspects `ftell` observes. An `fflush` first would
repair the position and the ordering and could not repair the third, and
is a stdio call itself, so it buys nothing. This is exactly the failure
mode the two-part test already names: parsing `/etc/passwd` is a route
to a nearby question and not to NSS, and the descriptor is a route to a
nearby question and not to the stream. The descriptor route is kept, as
`nshedit::history::history_save_fd`, for Rust callers that genuinely
have a descriptor and no stream — which is every consumer of the core.
`H_SAVE_FP` does not take it.

What the third site deliberately does not include is `stdin`, `stdout`
and `stderr`. Those are data objects, not functions, and on the BSDs not
even objects — `stdin` is a macro over a static array — and the only
rule that wants them is `rl_initialize`'s step 3, defaulting a NULL
`rl_instream`. Falling back to descriptors 0, 1 and 2 is what the port
does instead, and the difference an application can see — `rl_instream`
still NULL after `rl_initialize` where the C shows it `stdin` — goes in
the register rather than onto this list.

The core keeps the ban absolutely. `nshedit` names no libc symbol under
any argument — it is the surface [[idiomatic-core]] makes a deliverable
in its own right, and the families in question are syscalls, which
[[platform-layer]] puts behind a crate boundary regardless of who is
permitted to call them.

Choosing no FFI is what forces the terminal capability question:
libedit's one hard library dependency was a termcap provider, and
banning FFI means replacing it rather than calling it. See
[[terminal-caps-via-term-crate]].
