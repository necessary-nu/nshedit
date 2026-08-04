//! Ported from `src/tokenizer.c`; rules live in
//! `docs/spec/port/src/tokenizer.md`.
//!
//! The C compiles this file twice — wide here, narrow via `tokenizern.c`.
//! Only the wide instantiation carries rules in the port manifest; the
//! narrow handle is [`crate::histedit::Tokenizer`].
//!
//! Function names are the wide instantiation's: `FUN(tok,init)` expands to
//! `tok_winit` here, matching `TYPE(Tokenizer)` being [`TokenizerW`].

// Bodies are not written yet, so every parameter is unused. Remove this once
// the translations land.
#![allow(unused_variables)]

use crate::histedit::LineInfoW;

// [spec:libedit:def:tokenizer.quote-t]
/// The quoting state machine. A genuine C `enum`, so a Rust enum.
pub enum QuoteT {
    /// No quoting.
    QNone,
    /// Single quotes.
    QSingle,
    /// Double quotes.
    QDouble,
    /// Single quote, one character.
    QOne,
    /// Double quote, one character.
    QDoubleone,
}

/// C: `struct TYPE(tokenizer)` — the wide tokenizer, named `TokenizerW` by
/// `def:histedit.tokenizer-w`. The C defines this body with
/// no rule of its own, which is why there is no annotation here.
///
/// `wptr`, `wmax`, `wstart` and every `argv` slot are pointers into
/// `wspace` in the C, and `tok_line` rebases them after each `realloc`, so
/// they are offsets here.
pub struct TokenizerW {
    /// C: `Char *ifs` — in-field separators, owned. Defaults to `L"\t \n"`.
    pub ifs: Vec<u32>,
    /// Current number of arguments.
    pub argc: usize,
    /// Maximum number of arguments (the `argv` capacity, initially 10).
    pub amax: usize,
    /// C: `const Char **argv` — one offset into `wspace` per argument;
    /// `None` is the C's NULL terminator slot.
    pub argv: Vec<Option<usize>>,
    /// C: `Char *wptr` — write position, offset into `wspace`.
    pub wptr: usize,
    /// C: `Char *wmax` — limit, offset into `wspace`.
    pub wmax: usize,
    /// C: `Char *wstart` — beginning of the next word, offset into
    /// `wspace`.
    pub wstart: usize,
    /// C: `Char *wspace` — the word buffer, owned. Starts at 20 elements.
    pub wspace: Vec<u32>,
    /// Quoting state.
    pub quote: QuoteT,
    /// C: `int flags` — `TOK_KEEP` (1) and `TOK_EAT` (2). Kept an integer
    /// flag word.
    pub flags: i32,
}

// [spec:libedit:def:tokenizer.fun-tok-finish-fn]
// [spec:libedit:sem:tokenizer.fun-tok-finish-fn]
/// C: `static void FUN(tok,finish)(TYPE(Tokenizer) *tok)`.
fn tok_wfinish(tok: &mut TokenizerW) {
    todo!()
}

// [spec:libedit:def:tokenizer.fun-tok-init-fn]
// [spec:libedit:sem:tokenizer.fun-tok-init-fn]
/// C: `TYPE(Tokenizer) * FUN(tok,init)(const Char *ifs)`.
///
/// `None` for `ifs` is the C's NULL, which selects the default `"\t \n"`;
/// `None` for the return is an allocation failure. The `Box` is the C's
/// `malloc`ed handle, which [`tok_wend`] frees.
pub fn tok_winit(ifs: Option<&[u32]>) -> Option<Box<TokenizerW>> {
    todo!()
}

// [spec:libedit:def:tokenizer.fun-tok-reset-fn]
// [spec:libedit:sem:tokenizer.fun-tok-reset-fn]
/// C: `void FUN(tok,reset)(TYPE(Tokenizer) *tok)`.
pub fn tok_wreset(tok: &mut TokenizerW) {
    todo!()
}

// [spec:libedit:def:tokenizer.fun-tok-end-fn]
// [spec:libedit:sem:tokenizer.fun-tok-end-fn]
/// C: `void FUN(tok,end)(TYPE(Tokenizer) *tok)` — four `free`s, including
/// the handle itself, so this consumes the `Box` [`tok_winit`] handed out.
#[allow(clippy::boxed_local)]
pub fn tok_wend(tok: Box<TokenizerW>) {
    todo!()
}

// [spec:libedit:def:tokenizer.fun-tok-line-fn]
// [spec:libedit:sem:tokenizer.fun-tok-line-fn]
/// C: `int FUN(tok,line)(TYPE(Tokenizer) *tok, const TYPE(LineInfo) *line,
/// int *argc, const Char ***argv, int *cursorc, int *cursoro)`.
///
/// The C's `*argv = tok->argv` hands back an alias of the tokenizer's own
/// array, which the port cannot do while `tok` is uniquely borrowed — and
/// need not, because `argv`'s slots are offsets into `wspace` rather than
/// pointers. The out-parameter is therefore dropped: after this returns, the
/// words are `tok.argv[..argc]` resolved against `tok.wspace`. `argc` stays,
/// unconditional as in the C; `cursorc` and `cursoro` are NULL-checked
/// there, so they are `Option`.
///
/// Returns the C's status: -1 internal error, 3 quoted return, 2 unmatched
/// double quote, 1 unmatched single quote, 0 ok.
pub fn tok_wline(
    tok: &mut TokenizerW,
    line: &LineInfoW,
    argc: &mut i32,
    cursorc: Option<&mut i32>,
    cursoro: Option<&mut i32>,
) -> i32 {
    todo!()
}

// [spec:libedit:def:tokenizer.fun-tok-str-fn]
// [spec:libedit:sem:tokenizer.fun-tok-str-fn]
/// C: `int FUN(tok,str)(TYPE(Tokenizer) *tok, const Char *line, int *argc,
/// const Char ***argv)`.
///
/// The C's NUL-terminated `line` is the slice; `argv` is dropped for the
/// reason given on [`tok_wline`].
pub fn tok_wstr(tok: &mut TokenizerW, line: &[u32], argc: &mut i32) -> i32 {
    todo!()
}
