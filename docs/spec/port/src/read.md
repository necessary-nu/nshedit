# src/read.c

> [spec:libedit:def:read.el-read-getfn-fn]
> libedit_private el_rfunc_t el_read_getfn(struct el_read_t *el_read)

> [spec:libedit:sem:read.el-read-getfn-fn]
> Returns the character-reading callback currently installed on
> `el_read`, mapped back to the public sentinel when it is the builtin.
>
> 1. Compare `el_read->read_char` against the module-private builtin
>    `read_char` (rule `read.read-char-fn`).
> 2. If they are the same function, return `EL_BUILTIN_GETCFN` — which
>    is a null `el_rfunc_t`, not a real function pointer. Otherwise
>    return `el_read->read_char` unchanged.
>
> No state is modified and there is no failure path. `el_read` is not
> null-checked; the only caller is `el_get(EL_GETCFN, ...)`, which always
> has an initialised `EditLine`.
>
> This is the exact inverse of `el_read_setfn`, so a get/set round trip
> is lossless. The builtin's address is never exposed — it is `static`
> in the C and has no other escape route — so a client can never observe
> it and can never re-install it under its own name; passing
> `EL_BUILTIN_GETCFN` back to `el_read_setfn` is the only way to restore
> it.

> [spec:libedit:def:read.el-read-setfn-fn]
> libedit_private int el_read_setfn(struct el_read_t *el_read, el_rfunc_t rc)

> [spec:libedit:sem:read.el-read-setfn-fn]
> Installs a character-reading callback on `el_read`.
>
> 1. If `rc` compares equal to `EL_BUILTIN_GETCFN` (a null
>    `el_rfunc_t`), store the module-private builtin `read_char`
>    (rule `read.read-char-fn`) into `el_read->read_char`.
> 2. Otherwise store `rc` verbatim. There is no validation of any kind.
> 3. Return 0.
>
> The return value is unconditionally 0: this function cannot fail, so
> `el_set(EL_GETCFN, ...)` always reports success.
>
> The installed callback has signature `int (*)(EditLine *, wchar_t *)`
> and inherits the builtin's return contract, which every caller relies
> on: 1 means one wide character was stored through the out-pointer,
> 0 means end of file, -1 means error with `errno` set. Two places
> depend on the `errno` half of that contract — `el_wgetc` snapshots
> `errno` into `read_errno` whenever the callback returns a negative
> value, and `noedit_wgets` tests `errno == EINTR` after a -1 — so a
> client callback that returns -1 without setting `errno` produces
> unspecified behaviour there.
>
> The callback is also invoked DIRECTLY by `noedit_wgets`, bypassing the
> macro queue and the lazy raw-mode switch, so it must tolerate being
> called while the terminal is still in cooked mode and must not assume
> `el_wgetc` ran first.

> [spec:libedit:def:read.el-read-t]
> struct el_read_t {
>   struct macros macros;
>   el_rfunc_t read_char;
>   int read_errno;
> }

> [spec:libedit:def:read.el-wgetc-fn]
> int el_wgetc(EditLine *el, wchar_t *cp)

> [spec:libedit:sem:read.el-wgetc-fn]
> Produces the next wide character of input, taking it from the pending
> macro queue if one is draining and from the read callback otherwise.
> Returns 1 on success (character stored in `*cp`), 0 on end of file, or
> a negative value on error. This is the single choke point through
> which every edited keystroke passes.
>
> 1. Call `terminal__flush(el)` (an `fflush` of `el->el_outfile`)
>    unconditionally, before anything else — including when the
>    character will come from a macro and no I/O is about to block. This
>    is what guarantees the prompt and any redisplay are on the wire
>    before the process waits for a key.
> 2. Drain the macro queue. Let `ma = &el->el_read->macros`. Loop:
>    a. If `ma->level < 0` the queue is empty; leave the loop and go to
>       step 3.
>    b. If `ma->macro[0][ma->offset]` is `L'\0'`, the front entry is
>       exhausted: `read_pop(ma)` and repeat from (a). This also
>       silently disposes of empty pushed strings.
>    c. Otherwise store `ma->macro[0][ma->offset]` into `*cp` and
>       increment `ma->offset`.
>    d. If the character now at `ma->macro[0][ma->offset]` is `L'\0'`,
>       the character just taken was the last one, so `read_pop(ma)`
>       IMMEDIATELY rather than waiting for the next call. The C
>       comments this "Needed for QuoteMode On": it guarantees
>       `ma->level` has already dropped by the time the caller acts on
>       the character, which is what the quoted-insert path and the
>       `macros.level < 0` test in `el_wgets` observe.
>    e. Return 1.
>    The queue is always read from `macro[0]`, the OLDEST entry — see
>    `el_wpush` and `read_pop` for the FIFO consequence.
> 3. No macro input is pending. Call `tty_rawmode(el)`. If it returns a
>    negative value, RETURN 0 — a terminal-setup failure is reported as
>    end of file and is indistinguishable from one by any caller.
>    `el->el_read->read_errno` is NOT set on this path, `*cp` is NOT
>    written, and `errno` is left as whatever `tty_rawmode` produced.
> 4. Call `(*el->el_read->read_char)(el, cp)` — the builtin `read_char`
>    or a client callback installed via `el_read_setfn`.
> 5. If that returned a negative value, copy the current `errno` into
>    `el->el_read->read_errno`. This exists solely so `el_wgets` can
>    restore the original failure reason after its cleanup
>    (`terminal__flush`, `tty_cookedmode`, `sig_clr`), any of which may
>    clobber `errno`. `read_errno` is never cleared here; `el_wgets`
>    zeroes it on entry, and nothing else writes it.
> 6. Return the callback's value unchanged. `*cp` holds whatever the
>    callback left; the builtin stores `L'\0'` on both 0 and -1, but a
>    client callback is not obliged to.
>
> Macro characters bypass steps 3–6 entirely: the tty is NOT switched to
> raw mode while a macro is draining, `read_errno` is not touched, and
> the character is delivered verbatim — including values the multibyte
> decoder could never have produced, since `el_wpush` copies arbitrary
> `wchar_t`.
>
> Callers uniformly test `!= 1` rather than `< 0`, so EOF and error are
> handled identically at every call site in the library.

> [spec:libedit:def:read.el-wgets-fn]
> const wchar_t * el_wgets(EditLine *el, int *nread)

