# src/refresh.c, src/refresh.h

> [spec:libedit:def:refresh.el-refresh-t]
> typedef struct

> [spec:libedit:def:refresh.re-addc-fn]
> static void re_addc(EditLine *el, wint_t c)

> [spec:libedit:sem:refresh.re-addc-fn]
> Draws one character `c` into the virtual screen image `el->el_vdisplay`
> at the drawing cursor `el->el_refresh.r_cursor`, expanding it to its
> visible form. Nothing reaches the terminal; this only builds the image
> that `re_refresh` will later diff against `el->el_display`.
> Classify `c` with `ct_chr_class` and take one of four branches.
> `CHTYPE_TAB` (`\t`): loop, emitting one space with `re_putc(el, ' ',
> 1)` each time, and stop as soon as `r_cursor.h & 7` is zero. The test
> is after the write, so at least one space is always emitted, and the
> run stops on the next 8-column tab stop. If a space wraps the row,
> `re_putc` resets the column to 0, which satisfies `h & 7 == 0` and ends
> the run immediately — a tab that crosses the right margin therefore
> fills only up to the margin and does not continue on the new row.
> `CHTYPE_NL` (`\n`): remember `r_cursor.v`, call `re_putc(el, '\0', 0)`
> to terminate the virtual row at the current column without advancing
> the cursor, then, if `r_cursor.v` is unchanged, call `re_nextline`. The
> `re_putc` call cannot change `r_cursor.v`, because the no-shift form
> returns before touching the cursor at all, so the guard is always true
> and `re_nextline` always runs; the C flags the test `XXX`. A port may
> implement it as an unconditional `re_nextline`, but must keep the NUL
> write, which lands at the current column, not at column 0.
> `CHTYPE_PRINT`: a single `re_putc(el, c, 1)`.
> Anything else (`CHTYPE_ASCIICTL`, `CHTYPE_NONPRINT`): render the
> character into a `VISUAL_WIDTH_MAX` (8) element scratch buffer with
> `ct_visual_char` — `^X` form for ASCII controls, `\U+nnnn` or
> `\U+nnnnn` for non-printables — then emit each of the `n` returned
> characters in turn with `re_putc(..., 1)`. If `ct_visual_char` returns
> -1 ("would not fit") the loop body never executes and the character is
> silently dropped.
> Because each visual character is written by its own `re_putc`, a long
> `\U+nnnn` expansion that reaches the right margin is split across the
> row break at whatever column the margin falls; it is not kept whole.

> [spec:libedit:def:refresh.re-clear-display-fn]
> libedit_private void re_clear_display(EditLine *el)

> [spec:libedit:sem:refresh.re-clear-display-fn]
> Resets the model of what is physically on the screen so that a fresh
> prompt can be drawn from the top-left of libedit's region. Emits
> nothing.
> Step 1: set `el->el_cursor.h` and `el->el_cursor.v` to 0. This asserts
> that the real terminal cursor is at the home position of the region;
> the caller is responsible for having actually put it there (see
> `re_goto_bottom`, which writes the newline first).
> Step 2: for every row `i` from 0 to `el_terminal.t_size.v - 1`, store
> `'\0'` in `el_display[i][0]`. Only the first cell of each row is
> touched, so the rest keeps stale content; every row now reads as the
> empty string, which is all that `re_update_line` and `wcslen` care
> about.
> Step 3: set `el->el_refresh.r_oldcv` to 0, so the next `re_refresh`
> believes the previously drawn line occupied exactly one row and does
> not try to erase rows below it.
> `el_vdisplay` is not touched.

> [spec:libedit:def:refresh.re-clear-eol-fn]
> static void re_clear_eol(EditLine *el, int fx, int sx, int diff)

> [spec:libedit:sem:refresh.re-clear-eol-fn]
> Decides how many columns of leftover text can still be visible at the
> end of a line just rewritten, and calls `terminal_clear_EOL` with that
> count.
> Inputs: `fx` and `sx` are the signed character counts inserted
> (positive) or deleted (negative) by the first and second difference of
> `re_update_line`; `diff` is the old row's length minus the new row's
> length.
> Step 1: replace `fx` by its absolute value; likewise `sx`.
> Step 2: if `fx` is greater than `diff`, set `diff` to `fx`.
> Step 3: if `sx` is greater than `diff`, set `diff` to `sx`.
> Step 4: call `terminal_clear_EOL(el, diff)`.
> The value handed on is therefore `max(diff, |fx|, |sx|)` — an upper
> bound on how much stale text can remain to the right of what was just
> written. The count only matters on terminals without a
> clear-to-end-of-line capability (terminfo `clr_eol`), where
> `terminal_clear_EOL` writes that many spaces and charges them to the
> recorded column.
> The incoming `diff` may be negative (new row longer than old) and the
> maxima are not clamped at zero. Both call sites are reached only when
> the corresponding `fx` or `sx` is strictly negative, so `|fx|` or
> `|sx|` is at least 1 and the result is always positive in practice. A
> port must not rely on that if it reuses the helper: `terminal_clear_EOL`
> with a negative count writes nothing but still moves its recorded
> column backwards.

> [spec:libedit:def:refresh.re-clear-lines-fn]
> libedit_private void re_clear_lines(EditLine *el)

> [spec:libedit:sem:refresh.re-clear-lines-fn]
> Physically blanks every screen row the previous line occupied, rows 0
> through `el->el_refresh.r_oldcv`. It is used only for the
> `CC_REDISPLAY` result, always immediately followed by
> `re_clear_display` and then a full `re_refresh`. It emits output and
> nothing else — it does not reset any buffer.
> If the TERM_CAN_CEOL flag is set in `el_terminal.t_flags`: iterate `i`
> from `r_oldcv` downwards to 0 inclusive and for each value, in order:
> if `i > 0`, write a bare `'\r'` and then a bare `'\n'` with
> `terminal__putc`; then call `terminal_move_to_line(el, i)`,
> `terminal_move_to_char(el, 0)`, and `terminal_clear_EOL(el,
> el_terminal.t_size.h)`.
> If TERM_CAN_CEOL is clear: write `'\r'` then `'\n'` once for each `i`
> from `r_oldcv` down to 1 inclusive — that is, `r_oldcv` newline pairs,
> scrolling the old text up out of the way — then
> `terminal_move_to_line(el, r_oldcv)`, then one final bare `'\r'` and
> `'\n'`. Nothing is erased on this path.
> The bare `'\r'`/`'\n'` writes bypass `el->el_cursor` entirely, so after
> each of them libedit's recorded cursor no longer describes the
> terminal, and the `terminal_move_to_line` that follows computes its
> motion from a stale value. The sequence is only coherent because the
> caller runs `re_clear_display` immediately afterwards, which forces the
> recorded cursor to (0, 0), and then redraws everything. Reproduce the C
> here rather than "repairing" the bookkeeping: the byte sequence a
> terminal receives is the observable behaviour.

