# src/eln.c

> [spec:libedit:def:eln.el-get-fn]
> int el_get(EditLine *el, int op, ...)

> [spec:libedit:sem:eln.el-get-fn]
> Narrow-character `el_get`. Dispatches on `op`; forwards to `el_wget`
> where the narrow and wide argument types are identical, encodes wide
> results into the shared legacy buffer `el->el_lgcyconv` where they are
> not, and for two ops bypasses `el_wget` entirely.
>
> 1. If `el == NULL`, return -1 without touching the varargs.
> 2. `va_start(ap, op)`, run the dispatch below, `va_end(ap)`, return
>    `ret`. `ret` is not initialised before the switch; every arm
>    assigns it, including the default.
>
> Per-op behaviour:
>
> - `EL_PROMPT`, `EL_RPROMPT` — read one `el_pfunc_t *p`; return
>   `prompt_get(el, p, 0, op)`, which stores the installed prompt
>   function through `p` and returns 0, or returns -1 when `p` is NULL.
>   Nothing is converted, and the caller is not told whether the
>   installed function was registered as narrow (via `el_set`) or wide
>   (via `el_wset`) — the two share one slot and one `el_pfunc_t` type.
> - `EL_PROMPT_ESC`, `EL_RPROMPT_ESC` — read `el_pfunc_t *p` and
>   `char *c`; call `prompt_get(el, p, &wc, op)` into a local
>   `wchar_t wc = 0`, then **unconditionally** store `*c = (char)wc`.
>   Three consequences a port must keep:
>   - the store happens even when `prompt_get` returned -1, in which
>     case `'\0'` is written, and `c` is never checked for NULL;
>   - the escape character is *truncated* to the low byte of the
>     `wchar_t`, not encoded — this is not the inverse of any multibyte
>     conversion, and for values above `CHAR_MAX` the stored value is
>     implementation-defined (plain `char` signedness). `el_wget` hands
>     back the full `wchar_t` and does not truncate;
>   - inherited quirk from `prompt_get`: it selects `el->el_prompt` only
>     when `op == EL_PROMPT`, so `EL_PROMPT_ESC` retrieves the **right**
>     prompt's function and ignore character, not the left prompt's.
>     `el_wget` inherits the same quirk, so this is not an eln-specific
>     divergence.
> - `EL_EDITOR`, `EL_WORDCHARS` — read `const char **p`; call
>   `el_wget(el, op, &pw)` into a local `const wchar_t *pw`, then
>   `*p = ct_encode_string(pw, &el->el_lgcyconv)`, then, if
>   `el->el_lgcyconv.csize == 0`, overwrite `ret` with -1.
>   `EL_EDITOR` yields `"emacs"` or `"vi"` (encoded from static wide
>   literals); `EL_WORDCHARS` yields the current word-character set.
>   Notes:
>   - the `csize` test is the *only* error detection, and it is a proxy:
>     `ct_conv_cbuff_resize` zeroes `csize` when its `realloc` fails, so
>     `csize == 0` means "the encode ran out of memory". If `pw` came
>     back NULL, `ct_encode_string` returns NULL without touching
>     `csize`, so `*p` is set to NULL while `ret` keeps whatever
>     `el_wget` returned;
>   - `pw` is uninitialised on entry and is stored through regardless of
>     `el_wget`'s return value; the two wide ops used here always assign
>     it, but nothing in `el_get` enforces that.
>   - **The `const char *` written to `*p` is `el->el_lgcyconv.cbuff`
>     itself, not a copy**, and is invalidated exactly as described for
>     `el_gets`.
> - `EL_TERMINAL` — `el_wget(el, op, va_arg(ap, const char **))`. The
>   terminal name is narrow on both sides, so nothing is converted; the
>   pointer written is into the terminal layer's own storage
>   (`el->el_terminal.t_name`), not into `el_lgcyconv`. Always returns
>   0.
> - `EL_SIGNAL`, `EL_EDITMODE`, `EL_SAFEREAD`, `EL_UNBUFFERED`,
>   `EL_PREP_TERM` — `el_wget(el, op, va_arg(ap, int *))`, pure
>   pass-through. **`EL_PREP_TERM` is listed here but has no case in
>   `el_wget`**, so it consumes the pointer, stores nothing, and always
>   returns -1. It is a set-only op that the narrow wrapper pretends to
>   forward.
> - `EL_GETTC` — does *not* delegate to `el_wget`. Builds a local
>   `char *argv[3]` whose elements are, in order: a function-static
>   `char gettc[] = "gettc"`, the capability name read as `char *`, and
>   the destination read as `void *`; then calls
>   `terminal_gettc(el, 3, argv)` and returns its value. This is
>   byte-for-byte what `el_wget` does — `terminal_gettc` takes
>   `char **` in both APIs, so capability names are narrow even in the
>   wide API and nothing is converted. Only one capability is fetched
>   per call despite the header's `..., NULL` signature; any further
>   varargs are never read.
> - `EL_GETCFN` — `el_wget(el, op, va_arg(ap, el_rfunc_t *))`. Not
>   converted; `el_rfunc_t` is `int (*)(EditLine *, wchar_t *)` in both
>   APIs, so even through the narrow API the retrieved read function is
>   the wide-character one.
> - `EL_CLIENTDATA` — `el_wget(el, op, va_arg(ap, void **))`.
> - `EL_GETFP` — read `int what` then `FILE **fpp`; call
>   `el_wget(el, op, what, fpp)`. `what` is 0/1/2 for in/out/err;
>   anything else yields -1 with `*fpp` untouched.
> - Anything else — `ret = -1`, varargs left unread.
>
> Divergences from `el_wget` worth restating:
>
> - **`EL_GETENV` is missing.** `el_wget` supports it (retrieving the
>   installed `char *(*)(const char *)` hook); the narrow `el_get` falls
>   to the default and returns -1, even though the hook is narrow in
>   both APIs. `el_set` omits it symmetrically.
> - The set-only ops `EL_RESIZE`, `EL_ALIAS_TEXT`, `EL_ADDFN`,
>   `EL_HIST`, `EL_BIND`, `EL_TELLTC`, `EL_SETTC`, `EL_ECHOTC`,
>   `EL_SETTY`, `EL_SETFP` and `EL_REFRESH` have no `el_get` arm and
>   return -1.
> - Only `EL_EDITOR` and `EL_WORDCHARS` touch `el_lgcyconv`, and they
>   touch its **narrow** half (`cbuff`). Every other op leaves the
>   legacy buffer alone, so a pointer previously returned by `el_gets`
>   or `el_line` survives them.

