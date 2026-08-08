---
id [dec:libedit:terminal-caps-via-term-crate]
epitome "The in-workspace nshterm crate owns terminfo parsing, padding, secure discovery, and termcap-name compatibility without linking a C provider."
state @decided
category @existence
scope {
    elements ([arch:libedit:terminal-caps])
    rules (
        [spec:libedit:def:terminal.tgetent-fn]
        [spec:libedit:def:terminal.tgetstr-fn]
        [spec:libedit:def:terminal.tgetflag-fn]
        [spec:libedit:def:terminal.tgetnum-fn]
        [spec:libedit:def:terminal.tgoto-fn]
        [spec:libedit:def:terminal.tputs-fn]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Link libtinfo or ncurses as the C implementation does."
        rejected_because "This violates [dec:libedit:no-c-ffi] and reintroduces provider global state and callback constraints."
    }
    {
        option "Continue depending on the unmaintained term crate."
        rejected_because "The needed parser and expander are load-bearing, while its unrelated colour half and unmaintained release cadence are not acceptable dependencies."
    }
    {
        option "Use terminfo capnames internally and drop legacy termcap names."
        rejected_because "The public compatibility operations accept the legacy two-letter namespace, including live obsolete capabilities such as OTpt. Dropping it changes defined C behaviour."
    }
)
consequences {
    accepted (
        "nshterm is an in-workspace pure-Rust crate containing the terminfo database parser, searcher, parameter expansion, and compatibility name data."
        "The terminal layer uses typed terminfo capabilities internally while the compatibility boundary resolves the termcap names accepted by libedit."
        "Padding markers survive parameter expansion and are emitted by the Rust tputs implementation according to output speed and affected lines; no global putc destination is required."
        "TERMINFO, TERMINFO_DIRS, and HOME-derived search paths are ignored for a privileged process according to the secure environment guard."
        "Filesystem terminfo trees are supported. Hashed terminfo database support is decided and implemented by nshterm-hashed-db before final compatibility acceptance."
    )
    deferred (
        "No terminal-capability work other than the hashed-database decision remains deferred."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
    related_to ([dec:libedit:platform-layer] [dec:libedit:conformance-policy])
}
codifies (
    [spec:libedit:def:terminal.tgetent-fn]
    [spec:libedit:def:terminal.tputs-fn]
)
establishes ([arch:libedit:terminal-caps])
---

## Rationale

libedit reaches its terminal database through six termcap-shaped operations,
but linking their C provider is unnecessary. The parsing and parameter
expansion machinery now lives in `nshterm`, which the workspace owns and can
shape around the editor's actual contract.

Internal capability identity and compatibility input spelling are separate
concerns. Typed terminfo names are appropriate inside the renderer; the ABI
must still accept the two-letter termcap names deployed callers pass. Padding
is similarly preserved as structured information until `tputs` knows the
writer speed, avoiding the C provider's global callback destination.

Secure environment discovery and the compatibility name table are settled.
The remaining database-format question is whether supported systems require
ncurses' hashed database in addition to filesystem trees; that question is a
real dependency of final ABI acceptance and no longer a vague terminal-layer
deferral.