> [spec:libedit:def:refresh.re-copy-and-pad-fn]
> static void re__copy_and_pad(wchar_t *dst, const wchar_t *src, size_t width)

> [spec:libedit:sem:refresh.re-copy-and-pad-fn]
> Copies `src` into `dst` and blank-pads it to exactly `width` cells.
> Step 1: for `i` counting from 0 while `i < width`: if the current
> source cell is `'\0'`, stop; otherwise copy it and advance both
> pointers.
> Step 2: for the remaining values of `i` up to `width`, write a space.
> Step 3: write `'\0'` at the next cell, which is `dst[width]`.
> Exactly `width + 1` cells of `dst` are written, which is exactly how
> large a display row is allocated (`t_size.h + 1`), so a full-width row
> still receives its terminator. Source cells beyond `width` are ignored;
> a source shorter than `width` is space-filled.
> `re_refresh` uses this to make `el_display[i]` a faithful, blank-padded
> image of `el_vdisplay[i]`, and `re_fastputc` uses it with an empty
> source to blank a freshly exposed row. The padding is load-bearing:
> `terminal_move_to_char` moves rightwards by replaying the display row's
> own characters, so every cell it might replay has to hold a real space
> rather than a NUL or a leftover from a previous line.
> MB_FILL_CHAR cells are copied like any other; being `(wint_t)-1` they
> are non-zero and never terminate the copy.

> [spec:libedit:def:refresh.re-delete-fn]
> static void /*ARGSUSED*/ re_delete(EditLine *el __attribute__((__unused__)), wchar_t *d, int dat, int dlen, int num)

> [spec:libedit:sem:refresh.re-delete-fn]
> Deletes `num` cells from the wide-character array `d` starting at index
> `dat`, shifting the tail left. `dlen` is the usable length of `d`
> (callers pass `el_terminal.t_size.h`); index `dlen` itself is writable
> because display rows are allocated with `dlen + 1` cells. The `el`
> parameter is unused outside debug builds. Nothing is written to the
> terminal — this only keeps libedit's model of the screen in step with a
> `terminal_deletechars` that has already been emitted.
> Step 1: if `num <= 0`, return.
> Step 2: if `dat + num >= dlen` the deletion reaches or passes the end
> of the row: store `'\0'` at `d[dat]` and return. Everything from `dat`
> onwards is simply truncated; no shifting is done.
> Step 3: otherwise copy left. With a write pointer at `d + dat` and a
> read pointer at `d + dat + num`, copy one cell at a time while the read
> pointer is strictly below `d + dlen`, advancing both. This moves
> `d[dat+num .. dlen-1]` down to `d[dat .. dlen-1-num]`.
> Step 4: store `'\0'` at `d[dlen]`.
> The last `num` cells before `d[dlen]` keep stale copies of what was
> there before. In practice the string's own terminator is shifted down
> with the rest of the data, so the row still reads correctly; the
> exception is a row that was full to `dlen`, which is left unterminated
> until `re_refresh` rebuilds it with `re__copy_and_pad`.

> [spec:libedit:def:refresh.re-fastaddc-fn]
> libedit_private void re_fastaddc(EditLine *el)

> [spec:libedit:sem:refresh.re-fastaddc-fn]
> Fast path for "exactly one character was just appended at the end of
> the line": updates the screen incrementally instead of rebuilding and
> diffing the whole image. It assumes the recorded cursor `el->el_cursor`
> already matches the real terminal cursor, which holds because the
> previous refresh left it so.
> Three bail-outs, each of which calls the full `re_refresh` and returns:
> 1. `el_line.cursor == el_line.buffer` — there is no character before
> the cursor to have been added.
> 2. The character just inserted, `el_line.cursor[-1]`, is a tab, or the
> cursor is not at `el_line.lastchar`. A tab's width depends on the
> column, and an insertion in the middle of the line shifts everything
> after it; both are "too hard to handle" here.
> 3. A right-hand prompt is in use — `el_rprompt.p_pos.h` is non-zero —
> and `el_terminal.t_size.h - el_cursor.h - el_rprompt.p_pos.h` is less
> than 3, i.e. less than one clear column would be left between the input
> and the right prompt. A full refresh is needed to drop the rprompt.
> Note what `el_rprompt.p_pos.h` actually holds at this point: when
> `re_refresh` drew a right prompt it left `p_pos.h` equal to the column
> *after* the rprompt, `t_size.h - 1`, not the rprompt's width. The
> expression therefore evaluates to `1 - el_cursor.h`, which is always
> less than 3, so whenever a right prompt is displayed this bail-out
> always fires and the fast path is never taken. Reproduce this; it is
> load-bearing for correctness even though it makes the test vacuous.
> Otherwise classify `el_line.cursor[-1]` with `ct_chr_class` and emit:
> - `CHTYPE_TAB`: do nothing — already excluded by bail-out 2.
> - `CHTYPE_NL` and `CHTYPE_PRINT`: one `re_fastputc` of the character.
> - `CHTYPE_ASCIICTL` and `CHTYPE_NONPRINT`: expand with `ct_visual_char`
> into a `VISUAL_WIDTH_MAX` (8) element buffer and `re_fastputc` each of
> the returned characters in turn; a -1 return emits nothing.
> Finally call `terminal__flush(el)`.
> The newline case is a genuine inconsistency with the slow path:
> `re_fastputc` writes the `'\n'` straight out and stores `'\n'` in the
> display cell, advancing the recorded column by one, whereas `re_addc`
> would have terminated the virtual row and moved to the next one. With
> the tty's ONLCR translation — which libedit relies on elsewhere — the
> terminal performs a CR/LF, so the recorded cursor and the screen
> diverge. It is only reachable by pushing a literal newline into the
> buffer, never by pressing Return.

> [spec:libedit:def:refresh.re-fastputc-fn]
> static void re_fastputc(EditLine *el, wint_t c)

