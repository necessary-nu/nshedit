# src/tty.c, src/tty.h

> [spec:libedit:def:tty.el-tty-t]
> typedef struct

> [spec:libedit:def:tty.tty-bind-char-fn]
> libedit_private void tty_bind_char(EditLine *el, int force)

> [spec:libedit:sem:tty.tty-bind-char-fn]
> Re-binds the keymap entries that correspond to tty control characters, so
> that whatever byte the terminal currently uses for erase/kill/eof/etc. runs
> the matching editor action. Returns nothing; cannot fail.
>
> Two byte rows are compared:
>   - `t_n = el->el_tty.t_c[ED_IO]` — the control chars libedit wants in edit
>     mode, indexed by the `C_*` constants.
>   - `t_o = el->el_tty.t_ed.c_cc` — the chars currently recorded in the edit
>     `struct termios`, indexed by the termios `V*` subscripts.
>
> Pick the default maps from the current editor map type:
>   - `el->el_map.type == MAP_VI` (1): `dmap = el->el_map.vii` (vi insert
>     defaults), `dalt = el->el_map.vic` (vi command defaults).
>   - otherwise (`MAP_EMACS`, 0): `dmap = el->el_map.emacs`, `dalt = NULL`.
> The live maps are `map = el->el_map.key` and `alt = el->el_map.alt`, each
> `el_action_t[N_KEYS]` with `N_KEYS == 256`.
>
> The driving table is the static `tty_map[]`, an array of
> `{ wint_t nch; wint_t och; el_action_t bind[3]; }` terminated by the sentinel
> `{(wint_t)-1, (wint_t)-1, {ED_UNASSIGNED, ED_UNASSIGNED, ED_UNASSIGNED}}`.
> `nch` is a `C_*` index into `t_n`, `och` is a termios `V*` subscript into
> `t_o`, and `bind` is indexed 0 = emacs, 1 = vi insert, 2 = vi command. Each
> row exists only if the platform defines the corresponding `V*` macro:
>   - `C_ERASE` / `VERASE`   -> `{EM_DELETE_PREV_CHAR, VI_DELETE_PREV_CHAR, ED_PREV_CHAR}`
>   - `C_ERASE2` / `VERASE2` -> `{EM_DELETE_PREV_CHAR, VI_DELETE_PREV_CHAR, ED_PREV_CHAR}`
>   - `C_KILL` / `VKILL`     -> `{EM_KILL_LINE, VI_KILL_LINE_PREV, ED_UNASSIGNED}`
>   - `C_KILL2` / `VKILL2`   -> `{EM_KILL_LINE, VI_KILL_LINE_PREV, ED_UNASSIGNED}`
>   - `C_EOF` / `VEOF`       -> `{EM_DELETE_OR_LIST, VI_LIST_OR_EOF, ED_UNASSIGNED}`
>   - `C_WERASE` / `VWERASE` -> `{ED_DELETE_PREV_WORD, ED_DELETE_PREV_WORD, ED_PREV_WORD}`
>   - `C_REPRINT` / `VREPRINT` -> `{ED_REDISPLAY, ED_INSERT, ED_REDISPLAY}`
>   - `C_LNEXT` / `VLNEXT`   -> `{ED_QUOTED_INSERT, ED_QUOTED_INSERT, ED_UNASSIGNED}`
> Of those `V*` subscripts only `VERASE`, `VKILL` and `VEOF` are required by
> POSIX; `VERASE2` and `VKILL2` are BSD-only, and `VWERASE`, `VREPRINT` and
> `VLNEXT` are common BSD/Linux extensions. A POSIX-only port keeps the rows
> whose subscript it actually models and drops the rest; the table is data, so
> dropping a row simply means that char is never bound.
>
> For each non-sentinel row `tp`, with two one-character NUL-terminated
> buffers `new[2]` and `old[2]` (`new[1] = old[1] = '\0'`):
>   1. `new[0] = (wchar_t)t_n[tp->nch]`, `old[0] = (wchar_t)t_o[tp->och]`.
>      Both are bytes widened to `wchar_t`, so both are in 0..255.
>   2. If `new[0] == old[0]` and `force` is 0, skip this row. `force` non-zero
>      processes the row even when nothing changed — that is how `tty_setup`
>      installs the initial bindings, since it writes `t_ed.c_cc` from
>      `t_c[ED_IO]` immediately before calling here, making every pair equal.
>   3. `keymacro_clear(el, map, old)` — drops any multi-byte macro rooted at
>      the old byte (it deletes only if `map[old]` is `ED_SEQUENCE_LEAD_IN` and
>      the other map's entry for that byte is not).
>   4. `map[(unsigned char)old[0]] = dmap[(unsigned char)old[0]]` — restore the
>      compiled-in default binding for the byte we are giving up.
>   5. `keymacro_clear(el, map, new)`, then
>      `map[(unsigned char)new[0]] = tp->bind[el->el_map.type]`.
>   6. If `dalt != NULL` (vi only), repeat 3-5 on `alt`, using
>      `alt[(unsigned char)old[0]] = dalt[(unsigned char)old[0]]` and
>      `alt[(unsigned char)new[0]] = tp->bind[el->el_map.type + 1]`, which for
>      `MAP_VI` is `bind[2]`, the vi command-mode action.
> Order within a row matters: the old byte is restored to its default before
> the new byte is bound, so when old and new are the same byte the net effect
> is just the new binding.
>
> Quirk to preserve: nothing special-cases a *disabled* control char. A
> disabled char holds `el->el_tty.t_vdisable`, i.e. `_POSIX_VDISABLE`, whose
> value is platform-dependent — 0 on glibc/Linux, 0xff on the BSDs. Because
> `t_c[ED_IO][C_EOF]` is `_POSIX_VDISABLE` by default, on Linux this binds byte
> 0x00 (NUL) to `EM_DELETE_OR_LIST` / `VI_LIST_OR_EOF`, and on the BSDs it
> binds byte 0xff instead. The same applies to every row whose edit-mode char
> is disabled. This is observable through the key map and must be reproduced,
> including the platform split.

> [spec:libedit:def:tty.tty-cookedmode-fn]
> libedit_private int tty_cookedmode(EditLine *el)

> [spec:libedit:sem:tty.tty-cookedmode-fn]
> Leaves editing mode and puts the terminal back into the "execute" settings.
> Steps, in order:
>   1. If `el->el_tty.t_mode == EX_IO`, return 0 immediately — already cooked,
>      no syscall is issued.
>   2. If `el->el_flags & EDIT_DISABLED`, return 0 without touching the
>      terminal. Note this test comes *after* the mode test, so an EditLine
>      that was put in `ED_IO` and then had editing disabled never gets its
>      terminal restored by this function.
>   3. `tcsetattr(el->el_infd, TCSADRAIN, &el->el_tty.t_ex)` via the EINTR
>      retry wrapper. `TCSADRAIN` means the change takes effect only after all
>      output queued to the terminal has been written; queued input is kept.
>      On -1, return -1 and leave `t_mode` unchanged (the recorded mode stays
>      `ED_IO`/`QU_IO`). The `errno` text is only printed under `DEBUG_TTY`.
>   4. Set `el->el_tty.t_mode = EX_IO` and return 0.
> Reachable from both `ED_IO` and `QU_IO`; from quote mode it goes straight to
> execute mode without passing through `ED_IO`.
> This does not reload or re-derive `t_ex`; it pushes whatever `t_ex` currently
> holds, which is the state left by `tty_setup`, `tty_rawmode`'s propagation
> step, and any `setty -x`.

> [spec:libedit:def:tty.tty-end-fn]
> libedit_private void /*ARGSUSED*/ tty_end(EditLine *el, int how)

> [spec:libedit:sem:tty.tty-end-fn]
> Restores the terminal to the state captured in `el->el_tty.t_or` at
> `tty_setup` time. Returns nothing and reports nothing to the caller.
>   1. If `el->el_flags & EDIT_DISABLED`, return without touching the terminal.
>   2. If `el->el_tty.t_initialized` is 0, return without touching the
>      terminal. This is the "restoration skipped" path: setup either never ran
>      or failed, in which case libedit never changed the terminal either, so
>      the terminal is simply left as the application had it.
>   3. Otherwise `tcsetattr(el->el_infd, how, &el->el_tty.t_or)` via the EINTR
>      retry wrapper. `how` is supplied by the caller and is one of the POSIX
>      `TCSANOW` / `TCSADRAIN` / `TCSAFLUSH` actions — it is observable timing
>      behaviour and must be passed through unchanged. In-tree callers use
>      `TCSAFLUSH` from `el_end` (drain output, then discard pending input) and
>      `TCSADRAIN` from the readline compatibility layer. A -1 result is
>      swallowed; the message is printed only under `DEBUG_TTY`.
> Deliberately *not* done, and observable: `t_initialized` is not cleared and
> `t_mode` is not reset. So if `tty_end` runs while `t_mode` is `ED_IO` or
> `QU_IO`, the terminal is cooked again but libedit still believes it is raw,
> and the next `tty_rawmode` returns 0 without re-applying anything, leaving
> the terminal cooked during editing. The normal shutdown path avoids this
> because `el_end` calls `el_reset` -> `tty_cookedmode` first.

> [spec:libedit:def:tty.tty-get-flag-fn]
> static tcflag_t * tty__get_flag(struct termios *t, int kind)

> [spec:libedit:sem:tty.tty-get-flag-fn]
> Maps a mode-table column index to the matching `tcflag_t` field of a
> `struct termios`, returning a pointer so callers can read and write it:
>   - `MD_INP` (0) -> `&t->c_iflag`
>   - `MD_OUT` (1) -> `&t->c_oflag`
>   - `MD_CTL` (2) -> `&t->c_cflag`
>   - `MD_LIN` (3) -> `&t->c_lflag`
>   - anything else, including `MD_CHAR` (4) and `MD_NN` (5) -> `abort()`,
>     which raises `SIGABRT` and terminates the process. It never returns in
>     that case.
> `MD_CHAR` has no flag word — it is the control-character column — so passing
> it is a programming error and the C treats it as fatal. Every in-tree caller
> loops `MD_INP..MD_LIN` inclusive, so the abort is unreachable in practice; a
> Rust port should express the four-way mapping as a total function over an
> enum of the four flag words rather than reproduce the abort.

> [spec:libedit:def:tty.tty-get-signal-character-fn]
> libedit_private int tty_get_signal_character(EditLine *el, int sig)

> [spec:libedit:sem:tty.tty-get-signal-character-fn]
> Intended to return the control byte the terminal would echo for a given
> signal, so the readline layer (`rl_echo_signal_char`) can echo it manually.
> Returns a byte value 0..255, or -1 meaning "nothing to echo". As written it
> does the following, and it contains two bugs whose observable results the
> port must decide about explicitly.
>   1. Guard, compiled when `ECHOCTL` is defined — which is always, because
>      `tty.h` defines `ECHOCTL` to 0 when the platform lacks it. It reads
>      `*tty__get_flag(&el->el_tty.t_ed, MD_INP)`, i.e. `t_ed.c_iflag`, and
>      returns -1 if `(that & ECHOCTL) == 0`.
>      BUG: `ECHOCTL` (echo control chars as `^X`) is a **`c_lflag`** bit and
>      is not POSIX — it is a BSD/Linux extension. The code tests it against
>      `c_iflag`; the column should be `MD_LIN`. Consequences: where `tty.h`
>      supplied `ECHOCTL == 0` the guard is `(x & 0) == 0`, always true, so the
>      function always returns -1. On glibc/Linux `ECHOCTL` is `0001000`, the
>      same bit value as the input flag `IUCLC`, so the guard actually asks
>      "is `IUCLC` set in the edit-mode input flags" — libedit never sets it
>      and it is off on normal terminals, so again the function returns -1 and
>      `rl_echo_signal_char` is a silent no-op.
>   2. Otherwise switch on `sig`, each arm compiled only if both the signal and
>      the `V*` macro exist:
>        - `SIGINT`  -> `el->el_tty.t_c[ED_IO][VINTR]`
>        - `SIGQUIT` -> `el->el_tty.t_c[ED_IO][VQUIT]`
>        - `SIGINFO` -> `el->el_tty.t_c[ED_IO][VSTATUS]` (`SIGINFO` and
>          `VSTATUS` are BSD-only; neither exists on Linux, so this arm is not
>          compiled there)
>        - `SIGTSTP` -> `el->el_tty.t_c[ED_IO][VSUSP]`
>        - default   -> -1
>      BUG: the `t_c` rows are indexed by the libedit `C_*` constants, not by
>      the termios `V*` subscripts. The two coincide only by accident. On glibc
>      `VINTR == 0 == C_INTR` and `VQUIT == 1 == C_QUIT`, so those two arms are
>      right; `VSUSP == 10` while `C_SUSP == 13`, so `SIGTSTP` returns
>      `t_c[ED_IO][10]`, which is `C_START` — the flow-control start char
>      (`^Q`), not the suspend char. The intended expressions are
>      `t_c[ED_IO][C_INTR]`, `[C_QUIT]`, `[C_STATUS]` and `[C_SUSP]`.
> No arm checks whether the char is disabled; if `t_c[ED_IO][...]` holds
> `t_vdisable` the disable byte itself is returned and the caller echoes it.
> Because the return is an unsigned byte it can never be confused with -1.
> The function does not consult `t_mode`; it answers from the edit-mode row
> whether or not the terminal is currently in edit mode.
> Port decision required: reproducing the bugs freezes a broken observable
> (`rl_echo_signal_char` does nothing on Linux); fixing them changes it. Record
> whichever is chosen — do not fix silently.

> [spec:libedit:def:tty.tty-getchar-fn]
> static void tty__getchar(struct termios *td, unsigned char *s)

> [spec:libedit:sem:tty.tty-getchar-fn]
> Copies control characters out of a `struct termios` into a libedit char row.
> For each supported character it performs `s[C_XXX] = td->c_cc[VXXX]`, where
> `s` points at an array of at least `C_NCC` (25) bytes. Returns nothing;
> cannot fail. The pairs, each compiled only if the `V*` macro exists:
>   - POSIX-required subscripts: `C_INTR`/`VINTR`, `C_QUIT`/`VQUIT`,
>     `C_ERASE`/`VERASE`, `C_KILL`/`VKILL`, `C_EOF`/`VEOF`, `C_EOL`/`VEOL`,
>     `C_START`/`VSTART`, `C_STOP`/`VSTOP`, `C_SUSP`/`VSUSP`, `C_MIN`/`VMIN`,
>     `C_TIME`/`VTIME`.
>   - Non-POSIX / platform-conditional subscripts: `C_EOL2`/`VEOL2` (XSI),
>     `C_SWTCH`/`VSWTCH`, `C_DSWTCH`/`VDSWTCH`, `C_ERASE2`/`VERASE2`,
>     `C_WERASE`/`VWERASE`, `C_DSUSP`/`VDSUSP`, `C_REPRINT`/`VREPRINT`,
>     `C_DISCARD`/`VDISCARD`, `C_LNEXT`/`VLNEXT`, `C_STATUS`/`VSTATUS`,
>     `C_PAGE`/`VPAGE`, `C_PGOFF`/`VPGOFF`, `C_KILL2`/`VKILL2`.
> Slots whose `V*` macro is absent on the platform are **not written** — they
> keep whatever the destination row already held, which is why the caller must
> treat unwritten slots as stale rather than as zero.
> `C_BRK` (22) has no assignment at all, on any platform: there is no `VBRK`
> block here even though `tty.h` defines a `CBRK` default and `ttymodes` has a
> conditional `"brk"` entry. `C_BRK` is therefore inert in this direction.
> Note `VMIN`/`VTIME` are only meaningful in non-canonical mode, and POSIX
> permits them to share storage with `VEOF`/`VEOL`; this function copies all
> four unconditionally, so on a platform with that aliasing the row's
> `C_EOF`/`C_EOL` and `C_MIN`/`C_TIME` entries carry the same bytes.

> [spec:libedit:def:tty.tty-getcharindex-fn]
> static int tty__getcharindex(int i)

> [spec:libedit:sem:tty.tty-getcharindex-fn]
> Translates a libedit `C_*` control-character index into the corresponding
> termios `c_cc` subscript. Pure switch, no side effects. Returns the `V*`
> value, or -1 when the platform has no such subscript.
>   - POSIX-required: `C_INTR`->`VINTR`, `C_QUIT`->`VQUIT`,
>     `C_ERASE`->`VERASE`, `C_KILL`->`VKILL`, `C_EOF`->`VEOF`,
>     `C_EOL`->`VEOL`, `C_START`->`VSTART`, `C_STOP`->`VSTOP`,
>     `C_SUSP`->`VSUSP`, `C_MIN`->`VMIN`, `C_TIME`->`VTIME`.
>   - Non-POSIX / conditional: `C_EOL2`->`VEOL2`, `C_SWTCH`->`VSWTCH`,
>     `C_DSWTCH`->`VDSWTCH`, `C_ERASE2`->`VERASE2`, `C_WERASE`->`VWERASE`,
>     `C_DSUSP`->`VDSUSP`, `C_REPRINT`->`VREPRINT`, `C_DISCARD`->`VDISCARD`,
>     `C_LNEXT`->`VLNEXT`, `C_STATUS`->`VSTATUS`, `C_PAGE`->`VPAGE`,
>     `C_PGOFF`->`VPGOFF`, `C_KILL2`->`VKILL2`.
>   - Everything else, including out-of-range values: -1.
> Gap to preserve or fix deliberately: there is **no case for `C_BRK`** (22),
> so it returns -1 even on a platform that defines `VBRK`. The only caller that
> can reach it with `C_BRK` is `tty_stty`'s `brk=<value>` form, which then trips
> an `assert(c != -1)`; see that rule for the `NDEBUG` consequence.
> `C_NCC` (25) is a count, not an index, and also returns -1.

> [spec:libedit:def:tty.tty-getspeed-fn]
> static speed_t tty__getspeed(struct termios *td)

> [spec:libedit:sem:tty.tty-getspeed-fn]
> Resolves the single "line speed" libedit tracks from a `struct termios`.
> Calls `cfgetispeed(td)`; if the result is 0 it calls `cfgetospeed(td)` and
> returns that instead, otherwise it returns the input speed. Returns a
> `speed_t`; neither `cf*` accessor can fail, so there is no error path.
> The zero test is the POSIX convention that an input speed of `B0` means "the
> input speed equals the output speed". Note that a genuine `B0` output speed
> (hang up the line) is therefore reported as speed 0.
> The value is only ever used for equality comparison against
> `el->el_tty.t_speed` and as the argument to `cfsetispeed`/`cfsetospeed`, so
> it must be kept as the opaque `speed_t` encoding rather than a baud number.

> [spec:libedit:def:tty.tty-getty-fn]
> static int tty_getty(EditLine *el, struct termios *t)

> [spec:libedit:sem:tty.tty-getty-fn]
> EINTR-retrying wrapper around `tcgetattr`. Loops calling
> `tcgetattr(el->el_infd, t)` for as long as it returns -1 with
> `errno == EINTR`; returns the first result that is not an EINTR failure, i.e.
> 0 on success or -1 with `errno` set to something other than `EINTR`.
> `*t` is only meaningful on success.
> Note the descriptor: this reads the terminal attributes of the **input** fd
> `el_infd`, even though `tty_setup`'s `isatty` guard tests `el_outfd`.

> [spec:libedit:def:tty.tty-init-fn]
> libedit_private int tty_init(EditLine *el)

> [spec:libedit:sem:tty.tty-init-fn]
> Initialises the per-EditLine tty state from the compiled-in tables and then
> runs `tty_setup`. Returns whatever `tty_setup` returns: 0 on success, -1 on
> failure. Steps, in order and unconditionally (no `EDIT_DISABLED` check here):
>   1. `el->el_tty.t_mode = EX_IO` — the mode is *asserted*, not observed. If
>      the terminal happens to be raw at this point, libedit now disagrees with
>      reality.
>   2. `el->el_tty.t_vdisable = _POSIX_VDISABLE`. Platform-conditional value:
>      POSIX defines the constant but not its value; glibc/Linux uses 0, the
>      BSDs and macOS use 0xff. `tty.h` falls back to `(unsigned char)-1` where
>      the platform defines neither `_POSIX_VDISABLE` nor `VDISABLE`. Every
>      "is this char disabled" test in this file compares against this byte,
>      so the split is observable and must be sourced from the platform.
>   3. `el->el_tty.t_initialized = 0`. This is what makes re-initialisation
>      work: `tty_setup` refuses to run when the flag is already set, and
>      clearing it here is the only place it is ever cleared.
>   4. `memcpy(el->el_tty.t_t, ttyperm, sizeof(ttyperm_t))` — install a
>      writable per-EditLine copy of the mode-permission table.
>   5. `memcpy(el->el_tty.t_c, ttychar, sizeof(ttychar_t))` — install a
>      writable per-EditLine copy of the control-character table.
>   6. `return tty_setup(el)`.
> Because steps 4 and 5 are unconditional, calling `tty_init` again discards
> every `setty` customisation and reverts to the built-in defaults; the
> readline layer does exactly this on each `readline()` call.
>
> `ttyperm` is a `ttyperm_t`, i.e. `struct { const char *t_name; unsigned int
> t_setmask; unsigned int t_clrmask; } [NN_IO][MD_NN]`. The first index is the
> mode (`EX_IO` 0, `ED_IO` 1, `TS_IO`/`QU_IO` 2), the second the field group
> (`MD_INP` 0, `MD_OUT` 1, `MD_CTL` 2, `MD_LIN` 3, `MD_CHAR` 4). `t_name` is
> the label printed by `setty` and is `"iflag:"`, `"oflag:"`, `"cflag:"`,
> `"lflag:"`, `"chars:"` in every row. For the four flag groups the masks are
> bits of the matching `tcflag_t`; for `MD_CHAR` they are bitmaps over the
> `C_*` indices via `C_SH(A) == (unsigned)(1 << A)`. The compiled-in contents:
>   - `EX_IO` (execute / cooked):
>       `iflag:` set `ICRNL`; clear `INLCR | IGNCR`
>       `oflag:` set `OPOST | ONLCR`; clear `ONLRET`
>       `cflag:` set 0; clear 0
>       `lflag:` set `ISIG | ICANON | ECHO | ECHOE | ECHOCTL | IEXTEN`;
>                clear `NOFLSH | ECHONL | EXTPROC | FLUSHO`
>       `chars:` set 0; clear 0
>   - `ED_IO` (editing / raw):
>       `iflag:` set `INLCR | ICRNL`; clear `IGNCR`
>       `oflag:` set `OPOST | ONLCR`; clear `ONLRET`
>       `cflag:` set 0; clear 0
>       `lflag:` set `ISIG`;
>                clear `NOFLSH | ICANON | ECHO | ECHOK | ECHONL | EXTPROC | IEXTEN | FLUSHO`
>       `chars:` set `C_SH(C_MIN) | C_SH(C_TIME) | C_SH(C_SWTCH) |
>                C_SH(C_DSWTCH) | C_SH(C_SUSP) | C_SH(C_DSUSP) | C_SH(C_EOL) |
>                C_SH(C_DISCARD) | C_SH(C_PGOFF) | C_SH(C_PAGE) |
>                C_SH(C_STATUS)`; clear 0
>   - `QU_IO` (quoted-insert):
>       `iflag:` set 0; clear `IXON | IXOFF | INLCR | ICRNL`
>       `oflag:` set 0; clear 0
>       `cflag:` set 0; clear 0
>       `lflag:` set 0; clear `ISIG | IEXTEN`
>       `chars:` set 0; clear 0
> Of the bits named there, `ICRNL`, `INLCR`, `IGNCR`, `IXON`, `IXOFF`, `OPOST`,
> `ISIG`, `ICANON`, `ECHO`, `ECHOE`, `ECHOK`, `ECHONL`, `NOFLSH` and `IEXTEN`
> are POSIX; `ONLCR` and `ONLRET` are POSIX XSI output-processing bits;
> `ECHOCTL`, `EXTPROC` and `FLUSHO` are **not POSIX** and `tty.h` defines each
> to 0 where the platform lacks it, which makes those mask contributions
> no-ops rather than errors. Two entries are worth flagging as surprising but
> intended: `ED_IO` sets `INLCR` *and* `ICRNL`, so in edit mode a received CR
> is delivered as NL and a received NL is delivered as CR; and `ED_IO` sets
> `ISIG`, so signal chars stay live while editing.
>
> `ttychar` is a `ttychar_t`, i.e. `unsigned char [NN_IO][C_NCC]`, rows indexed
> the same way and columns by the `C_*` constants. The compiled-in contents:
>   - `EX_IO`: the platform defaults, in `C_*` order — `CINTR, CQUIT, CERASE,
>     CKILL, CEOF, CEOL, CEOL2, CSWTCH, CDSWTCH, CERASE2, CSTART, CSTOP,
>     CWERASE, CSUSP, CDSUSP, CREPRINT, CDISCARD, CLNEXT, CSTATUS, CPAGE,
>     CPGOFF, CKILL2, CBRK, CMIN, CTIME`. `tty.h` supplies fallbacks for any
>     the platform does not define: `CINTR`=`^C`, `CQUIT`=0o34 (`^\`),
>     `CERASE`=0o177 (DEL), `CKILL`=`^U`, `CEOF`=`^D`, `CSTART`=`^Q`,
>     `CSTOP`=`^S`, `CSUSP`=`^Z`, `CDSUSP`=`^Y`, `CREPRINT`=`^R`,
>     `CDISCARD`=`^O`, `CLNEXT`=`^V`, `CWERASE`=`^W`, `CSTATUS`=`^T`,
>     `CPAGE`=`' '`, `CPGOFF`=`^M`, `CBRK`=0o377, `CMIN`=`CEOF`,
>     `CTIME`=`CEOL`, and `_POSIX_VDISABLE` for `CEOL`, `CEOL2`, `CSWTCH`,
>     `CDSWTCH`, `CERASE2`, `CKILL2`. On hpux `CREPRINT`, `CDISCARD`, `CLNEXT`
>     and `CWERASE` fall back to `_POSIX_VDISABLE` instead. Note the fallback
>     `CMIN`/`CTIME` are nonsense as VMIN/VTIME (`^D` and the disable byte);
>     they are harmless only because `EX_IO` is canonical, where VMIN/VTIME are
>     ignored.
>   - `ED_IO`: `CINTR, CQUIT, CERASE, CKILL,` then `_POSIX_VDISABLE` for
>     `C_EOF`, `C_EOL`, `C_EOL2`, `C_SWTCH`, `C_DSWTCH`; `CERASE2`; `CSTART`;
>     `CSTOP`; `_POSIX_VDISABLE` for `C_WERASE`; `CSUSP`; `_POSIX_VDISABLE`
>     for `C_DSUSP` and `C_REPRINT`; `CDISCARD`; `_POSIX_VDISABLE` for
>     `C_LNEXT`, `C_STATUS`, `C_PAGE`, `C_PGOFF`, `C_KILL2`, `C_BRK`; then
>     `C_MIN` = 1 and `C_TIME` = 0. That is: libedit keeps interrupt, quit,
>     erase, kill, flow control, suspend and discard live in the kernel, takes
>     over EOF/word-erase/reprint/literal-next itself, and asks for
>     one-byte-at-a-time reads with no timer.
>   - `TS_IO`/`QU_IO`: all 25 bytes 0. Row 2 of `t_c` is only ever used in its
>     `TS_IO` sense, as the scratch row `tty__getchar` reads the terminal into;
>     there is no quote-mode character row, because quote mode only ever
>     applies flag masks. So these zeros are just the scratch row's initial
>     value.

> [spec:libedit:def:tty.tty-noquotemode-fn]
> libedit_private int tty_noquotemode(EditLine *el)

> [spec:libedit:sem:tty.tty-noquotemode-fn]
> Leaves quoted-insert mode and returns to editing mode.
>   1. If `el->el_tty.t_mode != QU_IO`, return 0 without touching the terminal.
>      There is no `EDIT_DISABLED` check — the mode test is the only guard, and
>      it suffices because quote mode can only be entered via `tty_quotemode`.
>   2. `tcsetattr(el->el_infd, TCSADRAIN, &el->el_tty.t_ed)` via the EINTR
>      retry wrapper. On -1, return -1 and leave `t_mode` at `QU_IO`, so the
>      terminal keeps the quote-mode flags and libedit knows it.
>   3. Set `el->el_tty.t_mode = ED_IO` and return 0.
> It does not repair `el->el_tty.t_ts`, which `tty_quotemode` overwrote (see
> that rule); nothing depends on the old contents, because `tty_rawmode`
> re-reads `t_ts` from the terminal before using it.

> [spec:libedit:def:tty.tty-printchar-fn]
> static void tty_printchar(EditLine *el, unsigned char *s)

> [spec:libedit:sem:tty.tty-printchar-fn]
> Debug-only dumper for a control-character row, guarded by `#ifdef notyet`,
> which nothing defines. It is dead code, it has no callers, and **as written
> it does not compile**: it declares `ttyperm_t *m` and initialises it from
> `el->el_tty.t_t`, then dereferences `m->m_name`, `m->m_type` and `m->m_value`
> — fields of `ttymodes_t`, not of `ttyperm_t`, whose members are `t_name`,
> `t_setmask` and `t_clrmask`. `ttyperm_t` is additionally a two-dimensional
> array type, so the initialisation is a type error too. The loop was plainly
> meant to walk the static `ttymodes[]` table.
> Intended behaviour, for the record: for each `i` in `0 .. C_NCC-1`, find the
> first `ttymodes` entry with `m_type == MD_CHAR` and `m_value == C_SH(i)`;
> if one is found print `"<m_name> ^<c> "` to `el->el_errfile`, where `c` is
> `s[i] + 'A' - 1`, i.e. the caret rendering of a control byte (it produces
> garbage for any byte outside 1..26, including the disable byte); and after
> every `i` where `i % 5 == 0` print a newline — note that this fires on
> `i == 0`, so the output starts with a blank line and the grouping is offset
> by one. A trailing newline is printed after the loop. Returns nothing.
> Port guidance: this is not portable, is not reachable, and is not part of the
> observable ABI. Either omit it, or write a fresh debug formatter from the
> intent above; do not transliterate the broken code.

> [spec:libedit:def:tty.tty-quotemode-fn]
> libedit_private int tty_quotemode(EditLine *el)

> [spec:libedit:sem:tty.tty-quotemode-fn]
> Enters quoted-insert mode: the terminal is put into edit-mode settings with
> flow control, CR/NL translation, signals and extended processing switched
> off, so the very next byte reaches the reader untouched.
>   1. If `el->el_tty.t_mode == QU_IO`, return 0 without touching anything.
>      There is no `EDIT_DISABLED` guard here, unlike `tty_rawmode` and
>      `tty_cookedmode`.
>   2. `el->el_tty.t_qu = el->el_tty.t_ed` — a whole-struct copy of the edit
>      termios. `t_qu` is `#define t_qu t_ts`, so this *overwrites the `TS_IO`
>      scratch termios*. There are only four `struct termios` in `el_tty_t`
>      (`t_or`, `t_ex`, `t_ed`, `t_ts`) and quote mode shares the last one with
>      the terminal-snapshot scratch.
>   3. `tty_setup_flags(el, &el->el_tty.t_qu, QU_IO)` — apply the `QU_IO` masks
>      to the four flag words: clear `IXON | IXOFF | INLCR | ICRNL` from
>      `c_iflag` (set nothing), leave `c_oflag` and `c_cflag` untouched (both
>      masks are 0), clear `ISIG | IEXTEN` from `c_lflag` (set nothing). The
>      control characters are *not* touched, so `c_cc` stays as edit mode left
>      it, including `VMIN == 1`, `VTIME == 0`.
>   4. `tcsetattr(el->el_infd, TCSADRAIN, &el->el_tty.t_qu)` via the EINTR
>      retry wrapper. On -1, return -1 with `t_mode` unchanged.
>   5. Set `el->el_tty.t_mode = QU_IO` and return 0.
> Because the base is `t_ed` rather than the current terminal state, entering
> quote mode from `EX_IO` (which the mode test permits) also puts the terminal
> into raw editing settings while `t_mode` records `QU_IO`. Leaving via
> `tty_noquotemode` then lands in `ED_IO`, not back in `EX_IO`.

> [spec:libedit:def:tty.tty-rawmode-fn]
> libedit_private int tty_rawmode(EditLine *el)

> [spec:libedit:sem:tty.tty-rawmode-fn]
> Enters editing mode: puts the terminal into one-character-at-a-time settings
> and, on the way, adopts any changes the application made to the terminal
> while libedit was in cooked mode. Returns 0 on success (including the
> no-op cases), -1 on failure.
>   1. If `el->el_tty.t_mode == ED_IO` or `== QU_IO`, return 0 — already raw or
>      quoting, no syscall issued.
>   2. If `el->el_flags & EDIT_DISABLED`, return 0 without touching anything.
>   3. `tty_getty(el, &el->el_tty.t_ts)` — snapshot the terminal's current
>      attributes into the `TS_IO` scratch termios. On -1, return -1; nothing
>      has been changed.
>   4. Unconditionally track two properties from that snapshot:
>        `el->el_tty.t_eight = ((t_ts.c_cflag & CSIZE) == CS8)`
>        `el->el_tty.t_speed = tty__getspeed(&t_ts)`
>      (the comment in the C is explicit that speed and the eight-bit setting
>      are always believed, while everything else is believed only when the
>      terminal was left in canonical mode).
>   5. If `tty__getspeed(&t_ex) != t_speed` **or**
>      `tty__getspeed(&t_ed) != t_speed`, call `cfsetispeed` and `cfsetospeed`
>      on **both** `t_ex` and `t_ed` with `t_speed`. Return values are ignored.
>      Note this forces input speed equal to output speed on both structures
>      even if only one of them was stale.
>   6. If `(t_ts.c_lflag & ICANON) != 0` — "the terminal is in cooked mode, so
>      believe what we see":
>        a. For `kind` = `MD_INP`, `MD_OUT`, `MD_CTL`, `MD_LIN` in that order,
>           call `tty_update_flags(el, kind)`, which re-derives `t_ed` and
>           `t_ex`'s flag word from the snapshot (see that rule).
>        b. Tabs: if `((t_ex.c_oflag & TAB3) == TAB3)` then `t_tabs = 0`, else
>           `t_tabs = EL_CAN_TAB ? 1 : 0`, where `EL_CAN_TAB` is the terminfo
>           capability bit for "the terminal can use hardware tabs". `TAB3` is
>           the XSI `TABDLY` value meaning "expand tabs to spaces"; `tty.h`
>           aliases it to `OXTABS` on the BSDs and to 0 where neither exists,
>           in which case the test `(x & 0) == 0` is always true and `t_tabs`
>           is always forced to 0. Note this inspects `t_ex`, which step (a)
>           may just have rewritten, not the raw snapshot.
>        c. `tty__getchar(&t_ts, el->el_tty.t_c[TS_IO])` — read the terminal's
>           control chars into the scratch row.
>        d. Scan `i` from 0 while `i < C_NCC` for the first index where
>           `t_c[TS_IO][i] != t_c[EX_IO][i]`. If the scan ran to completion
>           (`i == C_NCC`), nothing changed and the whole propagation block is
>           skipped. Otherwise, in this exact order:
>             - for `i` in `0 .. C_NCC-1`: `tty_update_char(el, ED_IO, i)`
>             - `tty_bind_char(el, 0)` — non-forced, so it rebinds only the
>               keys whose edit-mode char actually differs from what `t_ed.c_cc`
>               still holds. This **must** run before the next step, because it
>               diffs the new `t_c[ED_IO]` against the *old* `t_ed.c_cc`.
>             - `tty__setchar(&t_ed, t_c[ED_IO])`
>             - for `i` in `0 .. C_NCC-1`: `tty_update_char(el, EX_IO, i)`
>             - `tty__setchar(&t_ex, t_c[EX_IO])`
>           Since `EX_IO`'s `MD_CHAR` set and clear masks are both 0, the
>           second `tty_update_char` loop reduces to copying the scratch row
>           into the `EX_IO` row wholesale.
>   7. `tcsetattr(el->el_infd, TCSADRAIN, &el->el_tty.t_ed)` via the EINTR
>      retry wrapper. `TCSADRAIN` waits for queued output to drain and does not
>      discard pending input, so type-ahead typed before editing started
>      survives into the editor. On -1, return -1 with `t_mode` left at
>      `EX_IO`.
>   8. Set `el->el_tty.t_mode = ED_IO` and return 0.
> If step 6 was skipped (the terminal was already non-canonical), `t_ed` is
> pushed exactly as it stands, without re-reading anything from the terminal
> beyond speed and character size.

> [spec:libedit:def:tty.tty-setchar-fn]
> static void tty__setchar(struct termios *td, unsigned char *s)

> [spec:libedit:sem:tty.tty-setchar-fn]
> The inverse of `tty__getchar`: copies a libedit char row into a
> `struct termios`. For each supported character it performs
> `td->c_cc[VXXX] = s[C_XXX]`, with exactly the same set of conditionally
> compiled pairs and the same POSIX / non-POSIX split listed in the
> `tty__getchar` rule. Returns nothing; cannot fail. It writes only `c_cc` —
> no flag word and no speed is touched — and it does not call `tcsetattr`, so
> the change reaches the terminal only when a caller pushes the struct.
> Slots whose `V*` macro is absent on the platform are not written, and there
> is no `VBRK` assignment on any platform, so `C_BRK` is inert in this
> direction too.
> `s[C_*]` is an `unsigned char` and `c_cc[]` elements are `cc_t`, so the
> assignment is value-preserving on any POSIX platform where `cc_t` is at
> least 8 bits.
> Callers: `tty_setup` (into `t_ex` and `t_ed`) and `tty_rawmode`'s
> propagation block (into `t_ed` then `t_ex`).

> [spec:libedit:def:tty.tty-setty-fn]
> static int tty_setty(EditLine *el, int action, const struct termios *t)

> [spec:libedit:sem:tty.tty-setty-fn]
> EINTR-retrying wrapper around `tcsetattr`. Loops calling
> `tcsetattr(el->el_infd, action, t)` for as long as it returns -1 with
> `errno == EINTR`; returns the first result that is not an EINTR failure, i.e.
> 0 on success or -1 with `errno` set to something other than `EINTR`.
> `action` is a POSIX `TCSANOW` / `TCSADRAIN` / `TCSAFLUSH` value and is
> observable timing behaviour: `TCSANOW` applies immediately, `TCSADRAIN`
> waits for queued output to drain first, `TCSAFLUSH` drains output and then
> discards unread input. Every in-file caller passes `TCSADRAIN` except
> `tty_end`, which forwards its own `how` argument.
> Writes go to the **input** fd `el_infd`, matching `tty_getty`.
> Caveat inherited from POSIX and not compensated for anywhere: `tcsetattr`
> returns success if it managed to apply *any* of the requested changes, so a
> 0 return does not prove the terminal now matches `*t`. libedit never reads
> the settings back to verify.

> [spec:libedit:def:tty.tty-setup-flags-fn]
> static void tty_setup_flags(EditLine *el, struct termios *tios, int mode)

> [spec:libedit:sem:tty.tty-setup-flags-fn]
> Applies one mode's four flag-word masks to a `struct termios`, in place.
> For `kind` = `MD_INP`, `MD_OUT`, `MD_CTL`, `MD_LIN` in that order:
>   `f = tty__get_flag(tios, kind)`, then `*f = tty_update_flag(el, *f, mode, kind)`
> i.e. each of `c_iflag`, `c_oflag`, `c_cflag`, `c_lflag` has the mode's
> `t_clrmask` bits cleared and then its `t_setmask` bits set.
> Returns nothing; cannot fail. `MD_CHAR` is deliberately excluded — control
> characters are never touched here — so `tty__get_flag`'s abort is not
> reachable from this function. Speed is not touched either. Nothing is pushed
> to the terminal; the caller decides whether and when to `tcsetattr`.
> `mode` is `EX_IO`, `ED_IO` or `QU_IO`, and selects the row of
> `el->el_tty.t_t` used, so the effect depends on the *current* (possibly
> `setty`-modified) copy of the table, not on the compiled-in defaults.

> [spec:libedit:def:tty.tty-setup-fn]
> static int tty_setup(EditLine *el)

> [spec:libedit:sem:tty.tty-setup-fn]
> Captures the terminal's original state, derives the execute and edit
> termios from it, optionally resets the terminal's control characters, and
> installs the key bindings. Returns 0 on success, -1 on failure. Steps:
>   1. `rst = (el->el_flags & NO_RESET) == 0` — whether libedit is allowed to
>      write the terminal's control chars at startup. `NO_RESET` is set by the
>      readline compatibility layer.
>   2. If `el->el_flags & EDIT_DISABLED`, return **0** — success, but nothing
>      is done and `t_initialized` stays 0, so `tty_end` will later do nothing
>      as well.
>   3. If `el->el_tty.t_initialized` is already 1, return -1. Only `tty_init`
>      clears that flag, so setup runs exactly once per `tty_init`.
>   4. If `!isatty(el->el_outfd)`, return -1. Note the asymmetry: the guard
>      tests the **output** fd while every termios call in this file uses the
>      **input** fd. When they differ and input is not a terminal, the next
>      step fails with `ENOTTY` and setup still returns -1, so the asymmetry
>      degrades safely rather than corrupting anything.
>   5. `tty_getty(el, &el->el_tty.t_or)`; on -1 return -1. This is the sole
>      capture of the original terminal state, and `t_or` is exactly what
>      `tty_end` writes back. Nothing else ever assigns `t_or`.
>   6. `el->el_tty.t_ts = el->el_tty.t_ex = el->el_tty.t_ed = el->el_tty.t_or`
>      — three whole-struct copies, so all four termios start identical.
>   7. Derive the tracked properties from `t_ex`:
>        `t_speed = tty__getspeed(&t_ex)`
>        `t_tabs  = ((t_ex.c_oflag & TAB3) == TAB3) ? 0 : 1`
>        `t_eight = ((t_ex.c_cflag & CSIZE) == CS8)`
>      `TAB3` is the XSI `TABDLY` "expand tabs" value; `tty.h` aliases it to
>      `OXTABS` on the BSDs and to 0 where neither exists, in which case the
>      test is always true and `t_tabs` is always 0.
>   8. `tty_setup_flags(el, &el->el_tty.t_ex, EX_IO)` — apply the execute-mode
>      masks to `t_ex`'s four flag words.
>   9. If `rst`, reset the terminal's control characters to sane values:
>        a. If `(el->el_tty.t_ts.c_lflag & ICANON) != 0` — only trust the
>           terminal's chars if it was left in canonical mode:
>             - `tty__getchar(&t_ts, el->el_tty.t_c[TS_IO])` — snapshot the
>               terminal's chars into the scratch row.
>             - For `i` in `0 .. C_NCC-3` (i.e. **excluding** the last two
>               indices, `C_MIN` and `C_TIME`): if
>               `t_c[TS_IO][i] != t_vdisable` **and**
>               `t_c[ED_IO][i] != t_vdisable`, then
>               `t_c[ED_IO][i] = t_c[TS_IO][i]`. So the editor row adopts the
>               user's chars only where both the terminal's value and
>               libedit's own default are enabled: a char libedit deliberately
>               disables in edit mode (EOF, word-erase, reprint, literal-next,
>               ...) stays disabled, and a char the user disabled is not
>               propagated. `C_MIN`/`C_TIME` are skipped so edit mode keeps
>               `VMIN == 1`, `VTIME == 0`.
>             - For `i` in `0 .. C_NCC-1` (all indices this time): if
>               `t_c[TS_IO][i] != t_vdisable` then
>               `t_c[EX_IO][i] = t_c[TS_IO][i]`.
>        b. `tty__setchar(&el->el_tty.t_ex, el->el_tty.t_c[EX_IO])`.
>        c. `tcsetattr(el->el_infd, TCSADRAIN, &el->el_tty.t_ex)` via the EINTR
>           retry wrapper; on -1 return -1. This is the only terminal write
>           `tty_setup` performs, and it happens **before** `t_initialized` is
>           set — so a failure here leaves the terminal possibly modified while
>           `tty_end` will decline to restore it.
>      If `rst` is false, none of 9a-9c happens: no `tcsetattr` at all, the
>      `EX_IO` row keeps the compiled-in defaults, and the terminal is left
>      untouched until the first `tty_rawmode`.
>  10. `tty_setup_flags(el, &el->el_tty.t_ed, ED_IO)` — apply the edit-mode
>      masks to `t_ed`'s four flag words. Always done, `rst` or not.
>  11. `tty__setchar(&el->el_tty.t_ed, el->el_tty.t_c[ED_IO])`.
>  12. `tty_bind_char(el, 1)` — forced, because step 11 has just made
>      `t_ed.c_cc` agree with `t_c[ED_IO]` for every mapped char, so a
>      non-forced call would find no differences and bind nothing.
>  13. `el->el_tty.t_initialized = 1`; return 0.
> `t_mode` is not assigned here — `tty_init` sets it to `EX_IO` before calling.
> Implementation note carried over from the C: the local `rst` is reused as
> the loop counter in step 9a and holds `C_NCC` afterwards; it is never
> re-tested, so this is harmless.

> [spec:libedit:def:tty.tty-stty-fn]
> libedit_private int /*ARGSUSED*/ tty_stty(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv)

> [spec:libedit:sem:tty.tty-stty-fn]
> The `setty` builtin: displays or edits libedit's per-mode termios mask
> tables. Returns 0 on success, -1 on any error. `argc` is completely ignored;
> `argv` is a NULL-terminated array of wide strings whose first element is the
> command name.
>   1. If `argv == NULL`, return -1.
>   2. `name` = the first argument encoded to a multibyte string via
>      `ct_encode_string(*argv++, &el->el_scratch)`, copied with `strlcpy` into
>      a `char[EL_BUFSIZ]` (1024) local. It is used only in error messages.
>      UB note: `ct_encode_string` returns NULL for a NULL input, and the code
>      does not check, so `argv` pointing at an empty vector dereferences NULL.
>   3. Working state: `tios = &el->el_tty.t_ex`, `z = EX_IO`, `aflag = 0`.
>   4. Option loop, run while `argv && *argv && argv[0][0] == L'-' &&
>      argv[0][2] == L'\0'` — that is, while the next token is exactly two wide
>      characters long and starts with `-`. Dispatch on `argv[0][1]`:
>        - `a`: `aflag++`, advance. Show every mode name, not only those with
>          an explicit `+`/`-`.
>        - `d`: advance; `tios = &el->el_tty.t_ed`, `z = ED_IO`.
>        - `x`: advance; `tios = &el->el_tty.t_ex`, `z = EX_IO`.
>        - `q`: advance; `tios = &el->el_tty.t_ts`, `z = QU_IO`. (`QU_IO` and
>          `TS_IO` are both 2, and `t_qu` is `#define`d to `t_ts`, so this is
>          the quote-mode row and the scratch termios at once.)
>        - anything else: print `"<name>: Unknown switch `<c>'.\n"` to
>          `el->el_errfile` and return -1, without advancing.
>      UB note: for the one-character token `L"-"`, `argv[0][1]` is the
>      terminating NUL and `argv[0][2]` is read one element past the end of the
>      string — an out-of-bounds read. A port should test the length instead.
>   5. Display form — if no arguments remain (`!argv || !*argv`): walk the
>      static `ttymodes[]` table and print the mask state for mode `z` to
>      `el->el_outfile`, then print a final `"\n"` and return 0. State: `i` =
>      the group last printed, initialised -1; `len` = current output column;
>      `st` = the indent to use when wrapping.
>        - When `m->m_type != i`: print `"\n"` first unless this is the first
>          group, then print `el->el_tty.t_t[z][m->m_type].t_name` (one of
>          `"iflag:"`, `"oflag:"`, `"cflag:"`, `"lflag:"`, `"chars:"`); set
>          `i = m->m_type` and `st = len = strlen(t_name)`. This relies on
>          `ttymodes` being grouped by `m_type` in `MD_INP`, `MD_OUT`,
>          `MD_CTL`, `MD_LIN`, `MD_CHAR` order — hence the "Don't re-order"
>          comment on the `MD_*` constants in `tty.h`.
>        - Sign for the entry: `x = '+'` if
>          `t_t[z][i].t_setmask & m->m_value`, else `'\0'`; then, overriding,
>          `x = '-'` if `t_t[z][i].t_clrmask & m->m_value`. Clear wins in the
>          display. (The `i == -1` fallback branch in the C is unreachable,
>          since the first entry always assigns a non-negative `m_type`.)
>        - Print the entry only if `x != '\0'` or `aflag` is set. Wrapping:
>          `cu = strlen(m->m_name) + (x != '\0') + 1`; if
>          `len + cu >= (size_t)el->el_terminal.t_size.h` (the terminal width)
>          print `"\n"` followed by `st` spaces and set `len = st + cu`,
>          otherwise `len += cu`. Then print `"<x><m_name> "` if signed, or
>          `"<m_name> "` if not.
>   6. Edit form — otherwise, for each remaining argument `s`:
>        a. A leading `+` or `-` is consumed into `x` and `s` advanced;
>           otherwise `x = '\0'`.
>        b. `d = s`; `p = wcschr(s, L'=')`.
>        c. Find the first `ttymodes` entry matching the name: if `p != NULL`,
>           `strncmp(m->m_name, ct_encode_string(d, ...), (size_t)(p - d)) == 0`
>           **and** `m->m_type == MD_CHAR`; if `p == NULL`,
>           `strcmp(m->m_name, ct_encode_string(d, ...)) == 0`.
>           Caveat: the length passed to `strncmp` counts *wide* characters
>           while the comparison operates on the *encoded* bytes; all table
>           names are ASCII, so this only misbehaves on non-ASCII input. A
>           prefix match also succeeds against any longer table name.
>        d. No match (the walk reached the `{NULL, 0, -1}` sentinel): print
>           `"<name>: Invalid argument `<d>'.\n"` to `el->el_errfile` and
>           return -1. Arguments already processed keep their effect.
>        e. If `p != NULL` (the `name=value` form, only valid for `MD_CHAR`
>           entries): `c = ffs((int)m->m_value)` — the 1-based position of the
>           lowest set bit; since `MD_CHAR` entries hold `C_SH(C_XXX)` i.e.
>           `1 << C_XXX`, `c - 1` is the `C_*` index. `assert(c != 0)`, then
>           `c--`, then `c = tty__getcharindex(c)` and `assert(c != -1)`.
>           The value is `v = *++p ? parse__escape(&p) : el->el_tty.t_vdisable`
>           — so `name=` with nothing after the `=` **disables** the char,
>           otherwise the text is parsed as an escape (`^X`, `\n`, `\033`, a
>           literal char, ...). Finally `tios->c_cc[c] = (cc_t)v`, then move on
>           to the next argument.
>           Three hazards, all to be decided explicitly rather than inherited:
>             * The write lands in the selected `struct termios` **only**;
>               `el->el_tty.t_c[z][...]` is not updated. Any later
>               `tty__setchar` from `t_c` — which `tty_rawmode` performs
>               whenever it detects a control-char change — silently reverts it.
>             * `assert` compiles out under `NDEBUG`. The `"brk"` entry exists
>               where the platform defines `VBRK`, and `tty__getcharindex` has
>               no `C_BRK` case, so `c` becomes -1 and `tios->c_cc[-1]` is an
>               out-of-bounds write. That is UB.
>             * `parse__escape` returns -1 for a malformed escape; the result is
>               stored as `(cc_t)-1` with no check.
>        f. Otherwise apply the sign to the mask pair
>           `el->el_tty.t_t[z][m->m_type]`:
>             `+`  : `t_setmask |= m_value; t_clrmask &= ~m_value`
>             `-`  : `t_setmask &= ~m_value; t_clrmask |= m_value`
>             none : `t_setmask &= ~m_value; t_clrmask &= ~m_value` — the bit
>                    is left alone at apply time, i.e. inherited from whatever
>                    the terminal has.
>   7. `tty_setup_flags(el, tios, z)` — re-derive the selected termios' four
>      flag words from the (possibly just-modified) masks for mode `z`.
>   8. If `el->el_tty.t_mode == z`, push it:
>      `tcsetattr(el->el_infd, TCSADRAIN, tios)` via the EINTR retry wrapper;
>      on -1 return -1. If the edited mode is not the current one, the change
>      only takes effect the next time that mode is entered.
>   9. Return 0.
> Note on `-q`: `t_ts` is overwritten by `tty_rawmode` (from `tcgetattr`) and by
> `tty_quotemode` (a copy of `t_ed`), so direct edits to that struct are
> transient. Only the `t_t[QU_IO]` masks persist, and `tty_quotemode` re-applies
> them each time.
>
> The `ttymodes[]` table this walks is data: an array of
> `{ const char *m_name; unsigned int m_value; int m_type; }` terminated by
> `{NULL, 0, -1}`. `m_type` says which mask pair the entry belongs to and
> `m_value` is the bit(s) it controls — a `tcflag_t` bit for `MD_INP`/
> `MD_OUT`/`MD_CTL`/`MD_LIN`, or `C_SH(C_XXX)` for `MD_CHAR`. Each entry is
> compiled only if the corresponding macro exists, which is what makes the
> table platform-dependent, and the entries are grouped by `m_type` in
> ascending order. Contents, by group:
>   - `MD_INP` (`c_iflag`): `ignbrk`, `brkint`, `ignpar`, `parmrk`, `inpck`,
>     `istrip`, `inlcr`, `igncr`, `icrnl`, `iuclc`, `ixon`, `ixany`, `ixoff`,
>     `imaxbel`. All POSIX except `iuclc` (`IUCLC`, legacy SysV, Linux) and
>     `imaxbel` (`IMAXBEL`, BSD/Linux).
>   - `MD_OUT` (`c_oflag`): `opost`, `olcuc`, `onlcr`, `ocrnl`, `onocr`,
>     `onoeot`, `onlret`, `ofill`, `ofdel`, `nldly`, `crdly`, `tabdly`,
>     `xtabs`, `bsdly`, `vtdly`, `ffdly`, `pageout`, `wrap`. Only `opost` is
>     base POSIX; `onlcr`, `ocrnl`, `onocr`, `onlret`, `ofill`, `ofdel`,
>     `nldly`, `crdly`, `tabdly`, `bsdly`, `vtdly`, `ffdly` are XSI; `olcuc`
>     is legacy SysV; `onoeot` is BSD; `xtabs` (`XTABS`/`OXTABS`) is not POSIX
>     — it is a `TABDLY` value, not an independent bit, so `+xtabs` and
>     `+tabdly` interact; `pageout` and `wrap` are non-POSIX and exist almost
>     nowhere.
>   - `MD_CTL` (`c_cflag`): `cignore`, `cbaud`, `cstopb`, `cread`, `parenb`,
>     `parodd`, `hupcl`, `clocal`, `loblk`, `cibaud`, then either
>     `ccts_oflow` or `crtscts` (the former when `CCTS_OFLOW` is defined and
>     `CRTSCTS` is too), `crts_iflow`, `cdtrcts`, `mdmbuf`, `rcv1en`,
>     `xmt1en`. Only `cstopb`, `cread`, `parenb`, `parodd`, `hupcl` and
>     `clocal` are POSIX; the rest are BSD or SysV extensions, and `cbaud`/
>     `cibaud` overlap the speed encoding, so flipping them corrupts the line
>     speed.
>   - `MD_LIN` (`c_lflag`): `isig`, `icanon`, `xcase`, `echo`, `echoe`,
>     `echok`, `echonl`, `noflsh`, `tostop`, `echoctl`, `echoprt`, `echoke`,
>     `defecho`, `flusho`, `pendin`, `iexten`, `nokerninfo`, `altwerase`,
>     `extproc`. POSIX: `isig`, `icanon`, `echo`, `echoe`, `echok`, `echonl`,
>     `noflsh`, `tostop`, `iexten`; `xcase` is legacy XSI; the remainder are
>     BSD/Linux extensions.
>   - `MD_CHAR` (bits over the `C_*` indices): `intr`, `quit`, `erase`, `kill`,
>     `eof`, `eol`, `eol2`, `swtch`, `dswtch`, `erase2`, `start`, `stop`,
>     `werase`, `susp`, `dsusp`, `reprint`, `discard`, `lnext`, `status`,
>     `page`, `pgoff`, `kill2`, `brk`, `min`, `time` — each present only if the
>     matching `V*` macro is defined. For this group a `+` mask bit means "this
>     char is libedit's, do not adopt the user's value" and a `-` mask bit
>     means "force this char to `t_vdisable`"; see `tty_update_char`.

> [spec:libedit:def:tty.tty-update-char-fn]
> static void tty_update_char(EditLine *el, int mode, int c)

> [spec:libedit:sem:tty.tty-update-char-fn]
> Reconciles one control character of one mode's row against the terminal
> snapshot, applying the `MD_CHAR` mask pair for that mode. `mode` is `EX_IO`
> or `ED_IO`; `c` is a `C_*` index in `0 .. C_NCC-1`. Returns nothing; cannot
> fail. Two independent steps, in this order:
>   1. If `(el->el_tty.t_t[mode][MD_CHAR].t_setmask & C_SH(c)) == 0` **and**
>      `el->el_tty.t_c[TS_IO][c] != el->el_tty.t_c[EX_IO][c]`, then
>      `el->el_tty.t_c[mode][c] = el->el_tty.t_c[TS_IO][c]`.
>      Reading: a `t_setmask` bit in the `MD_CHAR` column marks the char as
>      libedit's own for that mode, so user changes are *not* adopted; and the
>      change is only interesting if the terminal's current value differs from
>      what libedit last pushed as the execute-mode value.
>   2. If `(el->el_tty.t_t[mode][MD_CHAR].t_clrmask & C_SH(c)) != 0`, then
>      `el->el_tty.t_c[mode][c] = el->el_tty.t_vdisable`, unconditionally
>      overwriting whatever step 1 may have just written. Clear beats adopt.
> `C_SH(A)` is `((unsigned int)(1 << A))`; with `C_NCC == 25` every index fits
> in the 32-bit mask, and the shift is well defined for all valid `c`.
> With the compiled-in tables, `EX_IO`'s `MD_CHAR` masks are both 0, so for
> that mode this reduces to "copy the snapshot row entry into the execute row
> whenever it differs" — which, called for every index, copies the whole row.
> `ED_IO`'s `t_setmask` pins `C_MIN`, `C_TIME`, `C_SWTCH`, `C_DSWTCH`,
> `C_SUSP`, `C_DSUSP`, `C_EOL`, `C_DISCARD`, `C_PGOFF`, `C_PAGE` and
> `C_STATUS` against adoption, and its `t_clrmask` is 0, so nothing is forced
> to the disable byte unless `setty` adds a `-` entry.
> Note step 1's comparison for `mode == EX_IO` is against the same row it
> writes; that is fine, since the write makes the two equal.

> [spec:libedit:def:tty.tty-update-flag-fn]
> static tcflag_t tty_update_flag(EditLine *el, tcflag_t f, int mode, int kind)

> [spec:libedit:sem:tty.tty-update-flag-fn]
> Pure function that applies one mode/group mask pair to a flag word:
>   `f &= ~el->el_tty.t_t[mode][kind].t_clrmask;`
>   `f |=  el->el_tty.t_t[mode][kind].t_setmask;`
>   `return f;`
> Clear happens before set, so if a bit appears in both masks the set wins.
> Bits in neither mask are passed through unchanged — that is the mechanism by
> which the terminal's own settings survive into libedit's modes.
> `mode` is `EX_IO`, `ED_IO` or `QU_IO`; `kind` is `MD_INP`, `MD_OUT`,
> `MD_CTL` or `MD_LIN` (`MD_CHAR` masks are bitmaps over `C_*` indices and are
> never fed here). No `struct termios` is touched and nothing is pushed to the
> terminal. It reads the live, per-EditLine copy of the table, so any `setty`
> edit is reflected on the next call.

> [spec:libedit:def:tty.tty-update-flags-fn]
> static void tty_update_flags(EditLine *el, int kind)

> [spec:libedit:sem:tty.tty-update-flags-fn]
> Re-derives one flag word of both the execute and edit termios from the
> terminal snapshot, so that changes the application made to the terminal
> while libedit was cooked are carried into both libedit modes. Called only
> from `tty_rawmode`, and only when the snapshot was in canonical mode.
> Let `tt`, `ed`, `ex` be pointers to the `kind` flag word of
> `el->el_tty.t_ts`, `t_ed` and `t_ex` respectively (via `tty__get_flag`, so
> `kind` must be one of `MD_INP`/`MD_OUT`/`MD_CTL`/`MD_LIN` — `MD_CHAR` would
> abort the process).
> If `*tt != *ex` **and** (`kind != MD_CTL` **or** `*tt != *ed`):
>   `*ed = tty_update_flag(el, *tt, ED_IO, kind);`
>   `*ex = tty_update_flag(el, *tt, EX_IO, kind);`
> Otherwise nothing happens.
> Reading of the guard: if the terminal's word still equals the execute word,
> nothing changed since libedit last wrote it, so there is nothing to adopt.
> The extra `MD_CTL` condition exists because libedit itself writes `c_cflag`
> — the speed bits, via `cfsetispeed`/`cfsetospeed` in `tty_rawmode` — into
> both `t_ex` and `t_ed`, so a `c_cflag` difference is only believed when the
> snapshot differs from *both* stored words. The C does not document this; do
> not "simplify" the condition.
> Note both new values are derived from `*tt`, the snapshot, not from the
> previous `*ed`/`*ex`, so any bit libedit had set that is in neither mask of
> the target mode is discarded in favour of the terminal's value.
> Returns nothing; cannot fail. Nothing is pushed to the terminal here —
> `tty_rawmode` does that afterwards.

> [spec:libedit:def:tty.ttychar-t-nn-io-c-ncc]
> typedef unsigned char ttychar_t[NN_IO][C_NCC]

> [spec:libedit:def:tty.ttymap-t]
> typedef struct ttymap_t

> [spec:libedit:def:tty.ttymodes-t]
> typedef struct ttymodes_t

> [spec:libedit:def:tty.ttyperm-t-nn-io-md-nn]
> typedef struct
