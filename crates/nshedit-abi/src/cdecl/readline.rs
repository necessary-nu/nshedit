//! ABI-owned declarations for the installed `editline/readline.h`.
//!
//! The readline compatibility implementation uses these records directly;
//! there is no core twin and no layout cast. The ten callback aliases mirror
//! C's function typedefs for coverage and Rust callers, while cbindgen emits
//! their exact C function-type spelling from configuration because Rust has
//! no function type distinct from a function pointer.

use core::ffi::{c_char, c_int, c_void};

/// C: `FILE *` — a stream owned by the application.
pub type CFile = *mut c_void;

// [spec:libedit:def:readline.rl-linebuf-func-t-const-char-int]
/// C: `typedef int rl_linebuf_func_t(const char *, int);`
pub type RlLinebufFuncT = unsafe extern "C" fn(*const c_char, c_int) -> c_int;

// [spec:libedit:def:readline.rl-voidfunc-t-void]
/// C: `typedef void rl_voidfunc_t(void);`
pub type RlVoidfuncT = unsafe extern "C" fn();

// [spec:libedit:def:readline.rl-vintfunc-t-int]
/// C: `typedef void rl_vintfunc_t(int);`
pub type RlVintfuncT = unsafe extern "C" fn(c_int);

// [spec:libedit:def:readline.rl-vcpfunc-t-char]
/// C: `typedef void rl_vcpfunc_t(char *);`
pub type RlVcpfuncT = unsafe extern "C" fn(*mut c_char);

// [spec:libedit:def:readline.rl-completion-func-t-const-char-int-int]
/// C: `typedef char **rl_completion_func_t(const char *, int, int);`
pub type RlCompletionFuncT = unsafe extern "C" fn(*const c_char, c_int, c_int) -> *mut *mut c_char;

// [spec:libedit:def:readline.rl-compentry-func-t-const-char-int]
/// C: `typedef char *rl_compentry_func_t(const char *, int);`
pub type RlCompentryFuncT = unsafe extern "C" fn(*const c_char, c_int) -> *mut c_char;

// [spec:libedit:def:readline.rl-compdisp-func-t-char-int-int]
/// C: `typedef void rl_compdisp_func_t(char **, int, int);`
pub type RlCompdispFuncT = unsafe extern "C" fn(*mut *mut c_char, c_int, c_int);

// [spec:libedit:def:readline.rl-command-func-t-int-int]
/// C: `typedef int rl_command_func_t(int, int);`
pub type RlCommandFuncT = unsafe extern "C" fn(c_int, c_int) -> c_int;

// [spec:libedit:def:readline.rl-hook-func-t-void]
/// C: `typedef int rl_hook_func_t(void);`
pub type RlHookFuncT = unsafe extern "C" fn() -> c_int;

// [spec:libedit:def:readline.rl-icppfunc-t-char]
/// C: `typedef int rl_icppfunc_t(char **);`
pub type RlIcppfuncT = unsafe extern "C" fn(*mut *mut c_char) -> c_int;

// [spec:libedit:def:readline.history-state]
/// C: `typedef struct { int length; } HISTORY_STATE;`.
#[repr(C)]
pub struct HistoryState {
    pub length: c_int,
}

// [spec:libedit:def:readline.histdata-t]
/// C: `typedef void *histdata_t;` — opaque per-entry application data.
pub type HistdataT = *mut c_void;

// [spec:libedit:def:readline.hist-entry]
/// C: `typedef struct _hist_entry { const char *line; histdata_t data; } HIST_ENTRY;`.
#[repr(C)]
pub struct HistEntry {
    pub line: *const c_char,
    pub data: HistdataT,
}

// [spec:libedit:def:readline.keymap-entry]
/// C: `typedef struct _keymap_entry { char type; rl_linebuf_func_t *function; } KEYMAP_ENTRY;`.
#[repr(C)]
pub struct KeymapEntry {
    /// `ISFUNC`, `ISKMAP`, or `ISMACR`.
    pub r#type: c_char,
    /// Nullable `rl_linebuf_func_t *`, expanded so cbindgen renders a C
    /// function pointer rather than an `Option` over an alias.
    pub function: Option<unsafe extern "C" fn(*const c_char, c_int) -> c_int>,
}

/// C: `#define KEYMAP_SIZE 256`.
pub const KEYMAP_SIZE: usize = 256;

// [spec:libedit:def:readline.keymap-entry-array-keymap-size]
/// C: `typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];`.
pub type KeymapEntryArray = [KeymapEntry; KEYMAP_SIZE];

// [spec:libedit:def:readline.keymap]
/// C: `typedef KEYMAP_ENTRY *Keymap;` — a borrowed mutable keymap view.
pub type Keymap = *mut KeymapEntry;

/// C: `typedef struct _hist_entry { ... } HIST_ENTRY;`.
pub type HistEntryAlias = HistEntry;

/// C: `typedef struct { int length; } HISTORY_STATE;`.
///
/// C leaves the record anonymous. cbindgen must emit a tag, so the generator
/// uses `_history_state` as an implementation spelling.
pub type HistoryStateAlias = HistoryState;

/// C: `typedef struct _keymap_entry { ... } KEYMAP_ENTRY;`.
pub type KeymapEntryAlias = KeymapEntry;

/// C: `typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];`.
pub type KeymapEntryArrayAlias = [KeymapEntryAlias; KEYMAP_SIZE];

/// C: `#define ISFUNC 0` — a `KEYMAP_ENTRY` holding a function.
pub const ISFUNC: u8 = 0;
/// C: `#define ISKMAP 1` — a `KEYMAP_ENTRY` holding a nested keymap.
pub const ISKMAP: u8 = 1;
/// C: `#define ISMACR 2` — a `KEYMAP_ENTRY` holding a macro.
pub const ISMACR: u8 = 2;

/// C: `#define control_character_threshold 0x20`.
pub const CONTROL_CHARACTER_THRESHOLD: u8 = 0x20;
/// C: `#define control_character_bit 0x40`.
pub const CONTROL_CHARACTER_BIT: u8 = 0x40;
/// C: `#define RUBOUT 0x7f`.
pub const RUBOUT: u8 = 0x7f;