> [spec:libedit:sem:refresh.re-fastputc-fn]
> Writes one character straight to the terminal and updates the real
> screen image `el_display` and the real cursor `el->el_cursor` to match,
> including right-margin wrap. No diffing is involved and `el_vdisplay`
> is not touched.
> Step 1: let `w = wcwidth(c)`.
> Step 2: while `w > 1` and `el_cursor.h + w > el_terminal.t_size.h`,
> recursively call `re_fastputc(el, ' ')`. This pads out the tail of the
> row so that a double-width character is not split across the margin;
> each space advances the column and, on reaching the margin, performs
> the wrap of step 5, after which the test fails and the loop ends. Only
> `w > 1` triggers padding, so zero- and single-width characters never
> pad.
> Step 3: emit the character with `terminal__putc`, which writes nothing
> at all for MB_FILL_CHAR and expands an EL_LITERAL magic character to
> its saved byte string.
> Step 4: store `c` in `el_display[el_cursor.v][el_cursor.h]` and
> post-increment the column. Then, while `--w > 0`, store MB_FILL_CHAR in
> each following cell, incrementing the column for each. A character of
> width `w` therefore advances the column by `max(w, 1)`: widths 0 and -1
> — combining marks and anything `wcwidth` rejects — still consume a
> whole cell and a whole column, the same convention as `re_putc`.
> Step 5: if `el_cursor.h` is now at or past `el_terminal.t_size.h`,
> wrap. Set the column to 0 and pick the row to blank:
> (a) if `el_cursor.v + 1 >= el_terminal.t_size.v` we are already on the
> last row, so emulate a scroll by rotating the `el_display` row pointers
> up by one — save row 0's pointer, move each `el_display[i]` down to
> index `i - 1` for `i` from 1 to `t_size.v - 1`, and install the saved
> pointer as the new last row, which is the row to blank. `el_cursor.v`
> is deliberately left where it is and `r_oldcv` is not touched.
> (b) otherwise increment `el_cursor.v`, pre-increment
> `el->el_refresh.r_oldcv`, and take `el_display[r_oldcv]` as the row to
> blank. Note this indexes by `r_oldcv`, not by `el_cursor.v`; the two
> coincide only when the counters agree.
> Then blank that row with `re__copy_and_pad(row, L"",
> el_terminal.t_size.h)`, filling it with `t_size.h` spaces and a NUL.
> Then force the physical wrap. If the TERM_HAS_AUTO_MARGINS flag is set
> the terminal wraps by itself; additionally, if TERM_HAS_MAGIC_MARGINS
> is also set (the terminfo `eat_newline_glitch` behaviour, where the
> wrap is deferred until the next character arrives), write a space and
> then a backspace to make the pending wrap resolve now. If
> TERM_HAS_AUTO_MARGINS is clear, write `'\r'` then `'\n'` instead.
> Only case (a) rotates row pointers; `el_vdisplay` is never scrolled to
> match, so after a scroll the two images no longer describe the same
> rows.

> [spec:libedit:def:refresh.re-goto-bottom-fn]
> libedit_private void re_goto_bottom(EditLine *el)

> [spec:libedit:sem:refresh.re-goto-bottom-fn]
> Moves the terminal to a fresh line below everything libedit has drawn
> and resets the screen model so whatever is printed next starts clean.
> Used when the editor is finished with a line — accepting it,
> end-of-file, and the vi equivalents.
> Step 1: `terminal_move_to_line(el, el->el_refresh.r_oldcv)` — go to the
> last row the previous refresh used.
> Step 2: write a single `'\n'` with `terminal__putc`. No `'\r'` is
> emitted: libedit relies on the tty's ONLCR translation to turn this
> into a CR/LF, the same assumption `terminal_move_to_line` documents
> when it moves downwards.
> Step 3: `re_clear_display(el)` — recorded cursor back to (0, 0), every
> `el_display` row marked empty, `r_oldcv` back to 0.
> Step 4: `terminal__flush(el)`.
> The recorded cursor becomes (0, 0) rather than a row further down
> because from here on libedit treats the current physical line as the
> new origin of the region it owns.

> [spec:libedit:def:refresh.re-insert-fn]
> static void /*ARGSUSED*/ re_insert(EditLine *el __attribute__((__unused__)), wchar_t *d, int dat, int dlen, wchar_t *s, int num)

> [spec:libedit:sem:refresh.re-insert-fn]
> Inserts `num` cells copied from `s` into the wide-character array `d`
> at index `dat`, shifting what was there to the right. `dlen` is the
> usable length of `d` (callers pass `el_terminal.t_size.h`); index
> `dlen` itself is writable because display rows are allocated with
> `dlen + 1` cells. The `el` parameter is unused outside debug builds.
> Nothing is written to the terminal — this only keeps libedit's model of
> the screen in step with a `terminal_insertwrite` already emitted.
> Step 1: if `num <= 0`, return.
> Step 2: clamp. If `num > dlen - dat`, set `num = dlen - dat`. If `dat`
> exceeds `dlen` this makes `num` negative; every remaining step is
> guarded on `num > 0`, so nothing happens in that case.
> Step 3: if `num > 0`, open the gap by copying right-to-left. Set a
> write pointer to `d + dlen - 1` and a read pointer `num` cells below
> it, and copy while the read pointer is at or above `d + dat`,
> decrementing both. This moves `d[dat .. dlen-1-num]` up to `d[dat+num
> .. dlen-1]`; the `num` cells that fall off the end are discarded. Then
> store `'\0'` at `d[dlen]`.
> Note the loop's exit test forms and compares a pointer one before the
> start of the array when `dat` is 0, which is undefined behaviour in C.
> It happens to work everywhere libedit is built. A Rust port must
> express the loop with indices (or iterate the range `dat..dlen-num` in
> reverse) rather than reproducing the pointer arithmetic.
> Step 4: copy the new content in. From `d + dat` forwards, while the
> write pointer is strictly below `d + dlen` and `num` is still positive,
> copy one cell from `s`, advance both pointers and decrement `num`.
> Unlike `re__strncopy` this does not stop at a NUL in `s`, so exactly
> `num` cells are written subject to the `d + dlen` bound.

> [spec:libedit:def:refresh.re-nextline-fn]
> static void re_nextline(EditLine *el)

