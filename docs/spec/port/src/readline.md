# src/readline.c

> [spec:libedit:def:readline.add-history-fn]
> int add_history(const char *line)

> [spec:libedit:sem:readline.add-history-fn]
> Appends `line` to the history as the new newest event.
>
> Steps: if either module-static `h` (the narrow `History *`) or `e` (the
> `EditLine *`) is NULL, calls `rl_initialize()` first. Then calls
> `history(h, &ev, H_ENTER, line)`, which stores a private copy of `line` at
> the front of the list; H_ENTER yields 1 on insert, 0 when the entry was
> suppressed as a duplicate of the current newest (only possible with
> H_UNIQUE, which this layer never enables), and -1 on failure. On -1 the
> function returns 0 immediately, touching no global.
>
> Otherwise it re-reads the event count with `history(h, &ev, H_GETSIZE)`
> and updates the exported globals: if `ev.num == history_length` (the count
> did not change, meaning the list was already at its H_SETSIZE cap and the
> oldest event was evicted to make room) it increments `history_base` and
> leaves `history_length` and `history_offset` alone; otherwise it
> increments `history_offset` and sets `history_length = ev.num`.
>
> Ownership: `line` is copied, the caller keeps it. Return value: always 0,
> so success and allocation failure are indistinguishable. GNU readline
> declares `add_history` as `void`; libedit returns `int` and the value
> carries no information.
>
> Divergence trap: the duplicate-suppression path also leaves the count
> unchanged, so it bumps `history_base` as though an eviction had happened.

> [spec:libedit:def:readline.append-history-fn]
> int append_history(int n, const char *filename)

> [spec:libedit:sem:readline.append-history-fn]
> Appends the `n` most recent history events to a file, without rewriting
> what is already there.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. If `filename` is
> NULL, substitutes `_default_history_file()`; if that also returns NULL,
> returns the current value of `errno` (whatever `getpwuid` left).
> `fopen(filename, "a")`; on failure returns `errno`. Then calls `history(h,
> &ev, H_NSAVE_FP, (size_t)n, fp)`.
>
> H_NSAVE_FP writes the history-file signature line only when the stream is
> positioned at offset 0, so appending to a non-empty file adds no second
> header. It positions from the newest event forward `n` steps and then
> writes from there back toward the newest, i.e. the last `n` entries in
> oldest-first order, each `strvis`-escaped with VIS_WHITE and followed by a
> newline. This on-disk encoding is part of the frozen ABI.
>
> If the history call returns -1, captures `errno` (or EINVAL if `errno` is
> 0), closes the file and returns that value. Otherwise closes the file and
> returns 0.
>
> Return convention: 0 on success, a positive errno value on failure; never
> -1. The caller passes an already-open-able path; this function owns and
> closes the FILE it opened.

> [spec:libedit:def:readline.clear-history-fn]
> void clear_history(void)

> [spec:libedit:sem:readline.clear-history-fn]
> Deletes every history entry.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL, then `history(h,
> &ev, H_CLEAR)`, which frees every event and its string, resets the
> internal cursor to the list head, and resets the internal event-id counter
> to 0 (so the next entered event is numbered 1 again). Any `histdata_t`
> attached to entries is dropped without being freed — the history layer
> never owned it.
>
> Then sets the exported globals `history_offset` and `history_length` both
> to 0. `history_base` is deliberately left at whatever value
> `add_history`/`stifle_history` last set it to, so after a clear the base
> can still be greater than 1 and `history_get()` will reject small indices.
>
> Returns nothing. Failure of H_CLEAR is not detectable.

> [spec:libedit:def:readline.current-history-fn]
> HIST_ENTRY * current_history(void)

> [spec:libedit:sem:readline.current-history-fn]
> Returns the history entry at the current `history_offset`.
>
> There is no lazy-initialization guard: `h` is passed straight to
> `history()`, so calling this before any other readline entry point
> dereferences NULL.
>
> Steps: `history(h, &ev, H_PREV_EVENT, history_offset + 1)` — starting from
> the internal cursor's current position this walks toward *newer* entries
> looking for the event whose `ev.num` equals `history_offset + 1`. If no
> such event exists it returns NULL and the cursor is left at the newest
> end. Note the implied identity between a zero-based offset and a one-based
> libedit event number, which only holds while event numbering is dense and
> starts at 1.
>
> On success it fills the file-static `HIST_ENTRY rl_he`: `rl_he.line =
> ev.str` (borrowed, owned by the history, and including whatever trailing
> newline the entry was stored with) and `rl_he.data = NULL` — the entry's
> real `histdata_t` is never surfaced here. Returns `&rl_he`.
>
> Ownership: the returned pointer is to a single shared static; it is
> overwritten by the next `current_history()` call and therefore also by
> `previous_history()` and `next_history()`, which both end in a call to
> this function. The caller must not free it or `->line`.

> [spec:libedit:def:readline.default-history-file-fn]
> static const char * _default_history_file(void)

> [spec:libedit:sem:readline.default-history-file-fn]
> Computes (and caches) the fallback history file path `$HOME/.history`
> derived from the password database, not from the environment.
>
> Steps: a function-static `char *path` acts as a permanent cache; if it is
> already non-NULL it is returned immediately with no further work.
> Otherwise `getpwuid(getuid())`; if that returns NULL the function returns
> NULL (leaving `errno` as `getpwuid` set it — callers such as
> `read_history` return that errno to their own caller).
>
> Computes `len = strlen(pw->pw_dir) + sizeof("/.history")`, i.e. the home
> directory length plus 10 (9 characters plus the NUL), allocates it with
> `el_malloc`, and on allocation failure returns NULL leaving the cache NULL
> so a later call retries. Formats with `snprintf(path, len, "%s/.history",
> pw->pw_dir)` and returns `path`.
>
> Ownership: the buffer is never freed — an intentional once-per-process
> leak. The returned `const char *` stays valid for the lifetime of the
> process and callers must not free it. Uses `getpwuid`'s static `passwd`
> storage and an unsynchronized static, so it is not thread-safe. Note it
> consults the passwd entry for the real uid, so `$HOME` is ignored.

> [spec:libedit:def:readline.el-rl-complete-fn]
> static unsigned char _el_rl_complete(EditLine *el __attribute__((__unused__)), int ch)

> [spec:libedit:sem:readline.el-rl-complete-fn]
> EditLine-shaped adapter that lets libedit's key map invoke `rl_complete`.
>
> The `el` parameter is unused; the module always works through the
> module-static `e`. The body is `return (unsigned char)rl_complete(0, ch)`
> — the first argument (readline's ignored `count`) is hardcoded to 0 and
> `ch` (the invoking key) is passed through.
>
> `rl_complete` returns a libedit CC_* code, not a readline status, and
> every CC_* value is small, so the narrowing cast to `unsigned char` is
> lossless in practice. The returned code is what the editor loop acts on:
> CC_NORM (0), CC_REFRESH (4), CC_ERROR (6) and so on.
>
> Registered by `rl_initialize` with `el_set(e, EL_ADDFN, "rl_complete",
> "ReadLine compatible completion function", _el_rl_complete)` and bound to
> `^I` — this is why Tab completes in readline emulation mode.

> [spec:libedit:def:readline.el-rl-tstp-fn]
> static unsigned char _el_rl_tstp(EditLine *el __attribute__((__unused__)), int ch __attribute__((__unused__)))

> [spec:libedit:sem:readline.el-rl-tstp-fn]
> EditLine-shaped adapter that suspends the process on `^Z`.
>
> Both parameters are unused. Calls `raise(SIGTSTP)`, whose return value is
> discarded, then returns CC_NORM (0).
>
> `raise` delivers SIGTSTP to the calling thread, so the process stops
> unless the signal is blocked, ignored or handled. Terminal state around
> the stop is EditLine's business (libedit installs its own handlers on
> entry to `el_gets` and clears them on the way out); this function does
> nothing about it. When the process is continued, control returns from
> `raise`, CC_NORM is returned, and editing resumes on the same line.
>
> Registered by `rl_initialize` with `el_set(e, EL_ADDFN, "rl_tstp",
> "ReadLine compatible suspend function", _el_rl_tstp)` and bound to `^Z`.

> [spec:libedit:def:readline.filename-completion-function-fn]
> char * filename_completion_function(const char *name, int state)

> [spec:libedit:sem:readline.filename-completion-function-fn]
> Pure forwarder: `return fn_filename_completion_function(name, state)`. No
> lazy initialization, no globals read or written, no allocation of its own.
>
> It exposes libedit's filename generator under readline's older un-prefixed
> name; `rl_filename_completion_function` is the identical forwarder under
> the newer name, and both may be used interchangeably.
>
> Generator protocol as seen by the caller: `state == 0` begins a fresh scan
> for the (possibly partial, possibly tilde-prefixed) pathname `name`; any
> non-zero `state` continues the scan started by the preceding call. Each
> call returns the next matching pathname as a freshly allocated
> NUL-terminated string that the caller must release with `free()`, and
> returns NULL once the matches are exhausted. Scan state lives in statics
> inside `fn_filename_completion_function`, so interleaving two scans is not
> supported.

> [spec:libedit:def:readline.free-history-entry-fn]
> histdata_t free_history_entry(HIST_ENTRY *he)

> [spec:libedit:sem:readline.free-history-entry-fn]
> Documented stub that frees nothing.
>
> The entire body is `return he ? NULL : NULL;` — the parameter is read only
> to suppress an unused-parameter warning, and both arms of the conditional
> evaluate to NULL. Neither `he`, nor `he->line`, nor `he->data` is
> released, and no global is touched. Always returns NULL.
>
> GNU readline's `free_history_entry` frees the entry's line and the
> `HIST_ENTRY` itself and returns the entry's `histdata_t` so the caller can
> dispose of it. Under libedit the readline-compatible allocation performed
> by `remove_history()` (an `el_malloc`'d `HIST_ENTRY` whose `line` is a
> fresh copy made by H_DELDATA) is therefore leaked by any program that
> follows readline's documented idiom of pairing `remove_history()` with
> `free_history_entry()`.
>
> The Rust port must reproduce the stub, not the readline behaviour: making
> it actually free would turn today's leak into a double free in programs
> that already free the entry themselves.

> [spec:libedit:def:readline.get-history-event-fn]
> const char * get_history_event(const char *cmd, int *cindex, int qchar)

