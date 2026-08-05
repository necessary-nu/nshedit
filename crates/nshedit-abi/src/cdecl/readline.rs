//! `editline/readline.h` declarations with no natural Rust site.
//!
//! Mostly the C's own names for records the core spells in Rust casing, plus
//! the three `KEYMAP_ENTRY` tag constants, which C hides inside the struct
//! body where Rust has nowhere to put them.
//!
//! As in [`super::histedit`], every alias is the *same Rust type* as the core
//! item it restates, and the block at the end is what makes that a
//! compiler-checked claim.
//!
//! # What is deliberately not here
//!
//! The ten `rl_*_func_t` typedefs. C declares each as a **function type** —
//! `typedef int rl_hook_func_t(void);` — and uses it as `rl_hook_func_t *`.
//! Rust has no function type distinct from a function pointer, so nothing
//! written here could render as one, and rendering the pointer form under
//! the same name would silently turn every consumer's `rl_hook_func_t *x`
//! into a pointer to a pointer. They are emitted from the generator's config
//! instead, and `conformance/header-diff.sh` checks each one against the
//! original — see the note there.

// `histdata_t` and the `_`-prefixed tags are C spellings; see [`super`].
#![allow(non_camel_case_types)]

use core::ffi::c_void;

use nshedit::editline::readline::{HistEntry, HistoryState, KeymapEntry};

/// C: `FILE *`. As [`super::histedit::CFile`], which see.
pub type CFile = *mut c_void;

/// C: `typedef struct _hist_entry { ... } HIST_ENTRY;` — `def:readline.hist-entry`.
pub type HIST_ENTRY = HistEntry;

/// C: `typedef struct { int length; } HISTORY_STATE;` — `def:readline.history-state`.
///
/// The C leaves the record anonymous. A generator cannot: cbindgen prints a
/// Rust type's name, and a Rust type has one. The tag `_history_state` is
/// therefore ours, and is the single adjudicated divergence in this header —
/// see `conformance/header-diff.sh`, which records the argument.
pub type HISTORY_STATE = HistoryState;

/// C: `typedef struct _keymap_entry { ... } KEYMAP_ENTRY;` — `def:readline.keymap-entry`.
pub type KEYMAP_ENTRY = KeymapEntry;

/// C: `typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];` —
/// `def:readline.keymap-entry-array-keymap-size`.
pub type KEYMAP_ENTRY_ARRAY = [KEYMAP_ENTRY; 256];

/// C: `#define ISFUNC 0` — a `KEYMAP_ENTRY` holding a function.
pub const ISFUNC: u8 = 0;
/// C: `#define ISKMAP 1` — a `KEYMAP_ENTRY` holding a nested keymap.
pub const ISKMAP: u8 = 1;
/// C: `#define ISMACR 2` — a `KEYMAP_ENTRY` holding a macro.
pub const ISMACR: u8 = 2;

/// C: `#define control_character_threshold 0x20`.
pub const control_character_threshold: u8 = 0x20;
/// C: `#define control_character_bit 0x40`.
pub const control_character_bit: u8 = 0x40;
/// C: `#define RUBOUT 0x7f`.
pub const RUBOUT: u8 = 0x7f;

/// As [`super::histedit`]'s: these compile only if each alias is the same
/// Rust type as the core item it restates.
const _: () = {
    fn hist_entry(x: HIST_ENTRY) -> HistEntry {
        x
    }
    fn history_state(x: HISTORY_STATE) -> HistoryState {
        x
    }
    fn keymap_entry(x: KEYMAP_ENTRY) -> KeymapEntry {
        x
    }
    fn cfile(x: CFile) -> nshedit::el::CFile {
        x
    }
    let _ = (hist_entry, history_state, keymap_entry, cfile);
};
