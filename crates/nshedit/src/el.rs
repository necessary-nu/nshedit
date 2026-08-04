//! Ported from `src/el.c`; rules live in `docs/spec/port/src/el.md`.

// Every function body below is still `todo!()`, so every parameter is unused.
// Remove this once the bodies land.
#![allow(unused_variables)]

use core::ffi::{c_char, c_void};
use std::ffi::OsString;
use std::path::Path;

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

// [spec:libedit:def:el.secure-getenv-fn]
// [spec:libedit:sem:el.secure-getenv-fn]
/// C: `char *secure_getenv(char const *name)`.
///
/// The default environment hook. Returns the variable's value, or `None`
/// when the process gained privilege from its executable — real and
/// effective uid differ, real and effective gid differ, or the loader marked
/// the process secure. That denial is what stops a set-uid program from
/// being fed an `.editrc`, a terminal description or an editor of the
/// attacker's choosing; it is the reason this, and not a bare environment
/// lookup, is what [`el_init_internal`] installs.
///
/// `OsString` rather than `String`: POSIX environment values are bytes, and
/// three of the four names libedit looks up (`EDITRC`, `HOME`, `EDITOR`)
/// yield paths or command lines that need not be UTF-8.
///
/// The C's degenerate build — no `secure_getenv`, no `__secure_getenv`, no
/// `issetugid`, so `issetugid()` is `1` and libedit reads no environment at
/// all — is a portability artefact and is deliberately not reproduced.
///
/// Note this does not have the shape of [`FuncT`]: an application-supplied
/// hook hands back a borrowed `char *` across the C ABI, whereas the
/// built-in default owns what it returns. The two are reconciled where the
/// hook is consulted, not here.
pub fn secure_getenv(name: &str) -> Option<OsString> {
    todo!()
}

// [spec:libedit:def:el.el-init-fn]
// [spec:libedit:sem:el.el-init-fn]
/// C: `EditLine *el_init(const char *prog, FILE *fin, FILE *fout, FILE *ferr)`.
///
/// Constructs an editor from three streams, deriving the three descriptors
/// from them. One tail call to [`el_init_fd`] with `fileno` of each stream;
/// `None` is the C's NULL return.
///
/// `prog` is a `&str` because the C's NULL and undecodable cases are
/// undefined behaviour rather than behaviour to reproduce. The streams stay
/// [`CFile`]: they are borrowed for the whole lifetime of the editor, never
/// duplicated and never closed, so there is nothing here to own.
///
/// The C's failure reporting is weak in ways the body must keep — a `fileno`
/// of -1 is stored undiagnosed and construction still reports success — and
/// weaker still further in; see [`el_init_internal`].
pub fn el_init(prog: &str, fin: CFile, fout: CFile, ferr: CFile) -> Option<Box<EditLine>> {
    todo!()
}

// [spec:libedit:def:el.el-init-internal-fn]
// [spec:libedit:sem:el.el-init-internal-fn]
/// C: `libedit_private EditLine *el_init_internal(const char *prog, FILE *fin,
/// FILE *fout, FILE *ferr, int fdin, int fdout, int fderr, int flags)`.
///
/// The real constructor: records the streams and descriptors, copies the
/// program name, and brings up every subsystem in the order the C marks
/// "Order is important!!!". `flags` is the initial `el_flags` word and is
/// stored before any subsystem init, so that `terminal_init` raising
/// `EDIT_DISABLED` and `tty_init` raising `NO_TTY` survive.
///
/// `Option<Box<EditLine>>` is the C's `EditLine *`/NULL, and the return type
/// is deliberately no stronger than that. `sem:el.el-init-fn` records that
/// the C reports success while handing back an object whose subsystems
/// silently failed to allocate, and that the one path documented as
/// returning NULL faults instead; reproducing and then correcting that is
/// the body's job and idiomatization's, not the signature's.
// Eight parameters because the C has eight; collapsing them into a builder or
// a stream triple is idiomatization's call, not the translation's.
#[allow(clippy::too_many_arguments)]
pub(crate) fn el_init_internal(
    prog: &str,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
    fdin: i32,
    fdout: i32,
    fderr: i32,
    flags: i32,
) -> Option<Box<EditLine>> {
    todo!()
}

// [spec:libedit:def:el.el-init-fd-fn]
// [spec:libedit:sem:el.el-init-fd-fn]
/// C: `EditLine *el_init_fd(const char *prog, FILE *fin, FILE *fout,
/// FILE *ferr, int fdin, int fdout, int fderr)`.
///
/// Constructs an editor from three streams plus three explicitly supplied
/// descriptors, for callers whose streams and descriptors are not related by
/// `fileno`. One tail call to [`el_init_internal`] with `flags` of 0.
///
/// Nothing is validated and no consistency is enforced between the streams
/// and the descriptors: reads and writes go through the streams while
/// terminal size and mode queries go through the descriptors, whatever they
/// happen to refer to.
pub fn el_init_fd(
    prog: &str,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
    fdin: i32,
    fdout: i32,
    fderr: i32,
) -> Option<Box<EditLine>> {
    todo!()
}

