//! The readline compatibility surface; rules in `docs/spec/port/src/readline.md`
//! and `docs/spec/port/src/editline/readline.md`.
//!
//! `readline.c` is a compatibility layer in the C itself, so this is its
//! faithful placement rather than an early idiomatization — see
//! `plan/decisions/idiomatic-core.md`. Everything the GNU readline API
//! exposes and Rust would never choose lives here: the exported mutable
//! statics, the process-global editor and history pair, and the returned
//! pointers that stay valid exactly until the next call.
//!
//! Functions that are `static` in `readline.c` stay private plain Rust
//! functions with no `#[unsafe(no_mangle)]` and no `extern "C"`: they are
//! not part of the ABI, only of the implementation.
//!
//! # Memory a C caller frees
//!
//! `readline.c`'s `el_malloc`/`el_calloc`/`el_free` are plain
//! `malloc`/`calloc`/`free`, and its documented contract is that the caller
//! releases the returned block with `free()`. Every such block is allocated
//! here through [`std::alloc::System`], which *is* `malloc`/`free` on this
//! port's POSIX target — see [`c_alloc`]. State the C keeps in file-statics
//! and never hands out (the last search pattern, the `history_list` arrays,
//! the completion scan) is ordinary owned Rust instead, so it is freed
//! properly rather than leaked; `docs/errata.md` ERR-readline-17,
//! ERR-readline-18, ERR-readline-20 and ERR-readline-21 all record that
//! those leaks are not observable.
//!
//! # What this cannot yet do
//!
//! Blockers, all of them outside this file, each marked at the site:
//!
//! - **`libedit_private` entry points `readline.c` calls directly are
//!   `pub(crate)` in the core**: `tty_init`, `tty_end`,
//!   `tty_get_signal_character`, `re_putc`, `em_kill_line` and
//!   `el_init_internal` (which is the only way to ask for `NO_RESET`). The
//!   stand-ins are gathered under "Core gaps" below.
//! - **`ioctl(FIONREAD)`**, the poll in `_rl_event_read_char`.
//!   `plan/decisions/platform-layer.md` defers whether to supply it: two
//!   rules want it and the other does not compile in the C on glibc, so this
//!   is the only real obligation and it is still open. `tcgetattr` (the
//!   `ECHO` test in `rl_initialize`) and `raise(SIGTSTP)` (`_el_rl_tstp`) are
//!   no longer gaps — both go through `nshedit-plat`.
//! - **No `stdin`/`stdout`/`stderr`.** `fileno` is available now — see
//!   [`crate::cstdio`], the third site on `plan/decisions/no-c-ffi.md`'s
//!   enumeration — so an application's own `rl_instream`/`rl_outstream`
//!   yields its own descriptors. The three standard streams are data objects
//!   rather than functions and are not on that enumeration, so a `NULL`
//!   `rl_instream`/`rl_outstream` still means "the standard descriptor" here
//!   rather than being rewritten to the C stream object.
//! - **The passwd database** is NSS through `nshedit-plat`, shared with the
//!   core's `fn_tilde_expand`. The `/etc/passwd` parser this file used to
//!   carry is deleted.

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int, c_uchar, c_ulong, c_void};
use core::ptr;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::{Cell, RefCell};
use std::io::{Read, Seek, SeekFrom, Write};

use nshedit::chartype::{CtBufferT, ct_decode_string};
use nshedit::editline::readline::{HistEntry, HistdataT, HistoryState, Keymap, KeymapEntry};
use nshedit::el::{CFile, EditLine};
use nshedit::filecomplete::{self, FilenameCompletionState};
use nshedit::histedit::{
    CC_EOF, CC_ERROR, CC_NORM, CC_REFRESH, H_CLEAR, H_CURR, H_DELDATA, H_ENTER, H_FIRST, H_GETSIZE,
    H_LAST, H_LOAD, H_NEXT, H_NEXT_EVDATA, H_NEXT_EVENT, H_NEXT_STR, H_PREV, H_PREV_EVENT,
    H_PREV_STR, H_REPLACE, H_SAVE, H_SET, H_SETSIZE, HistEvent, History,
};
use nshedit::tty::{C_EOF, C_REPRINT, TS_IO};
use std::os::fd::AsRawFd;

use crate::cstdio;

/// C: `rl_command_func_t *` — the application's keystroke handler.
///
/// Spelled out here rather than taken from
/// `nshedit::editline::readline::RlCommandFuncT`, which is a *Rust*-ABI `fn`
/// and so cannot cross an `extern "C"` boundary. Collapse the two when the
/// header types move into this crate.
type RlCommandFunc = unsafe extern "C" fn(c_int, c_int) -> c_int;

// ---------------------------------------------------------------------------
// Constants `readline.c` gets from headers this crate does not own yet.
//
// `histedit.h`'s `EL_*` and `H_*`, `editline/readline.h`'s `RL_*`, `tty.h`'s
// control-character indices, `fcns.h`'s action numbers and `vis.h`'s flags are
// all frozen ABI. The `H_*` and `CC_*` sets are `pub` in the core and imported
// above; the rest are `pub(crate)` there or have no Rust home at all, so they
// are spelled out here with their C values.
// ---------------------------------------------------------------------------

/// C: `#define EL_TERMINAL 1`.
const EL_TERMINAL: c_int = 1;
/// C: `#define EL_EDITOR 2`.
const EL_EDITOR: c_int = 2;
/// C: `#define EL_SIGNAL 3`.
const EL_SIGNAL: c_int = 3;
/// C: `#define EL_BIND 4`.
const EL_BIND: c_int = 4;
/// C: `#define EL_SETTC 6`.
const EL_SETTC: c_int = 6;
/// C: `#define EL_ADDFN 9`.
const EL_ADDFN: c_int = 9;
/// C: `#define EL_HIST 10`.
const EL_HIST: c_int = 10;
/// C: `#define EL_EDITMODE 11`.
const EL_EDITMODE: c_int = 11;
/// C: `#define EL_GETCFN 13`.
const EL_GETCFN: c_int = 13;
/// C: `#define EL_UNBUFFERED 15`.
const EL_UNBUFFERED: c_int = 15;
/// C: `#define EL_PREP_TERM 16`.
const EL_PREP_TERM: c_int = 16;
/// C: `#define EL_GETTC 17`.
const EL_GETTC: c_int = 17;
/// C: `#define EL_REFRESH 20`.
const EL_REFRESH: c_int = 20;
/// C: `#define EL_PROMPT_ESC 21`.
const EL_PROMPT_ESC: c_int = 21;
/// C: `#define EL_RESIZE 23`.
const EL_RESIZE: c_int = 23;
/// C: `#define EL_BUILTIN_GETCFN (NULL)`.
const EL_BUILTIN_GETCFN: *const c_void = ptr::null();

/// C: `#define RL_READLINE_VERSION 0x0402`.
pub const RL_READLINE_VERSION: c_int = 0x0402;
/// C: `#define RL_PROMPT_START_IGNORE '\1'`.
pub const RL_PROMPT_START_IGNORE: u8 = 1;
/// C: `#define RL_PROMPT_END_IGNORE '\2'`.
pub const RL_PROMPT_END_IGNORE: u8 = 2;
/// C: `#define RL_STATE_NONE 0x000000`.
pub const RL_STATE_NONE: c_ulong = 0;
/// C: `#define RL_STATE_DONE 0x000001`.
pub const RL_STATE_DONE: c_ulong = 1;

/// C: `#define ED_INSERT 9` (`fcns.h`, generated) — the self-insert action,
/// the one value `rl_bind_key` can install. The core's copy is `pub(crate)`.
const ED_INSERT: u8 = 9;

/// C: `#define TCSADRAIN 1` — `readline()`'s `tty_end` argument.
const TCSADRAIN: c_int = 1;

/// C: `#define VIS_WHITE (VIS_SP | VIS_TAB | VIS_NL)`, the `vis.h` flag pair
/// `rl_add_defun` renders a key byte with. The core's copies are
/// `pub(crate)`.
const VIS_WHITE: c_int = 0x0004 | 0x0008 | 0x0010;
/// C: `#define VIS_NOSLASH 0x0040`.
const VIS_NOSLASH: c_int = 0x0040;

/// C: `#define MAX_MESSAGE 160` — `rl_message`'s stack buffer.
const MAX_MESSAGE: usize = 160;

/// C: `EINVAL`, the fallback `read_history`/`write_history`/`append_history`
/// return when a failing history call left `errno` clear.
const EINVAL: c_int = 22;
/// C: `EAGAIN`, `history_truncate_file`'s "internal inconsistency" return.
const EAGAIN: c_int = 11;

// ---------------------------------------------------------------------------
// Exported mutable statics declared by `editline/readline.h`
//
// These are data symbols, not functions: the application writes them and
// libedit reads them. `plan/decisions/idiomatic-core.md` keeps them here and
// out of the core.
// ---------------------------------------------------------------------------

/// C: `static char empty[] = { '\0' };` — what `rl_readline_name` points at
/// until an application replaces it.
static EMPTY: [c_char; 1] = [0];

/// C: `static char expand_chars[] = { ' ', '\t', '\n', '=', '(', '\0' };`
static EXPAND_CHARS: [c_char; 6] = [b' ' as c_char, 9, 10, b'=' as c_char, b'(' as c_char, 0];

/// C: `static char break_chars[] = { ' ', '\t', '\n', '"', '\\', '\'', '`',
/// '@', '$', '>', '<', '=', ';', '|', '&', '{', '(', '\0' };`
static BREAK_CHARS: [c_char; 18] = [
    b' ' as c_char,
    9,
    10,
    b'"' as c_char,
    b'\\' as c_char,
    b'\'' as c_char,
    b'`' as c_char,
    b'@' as c_char,
    b'$' as c_char,
    b'>' as c_char,
    b'<' as c_char,
    b'=' as c_char,
    b';' as c_char,
    b'|' as c_char,
    b'&' as c_char,
    b'{' as c_char,
    b'(' as c_char,
    0,
];

/// C: `const char *rl_library_version = "EditLine wrapper";`
// [spec:libedit:def:readline.rl-library-version]
// [spec:libedit:sem:readline.rl-library-version]
#[unsafe(no_mangle)]
pub static mut rl_library_version: *const c_char = c"EditLine wrapper".as_ptr();

/// C: `int rl_readline_version = RL_READLINE_VERSION;`
// [spec:libedit:def:readline.rl-readline-version]
// [spec:libedit:sem:readline.rl-readline-version]
#[unsafe(no_mangle)]
pub static mut rl_readline_version: c_int = RL_READLINE_VERSION;

/// C: `const char *rl_readline_name = empty;` — the program name
/// `rl_initialize` hands `el_init_internal`, so `prog:` conditionals in
/// `.editrc` can select on it.
// [spec:libedit:def:readline.rl-readline-name]
// [spec:libedit:sem:readline.rl-readline-name]
#[unsafe(no_mangle)]
pub static mut rl_readline_name: *const c_char = EMPTY.as_ptr();

/// C: `FILE *rl_instream = NULL;` — NULL means the process's standard
/// input; see the module docs on the missing `FILE *` facility.
// [spec:libedit:def:readline.rl-instream]
// [spec:libedit:sem:readline.rl-instream]
#[unsafe(no_mangle)]
pub static mut rl_instream: CFile = ptr::null_mut();

/// C: `FILE *rl_outstream = NULL;` — NULL means the process's standard
/// output.
// [spec:libedit:def:readline.rl-outstream]
// [spec:libedit:sem:readline.rl-outstream]
#[unsafe(no_mangle)]
pub static mut rl_outstream: CFile = ptr::null_mut();

/// C: `int rl_point = 0;` — the cursor position in *bytes*, republished by
/// `_rl_update_pos`.
// [spec:libedit:def:readline.rl-point]
// [spec:libedit:sem:readline.rl-point]
#[unsafe(no_mangle)]
pub static mut rl_point: c_int = 0;

/// C: `int rl_end = 0;` — the line length in bytes.
// [spec:libedit:def:readline.rl-end]
// [spec:libedit:sem:readline.rl-end]
#[unsafe(no_mangle)]
pub static mut rl_end: c_int = 0;

/// C: `char *rl_line_buffer = NULL;` — borrowed; it points into EditLine's
/// legacy conversion buffer and must never be freed or resized.
// [spec:libedit:def:readline.rl-line-buffer]
// [spec:libedit:sem:readline.rl-line-buffer]
#[unsafe(no_mangle)]
pub static mut rl_line_buffer: *mut c_char = ptr::null_mut();

/// C: `rl_vcpfunc_t *rl_linefunc = NULL;` — the callback-mode line handler.
// [spec:libedit:def:readline.rl-linefunc]
// [spec:libedit:sem:readline.rl-linefunc]
#[unsafe(no_mangle)]
pub static mut rl_linefunc: Option<unsafe extern "C" fn(*mut c_char)> = None;

/// C: `int rl_done = 0;`
// [spec:libedit:def:readline.rl-done]
// [spec:libedit:sem:readline.rl-done]
#[unsafe(no_mangle)]
pub static mut rl_done: c_int = 0;

/// C: `rl_hook_func_t *rl_event_hook = NULL;`
// [spec:libedit:def:readline.rl-event-hook]
// [spec:libedit:sem:readline.rl-event-hook]
#[unsafe(no_mangle)]
pub static mut rl_event_hook: Option<unsafe extern "C" fn() -> c_int> = None;

/// C: `KEYMAP_ENTRY_ARRAY emacs_standard_keymap;` — zero-initialized and
/// never populated or consulted; it exists so programs referencing it link.
// [spec:libedit:def:readline.emacs-standard-keymap]
// [spec:libedit:sem:readline.emacs-standard-keymap]
#[unsafe(no_mangle)]
pub static mut emacs_standard_keymap: [KeymapEntry; 256] = [const {
    KeymapEntry {
        r#type: 0,
        function: None,
    }
}; 256];

/// C: `KEYMAP_ENTRY_ARRAY emacs_meta_keymap;` — likewise inert.
// [spec:libedit:def:readline.emacs-meta-keymap]
// [spec:libedit:sem:readline.emacs-meta-keymap]
#[unsafe(no_mangle)]
pub static mut emacs_meta_keymap: [KeymapEntry; 256] = [const {
    KeymapEntry {
        r#type: 0,
        function: None,
    }
}; 256];

/// C: `KEYMAP_ENTRY_ARRAY emacs_ctlx_keymap;` — likewise inert.
// [spec:libedit:def:readline.emacs-ctlx-keymap]
// [spec:libedit:sem:readline.emacs-ctlx-keymap]
#[unsafe(no_mangle)]
pub static mut emacs_ctlx_keymap: [KeymapEntry; 256] = [const {
    KeymapEntry {
        r#type: 0,
        function: None,
    }
}; 256];

/// C: `int rl_catch_signals = 1;` — read once, by `rl_initialize`, as the
/// `EL_SIGNAL` argument.
// [spec:libedit:def:readline.rl-catch-signals]
// [spec:libedit:sem:readline.rl-catch-signals]
#[unsafe(no_mangle)]
pub static mut rl_catch_signals: c_int = 1;

/// C: `int rl_catch_sigwinch = 1;` — exported but never consulted.
// [spec:libedit:def:readline.rl-catch-sigwinch]
// [spec:libedit:sem:readline.rl-catch-sigwinch]
#[unsafe(no_mangle)]
pub static mut rl_catch_sigwinch: c_int = 1;

/// C: `int history_base = 1;`
// [spec:libedit:def:readline.history-base]
// [spec:libedit:sem:readline.history-base]
#[unsafe(no_mangle)]
pub static mut history_base: c_int = 1;

/// C: `int history_length = 0;`
// [spec:libedit:def:readline.history-length]
// [spec:libedit:sem:readline.history-length]
#[unsafe(no_mangle)]
pub static mut history_length: c_int = 0;

/// C: `int history_offset = 0;`
// [spec:libedit:def:readline.history-offset]
// [spec:libedit:sem:readline.history-offset]
#[unsafe(no_mangle)]
pub static mut history_offset: c_int = 0;

/// C: `int max_input_history = 0;` — the mirror `history_is_stifled` reads;
/// note the initializer is 0, not `INT_MAX`.
// [spec:libedit:def:readline.max-input-history]
// [spec:libedit:sem:readline.max-input-history]
#[unsafe(no_mangle)]
pub static mut max_input_history: c_int = 0;

/// C: `char history_expansion_char = '!';`
// [spec:libedit:def:readline.history-expansion-char]
// [spec:libedit:sem:readline.history-expansion-char]
#[unsafe(no_mangle)]
pub static mut history_expansion_char: c_char = b'!' as c_char;

/// C: `char history_subst_char = '^';`
// [spec:libedit:def:readline.history-subst-char]
// [spec:libedit:sem:readline.history-subst-char]
#[unsafe(no_mangle)]
pub static mut history_subst_char: c_char = b'^' as c_char;

/// C: `char *history_no_expand_chars = expand_chars;`
// [spec:libedit:def:readline.history-no-expand-chars]
// [spec:libedit:sem:readline.history-no-expand-chars]
#[unsafe(no_mangle)]
pub static mut history_no_expand_chars: *mut c_char = EXPAND_CHARS.as_ptr().cast_mut();

/// C: `rl_linebuf_func_t *history_inhibit_expansion_function = NULL;`
// [spec:libedit:def:readline.history-inhibit-expansion-function]
// [spec:libedit:sem:readline.history-inhibit-expansion-function]
#[unsafe(no_mangle)]
pub static mut history_inhibit_expansion_function: Option<
    unsafe extern "C" fn(*const c_char, c_int) -> c_int,
> = None;

/// C: `int rl_inhibit_completion = 0;`
// [spec:libedit:def:readline.rl-inhibit-completion]
// [spec:libedit:sem:readline.rl-inhibit-completion]
#[unsafe(no_mangle)]
pub static mut rl_inhibit_completion: c_int = 0;

/// C: `int rl_attempted_completion_over = 0;`
// [spec:libedit:def:readline.rl-attempted-completion-over]
// [spec:libedit:sem:readline.rl-attempted-completion-over]
#[unsafe(no_mangle)]
pub static mut rl_attempted_completion_over: c_int = 0;

/// C: `const char *rl_basic_word_break_characters = break_chars;` — the only
/// word-break set `rl_complete` ever passes on.
// [spec:libedit:def:readline.rl-basic-word-break-characters]
// [spec:libedit:sem:readline.rl-basic-word-break-characters]
#[unsafe(no_mangle)]
pub static mut rl_basic_word_break_characters: *const c_char = BREAK_CHARS.as_ptr();

/// C: `char *rl_completer_word_break_characters = NULL;` — declared, and
/// read by no code path at all (ERR-readline-50).
// [spec:libedit:def:readline.rl-completer-word-break-characters]
// [spec:libedit:sem:readline.rl-completer-word-break-characters]
#[unsafe(no_mangle)]
pub static mut rl_completer_word_break_characters: *mut c_char = ptr::null_mut();

/// C: `const char *rl_completer_quote_characters = NULL;` — likewise unread.
// [spec:libedit:def:readline.rl-completer-quote-characters]
// [spec:libedit:sem:readline.rl-completer-quote-characters]
#[unsafe(no_mangle)]
pub static mut rl_completer_quote_characters: *const c_char = ptr::null();

/// C: `const char *rl_basic_quote_characters = "\"'";` — likewise unread.
// [spec:libedit:def:readline.rl-basic-quote-characters]
// [spec:libedit:sem:readline.rl-basic-quote-characters]
#[unsafe(no_mangle)]
pub static mut rl_basic_quote_characters: *const c_char = c"\"'".as_ptr();

/// C: `rl_compentry_func_t *rl_completion_entry_function = NULL;`
// [spec:libedit:def:readline.rl-completion-entry-function]
// [spec:libedit:sem:readline.rl-completion-entry-function]
#[unsafe(no_mangle)]
pub static mut rl_completion_entry_function: Option<
    unsafe extern "C" fn(*const c_char, c_int) -> *mut c_char,
> = None;

/// C: `extern char *(*rl_completion_word_break_hook)(void);`
///
/// A process-global, application-writable hook that widens the set of
/// characters `rl_complete` stops at when it decides which word the cursor
/// is in. `NULL` in the C, `None` here. Only `rl_complete` reads it.
// [spec:libedit:def:readline.rl-completion-word-break-hook-fn]
// [spec:libedit:sem:readline.rl-completion-word-break-hook-fn]
#[unsafe(no_mangle)]
pub static mut rl_completion_word_break_hook: Option<unsafe extern "C" fn() -> *mut c_char> = None;

/// C: `rl_completion_func_t *rl_attempted_completion_function = NULL;`
// [spec:libedit:def:readline.rl-attempted-completion-function]
// [spec:libedit:sem:readline.rl-attempted-completion-function]
#[unsafe(no_mangle)]
pub static mut rl_attempted_completion_function: Option<
    unsafe extern "C" fn(*const c_char, c_int, c_int) -> *mut *mut c_char,
> = None;

/// C: `rl_hook_func_t *rl_pre_input_hook = NULL;`
// [spec:libedit:def:readline.rl-pre-input-hook]
// [spec:libedit:sem:readline.rl-pre-input-hook]
#[unsafe(no_mangle)]
pub static mut rl_pre_input_hook: Option<unsafe extern "C" fn() -> c_int> = None;

/// C: `rl_hook_func_t *rl_startup1_hook = NULL;` — exported, never called.
// [spec:libedit:def:readline.rl-startup1-hook]
// [spec:libedit:sem:readline.rl-startup1-hook]
#[unsafe(no_mangle)]
pub static mut rl_startup1_hook: Option<unsafe extern "C" fn() -> c_int> = None;

/// C: `extern int (*rl_getc_function)(FILE *);`
///
/// A process-global, application-writable character reader. It sits under
/// the header's "not implemented" banner, but that comment is wrong for this
/// entry: `rl_initialize` samples it once for non-NULL-ness and, if set,
/// installs `_getc_function` as the editor's `EL_GETCFN`. Setting it after
/// initialisation has no effect.
// [spec:libedit:def:readline.rl-getc-function-fn]
// [spec:libedit:sem:readline.rl-getc-function-fn]
#[unsafe(no_mangle)]
pub static mut rl_getc_function: Option<unsafe extern "C" fn(CFile) -> c_int> = None;

/// C: `char *rl_terminal_name = NULL;` — written by `rl_initialize` with a
/// pointer into EditLine's own copy when the application left it NULL.
// [spec:libedit:def:readline.rl-terminal-name]
// [spec:libedit:sem:readline.rl-terminal-name]
#[unsafe(no_mangle)]
pub static mut rl_terminal_name: *mut c_char = ptr::null_mut();