> [spec:libedit:sem:read.el-wgets-fn]
> The public entry point of the editor: reads and edits one line and
> returns it as a NUL-terminated wide string, or NULL. `*nread` receives
> the length, or a negative marker. This is the main input loop.
>
> **Preamble**
>
> 1. If `nread` is NULL, retarget it at a function-local `int` so the
>    rest of the body can write through it unconditionally; the value is
>    then discarded.
> 2. Set `*nread = 0` and `el->el_read->read_errno = 0`.
>
> **NO_TTY early out**
>
> 3. If `el->el_flags & NO_TTY`: set `el->el_line.lastchar =
>    el->el_line.buffer` (unconditionally, `UNBUFFERED` or not) and
>    return `noedit_wgets(el, nread)` directly. NOTHING else in this
>    function runs — no `read_prepare`, therefore no `sig_set` and no
>    prompt; no `read_finish`; no final `terminal__flush`; and no
>    conversion of `*nread` to -1. A `NO_TTY` caller never sees -1.
>
> **Typeahead check — conditionally compiled**
>
> 4. Guarded by `#ifdef FIONREAD`. If the tty is currently in `EX_IO`
>    ("executing"/cooked) mode AND the macro queue is empty
>    (`el->el_read->macros.level < 0`), issue
>    `ioctl(el->el_infd, FIONREAD, &chrs)` with `chrs` pre-zeroed and the
>    return value IGNORED. If `chrs` is then 0 — nothing already sitting
>    in the tty input queue, or the ioctl failed — call `tty_rawmode(el)`;
>    if THAT fails, set `errno = 0`, `*nread = 0` and return NULL. The
>    intent is to avoid switching line disciplines while typeahead is
>    pending, which would disturb it.
>    This block does not compile on a glibc build: `read.c` includes
>    neither `<sys/ioctl.h>` nor any header that transitively defines
>    `FIONREAD`, so the check is simply absent and the mode switch is
>    left to `el_wgetc`'s lazy `tty_rawmode`. Whether it exists is a
>    property of the host headers, not of libedit. It is also the one
>    exit path that returns NULL with `*nread == 0` and `errno`
>    deliberately zeroed.
>
> **Setup**
>
> 5. If `UNBUFFERED` is clear, call `read_prepare(el)` — install signal
>    handlers, resize, reset the line, print the prompt. When
>    `UNBUFFERED` is set this is deliberately skipped, because
>    `el_set(EL_UNBUFFERED, 1)` already ran `read_prepare` once and the
>    line must persist across calls.
> 6. If `el->el_flags & EDIT_DISABLED`: reset `lastchar = buffer` only
>    when `UNBUFFERED` is clear, call `terminal__flush(el)`, and return
>    `noedit_wgets(el, nread)`.
>    IMPORTANT: this happens AFTER `read_prepare`, so the prompt has been
>    printed and the handlers have been installed — but `read_finish` is
>    never reached on this path, so those handlers stay installed after
>    `el_wgets` returns and the tty is never put back into cooked mode.
>    `EDIT_DISABLED` leaks signal dispositions on every call. A port must
>    decide whether to reproduce the leak; it is observable, because the
>    application's own `SIGINT` handler is displaced.
>
> **Main loop**
>
> 7. Initialise `num = -1` and loop while `num == -1`. `num` is the
>    "line is finished, and this is its length" signal; while it is -1
>    the line is still being edited.
>    a. `read_getcmd(el, &cmdnum, &ch)`. On -1 (EOF, read error, or a
>       key sequence abandoned mid-way) BREAK out of the loop with `num`
>       still -1.
>    b. If `(size_t)cmdnum >= el->el_map.nfunc`, the map slot holds an
>       action index with no function behind it: `continue` — read
>       another command without touching any per-command state. The C
>       calls this the "BUG CHECK".
>    c. Record `el->el_state.thiscmd = cmdnum` and
>       `el->el_state.thisch = ch` before dispatch, because vi's redo
>       machinery reads them from several levels down.
>    d. vi redo recording: if `el->el_map.type == MAP_VI` AND
>       `el->el_map.current == el->el_map.key` (in vi, `key` holds the
>       INSERT map and `alt` the command map, so this means "vi insert
>       mode") AND `el->el_chared.c_redo.pos < el->el_chared.c_redo.lim`
>       (room left in the redo buffer):
>       - if `cmdnum == VI_DELETE_PREV_CHAR` and `c_redo.pos` is not
>         already at `c_redo.buf` and `iswprint(c_redo.pos[-1])`, back
>         `c_redo.pos` up by one — a backspace un-records the character
>         it erases;
>       - otherwise append `ch` at `c_redo.pos++`.
>       This runs BEFORE the command executes.
>    e. `retval = (*el->el_map.func[cmdnum])(el, ch)` — dispatch. `ch`
>       is the character that resolved the binding, which for a
>       multi-key sequence is the LAST character of the sequence.
>    f. `el->el_state.lastcmd = cmdnum`, AFTER the call, so a command
>       function observes the PREVIOUS command in `lastcmd` (yank-pop,
>       vi repeat and the argument logic all depend on that).
>
> **Dispatch table — how each `el_action_t` return changes the loop**
>
> 8. Switch on `retval`:
>    - `CC_CURSOR` (5): `re_refresh_cursor(el)` — move the physical
>      cursor to agree with `el_line.cursor`, no redraw. Loop continues.
>    - `CC_REDISPLAY` (8): `re_clear_lines(el)` then
>      `re_clear_display(el)`, then FALL THROUGH into `CC_REFRESH`.
>    - `CC_REFRESH` (4): `re_refresh(el)`. Loop continues.
>    - `CC_REFRESH_BEEP` (9): `re_refresh(el)` then `terminal_beep(el)`.
>      Loop continues.
>    - `CC_NORM` (0): nothing at all. Loop continues.
>    - `CC_ARGHACK` (3): `continue` — jumps straight back to step 7a,
>      SKIPPING the per-command resets in step 9 AND the `UNBUFFERED`
>      break. That is the entire mechanism by which digit arguments
>      accumulate (`doingarg`/`argument` survive) and the reason an
>      argument prefix does not cause an `UNBUFFERED` caller to return.
>    - `CC_EOF` (2): if `UNBUFFERED` is clear, set `num = 0`, which ends
>      the loop and ultimately returns NULL with `*nread == 0`.
>      Otherwise (`UNBUFFERED` set, and `num` is necessarily -1 because
>      that is the loop condition) append `CONTROL('d')` (0x04) at
>      `el->el_line.lastchar++`, set `cursor = lastchar`, and set
>      `num = 1`. That append is UNBOUNDED — there is no `limit` check
>      and no `ch_enlargebufs` call — so it overruns the line buffer if
>      the line is already at capacity. In practice `UNBUFFERED` returns
>      after every command so the line is short, but the C defends
>      nothing.
>    - `CC_NEWLINE` (1): `num = el->el_line.lastchar - el->el_line.buffer`,
>      ending the loop. Note the newline-producing commands
>      (`ed_newline` and friends) append `'\n'` to the line themselves
>      before returning this, so an "empty" line still yields `num == 1`
>      and a one-character buffer `L"\n"`. `num == 0` from `CC_NEWLINE`
>      is possible only if a command returns it with an empty line, and
>      is then indistinguishable from `CC_EOF`.
>    - `CC_FATAL` (7): `re_clear_display(el)`, `ch_reset(el)` (which
>      resets cursor/lastchar, undo, vi command state, kill mark, map to
>      `el_map.key`, insert mode, `doingarg`, `metanext`, `argument`,
>      `lastcmd` and the history event number),
>      `read_clearmacros(&el->el_read->macros)` — this is the point at
>      which ALL pending macro input is discarded — then `re_refresh(el)`
>      to reprint the prompt. Loop continues with the line emptied.
>    - `CC_ERROR` (6) and `default` (any value the switch does not name,
>      including anything a client function invents):
>      `terminal_beep(el)` then `terminal__flush(el)`. Loop continues.
> 9. After the switch (i.e. for every case except `CC_ARGHACK`), reset
>    `el->el_state.argument = 1`, `el->el_state.doingarg = 0`, and
>    `el->el_chared.c_vcmd.action = NOP`.
> 10. If `el->el_flags & UNBUFFERED`, BREAK — one command per call.
>
> **Epilogue**
>
> 11. `terminal__flush(el)` — flush whatever the last command wrote.
> 12. If `UNBUFFERED` is clear: call `read_finish(el)` (restore cooked
>     mode, remove signal handlers), then set
>     `*nread = (num != -1) ? num : 0`.
>     If `UNBUFFERED` is set: do NOT call `read_finish` (the tty stays
>     raw and the handlers stay installed, by design, until
>     `el_set(EL_UNBUFFERED, 0)`), and set
>     `*nread = el->el_line.lastchar - el->el_line.buffer` — the
>     CUMULATIVE length of the line so far, not the number of characters
>     added by this call.
> 13. If `*nread` is non-zero, return `el->el_line.buffer`.
> 14. Otherwise (`*nread == 0`): if `num == -1` — meaning the loop was
>     broken by `read_getcmd` failing, not by a command finishing the
>     line — set `*nread = -1`, and if `el->el_read->read_errno` is
>     non-zero, restore `errno = el->el_read->read_errno`. Return NULL.
>
> **`*nread` on every exit path**
>
> - `NO_TTY`, or `EDIT_DISABLED`: whatever `noedit_wgets` set — the
>   character count, never negative. NULL when 0.
> - `FIONREAD` typeahead path, `tty_rawmode` failed: 0, `errno` forced
>   to 0, NULL returned.
> - Buffered, line completed: `num` — the character count including the
>   trailing newline. Buffer returned.
> - Buffered, `CC_EOF`: 0, NULL returned, `errno` untouched. This is how
>   a caller distinguishes "user typed the EOF key on an empty line"
>   from a read error.
> - Buffered, `read_getcmd` failed: -1, NULL returned.
> - `UNBUFFERED`, line non-empty: cumulative length, buffer returned.
> - `UNBUFFERED`, line still empty and `num == -1`: -1 and NULL — even
>   though nothing failed. Any command that neither inserts text nor
>   completes the line (a cursor move, a beep, a failed search) run as
>   the first keystroke of an `UNBUFFERED` line reports EOF to the
>   caller. This is a genuine trap and it is observable behaviour that a
>   port must reproduce.
>
> **`errno` on every exit path**
>
> `errno` is only ever explicitly written in two places: forced to 0 by
> the `FIONREAD` failure path, and restored from `read_errno` in step 14
> and only when `read_errno` is non-zero. On a clean end of file the
> read callback returned 0, so `read_errno` stayed 0 and `errno` is left
> holding whatever `terminal__flush`, `tty_cookedmode` or `sig_clr`
> incidentally set — UNSPECIFIED. A caller therefore cannot use `errno`
> to tell clean EOF from a swallowed failure; only `*nread == -1`
> together with a non-zero `errno` is meaningful, and even that requires
> the caller to have zeroed `errno` beforehand.
>
> **State the loop leaves behind**
>
> `el->el_state.thiscmd`, `thisch` and `lastcmd` retain the last
> dispatched command. `argument` is 1 and `doingarg` 0 unless the loop
> exited on a `CC_ARGHACK` (impossible — `CC_ARGHACK` `continue`s) or on
> the `read_getcmd` break (in which case they hold whatever the previous
> iteration left). The macro queue survives across `el_wgets` calls
> unless `CC_FATAL` cleared it.

> [spec:libedit:def:read.el-wpush-fn]
> void el_wpush(EditLine *el, const wchar_t *str)

> [spec:libedit:sem:read.el-wpush-fn]
> Appends a copy of `str` to the pending macro queue, so that subsequent
> `el_wgetc` calls take their characters from it before touching the
> terminal again. This is libedit's only pushback mechanism.
>
> Let `ma = &el->el_read->macros`. The queue is a fixed array of
> `EL_MAXMACRO` (10) `wchar_t *` slots, with `ma->level` holding the
> index of the last occupied slot and -1 meaning empty.
>
> 1. If `str` is non-NULL AND `ma->level + 1 < EL_MAXMACRO` (so at most
>    10 entries, levels 0 through 9):
>    a. Increment `ma->level`.
>    b. Set `ma->macro[ma->level] = wcsdup(str)` — an owning heap copy;
>       the caller's storage is not retained and may be freed or reused
>       immediately.
>    c. If the duplicate is non-NULL, return. The push succeeded.
>    d. Otherwise decrement `ma->level` back to its previous value and
>       fall into step 2.
> 2. Failure — `str` was NULL, the queue was full, or the allocation
>    failed: `terminal_beep(el)` then `terminal__flush(el)`. Nothing is
>    queued.
>
> The function returns void, so no caller can distinguish a successful
> push from a silent drop; the audible beep is the only signal. Note
> that a full queue and an out-of-memory condition produce identical
> observable behaviour.
>
> `ma->offset` is NOT touched by a push. It is the read cursor into
> `macro[0]`, the entry currently draining, and is reset only by
> `read_pop` and `read_clearmacros`.
>
> IMPORTANT: despite the "level" vocabulary and the stack-shaped API,
> the queue is FIFO, not LIFO. `el_wpush` writes at the BACK
> (`macro[++level]`) while `el_wgetc` always reads from the FRONT
> (`macro[0]`). A push issued while a macro is already draining is
> therefore queued BEHIND the remainder of that macro rather than
> spliced in ahead of it, so nested macro expansion plays back in push
> order. The port must reproduce this ordering, not the intuitive stack
> behaviour.
>
> An empty string is a legal push: it occupies a slot until `el_wgetc`
> sees the leading NUL and pops it, contributing no characters.
> `el_wpush(el, NULL)` is a real call site — `read_getcmd` hands it the
> null `val.str` that `keymacro_get` returns on a trie mismatch — and
> its only effect is the beep.
>
> The queue is invalidated wholesale by `CC_FATAL` in `el_wgets` (via
> `read_clearmacros`) and by `read_end`. Nothing else invalidates it; in
> particular it survives across `el_wgets` calls and across line
> completion.

> [spec:libedit:def:read.macros]
> struct macros {
>   wchar_t **macro;
>   int level;
>   int offset;
> }

> [spec:libedit:def:read.noedit-wgets-fn]
> static const wchar_t * noedit_wgets(EditLine *el, int *nread)

> [spec:libedit:sem:read.noedit-wgets-fn]
> The non-editing read path, used when `NO_TTY` or `EDIT_DISABLED` is
> set. It appends raw characters to `el->el_line` until a line
> terminator or end of input, then reports the buffer.
>
> Let `lp = &el->el_line`. This function does NOT reset `lp->lastchar`;
> `el_wgets` has already decided whether to.
>
> 1. Loop, calling `(*el->el_read->read_char)(el, lp->lastchar)` — the
>    read callback DIRECTLY, not through `el_wgetc`. Consequences: the
>    macro queue is never consulted, so `el_wpush` has no effect on this
>    path at all; `tty_rawmode` is never called; `terminal__flush` is
>    never called per character; and `el->el_read->read_errno` is never
>    written. The decoded character is written straight into the line
>    buffer at `lp->lastchar`.
>    While the callback returns 1:
>    a. The character has ALREADY been stored; the space check comes
>       after. If `lp->lastchar + 1 >= lp->limit`, call
>       `ch_enlargebufs(el, 2)`; if that returns 0 (allocation failure),
>       BREAK without advancing `lastchar`, so the character just read
>       is silently lost — step 4 overwrites it with the NUL terminator.
>       Writing at `lp->limit` is in bounds: the line allocation always
>       keeps two unused slots above `limit`. `ch_enlargebufs`
>       reallocates and fixes up `buffer`, `cursor`, `lastchar` and
>       `limit` itself, so `lp`'s fields stay coherent.
>    b. Advance `lp->lastchar`.
>    c. Stop if `el->el_flags & UNBUFFERED` (exactly one character per
>       call), or if the character just accepted — `lp->lastchar[-1]` —
>       is `'\r'` or `'\n'`. The terminator is KEPT in the buffer and
>       counted in `*nread`.
> 2. The loop also ends when the callback returns 0 (end of file) or a
>    negative value (error).
> 3. If the last callback returned -1 AND `errno == EINTR`, reset
>    `lp->lastchar = lp->buffer`, discarding the ENTIRE partial line
>    accumulated by this call — not just the last character. No other
>    `errno` value is special-cased: on any other error the partial line
>    is kept and returned, and on end of file it is likewise kept. This
>    is a data-loss path: an interrupted read throws away everything
>    typed so far on the line and reports it as "nothing read".
>    The test reads the global `errno` rather than anything the callback
>    returned, so a client callback that returns -1 with a stale `EINTR`
>    left in `errno` triggers the discard.
> 4. Set `lp->cursor = lp->lastchar` and write `L'\0'` at
>    `*lp->lastchar`.
> 5. `*nread = (int)(lp->lastchar - lp->buffer)`.
> 6. Return `lp->buffer` if `*nread` is non-zero, otherwise NULL.
>
> `*nread` is NEVER negative on this path. End of file, an interrupted
> read and a failed first allocation all report `*nread == 0` with a
> NULL return, so a `NO_TTY`/`EDIT_DISABLED` caller cannot distinguish
> them, and cannot use the `*nread == -1` convention the editing path
> provides. `errno` after a NULL return is whatever the failing callback
> left, un-normalised; after a successful return it is unspecified.
>
> Under `EDIT_DISABLED` combined with `UNBUFFERED` (but not `NO_TTY`),
> `el_wgets` does not reset `lastchar`, so the line accumulates one
> character per call and `*nread` reports the CUMULATIVE length each
> time rather than the single character added. Under `NO_TTY` the reset
> is unconditional, so each call starts a fresh line even in
> `UNBUFFERED` mode.

> [spec:libedit:def:read.read-char-fn]
> static int read_char(EditLine *el, wchar_t *cp)

> [spec:libedit:sem:read.read-char-fn]
> Reads one wide character from `el->el_infd`, decoding the multibyte
> input stream one byte at a time. This is the builtin `el_rfunc_t`.
> Returns 1 with the character in `*cp`, 0 for end of file, or -1 for
> error; on both 0 and -1 it also stores `L'\0'` in `*cp`.
>
> **Per-call local state**
>
> - `cbuf[MB_LEN_MAX]`, the partial-sequence accumulator, and `cbp`, the
>   count of bytes it holds, initially 0. This is the ONLY read-ahead in
>   the module and it never survives the call: at most one byte is
>   carried across a resynchronisation inside the call, and nothing is
>   carried between calls. `read(2)` is always issued for exactly ONE
>   byte, so libedit never consumes input it does not decode, and a
>   caller may take the descriptor back between characters without
>   losing data.
> - `tried`, initialised to `(el->el_flags & FIXIO) == 0`. Read that
>   carefully — the sense is inverted from what the name suggests.
>   `FIXIO` (set by `el_set(EL_SAFEREAD, 1)`) makes `tried` start at 0,
>   which is what ENABLES the recovery path. With `FIXIO` clear — the
>   DEFAULT — `tried` starts at 1 and `read__fixio` is never consulted,
>   so every `read` failure, `EINTR` included, fails the call at once.
> - `save_errno`, the caller's `errno` on entry, restored after a
>   successful recovery so a recovered error leaves no trace.
>
> **Algorithm** (`again:` labels the top of the read step)
>
> 1. (`again`) Set `el->el_signal->sig_no = 0`.
> 2. Issue `read(el->el_infd, cbuf + cbp, 1)`. While it returns -1:
>    a. Save `errno` into a local `e`.
>    b. Inspect `el->el_signal->sig_no`, which `sig_handler` wrote:
>       - `SIGCONT`: call `el_wset(el, EL_REFRESH)` — that option
>         performs `re_clear_display` + `re_refresh` + `terminal__flush`
>         — then FALL THROUGH into the `SIGWINCH` case.
>       - `SIGWINCH`: call `sig_set(el)` to re-arm every handler, then
>         `goto again`, discarding `e` and re-issuing the read from
>         step 1. The re-arm is required because `sig_handler` restores
>         the previous disposition and re-raises the signal before
>         returning, making libedit's handlers one-shot.
>       - anything else, including 0: fall through to (c).
>       This retry is NOT counted against `tried` and is not bounded: an
>       unending stream of `SIGWINCH` spins here forever, and the
>       redisplay work is redone each time.
>    c. If `tried` is 0 AND `read__fixio(el->el_infd, e)` returns 0:
>       restore `errno = save_errno`, set `tried = 1`, and loop back to
>       re-issue the read — back to the `while` condition, NOT to
>       `again`, so `sig_no` is not re-zeroed. Because `tried` is now 1,
>       this recovery is granted AT MOST ONCE PER CALL: a second `EINTR`
>       within the same `read_char` fails. Since `tried` also survives
>       the `goto again` taken by the decoder, a multi-byte character
>       whose bytes are interrupted twice fails on the second.
>    d. Otherwise set `errno = e`, store `L'\0'` in `*cp`, return -1.
> 3. If the read returned 0, this is end of file: store `L'\0'` in `*cp`
>    and return 0. Note that end of file is only ever detected on a
>    fresh byte; an EOF in the middle of a partial multibyte sequence
>    discards the accumulated bytes and reports plain EOF.
> 4. One byte now sits at `cbuf[cbp]`. Decode, looping:
>    a. Increment `cbp`, so it becomes the number of valid bytes.
>    b. Zero a fresh `mbstate_t` and call
>       `mbrtowc(cp, cbuf, cbp, &mbs)`. The WHOLE accumulator is
>       re-decoded from a clean conversion state on every added byte.
>       The C explicitly notes this "only works because UTF-8 is
>       stateless": for a genuinely stateful encoding (ISO-2022 and
>       relatives) the conversion is wrong, and no shift state is
>       carried either between bytes or between characters. The port
>       inherits that limitation as specified behaviour.
>    c. `(size_t)-1` — invalid sequence:
>       - if `cbp > 1`, RESYNCHRONISE on the last byte: `cbuf[0] =
>         cbuf[cbp - 1]`, `cbp = 0`, then repeat from (a), which makes
>         `cbp` 1 and re-decodes that single retained byte on its own.
>         The earlier bytes of the bad sequence are discarded silently.
>       - if `cbp == 1`, the lone byte is itself invalid: discard it
>         (`cbp = 0`) and `goto again` to read a fresh byte. NO error is
>         reported and nothing is returned to the caller — invalid input
>         is skipped, never surfaced. A stream of garbage bytes makes
>         this function block indefinitely without ever returning.
>    d. `(size_t)-2` — valid but incomplete prefix: if `cbp` has reached
>       `MB_LEN_MAX`, give up — set `errno = EILSEQ`, store `L'\0'` in
>       `*cp`, return -1. Otherwise `goto again` to read one more byte
>       into `cbuf[cbp]`. The bound is exact: the read always targets
>       `cbuf[cbp]` with `cbp < MB_LEN_MAX`, so the accumulator cannot
>       overflow.
>    e. Any other return, INCLUDING 0: a character was decoded into
>       `*cp`; return 1. A return of 0 means a NUL wide character was
>       decoded from an embedded NUL byte, and it is reported as a
>       SUCCESSFUL read of `L'\0'` — distinguishable from end of file
>       only by the return value (1 versus 0), never by `*cp`.
>
> **Signal interaction and its races**
>
> - `sig_no` is a `volatile sig_atomic_t` written by `sig_handler` and
>   cleared here at `again`. It is only ever CONSULTED after a failed
>   read. A signal that arrives while no read is in flight, or one that
>   arrives after the read has already returned its byte, leaves
>   `sig_no` set but unacted-on until the next `again` clears it — so
>   `sig_set` is not re-issued, and because `sig_handler` has already
>   restored the previous disposition, the NEXT `SIGWINCH` or `SIGCONT`
>   goes to whatever handler libedit displaced. Redisplay after a
>   terminal resize is therefore timing-dependent, and after two resizes
>   in the wrong window it can be lost entirely.
> - The window between clearing `sig_no` and the kernel entering the
>   `read` is unprotected. A signal delivered there does not interrupt
>   the read, so the read blocks until a byte arrives. The handler's own
>   work (`el_resize` for `SIGWINCH`; `tty_rawmode`, `ed_redisplay` and
>   `terminal__flush` for `SIGCONT`) has already run, but the re-arm has
>   not. There is no self-pipe, no `pselect`, and no `SA_RESTART`; the
>   race is inherent to the design and a faithful port keeps it or
>   documents its divergence.
> - Signals other than `SIGCONT`/`SIGWINCH` are not special-cased here.
>   `sig_handler` puts the tty back into cooked mode, restores the old
>   disposition and re-raises, so `SIGINT`/`SIGQUIT`/`SIGTERM`/`SIGHUP`
>   normally terminate or stop the process from inside the `read` and
>   never return to this function at all.
> - With `EL_SAFEREAD` off (the default), an `EINTR` from a handler that
>   DOES return surfaces to the caller as -1 with `errno == EINTR`, and
>   `el_wgets` turns that into `*nread == -1`.
>
> **`errno` discipline**
>
> On the -1 paths `errno` holds the failing `read`'s value, or `EILSEQ`
> for an over-long sequence. On the 1 and 0 paths `errno` is whatever
> the last step left: a recovery through `read__fixio` explicitly
> restores the entry value, but the `SIGCONT`/`SIGWINCH` retry does not,
> and the redisplay it performs may set `errno` arbitrarily.
>
> **Blocking versus non-blocking**
>
> This function never selects a mode. It inherits whatever the caller
> left on `el->el_infd`. If the descriptor is non-blocking, the read
> fails with `EWOULDBLOCK`; with `EL_SAFEREAD` off that is returned to
> the caller as -1, and with it on, `read__fixio` clears `O_NONBLOCK` on
> the descriptor permanently and retries once. There is no polling loop
> and no timeout anywhere in this module.

