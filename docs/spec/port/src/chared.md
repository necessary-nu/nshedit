# src/chared.c, src/chared.h

> [spec:libedit:def:chared.c-delafter-fn]
> libedit_private void c_delafter(EditLine *el, int num)

> [spec:libedit:sem:chared.c-delafter-fn]
> Deletes up to `num` characters starting at `el->el_line.cursor`,
> sliding the rest of the line left over them.
>
> 1. Clamp: if `cursor + num > lastchar`, set
>    `num = lastchar - cursor`, so at most the characters from the
>    cursor to the end of the line are removed. There is no clamp from
>    below; a negative `num` passes through unchanged.
> 2. If `el->el_map.current != el->el_map.emacs`, call `cv_undo(el)` to
>    snapshot the whole line for vi undo, then
>    `cv_yank(el, cursor, num)` to copy the characters about to be
>    deleted into the kill buffer. This runs after the clamp, before
>    any mutation, and it runs even when `num` is 0 (leaving the kill
>    buffer empty, `c_kill.last == c_kill.buf`).
>    IMPORTANT: that condition reads as "not in emacs mode" but is not.
>    `el_map.current` is only ever assigned `el_map.key` or
>    `el_map.alt`, both heap copies made by `map_init`, whereas
>    `el_map.emacs` is the static const default emacs table. The two
>    pointers are never equal, so the test is a tautology and the undo
>    snapshot plus kill-buffer write happen on EVERY call, in emacs
>    mode as much as in vi. The port must reproduce the observable
>    effect (always yank, always snapshot) — see the note on
>    `c_delbefore`, which has the identical test.
> 3. If `num > 0`, close the gap: for `cp` running from `cursor` up to
>    and including `lastchar`, assign `*cp = cp[num]`; then
>    `lastchar -= num`.
> 4. `el->el_line.cursor` is not moved, so the character formerly at
>    `cursor + num` is now under the cursor. Nothing is returned.
>
> The observable effect of step 3 is that `[cursor + num, lastchar]` is
> copied down onto `[cursor, lastchar - num]`. The loop then runs `num`
> further iterations whose writes all land above the new `lastchar` and
> are therefore unobservable — but whose reads reach as far as
> `lastchar + num`. For a large `num` on a nearly full line that is past
> the end of the line-buffer allocation, an out-of-bounds read in the C
> (worst case, deleting a full-capacity line from offset 0 reads roughly
> one whole buffer past the end). The port must copy only the in-range
> tail; the contents left above the new `lastchar` are unspecified.
>
> A negative `num` survives step 1 and is rejected by step 3, but step 2
> has already handed it to `cv_yank`, whose length is computed as an
> unsigned value — undefined behaviour. No caller passes one.

> [spec:libedit:def:chared.c-delafter1-fn]
> libedit_private void c_delafter1(EditLine *el)

> [spec:libedit:sem:chared.c-delafter1-fn]
> Deletes exactly one character at the cursor, saving it nowhere. There
> are no guards of any kind.
>
> 1. For `cp` running from `el->el_line.cursor` up to and including
>    `el->el_line.lastchar`, assign `*cp = cp[1]`.
> 2. `el->el_line.lastchar -= 1`.
>
> No undo snapshot is taken and the kill buffer is untouched, which is
> the whole difference from `c_delafter(el, 1)`. `el->el_line.cursor` is
> not moved and nothing is returned.
>
> The observable effect is that `[cursor + 1, lastchar]` is copied down
> onto `[cursor, lastchar - 1]`; the final iteration writes at
> `lastchar`, above the new end, so that slot's value is unspecified.
> The read of `lastchar[1]` stays inside the allocation, because the
> line buffer always keeps two unused trailing slots (`lastchar` never
> exceeds `limit`, and `limit == buffer + capacity - 2`).
>
> The caller must guarantee `cursor < lastchar`. If `cursor == lastchar`
> the loop still executes once and `lastchar` still decrements, leaving
> `lastchar == cursor - 1`, i.e. a cursor past the end of the line; if
> the line is empty as well (`cursor == lastchar == buffer`) `lastchar`
> ends up before `buffer` entirely. The C defends against neither.

> [spec:libedit:def:chared.c-delbefore-fn]
> libedit_private void c_delbefore(EditLine *el, int num)

> [spec:libedit:sem:chared.c-delbefore-fn]
> Deletes up to `num` characters immediately before the cursor.
>
> 1. Clamp: if `cursor - num < buffer`, set `num = cursor - buffer`, so
>    at most everything from the start of the line up to the cursor is
>    removed. There is no clamp from below.
> 2. If `el->el_map.current != el->el_map.emacs`, call `cv_undo(el)`
>    then `cv_yank(el, cursor - num, num)`, copying the characters about
>    to be deleted into the kill buffer. As in `c_delafter` this runs
>    after the clamp, before any mutation, and even when `num` is 0.
>    The same tautology applies: `el_map.current` is always `el_map.key`
>    or `el_map.alt` (heap copies) and never the static `el_map.emacs`
>    table, so the condition is always true and the snapshot plus yank
>    happen on every call regardless of editing mode.
> 3. If `num > 0`: starting at `cp = cursor - num`, while
>    `cp + num <= lastchar`, assign `*cp = cp[num]` and advance `cp`;
>    then `lastchar -= num`. This moves `lastchar - cursor + 1`
>    characters — the range `[cursor, lastchar]`, including the slot at
>    `lastchar` itself — down onto `[cursor - num, lastchar - num]`. The
>    loop bound keeps every read at or below `lastchar`, so unlike
>    `c_delafter` there is no out-of-range access here.
> 4. `el->el_line.cursor` is deliberately NOT adjusted. The text has
>    slid left underneath it, so the caller must do `cursor -= num`
>    itself, using the same clamped `num`, if the cursor is to stay on
>    the same character. `el_deletestr` and `cv_delfini` both do exactly
>    that. Nothing is returned.
>
> A negative `num` survives step 1, is rejected by step 3, but has
> already been passed to `cv_yank` in step 2, whose length is computed
> unsigned — undefined behaviour. No caller passes one.

> [spec:libedit:def:chared.c-delbefore1-fn]
> libedit_private void c_delbefore1(EditLine *el)