/// C: `int rl_already_prompted = 0;` — set by `_get_prompt`, cleared by
/// `readline()`.
// [spec:libedit:def:readline.rl-already-prompted]
// [spec:libedit:sem:readline.rl-already-prompted]
#[unsafe(no_mangle)]
pub static mut rl_already_prompted: c_int = 0;

/// C: `int rl_filename_completion_desired = 0;` — exported, never consulted.
// [spec:libedit:def:readline.rl-filename-completion-desired]
// [spec:libedit:sem:readline.rl-filename-completion-desired]
#[unsafe(no_mangle)]
pub static mut rl_filename_completion_desired: c_int = 0;

/// C: `int rl_ignore_completion_duplicates = 0;` — exported, never consulted.
// [spec:libedit:def:readline.rl-ignore-completion-duplicates]
// [spec:libedit:sem:readline.rl-ignore-completion-duplicates]
#[unsafe(no_mangle)]
pub static mut rl_ignore_completion_duplicates: c_int = 0;

/// C: `int readline_echoing_p = 1;` — exported, never consulted.
// [spec:libedit:def:readline.readline-echoing-p]
// [spec:libedit:sem:readline.readline-echoing-p]
#[unsafe(no_mangle)]
pub static mut readline_echoing_p: c_int = 1;

/// C: `int _rl_print_completions_horizontally = 0;` — exported, never
/// consulted.
// [spec:libedit:def:readline.rl-print-completions-horizontally]
// [spec:libedit:sem:readline.rl-print-completions-horizontally]
#[unsafe(no_mangle)]
pub static mut _rl_print_completions_horizontally: c_int = 0;

/// C: `rl_voidfunc_t *rl_redisplay_function = NULL;` — readline's
/// indirection point for a custom display routine, which nothing here calls
/// through.
// [spec:libedit:def:readline.rl-redisplay-function]
// [spec:libedit:sem:readline.rl-redisplay-function]
#[unsafe(no_mangle)]
pub static mut rl_redisplay_function: Option<unsafe extern "C" fn()> = None;

/// C: `rl_hook_func_t *rl_startup_hook = NULL;` — called by `readline()`
/// before the terminal is prepared.
// [spec:libedit:def:readline.rl-startup-hook]
// [spec:libedit:sem:readline.rl-startup-hook]
#[unsafe(no_mangle)]
pub static mut rl_startup_hook: Option<unsafe extern "C" fn() -> c_int> = None;

/// C: `rl_compdisp_func_t *rl_completion_display_matches_hook = NULL;` —
/// exported, and bypassed entirely by `rl_display_match_list`.
// [spec:libedit:def:readline.rl-completion-display-matches-hook]
// [spec:libedit:sem:readline.rl-completion-display-matches-hook]
#[unsafe(no_mangle)]
pub static mut rl_completion_display_matches_hook: Option<
    unsafe extern "C" fn(*mut *mut c_char, c_int, c_int),
> = None;

/// C: `rl_vintfunc_t *rl_prep_term_function = (rl_vintfunc_t *)
/// rl_prep_terminal;` — what `rl_reset_after_signal` calls through.
// [spec:libedit:def:readline.rl-prep-term-function]
// [spec:libedit:sem:readline.rl-prep-term-function]
#[unsafe(no_mangle)]
pub static mut rl_prep_term_function: Option<unsafe extern "C" fn(c_int)> = Some(rl_prep_terminal);

/// C: `rl_voidfunc_t *rl_deprep_term_function = (rl_voidfunc_t *)
/// rl_deprep_terminal;` — exported, and never called from this file.
// [spec:libedit:def:readline.rl-deprep-term-function]
// [spec:libedit:sem:readline.rl-deprep-term-function]
#[unsafe(no_mangle)]
pub static mut rl_deprep_term_function: Option<unsafe extern "C" fn()> = Some(rl_deprep_terminal);

/// C: `unsigned long rl_readline_state = RL_STATE_NONE;` — only
/// `RL_STATE_DONE` is ever touched, set by `rl_callback_read_char` and
/// cleared by `rl_initialize`.
// [spec:libedit:def:readline.rl-readline-state]
// [spec:libedit:sem:readline.rl-readline-state]
#[unsafe(no_mangle)]
pub static mut rl_readline_state: c_ulong = RL_STATE_NONE;

/// C: `int _rl_complete_mark_directories;` — exported, never consulted.
// [spec:libedit:def:readline.rl-complete-mark-directories]
// [spec:libedit:sem:readline.rl-complete-mark-directories]
#[unsafe(no_mangle)]
pub static mut _rl_complete_mark_directories: c_int = 0;

/// C: `rl_icppfunc_t *rl_directory_completion_hook;` — exported, never
/// consulted.
// [spec:libedit:def:readline.rl-directory-completion-hook]
// [spec:libedit:sem:readline.rl-directory-completion-hook]
#[unsafe(no_mangle)]
pub static mut rl_directory_completion_hook: Option<
    unsafe extern "C" fn(*mut *mut c_char) -> c_int,
> = None;

/// C: `int rl_completion_suppress_append;` — exported, never consulted.
// [spec:libedit:def:readline.rl-completion-suppress-append]
// [spec:libedit:sem:readline.rl-completion-suppress-append]
#[unsafe(no_mangle)]
pub static mut rl_completion_suppress_append: c_int = 0;

/// C: `int rl_sort_completion_matches;` — exported, never consulted.
// [spec:libedit:def:readline.rl-sort-completion-matches]
// [spec:libedit:sem:readline.rl-sort-completion-matches]
#[unsafe(no_mangle)]
pub static mut rl_sort_completion_matches: c_int = 0;

/// C: `int _rl_completion_prefix_display_length;` — exported, never
/// consulted.
// [spec:libedit:def:readline.rl-completion-prefix-display-length]
// [spec:libedit:sem:readline.rl-completion-prefix-display-length]
#[unsafe(no_mangle)]
pub static mut _rl_completion_prefix_display_length: c_int = 0;

/// C: `int _rl_echoing_p;` — exported, never consulted.
// [spec:libedit:def:readline.rl-echoing-p]
// [spec:libedit:sem:readline.rl-echoing-p]
#[unsafe(no_mangle)]
pub static mut _rl_echoing_p: c_int = 0;

/// C: `int history_max_entries;` — exported, never consulted.
// [spec:libedit:def:readline.history-max-entries]
// [spec:libedit:sem:readline.history-max-entries]
#[unsafe(no_mangle)]
pub static mut history_max_entries: c_int = 0;

/// C: `char *rl_display_prompt;` — exported, never consulted.
// [spec:libedit:def:readline.rl-display-prompt]
// [spec:libedit:sem:readline.rl-display-prompt]
#[unsafe(no_mangle)]
pub static mut rl_display_prompt: *mut c_char = ptr::null_mut();

/// C: `int rl_erase_empty_line;` — exported, never consulted.
// [spec:libedit:def:readline.rl-erase-empty-line]
// [spec:libedit:sem:readline.rl-erase-empty-line]
#[unsafe(no_mangle)]
pub static mut rl_erase_empty_line: c_int = 0;

/// C: `char *rl_prompt = NULL;` — the current prompt, owned by this module.
/// NULL until `rl_set_prompt` has succeeded once.
// [spec:libedit:def:readline.rl-prompt]
// [spec:libedit:sem:readline.rl-prompt]
#[unsafe(no_mangle)]
pub static mut rl_prompt: *mut c_char = ptr::null_mut();

/// C: `char *rl_prompt_saved = NULL;` — `rl_save_prompt`'s copy.
// [spec:libedit:def:readline.rl-prompt-saved]
// [spec:libedit:sem:readline.rl-prompt-saved]
#[unsafe(no_mangle)]
pub static mut rl_prompt_saved: *mut c_char = ptr::null_mut();

/// C: `int rl_completion_type = 0;` — written by `fn_complete2`.
// [spec:libedit:def:readline.rl-completion-type]
// [spec:libedit:sem:readline.rl-completion-type]
#[unsafe(no_mangle)]
pub static mut rl_completion_type: c_int = 0;

/// C: `int rl_completion_query_items = 100;` — the "ask before listing this
/// many" threshold.
// [spec:libedit:def:readline.rl-completion-query-items]
// [spec:libedit:sem:readline.rl-completion-query-items]
#[unsafe(no_mangle)]
pub static mut rl_completion_query_items: c_int = 100;

/// C: `const char *rl_special_prefixes = NULL;` — declared, and read by no
/// code path at all; `rl_complete` puts the word-break hook's result in the
/// special-prefixes slot instead (ERR-readline-50).
// [spec:libedit:def:readline.rl-special-prefixes]
// [spec:libedit:sem:readline.rl-special-prefixes]
#[unsafe(no_mangle)]
pub static mut rl_special_prefixes: *const c_char = ptr::null();

/// C: `int rl_completion_append_character = ' ';`
// [spec:libedit:def:readline.rl-completion-append-character]
// [spec:libedit:sem:readline.rl-completion-append-character]
#[unsafe(no_mangle)]
pub static mut rl_completion_append_character: c_int = b' ' as c_int;

// ---------------------------------------------------------------------------
// The module-private state `readline.c` keeps in file-statics.
// ---------------------------------------------------------------------------

/// C: `static History *h = NULL;` — the readline layer's own history.
static mut H: *mut History = ptr::null_mut();

/// C: `static EditLine *e = NULL;` — the readline layer's own editor.
static mut E: *mut EditLine = ptr::null_mut();

/// C: `static rl_command_func_t *map[256];` — `rl_add_defun`'s dispatch
/// table, consulted by `rl_bind_wrapper`.
static mut MAP: [Option<RlCommandFunc>; 256] = [None; 256];

thread_local! {
    /// C: `static HIST_ENTRY rl_he;` — the shared entry `current_history`
    /// (and therefore `previous_history` and `next_history`) hands back.
    ///
    /// A `thread_local` rather than a `static mut` because the value is
    /// owned Rust that this file hands a pointer to; the readline layer is
    /// single-threaded throughout, which the C's own use of an unsynchronized
    /// static already assumes.
    static RL_HE: Cell<HistEntry> = const {
        Cell::new(HistEntry { line: ptr::null(), data: ptr::null_mut() })
    };

    /// C: `static HIST_ENTRY she;` inside `history_get` — a second, distinct
    /// static entry.
    static SHE: Cell<HistEntry> = const {
        Cell::new(HistEntry { line: ptr::null(), data: ptr::null_mut() })
    };

    /// C: `static char *last_search_pat;` — the last `!?pat?` pattern.
    /// Owned here and dropped properly; the C never frees it
    /// (ERR-readline-17).
    static LAST_SEARCH_PAT: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };

    /// C: `static char *last_search_match;` — what the last `?pat?` matched,
    /// which the `:%` word designator expands to.
    static LAST_SEARCH_MATCH: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };

    /// C: `static HIST_ENTRY *_history_list;` — the entry array
    /// `history_list` hands out, invalidated by the next call.
    static HISTORY_LIST: RefCell<Vec<HistEntry>> = const { RefCell::new(Vec::new()) };

    /// C: `static HIST_ENTRY **_history_listp;` — the NULL-terminated
    /// pointer array over [`HISTORY_LIST`].
    static HISTORY_LISTP: RefCell<Vec<*mut HistEntry>> = const { RefCell::new(Vec::new()) };

    /// The file-statics inside `fn_filename_completion_function`, which the
    /// core made an explicit state object; presenting the C's one
    /// process-wide scan is this crate's job.
    static FILENAME_SCAN: RefCell<Option<FilenameCompletionState>> = const { RefCell::new(None) };

    /// Whether `username_completion_function` has a `<pwd.h>` enumeration
    /// open. The cursor itself is the libc's, process-global, exactly as the
    /// C's is; this only records whether `setpwent` has been issued, so that
    /// a `state` of 0 rewinds and anything else continues.
    static PASSWD_SCAN_OPEN: Cell<bool> = const { Cell::new(false) };

    /// C: `static ct_buffer_t wbreak_conv, sprefix_conv;` inside
    /// `rl_complete` — the wide forms of the two break-character strings,
    /// grown as needed and never freed (ERR-encoding-11).
    static WBREAK_CONV: RefCell<CtBufferT> = const {
        RefCell::new(CtBufferT { cbuff: Vec::new(), csize: 0, wbuff: Vec::new(), wsize: 0 })
    };
    static SPREFIX_CONV: RefCell<CtBufferT> = const {
        RefCell::new(CtBufferT { cbuff: Vec::new(), csize: 0, wbuff: Vec::new(), wsize: 0 })
    };
}

/// C: `static char *from` inside `_history_expand_command` — the last
/// `s///` pattern, reused across calls and reallocated by `getfrom`. It is a
/// C-owned block because `getfrom` takes its address as `char **`.
static mut EXPAND_FROM: *mut c_char = ptr::null_mut();

/// C: `static char *to` inside `_history_expand_command`.
static mut EXPAND_TO: *mut c_char = ptr::null_mut();

/// C: `static int used_event_hook;` inside `readline()`.
static mut USED_EVENT_HOOK: c_int = 0;

/// C: `static char *path;` inside `_default_history_file` — the cached
/// `$HOME/.history`, deliberately never freed.
static mut DEFAULT_HISTORY_FILE: *mut c_char = ptr::null_mut();

/// C: `static char buf[2];` inside
/// `_rl_completion_append_character_function`.
static mut APPEND_CHAR_BUF: [c_char; 2] = [0; 2];

/// The replacement for `readline()`'s `setjmp`/`longjmp` pair.
///
/// Rust has no `longjmp`, and a panic cannot cross the `extern "C"` boundary
/// `_rl_abort_internal` sits on. `sem:readline.rl-abort-internal-fn` calls
/// the jump undefined whenever no `readline()` frame is live, so the port
/// defines the mechanism instead: `_rl_abort_internal` raises this flag and
/// ends the editor loop, and `readline()` — the only frame the C could
/// legally have jumped into — sees it, drops the line and re-runs everything
/// its `setjmp` sat above. Raised with no `readline()` active it is simply
/// consumed by the next one. See ERR-readline-10.
static ABORT_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ---------------------------------------------------------------------------
// The variadic C entry points this file drives the editor and history with.
//
// `readline.c` reaches `el_set`, `el_get` and `history` through the same
// public symbols an application would, so the port does too rather than
// reaching around them into the core. Declaring them variadic is what lets a
// Rust call site pass the tail; *reading* that tail is the callee's problem,
// and all three of them read it.
// ---------------------------------------------------------------------------

// `EditLine` and `History` are opaque handles the C only ever passes by
// pointer, and the core does not spell them `repr(C)`; every exported
// definition in this crate takes them the same way.
#[allow(improper_ctypes)]
unsafe extern "C" {
    /// C: `int el_set(EditLine *el, int op, ...);` — defined by
    /// [`crate::eln::el_set`].
    #[link_name = "el_set"]
    fn el_set_va(el: *mut EditLine, op: c_int, ...) -> c_int;

    /// C: `int el_get(EditLine *el, int op, ...);` — defined by
    /// [`crate::eln::el_get`].
    #[link_name = "el_get"]
    fn el_get_va(el: *mut EditLine, op: c_int, ...) -> c_int;

    /// C: `int history(History *h, HistEvent *ev, int op, ...);` — defined
    /// by [`crate::histedit::history`].
    #[link_name = "history"]
    fn history_va(h: *mut History, ev: *mut HistEvent, op: c_int, ...) -> c_int;
}

/// C: `HistEvent ev;` — the out-parameter every [`history_va`] call needs.
///
/// The C's is an uninitialised local, so a failing op leaves it holding
/// whatever was on the stack, and the callers that read `ev.num` without
/// checking the status — `readline()`, `add_history` — read exactly that.
/// Starting from zero is what makes those reads deterministic; it is the only
/// respect in which the declaration differs from the C's.
const EMPTY_EVENT: HistEvent = HistEvent {
    num: 0,
    str: ptr::null(),
};
// ---------------------------------------------------------------------------
// Memory a C caller frees, and the C strings this file reads.
// ---------------------------------------------------------------------------

/// C: `el_malloc(size)` — `malloc`, so the block is one a C caller may
/// `free()`.
///
/// [`std::alloc::System`] is documented as the platform allocator, which on
/// this port's POSIX target is `malloc`/`free`; `plan/decisions/no-c-ffi.md`
/// leaves no other route to a block that crosses the ABI as owned memory.
/// NULL is returned exactly where the C's allocator would return it.
///
/// # Safety
///
/// The caller owns the block and must release it with [`c_free`] or hand it
/// to a C caller that will `free()` it.
unsafe fn c_alloc(size: usize) -> *mut u8 {
    // `malloc(0)` may return a freeable pointer; `GlobalAlloc` forbids a
    // zero-sized layout, so one byte stands in for it.
    let layout = Layout::from_size_align(size.max(1), 1).expect("byte layout");
    // SAFETY: the layout has a non-zero size.
    unsafe { System.alloc(layout) }
}

/// C: `el_calloc(n, 1)` — zeroed, and otherwise [`c_alloc`].
///
/// # Safety
///
/// As [`c_alloc`].
unsafe fn c_alloc_zeroed(size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size.max(1), 1).expect("byte layout");
    // SAFETY: the layout has a non-zero size.
    unsafe { System.alloc_zeroed(layout) }
}

/// `el_malloc(n * sizeof(T))` for the pointer and struct arrays this file
/// hands back — `char **`, `HIST_ENTRY *`, `HISTORY_STATE *`.
///
/// The alignment is `T`'s, which is at or under the platform's malloc
/// guarantee, so the block stays `free()`-able.
///
/// # Safety
///
/// As [`c_alloc`]. The caller writes every element before it is read.
unsafe fn c_alloc_array<T>(n: usize) -> *mut T {
    let layout = Layout::array::<T>(n.max(1)).expect("array layout");
    // SAFETY: the layout has a non-zero size.
    unsafe { System.alloc(layout).cast() }
}

/// C: `el_free(p)`.
///
/// `free` takes no size, so the layout handed back here is nominal — it is
/// what [`std::alloc::System`] forwards to `free` and ignores. NULL is a
/// no-op, as in the C.
///
/// # Safety
///
/// `p` must be NULL or a block from [`c_alloc`]/[`c_alloc_zeroed`] that is
/// not freed twice.
unsafe fn c_free(p: *mut u8, size: usize) {
    if p.is_null() {
        return;
    }
    let layout = Layout::from_size_align(size.max(1), 1).expect("byte layout");
    // SAFETY: the caller guarantees `p` came from this allocator.
    unsafe { System.dealloc(p, layout) }
}

/// [`c_free`] for a NUL-terminated string, whose length it measures.
///
/// # Safety
///
/// As [`c_free`], and `p` must be NUL-terminated.
pub(crate) unsafe fn c_free_str(p: *mut c_char) {
    if p.is_null() {
        return;
    }
    // SAFETY: the caller guarantees a NUL-terminated string.
    let len = unsafe { CStr::from_ptr(p) }.to_bytes().len();
    // SAFETY: the block came from this allocator.
    unsafe { c_free(p.cast(), len + 1) }
}

/// [`c_free`] for an array from [`c_alloc_array`].
///
/// # Safety
///
/// As [`c_free`], with the `n` the block was allocated with.
pub(crate) unsafe fn c_free_array<T>(p: *mut T, n: usize) {
    if p.is_null() {
        return;
    }
    let layout = Layout::array::<T>(n.max(1)).expect("array layout");
    // SAFETY: the caller guarantees `p` came from this allocator.
    unsafe { System.dealloc(p.cast(), layout) }
}

/// C: `strdup` of a byte string — a NUL-terminated copy the caller frees.
///
/// # Safety
///
/// As [`c_alloc`].
pub(crate) unsafe fn c_dup(b: &[u8]) -> *mut c_char {
    // SAFETY: the length is `b.len() + 1`, which is what is written below.
    let p = unsafe { c_alloc(b.len() + 1) };
    if p.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `p` owns `b.len() + 1` bytes and `b` is a distinct object.
    unsafe {
        ptr::copy_nonoverlapping(b.as_ptr(), p, b.len());
        *p.add(b.len()) = 0;
    }
    p.cast()
}

/// The C's `while (arr[n] != NULL) n++` — the length of a NULL-terminated
/// `char **`, not counting the terminator.
///
/// # Safety
///
/// `p` must be non-NULL and NULL-terminated.
unsafe fn c_array_len(p: *const *mut c_char) -> usize {
    let mut n = 0;
    // SAFETY: the caller guarantees the terminator is reached.
    while !unsafe { *p.add(n) }.is_null() {
        n += 1;
    }
    n
}

/// [`c_free_str`] over every element — the cleanup the completion and
/// tokenizer entry points do to the strings they had collected before an
/// allocation failure made them report NULL.
///
/// # Safety
///
/// As [`c_free_str`], for every element.
unsafe fn c_free_each(list: &[*mut c_char]) {
    for &p in list {
        // SAFETY: the caller guarantees every element.
        unsafe { c_free_str(p) }
    }
}

/// The C's `const char *` as a byte slice: everything up to the NUL.
///
/// # Safety
///
/// `p` must be non-NULL and NUL-terminated, and outlive the slice.
pub(crate) unsafe fn c_bytes<'a>(p: *const c_char) -> &'a [u8] {
    // SAFETY: the caller guarantees a live NUL-terminated string.
    unsafe { CStr::from_ptr(p) }.to_bytes()
}

/// [`c_bytes`], with the C's NULL as `None`.
///
/// # Safety
///
/// As [`c_bytes`], for a non-NULL `p`.
pub(crate) unsafe fn c_bytes_opt<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        None
    } else {
        // SAFETY: `p` is non-NULL; the caller guarantees the rest.
        Some(unsafe { c_bytes(p) })
    }
}

/// `str[i]` where the C may index the terminator or just past a scan's end:
/// out of range reads as `'\0'`, which is what the C finds there.
fn at(s: &[u8], i: usize) -> u8 {
    s.get(i).copied().unwrap_or(0)
}

/// The C's `strchr(s, c)` "found it" test, terminator included — `strchr`
/// matches the NUL, which is why a `!` at end of line never expands.
fn strchr(s: &[u8], c: u8) -> bool {
    c == 0 || s.contains(&c)
}

