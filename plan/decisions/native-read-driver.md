---
id [dec:libedit:native-read-driver]
epitome "The native read loop is a resumable, token-checked driver over owned input, typed effects, semantic keys, and transactional terminal state."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules ([spec:nshedit:req:core.read-driver] [spec:nshedit:req:core.effect-hooks] [spec:nshedit:req:core.line-commands] [spec:nshedit:req:core.command-effects] [spec:nshedit:req:abi.signal-lifecycle])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Hide the translated read, parse, and signal structs behind safe methods."
        rejected_because "Their callback slots, command integers, locale scratch state, and process-global signal machinery would remain the implementation."
    }
    {
        option "Let a Host trait perform callbacks while drive holds &mut Editor."
        rejected_because "An ABI callback may re-enter the same opaque handle, so the apparent safe borrow would alias foreign access."
    }
)
consequences {
    accepted (
        "Each Pending<E> owns an Editor suspension plus a private driver step token; host work runs with neither Editor nor ReadDriver borrowed, and stale or cross-driver steps are rejected."
        "ReadEffect accepts owned byte chunks, boundary-decoded units, EOF, semantic signals, and explicit key-prefix timeout. The incremental UTF-8 decoder preserves invalid and incomplete bytes as RawByte."
        "The driver owns key-prefix state, exact-versus-longer disambiguation, bounded repeat counts, bounded macro and semantic replay, and lossless reprocessing after an ambiguous fallback. Multi-key bindings use typed sequences, while operators use closed continuations with checked text anchors rather than C command-state fields."
        "Completion, history navigation and search, history selection, aliases, editor-command input, external editing, prompts, resize, user commands, and signal propagation use operation-specific typed effects. Rendering alone uses the caller's safe Write so screen state still commits only after emission succeeds."
        "Signals are semantic values rather than platform numbers. Stop and terminating transitions enter Cooked mode before host propagation; continue re-enters Editing; suspend resumes through a separate Continue propagation; typed prepare, delivery, and resume resize causes control display reconstruction."
        "Accepted, character, EOF, cancelled, and interrupted results are typed. Every error clears driver-local transient state, attempts Cooked mode, and leaves Editor's transactional mode plus RAII restoration valid."
    )
}
edges {
    requires ([dec:libedit:effect-driven-hooks] [dec:libedit:editor-session-ownership] [dec:libedit:native-line-state] [dec:libedit:native-history] [dec:libedit:native-terminal-render] [dec:libedit:native-token-completion])
    related_to ([dec:libedit:opaque-abi-adapter] [dec:libedit:platform-layer] [dec:libedit:signal-lifecycle] [dec:libedit:lint-policy])
}
codifies ([spec:nshedit:req:core.read-driver] [spec:nshedit:req:core.command-effects] [spec:nshedit:req:abi.signal-lifecycle])
---

## Rationale

The ownership boundary is the loop itself. Effects and display requests are
owned continuation tokens, so the later ABI adapter can call foreign code or
write through its boundary adapter only after Rust borrows have ended. The
core retains parsing and transition state, while platform signal installation,
locale conversion, C callbacks, descriptors, and pointer lifetimes stay out.
