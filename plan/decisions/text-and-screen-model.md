---
id [dec:libedit:text-and-screen-model]
epitome "Logical text, terminal literals, rendered glyphs, and physical cells are distinct typed values; compatibility data is never a display sentinel."
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
        "TextUnit represents a Unicode scalar, a raw undecodable byte, or a validated opaque non-Unicode code point; Text owns a sequence of those units without naming any host transport."
        "ScreenGlyph owns the printable scalar sequence anchored at one terminal column. ScreenCell represents a blank, an anchored glyph, its continuation column, or explicit padding; no spare character bits carry display sentinels."
        "A TerminalLiteral owns a zero-width byte sequence at a render boundary. It is a prompt/render atom rather than a physical ScreenCell, so emitting it cannot consume, replace, or alias a terminal column."
        "Narrow multibyte and wide C conversion occurs in nshedit-abi under the active locale contract. The core exposes only boundary-neutral code-point values, never wchar_t terminology or locale conversion buffers."
        "Native history remains byte-preserving and rejects or reports representations it cannot round-trip instead of silently truncating at NUL."
        "Public cursor and span values use checked domain indices rather than raw pointers or unchecked integer differences."
    )
    deferred ()
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
is Unicode, while anchored glyphs, physical cells, and zero-width terminal
literals remain separate types. That last separation matters: putting a
literal escape sequence in a rectangular cell grid falsely charges it one
column and makes it overwrite visible state. ABI conversion remains
locale-aware and isolated; Rust consumers get a stable semantic model named
for the data it carries rather than for a C transport it may never use.
