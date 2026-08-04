//! Ported from `src/search.c`; rules live in `docs/spec/port/src/search.md`.

// Every function below is a signature with a `todo!()` body, so no parameter
// is read yet. Remove this once the translations land.
#![allow(unused_variables)]

use core::ffi::c_char;

use crate::el::{EditLine, ElActionT};

// [spec:libedit:def:search.el-search-t]
/// Incremental- and character-search state.
pub struct ElSearchT {
    /// C: `wchar_t *patbuf` — the pattern buffer, owned.
    pub patbuf: Vec<u32>,
    /// C: `size_t patlen` — length of the pattern currently in `patbuf`,
    /// which is not the allocation size.
    pub patlen: usize,
    /// Direction of the last search.
    pub patdir: i32,
    /// Character search direction.
    pub chadir: i32,
    /// C: `wchar_t chacha` — the character we are looking for.
    pub chacha: u32,
    /// C: `char chatflg` — 0 if `f`, 1 if `t`. A byte-sized flag in the C,
    /// kept as one.
    pub chatflg: u8,
}

// [spec:libedit:def:search.search-init-fn]
// [spec:libedit:sem:search.search-init-fn]
/// C: `libedit_private int search_init(EditLine *el)`
pub(crate) fn search_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:search.search-end-fn]
// [spec:libedit:sem:search.search-end-fn]
/// C: `libedit_private void search_end(EditLine *el)`
pub(crate) fn search_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:search.regerror-fn]
// [spec:libedit:sem:search.regerror-fn]
/// C: `void regerror(const char *msg)`
///
/// Nothing is ported for this rule. The definition sits inside `#ifdef
/// REGEXP`, `src/sys.h` undefines `REGEXP` in favour of `REGEX`, and
/// `plan/decisions/posix-only-scope.md` puts the BSD `regexp` branch out of
/// scope — so this is never reached and has no caller. It is private, and
/// spelled out only so the rule has a home; the POSIX branch swallows a bad
/// pattern in `el_match` instead.
fn regerror(msg: *const c_char) {
    todo!()
}

// [spec:libedit:def:search.el-match-fn]
// [spec:libedit:sem:search.el-match-fn]
/// C: `libedit_private int el_match(const wchar_t *str, const wchar_t *pat)`
///
/// Both arguments are NUL-terminated wide strings the caller owns: `str` is a
/// history entry's `ev.str` and `pat` is `el_search.patbuf`, so neither is a
/// slice at the call sites.
pub(crate) fn el_match(str: *const u32, pat: *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:search.c-hmatch-fn]
// [spec:libedit:sem:search.c-hmatch-fn]
/// C: `libedit_private int c_hmatch(EditLine *el, const wchar_t *str)`
pub(crate) fn c_hmatch(el: &mut EditLine, str: *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:search.c-setpat-fn]
// [spec:libedit:sem:search.c-setpat-fn]
/// C: `libedit_private void c_setpat(EditLine *el)`
pub(crate) fn c_setpat(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:search.ce-inc-search-fn]
// [spec:libedit:sem:search.ce-inc-search-fn]
/// C: `libedit_private el_action_t ce_inc_search(EditLine *el, int dir)`
pub(crate) fn ce_inc_search(el: &mut EditLine, dir: i32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:search.cv-search-fn]
// [spec:libedit:sem:search.cv-search-fn]
/// C: `libedit_private el_action_t cv_search(EditLine *el, int dir)`
pub(crate) fn cv_search(el: &mut EditLine, dir: i32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:search.ce-search-line-fn]
// [spec:libedit:sem:search.ce-search-line-fn]
/// C: `libedit_private el_action_t ce_search_line(EditLine *el, int dir)`
pub(crate) fn ce_search_line(el: &mut EditLine, dir: i32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:search.cv-repeat-srch-fn]
// [spec:libedit:sem:search.cv-repeat-srch-fn]
/// C: `libedit_private el_action_t cv_repeat_srch(EditLine *el, wint_t c)`
pub(crate) fn cv_repeat_srch(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:search.cv-csearch-fn]
// [spec:libedit:sem:search.cv-csearch-fn]
/// C: `libedit_private el_action_t cv_csearch(EditLine *el, int direction,
/// wint_t ch, int count, int tflag)`
pub(crate) fn cv_csearch(
    el: &mut EditLine,
    direction: i32,
    ch: u32,
    count: i32,
    tflag: i32,
) -> ElActionT {
    todo!()
}
