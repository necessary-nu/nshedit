//! Ported from `src/chared.c`; rules live in `docs/spec/port/src/chared.md`.

// Every function body below is still `todo!()`, so no parameter is read yet.
// Remove this once the function translations land.
#![allow(unused_variables)]
// The C's function names are kept verbatim, and several of them — the `c__`,
// `cv__` and `isWord` families — are not snake case.
#![allow(non_snake_case)]

use core::ffi::{c_char, c_void};

use crate::el::{EditLine, ElActionT};

// [spec:libedit:def:chared.c-undo-t]
/// Undo information for vi — there is no undo in emacs (yet).
pub struct CUndoT {
    /// C: `ssize_t len` — length of the saved line, or -1 for "nothing
    /// saved". The sentinel is why this stays signed.
    pub len: isize,
    /// Position of the saved cursor. Already an index in the C, so
    /// `ch_enlargebufs` has nothing to rebase here.
    pub cursor: i32,
    /// C: `wchar_t *buf` — full saved text, owned.
    pub buf: Vec<u32>,
}

// [spec:libedit:def:chared.c-redo-t]
/// Redo for vi.
pub struct CRedoT {
    /// C: `wchar_t *buf` — redo insert key sequence, owned.
    pub buf: Vec<u32>,
    /// C: `wchar_t *pos` — write position, offset into `buf`.
    pub pos: usize,
    /// C: `wchar_t *lim` — usable limit, offset into `buf`. Note that
    /// `ch_enlargebufs` keeps the *old* offset here even as the allocation
    /// grows, so the redo buffer's usable limit does not grow with it; see
    /// `sem:chared.ch-enlargebufs-fn` step 7.
    pub lim: usize,
    /// Command to redo.
    pub cmd: ElActionT,
    /// C: `wchar_t ch` — char that invoked it.
    pub ch: u32,
    pub count: i32,
    /// From `cv_action()`.
    pub action: i32,
}

// [spec:libedit:def:chared.c-vcmd-t]
/// Current action information for vi.
pub struct CVcmdT {
    pub action: i32,
    /// C: `wchar_t *pos` — offset into `el_line.buffer`, not into any
    /// buffer of this struct.
    pub pos: usize,
}

// [spec:libedit:def:chared.c-kill-t]
/// Kill buffer for emacs.
pub struct CKillT {
    /// C: `wchar_t *buf` — the kill buffer, owned.
    pub buf: Vec<u32>,
    /// C: `wchar_t *last` — offset into `buf`.
    pub last: usize,
    /// C: `wchar_t *mark` — offset into **`el_line.buffer`**, not into
    /// `buf`. The asymmetry is the C's: `ch_enlargebufs` rebases `last`
    /// against the old kill base and `mark` against the old line base.
    ///
    /// `sem:emacs.em-set-mark-fn` records the mark's
    /// properties: it starts at the head of the line and is never NULL,
    /// which is why the NULL guards in `em_kill_region` and
    /// `em_copy_region` never fire; nothing but `ch_enlargebufs` and the
    /// explicit setters ever adjusts it, so editing moves text out from
    /// under it and it can end up above `lastchar`.
    pub mark: usize,
}

// [spec:libedit:def:chared.el-zfunc-t-edit-line-void]
/// C: `typedef void (*el_zfunc_t)(EditLine *, void *);`
///
/// The line-resize hook installed by `EL_RESIZE`, called once
/// `ch_enlargebufs` has published the new capacity so the application can
/// re-derive any pointers it holds into the line.
pub type ElZfuncT = fn(&mut EditLine, *mut c_void);

// [spec:libedit:def:chared.el-afunc-t-void-const-char]
/// C: `typedef const char *(*el_afunc_t)(void *, const char *);`
///
/// The alias-text hook installed by `EL_ALIAS_TEXT`. Both strings are narrow
/// and borrowed across the C ABI, so they stay raw pointers.
pub type ElAfuncT = fn(*mut c_void, *const c_char) -> *const c_char;

// [spec:libedit:def:chared.el-chared-t]
/// Both the emacs and the vi state, because the user can bind commands from
/// both editors.
pub struct ElCharedT {
    pub c_undo: CUndoT,
    pub c_kill: CKillT,
    pub c_redo: CRedoT,
    pub c_vcmd: CVcmdT,
    pub c_resizefun: Option<ElZfuncT>,
    pub c_aliasfun: Option<ElAfuncT>,
    /// C: `void *c_resizearg` — client cookie passed back to
    /// `c_resizefun`, never inspected.
    pub c_resizearg: *mut c_void,
    /// C: `void *c_aliasarg` — client cookie passed back to `c_aliasfun`,
    /// never inspected.
    pub c_aliasarg: *mut c_void,
}

// [spec:libedit:def:chared.cv-undo-fn]
// [spec:libedit:sem:chared.cv-undo-fn]
pub(crate) fn cv_undo(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:chared.cv-yank-fn]
// [spec:libedit:sem:chared.cv-yank-fn]
/// C: `const wchar_t *ptr` — an offset into `el_line.buffer`. Every caller
/// passes `el_line.buffer`, `el_line.cursor`, or the cursor displaced by a
/// count, so this is a line position and not a string of its own.
pub(crate) fn cv_yank(el: &mut EditLine, ptr: usize, size: i32) {
    todo!()
}

// [spec:libedit:def:chared.c-insert-fn]
// [spec:libedit:sem:chared.c-insert-fn]
pub(crate) fn c_insert(el: &mut EditLine, num: i32) {
    todo!()
}

// [spec:libedit:def:chared.c-delafter-fn]
// [spec:libedit:sem:chared.c-delafter-fn]
pub(crate) fn c_delafter(el: &mut EditLine, num: i32) {
    todo!()
}