/// `isspace` over the C locale's set, which is what `readline.c` gets: the
/// scan is over bytes and the C casts to `unsigned char` at every call site.
fn isspace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// `fprintf(rl_outstream, ...)` for the two diagnostics `get_history_event`
/// and `_history_expand_command` print.
///
/// The stream is the application's, so this goes through it — one `fputs`,
/// buffered exactly as the C's `fprintf` is, so a caller that interleaves its
/// own writes sees them in the order it wrote them. See [`crate::cstdio`].
///
/// A NULL `rl_outstream` reaches the process's standard output instead. The C
/// cannot be here with a NULL one: `rl_initialize` defaults it to `stdout`,
/// which is step 3 and the one part of that rule the port still cannot do —
/// `stdout` is a data object and is not on `no-c-ffi`'s enumeration. Writing
/// to descriptor 1 is what that stream would have done.
fn rl_out_write(msg: &[u8]) {
    // SAFETY: single-threaded module state.
    let stream = unsafe { rl_outstream };
    if !stream.is_null() {
        let _ = cstdio::CFileWriter::new(stream).write_all(msg);
        return;
    }
    let out = std::io::stdout();
    let mut out = out.lock();
    let _ = out.write_all(msg);
    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Core gaps
//
// `readline.c` calls these `libedit_private` entry points directly. Every one
// of them is `pub(crate)` in the core, so this crate cannot reach it; the
// stand-ins keep the call sites reading as the C does and are the list to work
// through when the core publishes them.
// ---------------------------------------------------------------------------

/// C: `tty_init(el)` — needs `nshedit::tty::tty_init`.
fn tty_init(el: *mut EditLine) {
    let _ = el;
}

/// C: `tty_end(el, how)` — needs `nshedit::tty::tty_end`.
fn tty_end(el: *mut EditLine, how: c_int) {
    let _ = (el, how);
}

/// C: `re_putc(el, c, 0)` — needs `nshedit::refresh::re_putc`. Writes into
/// the *virtual display array* without advancing the refresh cursor and
/// without reaching the terminal, which is why `rl_ding` and friends are
/// nearly no-ops even in the C (ERR-readline-25).
fn re_putc(el: *mut EditLine, c: u32) {
    let _ = (el, c);
}

/// C: `em_kill_line(el, 0)` — needs `nshedit::emacs::em_kill_line`.
fn em_kill_line(el: *mut EditLine) {
    let _ = el;
}

/// C: `tty_get_signal_character(el, sig)` — needs
/// `nshedit::tty::tty_get_signal_character`. -1 is the C's own answer when
/// `ECHOCTL` is clear or the signal has no character, so `rl_echo_signal_char`
/// takes its documented early return until the core publishes it.
fn tty_get_signal_character(el: *mut EditLine, sig: c_int) -> c_int {
    let _ = (el, sig);
    -1
}

/// C: `#define NO_TTY 0x002` (`el.h`) — the flag `readline()` tests before
/// installing the event-hook reader. The core's copy is `pub(crate)`.
const NO_TTY: i32 = 0x002;

// ---------------------------------------------------------------------------
// The passwd database
//
// `readline.c` uses `getpwuid(getuid())` and the
// `setpwent`/`getpwent`/`endpwent` cursor. Both go through `nshedit-plat`,
// which reaches NSS — so the `/etc/passwd` parser that used to stand here,
// the second of the two the port carried, is deleted rather than kept as a
// fallback. See `plan/decisions/platform-layer.md`, group 10.
// ---------------------------------------------------------------------------

/// `getpwuid(getuid())->pw_dir`.
///
/// The *real* uid, as the C's `getuid()` is. `None` where the C's `getpwuid`
/// returns NULL.
fn passwd_home_dir() -> Option<Vec<u8>> {
    nshedit_plat::passwd::home_dir_by_uid(nshedit_plat::getuid())
}

// ---------------------------------------------------------------------------
// Shared shims
// ---------------------------------------------------------------------------

/// C: `if (h == NULL || e == NULL) rl_initialize();` — the lazy-init guard
/// most entry points open with. The two orderings the C writes (`h` first or
/// `e` first) are equivalent.
///
/// # Safety
///
/// Reaches the module statics, so it inherits `rl_initialize`'s contract.
unsafe fn lazy_init() {
    // SAFETY: single-threaded module state, as the C's own statics assume.
    unsafe {
        if H.is_null() || E.is_null() {
            rl_initialize();
        }
    }
}

/// The `_rl_completion_append_character_function` result in the shape the
/// core's `AppFunc` wants.
///
/// The C hands `fn_complete2` a pointer to a shared `char[2]`. The core takes
/// a `fn(&str) -> &'static str`, so the byte is served out of a static ASCII
/// table instead. `rl_completion_append_character == 0` gives the empty
/// string, which is the readline idiom for "append nothing" and the input
/// that makes `escape_filename` produce an embedded NUL (ERR-completion-10).
/// A value outside ASCII cannot be a `&'static str` on its own and also gives
/// the empty string; the C would append one invalid byte.
fn append_char_str(_name: &str) -> &'static str {
    /// Every ASCII byte, one per index, so a single byte can be handed back
    /// as a `&'static str` without allocating.
    static ASCII: [u8; 128] = const {
        let mut a = [0u8; 128];
        let mut i = 0;
        while i < 128 {
            a[i] = i as u8;
            i += 1;
        }
        a
    };
    // SAFETY: a plain read of a module static.
    let c = unsafe { rl_completion_append_character };
    if (1..128).contains(&c) {
        let i = c as usize;
        std::str::from_utf8(&ASCII[i..=i]).unwrap_or("")
    } else {
        ""
    }
}

/// The `rl_attempted_completion_function` hook in the shape the core's
/// `AttemptedCompletionFunc` wants.
///
/// A plain `fn`, not a closure, because the core's type is a bare function
/// pointer — which works here only because the hook itself lives in a global.
/// Ownership follows the C: `fn_complete2` takes the returned array, so the
/// array and its strings are released here once copied.
fn attempted_completion_adapter(text: &str, start: i32, end: i32) -> Option<Vec<String>> {
    // SAFETY: single-threaded module state; the hook is called exactly as the
    // C calls it, with a borrowed NUL-terminated copy of `text`.
    unsafe {
        let hook = rl_attempted_completion_function?;
        let ctext = c_dup(text.as_bytes());
        if ctext.is_null() {
            return None;
        }
        let matches = hook(ctext, start, end);
        c_free_str(ctext);
        if matches.is_null() {
            return None;
        }
        let n = c_array_len(matches);
        let mut out = Vec::with_capacity(n);
        for &m in core::slice::from_raw_parts(matches, n) {
            out.push(String::from_utf8_lossy(c_bytes(m)).into_owned());
            c_free_str(m);
        }
        c_free_array(matches, n + 1);
        Some(out)
    }
}

/// `strcmp` over the two arguments `qsort` hands
/// `rl_completion_matches`' comparator: the *addresses* of the array
/// elements, i.e. the byte representations of two `char *` values.
///
/// This is the defect itself, not a helper around it — see ERR-readline-01
/// and step 6 of `sem:readline.rl-completion-matches-fn`. `strcmp` stops at
/// the first zero byte, so on a little-endian target the ordering is by the
/// low byte of the pointer first and terminates inside the pointer whenever
/// one of its bytes is zero — which for heap addresses it is. Two
/// representations that share no zero byte and never differ compare equal
/// here; the C would read on into the neighbouring array element, which is
/// out of bounds and is the part the port does not reproduce.
fn strcmp_pointer_repr(a: *mut c_char, b: *mut c_char) -> Ordering {
    let (x, y) = (a.addr().to_ne_bytes(), b.addr().to_ne_bytes());
    for i in 0..x.len() {
        if x[i] != y[i] {
            return x[i].cmp(&y[i]);
        }
        if x[i] == 0 {
            break;
        }
    }
    Ordering::Equal
}
// ---------------------------------------------------------------------------
// `readline.c`, in source order.
// ---------------------------------------------------------------------------

/// C: `static char *_get_prompt(EditLine *el);` — the `EL_PROMPT_ESC`
/// callback, handing libedit the application's `rl_prompt`.
// [spec:libedit:def:readline.get-prompt-fn]
// [spec:libedit:sem:readline.get-prompt-fn]
fn _get_prompt(el: *mut EditLine) -> *mut c_char {
    let _ = el;
    // SAFETY: single-threaded module state. `rl_prompt` is borrowed, not
    // handed over: libedit must not free or modify it, and `rl_set_prompt`
    // may replace it between calls.
    unsafe {
        rl_already_prompted = 1;
        rl_prompt
    }
}

/// C: `static int _getc_function(EditLine *el, wchar_t *c);` — the
/// `EL_GETCFN` shim that forwards to `rl_getc_function`.
// [spec:libedit:def:readline.getc-function-fn]
// [spec:libedit:sem:readline.getc-function-fn]
fn _getc_function(el: *mut EditLine, c: *mut u32) -> c_int {
    let _ = el;
    // SAFETY: `c` is EditLine's one-character out parameter.
    unsafe {
        // The C dereferences `rl_getc_function` with no NULL check, so an
        // application that clears it after `rl_initialize` calls NULL
        // (ERR-readline-32, UB). Defined here as end of input, which is what
        // the read layer does with the 0 the hook's own -1 produces.
        let Some(hook) = rl_getc_function else {
            return 0;
        };
        // The hook is called with the *current* `rl_instream`, not with
        // EditLine's own stream, so redirecting it after initialisation
        // changes what the hook is asked to read.
        let i = hook(rl_instream);
        if i == -1 {
            return 0;
        }
        // Widened with no multibyte decoding, so one call is exactly one wide
        // character and a byte-oriented hook produces mojibake under a UTF-8
        // locale (ERR-readline-32, reproduced). Only exactly -1 is EOF; any
        // other negative value is stored as a character.
        *c = i as u32;
        1
    }
}

/// C: `static void _resize_fun(EditLine *el, void *a);` — the `EL_RESIZE`
/// callback that republishes the line into `rl_line_buffer`.
// [spec:libedit:def:readline.resize-fun-fn]
// [spec:libedit:sem:readline.resize-fun-fn]
fn _resize_fun(el: *mut EditLine, a: *mut c_void) {
    if el.is_null() || a.is_null() {
        return;
    }
    // SAFETY: `a` is always `&rl_line_buffer`, which `rl_initialize` passes to
    // `el_set(EL_RESIZE, ...)` and to the one direct call. `el_line` guards
    // its own re-entry with FROM_ELLINE, so this does not recurse.
    unsafe {
        let li = crate::eln::el_line(el);
        if li.is_null() {
            return;
        }
        // Borrowed: this points into EditLine's conversion buffer, which is
        // reallocated whenever a longer line is encoded, so an application
        // must never free or resize it.
        *a.cast::<*const c_char>() = (*li).buffer;
    }
}

/// C: `static const char *_default_history_file(void);` — `$HOME/.history`,
/// cached in a file-static buffer.
// [spec:libedit:def:readline.default-history-file-fn]
// [spec:libedit:sem:readline.default-history-file-fn]
fn _default_history_file() -> *const c_char {
    // SAFETY: single-threaded module state, as the C's unsynchronized static
    // already assumes.
    unsafe {
        if !DEFAULT_HISTORY_FILE.is_null() {
            return DEFAULT_HISTORY_FILE;
        }
        // `getpwuid(getuid())`, so `$HOME` is ignored — the observable part,
        // reproduced (ERR-readline-21). NULL when there is no passwd entry;
        // callers turn that into their `errno` return.
        let Some(mut dir) = passwd_home_dir() else {
            return ptr::null();
        };
        dir.extend_from_slice(b"/.history");
        // Cached for the lifetime of the process and never freed, which is
        // what the C's function-static does. On allocation failure the cache
        // is left NULL so a later call retries.
        DEFAULT_HISTORY_FILE = c_dup(&dir);
        DEFAULT_HISTORY_FILE
    }
}

// [spec:libedit:def:readline.rl-set-prompt-fn]
// [spec:libedit:sem:readline.rl-set-prompt-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_prompt(prompt: *const c_char) -> c_int {
    // SAFETY: `prompt` is NULL or a NUL-terminated string owned by the
    // caller; `rl_prompt` is this module's own block.
    unsafe {
        let text = c_bytes_opt(prompt).unwrap_or(b"");
        // Unchanged prompt: no reallocation, so the existing buffer and any
        // pointer EditLine holds to it stay valid.
        if !rl_prompt.is_null() && c_bytes(rl_prompt) == text {
            return 0;
        }
        c_free_str(rl_prompt);
        rl_prompt = c_dup(text);
        if rl_prompt.is_null() {
            return -1;
        }

        // Collapse readline's start/end bracketing onto the single toggle
        // character EditLine understands: every END marker becomes a START
        // marker, and an adjacent END/START pair is removed outright so two
        // abutting invisible regions do not produce a double escape. The C
        // restarts the search from the beginning each time, which is
        // quadratic in the number of markers and always terminates.
        loop {
            let s = c_bytes(rl_prompt);
            let Some(i) = s.iter().position(|&b| b == RL_PROMPT_END_IGNORE) else {
                break;
            };
            if at(s, i + 1) == RL_PROMPT_START_IGNORE {
                let rest = s.len() - (i + 2);
                let p = rl_prompt.cast::<u8>().add(i);
                ptr::copy(p.add(2), p, rest + 1);
            } else {
                *rl_prompt.cast::<u8>().add(i) = RL_PROMPT_START_IGNORE;
            }
        }
        0
    }
}

// [spec:libedit:def:readline.rl-save-prompt-fn]
// [spec:libedit:sem:readline.rl-save-prompt-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_save_prompt() {
    // SAFETY: single-threaded module state.
    unsafe {
        // The C hands `rl_prompt` to `strdup` unchecked, and it is NULL until
        // `rl_set_prompt` has succeeded once (ERR-readline-09, UB). Defined
        // here as saving nothing, so the matching `rl_restore_prompt` is the
        // documented no-op.
        if rl_prompt.is_null() {
            c_free_str(rl_prompt_saved);
            rl_prompt_saved = ptr::null_mut();
            return;
        }
        // Saves do not nest. The C leaks the earlier copy (ERR-readline-18);
        // the leak is not observable, so the port releases it.
        c_free_str(rl_prompt_saved);
        rl_prompt_saved = c_dup(c_bytes(rl_prompt));
    }
}

// [spec:libedit:def:readline.rl-restore-prompt-fn]
// [spec:libedit:sem:readline.rl-restore-prompt-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_restore_prompt() {
    // SAFETY: single-threaded module state.
    unsafe {
        // Restore without a matching save is a no-op.
        if rl_prompt_saved.is_null() {
            return;
        }
        // Ownership transfers back to `rl_prompt`, reproduced. The C
        // overwrites the current prompt without freeing it, leaking the
        // message `rl_message` installed (ERR-readline-18); not observable,
        // so the port frees it. Nothing is redrawn.
        c_free_str(rl_prompt);
        rl_prompt = rl_prompt_saved;
        rl_prompt_saved = ptr::null_mut();
    }
}

// [spec:libedit:def:readline.rl-initialize-fn]
// [spec:libedit:sem:readline.rl-initialize-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_initialize() -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; every editor call below is the
    // one `readline.c` makes, in its order.
    unsafe {
        if !E.is_null() {
            crate::histedit::el_end(E);
        }
        if !H.is_null() {
            crate::histedit::history_end(H);
        }
        // The C clears neither static, so a failure below leaves one or both
        // dangling and every later entry point dereferences freed memory
        // (ERR-readline-19, UB). Defined here by clearing them first.
        E = ptr::null_mut();
        H = ptr::null_mut();

        rl_readline_state &= !RL_STATE_DONE;

        // Steps 4 and 5's `fileno` is no longer a gap: an application that
        // installs its own `rl_instream`/`rl_outstream` now gets *its*
        // descriptors, which is the whole point of those exported globals.
        //
        // Step 3 still is one. `stdin` and `stdout` are data objects in the C
        // library — and macros over an array on the BSDs — which
        // `plan/decisions/no-c-ffi.md`'s enumeration does not reach, so a
        // NULL stream keeps meaning "the standard one" instead of being
        // rewritten to the C's own stream object, and falls back to the
        // descriptor that stream carries at program start. Same for `stderr`,
        // which step 5 passes as `ferr`.
        let fdin = if rl_instream.is_null() {
            0
        } else {
            cstdio::fileno_of(rl_instream)
        };
        let fdout = if rl_outstream.is_null() {
            1
        } else {
            cstdio::fileno_of(rl_outstream)
        };
        let fderr = 2;

        // Step 4's `tcgetattr(fileno(rl_instream))` test for a clear `ECHO`,
        // both halves of which are answerable now — `fileno` through
        // `crate::cstdio` and `tcgetattr` through `nshedit-plat`. A terminal
        // that is not echoing is one something else is driving, so readline
        // declines to edit on it. A failing `tcgetattr` leaves `editmode` at
        // 1, which is what the C does with its uninitialised `t`: the branch
        // is only entered on success.
        let editmode = match nshedit_plat::termios::tcgetattr(fdin) {
            Some(t) => c_int::from(t.c_lflag & nshedit_plat::termios::ECHO != 0),
            None => 1,
        };

        E = crate::histedit::el_init_fd(
            rl_readline_name,
            rl_instream,
            rl_outstream,
            ptr::null_mut(),
            fdin,
            fdout,
            fderr,
        );
        // Gap: `el_init_internal`, and with it the `NO_RESET` flag the C asks
        // for, is `pub(crate)` in the core; `el_init_fd` passes flags of 0,
        // so EditLine will reset the tty on teardown.

        if editmode == 0 {
            el_set_va(E, EL_EDITMODE, 0);
        }

        H = crate::histedit::history_init();
        if E.is_null() || H.is_null() {
            return -1;
        }

        history_va(H, &mut ev, H_SETSIZE, c_int::MAX); /* unlimited */
        history_length = 0;
        max_input_history = c_int::MAX;
        el_set_va(E, EL_HIST, crate::histedit::history as *const c_void, H);

        /* Setup resize function */
        el_set_va(
            E,
            EL_RESIZE,
            _resize_fun as *const c_void,
            &raw mut rl_line_buffer,
        );

        /* setup getc function if valid */
        // Sampled once, and only for non-NULL-ness: setting `rl_getc_function`
        // afterwards has no effect until the next `rl_initialize`
        // (ERR-readline-31, reproduced).
        let getc_hook = rl_getc_function;
        if getc_hook.is_some() {
            el_set_va(E, EL_GETCFN, _getc_function as *const c_void);
        }

        /* for proper prompt printing in readline() */
        if rl_set_prompt(c"".as_ptr()) == -1 {
            crate::histedit::history_end(H);
            crate::histedit::el_end(E);
            H = ptr::null_mut();
            E = ptr::null_mut();
            return -1;
        }
        el_set_va(
            E,
            EL_PROMPT_ESC,
            _get_prompt as *const c_void,
            RL_PROMPT_START_IGNORE as c_int,
        );
        el_set_va(E, EL_SIGNAL, rl_catch_signals);

        /* set default mode to "emacs"-style and read setting afterwards */
        /* so this can be overridden */
        el_set_va(E, EL_EDITOR, c"emacs".as_ptr());
        if !rl_terminal_name.is_null() {
            el_set_va(E, EL_TERMINAL, rl_terminal_name);
        } else {
            // Writes the global with a pointer to EditLine's own copy: a
            // borrowed pointer the application must not free and that dies
            // with `e`.
            el_get_va(E, EL_TERMINAL, &raw mut rl_terminal_name);
        }

        /*
         * Word completion - this has to go AFTER rebinding keys
         * to emacs-style.
         */
        el_set_va(
            E,
            EL_ADDFN,
            c"rl_complete".as_ptr(),
            c"ReadLine compatible completion function".as_ptr(),
            _el_rl_complete as *const c_void,
        );
        el_set_va(
            E,
            EL_BIND,
            c"^I".as_ptr(),
            c"rl_complete".as_ptr(),
            ptr::null::<c_char>(),
        );

        /*
         * Send TSTP when ^Z is pressed.
         */
        el_set_va(
            E,
            EL_ADDFN,
            c"rl_tstp".as_ptr(),
            c"ReadLine compatible suspend function".as_ptr(),
            _el_rl_tstp as *const c_void,
        );
        el_set_va(
            E,
            EL_BIND,
            c"^Z".as_ptr(),
            c"rl_tstp".as_ptr(),
            ptr::null::<c_char>(),
        );

        /*
         * Set some readline compatible key-bindings.
         */
        el_set_va(
            E,
            EL_BIND,
            c"^R".as_ptr(),
            c"em-inc-search-prev".as_ptr(),
            ptr::null::<c_char>(),
        );

        /*
         * Allow the use of Home/End keys, of Delete/Insert, and of
         * Ctrl-left-arrow and Ctrl-right-arrow for word moving.
         */
        for (keyseq, cmd) in [
            (c"\\e[1~", c"ed-move-to-beg"),
            (c"\\e[4~", c"ed-move-to-end"),
            (c"\\e[7~", c"ed-move-to-beg"),
            (c"\\e[8~", c"ed-move-to-end"),
            (c"\\e[H", c"ed-move-to-beg"),
            (c"\\e[F", c"ed-move-to-end"),
            (c"\\e[3~", c"ed-delete-next-char"),
            (c"\\e[2~", c"em-toggle-overwrite"),
            (c"\\e[1;5C", c"em-next-word"),
            (c"\\e[1;5D", c"ed-prev-word"),
            (c"\\e[5C", c"em-next-word"),
            (c"\\e[5D", c"ed-prev-word"),
            (c"\\e\\e[C", c"em-next-word"),
            (c"\\e\\e[D", c"ed-prev-word"),
        ] {
            el_set_va(
                E,
                EL_BIND,
                keyseq.as_ptr(),
                cmd.as_ptr(),
                ptr::null::<c_char>(),
            );
        }

        /* read settings from configuration file */
        // `$EDITRC` or `~/.editrc`; readline's own `~/.inputrc` is not read
        // (ERR-readline-46).
        crate::histedit::el_source(E, ptr::null());

        /*
         * Unfortunately, some applications really do use rl_point
         * and rl_line_buffer directly.
         */
        _resize_fun(E, (&raw mut rl_line_buffer).cast());
        _rl_update_pos();

        tty_end(E, TCSADRAIN);

        0
    }
}

