# Rust-native core and C compatibility boundary

These rules describe the replacement architecture. Detailed compatibility
semantics remain in the `libedit` corpus under `docs/spec/port`; this corpus
states where those semantics may live and what the native Rust API must be.

## Compatibility adapter

> [spec:nshedit:req:abi.complete-surface+1]
> Every function, variable, operation code, callback, and stream behaviour
> declared by the shipped compatibility headers MUST implement the behaviour
> assigned by the detailed `libedit` corpus. An unsupported return or no-op is
> permitted only where that corpus defines it as reference behaviour; a
> port-private stand-in, unconditional error where the reference performs
> work, or documented "cannot yet" path does not satisfy this rule.

> [spec:nshedit:req:abi.opaque-owner]
> Each incomplete C handle type MUST be backed by an ABI-owned allocation
> containing the Rust-domain object and all state needed solely for C
> representation, callback, conversion, errno, and pointer-lifetime
> semantics. The ABI MUST NOT expose or cast directly to the core object's
> representation.

> [spec:nshedit:req:abi.surface-stability]
> The generated compatibility headers, exported symbol set, symbol versions
> and aliases, completed C record layouts, and opaque handle spellings MUST
> remain drop-in compatible with the supported libedit and readline ABIs.
> Rust identifier spelling is not part of this contract.

> [spec:nshedit:req:abi.behavioural-conformance+1]
> The detailed `libedit` corpus and maintained Rust implementation MUST remain
> mutually consistent. For every defined input the corpus covers, the ABI MUST
> preserve its specified return values, errno effects, emitted bytes, stream
> effects, callback order, pointer validity, and state transitions. Undefined C
> inputs MUST receive a documented safe result rather than an attempt to
> reproduce memory unsafety.

> [spec:nshedit:req:abi.termcap-view]
> The in-workspace terminal database layer MUST expose legacy termcap names as
> an explicit projection of typed terminfo data. The projection MUST preserve
> defined provider compatibility semantics that are not a simple name lookup,
> including an `me` reset that does not unexpectedly leave the caller's
> alternate character set, and MUST NOT require process-global terminal or
> output state.

> [spec:nshedit:req:abi.terminal-session]
> Each C editor handle MUST own one compatibility view of the selected terminal
> name, boolean, numeric, and stable C-string capability values. Database
> geometry MUST be overridden only by a non-zero kernel window size; baud-rate
> padding and capability mutation MUST reconfigure the native terminal profile
> through typed Rust values. Native rendering reached from the C ABI MUST write
> through the caller's `FILE *` buffering without placing that pointer in the
> native editor.

> [spec:nshedit:req:abi.tty-modes]
> The compatibility adapter MUST retain cooked, editing, and quoted terminal
> snapshots plus independent named flag and control-character overrides for
> each mode. `setty` mutation and listing MUST match the detailed tty rules, and
> a mutation of the active mode MUST be applied through the safe platform
> boundary while inactive-mode mutations remain deferred until activation.

> [spec:nshedit:req:abi.terminal-controls+1]
> Terminal capability and tty commands reached through `el_get`, `el_set`,
> `el_parse`, and `el_source` MUST perform the query, mutation, byte emission,
> listing, and diagnostic behaviour assigned by the detailed terminal and tty
> rules. `gettc`, `settc`, `telltc`, `echotc`, and `setty` MUST share one
> ABI-owned compatibility view of the native terminal profile and platform tty
> state by composing `abi.termcap-view`, `abi.terminal-session`, and
> `abi.tty-modes`; unconditional success is forbidden where the reference
> performs work.

> [spec:nshedit:req:abi.bindings]
> The compatibility `bind` command MUST implement the defined editing-map,
> alternate-map, string-macro, terminal-key, removal, listing, and query forms,
> and MUST resolve the complete built-in and registered user-command inventory
> required by the detailed map and command rules. A dispatched user command
> MUST receive the invoking character and the same observable editor state as
> the reference callback.

> [spec:nshedit:req:abi.binding-dispatch]
> Every built-in command in the compatibility inventory, whether reached from a
> default map or a caller-installed binding, MUST execute the behaviour assigned
> by the detailed command rules with the reference invoking unit, repeat count,
> editing mode, and downstream effects. A built-in MUST NOT be encoded as a
> registered user command, sent through foreign callback lookup, or collapsed to
> an unconditional beep unless the detailed rule assigns that result.

