---
id [dec:libedit:text-and-screen-model]
epitome "Logical text and rendered cells are distinct typed values; invalid bytes and wide values are preserved explicitly, never as sentinels."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.typed-domain]
        [spec:nshedit:req:core.text-screen-model]
        [spec:nshedit:req:core.terminal-render]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Use char for every logical unit."
        rejected_because "The compatibility boundary and native history must preserve undecodable bytes and non-scalar wide values that Rust char cannot represent."
    }
    {
        option "Use u32 everywhere and retain the C sentinel bits."
        rejected_because "It makes illegal combinations representable and conflates input text, compatibility transport, and display bookkeeping."
    }
    {
        option "Normalize all input to UTF-8 and reject anything else."
        rejected_because "That loses byte-preserving history and changes narrow/wide ABI behaviour for existing locales and malformed data."
    }
)
consequences {
    accepted (
        "Logical text has explicit variants for Unicode scalar values, undecodable bytes, and compatibility wide values that are not Unicode scalars."
        "Rendered cells are a separate type with explicit text, continuation, and padding states. No spare character bits carry display sentinels."
        "Narrow multibyte and wide C conversion occurs in nshedit-abi under the active locale contract. The core does not expose wchar_t or locale conversion buffers."
        "Native history remains byte-preserving and rejects or reports representations it cannot round-trip instead of silently truncating at NUL."
        "Public cursor and span values use checked domain indices rather than raw pointers or unchecked integer differences."
    )
    deferred (
        "The exact public names and storage optimization of the text variants are chosen with the domain-model implementation; their semantic distinctions are fixed here."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core])
    related_to ([dec:libedit:opaque-abi-adapter])
}
codifies (
    [spec:nshedit:req:core.typed-domain]
    [spec:nshedit:req:core.text-screen-model]
    [spec:nshedit:req:core.terminal-render]
)
---

## Rationale

The transliterated core uses wide integers both as characters and as storage
for rendering flags. Rust's `char` is safer but cannot represent every byte or
wide value the compatibility boundary must carry. A bare integer preserves
those values but also preserves the C model's accidental states.

Explicit logical variants retain information without pretending every value
is Unicode, while a separate screen-cell type makes rendering sentinels part
of the type system. ABI conversion remains locale-aware and isolated; native
consumers get a stable semantic model rather than C integer conventions.