> [spec:libedit:sem:chared.c-delbefore1-fn]
> Deletes exactly one character immediately before the cursor, saving it
> nowhere. There are no guards.
>
> 1. For `cp` running from `el->el_line.cursor - 1` up to and including
>    `el->el_line.lastchar`, assign `*cp = cp[1]`.
> 2. `el->el_line.lastchar -= 1`.
>
> No undo snapshot, no kill-buffer write — that is the difference from
> `c_delbefore(el, 1)`. `el->el_line.cursor` is not adjusted, so the
> caller must decrement it itself to stay on the same character.
> Nothing is returned.
>
> The observable effect is that `[cursor, lastchar]` is copied down onto
> `[cursor - 1, lastchar - 1]`; the final iteration writes at
> `lastchar`, above the new end, so that slot is unspecified. The read
> of `lastchar[1]` is in-allocation because of the two reserved trailing
> slots.
>
> The caller must guarantee `cursor > buffer`. If `cursor == buffer` the
> loop starts at `buffer - 1`: merely forming that pointer is undefined
> in C, and the first assignment writes one element before the start of
> the line buffer.

> [spec:libedit:def:chared.c-gets-fn]
> libedit_private int c_gets(EditLine *el, wchar_t *buf, const wchar_t *prompt)

> [spec:libedit:sem:chared.c-gets-fn]
> Reads a short line of text from the terminal into `buf`, using the
> edit line buffer as scratch display space. Backs the `: ` extended
> command prompt and the incremental-search pattern prompt.
>
> Setup:
> 1. `cp = el->el_line.buffer`.
> 2. If `prompt` is non-NULL, copy its `wcslen(prompt)` characters to
>    the START of the line buffer and advance `cp` past them. This
>    destroys whatever the user was editing; the line is cleared again
>    on exit.
> 3. `len = 0` (an `ssize_t`).
>
> Then loop indefinitely:
> 4. Set `el->el_line.cursor = cp`, store `L' '` at `*cp`, set
>    `el->el_line.lastchar = cp + 1`, and call `re_refresh(el)`. The
>    space is the blank the cursor sits on while typing.
> 5. Call `el_wgetc(el, &ch)`. If it returns anything other than exactly
>    1 (EOF or read error), call `ed_end_of_file(el, 0)`, set
>    `len = -1`, and leave the loop.
> 6. Dispatch on `ch`:
>    - `L'\b'` (0x08) or `0177` (DEL): if `len == 0`, set `len = -1` and
>      leave the loop — backspacing on empty input aborts the whole
>      read. Otherwise decrement both `len` and `cp` and continue at
>      step 4; the erased character stays in `buf` above `len` (it is
>      simply no longer counted) and in the line buffer it is
>      overwritten by the cursor space on the next iteration.
>    - `0033` (ESC), `L'\r'` or `L'\n'`: store the terminator itself at
>      `buf[len]` WITHOUT incrementing `len`, and leave the loop.
>    - anything else: if `len >= EL_BUFSIZ - 16` (1008), call
>      `terminal_beep(el)` and discard the character; otherwise store it
>      at `buf[len++]` and also at `*cp++`. Either way continue at
>      step 4.
>
> On every exit path, unconditionally: `el->el_line.buffer[0] = L'\0'`,
> `el->el_line.lastchar = el->el_line.buffer`,
> `el->el_line.cursor = el->el_line.buffer`. Only the first cell is
> cleared; the prompt and typed text remain in the buffer above
> `lastchar`.
>
> Returns `len` cast to `int`: the number of characters written to
> `buf`, not counting the terminator left at `buf[len]`; or `-1` on EOF,
> read error, or backspace on empty input. `buf` is never NUL-terminated
> here — callers overwrite `buf[len]` with `L'\0'` themselves, which
> also discards the stored terminator character.
>
> `buf` must have room for at least `EL_BUFSIZ - 16 + 1` characters. The
> writes into the line buffer are not checked against
> `el->el_line.limit`: `cp` can reach `buffer + wcslen(prompt) + 1008`
> and one more character is stored there, so a prompt longer than 15
> characters combined with maximal input would run past the 1024-slot
> initial line buffer. The two in-tree callers use prompts of 2 and 3
> characters.

> [spec:libedit:def:chared.c-hpos-fn]
> libedit_private int c_hpos(EditLine *el)

> [spec:libedit:sem:chared.c-hpos-fn]
> Returns the cursor's zero-based column within its physical line, where
> an embedded `L'\n'` in the edit buffer starts a new line. Modifies
> nothing.
>
> 1. If `el->el_line.cursor == el->el_line.buffer`, return 0.
> 2. Otherwise scan backwards starting at `cursor - 1`, decrementing
>    while the pointer is `>= buffer` and the character it addresses is
>    not `L'\n'`.
> 3. Return `cursor - ptr - 1`, where `ptr` is where the scan stopped.
>    If it stopped on a newline, that is the count of characters
>    strictly between the newline and the cursor. If it ran off the
>    front (`ptr == buffer - 1`), the result is `cursor - buffer`, the
>    full distance from the start of the buffer.
>
> The no-newline case forms the pointer `buffer - 1`, which is undefined
> behaviour in C. The port should scan over indices and treat "no
> newline found" as the column `cursor - buffer`.

> [spec:libedit:def:chared.c-insert-fn]
> libedit_private void c_insert(EditLine *el, int num)

> [spec:libedit:sem:chared.c-insert-fn]
> Opens a gap of `num` character slots at the cursor without writing
> anything into it; the caller fills the gap.
>
> 1. If `el->el_line.lastchar + num >= el->el_line.limit`, call
>    `ch_enlargebufs(el, num)`; if that returns 0 (allocation failure),
>    return immediately having changed nothing. Note the test is `>=`,
>    so growth triggers when the projected end merely reaches `limit`.
> 2. If `el->el_line.cursor < el->el_line.lastchar`, shift the tail
>    right: iterate `cp` DOWNWARD from `lastchar` to `cursor` inclusive,
>    assigning `cp[num] = *cp`. The descending order is what makes the
>    overlapping move correct. `lastchar - cursor + 1` characters move —
>    the tail plus the one slot at `lastchar` — so `[cursor, lastchar]`
>    ends up at `[cursor + num, lastchar + num]`.
> 3. `el->el_line.lastchar += num`.
>
> `el->el_line.cursor` is unchanged, and the `num` slots at
> `[cursor, cursor + num)` retain whatever was there before (shifted-away
> text, or zeros from the initial `calloc`) — `c_insert` never blanks
> them. When `cursor == lastchar` step 2 is skipped entirely, so
> appending at the end simply exposes `num` stale slots. Nothing is
> returned, and a failed enlargement is indistinguishable from success
> except that the line is unchanged.
>
> `num == 0` is a no-op except that step 2's loop copies each character
> onto itself. A negative `num` is not defended against: step 2 would
> shift the tail LEFT and step 3 would shrink the line, writing below
> `cursor` and possibly below `buffer`. No caller passes one.