// [spec:libedit:def:readline.readline-fn]
// [spec:libedit:sem:readline.readline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readline(p: *const c_char) -> *mut c_char {
    let prompt = p;
    let mut ev = EMPTY_EVENT;
    let mut buf: *mut c_char;
    // SAFETY: single-threaded module state.
    unsafe {
        if E.is_null() || H.is_null() {
            // The return value is not checked, so a failed initialization is
            // not detected here.
            rl_initialize();
        }
        if let Some(hook) = rl_startup_hook {
            hook();
        }
        tty_init(E);

        rl_done = 0;

        // The C's `setjmp(topbuf)` lands here; see `ABORT_PENDING`.
        loop {
            ABORT_PENDING.store(false, std::sync::atomic::Ordering::Relaxed);
            buf = ptr::null_mut();

            /* update prompt accordingly to what has been passed */
            if rl_set_prompt(prompt) == -1 {
                break;
            }

            if let Some(hook) = rl_pre_input_hook {
                hook();
            }

            // The restore installs the *builtin* reader, clobbering any
            // `_getc_function` installed for `rl_getc_function`: an
            // application using both hooks loses the getc hook permanently
            // (ERR-readline-31, reproduced).
            let event_hook = rl_event_hook;
            if event_hook.is_some() && !E.is_null() && (*E).el_flags & NO_TTY == 0 {
                el_set_va(E, EL_GETCFN, _rl_event_read_char as *const c_void);
                USED_EVENT_HOOK = 1;
            }
            if event_hook.is_none() && USED_EVENT_HOOK != 0 {
                el_set_va(E, EL_GETCFN, EL_BUILTIN_GETCFN);
                USED_EVENT_HOOK = 0;
            }

            rl_already_prompted = 0;

            /* get one line from input stream */
            let mut count: c_int = 0;
            let ret = crate::eln::el_gets(E, &mut count);

            // `_rl_abort_internal` ran: drop the line and re-execute
            // everything the C's `setjmp` sat above.
            if ABORT_PENDING.swap(false, std::sync::atomic::Ordering::Relaxed) {
                continue;
            }

            if !ret.is_null() && count > 0 {
                buf = c_dup(c_bytes(ret));
                if buf.is_null() {
                    break;
                }
                let lastidx = count as usize - 1;
                // Only a single trailing newline is removed, and only when
                // `count` says it is the last byte; a carriage return is not
                // stripped. The C indexes `strdup`'s block with `count`,
                // which under EL_UNBUFFERED can be past the copy's end — the
                // bound here is the port's definition of that read.
                if lastidx < c_bytes(buf).len() && *buf.add(lastidx) == b'\n' as c_char {
                    *buf.add(lastidx) = 0;
                }
            } else {
                buf = ptr::null_mut();
            }

            // Refreshed even though `readline()` never adds to the history.
            history_va(H, &mut ev, H_GETSIZE);
            history_length = ev.num;
            break;
        }

        tty_end(E, TCSADRAIN);
        buf
    }
}

// [spec:libedit:def:readline.using-history-fn]
// [spec:libedit:sem:readline.using-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn using_history() {
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();
        // One past the newest entry, which is where `previous_history`
        // expects to start and which `history_set_pos` cannot reach.
        history_offset = history_length;
    }
}

/// C: `static char *_rl_compat_sub(const char *str, const char *what,
/// const char *with, int globally);`
// [spec:libedit:def:readline.rl-compat-sub-fn]
// [spec:libedit:sem:readline.rl-compat-sub-fn]
fn _rl_compat_sub(
    str_: *const c_char,
    what: *const c_char,
    with: *const c_char,
    globally: c_int,
) -> *mut c_char {
    // SAFETY: all three are NUL-terminated strings owned by the caller.
    unsafe {
        // The C's one caller can reach this with a NULL `str` only after
        // `replace` lost its buffer to a failed `strdup` (ERR-readline-05),
        // which the port defines away; the guard keeps the definition local.
        if str_.is_null() || what.is_null() || with.is_null() {
            return ptr::null_mut();
        }
        let s = c_bytes(str_);
        let what = c_bytes(what);
        let with = c_bytes(with);

        // The C's first pass only sizes the result; a growing buffer does
        // that itself. An empty `what` never matches, because the C's
        // `*s == *what` test cannot hold before the input's NUL, so the
        // function degenerates to a plain copy.
        let mut out = Vec::with_capacity(s.len());
        let mut i = 0;
        while i < s.len() {
            if !what.is_empty() && s[i..].starts_with(what) {
                out.extend_from_slice(with);
                i += what.len();
                if globally == 0 {
                    // Not global: copy the whole remaining input and stop.
                    out.extend_from_slice(&s[i..]);
                    break;
                }
            } else {
                out.push(s[i]);
                i += 1;
            }
        }
        c_dup(&out)
    }
}

// [spec:libedit:def:readline.get-history-event-fn]
// [spec:libedit:sem:readline.get-history-event-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_history_event(
    cmd: *const c_char,
    cindex: *mut c_int,
    qchar: c_int,
) -> *const c_char {
    let mut ev = EMPTY_EVENT;
    // SAFETY: `cmd` is a NUL-terminated string and `cindex` a live `int`.
    unsafe {
        let s = c_bytes(cmd);
        let mut idx = *cindex as usize;
        if at(s, idx) != history_expansion_char as u8 {
            return ptr::null();
        }
        idx += 1;

        /* find out which event to take */
        if at(s, idx) == history_expansion_char as u8 || at(s, idx) == 0 {
            if history_va(H, &mut ev, H_FIRST) != 0 {
                return ptr::null();
            }
            *cindex = if at(s, idx) != 0 {
                idx as c_int + 1
            } else {
                idx as c_int
            };
            return ev.str;
        }
        let mut sign = false;
        if at(s, idx) == b'-' {
            sign = true;
            idx += 1;
        }

        if at(s, idx).is_ascii_digit() {
            // No overflow check, as in the C.
            let mut num: c_int = 0;
            while at(s, idx).is_ascii_digit() {
                num = num
                    .wrapping_mul(10)
                    .wrapping_add((at(s, idx) - b'0') as c_int);
                idx += 1;
            }
            if sign {
                num = history_length - num + history_base;
            }

            let he = history_get(num);
            if he.is_null() {
                return ptr::null();
            }
            *cindex = idx as c_int;
            // Points at `history_get`'s own static entry, so the result is
            // invalidated by the next `history_get`.
            return (*he).line;
        }

        let mut sub = false;
        if at(s, idx) == b'?' {
            sub = true;
            idx += 1;
        }
        let begin = idx;
        while at(s, idx) != 0 {
            let c = at(s, idx);
            if c == b'\n' {
                break;
            }
            if sub && c == b'?' {
                break;
            }
            if !sub && (c == b':' || c == b' ' || c == b'\t' || c == qchar as u8) {
                break;
            }
            idx += 1;
        }
        let len = idx - begin;
        if sub && at(s, idx) == b'?' {
            idx += 1;
        }

        // The pattern is either borrowed from `last_search_pat` or freshly
        // copied; the C tells the two apart by pointer identity, which is
        // what `pat_is_last` stands for.
        let mut pat_is_last = false;
        let pat: Vec<u8> = if sub
            && len == 0
            && LAST_SEARCH_PAT.with_borrow(|p| p.as_ref().is_some_and(|p| !p.is_empty()))
        {
            pat_is_last = true;
            LAST_SEARCH_PAT.with_borrow(|p| p.clone().unwrap_or_default())
        } else if len == 0 {
            return ptr::null();
        } else {
            s[begin..begin + len].to_vec()
        };
        let cpat = c_dup(&pat);
        if cpat.is_null() {
            return ptr::null();
        }

        // Recorded for later restoration.
        if history_va(H, &mut ev, H_CURR) != 0 {
            c_free_str(cpat);
            return ptr::null();
        }
        let num = ev.num;

        let ret = if sub {
            if !pat_is_last {
                LAST_SEARCH_PAT.with_borrow_mut(|p| *p = Some(pat.clone()));
            }
            history_search(cpat, -1)
        } else {
            history_search_prefix(cpat, -1)
        };

        if ret == -1 {
            /* restore to end of list on failed search */
            history_va(H, &mut ev, H_FIRST);
            let mut msg = pat.clone();
            msg.extend_from_slice(b": Event not found\n");
            rl_out_write(&msg);
            c_free_str(cpat);
            return ptr::null();
        }

        // What the `:%` word designator later expands to.
        if sub && len != 0 {
            LAST_SEARCH_MATCH.with_borrow_mut(|m| *m = Some(pat.clone()));
        }

        c_free_str(cpat);

        if history_va(H, &mut ev, H_CURR) != 0 {
            return ptr::null();
        }
        *cindex = idx as c_int;
        let rptr = ev.str;

        /* roll back to original position */
        history_va(H, &mut ev, H_SET, num);

        rptr
    }
}

/// C: `static int getfrom(const char **cmdp, char **fromp,
/// const char *search, int delim);`
// [spec:libedit:def:readline.getfrom-fn]
// [spec:libedit:sem:readline.getfrom-fn]
fn getfrom(
    cmdp: *mut *const c_char,
    fromp: *mut *mut c_char,
    search: *const c_char,
    delim: c_int,
) -> c_int {
    // SAFETY: both out-parameters are live; `*fromp` is this module's own
    // reusable block and `*cmdp` walks a NUL-terminated string.
    unsafe {
        let cmd = c_bytes(*cmdp);
        let delim = delim as u8;

        // The C reallocates the caller's buffer to 16 bytes and appends into
        // it. Growth is the defect: `if (len - 1 >= size)` wraps on the first
        // iteration and thereafter fires one element late, so the byte at
        // index `size` is written *before* the growth — a one-byte heap
        // overflow at 32, then 64, then 128, and the trailing NUL can land
        // one past the end for the same reason (ERR-readline-02, UB).
        // Defined here by growing when `len == size`, which is what the rule
        // prescribes; a `Vec` is exactly that policy.
        let mut what: Vec<u8> = Vec::with_capacity(16);
        let mut i = 0;
        while at(cmd, i) != 0 && at(cmd, i) != delim {
            if at(cmd, i) == b'\\' && at(cmd, i + 1) == delim {
                // Drop the backslash and take the delimiter literally.
                i += 1;
            }
            what.push(at(cmd, i));
            i += 1;
        }

        c_free_str(*fromp);
        *fromp = c_dup(&what);
        *cmdp = (*cmdp).add(i);
        if (*fromp).is_null() {
            return 0;
        }

        if what.is_empty() {
            if let Some(search) = c_bytes_opt(search) {
                // Dead at the only call site: `_history_expand_command`
                // initializes `search` to NULL and never assigns it. Were it
                // reached, the C would free its buffer a second time on the
                // way through the next check (ERR-readline-54); the port
                // simply falls through.
                c_free_str(*fromp);
                *fromp = c_dup(search);
                if (*fromp).is_null() {
                    return 0;
                }
            } else {
                c_free_str(*fromp);
                *fromp = ptr::null_mut();
                return -1;
            }
        }

        if at(cmd, i) == 0 {
            /* no closing delimiter */
            c_free_str(*fromp);
            *fromp = ptr::null_mut();
            return -1;
        }

        i += 1; /* shift after delim */
        *cmdp = (*cmdp).add(1);

        // `!!:s/foo/` with nothing after the second delimiter is an error
        // rather than a deletion, unlike GNU readline (ERR-readline-51).
        if at(cmd, i) == 0 {
            c_free_str(*fromp);
            *fromp = ptr::null_mut();
            return -1;
        }
        1
    }
}

/// C: `static int getto(const char **cmdp, char **top, const char *from,
/// int delim);`
// [spec:libedit:def:readline.getto-fn]
// [spec:libedit:sem:readline.getto-fn]
fn getto(
    cmdp: *mut *const c_char,
    top: *mut *mut c_char,
    from: *const c_char,
    delim: c_int,
) -> c_int {
    // SAFETY: as `getfrom`.
    unsafe {
        let cmd = c_bytes(*cmdp);
        let from = c_bytes(from);
        let delim = delim as u8;

        // The C discards the old buffer's only pointer right after
        // reallocating it, leaking it if the realloc failed
        // (ERR-readline-16); freeing it outright is the same thing without
        // the leak, and is also why the error exit cannot double-free.
        c_free_str(*top);
        *top = ptr::null_mut();

        let mut with: Vec<u8> = Vec::with_capacity(16);
        let mut i = 0;
        while at(cmd, i) != 0 && at(cmd, i) != delim {
            if at(cmd, i) == b'&' {
                // `&` expands to the whole `from` text.
                with.extend_from_slice(from);
                i += 1;
                continue;
            }
            if at(cmd, i) == b'\\' && (at(cmd, i + 1) == delim || at(cmd, i + 1) == b'&') {
                i += 1;
            }
            with.push(at(cmd, i));
            i += 1;
        }
        if at(cmd, i) == 0 {
            /* no closing delimiter */
            *cmdp = (*cmdp).add(i);
            return -1;
        }
        *top = c_dup(&with);
        // Left pointing *at* the closing delimiter, not past it, which is why
        // the caller does `cmd--` before its loop's `cmd++`.
        *cmdp = (*cmdp).add(i);
        if (*top).is_null() {
            return -1;
        }
        1
    }
}

/// C: `static void replace(char **tmp, int c);`
// [spec:libedit:def:readline.replace-fn]
// [spec:libedit:sem:readline.replace-fn]
fn replace(tmp: *mut *mut c_char, c: c_int) {
    // SAFETY: `*tmp` is a live block from this module.
    unsafe {
        if (*tmp).is_null() {
            return;
        }
        let s = c_bytes(*tmp);
        let Some(i) = s.iter().rposition(|&b| b == c as u8) else {
            return;
        };
        // The C does not check this `strdup` (`// XXX: check`), so on failure
        // `*tmp` becomes NULL after the old buffer was already freed and the
        // caller's modifier loop then dereferences it (ERR-readline-05, UB).
        // Defined here as leaving `*tmp` untouched, so the expansion carries
        // on with the unmodified string.
        let aptr = c_dup(&s[i + 1..]);
        if aptr.is_null() {
            return;
        }
        c_free_str(*tmp);
        *tmp = aptr;
    }
}
/// C: `static int _history_expand_command(const char *command, size_t offs,
/// size_t cmdlen, char **result);`
// [spec:libedit:def:readline.history-expand-command-fn]
// [spec:libedit:sem:readline.history-expand-command-fn]
fn _history_expand_command(
    command: *const c_char,
    offs: usize,
    cmdlen: usize,
    result: *mut *mut c_char,
) -> c_int {
    // SAFETY: `command` is NUL-terminated, `offs` indexes the `!` inside it,
    // and `result` is live.
    unsafe {
        *result = ptr::null_mut();
        let s = c_bytes(command);
        let mut aptr: *mut c_char = ptr::null_mut();
        let mut evptr: *const c_char = ptr::null();
        let mut idx: c_int = 0;
        let has_mods;
        // `search` is initialized to NULL and never assigned, which is what
        // makes every `search`-dependent branch in `getfrom` dead.
        let search: *const c_char = ptr::null();

        /* First get event specifier */
        if strchr(b":^*$", at(s, offs + 1)) {
            /*
             * "!:" is shorthand for "!!:".
             * "!^", "!*" and "!$" are shorthand for
             * "!!:^", "!!:*" and "!!:$" respectively.
             */
            // The C's fourth byte is left uninitialized and never read.
            let shorthand = [b'!' as c_char, b'!' as c_char, b'0' as c_char, 0];
            evptr = get_history_event(shorthand.as_ptr(), &mut idx, 0);
            idx = if at(s, offs + 1) == b':' { 1 } else { 0 };
            has_mods = true;
        } else {
            if at(s, offs + 1) == b'#' {
                /* use command so far */
                aptr = c_dup(&s[..offs.min(s.len())]);
                if aptr.is_null() {
                    return -1;
                }
                idx = 1;
            } else {
                let qchar = if offs > 0 && at(s, offs - 1) == b'"' {
                    b'"' as c_int
                } else {
                    0
                };
                evptr = get_history_event(command.add(offs), &mut idx, qchar);
            }
            has_mods = at(s, offs + idx as usize) == b':';
        }

        if evptr.is_null() && aptr.is_null() {
            return -1;
        }

        if !has_mods {
            let src = if aptr.is_null() {
                c_bytes(evptr)
            } else {
                c_bytes(aptr)
            };
            *result = c_dup(src);
            c_free_str(aptr);
            if (*result).is_null() {
                return -1;
            }
            return 1;
        }

        // Just past the `:`.
        let base = offs + idx as usize + 1;
        let mut cmd = base;
        let mut tmp: *mut c_char;

        /* Now parse any word designators */

        if at(s, cmd) == b'%' {
            /* last word matched by ?pat? */
            // The C does not check this `strdup`.
            tmp = LAST_SEARCH_MATCH.with_borrow(|m| c_dup(m.as_deref().unwrap_or(b"")));
        } else if strchr(b"^*$-0123456789", at(s, cmd)) {
            let mut start: c_int = -1;
            let mut end: c_int = -1;
            match at(s, cmd) {
                b'^' => {
                    start = 1;
                    end = 1;
                    cmd += 1;
                }
                b'$' => {
                    start = -1;
                    cmd += 1;
                }
                b'*' => {
                    start = 1;
                    cmd += 1;
                }
                c if c == b'-' || c.is_ascii_digit() => {
                    // A leading `-` with no digits leaves `start = 0`.
                    start = 0;
                    while at(s, cmd).is_ascii_digit() {
                        start = start * 10 + (at(s, cmd) - b'0') as c_int;
                        cmd += 1;
                    }
                    if at(s, cmd) == b'-' {
                        if at(s, cmd + 1).is_ascii_digit() {
                            cmd += 1;
                            end = 0;
                            while at(s, cmd).is_ascii_digit() {
                                end = end * 10 + (at(s, cmd) - b'0') as c_int;
                                cmd += 1;
                            }
                        } else if at(s, cmd + 1) == b'$' {
                            cmd += 2;
                            end = -1;
                        } else {
                            cmd += 1;
                            end = -2;
                        }
                    } else if at(s, cmd) == b'*' {
                        end = -1;
                        cmd += 1;
                    } else {
                        end = start;
                    }
                }
                _ => {}
            }
            tmp = history_arg_extract(start, end, if aptr.is_null() { evptr } else { aptr });
            if tmp.is_null() {
                // No trailing newline, as in the C.
                let mut msg = s[(offs + idx as usize).min(s.len())..].to_vec();
                msg.extend_from_slice(b": Bad word specifier");
                rl_out_write(&msg);
                c_free_str(aptr);
                return -1;
            }
        } else {
            // The C does not check this `strdup` either.
            tmp = c_dup(if aptr.is_null() {
                c_bytes(evptr)
            } else {
                c_bytes(aptr)
            });
        }

        c_free_str(aptr);

        if at(s, cmd) == 0 || cmd - offs >= cmdlen {
            *result = tmp;
            return 1;
        }

        let mut p_on = false;
        let mut g_on: c_int = 0;
        // The modifier loop's error exit carries `getfrom`/`getto`'s status.
        let mut ev: c_int = -1;
        let mut failed = false;

        'modifiers: while at(s, cmd) != 0 {
            match at(s, cmd) {
                b':' => {}
                b'h' => {
                    /* remove trailing path */
                    truncate_at(tmp, b'/');
                }
                b't' => {
                    /* remove leading path */
                    replace(&raw mut tmp, b'/' as c_int);
                }
                b'r' => {
                    /* remove trailing suffix */
                    truncate_at(tmp, b'.');
                }
                b'e' => {
                    /* remove all but suffix */
                    replace(&raw mut tmp, b'.' as c_int);
                }
                b'p' => {
                    /* print only */
                    p_on = true;
                }
                b'g' => {
                    // 2, not 1; `_rl_compat_sub` only tests it for truth.
                    g_on = 2;
                }
                c @ (b'&' | b's') => {
                    // `&` falls through into `s`, which immediately takes the
                    // *next* character as the delimiter — so it does not
                    // repeat the previous substitution the way GNU readline's
                    // does, and a bare `!!:&` reads NUL as the delimiter and
                    // errors out (ERR-readline-51, reproduced).
                    if c == b'&' && (EXPAND_FROM.is_null() || EXPAND_TO.is_null()) {
                        cmd += 1;
                        continue 'modifiers;
                    }
                    ev = -1;
                    cmd += 1;
                    let delim = at(s, cmd);
                    if delim == 0 {
                        failed = true;
                        break 'modifiers;
                    }
                    cmd += 1;
                    if at(s, cmd) == 0 {
                        failed = true;
                        break 'modifiers;
                    }

                    let mut cmdp = command.add(cmd);
                    ev = getfrom(&mut cmdp, &raw mut EXPAND_FROM, search, delim as c_int);
                    if ev != 1 {
                        failed = true;
                        break 'modifiers;
                    }
                    ev = getto(&mut cmdp, &raw mut EXPAND_TO, EXPAND_FROM, delim as c_int);
                    if ev != 1 {
                        failed = true;
                        break 'modifiers;
                    }
                    cmd = cmdp.offset_from(command) as usize;

                    // An allocation failure inside `_rl_compat_sub` silently
                    // leaves `tmp` unsubstituted.
                    let aptr = _rl_compat_sub(tmp, EXPAND_FROM, EXPAND_TO, g_on);
                    if !aptr.is_null() {
                        c_free_str(tmp);
                        tmp = aptr;
                    }
                    g_on = 0;
                    // `getto` leaves the index on the closing delimiter; the
                    // C's `cmd--` compensates for the loop's `cmd++`.
                    cmd -= 1;
                }
                // Any character with no case is silently ignored.
                _ => {}
            }
            cmd += 1;
        }

        if failed {
            c_free_str(tmp);
            return ev;
        }

        *result = tmp;
        if p_on { 2 } else { 1 }
    }
}

/// `if ((aptr = strrchr(tmp, c)) != NULL) *aptr = '\0';` — the `:h` and `:r`
/// modifiers, which truncate in place rather than reallocating.
///
/// # Safety
///
/// `tmp` must be NULL or a live NUL-terminated string.
unsafe fn truncate_at(tmp: *mut c_char, c: u8) {
    if tmp.is_null() {
        return;
    }
    // SAFETY: the caller guarantees a live NUL-terminated string.
    unsafe {
        let s = c_bytes(tmp);
        if let Some(i) = s.iter().rposition(|&b| b == c) {
            *tmp.add(i) = 0;
        }
    }
}

