//! Ported from `src/sig.c`; rules live in `docs/spec/port/src/sig.md`.

use core::sync::atomic::AtomicI32;

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