> [spec:libedit:def:chared.c-kill-t]
> typedef struct c_kill_t

> [spec:libedit:def:chared.c-next-word-fn]
> libedit_private wchar_t * c__next_word(EditLine *el, wchar_t *p, wchar_t *high, int n, int (*wtest)(EditLine *, wint_t))

> [spec:libedit:sem:chared.c-next-word-fn]
> Scans forward from `p` over `n` words and returns the position just
> past the end of the `n`th one. `high` is the exclusive upper bound,
> normally `el->el_line.lastchar`. `wtest` is a word-membership
> predicate (`ce__isword`, `cv__isword` or `cv__isWord`), applied as
> `wtest(el, *q)` and used as a BOOLEAN here — any nonzero result means
> "part of a word", so with `cv__isword` punctuation counts as word
> material.
>
> Repeat `n` times, with a working pointer starting at `p`:
> 1. While the pointer is `< high` and `wtest` reports the character not
>    a word character, advance it (skip the gap).
> 2. While the pointer is `< high` and `wtest` reports it a word
>    character, advance it (skip the word).
>
> Then, defensively, if the pointer is `> high` set it to `high`; this
> is unreachable because both loops stop at `high`. Return the pointer.
>
> Nothing is dereferenced at or beyond `high` and no `EditLine` state is
> modified — the return value is the entire result. With `n <= 0` the
> caller's `p` comes back unchanged. The count is consumed as
> `while (n--)`, so a negative `n` counts down toward `INT_MIN` rather
> than stopping: an effectively unbounded loop. No caller passes one.

> [spec:libedit:def:chared.c-prev-word-fn]
> libedit_private wchar_t * c__prev_word(EditLine *el, wchar_t *p, wchar_t *low, int n, int (*wtest)(EditLine *, wint_t))

> [spec:libedit:sem:chared.c-prev-word-fn]
> Scans backward from `p` over `n` words and returns the position of the
> first character of the `n`th one. `low` is the inclusive lower bound,
> normally `el->el_line.buffer`. `wtest` is the word-membership
> predicate, used as a boolean exactly as in `c__next_word`.
>
> 1. Decrement `p` once, so scanning starts on the character before the
>    caller's position; the character at the caller's `p` is never
>    examined.
> 2. Repeat `n` times:
>    a. While `p >= low` and `wtest` reports the character not a word
>       character, decrement `p`.
>    b. While `p >= low` and `wtest` reports it a word character,
>       decrement `p`.
> 3. Increment `p` once — it now sits on the first character of the word
>    rather than one before it.
> 4. If `p < low`, set `p = low`.
> 5. Return `p`.
>
> No `EditLine` state is modified. With `n <= 0` steps 1 and 3 cancel
> and the caller's `p` is returned unchanged (clamped to `low`). As in
> `c__next_word`, a negative `n` makes `while (n--)` run essentially
> forever; no caller passes one.
>
> Every dereference is guarded by `p >= low`, but steps 1 and 2 can
> leave `p` at `low - 1`. The C forms that pointer without dereferencing
> it, which is still undefined behaviour, and is a reason for the port
> to scan over indices with a signed or saturating position.

> [spec:libedit:def:chared.c-redo-t]
> typedef struct c_redo_t

> [spec:libedit:def:chared.c-undo-t]
> typedef struct c_undo_t

> [spec:libedit:def:chared.c-vcmd-t]
> typedef struct c_vcmd_t

> [spec:libedit:def:chared.ce-isword-fn]
> libedit_private int ce__isword(EditLine *el, wint_t p)

> [spec:libedit:sem:chared.ce-isword-fn]
> Emacs word-membership test for a single character. Returns 1 if
> `iswalnum(p)` is true in the current locale, or if `p` occurs anywhere
> in the NUL-terminated string `el->el_map.wordchars` (a `wcschr`
> lookup); returns 0 otherwise. Because the C expression is a `||`, the
> result is exactly 0 or 1 — never the raw `iswalnum` value.
>
> `el->el_map.wordchars` is the current extra-word-character set, seeded
> by `map_init_emacs`/`map_init_vi` and replaceable through
> `map_set_wordchars`; it is never NULL once `map_init` has run.
>
> One inherited edge case: `wcschr` matches the terminating NUL, so
> `p == L'\0'` is reported as a word character. The word scanners do not
> normally look past `lastchar`, but a port that classifies characters
> at the buffer edge must reproduce this to stay bit-identical.

> [spec:libedit:def:chared.ch-aliasfun-fn]
> libedit_private int ch_aliasfun(EditLine *el, el_afunc_t f, void *a)

> [spec:libedit:sem:chared.ch-aliasfun-fn]
> Installs the alias-expansion hook. Stores `f` into
> `el->el_chared.c_aliasfun` and `a` into `el->el_chared.c_aliasarg`,
> unconditionally and with no validation, then returns 0. It cannot
> fail; the `int` return exists only so `el_set` can propagate a status.
> Passing `f == NULL` clears the hook.
>
> This is the implementation behind `el_set(el, EL_ALIAS_TEXT, f, a)`.
> The stored function is called from `vi_alias` as
> `f(c_aliasarg, name)` where `name` is a three-byte narrow string
> `_<char>`; a non-NULL `const char *` result is pushed back into the
> input stream, and the returned storage is owned by the callee.
> `vi_alias` returns `CC_ERROR` without calling anything when the hook
> is NULL.

> [spec:libedit:def:chared.ch-end-fn]
> libedit_private void ch_end(EditLine *el)

