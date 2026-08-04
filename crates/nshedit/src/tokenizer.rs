//! Ported from `src/tokenizer.c`; rules live in
//! `docs/spec/port/src/tokenizer.md`.
//!
//! The C compiles this file twice — wide here, narrow via `tokenizern.c`.
//! Only the wide instantiation carries rules in the port manifest; the
//! narrow handle is [`crate::histedit::Tokenizer`].
//!
//! Function names are the wide instantiation's: `FUN(tok,init)` expands to
//! `tok_winit` here, matching `TYPE(Tokenizer)` being [`TokenizerW`].

use crate::histedit::LineInfoW;

/// C: `#define TOK_KEEP 1` — "this word exists even though it produced no
/// elements". Set by `'`, `"` and `\`, cleared only by [`tok_wfinish`].
const TOK_KEEP: i32 = 1;

/// C: `#define TOK_EAT 2` — the last thing consumed was a backslash-newline
/// pair, so end of input means "quoted return" (3) rather than a complete
/// parse.
const TOK_EAT: i32 = 2;

/// C: `#define WINCR 20` — the word buffer's initial size and its growth
/// step. Growth is linear, not doubling.
const WINCR: usize = 20;

/// C: `#define AINCR 10` — the `argv` array's initial size and growth step.
const AINCR: usize = 10;

/// C: `#define IFS STR("\t \n")` — the default separator set, used when the
/// caller passes NULL to [`tok_winit`]. Held without a terminator: the C's
/// `Strchr` would also match the terminating NUL, but a NUL element never
/// reaches the separator test (it has its own switch case), so the two are
/// equivalent.
const IFS: [u32; 3] = [0x09, 0x20, 0x0a];

/// The five elements the dispatch matches, as the ASCII code points the C
/// compares against in both instantiations — `'`, `"`, `\`, newline, NUL.
/// Named constants because Rust patterns take no cast expressions.
const C_SQUOTE: u32 = 0x27;
const C_DQUOTE: u32 = 0x22;
const C_BSLASH: u32 = 0x5c;
const C_NEWLINE: u32 = 0x0a;
const C_NUL: u32 = 0x00;

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
    // Terminate the pending word in place, without advancing `wptr`. The
    // caller's growth slack guarantees this slot exists; there is no bounds
    // check in the C and none is needed here.
    tok.wspace[tok.wptr] = 0;
    if (tok.flags & TOK_KEEP) != 0 || tok.wptr != tok.wstart {
        // Publish. `argv` slots are offsets into `wspace`, so the C's
        // `tok->argv[tok->argc++] = tok->wstart` is the offset itself.
        tok.argv[tok.argc] = Some(tok.wstart);
        tok.argc += 1;
        // `argv[argc] = NULL` is written only on this path, which is what
        // ERR-input-38 turns into an observable defect after `tok_wreset`.
        tok.argv[tok.argc] = None;
        tok.wptr += 1;
        tok.wstart = tok.wptr;
    }
    // Otherwise the NUL written above is inert: the next element emitted
    // overwrites it, which is how a run of separators collapses.
    tok.flags &= !TOK_KEEP;
}

// [spec:libedit:def:tokenizer.fun-tok-init-fn]
// [spec:libedit:sem:tokenizer.fun-tok-init-fn]
/// C: `TYPE(Tokenizer) * FUN(tok,init)(const Char *ifs)`.
///
/// `None` for `ifs` is the C's NULL, which selects the default `"\t \n"`;
/// `None` for the return is an allocation failure. The `Box` is the C's
/// `malloc`ed handle, which [`tok_wend`] frees.
pub fn tok_winit(ifs: Option<&[u32]>) -> Option<Box<TokenizerW>> {
    // The C's four allocations each abort to a NULL return, freeing whatever
    // it had already taken. Rust aborts on allocation failure rather than
    // reporting it, so `None` is unreachable here; the return type keeps the
    // C's contract because callers are specified to check it (and
    // ERR-input-13 is a caller that does not).
    Some(Box::new(TokenizerW {
        // The C copies the caller's string with `wcsdup`, so it is not
        // retained. A NUL inside the slice would truncate that copy; it
        // cannot be observed, since a NUL element never reaches the
        // separator test, so the slice is copied whole.
        ifs: ifs.unwrap_or(&IFS).to_vec(),
        argc: 0,
        amax: AINCR,
        // C: `argv[0] = NULL` only. The remaining slots are uninitialised
        // there and `None` here; nothing reads past `argc`.
        argv: vec![None; AINCR],
        wptr: 0,
        wmax: WINCR,
        wstart: 0,
        // C leaves the word buffer's contents uninitialised. Every element
        // is written before it is read, so zeroing is unobservable.
        wspace: vec![0; WINCR],
        quote: QuoteT::QNone,
        flags: 0,
    }))
}

