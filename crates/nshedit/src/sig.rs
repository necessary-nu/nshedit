//! Ported from `src/sig.c`; rules live in `docs/spec/port/src/sig.md`.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::el::EditLine;
use crate::terminal::terminal__flush;
use crate::tty::{tty_cookedmode, tty_rawmode};

/// C: `#define ALLSIGSNO 7` — `SIGINT`, `SIGTSTP`, `SIGQUIT`, `SIGHUP`,
/// `SIGTERM`, `SIGCONT`, `SIGWINCH`, in that fixed table order.
pub const ALLSIGSNO: usize = 7;

/// Stand-in for the C's `struct sigaction`: one saved previous disposition.
///
/// `plan/decisions/no-c-ffi.md` bars linking libc, so there is no
/// `struct sigaction` to borrow and no `sigaction()` to call. libedit only
/// ever stores and restores these wholesale, so the concrete carrier — a
/// libc-free syscall wrapper, or a crate — is a decision for the `sig.c`
/// translation. This placeholder holds the field's place and its
/// cardinality.
pub struct SigAction {
    _placeholder: (),
}

/// Stand-in for the C's `sigset_t`: the cached mask of the seven trapped
/// signals. Same reasoning as [`SigAction`].
pub struct SigSet {
    _placeholder: (),
}

// [spec:libedit:def:sig.el-signal-t]
/// The per-`EditLine` signal state.
///
/// C: `typedef struct { ... } *el_signal_t;` — the typedef names the
/// *pointer*, and the struct itself is anonymous. Rust has neither anonymous
/// structs nor a way to name a pointer-to-anonymous, so it is split into the
/// struct and the owning-handle alias below; both belong to this one rule.
pub struct ElSignal {
    /// C: `struct sigaction sig_action[ALLSIGSNO]` — the dispositions
    /// `sig_set` displaced, indexed by the fixed table order.
    ///
    /// The C uses `SIG_ERR` as its "nothing saved here" sentinel and does
    /// *not* re-blank a slot after restoring it, which
    /// `sem:sig.sig-clr-fn` records as observable and asks
    /// the port to fix by modelling each slot as an option consumed on
    /// restore. That is what `Option` is doing here.
    pub sig_action: [Option<SigAction>; ALLSIGSNO],
    /// The cached mask holding exactly the seven trapped signals.
    pub sig_set: SigSet,
    /// C: `volatile sig_atomic_t sig_no` — the signal the handler saw,
    /// written from a signal handler and read by `read_char`.
    pub sig_no: AtomicI32,
}

/// C: `el_signal_t` — the owning handle stored in `EditLine::el_signal`.
/// `None` before `sig_init` succeeds and after `sig_end`; the C leaves a
/// dangling pointer there instead, which
/// `sem:sig.sig-end-fn` requires the port not to reproduce.
pub type ElSignalT = Option<Box<ElSignal>>;

/// The seven signal numbers.
///
/// POSIX names these signals; it does not fix their numbers, and
/// `plan/decisions/no-c-ffi.md` bars the libc that would define them, so the
/// port carries the platform ABI's numbering itself. This is the only part of
/// the module that is not portable POSIX. Five of the seven agree everywhere
/// in scope; only `SIGCONT` and `SIGTSTP` differ. Linux's alpha, mips, sparc
/// and parisc ports renumber more widely and are not covered — that belongs
/// with the syscall layer described on [`plat`] when it lands.
pub(crate) mod signo {
    pub(crate) const SIGHUP: i32 = 1;
    pub(crate) const SIGINT: i32 = 2;
    pub(crate) const SIGQUIT: i32 = 3;
    pub(crate) const SIGTERM: i32 = 15;
    pub(crate) const SIGWINCH: i32 = 28;

    /// Linux's generic ABI numbers `SIGCONT` 18 and `SIGTSTP` 20; the BSDs
    /// and Darwin swap them to 19 and 18.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub(crate) const SIGCONT: i32 = 18;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub(crate) const SIGTSTP: i32 = 20;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    pub(crate) const SIGCONT: i32 = 19;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    pub(crate) const SIGTSTP: i32 = 18;
}

