//! Ported from `src/prompt.c`; rules live in `docs/spec/port/src/prompt.md`.

// Every function body below is still `todo!()`, so no parameter is read yet.
// Remove this once the function translations land.
#![allow(unused_variables)]

use crate::el::{CoordT, EditLine};

// [spec:libedit:def:prompt.el-pfunc-t-edit-line]
/// C: `typedef wchar_t *(*el_pfunc_t)(EditLine *);`
///
/// The prompt hook installed by `EL_PROMPT`/`EL_RPROMPT`. It returns a
/// NUL-terminated wide string libedit borrows and does not free — the
/// application owns the storage — so the return stays a raw pointer, as in
/// the C.
pub type ElPfuncT = fn(&mut EditLine) -> *const u32;

// [spec:libedit:def:prompt.el-prompt-t]
/// One prompt (left or right) and where it left the cursor.
pub struct ElPromptT {
    /// Function to return the prompt.
    pub p_func: Option<ElPfuncT>,
    /// Position in the line after the prompt.
    pub p_pos: CoordT,
    /// C: `wchar_t p_ignore` — character that starts and ends a literal
    /// run. 0 means "no literal marker"; see
    /// `sem:prompt.prompt-print-fn`.
    pub p_ignore: u32,
    pub p_wide: i32,
}

// [spec:libedit:def:prompt.prompt-default-fn]
// [spec:libedit:sem:prompt.prompt-default-fn]
/// Signature is fixed by [`ElPfuncT`]: this is installed as a prompt
/// callback, so it returns the borrowed raw pointer that type demands.
fn prompt_default(el: &mut EditLine) -> *const u32 {
    todo!()
}

// [spec:libedit:def:prompt.prompt-default-r-fn]
// [spec:libedit:sem:prompt.prompt-default-r-fn]
/// Signature is fixed by [`ElPfuncT`], as for [`prompt_default`].
fn prompt_default_r(el: &mut EditLine) -> *const u32 {
    todo!()
}

// [spec:libedit:def:prompt.prompt-print-fn]
// [spec:libedit:sem:prompt.prompt-print-fn]
pub(crate) fn prompt_print(el: &mut EditLine, op: i32) {
    todo!()
}

// [spec:libedit:def:prompt.prompt-init-fn]
// [spec:libedit:sem:prompt.prompt-init-fn]
pub(crate) fn prompt_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:prompt.prompt-end-fn]
// [spec:libedit:sem:prompt.prompt-end-fn]
pub(crate) fn prompt_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:prompt.prompt-set-fn]
// [spec:libedit:sem:prompt.prompt-set-fn]
/// `prf` is optional because a NULL function is the documented way to ask
/// for the built-in default back.
pub(crate) fn prompt_set(
    el: &mut EditLine,
    prf: Option<ElPfuncT>,
    c: u32,
    op: i32,
    wide: i32,
) -> i32 {
    todo!()
}

// [spec:libedit:def:prompt.prompt-get-fn]
// [spec:libedit:sem:prompt.prompt-get-fn]
/// Both out-parameters keep the C's nullability: a NULL `prf` is the one
/// failure path, and a NULL `c` simply skips the escape-character store.
pub(crate) fn prompt_get(
    el: &mut EditLine,
    prf: Option<&mut Option<ElPfuncT>>,
    c: Option<&mut u32>,
    op: i32,
) -> i32 {
    todo!()
}
