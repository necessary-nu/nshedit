---
id [dec:libedit:history-regex-dialect]
epitome "Compatibility history search is literal-first, with Rust regex syntax as its fallback."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules ([spec:nshedit:req:abi.history-effects+2])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Call the platform POSIX basic-regular-expression API."
        rejected_because "It would reintroduce a C and locale boundary solely for an internal matching policy, contrary to the native core and no-C-FFI decisions."
    }
    {
        option "Translate POSIX basic regular expressions into Rust regex syntax."
        rejected_because "A partial translator would add a second parser and ambiguous extension policy for no product benefit; the maintained Rust dialect is the intended interface."
    }
    {
        option "Keep literal substring matching only."
        rejected_because "History search deliberately accepts metacharacter patterns after the literal interpretation fails."
    }
)
consequences {
    accepted (
        "Every compatibility history search first tests the owned Text units as an unanchored literal substring, including raw bytes and opaque code points."
        "When the literal test fails and both operands contain only Unicode scalar values, the adapter compiles the pattern with regex::Regex and applies its boolean is_match result."
        "An invalid Rust regex is a silent no-match, while the same invalid pattern can still match through the earlier literal test."
        "Raw bytes and opaque code points participate in literal matching but never in the regex fallback; matching is independent of the process locale."
        "The detailed compatibility corpus continues to describe the original POSIX BRE implementation exactly; this decision and the native rule record the deliberate maintained-code divergence."
    )
    deferred ()
}
edges {
    requires ([dec:libedit:conformance-policy] [dec:libedit:no-c-ffi] [dec:libedit:text-and-screen-model])
    related_to ([dec:libedit:native-command-protocols])
}
codifies ([spec:nshedit:req:abi.history-effects+2])
---

## Rationale

History search has two interpretations in a fixed order. Literal matching is
the predictable hot path and also preserves every logical text unit. A failed
literal match may then use pattern syntax, but that syntax is a maintained Rust
interface rather than a locale-sensitive emulation of the retired C runtime.

The `regex` crate supplies one safe, documented dialect over Unicode scalar
text. Keeping the fallback in the ABI host avoids imposing that dependency on
the native editor core: the core owns only the typed matching policy, and the
host that owns compatibility history executes it. Invalid patterns are ordinary
misses, so callers never acquire a regex-error channel absent from the C API.
