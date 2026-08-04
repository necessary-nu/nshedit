# src/prompt.c, src/prompt.h

> [spec:libedit:def:prompt.el-pfunc-t-edit-line]
> typedef wchar_t *(*el_pfunc_t)(EditLine *)

> [spec:libedit:def:prompt.el-prompt-t]
> typedef struct el_prompt_t

> [spec:libedit:def:prompt.prompt-default-fn]
> static wchar_t * /*ARGSUSED*/ prompt_default(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:prompt.prompt-default-fn]
> The built-in left-hand prompt callback. It is installed by
> `prompt_init` and reinstalled by `prompt_set` whenever a caller
> passes a NULL function for `EL_PROMPT`/`EL_PROMPT_ESC`. It ignores
> its `EditLine *` argument completely.
>
> It returns a pointer to a function-local `static wchar_t a[3]`
> initialised to `L"? "` — question mark, space, terminating NUL. The
> storage is static and mutable, has program lifetime, and is shared by
> every `EditLine` in the process; the returned pointer is therefore
> valid forever, is never freed by libedit, and is never written to by
> it. That is the ownership contract every prompt callback lives under:
> the callee owns the string, libedit only reads it, and it must stay
> valid for the duration of the `prompt_print` call that asked for it.
>
> Caveat, and it is user-visible. `prompt_init` never assigns `p_wide`
> (see its rule), so an application that never sets a prompt of its own
> reaches this function through `prompt_print`'s *narrow* branch, which
> reinterprets the returned `wchar_t *` as a multibyte `char *`. With a
> 4-byte little-endian `wchar_t` the byte image of `L"? "` is
> `3F 00 00 00 20 00 00 00 00 ...`, so the decode yields the
> one-character string `"?"`: the default prompt observably renders as
> a bare `?` occupying one column, not `? ` occupying two. On a
> big-endian platform the leading byte is 0 and the default prompt is
> empty. Making the defaults wide would repair this and change what
> users see, so it is a behaviour change to be decided deliberately,
> not corrected in passing.

> [spec:libedit:def:prompt.prompt-default-r-fn]
> static wchar_t * /*ARGSUSED*/ prompt_default_r(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:prompt.prompt-default-r-fn]
> The built-in right-hand prompt callback. Installed by `prompt_init`
> and reinstalled by `prompt_set` whenever a caller passes a NULL
> function for `EL_RPROMPT`/`EL_RPROMPT_ESC`. Ignores its `EditLine *`
> argument completely.
>
> It returns a pointer to a function-local `static wchar_t a[1]`
> initialised to `L""` — a single NUL. Lifetime and ownership are
> exactly as for `prompt_default`: static, mutable, program lifetime,
> shared by every `EditLine`, never freed and never written to by
> libedit.
>
> The empty string is load-bearing rather than incidental.
> `prompt_print` walks it, emits nothing, leaves
> `el->el_refresh.r_cursor` untouched, and copies that into `p_pos`.
> Because `re_refresh` measures the rprompt with the drawing cursor at
> row 0 column 0, `el_rprompt.p_pos.h` comes back 0, and both
> `re_refresh` and `re_fastaddc` read `p_pos.h == 0` as "no right-hand
> prompt in use". So the default is not "an rprompt that happens to be
> blank", it is the rprompt feature switched off.
>
> Unlike `prompt_default`, the narrow/wide punning described under
> `prompt.prompt-init-fn` is harmless here: a wide NUL is all-zero
> bytes on either endianness, so reading `L""` as a `char *` also
> yields the empty string.

> [spec:libedit:def:prompt.prompt-end-fn]
> libedit_private void /*ARGSUSED*/ prompt_end(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:prompt.prompt-end-fn]
> Module teardown for the prompt module, called from `el_end` between
> `hist_end` and `sig_end`. The body is empty: it ignores its argument,
> releases nothing, resets nothing, and returns void.
>
> It is a no-op because the module owns no heap. `p_func` points at
> caller code or at one of the two static defaults; the prompt string
> belongs to the callback (see `prompt.prompt-default-fn`); the literal
> byte strings that `prompt_print` registers belong to the literal
> table and are freed by `literal_clear`/`literal_end`; `p_pos`,
> `p_ignore` and `p_wide` are plain values embedded in the `EditLine`.
>
> Note what it does *not* do: it does not restore the default
> callbacks and does not clear `p_ignore`, so both `el_prompt_t`
> records still hold the application's function pointer when `el_end`
> frees the `EditLine` around them. Nothing may use the handle
> afterwards, so this is unobservable.
>
> The port keeps this only if it keeps the module init/end protocol.
> There is no behaviour to reproduce.

