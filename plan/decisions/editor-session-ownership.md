---
id [dec:libedit:editor-session-ownership]
epitome "A native editor owns one terminal-restoration obligation while I/O capabilities remain outside its private state."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.raii-lifecycle]
        [spec:nshedit:req:core.rust-io]
        [spec:nshedit:req:core.effect-hooks]
        [spec:nshedit:req:abi.opaque-owner]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Store input, output, and host hooks inside Editor."
        rejected_because "Handling them would retain the editor borrow across host-controlled work and defeat the suspend/resume ownership boundary."
    }
    {
        option "Leave terminal restoration entirely to each driver or ABI adapter."
        rejected_because "Native callers would lose RAII cleanup, and every integration would have to reproduce the same exactly-once state machine."
    }
    {
        option "Offer Drop cleanup without an explicit finish operation."
        rejected_because "Drop cannot report restoration failure, so callers that care whether the terminal was restored would have no reliable result."
    }
)
consequences {
    accepted (
        "Editor owns a TerminalControl value behind private state and consumes its restoration obligation before calling restore, so finish, Drop, errors, and repeated internal cleanup cannot restore twice."
        "A failed activation receives one best-effort restoration attempt before construction fails; both activation and restoration failures remain inspectable."
        "Explicit finish reports restoration failure. Drop ignores the error from its one best-effort attempt and does not panic because restoration returned an error."
        "SessionIo is a separate borrowed capability set over std::io::Read, std::io::Write, and BorrowedFd. Editor never owns streams or descriptor ownership."
        "The effect driver may end the editor borrow before it uses SessionIo or invokes a host operation, preserving the later reentrancy boundary."
    )
    deferred (
        "A convenience native driver may own concrete streams around Editor after the typed effect protocol exists."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:effect-driven-hooks])
    related_to ([dec:libedit:opaque-abi-adapter] [dec:libedit:platform-layer])
}
codifies (
    [spec:nshedit:req:core.raii-lifecycle]
    [spec:nshedit:req:core.rust-io]
    [spec:nshedit:req:core.effect-hooks]
    [spec:nshedit:req:abi.opaque-owner]
)
---

## Rationale

Terminal restoration and host reentrancy pull ownership in opposite
directions. Restoration must stay attached to the editor so every exit has
one cleanup authority. Host-controlled reads, writes, prompts, history, and
completion must stay outside it so handling one never occurs through a live
mutable borrow of the editor.

The split is therefore between a small terminal controller owned by the
session and borrowed I/O capabilities owned by its driver. The controller's
activation failure is treated as potentially partial and is restored once;
the active controller is taken before any later restoration call. That makes
exactly-once cleanup a consequence of ownership instead of a convention.
