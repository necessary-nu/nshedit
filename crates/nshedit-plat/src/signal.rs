//! Scoped signal handling, and the first of this workspace's two libc
//! `extern` blocks.
//!
//! rustix declines these on principle. Its `not_implemented.rs` lists
//! `sigaction`, `sigprocmask`, `sigwait` and `tkill` as deliberately out of
//! scope on the grounds that a libc expects to be involved in signal
//! handling, and its `runtime` module's replacements are documented as
//! undefined behaviour in a process that has one — which `nshedit-abi`'s
//! `cdylib` always is, and which nsh is too, since it links `std`. A
//! hand-rolled `rt_sigaction` is worse: it would need an `SA_RESTORER`
//! trampoline in assembly per architecture and is unsound for the same
//! documented reason.
//!
//! So `sigaction`, `sigprocmask` and `raise` are named out to the platform's
//! libc, under the second site on `plan/decisions/no-c-ffi.md`'s enumeration
//! and the widening `plan/decisions/platform-layer.md` argues for. Everything
//! *around* them is ordinary Rust: `sigemptyset` and `sigaddset` are bit
//! operations on the private transcribed `SigSet` and are not linked.
//!
//! # Handler ownership
//!
//! `sig_trampoline` is what `sigaction` installs, and it is the whole of
//! the async-signal-safe work: a lock-free atomic state transition, no
//! allocation, no lock, no buffered write. It records the signal number in
//! process-lifetime storage while one [`SignalHandlers`] activation owns it.
//! The read loop observes that value and performs terminal transitions,
//! resize work, and previous-disposition propagation from ordinary context.
//!
//! Signal dispositions are process-global, so only one scoped owner may be
//! live at a time. Construction claims that ownership atomically; destruction
//! restores every displaced disposition before withdrawing its activation.
//! The handler never receives a pointer into scoped storage, so a delivery
//! racing teardown cannot dereference a freed owner. This rejects the C
//! implementation's last-editor-wins pointer overwrite and makes both normal
//! return and unwinding restore caller policy.

use core::ffi::c_int;
#[cfg(any(target_os = "linux", target_os = "android"))]
use core::ffi::{c_ulong, c_void};
#[cfg(test)]
use core::sync::atomic::AtomicI32;
use core::sync::atomic::{AtomicU64, Ordering};

mod handlers;

pub use handlers::{BlockedSignals, SignalError, SignalHandlers};

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
compile_error!("nshedit-plat has no signal ABI transcription for this target");

/// The signal numbers this port names.
///
/// POSIX names signals; it does not fix their numbers, and the libc that
/// would define them is not what supplies constants here. Five of the seven
/// libedit traps agree everywhere in scope; only `SIGCONT` and `SIGTSTP`
/// differ between GNU x86-64 Linux and Darwin. Other Linux targets are
/// rejected at the crate boundary rather than inheriting these numbers.
mod signo {
    pub const SIGHUP: i32 = 1;
    pub const SIGINT: i32 = 2;
    pub const SIGQUIT: i32 = 3;
    pub const SIGTERM: i32 = 15;
    pub const SIGWINCH: i32 = 28;

    /// Linux's generic ABI numbers `SIGCONT` 18 and `SIGTSTP` 20; the BSDs
    /// and Darwin swap them to 19 and 18.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub const SIGCONT: i32 = 18;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub const SIGTSTP: i32 = 20;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    pub const SIGCONT: i32 = 19;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    pub const SIGTSTP: i32 = 18;
}

/// One of the terminal signals an interactive editor can temporarily own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    /// Loss of the controlling session (`SIGHUP`).
    Hangup,
    /// Interactive interruption (`SIGINT`).
    Interrupt,
    /// Interactive quit (`SIGQUIT`).
    Quit,
    /// Termination request (`SIGTERM`).
    Terminate,
    /// Job-control stop (`SIGTSTP`).
    Suspend,
    /// Job-control continuation (`SIGCONT`).
    Continue,
    /// Terminal-size change (`SIGWINCH`).
    Resize,
}

impl Signal {
    /// The complete signal family owned by a signal-enabled editor read.
    pub const EDITOR: [Self; 7] = [
        Self::Interrupt,
        Self::Suspend,
        Self::Quit,
        Self::Hangup,
        Self::Terminate,
        Self::Continue,
        Self::Resize,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Interrupt => 0,
            Self::Suspend => 1,
            Self::Quit => 2,
            Self::Hangup => 3,
            Self::Terminate => 4,
            Self::Continue => 5,
            Self::Resize => 6,
        }
    }

    const fn number(self) -> i32 {
        match self {
            Self::Hangup => signo::SIGHUP,
            Self::Interrupt => signo::SIGINT,
            Self::Quit => signo::SIGQUIT,
            Self::Terminate => signo::SIGTERM,
            Self::Suspend => signo::SIGTSTP,
            Self::Continue => signo::SIGCONT,
            Self::Resize => signo::SIGWINCH,
        }
    }

    const fn from_number(number: i32) -> Option<Self> {
        match number {
            signo::SIGHUP => Some(Self::Hangup),
            signo::SIGINT => Some(Self::Interrupt),
            signo::SIGQUIT => Some(Self::Quit),
            signo::SIGTERM => Some(Self::Terminate),
            signo::SIGTSTP => Some(Self::Suspend),
            signo::SIGCONT => Some(Self::Continue),
            signo::SIGWINCH => Some(Self::Resize),
            _ => None,
        }
    }
}

