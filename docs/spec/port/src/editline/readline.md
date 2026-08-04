# src/editline/readline.h

> [spec:libedit:def:readline.completion-matches-fn]
> char **completion_matches(/* const */ char *, rl_compentry_func_t *)

> [spec:libedit:sem:readline.completion-matches-fn]
> The pre-4.2 GNU readline entry point that turns a "generator"
> function into a completion match list. It is declared here purely for
> source compatibility with programs written against old readline; the
> definition does NOT live in `readline.c`. The symbol a consumer links
> against is the exported `completion_matches` in `filecomplete.c`,
> whose step-by-step behaviour is the rule
> `[spec:libedit:sem:filecomplete.completion-matches-fn]`. `readline.c`
> only avoids a prototype clash by `#define`-ing the name to
> `xxx_completion_matches` around its `#include` of this header and
> then `#undef`-ing it, so no second definition exists anywhere. A port
> must export exactly one symbol of this name, and it must be the same
> function libedit itself calls from `fn_complete2` — TAB completion
> reaches it whenever no `rl_attempted_completion_function` produced
> matches, so its behaviour is observable without the consumer ever
> calling it directly.
>
> Contract seen by a C caller:
>
> 1. `text` is the partial word to complete. This header spells it
>    `char *` while the definition takes `const char *`; the
>    `/* const */` comment records that the string is only ever read.
>    The mismatched prototypes are formally incompatible across
>    translation units but harmless at the ABI level — a pointer is a
>    pointer. The practical consequence is that C++ consumers (this
>    block is inside `extern "C"`) and strict C consumers must cast
>    away const at the call site. In Rust the parameter is
>    `*mut c_char`; treat it as borrowed, read-only, and never retained
>    past the call. `text` is not inspected by `completion_matches`
>    itself — it is forwarded unchanged to every generator call — so a
>    NULL `text` is not rejected here and whether it faults depends
>    entirely on the generator. (This differs from libedit's
>    `rl_completion_matches`, which dereferences `str` itself and so
>    faults on NULL.)
> 2. `genfunc` must be non-NULL; it is called unconditionally and is
>    not checked. It is invoked as `genfunc(text, state)` with `state`
>    equal to the number of matches already collected: 0 on the first
>    call, then 1, 2, 3, … . `state == 0` is the generator's "start a
>    fresh scan for this text" signal; every later call means "next
>    match for the same text". The generator ends the scan by returning
>    NULL; a generator that never returns NULL loops forever and grows
>    the array without bound. Each non-NULL return transfers ownership
>    of a heap string to `completion_matches` and thence to the caller.
> 3. Return value: NULL means either "the generator produced no matches
>    at all" or "an allocation failed" — the two are indistinguishable
>    and `errno` is not set. Otherwise the return is a NULL-terminated
>    `char **` in which element 0 is a freshly allocated copy of the
>    longest common byte prefix of the matches, and elements 1..n are
>    the generator's strings in generation order. The array is
>    effectively 1-based for matches; element 0 is not a match. With
>    exactly one match, element 0 is a byte-identical copy of element
>    1 — that equality is how `fn_complete2` recognises a unique
>    completion. When the matches share no leading byte, element 0 is
>    the empty string.
> 4. Ownership: on success the caller owns elements 0..n and the array,
>    and must free each element and then the array. libedit's allocator
>    macros are plain `malloc`/`calloc`/`realloc`/`free`, so `free()` is
>    correct and the Rust port must keep the returned blocks freeable by
>    the platform `free`. On both failure paths libedit frees only the
>    array and leaks every match string collected so far; that leak is
>    part of the observed behaviour, not something a port must fix, but
>    it must not turn into a double free.
>
> Divergences a readline-compatible consumer will hit:
>
> - In GNU readline from 4.2 onward, `completion_matches()` is a thin
>   deprecated alias that simply calls `rl_completion_matches()`, so the
>   two names behave identically. In libedit they are two SEPARATE
>   implementations with different results, and both must be ported
>   as-is. `rl_completion_matches` (see
>   [spec:libedit:sem:readline.rl-completion-matches-fn]) sorts elements
>   1..n with `strcmp`, computes element 0 as the minimum common prefix
>   over adjacent pairs, and — when that prefix is empty and `text` is
>   non-empty — substitutes a copy of `text` for element 0 so the line
>   is not clobbered. `completion_matches` does none of that: no
>   sorting, generation order preserved, and an empty element 0 stays
>   empty. Swapping one name for the other changes both the order of the
>   displayed list and what gets inserted into the line.
> - No de-duplication, no case folding, no locale awareness. The prefix
>   is computed byte-wise and can be cut in the middle of a multibyte
>   character. The `rl_ignore_completion_duplicates`,
>   `rl_sort_completion_matches` and `rl_completion_case_fold`-style
>   knobs a GNU consumer would reach for either do not exist here or sit
>   in this header's "not implemented" block and are never read.
> - The "Display all N possibilities?" query, the columnar listing and
>   the append character are not this function's business; they belong
>   to `fn_complete2`/`fn_display_match_list`. This call only builds a
>   list.
>
> Thread-safety and reentrancy: the function keeps no state of its own
> and is reentrant in itself, but libedit's own generators are not —
> `fn_filename_completion_function` (and therefore
> `filename_completion_function`) holds a static open `DIR *` and static
> scan state, so two interleaved scans through the same generator
> corrupt each other. There is no locking anywhere; the readline layer
> assumes a single thread.
>
> Linkage note for the port: this symbol has default ELF visibility and
> libedit's internal call site in `fn_complete2` is not protected
> against interposition, so an application that defines its own
> `completion_matches` — which old readline programs sometimes do —
> preempts libedit's and is then also what libedit's internal TAB
> completion calls. A Rust ABI crate that hides the symbol or calls a
> private copy internally would silently change that behaviour.