> [spec:libedit:def:read.read-clearmacros-fn]
> static void read_clearmacros(struct macros *ma)

> [spec:libedit:sem:read.read-clearmacros-fn]
> Empties the macro queue, freeing every queued string.
>
> 1. While `ma->level >= 0`, `el_free(ma->macro[ma->level])` and
>    decrement `ma->level` in the same expression — so entries are freed
>    from the back to the front, and `ma->level` finishes at -1.
> 2. Set `ma->offset = 0`.
>
> The array slots themselves are NOT nulled; they are left holding
> dangling pointers. That is safe only because `ma->level == -1` makes
> them unreachable, and because the only other consumer, `read_end`,
> frees the array itself immediately afterwards. A port that keeps the
> slots must not double-free them.
>
> `ma->macro` is not null-checked. On the failure path of `read_init`
> this function is reached with `ma->macro == NULL` and `ma->level`
> uninitialised — see `read.read-init-fn` for that bug.
>
> Called from exactly two places: `read_end`, and the `CC_FATAL` arm of
> `el_wgets`, which is the only way pending macro input is discarded
> mid-line.

> [spec:libedit:def:read.read-end-fn]
> libedit_private void read_end(EditLine *el)

> [spec:libedit:sem:read.read-end-fn]
> Tears down the read subsystem, releasing everything `read_init`
> allocated.
>
> 1. `read_clearmacros(&el->el_read->macros)` — free every queued macro
>    string, leaving `level == -1` and `offset == 0`.
> 2. `el_free(el->el_read->macros.macro)` and set that pointer to NULL.
> 3. `el_free(el->el_read)` and set `el->el_read` to NULL.
>
> Nothing is returned and no failure is possible in the normal case.
>
> `el->el_read` is NOT null-checked, so calling this twice, or before
> `read_init` has run, dereferences NULL. The step-3 assignment to NULL
> makes a double call fail loudly rather than double-free, but only
> because the null dereference in step 1 comes first.
>
> This is also the cleanup path used by `read_init`'s own error handler,
> where it is reached with the structure only half-initialised — see
> `read.read-init-fn`.

