# src/sig.c, src/sig.h

> [spec:libedit:def:sig.el-signal-t]
> typedef struct

> [spec:libedit:def:sig.sig-clr-fn]
> libedit_private void sig_clr(EditLine *el)

> [spec:libedit:sem:sig.sig-clr-fn]
> Un-installs libedit's signal handler by putting back the dispositions that
> `sig_set()` saved. Returns nothing and reports nothing; every system
> call's result is discarded.
>
> Steps:
>   1. `sigprocmask(SIG_BLOCK, &el->el_signal->sig_set, &oset)`. That field is
>      the cached mask built by `sig_init()` and holds exactly the seven
>      trapped signals: `SIGINT`, `SIGTSTP`, `SIGQUIT`, `SIGHUP`, `SIGTERM`,
>      `SIGCONT`, `SIGWINCH`. Blocking them for the duration of the loop is
>      what stops `sig_handler()` from running between the `sigaction`
>      calls and rewriting a `sig_action[]` slot we are in the middle of using.
>   2. For `i` in `0..6`, walking the fixed table order
>      (`SIGINT`, `SIGTSTP`, `SIGQUIT`, `SIGHUP`, `SIGTERM`, `SIGCONT`,
>      `SIGWINCH`): if `el->el_signal->sig_action[i].sa_handler != SIG_ERR`,
>      call `sigaction(sig, &el->el_signal->sig_action[i], NULL)`.
>      `SIG_ERR` is the "nothing saved here" sentinel. A slot is `SIG_ERR`
>      either because `sig_init` initialised it that way and `sig_set`'s
>      `sigaction` never succeeded for that signal, or because the signal
>      actually fired and `sig_handler` already restored the old disposition
>      and re-blanked the slot itself. Either way the correct action is to
>      leave the current disposition alone, which is what the test achieves.
>   3. `sigprocmask(SIG_SETMASK, &oset, NULL)` to restore the caller's mask.
>      Any of the seven that arrived during the loop is delivered here, now
>      going to the application's own handler rather than to libedit's.
>
> Called only from `read_finish`, and only when `el->el_flags & HANDLE_SIGNALS`
> (`EL_SIGNAL`) is set.
>
> Quirks and defects to record rather than reproduce:
>   - Restored slots are **not** reset to `SIG_ERR`. The saved dispositions
>     survive into the next round. That is observable: `read_prepare` tests
>     `HANDLE_SIGNALS` to decide whether to call `sig_set`, and `read_finish`
>     tests it independently to decide whether to call `sig_clr`, so an
>     application that turns `EL_SIGNAL` on part-way through a read reaches
>     `sig_clr` with no matching `sig_set`, and libedit then re-installs
>     dispositions captured during an *earlier* `el_wgets`, silently clobbering
>     any handler the application installed in the meantime. A port should
>     model each slot as an option and consume it on restore.
>   - No `NULL` check on `el->el_signal`. Calling this after
>     `sig_end()`, or on an `EditLine` whose `sig_init` allocation
>     failed (`el_init_internal` discards `sig_init`'s return value), is an
>     immediate `NULL` dereference.
>   - `sigprocmask` is used, not `pthread_sigmask`. POSIX leaves `sigprocmask`
>     unspecified in a multi-threaded process, and the mask is per-thread while
>     dispositions are per-process, so blocking here does not stop the signal
>     being delivered to some other thread mid-loop.

> [spec:libedit:def:sig.sig-end-fn]
> libedit_private void sig_end(EditLine *el)

> [spec:libedit:sem:sig.sig-end-fn]
> Tears down the per-`EditLine` signal state. Two statements, in order:
>   1. `el_free(el->el_signal)` — `free()` the block allocated by
>      `sig_init()`. `free(NULL)` is well defined, so this is safe when
>      the allocation failed.
>   2. `el->el_signal = NULL`.
>
> Returns nothing; cannot fail. Called from `el_end`, after `el_reset` and
> after `tty_end`.
>
> What it deliberately does *not* do, and why the port must differ:
>   - It never calls `sig_clr()`. Nothing in `el_end`'s path does
>     (`el_reset` only does `tty_cookedmode` + `ch_reset`). In the normal flow
>     `read_finish` has already cleared the handlers, but if the application
>     destroys the `EditLine` without a matching `read_finish` — a `longjmp`
>     out of a read, an `el_end` from an application signal handler, or
>     `EL_SIGNAL` toggled off between `read_prepare` and `read_finish` —
>     libedit's `sig_handler` is still the installed disposition for up to
>     seven signals when this frees the struct it dereferences. The next
>     delivery is then a use-after-free of `el->el_signal` (and, via `sel`, of
>     the `EditLine` itself once the caller frees that too).
>   - It never clears the file-static `sel` pointer that `sig_set` set (see
>     `sig_handler()`). After `el_end` that global dangles at freed
>     memory for the rest of the process lifetime.
>
> A Rust port must invert this: restore dispositions first, drop/clear the
> global registration second, free last, and make that ordering a property of
> the destructor rather than of caller discipline.

> [spec:libedit:def:sig.sig-handler-fn]
> static void sig_handler(int signo)

> [spec:libedit:sem:sig.sig-handler-fn]
> The single handler installed by `sig_set()` for all seven trapped
> signals. Policy, in libedit's own words: trap everything, put the terminal
> into a sane state, then pass the ball back to whoever had the signal before
> us. Returns nothing.
>
> Entry conditions established by `sig_set`: `sa_flags == SA_ONSTACK` (so no
> `SA_RESTART` — blocking reads must fail with `EINTR`; and no `SA_NODEFER`,
> so the kernel has `signo` blocked for the duration) and an **empty**
> `sa_mask`, so any of the other six trapped signals can nest inside this
> handler and re-enter everything below.
>
> Steps:
>   1. `save_errno = errno`.
>   2. `sigemptyset(&nset)`; `sigaddset(&nset, signo)`;
>      `sigprocmask(SIG_BLOCK, &nset, &oset)`. Blocking `signo` is redundant —
>      the kernel already did it — but capturing `oset` is not: `oset` is the
>      mask as of handler entry and therefore **still contains `signo`**, which
>      is what makes step 8 behave the way it does.
>   3. `sel->el_signal->sig_no = signo`. `sel` is a file-static
>      `EditLine *` (see the global-instance note below); `sig_no` is
>      `volatile sig_atomic_t` and is the handler's only channel to the read
>      loop, which polls it in `read_char` after an interrupted `read()`.
>   4. Dispatch on `signo`:
>      - `SIGCONT` (we have just been resumed after a stop): `tty_rawmode(sel)`
>        to put the terminal back into libedit's edit-mode `termios`, which the
>        job-control stop left in the application's cooked settings; then
>        `if (ed_redisplay(sel, 0) == CC_REFRESH) re_refresh(sel);`; then
>        `terminal__flush(sel)`.
>        **The `re_refresh` call is unreachable.** `ed_redisplay` unconditionally
>        returns `CC_REDISPLAY` (8) and `CC_REFRESH` is 4, so the test is always
>        false. Port the `tty_rawmode` and the flush; do not port the redisplay
>        branch as live behaviour. The real redraw after a resume happens in the
>        read loop, which sees `sig_no == SIGCONT` and issues
>        `el_wset(el, EL_REFRESH)` (`re_clear_display` + `re_refresh` +
>        `terminal__flush`) from normal context.
>      - `SIGWINCH`: `el_resize(sel)` — re-read the window size and, if it
>        changed, resize and clear the display buffers.
>      - everything else (`SIGINT`, `SIGTSTP`, `SIGQUIT`, `SIGHUP`, `SIGTERM`):
>        `tty_cookedmode(sel)` — restore the application's `termios` *before*
>        the signal is allowed to take its real effect, so a process that is
>        about to die or stop does not leave the tty in raw mode. This is the
>        entire reason the handler exists.
>   5. Linear-scan the static table `{SIGINT, SIGTSTP, SIGQUIT, SIGHUP,
>      SIGTERM, SIGCONT, SIGWINCH, -1}` for `signo`, yielding index `i`.
>      If `signo` is not in the table the loop stops on the `-1` terminator
>      with `i == 7`, one past the end of `sig_action[7]`, and step 6 reads and
>      writes out of bounds. Unreachable in practice (the handler is `static`
>      and only ever installed for those seven), but a port should make the
>      lookup total rather than carry the hazard.
>   6. `sigaction(signo, &sel->el_signal->sig_action[i], NULL)` — reinstall the
>      disposition that was in place before libedit took over, so this handler
>      de-installs itself for this signal. Then blank the slot:
>      `sa_handler = SIG_ERR`, `sa_flags = 0`, `sigemptyset(&sa_mask)`.
>      Note this restore is unconditional: if the slot still holds the
>      `SIG_ERR` sentinel (nothing was ever saved because `sig_set`'s
>      `sigaction` failed for this signal) libedit hands `SIG_ERR` to
>      `sigaction` as a handler. Whether that fails with `EINVAL` — the result
>      is discarded — or is accepted and leaves a disposition that faults on
>      the next delivery is implementation-defined; the behaviour here is
>      undefined and should not be reproduced. A port must treat "nothing
>      saved" as "leave the disposition alone".
>   7. `sigprocmask(SIG_SETMASK, &oset, NULL)` — restore the entry mask, in
>      which `signo` is still blocked.
>   8. `raise(signo)`. Because `signo` is blocked this does **not** run the
>      just-restored handler inline; the signal goes pending and is delivered
>      when this handler returns and the kernel restores the pre-delivery mask.
>      That is the chaining mechanism: the previous handler, or the default
>      action, runs immediately after we return, on this same thread (POSIX
>      `raise` is `pthread_kill(pthread_self(), signo)`). Consequences per
>      signal, assuming the default disposition was in place: `SIGINT`,
>      `SIGQUIT`, `SIGHUP`, `SIGTERM` terminate the process with the tty already
>      back in cooked mode; `SIGTSTP` stops it; `SIGCONT` is a no-op; `SIGWINCH`
>      is ignored. If the application had its own handler, that handler runs
>      instead, which is the "pass the ball to our caller" contract.
>   9. `errno = save_errno`, so the interrupted code sees the errno it had.
>
> Async-signal-safety — the central problem with this function. Safe (on
> POSIX's async-signal-safe list): `sigemptyset`, `sigaddset`, `sigprocmask`,
> `sigaction`, `raise`, and the `errno` save/restore. Everything else is a
> violation:
>   - `el_resize` reaches `terminal_change_size` -> `terminal_rebuffer_display`
>     -> `free()` and `calloc()`. Calling the allocator from a handler that may
>     have interrupted the allocator is undefined and can deadlock on the
>     malloc lock or corrupt the heap. This is the most severe one.
>   - `el_resize` also issues `ioctl(TIOCGWINSZ)`, which is not on the
>     async-signal-safe list (and `TIOCGWINSZ` itself is not POSIX).
>   - `terminal__flush` is `fflush()` on a `FILE *`. stdio is not
>     async-signal-safe: interrupting a stdio call and re-entering it can
>     deadlock on the stream lock or emit interleaved/garbled output.
>   - `tty_rawmode` / `tty_cookedmode`: the `tcgetattr`/`tcsetattr`/`cfset*speed`
>     calls are themselves async-signal-safe, but the functions mutate shared
>     `EditLine` state (`el_tty.t_mode`, `t_c[]`, `t_ed`/`t_ex`) non-atomically
>     with the code they interrupted, and `tty_rawmode` can reach
>     `tty_bind_char` -> `keymacro_clear` -> `keymacro_delete` -> `free()`.
>   - The empty `sa_mask` means a second trapped signal can nest and re-enter
>     all of the above, including the allocator paths.
> None of this is concurrency in the thread sense — the handler runs on the
> interrupted thread — it is re-entrancy, and the interrupted state may be a
> half-updated `EditLine`, a half-updated `FILE`, or a half-updated heap.
>
> Global-instance mechanism, and why the port owes it nothing: a C signal
> handler receives only an `int`, so libedit cannot be handed the `EditLine`
> it is supposed to fix up. It keeps a file-static `static EditLine *sel`,
> assigned unconditionally by every `sig_set` call and never cleared — not by
> `sig_clr`, not by `sig_end`. Consequences: with more than one `EditLine`,
> whichever instance armed handlers last owns every signal, so a signal that
> arrives while a *different* instance is reading restores the wrong terminal
> state and stores `sig_no` where nobody will read it; and after `el_end` the
> pointer dangles. This is an artifact of the C signal API, not a requirement
> on the port. The only contract a port must preserve is observable: the tty
> is restored before the previous disposition takes effect, the signal number
> reaches the read loop, and the previously installed handler still runs. A
> Rust port should record the signal number in an `AtomicI32`/`AtomicBool` and
> at most `write()` a byte to a self-pipe from the handler, and do the
> `tty_cookedmode` / resize / redisplay work in the read loop where `read_char`
> already handles it; or take the signals synchronously — POSIX `sigwait`/
> `sigwaitinfo` on a dedicated thread, or a platform facility such as
> `signalfd`/`kqueue` — and have no handler at all. Under no circumstances may
> the handler allocate, take a lock, or touch a buffered writer.
>
> `EINTR` interaction: because `sig_set` does not use `SA_RESTART`, a blocking
> `read()` on `el_infd` fails with `EINTR` when any of the seven fires. That is
> the designed poll point. `read_char` zeroes `sig_no` before each `read()` and,
> on `-1`, inspects it: `SIGCONT` -> `el_wset(el, EL_REFRESH)` and fall through;
> `SIGWINCH` -> call `sig_set` again (the handler de-installed itself at step 6,
> so it must be re-armed) and restart the read; anything else -> fall through to
> `read__fixio`, which does not recognise `EINTR`, so the read fails and the
> error propagates with `errno == EINTR`. A port that sets `SA_RESTART`, or
> that transparently retries `EINTR` in its read wrapper, breaks the resume and
> resize redraw paths.

> [spec:libedit:def:sig.sig-init-fn]
> libedit_private int sig_init(EditLine *el)

> [spec:libedit:sem:sig.sig-init-fn]
> Allocates and primes the per-`EditLine` signal state. It installs **no**
> handlers and changes no disposition; arming is `sig_set()`'s job.
> Returns 0 on success, -1 on allocation failure — the only failure mode, since
> every `sigemptyset`/`sigaddset`/`sigprocmask` result here is discarded.
>
> Steps:
>   1. `el->el_signal = el_malloc(sizeof *el->el_signal)`, i.e. `malloc` of the
>      `el_signal_t` block (`struct sigaction sig_action[7]`, `sigset_t
>      sig_set`, `volatile sig_atomic_t sig_no`). If it returns `NULL`, return
>      -1 immediately, leaving `el->el_signal == NULL`. The block is
>      uninitialised at this point.
>   2. Build the cached trapped-signal mask in `el->el_signal->sig_set`:
>      `sigemptyset`, then `sigaddset` for `SIGINT`, `SIGTSTP`, `SIGQUIT`,
>      `SIGHUP`, `SIGTERM`, `SIGCONT`, `SIGWINCH` — in that order, which is
>      also the index order of `sig_action[]` everywhere else in this module.
>      That is the complete list; there are exactly seven. (The `editline.3`
>      manual page additionally lists `SIGSTOP`, which is wrong: `SIGSTOP`
>      cannot be caught or blocked and is not in the set. Document the code,
>      not the man page.) The field exists purely so `sig_set()`/`sig_clr()`
>      can block the whole group cheaply.
>   3. `sigprocmask(SIG_BLOCK, nset, &oset)` around the initialisation loop,
>      then `sigprocmask(SIG_SETMASK, &oset, NULL)` after it. This is vestigial
>      — none of libedit's handlers is installed yet, so nothing can observe
>      the half-initialised array — but it is faintly observable in that the
>      seven signals are briefly blocked on the calling thread, delaying any
>      delivery that lands in the window. A port may drop it.
>   4. The loop itself, for `i` in `0..6`:
>      `sig_action[i].sa_handler = SIG_ERR`, `sa_flags = 0`,
>      `sigemptyset(&sig_action[i].sa_mask)`. `SIG_ERR` is the module's
>      "nothing saved in this slot" sentinel, tested by `sig_clr()` and
>      re-written by `sig_handler()`. It is a strange sentinel — `SIG_ERR`
>      is `sigaction`/`signal`'s *error* return, not a disposition — and a port
>      should use an explicit optional instead.
>   5. `return 0`.
>
> Note `sig_no` is **not** initialised: it holds whatever `malloc` returned.
> Nothing reads it before `read_char` stores 0 into it ahead of every `read()`,
> so the garbage is not observable in the shipped flow, but a port should zero
> it.
>
> Caller contract to record: `el_init_internal` invokes this as
> `(void) sig_init(el)` and ignores the -1. On allocation failure `el->el_signal`
> stays `NULL` and the first read dereferences it (`read_char` does
> `el->el_signal->sig_no = 0` unconditionally, with no `HANDLE_SIGNALS` guard),
> as do `sig_set` and `sig_clr`. A latent `NULL` dereference on OOM. In Rust the
> field is simply a value, and the failure mode disappears.

> [spec:libedit:def:sig.sig-set-fn]
> libedit_private void sig_set(EditLine *el)

> [spec:libedit:sem:sig.sig-set-fn]
> Arms `sig_handler()` for all seven trapped signals and records the
> dispositions it displaces, so `sig_clr()` can put them back. Returns
> nothing; every `sigaction`/`sigprocmask` failure is swallowed, and a signal
> whose `sigaction` fails is simply left untrapped with no report.
>
> Steps:
>   1. Build the new action `nsa` on the stack:
>      `nsa.sa_handler = sig_handler`, `nsa.sa_flags = SA_ONSTACK`,
>      `sigemptyset(&nsa.sa_mask)`.
>      - `SA_ONSTACK` only has an effect if the *application* installed an
>        alternate signal stack; libedit never calls `sigaltstack`.
>      - `SA_RESTART` is deliberately **not** set. Interrupted `read()`s must
>        fail with `EINTR`, which is how the read loop learns a signal arrived;
>        see `sig_handler()`.
>      - The empty `sa_mask` means only the delivered signal is blocked during
>        the handler, so the other six can nest inside it.
>      - Only three members are assigned. On implementations whose
>        `struct sigaction` has more (a separate `sa_sigaction`, `sa_restorer`,
>        …) the rest is passed to the kernel uninitialised. Harmless in
>        practice because `SA_SIGINFO` is not set, but it is a genuine
>        uninitialised read; a port must initialise the whole action.
>   2. `sel = el` — publish this `EditLine` in the module-static pointer the
>      handler will use. Unconditional and never cleared, so the most recent
>      caller owns every signal process-wide; see the global-instance
>      discussion in `sig_handler()`.
>   3. `sigprocmask(SIG_BLOCK, &el->el_signal->sig_set, &oset)` — block all
>      seven for the duration of the loop, so the handler cannot fire between
>      two `sigaction` calls and rewrite a `sig_action[]` slot underneath us.
>   4. For each `i` / signal in the fixed order `SIGINT`, `SIGTSTP`, `SIGQUIT`,
>      `SIGHUP`, `SIGTERM`, `SIGCONT`, `SIGWINCH`:
>      `if (sigaction(sig, &nsa, &osa) != -1 && osa.sa_handler != sig_handler)`
>      `el->el_signal->sig_action[i] = osa;`
>      — install ours unconditionally, but save the displaced action only when
>      the call succeeded *and* what we displaced was not already our own
>      handler.
>      - That second test is the idempotence guard and is load-bearing.
>        `sig_set` is legitimately called more than once without an intervening
>        `sig_clr`: `read_char` re-arms after a `SIGWINCH`- or
>        `SIGCONT`-interrupted read, because the handler de-installs itself.
>        Without the guard the second call would record `sig_handler` as "the
>        previous handler", and `sig_clr` would then install libedit's handler
>        permanently, pointing at a stale `sel`. Note the guard also means a
>        re-arm after the handler ran does *not* refresh the saved slot — but
>        it does not need to, because the handler already restored the original
>        and blanked the slot to `SIG_ERR`, and this call's `sigaction`
>        displaces that same original and saves it again.
>      - On `sigaction` failure the slot keeps its previous value (`SIG_ERR`
>        from `sig_init()`, or `SIG_ERR` written by the handler), so
>        `sig_clr` will skip it — the right outcome, since we failed to install
>        anything either.
>      - Comparing `osa.sa_handler` is only strictly meaningful when the
>        displaced action did not use `SA_SIGINFO`; on implementations where
>        `sa_handler` and `sa_sigaction` overlap in a union, reading
>        `sa_handler` from an `SA_SIGINFO` action is not portable. Saving and
>        later restoring the whole `struct sigaction` by value does work for
>        such handlers.
>   5. `sigprocmask(SIG_SETMASK, &oset, NULL)`. Anything that arrived during
>      the loop is delivered here, to the freshly installed handler.
>
> Signals arriving during setup:
>   - Before step 3, or for a signal whose `sigaction` has not run yet, delivery
>     goes to whatever disposition the application had — the correct outcome,
>     and the reason the window is kept as small as it is.
>   - Steps 1-2 are *not* inside the blocked window. If a handler armed by an
>     earlier `sig_set` (this `EditLine`'s or another's) fires between
>     `sel = el` and the `sigprocmask` at step 3, it runs against the freshly
>     published `sel` while that instance has not been set up for it — it will
>     store `sig_no` on, and restore the terminal of, the new instance. Narrow
>     but genuine; publishing the instance under the block would close it.
>   - Everything that arrives inside the blocked window is merely delayed to
>     step 5, and standard POSIX signal merging applies: multiple deliveries of
>     the same signal while blocked collapse into one, so a burst of `SIGWINCH`
>     during setup produces a single handler run and a single resize.
>
> Callers: `read_prepare` when `el->el_flags & HANDLE_SIGNALS` (`EL_SIGNAL`) is
> set, and `read_char` after a `SIGCONT`/`SIGWINCH` interruption. The seven
> signals are exactly what `EL_SIGNAL` promises to trap.
>
> As with `sig_clr()`, `sigprocmask` rather than `pthread_sigmask` is
> used — unspecified in a multi-threaded process — and dispositions are
> process-wide while this call is made from whichever thread happens to be
> editing, so in a threaded program libedit silently reassigns handlers other
> threads depend on. A port that keeps handler installation at all should scope
> and serialise it explicitly.

