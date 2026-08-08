---
id [dec:libedit:opaque-abi-adapter]
epitome "Opaque C handles point to ABI-owned adapters; the core representation is never a C allocation or header source."
state @decided
category @existence
scope {
    elements ([arch:libedit:c-abi] [arch:libedit:core])
    rules (
        [spec:nshedit:req:abi.opaque-owner]
        [spec:nshedit:req:abi.surface-stability]
        [spec:nshedit:req:abi.behavioural-conformance]
        [spec:nshedit:req:core.public-surface]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Continue returning a boxed core Editor as the C handle."
        rejected_because "The ABI would keep requiring public fields, C callback storage, and compatibility buffers in the core, defeating representation independence."
    }
    {
        option "Expose a C-shaped Rust compatibility facade from nshedit and have the ABI call it."
        rejected_because "That preserves two public APIs and lets C lifetimes leak back into the native library. The C facade has one legitimate owner: nshedit-abi."
    }
    {
        option "Make the completed C records core types so cbindgen can continue reading both crates."
        rejected_because "LineInfo, HistEvent, readline records, and exported globals are ABI representation, not editor-domain state. Sharing them makes header layout a core concern."
    }
)
consequences {
    accepted (
        "Each incomplete C handle is backed by an ABI allocation containing a native object and its C-only state. The pointer spelling and ownership contract remain unchanged for callers."
        "During the bounded cutover, the ABI allocation may also contain a private translated compatibility payload for behaviour not yet routed through the native object. That payload is removed concern by concern and is never exposed or cast as the native object."
        "FILE objects, descriptors supplied by C, client data, callback pointers, cookies, narrow/wide conversion buffers, live line views, tokenizer arrays, term names, word-character strings, errno translation, and exported mutable globals are owned by nshedit-abi."
        "Completed C structs and header-generation inputs are declared by nshedit-abi. cbindgen does not derive public C layout from core source."
        "Rust identifiers remain idiomatic; export_name and cbindgen rename metadata preserve required C symbols and spellings."
        "The completed adapter calls only safe public semantic operations, and a compile boundary prevents access to core modules or fields. Until that firewall lands, existing exports may reach only the contained compatibility payload for behaviour not yet switched; new behaviour does not target it."
        "Pointer validity and callback reentrancy are explicit adapter state-machine obligations and are covered by conformance tests. While translated callbacks remain, the compatibility payload is an offset-zero prefix so the callback pointer has exactly the opaque handle's address; this invariant is deleted with the payload."
    )
    deferred ()
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:effect-driven-hooks] [dec:libedit:no-c-ffi] [dec:libedit:conformance-policy])
}
codifies (
    [spec:nshedit:req:abi.opaque-owner]
    [spec:nshedit:req:abi.surface-stability]
    [spec:nshedit:req:abi.behavioural-conformance]
    [spec:nshedit:req:core.public-surface]
)
---

## Rationale

The public C headers declare the major editor, history, and tokenizer handles
as incomplete types. Their pointer values are stable ABI, but their allocation
layout is not. That is the seam that allows a true Rust-native core without a
C-visible break.

The ABI adapter becomes an anti-corruption layer rather than a symbol-forwarding
crate. It owns every fact that exists because a C caller needs a pointer,
callback, global, varargs operation, or temporary conversion. The native editor
then remains free to change its private representation, and cbindgen reads only
the crate that actually owns C layout.

The migration is deliberately asymmetric. An opaque owner can carry the native
object alongside the translated payload while an exported concern still uses
the latter, but it does not synchronize two engines or make both authoritative.
Translated callbacks currently receive a pointer to that payload, so placing it
at offset zero preserves the enclosing opaque handle's address for re-entry.
Once callbacks and dispatch for a concern use the safe semantic interface, its
compatibility fields and this prefix constraint are removed rather than kept as
a permanent bridge.