> [spec:nshedit:req:abi.history-effects+1]
> An installed narrow or wide history callback MUST service editrc history
> commands and traversal with the arguments, ordering, return translation, and
> output assigned by the detailed history and read rules. The compatibility
> adapter MUST acknowledge the native accepted-line record effect without an
> implicit `H_ENTER`, because the C API assigns recording to the caller after
> `el_gets` returns. Foreign callback storage and invocation remain ABI-owned;
> the native editor MUST cross this boundary only through typed history effects.

> [spec:nshedit:req:abi.signal-lifecycle]
> Signal-enabled reads MUST install, observe, propagate, and restore the signal
> behaviour assigned by the detailed signal and read rules, while signal-disabled
> reads MUST leave caller signal policy untouched. Platform handler ownership
> MUST be RAII-safe, and the native driver MUST represent delivery and resume as
> typed state rather than silently accepting every signal effect.

> [spec:nshedit:req:abi.observational-coverage+1]
> Compatibility tests for an operation that changes state, emits bytes, or
> invokes callbacks MUST observe that effect and a later dependent operation;
> matching only the immediate return code is insufficient. The maintained
> compatibility suite MUST cover the terminal-control, binding, history-effect,
> and signal-lifecycle paths through both direct operation codes and editrc
> entry points where the shipped headers expose both.

> [spec:nshedit:req:abi.rust-internals]
> Exported ABI wrappers MAY parse C scalars, varargs, callbacks, pointers,
> operation codes, out-parameters, and status conventions. Once parsed, private
> Rust implementation MUST use typed operations, values, and `Result`; it MUST
> NOT call this crate's exported symbols through `extern` declarations or
> `link_name`, carry varargs or raw operation codes through private dispatch, or
> retain C out-parameters as its internal result protocol. Required C spelling
> MUST be supplied by export and header-generation metadata rather than by
> constraining private Rust identifiers.

> [spec:nshedit:req:abi.typed-history]
> Private history operations MUST couple each operation with its valid payload
> in a typed value and return a typed reply or error. Only the exported boundary
> MAY decode `H_*` integers, read variadic arguments, publish `HistEvent`
> records, encode C status integers, or transfer caller-owned pointers. Built-in
> and foreign callback backends MUST implement the same typed operation model.

> [spec:nshedit:req:abi.typed-completion]
> Private completion MUST accept one typed request containing provider and
> policy choices and return one typed report containing edits, listing state,
> and observable positions. Suffixes and candidates MUST be owned values;
> private completion MUST NOT use C flags, C status integers, out-parameters,
> leaked interning, or thread-local storage introduced solely to coerce a C
> callback into a Rust function-pointer signature. Exported wrappers alone MAY
> adapt scoped, reentrant C callbacks and encode their required outputs.

> [spec:nshedit:req:abi.typed-session]
> Each ABI-owned editor and readline runtime MUST organize policy, prompt,
> encoding, callbacks, terminal ownership, and initialization failure as typed
> components rather than boolean property bags, indexed side channels, or
> erased `Option` failures. Private process-global state required by readline
> MUST have one explicit owner, and no lock or dynamic borrow MAY remain held
> across a foreign callback. Narrow and wide callbacks MUST be invoked through
> their declared function-pointer types without transmutation.

## Native domain

> [spec:nshedit:req:core.typed-domain+1]
> Native configuration, modes, commands, outcomes, and failures MUST use
> Rust enums, newtypes, structs, `Option`, and `Result`. The native domain
> model MUST NOT use integer operation codes, bit-packed flag words, errno
> protocols, or sentinel values to represent domain state.

> [spec:nshedit:req:core.text-screen-model+1]
> Logical input text and rendered screen cells MUST be distinct types.
> Unicode scalar values, undecodable bytes, and opaque non-Unicode code points
> that a host boundary must preserve MUST have explicit, boundary-neutral
> variants; the safe core MUST NOT define them in terms of `wchar_t`, wide C
> strings, or compatibility transport names. Rendered continuation and padding
> cells MUST NOT be encoded in spare bits of a character integer.

> [spec:nshedit:req:core.raii-lifecycle]
> A native editor session MUST restore terminal state exactly once when it
> is explicitly finished or dropped. Explicit finish MUST report restoration
> failure, while Drop MUST perform best-effort restoration without panicking;
> partially constructed sessions and repeated internal cleanup MUST be safe.

