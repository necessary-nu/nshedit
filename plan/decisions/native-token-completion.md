---
id [dec:libedit:native-token-completion]
epitome "Tokenization returns owned syntax-aware values, while completion is a stale-checked query, candidate, and atomic-edit protocol across the host-effect boundary."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.token-completion+1]
        [spec:nshedit:req:core.effect-hooks]
        [spec:nshedit:req:core.line-commands]
        [spec:nshedit:req:core.text-screen-model]
        [spec:nshedit:req:abi.typed-completion]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Wrap TokenizerGen and the translated filecomplete module in safer method names."
        rejected_because "Their flat buffers, offset argv slots, integer continuation codes, ambient stdin, and callback-driven process state would remain the real implementation rather than boundary compatibility data."
    }
    {
        option "Store a safe completion closure inside Editor and invoke it during command dispatch."
        rejected_because "The ABI implementation can invoke foreign code and re-enter the same editor; even a Rust closure would run while Editor is mutably borrowed and violate the established effect boundary."
    }
    {
        option "Return Vec<Text> from completion and let the host decide how to edit the line."
        rejected_because "A bare vector loses source span, quoting, display spelling, append behavior, stale-response detection, and the distinction between no match, a unique match, and an ambiguous common-prefix edit."
    }
    {
        option "Return tokens as borrows into tokenizer-owned scratch storage."
        rejected_because "Results would be invalidated by the next parse and would recreate the lifetime coupling of argv pointers into a movable word buffer."
    }
)
consequences {
    accepted (
        "Tokenizer owns only its separator policy and borrows input for the duration of a parse. Each result owns cooked Text tokens, checked source spans, and a typed cursor position; incomplete input names the unmatched quote or escape instead of returning an integer code."
        "Only Scalar variants have syntax meaning: ASCII quotes, escape, and newline plus configured scalar separators. RawByte and CompatibilityWide values always remain ordinary token data, so boundary-preserved input is never reinterpreted as control syntax."
        "CompletionQuery owns the line snapshot, checked cursor and replacement span, cooked stem, and quoting style needed after the editor borrow ends. A stale snapshot cannot edit a line changed during a reentrant host call."
        "Completion candidates own insertion text, optional display text, and an optional suffix. A typed collection filters non-prefix results, removes duplicate insertions deterministically, computes a logical-unit common prefix, and distinguishes no match, unique replacement, and ambiguous replacement or listing."
        "CompletionEffect carries the query and accepts only the typed candidate collection. Filesystem scans, passwd lookup, application callbacks, and policy decisions run outside Editor; the core stores no generator, callback, thread-local result, or process-global scan state."
        "Applying a completion encodes the replacement for its quote context, revalidates the query snapshot and span, and records the whole replacement as one undoable line edit."
        "The translated tokenizer and file-completion engine is absent from the core. The ABI adapter owns only the temporary argv, C generator, and completion-record projections required by callers."
        "Private ABI completion accepts one typed request and returns one typed report. Candidates and suffixes are owned; C flags, status values, out-parameters, callback parking, and result publication exist only in exported wrapper scopes."
    )
    deferred (
        "Candidate ranking, fuzzy matching, interactive cycling, and menu presentation policy belong to future native consumers rather than this deterministic baseline."
        "Filesystem-specific suffix discovery and directory traversal stay host services; a later convenience crate may provide a native implementation without adding ambient I/O to Editor."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:text-and-screen-model] [dec:libedit:effect-driven-hooks] [dec:libedit:native-line-state])
    related_to ([dec:libedit:opaque-abi-adapter] [dec:libedit:conformance-policy] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:core.token-completion+1]
    [spec:nshedit:req:abi.typed-completion]
)
---

## Rationale

The compatibility tokenizer and completion machinery are lifetime protocols
disguised as string processing. Token words borrow a reallocating flat buffer;
completion arrays use a reserved prefix slot and NULL terminator; generators
restart through integer state; and file completion reaches global input,
filesystem, passwd, and callback state while mutating the editor in place.
Changing the spelling of those APIs would not remove any of their coupling.

The native seam is two pure transformations separated by one explicit host
effect. Tokenization turns borrowed logical input into an owned structural
result. Completion turns an owned, snapshot-bound query plus owned candidates
into a typed outcome and at most one atomic edit. The host may perform
arbitrary or reentrant work while producing candidates because no Editor
borrow crosses that boundary; when it returns, snapshot validation prevents a
response for an old line from being applied to new state.

Compatibility remains an adapter responsibility. Exported wrappers map owned
tokens back to temporary argv storage and adapt C generators into a typed
provider for the duration of one reentrant call. The private completion engine
receives one request and returns one report with owned suffixes, while the core
never learns pointer, sentinel, callback, flag, out-parameter, or integer-status
conventions.