// [spec:libedit:def:readline.history-expand-fn]
// [spec:libedit:sem:readline.history-expand-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_expand(str_: *mut c_char, output: *mut *mut c_char) -> c_int {
    // The C writes through `output` on every path without ever checking it,
    // so a NULL out-parameter is a null store — measured, it segfaults.
    // Rejected here instead: there is nowhere to put the answer, so there is
    // no answer to give. Unregistered UB, found by `conformance/ub.sh`.
    if output.is_null() {
        return -1;
    }
    let mut ret: c_int = 0;
    // SAFETY: `str_` is a NUL-terminated string owned by the caller and
    // `output` is live.
    unsafe {
        lazy_init();

        if history_expansion_char == 0 {
            // Unchecked, so `*output` may be NULL.
            *output = c_dup(c_bytes(str_));
            return 0;
        }

        *output = ptr::null_mut();
        // The scan below mutates only this private copy, never the caller's
        // buffer, even though the parameter is non-const `char *`.
        if at(c_bytes(str_), 0) == history_subst_char as u8 {
            /* ^foo^foo2^ is equivalent to !!:s^foo^foo2^ */
            let mut rewritten = vec![history_expansion_char as u8; 2];
            rewritten.push(b':');
            rewritten.push(b's');
            rewritten.extend_from_slice(c_bytes(str_));
            *output = c_dup(&rewritten);
        } else {
            *output = c_dup(c_bytes(str_));
        }
        if (*output).is_null() {
            return 0;
        }
        // The working copy, which is what the scan walks and edits.
        let work = *output;

        // The C's `result` starts NULL and stays NULL when nothing is ever
        // appended, which is why `history_expand("")` returns 0 with `*output`
        // NULL rather than an empty string (ERR-readline-29, reproduced).
        let mut result: Option<Vec<u8>> = None;
        let mut i: usize = 0;
        while at(c_bytes(work), i) != 0 {
            let mut qchar: u8 = 0;
            let mut loop_again = true;
            let start = i;
            let mut j = i;

            loop {
                // A two-pass scan: the first finds a live `!`, the second
                // finds the end of the reference it introduces.
                loop {
                    let s = c_bytes(work);
                    if at(s, j) == 0 {
                        break;
                    }
                    if at(s, j) == b'\\' && at(s, j + 1) == history_expansion_char as u8 {
                        // `\!` is emitted as a literal `!` and never expanded:
                        // the backslash is deleted in place and the C's
                        // `continue` still runs its `j++`, so the scan resumes
                        // past the `!` it uncovered.
                        let len = s.len() - (j + 1) + 1;
                        let p = work.cast::<u8>().add(j);
                        ptr::copy(p.add(1), p, len);
                        j += 1;
                        continue;
                    }
                    if !loop_again && (isspace(at(s, j)) || at(s, j) == qchar) {
                        break;
                    }
                    // `strchr` matches the terminating NUL, so a `!` at end of
                    // line never triggers expansion.
                    if at(s, j) == history_expansion_char as u8
                        && !strchr(c_bytes(history_no_expand_chars), at(s, j + 1))
                        && history_inhibit_expansion_function
                            .is_none_or(|f| f(work, j as c_int) == 0)
                    {
                        break;
                    }
                    j += 1;
                }

                if at(c_bytes(work), j) != 0 && loop_again {
                    i = j;
                    qchar = if j > 0 && at(c_bytes(work), j - 1) == b'"' {
                        b'"'
                    } else {
                        0
                    };
                    j += 1;
                    if at(c_bytes(work), j) == history_expansion_char as u8 {
                        j += 1;
                    }
                    loop_again = false;
                    continue;
                }
                break;
            }

            let s = c_bytes(work);
            result
                .get_or_insert_with(Vec::new)
                .extend_from_slice(&s[start..i.min(s.len())]);

            if at(s, i) == 0 || at(s, i) != history_expansion_char as u8 {
                let s = c_bytes(work);
                result
                    .get_or_insert_with(Vec::new)
                    .extend_from_slice(&s[i.min(s.len())..j.min(s.len())]);
                ret = if start == 0 { 0 } else { 1 };
                break;
            }

            let mut tmp: *mut c_char = ptr::null_mut();
            ret = _history_expand_command(work, i, j - i, &mut tmp);
            if ret > 0 && !tmp.is_null() {
                result
                    .get_or_insert_with(Vec::new)
                    .extend_from_slice(c_bytes(tmp));
            }
            c_free_str(tmp);
            i = j;
        }

        /* ret is 2 for "print only" option */
        if ret == 2
            && let Some(r) = result.as_deref()
        {
            let line = c_dup(r);
            if !line.is_null() {
                add_history(line);
                c_free_str(line);
            }
        }
        c_free_str(work);
        *output = match result {
            Some(r) => c_dup(&r),
            None => ptr::null_mut(),
        };

        ret
    }
}

// [spec:libedit:def:readline.history-arg-extract-fn]
// [spec:libedit:sem:readline.history-arg-extract-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_arg_extract(
    start: c_int,
    end: c_int,
    str_: *const c_char,
) -> *mut c_char {
    // SAFETY: `str_` is a NUL-terminated string owned by the caller; the token
    // array below is this function's own and is released on every path.
    unsafe {
        let arr = history_tokenize(str_);
        if arr.is_null() {
            return ptr::null_mut();
        }

        let count = c_array_len(arr);

        let mut result: *mut c_char = ptr::null_mut();
        // An array that is present but empty takes the cleanup exit.
        if count > 0 {
            // `max` is the index of the last word, so word 0 is the command
            // itself and `max` is `$`.
            let max = (count - 1) as c_int;
            let mut start = start;
            let mut end = end;

            // A vestigial hack: the only in-tree caller passes -1 for `$`,
            // never the character, so a genuine word index of 36 is silently
            // reinterpreted as "last word" (ERR-readline-30, reproduced).
            if start == b'$' as c_int {
                start = max;
            }
            if end == b'$' as c_int {
                end = max;
            }
            if end < 0 {
                end = max + end + 1;
            }
            if start < 0 {
                start = end;
            }

            if !(start < 0 || end < 0 || start > max || end > max || start > end) {
                let mut joined: Vec<u8> = Vec::new();
                for i in start..=end {
                    joined.extend_from_slice(c_bytes(*arr.add(i as usize)));
                    if i < end {
                        // Original whitespace and quoting between the words is
                        // not preserved.
                        joined.push(b' ');
                    }
                }
                result = c_dup(&joined);
            }
        }

        c_free_each(core::slice::from_raw_parts(arr, count));
        c_free_array(arr, count + 1);

        result
    }
}

// [spec:libedit:def:readline.history-tokenize-fn]
// [spec:libedit:sem:readline.history-tokenize-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_tokenize(str_: *const c_char) -> *mut *mut c_char {
    // SAFETY: `str_` is a NUL-terminated string owned by the caller.
    unsafe {
        let s = c_bytes(str_);
        let mut tokens: Vec<*mut c_char> = Vec::new();
        let mut delim: u8 = 0;
        let mut i = 0;

        while at(s, i) != 0 {
            while isspace(at(s, i)) {
                i += 1;
            }
            let start = i;
            while at(s, i) != 0 {
                let c = at(s, i);
                if c == b'\\' {
                    // The backslash is kept in the token text.
                    if at(s, i + 1) != 0 {
                        i += 1;
                    }
                } else if c == delim {
                    delim = 0;
                } else if delim == 0 && (isspace(c) || strchr_nonul(b"()<>;&|$", c)) {
                    // The shell metacharacters terminate a word but are not
                    // emitted as words of their own, and the scanner then
                    // skips one character, so a metacharacter between two
                    // words yields an extra empty token (ERR-readline-52).
                    break;
                } else if delim == 0 && strchr_nonul(b"'`\"", c) {
                    delim = c;
                }
                if at(s, i) != 0 {
                    i += 1;
                }
            }

            let temp = c_dup(&s[start.min(s.len())..i.min(s.len())]);
            if temp.is_null() {
                c_free_each(&tokens);
                return ptr::null_mut();
            }
            tokens.push(temp);
            if at(s, i) != 0 {
                i += 1;
            }
        }

        // An empty input never enters the loop, so the C returns NULL rather
        // than an empty array; `"   "` produces one empty token.
        if tokens.is_empty() {
            return ptr::null_mut();
        }

        let out: *mut *mut c_char = c_alloc_array(tokens.len() + 1);
        if out.is_null() {
            c_free_each(&tokens);
            return ptr::null_mut();
        }
        for (i, t) in tokens.iter().enumerate() {
            *out.add(i) = *t;
        }
        // The array is always terminated.
        *out.add(tokens.len()) = ptr::null_mut();
        out
    }
}

/// `strchr(s, c) != NULL` for a set that must *not* match the terminator —
/// the two `history_tokenize` scans, where the C guards the call with its own
/// `str[i]` test first.
fn strchr_nonul(s: &[u8], c: u8) -> bool {
    c != 0 && s.contains(&c)
}

// [spec:libedit:def:readline.stifle-history-fn]
// [spec:libedit:sem:readline.stifle-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stifle_history(max: c_int) {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        if history_va(H, &mut ev, H_SETSIZE, max) == 0 {
            max_input_history = max;
            if history_length > max {
                // Keeps subsequent `history_get` indices aligned with the
                // surviving entries.
                history_base = history_length - max;
            }
            while history_length > max {
                let he = remove_history(0);
                // The C dereferences this without a check (ERR-readline-08,
                // UB); defined here as ending the eviction loop.
                if he.is_null() {
                    break;
                }
                // The only place in the file that disposes of a
                // `remove_history` result at all, so the disposal is kept —
                // including `he->data`, which the *application* allocated and
                // the readline API never promised came from this allocator.
                // What the port drops is the C's `(unsigned long)` cast used
                // to strip `const` off `he->line` (ERR-readline-08).
                c_free((*he).data.cast(), 1);
                c_free_str((*he).line.cast_mut());
                c_free_array(he, 1);
            }
        }
    }
}

// [spec:libedit:def:readline.unstifle-history-fn]
// [spec:libedit:sem:readline.unstifle-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unstifle_history() -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        // There is no lazy-init guard here, unlike almost every other history
        // entry point, so the C dereferences a NULL history when this is the
        // first call an application makes — "a plausible thing to do at
        // startup" (ERR-readline-11, UB). Defined here by skipping the
        // history call; the mirror is still swapped, which is all the return
        // value reports.
        if !H.is_null() {
            history_va(H, &mut ev, H_SETSIZE, c_int::MAX);
        }
        let omax = max_input_history;
        max_input_history = c_int::MAX;
        omax /* some value _must_ be returned */
    }
}

// [spec:libedit:def:readline.history-is-stifled-fn]
// [spec:libedit:sem:readline.history-is-stifled-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_is_stifled() -> c_int {
    // SAFETY: a plain read of a module static.
    unsafe {
        /* cannot return true answer */
        // Inspects only the mirror, so `stifle_history(INT_MAX)` reports
        // *not* stifled and an `H_SETSIZE` set directly is invisible
        // (ERR-readline-38, reproduced).
        c_int::from(max_input_history != c_int::MAX)
    }
}
/// C: `static const char _history_tmp_template[] = "/tmp/.historyXXXXXX";`
const HISTORY_TMP_TEMPLATE: &str = "/tmp/.historyXXXXXX";

/// `mkstemp(template)` — a fresh file in `/tmp`, created exclusively.
///
/// The scratch file is always in `/tmp` regardless of where the history file
/// lives, so private history contents transit a world-writable directory and
/// the operation fails if `/tmp` is not writable. That is observable, so it
/// is preserved rather than quietly moved next to the target
/// (ERR-readline-14).
fn mkstemp() -> std::io::Result<(std::fs::File, std::path::PathBuf)> {
    let stem = HISTORY_TMP_TEMPLATE.trim_end_matches('X');
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as u64 ^ d.as_secs());
    let mut last = std::io::Error::from(std::io::ErrorKind::AlreadyExists);
    for _ in 0..64 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let name = format!("{stem}{:06}", seed % 1_000_000);
        match std::fs::File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&name)
        {
            Ok(f) => return Ok((f, std::path::PathBuf::from(name))),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// The C's `return errno;` after a failing stdio call.
fn errno_of(e: &std::io::Error) -> c_int {
    e.raw_os_error().unwrap_or(EINVAL)
}

/// The path argument these four entry points share: the caller's, or
/// `_default_history_file()`, or the C's `return errno` when there is
/// neither.
///
/// # Safety
///
/// `filename` must be NULL or a NUL-terminated string. The result borrows
/// from either the caller's string or the process-lifetime cache
/// `_default_history_file` keeps.
unsafe fn history_file_name(filename: *const c_char) -> Result<*const c_char, c_int> {
    if !filename.is_null() {
        return Ok(filename);
    }
    let d = _default_history_file();
    if d.is_null() {
        // The C's "whatever `getpwuid` left", read from the C's own `errno`
        // because that is where the failing lookup wrote.
        return Err(crate::errno::get());
    }
    Ok(d)
}

/// [`history_file_name`] for the two entry points that open the file
/// themselves rather than handing the name to `history()`.
///
/// # Safety
///
/// As [`history_file_name`].
unsafe fn history_file_path(filename: *const c_char) -> Result<std::path::PathBuf, c_int> {
    // SAFETY: the caller guarantees the string.
    let name = unsafe { c_bytes(history_file_name(filename)?) };
    Ok(std::path::PathBuf::from(
        String::from_utf8_lossy(name).into_owned(),
    ))
}

// [spec:libedit:def:readline.history-truncate-file-fn]
// [spec:libedit:sem:readline.history-truncate-file-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_truncate_file(filename: *const c_char, nlines: c_int) -> c_int {
    // SAFETY: `filename` is NULL or a NUL-terminated string.
    let path = match unsafe { history_file_path(filename) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    let mut fp = match std::fs::File::options().read(true).write(true).open(&path) {
        Ok(f) => f,
        Err(e) => return errno_of(&e),
    };
    let (mut tp, template) = match mkstemp() {
        Ok(t) => t,
        Err(e) => return errno_of(&e),
    };

    // The whole operation, with the temporary always unlinked on the way out —
    // the C's `out3`/`out2`/`out1` chain, which the success path falls through
    // as well.
    let ret = truncate_through_temp(&mut fp, &mut tp, nlines);
    drop(tp);
    let _ = std::fs::remove_file(&template);
    ret
}

/// The three phases of [`history_truncate_file`], with the file handles
/// already open.
fn truncate_through_temp(fp: &mut std::fs::File, tp: &mut std::fs::File, nlines: c_int) -> c_int {
    // Phase 1 — copy the whole file to the temporary. The C does this in
    // 4096-byte blocks and keeps the tail block; the block bookkeeping is not
    // observable, so the copy is whole here.
    let mut buf = Vec::new();
    if let Err(e) = fp.read_to_end(&mut buf) {
        return errno_of(&e);
    }
    if let Err(e) = tp.write_all(&buf) {
        return errno_of(&e);
    }
    if let Err(e) = tp.flush() {
        return errno_of(&e);
    }

    // `nlines <= 0` cannot terminate the C's backward scan — the first
    // decrement makes it negative, it never reaches 0, and the cut point ends
    // up one byte before a block boundary, i.e. arbitrary (ERR-readline-14,
    // UB). Defined here as leaving the file alone.
    if nlines <= 0 || buf.is_empty() {
        return 0;
    }

    // Phase 2 — walk backwards counting newlines. The scan starts one before
    // the last byte when that byte is a newline, so a file's final newline is
    // not counted, and stops just past the newline that brings the count to
    // zero.
    let mut nlines = nlines;
    let mut i = if *buf.last().unwrap() == b'\n' {
        buf.len() - 1
    } else {
        buf.len()
    };
    let mut cut = None;
    while i > 0 {
        i -= 1;
        if buf[i] == b'\n' {
            nlines -= 1;
            if nlines == 0 {
                cut = Some(i + 1);
                break;
            }
        }
    }
    // "File shorter than requested" is a silent success that changes nothing.
    let Some(cut) = cut else {
        return 0;
    };

    // Phase 3 — copy the retained tail back over the original. The C inspects
    // `ferror(fp)` where it should inspect `ferror(tp)`, so a read error on
    // the temporary is reported as success (ERR-readline-14, reproduced): the
    // read here is from the buffer already in hand, which cannot fail, and
    // the equivalent silent success is the empty-tail case.
    let mut ret = 0;
    if let Err(e) = fp.seek(SeekFrom::Start(0)) {
        return errno_of(&e);
    }
    if let Err(e) = fp.write_all(&buf[cut..]) {
        ret = errno_of(&e);
    }
    let _ = fp.flush();
    // The rewrite is not atomic: a crash between the seek and the truncate
    // leaves a corrupted history file, which is the C's exposure too.
    if let Ok(off) = fp.stream_position()
        && off > 0
    {
        let _ = fp.set_len(off);
    }
    ret
}

// [spec:libedit:def:readline.read-history-fn]
// [spec:libedit:sem:readline.read-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read_history(filename: *const c_char) -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; `filename` is NULL or a
    // NUL-terminated string.
    unsafe {
        lazy_init();
        let name = match history_file_name(filename) {
            Ok(n) => n,
            Err(e) => return e,
        };
        // The C's `errno = 0`, cleared in both homes so that neither a stale
        // platform value nor a stale core one can be mistaken for this call's
        // failure. The sample is taken after it, or the clear itself would
        // count as something to publish.
        crate::errno::set(0);
        let mark = crate::errno::mark();
        if history_va(H, &mut ev, H_LOAD, name) == -1 {
            // Whatever failed wrote one of the two homes — a decoder in the
            // history layer writes the core's, a failing `open` or `read`
            // writes the platform's — so they are reconciled before the read.
            crate::errno::publish(mark);
            let e = crate::errno::get();
            return if e != 0 { e } else { EINVAL };
        }
        if history_va(H, &mut ev, H_GETSIZE) == 0 {
            // `history_base` and `history_offset` are *not* adjusted, so
            // callers are expected to follow with `using_history()`
            // (ERR-readline-40).
            history_length = ev.num;
        }
        if history_length < 0 {
            return EINVAL;
        }
        0
    }
}

// [spec:libedit:def:readline.write-history-fn]
// [spec:libedit:sem:readline.write-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn write_history(filename: *const c_char) -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: as `read_history`.
    unsafe {
        lazy_init();
        let name = match history_file_name(filename) {
            Ok(n) => n,
            Err(e) => return e,
        };
        // No `errno = 0` here — unlike `read_history` the C does not clear it,
        // so a value left over from before the call is what a failure with no
        // `errno` of its own reports. Reproduced by sampling without clearing.
        let mark = crate::errno::mark();
        // H_SAVE truncates or creates the file and writes the signature line
        // and every event `strvis`-escaped — the frozen on-disk format.
        if history_va(H, &mut ev, H_SAVE, name) == -1 {
            crate::errno::publish(mark);
            let e = crate::errno::get();
            if e != 0 { e } else { EINVAL }
        } else {
            0
        }
    }
}

// [spec:libedit:def:readline.append-history-fn]
// [spec:libedit:sem:readline.append-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn append_history(n: c_int, filename: *const c_char) -> c_int {
    // No [`EMPTY_EVENT`] here. The C declares one for its `H_NSAVE_FP` call,
    // and this is the one history entry point that does not make that call —
    // see the note on `history_save_fd` below — so there is no out-parameter
    // to declare.
    // SAFETY: as `read_history`.
    unsafe {
        lazy_init();
        let path = match history_file_path(filename) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let mut fp = match std::fs::File::options()
            .append(true)
            .create(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) => return errno_of(&e),
        };
        // Gap, and a different one from the rest of this crate's `FILE *`
        // work. `crate::cstdio` reads and writes a stream the *application*
        // owns; this function has to *make* one, for a file it opened itself,
        // and `fopen`/`fdopen` are not on `plan/decisions/no-c-ffi.md`'s
        // enumeration — the argument there is about a caller's opaque object,
        // which this is not.
        //
        // The route out is `nshedit::history::history_save_fd`, which is
        // `history_save_fp` with the caller's stream replaced by a descriptor
        // — exactly the shape this function has: it opened the file ITSELF, so
        // there is no application-owned `FILE *` to respect and nothing on
        // `no-c-ffi`'s enumeration to reach for. That distinction is the whole
        // reason the decision permits this and not `fdopen`.
        //
        // Neither a NULL nor a fabricated cookie is an alternative. A NULL one
        // used to go over to `H_NSAVE_FP`, which then failed its cookie write,
        // so `append_history` returned EINVAL and the file it had just opened
        // was closed empty — measured by `conformance/driver/readline_api.c`,
        // which is what turned "a gap recorded here" into "this function does
        // not work". A fabricated pointer is worse: the dispatcher calls the
        // real `ftell` on whatever arrives, so a `Box<File>` cast to `CFile`
        // would be handed to the C library as a `FILE *`.
        //
        // The descriptor is borrowed: `fp` still owns it and closes it below,
        // as the C's `fclose` does.
        //
        // Seek it to the end first, and this is load-bearing rather than
        // tidiness. `history_save_fd` decides whether to write the
        // `_HiStOrY_V2_` cookie from `ftell(fp) == 0`, faithfully to the C.
        // The C reaches it through `fopen(filename, "a")`, and glibc
        // positions an append stream at EOF, so `ftell` reports the size. A
        // raw `O_APPEND` descriptor instead sits at offset 0 until its first
        // write — so without this, appending to a history file writes a
        // SECOND cookie into the middle of it, which `history_load` then
        // reads as an entry. Measured: the port did exactly that.
        //
        // `O_APPEND` already forces every write to the end, so seeking
        // changes only what the position REPORTS, which is precisely the
        // question `at_start` is asking.
        let _ = fp.seek(SeekFrom::End(0));
        // As `write_history`: sampled, not cleared.
        let mark = crate::errno::mark();
        let rc = nshedit::history::history_save_fd(&mut *H, n as usize, fp.as_raw_fd());
        // The C captures `errno` before `fclose`, which can overwrite it.
        let e = if rc == -1 {
            crate::errno::publish(mark);
            crate::errno::get()
        } else {
            0
        };
        // The file this function opened is the one it closes.
        drop(fp);
        if rc == -1 {
            return if e != 0 { e } else { EINVAL };
        }
        0
    }
}

// [spec:libedit:def:readline.history-get-fn]
// [spec:libedit:sem:readline.history-get-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_get(num: c_int) -> *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; the returned entry is the shared
    // `she` static, invalidated by the next call.
    unsafe {
        lazy_init();

        // Indices are 1-based against `history_base`, which `add_history` and
        // `stifle_history` bump as old events are evicted.
        if num < history_base {
            return ptr::null_mut();
        }

        /* save current position */
        if history_va(H, &mut ev, H_CURR) != 0 {
            return ptr::null_mut();
        }
        let curr_num = ev.num;

        let she = SHE.with(Cell::as_ptr);
        let mut ok = false;
        /*
         * use H_DELDATA to set to nth history (without delete) by passing
         * (void **)-1  -- as in history_set_pos
         */
        let sentinel = ptr::without_provenance_mut::<*mut c_void>(usize::MAX);
        if history_va(H, &mut ev, H_DELDATA, num - history_base, sentinel) == 0
            && history_va(H, &mut ev, H_CURR) == 0
            && history_va(H, &mut ev, H_NEXT_EVDATA, ev.num, &raw mut (*she).data) == 0
        {
            (*she).line = ev.str;
            ok = true;
        }

        /* restore pointer to where it was */
        history_va(H, &mut ev, H_SET, curr_num);

        if ok { she } else { ptr::null_mut() }
    }
}