/// One word in the host's `<sys/signal.h>` `sigset_t`.
#[cfg(any(target_os = "linux", target_os = "android"))]
type SigSetWord = c_ulong;
/// Darwin publishes `sigset_t` as one 32-bit mask.
#[cfg(target_os = "macos")]
type SigSetWord = u32;

/// The host `sigset_t` word count. glibc reserves 1024 bits; Darwin carries
/// the complete set in one 32-bit word.
#[cfg(any(target_os = "linux", target_os = "android"))]
const SIGSET_WORDS: usize = 1024 / SIGSET_BITS;
#[cfg(target_os = "macos")]
const SIGSET_WORDS: usize = 1;

/// The width of one `sigset_t` word, in bits.
const SIGSET_BITS: usize = SigSetWord::BITS as usize;

/// `SIG_BLOCK`.
#[cfg(any(target_os = "linux", target_os = "android"))]
const SIG_BLOCK: c_int = 0;
#[cfg(target_os = "macos")]
const SIG_BLOCK: c_int = 1;
/// `SIG_SETMASK`.
#[cfg(any(target_os = "linux", target_os = "android"))]
const SIG_SETMASK: c_int = 2;
#[cfg(target_os = "macos")]
const SIG_SETMASK: c_int = 3;

/// `SA_ONSTACK` — run on the alternate signal stack if the application
/// installed one.
#[cfg(any(target_os = "linux", target_os = "android"))]
const SA_ONSTACK: c_int = 0x0800_0000;
#[cfg(target_os = "macos")]
const SA_ONSTACK: c_int = 0x0001;

/// POSIX `sigset_t`, transcribed.
///
/// `sigemptyset`, `sigaddset` and `sigismember` are bit arithmetic over this
/// and are implemented below rather than linked: `plan/decisions/no-c-ffi.md`
/// rations the exception, and these need no ration.
#[repr(C)]
#[derive(Clone, Copy)]
struct SigSet {
    val: [SigSetWord; SIGSET_WORDS],
}

impl SigSet {
    /// `sigemptyset(&set)`.
    #[must_use]
    const fn empty() -> Self {
        Self {
            val: [0; SIGSET_WORDS],
        }
    }

    /// `sigaddset(&set, signo)`. Out-of-range numbers are dropped, where the
    /// C would answer `EINVAL` and libedit would ignore it.
    const fn add(&mut self, signo: i32) {
        if signo <= 0 {
            return;
        }
        let bit = (signo - 1) as usize;
        let word = bit / SIGSET_BITS;
        if word < SIGSET_WORDS {
            self.val[word] |= 1 << (bit % SIGSET_BITS);
        }
    }

    /// One set holding exactly the given signals, in the order named.
    #[must_use]
    #[cfg(test)]
    fn of(signos: &[i32]) -> Self {
        let mut set = Self::empty();
        for &s in signos {
            set.add(s);
        }
        set
    }
}

impl Default for SigSet {
    fn default() -> Self {
        Self::empty()
    }
}

/// The host's `<sys/signal.h>` `struct sigaction`, transcribed.
///
/// libedit only ever stores one of these wholesale and puts it back, so
/// nothing here reads the fields; they exist so that the displaced
/// disposition survives a round trip *intact*, which is what
/// `sem:sig.sig-clr-fn` promises the application.
#[repr(C)]
#[derive(Clone, Copy)]
struct SigAction {
    /// `sa_handler`/`sa_sigaction`, which are a union.
    handler: usize,
    mask: SigSet,
    flags: c_int,
    #[cfg(any(target_os = "linux", target_os = "android"))]
    restorer: *const c_void,
}

// SAFETY: the fields are a function address, a bit set and a flag word. The
// two pointers are never dereferenced here — they are handed straight back to
// `sigaction` — and a saved disposition is moved between the arming thread
// and nothing else. `sem:sig.sig-set-fn` already requires the caller to
// serialise arming.
unsafe impl Send for SigAction {}
// SAFETY: as above; the type is plain data with no interior mutability.
unsafe impl Sync for SigAction {}

impl SigAction {
    /// A fully initialised empty action suitable for an out-parameter.
    const fn empty() -> Self {
        Self {
            handler: 0,
            mask: SigSet::empty(),
            flags: 0,
            #[cfg(any(target_os = "linux", target_os = "android"))]
            restorer: core::ptr::null(),
        }
    }

