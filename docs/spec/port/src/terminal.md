# src/terminal.c, src/terminal.h

> [spec:libedit:def:terminal.el-terminal-t]
> typedef struct

> [spec:libedit:def:terminal.funckey-t]
> typedef struct

> [spec:libedit:def:terminal.termcapstr]
> struct termcapstr {
>   const char *name;
>   const char *long_name;
> }

> [spec:libedit:def:terminal.termcapval]
> struct termcapval {
>   const char *name;
>   const char *long_name;
> }

> [spec:libedit:def:terminal.terminal-alloc-buffer-fn]
> static wint_t ** terminal_alloc_buffer(EditLine *el)

> [spec:libedit:sem:terminal.terminal-alloc-buffer-fn]
> Allocates one screen-image buffer sized from `el->el_terminal.t_size`,
> where `.v` is the row count and `.h` is the column count.
> Step 1: allocate a zero-filled array of `t_size.v + 1` row pointers; on
> allocation failure return NULL.
> Step 2: for each row index `i` from 0 to `t_size.v - 1`, allocate a
> zero-filled row of `t_size.h + 1` cells, each cell a `wint_t`. If any
> row allocation fails, free rows 0 through `i - 1`, free the row-pointer
> array, and return NULL — no partial buffer ever escapes.
> Step 3: store NULL in slot `t_size.v`. The zero-fill already did this;
> the explicit store records the contract that the array is
> NULL-terminated, which is what `terminal_free_buffer` walks and what
> makes freeing independent of the size that was current at allocation.
> Step 4: return the array.
> Rows are one cell longer than the column count so that a full-width
> line can still carry a terminating `'\0'` cell. Every cell starts at 0,
> so every row initially reads as an empty line.
> The caller guarantees `t_size` is already clamped to sane values
> (`terminal_change_size` forces columns >= 2 and lines >= 1), so the
> degenerate zero/negative dimensions are not reachable in practice.
> Two of these buffers exist per EditLine: `el_display`, the image of
> what is believed to be physically on the screen, and `el_vdisplay`,
> the image of what the editor wants on the screen.

> [spec:libedit:def:terminal.terminal-alloc-display-fn]
> static int terminal_alloc_display(EditLine *el)

> [spec:libedit:sem:terminal.terminal-alloc-display-fn]
> Allocates both screen images at the current `el_terminal.t_size`.
> Step 1: allocate `el->el_display` with the buffer allocator; if it
> returns NULL, go to the cleanup path.
> Step 2: allocate `el->el_vdisplay` the same way; if it returns NULL, go
> to the cleanup path.
> Step 3: return 0.
> Cleanup path: call `terminal_free_display`, which frees whichever of
> the two was allocated and sets both fields to NULL, then return -1.
> On failure the caller is therefore left with both fields NULL rather
> than a half-built pair.

> [spec:libedit:def:terminal.terminal-alloc-fn]
> static void terminal_alloc(EditLine *el, const struct termcapstr *t, const char *cap)

> [spec:libedit:sem:terminal.terminal-alloc-fn]
> Interns the capability string `cap` into the per-EditLine string pool
> and points the table slot for `t` at it. The slot index is `t`'s
> position in the string-capability table; the pool is
> `el->el_terminal.t_buf`, a flat byte array of TC_BUFSIZE (2048) bytes,
> and `el->el_terminal.t_loc` is the high-water mark of bytes used in it.
> Call `str` the slot `el->el_terminal.t_str[index of t]`.
> Step 1: if `cap` is NULL or the empty string, store NULL in the slot
> and return. The slot now reads as "capability absent"; whatever pool
> bytes it used are abandoned, not reclaimed.
> Step 2: let `clen` be the length of `cap`, and `tlen` be the length of
> whatever the slot currently points at (0 if the slot is NULL).
> Step 3: if `clen <= tlen`, the new value fits where the old one lived:
> copy it over the old string in place, including its NUL, and return.
> `t_loc` does not move, so the slack bytes are wasted but harmless.
> Because `cap` is non-empty, `clen >= 1`, so this branch is only ever
> taken when the slot is non-NULL.
> Step 4: otherwise the new value is longer than the old and must be
> appended. If `t_loc + 3 < TC_BUFSIZE`, point the slot at
> `&t_buf[t_loc]`, copy `cap` and its NUL there, and advance `t_loc` by
> `clen + 1`. Note that the bound test does not involve `clen` at all: it
> reserves a fixed three bytes regardless of how long the string is, so a
> capability whose length exceeds the remaining pool space writes past
> the end of `t_buf`. That is a buffer overflow and undefined behaviour
> in the C. The port must test that `clen + 1` fits.
> Step 5: if the append bound fails, compact the pool. Walk the string
> table in index order and copy every slot that is non-NULL, non-empty
> and not this slot into a scratch buffer, each followed by a NUL,
> accumulating the total length `tlen`. Copy the whole scratch buffer
> (all TC_BUFSIZE bytes) over `t_buf` and set `t_loc = tlen`. The
> compaction does not repoint the table slots at their new offsets, so
> after it runs every other slot still points at its old offset and now
> reads whatever the compacted layout put there: the retained capability
> strings are silently corrupted. This is a latent bug in the C, reached
> only when 2048 bytes of pool are exhausted (rare, since 39 capabilities
> rarely total that much). The scratch copy is also unbounded and can
> itself overflow. The port should not reproduce the pool at all — an
> owned string per slot removes the whole class of problem.
> Step 6: after compaction, if `t_loc + 3 >= TC_BUFSIZE`, print
> "Out of termcap string space.\n" to `el->el_errfile` and return with
> the slot unchanged.
> Step 7: otherwise append exactly as in step 4 and advance `t_loc` by
> `clen + 1`.
> Callers: `terminal_set`, which fills every slot after loading a
> terminal entry, and `terminal_settc`, which sets one slot from user
> input.

> [spec:libedit:def:terminal.terminal-beep-fn]
> libedit_private void terminal_beep(EditLine *el)

> [spec:libedit:sem:terminal.terminal-beep-fn]
> Rings the terminal's bell.
> If the audible-bell capability (terminfo `bell`, capname `bel`; the C's
> termcap code `bl`) is present and non-empty, emit it through
> `terminal_tputs` with an affected-line count of 1.
> Otherwise write a literal ASCII BEL (0x07) with `terminal__putc`.
> "Present and non-empty" means the table slot is neither NULL nor a
> zero-length string; that test is used identically throughout this file.
> The function never touches `el_cursor` and never flushes; the caller
> decides when output reaches the terminal.

> [spec:libedit:def:terminal.terminal-bind-arrow-fn]
> libedit_private void terminal_bind_arrow(EditLine *el)

> [spec:libedit:sem:terminal.terminal-bind-arrow-fn]
> Installs the arrow/edit key bindings into the live key map, from both
> libedit's hard-coded defaults and the terminal's own key capabilities,
> without clobbering anything the user has rebound.
> Step 1: if `el->el_terminal.t_buf` is NULL or `el->el_map.key` is NULL,
> return immediately. Both subsystems must exist; `terminal_set` calls
> this function and `terminal_set` runs from inside `terminal_init`,
> which in the library's initialisation order precedes `map_init`, so on
> the very first call the map is still NULL and this guard is what stops
> the function.
> Step 2: pick the map pair by editor mode. If `el->el_map.type` is
> MAP_VI, the live map is `el->el_map.alt` (the vi command map) and the
> reference map is `el->el_map.vic`; otherwise the live map is
> `el->el_map.key` and the reference map is `el->el_map.emacs`. Both are
> 256-entry `el_action_t` arrays indexed by a leading input byte; the
> reference map holds the mode's factory defaults and is used to detect
> whether the user has changed a binding.
> Step 3: call `terminal_reset_arrow` to (re)install the twelve
> hard-coded ANSI/SS3 sequences.
> Step 4: for each of the seven function-key slots, in table order:
> (a) take `p`, the capability string for that slot's capability index
> (`el->el_terminal.t_str[slot.key]`). If it is NULL or empty, skip the
> slot — the terminal does not report that key.
> (b) widen `p` into a wide-character buffer of exactly VISUAL_WIDTH_MAX
> (8) elements: for `n` from 0 while `n < 8` and `p[n]` is not NUL, store
> `p[n]` widened; then fill the remaining elements with L'\0'. Two
> defects live here. If the capability is 8 bytes or longer the buffer
> ends up with no terminator at all, and the `keymacro_*` calls below
> read past its end — undefined behaviour; the port must bound the copy
> to 7 elements and always terminate. And `p[n]` is a plain `char`, so on
> a platform where `char` is signed a byte >= 0x80 widens to a negative
> wide character; the port must widen through an unsigned byte.
> (c) let `j` be the first byte of the capability taken as an unsigned
> char, i.e. an index in 0..255 into the maps.
> (d) if the slot's type is XK_NOD — the binding was explicitly cleared
> by `terminal_clear_arrow` — call `keymacro_clear` on the live map with
> the widened sequence, and move to the next slot.
> (e) otherwise, if the capability is more than one byte long (`p[1]` is
> not NUL) and the leading byte is either still at its factory default
> (reference map and live map agree at `j`) or already marked as a
> sequence lead-in (live map at `j` is ED_SEQUENCE_LEAD_IN): add the
> whole sequence as a macro with `keymacro_add`, using the slot's bound
> function value and type, and set the live map at `j` to
> ED_SEQUENCE_LEAD_IN. This is what lets ESC-prefixed arrow sequences
> work while leaving a user-rebound ESC alone.
> (f) otherwise, if the leading byte is unassigned in the live map (equal
> to ED_UNASSIGNED): call `keymacro_clear` on the sequence, then if the
> slot's type is XK_CMD bind the single byte directly by storing the
> slot's command in the live map at `j`, else add the sequence with
> `keymacro_add`.
> (g) otherwise the user has bound that leading byte to something of
> their own; do nothing for this slot.
> No return value; failures inside `keymacro_add` are not surfaced.

> [spec:libedit:def:terminal.terminal-change-size-fn]
> libedit_private int terminal_change_size(EditLine *el, int lins, int cols)