// [spec:libedit:def:tokenizer.fun-tok-reset-fn]
// [spec:libedit:sem:tokenizer.fun-tok-reset-fn]
/// C: `void FUN(tok,reset)(TYPE(Tokenizer) *tok)`.
pub fn tok_wreset(tok: &mut TokenizerW) {
    tok.argc = 0;
    tok.wstart = 0;
    tok.wptr = 0;
    tok.flags = 0;
    tok.quote = QuoteT::QNone;
    // Exactly five assignments, as in the C. In particular `argv[0]` is
    // *not* restored to `None`: the stale offset from the previous parse
    // survives, so a following `tok_wline` that publishes no word leaves the
    // array without its terminator. Reproduced deliberately — ERR-input-38.
}

// [spec:libedit:def:tokenizer.fun-tok-end-fn]
// [spec:libedit:sem:tokenizer.fun-tok-end-fn]
/// C: `void FUN(tok,end)(TYPE(Tokenizer) *tok)` — four `free`s, including
/// the handle itself, so this consumes the `Box` [`tok_winit`] handed out.
#[allow(clippy::boxed_local)]
pub fn tok_wend(tok: Box<TokenizerW>) {
    // C: `free(ifs)`, `free(wspace)`, `free(argv)`, `free(tok)`, in that
    // order, with nothing zeroed first. Dropping the box runs the three Vec
    // deallocations and then releases the handle. Taking the `Box` by value
    // is what makes the C's double free and its missing NULL guard
    // unrepresentable.
    drop(tok);
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
    /// The rule's "emit x": C `*tok->wptr++ = x`. No bounds check and no
    /// allocation, exactly as in the C — the previous pass's growth step
    /// left at least four free elements, which covers the two this can be
    /// asked for in one pass plus [`tok_wfinish`]'s terminator.
    fn emit(tok: &mut TokenizerW, x: u32) {
        tok.wspace[tok.wptr] = x;
        tok.wptr += 1;
    }

    let mut cc: i32 = -1;
    let mut co: i32 = -1;
    let mut ptr = line.buffer;

    'tokenize: loop {
        // (a) End-of-input substitution. The C repoints `ptr` at a static
        // empty string so `*ptr` reads as NUL, then increments past it on
        // the next pass and compares that out-of-object pointer against
        // `lastchar` — undefined, and on Linux/x86-64 the comparison
        // usually fails and the loop walks off the literal (ERR-input-15).
        //
        // Defined here as the intent rather than the accident: everything
        // at or past `lastchar` reads as NUL, indefinitely. `ptr` only ever
        // moves forward from `buffer`, so once it reaches `lastchar` it
        // stays there and no read is ever performed out of bounds. The
        // frozen consequence is kept: a trailing backslash still drives the
        // `Q_one` NUL arm once, appending an extra NUL element to the word
        // that is invisible in the word itself but counted by `co`.
        let c = if ptr >= line.lastchar {
            C_NUL
        } else {
            // (b) Cursor capture, by pointer identity and before the
            // element is processed. Substitution runs first in the C, so a
            // cursor at or past `lastchar` — or NULL, or below `buffer` —
            // never matches and falls through to the success exit's
            // fallback. Both casts truncate past `INT_MAX`, as in the C.
            if ptr == line.cursor {
                cc = tok.argc as i32;
                co = (tok.wptr - tok.wstart) as i32;
            }
            // SAFETY: `ptr` started at `line.buffer` and has advanced by
            // one element at a time without reaching `line.lastchar`, so it
            // addresses an element of the caller's line buffer. That the
            // three pointers describe one live region is the contract
            // `def:histedit.line-info-w` inherits from the C.
            unsafe { *ptr }
        };

        // (c) Dispatch. The five special elements are matched before the
        // `ifs` test, so none of them can ever act as a separator however
        // `ifs` is set — and the default `ifs` does contain a newline, which
        // therefore ends the line instead of splitting a word
        // (ERR-input-39, reproduced). Flag housekeeping is per-case, not
        // uniform. Each of the C's `default:` arms — four returning -1 and
        // the newline one returning 0 — is unreachable over a five-valued
        // `quote_t` and unrepresentable over a Rust enum, so it is not
        // ported (ERR-input-43).
        match c {
            C_SQUOTE => {
                tok.flags |= TOK_KEEP;
                tok.flags &= !TOK_EAT;
                match tok.quote {
                    // Enter single quote mode.
                    QuoteT::QNone => tok.quote = QuoteT::QSingle,
                    // Exit single quote mode.
                    QuoteT::QSingle => tok.quote = QuoteT::QNone,
                    // Quote this '.
                    QuoteT::QOne => {
                        tok.quote = QuoteT::QNone;
                        emit(tok, c);
                    }
                    // Stay in double quote mode.
                    QuoteT::QDouble => emit(tok, c),
                    // Quote this ' — dropping the backslash, where sh(1)
                    // keeps it. Deliberate-looking divergence, preserved:
                    // ERR-input-41.
                    QuoteT::QDoubleone => {
                        tok.quote = QuoteT::QDouble;
                        emit(tok, c);
                    }
                }
            }

            C_DQUOTE => {
                tok.flags &= !TOK_EAT;
                tok.flags |= TOK_KEEP;
                match tok.quote {
                    // Enter double quote mode.
                    QuoteT::QNone => tok.quote = QuoteT::QDouble,
                    // Exit double quote mode.
                    QuoteT::QDouble => tok.quote = QuoteT::QNone,
                    // Quote this ".
                    QuoteT::QOne => {
                        tok.quote = QuoteT::QNone;
                        emit(tok, c);
                    }
                    // Stay in single quote mode.
                    QuoteT::QSingle => emit(tok, c),
                    // Quote this ".
                    QuoteT::QDoubleone => {
                        tok.quote = QuoteT::QDouble;
                        emit(tok, c);
                    }
                }
            }

            C_BSLASH => {
                tok.flags |= TOK_KEEP;
                tok.flags &= !TOK_EAT;
                match tok.quote {
                    // Quote next character.
                    QuoteT::QNone => tok.quote = QuoteT::QOne,
                    // Quote next character.
                    QuoteT::QDouble => tok.quote = QuoteT::QDoubleone,
                    // Quote this, restore state.
                    QuoteT::QOne => {
                        emit(tok, c);
                        tok.quote = QuoteT::QNone;
                    }
                    // Stay in single quote mode.
                    QuoteT::QSingle => emit(tok, c),
                    // Quote this \.
                    QuoteT::QDoubleone => {
                        tok.quote = QuoteT::QDouble;
                        emit(tok, c);
                    }
                }
            }

            C_NEWLINE => {
                tok.flags &= !TOK_EAT;
                match tok.quote {
                    // The line is complete. TOK_KEEP is not set here.
                    QuoteT::QNone => break 'tokenize,
                    // Add the return: a newline inside quotes is an
                    // ordinary element and does not end the line.
                    QuoteT::QSingle | QuoteT::QDouble => emit(tok, c),
                    // Back to double, eat the '\n'.
                    QuoteT::QDoubleone => {
                        tok.flags |= TOK_EAT;
                        tok.quote = QuoteT::QDouble;
                    }
                    // No quote, more, eat the '\n'.
                    QuoteT::QOne => {
                        tok.flags |= TOK_EAT;
                        tok.quote = QuoteT::QNone;
                    }
                }
            }

            // Either an element inside the buffer — an embedded NUL
            // truncates the line — or the end-of-input NUL from (a).
            // Neither flag is touched on entry.
            C_NUL => match tok.quote {
                QuoteT::QNone => {
                    // Finish word and return.
                    if (tok.flags & TOK_EAT) != 0 {
                        tok.flags &= !TOK_EAT;
                        return 3;
                    }
                    break 'tokenize;
                }
                QuoteT::QSingle => return 1,
                QuoteT::QDouble => return 2,
                QuoteT::QDoubleone => {
                    tok.quote = QuoteT::QDouble;
                    emit(tok, c);
                }
                QuoteT::QOne => {
                    tok.quote = QuoteT::QNone;
                    emit(tok, c);
                }
            },

            _ => {
                tok.flags &= !TOK_EAT;
                match tok.quote {
                    QuoteT::QNone => {
                        // C: `Strchr(tok->ifs, *ptr) != NULL`. Element-wise,
                        // with no multibyte or locale awareness. `Strchr`
                        // would also match the terminating NUL, which cannot
                        // be reached: NUL has its own case above.
                        if tok.ifs.contains(&c) {
                            tok_wfinish(tok);
                        } else {
                            emit(tok, c);
                        }
                    }
                    QuoteT::QSingle | QuoteT::QDouble => emit(tok, c),
                    // A backslash inside double quotes is preserved before
                    // anything other than ' " \ newline NUL. The only arm
                    // that emits two elements in one pass.
                    QuoteT::QDoubleone => {
                        emit(tok, C_BSLASH);
                        tok.quote = QuoteT::QDouble;
                        emit(tok, c);
                    }
                    QuoteT::QOne => {
                        tok.quote = QuoteT::QNone;
                        emit(tok, c);
                    }
                }
            }
        }

        // (d) Growth, reached only when (c) fell through. Linear, not
        // doubling. The C rebases every published `argv[i]`, `wptr` and
        // `wstart` when the block moves; here they are offsets, so there is
        // nothing to rebase. The C's two -1 returns for a failed `realloc`
        // are unreachable in Rust, which aborts on allocation failure.
        if tok.wptr >= tok.wmax - 4 {
            let size = tok.wmax + WINCR;
            tok.wspace.resize(size, 0);
            tok.wmax = size;
        }
        if tok.argc >= tok.amax - 4 {
            tok.amax += AINCR;
            tok.argv.resize(tok.amax, None);
        }

        ptr = ptr.wrapping_add(1);
    }

    // Success exit, in the C's order. Step 3 running after step 2 is
    // load-bearing: `cc` can therefore equal the final `*argc`, naming a
    // word that was never published.
    if cc == -1 && co == -1 {
        cc = tok.argc as i32;
        co = (tok.wptr - tok.wstart) as i32;
    }
    if let Some(cursorc) = cursorc {
        *cursorc = cc;
    }
    if let Some(cursoro) = cursoro {
        *cursoro = co;
    }
    tok_wfinish(tok);
    // C: `*argv = tok->argv` — dropped, see the note above. The words are
    // `tok.argv[..argc]`, each an offset into `tok.wspace`.
    *argc = tok.argc as i32;
    0
}

