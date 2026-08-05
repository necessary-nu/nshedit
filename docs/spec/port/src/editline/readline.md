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

> [spec:libedit:def:readline.emacs-ctlx-keymap]
> extern KEYMAP_ENTRY_ARRAY emacs_ctlx_keymap

> [spec:libedit:sem:readline.emacs-ctlx-keymap]
> One of the three exported keymap arrays. See
> the rule [spec:libedit:sem:readline.emacs-standard-keymap] for the shape,
> the zero-fill, the ABI stride a consumer indexes with, and the fact that
> nothing in libedit reads or writes any of them. This one corresponds to
> GNU readline's `^X` prefix map and is equally inert.

> [spec:libedit:def:readline.emacs-meta-keymap]
> extern KEYMAP_ENTRY_ARRAY emacs_meta_keymap

> [spec:libedit:sem:readline.emacs-meta-keymap]
> One of the three exported keymap arrays; see
> the rule [spec:libedit:sem:readline.emacs-standard-keymap] for the shape
> and the inertness. This one corresponds to GNU readline's ESC-prefix map.
>
> It has one property the other two do not: `readline.c` defines it
> **twice**. Once in the three-name declarator at the top of the global
> block —
>
> ```c
> KEYMAP_ENTRY_ARRAY emacs_standard_keymap,
>     emacs_meta_keymap,
>     emacs_ctlx_keymap;
> ```
>
> — and once again, alone, 44 lines later, as `KEYMAP_ENTRY_ARRAY
> emacs_meta_keymap;`. Both are *tentative* definitions in the same
> translation unit, so C merges them into a single zero-initialised object;
> the duplicate is harmless, changes no observable behaviour, and is almost
> certainly an editing accident. A port exports exactly one symbol, as the C
> in fact does. The point of recording it is that a mechanical
> declaration-by-declaration translation would otherwise produce two
> definitions and fail to link, and the reader who notices the second line
> should not go looking for a distinction that is not there.

> [spec:libedit:def:readline.emacs-standard-keymap]
> extern KEYMAP_ENTRY_ARRAY emacs_standard_keymap

> [spec:libedit:sem:readline.emacs-standard-keymap]
> A process-global array of 256 `KEYMAP_ENTRY` structures, defined in
> `readline.c` as part of the declarator `KEYMAP_ENTRY_ARRAY
> emacs_standard_keymap, emacs_meta_keymap, emacs_ctlx_keymap;` — a
> tentative definition with no initialiser, so every byte is zero: each
> entry's `type` is `ISFUNC` (0) and each `function` is NULL.
>
> Nothing in libedit reads it, and nothing in libedit writes it. It exists
> for one reason: so that a program written against GNU readline, which
> refers to `emacs_standard_keymap` when it wants to bind a key in the
> default map, resolves the symbol at link time. That is the whole of the
> specified behaviour, and it is the behaviour a port must reproduce — see
> ERR-readline-54, which lists the three keymap arrays among the inert
> exports.
>
> Consumer writes are permitted by the type and are simply pointless. Filling
> an entry with a function pointer binds nothing, because the two functions
> that could plausibly consult a keymap ignore it: `rl_bind_key_in_map` and
> `rl_generic_bind` are stubs that return 0 without reading their `Keymap`
> argument, and `rl_set_keymap` does nothing at all. libedit's real bindings
> live in `EditLine`'s own `el_map`, reached through `rl_add_defun`,
> `rl_bind_key`, `.editrc` and `rl_parse_and_bind`. A consumer that fills
> these arrays and then observes no change in key handling is seeing the
> specified behaviour.
>
> There is a second, subtler reason a consumer touches them: it can obtain a
> `Keymap` value only from `rl_get_keymap` or `rl_make_bare_keymap`, and both
> return NULL, so `&emacs_standard_keymap[0]` is the one non-NULL `Keymap`
> an application can construct. Passing it anywhere still has no effect.
>
> ABI obligations for the port. The array is addressable and indexable from C,
> so its layout is part of the contract even though its contents are never
> consulted: 256 elements of `KEYMAP_ENTRY`
> ([spec:libedit:def:readline.keymap-entry]), which is `{ char type;
> rl_linebuf_func_t *function; }` and therefore 16 bytes with 7 bytes of
> padding after `type` on LP64 — a consumer doing `emacs_standard_keymap[c]`
> computes that stride itself from the header, so a port whose static has a
> different size or alignment corrupts memory rather than merely
> misbehaving. The three arrays must be three distinct objects with distinct
> addresses; a port that aliases them to one shared array would let a
> consumer's write to one appear in another.
>
> Validity: the storage exists for the lifetime of the process, before and
> after `rl_initialize`, and is never reallocated, so a consumer may cache
> the address. Ownership stays with the library; there is nothing to free.
> No locking, like every global in this layer.

> [spec:libedit:def:readline.hist-entry]
> typedef struct _hist_entry

> [spec:libedit:def:readline.histdata-t]
> typedef void *histdata_t

> [spec:libedit:def:readline.history-base]
> extern int history_base

> [spec:libedit:sem:readline.history-base]
> The logical index of the oldest history entry. Defined in `readline.c` as
> `int history_base = 1;`, with the source comment "probably never subject to
> change". Declared in the header on a shared line with `history_length`.
>
> Written by libedit in exactly two places, both of them accidents of
> bookkeeping rather than deliberate API:
>
> - `add_history` re-reads the list size after the insert and, if the count
>   did **not** change, does `history_base++` on the theory that the oldest
>   event must have been evicted to make room. The duplicate-suppression path
>   also leaves the count unchanged, so a suppressed duplicate bumps the base
>   as though an eviction had happened (ERR-readline-27).
> - `stifle_history(max)` sets `history_base = history_length - max` before
>   trimming, when the list is longer than the new cap.
>
> Two functions that change the list deliberately leave it alone:
> `clear_history` zeroes `history_offset` and `history_length` but not
> `history_base` (ERR-readline-28), and `read_history` adjusts neither
> (ERR-readline-40). After a `clear_history` the base can therefore still
> exceed 1 while the list is empty.
>
> Read by libedit in two places: `history_get(num)` rejects `num <
> history_base` and then addresses the entry as `num - history_base`, and
> `get_history_event` converts a negative event designator with `num =
> history_length - num + history_base`.
>
> The consumer may write it, and nothing validates the value. It is a pure
> index offset, so the effects are arithmetic: raising it makes the low
> indices unreachable through `history_get` (they fail the `num <
> history_base` test and return NULL); lowering it shifts every `history_get`
> index towards the front of the list, and a resulting `num - history_base`
> beyond the end simply makes the underlying `H_DELDATA` fail, so
> `history_get` returns NULL after restoring the cursor. No index is ever
> range-checked against the real list, so a port must not add one: the C
> reaches libedit's own history layer and lets it refuse.
>
> Validity: readable and writable at any time, including before
> `rl_initialize`, since it is a plain `int` with a static initialiser. Its
> value is only *meaningful* relative to the current list contents, which
> means it is stale after `read_history` and after a `clear_history`.
>
> Divergence from GNU readline: the meaning matches — the index of the first
> entry in the list, normally 1 — but GNU maintains it consistently with
> `history_length` across every list-mutating call, so `history_get
> (history_base + n)` addresses the n'th entry there and can silently miss
> here. GNU also exposes `history_set_pos`/`where_history` in terms of a
> zero-based offset from the same base; libedit's `history_offset` is
> maintained separately and is not kept in step with `history_base` at all.

> [spec:libedit:def:readline.history-expansion-char]
> extern char history_expansion_char

> [spec:libedit:sem:readline.history-expansion-char]
> The character that introduces a history expansion. Defined in `readline.c`
> as `char history_expansion_char = '!';`.
>
> Written only by the consumer; libedit never assigns it. Read on four
> distinct paths, all inside the expansion machinery:
>
> 1. `history_expand` first tests `history_expansion_char == 0` and, if so,
>    sets `*output = strdup(str)` and returns 0 — expansion is switched off
>    wholesale. The `strdup` result is not checked, so an allocation failure
>    on this path yields `*output == NULL` with a 0 return, indistinguishable
>    from "nothing to expand".
> 2. In the `^old^new^` rewrite, the two leading bytes of the synthesised
>    `!!:s…` command are written as `history_expansion_char`, not as literal
>    `'!'`.
> 3. In the scanning loop, `\` followed by `history_expansion_char` is an
>    escape: the backslash is removed with a `memmove` *through the caller's
>    buffer* (ERR-readline-55) and scanning continues. An unescaped
>    `history_expansion_char` ends a literal run and starts an expansion,
>    unless the following byte is in `history_no_expand_chars` or
>    `history_inhibit_expansion_function` vetoes it.
> 4. `get_history_event` requires its first byte to equal
>    `history_expansion_char` and returns NULL otherwise.
>
> The trap a consumer will actually hit: the `!:`, `!^`, `!*` and `!$`
> shorthands are built by `_history_expand_command` as the *literal* string
> `"!!0"` and handed to `get_history_event`, whose first test is against
> `history_expansion_char`. Change the character and the outer dispatch still
> finds the expansion but every shorthand form then returns NULL and the whole
> expansion fails with -1 (ERR-readline-57). Changing this global is therefore
> only half-supported, and a port must reproduce the half that is broken.
>
> The type is plain `char`, whose signedness is implementation-defined; the
> comparisons are all against `str[j]`, also a plain `char`, so they agree
> with each other and no sign-extension question arises. A port must keep it
> a one-byte signed-or-unsigned integer matching the platform `char`, not a
> Unicode scalar: the scan is byte-wise and a multibyte character cannot be
> expressed here.
>
> Validity: any time. It is read fresh on every call, so a consumer may
> toggle it around individual `history_expand` calls; nothing caches it.
>
> Divergence from GNU readline: same name, same default, same "0 disables"
> convention, but GNU applies the custom character consistently across the
> word designators and libedit does not (above).

> [spec:libedit:def:readline.history-inhibit-expansion-function]
> extern rl_linebuf_func_t *history_inhibit_expansion_function

> [spec:libedit:sem:readline.history-inhibit-expansion-function]
> An application-supplied veto on individual history expansions. Defined in
> `readline.c` as `rl_linebuf_func_t *history_inhibit_expansion_function =
> NULL;`, i.e. a pointer to `int (const char *, int)`. In Rust it is an
> exported mutable data symbol, `Option<extern "C" fn(*const c_char, c_int)
> -> c_int>` initialised to `None`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, in `history_expand`'s inner scan:
>
> ```c
> if (str[j] == history_expansion_char
>     && !strchr(history_no_expand_chars, str[j + 1])
>     && (!history_inhibit_expansion_function ||
>     (*history_inhibit_expansion_function)(str, (int)j) == 0))
>         break;
> ```
>
> so it is consulted only after the byte has already survived the
> `history_no_expand_chars` test, and short-circuit evaluation means it is not
> called at all when that test rejects the byte. NULL is the "no veto"
> value and is checked for; any non-NULL value is called without further
> validation.
>
> Contract seen by the callback:
>
> - Argument 1 is the line being scanned and argument 2 is the **byte index**
>   of the candidate expansion character within it. The index is `(int)j`, a
>   narrowing cast from `size_t`, so a line longer than `INT_MAX` passes a
>   negative index; that is not reachable in practice but a port must
>   reproduce the truncation rather than widen the parameter.
> - The string is *not* the caller's original in every case, and is *not* a
>   stable snapshot. In the ordinary path `str` still points at the
>   application's own buffer — `history_expand` `strdup`s a copy into
>   `*output` but never repoints `str` at it (ERR-readline-55) — and the
>   `\!` unescape `memmove`s through that same buffer as the scan proceeds,
>   so the callback can observe the line mutating between calls. In the
>   `^old^new^` path `str` has been repointed at the synthesised
>   `!!:s…` buffer instead, so the callback sees text the application never
>   wrote and an index into that text.
> - Return 0 to allow the expansion, non-zero to inhibit it. Inhibiting makes
>   the scan treat the character as ordinary text and continue.
> - The callback must not free the string, must not retain it past the call,
>   and must tolerate being called several times for one `history_expand`
>   invocation — once per candidate character.
>
> Validity: the pointer is read fresh at every candidate, so it may be
> installed and removed between `history_expand` calls, and even from inside
> the callback. There is no locking and no way to query what is installed.
>
> Divergence from GNU readline: the name, type and 0-means-expand convention
> match, but GNU passes its own working copy of the string, so the mutation
> visible above is libedit-specific; and GNU consults the hook for `^`
> substitutions on the same footing, whereas here the `^` rewrite happens
> before the scan and the hook then sees the rewritten `!!:s…` form.

> [spec:libedit:def:readline.history-length]
> extern int history_length

> [spec:libedit:sem:readline.history-length]
> The number of entries in the history list, as a mirror maintained by the
> readline layer rather than as a query against the real list. Defined in
> `readline.c` as `int history_length = 0;` and declared in the header on a
> shared line with `history_base`.
>
> Written by libedit at eight sites, always by re-reading the true count with
> `H_GETSIZE` (or by zeroing):
>
> - `rl_initialize` sets it to 0 after building a fresh history.
> - `readline()` sets it from `H_GETSIZE` after each line is read.
> - `add_history` sets it from `H_GETSIZE`, but *only* on the branch where
>   the count changed; the unchanged branch bumps `history_base` instead.
> - `remove_history` and `read_history` set it from `H_GETSIZE`.
> - `clear_history` sets it and `history_offset` to 0 in one statement.
> - `stifle_history` reads it to decide how many entries to evict.
>
> Read by libedit in `get_history_event` (negative-designator arithmetic),
> `history_set_pos` (rejects `pos >= history_length`), `next_history`
> (refuses to advance past it), `history_list` (sizes both allocations from
> it and `abort()`s if the real list turns out longer — ERR-readline-03),
> `history_get_history_state` (copies it into the returned `HISTORY_STATE`)
> and `stifle_history`.
>
> The consumer may write it, and nothing validates the value; because
> `history_list` sizes its two allocations from this global and then walks the
> *real* list, lowering it makes `history_list` write one element past the end
> of its buffer before aborting. That is the sharpest reason a port must treat
> the mirror as observable state rather than deriving the value from the list
> on demand: deriving it would silently repair a defect a consumer can trip.
>
> When it is valid: after any call that refreshes it, and stale otherwise.
> `read_history` refreshes it but leaves `history_offset` stale;
> `history_search`, `history_search_prefix` and `history_search_pos` leave
> both stale (ERR-readline-40). A negative value read back from
> `read_history` is treated as an error there (`return EINVAL`) but is not
> corrected.
>
> Divergence from GNU readline: same name and meaning, but GNU derives it
> from the list itself and keeps it correct across every mutation, and GNU's
> `HISTORY_STATE` carries `length` alongside `offset`, `size` and `flags`
> where libedit's carries only `length` (ERR-readline-48).

> [spec:libedit:def:readline.history-max-entries]
> extern int history_max_entries

> [spec:libedit:sem:readline.history-max-entries]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int history_max_entries;`, so it starts at 0 rather
> than at any meaningful capacity.
>
> Nothing in libedit writes it and nothing reads it. `stifle_history` records
> the cap it applied in `max_input_history`, not here, and
> `history_is_stifled` consults `max_input_history` too. A consumer that sets
> `history_max_entries` changes nothing; a consumer that reads it to discover
> the history capacity always sees 0, whatever `stifle_history` has been
> told.
>
> That inertness is the specified behaviour and a port must reproduce it — a
> port that helpfully kept it in step with the real cap would report a
> capacity to programs the C leaves reading 0, which is a behavioural change
> in the direction consumers cannot detect. Export a zero-initialised,
> writable `int` and never consult it. ERR-readline-54 collects the inert
> exports; this one is not currently among them (see the proposed extension
> in the errata notes for this rule set).
>
> Divergence from GNU readline: GNU's `history_max_entries` is the real
> stifle cap, maintained by `stifle_history`/`unstifle_history`, and reading
> it is the documented way to discover the limit.

