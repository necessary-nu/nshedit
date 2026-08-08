//! ABI-owned declarations for the installed `histedit.h`.
//!
//! The complete records, operation codes, callback spelling, version macros,
//! and foreign scalar aliases live here so header generation never parses the
//! core. The translated engine temporarily carries layout-compatible record
//! twins; compile-time size, alignment, and offset assertions below guard the
//! casts at that migration seam.

use core::ffi::{c_char, c_int, c_void};
use core::mem::{align_of, offset_of, size_of};

use super::handles::EditLine;

/// C: `#define LIBEDIT_MAJOR 2`.
pub const LIBEDIT_MAJOR: c_int = 2;
/// C: `#define LIBEDIT_MINOR 11`.
pub const LIBEDIT_MINOR: c_int = 11;

/// C: `#define CC_NORM 0` — command completed, no redraw needed.
pub const CC_NORM: u8 = 0;
/// C: `#define CC_NEWLINE 1` — the line is complete.
pub const CC_NEWLINE: u8 = 1;
/// C: `#define CC_EOF 2` — end of input.
pub const CC_EOF: u8 = 2;
/// C: `#define CC_ARGHACK 3` — preserve the pending argument or vi action.
pub const CC_ARGHACK: u8 = 3;
/// C: `#define CC_REFRESH 4` — redraw the line.
pub const CC_REFRESH: u8 = 4;
/// C: `#define CC_CURSOR 5` — move the cursor only.
pub const CC_CURSOR: u8 = 5;
/// C: `#define CC_ERROR 6` — beep, no redraw.
pub const CC_ERROR: u8 = 6;
/// C: `#define CC_FATAL 7` — unrecoverable; the editor resets.
pub const CC_FATAL: u8 = 7;
/// C: `#define CC_REDISPLAY 8` — full redisplay.
pub const CC_REDISPLAY: u8 = 8;
/// C: `#define CC_REFRESH_BEEP 9` — redraw and beep.
pub const CC_REFRESH_BEEP: u8 = 9;

/// C: `history()` operation codes.
pub const H_FUNC: c_int = 0;
pub const H_SETSIZE: c_int = 1;
pub const H_GETSIZE: c_int = 2;
pub const H_FIRST: c_int = 3;
pub const H_LAST: c_int = 4;
pub const H_PREV: c_int = 5;
pub const H_NEXT: c_int = 6;
pub const H_SET: c_int = 7;
pub const H_CURR: c_int = 8;
pub const H_ADD: c_int = 9;
pub const H_ENTER: c_int = 10;
pub const H_APPEND: c_int = 11;
pub const H_END: c_int = 12;
pub const H_NEXT_STR: c_int = 13;
pub const H_PREV_STR: c_int = 14;
pub const H_NEXT_EVENT: c_int = 15;
pub const H_PREV_EVENT: c_int = 16;
pub const H_LOAD: c_int = 17;
pub const H_SAVE: c_int = 18;
pub const H_CLEAR: c_int = 19;
pub const H_SETUNIQUE: c_int = 20;
pub const H_GETUNIQUE: c_int = 21;
pub const H_DEL: c_int = 22;
pub const H_NEXT_EVDATA: c_int = 23;
pub const H_DELDATA: c_int = 24;
pub const H_REPLACE: c_int = 25;
pub const H_SAVE_FP: c_int = 26;
pub const H_NSAVE_FP: c_int = 27;

/// C: `wchar_t`.
///
/// The core spells the wide character `u32` and cbindgen renders that
/// `uint32_t`, which is a **different C type**: `wchar_t` is `int` on Linux
/// and signed on every other target this library builds for, so
/// `uint32_t *` and `wchar_t *` are incompatible pointer types and a
/// consumer passing one where the other is declared gets a diagnostic. The
/// header must say `wchar_t`.
///
/// `u32` is the right Rust representation and stays; this alias only gives
/// the generator a name to print. The generator excludes it from the output,
/// because `<wchar.h>` already declares the real one.
pub type WcharT = u32;

/// C: `FILE *` — a stream the application owns.
///
/// Rust stores an opaque `*mut c_void`, which would render as `void *`.
/// `FILE *` is a stronger C type, so the generator renames this alias. See
/// [`crate::cstdio`] for why the stream is used as a stream.
pub type CFile = *mut c_void;

/// C: `struct lineinfo` and `struct lineinfow`, differing only in character
/// type.
#[repr(C)]
pub struct LineInfoGen<C> {
    pub buffer: *const C,
    pub cursor: *const C,
    pub lastchar: *const C,
}