/// C: `static const int sighdl[]` — the trapped signals in the fixed table
/// order that indexes `ElSignal::sig_action` throughout this module. The set
/// is exactly these seven; `editline.3`'s additional `SIGSTOP` is wrong and
/// cannot be caught or blocked (ERR-terminal-64).
///
/// The C's trailing `-1` terminator is not carried. It exists only for the
/// linear scans, and it is what lets the scan in `sig_handler` step 5 run one
/// past `sig_action[]` (ERR-terminal-12); [`sighdl_index`] is total instead.
const SIGHDL: [i32; ALLSIGSNO] = [
    signo::SIGINT,
    signo::SIGTSTP,
    signo::SIGQUIT,
    signo::SIGHUP,
    signo::SIGTERM,
    signo::SIGCONT,
    signo::SIGWINCH,
];

/// Total replacement for the C's `for (i = 0; sighdl[i] != -1; i++)` scan
/// (`sem:sig.sig-handler-fn` step 5).
///
/// The C leaves `i == ALLSIGSNO` when the signal is not in the table and then
/// reads and writes `sig_action[7]`. ERR-terminal-12's disposition is
/// `define`, and the definition chosen here is `None`: a signal libedit never
/// armed has no saved disposition, so there is nothing to restore and nothing
/// to blank.
fn sighdl_index(signo: i32) -> Option<usize> {
    SIGHDL.iter().position(|&s| s == signo)
}

/// The POSIX signal primitives this module is written against — and the one
/// place the port cannot reach.
///
/// `plan/decisions/no-c-ffi.md` bars the `libc` crate and Rust's standard
/// library exposes no signal API, so `sigaction`, `sigprocmask` (properly
/// `pthread_sigmask`, ERR-terminal-55), `raise` and the
/// `sigemptyset`/`sigaddset` pair have no caller available here. The gap is
/// wider than the calls: [`SigAction`] and [`SigSet`] are `def`-rule types
/// this translation may not change, and both are declared as placeholders
/// carrying no state, so even with a syscall layer in hand there is nowhere
/// to put a displaced `struct sigaction` or the bits of a `sigset_t`.
///
/// So the operations are named exactly, one function each, and every one of
/// them reports failure today. That is not a silent no-op: "every `sigaction`
/// and every `sigprocmask` failed" is a state the C itself defines and
/// swallows — `sem:sig.sig-set-fn` step 4 keeps the slot as it is on failure
/// and `sem:sig.sig-clr-fn` step 2 then skips it — so the translation above
/// degrades to *nothing armed, nothing saved, nothing restored, nothing
/// reported*, exactly as the C does on a platform that refuses every call.
/// No path here panics and none pretends to have installed anything.
///
/// What has to arrive for the module to function is listed once, here:
///
/// 1. Real bodies for [`SigAction`] and [`SigSet`] (a `def`-rule change), or
///    an ABI-crate-owned table these hand out handles into.
/// 2. The three syscalls, issued without libc.
/// 3. The async-signal-safe C handler that [`install_handler`] installs, and
///    the process-global instance registration it reaches its `EditLine`
///    through. Both belong to the ABI crate — see the note on [`sig_handler`]
///    and on [`sig_set`].
mod plat {
    use super::{SigAction, SigSet};

    /// What POSIX `sigaction(signo, &nsa, &osa)` says about the disposition
    /// it displaced. The C spells this as
    /// `sigaction(...) != -1 && osa.sa_handler != sig_handler`; only the
    /// platform layer can compare handler identity, so the whole test is
    /// answered here.
    pub(super) enum Installed {
        /// Installed, and what it displaced was somebody else's disposition.
        /// This is the case that fills a `sig_action[]` slot.
        Displaced(SigAction),
        /// Installed, but what it displaced was already our own handler. The
        /// idempotence guard of `sem:sig.sig-set-fn` step 4: the slot must be
        /// left alone, or `sig_clr` would later install libedit's own handler
        /// permanently.
        AlreadyOurs,
        /// The call failed — the C's -1. The slot keeps whatever it held.
        Failed,
    }

    /// `sigemptyset` plus one `sigaddset` per trapped signal, in table order:
    /// the cached mask `sig_init` builds and `sig_set`/`sig_clr` block with.
    pub(super) fn trapped_sigset() -> SigSet {
        SigSet { _placeholder: () }
    }