> [spec:libedit:sem:terminal.terminal-change-size-fn]
> Applies a new screen size: reclamps it, rebuilds both screen images and
> resets the display model.
> Step 1: save a copy of `el->el_cursor`.
> Step 2: store the clamped size into the capability value table — the
> column slot (terminfo `columns`; termcap `co`) becomes 80 if `cols` is
> less than 2 and `cols` otherwise; the line slot (terminfo `lines`;
> termcap `li`) becomes 24 if `lins` is less than 1 and `lins` otherwise.
> Degenerate sizes are clamped to the classic 80x24 default rather than
> rejected.
> Step 3: call `terminal_rebuffer_display`, which frees both screen
> images, copies the clamped size into `el_terminal.t_size` and
> reallocates them. If it returns -1, return -1 at once: `el_display` and
> `el_vdisplay` are left NULL and the saved cursor is not restored.
> Step 4: call `re_clear_display`, which zeroes `el_cursor`, blanks the
> first cell of every row of `el_display` and resets the refresh layer's
> old-cursor-line record.
> Step 5: restore `el->el_cursor` from the saved copy, undoing step 4's
> reset of it.
> Step 6: return 0.
> Nothing revalidates the restored cursor against the new dimensions, so
> after a shrink `el_cursor` may name a position off the screen.

> [spec:libedit:def:terminal.terminal-clear-arrow-fn]
> libedit_private int terminal_clear_arrow(EditLine *el, const wchar_t *name)

> [spec:libedit:sem:terminal.terminal-clear-arrow-fn]
> Marks one named function-key binding as cleared.
> Scan the seven function-key table entries in order, comparing `name`
> for exact wide-string equality with each entry's name. On the first
> match, set that entry's type to XK_NOD and return 0; the entry's bound
> function value is left untouched. If no entry has that name, return -1
> and change nothing.
> The recognised names are exactly those installed by
> `terminal_init_arrow`: "down", "up", "left", "right", "home", "end",
> "delete".
> Clearing only updates the table. The key map is not modified until
> `terminal_bind_arrow` next runs, at which point an XK_NOD entry causes
> `keymacro_clear` to be called on that key's capability sequence.

> [spec:libedit:def:terminal.terminal-clear-eol-fn]
> libedit_private void terminal_clear_EOL(EditLine *el, int num)

> [spec:libedit:sem:terminal.terminal-clear-eol-fn]
> Clears from the cursor to the end of the line. `num` is the caller's
> count of columns that still need clearing.
> If the TERM_CAN_CEOL flag is set in `el_terminal.t_flags` *and* the
> clear-to-end-of-line capability (terminfo `clr_eol`, capname `el`;
> termcap `ce`) is present and non-empty, emit that capability via
> `terminal_tputs` with an affected-line count of 1, and leave
> `el->el_cursor.h` unchanged — the capability does not move the cursor.
> Otherwise write `num` space characters with `terminal__putc` and add
> `num` to `el->el_cursor.h`.
> The two paths deliberately disagree about the cursor: the capability
> path leaves it where it was, the fallback leaves it `num` columns
> further right, which is where writing spaces actually put it.
> There is no wrap handling on the fallback path — if `num` pushes the
> recorded column past `t_size.h` nothing corrects it, unlike
> `terminal_overwrite`.
> With `num <= 0` the fallback writes nothing but still adds `num` to the
> recorded column, so a negative `num` would move it backwards. No
> in-tree caller passes a negative value.

> [spec:libedit:def:terminal.terminal-clear-screen-fn]
> libedit_private void terminal_clear_screen(EditLine *el)

> [spec:libedit:sem:terminal.terminal-clear-screen-fn]
> Clears the whole screen and homes the cursor, using the best available
> capability. The three strategies are tried in this order.
> 1. If the clear-screen capability (terminfo `clear_screen`, capname
> `clear`; termcap `cl`) is present and non-empty, emit it via
> `terminal_tputs` with an affected-line count equal to the recorded line
> count (terminfo `lines`; termcap `li`), so that a per-affected-line
> padding delay is computed for a whole screen's worth of work.
> 2. Otherwise, if both the home capability (terminfo `cursor_home`,
> capname `home`; termcap `ho`) and the clear-to-bottom capability
> (terminfo `clr_eos`, capname `ed`; termcap `cd`) are present and
> non-empty, emit home first and then clear-to-bottom, each with an
> affected-line count equal to the recorded line count.
> 3. Otherwise write a carriage return and then a line feed with
> `terminal__putc` — on a terminal with no clearing capability at all,
> the best that can be done is to scroll one line.
> None of the three paths updates `el_cursor`; the caller is responsible
> for resynchronising the model, normally by way of `re_clear_display`.

> [spec:libedit:def:terminal.terminal-deletechars-fn]
> libedit_private void terminal_deletechars(EditLine *el, int num)

> [spec:libedit:sem:terminal.terminal-deletechars-fn]
> Deletes `num` characters at the cursor, pulling the remainder of the
> line left.
> Early returns, tested in this order: if `num <= 0`, return; if the
> TERM_CAN_DELETE flag is clear in `el_terminal.t_flags`, return with
> nothing emitted (under DEBUG_EDIT the C also prints an error line); if
> `num` is greater than `el_terminal.t_size.h`, return (a sanity check
> against a nonsensical count).
> Then, in order:
> 1. If the parameterised delete capability (terminfo `parm_dch`, capname
> `dch`; termcap `DC`) is present and non-empty, and either `num` is
> greater than 1 or the single-character delete (terminfo
> `delete_character`, capname `dch1`; termcap `dc`) is absent or empty,
> expand the parameterised capability with `num` passed as both
> parameters and emit it with an affected-line count of `num`; then
> return. The "num > 1 or no dch1" test is the cost heuristic: for a
> single deletion the one-character form is assumed cheaper.
> 2. Otherwise, if delete mode is available (terminfo
> `enter_delete_mode`, capname `smdc`; termcap `dm`) emit it once with an
> affected-line count of 1.
> 3. If the single-character delete is present and non-empty, emit it
> `num` times, each with an affected-line count of 1.
> 4. If exit-delete-mode (terminfo `exit_delete_mode`, capname `rmdc`;
> termcap `ed`) is present and non-empty, emit it once with an
> affected-line count of 1.
> Because TERM_CAN_DELETE requires at least one of the two delete
> capabilities, and the case "parameterised present, one-shot absent" has
> already returned at step 1, step 3 always emits something when reached.
> This function updates neither `el_cursor` nor `el_display`; the caller
> owns the screen model.

> [spec:libedit:def:terminal.terminal-echotc-fn]
> libedit_private int /*ARGSUSED*/ terminal_echotc(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv)

> [spec:libedit:sem:terminal.terminal-echotc-fn]
> Implements the `echotc` editrc command and `el_set(EL_ECHOTC, ...)`:
> look up a capability by name and either emit it to the terminal or
> print a derived value. `argc` is ignored; `argv[0]` is the command name
> and is used only in diagnostics.
> Step 1: if `argv` is NULL or `argv[1]` is NULL, return -1.
> Step 2: advance past `argv[0]`.
> Step 3: option parsing, at most one option. If the current argument
> begins with '-', look at its second character: 'v' turns on verbose,
> 's' turns on silent, and any other letter is ignored with no
> diagnostic. Advance one argument in all three cases. Only one leading
> option is ever consumed.
> Step 4: if there is no remaining argument, or it is the empty string,
> return 0 having produced no output.
> Step 5: pseudo-capabilities. Compare the remaining argument for exact
> wide-string equality against each of the following; on a match print
> one line to `el->el_outfile` and return 0.
> "tabs" prints "yes" or "no" from the TERM_CAN_TAB flag.
> "meta" prints "yes" or "no" from the meta-key value slot (terminfo
> `has_meta_key`; termcap `km`) directly — not from the TERM_HAS_META
> flag, so the separate MT slot is deliberately not considered here.
> "xn" prints "yes" or "no" from the TERM_HAS_MAGIC_MARGINS flag.
> "am" prints "yes" or "no" from the TERM_HAS_AUTO_MARGINS flag.
> "baud" prints `el->el_tty.t_speed` as a decimal integer.
> "rows" or "lines" prints the recorded line count as a decimal integer.
> "cols" prints the recorded column count as a decimal integer.
> The yes/no lines use the format "%s\n" and the numeric ones "%d\n".
> Step 6: resolve the argument to a capability string. Encode it to a
> narrow string and compare it against the `name` field of each entry in
> the string-capability table; on a match take the corresponding slot
> from `el->el_terminal.t_str` and stop searching. If the name matched no
> table entry, fall back to a direct lookup in the terminal database (the
> C's `tgetstr`; in the port, the capability module's string lookup), so
> that `echotc` can print capabilities libedit does not itself track.
> Note the fallback is reached only when the *name* is unknown: a known
> name whose slot happens to be empty does not fall back.
> Step 7: if the resolved string is NULL or empty then, unless silent,
> print "echotc: Termcap parameter `%ls' not found.\n" to
> `el->el_errfile`; return -1.
> Step 8: count how many parameters the capability needs, by scanning it
> for '%' escapes in the termcap parameter grammar. Each of `%d`, `%2`,
> `%3`, `%.` and `%+` increments the required count. `%%`, `%>`, `%i`,
> `%r`, `%n`, `%B` and `%D` require no parameter. Anything else is
> unrecognised: if verbose, warn "echotc: Warning: unknown termcap %%
> `%c'.\n" on `el->el_errfile`; otherwise ignore it silently. The scan
> reads the character after '%' unconditionally, so a capability that
> ends in a bare '%' reads the terminating NUL and the loop then steps
> one position past it, reading out of bounds — undefined behaviour in
> the C. The port must stop the scan at the NUL.
> Step 9: dispatch on the count.
> Count 0: consume one more argument; if a non-empty extra argument is
> present, warn (unless silent) "echotc: Warning: Extra argument `%ls'.\n"
> and return -1. Otherwise emit the capability unchanged via
> `terminal_tputs` with an affected-line count of 1.
> Count 1: consume one argument; if it is absent or empty, warn
> "echotc: Warning: Missing argument.\n" and return -1. Parse it as a
> base-10 wide integer; if any character is left unconsumed, or the value
> is negative, warn "echotc: Bad value `%ls' for rows.\n" and return -1.
> That value becomes the row parameter and the column parameter is forced
> to 0. Consume one more argument; a non-empty extra argument warns
> "Extra argument" and returns -1. Emit the capability expanded with
> (column = 0, row = the parsed value) and an affected-line count of 1.
> Note that the single user-supplied value is therefore passed as the
> *second* expansion parameter, not the first.
> Count 2 — and, by fall-through, any count greater than 2, which first
> warns "echotc: Warning: Too many required arguments (%d).\n" when
> verbose: consume one argument for the column value, diagnosing an
> absent or empty argument as "Missing argument" and a trailing-garbage
> or negative value as "echotc: Bad value `%ls' for cols.\n", each
> returning -1. Then consume one argument for the row value with the same
> treatment, diagnosed as "... for rows.". (The C re-tests the same parse
> result a third time here; that test can never fire and is dead code.)
> Consume one more argument; a non-empty extra argument warns and returns
> -1. Emit the capability expanded with (column, row) and an
> affected-line count equal to the row value — which can be 0, zeroing
> any per-affected-line padding.
> Step 10: return 0.
> Every diagnostic phrased as a "Warning:" nonetheless returns -1; the
> wording is historical. The `-s` flag suppresses the messages but does
> not change any return value.
> The expansion helper takes its parameters in (column, row) order, which
> is the C `tgoto` argument order and the reverse of terminfo's own
> two-parameter cursor-addressing convention; see the tgoto rule.