> [spec:libedit:def:prompt.prompt-get-fn]
> libedit_private int prompt_get(EditLine *el, el_pfunc_t *prf, wchar_t *c, int op)

> [spec:libedit:sem:prompt.prompt-get-fn]
> Reads back the installed prompt callback and its literal escape
> character. It never invokes the callback, never touches the display,
> and never allocates.
>
> 1. If `prf` is NULL, return -1 immediately, having written nothing.
>    This is the only failure path.
> 2. Select the record: `&el->el_prompt` if `op` is exactly `EL_PROMPT`
>    (0), otherwise `&el->el_rprompt`. Every other value of `op` —
>    `EL_RPROMPT` (12), `EL_PROMPT_ESC` (21), `EL_RPROMPT_ESC` (22) and
>    anything unrecognised — selects the right-hand prompt. `op` is
>    never rejected.
> 3. Store `p->p_func` through `prf`. (The C re-tests `prf` against
>    NULL here; step 1 already guaranteed it is non-NULL, so the test
>    is dead.)
> 4. If `c` is non-NULL, store `p->p_ignore` through it. A NULL `c` is
>    legal and simply skips this store.
> 5. Return 0.
>
> BUG: step 2 is missing the `EL_PROMPT_ESC` arm that `prompt_set`
> has. `el_wget(el, EL_PROMPT_ESC, &f, &wc)` and
> `el_get(el, EL_PROMPT_ESC, &f, &c)` therefore return the *right-hand*
> prompt's callback and escape character, not the left-hand one's — the
> op most likely to be used for reading an escape character back is the
> one that reads the wrong record. Combined with the fact that both
> `el_get` and `el_wget` pass `c == NULL` for plain `EL_PROMPT`, there
> is no route through the public API to retrieve
> `el->el_prompt.p_ignore` at all. This is frozen ABI behaviour; the
> port reproduces it rather than fixing it.
>
> What comes back in `*prf` is the raw stored pointer, including
> `prompt_default`/`prompt_default_r` when the application never
> installed one — that is, a pointer to libedit-internal code the
> caller has no declaration for. `p_wide` is not reported, so the
> caller cannot tell whether the returned pointer is really a
> `char *(*)(EditLine *)` (installed through `el_set`) or a
> `wchar_t *(*)(EditLine *)` (installed through `el_wset`). The narrow
> `el_get` wrapper additionally truncates the `wchar_t` escape
> character to `char` on the way out.

> [spec:libedit:def:prompt.prompt-init-fn]
> libedit_private int prompt_init(EditLine *el)

> [spec:libedit:sem:prompt.prompt-init-fn]
> Module init for the prompt module, called from `el_init_internal`
> after `hist_init` and before `sig_init`. It puts both prompt records
> into their default state and returns 0; it cannot fail and its return
> value is discarded by the caller.
>
> 1. `el->el_prompt.p_func = prompt_default`, `el->el_prompt.p_pos.v =
>    0`, `el->el_prompt.p_pos.h = 0`, `el->el_prompt.p_ignore = L'\0'`.
> 2. `el->el_rprompt.p_func = prompt_default_r`,
>    `el->el_rprompt.p_pos.v = 0`, `el->el_rprompt.p_pos.h = 0`,
>    `el->el_rprompt.p_ignore = L'\0'`.
> 3. Return 0.
>
> A zero `p_ignore` disables the literal-escape mechanism outright:
> `prompt_print` only ever compares it against characters of a
> NUL-terminated string, so it can never match. A zero `p_pos` for the
> rprompt is also the "rprompt not in use" flag that `re_refresh` and
> `re_fastaddc` test.
>
> GAP: neither record's `p_wide` is assigned. It is 0 solely because
> `el_init_internal` allocates the `EditLine` with `el_calloc`, and 0
> means "narrow" — while both functions installed in steps 1 and 2
> return `wchar_t *`. So until the application calls `el_set`/`el_wset`
> with an `EL_PROMPT*` op, `prompt_print` calls a wide function and
> then decodes its result as a multibyte `char *`: the type-punning UB
> described in `prompt.prompt-print-fn`, with the observable
> consequence spelled out in `prompt.prompt-default-fn` (the `? `
> default renders as `?`). A port that drops the zeroing allocation, or
> that represents the callback as a tagged narrow/wide value with a
> different default, changes this.

