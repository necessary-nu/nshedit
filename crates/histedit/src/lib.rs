//! A Rust re-implementation of libedit.
//!
//! Modules mirror the C source file for file, so that each `sem` rule in
//! `docs/spec/port/src/` has one obvious home and the two implementations can
//! be diffed against each other. Idiomatic shape comes later, once the ported
//! behaviour is under test.
//!
//! Text is carried as `u32`, not `char`. The screen image stores sentinel
//! values that are not Unicode scalar values — see
//! `docs/spec/port/src/literal.md` — and the C admits `wchar_t` values that
//! `char` forbids.

// Encoding and escaping.
pub mod chartype;
pub mod literal;
pub mod unvis;
pub mod vis;

// Terminal capability and tty control.
pub mod terminal;
pub mod tty;

// Line buffer and screen refresh.
pub mod chared;
pub mod prompt;
pub mod refresh;

// Input dispatch and key binding.
pub mod keymacro;
pub mod map;
pub mod parse;
pub mod read;
pub mod sig;

// History storage and search.
pub mod hist;
pub mod history;
pub mod search;

// Editor command sets.
pub mod common;
pub mod emacs;
pub mod vi;

// Completion and tokenization.
pub mod filecomplete;
pub mod tokenizer;

// EditLine lifecycle.
pub mod el;
