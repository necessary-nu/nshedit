---
id [dec:libedit:native-history]
epitome "Native history owns typed logical records while each traversal owns an independent stable-ID cursor."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.history+1]
        [spec:nshedit:req:core.effect-hooks]
        [spec:nshedit:req:abi.behavioural-conformance]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Promote NativeHistory's byte records and one internal cursor as the final native API."
        rejected_because "Byte text requires a later locale decode, and one cursor makes concurrent editor views interfere. Both are artifacts of its bridge to the translated EditorHistory trait."
    }
    {
        option "Attach a History trait object directly to Editor and invoke it while editing."
        rejected_because "An ABI implementation may call foreign history code and re-enter the editor. History remains a host effect so the editor borrow ends before traversal runs."
    }
    {
        option "Retain i32 event numbers with zero or minus one as special values."
        rejected_because "Positions, identities, and absence are different concepts. A private HistoryId constructor plus Option expresses them without collision or wraparound."
    }
    {
        option "Make the store's persistence format part of its record representation."
        rejected_because "Encoding Text scalars, preserved raw bytes, non-scalar wide values, and application metadata is host policy. Coupling storage to a byte container would recreate narrow/wide conversion state in the core."
    }
)
consequences {
    accepted (
        "HistoryStore<M> owns HistoryEntry<M> values containing a stable HistoryId, logical Text, and caller-chosen typed metadata. No borrowed event record, opaque cookie, callback, or conversion buffer exists in the store."
        "HistoryId allocation never wraps or reuses an identity after clear. Exhaustion is a typed error that returns ownership of the supplied Text and metadata."
        "HistoryCursor is an independent value holding an optional stable identity. Several editors or views may traverse one immutable store without sharing mutable position state."
        "Previous means older input and Next means newer input. Navigation distinguishes selecting an entry, returning to the saved live line, and hitting a boundary. Removed or evicted cursor identities repair to the live position."
        "Capacity is Option<NonZeroUsize>, eliminating zero-as-magic ambiguity. Shrinking and bounded insertion return evicted owned entries; consecutive-duplicate rejection returns the caller's Text and metadata."
        "HistoryNavigateEffect carries a typed repeat count and answers atomically with HistoryResponse::Entry, Live, or Boundary, so the read driver neither exposes intermediate navigation nor infers live-line restoration from an ambiguous None."
        "Persistence and locale conversion are integrations over owned records, not HistoryStore fields. The existing byte history container remains available to compatibility code until a Text-aware native codec is separately decided."
        "The former NativeHistory byte/global-cursor facade is replaced rather than aliased. The translated HistoryGen, EditorHistory, varargs, event, and narrow/wide paths remain legacy-only until the ABI adapter moves them and core.no-compat-internals deletes them."
    )
    deferred (
        "A versioned native persistence codec for every TextUnit representation and application metadata schema."
        "Whether a future concurrent store offers interior synchronization; the current type is Send and Sync whenever its metadata is."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:text-and-screen-model] [dec:libedit:effect-driven-hooks] [dec:libedit:native-line-state])
    related_to ([dec:libedit:conformance-policy] [dec:libedit:opaque-abi-adapter] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:core.history+1]
    [spec:nshedit:req:core.effect-hooks]
)
---

## Rationale

A history store has two independent jobs: retain owned records and let a
consumer describe where it is while browsing them. The translated store and
the earlier Rust bridge combined those jobs by keeping one mutable positional
cursor beside byte records and C-style event numbers. That shape is safe, but
it is not the semantic model the new editor needs.

The native store therefore owns only records and policy. Traversal state is a
small external cursor keyed by stable identity, so inserting at the front,
evicting at the back, or running a second traversal cannot silently retarget
it. Typed navigation also exposes the state transition a line editor actually
needs: moving newer from the newest history entry restores the saved live
line, while a real boundary changes nothing.

History remains outside Editor ownership because it is a host-controlled
effect. Native callers may service that effect from HistoryStore; the ABI may
service it through a foreign callback after releasing the editor borrow. The
two integrations share semantic responses without sharing callback or C
representations.