> [spec:libedit:def:prompt.prompt-print-fn]
> libedit_private void prompt_print(EditLine *el, int op)

> [spec:libedit:sem:prompt.prompt-print-fn]
> Renders one of the two prompts into the virtual display starting at
> the current drawing cursor, and records where that cursor ended up.
> It writes nothing to the terminal — the bytes leave later, when
> `re_update_line` flushes `el_vdisplay`. No return value, no error
> path.
>
> 1. Select the record: `elp = &el->el_prompt` if `op == EL_PROMPT`
>    (0), otherwise `elp = &el->el_rprompt`. Unlike `prompt_set`, the
>    `_ESC` ops are not recognised here; the only values any caller
>    passes are `EL_PROMPT` and `EL_RPROMPT`.
> 2. Obtain the prompt text by calling `(*elp->p_func)(el)` — exactly
>    once per `prompt_print` call, with no caching between calls and no
>    memoisation of the previous result.
>    - If `elp->p_wide` is non-zero, the returned pointer is used
>      directly as the `wchar_t *` to render.
>    - If `elp->p_wide` is zero, the returned pointer is treated as a
>      multibyte `char *` and decoded with
>      `ct_decode_string(..., &el->el_scratch)` — `mbstowcs` in the
>      current locale into the `EditLine`'s shared scratch conversion
>      buffer, grown as needed. What gets rendered is then that scratch
>      buffer, not the callback's own memory.
>    The C makes the same indirect call through `el_pfunc_t` in both
>    branches and casts the result in the narrow one, so a narrow
>    callback is invoked through an incompatible prototype: UB in C,
>    which happens to work because every ABI in scope returns both
>    pointer types the same way. See `prompt.prompt-set-fn`.
> 3. CRASH: the resulting pointer is never checked for NULL. A callback
>    that returns NULL, or a narrow callback whose bytes are not a
>    valid multibyte string in the current locale (which makes
>    `ct_decode_string` return NULL, as does a scratch-buffer
>    allocation failure), reaches step 4 and dereferences NULL. There
>    is no defined behaviour to reproduce; the port should treat a
>    missing string as empty — render nothing, then still perform
>    step 5.
> 4. Walk the string with a cursor `p`, from its first character until
>    the terminating NUL. At each position:
>    a. If `elp->p_ignore` equals `*p`, a literal (non-printing) region
>       opens here. Record `litstart` as the position just past the
>       delimiter, then advance `p` until it reaches either the NUL or
>       the next occurrence of `elp->p_ignore`.
>       - If the scan stopped at the NUL (unterminated region), or the
>         closing delimiter is the final character of the string
>         (nothing follows it), abandon the entire walk immediately:
>         the opening delimiter, the region, and the closing delimiter
>         if there was one, are all discarded and nothing further is
>         rendered. The C marks this "XXX: We lose the last literal".
>         In both cases the string ends there, so the only text lost is
>         the region itself. This is why the manual states that the
>         escape character may not be the last character of a prompt.
>       - Otherwise call `re_putliteral(el, litstart, p)` with `p` at
>         the closing delimiter, then advance `p` past that closing
>         delimiter *and* past the single character following it. That
>         following character is consumed by the literal and is not
>         rendered separately — see below.
>    b. Otherwise call `re_putc(el, *p, 1)`, drawing the character into
>       the virtual display with column shifting, and advance one
>       character.
>    Neither delimiter is ever rendered.
> 5. Copy the drawing cursor into the record: `elp->p_pos.v =
>    el->el_refresh.r_cursor.v` then `elp->p_pos.h =
>    el->el_refresh.r_cursor.h`. Return.
>
> A zero `p_ignore` — the state after `prompt_init` and after any
> non-`_ESC` `prompt_set` — disables branch (a) entirely, since the
> comparison is only ever made against characters of a NUL-terminated
> string. The same character both opens and closes a region; there are
> no distinct start and end markers. (The readline compatibility layer
> normalises `RL_PROMPT_END_IGNORE`, `\2`, into
> `RL_PROMPT_START_IGNORE`, `\1`, in `rl_set_prompt` before the prompt
> ever reaches here, and installs `\1` as `p_ignore` via
> `EL_PROMPT_ESC`.)
>
> How the region escapes the column count. `re_putliteral(el, begin,
> end)` takes the half-open range `[begin, end)` — the characters
> between the delimiters — *plus* `end[1]`, the visible character
> immediately after the closing delimiter. It encodes all of them into
> one multibyte byte string, appends that to the `EditLine`'s literal
> table, and stores a single magic cell value (`EL_LITERAL | index`,
> i.e. bit 0x80000000 set) in the virtual display at the cursor. The
> column cursor then advances by `wcwidth(end[1])` alone — by 1 if that
> width is 0 — with `MB_FILL_CHAR` padding cells written for a
> double-width visible character. When the display is flushed,
> `terminal__putc` sees the `EL_LITERAL` bit and writes the saved bytes
> verbatim without re-encoding. Net effect: the escape sequence is
> emitted at exactly the position of the character it decorates and
> costs zero columns, which is the entire purpose of the mechanism.
>
> Consequences of that gluing which a re-implementation must keep:
> - `<esc>SEQ<esc>` at the very end of a prompt has no character to
>   attach to and is dropped whole (the abandon case in step 4a).
> - An empty region, `<esc><esc>X`, is legal: `X` is still swallowed
>   into the literal, with an empty byte prefix.
> - If the glued character is non-printing (`wcwidth < 0` — any control
>   character, including the delimiter itself when doubled), the
>   literal is discarded in its entirety, escape bytes and visible
>   character both, and no column is consumed.
> - Two regions each attach to their own following character, so
>   colour-on/colour-off around a single visible character is written
>   `<esc>ON<esc>X<esc>OFF<esc>Y` — the `OFF` sequence rides on
>   whatever comes next.
> - The literal table is index-based and is cleared by `literal_clear`
>   at the top of every `re_refresh`, so the magic cell values are only
>   meaningful within the refresh that produced them. `prompt_print`
>   must therefore only ever be called inside a refresh cycle, between
>   that clear and the flush.
>
> Everything outside a literal region goes through `re_putc` with
> shifting, which is *not* the path the input buffer takes: there is no
> `ct_visual_char` expansion, no tab-to-tab-stop handling, and no
> newline handling for prompt text. A control character in a prompt is
> stored raw, is emitted raw to the terminal, and — because `wcwidth`
> returns -1 for it, which `re_putc` folds to 0 and then advances by
> 1 — is counted as exactly one column. A tab counts as one column but
> moves the real terminal to the next tab stop; a newline counts as one
> column but moves the real terminal to column 0 of the next line.
> Either silently desynchronises the column accounting for the rest of
> the session. Non-printing content belongs inside a literal region,
> which is precisely why the mechanism exists. A prompt wider than the
> terminal wraps through `re_nextline`, which zeroes the column, bumps
> the row, and scrolls the virtual display when it runs past the last
> row; `p_pos.v` then records the wrapped row.
>
> Why `p_pos` is the critical output. `prompt_print` appends at
> whatever `el->el_refresh.r_cursor` currently holds; it never resets,
> clears or repositions anything, so placement is the caller's job and
> `re_refresh` zeroes the cursor before each call. For `EL_PROMPT`,
> `p_pos` is where the input text begins, and `re_refresh_cursor` uses
> it as the origin for all cursor arithmetic — a prompt width that is
> one column wrong puts the cursor one column wrong on every subsequent
> keystroke of that line. For `EL_RPROMPT`, `p_pos.h` is the rprompt's
> width and doubles as the in-use flag: `re_refresh` forces it back to
> 0/0 when the rprompt will not fit, and `re_fastaddc` bails out to a
> full refresh when `p_pos.h` is non-zero and the gap to the rprompt
> has shrunk below 3 columns.
>
> How often the callback runs — callers do depend on this.
> `re_refresh` calls `prompt_print(el, EL_RPROMPT)` once at the top
> purely to measure the rprompt (it really is drawn, at column 0, into
> cells the real content then overwrites), then
> `prompt_print(el, EL_PROMPT)` once, then
> `prompt_print(el, EL_RPROMPT)` a second time to place the rprompt at
> the right edge, but only when it fits on the first line with at least
> a one-column gap after the input text. So per refresh the left-prompt
> callback runs exactly once and the right-prompt callback runs twice
> when the rprompt is displayed and once when it is not. `re_refresh`
> itself runs on essentially every keystroke that cannot take the
> `re_fastaddc` fast path, and again on resize, on SIGCONT, on
> `el_set(EL_REFRESH)`, and on every incremental-search redraw — so a
> prompt callback is emphatically *not* invoked once per line, and must
> be cheap and free of side effects. A right-prompt callback that
> returns a different string on its two calls within one refresh is
> measured at one width and drawn at another, corrupting the line. The
> throwaway measuring pass has real side effects of its own: it
> registers its literals in the literal table (indices that the drawing
> pass then never uses), and an rprompt wider than the terminal will
> scroll the virtual display through `re_nextline` before any real
> content is drawn.
>
> Ownership of the string the callback returns. The callback owns it;
> libedit does not copy it wholesale, does not free it, and does not
> write to it. It must stay valid and unchanged for the duration of
> this call only — every character libedit keeps is copied out during
> the walk, into the virtual display or into the literal table's own
> allocation. Because the callback is re-invoked on every refresh, a
> static or otherwise callee-owned buffer is the normal pattern, as
> with the two built-in defaults. In the narrow case the string
> actually walked is `el->el_scratch`, which is shared with `hist`,
> `tty`, `terminal`, `keymacro` and others; it is consumed entirely
> before this function returns, so nothing can invalidate it mid-walk,
> but that is exactly why the narrow decode must not be made lazy or
> deferred in the port.

