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

// Rust owns the source-level names; cbindgen's checked rename table owns the
// C tag spellings. `conformance/header-diff.sh` proves that the latter remain
// exactly the declarations consumers compile against.

/// C: `struct editline` — the editor, `def:el.editline`.
pub struct EditlineTag;
/// C: `typedef struct editline EditLine;` — `def:histedit.edit-line`.
pub type EditLine = EditlineTag;

/// C: `struct history` — the narrow history, `historyn.c`.
pub struct HistoryTag;
/// C: `typedef struct history History;` — `def:histedit.history`.
pub type History = HistoryTag;

/// C: `struct historyW` — the wide history, `history.c`.
pub struct HistoryWideTag;
/// C: `typedef struct historyW HistoryW;` — `def:histedit.history-w`.
pub type HistoryW = HistoryWideTag;

/// C: `struct tokenizer` — the narrow tokenizer, `tokenizern.c`.
pub struct TokenizerTag;
/// C: `typedef struct tokenizer Tokenizer;` — `def:histedit.tokenizer`.
pub type Tokenizer = TokenizerTag;

/// C: `struct tokenizerW` — the wide tokenizer, `tokenizer.c`.
pub struct TokenizerWideTag;
/// C: `typedef struct tokenizerW TokenizerW;` — `def:histedit.tokenizer-w`.
pub type TokenizerW = TokenizerWideTag;
