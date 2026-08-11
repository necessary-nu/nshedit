# src/search.c, src/search.h

> [spec:libedit:def:search.c-hmatch-fn]
> libedit_private int c_hmatch(EditLine *el, const wchar_t *str)

> [spec:libedit:sem:search.c-hmatch-fn]
> Answers the question "does this history line match the search pattern
> currently held in `el->el_search.patbuf`?".
>
> 1. Under `SDEBUG` — a debug macro no shipped build defines — it first
>    writes ``match `<patbuf>' with `<str>'\n`` to `el->el_errfile`.
>    That trace is the only thing `el` is used for besides reaching the
>    pattern buffer. It is compiled out and is not ported.
> 2. Returns `el_match(str, el->el_search.patbuf)`.
>
> IMPORTANT: the argument order is subject-first, pattern-second.
> `str` is the candidate (a history entry, or `el->el_history.buf` for
> the not-yet-committed current line) and `el->el_search.patbuf` is the
> pattern. Getting this backwards silently inverts the whole history
> search.
>
> Returns 1 for a match and 0 for no match; the match rule is exactly
> the one specified for `el_match`, including its literal-substring
> fast path, its POSIX basic-regular-expression fallback and its
> treatment of a pattern that fails to compile. In particular an empty
> `patbuf` matches every candidate, because the fast path's substring
> search for the empty string succeeds immediately.
>
> `c_hmatch` itself mutates no `EditLine` state. `el_match` does own a
> hidden static encoding buffer; see its rule.
>
> Called only from `ed_search_prev_history` and `ed_search_next_history`
> in `common.c`, once per candidate history entry, after those functions
> have called `c_setpat` to (re)establish the pattern.

> [spec:libedit:def:search.c-setpat-fn]
> libedit_private void c_setpat(EditLine *el)