> [spec:libedit:def:eln.el-getc-fn]
> int el_getc(EditLine *el, char *cp)

> [spec:libedit:sem:eln.el-getc-fn]
> Narrow-character wrapper over `el_wgetc`: read one character and
> deliver it as a single byte.
>
> 1. Declare `wchar_t wc = 0` and call `num_read = el_wgetc(el, &wc)`.
>    `el` is not checked for NULL.
> 2. Unconditionally store `*cp = '\0'`, so the output byte is cleared
>    on every path including EOF and error. `cp` is not checked for
>    NULL.
> 3. If `num_read <= 0`, return `num_read` unchanged and ignore `wc`:
>    0 means no character was available (end of input, or the tty could
>    not be put into raw mode), negative means a read error, with
>    `errno` left exactly as the underlying read set it.
> 4. Otherwise convert with `num_read = wctob(wc)`:
>    - if `wctob` returns `EOF` — i.e. `wc` has no single-byte
>      representation in the initial shift state of the current
>      `LC_CTYPE` locale — set `errno = ERANGE` and return -1, leaving
>      `*cp` as `'\0'`;
>    - otherwise store `*cp = (char)num_read` and return **1**, not
>      `num_read` (`el_wgetc` only ever reports 1 on success, so the two
>      coincide).
>
> Consequences a re-implementation must preserve:
>
> - **This is not a multibyte interface.** It cannot return a
>   multi-byte character in pieces and it does not buffer one. In a
>   UTF-8 locale every character outside US-ASCII fails with
>   -1/`ERANGE`, and the character is *consumed and lost*: `el_wgetc`
>   has already popped it from the macro stack or read it from the
>   terminal, and `el_getc` provides no pushback.
> - Which characters survive is locale-dependent, and the sign of the
>   stored `char` for byte values above 127 is implementation-defined.
> - `errno` is assigned by this function only on the `ERANGE` path.
> - `*cp` is written before any early return, so a caller that
>   pre-loaded `*cp` will find it clobbered even when nothing was read.

> [spec:libedit:def:eln.el-gets-fn]
> const char * el_gets(EditLine *el, int *nread)

