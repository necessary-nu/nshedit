//! Ported from `src/sig.c`; rules live in `docs/spec/port/src/sig.md`.

use core::sync::atomic::{AtomicI32, Ordering};

use crate::el::EditLine;
use crate::terminal::terminal__flush;
use crate::tty::{tty_cookedmode, tty_rawmode};

/// C: `#define ALLSIGSNO 7` — `SIGINT`, `SIGTSTP`, `SIGQUIT`, `SIGHUP`,
/// `SIGTERM`, `SIGCONT`, `SIGWINCH`, in that fixed table order.
pub const ALLSIGSNO: usize = 7;

/// The C's `struct sigaction`: one saved previous disposition.
///
/// Transcribed in `nshedit-plat` alongside the termios ABI, because
/// `plan/decisions/platform-layer.md` puts the syscall that fills it there
/// and the layout is the platform's rather than this port's. libedit only
/// ever stores one of these wholesale and puts it back, so nothing in this
/// crate reads a field.
pub type SigAction = nshedit_plat::signal::SigAction;

/// The C's `sigset_t`: the cached mask of the seven trapped signals. Same
/// reasoning as [`SigAction`]; `sigemptyset` and `sigaddset` are bit
/// operations on it rather than linked symbols.
pub type SigSet = nshedit_plat::signal::SigSet;

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
/// POSIX names these signals; it does not fix their numbers, so the port
/// carries the platform ABI's numbering itself — in `nshedit-plat`, next to
/// the `sigaction` that consumes them, since `tty.rs` needs three of the same
/// constants and two copies is how the divergences got lost the first time.
pub(crate) use nshedit_plat::signal::signo;

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

// The POSIX signal primitives this module is written against.
//
// `plan/decisions/platform-layer.md` moved them into `nshedit-plat`, so the
// module that used to stand here — with every call reporting a permanent
// failure, and [`SigAction`]/[`SigSet`] carrying no state — is gone and the
// name is an import. rustix declines the signal family on principle, so
// those three are libc symbols there, under the second site on
// `plan/decisions/no-c-ffi.md`'s enumeration; nothing in *this* crate names
// one.
//
// Two things the platform crate owns that this module used to describe as
// missing:
//
// - The installed handler. `plat::install_handler` arms
//   `nshedit_plat::signal`'s own async-signal-safe trampoline, which does
//   nothing but record the signal number; [`sig_handler`] below is the
//   *body*, run deferred from the read loop.
// - The registration that carries the instance to it. `plat::set_signal_slot`
//   is the port's counterpart of the C's file-static `sel`, and [`sig_set`]
//   publishes into it inside the blocked window rather than before it, which
//   is what the rule asks for.
use nshedit_plat::signal as plat;