> [spec:libedit:sem:search.c-setpat-fn]
> Derives the history-search pattern from the text the user has typed,
> unless the previous command was itself a history search — in which
> case the existing pattern is left alone so that repeated searches keep
> hunting for the same thing.
>
> 1. If `el->el_state.lastcmd` is either `ED_SEARCH_PREV_HISTORY` or
>    `ED_SEARCH_NEXT_HISTORY`, do nothing at all and return. This is the
>    entire guard; every caller that wants to protect a pattern it has
>    already built (`ce_inc_search`, `cv_search`, `cv_repeat_srch`)
>    fakes it by assigning one of those two command codes to
>    `el->el_state.lastcmd` before calling into the history search.
> 2. Otherwise compute the end of the prefix as
>    `EL_CURSOR(el)`, which is `el->el_line.cursor` plus one extra
>    position when *both* `el->el_map.type == MAP_VI` and
>    `el->el_map.current == el->el_map.alt` — i.e. in vi command mode
>    the character sitting under the cursor is included, in emacs mode
>    and vi insert mode it is not.
> 3. Set `el->el_search.patlen = EL_CURSOR(el) - el->el_line.buffer`,
>    computed as a `size_t`.
> 4. If `patlen >= EL_BUFSIZ` (1024), clamp it to `EL_BUFSIZ - 1`. This
>    keeps the terminator write in step 6 inside the 1024-element
>    `patbuf` allocation.
> 5. `wcsncpy(el->el_search.patbuf, el->el_line.buffer, patlen)` —
>    copy that many characters from the start of the line, with
>    `wcsncpy`'s padding rule: if the source hits a NUL first the rest
>    of the `patlen` slots are filled with NUL rather than copied.
> 6. Write `L'\0'` at `patbuf[patlen]`.
> 7. Under `SDEBUG` it then dumps `eventno`, `patlen`, `patbuf` and the
>    cursor/lastchar offsets to `el->el_errfile`. Compiled out; not
>    ported.
>
> `el->el_search.patdir` is *not* touched here; only `cv_search` sets
> it.
>
> Consequences the port must preserve:
>
> - The pattern produced is the raw typed prefix with **no anchors and
>   no metacharacter escaping**. Combined with `el_match`, that makes
>   the emacs `M-p`/`M-n` and vi `K`/`J` history search a *substring*
>   test, not the prefix test the function's own comment implies: a
>   history entry matches if the typed text occurs anywhere in it.
> - If the cursor is at the start of the line, `patlen` is 0 and
>   `patbuf` becomes the empty string, which `el_match` reports as
>   matching everything. So `M-p` with an empty line walks history one
>   entry at a time.
> - Because the prefix is used verbatim as a POSIX BRE, typed text
>   containing `.`, `*`, `[`, `^`, `$` or `\` changes the match, and
>   text that is not a valid BRE (an unclosed `[`, a trailing `\`)
>   makes the regex leg fail to compile so only the literal-substring
>   leg of `el_match` can still match.
> - In vi command mode with the cursor at end of line, `EL_CURSOR`
>   yields `lastchar + 1`, so `patlen` counts one slot past the last
>   character. The callers write `*el->el_line.lastchar = '\0'` first,
>   so what actually gets copied is the line plus its terminator and
>   `wcsncpy`'s NUL padding; the effective pattern is still the line
>   text. The port should just clamp the prefix end to `lastchar`.
> - A cursor below `buffer` would make the `size_t` subtraction wrap to
>   an enormous value, which step 4 would clamp to `EL_BUFSIZ - 1` and
>   then read 1023 characters from the line buffer. No caller does
>   this; it is not a reachable state, only an absent guard.

> [spec:libedit:def:search.ce-inc-search-fn]
> libedit_private el_action_t ce_inc_search(EditLine *el, int dir)

> [spec:libedit:sem:search.ce-inc-search-fn]
> The emacs incremental search (`^R` / `^S`) read-and-match loop. It
> owns the terminal until the search ends: it draws its own prompt after
> the line, reads one keystroke, folds it into the state machine,
> re-runs the search, and then **calls itself recursively** to read the
> next keystroke. The recursion is the undo stack — each level's saved
> locals are the state to roll back to, so one backspace is one level
> popped.
>
> Entered only from `em_inc_search_prev` and `em_inc_search_next`, both
> of which set `el->el_search.patlen = 0` immediately before the call
> and pass `dir` as `ED_SEARCH_PREV_HISTORY` or
> `ED_SEARCH_NEXT_HISTORY` respectively. The port may rely on
> `patlen == 0` at the outermost entry.
>
> Throughout, `LEN` is 2, because `ANCHOR` is unconditionally defined in
> `el.h`. `patbuf` always carries a two-character `".*"` prefix that is
> never displayed and never counted as user text; a matching `".*"`
> suffix is appended just before each search and stripped again right
> after.
>
> **Statics.** Three function-level statics:
> `pchar`, initially `L':'`, the prompt punctuation — `':'` means the
> last search succeeded, `'?'` means it failed; `endcmd[2]`, a scratch
> two-element string used to push a terminating keystroke back into the
> input; and the two constant strings `L"fwd"` and `L"bck"`. `pchar` and
> `endcmd` are shared by every recursion level *and* by every `EditLine`
> instance in the process, so the C is not reentrant across editors or
> threads. A port may hold both as state threaded through the
> invocation chain; for a single editor that is observationally
> identical, and it removes the cross-instance hazard.
>
> **Per-level saved state**, captured on entry before anything else:
> `ocursor = el->el_line.cursor`, `oldpchar = pchar`,
> `ohisteventno = el->el_history.eventno`,
> `oldpatlen = el->el_search.patlen`. Also `newdir = dir` and
> `ret = CC_NORM`.
>
> **Entry bound check.** If
> `el->el_line.lastchar + 4 + 2 + el->el_search.patlen >= el->el_line.limit`
> return `CC_ERROR` immediately. (The 4 is
> `sizeof(L"fwd") / sizeof(wchar_t)`, i.e. three characters plus the
> terminator.) This reserves room for the prompt this level is about to
> append to the live line buffer. Nothing has been modified yet, so the
> failure is clean.
>
> Then loop forever:
>
> 1. **First round.** If `el->el_search.patlen == 0`: set `pchar = ':'`,
>    write `'.'` and `'*'` into `patbuf[0]` and `patbuf[1]`, and set
>    `patlen = 2`.
> 2. Clear the per-iteration flags `done = redo = 0`.
> 3. **Draw the prompt into the line buffer itself.** Append, starting
>    at `el->el_line.lastchar` and advancing it: `'\n'`; then `"bck"` if
>    `newdir == ED_SEARCH_PREV_HISTORY` else `"fwd"`; then `pchar`; then
>    the characters `patbuf[2 .. patlen)`. Finally write `'\0'` at
>    `lastchar` *without* advancing. The search UI is therefore a second
>    display line appended to the user's real line, e.g.
>    `bck:foo` or `fwd?foo`, and `lastchar` temporarily includes it.
> 4. `re_refresh(el)`.
> 5. `el_wgetc(el, &ch)`. If it does not return 1, return
>    `ed_end_of_file(el, 0)` (which does `re_goto_bottom`, writes
>    `'\0'` at `lastchar`, and yields `CC_EOF`). NOTE: this bails out
>    *without* stripping the prompt appended in step 3 and without
>    restoring cursor, `patlen` or `eventno` at this or any outer level;
>    the line buffer is left holding the user's text plus
>    `"\nbck:<pattern>"`. That is a bug, and the port should strip the
>    prompt before returning `CC_EOF`.
> 6. **Dispatch on `el->el_map.current[(unsigned char) ch]`** — the
>    *low byte* of the wide character is used as the keymap index.
>    IMPORTANT: this truncation is a real defect. Every non-ASCII
>    character is looked up under `ch & 0xff`, and the default emacs map
>    has `ED_UNASSIGNED` for indices 128-255, so typing an accented
>    letter into an incremental search does not extend the pattern —
>    it falls through to the `default` arm below and *terminates* the
>    search, pushing the character back for re-execution. Only U+0000-
>    U+007F can be searched for. (Contrast `read_getcmd`, which refuses
>    to map anything `>= 256` at all.) The port should specify this
>    behaviour deliberately rather than reproduce the truncation by
>    accident.
>
>    - `ED_INSERT` or `ED_DIGIT`: if `patlen >= EL_BUFSIZ - 2` call
>      `terminal_beep(el)` and change nothing; otherwise append `ch` to
>      `patbuf` (incrementing `patlen`), append `ch` to the line buffer
>      at `lastchar` (advancing it), write `'\0'`, and `re_refresh(el)`
>      so the keystroke echoes before the search runs.
>    - `EM_INC_SEARCH_NEXT`: `newdir = ED_SEARCH_NEXT_HISTORY`,
>      `redo++`.
>    - `EM_INC_SEARCH_PREV`: `newdir = ED_SEARCH_PREV_HISTORY`,
>      `redo++`.
>      These two are how the direction flips mid-search, and how a
>      repeat of the same direction is signalled. `patbuf` is untouched.
>    - `EM_DELETE_PREV_CHAR` or `ED_DELETE_PREV_CHAR`: if
>      `patlen > 2` set `done++`; otherwise `terminal_beep(el)`.
>      Note it deletes nothing directly. Setting `done` makes this level
>      return `CC_NORM` after the restore in step 10, which drops the
>      caller back to *its* pattern — one character shorter. Backspace
>      is implemented purely by unwinding one recursion level.
>    - anything else: dispatch a second time, on the raw wide `ch`:
>      - `0007` (`^G`, abort): `ret = CC_ERROR`, `done++`.
>      - `0027` (`^W`, append the rest of the current word to the
>        pattern): scan `cp` from `&patbuf[2]` upward. If any character
>        strictly before `&patbuf[patlen]` satisfies `isglob` — that is,
>        it is one of `*`, `[`, `]`, `?` — `terminal_beep(el)` and do
>        nothing else (the pattern is a regex and the cursor is not
>        reliably on a literal match). Otherwise, on reaching
>        `cp >= &patbuf[patlen]`:
>        - if `el->el_line.cursor == el->el_line.buffer`, do nothing;
>        - else advance `el->el_line.cursor` by `patlen - 2 - 1`, which
>          lands it on the last character of the current match; compute
>          `cp = c__next_word(el, cursor, lastchar, 1, ce__isword)`; then
>          while `cursor < cp` and `*cursor != '\n'`, append `*cursor` to
>          both `patbuf` (incrementing `patlen`) and the line buffer at
>          `lastchar`, advancing `cursor` — stopping early with
>          `terminal_beep(el)` if `patlen` reaches `EL_BUFSIZ - 2`.
>          Then restore `el->el_line.cursor = ocursor`, write `'\0'` at
>          `lastchar`, and `re_refresh(el)`.
>        The `*cursor != '\n'` test is what stops the copy from running
>        into the prompt this level appended in step 3; note `high` for
>        `c__next_word` is the *inflated* `lastchar` that includes the
>        prompt.
>        Two defects here. (a) When `patlen == 2` (`^W` as the very
>        first keystroke of a search) `patlen - 2 - 1` is computed in
>        `size_t` and wraps to `SIZE_MAX`; adding that to a `wchar_t *`
>        is undefined behaviour, and on the usual two's-complement
>        target it happens to move the cursor back by one element. The
>        port must choose a defined behaviour — treating it as
>        "start at the cursor" is the sane reading. (b) The append loop
>        is bounded only against `EL_BUFSIZ` for `patbuf`, never against
>        `el->el_line.limit` for the line buffer; the entry check only
>        guaranteed about three spare slots past the prompt, so `^W` on
>        a nearly-full line with a long word at the cursor writes past
>        `limit` and past the allocation. A port must bound this append
>        by the line capacity.
>      - `0033` (`ESC`, terminate): `ret = CC_REFRESH`, `done++`.
>      - any other character: store it in `endcmd[0]` (with
>        `endcmd[1]` already `'\0'`), `el_wpush(el, endcmd)` to queue it
>        as the next input so it executes as a normal command once the
>        search returns, then **fall through** to the `ESC` case —
>        `ret = CC_REFRESH`, `done++`.
> 7. **Strip the prompt.** While `lastchar > buffer` and
>    `*lastchar != '\n'`, write `'\0'` at `lastchar` and decrement it;
>    then write `'\0'` at `lastchar`. This walks back over everything
>    appended in step 3 (and by `ED_INSERT`/`^W`) and consumes the
>    leading `'\n'` too, so `lastchar` ends exactly where it was on
>    entry to this iteration. Nothing the search machinery puts in
>    `patbuf` can contain `'\n'`, so the scan cannot stop early; the
>    `lastchar > buffer` guard would otherwise clear the whole line.
> 8. **If `done` is still 0, run a search.**
>    - a. *Unmatched-`[` check.* Scan `cp` from `&patbuf[patlen - 1]`
>      down to `&patbuf[2]`, starting with `ch = L']'`; on the first
>      `'['` or `']'` found, set `ch = *cp` and stop. So `ch` ends up
>      `'['` exactly when the last bracket character in the pattern is
>      an opening one. (When `patlen == 2` the loop body never runs and
>      `ch` stays `']'`.)
>    - b. If `patlen > 2` **and** `ch != L'['` — i.e. there is real
>      pattern text and it is not sitting inside a half-typed bracket
>      expression — do the search. An unmatched `'['` simply skips the
>      search for this keystroke, leaving the display and history where
>      they are, so the user can finish typing the bracket expression.
>      - i. *Advance past the current match, or wrap.* Only if `redo` is
>        non-zero **and** `newdir == dir` (the user pressed the same
>        direction key that started this level, not a flip):
>        - if `pchar == '?'` (the previous search failed, so wrap
>          around): set `el->el_history.eventno` to `0` for
>          `ED_SEARCH_PREV_HISTORY` or `0x7fffffff` for
>          `ED_SEARCH_NEXT_HISTORY`; call `hist_get(el)`, and if it
>          returns `CC_ERROR` call `hist_get(el)` a second time — the
>          first call clamps `eventno` to the last real event as a side
>          effect of failing, and the second call then loads it. Set
>          `el->el_line.cursor` to `lastchar` for a backward search or
>          `buffer` for a forward one, so the in-line scan starts at the
>          far end.
>        - else nudge `el->el_line.cursor` by `-1` for
>          `ED_SEARCH_PREV_HISTORY` or `+1` for
>          `ED_SEARCH_NEXT_HISTORY`, so the in-line scan cannot re-find
>          the match it is already sitting on.
>        When the keystroke was an ordinary character rather than a
>        repeat, neither happens: the search restarts at the current
>        cursor, which is how an extended pattern re-matches the same
>        position.
>      - ii. Append the trailing anchor: write `'.'` and `'*'` at
>        `patbuf[patlen]` and `patbuf[patlen+1]`, adding 2 to `patlen`,
>        then write `'\0'` at `patbuf[patlen]`. The pattern handed to
>        the matcher is thus `".*" + <typed> + ".*"`.
>      - iii. *Search the current line first, then history.* If
>        `cursor < buffer`, or `cursor > lastchar`, or
>        `(ret = ce_search_line(el, newdir)) == CC_ERROR`, then: set
>        `el->el_state.lastcmd = (el_action_t) newdir` to stop
>        `c_setpat` from overwriting the pattern; set `ret` to
>        `ed_search_prev_history(el, 0)` or
>        `ed_search_next_history(el, 0)` according to `newdir`; and if
>        that did not return `CC_ERROR`, put the cursor at `lastchar`
>        (backward) or `buffer` (forward) of the newly loaded history
>        line and call `ce_search_line(el, newdir)` again, discarding
>        its result, to place the cursor on the match within that line.
>        Note the `||` short-circuits: when the cursor is out of range
>        `ce_search_line` is not called at all.
>      - iv. Strip the trailing anchor again: `patlen -= 2` and write
>        `'\0'` at `patbuf[patlen]`. The leading `".*"` stays.
>      - v. *Record success or failure.* If `ret == CC_ERROR`:
>        `terminal_beep(el)`; if `el->el_history.eventno != ohisteventno`
>        restore `eventno = ohisteventno` and call `hist_get(el)`,
>        returning `CC_ERROR` outright if that itself fails; restore
>        `el->el_line.cursor = ocursor`; and set `pchar = '?'` so the
>        next level's prompt renders as `bck?`. Otherwise set
>        `pchar = ':'`.
>    - c. **Recurse:** `ret = ce_inc_search(el, newdir)`. This is where
>      the next keystroke is read. The direction passed down is the
>      current `newdir`, so a `^R` after a `^S` counts as a flip at this
>      level but as a repeat at the next.
>    - d. If the recursion returned `CC_ERROR` **and** `pchar == '?'`
>      **and** `oldpchar == ':'`, rewrite `ret = CC_NORM`. This is the
>      "break abort of failed search at last non-failed" rule: a `^G`
>      pressed while the search is failing propagates up through every
>      level whose own search failed and is absorbed by the last level
>      that was still succeeding, which then resumes the loop instead of
>      aborting. A second `^G` from that resumed state aborts for real.
> 9. **Restore-on-unwind.** If `ret == CC_NORM`, or
>    (`ret == CC_ERROR` and `oldpatlen == 0`, which is only true at the
>    outermost level): set `pchar = oldpchar` and
>    `el->el_search.patlen = oldpatlen`; if
>    `el->el_history.eventno != ohisteventno`, restore it and call
>    `hist_get(el)`, returning `CC_ERROR` immediately if that fails; set
>    `el->el_line.cursor = ocursor`; and if `ret == CC_ERROR` also call
>    `re_refresh(el)`. `CC_REFRESH` and `CC_EOF` deliberately skip this,
>    so a terminated search keeps the history entry it landed on, the
>    cursor position of the match, and the pattern.
> 10. If `done` is set, or `ret != CC_NORM`, return `ret`. Otherwise
>     loop back to step 1 — which, after a restore that put `patlen`
>     back to 0 at the outermost level, re-seeds the `".*"` prefix and
>     resets `pchar` to `':'`.
>
> **Returned values as seen by `em_inc_search_prev`/`next`:**
> `CC_ERROR` when the entry bound check fails, when `^G` aborts a search
> that never failed, or when a `hist_get` during restore fails;
> `CC_REFRESH` when the search is terminated by `ESC` or by any other
> command key (that key having been pushed back with `el_wpush`);
> `CC_EOF` when the terminal reports end of file. `CC_NORM` is an
> internal value used between recursion levels and, given that both
> callers zero `patlen` first, cannot escape the outermost level.
>
> **What persists after a `CC_REFRESH` termination:**
> `el->el_search.patbuf` holds `".*" + <typed text>` and
> `el->el_search.patlen` counts both the prefix and the text, the
> history event is the one found, and the cursor is on the match.
> `el->el_search.patdir` is never written here.

> [spec:libedit:def:search.ce-search-line-fn]
> libedit_private el_action_t ce_search_line(EditLine *el, int dir)

> [spec:libedit:sem:search.ce-search-line-fn]
> Finds the pattern *inside the line currently in the edit buffer* and
> moves the cursor to the start of the match. This is the intra-line
> half of the incremental search; `ce_inc_search` is its only caller.
>
> Precondition, and it is not checked: `el->el_search.patbuf` must begin
> with a two-character throwaway prefix. With `ANCHOR` defined (always,
> from `el.h`) that prefix is `".*"`, written by `ce_inc_search`.
>
> 1. Let `cp = el->el_line.cursor` and `pattern = el->el_search.patbuf`.
> 2. Set `ocp = &pattern[1]`, save `oc = *ocp` (the `'*'`), and
>    overwrite `*ocp` with `'^'`. The effective pattern is therefore the
>    string starting at `patbuf[1]`, i.e. `"^" + <typed text> + ".*"` —
>    a POSIX BRE anchored at the start of whatever subject it is handed.
>    IMPORTANT: this temporarily **mutates the shared pattern buffer**.
>    Every return path restores `*ocp = oc` first, but during the call
>    `patbuf` is corrupt, so nothing reentrant may observe it. A Rust
>    port should build the anchored pattern as a separate value instead.
> 3. If `dir == ED_SEARCH_PREV_HISTORY`, walk `cp` **downward** from the
>    cursor while `cp >= el->el_line.buffer`; otherwise walk `cp`
>    **upward** while `*cp != '\0'` and `cp < el->el_line.limit`. Note
>    the forward loop's stop conditions: the NUL test comes first, so it
>    halts at the line terminator, and the fallback bound is `limit`,
>    not `lastchar`.
> 4. At each position call `el_match(cp, ocp)` — subject is the line
>    suffix beginning at `cp`, pattern is the anchored string. On the
>    first success: restore `*ocp = oc`, set
>    `el->el_line.cursor = cp`, and return `CC_NORM`.
> 5. If the walk runs out: restore `*ocp = oc` and return `CC_ERROR`,
>    leaving the cursor untouched.
>
> Because the pattern is anchored with `^`, "matches at `cp`" means the
> text at `cp` starts with the pattern, so the backward walk lands on
> the **latest** match at or before the cursor and the forward walk on
> the **earliest** match at or after it. The cursor position itself is
> always tried first in both directions; `ce_inc_search` is what nudges
> the cursor by one beforehand when the user is repeating a search.
>
> Two things the port must get right:
>
> - The anchoring depends on `^` being an anchor in the regex dialect.
>   In a POSIX BRE with `cflags == 0`, `^` anchors only at the start of
>   the whole pattern, which is exactly where it lands here. A port that
>   swaps in a different engine must anchor explicitly rather than by
>   string-prepending `^`, and must keep the trailing `.*` harmless.
> - `el_match` tries a literal substring search *before* the regex. For
>   the string built here that fast path asks whether the line literally
>   contains the characters `^`, the typed text, `.` and `*`, which
>   essentially never holds — so in practice the regex leg does all the
>   work, and any pattern that fails to compile as a BRE makes this
>   function return `CC_ERROR` for every position.
>
> The backward loop decrements `cp` one past `el->el_line.buffer` before
> the guard rejects it; forming that pointer is undefined behaviour in
> C. It works on every real target and the port simply uses a bounded
> index.

> [spec:libedit:def:search.cv-csearch-fn]
> libedit_private el_action_t cv_csearch(EditLine *el, int direction, wint_t ch, int count, int tflag)

> [spec:libedit:sem:search.cv-csearch-fn]
> The vi single-character line search behind `f`, `F`, `t`, `T`, `;`
> and `,`. No regular expressions and no history are involved; it moves
> the cursor within the current line only. `direction` is `CHAR_FWD`
> (`+1`) or `CHAR_BACK` (`-1`); `tflag` is 0 for `f`/`F` (land *on* the
> character) and 1 for `t`/`T` (land *next to* it); `count` is the
> repeat count, `el->el_state.argument`.
>
> 1. If `ch == 0`, return `CC_ERROR`. This is the "nothing remembered
>    yet" case: `el->el_search.chacha` starts as `L'\0'` from
>    `search_init`, so `;` or `,` before any `f`/`F`/`t`/`T` fails here.
> 2. If `ch == (wint_t)-1`, that is the sentinel meaning "read the
>    target character from the terminal now", used by `f`/`F`/`t`/`T`:
>    call `el_wgetc(el, &c)` and, if it does not return 1, return
>    `ed_end_of_file(el, 0)` (i.e. `CC_EOF`); otherwise `ch = c`.
>    `(wint_t)-1` is `WEOF` on the usual platform; `el_wgetc` only
>    reports success for a real character so the sentinel cannot collide
>    with input in practice, but a port should use an explicit optional
>    instead of an in-band sentinel.
> 3. **Remember the search, before running it**: `el->el_search.chacha
>    = ch`, `el->el_search.chadir = direction`,
>    `el->el_search.chatflg = (char) tflag`. These are what `;` and `,`
>    replay. They are updated even when the search below fails, and even
>    when a pending vi operator is left dangling.
> 4. Let `cp = el->el_line.cursor`. Repeat `count` times (`while
>    (count--)`, so a `count` of 0 does nothing and leaves `cp` at the
>    cursor):
>    - If `*cp == ch`, first step `cp += direction`, so a search never
>      re-finds the character already under the cursor.
>    - Then loop: if `cp >= el->el_line.lastchar` return `CC_ERROR`; if
>      `cp < el->el_line.buffer` return `CC_ERROR`; if `*cp == ch`
>      stop; otherwise `cp += direction` and repeat. The bounds are
>      checked before each dereference, and the position at `lastchar`
>      (the terminator) is excluded, so only real line characters can
>      match.
> 5. If `tflag`, step back off the target: `cp -= direction`.
> 6. `el->el_line.cursor = cp`.
> 7. If `el->el_chared.c_vcmd.action != NOP` — a vi operator such as
>    `d` or `c` is pending and this was its motion — then for a forward
>    direction increment `el->el_line.cursor` once more, making the
>    motion inclusive of the target character; call `cv_delfini(el)` to
>    perform the delete/yank between `c_vcmd.pos` and the cursor; and
>    return `CC_REFRESH`.
> 8. Otherwise return `CC_CURSOR`.
>
> On any of the `CC_ERROR` returns in step 4 the cursor is not moved
> (`cp` is a local), but `chacha`/`chadir`/`chatflg` have already been
> overwritten and a pending vi operator is left pending with
> `c_vcmd.action` still set — the C never clears it on this path.
>
> Fidelity notes for the port:
>
> - The step-off-the-current-character rule in step 4 runs on **every**
>   iteration of the count loop and applies to `t`/`T` as well as
>   `f`/`F`. After a `t`, the cursor sits one position before the
>   target, so repeating with `;` finds that same target again and step
>   5 puts the cursor straight back where it started: `t` followed by
>   `;` does not move. Real vi special-cases this. Reproduce libedit's
>   behaviour, not vi's, unless the port is deliberately diverging.
> - Comparison is exact wide-character equality (`(wint_t)*cp == ch`) —
>   no case folding, no locale collation, no combining-mark handling.
>   A multi-code-point grapheme is only ever matched by its first code
>   point.
> - Step 4's first dereference happens before any bound check, so a
>   caller with `cursor > lastchar` reads out of range. In vi command
>   mode the cursor is at most `lastchar - 1`, and reading the NUL at
>   `lastchar` is in-bounds, so this is unreachable in practice.
> - The backward walk forms `buffer - 1` before rejecting it, which is
>   undefined behaviour in C; use a bounded index.

> [spec:libedit:def:search.cv-repeat-srch-fn]
> libedit_private el_action_t cv_repeat_srch(EditLine *el, wint_t c)

> [spec:libedit:sem:search.cv-repeat-srch-fn]
> Re-runs the last vi history search (`n` and `N`) using the pattern
> already in `el->el_search.patbuf`. `c` is the direction to search in,
> supplied by the caller: `vi_repeat_search_next` passes
> `el->el_search.patdir` unchanged, `vi_repeat_search_prev` passes the
> opposite of it. Both callers refuse to call this when
> `el->el_search.patlen == 0`.
>
> 1. Under `SDEBUG` (never defined in a shipped build) it prints the
>    direction, `patlen` and the multibyte-encoded `patbuf` to
>    `el->el_errfile`, using its own function-level static
>    `ct_buffer_t`. Compiled out; not ported.
> 2. `el->el_state.lastcmd = (el_action_t) c`. This is the standard
>    trick to stop the `c_setpat` call inside
>    `ed_search_prev_history`/`ed_search_next_history` from replacing
>    the pattern with the current line prefix — `c` is one of the two
>    command codes `c_setpat` tests for. Note `el_action_t` is an
>    `unsigned char`, so the value is truncated to 8 bits; the two
>    command codes fit.
> 3. `el->el_line.lastchar = el->el_line.buffer`, truncating the current
>    line to empty so the prefix comparison inside the history search
>    (`wcsncmp` over `lastchar - buffer` characters) is vacuous and
>    every history entry is judged by the pattern alone.
> 4. Dispatch on `c`: `ED_SEARCH_NEXT_HISTORY` returns
>    `ed_search_next_history(el, 0)`; `ED_SEARCH_PREV_HISTORY` returns
>    `ed_search_prev_history(el, 0)`; anything else returns `CC_ERROR`.
>    Those two return `CC_REFRESH` on a hit (via `hist_get`, which also
>    reloads the line and repositions the cursor) and `CC_ERROR` when no
>    further history entry matches.
>
> `el->el_search.patdir` is not updated here, so `N` searches the
> opposite way without making that the new default direction — the next
> `n` still follows the direction the original `/` or `?` established.
>
> Bug the port should fix: step 3 moves `lastchar` but never `cursor`.
> On the `CC_ERROR` paths — an unmatched pattern, or a `c` that is
> neither command code — nothing puts the cursor back, so it is left
> pointing past `lastchar` at a position that is no longer part of the
> line. On the success path `hist_get` reassigns both, hiding the
> problem. Set the cursor to `buffer` alongside `lastchar`.

> [spec:libedit:def:search.cv-search-fn]
> libedit_private el_action_t cv_search(EditLine *el, int dir)

> [spec:libedit:sem:search.cv-search-fn]
> The vi `/` and `?` history search: prompt for a pattern on the line,
> then jump to the most recent matching history entry. Unlike the emacs
> incremental search this is a single non-incremental round-trip.
> `dir` is `ED_SEARCH_PREV_HISTORY` (from `vi_search_prev`, bound to
> `/`) or `ED_SEARCH_NEXT_HISTORY` (from `vi_search_next`, bound to
> `?`).
>
> `LEN` is 2 throughout, because `ANCHOR` is unconditionally defined in
> `el.h`. `tmpbuf` is a stack array of `EL_BUFSIZ` (1024) `wchar_t`.
>
> 1. Seed `tmpbuf[0] = '.'`, `tmpbuf[1] = '*'` and `tmplen = 2`.
> 2. `el->el_search.patdir = dir`. This is the only place `patdir` is
>    ever assigned outside `search_init`; `vi_repeat_search_next` and
>    `vi_repeat_search_prev` read it back.
> 3. Read the pattern with
>    `c_gets(el, &tmpbuf[2], dir == ED_SEARCH_PREV_HISTORY ? L"\n/" : L"\n?")`.
>    Note the prompt mapping: a *backward* history search is prompted
>    with `/` and a *forward* one with `?`, matching vi's convention
>    that `/` searches back through history. `c_gets` renders that
>    prompt by overwriting `el->el_line.buffer`, echoing keystrokes into
>    it, and clearing the line to empty before it returns — so the line
>    the user was editing is destroyed the moment `/` is pressed,
>    whatever happens afterwards.
> 4. If `c_gets` returned -1, return `CC_REFRESH`. It returns -1 both
>    for "backspaced past the start of an empty pattern" (a deliberate
>    cancel) and for end of file — in the latter case it has already
>    called `ed_end_of_file` and thrown away the `CC_EOF`, so `cv_search`
>    silently swallows EOF and reports an ordinary refresh. The port
>    should propagate the EOF.
> 5. `tmplen += 2`, then `ch = tmpbuf[tmplen]` — `c_gets` stores the
>    terminating keystroke (`ESC`, `CR` or `LF`) one past the text it
>    collected — and then overwrite that slot with `'\0'`.
> 6. **If `tmplen == 2`** the user entered nothing, so reuse the
>    previous pattern:
>    - If `el->el_search.patlen == 0` there is no previous pattern:
>      `re_refresh(el)` and return `CC_ERROR`.
>    - Otherwise, if `patbuf[0]` is neither `'.'` nor `'*'` — i.e. the
>      stored pattern was *not* produced by this function or by
>      `ce_inc_search`, so it carries no `".*"` prefix; in practice it
>      came from `c_setpat` via vi's `K`/`J` or emacs' `M-p`/`M-n` —
>      wrap it in `".*"` on both ends: copy `patbuf` into `tmpbuf` (at
>      most `EL_BUFSIZ - 1` characters), write `'.'` and `'*'` into
>      `patbuf[0]` and `patbuf[1]`, copy `tmpbuf` back to `&patbuf[2]`
>      (at most `EL_BUFSIZ - 3` characters), then increment `patlen`
>      **once**, append `'.'` and `'*'` at `patbuf[patlen++]` twice, and
>      terminate at `patbuf[patlen]`.
>      IMPORTANT: that single increment is an off-by-one bug. Shifting
>      the old text right by two costs two positions but `patlen` only
>      gains one, so the trailing `'.'` is written **over the last
>      character of the old pattern**. Reusing the pattern `abc` yields
>      `".*ab.*"`, not `".*abc.*"` — the final character is silently
>      dropped and the search is looser than the user asked for. The
>      port should add 2 and produce `".*" + old + ".*"`.
>      If `patbuf[0]` already is `'.'` or `'*'` the pattern is assumed
>      already wrapped and is reused untouched.
> 7. **Otherwise** (the user typed something): append `'.'` and `'*'`
>    at `tmpbuf[tmplen++]` twice, terminate at `tmpbuf[tmplen]`, copy
>    `tmpbuf` into `el->el_search.patbuf` (at most `EL_BUFSIZ - 1`
>    characters) and set `el->el_search.patlen = tmplen`. The stored
>    pattern is `".*" + <typed> + ".*"`, and it is a POSIX BRE — `.` and
>    `*` are the wildcards, `+`, `?`, `|`, `(` and `)` are literals, and
>    groups are `\(`…`\)`.
> 8. `el->el_state.lastcmd = (el_action_t) dir` so the `c_setpat` inside
>    the history search leaves the pattern alone.
> 9. `el->el_line.cursor = el->el_line.lastchar = el->el_line.buffer` —
>    the line is emptied, making the prefix comparison in the history
>    search vacuous so the pattern alone decides.
> 10. Call `ed_search_prev_history(el, 0)` or
>     `ed_search_next_history(el, 0)` according to `dir`. If it returns
>     `CC_ERROR`, `re_refresh(el)` and return `CC_ERROR`, leaving the
>     line empty — the text the user was editing before pressing `/` is
>     gone, and because step 9 ran before the search, the "current line"
>     that `ed_search_prev_history` stashes into `el->el_history.buf` at
>     `eventno == 0` is empty too. Reproduce it; it is what libedit
>     does, but it is worth a note in the port.
> 11. If the terminating keystroke `ch` was `0033` (`ESC`),
>     `re_refresh(el)` and return `ed_newline(el, 0)` — the matched
>     history entry is accepted and submitted immediately. Otherwise
>     (`CR` or `LF`) return `CC_REFRESH`, leaving the matched entry in
>     the buffer for further editing. Note this is the inverse of the
>     naive expectation, and that `c_gets` treats `ESC`, `CR` and `LF`
>     alike as terminators.
>
> Returns `CC_REFRESH`, `CC_ERROR`, or whatever `ed_newline` returns
> (`CC_NEWLINE`).

> [spec:libedit:def:search.el-match-fn]
> libedit_private int el_match(const wchar_t *str, const wchar_t *pat)

> [spec:libedit:sem:search.el-match-fn]
> The one and only matcher in libedit: does `str` match `pat`? Returns 1
> for match, 0 for no match. Used by `c_hmatch` for history entries, by
> `ce_search_line` for positions within the current line, and by
> `el_wparse` to decide whether an editrc line's `prog:` qualifier
> applies to `el->el_prog`.
>
> The C carries three implementations selected by `#if defined(REGEX)` /
> `#elif defined(REGEXP)` / `#else`. `src/sys.h` hardcodes
> `#define REGEX` and `#undef REGEXP`, so the **POSIX
> `regcomp`/`regexec` branch is the only one ever compiled**. Everything
> below specifies that branch. The BSD `regexp`/`regexec` branch and the
> V7 `re_comp`/`re_exec` branch are dead code, and per
> [dec:libedit:posix-only-scope] they are not ported and are not
> alternatives the port must offer.
>
> Steps:
>
> 1. **Literal-substring fast path, tried first.** If
>    `wcsstr(str, pat) != NULL`, return 1 immediately, without ever
>    consulting the regex engine. This is a plain wide-character
>    substring search over the whole of `str`, unanchored, case- and
>    locale-insensitive in the sense that it compares code units
>    directly.
> 2. Otherwise encode `pat` from `wchar_t` to a multibyte `char *` with
>    `ct_encode_string(pat, &conv)` and compile it with
>    `regcomp(&re, <encoded pat>, 0)`.
> 3. If `regcomp` returned non-zero (the pattern is not a valid regular
>    expression), the result is 0. No diagnostic is produced anywhere;
>    the error code is discarded.
> 4. If it returned 0, encode `str` the same way and evaluate
>    `regexec(&re, <encoded str>, 0, NULL, 0) == 0`; that boolean is the
>    result. `nmatch` is 0 and `pmatch` is `NULL`, so no capture
>    positions are requested — only the yes/no answer is used. Then
>    `regfree(&re)`: the compiled pattern is thrown away after a single
>    use, so every call recompiles.
> 5. Return the result.
>
> **The fast path changes the semantics materially, and the port must
> keep it.** Because `wcsstr` runs first:
>
> - A pattern that is not a valid regular expression still matches when
>   it occurs literally. `foo[bar` cannot compile, but a candidate
>   containing the exact text `foo[bar` matches anyway.
> - A pattern whose metacharacters were meant literally matches its
>   literal occurrence even when the regex reading would not — and the
>   two readings can disagree in both directions, since whichever
>   succeeds first wins and the fast path is always first.
> - An **empty pattern always matches**: `wcsstr(str, L"")` returns
>   `str`. `c_setpat` produces an empty pattern whenever the cursor is
>   at the start of the line, so this case is live.
> - The comparison in step 1 is over `wchar_t` values, whereas steps 2-4
>   compare the multibyte encodings. They can disagree for characters
>   the current locale cannot encode (see below).
>
> **Regex dialect — the biggest trap for a Rust port.** `cflags` is `0`,
> which means POSIX **basic** regular expressions, case-sensitive, with
> `REG_NEWLINE` off so `.` matches `\n` and `^`/`$` anchor only at the
> ends of the whole subject. In a BRE:
>
> - `.`, `*`, `[...]`, `^` (only at the start of the pattern or of a
>   subexpression) and `$` (only at the end) are the operators;
> - `+`, `?`, `|`, `(`, `)`, `{` and `}` are **ordinary literal
>   characters**, so `a+b` matches the three-character text `a+b`;
> - grouping is `\(`…`\)`, alternation is not available at all in strict
>   POSIX BRE (glibc offers `\|` as an extension), back-references are
>   `\1`…`\9`, and bounded repetition is `\{m,n\}`;
> - matching is leftmost-longest, not leftmost-first.
>
> Rust's `regex` crate defaults to a Perl-flavoured extended syntax and
> will read every one of those the other way. The port must either use a
> BRE-capable engine or translate the pattern, and must treat an
> uncompilable pattern as "no match" rather than as an error — the C
> reports nothing.
>
> **Wide-to-multibyte conversion and locale dependence.** Both operands
> go through `ct_encode_string`, which encodes each `wchar_t` with
> `wctomb` in the process's current locale. Consequences:
>
> - Any character the locale cannot encode is **silently dropped** —
>   `ct_encode_char` catches `wctomb`'s failure, resets the shift state
>   and reports a length of zero — so it disappears from the pattern and
>   from the subject before matching. In a `C`/`POSIX` locale that is
>   every non-ASCII character, which reduces a pattern of accented text
>   to the empty string, which then matches everything.
> - What `.` and a bracket expression mean is likewise the locale's
>   business: in a UTF-8 locale glibc's `regexec` treats a multibyte
>   sequence as one character, in the `C` locale each byte is its own
>   character. So the same pattern and the same input can match
>   differently depending only on `LC_CTYPE`.
> - `ct_encode_string` aborts the process (`abort()`) if a single
>   character needs more than 5 bytes, and returns `NULL` on allocation
>   failure — in which case `regcomp` is handed a `NULL` pointer, which
>   is undefined behaviour. Neither is reachable with any real locale
>   and a working allocator, but a port must not reproduce them: return
>   "no match" instead.
> - Anchoring and matching therefore all happen on **bytes-in-a-locale**,
>   not on `wchar_t`. A Rust port working natively in `str`/`char` will
>   diverge for un-encodable input; that divergence is an improvement
>   and should be recorded as such rather than emulated.
>
> **The static conversion buffer.** `conv` is a function-level
> `static ct_buffer_t`, so it is shared by every caller and every
> `EditLine` instance in the process, grows to the largest string ever
> encoded, and is never freed — one deliberate permanent allocation, not
> a leak per call. It also means `el_match` is not thread-safe and not
> reentrant. Note both encodings in the REGEX branch reuse the same
> buffer: the pattern's encoding is overwritten by the subject's, which
> is safe only because `regcomp` has already consumed the pattern by
> then. A port should use a local buffer.

Maintained-port note: [dec:libedit:history-regex-dialect] deliberately keeps
the literal-first contract while selecting `regex::Regex` over Unicode scalar
text for the fallback. The rule above remains the source of truth for the
original C implementation and its POSIX BRE behaviour; the decision and
`[spec:nshedit:req:abi.history-effects+2]` define the approved Rust divergence.

> [spec:libedit:def:search.el-search-t]
> typedef struct el_search_t

> [spec:libedit:def:search.regerror-fn]
> void /*ARGSUSED*/ regerror(const char *msg)

> [spec:libedit:sem:search.regerror-fn]
> An empty function: it takes a message string and does nothing with it,
> returning immediately. It exists only to satisfy the BSD `regexp`
> library, which calls a caller-supplied `regerror` to report a bad
> pattern; libedit deliberately swallows the diagnostic so a malformed
> search pattern is silently treated as "no match".
>
> It sits inside `#ifdef REGEXP`. `src/sys.h` does `#undef REGEXP` and
> `#define REGEX`, so this function is never compiled, and per
> [dec:libedit:posix-only-scope] the BSD `regexp` branch is out of
> scope. **Nothing is ported for this rule.** The POSIX branch that
> replaces it has no error callback at all — `regcomp`'s non-zero return
> is simply discarded by `el_match`, giving the same observable
> behaviour of a silently ignored bad pattern.
>
> Note also that the name collides with POSIX's own `regerror(int,
> const regex_t *, char *, size_t)`, which has a different signature;
> that collision is another reason the definition only exists under the
> non-POSIX branch.

> [spec:libedit:def:search.search-end-fn]
> libedit_private void search_end(EditLine *el)

> [spec:libedit:sem:search.search-end-fn]
> Releases the search subsystem's one heap allocation.
>
> 1. `el_free(el->el_search.patbuf)`.
> 2. `el->el_search.patbuf = NULL`.
>
> Nothing else in `el->el_search` is reset — `patlen`, `patdir`,
> `chacha`, `chadir` and `chatflg` keep whatever they held, so after
> this call `patlen` may be non-zero while `patbuf` is `NULL`. Nothing
> reads them again: `search_end` is called only from `el_end`, on the
> way to freeing the whole `EditLine`. The function is idempotent in the
> C only because `el_free`/`free` accepts `NULL` and the pointer is
> nulled; a port should make that explicit. Returns nothing and cannot
> fail.
>
> In Rust this rule has no code of its own — the allocation is owned by
> the search state and dropped with it. Keep the rule as the record that
> the buffer's lifetime ends with the editor.

> [spec:libedit:def:search.search-init-fn]
> libedit_private int search_init(EditLine *el)

> [spec:libedit:sem:search.search-init-fn]
> Allocates the pattern buffer and puts `el->el_search` into its
> starting state. Called once from `el_init_internal`.
>
> 1. `el->el_search.patbuf = el_calloc(EL_BUFSIZ, sizeof(wchar_t))` —
>    a zeroed 1024-element wide-character buffer. `EL_BUFSIZ` is fixed
>    at 1024 (`el.h`) and the buffer is never resized, so every pattern
>    length check elsewhere in the file is against this constant.
> 2. If the allocation failed, return -1 immediately, leaving the rest
>    of the fields untouched (they are whatever `el_calloc` of the
>    `EditLine` left them, i.e. zero).
> 3. `patbuf[0] = L'\0'` and `el->el_search.patlen = 0` — no pattern
>    yet. `patlen == 0` is the sentinel several callers test:
>    `vi_repeat_search_next`/`prev` refuse to run without a pattern,
>    `cv_search` refuses to reuse an empty one, and `ce_inc_search`
>    treats it as "first round, seed the `.*` prefix".
> 4. `el->el_search.patdir = -1` — the last history-search direction, as
>    a command code. -1 is deliberately not a valid command code; it
>    means "no direction established yet". Only `cv_search` ever writes
>    a real value here. Note that if `patdir` were read while still -1,
>    `cv_repeat_srch` would fall through its `switch` to `CC_ERROR`,
>    but the `patlen == 0` guard in its callers gets there first.
> 5. `el->el_search.chacha = L'\0'` — no remembered `f`/`t` target
>    character. `cv_csearch` treats a `ch` of 0 as an immediate
>    `CC_ERROR`, which is how `;` and `,` fail before any `f`/`F`/`t`/`T`
>    has run.
> 6. `el->el_search.chadir = CHAR_FWD` (`+1`) — remembered character
>    search direction.
> 7. `el->el_search.chatflg = 0` — remembered `t`-versus-`f` flag; 0
>    means `f` (land on the character).
> 8. Return 0.
>
> The success path leaves no other observable effect; nothing is drawn
> and no other subsystem is touched.
