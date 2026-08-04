//! Ported from `src/chared.c`; rules live in `docs/spec/port/src/chared.md`.

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
