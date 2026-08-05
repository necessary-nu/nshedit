---
id [dec:libedit:platform-layer]
epitome "One crate owns the syscalls: rustix wherever it reaches, and the two families it declines — signals and passwd — are libc symbols named in nshedit-plat itself, so the core calls them with no hook to install."
state @decided
category @existence
scope {
    elements ([arch:libedit:platform] [arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:libedit:sem:tty.tty-getty-fn]
        [spec:libedit:sem:tty.tty-setty-fn]
        [spec:libedit:sem:tty.tty-setup-fn]
        [spec:libedit:sem:tty.tty-init-fn]
        [spec:libedit:sem:tty.tty-rawmode-fn]
        [spec:libedit:sem:tty.tty-cookedmode-fn]
        [spec:libedit:sem:tty.tty-quotemode-fn]
        [spec:libedit:sem:tty.tty-noquotemode-fn]
        [spec:libedit:sem:tty.tty-end-fn]
        [spec:libedit:sem:tty.tty-stty-fn]
        [spec:libedit:sem:tty.tty-bind-char-fn]
        [spec:libedit:sem:tty.tty-getspeed-fn]
        [spec:libedit:sem:tty.tty-get-signal-character-fn]
        [spec:libedit:sem:terminal.terminal-get-size-fn]
        [spec:libedit:sem:terminal.terminal-change-size-fn]
        [spec:libedit:sem:terminal.terminal-set-fn]
        [spec:libedit:sem:terminal.terminal-setflags-fn]
        [spec:libedit:sem:terminal.terminal-telltc-fn]
        [spec:libedit:sem:terminal.terminal-echotc-fn]
        [spec:libedit:sem:terminal.tputs-fn]
        [spec:libedit:sem:terminal.terminal-tputs-fn]
        [spec:libedit:sem:sig.sig-set-fn]
        [spec:libedit:sem:sig.sig-clr-fn]
        [spec:libedit:sem:sig.sig-handler-fn]
        [spec:libedit:sem:read.read-fixio-fn]
        [spec:libedit:sem:read.read-char-fn]
        [spec:libedit:sem:read.el-wgetc-fn]
        [spec:libedit:sem:read.el-wgets-fn]
        [spec:libedit:sem:read.read-prepare-fn]
        [spec:libedit:sem:read.read-finish-fn]
        [spec:libedit:sem:el.el-init-fn]
        [spec:libedit:sem:el.el-init-internal-fn]
        [spec:libedit:sem:el.el-resize-fn]
        [spec:libedit:sem:el.secure-getenv-fn]
        [spec:libedit:sem:el.el-source-fn]
        [spec:libedit:sem:el.el-wset-fn]
        [spec:libedit:sem:el.el-end-fn]
        [spec:libedit:sem:el.el-reset-fn]
        [spec:libedit:sem:el.el-editmode-fn]
        [spec:libedit:sem:filecomplete.fn-tilde-expand-fn]
        [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]
        [spec:libedit:sem:filecomplete.append-char-function-fn]
        [spec:libedit:sem:readline.rl-initialize-fn]
        [spec:libedit:sem:readline.readline-fn]
        [spec:libedit:sem:readline.rl-prep-terminal-fn]
        [spec:libedit:sem:readline.rl-deprep-terminal-fn]
        [spec:libedit:sem:readline.rl-reset-after-signal-fn]
        [spec:libedit:sem:readline.rl-resize-terminal-fn]
        [spec:libedit:sem:readline.rl-get-screen-size-fn]
        [spec:libedit:sem:readline.rl-event-read-char-fn]
        [spec:libedit:sem:readline.el-rl-tstp-fn]
        [spec:libedit:sem:readline.rl-echo-signal-char-fn]
        [spec:libedit:sem:readline.default-history-file-fn]
        [spec:libedit:sem:readline.username-completion-function-fn]
        [spec:libedit:sem:readline.tilde-expand-fn]
        [spec:libedit:sem:history.history-save-fp-fn]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep the platform code as private `plat` modules inside the core, one per consumer, as it is today."
        rejected_because "Six copies is how the divergences got lost in the first place: three of them declare their own `sigset_t` stand-in and two their own `/etc/passwd` parser, and every one of them documents its own gap in its own words. Nothing enumerates them together, so nobody can answer what the port cannot do."
    }
    {
        option "One `plat` module inside the core, public so the ABI crate can reach it."
        rejected_because "It solves the duplication but puts `tcsetattr` and `sigaction` into the core's public API, which is the surface nsh links and which [dec:libedit:idiomatic-core] makes a deliverable in its own right. A crate boundary keeps the syscall surface out of that namespace and makes the dependency auditable in one manifest."
    }
    {
        option "Put the platform layer inside the ABI crate, where C-shaped machinery is already allowed."
        rejected_because "It inverts the dependency the register already fixes: nsh links the core, not the ABI crate, so the core would need the ABI crate to put a terminal into raw mode. The core would not be a usable library on its own, which is the whole point of the split."
    }
    {
        option "nix for the syscalls."
        rejected_because "Widening the libc exception does not rescue it. [dec:libedit:no-c-ffi]'s test is per-facility and turns on there being no pure-Rust route; nix routes the entire surface through libc — termios, ioctl and the uid queries included, where rustix has one — so the pure-Rust majority is surrendered to buy the signal minority and the exception stops being enumerable, which is the property that keeps it from spreading. Its typed wrappers also need conversion at every `def`-rule boundary because `Termios`, `SigAction` and `SigSet` are frozen to the C's shape, and it brings cfg_aliases, memoffset and pin-utils. Licence is clean (MIT); the cost is architectural."
    }
    {
        option "Hand-rolled syscalls: an asm! wrapper per call."
        rejected_because "rt_sigaction needs an SA_RESTORER trampoline per architecture, and rustix's own maintainers document raw signal syscalls as undefined behaviour inside a process that already has a libc. The ABI crate is a cdylib loaded into exactly such processes, so this is the one place hand-rolling is not merely risky but wrong."
    }
    {
        option "Keep nshedit-plat pure rustix and have the core reach signals and passwd through a process-global hook with a built-in default, the libc-backed implementation being installed by nshedit-abi."
        rejected_because "This is what the first pass of this decision chose, and it was chosen on a false premise: that nsh would consume the port through the C ABI. nsh links the core, so the hook hands the port's primary consumer machinery it has to build itself — a libc-backed signal and passwd shim duplicating the one nshedit-abi already ships — merely to get EL_SIGNAL and ~user on a directory-joined host. The property it was defending turns out not to exist: nsh is a POSIX shell, so it handles SIGINT, SIGCHLD, SIGTSTP/SIGTTOU and tcsetpgrp itself and links a signal API whatever nshedit does, and rustix declines sigaction for nsh on exactly the grounds it declines it for us. 'A core whose only kernel route is rustix' therefore kept the declaration out of one crate by forcing the identical declaration into the next one. Two consumers, one facility, and the shared crate declining to supply it: this is the friction the first pass deferred a question about, and the amendment it named."
    }
    {
        option "Name the libc symbols in nshedit-plat and offer no override at all."
        rejected_because "What failed about the hook was that it was mandatory, not that a seam existed. One process-global slot per family, defaulting to nshedit-plat's own implementation and installed by nobody, costs a static and a null check, and buys an embedder that must route signal arming through its own bookkeeping — or answer passwd lookups from a cache rather than a blocking NSS call inside a keystroke — a way to do it without forking the crate."
    }
    {
        option "Reach the user database through NSS without libc, by speaking the SSSD socket or the systemd-userdb varlink protocol directly."
        rejected_because "It is a name-service client rewrite that covers two backends and misses nss_ldap, NIS and everything else, and it answers a question that one getpwnam_r call answers completely and correctly."
    }
    {
        option "Settle for parsing /etc/passwd everywhere, and accept that directory users get no tilde expansion."
        rejected_because "On a host whose accounts come from LDAP, SSSD, AD or systemd-homed the invoking user is usually absent from /etc/passwd, so it is not only `~alice` that breaks — bare `~` and `~/...` break for the person at the keyboard. getpwnam_r is what the C calls and what the rule specifies, and with the exception widened there is nothing left standing between the core and it."
    }
)
consequences {
    accepted (
        "A third workspace crate, nshedit-plat, is the only place in the workspace that issues a syscall. Both nshedit and nshedit-abi depend on it; nothing else does."
        "rustix 1.1.x is nshedit-plat's route for everything rustix reaches: termios, TIOCGWINSZ, fcntl and the uid/gid queries — five of the eight holes, completely. Licence: Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT, and we take MIT OR Apache-2.0. Transitively on Linux it is exactly linux-raw-sys (same triple) and bitflags (MIT OR Apache-2.0). No WTFPL anywhere in that tree."
        "On aarch64 and x86_64 Linux rustix selects its linux_raw backend and issues syscalls directly, so nothing in that five-eighths of the layer goes near a libc. Cargo.lock will still name libc and errno because they are non-optional in rustix's libc-backend target block; they are never compiled for our targets."
        "rustix declines signals on principle — not_implemented.rs lists sigaction, sigprocmask and sigwait as out of scope because a libc expects to be involved, and the runtime module's replacements are documented as undefined behaviour in a process that has a libc, which is exactly what the exported cdylib is loaded into. So sigaction, sigprocmask and pthread_kill are libc symbols, and so are getpwnam_r, getpwuid_r and setpwent/getpwent/endpwent, whose NSS backends are dlopened C objects with no pure-Rust route at all."
        "Those two families are named in nshedit-plat directly, under [dec:libedit:no-c-ffi]'s libc exception, which this decision widens from the ABI crate to the platform crate. The widening is argued and recorded there, in the decision that owns the rule. The core crate still names no libc symbol, and no build.rs hunts for a library."
        "The core calls them the way it calls tcsetattr. There is no hook to install, nothing to arm and no built-in default to fall back to: a consumer that links nshedit gets EL_SIGNAL and NSS-backed tilde expansion by linking it, and nshedit-abi installs nothing."
        "An override survives, optional and installed by nobody: one process-global slot per family, defaulting to nshedit-plat's implementation. It is there for an embedder that must route signal arming through its own bookkeeping, or answer passwd lookups from a cache instead of a blocking NSS call inside a keystroke. Nothing has to install one for the specified behaviour to hold — that mandatory quality is what sank the hook."
        "This moves where the capability lives, not when it fires. EL_SIGNAL still defaults to off and libedit still installs no handler until a caller asks, so a shell that manages its own SIGINT, SIGCHLD and job control keeps every disposition it had. sem:sig.sig-set-fn's contract is unchanged in every particular except that it can now be honoured."
        "The declarations are transcribed in nshedit-plat alongside the termios ABI and signal numbers the layer already transcribes — struct sigaction, sigset_t, struct passwd — so no crate joins the graph, cargo tree -p nshedit is unchanged, and the workspace's whole libc surface is two extern blocks a reader can count."
        "The user database goes through NSS, via getpwnam_r and getpwuid_r, for every consumer — so a user that exists only in LDAP, SSSD, AD, NIS or systemd-homed resolves exactly as it does for the C. The cost is the one the rule already names, now paid by default rather than opted into: the lookup can block on a network name service, inside a completion keystroke."
        "Both /etc/passwd parsers are retired outright rather than kept as a fallback. Nothing needs to answer when no hook is installed, because there is no hook, and a parse sitting behind getpwnam_r would disagree with the C in exactly the case sem:filecomplete.fn-tilde-expand-fn pins: any non-zero return, ERANGE included, reads as no such user, and a hand parser has no 1024-byte limit to hit."
        "A static-musl build gets musl's files-only getpwnam_r, which is the /etc/passwd behaviour again. That is a property of the target, not of this decision, and it is the same answer a statically linked C libedit gives."
        "What the widening costs: what does the core link was answerable from one manifest and is now answerable from one manifest plus one enumerated extern block, read rather than queried; and a target without a libc could not link the signal and passwd paths. Neither reaches nsh, which links std and therefore a libc, and handles signals itself regardless."
        "The platform layer is Linux-shaped, following posix-only-scope and the precedent tty.rs already set: the termios ABI, the V* subscripts, the signal numbers, the struct layouts above and _POSIX_VDISABLE are all transcribed for Linux/glibc. rustix supplies the ones it can portably; the rest stay transcribed."
        "Order matters. Landing termios before signals makes ^Z strictly worse than today: raw mode with no SIGTSTP handler leaves a suspended process behind a terminal in raw mode, where today nothing is raw so nothing is broken. platform-build lands tcgetattr/tcsetattr and sigaction together or neither."
        "The two /proc readers already written stay, because they answer things no syscall does: /proc/self/auxv for AT_SECURE, which sem:el.secure-getenv-fn names as one of its three conditions and which rustix exposes only through the same unsafe runtime module. The uid/gid half moves to getuid/geteuid/getgid/getegid."
        "Retiring the stubs changes behaviour that tests may already pin, because every one of them degrades into a state the C also defines. Any test asserting NO_TTY, a zero baud rate, an unexpanded tilde or a silent el_resize is asserting the stub, not the C."
    )
    deferred (
        "Whether nshedit-plat is one flat module or splits by facility. It has one caller each for most functions, so flat until it is not."
        "The ABI crate's FILE * surface — fileno, and fputs/ftell through a caller-supplied stream — would be a third site for [dec:libedit:no-c-ffi]'s exception and is not yet one: the enumeration there is closed until it is argued there. Either way it is C-representation machinery rather than a syscall, so it is nshedit-abi's to build and is not this layer's scope. It is listed in the divergence register because nothing else records it."
        "Whether the two override slots earn their keep. They cost a static and a null check each, nothing installs them today, and if no embedder asks they are deletable without touching a rule."
        "Whether ioctl(FIONREAD) is worth supplying. Two rules want it — read.el-wgets-fn's typeahead pre-check and readline.rl-event-read-char-fn's poll — and the first does not compile in the C on glibc, so only the readline one is a real obligation."
        "Whether a non-Linux build is supported at all. rustix falls back to its libc backend off Linux, and the transcribed termios and signal numbers are wrong there regardless, so the honest answer is probably no."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi] [dec:libedit:idiomatic-core] [dec:libedit:posix-only-scope] [dec:libedit:conformance-policy])
    related_to ([dec:libedit:terminal-caps-via-term-crate])
}
establishes ([arch:libedit:platform])
---

