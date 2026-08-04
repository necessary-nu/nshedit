//! Ported from `src/read.c`; rules live in `docs/spec/port/src/read.md`.

// The signatures land before the bodies, so every parameter is unused until
// its `todo!()` is replaced. Remove this with the last one.
#![allow(unused_variables)]

use crate::el::{EditLine, ElActionT};
use crate::histedit::ElRfuncT;

/// C: `#define EL_MAXMACRO 10` — the macro nesting limit.
pub const EL_MAXMACRO: usize = 10;

// [spec:libedit:def:read.macros]
/// The macro pushback stack.
pub struct Macros {
    /// C: `wchar_t **macro` — up to `EL_MAXMACRO` owned strings, innermost
    /// last. `macro` is a Rust keyword, so the name is written `r#macro`;
    /// it is still the C's field name.
    pub r#macro: Vec<Vec<u32>>,
    /// Index of the innermost live macro, -1 when none is running.
    pub level: i32,
    /// Read position within `macro[level]`.
    pub offset: i32,
}

// [spec:libedit:def:read.el-read-t]
/// The character-reading state, hung off `EditLine::el_read`.
pub struct ElReadT {
    pub macros: Macros,
    /// Function to read a character.
    pub read_char: Option<ElRfuncT>,
    /// The `errno` the last read failed with, surfaced through
    /// `EL_GETCFN`'s error reporting.
    pub read_errno: i32,
}

// [spec:libedit:def:read.read-init-fn]
// [spec:libedit:sem:read.read-init-fn]
/// Initialize the read stuff. 0 on success, -1 if an allocation failed.
pub(crate) fn read_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:read.read-end-fn]
// [spec:libedit:sem:read.read-end-fn]
/// Free the data structures used by the read stuff.
pub(crate) fn read_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:read.el-read-setfn-fn]
// [spec:libedit:sem:read.el-read-setfn-fn]
/// Set the read-char function to the one provided. `None` is the C's
/// `EL_BUILTIN_GETCFN` — a NULL `el_rfunc_t` — and restores [`read_char`].
pub(crate) fn el_read_setfn(el_read: &mut ElReadT, rc: Option<ElRfuncT>) -> i32 {
    todo!()
}

// [spec:libedit:def:read.el-read-getfn-fn]
// [spec:libedit:sem:read.el-read-getfn-fn]
/// Return the current read-char function, or `None` when it is the builtin
/// one — the C's `EL_BUILTIN_GETCFN`.
pub(crate) fn el_read_getfn(el_read: &mut ElReadT) -> Option<ElRfuncT> {
    todo!()
}

// [spec:libedit:def:read.read-fixio-fn]
// [spec:libedit:sem:read.read-fixio-fn]
/// Try to recover from a failed read; `e` is the `errno` it failed with.
#[allow(non_snake_case)]
fn read__fixio(fd: i32, e: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:read.el-wpush-fn]
// [spec:libedit:sem:read.el-wpush-fn]
/// Push a macro onto the back of the pending queue. `None` is the C's NULL
/// `str`, a live call site from `read_getcmd` whose only effect is the beep.
pub fn el_wpush(el: &mut EditLine, str: Option<&[u32]>) {
    todo!()
}

// [spec:libedit:def:read.read-getcmd-fn]
// [spec:libedit:sem:read.read-getcmd-fn]
/// Get the next command from the input stream: 0 on success, -1 on EOF or
/// error.
fn read_getcmd(el: &mut EditLine, cmdnum: &mut ElActionT, ch: &mut u32) -> i32 {
    todo!()
}

// [spec:libedit:def:read.read-char-fn]
// [spec:libedit:sem:read.read-char-fn]
/// Read a character from the tty. This is the builtin [`ElRfuncT`], so its
/// signature is that type's: 1 for a character, 0 for end of input, -1 for
/// an error.
fn read_char(el: &mut EditLine, cp: &mut u32) -> i32 {
    todo!()
}

// [spec:libedit:def:read.read-pop-fn]
// [spec:libedit:sem:read.read-pop-fn]
/// Drop the draining macro and shuffle the queue down.
fn read_pop(ma: &mut Macros) {
    todo!()
}

// [spec:libedit:def:read.read-clearmacros-fn]
// [spec:libedit:sem:read.read-clearmacros-fn]
/// Discard every queued macro.
fn read_clearmacros(ma: &mut Macros) {
    todo!()
}

// [spec:libedit:def:read.el-wgetc-fn]
// [spec:libedit:sem:read.el-wgetc-fn]
/// Read a wide character, from the macro queue while one is draining and
/// from the tty otherwise.
pub fn el_wgetc(el: &mut EditLine, cp: &mut u32) -> i32 {
    todo!()
}

// [spec:libedit:def:read.read-prepare-fn]
// [spec:libedit:sem:read.read-prepare-fn]
/// Set up for a read: signals, raw mode, resize, and the prompt.
pub(crate) fn read_prepare(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:read.read-finish-fn]
// [spec:libedit:sem:read.read-finish-fn]
/// Undo [`read_prepare`]: cooked mode and the signal handlers.
pub(crate) fn read_finish(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:read.noedit-wgets-fn]
// [spec:libedit:sem:read.noedit-wgets-fn]
/// Read a line with editing disabled. The C returns `el_line.buffer` or
/// NULL, so this borrows `el` for as long as the caller holds the line.
fn noedit_wgets<'a>(el: &'a mut EditLine, nread: &mut i32) -> Option<&'a [u32]> {
    todo!()
}

// [spec:libedit:def:read.el-wgets-fn]
// [spec:libedit:sem:read.el-wgets-fn]
/// Read a line. `nread` is `None` for the C's NULL out-parameter, which it
/// retargets at a local. The result is a view of `el_line.buffer`, valid
/// until the next call, which is what the borrow of `el` expresses.
pub fn el_wgets<'a>(el: &'a mut EditLine, nread: Option<&mut i32>) -> Option<&'a [u32]> {
    todo!()
}
