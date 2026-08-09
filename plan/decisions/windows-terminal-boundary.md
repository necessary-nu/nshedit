---
id [dec:libedit:windows-terminal-boundary]
epitome "Windows native sessions decode console records at the platform boundary and reuse the existing VT renderer; ConPTY is a host and test transport, never an editor backend."
state @decided
category @property
scope {
    elements ([arch:libedit:platform] [arch:libedit:core] [arch:libedit:terminal-caps])
    rules (
        [spec:nshedit:req:workspace.windows-native-build]
        [spec:nshedit:req:platform.windows-console]
        [spec:nshedit:req:platform.windows-input]
        [spec:nshedit:req:core.windows-session]
        [spec:nshedit:req:workspace.windows-acceptance]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Introduce a new monolithic TerminalBackend, InputEvent model, and render-operation language for every platform."
        rejected_because "TerminalControl, SessionIo, ReadEffect and ReadOutcome already separate lifecycle, host I/O, input suspension, and rendering. Replacing those working seams would make Windows support a second editor architecture rather than a platform adapter."
    }
    {
        option "Create a ConPTY inside each editor session and treat it as the Windows terminal API."
        rejected_because "ConPTY is created by a terminal host to run a child application. nshedit is the application inside that relationship; creating another pseudoconsole would invert ownership and fail for ordinary console and redirected handles."
    }
    {
        option "Render with legacy Win32 screen-buffer mutation calls."
        rejected_because "The native renderer already owns transactional VT output and an incremental physical-screen model. A Win32 renderer would duplicate that state and diverge from Unix and ConPTY output."
    }
    {
        option "Extend the libedit-compatible C ABI to Windows."
        rejected_because "Windows consumers use the native Rust editor through nsh. POSIX FILE, signal, passwd, and libedit library compatibility have no Windows drop-in target."
    }
)
consequences {
    accepted (
        "The supported Windows product is nshedit, nshedit-plat, nshterm, and their native nsh integration. nshedit-abi is not built or emulated on Windows."
        "Windows handle inspection, console-mode flags, UTF-16 decoding, virtual-key interpretation, and unsafe Win32 calls live in nshedit-plat. The core receives only existing owned Rust-domain values."
        "A real console input handle is read as structured key, resize, and control events. A pipe or pseudoterminal input remains a byte stream and enters the driver's existing incremental decoder."
        "Console output enables VT processing, writes the existing TerminalProfile::ansi() sequences through the existing transactional renderer, and restores the exact mode captured before activation. Stream output receives bytes without console-mode calls."
        "Input, display output, and diagnostic handles are classified independently. Redirecting one never changes the classification or API used for another."
        "Key-up records are ignored; repeat counts are preserved; UTF-16 surrogate pairs become Unicode scalars without entering the core as code units; navigation and modifier state are normalized before keymap dispatch; resize and interrupt remain semantic driver events."
        "nshterm remains the terminfo provider for platforms that discover terminfo. Windows editor construction selects the core's existing ANSI profile directly and does not invent a terminfo database entry or a second built-in profile."
        "Windows does not grow passwd enumeration or ~user expansion for the native editor. Terminfo environment discovery is not used by Windows editor construction, and the C compatibility behaviours that require passwd remain outside the Windows target."
        "ConPTY is used by integration tests as the terminal host around a child editor process. Production editor code does not call CreatePseudoConsole."
        "Linux-hosted validation uses cargo-xwin for the MSVC target and the installed GNU Windows target as a secondary compile check. Runtime console acceptance runs on Windows."
    )
    deferred (
        "A legacy non-VT Win32 screen renderer."
        "A Windows C ABI compatibility surface."
        "Named-user home expansion on Windows."
    )
}
edges {
    requires (
        [dec:libedit:platform-targets]
        [dec:libedit:platform-layer]
        [dec:libedit:editor-session-ownership]
        [dec:libedit:native-read-driver]
        [dec:libedit:native-terminal-render]
    )
    related_to ([dec:libedit:effect-driven-hooks] [dec:libedit:text-and-screen-model])
}
codifies (
    [spec:nshedit:req:workspace.windows-native-build]
    [spec:nshedit:req:platform.windows-console]
    [spec:nshedit:req:platform.windows-input]
    [spec:nshedit:req:core.windows-session]
    [spec:nshedit:req:workspace.windows-acceptance]
)
---

## Rationale

The editor already has the abstractions Windows needs. TerminalControl owns
transactional Cooked, Editing, and Quoted transitions; ReadEffect suspends the
driver while the host obtains input; ReadOutcome accepts owned bytes, decoded
units, semantic signals, timeout, and end of input; and TerminalProfile owns
the byte capabilities consumed by the transactional renderer. The Windows
port supplies those values from different operating-system facilities without
changing their ownership model.

Windows has two materially different input paths. A console handle exposes
structured UTF-16 key and resize records, while ConPTY, SSH, and redirected
input expose bytes. The platform adapter distinguishes them at runtime and
normalizes both into the existing read protocol. Output is simpler: modern
console hosts and pseudoterminals consume VT. A console handle needs its VT
mode enabled and later restored; a stream needs only ordinary writes.

This division keeps Win32 representations at the one unsafe system boundary,
keeps terminal emulation out of a line-editing library, and lets the same
renderer and keymaps remain authoritative on every supported host.