## Rationale

Barring libc left eight holes in the port, not six. Each was papered over
where it was found, in a private module, in that module's own words:
`read.rs` for `fcntl`, `sig.rs` for the signal calls, `terminal.rs` for
`ioctl` and a second copy of the signal mask, `tty.rs` for `tcgetattr`
and `tcsetattr`, `el.rs` for `issetugid` and the uid comparison,
`filecomplete.rs` for `getpwnam_r`, and — the two the survey turned up —
`nshedit-abi/src/readline.rs`, which carries its own `/etc/passwd`
parser and its own `/proc/self/status` reader for `getuid`, and
`history.rs`, which cannot write through a caller's `FILE *` at all.

Every one of them is honest about its own gap and none of them can see
the others, which is the failure mode. Three separate modules declare a
`sigset_t` stand-in that holds nothing. Two parse `/etc/passwd`
independently, in different crates, with different field handling. And
the aggregate — what the library actually cannot do today — is written
down nowhere, so it cannot be tested, scheduled or told to a user. That
aggregate is the second half of this document.

### Where it lives

A crate: `nshedit-plat`, depended on by both `nshedit` and
`nshedit-abi`.

The obvious alternative is a `plat` module in the core, next to `locale`
and `errno`, which have no C counterpart either. It would work. What
decides against it is that the ABI crate needs the same primitives —
`tcgetattr` for `rl_initialize`'s `ECHO` test, `ioctl(FIONREAD)` for
the event-hook poll, `tcsetattr` for the `tty_init`/`tty_end` pair
`readline()` runs per line — and the only way it can reach a core
module is for that module to be `pub`. `errno` sets that precedent and
it is a small one: a thread-local integer. `tcsetattr` is not small.
[[idiomatic-core]] says the core's public API is a deliverable designed
for nsh, and nsh does not want a syscall surface in that namespace.

