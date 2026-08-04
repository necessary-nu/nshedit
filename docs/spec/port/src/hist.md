# src/hist.c, src/hist.h

> [spec:libedit:def:hist.el-history-t]
> typedef struct el_history_t

> [spec:libedit:def:hist.hist-command-fn]
> libedit_private int hist_command(EditLine *el, int argc, const wchar_t **argv)

> [spec:libedit:sem:hist.hist-command-fn]
> Implements the `history` builtin of the editor's command language. It is
> reached from `el_wparse`/`el_parse`, hence from `parse_line`, hence from
> `.editrc` lines and `el_source`. `argv[0]` is the command name and is
> never examined here; `argc` counts it. Returns 0 on success, -1 on
> error, or the underlying store's status for the two setter forms.
> `el_wparse` negates whatever comes back before returning it.
>
> 1. If `el->el_history.ref == NULL`, return -1. No history is installed,
>    so there is nothing to list or configure.
> 2. **List form** — selected when `argc == 1`, or when `argv[1]` is
>    exactly `L"list"`. Any further arguments are ignored. Prints the whole
>    history to `el->el_outfile`:
>    - Begin with `buf = NULL`, `maxlen = 0`, `hno = 1`.
>    - Walk with `HIST_LAST(el)` (the **oldest** entry) and then repeated
>      `HIST_PREV(el)` (each step toward the more recent), stopping at the
>      first NULL. The listing is therefore chronological, oldest first. On
>      an empty history the loop body never runs, nothing is printed, and
>      the function returns 0. With libedit's own store this walk leaves
>      the store's traversal cursor sitting on the most recent entry —
>      `H_PREV` refuses to step onto the list sentinel — which is an
>      observable side effect on the history object.
>    - For each entry string `str` (a `wchar_t *`):
>      1. `ptr = ct_encode_string(str, &el->el_scratch)` — encode to a
>         multibyte byte string in the current locale, into the shared
>         scratch conversion buffer `el_scratch.cbuff`. Wide characters the
>         locale cannot represent are dropped silently (they encode to zero
>         bytes). If this returns NULL — its only failure is an allocation
>         failure while growing the scratch buffer — the C dereferences it
>         immediately in the next step. That is a crash on out-of-memory
>         and the port must not reproduce it; treat it as a -1 return.
>      2. `len = strlen(ptr)`; if `len > 0` and the final byte is `'\n'`,
>         overwrite that byte with `'\0'` and decrement `len`. Exactly one
>         trailing newline is stripped, and it is stripped in place in the
>         scratch buffer, not in the stored entry. Entries the editor
>         itself stores carry the terminating newline of the line the user
>         entered, which is why this exists.
>      3. `len = len * 4 + 1` — the worst-case escaped size (four output
>         characters per input byte) plus a terminator.
>      4. If `len >= maxlen`: set `maxlen = len + 1024`, reallocate `buf`
>         to `maxlen` bytes, and on failure free `buf` and return -1.
>         `maxlen` never shrinks, so the scratch output buffer grows
>         monotonically across the listing and is reused otherwise.
>      5. `strvis(buf, ptr, VIS_NL)` — escape into `buf`. See below.
>      6. `fprintf(el->el_outfile, "%d\t%s\n", hno++, buf)`. The number is
>         a fresh 1-based counter over this walk, **not** the history event
>         number (`ev.num`), and nothing ever reads it back. `fprintf`
>         failures are not checked.
>    - After the loop, free `buf` and return 0.
> 3. Otherwise, if `argc != 3`, return -1. Both remaining subcommands take
>    exactly one argument.
> 4. `num = (int)wcstol(argv[2], NULL, 0)` — base 0, so `10`, `012` and
>    `0xa` all parse. There is no error checking at all: a non-numeric
>    argument yields 0, a value outside `int` is truncated by the cast, and
>    `wcstol`'s `errno` is never consulted.
> 5. `history size N` → return `history_w(el->el_history.ref, &ev,
>    H_SETSIZE, num)` directly (0 or -1). `ev` is an uninitialised local
>    `HistEventW`, filled by the callee and then discarded, so the error
>    string is thrown away.
> 6. `history unique N` → return `history_w(el->el_history.ref, &ev,
>    H_SETUNIQUE, num)`, likewise.
> 7. Any other `argv[1]` → return -1.
>
> **Bug in steps 5 and 6 — they bypass the installed dispatcher.** Both
> call `history_w` directly on `el->el_history.ref` instead of calling
> through `el->el_history.fun`. `ref` is an opaque handle whose real type
> is whatever the application passed to `EL_HIST`; the cast is only correct
> when that was libedit's own *wide* store (`history_winit` + `history_w`).
> - With the **narrow** store (`history_init` + `history`) — what the
>   narrow `el_set(EL_HIST, …)` and the readline compatibility layer both
>   install — the two structs are layout-compatible so nothing crashes, but
>   `history_setsize`/`history_setunique` check that the store's `next`
>   hook is the *wide* default, find the narrow one at that slot instead,
>   and return -1 with `_HE_NOT_ALLOWED`. `history size` and
>   `history unique` in an `.editrc` are therefore silently inoperative for
>   every narrow-API application, which is the common case.
> - With a **custom** history function the handle is the application's own
>   object and this is straight type confusion. That is undefined
>   behaviour; the C gives it no meaning and neither does this rule. The
>   port must fail rather than invent one.
>
> The port must preserve the observable outcome (a -1 for a narrow store)
> rather than "fix" it into working — the C ABI freezes it — while
> expressing it as a checked dispatch instead of a punned pointer.
>
> **The escaping: `strvis(buf, ptr, VIS_NL)`, and why it is not
> `VIS_WHITE`.** These are the two `strvis` call sites in libedit and their
> flags differ on purpose.
> - `history.c`'s save path writes the history *file* and uses
>   `VIS_WHITE` = `VIS_SP | VIS_TAB | VIS_NL`. That output is a wire
>   format: it is parsed back by `strunvis` on load, one entry per line,
>   and leading and trailing spaces and tabs must survive the round trip
>   byte for byte. Escaping all three whitespace characters is what makes
>   the encoding total.
> - `hist_command` writes a human-readable listing to the user's terminal
>   and uses `VIS_NL` alone. Nothing ever parses this back. The only
>   escaping that is structurally required is the newline: the output
>   format is `"%d\t%s\n"`, one entry per printed line, and an entry
>   containing an embedded newline would otherwise split across lines and
>   desynchronise the numbering. Spaces and tabs are deliberately left
>   literal — history entries are command lines, and rendering every space
>   as `\040` would make the listing unreadable.
> - So `VIS_NL` is not a weakened `VIS_WHITE`; it is the minimum that
>   preserves the listing's one-line-per-entry structure, where `VIS_WHITE`
>   is the maximum that preserves an exact byte-level round trip.
>
> What `strvis(buf, ptr, VIS_NL)` concretely produces, given that
> `VIS_CSTYLE`, `VIS_OCTAL` and `VIS_NOSLASH` are all absent: the escape
> set is `{'\n', '\\'}` — the backslash is always added when `VIS_NOSLASH`
> is clear — and members of that set are emitted as a backslash followed by
> **three octal digits**. A newline becomes `\012` and a backslash becomes
> `\134`; there is no `\n` C-style form here. Space and tab pass through
> literally. Characters graphic in the current locale pass through
> literally. Other control characters become `\^X` (`\^?` for DEL) and
> bytes with the high bit set become `\M-x` or `\M^X`. Four output
> characters per input byte is the maximum, which is exactly what the
> `len * 4 + 1` sizing in step 2.iii assumes. `strvis` does no bounds
> checking whatsoever — that sizing is the only thing standing between this
> and a heap overflow, and it is an assumption rather than a guarantee in
> any locale where one input byte can decode to a wide character needing
> more than one significant byte to escape.
>
> Because step 2.ii already removed a trailing newline, `VIS_NL` only ever
> fires on a newline *inside* an entry, or on a second trailing one.

