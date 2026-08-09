//! The signal family, and the first of this workspace's two libc `extern`
//! blocks.
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
//! operations on the transcribed [`SigSet`] and are not linked.
//!
//! # The installed handler
//!
//! [`sig_trampoline`] is what `sigaction` installs, and it is the whole of
//! the async-signal-safe work: two atomic operations, no allocation, no lock,
//! no buffered write. It records the signal number into the slot
//! [`set_signal_slot`] registered — which is the port's counterpart of
//! libedit's file-static `sel`, and is `el_signal->sig_no` for the `EditLine`
//! that last armed. `crate::sig::sig_handler` in the core is the *body*, run
//! from ordinary context by the read loop when it notices; see the note
//! there for what that buys and what it costs.

use core::ffi::{c_int, c_ulong, c_void};
use core::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

#[cfg(any(
    target_arch = "mips",
    target_arch = "mips32r6",
    target_arch = "mips64",
    target_arch = "mips64r6",
    target_arch = "sparc",
    target_arch = "sparc64",
))]
compile_error!(
    "the struct sigaction layout and the signal numbers transcribed here are \
     the Linux generic ABI; mips and sparc renumber and reorder both"
);

/// The signal numbers this port names.
///
/// POSIX names signals; it does not fix their numbers, and the libc that
/// would define them is not what supplies constants here. Five of the seven
/// libedit traps agree everywhere in scope; only `SIGCONT` and `SIGTSTP`
/// differ. Linux's alpha, mips, sparc and parisc ports renumber more widely
/// and are not covered — see the `compile_error!` above.
pub mod signo {
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

/// How many `unsigned long` words a `sigset_t` holds: 1024 bits, which is
/// what glibc and musl both lay out.
const SIGSET_WORDS: usize = 1024 / SIGSET_BITS;

/// The width of one `sigset_t` word, in bits.
const SIGSET_BITS: usize = c_ulong::BITS as usize;

/// `SIG_BLOCK`.
const SIG_BLOCK: c_int = 0;
/// `SIG_SETMASK`.
const SIG_SETMASK: c_int = 2;

/// `SA_ONSTACK` — run on the alternate signal stack if the application
/// installed one.
const SA_ONSTACK: c_int = 0x0800_0000;

/// POSIX `sigset_t`, transcribed.
///
/// `sigemptyset`, `sigaddset` and `sigismember` are bit arithmetic over this
/// and are implemented below rather than linked: `plan/decisions/no-c-ffi.md`
/// rations the exception, and these need no ration.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SigSet {
    val: [c_ulong; SIGSET_WORDS],
}

impl SigSet {
    /// `sigemptyset(&set)`.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            val: [0; SIGSET_WORDS],
        }
    }

    /// `sigaddset(&set, signo)`. Out-of-range numbers are dropped, where the
    /// C would answer `EINVAL` and libedit would ignore it.
    pub const fn add(&mut self, signo: i32) {
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
    pub fn of(signos: &[i32]) -> Self {
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

/// POSIX `struct sigaction`, transcribed for the Linux generic ABI.
///
/// libedit only ever stores one of these wholesale and puts it back, so
/// nothing here reads the fields; they exist so that the displaced
/// disposition survives a round trip *intact*, which is what
/// `sem:sig.sig-clr-fn` promises the application.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SigAction {
    /// `sa_handler`/`sa_sigaction`, which are a union.
    handler: usize,
    mask: SigSet,
    flags: c_int,
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
            restorer: core::ptr::null(),
        }
    }

    /// Whether this disposition is already ours.
    fn is_ours(&self) -> bool {
        self.handler == sig_trampoline as *const () as usize
    }
}

/// The transcription is checked against the platform's own headers rather
/// than trusted: on x86_64 and aarch64 glibc, `sizeof(struct sigaction)` is
/// 152 and `sizeof(sigset_t)` is 128, and the field order is handler, mask,
/// flags, restorer.
const _: () = {
    assert!(size_of::<SigSet>() == 128);
    assert!(size_of::<SigAction>() == 8 + 128 + 8 + 8);
    assert!(align_of::<SigAction>() == align_of::<usize>());
};