Putting the layer in the ABI crate is worse and can be dismissed
outright. nsh links the core; the register already records
`[arch:libedit:c-abi]` as depending on `[arch:libedit:core]`. Making
raw mode an ABI-crate facility would mean the library cannot edit a
line without the C compatibility shim loaded.

So the honest trade-off is a third crate against a `pub` module, and the
crate wins on two counts that are not aesthetic. The syscall dependency
is declared in exactly one manifest, so `cargo tree -p nshedit` stays a
short answer to *what does the core link* — short, though after the
widening below not a complete one on its own: one `extern` block in the
same crate is the rest of it. And the layer
can be built and exercised against a real kernel without the 35,000
lines above it.

### How the syscalls are reached

`rustix` wherever `rustix` reaches, which is most of them, and the
platform's libc for the two families it declines.

`rustix` 1.1.x on Linux selects its `linux_raw` backend and issues
syscalls directly: no libc linkage, no `build.rs` probing, and a
dependency tree of exactly `linux-raw-sys` and `bitflags`. Its licence
is `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` and
`linux-raw-sys` matches it; `bitflags` is `MIT OR Apache-2.0`. We take
the MIT or the plain Apache-2.0 arm and never touch the LLVM-exception
one. Nothing in that tree is WTFPL. (The rejected `terminfo` crate
still is — verified, `license = "WTFPL"` — so
[[terminal-caps-via-term-crate]] stands.)

