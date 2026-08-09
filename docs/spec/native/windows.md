# Native Windows support

The Windows product is the Rust-native editor used by nsh. The libedit C ABI
is a POSIX compatibility surface and is not part of this target.

## Build boundary

> [spec:nshedit:req:workspace.windows-native-build]
> The `nshedit`, `nshedit-plat`, and `nshterm` crates MUST compile for the
> Windows MSVC target without compiling `nshedit-abi`, POSIX termios, POSIX
> signals, or passwd representations. Linux-hosted validation MUST use
> `cargo xwin` for MSVC and SHOULD also check the supported GNU Windows target.

## Console lifecycle and output

> [spec:nshedit:req:platform.windows-console]
> The Windows platform boundary MUST classify input and output handles
> independently as consoles or streams. Console activation MUST capture each
> original mode exactly once, enable character-at-a-time input and VT output,
> expose terminal size, and restore the captured modes idempotently after
> normal completion, errors, and unwinding. Stream handles MUST be read or
> written without invoking console-mode APIs. Rendering MUST use the native
> editor's existing ANSI profile and transactional VT renderer rather than
> Win32 screen-buffer mutation.

## Input normalization

> [spec:nshedit:req:platform.windows-input]
> A real Windows console input handle MUST decode key-down records, repeat
> counts, UTF-16 surrogate pairs, navigation keys, Home, End, Delete,
> Backspace, Ctrl, Alt, Shift, resize, interrupt, and end-of-input conditions
> into owned values accepted by the native read protocol. Key-up records and
> raw Windows virtual-key codes MUST NOT reach the editor. Pipe, ConPTY, SSH,
> and redirected input MUST retain the existing incremental byte-stream path,
> including input split at arbitrary read boundaries.

## Native editor integration

> [spec:nshedit:req:core.windows-session]
> A Windows native session MUST integrate through `TerminalControl`,
> `ReadEffect` and `ReadOutcome`, `SessionIo`, semantic signals and resize,
> and `TerminalProfile::ansi()`. It MUST NOT add a parallel editor event
> model, renderer, or ConPTY production backend. Redirecting input, display
> output, or diagnostics independently MUST preserve the correct behavior of
> the other two streams.

## Acceptance

> [spec:nshedit:req:workspace.windows-acceptance]
> Windows-hosted acceptance MUST exercise the native editor and nsh through
> real console input and through a ConPTY-hosted byte stream. It MUST cover
> insertion, navigation, Home, End, Delete, Backspace, history, completion,
> Unicode outside the BMP, resize, interrupt, EOF, redirected input,
> redirected output, ordinary-error restoration, and unwinding restoration.
> The workspace's Unix tests MUST remain unchanged and passing.
