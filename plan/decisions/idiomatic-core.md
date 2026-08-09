---
id [dec:libedit:idiomatic-core]
epitome "nshedit is a safe Rust-domain library; every C representation, callback, and lifetime obligation belongs to nshedit-abi."
state @decided
category @executive
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.typed-domain]
        [spec:nshedit:req:core.raii-lifecycle]
        [spec:nshedit:req:core.rust-io]
        [spec:nshedit:req:core.public-surface]
        [spec:nshedit:req:core.no-compat-internals]
        [spec:nshedit:req:core.native-consumer]
        [spec:nshedit:req:core.unsafe-free]
        [spec:nshedit:req:abi.rust-internals]
        [spec:nshedit:req:workspace.semantic-naming]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep the transliterated core and add a small ergonomic facade."
        rejected_because "The public fields, raw streams, callback pointers, integer dispatch, manual teardown, and C-shaped internal invariants would remain the real implementation and keep constraining every safe wrapper."
    }
    {
        option "Keep the existing Rust API as a compatibility facade beside a new API."
        rejected_because "The crate is pre-1.0 and its only known consumer can migrate. Carrying two public models would permanently preserve the representation this programme exists to remove."
    }
    {
        option "Build a second complete editor and switch the ABI after both coexist."
        rejected_because "Two independently authoritative engines would double state and behavioural drift. A bounded ABI owner may carry migration state for both representations, but each exported concern has one authority and deletes its compatibility state when switched."
    }
)
consequences {
    accepted (
        "The pre-1.0 Rust API is replaced once. The C ABI, not the current Rust surface, is the compatibility promise."
        "Editor state is private and reached through typed operations. Configuration, modes, commands, outcomes, and errors use Rust domain types rather than integer operation codes or errno."
        "A native editor owns its lifecycle through RAII. Explicit finish reports restoration errors and Drop restores best-effort exactly once."
        "The public core contains no raw pointers, FILE objects, C scalar aliases, extern callbacks, varargs dispatch, compatibility buffers, exported mutable state, or ABI record layouts."
        "The core ultimately forbids unsafe code. Platform unsafety belongs to nshedit-plat and C unsafety belongs to nshedit-abi."
        "nshedit-abi depends only on the core's public semantic API and cannot access editor fields or internal modules."
        "The transliterated implementation and its compatibility payload are absent; the typed native editor is the only core engine and no legacy backend remains."
        "Opaque ABI owners contain the native object plus ABI-only state, never a second editor representation or synchronized shadow model."
        "The safe crate's maintained identifiers and public formats are named for their responsibility or stable identity, never relative to a removed implementation or to C transport terminology."
        "The ABI crate may expose exact C wrappers, but its private implementation uses the same typed Rust discipline as the core and never re-enters through exported varargs symbols."
    )
    deferred (
        "Whether a future native API offers async integration in addition to the resumable synchronous driver."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi] [dec:libedit:conformance-policy])
    related_to ([dec:libedit:effect-driven-hooks] [dec:libedit:text-and-screen-model] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:core.typed-domain]
    [spec:nshedit:req:core.raii-lifecycle]
    [spec:nshedit:req:core.rust-io]
    [spec:nshedit:req:core.public-surface]
    [spec:nshedit:req:core.no-compat-internals]
    [spec:nshedit:req:core.native-consumer]
    [spec:nshedit:req:core.unsafe-free]
    [spec:nshedit:req:abi.rust-internals]
    [spec:nshedit:req:workspace.semantic-naming]
)
establishes ([arch:libedit:core])
---

## Rationale

The replaced core was a faithful transliteration, not the Rust library this
decision requires. Its file-for-file modules, public state, raw streams,
foreign callbacks, compatibility buffers, and manual `el_end` lifecycle made
C representation choices part of the Rust API. A wrapper could not make those
invariants disappear while the ABI still reached through it to mutate fields.

The opaque C handle gives the project a clean seam. `nshedit-abi` can allocate
an adapter containing a private native editor plus every piece of C-only state,
while callers continue to hold the same incomplete `EditLine *`, `History *`,
and `Tokenizer *` types. Completed records and header generation likewise move
to the ABI crate. The native crate can then be designed for Rust without
changing a C layout that consumers can observe.

Replacement proceeded through an ABI-owned shell that temporarily contained a
native object and translated payload, then through vertical concern
replacements. It was migration scaffolding, never a second supported backend:
the payload was authoritative only for an unswitched concern and was deleted
when that concern moved to the native semantic API. Its final deletion proves
the native model is the implementation rather than decoration.