> [spec:libedit:sem:eln.el-gets-fn]
> Narrow-character wrapper over `el_wgets`: read a line, encode it into
> the shared legacy buffer, and restate `*nread` from characters to
> bytes.
>
> 1. `tmp = el_wgets(el, nread)`. `el` is not checked for NULL. The wide
>    call sets `*nread` to a count of **wide characters** and returns the
>    editor's internal wide line buffer, or returns NULL having set
>    `*nread` to 0 (nothing read) or -1 (read failure, with `errno`
>    restored to the original failure reason).
> 2. If `tmp != NULL`, rewrite `*nread` into a **byte** count: sum
>    `ct_enc_width(tmp[i])` over `i` in `[0, *nread)` into a `size_t`,
>    then store that total back through `nread`, cast to `int`.
>    `ct_enc_width(c)` is the number of bytes `wcrtomb` produces for `c`
>    starting from a zeroed `mbstate_t`, and 0 for a character the
>    locale cannot encode — so unencodable characters contribute
>    nothing, consistently with step 3 dropping them.
> 3. Return `ct_encode_string(tmp, &el->el_lgcyconv)`. That encodes the
>    wide string character by character with `wctomb` into
>    `el->el_lgcyconv.cbuff` (grown in 1024-byte increments as needed),
>    NUL-terminates it, and returns the *start of that buffer*. When
>    `tmp` is NULL the encoder returns NULL immediately, so `el_gets`
>    returns NULL with `*nread` exactly as `el_wgets` left it.
>
> Hazards and divergences from `el_wgets` that a port must reproduce:
>
> - **`nread` may not be NULL here, although it may be for `el_wgets`.**
>   `el_wgets` substitutes a local `int` when handed NULL; `el_gets`
>   dereferences `*nread` in step 2 whenever a line was returned. So
>   `el_gets(el, NULL)` faults the moment a line is read, while
>   `el_wgets(el, NULL)` is well-defined.
> - **The returned pointer is `el->el_lgcyconv.cbuff` itself, not a
>   copy, and there is exactly one such buffer per `EditLine`.** It
>   stays valid only until the next call that writes the *narrow* half
>   of `el_lgcyconv`: another `el_gets`, an `el_line`, or an
>   `el_get(EL_EDITOR)` / `el_get(EL_WORDCHARS)`. Any of those
>   overwrites the contents and may `realloc` the buffer to a different
>   address; `el_end` frees it. Callers that need the line to outlive
>   the next such call must copy it. Calls that only *decode* —
>   `el_push`, `el_parse`, `el_insertstr`, `el_replacestr`,
>   `el_set(EL_EDITOR / EL_WORDCHARS / EL_BIND / EL_TELLTC / EL_SETTC /
>   EL_ECHOTC / EL_SETTY / EL_ADDFN)` — write only the wide half
>   (`wbuff`) and leave a previously returned string intact.
> - **`*nread` and the returned string can disagree.** Step 2 measures
>   exactly `*nread` wide characters; step 3 encodes the whole wide
>   string up to its terminating `L'\0'`. They agree only when the wide
>   line buffer is NUL-terminated at offset `*nread`. The buffered read
>   path guarantees that (a line ends via `ed_newline` or
>   `ed_end_of_file`, both of which store the terminator), but the
>   `EL_UNBUFFERED` path does not — there `*nread` is just
>   `lastchar - buffer` and no terminator is written, so the returned
>   byte string can run past `*nread` bytes into characters left over
>   from an earlier, longer line.
> - The byte count excludes the NUL terminator that `ct_encode_string`
>   appends.
> - The two directions use different primitives: `ct_enc_width` calls
>   `wcrtomb` from a zeroed `mbstate_t`, while `ct_encode_string` calls
>   `wctomb`, which carries the process-global shift state. They agree
>   for stateless encodings — every POSIX locale of interest — but not
>   by construction, and a stateful locale can make the count and the
>   string diverge.
> - `ct_encode_string` calls `abort()` if any single character requires
>   more than 5 bytes to encode, and returns NULL on allocation failure.
>   In the latter case `el_gets` returns NULL *after* having already
>   rewritten `*nread` to a byte count, so a NULL return does not imply
>   `*nread <= 0`.

> [spec:libedit:def:eln.el-insertstr-fn]
> int el_insertstr(EditLine *el, const char *str)