    /// A fully initialised action for [`sig_trampoline`].
    ///
    /// Every member is set, not just the three the C assigns
    /// (ERR-terminal-15). Two departures from the C, both required by
    /// `sem:sig.sig-set-fn`'s translation:
    ///
    /// - `SA_RESTART` is **not** set, so an interrupted `read` fails with
    ///   `EINTR`. That is how the read loop learns a signal arrived.
    /// - `sa_mask` holds every trapped signal rather than being empty, so a
    ///   second one cannot nest inside the first and re-enter the editor.
    fn ours(mask: &SigSet) -> Self {
        Self {
            handler: sig_trampoline as *const () as usize,
            mask: *mask,
            flags: SA_ONSTACK,
            #[cfg(any(target_os = "linux", target_os = "android"))]
            restorer: core::ptr::null(),
        }
    }

    /// Whether this disposition is already ours.
    fn is_ours(&self) -> bool {
        self.handler == sig_trampoline as *const () as usize
    }
}

/// The x86-64 glibc transcription, checked against `<signal.h>`.
#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_vendor = "unknown",
    target_env = "gnu",
    target_pointer_width = "64",
))]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};

    assert!(size_of::<SigSet>() == 128);
    assert!(size_of::<SigAction>() == 8 + 128 + 8 + 8);
    assert!(align_of::<SigAction>() == align_of::<usize>());
    assert!(offset_of!(SigAction, handler) == 0);
    assert!(offset_of!(SigAction, mask) == 8);
    assert!(offset_of!(SigAction, flags) == 136);
    assert!(offset_of!(SigAction, restorer) == 144);
};

/// Darwin's published `<sys/signal.h>` layout: one 32-bit mask and no
/// `sa_restorer` member.
// [spec:nshedit:req:platform.per-os-layouts]
#[cfg(target_os = "macos")]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};

    assert!(size_of::<SigSet>() == 4);
    assert!(align_of::<SigSet>() == 4);
    assert!(size_of::<SigAction>() == 16);
    assert!(align_of::<SigAction>() == align_of::<usize>());
    assert!(offset_of!(SigAction, handler) == 0);
    assert!(offset_of!(SigAction, mask) == 8);
    assert!(offset_of!(SigAction, flags) == 12);
};

/// What POSIX `sigaction(signo, &nsa, &osa)` says about the disposition it
/// displaced.
///
/// The C spells this as `sigaction(...) != -1 && osa.sa_handler !=
/// sig_handler`; only this layer can compare handler identity, so the whole
/// test is answered here.
enum Installed {
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

/// The one libc `extern` block this module has.
mod sys {
    use core::ffi::c_int;

    use super::{SigAction, SigSet};

    unsafe extern "C" {
        pub(super) fn sigaction(
            signum: c_int,
            act: *const SigAction,
            oldact: *mut SigAction,
        ) -> c_int;
        pub(super) fn sigprocmask(how: c_int, set: *const SigSet, oldset: *mut SigSet) -> c_int;
        pub(super) fn raise(sig: c_int) -> c_int;
    }
}

// ---------------------------------------------------------------------------
// The installed handler and where it records
// ---------------------------------------------------------------------------

/// The lower half of [`SignalSlot::state`] carries the pending `c_int`.
const PENDING_MASK: u64 = u32::MAX as u64;
/// The upper bit distinguishes a live owner from an inactive generation.
const ACTIVE_BIT: u64 = 1 << 63;
/// Advancing by one in the upper half gives each activation a fresh token.
const GENERATION_STEP: u64 = 1 << 32;
const GENERATION_MASK: u64 = !(ACTIVE_BIT | PENDING_MASK);
const OWNER_MASK: u64 = !PENDING_MASK;

/// One scoped claim on the process-wide pending-signal state.
#[derive(Clone, Copy, PartialEq, Eq)]
struct SignalActivation(u64);

/// Process-lifetime state shared with [`sig_trampoline`].
///
/// A single atomic keeps ownership and the pending signal in one modification
/// order. In particular, a handler preempted after loading an activation can
/// only publish while that exact activation still owns the slot. Teardown or
/// a later owner changes the upper half, making the stale compare-exchange a
/// no-op.
struct SignalSlot {
    state: AtomicU64,
}

impl SignalSlot {
    const fn new() -> Self {
        Self {
            state: AtomicU64::new(0),
        }
    }

    fn activate(&self) -> Option<SignalActivation> {
        let mut observed = self.state.load(Ordering::Acquire);
        loop {
            if observed & ACTIVE_BIT != 0 {
                return None;
            }
            let mut generation =
                (observed & GENERATION_MASK).wrapping_add(GENERATION_STEP) & GENERATION_MASK;
            if generation == 0 {
                generation = GENERATION_STEP;
            }
            let activation = SignalActivation(ACTIVE_BIT | generation);
            match self.state.compare_exchange_weak(
                observed,
                activation.0,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(activation),
                Err(actual) => observed = actual,
            }
        }
    }

