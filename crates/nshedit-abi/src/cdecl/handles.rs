//! `histedit.h`'s five incomplete types.
//!
//! C:
//!
//! ```c
//! typedef struct editline   EditLine;
//! typedef struct history    History;
//! typedef struct historyW   HistoryW;
//! typedef struct tokenizer  Tokenizer;
//! typedef struct tokenizerW TokenizerW;
//! ```
//!
//! Each names a `struct` tag that no public header ever completes: the
//! consumer may hold a pointer to one and may not look inside. Rust has no
//! incomplete type, so each is a unit struct here — cbindgen renders one as
//! `struct editline;`, a forward declaration and nothing more, which is
//! exactly the C's meaning.
//!
//! These are the only declarations in [`crate::cdecl`] with no counterpart
//! in the core, and they need none: an incomplete type has no body to agree
//! about. The bodies live in `nshedit`, are never named by a header, and
//! the C's are equally private (`src/el.h`, `src/hist.h`,
//! `src/tokenizer.c`). Nothing constructs one of these; they exist to be
//! spelled.
//!
//! Deliberately *not* here: `struct lineinfo`, `struct lineinfow`,
//! `struct HistEvent` and `struct histeventW`. Those four the C completes,
//! so a consumer reads their fields, so their layout is the contract and
//! they are generated from the core's real types rather than restated. That
//! is the whole point of `conformance-header-diff`.

// These names are C `struct` tags, and cbindgen emits a type's Rust name
// verbatim. Renaming them to Rust casing and mapping them back in the
// generator's config would put the C spelling somewhere the compiler cannot
// see it, which is the opposite of what this module is for.
#![allow(non_camel_case_types)]

/// C: `struct editline` — the editor, `def:el.editline`.
pub struct editline;
/// C: `typedef struct editline EditLine;` — `def:histedit.edit-line`.
pub type EditLine = editline;

/// C: `struct history` — the narrow history, `historyn.c`.
pub struct history;
/// C: `typedef struct history History;` — `def:histedit.history`.
pub type History = history;

/// C: `struct historyW` — the wide history, `history.c`.
pub struct historyW;
/// C: `typedef struct historyW HistoryW;` — `def:histedit.history-w`.
pub type HistoryW = historyW;

/// C: `struct tokenizer` — the narrow tokenizer, `tokenizern.c`.
pub struct tokenizer;
/// C: `typedef struct tokenizer Tokenizer;` — `def:histedit.tokenizer`.
pub type Tokenizer = tokenizer;

/// C: `struct tokenizerW` — the wide tokenizer, `tokenizer.c`.
pub struct tokenizerW;
/// C: `typedef struct tokenizerW TokenizerW;` — `def:histedit.tokenizer-w`.
pub type TokenizerW = tokenizerW;
