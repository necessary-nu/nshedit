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
//! An incomplete type has no public body to agree about. The ABI allocations
//! stay private; these declaration-only tags exist solely to be spelled in
//! the installed header. Completed records live beside them in
//! [`super::histedit`], never in the core's header input.

// Rust owns the source-level names; cbindgen's checked rename table owns the
// C tag spellings carried by the committed generated header.

/// C: `struct editline` — the editor, `def:el.editline`.
pub struct EditlineTag;
// [spec:libedit:def:histedit.edit-line]
/// C: `typedef struct editline EditLine;` — `def:histedit.edit-line`.
pub type EditLine = EditlineTag;

/// C: `struct history` — the narrow history, `historyn.c`.
pub struct HistoryTag;
// [spec:libedit:def:histedit.history]
/// C: `typedef struct history History;` — `def:histedit.history`.
pub type History = HistoryTag;

/// C: `struct historyW` — the wide history, `history.c`.
pub struct HistoryWideTag;
// [spec:libedit:def:histedit.history-w]
/// C: `typedef struct historyW HistoryW;` — `def:histedit.history-w`.
pub type HistoryW = HistoryWideTag;

/// C: `struct tokenizer` — the narrow tokenizer, `tokenizern.c`.
pub struct TokenizerTag;
// [spec:libedit:def:histedit.tokenizer]
/// C: `typedef struct tokenizer Tokenizer;` — `def:histedit.tokenizer`.
pub type Tokenizer = TokenizerTag;

/// C: `struct tokenizerW` — the wide tokenizer, `tokenizer.c`.
pub struct TokenizerWideTag;
// [spec:libedit:def:histedit.tokenizer-w]
/// C: `typedef struct tokenizerW TokenizerW;` — `def:histedit.tokenizer-w`.
pub type TokenizerW = TokenizerWideTag;