// [spec:libedit:def:chared.c-delafter1-fn]
// [spec:libedit:sem:chared.c-delafter1-fn]
pub(crate) fn c_delafter1(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:chared.c-delbefore-fn]
// [spec:libedit:sem:chared.c-delbefore-fn]
pub(crate) fn c_delbefore(el: &mut EditLine, num: i32) {
    todo!()
}

// [spec:libedit:def:chared.c-delbefore1-fn]
// [spec:libedit:sem:chared.c-delbefore1-fn]
pub(crate) fn c_delbefore1(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:chared.ce-isword-fn]
// [spec:libedit:sem:chared.ce-isword-fn]
pub(crate) fn ce__isword(el: &mut EditLine, p: u32) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.cv-isword-fn]
// [spec:libedit:sem:chared.cv-isword-fn]
pub(crate) fn cv__isword(el: &mut EditLine, p: u32) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.cv-is-word-fn]
// [spec:libedit:sem:chared.cv-is-word-fn]
/// The capital `W` is the C's: this is the vi big-word test, `cv__isword`'s
/// coarser sibling.
pub(crate) fn cv__isWord(el: &mut EditLine, p: u32) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.c-prev-word-fn]
// [spec:libedit:sem:chared.c-prev-word-fn]
/// `p` and `low` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn c__prev_word(
    el: &mut EditLine,
    p: usize,
    low: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    todo!()
}

// [spec:libedit:def:chared.c-next-word-fn]
// [spec:libedit:sem:chared.c-next-word-fn]
/// `p` and `high` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn c__next_word(
    el: &mut EditLine,
    p: usize,
    high: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    todo!()
}

// [spec:libedit:def:chared.cv-next-word-fn]
// [spec:libedit:sem:chared.cv-next-word-fn]
/// `p` and `high` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn cv_next_word(
    el: &mut EditLine,
    p: usize,
    high: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    todo!()
}

// [spec:libedit:def:chared.cv-prev-word-fn]
// [spec:libedit:sem:chared.cv-prev-word-fn]
/// `p` and `low` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn cv_prev_word(
    el: &mut EditLine,
    p: usize,
    low: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    todo!()
}

// [spec:libedit:def:chared.cv-delfini-fn]
// [spec:libedit:sem:chared.cv-delfini-fn]
pub(crate) fn cv_delfini(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:chared.cv-endword-fn]
// [spec:libedit:sem:chared.cv-endword-fn]
/// `p` and `high` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn cv__endword(
    el: &mut EditLine,
    p: usize,
    high: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    todo!()
}

// [spec:libedit:def:chared.ch-init-fn]
// [spec:libedit:sem:chared.ch-init-fn]
pub(crate) fn ch_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.ch-reset-fn]
// [spec:libedit:sem:chared.ch-reset-fn]
pub(crate) fn ch_reset(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:chared.ch-enlargebufs-fn]
// [spec:libedit:sem:chared.ch-enlargebufs-fn]
pub(crate) fn ch_enlargebufs(el: &mut EditLine, addlen: usize) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.ch-end-fn]
// [spec:libedit:sem:chared.ch-end-fn]
pub(crate) fn ch_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:chared.el-winsertstr-fn]
// [spec:libedit:sem:chared.el-winsertstr-fn]
/// C: `const wchar_t *s` — a NUL-terminated string the caller owns and
/// libedit only reads, so the length comes from the slice rather than from
/// `wcslen`. The C's `s == NULL` and `wcslen(s) == 0` rejections are the
/// same case here: an empty slice.
pub fn el_winsertstr(el: &mut EditLine, s: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.el-deletestr-fn]
// [spec:libedit:sem:chared.el-deletestr-fn]
pub fn el_deletestr(el: &mut EditLine, n: i32) {
    todo!()
}

// [spec:libedit:def:chared.el-deletestr1-fn]
// [spec:libedit:sem:chared.el-deletestr1-fn]
pub fn el_deletestr1(el: &mut EditLine, start: i32, end: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.el-wreplacestr-fn]
// [spec:libedit:sem:chared.el-wreplacestr-fn]
/// `s` is borrowed exactly as in [`el_winsertstr`].
pub fn el_wreplacestr(el: &mut EditLine, s: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.el-cursor-fn]
// [spec:libedit:sem:chared.el-cursor-fn]
pub fn el_cursor(el: &mut EditLine, n: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.c-gets-fn]
// [spec:libedit:sem:chared.c-gets-fn]
/// `buf` is caller storage, not part of `el` — both callers pass a local
/// `wchar_t[EL_BUFSIZ]` — so it stays a borrowed slice rather than becoming
/// an index. `prompt` is optional because the C tests it against NULL.
pub(crate) fn c_gets(el: &mut EditLine, buf: &mut [u32], prompt: Option<&[u32]>) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.c-hpos-fn]
// [spec:libedit:sem:chared.c-hpos-fn]
pub(crate) fn c_hpos(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.ch-resizefun-fn]
// [spec:libedit:sem:chared.ch-resizefun-fn]
/// `f` is optional because `el_set(EL_RESIZE, ...)` may pass NULL, which
/// stores NULL and so switches the hook back off.
pub(crate) fn ch_resizefun(el: &mut EditLine, f: Option<ElZfuncT>, a: *mut c_void) -> i32 {
    todo!()
}

// [spec:libedit:def:chared.ch-aliasfun-fn]
// [spec:libedit:sem:chared.ch-aliasfun-fn]
/// `f` is optional for the same reason as in [`ch_resizefun`].
pub(crate) fn ch_aliasfun(el: &mut EditLine, f: Option<ElAfuncT>, a: *mut c_void) -> i32 {
    todo!()
}