> [spec:nshedit:req:core.rust-io+1]
> Native editor input, output, flushing, terminal control, and descriptor
> access, including every replacement concern, MUST cross safe Rust
> interfaces. `FILE *`, C stream ownership, raw descriptor ownership transfer,
> and foreign I/O callbacks MUST NOT be part of the native editor model.

> [spec:nshedit:req:core.effect-hooks]
> Any host-controlled prompt, read, history, alias, resize, completion, or
> user-command operation that can call foreign code MUST suspend as a typed
> effect and resume from a typed response after the core borrow has ended.
> The core MUST NOT store or invoke an `extern "C"` callback.

> [spec:nshedit:req:core.public-surface]
> Every public `nshedit` operation MUST be callable without unsafe code and
> every public type MUST express Rust-domain semantics. Internal editor state
> and modules MUST be private; public raw pointers, C scalar aliases, C
> callback types, compatibility buffers, and ABI record types are forbidden.

## Replacement concerns

> [spec:nshedit:req:core.line-commands]
> Line storage, cursor movement, undo, kill/yank state, keymaps, search, and
> built-in editor commands MUST operate on private typed Rust state while
> retaining the behaviours required by the corresponding detailed libedit
> rules at the ABI adapter.

> [spec:nshedit:req:core.command-sequences]
> Repeat arguments, quoted and meta next-unit input, Vi operator-motion
> composition, character search and repetition, replacement, substitution, and
> redo MUST be native typed continuations over semantic actions, checked text
> boundaries, and bounded replay. The core MUST NOT represent these protocols
> with C command numbers, operator bit masks, pointer anchors, or compatibility
> callback names.

> [spec:nshedit:req:core.command-effects]
> Commands that require history search, history line or word selection, alias
> expansion, editor-command input, or external history editing MUST suspend as
> owned typed effects and resume with operation-specific responses after all
> editor borrows have ended. Cancellation, unavailability, and callback failure
> MUST remain typed outcomes until the compatibility adapter translates them.

> [spec:nshedit:req:core.history+1]
> Native history storage and traversal MUST use owned `Text` records, typed
> identifiers, explicit traversal cursors, and safe Rust interfaces. The
> native store MUST contain no C varargs dispatch, raw callback handles, event
> records, or narrow/wide conversion storage and MUST be the only history
> implementation reachable from the native `Editor`. Until the ABI adapter
> replaces the transliterated compatibility path, C-shaped history machinery
> MAY remain only in those existing compatibility modules;
> `core.no-compat-internals` governs its final removal.

> [spec:nshedit:req:core.token-completion+1]
> Native tokenization and completion MUST use owned `Text`, checked source
> spans and cursor positions, and explicit token, continuation, query,
> candidate, edit, and outcome types. Completion across the host-effect
> boundary MUST own its request and response, and applying a response MUST
> reject a stale line snapshot before changing editor state. Null-terminated
> pointer arrays, thread-local return storage, C generators, and integer status
> protocols MUST NOT be reachable from the native `Editor`. Until the ABI
> adapter replaces the transliterated compatibility path, those C-shaped
> mechanisms MAY remain only in the existing compatibility modules;
> `core.no-compat-internals` governs their final removal.

> [spec:nshedit:req:core.terminal-render+1]
> The native terminal profile, tty mode, prompts, render plan, and committed
> screen image MUST use private typed state. Rendering MUST write only through
> a caller-supplied safe Rust writer and MUST NOT require a global destination,
> a `FILE *`, foreign putc callback, C sentinel encoding, or zero-width terminal
> literal disguised as a physical screen cell. These native types MUST be the
> only terminal-render implementation reachable from the native `Editor`.
> Until the ABI adapter replaces the transliterated compatibility path,
> C-shaped terminal, tty, prompt, and refresh machinery MAY remain only in
> those existing compatibility modules; `core.no-compat-internals` governs
> its final removal.