> [spec:libedit:sem:eln.el-insertstr-fn]
> Narrow-character wrapper over `el_winsertstr`. The whole body is
> `return el_winsertstr(el, ct_decode_string(str, &el->el_lgcyconv))`.
>
> 1. Decode `str` from the current locale's multibyte encoding into wide
>    characters using the shared legacy buffer `el->el_lgcyconv`: this
>    sizes and writes `el_lgcyconv.wbuff` (via `mbstowcs`) and returns a
>    pointer into it. `ct_decode_string` returns NULL when `str` is
>    NULL, when `str` holds a byte sequence invalid in the current
>    locale, or when the buffer could not be grown.
> 2. Hand the result — possibly NULL — to `el_winsertstr`, which
>    rejects both NULL and the empty string with -1, and otherwise
>    inserts the wide string at `el->el_line.cursor`, enlarging the line
>    buffer if needed, advancing the cursor past the inserted text, and
>    returning 0. It returns -1 if the buffer could not be enlarged.
> 3. Return that value unchanged.
>
> Notes:
>
> - A NULL, empty, or invalidly-encoded `str` is reported as -1 and is
>   indistinguishable from an insert that failed for lack of buffer
>   space. There is no `errno` contract.
> - `el_winsertstr` copies the characters into the line buffer, so
>   nothing retains a pointer into `el_lgcyconv.wbuff`.
> - This call overwrites, and may `realloc`, `el_lgcyconv.wbuff`. It
>   does not touch `el_lgcyconv.cbuff`, so a `const char *` previously
>   returned by `el_gets`, `el_line` or `el_get` remains valid across
>   it.
> - `el` is not checked for NULL.

> [spec:libedit:def:eln.el-line-fn]
> const LineInfo * el_line(EditLine *el)

> [spec:libedit:sem:eln.el-line-fn]
> Narrow-character counterpart of `el_wline`: fill in and return a
> `LineInfo` whose three `const char *` fields describe the current edit
> line in bytes. Unlike `el_wline`, which merely casts `&el->el_line`,
> this performs a conversion, carries a re-entrancy guard, and invokes
> the client's resize callback.
>
> 1. `winfo = el_wline(el)` — a pointer to the live wide line state
>    (`buffer`, `cursor`, `lastchar`). `info = &el->el_lgcylinfo`, the
>    single `LineInfo` embedded in the `EditLine`; `el_line` always
>    returns that same address. `el` is not checked for NULL, so
>    `el_line(NULL)` is undefined.
> 2. Re-entrancy guard: if `el->el_flags & FROM_ELLINE` is already set,
>    return `info` immediately, with whatever contents it already has,
>    performing no conversion and not calling the resize callback.
>    `FROM_ELLINE` is set nowhere else in the library, so this fires
>    exactly when `el_line` is called from inside the resize callback
>    that step 5 invokes; without it that would recurse forever.
> 3. Set `FROM_ELLINE`, then
>    `info->buffer = ct_encode_string(winfo->buffer, &el->el_lgcyconv)`
>    — encode the wide line into `el->el_lgcyconv.cbuff` and point
>    `info->buffer` at the start of that buffer. The encode runs to the
>    wide string's terminating `L'\0'`, *not* to `winfo->lastchar`.
> 4. Derive the two other fields as **byte** offsets, recomputed rather
>    than measured:
>    - `offset = sum of ct_enc_width(*p)` for `p` over
>      `[winfo->buffer, winfo->cursor)`; `info->cursor = info->buffer +
>      offset`;
>    - independently, `offset = sum of ct_enc_width(*p)` for `p` over
>      `[winfo->buffer, winfo->lastchar)`; `info->lastchar =
>      info->buffer + offset`.
> 5. If `el->el_chared.c_resizefun` is non-NULL — installed by
>    `el_set(EL_RESIZE, fn, arg)` — call
>    `(*c_resizefun)(el, el->el_chared.c_resizearg)`. This runs *after*
>    `info` is fully populated, so the callback sees current data, and a
>    nested `el_line` from inside it takes the step-2 shortcut and
>    receives exactly this `info`.
> 6. Clear `FROM_ELLINE` and return `info`.
>
> Divergences from `el_wline`, and hazards:
>
> - **`el_wline` has no side effects; `el_line` does.** Every
>   non-nested `el_line` runs the client's `EL_RESIZE` callback. That is
>   observable and must be kept: `ch_enlargebufs` is the only other
>   caller of that hook, and clients use the pair to learn that line
>   pointers may have moved.
> - **`info->buffer` is `el->el_lgcyconv.cbuff`, not a copy.** The next
>   `el_line`, `el_gets`, `el_get(EL_EDITOR)` or
>   `el_get(EL_WORDCHARS)` overwrites the bytes and may `realloc` the
>   buffer to a new address, invalidating `info->buffer`,
>   `info->cursor` and `info->lastchar` together. `el_end` frees it. The
>   `LineInfo` struct itself is stable — it lives inside the
>   `EditLine` — but is shared: there is one per `EditLine` and every
>   caller receives the same one, so two live `LineInfo` views of the
>   same editor are impossible.
> - **`info->lastchar` is not the end of the encoded string.** It is the
>   byte width of the wide prefix `[buffer, lastchar)`. When the wide
>   buffer is not NUL-terminated at `lastchar` — which the
>   `EL_UNBUFFERED` read path does not guarantee — step 3 encodes past
>   it and `info->lastchar` lands in the middle of a longer, still
>   NUL-terminated byte string.
> - Characters the locale cannot encode contribute 0 bytes both to the
>   encoded string and to the offsets, so the offsets remain consistent
>   with the string; but the two use different primitives (`wctomb`
>   with the global shift state in the encoder, `wcrtomb` from a zeroed
>   state in `ct_enc_width`), which coincide only for stateless
>   encodings.
> - If `ct_encode_string` fails — allocation failure — it returns NULL,
>   `info->buffer` becomes NULL, and step 4 evaluates `NULL + offset`,
>   which is undefined behaviour. There is no error return; a caller can
>   only see a `LineInfo` with a NULL `buffer`. A port should treat the
>   allocation-failure case as unspecified rather than reproducing the
>   pointer arithmetic.
> - `ct_encode_string` calls `abort()` if any single character needs
>   more than 5 bytes.