That covers `tcgetattr`, `tcsetattr`, `tcgetwinsize` (which is
`TIOCGWINSZ`), `fcntl` with `F_GETFL`/`F_SETFL`, `isatty`, and
`getuid`/`geteuid`/`getgid`/`getegid`. Five of the eight holes close
completely, and with them everything in groups 1 through 7 of the
register below — which is to say, the line editor.

It does not cover signals, and not by omission. rustix's
`not_implemented.rs` lists `sigaction`, `sigprocmask`, `sigwait` and
`tkill` as deliberately out of scope, on the grounds that a libc
expects to be involved in signal handling. Its `runtime` module has
replacements, and its own documentation says calling them in a process
that has a libc is undefined behaviour. `nshedit-abi` builds a `cdylib`
whose entire reason for existing is to be loaded into C programs, and
nsh links `std`, which links a libc too — so there is no consumer for
whom the replacements are defined. That rules out rustix's runtime
module and a hand-rolled `rt_sigaction` alike, and the hand-rolled
version would additionally need an `SA_RESTORER` trampoline written in
assembly per architecture.

`nix` would cover both halves under a clean MIT and loses anyway, and
widening the exception does not save it. It depends on `libc`
unconditionally and routes the *whole* surface through it — termios,
`ioctl` and the uid queries included, where rustix has a pure-Rust
route — so it fails the first half of [[no-c-ffi]]'s test everywhere
except the two families that actually need it, and an exception granted
that broadly stops being enumerable. Its wrappers also wrap types that
`def` rules have already frozen to the C's shape, so every call site
converts, and it adds three more crates.