    /// `sigprocmask(SIG_BLOCK, set, &oset)`. `None` is the C's -1.
    pub(super) fn sigmask_block(_set: &SigSet) -> Option<SigSet> {
        None
    }

    /// `sigemptyset(&nset)`, `sigaddset(&nset, signo)`,
    /// `sigprocmask(SIG_BLOCK, &nset, &oset)` — the handler's step 2.
    pub(super) fn sigmask_block_one(_signo: i32) -> Option<SigSet> {
        None
    }

    /// `sigprocmask(SIG_SETMASK, oset, NULL)`. `false` is the C's -1.
    pub(super) fn sigmask_set(_oset: &SigSet) -> bool {
        false
    }

    /// `sigaction(signo, &nsa, &osa)` where `nsa` is the fully initialised
    /// action for the ABI crate's handler: `SA_ONSTACK`, no `SA_RESTART` (an
    /// interrupted `read` must fail with `EINTR`, which is how the read loop
    /// learns a signal arrived), and — departing from the C, whose empty
    /// `sa_mask` lets the other six nest and re-enter everything — an
    /// `sa_mask` holding all seven trapped signals. Every member is
    /// initialised, not just the three the C assigns (ERR-terminal-15).
    pub(super) fn install_handler(_signo: i32) -> Installed {
        Installed::Failed
    }

    /// `sigaction(signo, osa, NULL)`, putting a saved disposition back. Takes
    /// the disposition by value because the callers consume the slot in the
    /// same breath. `false` is the C's -1.
    pub(super) fn restore_handler(_signo: i32, _osa: SigAction) -> bool {
        false
    }

    /// `raise(signo)` — POSIX defines it as
    /// `pthread_kill(pthread_self(), signo)`, so the re-raised signal is
    /// delivered to this same thread. `false` is the C's non-zero return.
    pub(super) fn raise(_signo: i32) -> bool {
        false
    }
}