/// What POSIX `sigaction(signo, &nsa, &osa)` says about the disposition it
/// displaced.
///
/// The C spells this as `sigaction(...) != -1 && osa.sa_handler !=
/// sig_handler`; only this layer can compare handler identity, so the whole
/// test is answered here.
pub enum Installed {
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

/// Where [`sig_trampoline`] records the signal it saw.
///
/// The port's counterpart of libedit's file-static `sel`: process-global,
/// because a C signal handler carries no user data and there is nowhere else
/// to put it. Null means nothing is armed.
static PENDING_SLOT: AtomicPtr<AtomicI32> = AtomicPtr::new(core::ptr::null_mut());

/// The disposition `sigaction` installs. Async-signal-safe by construction:
/// one atomic load, one atomic store, nothing else.
extern "C" fn sig_trampoline(signo: c_int) {
    let slot = PENDING_SLOT.load(Ordering::Acquire);
    if !slot.is_null() {
        // SAFETY: `set_signal_slot`'s contract is that the pointer stays
        // valid until `clear_signal_slot`, and the core clears it in
        // `sig_clr` — which runs before the state it points into is dropped,
        // and before any disposition of ours is left installed. Reading a
        // shared `AtomicI32` through it is the only use.
        unsafe { (*slot).store(signo, Ordering::Relaxed) };
    }
}

/// Register the atomic [`sig_trampoline`] records into.
///
/// This is `sem:sig.sig-set-fn`'s step 2, the C's `sel = el`, and it must be
/// issued *inside* the blocked window the rule describes rather than before
/// it, so that a delivery cannot land while the slot and the dispositions
/// disagree.
///
/// # Safety
///
/// `slot` must point at a live `AtomicI32` that neither moves nor is dropped
/// until [`clear_signal_slot`] is called. The core satisfies this by pointing
/// it at the boxed `ElSignal::sig_no` and clearing it in `sig_clr`, which
/// `sig_end` runs before the box is dropped.
pub unsafe fn set_signal_slot(slot: *const AtomicI32) {
    PENDING_SLOT.store(slot.cast_mut(), Ordering::Release);
}

/// Drop the registration [`set_signal_slot`] made.
///
/// The C never does this, which is why after `el_end` its `sel` dangles for
/// the rest of the process (ERR-terminal-18). `sem:sig.sig-end-fn` requires
/// the port not to reproduce that.
pub fn clear_signal_slot() {
    PENDING_SLOT.store(core::ptr::null_mut(), Ordering::Release);
}

// ---------------------------------------------------------------------------
// The override slot
// ---------------------------------------------------------------------------

/// The signal family, as one replaceable table.
///
/// Nothing installs one and nothing has to: the default is this module's own
/// implementation and `plan/decisions/platform-layer.md` makes that the
/// specified behaviour. The slot is here for an embedder that must route
/// signal arming through its own bookkeeping — a shell with a job-control
/// table, say — without forking the crate.
pub struct SignalOps {
    pub install_handler: fn(i32, &SigSet) -> Installed,
    pub restore_handler: fn(i32, SigAction) -> bool,
    pub sigmask_block: fn(&SigSet) -> Option<SigSet>,
    pub sigmask_set: fn(&SigSet) -> bool,
    pub raise: fn(i32) -> bool,
}

const BUILTIN_OPS: SignalOps = SignalOps {
    install_handler: install_handler_default,
    restore_handler: restore_handler_default,
    sigmask_block: sigmask_block_default,
    sigmask_set: sigmask_set_default,
    raise: raise_default,
};

static OPS: AtomicPtr<SignalOps> = AtomicPtr::new(core::ptr::null_mut());

/// Install an override for the whole family. Idempotent, and there is no way
/// to take one back off — the slot is a one-way seam, not a stack.
pub fn set_signal_ops(ops: &'static SignalOps) {
    OPS.store(core::ptr::from_ref(ops).cast_mut(), Ordering::Release);
}

fn ops() -> &'static SignalOps {
    let p = OPS.load(Ordering::Acquire);
    if p.is_null() {
        return &BUILTIN_OPS;
    }
    // SAFETY: `set_signal_ops` is the only writer and takes a `&'static`.
    unsafe { &*p }
}

// ---------------------------------------------------------------------------
// The family
// ---------------------------------------------------------------------------

/// `sigaction(signo, &nsa, &osa)` with `nsa` built by [`SigAction::ours`].
#[must_use]
pub fn install_handler(signo: i32, mask: &SigSet) -> Installed {
    (ops().install_handler)(signo, mask)
}

/// `sigaction(signo, osa, NULL)`, putting a saved disposition back. Takes the
/// disposition by value because the callers consume the slot in the same
/// breath. `false` is the C's -1.
#[must_use]
pub fn restore_handler(signo: i32, osa: SigAction) -> bool {
    (ops().restore_handler)(signo, osa)
}