> [spec:libedit:def:hist.hist-convert-fn]
> libedit_private wchar_t * hist_convert(EditLine *el, int fn, void *arg)

> [spec:libedit:sem:hist.hist-convert-fn]
> The `NARROW_HISTORY` adapter. Every history access in the editor goes
> through the `HIST_FUN` macro, which chooses between two shapes: when
> `el->el_flags & NARROW_HISTORY` is clear, the installed store is the wide
> one, its event string is already `wchar_t *`, and the macro reads it
> straight out of `el->el_history.ev`; when the flag is set, the installed
> store is the narrow one, its event string is really `char *`, and this
> function is called to fetch and convert it.
>
> 1. Declare a **local** `HistEventW ev`.
> 2. Call `(*el->el_history.fun)(el->el_history.ref, &ev, fn, arg)` — the
>    installed dispatcher, with the requested `H_*` operation code and its
>    one variadic argument. `arg` is `NULL` for the traversal operations
>    (`H_FIRST`, `H_LAST`, `H_NEXT`, `H_PREV`).
> 3. If it returns -1, return NULL. The event is discarded. Callers cannot
>    distinguish this from an empty history; both surface as NULL.
> 4. Otherwise return `ct_decode_string((const char *)(const void *)ev.str,
>    &el->el_scratch)` — reinterpret the event string as a multibyte byte
>    string, decode it in the current locale with `mbstowcs` into
>    `el->el_scratch.wbuff`, and return that buffer. This yields NULL if
>    the bytes are not a valid multibyte sequence in the current locale, or
>    if the scratch buffer cannot be grown — both of which the caller again
>    cannot distinguish from "no such entry".
>
> **The cast is a type pun and is undefined behaviour.** `ev` is declared
> `HistEventW` (`{int num; const wchar_t *str;}`) but the callee, being the
> narrow store, writes a `HistEvent` (`{int num; const char *str;}`)
> through the pointer. The two are layout-compatible on every ABI libedit
> supports, so it works in practice, but the standard does not license it;
> the double cast through `const void *` exists only to silence the
> resulting `wchar_t *` → `char *` conversion. A Rust port should model the
> narrow event as the narrow event it actually is rather than reproduce the
> pun.
>
> **Bug: the event cookie is never updated on this path.** The wide path
> (`HIST_FUN_INTERNAL`) writes into `el->el_history.ev`, the cookie living
> in the `EditLine`. This function writes into a local instead, so under
> `NARROW_HISTORY` `el->el_history.ev` is never touched by any history
> operation and retains whatever it started with — all zeroes, from the
> `calloc` of the `EditLine`. The one reader of that cookie,
> `vi_to_history_line`, computes `eventno = 1 + el->el_history.ev.num -
> el->el_state.argument` after a successful `hist_get`, so under narrow
> history the vi `G` command with a count reads a stale `num` of 0 and
> derives a wrong (normally negative, and therefore rejected) event.
> Narrow history is what `el_set` and the readline layer install, so this
> is the default configuration, not a corner. The divergence is observable
> and the port must decide about it deliberately rather than by accident.
>
> **Lifetime of the returned pointer.** It is `el->el_scratch.wbuff`, a
> buffer shared across the whole `EditLine`. The next call to
> `hist_convert`, or to anything else that decodes into `el_scratch`
> (`ct_decode_string`, `ct_decode_argv`), invalidates it. Every caller
> holds only the most recently returned value, which is what makes this
> safe in practice. `ct_encode_string` writes the sibling field `cbuff`, so
> `hist_command`'s list loop — which decodes each entry into `wbuff` and
> then re-encodes it into `cbuff` — does not clobber itself; the cost is a
> lossy `char → wchar_t → char` round trip through the locale for entries
> holding bytes the locale cannot decode.
>
> **Precondition.** `el->el_history.fun` is called with no NULL check. Call
> sites guard on `el->el_history.ref == NULL` instead, which is only
> equivalent because `hist_set` writes both fields together. An application
> that installs a NULL `fun` alongside a non-NULL `ref` reaches a NULL
> indirect call here — undefined behaviour, undefended by the C.