> [spec:libedit:sem:chared.ch-end-fn]
> Releases everything `ch_init` allocated and returns the character
> editor to its post-reset state. Idempotent: every pointer it frees is
> then set to NULL, and freeing NULL is a no-op.
>
> In order:
> 1. Free `el->el_line.buffer`; set `el->el_line.buffer` and
>    `el->el_line.limit` to NULL.
> 2. Free `el->el_chared.c_undo.buf`; set it to NULL.
> 3. Free `el->el_chared.c_redo.buf`; set `c_redo.buf`, `c_redo.pos` and
>    `c_redo.lim` to NULL and `c_redo.cmd` to `ED_UNASSIGNED`.
> 4. Free `el->el_chared.c_kill.buf`; set it to NULL.
> 5. Call `ch_reset(el)`.
>
> Because `ch_reset` re-derives `el->el_line.cursor`,
> `el->el_line.lastchar`, `el->el_chared.c_vcmd.pos` and
> `el->el_chared.c_kill.mark` from `el->el_line.buffer` — now NULL — all
> four end up NULL, and the editor state fields are reset too
> (`c_undo.len = -1`, `c_undo.cursor = 0`, `c_vcmd.action = NOP`,
> `el_map.current = el_map.key`, the `el_state` defaults, and
> `el_history.eventno = 0`).
>
> One field is left dangling: `el->el_chared.c_kill.last` still points
> into the freed kill buffer, since neither `ch_end` nor `ch_reset`
> touches it. Nothing reads it before the next `ch_init` or `cv_yank`
> reassigns it, so the port may simply null it; that is not observable.
>
> Nothing is returned. `ch_init` calls `ch_end` on two of its own
> failure paths, which is why every step must tolerate partially
> initialised state.

> [spec:libedit:def:chared.ch-enlargebufs-fn]
> libedit_private int ch_enlargebufs(EditLine *el, size_t addlen)

> [spec:libedit:sem:chared.ch-enlargebufs-fn]
> Grows the line buffer, the three buffers whose allocations track it
> (kill, undo, redo) and the history buffer, so that at least `addlen`
> more characters fit. Returns 1 on success, 0 on failure.
>
> Sizing, in `wchar_t` units:
> 1. `sz = (limit - buffer) + EL_LEAVE`, the current allocation.
>    `EL_LEAVE` is 2, the file-local count of slots deliberately kept
>    unused at the end, and `limit` is always `buffer + sz - EL_LEAVE`.
> 2. `newsz = sz * 2`.
> 3. If `addlen > sz`, keep doubling `newsz` while `newsz - sz < addlen`
>    — the newly added space alone must cover `addlen`.
>
> Then the following steps in exactly this order, each reallocating to
> `newsz * sizeof(wchar_t)` and returning 0 immediately if the
> reallocation fails. A failed `realloc` leaves the original block
> allocated and still referenced by the field, so no failure path leaks
> or dangles.
> 4. Line buffer. Zero only the newly added tail `[sz, newsz)`, leaving
>    existing contents in place. Rebase `el->el_line.cursor` and
>    `el->el_line.lastchar` by the difference between the new base and
>    the pre-realloc base. Then set `limit = &newbuffer[sz - EL_LEAVE]`
>    — deliberately still the OLD capacity, so that if a later step
>    fails the line cannot be written past the smallest buffer that
>    actually grew.
> 5. Kill buffer. Zero the new tail. Rebase `c_kill.last` against the
>    old kill-buffer base, and `c_kill.mark` against the old LINE base
>    onto the new line buffer — `mark` is a position in the line, not in
>    the kill buffer.
> 6. Undo buffer. Zero the new tail and store the new pointer. There is
>    nothing to rebase: `c_undo.cursor` is an index and `c_undo.len` a
>    length.
> 7. Redo buffer. Rebase `c_redo.pos` and `c_redo.lim` against the old
>    redo base, then store the new base. Two asymmetries with the steps
>    above are deliberate to record, not necessarily to keep: the new
>    tail is NOT zeroed, and `c_redo.lim` keeps its old offset, so the
>    redo buffer's usable limit does not grow even though its allocation
>    does.
> 8. `hist_enlargebuf(el, newsz)`; if it returns 0, return 0.
>
> Only once all of that has succeeded:
> 9. `limit = &el->el_line.buffer[newsz - EL_LEAVE]`, publishing the
>    enlarged capacity.
> 10. If `el->el_chared.c_resizefun` is non-NULL, call it as
>     `c_resizefun(el, c_resizearg)`, so the application can re-derive
>     any pointers it holds into the line.
> 11. Return 1.
>
> On any failure the return is 0 and the growth is partial but
> consistent: some buffers are larger than others, `limit` still
> describes the pre-call capacity, and no live pointer is stale. Callers
> treat 0 as "cannot grow" and abandon the operation.
>
> The rebasing reads the old pointer values after the `realloc` that
> invalidated them, which is undefined in C. The port should hold
> offsets rather than pointers across the growth. It may then simplify
> the two-phase `limit` update, provided that after a failure the
> observable capacity is still the pre-call one.

> [spec:libedit:def:chared.ch-init-fn]
> libedit_private int ch_init(EditLine *el)

> [spec:libedit:sem:chared.ch-init-fn]
> Allocates the character editor's four buffers and sets the editor's
> initial state. Returns 0 on success, -1 on failure.
>
> 1. `el->el_line.buffer = calloc(EL_BUFSIZ, sizeof(wchar_t))`, with
>    `EL_BUFSIZ == 1024`; on failure return -1 at once.
> 2. `el->el_line.cursor = el->el_line.lastchar = el->el_line.buffer`;
>    `el->el_line.limit = &buffer[EL_BUFSIZ - EL_LEAVE]`, i.e.
>    `buffer + 1022`, reserving the last two slots.
> 3. `c_undo.buf = calloc(EL_BUFSIZ, sizeof(wchar_t))`; on failure
>    return -1. Then `c_undo.len = -1` (the "nothing saved" marker) and
>    `c_undo.cursor = 0`.
> 4. `c_redo.buf = calloc(EL_BUFSIZ, sizeof(wchar_t))`; on failure jump
>    to the cleanup path. Then `c_redo.pos = c_redo.buf`,
>    `c_redo.lim = c_redo.buf + EL_BUFSIZ`, and
>    `c_redo.cmd = ED_UNASSIGNED`. `c_redo.count`, `c_redo.action` and
>    `c_redo.ch` are left alone — the whole `EditLine` came from
>    `calloc` in `el_init_internal`, so they are already zero.
> 5. `c_vcmd.action = NOP`; `c_vcmd.pos = el->el_line.buffer`.
> 6. `c_kill.buf = calloc(EL_BUFSIZ, sizeof(wchar_t))`; on failure jump
>    to the cleanup path. Then `c_kill.mark = el->el_line.buffer` (it
>    points into the line) and `c_kill.last = c_kill.buf` (kill buffer
>    empty).
> 7. `c_resizefun`, `c_resizearg`, `c_aliasfun`, `c_aliasarg` all NULL.
> 8. `el->el_map.current = el->el_map.key`.
> 9. `el->el_state`: `inputmode = MODE_INSERT`, `doingarg = 0`,
>    `metanext = 0`, `argument = 1`, `lastcmd = ED_UNASSIGNED`.
> 10. Return 0.
>
> The cleanup path calls `ch_end(el)`, which frees whatever was
> allocated and resets the state, then returns -1.
>
> The error handling is inconsistent and the port should note it rather
> than copy it blindly: the step 3 failure returns -1 without freeing
> the line buffer allocated in step 1, leaking it while
> `el->el_line.buffer` still points at live memory, whereas the step 4
> and step 6 failures unwind through `ch_end`. All three are unreachable
> except under allocation failure. All four buffers are `calloc`ed so
> their contents start zeroed, and `ch_enlargebufs` keeps all four the
> same size thereafter.