> [spec:nshedit:req:core.incremental-render+1]
> Native rendering MUST plan deterministic terminal operations from the
> committed typed screen and cursor to the next complete frame. Unchanged
> cells MUST NOT force a complete redraw. The committed image MUST belong to
> an explicit physical editor region anchored at the current terminal line,
> with an extent covering every row the renderer has reserved or drawn.
> Multiline movement MUST be relative to that origin; an editor-local frame row
> MUST NOT be used as a screen-absolute cursor coordinate. Damage and resize
> recovery MUST erase only rows inside the tracked extent and MUST NOT clear the
> whole terminal for local invalidation. Reconfiguring the terminal profile MUST
> preserve an established region; a profile that cannot address an owned
> multiline region MUST return a typed error without abandoning or overwriting
> it. The planner MUST use only capabilities
> present in the selected profile, MUST support a one-line terminal through
> carriage return, backspace, forward text, and explicit erasure, and MUST
> commit the new screen, cursor, capability variables, origin, extent, and
> damage state only after the corresponding byte plan is written and flushed
> successfully.

> [spec:nshedit:req:core.read-driver+1]
> Input preparation, decoding, key dispatch, signal transitions, and editing
> completion MUST form a resumable native driver over the typed domain and
> effect interfaces. Driver state MUST have exactly one continuation authority:
> a continuation variant carries its request, legal response, ownership token,
> and next transition. Parallel effect-kind, phase, step, or command-host-work
> protocols that can disagree with that continuation are forbidden. All
> successful, EOF, interrupt, and error exits MUST leave terminal and editor
> state valid for finish or Drop.

> [spec:nshedit:req:platform.typed-boundary]
> Public safe platform operations MUST use borrowed descriptors, typed actions
> and flags, and `io::Result` or an equally descriptive typed error. Raw integer
> descriptors, transcribed layouts and constants, boolean syscall results,
> fabricated lifetimes, and libc callback tables MUST remain private. Host
> customization such as user lookup MUST be owned per editor session or effect,
> never installed through process-global test or override hooks.

> [spec:nshedit:req:terminal.typed-api]
> The terminal-capability crate MUST expose typed format, name, and environment
> policies; capability storage MUST remain private behind focused accessors;
> parser and discovery failures MUST preserve their error source. Boolean mode
> arguments, public mutable representation maps, and swallowed I/O failures are
> forbidden in its maintained public API.

> [spec:nshedit:req:core.no-compat-internals]
> Once replacement concerns are active, the transliterated C-shaped core,
> file-for-file module facade, legacy conversion buffers, core errno storage,
> raw compatibility callbacks, and ABI-only fields MUST be deleted rather
> than retained as a second implementation or compatibility facade.

> [spec:nshedit:req:core.native-consumer]
> The repository MUST contain a compiling native Rust consumer that performs
> real line editing, prompt delivery, history integration, and teardown using
> only the safe public API and no C-shaped compatibility types or unsafe code.

> [spec:nshedit:req:core.unsafe-free]
> The `nshedit` crate MUST forbid unsafe code. Required syscall unsafety MUST
> be encapsulated by `nshedit-plat`; required C representation and foreign-call
> unsafety MUST be encapsulated by `nshedit-abi`.

## Workspace lint discipline

> [spec:nshedit:req:workspace.no-legacy-allows]
> First-party source MUST NOT suppress dead code, unused variables, missing
> safety documentation, or Rust naming conventions at crate or module scope.
> Dead items MUST be deleted, made reachable, moved to their owning boundary,
> or accurately conditionalized; C spelling MUST be preserved with export or
> header-generation metadata rather than Rust identifier spelling.

> [spec:nshedit:req:workspace.lint-policy+1]
> First-party source MUST contain no lint-suppression attribute, including
> `allow`, `expect`, or a conditional attribute that enables either. External
> ABI and generated-format constraints MUST be represented so the lint does not
> arise, or isolated outside first-party checked source; they do not justify a
> suppression. Dead code, unused items, missing safety documentation, naming
> violations, and structural lints selected by the workspace MUST fail checks.

> [spec:nshedit:req:workspace.self-contained]
> Every workspace dependency and build input MUST resolve from the repository,
> the configured package registry, or an explicitly declared repository source.
> A clean checkout MUST build, test, and package without a sibling personal
> checkout or an absolute/path dependency outside the repository.

> [spec:nshedit:req:workspace.semantic-naming]
> Maintained Rust identifiers, modules, examples, and public formats MUST be
> named for their responsibility, data, or stable identity. Relative migration
> labels such as `native`, `legacy`, `compatibility`, or `translated` MUST NOT
> identify the sole implementation or an unnamed format. Where two external
> protocols truly coexist, their concrete protocol or version names MUST make
> the distinction. Required C symbol spelling belongs only in export and
> header-generation metadata.