/// `sigprocmask(SIG_BLOCK, set, &oset)`. `None` is the C's -1.
#[must_use]
pub fn sigmask_block(set: &SigSet) -> Option<SigSet> {
    (ops().sigmask_block)(set)
}

/// `sigemptyset(&nset)`, `sigaddset(&nset, signo)`, `sigprocmask(SIG_BLOCK,
/// &nset, &oset)` — one signal, which is both `sig_handler`'s step 2 and
/// `terminal_set`'s `SIGWINCH` block.
#[must_use]
pub fn sigmask_block_one(signo: i32) -> Option<SigSet> {
    sigmask_block(&SigSet::of(&[signo]))
}

/// `sigprocmask(SIG_SETMASK, oset, NULL)`. `false` is the C's -1.
#[must_use]
pub fn sigmask_set(oset: &SigSet) -> bool {
    (ops().sigmask_set)(oset)
}

/// `raise(signo)` — POSIX defines it as `pthread_kill(pthread_self(),
/// signo)`, so the re-raised signal is delivered to this same thread.
/// `false` is the C's non-zero return.
#[must_use]
pub fn raise(signo: i32) -> bool {
    (ops().raise)(signo)
}

fn install_handler_default(signo: i32, mask: &SigSet) -> Installed {
    let nsa = SigAction::ours(mask);
    let mut osa = SigAction {
        handler: 0,
        mask: SigSet::empty(),
        flags: 0,
        restorer: core::ptr::null(),
    };
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
    use core::mem::offset_of;
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use super::*;
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
            clear_signal_slot();
            // Not asserted: a failure here during an unwind would abort the
            // runner and hide the failure that caused it.
            install(self.signo, &self.saved);
        }
    }

    /// Ours are the numbers glibc's own headers give, not the ones POSIX
    /// declines to fix. `SIGCONT` and `SIGTSTP` are the pair that actually
    /// differ across the platforms in scope, and getting either wrong would
    /// trap a signal the editor was never asked to handle.
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

        // The word boundary, where the `signo - 1` arithmetic would show. 64
        // is `SIGRTMAX`, the highest signal Linux has, and it belongs in the
        // top bit of the first word; 65 is the first that does not exist.
        assert_eq!(SigSet::of(&[64]).val[0], 1 << 63);
        assert_eq!(SigSet::of(&[65]).val[1], 1);

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

        let want = SigSet::of(&[signo::SIGHUP, signo::SIGWINCH, 64]);
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

    /// The one test that can catch a wrong `struct sigaction` transcription,
    /// which the size assertion above cannot: a field in the wrong place
    /// makes the libc install a handler address it read out of `sa_mask`.
    #[test]
    fn a_handler_installs_records_and_restores() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        static SLOT: AtomicI32 = AtomicI32::new(0);
        // SAFETY: `SLOT` is a static, so it outlives the registration below,
        // which is cleared before this test returns.
        unsafe { set_signal_slot(&raw const SLOT) };

        let mask = SigSet::of(&[signo::SIGWINCH]);
        let Installed::Displaced(osa) = install_handler(signo::SIGWINCH, &mask) else {
            panic!("the first install must displace the default disposition");
        };

        // `raise` is `pthread_kill(pthread_self(), …)`, so this lands on this
        // thread and the store is visible by the time it returns.
        assert!(raise(signo::SIGWINCH));
        assert_eq!(SLOT.load(Ordering::Relaxed), signo::SIGWINCH);

        // The idempotence guard `sem:sig.sig-set-fn` step 4 depends on.
        assert!(matches!(
            install_handler(signo::SIGWINCH, &mask),
            Installed::AlreadyOurs
        ));

        assert!(restore_handler(signo::SIGWINCH, osa));
        SLOT.store(0, Ordering::Relaxed);
        assert!(raise(signo::SIGWINCH));
        assert_eq!(
            SLOT.load(Ordering::Relaxed),
            0,
            "the displaced disposition was not put back"
        );
        clear_signal_slot();
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
        let h = sigaction_defines();
        let sa_restart = c_int::try_from(h["SA_RESTART"]).expect("SA_RESTART");

        let theirs = SigAction {
            handler: application_handler as *const () as usize,
            mask: SigSet::of(&[signo::SIGINT, signo::SIGQUIT]),
            flags: sa_restart,
            restorer: core::ptr::null(),
        };
        assert!(install(signo::SIGWINCH, &theirs));

        let Installed::Displaced(osa) = install_handler(signo::SIGWINCH, &SigSet::empty()) else {
            panic!("an application's handler is not ours and must be reported as displaced");
        };
        assert_eq!(osa.handler, theirs.handler, "sa_handler");
        assert_eq!(osa.mask.val[0], theirs.mask.val[0], "sa_mask");
        assert_eq!(osa.flags & sa_restart, sa_restart, "sa_flags");

        APPLICATION_RAN.store(0, Ordering::Relaxed);
        assert!(restore_handler(signo::SIGWINCH, osa));
        assert!(raise(signo::SIGWINCH));
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
        let h = sigaction_defines();
        let sa_restart = c_int::try_from(h["SA_RESTART"]).expect("SA_RESTART");

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
    /// This is what the `Release`/`Acquire` pair on `PENDING_SLOT` is for.
    /// Spawning is itself a synchronisation point, so this cannot catch a
    /// weakened ordering on its own; what it does pin is that the trampoline
    /// records for a thread other than the one that armed, which is the
    /// arrangement the ordering exists to make sound.
    #[test]
    fn the_trampoline_records_from_a_thread_that_never_armed() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        static SLOT: AtomicI32 = AtomicI32::new(0);
        // SAFETY: `SLOT` is a static and the registration is cleared before
        // this test returns.
        unsafe { set_signal_slot(&raw const SLOT) };
        let Installed::Displaced(osa) = install_handler(signo::SIGWINCH, &SigSet::empty()) else {
            panic!("the first install must displace the default disposition");
        };

        std::thread::spawn(|| assert!(raise(signo::SIGWINCH)))
            .join()
            .expect("the raising thread");
        assert_eq!(SLOT.load(Ordering::Relaxed), signo::SIGWINCH);

        clear_signal_slot();
        assert!(restore_handler(signo::SIGWINCH, osa));
    }

    /// `sig_clr` drops the registration, which the C never does — after its
    /// `el_end` the file-static `sel` dangles for the rest of the process
    /// (ERR-terminal-18), and `sem:sig.sig-end-fn` requires the port not to
    /// reproduce that.
    ///
    /// Also the re-pointing case: arming a second `EditLine` must send the
    /// record to the second one, since the slot is the port's counterpart of a
    /// single file-static.
    #[test]
    fn the_slot_can_be_repointed_and_dropped() {
        let _disposition = Disposition::take(signo::SIGWINCH);
        static FIRST: AtomicI32 = AtomicI32::new(0);
        static SECOND: AtomicI32 = AtomicI32::new(0);

        let Installed::Displaced(osa) = install_handler(signo::SIGWINCH, &SigSet::empty()) else {
            panic!("the first install must displace the default disposition");
        };

        // SAFETY: both are statics and the registration is cleared below.
        unsafe { set_signal_slot(&raw const FIRST) };
        assert!(raise(signo::SIGWINCH));
        assert_eq!(FIRST.load(Ordering::Relaxed), signo::SIGWINCH);

        // SAFETY: SECOND is also static and the registration is cleared below;
        // repointing deliberately replaces the still-live FIRST registration.
        unsafe { set_signal_slot(&raw const SECOND) };
        FIRST.store(0, Ordering::Relaxed);
        assert!(raise(signo::SIGWINCH));
        assert_eq!(SECOND.load(Ordering::Relaxed), signo::SIGWINCH);
        assert_eq!(
            FIRST.load(Ordering::Relaxed),
            0,
            "the first slot was still written"
        );

        // The handler stays installed here on purpose: a delivery arriving
        // after the slot is dropped must be swallowed, not followed into
        // freed storage.
        clear_signal_slot();
        SECOND.store(0, Ordering::Relaxed);
        assert!(raise(signo::SIGWINCH));
        assert_eq!(
            SECOND.load(Ordering::Relaxed),
            0,
            "a cleared slot was written"
        );

        assert!(restore_handler(signo::SIGWINCH, osa));
    }

    /// Stands in for an application that had its own `SIGWINCH` handler before
    /// libedit was ever initialised.
    static APPLICATION_RAN: AtomicI32 = AtomicI32::new(0);

    extern "C" fn application_handler(signo: c_int) {
        APPLICATION_RAN.store(signo, Ordering::Relaxed);
    }

    /// `sigaction(signo, NULL, &osa)` — what is installed right now.
    fn current(signo: i32) -> SigAction {
        let mut osa = SigAction {
            handler: 0,
            mask: SigSet::empty(),
            flags: 0,
            restorer: core::ptr::null(),
        };
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

    fn sigaction_defines() -> std::collections::HashMap<String, i64> {
        cheader::defines(&["bits/sigaction.h"])
    }
}
