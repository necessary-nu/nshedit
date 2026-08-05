# src/el.c, src/el.h

> [spec:libedit:def:el.coord-t]
> typedef struct coord_t

> [spec:libedit:def:el.editline]
> struct editline {
>   wchar_t *el_prog;
>   FILE *el_infile;
>   FILE *el_outfile;
>   FILE *el_errfile;
>   int el_infd;
>   int el_outfd;
>   int el_errfd;
>   int el_flags;
>   coord_t el_cursor;
>   wint_t **el_display;
>   wint_t **el_vdisplay;
>   void *el_data;
>   el_line_t el_line;
>   el_state_t el_state;
>   el_terminal_t el_terminal;
>   el_tty_t el_tty;
>   el_refresh_t el_refresh;
>   el_prompt_t el_prompt;
>   el_prompt_t el_rprompt;
>   el_literal_t el_literal;
>   el_chared_t el_chared;
>   el_map_t el_map;
>   el_keymacro_t el_keymacro;
>   el_history_t el_history;
>   el_search_t el_search;
>   el_signal_t el_signal;
>   struct el_read_t *el_read;
>   ct_buffer_t el_visual;
>   ct_buffer_t el_scratch;
>   ct_buffer_t el_lgcyconv;
>   LineInfo el_lgcylinfo;
> }

> [spec:libedit:def:el.editline.el-getenv-fn]
> char * (*el_getenv)(const char *)

> [spec:libedit:sem:el.editline.el-getenv-fn]
> The environment-lookup hook. Every environment variable libedit reads
> on behalf of an EditLine goes through this function pointer rather
> than calling `getenv` directly, so an embedding application can
> substitute its own environment.
>
> Contract: called with a NUL-terminated narrow variable name, returns a
> pointer to that variable's NUL-terminated value, or NULL if the
> variable is unset (or if the hook declines to answer). The result is
> borrowed, never freed by libedit and never retained past the call that
> obtained it — `el_source` uses the `HOME` value only to build a path
> string and the `EDITRC` value only as an immediate `fopen` argument,
> `terminal_set` copies the `TERM` value into its own storage, and
> `vi_histedit` passes the `EDITOR` value straight to the command it
> builds. A hook returning a pointer into storage it reuses must keep
> that storage valid at least until the next hook call.
>
> Lifecycle:
> - `el_init_internal` sets it to `secure_getenv` when the EditLine is
>   constructed. That default is load-bearing for security: in a set-uid
>   or set-gid process `secure_getenv` returns NULL for everything, so
>   such a process reads none of `EDITRC`, `HOME`, `TERM` or `EDITOR`
>   from an untrusted environment. Replacing the hook with plain
>   `getenv` removes that protection, which is the caller's choice to
>   make. See `[spec:libedit:sem:el.secure-getenv-fn]`.
> - `el_set(el, EL_GETENV, fn)` replaces it, with no NULL check: storing
>   NULL makes the next lookup an indirect call through a null pointer.
> - `el_get(el, EL_GETENV, &fn)` reads it back.
> - `el_end` does not free it and never calls it.
>
> The complete set of call sites in the library is: `el_source` looks up
> `"EDITRC"` and then `"HOME"`; `terminal_set` looks up `"TERM"` when
> called with a NULL terminal name; `vi_histedit` looks up `"EDITOR"`.
> Nothing else in libedit reads the environment. The port must route
> exactly these four lookups through the hook and no others, because an
> application that installs a hook is entitled to see precisely this
> call pattern.

> [spec:libedit:def:el.el-action-t]
> typedef unsigned char el_action_t

> [spec:libedit:def:el.el-beep-fn]
> void el_beep(EditLine *el)

> [spec:libedit:sem:el.el-beep-fn]
> Rings the terminal bell. The whole body is `terminal_beep(el);` — a
> public re-export of the internal call, with no added logic.
>
> `terminal_beep` emits the terminal's audible-bell capability if the
> loaded terminal description has a non-empty one, and otherwise writes
> a literal ASCII BEL (0x07); see
> `[spec:libedit:sem:terminal.terminal-beep-fn]`.
>
> Returns void, reports no errors, does not flush the output (so the
> bell may not reach the terminal until the next refresh or flush), and
> does not touch the cursor position or any editing state. There is no
> NULL check on `el`.

> [spec:libedit:def:el.el-editmode-fn]
> libedit_private int /*ARGSUSED*/ el_editmode(EditLine *el, int argc, const wchar_t **argv)

