//! Ported from `src/refresh.c`; rules live in
//! `docs/spec/port/src/refresh.md`.

// Every function body below is still `todo!()`, so no parameter is read yet.
// Remove this once the function translations land.
#![allow(unused_variables)]
// The C's function names are kept verbatim, and the `re__` family is not
// snake case.
#![allow(non_snake_case)]

use crate::el::{CoordT, EditLine};

// [spec:libedit:def:refresh.el-refresh-t]
/// Where the refresh machinery believes the cursor is, and how tall the
/// display was last time round.
pub struct ElRefreshT {
    /// Refresh cursor position.
    pub r_cursor: CoordT,
    /// Vertical locations: rows used by the previous refresh.
    pub r_oldcv: i32,
    /// Rows used by this refresh.
    pub r_newcv: i32,
}

// [spec:libedit:def:refresh.re-printstr-fn]
// [spec:libedit:sem:refresh.re-printstr-fn]
/// The C's `f` and `t` delimit a half-open range of one screen row; the
/// range is the argument here, so the pair collapses to a single slice.
/// Debug-only in the C (`DEBUG_REFRESH`), and dead unless a port wires it
/// to tracing.
fn re_printstr(el: &mut EditLine, str: &str, f: &[u32]) {
    todo!()
}

// [spec:libedit:def:refresh.re-nextline-fn]
// [spec:libedit:sem:refresh.re-nextline-fn]
fn re_nextline(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:refresh.re-addc-fn]
// [spec:libedit:sem:refresh.re-addc-fn]
fn re_addc(el: &mut EditLine, c: u32) {
    todo!()
}

// [spec:libedit:def:refresh.re-putliteral-fn]
// [spec:libedit:sem:refresh.re-putliteral-fn]
/// C: `begin` and `end` are two pointers into the prompt string the
/// application's callback returned. The string is not part of `el`, so it
/// stays a borrowed slice; `end` indexes it.
///
/// The pair cannot collapse to one half-open slice, because
/// [`crate::literal::literal_add`] reads *past* `end` — `buf[end]` is the
/// closing delimiter and `buf[end + 1]` is the visible character glued to the
/// literal, whose width decides whether the literal is kept at all. So `buf`
/// must extend at least to `end + 1`, and the caller keeps the delimiter and
/// the glued character inside the slice it passes.
pub(crate) fn re_putliteral(el: &mut EditLine, buf: &[u32], end: usize) {
    todo!()
}

// [spec:libedit:def:refresh.re-putc-fn]
// [spec:libedit:sem:refresh.re-putc-fn]
pub(crate) fn re_putc(el: &mut EditLine, c: u32, shift: i32) {
    todo!()
}

// [spec:libedit:def:refresh.re-refresh-fn]
// [spec:libedit:sem:refresh.re-refresh-fn]
pub(crate) fn re_refresh(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:refresh.re-goto-bottom-fn]
// [spec:libedit:sem:refresh.re-goto-bottom-fn]
pub(crate) fn re_goto_bottom(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:refresh.re-insert-fn]
// [spec:libedit:sem:refresh.re-insert-fn]
/// `d` is the `el_display` row being edited and `s` a position within the
/// matching `el_vdisplay` row; both are borrowed out of the caller's
/// `EditLine`, so the C's `el` parameter — unused outside debug builds —
/// cannot be passed alongside them and is dropped.
fn re_insert(d: &mut [u32], dat: i32, dlen: i32, s: &[u32], num: i32) {
    todo!()
}

// [spec:libedit:def:refresh.re-delete-fn]
// [spec:libedit:sem:refresh.re-delete-fn]
/// `d` and the dropped `el` parameter are as in [`re_insert`].
fn re_delete(d: &mut [u32], dat: i32, dlen: i32, num: i32) {
    todo!()
}

// [spec:libedit:def:refresh.re-strncopy-fn]
// [spec:libedit:sem:refresh.re-strncopy-fn]
/// `a` is a position in an `el_display` row, `b` the matching position in
/// an `el_vdisplay` row. Different fields of the same `EditLine`, so the
/// two borrows coexist.
fn re__strncopy(a: &mut [u32], b: &[u32], n: usize) {
    todo!()
}

// [spec:libedit:def:refresh.re-clear-eol-fn]
// [spec:libedit:sem:refresh.re-clear-eol-fn]
fn re_clear_eol(el: &mut EditLine, fx: i32, sx: i32, diff: i32) {
    todo!()
}

// [spec:libedit:def:refresh.re-update-line-fn]
// [spec:libedit:sem:refresh.re-update-line-fn]
/// C: `wchar_t *old, wchar_t *new` — row pointers, always `el_display[i]`
/// and `el_vdisplay[i]`. They are row indices here, since the function also
/// needs `el` for the terminal calls and so cannot hold the rows borrowed
/// across them. All three arguments carry the same value, exactly as in
/// the C.
fn re_update_line(el: &mut EditLine, old: usize, new: usize, i: i32) {
    todo!()
}

// [spec:libedit:def:refresh.re-copy-and-pad-fn]
// [spec:libedit:sem:refresh.re-copy-and-pad-fn]
fn re__copy_and_pad(dst: &mut [u32], src: &[u32], width: usize) {
    todo!()
}

// [spec:libedit:def:refresh.re-refresh-cursor-fn]
// [spec:libedit:sem:refresh.re-refresh-cursor-fn]
pub(crate) fn re_refresh_cursor(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:refresh.re-fastputc-fn]
// [spec:libedit:sem:refresh.re-fastputc-fn]
fn re_fastputc(el: &mut EditLine, c: u32) {
    todo!()
}

// [spec:libedit:def:refresh.re-fastaddc-fn]
// [spec:libedit:sem:refresh.re-fastaddc-fn]
pub(crate) fn re_fastaddc(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:refresh.re-clear-display-fn]
// [spec:libedit:sem:refresh.re-clear-display-fn]
pub(crate) fn re_clear_display(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:refresh.re-clear-lines-fn]
// [spec:libedit:sem:refresh.re-clear-lines-fn]
pub(crate) fn re_clear_lines(el: &mut EditLine) {
    todo!()
}