> [spec:libedit:def:read.read-finish-fn]
> libedit_private void read_finish(EditLine *el)

> [spec:libedit:sem:read.read-finish-fn]
> Undoes what `read_prepare` set up, at the end of a line or when
> unbuffered mode is switched off.
>
> 1. If `el->el_flags & UNBUFFERED` is clear, call `tty_cookedmode(el)`,
>    discarding its return value. When `UNBUFFERED` is set the tty is
>    deliberately LEFT in raw mode, because the caller is mid-line and
>    will call back in.
> 2. If `el->el_flags & HANDLE_SIGNALS` is set, call `sig_clr(el)`,
>    restoring every signal disposition libedit displaced. Note this is
>    NOT gated on `UNBUFFERED`: the handlers come off even on the path
>    that leaves the tty raw.
>
> Nothing else is touched — the line buffer, the macro queue and
> `read_errno` all survive.
>
> Two callers, with different flag states at the moment of the call:
> `el_wgets` calls it only when `UNBUFFERED` is clear, so both steps run
> there; `el_set(EL_UNBUFFERED, 0)` clears the flag FIRST and then calls
> this, so it too takes the `tty_cookedmode` branch. Consequently the
> `UNBUFFERED`-set branch of step 1 is never exercised by libedit's own
> code, only by a direct internal call.
>
> The asymmetry with `read_prepare` is that `read_prepare` returns early
> for `NO_TTY` while this function has no `NO_TTY` guard — but `NO_TTY`
> never reaches `read_finish` from `el_wgets`, because that path returns
> before the epilogue. Likewise `EDIT_DISABLED` returns early and never
> reaches this function, which is why it leaks the handlers `sig_set`
> installed.