    fn record(&self, signo: c_int) {
        self.record_observed(self.state.load(Ordering::Relaxed), signo);
    }

    fn record_observed(&self, mut observed: u64, signo: c_int) {
        let owner = observed & OWNER_MASK;
        if owner & ACTIVE_BIT == 0 {
            return;
        }
        let desired = owner | (u64::from(signo.cast_unsigned()) & PENDING_MASK);
        loop {
            match self.state.compare_exchange_weak(
                observed,
                desired,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) if actual & OWNER_MASK == owner => observed = actual,
                Err(_) => return,
            }
        }
    }

    fn take(&self, activation: SignalActivation) -> Option<c_int> {
        let mut observed = self.state.load(Ordering::Acquire);
        loop {
            if observed & OWNER_MASK != activation.0 {
                return None;
            }
            let signo = (observed & PENDING_MASK) as u32 as c_int;
            if signo == 0 {
                return None;
            }
            match self.state.compare_exchange_weak(
                observed,
                activation.0,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(signo),
                Err(actual) => observed = actual,
            }
        }
    }

    fn deactivate(&self, activation: SignalActivation) -> bool {
        let mut observed = self.state.load(Ordering::Acquire);
        loop {
            if observed & OWNER_MASK != activation.0 {
                return false;
            }
            let inactive = activation.0 & GENERATION_MASK;
            match self.state.compare_exchange_weak(
                observed,
                inactive,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => observed = actual,
            }
        }
    }

    #[cfg(test)]
    fn force_inactive(&self) {
        self.state.fetch_and(GENERATION_MASK, Ordering::AcqRel);
    }
}

/// The port's counterpart of libedit's file-static `sel`.
///
/// A C signal handler carries no user data, so its communication storage must
/// be process-global. The activation embedded in the same word supplies the
/// scoped ownership that the storage itself deliberately does not have.
static PENDING_SLOT: SignalSlot = SignalSlot::new();

/// The disposition `sigaction` installs. Async-signal-safe by construction:
/// one atomic load followed by a compare-exchange, nothing else.
extern "C" fn sig_trampoline(signo: c_int) {
    PENDING_SLOT.record(signo);
}

// ---------------------------------------------------------------------------
// The family
// ---------------------------------------------------------------------------

/// `sigaction(signo, &nsa, &osa)` with `nsa` built by [`SigAction::ours`].
#[must_use]
fn install_handler(signo: i32, mask: &SigSet) -> Installed {
    install_handler_default(signo, mask)
}

/// `sigaction(signo, osa, NULL)`, putting a saved disposition back. Takes the
/// disposition by value because the callers consume the slot in the same
/// breath. `false` is the C's -1.
#[must_use]
fn restore_handler(signo: i32, osa: SigAction) -> bool {
    restore_handler_default(signo, osa)
}

/// `sigprocmask(SIG_BLOCK, set, &oset)`. `None` is the C's -1.
#[must_use]
fn sigmask_block(set: &SigSet) -> Option<SigSet> {
    sigmask_block_default(set)
}

/// `sigprocmask(SIG_SETMASK, oset, NULL)`. `false` is the C's -1.
#[must_use]
fn sigmask_set(oset: &SigSet) -> bool {
    sigmask_set_default(oset)
}

/// Raise one handled signal on this thread.
///
/// POSIX defines `raise` as `pthread_kill(pthread_self(), signo)`, so the
/// signal is delivered to the calling thread.
pub fn raise(signal: Signal) -> Result<(), SignalError> {
    if raise_default(signal.number()) {
        Ok(())
    } else {
        Err(SignalError::RaiseFailed(signal))
    }
}

fn install_handler_default(signo: i32, mask: &SigSet) -> Installed {
    let nsa = SigAction::ours(mask);
    let mut osa = SigAction::empty();
    // SAFETY: both structs are the platform's `struct sigaction` layout, live
    // for the call, and not aliased.
    let rv = unsafe { sys::sigaction(signo, &raw const nsa, &raw mut osa) };
    if rv == -1 {
        return Installed::Failed;
    }
    if osa.is_ours() {
        return Installed::AlreadyOurs;
    }
    Installed::Displaced(osa)
}

fn restore_handler_default(signo: i32, osa: SigAction) -> bool {
    // SAFETY: `osa` is a disposition this module read out of `sigaction`,
    // unmodified since, and lives for the call.
    unsafe { sys::sigaction(signo, &raw const osa, core::ptr::null_mut()) == 0 }
}

fn sigmask_block_default(set: &SigSet) -> Option<SigSet> {
    let mut oset = SigSet::empty();
    // SAFETY: both are the platform's `sigset_t` layout and live for the
    // call.
    let rv = unsafe { sys::sigprocmask(SIG_BLOCK, &raw const *set, &raw mut oset) };
    (rv == 0).then_some(oset)
}