> [spec:libedit:sem:el.el-editmode-fn]
> Implements the `edit` builtin — the command form of `EL_EDITMODE`,
> reached from an `.editrc` line or from `el_parse`. It is
> `libedit_private`, dispatched through the builtin command table, so
> `argv[0]` is `L"edit"` and `argv[1]` is the single expected operand.
>
> Step 1: if `argv` is NULL, or `argc` is not exactly 2, or `argv[1]` is
> NULL, return -1 with no message and no state change. Exactly one
> operand is required: bare `edit` and `edit on somethingelse` are both
> rejected.
> Step 2: compare `argv[1]` against `L"on"` with `wcscmp` — exact wide
> string equality, case-sensitive, no abbreviations. If it matches:
> clear `EDIT_DISABLED` (0x004) in `el_flags`, then call
> `tty_rawmode(el)`. That order is required: `tty_rawmode` returns 0
> immediately while `EDIT_DISABLED` is set, so the flag must come down
> before the mode change is attempted.
> Step 3: otherwise compare against `L"off"`. If it matches: call
> `tty_cookedmode(el)` first, then set `EDIT_DISABLED`. The mirror of
> the same constraint — `tty_cookedmode` also bails out while
> `EDIT_DISABLED` is set, so the terminal must be restored before the
> flag goes up. Getting either order wrong leaves the terminal in the
> wrong mode with no diagnostic.
> Step 4: otherwise write ``edit: Bad value `%ls'.\n`` to `el_errfile`,
> substituting `argv[1]`, and return -1. This is the only case that
> produces output; the `fprintf` result is discarded.
> Step 5: return 0 for both `on` and `off`.
>
> The return values of `tty_rawmode` and `tty_cookedmode` are discarded,
> so a failure to change the terminal mode still reports success — the
> flag has been updated regardless, leaving `el_flags` and the actual
> terminal state out of step.
>
> The `/*ARGSUSED*/` marker in the C is stale: every parameter is used.

> [spec:libedit:def:el.el-end-fn]
> void el_end(EditLine *el)

> [spec:libedit:sem:el.el-end-fn]
> Destroys an EditLine: restores the terminal, tears down every
> subsystem, frees the conversion buffers and the object itself.
> Returns void.
>
> Step 1: if `el` is NULL, return. This is the only NULL-tolerant entry
> point in the file.
>
> Step 2: `el_reset(el)` — put the terminal back in cooked mode and
> reset the editing line and state. This runs first so that the terminal
> modes are restored while the tty subsystem is still live.
>
> Step 3: tear down the subsystems in exactly this order:
> 1. `terminal_end(el)` — frees the capability pool, the capability
>    tables, the function-key table, and the display buffers
>    (`el_display` / `el_vdisplay`), which is why those two fields are
>    not freed explicitly below.
> 2. `keymacro_end(el)`
> 3. `map_end(el)` — also frees the `wcsdup`'d name and description of
>    every function added through `EL_ADDFN`.
> 4. `tty_end(el, TCSAFLUSH)`, but only if `NO_TTY` (0x002) is clear.
>    `TCSAFLUSH` means the original termios is restored after pending
>    output drains and pending unread input is discarded.
> 5. `ch_end(el)`
> 6. `read_end(el)`
> 7. `search_end(el)`
> 8. `hist_end(el)`
> 9. `prompt_end(el)` — a no-op.
> 10. `sig_end(el)`
> 11. `literal_end(el)`
>
> IMPORTANT: this is *not* the reverse of the construction order. It is
> the same forward order as `el_init_internal`'s step 6, with one
> exception: `read_end` runs sixth here but `read_init` runs eleventh
> (last) there. Teardown therefore runs first-initialised-first, which
> is the opposite of what a Rust `Drop` order would give for a struct
> whose fields are declared in init order. Nothing in the current
> subsystem set actually depends on the teardown order — every `*_end`
> touches only its own fields and every one of them is idempotent
> (pointers are freed and then nulled) except `read_end` — but the port
> should not silently swap the order, since a future subsystem
> interaction would then differ.
>
> Step 4: free the object's own allocations, in this order: `el_prog`,
> `el_visual.cbuff`, `el_visual.wbuff`, `el_scratch.cbuff`,
> `el_scratch.wbuff`, `el_lgcyconv.cbuff`, `el_lgcyconv.wbuff`, and
> finally `el` itself. `el_lgcylinfo` needs no free — its three `char *`
> members point into `el_lgcyconv.cbuff`, which was just released, and
> they are left dangling for the instant before the object goes away.
>
> Ownership, explicitly:
> - `el_infile`, `el_outfile` and `el_errfile` are NOT `fclose`d, and
>   `el_infd` / `el_outfd` / `el_errfd` are NOT closed. They were
>   borrowed at construction and go back to the caller untouched.
> - `el_data` (whatever was stored via `EL_CLIENTDATA`) is not freed and
>   not inspected.
> - The `el_getenv` hook is not called and not freed.
> - Callbacks installed through `EL_PROMPT`, `EL_RESIZE`, `EL_ALIAS_TEXT`,
>   `EL_HIST` and `EL_GETCFN`, and their opaque arguments, are dropped
>   without notification; there is no destructor callback.
> - The history object behind `EL_HIST` belongs to the caller and must
>   be destroyed separately with `history_end`.
>
> Not idempotent and not re-entrant: the object is freed, so a second
> `el_end` on the same pointer is a use-after-free, and there is no
> "already ended" marker to test. Any `LineInfoW` or `LineInfo` view
> handed out by `el_wline` / `el_line`, and any `const char *` obtained
> from `EL_TERMINAL` or `EL_WORDCHARS`, dangles from this point on.
>
> Two failure modes inherited from step 3:
> - `read_end` dereferences `el->el_read` without a NULL check, so
>   calling `el_end` on an object whose `read_init` never succeeded
>   faults. See `[spec:libedit:sem:el.el-init-internal-fn]`.
> - The original terminal modes can go unrestored for three independent
>   reasons: `NO_TTY` set (step 3.4 skipped entirely), or `tty_end`'s
>   own early exits when `EDIT_DISABLED` is set or when
>   `el_tty.t_initialized` is still 0. A `TERM=emacs` EditLine hits the
>   second of those. `tty_end` also discards the result of its
>   `tcsetattr`, so a failure to restore is silent.

> [spec:libedit:def:el.el-init-fd-fn]
> EditLine * el_init_fd(const char *prog, FILE *fin, FILE *fout, FILE *ferr, int fdin, int fdout, int fderr)

> [spec:libedit:sem:el.el-init-fd-fn]
> Constructs an EditLine from three stdio streams plus three explicitly
> supplied file descriptors, for callers whose streams and descriptors
> are not related by `fileno` — for example a stream wrapped around a
> pty whose descriptor the caller wants libedit to use for `ioctl` and
> `isatty`.
>
> The entire body is one tail call:
> `el_init_internal(prog, fin, fout, ferr, fdin, fdout, fderr, 0)`.
> The trailing 0 is the initial `el_flags` word, so an EditLine built
> this way starts with every flag clear: signal handling off, tty
> assumed usable, editing enabled, buffered, wide history, tty reset on
> setup enabled, `FIXIO` off. Its return value is returned unchanged: a
> fresh `EditLine *`, or NULL on failure.
>
> Nothing is validated and no consistency is enforced between the
> streams and the descriptors — a caller may pass descriptors that
> belong to entirely different files, and libedit will read and write
> through the streams while querying terminal size and modes through the
> descriptors. `el_init` is the special case where the descriptors are
> `fileno` of the streams.
>
> The streams and descriptors are borrowed, never duplicated and never
> closed by libedit; the caller owns them for the whole lifetime of the
> EditLine and afterwards.

> [spec:libedit:def:el.el-init-fn]
> EditLine * el_init(const char *prog, FILE *fin, FILE *fout, FILE *ferr)

> [spec:libedit:sem:el.el-init-fn]
> Constructs an EditLine from three stdio streams, deriving the three
> file descriptors from them.
>
> The entire body is one tail call:
> `el_init_fd(prog, fin, fout, ferr, fileno(fin), fileno(fout), fileno(ferr))`.
> Its return value is returned unchanged: a fresh `EditLine *`, or NULL
> if construction failed.
>
> Nothing is validated. In particular:
> - A NULL `fin`, `fout` or `ferr` is passed to `fileno`, which is
>   undefined behaviour (a null dereference in practice). The port must
>   treat NULL streams as a caller error; there is no defined result to
>   reproduce.
> - `fileno` on a stream that is not open returns -1 and sets `errno`.
>   That -1 is stored as the descriptor without diagnosis, and
>   construction still reports success. The EditLine then holds an
>   invalid descriptor that later `isatty`/`ioctl`/`read`/`write` calls
>   fail on — `tty_init` fails, so `NO_TTY` gets set, but nothing else
>   notices.
> - The evaluation order of the three `fileno` calls is unspecified in
>   C. It has no observable consequence here because `fileno` on
>   distinct streams has no interacting side effects.
>
> `prog` is borrowed for the duration of the call only (see
> `[spec:libedit:sem:el.el-init-internal-fn]`, which copies it). The
> three `FILE *` handles are retained by the EditLine but not owned:
> libedit never `fclose`s them and never closes the descriptors, so the
> caller must keep them alive for the lifetime of the EditLine and is
> responsible for closing them afterwards.

> [spec:libedit:def:el.el-init-internal-fn]
> libedit_private EditLine * el_init_internal(const char *prog, FILE *fin, FILE *fout, FILE *ferr, int fdin, int fdout, int fderr, int flags)

> [spec:libedit:sem:el.el-init-internal-fn]
> The real constructor. Allocates the `struct editline`, records the
> streams and descriptors, copies the program name, and brings up every
> subsystem in a fixed order. Returns the new `EditLine *`, or NULL if
> construction failed. `flags` is the initial value of `el_flags`;
> `el_init_fd` passes 0 and the readline compatibility layer passes
> `NO_RESET` (0x080), which is the only reason this entry point exists
> separately from `el_init_fd`.
>
> Step 1: `el = el_calloc(1, sizeof(struct editline))`. Zero-filled, so
> every pointer field starts NULL, every counter 0, and every embedded
> subsystem struct starts blank. If it returns NULL, return NULL. From
> here on "the object" means this allocation.
>
> Step 2: store the three streams into `el_infile`, `el_outfile`,
> `el_errfile` and the three descriptors into `el_infd`, `el_outfd`,
> `el_errfd`. No duplication, no validation, no ownership taken.
>
> Step 3: `el->el_getenv = secure_getenv` — the default environment
> hook. See `[spec:libedit:sem:el.editline.el-getenv-fn]`.
>
> Step 4: `el->el_prog = wcsdup(ct_decode_string(prog, &el->el_scratch))`.
> Two operations: decode `prog` from the current locale's multibyte
> encoding into the object's shared scratch conversion buffer
> (`el_scratch`, allocated on this first use to roughly the decoded
> length plus 1024 `wchar_t`), then take a private heap copy of that
> wide string, which the object owns until `el_end`. The program name is
> what the `prog:` prefix in an `.editrc` line is matched against.
> If the copy is NULL, `el_free(el)` and return NULL.
> IMPORTANT: the NULL test covers only `wcsdup`'s allocation failure.
> `ct_decode_string` returns NULL when `prog` is NULL or when `prog` is
> not a valid multibyte string in the current `LC_CTYPE` locale, and
> that NULL is handed straight to `wcsdup` — undefined behaviour, a
> crash on the usual implementations. The port must reject a NULL or
> undecodable `prog` by failing construction; there is no defined C
> behaviour to preserve.
> LEAK: on this failure path `el_free(el)` releases the object but not
> `el->el_scratch.wbuff`, which step 4 just allocated. That buffer is
> only ever freed by `el_end`, which is not called here. The port
> obviously must not reproduce the leak.
>
> Step 5: `el->el_flags = flags`. This happens *before* any subsystem
> init, which matters because `terminal_init` can raise `EDIT_DISABLED`
> (it does so when `TERM` is `emacs`) and `tty_init` can raise `NO_TTY`;
> both would be wiped out if the assignment came later.
>
> Step 6: initialise the subsystems in exactly this order. The C
> comments it "Order is important!!!".
> 1. `terminal_init(el)`. The only init whose failure is fatal. On -1:
>    `el_free(el->el_prog)`, `el_free(el)`, return NULL — with the same
>    `el_scratch.wbuff` leak as step 4, and note that `terminal_init`
>    has already released its own partial allocations before returning
>    -1, so nothing of the terminal layer leaks. Success also loads the
>    capabilities for `$TERM` (via the `el_getenv` hook) and may set
>    `EDIT_DISABLED`.
> 2. `keymacro_init(el)` — result discarded.
> 3. `map_init(el)` — result discarded. Must follow `terminal_init`:
>    `terminal_init` calls `terminal_bind_arrow`, which bails out while
>    `el_map.key` is still NULL, and that early exit is what stops it
>    from installing garbage bindings from the not-yet-populated
>    function-key table (see
>    `[spec:libedit:sem:terminal.terminal-init-fn]`). The ordering is
>    load-bearing by accident, not by design.
> 4. `tty_init(el)`. If it returns -1, `el->el_flags |= NO_TTY` (0x002).
>    This is the only init whose failure is recorded rather than
>    ignored, and the flag is consulted exactly once, by `el_end`, to
>    decide whether to restore the terminal modes. `tty_init` returns 0
>    without doing anything when `EDIT_DISABLED` is already set, so a
>    `TERM=emacs` EditLine ends up with `NO_TTY` clear but
>    `el_tty.t_initialized` still 0.
> 5. `ch_init(el)` — result discarded. Must follow `map_init`, because
>    it sets `el_map.current = el_map.key`.
> 6. `search_init(el)` — result discarded.
> 7. `hist_init(el)` — result discarded.
> 8. `prompt_init(el)` — result discarded (it cannot fail).
> 9. `sig_init(el)` — result discarded.
> 10. `literal_init(el)` — returns void.
> 11. `read_init(el)`. On -1: `el_end(el)`, return NULL.
>
> Step 7: return `el`.
>
> Partial-init failure, in full — this is the part a re-implementation
> must get right, because the C gets it wrong:
> - Steps 6.2, 6.3 and 6.5 through 6.9 all return -1 on allocation
>   failure and all have their results discarded. The object is then
>   returned to the caller as a success with NULL buffers in it:
>   `el_keymacro.buf`; `el_map.alt` / `key` / `help` / `func`;
>   `el_line.buffer` and the `el_chared` undo/redo/kill buffers;
>   `el_search.patbuf`; `el_history.buf`; `el_signal`. Each of those is
>   dereferenced without a guard the first time the corresponding
>   feature is used, so an out-of-memory during construction turns into
>   a crash somewhere else entirely. The port must fail construction
>   instead.
> - CRASH: `read_init` failure is not survivable. `read_init` returns -1
>   either without having allocated `el_read` at all, or after calling
>   `read_end` itself, which frees `el_read` and sets it to NULL. Either
>   way `el->el_read` is NULL on entry to the `el_end(el)` cleanup, and
>   `el_end` calls `read_end` unconditionally, whose first act
>   dereferences `el->el_read`. So the documented "returns NULL because
>   `read_init` failed" outcome is unreachable: the process faults
>   first. It is also a double `read_end` on the second of the two
>   paths. The port must make teardown tolerate an uninitialised read
>   subsystem and must actually return NULL here.
> - No init after a silent failure is skipped; the sequence always runs
>   to completion.
>
> Locale: this function never calls `setlocale`. It inherits whatever
> `LC_CTYPE` the application has installed, and step 4's `mbstowcs`
> resolves against it at call time — so constructing an EditLine before
> `setlocale(LC_CTYPE, "")` decodes the program name in the C locale.
> There is no codeset detection anywhere in this file: `<langinfo.h>` is
> included but `nl_langinfo` is never called, and `<locale.h>` is
> included but nothing from it is used. The single locale-sensitive
> decision in el.c is the `MB_CUR_MAX` test in `el_wset`'s `EL_HIST`
> case. `MAXPATHLEN` is likewise defined at the top of the file and
> never used. None of the three carries over to the port.

> [spec:libedit:def:el.el-line-t]
> typedef struct el_line_t

> [spec:libedit:def:el.el-reset-fn]
> void el_reset(EditLine *el)

> [spec:libedit:sem:el.el-reset-fn]
> Abandons whatever line is being edited and puts the terminal back the
> way the application had it. Two calls, in this order:
>
> Step 1: `tty_cookedmode(el)` — restore the saved `EX_IO` termios
> settings with `TCSADRAIN`. It is a no-op if the terminal is already in
> `EX_IO` mode or if `EDIT_DISABLED` is set, and its return value is
> discarded here, so a failure to restore the modes is silent.
> Step 2: `ch_reset(el)` — reset the character editor: cursor and
> `lastchar` back to the start of the line buffer (discarding the line's
> contents without freeing anything), undo length -1 and undo cursor 0,
> vi pending command cleared to `NOP` at the buffer start, kill mark
> back to the buffer start, `el_map.current` back to `el_map.key` (the
> normal, non-alt keymap), input mode back to `MODE_INSERT`, `doingarg`
> and `metanext` cleared, numeric `argument` back to 1, `lastcmd` back
> to `ED_UNASSIGNED`, and the history event number back to 0.
> The C carries an `XXX: Do we want that?` comment on step 2; the answer
> is frozen either way, because the behaviour crosses the ABI.
>
> The order matters: `tty_cookedmode` must run before the editor state
> is discarded, since it is the last chance to drain output written for
> the line being abandoned.
>
> Returns void. There is no NULL check on `el` — passing NULL faults.
> Called by the application between lines, and by `el_end` as its first
> act.

> [spec:libedit:def:el.el-resize-fn]
> void el_resize(EditLine *el)

> [spec:libedit:sem:el.el-resize-fn]
> Re-reads the terminal's window size and rebuilds the display buffers
> if it changed, with `SIGWINCH` blocked across the whole operation.
> Returns void.
>
> Step 1: build a signal set containing only `SIGWINCH` (`sigemptyset`
> then `sigaddset`) and block it with
> `sigprocmask(SIG_BLOCK, &nset, &oset)`, saving the previous mask in
> `oset`. All three results are discarded. The point is to keep
> libedit's own `SIGWINCH` handler from re-entering the terminal layer
> while the display buffers are being reallocated underneath it.
>
> Step 2: `if (terminal_get_size(el, &lins, &cols)) terminal_change_size(el, lins, cols);`
> - `terminal_get_size` seeds `cols` and `lins` from the currently
>   loaded capability values for columns and lines, then overrides them
>   from `TIOCGWINSZ` on `el_infd` (and from `TIOCGSIZE` on platforms
>   that have it), ignoring any field the kernel reports as zero and
>   ignoring `ioctl` failure entirely. It returns non-zero exactly when
>   at least one of the two differs from the stored capability value.
>   Note the query goes to the *input* descriptor, not the output one
>   that `tty_setup` uses for its `isatty` check.
> - So `terminal_change_size` runs only when the size actually changed.
>   It clamps first (`cols < 2` becomes 80, `lins < 1` becomes 24),
>   stores the new values into the capability table, rebuilds the real
>   and virtual display buffers, clears the display record, and restores
>   the saved cursor coordinates. Its return value (-1 if the buffer
>   rebuild failed) is discarded here, so an out-of-memory during a
>   resize is silent and leaves the display state inconsistent.
>
> Step 3: `sigprocmask(SIG_SETMASK, &oset, NULL)` — restore exactly the
> mask that was in effect on entry. Deliberately `SIG_SETMASK` rather
> than `SIG_UNBLOCK`, so a caller who already had `SIGWINCH` blocked
> still has it blocked afterwards.
>
> Two caveats for the port. First, `sigprocmask` in a multithreaded
> process has unspecified behaviour under POSIX; `pthread_sigmask` is
> the defined equivalent, and libedit uses the former. Second, blocking
> a signal does not discard it — a `SIGWINCH` that arrives during the
> critical section is delivered as soon as step 3 unblocks it — so this
> only makes the query-and-rebuild atomic with respect to the handler,
> it does not lose resize events.
>
> There is no NULL check on `el`. The application must call this itself
> whenever the terminal is resized unless `EL_SIGNAL` is enabled, in
> which case libedit's own handler does it.

> [spec:libedit:def:el.el-source-fn]
> int el_source(EditLine *el, const char *fname)

> [spec:libedit:sem:el.el-source-fn]
> Reads an `.editrc` file and executes each line as an editline
> configuration command.
>
> File lookup, in order:
> Step 1: if `fname` is non-NULL it is used exactly as given — no `~`
> expansion, no search path, no directory prefix.
> Step 2: if `fname` is NULL, ask the instance's environment hook for
> `"EDITRC"`. If it answers non-NULL, that string is the file name,
> again used verbatim. Because the default hook is `secure_getenv`, a
> set-uid/set-gid process gets NULL here and so never honours `EDITRC`.
> Step 3: otherwise ask the hook for `"HOME"`. If that is NULL, return
> -1 immediately, having allocated nothing.
> Step 4: allocate a path buffer of `strlen(HOME) + sizeof("/.editrc")`
> bytes — i.e. `strlen(HOME) + 9`, exactly enough — with `el_calloc`. On
> failure return -1. Format into it with `snprintf` as `HOME`
> concatenated with `"/.editrc"`, except that when `HOME` is the empty
> string the leading `/` is skipped, producing the *relative* path
> `.editrc`, which `fopen` then resolves against the current working
> directory. That empty-`HOME` case is the only way this function ever
> looks at the current directory.
> IMPORTANT: `histedit.h` documents this function as sourcing
> "$PWD/.editrc or $HOME/.editrc". It does not: there is no `$PWD`
> lookup and no attempt at `./.editrc`. The vestige of the removed
> attempt is still visible in the C as a `fp = NULL;` initialisation
> followed by a redundant `if (fp == NULL)` before the only `fopen`.
> The header comment is wrong; the code is the contract.
>
> Step 5: if the resolved name's first byte is `'\0'`, return -1. This
> only rejects a caller-supplied `""` or an `EDITRC` set to `""`; a
> constructed path is never empty, so no allocation is leaked here.
> Step 6: `fopen(fname, "r")`. On failure, free the path buffer if one
> was allocated and return -1. `errno` is left as `fopen` set it but is
> not reported.
>
> Line loop, using `getline` with an initially NULL buffer and zero
> size, so one heap buffer is allocated on the first line and reused and
> grown for the rest. For each line returned, with `slen` its length:
> a. If the first byte is `'\n'`, skip the line. Note this tests only
>    the first byte, so a truly empty line is skipped but a line of
>    spaces is not, and a CRLF file's `"\r\n"` line is not.
> b. If `slen > 0` and the last byte is `'\n'`, overwrite it with
>    `'\0'`. A final line with no trailing newline keeps all its bytes.
>    No other trailing whitespace is stripped, and `'\r'` is not.
> c. Decode the line from the current `LC_CTYPE` locale's multibyte
>    encoding into the EditLine's shared scratch buffer with
>    `ct_decode_string(ptr, &el->el_scratch)`. If that returns NULL —
>    invalid multibyte sequence, or allocation failure — skip the line
>    silently, with no diagnostic and no effect on the return value. An
>    embedded NUL byte truncates the line at that point as far as
>    decoding is concerned; the remainder is never seen.
> d. Advance past leading `iswspace` characters, stopping at the
>    terminating NUL.
> e. If the first non-blank character is `L'#'`, skip the line: comment.
> f. `error = parse_line(el, dptr)`, which tokenises the wide line and
>    runs it through `el_wparse`. If the result is -1, break out of the
>    loop; otherwise continue with the next line.
>
> Step 7: free the `getline` buffer, free the path buffer if one was
> allocated, `fclose` the file with the result discarded, and return
> `error`.
>
> Return value — this is subtler than it looks, because `error` is
> assigned by every line that reaches step f and is therefore the result
> of the LAST such line, not an accumulation:
> - 0 if no line ever reached step f (empty file, or every line blank,
>   comment or undecodable); or if the last line that did was executed
>   successfully; or if the last such line carried a `prog:` prefix that
>   did not match `el_prog` and was therefore skipped by `el_wparse`.
> - 1 if the last such line named a known builtin and that builtin
>   failed. `el_wparse` returns the negation of the builtin's result, so
>   a builtin returning -1 surfaces here as +1. A caller testing
>   `!= 0` sees the failure; a caller testing `== -1` does not.
> - -1 if some line named an unknown command or tokenised to zero words.
>   That aborts the loop immediately and every later line in the file is
>   never read, so a typo part-way through an `.editrc` silently
>   discards the rest of it.
>   IMPORTANT: a whitespace-only line takes exactly that path. It
>   survives step a (its first byte is not `'\n'`), decodes fine, is not
>   a comment, and tokenises to zero words, which `el_wparse` rejects
>   with -1. So a single line containing only spaces or tabs stops
>   sourcing and makes `el_source` return -1. Blank-looking lines are
>   only safe when they are completely empty.
> - -1 also for each early exit: `fname` NULL with neither `EDITRC` nor
>   `HOME` available; path allocation failure; empty file name; `fopen`
>   failure.
> There is no way for the caller to distinguish "could not open the
> file" from "a line failed to parse"; both are -1.
>
> Ownership: the constructed path buffer is libedit's and is freed on
> every path that allocates it. The `FILE` is opened and closed here.
> The caller's `fname` is borrowed for the call only. The decoded line
> lives in `el_scratch`, which belongs to the EditLine and is freed by
> `el_end` — so `el_source` overwrites the shared scratch buffer, and
> anything else holding a pointer previously returned from
> `ct_decode_string` on the same EditLine has it invalidated. There is
> no NULL check on `el`.

> [spec:libedit:def:el.el-state-t]
> typedef struct el_state_t

> [spec:libedit:def:el.el-wget-fn]
> int el_wget(EditLine *el, int op, ...)

> [spec:libedit:sem:el.el-wget-fn]
> The wide-character parameter getter, the read side of `el_wset`. Same
> varargs dispatch shape, but every argument is a pointer to the type
> the corresponding `el_wset` op takes by value, and the result is
> stored through it.
>
> Frame: if `el` is NULL, return -1 immediately without starting the
> varargs. Otherwise `va_start`, dispatch, `va_end`, return `rv`. `rv`
> is declared uninitialised, which is safe only because every arm
> including `default` assigns it. Unrecognised ops set `rv = -1` and
> consume no varargs. The out-pointers are written only on the paths
> noted below; on a -1 return the caller's storage is generally
> untouched.
>
> Ops, by code:
>
> - `EL_PROMPT` (0) and `EL_RPROMPT` (12) — one `el_pfunc_t *`. Calls
>   `prompt_get(el, p, NULL, op)`: returns -1 if `p` is NULL; otherwise
>   stores the selected prompt's function pointer and returns 0. The
>   escape character is not retrieved because the character out-pointer
>   is NULL. Note the value returned may be the internal default
>   (`prompt_default` / `prompt_default_r`) rather than anything the
>   application installed.
> - `EL_PROMPT_ESC` (21) and `EL_RPROMPT_ESC` (22) — an `el_pfunc_t *`
>   then a `wchar_t *`. Calls `prompt_get(el, p, c, op)`, which stores
>   the function through `p` and the ignore/escape character through `c`
>   (each skipped if that pointer is NULL, though a NULL `p` returns -1
>   before anything is stored), returning 0.
>   IMPORTANT: `prompt_get` selects the left-hand prompt only when `op`
>   is exactly `EL_PROMPT`; every other op, `EL_PROMPT_ESC` included,
>   selects `el_rprompt`. So `el_get(el, EL_PROMPT_ESC, &f, &c)` reports
>   the *right* prompt's function and escape character, not the left
>   one's — asymmetric with `prompt_set`, which correctly treats
>   `EL_PROMPT_ESC` as the left prompt. This is a bug, but it is
>   observable across the C ABI and therefore frozen: the port
>   reproduces it.
> - `EL_TERMINAL` (1) — one `const char **`. `terminal_get` (which
>   returns void) stores `el_terminal.t_name`, the terminal type name
>   currently loaded. `rv = 0` unconditionally. The pointer is into
>   libedit's storage and is invalidated by the next `terminal_set`; the
>   caller must not free it.
> - `EL_EDITOR` (2) — one `const wchar_t **`. `map_get_editor` returns
>   -1 if the pointer is NULL, otherwise stores the static literal
>   `L"emacs"` or `L"vi"` according to `el_map.type` and returns 0 (-1
>   if the type is neither, which cannot happen through the public API).
>   The string is static and must not be freed.
> - `EL_SIGNAL` (3) — one `int *`, set to `el->el_flags & HANDLE_SIGNALS`.
>   NOT normalised: the stored value is 0 or 0x001 (i.e. 1 here, but by
>   coincidence of the flag's value, not by design). `rv = 0`.
> - `EL_EDITMODE` (11) — one `int *`, set to `!(el_flags & EDIT_DISABLED)`,
>   so genuinely 0 or 1, and inverted relative to the flag to match the
>   setter's polarity. `rv = 0`.
> - `EL_SAFEREAD` (25) — one `int *`, set to `el_flags & FIXIO`. NOT
>   normalised: the stored value is 0 or 0x100 (256), so a caller
>   comparing it against 1 gets the wrong answer. `rv = 0`. Frozen
>   behaviour.
> - `EL_GETTC` (17) — a `char *` capability name (narrow, even in the
>   wide API) then a `void *` out-pointer. Builds a local
>   `char *argv[3]` with `argv[0]` pointing at a function-local
>   `static char name[] = "gettc"` — mutable, shared across calls and
>   across threads, though nothing writes through it — `argv[1]` the
>   name and `argv[2]` the out-pointer, then calls
>   `terminal_gettc(el, 3, argv)`. That returns -1 if either the name or
>   the out-pointer is NULL, or if the name matches no capability;
>   otherwise it stores through the out-pointer and returns 0. What it
>   stores depends on which table the name is found in:
>   a string capability stores a `char *` (so the out-pointer must be a
>   `char **`); the boolean-ish capabilities (the C's termcap codes
>   `pt`, `km`, `am`, `xn`) store a pointer to a static `"yes"` or
>   `"no"` string, also through a `char **`; any other numeric
>   capability stores an `int` (so the out-pointer must be an `int *`).
>   Passing the wrong pointer type for the capability is undefined
>   behaviour, and there is no way to ask which kind a name is. Exactly
>   two varargs are read: despite the header's `..., NULL` annotation,
>   no sentinel is consumed. `rv` is `terminal_gettc`'s value.
>   Under the terminfo decision this op's capability names are the one
>   place the termcap-to-terminfo translation is user-visible.
> - `EL_GETCFN` (13) — one `el_rfunc_t *`, set to
>   `el_read_getfn(el->el_read)`, which yields `EL_BUILTIN_GETCFN` (NULL)
>   when the built-in `read_char` is installed and the installed
>   function otherwise. So a round trip through `el_wset`/`el_wget`
>   normalises "the built-in" to NULL rather than reporting the internal
>   function's address. `rv = 0`.
> - `EL_CLIENTDATA` (14) — one `void **`, set to `el_data`. `rv = 0`.
> - `EL_UNBUFFERED` (15) — one `int *`, set to
>   `(el_flags & UNBUFFERED) != 0`, i.e. normalised to 0 or 1 — unlike
>   `EL_SIGNAL` and `EL_SAFEREAD`. `rv = 0`.
> - `EL_GETFP` (18) — an `int what` then a `FILE **`. `what` 0 stores
>   `el_infile`, 1 stores `el_outfile`, 2 stores `el_errfile`, any other
>   value sets `rv = -1` and leaves the caller's `FILE *` untouched;
>   otherwise `rv = 0`. There is no way to read back the file
>   descriptors, only the streams.
> - `EL_WORDCHARS` (26) — one `const wchar_t **`. `map_get_wordchars`
>   returns -1 if the pointer is NULL, otherwise stores
>   `el_map.wordchars` and returns 0. The stored value is NULL when the
>   application never set word characters (or when the `wcsdup` in
>   `map_set_wordchars` failed), which the caller must be prepared for;
>   NULL means "the built-in defaults are in use", not "empty".
> - `EL_GETENV` (27) — one `func_t *`, set to `el_getenv`. `rv = 0`.
> - Anything else: `rv = -1`, no varargs read. That covers every
>   set-only op — `EL_BIND` (4), `EL_TELLTC` (5), `EL_SETTC` (6),
>   `EL_ECHOTC` (7), `EL_SETTY` (8), `EL_ADDFN` (9), `EL_HIST` (10),
>   `EL_PREP_TERM` (16), `EL_SETFP` (19), `EL_REFRESH` (20),
>   `EL_RESIZE` (23), `EL_ALIAS_TEXT` (24) — so the `EL_*` space is not
>   symmetric and a caller cannot read back a resize or alias callback,
>   a history hook, or an added function.

> [spec:libedit:def:el.el-wline-fn]
> const LineInfoW * el_wline(EditLine *el)

> [spec:libedit:sem:el.el-wline-fn]
> Hands the caller a read-only view of the line currently being edited.
> The whole body is one cast:
> `return (const LineInfoW *)(void *)&el->el_line;`
>
> No work is done and nothing is copied. `el_line_t` starts with
> `wchar_t *buffer`, `wchar_t *cursor`, `wchar_t *lastchar` and then has
> a fourth member, `const wchar_t *limit`; `LineInfoW` is exactly the
> first three as `const wchar_t *`. The cast therefore re-labels the
> live editing state as a `LineInfoW`, exposing the buffer start, the
> cursor position and the one-past-the-end position, and hiding `limit`
> (the growth watermark) simply by being a shorter struct. The
> intermediate cast through `void *` exists to silence the compiler; the
> access is still type punning between two unrelated struct types, which
> is undefined behaviour under C's aliasing rules and works only because
> the two layouts agree on the first three pointer-sized members. A port
> must express this as a genuine borrowed view, not a transmute.
>
> Properties the caller depends on:
> - It is a live alias, not a snapshot. The three pointers change as the
>   user edits; a caller that stashes the `LineInfoW *` sees the updates
>   through it.
> - `lastchar` is one past the last character, and the buffer is NOT
>   NUL-terminated at that point. The length is `lastchar - buffer`.
> - The pointers themselves are invalidated when the line buffer is
>   reallocated to grow (`ch_enlargebufs` moves `buffer`, `cursor` and
>   `lastchar` to the new allocation), and the `LineInfoW *` itself is
>   invalidated by `el_end`. The caller must not free it.
> - The intended use is from inside a user-defined editor function
>   installed with `EL_ADDFN`, which needs to see and reason about the
>   line while it is being edited. It is callable at any time.
>
> There is no NULL check on `el`: `el_wline(NULL)` computes the offset
> of `el_line` from a null pointer, which is undefined behaviour rather
> than a fault, and typically yields a small non-null garbage pointer
> that the caller will then dereference.

> [spec:libedit:def:el.el-wset-fn]
> int el_wset(EditLine *el, int op, ...)

> [spec:libedit:sem:el.el-wset-fn]
> The wide-character parameter setter: a varargs dispatcher over the
> `EL_*` operation codes. This and `el_wget` are the library's whole
> configuration surface.
>
> Frame: if `el` is NULL, return -1 immediately without starting the
> varargs. Otherwise set `rv = 0`, `va_start`, run the dispatch below,
> `va_end`, return `rv`. An unrecognised `op` falls to the default case,
> which sets `rv = -1` and consumes no varargs. Every recognised op has
> a fixed argument shape; supplying different types is undefined
> behaviour, as is supplying fewer arguments than listed.
>
> Ops, by code:
>
> - `EL_PROMPT` (0) and `EL_RPROMPT` (12) — one `el_pfunc_t`
>   (`wchar_t *(*)(EditLine *)`). Calls `prompt_set(el, p, 0, op, 1)`:
>   selects `el_prompt` for `EL_PROMPT`, `el_rprompt` otherwise;
>   installs `p`, or the built-in `prompt_default` / `prompt_default_r`
>   if `p` is NULL; sets the prompt's ignore/escape character to
>   `L'\0'`; zeroes the recorded prompt position; sets `p_wide = 1`
>   (marking the callback as returning wide characters). Returns 0. The
>   function pointer is stored, not copied or wrapped.
> - `EL_PROMPT_ESC` (21) and `EL_RPROMPT_ESC` (22) — an `el_pfunc_t`
>   then an `int`. Same as above except `prompt_set(el, p, (wchar_t)c,
>   op, 1)`, so the ignore/escape character is set to `c`. The character
>   travels through varargs as an `int` (default promotion) and is
>   truncated to `wchar_t`; a value outside the `wchar_t` range is
>   implementation-defined. Setting it to `L'\0'` disables the literal
>   bracketing. Returns 0.
> - `EL_TERMINAL` (1) — one `char *` (narrow, even in the wide API,
>   because terminal type names are ASCII). Calls `terminal_set(el,
>   name)`, which reloads the whole capability set for that terminal
>   type; a NULL name means "re-read `TERM` through the `el_getenv`
>   hook", an empty or unknown name falls back to `dumb`, and the name
>   `emacs` additionally raises `EDIT_DISABLED`. Returns `terminal_set`'s
>   value: -1 on allocation failure, and -1 *also* whenever the capability
>   lookup failed, even though the dumb-terminal fallback was installed
>   successfully; 0 only when the lookup succeeded. See
>   `[spec:libedit:sem:terminal.terminal-set-fn]` step 14.
> - `EL_EDITOR` (2) — one `wchar_t *`. Calls `map_set_editor`: `L"emacs"`
>   installs the emacs key map and returns 0, `L"vi"` installs the vi key
>   map and returns 0, anything else returns -1 leaving the map
>   untouched. A NULL argument reaches `wcscmp` — undefined behaviour.
> - `EL_SIGNAL` (3) — one `int`. Non-zero sets `HANDLE_SIGNALS` (0x001)
>   in `el_flags`, zero clears it. `rv` is not assigned, so this always
>   returns 0. No handler is installed or removed here; the flag is
>   acted on later, by `read_prepare` (`sig_set`) and `read_finish`
>   (`sig_clr`).
> - `EL_BIND` (4), `EL_TELLTC` (5), `EL_SETTC` (6), `EL_ECHOTC` (7),
>   `EL_SETTY` (8) — a NULL-terminated list of `wchar_t *`. All five
>   share one collection loop into a local `const wchar_t *argv[20]`:
>   for `i` from 1 while `i < 20`, read the next vararg into `argv[i]`
>   and stop at the first NULL. On exit `i` is the argument count to
>   pass on. Then `argv[0]` is set to the command word and the callee is
>   invoked with `(el, i, argv)`:
>   `EL_BIND` → `L"bind"`, `map_bind`;
>   `EL_TELLTC` → `L"telltc"`, `terminal_telltc`;
>   `EL_SETTC` → `L"settc"`, `terminal_settc`;
>   `EL_ECHOTC` → `L"echotc"`, `terminal_echotc`;
>   `EL_SETTY` → `L"setty"`, `tty_stty`.
>   `rv` is the callee's return value. These are exactly the `.editrc`
>   builtins of the same names, reached programmatically instead of
>   through a file.
>   Bounds: at most 19 caller arguments fit. If all 19 slots are filled
>   the loop exits with `i == 20` and no NULL sentinel is stored, so a
>   20th and further varargs are neither read nor reported — they are
>   silently dropped, not diagnosed. If the caller omits the NULL
>   sentinel with fewer than 19 arguments, the loop reads past the end
>   of the argument list: undefined behaviour. If the very first vararg
>   is NULL, `i` is 1 and the callee sees only the command word.
>   The inner `switch`'s `default:` arm sets `rv = -1` and then invokes
>   `EL_ABORT`, which under `DEBUG` prints `"<file>, <line>: Bad op
>   %d\n"` to `el_errfile` and calls `abort()`, and otherwise calls
>   `abort()` bare. It is unreachable — the outer `switch` has already
>   restricted `op` to these five — and the port should express it as an
>   unreachable branch rather than a runtime abort.
> - `EL_ADDFN` (9) — `wchar_t *name`, `wchar_t *help`, `el_func_t func`
>   (`el_action_t (*)(EditLine *, wint_t)`). Calls `map_addfunc`, which
>   returns -1 if any of the three is NULL or if growing the function
>   and help tables fails, and otherwise appends: the function pointer
>   at index `nfunc`, a `wcsdup` copy of `name` and of `help` in the
>   help table (libedit owns those copies and frees them in `map_end`),
>   the help entry's `func` field set to the new index, `nfunc`
>   incremented, returning 0. The new function becomes bindable by name
>   through `EL_BIND` and through `.editrc`.
> - `EL_HIST` (10) — `hist_fun_t func`
>   (`int (*)(void *, HistEventW *, int, ...)`) and `void *ptr`. Calls
>   `hist_set`, which stores them in `el_history.fun` and
>   `el_history.ref` and returns 0 — so `rv` is 0. Then:
>   `if (MB_CUR_MAX == 1) el->el_flags &= ~NARROW_HISTORY;`.
>   `MB_CUR_MAX` is the current `LC_CTYPE` locale's maximum multibyte
>   character length, read at call time, so this decision depends on
>   whatever locale is installed at the moment of the call.
>   `NARROW_HISTORY` (0x040) means "the installed history callback
>   speaks narrow `HistEvent`, so decode its result before use"; it is
>   raised by the narrow `el_set(EL_HIST)` in eln.c, which does not
>   route through this function.
>   IMPORTANT: the guard is the wrong way round for safety. Installing a
>   *wide* history callback through `el_wset` should always clear
>   `NARROW_HISTORY`, but it only does so in a single-byte locale. In a
>   multibyte locale, an application that called the narrow
>   `el_set(EL_HIST)` and later the wide `el_wset(EL_HIST)` leaves the
>   flag set, and every subsequent history access then passes the wide
>   callback's `wchar_t *` result to `hist_convert`, which reinterprets
>   it as a `char *` and decodes it — type confusion producing garbage
>   or a fault. The behaviour crosses the ABI, so the port is expected
>   to reproduce the flag manipulation as written, but should carry the
>   hazard in its own notes.
> - `EL_EDITMODE` (11) — one `int`. Non-zero *clears* `EDIT_DISABLED`
>   (0x004), zero *sets* it; the polarity is inverted relative to the
>   flag. `rv` is explicitly set to 0. Disabling editing makes libedit
>   read lines without putting the terminal in raw mode.
> - `EL_GETCFN` (13) — one `el_rfunc_t` (`int (*)(EditLine *, wchar_t *)`).
>   Calls `el_read_setfn(el->el_read, rc)`, which installs the built-in
>   `read_char` when `rc` is `EL_BUILTIN_GETCFN` (i.e. NULL) and `rc`
>   otherwise, returning 0. Dereferences `el->el_read`, which is only
>   ever NULL on the unreachable `read_init`-failed path.
> - `EL_CLIENTDATA` (14) — one `void *`, stored verbatim in `el_data`.
>   `rv` is not assigned, so this returns 0. libedit never dereferences
>   or frees it; it exists so a callback can find its way back to the
>   application's own state.
> - `EL_UNBUFFERED` (15) — one `int`, read into `rv`. Edge-triggered:
>   if it is non-zero and `UNBUFFERED` (0x008) is currently clear, set
>   the flag and then call `read_prepare(el)`; if it is zero and
>   `UNBUFFERED` is currently set, clear the flag and then call
>   `read_finish(el)`; in the two matching cases do nothing at all.
>   The flag is updated *before* the call, deliberately, because both
>   callees test it: `read_prepare` sees `UNBUFFERED` set and so ends by
>   flushing the terminal, and `read_finish` sees it clear and so calls
>   `tty_cookedmode`. `rv` is then reset to 0, so this always returns 0
>   regardless of what happened.
> - `EL_PREP_TERM` (16) — one `int`, read into `rv`. Non-zero calls
>   `tty_rawmode(el)`, zero calls `tty_cookedmode(el)`; both results are
>   discarded. `rv` is then reset to 0, so this always returns 0 even
>   when the terminal mode change failed.
> - `EL_SETFP` (19) — an `int what` then a `FILE *fp`. `what` 0 sets
>   `el_infile = fp` and `el_infd = fileno(fp)`; 1 sets `el_outfile` /
>   `el_outfd`; 2 sets `el_errfile` / `el_errfd`; any other value sets
>   `rv = -1` and changes nothing. Otherwise `rv` is 0. Both varargs are
>   consumed before `what` is validated. The previously installed stream
>   is neither flushed nor closed — the caller owns both the old and the
>   new. A NULL `fp` is not rejected and reaches `fileno`: undefined
>   behaviour.
> - `EL_REFRESH` (20) — no arguments. Calls `re_clear_display(el)`
>   (forget what libedit believes is on the screen, so the next redraw
>   is unconditional), then `re_refresh(el)` (redraw prompt and current
>   line), then `terminal__flush(el)`. `rv` is not assigned, so this
>   returns 0.
> - `EL_RESIZE` (23) — an `el_zfunc_t` (`void (*)(EditLine *, void *)`)
>   and a `void *`. `ch_resizefun` stores them in
>   `el_chared.c_resizefun` and `c_resizearg` and returns 0. The
>   callback is invoked when the terminal size changes while a line is
>   being edited; the `void *` is opaque and not copied.
> - `EL_ALIAS_TEXT` (24) — an `el_afunc_t`
>   (`const char *(*)(void *, const char *)`) and a `void *`.
>   `ch_aliasfun` stores them in `el_chared.c_aliasfun` and
>   `c_aliasarg` and returns 0.
> - `EL_SAFEREAD` (25) — one `int`. Non-zero sets `FIXIO` (0x100), zero
>   clears it. `rv` is explicitly set to 0. The flag controls whether
>   the read path retries after recoverable I/O errors.
> - `EL_WORDCHARS` (26) — one `wchar_t *`. `map_set_wordchars` frees the
>   previous `el_map.wordchars` and stores `wcsdup` of the argument.
>   Always returns 0 — including when `wcsdup` fails, in which case
>   `wordchars` is left NULL, which means "use the built-in default word
>   characters", so an allocation failure silently reverts the setting.
>   A NULL argument reaches `wcsdup`: undefined behaviour.
> - `EL_GETENV` (27) — one `func_t` (`char *(*)(const char *)`), stored
>   directly into `el_getenv`. `rv` is not assigned, so this returns 0.
>   Not NULL-checked; see
>   `[spec:libedit:sem:el.editline.el-getenv-fn]`.
> - Anything else, including the get-only codes `EL_GETTC` (17) and
>   `EL_GETFP` (18): `rv = -1`, no varargs read.
>
> Note that a NULL `el` and an unknown `op` are the only two errors this
> function detects itself; every other -1 comes from a callee.