> [spec:libedit:def:eln.el-parse-fn]
> int el_parse(EditLine *el, int argc, const char *argv[])

> [spec:libedit:sem:eln.el-parse-fn]
> Narrow-character wrapper over `el_wparse`: decode the whole argument
> vector, delegate, free the vector.
>
> 1. `wargv = ct_decode_argv(argc, argv, &el->el_lgcyconv)`, with the
>    result laundered through `void *` only to shed constness. That
>    decodes all `argc` strings in one pass into the shared legacy wide
>    buffer `el_lgcyconv.wbuff` (sized to the sum of
>    `strlen(argv[i]) + 1` plus 1024 slack), and returns a freshly
>    `calloc`ed, NULL-terminated array of `argc + 1` pointers whose
>    elements point at consecutive strings *inside that one buffer*. A
>    NULL element of `argv` is passed through as a NULL element of
>    `wargv` rather than being handed to `mbstowcs`.
> 2. If `wargv` is NULL — allocation failure, or any `argv[i]`
>    containing a byte sequence invalid in the current locale — return
>    -1 immediately, without calling `el_wparse`.
> 3. `ret = el_wparse(el, argc, wargv)` — the wide parser: it
>    interprets `wargv[0]` as an optional `prog:command` selector,
>    matches `command` against the builtin command table, and returns
>    the negated command result, or -1 for `argc < 1` and for an
>    unrecognised command. `el_parse` does not interpret any of that.
> 4. `el_free(wargv)` — this frees only the pointer array. The decoded
>    wide strings live in `el_lgcyconv.wbuff` and remain there,
>    untouched, until the next use of that buffer.
> 5. Return `ret` unchanged.
>
> Notes:
>
> - `el` is not checked for NULL, and neither is `argv`; with
>   `argc > 0` a NULL `argv` faults inside `ct_decode_argv`.
> - Unlike `el_set`'s list ops, `el_parse` imposes no 20-entry cap: it
>   decodes exactly `argc` arguments.
> - Only the wide half of `el_lgcyconv` is used, so a `const char *`
>   previously returned by `el_gets`, `el_line` or `el_get` survives an
>   `el_parse`.

> [spec:libedit:def:eln.el-push-fn]
> void el_push(EditLine *el, const char *str)