// [spec:libedit:def:sig.sig-handler-fn]
// [spec:libedit:sem:sig.sig-handler-fn]
/// The handler body for all seven trapped signals: record `signo`, put the
/// terminal into a sane state, restore the previous disposition and re-raise.
///
/// The C's handler takes only `signo`, because a C signal handler carries no
/// user data, and reaches its `EditLine` through the file-static `sel` that
/// `sig_set` assigns. Here the instance is a parameter, and what the installed
/// trampoline reaches through the process-global slot is one atomic rather
/// than the whole editor — see [`sig_set`].
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
    //   - `nshedit_plat::signal`'s installed trampoline does only
    //     async-signal-safe work: store `signo` into `sig_no` (a
    //     `sig_atomic_t`-shaped atomic) reached through the process-global
    //     slot `sig_set` published. No allocation, no lock, no buffered
    //     writer.
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
    // trampoline has already stored this; storing it again is idempotent and
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
        // The C discards this, as it discards every result in the family.
        let _ = plat::restore_handler(signo, osa);
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
    let _ = plat::raise(signo);

    // The C's delivery point is the kernel's return from the handler, which
    // unblocks `signo` and lets the pending signal through. Deferred, the
    // unblock is explicit and lands here: restoring the mask captured at step
    // 2 is what delivers the re-raised signal to the disposition just put
    // back. Nothing may be added between the raise and this line.
    if let Some(oset) = oset {
        let _ = plat::sigmask_set(&oset);
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
        sig_set: SigSet::of(&SIGHDL),
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

    // Step 2 is `sig_clr`'s doing, one line above: it clears the
    // process-global registration that stands in for the C's file-static
    // `sel`, so the trampoline has nowhere to write by the time the box goes.
    // The C never clears `sel` at all, so after `el_end` that pointer dangles
    // for the rest of the process (ERR-terminal-18).

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
    // Step 3, hoisted above step 2 deliberately. Block all seven for the
    // duration of the loop so that a delivery cannot land between two
    // installs and rewrite a slot underneath us.
    let mask = sig.sig_set;
    let oset = plat::sigmask_block(&mask);

    // Step 2, the C's `sel = el`. What the handler needs to reach is the one
    // field it writes, `el_signal->sig_no`, so that address is what gets
    // published rather than the `EditLine`. It is published *inside* the
    // blocked window rather than before it as the C does, which is what the
    // rule asks for: it names the race a signal in that gap opens, and says
    // publishing under the block closes it.
    //
    // Not closed, and the C's too: dispositions are process-wide while this
    // call comes from whichever thread happens to be editing, so two
    // `EditLine`s arming concurrently leave the second one's slot published
    // (ERR-terminal-55). Serialising that is the caller's, exactly as it is
    // in the C.
    //
    // SAFETY: `sig` is the boxed `ElSignal`, so `sig_no`'s address is stable
    // for as long as the box lives, and the box is dropped only by `sig_end`
    // — which calls `sig_clr` first, and `sig_clr` clears the registration.
    unsafe { plat::set_signal_slot(&raw const sig.sig_no) };

    // Step 4. Fixed table order, one slot per signal. Step 1, the new action,
    // is built inside `plat::install_handler`: only the platform layer knows
    // the handler's identity, and folding the construction in with the call
    // is what keeps every member initialised rather than the C's three
    // (ERR-terminal-15). Its `sa_mask` is the cached seven rather than the
    // C's empty set, so a second trapped signal cannot nest inside the first.
    for (slot, signo) in sig.sig_action.iter_mut().zip(SIGHDL) {
        match plat::install_handler(signo, &mask) {
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
        let _ = plat::sigmask_set(&oset);
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
            let _ = plat::restore_handler(signo, osa);
        }
    }

    // The counterpart of `sig_set`'s step 2, which the C has no counterpart
    // for at all: nothing of ours is installed any more, so the trampoline
    // has nothing to record into and the registration comes off. This is what
    // `sem:sig.sig-end-fn` requires happen before the state is freed, and
    // doing it here rather than only in `sig_end` also covers the paths that
    // reach `sig_clr` and never reach `sig_end`. Still inside the block, so a
    // delivery cannot arrive between the last restore and this line.
    plat::clear_signal_slot();

    // Step 3. Any of the seven that arrived during the loop is delivered
    // here, now going to the application's own disposition rather than ours.
    if let Some(oset) = oset {
        let _ = plat::sigmask_set(&oset);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::el::blank_editline;

    /// Signal dispositions are process-wide, so the tests below cannot run at
    /// the same time as each other — `cargo test` runs them on threads of one
    /// process — and every one of them must put back what it installed before
    /// it returns. This is that serialisation. Nothing else in the crate's
    /// tests touches a disposition; if something ever does, it belongs behind
    /// this lock too.
    ///
    /// The poison is stepped over deliberately: a panic in one test would
    /// otherwise turn the other's report into "poisoned mutex" and hide the
    /// assertion that actually failed.
    static SIGNALS: Mutex<()> = Mutex::new(());

    fn serialised() -> std::sync::MutexGuard<'static, ()> {
        SIGNALS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every signal here is `SIGWINCH`, whose default disposition is *ignore*.
    /// A test that leaves a disposition unrestored — or one that reaches the
    /// re-raise in [`sig_handler`] — therefore cannot kill the test runner,
    /// which is not true of any other member of the seven.
    const PROBE: i32 = signo::SIGWINCH;

    /// An editor with signal state allocated and no streams: the three
    /// descriptors a `calloc`ed `EditLine` carries are 0, and `el_infd` at -1
    /// is also what makes `el_resize` a no-op, since `TIOCGWINSZ` cannot
    /// answer and the loaded capability values are zero.
    fn el() -> EditLine {
        let mut el = blank_editline();
        el.el_infd = -1;
        el.el_outfd = -1;
        el.el_errfd = -1;
        assert_eq!(sig_init(&mut el), 0);
        el
    }

    fn saw(el: &EditLine) -> i32 {
        el.el_signal
            .as_deref()
            .expect("sig_init ran")
            .sig_no
            .load(Ordering::Relaxed)
    }

    fn slots(el: &EditLine) -> [bool; ALLSIGSNO] {
        let sig = el.el_signal.as_deref().expect("sig_init ran");
        std::array::from_fn(|i| sig.sig_action[i].is_some())
    }

    /// Whether libedit's own handler is the current disposition for `PROBE`,
    /// asking the only layer that can compare handler identity. The probe is
    /// undone before it returns, so it is safe to call between assertions.
    fn ours_is_installed() -> bool {
        match plat::install_handler(PROBE, &SigSet::of(&[PROBE])) {
            plat::Installed::AlreadyOurs => true,
            plat::Installed::Displaced(osa) => {
                assert!(plat::restore_handler(PROBE, osa), "the probe must undo");
                false
            }
            plat::Installed::Failed => panic!("sigaction failed"),
        }
    }

    /// Arming publishes the registration the installed trampoline records
    /// through — the port's counterpart of the C's file-static `sel` — and
    /// fills one slot per signal with the disposition it displaced.
    ///
    /// The part that is neither in the C nor visible to a differential is the
    /// idempotence guard. `sig_set` is legitimately re-issued without an
    /// intervening `sig_clr`, because the handler de-installs itself for the
    /// signal that fired and the read loop re-arms; recording *our own*
    /// handler as "the previous one" on that second pass is what would make
    /// `sig_clr` install libedit's handler permanently. The only way to see
    /// that is to arm twice, clear, and ask what is installed.
    // [spec:libedit:sem:sig.sig-set-fn/test]
    #[test]
    fn arming_twice_still_restores_the_application_disposition() {
        let _guard = serialised();
        let mut el = el();

        sig_set(&mut el);
        assert_eq!(slots(&el), [true; ALLSIGSNO], "one slot per trapped signal");
        assert!(ours_is_installed());

        // The registration reaches the trampoline: a real delivery lands in
        // `sig_no`, which is the handler's only channel to the read loop.
        assert!(plat::raise(PROBE));
        assert_eq!(saw(&el), PROBE);

        // The re-arm. Every slot must still hold the *application's*
        // disposition, not ours.
        sig_set(&mut el);
        assert_eq!(slots(&el), [true; ALLSIGSNO]);

        sig_clr(&mut el);
        assert_eq!(slots(&el), [false; ALLSIGSNO], "each slot is consumed");
        assert!(
            !ours_is_installed(),
            "a re-arm recorded our own handler as the previous one"
        );

        // `sig_clr` also drops the registration, which the C never does — its
        // `sel` dangles for the rest of the process after `el_end`.
        el.el_signal
            .as_deref()
            .expect("sig_init ran")
            .sig_no
            .store(0, Ordering::Relaxed);
        assert!(plat::raise(PROBE));
        assert_eq!(saw(&el), 0, "the trampoline still had somewhere to write");

        sig_end(&mut el);
    }

    /// The deferred handler body de-installs itself for the signal it ran
    /// for: it consumes that one slot, puts the displaced disposition back,
    /// and leaves the other six alone. That is why the read loop re-arms with
    /// `sig_set` after a `SIGWINCH` or `SIGCONT`, and it is the whole
    /// observable contract — the number reaches the read loop, the tty work
    /// happens before the previous disposition takes effect, and the previous
    /// handler still runs.
    ///
    /// A signal that is not in the table restores nothing. The C's scan stops
    /// on the array's `-1` terminator with the index one past the end and
    /// then reads and writes `sig_action[7]` (ERR-terminal-12); the lookup
    /// here is total. Signal 0 stands in for that case because it is not a
    /// signal at all, so the re-raise cannot deliver anything — picking a
    /// live signal outside the seven would mean picking one whose default
    /// action does not terminate the test runner, on every target.
    // [spec:libedit:sem:sig.sig-handler-fn/test]
    #[test]
    fn the_handler_body_deinstalls_itself_for_the_signal_that_fired() {
        let _guard = serialised();
        let mut el = el();
        sig_set(&mut el);

        sig_handler(&mut el, PROBE);
        assert_eq!(saw(&el), PROBE, "the read loop's channel");
        assert!(
            !ours_is_installed(),
            "the handler must restore the disposition it displaced"
        );
        // `SIGWINCH` is the last of the seven, so only the tail is consumed.
        assert_eq!(
            slots(&el),
            [true, true, true, true, true, true, false],
            "one slot consumed, the other six untouched"
        );

        // The re-arm the read loop issues: the same original is displaced and
        // saved again.
        sig_set(&mut el);
        assert_eq!(slots(&el), [true; ALLSIGSNO]);

        // A second body for the same signal, with nothing left saved. The C
        // would hand `SIG_ERR` to `sigaction` here (ERR-terminal-13); the
        // definition is to leave the disposition alone.
        sig_handler(&mut el, PROBE);
        sig_handler(&mut el, PROBE);
        assert!(!ours_is_installed());

        // Not in the table: nothing is restored and no slot is touched.
        let before = slots(&el);
        sig_handler(&mut el, 0);
        assert_eq!(slots(&el), before);
        assert_eq!(saw(&el), 0, "step 3 stores whatever it was called with");

        sig_clr(&mut el);
        assert!(!ours_is_installed());
        sig_end(&mut el);
    }

    /// `sig_end` must restore before it frees. The C does neither in that
    /// order — it frees the state while its own handler may still be the
    /// installed disposition for up to seven signals, so the next delivery is
    /// a use-after-free of both the state and the `EditLine`
    /// (ERR-terminal-18). Here it is a property of the function rather than
    /// of caller discipline, which is what covers the paths that never reach
    /// `read_finish`: a `longjmp` out of a read, an `el_end` from an
    /// application handler, `EL_SIGNAL` toggled off mid-read.
    #[test]
    fn tearing_down_without_a_read_finish_still_restores_the_dispositions() {
        let _guard = serialised();
        let mut el = el();
        sig_set(&mut el);
        assert!(ours_is_installed());

        sig_end(&mut el);
        assert!(el.el_signal.is_none());
        assert!(!ours_is_installed(), "sig_end left our handler installed");

        // And the no-state paths, where the C dereferences NULL outright.
        sig_set(&mut el);
        sig_clr(&mut el);
        sig_handler(&mut el, PROBE);
        sig_end(&mut el);
        assert!(!ours_is_installed());
    }
}