fn sigmask_set_default(oset: &SigSet) -> bool {
    // SAFETY: as above; the out-parameter is NULL, which POSIX allows.
    unsafe { sys::sigprocmask(SIG_SETMASK, &raw const *oset, core::ptr::null_mut()) == 0 }
}

fn raise_default(signo: i32) -> bool {
    // SAFETY: no arguments to get wrong.
    unsafe { sys::raise(signo) == 0 }
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    use core::mem::offset_of;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    use crate::cheader;

    /// The `SIGWINCH` disposition and the trampoline's slot are
    /// process-global, and `cargo test` runs these on parallel threads. Every
    /// test that arms or disarms takes this first, so that one test's restore
    /// cannot land between another's install and raise.
    static ARMED: Mutex<()> = Mutex::new(());

    /// Exclusive use of one signal's disposition, put back on the way out.
    ///
    /// `SIGWINCH` throughout, because its default disposition is *ignore*: a
    /// failure that leaves no handler installed cannot kill the test runner.
    /// Restoring from `Drop` rather than from the end of each test is what
    /// stops a single failure from leaving our trampoline installed and
    /// turning every later arming test red for a reason of its own.
    struct Disposition {
        signo: i32,
        saved: SigAction,
        _armed: MutexGuard<'static, ()>,
    }

    impl Disposition {
        fn take(signo: i32) -> Self {
            let armed = ARMED.lock().unwrap_or_else(PoisonError::into_inner);
            Self {
                signo,
                saved: current(signo),
                _armed: armed,
            }
        }
    }

    impl Drop for Disposition {
        fn drop(&mut self) {
            PENDING_SLOT.force_inactive();
            // Not asserted: a failure here during an unwind would abort the
            // runner and hide the failure that caused it.
            install(self.signo, &self.saved);
        }
    }

    /// Ours are the numbers glibc's own headers give, not the ones POSIX
    /// declines to fix. `SIGCONT` and `SIGTSTP` are the pair that actually
    /// differ across the platforms in scope, and getting either wrong would
    /// trap a signal the editor was never asked to handle.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn the_signal_numbers_are_the_ones_the_headers_define() {
        let h = cheader::defines(&["bits/signum-generic.h", "bits/signum-arch.h"]);
        for (name, ours) in [
            ("SIGHUP", signo::SIGHUP),
            ("SIGINT", signo::SIGINT),
            ("SIGQUIT", signo::SIGQUIT),
            ("SIGTERM", signo::SIGTERM),
            ("SIGWINCH", signo::SIGWINCH),
            ("SIGCONT", signo::SIGCONT),
            ("SIGTSTP", signo::SIGTSTP),
        ] {
            assert_eq!(h[name], i64::from(ours), "{name}");
        }
    }

    /// The three `sigaction`/`sigprocmask` words this module transcribes,
    /// against `bits/sigaction.h`.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn the_sigaction_words_are_the_ones_the_header_defines() {
        let h = sigaction_defines();
        assert_eq!(h["SA_ONSTACK"], i64::from(SA_ONSTACK));
        assert_eq!(h["SIG_BLOCK"], i64::from(SIG_BLOCK));
        assert_eq!(h["SIG_SETMASK"], i64::from(SIG_SETMASK));
    }

    /// The field offsets of `struct sigaction`, which no header states and
    /// only a compiler can answer.
    ///
    /// Produced by gcc 15 on x86_64 glibc and carried here rather than probed,
    /// because a `build.rs` that compiles a program to find out what the
    /// platform looks like is exactly what `plan/decisions/no-c-ffi.md`
    /// forbids:
    ///
    /// ```c
    /// printf("%zu %zu %zu %zu %zu\n", sizeof(struct sigaction),
    ///        offsetof(struct sigaction, sa_handler),
    ///        offsetof(struct sigaction, sa_mask),
    ///        offsetof(struct sigaction, sa_flags),
    ///        offsetof(struct sigaction, sa_restorer));
    /// // 152 0 8 136 144
    /// ```
    ///
    /// The size assertion beside the type cannot catch a reorder — `sa_flags`
    /// and `sa_restorer` are both 8-aligned words and swapping them keeps the
    /// size — and a reorder is the mistake that makes the libc install a
    /// handler address it read out of `sa_mask`.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn the_struct_sigaction_layout_is_the_one_gcc_lays_out() {
        assert_eq!(size_of::<SigAction>(), 152);
        assert_eq!(size_of::<SigSet>(), 128);
        assert_eq!(offset_of!(SigAction, handler), 0);
        assert_eq!(offset_of!(SigAction, mask), 8);
        assert_eq!(offset_of!(SigAction, flags), 136);
        assert_eq!(offset_of!(SigAction, restorer), 144);
    }

    #[test]
    fn a_sigset_sets_the_bit_the_kernel_reads() {
        let set = SigSet::of(&[signo::SIGHUP, signo::SIGWINCH]);
        // Bit `signo - 1`, little-endian within the first word for every
        // signal Linux numbers below 65.
        assert_eq!(set.val[0], (1 << 0) | (1 << 27));

        // The Linux word boundary, where the `signo - 1` arithmetic would
        // show. Darwin's complete signal set is the one 32-bit word above.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            assert_eq!(SigSet::of(&[64]).val[0], 1 << 63);
            assert_eq!(SigSet::of(&[65]).val[1], 1);
        }

        // Out of range is dropped rather than corrupting a neighbouring word.
        let mut wild = SigSet::empty();
        wild.add(0);
        wild.add(-1);
        wild.add(1 << 20);
        assert!(wild.val.iter().all(|&w| w == 0));
    }

    /// The bits this module sets are the bits the kernel reads back — every
    /// one of them, not merely a mask that round-trips through itself.
    ///
    /// The thread's own mask is restored before returning; `cargo test` gives
    /// each test its own thread, and a mask is per-thread.
    #[test]
    fn the_kernel_reads_back_exactly_the_bits_the_sigset_holds() {
        let saved = sigmask_block(&SigSet::empty()).expect("read the current mask");

        #[cfg(any(target_os = "linux", target_os = "android"))]
        let want = SigSet::of(&[signo::SIGHUP, signo::SIGWINCH, 64]);
        #[cfg(target_os = "macos")]
        let want = SigSet::of(&[signo::SIGHUP, signo::SIGWINCH]);
        assert!(sigmask_set(&want), "SIG_SETMASK");
        let now = sigmask_block(&SigSet::empty()).expect("read it back");
        assert_eq!(now.val, want.val);

        assert!(sigmask_set(&saved), "SIG_SETMASK");
        assert_eq!(
            sigmask_block(&SigSet::empty()).expect("read it back").val,
            saved.val
        );
    }

    /// `sigmask_block` is `SIG_BLOCK`, not `SIG_SETMASK`: it adds to the
    /// thread's mask and reports what was there before, which is what makes
    /// `sig_handler`'s save-and-restore pair work.
    #[test]
    fn blocking_adds_to_the_mask_and_reports_the_old_one() {
        let saved = sigmask_block(&SigSet::empty()).expect("read the current mask");

        assert!(sigmask_set(&SigSet::of(&[signo::SIGHUP])));
        let before = sigmask_block(&SigSet::of(&[signo::SIGWINCH])).expect("SIG_BLOCK");
        assert_eq!(before.val, SigSet::of(&[signo::SIGHUP]).val);
        let after = sigmask_block(&SigSet::empty()).expect("read it back");
        assert_eq!(
            after.val,
            SigSet::of(&[signo::SIGHUP, signo::SIGWINCH]).val,
            "SIG_BLOCK replaced the mask instead of adding to it"
        );

        assert!(sigmask_set(&saved));
    }

    #[test]
    fn scoped_mask_delays_and_restores() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let saved_mask = sigmask_block(&SigSet::empty()).expect("read the current mask");
        assert!(sigmask_set(&SigSet::empty()));

        let theirs = action(
            application_handler as *const () as usize,
            SigSet::empty(),
            0,
        );
        assert!(install(signo::SIGWINCH, &theirs));
        APPLICATION_RAN.store(0, Ordering::Relaxed);

        let blocked = BlockedSignals::block(&[Signal::Resize]).unwrap();
        assert!(raise(Signal::Resize).is_ok());
        assert_eq!(APPLICATION_RAN.load(Ordering::Relaxed), 0);

        blocked.restore().unwrap();
        assert_eq!(
            APPLICATION_RAN.load(Ordering::Relaxed),
            signo::SIGWINCH,
            "the pending signal was not delivered when the guard restored the mask"
        );
        assert_eq!(
            sigmask_block(&SigSet::empty())
                .expect("read the restored mask")
                .val,
            SigSet::empty().val
        );
        assert!(sigmask_set(&saved_mask));
    }

    /// The one test that can catch a wrong `struct sigaction` transcription,
    /// which the size assertion above cannot: a field in the wrong place
    /// makes the libc install a handler address it read out of `sa_mask`.
    #[test]
    fn a_handler_installs_records_and_restores() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let handlers = SignalHandlers::with_signals(&[Signal::Resize]).unwrap();
        assert!(current(signo::SIGWINCH).is_ours());

        // `raise` is `pthread_kill(pthread_self(), …)`, so this lands on this
        // thread and the store is visible by the time it returns.
        assert!(raise(Signal::Resize).is_ok());
        assert_eq!(handlers.take_pending(), Some(Signal::Resize));

        // The idempotence guard `sem:sig.sig-set-fn` step 4 depends on.
        assert!(matches!(
            install_handler(signo::SIGWINCH, &SigSet::of(&[signo::SIGWINCH])),
            Installed::AlreadyOurs
        ));
        assert!(matches!(
            SignalHandlers::with_signals(&[Signal::Resize]),
            Err(SignalError::AlreadyActive)
        ));

        handlers.restore().unwrap();
        assert!(!current(signo::SIGWINCH).is_ours());
    }

    /// What `sem:sig.sig-clr-fn` promises the application: the disposition
    /// libedit displaced comes back *intact*, and comes back working.
    ///
    /// Each field is checked as far as the libc round-trips it, because
    /// nothing in this crate ever reads one — a `SigAction` is only ever
    /// stored whole and put back whole, so a field read into the wrong place
    /// would be invisible until an application that had its own `SIGWINCH`
    /// handler stopped receiving them.
    ///
    /// **`sa_mask` is only meaningful in its first eight bytes.** glibc's
    /// `sigaction` talks to the kernel through a `struct kernel_sigaction`
    /// whose mask is one 64-bit word, and on the way back out it copies the
    /// caller's full 128-byte `sigset_t` worth out of it — so the other 120
    /// bytes of a displaced disposition are whatever was on glibc's stack,
    /// even when the caller zeroed the struct first. Reproduced in plain C
    /// against glibc 2.41 with glibc's own `struct sigaction`, so it is a
    /// property of the libc and not of the transcription here.
    ///
    /// It is harmless in this crate only because a saved disposition is never
    /// read, compared or hashed — it goes straight back to `sigaction`, which
    /// copies the same eight bytes back in. Anything that starts inspecting
    /// one is reading uninitialised memory.
    #[test]
    fn an_applications_own_disposition_survives_being_displaced() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let sa_restart = sa_restart();

        let theirs = action(
            application_handler as *const () as usize,
            SigSet::of(&[signo::SIGINT, signo::SIGQUIT]),
            sa_restart,
        );
        assert!(install(signo::SIGWINCH, &theirs));

        let Installed::Displaced(osa) = install_handler(signo::SIGWINCH, &SigSet::empty()) else {
            panic!("an application's handler is not ours and must be reported as displaced");
        };
        assert_eq!(osa.handler, theirs.handler, "sa_handler");
        assert_eq!(osa.mask.val[0], theirs.mask.val[0], "sa_mask");
        assert_eq!(osa.flags & sa_restart, sa_restart, "sa_flags");

        APPLICATION_RAN.store(0, Ordering::Relaxed);
        assert!(restore_handler(signo::SIGWINCH, osa));
        assert!(raise(Signal::Resize).is_ok());
        assert_eq!(
            APPLICATION_RAN.load(Ordering::Relaxed),
            signo::SIGWINCH,
            "the application's handler was not the one put back"
        );
    }

    /// The two departures from the C that `sem:sig.sig-set-fn` requires,
    /// checked against what the kernel actually stored rather than against the
    /// struct we handed it.
    ///
    /// `SA_RESTART` unset is load-bearing: it is how an interrupted `read`
    /// comes back `EINTR`, which is the only way the read loop learns a signal
    /// arrived. A full `sa_mask` is the other: it stops a second trapped
    /// signal nesting inside the first and re-entering the editor.
    #[test]
    fn the_installed_action_neither_restarts_nor_nests() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let sa_restart = sa_restart();

        let mask = SigSet::of(&[signo::SIGINT, signo::SIGWINCH, signo::SIGCONT]);
        let Installed::Displaced(osa) = install_handler(signo::SIGWINCH, &mask) else {
            panic!("the first install must displace the default disposition");
        };

        let installed = current(signo::SIGWINCH);
        assert_eq!(installed.flags & sa_restart, 0, "SA_RESTART was set");
        assert_eq!(
            installed.flags & SA_ONSTACK,
            SA_ONSTACK,
            "SA_ONSTACK was not set"
        );
        // Only the kernel's own word of `sa_mask` survives the trip back out
        // of glibc; see the test above for why.
        assert_eq!(
            installed.mask.val[0], mask.val[0],
            "sa_mask was not every trap"
        );
        assert!(installed.is_ours());

        assert!(restore_handler(signo::SIGWINCH, osa));
    }

    /// The handler runs on whatever thread the kernel picks, so the slot
    /// `sig_set` publishes has to be visible from one that never touched it.
    ///
    /// Spawning is itself a synchronisation point, so this cannot catch a
    /// weakened ordering on its own; what it does pin is that the trampoline
    /// records for a thread other than the one that armed, which is the
    /// arrangement the atomic slot exists to make sound.
    #[test]
    fn the_trampoline_records_from_a_thread_that_never_armed() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let handlers = SignalHandlers::with_signals(&[Signal::Resize]).unwrap();

        std::thread::spawn(|| assert!(raise(Signal::Resize).is_ok()))
            .join()
            .expect("the raising thread");
        assert_eq!(handlers.take_pending(), Some(Signal::Resize));
    }

    /// A handler can be preempted after reading process-global state and
    /// resume after its owner has gone away. The activation token makes that
    /// delayed write fail instead of attributing the old delivery to a later
    /// editor scope.
    #[test]
    fn delayed_record_stays_in_its_activation() {
        let slot = SignalSlot::new();
        let first = slot.activate().expect("first activation");
        let delayed_observation = slot.state.load(Ordering::Relaxed);

        assert!(slot.deactivate(first));
        let second = slot.activate().expect("second activation");
        slot.record_observed(delayed_observation, signo::SIGWINCH);

        assert_eq!(slot.take(second), None);
        slot.record(signo::SIGWINCH);
        assert_eq!(slot.take(second), Some(signo::SIGWINCH));
        assert!(slot.deactivate(second));
    }

    /// `sig_clr` drops the registration, which the C never does — after its
    /// `el_end` the file-static `sel` dangles for the rest of the process
    /// (ERR-terminal-18), and `sem:sig.sig-end-fn` requires the port not to
    /// reproduce that.
    ///
    /// Propagation consumes one saved disposition, rearming captures it
    /// again, and dropping without propagating a later delivery still puts
    /// the application policy back.
    #[test]
    fn propagation_rearm_and_drop_restore_policy() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let application = action(
            application_handler as *const () as usize,
            SigSet::empty(),
            0,
        );
        assert!(install(signo::SIGWINCH, &application));
        APPLICATION_RAN.store(0, Ordering::Relaxed);

        let mut handlers = SignalHandlers::with_signals(&[Signal::Resize]).unwrap();
        assert!(raise(Signal::Resize).is_ok());
        assert_eq!(handlers.take_pending(), Some(Signal::Resize));
        assert_eq!(APPLICATION_RAN.load(Ordering::Relaxed), 0);

        handlers.propagate(Signal::Resize).unwrap();
        assert_eq!(APPLICATION_RAN.load(Ordering::Relaxed), signo::SIGWINCH);
        handlers.rearm().unwrap();

        APPLICATION_RAN.store(0, Ordering::Relaxed);
        assert!(raise(Signal::Resize).is_ok());
        assert_eq!(handlers.take_pending(), Some(Signal::Resize));
        drop(handlers);

        assert!(raise(Signal::Resize).is_ok());
        assert_eq!(APPLICATION_RAN.load(Ordering::Relaxed), signo::SIGWINCH);
    }

    #[test]
    fn empty_scope_claims_nothing() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let handlers = SignalHandlers::with_signals(&[]).unwrap();

        assert!(!current(signo::SIGWINCH).is_ours());
        handlers.restore().unwrap();
        assert!(!current(signo::SIGWINCH).is_ours());
    }

    #[test]
    fn subset_rejects_unarmed_signal() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let mut handlers = SignalHandlers::with_signals(&[Signal::Resize]).unwrap();

        assert_eq!(
            handlers.propagate(Signal::Interrupt),
            Err(SignalError::NotArmed(Signal::Interrupt))
        );
        handlers.restore().unwrap();
    }

    #[test]
    fn pending_slot_keeps_latest_delivery() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        let handlers = SignalHandlers::with_signals(&[Signal::Resize, Signal::Continue]).unwrap();

        assert!(raise(Signal::Resize).is_ok());
        assert!(raise(Signal::Continue).is_ok());
        assert_eq!(handlers.take_pending(), Some(Signal::Continue));
        assert_eq!(handlers.take_pending(), None);
        handlers.restore().unwrap();
    }

    /// Stands in for an application that had its own `SIGWINCH` handler before
    /// libedit was ever initialised.
    static APPLICATION_RAN: AtomicI32 = AtomicI32::new(0);

    extern "C" fn application_handler(signo: c_int) {
        APPLICATION_RAN.store(signo, Ordering::Relaxed);
    }

    /// `sigaction(signo, NULL, &osa)` — what is installed right now.
    fn current(signo: i32) -> SigAction {
        let mut osa = SigAction::empty();
        // SAFETY: the out-parameter is live and correctly shaped; a NULL `act`
        // is POSIX's "report only".
        let rv = unsafe { sys::sigaction(signo, core::ptr::null(), &raw mut osa) };
        assert_eq!(rv, 0, "sigaction({signo}, NULL, &osa)");
        osa
    }

    /// `sigaction(signo, &sa, NULL)` — install without asking what was there.
    fn install(signo: i32, sa: &SigAction) -> bool {
        // SAFETY: `sa` is the platform's `struct sigaction` layout and lives
        // for the call.
        unsafe { sys::sigaction(signo, &raw const *sa, core::ptr::null_mut()) == 0 }
    }

    fn action(handler: usize, mask: SigSet, flags: c_int) -> SigAction {
        let mut action = SigAction::empty();
        action.handler = handler;
        action.mask = mask;
        action.flags = flags;
        action
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn sa_restart() -> c_int {
        c_int::try_from(sigaction_defines()["SA_RESTART"]).expect("SA_RESTART")
    }

    #[cfg(target_os = "macos")]
    const fn sa_restart() -> c_int {
        // `<sys/signal.h>`.
        0x0002
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn sigaction_defines() -> std::collections::HashMap<String, i64> {
        cheader::defines(&["bits/sigaction.h"])
    }
}
