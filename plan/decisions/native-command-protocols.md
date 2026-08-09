---
id [dec:libedit:native-command-protocols]
epitome "Every editor command is composed from closed Rust semantic protocols; only registered host commands retain names at dispatch time."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.command-sequences]
        [spec:nshedit:req:core.command-effects]
        [spec:nshedit:req:abi.binding-dispatch]
        [spec:nshedit:req:abi.bindings]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Carry every historical C command name or generated command number into the native Action model."
        rejected_because "That would make the compatibility catalog the core's type system and preserve command-specific C coupling instead of expressing shared editing semantics."
    }
    {
        option "Represent an unimplemented built-in as a registered user command and beep when no callback exists."
        rejected_because "It makes inventory lookup appear complete while silently replacing defined built-in behaviour with an error path."
    }
    {
        option "Recreate the C repeat, meta-next, and Vi operator fields behind private Rust methods."
        rejected_because "Command numbers, operator bit masks, and pointer anchors would remain the real state machine even if field access were memory-safe."
    }
)
consequences {
    accepted (
        "The ABI catalog maps every compatibility command name to closed semantic actions, dispatch-context ImmediateCommand values, command-sequence continuations, or typed host effects. Only caller-registered command names use Binding::User and the user-command effect."
        "The read driver owns bounded repeat and replay state plus closed continuations for commands that consume later input. Vi operators compose with semantic motions and checked edit targets rather than storing C action masks or buffer pointers."
        "History search, alias expansion, command input, and external editing suspend through operation-specific effects after native borrows end; the adapter alone invokes foreign callbacks or platform facilities."
        "A repeat count is part of a host-navigation request and is applied atomically by the effect handler. Interactive search and command-input continuations own their temporary line lifecycle and retain the keymap context needed to interpret termination."
        "Locale-dependent 8-bit meta keys and narrow-history numbering defects are adapter projections. Native default keymaps contain logical Rust text sequences only, and native history effects carry semantic selections rather than compatibility event numbers."
        "Compatibility-only cursor conventions and documented defects are explicit adapter policies over checked native operations, never placeholder success, unconditional error, or unconditional beep."
        "Conformance executes every built-in through an installed binding in both editing maps, with and without a repeat count, and observes its resulting line, cursor, mode, output, effect, or continuation."
    )
}
edges {
    requires ([dec:libedit:native-line-state] [dec:libedit:native-read-driver] [dec:libedit:effect-driven-hooks])
    related_to ([dec:libedit:opaque-abi-adapter] [dec:libedit:conformance-policy] [dec:libedit:native-history])
}
codifies (
    [spec:nshedit:req:core.command-sequences]
    [spec:nshedit:req:core.command-effects]
    [spec:nshedit:req:abi.binding-dispatch]
)
---

## Rationale

The compatibility command table is a name-to-behaviour catalog, not a suitable
native domain model. Several C functions are aliases for one semantic edit;
others are phases of one interaction; still others cross a host boundary.
Treating every missing mapping as `Action::User` erased those distinctions and
made a successful `bind` call conceal an unconditional beep at execution time.

The native split is therefore by protocol. Self-contained edits stay ordinary
actions; commands whose edit depends on the invoking unit, repeat count, mode,
or pending operator use the closed `ImmediateCommand` protocol; interactions
that consume more keys become typed driver continuations; and operations
controlled by the application or platform become typed effects. The ABI
adapter resolves legacy names into those closed values and translates the
result back to C status codes only after the native operation has run.

Counts remain data when an operation crosses the host boundary. Replaying a
history callback once per count would expose intermediate callback state and
could leave a partially navigated line; one counted request instead lets the
adapter reproduce the reference operation atomically. Likewise, prompted
search and external-command input are driver-owned interactions: their
temporary line is cleared or accepted by the continuation itself, while the
adapter supplies history storage, process execution, and locale projection.
