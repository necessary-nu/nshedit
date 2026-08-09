---
id [dec:libedit:conformance-policy]
epitome "The detailed compatibility corpus and maintained Rust code jointly define C ABI behaviour."
state @decided
category @executive
scope {
    elements ([arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:abi.complete-surface]
        [spec:nshedit:req:abi.surface-stability]
        [spec:nshedit:req:abi.behavioural-conformance]
        [spec:nshedit:req:abi.terminal-controls]
        [spec:nshedit:req:abi.bindings]
        [spec:nshedit:req:abi.binding-dispatch]
        [spec:nshedit:req:abi.history-effects]
        [spec:nshedit:req:abi.signal-lifecycle]
        [spec:nshedit:req:abi.observational-coverage]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep the imported C implementation as an executable oracle."
        rejected_because "A second implementation is stale maintenance weight after the Rust port is complete. Compatibility evidence must exercise what ships."
    }
    {
        option "Fix any historical reference defect encountered in the ABI."
        rejected_because "A C caller did not opt into new semantics. Consumer-visible corrections require an explicit decision and versioned rule."
    }
    {
        option "Reproduce undefined behaviour as well as defined behaviour."
        rejected_because "Memory unsafety is not a compatibility property. Undefined inputs receive an explicit safe definition instead."
    }
)
consequences {
    accepted (
        "The detailed libedit corpus and the maintained Rust implementation are the source of truth together. A mismatch is resolved by reviewing both, not by executing a retired implementation."
        "The committed generated headers and export manifest freeze the C-facing shape. Direct C consumers verify that those artifacts compile, link, install, and run."
        "A compatibility probe counts as evidence only when it observes the effect the operation promises. A matching success code cannot prove a state mutation, emitted sequence, callback, or handler transition."
        "State-changing probes include a dependent observation after the mutation, so an unconditional stand-in cannot satisfy the contract by returning the expected status."
        "Generated execution claims are replaced from current instrumentation rather than accumulated across deleted implementations; a lower measured count is preferable to stale proof."
        "Defined historical behaviour is preserved unless a separate decided record and a versioned rule change authorize a C-visible divergence."
        "Undefined C constructs receive deterministic safe behaviour recorded in the corresponding rule; unsafe emulation is forbidden."
        "The imported C and Autotools trees are retired. Git history retains their provenance; they are not build, test, or distribution inputs."
        "The core is not representation- or API-compatible with C. It may expose cleaner native semantics so long as the ABI adapter reconstructs the required observations."
    )
    deferred (
        "Release and soname policy for a future explicitly approved C-visible divergence."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
    related_to ([dec:libedit:idiomatic-core] [dec:libedit:opaque-abi-adapter] [dec:libedit:signal-lifecycle])
}
codifies (
    [spec:nshedit:req:abi.complete-surface]
    [spec:nshedit:req:abi.surface-stability]
    [spec:nshedit:req:abi.behavioural-conformance]
    [spec:nshedit:req:abi.terminal-controls]
    [spec:nshedit:req:abi.bindings]
    [spec:nshedit:req:abi.binding-dispatch]
    [spec:nshedit:req:abi.history-effects]
    [spec:nshedit:req:abi.signal-lifecycle]
    [spec:nshedit:req:abi.observational-coverage]
)
---

## Rationale

Drop-in compatibility is a boundary property. A C consumer observes headers
and symbols, but also bytes written to a terminal, callback order, pointer
lifetime, stream ownership, errno, and state transitions. The detailed corpus
records those obligations; the Rust implementation and its maintained tests
show how the shipped library satisfies them.

An executable copy of the imported implementation no longer improves that
contract. It duplicates thousands of lines the product does not build,
requires a separate Autotools pipeline, and turns an abandoned implementation
into a permanent release input. The useful evidence is now owned directly:
generated headers, a committed export set, Rust behaviour tests, focused C
consumers, and native platform acceptance.

Return-code-only checks remain too weak for editrc commands and callbacks. A
stub can return success for a mutation while doing nothing, so maintained
tests must observe a later effect. Undefined inputs are different: there is no
sound historical result to preserve, and the ABI must define one safely.
