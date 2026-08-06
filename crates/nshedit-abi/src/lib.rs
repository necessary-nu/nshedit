//! The libedit and readline C ABIs.
//!
//! Two surfaces, both of which consumers compile against today:
//!
//! - `histedit.h` — libedit's own API, the `el_*`, `history*` and `tok_*`
//!   entry points.
//! - `editline/readline.h` — the GNU readline compatibility layer.
//!
//! Neither header is the whole ABI. `chartype.h` and `filecomplete.h` are not
//! installed, and seven of their functions carry no `libedit_private` and are
//! therefore exported symbols of `libedit.so` all the same — the two `ct_*`
//! string converters and the five `fn_*` completion entry points. Debian's
//! `libedit.so.2` exports every one of them, so a consumer that declared them
//! itself reaches them today. The symbol table is the contract, not the
//! header, so [`chartype`] and [`filecomplete`] export them here.
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
// The bodies have landed, so the reason these are still here is narrower than
// it was: `readline.c`'s `static` helpers translated across with no callers
// yet, and a handful of parameters an arm accepts to match the C's signature
// and does not read.
#![allow(dead_code, unused_variables)]

pub mod cdecl;
pub mod chartype;
pub mod eln;
pub mod filecomplete;
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
    }
}

/// The caller's `FILE *`, which only this crate may touch.
///
/// Four entry points on the C ABI take a stream the application owns and are
/// specified in terms of what stdio does with it — `sem:el.el-init-fn` is
/// literally `el_init_fd(prog, fin, fout, ferr, fileno(fin), fileno(fout),
/// fileno(ferr))`, `sem:el.el-wset-fn`'s `EL_SETFP` stores `fileno(fp)`
/// alongside the stream, `sem:readline.rl-initialize-fn` steps 4 and 5 take
/// three more `fileno`s, and `sem:history.history-save-fp-fn` decides on
/// `ftell(fp) == 0` and writes with `fputs`/`fprintf`.
///
/// A `FILE *` is an opaque handle into the C library's own object. Its
/// buffer, its position bookkeeping and its descriptor live in memory whose
/// layout is that library's private business and differs between glibc, musl
/// and the BSDs, so no Rust — in this workspace or in any crate — can read
/// it. `plan/decisions/no-c-ffi.md` names this the third and last site where
/// a libc symbol may be declared, and nothing but this module does it.
///
/// # Why not the descriptor
///
/// `fileno` alone would let `std::fs::File::from_raw_fd` carry the writes,
/// which would make this one symbol instead of three. It answers a different
/// question. The stream owns a userspace buffer the descriptor knows nothing
/// about, so:
///
/// - `ftell(fp)` counts bytes still sitting in that buffer and
///   `lseek(fd, 0, SEEK_CUR)` does not. On a stream the caller has already
///   written to and not flushed, the two disagree, and step 1 of
///   `sem:history.history-save-fp-fn` turns that disagreement into a cookie
///   written where the C writes none.
/// - A write to the descriptor reaches the file immediately; a write to the
///   stream sits in the buffer until the caller flushes. Bytes the caller
///   wrote *before* calling us would therefore land *after* ours.
/// - The rule's closing clause — *the stream is neither flushed nor closed*
///   — is a positive statement about where the bytes are when the call
///   returns. Through the descriptor they are on the file; through the
///   stream they are in the caller's buffer, which is what the C promises.
///
/// Flushing first would repair the first two and not the third, and would
/// itself be a stdio call. So the stream is used as a stream.
pub(crate) mod cstdio {
    use core::ffi::{c_char, c_int, c_long, c_void};
    use std::io::{self, Write};

    use nshedit::el::CFile;

