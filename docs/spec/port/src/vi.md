# src/vi.c

> [spec:libedit:def:vi.cv-action-fn]
> static el_action_t cv_action(EditLine *el, wint_t c)

> [spec:libedit:sem:vi.cv-action-fn]
> Internal two-phase handler shared by the vi operator prefixes `d`, `c`
> and `y`. `c` is not a keystroke; it is the operator bitmask —
> `DELETE` (0x01) for `d`, `DELETE|INSERT` (0x03) for `c`, `YANK` (0x04)
> for `y`.
>
> Phase 1 — no operator pending (`el->el_chared.c_vcmd.action == NOP`,
> which is 0): store `c_vcmd.pos = el->el_line.cursor` (the anchor that
> the following motion will delete/yank back to) and
> `c_vcmd.action = c`. Change nothing else. Return `CC_ARGHACK`.
> `CC_ARGHACK` is load-bearing: the dispatcher performs no redraw for it
> and, uniquely, skips the per-command reset of `el_state.argument`,
> `el_state.doingarg` and `c_vcmd.action`, so both the pending operator
> and any numeric count entered before it survive into the next
> keystroke.
>
> Phase 2 — an operator is already pending; this is the doubled form
> `dd` / `cc` / `yy`. Compare `c` numerically with `c_vcmd.action`:
>   - Different (e.g. `dy`, `cd`): return `CC_ERROR` immediately with no
>     other effect at all — line, cursor, kill buffer, undo buffer and
>     `c_vcmd` are untouched. The dispatcher beeps and then clears the
>     pending operator.
>   - Same: act on the entire line and ignore `el_state.argument`
>     completely (there is no `3dd` — the count is silently discarded):
>       1. If `(c & YANK) == 0` (i.e. `dd` or `cc`), take an undo
>          snapshot with `cv_undo`.
>       2. Copy the whole line `[el_line.buffer, el_line.lastchar)` into
>          the kill buffer with `cv_yank`, replacing its previous
>          contents. A zero-length line yields an empty kill buffer.
>       3. Set `c_vcmd.action = NOP` and `c_vcmd.pos = NULL` (a null
>          pointer, not `buffer`; `cv_delfini` treats null as "nothing
>          pending" and returns early).
>       4. If `(c & YANK) == 0`, empty the line: `lastchar = buffer` and
>          `cursor = buffer`.
>       5. If `(c & INSERT)` (`cc` only), switch to the vi insert keymap
>          by setting `el_map.current = el_map.key`.
>       6. Return `CC_REFRESH`.
>     So `yy` copies the line and leaves line, cursor, undo buffer and
>     keymap entirely unchanged, still returning `CC_REFRESH`.

> [spec:libedit:def:vi.cv-paste-fn]
> static el_action_t cv_paste(EditLine *el, wint_t c)

> [spec:libedit:sem:vi.cv-paste-fn]
> Internal helper behind `p` and `P`. `c` is a boolean, not a character:
> `c == 0` means paste *after* the cursor, any non-zero `c` means paste
> *at* the cursor (before it).
>
> Steps:
>   1. Let `k = &el->el_chared.c_kill` and
>      `len = (size_t)(k->last - k->buf)`. If `k->buf` is null or
>      `len == 0`, return `CC_ERROR` with no side effects.
>   2. Take an undo snapshot with `cv_undo`.
>   3. If `c == 0` and `el_line.cursor < el_line.lastchar`, advance
>      `cursor` by one. (At end of line the cursor does not move, so `p`
>      on an empty or end-positioned line behaves like `P`.)
>   4. `c_insert(el, (int)len)` — open a `len`-wide gap at the cursor by
>      shifting `[cursor, lastchar]` right by `len` and advancing
>      `lastchar` by `len`, growing the buffers first if needed.
>   5. If `cursor + len > lastchar`, return `CC_ERROR`. This can only
>      happen when `c_insert` silently failed to grow the buffers; note
>      that by this point `cv_undo` has already run and the cursor may
>      already have been advanced in step 3, so the error path is not
>      side-effect free.
>   6. `memcpy` `len` wide characters from `k->buf` to `cursor`.
>   7. Return `CC_REFRESH`.
>
> The kill buffer is not consumed and is pasted exactly once:
> `el_state.argument` is never read, so `3p` pastes a single copy.
> The cursor is *not* moved past the pasted text — it is left on the
> first pasted character. Real vi leaves the cursor on the last pasted
> character; this is a deliberate-looking divergence that must be
> preserved.

> [spec:libedit:def:vi.vi-add-at-eol-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_add_at_eol(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-add-at-eol-fn]
> Implements vi `A`. `c` is ignored.
>
> In order: switch to the vi insert keymap (`el_map.current =
> el_map.key`); set `el_line.cursor = el_line.lastchar` (one past the
> last character, the append position); take an undo snapshot with
> `cv_undo` — after the cursor move, so the snapshot's saved cursor
> offset is the end-of-line position. Return `CC_CURSOR` (redraw the
> cursor only; the text did not change).
>
> `el_state.argument` is ignored (no `3A`). Any pending vi operator in
> `c_vcmd.action` is not honoured; the dispatcher clears it on return.

> [spec:libedit:def:vi.vi-add-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_add(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-add-fn]
> Implements vi `a`. `c` is ignored.
>
> Steps:
>   1. Switch to the vi insert keymap: `el_map.current = el_map.key`.
>   2. If `el_line.cursor < el_line.lastchar`: advance `cursor` by one
>      (a redundant clamp to `lastchar` follows in the C and can never
>      fire), and select `CC_CURSOR` as the return value. Otherwise
>      (cursor already at `lastchar`) leave the cursor alone and select
>      `CC_NORM`.
>   3. Take an undo snapshot with `cv_undo` — again after the cursor
>      move, so the snapshot records the post-move cursor offset.
>   4. Return the selected value: `CC_CURSOR` when the cursor moved,
>      `CC_NORM` when it did not.
>
> `el_state.argument` is ignored (no `3a`). A pending vi operator is not
> honoured.

> [spec:libedit:def:vi.vi-alias-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_alias(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-alias-fn]
> Implements vi `@` — expand a single-letter shell alias into the input
> stream. `c` is ignored.
>
> Steps:
>   1. If `el->el_chared.c_aliasfun` is null (no alias callback was
>      registered via `el_set(EL_ALIAS_TEXT)`), return `CC_ERROR`.
>   2. Build a 3-byte name buffer: `name[0] = '_'`, `name[2] = '\0'`.
>   3. Read one character with `el_getc(el, &name[1])`. `el_getc` reads a
>      wide character and narrows it with `wctob`; it returns 1 on
>      success, 0 or negative on EOF/read error, and -1 with `errno` set
>      to `ERANGE` when the wide character has no single-byte
>      representation in the current locale. If the result is not exactly
>      1, return `CC_ERROR`. (On any non-1 result `el_getc` has already
>      stored `'\0'` into `name[1]`.)
>   4. Call `alias_text = (*c_aliasfun)(c_aliasarg, name)` with the
>      two-character name, e.g. `"_x"`.
>   5. If `alias_text` is non-null, convert it from the locale multibyte
>      encoding to wide characters with `ct_decode_string(alias_text,
>      &el->el_scratch)` and push the result onto the macro/pushback
>      stack with `el_wpush`, so the expansion is re-read as input.
>      `ct_decode_string` yields null on an invalid multibyte sequence,
>      and `el_wpush` responds to a null string (or to macro-nesting
>      overflow, `EL_MAXMACRO` levels) by beeping and flushing.
>   6. Return `CC_NORM` unconditionally — including when the alias
>      lookup returned null (unknown alias is silently ignored, not an
>      error) and when the push failed.
>
> `el_state.argument` is ignored. The keymap is deliberately *not*
> switched to insert mode; POSIX implies it should be, and libedit's own
> comment records the choice to follow historical precedent instead.

> [spec:libedit:def:vi.vi-change-case-fn]
> libedit_private el_action_t vi_change_case(EditLine *el, wint_t c)

> [spec:libedit:sem:vi.vi-change-case-fn]
> Implements vi `~` — toggle the case of the character under the cursor
> and advance, `el_state.argument` times. The incoming `c` is ignored and
> the parameter is reused as a scratch variable.
>
> Steps:
>   1. If `el_line.cursor >= el_line.lastchar` (nothing under the
>      cursor, including the empty-line case), return `CC_ERROR` without
>      taking an undo snapshot.
>   2. Take one undo snapshot with `cv_undo`, covering the whole run.
>   3. Loop `i` from 0 while `i < el_state.argument`:
>        a. Read `ch = *cursor`. If `iswupper(ch)` store `towlower(ch)`;
>           else if `iswlower(ch)` store `towupper(ch)`; otherwise store
>           nothing (the character is left as-is, but the iteration is
>           still consumed and the cursor still advances).
>        b. Advance `cursor` by one. If the advanced cursor is now
>           `>= lastchar`, step `cursor` back by one (so it ends on the
>           final character, never past the end), call `re_fastaddc`, and
>           break out of the loop.
>        c. Otherwise call `re_fastaddc` and continue.
>   4. Return `CC_NORM`.
>
> Cursor landing: after `n~` the cursor sits `n` characters right of
> where it started, except that it is clamped to the last character of
> the line (`lastchar - 1`), never to `lastchar`. A count larger than the
> number of remaining characters simply stops at end of line without
> error; the return is still `CC_NORM`.
>
> Redraw: `re_fastaddc` only takes its incremental path when the cursor
> is exactly at `lastchar` and the preceding character is not a tab. In
> this function the cursor is never at `lastchar` at the point of call,
> so `re_fastaddc` always degrades to a full `re_refresh`. The net
> observable effect is a full line redraw per iteration, which is why the
> return is `CC_NORM` (no further redraw) rather than `CC_REFRESH`.
>
> Case mapping is locale-dependent (`iswupper`/`iswlower`/`towupper`/
> `towlower`), and characters that are neither upper nor lower case pass
> through unchanged. A pending vi operator is not honoured — `~` is not a
> motion.

> [spec:libedit:def:vi.vi-change-meta-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_change_meta(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-change-meta-fn]
> Implements the vi `c` operator prefix. `c` (the parameter) is ignored.
> Tail-calls the shared operator handler with the mask
> `DELETE | INSERT` (0x03) — change is modelled as "delete, then stay in
> insert mode".
>
> Consequently:
>   - With no operator pending: records `c_vcmd.pos = el_line.cursor` and
>     `c_vcmd.action = DELETE|INSERT`, and returns `CC_ARGHACK`, which
>     suppresses redraw and preserves both the pending operator and any
>     count already accumulated so the following motion can consume them.
>   - With `DELETE|INSERT` already pending (the `cc` form): snapshots
>     undo, copies the whole line into the kill buffer, clears
>     `c_vcmd.action`/`c_vcmd.pos`, empties the line
>     (`lastchar = cursor = buffer`), switches to the vi insert keymap,
>     and returns `CC_REFRESH`. The count is ignored.
>   - With a *different* operator pending (`dc`, `yc`): returns
>     `CC_ERROR` with no side effects.
>
> The `DELETE|INSERT` value is also read directly by `cv_next_word`,
> which suppresses the trailing-whitespace skip on its last iteration
> when exactly this mask is pending — that is what makes `cw` behave like
> `ce`.

> [spec:libedit:def:vi.vi-change-to-eol-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_change_to_eol(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-change-to-eol-fn]
> Implements vi `C` (equivalent to `c$`). `c` is ignored.
>
> In order:
>   1. `cv_undo(el)` — snapshot the whole line for undo.
>   2. `cv_yank(el, el_line.cursor, (int)(el_line.lastchar -
>      el_line.cursor))` — copy the tail `[cursor, lastchar)` into the
>      kill buffer.
>   3. `ed_kill_line(el, 0)` — which copies exactly the same span into
>      the kill buffer again (redundant but harmless, the second copy is
>      byte-identical), sets `c_kill.last` accordingly, and truncates the
>      line by setting `lastchar = cursor`. Its `CC_REFRESH` return is
>      discarded.
>   4. Switch to the vi insert keymap: `el_map.current = el_map.key`.
>   5. Return `CC_REFRESH`.
>
> The cursor does not move; it is left at the new end of line. There is
> no error path — on an empty line or with the cursor at `lastchar` this
> yanks zero characters, empties the kill buffer, deletes nothing and
> still returns `CC_REFRESH` in insert mode. `el_state.argument` is
> ignored.

> [spec:libedit:def:vi.vi-command-mode-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_command_mode(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-command-mode-fn]
> Implements `<ESC>` — leave insert/replace mode and enter vi command
> mode. `c` is ignored. Also called internally by `ed_insert` to end a
> one-shot `MODE_REPLACE_1` (`r`) replacement.
>
> Steps, all unconditional except the last:
>   1. Cancel any pending vi operator: `c_vcmd.action = NOP` and
>      `c_vcmd.pos = NULL`.
>   2. `el_state.doingarg = 0`. Note it does *not* reset
>      `el_state.argument`; the dispatcher does that on return.
>   3. `el_state.inputmode = MODE_INSERT` — i.e. leave `MODE_REPLACE` /
>      `MODE_REPLACE_1` so that subsequent `ed_insert` calls insert
>      rather than overwrite.
>   4. `el_map.current = el_map.alt` — in vi mode `alt` is the vi command
>      keymap and `key` is the vi insert keymap, so this is the mode
>      switch. (In emacs mode `alt` is all `ED_UNASSIGNED`, so binding
>      this function under emacs makes the keyboard inert.)
>   5. `VI_MOVE` is defined in `chared.h`, so: if
>      `el_line.cursor > el_line.buffer`, step the cursor back one.
>      This is real-vi behaviour — leaving insert mode moves the cursor
>      left onto the last character typed.
>   6. Return `CC_CURSOR`.
>
> There is no error path.

> [spec:libedit:def:vi.vi-comment-out-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_comment_out(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-comment-out-fn]
> Implements vi `#` — prefix the line with `#` and submit it, so the
> shell records it in history without executing it. `c` is ignored.
>
> Steps:
>   1. `el_line.cursor = el_line.buffer`.
>   2. `c_insert(el, 1)` — open one slot at the start of the line,
>      shifting `[buffer, lastchar]` right by one and advancing
>      `lastchar`. If the buffers cannot be grown, `c_insert` silently
>      does nothing, in which case the next step *overwrites* the first
>      character instead of inserting before it. There is no check.
>   3. Store `'#'` at the cursor (i.e. at `buffer[0]`).
>   4. `re_refresh(el)` — redraw the line explicitly so the `#` is
>      visible before the line scrolls away.
>   5. Tail-call `ed_newline(el, 0)`, which moves the display to the
>      bottom, appends `'\n'` at `lastchar` (advancing it),
>      NUL-terminates, and returns `CC_NEWLINE`.
>
> So the return is always `CC_NEWLINE` and the line is accepted. No undo
> snapshot is taken (`u` cannot recover from `#`), the cursor is left at
> `buffer` before `ed_newline` runs, and `el_state.argument` is ignored.

> [spec:libedit:def:vi.vi-delete-meta-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_delete_meta(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-delete-meta-fn]
> Implements the vi `d` operator prefix. `c` (the parameter) is ignored.
> Tail-calls the shared operator handler with the mask `DELETE` (0x01).
>
> Consequently:
>   - With no operator pending: records `c_vcmd.pos = el_line.cursor` and
>     `c_vcmd.action = DELETE`, returning `CC_ARGHACK` — no redraw, and
>     the pending operator plus any accumulated count survive into the
>     next keystroke, where the motion command will call `cv_delfini` to
>     perform the deletion.
>   - With `DELETE` already pending (the `dd` form): snapshots undo,
>     copies the whole line into the kill buffer, clears
>     `c_vcmd.action`/`c_vcmd.pos`, empties the line
>     (`lastchar = cursor = buffer`), leaves the keymap in command mode,
>     and returns `CC_REFRESH`. `el_state.argument` is ignored, so there
>     is no `3dd`.
>   - With a different operator pending (`cd`, `yd`): returns `CC_ERROR`
>     with no side effects.

> [spec:libedit:def:vi.vi-delete-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_delete_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-delete-prev-char-fn]
> Implements backspace (`^H` and `^?`) in the vi *insert* keymap. `c` is
> ignored.
>
> Steps:
>   1. If `el_line.cursor <= el_line.buffer`, return `CC_ERROR` (beep)
>      with no side effects.
>   2. `c_delbefore1(el)` — delete exactly one character before the
>      cursor by copying `cp[1]` over `cp` for `cp` running from
>      `cursor - 1` up to and including `lastchar`, then decrementing
>      `lastchar`. Note this is the *no-yank* variant: it does not touch
>      the kill buffer and does not take an undo snapshot, unlike
>      `c_delbefore`. (It reads `lastchar[1]`, one slot past the text;
>      that slot is always inside the allocation because `el_line.limit`
>      leaves two spare elements.)
>   3. Step the cursor back one.
>   4. Return `CC_REFRESH`.
>
> Exactly one character is deleted per invocation; `el_state.argument` is
> ignored. This is also the command `el_wgets` special-cases when
> recording keystrokes for `.` (vi redo): while in the insert keymap, a
> `VI_DELETE_PREV_CHAR` rewinds the redo recording pointer over the
> previously recorded printable character instead of appending.

> [spec:libedit:def:vi.vi-end-big-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_end_big_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-end-big-word-fn]
> Implements vi `E` — move to the end of the current/next whitespace-
> delimited "big word". `c` is ignored.
>
>   1. If `el_line.cursor == el_line.lastchar` (note `==`, not `>=`;
>      this covers the empty line) return `CC_ERROR`.
>   2. Compute the destination with `cv__endword(el, cursor, lastchar,
>      el_state.argument, cv__isWord)` and store it in `el_line.cursor`.
>      `cv__isWord` classifies a character as 1 if it is not
>      `iswspace`, else 0 — so a "big word" is any maximal run of
>      non-whitespace, and punctuation does not split words.
>      `cv__endword` works as follows, with `p` starting at `cursor`:
>        - advance `p` by one, unconditionally;
>        - repeat `argument` times: skip forward while `p < lastchar` and
>          `iswspace(*p)`; then sample `test = cv__isWord(*p)` at the
>          current `p` and advance while `p < lastchar` and the class of
>          `*p` equals `test`;
>        - step `p` back by one and return it.
>      The class sample can read `*lastchar`, one past the text, when the
>      scan has already reached the end; that slot is inside the
>      allocation but holds stale data, and the result is still clamped
>      because the inner loops guard on `p < lastchar`. The returned
>      position is therefore always in `[buffer, lastchar - 1]`.
>      With the cursor already on the last character of the line, the
>      result is the same position (no movement, no error).
>   3. If a vi operator is pending (`c_vcmd.action != NOP`): advance
>      `el_line.cursor` by one *before* calling `cv_delfini(el)`, making
>      the motion **inclusive** of the character it landed on, then
>      return `CC_REFRESH`. This is the correct vi semantics for `dE` /
>      `cE` / `yE`. Note there is no `el_map.type == MAP_VI` guard here,
>      unlike `vi_next_word`/`vi_next_big_word`.
>   4. Otherwise return `CC_CURSOR`.
>
> A count larger than the number of remaining words lands on the last
> character of the line rather than erroring.

> [spec:libedit:def:vi.vi-end-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_end_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-end-word-fn]
> Implements vi `e` — move to the end of the current/next word. `c` is
> ignored. Identical to `E` (`vi_end_big_word`) except for the character
> classifier.
>
>   1. If `el_line.cursor == el_line.lastchar` (covers the empty line),
>      return `CC_ERROR`.
>   2. `el_line.cursor = cv__endword(el, cursor, lastchar,
>      el_state.argument, cv__isword)`.
>      `cv__isword` returns **three** classes, and word runs are runs of
>      *equal class*:
>        - 1 if `iswalnum(ch)` or `ch` occurs in `el_map.wordchars`
>          (which `map_init_vi` sets to `L"_"`, so `_` counts as a word
>          character);
>        - 2 otherwise if `iswgraph(ch)` — i.e. printable punctuation;
>        - 0 otherwise — whitespace and non-graphic characters.
>      So `foo.bar` is three words (`foo`, `.`, `bar`) for `e`, whereas
>      it is one big word for `E`.
>      `cv__endword` itself: start `p = cursor`, advance `p` by one;
>      repeat `argument` times { skip while `p < lastchar &&
>      iswspace(*p)`; sample `test = cv__isword(*p)`; advance while
>      `p < lastchar` and `cv__isword(*p) == test` }; then step `p` back
>      one and return it. The class sample may read `*lastchar` once the
>      scan is exhausted (stale but in-allocation data); the result is
>      always within `[buffer, lastchar - 1]`. With the cursor on the
>      last character of the line the result is that same position.
>   3. If `c_vcmd.action != NOP`: advance `el_line.cursor` by one, then
>      `cv_delfini(el)`, then return `CC_REFRESH`. The `+1` makes `de` /
>      `ce` / `ye` **inclusive** of the last character of the word, which
>      is the whole reason this differs from `w`. No `MAP_VI` guard.
>   4. Otherwise return `CC_CURSOR`.

> [spec:libedit:def:vi.vi-histedit-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_histedit(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-histedit-fn]
> Implements vi `v` — dump the current line to a temporary file, run an
> external editor on it synchronously, read the result back, and submit
> it. `c` is ignored.
>
> Steps:
>   1. If `el_state.doingarg` is set, first call `vi_to_history_line(el,
>      0)` so the count selects a history entry to edit; if that returns
>      `CC_ERROR`, return `CC_ERROR`.
>   2. `editor = (el->el_getenv)("EDITOR")`; if null, use `"vi"`.
>      `el_getenv` defaults to `secure_getenv` and is overridable via
>      `EL_GETENV`.
>   3. `fd = mkstemp(tempfile)` on the *hardcoded* template
>      `"/tmp/histedit.XXXXXXXXXX"` — `TMPDIR` is not consulted. If
>      `fd < 0`, return `CC_ERROR` (nothing to clean up).
>   4. Let `len = lastchar - buffer`. Allocate a byte buffer `cp` of
>      `TMP_BUFSIZ = EL_BUFSIZ * MB_LEN_MAX` bytes with `el_calloc`, and
>      a wide buffer `line` of `len + 1` elements. Either allocation
>      failing jumps to the error exit below.
>   5. `wcsncpy(line, buffer, len)`, force `line[len] = L'\0'`,
>      `wcstombs(cp, line, TMP_BUFSIZ - 1)`, force
>      `cp[TMP_BUFSIZ - 1] = '\0'`, then `len = strlen(cp)`. The
>      `wcstombs` return value is unchecked; on an unconvertible wide
>      character it returns `(size_t)-1` leaving `cp` partially written,
>      but because `cp` was zero-filled the following `strlen` stays in
>      bounds.
>   6. `write(fd, cp, len)` then `write(fd, "\n", 1)`. Both return values
>      are unchecked (short writes and `ENOSPC` are silently ignored).
>   7. `fork()`:
>        - `-1`: jump to the error exit.
>        - `0` (child): `close(fd)`, then `execlp(editor, editor,
>          tempfile, NULL)`, then `exit(0)`. Note `exit`, not `_exit`, so
>          an exec failure flushes the inherited stdio buffers a second
>          time, and the parent cannot distinguish exec failure from a
>          successful edit (it simply re-reads the unmodified file).
>          The child inherits the raw-mode terminal.
>        - default (parent): spin on `while (waitpid(pid, &status, 0) !=
>          pid) continue;` — `status` is never examined, and if `waitpid`
>          fails persistently (e.g. `ECHILD` because `SIGCHLD` is set to
>          `SIG_IGN`) this loops forever. Then `lseek(fd, 0, SEEK_SET)`
>          and `st = read(fd, cp, TMP_BUFSIZ - 1)` **through the original
>          descriptor**, so an editor that saves by writing a new file
>          and renaming leaves this reading the stale original contents.
>            - If `st > 0`: `cp[st] = '\0'`; set `len = limit - buffer`
>              and then `len = mbstowcs(el_line.buffer, cp, len)`,
>              decoding straight over the line buffer; if `len > 0` and
>              `buffer[len - 1] == L'\n'`, decrement `len` to strip the
>              trailing newline.
>              **UB:** `mbstowcs` returns `(size_t)-1` on an invalid
>              multibyte sequence and the result is not checked, so
>              `len` becomes `SIZE_MAX`, `buffer[len - 1]` is a wild
>              read, and `lastchar` is then set to `buffer + SIZE_MAX`.
>              The Rust port must define this case explicitly (treat a
>              decode failure as an empty result) rather than reproduce
>              it.
>            - Else `len = 0`, which is also what a read error (`st < 0`)
>              produces — the line becomes empty.
>          Then `cursor = buffer`, `lastchar = buffer + len`, and free
>          both `cp` and `line`.
>   8. `close(fd)`, `unlink(tempfile)`, and tail-call `ed_newline(el, 0)`
>      — which moves to the bottom of the display, appends `'\n'` at
>      `lastchar` (advancing it), NUL-terminates and returns
>      `CC_NEWLINE`. So the edited text is **submitted immediately**, not
>      returned to the editor. (The C carries a commented-out
>      `return CC_REFRESH;` showing the alternative.)
>   9. Error exit: free `line` and `cp` (both null-safe), `close(fd)`,
>      `unlink(tempfile)`, return `CC_ERROR`.
>
> No undo snapshot is taken, so `u` cannot recover the pre-edit line, and
> the keymap is not changed.

> [spec:libedit:def:vi.vi-history-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_history_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-history-word-fn]
> Implements vi `_` — append a word taken from the most recent history
> entry, then enter insert mode. `c` is ignored. (libedit's own comment
> notes that `_` means "the whole current line" in real vi, so this is a
> libedit invention; it also explains why `cc` is documented as a synonym
> for `c_`.)
>
> Steps:
>   1. `wp = HIST_FIRST(el)` — the most recent history entry as a
>      wide string, via the registered history function (`H_FIRST`). If
>      it is null, return `CC_ERROR`.
>   2. Word scan over `wp`, splitting on `iswspace` only (no
>      `cv__isword`/`cv__isWord` classes here; punctuation is part of the
>      word). Initialise `wsp = wep = NULL` and run a do/while:
>        - skip `iswspace` characters;
>        - if the character is `L'\0'`, break out;
>        - set `wsp` to this position, advance while non-NUL and not
>          `iswspace`, set `wep` to the stopping position;
>        - continue while `(!el_state.doingarg || --el_state.argument >
>          0) && *wp != L'\0'`.
>      With **no** count, the guard's left half is always true so the
>      loop runs to the end of the string and `[wsp, wep)` is the
>      **last** word of the history line. With a count `n`, the loop body
>      runs at most `n` times and `[wsp, wep)` is the **n-th** word
>      counting from the left, 1-based. The count is consumed
>      destructively by `--el_state.argument`, which the dispatcher
>      resets afterwards.
>   3. If `wsp` is still null (the history line was empty or all
>      whitespace), **or** a count was given and `el_state.argument != 0`
>      (the line had fewer than `n` words), return `CC_ERROR`. Nothing
>      has been modified at this point.
>   4. `cv_undo(el)` — snapshot for undo.
>   5. `len = wep - wsp`.
>   6. If `cursor < lastchar`, advance the cursor by one, so the text is
>      appended *after* the character under the cursor (vi `a`
>      positioning); at end of line the cursor stays put.
>   7. `c_insert(el, len + 1)` — open `len + 1` slots (one for the
>      separating space), growing the buffers if needed.
>   8. Write, bounded by `el_line.limit`: with `cp = cursor` and
>      `lim = limit`, if `cp < lim` store `L' '` and advance; then while
>      `wsp < wep && cp < lim` copy one character and advance both.
>      Finally set `el_line.cursor = cp`, i.e. just past the last
>      character written.
>      Note the mismatch: `c_insert` unconditionally advanced `lastchar`
>      by `len + 1`, but the copy loop stops at `limit`. If `c_insert`
>      could not grow the buffers it returns without moving `lastchar`
>      at all, and the writes then overwrite existing text instead of
>      inserted space. Neither case is detected.
>   9. Switch to the vi insert keymap (`el_map.current = el_map.key`).
>  10. Return `CC_REFRESH`.

> [spec:libedit:def:vi.vi-insert-at-bol-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_insert_at_bol(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-insert-at-bol-fn]
> Implements vi `I`. `c` is ignored.
>
> In order: `el_line.cursor = el_line.buffer`; `cv_undo(el)` (snapshot
> taken *after* the cursor move, so the recorded cursor offset is 0);
> `el_map.current = el_map.key` (switch to the vi insert keymap). Return
> `CC_CURSOR`.
>
> There is no error path and no dependence on `el_state.argument`. Unlike
> real vi's `I`, the cursor goes to column 0 rather than to the first
> non-blank character. A pending vi operator is not honoured.

> [spec:libedit:def:vi.vi-insert-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_insert(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-insert-fn]
> Implements vi `i`. `c` is ignored.
>
> Switch to the vi insert keymap (`el_map.current = el_map.key`), then
> `cv_undo(el)` to snapshot the line (and record `el_state.thiscmd` /
> `thisch` / the current count into the redo record). The cursor does not
> move and the text is unchanged. Return `CC_NORM` — no redraw at all,
> since nothing visible changed.
>
> There is no error path; `el_state.argument` is ignored (no `3i`
> repetition); a pending vi operator is not honoured.

> [spec:libedit:def:vi.vi-kill-line-prev-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_kill_line_prev(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-kill-line-prev-fn]
> Implements `^U` — cut from the beginning of the line to the cursor. `c`
> is ignored. Bound in all three vi keymaps (insert, command, and the
> `tty_bind_char` slot).
>
> Steps:
>   1. Copy `[buffer, cursor)` into the kill buffer directly (a manual
>      element-by-element copy from `el_line.buffer` into
>      `el_chared.c_kill.buf`, not via `cv_yank`), and set
>      `c_kill.last` to one past the last element copied. The kill and
>      line buffers are kept the same size by `ch_enlargebufs`, so this
>      cannot overrun, but there is no explicit bound check.
>   2. `c_delbefore(el, (int)(cursor - buffer))` — delete that many
>      characters before the cursor. `c_delbefore` clamps the count to
>      the available text, and because `el_map.current` is never equal to
>      `el_map.emacs` (the latter is the static default table, the former
>      is always the heap `key`/`alt` copy) it *always* also performs
>      `cv_undo` and re-yanks the same span, redundantly redoing step 1.
>      This makes `^U` undoable with `u`.
>   3. `el_line.cursor = el_line.buffer`.
>   4. Return `CC_REFRESH`.
>
> There is no error path: with the cursor already at `buffer` this
> deletes nothing, empties the kill buffer, still takes an undo snapshot
> and still returns `CC_REFRESH`. `el_state.argument` is ignored.

> [spec:libedit:def:vi.vi-list-or-eof-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_list_or_eof(EditLine *el, wint_t c)

> [spec:libedit:sem:vi.vi-list-or-eof-fn]
> Implements `^D`. Despite the `/*ARGSUSED*/` marker, `c` **is** used —
> it is echoed on the EOF path. Bound in both the vi insert and vi
> command keymaps.
>
> Three-way branch:
>   - `cursor == lastchar` **and** `cursor == buffer` (the line is empty
>     and the cursor is at its start): call `terminal_writec(el, c)` to
>     echo the character in its visual form (`^D`) and flush, then return
>     `CC_EOF`. The dispatcher turns this into end-of-input for
>     `el_wgets` (or, in `UNBUFFERED` mode with nothing read yet, into a
>     literal `^D` character in the buffer).
>   - `cursor == lastchar` but the line is non-empty: `terminal_beep(el)`
>     and return `CC_ERROR`.
>   - `cursor < lastchar`: `terminal_beep(el)` and return `CC_ERROR`.
>
> The name promises completion listing, but the listing branch is behind
> `#ifdef notyet` and is not compiled: mid-line `^D` is simply an error.
> The port should reproduce the error, not implement listing. Note the
> last two branches are behaviourally identical (beep, `CC_ERROR`) and
> the dispatcher beeps a second time for `CC_ERROR`.
>
> `el_state.argument` is ignored.

> [spec:libedit:def:vi.vi-match-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_match(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-match-fn]
> Implements vi `%` — jump to the bracket matching the first bracket at
> or after the cursor. `c` is ignored. The bracket alphabet is exactly
> `L"()[]{}"`, in that order, and nothing else (no quote matching, no
> comment awareness).
>
> Steps:
>   1. Write `L'\0'` at `*lastchar` so the buffer is NUL-terminated for
>      the `wcs*` calls that follow.
>   2. `i = wcscspn(cursor, L"()[]{}")` — the offset from the cursor to
>      the first bracket. Let `o_ch = cursor[i]`. If `o_ch == 0` (no
>      bracket between the cursor and end of line) return `CC_ERROR`.
>      Note the search only ever looks *forward* from the cursor, even
>      when the found bracket is a closing one.
>   3. Let `delta = index of o_ch in "()[]{}"`. The matching character is
>      `c_ch = match_chars[delta ^ 1]` (pairing 0↔1, 2↔3, 4↔5). Set
>      `count = 1` and then re-purpose `delta` as the scan direction:
>      `delta = 1 - (delta & 1) * 2`, i.e. `+1` when `o_ch` is an opening
>      bracket (even index) and, for a closing bracket, the `size_t`
>      wraparound of `-1` — `delta` is declared `size_t`, so the odd case
>      yields `SIZE_MAX`, not a negative number. Pointer arithmetic still
>      steps backward by one element, but the value itself never compares
>      as negative; step 6 depends on this.
>   4. Scan with proper nesting: starting from `cp = &cursor[i]`, while
>      `count != 0` advance `cp += delta`; if `cp < buffer` or
>      `cp >= lastchar`, return `CC_ERROR` (unbalanced — the cursor and
>      the line are left untouched); if `*cp == o_ch` increment `count`;
>      else if `*cp == c_ch` decrement `count`.
>   5. `el_line.cursor = cp` — the matching bracket.
>   6. If a vi operator is pending (`c_vcmd.action != NOP`): advance the
>      cursor by one **unconditionally**, then `cv_delfini(el)` and return
>      `CC_REFRESH`. The C writes that advance under `if (delta > 0)`, but
>      the `size_t` `delta` of step 3 is `SIZE_MAX` in the backward case,
>      so the guard is true in both directions and never selects a second
>      path. For a forward match the advance includes the matched closing
>      bracket in the operated range. For a backward match it leaves the
>      cursor one *past* the matched opening bracket, so the range is
>      `[matched_open + 1, c_vcmd.pos)`: the matched opening bracket is
>      **not** deleted, and neither is the character at the anchor. The C
>      carries an explicit comment saying the backward case must not
>      delete the character under the cursor, following POSIX and
>      diverging from NetBSD vi — but the guard that comment annotates is
>      dead, so a port must reproduce the unconditional advance, not the
>      conditional the comment implies.
>   7. Otherwise return `CC_CURSOR`.
>
> `el_state.argument` is ignored entirely — there is no `3%`. Note also
> that in the operator case the range is anchored at `c_vcmd.pos` (the
> cursor when `d`/`c`/`y` was pressed), not at the bracket found by
> `wcscspn`, so any characters between the anchor and the bracket are
> included.

> [spec:libedit:def:vi.vi-next-big-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_next_big_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-next-big-word-fn]
> Implements vi `W` — move forward to the start of the next
> whitespace-delimited big word. `c` is ignored.
>
>   1. If `el_line.cursor >= el_line.lastchar - 1` return `CC_ERROR`.
>      This means `W` fails when the cursor is on the *last* character of
>      the line, not merely past it. On an empty line (`lastchar ==
>      buffer`) the C forms the pointer `buffer - 1`, which is undefined
>      behaviour in strict C though it reliably yields `CC_ERROR` in
>      practice; the port should express this as "error if the line has
>      fewer than 2 characters remaining at or after the cursor".
>   2. `el_line.cursor = cv_next_word(el, cursor, lastchar,
>      el_state.argument, cv__isWord)`.
>      `cv__isWord` gives two classes: 1 for `!iswspace`, 0 for
>      whitespace — punctuation never splits a big word.
>      `cv_next_word`, with `p` starting at the cursor, repeats
>      `argument` times:
>        - sample `test = cv__isWord(*p)` at the current position and
>          advance while `p < lastchar` and the class of `*p` equals
>          `test` (so from inside a word this skips to the end of the
>          word; from inside whitespace it skips to the next non-space);
>        - then, **unless** this is the final iteration and
>          `c_vcmd.action` is exactly `DELETE|INSERT`, additionally skip
>          forward while `p < lastchar` and `iswspace(*p)`.
>      Finally the result is clamped to `lastchar`. The suppressed
>      whitespace skip is what makes `cW` change only the word and leave
>      the trailing whitespace, matching historical vi — it applies to
>      `c` only, not to `d` or `y`. As with `cv__endword`, the class
>      sample can read `*lastchar` on a later iteration once the scan is
>      exhausted; stale but in-allocation data, and the loop guards keep
>      the result clamped.
>   3. If `el_map.type == MAP_VI` **and** `c_vcmd.action != NOP`:
>      `cv_delfini(el)` and return `CC_REFRESH`. Note there is **no**
>      `cursor++` here, so `dW`/`cW`/`yW` is **exclusive** of the
>      character at the landing position — correct vi behaviour for a `W`
>      motion, and the key difference from `E`.
>   4. Otherwise return `CC_CURSOR`. (Because of the `MAP_VI` guard, a
>      pending operator is ignored when this function is invoked from an
>      emacs-type map, unlike `vi_prev_big_word` and `vi_end_big_word`,
>      which have no such guard.)
>
> A count that runs past the end of the line lands the cursor on
> `lastchar` and returns normally; it is not an error.

> [spec:libedit:def:vi.vi-next-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_next_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-next-char-fn]
> Implements vi `f` — move forward to the `argument`-th occurrence of a
> character read from the terminal. `c` is ignored.
>
> Tail-calls `cv_csearch(el, CHAR_FWD /* +1 */, (wint_t)-1,
> el_state.argument, /*tflag=*/0)`. The `-1` target means "prompt for the
> character now"; `tflag = 0` means land **on** the match rather than
> just before it. That helper:
>   - reads one wide character with `el_wgetc`; if the read does not
>     return exactly 1 it returns `ed_end_of_file(el, 0)`, i.e. `CC_EOF`;
>   - records the search state *before* searching — `el_search.chacha =
>     the target`, `el_search.chadir = +1`, `el_search.chatflg = 0` — so
>     `;` and `,` are updated even when the search subsequently fails;
>   - for each of `argument` repetitions: if the character under `cp`
>     already equals the target, step `cp` by `+1` first; then advance
>     `cp` by `+1` until `*cp` equals the target, returning `CC_ERROR` as
>     soon as `cp >= lastchar` (or `cp < buffer`). A failed search leaves
>     the cursor unmoved, but the `;`/`,` state has already been
>     overwritten;
>   - sets `el_line.cursor = cp`;
>   - if a vi operator is pending, advances the cursor by one (because
>     the direction is positive), calls `cv_delfini(el)` and returns
>     `CC_REFRESH` — so `df<ch>` is **inclusive** of the target
>     character;
>   - otherwise returns `CC_CURSOR`.

> [spec:libedit:def:vi.vi-next-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_next_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-next-word-fn]
> Implements vi `w` — move forward to the start of the next word. `c` is
> ignored. Identical to `W` (`vi_next_big_word`) except for the
> classifier.
>
>   1. If `el_line.cursor >= el_line.lastchar - 1` return `CC_ERROR`
>      (fails on the last character of the line, and on the empty line
>      where the C forms `buffer - 1`, strictly UB but reliably an error
>      in practice).
>   2. `el_line.cursor = cv_next_word(el, cursor, lastchar,
>      el_state.argument, cv__isword)`.
>      `cv__isword` gives **three** classes and a word is a maximal run
>      of one class:
>        - 1 if `iswalnum(ch)` or `ch` occurs in `el_map.wordchars`
>          (`map_init_vi` sets that to `L"_"`);
>        - 2 otherwise if `iswgraph(ch)` (punctuation and symbols);
>        - 0 otherwise (whitespace, non-graphic).
>      So in `foo.bar`, `w` stops at `.` and then at `b`, whereas `W`
>      would skip the whole token.
>      `cv_next_word` repeats `argument` times, with `p` starting at the
>      cursor: sample `test = cv__isword(*p)` and advance while
>      `p < lastchar` and the class of `*p` equals `test`; then, unless
>      this is the last iteration and `c_vcmd.action` is exactly
>      `DELETE|INSERT`, additionally skip while `p < lastchar` and
>      `iswspace(*p)`. The result is clamped to `lastchar`. Suppressing
>      the final whitespace skip for `c` is what makes `cw` behave like
>      `ce` — change the word, preserve the trailing blanks — exactly as
>      historical vi does, and it applies to `c` only, not `d` or `y`.
>      Starting on whitespace, the first inner loop consumes the
>      whitespace run (class 0) and the second is then a no-op, so the
>      cursor lands on the first character of the following word.
>   3. If `el_map.type == MAP_VI` **and** `c_vcmd.action != NOP`:
>      `cv_delfini(el)` and return `CC_REFRESH`. There is deliberately no
>      `cursor++`, so `dw`/`cw`/`yw` is **exclusive** of the character at
>      the landing position.
>   4. Otherwise return `CC_CURSOR`.

> [spec:libedit:def:vi.vi-paste-next-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_paste_next(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-paste-next-fn]
> Implements vi `p` — paste the kill buffer to the right of the cursor.
> `c` is ignored; it delegates to the shared paste helper with the
> "after" flag (0).
>
> Concretely:
>   - Let `len = c_kill.last - c_kill.buf`. If `c_kill.buf` is null or
>     `len == 0`, return `CC_ERROR` with no side effects.
>   - `cv_undo(el)` to snapshot the line.
>   - If `cursor < lastchar`, advance the cursor by one, so the text
>     lands after the character under the cursor. At end of line the
>     cursor does not move, so `p` there behaves like `P`.
>   - `c_insert(el, len)` to open the gap, then copy all `len` wide
>     characters from the kill buffer to the cursor.
>   - If `c_insert` failed to make room (`cursor + len > lastchar`),
>     return `CC_ERROR` — after `cv_undo` has run and after the cursor
>     has possibly moved.
>   - Otherwise return `CC_REFRESH`.
>
> `el_state.argument` is ignored: `3p` pastes one copy. The cursor is
> left on the **first** pasted character, not the last as in real vi.

> [spec:libedit:def:vi.vi-paste-prev-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_paste_prev(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-paste-prev-fn]
> Implements vi `P` — paste the kill buffer to the left of the cursor.
> `c` is ignored; it delegates to the shared paste helper with the
> "before" flag (1).
>
> Identical to `p` except that the cursor is **not** advanced first, so
> the text is inserted at the current cursor position:
>   - Let `len = c_kill.last - c_kill.buf`. If `c_kill.buf` is null or
>     `len == 0`, return `CC_ERROR` with no side effects.
>   - `cv_undo(el)` to snapshot the line.
>   - `c_insert(el, len)` to open a `len`-wide gap at the cursor, then
>     copy all `len` wide characters from the kill buffer there.
>   - If `c_insert` could not make room (`cursor + len > lastchar`),
>     return `CC_ERROR` (after `cv_undo` has already run).
>   - Otherwise return `CC_REFRESH`.
>
> `el_state.argument` is ignored: `3P` pastes one copy. The cursor is
> left on the first pasted character.

> [spec:libedit:def:vi.vi-prev-big-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_prev_big_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-prev-big-word-fn]
> Implements vi `B` — move back to the start of the previous
> whitespace-delimited big word. `c` is ignored.
>
>   1. If `el_line.cursor == el_line.buffer` return `CC_ERROR`.
>   2. `el_line.cursor = cv_prev_word(el, cursor, buffer,
>      el_state.argument, cv__isWord)`.
>      `cv__isWord` gives two classes: 1 for `!iswspace`, 0 for
>      whitespace. `cv_prev_word` works backwards, starting with
>      `p = cursor - 1`, and repeats `argument` times:
>        - step back while `p > buffer` and `iswspace(*p)` (note the
>          strict `>`: it will not skip past `buffer` itself);
>        - sample `test = cv__isWord(*p)` and step back while
>          `p >= buffer` and the class of `*p` equals `test`;
>        - if `p` has fallen below `buffer`, return `buffer` immediately.
>      After the loop, `p` is advanced by one (it sits one before the
>      word start) and clamped up to `buffer`. The landing position is
>      therefore the first character of the target big word.
>   3. If `c_vcmd.action != NOP`: `cv_delfini(el)` and return
>      `CC_REFRESH`. Because the landing position is to the *left* of
>      `c_vcmd.pos`, `cv_delfini` computes a negative size and uses
>      `c_delbefore`, deleting `[new_cursor, c_vcmd.pos)` — **exclusive**
>      of the character that was under the cursor when the operator was
>      pressed — and leaves the cursor at `new_cursor`.
>      Unlike `vi_next_big_word` there is no `el_map.type == MAP_VI`
>      guard, so this path is taken even from an emacs-type map.
>   4. Otherwise return `CC_CURSOR`.
>
> A count larger than the number of preceding words stops at `buffer`
> and returns normally.

> [spec:libedit:def:vi.vi-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-prev-char-fn]
> Implements vi `F` — move backward to the `argument`-th occurrence of a
> character read from the terminal. `c` is ignored.
>
> Tail-calls `cv_csearch(el, CHAR_BACK /* -1 */, (wint_t)-1,
> el_state.argument, /*tflag=*/0)`. The `-1` target means "prompt for the
> character now"; `tflag = 0` means land **on** the match. That helper:
>   - reads one wide character with `el_wgetc`; a read that does not
>     return exactly 1 yields `ed_end_of_file(el, 0)`, i.e. `CC_EOF`;
>   - records `el_search.chacha = target`, `el_search.chadir = -1`,
>     `el_search.chatflg = 0` **before** searching, so `;`/`,` state is
>     updated even on failure;
>   - for each of `argument` repetitions: if `*cp` already equals the
>     target step `cp` by `-1` first, then step by `-1` until `*cp`
>     equals the target, returning `CC_ERROR` as soon as `cp < buffer`
>     (or `cp >= lastchar`). On failure the cursor is left unmoved;
>   - sets `el_line.cursor = cp`;
>   - if a vi operator is pending: the direction is negative so the
>     cursor is **not** advanced, then `cv_delfini(el)` and return
>     `CC_REFRESH`. `cv_delfini` sees a negative size and uses
>     `c_delbefore`, so `dF<ch>` deletes `[match, original_cursor)` —
>     inclusive of the target character, exclusive of the character the
>     cursor started on;
>   - otherwise returns `CC_CURSOR`.

> [spec:libedit:def:vi.vi-prev-word-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_prev_word(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-prev-word-fn]
> Implements vi `b` — move back to the start of the previous word. `c` is
> ignored. Identical to `B` (`vi_prev_big_word`) except for the
> classifier.
>
>   1. If `el_line.cursor == el_line.buffer` return `CC_ERROR`.
>   2. `el_line.cursor = cv_prev_word(el, cursor, buffer,
>      el_state.argument, cv__isword)`.
>      `cv__isword` gives three classes and a word is a maximal run of
>      one class: 1 if `iswalnum(ch)` or `ch` occurs in
>      `el_map.wordchars` (set to `L"_"` by `map_init_vi`); else 2 if
>      `iswgraph(ch)`; else 0. So `b` treats a punctuation run as its own
>      word, while `B` does not.
>      `cv_prev_word` starts at `p = cursor - 1` and repeats `argument`
>      times: step back while `p > buffer` and `iswspace(*p)`; sample
>      `test = cv__isword(*p)`; step back while `p >= buffer` and the
>      class of `*p` equals `test`; if `p` fell below `buffer` return
>      `buffer` at once. After the loop `p` is advanced by one and
>      clamped up to `buffer`, landing on the first character of the
>      target word.
>   3. If `c_vcmd.action != NOP`: `cv_delfini(el)` and return
>      `CC_REFRESH`. The landing position is left of `c_vcmd.pos`, so
>      `cv_delfini` takes the negative-size branch and calls
>      `c_delbefore`, deleting `[new_cursor, c_vcmd.pos)` — **exclusive**
>      of the character under the original cursor — and leaving the
>      cursor at `new_cursor`. No `el_map.type == MAP_VI` guard, unlike
>      `vi_next_word`.
>   4. Otherwise return `CC_CURSOR`.

> [spec:libedit:def:vi.vi-redo-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_redo(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-redo-fn]
> Implements vi `.` — repeat the last non-motion command. `c` is ignored.
> It replays the `c_redo_t` record that `cv_undo` writes on every
> undoable command (`cv_undo` stores `count = doingarg ? argument : 0`,
> `action = c_vcmd.action`, `cmd = el_state.thiscmd`,
> `ch = el_state.thisch`, and resets `pos = buf`), together with the
> insert-mode keystrokes that `el_wgets` appends to `c_redo.buf` while
> the vi *insert* keymap is current.
>
> Steps, with `r = &el->el_chared.c_redo`:
>   1. If `el_state.doingarg` is 0 and `r->count` is non-zero, restore
>      the recorded count: `el_state.doingarg = 1` and
>      `el_state.argument = r->count`. An explicitly typed count on the
>      `.` itself therefore overrides the recorded one.
>   2. `c_vcmd.pos = el_line.cursor` — re-anchor the operator at the
>      current cursor — and `c_vcmd.action = r->action`, restoring the
>      pending `DELETE` / `DELETE|INSERT` / `YANK` mask (or `NOP`) that
>      was in force when the command ran.
>   3. If `r->pos != r->buf` (insert-mode text was recorded): clamp with
>      `if (r->pos + 1 > r->lim) r->pos = r->lim - 1;`, then write
>      `r->pos[0] = 0` to NUL-terminate the recording in place, and
>      `el_wpush(el, r->buf)` to push it onto the macro stack so it is
>      re-read as input *after* this command returns. Note the push
>      happens **before** the command is invoked, so a redone command
>      that reads a character itself (e.g. `r`) consumes it from the
>      pushback.
>   4. `el_state.thiscmd = r->cmd` and `el_state.thisch = r->ch`, so that
>      any nested `cv_undo` re-records the same command rather than
>      `VI_REDO`. This is also why `r->cmd` can never become `VI_REDO`
>      and the replay cannot recurse.
>   5. Tail-call `(*el->el_map.func[r->cmd])(el, r->ch)` and return
>      whatever that returns — every action value the replayed command
>      can produce is possible here.
>
> `c_redo.cmd` is initialised to `ED_UNASSIGNED` by `ch_init`, whose
> handler returns `CC_ERROR`, so `.` before any undoable command beeps.
> `r->cmd` is not re-validated here; it is trusted because `el_wgets`
> bounds-checks `thiscmd` against `el_map.nfunc` before dispatch.

> [spec:libedit:def:vi.vi-repeat-next-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_repeat_next_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-repeat-next-char-fn]
> Implements vi `;` — repeat the last `f`/`F`/`t`/`T` character search in
> the same direction. `c` is ignored.
>
> Tail-calls `cv_csearch(el, el_search.chadir, el_search.chacha,
> el_state.argument, el_search.chatflg)` — reusing the stored direction,
> target character and "till" flag, but taking a fresh count from the
> current `el_state.argument`.
>
> Return values:
>   - `CC_ERROR` if `el_search.chacha` is 0, i.e. no character search has
>     been performed yet on this `EditLine` (the helper's first test).
>     The state fields are not modified in that case.
>   - `CC_ERROR` if the scan runs off the end (`cp >= lastchar`) or the
>     start (`cp < buffer`) of the line before satisfying the count; the
>     cursor is left unmoved.
>   - `CC_REFRESH` if a vi operator is pending: the helper sets the
>     cursor to the match, advances it by one when the stored direction
>     is positive (making a forward search inclusive of the target),
>     calls `cv_delfini`, and returns.
>   - `CC_CURSOR` otherwise.
>
> The search state is rewritten with the same values before the scan, so
> `;` is idempotent with respect to `chacha`/`chadir`/`chatflg`. Because
> the helper only skips an adjacent match when the cursor is *on* the
> target — which it never is after a `t`/`T` — repeating `;` after a `t`
> re-finds the same target and the cursor does not move. That is the
> classic vi `t`-then-`;` behaviour and must be preserved.

> [spec:libedit:def:vi.vi-repeat-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_repeat_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-repeat-prev-char-fn]
> Implements vi `,` — repeat the last `f`/`F`/`t`/`T` character search in
> the **opposite** direction. `c` is ignored.
>
> Steps:
>   1. Save `dir = el_search.chadir`.
>   2. `r = cv_csearch(el, -dir, el_search.chacha, el_state.argument,
>      el_search.chatflg)` — the same target character and "till" flag, a
>      fresh count from `el_state.argument`, and the negated direction.
>      The helper overwrites `el_search.chadir` with `-dir` as part of
>      its normal bookkeeping.
>   3. Restore `el_search.chadir = dir`, undoing that overwrite, so a
>      subsequent `;` still goes in the original direction and a
>      subsequent `,` still goes opposite to it (repeated `,` does not
>      alternate).
>   4. Return `r` unchanged.
>
> Possible values of `r`: `CC_ERROR` when `el_search.chacha` is 0 (no
> prior character search) or when the scan hits either end of the line
> before satisfying the count; `CC_REFRESH` when a vi operator is pending
> (the helper sets the cursor to the match, adds one when the *negated*
> direction is positive, and calls `cv_delfini`); `CC_CURSOR` otherwise.
> Note the direction restore happens even on the error paths.

> [spec:libedit:def:vi.vi-repeat-search-next-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_repeat_search_next(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-repeat-search-next-fn]
> Implements vi `n` — repeat the last history search in the same
> direction. `c` is ignored.
>
>   1. If `el_search.patlen == 0` (no pattern has been entered yet)
>      return `CC_ERROR` with no side effects.
>   2. Otherwise tail-call `cv_repeat_srch(el, el_search.patdir)`, which
>      sets `el_state.lastcmd` to the direction value (a hack that stops
>      the history search from re-deriving the pattern from the current
>      line), **truncates the line by setting `lastchar = buffer`**, and
>      then dispatches to `ed_search_next_history(el, 0)` for
>      `ED_SEARCH_NEXT_HISTORY` or `ed_search_prev_history(el, 0)` for
>      `ED_SEARCH_PREV_HISTORY`, returning whatever those return
>      (`CC_REFRESH` on a hit, `CC_ERROR` on no match — in which case the
>      line has still been truncated). Any other stored direction value
>      yields `CC_ERROR`.
>
> `el_state.argument` is ignored: `3n` performs one search.

> [spec:libedit:def:vi.vi-repeat-search-prev-fn]
> libedit_private el_action_t vi_repeat_search_prev(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-repeat-search-prev-fn]
> Implements vi `N` — repeat the last history search in the **opposite**
> direction. `c` is ignored.
>
>   1. If `el_search.patlen == 0` return `CC_ERROR` with no side effects.
>   2. Otherwise tail-call `cv_repeat_srch(el, d)` where `d` is
>      `ED_SEARCH_NEXT_HISTORY` if `el_search.patdir` is
>      `ED_SEARCH_PREV_HISTORY`, and `ED_SEARCH_PREV_HISTORY` otherwise.
>      That helper sets `el_state.lastcmd = d` (suppressing pattern
>      re-derivation), truncates the line by setting `lastchar = buffer`,
>      and dispatches to `ed_search_next_history` /
>      `ed_search_prev_history`, returning `CC_REFRESH` on a hit and
>      `CC_ERROR` on no match.
>
> `el_search.patdir` is deliberately **not** updated, so `N` always
> searches opposite to the direction of the last explicit `/` or `?`;
> pressing `N` repeatedly keeps going the same way rather than
> alternating. `el_state.argument` is ignored.

> [spec:libedit:def:vi.vi-replace-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_replace_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-replace-char-fn]
> Implements vi `r` — replace the character(s) under the cursor with the
> next character typed. `c` is ignored.
>
>   1. If `el_line.cursor >= el_line.lastchar` (nothing under the cursor,
>      including the empty line) return `CC_ERROR`.
>   2. `el_map.current = el_map.key` — switch to the vi *insert* keymap,
>      so the next keystroke reaches `ed_insert`.
>   3. `el_state.inputmode = MODE_REPLACE_1` — one-shot overwrite mode.
>   4. `cv_undo(el)` — snapshot for undo, and record this command for
>      redo.
>   5. Return `CC_ARGHACK`, which suppresses any redraw and, crucially,
>      stops the dispatcher from resetting `el_state.argument`,
>      `el_state.doingarg` and `c_vcmd.action`. That is how `3rx`
>      reaches `ed_insert` with a count of 3.
>
> The follow-on behaviour lives in `ed_insert`: with `count == 1` and
> `MODE_REPLACE_1` and the cursor before `lastchar`, no gap is opened and
> the character simply overwrites `*cursor` before the cursor advances;
> with `count > 1` and `MODE_REPLACE_1` no gap is opened either and the
> character is written up to `count` times, stopping at `lastchar` (so
> `9rx` near the end of the line replaces only what is there, without
> error). `ed_insert` then observes `MODE_REPLACE_1` and tail-calls
> `vi_command_mode(el, 0)`, which clears the pending operator, clears
> `doingarg`, resets `inputmode` to `MODE_INSERT`, selects the vi command
> keymap and (under `VI_MOVE`) steps the cursor back one — so after `r`
> the cursor rests on the last replaced character and the action returned
> is `CC_CURSOR`.
>
> Note that only the *first* character is guaranteed to exist: step 1
> checks a single character, not `argument` of them.

> [spec:libedit:def:vi.vi-replace-mode-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_replace_mode(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-replace-mode-fn]
> Implements vi `R` — enter overwrite mode. `c` is ignored.
>
> Unconditionally: `el_map.current = el_map.key` (switch to the vi insert
> keymap); `el_state.inputmode = MODE_REPLACE` (sticky overwrite, as
> opposed to `r`'s one-shot `MODE_REPLACE_1`); `cv_undo(el)` to snapshot
> the line. Return `CC_NORM` — nothing visible changed, so no redraw.
>
> There is no error path, no cursor movement, and `el_state.argument` is
> ignored. `MODE_REPLACE` persists until `vi_command_mode` (`<ESC>`)
> resets `inputmode` to `MODE_INSERT`; while it is set, `ed_insert`
> overwrites instead of inserting whenever the cursor is before
> `lastchar`, and appends normally at end of line.

> [spec:libedit:def:vi.vi-search-next-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_search_next(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-search-next-fn]
> Implements the key bound to `?` in the vi command map. `c` is ignored.
> Tail-calls `cv_search(el, ED_SEARCH_NEXT_HISTORY)` — searching
> *forward* through history (towards more recent entries), prompting with
> `"\n?"`.
>
> `cv_search` sets `el_search.patdir` to the direction, reads a pattern
> with `c_gets` using that prompt, and then:
>   - returns `CC_REFRESH` if `c_gets` returned -1 (the user backspaced
>     past the start of the pattern, or input ended);
>   - if the pattern entered was empty, reuses `el_search.patbuf`; if
>     that is also empty (`patlen == 0`) it calls `re_refresh` and
>     returns `CC_ERROR`;
>   - otherwise stores the new pattern (wildcard-anchored with `.*` at
>     both ends when `ANCHOR` is compiled in), sets
>     `el_state.lastcmd` to the direction to suppress pattern
>     re-derivation, resets `cursor = lastchar = buffer`, and calls
>     `ed_search_next_history(el, 0)`. On `CC_ERROR` from that it calls
>     `re_refresh` and returns `CC_ERROR`;
>   - if the pattern was terminated with `<ESC>` (0033) rather than
>     newline, calls `re_refresh` and tail-calls `ed_newline(el, 0)`,
>     returning `CC_NEWLINE` — i.e. the found line is submitted
>     immediately;
>   - otherwise returns `CC_REFRESH`.
>
> `el_state.argument` is ignored. Note the prompt characters are crossed
> over relative to the key bindings: `?` invokes this "next" (forward in
> time) search, while `/` invokes `vi_search_prev`.

> [spec:libedit:def:vi.vi-search-prev-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_search_prev(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-search-prev-fn]
> Implements the key bound to `/` in the vi command map. `c` is ignored.
> Tail-calls `cv_search(el, ED_SEARCH_PREV_HISTORY)` — searching
> *backward* through history (towards older entries), prompting with
> `"\n/"`. This is the usual vi `/` behaviour, since history "forward" is
> backwards in time.
>
> `cv_search` sets `el_search.patdir` to the direction, reads a pattern
> with `c_gets` using that prompt, and then:
>   - returns `CC_REFRESH` if `c_gets` returned -1 (backspaced past the
>     start of the pattern, or input ended);
>   - if the pattern entered was empty, reuses the previous
>     `el_search.patbuf`; if there is none (`patlen == 0`) it calls
>     `re_refresh` and returns `CC_ERROR`;
>   - otherwise stores the new pattern (wildcard-anchored with `.*` at
>     both ends when `ANCHOR` is compiled in), sets `el_state.lastcmd` to
>     the direction to suppress pattern re-derivation, resets
>     `cursor = lastchar = buffer`, and calls
>     `ed_search_prev_history(el, 0)`. On `CC_ERROR` from that it calls
>     `re_refresh` and returns `CC_ERROR`;
>   - if the pattern was terminated with `<ESC>` (0033), calls
>     `re_refresh` and tail-calls `ed_newline(el, 0)`, returning
>     `CC_NEWLINE` — the matched line is submitted immediately;
>   - otherwise returns `CC_REFRESH`.
>
> `el_state.argument` is ignored.

> [spec:libedit:def:vi.vi-substitute-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_substitute_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-substitute-char-fn]
> Implements vi `s` — delete `el_state.argument` characters at the cursor
> and enter insert mode. `c` is ignored.
>
>   1. `c_delafter(el, el_state.argument)`. That helper first clamps the
>      count to `lastchar - cursor`, then — because
>      `el_map.current` is never equal to `el_map.emacs` (`emacs` is the
>      static default table while `current` is always the heap `key` or
>      `alt` copy) — it *always* calls `cv_undo(el)` and
>      `cv_yank(el, cursor, clamped_count)`, so the deleted text lands in
>      the kill buffer and the line is undoable. Finally, if the clamped
>      count is positive, it shifts `[cursor + n, lastchar]` down over
>      `[cursor, ...]` and decreases `lastchar` by `n`.
>   2. `el_map.current = el_map.key` — switch to the vi insert keymap.
>   3. Return `CC_REFRESH`.
>
> There is **no error path**: on an empty line or with the cursor at
> `lastchar` the clamped count is 0, so nothing is deleted, the kill
> buffer is set empty, an undo snapshot is still taken, insert mode is
> still entered, and `CC_REFRESH` is still returned. The cursor never
> moves.

> [spec:libedit:def:vi.vi-substitute-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_substitute_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-substitute-line-fn]
> Implements vi `S` — replace the entire line. `c` is ignored.
>
> In order:
>   1. `cv_undo(el)` — snapshot the whole line.
>   2. `cv_yank(el, el_line.buffer, (int)(el_line.lastchar -
>      el_line.buffer))` — copy the whole line into the kill buffer.
>   3. `em_kill_line(el, 0)` — which copies `[buffer, lastchar)` into the
>      kill buffer *again* (identical content, so the duplication is
>      harmless), sets `c_kill.last` accordingly, and empties the line by
>      setting both `lastchar` and `cursor` to `buffer`. Its `CC_REFRESH`
>      return is discarded.
>   4. `el_map.current = el_map.key` — switch to the vi insert keymap.
>   5. Return `CC_REFRESH`.
>
> There is no error path and `el_state.argument` is ignored. The result
> is identical to `cc` except that `S` requires no doubled keystroke and
> does not go through the pending-operator machinery.

> [spec:libedit:def:vi.vi-to-column-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_to_column(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-to-column-fn]
> Implements vi `|` — go to a specified column. `c` is ignored.
>
> Steps:
>   1. `el_line.cursor = el_line.buffer`.
>   2. Decrement `el_state.argument` (converting the 1-based column to a
>      0-based offset). This mutates the shared count, which the
>      dispatcher resets to 1 on return, so the mutation is not
>      observable afterwards. With no count given, `argument` is 1 and
>      becomes 0.
>   3. Tail-call `ed_next_char(el, 0)`, which:
>        - returns `CC_ERROR` if `cursor >= lastchar` — i.e. when the
>          line is empty;
>        - returns `CC_ERROR` if `cursor == lastchar - 1` **and**
>          `el_map.type == MAP_VI` **and** `c_vcmd.action == NOP` — i.e.
>          `|` on a one-character line with no operator pending is an
>          error;
>        - otherwise adds `el_state.argument` to the cursor and clamps it
>          up to `lastchar`. Note the clamp is to `lastchar`, not
>          `lastchar - 1`, so `999|` parks the cursor one past the last
>          character, which vi command mode otherwise never does;
>        - then, if `el_map.type == MAP_VI` and `c_vcmd.action != NOP`,
>          calls `cv_delfini(el)` and returns `CC_REFRESH`; otherwise
>          returns `CC_CURSOR`.
>
> So `n|` lands on the n-th character of the line, counting from 1 —
> POSIX's definition. libedit's own comment records that NetBSD vi
> instead goes to screen column n; this implementation counts characters,
> so wide and control characters that occupy more than one display column
> make the two definitions differ.
>
> With an operator pending, `c_vcmd.pos` is still the cursor from before
> step 1, so `d5|` deletes between the original position and
> `buffer + 4`, in whichever direction that is — `cv_delfini` picks
> `c_delafter` for a forward span and `c_delbefore` (plus a cursor
> adjustment) for a backward one.

> [spec:libedit:def:vi.vi-to-history-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_to_history_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-to-history-line-fn]
> Implements vi `G` — go to a numbered history line. `c` is ignored.
>
> Steps:
>   1. Save `sv_event_no = el_history.eventno`.
>   2. If `el_history.eventno == 0` — we are still editing the fresh,
>      unsaved line — stash it so it can be returned to:
>      `wcsncpy(el_history.buf, el_line.buffer, EL_BUFSIZ)` and
>      `el_history.last = el_history.buf + (lastchar - buffer)`.
>      **Bug:** the copy length is the compile-time constant `EL_BUFSIZ`
>      (1024) rather than `el_history.sz`, so after `ch_enlargebufs` has
>      grown the buffers a line longer than 1024 wide characters is
>      truncated in the copy while `el_history.last` still records the
>      full length — restoring it later yields the tail as whatever the
>      history buffer already held.
>   3. If `el_state.doingarg` is 0 (no count given): set
>      `el_history.eventno = 0x7fffffff` and call `hist_get(el)`,
>      discarding its result. That call walks history until it runs out,
>      and its failure path sets `el_history.eventno` to the last
>      reachable event index. So "no count" means **oldest entry**, not
>      entry 1.
>   4. Otherwise (a count `n` was given):
>        - set `el_history.eventno = 1` and call `hist_get(el)`; if it
>          returns `CC_ERROR`, return `CC_ERROR` (leaving `eventno` at 1;
>          it is not restored on this path). The purpose of this call is
>          to populate `el_history.ev`, whose `num` field is the event
>          number of the most recent entry;
>        - set `el_history.eventno = 1 + el_history.ev.num -
>          el_state.argument`. This inverts libedit's internal numbering
>          (which counts upward into the past) so that the count matches
>          what `fc -l` prints, oldest first;
>        - if the result is negative, restore `el_history.eventno =
>          sv_event_no` and return `CC_ERROR`.
>   5. `rval = hist_get(el)` — the call that actually loads the line.
>      It returns `CC_REFRESH` on success (copying the entry into
>      `el_line.buffer`, stripping one trailing `'\n'` and then one
>      trailing `' '`, and — because `KSHVI` is defined and the map type
>      is `MAP_VI` — placing the cursor at `buffer`, the beginning of the
>      line). It returns `CC_ERROR` when `el_history.ref` is null or the
>      requested event does not exist.
>   6. If `rval == CC_ERROR`, restore `el_history.eventno = sv_event_no`.
>      Return `rval`.
>
> No undo snapshot is taken, the keymap is not changed, and the kill
> buffer is untouched.

> [spec:libedit:def:vi.vi-to-next-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_to_next_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-to-next-char-fn]
> Implements vi `t` — move forward to just *before* the `argument`-th
> occurrence of a character read from the terminal. `c` is ignored.
>
> Tail-calls `cv_csearch(el, CHAR_FWD /* +1 */, (wint_t)-1,
> el_state.argument, /*tflag=*/1)`. Identical to `f` except for
> `tflag = 1`, which makes the helper back the landing position off by
> one step against the direction (`cp -= direction`, so one position to
> the *left* of the match) after the scan succeeds. That helper:
>   - reads one wide character with `el_wgetc`; a read not returning
>     exactly 1 yields `ed_end_of_file(el, 0)`, i.e. `CC_EOF`;
>   - records `el_search.chacha = target`, `chadir = +1`, `chatflg = 1`
>     before searching, so `;`/`,` are updated even on failure;
>   - for each of `argument` repetitions: if `*cp` already equals the
>     target step `cp` forward first, then advance until `*cp` equals the
>     target, returning `CC_ERROR` as soon as `cp >= lastchar`;
>   - applies the `tflag` back-off, sets `el_line.cursor = cp`;
>   - if a vi operator is pending, advances the cursor by one (direction
>     is positive), calls `cv_delfini(el)`, returns `CC_REFRESH` — so
>     `dt<ch>` deletes up to but not including the target character;
>   - otherwise returns `CC_CURSOR`.
>
> Because the "skip an adjacent match" test only fires when the cursor is
> *on* the target, and `t` never leaves it there, a following `;` re-finds
> the same occurrence and the cursor does not advance. Preserve that.

> [spec:libedit:def:vi.vi-to-prev-char-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_to_prev_char(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-to-prev-char-fn]
> Implements vi `T` — move backward to just *after* the `argument`-th
> occurrence of a character read from the terminal. `c` is ignored.
>
> Tail-calls `cv_csearch(el, CHAR_BACK /* -1 */, (wint_t)-1,
> el_state.argument, /*tflag=*/1)`. Identical to `F` except for
> `tflag = 1`, which backs the landing position off by one step against
> the direction (`cp -= direction` with `direction == -1`, so one
> position to the *right* of the match). That helper:
>   - reads one wide character with `el_wgetc`; a read not returning
>     exactly 1 yields `ed_end_of_file(el, 0)`, i.e. `CC_EOF`;
>   - records `el_search.chacha = target`, `chadir = -1`, `chatflg = 1`
>     before searching, so `;`/`,` are updated even on failure;
>   - for each of `argument` repetitions: if `*cp` already equals the
>     target step `cp` back first, then step back until `*cp` equals the
>     target, returning `CC_ERROR` as soon as `cp < buffer`;
>   - applies the `tflag` back-off, sets `el_line.cursor = cp`;
>   - if a vi operator is pending, the direction is negative so the
>     cursor is not advanced; calls `cv_delfini(el)` and returns
>     `CC_REFRESH`, which takes the `c_delbefore` branch and deletes
>     `[landing, original_cursor)`;
>   - otherwise returns `CC_CURSOR`.
>
> As with `t`, a following `;` re-finds the same occurrence and does not
> move the cursor.

> [spec:libedit:def:vi.vi-undo-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_undo(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-undo-fn]
> Implements vi `u` — undo the last change. `c` is ignored. It does not
> copy text; it **swaps** the line buffer and the undo buffer, which is
> what makes `u` its own inverse (`u u` returns to where you were).
>
> Steps:
>   1. Take a by-value copy `un` of `el_chared.c_undo` (`{len, cursor,
>      buf}`).
>   2. If `un.len == -1` return `CC_ERROR`. `len` is `-1` only from
>      `ch_init`/`ch_reset` until the first `cv_undo`, so this is the
>      "nothing to undo yet" case. After any undoable command it is never
>      `-1` again, so `u` always succeeds thereafter.
>   3. Write the *current* state into `c_undo`:
>      `c_undo.buf = el_line.buffer`,
>      `c_undo.len = lastchar - buffer`,
>      `c_undo.cursor = (int)(el_line.cursor - el_line.buffer)`.
>   4. Install the saved state as the line, preserving the limit offset:
>      `el_line.limit = un.buf + (el_line.limit - el_line.buffer)`,
>      then `el_line.buffer = un.buf`,
>      `el_line.cursor = un.buf + un.cursor`,
>      `el_line.lastchar = un.buf + un.len`.
>      Recomputing `limit` from the old offset is only sound because
>      `ch_enlargebufs` grows the line and undo allocations together and
>      keeps them the same size.
>   5. Return `CC_REFRESH`.
>
> Consequences the port must reproduce or consciously fix: after the swap
> `el_chared.c_kill.mark` and `el_chared.c_vcmd.pos` still point into the
> buffer that has just become the undo buffer, so they are stale; and
> `c_redo` is not touched, so `.` still replays the command that `u` just
> reverted. `el_state.argument` is ignored, the keymap and `inputmode`
> are unchanged, and the kill buffer is unchanged.

> [spec:libedit:def:vi.vi-undo-line-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_undo_line(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-undo-line-fn]
> Implements vi `U` — discard all edits to the current line. `c` is
> ignored.
>
> Two steps: `cv_undo(el)` — snapshot the current (edited) line first, so
> a following `u` can get back to it — then tail-call `hist_get(el)` and
> return its result.
>
> `hist_get` reloads the line from history according to
> `el_history.eventno`:
>   - `eventno == 0` (the fresh line): copy `el_history.buf` back into
>     `el_line.buffer` for `el_history.sz` elements and set `lastchar`
>     from `el_history.last`; returns `CC_REFRESH`.
>   - otherwise: return `CC_ERROR` if `el_history.ref` is null or the
>     event cannot be reached (in which case `el_history.eventno` is left
>     at the last reachable index); otherwise copy the entry in, strip
>     one trailing `'\n'` and then one trailing `' '`, and return
>     `CC_REFRESH`.
>   - in both success cases, because `KSHVI` is defined and
>     `el_map.type == MAP_VI`, the cursor is placed at `el_line.buffer`
>     (the beginning of the line) rather than at `lastchar`.
>
> So the return is `CC_REFRESH` on success and `CC_ERROR` on failure —
> and on failure `cv_undo` has still run, so the undo buffer has been
> overwritten with the current line. `el_state.argument` is ignored and
> the keymap is not changed. Note the difference from real vi, where `U`
> restores the line as it was on first entry; here it restores whatever
> the currently selected history event contains.

> [spec:libedit:def:vi.vi-yank-end-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_yank_end(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-yank-end-fn]
> Implements vi `Y`. `c` is ignored.
>
> A single action: `cv_yank(el, el_line.cursor, (int)(el_line.lastchar -
> el_line.cursor))` — copy `[cursor, lastchar)` into the kill buffer,
> replacing its contents and setting `c_kill.last` to `c_kill.buf +
> size`. Then return `CC_REFRESH`.
>
> Nothing is deleted, the cursor does not move, no undo snapshot is
> taken, the keymap is unchanged and `el_state.argument` is ignored.
> There is no error path: with the cursor at `lastchar` this yanks zero
> characters (leaving an empty kill buffer, which then makes `p`/`P`
> return `CC_ERROR`) and still returns `CC_REFRESH`.
>
> Divergence from real vi, which must be preserved: vi's `Y` yanks the
> **whole line**; libedit's yanks only from the cursor to end of line,
> i.e. it behaves as `y$`.

> [spec:libedit:def:vi.vi-yank-fn]
> libedit_private el_action_t /*ARGSUSED*/ vi_yank(EditLine *el, wint_t c __attribute__((__unused__)))

> [spec:libedit:sem:vi.vi-yank-fn]
> Implements the vi `y` operator prefix. `c` (the parameter) is ignored.
> Tail-calls the shared operator handler with the mask `YANK` (0x04).
>
> Consequently:
>   - With no operator pending: records `c_vcmd.pos = el_line.cursor` and
>     `c_vcmd.action = YANK`, and returns `CC_ARGHACK` — no redraw, and
>     the pending operator plus any accumulated count survive into the
>     next keystroke, where the motion command calls `cv_delfini`.
>     Because `YANK` is set, `cv_delfini` takes its yank branch: it
>     copies the spanned text into the kill buffer (choosing the correct
>     start for a backward span) and does **not** modify the line, but it
>     still resets the cursor to `c_vcmd.pos`, so `yw` leaves the cursor
>     where the `y` was pressed.
>   - With `YANK` already pending (the `yy` form): copies the whole line
>     `[buffer, lastchar)` into the kill buffer, clears
>     `c_vcmd.action`/`c_vcmd.pos`, and returns `CC_REFRESH`. Because
>     `(c & YANK)` is set, **no** undo snapshot is taken and the line,
>     cursor and keymap are left untouched. The count is ignored, so
>     there is no `3yy`.
>   - With a different operator pending (`dy`, `cy`): returns `CC_ERROR`
>     with no side effects.

> [spec:libedit:def:vi.vi-zero-fn]
> libedit_private el_action_t vi_zero(EditLine *el, wint_t c)

> [spec:libedit:sem:vi.vi-zero-fn]
> Implements the `0` key in vi command mode, which is overloaded: it is
> the "go to column 0" motion when no count is being entered, and the
> digit zero when one is. `c` is the character that invoked it (`L'0'`
> under the default binding).
>
>   1. If `el_state.doingarg` is set, tail-call `ed_argument_digit(el,
>      c)` and return its result. That function returns `CC_ERROR` if
>      `c` is not `iswdigit` (only reachable if the user rebinds this
>      function to a non-digit key) or if `el_state.argument` already
>      exceeds 1000000; otherwise it sets `el_state.argument =
>      el_state.argument * 10 + (c - '0')` and returns `CC_ARGHACK`,
>      which keeps the count and any pending operator alive for the next
>      keystroke. (The "start a new argument" branch of
>      `ed_argument_digit` is unreachable from here, since `doingarg` is
>      already 1.)
>   2. Otherwise set `el_line.cursor = el_line.buffer`.
>   3. If a vi operator is pending (`c_vcmd.action != NOP`): call
>      `cv_delfini(el)` and return `CC_REFRESH`. The landing position is
>      at or left of `c_vcmd.pos`, so `cv_delfini` takes the negative
>      (or zero-normalised-to-one) size branch: `d0` deletes
>      `[buffer, c_vcmd.pos)` — everything before the cursor, exclusive
>      of the character under it — and leaves the cursor at `buffer`.
>   4. Otherwise return `CC_CURSOR`.
>
> There is no error path in the motion branch: `0` on an empty line
> simply leaves the cursor at `buffer` and returns `CC_CURSOR`.