// [spec:libedit:def:sig.sig-handler-fn]
// [spec:libedit:sem:sig.sig-handler-fn]
/// The handler body for all seven trapped signals: record `signo`, put the
/// terminal into a sane state, restore the previous disposition and re-raise.
///
/// The C's handler takes only `signo`, because a C signal handler carries no
/// user data, and reaches its `EditLine` through the file-static `sel` that
/// `sig_set` assigns. That global is a C-shaped compatibility artifact and
/// belongs in the ABI crate, not here (`plan/decisions/idiomatic-core.md`),
/// so the instance is a parameter: whatever registration mechanism the ABI
/// crate uses to find it is what supplies this argument.
pub(crate) fn sig_handler(el: &mut EditLine, signo: i32) {
    // ERR-terminal-14, disposition `define`: the C runs every step below in
    // async-signal context, where `el_resize` reaches `calloc`/`free` and
    // `ioctl`, `terminal__flush` is `fflush` on a `FILE *`, `tty_rawmode` can
    // reach `keymacro_delete` -> `free`, and the empty `sa_mask` lets a
    // second trapped signal nest and re-enter all of it. None of that is
    // reproduced. This function is NOT the installed handler: it is the
    // deferred body, run from ordinary context on the editing thread. The
    // `&mut EditLine` in the signature says so on its own — an exclusive
    // borrow of the editor cannot exist inside a real handler that has
    // interrupted the editor, and cannot be handed to a separate `sigwait`
    // thread while the editing thread holds it either, so the signature
    // admits exactly one design and this is it.
    //
    // The division of labour that makes it observably equivalent:
    //   - The ABI crate's installed handler does only async-signal-safe work:
    //     store `signo` into `sig_no` (a `sig_atomic_t`-shaped atomic) and
    //     optionally `write()` one byte to a self-pipe. No allocation, no
    //     lock, no buffered writer.
    //   - The read loop, which already polls `sig_no` after a `read` that
    //     failed with `EINTR` (`sem:read.read-char-fn`), calls this. Every
    //     unsafe thing the C did in the handler happens here instead, where
    //     the heap and the streams are consistent and re-entrancy is not
    //     possible.
    //   - The observable contract of `sem:sig.sig-handler-fn` is unchanged:
    //     the tty is restored before the previous disposition takes effect,
    //     the signal number reaches the read loop, and the previously
    //     installed handler still runs, on this thread.
    // The cost is timing, and it is stated rather than hidden: the tty work
    // and the re-raise happen when the read loop next pumps rather than
    // inside the delivery itself. For the terminating and stopping signals
    // the process therefore dies or stops just after the interrupted `read`
    // returns instead of inside it — same order, same cooked tty, later
    // instant. With no read in flight the work waits, which is the window
    // ERR-terminal-56 already documents for the C.

    // Step 1 (`save_errno = errno`) and step 9 have no counterpart. There is
    // no `errno` in this crate to disturb, and the C's save/restore exists
    // only to hide the handler from code it interrupted — which, running
    // deferred, this no longer interrupts.

    // The C dereferences `sel->el_signal` with no NULL check. Defined here as
    // "nothing was ever armed, so there is nothing to unwind".
    if el.el_signal.is_none() {
        return;
    }

    // Step 2. Blocking `signo` is what the kernel did for the C before entry;
    // here it has to be done explicitly, and it is load-bearing for step 8.
    // `oset` is the mask of the code we are running on behalf of.
    let oset = plat::sigmask_block_one(signo);

    // Step 3. The handler's only channel to the read loop. The installed
    // handler has already stored this; storing it again is idempotent and
    // keeps the channel correct if a future caller pumps from elsewhere.
    if let Some(sig) = el.el_signal.as_deref() {
        sig.sig_no.store(signo, Ordering::Relaxed);
    }

    // Step 4.
    match signo {
        signo::SIGCONT => {
            // Back from a job-control stop: the tty is in the application's
            // cooked settings, so put libedit's edit-mode termios back.
            tty_rawmode(el);
            // The C's `if (ed_redisplay(el, 0) == CC_REFRESH) re_refresh(el);`
            // is dead — `ed_redisplay` always returns `CC_REDISPLAY` (8) and
            // `CC_REFRESH` is 4 (ERR-terminal-65) — so it is not ported. The
            // real redraw after a resume is the read loop's `EL_REFRESH`.
            terminal__flush(el);
        }
        signo::SIGWINCH => {
            // Re-read the window size and, if it changed, resize and clear
            // the display buffers. This is the allocation the C performs in
            // async-signal context; deferring it is the whole point. Written
            // out in full rather than imported: `el_resize` next to
            // `EditLine` in one `use` is the single ordering rustfmt's 2015
            // and 2024 style editions disagree about.
            crate::el::el_resize(el);
        }
        _ => {
            // SIGINT, SIGTSTP, SIGQUIT, SIGHUP, SIGTERM: restore the
            // application's termios *before* the signal takes its real
            // effect, so a process about to stop or die does not leave the
            // tty in raw mode. This is the entire reason the handler exists.
            tty_cookedmode(el);
        }
    }

    // Steps 5 and 6. `Option::take` is both halves of the C: it yields the
    // saved disposition and blanks the slot to the "nothing saved here"
    // state that the C writes as `SIG_ERR`. `None` means nothing was saved,
    // and ERR-terminal-13's definition applies — leave the disposition alone
    // rather than hand `SIG_ERR` to `sigaction` as a handler. This is also
    // what de-installs us for `signo`, which is why the read loop re-arms
    // with `sig_set` after a SIGWINCH or SIGCONT.
    let saved = sighdl_index(signo).and_then(|i| {
        el.el_signal
            .as_deref_mut()
            .and_then(|sig| sig.sig_action[i].take())
    });
    if let Some(osa) = saved {
        plat::restore_handler(signo, osa);
    }

    // Steps 7 and 8, in the order the C fixes and the one the rule calls the
    // observable contract: disposition restored, then the signal re-raised
    // while it is still blocked, so the previous handler or the default
    // action runs *after* we are done rather than inline, with the tty
    // already back in cooked mode.
    //
    // Step 7 itself is a no-op here and that is not a shortcut. The C
    // restores the entry mask because a nested handler may have moved it; in
    // this body nothing between step 2 and here touches the mask, so the mask
    // in force is already "the entry mask, with `signo` still blocked" — the
    // exact state the C's `SIG_SETMASK oset` produces.
    plat::raise(signo);

    // The C's delivery point is the kernel's return from the handler, which
    // unblocks `signo` and lets the pending signal through. Deferred, the
    // unblock is explicit and lands here: restoring the mask captured at step
    // 2 is what delivers the re-raised signal to the disposition just put
    // back. Nothing may be added between the raise and this line.
    if let Some(oset) = oset {
        plat::sigmask_set(&oset);
    }
}