> [spec:libedit:def:hist.hist-end-fn]
> libedit_private void hist_end(EditLine *el)

> [spec:libedit:sem:hist.hist-end-fn]
> Tears down the editor-side history bridge. Two statements, no return
> value, no failure path:
>
> 1. `el_free(el->el_history.buf)` — release the saved-line stash. Freeing
>    NULL is a no-op, so this is safe after a failed `hist_init` and safe
>    to call twice.
> 2. `el->el_history.buf = NULL`.
>
> Nothing else in `el_history` is touched. `sz` keeps its last value, `last`
> keeps pointing into the memory just released and is now dangling, and
> `fun`, `ref`, `eventno` and `ev` are left exactly as they were.
>
> This function does **not** call through `el->el_history.fun`, so the
> application's history store is not notified, not reset and not
> destroyed. Its lifetime belongs entirely to the application —
> `history_end` is the application's to call — and a port must not attach
> ownership of the store to the `EditLine`.
>
> The state left behind is unusable: a `hist_get` with `eventno == 0` after
> this would copy `sz` (still non-zero) characters from a NULL `buf` and
> derive `lastchar` from the dangling `last`. Nothing reaches that state in
> the C, because the only caller is `el_end`, which runs `el_reset` first
> and frees the `EditLine` immediately after. The port should additionally
> clear `sz` to 0 and drop `last`; that is unobservable across the C ABI
> and removes the trap.

