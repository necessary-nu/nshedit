//! The libedit and readline C ABIs.
//!
//! Two surfaces, both of which consumers compile against today:
//!
//! - `histedit.h` — libedit's own API, the `el_*`, `history*` and `tok_*`
//!   entry points.
//! - `editline/readline.h` — the GNU readline compatibility layer.
//!
//! Nothing here links a C library. This crate *is* the C library: it exports
//! the symbols and builds as `libnshedit.so`, installed with `libedit.so.0`
//! and `libreadline.so.8` symlinked onto it. See
//! `plan/decisions/no-c-ffi.md`.
//!
//! Everything C-shaped lives here and nowhere else — the varargs dispatch,
//! the shared conversion buffers whose returned pointers stay valid exactly
//! until the next call, the exported mutable statics, the live view cast onto
//! the line struct. `nshedit` itself is ordinary Rust and carries none of it.
//! See `plan/decisions/idiomatic-core.md`.
//!
//! Behaviour across this boundary is frozen through translation and test,
//! defects included, then corrected during idiomatization — per
//! `plan/decisions/conformance-policy.md` and the register in
//! `docs/errata.md`. A function here that looks wrong is reproducing
//! something; check its `sem` rule before changing it.

// Exported C symbols keep their C names, which are not Rust's casing.
#![allow(non_upper_case_globals)]
// Every function in this crate is a C entry point and its safety contract is
// its `sem` rule, quoted per function in `docs/spec/port/`. Repeating a
// boilerplate `# Safety` section on all ~160 of them would say less than the
// rule already does.
#![allow(clippy::missing_safety_doc)]
// Both of these go away as the bodies land: until then every function is
// `todo!()`, so its parameters are unread, and the private helpers translated
// from `readline.c`'s `static` functions have no callers yet.
#![allow(dead_code, unused_variables)]

pub mod eln;
pub mod histedit;
pub mod readline;

/// The platform's `errno`, which only this crate may touch.
///
/// The `sem` rules promise C callers real `errno` values — `ENOSPC` and
/// `ENOMEM` out of `sem:vis.istrsenvisx-fn`, `EINVAL` out of
/// `sem:unvis.strnunvisx-fn`, `ERANGE` out of `sem:eln.el-getc-fn`, the
/// failing read's own value restored by `sem:read.el-wgets-fn` — and a caller
/// reads them the only way C offers, out of `errno` after the call returns.
/// `plan/decisions/no-c-ffi.md` names this the one thing the ABI boundary may
/// reach libc for, and nothing but the accessor below does.
///
/// No library is linked for it. A `cdylib` already links the platform libc
/// through `std`, exactly as `libedit.so` does, so this is a declaration of a
/// symbol that is present either way — not a dependency the port acquires.
///
/// The core cannot do this itself: it records into [`nshedit::errno`] instead,
/// and this module is what turns that record into the C's `errno`. Both are
/// per-thread, and [`Mark`] is neither `Send` nor `Sync` so the copy between
/// them cannot be split across two threads.
mod errno {
    use core::ffi::c_int;
    use core::marker::PhantomData;

    // glibc, musl and Bionic all publish the thread-local `errno` through
    // `__errno_location`.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    unsafe extern "C" {
        #[link_name = "__errno_location"]
        safe fn errno_location() -> *mut c_int;
    }

    // Darwin and the FreeBSD line spell the same accessor `__error`. The port
    // targets Linux (`plan/decisions/posix-only-scope.md`), so this arm is
    // what keeps the crate building elsewhere rather than a claim of support;
    // NetBSD and OpenBSD spell it `__errno` and would need an arm of their own.
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    unsafe extern "C" {
        #[link_name = "__error"]
        safe fn errno_location() -> *mut c_int;
    }

    /// A sample of what the core has recorded, taken before a call and spent
    /// on it afterwards.
    ///
    /// Not `Send` and not `Sync`: `errno` is per-thread on both sides, so a
    /// sample is only meaningful to the thread that took it.
    #[derive(Debug)]
    pub(crate) struct Mark(u64, PhantomData<*const ()>);

    /// Sample the core's `errno` before calling into it.
    #[must_use]
    pub(crate) fn mark() -> Mark {
        Mark(nshedit::errno::writes(), PhantomData)
    }

    /// Copy what the core recorded since `mark` into the C's `errno`.
    ///
    /// Nothing is written when the call recorded nothing, so a successful call
    /// leaves the caller's `errno` exactly as it found it and a value left
    /// over from an earlier failure is never republished — which is the C's
    /// discipline: written on failure, never cleared on success.
    pub(crate) fn publish(mark: Mark) {
        if nshedit::errno::writes() != mark.0 {
            set(nshedit::errno::errno());
        }
    }

    /// Write `e` to both homes, for the two errno values this crate produces
    /// itself: `sem:eln.el-getc-fn`'s `ERANGE`, and the `errno = 0` that
    /// `sem:readline.read-history-fn` clears with. The core's copy is kept in
    /// step because the C has one `errno` and code on both sides of this
    /// boundary reads it.
    pub(crate) fn set(e: c_int) {
        nshedit::errno::set_errno(e);
        // SAFETY: the accessor answers with this thread's `errno` slot, which
        // is valid for as long as the thread is.
        unsafe { *errno_location() = e };
    }

    /// What a C caller would find in `errno` right now.
    ///
    /// This is the value the entry points whose rule *returns* an errno —
    /// `sem:readline.read-history-fn` and its two neighbours — hand back. It
    /// reads the platform's copy rather than the core's because the failing
    /// call may equally have been a `std` one, which sets the platform's and
    /// not the core's; [`publish`] is what makes the two agree first.
    pub(crate) fn get() -> c_int {
        // SAFETY: as `set`.
        unsafe { *errno_location() }
    }

    #[cfg(test)]
    mod tests {
        use super::{get, mark, publish, set};

        /// The accessor is the same `errno` the platform's own calls write,
        /// which is the whole claim this module rests on: a failing `std` call
        /// and a C caller reading `errno` see one value.
        #[test]
        fn the_accessor_is_the_platforms_errno() {
            set(0);
            let e = std::fs::File::open("/nonexistent/nshedit/errno/probe").unwrap_err();
            assert_eq!(get(), e.raw_os_error().unwrap());
        }

        /// A call that recorded nothing leaves the caller's `errno` alone; one
        /// that recorded hands the value over.
        #[test]
        fn publishing_follows_what_the_core_recorded() {
            set(7);
            publish(mark());
            assert_eq!(get(), 7);

            let m = mark();
            nshedit::errno::set_errno(nshedit::errno::EINVAL);
            publish(m);
            assert_eq!(get(), nshedit::errno::EINVAL);
        }

        /// End to end, on the promise `sem:vis.istrsenvisx-fn` makes: a
        /// destination too small for the encoding fails with `ENOSPC` in the
        /// C's `errno`. `"a b"` needs four bytes with its terminator.
        #[test]
        fn a_failing_vis_call_reaches_the_c_errno() {
            set(0);
            let mut dst = [0i8; 8];
            let m = mark();
            let n = nshedit::vis::strnvis(dst.as_mut_ptr(), 2, c"a b".as_ptr(), 0);
            publish(m);
            assert_eq!(n, -1);
            assert_eq!(get(), nshedit::errno::ENOSPC);
        }
    }
}