> [spec:libedit:def:chared.ch-reset-fn]
> libedit_private void ch_reset(EditLine *el)

> [spec:libedit:sem:chared.ch-reset-fn]
> Returns the editor to the state it holds at the start of a fresh input
> line. Called once per line from `read_prepare`, and by `ch_end` after
> the buffers have been freed. Takes no decisions and returns nothing;
> every assignment below is unconditional.
>
> - `el->el_line.cursor = el->el_line.lastchar = el->el_line.buffer` —
>   the line is now logically empty. Its contents are NOT cleared; the
>   previous text stays in the buffer above `lastchar`.
> - `el->el_chared.c_undo.len = -1` and `c_undo.cursor = 0`, i.e. no
>   undo state.
> - `el->el_chared.c_vcmd.action = NOP` and
>   `c_vcmd.pos = el->el_line.buffer`.
> - `el->el_chared.c_kill.mark = el->el_line.buffer`.
> - `el->el_map.current = el->el_map.key`. `el_map.key` holds the emacs
>   bindings in emacs mode and the vi INSERT bindings in vi mode
>   (`el_map.alt` holds vi command mode), so every line starts in insert
>   mode.
> - `el->el_state.inputmode = MODE_INSERT`, `doingarg = 0`,
>   `metanext = 0`, `argument = 1`, `lastcmd = ED_UNASSIGNED`.
> - `el->el_history.eventno = 0`, i.e. back to the newest history entry.
>
> Explicitly NOT touched: the kill buffer's contents and
> `c_kill.last`, so a previous kill survives into the next line; the
> undo and redo buffer contents; `c_redo.pos`, `c_redo.lim` and
> `c_redo.cmd`; the resize and alias hooks; and `el->el_line.limit`.
>
> When called from `ch_end`, `el->el_line.buffer` is already NULL, so
> `cursor`, `lastchar`, `c_vcmd.pos` and `c_kill.mark` all become NULL
> rather than pointing at a buffer.

> [spec:libedit:def:chared.ch-resizefun-fn]
> libedit_private int ch_resizefun(EditLine *el, el_zfunc_t f, void *a)

> [spec:libedit:sem:chared.ch-resizefun-fn]
> Installs the buffer-resize callback. Stores `f` into
> `el->el_chared.c_resizefun` and `a` into `el->el_chared.c_resizearg`,
> unconditionally and with no validation, then returns 0. It cannot
> fail; the `int` return exists only so `el_set` can propagate a status.
> Passing `f == NULL` clears the callback.
>
> This is the implementation behind `el_set(el, EL_RESIZE, f, a)`. The
> stored function is called as `f(el, c_resizearg)` at the very end of a
> successful `ch_enlargebufs`, after `el->el_line.limit` has been
> updated to the new capacity, so that the application can re-derive any
> pointers it holds into the line buffer.

> [spec:libedit:def:chared.cv-delfini-fn]
> libedit_private void cv_delfini(EditLine *el)

> [spec:libedit:sem:chared.cv-delfini-fn]
> Completes a pending vi operator (`d`, `c`, `y`, …) once the motion
> that follows it has moved the cursor. `el->el_chared.c_vcmd.pos` is
> the anchor position the operator was started at, and `c_vcmd.action`
> is the bitmask set by `cv_action`: `NOP` 0x00, `DELETE` 0x01,
> `INSERT` 0x02, `YANK` 0x04.
>
> 1. Read `action = el->el_chared.c_vcmd.action`.
> 2. If `action & INSERT`, set `el->el_map.current = el->el_map.key` — a
>    change operator drops into insert mode. This happens before the
>    sanity check below, so it happens even when nothing is edited.
> 3. If `c_vcmd.pos == NULL`, return immediately. Note `c_vcmd.action`
>    is NOT cleared on this path.
> 4. `size = cursor - c_vcmd.pos`, a signed character count: positive if
>    the motion ran forward, negative if backward.
> 5. If `size == 0`, set `size = 1`; a zero-width motion therefore
>    affects the single character under the cursor.
> 6. `el->el_line.cursor = c_vcmd.pos` — the cursor goes back to the
>    anchor before any edit.
> 7. If `action & YANK`, copy the span into the kill buffer without
>    deleting it: `cv_yank(el, cursor, size)` when `size > 0`, otherwise
>    `cv_yank(el, cursor + size, -size)`, which starts at the lower end
>    of the span and passes a positive length.
> 8. Otherwise (delete or change): if `size > 0`, call
>    `c_delafter(el, size)` and then `re_refresh_cursor(el)`; if
>    `size < 0`, call `c_delbefore(el, -size)` and then
>    `el->el_line.cursor += size`, which is the caller-side adjustment
>    `c_delbefore` requires. Note `c_delafter`/`c_delbefore` also take
>    the undo snapshot and fill the kill buffer, so a delete leaves the
>    removed text yankable.
> 9. `c_vcmd.action = NOP`. `c_vcmd.pos` is left as it was.
>
> Nothing is returned. Only step 8's forward branch refreshes the
> cursor; the backward branch and the yank branch rely on the caller's
> return code to drive redisplay.

> [spec:libedit:def:chared.cv-endword-fn]
> libedit_private wchar_t * cv__endword(EditLine *el, wchar_t *p, wchar_t *high, int n, int (*wtest)(EditLine *, wint_t))