// [spec:libedit:def:readline.add-history-fn]
// [spec:libedit:sem:readline.add-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn add_history(line: *const c_char) -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; `line` is copied by the history.
    unsafe {
        lazy_init();

        if history_va(H, &mut ev, H_ENTER, line) == -1 {
            return 0;
        }

        history_va(H, &mut ev, H_GETSIZE);
        if ev.num == history_length {
            // The count did not change, so the list was at its cap and the
            // oldest event was evicted — except that duplicate suppression
            // leaves the count unchanged too, and bumps the base as though an
            // eviction had happened (ERR-readline-27, reproduced).
            history_base += 1;
        } else {
            history_offset += 1;
            history_length = ev.num;
        }
        // Always 0, so success and allocation failure are indistinguishable.
        0
    }
}

// [spec:libedit:def:readline.remove-history-fn]
// [spec:libedit:sem:readline.remove-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remove_history(num: c_int) -> *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; the returned entry and its line
    // are heap blocks the caller owns.
    unsafe {
        lazy_init();

        let he: *mut HistEntry = c_alloc_array(1);
        if he.is_null() {
            return ptr::null_mut();
        }
        (*he).line = ptr::null();
        (*he).data = ptr::null_mut();

        if history_va(H, &mut ev, H_DELDATA, num, &raw mut (*he).data) != 0 {
            c_free_array(he, 1);
            return ptr::null_mut();
        }

        // The fresh copy H_DELDATA made, so this string is owned by the
        // caller — and leaked by the documented `free_history_entry` idiom,
        // which frees nothing (ERR-readline-15).
        (*he).line = ev.str;
        if history_va(H, &mut ev, H_GETSIZE) == 0 {
            history_length = ev.num;
        }
        // `history_base` and `history_offset` are not adjusted.

        he
    }
}

// [spec:libedit:def:readline.replace-history-entry-fn]
// [spec:libedit:sem:readline.replace-history-entry-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn replace_history_entry(
    num: c_int,
    line: *const c_char,
    data: HistdataT,
) -> *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        /* save current position */
        if history_va(H, &mut ev, H_CURR) != 0 {
            return ptr::null_mut();
        }
        let curr_num = ev.num;

        /* start from the oldest */
        if history_va(H, &mut ev, H_LAST) != 0 {
            return ptr::null_mut(); /* error */
        }

        let he: *mut HistEntry = c_alloc_array(1);
        if he.is_null() {
            return ptr::null_mut();
        }
        (*he).line = ptr::null();
        (*he).data = ptr::null_mut();

        // `num` is matched against `ev.num`, libedit's monotonically
        // increasing event id, not against a positional index.
        //
        // H_REPLACE overwrites the entry's string without freeing the old
        // one, so the pointer returned in `he->line` stays valid indefinitely
        // and the old line is leaked by the history layer.
        if history_va(H, &mut ev, H_NEXT_EVDATA, num, &raw mut (*he).data) == 0
            && {
                (*he).line = ev.str;
                !(*he).line.is_null()
            }
            && history_va(H, &mut ev, H_REPLACE, line, data) == 0
            && history_va(H, &mut ev, H_SET, curr_num) == 0
        {
            return he;
        }

        // A late failure leaves the history modified with no way for the
        // caller to learn what the old contents were.
        c_free_array(he, 1);
        ptr::null_mut()
    }
}

// [spec:libedit:def:readline.clear-history-fn]
// [spec:libedit:sem:readline.clear-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clear_history() {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        history_va(H, &mut ev, H_CLEAR);
        // `history_base` is deliberately left where `add_history` and
        // `stifle_history` put it, so after a clear the base can still exceed
        // 1 and `history_get` rejects small indices (ERR-readline-28).
        history_offset = 0;
        history_length = 0;
    }
}

// [spec:libedit:def:readline.where-history-fn]
// [spec:libedit:sem:readline.where-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn where_history() -> c_int {
    // SAFETY: a plain read of a module static, with no validation — the value
    // is stale after `read_history`, `history_search`, `history_search_prefix`
    // or `remove_history` (ERR-readline-40).
    unsafe { history_offset }
}

// [spec:libedit:def:readline.history-list-fn]
// [spec:libedit:sem:readline.history-list-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_list() -> *mut *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state. The arrays, the entries and the
    // line strings are all borrowed; the whole result is invalidated by the
    // next call, and there is no lazy-init guard, as in the C.
    unsafe {
        if H.is_null() || history_va(H, &mut ev, H_LAST) != 0 {
            return ptr::null_mut();
        }

        // The C sizes both arrays from the cached `history_length` and guards
        // the walk with `if (i++ == history_length) abort();` — evaluated
        // *after* the write, so a history longer than the cache writes one
        // element past the end and only then kills the process from inside a
        // library (ERR-readline-03, UB). Defined here by sizing to what the
        // walk actually finds: no out-of-bounds write, no `abort`, and no
        // truncation either.
        HISTORY_LIST.with_borrow_mut(|list| {
            list.clear();
            loop {
                list.push(HistEntry {
                    line: ev.str,
                    data: ptr::null_mut(),
                });
                if history_va(H, &mut ev, H_PREV) != 0 {
                    break;
                }
            }
            HISTORY_LISTP.with_borrow_mut(|listp| {
                listp.clear();
                listp.reserve(list.len() + 1);
                for entry in list.iter_mut() {
                    listp.push(ptr::from_mut(entry));
                }
                listp.push(ptr::null_mut());
                listp.as_mut_ptr()
            })
        })
        // The cursor is left at the newest entry, not restored.
    }
}

// [spec:libedit:def:readline.current-history-fn]
// [spec:libedit:sem:readline.current-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn current_history() -> *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; the result is the shared `rl_he`
    // static, overwritten by the next navigation call.
    unsafe {
        // No lazy-init guard: the C hands a NULL history straight to
        // `history()` (ERR-readline-11, UB), defined here as "no such event".
        //
        // The lookup implies an identity between a zero-based readline offset
        // and a one-based libedit event number, which holds only while event
        // numbering is dense and starts at 1 (ERR-readline-39).
        if H.is_null() || history_va(H, &mut ev, H_PREV_EVENT, history_offset + 1) != 0 {
            return ptr::null_mut();
        }

        let he = RL_HE.with(Cell::as_ptr);
        // Borrowed, including whatever trailing newline the entry was stored
        // with; the entry's real `histdata_t` is never surfaced here.
        (*he).line = ev.str;
        (*he).data = ptr::null_mut();
        he
    }
}

// [spec:libedit:def:readline.history-total-bytes-fn]
// [spec:libedit:sem:readline.history-total-bytes-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_total_bytes() -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        // No lazy-init guard, as in the C (ERR-readline-11, UB): a NULL
        // history takes the documented -1 return instead of faulting.
        if H.is_null() || history_va(H, &mut ev, H_CURR) != 0 {
            return -1;
        }
        let curr_num = ev.num;

        history_va(H, &mut ev, H_FIRST);
        let mut size: usize = 0;
        loop {
            if !ev.str.is_null() {
                // The terminating NUL is not counted, and neither is any
                // per-entry overhead.
                size += c_bytes(ev.str).len();
            }
            if history_va(H, &mut ev, H_NEXT) != 0 {
                break;
            }
        }

        /* get to the same position as before */
        history_va(H, &mut ev, H_PREV_EVENT, curr_num);

        // The cast is unchecked in the C, so a history over INT_MAX bytes
        // yields an implementation-defined value; the wrap is that value on
        // every platform the port targets.
        size as c_int
    }
}

// [spec:libedit:def:readline.history-set-pos-fn]
// [spec:libedit:sem:readline.history-set-pos-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_set_pos(pos: c_int) -> c_int {
    // SAFETY: single-threaded module state.
    unsafe {
        // The upper bound is exclusive, so `pos == history_length` — GNU
        // readline's "the line being typed" — is rejected and a caller cannot
        // use this to return to the end of the list.
        if pos >= history_length || pos < 0 {
            return 0;
        }
        // libedit's internal cursor is *not* moved, which is why
        // `history_search_pos` does not actually start its search at `pos`.
        history_offset = pos;
        1
    }
}

// [spec:libedit:def:readline.previous-history-fn]
// [spec:libedit:sem:readline.previous-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn previous_history() -> *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        // readline's "previous" means further back in time, which is
        // libedit's H_NEXT direction; the op names here cannot be read
        // literally.
        if history_offset == 0 {
            return ptr::null_mut();
        }

        // No lazy-init guard, as in the C (ERR-readline-11, UB).
        if H.is_null() || history_va(H, &mut ev, H_LAST) != 0 {
            return ptr::null_mut();
        }

        history_offset -= 1;
        current_history()
    }
}

// [spec:libedit:def:readline.next-history-fn]
// [spec:libedit:sem:readline.next-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn next_history() -> *mut HistEntry {
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state.
    unsafe {
        if history_offset >= history_length {
            return ptr::null_mut();
        }

        // The H_LAST-then-scan pattern makes every navigation step O(n)
        // (ERR-readline-39).
        if H.is_null() || history_va(H, &mut ev, H_LAST) != 0 {
            return ptr::null_mut();
        }

        history_offset += 1;
        current_history()
    }
}

// [spec:libedit:def:readline.history-search-fn]
// [spec:libedit:sem:readline.history-search-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_search(str_: *const c_char, direction: c_int) -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: `str_` is a NUL-terminated string; single-threaded module state.
    unsafe {
        if H.is_null() || history_va(H, &mut ev, H_CURR) != 0 {
            return -1;
        }
        let curr_num = ev.num;
        let needle = c_bytes(str_);

        loop {
            if !ev.str.is_null()
                && let Some(off) = find_substring(c_bytes(ev.str), needle)
            {
                // The cursor is deliberately left *on the matching entry*,
                // which is how `get_history_event` follows a `!?pat?`
                // reference with an H_CURR read. `history_offset` is not
                // updated (ERR-readline-40).
                return off as c_int;
            }
            // A negative direction uses H_NEXT, which moves toward *older*
            // entries: readline's convention, and the opposite of the plain
            // reading of the libedit op names.
            if history_va(H, &mut ev, if direction < 0 { H_NEXT } else { H_PREV }) != 0 {
                break;
            }
        }
        history_va(H, &mut ev, H_SET, curr_num);
        -1
    }
}

/// `strstr(haystack, needle)` as an offset. An empty needle matches at 0,
/// which is what `strstr` returns for it.
fn find_substring(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// [spec:libedit:def:readline.history-search-prefix-fn]
// [spec:libedit:sem:readline.history-search-prefix-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_search_prefix(str_: *const c_char, direction: c_int) -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: `str_` is a NUL-terminated string; single-threaded module state.
    unsafe {
        // No lazy init, no global read or written, and the event is filled
        // and discarded. 0 when a match was found, -1 when not — which is
        // neither GNU readline's offset nor `history_search`'s
        // (ERR-readline-49).
        if H.is_null() {
            return -1;
        }
        history_va(
            H,
            &mut ev,
            if direction < 0 {
                H_PREV_STR
            } else {
                H_NEXT_STR
            },
            str_,
        )
    }
}

// [spec:libedit:def:readline.history-search-pos-fn]
// [spec:libedit:sem:readline.history-search-pos-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_search_pos(
    str_: *const c_char,
    direction: c_int,
    pos: c_int,
) -> c_int {
    let _ = direction; /* declared unused: the sign of `pos` carries it */
    let mut ev = EMPTY_EVENT;
    // SAFETY: `str_` is a NUL-terminated string; single-threaded module state.
    unsafe {
        let off = if pos > 0 { pos } else { -pos };
        let pos = if pos > 0 { 1 } else { -1 };

        if H.is_null() || history_va(H, &mut ev, H_CURR) != 0 {
            return -1;
        }
        let curr_num = ev.num;

        // `history_set_pos` only assigns the global; it does not move
        // libedit's cursor, so the H_CURR below re-reads the entry the cursor
        // was already on and `off` acts purely as a range check
        // (ERR-readline-22, reproduced). The side effect on `history_offset`
        // survives even when the search then fails.
        if history_set_pos(off) == 0 || history_va(H, &mut ev, H_CURR) != 0 {
            return -1;
        }

        let needle = c_bytes(str_);
        loop {
            if !ev.str.is_null() && find_substring(c_bytes(ev.str), needle).is_some() {
                // The *requested* position, not the position of the match.
                return off;
            }
            if history_va(H, &mut ev, if pos < 0 { H_PREV } else { H_NEXT }) != 0 {
                break;
            }
        }

        /* set "current" pointer back to previous state */
        history_va(
            H,
            &mut ev,
            if pos < 0 { H_NEXT_EVENT } else { H_PREV_EVENT },
            curr_num,
        );

        -1
    }
}
// [spec:libedit:def:readline.tilde-expand-fn]
// [spec:libedit:sem:readline.tilde-expand-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tilde_expand(name: *mut c_char) -> *mut c_char {
    // SAFETY: `name` is a NUL-terminated string; it is read, never modified,
    // despite the non-const parameter readline source compatibility wants.
    // C: `return fn_tilde_expand(name);`
    unsafe { crate::filecomplete::fn_tilde_expand(name) }
}

// [spec:libedit:def:readline.filename-completion-function-fn]
// [spec:libedit:sem:readline.filename-completion-function-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filename_completion_function(
    name: *const c_char,
    state: c_int,
) -> *mut c_char {
    // C: `return fn_filename_completion_function(name, state);`
    // SAFETY: `name` is a NUL-terminated string.
    unsafe { crate::filecomplete::fn_filename_completion_function(name, state) }
}

// [spec:libedit:def:readline.username-completion-function-fn]
// [spec:libedit:sem:readline.username-completion-function-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn username_completion_function(
    text: *const c_char,
    state: c_int,
) -> *mut c_char {
    // SAFETY: `text` is a NUL-terminated string.
    unsafe {
        let mut t = c_bytes(text);
        if t.is_empty() {
            return ptr::null_mut();
        }
        if t[0] == b'~' {
            t = &t[1..];
        }

        // `state == 0` rewinds the database, as the generator protocol
        // requires; a scan started without it continues from wherever the
        // previous one stopped.
        if state == 0 || !PASSWD_SCAN_OPEN.get() {
            nshedit_plat::passwd::setpwent();
            PASSWD_SCAN_OPEN.set(true);
        }

        // The loop condition is inverted: it *continues* while the entry is
        // exactly equal to `text` and stops at the first entry that differs,
        // so it exits after a single `getpwent()` and returns the next
        // database entry regardless of whether it starts with `text`. No
        // prefix matching happens at all (ERR-readline-24, reproduced).
        let mut found = None;
        while let Some(name) = nshedit_plat::passwd::getpwent_name() {
            if name.first() == t.first() && name == t {
                continue;
            }
            found = Some(name);
            break;
        }
        match found {
            // Unchecked in the C, so NULL is also an allocation failure.
            Some(name) => c_dup(&name),
            None => {
                // `endpwent()` is only reached when the database is
                // exhausted, so an abandoned scan leaks the handle in the C;
                // the port inherits that, since the cursor is the libc's.
                nshedit_plat::passwd::endpwent();
                PASSWD_SCAN_OPEN.set(false);
                ptr::null_mut()
            }
        }
    }
}

/// C: `static unsigned char _el_rl_tstp(EditLine *el, int ch);` — the
/// `ED_TTY_SIGTSTP`-alike bound to `^Z`.
// [spec:libedit:def:readline.el-rl-tstp-fn]
// [spec:libedit:sem:readline.el-rl-tstp-fn]
fn _el_rl_tstp(el: *mut EditLine, ch: c_int) -> c_uchar {
    let _ = (el, ch);
    // The whole body. Terminal state around the stop is EditLine's business:
    // `EL_SIGNAL`'s own `SIGTSTP` handler is what puts the tty back into
    // cooked mode before the process actually stops, and if the application
    // did not turn `EL_SIGNAL` on, nothing does — which is the C's behaviour
    // too. The result is discarded, as the C discards `raise`'s.
    let _ = nshedit_plat::signal::raise(nshedit_plat::signal::signo::SIGTSTP);
    CC_NORM
}

/// C: `static const char *_rl_completion_append_character_function(
/// const char *dummy);` — renders `rl_completion_append_character` for
/// `fn_complete2`.
// [spec:libedit:def:readline.rl-completion-append-character-function-fn]
// [spec:libedit:sem:readline.rl-completion-append-character-function-fn]
fn _rl_completion_append_character_function(dummy: *const c_char) -> *const c_char {
    let _ = dummy;
    // SAFETY: single-threaded module state. The result points at shared
    // static storage that the next call overwrites and that must not be
    // freed; only a single byte can be expressed, so a multibyte append
    // character is truncated, and 0 gives the empty string.
    unsafe {
        APPEND_CHAR_BUF[0] = rl_completion_append_character as c_char;
        APPEND_CHAR_BUF[1] = 0;
        (&raw const APPEND_CHAR_BUF).cast()
    }
}

// [spec:libedit:def:readline.rl-display-match-list-fn]
// [spec:libedit:sem:readline.rl-display-match-list-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_display_match_list(matches: *mut *mut c_char, len: c_int, max: c_int) {
    // SAFETY: `matches` is the caller's readline-shaped array: index 0 is the
    // common prefix and 1..=len are the entries shown. Nothing here is freed
    // and ownership stays with the caller.
    unsafe {
        // No lazy-initialization guard in the C, so `e` must already exist.
        if E.is_null() || matches.is_null() {
            return;
        }
        // The C widens both counts to `size_t` with no range check, so a
        // negative argument becomes an enormous count and the display walks
        // off the array (UB). Defined here as doing nothing.
        if len < 0 || max < 0 {
            return;
        }
        // ERR-completion-02's disposition is "treat it as a caller error and
        // reject it", and `len` is the caller's claim about an array only the
        // caller can measure. What actually bounds it is the NULL terminator
        // the readline contract puts there — `rl_completion_matches` and
        // `completion_matches` both produce one — so the walk stops at the
        // first NULL rather than trusting the count.
        //
        // Without this, `rl_display_match_list(matches, 99, 6)` over a
        // two-element array read 100 pointers and died. Measured by
        // `conformance/ub.sh`, where the C dies on it too; this is the half
        // the erratum says must not.
        //
        // The core takes owned strings and sorts them in place, so the
        // caller's array is permuted afterwards to match — which is what the
        // C's in-place `qsort` leaves behind.
        let claimed = len as usize;
        let mut owned: Vec<String> = Vec::with_capacity(claimed + 1);
        for i in 0..=claimed {
            let p = *matches.add(i);
            if p.is_null() {
                // Index 0 is the common prefix and may legitimately be empty;
                // a NULL past it ends the list.
                if i == 0 {
                    owned.push(String::new());
                    continue;
                }
                break;
            }
            owned.push(String::from_utf8_lossy(c_bytes(p)).into_owned());
        }
        // `owned[0]` is the prefix, so the count of entries is what follows.
        let num = owned.len().saturating_sub(1);

        filecomplete::fn_display_match_list(
            &mut *E,
            &mut owned,
            num,
            max as usize,
            Some(append_char_str),
        );

        crate::filecomplete::permute_to_match(matches, &owned);
    }
}

// [spec:libedit:def:readline.rl-complete-fn]
// [spec:libedit:sem:readline.rl-complete-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_complete(ignore: c_int, invoking_key: c_int) -> c_int {
    let _ = ignore;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        if rl_inhibit_completion != 0 {
            // A disabled Tab inserts a literal Tab.
            let arr = [invoking_key as c_char, 0];
            crate::eln::el_insertstr(E, arr.as_ptr());
            return c_int::from(CC_REFRESH);
        }

        // Read fresh on every completion; nothing is cached, the returned
        // pointer is used directly and never freed, and the hook runs
        // *before* `_rl_update_pos`, so `rl_point`/`rl_end` are stale inside
        // it (ERR-readline-50).
        let breakchars = match rl_completion_word_break_hook {
            Some(hook) => hook().cast_const(),
            None => rl_basic_word_break_characters,
        };

        _rl_update_pos();

        if E.is_null() {
            return c_int::from(CC_ERROR);
        }

        // Out-parameters `fn_complete2` writes; the C hands it the globals
        // directly.
        let mut completion_type = rl_completion_type;
        let mut over = rl_attempted_completion_over;
        let mut point = rl_point;
        let mut end = rl_end;

        let mut generator = |text: &str, state: i32| -> Option<String> {
            let f = rl_completion_entry_function?;
            let ctext = c_dup(text.as_bytes());
            if ctext.is_null() {
                return None;
            }
            let m = f(ctext, state);
            c_free_str(ctext);
            if m.is_null() {
                return None;
            }
            let out = String::from_utf8_lossy(c_bytes(m)).into_owned();
            // The C's `fn_complete2` takes ownership of what the generator
            // returns, so the block is released once copied.
            c_free_str(m);
            Some(out)
        };
        let has_generator = { rl_completion_entry_function }.is_some();
        let attempted = if { rl_attempted_completion_function }.is_some() {
            Some(attempted_completion_adapter as filecomplete::AttemptedCompletionFunc)
        } else {
            None
        };

        /* Just look at how many global variables modify this operation! */
        let rc = WBREAK_CONV.with_borrow_mut(|wconv| {
            SPREFIX_CONV.with_borrow_mut(|sconv| {
                // The *word-break* argument is always
                // `rl_basic_word_break_characters`; the hook's result goes
                // into the *special-prefixes* slot, so it can only ever add
                // break characters, never replace them — and
                // `rl_completer_word_break_characters` and
                // `rl_special_prefixes` are not consulted at all
                // (ERR-readline-50, reproduced).
                let word_break =
                    ct_decode_string(c_bytes_opt(rl_basic_word_break_characters), wconv)
                        .unwrap_or(&[]);
                let special = ct_decode_string(c_bytes_opt(breakchars), sconv);
                let generator: Option<&mut filecomplete::CompleteFunc> = if has_generator {
                    Some(&mut generator)
                } else {
                    None
                };
                filecomplete::fn_complete2(
                    &mut *E,
                    generator,
                    attempted,
                    word_break,
                    special,
                    Some(append_char_str),
                    rl_completion_query_items as usize,
                    Some(&mut completion_type),
                    Some(&mut over),
                    Some(&mut point),
                    Some(&mut end),
                    0, /* no FN_QUOTE_MATCH: matches are not re-quoted */
                )
            })
        });

        rl_completion_type = completion_type;
        rl_attempted_completion_over = over;
        rl_point = point;
        rl_end = end;

        // A libedit CC_* code, not readline's 0/non-zero status, because the
        // function doubles as an EditLine command through `_el_rl_complete`.
        rc
    }
}