> [spec:libedit:sem:readline.get-history-event-fn]
> Parses one csh-style history *event specifier* starting at `cmd[*cindex]`
> and returns the text of the referenced history line. `qchar` is a quote
> character that additionally terminates a bare word reference (the caller
> passes `'"'` when the `!` was preceded by a double quote, else 0).
>
> Order of operations:
>
> 1. `idx = *cindex`; if `cmd[idx++] != history_expansion_char` (default
>    `'!'`) returns NULL without touching `*cindex`. `idx` now indexes the
>    character after the `!`.
> 2. If `cmd[idx]` is another `history_expansion_char` or `'\0'` (i.e. `!!`
>    or a trailing `!`): `history(h, &ev, H_FIRST)` positions the cursor at
>    the newest event; on failure returns NULL. Sets `*cindex = cmd[idx] ?
>    idx + 1 : idx` and returns `ev.str`.
> 3. Otherwise, a leading `'-'` sets `sign = 1` and advances `idx`.
> 4. If a decimal digit follows, accumulates digits into `num` (no overflow
>    check). If `sign`, rewrites `num = history_length - num +
>    history_base`, turning `!-n` into an absolute index. Calls
>    `history_get(num)`; if that returns NULL, returns NULL. Otherwise sets
>    `*cindex = idx` and returns `he->line` — which points at
>    `history_get`'s own static `HIST_ENTRY`, so the result is invalidated
>    by the next `history_get`.
> 5. Otherwise this is a word reference. A leading `'?'` sets `sub = 1`
>    (search anywhere in the line rather than at the start) and advances
>    `idx`. `begin = idx`. The pattern then runs to the first `'\n'`; to a
>    `'?'` if `sub`; or, if not `sub`, to any of `':'`, `' '`, `'\t'` or
>    `qchar`; or to end of string. `len = idx - begin`. If `sub` and the
>    terminator was `'?'`, `idx` is advanced past it.
> 6. Pattern selection: if `sub` and `len == 0` and the static
>    `last_search_pat` is non-NULL and non-empty, reuses `last_search_pat`
>    (borrowed, not copied). Else if `len == 0`, returns NULL. Else
>    allocates `len + 1` bytes with `el_calloc` and `strlcpy`s the pattern
>    text into it; allocation failure returns NULL.
> 7. `history(h, &ev, H_CURR)` records the current event number in `num` for
>    later restoration; on failure frees the pattern (only if it is not the
>    borrowed `last_search_pat`) and returns NULL.
> 8. If `sub`: takes ownership of the pattern into the static
>    `last_search_pat` (freeing the previous one) unless it already is that
>    pointer, then `ret = history_search(pat, -1)`. If not `sub`: `ret =
>    history_search_prefix(pat, -1)`. Both search toward older entries and
>    leave the cursor on the match.
> 9. If `ret == -1` (not found): `history(h, &ev, H_FIRST)` to restore the
>    cursor to the newest entry, prints `"<pat>: Event not found\n"` to
>    `rl_outstream`, frees the pattern if it is not `last_search_pat`, and
>    returns NULL.
> 10. If `sub && len` (a fresh `?pat?`), frees `last_search_match` and
>     replaces it with `strdup(pat)` — this is what the `:%` word designator
>     later expands to. The strdup result is not checked.
> 11. Frees the pattern if it is not `last_search_pat`.
> 12. `history(h, &ev, H_CURR)`; on failure returns NULL (with `*cindex`
>     still unset). Sets `*cindex = idx`, takes `rptr = ev.str`, then
>     `history(h, &ev, H_SET, num)` rolls the cursor back to where it was
>     before the search, and returns `rptr`.
>
> Return value: a borrowed pointer into history-owned storage (or into
> `history_get`'s static entry). The caller must not free it, and it is
> invalidated by anything that deletes or replaces that history entry. NULL
> means "no event matched" or "parse failed"; the two are not distinguished.
> `*cindex` is advanced past the consumed specifier only on success.
>
> Statics used: `last_search_pat` (the last `!?pat?` pattern, owned here,
> never freed at shutdown) and `last_search_match` (a copy of the last
> successful `?pat?`, also owned here and never freed at shutdown).

> [spec:libedit:def:readline.get-prompt-fn]
> static char * _get_prompt(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:readline.get-prompt-fn]
> Prompt callback handed to EditLine; libedit calls it whenever it needs the
> prompt text to draw.
>
> The `el` parameter is unused. Sets the exported global
> `rl_already_prompted = 1` as a side effect — `readline()` clears it to 0
> immediately before calling `el_gets`, so an application can tell whether
> the prompt has been emitted for the current line. Returns `rl_prompt`, the
> module-owned prompt buffer, which is NULL until `rl_set_prompt` has
> succeeded at least once (in practice `rl_initialize` calls
> `rl_set_prompt("")`).
>
> Ownership: the returned pointer is borrowed; libedit must not free or
> modify it, and `rl_set_prompt` may `el_free` and replace it between calls.
>
> Registered by `rl_initialize` as `el_set(e, EL_PROMPT_ESC, _get_prompt,
> RL_PROMPT_START_IGNORE)`. The escape character is `RL_PROMPT_START_IGNORE`
> (`'\001'`) alone, because libedit's prompt renderer toggles "invisible"
> mode on each occurrence of one delimiter; `rl_set_prompt` has already
> rewritten every `RL_PROMPT_END_IGNORE` (`'\002'`) into `'\001'` so
> readline's start/end bracketing collapses onto that single toggle
> character.

> [spec:libedit:def:readline.getc-function-fn]
> static int /*ARGSUSED*/ _getc_function(EditLine *el __attribute__((__unused__)), wchar_t *c)

> [spec:libedit:sem:readline.getc-function-fn]
> EL_GETCFN adapter that reads one character through the application's
> `rl_getc_function`.
>
> Installed by `rl_initialize` only if `rl_getc_function` was non-NULL at
> that moment; there is no re-check here, so an application that clears
> `rl_getc_function` afterwards causes a NULL function-pointer call.
>
> Steps: `i = (*rl_getc_function)(rl_instream)` — the hook is called with
> the current value of the exported global `rl_instream`, not with
> EditLine's own input `FILE *`, so redirecting `rl_instream` after
> initialization changes what the hook is asked to read. If `i == -1` the
> function returns 0, which EditLine reads as end of input. Only exactly -1
> is treated as EOF; any other negative value falls through and is stored as
> a character. Otherwise `*c = (wchar_t)i` and it returns 1.
>
> The `el` parameter is unused. No multibyte decoding happens: the `int`
> from the hook is widened directly to `wchar_t`, so under a UTF-8 locale an
> application hook returning raw bytes produces mojibake — one call yields
> exactly one wide character. Return value: 1 = one character stored in
> `*c`, 0 = end of input.

> [spec:libedit:def:readline.getfrom-fn]
> static int getfrom(const char **cmdp, char **fromp, const char *search, int delim)

> [spec:libedit:sem:readline.getfrom-fn]
> Parses the *from* half of an `s/from/to/` history modifier out of `*cmdp`,
> up to the first unescaped `delim`.
>
> `*fromp` is an in/out slot holding a buffer reused across calls (the
> caller passes the address of a function-static `from` pointer); `search`
> is a fallback pattern used when the parsed text is empty.
>
> Steps: `size = 16`, `len = 0`, `cmd = *cmdp`. Reallocates the caller's
> existing buffer to 16 bytes: `what = el_realloc(*fromp, 16)`. If that
> fails, frees `*fromp`, sets `*fromp = NULL` and returns 0 — note 0 is
> neither of the documented outcomes, and callers treat it as failure.
>
> Scan loop, for each `*cmd` until end of string or an unescaped `delim`: if
> `*cmd == '\\'` and `cmd[1] == delim`, `cmd` is advanced once so the
> backslash is dropped and the delimiter is taken literally. Then the growth
> test `if (len - 1 >= size)` doubles `size` and reallocates; on failure it
> frees `what`, frees `*fromp`, sets `*cmdp = cmd`, `*fromp = NULL` and
> returns 0. Finally `what[len++] = *cmd`.
>
> The growth test is wrong in two ways and both are observable. `len` is
> `size_t`, so on the first iteration `len - 1` wraps to SIZE_MAX and the
> buffer is always grown from 16 to 32 before the first byte is stored.
> Thereafter the condition only fires at `len >= size + 1`, so the byte at
> index `size` is written *before* the growth — a one-byte heap overflow
> each time `len` reaches the current capacity (at 32, then 64, then 128,
> ...). The trailing `what[len] = '\0'` after the loop can land one past the
> end for the same reason. The Rust port must not reproduce the overflow; it
> should grow when `len == size`.
>
> After the loop: `what[len] = '\0'`, `*fromp = what`, `*cmdp = cmd`.
>
> If the parsed text is empty (`*what == '\0'`): frees `what` while `*fromp`
> still points at it, then if `search` is non-NULL sets `*fromp =
> strdup(search)` (returning 0 if that fails), otherwise sets `*fromp =
> NULL` and returns -1. The `search != NULL` arm is dead at the only call
> site — `_history_expand_command` initializes its `search` local to NULL
> and never assigns it — but if it were reached, control would fall through
> to the next check and free `what` a second time.
>
> If `*cmd` is NUL (no closing delimiter): frees `what`, sets `*fromp =
> NULL`, returns -1. Otherwise skips the delimiter (`cmd++`), stores `*cmdp
> = cmd`, and if the string ends right there frees `what`, sets `*fromp =
> NULL` and returns -1 — so `!!:s/foo/` with nothing after the second
> delimiter is an error rather than a deletion, unlike GNU readline.
>
> Returns 1 on success with `*fromp` owning an `el_malloc`'d NUL-terminated
> pattern and `*cmdp` pointing at the first character of the replacement
> text; -1 on a parse error; 0 on allocation failure.

> [spec:libedit:def:readline.getto-fn]
> static int getto(const char **cmdp, char **top, const char *from, int delim)

> [spec:libedit:sem:readline.getto-fn]
> Parses the *to* half of an `s/from/to/` history modifier out of `*cmdp`,
> up to the first unescaped `delim`, expanding `&` to `from`.
>
> `*top` is an in/out slot holding a buffer reused across calls (the caller
> passes the address of a function-static `to` pointer).
>
> Steps: `size = 16`, `len = 0`, `from_len = strlen(from)`, `cmd = *cmdp`.
> `with = el_realloc(*top, 16)` and then `*top = NULL` unconditionally — if
> that realloc failed, the previous buffer is still allocated and its only
> pointer has just been discarded, so it leaks. If `with` is NULL, jumps to
> the error exit.
>
> Scan loop, for each `*cmd` until end of string or `delim`: if `len +
> from_len + 1 >= size`, grows `size` by `from_len + 1` (linear growth, so
> long replacements are quadratic) and reallocates; on failure jumps to the
> error exit. If `*cmd == '&'`, copies the whole `from` string at
> `with[len]` and adds `from_len` to `len`, then continues without consuming
> anything else. Otherwise, if `*cmd == '\\'` and the next character is
> `delim` or `'&'`, `cmd` is advanced once so the backslash is dropped and
> that character is taken literally; then `with[len++] = *cmd`. Unlike
> `getfrom`, the bound check here is correct: after it, `len + from_len <
> size`, so both the `&` expansion and the final terminator fit.
>
> If the loop ended because the string ran out (`!*cmd`, no closing
> delimiter), jumps to the error exit. Otherwise `with[len] = '\0'`, `*top =
> with`, `*cmdp = cmd` — note `*cmdp` is left pointing *at* the closing
> delimiter, not past it, which is why the caller does `cmd--` before its
> loop's `cmd++` — and returns 1.
>
> Error exit: `el_free(with)`, `el_free(*top)` (a no-op, since `*top` was
> already NULLed at the top — this is why there is no double free here),
> `*top = NULL`, `*cmdp = cmd`, return -1.
>
> Returns 1 on success with `*top` owning an `el_malloc`'d NUL-terminated
> replacement string, -1 on any failure with `*top` NULL.

> [spec:libedit:def:readline.history-arg-extract-fn]
> char * history_arg_extract(int start, int end, const char *str)

> [spec:libedit:sem:readline.history-arg-extract-fn]
> Returns the words `start` through `end` of `str`, space-joined, as a
> freshly allocated string. This implements the `:n`, `:^`, `:$`, `:*` and
> `:n-m` word designators of history expansion.
>
> Steps: tokenizes with `history_tokenize(str)`. If that returns NULL,
> returns NULL. If the array is present but empty (`*arr == NULL`), jumps to
> the cleanup exit and returns NULL.
>
> Counts tokens into `max`, then decrements: `max` is the index of the last
> word, so word 0 is the command itself and `max` is `$`.
>
> Index normalization, in this order: if `start == '$'` (the integer 36) it
> is replaced by `max`; likewise for `end`. This is a vestigial hack — the
> only in-tree caller passes -1 for `$`, never the character — and it means
> a genuine word index of 36 is silently reinterpreted as "last word". If
> `end < 0` it becomes `max + end + 1`, so -1 means the last word and -2 the
> one before it. If `start < 0` it is set to `end`.
>
> Range check: if `start < 0 || end < 0 || start > max || end > max || start
> > end`, jumps to the cleanup exit and returns NULL. Note the comparisons
> against `max` are done after casting to `size_t`.
>
> Otherwise sums `strlen(arr[i]) + 1` over `i` in `[start, end]`, adds one
> more, allocates that many zeroed bytes with `el_calloc` (NULL on failure
> takes the cleanup exit), then copies each word in turn separating them
> with a single `' '` and NUL-terminates. Original whitespace and quoting
> between the words is not preserved.
>
> Cleanup exit (also taken on the success path): frees every token and the
> token array. Note that when `history_tokenize` returned a non-NULL array
> this loop always runs, so the tokens are never leaked.
>
> Returns the joined string, which the caller owns and must release with
> `free()`, or NULL for "bad word specifier" — which
> `_history_expand_command` reports to `rl_outstream`.

> [spec:libedit:def:readline.history-expand-command-fn]
> static int _history_expand_command(const char *command, size_t offs, size_t cmdlen, char **result)

> [spec:libedit:sem:readline.history-expand-command-fn]
> Expands the single history reference that begins at `command[offs]` (where
> `command[offs]` is the `history_expansion_char`) and stores the result in
> `*result`. `cmdlen` is the length of the reference as measured by
> `history_expand`.
>
> Contract: returns 1 if an expansion was produced, 2 if the `:p` modifier
> was seen and the result should be printed rather than executed, -1 on
> error with `*result` left NULL, and 0 on certain allocation failures
> inside `getfrom` (also with `*result` NULL). The caller owns `*result` and
> must release it with `free()`.
>
> Locals of note: `search` is initialized to NULL and never assigned, so
> every `search`-dependent branch in `getfrom` is dead. `from` and `to` are
> *function statics* holding the last `s///` pattern and replacement; they
> are reused and reallocated across calls and are never freed, which is both
> the mechanism behind the `&` modifier and a permanent allocation.
>
> Event selection:
>
> - If `command[offs + 1]` is one of `:`, `^`, `*`, `$`: builds the
>   three-character array `{'!','!','0'}` (the fourth byte is left
>   uninitialized but is never read) and calls `get_history_event(str, &idx,
>   0)` on it, which takes the `!!` branch and returns the newest history
>   line. Then `idx` is reset to 1 if the character was `:` and 0 otherwise,
>   and `has_mods` is forced to 1. This is the `!:` / `!^` / `!*` / `!$`
>   shorthand for `!!:...`.
> - Else if `command[offs + 1] == '#'`: allocates `offs + 1` zeroed bytes
>   into `aptr` (returning -1 on failure) and `strlcpy`s the first `offs`
>   bytes of `command` into it — the command line typed so far, before the
>   `!#`. Sets `idx = 1`.
> - Else: `qchar` is `'"'` if `offs > 0 && command[offs - 1] == '"'`, else
>   0, and `ptr = get_history_event(command + offs, &idx, qchar)`.
> - In the last two cases `has_mods = (command[offs + idx] == ':')`.
>
> If both `ptr` and `aptr` are NULL, returns -1.
>
> If `has_mods` is false: `*result = strdup(aptr ? aptr : ptr)`, frees
> `aptr`, returns -1 if the strdup failed, else returns 1.
>
> Otherwise `cmd = command + offs + idx + 1`, pointing just past the `:`.
>
> Word designators (one optional step):
>
> - `%` — `tmp = strdup(last_search_match ? last_search_match : "")`, the
>   text last matched by a `!?pat?` search. The strdup is not checked.
> - A character in `"^*$-0123456789"` — decodes into a `(start, end)` pair:
>   `^` gives (1,1); `$` gives (-1,-1); `*` gives (1,-1); a digit run gives
>   `start = <digits>` and then `-<digits>` gives that end, `-$` gives end =
>   -1, a bare trailing `-` gives end = -2, `*` gives end = -1, and nothing
>   gives `end = start`. A leading `-` with no digits leaves `start = 0`.
>   The pair is passed to `history_arg_extract(start, end, aptr ? aptr :
>   ptr)`. If that returns NULL, prints `"<rest of spec>: Bad word
>   specifier"` to `rl_outstream` — with no trailing newline — frees `aptr`
>   and returns -1.
> - Anything else — `tmp = strdup(aptr ? aptr : ptr)`, unchecked.
>
> `aptr` is then freed if it was allocated.
>
> If `*cmd == '\0'` or `cmd` has already run past `cmdlen` bytes from
> `command + offs`, sets `*result = tmp` and returns 1.
>
> Modifier loop over the remaining characters, one `switch` per character;
> any character with no case is silently ignored:
>
> - `:` — separator, ignored.
> - `h` — head: truncate `tmp` at the last `/` (no change if none).
> - `t` — tail: `replace(&tmp, '/')`, keeping the text after the last `/`.
> - `r` — root: truncate `tmp` at the last `.`.
> - `e` — extension: `replace(&tmp, '.')`, keeping the text after the last
>   `.`.
> - `p` — set `p_on = 1` (print only).
> - `g` — set `g_on = 2` (substitute globally). Note the value is 2, not 1;
>   `_rl_compat_sub` only tests it for truth.
> - `&` — if either static `from` or `to` is NULL, ignored; otherwise falls
>   through into the `s` case. Because the `s` case immediately takes the
>   *next* character as the delimiter, `&` does not repeat the previous
>   substitution the way GNU readline's `&` does; a bare `!!:&` reads NUL as
>   the delimiter and errors out.
> - `s` — substitution. `ev = -1`; `delim = *++cmd`; if the delimiter is
>   NUL, or the character after it is NUL, jumps to the error exit. Calls
>   `getfrom(&cmd, &from, search, delim)` and then `getto(&cmd, &to, from,
>   delim)`; anything other than 1 from either jumps to the error exit
>   carrying that return value. Then `aptr = _rl_compat_sub(tmp, from, to,
>   g_on)` and, if non-NULL, frees `tmp` and adopts `aptr`; an allocation
>   failure inside `_rl_compat_sub` silently leaves `tmp` unsubstituted.
>   Resets `g_on = 0`, does `cmd--` to compensate for the loop's `cmd++`
>   (since `getto` leaves `cmd` on the closing delimiter) and continues.
>
> Falling out of the loop: `*result = tmp`, return `p_on ? 2 : 1`.
>
> Error exit: frees `tmp` and returns `ev` (either -1 or 0), leaving
> `*result` NULL.

> [spec:libedit:def:readline.history-expand-fn]
> int history_expand(char *str, char **output)

> [spec:libedit:sem:readline.history-expand-fn]
> csh-style history expansion over a whole line. Produces a newly allocated
> expanded copy in `*output`.
>
> Steps:
>
> 1. If `h` or `e` is NULL, `rl_initialize()`.
> 2. If `history_expansion_char` is 0, expansion is disabled: sets `*output
>    = strdup(str)` (unchecked, so `*output` may be NULL) and returns 0.
> 3. `*output = NULL`. If `str[0] == history_subst_char` (default `'^'`),
>    rewrites the quick-substitution form: allocates `strlen(str) + 5`
>    zeroed bytes, writes `history_expansion_char` twice, then `':'`, then
>    `'s'`, then copies `str` after them, and re-points the working pointer
>    `str` at that buffer. So `^a^b^` is processed as `!!:s^a^b^`.
>    Allocation failure returns 0 with `*output` NULL. Otherwise `*output =
>    strdup(str)` and `str` is re-pointed at that copy; allocation failure
>    returns 0 with `*output` NULL. Either way the scan below mutates only
>    this private copy, never the caller's buffer, even though the parameter
>    is non-const `char *`.
> 4. The `result` accumulator starts NULL with `size = idx = 0`; the
>    internal `ADD_STRING` macro grows it with `el_realloc` and appends with
>    `strlcpy`. On a growth failure it frees `*output` and the passed-in
>    fragment and returns 0 — leaving the caller's `*output` pointing at
>    freed memory (a dangling pointer, not NULL) and leaking `result`.
> 5. Main loop over `i` while `str[i]`: a two-pass scan finds the next
>    history reference. `start = j = i`, `qchar = 0`, `loop_again = 1`. The
>    inner scan advances `j` and:
>    - on `\` immediately followed by `history_expansion_char`, deletes the
>      backslash in place with `memmove` and continues, so `\!` is emitted
>      as a literal `!` and never expanded;
>    - once `loop_again` is 0 (second pass), stops at the first whitespace
>      or at `qchar`;
>    - stops at `history_expansion_char` when the following character is not
>      in `history_no_expand_chars` (default `" \t\n=("`) and either
>      `history_inhibit_expansion_function` is NULL or it returns 0 for this
>      offset. Because `strchr` also matches the terminating NUL, a `!` at
>      end of line never triggers expansion. If the scan stopped on a live
>      `!` during the first pass, sets `i = j`, sets `qchar` to `'"'` when
>      the preceding character is a double quote, skips the `!` (and a
>      second one if present), clears `loop_again` and re-enters the scan to
>      find the end of the reference.
> 6. Emits the literal text `str[start .. i)` with `ADD_STRING`.
> 7. If `str[i]` is NUL or is not `history_expansion_char`, emits `str[i ..
>    j)` as literal text, sets `ret` to 0 when `start == 0` and 1 otherwise,
>    and breaks out of the loop.
> 8. Otherwise calls `_history_expand_command(str, i, j - i, &tmp)` and
>    stores its return in `ret`; if `ret > 0` and `tmp` is non-NULL, appends
>    `tmp`. Frees `tmp` either way, sets `i = j` and loops.
> 9. After the loop, if `ret == 2` the expansion was `:p`-modified: the
>    expanded text is added to the history with `add_history(result)`. (A
>    `GDB_411_HACK` block that would rewrite `ret` to -1 is commented out at
>    the top of the file and is not compiled.)
> 10. Frees the working copy in `*output`, assigns `*output = result` and
>     returns `ret`.
>
> Return values: 0 = nothing was expanded, 1 = expansion performed, 2 =
> `:p`, print but do not execute, -1 = error. Allocation failures also
> surface as 0, sometimes with `*output` NULL and sometimes dangling.
>
> Ownership: `*output` is heap memory the caller must release with `free()`.
> Trap: for an empty input string the main loop never runs and `result` is
> still NULL, so `history_expand("", &out)` returns 0 with `out == NULL`
> rather than an empty string.

> [spec:libedit:def:readline.history-get-fn]
> HIST_ENTRY * history_get(int num)

> [spec:libedit:sem:readline.history-get-fn]
> Returns the history entry with readline index `num`.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. If `num <
> history_base`, returns NULL immediately — indices are 1-based against
> `history_base`, which `add_history` and `stifle_history` bump as old
> events are evicted.
>
> Saves the current cursor position: `history(h, &ev, H_CURR)`, and on
> failure returns NULL without further work; `curr_num = ev.num`.
>
> Positions the cursor with `history(h, &ev, H_DELDATA, num - history_base,
> (void **)-1)`. The `(void **)-1` is a documented magic sentinel meaning
> "set to the n-th event but do not delete it"; the index is 0-based
> counting from the *oldest* entry. On failure it restores the cursor with
> H_SET and returns NULL.
>
> Then `history(h, &ev, H_CURR)` reads the event number now under the
> cursor, and `history(h, &ev, H_NEXT_EVDATA, ev.num, &she.data)` fetches
> that event's `histdata_t` into the static entry. `she.line = ev.str`. Any
> failure restores the cursor and returns NULL.
>
> Restores the cursor with `history(h, &ev, H_SET, curr_num)` and returns
> `&she`.
>
> Ownership: `she` is a single function-static `HIST_ENTRY`; the returned
> pointer is invalidated by the next `history_get()` call. `she.line` points
> into history-owned storage and must not be freed; `she.data` is whatever
> the application attached. This static is distinct from the `rl_he` static
> used by `current_history()`.

> [spec:libedit:def:readline.history-get-history-state-fn]
> HISTORY_STATE * history_get_history_state(void)

> [spec:libedit:sem:readline.history-get-history-state-fn]
> Allocates and returns a snapshot of the history state.
>
> Steps: `hs = el_malloc(sizeof(HISTORY_STATE))`; returns NULL on failure.
> Sets `hs->length = history_length`. Returns `hs`.
>
> libedit's `HISTORY_STATE` has exactly one member, `int length` — this is
> an ABI difference from GNU readline, whose `HISTORY_STATE` also carries
> `entries`, `offset`, `size` and `flags`. Programs compiled against the GNU
> header and run against libedit will read past the end of this allocation;
> programs compiled against libedit's header see only `length`. The struct
> definition is frozen by the drop-in requirement.
>
> Ownership: the caller owns the returned block and must release it with
> `free()`. Nothing else is populated and no global is modified. There is no
> `history_set_history_state` counterpart in this library.

> [spec:libedit:def:readline.history-is-stifled-fn]
> int history_is_stifled(void)

> [spec:libedit:sem:readline.history-is-stifled-fn]
> Reports whether the history size is capped.
>
> The entire body is `return max_input_history != INT_MAX;` — no history
> call, no lazy initialization, no globals written. The source carries the
> comment "cannot return true answer", acknowledging the approximation: the
> real cap lives inside the History object (set with H_SETSIZE) and this
> only inspects the mirror global that `stifle_history` and
> `unstifle_history` maintain.
>
> Consequences: `rl_initialize` sets `max_input_history = INT_MAX`, so a
> freshly initialized library reports unstifled. `stifle_history(INT_MAX)`
> sets a real cap of INT_MAX and is reported as *not* stifled. Changing the
> cap directly through `history(h, &ev, H_SETSIZE, n)` is invisible here.
> Calling this before any initialization returns 0, since
> `max_input_history`'s static initializer is 0, not INT_MAX.
>
> Returns non-zero (1) for stifled, 0 for unstifled.

> [spec:libedit:def:readline.history-list-fn]
> HIST_ENTRY ** history_list(void)

> [spec:libedit:sem:readline.history-list-fn]
> Returns a NULL-terminated array of pointers to every history entry, oldest
> first — readline's `the_history` view.
>
> Steps: `history(h, &ev, H_LAST)` moves the cursor to the oldest entry; if
> that fails (empty history) returns NULL. There is no lazy initialization,
> so a NULL `h` crashes.
>
> Grows two module statics with `el_realloc`: `_history_listp` to
> `history_length + 1` pointers, and `_history_list` to `history_length`
> `HIST_ENTRY` structs. Either allocation failing returns NULL — but note
> `_history_listp` is committed before `_history_list` is attempted, so a
> failure on the second leaves the statics mutually inconsistent for the
> next call.
>
> Then walks the history from oldest to newest with H_PREV, filling
> `_history_listp[i] = &_history_list[i]`, `_history_list[i].line = ev.str`
> (borrowed, history-owned) and `_history_list[i].data = NULL` — entry data
> is never surfaced. The bounds guard is `if (i++ == history_length)
> abort();`, which is evaluated *after* the write, so if the real number of
> entries exceeds the cached `history_length` the code writes one element
> past the end of `_history_list` and then calls `abort()`. Both the
> out-of-bounds write and the `abort()` inside a library are hazards the
> port must handle differently (it must not corrupt memory, and it should
> not kill the process).
>
> Terminates the pointer array with `_history_listp[i] = NULL` and returns
> `_history_listp`.
>
> Ownership: the array, the `HIST_ENTRY` structs and the `line` strings are
> all borrowed — the caller must free none of them. The whole result is
> invalidated (reallocated and overwritten) by the next call. The cursor is
> left at the newest entry, not restored.

> [spec:libedit:def:readline.history-search-fn]
> int history_search(const char *str, int direction)

> [spec:libedit:sem:readline.history-search-fn]
> Searches the history for the first entry *containing* `str`, starting from
> the current cursor position.
>
> Steps: `history(h, &ev, H_CURR)` seeds `ev` with the current entry and
> records `curr_num = ev.num`; failure returns -1. There is no lazy
> initialization.
>
> Loop: `strstr(ev.str, str)`; if it matches, returns the byte offset of the
> match within that entry's line and leaves the history cursor *on the
> matching entry* — the position is deliberately not restored, which is how
> `get_history_event` follows a `!?pat?` reference with an H_CURR read.
> Otherwise moves with `history(h, &ev, direction < 0 ? H_NEXT : H_PREV)`
> and repeats; when the move fails the loop ends.
>
> Direction mapping: a negative `direction` uses H_NEXT, which in libedit
> means toward *older* entries (new events are inserted at the front of the
> list, so H_FIRST is the newest and H_LAST the oldest). A non-negative
> `direction` uses H_PREV, toward newer entries. This matches readline's
> convention that a negative direction searches backward through time, but
> is the opposite of the plain reading of the libedit op names.
>
> On exhaustion, restores the cursor with `history(h, &ev, H_SET, curr_num)`
> and returns -1.
>
> Return value: the byte offset of the match inside the matched line, or -1.
> Note the current entry is tested first, so a match at the current position
> returns immediately without moving. GNU readline's `history_search`
> returns the offset too but also updates `history_offset`; this
> implementation leaves `history_offset` untouched.

> [spec:libedit:def:readline.history-search-pos-fn]
> int history_search_pos(const char *str, int direction __attribute__((__unused__)), int pos)

> [spec:libedit:sem:readline.history-search-pos-fn]
> Searches for an entry containing `str`, nominally starting at absolute
> position `abs(pos)` and running backward when `pos < 0`, forward
> otherwise. The `direction` parameter is declared unused and is ignored
> entirely — the sign of `pos` carries the direction.
>
> Steps: `off = (pos > 0) ? pos : -pos` (so `off = 0` when `pos == 0`), then
> `pos` is collapsed to 1 if it was positive and -1 otherwise (including for
> 0).
>
> `history(h, &ev, H_CURR)` saves `curr_num`; failure returns -1.
>
> `if (!history_set_pos(off) || history(h, &ev, H_CURR) != 0) return -1;` —
> this is the broken part. `history_set_pos` only assigns the global
> `history_offset`; it does not move libedit's internal cursor. The
> following H_CURR therefore re-reads the entry the cursor was already on.
> So `pos` acts purely as a range check (`0 <= off < history_length`) and
> the search always begins wherever the cursor happens to be, not at `off`.
>
> Loop: if `strstr(ev.str, str)` matches, returns `off` — the *requested*
> position, not the position of the match. Otherwise moves with `history(h,
> &ev, pos < 0 ? H_PREV : H_NEXT)`; note H_NEXT goes toward older entries,
> so a positive `pos` searches backward through time. The loop ends when the
> move fails.
>
> On exhaustion, restores the cursor with `history(h, &ev, pos < 0 ?
> H_NEXT_EVENT : H_PREV_EVENT, curr_num)` and returns -1.
>
> Side effect: `history_offset` is set to `off` by the internal
> `history_set_pos` call whenever `off` is in range, even when the search
> subsequently fails. Return value: `off` on a hit, -1 on a miss or on any
> history error.

> [spec:libedit:def:readline.history-search-prefix-fn]
> int history_search_prefix(const char *str, int direction)

> [spec:libedit:sem:readline.history-search-prefix-fn]
> Searches the history for the first entry *beginning with* `str`, starting
> from the current cursor position.
>
> The whole body is `return history(h, &ev, direction < 0 ? H_PREV_STR :
> H_NEXT_STR, str);` — the `HistEvent` is a local that is filled and then
> discarded, no lazy initialization is performed, and no global is read or
> written.
>
> Direction: H_PREV_STR scans from the current entry toward *older* entries;
> H_NEXT_STR scans toward *newer* ones. So a negative `direction` searches
> backward through time, matching `history_search`'s convention even though
> the op names read the other way round. Both ops compare `strncmp(str,
> entry, strlen(str)) == 0`, i.e. a plain prefix test with no case folding,
> and they test the current entry first. On a match the cursor is left on
> the matching entry; on failure the cursor has been moved to the end of the
> list in that direction and is *not* restored.
>
> Return value: 0 when a match was found, -1 when not. This differs from GNU
> readline, whose `history_search_prefix` returns the offset of the matching
> line (or -1), and from libedit's own `history_search`, which returns a
> byte offset. `history_offset` is never updated.

> [spec:libedit:def:readline.history-set-pos-fn]
> int history_set_pos(int pos)

> [spec:libedit:sem:readline.history-set-pos-fn]
> Sets the readline history position used by `current_history`,
> `previous_history` and `next_history`.
>
> The entire body is: if `pos >= history_length || pos < 0`, return 0
> without changing anything; otherwise set the global `history_offset = pos`
> and return 1.
>
> Nothing else happens — in particular libedit's internal history cursor is
> *not* moved, which is why `history_search_pos` (its only in-tree caller)
> does not actually start its search at `pos`.
>
> Note the upper bound is exclusive: `pos == history_length`, which in GNU
> readline means "one past the newest entry, i.e. the line being typed", is
> rejected here and returns 0. A caller cannot use this function to return
> to the end of the list.
>
> Return value: 1 if the position was accepted and stored, 0 if it was out
> of range.

> [spec:libedit:def:readline.history-tokenize-fn]
> char ** history_tokenize(const char *str)

> [spec:libedit:sem:readline.history-tokenize-fn]
> Splits `str` into shell-like words and returns them as a NULL-terminated
> array.
>
> Outer loop, while `str[i]` is not NUL: skips `isspace` characters, records
> `start = i`, then runs the inner scanner.
>
> Inner scanner, per character, with a single `delim` state holding the
> currently open quote:
>
> - `\` — if the next character is not NUL, `i` is advanced once so the
>   escaped character is consumed as part of the word (the backslash is kept
>   in the token text).
> - a character equal to the open `delim` — closes the quote (`delim =
>   '\0'`).
> - when no quote is open, `isspace` or any of `()<>;&|$` — ends the word.
> - when no quote is open, any of `'` `` ` `` `"` — opens a quote.
> - then, if not at end of string, `i++`.
>
> After the scan the result array is grown when `idx + 2 >= size`: `size` is
> doubled (starting from 1, so the first growth gives 2) and `el_realloc`ed.
> On failure the array is freed and NULL returned — leaking every token
> already allocated. The word text `str[start .. i)` is copied into a fresh
> `el_calloc`'d buffer via `strlcpy`; on failure all previously stored
> tokens and the array are freed and NULL returned. The token is stored at
> `result[idx++]` and `result[idx]` is set to NULL after every append, so
> the array is always terminated. Finally, if `str[i]` is not NUL, `i++`
> skips the terminating character.
>
> Quirks that are part of the observable behaviour:
>
> - An empty or all-whitespace input: for `""` the outer loop never runs and
>   the function returns NULL, not an empty array. For `"   "` the loop runs
>   once, produces a single zero-length token, and returns an array of one
>   empty string.
> - The shell metacharacters `()<>;&|$` terminate a word but are not emitted
>   as words of their own; the scanner then skips one character, so a
>   metacharacter between two words yields an extra empty token. GNU
>   readline returns metacharacters as separate tokens.
> - Quotes and backslashes are retained verbatim in the token text; no
>   unquoting is performed.
>
> Ownership: the caller owns the array and every string in it, and must
> release each element and then the array with `free()`.

> [spec:libedit:def:readline.history-total-bytes-fn]
> int history_total_bytes(void)

> [spec:libedit:sem:readline.history-total-bytes-fn]
> Returns the sum of the lengths of every history line.
>
> Steps: `history(h, &ev, H_CURR)` saves `curr_num`; if it fails (empty
> history, or `h` never initialized — there is no lazy initialization guard)
> returns -1.
>
> `history(h, &ev, H_FIRST)` moves to the newest entry, then a do/while loop
> adds `strlen(ev.str) * sizeof(*ev.str)` for each entry and advances with
> H_NEXT (toward older) until that fails. Because `ev.str` is `char *` in
> the narrow history this is simply the byte length; the terminating NUL is
> not counted, and neither is any per-entry overhead.
>
> Restores the position with `history(h, &ev, H_PREV_EVENT, curr_num)` —
> searching from the oldest end back toward `curr_num` — and returns the sum
> cast to `int`. The cast is unchecked, so a history whose combined length
> exceeds INT_MAX yields an implementation-defined (on typical platforms,
> negative) value.
>
> Return value: total bytes, or -1 if the initial H_CURR failed.

> [spec:libedit:def:readline.history-truncate-file-fn]
> int history_truncate_file (const char *filename, int nlines)

> [spec:libedit:sem:readline.history-truncate-file-fn]
> Truncates a history file in place so that only its last `nlines` lines
> remain, using a temporary file as scratch.
>
> Setup: if `filename` is NULL, substitutes `_default_history_file()`; if
> that is also NULL returns `errno`. Opens the target with `fopen(filename,
> "r+")`; on failure returns `errno`. Copies the template
> `"/tmp/.historyXXXXXX"` into a stack buffer, `mkstemp`s it, and `fdopen`s
> the descriptor `"r+"`. Failures return `errno` after the appropriate
> cleanup. The temporary file is always `unlink`ed before return, on every
> path after `mkstemp` succeeded — including the success path, which falls
> through the same three labels (`fclose(tp)`, `unlink(template)`,
> `fclose(fp)`).
>
> Phase 1 — copy the whole file to the temporary. Reads 4096-byte blocks
> with `fread(buf, 4096, 1, fp)` and writes each with `fwrite(buf, 4096, 1,
> tp)`, counting complete blocks in `count`. When a read returns short: any
> `ferror` sets `ret = errno` and breaks; else it `fseeko`s back to `4096 *
> count`, re-reads the tail with `fread(buf, 1, 4096, fp)` into `left`,
> writes `left` bytes to the temporary, flushes and breaks. If `left` is 0
> (the file was an exact multiple of 4096) it decrements `count` and
> pretends `left = 4096`, so `buf` still holds the last full block.
>
> Phase 2 — walk backwards over the temporary counting newlines. `cp` starts
> at the last byte of the tail block, advanced by one if that byte is not
> `'\n'`. The inner loop scans backwards decrementing `cp`; each `'\n'`
> decrements `nlines`, and when `nlines` reaches exactly 0 the scan stops
> with `cp` just past that newline (wrapping to the next block if it ran off
> the end of `buf`). If `nlines` is still positive and `count` is non-zero,
> it decrements `count`, seeks the temporary to `4096 * count`, reads that
> block, points `cp` at the end of `buf` and repeats. A short read there
> sets `ret = errno` on a real error and `EAGAIN` otherwise.
>
> If `ret` is set, or `nlines` is still greater than 0 (the file has fewer
> than `nlines` lines), the function jumps to cleanup and returns `ret` — so
> "file shorter than requested" is a silent success that changes nothing.
> Passing `nlines <= 0` is not handled meaningfully: the first decrement
> makes it negative, it can never hit 0, and the resulting `cp` lands one
> byte before the block, producing an arbitrary cut point.
>
> Phase 3 — copy the retained tail back over the original. Seeks `fp` to 0
> and `tp` to `4096 * count + (cp - buf)`, then loops `fread(buf, 1, 4096,
> tp)` / `fwrite(buf, left, 1, fp)` until the read returns 0. The
> end-of-loop error check inspects `ferror(fp)` where it should inspect
> `ferror(tp)`, so a read error on the temporary is reported as success.
> Flushes `fp`, and if `ftello(fp)` is positive truncates the file to that
> length with `ftruncate`.
>
> Cleanup and return: closes the temporary, unlinks it, closes the target,
> returns `ret`. Return convention: 0 on success, a positive errno value (or
> `EAGAIN` for an internal inconsistency) on failure — never -1.
>
> Notes the port must preserve or consciously change: the scratch file is
> always created in `/tmp` regardless of where the history file lives, so
> private history contents transit a world-writable directory and the
> operation fails if `/tmp` is not writable; and the rewrite is not atomic —
> a crash between the seek-to-0 and the `ftruncate` leaves a corrupted
> history file.

> [spec:libedit:def:readline.next-history-fn]
> HIST_ENTRY * next_history(void)

> [spec:libedit:sem:readline.next-history-fn]
> Moves the readline history position one step toward the newest entry and
> returns the entry now current.
>
> Steps: if `history_offset >= history_length`, returns NULL without any
> change — the position is already at the end. Then `history(h, &ev,
> H_LAST)` moves libedit's internal cursor to the oldest entry; if that
> fails (empty history) returns NULL. Increments `history_offset` and
> returns `current_history()`, which does the actual lookup by walking from
> the oldest end toward newer entries until it finds event number
> `history_offset + 1`.
>
> There is no lazy initialization, so a NULL `h` crashes.
>
> Ownership: the returned pointer is the shared static `rl_he` used by
> `current_history()`; it must not be freed and is overwritten by the next
> history navigation call. `->line` is borrowed history storage and `->data`
> is always NULL.
>
> Note the H_LAST-then-scan pattern makes every navigation step O(n) in the
> history length, and that the direction naming is inverted between readline
> and libedit — readline's "next" is toward the most recent entry, which is
> libedit's H_PREV direction.

> [spec:libedit:def:readline.previous-history-fn]
> HIST_ENTRY * previous_history(void)

> [spec:libedit:sem:readline.previous-history-fn]
> Moves the readline history position one step toward the oldest entry and
> returns the entry now current.
>
> Steps: if `history_offset == 0`, returns NULL without any change — the
> position is already at the oldest entry. Then `history(h, &ev, H_LAST)`
> moves libedit's internal cursor to the oldest entry; if that fails (empty
> history) returns NULL. Decrements `history_offset` and returns
> `current_history()`, which walks from the oldest end toward newer entries
> until it finds event number `history_offset + 1`.
>
> There is no lazy initialization, so a NULL `h` crashes.
>
> Ownership: the returned pointer is the shared static `rl_he` used by
> `current_history()`; it must not be freed and is overwritten by the next
> history navigation call. `->line` is borrowed history storage and `->data`
> is always NULL.
>
> The source comments on the direction inversion: readline's "previous"
> means further back in time, which corresponds to libedit's H_NEXT
> direction, so the op names in this file cannot be read literally.