// [spec:libedit:def:sig.sig-init-fn]
// [spec:libedit:sem:sig.sig-init-fn]
/// Allocate the signal state and cache the mask of the seven trapped
/// signals. 0 on success, -1 if the allocation failed.
pub(crate) fn sig_init(el: &mut EditLine) -> i32 {
    // Step 1. The C's `malloc` failure is the function's only failure mode
    // and its only `-1`; `Box` aborts instead of returning null, so that
    // return can no longer occur and with it goes the latent NULL
    // dereference the rule records for `el_init_internal`'s discarded result.
    // The signature keeps the C's `int` because callers are written to it.
    //
    // Assigning over an existing state drops it. The C would leak it, but
    // nothing calls `sig_init` twice on one `EditLine`, so this is not
    // observable.
    el.el_signal = Some(Box::new(ElSignal {
        // Step 4. `None` is the port's sentinel for the C's `SIG_ERR` slot,
        // which the rule asks for explicitly: an option, consumed on restore.
        sig_action: [const { None }; ALLSIGSNO],
        // Step 2. Exactly the seven, in table order.
        sig_set: plat::trapped_sigset(),
        // The C leaves `sig_no` holding whatever `malloc` returned
        // (ERR-terminal-15); zeroed here, as that entry's definition
        // requires.
        sig_no: AtomicI32::new(0),
    }));

    // Step 3, the `sigprocmask` pair around the initialisation loop, is not
    // ported. The rule marks it vestigial and says a port may drop it: no
    // handler of ours is installed yet, so nothing can observe the
    // half-initialised state, and its only effect is to delay any of the
    // seven that lands in the window.

    // Step 5.
    0
}

// [spec:libedit:def:sig.sig-end-fn]
// [spec:libedit:sem:sig.sig-end-fn]
/// Tear the signal state down. The rule requires the port to restore the
/// dispositions and drop the handler's registration first, which the C does
/// not do.
pub(crate) fn sig_end(el: &mut EditLine) {
    // The C is two statements — `el_free(el->el_signal); el->el_signal =
    // NULL;` — and frees the state while its own handler may still be the
    // installed disposition for up to seven signals, leaving the next
    // delivery a use-after-free of both the state and the `EditLine`
    // (ERR-terminal-18). The rule requires the inverted order, and requires
    // it to be a property of this function rather than of caller discipline:
    //
    //   1. restore the dispositions,
    //   2. drop the registration that carries the instance to the handler,
    //   3. free last.
    //
    // Step 1. Unconditional, and safe when `read_finish` already did it:
    // every slot is then `None` and this restores nothing. This is what
    // covers the paths that never reach `read_finish` — a `longjmp` out of a
    // read, an `el_end` from an application handler, `EL_SIGNAL` toggled off
    // mid-read (ERR-input-22).
    sig_clr(el);

    // Step 2 belongs to the ABI crate: it owns the process-global instance
    // registration that stands in for the C's file-static `sel`, and must
    // clear it here. The C never does, so after `el_end` that pointer dangles
    // for the rest of the process. Nothing in this crate holds it, which is
    // why there is nothing to clear at this line.

    // Step 3. Dropping the `Box` is the C's `el_free` plus its NULL
    // assignment, and the type makes the C's dangling `el_signal`
    // unrepresentable.
    el.el_signal = None;
}