/// C: `static unsigned char _el_rl_complete(EditLine *el, int ch);` — the
/// editor command bound to TAB, which calls `rl_complete`.
// [spec:libedit:def:readline.el-rl-complete-fn]
// [spec:libedit:sem:readline.el-rl-complete-fn]
fn _el_rl_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    let _ = el;
    // The first argument, readline's ignored `count`, is hardcoded to 0. Every
    // CC_* value is small, so the narrowing is lossless in practice.
    // SAFETY: `rl_complete` reaches the module statics.
    unsafe { rl_complete(0, ch) as c_uchar }
}

// [spec:libedit:def:readline.rl-bind-key-fn]
// [spec:libedit:sem:readline.rl-bind-key-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_bind_key(
    c: c_int,
    func: Option<unsafe extern "C" fn(c_int, c_int) -> c_int>,
) -> c_int {
    let mut retval = -1;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        // Exactly one function is supported: this library's own `rl_insert`,
        // compared by address (ERR-readline-43). Anything else is silently
        // ignored, so `rl_add_defun` is the only route to a custom binding.
        let is_insert = func.map(|f| f as usize)
            == Some(rl_insert as unsafe extern "C" fn(c_int, c_int) -> c_int as usize);
        if is_insert {
            // The C range-checks nothing, and `el_map.key` is exactly 256
            // entries, so a `c` outside 0..255 — a negative meta key, or EOF —
            // corrupts adjacent `EditLine` state (ERR-readline-04, UB).
            // Defined here by rejecting it, which is what `rl_add_defun`
            // already does with the same range.
            if !(0..256).contains(&c) {
                return -1;
            }
            if E.is_null() {
                return -1;
            }
            let el = &mut *E;
            el.el_map.key[c as usize] = ED_INSERT;
            retval = 0;
        }
        retval
    }
}

// [spec:libedit:def:readline.rl-read-key-fn]
// [spec:libedit:sem:readline.rl-read-key-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_read_key() -> c_int {
    // The C's oversized scratch buffer; `el_getc` writes exactly one byte.
    let mut fooarr = [0 as c_char; 2 * size_of::<c_int>()];
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        // The character is written into the scratch buffer and then dropped:
        // what comes back is `el_getc`'s *status*, so this yields 1 for every
        // successful read regardless of which key was pressed, 0 at EOF and -1
        // on error (ERR-readline-23, reproduced).
        crate::eln::el_getc(E, fooarr.as_mut_ptr())
    }
}

// [spec:libedit:def:readline.rl-reset-terminal-fn]
// [spec:libedit:sem:readline.rl-reset-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_reset_terminal(p: *const c_char) -> c_int {
    // readline's terminal *name*, declared unused and ignored entirely: the
    // port must not re-query terminfo for it here.
    let _ = p;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();
        crate::histedit::el_reset(E);
        0
    }
}

// [spec:libedit:def:readline.rl-insert-fn]
// [spec:libedit:sem:readline.rl-insert-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_insert(count: c_int, c: c_int) -> c_int {
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        /* XXX - int -> char conversion can lose on multichars */
        let arr = [c as c_char, 0];

        // Pushes the keystroke back onto the pending-input queue, so whatever
        // the current binding for it is will run. GNU readline's `rl_insert`
        // inserts into the line instead; the two are swapped relative to
        // readline (ERR-readline-42, reproduced).
        for _ in 0..count {
            crate::eln::el_push(E, arr.as_ptr());
        }

        0
    }
}

// [spec:libedit:def:readline.rl-insert-text-fn]
// [spec:libedit:sem:readline.rl-insert-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_insert_text(text: *const c_char) -> c_int {
    // SAFETY: `text` is NULL or a NUL-terminated string.
    unsafe {
        // Safe to call on an uninitialized library in this case only.
        if text.is_null() || *text == 0 {
            return 0;
        }

        lazy_init();

        if crate::eln::el_insertstr(E, text) < 0 {
            return 0;
        }
        // The number of *bytes*, not of characters, so under a multibyte
        // locale this exceeds what was added to the line (ERR-readline-35).
        // Neither `rl_point` nor `rl_end` is refreshed here.
        c_bytes(text).len() as c_int
    }
}

// [spec:libedit:def:readline.rl-newline-fn]
// [spec:libedit:sem:readline.rl-newline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_newline(count: c_int, c: c_int) -> c_int {
    /*
     * Readline-4.0 appears to ignore the args.
     */
    let _ = (count, c);
    // Pushes `'\n'` onto the pending-input queue, so the effect depends on
    // the current binding of that key rather than accepting the line
    // (ERR-readline-44).
    // SAFETY: `rl_insert` reaches the module statics.
    unsafe { rl_insert(1, b'\n' as c_int) }
}

/// C: `static unsigned char rl_bind_wrapper(EditLine *el, unsigned char c);`
/// — dispatches an editor keystroke into the `map[]` table of
/// `rl_command_func_t`s installed by `rl_bind_key`.
// [spec:libedit:def:readline.rl-bind-wrapper-fn]
// [spec:libedit:sem:readline.rl-bind-wrapper-fn]
fn rl_bind_wrapper(el: *mut EditLine, c: c_uchar) -> c_uchar {
    let _ = el;
    // SAFETY: single-threaded module state.
    unsafe {
        let Some(f) = MAP[c as usize] else {
            return CC_ERROR;
        };

        // Refreshes `rl_point`, `rl_end` and `rl_line_buffer` so the readline
        // function sees consistent state — inbound only. Changes the callback
        // makes to those globals are never written back into EditLine's wide
        // line (ERR-readline-34).
        _rl_update_pos();

        // `count` hardcoded to 1, `key` the invoking byte; the return value is
        // discarded, so a readline command cannot report failure.
        f(1, c as c_int);

        /* If rl_done was set by the above call, deal with it here */
        if rl_done != 0 {
            return CC_EOF;
        }

        CC_NORM
    }
}

// [spec:libedit:def:readline.rl-add-defun-fn]
// [spec:libedit:sem:readline.rl-add-defun-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_add_defun(
    name: *const c_char,
    fun: Option<unsafe extern "C" fn(c_int, c_int) -> c_int>,
    c: c_int,
) -> c_int {
    let mut dest = [0 as c_char; 8];
    // SAFETY: `name` is a NUL-terminated string the callee copies.
    unsafe {
        // C: `(size_t)c >= sizeof(map) / sizeof(map[0]) || c < 0`.
        if !(0..256).contains(&c) {
            return -1;
        }
        // Any previous entry for that byte is silently overwritten, and `fun`
        // is stored raw: the caller must keep the function alive.
        MAP[c as usize] = fun;
        // There is no lazy `rl_initialize()` here, so the C hands a NULL
        // editor to `el_set` and crashes (ERR-readline-11, UB); `el_set`
        // taking the NULL is the port's definition of it.
        el_set_va(E, EL_ADDFN, name, name, rl_bind_wrapper as *const c_void);
        // strvis form: control characters as `^X`, other non-printables as
        // `\nnn`, whitespace encoded, no backslash doubling.
        let vised = nshedit::vis::Encoder::new(nshedit::vis::Flags::from_bits(
            (VIS_WHITE | VIS_NOSLASH) as u32,
        ))
        .encode_byte(c as u8, 0);
        // The C's `char dest[8]`. One byte cannot encode past four, but the
        // copy is bounded rather than assumed.
        for (slot, &b) in dest.iter_mut().zip(&vised[..vised.len().min(7)]) {
            *slot = b as c_char;
        }
        el_set_va(E, EL_BIND, dest.as_ptr(), name, ptr::null::<c_char>());
        // Both `el_set` results are discarded, so a failed registration or
        // binding is not reported.
        0
    }
}

// [spec:libedit:def:readline.rl-callback-read-char-fn]
// [spec:libedit:sem:readline.rl-callback-read-char-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_callback_read_char() {
    // SAFETY: single-threaded module state; `e` must already have been set up
    // by `rl_callback_handler_install`.
    unsafe {
        if E.is_null() {
            // No lazy-initialization guard in the C (ERR-readline-11, UB).
            return;
        }
        let mut count: c_int = 0;
        let mut done = 0;
        // Happens *before* unbuffered mode is re-asserted, so the very first
        // call after installation behaves like a blocking line read
        // (ERR-readline-37).
        let buf = crate::eln::el_gets(E, &mut count);

        el_set_va(E, EL_UNBUFFERED, 1);
        count -= 1;
        if buf.is_null() || count < 0 {
            return;
        }
        let bytes = c_bytes(buf);
        let last = count as usize;

        if count == 0 && !bytes.is_empty() && bytes[0] == (*E).el_tty.t_c[TS_IO][C_EOF] {
            /* a lone EOF keystroke on an empty line */
            done = 1;
        }
        if last < bytes.len() && (bytes[last] == b'\n' || bytes[last] == b'\r') {
            /* a completed line; this overrides the EOF test */
            done = 2;
        }

        let linefunc = rl_linefunc;
        if done != 0 && linefunc.is_some() {
            el_set_va(E, EL_UNBUFFERED, 0);
            let wbuf = if done == 2 {
                let w = c_dup(bytes);
                if !w.is_null() {
                    *w.add(last) = 0;
                }
                // Set but never cleared here; only `rl_initialize` clears it.
                rl_readline_state |= RL_STATE_DONE;
                w
            } else {
                ptr::null_mut()
            };
            // The callback takes ownership of `wbuf` and must `free()` it;
            // NULL means end of input, indistinguishably from a failed copy.
            if let Some(f) = linefunc {
                f(wbuf);
            }
        }
        _rl_update_pos();
    }
}

// [spec:libedit:def:readline.rl-callback-handler-install-fn]
// [spec:libedit:sem:readline.rl-callback-handler-install-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_callback_handler_install(
    prompt: *const c_char,
    linefunc: Option<unsafe extern "C" fn(*mut c_char)>,
) {
    // SAFETY: single-threaded module state.
    unsafe {
        // Note the guard tests only `e`, unlike every other lazy-init site.
        if E.is_null() {
            rl_initialize();
        }
        // Return value discarded, so an allocation failure goes unreported.
        rl_set_prompt(prompt);
        // Installing a second handler simply overwrites this; there is no
        // stack of handlers.
        rl_linefunc = linefunc;
        el_set_va(E, EL_UNBUFFERED, 1);
    }
}

// [spec:libedit:def:readline.rl-callback-handler-remove-fn]
// [spec:libedit:sem:readline.rl-callback-handler-remove-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_callback_handler_remove() {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C
    // (ERR-readline-11, UB) — the NULL editor is passed on to `el_set`.
    unsafe {
        // Nothing else happens: the prompt is not restored or freed, no
        // partially typed line is discarded, the terminal is not deprepped and
        // the display is left as it is.
        el_set_va(E, EL_UNBUFFERED, 0);
        rl_linefunc = None;
    }
}

// [spec:libedit:def:readline.rl-redisplay-fn]
// [spec:libedit:sem:readline.rl-redisplay-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_redisplay() {
    // SAFETY: single-threaded module state.
    unsafe {
        if E.is_null() {
            return;
        }
        // The reprint character is *pushed as input*, so the redraw happens
        // twice: once through EL_REFRESH and once when the pushed character is
        // consumed and runs whatever is bound to it (ERR-readline-26). With no
        // reprint character configured a NUL byte is pushed, which `el_push`
        // treats as an empty push.
        let a = [(*E).el_tty.t_c[TS_IO][C_REPRINT] as c_char, 0];
        crate::eln::el_push(E, a.as_ptr());
        rl_forced_update_display();
    }
}

// [spec:libedit:def:readline.rl-get-previous-history-fn]
// [spec:libedit:sem:readline.rl-get-previous-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_get_previous_history(count: c_int, key: c_int) -> c_int {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        let a = [key as c_char, 0];
        // Pushes the key back `count` times rather than moving through the
        // history, so it only works if `key` is bound to a history-recall
        // command; no history global is touched (ERR-readline-44).
        for _ in 0..count {
            crate::eln::el_push(E, a.as_ptr());
        }
        0
    }
}

// [spec:libedit:def:readline.rl-prep-terminal-fn]
// [spec:libedit:sem:readline.rl-prep-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_prep_terminal(meta_flag: c_int) {
    // readline's eight-bit-input request, ignored entirely.
    let _ = meta_flag;
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        el_set_va(E, EL_PREP_TERM, 1);
    }
}

// [spec:libedit:def:readline.rl-deprep-terminal-fn]
// [spec:libedit:sem:readline.rl-deprep-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_deprep_terminal() {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        el_set_va(E, EL_PREP_TERM, 0);
    }
}

// [spec:libedit:def:readline.rl-read-init-file-fn]
// [spec:libedit:sem:readline.rl-read-init-file-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_read_init_file(s: *const c_char) -> c_int {
    // SAFETY: `s` is NULL or a NUL-terminated path; no lazy-init guard, as in
    // the C.
    //
    // The grammar is libedit's `.editrc`, not readline's `.inputrc`, so almost
    // every line of a readline init file is rejected, and the failure encoding
    // is `el_source`'s -1 rather than an errno (ERR-readline-46).
    unsafe { crate::histedit::el_source(E, s) }
}

// [spec:libedit:def:readline.rl-parse-and-bind-fn]
// [spec:libedit:sem:readline.rl-parse-and-bind-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_parse_and_bind(line: *const c_char) -> c_int {
    // SAFETY: `line` is a NUL-terminated string; the argument vector belongs
    // to the tokenizer and dies with it.
    unsafe {
        // The C checks neither return value: an allocation failure crashes on
        // the next call, and an incomplete quote leaves `argc`/`argv` in
        // whatever state `tok_str` left them (ERR-readline-12, UB). Defined
        // here by taking the NULL tokenizer as "nothing parsed", which is the
        // failure the return value already reports.
        let tok = crate::histedit::tok_init(ptr::null());
        if tok.is_null() {
            return 1;
        }
        let mut argc: c_int = 0;
        let mut argv: *mut *const c_char = ptr::null_mut();
        crate::histedit::tok_str(tok, line, &mut argc, &mut argv);
        // 0 when the command was recognized and executed, and also when a
        // `prog:` prefix did not match this program.
        let argc = if argv.is_null() {
            1
        } else {
            crate::eln::el_parse(E, argc, argv)
        };
        crate::histedit::tok_end(tok);
        c_int::from(argc != 0)
    }
}

// [spec:libedit:def:readline.rl-variable-bind-fn]
// [spec:libedit:sem:readline.rl-variable-bind-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_variable_bind(var: *const c_char, value: *const c_char) -> c_int {
    /*
     * The proper return value is undocument, but this is what the
     * readline source seems to do.
     */
    // `bind <var> <value>`: `var` is interpreted as a *key* and `value` as the
    // *command*, which is not readline's variable namespace at all
    // (ERR-readline-45, reproduced). No lazy-init guard, as in the C.
    // SAFETY: both are NUL-terminated strings owned by the caller.
    unsafe {
        c_int::from(el_set_va(E, EL_BIND, c"".as_ptr(), var, value, ptr::null::<c_char>()) == -1)
    }
}

// [spec:libedit:def:readline.rl-stuff-char-fn]
// [spec:libedit:sem:readline.rl-stuff-char-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_stuff_char(c: c_int) -> c_int {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        // Inserts into the *line*, which is what GNU readline's `rl_insert`
        // does, while libedit's `rl_insert` pushes onto the input queue: the
        // two are swapped relative to readline (ERR-readline-42, reproduced).
        // A `c` of 0 makes the string empty and inserts nothing, and the
        // return never reports failure.
        let buf = [c as c_char, 0];
        crate::eln::el_insertstr(E, buf.as_ptr());
        1
    }
}

/// C: `static int _rl_event_read_char(EditLine *el, wchar_t *wc);` — the
/// `EL_GETCFN` shim that spins `rl_event_hook` while the read would block.
// [spec:libedit:def:readline.rl-event-read-char-fn]
// [spec:libedit:sem:readline.rl-event-read-char-fn]
fn _rl_event_read_char(el: *mut EditLine, wc: *mut u32) -> c_int {
    let mut ch: u8 = 0;
    let mut num_read: c_int = 0;
    // SAFETY: `wc` is EditLine's one-character out parameter and `el` its own
    // editor.
    unsafe {
        *wc = 0;
        while let Some(hook) = rl_event_hook {
            // The return value is ignored.
            hook();
            if el.is_null() {
                return -1;
            }
            // Gap: the C polls with `ioctl(FIONREAD)` and reads only when a
            // byte is pending, which is what makes the loop call the hook
            // repeatedly (ERR-readline-33). The ioctl has no route here, so
            // the retry only happens when the descriptor is itself
            // non-blocking — the C's EAGAIN arm, and the case an event-hook
            // consumer sets up. Otherwise the read blocks and the hook is
            // called once per character.
            match read_one_byte((*el).el_infd, &mut ch) {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    num_read = 0;
                    continue;
                }
                Err(_) => {
                    num_read = -1;
                    break;
                }
                // The C also retries on a zero-length read, which under
                // FIONREAD means "nothing pending yet". A blocking read
                // answers 0 only at end of input, so the port stops rather
                // than spinning.
                Ok(n) => {
                    num_read = n as c_int;
                    break;
                }
            }
        }
        // The hook cleared itself: put the builtin reader back.
        if { rl_event_hook }.is_none() && !el.is_null() {
            el_set_va(el, EL_GETCFN, EL_BUILTIN_GETCFN);
        }
        // Exactly one *byte*, widened with no multibyte decoding, so non-ASCII
        // input is corrupted whenever an event hook is installed
        // (ERR-readline-32, reproduced).
        *wc = ch as u32;
        num_read
    }
}

/// One byte from a descriptor, without taking ownership of it.
///
/// `std::fs::File` is the only reader the port can build from a raw
/// descriptor without libc; the descriptor is handed straight back so the
/// `File` never closes it. The C's `EAGAIN` — the one error its caller tells
/// apart from the rest — arrives as [`std::io::ErrorKind::WouldBlock`], so no
/// sentinel return has to stand in for it.
fn read_one_byte(fd: i32, out: &mut u8) -> std::io::Result<usize> {
    use std::os::fd::{FromRawFd, IntoRawFd};
    if fd < 0 {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
    }
    // SAFETY: the descriptor is EditLine's own and stays open — `into_raw_fd`
    // below gives it back rather than closing it.
    let mut f = unsafe { std::fs::File::from_raw_fd(fd) };
    let r = f.read(core::slice::from_mut(out));
    let _ = f.into_raw_fd();
    r
}

/// C: `static void _rl_update_pos(void);` — republishes `el_line` into
/// `rl_point` and `rl_end`.
// [spec:libedit:def:readline.rl-update-pos-fn]
// [spec:libedit:sem:readline.rl-update-pos-fn]
fn _rl_update_pos() {
    // SAFETY: single-threaded module state. The C has no guards at all here;
    // a NULL editor or line buffer is UB, defined here as leaving the globals
    // alone.
    unsafe {
        if E.is_null() {
            return;
        }
        let li = crate::eln::el_line(E);
        if li.is_null() {
            return;
        }
        // Byte offsets into the *encoded* line, not character counts. Nothing
        // here reads the globals back into the editor, so assignments an
        // application makes to them are discarded.
        rl_point = (*li).cursor.offset_from((*li).buffer) as c_int;
        rl_end = (*li).lastchar.offset_from((*li).buffer) as c_int;
        if !rl_line_buffer.is_null() {
            *rl_line_buffer.add(rl_end as usize) = 0;
        }
    }
}

// [spec:libedit:def:readline.rl-copy-text-fn]
// [spec:libedit:sem:readline.rl-copy-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_copy_text(from: c_int, to: c_int) -> *mut c_char {
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        let li = crate::eln::el_line(E);
        if li.is_null() {
            return ptr::null_mut();
        }

        if from > to {
            return ptr::null_mut();
        }

        let len_line = (*li).lastchar.offset_from((*li).buffer) as c_int;
        // A negative `from` reads before the start of the buffer in the C
        // (ERR-readline-07, UB); defined here by clamping to the line start.
        let mut from = from.max(0);
        let mut to = to;
        if from > len_line {
            from = len_line;
        }
        if to > len_line {
            to = len_line;
        }
        if to < from {
            to = from;
        }

        let len = (to - from) as usize;
        let out = c_alloc(len + 1);
        if out.is_null() {
            return ptr::null_mut();
        }

        // `strlcpy`'s third argument is the destination *size*, so passing
        // `len` copies only `len - 1` bytes plus a NUL and the last requested
        // character is dropped (ERR-readline-06). That is defined-but-wrong,
        // so it is reproduced. The `len == 0` case is not: the C leaves the
        // block uninitialised and unterminated, which is UB, so the port
        // writes the terminator and hands back an empty string.
        let src = core::slice::from_raw_parts((*li).buffer.cast::<u8>().add(from as usize), len);
        let copy = len.saturating_sub(1);
        let copy = src[..copy].iter().position(|&b| b == 0).unwrap_or(copy);
        ptr::copy_nonoverlapping(src.as_ptr(), out, copy);
        *out.add(copy) = 0;

        out.cast()
    }
}

// [spec:libedit:def:readline.rl-replace-line-fn]
// [spec:libedit:sem:readline.rl-replace-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_replace_line(text: *const c_char, clear_undo: c_int) {
    // Declared unused: this layer has no undo list to clear.
    let _ = clear_undo;
    // SAFETY: `text` is NULL or a NUL-terminated string.
    unsafe {
        // `rl_replace_line("")` is a silent no-op here, where GNU readline's
        // clears the line (ERR-readline-47, reproduced).
        if text.is_null() || *text == 0 {
            return;
        }

        lazy_init();

        // The result is discarded, so a line-too-long or decoding failure is
        // invisible, and none of `rl_point`/`rl_end`/`rl_line_buffer` is
        // refreshed.
        crate::eln::el_replacestr(E, text);
    }
}

// [spec:libedit:def:readline.rl-delete-text-fn]
// [spec:libedit:sem:readline.rl-delete-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_delete_text(start: c_int, end: c_int) -> c_int {
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        // `el_deletestr1` works in *wide characters*, while `rl_point` and
        // `rl_end` are byte offsets, so in a multibyte locale the offsets an
        // application computes do not correspond to what is deleted
        // (ERR-readline-35). 0 for the rejected cases and 0 on success, so the
        // two are indistinguishable.
        crate::histedit::el_deletestr1(E, start, end)
    }
}

