# src/emacs.c

> [spec:libedit:def:emacs.em-capitol-case-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_capitol_case(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-capitol-case-fn]
> Capitalise from the cursor through the end of the word span selected by
> the repeat count. Bound to `M-c` and `M-C` in the default emacs keymap.
> The `c` parameter is unused.
>
> 1. Compute the end of the span:
>    `ep = c__next_word(el, cursor, lastchar, el->el_state.argument, ce__isword)`.
>    `c__next_word` repeats `argument` times; each repetition first advances
>    over characters for which `ce__isword` is false, then over characters
>    for which it is true, never passing `lastchar`; the result is clamped
>    up to `lastchar`. `ce__isword(ch)` is
>    `iswalnum(ch) || wcschr(el->el_map.wordchars, ch) != NULL`, and
>    `wordchars` defaults to `*?_-.[]~=` under the emacs keymap and `_`
>    under the vi keymap. `el->el_state.argument` is 1 unless a preceding
>    `CC_ARGHACK` command (`ed_digit`, `ed_argument_digit`,
>    `em_universal_argument`) accumulated a count; the read loop resets it
>    to 1 after this command returns. A count larger than the words
>    remaining is silently clamped to `lastchar`, not an error.
> 2. First pass over `[cursor, ep)`: scan forward, skipping characters that
>    are not `iswalpha` and leaving them untouched. At the first `iswalpha`
>    character, if it is `iswlower` replace it with `towupper` of itself
>    (an already-uppercase or caseless alphabetic is left alone); then
>    advance one past it and stop the pass. If the span contains no
>    alphabetic character, the pass ends having reached `ep`.
> 3. Second pass: over every remaining character up to `ep`, if it is
>    `iswupper` replace it with `towlower` of itself. Characters that are
>    neither upper- nor lowercase are left alone.
> 4. Set `cursor = ep`; then, if `cursor > lastchar`, set `cursor = lastchar`.
>    (The clamp is redundant — `c__next_word` already clamps — but it is
>    present in the C and harmless.)
> 5. Return `CC_REFRESH` on every path. There is no error return: when the
>    cursor is already at `lastchar`, `ep == cursor`, nothing is modified,
>    and `CC_REFRESH` is still returned.
>
> The mark, the kill buffer and `lastchar` are never touched. The
> case-mapping is the locale-sensitive wide-character mapping, applied
> per code point with no context and no multi-character expansions.

> [spec:libedit:def:emacs.em-copy-prev-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_copy_prev_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-copy-prev-word-fn]
> Insert a copy of the preceding word span at the cursor. Bound to `M-^_`
> (0237) in the default emacs keymap. The `c` parameter is unused.
>
> 1. If `cursor == buffer` there is nothing before the cursor: return
>    `CC_ERROR` with nothing modified.
> 2. Find the start of the span:
>    `cp = c__prev_word(el, cursor, buffer, el->el_state.argument, ce__isword)`.
>    `c__prev_word` steps one character back from `cursor` and then, `argument`
>    times, first skips backwards over characters for which `ce__isword` is
>    false and then backwards over characters for which it is true, never
>    going below `buffer`; it then steps forward one and clamps up to
>    `buffer`. The result is the first character of the `argument`-th
>    previous word. A repeat count larger than the words available is
>    clamped to `buffer`, not an error. `ce__isword(ch)` is
>    `iswalnum(ch) || wcschr(el->el_map.wordchars, ch) != NULL`.
> 3. Let `n = cursor - cp`. Call `c_insert(el, n)`: it grows the line, kill,
>    undo, redo and history buffers via `ch_enlargebufs` if
>    `lastchar + n >= limit`, then shifts `[cursor, lastchar]` right by `n`
>    and advances `lastchar` by `n`. Text before the cursor — including the
>    source span `[cp, cursor)` — is not moved, so `cp` stays valid.
> 4. Remember `oldc = cursor`. Copy characters forward from `cp` into
>    successive positions starting at `oldc`, continuing while the source
>    pointer is below `oldc` AND the destination pointer is below
>    `lastchar`. In the normal case this writes exactly the `n` characters
>    of `[cp, oldc)` into the gap just opened.
> 5. Set `cursor` to the destination pointer, i.e. `oldc + n`: the cursor
>    lands immediately after the inserted copy.
> 6. Return `CC_REFRESH`.
>
> Neither the mark nor the kill buffer is touched, and the source text is
> left in place — this copies, it does not move.
>
> Failure mode, present in the C and observable: `c_insert` returns
> silently without doing anything when `ch_enlargebufs` fails to allocate.
> The copy loop is not conditioned on that, so in that case it overwrites
> the characters that follow the cursor (stopping at the un-advanced
> `lastchar`) instead of inserting them, `lastchar` does not move, and the
> function still returns `CC_REFRESH` with no error indication.

> [spec:libedit:def:emacs.em-copy-region-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_copy_region(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-copy-region-fn]
> Copy the region between mark and cursor into the kill buffer without
> deleting it. Bound to `M-w` and `M-W` in the default emacs keymap. The
> `c` parameter is unused.
>
> 1. If `el->el_chared.c_kill.mark` is a NULL pointer, return `CC_ERROR`.
>    This branch is dead in practice: `ch_init` and `ch_reset` set the mark
>    to `el->el_line.buffer`, which is non-NULL, `ch_enlargebufs` relocates
>    it to the new buffer, and nothing ever assigns NULL to it. There is no
>    "mark unset" state to reach — model the mark as always set, starting
>    at the beginning of the line.
> 2. If `mark > cursor`, copy the characters of `[cursor, mark)` into
>    `el->el_chared.c_kill.buf` starting at index 0, and set
>    `el->el_chared.c_kill.last` to one past the last character written.
> 3. Otherwise (`mark <= cursor`), copy `[mark, cursor)` into
>    `c_kill.buf` starting at index 0 and set `c_kill.last` the same way.
> 4. Return `CC_NORM`: no redraw, no cursor reposition, no beep, no
>    message. The user gets no feedback that anything happened.
>
> The kill buffer is always rewritten from index 0. Consecutive kills and
> copies never append, in either direction — this is a divergence from GNU
> emacs, where successive kill commands accumulate into one kill-ring
> entry. Consequently copying an empty region (`mark == cursor`) sets
> `c_kill.last == c_kill.buf`, which *empties* the kill buffer; a following
> `em_yank` then returns `CC_NORM` and inserts nothing.
>
> Neither the cursor, the mark, the line contents nor `lastchar` is
> modified. `el->el_state.argument` is ignored. No bounds check is made on
> the copy and none is needed: `c_kill.buf` is allocated with the same
> element count as the line buffer and grown in lockstep with it by
> `ch_enlargebufs`, so a whole-line copy always fits.

> [spec:libedit:def:emacs.em-delete-next-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_delete_next_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-delete-next-word-fn]
> Kill from the cursor to the end of the word span selected by the repeat
> count. Bound to `M-d` and `M-D` in the default emacs keymap. The `c`
> parameter is unused.
>
> 1. If `cursor == lastchar`, return `CC_ERROR` immediately; the line, the
>    cursor and the kill buffer are all left as they were.
> 2. Compute the end of the span:
>    `cp = c__next_word(el, cursor, lastchar, el->el_state.argument, ce__isword)`
>    — `argument` repetitions of "skip non-word characters, then skip word
>    characters", never passing `lastchar`, result clamped to `lastchar`.
>    `ce__isword(ch)` is
>    `iswalnum(ch) || wcschr(el->el_map.wordchars, ch) != NULL`. A repeat
>    count exceeding the words remaining is clamped to `lastchar`, killing
>    the rest of the line rather than erroring.
> 3. Copy `[cursor, cp)` into `el->el_chared.c_kill.buf` starting at index
>    0, and set `el->el_chared.c_kill.last` to one past the last character
>    written. This OVERWRITES the previous kill-buffer contents —
>    consecutive `M-d` presses do not append (divergence from GNU emacs).
>    A zero-length span leaves `last == buf`, i.e. empties the kill buffer.
>    The copy is unbounded in the C but cannot overflow: `c_kill.buf` has
>    at least as many elements as the line buffer.
> 4. `c_delafter(el, cp - cursor)`: it first clamps the count to
>    `lastchar - cursor`; then, if `el->el_map.current != el->el_map.emacs`
>    (i.e. this function was bound into a non-emacs map), it records vi
>    undo state via `cv_undo` and copies the doomed text into the vi yank
>    buffer via `cv_yank`; then, if the count is positive, it shifts the
>    tail left over the deleted characters and decrements `lastchar` by the
>    clamped count.
> 5. Bounds check: if `cursor > lastchar`, set `cursor = lastchar`. The
>    cursor pointer is otherwise not moved by the kill, so it stays where
>    it was and now sits at the start of the text that followed the span.
> 6. Return `CC_REFRESH`.
>
> The mark is not adjusted even though the text under it has shifted left,
> so a mark that was to the right of the cursor is left designating a
> different logical position.

> [spec:libedit:def:emacs.em-delete-or-list-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_delete_or_list(EditLine *el, wint_t c)

> [spec:libedit:sem:emacs.em-delete-or-list-fn]
> Delete the character under the cursor, or signal EOF on an empty line.
> Bound to `^D` in the default emacs keymap. This function DOES use its
> `c` parameter (the key that invoked it).
>
> 1. If `cursor == lastchar` (nothing to the right of the cursor):
>    a. If also `cursor == buffer` — the line is completely empty — echo
>       the invoking key with `terminal_writec(el, c)`, which renders `c`
>       through `ct_visual_char` into its visible form (`^D` for the
>       default binding), writes that to the terminal and flushes; then
>       return `CC_EOF`. The line buffer is not modified.
>    b. Otherwise (end of a non-empty line): the "list completions"
>       behaviour named in the function's own comment header is NOT
>       implemented — the C says so explicitly. Ring the bell with
>       `terminal_beep(el)` and return `CC_ERROR`. Because the read loop
>       also beeps on `CC_ERROR`, this path beeps twice.
> 2. Otherwise (there is at least one character under/after the cursor):
>    a. If `el->el_state.doingarg` is set, call
>       `c_delafter(el, el->el_state.argument)`. That clamps the count to
>       `lastchar - cursor`; performs vi undo/yank bookkeeping (`cv_undo`,
>       `cv_yank`) when `el->el_map.current != el->el_map.emacs`; and then,
>       if the count is positive, shifts the tail left and decrements
>       `lastchar` by the clamped count. An over-large repeat count
>       therefore deletes to end of line and is not an error.
>    b. If `doingarg` is clear, call `c_delafter1(el)` instead, which
>       deletes exactly one character with no clamping and no vi
>       bookkeeping. The two paths differ observably under a vi keymap:
>       the no-argument path records neither undo nor yank state. (Note
>       `c_delafter1` copies from one position past `lastchar`, reading an
>       uninitialised element that lands above the new `lastchar`; the
>       allocation reserves slack for this, and the value is never
>       observable.)
>    c. Bounds check: if `cursor > lastchar`, set `cursor = lastchar`. The
>       cursor is otherwise not moved.
>    d. Return `CC_REFRESH`.
>
> Nothing is ever written to the kill buffer — this is a delete, not a
> kill — and the mark is never updated.

> [spec:libedit:def:emacs.em-delete-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_delete_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-delete-prev-char-fn]
> Delete backwards from the cursor. Bound to `^H` and `^?` (127) in the
> default emacs keymap. The `c` parameter is unused.
>
> 1. If `cursor <= buffer`, return `CC_ERROR` with nothing modified.
> 2. If `el->el_state.doingarg` is set, call
>    `c_delbefore(el, el->el_state.argument)`. That clamps the count to
>    `cursor - buffer`; then, if `el->el_map.current != el->el_map.emacs`,
>    records vi undo state (`cv_undo`) and copies the doomed text into the
>    vi yank buffer (`cv_yank`); then, if the count is positive, shifts the
>    tail left over the deleted characters and decrements `lastchar` by the
>    clamped count.
>    If `doingarg` is clear, call `c_delbefore1(el)` instead, which removes
>    exactly the one character before the cursor with no clamping and no vi
>    bookkeeping.
> 3. Move the cursor: `cursor -= el->el_state.argument`. This uses the
>    UNCLAMPED argument, unlike step 2. `argument` is 1 whenever `doingarg`
>    is clear (the read loop resets it after every command), so the
>    no-argument path moves exactly one position left.
> 4. If `cursor < buffer`, set `cursor = buffer`.
> 5. Return `CC_REFRESH`.
>
> Net effect of steps 2-4 for an over-large repeat count: everything from
> the start of the line up to the old cursor is deleted and the cursor
> lands at `buffer`. That is the behaviour to reproduce — but note the C
> gets there by forming `cursor - argument`, a pointer before the start of
> the object, which is undefined behaviour in C. The port must do the
> arithmetic saturating at `buffer` rather than relying on wraparound.
>
> The kill buffer is never written — a backspace is not a kill and cannot
> be yanked back. The mark is not adjusted even though text before it has
> shifted left.

> [spec:libedit:def:emacs.em-exchange-mark-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_exchange_mark(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-exchange-mark-fn]
> Swap point and mark. Not present in the keymap table; `map_init_emacs`
> registers it as the two-key macro `^X^X` via
> `keymacro_add(el, L"\030\030", EM_EXCHANGE_MARK, XK_CMD)`. The `c`
> parameter is unused.
>
> 1. Save the current `el->el_line.cursor`.
> 2. Set `el->el_line.cursor = el->el_chared.c_kill.mark`.
> 3. Set `el->el_chared.c_kill.mark` to the saved cursor value.
> 4. Return `CC_CURSOR` — the caller repositions the on-screen cursor only
>    (`re_refresh_cursor`); the line text is not redrawn.
>
> The exchange is unconditional and unvalidated: there is no NULL test, no
> clamp to `[buffer, lastchar]`, and no error return on any input.
> `el->el_state.argument` is ignored.
>
> Hazard to preserve: the mark is a raw pointer that is never adjusted when
> the line is edited. It always points inside the line-buffer allocation
> (it is only ever assigned a former cursor value, `buffer`, or its
> relocation by `ch_enlargebufs`), but it can point ABOVE `lastchar` —
> most easily after `em_kill_line`, which sets `lastchar = buffer` without
> touching the mark. Swapping then leaves the cursor beyond `lastchar`,
> an incoherent editor state that the rest of libedit does not defend
> against. The port must reproduce the swap, including this case, without
> introducing a clamp that the C does not have.

> [spec:libedit:def:emacs.em-gosmacs-transpose-fn]
> libedit_private el_action_t em_gosmacs_transpose(EditLine *el, wint_t c)

> [spec:libedit:sem:emacs.em-gosmacs-transpose-fn]
> Gosling-emacs style transpose-characters: exchange the two characters
> immediately before the cursor. NOT bound in the default emacs keymap
> (`^T` there is `ED_TRANSPOSE_CHARS`, a different function), so this is
> reachable only through an explicit user binding. The `c` parameter is
> declared as the invoking key but is used purely as a scratch variable
> for the swap; its incoming value is discarded.
>
> 1. If `cursor > &buffer[1]` — that is, `cursor >= buffer + 2`, so at
>    least two characters exist before the cursor — swap them:
>    `tmp = cursor[-2]; cursor[-2] = cursor[-1]; cursor[-1] = tmp`.
>    Return `CC_REFRESH`.
> 2. Otherwise (`cursor` is at `buffer` or at `buffer + 1`) return
>    `CC_ERROR` with nothing modified.
>
> The cursor does NOT move — a divergence from GNU emacs `transpose-chars`,
> which advances point past the transposed pair. `el->el_state.argument` is
> ignored entirely: a repeat count neither repeats the swap nor widens it.
> The kill buffer, the mark and `lastchar` are untouched.

> [spec:libedit:def:emacs.em-inc-search-next-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_inc_search_next(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-inc-search-next-fn]
> Start a fresh emacs-style incremental search forwards through history.
> NOT bound in the default emacs keymap (`^S` is `ED_IGNORE` there, since
> `^S` is normally consumed by tty flow control) — only
> `EM_INC_SEARCH_PREV` is bound, at `^R`. Reachable by explicit binding,
> and reachable from inside an in-progress incremental search, where a key
> bound to this command switches the search direction to forwards. The `c`
> parameter is unused.
>
> 1. Set `el->el_search.patlen = 0`. This discards the length of any
>    pattern left over from a previous incremental search so the helper
>    takes its "first round" path and starts from an empty pattern. Only
>    the length is reset; `el->el_search.patbuf` itself is not cleared.
> 2. Return, unchanged, whatever `ce_inc_search(el, ED_SEARCH_NEXT_HISTORY)`
>    returns. There is no post-processing and no other side effect here.
>
> `ce_inc_search` runs its own read loop and is specified separately; its
> possible return values, which this function propagates verbatim, are
> `CC_ERROR` (immediately, when the line buffer has no room for the search
> prompt plus pattern; or on `^G` abort; or on a failed search that had no
> prior successful pattern), `CC_NORM` (search abandoned or unwound, with
> history event number, pattern length and cursor restored),
> `CC_REFRESH` (terminated by `ESC`, or by any other key, which is pushed
> back onto the input so it is re-read and executed as the next command),
> and `CC_EOF` (via `ed_end_of_file`) if the input stream ends mid-search.
>
> `el->el_state.argument` is ignored; there is no repeat-count behaviour.
> The kill buffer and mark are untouched by this function.

> [spec:libedit:def:emacs.em-inc-search-prev-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_inc_search_prev(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-inc-search-prev-fn]
> Start a fresh emacs-style incremental search backwards through history.
> Bound to `^R` in the default emacs keymap. Also reachable from inside an
> in-progress incremental search, where a key bound to this command
> switches the search direction to backwards. The `c` parameter is unused.
>
> Identical to `em_inc_search_next` except for the direction passed on:
>
> 1. Set `el->el_search.patlen = 0`, discarding the length of any pattern
>    left over from a previous incremental search so the helper takes its
>    "first round" path and starts from an empty pattern.
>    `el->el_search.patbuf` itself is not cleared.
> 2. Return, unchanged, whatever `ce_inc_search(el, ED_SEARCH_PREV_HISTORY)`
>    returns — `CC_ERROR`, `CC_NORM`, `CC_REFRESH` or `CC_EOF`, with the
>    meanings given under `em_inc_search_next`. There is no
>    post-processing and no other side effect.
>
> `el->el_state.argument` is ignored. The kill buffer and mark are
> untouched by this function.

> [spec:libedit:def:emacs.em-kill-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_kill_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-kill-line-fn]
> Cut the ENTIRE line — not just to end of line, and not just before the
> cursor — into the kill buffer and clear it. Bound to `^U` in the default
> emacs keymap, and installed by `tty_bind_char` as the binding for the
> tty "kill" character. The `c` parameter is unused.
>
> Two divergences from GNU emacs worth stating up front: `^U` there is
> `universal-argument` (libedit's `EM_UNIVERSAL_ARGUMENT` is not bound at
> all by default), and emacs's `kill-line` (`^K`, here `ED_KILL_LINE`)
> kills only from point to end of line. This function kills the whole
> line regardless of where the cursor is.
>
> 1. Copy the whole current line, `[buffer, lastchar)`, into
>    `el->el_chared.c_kill.buf` starting at index 0, and set
>    `el->el_chared.c_kill.last` to one past the last character copied.
>    The previous kill-buffer contents are overwritten; nothing is
>    appended in either direction. An already-empty line yields
>    `last == buf`, i.e. it EMPTIES the kill buffer, after which `em_yank`
>    returns `CC_NORM` and inserts nothing. No bounds check is made and
>    none is needed: `c_kill.buf` is allocated with the same element count
>    as the line buffer and grown in lockstep with it by `ch_enlargebufs`.
> 2. Discard the line by pointer assignment only: set
>    `el->el_line.lastchar = el->el_line.buffer` and then
>    `el->el_line.cursor = el->el_line.buffer`. No chared.c helper is
>    used — `c_delbefore` / `c_delafter` are NOT called — so no vi undo
>    state is recorded and no vi yank buffer is filled, even when this
>    function has been bound into a vi keymap. The old characters remain
>    in the buffer storage above the new `lastchar`; only the length
>    pointer changes.
> 3. Return `CC_REFRESH`.
>
> `el->el_state.argument` is ignored entirely — a repeat count changes
> nothing, and there is no error path: the function returns `CC_REFRESH`
> even for an already-empty line.
>
> Hazard to preserve: `el->el_chared.c_kill.mark` is NOT reset. Any mark
> that was above `buffer` is left pointing above the new `lastchar`, so a
> subsequent `em_kill_region`, `em_copy_region` or `em_exchange_mark` acts
> on a position outside the live line. The port must leave the mark alone
> here rather than "fixing" it.

> [spec:libedit:def:emacs.em-kill-region-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_kill_region(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-kill-region-fn]
> Cut the region between mark and cursor into the kill buffer. NOT bound
> in the default emacs keymap — `^W` there is `ED_DELETE_PREV_WORD` and
> `M-^W` is unassigned — so this is reachable only through an explicit
> user binding. The `c` parameter is unused.
>
> 1. If `el->el_chared.c_kill.mark` is a NULL pointer, return `CC_ERROR`.
>    This branch is dead in practice: `ch_init` and `ch_reset` set the mark
>    to `el->el_line.buffer` (non-NULL), `ch_enlargebufs` relocates it, and
>    nothing ever assigns NULL. The intended "no mark set, refuse to kill"
>    guard cannot fire — model the mark as always set, defaulting to the
>    start of the line, so an unset mark behaves as a mark at `buffer`.
> 2. If `mark > cursor` (region lies to the right of the cursor):
>    - Copy `[cursor, mark)` into `el->el_chared.c_kill.buf` starting at
>      index 0 and set `el->el_chared.c_kill.last` to one past the last
>      character copied.
>    - `c_delafter(el, mark - cursor)`: clamps the count to
>      `lastchar - cursor`; performs vi undo/yank bookkeeping (`cv_undo`,
>      `cv_yank`) when `el->el_map.current != el->el_map.emacs`; then
>      shifts the tail left and decrements `lastchar`.
>    - The cursor is NOT moved; it now sits at the start of the text that
>      followed the region.
> 3. Otherwise (`mark <= cursor`, region lies to the left of or at the
>    cursor):
>    - Copy `[mark, cursor)` into `c_kill.buf` starting at index 0 and set
>      `c_kill.last` to one past the last character copied.
>    - `c_delbefore(el, cursor - mark)`: clamps the count to
>      `cursor - buffer`; performs the same vi bookkeeping when
>      applicable; then shifts the tail left over the region and
>      decrements `lastchar`.
>    - Set `cursor = mark`: the cursor lands at the old mark, i.e. at the
>      start of where the removed text was.
> 4. Return `CC_REFRESH` from both branches.
>
> `mark == cursor` falls into branch 3: zero characters are copied, so
> `c_kill.last` becomes `c_kill.buf` and the kill buffer is EMPTIED;
> `c_delbefore(el, 0)` deletes nothing; the cursor does not move; and
> `CC_REFRESH` is still returned. Killing an empty region is therefore a
> destructive no-op on the kill buffer, not an error and not a beep.
>
> The kill buffer is always rewritten from index 0 — consecutive kills
> never accumulate, in either direction (divergence from GNU emacs). The
> mark keeps its old pointer value in both branches and is not adjusted
> for the text that just moved; in branch 3 that leaves `mark == cursor`,
> so an immediately repeated kill-region empties the kill buffer.
> `el->el_state.argument` is ignored.

> [spec:libedit:def:emacs.em-lower-case-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_lower_case(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-lower-case-fn]
> Lowercase from the cursor through the end of the word span selected by
> the repeat count. Bound to `M-l` and `M-L` in the default emacs keymap.
> The `c` parameter is unused.
>
> 1. Compute the end of the span:
>    `ep = c__next_word(el, cursor, lastchar, el->el_state.argument, ce__isword)`
>    — `argument` repetitions of "skip characters for which `ce__isword` is
>    false, then skip characters for which it is true", never passing
>    `lastchar`, result clamped up to `lastchar`. `ce__isword(ch)` is
>    `iswalnum(ch) || wcschr(el->el_map.wordchars, ch) != NULL`;
>    `wordchars` defaults to `*?_-.[]~=` under the emacs keymap and `_`
>    under the vi keymap. `el->el_state.argument` is 1 unless a preceding
>    `CC_ARGHACK` command accumulated a count; the read loop resets it to 1
>    after this command returns. An over-large count is clamped to
>    `lastchar`, not an error.
> 2. For every character in `[cursor, ep)`: if it is `iswupper`, replace it
>    with `towlower` of itself. Everything else — including caseless and
>    already-lowercase characters, and the non-word characters skipped over
>    by the span computation — is left byte-identical.
> 3. Set `cursor = ep`; then, if `cursor > lastchar`, set
>    `cursor = lastchar` (a redundant re-clamp, present in the C).
> 4. Return `CC_REFRESH` on every path. There is no error return: with the
>    cursor already at `lastchar` the span is empty, nothing changes, and
>    `CC_REFRESH` is still returned.
>
> The mark, the kill buffer and `lastchar` are never touched. Case mapping
> is the locale-sensitive per-code-point `towlower`, with no context
> sensitivity and no length-changing expansions.

> [spec:libedit:def:emacs.em-meta-next-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_meta_next(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-meta-next-fn]
> Set the meta prefix so the next key typed is treated as if its 8th bit
> were set. Bound to `ESC` (`^[`, 27) in both the default emacs and the
> default vi command keymaps. The `c` parameter is unused.
>
> 1. Set `el->el_state.metanext = 1`.
> 2. Return `CC_ARGHACK`.
>
> Two consequences of `CC_ARGHACK` that are load-bearing here. First, the
> read loop treats it as "keep going": it performs no redraw, no beep and
> no cursor reposition, and immediately reads the next key. Second, it
> `continue`s past the loop's post-command reset, so
> `el->el_state.argument`, `el->el_state.doingarg` and the pending vi
> action (`el->el_chared.c_vcmd.action`) are all left as they were — this
> is what allows a repeat count typed before `ESC` to survive into the
> command that `ESC` prefixes.
>
> The prefix is consumed by the key reader, not by this function: on the
> next character read, `read_getcmd` clears `metanext` back to 0 and ORs
> `0x80` into the character before looking it up in the current keymap.
> The upper half of the 256-entry map (indices 128..255) is where the
> `M-x` bindings live, so `ESC f` and the single byte 0346 dispatch
> identically. Characters at or above `N_KEYS` (256) bypass the map
> entirely and are inserted, so the meta bit only reaches codes below it.
>
> This function neither reads nor writes `el->el_state.argument`, the kill
> buffer, the mark or the line.

> [spec:libedit:def:emacs.em-next-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_next_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-next-word-fn]
> Move the cursor forward past the end of the word span selected by the
> repeat count. Bound to `M-f` and `M-F` in the default emacs keymap. The
> `c` parameter is unused.
>
> 1. If `cursor == lastchar`, return `CC_ERROR` with nothing modified.
> 2. Set
>    `cursor = c__next_word(el, cursor, lastchar, el->el_state.argument, ce__isword)`.
>    `c__next_word` repeats `argument` times; each repetition first
>    advances over characters for which `ce__isword` is false, then over
>    characters for which it is true, stopping at `lastchar`; the result is
>    clamped up to `lastchar`. The cursor therefore lands just past the end
>    of the `argument`-th word ahead, or at `lastchar` when the count
>    exceeds the words remaining — an over-large repeat count is silently
>    clamped, never an error. `ce__isword(ch)` is
>    `iswalnum(ch) || wcschr(el->el_map.wordchars, ch) != NULL`;
>    `el->el_map.wordchars` is set to `*?_-.[]~=` by `map_init_emacs` and
>    to `_` by `map_init_vi`, and can be changed by the caller.
>    `el->el_state.argument` is 1 unless a preceding `CC_ARGHACK` command
>    accumulated a count; the read loop resets it to 1 afterwards.
> 3. If `el->el_map.type == MAP_VI` AND `el->el_chared.c_vcmd.action != NOP`
>    — that is, the editor is in vi mode and a vi operator such as `d`, `c`
>    or `y` is pending and this motion is its target — call
>    `cv_delfini(el)` to apply that operator over the span between
>    `el->el_chared.c_vcmd.pos` and the new cursor position, then return
>    `CC_REFRESH`.
> 4. Otherwise return `CC_CURSOR`: the caller repositions the on-screen
>    cursor only and does not redraw the line.
>
> Note the test in step 3 is on the keymap *type*, not on which map is
> currently active, so this emacs function participates in vi operator
> completion whenever the editor has been put in vi mode. The kill buffer,
> the mark and the line contents are never modified by this function
> itself (step 3 delegates any mutation to `cv_delfini`).

> [spec:libedit:def:emacs.em-set-mark-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_set_mark(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-set-mark-fn]
> Set the mark at the current cursor position. Bound to `^@` (NUL, index
> 0) in the default emacs keymap. The `c` parameter is unused.
>
> 1. Set `el->el_chared.c_kill.mark = el->el_line.cursor`.
> 2. Return `CC_NORM` — no redraw, no cursor reposition, no beep and no
>    "Mark set" style message. There is no error path and no condition
>    under which the assignment is skipped.
>
> `el->el_state.argument` is ignored. There is no mark ring and no
> previous mark is preserved: the single mark slot is overwritten.
>
> Properties of the mark that the port must preserve. It is a raw pointer
> into the line-buffer allocation, not an offset. `ch_init` and `ch_reset`
> set it to `el->el_line.buffer`, so it is never NULL and defaults to the
> start of the line — which is why the NULL guards in `em_kill_region` and
> `em_copy_region` never fire. `ch_enlargebufs` relocates it onto the
> reallocated line buffer. Nothing else ever adjusts it: inserting or
> deleting text moves characters out from under the mark without updating
> it, and `em_kill_line` can leave it above `lastchar`. `em_yank` also
> assigns the mark, to the start of the text it inserts.

> [spec:libedit:def:emacs.em-toggle-overwrite-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_toggle_overwrite(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-toggle-overwrite-fn]
> Toggle between insert and overwrite input modes. NOT bound in the
> default emacs or vi keymaps; reachable only through an explicit user
> binding (typically to a terminal's Insert key). The `c` parameter is
> unused.
>
> 1. If `el->el_state.inputmode == MODE_INSERT` (0), set
>    `el->el_state.inputmode = MODE_REPLACE` (1); otherwise set it to
>    `MODE_INSERT`. "Otherwise" includes `MODE_REPLACE_1` (2), the vi
>    single-character-replace state, which is therefore folded back to
>    `MODE_INSERT` rather than to `MODE_REPLACE`.
> 2. Return `CC_NORM` — no redraw, no cursor reposition, no beep, and no
>    visible indication that the mode changed.
>
> There is no error path. `el->el_state.argument` is ignored: the toggle
> happens exactly once regardless of any repeat count, so an even count
> does not cancel out.
>
> The mode is consumed by the insert path (`ed_insert`), not here. It does
> not persist across lines: `ch_init` and `ch_reset` both set
> `inputmode = MODE_INSERT`. The kill buffer, the mark, the cursor and the
> line contents are untouched.

> [spec:libedit:def:emacs.em-universal-argument-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_universal_argument(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-universal-argument-fn]
> Multiply the pending repeat count by 4. GNU emacs binds this to `^U`;
> libedit does NOT bind it anywhere in the default emacs or vi keymaps
> (`^U` is `EM_KILL_LINE` here), so it is reachable only through an
> explicit user binding. The `c` parameter is unused.
>
> 1. If `el->el_state.argument > 1000000`, return `CC_ERROR` and change
>    nothing — not `doingarg`, not `argument`. Note the test is made
>    BEFORE the multiplication, so the argument can legitimately be driven
>    up to 4000000 (4 × 1000000) through this function; the constant is an
>    overflow guard, not a user-facing maximum.
> 2. Set `el->el_state.doingarg = 1`.
> 3. Set `el->el_state.argument *= 4`. The multiplication is
>    unconditional — it is not skipped when `doingarg` was previously
>    clear — so the first invocation always turns the reset value 1 into 4.
> 4. Return `CC_ARGHACK`.
>
> `CC_ARGHACK` makes the read loop `continue`: no redraw, no beep, and —
> critically — it skips the loop's post-command reset of
> `el->el_state.argument` to 1, `doingarg` to 0 and
> `el->el_chared.c_vcmd.action` to `NOP`. That is what lets the count
> accumulate across successive invocations: 4, 16, 64, 256, ...
>
> The read loop records this command in `el->el_state.lastcmd` before the
> next key is dispatched, and `ed_digit` keys on that: a digit typed
> immediately after this command REPLACES the argument with that digit
> (`argument = c - '0'`) rather than appending a decimal place to it.
>
> The kill buffer, the mark, the cursor and the line contents are
> untouched.

> [spec:libedit:def:emacs.em-upper-case-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_upper_case(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-upper-case-fn]
> Uppercase from the cursor through the end of the word span selected by
> the repeat count. Bound to `M-u` and `M-U` in the default emacs keymap.
> The `c` parameter is unused.
>
> 1. Compute the end of the span:
>    `ep = c__next_word(el, cursor, lastchar, el->el_state.argument, ce__isword)`
>    — `argument` repetitions of "skip characters for which `ce__isword` is
>    false, then skip characters for which it is true", never passing
>    `lastchar`, result clamped up to `lastchar`. `ce__isword(ch)` is
>    `iswalnum(ch) || wcschr(el->el_map.wordchars, ch) != NULL`;
>    `wordchars` defaults to `*?_-.[]~=` under the emacs keymap and `_`
>    under the vi keymap. `el->el_state.argument` is 1 unless a preceding
>    `CC_ARGHACK` command accumulated a count; the read loop resets it to 1
>    after this command returns. An over-large count is clamped to
>    `lastchar`, not an error.
> 2. For every character in `[cursor, ep)`: if it is `iswlower`, replace it
>    with `towupper` of itself. Everything else — including caseless and
>    already-uppercase characters, and the non-word characters skipped over
>    by the span computation — is left byte-identical.
> 3. Set `cursor = ep`; then, if `cursor > lastchar`, set
>    `cursor = lastchar` (a redundant re-clamp, present in the C).
> 4. Return `CC_REFRESH` on every path. There is no error return: with the
>    cursor already at `lastchar` the span is empty, nothing changes, and
>    `CC_REFRESH` is still returned.
>
> The mark, the kill buffer and `lastchar` are never touched. Case mapping
> is the locale-sensitive per-code-point `towupper`, with no context
> sensitivity and no length-changing expansions.

> [spec:libedit:def:emacs.em-yank-fn]
> libedit_private el_action_t /*ARGSUSED*/ em_yank(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:emacs.em-yank-fn]
> Paste the kill buffer at the cursor. Bound to `^Y` in the default emacs
> keymap. The `c` parameter is unused.
>
> 1. If `el->el_chared.c_kill.last == el->el_chared.c_kill.buf` — the kill
>    buffer is empty — return `CC_NORM`. Silently: no beep, no error, no
>    redraw. Note that an empty kill buffer is reachable not only at
>    startup but by killing a zero-length span (see `em_kill_region`,
>    `em_copy_region`, `em_delete_next_word`, `em_kill_line`), because
>    every kill rewrites the buffer from index 0.
> 2. Let `n = c_kill.last - c_kill.buf`. If `el->el_line.lastchar + n >=
>    el->el_line.limit`, return `CC_ERROR` with nothing modified. The
>    comparison is `>=`, so a yank that would land exactly on `limit` is
>    refused. The line buffer is NOT grown to make room — this is a hard
>    refusal, not an enlargement attempt.
> 3. Set `el->el_chared.c_kill.mark = el->el_line.cursor`, before any text
>    moves. Because the insertion happens at the cursor, the mark ends up
>    at the START of the yanked text, so a following `em_exchange_mark` or
>    `em_kill_region` brackets exactly what was just pasted. Any previous
>    mark is destroyed.
> 4. `c_insert(el, n)` opens `n` characters of space at the cursor: it
>    shifts `[cursor, lastchar]` right by `n` and advances `lastchar` by
>    `n`. Its `ch_enlargebufs` path cannot be reached from here, because
>    step 2 applied the identical `lastchar + n >= limit` test first and
>    already returned `CC_ERROR`.
> 5. Copy all `n` characters of the kill buffer into the gap, starting at
>    the cursor position. The kill buffer itself is left unchanged, so
>    repeated yanks paste the same text.
> 6. Cursor placement, driven by the repeat count: if
>    `el->el_state.argument == 1` — which is the case whenever no numeric
>    argument was given, since the read loop resets `argument` to 1 after
>    every command — set the cursor to one past the last character
>    written, i.e. to the END of the yanked text. Otherwise (any
>    `argument` other than 1) leave the cursor where it was, i.e. at the
>    BEGINNING of the yanked text.
> 7. Return `CC_REFRESH`.
>
> The repeat count therefore only chooses where the cursor lands; it does
> NOT paste multiple copies. This is a divergence from GNU emacs, where a
> numeric argument to `C-y` repeats the yank. An explicit argument of 1 is
> indistinguishable from no argument at all, so there is no way to ask for
> "cursor at the beginning" with a count of one.
