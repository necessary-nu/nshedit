//! Ported from `src/hist.c`; rules live in `docs/spec/port/src/hist.md`.

// Every function below is a signature with a `todo!()` body, so no parameter
// is read yet. Remove this once the translations land.
#![allow(unused_variables)]

use core::ffi::{c_int, c_void};

use crate::el::{EditLine, ElActionT};
use crate::histedit::HistEventW;

// [spec:libedit:def:hist.hist-fun-t-void-hist-event-w-int]
/// C: `typedef int (*hist_fun_t)(void *, HistEventW *, int, ...);`
///
/// The history dispatch hook installed by `el_set(el, EL_HIST, fun, ptr)`,
/// normally `history_w`. This is the one callback in the port that has to be
/// `extern "C"` and variadic: the C ABI genuinely passes a variadic function
/// pointer here, and libedit calls it through `HIST_FUN` with zero or one
/// trailing argument depending on the operation. Rust has no safe variadic
/// `fn` pointer, so calls through it are `unsafe`, exactly as the C's are
/// unchecked.
pub type HistFunT = unsafe extern "C" fn(*mut c_void, *mut HistEventW, c_int, ...) -> c_int;

// [spec:libedit:def:hist.el-history-t]
/// The `EditLine`'s view of the history: a stash for the line being edited,
/// plus the hook that reaches the actual history object.
pub struct ElHistoryT {
    /// C: `wchar_t *buf` — the history buffer, owned. Holds the live line
    /// while the user walks the history.
    pub buf: Vec<u32>,
    /// C: `size_t sz` — allocated `wchar_t` count of `buf`.
    pub sz: usize,
    /// C: `wchar_t *last` — offset into `buf`, one past the saved line.
    ///
    /// `sem:common.ed-search-next-history-fn` notes that the
    /// C's `wcsncpy` into `buf` does not NUL-terminate when the saved line
    /// exactly fills it, so the port must use this length rather than a
    /// terminator.
    pub last: usize,
    /// Event we are looking for.
    pub eventno: i32,
    /// C: `void *ref` — argument for the history functions, a client cookie
    /// libedit stores and hands back untouched. `el_end` does not free it.
    pub r#ref: *mut c_void,
    /// Event access.
    pub fun: Option<HistFunT>,
    /// Event cookie. `ev.str` is borrowed from the history entry the last
    /// operation returned.
    pub ev: HistEventW,
}

// [spec:libedit:def:hist.hist-init-fn]
// [spec:libedit:sem:hist.hist-init-fn]
/// C: `libedit_private int hist_init(EditLine *el)`
pub(crate) fn hist_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:hist.hist-end-fn]
// [spec:libedit:sem:hist.hist-end-fn]
/// C: `libedit_private void hist_end(EditLine *el)`
pub(crate) fn hist_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:hist.hist-set-fn]
// [spec:libedit:sem:hist.hist-set-fn]
/// C: `libedit_private int hist_set(EditLine *el, hist_fun_t fun, void *ptr)`
///
/// `fun` is `Option<HistFunT>` because the C stores it straight into
/// [`ElHistoryT::fun`] with no NULL check, and `el_set(EL_HIST, NULL, NULL)`
/// is how a caller detaches the history.
pub(crate) fn hist_set(el: &mut EditLine, fun: Option<HistFunT>, ptr: *mut c_void) -> i32 {
    todo!()
}

// [spec:libedit:def:hist.hist-get-fn]
// [spec:libedit:sem:hist.hist-get-fn]
/// C: `libedit_private el_action_t hist_get(EditLine *el)`
pub(crate) fn hist_get(el: &mut EditLine) -> ElActionT {
    todo!()
}

// [spec:libedit:def:hist.hist-command-fn]
// [spec:libedit:sem:hist.hist-command-fn]
/// C: `libedit_private int hist_command(EditLine *el, int argc, const wchar_t **argv)`
///
/// `argv` stays a raw array of NUL-terminated wide strings: it is the shared
/// shape of every builtin command function (`map_bind`, `tty_stty`,
/// `terminal_telltc`, …), handed straight through from the tokenizer, and
/// nothing here owns it.
pub(crate) fn hist_command(el: &mut EditLine, argc: i32, argv: *const *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:hist.hist-enlargebuf-fn]
// [spec:libedit:sem:hist.hist-enlargebuf-fn]
/// C: `libedit_private int hist_enlargebuf(EditLine *el, size_t newsz)`
pub(crate) fn hist_enlargebuf(el: &mut EditLine, newsz: usize) -> i32 {
    todo!()
}

// [spec:libedit:def:hist.hist-convert-fn]
// [spec:libedit:sem:hist.hist-convert-fn]
/// C: `libedit_private wchar_t *hist_convert(EditLine *el, int fn, void *arg)`
///
/// `fn` is a Rust keyword, so the parameter is spelled `r#fn`, the way
/// `ElHistoryT::r#ref` spells the C's `ref`. The return is a raw pointer
/// because it is `ct_decode_string`'s view into `el->el_scratch`, valid only
/// until the next conversion.
pub(crate) fn hist_convert(el: &mut EditLine, r#fn: i32, arg: *mut c_void) -> *mut u32 {
    todo!()
}