So `sigaction`, `sigprocmask` and `pthread_kill` are libc symbols with
nowhere pure to go, and so are `getpwnam_r` and its neighbours. They are
declared in `nshedit-plat`, and the core calls them the way it calls
`tcsetattr`.

That is a widening of [[no-c-ffi]], which first confined the exception
to the ABI crate, and it is argued there rather than assumed here. The
short form: the original test was *the C ABI cannot otherwise be
honoured*, which is the wrong question to ask about this port's own
consumers. nsh links `nshedit` and never includes `histedit.h`, so it is
not the C ABI and could never meet that test — while still needing
`EL_SIGNAL`, and still needing `~alice` to expand on a host whose
accounts live in LDAP. The replacement test asks whether a pure-Rust
route exists and whether a rule specifies the behaviour. Both questions
are answerable for both consumers, and both answer yes here.

An override survives on each family — one process-global slot,
defaulting to `nshedit-plat`'s own implementation — for an embedder that
must route signal arming through its own bookkeeping or answer passwd
lookups from a cache. Nothing installs one, and nothing has to.

### What the first pass decided, and why this reverses it

The first pass put these two families on the ABI crate's side of the
exception and had the core reach them through a process-global hook with
a built-in default, on the model of `el_getenv`. The shape was
defensible in itself. What made it wrong was a factual error one layer
up: it was written believing nsh would consume the port through the C
ABI, so the hook looked like a seam only an unusual embedder would ever
meet, with `nshedit-abi` installing the real implementation for
everybody else.

nsh links the core. Under that design the port's primary consumer is
precisely the consumer that gets the built-in default — no signal
handling at all, and an `/etc/passwd` parse that misses the person at
the keyboard on a directory-joined workstation — unless it writes a
libc-backed signal and passwd shim of its own, byte for byte the one
`nshedit-abi` already carries. Two consumers, one facility, and the
crate they share declining to supply it. That is what the deferred
question meant by friction, and this is the amendment it named.

The property the hook was defending does not survive inspection either.
It was *a core whose only kernel route is rustix*. nsh is a POSIX shell:
it handles `SIGINT` on the foreground job, reaps on `SIGCHLD`, does job
control with `SIGTSTP`/`SIGTTOU` and `tcsetpgrp`. There is no more a
pure-Rust route to `sigaction` for nsh than there is for us — rustix
declines it for nsh on the same documented grounds — so nsh links a
signal API whatever `nshedit` does. Keeping the declaration out of
`nshedit-plat` would have relocated it into nsh rather than eliminating
it, and charged nsh a duplicate implementation for the privilege. It was
not a close call and should not be recorded as one.

Two things are genuinely lost, and they are small. *What does the core
link* was answerable from one manifest and is now answerable from one
manifest plus one `extern` block — enumerated in [[no-c-ffi]], but read
rather than queried. And the safe default changes character: a passwd
lookup that would have been a local file parse until somebody opted into
NSS is an NSS call for everyone, so a completion keystroke can block on
a name service by default, including in a test on a host with an
interesting `nsswitch.conf`. That is the C's behaviour and the rule's,
it is the only thing that makes directory users work at all, and the
override slot is there for a caller that must not pay it.

One thing that might look lost is not. Under the hook, an embedder had
to consciously install something before libedit could touch a signal
disposition, which read as a safety property. It was not doing that
work: `EL_SIGNAL` defaults to off, `sem:sig.sig-set-fn` runs only when a
caller turns it on, and a shell that owns its dispositions keeps them
either way. This decision moves where the capability lives, not whether
libedit reaches for it.

### The user database

`getpwnam_r` and `getpwuid_r`, through NSS. `/etc/passwd` is not the
answer, and once the exception reaches `nshedit-plat` it is not the
fallback either.

The difference is not academic and it is not small. On a workstation
joined to a directory — LDAP with `nss_ldap`, SSSD, AD, `nss_systemd`
for `systemd-homed`, or NIS — user accounts are not in `/etc/passwd`.
Parsing that file, which is what the port does today, means `~alice`
does not expand for any such user. Worse, the *invoking* user is
usually one of them, so `getpwuid_r(getuid())` also misses and bare `~`
and `~/...` stop expanding for the person at the keyboard. The failure
is silent by specification: `sem:filecomplete.fn-tilde-expand-fn` step 4
hands the original text back unchanged, so the caller cannot tell *no
such user* from *no tilde present*.

Reaching NSS without libc is not on the table. NSS backends are
`dlopen`ed C shared objects with a C ABI; there is no pure-Rust route to
them. Speaking SSSD's socket or systemd's `io.systemd.UserDatabase`
varlink protocol directly would cover two backends, miss the rest, and
amount to writing a name-service client to avoid a function call.