> [spec:libedit:sem:eln.el-push-fn]
> Narrow-character wrapper over `el_wpush`. The whole body is
> `el_wpush(el, ct_decode_string(str, &el->el_lgcyconv))`.
>
> 1. Decode `str` from the current locale's multibyte encoding into wide
>    characters in the shared legacy buffer `el->el_lgcyconv`: size and
>    write `el_lgcyconv.wbuff` via `mbstowcs` and return a pointer into
>    it. Multibyte-to-wide decoding is used unconditionally; it behaves
>    correctly under single-byte locales too, so there is no separate
>    narrow path.
> 2. `ct_decode_string` returns NULL when `str` is NULL, when `str`
>    holds a byte sequence invalid in the current locale, or when the
>    buffer could not be grown. That NULL is passed straight through to
>    `el_wpush`, which treats a NULL string as failure: it beeps the
>    terminal, flushes, and pushes nothing. `el_push(el, NULL)` is
>    therefore well-defined — it beeps.
> 3. `el_wpush` pushes a `wcsdup` copy onto the macro stack, so no
>    pointer into `el_lgcyconv.wbuff` is retained. It also fails (beeps)
>    when the macro stack is full or the `wcsdup` fails.
>
> Notes:
>
> - Returns nothing. The caller has no way to learn whether the push
>   succeeded other than by hearing the beep — the same as `el_wpush`.
> - `el` is not checked for NULL.
> - This call writes, and may `realloc`, `el_lgcyconv.wbuff`; it does
>   not touch `el_lgcyconv.cbuff`, so a `const char *` previously
>   returned by `el_gets`, `el_line` or `el_get` survives an `el_push`.

> [spec:libedit:def:eln.el-replacestr-fn]
> int el_replacestr(EditLine *el, const char *str)

> [spec:libedit:sem:eln.el-replacestr-fn]
> Narrow-character wrapper over `el_wreplacestr`. The whole body is
> `return el_wreplacestr(el, ct_decode_string(str, &el->el_lgcyconv))`.
>
> 1. Decode `str` from the current locale's multibyte encoding into wide
>    characters in the shared legacy buffer `el->el_lgcyconv`, as
>    `el_insertstr` does. `ct_decode_string` returns NULL for a NULL
>    `str`, for input invalid in the current locale, or on allocation
>    failure.
> 2. Hand the result — possibly NULL — to `el_wreplacestr`, which
>    rejects NULL and the empty string with -1, and otherwise replaces
>    the *entire* line: it copies the wide string over
>    `el->el_line.buffer` from offset 0 (enlarging the buffer first if
>    needed, returning -1 if that fails), writes a terminating `L'\0'`,
>    sets `lastchar` to `buffer + len`, clamps `cursor` down to
>    `lastchar` if it now points past the end, and returns 0.
> 3. Return that value unchanged.
>
> Notes:
>
> - This is a whole-line replacement, not a replacement at the cursor;
>   that is the only difference from `el_insertstr` at this layer.
> - A NULL, empty, or invalidly-encoded `str` is reported as -1 and is
>   indistinguishable from a buffer-growth failure.
> - The characters are copied into the line buffer, so nothing retains a
>   pointer into `el_lgcyconv.wbuff`.
> - This call writes, and may `realloc`, `el_lgcyconv.wbuff`; it does
>   not touch `el_lgcyconv.cbuff`, so a `const char *` previously
>   returned by `el_gets`, `el_line` or `el_get` survives it.
> - `el` is not checked for NULL.

> [spec:libedit:def:eln.el-set-fn]
> int el_set(EditLine *el, int op, ...)