> [spec:libedit:sem:refresh.re-nextline-fn]
> Advances the virtual drawing cursor `el->el_refresh.r_cursor` to the
> start of the next row of `el_vdisplay`, scrolling the virtual image if
> there is no next row. Emits nothing.
> Step 1: set `r_cursor.h` to 0.
> Step 2: if `r_cursor.v + 1 >= el_terminal.t_size.v` — already on the
> last row — emulate a scroll instead of advancing: save the row-0
> pointer, move each `el_vdisplay[i]` down to index `i - 1` for `i` from
> 1 to `t_size.v - 1`, install the saved pointer as the new last row, and
> store `'\0'` in its first cell so it reads as empty. `r_cursor.v` is
> deliberately left at `t_size.v - 1`. Only the first cell of the
> recycled row is cleared; the rest keeps stale content, which is
> harmless because drawing into it is sequential and always
> re-terminates.
> Step 3: otherwise increment `r_cursor.v`.
> The scroll rotates only `el_vdisplay`. `el_display`, the image of what
> is physically on screen, is not rotated to match, so once the input is
> long enough to scroll, `re_update_line` diffs row `i` of the new
> virtual image against a stale row `i` of the old real image and the two
> disagree about what is on screen. This is a known limitation of the C
> for input longer than the terminal; do not silently repair it in the
> port, since doing so changes what is emitted.
> Under DEBUG_REFRESH the C additionally asserts `r_cursor.v <
> t_size.v` and calls `abort()` if not; the check is compiled out of
> normal builds.

> [spec:libedit:def:refresh.re-printstr-fn]
> static void re_printstr(EditLine *el, const char *str, wchar_t *f, wchar_t *t)

> [spec:libedit:sem:refresh.re-printstr-fn]
> Debug-only tracing helper. It exists only when the C is compiled with
> DEBUG_REFRESH; in a normal build the function and the ELRE_DEBUG /
> ELRE_ASSERT macros it depends on all expand to nothing.
> Writes to `el->el_errfile`: the label `str`, then `:`, then a double
> quote; then, for every wide character in the half-open range `[f, t)`,
> that character masked with 0177 written as one byte; then a closing
> double quote and a CR/LF.
> The 0177 mask folds every wide character into the ASCII range, so the
> dump is legible for ASCII content and meaningless for anything else;
> MB_FILL_CHAR, being `(wint_t)-1`, prints as `\177`.
> `re_update_line` calls it twelve times to dump the twelve regions its
> pointers delimit. A port may implement this as tracing behind a feature
> flag, or omit it: it has no effect on any shipped build and no
> observable behaviour across the C ABI.

> [spec:libedit:def:refresh.re-putc-fn]
> libedit_private void re_putc(EditLine *el, wint_t c, int shift)

> [spec:libedit:sem:refresh.re-putc-fn]
> Places one character into the virtual screen image `el_vdisplay` at the
> virtual drawing cursor `el->el_refresh.r_cursor`, optionally advancing
> that cursor. Nothing reaches the terminal. `shift` selects between
> "write and advance" (non-zero) and "write only" (zero).
> Step 1: let `w = wcwidth(c)`, and if it is -1 (non-printable) set `w`
> to 0. Let `sizeh = el_terminal.t_size.h`.
> Step 2: if `shift` is non-zero, then while `r_cursor.h + w > sizeh`,
> recursively call `re_putc(el, ' ', 1)`. This pads the tail of the row
> with spaces so a double-width character is not split across the right
> margin; each space advances the column and, at the margin, triggers
> step 6, which resets the column to 0 and ends the loop. For `w <= 1`
> the condition is false on entry, since `r_cursor.h <= sizeh` always
> holds, so only wide characters ever pad. The loop would not terminate
> if `w` could exceed `sizeh`; `terminal_change_size` forces at least 2
> columns and `wcwidth` never returns more than 2, so it cannot happen in
> practice.
> Step 3: store `c` in `el_vdisplay[r_cursor.v][r_cursor.h]`, then store
> MB_FILL_CHAR in the following `w - 1` cells — none when `w` is 0 or 1.
> Step 4: if `shift` is zero, return here. The cursor has not moved, so
> `re_putc(el, '\0', 0)` is exactly "terminate the virtual row at the
> current column"; that is how `re_refresh` and `re_addc` end a row.
> Step 5: advance `r_cursor.h` by `w` if `w` is non-zero, otherwise by 1.
> Zero-width characters — combining marks, and everything `wcwidth`
> rejects — therefore occupy a full column in libedit's virtual model
> even though they occupy none on the screen. `re_refresh_cursor`, which
> recomputes the same position independently, uses `ct_visual_width`,
> which for a printable character returns `wcwidth` and therefore 0 for a
> combining mark. The two disagree by one column for every zero-width
> printable character on the line, and the visible symptom is the cursor
> landing too far left. This is a live defect in the C; a port that fixes
> it must fix both sides together, or leave both as they are.
> Step 6: if `r_cursor.h` is now at or past `sizeh`, store `'\0'` at
> `el_vdisplay[r_cursor.v][sizeh]` — the one extra cell every row is
> allocated with — and call `re_nextline`.

> [spec:libedit:def:refresh.re-putliteral-fn]
> libedit_private void re_putliteral(EditLine *el, const wchar_t *begin, const wchar_t *end)

> [spec:libedit:sem:refresh.re-putliteral-fn]
> Places a "literal" prompt sequence — an escape sequence bracketed by
> the prompt's `p_ignore` delimiter — into the virtual screen image as a
> single magic cell, so that it takes up only its visible width in the
> image but replays its full byte string when printed. `begin` points at
> the first character inside the delimiters and `end` at the closing
> delimiter, so `end[1]` is the visible character the sequence decorates.
> Let `cur` be `el->el_refresh.r_cursor` and `sizeh` be
> `el_terminal.t_size.h`.
> Step 1: call `literal_add(el, begin, end, &w)`. It interns the whole
> bracketed sequence plus the encoded `end[1]` as a byte string, returns
> a magic `wint_t` of the form `EL_LITERAL | index` naming it, and sets
> `w` to `wcwidth(end[1])`, the number of columns the sequence actually
> occupies. It returns 0 on allocation failure or if `end[1]` is
> non-printable.
> Step 2: if the returned character is 0 or `w` is negative, return
> without changing anything — the sequence is dropped.
> Step 3: store the magic character in `el_vdisplay[cur->v][cur->h]`.
> Step 4: fill the following cells with MB_FILL_CHAR. Take `i = w`, clamp
> it down to `sizeh - cur->h`, then write MB_FILL_CHAR at offsets `i - 1`
> down to 1 from the cursor. The clamp is what keeps the fill inside the
> row.
> Step 5: advance `cur->h` by `w` if `w` is non-zero, otherwise by 1 —
> the same "zero width still costs a column" rule as `re_putc`.
> Step 6: if `cur->h` is now at or past `sizeh`, store `'\0'` at
> `el_vdisplay[cur->v][sizeh]` and call `re_nextline`.
> Unlike `re_putc` there is no pre-padding loop, so a wide literal that
> straddles the right margin is not pushed to the next row: its magic
> cell is written where it falls, its fill cells are truncated at the
> margin, and `cur->h` may overshoot `sizeh` before `re_nextline` resets
> it to 0. The overshoot is absorbed silently.

