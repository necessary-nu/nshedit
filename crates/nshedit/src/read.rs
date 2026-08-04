//! Ported from `src/read.c`; rules live in `docs/spec/port/src/read.md`.

use crate::histedit::ElRfuncT;

/// C: `#define EL_MAXMACRO 10` — the macro nesting limit.
pub const EL_MAXMACRO: usize = 10;

// [spec:libedit:def:read.macros]
/// The macro pushback stack.
pub struct Macros {
    /// C: `wchar_t **macro` — up to `EL_MAXMACRO` owned strings, innermost
    /// last. `macro` is a Rust keyword, so the name is written `r#macro`;
    /// it is still the C's field name.
    pub r#macro: Vec<Vec<u32>>,
    /// Index of the innermost live macro, -1 when none is running.
    pub level: i32,
    /// Read position within `macro[level]`.
    pub offset: i32,
}

// [spec:libedit:def:read.el-read-t]
/// The character-reading state, hung off `EditLine::el_read`.
pub struct ElReadT {
    pub macros: Macros,
    /// Function to read a character.
    pub read_char: Option<ElRfuncT>,
    /// The `errno` the last read failed with, surfaced through
    /// `EL_GETCFN`'s error reporting.
    pub read_errno: i32,
}