// [spec:libedit:def:tokenizer.fun-tok-str-fn]
// [spec:libedit:sem:tokenizer.fun-tok-str-fn]
/// C: `int FUN(tok,str)(TYPE(Tokenizer) *tok, const Char *line, int *argc,
/// const Char ***argv)`.
///
/// The C's NUL-terminated `line` is the slice; `argv` is dropped for the
/// reason given on [`tok_wline`].
pub fn tok_wstr(tok: &mut TokenizerW, line: &[u32], argc: &mut i32) -> i32 {
    // C: `li.cursor = li.lastchar = Strchr(line, '\0')` — the address of the
    // terminating NUL, one past the last character. A slice with no NUL in
    // it would run that search off the end in the C, which the rule leaves
    // undefined; defined here as the end of the slice.
    let end = line.iter().position(|&x| x == 0).unwrap_or(line.len());
    let buffer = line.as_ptr();
    // The C's `memset` is redundant — all three fields are assigned.
    let li = LineInfoW {
        buffer,
        cursor: buffer.wrapping_add(end),
        lastchar: buffer.wrapping_add(end),
    };
    // Because `cursor == lastchar`, the in-loop cursor match can never fire;
    // the bookkeeping falls through to the end-of-input fallback and is then
    // discarded, both cursor out-parameters being NULL. Return value
    // verbatim, the full 0/1/2/3/-1 set — `tok_wstr` is not restricted to
    // complete lines.
    tok_wline(tok, &li, argc, None, None)
}
