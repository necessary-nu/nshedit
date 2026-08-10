---
id [dec:libedit:platform-layer]
epitome "nshedit-plat is the safe system boundary: rustix where available, narrowly enumerated libc for signals and NSS."
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
        [spec:nshedit:req:core.rust-io]
        [spec:nshedit:req:core.unsafe-free]
        [spec:nshedit:req:platform.typed-boundary]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep private syscall stubs beside each caller."
        rejected_because "Duplicated termios, signal, uid, passwd, and ioctl seams drift independently and hide which behaviours are unavailable."
    }
    {
        option "Expose the syscall surface as public modules in nshedit."
        rejected_because "System ABI details are not editor-domain API and would prevent the core from forbidding unsafe code."
    }
    {
        option "Put platform access in nshedit-abi."
        rejected_because "Native consumers require terminal control, signals, and user lookup without depending on the C adapter."
    }
    {
        option "Route all facilities through libc or nix."
        rejected_because "rustix provides a sound pure-Rust route for the majority. Broad libc use would make [dec:libedit:no-c-ffi]'s exception unbounded."
    }
    {
        option "Keep process-global override hooks for signals and passwd lookup."
        rejected_because "Nothing installs them, they introduce mutable global state, and host customization belongs to the editor's typed effect boundary rather than the syscall crate."
    }
)
consequences {
    accepted (
        "nshedit-plat is the only crate that owns syscall and platform-ABI implementation details. nshedit and nshedit-abi consume its safe interfaces."
        "rustix 1.1.x supplies termios, window-size ioctl, fcntl, uid/gid queries, and FIONREAD. The event-hook poll and any supported typeahead check use its safe ioctl_fionread interface."
        "Signals use the platform libc because raw signal syscalls are unsound in a process containing libc. NSS passwd lookup and enumeration use libc because the configured name-service backends are C modules."
        "Platform ABI declarations, transcribed structs, constants, and syscall conversions are private to nshedit-plat and covered by focused platform tests. A C-provided integer descriptor is scoped into BorrowedFd only by nshedit-abi's handle adapter; nshedit-plat never accepts the raw integer."
        "Public safe operations take BorrowedFd or typed platform values and return io::Result or a descriptive typed error. They never fabricate a 'static descriptor lifetime or flatten syscall failure into bool or Option."
        "The core receives safe typed results and never exposes termios, sigaction, passwd, ioctl, or raw ownership mechanics in its public API."
        "There are no public process-global override hooks. Scoped disposition ownership follows [dec:libedit:signal-lifecycle], while editor-level customization suspends through [dec:libedit:effect-driven-hooks]."
        "The Linux system ABI is exactly x86_64-unknown-linux-gnu. nshedit-plat rejects other Linux triples before they can inherit its x86-64 glibc transcriptions; macOS and Windows use their separately decided platform boundaries."
    )
    deferred (
        "Another Linux triple requires its own verified constants, layouts, libc accessors, build artifacts, and conformance matrix."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi] [dec:libedit:idiomatic-core] [dec:libedit:posix-only-scope] [dec:libedit:conformance-policy])
    related_to ([dec:libedit:terminal-caps-via-term-crate] [dec:libedit:effect-driven-hooks] [dec:libedit:signal-lifecycle])
}
codifies (
    [spec:nshedit:req:core.rust-io]
    [spec:nshedit:req:core.unsafe-free]
    [spec:nshedit:req:platform.typed-boundary]
)
establishes ([arch:libedit:platform])
---

## Rationale

Terminal control, event polling, signals, and user lookup are required by
both the native editor and the C adapter, but none belongs in the editor's
domain API. A dedicated crate gives the workspace one auditable place for
system details and lets `nshedit` forbid unsafe code.

rustix is used whenever it offers the operation, including `FIONREAD`, whose
earlier deferral became obsolete once the dependency exposed a safe wrapper.
Signals are different: libc expects to participate in their runtime and raw
signal syscalls are not sound in a process already using libc. User lookup is
likewise a libc boundary because NSS dynamically loads configured providers.
Those two families remain the enumerated platform exception.

The platform boundary is selected by a proved target ABI, not by an
operating-system family name. On Linux that proof currently exists only for
`x86_64-unknown-linux-gnu`; other Linux triples fail the build instead of
receiving records transcribed from x86-64 glibc.

The platform crate provides typed operations and defaults, not a C-shaped
public syscall facade or process-global injection points. Platform layouts,
constants, and callback tables remain inside the operation that validates and
converts them. The ABI adapter necessarily receives C integer descriptors, but
confines each conversion to the duration of one typed platform call rather
than fabricating or storing a Rust borrow. If an embedder must supply history,
input, aliases, completion, or related host behaviour, that customization
belongs to the editor's effect protocol, where ownership and reentrancy can be
expressed per session.
