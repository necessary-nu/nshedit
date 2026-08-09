---
id [dec:libedit:signal-lifecycle]
epitome "Signal dispositions are scoped platform ownership; the core drives semantic transitions and the C adapter chains caller policy."
state @decided
category @property
scope {
    elements ([arch:libedit:platform] [arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:abi.signal-lifecycle]
        [spec:nshedit:req:core.read-driver]
        [spec:libedit:sem:sig.sig-set-fn]
        [spec:libedit:sem:sig.sig-clr-fn]
        [spec:libedit:sem:sig.sig-handler-fn]
        [spec:libedit:sem:read.read-char-fn]
        [spec:libedit:sem:read.el-wgetc-fn]
        [spec:libedit:sem:read.el-wgets-fn]
        [spec:libedit:sem:read.read-prepare-fn]
        [spec:libedit:sem:read.read-finish-fn]
        [spec:libedit:sem:el.el-resize-fn]
        [spec:libedit:sem:histedit.el-resize-fn]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Retain the C handler's terminal, allocation, display, and propagation work in async signal context."
        rejected_because "It re-enters mutable editor, allocator, and buffered-I/O state from an interrupt and therefore preserves undefined behaviour rather than compatibility."
    }
    {
        option "Expose sigaction layouts, platform signal numbers, and disposition storage to the core driver."
        rejected_because "Those are system-ABI and C-compatibility mechanics; admitting them would contaminate the safe Rust-domain API and duplicate the platform boundary."
    }
    {
        option "Keep a last-editor-wins process-global EditLine pointer."
        rejected_because "Concurrent handles target the wrong terminal and destruction leaves a dangling pointer reachable by the next delivery."
    }
    {
        option "Consume signals on a dedicated sigwait or platform-specific signal thread."
        rejected_because "An embedded library cannot retroactively control every host thread's signal mask, and taking that ownership would change caller-wide process policy."
    }
)
consequences {
    accepted (
        "One thread-bound SignalHandlers value owns a selected disposition set. It publishes a stable boxed atomic through a compare-and-swap slot, stores displaced dispositions as consumable options, fails construction if any requested installation fails, restores them before withdrawing the slot, reports explicit-restoration failures, and rejects a second simultaneous owner."
        "The installed trampoline performs only an atomic load and store. Terminal modes, display allocation, callbacks, restoration, re-raising, and rearming run from normal context after the read boundary observes the recorded signal."
        "A thread-bound BlockedSignals guard restores the caller's exact signal mask and protects window-size queries and display rebuilds from SIGWINCH re-entry."
        "The core represents delivery as a closed Signal value and separates prepare, signal, and resume resize causes. Stop and terminating signals request Cooked mode before propagation; continue requests Editing mode; suspend resumes through a distinct Continue propagation before display reconstruction."
        "The C adapter maps platform delivery into the native protocol. Buffered edited reads own a local handler guard, unbuffered mode retains one on the opaque handle, signal-disabled reads never mutate caller dispositions, and every normal exit or handle destruction restores what was displaced."
        "Previous dispositions are restored and re-raised on the delivery thread. Resize, continue, and suspend are rearmed after their resumable lifecycle; interrupting and terminating signals remain consumed from the editor scope after propagation."
        "A foreign read callback cannot be interrupted with editor work safely. Delivery is recorded asynchronously, then terminal and disposition work runs after the callback returns; the callback's own failure remains its read result. This is the deterministic safe definition for the reference handler's undefined re-entry into arbitrary callback state."
    )
    deferred (
        "Concurrent signal-owning editor sessions in one process; callers may use multiple handles, but only one may temporarily own process dispositions."
        "A non-Linux signal ABI and its independently verified layouts and numbers."
    )
}
edges {
    requires ([dec:libedit:platform-layer] [dec:libedit:native-read-driver] [dec:libedit:effect-driven-hooks] [dec:libedit:conformance-policy])
    related_to ([dec:libedit:editor-session-ownership] [dec:libedit:no-c-ffi] [dec:libedit:opaque-abi-adapter])
}
codifies (
    [spec:nshedit:req:abi.signal-lifecycle]
    [spec:nshedit:req:core.read-driver]
    [spec:libedit:sem:sig.sig-set-fn]
    [spec:libedit:sem:sig.sig-clr-fn]
    [spec:libedit:sem:sig.sig-handler-fn]
    [spec:libedit:sem:read.read-char-fn]
    [spec:libedit:sem:read.el-wgetc-fn]
    [spec:libedit:sem:read.el-wgets-fn]
    [spec:libedit:sem:read.read-prepare-fn]
    [spec:libedit:sem:read.read-finish-fn]
    [spec:libedit:sem:el.el-resize-fn]
    [spec:libedit:sem:histedit.el-resize-fn]
)
---

## Rationale

The observable contract is an ordering contract: libedit temporarily owns a
known signal family, restores the terminal before a stop or terminating action,
passes delivery to the disposition it displaced, resumes editing after job
control, and leaves caller policy intact when the scope ends. None of those
observations requires editor work to execute in an asynchronous handler.

The platform layer therefore owns the irreducible process-global mechanism and
publishes only typed, scoped operations. A stable atomic is the handler's sole
connection to ordinary code. Blocking the selected family makes installation,
propagation, restoration, and withdrawal atomic with respect to the delivery
thread; optional saved dispositions prevent stale actions from surviving into a
later read.

The native core owns meaning rather than mechanism. Its driver requests terminal
and resize transitions through typed effects, so the ABI can end every editor
borrow before it touches a descriptor, calls foreign code, or changes process
policy. The adapter then reconstructs the C-visible lifecycle around buffered,
unbuffered, edited, and direct reads without exposing signal numbers or C
layouts to the core.