> [spec:libedit:def:readline.history-no-expand-chars]
> extern char *history_no_expand_chars

> [spec:libedit:sem:readline.history-no-expand-chars]
> The set of characters which, when they immediately follow
> `history_expansion_char`, suppress the expansion. Defined in `readline.c`
> as `char *history_no_expand_chars = expand_chars;`, where `expand_chars` is
> the file-static `static char expand_chars[] = { ' ', '\t', '\n', '=', '(',
> '\0' };` — so the default set is space, tab, newline, `=` and `(`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, in `history_expand`'s scan: `!strchr(history_no_expand_chars, str[j +
> 1])`.
>
> Three properties of that single call site are load-bearing and a port must
> reproduce all three:
>
> 1. **No NULL check.** `strchr(NULL, …)` is undefined; a consumer that
>    clears the global to mean "nothing suppresses expansion" crashes on the
>    next `history_expand` that contains an expansion character. The port
>    defines this case rather than reproducing it: treat NULL as the empty
>    set, i.e. suppress nothing.
> 2. **`strchr` matches the terminating NUL.** When the expansion character
>    is the last byte of the line, `str[j + 1]` is `'\0'` and `strchr`
>    returns a pointer to the set's own terminator — non-NULL — so the
>    expansion is suppressed. A trailing `!` is therefore always literal,
>    whatever the set contains, and a port that reimplements the test as a
>    membership check over the set's characters loses that and starts trying
>    to expand a bare trailing `!`.
> 3. **The default points at mutable static storage inside the library.** The
>    type is `char *`, not `const char *`, and the initialiser is a writable
>    array, so a consumer may legally overwrite the default set in place
>    rather than repointing the global. A port must therefore back the
>    default with writable, process-lifetime storage, not a read-only string
>    constant, and must not assume the pointer still addresses the default
>    contents.
>
> Ownership: whatever the consumer stores, the consumer owns; libedit never
> frees it, never copies it, and re-reads the pointer on every candidate
> character, so it must stay valid for the duration of each `history_expand`
> call. There is no length limit and no validation of the contents.
>
> Divergence from GNU readline: GNU's default set is `" \t\n\r="`, which
> differs from libedit's on both `\r` (GNU has it, libedit does not) and `(`
> (libedit has it, GNU does not). A shell that relies on `!(` expanding —
> process substitution, say — behaves differently under the two libraries
> even with the global untouched.

> [spec:libedit:def:readline.history-offset]
> extern int history_offset

> [spec:libedit:sem:readline.history-offset]
> The readline-style cursor into the history list: the zero-based offset that
> `where_history` reports and that `previous_history`/`next_history` move.
> Defined in `readline.c` as `int history_offset = 0;`.
>
> This global is the *whole* of the cursor. libedit's own `History` object has
> a cursor too, and the two are not kept in step; `history_set_pos` assigns
> this global and nothing else, so the following `current_history` re-reads
> whatever entry the internal cursor was already on (ERR-readline-22).
>
> Written by libedit in: `using_history` (`history_offset = history_length`,
> i.e. positioned past the newest entry), `add_history` (incremented, but
> only on the branch where the count changed), `clear_history` (zeroed
> together with `history_length`), `history_set_pos` (assigned after a range
> check), `previous_history` (decremented) and `next_history` (incremented).
> Read by `where_history` (returns it verbatim), `current_history` (looks the
> entry up as `H_PREV_EVENT, history_offset + 1`), `previous_history` (a 0
> means "already at the oldest, return NULL") and `next_history` (a value
> `>= history_length` means "already past the newest").
>
> Left stale by every other list-mutating call — `read_history`,
> `remove_history`, `history_search`, `history_search_prefix` — so
> `where_history()` stops describing the list after any of them
> (ERR-readline-40). `history_search_pos` leaves it assigned to the requested
> start offset even when the search fails (ERR-readline-22).
>
> The `history_offset + 1` in `current_history` encodes an assumed identity
> between a zero-based readline offset and a one-based libedit event number.
> That identity holds only while event numbering is dense and starts at 1,
> which `H_CLEAR` and any deletion break (ERR-readline-39). A port must
> reproduce the arithmetic, not repair it.
>
> The consumer may write it; the only validation anywhere is
> `history_set_pos`'s `pos >= history_length || pos < 0` test, and assigning
> the global directly bypasses that. An out-of-range value makes
> `current_history`'s `H_PREV_EVENT` lookup fail and return NULL rather than
> misbehaving.
>
> Divergence from GNU readline: same name and same zero-based meaning, but
> GNU keeps its offset consistent with the list across every mutation and its
> `history_set_pos` really does move the cursor. GNU also exposes the offset
> in `HISTORY_STATE`, which libedit's one-member struct does not
> (ERR-readline-48).

> [spec:libedit:def:readline.history-state]
> typedef struct

> [spec:libedit:def:readline.history-subst-char]
> extern char history_subst_char

> [spec:libedit:sem:readline.history-subst-char]
> The character that, at the very start of a line, introduces the
> `^old^new^` quick-substitution form. Defined in `readline.c` as `char
> history_subst_char = '^';`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, the first thing `history_expand` does after the
> `history_expansion_char == 0` early exit: `if (str[0] ==
> history_subst_char)`. On a match the line is rewritten into an equivalent
> `!!:s<line>` command — `el_calloc(strlen(str) + 4 + 1, …)`, then
> `history_expansion_char`, `history_expansion_char`, `':'`, `'s'`, then a
> `strcpy` of the whole original line including its leading substitution
> character — and `str` is repointed at that buffer before the scan runs.
> Note that the `:` and `s` are literals: only the first two bytes track
> `history_expansion_char`.
>
> The test is on `str[0]` alone, so the form is recognised only at the start
> of the line, and it is not recognised at all when `history_expansion_char`
> is 0 because that early exit returns first.
>
> Degenerate value: setting it to 0 makes the test `str[0] == '\0'`, so the
> *empty* line takes the substitution branch and is rewritten to the
> four-byte command `"!!:s"`, which the scan then fails to expand. Setting it
> equal to `history_expansion_char` makes a leading `!` take the substitution
> branch instead of the ordinary expansion branch. Neither is validated. A
> port reproduces the comparison as written rather than special-casing 0.
>
> The type is plain `char` compared against `str[0]`, also plain `char`, so
> the comparison is self-consistent whatever the platform's signedness; a
> port keeps it a single byte, not a code point.
>
> Validity: any time; read fresh on every `history_expand` call.
>
> Divergence from GNU readline: same name, same default, same meaning. GNU's
> equivalent rewrite is internal and does not synthesise a textual `!!:s`
> command, so the `history_inhibit_expansion_function` and
> `history_no_expand_chars` hooks here see the synthesised text rather than
> the user's line — a difference that only becomes visible if the consumer
> has installed one of those.

> [spec:libedit:def:readline.keymap]
> typedef KEYMAP_ENTRY *Keymap

> [spec:libedit:def:readline.keymap-entry]
> typedef struct _keymap_entry

> [spec:libedit:def:readline.keymap-entry-array-keymap-size]
> typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE]

> [spec:libedit:def:readline.max-input-history]
> extern int max_input_history

> [spec:libedit:sem:readline.max-input-history]
> The readline layer's mirror of the history size cap. Defined in
> `readline.c` as `int max_input_history = 0;`.
>
> It is a mirror, not the cap. The real limit lives inside the `History`
> object and is set with `H_SETSIZE`; this global only records what the
> readline layer last asked for. Written by libedit in `rl_initialize`
> (`max_input_history = INT_MAX`, alongside the `H_SETSIZE, INT_MAX` that
> makes the fresh list unlimited), `stifle_history` (assigned `max` only if
> the `H_SETSIZE` succeeded) and `unstifle_history` (assigned `INT_MAX` after
> saving the previous value for the return). Read only by
> `history_is_stifled`, whose entire body is `return max_input_history !=
> INT_MAX;`, and by `unstifle_history` to produce its return value.
>
> Three consequences a port must reproduce, all recorded in ERR-readline-38:
>
> - The static initialiser is 0, not `INT_MAX`, so `history_is_stifled()`
>   returns **true** before anything has initialised the layer — a fresh
>   process reports a stifled history it does not have. Note that
>   `history_is_stifled` has no lazy-initialisation guard, so it really can
>   be reached in that state.
> - `stifle_history(INT_MAX)` sets a genuine cap and is then reported as
>   *not* stifled, because the mirror equals the sentinel.
> - A cap applied directly to the `History` object through `H_SETSIZE` — by
>   an application that also uses the `histedit.h` API on the same history —
>   is invisible here, and a cap set through `stifle_history` is invisible
>   there. The source comment on `history_is_stifled` concedes it "cannot
>   return true answer".
>
> `unstifle_history` returns the raw previous mirror value, so a fresh
> process gets 0 and an unstifled one gets `INT_MAX`, where GNU readline
> documents "the previous stifle amount, or negative if the history was not
> stifled".
>
> The consumer may write it. Doing so changes only what `history_is_stifled`
> and the next `unstifle_history` report; the real cap is untouched, which is
> exactly the divergence above seen from the other side. Nothing validates
> the value, and a negative one is simply `!= INT_MAX` and so reads as
> stifled.
>
> Validity: any time. It is not derived from anything, so it never goes
> stale in the sense the history counters do — it is simply not authoritative
> in the first place.

> [spec:libedit:def:readline.readline-echoing-p]
> extern int readline_echoing_p

> [spec:libedit:sem:readline.readline-echoing-p]
> Exported, and read by no code path at all. Defined in `readline.c` as `int
> readline_echoing_p = 1;`, under the header's "not implemented" banner.
>
> Nothing in libedit writes it and nothing reads it. Whether the terminal is
> echoing is decided in `rl_initialize`, which does its own `tcgetattr` on
> `fileno(rl_instream)` and drops the editor into non-edit mode (`el_set(e,
> EL_EDITMODE, 0)`) when `ECHO` is clear — without consulting or updating this
> global. A consumer that clears `readline_echoing_p` to suppress echo gets
> full echo; a consumer that reads it to discover whether echo is on always
> sees the initialiser, 1, even when `rl_initialize` has just concluded the
> opposite.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 1 and never consult
> it. Wiring it to the real echo state would tell programs something the C
> never tells them.
>
> Note that `_rl_echoing_p` — GNU's internal name for the same concept — is
> *also* exported here, separately, and is equally inert; the two are
> independent objects and libedit never keeps them in step.
>
> Divergence from GNU readline: `readline_echoing_p` there is maintained by
> the terminal preparation code and read by the redisplay engine, so it both
> reports and, in some paths, controls echoing.

> [spec:libedit:def:readline.rl-already-prompted]
> extern int rl_already_prompted

> [spec:libedit:sem:readline.rl-already-prompted]
> A libedit-to-consumer flag recording that the prompt has been handed to the
> display layer. Defined in `readline.c` as `int rl_already_prompted = 0;`.
>
> Written by libedit at two sites and read by libedit nowhere:
>
> - `_get_prompt`, the `EL_PROMPT_ESC` callback EditLine calls whenever it
>   needs the prompt text, sets it to 1 as its first statement and then
>   returns `rl_prompt`.
> - `readline()` clears it to 0 immediately before `el_gets`.
>
> So across one `readline()` call it reads 0 until EditLine first asks for
> the prompt and 1 thereafter, and it stays 1 after `readline()` returns
> until the next call clears it. Because `_get_prompt` fires on every
> refresh, not only the first, the flag says "the prompt has been requested
> at least once since the last `readline()` entry" — it does not say the
> prompt has been *written to the terminal*, and it is never cleared by a
> redisplay.
>
> The consumer may write it, and no code path cares: libedit reads it
> nowhere, so an assignment is a pure no-op with respect to libedit's
> behaviour and merely destroys the flag's value for the next reader. It is
> not validated and any `int` is storable.
>
> Validity: meaningful only in relation to the most recent `readline()`
> entry. In callback mode (`rl_callback_handler_install` /
> `rl_callback_read_char`) nothing ever clears it, because that path does not
> go through `readline()`, so once EditLine has asked for the prompt the flag
> stays 1 for the rest of the session.
>
> Divergence from GNU readline: there it is an *input* — the application sets
> it to tell readline that it has already displayed the prompt itself, so
> readline should not redisplay it. Here the direction is reversed: libedit
> writes it and never reads it, so setting it has no effect and the prompt is
> displayed regardless. A drop-in consumer relying on the GNU meaning gets a
> duplicated prompt.

> [spec:libedit:def:readline.rl-attempted-completion-function]
> extern rl_completion_func_t *rl_attempted_completion_function

> [spec:libedit:sem:readline.rl-attempted-completion-function]
> The application's completion generator: the hook that gets first refusal on
> every completion attempt. Defined in `readline.c` as
> `rl_completion_func_t *rl_attempted_completion_function = NULL;`, i.e. a
> pointer to `char **(const char *, int, int)`. In Rust it is an exported
> mutable data symbol, `Option<extern "C" fn(*const c_char, c_int, c_int) ->
> *mut *mut c_char>` initialised to `None`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site — `rl_complete` passes it straight to `fn_complete2`, which is where
> all of the behaviour below lives. The native `fn_complete` entry point used
> by `histedit.h` consumers passes NULL in this slot, so the hook is reached
> only through `rl_complete` (and therefore through the TAB binding
> `rl_initialize` installs).
>
> How `fn_complete2` uses it, in order:
>
> 1. `find_word_to_complete` first isolates the word under the cursor using
>    the break-character sets; if that fails the hook is never called.
> 2. `rl_point` and `rl_end` are republished *before* the call, expressly so
>    the hook can read them — the source says "these can be used by function
>    called in completion_matches() or (*attempted_completion_function)()".
>    See [spec:libedit:sem:readline.rl-point] for the units trap: on this
>    path they are wide-character offsets, not the byte offsets
>    `_rl_update_pos` publishes everywhere else.
> 3. The hook is called as `(*f)(text, start, end)` where `text` is the
>    isolated word re-encoded into `el->el_scratch`, `end` is the cursor
>    offset and `start` is `end - len`, `len` being the length of the
>    isolated word. `text` is a pointer into libedit's scratch conversion
>    buffer: borrowed, valid only for the duration of the call, and
>    invalidated by any libedit call the hook itself makes.
> 4. If the hook returns NULL **and** `rl_attempted_completion_over` is zero,
>    libedit falls back to `completion_matches(text,
>    rl_completion_entry_function)`. A non-NULL return suppresses the
>    fallback whatever `rl_attempted_completion_over` says.
> 5. `rl_attempted_completion_over` is then reset to 0 unconditionally.
>
> Ownership of the return value: `fn_complete2` takes it. Element 0 is the
> text to insert (the common prefix), elements 1..n are the matches, and the
> array is NUL-terminated. On the way out `fn_complete2` frees every element
> with `el_free` and then the array itself, so the hook must return
> `malloc`-family memory that the platform `free` can release, and must not
> retain the array. `rl_completion_matches` and `completion_matches` produce
> exactly this shape and are the intended way to build it. A hook returning
> static or stack storage corrupts the heap.
>
> Bad values: any non-NULL pointer is called with no validation. Returning an
> array whose element 0 is NULL is not tolerated — `fn_complete2` reads
> `matches[0][0]` unguarded — so "no matches" must be reported as a NULL
> return, not as an empty array.
>
> Validity: read fresh on every completion, so it may be installed, changed
> or cleared at any time, including from inside itself.
>
> Divergences from GNU readline: the name and signature match, and so does
> the "return NULL to fall through to the default generator" convention. What
> differs is everything around it — `rl_completion_append_character` is
> applied only when this hook is installed and produced a single match
> ([spec:libedit:sem:readline.rl-completion-append-character]);
> `rl_completion_type` is a libedit-computed `'\t'`/`'?'` rather than
> readline's completion-type character; `rl_filename_completion_desired`,
> `rl_ignore_completion_duplicates` and `rl_sort_completion_matches`, the
> knobs a GNU hook sets on the way out, are all inert here.

> [spec:libedit:def:readline.rl-attempted-completion-over]
> extern int rl_attempted_completion_over

> [spec:libedit:sem:readline.rl-attempted-completion-over]
> The "do not fall back to filename completion" flag. Defined in `readline.c`
> as `int rl_attempted_completion_over = 0;`.
>
> This is a consumer-to-libedit input with a libedit-to-consumer reset, and
> both halves matter. `rl_complete` passes `&rl_attempted_completion_over` to
> `fn_complete2` as its `over` parameter, and `fn_complete2`:
>
> 1. reads it, in `if (!attempted_completion_function || (over != NULL &&
>    !*over && !matches)) matches = completion_matches(…)` — so the default
>    generator runs when there is no attempted-completion hook at all, or
>    when the hook returned no matches **and** this flag is zero; then
> 2. writes 0 back into it, unconditionally, on every completion that got as
>    far as isolating a word.
>
> The consequence is that the flag is a *one-shot*. The intended use is for
> `rl_attempted_completion_function` to set it to non-zero just before
> returning NULL, meaning "I deliberately found nothing; do not offer
> filenames". It is consumed and cleared by that same completion, so it must
> be set again on the next one. Setting it once outside a completion callback
> suppresses the fallback for exactly one attempt.
>
> Note the early exits. If `find_word_to_complete` returns NULL,
> `fn_complete2` jumps to its exit before both the read and the reset, so the
> flag survives untouched. If `rl_inhibit_completion` is set, `rl_complete`
> returns before calling `fn_complete2` at all, and the flag is likewise
> untouched.
>
> Any non-zero value means "over"; nothing is validated and the value is
> never read as anything but a truth value. Writes from the consumer are
> unrestricted.
>
> Validity: meaningful only between being set and the end of the completion
> that consumes it. Reading it afterwards always yields 0, so it cannot be
> used to discover what the last completion did.
>
> Divergence from GNU readline: the name and the meaning match, and GNU also
> resets it per completion — but GNU resets it *before* calling the attempted
> completion function, whereas here the reset happens after, and only on the
> paths that reach it. The practical difference is the two early exits above,
> where a stale non-zero value carries into the next attempt.

> [spec:libedit:def:readline.rl-basic-quote-characters]
> extern const char *rl_basic_quote_characters

> [spec:libedit:sem:readline.rl-basic-quote-characters]
> Exported, and read by no code path at all. Defined in `readline.c` as
> `const char *rl_basic_quote_characters = "\"'";`.
>
> Nothing in libedit writes it and nothing reads it. Quote handling during
> completion is hard-coded: `find_word_to_complete` tests for `'` and `"`
> (and `\`) literally when it decides where the word under the cursor starts,
> and `escape_filename` does the same when it quotes a match. Changing this
> global changes neither.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it. The value is a pointer to a string literal, so the initial
> value is read-only storage in the C — a consumer that writes *through* the
> pointer rather than replacing it invokes undefined behaviour. The port
> defines this by backing the default with immutable storage: the pointer is
> writable, the bytes it initially addresses are not.
>
> Ownership: whatever a consumer stores, the consumer owns. libedit never
> frees, copies or dereferences it, so even a dangling pointer is harmless
> here — which is worth stating precisely because it stops being true if a
> port "helpfully" starts consulting it.
>
> Divergence from GNU readline: there it is genuinely consulted, both when
> deciding whether the word being completed is inside quotes and when
> deciding how to quote a completed filename, and it interacts with
> `rl_completer_quote_characters` — which is likewise inert here.

> [spec:libedit:def:readline.rl-basic-word-break-characters]
> extern const char *rl_basic_word_break_characters

> [spec:libedit:sem:readline.rl-basic-word-break-characters]
> The one and only word-break set libedit actually uses. Defined in
> `readline.c` as `const char *rl_basic_word_break_characters =
> break_chars;`, where `break_chars` is the file-static
>
> ```c
> static char break_chars[] = { ' ', '\t', '\n', '"', '\\', '\'', '`', '@',
>     '$', '>', '<', '=', ';', '|', '&', '{', '(', '\0' };
> ```
>
> Written only by the consumer; libedit never assigns it. Read twice, both in
> `rl_complete`: once as the fallback for `breakchars` when
> `rl_completion_word_break_hook` is NULL, and once — unconditionally,
> whatever the hook returned — as the `word_break` argument to
> `fn_complete2`. That second read is the important one: it means the
> word-break set is *always* this global. `rl_completer_word_break_characters`
> is never consulted, and the hook's return value lands in the
> special-prefixes slot instead, so it can only add break characters and
> never remove one (ERR-readline-50).
>
> How the set is used: `find_word_to_complete` scans backwards from the
> cursor and stops at the first character found in it (or in the special
> prefixes). A character preceded by a backslash is skipped over rather than
> matched, and a lone leading `'` or `"` is dropped from the word.
>
> Bad values, and what the port defines:
>
> - **NULL faults.** `rl_complete` converts the string with
>   `ct_decode_string`, which returns NULL for a NULL input, and
>   `find_word_to_complete` then calls `wcschr(NULL, …)` with no check. Any
>   completion where the cursor is not already at the start of the buffer
>   dereferences NULL. This is ERR-completion-05; the port defines it as the
>   empty set — nothing breaks the word, so the whole line becomes one word.
> - **A string invalid in the current locale faults the same way**, because
>   `mbstowcs` fails and `ct_decode_string` again returns NULL. The port
>   applies the same definition.
> - An empty string is legal and means "nothing breaks the word".
> - The conversion goes through a function-`static ct_buffer_t` that grows on
>   demand and is never freed — a bounded once-per-process allocation, not a
>   leak per call, but it does mean two interleaved completions would share
>   one buffer. The readline layer is single-threaded throughout.
>
> Ownership and lifetime: the consumer owns whatever it stores. libedit
> re-reads the pointer and re-converts the string on every completion, and
> never retains the multibyte pointer past the conversion, so it must be
> valid for the duration of `rl_complete`. The default points at *mutable*
> file-static storage despite the `const char *` declaration, so a consumer
> may overwrite the default set in place; a port must therefore back the
> default with writable process-lifetime storage rather than a read-only
> literal.
>
> Divergence from GNU readline: GNU treats this as the *default* set and
> lets `rl_completer_word_break_characters` override it per application, and
> lets `rl_completion_word_break_hook` replace it per completion. Here it is
> the only set there is, so the override and the replacement both silently
> fail to take effect. GNU's default value also differs: it includes `)`,
> `:` and `?`, which libedit's does not, and libedit's includes `{` and `(`.

> [spec:libedit:def:readline.rl-catch-signals]
> extern int rl_catch_signals

> [spec:libedit:sem:readline.rl-catch-signals]
> Whether libedit installs its own signal handlers around line editing.
> Defined in `readline.c` as `int rl_catch_signals = 1;`, under the header's
> "not implemented" banner — a banner that is wrong for this entry.
>
> It is honoured, at exactly one site: `rl_initialize` does `el_set(e,
> EL_SIGNAL, rl_catch_signals)`. That is the whole of the read. libedit's
> signal discipline is to set handlers on entry to `el_gets` and clear them
> on the way out, for `SIGINT`, `SIGQUIT`, `SIGHUP`, `SIGTERM`, `SIGCONT` and
> `SIGWINCH`; a zero here disables that, leaving the application's handlers
> in place and the terminal modes unrestored on a signal.
>
> The timing is the trap, and a port must reproduce it: the value is sampled
> **once**, when the editor is created. Setting it before the first call to
> `readline()` — or any other entry point that lazily initialises — works.
> Setting it afterwards has no effect at all; the only way to change the
> setting later is another `rl_initialize()`, which tears down and rebuilds
> both the editor and the history list, losing the history contents, custom
> bindings, prompt and terminal settings. GNU readline reads
> `rl_catch_signals` each time it prepares the terminal, so it can be toggled
> between calls.
>
> The value is passed through to `EL_SIGNAL` untouched and is treated there
> as a boolean; nothing validates it and any non-zero value enables handling.
> The consumer may write it at any time — the write always succeeds, it is
> the *effect* that is confined to initialisation time.
>
> ERR-readline-54 lists the inert readline exports and once listed this one
> among them; that was an error, corrected in the entry itself. It is
> genuinely read. Its sibling `rl_catch_sigwinch` is not.

> [spec:libedit:def:readline.rl-catch-sigwinch]
> extern int rl_catch_sigwinch

> [spec:libedit:sem:readline.rl-catch-sigwinch]
> Exported, and read by no code path at all. Defined in `readline.c` as `int
> rl_catch_sigwinch = 1;`, immediately below `rl_catch_signals` and under the
> same "not implemented" banner — but unlike its neighbour, this one really
> is not implemented. ERR-readline-54 records it among the inert exports.
>
> Nothing in libedit writes it and nothing reads it. Window-change handling
> is not separable here: `SIGWINCH` is part of the set `EL_SIGNAL` installs,
> so it is governed by `rl_catch_signals` along with everything else. Clearing
> `rl_catch_sigwinch` while leaving `rl_catch_signals` set does not stop
> libedit catching `SIGWINCH`, and setting it while `rl_catch_signals` is
> clear does not make libedit catch it.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 1 and never consult
> it. A port that wired it to a separate `SIGWINCH` decision would give
> consumers a control the C does not give them.
>
> Divergence from GNU readline: there the two flags are independent, and
> `rl_catch_sigwinch` specifically governs whether readline installs a
> `SIGWINCH` handler to track terminal resizes. An application that clears it
> because it manages resizes itself will find libedit still resizing.

> [spec:libedit:def:readline.rl-command-func-t-int-int]
> typedef int rl_command_func_t(int, int)

> [spec:libedit:def:readline.rl-compdisp-func-t-char-int-int]
> typedef void rl_compdisp_func_t(char **, int, int)

> [spec:libedit:def:readline.rl-compentry-func-t-const-char-int]
> typedef char *rl_compentry_func_t(const char *, int)

> [spec:libedit:def:readline.rl-complete-mark-directories]
> extern int _rl_complete_mark_directories

> [spec:libedit:sem:readline.rl-complete-mark-directories]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int _rl_complete_mark_directories;`, so it starts at
> 0 — note that GNU readline's counterpart defaults to *on*, so even the
> initial value disagrees with what a consumer reading it would expect.
>
> Nothing in libedit writes it and nothing reads it. Directory marking is
> unconditional: `append_char_function`, the default append hook inside
> `filecomplete.c`, `stat`s each match and returns `"/"` for a directory and
> `" "` otherwise, with nothing consulted in between. Clearing this global
> does not stop the slash being appended.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.
>
> The leading underscore marks it as one of GNU readline's internal
> variables that libedit exports anyway, because programs — notably older
> shells — reach into readline's internals and would otherwise fail to link.
> The same applies to `_rl_print_completions_horizontally`,
> `_rl_completion_prefix_display_length` and `_rl_echoing_p`.

> [spec:libedit:def:readline.rl-completer-quote-characters]
> extern const char *rl_completer_quote_characters

> [spec:libedit:sem:readline.rl-completer-quote-characters]
> Exported, and read by no code path at all. Defined in `readline.c` as
> `const char *rl_completer_quote_characters = NULL;`.
>
> Nothing in libedit writes it and nothing reads it. As with
> `rl_basic_quote_characters`, quoting during completion is hard-coded:
> `find_word_to_complete` tests for `'`, `"` and `\` literally, and
> `escape_filename` — which is only reached under the `FN_QUOTE_MATCH` flag
> that no readline path sets — has its own fixed notion of quoting. Setting
> this global has no effect whatsoever.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `*const c_char` initialised to NULL and
> never dereference it. Because it is never dereferenced, no value is
> invalid — including a dangling pointer — and a port that started consulting
> it would turn previously harmless consumer state into a crash.
>
> Divergence from GNU readline: there, a non-NULL value switches on the whole
> quoted-word completion machinery — the word under the cursor is recognised
> as quoted, `rl_char_is_quoted_p` is consulted, and matches are requoted on
> insertion. A consumer that sets it expecting that behaviour gets libedit's
> fixed handling instead.

> [spec:libedit:def:readline.rl-completer-word-break-characters]
> extern char *rl_completer_word_break_characters

> [spec:libedit:sem:readline.rl-completer-word-break-characters]
> Exported, and read by no code path at all. Defined in `readline.c` as `char
> *rl_completer_word_break_characters = NULL;`. ERR-readline-50 records it,
> together with `rl_special_prefixes`, as declared but consulted nowhere.
>
> Nothing in libedit writes it and nothing reads it. `rl_complete` passes
> `rl_basic_word_break_characters` as `fn_complete2`'s word-break set every
> time, and puts `rl_completion_word_break_hook`'s result — or, absent a
> hook, `rl_basic_word_break_characters` again — into the *special-prefixes*
> slot. There is no third slot and no path that reaches this global.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it. This one deserves emphasis because it is the single most
> likely global for a drop-in consumer to set: in GNU readline it is *the*
> per-application word-break set, the documented way for a shell to say
> "break on my metacharacters", and it takes precedence over
> `rl_basic_word_break_characters`. Here the assignment is silently
> discarded and the built-in set applies. A consumer that wants to add break
> characters must install `rl_completion_word_break_hook`
> ([spec:libedit:sem:readline.rl-completion-word-break-hook-fn]); a consumer
> that wants to *remove* one must overwrite
> `rl_basic_word_break_characters`, since the hook can only widen the set.
>
> Ownership: whatever a consumer stores, the consumer owns; libedit never
> frees, copies or dereferences it. The declaration is `char *`, not `const
> char *`, so it is one of the two word-break globals a consumer may write
> through as well as assign — and neither write has any effect.

> [spec:libedit:def:readline.rl-completion-append-character]
> extern int rl_completion_append_character

> [spec:libedit:sem:readline.rl-completion-append-character]
> The character appended after a completed word. Defined in `readline.c` as
> `int rl_completion_append_character = ' ';`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, `_rl_completion_append_character_function`
> ([spec:libedit:sem:readline.rl-completion-append-character-function-fn]),
> whose whole body is
>
> ```c
> static char buf[2];
> buf[0] = (char)rl_completion_append_character;
> buf[1] = '\0';
> return buf;
> ```
>
> That function is handed to `filecomplete.c` as the `app_func` in two
> places: by `rl_complete`, as `fn_complete2`'s append hook, and by
> `rl_display_match_list`, as `fn_display_match_list`'s. It therefore
> displaces `append_char_function`, the default that appends `"/"` for a
> directory and `" "` otherwise — so on the readline path directories are
> **not** marked with a slash, whatever the completed name is, and
> `_rl_complete_mark_directories` is not consulted either.
>
> Where the character actually lands, and where it does not:
>
> - **After an inserted completion**, but only under `single_match &&
>   attempted_completion_function && !(flags & FN_QUOTE_MATCH)`.
>   `rl_complete` always passes flags 0, so the live condition is: the
>   completion was unique **and** `rl_attempted_completion_function` is
>   installed. With no attempted-completion hook — the default filename
>   completion — nothing is ever appended, however unique the match. GNU
>   readline appends after any single match, so this is a visible divergence
>   for the most common consumer configuration of all.
> - **After every entry in a displayed match list.**
>   `fn_display_match_list` prints each match followed by
>   `(*app_func)(match)`, so the listing shows a trailing space (or whatever
>   the character is) on every column entry. The column padding is computed
>   from `strlen(match)` alone and does not account for the appended byte, so
>   changing this global from a space to a wider-looking character skews the
>   columns.
>
> Bad values. The `(char)` narrowing is unchecked, so anything outside the
> platform `char` range is truncated: 0x2020 becomes 0x20 and appends a
> space. A multibyte character cannot be expressed — the hook returns a
> one-byte string, which `ct_decode_string` then converts, so a byte that is
> not a complete character in the current locale is dropped by the conversion
> rather than inserted. Setting it to **0** makes `buf[0]` the terminator, so
> the hook returns the empty string and nothing is appended; that is the
> supported way to suppress the append, and it is also the trigger for
> ERR-completion-10's embedded-NUL defect in `escape_filename` — unreachable
> from the readline paths, which never set `FN_QUOTE_MATCH`, but reachable
> for an application calling `fn_complete2` directly.
>
> Lifetime note for the port: the returned pointer addresses a function-local
> `static char[2]`, rewritten on every call and never freed. Two nested uses
> would alias, which is only theoretical in a single-threaded layer, but the
> port must return storage with the same "valid until the next call"
> contract rather than a fresh allocation, because nothing frees it.
>
> Validity: read fresh on every append, so the consumer may change it at any
> time, including from inside `rl_attempted_completion_function`.
>
> Divergence from GNU readline: besides the two-condition restriction above,
> GNU also honours `rl_completion_suppress_append` as a per-completion
> override of this character. That global is exported here and never read, so
> the override is unavailable.

> [spec:libedit:def:readline.rl-completion-display-matches-hook]
> extern rl_compdisp_func_t *rl_completion_display_matches_hook

> [spec:libedit:sem:readline.rl-completion-display-matches-hook]
> Exported, and read by no code path at all. Defined in `readline.c` as
> `rl_compdisp_func_t *rl_completion_display_matches_hook = NULL;`, i.e. a
> pointer to `void (char **, int, int)`. ERR-readline-54 records it among the
> inert exports.
>
> Nothing in libedit writes it and nothing reads it. Match listing goes
> straight to `fn_display_match_list`, both from `fn_complete2`'s "more than
> one match and the user asked to see them" branch and from the exported
> `rl_display_match_list`, and neither checks this hook first. A consumer
> that installs it to take over the display sees libedit's own columnar
> listing on `el->el_outfile` instead, and its hook is never called.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable nullable function pointer and never call
> through it. This is the inert export most likely to be mistaken for a bug
> during the port, because calling it would be easy and would look like an
> improvement; it would in fact silence the built-in listing for every
> consumer that has ever set the hook defensively.
>
> Divergence from GNU readline: there the hook, when set, entirely replaces
> the built-in display and is called with the match array, the match count
> and the longest match's length — the same `(char **, int, int)` signature
> the typedef here describes.

> [spec:libedit:def:readline.rl-completion-entry-function]
> extern rl_compentry_func_t *rl_completion_entry_function

> [spec:libedit:sem:readline.rl-completion-entry-function]
> The generator libedit uses when it falls back to its own match collection.
> Defined in `readline.c` as `rl_compentry_func_t
> *rl_completion_entry_function = NULL;`, i.e. a pointer to `char *(const
> char *, int)`. In Rust it is an exported mutable data symbol,
> `Option<extern "C" fn(*const c_char, c_int) -> *mut c_char>` initialised to
> `None`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site: `rl_complete` passes it to `fn_complete2` as `complete_func`, through
> a redundant `(rl_compentry_func_t *)` cast that changes nothing — the
> declared type is already that. `fn_complete2` substitutes
> `fn_filename_completion_function` when it is NULL, so NULL means "complete
> filenames", which is the default.
>
> It is reached only on the fallback path — when there is no
> `rl_attempted_completion_function`, or when that hook returned NULL and
> `rl_attempted_completion_over` is zero — and then only through
> `completion_matches`, which drives it as a stateful generator:
> `(*f)(text, state)` with `state` 0 on the first call for a given word and
> 1, 2, 3, … thereafter. `state == 0` means "start a fresh scan for this
> text"; every later call means "next match for the same text". The generator
> ends the scan by returning NULL. A generator that never returns NULL loops
> forever and grows the match array without bound.
>
> Ownership: every non-NULL return transfers a heap string to
> `completion_matches` and thence to `fn_complete2`, which frees the whole
> array with `el_free` on the way out. The generator must return
> `malloc`-family memory; static or stack storage corrupts the heap. `text`
> is borrowed — it points into `el->el_scratch` — and must not be retained or
> freed.
>
> Bad values: any non-NULL pointer is called with no validation. Note that
> `completion_matches` does not itself dereference `text`, so whether a NULL
> `text` is survivable depends entirely on the generator; libedit never
> passes NULL on this path.
>
> Validity: read fresh on every completion, so it may be installed or cleared
> at any time. Reentrancy is the generator's problem: libedit's own
> `fn_filename_completion_function` holds a static `DIR *` and static scan
> state, so two interleaved scans through it corrupt each other.
>
> Divergence from GNU readline: the name, signature and generator protocol
> match. What differs is the list-building step behind it — libedit's
> fallback goes through `completion_matches`, which does not sort and leaves
> element 0 empty when the matches share no prefix, rather than through
> `rl_completion_matches`, which sorts and substitutes the original text
> ([spec:libedit:sem:readline.completion-matches-fn] sets out the
> divergence). GNU also documents `rl_completion_entry_function` as
> defaulting to filename completion when NULL, which does match.

> [spec:libedit:def:readline.rl-completion-func-t-const-char-int-int]
> typedef char **rl_completion_func_t(const char *, int, int)

> [spec:libedit:def:readline.rl-completion-prefix-display-length]
> extern int _rl_completion_prefix_display_length

> [spec:libedit:sem:readline.rl-completion-prefix-display-length]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int _rl_completion_prefix_display_length;`, so it
> starts at 0.
>
> Nothing in libedit writes it and nothing reads it. `fn_display_match_list`
> prints each match in full, computing its column width from
> `strlen(matches[thisguy])`, with no notion of eliding a shared prefix.
> Setting this global does not shorten the listing.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.
>
> Divergence from GNU readline: there, a positive value makes readline
> replace the common prefix of the displayed matches with an ellipsis once
> that prefix is at least this long, so long shared paths do not dominate the
> listing.

> [spec:libedit:def:readline.rl-completion-query-items]
> extern int rl_completion_query_items

> [spec:libedit:sem:readline.rl-completion-query-items]
> The threshold above which libedit asks before listing possible completions.
> Defined in `readline.c` as `int rl_completion_query_items = 100;` with the
> source comment "If more than this number of items results from query for
> possible completions, we ask user if they are sure to really display the
> list."
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site: `rl_complete` passes `(size_t)rl_completion_query_items` to
> `fn_complete2` as `query_items`. `fn_complete2` uses it only in the branch
> that lists matches — reached when the match is not unique and the same
> completion command has been invoked twice in a row — as
>
> ```c
> if (matches_num > query_items) {
>         (void)fprintf(el->el_outfile,
>             "Display all %zu possibilities? (y or n) ", matches_num);
>         (void)fflush(el->el_outfile);
>         if (getc(stdin) != 'y')
>                 match_display = 0;
>         (void)fprintf(el->el_outfile, "\n");
> }
> ```
>
> `matches_num` counts the real matches, i.e. the array length excluding the
> common-prefix element 0.
>
> Two properties a port must reproduce exactly:
>
> - **The confirmation is read from `stdin`, not from `rl_instream`**, with a
>   plain blocking `getc`. An application that redirected `rl_instream` to
>   another stream has its completion prompt answered from the process's
>   standard input regardless, and an application whose `stdin` is not a
>   terminal gets whatever byte happens to be there — or EOF, which is not
>   `'y'`, so the list is suppressed. Only a single byte is consumed, so the
>   user's newline stays in the buffer.
> - **The comparison is unsigned.** The cast to `size_t` means a negative
>   `rl_completion_query_items` becomes an enormous threshold and the query
>   never fires, so every list is shown unconditionally. That happens to
>   agree with GNU readline, where a negative value is *documented* to mean
>   "never ask", but here it is a consequence of the cast rather than a
>   check, and a port must obtain it the same way rather than by testing for
>   a negative value — the two differ at exactly one input, `INT_MIN` on a
>   platform where `size_t` is narrower than `int`, which the port should
>   simply define as "never ask" in line with the platforms libedit targets.
>
> A value of 0 makes the query fire whenever there is at least one match to
> list. Nothing else validates the value.
>
> Validity: read fresh on every completion, so it may be changed at any time,
> including from inside `rl_attempted_completion_function`.
>
> Divergence from GNU readline: the name, default and meaning match. What
> differs is the input stream the answer is read from (above), and that GNU
> accepts `y`/`Y`/`space` and treats anything else as no, where libedit
> accepts only a lower-case `y`.

> [spec:libedit:def:readline.rl-completion-suppress-append]
> extern int rl_completion_suppress_append

> [spec:libedit:sem:readline.rl-completion-suppress-append]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int rl_completion_suppress_append;`, so it starts at
> 0.
>
> Nothing in libedit writes it and nothing reads it. Whether the append
> character is inserted is decided entirely by `fn_complete2`'s `single_match
> && attempted_completion_function && !(flags & FN_QUOTE_MATCH)` test; this
> global is not part of it. A consumer that sets it from inside
> `rl_attempted_completion_function` to suppress the trailing space — the
> documented GNU idiom for completing a directory name or an unfinished
> token — gets the space anyway.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.
>
> The supported way to suppress the append in this library is to set
> `rl_completion_append_character` to 0, which makes the append hook return
> an empty string — a process-wide switch rather than the per-completion one
> a GNU consumer expects.

> [spec:libedit:def:readline.rl-completion-type]
> extern int rl_completion_type

> [spec:libedit:sem:readline.rl-completion-type]
> A libedit-to-consumer output describing what kind of completion is in
> progress. Defined in `readline.c` as `int rl_completion_type = 0;` with the
> source comment "This is set to character indicating type of completion
> being done by rl_complete_internal(); this is available for application
> completion functions."
>
> Written by libedit only, and read by libedit nowhere. `rl_complete` passes
> `&rl_completion_type` to `fn_complete2`, which assigns it near the top of
> the function, *before* the word is isolated and before any hook runs:
>
> ```c
> int what_to_do = '\t';
> if (el->el_state.lastcmd == el->el_state.thiscmd)
>         what_to_do = '?';
> if (completion_type != NULL)
>         *completion_type = what_to_do;
> ```
>
> So it takes exactly two values: `'\t'` (9) for a first completion attempt
> and `'?'` (63) when the immediately preceding editor command was the same
> command — that is, on the second and subsequent consecutive TAB, which is
> also what switches `fn_complete2` from "insert the common prefix" to "list
> the possibilities".
>
> When it is valid: inside `rl_attempted_completion_function` and inside the
> generator, which is precisely the window the source comment describes,
> since the assignment precedes both calls. Outside a completion it holds
> whatever the last completion left, and it is *not* updated on the two paths
> that never reach `fn_complete2` — a `rl_inhibit_completion` short-circuit
> in `rl_complete`, and a `find_word_to_complete` failure, which happens
> after the assignment and so does update it.
>
> The consumer may write it, and libedit neither validates nor reads the
> value; the next completion overwrites it. Because `rl_completion_type` is
> also the only channel through which an application can tell "first TAB"
> from "second TAB", a consumer that clobbers it loses that distinction
> without libedit noticing.
>
> Divergence from GNU readline: the variable is set from the invoking
> character there too, but readline's repertoire is larger — `'\t'` for
> `rl_complete`, `'?'` for `rl_possible_completions`, `'*'` for
> `rl_insert_completions`, `'!'` and `'@'` for the menu variants — and the
> value reflects *which function the user invoked*, not how many times in a
> row. libedit synthesises `'?'` from command repetition instead, so an
> application switching on the value sees `'?'` in situations GNU readline
> would report as `'\t'`, and never sees the other three at all.

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

> [spec:libedit:def:readline.rl-deprep-term-function]
> extern rl_voidfunc_t *rl_deprep_term_function

> [spec:libedit:sem:readline.rl-deprep-term-function]
> Exported with a non-NULL default, and called by no code path at all.
> Defined in `readline.c` as
>
> ```c
> rl_voidfunc_t *rl_deprep_term_function = (rl_voidfunc_t *)rl_deprep_terminal;
> ```
>
> The cast is an identity: `rl_deprep_terminal` is already `void (void)` and
> `rl_voidfunc_t` is `void (void)`. It is written for symmetry with
> `rl_prep_term_function`, whose cast is equally redundant.
>
> Nothing in libedit writes it after initialisation and nothing calls through
> it. `rl_reset_after_signal` calls through `rl_prep_term_function`, its
> counterpart, but there is no site that calls through this one — the
> terminal is de-prepared by `tty_end(e, TCSADRAIN)` at the end of
> `readline()`, directly, without indirection. Replacing this pointer
> therefore changes nothing about libedit's behaviour.
>
> A port must still export it with the correct default, because the default
> is observable in a way the inert-and-NULL globals are not: a consumer may
> *read* it and call through it, which is a supported readline idiom for
> "restore the terminal the way the library would". Doing so here reaches
> `rl_deprep_terminal`, whose whole body is `el_set(e, EL_PREP_TERM, 0)`. So
> the exported value must be the address of the exported `rl_deprep_terminal`
> — not a private copy, and not NULL.
>
> Bad values: never dereferenced by libedit, so no value is invalid from
> libedit's side; the hazard belongs entirely to a consumer calling through
> it.
>
> Divergence from GNU readline: there the pointer is the indirection readline
> itself uses whenever it restores terminal modes, so replacing it really
> does take over terminal handling. Here it is a value to read, not a hook to
> install.

> [spec:libedit:def:readline.rl-directory-completion-hook]
> extern rl_icppfunc_t *rl_directory_completion_hook

> [spec:libedit:sem:readline.rl-directory-completion-hook]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `rl_icppfunc_t *rl_directory_completion_hook;`, i.e. a
> pointer to `int (char **)`, zero-initialised to NULL.
>
> Nothing in libedit writes it and nothing reads it. Directory names are
> resolved during completion by `fn_filename_completion_function`, which
> splits the text at the last `/`, tilde-expands the directory part with
> `fn_tilde_expand` and calls `opendir` — with no hook consulted anywhere on
> that path. A consumer that installs this to rewrite directory names before
> the scan (the GNU idiom for `cdable_vars`-style shortcuts, or for
> canonicalising `..`) sees its function never called.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable nullable function pointer and never call
> through it. ERR-readline-54 collects the inert exports; this one is not
> currently among them.
>
> Divergence from GNU readline: there the hook receives the address of the
> directory-name string, may `free` it and replace it with a new allocation,
> and returns non-zero if it changed it. None of that ownership protocol is
> exercised here, so a hook written for GNU semantics simply never runs.

> [spec:libedit:def:readline.rl-display-prompt]
> extern char *rl_display_prompt

> [spec:libedit:sem:readline.rl-display-prompt]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `char *rl_display_prompt;`, zero-initialised to NULL —
> and never assigned, so it stays NULL for the life of the process however
> many prompts are set.
>
> Nothing in libedit writes it and nothing reads it. The prompt libedit
> displays is `rl_prompt`, handed to EditLine by the `_get_prompt` callback;
> this global is not part of that path. A consumer that reads it expecting
> the currently displayed prompt gets NULL, and one that dereferences it
> without checking crashes — which is worth stating plainly, because the GNU
> variable it mirrors is never NULL once readline has initialised.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `*mut c_char` initialised to NULL and never
> assign it. ERR-readline-54 collects the inert exports; this one is not
> currently among them.
>
> Divergence from GNU readline: there it points at the prompt actually being
> displayed, which is `rl_prompt` most of the time but the search prompt
> during an incremental search — the distinction being exactly why an
> application reads it rather than reading `rl_prompt`.

> [spec:libedit:def:readline.rl-done]
> extern int rl_done

> [spec:libedit:sem:readline.rl-done]
> The "stop editing this line" flag. Defined in `readline.c` as `int rl_done
> = 0;`.
>
> This is a consumer-to-libedit input with exactly one producer and one
> consumer inside the library:
>
> - `readline()` clears it to 0 immediately before entering `el_gets`, so
>   each line starts with it clear. This is the only libedit write.
> - `rl_bind_wrapper` — the EditLine command `rl_add_defun` installs for
>   every application command — reads it immediately after dispatching that
>   command and returns `CC_EOF` if it is non-zero, otherwise `CC_NORM`.
>
> So the supported use is narrow and specific: inside a function registered
> with `rl_add_defun`, set `rl_done` non-zero to make the current line
> terminate as though end-of-input had been reached. `CC_EOF` makes `el_gets`
> return NULL, so `readline()` then returns NULL — the line's contents are
> *discarded*, not accepted. That is the opposite of what a consumer setting
> `rl_done` usually intends and a port must reproduce it.
>
> Everywhere else it is inert. Setting it from outside a command has no
> effect at all — nothing polls it, and the next `readline()` clears it.
> Setting it in callback mode has no effect either: `rl_callback_read_char`
> neither reads nor clears it, so a callback-mode application cannot stop the
> line this way, and a value left over from a `rl_add_defun` command persists
> until the next `readline()` call clears it.
>
> Any non-zero value is "done"; nothing validates it and no value is
> reserved.
>
> Divergence from GNU readline: there `rl_done` is polled by the main
> read loop, so setting it from any hook — `rl_event_hook`,
> `rl_pre_input_hook`, a signal handler — ends the line, and the line's
> contents are *returned* to the caller. Here neither is true: only
> `rl_bind_wrapper` looks, and what it does with the answer is report
> end-of-file.

> [spec:libedit:def:readline.rl-echoing-p]
> extern int _rl_echoing_p

> [spec:libedit:sem:readline.rl-echoing-p]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int _rl_echoing_p;`, so it starts at 0 — where GNU
> readline's starts at 1, so the initial value disagrees as well as the
> behaviour.
>
> Nothing in libedit writes it and nothing reads it. It is the internal twin
> of `readline_echoing_p`, which is *also* exported here and equally inert;
> the two are separate objects and libedit never keeps them in step, so an
> application that sets one and reads the other sees no connection. Echo is
> decided by `rl_initialize`'s own `tcgetattr` on `fileno(rl_instream)`,
> which consults neither.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.

> [spec:libedit:def:readline.rl-end]
> extern int rl_end

> [spec:libedit:sem:readline.rl-end]
> The offset of the end of the line — that is, its length. Defined in
> `readline.c` as `int rl_end = 0;` and declared in the header on a shared
> line with `rl_point`.
>
> It is written by the same two producers as `rl_point`, in the same units,
> at the same moments, and it carries the same units divergence between them;
> see [spec:libedit:sem:readline.rl-point], which specifies both. In
> summary: `_rl_update_pos` sets it to `li->lastchar - li->buffer` from the
> **narrow** `el_line`, a byte count, and then writes `rl_line_buffer[rl_end]
> = '\0'`; `fn_complete2` sets it to `li->lastchar - li->buffer` from the
> **wide** `el_wline`, a character count, just before calling
> `rl_attempted_completion_function`.
>
> The `rl_line_buffer[rl_end] = '\0'` is the reason `rl_end` matters beyond
> reporting: it is the index at which `_rl_update_pos` terminates the string
> a consumer reads through `rl_line_buffer`. A consumer that assigns
> `rl_end` does not shorten the line — nothing propagates the value back into
> EditLine — but the *next* `_rl_update_pos` recomputes it from the real line
> and re-terminates there, so the assignment is not merely ignored, it is
> overwritten.
>
> Read by libedit nowhere. `rl_insert_text` and `rl_delete_text` do not
> refresh it, so it is stale after either (ERR-readline-35).
>
> Divergence from GNU readline: there `rl_end` is maintained continuously by
> the editing primitives and is authoritative — assigning it, together with
> `rl_line_buffer`, is how an application replaces the line. Here it is a
> report, and only as fresh as the last `_rl_update_pos`.

> [spec:libedit:def:readline.rl-erase-empty-line]
> extern int rl_erase_empty_line

> [spec:libedit:sem:readline.rl-erase-empty-line]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int rl_erase_empty_line;`, so it starts at 0.
>
> Nothing in libedit writes it and nothing reads it. Line display is
> EditLine's `refresh.c`, which has no notion of this setting. Setting it
> changes nothing about what appears on the terminal when the line is empty.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.
>
> Divergence from GNU readline: there, a non-zero value makes readline erase
> the prompt and the line when the user deletes the last character, rather
> than leaving an empty prompt on screen — a visible difference an
> application enabling it will not get here.

> [spec:libedit:def:readline.rl-event-hook]
> extern rl_hook_func_t *rl_event_hook

> [spec:libedit:sem:readline.rl-event-hook]
> An application callback libedit runs while it waits for input. Defined in
> `readline.c` as `rl_hook_func_t *rl_event_hook = NULL;`, i.e. a pointer to
> `int (void)`. In Rust it is an exported mutable data symbol,
> `Option<extern "C" fn() -> c_int>` initialised to `None`.
>
> Written only by the consumer; libedit never assigns it. Read at three
> sites, two of which are the installation machinery and one of which is the
> call:
>
> 1. `readline()`, on every call, before reading: if it is non-NULL **and**
>    the editor is not in `NO_TTY` mode, `el_set(e, EL_GETCFN,
>    _rl_event_read_char)` displaces whatever character reader was installed,
>    and a file-static `used_event_hook` is set.
> 2. `readline()`, immediately after: if it is NULL and `used_event_hook` is
>    set, the reader is restored to `EL_BUILTIN_GETCFN` and the flag cleared.
> 3. `_rl_event_read_char`, the installed reader, loops `while
>    (rl_event_hook)` calling `(*rl_event_hook)()` and polling the descriptor
>    for a byte; on exit from the loop, if the hook has become NULL, it
>    restores the built-in reader itself.
>
> Two consequences a port must reproduce. First, the restore in step 2 goes
> to the *built-in* reader, not to the `rl_getc_function` wrapper — so once
> an event hook has been used, a previously installed `rl_getc_function` stays
> disabled for the rest of the session unless `rl_initialize()` runs again
> (ERR-readline-31). Second, the hook is only installed by `readline()`, so
> a callback-mode application (`rl_callback_handler_install` /
> `rl_callback_read_char`) never gets it at all.
>
> What the hook may assume when it runs: the editor exists, and it is being
> called from inside a read, so it must not call back into `readline()` or
> `el_gets`. Its return value is **discarded** — there is no way to signal
> anything to libedit from it. Setting `rl_event_hook = NULL` from inside the
> hook does terminate the loop and restore the built-in reader, which is the
> only in-band control it has.
>
> The polling loop has no sleep, no `select` and no `poll`: it calls the hook
> as fast as the CPU allows whenever no input is pending (ERR-readline-33).
> An application that expects to be called at a leisurely interval will
> instead burn a core. It also reads exactly one `char` byte and widens it to
> `wchar_t` with no multibyte decoding (ERR-readline-32).
>
> Bad values: any non-NULL pointer is called with no validation. The pointer
> is re-read on every loop iteration, so it may be changed or cleared at any
> time, including from inside the hook.
>
> Divergence from GNU readline: the name and signature match and the intent
> is the same, but GNU calls the hook from its own `select`-based wait with a
> timeout, so it is called at a bounded rate rather than spun on; GNU also
> checks `rl_done` after each call, which libedit does not; and GNU's hook
> composes with `rl_getc_function` instead of displacing it.

> [spec:libedit:def:readline.rl-filename-completion-desired]
> extern int rl_filename_completion_desired

> [spec:libedit:sem:readline.rl-filename-completion-desired]
> Exported, and read by no code path at all. Defined in `readline.c` as `int
> rl_filename_completion_desired = 0;`, under the header's "not implemented"
> banner.
>
> Nothing in libedit writes it and nothing reads it. Whether the matches are
> treated as filenames is not a decision libedit makes: `append_char_function`
> always `stat`s the match and appends `/` for a directory, and on the
> readline path it is displaced by
> `_rl_completion_append_character_function`, which appends the append
> character and never `stat`s anything. Setting this global from inside
> `rl_attempted_completion_function` — the GNU idiom for "my matches are
> filenames, quote and mark them accordingly", or for its converse — has no
> effect either way.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.
>
> Divergence from GNU readline: there it is both an output (set to 1 by the
> filename generator) and an input (an application completion function sets
> it to control quoting, the trailing slash and the `visible-stats` marker),
> and readline resets it before each completion. Here neither direction
> exists.

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

> [spec:libedit:def:readline.rl-ignore-completion-duplicates]
> extern int rl_ignore_completion_duplicates

> [spec:libedit:sem:readline.rl-ignore-completion-duplicates]
> Exported, and read by no code path at all. Defined in `readline.c` as `int
> rl_ignore_completion_duplicates = 0;`, under the header's "not implemented"
> banner.
>
> Nothing in libedit writes it and nothing reads it. Neither
> `completion_matches` nor `rl_completion_matches` de-duplicates, and
> `fn_display_match_list` prints whatever it is given, so duplicate matches
> are always collected, always counted towards `rl_completion_query_items`
> and always displayed. Setting this global does not change that. Note also
> that the default here is 0 where GNU readline's is 1, so even a consumer
> that never touches it sees different behaviour.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.

> [spec:libedit:def:readline.rl-inhibit-completion]
> extern int rl_inhibit_completion

> [spec:libedit:sem:readline.rl-inhibit-completion]
> A consumer-to-libedit switch that turns the completion key back into an
> ordinary self-inserting character. Defined in `readline.c` as `int
> rl_inhibit_completion = 0;`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, at the top of `rl_complete` and after the lazy-initialisation guard:
>
> ```c
> if (rl_inhibit_completion) {
>         char arr[2];
>         arr[0] = (char)invoking_key;
>         arr[1] = '\0';
>         el_insertstr(e, arr);
>         return CC_REFRESH;
> }
> ```
>
> so a non-zero value makes TAB (or whatever key reached `rl_complete`)
> insert itself literally into the line and refresh the display. The return
> is `CC_REFRESH`, so the editor treats it as a successful command.
>
> What this short-circuit skips is worth spelling out, because the ordering
> is observable: `rl_completion_word_break_hook` is *not* called,
> `_rl_update_pos` is *not* run — so `rl_point`, `rl_end` and
> `rl_line_buffer`'s terminator all stay at their previous values even though
> a character was just inserted — and `fn_complete2` is never entered, so
> `rl_completion_type` and `rl_attempted_completion_over` are not updated
> either.
>
> The `(char)` narrowing means the key is inserted as a single byte: a
> multibyte character cannot be inserted this way, and `invoking_key == 0`
> makes `arr` the empty string, so nothing is inserted at all while the call
> still reports `CC_REFRESH`.
>
> Any non-zero value inhibits; nothing is validated. The global is read fresh
> on every completion attempt, so it may be toggled at any time — including
> from inside `rl_attempted_completion_function`, though that will not affect
> the completion already in progress.
>
> Divergence from GNU readline: the name and meaning match, and GNU also
> inserts the invoking character. GNU applies it inside
> `rl_complete_internal`, so it covers every completion entry point;
> libedit's test sits only in `rl_complete`, so an application calling
> `fn_complete`/`fn_complete2` directly bypasses it.

> [spec:libedit:def:readline.rl-instream]
> extern FILE *rl_instream

> [spec:libedit:sem:readline.rl-instream]
> The stream libedit reads the line from. Defined in `readline.c` as `FILE
> *rl_instream = NULL;`, where NULL means "not chosen yet" rather than "no
> input".
>
> Written by libedit once, in `rl_initialize`: `if (!rl_instream) rl_instream
> = stdin;`. That is a *defaulting* write, not a reset — a value the consumer
> already stored is left alone — and it is the only libedit write, so after
> the first initialisation the global always reads non-NULL.
>
> Read at four sites, three of them inside `rl_initialize`:
>
> 1. `tcgetattr(fileno(rl_instream), &t)` decides whether to run the editor
>    at all: if the call succeeds and `ECHO` is clear, the editor is put into
>    non-edit mode with `el_set(e, EL_EDITMODE, 0)`. A `tcgetattr` failure —
>    which is what a non-terminal stream produces — leaves edit mode on.
> 2. The `FILE *` itself is passed to `el_init_internal` as the editor's
>    input file.
> 3. `fileno(rl_instream)` is passed separately as the descriptor libedit
>    uses for terminal control, resize handling and readiness polling.
> 4. `_getc_function`, the wrapper installed when `rl_getc_function` is set,
>    passes it to the application's callback on every character.
>
> The timing trap: reads 1–3 happen once, at initialisation, and capture a
> descriptor. Read 4 happens per character and re-reads the global. So
> changing `rl_instream` after initialisation changes what the getc callback
> receives but **not** the descriptor libedit reads from, polls, or applies
> `tcgetattr`/`tcsetattr` to. The two can therefore disagree, and a port must
> reproduce that rather than re-deriving the descriptor.
>
> Ownership and lifetime: the consumer owns the stream. libedit never
> `fclose`s it, never `fflush`es it, and does not keep a copy of the
> pointer — but it does keep the *descriptor*, so closing the stream after
> initialisation leaves libedit using a stale descriptor. Setting it to a
> stream whose `fileno` is -1 (a `fmemopen` stream, for instance) makes every
> descriptor-level operation fail rather than crash.
>
> Bad values: not validated. A NULL stored by the consumer after
> initialisation is not re-defaulted, so `_getc_function` hands NULL to the
> callback; libedit itself does not dereference it after initialisation.
>
> Port note under [dec:libedit:no-c-ffi]: this global is the hardest of the
> two stream globals to express, because `fileno` and `tcgetattr` mean the
> value cannot be treated as an opaque token the way `rl_getc_function`'s
> argument can. The port must be able to obtain a real file descriptor from
> whatever it stores here.
>
> Divergence from GNU readline: same name and meaning, but GNU re-reads
> `rl_instream` on every read and re-derives the descriptor when the terminal
> is prepared, so it can be changed between calls. Here it is effectively
> fixed at initialisation.

> [spec:libedit:def:readline.rl-library-version]
> extern const char *rl_library_version

> [spec:libedit:sem:readline.rl-library-version]
> The library version string a consumer can print. Defined in `readline.c` as
> `const char *rl_library_version = "EditLine wrapper";`.
>
> Written by nobody — neither libedit nor, in practice, the consumer — and
> read by nobody inside libedit. It exists to be read by the application.
>
> The value is the fixed string `"EditLine wrapper"`, and that is the
> specified behaviour: it does not carry libedit's version, it does not carry
> a readline version, and it does not change. A port must export this exact
> byte sequence. Programs do read it — configure scripts and shells print it,
> and some compare it — so changing it to something more informative would be
> a visible behavioural change.
>
> The pointer is writable (the `const` is on the pointee) and libedit never
> reads it back, so a consumer may replace it; nothing depends on the value.
> The initial value points at a string literal, so writing *through* the
> pointer is undefined in the C; the port defines this by backing the default
> with immutable storage.
>
> Divergence from GNU readline: there the string is a dotted release number
> such as `"8.2"`, and `rl_readline_version` is its integer encoding. An
> application that parses `rl_library_version` as a number gets 0 here, and
> one that tests for a minimum version by string comparison gets an answer
> that has nothing to do with capability. `rl_readline_version` is the more
> usable of the two here, since it does at least carry a plausible integer
> ([spec:libedit:sem:readline.rl-readline-version]).

> [spec:libedit:def:readline.rl-line-buffer]
> extern char *rl_line_buffer

> [spec:libedit:sem:readline.rl-line-buffer]
> The line as a byte string, republished for consumers that read readline's
> buffer directly. Defined in `readline.c` as `char *rl_line_buffer = NULL;`;
> the source calls the arrangement out explicitly — "Unfortunately, some
> applications really do use `rl_point` and `rl_line_buffer` directly."
>
> Written by libedit only, through `_resize_fun`, whose whole body is `*ap =
> el_line(el)->buffer`. That function is wired up two ways in
> `rl_initialize`: registered as EditLine's resize callback with `el_set(e,
> EL_RESIZE, _resize_fun, &rl_line_buffer)`, and then called once directly.
> The registration is what makes this global track the buffer, because
> `el_line()` invokes the resize callback as its last step — so **every**
> `el_line()` call anywhere in the library republishes `rl_line_buffer`.
>
> What it points at, and why that matters: `el_line()` re-encodes EditLine's
> wide line into `el->el_lgcyconv`, a conversion buffer that grows on demand
> with `realloc`. `rl_line_buffer` therefore addresses library-internal
> storage that can move. A consumer must re-read the global after any libedit
> call rather than caching the pointer, must never `free` or `realloc` it,
> and must not assume the bytes survive the next call.
>
> Termination: `el_line()` does not necessarily leave a NUL at the end of the
> line — `_rl_update_pos` writes `rl_line_buffer[rl_end] = '\0'` for exactly
> that reason. So the string is properly terminated only after an
> `_rl_update_pos`, which runs at the end of `rl_initialize`, inside
> `rl_complete` (after the word-break hook), inside `rl_bind_wrapper` before
> an application command is dispatched, and at the end of
> `rl_callback_read_char`. Between those points the buffer may carry stale
> bytes past the true end. A consumer reading it from
> `rl_completion_word_break_hook` sees exactly that, since the hook runs
> before the update (ERR-readline-50).
>
> Consumer writes do not edit the line. Nothing propagates a modified
> `rl_line_buffer` back into EditLine's wide line, so an application that
> rewrites the bytes in place sees them discarded at the next `el_line()`
> (ERR-readline-34). The supported route is `rl_insert_text`,
> `rl_delete_text` and `rl_replace_line`. Writing past `rl_end` writes into
> the conversion buffer's slack — or past its end, since the consumer has no
> way to learn its capacity.
>
> Validity: NULL until `rl_initialize` has run, so a consumer that reads it
> without having called `readline()` or any lazily-initialising entry point
> dereferences NULL. `_rl_update_pos` itself would write through a NULL here,
> but every one of its call sites is preceded by initialisation.
>
> Divergence from GNU readline: there `rl_line_buffer` is the editor's own
> storage, is authoritative, is always NUL-terminated, and may be written by
> the application (with `rl_end` updated to match) as a supported way to
> replace the line. Here it is a read-only view that goes stale.

> [spec:libedit:def:readline.rl-linebuf-func-t-const-char-int]
> typedef int rl_linebuf_func_t(const char *, int)

> [spec:libedit:def:readline.rl-linefunc]
> extern rl_vcpfunc_t *rl_linefunc

> [spec:libedit:sem:readline.rl-linefunc]
> The line handler installed for callback-mode editing. Defined in
> `readline.c` as `rl_vcpfunc_t *rl_linefunc = NULL;`, i.e. a pointer to
> `void (char *)`.
>
> It is exported, but it is not meant to be assigned directly: it is the
> storage behind `rl_callback_handler_install`, which sets it, and
> `rl_callback_handler_remove`, which clears it. Read at exactly one site,
> `rl_callback_read_char`, and only when the line just read was terminated —
> `done` non-zero — in which case it is called with the line.
>
> The call is
>
> ```c
> (*(void (*)(const char *))rl_linefunc)(wbuf);
> ```
>
> — the stored `void (*)(char *)` is called through a `void (*)(const char
> *)` type. That is formally undefined behaviour (the two function types are
> not compatible) and harmless on every ABI libedit targets, because the
> parameter is a pointer either way. A port expresses the stored value with
> the declared `rl_vcpfunc_t` shape and simply passes the pointer; there is
> nothing to reproduce beyond the call itself.
>
> What the handler receives:
>
> - A `strdup`'d copy of the line with the terminating newline or carriage
>   return replaced by NUL, when the line ended with one. **Ownership passes
>   to the handler**, which must `free` it. libedit does not free it and
>   keeps no reference.
> - NULL when the line was ended by the terminal's EOF character on an
>   otherwise empty line. This is the readline "end of input" convention.
> - NULL also when the `strdup` failed, which is indistinguishable from
>   end-of-input.
>
> If `rl_linefunc` is NULL when a line completes, the line is silently
> dropped — no handler runs, nothing is freed, and `el_set(e, EL_UNBUFFERED,
> 0)` is not restored either, because that call sits inside the same `if`
> (ERR-readline-37).
>
> `RL_STATE_DONE` is set in `rl_readline_state` immediately before the
> handler is called, but only on the newline path, and nothing ever clears it
> except `rl_initialize`.
>
> Bad values: any non-NULL pointer is called with no validation.
> `rl_callback_handler_remove` is the supported way to clear it; assigning
> NULL directly has the same effect on this global but skips the `el_set(e,
> EL_UNBUFFERED, 0)` that the remove call also performs.
>
> Divergence from GNU readline: there the handler is passed to
> `rl_callback_handler_install` and kept in an internal variable, not
> exported at all — so a consumer reading or writing `rl_linefunc` is using a
> libedit extension. GNU also displays the prompt on install and clears the
> line on remove; libedit's remove leaves the line and the prompt as they
> were.

> [spec:libedit:def:readline.rl-outstream]
> extern FILE *rl_outstream

> [spec:libedit:sem:readline.rl-outstream]
> The stream libedit writes the prompt, the line and its diagnostics to.
> Defined in `readline.c` as `FILE *rl_outstream = NULL;`, where NULL means
> "not chosen yet".
>
> Written by libedit once, in `rl_initialize`: `if (!rl_outstream)
> rl_outstream = stdout;`. A value the consumer already stored is left alone.
>
> Read at four sites, split across two lifetimes:
>
> - **Captured at initialisation.** `el_init_internal` receives the `FILE *`
>   as the editor's output file and `fileno(rl_outstream)` as its output
>   descriptor. Everything the editor itself writes — the prompt, the line,
>   the refresh, the completion listing, the "Display all N possibilities?"
>   query — goes to `el->el_outfile`, which is that captured pointer.
>   Changing the global afterwards does not redirect any of it.
> - **Re-read on each use.** Two `fprintf` calls in the history-expansion
>   code read the global directly: `get_history_event`'s `"%s: Event not
>   found\n"` and `_history_expand_command`'s `"%s: Bad word specifier"` —
>   the latter with no trailing newline, so it runs into whatever is printed
>   next. These two do follow a change to the global.
>
> That split is the specified behaviour and a port must reproduce it: an
> application that redirects `rl_outstream` after initialisation sees its
> history-expansion errors move and everything else stay put.
>
> Ownership and lifetime: the consumer owns the stream. libedit never
> `fclose`s it and never `fflush`es it on these two paths — `fn_complete2`
> does flush before its query, but only `el->el_outfile`. Closing the stream
> after initialisation leaves libedit holding both a stale `FILE *` and a
> stale descriptor.
>
> Bad values: not validated. A NULL stored after initialisation makes the two
> `fprintf` calls undefined; libedit does not check. `fileno` returning -1 is
> not checked either, so a stream with no descriptor produces an editor whose
> output descriptor is -1 and whose terminal operations all fail.
>
> Divergence from GNU readline: same name and meaning, but GNU writes
> everything through the current value of `rl_outstream` rather than through
> a captured copy, so redirecting it takes full effect at any time.

> [spec:libedit:def:readline.rl-point]
> extern int rl_point

> [spec:libedit:sem:readline.rl-point]
> The cursor position within the line. Defined in `readline.c` as `int
> rl_point = 0;` and declared in the header on a shared line with `rl_end`.
> The source explains why it exists: "Unfortunately, some applications really
> do use `rl_point` and `rl_line_buffer` directly."
>
> Written by libedit only — it is an output, never an input — and read by
> libedit nowhere. There are **two** producers, and they do not agree on
> units:
>
> 1. `_rl_update_pos` sets `rl_point = (int)(li->cursor - li->buffer)` from
>    `el_line(e)`, the **narrow** line info, whose pointers address the
>    encoded byte buffer. So this producer yields a **byte** offset. It also
>    sets `rl_end` and writes `rl_line_buffer[rl_end] = '\0'`. It runs at
>    four moments: the end of `rl_initialize`; inside `rl_complete`, after
>    the word-break hook and before `fn_complete2`; inside `rl_bind_wrapper`,
>    before dispatching an application command registered with
>    `rl_add_defun`; and at the end of `rl_callback_read_char`.
> 2. `fn_complete2` sets `*point = (int)(li->cursor - li->buffer)` from
>    `el_wline(el)`, the **wide** line info, whose pointers address
>    `wchar_t`s. So this producer yields a **character** offset. It runs
>    once per completion, after the word has been isolated and immediately
>    before `rl_attempted_completion_function` is called — deliberately, so
>    the hook can read it.
>
> In a single-byte locale, or on a pure-ASCII line, the two agree. On a line
> containing any multibyte character they do not, and nothing marks which
> producer wrote the value last. A consumer that reads `rl_point` from inside
> `rl_attempted_completion_function` gets a character offset; the same
> consumer reading it from inside an `rl_add_defun` command, or after
> `rl_callback_read_char`, gets a byte offset — and `rl_line_buffer`, which
> is what the offset is nominally an index into, is byte-addressed in both
> cases. A port must reproduce both producers with their respective units
> rather than normalising them; see the proposed errata entry accompanying
> this rule set, and note that ERR-readline-35 describes `rl_point`/`rl_end`
> as byte offsets, which holds only for producer 1.
>
> When it is valid: only immediately after one of the five moments above.
> It is **stale** inside `rl_completion_word_break_hook`, which
> `rl_complete` calls before `_rl_update_pos` (ERR-readline-50), and after
> `rl_insert_text` or `rl_delete_text`, neither of which refreshes it
> (ERR-readline-35). It is never refreshed by ordinary editing, so an
> application that reads it outside a callback sees the position as of the
> last completion or command dispatch, not the position now.
>
> Consumer writes have no effect on the line. Nothing propagates a modified
> `rl_point` back into EditLine's wide line, so an application that moves the
> cursor by assigning to it sees the assignment discarded at the next update
> (ERR-readline-34). Moving the cursor requires a real editor command. No
> value is validated, and none can do harm on its own, because libedit never
> uses the global as an index — except via `rl_end`, which
> `_rl_update_pos` immediately recomputes before indexing with it.
>
> Divergence from GNU readline: there `rl_point` is authoritative, always a
> byte offset into `rl_line_buffer`, maintained continuously by every editing
> primitive, and writable by the application as the supported way to move the
> cursor. Here it is a periodically-refreshed report whose units depend on
> which code path last wrote it, and writing it does nothing.

> [spec:libedit:def:readline.rl-pre-input-hook]
> extern rl_hook_func_t *rl_pre_input_hook

> [spec:libedit:sem:readline.rl-pre-input-hook]
> A callback run once per line, just before reading starts. Defined in
> `readline.c` as `rl_hook_func_t *rl_pre_input_hook = NULL;`, i.e. a pointer
> to `int (void)`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, in `readline()`:
>
> ```c
> if (rl_pre_input_hook)
>         (*rl_pre_input_hook)();
> ```
>
> placed after `rl_set_prompt(prompt)` and before the `rl_event_hook`
> installation and `el_gets`. The return value is **discarded**, so the hook
> cannot report failure or influence anything by its result.
>
> When it runs, relative to the rest of `readline()`: after the lazy
> `rl_initialize`, after `rl_startup_hook`, after `tty_init(e)`, after
> `rl_done` has been cleared, after the `setjmp(topbuf)` that
> `_rl_abort_internal` jumps to, and after the prompt has been *set* — but
> before it has been *displayed*, since display happens inside `el_gets`.
> `rl_already_prompted` is cleared after the hook, not before.
>
> Because the `setjmp` precedes it, a hook that triggers
> `_rl_abort_internal` re-enters `readline()`'s body from the top of the read
> and the hook runs again.
>
> It is not reached in callback mode: `rl_callback_read_char` does not call
> it, so an application using `rl_callback_handler_install` never sees it
> fire.
>
> Bad values: any non-NULL pointer is called with no validation. The pointer
> is read fresh on each `readline()`, so it may be installed or cleared
> between calls, and from inside itself.
>
> Divergence from GNU readline: GNU calls its pre-input hook *after* the
> prompt has been displayed and the line has been made ready, so the
> documented idiom — insert some text with `rl_insert_text` and call
> `rl_redisplay` to show it — works there. Here the hook runs before the
> first display, so the `rl_redisplay` is unnecessary and the editor state
> the hook inspects (`rl_point`, `rl_end`, `rl_line_buffer`) is whatever the
> previous line left. GNU also runs it in callback mode; libedit does not.

> [spec:libedit:def:readline.rl-prep-term-function]
> extern rl_vintfunc_t *rl_prep_term_function

> [spec:libedit:sem:readline.rl-prep-term-function]
> The indirection through which the terminal is put back into editing mode
> after a signal. Defined in `readline.c` as
>
> ```c
> rl_vintfunc_t *rl_prep_term_function = (rl_vintfunc_t *)rl_prep_terminal;
> ```
>
> The cast is an identity — `rl_prep_terminal` is already `void (int)` and
> `rl_vintfunc_t` is `void (int)` — so the default value is simply the
> address of the exported `rl_prep_terminal`. A port must export that exact
> default, not NULL and not a private copy, because a consumer may both read
> it and call through it.
>
> Written only by the consumer after initialisation; libedit never reassigns
> it. Read and called at exactly one site, the whole body of
> `rl_reset_after_signal`:
>
> ```c
> if (rl_prep_term_function)
>         (*rl_prep_term_function)(1);
> ```
>
> — NULL-checked, and always called with the argument 1, meaning "enable
> meta-key handling"; libedit never passes 0 here. The default target,
> `rl_prep_terminal(1)`, is `el_set(e, EL_PREP_TERM, 1)`.
>
> So replacing this pointer takes over exactly one thing: what happens when
> the application calls `rl_reset_after_signal`. It does **not** intercept
> the terminal preparation `readline()` does on every line, which goes
> through `tty_init(e)` directly with no indirection. A consumer that
> installs a hook here expecting to see every terminal transition sees one
> call per explicit `rl_reset_after_signal`, and none otherwise.
>
> Bad values: NULL is checked and makes `rl_reset_after_signal` a no-op; any
> other value is called without validation.
>
> Divergence from GNU readline: there this is the indirection readline itself
> uses whenever it prepares the terminal — once per `readline()` call and
> after each signal — and readline passes the value of `rl_catch_signals`-era
> meta configuration through the argument. Here the hook is confined to
> `rl_reset_after_signal` and the argument is the constant 1. Its counterpart
> `rl_deprep_term_function` is never called at all.

> [spec:libedit:def:readline.rl-print-completions-horizontally]
> extern int _rl_print_completions_horizontally

> [spec:libedit:sem:readline.rl-print-completions-horizontally]
> Exported, and read by no code path at all. Defined in `readline.c` as `int
> _rl_print_completions_horizontally = 0;`, under the header's "not
> implemented" banner.
>
> Nothing in libedit writes it and nothing reads it. `fn_display_match_list`
> lays the matches out column-major unconditionally — "On the ith line print
> elements i, i+lines, i+lines*2, etc." — which is the vertical arrangement.
> Setting this global does not switch it to row-major.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.
>
> The leading underscore marks it as one of GNU readline's internals that
> libedit exports anyway so that programs reaching into them still link; see
> the rule [spec:libedit:sem:readline.rl-complete-mark-directories] for the
> others.

> [spec:libedit:def:readline.rl-prompt]
> extern char *rl_prompt

> [spec:libedit:sem:readline.rl-prompt]
> The current prompt string, owned by the readline layer. Defined in
> `readline.c` as `char *rl_prompt = NULL;`.
>
> Written by libedit at three sites, and — this is the crux — the memory is
> libedit's:
>
> - `rl_set_prompt(p)` compares the new text against the current value,
>   returns immediately if they are equal, and otherwise `el_free`s the old
>   buffer and installs `strdup(p)`. It then rewrites `RL_PROMPT_END_IGNORE`
>   markers in place. On `strdup` failure it leaves `rl_prompt` NULL and
>   returns -1.
> - `rl_restore_prompt` assigns `rl_prompt = rl_prompt_saved` **without
>   freeing** the current value, leaking it.
> - `rl_message` formats into a 160-byte stack buffer and installs the result
>   through `rl_set_prompt`, overwriting whatever was there
>   (ERR-readline-36).
>
> Read by libedit at three sites: `_get_prompt`, the `EL_PROMPT_ESC` callback,
> which returns the pointer straight to EditLine; `rl_save_prompt`, which
> `strdup`s it (with no NULL check — ERR-readline-09); and `rl_set_prompt`
> itself, for the equality test and the free.
>
> Ownership rules a consumer must obey, and which nothing enforces:
>
> - The buffer is allocated with `strdup` and released with `el_free`. A
>   consumer that assigns its own pointer — a string literal, a stack buffer,
>   memory from a different allocator — causes the *next* `rl_set_prompt` to
>   free it, which is a heap corruption rather than a diagnosable error.
>   `readline(p)` calls `rl_set_prompt(p)` on every line, so the next one is
>   usually immediate.
> - A consumer that `free`s `rl_prompt` itself leaves a dangling pointer that
>   `_get_prompt` will hand to EditLine and `rl_set_prompt` will free again.
> - Reading it is safe and is the supported use. The pointer is stable
>   between `rl_set_prompt` calls, which is exactly what the equality
>   fast-path in `rl_set_prompt` exists to guarantee: EditLine holds the
>   pointer returned by `_get_prompt` across refreshes, so re-setting the
>   same text must not reallocate.
>
> The stored text is not what the caller passed. Every `RL_PROMPT_END_IGNORE`
> (`'\002'`) byte has been rewritten to `RL_PROMPT_START_IGNORE` (`'\001'`),
> and adjacent end/start pairs have been removed, so a consumer comparing
> `rl_prompt` against its own string will find them unequal whenever
> invisible-text brackets are used.
>
> Validity: NULL until `rl_set_prompt` has succeeded once, which
> `rl_initialize` arranges by calling `rl_set_prompt("")`. Any entry point
> that lazily initialises therefore leaves it non-NULL. `rl_save_prompt`
> called before that point passes NULL to `strdup`.
>
> Divergence from GNU readline: there `rl_prompt` is also readline's own
> storage and also must not be assigned by the application — `rl_set_prompt`
> is the documented setter in both — but GNU stores the prompt with its
> invisible-text brackets intact and computes the visible length separately,
> so the value round-trips.

> [spec:libedit:def:readline.rl-prompt-saved]
> extern char *rl_prompt_saved

> [spec:libedit:sem:readline.rl-prompt-saved]
> The prompt stashed by `rl_save_prompt`. Defined in `readline.c` as `char
> *rl_prompt_saved = NULL;`.
>
> Written by libedit at two sites and read at one, all within the
> save/restore pair:
>
> - `rl_save_prompt`'s entire body is `rl_prompt_saved = strdup(rl_prompt);`.
>   Neither input nor output is checked: `rl_prompt` is NULL until
>   `rl_set_prompt` has succeeded once, so an early call passes NULL to
>   `strdup` (ERR-readline-09), and an allocation failure silently leaves
>   this global NULL. The previous saved value is overwritten without being
>   freed, so saves do not nest and a second save leaks the first copy.
> - `rl_restore_prompt` returns immediately if it is NULL — so a restore
>   without a matching save is a safe no-op — and otherwise assigns
>   `rl_prompt = rl_prompt_saved` and then clears this global to NULL.
>
> Ownership: the buffer is `strdup`'d, i.e. plain `malloc` memory. While it
> sits here it is libedit's; on restore, ownership transfers to `rl_prompt`,
> where the next successful `rl_set_prompt` will release it with `el_free`.
> The allocation and the free are deliberately asymmetric — `strdup` versus
> `el_free` — which matters only to a port that makes those different
> allocators.
>
> A consumer may read it to discover whether a save is outstanding, which is
> the only safe use. Assigning it makes `rl_restore_prompt` install the
> consumer's pointer as `rl_prompt`, which the next `rl_set_prompt` then
> `el_free`s — the same heap-corruption hazard as assigning `rl_prompt`
> directly ([spec:libedit:sem:readline.rl-prompt]). Freeing it leaves a
> dangling pointer that `rl_restore_prompt` will promote to `rl_prompt`.
>
> Validity: non-NULL only between a successful `rl_save_prompt` and the
> following `rl_restore_prompt`. It survives `rl_initialize`, which does not
> clear it, so a save that straddles a re-initialisation restores a prompt
> from the previous editor.
>
> Divergence from GNU readline: GNU has `rl_save_prompt`/`rl_restore_prompt`
> but keeps the saved text in internal storage and exports no such variable,
> so this global is a libedit extension. GNU's pair also saves and restores
> the computed prompt lengths, and its restore redisplays; libedit's restores
> the pointer only and redraws nothing.

> [spec:libedit:def:readline.rl-readline-name]
> extern const char *rl_readline_name

> [spec:libedit:sem:readline.rl-readline-name]
> The application name libedit reports to its configuration machinery.
> Defined in `readline.c` as `const char *rl_readline_name = empty;`, where
> `empty` is the file-static `static char empty[] = { '\0' };` — a
> *writable* one-byte array, not a string literal.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site: `rl_initialize` passes it as `el_init_internal`'s `prog` argument.
> `el_init_internal` immediately converts and copies it —
> `el->el_prog = wcsdup(ct_decode_string(prog, &el->el_scratch))` — so the
> pointer need only stay valid for the duration of that call, and libedit
> never holds it afterwards. The copy is freed by `el_end`.
>
> What the name is for: EditLine's `.editrc` parser matches `prog:`-prefixed
> lines against it, so a consumer setting `rl_readline_name = "myshell"`
> before the first `readline()` makes `myshell:bind …` lines in `~/.editrc`
> apply. The default empty string matches no `prog:` line, so only unprefixed
> lines apply.
>
> The timing trap, and it is the same one `rl_catch_signals` has: the value
> is read once, when the editor is created. Setting it after initialisation
> has no effect until another `rl_initialize()`, which tears down and rebuilds
> both the editor and the history list. Consumers must set it before the
> first call to `readline()` or any other lazily-initialising entry point.
>
> Bad values: not validated. A NULL reaches `ct_decode_string`, which returns
> NULL for a NULL input, and `wcsdup(NULL)` is then undefined; the port
> defines this as the empty name. A string not representable in the current
> locale also makes the decode return NULL and takes the same path.
>
> The default points at mutable file-static storage despite the `const char
> *` declaration, so a consumer may overwrite the empty string in place —
> though with one byte of room there is nothing useful to write. A port backs
> it with process-lifetime storage.
>
> Divergence from GNU readline: same name and the same "set it before you
> call readline" contract, but GNU defaults it to `"other"` rather than the
> empty string, and uses it for `$if` conditionals in `~/.inputrc` — a file
> libedit never reads at all (ERR-readline-46). So the variable works, and
> works on a different configuration file with a different syntax.

> [spec:libedit:def:readline.rl-readline-state]
> extern unsigned long rl_readline_state

> [spec:libedit:sem:readline.rl-readline-state]
> A bitmask reporting what the library is doing. Defined in `readline.c` as
> `unsigned long rl_readline_state = RL_STATE_NONE;`, i.e. 0.
>
> The header defines exactly two state values — `RL_STATE_NONE` (0x000000)
> and `RL_STATE_DONE` (0x000001) — together with the accessor macros
> `RL_SETSTATE`, `RL_UNSETSTATE` and `RL_ISSTATE`, all of which expand to
> plain reads and writes of this global with an `unsigned long` cast. None of
> GNU readline's other `RL_STATE_*` bits exist here, so a consumer that
> writes `RL_ISSTATE(RL_STATE_INITIALIZED)` does not compile.
>
> Written by libedit at exactly two sites, both touching only the one bit:
>
> - `rl_initialize` clears it with `RL_UNSETSTATE(RL_STATE_DONE)`.
> - `rl_callback_read_char` sets it with `RL_SETSTATE(RL_STATE_DONE)`, just
>   before calling `rl_linefunc`, and only on the newline-terminated path —
>   not on the EOF path.
>
> Read by libedit nowhere. So the bit is set once per completed line in
> callback mode and cleared only by a full re-initialisation
> (ERR-readline-37): after the first line, `RL_ISSTATE(RL_STATE_DONE)` is
> true forever. A consumer using it to detect line completion sees it stick.
> Nothing in the `readline()` path touches it at all, so a non-callback
> application always reads 0.
>
> The consumer may write it, and since libedit never reads it, a write only
> affects what the consumer itself sees next. `RL_SETSTATE`/`RL_UNSETSTATE`
> are non-atomic read-modify-write macros with no locking, like every global
> in this layer.
>
> A port must export it as a mutable `unsigned long` — the width matters,
> because the macros are what consumers use and they cast to `unsigned long`
> — initialised to 0, set the one bit at the one site and clear it at the
> other, and read it nowhere.
>
> Divergence from GNU readline: there the mask carries a dozen live bits
> (`INITIALIZING`, `READCMD`, `ISEARCH`, `SIGHANDLER`, `CALLBACK`, …), is
> maintained continuously, and is the documented way for a signal handler or
> an event hook to discover what readline is in the middle of. Here one bit
> exists, is set once and never cleared.

> [spec:libedit:def:readline.rl-readline-version]
> extern int rl_readline_version

> [spec:libedit:sem:readline.rl-readline-version]
> The readline API version libedit claims to implement. Defined in
> `readline.c` as `int rl_readline_version = RL_READLINE_VERSION;`, and the
> header defines `RL_READLINE_VERSION` as `0x0402` — readline 4.2, in
> readline's own major/minor-nibble encoding.
>
> Written by nobody and read by nobody inside libedit. It exists to be read
> by the application, and the value is a claim rather than a measurement: the
> layer implements a subset of the 4.2 API, with the divergences this
> document records, and does not track later readline releases.
>
> A port must export the exact value 0x0402 as a mutable `int`. Consumers do
> branch on it — `#if`-style feature tests written against readline compare
> it to constants like `0x0402` to decide whether `rl_completion_matches` or
> the older `completion_matches` is available — so raising it would advertise
> entry points that do not exist here, and lowering it would hide ones that
> do.
>
> The consumer may write it; nothing reads it back.
>
> Divergence from GNU readline: the encoding matches, the number does not
> match any readline release libedit actually tracks, and its string
> counterpart `rl_library_version` carries `"EditLine wrapper"` rather than a
> parseable version, so the two cannot be cross-checked
> ([spec:libedit:sem:readline.rl-library-version]).

> [spec:libedit:def:readline.rl-redisplay-function]
> extern rl_voidfunc_t *rl_redisplay_function

> [spec:libedit:sem:readline.rl-redisplay-function]
> Exported, and read by no code path at all. Defined in `readline.c` as
> `rl_voidfunc_t *rl_redisplay_function = NULL;`, i.e. a pointer to `void
> (void)`. ERR-readline-54 records it among the inert exports.
>
> Nothing in libedit writes it and nothing reads it. Redisplay is
> EditLine's, reached directly: `rl_redisplay` pushes the terminal's reprint
> character and calls `rl_forced_update_display`, which is `el_set(e,
> EL_REFRESH)`, and neither consults this hook. A consumer that installs its
> own display routine here finds libedit still drawing the line itself.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable nullable function pointer and never call
> through it. Note the asymmetry with `rl_prep_term_function`, which looks
> similar but *is* called: a port cannot treat "function pointer with a
> non-obvious purpose" as a single category.
>
> Divergence from GNU readline: there the hook, when set, replaces
> `rl_redisplay` wholesale and readline calls through it for every refresh —
> the standard way an application takes over the display, used by programs
> that draw the line inside their own UI.

> [spec:libedit:def:readline.rl-sort-completion-matches]
> extern int rl_sort_completion_matches

> [spec:libedit:sem:readline.rl-sort-completion-matches]
> Exported, and read by no code path at all. Defined in `readline.c` as the
> tentative definition `int rl_sort_completion_matches;`, so it starts at 0 —
> where GNU readline's starts at 1, so the default disagrees as well.
>
> Nothing in libedit writes it and nothing reads it, and the sorting that
> does happen is not conditional on anything. `rl_completion_matches` sorts
> its results — through a comparator that in fact compares pointer bytes
> rather than strings (ERR-readline-01) — while `completion_matches` does not
> sort at all, and `fn_display_match_list` always sorts the copy it is about
> to print with `_fn_qsort_string_compare`. Clearing this global suppresses
> none of that.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable `int` initialised to 0 and never consult
> it. ERR-readline-54 collects the inert exports; this one is not currently
> among them.

> [spec:libedit:def:readline.rl-special-prefixes]
> extern const char *rl_special_prefixes

> [spec:libedit:sem:readline.rl-special-prefixes]
> Exported, and read by no code path at all. Defined in `readline.c` as
> `const char *rl_special_prefixes = NULL;`, with the source comment "List of
> characters which are word break characters, but should be left in the
> parsed text when it is passed to the completion function. Shell uses this
> to help determine what kind of completing to do." ERR-readline-50 records
> it, together with `rl_completer_word_break_characters`, as declared but
> consulted nowhere.
>
> Nothing in libedit writes it and nothing reads it. The irony is that
> `fn_complete2` *does* take a special-prefixes argument and
> `find_word_to_complete` *does* use it — `rl_complete` simply fills that
> slot with `rl_completion_word_break_hook`'s return value (or, absent a
> hook, with `rl_basic_word_break_characters` again) instead of with this
> global. So the mechanism this variable names is live and reachable; the
> variable is not connected to it.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it. A consumer that needs the effect must install
> `rl_completion_word_break_hook` and return the set from there
> ([spec:libedit:sem:readline.rl-completion-word-break-hook-fn]) — noting
> that in libedit the special-prefix set is consulted identically to the
> word-break set, so the "left in the parsed text" half of the documented
> behaviour does not happen either.
>
> Ownership: whatever a consumer stores, the consumer owns; libedit never
> frees, copies or dereferences it, so no value is invalid — which stops
> being true the moment a port starts consulting it.
>
> Divergence from GNU readline: there the characters in this set break the
> word but are *retained* at the front of the text handed to the completion
> function, which is how a shell tells `$VAR` completion from `@host`
> completion from plain filename completion. None of that is available here.

> [spec:libedit:def:readline.rl-startup-hook]
> extern rl_hook_func_t *rl_startup_hook

> [spec:libedit:sem:readline.rl-startup-hook]
> A callback run at the top of every `readline()` call. Defined in
> `readline.c` as `rl_hook_func_t *rl_startup_hook = NULL;`, i.e. a pointer
> to `int (void)`.
>
> Written only by the consumer; libedit never assigns it. Read at exactly one
> site, in `readline()`:
>
> ```c
> if (rl_startup_hook) {
>         (*rl_startup_hook)();
> }
> ```
>
> The return value is **discarded**.
>
> When it runs, and this ordering is the whole of its behaviour: after the
> lazy `if (e == NULL || h == NULL) rl_initialize();` and before everything
> else — before `tty_init(e)`, before `rl_done` is cleared, before the
> `setjmp(topbuf)` that `_rl_abort_internal` needs, before `rl_set_prompt`
> installs the prompt this call was given, and before `rl_pre_input_hook`.
>
> Three consequences a port must reproduce. The editor exists when the hook
> runs, so calling `rl_bind_key`, `rl_add_defun`, `rl_parse_and_bind` or
> `add_history` from it works — this is the intended use. The terminal is
> **not** yet in editing mode, so anything the hook does that assumes raw
> mode is premature. And `rl_prompt` still holds the *previous* call's
> prompt, so a hook that inspects it does not see the prompt this call will
> display; nor can it usefully set the prompt, since `rl_set_prompt` runs
> afterwards and overwrites it.
>
> Because it runs before the `setjmp`, a hook that triggers
> `_rl_abort_internal` jumps into whatever `topbuf` held from a previous call
> — a dead frame if none is active (ERR-readline-10).
>
> It is not reached in callback mode: `rl_callback_read_char` does not call
> it, so an application using `rl_callback_handler_install` never sees it
> fire, though `rl_callback_handler_install` does trigger the same lazy
> `rl_initialize`.
>
> Bad values: any non-NULL pointer is called with no validation. The pointer
> is read fresh on each `readline()`.
>
> Divergence from GNU readline: same name, same signature, same
> discarded-return convention, and GNU also calls it once per `readline()`.
> GNU calls it *after* the terminal has been prepared and the line
> initialised, so the documented idiom of seeding the line with
> `rl_insert_text` from the startup hook works there and here operates on the
> previous line's state.

> [spec:libedit:def:readline.rl-startup1-hook]
> extern rl_hook_func_t *rl_startup1_hook

> [spec:libedit:sem:readline.rl-startup1-hook]
> Exported, and called by no code path at all. Defined in `readline.c` as
> `rl_hook_func_t *rl_startup1_hook = NULL;`, i.e. a pointer to `int (void)`.
>
> Nothing in libedit writes it and nothing reads it. `readline()` calls
> `rl_startup_hook` and `rl_pre_input_hook` and nothing else; there is no
> second startup callback anywhere in the layer. A consumer that installs one
> here waits forever.
>
> Two things make this one worth calling out separately from the other inert
> exports. First, it is declared **above** the header's `/* The following is
> not implemented */` banner, among the entries a reader is entitled to
> assume work — the only global in the supported block that does nothing at
> all. Second, it has no GNU readline counterpart under this name, so there
> is no external documentation a consumer could check against; the name
> appears to anticipate a "hook that runs once, on the first call" that was
> never written.
>
> Storing and never reading is the specified behaviour and a port must
> reproduce it: export a writable nullable function pointer and never call
> through it. ERR-readline-54 collects the inert exports; this one is not
> currently among them.

> [spec:libedit:def:readline.rl-terminal-name]
> extern char *rl_terminal_name

> [spec:libedit:sem:readline.rl-terminal-name]
> The terminal type, used as an input if the consumer set one and written as
> an output if it did not. Defined in `readline.c` as `char *rl_terminal_name
> = NULL;`.
>
> Read and written at one site, the two arms of a single test in
> `rl_initialize`:
>
> ```c
> if (rl_terminal_name != NULL)
>         el_set(e, EL_TERMINAL, rl_terminal_name);
> else
>         el_get(e, EL_TERMINAL, &rl_terminal_name);
> ```
>
> So a non-NULL value is consumed as the terminal type to load termcap
> entries for — the same effect as `TERM` in the environment, and it wins
> over it. A NULL value is replaced with whatever type EditLine settled on.
>
> The ownership of that written-back value is the trap, and a port must
> reproduce it rather than tidy it. `el_get(EL_TERMINAL, …)` returns
> `el->el_terminal.t_name`, which `terminal_set` assigns **without copying**
> from one of three sources: the string the caller passed to
> `el_set(EL_TERMINAL, …)`; the pointer `getenv("TERM")` returned; or the
> string literal `"dumb"` when `TERM` is unset, empty, or the termcap lookup
> failed. So after `rl_initialize` the global may point at
>
> - the consumer's own buffer,
> - the process environment — invalidated by any later `setenv`, `putenv` or
>   `unsetenv`, and never safe to write through, or
> - read-only literal storage.
>
> The declared type is `char *`, which invites a consumer to write through
> it; two of those three cases make that undefined behaviour, and all three
> make `free(rl_terminal_name)` a corruption. libedit never frees it and
> never copies it. A consumer that needs to keep the value must copy it
> immediately.
>
> Note also the const mismatch at the call site: `el_get`'s `EL_TERMINAL`
> case reads a `const char **` from the varargs list while `&rl_terminal_name`
> is a `char **`. The two are not compatible types, but they have identical
> representation, so this is a diagnostic-free defect rather than a runtime
> one — of a piece with the `completion_matches` prototype mismatch
> (ERR-readline-53).
>
> Timing: read and written once, at initialisation. Setting it afterwards has
> no effect until another `rl_initialize()`, which tears down and rebuilds
> the editor and the history. Reading it before initialisation yields NULL.
>
> Divergence from GNU readline: there `rl_terminal_name` is consulted by
> `rl_reset_terminal(NULL)` as well as at startup, so it can be changed and
> re-applied; and readline leaves it holding the environment's `TERM` string
> without ever substituting `"dumb"`.

> [spec:libedit:def:readline.rl-vcpfunc-t-char]
> typedef void rl_vcpfunc_t(char *)

> [spec:libedit:def:readline.rl-vintfunc-t-int]
> typedef void rl_vintfunc_t(int)

> [spec:libedit:def:readline.rl-voidfunc-t-void]
> typedef void rl_voidfunc_t(void)

