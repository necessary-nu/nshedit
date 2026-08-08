---
id [dec:libedit:conformance-policy]
epitome "The C ABI remains reference-compatible for defined inputs; native Rust semantics may differ behind that boundary."
state @decided
category @executive
scope {
    elements ([arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:abi.complete-surface]
        [spec:nshedit:req:abi.surface-stability]
        [spec:nshedit:req:abi.behavioural-conformance]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Fix any reference defect encountered while replacing the core."
        rejected_because "A C caller did not opt into new semantics. Combining a rewrite with observable corrections destroys the oracle needed to distinguish architectural regressions from deliberate changes."
    }
    {
        option "Reproduce undefined behaviour as well as defined behaviour."
        rejected_because "Memory unsafety is not a compatibility property. Undefined inputs receive an explicit safe definition instead."
    }
    {
        option "Freeze the current Rust port, including known compatibility gaps, as the oracle."
        rejected_because "The deliverable is a drop-in libedit/readline implementation. Existing port-only stand-ins and missing operations are defects to close before their internals become a migration baseline."
    }
)
consequences {
    accepted (
        "The detailed libedit corpus remains the behavioural authority for the ABI: return values, errno, emitted bytes, stream effects, callback ordering, pointer validity, and state transitions are observable."
        "The compatibility oracle is strengthened before structural replacement. A missing implementation, unconditional error, or documented stand-in is fixed before it can be treated as baseline behaviour whenever the reference performs real work; reference-defined unsupported and no-op behaviour remains compatible."
        "Defined defects in the reference are preserved unless a separate decided record and a versioned rule change authorize a C-visible divergence. Idiomatization is not automatic permission to change them."
        "Existing intentional divergences are re-proven by the oracle and remain only where a rule explicitly defines them."
        "Undefined C constructs receive deterministic safe behaviour recorded in the corresponding rule; unsafe emulation is forbidden."
        "The core is not representation- or API-compatible with C. It may expose cleaner native semantics so long as the ABI adapter reconstructs the required observations."
    )
    deferred (
        "Release and soname policy for a future explicitly approved C-visible divergence."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
    related_to ([dec:libedit:idiomatic-core] [dec:libedit:opaque-abi-adapter])
}
codifies (
    [spec:nshedit:req:abi.complete-surface]
    [spec:nshedit:req:abi.surface-stability]
    [spec:nshedit:req:abi.behavioural-conformance]
)
---

## Rationale

Drop-in compatibility is a boundary property. A C consumer observes the
headers and symbols, but also the bytes written to a terminal, the order in
which its callbacks run, how long a returned pointer remains valid, which
stream owns buffered output, and what errno contains on failure. Those
observations remain tied to the reference implementation and the detailed
rules extracted from it.

The greenfield core changes the mechanism, not that contract. Port-only
compatibility gaps already documented by the ABI are therefore closed and
captured by the oracle before the core representation is replaced. Otherwise
the rewrite would faithfully preserve omissions that were never part of
libedit. Conversely, an intentional no-op in the reference is an ABI
observation to preserve, not an implementation gap to invent away.

The old reproduce-then-fix policy served a staged port but is unsafe as a
standing policy for a shipped compatibility library. A defect fix can still
be worthwhile; it simply needs its own decision, rule change, and tests so a
consumer-visible change is intentional and reviewable. Undefined behaviour
is the exception because there is no sound observation to preserve.
