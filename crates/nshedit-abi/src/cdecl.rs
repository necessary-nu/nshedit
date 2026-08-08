//! The C declarations the shipped headers are generated from.
//!
//! `histedit.h` and `editline/readline.h` are **generated from this crate**
//! by `cbindgen` and committed under `include/`; the originals in `src/` are
//! what `conformance/header-diff.sh` diffs the result against, and that diff
//! proves the *Rust* is right rather than proving the header is. See the
//! `abi-headers` and `conformance-header-diff` plan nodes.
//!
//! Almost everything in the two headers falls out of the exported functions
//! and statics on its own. This module holds the remainder — the declarations
//! that exist in C and have no natural Rust site, or whose Rust site spells
//! the type in a way C cannot read:
//!
//! - The five **incomplete types** `histedit.h` forward-declares. C has
//!   `typedef struct editline EditLine;` and never defines `struct editline`
//!   in a public header; Rust has no incomplete type, so [`handles`] declares
//!   unit structs whose only purpose is to be named.
//! - `wchar_t`. The core spells the wide character `u32`, which cbindgen
//!   renders `uint32_t` — a *different* C type from `wchar_t`, which is
//!   signed on every target this library builds for, so a consumer passing a
//!   `wchar_t *` to a `uint32_t *` parameter gets a diagnostic. [`histedit`]
//!   declares [`histedit::WcharT`] as `u32` and restates the wide aliases in
//!   terms of it. The generator maps that idiomatic Rust name back to
//!   `wchar_t`, producing the identical Rust type and the correct C spelling.
//! - `FILE *`. The core's `CFile` is `*mut c_void`, which is not `FILE *`.
//!   The generator renames it; the alias here is what there is to rename.
//! - The C's own names for the readline records — `HIST_ENTRY`,
//!   `KEYMAP_ENTRY`, `HISTORY_STATE` — which the core and declaration aliases
//!   spell in Rust casing and the generator maps back at the header boundary.
//!
//! # Nothing here may drift
//!
//! Every alias in this module is the *same Rust type* as the core item it
//! restates, not a copy of it, and each one is checked by an identity
//! function that the compiler accepts only if the two types are literally
//! the same. A type that started to disagree with the core's would fail to
//! build rather than fail to be noticed — which is the failure mode
//! `plan/main.styx`'s correction on `abi-headers` exists to rule out.

pub mod handles;
pub mod histedit;
pub mod readline;