// [spec:libedit:def:readline.rl-get-screen-size-fn]
// [spec:libedit:sem:readline.rl-get-screen-size-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_get_screen_size(rows: *mut c_int, cols: *mut c_int) {
    // SAFETY: either pointer may be NULL independently; no lazy-init guard, as
    // in the C.
    unsafe {
        // Neither result is checked, so on failure the caller's variables keep
        // whatever they held: this function does not initialize them.
        if !rows.is_null() {
            el_get_va(E, EL_GETTC, c"li".as_ptr(), rows);
        }
        if !cols.is_null() {
            el_get_va(E, EL_GETTC, c"co".as_ptr(), cols);
        }
    }
}

/// C: `void rl_message(const char *format, ...);`
///
/// The one genuinely variadic entry point in this file, and the only one whose
/// tail is a `printf` argument list rather than an enumerated per-op shape.
//
// THE FORMAT STRING IS INSTALLED UNEXPANDED. `c_variadic` is stable, so the
// tail is now reachable — `abi-varargs` — but running the C's
// `vsnprintf(msg, sizeof msg, format, args)` over it needs a `printf`
// implementation, and `plan/decisions/no-c-ffi.md` enumerates the three sites
// where a libc symbol may be named: `vsnprintf` is at none of them, and a
// fourth site amends that decision before it is written. So the remaining
// blocker is the formatter, not the `va_list`. Everything around it
// is faithful — the 160-byte truncation, the prompt being overwritten rather
// than saved, and the forced redisplay (ERR-readline-36) — so an application
// whose message carries no conversions sees the C's behaviour exactly.
// [spec:libedit:def:readline.rl-message-fn]
// [spec:libedit:sem:readline.rl-message-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_message(format: *const c_char, ap: ...) {
    // SAFETY: `format` is a NUL-terminated string owned by the caller.
    unsafe {
        let mut msg = [0u8; MAX_MESSAGE];
        if !format.is_null() {
            let src = c_bytes(format);
            // `vsnprintf` truncates silently at 159 characters plus NUL.
            let n = src.len().min(MAX_MESSAGE - 1);
            msg[..n].copy_from_slice(&src[..n]);
        }
        // The message *becomes* `rl_prompt`, overwriting whatever was there,
        // so an application that did not call `rl_save_prompt` first loses the
        // original prompt for good. The failure return is discarded.
        rl_set_prompt(msg.as_ptr().cast());
        rl_forced_update_display();
    }
}

// [spec:libedit:def:readline.rl-set-screen-size-fn]
// [spec:libedit:sem:readline.rl-set-screen-size-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_screen_size(rows: c_int, cols: c_int) {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        // `EL_SETTC` takes strings, so the values are formatted as decimal.
        // Negative values are accepted without validation, and the display
        // arrays are not resized the way `el_resize` would resize them.
        let buf = format!("{rows}\0");
        el_set_va(
            E,
            EL_SETTC,
            c"li".as_ptr(),
            buf.as_ptr(),
            ptr::null::<c_char>(),
        );
        let buf = format!("{cols}\0");
        el_set_va(
            E,
            EL_SETTC,
            c"co".as_ptr(),
            buf.as_ptr(),
            ptr::null::<c_char>(),
        );
    }
}

// [spec:libedit:def:readline.rl-completion-matches-fn]
// [spec:libedit:sem:readline.rl-completion-matches-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_completion_matches(
    str_: *const c_char,
    fun: Option<unsafe extern "C" fn(*const c_char, c_int) -> *mut c_char>,
) -> *mut *mut c_char {
    // SAFETY: `str_` is dereferenced by this function, as in the C, and `fun`
    // is called unconditionally. Every string the generator returns is adopted,
    // not copied, and the caller frees all of them plus the array.
    unsafe {
        let Some(fun) = fun else {
            return ptr::null_mut();
        };

        // Index 0 is reserved for the common prefix.
        let mut list: Vec<*mut c_char> = vec![ptr::null_mut()];
        loop {
            let m = fun(str_, (list.len() - 1) as c_int);
            if m.is_null() {
                break;
            }
            list.push(m);
        }

        // No match was produced. The C's error exit frees only the array and
        // leaks every match collected so far (ERR-readline-20); there are none
        // to leak here.
        if list.len() == 1 {
            return ptr::null_mut();
        }

        if list.len() == 2 {
            /* exactly one match */
            list[0] = c_dup(c_bytes(list[1]));
            if list[0].is_null() {
                return ptr::null_mut();
            }
            return finish_match_list(list);
        }

        // Reproduced, not repaired: the C sorts with `strcmp` cast to
        // `qsort`'s comparator type, so the comparator is handed the
        // *addresses* of the elements and orders them by their pointer
        // representations rather than by their contents (ERR-readline-01).
        // `plan/decisions/conformance-policy.md` names this as one of the six
        // forks that default to reproduce; the correctly written
        // `_rl_qsort_string_compare` sits unused beside it, as in the C. The
        // sort here is stable, which pins the one ordering `qsort` leaves
        // unspecified.
        list[1..].sort_by(|a, b| strcmp_pointer_repr(*a, *b));

        // The shortest common prefix over *adjacent* pairs, which is only
        // meaningful if the array is sorted — so the broken sort corrupts this
        // too.
        let min = list[1..]
            .windows(2)
            .map(|pair| {
                let (a, b) = (c_bytes(pair[0]), c_bytes(pair[1]));
                a.iter().zip(b).take_while(|(x, y)| x == y).count()
            })
            .min()
            .unwrap_or(usize::MAX);

        if min == 0 && *str_ != 0 {
            /* the matches share nothing, so offer the original text back */
            list[0] = c_dup(c_bytes(str_));
        } else {
            let first = c_bytes(list[1]);
            list[0] = c_dup(&first[..min.min(first.len())]);
        }
        if list[0].is_null() {
            return ptr::null_mut();
        }

        finish_match_list(list)
    }
}

/// The NULL-terminated `char **` a completion entry point hands back.
///
/// # Safety
///
/// The caller owns every pointer in `list`; ownership passes to the C caller,
/// who frees each element and then the array.
unsafe fn finish_match_list(list: Vec<*mut c_char>) -> *mut *mut c_char {
    // SAFETY: the array is sized for the list plus its NULL terminator.
    unsafe {
        let out: *mut *mut c_char = c_alloc_array(list.len() + 1);
        if out.is_null() {
            return ptr::null_mut();
        }
        for (i, p) in list.iter().enumerate() {
            *out.add(i) = *p;
        }
        *out.add(list.len()) = ptr::null_mut();
        out
    }
}

// [spec:libedit:def:readline.rl-filename-completion-function-fn]
// [spec:libedit:sem:readline.rl-filename-completion-function-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_filename_completion_function(
    text: *const c_char,
    state: c_int,
) -> *mut c_char {
    // C: `return fn_filename_completion_function(text, state);` — identical to
    // `filename_completion_function`; both spellings exist because readline
    // renamed the function.
    // SAFETY: `text` is a NUL-terminated string.
    unsafe { crate::filecomplete::fn_filename_completion_function(text, state) }
}

// [spec:libedit:def:readline.rl-forced-update-display-fn]
// [spec:libedit:sem:readline.rl-forced-update-display-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_forced_update_display() {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    // `rl_redisplay_function` is never consulted, so a custom redisplay
    // routine has no effect.
    unsafe {
        el_set_va(E, EL_REFRESH);
    }
}

// [spec:libedit:def:readline.rl-abort-internal-fn]
// [spec:libedit:sem:readline.rl-abort-internal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rl_abort_internal() -> c_int {
    // SAFETY: single-threaded module state.
    unsafe {
        if !E.is_null() {
            crate::histedit::el_beep(E);
        }
        // The C `longjmp`s into `readline()`'s frame and never returns; with
        // no live `readline()` the jump lands in a dead frame, and nested
        // `readline()` calls can only reach the innermost `setjmp`
        // (ERR-readline-10, UB). Rust has neither `longjmp` nor a panic that
        // may cross this boundary, so the port defines the mechanism: raise
        // the flag `readline()` checks after `el_gets`, and end the editor
        // loop so it is checked promptly. Raised with no `readline()` running,
        // it is simply consumed by the next one.
        ABORT_PENDING.store(true, std::sync::atomic::Ordering::Relaxed);
        rl_done = 1;
        // The C's declared `int` is never produced; 0 is the port's.
        0
    }
}

// [spec:libedit:def:readline.rl-qsort-string-compare-fn]
// [spec:libedit:sem:readline.rl-qsort-string-compare-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rl_qsort_string_compare(
    s1: *mut *mut c_char,
    s2: *mut *mut c_char,
) -> c_int {
    // SAFETY: both are pointers to live `char *` elements.
    unsafe {
        // Not used anywhere in libedit: `rl_completion_matches` casts `strcmp`
        // itself instead (ERR-readline-01). Kept because it is exported.
        //
        // Gap: `strcoll` needs `LC_COLLATE`, which the port has no route to,
        // so this is byte order — which is what `strcoll` gives in the C
        // locale.
        let (a, b) = (c_bytes(*s1), c_bytes(*s2));
        match a.cmp(b) {
            Ordering::Less => -1,
            Ordering::Equal => 0,
            Ordering::Greater => 1,
        }
    }
}

// [spec:libedit:def:readline.history-get-history-state-fn]
// [spec:libedit:sem:readline.history-get-history-state-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_get_history_state() -> *mut HistoryState {
    // SAFETY: the caller owns the returned block and frees it.
    unsafe {
        let hs: *mut HistoryState = c_alloc_array(1);
        if hs.is_null() {
            return ptr::null_mut();
        }
        // libedit's `HISTORY_STATE` has exactly one member, where GNU
        // readline's carries four more, so a program built against the GNU
        // header reads past the end of this allocation (ERR-readline-48). The
        // struct is frozen by the drop-in requirement.
        (*hs).length = history_length;
        hs
    }
}

// [spec:libedit:def:readline.rl-kill-full-line-fn]
// [spec:libedit:sem:readline.rl-kill-full-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_kill_full_line(count: c_int, key: c_int) -> c_int {
    let _ = (count, key);
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        // The text is killed in the EditLine sense — recoverable with
        // `em-yank` — not merely discarded, and it does not land in readline's
        // kill ring, which this layer does not implement at all. The CC_*
        // return is discarded and none of the position globals is refreshed.
        em_kill_line(E);
        0
    }
}

// [spec:libedit:def:readline.rl-kill-text-fn]
// [spec:libedit:sem:readline.rl-kill-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_kill_text(from: c_int, to: c_int) -> c_int {
    // Stub: nothing is deleted, nothing is copied to any kill ring, and no
    // global is touched. Because GNU readline's also returns 0, a caller
    // cannot tell "killed" from "did nothing"; `rl_delete_text` is what
    // actually deletes.
    let _ = (from, to);
    0
}

// [spec:libedit:def:readline.rl-make-bare-keymap-fn]
// [spec:libedit:sem:readline.rl-make-bare-keymap-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_make_bare_keymap() -> Keymap {
    // Stub: no keymap object is allocated, so there is nothing to free and
    // nothing to populate. A caller that indexes the result crashes.
    ptr::null_mut()
}

// [spec:libedit:def:readline.rl-get-keymap-fn]
// [spec:libedit:sem:readline.rl-get-keymap-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_get_keymap() -> Keymap {
    // Stub: libedit implements no readline keymaps at all, and the exported
    // `emacs_*_keymap` arrays are never returned from here.
    ptr::null_mut()
}

// [spec:libedit:def:readline.rl-set-keymap-fn]
// [spec:libedit:sem:readline.rl-set-keymap-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_keymap(k: Keymap) {
    // Stub: bindings continue to come from EditLine's own map.
    let _ = k;
}

// [spec:libedit:def:readline.rl-generic-bind-fn]
// [spec:libedit:sem:readline.rl-generic-bind-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_generic_bind(
    type_: c_int,
    keyseq: *const c_char,
    data: *const c_char,
    k: Keymap,
) -> c_int {
    // Stub: nothing is bound and nothing is copied, so the caller retains
    // ownership of `keyseq` and `data`. 0 unconditionally reports success, so
    // a caller cannot detect that the binding was dropped.
    let _ = (type_, keyseq, data, k);
    0
}

// [spec:libedit:def:readline.rl-bind-key-in-map-fn]
// [spec:libedit:sem:readline.rl-bind-key-in-map-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_bind_key_in_map(
    key: c_int,
    fun: Option<unsafe extern "C" fn(c_int, c_int) -> c_int>,
    k: Keymap,
) -> c_int {
    // Stub, as `rl_generic_bind`.
    let _ = (key, fun, k);
    0
}

// [spec:libedit:def:readline.rl-set-key-fn]
// [spec:libedit:sem:readline.rl-set-key-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_key(
    keyseq: *const c_char,
    function: Option<unsafe extern "C" fn(c_int, c_int) -> c_int>,
    k: Keymap,
) -> c_int {
    // Stub: `rl_add_defun`, which binds exactly one byte, is the only working
    // route to a custom binding.
    let _ = (keyseq, function, k);
    0
}

// [spec:libedit:def:readline.rl-cleanup-after-signal-fn]
// [spec:libedit:sem:readline.rl-cleanup-after-signal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_cleanup_after_signal() {
    // Empty stub — "unsupported, but needed by python". libedit installs and
    // clears its own handlers around `el_gets`, so there is nothing to undo.
}

// [spec:libedit:def:readline.rl-on-new-line-fn]
// [spec:libedit:sem:readline.rl-on-new-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_on_new_line() -> c_int {
    // Stub: no display state is reset and EditLine is not told the cursor
    // moved. 0 is readline's "success".
    0
}

// [spec:libedit:def:readline.rl-free-line-state-fn]
// [spec:libedit:sem:readline.rl-free-line-state-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_free_line_state() {
    // Empty stub: the current line survives a signal untouched.
}

// [spec:libedit:def:readline.rl-set-keyboard-input-timeout-fn]
// [spec:libedit:sem:readline.rl-set-keyboard-input-timeout-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_keyboard_input_timeout(u: c_int) -> c_int {
    // Stub: no timeout is stored or applied. GNU readline returns the
    // *previous* value, so a caller saving this to restore it later restores
    // 0 — "no wait at all".
    let _ = u;
    0
}

// [spec:libedit:def:readline.rl-resize-terminal-fn]
// [spec:libedit:sem:readline.rl-resize-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_resize_terminal() {
    // SAFETY: single-threaded module state; no lazy-init guard, as in the C.
    unsafe {
        // Re-queries the window size, rebuilds the display arrays and
        // repaints. Not async-signal-safe: a SIGWINCH handler should set a
        // flag and call this from the main loop.
        crate::histedit::el_resize(E);
    }
}

// [spec:libedit:def:readline.rl-reset-after-signal-fn]
// [spec:libedit:sem:readline.rl-reset-after-signal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_reset_after_signal() {
    // SAFETY: single-threaded module state; the hook is the application's if
    // it replaced the default.
    unsafe {
        // Note the asymmetry with `rl_cleanup_after_signal`, which is an empty
        // stub: the conventional cleanup-then-reset pairing only performs the
        // second half. Nothing is repainted.
        if let Some(f) = rl_prep_term_function {
            f(1);
        }
    }
}

// [spec:libedit:def:readline.rl-echo-signal-char-fn]
// [spec:libedit:sem:readline.rl-echo-signal-char-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_echo_signal_char(sig: c_int) {
    // SAFETY: single-threaded module state; no NULL guard on `e` in the C.
    unsafe {
        let c = tty_get_signal_character(E, sig);
        if c == -1 {
            return;
        }
        // The raw control byte is deposited in the virtual display, where GNU
        // readline echoes `^C` to `rl_outstream`; libedit's is what crosses
        // the ABI (ERR-readline-25, reproduced).
        re_putc(E, c as u32);
    }
}

// [spec:libedit:def:readline.rl-crlf-fn]
// [spec:libedit:sem:readline.rl-crlf-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_crlf() -> c_int {
    // SAFETY: single-threaded module state; no NULL guard on `e` in the C.
    unsafe {
        // Into the virtual display array at the current refresh cursor,
        // without advancing it and without reaching the terminal: nothing is
        // flushed and the character becomes visible only if a later refresh
        // renders that cell (ERR-readline-25, reproduced).
        re_putc(E, u32::from(b'\n'));
        0
    }
}

// [spec:libedit:def:readline.rl-ding-fn]
// [spec:libedit:sem:readline.rl-ding-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_ding() -> c_int {
    // SAFETY: as `rl_crlf`.
    unsafe {
        // Notably *not* `el_beep(e)`, which is what actually emits the bell,
        // so this is close to a no-op and can leave a stray BEL byte in the
        // virtual display (ERR-readline-25, reproduced).
        re_putc(E, 0x07);
        0
    }
}

// [spec:libedit:def:readline.rl-abort-fn]
// [spec:libedit:sem:readline.rl-abort-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_abort(count: c_int, key: c_int) -> c_int {
    // Stub: nothing is aborted — no bell, no line reset, no jump, no change to
    // `rl_done`. Both parameters are read only to suppress unused-parameter
    // warnings, and every path evaluates to 0. `_rl_abort_internal` is
    // unrelated despite the name.
    let _ = (count, key);
    0
}

// [spec:libedit:def:readline.rl-set-keymap-name-fn]
// [spec:libedit:sem:readline.rl-set-keymap-name-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_keymap_name(name: *const c_char, k: Keymap) -> c_int {
    // Stub: no name is registered and `name` is not copied. Always 0, so a
    // caller cannot detect that the association was dropped.
    let _ = (name, k);
    0
}

// [spec:libedit:def:readline.free-history-entry-fn]
// [spec:libedit:sem:readline.free-history-entry-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_history_entry(he: *mut HistEntry) -> HistdataT {
    // Documented stub that frees nothing: the whole C body is
    // `return he ? NULL : NULL;`. Neither `he`, nor `he->line`, nor `he->data`
    // is released, so the readline idiom
    // `free_history_entry(remove_history(i))` leaks both blocks
    // (ERR-readline-15).
    //
    // Reproduced deliberately, and named as one of the six forks in
    // `plan/decisions/conformance-policy.md`: making it actually free would
    // turn today's leak into a double free in programs that already free the
    // entry themselves.
    let _ = he;
    ptr::null_mut()
}

// [spec:libedit:def:readline.rl-erase-entire-line-fn]
// [spec:libedit:sem:readline.rl-erase-entire-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rl_erase_entire_line() {
    // Empty stub: the line buffer is not cleared, the cursor is not moved and
    // nothing is written to the terminal. It exists only so the symbol
    // resolves for programs that reach into readline's private namespace.
}

// ---------------------------------------------------------------------------
// Declared by `editline/readline.h`, defined elsewhere in the C.
// ---------------------------------------------------------------------------

/// C: `char **completion_matches(/* const */ char *, rl_compentry_func_t *);`
///
/// The pre-4.2 GNU readline generator-to-match-list entry point. The header
/// declares it, but `readline.c` `#define`s the name away around its own
/// `#include` and never defines it: the symbol a consumer links is
/// `filecomplete.c`'s, whose behaviour is `sem:filecomplete.completion-matches-fn`
/// and which libedit itself reaches from `fn_complete2`. A port must export
/// exactly one symbol of this name, so the export lives here — in the crate
/// that owns the ABI — and defers to the core's `filecomplete`.
///
/// Note the prototypes differ in `const`ness between the two headers. That is
/// formally incompatible and harmless at the ABI level; `text` is borrowed,
/// read-only and never retained.
// [spec:libedit:def:readline.completion-matches-fn]
// [spec:libedit:sem:readline.completion-matches-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn completion_matches(
    text: *mut c_char,
    genfunc: Option<unsafe extern "C" fn(*const c_char, c_int) -> *mut c_char>,
) -> *mut *mut c_char {
    // SAFETY: `text` is borrowed and never retained; `genfunc` is called
    // unconditionally, as in the C, and every string it returns is owned.
    unsafe {
        let Some(genfunc) = genfunc else {
            return ptr::null_mut();
        };
        // The C forwards `text` to the generator unchanged and never inspects
        // it, so a NULL is the generator's problem; the core takes a `&str`,
        // so a NULL becomes the empty string here.
        let t = c_bytes_opt(text).unwrap_or(b"");
        let t = String::from_utf8_lossy(t).into_owned();

        let mut make_match = move |text: &str, state: i32| -> Option<String> {
            let ctext = c_dup(text.as_bytes());
            if ctext.is_null() {
                return None;
            }
            let m = genfunc(ctext, state);
            c_free_str(ctext);
            if m.is_null() {
                return None;
            }
            let out = String::from_utf8_lossy(c_bytes(m)).into_owned();
            c_free_str(m);
            Some(out)
        };

        // No sorting, generation order preserved, and an empty element 0 stays
        // empty — none of `rl_completion_matches`' behaviour (ERR-completion-22).
        let Some(matches) = filecomplete::completion_matches(&t, &mut make_match) else {
            return ptr::null_mut();
        };
        let mut list: Vec<*mut c_char> = Vec::with_capacity(matches.len() + 1);
        for m in &matches {
            let p = c_dup(m.as_bytes());
            if p.is_null() {
                // The C frees only the array and leaks the matches; the port
                // releases what it allocated and reports the same NULL.
                c_free_each(&list);
                return ptr::null_mut();
            }
            list.push(p);
        }
        finish_match_list(list)
    }
}

#[cfg(test)]
mod vis_flags_test {
    use super::{VIS_NOSLASH, VIS_WHITE};

    /// `rl_add_defun` builds the key-binding string it hands `EL_BIND` by
    /// `vis`ing the character, so a wrong flag binds the function to a
    /// different key than libedit would.
    ///
    /// This declared `0x0008 | 0x0010 | 0x0020` until the `bsd` crate's own
    /// constants forced the comparison — `VIS_TAB | VIS_NL | VIS_SAFE`, both
    /// missing `VIS_SP` and carrying a flag that does not belong. No
    /// conformance driver calls `rl_add_defun`, so nothing else would have
    /// caught it.
    #[test]
    fn vis_white_is_what_vis_h_says() {
        // src/vis.h:47-50 — VIS_SP | VIS_TAB | VIS_NL.
        assert_eq!(VIS_WHITE, 0x0004 | 0x0008 | 0x0010);
        assert_eq!(VIS_NOSLASH, 0x0040);
    }

    /// The consequence, end to end: binding a space must produce `\040`. With
    /// the old value `VIS_SP` was absent, so the space passed through as
    /// itself and the binding went to a different key entirely.
    #[test]
    fn a_space_encodes_to_an_octal_escape() {
        let out = nshedit::vis::Encoder::new(nshedit::vis::Flags::from_bits(
            (VIS_WHITE | VIS_NOSLASH) as u32,
        ))
        .encode_byte(b' ', 0);
        assert_eq!(out, b"\\040");
    }
}

#[cfg(test)]
mod tests;