> [spec:libedit:sem:eln.el-set-fn]
> Narrow-character `el_set`. Dispatches on `op` and, for the ops whose
> arguments are strings, decodes them from the locale's multibyte
> encoding into wide characters (always via the shared legacy buffer
> `el->el_lgcyconv`) before doing the work. Ops whose argument types are
> identical in both APIs are forwarded to `el_wset` with a fresh varargs
> call; the list ops and `EL_ADDFN` cannot be forwarded, because a
> converted `wchar_t **` cannot portably be re-packed into a `va_list`,
> so they re-implement `el_wset`'s body by calling the internal
> functions directly.
>
> 1. If `el == NULL`, return -1 without touching the varargs.
> 2. `va_start(ap, op)`, run the dispatch below, then at label `out`
>    `va_end(ap)` and return `ret`. `ret` is not initialised before the
>    switch; every arm assigns it, and the two `goto out` error exits
>    assign it first, so `va_end` is reached on every path.
>
> Per-op behaviour:
>
> - `EL_PROMPT`, `EL_RPROMPT` — read one `el_pfunc_t p`; call
>   `prompt_set(el, p, 0, op, 0)`. The trailing `0` is the `wide` flag
>   and is the *only* difference from `el_wset`, which passes 1: it
>   records that the callback returns a `char *`, so `prompt_print`
>   will later call the function and decode its result with
>   `ct_decode_string` into `el->el_scratch` (a different `ct_buffer_t`
>   from `el_lgcyconv`, so prompt rendering never disturbs a string
>   handed out by `el_gets`). `prompt_set` also resets the ignore
>   character to 0 and the recorded prompt position to (0,0), installs
>   the default prompt function when `p` is NULL, and returns 0.
> - `EL_PROMPT_ESC`, `EL_RPROMPT_ESC` — read `el_pfunc_t p` then
>   `int c`; call `prompt_set(el, p, c, op, 0)`. `c` reaches the
>   `wchar_t` parameter by ordinary argument conversion, i.e. it is
>   taken as a character *code*, not decoded from a byte, so this is not
>   a narrow interface in any meaningful sense — it differs from
>   `el_wset` only in the `wide` flag.
> - `EL_RESIZE` — read `el_zfunc_t p` then `void *arg`; call
>   `ch_resizefun(el, p, arg)`, which stores both into
>   `el->el_chared.c_resizefun` / `c_resizearg` and returns 0. Identical
>   to `el_wset`; no conversion, as the callback takes no strings. Note
>   that installing this hook makes every subsequent `el_line` call it.
> - `EL_ALIAS_TEXT` — read `el_afunc_t p` then `void *arg`; call
>   `ch_aliasfun(el, p, arg)`, returns 0. Identical to `el_wset`;
>   `el_afunc_t` is `const char *(*)(void *, const char *)` — narrow in
>   *both* APIs — so nothing is converted here even on the wide side.
> - `EL_TERMINAL` — `el_wset(el, op, va_arg(ap, char *))`. Terminal
>   names are narrow in both APIs, so this is a straight pass-through
>   (the `const` is dropped by the `va_arg` type). A NULL argument is
>   meaningful and means "consult the environment".
> - `EL_EDITOR`, `EL_WORDCHARS` — `el_wset(el, op,
>   ct_decode_string(va_arg(ap, char *), &el->el_lgcyconv))`. The decode
>   result is **not checked for NULL**, and neither is it checked on the
>   wide side: `map_set_editor` immediately calls `wcscmp` on it and
>   `map_set_wordchars` immediately calls `wcsdup` on it. So
>   `el_set(el, EL_EDITOR, NULL)`, or a string that is invalid in the
>   current locale, dereferences NULL — undefined behaviour that a port
>   should reject rather than reproduce. On success `EL_EDITOR` accepts
>   only `"emacs"` or `"vi"` (returning -1 otherwise) and
>   `EL_WORDCHARS` `wcsdup`s the string, so `el_lgcyconv.wbuff` is not
>   retained.
> - `EL_SIGNAL`, `EL_EDITMODE`, `EL_SAFEREAD`, `EL_UNBUFFERED`,
>   `EL_PREP_TERM` — `el_wset(el, op, va_arg(ap, int))`; pure
>   pass-through of a single `int`.
> - `EL_BIND`, `EL_TELLTC`, `EL_SETTC`, `EL_ECHOTC`, `EL_SETTY` —
>   a NULL-terminated variadic list of `const char *`:
>   1. Declare a local `const char *argv[20]`. For `i` from 1 while
>      `i < 19`, read the next `const char *` into `argv[i]` and break
>      as soon as one is NULL. So at most 18 caller-supplied arguments
>      (indices 1..18) are consumed; a 19th and beyond are never read
>      from the varargs.
>   2. `argv[0] = argv[i] = NULL`. After this, `i` is the index of the
>      terminator — the caller's NULL, or 19 when the caller supplied 18
>      non-NULL arguments and the loop ran out — and is also the number
>      of live entries `argv[0..i-1]`.
>   3. `wargv = ct_decode_argv(i + 1, argv, &el->el_lgcyconv)` decodes
>      entries `0..i` into `el_lgcyconv.wbuff`, mapping NULL to NULL,
>      and returns a `calloc`ed array of `i + 2` pointers with NULLs at
>      indices `i` and `i + 1`. If it returns NULL (allocation failure,
>      or an argument invalid in the current locale) set `ret = -1` and
>      `goto out`.
>   4. Overwrite `wargv[0]` with the wide command word and call the
>      internal implementation with argc `i`:
>      `EL_BIND` → `wargv[0] = L"bind"`, `map_bind(el, i, wargv)`;
>      `EL_TELLTC` → `L"telltc"`, `terminal_telltc(el, i, wargv)`;
>      `EL_SETTC` → `L"settc"`, `terminal_settc(el, i, wargv)`;
>      `EL_ECHOTC` → `L"echotc"`, `terminal_echotc(el, i, wargv)`;
>      `EL_SETTY` → `L"setty"`, `tty_stty(el, i, wargv)`.
>      The inner `default:` arm setting `ret = -1` is unreachable.
>   5. `el_free(wargv)` — the pointer array only; the decoded strings
>      stay in `el_lgcyconv.wbuff`. `ret` is the internal function's
>      return value.
>   Divergence from `el_wset`: the wide version loops `i < 20`, so it
>   accepts 19 arguments, and it never stores a terminating NULL — with
>   19 non-NULL arguments its `argv` reaches the callee unterminated and
>   with `argc == 20`. The narrow version caps at 18 and always writes
>   the terminator, so it is the safer of the two; the argument-count
>   limit differs by one between the two APIs.
> - `EL_ADDFN` — read `const char *name`, `const char *help`, then
>   `el_func_t func`. Decode the two strings together with
>   `ct_decode_argv(2, args, &el->el_lgcyconv)`; on NULL set `ret = -1`
>   and `goto out`. Otherwise call
>   `map_addfunc(el, wargv[0], wargv[1], func)` and `el_free(wargv)`.
>   `map_addfunc` `wcsdup`s both strings, so nothing retains a pointer
>   into `el_lgcyconv.wbuff`, and it returns -1 if either string or
>   `func` is NULL. `el_func_t` is `el_action_t (*)(EditLine *, wint_t)`
>   in both APIs and is passed through unconverted — the source carries
>   an `XXX` questioning exactly this, so a user editor function
>   installed through the *narrow* API is still invoked with a wide
>   character.
> - `EL_HIST` — read `hist_fun_t fun` then `void *ptr`; call
>   `hist_set(el, fun, ptr)` **directly**, not via `el_wset` (it stores
>   both into `el->el_history` and always returns 0), then
>   unconditionally `el->el_flags |= NARROW_HISTORY`. That flag routes
>   every internal history access through `hist_convert`, which calls
>   the client's history function and reinterprets the resulting
>   `HistEventW.str` as a `char *`, decoding it into `el->el_scratch`.
>   **Divergence:** `el_wset`'s `EL_HIST` never sets the flag and
>   *clears* it when `MB_CUR_MAX == 1`; the narrow version sets it in
>   every locale, single-byte ones included, and never clears it. The
>   flag is the sole mechanism by which the library knows a narrow
>   history implementation is installed, and this is its only set site.
> - `EL_GETCFN` — `el_wset(el, op, va_arg(ap, el_rfunc_t))`.
>   Unconverted, and `el_rfunc_t` is `int (*)(EditLine *, wchar_t *)` in
>   both APIs: a read function installed through the narrow `el_set` is
>   still called with a `wchar_t *` to fill in. `EL_BUILTIN_GETCFN`
>   (`NULL`) restores the default reader.
> - `EL_CLIENTDATA` — `el_wset(el, op, va_arg(ap, void *))`.
> - `EL_SETFP` — read `int what` then `FILE *fp`; call
>   `el_wset(el, op, what, fp)`. `what` is 0/1/2 for in/out/err;
>   anything else yields -1.
> - `EL_REFRESH` — takes no argument and does **not** delegate: it
>   inlines `re_clear_display(el); re_refresh(el); terminal__flush(el);`
>   and sets `ret = 0`. Behaviourally identical to `el_wset`'s arm.
> - Anything else — `ret = -1`, varargs left unread.
>
> Divergences and cross-cutting notes:
>
> - **`EL_GETENV` is missing.** `el_wset` supports it (installing a
>   `char *(*)(const char *)` used for `TERM`, `HOME`, `EDITRC` and so
>   on); the narrow `el_set` falls to the default and returns -1, even
>   though the hook is narrow in both APIs. `el_get` omits it
>   symmetrically, so through the narrow API the hook can be neither set
>   nor read.
> - Every conversion in this function uses the **wide** half of
>   `el_lgcyconv` (`wbuff`), so `el_set` never invalidates a
>   `const char *` previously handed out by `el_gets`, `el_line` or
>   `el_get`. It does invalidate any `wchar_t *` obtained from an
>   earlier decode against the same buffer — including, across the two
>   `goto out` paths, nothing observable, since every consumer here
>   copies before returning.
> - A mistyped or unsupported `op` is silently -1; its varargs are never
>   consumed, which is harmless for `va_end` but means no diagnostic.

