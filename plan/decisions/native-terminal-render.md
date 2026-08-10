---
id [dec:libedit:native-terminal-render]
epitome "Native rendering anchors a bounded physical region, emits a typed frame through one safe writer, and commits display state only after a successful flush."
state @decided
category @property
scope {
    elements ([arch:libedit:core] [arch:libedit:terminal-caps] [arch:libedit:c-abi])
    rules (
        [spec:nshedit:req:core.terminal-render+1]
        [spec:nshedit:req:core.incremental-render+3]
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
        option "Pass editor-local frame rows to the terminal's screen-absolute cursor-address capability."
        rejected_because "The editor starts wherever the host's cursor currently sits, not at physical row zero. Treating its local origin as screen row zero overwrites unrelated terminal content and makes local damage recovery clear the host's screen."
    }
    {
        option "Install a process-global terminfo entry and output destination."
        rejected_because "Two editors would share mutable capability variables and output routing, recreating the C mutex/callback design and preventing safe independent sessions."
    }
)
consequences {
    accepted (
        "Prompt owns ordered PromptPart values. Logical Text participates in layout; TerminalLiteral owns explicit zero-width bytes and is never stored in the rectangular Screen. PromptEffect returns this typed Prompt rather than an ambiguous Text."
        "Each laid-out row carries the ordered literal state active at its origin. Emitting an independent row or suffix replays that row prefix and any literals before the changed column, so moved cursors do not inherit unrelated attributes and multiline incremental output matches full-frame output without interpreting opaque prompt bytes."
        "ScreenGlyph anchors one printable scalar sequence and any following combining scalars. ScreenCell represents Blank, Glyph, Continuation, or Padding, so each value has physical-column semantics. Raw bytes, non-scalar wide values, and unprintable scalars render as visible owned escapes."
        "Editor owns the only native renderer state. TerminalProfile owns the selected terminfo bytes, semantic BaudRate, padding policy, capability variables, committed Screen, cursor, and row count; no global entry or destination exists."
        "A frame and its complete transition from the committed screen are planned before I/O. write_all and flush use a caller-supplied std::io::Write; screen, cursor, damage, and terminfo-variable state commit together only after both succeed. A failed write leaves the previous state committed so the next plan can repair from a conservative damage marker."
        "The renderer anchors the host's current terminal line with owned save/restore capabilities and tracks the high-water extent of rows it reserves or draws. Multiline transitions use relative vertical motion within that region; damage, resize, and profile reconfiguration preserve the saved origin and erase only the tracked rows, never the whole terminal. A downgraded profile that cannot address an owned multiline region fails without emitting output or abandoning that region."
        "Accepted-line and end-of-input finalization restore the saved origin, descend to the region's last owned row, and emit a line feed before releasing the region. Optional visible EOF echo bytes and any row they newly occupy are part of the same planned write. A failed write retains the committed screen and extent; if row reservation may have partially replaced the saved anchor, that origin becomes unavailable until reconfiguration."
        "TerminalMode names Cooked, Editing, and Quoted states. TerminalControl transitions through that enum, and Editor publishes a new committed mode only after the controller succeeds."
        "The renderer performs deterministic incremental row differencing with owned terminfo or explicit ANSI capabilities. A one-line terminal uses carriage return, backspace, forward text, and explicit erasure; it receives a typed error for an impossible multiline position instead of guessed escape bytes."
        "The native renderer uses Unicode terminal-width data for scalar layout. Locale-specific narrow and wide C behaviour remains an ABI conversion and conformance responsibility rather than global native renderer state."
        "The transliterated terminal, tty, prompt, and refresh engine is absent. The ABI adapter projects C-visible configuration into the native renderer without retaining a compatibility renderer."
    )
    deferred (
        "Insert/delete-character cost models beyond suffix rewriting when real terminal profiles demonstrate a byte or latency benefit."
        "Additional movement fallbacks for terminals lacking the complete saved-origin capability set but providing enough alternative relative motion."
        "A native policy knob for East Asian ambiguous-width characters if a real consumer needs one."
    )
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:text-and-screen-model] [dec:libedit:editor-session-ownership] [dec:libedit:effect-driven-hooks] [dec:libedit:terminal-caps-via-term-crate])
    related_to ([dec:libedit:conformance-policy] [dec:libedit:opaque-abi-adapter] [dec:libedit:platform-layer] [dec:libedit:lint-policy])
}
codifies (
    [spec:nshedit:req:core.terminal-render+1]
    [spec:nshedit:req:core.incremental-render+3]
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
glyphs, while the screen contains only physical cells. Rows carry the ordered
literal prefix needed to reconstruct terminal state at their origin; a partial
row emission also replays literals before its first changed column. Its local
row zero is anchored at the host's current terminal line, not fabricated as
physical screen row zero, and the renderer owns only the bounded row extent it
has reserved. Capability expansion, padding, writing, and flushing happen
against temporary output and cloned terminfo variables. Only success replaces
the committed image and variables.

The first implementation deliberately established state, ownership, width,
and failure semantics with a correctness-first full redraw. PTY conformance
then demonstrated that redraw policy is observable API behaviour, not merely
an optimization: it changed every emitted byte and made reaction boundaries
timing-sensitive. The same typed frame now feeds an incremental transition
planner, so compatibility improves without introducing another screen model
or weakening transactional commit.