> [spec:libedit:def:hist.hist-enlargebuf-fn]
> libedit_private int /*ARGSUSED*/ hist_enlargebuf(EditLine *el, size_t newsz)

> [spec:libedit:sem:hist.hist-enlargebuf-fn]
> Grows the saved-line stash to hold `newsz` `wchar_t` (a count of
> characters, not bytes). Called from exactly one place —
> `ch_enlargebufs`, as the last of its reallocations, with the same
> `newsz` the line buffer was just grown to. That lockstep is the invariant
> that makes `hist_get`'s restore safe, and the port must preserve it.
>
> The return convention is **inverted** relative to most of libedit:
> **1 means success, 0 means failure.**
>
> 1. Read `oldsz = el->el_history.sz`.
> 2. If `newsz <= oldsz`, return 1 immediately. The stash is never shrunk;
>    an equal or smaller request is a successful no-op.
> 3. Reallocate `el->el_history.buf` to `newsz * sizeof(wchar_t)`. On
>    failure return 0 with every field unchanged and the old buffer still
>    allocated and still valid — `realloc` does not free on failure, and
>    this code relies on that. The caller turns the 0 into its own 0
>    return, so the editor survives; note it survives having *already*
>    grown the line buffer, leaving `el_history.sz` smaller than the line
>    buffer's allocation. That direction is harmless (`hist_get` copies
>    `sz` characters into a buffer at least that large). The opposite
>    direction — stash larger than line buffer — cannot arise, because the
>    line buffer is grown first and a failure there aborts before reaching
>    this function.
> 4. Zero the newly added tail: the `newsz - oldsz` elements starting at
>    index `oldsz` are set to `L'\0'`. The first `oldsz` elements are left
>    alone, so a saved line survives the growth.
> 5. Rebase the stash-length pointer: `last = newbuf + (last - oldbuf)`.
>    The offset is computed against the *old* `buf` before `buf` is
>    overwritten, so the recorded length is preserved across the move.
> 6. Store `buf = newbuf` and `sz = newsz`.
> 7. Return 1.
>
> The `/*ARGSUSED*/` lint marker in the C is stale; both parameters are
> used.
>
> If `hist_init`'s allocation failed, `buf` is NULL and `oldsz` is 0. Step
> 3 then degenerates to a plain allocation, step 4 zeroes the whole
> buffer, and step 5 computes an offset of 0 from two null pointers (the
> `NULL - NULL` caveat noted under `hist_init`). The effect is that the
> first line-buffer growth silently repairs a failed `hist_init`.