> [spec:libedit:def:read.read-fixio-fn]
> static int read__fixio(int fd __attribute__((__unused__)), int e)

> [spec:libedit:sem:read.read-fixio-fn]
> Attempts to make a failed `read(2)` retryable, given the `errno` value
> `e` that caused it. Returns 0 if the caller should retry, -1 if the
> error is not recoverable. The `__unused__` annotation on `fd` is
> stale: `fd` IS used on the recovery path wherever it compiles in.
>
> Dispatch on `e`:
>
> - `-1`: never a real `errno` value. The case label exists only so the
>   compiler cannot prove the block below unreachable when every other
>   label is preprocessed away. It shares the block with the would-block
>   case.
> - `EWOULDBLOCK`, and `EAGAIN` where the platform gives it a value
>   distinct from `EWOULDBLOCK`: the descriptor is in non-blocking mode
>   and had nothing to give. Seed a working value `e = 0`, then:
>   1. `e = fcntl(fd, F_GETFL, 0)`; if it returns -1, return -1.
>   2. `fcntl(fd, F_SETFL, e & ~O_NDELAY)`; if it returns -1, return -1;
>      otherwise set `e = 1`.
>   3. Where `FIONBIO` is visible, additionally `ioctl(fd, FIONBIO,
>      &zero)` with `zero == 0`; -1 returns -1, otherwise `e = 1`. On a
>      glibc build this sub-block is ABSENT — `read.c` includes neither
>      `<sys/ioctl.h>` nor anything defining `FIONBIO` — so only the
>      `fcntl` half compiles.
>   4. Return 0 if `e` is non-zero, -1 otherwise. Because `e` was seeded
>      to 0, a build where neither sub-block compiles returns -1 from
>      this arm unconditionally.
> - `EINTR`: return 0 with NO side effects whatsoever — a pure "retry
>   me".
> - anything else: return -1.
>
> IMPORTANT: the side effect on the would-block arm is the point and the
> trap. Recovering from a would-block error PERMANENTLY clears
> `O_NONBLOCK`/`O_NDELAY` on the caller's input descriptor. That
> descriptor is normally the process's standard input, shared with the
> rest of the program and possibly with other processes through a
> common open file description, so libedit silently converts the
> application's non-blocking stdin into a blocking one. Nothing is saved
> and nothing is restored on the way out of `el_wgets`. A port must
> either reproduce this or treat it as a deliberate, documented
> divergence.
>
> `EAGAIN` earns its own case label only when the platform defines it
> distinctly from `EWOULDBLOCK` AND the macro `POSIX` is defined.
> libedit's build never defines `POSIX`, and on Linux the two values are
> equal anyway, so `EWOULDBLOCK` is the only label that matters in
> practice. A port should treat "would block" as a single condition
> regardless of spelling.
>
> This function is reached only when the `FIXIO` flag is set on the
> `EditLine` (`el_set(EL_SAFEREAD, 1)`); see `read.read-char-fn` for the
> inverted-looking test that decides that, and note the retry it grants
> is available at most once per `read_char` call.