`getpwnam_r` also buys back two things the hand parser cannot express.
The rule requires the POSIX call shape with a fixed 1024-byte scratch
buffer, *treating any non-zero return as no such user* — which
deliberately conflates `ERANGE` with absence, so an over-long entry must
expand to nothing. A hand parser has no buffer limit and expands it
successfully. And `readline.username-completion-function-fn` needs
`setpwent`/`getpwent`/`endpwent`, an enumeration API that has no
`/etc/passwd` equivalent for directory users at all.

The accepted cost is the one the rule already warns about: the lookup
can block on a network name service, inside a completion keystroke. The
C has always had that property.

Because `getpwnam_r` is a libc symbol, it sits where the previous
section put it: declared in `nshedit-plat`, called by the core, for
every consumer. `fn_tilde_expand` is a free function with no `EditLine`
to hang a per-instance lookup on, which would be awkward if the lookup
needed per-instance configuration — it does not, because the C's
`getpwnam_r` is process-global too, so the shape is the C's rather than
a compromise. The two `/etc/passwd` parsers already written are retired
rather than demoted to a fallback: nothing has to answer when no
override is installed, and a parse sitting behind `getpwnam_r` would
disagree with the C in exactly the case the rule pins, where any
non-zero return — `ERANGE` included — must read as *no such user*.

### The divergence register

This is what the stubs cost today, enumerated against the rules that
specify the behaviour. It is written to be executable: each group names
the primitive, the rules it defeats, and what a user sees.

**1. `tcgetattr` fails, so there is no line editor at all.** This is the
whole ballgame and it is worth stating before the detail.
`sem:tty.tty-setup-fn` step 5 is the sole capture of the original
termios and returns -1; `sem:tty.tty-init-fn` returns that verbatim;
`sem:el.el-init-internal-fn` step 6.4 sets `NO_TTY`; and
`sem:read.el-wgets-fn` step 3 then routes every read to
`noedit_wgets`. So `el_gets` reads a line with no prompt, no editing,
no history keys, no key bindings, no completion and no macro queue, and
`sem:read.read-getcmd-fn` — the whole dispatch trie — is unreachable.
Construction still reports success, so the caller sees a working
`EditLine *` that silently never edits. `readline()` re-runs `tty_init`
on every call (`sem:readline.readline-fn` step 4), so it re-fails every
line.

**2. `tcsetattr` fails, so no terminal mode is ever applied.**
`sem:tty.tty-setty-fn` is the wrapper; `sem:tty.tty-rawmode-fn` dies at
its step 3, taking with it `t_eight`, the speed propagation, the
ICANON believe-what-we-see block and the non-forced
`tty_bind_char`. `sem:tty.tty-cookedmode-fn`,
`sem:tty.tty-quotemode-fn` and `sem:tty.tty-noquotemode-fn` cannot push
their settings, so quoted-insert (`^V`) cannot pass a byte through
untouched. `sem:tty.tty-stty-fn` works through step 7 and fails at step
8, so `setty` edits the tables but never reaches the hardware, and
returns -1 when the edited mode is the current one.
`sem:tty.tty-end-fn` is the one rule that degrades correctly: with
`t_initialized` at 0 it declines to restore, which is right, because
nothing was changed. `sem:el.el-wset-fn`'s `EL_PREP_TERM` discards the
result, so `sem:readline.rl-prep-terminal-fn`,
`sem:readline.rl-deprep-terminal-fn` and
`sem:readline.rl-reset-after-signal-fn` are silent no-ops, and
`sem:el.el-editmode-fn` reports success while `el_flags` and the
terminal disagree.

**3. `tcgetattr` fails, so `t_speed` is 0 and no padding is ever
emitted.** `sem:terminal.tputs-fn` takes the baud rate from
`el_tty.t_speed`, which only `sem:tty.tty-setup-fn` step 7 and
`sem:tty.tty-rawmode-fn` step 4 ever write. Zero speed means *emit no
padding*, which the rule sanctions — so the port is conformant and
permanently unable to reach the non-zero path.
`sem:terminal.terminal-tputs-fn` loses the honouring of embedded
padding, and every consumer with it: `terminal_beep`,
`terminal_clear_eol`, `terminal_clear_screen`, `terminal_deletechars`,
`terminal_insertwrite` (including the `ip` capability),
`terminal_move_to_char`, `terminal_move_to_line`.
`sem:terminal.terminal-echotc-fn` step 5 prints `baud` as a literal 0.
On a slow or emulated serial line this is visible corruption, not a
missing optimisation.

**4. `tcgetattr` fails, so `t_tabs` is 0 and `TERM_CAN_TAB` can never be
set.** `sem:terminal.terminal-setflags-fn` sets the bit only when the
tty layer reports hardware tab expansion; it never does.
`sem:terminal.terminal-telltc-fn` therefore always prints *It cannot
use tabs*, and `sem:tty.tty-rawmode-fn`'s `EL_CAN_TAB` branch is dead.
This is also the first of [[conformance-policy]]'s six behavioural
forks, so the stub has silently pre-decided a question the errata says
is open.

