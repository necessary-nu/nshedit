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

> [spec:nshedit:req:abi.behavioural-conformance]
> For every defined input covered by the `libedit` corpus, the ABI MUST match
> the reference library's return values, errno effects, emitted bytes,
> stream effects, callback order, pointer validity, and state transitions.
> Undefined C inputs MUST receive a documented safe result rather than an
> attempt to reproduce memory unsafety.

## Native domain

> [spec:nshedit:req:core.typed-domain+1]
> Native configuration, modes, commands, outcomes, and failures MUST use
> Rust enums, newtypes, structs, `Option`, and `Result`. The native domain
> model MUST NOT use integer operation codes, bit-packed flag words, errno
> protocols, or sentinel values to represent domain state.

> [spec:nshedit:req:core.text-screen-model]
> Logical input text and rendered screen cells MUST be distinct types.
> Unicode scalar values, undecodable bytes, and non-scalar wide values that
> the compatibility boundary must preserve MUST have explicit variants;
> rendered continuation and padding cells MUST NOT be encoded in spare bits
> of a character integer.

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

> [spec:nshedit:req:core.history+1]
> Native history storage and traversal MUST use owned `Text` records, typed
> identifiers, explicit traversal cursors, and safe Rust interfaces. The
> native store MUST contain no C varargs dispatch, raw callback handles, event
> records, or narrow/wide conversion storage and MUST be the only history
> implementation reachable from the native `Editor`. Until the ABI adapter
> replaces the transliterated compatibility path, C-shaped history machinery
> MAY remain only in those existing compatibility modules;
> `core.no-compat-internals` governs its final removal.

> [spec:nshedit:req:core.token-completion]
> Native tokenization and completion MUST return owned or explicitly borrowed
> typed results. Null-terminated pointer arrays, thread-local return storage,
> C generators, and integer status protocols MUST remain in the ABI adapter.

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

> [spec:nshedit:req:core.read-driver]
> Input preparation, decoding, key dispatch, signal transitions, and editing
> completion MUST form a resumable native driver over the typed domain and
> effect interfaces. All successful, EOF, interrupt, and error exits MUST
> leave terminal and editor state valid for finish or Drop.

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

> [spec:nshedit:req:workspace.lint-policy]
> First-party crates MUST contain no blanket lint exemptions. A constraint
> imposed by an external ABI or generated format MAY use the narrowest
> `expect` attribute with a reason; `allow(dead_code)`, `allow(unused_*)`, and
> `allow(nonstandard_style)` are forbidden, and an unfulfilled expectation
> MUST fail lint checks.