> [spec:libedit:def:read.read-getcmd-fn]
> static int read_getcmd(EditLine *el, el_action_t *cmdnum, wchar_t *ch)

> [spec:libedit:sem:read.read-getcmd-fn]
> Reads input until it has resolved a complete editor command. Stores
> the command's action index in `*cmdnum` and the character that
> produced it in `*ch`. Returns 0 on success, -1 on end of file or read
> error.
>
> Repeat the following until the resolved action is not
> `ED_SEQUENCE_LEAD_IN`:
>
> 1. `el_wgetc(el, ch)`. If it returns anything other than 1 — 0 for end
>    of file or for a `tty_rawmode` failure, negative for a read error —
>    return -1 IMMEDIATELY. `*cmdnum` is left untouched and `*ch` may be
>    unmodified, so neither out-parameter is meaningful after -1.
> 2. Apply a pending meta prefix: if `el->el_state.metanext` is set,
>    clear it and OR `0x80` into `*ch`. This happens BEFORE the map
>    lookup, so `ESC` followed by `a` looks up index 0xE1 rather than
>    0x61. For a character that already has bit 7 set the OR is a no-op,
>    which silently merges the meta and non-meta bindings for those
>    characters.
>    (A `KANJI` variant that instead maps any character with bit 7 set
>    to `CcViMap[' ']` and clears `metanext` exists in the source, but
>    `KANJI` is never defined anywhere in the tree; it is not part of
>    the port.)
> 3. Resolve the action:
>    - if `*ch >= N_KEYS` (256), the action is `ED_INSERT`. Wide
>      characters above U+00FF are NEVER looked up in a key map; they go
>      straight to self-insert and cannot be rebound.
>    - otherwise the action is `el->el_map.current[(unsigned char)*ch]`
>      — the currently active 256-entry map (emacs, vi-insert or
>      vi-command, depending on what `el_map.current` points at). The
>      `unsigned char` cast is redundant for the values that reach it,
>      since the range test already excludes anything above 255.
> 4. If the resolved action is `ED_SEQUENCE_LEAD_IN`, this character
>    begins a multi-key binding. Call `keymacro_get(el, ch, &val)`,
>    which walks the binding trie from the root, consuming further
>    characters through `el_wgetc` as it descends, and leaves the LAST
>    character it read in `*ch`. Act on its return:
>    - `XK_CMD`: a complete binding resolving to a command. Set the
>      action to `val.cmd` and leave the loop. `*ch` is now the FINAL
>      character of the sequence, not the first, and that is what gets
>      handed to the command function and recorded in
>      `el->el_state.thisch`.
>    - `XK_STR`: either a complete binding resolving to a replacement
>      string, or a MISMATCH — the trie returns `XK_STR` with
>      `val.str == NULL` when no sibling matches at some depth, and sets
>      `*ch` to `L'\0'`. Call `el_wpush(el, val.str)` in both cases,
>      then loop. On a real string binding this queues the replacement
>      for `el_wgetc` to replay. On a mismatch the null pointer makes
>      `el_wpush` merely beep, and the characters already consumed by
>      the trie walk are DISCARDED — there is no pushback of the
>      unmatched prefix, so an unrecognised escape sequence swallows its
>      own bytes. Either way the action is still `ED_SEQUENCE_LEAD_IN`,
>      which is precisely what makes the do/while repeat.
>    - `XK_NOD`: `el_wgetc` failed part-way through the sequence (end of
>      file or read error). Return -1.
>    - anything else: `EL_ABORT`, i.e. `abort()` — with a diagnostic to
>      `el_errfile` under `DEBUG`. Unreachable; the trie only ever
>      yields the three values above.
> 5. Once the loop ends, store the resolved action in `*cmdnum` and
>    return 0. `*cmdnum` is written only on this path.
>
> **Partial matches block, with no timeout.** While the trie is
> descending it calls `el_wgetc` with no timeout of any kind — there is
> no `keyseq-timeout` equivalent, no `select`, no `VMIN`/`VTIME` trick.
> A prefix of a bound sequence (a bare `ESC` under a map where `ESC`
> leads in) therefore hangs until another character arrives or the input
> ends. If the input ends, the walk returns `XK_NOD`, this function
> returns -1, and `el_wgets` abandons the whole line as if end of file
> had been seen at the very start — the characters already typed on the
> line are still in the buffer and are still returned, but the partially
> matched sequence is gone. That is the specified behaviour; a port must
> reproduce it rather than inventing a timeout.
>
> The loop can also repeat when a string binding's replacement itself
> begins with a lead-in character. A self-referential string binding
> loops forever, which the C does not guard against. If `el_wpush` fails
> because the queue is full, the loop still repeats but then reads a
> genuinely new character from the terminal, so it makes progress.