> [spec:libedit:sem:chared.cv-endword-fn]
> Scans forward from `p` and returns the position of the LAST character
> of the `n`th word — vi's `e`/`E` motion. `wtest` is `cv__isword`
> (three-valued: 1 word, 2 punctuation, 0 space) or `cv__isWord`
> (1 non-space, 0 space), and its result is compared for EQUALITY rather
> than truth, so with `cv__isword` a run of punctuation is a word in its
> own right.
>
> 1. Increment `p` once, so the character the cursor already sits on
>    cannot end the first word.
> 2. Repeat `n` times:
>    a. While `p < high` and `iswspace(*p)`, advance `p`.
>    b. `test = wtest(el, *p)` — classify the character now under `p`.
>    c. While `p < high` and `wtest(el, *p) == test`, advance `p`.
> 3. Decrement `p` once, moving from one-past-the-word back onto its
>    last character.
> 4. Return `p`.
>
> No `EditLine` state is modified. There is no clamp on the way out: if
> the scan reaches `high` the result is `high - 1`, which is the
> intended "last character of the line" answer when `high` is
> `lastchar`. With `n <= 0` steps 1 and 3 cancel and the caller's `p`
> comes back unchanged.
>
> Step 2b dereferences `*p` without first testing `p < high`, so when
> the scan has already reached `high` the character AT `high` is read —
> for `high == lastchar` that is the reserved slot past the line, inside
> the allocation but holding stale data. The value cannot affect the
> result, because loop 2c's `p < high` guard fails immediately, but it
> is a read of unspecified data; the port should treat "already at
> `high`" as classifying nothing.

> [spec:libedit:def:chared.cv-is-word-fn]
> libedit_private int cv__isWord(EditLine *el __attribute__((__unused__)), wint_t p)

> [spec:libedit:sem:chared.cv-is-word-fn]
> vi "big word" (WORD) membership test. Ignores its `EditLine` parameter
> entirely — it is declared unused — and returns `!iswspace(p)`:
> exactly 1 for any character that is not whitespace in the current
> locale, 0 for whitespace. No state is read or written.
>
> The values matter as much as the truth. `cv_next_word`,
> `cv_prev_word` and `cv__endword` compare `wtest` results for equality,
> and since this predicate yields only 0 or 1, every non-space character
> falls into a single class. That is precisely what makes vi's `W`, `B`
> and `E` treat punctuation as part of the surrounding word, where
> `cv__isword` would split it out.

> [spec:libedit:def:chared.cv-isword-fn]
> libedit_private int cv__isword(EditLine *el, wint_t p)