> [spec:libedit:def:readline.read-history-fn]
> int read_history(const char *filename)

> [spec:libedit:sem:readline.read-history-fn]
> Loads a history file into the history list.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. If `filename` is
> NULL, substitutes `_default_history_file()`; if that is also NULL, returns
> the current `errno`. Clears `errno` to 0, then calls `history(h, &ev,
> H_LOAD, filename)`.
>
> H_LOAD opens the file, requires the first line to be libedit's history
> signature line (a file without it loads zero entries and is not an error),
> then reads each subsequent line, strips the trailing newline,
> `strunvis`-decodes it and enters it with H_ENTER. This escaping is the
> frozen on-disk format the port must round-trip. Entries are appended to
> whatever is already in the list, not replacing it.
>
> If H_LOAD returns -1, returns `errno` if it is now non-zero and `EINVAL`
> otherwise. Otherwise queries `history(h, &ev, H_GETSIZE)` and, if that
> succeeded, sets the exported global `history_length = ev.num`. If
> `history_length` ends up negative, returns `EINVAL`. Otherwise returns 0.
>
> `history_base` and `history_offset` are *not* adjusted, so after a load
> the readline position globals no longer reflect the list — callers are
> expected to follow with `using_history()`.
>
> Return convention: 0 on success, a positive errno value on failure; never
> -1.

> [spec:libedit:def:readline.readline-fn]
> char * readline(const char *p)