> [spec:libedit:def:refresh.re-refresh-cursor-fn]
> libedit_private void re_refresh_cursor(EditLine *el)

> [spec:libedit:sem:refresh.re-refresh-cursor-fn]
> Recomputes from `el_line` alone where the cursor belongs on screen, and
> moves the terminal there. Used for the `CC_CURSOR` result — commands
> that move the cursor but change nothing on the line — so nothing is
> redrawn and neither display image is consulted or updated.
> Step 1: clamp the logical cursor exactly as `re_refresh` does. If
> `el_line.cursor >= el_line.lastchar`, then: if the alt keymap is
> current (`el_map.current == el_map.alt`, i.e. vi command mode) and the
> line is non-empty (`lastchar != buffer`), set the cursor to `lastchar -
> 1`; otherwise set it to `lastchar`.
> Step 2: start from just after the prompt: `h = el_prompt.p_pos.h`, `v =
> el_prompt.p_pos.v`. Let `th = el_terminal.t_size.h`. Those prompt
> coordinates were recorded by the last `re_refresh`, so this routine is
> only correct if a refresh has already run.
> Step 3: walk every character `*cp` from `el_line.buffer` up to but not
> including `el_line.cursor`, classifying each with `ct_chr_class`:
> - `CHTYPE_NL`: set `h = 0` and increment `v`.
> - `CHTYPE_TAB`: pre-increment `h` until `h & 7` is zero, i.e. advance
> to the next 8-column tab stop.
> - anything else: let `w = wcwidth(*cp)`; if `w > 1` and `h + w > th`
> the character will not fit in the remaining columns, so set `h = 0` and
> increment `v`; then add `ct_visual_width(*cp)` to `h`.
> After each character, whichever branch ran, if `h >= th` then subtract
> `th` from `h` and increment `v`. This is the ordinary wrap. It
> subtracts `th` exactly once, so a single expansion that spanned more
> than one full row would be mis-accounted; the C's comment notes this is
> also where over-long tabs are meant to be caught.
> Step 4: if the cursor is not at `lastchar` and the character under it
> is double-width (`wcwidth > 1`) and would not fit in the remaining
> columns (`h + w > th`), set `h = 0` and increment `v` — the character
> will have been pushed to the next row, so the cursor must follow it.
> Step 5: `terminal_move_to_line(el, v)`, then `terminal_move_to_char(el,
> h)`, then `terminal__flush(el)`.
> Two accounting mismatches against the drawing path are worth calling
> out, because both silently put the cursor in the wrong place:
> (a) Tabs. The drawing path (`re_addc`) emits spaces one at a time and
> stops the moment a wrap resets the column to 0, so a tab crossing the
> margin ends at column 0 of the next row. Here the tab is advanced to
> the next multiple of 8 first and the wrap subtraction applied
> afterwards, ending at `(h_next_tabstop - th)`. The two agree only when
> `th` is a multiple of 8; on any other terminal width, a tab near the
> right margin leaves the two computations disagreeing.
> (b) Zero-width printable characters. `re_putc` charges a combining mark
> one column; `ct_visual_width` returns `wcwidth`, i.e. zero, so this
> routine charges it none. See the `re_putc` rule.
> The local `w` is only ever read after being assigned in the same
> expression, so the C's apparently uninitialised use in step 4 is not
> actually a defect.

> [spec:libedit:def:refresh.re-refresh-fn]
> libedit_private void re_refresh(EditLine *el)

