---
id [dec:libedit:native-line-state]
epitome "Line editing is a private checked state machine over semantic actions and logical key sequences, never C command numbers or pointer offsets."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.line-commands]
        [spec:nshedit:req:abi.behavioural-conformance]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep the translated line structs and place typed methods over them."
        rejected_because "Pointer offsets, exposed mutable buffers, integer modes, and command-number dispatch would remain the actual invariants and make the native API a facade over the C model."
    }
    {
        option "Encode every historical editor command and erratum as a native action variant."
        rejected_because "The compatibility corpus includes low-byte key dispatch, sentinel states, and command-specific defects that are ABI obligations rather than desirable native semantics. The adapter can compose checked spans and semantic operations to reproduce them."
    }
    {
        option "Use 256-entry integer key tables and a separate macro trie like the reference implementation."
        rejected_because "That truncates logical input to a byte, permits invalid command numbers, and makes exact bindings disagree with prefix state. A map keyed by validated logical sequences represents all three match outcomes directly."
    }
    {
        option "Store closures or foreign callbacks as key bindings."
        rejected_because "A callback can re-enter the editor. Bindings must resolve to owned semantic actions or macros, while host-defined commands cross the existing typed effect boundary after the editor borrow ends."
    }
)
consequences {
    accepted (
        "The native Editor owns one private line State containing Text, checked cursor and mark boundaries, input/keymap modes, one owned kill register, exact search state, deterministic keymaps, and undo/redo snapshots."
        "Action names semantic operations. EditTarget resolves characters, words, motions, embedded lines, the whole buffer, checked spans, and marked regions without exposing buffer pointers or integer operation codes."
        "One undo record represents one successful command-level text mutation. Cursor-only commands, mode changes, searches, register copies, and no-ops create no record; a new edit clears redo; an operation error restores its pre-command snapshot."
        "Insert, replace, delete, kill, yank, Unicode case transformation, and transpose preserve raw bytes and non-scalar compatibility values. Marks are revalidated and rebased after every text replacement."
        "Word motion treats Unicode whitespace explicitly, treats alphanumeric scalars plus underscore as ordinary words, and keeps raw bytes, non-scalar wide values, and scalar punctuation in the non-word class. Big-word motion groups every non-whitespace unit."
        "A KeySequence is non-empty logical Text. Each mode map returns Exact, Ambiguous, Prefix, or Unbound and stores only typed Action, closed CommandSequence, or owned macro bindings. Built-in Emacs and Vi maps are native defaults, not transcribed numeric tables."
        "Pure actions return typed outcomes. Completion, history navigation, and user commands return typed CommandStep requests for the driver to turn into effects; no host operation runs inside line-state code."
        "The read driver owns repeat counts, bounded semantic replay, key-prefix disambiguation, and closed continuations. Multi-key bindings remain typed KeySequence values, while Vi operators compose semantic motions with checked anchors; the ABI adapter translates reference quirks rather than adding C operator fields to the core."
    )
    deferred (
        "A native kill ring may replace the single kill register when native consumers require rotating yanks; the drop-in adapter only requires the current register semantics."
        "Interactive history and incremental-search protocols join this state through the history concern and read driver; this layer owns exact search within the current logical line."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:text-and-screen-model] [dec:libedit:effect-driven-hooks] [dec:libedit:editor-session-ownership])
    related_to ([dec:libedit:conformance-policy] [dec:libedit:opaque-abi-adapter] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:core.line-commands]
)
---

## Rationale

The reference editor represents a cursor and mark as pointers into a mutable
wide-character allocation, stores commands as generated integers in byte-keyed
tables, and spreads command-local state through line, map, search, vi, and
history records. Reproducing those representations would preserve the exact
coupling the greenfield core is intended to remove.

The native seam is a checked state transition instead. Text boundaries are
validated when an action runs, mutations are transactional at command
granularity, and every retained auxiliary value owns its data. Key dispatch
answers the real parsing question—including an exact binding that is also a
prefix—without a sentinel command slot or a second trie that can drift.

This does not weaken drop-in compatibility. The detailed corpus remains the
oracle for the ABI adapter, which can inspect typed line values and issue exact
spans, motions, register operations, and mode transitions. Numeric command
IDs, vi cursor conventions, low-byte lookup, and recorded defects therefore
remain translations at the compatibility boundary rather than native Rust
semantics.
