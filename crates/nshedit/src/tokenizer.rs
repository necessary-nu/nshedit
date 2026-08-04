//! Ported from `src/tokenizer.c`; rules live in
//! `docs/spec/port/src/tokenizer.md`.
//!
//! The C compiles this file twice — wide here, narrow via `tokenizern.c`.
//! Only the wide instantiation carries rules in the port manifest; the
//! narrow handle is [`crate::histedit::Tokenizer`].

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