> [spec:libedit:sem:readline.readline-fn]
> The main entry point: prints prompt `p`, reads and edits one line, and
> returns it with any trailing newline removed.
>
> Steps in order:
>
> 1. `prompt` is a `volatile` copy of `p`, because the function contains a
>    `setjmp` and `_rl_abort_internal` may `longjmp` back into it.
> 2. If `e` or `h` is NULL, `rl_initialize()`. Its return value is not
>    checked, so a failed initialization is not detected here.
> 3. If `rl_startup_hook` is non-NULL, calls it; its return value is
>    ignored.
> 4. `tty_init(e)` — puts the terminal into editing mode.
> 5. Sets the exported global `rl_done = 0`.
> 6. `setjmp(topbuf)`. Everything from here on re-executes if
>    `_rl_abort_internal` longjmps.
> 7. `buf = NULL`.
> 8. `rl_set_prompt(prompt)`; on -1 (allocation failure) jumps to the exit
>    and returns NULL. `rl_set_prompt` maps a NULL prompt to `""`.
> 9. If `rl_pre_input_hook` is non-NULL, calls it; return value ignored.
> 10. Input-function selection, using a function-static `used_event_hook`
>     that persists across calls: if `rl_event_hook` is set and `e` does not
>     have the NO_TTY flag, installs `el_set(e, EL_GETCFN,
>     _rl_event_read_char)` and records `used_event_hook = 1`. If
>     `rl_event_hook` is NULL but `used_event_hook` is set, restores
>     `el_set(e, EL_GETCFN, EL_BUILTIN_GETCFN)` and clears the flag. Note
>     the restore installs the *builtin* reader, clobbering any
>     `_getc_function` that `rl_initialize` installed on behalf of
>     `rl_getc_function` — an application using both an event hook and a
>     getc hook loses the getc hook permanently.
> 11. Sets `rl_already_prompted = 0` (the prompt callback sets it back to 1
>     when libedit asks for the prompt).
> 12. `ret = el_gets(e, &count)` — `count` comes back as the number of
>     *bytes* in the encoded line.
> 13. If `ret` is non-NULL and `count > 0`: `buf = strdup(ret)`; on failure
>     jumps to the exit returning NULL. Then, if `buf[count - 1] == '\n'`,
>     that byte is replaced with NUL. Only a single trailing newline is
>     removed and only when `count` says it is the last byte; a carriage
>     return is not stripped. Otherwise `buf = NULL` (end of input, or an
>     empty read).
> 14. `history(h, &ev, H_GETSIZE)` and `history_length = ev.num` — the
>     exported global is refreshed even though `readline()` itself never
>     adds to the history. Applications must call `add_history()`
>     themselves.
> 15. Exit label: `tty_end(e, TCSADRAIN)` restores the terminal, draining
>     output first. Returns `buf`.
>
> Ownership: the returned string comes from `strdup`, so the caller must
> release it with `free()` (not `el_free` unless they are the same). NULL
> means end of input, an empty read, or an allocation failure — the three
> are not distinguished.
>
> Globals written: `rl_done`, `rl_prompt` (via `rl_set_prompt`),
> `rl_already_prompted`, `history_length`, and indirectly `rl_line_buffer` /
> `rl_point` / `rl_end` through EditLine's resize callback.

> [spec:libedit:def:readline.remove-history-fn]
> HIST_ENTRY * remove_history(int num)

> [spec:libedit:sem:readline.remove-history-fn]
> Removes the history entry at index `num` and returns it to the caller.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. Allocates a
> `HIST_ENTRY` with `el_malloc`; returns NULL on failure. Calls `history(h,
> &ev, H_DELDATA, num, &he->data)`, which positions to the `num`-th event
> counting 0-based from the *oldest*, copies its `histdata_t` into
> `he->data`, sets `ev.str` to a freshly `strdup`ed copy of the line, and
> then unlinks and frees the entry. On failure the `HIST_ENTRY` is freed and
> NULL is returned.
>
> Sets `he->line = ev.str` — the fresh copy H_DELDATA made, so this string
> is *owned by the caller*. Then re-reads `history(h, &ev, H_GETSIZE)` and,
> if it succeeded, updates the exported global `history_length = ev.num`.
> `history_base` and `history_offset` are not adjusted.
>
> Returns the entry.
>
> Ownership trap, and the one the port most needs to get right: both the
> `HIST_ENTRY` and its `line` are heap blocks the caller must free, but
> libedit's `free_history_entry()` is a no-op stub, so the readline idiom
> `free_history_entry(remove_history(i))` leaks both. `stifle_history()` is
> the only in-tree caller that disposes of the entry correctly, and it does
> so by hand with three `el_free` calls (`he->data`, `he->line` through a
> const-stripping cast, and `he`). Note it also frees `he->data`, which the
> application, not the history, allocated.

> [spec:libedit:def:readline.replace-fn]
> static void replace(char **tmp, int c)

> [spec:libedit:sem:readline.replace-fn]
> Rewrites `*tmp` to be just the text following the last occurrence of
> character `c`. Used by the `:t` (tail, `c == '/'`) and `:e` (extension, `c
> == '.'`) history modifiers.
>
> Steps: `aptr = strrchr(*tmp, c)`; if there is no such character the
> function returns with `*tmp` unchanged. Otherwise `aptr = strdup(aptr +
> 1)` — a copy of everything after that character, which is the empty string
> when the character is last. The original `*tmp` is released with `el_free`
> and `*tmp` is assigned the copy.
>
> Undefined behaviour to preserve knowingly, not to fix silently: the
> `strdup` result is never checked (the source carries the comment `// XXX:
> check`), so on allocation failure `*tmp` is set to NULL after the old
> buffer has already been freed. `_history_expand_command`'s modifier loop
> then continues and passes that NULL to `strrchr`, `_rl_compat_sub` or
> `strdup`, dereferencing it. The Rust port should decide explicitly whether
> to abort the expansion or leave the string untouched; either is a
> deliberate divergence from a crash.
>
> Returns nothing. `*tmp` must be a heap pointer suitable for `el_free`.

> [spec:libedit:def:readline.replace-history-entry-fn]
> HIST_ENTRY * replace_history_entry(int num, const char *line, histdata_t data)

> [spec:libedit:sem:readline.replace-history-entry-fn]
> Replaces the line and data of the entry whose *event number* is `num`, and
> returns a `HIST_ENTRY` describing what was there before.
>
> Note the index convention differs from `history_get` and `remove_history`:
> `num` here is matched against `ev.num`, libedit's monotonically increasing
> event id, not against a positional index.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. `history(h, &ev,
> H_CURR)` saves `curr_num`; failure returns NULL. `history(h, &ev, H_LAST)`
> moves to the oldest entry; failure returns NULL. Allocates a `HIST_ENTRY`
> with `el_malloc`; failure returns NULL. `history(h, &ev, H_NEXT_EVDATA,
> num, &he->data)` scans from the current position toward newer entries for
> the event with that number, filling `he->data` with its attached
> `histdata_t`; failure jumps to the error exit. `he->line = ev.str`; if it
> is NULL, jumps to the error exit. `history(h, &ev, H_REPLACE, line, data)`
> installs a private copy of `line` and the new `data` on the entry under
> the cursor; failure jumps to the error exit. `history(h, &ev, H_SET,
> curr_num)` restores the cursor; failure also jumps to the error exit —
> after the replacement has already happened.
>
> Error exit: frees the `HIST_ENTRY` and returns NULL, so a late failure
> leaves the history modified with no way for the caller to learn what the
> old contents were.
>
> Ownership, and a trap: H_REPLACE overwrites the entry's `str` pointer
> *without freeing the old string*, so the old line is leaked by the history
> layer and the pointer this function returned in `he->line` remains valid
> indefinitely. The returned `HIST_ENTRY` itself is `el_malloc`'d and
> belongs to the caller. Freeing `he->line` would actually reclaim the
> leaked line, but `free_history_entry()` — the function readline programs
> would use — frees nothing, so in practice both leak. No globals are
> updated: `history_length`, `history_base` and `history_offset` are
> unchanged, which is correct since the entry count did not change.

> [spec:libedit:def:readline.resize-fun-fn]
> static void _resize_fun(EditLine *el, void *a)

> [spec:libedit:sem:readline.resize-fun-fn]
> EL_RESIZE callback that keeps the exported global `rl_line_buffer`
> pointing at EditLine's current narrow line buffer.
>
> Registered by `rl_initialize` with `el_set(e, EL_RESIZE, _resize_fun,
> &rl_line_buffer)`, so `a` is always `&rl_line_buffer`; the function treats
> it as `const char **`.
>
> Steps: `li = el_line(el)` obtains the narrow `LineInfo`, then `*ap =
> li->buffer`.
>
> `el_line()` re-encodes the wide edit line into EditLine's internal legacy
> conversion buffer (`el->el_lgcyconv`), recomputes `cursor` and `lastchar`
> as byte offsets into it, and *then* invokes this registered resize
> function — which is precisely how `rl_line_buffer` is refreshed after
> every line change. `el_line()` guards against re-entry with the
> FROM_ELLINE flag, so the `el_line()` call made here returns the cached
> `LineInfo` rather than recursing.
>
> Aliasing contract the port must honour: `rl_line_buffer` is *not* owned by
> the readline layer. It points into EditLine's conversion buffer, which is
> reallocated whenever a longer line is encoded, so the pointer may change
> on any editing operation and must never be freed or resized by an
> application. `_rl_update_pos` additionally writes a NUL into it at index
> `rl_end`. `rl_initialize` also calls this function directly once, so
> `rl_line_buffer` is non-NULL from initialization onward.

> [spec:libedit:def:readline.rl-abort-fn]
> int rl_abort(int count, int key)

> [spec:libedit:sem:readline.rl-abort-fn]
> Stub. The entire body is `return count && key ? 0 : 0;` — both parameters
> are read solely to suppress unused-parameter warnings and every path
> evaluates to 0.
>
> Nothing is aborted: no bell, no line reset, no `longjmp`, no change to
> `rl_done` or any other global. Contrast `_rl_abort_internal`, which does
> beep and longjmp; the two are unrelated despite the names.
>
> GNU readline's `rl_abort` aborts the current command, rings the bell and
> returns non-zero so the editor loop unwinds. Under libedit a key bound to
> `rl_abort` through `rl_add_defun` simply does nothing and the editor
> continues.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-abort-internal-fn]
> int _rl_abort_internal(void)

> [spec:libedit:sem:readline.rl-abort-internal-fn]
> Aborts the current `readline()` call by unwinding to its `setjmp`.
>
> Steps: `el_beep(e)` rings the terminal bell through EditLine — there is no
> NULL guard on `e`, so calling this before initialization dereferences
> NULL. Then `longjmp(topbuf, 1)`, which never returns; the declared `int`
> return value is never produced and the end of the function is unreachable.
>
> `topbuf` is a single module-global `jmp_buf` set by `readline()` before it
> configures the prompt and reads. Control therefore resumes inside
> `readline()` just after its `setjmp`, with `buf` reset to NULL, and
> `rl_set_prompt` / `rl_pre_input_hook` / the getc-function selection /
> `el_gets` all re-run. `tty_init` is *not* re-run and `rl_done` is not
> re-cleared.
>
> Undefined behaviour the port must handle deliberately rather than inherit:
> if no `readline()` call is active on the stack — for example in callback
> mode, or after `readline()` has returned — `topbuf` holds a stale or
> zero-initialized `jmp_buf` and longjmping into a dead frame is undefined.
> Because `topbuf` is a single global, nesting `readline()` calls (say from
> a hook) makes only the innermost `setjmp` reachable, and aborting then
> destroys the outer frame's expectations.

> [spec:libedit:def:readline.rl-add-defun-fn]
> int rl_add_defun(const char *name, rl_command_func_t *fun, int c)

> [spec:libedit:sem:readline.rl-add-defun-fn]
> Registers a readline-style command function and binds a single key to it.
>
> Steps in order:
>
> 1. Range check: `if ((size_t)c >= sizeof(map)/sizeof(map[0]) || c < 0)
>    return -1;` — the table has 256 slots, so `c` must be 0..255.
> 2. `map[(unsigned char)c] = fun` — stores the function in the
>    module-static dispatch table that `rl_bind_wrapper` later consults. Any
>    previous entry for that byte is silently overwritten. `fun` is stored
>    as a raw pointer; the caller must keep the function alive.
> 3. `el_set(e, EL_ADDFN, name, name, rl_bind_wrapper)` — registers an
>    EditLine editor command called `name`, using `name` again as its help
>    text, implemented by `rl_bind_wrapper`. There is *no* lazy
>    `rl_initialize()` here, so calling `rl_add_defun` before
>    `rl_initialize()` or `readline()` passes a NULL `EditLine *` and
>    crashes. EL_ADDFN converts both strings to wide characters and
>    duplicates them, so `name` need not outlive the call.
> 4. `vis(dest, c, VIS_WHITE | VIS_NOSLASH, 0)` into an 8-byte stack buffer,
>    rendering the key byte in strvis form: control characters as `^X`,
>    other non-printables as `\nnn`, whitespace encoded rather than literal,
>    and with no backslash doubling.
> 5. `el_set(e, EL_BIND, dest, name, NULL)` binds that rendered key sequence
>    to the new command.
>
> Both `el_set` results are discarded, so a failed registration or binding
> is not reported. Returns 0 for any in-range `c`, -1 only for an
> out-of-range one.
>
> Divergence: readline lets a named function be bound to arbitrary key
> sequences afterwards; here the name and exactly one key are established
> together, and the key is identified by a single byte, so multi-byte
> sequences cannot be reached this way.

> [spec:libedit:def:readline.rl-bind-key-fn]
> int rl_bind_key(int c, rl_command_func_t *func)

> [spec:libedit:sem:readline.rl-bind-key-fn]
> Binds key byte `c` to a readline command function — but supports exactly
> one function.
>
> Steps: `retval = -1`. Lazily `rl_initialize()` if `h` or `e` is NULL. Then
> a single pointer-equality test: `if (func == rl_insert)` — this library's
> own `rl_insert`, compared by address. When it matches, sets
> `e->el_map.key[c] = ED_INSERT`, i.e. binds the key directly in EditLine's
> 256-entry action table to the self-insert action, and sets `retval = 0`.
> Any other `func` is silently ignored and nothing is bound.
>
> Returns 0 if the binding was made, -1 otherwise.
>
> Memory-safety hazard the source acknowledges in a comment: `c` is not
> range-checked. `el_map.key` is an array of 256 `el_action_t`, so any `c`
> outside 0..255 — including the negative values a caller might pass for
> meta keys, and `EOF` — writes out of bounds and corrupts adjacent
> `EditLine` state. This is undefined behaviour and the port must
> range-check instead of reproducing it.
>
> Divergence: GNU readline binds arbitrary command functions in the current
> keymap. Here anything other than self-insert fails, so applications that
> bind their own handlers through `rl_bind_key` get -1 and no binding;
> `rl_add_defun` is the only route to a custom function.

> [spec:libedit:def:readline.rl-bind-key-in-map-fn]
> int /*ARGSUSED*/ rl_bind_key_in_map(int key __attribute__((__unused__)), rl_command_func_t *fun __attribute__((__unused__)), Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-bind-key-in-map-fn]
> Stub. All three parameters are declared unused and the body is `return
> 0;`.
>
> Nothing is bound, no keymap is consulted (libedit has no readline keymaps
> at all — `rl_get_keymap` and `rl_make_bare_keymap` both return NULL, so
> any `Keymap` a caller could pass is NULL to begin with), and no global is
> touched.
>
> GNU readline binds `key` to `fun` in keymap `k` and returns non-zero on
> error. Here the return value 0 reports success unconditionally, so callers
> cannot detect that nothing happened.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-bind-wrapper-fn]
> static unsigned char rl_bind_wrapper(EditLine *el __attribute__((__unused__)), unsigned char c)

> [spec:libedit:sem:readline.rl-bind-wrapper-fn]
> EditLine command that dispatches to a readline command function registered
> by `rl_add_defun`.
>
> The `el` parameter is unused; the module-static `e` is what
> `_rl_update_pos` operates on.
>
> Steps:
>
> 1. If `map[c]` is NULL, returns CC_ERROR (6). This happens when the key is
>    still bound to this wrapper but the table slot was cleared or the key
>    differs from the one `rl_add_defun` registered.
> 2. `_rl_update_pos()` — refreshes the exported globals `rl_point`,
>    `rl_end` and `rl_line_buffer` from EditLine's current line, so the
>    readline function sees consistent state. This also NUL-terminates the
>    line buffer at `rl_end`.
> 3. `(*map[c])(1, c)` — calls the readline function with `count` hardcoded
>    to 1 and `key` set to the invoking byte. The function's return value is
>    discarded, so a readline command cannot report failure.
> 4. If the exported global `rl_done` is non-zero afterwards — the readline
>    convention for "finish this line" — returns CC_EOF (2), which ends
>    `el_gets`. Otherwise returns CC_NORM (0).
>
> Trap for the port: the globals are propagated *into* the callback only.
> Changes the callback makes to `rl_point` or `rl_line_buffer` are never
> written back into `EditLine`'s wide line, so readline commands that edit
> through the globals have no effect; only commands that call back into
> `rl_insert_text`, `rl_delete_text`, `rl_replace_line` and friends actually
> change the line.

> [spec:libedit:def:readline.rl-callback-handler-install-fn]
> void rl_callback_handler_install(const char *prompt, rl_vcpfunc_t *linefunc)

> [spec:libedit:sem:readline.rl-callback-handler-install-fn]
> Switches the library into readline's callback (non-blocking) mode.
>
> Steps: if `e` is NULL, `rl_initialize()` — note the guard tests only `e`,
> not `h`, unlike every other lazy-init site in this file. Then
> `rl_set_prompt(prompt)`, whose return value is discarded (a NULL prompt
> becomes `""`, and an allocation failure goes unreported). Stores
> `linefunc` in the exported global `rl_linefunc`. Finally `el_set(e,
> EL_UNBUFFERED, 1)`, which makes `el_gets` return as soon as input is
> available instead of waiting for a complete line.
>
> Returns nothing.
>
> Globals written: `rl_prompt` (through `rl_set_prompt`) and `rl_linefunc`.
> Installing a second handler simply overwrites `rl_linefunc`; there is no
> stack of handlers.
>
> Contract with `rl_callback_read_char`: the application is expected to call
> that function whenever input is ready on `rl_instream`, and `linefunc` is
> invoked with the completed line — a heap string the callback takes
> ownership of and must `free()` — or with NULL at end of input.
> `rl_callback_handler_remove` tears the mode down.

> [spec:libedit:def:readline.rl-callback-handler-remove-fn]
> void rl_callback_handler_remove(void)

> [spec:libedit:sem:readline.rl-callback-handler-remove-fn]
> Leaves readline's callback mode.
>
> Steps: `el_set(e, EL_UNBUFFERED, 0)` restores line-at-a-time reading, then
> `rl_linefunc = NULL`.
>
> There is no lazy-initialization guard, so calling this before
> `rl_callback_handler_install` (or after a failed initialization)
> dereferences a NULL `EditLine *`.
>
> Nothing else happens: the prompt is not restored or freed, no partially
> typed line is discarded, the terminal is not deprepped, and no completion
> callback is fired. GNU readline additionally clears the line and
> redisplays; the port must reproduce the libedit behaviour of leaving the
> display as it is.
>
> Returns nothing.