> [spec:libedit:def:read.read-init-fn]
> libedit_private int read_init(EditLine *el)

> [spec:libedit:sem:read.read-init-fn]
> Allocates and initialises the read subsystem. Returns 0 on success,
> -1 on failure.
>
> 1. `el->el_read = el_malloc(sizeof(struct el_read_t))`. If NULL,
>    return -1 with nothing allocated and `el->el_read` left holding
>    NULL.
> 2. Let `ma = &el->el_read->macros`. Allocate the macro slot array:
>    `ma->macro = el_calloc(EL_MAXMACRO, sizeof(wchar_t *))` — 10
>    zeroed pointer slots. If NULL, jump to the cleanup at step 5.
> 3. Set `ma->level = -1` (queue empty) and `ma->offset = 0`.
> 4. Set `el->el_read->read_char` to the builtin `read_char`
>    (rule `read.read-char-fn`) and return 0.
> 5. Cleanup: call `read_end(el)` and return -1.
>
> `el->el_read` comes from `malloc`, not `calloc`, and `read_errno` is
> never assigned here. It stays INDETERMINATE until `el_wgets` zeroes it
> on entry or `el_wgetc` writes a failing `errno` into it. Nothing reads
> it before then, so the C gets away with it, but a port should
> zero-initialise it.
>
> IMPORTANT — bug on the out-of-memory path. `ma->level` is assigned in
> step 3, AFTER the allocation in step 2. If step 2 fails, step 5 calls
> `read_end`, which calls `read_clearmacros`, which loops
> `while (ma->level >= 0) el_free(ma->macro[ma->level--])` on an
> `ma->level` that was never initialised — indeterminate bytes from
> `malloc`. If those bytes happen to be non-negative, the loop
> dereferences `ma->macro`, which is NULL, and/or passes garbage
> pointers to `free`. This is undefined behaviour reachable purely by
> allocation failure. A port must set `level` to -1 before anything can
> fail, or use a representation where "empty" is the default.