> [spec:libedit:def:el.func-t-const-char]
> typedef char * (*func_t)(const char *)

> [spec:libedit:def:el.secure-getenv-fn]
> char *secure_getenv(char const *name)

> [spec:libedit:sem:el.secure-getenv-fn]
> A privilege-aware `getenv`: returns the variable's value only when the
> process is not running with elevated privilege obtained from its
> executable.
>
> Step 1: if `issetugid()` is non-zero, return NULL (written `return 0`
> in the C, which is a null pointer, not an empty string).
> Step 2: otherwise return `getenv(name)` — a borrowed pointer into the
> process environment, not a copy.
>
> This definition is compiled only when the platform supplies neither
> `secure_getenv` nor `__secure_getenv`. The selection cascade, in
> order:
> - `HAVE_SECURE_GETENV` — the libc function is used and this body is
>   not compiled.
> - otherwise `HAVE___SECURE_GETENV` — `secure_getenv` is a macro for
>   `__secure_getenv`, `HAVE_SECURE_GETENV` is then treated as set, and
>   this body is still not compiled.
> - otherwise this body is compiled, with `issetugid()` coming from
>   `<unistd.h>` if `HAVE_ISSETUGID` is set.
> - otherwise `issetugid()` is `#define`d to the constant 1, so the
>   function unconditionally returns NULL and libedit reads *no*
>   environment variables at all on that platform. That is a real,
>   shipped configuration, not a theoretical one: on a host without
>   `secure_getenv`, `__secure_getenv` and `issetugid`, `TERM`, `HOME`,
>   `EDITRC` and `EDITOR` are all invisible to libedit.
>
> The whole construct exists so that `el_getenv`'s default cannot be
> used to feed a set-uid program a terminal description or an `.editrc`
> of the attacker's choosing.
>
> For the port (no C FFI, POSIX-only), this becomes a single Rust
> function with the step 1/step 2 semantics: return `None` when the
> process is privileged — the real uid differs from the effective uid,
> or the real gid differs from the effective gid, or the loader marked
> the process secure (`AT_SECURE`) — and otherwise return the
> environment value. The "always deny" degenerate branch above must not
> be reproduced; it is a portability artefact, not intended behaviour.