> [spec:libedit:def:readline.rl-callback-read-char-fn]
> void rl_callback_read_char(void)

> [spec:libedit:sem:readline.rl-callback-read-char-fn]
> Reads whatever input is currently available and, if a line has been
> completed, hands it to `rl_linefunc`.
>
> There is no lazy-initialization guard: `e` must already have been set up
> by `rl_callback_handler_install`, otherwise this crashes.
>
> Steps in order:
>
> 1. `buf = el_gets(e, &count)` with `count` initialized to 0. Note this
>    happens *before* unbuffered mode is (re)asserted, so the very first
>    call after installation behaves like a blocking line read.
> 2. `el_set(e, EL_UNBUFFERED, 1)` — re-arms unbuffered mode for the next
>    call.
> 3. `if (buf == NULL || count-- <= 0) return;` — nothing was read. The
>    post-decrement means that when execution continues, `count` is the
>    index of the last byte of `buf`.
> 4. If `count == 0` and `buf[0]` equals the terminal's EOF character
>    (`e->el_tty.t_c[TS_IO][C_EOF]`), sets `done = 1` — a lone EOF keystroke
>    on an empty line.
> 5. If `buf[count]` is `'\n'` or `'\r'`, sets `done = 2` — a completed
>    line. This test overrides the previous one when both hold.
> 6. If `done` is non-zero and `rl_linefunc` is non-NULL: `el_set(e,
>    EL_UNBUFFERED, 0)` leaves unbuffered mode; for `done == 2` it makes
>    `wbuf = strdup(buf)` and, when that succeeded, overwrites `wbuf[count]`
>    with NUL to strip the newline or carriage return, and sets
>    RL_STATE_DONE in `rl_readline_state`; for `done == 1` `wbuf` is NULL.
>    Then calls `(*rl_linefunc)(wbuf)`.
> 7. `_rl_update_pos()` unconditionally — refreshes `rl_point`, `rl_end` and
>    `rl_line_buffer`.
>
> Ownership: `wbuf` is `strdup`'d here and passed to the callback, which
> takes ownership and must release it with `free()`. NULL means end of input
> (and, indistinguishably, that `strdup` failed).
>
> Globals read/written: `rl_linefunc`, `rl_readline_state` (RL_STATE_DONE is
> set but never cleared here — `rl_initialize` is the only place that clears
> it), `rl_point`, `rl_end`, `rl_line_buffer`.
>
> Divergence: readline's callback interface delivers the line without a
> trailing newline and with the prompt already redrawn; libedit's version
> does no redisplay, never calls the callback for partial input, and if
> `rl_linefunc` is NULL it silently drops a completed line.

> [spec:libedit:def:readline.rl-cleanup-after-signal-fn]
> void rl_cleanup_after_signal(void)