> [spec:libedit:def:terminal.terminal-end-fn]
> libedit_private void terminal_end(EditLine *el)

> [spec:libedit:sem:terminal.terminal-end-fn]
> Releases everything `terminal_init` allocated, in this order: free and
> NULL the capability string pool `t_buf`; free and NULL the
> terminal-database scratch buffer `t_cap`; set `t_loc` to 0; free and
> NULL the string-capability pointer array `t_str`; free and NULL the
> capability value array `t_val`; free and NULL the function-key table
> `t_fkey`; then call `terminal_free_display`, which frees `el_display`
> and `el_vdisplay` and NULLs both.
> Every step is safe on a partially initialised structure — the free
> routine tolerates NULL and `terminal_free_buffer` checks for it — which
> is what allows `terminal_init` to use this function as its error path.
> It does not clear `t_name`, which still points at a string the
> structure never owned, and does not clear `t_size` or `t_flags`.
> No return value.

> [spec:libedit:def:terminal.terminal-flush-fn]
> libedit_private void terminal__flush(EditLine *el)

> [spec:libedit:sem:terminal.terminal-flush-fn]
> Flushes `el->el_outfile`, pushing everything the terminal routines have
> buffered out to the device. The flush's own result is discarded, so a
> write error at this point is not reported to anyone. Nothing else
> happens: no cursor state changes, no capability is consulted.

> [spec:libedit:def:terminal.terminal-free-buffer-fn]
> static void terminal_free_buffer(wint_t ***bp)

> [spec:libedit:sem:terminal.terminal-free-buffer-fn]
> Frees one screen-image buffer, given the address of the field holding
> it.
> Step 1: if the field is NULL, return — freeing is idempotent.
> Step 2: take a local copy of the buffer pointer and immediately store
> NULL through `bp`, so the caller's field is cleared before any memory
> is released and a re-entrant or repeated call becomes a no-op.
> Step 3: walk the row-pointer array from index 0 until a NULL entry is
> found, freeing each row. The walk is terminated by the NULL that
> `terminal_alloc_buffer` placed at index `t_size.v`; it does not consult
> `t_size` at all, so it stays correct even if the recorded size has
> already been changed to the new dimensions.
> Step 4: free the row-pointer array itself.

> [spec:libedit:def:terminal.terminal-free-display-fn]
> static void terminal_free_display(EditLine *el)

> [spec:libedit:sem:terminal.terminal-free-display-fn]
> Frees both screen images: calls the buffer free routine on the address
> of `el->el_display` and then on the address of `el->el_vdisplay`,
> releasing each and setting both fields to NULL. Safe to call when
> either or both are already NULL, and therefore safe to call twice.

> [spec:libedit:def:terminal.terminal-get-fn]
> libedit_private void terminal_get(EditLine *el, const char **term)

> [spec:libedit:sem:terminal.terminal-get-fn]
> Stores `el->el_terminal.t_name` into `*term` and returns nothing.
> That field is the terminal type name last resolved by `terminal_set`.
> It is a borrowed pointer that libedit neither owns nor copies: it may
> point into the process environment (the value of TERM), at the static
> literal "dumb", or at a buffer the caller of `terminal_set` supplied.
> Its lifetime is entirely the caller's problem, and a later
> `terminal_set` replaces it. No NULL check on `term`, no copy, no error
> return.

> [spec:libedit:def:terminal.terminal-get-size-fn]
> libedit_private int terminal_get_size(EditLine *el, int *lins, int *cols)

> [spec:libedit:sem:terminal.terminal-get-size-fn]
> Reports the terminal's current window size and whether it differs from
> what libedit has recorded. It reports only; it does not apply anything.
> Step 1: seed `*cols` from the recorded column count (terminfo
> `columns`; termcap `co`) and `*lins` from the recorded line count
> (terminfo `lines`; termcap `li`). These are the values loaded from the
> terminal database or set by a previous resize, and they are what is
> returned if the kernel cannot be asked.
> Step 2: if the platform provides TIOCGWINSZ, issue it on
> `el->el_infd`. On success, overwrite `*cols` with the reported column
> count if that value is non-zero, and `*lins` with the reported row
> count if that value is non-zero. A zero field means "the kernel does
> not know" and leaves the seeded value in place.
> Step 3: if the platform provides TIOCGSIZE (the older BSD ioctl with
> `struct ttysize`), do the same with its column and row fields. Where a
> platform has both ioctls, this one runs second and therefore wins.
> Step 4: return non-zero if the recorded column count differs from
> `*cols` or the recorded line count differs from `*lins` — meaning "the
> size changed" — and 0 otherwise.
> An ioctl failure is silently ignored; errno is not inspected.
> Callers that want the new size applied pass the two outputs to
> `terminal_change_size`.
> This is the only place the terminal layer touches the tty device
> directly, and it queries the *input* descriptor, not the output one.

> [spec:libedit:def:terminal.terminal-gettc-fn]
> libedit_private int /*ARGSUSED*/ terminal_gettc(EditLine *el, int argc __attribute__((__unused__)), char **argv)

> [spec:libedit:sem:terminal.terminal-gettc-fn]
> Implements `el_get(EL_GETTC, ...)`: read back one capability value.
> `argc` is ignored. Note that `argv` here is an array of *narrow*
> strings, unlike `terminal_settc`. `argv[1]` is the capability name;
> `argv[2]` is not a string at all but a caller-supplied destination
> pointer smuggled through the array, and the type it points to depends
> on which capability was named.
> Step 1: if `argv`, `argv[1]` or `argv[2]` is NULL, return -1.
> Step 2: search the 39-entry string-capability table for an exact match
> on the name. On a match, store the corresponding slot of
> `el->el_terminal.t_str` through `argv[2]` treated as `char **`, and
> return 0. The caller receives libedit's own interior pointer, which may
> be NULL when the capability is absent, and which is invalidated by any
> later `terminal_set` or `terminal_settc`.
> Step 3: otherwise search the 8-entry flag/numeric table. If the name is
> in neither table, return -1 with nothing stored.
> Step 4: if the matched entry is one of the four the code treats as
> boolean — physical tabs (termcap `pt`), meta key (terminfo
> `has_meta_key`; termcap `km`), auto margins (terminfo
> `auto_right_margin`; termcap `am`) or magic margins (terminfo
> `eat_newline_glitch`, capname `xenl`; termcap `xn`) — store through
> `argv[2]` treated as `char **` a pointer to the static string "yes" if
> the slot is non-zero and "no" otherwise, then return 0. Both strings
> are statically allocated and remain valid after the call.
> Step 5: for every other entry — the line count (terminfo `lines`;
> termcap `li`), the column count (terminfo `columns`; termcap `co`), the
> destructive-tabs flag (termcap `xt`) and the MT meta flag (termcap-only)
> — store the raw integer from the value table through `argv[2]` treated
> as `int *`, and return 0. Note that the last two are booleans by nature
> but are reported as integers here, matching `terminal_settc`.

> [spec:libedit:def:terminal.terminal-init-arrow-fn]
> static void terminal_init_arrow(EditLine *el)

> [spec:libedit:sem:terminal.terminal-init-arrow-fn]
> Fills the seven-entry function-key table `el->el_terminal.t_fkey` with
> libedit's built-in arrow and edit-key defaults. Each entry receives four
> fields: a wide name (the handle used by set/clear/print arrow), the
> index of the capability whose string is that key's escape sequence, a
> default editor command, and the type XK_CMD.
> Slot A_K_DN (0): name L"down", capability key_down (terminfo `kcud1`;
> termcap `kd`), command ED_NEXT_HISTORY.
> Slot A_K_UP (1): name L"up", capability key_up (terminfo `kcuu1`;
> termcap `ku`), command ED_PREV_HISTORY.
> Slot A_K_LT (2): name L"left", capability key_left (terminfo `kcub1`;
> termcap `kl`), command ED_PREV_CHAR.
> Slot A_K_RT (3): name L"right", capability key_right (terminfo `kcuf1`;
> termcap `kr`), command ED_NEXT_CHAR.
> Slot A_K_HO (4): name L"home", capability key_home (terminfo `khome`;
> termcap `kh`), command ED_MOVE_TO_BEG.
> Slot A_K_EN (5): name L"end", capability key_end (terminfo `kend`;
> termcap `@7`), command ED_MOVE_TO_END.
> Slot A_K_DE (6): name L"delete", capability key_dc (terminfo `kdch1`;
> termcap `kD`), command ED_DELETE_NEXT_CHAR.
> The function only fills the table; nothing reaches a key map until
> `terminal_bind_arrow` runs. It is called once, from `terminal_init`,
> and — see that rule — it runs *after* the first `terminal_set`, so the
> first `terminal_bind_arrow` sees an all-zero table.

