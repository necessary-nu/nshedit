//! Record and opcode shapes used by the translated compatibility engine.
//!
//! `nshedit-abi::cdecl::histedit` owns the installed C declarations. These
//! temporary twins remain `#[repr(C)]` only while the translated history and
//! tokenizer payloads consume them; the ABI crate checks their layout before
//! casting at that migration seam.
//!
//! The wide and narrow translated modules share generic record bodies over
//! `u32` and `c_char`. Their handle re-exports and integer opcodes exist only
//! so the remaining file-for-file implementation can compile; they are not a
//! native Rust API or a source for the public headers.

use core::ffi::{c_char, c_int};

// The installed declarations own these ABI values. The translated engine
// imports only the subset it still executes.
pub(crate) use crate::cdecl::histedit::{
    CC_ARGHACK, CC_CURSOR, CC_EOF, CC_ERROR, CC_FATAL, CC_NEWLINE, CC_NORM, CC_REDISPLAY,
    CC_REFRESH, CC_REFRESH_BEEP, H_FIRST, H_LAST, H_NEXT, H_PREV, H_SETSIZE, H_SETUNIQUE,
};

/// C: `typedef struct editline EditLine;` — the editor handle. Its body is
/// `def:el.editline`, in [`crate::compat::el`].
pub use crate::compat::el::EditLine;

/// C: `struct histevent` and `struct histeventW`, which differ only in
/// `str`'s character type.
///
/// As [`LineInfoGen`]: the C declares the two separately, and the two are
/// [`HistEvent`] and [`HistEventW`], which carry the rules. Generic so that
/// `history.c` can be one source across its two compilations.
#[repr(C)]
pub struct HistEventGen<C> {
    pub num: i32,
    pub str: *const C,
}

/// A narrow history event.
///
/// `str` is borrowed from the history entry that produced it and is
/// invalidated when that entry is deleted or replaced; see
/// `sem:histedit.history-fn`.
pub type HistEvent = HistEventGen<c_char>;

/// C: `typedef int (*el_rfunc_t)(EditLine *, wchar_t *);`
///
/// The character-reading hook installed by `EL_GETCFN`, and part of the ABI:
/// a C application hands one in through `el_set` and libedit calls it back,
/// so this is `unsafe extern "C"` with the C's own parameter shapes. The
/// second argument is a one-element out parameter — `wchar_t *`, hence
/// `*mut u32` rather than `&mut u32`, since the callee is C code the borrow
/// rules do not reach.
///
/// Returns 1 for a character read, 0 for end of input, -1 for an error.
pub type ElRfuncT = unsafe extern "C" fn(*mut EditLine, *mut u32) -> c_int;

/// A wide history event — both rules sit on the one C declaration.
///
/// Embedded in `el_history_t` as the event cookie, and filled in by every
/// `history_w` operation. `str` is borrowed from the history entry that owns
/// it and is invalidated when that entry is deleted or replaced.
pub type HistEventW = HistEventGen<u32>;