> [spec:libedit:sem:readline.rl-cleanup-after-signal-fn]
> Empty stub — the body contains no statements. The source labels the group
> it heads with the comment "unsupported, but needed by python".
>
> Does nothing at all: it does not restore the terminal (that is
> `rl_deprep_terminal`'s job), does not free line state, does not reset the
> display, and touches no global.
>
> GNU readline's `rl_cleanup_after_signal` undoes terminal preparation,
> clears the display state and frees the line buffer so a signal handler can
> safely leave the readline call. Under libedit this is a no-op because
> libedit installs and clears its own signal handlers around `el_gets` — the
> comment at the top of the file records that the readline signal-control
> globals `rl_catch_signals` and `rl_catch_sigwinch` are otherwise not
> honoured either.
>
> Returns nothing. It exists purely so that programs linking against it
> resolve the symbol.

> [spec:libedit:def:readline.rl-compat-sub-fn]
> static char * _rl_compat_sub(const char *str, const char *what, const char *with, int globally)

> [spec:libedit:sem:readline.rl-compat-sub-fn]
> Returns a newly allocated copy of `str` with occurrences of the literal
> substring `what` replaced by `with`. If `globally` is non-zero every
> occurrence is replaced, otherwise only the first.
>
> Pass 1 — sizing. `len = strlen(str)`, `with_len = strlen(with)`, `what_len
> = strlen(what)`. Walks `str`; at each position where `*s == *what` and
> `strncmp(s, what, what_len) == 0`, adds `with_len - what_len` to `len`
> and, if not global, stops; otherwise skips `what_len` bytes. Non-matching
> positions advance by one. The `with_len - what_len` term is `size_t`
> arithmetic and wraps when the replacement is shorter, but modular addition
> still yields the correct final total.
>
> Allocation: `el_calloc(len + 1, 1)`; returns NULL on failure.
>
> Pass 2 — building. Walks `str` again with the same match test. On a match
> it copies `with_len` bytes of `with`, advances the output by `with_len`
> and the input by `what_len`; if not global it then `strcpy`s the entire
> remaining input and returns immediately. Non-matching bytes are copied one
> at a time. After the loop the result is NUL-terminated.
>
> Edge cases: an empty `what` never matches, because the scan stops at the
> input's NUL before `*s == *what` can hold, so the function degenerates to
> a plain copy. Overlapping matches are not possible since the input pointer
> skips the whole matched substring. The match is a byte comparison with no
> locale or case handling, and no multibyte awareness.
>
> Ownership: the returned string belongs to the caller and must be released
> with `free()`. The one in-tree caller, `_history_expand_command`, treats a
> NULL return as "leave the string unsubstituted" rather than as an error.

> [spec:libedit:def:readline.rl-complete-fn]
> int rl_complete(int ignore __attribute__((__unused__)), int invoking_key)

> [spec:libedit:sem:readline.rl-complete-fn]
> Performs completion at the current point. This is what Tab is bound to in
> readline emulation mode; the `ignore` parameter is unused and
> `invoking_key` is the key that triggered it.
>
> Steps:
>
> 1. Two function-static `ct_buffer_t` conversion buffers, `wbreak_conv` and
>    `sprefix_conv`, hold the wide-character forms of the break-character
>    strings across calls; they are grown as needed and never freed.
> 2. Lazily `rl_initialize()` if `h` or `e` is NULL.
> 3. If the exported global `rl_inhibit_completion` is non-zero, completion
>    is disabled: builds the one-character string `{invoking_key, '\0'}`,
>    inserts it into the line with `el_insertstr(e, arr)` and returns
>    CC_REFRESH (4). So a disabled Tab inserts a literal Tab.
> 4. Chooses the break characters: if `rl_completion_word_break_hook` is
>    non-NULL it is called and its result used, otherwise
>    `rl_basic_word_break_characters`. The hook's returned pointer is used
>    directly and never freed.
> 5. `_rl_update_pos()` refreshes `rl_point`, `rl_end` and `rl_line_buffer`
>    before handing them off.
> 6. Returns the result of `fn_complete2(...)` with:
>    - the completion entry generator `rl_completion_entry_function` (may be
>      NULL, in which case filename completion is used);
>    - the application override `rl_attempted_completion_function`;
>    - as the *word-break* argument, the wide decoding of
>      `rl_basic_word_break_characters`;
>    - as the *special-prefixes* argument, the wide decoding of the
>      `breakchars` chosen in step 4;
>    - `_rl_completion_append_character_function` as the append-character
>      supplier;
>    - `(size_t)rl_completion_query_items` as the "ask before listing this
>      many" threshold;
>    - `&rl_completion_type`, `&rl_attempted_completion_over`, `&rl_point`
>      and `&rl_end` as out-parameters that `fn_complete2` writes;
>    - flags `0`, i.e. no FN_QUOTE_MATCH, so returned matches are not
>      re-quoted.
>
> Divergences the port must not silently "fix": the argument in the
> special-prefixes slot is the word-break-hook result, not
> `rl_special_prefixes`, and `rl_completer_word_break_characters` and
> `rl_special_prefixes` are never consulted at all. The return value is a
> libedit CC_* code (CC_NORM, CC_REFRESH, CC_ERROR, ...) rather than
> readline's 0/non-zero status, because the function doubles as an EditLine
> command through `_el_rl_complete`.
>
> Globals written: `rl_completion_type`, `rl_attempted_completion_over`,
> `rl_point`, `rl_end`, and indirectly `rl_line_buffer`. The file also
> defines `#define TAB '\r'` "for rl_complete()", which is dead — nothing
> references it.

> [spec:libedit:def:readline.rl-completion-append-character-function-fn]
> static const char * /*ARGSUSED*/ _rl_completion_append_character_function(const char *dummy __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-completion-append-character-function-fn]
> Supplies the string libedit should append after a unique completion.
>
> The `dummy` parameter (the completed text, in the general contract) is
> unused. The body copies the low byte of the exported global
> `rl_completion_append_character` into a function-static `char buf[2]`,
> NUL-terminates it, and returns `buf`.
>
> Consequences: the return value is a pointer to shared static storage that
> is overwritten by the next call and must not be freed — the completion
> machinery is expected to copy it immediately. Only a single byte can be
> expressed, so a multibyte append character is truncated. If
> `rl_completion_append_character` is 0 (the readline idiom for "append
> nothing") the result is an empty string, which is exactly the desired
> behaviour. The default value is `' '`.
>
> The function is not reentrant and is passed by pointer to both
> `fn_complete2` (from `rl_complete`) and `fn_display_match_list` (from
> `rl_display_match_list`).

> [spec:libedit:def:readline.rl-completion-matches-fn]
> char ** rl_completion_matches(const char *str, rl_compentry_func_t *fun)

> [spec:libedit:sem:readline.rl-completion-matches-fn]
> Runs a completion generator to exhaustion and returns readline's match
> array: element 0 is the longest common prefix of the matches, elements
> 1..n-1 are the matches themselves, and the array is NULL-terminated.
>
> Steps:
>
> 1. `len = 1` (index 0 is reserved for the common prefix), `max = 10`;
>    `list = el_calloc(10, sizeof(char *))`, returning NULL on failure.
> 2. Loop: `match = (*fun)(str, len - 1)` — the generator is called with
>    state 0, 1, 2, ... until it returns NULL. Each returned pointer is
>    stored at `list[len++]` and adopted, not copied. When `len` reaches
>    `max`, `max` grows by 10 and the array is `el_realloc`ed; a failure
>    there jumps to the error exit.
> 3. If `len == 1` no match was produced: jumps to the error exit, which
>    frees the array and returns NULL.
> 4. `list[len] = NULL`.
> 5. If `len == 2` there is exactly one match: `list[0] = strdup(list[1])`
>    and the array is returned. A strdup failure jumps to the error exit,
>    which frees only the array and leaks the single match.
> 6. Otherwise sorts elements 1..len-1 with `qsort(&list[1], len - 1,
>    sizeof(*list), (int (*)(const void *, const void *))strcmp)`. This is
>    wrong twice over: calling `strcmp` through an incompatible
>    function-pointer type is undefined behaviour, and `qsort` passes the
>    comparator the *addresses* of the array elements, so `strcmp` receives
>    `char **` values and compares the raw bytes of the pointers rather than
>    the strings. The resulting order is effectively the allocation-address
>    order. The correctly written comparator `_rl_qsort_string_compare`
>    exists in this same file and is not used here. The port must sort the
>    strings; that changes observable output, and it is the right change.
> 7. Computes `min`, the length of the shortest common prefix over adjacent
>    pairs `list[i]` / `list[i+1]` for `i` in `1..len-2`, starting from
>    SIZE_MAX. (Because step 5 already returned for `len == 2`, this loop
>    always runs at least once here.) Note the prefix length is only
>    meaningful if the array is sorted, so the broken sort corrupts this
>    too.
> 8. If `min == 0` and `str` is non-empty, `list[0] = strdup(str)` — the
>    matches share nothing, so the original text is offered back. Otherwise
>    `list[0]` is an `el_calloc`'d buffer of `min + 1` bytes holding the
>    first `min` bytes of `list[1]`, NUL-terminated. Allocation failure
>    jumps to the error exit.
> 9. Returns `list`.
>
> Error exit: `el_free(list); return NULL;` — this frees only the array,
> never the match strings already stored in it, so every failure after the
> first successful generator call leaks all matches collected so far.
>
> Ownership on success: the caller owns the array and every string in it,
> including `list[0]`, and must `free()` each element and then the array.
> The strings at indices 1..n-1 are exactly the pointers the generator
> returned, so the generator must return heap memory.

> [spec:libedit:def:readline.rl-copy-text-fn]
> char * rl_copy_text(int from, int to)

> [spec:libedit:sem:readline.rl-copy-text-fn]
> Returns a copy of the current line's text between byte offsets `from` and
> `to`.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. `li = el_line(e)`
> gets the narrow `LineInfo` (which also refreshes `rl_line_buffer` via the
> resize callback). If `from > to`, returns NULL. Then each endpoint is
> clamped: if `li->buffer + from` is past `li->lastchar`, `from` is set to
> the line length, and likewise for `to`. `len = to - from`. Allocates `len
> + 1` bytes with `el_malloc`; returns NULL on failure. Copies with
> `strlcpy(out, li->buffer + from, len)` and returns `out`.
>
> Two defects the port must decide about explicitly:
>
> - `strlcpy`'s third argument is the destination *size*, so passing `len`
>   copies only `len - 1` bytes plus a NUL: the last requested character is
>   dropped. The correct call would pass `len + 1`.
> - For `len == 0` (`from == to`), `strlcpy` with size 0 writes nothing at
>   all, so the `el_malloc`'d buffer is returned uninitialized and is not
>   NUL-terminated — reading it is undefined behaviour.
>
> Negative `from` is not checked either, so a caller passing a negative
> offset reads before the start of the buffer.
>
> Ownership: the returned block is heap memory the caller must release with
> `free()`. NULL means either `from > to` or allocation failure. No globals
> are modified beyond the refresh `el_line()` performs.

> [spec:libedit:def:readline.rl-crlf-fn]
> int rl_crlf(void)

> [spec:libedit:sem:readline.rl-crlf-fn]
> Emits a newline into EditLine's display.
>
> The body is `re_putc(e, '\n', 0); return 0;`.
>
> `re_putc` with `shift == 0` writes the character into the *virtual display
> array* at the current refresh cursor position without advancing that
> cursor and without any wrap handling — it does not write to `rl_outstream`
> or to the terminal. Nothing is flushed; the character becomes visible only
> when a subsequent refresh renders the virtual display. There is no NULL
> guard on `e`.
>
> Divergence: GNU readline's `rl_crlf` writes an actual carriage
> return/newline to `rl_outstream` immediately. The libedit version's effect
> depends entirely on the refresh state and is a no-op in many situations.
> Always returns 0.

> [spec:libedit:def:readline.rl-delete-text-fn+1]
> int rl_delete_text(int start, int end)

> [spec:libedit:sem:readline.rl-delete-text-fn+1]
> Deletes the characters between offsets `start` and `end` in the current
> line.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL, then `return
> el_deletestr1(e, start, end);`.
>
> `el_deletestr1` works in *wide characters* against `el->el_line`, not in
> the bytes that `rl_point`/`rl_end` are expressed in, so under a multibyte
> locale the offsets a readline application computes from `rl_line_buffer`
> do not correspond to what is deleted. Endpoint validation, oversized-end
> clamping, deletion, and internal cursor rebasing are exactly those defined
> by `[spec:libedit:sem:histedit.el-deletestr1-fn]`.
>
> Return the number of wide characters actually removed, or 0 when the
> normalized range is empty or rejected. Neither `rl_point` nor `rl_end` is
> refreshed by this call; the caller must invoke something that runs
> `_rl_update_pos`.

> [spec:libedit:def:readline.rl-deprep-terminal-fn]
> void rl_deprep_terminal(void)

> [spec:libedit:sem:readline.rl-deprep-terminal-fn]
> Restores the terminal from editing mode.
>
> The body is `el_set(e, EL_PREP_TERM, 0)`, which makes EditLine restore the
> saved terminal attributes. There is no lazy-initialization guard, so a
> NULL `e` crashes. The `el_set` return value is discarded.
>
> Returns nothing.
>
> This function is the initial value of the exported global
> `rl_deprep_term_function` (installed with a cast from `void (*)(void)`),
> so applications that call `(*rl_deprep_term_function)()` reach it by
> default. Note that `rl_reset_after_signal` calls the *prep* hook, not this
> one, so nothing in the file calls `rl_deprep_terminal` internally —
> `readline()` uses `tty_end(e, TCSADRAIN)` directly instead.

> [spec:libedit:def:readline.rl-ding-fn]
> int rl_ding(void)

> [spec:libedit:sem:readline.rl-ding-fn]
> Rings the terminal bell — in principle.
>
> The body is `re_putc(e, '\a', 0); return 0;`.
>
> As with `rl_crlf`, `re_putc` with `shift == 0` deposits the character into
> EditLine's virtual display array at the current refresh cursor without
> advancing it and without writing to the terminal or to `rl_outstream`.
> Nothing is flushed. Notably this is *not* `el_beep(e)`, which is what
> `_rl_abort_internal` uses and what actually emits the bell; so `rl_ding`
> is close to a no-op in practice, and if the virtual display is later
> rendered it will contain a stray BEL byte at whatever position the refresh
> cursor happened to be.
>
> There is no NULL guard on `e`. Always returns 0.

> [spec:libedit:def:readline.rl-display-match-list-fn]
> void rl_display_match_list(char **matches, int len, int max)

> [spec:libedit:sem:readline.rl-display-match-list-fn]
> Prints `matches` in columns on the editor's output.
>
> The body forwards directly: `fn_display_match_list(e, matches,
> (size_t)len, (size_t)max, _rl_completion_append_character_function)`.
>
> `matches` follows the readline convention — index 0 holds the common
> prefix and is skipped by the display code, indices 1..len are the entries
> actually shown. `len` is the number of matches (not counting element 0)
> and `max` is the length of the longest match, used for column width; both
> are widened to `size_t` without a range check, so a negative argument
> becomes an enormous count.
>
> There is no lazy-initialization guard: `e` must already be non-NULL or
> this crashes. Ownership of `matches` stays with the caller — nothing is
> freed here — and the array is not modified.
>
> Returns nothing. The append-character function is passed so the display
> code can show the same trailing character completion would insert. Note
> this bypasses `rl_completion_display_matches_hook` entirely: that exported
> global exists but nothing in this file consults it.

> [spec:libedit:def:readline.rl-echo-signal-char-fn]
> void rl_echo_signal_char(int sig)

> [spec:libedit:sem:readline.rl-echo-signal-char-fn]
> Echoes the terminal control character associated with signal `sig`.
>
> Steps: `c = tty_get_signal_character(e, sig)`, which returns -1 when the
> terminal's ECHOCTL flag is clear or when the signal has no configured
> character, and otherwise the `c_cc` entry for that signal — VINTR for
> SIGINT, VQUIT for SIGQUIT, and so on — taken from EditLine's *edit-mode*
> termios copy. If it is -1 the function returns immediately. Otherwise
> `re_putc(e, c, 0)`.
>
> As with `rl_crlf` and `rl_ding`, `re_putc` with `shift == 0` writes into
> EditLine's virtual display array at the current refresh cursor without
> advancing it and without emitting anything to the terminal.
>
> Divergence: GNU readline echoes the character in caret notation (`^C`)
> directly to `rl_outstream`. Here the raw control byte is deposited in the
> display buffer, so nothing readable appears; the two behaviours are not
> equivalent and the port should reproduce libedit's, since that is what
> crosses the ABI.
>
> There is no NULL guard on `e`. Returns nothing.

> [spec:libedit:def:readline.rl-erase-entire-line-fn]
> void _rl_erase_entire_line(void)

> [spec:libedit:sem:readline.rl-erase-entire-line-fn]
> Empty stub — the body contains no statements.
>
> Does nothing: the line buffer is not cleared, the cursor is not moved,
> nothing is written to the terminal, and no global is touched. Returns
> nothing.
>
> GNU readline's `_rl_erase_entire_line` moves to column 0, clears to end of
> line and resets the display cursor. Applications (notably ones that print
> asynchronous output above the prompt) that call this expecting a cleared
> line get an unchanged display under libedit.
>
> It exists only so the symbol resolves for programs that reach into
> readline's private namespace.

> [spec:libedit:def:readline.rl-event-read-char-fn]
> static int _rl_event_read_char(EditLine *el, wchar_t *wc)

> [spec:libedit:sem:readline.rl-event-read-char-fn]
> EL_GETCFN replacement installed by `readline()` while `rl_event_hook` is
> set; it polls for input, calling the hook between attempts.
>
> Steps: `ch = '\0'`, `*wc = L'\0'`, `num_read = 0`. Then a loop that runs
> while `rl_event_hook` is non-NULL:
>
> 1. Calls `(*rl_event_hook)()`; its return value is ignored.
> 2. Attempts a non-blocking single-byte read from `el->el_infd`. On a POSIX
>    build this is the FIONREAD branch: `ioctl(el->el_infd, FIONREAD, &n)` —
>    returning -1 immediately if the ioctl fails — then `read(el->el_infd,
>    &ch, 1)` if `n` is non-zero and `num_read = 0` otherwise. (Two fallback
>    branches exist for platforms without FIONREAD: one toggling O_NDELAY
>    around the read, and a last-resort blocking read that then returns -1
>    unconditionally. Neither is reachable on the port's POSIX target.)
> 3. If the read failed with EAGAIN, or read nothing, loops again —
>    immediately, with no sleep and no `select`/`poll`. This is a busy spin
>    that calls the application hook as fast as the CPU allows. Otherwise
>    breaks.
>
> After the loop, if `rl_event_hook` has become NULL (the hook cleared
> itself), reinstalls the builtin reader with `el_set(el, EL_GETCFN,
> EL_BUILTIN_GETCFN)`. Then `*wc = (wchar_t)ch` and returns `num_read`.
>
> Return value: 1 when a byte was read, 0 when the loop exited because the
> hook was cleared before any byte arrived, and a negative value on a read
> or ioctl error.
>
> Divergence to record: exactly one *byte* is read and widened directly to
> `wchar_t` with no multibyte decoding, so non-ASCII input is corrupted
> whenever an event hook is installed, even though the same input read
> through the builtin getc function decodes correctly. `ch` is `char`, so on
> a platform with signed `char` a byte above 0x7F widens to a negative
> `wchar_t`.

> [spec:libedit:def:readline.rl-filename-completion-function-fn]
> char * rl_filename_completion_function (const char *text, int state)

> [spec:libedit:sem:readline.rl-filename-completion-function-fn]
> Pure forwarder: `return fn_filename_completion_function(text, state)`. No
> lazy initialization, no globals read or written, no allocation of its own.
>
> Identical in behaviour to `filename_completion_function` in this same
> file; the two names exist because readline renamed the function and both
> spellings appear in the wild.
>
> Generator protocol as seen by the caller: `state == 0` starts a fresh scan
> for the (possibly partial, possibly tilde-prefixed) pathname `text`; any
> non-zero `state` continues the scan started by the preceding call. Each
> call returns the next matching pathname as a freshly allocated
> NUL-terminated string the caller must release with `free()`, and NULL once
> the matches are exhausted. Scan state lives in statics inside
> `fn_filename_completion_function`, so two interleaved scans are not
> supported.

> [spec:libedit:def:readline.rl-forced-update-display-fn]
> void rl_forced_update_display(void)

> [spec:libedit:sem:readline.rl-forced-update-display-fn]
> Forces a full redraw of the current line.
>
> The body is `el_set(e, EL_REFRESH)`, which makes EditLine recompute and
> repaint the display from its current line and prompt state. The return
> value is discarded and there is no NULL guard on `e`.
>
> Returns nothing.
>
> Callers inside this file: `rl_redisplay` (after pushing the terminal's
> reprint character) and `rl_message` (after replacing the prompt).
>
> Note the exported global `rl_redisplay_function` — readline's indirection
> point for a custom display routine — is declared and initialized to NULL,
> but nothing in this file ever calls through it, so installing a custom
> redisplay function has no effect.

> [spec:libedit:def:readline.rl-free-line-state-fn]
> void rl_free_line_state(void)

> [spec:libedit:sem:readline.rl-free-line-state-fn]
> Empty stub — the body contains no statements.
>
> Does nothing: no line buffer is freed, no undo list is discarded, no kill
> ring is touched, and no global is modified. Returns nothing.
>
> GNU readline's `rl_free_line_state` discards the current line, the undo
> list and any pending completion state; it is normally called from a signal
> handler together with `rl_cleanup_after_signal`. Under libedit it is
> present only so the symbol resolves (the source groups it with the
> functions marked "unsupported, but needed by python"), and the current
> line survives across a signal untouched.

> [spec:libedit:def:readline.rl-generic-bind-fn]
> int /*ARGSUSED*/ rl_generic_bind(int type __attribute__((__unused__)), const char * keyseq __attribute__((__unused__)), const char * data __attribute__((__unused__)), Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-generic-bind-fn]
> Stub. All four parameters are declared unused and the body is `return 0;`.
>
> Nothing is bound. libedit has no readline keymap objects — `rl_get_keymap`
> and `rl_make_bare_keymap` both return NULL — so the `Keymap` argument is
> necessarily NULL, and the `type` (ISFUNC, ISKMAP, ISMACR), `keyseq` and
> `data` arguments are all ignored. No global is touched and nothing is
> copied, so the caller retains ownership of `keyseq` and `data`.
>
> GNU readline installs a function, sub-keymap or macro for the given key
> sequence and returns non-zero on error. Here 0 unconditionally reports
> success, so a caller cannot detect that the binding was dropped; macro
> bindings in particular silently do nothing.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-get-keymap-fn]
> Keymap rl_get_keymap(void)

> [spec:libedit:sem:readline.rl-get-keymap-fn]
> Stub. The body is `return NULL;`.
>
> libedit implements no readline keymaps at all. The exported
> `emacs_standard_keymap`, `emacs_meta_keymap` and `emacs_ctlx_keymap`
> arrays exist as zero-initialized `KEYMAP_ENTRY_ARRAY` objects so that
> programs referencing them link, but nothing populates or consults them,
> and this function does not return any of them.
>
> Consequences for callers: every `Keymap`-taking entry point in this
> compatibility layer (`rl_set_keymap`, `rl_generic_bind`,
> `rl_bind_key_in_map`, `rl_set_key`, `rl_set_keymap_name`) receives NULL
> and ignores it, so the whole keymap API is inert. A caller that
> dereferences the returned pointer — for example to inspect
> `keymap[c].function` — crashes.
>
> Always returns NULL; no global is read or written.

> [spec:libedit:def:readline.rl-get-previous-history-fn]
> int rl_get_previous_history(int count, int key)

> [spec:libedit:sem:readline.rl-get-previous-history-fn]
> Pushes the key byte `key` back into EditLine's input queue `count` times.
>
> The body builds the one-character string `{key, '\0'}` and calls
> `el_push(e, a)` in a `while (count--)` loop, then returns 0. Because the
> test is a post-decrement, a `count` of 0 or negative pushes nothing. There
> is no lazy-initialization guard, so a NULL `e` crashes.
>
> `el_push` decodes the string and prepends it to the pending-input queue,
> so the character is re-read by the editor as though typed. This function
> therefore does *not* move through the history itself: it only works if
> `key` is bound to a history-recall command such as `ed-prev-history`.
> Under the default emacs bindings, calling it with a key that is not so
> bound simply replays that key.
>
> Divergence: GNU readline's `rl_get_previous_history` moves back `count`
> entries in the history and replaces the line buffer, and returns 0 on
> success or non-zero when it cannot move. Here the return is always 0 and
> no history global (`history_offset`, `history_length`) is touched.

> [spec:libedit:def:readline.rl-get-screen-size-fn]
> void rl_get_screen_size(int *rows, int *cols)

> [spec:libedit:sem:readline.rl-get-screen-size-fn]
> Reports the terminal size.
>
> Steps: if `rows` is non-NULL, `el_get(e, EL_GETTC, "li", rows)`; if `cols`
> is non-NULL, `el_get(e, EL_GETTC, "co", cols)`. Either pointer may be NULL
> independently and that half is simply skipped.
>
> The capability codes are the termcap two-letter names `li` (lines) and
> `co` (columns). Under the port's terminfo decision these become the
> terminfo long names `lines` and `columns`; the two-letter spellings must
> still be accepted at this ABI because applications pass them through
> `EL_GETTC` themselves.
>
> Neither `el_get` result is checked, so on failure the caller's variables
> are left with whatever they contained on entry — this function has no way
> to report an error and does not initialize the out-parameters itself.
> There is no lazy-initialization guard, so a NULL `e` crashes.
>
> Returns nothing.

> [spec:libedit:def:readline.rl-initialize-fn]
> int rl_initialize(void)

> [spec:libedit:sem:readline.rl-initialize-fn]
> Creates (or recreates) the EditLine and History objects behind the
> readline compatibility layer and configures them to behave like readline.
> Called lazily by most entry points; applications may also call it
> directly.
>
> Steps in order:
>
> 1. If `e` is non-NULL, `el_end(e)`; if `h` is non-NULL, `history_end(h)`.
>    Neither pointer is cleared first, so a failure later leaves dangling
>    statics; a second `rl_initialize()` therefore tears down the previous
>    session.
> 2. `RL_UNSETSTATE(RL_STATE_DONE)` clears that bit in `rl_readline_state`.
> 3. Defaults the exported streams: `rl_instream` to `stdin` and
>    `rl_outstream` to `stdout` if either is still NULL.
> 4. Decides whether to edit at all: `tcgetattr(fileno(rl_instream), &t)`
>    and, if it succeeds and `ECHO` is clear in `t.c_lflag`, sets `editmode
>    = 0`.
> 5. `e = el_init_internal(rl_readline_name, rl_instream, rl_outstream,
>    stderr, fileno(rl_instream), fileno(rl_outstream), fileno(stderr),
>    NO_RESET)` — the program name used for `prog:` conditionals in
>    `.editrc` is the exported `rl_readline_name` (default the empty
>    string), and NO_RESET means EditLine will not reset the tty on
>    teardown.
> 6. If `editmode` was cleared, `el_set(e, EL_EDITMODE, 0)`.
> 7. `h = history_init()`. If either `e` or `h` is NULL, returns -1 — note
>    this leaves the non-NULL one allocated and stored.
> 8. `history(h, &ev, H_SETSIZE, INT_MAX)` — effectively unlimited — then
>    `history_length = 0` and `max_input_history = INT_MAX`, and `el_set(e,
>    EL_HIST, history, h)` wires the *narrow* history implementation into
>    EditLine.
> 9. `el_set(e, EL_RESIZE, _resize_fun, &rl_line_buffer)` so
>    `rl_line_buffer` tracks the line buffer.
> 10. If `rl_getc_function` is non-NULL, `el_set(e, EL_GETCFN,
>     _getc_function)`. The hook is captured only at this moment; setting
>     `rl_getc_function` afterwards has no effect until the next
>     `rl_initialize()`.
> 11. `rl_set_prompt("")`; on -1 it calls `history_end(h)` and `el_end(e)`
>     and returns -1 — leaving both statics dangling. Then `el_set(e,
>     EL_PROMPT_ESC, _get_prompt, RL_PROMPT_START_IGNORE)` and `el_set(e,
>     EL_SIGNAL, rl_catch_signals)`.
> 12. `el_set(e, EL_EDITOR, "emacs")` before reading any config, so the
>     config file can override it.
> 13. Terminal name: if `rl_terminal_name` is non-NULL it is pushed with
>     `el_set(e, EL_TERMINAL, rl_terminal_name)`; otherwise `el_get(e,
>     EL_TERMINAL, &rl_terminal_name)` *writes* the global with the exact
>     pointer `terminal_set` retained without copying: normally borrowed
>     process-environment storage, or the read-only `"dumb"` literal when
>     `TERM` is absent or unusable. The application must neither free nor
>     mutate it; environment mutation may invalidate it.
> 14. Registers `rl_complete` as an EditLine function (`el_set(e, EL_ADDFN,
>     "rl_complete", "ReadLine compatible completion function",
>     _el_rl_complete)`) and binds it to `^I`; this must come after the
>     emacs rebind.
> 15. Registers `rl_tstp` (`_el_rl_tstp`) and binds it to `^Z`.
> 16. Binds `^R` to `em-inc-search-prev`.
> 17. Binds the Home/End variants `\e[1~`, `\e[7~`, `\e[H` to
>     `ed-move-to-beg` and `\e[4~`, `\e[8~`, `\e[F` to `ed-move-to-end`.
> 18. Binds `\e[3~` to `ed-delete-next-char` and `\e[2~` to
>     `em-toggle-overwrite`.
> 19. Binds `\e[1;5C`, `\e[5C`, `\e\e[C` to `em-next-word` and `\e[1;5D`,
>     `\e[5D`, `\e\e[D` to `ed-prev-word`.
> 20. `el_source(e, NULL)` reads `$EDITRC` or `~/.editrc`; the return value
>     is ignored. Readline's own `~/.inputrc` is *not* read.
> 21. `_resize_fun(e, &rl_line_buffer)` and `_rl_update_pos()` are called
>     directly so `rl_line_buffer`, `rl_point` and `rl_end` are valid
>     immediately — the source comment notes this exists because
>     applications read those globals directly.
> 22. `tty_end(e, TCSADRAIN)` leaves the terminal in its normal state.
> 23. Returns 0.
>
> Return value: 0 on success, -1 if EditLine or History could not be created
> or the prompt could not be set. Every `el_set` result is otherwise
> ignored.

> [spec:libedit:def:readline.rl-insert-fn]
> int rl_insert(int count, int c)

> [spec:libedit:sem:readline.rl-insert-fn]
> Pushes character `c` back onto EditLine's pending-input queue, `count`
> times.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. Builds the
> one-character string `{(char)c, '\0'}` — the source flags the
> `int`-to-`char` narrowing as lossy for multibyte characters — and calls
> `el_push(e, arr)` in a `for (; count > 0; count--)` loop. A `count` of 0
> or negative pushes nothing. Returns 0 unconditionally.
>
> `el_push` decodes the byte and prepends it to the pending-input queue, so
> the character is re-read by the editor as though it had just been typed,
> and whatever the current key binding for it is will run.
>
> Divergence to preserve: GNU readline's `rl_insert` inserts `c` into the
> line buffer at point `count` times. libedit's version does not touch the
> line at all — it re-injects the keystroke. Its counterpart
> `rl_stuff_char`, which in readline pushes onto the input stream, instead
> *inserts into the line* here; the two are effectively swapped relative to
> readline.
>
> Note also that `rl_bind_key` compares its `func` argument against this
> function's address as the sole way to request a self-insert binding.

> [spec:libedit:def:readline.rl-insert-text-fn]
> int rl_insert_text(const char *text)

> [spec:libedit:sem:readline.rl-insert-text-fn]
> Inserts `text` into the current line at the cursor.
>
> Steps: if `text` is NULL or the empty string, returns 0 immediately —
> before any initialization, so this is safe to call on an uninitialized
> library only in that case. Otherwise lazily `rl_initialize()` if `h` or
> `e` is NULL, then `el_insertstr(e, text)`, which decodes the bytes to wide
> characters through EditLine's legacy conversion buffer and splices them in
> at the cursor, advancing the cursor past the inserted text. If that
> returns a negative value (no room in the line buffer, or a decoding
> failure), returns 0.
>
> Otherwise returns `(int)strlen(text)` — the number of *bytes* inserted,
> not the number of characters, so under a multibyte locale the count
> exceeds the number of characters actually added to the line.
>
> Return value: the byte length on success, 0 on failure or for empty input;
> the two zeros are indistinguishable.
>
> Neither `rl_point` nor `rl_end` is refreshed by this call; the caller must
> trigger `_rl_update_pos` (for example via `rl_complete` or the
> `rl_bind_wrapper` path) to see the new values.

> [spec:libedit:def:readline.rl-kill-full-line-fn]
> int /*ARGSUSED*/ rl_kill_full_line(int count __attribute__((__unused__)), int key __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-kill-full-line-fn]
> Deletes the entire current line.
>
> Both parameters are declared unused. The body is `em_kill_line(e, 0);
> return 0;` — it calls EditLine's emacs `em-kill-line` editor command
> directly, bypassing the key map, with a zero key argument.
>
> `em_kill_line` copies the whole line into EditLine's kill buffer and then
> empties the line, leaving cursor and `lastchar` at the buffer start. So
> the text is killed in the EditLine sense (recoverable with `em-yank`), not
> merely discarded — but it does not land in readline's kill ring, which
> this compatibility layer does not implement at all (`rl_kill_text` is a
> pure stub).
>
> There is no lazy-initialization guard, so a NULL `e` crashes, and
> `em_kill_line`'s CC_* return is discarded.
>
> Returns 0 unconditionally. Neither `rl_point`, `rl_end` nor
> `rl_line_buffer` is refreshed by this call.

> [spec:libedit:def:readline.rl-kill-text-fn]
> int /*ARGSUSED*/ rl_kill_text(int from __attribute__((__unused__)), int to __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-kill-text-fn]
> Stub. Both parameters are declared unused and the body is `return 0;`.
>
> Nothing is deleted, nothing is copied to any kill ring, and no global is
> touched. libedit's readline layer has no kill ring: `rl_kill_full_line`
> uses EditLine's own kill buffer instead, and there is no `rl_yank`
> counterpart exported at all.
>
> GNU readline's `rl_kill_text` deletes the text between `from` and `to` and
> pushes it onto the kill ring, returning 0. Because this stub also returns
> 0, a caller cannot distinguish "killed" from "did nothing" — the line is
> simply left unchanged. Applications that want the deletion should call
> `rl_delete_text` instead.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-make-bare-keymap-fn]
> Keymap rl_make_bare_keymap(void)

> [spec:libedit:sem:readline.rl-make-bare-keymap-fn]
> Stub. The body is `return NULL;`.
>
> No keymap object is allocated, so there is nothing for the caller to free
> and nothing to populate. This is the constructor half of a keymap API that
> libedit does not implement: `rl_get_keymap` also returns NULL,
> `rl_set_keymap` is a no-op, and `rl_generic_bind`, `rl_bind_key_in_map`
> and `rl_set_key` all ignore the `Keymap` they are handed.
>
> GNU readline returns a freshly allocated, empty `KEYMAP_ENTRY_ARRAY` of
> KEYMAP_SIZE (256) entries that the caller owns and eventually frees with
> `rl_discard_keymap`/`rl_free_keymap` — neither of which exists here. A
> caller that indexes the result crashes.
>
> Always returns NULL; no global is read or written.

> [spec:libedit:def:readline.rl-message-fn]
> void rl_message(const char *format, ...)

> [spec:libedit:sem:readline.rl-message-fn]
> Replaces the prompt with a formatted message and redraws.
>
> Steps: formats the varargs into a 160-byte stack buffer (`MAX_MESSAGE`)
> with `vsnprintf`, which truncates silently at 159 characters plus NUL and
> whose return value is discarded. Calls `rl_set_prompt(msg)` — so the
> message *becomes* `rl_prompt`, overwriting whatever was there and freeing
> the previous buffer — and then `rl_forced_update_display()` to repaint.
>
> The declaration carries `__attribute__((__format__(__printf__, 1, 2)))`,
> so the format string is checked at compile time in the C header; the
> port's exported ABI must accept a C varargs `printf`-style call.
>
> Returns nothing. `rl_set_prompt`'s failure return is discarded, so an
> allocation failure leaves the old prompt in place unnoticed.
>
> Contract the caller is expected to follow, and a trap if they do not:
> because the message overwrites the prompt outright, the application must
> have called `rl_save_prompt()` first and must call `rl_restore_prompt()`
> afterwards; otherwise the original prompt is gone for good. Note
> `rl_restore_prompt` leaks the message buffer when it does so. The 160-byte
> truncation is a libedit-specific limit with no readline counterpart.

> [spec:libedit:def:readline.rl-newline-fn]
> int rl_newline(int count __attribute__((__unused__)), int c __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-newline-fn]
> Both parameters are declared unused — the source notes that readline 4.0
> appears to ignore them too — and the body is `return rl_insert(1, '\n');`.
>
> So it pushes a single `'\n'` byte onto EditLine's pending-input queue (via
> `el_push`), which lazily initializes the library first if needed. The
> newline is then re-read by the editor and whatever is bound to it runs —
> under the default emacs bindings, `ed-newline`, which terminates the line.
> It does *not* insert a literal newline into the line buffer and does not
> itself set `rl_done`.
>
> Returns `rl_insert`'s value, which is always 0.
>
> Divergence: GNU readline's `rl_newline` accepts the line immediately,
> running the accept-line machinery; here the effect is indirect and depends
> on the current binding of `'\n'`, so rebinding that key changes what
> `rl_newline` does.