> [spec:libedit:def:terminal.terminal-init-fn]
> libedit_private int terminal_init(EditLine *el)

> [spec:libedit:sem:terminal.terminal-init-fn]
> Brings up the terminal subsystem for a fresh EditLine. Every allocation
> is zero-filled.
> Step 1: allocate the capability string pool `el_terminal.t_buf`,
> TC_BUFSIZE (2048) bytes. On failure return -1 immediately with no
> cleanup at all — asymmetric with every later failure, and harmless only
> because nothing has been allocated yet.
> Step 2: allocate the terminal-database scratch buffer
> `el_terminal.t_cap`, also TC_BUFSIZE bytes. On failure take the cleanup
> path. This buffer exists purely because the C `tgetent` copies the
> terminal's whole raw entry into a caller-supplied area; libedit never
> reads it. A terminfo-based capability module owns its own storage, so
> the port has no reason to keep the field.
> Step 3: allocate the function-key table `el_terminal.t_fkey`, A_K_NKEYS
> (7) entries. On failure take the cleanup path.
> Step 4: set `el_terminal.t_loc` to 0 — the string pool is empty.
> Step 5: allocate the string-capability pointer array
> `el_terminal.t_str`, T_str (39) slots. On failure take the cleanup path.
> Step 6: allocate the capability value array `el_terminal.t_val`, T_val
> (8) integers. On failure take the cleanup path.
> Step 7: call `terminal_set(el, NULL)` to load capabilities for the
> TERM environment variable, discarding its result — a missing or unknown
> terminal type does not make initialisation fail, because
> `terminal_set` installs dumb-terminal defaults in that case.
> Step 8: call `terminal_init_arrow` to fill the function-key table with
> defaults.
> Step 9: return 0.
> Cleanup path: call `terminal_end`, then return -1.
> The ordering matters and is subtly wrong. `t_str`, `t_val` and `t_fkey`
> must all exist before step 7, because `terminal_set` writes the first
> two and calls `terminal_bind_arrow`, which reads the third. But step 8
> runs *after* step 7, so that first `terminal_bind_arrow` sees a
> zero-filled function-key table: every entry has a NULL name, capability
> index 0 (which is the add-blank-line capability, terminfo `insert_line`
> / termcap `al`) and type XK_CMD (0, not XK_NOD). In the shipped library
> this is inert only because `terminal_init` runs before `map_init`, so
> `terminal_bind_arrow`'s "is the key map built yet" guard fires first.
> The port should install the arrow defaults before the first capability
> load rather than rely on that.

> [spec:libedit:def:terminal.terminal-insertwrite-fn]
> libedit_private void terminal_insertwrite(EditLine *el, wchar_t *cp, int num)

> [spec:libedit:sem:terminal.terminal-insertwrite-fn]
> Inserts `num` columns' worth of characters from `cp` at the cursor,
> pushing the rest of the line right. `cp` is assumed to be
> column-padded: a double-width character is followed by MB_FILL_CHAR
> cells so that `num` counts screen columns rather than code points.
> Early returns, in order: if `num <= 0`, return; if the TERM_CAN_INSERT
> flag is clear, return with nothing emitted (under DEBUG_EDIT the C also
> prints an error line); if `num` is greater than `el_terminal.t_size.h`,
> return.
> Then the first applicable strategy is used.
> Strategy A — parameterised insert. If the capability (terminfo
> `parm_ich`, capname `ich`; termcap `IC`) is present and non-empty, and
> either `num` is greater than 1 or the single-character insert (terminfo
> `insert_character`, capname `ich1`; termcap `ic`) is absent or empty:
> expand it with `num` passed as both parameters and emit it with an
> affected-line count of `num`, opening `num` blank columns; then call
> `terminal_overwrite(el, cp, num)` to write the characters into the
> hole. `terminal_overwrite` is what advances `el_cursor.h` and applies
> the auto/magic margin rules. Return.
> Strategy B — insert mode. If both enter-insert-mode (terminfo
> `enter_insert_mode`, capname `smir`; termcap `im`) and exit-insert-mode
> (terminfo `exit_insert_mode`, capname `rmir`; termcap `ei`) are present
> and non-empty: emit enter-insert-mode with an affected-line count of 1;
> add `num` to `el_cursor.h` up front; write all `num` cells with
> `terminal__putc`, which emits nothing for MB_FILL_CHAR cells but they
> were already counted; then, if the insert-padding capability (terminfo
> `insert_padding`, capname `ip`; termcap `ip`) is present and non-empty,
> emit it **once** for the whole run; then emit exit-insert-mode with an
> affected-line count of 1. Return. This path performs no wrap handling
> at all — unlike `terminal_overwrite`, it will let the recorded column
> run past `t_size.h` without applying the margin rules.
> Strategy C — one character at a time. For each of the `num` cells: emit
> the single-character insert capability if it is present and non-empty
> (count 1); write the cell with `terminal__putc`; increment
> `el_cursor.h` by one; emit the insert-padding capability if it is
> present and non-empty (count 1). Note the padding is emitted per
> character here, unlike strategy B, which emits it once.
> If neither the single-character insert nor a complete insert mode
> exists — reachable when TERM_CAN_INSERT was set by enter-insert-mode
> alone, without a matching exit-insert-mode — strategy C degenerates
> into a plain overwrite with no insertion at all.

> [spec:libedit:def:terminal.terminal-move-to-char-fn]
> libedit_private void terminal_move_to_char(EditLine *el, int where)

> [spec:libedit:sem:terminal.terminal-move-to-char-fn]
> Moves the cursor to column `where` (0-based) on the current row as
> cheaply as the terminal allows, keeping `el->el_cursor.h` in step. The
> whole body is a loop target; one path restarts it from the top.
> Step 1: if `where` equals `el_cursor.h`, return — already there.
> Step 2: if `where` is greater than `el_terminal.t_size.h`, return with
> nothing emitted and `el_cursor` unchanged. The test is strictly
> greater, so a column equal to `t_size.h` — one past the last real
> column — is accepted; contrast `terminal_move_to_line`, which uses `>=`.
> Step 3: if `where` is 0, write a carriage return with `terminal__putc`,
> set `el_cursor.h` to 0, and return.
> Step 4: let `del` be `where - el_cursor.h`; positive means rightwards.
> Step 5 — direct column addressing: if the distance exceeds 4 in either
> direction (`del < -4` or `del > 4`) and the column-address capability
> (terminfo `column_address`, capname `hpa`; termcap `ch`) is present and
> non-empty, expand it with `where` passed as **both** parameters and
> emit it with an affected-line count of `where`. Passing the value twice
> is what the C does; a one-parameter capability consumes only the first.
> Go to step 8.
> Step 6 — moving right (`del > 0`), when step 5 did not apply:
> (a) if `del > 4` and the parameterised right capability (terminfo
> `parm_right_cursor`, capname `cuf`; termcap `RI`) is present and
> non-empty, expand it with `del` as both parameters and emit it with an
> affected-line count of `del`.
> (b) otherwise, optionally tab first. If the TERM_CAN_TAB flag is set,
> and the current column's 8-column tab-stop group differs from the
> target's — the C compares `el_cursor.h & 0370` against `where & ~0x7`
> — and the display cell at row `el_cursor.v`, column `where & 0370` is
> not MB_FILL_CHAR (i.e. the tab stop we would land on is not the
> interior of a double-width character), then emit a TAB for every
> 8-column step from `el_cursor.h & 0370` up to but excluding
> `where & ~0x7`, and set `el_cursor.h` to `where & ~0x7`. The two masks
> are not the same operation: `0370` is 0xF8 and also clears every bit
> above bit 7, whereas `~0x7` does not, so for columns of 256 or more
> both the comparison and the display index are wrong. The port should
> use "clear the low three bits" consistently.
> (c) then write the remaining columns literally, by calling
> `terminal_overwrite` on `&el_display[el_cursor.v][el_cursor.h]` for
> `where - el_cursor.h` cells. Re-emitting characters already believed to
> be on screen is normally cheaper than a cursor-motion sequence. Note
> that `terminal_overwrite` mutates `el_cursor.h`, and that it returns
> without writing anything if the count exceeds `t_size.h`.
> Step 7 — moving left (`del < 0`), when step 5 did not apply:
> (a) if `-del > 4` and the parameterised left capability (terminfo
> `parm_left_cursor`, capname `cub`; termcap `LE`) is present and
> non-empty, expand it with `-del` as both parameters and emit it with an
> affected-line count of `-del`.
> (b) otherwise compare the cost of backspacing against the cost of
> returning to column 0 and coming back out. With tabs available the
> return trip costs `where >> 3` tabs plus `where & 7` single steps;
> without tabs it costs `where` steps. If the backspace count `-del`
> exceeds that cost — the comparison is performed in unsigned arithmetic
> — write a carriage return, set `el_cursor.h` to 0, and restart at step
> 1, which now takes the rightward path.
> (c) otherwise write `-del` backspace characters (0x08).
> Step 8: unconditionally set `el_cursor.h` to `where`, overriding
> whatever `terminal_overwrite` or the tab loop left there.
> Because step 8 is unconditional, the recorded column becomes
> authoritative even on the paths that emitted nothing — notably when
> `terminal_overwrite` bailed out on an oversized count, which leaves the
> model claiming a position the cursor never reached.
> The routine reads `el_display` at the *current* row, so it relies on
> the file-wide invariant that the recorded screen image and the recorded
> cursor position are both correct on entry.

> [spec:libedit:def:terminal.terminal-move-to-line-fn]
> libedit_private void terminal_move_to_line(EditLine *el, int where)