> [spec:libedit:sem:chared.cv-isword-fn]
> vi small-word membership test, three-valued, evaluated in this order:
> - return 1 if `iswalnum(p)` is true in the current locale, or if `p`
>   occurs in `el->el_map.wordchars` (a `wcschr` lookup, which also
>   matches the string's terminating NUL, so `p == L'\0'` yields 1);
> - otherwise return 2 if `iswgraph(p)` is true — a printable
>   non-space, non-word character, i.e. punctuation;
> - otherwise return 0 (whitespace and non-printables).
>
> No state is written. The two nonzero values are not interchangeable:
> `cv_next_word`, `cv_prev_word` and `cv__endword` compare this result
> for EQUALITY in order to find runs of a single class, which is how
> vi's `w`, `b` and `e` stop at the boundary between a word and adjacent
> punctuation. `c__next_word` and `c__prev_word`, which treat the result
> as a boolean, would instead see punctuation as word material.

> [spec:libedit:def:chared.cv-next-word-fn]
> libedit_private wchar_t * cv_next_word(EditLine *el, wchar_t *p, wchar_t *high, int n, int (*wtest)(EditLine *el, wint_t))

> [spec:libedit:sem:chared.cv-next-word-fn]
> vi's `w`/`W` motion: scans forward over `n` words and returns the
> start of the word after them. `high` is the exclusive upper bound.
> `wtest` is `cv__isword` or `cv__isWord`, and its result is compared
> for EQUALITY, so a run of punctuation counts as a word of its own.
>
> Repeat `n` times. The count is consumed as `while (n--)`, so inside
> the body `n` already means "iterations still to come after this one":
> 1. `test = wtest(el, *p)` — classify the character at the current
>    position, without first checking `p < high`.
> 2. While `p < high` and `wtest(el, *p) == test`, advance `p`. This
>    consumes the run the cursor started inside, whatever its class.
> 3. If `n != 0` (more iterations follow) OR
>    `el->el_chared.c_vcmd.action != (DELETE|INSERT)` (0x03, i.e. this
>    is not a pending `cw`), then while `p < high` and `iswspace(*p)`,
>    advance `p`. This encodes the historical vi quirk: `cw` deletes the
>    word but preserves the whitespace after it, so on the FINAL
>    iteration of a change-word the trailing blanks are left alone,
>    while plain `w` and every non-final iteration consume them.
>
> Afterwards return `high` if `p > high`, otherwise `p`. No `EditLine`
> state is modified — the pending vi action is read, never written.
>
> With `n <= 0` the body never runs, `p` comes back unchanged and `*p`
> is not read. Step 1's unguarded dereference means that when `p` is
> already `high` (cursor at end of line) the reserved slot at `lastchar`
> is read; the value cannot change the result, since step 2's guard
> fails immediately, but it is a read of unspecified data that the port
> should skip.

> [spec:libedit:def:chared.cv-prev-word-fn]
> libedit_private wchar_t * cv_prev_word(EditLine *el, wchar_t *p, wchar_t *low, int n, int (*wtest)(EditLine *el, wint_t))

> [spec:libedit:sem:chared.cv-prev-word-fn]
> vi's `b`/`B` motion: scans backward over `n` words and returns the
> first character of the `n`th one. `low` is the inclusive lower bound,
> normally `el->el_line.buffer`. `wtest`'s result is compared for
> EQUALITY, as in `cv_next_word`.
>
> 1. Decrement `p` once — the character the cursor sits on is not
>    considered.
> 2. Repeat `n` times:
>    a. While `p > low` and `iswspace(*p)`, decrement `p`. Note the
>       strict `>`: the scan stops at `low` without testing it.
>    b. `test = wtest(el, *p)`, classifying the character now under `p`.
>    c. While `p >= low` and `wtest(el, *p) == test`, decrement `p`.
>    d. If `p < low` — the run reached the start of the buffer — return
>       `low` immediately, skipping any remaining iterations and step 3.
> 3. Increment `p` once, moving from one-before-the-word onto its first
>    character.
> 4. Return `low` if `p < low`, otherwise `p`. (After step 2d this clamp
>    is unreachable.)
>
> No `EditLine` state is modified. With `n <= 0` steps 1 and 3 cancel
> and the caller's `p` is returned unchanged.
>
> Step 2b is the hazard. If `p == low` on entry, step 1 has already
> produced `low - 1` and step 2a's `p > low` guard does not fire, so
> `*(low - 1)` — one element before the line buffer — is read and
> classified. The following `p >= low` guard then stops loop 2c at once
> and step 2d returns `low`, so the value never reaches the result, but
> the read itself is out of bounds. The port should return `low` as soon
> as the position falls below it and never form that position at all.

> [spec:libedit:def:chared.cv-undo-fn]
> libedit_private void cv_undo(EditLine *el)

> [spec:libedit:sem:chared.cv-undo-fn]
> Snapshots the current line so vi's `u` can restore it, and records the
> command about to run so vi's `.` can repeat it. Called at the start of
> every line-modifying vi command, and (because of the tautological
> keymap test) from every `c_delafter`/`c_delbefore` as well.
>
> Undo half, into `el->el_chared.c_undo`:
> 1. `size = lastchar - buffer`, the current line length in characters.
> 2. `c_undo.len = size` as an `ssize_t`. -1 is the "nothing saved"
>    marker set by `ch_init`/`ch_reset`; 0 is a legitimately saved empty
>    line.
> 3. `c_undo.cursor = cursor - buffer`, the cursor as an `int` offset
>    rather than a pointer, so it survives a `ch_enlargebufs`.
> 4. `memcpy(c_undo.buf, buffer, size * sizeof(wchar_t))` — the whole
>    line from its start into the undo buffer. No terminator is written;
>    `c_undo.len` is the only length. The undo buffer is always the same
>    size as the line buffer (both start at `EL_BUFSIZ` and
>    `ch_enlargebufs` grows them together), so the copy always fits, and
>    they are distinct allocations, so it never overlaps.
>
> Redo half, into `el->el_chared.c_redo`:
> 5. `c_redo.count = el->el_state.doingarg ? el->el_state.argument : 0`
>    — the numeric prefix if one was typed, else 0.
> 6. `c_redo.action = el->el_chared.c_vcmd.action`, the pending vi
>    operator bitmask.
> 7. `c_redo.pos = c_redo.buf`, rewinding the redo buffer's write
>    pointer so that recording of the inserted key sequence starts
>    fresh. The previous contents are not cleared, only orphaned.
> 8. `c_redo.cmd = el->el_state.thiscmd` and
>    `c_redo.ch = el->el_state.thisch` — the command being executed and
>    the character that invoked it.
>
> `c_redo.lim` is not touched. The line itself is not modified and
> nothing is returned.

> [spec:libedit:def:chared.cv-yank-fn]
> libedit_private void cv_yank(EditLine *el, const wchar_t *ptr, int size)

> [spec:libedit:sem:chared.cv-yank-fn]
> Copies `size` characters starting at `ptr` into the kill buffer,
> replacing whatever was there.
>
> 1. `memcpy(el->el_chared.c_kill.buf, ptr, size * sizeof(wchar_t))` —
>    always from the START of the kill buffer. This is a replace, never
>    an append.
> 2. `el->el_chared.c_kill.last = el->el_chared.c_kill.buf + size`,
>    which is what gives the kill buffer its length.
>
> `c_kill.mark` is untouched, the line is untouched, and nothing is
> returned. `size == 0` copies nothing and leaves the kill buffer empty
> (`last == buf`), which is exactly what `c_delafter`/`c_delbefore`
> produce when their clamp reduces the count to zero.
>
> There is no bounds check. The kill buffer is allocated at `EL_BUFSIZ`
> and grown in lockstep with the line buffer by `ch_enlargebufs`, and
> every caller derives `size` from a span of the line, so it fits. `ptr`
> normally points into the line buffer — a different allocation — so the
> `memcpy` does not overlap; passing a pointer into the kill buffer
> itself would be undefined, and no caller does. A negative `size`
> becomes an enormous `size_t` length, i.e. undefined behaviour;
> callers must pass a non-negative count, and `cv_delfini` negates its
> own before calling.

> [spec:libedit:def:chared.el-afunc-t-void-const-char]
> typedef const char *(*el_afunc_t)(void *, const char *)

> [spec:libedit:def:chared.el-chared-t]
> typedef struct el_chared_t

> [spec:libedit:def:chared.el-cursor-fn]
> int el_cursor(EditLine *el, int n)

> [spec:libedit:sem:chared.el-cursor-fn]
> Public API declared in `histedit.h`. Moves the cursor `n` characters
> right (positive `n`) or left (negative `n`), clamps it to the line,
> and returns the resulting offset.
>
> 1. If `n == 0`, skip straight to the return. No clamping happens on
>    this path, so a cursor that is somehow already out of range is
>    reported as-is rather than corrected.
> 2. Otherwise `el->el_line.cursor += n`.
> 3. If `cursor < el->el_line.buffer`, set `cursor = buffer`.
> 4. If `cursor > el->el_line.lastchar`, set `cursor = lastchar`.
> 5. Return `(int)(cursor - buffer)`: the zero-based offset of the
>    cursor from the start of the line, after clamping.
>
> The clamps apply in that order, so for any `n != 0` the result is in
> `[0, lastchar - buffer]` however large `n` is. Nothing else in
> `EditLine` changes and no redisplay is triggered; the caller is
> expected to arrange one.
>
> Step 2 can transiently form a pointer far outside the line allocation
> before steps 3 and 4 pull it back, which is undefined in C: the port
> must do this arithmetic on a saturating index. Under
> `[dec:libedit:no-c-ffi]` this function's behaviour is frozen at the C
> ABI, so both the clamped return value and the `n == 0`
> short-circuit must be reproduced exactly.

> [spec:libedit:def:chared.el-deletestr-fn]
> void el_deletestr(EditLine *el, int n)

> [spec:libedit:sem:chared.el-deletestr-fn]
> Public API declared in `histedit.h`. Deletes `n` characters
> immediately before the cursor and moves the cursor back over them.
> Returns nothing, and gives the caller no way to tell whether anything
> happened.
>
> 1. If `n <= 0`, return without doing anything.
> 2. If `el->el_line.cursor < &el->el_line.buffer[n]` — fewer than `n`
>    characters exist before the cursor — return without doing anything.
>    This is all-or-nothing: it does NOT delete the characters that are
>    available.
> 3. `c_delbefore(el, n)`, which slides `[cursor, lastchar]` down by `n`
>    and decrements `lastchar` by `n`. Because of the tautological
>    keymap test inside `c_delbefore`, this also takes a vi undo
>    snapshot and overwrites the kill buffer with the deleted text, in
>    emacs mode as much as in vi — an ABI-visible side effect of calling
>    `el_deletestr`.
> 4. `el->el_line.cursor -= n`.
> 5. If `cursor < buffer`, set `cursor = buffer`. Step 2 already
>    guarantees this cannot fire; it is defensive only.
>
> No redisplay is triggered. `[dec:libedit:no-c-ffi]` freezes this
> behaviour at the ABI, including the refuse-rather-than-truncate rule
> of step 2 and the kill-buffer side effect of step 3.

> [spec:libedit:def:chared.el-deletestr1-fn]
> int el_deletestr1(EditLine *el, int start, int end)

> [spec:libedit:sem:chared.el-deletestr1-fn]
> Public API declared in `histedit.h`, reached from readline's
> `rl_delete_text`. Its intent is to delete the characters in the
> half-open index range `[start, end)` of the line. What it actually
> does is narrower, and the difference is observable — see below.
>
> 1. If `end <= start`, return 0 having touched nothing.
> 2. `line_length = lastchar - buffer`.
> 3. If `start >= line_length` OR `end >= line_length`, return 0 having
>    touched nothing. Note the second test is `>=`, not `>`, so a range
>    ending exactly at the end of the line is rejected: the final
>    character of the line can never be deleted through this entry
>    point.
> 4. `len = end - start`, then clamp `len` down to `line_length - end`
>    if that is smaller.
> 5. With `p1 = buffer + start` and `p2 = buffer + end`, copy `len`
>    characters one at a time from `p2` upward to `p1` upward,
>    decrementing `el->el_line.lastchar` once per character copied.
>    `lastchar` therefore falls by `len`, not by `end - start`.
> 6. If `cursor < buffer`, set `cursor = buffer` — dead code. The cursor
>    is NOT adjusted for the deletion, so it can be left pointing above
>    the new `lastchar`.
> 7. Return `end - start`, the size of the range that was requested,
>    regardless of how many characters were actually removed. The two
>    early returns give 0.
>
> Step 5 is wrong, and recording that is the point of this rule.
> Deleting `[start, end)` correctly requires moving the entire tail
> `[end, line_length)` down to `start` — `line_length - end` characters
> — and shortening the line by `end - start`. The C instead moves
> `min(end - start, line_length - end)` characters and shortens the line
> by that same clamped count. Both failure modes are reachable:
> - Long tail (`line_length - end >= end - start`): the line length ends
>   up right but the content does not, because only the first
>   `end - start` characters of the tail are moved down and everything
>   beyond `end + len` stays where it was. With `abcdefgh`,
>   `start = 1`, `end = 3`: `len` is 2, the buffer becomes `adedefgh`
>   and `lastchar` lands at offset 6, so the line reads `adedef` where
>   `adefgh` is correct.
> - Short tail (`line_length - end < end - start`): the whole tail is
>   moved down correctly but the line is left too long by
>   `(end - start) - (line_length - end)`, exposing stale characters at
>   the end. With `abcdefgh`, `start = 1`, `end = 6`: `len` is 2, the
>   buffer becomes `aghdefgh` and `lastchar` lands at offset 6, so the
>   line reads `aghdef` where `agh` is correct.
>
> `[dec:libedit:no-c-ffi]` freezes what a C caller can observe, so the
> port must make this a deliberate decision: reproduce the C's
> arithmetic bug for bug, or fix it and accept a visible divergence in
> `rl_delete_text`. It must not be resolved silently by writing the
> obvious correct loop.

> [spec:libedit:def:chared.el-winsertstr-fn]
> int el_winsertstr(EditLine *el, const wchar_t *s)

> [spec:libedit:sem:chared.el-winsertstr-fn]
> Public API declared in `histedit.h`. Inserts the NUL-terminated wide
> string `s` at the cursor and leaves the cursor just past it. Returns 0
> on success, -1 on failure.
>
> 1. If `s == NULL`, or `len = wcslen(s)` is 0, return -1. An empty
>    insert is an error, not a no-op.
> 2. If `el->el_line.lastchar + len >= el->el_line.limit`, call
>    `ch_enlargebufs(el, len)`; if that returns 0, return -1 with
>    nothing changed.
> 3. `c_insert(el, (int)len)`, which opens a gap of `len` slots at the
>    cursor by shifting `[cursor, lastchar]` right and advancing
>    `lastchar` by `len`. It repeats the capacity test of step 2
>    internally, which is harmless.
> 4. Copy the string in place with `*el->el_line.cursor++ = *s++` until
>    the terminating NUL. On return `el->el_line.cursor` therefore sits
>    immediately after the inserted text, and the gap is exactly filled.
> 5. Return 0.
>
> No NUL is written into the line — `el->el_line.lastchar` is the only
> end marker — and no redisplay is triggered. `len` is cast from
> `size_t` to `int` for `c_insert`; a string longer than `INT_MAX` would
> yield a negative count and corrupt the line. That is unreachable in
> practice and the port should simply carry an unsigned count.

> [spec:libedit:def:chared.el-wreplacestr-fn]
> int el_wreplacestr(EditLine *el, const wchar_t *s)

> [spec:libedit:sem:chared.el-wreplacestr-fn]
> Public API declared in `histedit.h`. Replaces the whole line with `s`.
> Returns 0 on success, -1 on failure.
>
> 1. If `s == NULL`, or `len = wcslen(s)` is 0, return -1 — clearing the
>    line by passing an empty string is not supported.
> 2. If `el->el_line.buffer + len >= el->el_line.limit`, call
>    `ch_enlargebufs(el, len)`; if that returns 0, return -1 with
>    nothing changed. The test is against `buffer`, not `lastchar`,
>    because the new content starts at the beginning of the line.
> 3. Copy `len` characters from `s` to `el->el_line.buffer` onward, one
>    at a time. The destination pointer is taken AFTER step 2, so a
>    reallocation there is handled correctly.
> 4. Write `L'\0'` at `el->el_line.buffer[len]`. This is in bounds
>    because `limit` always leaves two unused slots at the end of the
>    allocation.
> 5. `el->el_line.lastchar = el->el_line.buffer + len`.
> 6. If `el->el_line.cursor > lastchar`, set `cursor = lastchar`.
>    Otherwise the cursor keeps its absolute offset into the line: it is
>    clamped downward only, never moved to the end or to the start.
> 7. Return 0.
>
> Any previous content beyond `len` stays in the buffer above `lastchar`
> and is not visible. No redisplay is triggered.

> [spec:libedit:def:chared.el-zfunc-t-edit-line-void]
> typedef void (*el_zfunc_t)(EditLine *, void *)

