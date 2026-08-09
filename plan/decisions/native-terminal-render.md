---
id [dec:libedit:native-terminal-render]
epitome "Native rendering builds a typed frame, emits it through one safe writer, and commits display state only after a successful flush."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:terminal-caps] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.terminal-render+1]
        [spec:nshedit:req:core.incremental-render]
        [spec:nshedit:req:core.text-screen-model]
        [spec:nshedit:req:core.rust-io+1]
        [spec:nshedit:req:core.effect-hooks]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Wrap ElTerminalT, ElTtyT, ElPromptT, and ElRefreshT behind safe methods."
        rejected_because "Their integer flags, public parallel buffers, encoded speed_t, foreign prompt callbacks, and sentinel cells would remain the renderer's actual invariants. A safe facade would preserve the translated engine rather than replace it."
    }
    {
        option "Store zero-width prompt escapes as ScreenCell values."
        rejected_because "A rectangular screen slot denotes a physical column. Giving a zero-width byte sequence a slot charges it width, overwrites visible state, and cannot represent a literal adjacent to a glyph at the same boundary."
    }
    {
        option "Mutate the remembered screen while writing each terminal operation."
        rejected_because "A partial or failed write would make the model claim bytes the terminal never received. Planning the complete incremental transition first and committing only after write_all plus flush keeps recovery deterministic."
    }
    {
        option "Install a process-global terminfo entry and output destination."
        rejected_because "Two editors would share mutable capability variables and output routing, recreating the C mutex/callback design and preventing safe independent sessions."
    }
)
consequences {
    accepted (
        "Prompt owns ordered PromptPart values. Logical Text participates in layout; TerminalLiteral owns explicit zero-width bytes and is never stored in the rectangular Screen. PromptEffect returns this typed Prompt rather than an ambiguous Text."
        "ScreenGlyph anchors one printable scalar sequence and any following combining scalars. ScreenCell represents Blank, Glyph, Continuation, or Padding, so each value has physical-column semantics. Raw bytes, non-scalar wide values, and unprintable scalars render as visible owned escapes."
        "Editor owns the only native renderer state. TerminalProfile owns the selected terminfo bytes, semantic BaudRate, padding policy, capability variables, committed Screen, cursor, and row count; no global entry or destination exists."
        "A frame and its complete transition from the committed screen are planned before I/O. write_all and flush use a caller-supplied std::io::Write; screen, cursor, damage, and terminfo-variable state commit together only after both succeed. A failed write leaves the previous state committed so the next plan can repair from a conservative damage marker."
        "TerminalMode names Cooked, Editing, and Quoted states. TerminalControl transitions through that enum, and Editor publishes a new committed mode only after the controller succeeds."
        "The renderer performs deterministic incremental row differencing with owned terminfo or explicit ANSI capabilities. A one-line terminal uses carriage return, backspace, forward text, and explicit erasure; it receives a typed error for an impossible multiline position instead of guessed escape bytes."
        "The native renderer uses Unicode terminal-width data for scalar layout. Locale-specific narrow and wide C behaviour remains an ABI conversion and conformance responsibility rather than global native renderer state."
        "The transliterated terminal, tty, prompt, and refresh engine is absent. The ABI adapter projects C-visible configuration into the native renderer without retaining a compatibility renderer."
    )
    deferred (
        "Insert/delete-character cost models beyond suffix rewriting when real terminal profiles demonstrate a byte or latency benefit."
        "Additional movement fallbacks for terminals lacking cursor_address but providing relative cursor capabilities."
        "A native policy knob for East Asian ambiguous-width characters if a real consumer needs one."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:text-and-screen-model] [dec:libedit:editor-session-ownership] [dec:libedit:effect-driven-hooks] [dec:libedit:terminal-caps-via-term-crate])
    related_to ([dec:libedit:conformance-policy] [dec:libedit:opaque-abi-adapter] [dec:libedit:platform-layer] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:core.terminal-render+1]
    [spec:nshedit:req:core.incremental-render]
    [spec:nshedit:req:core.text-screen-model]
    [spec:nshedit:req:core.rust-io+1]
)
---

## Rationale

Rendering crosses three representations that must not collapse into one:
logical input, a sequence of terminal operations, and the physical screen
image believed to have been committed. The retired engine combined them in
wide-integer buffers and sent output while mutating those buffers, which
makes sentinel values, callback routing, and partial writes part of the state
model.

The native renderer instead constructs an owned frame from the editor's
checked line and cursor. Prompt literals remain zero-width operations between
glyphs, while the screen contains only physical cells. Capability expansion,
padding, writing, and flushing happen against temporary output and cloned
terminfo variables. Only success replaces the committed image and variables.

The first implementation deliberately established state, ownership, width,
and failure semantics with a correctness-first full redraw. PTY conformance
then demonstrated that redraw policy is observable API behaviour, not merely
an optimization: it changed every emitted byte and made reaction boundaries
timing-sensitive. The same typed frame now feeds an incremental transition
planner, so compatibility improves without introducing another screen model
or weakening transactional commit.