**5. `tcgetattr` fails, so `tty_bind_char` never runs.**
`sem:tty.tty-bind-char-fn` has two callers, `tty_setup` step 12 and
`tty_rawmode` step 6d, and both abort first. The user's own erase,
kill, eof, werase, reprint and lnext characters are never bound into
the key map — so a user with `stty erase ^H` gets libedit's compiled-in
default instead. The platform quirk the rule insists on preserving
(`C_EOF` disabled by default on Linux, so byte 0x00 binds to
`EM_DELETE_OR_LIST`) is untestable because nothing calls the function.
`sem:tty.tty-getspeed-fn` and
`sem:tty.tty-get-signal-character-fn` read the same never-populated
structs, so the latter always answers -1 and
`sem:readline.rl-echo-signal-char-fn` is a no-op — indistinguishable
from the C's own ERR-terminal-36 defect, which quietly pre-empts a
decision the rule says must be recorded.

**6. `ioctl(TIOCGWINSZ)` fails, so the window size is never queried.**
`sem:terminal.terminal-get-size-fn` degenerates to *return the terminfo
`lines`/`cols` and always report unchanged*. Because it always reports
unchanged, `sem:el.el-resize-fn` never calls `terminal_change_size` and
is a complete no-op — and `sem:read.read-prepare-fn` step 4 calls it on
every single line precisely because, in the C's words, things go
terribly wrong if we have the wrong size. `sem:readline.rl-resize-terminal-fn`
is a no-op for the same reason, and `sem:readline.rl-get-screen-size-fn`
hands back the fiction. `sem:terminal.terminal-set-fn` step 10 seeds the
geometry at construction from the same dead call.
`sem:terminal.terminal-settc-fn` and `sem:readline.rl-set-screen-size-fn`
still work, so a user-supplied size is the only correct size available.
What this looks like: a terminal that is not exactly the terminfo
default wraps and overwrites, and resizing the window changes nothing.

**7. `sigprocmask` fails, so nothing is ever blocked.**
`sem:terminal.terminal-set-fn` steps 1 and 11 and `sem:el.el-resize-fn`
steps 1 and 3 lose their critical sections. Today that is harmless
because nothing can deliver a `SIGWINCH` to libedit anyway; it stops
being harmless the moment group 8 lands. Note `sem:terminal.terminal-set-fn`
also carries a defect the port is told to fix — an early return that
leaves `SIGWINCH` blocked for the rest of the process — and the fix has
to be built at the same time as the block.

**8. `sigaction`, `sigprocmask` and `raise` fail, so `EL_SIGNAL` does
nothing.** `sem:sig.sig-set-fn` installs nothing and saves nothing;
`sem:sig.sig-clr-fn` restores nothing; `sem:sig.sig-handler-fn` never
runs, and all three clauses of its observable contract fail — the tty
is not restored before the disposition takes effect, the signal number
never reaches the read loop, and the previous handler is not chained.
`sem:read.read-char-fn` step 2b's `SIGCONT`/`SIGWINCH` poll is dead
because `sig_no` is never written. `sem:read.read-prepare-fn` step 1 and
`sem:read.read-finish-fn` step 2 are no-ops.
`sem:readline.el-rl-tstp-fn` needs `raise(SIGTSTP)`, so **`^Z` does
nothing at all** — the function returns `CC_NORM` and editing continues
on the same line, which is exactly what it would do if the process had
suspended and resumed, so the caller cannot tell.
`sem:readline.rl-cleanup-after-signal-fn` remains an empty stub whose
stated rationale — that libedit installs and clears its own handlers
around `el_gets` — is now false. `sem:sig.sig-init-fn` and
`sem:sig.sig-end-fn` survive intact.

**9. `fcntl` fails, so `EL_SAFEREAD` is half dead.**
`sem:read.read-fixio-fn`'s would-block arm returns -1 unconditionally;
its `EINTR` arm is pure and still works. So `EL_SAFEREAD` on and off are
behaviourally identical for a non-blocking descriptor. The rule's
headline side effect — that recovery *permanently* clears `O_NONBLOCK`
on the caller's input descriptor, saving and restoring nothing — cannot
be reproduced, and the rule says a port must either reproduce it or
treat it as a documented divergence. This one is documented here.
`sem:read.el-wgetc-fn` step 3 reports a terminal-setup failure as end of
file, which is how group 2 reaches the caller.

