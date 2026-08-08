//! The C's `errno`, in the one home the core is allowed to write.
//!
//! This module has no C counterpart. `plan/decisions/no-c-ffi.md` bars the
//! core from linking libc, so the real thread-local `errno` that the `sem`
//! rules promise — `ENOSPC` for an undersized destination in
//! `sem:vis.istrsenvisx-fn` and `sem:unvis.strnunvisx-fn`, `ENOMEM` for the
//! overflow guard, `EINVAL` for a decoder handed an impossible state, `ERANGE`
//! for a character with no single-byte form in `sem:eln.el-getc-fn` — is
//! recorded here. The numbers are Linux's, which is the whole of
//! `plan/decisions/posix-only-scope.md`'s target.
//!
//! The read path's set lives here too — `EINTR`, `EILSEQ`, `EWOULDBLOCK`,
//! `EBADF` and `EIO`, which `sem:read.read-char-fn` and
//! `sem:read.read-fixio-fn` test and store. `read.rs` carried a private copy
//! of them, and a second table of Linux errno numbers is how the two come to
//! disagree about one.
//!
//! **Shape.** The ABI crate copies this into the platform's `errno` on the way
//! out of the C entry point whose rule promises it, which is where the same
//! decision permits reaching libc; the mechanism is `nshedit-abi`'s `errno`
//! module, and this is the value it publishes. The exported reader was the
//! alternative and lost: a C caller reads `errno`, not a symbol only this
//! library knows about.
//!
//! [`set_errno`] is public because the ABI crate has to write this home as
//! well as read it — it clears the value where the C clears `errno`
//! (`sem:readline.read-history-fn`) and records the one errno it produces
//! itself (`sem:eln.el-getc-fn`'s `ERANGE`) — and both have to land here as
//! well as in the platform's copy, or the core and a C caller would disagree
//! about what the last failure was where the C has a single `errno`.
//!
//! **Publishing.** [`writes`] counts what this thread has recorded. The ABI
//! crate samples it around a call and copies only when it moved, which is how
//! "publish on failure" is decided without every entry point restating which
//! of its paths sets `errno`: a call that recorded nothing leaves the caller's
//! `errno` untouched, and a value left over from an earlier failure can never
//! be republished over it.
//!
//! **Semantics.** Matching the C, the value is written only on failure paths
//! and never cleared on success, so it is meaningful only immediately after a
//! call that reported an error.
//!
//! **Threads.** This is a thread-local and so is the C's `errno`, which is
//! what makes the copy sound: both ends of it name the calling thread. The
//! count is per-thread for the same reason — a sample taken on one thread says
//! nothing about another.

use std::cell::Cell;

/// C: `EBADF` — what `read(2)` reports for the descriptor a half-built
/// `EditLine` carries.
pub const EBADF: i32 = 9;

/// C: `EILSEQ` — what `sem:read.read-char-fn` step 4d reports for an
/// over-long multibyte sequence.
pub const EILSEQ: i32 = 84;

/// C: `EINTR`.
pub const EINTR: i32 = 4;

/// C: `EINVAL`.
pub const EINVAL: i32 = 22;

/// C: `EIO`. Only a fallback: every `io::Error` a raw `read(2)` produces on
/// Unix carries its own `errno`, so `read_byte` never actually stores this
/// one.
pub const EIO: i32 = 5;

/// C: `ENOMEM`.
pub const ENOMEM: i32 = 12;

/// C: `ENOSPC`.
pub const ENOSPC: i32 = 28;

/// C: `ERANGE`.
pub const ERANGE: i32 = 34;

/// C: `EWOULDBLOCK`. On Linux `EAGAIN` has the same value, which is why
/// `read__fixio` needs only one label for the would-block condition.
pub const EWOULDBLOCK: i32 = 11;

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
    static WRITES: Cell<u64> = const { Cell::new(0) };
}

/// Reads back what the last failing call recorded.
pub fn errno() -> i32 {
    ERRNO.with(Cell::get)
}

/// How many values this thread has recorded, which is what makes "did this
/// call set `errno`?" answerable: sample it before the call and compare after.
/// The number itself means nothing; only the difference does.
pub fn writes() -> u64 {
    WRITES.with(Cell::get)
}

/// Records what the C would have stored in `errno`.
pub fn set_errno(e: i32) {
    ERRNO.with(|slot| slot.set(e));
    // Saturating rather than wrapping: a wrap could land back on a sample
    // taken 2^64 writes earlier and lose that one publish. It cannot be
    // reached, and if it were, a stuck count publishes too often rather than
    // too rarely.
    WRITES.with(|n| n.set(n.get().saturating_add(1)));
}

#[cfg(test)]
mod tests {
    use super::{ENOSPC, errno, set_errno, writes};

    /// The count moves on every write, including a repeat of the same value —
    /// which is the case a comparison of the values themselves would miss.
    #[test]
    fn writes_counts_repeats() {
        let start = writes();
        set_errno(ENOSPC);
        set_errno(ENOSPC);
        assert_eq!(writes() - start, 2);
        assert_eq!(errno(), ENOSPC);
    }

    /// Reading does not count as writing, so a call that records nothing
    /// leaves the sample where it was and publishes nothing.
    #[test]
    fn reads_do_not_move_the_count() {
        let start = writes();
        let _ = errno();
        assert_eq!(writes(), start);
    }

    /// Both cells are per-thread: a write on another thread is invisible here,
    /// which is what makes copying this into the C's per-thread `errno` sound.
    #[test]
    fn state_is_per_thread() {
        set_errno(ENOSPC);
        let here = writes();
        std::thread::spawn(|| {
            assert_eq!(errno(), 0);
            assert_eq!(writes(), 0);
        })
        .join()
        .unwrap();
        assert_eq!(errno(), ENOSPC);
        assert_eq!(writes(), here);
    }
}
