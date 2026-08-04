//! Ported from `src/hist.c`; rules live in `docs/spec/port/src/hist.md`.

use core::ffi::{c_int, c_void};

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