> [spec:libedit:sem:refresh.re-refresh-fn]
> Rebuilds the virtual screen image from the current input line, then
> drives the real screen towards it row by row. This is the general
> redraw path; `re_fastaddc` is the one special case that avoids it.
> Three pieces of state matter throughout. `el_vdisplay` is the virtual
> image — what the editor wants on screen — and is rebuilt from scratch
> here by `re_addc`/`re_putc`/`re_putliteral`, which write only to it.
> `el_display` is the real image — what libedit believes is physically on
> screen — and is brought into line row by row. `el->el_refresh.r_cursor`
> is the drawing cursor into `el_vdisplay`; `el->el_cursor` is the
> recorded position of the real terminal cursor and is maintained by the
> `terminal_*` routines, never directly here.
> Step 1: `literal_clear(el)` — discard the literal byte strings interned
> by the previous refresh, because the magic characters about to be
> written index a freshly numbered table.
> Step 2: zero the drawing cursor (`r_cursor.h = r_cursor.v = 0`) and
> call `terminal_move_to_char(el, 0)` to put the real cursor at column 0
> of whatever row it is on.
> Step 3: `prompt_print(el, EL_RPROMPT)`. This draws the right-hand
> prompt into `el_vdisplay` starting at row 0 column 0 purely so that
> `prompt_print` leaves `el_rprompt.p_pos` holding its width and row
> count; nothing is emitted to the terminal. Then zero the drawing cursor
> again, so the real prompt overwrites those cells.
> Step 4: clamp the logical cursor. If `el_line.cursor >=
> el_line.lastchar`, then: if the alt keymap is current (vi command mode)
> and the line is non-empty, set the cursor to `lastchar - 1`; otherwise
> set it to `lastchar`.
> Step 5: set the saved screen position `cur.h = -1` (a "not yet found"
> sentinel) and `cur.v = 0`.
> Step 6: `prompt_print(el, EL_PROMPT)` — draw the left prompt into the
> virtual image; it records its end position in `el_prompt.p_pos`, which
> `re_refresh_cursor` later takes as its origin.
> Step 7: walk the input buffer from `el_line.buffer` up to but not
> including `el_line.lastchar`. For each character: if this position is
> `el_line.cursor`, save the current drawing cursor into `cur`, and then,
> if the character is double-width (`wcwidth > 1`) and `r_cursor.h + w >
> el_terminal.t_size.h` — it will be pushed to the next row — overwrite
> that with `cur.h = 0` and `cur.v` one greater. Then draw the character
> with `re_addc`, which expands tabs, control characters and
> non-printables. (A `#if notyet` block would have started this walk
> partway into the buffer for lines longer than the screen; it is not
> compiled and must not be ported.)
> Step 8: if `cur.h` is still -1 the cursor is at end of line, so set
> `cur` to the current drawing cursor.
> Step 9: decide whether the right prompt fits. Compute `rhdiff =
> el_terminal.t_size.h - r_cursor.h - el_rprompt.p_pos.h`. Draw the right
> prompt only if all four hold: `el_rprompt.p_pos.h` is non-zero (one
> exists), `el_rprompt.p_pos.v` is zero (it fits on one row),
> `r_cursor.v` is zero (the input has not wrapped past the first row),
> and `rhdiff > 1` (at least two free columns). If so, emit `rhdiff - 1`
> spaces with `re_putc(el, ' ', 1)` — the C loop is `while (--rhdiff >
> 0)` — and then `prompt_print(el, EL_RPROMPT)` again. That leaves at
> least one blank column between the input and the prompt and stops one
> column short of the right margin, so the row never wraps. If the test
> fails, set both `el_rprompt.p_pos.h` and `.v` to 0, which is the flag
> "not using the right prompt" that `re_fastaddc` and the next refresh
> read.
> Note the side effect of the second `prompt_print`: it overwrites
> `el_rprompt.p_pos` with the drawing cursor *after* the rprompt, so from
> here until the next refresh `p_pos.h` is `t_size.h - 1` — the end
> column — and no longer the rprompt's width. `re_fastaddc` reads it in
> that state.
> Step 10: `re_putc(el, '\0', 0)` to terminate the virtual row at the
> current column without moving the drawing cursor.
> Step 11: record `el->el_refresh.r_newcv = r_cursor.v`, the index of the
> last row the new image occupies.
> Step 12: for each row `i` from 0 to `r_newcv` inclusive, in order:
> call `re_update_line(el, el_display[i], el_vdisplay[i], i)` to emit the
> minimal terminal operations that turn the real row into the virtual
> one, then `re__copy_and_pad(el_display[i], el_vdisplay[i],
> el_terminal.t_size.h)` to make the real image an exact, space-padded
> copy of the virtual one. Both arrays are mutated in place by
> `re_update_line`, which truncates each at its last non-blank character;
> the pad afterwards is what restores a full-width row, and it is what
> makes `el_display` safe for `terminal_move_to_char` to replay.
> Step 13: if the previous image was taller than the new one — `r_oldcv >
> r_newcv` — erase the surplus rows. For each `i` from `r_newcv + 1` up
> to `r_oldcv` inclusive: `terminal_move_to_line(el, i)`,
> `terminal_move_to_char(el, 0)`, `terminal_clear_EOL(el, (int)
> wcslen(el_display[i]))`, and then store `'\0'` in `el_display[i][0]` to
> mark the row empty. The `wcslen` is safe over MB_FILL_CHAR cells
> because MB_FILL_CHAR is `(wint_t)-1`, never zero.
> Step 14: set `r_oldcv = r_newcv` for the next refresh.
> Step 15: `terminal_move_to_line(el, cur.v)` then
> `terminal_move_to_char(el, cur.h)`, leaving the terminal cursor where
> the logical cursor belongs. No flush is done here; callers flush.
> Nothing in this routine rescues the case where the input is longer than
> the screen and `re_nextline` scrolled `el_vdisplay` without scrolling
> `el_display`: rows are diffed by index, so after a scroll the
> comparison is against the wrong content.

> [spec:libedit:def:refresh.re-strncopy-fn]
> static void re__strncopy(wchar_t *a, wchar_t *b, size_t n)

> [spec:libedit:sem:refresh.re-strncopy-fn]
> Copies at most `n` wide characters from `b` to `a`, stopping early at
> the first NUL in `b`. The loop condition post-decrements `n` and also
> tests the current source cell, so it runs for at most the initial value
> of `n` iterations and stops at a source terminator; each iteration
> copies one cell and advances both pointers.
> Unlike `strncpy` it neither pads the destination nor writes a
> terminating NUL — the destination keeps whatever lay beyond the copied
> region. That is deliberate: `re_update_line` uses it to mirror into
> `el_display` exactly the run of characters it has just written to the
> terminal, and the rest of the row must survive untouched.
> `n` is a `size_t`. A caller passing a value derived from a negative
> difference would loop until it happened to find a NUL in `b`; no
> in-tree caller does, since every length `re_update_line` passes is a
> non-negative pointer difference.

> [spec:libedit:def:refresh.re-update-line-fn]
> static void re_update_line(EditLine *el, wchar_t *old, wchar_t *new, int i)

