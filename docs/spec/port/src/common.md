# src/common.c

> [spec:libedit:def:common.ed-argument-digit-fn]
> libedit_private el_action_t ed_argument_digit(EditLine *el, wint_t c)

> [spec:libedit:sem:common.ed-argument-digit-fn]
> Accumulates a decimal digit into the pending repeat count. Bound to
> the digit keys reached through the meta/ESC prefix (`ESC 0` .. `ESC 9`)
> and called by `vi_zero` when a count is already being entered. It
> never inserts anything into the line.
>
> 1. If `iswdigit(c)` is false, return `CC_ERROR` and change nothing.
> 2. If `el->el_state.doingarg` is already set (a count is in progress):
>    if `el->el_state.argument > 1000000`, return `CC_ERROR` leaving
>    `argument` and `doingarg` untouched; otherwise set
>    `argument = argument * 10 + (c - '0')`.
> 3. Otherwise (no count in progress) set `argument = c - '0'` and
>    `doingarg = 1`. Note that this *replaces* whatever `argument` held;
>    it does not multiply. A leading `0` therefore sets `argument` to 0.
> 4. Return `CC_ARGHACK` on both accumulate paths.
>
> `CC_ARGHACK` is the signal to `el_wgets` to `continue` the read loop
> without running the post-command reset, so `argument`/`doingarg`
> survive to the next keystroke. Every other return value (including the
> `CC_ERROR` paths above) lets `el_wgets` reset `argument` to 1 and
> `doingarg` to 0, discarding the count.
>
> The cap is tested *before* the multiply, so `argument` can reach at
> most `10000009` before a further digit is refused. There is no
> overflow check beyond that cap and none on the first digit.
>
> `c - '0'` subtracts ASCII `'0'` from the wide character. `iswdigit` is
> locale-dependent and in some locales is true for non-ASCII decimal
> digits, for which `c - '0'` yields a meaningless value; the C does not
> guard against this. In the C/POSIX locale only U+0030..U+0039 pass.
>
> This differs from `ed_digit` in two ways: it never falls through to
> `ed_insert`, and it has no special case for a preceding
> `em_universal_argument`.

