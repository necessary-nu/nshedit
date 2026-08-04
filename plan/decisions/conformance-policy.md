---
id [dec:libedit:conformance-policy]
epitome "Reproduce every defined observable behaviour, bugs included; define what the C leaves undefined."
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
        "The six known behavioural forks default to reproduce: the physical-tabs capability, H_FUNC's dropped ref pointer, free_history_entry's empty body, the pointer-sorting completion comparator, tilde expansion of a bare tilde, and el_deletestr1's arithmetic."
        "Where the port defines what the C left undefined, the choice is recorded in the rule rather than left to the implementation."
        "Conformance tests assert the reproduced behaviour, so a later fix has to change a test deliberately rather than drift."
    )
    deferred (
        "Each fork may be overridden by its own decision superseding this default. That is expected for at least some of them."
        "Whether reproduced defects are documented to consumers, and where."
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

The default is to reproduce them. A replacement that quietly behaves
better is still a replacement that behaves differently, and the
difference arrives at consumers who never asked for it. Someone whose
completion list comes back in libedit's peculiar order has code shaped
around that order. Improving it is a decision to be taken per defect,
with its consequences understood — not a side effect of finding it
during translation.

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

Deciding this once, up front, is what keeps translation from becoming
an argument. An agent porting a function does not get to weigh whether a
defect is worth keeping; it reproduces, or if the construct is
undefined, it defines and records. Disagreement goes into a superseding
decision, where it is visible. See [[no-c-ffi]] for why the ABI is the
contract in the first place.