> [spec:libedit:sem:refresh.re-update-line-fn]
> Turns the physically-displayed row `old` (which is `el_display[i]`)
> into the wanted row `new` (which is `el_vdisplay[i]`), emitting the
> smallest set of terminal operations it can find, and keeping `old` in
> step with what it emitted. Both arrays are mutated in place. `i` is the
> screen row number.
> The model is a single "middle difference": the two rows share a common
> prefix and a common trailing run, and between them lies a middle that
> may itself contain one long common run. That lets the change be
> expressed as at most two edits — a first difference near the start and
> a second difference near the end — each of which is an insert, a
> delete, or a plain overwrite.
>
> **Pointers and their meaning.** Twelve pointers delimit the row, six
> into `old` and six into `new`:
> `ofd` / `nfd` — first difference: the first index at which the two rows
> differ.
> `osb` / `nsb` — start of the common middle run ("same begin").
> `ose` / `nse` — end of that run, exclusive ("same end").
> `ols` / `nls` — start of the common trailing run ("last same").
> `oe` / `ne` — end of the row, after trailing blanks are trimmed.
> Invariants once all are set: `old <= ofd <= osb <= ose <= ols <= oe`
> and likewise for `new`; `ose - osb == nse - nsb` (the middle run is the
> same length on both sides); `oe - ols == ne - nls` (so is the trailing
> run); and, because the prefix scan runs in lockstep, `ofd - old == nfd
> - new`.
>
> **Phase 1 — find the boundaries.**
> Step 1: advance in lockstep from the start of both rows while the old
> character is non-NUL and the two characters are equal; set `ofd` and
> `nfd` where that stops. Testing only `old`'s terminator suffices, since
> `new`'s terminator cannot equal a non-NUL old character.
> Step 2: find the end of `old`, then back that pointer up over trailing
> spaces, never below `ofd`; set `oe` there and store `'\0'` at `oe`,
> truncating the row in place. Do the same for `new`, giving `ne`.
> Trailing blanks are dropped because the terminal is assumed already
> blank to the right of the text; this in-place truncation is exactly why
> `re_refresh` must pad `el_display` back out with `re__copy_and_pad`
> afterwards, and why `el_vdisplay` must be rebuilt rather than reused.
> Step 3: if both `*ofd` and `*nfd` are now NUL, the rows are identical
> after trimming — return immediately, having emitted nothing and, in
> particular, without moving the cursor to row `i`. Not moving is the
> whole point: an unchanged row must cost nothing.
> Step 4: find the common trailing run. Starting from `oe` and `ne`,
> pre-decrement both pointers and compare, continuing while the old
> pointer is above `ofd`, the new pointer is above `nfd`, and the
> characters match. Then set `ols` and `nls` to one past where each
> pointer stopped. Because the increment is applied both on the mismatch
> exit and on the "ran back to `ofd`/`nfd`" exit, this is the longest
> common suffix except that at least one character is always left in each
> middle: `ols > ofd` and `nls > nfd` always hold, and the two suffixes
> have equal length.
>
> **Phase 2 — find the common run inside the middle.** The middle is
> `[ofd, ols)` on the old side and `[nfd, nls)` on the new side. `osb`,
> `ose`, `nsb` and `nse` all start at `ols` / `nls`, i.e. an empty run
> pinned at the far end. Two scans then look for a better one; each keeps
> the strictly longest match found so far and each rejects a match not
> worth its cost.
> Scan A models an insertion and runs only if the old middle is non-empty
> (`*ofd` is not NUL). Let `c = *ofd`. For each position `n` in `[nfd,
> nls)` with `*n == c`, extend a match forward from `ofd` in old and from
> `n` in new while both stay inside their middles (`o < ols` and `p <
> nls`) and the characters agree; the match length is `p - n`. Accept it
> only if **both**: `(nse - nsb) < (p - n)` — strictly longer than the
> best so far — and `2 * (p - n) > n - nfd` — its length is more than
> half the number of new characters that would have to be inserted ahead
> of it. On acceptance set `nsb = n`, `nse = p`, `osb = ofd`, `ose = o`.
> The old side is anchored at the first difference, because an insertion
> does not move old content.
> Scan B models a deletion and runs only if the new middle is non-empty
> (`*nfd` is not NUL). Let `c = *nfd`. For each position `o` in `[ofd,
> ols)` with `*o == c`, extend forward from `nfd` in new and from `o` in
> old within their middles; the match length is `p - o`. Accept only if
> `(ose - osb) < (p - o)` and `2 * (p - o) > o - ofd`. On acceptance set
> `nsb = nfd`, `nse = n`, `osb = o`, `ose = p`.
> Scan B runs after scan A and compares against whatever A left, so the
> two compete under one rule: strictly longer wins, and a deletion
> interpretation must be strictly longer to displace an insertion one.
> Both scans are quadratic in the middle length in the worst case; that
> is the cost model as written and there is no early exit.
>
> **Phase 3 — pragmatics.**
> Pragmatic I: if the common trailing run is shorter than MIN_END_KEEP
> (4) characters — `(oe - ols) < 4` — abandon it: set `ols = oe` and
> `nls = ne`. Preserving a two- or three-character tail is not worth the
> cursor motion. The C describes MIN_END_KEEP as roughly half the cost of
> entering insert mode, inserting a character and leaving again, notes it
> "should really be calculated from the termcap data", and hardcodes 4 as
> a good value for ANSI terminals. It stays a constant in the port; it is
> not derived from terminfo.
> Pragmatic II: degrade the plan to what the terminal can actually do.
> First compute the two edit sizes:
> `fx = (nsb - nfd) - (osb - ofd)` — the net number of characters to
> insert (positive) or delete (negative) at the first difference, to
> bring the two middle runs into alignment.
> `sx = (nls - nse) - (ols - ose)` — the same for the second difference,
> between the end of the middle run and the start of the trailing run.
> If the TERM_CAN_INSERT flag is clear in `el_terminal.t_flags`: if `fx >
> 0`, collapse the middle run to nothing at the far end (`osb = ose =
> ols`, `nsb = nse = nls`); if `sx > 0`, abandon the trailing run (`ols =
> oe`, `nls = ne`); and if `(ols - ofd) < (nls - nfd)` — the new region
> is longer, so honouring the tail would require an insert — abandon the
> trailing run too.
> If the TERM_CAN_DELETE flag is clear: the mirror image. If `fx < 0`,
> collapse the middle run; if `sx < 0`, abandon the trailing run; and if
> `(ols - ofd) > (nls - nfd)`, abandon the trailing run.
> `fx` and `sx` are *not* recomputed between the two blocks, so the
> delete block tests the values that were current before the insert block
> ran. Reproduce that ordering literally.
> Pragmatic III: if the middle run that survived is shorter than
> MIN_END_KEEP (4) — `(ose - osb) < 4` — collapse it as well: `osb = ose
> = ols` and `nsb = nse = nls`. (The C notes this test replaced an older
> `osb == ose`.)
> Then recompute `fx` and `sx` from the final pointers with the same two
> formulas. From here on `fx` and `sx` are the plan.
>
> **Phase 4 — emit.** Call `terminal_move_to_line(el, i)`. This happens
> here and not earlier precisely so that the early return in phase 1 step
> 3 leaves the cursor alone.
> Let `p` be the last old character position that still matters: `oe` if
> a trailing run survived (`ols != oe`), otherwise `ose`.
>
> *4a — early first-difference insert.* Taken when all three hold: `nsb
> != nfd` (the new side really does have extra leading characters), `fx >
> 0`, and `(p - old) + fx <= el_terminal.t_size.h` (inserting `fx`
> columns will not push the last character that matters off the right
> edge). Move to column `nfd - new`. Then:
> - If `nsb != ne` — there is content beyond the insertion worth keeping
> — call `terminal_insertwrite(el, nfd, fx)` to open `fx` columns and
> write the first `fx` new characters into them, and mirror the same edit
> into `old` with `re_insert(el, old, ofd - old, t_size.h, nfd, fx)`.
> Then overwrite the remaining `len = (nsb - nfd) - fx` characters
> starting at `nfd + fx` with `terminal_overwrite`, mirroring with
> `re__strncopy(ofd + fx, nfd + fx, len)`. Note `len` equals `osb - ofd`
> and so is never negative. Fall through to 4c with `fx` still positive.
> - Otherwise (`nsb == ne`, nothing beyond): `terminal_overwrite(el, nfd,
> nsb - nfd)`, mirror with `re__strncopy(ofd, nfd, nsb - nfd)`, and
> **return** — the row is finished.
>
> *4b — first-difference delete.* Taken when 4a was not taken and `fx <
> 0`. Move to column `ofd - old`. Then:
> - If `osb != oe` — old content past the deletion must be preserved —
> call `terminal_deletechars(el, -fx)` and mirror with `re_delete(el,
> old, ofd - old, t_size.h, -fx)`; then `terminal_overwrite(el, nfd, nsb
> - nfd)` and mirror with `re__strncopy(ofd, nfd, nsb - nfd)`. Fall
> through to 4c with `fx` still negative.
> - Otherwise: `terminal_overwrite(el, nfd, nsb - nfd)`, then
> `re_clear_eol(el, fx, sx, (oe - old) - (ne - new))` to blank whatever
> is still visible to the right, and **return**.
>
> If neither 4a nor 4b applied, set `fx = 0`. This is a flag as much as a
> value — it records "no early first-difference edit was performed" — and
> it is simultaneously the column shift that 4c must add. An insert
> rejected by 4a's width test lands here and is retried in 4d.
>
> *4c — second-difference delete.* Taken when `sx < 0` and `(ose - old) +
> fx < el_terminal.t_size.h`. Move to column `(ose - old) + fx`. Then:
> - If `ols != oe` (a trailing run survives): `terminal_deletechars(el,
> -sx)`, then `terminal_overwrite(el, nse, nls - nse)`.
> - Otherwise: `terminal_overwrite(el, nse, nls - nse)` followed by
> `re_clear_eol(el, fx, sx, (oe - old) - (ne - new))`.
> The column arithmetic here is the subtlest part of the routine and is
> deliberate, not accidental. By the invariants, `nse - new` equals `(ose
> - old) + fx_true`, where `fx_true` is the real first-difference shift.
> When 4a or 4b already performed that shift, `fx` still holds
> `fx_true` and this addresses the row in its post-shift coordinates.
> When the shift was deferred (4a rejected on width), `fx` is 0 and this
> addresses the row in its pre-shift coordinates; 4d's later insert then
> pushes what 4c wrote into its correct final column. Either way the
> content lands at `nse - new`.
> Neither branch mirrors anything into `old`; from here on the old image
> is stale. That is safe only because `re_refresh` rewrites the whole row
> with `re__copy_and_pad` immediately afterwards. It is *not* safe to
> reorder these steps: `terminal_move_to_char` moves rightwards by
> replaying characters out of `el_display`, so `old` must still be
> accurate for every column to the left of the next move — which is
> exactly what the mirroring in 4a and 4b protects.
> If `sx < 0` but the on-screen test fails, the second difference is
> never applied at all: nothing is emitted for it, and yet `re_refresh`
> then declares `el_display` equal to `el_vdisplay`. Screen and model
> diverge silently, and the divergence persists until something forces a
> full redraw. Reproduce it, but it is a real hole in the algorithm.
>
> *4d — late first-difference insert.* Taken when `nsb != nfd`, `(osb -
> ofd) <= (nsb - nfd)`, and `fx == 0` — a first-difference edit is still
> owed and 4a/4b did not do it, either because `fx` was genuinely 0 (a
> pure overwrite) or because 4a's width test rejected the insert. Move to
> column `nfd - new`. Then:
> - If `nsb != ne`: recompute `fx = (nsb - nfd) - (osb - ofd)`, since it
> was zeroed as a flag. If it is positive, `terminal_insertwrite(el, nfd,
> fx)` and mirror with `re_insert(el, old, ofd - old, t_size.h, nfd,
> fx)`. Then `terminal_overwrite(el, nfd + fx, len)` with `len = (nsb -
> nfd) - fx`, mirroring with `re__strncopy(ofd + fx, nfd + fx, len)`.
> - Otherwise: `terminal_overwrite(el, nfd, nsb - nfd)`, mirroring with
> `re__strncopy(ofd, nfd, nsb - nfd)`.
> This runs *after* 4c, so when both fire the second difference is edited
> before the first: deletes at the end first, insert at the front second.
> That order is what keeps the intermediate row from overflowing, and it
> is what makes 4c's pre-shift addressing come out right.
>
> *4e — second-difference insert or overwrite.* Taken when `sx >= 0`,
> which includes `sx == 0`, the plain-overwrite case. Move to column `nse
> - new`. Then:
> - If `ols != oe` (a trailing run survives): if `sx > 0`,
> `terminal_insertwrite(el, nse, sx)` to open room; then
> `terminal_overwrite(el, nse + sx, (nls - nse) - sx)`. That length
> equals `ols - ose` and so is never negative.
> - Otherwise: `terminal_overwrite(el, nse, nls - nse)`. No
> clear-to-end-of-line is needed, because with no trailing run to
> preserve this write has covered everything the old row had.
> Emitting is then complete and the routine returns. It has no return
> value; the only "results" are the terminal output, the mutations to
> `old`, and the truncations of `old` and `new`.
>
> **Notes for the port.**
> - The four `ELRE_DEBUG(!EL_CAN_INSERT, ...)` / `(!EL_CAN_DELETE, ...)`
> calls are assertions compiled only under DEBUG_REFRESH. They record the
> claim that Pragmatic II makes an insert or delete on an incapable
> terminal unreachable. `terminal_insertwrite` and `terminal_deletechars`
> also refuse on their own, so if the claim were ever false the failure
> mode is a missing edit, not a malformed escape sequence.
> - The twelve `re_printstr` calls are debug-only region dumps.
> - Nothing in this routine is aware of MB_FILL_CHAR or of column widths:
> it compares and counts cells. Cell counts equal column counts only
> because `re_putc` writes one MB_FILL_CHAR per extra column of a
> multi-column character, and because the zero-width case is charged one
> cell and one column. A port must preserve that invariant, or the
> comparisons against `el_terminal.t_size.h` stop meaning columns. Note
> also that a MB_FILL_CHAR cell compares equal only to another
> MB_FILL_CHAR, so a wide character can never be judged "the same" as the
> tail of a different one.
> - `wcwidth` is never called here. Any port that tries to make the
> differ width-aware is changing the algorithm, not implementing it.
