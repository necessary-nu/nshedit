---
id [dec:libedit:effect-driven-hooks]
epitome "Host-controlled and foreign operations suspend as typed effects and resume only after the editor borrow ends."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.effect-hooks]
        [spec:nshedit:req:core.read-driver]
        [spec:nshedit:req:abi.opaque-owner]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Store extern C callbacks and opaque cookies in the core."
        rejected_because "It imports C representation and unsafe invocation into the native library and prevents the core from forbidding unsafe code."
    }
    {
        option "Call a safe Host trait inline while holding &mut Editor."
        rejected_because "The ABI implementation of that trait can invoke foreign code which re-enters the same C handle, creating an aliased mutable borrow or forcing unsound escape hatches."
    }
    {
        option "Reject all callback reentrancy."
        rejected_because "Deployed C callbacks may call permitted el_get/el_set/readline operations. Removing that behaviour is an ABI divergence, not a Rust implementation choice."
    }
)
consequences {
    accepted (
        "The editor advances until it completes or yields a typed effect such as prompt, input, history, alias, resize, completion, environment, or user command."
        "An effect owns or explicitly borrows all request data needed by the host and names the typed response accepted on resume."
        "The driver releases the editor borrow before handling an effect. The ABI may therefore invoke foreign code and service permitted reentrant operations without aliasing Rust references."
        "Built-in operations remain ordinary typed core code; only host-controlled boundaries suspend."
        "Cancellation, EOF, interruption, and callback failure are typed responses and leave the editor resumable or safely finishable."
    )
    deferred (
        "An async facade may later drive the same effect protocol; it is not required for the synchronous API."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core])
    related_to ([dec:libedit:opaque-abi-adapter] [dec:libedit:platform-layer])
}
codifies (
    [spec:nshedit:req:core.effect-hooks]
    [spec:nshedit:req:core.read-driver]
)
---

## Rationale

Foreign callback reentrancy is the hard ownership problem in this rewrite.
Changing a function-pointer field into a closure field makes the syntax more
Rust-like but does not solve calling out while the editor is mutably borrowed.

A suspend/resume protocol does. The core produces data describing the next
external operation, returns control, and is borrowed again only when a typed
response is ready. Native callers can drive the same protocol safely, while
the ABI adapter can temporarily expose its opaque handle to C without a live
Rust reference into the editor.