> [spec:libedit:def:hist.hist-fun-t-void-hist-event-w-int]
> typedef int (*hist_fun_t)(void *, HistEventW *, int, ...)

> [spec:libedit:def:hist.hist-get-fn]
> libedit_private el_action_t hist_get(EditLine *el)

> [spec:libedit:sem:hist.hist-get-fn]
> Loads the event named by `el->el_history.eventno` into the edit line.
> This is the single point at which history recall becomes visible: every
> history motion command sets `eventno` and then calls this, and this
> function does all of the buffer, cursor and error-signalling work.
>
> **The `eventno` model.** `eventno` is an ordinal counting backwards into
> the past, not a history event id. 0 means "the line the user was actually
> typing"; 1 means the most recent history entry; 2 the one before it, and
> so on. `ch_reset` sets it to 0 at the start of every line (and on
> `CC_FATAL`). Callers own all the arithmetic; `hist_get` only fetches what
> it is told to.
>
> **Branch A — `eventno == 0`: restore the user's own line.**
> 1. `wcsncpy(el->el_line.buffer, el->el_history.buf, el->el_history.sz)`.
>    This writes exactly `el_history.sz` elements: the saved line up to its
>    first NUL, then NUL padding out to `sz`. It depends on `el_history.sz`
>    never exceeding the line buffer's allocation, which `ch_enlargebufs`
>    and `hist_enlargebuf` between them guarantee.
> 2. `el->el_line.lastchar = el->el_line.buffer + (el->el_history.last -
>    el->el_history.buf)`. The restored length comes from the stash's
>    recorded offset, **not** from a `wcslen` of what was just copied. If
>    the saved line contained an embedded NUL, step 1 stopped copying there
>    and zeroed the remainder, yet `lastchar` is still placed at the full
>    original length — the tail returns as NULs.
> 3. Cursor: `el->el_line.buffer` when `el->el_map.type == MAP_VI`,
>    otherwise `el->el_line.lastchar`. `KSHVI` is unconditionally defined
>    in `el.h`, so this is live code, not a disabled branch: in vi mode
>    recall lands the cursor at column 0, in emacs mode at the end of the
>    line.
> 4. Return `CC_REFRESH`.
>
> Branch A consults neither `ref` nor `fun`, so restoring the in-progress
> line works even with no history installed at all.
>
> **Where the stash comes from, and the edit-then-move case.** `hist_get`
> never writes `el_history.buf`. Saving is the callers' job
> (`ed_prev_history`, `ed_search_prev_history`, `vi_to_history_line`) and
> all of them do it identically and **only while `eventno` is still 0** —
> that is, only on the first step away from the user's own line:
>
>     *el_line.lastchar = '\0';                  /* terminate the copy */
>     wcsncpy(el_history.buf, el_line.buffer, el_history.sz);
>     el_history.last = el_history.buf + (el_line.lastchar - el_line.buffer);
>
> The consequence, which is the behaviour to reproduce exactly: **edits
> made to a recalled entry are not saved anywhere.** Because the stash is
> written only at `eventno == 0`, moving from one recalled event to another
> discards whatever the user typed into the first one — the next `hist_get`
> simply overwrites the line buffer from the store, and the store is not
> modified either. Only the original in-progress line is preserved, and it
> reappears when the user walks forward to `eventno == 0`. There is exactly
> one slot; it is not a per-event undo stack.
>
> (One caller diverges: `vi_to_history_line` passes the literal `EL_BUFSIZ`
> as the copy length instead of `el_history.sz`. Once the buffers have
> grown past 1024, a longer line is stashed only in its first 1024
> characters while `last` still records the full length, so branch A later
> restores a line whose tail is stale stash content or NULs. The bug lives
> in that caller and is specified with it, but it is observed here.)
>
> **Branch B — `eventno != 0`: fetch from the history store.**
> 1. If `el->el_history.ref == NULL`, return `CC_ERROR`. Nothing is
>    modified. Note there is no guard on a negative `eventno`.
> 2. `hp = HIST_FIRST(el)` — move the store's traversal cursor to the most
>    recent entry and take its string. If that yields NULL — empty history,
>    or the implementation reported an error — return `CC_ERROR` with the
>    line buffer, `lastchar`, `cursor` and `eventno` all untouched.
> 3. Walk into the past: with `h` starting at 1, while `h < eventno`, take
>    `hp = HIST_NEXT(el)` and increment `h`. The first NULL jumps to the
>    failure epilogue below with `h` holding the ordinal of the last entry
>    that *was* reachable. Any `eventno` less than 1 (other than 0, handled
>    by branch A) makes the loop body never execute, leaving `hp` on event
>    1 — `hist_get` silently treats a negative `eventno` as 1 while leaving
>    the field negative.
> 4. `hlen = wcslen(hp) + 1` (characters, including the terminator).
>    `blen = el->el_line.limit - el->el_line.buffer` — the usable line
>    capacity, which is the allocation less the two reserved trailing
>    slots. If `hlen > blen`, call `ch_enlargebufs(el, hlen)`; if that
>    returns 0, jump to the failure epilogue. `hlen` is a total length
>    passed where `ch_enlargebufs` expects an *additional* length, so the
>    growth is conservative — it over-allocates, never under-allocates.
>    `ch_enlargebufs` moves `el_line.buffer`; `hp` points into the history
>    implementation's own storage, or into `el->el_scratch.wbuff` under
>    `NARROW_HISTORY`, neither of which it touches, so `hp` survives. It
>    does however invoke the application's `c_resizefun` callback, and a
>    callback that itself uses the history or the scratch buffer would
>    invalidate `hp` under the C's feet. The C does not guard against that
>    and this rule does not define the result.
> 5. `memcpy` `hlen` characters — the entry and its NUL — to
>    `el->el_line.buffer`, then set `el->el_line.lastchar =
>    el->el_line.buffer + hlen - 1`, i.e. onto the NUL.
> 6. Trim, in this fixed order, at most one character each:
>    - if `lastchar > buffer` and `lastchar[-1] == '\n'`, decrement
>      `lastchar`;
>    - then if `lastchar > buffer` and `lastchar[-1] == ' '`, decrement
>      `lastchar`.
>    The `lastchar > buffer` guards exist only to stop an empty entry
>    (`hlen == 1`, `lastchar == buffer`) from reading `buffer[-1]` and
>    driving `lastchar` before the buffer.
>    Only `lastchar` moves — **no NUL is written at the new position** — so
>    the trimmed characters and the original terminator are still sitting
>    in the buffer above `lastchar`. Consumers that need a terminated line
>    write `*lastchar = '\0'` themselves, which is what the history motion
>    commands in `common.c` do on entry. Everything above `lastchar` is
>    unspecified content and the port need not match it byte for byte.
>    Worked consequences of the fixed order: `"cmd \n"` → `"cmd"`;
>    `"cmd\n "` → `"cmd\n"`, the space goes and the newline stays *inside*
>    the line; `"cmd  "` → `"cmd "`, only one space; `"cmd\n\n"` →
>    `"cmd\n"`; `"\n"` → empty; `" "` → empty; `""` → empty.
> 7. Cursor: the same rule as branch A — `buffer` under `MAP_VI`,
>    `lastchar` otherwise.
> 8. Return `CC_REFRESH`.
>
> **Failure epilogue.** Set `el->el_history.eventno = h` and return
> `CC_ERROR`.
> - Entered from step 3, `h` is the ordinal of the oldest reachable entry,
>   so `eventno` is **clamped to the number of entries in the history** and
>   the line buffer is left untouched. This is the mechanism behind the
>   "eventno was fixed by the first call" idiom used throughout the editor:
>   a caller that asks for an out-of-range event — `ed_prev_history`
>   overshooting the end, `vi_to_history_line` passing `0x7fffffff` to mean
>   "oldest", `ce_inc_search` wrapping around — receives `CC_ERROR`, then
>   simply calls `hist_get` a second time and lands on the oldest entry. A
>   port that returns the error without writing `eventno` breaks every one
>   of those callers.
> - Entered from step 4, the walk completed normally so `h == eventno` and
>   the assignment is a no-op — except when `eventno < 1`, where `h` is 1
>   and `eventno` is silently rewritten to 1.
>
> **Return values.** `CC_REFRESH` (4) on both success paths. `CC_ERROR` (6)
> on all four failure paths: no history installed, empty or failing
> `HIST_FIRST`, the walk running off the oldest entry, and a failed buffer
> enlargement. No other value is ever produced. `el_action_t` is
> `unsigned char`.
>
> **Display.** `hist_get` writes nothing to the terminal itself. It returns
> `CC_REFRESH` and the editor's read loop turns that into `re_refresh(el)`
> — a full repaint of the line from `buffer` to `lastchar` with the
> hardware cursor placed at `el_line.cursor`. A full refresh rather than
> `CC_CURSOR` is required because recall replaces the entire line contents,
> not just the cursor position. `CC_ERROR` makes the read loop beep and
> flush. Callers layer on top of this: `ed_prev_history` turns a clamped
> error into `CC_REFRESH_BEEP` (repaint *and* beep) after its retry, and
> `ce_inc_search` calls `hist_get` for its own purposes and then overrides
> the cursor placement this function chose.
>
> **What invalidates the tracking.** `eventno` persists across commands
> within a line and is cleared to 0 only by `ch_reset`. It is *not* cleared
> by `hist_set`, so replacing the history implementation mid-line leaves a
> stale ordinal that will be resolved against the new store. It is *not*
> validated against the store's actual contents except by the clamp above,
> and the clamp only runs when the walk fails — so if the application adds
> or deletes entries behind the editor's back, an unchanged `eventno`
> silently denotes a different entry. Neither `cursor` nor `lastchar` is
> remembered per event: leaving a recalled entry and returning to it
> re-fetches from the store and re-derives both.
>
> One state leak worth naming: when the history is empty, step 2 returns
> `CC_ERROR` **without** resetting `eventno`, and `ed_prev_history` under
> the emacs keymap leaves it incremented. The editor then believes it is on
> event 1 while displaying the stashed line. The stash was already written,
> so a later `ed_next_history` down to 0 restores it — but anything the
> user typed in between is lost. The vi keymap escapes this because
> `ed_prev_history` restores its saved `eventno` on error when
> `el_map.type == MAP_VI`; the emacs keymap has no such restore.