> [spec:libedit:def:readline.rl-on-new-line-fn]
> int rl_on_new_line(void)

> [spec:libedit:sem:readline.rl-on-new-line-fn]
> Stub. The body is `return 0;`.
>
> Does nothing: no display state is reset, `rl_point`, `rl_end` and
> `rl_line_buffer` are untouched, and EditLine is not told that the cursor
> has moved to a fresh line. Returns 0, which in readline's convention means
> success.
>
> GNU readline's `rl_on_new_line` tells the redisplay engine that the cursor
> is now at column 0 of a new line, so the next redisplay does not try to
> erase text that the application has already scrolled away. It is the
> standard way to print asynchronous output above a prompt. Under libedit
> that coordination does not exist, so an application printing around the
> prompt will see the display state and the terminal disagree;
> `rl_forced_update_display()` is the closest available remedy.

> [spec:libedit:def:readline.rl-parse-and-bind-fn]
> int rl_parse_and_bind(const char *line)

> [spec:libedit:sem:readline.rl-parse-and-bind-fn]
> Parses one configuration line and applies it, using libedit's `.editrc`
> grammar rather than readline's `.inputrc` grammar.
>
> Steps: `tok = tok_init(NULL)` creates a tokenizer with the default quoting
> characters; its return value is *not* checked, so an allocation failure
> crashes on the next call. `tok_str(tok, line, &argc, &argv)` splits the
> line into an argument vector owned by the tokenizer; its return value is
> also unchecked, so an incomplete quote leaves `argc` and `argv` in
> whatever state it left them. `argc = el_parse(e, argc, argv)` dispatches
> the command. `tok_end(tok)` frees the tokenizer and, with it, `argv`.
>
> `el_parse` returns 0 when the command was recognized and executed (and
> also when the line carried a `prog:` prefix that did not match this
> program, in which case it is skipped), and non-zero — -1 for an unknown
> command, or the negation of the command's own status — otherwise.
>
> Return value: `argc ? 1 : 0`, i.e. 1 on failure and 0 on success, which
> matches readline's convention of 0 for success. Note `e` is used without a
> lazy-initialization guard, so calling this before `rl_initialize()`
> crashes.
>
> Divergence the port must keep: the accepted syntax is libedit's (`bind`,
> `echotc`, `settc`, `setty`, `telltc`, `history`, ...), not readline's
> `"\C-x": command` or `set variable value` forms, so most real `.inputrc`
> lines are rejected with 1.

> [spec:libedit:def:readline.rl-prep-terminal-fn]
> void /*ARGSUSED*/ rl_prep_terminal(int meta_flag __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-prep-terminal-fn]
> Puts the terminal into editing mode.
>
> The `meta_flag` parameter is declared unused: readline uses it to request
> eight-bit input, and libedit ignores the request entirely. The body is
> `el_set(e, EL_PREP_TERM, 1)`, which makes EditLine save the current
> terminal attributes and install its editing attributes. The `el_set`
> return value is discarded and there is no lazy-initialization guard, so a
> NULL `e` crashes.
>
> Returns nothing.
>
> This function is the initial value of the exported global
> `rl_prep_term_function` (installed with a cast from `void (*)(int)`), so
> applications that call `(*rl_prep_term_function)(1)` reach it by default,
> and `rl_reset_after_signal` calls through that pointer. It is paired with
> `rl_deprep_terminal`, but `readline()` itself uses `tty_init`/`tty_end`
> directly and never calls either.

> [spec:libedit:def:readline.rl-qsort-string-compare-fn]
> int _rl_qsort_string_compare(char **s1, char **s2)

> [spec:libedit:sem:readline.rl-qsort-string-compare-fn]
> Comparison function for sorting an array of `char *`.
>
> The body is `return strcoll(*s1, *s2);` — it dereferences both `char **`
> arguments and compares the strings with `strcoll`, so ordering follows the
> current `LC_COLLATE` locale rather than raw byte order. The parameter
> types are `char **`, which is the element type of the array, so this is
> intended to be cast to `qsort`'s comparator type at the call site.
>
> Return value: negative, zero or positive per `strcoll`. `strcoll` may set
> `errno` on invalid multibyte sequences; that is not checked, and no global
> is otherwise read or written.
>
> Notable: this function is *not* used anywhere in libedit.
> `rl_completion_matches`, which is the one place that sorts completion
> matches, instead casts `strcmp` itself to the comparator type — which
> compares the pointer bytes rather than the strings and is undefined
> behaviour. The correct comparator sitting unused beside the broken call is
> worth recording, because the port fixing that call will change observable
> match ordering.

> [spec:libedit:def:readline.rl-read-init-file-fn]
> int rl_read_init_file(const char *s)

> [spec:libedit:sem:readline.rl-read-init-file-fn]
> Reads a configuration file.
>
> The whole body is `return el_source(e, s);`. There is no
> lazy-initialization guard, so a NULL `e` crashes.
>
> `el_source` with a non-NULL `s` opens that path. With `s == NULL` it looks
> up `EDITRC` in the environment and, failing that, builds `$HOME/.editrc`;
> if `HOME` is unset it returns -1 without reading anything. Each line read
> is tokenized and dispatched through `el_parse`, so the grammar is
> libedit's, not readline's.
>
> Return value: `el_source`'s — 0 when the file was read (including when
> individual lines failed to parse), -1 when the file could not be opened or
> the path could not be constructed. GNU readline's `rl_read_init_file`
> returns 0 on success and an errno value on failure, so the failure
> encoding differs.
>
> Divergence: readline would read `~/.inputrc` and understand `set`/`"key":
> function` syntax. Here a readline init file is read as `.editrc` and
> almost every line is rejected.

> [spec:libedit:def:readline.rl-read-key-fn]
> int rl_read_key(void)

> [spec:libedit:sem:readline.rl-read-key-fn]
> Reads one key from the input.
>
> Steps: declares a `char fooarr[2 * sizeof(int)]` scratch buffer, lazily
> `rl_initialize()`s if `e` or `h` is NULL, and returns `el_getc(e,
> fooarr)`.
>
> `el_getc` reads one wide character (handling pushed-back input from
> `el_push` first), converts it back to a single byte with `wctob`, stores
> that byte in `fooarr[0]`, and returns 1 on success, 0 at end of input, or
> -1 on error — setting `errno` to ERANGE when the character has no
> single-byte representation.
>
> Bug to record rather than fix silently: the character itself is written
> into `fooarr` and then discarded when the function returns, and what is
> returned is `el_getc`'s *status*. So `rl_read_key()` yields 1 for every
> successful read regardless of which key was pressed, 0 at EOF and -1 on
> error. GNU readline's `rl_read_key` returns the character read. Any
> application that switches on the return value sees only the constant 1.
>
> The oversized scratch buffer is harmless — `el_getc` writes exactly one
> byte — and nothing else is allocated or freed. No global is modified.

> [spec:libedit:def:readline.rl-redisplay-fn]
> void rl_redisplay(void)

> [spec:libedit:sem:readline.rl-redisplay-fn]
> Redraws the current line.
>
> Steps: builds the one-character string `{e->el_tty.t_c[TS_IO][C_REPRINT],
> '\0'}` — the terminal's configured "reprint line" control character, taken
> from EditLine's I/O termios copy — and pushes it onto the pending-input
> queue with `el_push(e, a)`. Then calls `rl_forced_update_display()`, which
> issues `el_set(e, EL_REFRESH)`.
>
> There is no lazy-initialization guard, so a NULL `e` crashes. Returns
> nothing.
>
> Consequence to preserve: because the reprint character is *pushed as
> input*, the next read pulls it out of the queue and runs whatever is bound
> to it (normally `ed-redisplay`), so the redraw happens twice — once
> immediately through EL_REFRESH and once when the pushed character is
> consumed. If the terminal has no reprint character configured, a NUL byte
> is pushed, and `el_push` of a string whose first byte is NUL is an empty
> push.
>
> Note the exported global `rl_redisplay_function` is never consulted;
> installing a custom redisplay routine has no effect on this function.

> [spec:libedit:def:readline.rl-replace-line-fn]
> void rl_replace_line(const char * text, int clear_undo __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-replace-line-fn]
> Replaces the entire current line with `text`.
>
> The `clear_undo` parameter is declared unused — libedit's readline layer
> has no undo list to clear.
>
> Steps: if `text` is NULL or the empty string, returns immediately without
> touching anything. Otherwise lazily `rl_initialize()` if `h` or `e` is
> NULL, then `el_replacestr(e, text)`, which decodes the bytes to wide
> characters through EditLine's legacy conversion buffer and swaps them in
> for the current line contents. The return value is discarded, so failures
> (line too long, decoding error) are invisible.
>
> Returns nothing.
>
> Divergence to preserve: passing `""` is a silent no-op here, whereas GNU
> readline's `rl_replace_line("")` clears the line. Applications that clear
> the line that way get no effect at all under libedit.
>
> Neither `rl_point`, `rl_end` nor `rl_line_buffer` is refreshed by this
> call; the caller must trigger `_rl_update_pos` to observe the new state.

> [spec:libedit:def:readline.rl-reset-after-signal-fn]
> void rl_reset_after_signal(void)

> [spec:libedit:sem:readline.rl-reset-after-signal-fn]
> Re-establishes terminal editing mode after a signal.
>
> The body is: if the exported global `rl_prep_term_function` is non-NULL,
> call it with the argument 1. Its default value is `rl_prep_terminal`
> (installed with a cast from `void (*)(int)`), which issues `el_set(e,
> EL_PREP_TERM, 1)`. An application that replaced the hook gets its own
> function called with 1 — readline's "meta on" flag, which libedit's own
> implementation ignores.
>
> Returns nothing. Nothing else is done: the display is not repainted, the
> line is not redrawn, and no global is modified.
>
> Note the asymmetry with `rl_cleanup_after_signal`, which is an empty stub
> — so the conventional readline pairing of cleanup-then-reset only performs
> the second half here. This is consistent with the file's design note that
> signals are handled the libedit way (handlers installed on entry to
> `el_gets` and cleared on the way out) rather than through readline's
> `rl_catch_signals` machinery.

> [spec:libedit:def:readline.rl-reset-terminal-fn]
> int rl_reset_terminal(const char *p __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-reset-terminal-fn]
> Resets EditLine's terminal state.
>
> The `p` parameter — readline's terminal *name* — is declared unused and
> ignored entirely; the port must not attempt to re-query terminfo for it
> here. To change the terminal type an application has to set
> `rl_terminal_name` before `rl_initialize()`, or use `EL_TERMINAL`
> directly.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL, then `el_reset(e)`,
> whose return value is discarded. `el_reset` discards pending input, clears
> the character-read state and resets the editor's argument/kill state; it
> does not re-read terminal capabilities and does not repaint.
>
> Always returns 0, so failures are not reportable. GNU readline's
> `rl_reset_terminal` reinitializes the terminal-dependent tables for the
> named terminal and also returns 0.

> [spec:libedit:def:readline.rl-resize-terminal-fn]
> void rl_resize_terminal(void)

> [spec:libedit:sem:readline.rl-resize-terminal-fn]
> Tells EditLine that the window size has changed.
>
> The whole body is `el_resize(e)`. There is no lazy-initialization guard,
> so a NULL `e` crashes, and the call's effect is not reported.
>
> `el_resize` re-queries the terminal size (via TIOCGWINSZ, falling back to
> the capability values), rebuilds the display arrays for the new geometry
> and repaints the current line. It runs with signals blocked internally, so
> it is safe to call from ordinary code but is *not* itself
> async-signal-safe — an application handling SIGWINCH should set a flag and
> call this from its main loop.
>
> Returns nothing.
>
> Related globals: `rl_catch_sigwinch` is exported and defaults to 1 but is
> never consulted; libedit installs its own SIGWINCH handling around
> `el_gets` instead, so window resizes are usually handled without the
> application calling this at all.

> [spec:libedit:def:readline.rl-restore-prompt-fn]
> void rl_restore_prompt(void)

> [spec:libedit:sem:readline.rl-restore-prompt-fn]
> Restores the prompt saved by `rl_save_prompt`.
>
> Steps: if the exported global `rl_prompt_saved` is NULL, returns
> immediately — so calling restore without a matching save is a safe no-op.
> Otherwise assigns `rl_prompt = rl_prompt_saved` and then `rl_prompt_saved
> = NULL`.
>
> Ownership trap to record: the current `rl_prompt` is *overwritten, not
> freed*, so whatever prompt was in place — typically the message buffer
> that `rl_message` installed via `rl_set_prompt` — is leaked on every
> save/message/restore cycle. Ownership of the saved buffer transfers back
> to `rl_prompt`, which `rl_set_prompt` will `el_free` on its next
> successful call.
>
> Nothing is redrawn: the display still shows the message until something
> triggers a refresh, so callers normally follow with
> `rl_forced_update_display()`.
>
> Returns nothing. Saves do not nest — a second `rl_save_prompt` before a
> restore discards the first saved pointer.

> [spec:libedit:def:readline.rl-save-prompt-fn]
> void rl_save_prompt(void)

> [spec:libedit:sem:readline.rl-save-prompt-fn]
> Saves the current prompt so a later `rl_restore_prompt` can put it back.
>
> The entire body is `rl_prompt_saved = strdup(rl_prompt);`.
>
> Three hazards the port must handle deliberately:
>
> - `rl_prompt` is not checked for NULL. It is NULL until `rl_set_prompt()`
>   has succeeded once, so calling `rl_save_prompt()` before
>   `rl_initialize()` or `readline()` passes NULL to `strdup`, which is
>   undefined behaviour and crashes on glibc.
> - The `strdup` result is not checked, so on allocation failure
>   `rl_prompt_saved` silently becomes NULL and the subsequent
>   `rl_restore_prompt()` does nothing, losing the prompt.
> - Saves do not nest and the previous saved value is not freed: calling
>   `rl_save_prompt()` twice without an intervening restore leaks the first
>   copy.
>
> The saved copy is `strdup`'d, i.e. plain `malloc` memory, and ownership
> passes to `rl_prompt` on restore — where `rl_set_prompt` will later free
> it with `el_free`. Nothing is redrawn.
>
> Returns nothing. Globals written: `rl_prompt_saved`.