// [spec:libedit:def:el.el-end-fn]
// [spec:libedit:sem:el.el-end-fn]
/// C: `void el_end(EditLine *el)`.
///
/// Destroys an editor: [`el_reset`] first, so the terminal is restored while
/// the tty subsystem is still live, then every subsystem torn down in the
/// forward order of [`el_init_internal`]'s step 6 — *not* its reverse, and
/// not the order a derived `Drop` would give.
///
/// Consumes the editor, which is why it takes it by value. `None` is the C's
/// NULL argument, the one NULL-tolerant entry point in this file. The three
/// streams and three descriptors are borrowed, so they are neither flushed
/// nor closed; `el_data`, the `el_getenv` hook, and every callback installed
/// through `EL_PROMPT`, `EL_RESIZE`, `EL_ALIAS_TEXT`, `EL_HIST` and
/// `EL_GETCFN` are dropped without notification.
pub fn el_end(el: Option<Box<EditLine>>) {
    todo!()
}

// [spec:libedit:def:el.el-reset-fn]
// [spec:libedit:sem:el.el-reset-fn]
/// C: `void el_reset(EditLine *el)`.
///
/// Abandons the line being edited and puts the terminal back the way the
/// application had it: `tty_cookedmode` then `ch_reset`, in that order,
/// because the cooked-mode switch is the last chance to drain output written
/// for the line about to be discarded. Both results are discarded, so a
/// failure to restore the modes is silent.
pub fn el_reset(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:el.el-source-fn]
// [spec:libedit:sem:el.el-source-fn]
/// C: `int el_source(EditLine *el, const char *fname)`.
///
/// Reads an `.editrc` and runs each line through `parse_line`. `None` for
/// `fname` is the C's NULL: the file is then `EDITRC` from the environment
/// hook, or `$HOME/.editrc`, or — when `HOME` is the empty string — the
/// relative path `.editrc`. Despite what `histedit.h` claims, there is no
/// `$PWD` lookup.
///
/// The `i32` is the C's status and keeps its oddities: 0 for success or for
/// a file that executed nothing, +1 when the *last* executed builtin failed
/// (`el_wparse` negates), and -1 for an unknown command, a line that
/// tokenised to zero words (a whitespace-only line does exactly this, and
/// aborts the rest of the file), or any of the early exits. There is no way
/// for the caller to tell "could not open" from "a line failed".
pub fn el_source(el: &mut EditLine, fname: Option<&Path>) -> i32 {
    todo!()
}

// [spec:libedit:def:el.el-resize-fn]
// [spec:libedit:sem:el.el-resize-fn]
/// C: `void el_resize(EditLine *el)`.
///
/// Re-reads the terminal window size and rebuilds the display buffers if it
/// changed, with `SIGWINCH` blocked across the whole operation and the
/// caller's previous mask restored with `SIG_SETMASK` rather than
/// `SIG_UNBLOCK`. Blocking does not discard the signal, so resize events are
/// not lost; the block only makes query-and-rebuild atomic against libedit's
/// own handler.
///
/// The size query goes to the *input* descriptor. Every result on the path —
/// the mask calls, and `terminal_change_size`'s -1 — is discarded, so an
/// out-of-memory during a resize is silent.
pub fn el_resize(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:el.el-beep-fn]
// [spec:libedit:sem:el.el-beep-fn]
/// C: `void el_beep(EditLine *el)`.
///
/// Rings the terminal bell — a public re-export of `terminal_beep` with no
/// added logic. Reports no errors, does not flush, and does not touch the
/// cursor or any editing state.
pub fn el_beep(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:el.el-editmode-fn]
// [spec:libedit:sem:el.el-editmode-fn]
/// C: `libedit_private int el_editmode(EditLine *el, int argc,
/// const wchar_t **argv)`.
///
/// The `edit` builtin, dispatched from `.editrc` or `el_parse`, so
/// `argv[0]` is `edit` and `argv[1]` is the single required operand. `on`
/// clears `EDIT_DISABLED` and *then* calls `tty_rawmode`; `off` calls
/// `tty_cookedmode` and *then* sets the flag. Both orders are load-bearing —
/// each tty call returns immediately while `EDIT_DISABLED` is set — and both
/// tty results are discarded, so a failed mode change still reports success
/// and leaves `el_flags` out of step with the terminal.
///
/// The C's `argc` is dropped: its three rejections (`argv` NULL, `argc != 2`,
/// `argv[1]` NULL) all return -1 with no message and no state change, so
/// `argv.len() != 2` is observationally the same test. The wide operand is
/// `&[u32]` per the crate's `wchar_t` convention, compared for exact
/// equality — case-sensitive, no abbreviations.
pub(crate) fn el_editmode(el: &mut EditLine, argv: &[&[u32]]) -> i32 {
    todo!()
}