> [spec:libedit:def:common.ed-clear-screen-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_clear_screen(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-clear-screen-fn]
> Clears the physical screen and redraws the prompt and the current line
> at the top of it. Bound to `^L` in emacs and in the vi command map.
>
> 1. `terminal_clear_screen(el)` — emit the terminal's clear-screen
>    capability (or, if the terminal has none, the fallback that
>    function defines).
> 2. `re_clear_display(el)` — reset the refresh module's model of what
>    is on screen (cursor believed to be at row 0 column 0, no lines
>    considered drawn), so the next refresh redraws everything rather
>    than diffing against a stale image.
> 3. Return `CC_REFRESH`, which makes `el_wgets` call `re_refresh(el)`
>    and repaint prompt plus line.
>
> The line buffer, cursor, kill buffer, undo state and history position
> are all untouched. `el->el_state.argument` is ignored entirely: there
> is no repeat and no error path. `c` is unused.

> [spec:libedit:def:common.ed-command-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_command(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-command-fn]
> Prompts for and executes one editline built-in command line — the same
> command language as `.editrc` (`bind`, `settc`, `setty`, `echotc`,
> `history`, `telltc`, ...). Bound to `:` in the vi command map; the
> emacs map reaches it through `M-X` style bindings only if the user
> binds it.
>
> 1. Declare a local `wchar_t tmpbuf[EL_BUFSIZ]` (1024 wide chars),
>    uninitialised.
> 2. `tmplen = c_gets(el, tmpbuf, L"\n: ")`. `c_gets` takes over the
>    edit line: it writes the literal prompt `"\n: "` into
>    `el->el_line.buffer`, then reads characters one at a time,
>    echoing them after the prompt and calling `re_refresh` on every
>    iteration. It collects the typed characters into `tmpbuf`,
>    terminates on ESC (0x1B), CR or LF (storing that terminator at
>    `tmpbuf[len]` but not counting it in `len`), treats `\b`/DEL as a
>    one-character rubout, returns -1 if a rubout is attempted at
>    length 0 or if the read fails (in which case `c_gets` itself calls
>    `ed_end_of_file`), and beeps and drops characters once
>    `len >= EL_BUFSIZ - 16`. Before returning, `c_gets` **clears the
>    edit line**: `buffer[0] = '\0'`, `lastchar = cursor = buffer`. So
>    whatever the user was editing is destroyed by invoking this
>    command, unconditionally, whether or not the command line parses.
> 3. `terminal__putc(el, '\n')` — emit a newline so the executed command
>    does not sit on the prompt line.
> 4. If `tmplen < 0`, beep (`terminal_beep`). Otherwise set
>    `tmpbuf[tmplen] = 0` (overwriting the terminator `c_gets` left
>    there) and call `parse_line(el, tmpbuf)`; if that returns exactly
>    -1, beep. The `tmpbuf[tmplen] = 0` store is inside a comma
>    expression on the right-hand side of `||`, so it is evaluated only
>    when `tmplen >= 0` — an empty line (`tmplen == 0`) does get
>    NUL-terminated at index 0.
> 5. `el->el_map.current = el->el_map.key` — force the primary keymap
>    back on. In vi this leaves command mode and returns to insert mode,
>    which is how `:` behaves after the command runs.
> 6. `re_clear_display(el)` — forget the on-screen image, since the
>    command's own output may have scrolled it.
> 7. Return `CC_REFRESH`.
>
> Only `-1` from `parse_line` beeps. `parse_line` tokenises with
> `tok_wstr` and returns `el_wparse`'s value, which is -1 when there are
> no tokens or the command name matches no built-in, 0 when a `prog:`
> prefix fails to match `el->el_prog` (or on an allocation failure), and
> otherwise the *negation* of the built-in's own return value — so a
> built-in that reports failure as -1 yields +1 and does **not** beep.
> Unrecognised commands beep; failing commands generally do not.
>
> `el->el_state.argument` is ignored; there is no error return path,
> `CC_REFRESH` is returned on every branch.

> [spec:libedit:def:common.ed-delete-next-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_delete_next_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-delete-next-char-fn]
> Deletes `el->el_state.argument` characters starting at the cursor.
> Bound to `x` in the vi command map (emacs `^D` goes to
> `em_delete_or_list`, not here).
>
> The end-of-line guard, and only it, is mode-dependent. `KSHVI` is
> `#define`d unconditionally in `el.h` and there is no build switch that
> can unset it, so the port implements the KSHVI variant and the
> non-KSHVI code (which would write the character out with
> `terminal_writec` and return `CC_EOF`) is dead and is **not** ported.
>
> 1. If `el->el_line.cursor == el->el_line.lastchar` (nothing under the
>    cursor):
>    - If `el->el_map.type == MAP_VI`:
>      - and the line is also empty (`cursor == buffer`): return
>        `CC_ERROR`.
>      - otherwise (cursor sits past the last character of a non-empty
>        line): `el->el_line.cursor--`, then fall through to the delete.
>        This is what makes `x` at end of line in vi delete the last
>        character rather than failing.
>    - Else (emacs): return `CC_ERROR`.
> 2. `c_delafter(el, el->el_state.argument)`. That helper clamps the
>    count to `lastchar - cursor`, so an argument larger than the tail
>    just deletes to end of line and never errors. Because of the
>    always-true `el_map.current != el_map.emacs` test inside
>    `c_delafter` (documented under that function's own rule), it
>    also takes a full-line vi undo snapshot via `cv_undo` and copies
>    the deleted text into the kill buffer via `cv_yank` — in emacs mode
>    as well as vi. The cursor is not moved by `c_delafter`;
>    `lastchar` drops by the clamped count.
> 3. Vi bounds fix-up: if `el->el_map.type == MAP_VI` and
>    `cursor >= lastchar` and `cursor > buffer`, set
>    `cursor = lastchar - 1`, so the vi cursor stays on a real character
>    after deleting the tail of the line. In emacs the cursor is left
>    wherever it was, which may be exactly `lastchar`.
> 4. Return `CC_REFRESH`.
>
> Worked examples (vi, argument 1): line `abc` with the cursor on `c`
> deletes `c`, leaving `ab` with the cursor on `b`. Line `abc` with the
> cursor past `c` steps back to `c`, deletes it, and lands on `b`. Line
> `abc` cursor on `a` with argument 5 deletes all three characters and
> the fix-up clamps the cursor to `buffer`.
>
> The only `CC_ERROR` paths are the two end-of-line cases in step 1;
> everything else returns `CC_REFRESH`, including a no-op delete of zero
> characters when `argument` is 0. The `DEBUG_EDIT` `fprintf` at the top
> of the C is diagnostic only and is not part of the behaviour.

> [spec:libedit:def:common.ed-delete-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_delete_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-delete-prev-char-fn]
> Deletes `el->el_state.argument` characters immediately to the left of
> the cursor and moves the cursor left by the same amount. Mode
> independent.
>
> 1. If `el->el_line.cursor <= el->el_line.buffer`, return `CC_ERROR`.
>    Nothing is touched — this covers both an empty line and a cursor
>    already at the start of a non-empty one.
> 2. `c_delbefore(el, el->el_state.argument)`. That helper clamps the
>    count to `cursor - buffer`, so an argument larger than the text to
>    the left deletes just that text. As documented under that
>    function's own rule, its `el_map.current != el_map.emacs` test is
>    always true, so this also
>    snapshots the whole line for vi undo (`cv_undo`) and copies the
>    deleted characters into the kill buffer (`cv_yank`), in emacs mode
>    as much as in vi. `c_delbefore` deliberately does not move the
>    cursor.
> 3. `el->el_line.cursor -= el->el_state.argument`, using the *raw,
>    unclamped* argument, then clamp: if `cursor < buffer`, set
>    `cursor = buffer`. The two clamps agree — when the argument
>    overshoots, `c_delbefore` deletes exactly `cursor - buffer`
>    characters and the cursor is clamped to `buffer`, which is where
>    the surviving text now starts. There is no off-by-one here.
> 4. Return `CC_REFRESH`.
>
> An argument of 0 passes the step-1 guard, deletes nothing, still takes
> the undo snapshot and still empties the kill buffer
> (`c_kill.last == c_kill.buf`), leaves the cursor put, and returns
> `CC_REFRESH`.

> [spec:libedit:def:common.ed-delete-prev-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_delete_prev_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-delete-prev-word-fn]
> Deletes backwards from the cursor to the start of the
> `el->el_state.argument`-th preceding word, putting the deleted text in
> the kill buffer. Bound to `^W` in the emacs map and in both vi maps.
> Mode independent — notably it uses the **emacs** word test even in vi.
>
> 1. If `el->el_line.cursor == el->el_line.buffer`, return `CC_ERROR`
>    with no other effect.
> 2. `cp = c__prev_word(el, cursor, buffer, el->el_state.argument,
>    ce__isword)`. As documented under that function's own rule, it
>    starts one character left of the cursor and, `argument` times,
>    skips backwards over non-word characters and then backwards over
>    word characters, finally stepping forward one and clamping to
>    `buffer`. A "word character" here is `ce__isword`, i.e.
>    `iswalnum(ch)` or `ch` appears in `el->el_map.wordchars` (the
>    user-settable separator set, default `*?_-.[]~=`). `cv__isword`
>    and `cv__isWord`, the vi word tests, are *not* used, so `^W` in vi
>    mode has emacs word semantics.
> 3. Copy the characters in `[cp, cursor)` into
>    `el->el_chared.c_kill.buf` one at a time and set
>    `el->el_chared.c_kill.last` to just past the last one copied. The
>    kill buffer is not NUL-terminated; `c_kill.last` is the only length
>    record. There is no capacity check, but the kill buffer is
>    allocated and reallocated to the same size as the line buffer, and
>    the copied range is a subrange of the line, so it always fits.
> 4. `c_delbefore(el, (int)(cursor - cp))` to remove the text. Because
>    that helper's `el_map.current != el_map.emacs` test is always true,
>    it takes a `cv_undo` snapshot of the whole line and then does its
>    own `cv_yank` over the *same* range — rewriting the kill buffer
>    with byte-identical content, so step 3's copy is observably
>    redundant. The undo snapshot is taken in emacs mode too.
> 5. `el->el_line.cursor = cp`, then the redundant bounds check
>    `if (cursor < buffer) cursor = buffer` (`c__prev_word` already
>    clamps to `buffer`, so this never fires).
> 6. Return `CC_REFRESH`.
>
> If `argument` exceeds the number of words to the left, `c__prev_word`
> clamps to `buffer` and the whole prefix of the line is deleted with the
> cursor landing at the start; there is no error.
>
> With `argument == 0`, `c__prev_word` performs no iterations and
> returns the cursor unchanged, so nothing is deleted, the kill buffer
> is emptied, an undo snapshot is still taken, and `CC_REFRESH` is still
> returned. A negative argument is not reachable through the key
> dispatcher; `c__prev_word`'s `while (n--)` would spin roughly `2^32`
> trivial iterations before terminating.

> [spec:libedit:def:common.ed-digit-fn]
> libedit_private el_action_t ed_digit(EditLine *el, wint_t c)

> [spec:libedit:sem:common.ed-digit-fn]
> The handler bound to the plain digit keys `0`-`9` in the emacs map. It
> either extends a repeat count already being entered or types the digit
> into the line.
>
> 1. If `iswdigit(c)` is false, return `CC_ERROR` and change nothing.
>    (This can only happen if the user rebinds `ed-digit` to a non-digit
>    key.)
> 2. If `el->el_state.doingarg` is false — no count in progress — return
>    `ed_insert(el, c)`, inserting the digit as ordinary text with all of
>    that function's semantics (including the current `argument` as its
>    repeat count).
> 3. If `doingarg` is true:
>    - If `el->el_state.lastcmd == EM_UNIVERSAL_ARGUMENT` (the previous
>      command was `^U`, which set `argument` to a multiple of 4), set
>      `argument = c - '0'`, *replacing* the universal argument rather
>      than appending to it. So `^U 5` means 5, not 45.
>    - Otherwise: if `argument > 1000000` return `CC_ERROR` (leaving
>      `argument` and `doingarg` as they were), else
>      `argument = argument * 10 + (c - '0')`.
>    - Return `CC_ARGHACK`, which makes `el_wgets` skip its
>      post-command reset so `argument`/`doingarg` carry to the next key.
>
> Note the asymmetry: only the *immediately* preceding
> `em_universal_argument` triggers the replace; a second digit sees
> `lastcmd == ED_DIGIT` and appends normally, so `^U 1 2` gives 12.
>
> The `> 1000000` cap is checked before multiplying, so `argument` tops
> out at `10000009`. As in `ed_argument_digit`, `c - '0'` subtracts
> ASCII `'0'` and is meaningless for the non-ASCII digits `iswdigit`
> accepts in some locales; the C does not guard this.
>
> Returns: `CC_ERROR` on a non-digit or at the cap; `CC_ARGHACK` when
> accumulating; otherwise whatever `ed_insert` returns (`CC_NORM`,
> `CC_ERROR`, or `CC_CURSOR` in vi `MODE_REPLACE_1`).

> [spec:libedit:def:common.ed-end-of-file-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_end_of_file(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-end-of-file-fn]
> Signals end of input. Also called internally by `ed_quoted_insert` and
> by `c_gets` when a read fails.
>
> 1. `re_goto_bottom(el)` — move the physical cursor to the last screen
>    line the refresh module knows it used (`el_refresh.r_oldcv`), emit
>    a `'\n'`, reset the on-screen model with `re_clear_display`, and
>    flush. The effect is that the terminal cursor ends on a fresh line
>    below the edited line.
> 2. `*el->el_line.lastchar = '\0'` — NUL-terminate the line where it
>    currently ends. This writes *at* `lastchar`, which is always in
>    bounds because `limit` is set two slots below the end of the
>    allocation. Neither `lastchar` nor `cursor` is moved, and the line
>    contents are otherwise untouched.
> 3. Return `CC_EOF`.
>
> `el->el_state.argument` is ignored and `c` is unused; there is no
> error path. What `CC_EOF` then means is `el_wgets`'s business: in the
> normal buffered mode it sets the read count to 0 so `el_wgets`
> returns `NULL`; in `UNBUFFERED` mode, if nothing has been returned
> yet, it appends a literal `^D` (0x04) to the line, moves the cursor to
> the new `lastchar`, and returns that one character.

> [spec:libedit:def:common.ed-ignore-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_ignore(EditLine *el __attribute__((__unused__)), wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-ignore-fn]
> Does nothing at all and returns `CC_NORM`. Neither argument is read
> and no field of `EditLine` is touched — not the line, not the cursor,
> not the count.
>
> This is the binding for keys that must be swallowed silently rather
> than rejected: `^C ^O ^Q ^S ^Z ^\ ^]` in the emacs map and
> `^C ^O ^Q ^S ^\` in the vi maps. The distinction from `ed_unassigned`
> is exactly the return value — `CC_NORM` leaves `el_wgets` doing
> nothing, whereas `CC_ERROR` makes it beep.

> [spec:libedit:def:common.ed-insert-fn]
> libedit_private el_action_t ed_insert(EditLine *el, wint_t c)

> [spec:libedit:sem:common.ed-insert-fn]
> Puts `el->el_state.argument` copies of the character `c` into the line
> at the cursor. Bound to every self-inserting key. Its behaviour is
> driven by `el->el_state.inputmode`, which is `MODE_INSERT` (0),
> `MODE_REPLACE` (1, vi `R`) or `MODE_REPLACE_1` (2, vi `r`).
>
> Let `count` be a snapshot of `el->el_state.argument` taken on entry.
>
> 1. If `c == L'\0'`, return `CC_ERROR` immediately, with no side
>    effects. A NUL can never be inserted.
> 2. Buffer space: if `el->el_line.lastchar + argument >=
>    el->el_line.limit`, call `ch_enlargebufs(el, (size_t)count)`; if it
>    returns 0 (allocation failed), return `CC_ERROR`. A successful
>    enlargement reallocates and repoints `buffer`, `cursor`,
>    `lastchar`, `limit`, the kill buffer, the undo and redo buffers and
>    the history buffer, and runs the client resize callback.
> 3. If `count == 1`:
>    - If `inputmode == MODE_INSERT` **or** `cursor >= lastchar`, call
>      `c_insert(el, 1)`, which shifts `[cursor, lastchar]` one slot
>      right and increments `lastchar`, opening a hole. In the two
>      replace modes with the cursor still inside the text no hole is
>      made and the existing character is overwritten. Note the
>      `cursor >= lastchar` disjunct: at end of line even a replace mode
>      appends rather than clobbering past the end.
>    - `*el->el_line.cursor++ = c` — store and step right.
>    - `re_fastaddc(el)` — the cheap single-character screen update.
> 4. If `count != 1`:
>    - If `inputmode != MODE_REPLACE_1`, call
>      `c_insert(el, el->el_state.argument)`, opening a hole of
>      `argument` slots and raising `lastchar` by that much.
>      Consequently `MODE_REPLACE` with a count greater than 1 behaves
>      as an *insert*, not a replace — only `MODE_REPLACE_1` genuinely
>      overwrites when repeated.
>    - Loop `while (count-- && cursor < lastchar) *cursor++ = c`. The
>      post-decrement means the loop body runs exactly `count` times
>      (for `count > 0`), subject to the `cursor < lastchar` guard. When
>      the hole was opened in the previous step that guard never bites;
>      in `MODE_REPLACE_1` it caps the writes at the number of
>      characters actually remaining to the right, so `3r` near end of
>      line quietly replaces fewer than three and does not error. It
>      also protects the case where `c_insert` silently failed to grow
>      the buffer.
>    - `re_refresh(el)` — full redraw.
>    - A `count` of 0 takes this branch, opens a zero-width hole, writes
>      nothing, and still refreshes.
> 5. If `inputmode == MODE_REPLACE_1`, return `vi_command_mode(el, 0)`.
>    That clears `c_vcmd.action` to `NOP`, sets `c_vcmd.pos = NULL`,
>    clears `doingarg`, sets `inputmode = MODE_INSERT`, switches
>    `el_map.current` to `el_map.alt` (the vi command map), and — since
>    `VI_MOVE` is unconditionally defined in `chared.h` — steps the
>    cursor back one if it is above `buffer`. It returns `CC_CURSOR`,
>    which is therefore `ed_insert`'s return value on this path. That
>    backward step is what leaves the vi `r` cursor sitting *on* the
>    character it just replaced.
> 6. Otherwise return `CC_NORM`.
>
> Returns: `CC_ERROR` for a NUL character or a failed enlargement;
> `CC_CURSOR` when `inputmode == MODE_REPLACE_1`; `CC_NORM` in every
> other case. `el->el_map.type` is never consulted — the mode
> dependency here is entirely through `inputmode`.

> [spec:libedit:def:common.ed-kill-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_kill_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-kill-line-fn]
> Cuts everything from the cursor to the end of the line into the kill
> buffer. Bound to `^K` in the emacs map and in the vi insert map. Mode
> independent, and it does **not** go through `c_delbefore`/`c_delafter`.
>
> 1. Copy the characters in `[cursor, lastchar)` one at a time into
>    `el->el_chared.c_kill.buf`, then set `el->el_chared.c_kill.last` to
>    just past the last one copied. The kill buffer is not
>    NUL-terminated; `c_kill.last` carries the length. No capacity check
>    is performed and none is needed: the kill buffer is always
>    allocated and reallocated to the same size as the line buffer.
> 2. `el->el_line.lastchar = el->el_line.cursor` — truncate the line.
>    The killed characters are left physically in the line buffer above
>    the new `lastchar`, which is unobservable but means the buffer tail
>    is stale rather than zeroed.
> 3. Return `CC_REFRESH`.
>
> `el->el_line.cursor` is not moved. `el->el_state.argument` is ignored
> entirely — `^K` never repeats. No vi undo snapshot is taken and
> `c_undo` is left as it was, so this is one of the deletions vi `u`
> cannot restore. There is no error path: with the cursor already at
> `lastchar` this kills nothing, sets `c_kill.last == c_kill.buf`
> (emptying the kill buffer), and still returns `CC_REFRESH`.

> [spec:libedit:def:common.ed-move-to-beg-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_move_to_beg(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-move-to-beg-fn]
> Moves the cursor to the start of the line. Bound to `^A` in emacs and
> in the vi maps.
>
> 1. `el->el_line.cursor = el->el_line.buffer`.
> 2. If `el->el_map.type == MAP_VI`, advance the cursor past leading
>    whitespace: `while (iswspace(*cursor)) cursor++`. This gives vi's
>    "first non-blank" semantics (`^`) rather than column zero. In emacs
>    mode this step is skipped and the cursor stays at `buffer`.
> 3. Still in vi mode, if `el->el_chared.c_vcmd.action != NOP` — a
>    pending operator such as `d` or `c` is waiting for a motion — call
>    `cv_delfini(el)` to apply that operator over the range between
>    `c_vcmd.pos` and the new cursor, and return `CC_REFRESH`.
> 4. Otherwise return `CC_CURSOR`.
>
> `el->el_state.argument` is ignored; there is no error path, so an
> empty line returns `CC_CURSOR` (or `CC_REFRESH`) just the same.
>
> BUG: the whitespace skip in step 2 has no upper bound. It does not
> stop at `lastchar` and it does not stop at the end of the allocation;
> it only stops at the first non-whitespace wide character it reads.
> Whatever lies above `lastchar` in the line buffer is stale data left
> by earlier edits, not reliably a NUL. Concretely, on a line of
> `"   abc"` with the cursor at column 0, `ed_kill_line` sets
> `lastchar = buffer` while leaving `"   abc"` physically in the buffer;
> a following vi `^` then skips the three stale spaces and parks the
> cursor at `buffer + 3`, three positions past `lastchar` on what the
> user sees as an empty line. A line consisting entirely of whitespace
> similarly leaves the cursor at or past `lastchar`. In the worst case
> the scan runs off the end of the allocation. The port must bound this
> scan at `lastchar`; the C's behaviour past that point is not a defined
> semantic to reproduce.

> [spec:libedit:def:common.ed-move-to-end-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_move_to_end(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-move-to-end-fn]
> Moves the cursor to the end of the line. Bound to `^E` in emacs and in
> the vi maps.
>
> 1. `el->el_line.cursor = el->el_line.lastchar`.
> 2. If `el->el_map.type == MAP_VI`:
>    - If `el->el_chared.c_vcmd.action != NOP` (a vi operator such as
>      `d` or `c` is pending), call `cv_delfini(el)` to apply it over
>      the range from `c_vcmd.pos` to the new cursor, and return
>      `CC_REFRESH`. The `VI_MOVE` step below is *not* executed on this
>      path, so the operator's range extends to `lastchar` inclusive of
>      the last character.
>    - Otherwise, because `VI_MOVE` is unconditionally defined in
>      `chared.h`, step the cursor back one if `cursor > buffer`. This
>      leaves the vi cursor *on* the last character rather than past it,
>      which is where vi expects it. On an empty line the guard keeps
>      the cursor at `buffer`.
> 3. Return `CC_CURSOR`.
>
> In emacs mode neither the operator check nor the back-step happens:
> the cursor is left exactly at `lastchar`.
>
> `el->el_state.argument` is ignored and there is no error path.

> [spec:libedit:def:common.ed-newline-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_newline(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-newline-fn]
> Accepts the line and hands it to the caller. Bound to `^J` and `^M` in
> the emacs map and in both vi maps.
>
> 1. `re_goto_bottom(el)` — move the physical cursor to the last screen
>    line the refresh module used, emit a `'\n'`, reset the on-screen
>    model, flush. This is what leaves the terminal cursor below the
>    accepted line.
> 2. `*el->el_line.lastchar++ = '\n'` — append a literal newline to the
>    line buffer and advance `lastchar` past it. The returned line
>    therefore **includes** the trailing `'\n'`.
> 3. `*el->el_line.lastchar = '\0'` — NUL-terminate after it.
> 4. Return `CC_NEWLINE`, on which `el_wgets` sets the character count
>    to `lastchar - buffer` (counting the newline) and leaves the read
>    loop.
>
> There is no bounds check on the two stores, and none is needed:
> `lastchar` never exceeds `limit`, and `limit` is fixed two slots below
> the end of the line allocation precisely to leave room for them.
>
> `el->el_line.cursor` is not moved, `el->el_state.argument` is ignored,
> and there is no error path — `CC_NEWLINE` is returned unconditionally.
> An empty line yields a one-character result containing just `"\n"`.

> [spec:libedit:def:common.ed-next-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_next_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-next-char-fn]
> Moves the cursor `el->el_state.argument` characters to the right.
> Bound to `^F` in emacs and in the vi maps (and reached by vi `l`
> through its own binding).
>
> Let `lim = el->el_line.lastchar`, captured on entry.
>
> 1. Refuse the move and return `CC_ERROR` if either:
>    - `cursor >= lim` — already at (or past) the end of the line, which
>      includes the empty-line case where `cursor == lim == buffer`; or
>    - `cursor == lim - 1` **and** `el->el_map.type == MAP_VI` **and**
>      `el->el_chared.c_vcmd.action == NOP`. That is: in vi command mode
>      with no operator pending, sitting on the last character, moving
>      right fails, because the vi cursor must stay on a real character.
>      With an operator pending (`d l`, `c l`, ...) the move *is*
>      allowed, so the operator can consume the final character.
> 2. `el->el_line.cursor += el->el_state.argument`.
> 3. Clamp above only: `if (cursor > lim) cursor = lim`. Note the clamp
>    is to `lastchar`, not `lastchar - 1`, even in vi mode — so a count
>    that overshoots (`5l` with three characters left) leaves the vi
>    cursor one past the last character, which the guard in step 1
>    otherwise forbids. There is no clamp below, so a zero or negative
>    argument moves the cursor backwards unchecked; a negative argument
>    is not reachable through the key dispatcher, and a zero argument
>    simply leaves the cursor where it was.
> 4. If `el->el_map.type == MAP_VI` and `c_vcmd.action != NOP`, call
>    `cv_delfini(el)` to apply the pending operator over
>    `[c_vcmd.pos, cursor)` and return `CC_REFRESH`.
> 5. Otherwise return `CC_CURSOR`.
>
> In emacs mode only the `cursor >= lim` guard applies, the operator
> logic is skipped entirely, and the result is always `CC_ERROR` or
> `CC_CURSOR`.

> [spec:libedit:def:common.ed-next-history-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_next_history(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-next-history-fn]
> Moves `el->el_state.argument` entries *forward* through history
> (towards more recent lines, ending at the line the user was typing).
> Bound to `^N` in emacs and `j` in the vi command map. Unlike
> `ed_prev_history` it has no mode-dependent behaviour of its own.
>
> `el->el_history.eventno` is the position: 0 means the live line the
> user was editing (stashed in `el->el_history.buf`), 1 the most recent
> history entry, larger numbers older entries.
>
> 1. `el->el_chared.c_undo.len = -1` — invalidate the vi undo snapshot;
>    history motion is not undoable.
> 2. `*el->el_line.lastchar = '\0'` — defensively terminate the line
>    before it is overwritten.
> 3. `el->el_history.eventno -= el->el_state.argument`.
> 4. If `eventno` has gone negative, clamp it to 0 and arrange to beep
>    (a local starts at `CC_REFRESH` and becomes `CC_REFRESH_BEEP`
>    here). Walking forward past the newest entry lands on the saved
>    live line and beeps rather than erroring.
> 5. `rval = hist_get(el)`. That loads event `eventno` into the line
>    buffer: for `eventno == 0` it copies `el->el_history.buf` back and
>    sets `lastchar` from `el->el_history.last`; otherwise it walks
>    `HIST_FIRST` then `eventno - 1` `HIST_NEXT`s, copies the entry in
>    (growing the buffer if needed) and trims one trailing `'\n'` then
>    one trailing `' '`. Cursor placement inside `hist_get` is
>    mode-dependent, `KSHVI` being defined: in `MAP_VI` the cursor goes
>    to `buffer`, in emacs to `lastchar`. On failure `hist_get` sets
>    `eventno` to the last index it could reach and returns `CC_ERROR`.
> 6. If `rval == CC_REFRESH`, return the beep-or-not local
>    (`CC_REFRESH` normally, `CC_REFRESH_BEEP` if step 4 clamped).
>    Otherwise return `rval` unchanged — in practice `CC_ERROR`, which
>    also discards the pending beep.
>
> Note that this function never saves the current line into
> `el->el_history.buf`; only `ed_prev_history` and
> `ed_search_prev_history` do. If `^N` is pressed with `eventno` already
> 0 and no prior `^P`, `eventno` clamps to 0, `hist_get` copies back
> whatever `history.buf` holds — a zero-filled buffer with
> `history.last == history.buf` after `hist_init`, i.e. an empty line —
> and the current line is silently wiped, with `CC_REFRESH_BEEP`
> returned only if the argument was non-zero. With `argument == 0`
> nothing is clamped and `hist_get` simply reloads the same entry.

> [spec:libedit:def:common.ed-next-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_next_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-next-line-fn]
> Moves the cursor down one *embedded* line, i.e. across a literal
> `'\n'` inside the edit buffer. This is about multi-line input, not
> about screen wrapping. Not bound by default in either map; reachable
> only if the user binds `ed-next-line`. Mode independent.
>
> 1. `nchars = c_hpos(el)` — the current column, defined as the number
>    of characters between the cursor and the nearest preceding `'\n'`
>    (or `buffer` if there is none); 0 when `cursor == buffer`.
> 2. Scan forward for the `argument`-th newline: for `ptr` from `cursor`
>    while `ptr < lastchar`, if `*ptr == '\n'` and
>    `--el->el_state.argument <= 0`, break. Note this **decrements
>    `el->el_state.argument` in place**, once per newline crossed.
> 3. If `el->el_state.argument > 0` — the scan reached `lastchar`
>    without finding enough newlines — return `CC_ERROR` with the cursor
>    unchanged. `argument` is left partially decremented, which is
>    harmless because `el_wgets` resets it to 1 after the command.
> 4. Move to the target column: `ptr++` steps past the newline onto the
>    first character of the next line, then advance while
>    `nchars-- > 0` and `ptr < lastchar` and `*ptr != '\n'`. So the
>    cursor lands at the same column if that line is long enough, and
>    otherwise at that line's end (just before its `'\n'`, or at
>    `lastchar`).
> 5. `el->el_line.cursor = ptr`; return `CC_CURSOR`.
>
> Unlike `ed_prev_line`, there is no adjustment for a cursor already
> sitting *on* a `'\n'`: that newline is counted as the first one
> crossed, so pressing this at the end of a line moves down one line
> rather than two.
>
> Undefined edge: with `argument <= 0` on entry and no newline between
> the cursor and `lastchar`, step 2 exits with `ptr == lastchar` and
> `argument == 0`, step 3 does not fire, and step 4's `ptr++` puts
> `ptr` at `lastchar + 1` before its guard rejects the loop — leaving
> `cursor == lastchar + 1`, one past the end of the line. That slot is
> inside the allocation (two spare slots below `limit`) but the cursor
> value is invalid. The port must not reproduce this; treat a
> non-positive argument as producing no movement.

> [spec:libedit:def:common.ed-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-prev-char-fn]
> Moves the cursor `el->el_state.argument` characters to the left. Bound
> to `^B` in emacs and in the vi maps (vi `h` reaches it through its own
> binding).
>
> 1. If `el->el_line.cursor <= el->el_line.buffer`, return `CC_ERROR`
>    and change nothing. This covers both an empty line and a cursor
>    already at the start.
> 2. `el->el_line.cursor -= el->el_state.argument`, then clamp below: if
>    `cursor < buffer`, set `cursor = buffer`. An argument larger than
>    the distance to the start therefore lands on `buffer` and does not
>    error. There is no clamp above, so a zero argument is a no-op and a
>    negative one would move right unchecked (not reachable through the
>    key dispatcher).
> 3. If `el->el_map.type == MAP_VI` and `el->el_chared.c_vcmd.action !=
>    NOP` (a vi operator is pending), call `cv_delfini(el)` to apply it
>    over the range between `c_vcmd.pos` and the new cursor, and return
>    `CC_REFRESH`.
> 4. Otherwise return `CC_CURSOR`.
>
> In emacs mode step 3 is skipped entirely, so the result is always
> `CC_ERROR` or `CC_CURSOR`. This function is exactly symmetric with
> `ed_next_char` except that it has no vi "must stay on a character"
> guard — moving left onto `buffer` is always legal in vi.

> [spec:libedit:def:common.ed-prev-history-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_prev_history(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-prev-history-fn]
> Moves `el->el_state.argument` entries *back* through history (towards
> older lines). Bound to `^P` in emacs and `k` in the vi command map.
> `el->el_history.eventno` is 0 for the live line being typed, 1 for the
> most recent history entry, larger for older ones.
>
> 1. Remember `sv_event = el->el_history.eventno`.
> 2. `el->el_chared.c_undo.len = -1` — invalidate the vi undo snapshot.
> 3. `*el->el_line.lastchar = '\0'` — defensively terminate the line.
> 4. If `eventno == 0`, i.e. we are leaving the live line, stash it:
>    `wcsncpy(el->el_history.buf, el->el_line.buffer,
>    el->el_history.sz)` and set `el->el_history.last =
>    el->el_history.buf + (lastchar - buffer)`. `wcsncpy` does not
>    NUL-terminate when the source fills the destination, but
>    `history.last` records the length so nothing downstream depends on
>    the terminator — except `ed_search_next_history`, which does (see
>    that rule). `history.sz` starts at `EL_BUFSIZ` and is grown
>    alongside the line buffer.
> 5. `el->el_history.eventno += el->el_state.argument`.
> 6. `hist_get(el)` loads that event into the line buffer. Its cursor
>    placement is mode-dependent (`KSHVI` being defined): `MAP_VI` puts
>    the cursor at `buffer`, emacs at `lastchar`. On failure it sets
>    `eventno` to the last index it could actually reach and returns
>    `CC_ERROR`.
> 7. If `hist_get` returned `CC_ERROR` (we asked for an event older than
>    the history holds):
>    - **Mode-dependent**: if `el->el_map.type == MAP_VI`, restore
>      `eventno = sv_event`, throwing away `hist_get`'s clamp. In emacs
>      mode the clamped value is kept. So in emacs `^P` past the top of
>      history leaves you on the *oldest* entry; in vi `k` past the top
>      leaves you on the entry you were already on.
>    - Set the beep flag and call `hist_get(el)` a second time to load
>      whichever event `eventno` now names. **This second return value
>      is discarded.** If it also fails — for instance when
>      `el->el_history.ref` is `NULL`, in which case `hist_get` returns
>      `CC_ERROR` without adjusting `eventno` at all and both calls fail
>      identically — the line buffer is left untouched, `eventno` is
>      left at the bumped value in emacs mode, and this function still
>      reports success-with-beep.
> 8. Return `CC_REFRESH_BEEP` if the beep flag was set, otherwise
>    `CC_REFRESH`. There is no `CC_ERROR` path out of this function at
>    all.
>
> With `argument == 0`, `eventno` does not change and `hist_get` simply
> reloads the current entry, resetting any edits made to it.

> [spec:libedit:def:common.ed-prev-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_prev_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-prev-line-fn]
> Moves the cursor up one *embedded* line, i.e. across a literal `'\n'`
> inside the edit buffer. This is about multi-line input, not about
> screen wrapping. Not bound by default in either map; reachable only if
> the user binds `ed-prev-line`. Mode independent.
>
> 1. `nchars = c_hpos(el)` — the current column: the number of
>    characters between the cursor and the nearest preceding `'\n'` (or
>    `buffer`), 0 when `cursor == buffer`.
> 2. `ptr = cursor`; if `*ptr == '\n'`, `ptr--`. This keeps a cursor
>    that is sitting *on* a newline from counting that newline as the
>    boundary it is looking for. When `cursor == lastchar` this reads
>    the slot at `lastchar`, which is inside the allocation but holds
>    stale data unless some earlier step wrote a terminator there — the
>    test's outcome in that case is not reliably defined.
> 3. Scan backwards for the `argument`-th newline: for `ptr` down to
>    `buffer`, if `*ptr == '\n'` and `--el->el_state.argument <= 0`,
>    break. This **decrements `el->el_state.argument` in place**, once
>    per newline crossed.
> 4. If `el->el_state.argument > 0` — not enough newlines above the
>    cursor — return `CC_ERROR` with the cursor unchanged. `argument` is
>    left partially decremented, which is harmless because `el_wgets`
>    resets it after the command. A single-line buffer with the default
>    argument of 1 always takes this path.
> 5. Walk back to the start of that preceding line: `ptr--`, then keep
>    decrementing while `ptr >= buffer && *ptr != '\n'`. `ptr` ends
>    either on the newline that precedes the target line or at
>    `buffer - 1`.
> 6. Move to the target column: `ptr++` (onto the first character of the
>    target line), then advance while `nchars-- > 0` and
>    `ptr < lastchar` and `*ptr != '\n'`. The cursor lands at the same
>    column if that line is long enough, otherwise at that line's end.
> 7. `el->el_line.cursor = ptr`; return `CC_CURSOR`.
>
> Undefined edge: with `argument <= 0` on entry and no `'\n'` anywhere
> at or before the cursor, step 3 runs off the front leaving
> `ptr == buffer - 1` while `argument == 0`, so step 4 does not fire.
> Step 5 then leaves `ptr == buffer - 2` and step 6's `ptr++` makes it
> `buffer - 1`, which the loop condition immediately dereferences — an
> out-of-bounds read below the start of the line buffer, after which the
> cursor may be set to `buffer - 1`. The port must not reproduce this;
> treat a non-positive argument as producing no movement. With
> `argument >= 1` the step-4 guard catches the same situation safely.

> [spec:libedit:def:common.ed-prev-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_prev_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-prev-word-fn]
> Moves the cursor back to the start of the `el->el_state.argument`-th
> preceding word. Bound to `M-b` in emacs and `b` in the vi command map.
>
> 1. If `el->el_line.cursor == el->el_line.buffer`, return `CC_ERROR`
>    with no other effect.
> 2. `el->el_line.cursor = c__prev_word(el, cursor, buffer,
>    el->el_state.argument, ce__isword)`. As documented under that
>    function's own rule, it starts one character to the left of the
>    cursor and, `argument` times, skips
>    backwards over non-word characters and then backwards over word
>    characters, finally stepping forward one and clamping to `buffer`.
>    The word test is `ce__isword` — `iswalnum(ch)` or `ch` present in
>    `el->el_map.wordchars` — the **emacs** notion of a word, used here
>    even when the vi command map is active. The vi tests `cv__isword`
>    (which distinguishes alphanumeric-plus-wordchars from other
>    printable characters) and `cv__isWord` are not used, so `b` in vi
>    does not split on punctuation the way real vi does.
> 3. If `el->el_map.type == MAP_VI` and `el->el_chared.c_vcmd.action !=
>    NOP` (a pending operator such as `d` or `c`), call `cv_delfini(el)`
>    to apply it between `c_vcmd.pos` and the new cursor, and return
>    `CC_REFRESH`.
> 4. Otherwise return `CC_CURSOR`.
>
> An argument larger than the number of words to the left clamps the
> cursor to `buffer`; it is not an error. An argument of 0 leaves the
> cursor exactly where it was and still returns `CC_CURSOR`. In emacs
> mode step 3 never runs, so the result is `CC_ERROR` or `CC_CURSOR`.

> [spec:libedit:def:common.ed-quoted-insert-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_quoted_insert(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-quoted-insert-fn]
> Reads one more character with the tty's special-character processing
> suppressed and inserts it literally. Bound to `^V` in the emacs map
> and in both vi maps. Mode independent.
>
> 1. `tty_quotemode(el)` — switch the tty to the `QU_IO` termios setting
>    (a copy of the editing settings with the special characters
>    disabled) so that the next byte arrives raw rather than being
>    interpreted as an interrupt, flow control, EOF and so on. Its
>    return value is discarded, so a failure to change the tty mode is
>    not reported and the read proceeds anyway.
> 2. `num = el_wgetc(el, &ch)` — read exactly one wide character. This
>    returns 1 on success (including when the character came from a
>    pending key macro), 0 if the tty could not be put into raw mode,
>    and negative on a read error.
> 3. `tty_noquotemode(el)` — restore the editing tty settings. Its
>    return value is likewise discarded. This runs before the branch, so
>    it happens on both the success and the failure path.
> 4. If `num == 1`, return `ed_insert(el, ch)` — the character is
>    inserted with the *current* `el->el_state.argument` as the repeat
>    count, so `ESC 4 ^V x` inserts four `x`s, and all of `ed_insert`'s
>    input-mode behaviour applies. In particular a read of a NUL makes
>    `ed_insert` return `CC_ERROR`, and in vi `MODE_REPLACE_1` the
>    return is `CC_CURSOR`.
> 5. Otherwise return `ed_end_of_file(el, 0)`, which goes to the bottom
>    of the display, NUL-terminates the line at `lastchar` and returns
>    `CC_EOF`. Both the "tty could not be set raw" case and the read
>    error case are reported as end of file; the distinction is lost.
>
> Returns: `CC_EOF` on any read failure; otherwise `ed_insert`'s value —
> `CC_NORM`, `CC_ERROR` (NUL character or a failed buffer enlargement),
> or `CC_CURSOR` (vi `MODE_REPLACE_1`).

> [spec:libedit:def:common.ed-redisplay-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_redisplay(EditLine *el __attribute__((__unused__)), wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-redisplay-fn]
> Does nothing itself and returns `CC_REDISPLAY`. Neither parameter is
> read and no field of `EditLine` is touched — all the work is in the
> return value.
>
> `CC_REDISPLAY` makes `el_wgets` call `re_clear_lines(el)` (erase the
> screen lines the current line occupies, working from the bottom up),
> then `re_clear_display(el)` (forget the on-screen image), and then
> fall through into the `CC_REFRESH` handling, `re_refresh(el)`, which
> repaints prompt and line from scratch.
>
> The difference from `ed_clear_screen` is that this erases only the
> lines belonging to the current input, leaving the scrollback above it
> alone, and it emits no terminal clear-screen capability. The
> difference from returning `CC_REFRESH` is the forced erase-and-forget
> instead of an incremental diff. `el->el_state.argument` is irrelevant
> and there is no error path.

> [spec:libedit:def:common.ed-search-next-history-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_search_next_history(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-search-next-history-fn]
> Searches *forward* through history (towards more recent entries) for
> an entry matching the current search pattern. Bound to `M-N` in emacs
> and `J` in the vi command map. `el->el_state.argument` is ignored —
> the search always advances to the nearest match, never `argument` of
> them.
>
> 1. `el->el_chared.c_vcmd.action = NOP` — drop any pending vi operator.
> 2. `el->el_chared.c_undo.len = -1` — invalidate the vi undo snapshot.
> 3. `*el->el_line.lastchar = '\0'` — terminate the line so it can be
>    used as a pattern source and as a comparison string.
> 4. If `el->el_history.eventno == 0`, return `CC_ERROR`: we are already
>    on the live line and there is nothing newer to find.
> 5. If `el->el_history.ref == NULL` (no history attached), return
>    `CC_ERROR`.
> 6. `hp = HIST_FIRST(el)` — position the history cursor on event 1, the
>    newest entry. If it is `NULL` (empty history), return `CC_ERROR`.
> 7. `c_setpat(el)` — set the search pattern, but only if
>    `el->el_state.lastcmd` is neither `ED_SEARCH_PREV_HISTORY` nor
>    `ED_SEARCH_NEXT_HISTORY`; that way a run of consecutive searches
>    reuses the pattern established by the first one. When it does run,
>    the pattern is `el->el_line.buffer` up to `EL_CURSOR(el)` —
>    the cursor, plus one when `el->el_map.type == MAP_VI` and
>    `el->el_map.current == el->el_map.alt`, so vi command mode includes
>    the character under the cursor. It is truncated to `EL_BUFSIZ - 1`
>    and NUL-terminated in `el->el_search.patbuf`.
> 8. Walk events 1 .. `eventno - 1` (all entries newer than the current
>    position): for `h` from 1 while `h < eventno` and `hp != NULL`, if
>    `hp` both differs from the current line and matches the pattern,
>    record `found = h`; then `hp = HIST_NEXT(el)`. **There is no
>    `break`** — the loop runs to completion and `found` therefore ends
>    up holding the *largest* matching `h`, which is the match nearest
>    to the current position and therefore the correct "next" one.
>    - "Differs from the current line" is
>      `wcsncmp(hp, buffer, lastchar - buffer) != 0 || hp[lastchar -
>      buffer] != 0`, i.e. the candidate differs somewhere in the first
>      `lastchar - buffer` characters, or is strictly longer. The
>      second test is reached only when the prefixes are equal, which
>      implies `hp` is at least that long, so the index is in range.
>    - "Matches the pattern" is `c_hmatch(el, hp)`, which is
>      `el_match(hp, patbuf)`: true if `patbuf` occurs anywhere in `hp`
>      as a substring, or failing that if `patbuf` compiles as a POSIX
>      basic regular expression that `regexec` matches against `hp`.
>      The match is **not** anchored despite the "matches the prefix"
>      comment on `c_hmatch`.
> 9. If nothing matched, fall back to the saved live line: if
>    `c_hmatch(el, el->el_history.buf)` is false, return `CC_ERROR`
>    (with `eventno` unchanged). If it is true, fall through with
>    `found` still 0.
> 10. `el->el_history.eventno = found` and return `hist_get(el)` —
>     `CC_REFRESH` when the entry loads, `CC_ERROR` if it does not.
>     `found == 0` selects the live line, restoring `el->el_history.buf`
>     into the edit buffer.
>
> Because `found` is initialised to 0 and 0 is also the live-line event
> number, the "no match, but the live line matches" case and the
> "matched event 0" case are the same code path; a real match always has
> `h >= 1`, so there is no ambiguity.
>
> This function never saves the current line into `el->el_history.buf`
> the way `ed_prev_history` and `ed_search_prev_history` do — it only
> reads it. If nothing ever stashed a live line, step 9 matches against
> the zero-filled buffer `hist_init` allocated. `el->el_history.buf` is
> filled by `wcsncpy` bounded by `el->el_history.sz`, which does not
> NUL-terminate when the saved line exactly fills it; `c_hmatch` then
> runs `wcsstr`/`regexec` over an unterminated buffer, an out-of-bounds
> read. The port should keep the saved line's length explicitly (it is
> already tracked in `el->el_history.last`) rather than relying on a
> terminator.

> [spec:libedit:def:common.ed-search-prev-history-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_search_prev_history(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-search-prev-history-fn]
> Searches *backward* through history (towards older entries) for an
> entry matching the current search pattern. Bound to `M-P` in emacs and
> `K` in the vi command map. `el->el_state.argument` is ignored — the
> search always stops at the first match, never the `argument`-th.
>
> 1. `el->el_chared.c_vcmd.action = NOP` — drop any pending vi operator.
> 2. `el->el_chared.c_undo.len = -1` — invalidate the vi undo snapshot.
> 3. `*el->el_line.lastchar = '\0'` — terminate the line.
> 4. If `el->el_history.eventno < 0` (should never happen), set it to 0
>    and return `CC_ERROR`.
> 5. If `el->el_history.eventno == 0`, stash the live line:
>    `wcsncpy(el->el_history.buf, el->el_line.buffer,
>    el->el_history.sz)` and `el->el_history.last = el->el_history.buf +
>    (lastchar - buffer)`. As in `ed_prev_history`, `wcsncpy` leaves the
>    destination unterminated when the source fills it.
> 6. If `el->el_history.ref == NULL`, return `CC_ERROR`.
> 7. `hp = HIST_FIRST(el)` — position on event 1, the newest entry. If
>    it is `NULL`, return `CC_ERROR`.
> 8. `c_setpat(el)` — establish the search pattern, skipped when
>    `el->el_state.lastcmd` is `ED_SEARCH_PREV_HISTORY` or
>    `ED_SEARCH_NEXT_HISTORY` so that repeated searches keep the
>    original pattern. The pattern is `el->el_line.buffer` up to
>    `EL_CURSOR(el)`: the cursor position, plus one when
>    `el->el_map.type == MAP_VI` and
>    `el->el_map.current == el->el_map.alt`, so vi command mode includes
>    the character under the cursor in the pattern. Truncated to
>    `EL_BUFSIZ - 1` and NUL-terminated into `el->el_search.patbuf`.
> 9. Skip past everything at or newer than the current position: for
>    `h` from 1 through `eventno` inclusive, `hp = HIST_NEXT(el)`. `h`
>    ends at `eventno + 1` and `hp` names that event — the first one
>    strictly older than where we are. There is **no NULL check inside
>    this loop**; if `eventno` exceeds the history length, `HIST_NEXT`
>    simply keeps returning `NULL` and `hp` ends `NULL`, which the next
>    step handles.
> 10. While `hp != NULL`: if `hp` both differs from the current line and
>     matches the pattern, set `found = 1` and break; otherwise `h++`
>     and `hp = HIST_NEXT(el)`.
>     - "Differs from the current line" is
>       `wcsncmp(hp, buffer, lastchar - buffer) != 0 ||
>       hp[lastchar - buffer] != 0` — the candidate differs in the first
>       `lastchar - buffer` characters, or is strictly longer. The
>       index into `hp` is only evaluated when the prefixes compare
>       equal, which guarantees `hp` is at least that long.
>     - "Matches the pattern" is `c_hmatch(el, hp)` =
>       `el_match(hp, patbuf)`: true if `patbuf` occurs anywhere in `hp`
>       as a substring, or failing that if `patbuf` compiles as a POSIX
>       basic regular expression matching `hp`. Unanchored, despite the
>       "matches the prefix" comment on `c_hmatch`.
> 11. If nothing matched, return `CC_ERROR` with `eventno` unchanged
>     (apart from the step-4 and step-5 effects).
> 12. Otherwise `el->el_history.eventno = h` and return `hist_get(el)`
>     — `CC_REFRESH` when the entry loads, `CC_ERROR` if it does not.
>     `hist_get` places the cursor at `buffer` in `MAP_VI` and at
>     `lastchar` in emacs (`KSHVI` is defined).
>
> Note the "differs from the current line" filter is what keeps a
> repeated `M-P` from finding the entry it has just loaded, since after
> `hist_get` the line buffer *is* that entry; combined with the
> `c_setpat` skip it gives a working "search again" without any explicit
> state.

> [spec:libedit:def:common.ed-sequence-lead-in-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_sequence_lead_in(EditLine *el __attribute__((__unused__)), wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-sequence-lead-in-fn]
> Does nothing and returns `CC_NORM`. Neither parameter is read and no
> field of `EditLine` is touched.
>
> It is the placeholder bound to the first character of a multi-character
> key sequence — `^X` in the emacs map, and any prefix installed by
> `el_wset(EL_BIND, ...)` for a terminal function key. The real work is
> done before this ever runs: `read_getcmd` consults the key-macro trie
> (`keymacro_get`) and, when the bytes read so far are the prefix of a
> longer bound sequence, keeps reading instead of dispatching. This
> function is what the single-character map holds so that the prefix is
> not treated as unbound; reaching it means the sequence did not
> complete, and the correct response is to swallow the character
> silently rather than beep.
>
> Identical in behaviour to `ed_ignore`; the two exist as separate
> entries in the function table so that bindings and `bind`-command
> output can tell them apart.

> [spec:libedit:def:common.ed-start-over-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_start_over(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-start-over-fn]
> Throws the current line away and starts editing from scratch. Bound to
> `^G` in the vi maps (the emacs map leaves `^G` as `ed_unassigned`).
>
> 1. `ch_reset(el)`, which sets: `cursor = lastchar = buffer` (the
>    line becomes empty, though
>    the old characters are left physically in the buffer and no NUL is
>    written); `c_undo.len = -1` and `c_undo.cursor = 0`;
>    `c_vcmd.action = NOP` and `c_vcmd.pos = buffer`;
>    `c_kill.mark = buffer`; `el_map.current = el_map.key`;
>    `el_state.inputmode = MODE_INSERT`; `doingarg = 0`;
>    `metanext = 0`; `argument = 1`; `lastcmd = ED_UNASSIGNED`; and
>    `el_history.eventno = 0`.
> 2. Return `CC_REFRESH`.
>
> Three consequences worth stating. `el_map.current = el_map.key` drops
> vi command mode, so `^G` from vi command mode leaves you in insert
> mode. `el_history.eventno = 0` discards the history position, so a
> subsequent `^N`/`j` reloads `el->el_history.buf` — which is *not*
> cleared here and still holds whatever line was last stashed. And the
> **kill buffer contents are not cleared**: only `c_kill.mark` is reset,
> so `c_kill.buf`/`c_kill.last` survive and a yank after `^G` still
> pastes the pre-`^G` kill.
>
> `el->el_state.argument` is ignored (and reset by `ch_reset` anyway,
> then reset again by `el_wgets`). There is no error path.

> [spec:libedit:def:common.ed-transpose-chars-fn]
> libedit_private el_action_t ed_transpose_chars(EditLine *el, wint_t c)

> [spec:libedit:sem:common.ed-transpose-chars-fn]
> Exchanges the two characters immediately to the left of the cursor,
> after first stepping the cursor right when it is not already at the
> end of the line. Bound to `^T` in emacs and in the vi insert map. Mode
> independent.
>
> The `c` parameter's incoming value is **ignored**; `c` is reused
> purely as the scratch variable for the swap.
>
> 1. If `el->el_line.cursor < el->el_line.lastchar` (the cursor is
>    inside the text, not at the end):
>    - If `el->el_line.lastchar <= &el->el_line.buffer[1]` — fewer than
>      two characters in the line — return `CC_ERROR` with nothing
>      changed.
>    - Otherwise `el->el_line.cursor++`. This is what makes the command
>      swap the character *under* the cursor with the one before it and
>      leave the cursor after the pair, rather than acting on the two to
>      the left.
> 2. If `el->el_line.cursor > &el->el_line.buffer[1]` (the cursor is at
>    offset 2 or more, so two characters precede it): swap
>    `cursor[-2]` and `cursor[-1]` in place and return `CC_REFRESH`.
> 3. Otherwise return `CC_ERROR`.
>
> `el->el_state.argument` is ignored entirely — `^T` never repeats, and
> `ESC 3 ^T` transposes once. Nothing is written to the kill buffer, no
> vi undo snapshot is taken, and `lastchar` never moves.
>
> BUG: step 1's `cursor++` is not undone on the step-3 error path. With
> the cursor at `buffer` on a line of two or more characters, the guard
> in step 1 passes, the cursor advances to `buffer + 1`, the step-2 test
> `cursor > buffer + 1` then fails, and `CC_ERROR` is returned with the
> cursor **already moved one to the right**. `el_wgets` only beeps on
> `CC_ERROR` — it does not refresh — so the internal cursor and the
> displayed cursor disagree until something else forces a redraw. The
> port must reproduce the cursor move to stay behaviourally identical,
> or treat it as a fix; it cannot silently do neither.
>
> Worked cases. `"ab"` with the cursor at the end: step 1 is skipped,
> `cursor == buffer + 2 > buffer + 1`, so `a` and `b` are swapped to
> `"ba"` and the cursor stays at the end. `"abc"` with the cursor on
> `b` (offset 1): the cursor advances to offset 2, then `b` and `a`
> — positions 0 and 1 — are swapped, giving `"bac"` with the cursor on
> `c`. A one-character line returns `CC_ERROR` from step 1 (cursor at
> offset 0) or step 3 (cursor at offset 1), in both cases without
> moving anything. An empty line returns `CC_ERROR` from step 3.

> [spec:libedit:def:common.ed-unassigned-fn]
> libedit_private el_action_t /*ARGSUSED*/ ed_unassigned(EditLine *el __attribute__((__unused__)), wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:common.ed-unassigned-fn]
> Does nothing and returns `CC_ERROR`. Neither parameter is read and no
> field of `EditLine` is touched.
>
> This is the entry the keymaps hold for every key that has no command
> bound to it, and it is also the initial value of
> `el->el_state.lastcmd` and of `el->el_chared.c_redo.cmd` after
> `ch_init`/`ch_reset`. Returning `CC_ERROR` makes `el_wgets` call
> `terminal_beep` and `terminal__flush`, so pressing an unbound key
> beeps and changes nothing else. Contrast `ed_ignore`, which is
> identical apart from returning `CC_NORM` and therefore staying silent.