> [spec:libedit:def:readline.rl-set-key-fn]
> int rl_set_key(const char *keyseq __attribute__((__unused__)), rl_command_func_t *function __attribute__((__unused__)), Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-set-key-fn]
> Stub. All three parameters are declared unused and the body is `return
> 0;`.
>
> Nothing is bound. libedit has no readline keymaps, so the `Keymap`
> argument is necessarily NULL (`rl_get_keymap` and `rl_make_bare_keymap`
> both return NULL), and `keyseq` and `function` are ignored without being
> copied — the caller keeps ownership of the key sequence string.
>
> GNU readline's `rl_set_key` binds a key sequence to a command in the given
> keymap, translating the sequence through the current conversion rules, and
> returns non-zero on error. Here 0 unconditionally reports success, so a
> caller cannot detect that the binding was dropped. The only working route
> to a custom binding in this layer is `rl_add_defun`, which binds exactly
> one byte.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-set-keyboard-input-timeout-fn]
> int /*ARGSUSED*/ rl_set_keyboard_input_timeout(int u __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-set-keyboard-input-timeout-fn]
> Stub. The parameter is declared unused and the body is `return 0;`.
>
> No timeout is stored and none is applied: reads through
> `el_gets`/`el_getc` block as they always do, and `_rl_event_read_char`
> busy-polls without any timeout either. No global is touched.
>
> GNU readline's `rl_set_keyboard_input_timeout` sets the microsecond
> timeout used when deciding whether an escape sequence is complete, and
> returns the *previous* value. This stub returns 0 unconditionally, so a
> caller that saves the return value in order to restore it later will
> restore 0 — which in readline means "no wait at all". Applications relying
> on that round trip must be assumed to be relying on nothing.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-set-keymap-fn]
> void /*ARGSUSED*/ rl_set_keymap(Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-set-keymap-fn]
> Stub. The parameter is declared unused and the body is empty.
>
> No keymap is installed and nothing is recorded, because libedit has no
> readline keymap objects at all — `rl_get_keymap` and `rl_make_bare_keymap`
> both return NULL, so the only `Keymap` value a caller can obtain to pass
> here is NULL. The exported `emacs_standard_keymap`, `emacs_meta_keymap`
> and `emacs_ctlx_keymap` arrays exist zero-initialized so that programs
> referencing them link, but passing one of them here has no effect either.
>
> Key bindings continue to come from EditLine's own map, configured by
> `rl_initialize`, `.editrc` and `rl_add_defun`/`rl_bind_key`.
>
> Returns nothing; no global is read or written.

> [spec:libedit:def:readline.rl-set-keymap-name-fn]
> int rl_set_keymap_name(const char *name, Keymap k)

> [spec:libedit:sem:readline.rl-set-keymap-name-fn]
> Stub. The body is `return name && k ? 0 : 0;` — both parameters are read
> only to suppress unused-parameter warnings and every path evaluates to 0.
>
> No name is registered and nothing is stored, because libedit has no
> readline keymaps to name. `name` is not copied, so the caller retains
> ownership.
>
> GNU readline's `rl_set_keymap_name` associates a name with a keymap so it
> can later be found by `rl_get_keymap_by_name`, returning 0 on success and
> -1 when the name is already taken or the keymap is invalid. Because this
> stub always returns 0, a caller cannot detect that the association was
> dropped; a subsequent lookup by that name has no counterpart in this
> library at all.
>
> Always returns 0.

> [spec:libedit:def:readline.rl-set-prompt-fn]
> int rl_set_prompt(const char *prompt)

> [spec:libedit:sem:readline.rl-set-prompt-fn]
> Sets the prompt string, translating readline's invisible-text brackets
> into the single toggle character EditLine understands.
>
> Steps:
>
> 1. If `prompt` is NULL it is replaced by `""`.
> 2. If `rl_prompt` is already non-NULL and `strcmp`s equal to `prompt`,
>    returns 0 immediately without reallocating — so the existing buffer and
>    any pointer EditLine holds to it stay valid.
> 3. Otherwise `el_free(rl_prompt)` (skipped when it is NULL) and `rl_prompt
>    = strdup(prompt)`. Note the allocation/free pairing is deliberately
>    asymmetric — freed with `el_free`, allocated with `strdup` — which
>    matters only if the port makes those different allocators. If `strdup`
>    returns NULL, `rl_prompt` is left NULL and the function returns -1.
> 4. Marker rewrite loop: repeatedly `strchr(rl_prompt,
>    RL_PROMPT_END_IGNORE)` (`'\002'`). For each occurrence, if the very
>    next byte is `RL_PROMPT_START_IGNORE` (`'\001'`), both bytes are
>    removed with `memmove(p, p + 2, 1 + strlen(p + 2))` — collapsing an
>    adjacent end/start pair so two abutting invisible regions do not
>    produce a double escape. Otherwise the `'\002'` byte is overwritten in
>    place with `'\001'`. The loop restarts the search from the beginning of
>    the string each time, so it is quadratic in the number of markers but
>    always terminates.
> 5. Returns 0.
>
> Why the rewrite: `rl_initialize` registers the prompt with `el_set(e,
> EL_PROMPT_ESC, _get_prompt, RL_PROMPT_START_IGNORE)`, and EditLine's
> prompt renderer toggles "this text occupies no columns" on each occurrence
> of that one delimiter. Readline instead brackets invisible text with a
> distinct start and end marker. Mapping every end marker onto the start
> marker makes the toggle scheme reproduce readline semantics.
>
> Globals: `rl_prompt` is written and owned by this module; callers must not
> free it, and `_get_prompt` hands the same pointer to EditLine. Return
> value: 0 on success (including the unchanged-prompt fast path), -1 on
> allocation failure.

> [spec:libedit:def:readline.rl-set-screen-size-fn]
> void rl_set_screen_size(int rows, int cols)

> [spec:libedit:sem:readline.rl-set-screen-size-fn]
> Overrides the terminal size EditLine believes it has.
>
> Steps: formats `rows` into a 64-byte stack buffer with `snprintf` and
> issues `el_set(e, EL_SETTC, "li", buf, NULL)`; then formats `cols` into
> the same buffer and issues `el_set(e, EL_SETTC, "co", buf, NULL)`. The
> values are passed as decimal strings because `EL_SETTC` takes strings.
> Both `el_set` results are discarded and there is no lazy-initialization
> guard, so a NULL `e` crashes.
>
> The capability codes are the termcap two-letter names `li` (lines) and
> `co` (columns). Under the port's terminfo decision these become the
> terminfo long names `lines` and `columns`, but the two-letter spellings
> must still be accepted here because they cross the ABI.
>
> Returns nothing. Note the display arrays are not resized by this call the
> way `el_resize` would resize them, so setting an implausible size changes
> only the numbers EditLine reasons with. Negative values are formatted and
> accepted without validation.

> [spec:libedit:def:readline.rl-stuff-char-fn]
> int rl_stuff_char(int c)

> [spec:libedit:sem:readline.rl-stuff-char-fn]
> Inserts character `c` into the current line at the cursor.
>
> Steps: builds the one-character string `{(char)c, '\0'}` — the
> `int`-to-`char` narrowing is lossy for anything outside a single byte —
> and calls `el_insertstr(e, buf)`, discarding the result. Returns 1
> unconditionally.
>
> There is no lazy-initialization guard, so a NULL `e` crashes. If `c` is 0
> the resulting string is empty and `el_insertstr` inserts nothing.
>
> Divergence the port must preserve: GNU readline's `rl_stuff_char` pushes
> the character onto readline's *pending input* so it is read as though
> typed, returning 1 on success and 0 when the stuff buffer is full.
> libedit's version instead inserts it into the line buffer — which is what
> readline's `rl_insert` does — while libedit's `rl_insert` pushes onto the
> input queue. The two functions are effectively swapped relative to
> readline, and the return value here never reports failure.
>
> Neither `rl_point`, `rl_end` nor `rl_line_buffer` is refreshed by this
> call.

> [spec:libedit:def:readline.rl-update-pos-fn]
> static void _rl_update_pos(void)

> [spec:libedit:sem:readline.rl-update-pos-fn]
> Refreshes the exported line-state globals from EditLine's current line.
>
> Steps: `li = el_line(e)` obtains the narrow `LineInfo`, which re-encodes
> the wide edit line into EditLine's legacy conversion buffer, recomputes
> `cursor` and `lastchar` as byte offsets into it, and invokes the
> registered resize callback `_resize_fun` — which is what assigns
> `rl_line_buffer`. Then:
>
> - `rl_point = (int)(li->cursor - li->buffer)` — the cursor position in
>   *bytes* from the start of the encoded line, not in characters.
> - `rl_end = (int)(li->lastchar - li->buffer)` — the line length in bytes.
> - `rl_line_buffer[rl_end] = '\0'` — writes a terminator into EditLine's
>   conversion buffer at the end of the line.
>
> There are no guards: `e` must be non-NULL and `rl_line_buffer` must
> already point at the conversion buffer, otherwise this dereferences NULL.
> In practice `rl_initialize` calls `_resize_fun` and then this function
> directly, so both are valid from initialization onward.
>
> Direction of flow: this is strictly EditLine → globals. Nothing here reads
> `rl_point` or `rl_end` back into the editor, so assignments an application
> makes to those globals are discarded the next time this runs.
>
> Called from `rl_complete`, `rl_bind_wrapper`, `rl_callback_read_char` and
> the tail of `rl_initialize`. Returns nothing.

> [spec:libedit:def:readline.rl-variable-bind-fn]
> int rl_variable_bind(const char *var, const char *value)

> [spec:libedit:sem:readline.rl-variable-bind-fn]
> Sets a readline-style variable — by routing it through libedit's `bind`
> command.
>
> The body is `return el_set(e, EL_BIND, "", var, value, NULL) == -1 ? 1 :
> 0;`.
>
> `EL_BIND` takes a NULL-terminated argument vector in which the first
> element is the command name and is ignored by the parser, so the effective
> invocation is `bind <var> <value>`: `var` is interpreted as a *key* and
> `value` as the *command* to bind it to. This is not readline's variable
> namespace at all — `rl_variable_bind("editing-mode", "vi")` and similar
> calls do not do what a readline application expects, and most readline
> variable names are rejected as unparsable key specifications.
>
> There is no lazy-initialization guard, so a NULL `e` crashes.
>
> Return value: 0 when `el_set` succeeded, 1 when it returned -1. The source
> records that the proper return value is undocumented in readline and that
> this is what the readline sources appear to do. No global is read or
> written.

> [spec:libedit:def:readline.stifle-history-fn]
> void stifle_history(int max)

> [spec:libedit:sem:readline.stifle-history-fn]
> Caps the history at `max` entries, discarding the oldest ones immediately.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. Calls `history(h,
> &ev, H_SETSIZE, max)`; if it returns anything other than 0, nothing
> further happens and no global changes.
>
> On success:
>
> 1. `max_input_history = max` — this mirror is what `history_is_stifled`
>    and `unstifle_history` consult.
> 2. If `history_length > max`, sets `history_base = history_length - max`,
>    so subsequent `history_get` indices stay aligned with the surviving
>    entries.
> 3. Then, while `history_length > max`, evicts the oldest entry: `he =
>    remove_history(0)` (index 0 is the oldest), followed by
>    `el_free(he->data)`, `el_free((void *)(unsigned long)he->line)` and
>    `el_free(he)`. `remove_history` refreshes `history_length` from
>    H_GETSIZE on each iteration, which is what terminates the loop.
>
> Hazards to record rather than reproduce:
>
> - `remove_history` can return NULL (allocation failure, or the history
>   running out before `history_length` says it should) and the result is
>   dereferenced without a check — a NULL dereference.
> - `he->line` is cast through `unsigned long` to strip `const`. On LP64
>   that round-trips, but it is not pointer-safe in general and is undefined
>   behaviour on any platform where `unsigned long` is narrower than a
>   pointer.
> - `he->data` is the application's `histdata_t`; freeing it here assumes it
>   was allocated with the same allocator, which the readline API never
>   promised.
>
> This is nonetheless the only place in the file that disposes of a
> `remove_history` result correctly, since `free_history_entry` is a no-op.
>
> Returns nothing. Failure of H_SETSIZE is silently ignored.

> [spec:libedit:def:readline.tilde-expand-fn]
> char * tilde_expand(char *name)

> [spec:libedit:sem:readline.tilde-expand-fn]
> Expands a leading `~` or `~user` in `name` to the corresponding home
> directory.
>
> Pure forwarder: `return fn_tilde_expand(name);`. No lazy initialization,
> no globals read or written.
>
> The parameter is declared `char *` rather than `const char *` for readline
> source compatibility, but the string is not modified — `fn_tilde_expand`
> takes `const char *`.
>
> Behaviour of the callee that the caller depends on: a name that does not
> begin with `~` is returned as a plain copy; `~` alone or `~/...` uses the
> home directory of the current real uid taken from the password database;
> `~user` looks `user` up in the password database and returns NULL if there
> is no such user. The result is always a freshly allocated string, so the
> caller must release it with `free()`, and NULL means either an unknown
> user or an allocation failure.

> [spec:libedit:def:readline.unstifle-history-fn]
> int unstifle_history(void)

> [spec:libedit:sem:readline.unstifle-history-fn]
> Removes the history size cap.
>
> Steps: `history(h, &ev, H_SETSIZE, INT_MAX)` — its return value is not
> checked. Saves the previous `max_input_history` into a local, sets
> `max_input_history = INT_MAX`, and returns the saved value. The source
> comments that "some value _must_ be returned".
>
> Hazard: there is *no* lazy-initialization guard here, unlike almost every
> other history entry point in this file. `h` is passed straight to
> `history()`, which dereferences it, so calling `unstifle_history()` before
> any other readline call — a plausible thing for an application to do at
> startup — is a NULL pointer dereference.
>
> Return value: the previous value of `max_input_history`. GNU readline
> returns the previous stifle amount, or a negative value if the history was
> not stifled; libedit always returns the raw previous mirror value, which
> is INT_MAX when it was not stifled and 0 if nothing has ever initialized
> it.
>
> `history_length`, `history_base` and `history_offset` are not touched, and
> entries already evicted by an earlier `stifle_history` are of course not
> restored.

> [spec:libedit:def:readline.username-completion-function-fn]
> char * username_completion_function(const char *text, int state)

> [spec:libedit:sem:readline.username-completion-function-fn]
> Generator that is supposed to return usernames beginning with `text`,
> where `text` may carry a leading `~`.
>
> Steps as written: if `text[0]` is NUL, returns NULL. If `*text` is `~`,
> advances past it. If `state == 0`, calls `setpwent()` to rewind the
> password database — so a fresh scan starts on the first call of a
> generator sequence, as the readline generator protocol requires. Then:
>
>     while ((pass = getpwent()) != NULL
>         && text[0] == pass->pw_name[0]
>         && strcmp(text, pass->pw_name) == 0)
>         continue;
>
> If `pass` is NULL, calls `endpwent()` and returns NULL. Otherwise returns
> `strdup(pass->pw_name)` — unchecked, so NULL is also returned on
> allocation failure.
>
> The loop condition is inverted: it *continues* while the entry is exactly
> equal to `text` and stops at the first entry that differs. Since the first
> password-database entry is essentially never string-equal to the partial
> username being completed, the loop exits after a single `getpwent()` and
> the function returns the next database entry regardless of whether it
> starts with `text`. No prefix matching happens at all, despite the source
> comment describing exactly that. Repeated calls with increasing `state`
> therefore walk the whole password file in order, which is what a caller
> building a match list will collect.
>
> Further consequences: `endpwent()` is only reached when the database is
> exhausted, so an abandoned scan leaks the open `passwd` handle; `getpwent`
> uses static storage and is not thread-safe; and starting a new scan
> without `state == 0` continues from wherever the previous one stopped.
>
> Ownership: the returned string is `strdup`'d and the caller must release
> it with `free()`. NULL means "no more entries" (indistinguishably from an
> allocation failure or an empty `text`).

> [spec:libedit:def:readline.using-history-fn]
> void using_history(void)

> [spec:libedit:sem:readline.using-history-fn]
> Prepares the history for expansion and navigation; readline applications
> call it once at startup.
>
> Steps: if `h` or `e` is NULL, `rl_initialize()` — note the condition is `h
> == NULL || e == NULL`, so a partially initialized library is reinitialized
> from scratch, which tears down and rebuilds both objects. Then
> `history_offset = history_length`.
>
> That single assignment is the whole point: it moves the readline history
> position to one past the newest entry, which is where `previous_history`
> expects to start and where `next_history` will refuse to advance from.
> Note `history_set_pos` cannot reach this value, since it rejects `pos ==
> history_length`.
>
> No other global is touched: `history_base`, `max_input_history` and
> libedit's internal cursor are all left as they were. Returns nothing.
>
> GNU readline's `using_history` also initializes the history library
> itself; here that job belongs to `rl_initialize`, which this function
> merely triggers.

> [spec:libedit:def:readline.where-history-fn]
> int where_history(void)

> [spec:libedit:sem:readline.where-history-fn]
> Returns the current readline history position.
>
> The entire body is `return history_offset;` — a direct read of the
> exported global, with no lazy initialization, no history call and no
> validation. Calling it before any initialization returns 0, the variable's
> static initializer.
>
> `history_offset` is maintained by `using_history` (which sets it to
> `history_length`), `add_history` (which increments it when the entry count
> grew), `clear_history` (which zeroes it), `history_set_pos`,
> `previous_history` and `next_history`. It is *not* updated by
> `read_history`, by `history_search`, by `history_search_prefix` or by
> `remove_history`, so after any of those the value returned here no longer
> describes the list.
>
> The position is a 0-based offset into the history where 0 is the oldest
> entry, in contrast to `history_get`'s indices, which are 1-based against
> `history_base`.

> [spec:libedit:def:readline.write-history-fn]
> int write_history(const char *filename)

> [spec:libedit:sem:readline.write-history-fn]
> Writes the entire history to a file, replacing its contents.
>
> Steps: lazily `rl_initialize()` if `h` or `e` is NULL. If `filename` is
> NULL, substitutes `_default_history_file()`; if that is also NULL, returns
> the current `errno`. Then `history(h, &ev, H_SAVE, filename)`.
>
> H_SAVE truncates or creates the file, writes libedit's history signature
> line, and then writes every event from oldest to newest, each
> `strvis`-escaped with VIS_WHITE and terminated by a newline. That escaping
> is the frozen on-disk format the port must round-trip with `read_history`.
> Existing file contents are discarded — use `append_history` to add without
> rewriting.
>
> Return value, computed in a single expression: if H_SAVE returned -1,
> returns `errno` when it is non-zero and `EINVAL` otherwise; else returns
> 0. So the convention is 0 on success and a positive errno value on
>    failure, never -1 and never the number of entries written (which H_SAVE
>    does return internally but is discarded here).
>
> No global is modified; `history_length` is not refreshed by this call.