> [spec:libedit:def:hist.hist-init-fn]
> libedit_private int hist_init(EditLine *el)

> [spec:libedit:sem:hist.hist-init-fn]
> Initialises `el->el_history`, the editor's bridge to the history
> subsystem. That struct holds two separate things: the handle to whatever
> history store the application installs, and the editor's own one-slot
> stash for the line the user was typing before they started walking
> through history.
>
> 1. `el->el_history.fun = NULL` and `el->el_history.ref = NULL` — no
>    history implementation attached yet. The two are always written
>    together by `hist_set`, and `ref == NULL` is the sentinel every other
>    function in this file tests to mean "no history".
> 2. Allocate the stash: `EL_BUFSIZ` (1024) `wchar_t`, zero-filled with
>    `calloc` semantics. On failure return -1 immediately, leaving `sz` and
>    `last` untouched — they are 0 and NULL because `el_init_internal`
>    allocates the whole `EditLine` zeroed.
> 3. `el->el_history.sz = EL_BUFSIZ`.
> 4. `el->el_history.last = el->el_history.buf` — the stash is empty, i.e.
>    a saved line of length 0. `last` is a pointer used purely to record a
>    length; `hist_get` reads it as the offset `last - buf`.
> 5. Return 0.
>
> Deliberately not touched here: `eventno` and the `ev` event cookie. Both
> begin zeroed with the `EditLine`, and `ch_reset` resets `eventno` to 0 at
> the start of every line. `eventno == 0` means "the line being edited is
> the user's own, not a recalled history entry".
>
> The zero fill is observable: before anything has ever been stashed, a
> `hist_get` at `eventno == 0` copies `sz` NULs into the line buffer, and
> `vi_to_history_line`'s short stash leaves the tail of the stash as these
> zeroes.
>
> The sole caller, `el_init_internal`, **discards the return value**, so an
> allocation failure here is not fatal and must not be made fatal by the
> port. The editor continues with `buf == NULL`, `sz == 0` and
> `last == NULL`. In that state `hist_get`'s `eventno == 0` branch copies
> zero characters and computes `last - buf` as `NULL - NULL` — universally
> 0 in practice, but not something the C standard defines — so it degrades
> to "restore an empty line"; and the first `ch_enlargebufs` calls
> `hist_enlargebuf`, which allocates from NULL and repairs the state. A
> port that propagates the error, or that panics, changes behaviour.