> [spec:libedit:sem:terminal.terminal-move-to-line-fn]
> Moves the cursor to screen row `where` (0-based, first line is 0) as
> efficiently as possible, keeping `el->el_cursor.v` in step.
> Step 1: if `where` equals `el_cursor.v`, return.
> Step 2: if `where` is greater than or equal to `el_terminal.t_size.v`,
> return with nothing emitted and `el_cursor` unchanged (under
> DEBUG_SCREEN the C prints "where is ridiculous"). Note this bound is
> `>=`, unlike the `>` used by `terminal_move_to_char`.
> Step 3: let `del` be `where - el_cursor.v`.
> Step 4 — moving down (`del > 0`): write `del` newline characters with
> `terminal__putc`, then set `el_cursor.h` to 0. The parameterised down
> capability (terminfo `parm_down_cursor`, capname `cud`; termcap `DO`)
> is deliberately *not* used, because some terminals misbehave when the
> destination is below the bottom of the screen. The column is reset
> because the tty's output post-processing turns each `\n` into CR LF,
> so the cursor also returns to column 0.
> Step 5 — moving up (`del < 0`):
> (a) if the parameterised up capability (terminfo `parm_up_cursor`,
> capname `cuu`; termcap `UP`) is present and non-empty, and either the
> distance is more than one row or the single-row up capability (terminfo
> `cursor_up`, capname `cuu1`; termcap `up`) is absent or empty: expand
> the parameterised capability with `-del` passed as both parameters and
> emit it with an affected-line count of `-del`.
> (b) otherwise, if the single-row up capability is present and
> non-empty, emit it `-del` times, each with an affected-line count of 1.
> (c) if neither is available, nothing at all is emitted — the cursor
> does not move, yet step 6 updates the model anyway, desynchronising it
> from the screen. This is why the TERM_CAN_UP flag exists and why
> callers avoid upward motion on such terminals.
> Step 6: set `el_cursor.v` to `where`.
> Note the asymmetry: downward motion also resets the recorded column to
> 0, upward motion leaves the recorded column alone.

> [spec:libedit:def:terminal.terminal-overwrite-fn]
> libedit_private void terminal_overwrite(EditLine *el, const wchar_t *cp, size_t n)

> [spec:libedit:sem:terminal.terminal-overwrite-fn]
> Writes `n` columns' worth of characters at the cursor, overstriking
> whatever is there, and updates `el_cursor` to the resulting position
> including any wrap. The input is assumed to be column-padded:
> double-width characters are followed by MB_FILL_CHAR cells so that `n`
> counts screen columns rather than code points.
> Step 1: if `n` is 0, return.
> Step 2: if `n` is greater than `el_terminal.t_size.h`, return without
> writing anything. This is a sanity guard and also catches a negative
> count that arrived as an enormous unsigned value.
> Step 3: for each of the `n` cells in turn, call `terminal__putc`, which
> writes the character but emits nothing for MB_FILL_CHAR, and increment
> `el_cursor.h` by one regardless of which case applied. Incrementing on
> the fill cells too is exactly how the column count stays honest across
> double-width characters.
> Step 4: if `el_cursor.h` is now greater than or equal to
> `el_terminal.t_size.h`, the write reached or crossed the right margin.
> (a) If the TERM_HAS_AUTO_MARGINS flag is set — the terminal wraps by
> itself — set `el_cursor.h` to 0, and if `el_cursor.v + 1` is less than
> `el_terminal.t_size.v`, increment `el_cursor.v`; on the last row the
> row number is left alone (clamped).
> (b) Additionally, if the TERM_HAS_MAGIC_MARGINS flag is also set — the
> terminal defers the wrap until the next character arrives, the terminfo
> `eat_newline_glitch` behaviour — force the wrap now so the deferred
> state cannot confuse later cursor motion. Read the display cell at the
> new position, row `el_cursor.v` column 0. If it is non-zero, recurse
> into this same routine to write that one character (which emits it and
> leaves the recorded column at 1), then advance `el_cursor.h` past any
> immediately following MB_FILL_CHAR cells in that row. If the cell is
> zero — the row is blank there — write a single space with
> `terminal__putc` and set `el_cursor.h` to 1.
> The cell is read as a `wint_t` and stored into a `wchar_t` before the
> recursive call, so on a platform where `wchar_t` is narrower the value
> is truncated. The recursion terminates only because a single-character
> write cannot itself wrap unless the screen is one column wide.
> (c) If TERM_HAS_AUTO_MARGINS is clear, the terminal does not wrap and
> the cursor stays pinned at the last column: set `el_cursor.h` to
> `el_terminal.t_size.h - 1`.
> Nothing is written into `el_display` here. This routine emits bytes and
> tracks the cursor; the caller owns the screen model.

> [spec:libedit:def:terminal.terminal-print-arrow-fn]
> libedit_private void terminal_print_arrow(EditLine *el, const wchar_t *name)

> [spec:libedit:sem:terminal.terminal-print-arrow-fn]
> Prints the current arrow/edit key bindings in the editrc-readable form.
> For each of the seven function-key table entries, in table order: if
> `name` is the empty wide string — meaning "print them all" — or matches
> that entry's name exactly, and the entry's type is not XK_NOD (i.e. the
> binding has not been cleared), call `keymacro_kprint` with the entry's
> name, its bound function value and its type, which renders one
> descriptive line.
> A name matching nothing produces no output. There is no return value
> and no diagnostic for an unknown name.

> [spec:libedit:def:terminal.terminal-putc-fn]
> libedit_private int terminal__putc(EditLine *el, wint_t c)

> [spec:libedit:sem:terminal.terminal-putc-fn]
> Writes one editor character to `el->el_outfile`. This is the single
> choke point through which every non-capability byte leaves the terminal
> layer.
> Step 1: if `c` is MB_FILL_CHAR — the sentinel `(wint_t)-1` used as
> column padding after a double-width character — write nothing and
> return 0. This is what lets the callers count columns while the byte
> stream stays correct.
> Step 2: if `c` has the EL_LITERAL bit (0x80000000) set, it is a handle
> into the literal-string table rather than a character: look the string
> up with `literal_get` and write it verbatim. Literals are byte
> sequences, typically prompt escape sequences, that occupy no columns
> and must not be re-encoded. Return the write's result.
> Step 3: otherwise encode `c` into a multibyte sequence with
> `ct_encode_char` into a buffer of MB_LEN_MAX bytes. If the encoder
> returns a value less than or equal to 0 — unencodable in the current
> locale, or an empty result — return that value unchanged, having
> written nothing.
> Step 4: NUL-terminate the encoded bytes at the returned length and
> write the resulting string.
> The return value is therefore one of: 0 for a skipped fill character;
> the encoder's non-positive result for an unencodable character; or the
> underlying string write's result, which is non-negative on success and
> EOF on failure.

> [spec:libedit:def:terminal.terminal-rebuffer-display-fn]
> static int terminal_rebuffer_display(EditLine *el)

> [spec:libedit:sem:terminal.terminal-rebuffer-display-fn]
> Rebuilds both screen images after the screen size changed.
> Step 1: call `terminal_free_display`, releasing `el_display` and
> `el_vdisplay` and setting both to NULL. The old contents are discarded,
> not carried over.
> Step 2: set `el_terminal.t_size.h` from the recorded column count
> (terminfo `columns`; termcap `co`) and `el_terminal.t_size.v` from the
> recorded line count (terminfo `lines`; termcap `li`). This is the
> assignment that gives `t_size` its correct meaning — `.h` columns, `.v`
> rows. Both `terminal_set` and `terminal_settc` write `t_size` with the
> two fields swapped just before reaching here, and this step is what
> silently repairs them.
> Step 3: call `terminal_alloc_display`; if it returns -1, return -1,
> leaving both images NULL.
> Step 4: return 0.
> The caller (`terminal_change_size`) follows up with `re_clear_display`,
> since the freshly allocated images are blank and no longer describe
> what is on the screen.

> [spec:libedit:def:terminal.terminal-reset-arrow-fn]
> static void terminal_reset_arrow(EditLine *el)

> [spec:libedit:sem:terminal.terminal-reset-arrow-fn]
> Installs libedit's hard-coded default escape sequences for the arrow
> and home/end keys, independently of anything the terminal database
> says. Each sequence is bound with `keymacro_add` to the *current*
> function value and type of the corresponding function-key table slot,
> so a prior `terminal_set_arrow` or `terminal_clear_arrow` is honoured.
> Twelve sequences are added, in this order:
> ESC `[` `A` to the up slot; ESC `[` `B` to down; ESC `[` `C` to right;
> ESC `[` `D` to left; ESC `[` `H` to home; ESC `[` `F` to end — the CSI
> forms;
> then ESC `O` `A` to up; ESC `O` `B` to down; ESC `O` `C` to right;
> ESC `O` `D` to left; ESC `O` `H` to home; ESC `O` `F` to end — the SS3
> forms a terminal in application-cursor-key mode sends.
> Then, only if `el->el_map.type` is MAP_VI, the same twelve are added a
> second time with the leading ESC removed: "[A", "[B", "[C", "[D", "[H",
> "[F", "OA", "OB", "OC", "OD", "OH", "OF". In vi command mode the ESC is
> consumed as the mode switch and the remainder of the sequence arrives
> on its own, so it must also be bound bare. In emacs mode the function
> returns after the first twelve.
> The delete slot (A_K_DE) receives no hard-coded sequence at all; it is
> only ever bound from the terminal's own key_dc capability, by
> `terminal_bind_arrow`.

> [spec:libedit:def:terminal.terminal-set-arrow-fn]
> libedit_private int terminal_set_arrow(EditLine *el, const wchar_t *name, keymacro_value_t *fun, int type)

> [spec:libedit:sem:terminal.terminal-set-arrow-fn]
> Rebinds one named function key.
> Scan the seven function-key table entries in order, comparing `name`
> for exact wide-string equality with each entry's name. On the first
> match, copy the value pointed to by `fun` into that entry's function
> field, store `type` in its type field, and return 0. If no entry has
> that name, return -1 and change nothing.
> The recognised names are "down", "up", "left", "right", "home", "end",
> "delete".
> The change updates only the table. It does not reach the key map until
> `terminal_bind_arrow` next runs, which is what re-derives both the
> hard-coded default sequences and the terminal's own key capability
> sequences from the table.

> [spec:libedit:def:terminal.terminal-set-fn]
> libedit_private int terminal_set(EditLine *el, const char *term)

