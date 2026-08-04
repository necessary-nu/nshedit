//! Ported from `src/prompt.c`; rules live in `docs/spec/port/src/prompt.md`.

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
