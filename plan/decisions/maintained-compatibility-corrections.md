---
id [dec:libedit:maintained-compatibility-corrections]
epitome "The maintained ABI corrects four unsafe or representation-driven C behaviours without changing its exported surface."
state @decided
category @property
scope {
    elements ([arch:libedit:c-abi] [arch:libedit:core])
    rules (
        [spec:nshedit:req:abi.caller-fd-flags]
        [spec:nshedit:req:abi.logical-key-bindings]
        [spec:nshedit:req:abi.history-callback-encoding]
        [spec:nshedit:req:abi.internal-completion-dispatch]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Reproduce every defined behaviour of the retired C implementation."
        rejected_because "These four behaviours mutate caller-owned descriptor policy, alias logical keys through an 8-bit representation, infer callback representation from locale state, or let an unrelated exported symbol replace private typed dispatch."
    }
    {
        option "Remove the affected compatibility entry points."
        rejected_because "The exported surface remains drop-in compatible; only the maintained implementation's internal policy and defined results change."
    }
)
consequences {
    accepted (
        "EL_SAFEREAD retries interrupted reads without clearing O_NONBLOCK, O_NDELAY, or any other caller-owned descriptor status flag."
        "Direct bind keys and binding queries retain their complete decoded logical key sequences instead of indexing by the first unit's low byte."
        "The narrow and wide EL_HIST setters install narrow and wide callback representations respectively; locale state never changes the representation promised by the setter."
        "The completion_matches symbol remains exported, while internal TAB completion calls the private typed implementation and cannot be replaced through ELF symbol interposition."
        "The detailed compatibility corpus continues to describe the retired C behaviour exactly; these native rules identify the deliberate maintained-code corrections."
    )
    deferred ()
}
edges {
    requires ([dec:libedit:conformance-policy] [dec:libedit:opaque-abi-adapter] [dec:libedit:native-read-driver])
    related_to ([dec:libedit:native-line-state] [dec:libedit:native-token-completion] [dec:libedit:text-and-screen-model])
}
codifies (
    [spec:nshedit:req:abi.caller-fd-flags]
    [spec:nshedit:req:abi.logical-key-bindings]
    [spec:nshedit:req:abi.history-callback-encoding]
    [spec:nshedit:req:abi.internal-completion-dispatch]
)
---

## Rationale

The compatibility boundary preserves the public C shape, but none of these
behaviours is a useful part of that shape. A read option does not grant the
library ownership of the caller's open-file description. A logical key is not
an eight-bit table index. A callback's record representation is fixed by the
entry point that installed it, not by ambient locale state. An exported helper
does not need to become a dynamically replaceable private implementation.

Keeping those distinctions explicit lets the Rust code use its typed model
without quietly claiming exact agreement with the historical implementation.
