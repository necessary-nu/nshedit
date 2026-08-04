//! The C's `errno`, parked where a C caller can eventually be handed it.
//!
//! This module has no C counterpart. `plan/decisions/no-c-ffi.md` bars linking
//! libc, so the port cannot write the real thread-local `errno` that the `sem`
//! rules promise: `ENOSPC` for an undersized destination in
//! `sem:vis.istrsenvisx-fn` and `sem:unvis.strnunvisx-fn`, `ENOMEM` for the
//! overflow guard, `EINVAL` for a decoder handed an impossible state. The
//! engine sets this thread-local wherever the C sets `errno`, so the value is
//! recorded rather than lost.
//!
//! **Shape.** [`errno`] is `pub` and the setter is not: publishing the value to
//! a C caller is the ABI crate's job, and it can do it either by copying this
//! into the platform `errno` after a failing call or by exporting a reader.
//! Nothing here presumes which; that side is not built yet. The numbers are
//! Linux's, which is the whole of `plan/decisions/posix-only-scope.md`'s
//! target.
//!
//! **Semantics.** Matching the C, the value is written only on failure paths
//! and never cleared on success, so it is meaningful only immediately after a
//! call that reported an error.

use std::cell::Cell;

/// C: `EINVAL`.
pub(crate) const EINVAL: i32 = 22;

/// C: `ENOMEM`.
pub(crate) const ENOMEM: i32 = 12;

/// C: `ENOSPC`.
pub(crate) const ENOSPC: i32 = 28;

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
}

/// Reads back what the last failing call recorded.
pub fn errno() -> i32 {
    ERRNO.with(Cell::get)
}

/// Records what the C would have stored in `errno`.
pub(crate) fn set_errno(e: i32) {
    ERRNO.with(|slot| slot.set(e));
}