> [spec:libedit:sem:terminal.terminal-set-fn]
> Loads the capability set for a terminal type and reconfigures
> everything that depends on it.
> Step 1: block SIGWINCH for the duration — build a signal set containing
> only SIGWINCH, block it, and save the previous mask — so that a resize
> cannot arrive while the capability tables and the screen images are
> inconsistent.
> Step 2: resolve the terminal name. If `term` is NULL, take it from
> `el->el_getenv("TERM")`. If the result is NULL or the empty string, use
> the literal "dumb".
> Step 3: if the resolved name is exactly "emacs", set EDIT_DISABLED in
> `el->el_flags` — this is the Emacs inferior-shell terminal type, where
> line editing must be off.
> Step 4: zero the terminal-database scratch buffer `el_terminal.t_cap`
> and load the entry for the resolved name (the C's `tgetent`; in the
> port, the capability module's "open this terminal type" operation). Its
> result is greater than 0 when an entry was found, 0 when the database
> is readable but has no entry for that type, and -1 when the database
> itself cannot be read. All three are distinguished below.
> Step 5: if the result is 0 or -1, fall back to dumb-terminal settings.
> (a) Diagnose on `el->el_errfile`: for -1 print "Cannot read termcap
> database;\n"; for 0 print "No entry for terminal type \"%s\";\n" with
> the resolved name; then in both cases print "using dumb terminal
> settings.\n".
> (b) Set the column count to 80, and set the physical-tabs value, the
> meta-key value and the line count all to 0. Then set the
> destructive-tabs value from the MT value — a straight copy between two
> unrelated slots, which on a freshly zeroed EditLine simply leaves both
> at 0 and is best read as "both false". The auto-margin and magic-margin
> values are deliberately left at whatever they already held.
> (c) Clear every string slot, by calling `terminal_alloc` with a NULL
> capability for each of the 39 table entries.
> Step 6: otherwise read the capabilities from the loaded entry.
> (a) Flags, in this order: auto margins (terminfo `auto_right_margin`;
> termcap `am`), magic margins (terminfo `eat_newline_glitch`, capname
> `xenl`; termcap `xn`), physical tabs (termcap `pt` — no clean terminfo
> counterpart, see the tgetflag rule), destructive tabs (termcap `xt`),
> meta key (terminfo `has_meta_key`; termcap `km`), and the MT meta flag
> (termcap-only, no terminfo counterpart).
> (b) Numbers: the column count (terminfo `columns`; termcap `co`) and
> the line count (terminfo `lines`; termcap `li`). An absent number reads
> back as -1, not 0.
> (c) Strings: for each of the 39 string-capability table entries, look
> the capability up and intern the result with `terminal_alloc`; an
> absent capability interns as NULL. The C passes a 2048-byte stack
> buffer as the lookup's scratch arena and every result is copied out by
> `terminal_alloc` before the function returns, so nothing outlives the
> call.
> Step 7: clamp. If the column count is less than 2, set it to 80; if the
> line count is less than 1, set it to 24. This is what converts the
> "absent" -1 from step 6(b) into a usable default, and it applies on
> both the success and fallback paths.
> Step 8: store the size into `el_terminal.t_size` — with the two fields
> **swapped**: the C writes the column count into `.v` and the line count
> into `.h`, the reverse of the meaning used everywhere else in the file.
> The mistake is masked because step 10 reaches
> `terminal_rebuffer_display`, which overwrites both fields correctly
> before anything reads them. The port should simply not do this.
> Step 9: call `terminal_setflags` to derive `el_terminal.t_flags` from
> the freshly loaded capabilities.
> Step 10: call `terminal_get_size` to consult the kernel for the real
> window size (its "did it change" result is ignored), then call
> `terminal_change_size` with those lines and columns, which re-clamps,
> reallocates both screen images and clears the display model. If
> `terminal_change_size` returns -1, return -1 immediately — **without
> restoring the signal mask**, so SIGWINCH stays blocked for the rest of
> the process. That is a bug; the port must restore the mask on every
> exit path, or arrange not to need a mask at all.
> Step 11: restore the saved signal mask.
> Step 12: call `terminal_bind_arrow` to reinstall the arrow key
> bindings from the new capabilities.
> Step 13: store the resolved name in `el_terminal.t_name`, keeping the
> pointer without copying it — it may point into the environment, at the
> static literal "dumb", or at the caller's own buffer.
> Step 14: return 0 if the database lookup in step 4 succeeded, and -1 if
> it did not — that is, -1 is returned even though dumb-terminal defaults
> were installed successfully and the EditLine is fully usable.

> [spec:libedit:def:terminal.terminal-setflags-fn]
> static void terminal_setflags(EditLine *el)

> [spec:libedit:sem:terminal.terminal-setflags-fn]
> Recomputes `el_terminal.t_flags` from the capability tables. Called
> after every capability load and after every string or boolean change
> made through `terminal_settc`.
> Throughout, "present" means the string slot is neither NULL nor a
> zero-length string; an interned empty string counts as absent.
> Start the accumulator at 0, then:
> TERM_CAN_TAB (0x008) is set only if `el->el_tty.t_tabs` is true — the
> tty layer reports that hardware tab expansion is enabled — *and* the
> physical-tabs value is non-zero *and* the destructive-tabs value is
> zero. When `t_tabs` is false the capability values are not even
> consulted. Note that during the library's first initialisation pass the
> tty layer has not run yet and `t_tabs` is 0, so this flag starts clear.
> TERM_HAS_META (0x040) is set if either the meta-key value (terminfo
> `has_meta_key`; termcap `km`) or the MT value (termcap-only) is
> non-zero.
> TERM_CAN_CEOL (0x004) is set if the clear-to-end-of-line string
> (terminfo `clr_eol`, capname `el`; termcap `ce`) is present.
> TERM_CAN_DELETE (0x002) is set if either the single-character delete
> (terminfo `delete_character`, capname `dch1`; termcap `dc`) or the
> parameterised delete (terminfo `parm_dch`, capname `dch`; termcap `DC`)
> is present.
> TERM_CAN_INSERT (0x001) is set if any of enter-insert-mode (terminfo
> `enter_insert_mode`, capname `smir`; termcap `im`), single-character
> insert (terminfo `insert_character`, capname `ich1`; termcap `ic`) or
> parameterised insert (terminfo `parm_ich`, capname `ich`; termcap `IC`)
> is present.
> TERM_CAN_UP (0x020) is set if either single-row up (terminfo
> `cursor_up`, capname `cuu1`; termcap `up`) or parameterised up
> (terminfo `parm_up_cursor`, capname `cuu`; termcap `UP`) is present.
> TERM_HAS_AUTO_MARGINS (0x080) is set if the auto-margin value (terminfo
> `auto_right_margin`; termcap `am`) is non-zero.
> TERM_HAS_MAGIC_MARGINS (0x100) is set if the magic-margin value
> (terminfo `eat_newline_glitch`, capname `xenl`; termcap `xn`) is
> non-zero.
> TERM_CAN_ME (0x010) — "one sequence turns every attribute off" — is
> then decided by two independent byte-for-byte string comparisons:
> first, if both exit-attributes (terminfo `exit_attribute_mode`, capname
> `sgr0`; termcap `me`) and exit-underline (terminfo
> `exit_underline_mode`, capname `rmul`; termcap `ue`) are present, set
> the flag when the two strings are identical, and otherwise (when either
> is absent) explicitly clear it — a no-op, since the accumulator started
> at 0 and nothing has set the bit yet;
> second, independently, if both exit-attributes and exit-standout
> (terminfo `exit_standout_mode`, capname `rmso`; termcap `se`) are
> present, set the flag when those two strings are identical. This second
> test can only set the bit, never clear it, so a match on either pair is
> enough.
> Under DEBUG_SCREEN the C additionally warns on `el->el_errfile` when
> the up, clear-EOL, delete-character or insert-character capabilities
> are missing. That output is not part of a normal build and is not
> required of the port.

> [spec:libedit:def:terminal.terminal-settc-fn]
> libedit_private int /*ARGSUSED*/ terminal_settc(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv)

> [spec:libedit:sem:terminal.terminal-settc-fn]
> Implements the `settc` editrc command and `el_set(EL_SETTC, ...)`:
> override one capability by hand. `argc` is ignored; `argv[0]` is the
> command name, used only in diagnostics; `argv[1]` is the capability
> name; `argv[2]` is the new value.
> Step 1: if `argv`, `argv[1]` or `argv[2]` is NULL, return -1.
> Step 2: encode `argv[1]` and `argv[2]` to narrow strings and copy each
> into an 8-byte buffer with a truncating bounded copy. Both the name and
> the value are therefore silently cut to 7 characters. That is harmless
> for two-letter capability names and for the "yes"/"no" and numeric
> values, but it also caps the *string* form: a capability string longer
> than 7 bytes cannot be installed through this interface. The port must
> keep that limit or change it deliberately, since it is user-visible.
> Step 3: search the 39-entry string-capability table for an exact name
> match. On a match, intern the value into that slot with
> `terminal_alloc` (an empty value clears the slot to NULL), call
> `terminal_setflags` to recompute the derived flags, and return 0.
> Step 4: otherwise search the 8-entry flag/numeric table. If the name is
> in neither table, print "%ls: Bad capability `%s'.\n" — the command
> name and the requested name — to `el->el_errfile` and return -1.
> Step 5: if the matched entry is one of the four treated as boolean —
> physical tabs (termcap `pt`), meta key (terminfo `has_meta_key`;
> termcap `km`), auto margins (terminfo `auto_right_margin`; termcap
> `am`) or magic margins (terminfo `eat_newline_glitch`, capname `xenl`;
> termcap `xn`) — accept exactly the strings "yes" (store 1) and "no"
> (store 0). Anything else prints "%ls: Bad value `%s'.\n" and returns
> -1. On success call `terminal_setflags` and return 0.
> Step 6: every other entry is treated as numeric — which includes the
> destructive-tabs flag (termcap `xt`) and the MT meta flag, both of
> which are booleans by nature yet are set here as integers. Parse the
> value with a base-10 string-to-long conversion; if any character is
> left unconsumed, print "%ls: Bad value `%s'.\n" and return -1. An empty
> value consumes nothing, leaves the terminator at the first position and
> is therefore *accepted* as 0. Store the parsed value, narrowed to int,
> in the slot.
> Step 7: if the entry was the column count, also set
> `el_terminal.t_size.v` from it; if it was the line count, also set
> `el_terminal.t_size.h` from it. Both assignments are swapped relative
> to the field meanings, exactly as in `terminal_set`, and are likewise
> overwritten by the `terminal_rebuffer_display` that step 8 reaches.
> Step 8: if either size slot was the one written, call
> `terminal_change_size` with the line count first and the column count
> second, and return -1 if it fails.
> Step 9: return 0.
> Note that `terminal_setflags` is *not* called on the numeric path, so
> changing the destructive-tabs value through `settc` does not update
> TERM_CAN_TAB until some later event recomputes the flags.

> [spec:libedit:def:terminal.terminal-telltc-fn]
> libedit_private int /*ARGSUSED*/ terminal_telltc(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv __attribute__((__unused__)))

> [spec:libedit:sem:terminal.terminal-telltc-fn]
> Implements the `telltc` editrc command and `el_set(EL_TELLTC, ...)`:
> dump the terminal's characteristics to `el->el_outfile`. Both `argc`
> and `argv` are ignored; the command takes no arguments.
> Prints, in order:
> 1. "\n\tYour terminal has the\n", then "\tfollowing characteristics:\n\n".
> 2. "\tIt has %d columns and %d lines\n" from the column count (terminfo
> `columns`; termcap `co`) and the line count (terminfo `lines`; termcap
> `li`).
> 3. "\tIt has %s meta key\n", substituting "a" when TERM_HAS_META is set
> and "no" when it is not — reading as "It has a meta key" or "It has no
> meta key".
> 4. "\tIt can%suse tabs\n", substituting " " when TERM_CAN_TAB is set
> and "not " when it is not.
> 5. "\tIt %s automatic margins\n", substituting "has" or "does not
> have" from TERM_HAS_AUTO_MARGINS.
> 6. Only when TERM_HAS_AUTO_MARGINS is set, "\tIt %s magic margins\n",
> substituting "has" or "does not have" from TERM_HAS_MAGIC_MARGINS.
> 7. One line per entry of the 39-entry string-capability table, walking
> the table and the string slots in lockstep, formatted
> "\t%25s (%s) == %s\n": the human-readable description right-aligned in
> 25 columns, the capability's short name in parentheses, and the value.
> The value is the stored string rendered visually — decoded to wide
> characters, passed through the same visual-escaping used for display,
> then re-encoded — so control characters appear in a printable form. A
> NULL or empty slot prints the literal "(empty)".
> 8. A final newline character.
> Returns 0 unconditionally.
> For the port: the parenthesised token is the C's termcap two-letter
> code, and the natural replacement is the terminfo name the port
> actually looks up. The human-readable description is libedit's own text
> ("clear to end of line", "delete multiple chars", and so on), not a
> terminfo variable name. Changing the parenthesised token changes
> user-visible output, so it is a deliberate compatibility decision
> rather than a free choice.

> [spec:libedit:def:terminal.terminal-tputs-fn]
> static void terminal_tputs(EditLine *el, const char *cap, int affcnt)

> [spec:libedit:sem:terminal.terminal-tputs-fn]
> Emits an already-expanded capability string to `el->el_outfile`,
> honouring the padding embedded in it.
> Step 1: under _REENTRANT, acquire a file-static mutex.
> Step 2: store `el->el_outfile` into a file-static `FILE *`.
> Step 3: call `tputs` with the capability string, the affected-line
> count, and a file-static one-byte callback that returns -1 when that
> `FILE *` is NULL and otherwise writes the byte to it. `tputs`'s return
> value is discarded, so write errors are invisible.
> Step 4: under _REENTRANT, release the mutex.
> The global and the mutex exist for exactly one reason: the C `tputs`
> takes an `int (*)(int)` callback with no user-data parameter, so the
> destination stream cannot be threaded through and has to be a global,
> and the mutex then serialises concurrent EditLine instances so two of
> them cannot race on it. That is an artifact of the C API, not a
> requirement on the port. A Rust padding emitter takes the writer as a
> parameter, so both the global and the lock disappear and this function
> reduces to: "expand the padding in `cap` for `affcnt` affected lines
> and write the result to this EditLine's output stream". The port must
> also make two EditLine instances on different streams safe to use
> concurrently, which the C could not guarantee.
> The `affcnt` argument feeds only the `*` (per-affected-line) form of
> the padding grammar; see the tputs rule for how it is used.

> [spec:libedit:def:terminal.terminal-writec-fn]
> libedit_private void terminal_writec(EditLine *el, wint_t c)

> [spec:libedit:sem:terminal.terminal-writec-fn]
> Writes one character out in human-readable form and flushes.
> Step 1: render `c` into a wide buffer with `ct_visual_char`, bounded to
> VISUAL_WIDTH_MAX (8) elements. That routine expands control and
> non-printable characters into a printable representation (`^X` form,
> octal escapes, and so on) and returns the number of wide characters it
> produced.
> Step 2: if the returned count is negative — the character could not be
> rendered — treat it as 0.
> Step 3: store a terminating L'\0' at that offset. The buffer is
> declared one element longer than VISUAL_WIDTH_MAX, so this is always in
> bounds even for a maximum-width rendering.
> Step 4: call `terminal_overwrite` with the buffer and the count, which
> emits the characters, advances `el_cursor` and applies the auto/magic
> margin rules. A count of 0 makes this a no-op.
> Step 5: call `terminal__flush`.
> No return value; write errors are not reported.

> [spec:libedit:def:terminal.tgetent-fn]
> extern int tgetent(char *, const char *)

> [spec:libedit:sem:terminal.tgetent-fn]
> Foreign in the C — libedit calls into ncurses/termcap for it — and ours
> in the port. This rule states the contract the Rust capability module
> must satisfy, not what ncurses does internally. See
> [dec:libedit:terminal-caps-via-term-crate].
> Contract: load the capability entry for the terminal type named by the
> second argument, and make it the entry that all subsequent capability
> lookups (`tgetstr`, `tgetflag`, `tgetnum`) resolve against.
> Return value, and libedit distinguishes all three cases: a value
> greater than 0 (the C returns 1) when an entry for that name was found
> and loaded; 0 when the capability database is readable but contains no
> entry for that type; -1 when the database itself cannot be read at all.
> `terminal_set` prints a different diagnostic for -1 and for 0, and
> installs dumb-terminal defaults for either.
> The first argument is a caller-supplied buffer of at least 1024 bytes
> (libedit passes 2048, its `el_terminal.t_cap`) into which the C
> implementation copies the terminal's raw termcap entry. libedit never
> reads it; it exists only because the historical API demanded it. The
> Rust replacement takes no such buffer, and the `t_cap` field can be
> deleted along with it.
> Lifetime: in the C the loaded entry is *process-global* state that the
> three lookup functions read. libedit depends on it persisting past the
> call — `terminal_echotc` performs a string lookup with no preceding
> load, relying on the entry installed by the last `terminal_set`. The
> port must keep the loaded entry alive for at least that long; storing
> it on the EditLine rather than in a global both satisfies the
> dependency and removes a shared-state hazard nothing depends on.
> Rust realisation: the `term` crate's terminfo loading —
> `TermInfo::from_name` / `from_env` plus the `searcher` module — reads
> the compiled terminfo entry for the given name from the
> ncurses-compatible search path ($TERMINFO, $TERMINFO_DIRS, ~/.terminfo,
> the system terminfo tree). Map "no entry for this name" to 0 and "no
> readable database, or an I/O error" to -1. Whether `term`'s searcher
> covers every database layout in use — the hashed `terminfo.db` file is
> a distinct format from the directory tree — is explicitly deferred by
> the governing decision.

> [spec:libedit:def:terminal.tgetflag-fn]
> extern int tgetflag(char *)

> [spec:libedit:sem:terminal.tgetflag-fn]
> Foreign in the C, ours in the port; this rule is the contract for the
> Rust capability module. Boolean capability lookup.
> Contract: return 1 if the entry loaded by `tgetent` defines the named
> boolean capability, and 0 if it does not, if no entry is loaded, or if
> the name is not a capability at all. There is no error return: an
> unknown capability name is indistinguishable from a false one.
> In the C the name is a termcap two-letter code; in the port it is the
> terminfo name for the same capability.
> libedit queries exactly six, all from `terminal_set`:
> `am` — auto right margin. Terminfo `auto_right_margin`, capname `am`.
> Clean mapping.
> `xn` — newline ignored at the right margin. Terminfo
> `eat_newline_glitch`, capname `xenl`. Clean mapping.
> `km` — has a meta key. Terminfo `has_meta_key`, capname `km`. Clean
> mapping.
> `pt` — has physical (hardware) tabs. **No terminfo boolean exists.**
> Terminfo expresses hardware tabbing as the *string* capability
> `tab_to_next_stop` (capname `ht`), not as a flag. On an
> ncurses-backed system the C's `tgetflag("pt")` therefore already
> returns 0 for every terminal, which means TERM_CAN_TAB is effectively
> dead in practice today. Unresolved and flagged by
> [dec:libedit:terminal-caps-via-term-crate]: the port must choose
> between "always false", which reproduces current observable behaviour,
> and "true when `tab_to_next_stop` is present", which reproduces the
> original intent and would newly enable tab-based cursor motion.
> `xt` — tabs destructive. Nominally terminfo `dest_tabs_magic_smso`
> (capname `xt`), but that capability conflates "tabs destroy the
> characters they move over" with the Teleray 1061 magic-standout quirk,
> and libedit wants only the first half. Flagged as unresolved by the
> same decision.
> `MT` — has a meta key. A termcap-only extension, historically a variant
> spelling of `km`; libedit's own table annotates it "XXX?". **No
> terminfo counterpart exists.** Under ncurses it already reads 0.
> Flagged as unresolved by the same decision; treating it as permanently
> false is the behaviour-preserving choice on any terminfo system, and
> it only ever widens TERM_HAS_META, which `km` already covers.
> Rust realisation: index the `bools` map of the loaded TermInfo by the
> terminfo capability name; a missing key is false.

> [spec:libedit:def:terminal.tgetnum-fn]
> extern int tgetnum(char *)

> [spec:libedit:sem:terminal.tgetnum-fn]
> Foreign in the C, ours in the port; this rule is the contract for the
> Rust capability module. Numeric capability lookup.
> Contract: return the value the entry loaded by `tgetent` gives for the
> named numeric capability, or **-1** if the entry does not define it, if
> the capability was cancelled in the entry, or if no entry is loaded.
> The absent marker is -1, not 0, and that matters: `terminal_set` relies
> on it, clamping a column count below 2 to 80 and a line count below 1
> to 24, which is how "absent" becomes the classic 80x24 default.
> libedit queries exactly two, both from `terminal_set`:
> `co` — number of columns. Terminfo `columns`, capname `cols`.
> `li` — number of lines. Terminfo `lines`, capname `lines`.
> Both are clean mappings.
> Note that these values are only a starting point: `terminal_get_size`
> immediately overrides them from the kernel's window size whenever the
> tty can supply one, so the database numbers matter mainly for
> non-tty output.
> Rust realisation: index the `numbers` map of the loaded TermInfo by the
> terminfo capability name, returning -1 for a missing key.

> [spec:libedit:def:terminal.tgetstr-fn]
> extern char* tgetstr(char*, char**)

> [spec:libedit:sem:terminal.tgetstr-fn]
> Foreign in the C, ours in the port; this rule is the contract for the
> Rust capability module. String capability lookup.
> Contract: return the string capability named by the first argument from
> the entry loaded by `tgetent`, or NULL if the entry does not define it
> (or if no entry is loaded). In the C the name is a termcap two-letter
> code — libedit passes it through a `strchr(name, name[0])` idiom whose
> only purpose is to launder away a `const`, and which always yields the
> name unchanged. In the port the name is the terminfo name for the same
> capability; the full termcap-to-terminfo table libedit needs is
> enumerated in the `terminal_init_arrow`, `terminal_setflags` and
> `terminal_set` rules and in the individual operation rules.
> The second argument is an in/out pointer into a caller-supplied scratch
> arena: the C decodes the capability's escape notation, copies the
> result there NUL-terminated, and advances the pointer past it, so
> successive calls pack their results into one buffer. libedit passes a
> 2048-byte stack buffer and copies every result out with
> `terminal_alloc` before returning, so nothing is expected to outlive
> the call. The Rust replacement returns an owned or borrowed string and
> has no arena; the `char **` parameter disappears.
> An entry may define a capability as the empty string. libedit's
> "present and non-empty" test treats NULL and "" identically, so no
> caller in this file distinguishes them.
> Rust realisation: index the `strings` map of the loaded TermInfo by the
> terminfo capability name. Two properties of that value matter. It is
> raw bytes, and it must **not** be parameter-expanded here — expansion
> is `tgoto`'s job, and several capabilities libedit emits directly
> (bell, clear, home, clear-to-EOL, insert/delete mode, insert padding,
> single-step cursor motion) are never expanded at all. And it still
> carries its `$<...>` padding markers, which is what `tputs` needs;
> `term` strips padding during parameter expansion, not during the map
> lookup, so the raw map value is the correct source.

> [spec:libedit:def:terminal.tgoto-fn]
> extern char* tgoto(const char*, int, int)

> [spec:libedit:sem:terminal.tgoto-fn]
> Foreign in the C, ours in the port; this rule is the contract for the
> Rust capability module. Parameter substitution into a capability
> string.
> Contract: expand the given capability string with two integer
> parameters — **column first, row second** — and return the resulting
> byte string.
> Every call site is in this file and they take one of two shapes.
> Genuine two-parameter use: `terminal_echotc`'s two-argument form, which
> is user-supplied and is the only place both parameters are meaningful.
> One-parameter capabilities called with the same value passed twice:
> `terminal_move_to_line` (parameterised up), `terminal_move_to_char`
> (column address, parameterised right, parameterised left),
> `terminal_deletechars` (parameterised delete) and
> `terminal_insertwrite` (parameterised insert). Those capabilities
> consume only the first parameter and ignore the second.
> `terminal_echotc`'s one-argument form is the odd case: it passes 0 as
> the column and the user's value as the row.
> The C returns a pointer into a static buffer that the next call
> overwrites. Every libedit call site consumes the result immediately —
> it is passed straight into `terminal_tputs` as the sole argument of the
> same expression — so nothing depends on that lifetime, and the port
> should return an owned string.
> The C `tgoto` understands only the termcap `%` grammar: `%d`, `%2`,
> `%3`, `%.`, `%+x`, `%>xy`, `%i`, `%r`, `%n`, `%B`, `%D`, `%%` — the
> same set `terminal_echotc`'s parameter counter enumerates.
> Rust realisation: `term`'s `parm::expand`, which implements the richer
> terminfo `%` grammar (`%p1`, `%d`, `%i`, `%{n}`, arithmetic,
> conditionals) and is a superset for our purposes. Two cautions.
> First, parameter order. terminfo's two-parameter cursor addressing
> (`cursor_address`, capname `cup`) takes row first and column second,
> the opposite of `tgoto`'s argument order, and the C's termcap emulation
> hides that by swapping at the boundary. libedit never uses a
> two-parameter cursor-address capability internally, so the only exposed
> path is user-supplied via `echotc`; the port must fix the order
> explicitly at the boundary and document which convention it exposes,
> rather than let the discrepancy pass silently.
> Second, padding. `term`'s expander recognises `$<...>` delay markers
> and **discards them**, so a capability that carried padding loses it on
> expansion. Since `tputs` is where padding is realised, the port's
> expansion routine must preserve the `$<...>` runs in its output (or
> return the padding alongside the expanded bytes) — otherwise every
> parameterised capability silently loses its delays. This is called out
> in [dec:libedit:terminal-caps-via-term-crate].

> [spec:libedit:def:terminal.tputs-fn]
> extern int tputs(const char *, int, int (*)(int))

> [spec:libedit:sem:terminal.tputs-fn]
> Foreign in the C, ours in the port — and the one of the six that has no
> counterpart in the `term` crate at all, so this rule is the whole
> specification. See [dec:libedit:terminal-caps-via-term-crate].
> Contract: write the given capability string to the output, expanding
> any embedded padding specification into real delay. In the C each byte
> is passed to the third argument, a one-character callback; libedit's
> callback writes to a file-static `FILE *` (see the terminal_tputs
> rule). The return value is OK/0 in the C and libedit discards it. The
> Rust replacement takes a writer instead of a callback, and the global
> disappears with it.
> The second argument, `affcnt`, is the caller's count of screen lines
> the operation affects. libedit passes 1 for most capabilities, the
> repeat count for the parameterised motion/insert/delete capabilities,
> the recorded line count for clear-screen, and — via `echotc` — possibly
> 0. It is used only by the `*` form below.
> **Padding grammar.** A padding specification is `$<` ... `>` and may
> appear anywhere in the string; in practice it sits at the start or the
> end. Between the delimiters:
> a delay in milliseconds, written as decimal digits with an optional
> single fractional digit after a `.` — `$<5>`, `$<12.5>`. The resolution
> is tenths of a millisecond, and the value is best carried as
> (whole milliseconds * 10 + tenths).
> optionally `*`, meaning the delay is **per affected line**: multiply it
> by `affcnt` before use. `affcnt` is taken as given, so a caller passing
> 0 zeroes such a delay.
> optionally `/`, meaning the delay is **mandatory**: it must be emitted
> even when the terminal claims xon/xoff flow control (terminfo
> `xon_xoff`, capname `xon`). Without `/` the delay is *advisory* and is
> skipped entirely on a flow-controlled terminal, which throttles itself.
> The two suffix characters may appear in either order: `$<5*/>` and
> `$<5/*>` are both valid.
> Everything outside a `$<`...`>` run is emitted verbatim. A `$` not
> followed by `<`, and an unterminated `$<`, are emitted verbatim.
> (Historic BSD `tputs` also accepted a bare leading numeric delay with
> no `$<>` wrapper; terminfo strings never use that form and the port
> need not support it.)
> **Deriving the pad count.** Delay is realised by transmitting padding
> characters, on the theory that a character occupies the line for the
> time it takes to transmit:
> 1. Take the delay in tenths of a millisecond, multiplied by `affcnt` if
> `*` was present.
> 2. If the delay is advisory (no `/`) and the terminal has xon/xoff flow
> control, emit nothing and stop.
> 3. Otherwise let `baud` be the output line speed in bits per second and
> assume 10 bits per transmitted character (one start bit, eight data
> bits, one stop bit — the classical assumption, and the one ncurses
> makes). A character therefore takes `10 / baud` seconds, and a delay of
> `D` tenths of a millisecond needs `D * baud / 100000` padding
> characters; equivalently, for `D_ms` whole milliseconds,
> `D_ms * baud / 10000`. Worked example: 5 ms at 9600 baud is 4.8
> characters.
> 4. Emit that many pad characters. The exact rounding is not portable —
> ncurses truncates, some historical implementations round up — so the
> count can differ by one between implementations. The port must pick one
> and state it; truncation matches ncurses and is the conservative
> choice. A computed count of 0 means no padding at all.
> **The pad character** is whichever byte the terminal's entry gives for
> the pad capability (terminfo `pad_char`, capname `pad`; termcap `pc`).
> If the entry does not define it, the pad character is NUL (0x00). In
> the C this arrives through a global that `tgetent`/`setupterm` sets;
> the port reads it from the loaded entry.
> **The baud rate** is the *output* speed of the tty the string is being
> written to. In the C this is the global `ospeed`, which libedit never
> sets itself — it is initialised by the curses library from the
> terminal's termios — so under libedit's usage it may well be 0, in
> which case ncurses emits no padding whatever. The port must take the
> speed from `el->el_tty.t_speed`, which the tty layer already records
> from the tty's termios, and must treat a zero or unknown speed as
> "emit no padding" rather than as an arithmetic edge case.

