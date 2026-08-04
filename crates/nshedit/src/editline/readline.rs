//! The `src/editline/readline.h` public types; rules live in
//! `docs/spec/port/src/editline/readline.md`.
//!
//! This is the GNU readline compatibility ABI, which
//! `plan/decisions/no-c-ffi.md` keeps in scope: consumers include
//! `editline/readline.h` and link against us. Every string here is a narrow
//! C string the application owns or libedit hands back borrowed, so they
//! stay raw pointers and the structs stay `#[repr(C)]`.
//!
//! The C spells these as *function* types (`typedef int rl_hook_func_t(void)`),
//! declaring variables as `rl_hook_func_t *`. Rust has no function type
//! distinct from a function pointer, so each becomes a `fn` pointer and the
//! `*` at the use site disappears.

use core::ffi::{c_char, c_int};

// [spec:libedit:def:readline.rl-linebuf-func-t-const-char-int]
/// C: `typedef int rl_linebuf_func_t(const char *, int);`
pub type RlLinebufFuncT = fn(*const c_char, c_int) -> c_int;

// [spec:libedit:def:readline.rl-voidfunc-t-void]
/// C: `typedef void rl_voidfunc_t(void);`
pub type RlVoidfuncT = fn();

// [spec:libedit:def:readline.rl-vintfunc-t-int]
/// C: `typedef void rl_vintfunc_t(int);`
pub type RlVintfuncT = fn(c_int);

// [spec:libedit:def:readline.rl-vcpfunc-t-char]
/// C: `typedef void rl_vcpfunc_t(char *);`
pub type RlVcpfuncT = fn(*mut c_char);

// [spec:libedit:def:readline.rl-completion-func-t-const-char-int-int]
/// C: `typedef char **rl_completion_func_t(const char *, int, int);`
///
/// Returns a NULL-terminated, caller-freed array of narrow strings.
pub type RlCompletionFuncT = fn(*const c_char, c_int, c_int) -> *mut *mut c_char;

// [spec:libedit:def:readline.rl-compentry-func-t-const-char-int]
/// C: `typedef char *rl_compentry_func_t(const char *, int);`
pub type RlCompentryFuncT = fn(*const c_char, c_int) -> *mut c_char;

// [spec:libedit:def:readline.rl-compdisp-func-t-char-int-int]
/// C: `typedef void rl_compdisp_func_t(char **, int, int);`
pub type RlCompdispFuncT = fn(*mut *mut c_char, c_int, c_int);

// [spec:libedit:def:readline.rl-command-func-t-int-int]
/// C: `typedef int rl_command_func_t(int, int);`
pub type RlCommandFuncT = fn(c_int, c_int) -> c_int;

// [spec:libedit:def:readline.rl-hook-func-t-void]
/// C: `typedef int rl_hook_func_t(void);`
pub type RlHookFuncT = fn() -> c_int;

// [spec:libedit:def:readline.rl-icppfunc-t-char]
/// C: `typedef int rl_icppfunc_t(char **);`
pub type RlIcppfuncT = fn(*mut *mut c_char) -> c_int;

// [spec:libedit:def:readline.history-state]
/// C: `typedef struct { int length; } HISTORY_STATE;`
///
/// libedit supports only the length, as the C header's own comment says.
#[repr(C)]
pub struct HistoryState {
    pub length: c_int,
}

// [spec:libedit:def:readline.histdata-t]
/// C: `typedef void *histdata_t;` — opaque per-entry application data,
/// stored and handed back untouched.
pub type HistdataT = *mut core::ffi::c_void;

// [spec:libedit:def:readline.hist-entry]
/// C: `typedef struct _hist_entry { const char *line; histdata_t data; } HIST_ENTRY;`
///
/// `line` is borrowed from the history entry and, for `history_get`, from a
/// single file-static `HIST_ENTRY` that the next call overwrites; see
/// `sem:readline.history-get-fn`.
#[repr(C)]
pub struct HistEntry {
    pub line: *const c_char,
    pub data: HistdataT,
}

// [spec:libedit:def:readline.keymap-entry]
/// C: `typedef struct _keymap_entry { char type; rl_linebuf_func_t *function; } KEYMAP_ENTRY;`
pub struct KeymapEntry {
    /// `ISFUNC` (0), `ISKMAP` (1) or `ISMACR` (2). A `char` in the C, and
    /// left a byte-sized tag here.
    pub r#type: u8,
    pub function: Option<RlLinebufFuncT>,
}

/// C: `#define KEYMAP_SIZE 256`.
pub const KEYMAP_SIZE: usize = 256;

// [spec:libedit:def:readline.keymap-entry-array-keymap-size]
/// C: `typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];`
pub type KeymapEntryArray = [KeymapEntry; KEYMAP_SIZE];

// [spec:libedit:def:readline.keymap]
/// C: `typedef KEYMAP_ENTRY *Keymap;` — a borrowed view of a
/// [`KeymapEntryArray`], not an owning handle. libedit's readline layer
/// only ever hands back `emacs_meta_keymap`, a file-static array.
pub type Keymap = *mut KeymapEntry;
