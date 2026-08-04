---
id [dec:libedit:conformance-policy]
epitome "Reproduce defects through translation and test; fix them in idiomatization, including for C consumers."
state @decided
category @executive
scope {
    elements ([arch:libedit:c-abi])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Fix the defects found during markup as we port them."
        rejected_because "A drop-in replacement that behaves better is still behaving differently, and the difference lands on consumers who never opted into it. Fixes are a decision per defect, not a default."
    }
    {
        option "Reproduce everything, undefined behaviour included."
        rejected_because "Not available. A safe Rust port cannot reproduce an out-of-bounds read, and emulating one would mean writing unsafe code to preserve a defect nobody depends on."
    }
)
consequences {
    accepted (
        "Observable means observable to a C caller across the exported ABI. The core is not bound by it — see [dec:libedit:idiomatic-core]."
        "The six known behavioural forks default to reproduce: the physical-tabs capability, H_FUNC's dropped ref pointer, free_history_entry's empty body, the pointer-sorting completion comparator, tilde expansion of a bare tilde, and el_deletestr1's arithmetic."
        "Where the port defines what the C left undefined, the choice is recorded in the rule rather than left to the implementation."
        "Conformance tests assert the reproduced behaviour, so a later fix has to change a test deliberately rather than drift."
        "Reproduction is a discipline for translation and test, not the shipped end state. Idiomatization fixes the defects, C consumers included, and docs/errata.md is that work list."
        "Each fix in idiomatization bumps its rule's version, which invalidates the stale annotations and forces the change to be re-verified rather than assumed."
    )
    deferred (
        "Two fixes can break a consumer that already compensates: making free_history_entry actually free turns a leak into a double free, and repairing H_FUNC's dropped ref makes history_end start freeing caller memory. Both need handling beyond just correcting the code."
        "How the fixed behaviour is announced — release notes, a soname decision, or both."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
}
---

## Rationale

The port is a drop-in replacement, so the question every ported function
raises is not "what should this do" but "what does this do". Those
diverge more often than expected: the markup pass found defects across
the readline compatibility layer, the allocation-failure paths, and a
handful of live editing behaviours.

Through translation and test, the answer is to reproduce them. Not
because the defects deserve to survive — they do not, and idiomatization
fixes them, C consumers included — but because fixing a defect while
also translating it is how you lose track of which of the two changed
the behaviour. Reproduce first and the port is provably equivalent;
then fix, with tests that already pass proving exactly what moved.

So the sequence is deliberate. Wave 2 translates the defect. Wave 3
writes a test asserting it. Wave 4 fixes it and changes that test on
purpose, bumping the rule's version so the annotations go stale and the
change has to be re-verified rather than assumed. `docs/errata.md` is
the register that carries this across the waves: every entry has a
stable id, so a test can name what it pins and a later commit can name
what it corrected.

The exception is undefined behaviour, where reproduction is not on the
table. A safe Rust port cannot read past an allocation, and writing
`unsafe` to preserve a defect would be absurd. So where the C is
undefined the port *defines*, and the `sem` rule records which
definition was chosen. Where the C's undefined construct still has a
determinate observable result on the platforms we target, that result is
what we define; where it does not, the rule says so and picks the safe
reading.

This splits the markup findings cleanly. The memory-safety defects —
the history-expansion buffer overflow, the stale literal sentinels, the
tokenizer's walk past its guard, the `\U+` escape running off the end of
its string — are all undefined, and all get defined behaviour. The
behavioural forks are defined and merely wrong, so they stand until
someone decides otherwise.

One boundary condition matters more than it first appears: *observable*
means observable to a C caller through the exported ABI. A defect that
exists only because of a C representation — a pointer invalidated by the
next call, a sentinel packed into a spare bit — has nothing to reproduce
inside a core that does not share the representation. The obligation
lands on the ABI crate, which is where the contract is visible. See
[[idiomatic-core]].

Deciding this once, up front, is what keeps translation from becoming
an argument. An agent porting a function does not get to weigh whether a
defect is worth keeping; it reproduces, or if the construct is
undefined, it defines and records. Disagreement goes into a superseding
decision, where it is visible. See [[no-c-ffi]] for why the ABI is the
contract in the first place.