// [spec:libedit:def:sig.sig-set-fn]
// [spec:libedit:sem:sig.sig-set-fn]
/// Install [`sig_handler`] for all seven signals, saving the dispositions it
/// displaces. This is where the C assigns the file-static `sel`, so it is
/// where the port registers `el` with whatever carries it to the handler.
pub(crate) fn sig_set(el: &mut EditLine) {
    // No NULL check in the C. Defined here as "no state, nothing to arm".
    let Some(sig) = el.el_signal.as_deref_mut() else {
        return;
    };

    // Step 1, the new action, is built inside `plat::install_handler`: only
    // the platform layer knows the handler's identity, and folding the
    // construction in with the call is what keeps every member initialised
    // rather than the C's three (ERR-terminal-15). Its `sa_mask` holds all
    // seven rather than being empty, so a second trapped signal cannot nest
    // inside the first — see the note on `install_handler`.
    //
    // Step 2 is the C's `sel = el`. It has no counterpart in this crate: the
    // instance reaches `sig_handler` as a parameter, and the registration
    // that supplies it is the ABI crate's (`plan/decisions/idiomatic-core.md`).
    // Two requirements land on that crate here. It must publish the instance
    // *inside* the blocked window below, not before it as the C does — the
    // rule names the race a signal in that gap opens, and says publishing
    // under the block closes it. And it must serialise arming: dispositions
    // are process-wide while this call comes from whichever thread happens to
    // be editing (ERR-terminal-55).

    // Step 3. Block all seven for the duration of the loop so that a
    // delivery cannot land between two installs and rewrite a slot
    // underneath us.
    let oset = plat::sigmask_block(&sig.sig_set);

    // Step 4. Fixed table order, one slot per signal.
    for (slot, signo) in sig.sig_action.iter_mut().zip(SIGHDL) {
        match plat::install_handler(signo) {
            // Installed, and we displaced somebody else: save it.
            plat::Installed::Displaced(osa) => *slot = Some(osa),
            // The idempotence guard. `sig_set` is legitimately re-issued
            // without an intervening `sig_clr` — the read loop re-arms after
            // a SIGWINCH- or SIGCONT-interrupted read, because the handler
            // de-installed itself — and recording our own handler as "the
            // previous one" is what would make `sig_clr` install libedit's
            // handler permanently. A re-arm after the handler ran does not
            // need to refresh the slot: the handler restored the original and
            // emptied the slot, and this call displaces that same original
            // and saves it again.
            plat::Installed::AlreadyOurs => {}
            // The call failed, so nothing was installed. The slot keeps what
            // it held — which is the right outcome, since `sig_clr` will then
            // skip it.
            plat::Installed::Failed => {}
        }
    }

    // Step 5. Anything that arrived during the loop is delivered here, to the
    // freshly installed handler, and POSIX signal merging applies: a burst of
    // SIGWINCH inside the window collapses to one delivery.
    if let Some(oset) = oset {
        plat::sigmask_set(&oset);
    }
}

// [spec:libedit:def:sig.sig-clr-fn]
// [spec:libedit:sem:sig.sig-clr-fn]
/// Put back the dispositions [`sig_set`] saved, consuming each slot so it is
/// not re-installed by a later unpaired call.
pub(crate) fn sig_clr(el: &mut EditLine) {
    // No NULL check in the C, and calling it after `sig_end` is an immediate
    // NULL dereference there. Defined here as "nothing saved, nothing to do".
    let Some(sig) = el.el_signal.as_deref_mut() else {
        return;
    };

    // Step 1. Block the seven so a delivery cannot rewrite a slot between two
    // restores.
    let oset = plat::sigmask_block(&sig.sig_set);

    // Step 2. `take` is the fix the rule asks for (ERR-terminal-54): the C
    // leaves a restored slot holding its saved disposition, so an application
    // that turns `EL_SIGNAL` on part-way through a read reaches an unpaired
    // `sig_clr` and libedit re-installs dispositions captured during an
    // earlier `el_wgets`, clobbering whatever the application installed since.
    // Consuming the slot makes the second restore a no-op instead.
    //
    // An empty slot means either that `sig_set`'s install failed or that the
    // signal fired and `sig_handler` already restored and consumed it; either
    // way the current disposition is the right one and is left alone.
    for (slot, signo) in sig.sig_action.iter_mut().zip(SIGHDL) {
        if let Some(osa) = slot.take() {
            plat::restore_handler(signo, osa);
        }
    }

    // Step 3. Any of the seven that arrived during the loop is delivered
    // here, now going to the application's own disposition rather than ours.
    if let Some(oset) = oset {
        plat::sigmask_set(&oset);
    }
}