> [spec:libedit:def:read.read-pop-fn]
> static void read_pop(struct macros *ma)

> [spec:libedit:sem:read.read-pop-fn]
> Removes the front entry from the macro queue, so that `macro[0]`
> becomes the next one to drain.
>
> 1. `el_free(ma->macro[0])` — the front entry's string.
> 2. For `i` from 0 up to but excluding `ma->level`, shift
>    `ma->macro[i] = ma->macro[i + 1]`, moving every remaining entry one
>    slot toward the front. The queue is FIFO, so the front is what
>    leaves.
> 3. Decrement `ma->level`.
> 4. Set `ma->offset = 0`, so the new front entry is read from its first
>    character.
>
> After the shift, the slot at the OLD `ma->level` still holds a
> duplicate of the pointer now at `ma->level` — a stale alias sitting
> above the new top. It is unreachable because it is above `level`, but
> a port that eagerly frees the whole array must not free it twice.
>
> There is no guard on `ma->level >= 0`. Calling this on an empty queue
> frees `macro[0]` a second time — a double free — and drives `level` to
> -2. Every caller (`el_wgetc` in two places, both under
> `ma->level >= 0`) satisfies the precondition, so this is a latent
> hazard rather than a live bug, but the precondition is part of the
> contract.
>
> The special case `ma->level == 0` works out: the shift loop does not
> execute, `level` becomes -1, `offset` becomes 0, and the dangling
> `macro[0]` is never read because the queue now tests as empty.

> [spec:libedit:def:read.read-prepare-fn]
> libedit_private void read_prepare(EditLine *el)

> [spec:libedit:sem:read.read-prepare-fn]
> Sets up the terminal, the display and the line state before a line is
> edited. Called from `el_wgets` when `UNBUFFERED` is clear, and from
> `el_set(EL_UNBUFFERED, 1)` at the moment unbuffered mode is switched
> on.
>
> 1. If `el->el_flags & HANDLE_SIGNALS`, call `sig_set(el)` — install
>    libedit's handler for `SIGINT`, `SIGTSTP`, `SIGQUIT`, `SIGHUP`,
>    `SIGTERM`, `SIGCONT` and `SIGWINCH`, saving the previous
>    dispositions. This runs FIRST and, importantly, runs even for
>    `NO_TTY`.
> 2. If `el->el_flags & NO_TTY`, return now. Nothing below runs — no
>    resize, no prompt, no line reset. (In practice `el_wgets` never
>    reaches this function under `NO_TTY`, but `el_set(EL_UNBUFFERED, 1)`
>    can.)
> 3. If `(el->el_flags & (UNBUFFERED|EDIT_DISABLED)) == UNBUFFERED` —
>    i.e. unbuffered mode is on AND editing is not disabled — call
>    `tty_rawmode(el)`, ignoring its return value. In every other
>    configuration the switch to raw mode is left to `el_wgetc`, which
>    does it lazily on the first character that is not supplied by a
>    macro.
> 4. `el_resize(el)` — re-query the window size (with `SIGWINCH` blocked
>    around the query) and propagate any change. The C notes this is
>    "relatively cheap, and things go terribly wrong if we have the
>    wrong size", so it is unconditional on every line, not cached.
> 5. `re_clear_display(el)` — discard the display model so the next
>    refresh redraws from scratch.
> 6. `ch_reset(el)` — reset `cursor` and `lastchar` to `buffer`,
>    invalidate the undo state, clear the vi pending command, reset the
>    kill mark, set `el_map.current = el_map.key`, set insert mode,
>    clear `doingarg` and `metanext`, set `argument = 1`, set
>    `lastcmd = ED_UNASSIGNED`, and zero the history event number.
> 7. `re_refresh(el)` — draw the (now empty) line, which is what prints
>    the prompt.
> 8. If `el->el_flags & UNBUFFERED`, call `terminal__flush(el)` so the
>    prompt reaches the terminal immediately. In buffered mode the flush
>    is left to `el_wgetc`, which flushes before every blocking read.
>
> Nothing is returned and no failure is reported; `tty_rawmode`'s error
> is discarded here, and if it failed, `el_wgetc` will hit it again and
> report it as end of file.
>
> The macro queue and `read_errno` are NOT touched, so pushed macros
> survive a `read_prepare` and are consumed by the line it prepares.

