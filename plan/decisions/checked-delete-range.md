---
id [dec:libedit:checked-delete-range]
epitome "el_deletestr1 deletes one checked half-open range and reports the characters actually removed."
state @decided
category @property
scope {
    elements ([arch:libedit:c-abi])
    rules (
        [spec:libedit:sem:chared.el-deletestr1-fn]
        [spec:libedit:sem:histedit.el-deletestr1-fn]
        [spec:libedit:sem:readline.rl-delete-text-fn]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Continue reproducing the imported el_deletestr1 arithmetic."
        rejected_because "It corrupts ordinary middle deletions, refuses the line-end boundary, over-reports partial deletion, and can leave the cursor outside the shortened line."
    }
    {
        option "Reject every endpoint outside the current line."
        rejected_because "An end at the one-past-line boundary is a valid half-open range, and clamping an oversized end gives the operation a useful deterministic meaning without admitting an unchecked index."
    }
    {
        option "Clamp negative endpoints to zero."
        rejected_because "A negative C offset is not a logical line boundary; rejecting the call preserves the existing safe definition instead of silently widening the requested deletion."
    }
)
consequences {
    accepted (
        "The exported symbol, signature, and wide-character unit remain unchanged; only the previously defective observable semantics change."
        "A negative endpoint, an empty or reversed range, or a start at or beyond the line end returns zero without mutation."
        "An end at the line length is valid and an oversized end clamps to the line length. The complete normalized half-open span is removed and the return value is its actual length."
        "The cursor is rebased through the edit: positions before the span stay fixed, positions inside it move to the start, and positions after it shift left by the removed length."
        "The three detailed semantic rules carry a new version, and direct Rust and C acceptance tests observe the resulting text, return value, and cursor."
    )
    deferred (
        "Converting readline's byte-oriented rl_point and rl_end into the wide-character units consumed by el_deletestr1; ERR-readline-35 remains a separate compatibility defect."
    )
}
edges {
    requires ([dec:libedit:conformance-policy] [dec:libedit:native-line-state] [dec:libedit:opaque-abi-adapter])
    related_to ([dec:libedit:idiomatic-core] [dec:libedit:rust-internal-boundary])
}
codifies (
    [spec:libedit:sem:chared.el-deletestr1-fn]
    [spec:libedit:sem:histedit.el-deletestr1-fn]
    [spec:libedit:sem:readline.rl-delete-text-fn]
)
---

## Rationale

The imported implementation never performed the range operation its public
name and signature describe. It moved only a prefix of the following tail,
shortened the line by that copied prefix, rejected the ordinary one-past-end
boundary, and left cursor state unrelated to the mutation. These are defined
and visible C behaviours, so the conformance policy requires an explicit
decision and versioned rules before correcting them.

The adapter already receives integer offsets at the foreign boundary and owns
the conversion into the core's checked text model. Normalizing the end once,
constructing one checked span, and rebasing the cursor through the same edit
gives every input a deterministic result without carrying pointer arithmetic
or the imported copy-loop defect into private Rust.
