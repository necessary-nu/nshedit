//! Ported from `src/sig.c`; rules live in `docs/spec/port/src/sig.md`.

// The signatures land before the bodies, so every parameter is unused until
// its `todo!()` is replaced. Remove this with the last one.
#![allow(unused_variables)]

use core::sync::atomic::AtomicI32;

use crate::el::EditLine;

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
fn sig_handler(el: &mut EditLine, signo: i32) {
    todo!()
}

// [spec:libedit:def:sig.sig-init-fn]
// [spec:libedit:sem:sig.sig-init-fn]
/// Allocate the signal state and cache the mask of the seven trapped
/// signals. 0 on success, -1 if the allocation failed.
pub(crate) fn sig_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:sig.sig-end-fn]
// [spec:libedit:sem:sig.sig-end-fn]
/// Tear the signal state down. The rule requires the port to restore the
/// dispositions and drop the handler's registration first, which the C does
/// not do.
pub(crate) fn sig_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:sig.sig-set-fn]
// [spec:libedit:sem:sig.sig-set-fn]
/// Install [`sig_handler`] for all seven signals, saving the dispositions it
/// displaces. This is where the C assigns the file-static `sel`, so it is
/// where the port registers `el` with whatever carries it to the handler.
pub(crate) fn sig_set(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:sig.sig-clr-fn]
// [spec:libedit:sem:sig.sig-clr-fn]
/// Put back the dispositions [`sig_set`] saved, consuming each slot so it is
/// not re-installed by a later unpaired call.
pub(crate) fn sig_clr(el: &mut EditLine) {
    todo!()
}