> [spec:libedit:def:prompt.prompt-set-fn]
> libedit_private int prompt_set(EditLine *el, el_pfunc_t prf, wchar_t c, int op, int wide)

> [spec:libedit:sem:prompt.prompt-set-fn]
> Installs a prompt callback, its literal escape character and its
> narrow/wide flag, for one of the two prompts. It validates nothing,
> draws nothing, does not call the callback, and always returns 0.
>
> 1. Select the record: `&el->el_prompt` if `op` is `EL_PROMPT` (0) or
>    `EL_PROMPT_ESC` (21), otherwise `&el->el_rprompt`. So `EL_RPROMPT`
>    (12), `EL_RPROMPT_ESC` (22) and any unrecognised `op` all land on
>    the right-hand prompt.
> 2. If `prf` is NULL, restore the built-in default for the selected
>    side — `prompt_default` for the `EL_PROMPT`/`EL_PROMPT_ESC` ops,
>    `prompt_default_r` otherwise, using the same `op` test as step 1.
>    Otherwise store `prf` in `p->p_func`.
> 3. `p->p_ignore = c`, unconditionally. Both `el_set` and `el_wset`
>    pass `c == 0` for the non-`_ESC` ops, so installing a prompt with
>    `EL_PROMPT`/`EL_RPROMPT` *clears* any escape character previously
>    installed with the `_ESC` form. The manual's "0 unsets it" is this
>    line.
> 4. Reset `p->p_pos.v = 0` and `p->p_pos.h = 0`. For the rprompt this
>    matters beyond tidiness: `p_pos.h == 0` is the "rprompt not in
>    use" flag, so the stale width of the previous callback cannot leak
>    into the next redraw's fit test before the new one is measured.
> 5. `p->p_wide = wide`. Return 0.
>
> `wide` is 1 from `el_wset` (wide API, callback returns `wchar_t *`)
> and 0 from `el_set` (narrow API, callback returns `char *`). This is
> the only place `p_wide` is ever assigned, and it is the flag
> `prompt_print` branches on; `prompt_init` leaves it alone.
>
> The narrow path is UB by construction: `el_set` pulls a
> `char *(*)(EditLine *)` out of its varargs as an `el_pfunc_t`
> (`wchar_t *(*)(EditLine *)`) and stores it here, and `prompt_print`
> later calls it through that incompatible type. It works on every
> POSIX ABI in scope because both return a pointer in the same place. A
> Rust port should model the callback as a tagged pair of (pointer,
> narrow-or-wide) at the ABI boundary rather than reproduce the pun.
>
> Nothing is copied and nothing is retained but the function pointer;
> the string the callback will eventually return is not the library's
> to own. The change takes effect at the next `re_refresh`.