> [spec:libedit:def:readline.hist-entry]
> typedef struct _hist_entry

> [spec:libedit:def:readline.histdata-t]
> typedef void *histdata_t

> [spec:libedit:def:readline.history-state]
> typedef struct

> [spec:libedit:def:readline.keymap]
> typedef KEYMAP_ENTRY *Keymap

> [spec:libedit:def:readline.keymap-entry]
> typedef struct _keymap_entry

> [spec:libedit:def:readline.keymap-entry-array-keymap-size]
> typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE]

> [spec:libedit:def:readline.rl-command-func-t-int-int]
> typedef int rl_command_func_t(int, int)

> [spec:libedit:def:readline.rl-compdisp-func-t-char-int-int]
> typedef void rl_compdisp_func_t(char **, int, int)

> [spec:libedit:def:readline.rl-compentry-func-t-const-char-int]
> typedef char *rl_compentry_func_t(const char *, int)

> [spec:libedit:def:readline.rl-completion-func-t-const-char-int-int]
> typedef char **rl_completion_func_t(const char *, int, int)

> [spec:libedit:def:readline.rl-completion-word-break-hook-fn]
> extern char *(*rl_completion_word_break_hook)(void)

> [spec:libedit:sem:readline.rl-completion-word-break-hook-fn]
> A process-global, application-writable function pointer, of type
> `char *(*)(void)`, initialised to NULL in `readline.c`. In Rust it is
> an exported mutable data symbol —
> `Option<extern "C" fn() -> *mut c_char>` with `None` as the initial
> value — not a function. A consumer sets it so that it can widen the
> set of characters at which libedit stops scanning backwards when it
> decides which word the cursor is sitting in.
>
> Exactly one code path reads it: `rl_complete()`
> ([spec:libedit:sem:readline.rl-complete-fn]), which is both the
> function bound to TAB (`^I`, via `_el_rl_complete`) by
> `rl_initialize` and a function the application may call directly.
> Nothing else in libedit consults it; in particular the native
> `fn_complete`/`fn_complete2` entry points used by `histedit.h`
> consumers never see it.
>
> The sequence inside `rl_complete(ignore, invoking_key)` is:
>
> 1. If the editor/history pair has not been created yet, call
>    `rl_initialize()`.
> 2. If `rl_inhibit_completion` is non-zero, insert `invoking_key` as a
>    literal character and return `CC_REFRESH`. The hook is NOT called
>    on this path.
> 3. If `rl_completion_word_break_hook` is non-NULL, call it with no
>    arguments and keep the returned pointer as `breakchars`. Otherwise
>    `breakchars = rl_basic_word_break_characters`. The pointer is read
>    fresh on every completion; nothing is cached, and the hook is
>    called exactly once per completion attempt.
> 4. Call `_rl_update_pos()`, which refreshes `rl_point`, `rl_end` and
>    the NUL terminator of `rl_line_buffer` from the editor's line.
> 5. Call `fn_complete2` with `word_break` = the wide conversion of
>    `rl_basic_word_break_characters` and `special_prefixes` = the wide
>    conversion of `breakchars`.
>
> What the returned string actually does — and this is the sharp
> divergence from GNU readline — is become the *special prefixes* set,
> not the word-break set. libedit always uses
> `rl_basic_word_break_characters` as the word-break set. Inside
> `find_word_to_complete` the two sets are consulted identically: the
> backward scan from the cursor stops at the first character found in
> EITHER set (`special_prefixes` is additionally NULL-checked). The net
> effect is that the hook can only ADD break characters to the built-in
> set; it can never remove or replace one. GNU readline instead uses the
> returned string *in place of* `rl_completer_word_break_characters` for
> that completion, so a consumer that returns a deliberately narrow set
> such as `" \t\n"` will still see libedit break on `"`, `'`, `` ` ``,
> `\`, `@`, `$`, `>`, `<`, `=`, `;`, `|`, `&`, `{` and `(`. Related: the
> globals `rl_completer_word_break_characters` and `rl_special_prefixes`
> are declared in this header but are read by no code path at all, so
> setting either has no effect whatsoever.
>
> Returning NULL is safe and means "add nothing": `special_prefixes`
> becomes NULL, the wide conversion of NULL is NULL, and
> `find_word_to_complete` skips the check. That coincidentally matches
> GNU readline's documented "return NULL to use the default" semantics.
> Leaving the hook NULL is equally a no-op, since `breakchars` then
> aliases `rl_basic_word_break_characters` and the union of a set with
> itself changes nothing.
>
> Because the hook runs before step 5 reads
> `rl_basic_word_break_characters`, a hook that assigns to that global
> and then returns NULL DOES change the break set for the same
> completion. That is the only way to genuinely replace, rather than
> extend, the set — worth knowing because GNU readline explicitly
> permits the hook to set the word-break globals itself, and this is the
> libedit-shaped equivalent.
>
> Encoding and lifetime of the return value: it is a multibyte C string
> in the process's current locale. `rl_complete` converts it with
> `mbstowcs` into a function-local `static` conversion buffer
> (`sprefix_conv`, distinct from the buffer used for the word-break
> set) which grows on demand and is never freed. If the string is not
> valid in the current locale the conversion returns NULL and the
> characters are silently dropped — no error, no diagnostic, completion
> just proceeds with the default set. An empty string is legal and adds
> nothing. libedit never frees the returned pointer and never stores it
> past the conversion, so a hook that returns freshly allocated memory
> leaks on every TAB; the expected implementation returns a pointer to
> static or otherwise long-lived storage. The pointer must stay valid at
> least until `rl_complete` has converted it, i.e. for the duration of
> the call.
>
> What the hook may assume when it runs: the editor exists (step 1 has
> already run) and `rl_line_buffer` points at libedit's live line
> buffer. It may NOT assume `rl_point` and `rl_end` are current — they
> are refreshed in step 4, *after* the hook returns, so during the hook
> they still hold values from the previous `_rl_update_pos` call, and
> `rl_line_buffer` is not yet NUL-terminated at the current end. GNU
> readline calls its hook with those variables already current, so a
> hook ported straight across that inspects `rl_point` to decide the
> break set will read stale data.
>
> No validation is applied to the value a consumer stores: any non-NULL
> pointer is called, so a dangling or wrongly typed function pointer
> faults or corrupts. There is no locking; the pointer and the static
> conversion buffer are plain process globals, and the readline layer is
> single-threaded throughout. Assigning to the hook while a completion
> is in flight (for instance from another hook) is racy but not
> otherwise special-cased.

> [spec:libedit:def:readline.rl-getc-function-fn]
> extern int (*rl_getc_function)(FILE *)

> [spec:libedit:sem:readline.rl-getc-function-fn]
> A process-global, application-writable function pointer of type
> `int (*)(FILE *)`, initialised to NULL in `readline.c`. In Rust it is
> an exported mutable data symbol —
> `Option<extern "C" fn(*mut FILE) -> c_int>` initialised to `None`. A
> consumer sets it to take over how libedit obtains input characters,
> typically to multiplex the editor with an event loop or to feed the
> editor from something other than a terminal.
>
> It sits under this header's `/* The following is not implemented */`
> banner, but that comment is wrong for this entry and a port must not
> treat it as a no-op: the pointer IS honoured, just far more narrowly
> than in GNU readline.
>
> Installation, and the timing trap: `rl_initialize()`
> ([spec:libedit:sem:readline.rl-initialize-fn]) defaults `rl_instream`
> to `stdin` and `rl_outstream` to `stdout` if unset, creates the
> `EditLine`, and then — only if `rl_getc_function` is non-NULL at that
> instant — installs libedit's wrapper `_getc_function` as the editor's
> character reader (`EL_GETCFN`). The pointer is sampled once, and only
> for non-NULL-ness; its value is re-read on every character, but if it
> was NULL at initialisation the wrapper is never installed and setting
> it later has no effect at all. The only way to take effect afterwards
> is another `rl_initialize()`, which tears down and rebuilds both the
> editor and the history list — losing the history contents, custom
> bindings, prompt and terminal settings. Consumers must therefore set
> this before the first call to `readline()` or any other entry point
> that lazily initialises (`rl_complete`, `rl_read_key`, `rl_bind_key`,
> …). GNU readline consults `rl_getc_function` on every character read
> with no installation step, so it can be swapped in and out at will;
> that is the single biggest behavioural difference here.
>
> Per-character behaviour once installed, via the wrapper spec'd by
> `[spec:libedit:sem:readline.getc-function-fn]`:
>
> 1. libedit calls `(*rl_getc_function)(rl_instream)` — the pointer is
>    dereferenced with NO null check, so storing NULL back into
>    `rl_getc_function` after initialisation turns the next keystroke
>    into a null call. `rl_instream` is re-read from the global on every
>    call, so changing it mid-session changes what the callback
>    receives.
> 2. The `FILE *` is passed through opaquely — the wrapper never
>    dereferences it. A Rust port must hand back exactly the pointer
>    value stored in `rl_instream`, treating it as an untyped token.
>    (Note that `rl_initialize` separately calls `fileno()` and
>    `tcgetattr()` on `rl_instream`, and captures that descriptor for
>    tty control, resizing and readiness checks — so changing
>    `rl_instream` after initialisation redirects the callback's
>    argument but NOT the descriptor libedit uses for terminal work.)
> 3. If the callback returns -1 the wrapper reports "no character" (0)
>    to the read layer. That propagates as EOF: `el_wgets` returns NULL
>    with the count set to -1, and `readline()` returns NULL. `errno` is
>    not preserved and there is no way to distinguish a read error from
>    a clean end of input — the wrapper never returns the read layer's
>    -1 "error" code, so libedit's `read_errno` machinery is bypassed
>    entirely.
> 4. Any other return value `i`, including 0 and negative values other
>    than -1, is stored as `(wchar_t)i` and reported as one character
>    read. Returning 0 to mean "no data yet" therefore inserts a NUL
>    character into the line rather than signalling anything; the
>    callback must block until it has a character or return -1.
>
> Encoding divergence: the returned `int` is cast straight to `wchar_t`
> with no multibyte decoding. libedit's built-in reader reads single
> bytes and assembles them with `mbrtowc`; a custom getc function
> bypasses that completely. So the callback must return whole wide
> characters (code points), not UTF-8 bytes — a byte-oriented callback
> ported unchanged from GNU readline, where `rl_getc` returns a byte in
> 0..255 or `EOF`, produces mojibake for every non-ASCII input under a
> UTF-8 locale. Values that do not fit in `wchar_t` are truncated by the
> cast. `EOF` is -1 on every supported platform, so a plain
> `getc`-style callback does at least terminate correctly.
>
> Interaction with `rl_event_hook` — a second way to lose the callback:
> `readline()` installs `_rl_event_read_char` as `EL_GETCFN` whenever
> `rl_event_hook` is non-NULL and the editor is not in `NO_TTY` mode,
> displacing the `rl_getc_function` wrapper. When `rl_event_hook` is
> later cleared, `readline()` restores the BUILT-IN reader, not the
> `rl_getc_function` wrapper. Once an event hook has been used, this
> callback stays disabled for the rest of the session unless
> `rl_initialize()` runs again. A port must reproduce that asymmetry.
>
> Scope of use: the callback supplies every character the editor
> consumes — `readline()`/`el_gets`, `rl_read_key()` (via `el_getc`),
> and the non-editing paths (`NO_TTY` or `EDIT_DISABLED`, which read
> through the same function pointer). It is NOT consulted for
> characters already queued as macro/pushback text, which `el_wgetc`
> serves from its macro stack before ever calling the reader.
>
> Lifetime and threading: a plain process global with no locking; the
> function it points at must stay valid for as long as the editor
> exists. There is no way to query what is installed through the
> readline API, and no validation of the stored value — any non-NULL
> pointer is called. Note for the port that this hook is the one place
> where a consumer's `FILE *` crosses the boundary without libedit
> needing to understand it, which is what makes it expressible under
> [dec:libedit:no-c-ffi]; `rl_instream` itself is not so lucky, since
> `rl_initialize` must derive a file descriptor from it.

> [spec:libedit:def:readline.rl-hook-func-t-void]
> typedef int rl_hook_func_t(void)

> [spec:libedit:def:readline.rl-icppfunc-t-char]
> typedef int rl_icppfunc_t(char **)

> [spec:libedit:def:readline.rl-linebuf-func-t-const-char-int]
> typedef int rl_linebuf_func_t(const char *, int)

> [spec:libedit:def:readline.rl-vcpfunc-t-char]
> typedef void rl_vcpfunc_t(char *)

> [spec:libedit:def:readline.rl-vintfunc-t-int]
> typedef void rl_vintfunc_t(int)

> [spec:libedit:def:readline.rl-voidfunc-t-void]
> typedef void rl_voidfunc_t(void)