> [spec:libedit:def:hist.hist-set-fn]
> libedit_private int hist_set(EditLine *el, hist_fun_t fun, void *ptr)

> [spec:libedit:sem:hist.hist-set-fn]
> Installs the application's history implementation. Two stores and a
> constant return:
>
> 1. `el->el_history.ref = ptr` — the opaque handle passed as the first
>    argument of every subsequent history call.
> 2. `el->el_history.fun = fun` — the dispatcher, invoked as
>    `fun(ref, &ev, op, ...)` where `op` is an `H_*` code.
> 3. Return 0. There is no failure path, no validation of either argument,
>    and no inspection of what was there before.
>
> Not touched: `buf`, `sz`, `last`, `eventno` and `ev`. In particular
> `eventno` is not reset, so installing or replacing a history part-way
> through a line leaves an ordinal that referred to the old store; the next
> `hist_get` resolves it against the new one.
>
> `hist_set(el, fun, NULL)` is the supported way to detach a history:
> `ref == NULL` makes `hist_get` return `CC_ERROR` for every non-zero
> `eventno` and makes `hist_command` return -1, while branch A of
> `hist_get` (restoring the user's in-progress line) keeps working.
>
> `hist_set(el, NULL, ptr)` with a non-NULL `ptr` is accepted and is a
> loaded gun. Every guard elsewhere in this file tests `ref == NULL`, never
> `fun == NULL`, so the next history access performs a NULL indirect call.
> That is undefined behaviour and the C makes no attempt to prevent it; the
> port should reject the combination rather than define it.
>
> The `NARROW_HISTORY` flag — which decides whether event strings arrive as
> `wchar_t *` or as `char *` needing conversion by `hist_convert` — is
> **not** managed here. The narrow `el_set(EL_HIST, …)` sets it and the
> wide `el_wset(EL_HIST, …)` clears it when `MB_CUR_MAX == 1`, each around
> its own call to this function. A port must keep that split: `hist_set`
> itself is flag-agnostic and stores only the two fields above.