// [spec:libedit:def:histedit.lineinfo]
// [spec:libedit:def:histedit.line-info]
/// C: `typedef struct lineinfo { ... } LineInfo;`.
pub type LineInfo = LineInfoGen<c_char>;

// [spec:libedit:def:histedit.lineinfow]
// [spec:libedit:def:histedit.line-info-w]
/// C: `typedef struct lineinfow { ... } LineInfoW;` — `def:histedit.lineinfow`.
///
/// Named `LineInfoWide` and renamed at generation because [`WcharT`] must be
/// printed as C's `wchar_t`.
pub type LineInfoWide = LineInfoGen<WcharT>;

/// C: `struct HistEvent` and `struct histeventW`, differing only in character
/// type.
#[repr(C)]
pub struct HistEventGen<C> {
    pub num: c_int,
    pub str: *const C,
}

// [spec:libedit:def:histedit.hist-event]
/// C: `typedef struct HistEvent { ... } HistEvent;`.
pub type HistEvent = HistEventGen<c_char>;

// [spec:libedit:def:histedit.histevent-w]
// [spec:libedit:def:histedit.hist-event-w]
/// C: `typedef struct histeventW { ... } HistEventW;` — `def:histedit.hist-event-w`.
pub type HistEventWide = HistEventGen<WcharT>;

/// C: `typedef int (*el_rfunc_t)(EditLine *, wchar_t *);` —
/// `def:histedit.el-rfunc-t-edit-line-wchar-t`.
///
/// The character-reading hook `EL_GETCFN` installs. Nothing in the header's
/// own signatures mentions it — it reaches a consumer only through the
/// varargs of `el_set`/`el_wset` — so the generator is told to emit it
/// explicitly; it would otherwise be dropped as unreachable.
///
/// [`EditLine`] is the declaration-only incomplete handle, so the header
/// cannot accidentally learn the ABI adapter's allocation layout.
// [spec:libedit:def:histedit.el-rfunc-t-edit-line-wchar-t]
pub type ElReadCallback = unsafe extern "C" fn(*mut EditLine, *mut WcharT) -> c_int;

/// The translated payload still consumes its own temporary record twins.
/// These assertions make every pointer cast at that seam a checked layout
/// claim rather than an assumption.
const _: () = {
    assert!(size_of::<LineInfo>() == size_of::<nshedit::histedit::LineInfo>());
    assert!(align_of::<LineInfo>() == align_of::<nshedit::histedit::LineInfo>());
    assert!(offset_of!(LineInfo, buffer) == offset_of!(nshedit::histedit::LineInfo, buffer));
    assert!(offset_of!(LineInfo, cursor) == offset_of!(nshedit::histedit::LineInfo, cursor));
    assert!(offset_of!(LineInfo, lastchar) == offset_of!(nshedit::histedit::LineInfo, lastchar));

    assert!(size_of::<LineInfoWide>() == size_of::<nshedit::histedit::LineInfoW>());
    assert!(align_of::<LineInfoWide>() == align_of::<nshedit::histedit::LineInfoW>());
    assert!(offset_of!(LineInfoWide, buffer) == offset_of!(nshedit::histedit::LineInfoW, buffer));
    assert!(offset_of!(LineInfoWide, cursor) == offset_of!(nshedit::histedit::LineInfoW, cursor));
    assert!(
        offset_of!(LineInfoWide, lastchar) == offset_of!(nshedit::histedit::LineInfoW, lastchar)
    );

    assert!(size_of::<HistEvent>() == size_of::<nshedit::histedit::HistEvent>());
    assert!(align_of::<HistEvent>() == align_of::<nshedit::histedit::HistEvent>());
    assert!(offset_of!(HistEvent, num) == offset_of!(nshedit::histedit::HistEvent, num));
    assert!(offset_of!(HistEvent, str) == offset_of!(nshedit::histedit::HistEvent, str));

    assert!(size_of::<HistEventWide>() == size_of::<nshedit::histedit::HistEventW>());
    assert!(align_of::<HistEventWide>() == align_of::<nshedit::histedit::HistEventW>());
    assert!(offset_of!(HistEventWide, num) == offset_of!(nshedit::histedit::HistEventW, num));
    assert!(offset_of!(HistEventWide, str) == offset_of!(nshedit::histedit::HistEventW, str));
};