**10. `getpwnam_r`/`getpwuid_r`/`getuid` are `/etc/passwd`, so
directory users do not exist.** `sem:filecomplete.fn-tilde-expand-fn`
step 3 is the site. Three distinct losses: NSS-only users read as
absent, so `~alice` comes back unexpanded and the caller cannot tell
why; the `ERANGE` conflation the rule mandates is unreproducible,
because a hand parser has no 1024-byte limit and succeeds where
`getpwnam_r` must fail; and the current user is found through
`/proc/self/status` rather than `getuid`, so on a host without `/proc`
even bare `~` fails. Downstream:
`sem:filecomplete.fn-filename-completion-function-fn` step 3 gets the
literal `~user/` back, fails to `opendir` it, and — because the static
stream stays NULL — fails identically on every later call, so tab
completion under another user's home silently yields nothing.
`sem:filecomplete.append-char-function-fn` appends a space instead of a
slash. `sem:readline.tilde-expand-fn` forwards the lot.
`sem:readline.default-history-file-fn` needs `getpwuid(getuid())` and
ignores `$HOME` by design, so `read_history(NULL)`,
`write_history(NULL)`, `append_history` and `history_truncate_file` all
return an errno for a directory user.
`sem:readline.username-completion-function-fn` needs
`setpwent`/`getpwent`/`endpwent` outright.

**11. `issetugid` is `/proc`, which can be unreadable.**
`sem:el.secure-getenv-fn` names three conditions — real uid differs from
effective, real gid differs from effective, or the loader marked the
process secure (`AT_SECURE`) — and forbids reproducing the C's
degenerate always-deny branch. The port answers all three from
`/proc/self/auxv` and `/proc/self/status`, and fails closed when
neither can be read, which is the forbidden answer arrived at
legitimately. On a host with `hidepid` or no `/proc`, every
`el_source` returns -1 (`sem:el.el-source-fn` step 3 exits immediately
on a NULL `HOME`), `TERM` is never read so every terminal is `dumb`
(`sem:el.el-init-internal-fn` step 6.1), and `EDITOR` is never read.
Moving the uid and gid halves to real syscalls removes two thirds of
the exposure; `AT_SECURE` still needs the auxv read, because rustix
exposes it only through the same unsafe runtime module signals are
barred from.

**12. `fileno` is unreachable, so `el_init` and `EL_SETFP` cannot be
honoured.** This group is `nshedit-abi`'s rather than this layer's, and
closing it would mean a third entry on [[no-c-ffi]]'s enumeration,
argued there. It is recorded here because nothing else records it. `sem:el.el-init-fn` is specified as
`el_init_fd(prog, fin, fout, ferr, fileno(fin), fileno(fout),
fileno(ferr))`. There is no way to derive a descriptor from a
caller-supplied `FILE *` without libc, so the ABI crate assumes
descriptors 0, 1 and 2 — which is wrong for any application that hands
libedit its own streams. `sem:el.el-wset-fn`'s `EL_SETFP` has no
`el_init_fd`-shaped escape hatch at all, so that op is simply
unimplementable. `sem:readline.rl-initialize-fn` steps 4 and 5 need
three `fileno` calls, and its `tcgetattr` `ECHO` test is the one place
in this register that fails *open*: the test never succeeds, so
`editmode` is never cleared and libedit edits on a non-echoing input
where the C would not. `sem:history.history-save-fp-fn` needs `ftell`
and `fputs` through a caller's stream, including the observable
distinction between `ftell` returning 0 and returning -1 on a pipe;
`H_SAVE_FP` and `H_NSAVE_FP` put a raw `FILE *` on the public varargs
ABI, so there is nothing to work around.

`sem:el.el-end-fn` step 4 and `sem:el.el-reset-fn` step 1 close the
list: with `NO_TTY` always set, the original terminal modes are never
restored — which is currently harmless only because they were never
changed.

### What platform-build does with this

Build `nshedit-plat` over `rustix`, declare the signal and passwd
families in it, retire the six core stubs against it, and delete the two
duplicated `/etc/passwd` parsers and the two duplicated `getuid` readers
rather than hoisting them. Groups 1 through 10 close outright — 8 and 10
included, with no hook to build first, no built-in default to preserve
and nothing for a consumer to install. Group 11 loses its uid and gid
exposure and keeps the auxv read. Group 12 is `nshedit-abi`'s alone.

The two override slots are a static and a null check each and sit off
the critical path; nothing in the register depends on either.

Two orderings are load-bearing. `tcgetattr`/`tcsetattr` and the signal
calls land together, because raw mode without a `SIGTSTP` handler is
worse than neither: today `^Z` suspends a process whose terminal was
never made raw, and a half-built platform layer would suspend it behind
a raw terminal instead. And `sigprocmask` lands with
`sem:terminal.terminal-set-fn`'s mask-restore fix, because that defect —
an early return that leaves `SIGWINCH` blocked for the rest of the
process — only bites once the mask is real.

Then re-verify: every rule listed above is a rule whose `sem` behaviour
changes when the stub goes, so annotations that were verified against
the stub are stale, not merely untested.
