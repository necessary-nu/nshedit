//! `fputs`/`fprintf` on `el_outfile` and `el_errfile`, done on the
//! descriptors underneath them.
//!
//! This module has no C counterpart. Both destinations are caller-owned
//! `FILE *`s the port cannot write through — `plan/decisions/no-c-ffi.md`
//! bars the core from linking the stdio that owns them — so every byte
//! libedit would have put in a stream goes to the matching descriptor, which
//! the `EditLine` carries for exactly this reason (`def:el.editline`).
//!
//! Seven modules reached for the same three lines and each had grown its own
//! copy, the way `errno` and the locale layer had before they were hoisted.

use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;

use crate::el::EditLine;

/// The one place the core touches an output descriptor.
///
/// Returns the C's `fputs` result: non-negative on success, `EOF` on
/// failure. A negative descriptor stands for the C's NULL stream and fails
/// the same way, which is what lets a caller leave one unset.
///
/// Unbuffered, where the C's `FILE *` is not — see `terminal::terminal_flush`
/// for what that trades away.
pub(crate) fn write_fd(fd: i32, bytes: &[u8]) -> i32 {
    if fd < 0 {
        return -1;
    }
    // SAFETY: the descriptor is the application's and stays open for the life
    // of the `EditLine`; `ManuallyDrop` is what keeps this borrow from
    // closing it, which libedit never does.
    let mut out = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    match out.write_all(bytes) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

impl EditLine {
    /// C: `fputs(s, el->el_outfile)` / `fprintf(el->el_outfile, …)` for an
    /// already-formatted byte string.
    ///
    /// Carries [`write_fd`]'s result onward, because `terminal_putc` is a
    /// translation of a `fputs` whose value the C returns to its caller.
    /// Everywhere else discards it, as the C discards `fprintf`'s.
    pub(crate) fn write_outfile(&self, bytes: &[u8]) -> i32 {
        write_fd(self.el_outfd, bytes)
    }

    /// C: `fprintf(el->el_errfile, …)`. As [`EditLine::write_outfile`], on the
    /// error descriptor.
    ///
    /// Every diagnostic in the crate discards the result and so does the C, so
    /// this offers none rather than leaving one to be discarded 30 times over.
    pub(crate) fn write_errfile(&self, bytes: &[u8]) {
        let _ = write_fd(self.el_errfd, bytes);
    }
}
