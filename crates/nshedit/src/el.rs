//! Ported from `src/el.c`; rules live in `docs/spec/port/src/el.md`.

use core::ffi::{c_char, c_void};

use crate::chared::ElCharedT;
use crate::chartype::CtBufferT;
use crate::hist::ElHistoryT;
use crate::histedit::LineInfo;
use crate::keymacro::ElKeymacroT;
use crate::literal::ElLiteralT;
use crate::map::ElMapT;
use crate::prompt::ElPromptT;
use crate::read::ElReadT;
use crate::refresh::ElRefreshT;
use crate::search::ElSearchT;
use crate::sig::ElSignalT;
use crate::terminal::ElTerminalT;
use crate::tty::ElTtyT;

/// Stand-in for the C's `FILE *`.
///
/// The three streams an `EditLine` holds are caller-owned: libedit never
/// closes or frees them (`sem:histedit.el-end-fn`), and
/// `EL_GETFP`/`EL_SETFP` round-trip them through the C ABI unchanged, so
/// there is nothing here to own and no Rust handle that would survive the
/// trip. Actual I/O should go through the matching `el_infd`/`el_outfd`/
/// `el_errfd` descriptor, which is carried alongside precisely because the C
/// already keeps both.
pub type CFile = *mut c_void;

// [spec:libedit:def:el.func-t-const-char]
/// C: `typedef char * (*func_t)(const char *);`
///
/// The type `el_set`/`el_get` cast `EL_GETENV`'s argument through. Same
/// shape as the `el_getenv` member of [`EditLine`]; `el.c` declares it
/// separately only because the member is spelled out inline there.
pub type FuncT = fn(*const c_char) -> *mut c_char;

// [spec:libedit:def:el.el-action-t]
/// C: `typedef unsigned char el_action_t;` — index into the command array.
pub type ElActionT = u8;

// [spec:libedit:def:el.coord-t]
/// Position on the screen.
///
/// Note that `el_terminal.t_size` stores columns in `v` and lines in `h`,
/// the reverse of what the names suggest. That is the C's doing, not a slip
/// here; see `sem:terminal.terminal-set-fn`.
pub struct CoordT {
    pub h: i32,
    pub v: i32,
}

// [spec:libedit:def:el.el-line-t]
/// The current line.
///
/// The C is four pointers, three of them into the allocation the first one
/// owns. They are offsets here: `ch_enlargebufs` rebases them by reading the
/// old pointer values *after* the `realloc` that invalidated them, which
/// `sem:chared.ch-enlargebufs-fn` calls out as undefined
/// behaviour the port must not inherit.
///
/// Field order is load-bearing and must not change: `el_wline` hands out a
/// `LineInfoW` view onto this struct, and
/// `sem:el.el-wline-fn` requires that to be a genuine
/// borrowed view of the first three members.
pub struct ElLineT {
    /// C: `wchar_t *buffer` — the input line, owned.
    pub buffer: Vec<u32>,
    /// C: `wchar_t *cursor` — offset into `buffer`.
    pub cursor: usize,
    /// C: `wchar_t *lastchar` — offset into `buffer`, one past the last
    /// character. The buffer is not NUL-terminated there.
    pub lastchar: usize,
    /// C: `const wchar_t *limit` — offset into `buffer` of the highest
    /// writable position, `buffer.len() - EL_LEAVE` once published.
    pub limit: usize,
}

// [spec:libedit:def:el.el-state-t]
/// Editor state.
pub struct ElStateT {
    /// What mode are we in?
    pub inputmode: i32,
    /// Are we getting an argument?
    pub doingarg: i32,
    /// Numeric argument.
    pub argument: i32,
    /// Is the next char a meta char?
    pub metanext: i32,
    /// Previous command.
    pub lastcmd: ElActionT,
    /// This command.
    pub thiscmd: ElActionT,
    /// C: `wchar_t thisch` — char that generated it.
    pub thisch: u32,
}

// [spec:libedit:def:el.editline]
/// The editor. `histedit.h` names it `EditLine`; see
/// `def:histedit.edit-line`.
pub struct EditLine {
    /// C: `wchar_t *el_prog` — the program name, owned.
    pub el_prog: Vec<u32>,
    /// C: `FILE *el_infile` — caller-owned, never closed.
    pub el_infile: CFile,
    /// C: `FILE *el_outfile` — caller-owned, never closed.
    pub el_outfile: CFile,
    /// C: `FILE *el_errfile` — caller-owned, never closed.
    pub el_errfile: CFile,
    /// Input file descriptor.
    pub el_infd: i32,
    /// Output file descriptor.
    pub el_outfd: i32,
    /// Error file descriptor.
    pub el_errfd: i32,
    /// Various flags.
    pub el_flags: i32,
    /// Cursor location.
    pub el_cursor: CoordT,
    /// C: `wint_t **el_display` — the real screen image, one owned row per
    /// screen line. `u32` and not `char`: rows carry `MB_FILL_CHAR` and the
    /// `EL_LITERAL` sentinel (bit 31), neither of them a Unicode scalar
    /// value. See `sem:literal.literal-add-fn`.
    pub el_display: Vec<Vec<u32>>,
    /// C: `wint_t **el_vdisplay` — the virtual screen image, same shape.
    pub el_vdisplay: Vec<Vec<u32>>,
    /// C: `void *el_data` — client data, stored and handed back untouched.
    pub el_data: *mut c_void,
    /// The current line information.
    pub el_line: ElLineT,
    /// Current editor state.
    pub el_state: ElStateT,
    /// Terminal dependent stuff.
    pub el_terminal: ElTerminalT,
    /// Tty dependent stuff.
    pub el_tty: ElTtyT,
    /// Refresh stuff.
    pub el_refresh: ElRefreshT,
    /// Prompt stuff.
    pub el_prompt: ElPromptT,
    /// Right-hand prompt stuff.
    pub el_rprompt: ElPromptT,
    /// Prompt literal bits.
    pub el_literal: ElLiteralT,
    /// Character editor stuff.
    pub el_chared: ElCharedT,
    /// Key mapping stuff.
    pub el_map: ElMapT,
    /// Key binding stuff.
    pub el_keymacro: ElKeymacroT,
    /// History stuff.
    pub el_history: ElHistoryT,
    /// Search stuff.
    pub el_search: ElSearchT,
    /// Signal handling stuff.
    pub el_signal: ElSignalT,
    /// C: `struct el_read_t *el_read` — character reading stuff, owned.
    /// `None` before `read_init` and after `read_end`.
    pub el_read: Option<Box<ElReadT>>,
    /// Buffer for displayable strings.
    pub el_visual: CtBufferT,
    /// Scratch conversion buffer.
    pub el_scratch: CtBufferT,
    /// Buffer for legacy wrappers.
    pub el_lgcyconv: CtBufferT,
    /// Legacy `LineInfo` buffer.
    pub el_lgcylinfo: LineInfo,
    // [spec:libedit:def:el.editline.el-getenv-fn]
    /// C: `char *(*el_getenv)(const char *)` — the environment-lookup hook.
    /// Defaults to `secure_getenv`, which is load-bearing for set-uid
    /// processes; `el_set(EL_GETENV, fn)` replaces it with no NULL check, so
    /// this stays nullable. `sem:el.editline.el-getenv-fn`
    /// lists the four lookups that must route through it.
    pub el_getenv: Option<FuncT>,
}