    // `fileno`, `ftell` and `fputs` are POSIX and spelled the same in every
    // libc this port targets, so unlike `errno`'s accessor there is no
    // per-platform arm. None of them is `safe`: each dereferences the stream,
    // and a NULL or already-closed one is undefined behaviour in the C too.
    unsafe extern "C" {
        /// C: `int fileno(FILE *stream)` — the descriptor behind a stream, or
        /// -1 with `errno` set if it has none.
        fn fileno(stream: *mut c_void) -> c_int;
        /// C: `long ftell(FILE *stream)` — the stream's position, or -1 on a
        /// stream that cannot report one (a pipe, a socket).
        fn ftell(stream: *mut c_void) -> c_long;
        /// C: `int fputs(const char *s, FILE *stream)` — non-negative on
        /// success, `EOF` on failure.
        fn fputs(s: *const c_char, stream: *mut c_void) -> c_int;
    }

    /// C: `fileno(fp)`, with the null dereference defined away.
    ///
    /// `sem:el.el-init-fn` is explicit that a NULL stream reaching `fileno`
    /// is undefined behaviour and that a port must treat it as a caller
    /// error rather than reproduce it. -1 is that treatment, and it is also
    /// the value `fileno` itself yields for a stream with no descriptor —
    /// which the same rule follows through: the -1 is stored undiagnosed,
    /// construction still reports success, `tty_init` then fails and sets
    /// `NO_TTY`, and nothing else notices.
    pub(crate) fn fileno_of(stream: CFile) -> c_int {
        if stream.is_null() {
            return -1;
        }
        // SAFETY: a non-NULL `CFile` is a live `FILE *` the caller owns; that
        // is the contract every rule taking one states, and the C dereferences
        // it with no more checking than this.
        unsafe { fileno(stream) }
    }

    /// C: `ftell(fp) == 0` — step 1 of `sem:history.history-save-fp-fn`.
    ///
    /// A NULL stream answers `true`, so the cookie write is attempted and
    /// fails through [`CFileWriter`], which is the -1/`_HE_HIST_WRITE` the
    /// port already defined for this case before the stream could be reached
    /// at all. The C would fault instead.
    pub(crate) fn at_start(stream: CFile) -> bool {
        if stream.is_null() {
            return true;
        }
        // SAFETY: as `fileno_of`.
        unsafe { ftell(stream) == 0 }
    }

    /// The caller's stream as a byte sink, one `fputs` per write.
    ///
    /// [`Write::flush`] is deliberately a no-op: the rule ends *the stream is
    /// neither flushed nor closed — the caller owns it*, so everything
    /// written here stays in the caller's own buffer exactly as the C leaves
    /// it.
    pub(crate) struct CFileWriter {
        stream: CFile,
        /// The NUL-terminated copy `fputs` needs. Reused across writes, as
        /// the C reuses its `ptr` scratch.
        term: Vec<u8>,
    }

    impl CFileWriter {
        pub(crate) fn new(stream: CFile) -> Self {
            Self {
                stream,
                term: Vec::new(),
            }
        }
    }

    impl Write for CFileWriter {
        /// One call, one `fputs`.
        ///
        /// The two things the core writes are the 13-byte history cookie and
        /// one vis-encoded entry with its newline, and neither can contain a
        /// NUL — `strvis` never emits one. That is the same guarantee the C's
        /// own `fputs(hist_cookie, fp)` and `fprintf(fp, "%s\n", ptr)` rest
        /// on, so terminating here loses nothing: `fputs` writes exactly the
        /// bytes `buf` holds.
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.stream.is_null() {
                return Err(io::Error::from(io::ErrorKind::InvalidInput));
            }
            self.term.clear();
            self.term.reserve(buf.len() + 1);
            self.term.extend_from_slice(buf);
            self.term.push(0);
            // SAFETY: `term` is NUL-terminated and outlives the call, and the
            // stream is the caller's live `FILE *`.
            let rc = unsafe { fputs(self.term.as_ptr().cast::<c_char>(), self.stream) };
            if rc < 0 {
                // The C's `EOF`. Only the cookie write is checked
                // (`sem:history.history-save-fp-fn` step 1); the per-entry
                // writes discard this, which is ERR-history-21.
                return Err(io::Error::other("fputs"));
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
