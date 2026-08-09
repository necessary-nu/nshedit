---
id [dec:libedit:rust-internal-boundary]
epitome "C representation ends at exported wrappers; every maintained private path is typed, suppression-free Rust named for its actual responsibility."
state @decided
category @ban
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi] [arch:libedit:platform] [arch:libedit:terminal-caps])
    rules (
        [spec:nshedit:req:abi.rust-internals]
        [spec:nshedit:req:abi.typed-history]
        [spec:nshedit:req:abi.typed-completion]
        [spec:nshedit:req:abi.typed-session]
        [spec:nshedit:req:platform.typed-boundary]
        [spec:nshedit:req:terminal.typed-api]
        [spec:nshedit:req:workspace.lint-policy]
        [spec:nshedit:req:workspace.self-contained]
        [spec:nshedit:req:workspace.semantic-naming]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Let private ABI code reuse the exported varargs functions and C status protocols."
        rejected_because "That moves the anti-corruption boundary inward until private Rust once again depends on opcodes, out-parameters, callback coercions, and symbol linkage. The exported wrapper is the only place those representations are required."
    }
    {
        option "Keep lint expectations for implementation shapes forced by the ABI."
        rejected_because "The two existing expectations describe avoidable private designs. An exact exported signature can remain exact without making the private implementation trigger a lint."
    }
    {
        option "Retain native, legacy, compatibility, and translated labels as migration history."
        rejected_because "Relative labels conceal responsibility and stable identity once only one implementation remains. History belongs in decisions and version control, not maintained identifiers."
    }
)
consequences {
    accepted (
        "Exported wrappers alone decode C scalars, varargs, raw pointers, callbacks, operation codes, out-parameters, and status values; private code receives typed operations and returns typed replies or errors."
        "Private Rust never calls this crate's exported symbols through an extern declaration or link_name, and no callback is invoked through a transmuted function-pointer type."
        "The workspace contains no allow or expect lint attributes. External constraints are modeled so the lint does not arise, and generated inputs are fixed or isolated from first-party checked source."
        "Core text, modules, examples, formats, and adapter accessors use boundary-neutral responsibility or proper-format names rather than relative migration labels."
        "Platform APIs expose borrowed descriptors, typed actions, and descriptive errors; raw layouts, constants, descriptors, and libc callback tables remain private."
        "History, completion, editor-session, readline-runtime, and terminal-capability internals use typed state whose invalid combinations are unrepresentable."
        "Every dependency resolves from the repository or a declared registry/repository source, so a clean checkout needs no sibling personal checkout."
        "Required C symbols, layouts, mutable data exports, varargs entry points, pointer lifetimes, and observable defects remain unchanged at the drop-in boundary."
    )
    deferred ()
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:opaque-abi-adapter] [dec:libedit:platform-layer] [dec:libedit:conformance-policy])
    related_to ([dec:libedit:lint-policy] [dec:libedit:native-read-driver] [dec:libedit:native-history] [dec:libedit:native-token-completion] [dec:libedit:text-and-screen-model])
}
codifies (
    [spec:nshedit:req:abi.rust-internals]
    [spec:nshedit:req:abi.typed-history]
    [spec:nshedit:req:abi.typed-completion]
    [spec:nshedit:req:abi.typed-session]
    [spec:nshedit:req:platform.typed-boundary]
    [spec:nshedit:req:terminal.typed-api]
    [spec:nshedit:req:workspace.lint-policy]
    [spec:nshedit:req:workspace.self-contained]
    [spec:nshedit:req:workspace.semantic-naming]
)
---

## Rationale

An ABI adapter is an anti-corruption layer only when foreign representation
ends immediately after argument parsing. Calling an exported variadic symbol
from private Rust, passing integer operation codes through private dispatch,
or reshaping callbacks with global storage preserves the implementation model
that the adapter exists to contain.

Drop-in compatibility constrains what a C caller can observe. It does not
constrain private Rust signatures, module structure, error types, ownership,
or names. Keeping that distinction exact allows the public ABI to remain
unchanged while the maintained implementation becomes ordinary reviewable
Rust rather than a second spelling of the C program.
