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

use crate::compat::el::ElActionT;

// The `el_action_t` an editor command returns. C: `histedit.h` defines these
// as untyped `#define`s; here they carry the type the commands return, so a
// mismatch is a compile error rather than a silent widening.
//
// These are ABI: a consumer's own command function returns one of them.
/// C: `#define CC_NORM 0` — command completed, no redraw needed.
pub const CC_NORM: ElActionT = 0;
/// C: `#define CC_NEWLINE 1` — the line is complete.
pub const CC_NEWLINE: ElActionT = 1;
/// C: `#define CC_EOF 2` — end of input.
pub const CC_EOF: ElActionT = 2;
/// C: `#define CC_ARGHACK 3` — do not reset the pending argument or vi
/// action; this is the mechanism by which counts and operators accumulate
/// across keystrokes.
pub const CC_ARGHACK: ElActionT = 3;
/// C: `#define CC_REFRESH 4` — redraw the line.
pub const CC_REFRESH: ElActionT = 4;
/// C: `#define CC_CURSOR 5` — move the cursor only.
pub const CC_CURSOR: ElActionT = 5;
/// C: `#define CC_ERROR 6` — beep, no redraw.
pub const CC_ERROR: ElActionT = 6;
/// C: `#define CC_FATAL 7` — unrecoverable; the editor resets.
pub const CC_FATAL: ElActionT = 7;
/// C: `#define CC_REDISPLAY 8` — full redisplay.
pub const CC_REDISPLAY: ElActionT = 8;
/// C: `#define CC_REFRESH_BEEP 9` — redraw and beep.
pub const CC_REFRESH_BEEP: ElActionT = 9;

// The `history()` operation codes. C: `histedit.h`. The numbering is ABI —
// a consumer passes these integers directly — so it is reproduced exactly,
// including that `H_SET` is 7 and `H_CURR` is 8 despite being declared the
// other way round in the header.
pub const H_FUNC: i32 = 0;
pub const H_SETSIZE: i32 = 1;
pub const H_GETSIZE: i32 = 2;
pub const H_FIRST: i32 = 3;
pub const H_LAST: i32 = 4;
pub const H_PREV: i32 = 5;
pub const H_NEXT: i32 = 6;
pub const H_SET: i32 = 7;
pub const H_CURR: i32 = 8;
pub const H_ADD: i32 = 9;
pub const H_ENTER: i32 = 10;
pub const H_APPEND: i32 = 11;
pub const H_END: i32 = 12;
pub const H_NEXT_STR: i32 = 13;
pub const H_PREV_STR: i32 = 14;
pub const H_NEXT_EVENT: i32 = 15;
pub const H_PREV_EVENT: i32 = 16;
pub const H_LOAD: i32 = 17;
pub const H_SAVE: i32 = 18;
pub const H_CLEAR: i32 = 19;
pub const H_SETUNIQUE: i32 = 20;
pub const H_GETUNIQUE: i32 = 21;
pub const H_DEL: i32 = 22;
pub const H_NEXT_EVDATA: i32 = 23;
pub const H_DELDATA: i32 = 24;
pub const H_REPLACE: i32 = 25;
pub const H_SAVE_FP: i32 = 26;
pub const H_NSAVE_FP: i32 = 27;

/// C: `typedef struct editline EditLine;` — the editor handle. Its body is
/// `def:el.editline`, in [`crate::compat::el`].
pub use crate::compat::el::EditLine;
/// C: `struct lineinfo` and `struct lineinfow`, which differ only in their
/// three members' character type.
///
/// Not itself a C declaration — the header writes the struct out twice — but
/// the two it *is* are [`LineInfo`] and [`LineInfoW`], which carry the rules.
/// Generic so that `tokenizer.c`, which the C compiles twice against one of
/// them each time, can likewise be one source here.
#[repr(C)]
pub struct LineInfoGen<C> {
    pub buffer: *const C,
    pub cursor: *const C,
    pub lastchar: *const C,
}

/// The narrow user-function line view. The C carries both rules on the one
/// declaration (`typedef struct lineinfo { ... } LineInfo;`), so both sit
/// here.
///
/// The ABI adapter owns one stable record per opaque editor and returns it
/// from `el_line`. Its three pointers refer to the adapter's narrow
/// conversion buffer and are invalidated by the next narrow call; see
/// `sem:eln.el-line-fn`.
pub type LineInfo = LineInfoGen<c_char>;

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

/// The wide user-function line view — both rules sit on the one C
/// declaration.
///
/// `el_wline` returns a live alias of `el->el_line`, not a snapshot: the
/// three pointers change as the user edits, `lastchar` is one past the last
/// character with no NUL there, and all three are invalidated when the line
/// buffer grows. `sem:el.el-wline-fn` requires the port to
/// produce this as a genuine borrowed view rather than a transmute, which is
/// why `def:el.el-line-t`'s field order is frozen.
pub type LineInfoW = LineInfoGen<u32>;

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
