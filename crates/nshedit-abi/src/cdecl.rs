//! The C declarations the shipped headers are generated from.
//!
//! `histedit.h` and `editline/readline.h` are generated from this crate by
//! `cbindgen` and committed under `include/`.
//!
//! Almost everything in the two headers falls out of exported functions and
//! statics. This module owns the remaining declarations and every completed
//! ABI record, so generation has no reason to parse the core:
//!
//! - The five incomplete handle types. Rust has no incomplete type, so
//!   [`handles`] declares unit structs used only to generate forward tags.
//! - The complete `LineInfo` and `HistEvent` record pairs and their exact
//!   pointer layouts.
//! - `wchar_t`. Rust stores the wide value as `u32`, which cbindgen otherwise
//!   prints as `uint32_t`; [`histedit::WcharT`] is renamed at generation.
//! - `FILE *`. The implementation stores an opaque `*mut c_void`, while the
//!   public declaration must retain the stronger C stream type.
//! - The C names for the readline records: `HIST_ENTRY`, `KEYMAP_ENTRY`, and
//!   `HISTORY_STATE`.
//!
//! Boundary code constructs the completed records field by field; it never
//! casts a core record into one. These layouts are authoritative and are
//! exercised by the direct C consumer.

// [spec:nshedit:req:abi.surface-stability]
pub mod handles;
pub mod histedit;
pub mod readline;
