---
id [dec:libedit:idiomatic-core]
epitome "The core is idiomatic modern Rust; every C-shaped compatibility artifact lives in the ABI crate."
state @decided
category @executive
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep the literal translation as the shipped shape and expose it directly."
        rejected_because "nsh links the core, not the ABI. Handing its primary consumer a transliterated C API — varargs dispatch, pointers valid until the next call, mutable globals — would make the port's main deliverable worse than the thing it replaces."
    }
    {
        option "One crate that is idiomatic internally and grows the C exports on the side."
        rejected_because "The compatibility state is not decoration. Scratch buffers, the global instance pointer and the legacy conversion buffers have lifetimes the core must not inherit, and a shared crate is how they leak back in."
    }
)
consequences {
    accepted (
        "Conformance is a property of the ABI boundary, not of the core. The core is free to differ internally as long as the exported behaviour matches."
        "The ABI crate owns the compatibility state outright: the shared conversion buffers, the varargs dispatch, the exported mutable statics, the live line view, and the pointer-lifetime contracts."
        "Translation lands C-compat layers in the ABI crate directly — readline.c, eln.c's narrow wrappers, and el_set/el_get's varargs dispatch are already compatibility layers in the C, so putting them there is the literal placement, not an early idiomatization."
        "The core may use owned and borrowed Rust types, real error types, and no globals. It is not bound by the C's representation choices."
    )
    deferred (
        "The core's public API shape is designed during idiomatization, informed by what nsh needs, not derived mechanically from the C."
        "Whether the core exposes an async or non-blocking read path at all."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi] [dec:libedit:conformance-policy])
}
establishes ([arch:libedit:core])
---

## Rationale

Two crates, two different jobs, and only one of them answers to C.

`nshedit-abi` exists to be indistinguishable from libedit to a C caller.
Everything the C exposes that Rust would never choose — the `el_set`
varargs dispatch, the conversion buffers whose returned pointers stay
valid exactly until the next call, `rl_point` and its fellow exported
mutable statics, the signal handler finding its instance through a
file-static, the live view cast onto the internal line struct — lives
there, and is frozen there.

`nshedit` is the library. nsh links it directly, so its API is a
deliverable in its own right rather than a byproduct of the translation.
It gets owned and borrowed types with real lifetimes, errors that are
errors, no global state, and representations chosen for Rust rather than
inherited from a 1992 codebase.

The consequence worth stating plainly: **[[conformance-policy]] binds at
the ABI boundary, not inside the core.** Reproducing a defect means a C
caller observes what it observed before. It does not mean the core
carries the defect internally. Where a defect is purely an artifact of
the C representation — a pointer invalidated by the next call, a
sentinel packed into a spare bit, a buffer shared between two unrelated
call sites — the core does not have the representation, so there is
nothing there to reproduce, and the ABI crate's shim is where the
observable contract is honoured.

The split also decides where translated code goes. `readline.c`,
`eln.c`'s narrow wrappers and the varargs dispatch inside `el.c` are
compatibility layers *in the C itself*. Translating them into the ABI
crate is the faithful placement; routing them through the core first
would only mean evicting them later, and risks something coming to
depend on them in the meantime. Everything genuinely internal — the line
buffer, refresh, terminal, history, the command sets — belongs to the
core.
