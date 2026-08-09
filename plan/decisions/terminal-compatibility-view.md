---
id [dec:libedit:terminal-compatibility-view]
epitome "The C adapter owns a per-handle termcap and tty compatibility view that projects into, but never contaminates, the typed native terminal model."
state @decided
category @property
scope {
    elements ([arch:libedit:c-abi] [arch:libedit:terminal-caps] [arch:libedit:platform])
    rules (
        [spec:nshedit:req:abi.termcap-view]
        [spec:nshedit:req:abi.terminal-session]
        [spec:nshedit:req:abi.tty-modes]
        [spec:nshedit:req:abi.terminal-controls]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Teach the native Editor about termcap names, C strings, FILE pointers, and setty syntax."
        rejected_because "Those are representation and compatibility protocols, not editor-domain concepts; admitting them would contradict the typed public firewall and safe Rust I/O contract."
    }
    {
        option "Reconstruct compatibility answers from the renderer for every query."
        rejected_because "EL_GETTC returns stable borrowed C strings, SETTC mutates values independently, and failed database lookup preserves several historical derived flags; the renderer intentionally models none of those pointer or provider quirks."
    }
    {
        option "Reuse a process-global termcap entry and output callback."
        rejected_because "Multiple editor handles may select and mutate different terminals and streams; global provider state would make those handles interfere and would reintroduce a C callback into native rendering."
    }
)
consequences {
    accepted (
        "Each ABI handle owns terminal name, typed capability maps, stable CString storage, database and live geometry, and the three tty-mode override sets."
        "nshterm owns the pure terminfo-to-termcap projection, including provider compatibility such as the me reset; it owns no selected terminal or output destination."
        "Every successful terminal mutation derives a fresh native TerminalProfile and ScreenSize, so the safe Editor remains the only rendering engine while the compatibility view remains the query oracle."
        "The platform crate exposes safe termios snapshots, speed, geometry, and mutation. The C adapter alone parses legacy setty syntax and retains deferred overrides."
        "Native rendering reached through the ABI receives a safe Write adapter over the caller-owned FILE stream; the raw pointer never enters nshedit."
    )
    deferred (
        "Non-Linux tty flag tables and encoded baud-rate mappings enter only with a verified platform ABI."
    )
}
edges {
    requires ([dec:libedit:opaque-abi-adapter] [dec:libedit:native-terminal-render] [dec:libedit:terminal-caps-via-term-crate] [dec:libedit:platform-layer] [dec:libedit:no-c-ffi])
    related_to ([dec:libedit:conformance-policy] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:abi.termcap-view]
    [spec:nshedit:req:abi.terminal-session]
    [spec:nshedit:req:abi.tty-modes]
    [spec:nshedit:req:abi.terminal-controls]
)
---

## Rationale

The greenfield core and the drop-in C contract need different terminal views.
The core needs owned capabilities, typed speed and geometry, safe writers, and
transactional screen state. The C surface additionally promises two-letter
termcap names, stable borrowed strings, mutable `settc` values, historical
`setty` parsing, and caller-owned stdio buffering.

One per-handle adapter view is the narrow seam between them. It translates
provider data and legacy commands into typed native configuration, while
queries and diagnostics read the same retained state. This preserves the
observable ABI without making the native Editor C-shaped or installing global
terminal state.
