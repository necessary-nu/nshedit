//! Ported from `src/tokenizer.c`; rules live in
//! `docs/spec/port/src/tokenizer.md`.
//!
//! This temporary compatibility parser remains only for the translated
//! editrc command path. Public C tokenizers are owned by `nshedit-abi` and
//! parse through the native `editor::Tokenizer`; no narrow instantiation is
//! retained in the core.

use crate::histedit::LineInfoGen;

/// C: `#define WINCR 20` — the word buffer's initial size and its growth
/// step. Growth is linear, not doubling.
const WINCR: usize = 20;

/// C: `#define AINCR 10` — the `argv` array's initial size and growth step.
const AINCR: usize = 10;

/// C: `#define IFS STR("\t \n")` — the default separator set, used when the
/// caller passes NULL to [`tok_init_gen`]. Held without a terminator: the C's
/// `Strchr` would also match the terminating NUL, but a NUL element never
/// reaches the separator test (it has its own switch case), so the two are
/// equivalent.
static IFS_W: [u32; 3] = [0x09, 0x20, 0x0a];

/// The five elements the dispatch matches, as the ASCII code points the C
/// compares against in both instantiations — `'`, `"`, `\`, newline, NUL.
/// Named constants because Rust patterns take no cast expressions.
const C_SQUOTE: u32 = 0x27;
const C_DQUOTE: u32 = 0x22;
const C_BSLASH: u32 = 0x5c;
const C_NEWLINE: u32 = 0x0a;
const C_NUL: u32 = 0x00;

/// The C's `Char` — `wchar_t` in `tokenizer.c`, `char` in `tokenizern.c` —
/// and the three macros whose expansion differs between them.
///
/// Not a ported C type: it is `tokenizer.c`'s `#ifdef NARROWCHAR` table, made
/// a trait so the one source below can be instantiated twice. All of it is
/// substitution — there is no analogue here of `history.c`'s
/// `ct_decode_string`/`ct_encode_string` fork.
///
/// `Strchr` does not appear: the C's `Strchr(tok->ifs, *ptr) != NULL` is
/// `ifs.contains(&c)` over `PartialEq`, element for element in either build.
pub trait TokChar: Copy + PartialEq + 'static {
    /// C: `'\0'` as a `Char`.
    const NUL: Self;
    /// C: `'\\'` as a `Char`. The one element the state machine emits without
    /// having read it, so the only one that needs a name on this side.
    const BSLASH: Self;

    /// C: `IFS`, i.e. `STR("\t \n")`.
    fn default_ifs() -> &'static [Self];

    /// The value the C's `switch (*ptr)` compares against its five `case`
    /// labels.
    ///
    /// In the wide build that is the `wchar_t` itself. In the narrow build the
    /// C promotes a possibly-signed `char` to `int`, so byte 0x80 arrives as
    /// -128; this widens without sign instead. The two agree because the five
    /// labels are all ASCII and neither route can map a byte at or above 0x80
    /// onto one of them.
    fn code(self) -> u32;
}

/// `Char = wchar_t`: `tokenizer.c` compiled as itself.
impl TokChar for u32 {
    const NUL: Self = C_NUL;
    const BSLASH: Self = C_BSLASH;

    fn default_ifs() -> &'static [Self] {
        &IFS_W
    }

    fn code(self) -> u32 {
        self
    }
}

/// C: `TYPE(Tokenizer)` with `Char = wchar_t` — `tokenizerW`, the wide
/// handle.
pub type TokenizerW = TokenizerGen<u32>;

// [spec:libedit:def:tokenizer.quote-t]
/// The quoting state machine. A genuine C `enum`, so a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteState {
    /// No quoting.
    Plain,
    /// Single quotes.
    Single,
    /// Double quotes.
    Double,
    /// Single quote, one character.
    EscapePlain,
    /// Double quote, one character.
    EscapeDouble,
}

/// C: `struct TYPE(tokenizer)`, named `TokenizerW` by
/// `def:histedit.tokenizer-w` and `Tokenizer` by `def:histedit.tokenizer`.
/// The C defines this body with no rule of its own, which is why there is no
/// annotation here.
///
/// `wptr`, `wstart` and every `argv` slot are pointers into `wspace` in the
/// C, and `tok_line` rebases them after each `realloc`, so they are offsets
/// here.
///
/// Two C members have no field of their own. `wmax` and `amax` are the ends
/// of the two blocks, which a `char *` cannot report and a `Vec` can: they
/// are `wspace.len()` and `argv.len()`, always, so keeping them would be
/// keeping an invariant that can only ever be broken. `flags` is a two-bit
/// word and is the two bits.
///
/// Only `argv` and `wspace` are public. The C's handle is opaque —
/// `histedit.h` declares the struct and defines it nowhere a consumer can
/// see — and the two exceptions are what the ABI crate resolves a published
/// `argv` slot against.
pub struct TokenizerGen<C> {
    /// C: `Char *ifs` — in-field separators, owned. Defaults to `L"\t \n"`.
    pub(crate) ifs: Vec<C>,
    /// Current number of arguments.
    pub(crate) argc: usize,
    /// C: `const Char **argv` — one offset into `wspace` per argument;
    /// `None` is the C's NULL terminator slot. Its length is the C's `amax`.
    pub argv: Vec<Option<usize>>,
    /// C: `Char *wptr` — write position, offset into `wspace`.
    pub(crate) wptr: usize,
    /// C: `Char *wstart` — beginning of the next word, offset into
    /// `wspace`.
    pub(crate) wstart: usize,
    /// C: `Char *wspace` — the word buffer, owned. Starts at 20 elements,
    /// and its length is the C's `wmax`.
    pub wspace: Vec<C>,
    /// Quoting state.
    pub(crate) quote: QuoteState,
    /// C: `flags & TOK_KEEP` — "this word exists even though it produced no
    /// elements". Set by `'`, `"` and `\`, cleared only by
    /// [`tok_finish_gen`].
    pub(crate) keep: bool,
    /// C: `flags & TOK_EAT` — the last thing consumed was a backslash-newline
    /// pair, so end of input means "quoted return" (3) rather than a complete
    /// parse.
    pub(crate) eat: bool,
}

impl<C: TokChar> TokenizerGen<C> {
    /// The rule's "emit x": C `*tok->wptr++ = x`.
    ///
    /// No bounds check and no allocation, exactly as in the C — the previous
    /// pass's growth step left at least four free elements, which covers the
    /// two this can be asked for in one pass plus [`tok_finish_gen`]'s
    /// terminator.
    fn emit(&mut self, x: C) {
        self.wspace[self.wptr] = x;
        self.wptr += 1;
    }
}

// [spec:libedit:def:tokenizer.fun-tok-finish-fn]
// [spec:libedit:sem:tokenizer.fun-tok-finish-fn]
/// C: `static void FUN(tok,finish)(TYPE(Tokenizer) *tok)`.
fn tok_finish_gen<C: TokChar>(tok: &mut TokenizerGen<C>) {
    // Terminate the pending word in place, without advancing `wptr`. The
    // caller's growth slack guarantees this slot exists; there is no bounds
    // check in the C and none is needed here.
    tok.wspace[tok.wptr] = C::NUL;
    if tok.keep || tok.wptr != tok.wstart {
        // Publish. `argv` slots are offsets into `wspace`, so the C's
        // `tok->argv[tok->argc++] = tok->wstart` is the offset itself.
        tok.argv[tok.argc] = Some(tok.wstart);
        tok.argc += 1;
        // `argv[argc] = NULL` is written only on this path, which is what
        // ERR-input-38 turns into an observable defect after an ABI reset.
        tok.argv[tok.argc] = None;
        tok.wptr += 1;
        tok.wstart = tok.wptr;
    }
    // Otherwise the NUL written above is inert: the next element emitted
    // overwrites it, which is how a run of separators collapses.
    //
    // `eat` is deliberately untouched.
    tok.keep = false;
}

// [spec:libedit:def:tokenizer.fun-tok-init-fn]
// [spec:libedit:sem:tokenizer.fun-tok-init-fn]
/// C: `TYPE(Tokenizer) * FUN(tok,init)(const Char *ifs)`.
///
/// `None` for `ifs` is the C's NULL, which selects the default `"\t \n"`;
/// `None` for the return is an allocation failure. The `Box` is the C's
/// `malloc`ed handle, which [`tok_end_gen`] frees.
pub fn tok_init_gen<C: TokChar>(ifs: Option<&[C]>) -> Option<Box<TokenizerGen<C>>> {
    // The C's four allocations each abort to a NULL return, freeing whatever
    // it had already taken. Rust aborts on allocation failure rather than
    // reporting it, so `None` is unreachable here; the return type keeps the
    // C's contract because callers are specified to check it (and
    // ERR-input-13 is a caller that does not).
    Some(Box::new(TokenizerGen {
        // The C copies the caller's string with `wcsdup`, so it is not
        // retained. A NUL inside the slice would truncate that copy; it
        // cannot be observed, since a NUL element never reaches the
        // separator test, so the slice is copied whole.
        ifs: ifs.unwrap_or(C::default_ifs()).to_vec(),
        argc: 0,
        // C: `argv[0] = NULL` only, in a block of `amax` slots. The remaining
        // ones are uninitialised there and `None` here; nothing reads past
        // `argc`.
        argv: vec![None; AINCR],
        wptr: 0,
        wstart: 0,
        // C leaves the word buffer's contents uninitialised. Every element
        // is written before it is read, so zeroing is unobservable.
        wspace: vec![C::NUL; WINCR],
        quote: QuoteState::Plain,
        keep: false,
        eat: false,
    }))
}

// [spec:libedit:def:tokenizer.fun-tok-end-fn]
// [spec:libedit:sem:tokenizer.fun-tok-end-fn]
/// C: `void FUN(tok,end)(TYPE(Tokenizer) *tok)` — four `free`s, including
/// the handle itself, so this consumes the `Box` [`tok_init_gen`] handed out.
pub fn tok_end_gen<C: TokChar>(tok: Box<TokenizerGen<C>>) {
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
pub fn tok_line_gen<C: TokChar>(
    tok: &mut TokenizerGen<C>,
    line: &LineInfoGen<C>,
    argc: &mut i32,
    cursorc: Option<&mut i32>,
    cursoro: Option<&mut i32>,
) -> i32 {
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
        let c: C = if ptr >= line.lastchar {
            C::NUL
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
        match c.code() {
            C_SQUOTE => {
                tok.keep = true;
                tok.eat = false;
                match tok.quote {
                    // Enter single quote mode.
                    QuoteState::Plain => tok.quote = QuoteState::Single,
                    // Exit single quote mode.
                    QuoteState::Single => tok.quote = QuoteState::Plain,
                    // Quote this '.
                    QuoteState::EscapePlain => {
                        tok.quote = QuoteState::Plain;
                        tok.emit(c);
                    }
                    // Stay in double quote mode.
                    QuoteState::Double => tok.emit(c),
                    // Quote this ' — dropping the backslash, where sh(1)
                    // keeps it. Deliberate-looking divergence, preserved:
                    // ERR-input-41.
                    QuoteState::EscapeDouble => {
                        tok.quote = QuoteState::Double;
                        tok.emit(c);
                    }
                }
            }

            C_DQUOTE => {
                tok.eat = false;
                tok.keep = true;
                match tok.quote {
                    // Enter double quote mode.
                    QuoteState::Plain => tok.quote = QuoteState::Double,
                    // Exit double quote mode.
                    QuoteState::Double => tok.quote = QuoteState::Plain,
                    // Quote this ".
                    QuoteState::EscapePlain => {
                        tok.quote = QuoteState::Plain;
                        tok.emit(c);
                    }
                    // Stay in single quote mode.
                    QuoteState::Single => tok.emit(c),
                    // Quote this ".
                    QuoteState::EscapeDouble => {
                        tok.quote = QuoteState::Double;
                        tok.emit(c);
                    }
                }
            }

            C_BSLASH => {
                tok.keep = true;
                tok.eat = false;
                match tok.quote {
                    // Quote next character.
                    QuoteState::Plain => tok.quote = QuoteState::EscapePlain,
                    // Quote next character.
                    QuoteState::Double => tok.quote = QuoteState::EscapeDouble,
                    // Quote this, restore state.
                    QuoteState::EscapePlain => {
                        tok.emit(c);
                        tok.quote = QuoteState::Plain;
                    }
                    // Stay in single quote mode.
                    QuoteState::Single => tok.emit(c),
                    // Quote this \.
                    QuoteState::EscapeDouble => {
                        tok.quote = QuoteState::Double;
                        tok.emit(c);
                    }
                }
            }

            C_NEWLINE => {
                tok.eat = false;
                match tok.quote {
                    // The line is complete. `keep` is not set here.
                    QuoteState::Plain => break 'tokenize,
                    // Add the return: a newline inside quotes is an
                    // ordinary element and does not end the line.
                    QuoteState::Single | QuoteState::Double => tok.emit(c),
                    // Back to double, eat the '\n'.
                    QuoteState::EscapeDouble => {
                        tok.eat = true;
                        tok.quote = QuoteState::Double;
                    }
                    // No quote, more, eat the '\n'.
                    QuoteState::EscapePlain => {
                        tok.eat = true;
                        tok.quote = QuoteState::Plain;
                    }
                }
            }

            // Either an element inside the buffer — an embedded NUL
            // truncates the line — or the end-of-input NUL from (a).
            // Neither flag is touched on entry.
            C_NUL => match tok.quote {
                QuoteState::Plain => {
                    // Finish word and return.
                    if tok.eat {
                        tok.eat = false;
                        return 3;
                    }
                    break 'tokenize;
                }
                QuoteState::Single => return 1,
                QuoteState::Double => return 2,
                QuoteState::EscapeDouble => {
                    tok.quote = QuoteState::Double;
                    tok.emit(c);
                }
                QuoteState::EscapePlain => {
                    tok.quote = QuoteState::Plain;
                    tok.emit(c);
                }
            },

            _ => {
                tok.eat = false;
                match tok.quote {
                    QuoteState::Plain => {
                        // C: `Strchr(tok->ifs, *ptr) != NULL`. Element-wise,
                        // with no multibyte or locale awareness. `Strchr`
                        // would also match the terminating NUL, which cannot
                        // be reached: NUL has its own case above.
                        if tok.ifs.contains(&c) {
                            tok_finish_gen(tok);
                        } else {
                            tok.emit(c);
                        }
                    }
                    QuoteState::Single | QuoteState::Double => tok.emit(c),
                    // A backslash inside double quotes is preserved before
                    // anything other than ' " \ newline NUL. The only arm
                    // that emits two elements in one pass.
                    QuoteState::EscapeDouble => {
                        tok.emit(C::BSLASH);
                        tok.quote = QuoteState::Double;
                        tok.emit(c);
                    }
                    QuoteState::EscapePlain => {
                        tok.quote = QuoteState::Plain;
                        tok.emit(c);
                    }
                }
            }
        }

        // (d) Growth, reached only when (c) fell through. Linear, not
        // doubling. The C rebases every published `argv[i]`, `wptr` and
        // `wstart` when the block moves; here they are offsets, so there is
        // nothing to rebase. The C's two -1 returns for a failed `realloc`
        // are unreachable in Rust, which aborts on allocation failure.
        if tok.wptr >= tok.wspace.len() - 4 {
            tok.wspace.resize(tok.wspace.len() + WINCR, C::NUL);
        }
        if tok.argc >= tok.argv.len() - 4 {
            tok.argv.resize(tok.argv.len() + AINCR, None);
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
    tok_finish_gen(tok);
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
/// reason given on [`tok_line_gen`].
pub fn tok_str_gen<C: TokChar>(tok: &mut TokenizerGen<C>, line: &[C], argc: &mut i32) -> i32 {
    // C: `li.cursor = li.lastchar = Strchr(line, '\0')` — the address of the
    // terminating NUL, one past the last character. A slice with no NUL in
    // it would run that search off the end in the C, which the rule leaves
    // undefined; defined here as the end of the slice.
    let end = line.iter().position(|&x| x == C::NUL).unwrap_or(line.len());
    let buffer = line.as_ptr();
    // The C's `memset` is redundant — all three fields are assigned.
    let li = LineInfoGen {
        buffer,
        cursor: buffer.wrapping_add(end),
        lastchar: buffer.wrapping_add(end),
    };
    // Because `cursor == lastchar`, the in-loop cursor match can never fire;
    // the bookkeeping falls through to the end-of-input fallback and is then
    // discarded, both cursor out-parameters being NULL. Return value
    // verbatim, the full 0/1/2/3/-1 set — `tok_str_gen` is not restricted to
    // complete lines.
    tok_line_gen(tok, &li, argc, None, None)
}

// The one translated instantiation still consumed by `parse.rs`. The public
// narrow and wide C entry points live in the ABI adapter.

/// C: `TokenizerW *tok_winit(const wchar_t *)`.
pub fn tok_winit(ifs: Option<&[u32]>) -> Option<Box<TokenizerW>> {
    tok_init_gen::<u32>(ifs)
}

/// C: `void tok_wend(TokenizerW *)`.
pub fn tok_wend(tok: Box<TokenizerW>) {
    tok_end_gen::<u32>(tok);
}

/// C: `int tok_wstr(TokenizerW *, const wchar_t *, int *, const wchar_t ***)`.
pub fn tok_wstr(tok: &mut TokenizerW, line: &[u32], argc: &mut i32) -> i32 {
    tok_str_gen::<u32>(tok, line, argc)
}

#[cfg(test)]
mod test {
    use super::*;

    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    /// The expected side of a `parse` comparison, spelled once.
    fn vec_of(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    /// The published words, resolved the way a caller resolves `argv`: each
    /// slot is an offset into `wspace`, and the word runs to the NUL
    /// [`tok_finish_gen`] wrote after it.
    fn words(tok: &TokenizerW) -> Vec<String> {
        tok.argv[..tok.argc]
            .iter()
            .map(|slot| {
                let start = slot.expect("a slot below argc is never the terminator");
                tok.wspace[start..]
                    .iter()
                    .take_while(|&&c| c != 0)
                    .filter_map(|&c| char::from_u32(c))
                    .collect()
            })
            .collect()
    }

    /// One whole line through a fresh wide tokenizer, with the default `ifs`.
    ///
    /// `argc` starts at -1 and is returned untouched by the three early exits,
    /// which is itself part of the contract: a caller that reads `argc` after
    /// a non-zero return is reading whatever it passed in.
    fn parse(line: &str) -> (i32, i32, Vec<String>) {
        let mut tok = tok_winit(None).unwrap();
        let mut argc = -1;
        let rv = tok_wstr(&mut tok, &wide(line), &mut argc);
        let w = words(&tok);
        tok_wend(tok);
        (rv, argc, w)
    }

    /// Both quote characters suppress the separator test for everything
    /// between them, which is the whole point of the state machine: `ifs` is
    /// consulted only in `Q_none`.
    // [spec:libedit:sem:tokenizer.quote-t/test]
    // [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]
    // [spec:libedit:sem:tokenizer.fun-tok-str-fn/test]
    #[test]
    fn a_separator_inside_quotes_is_an_ordinary_character() {
        assert_eq!(parse("a 'b c' d"), (0, 3, vec_of(&["a", "b c", "d"])));
        assert_eq!(parse("a \"b c\" d"), (0, 3, vec_of(&["a", "b c", "d"])));

        // The quotes themselves are consumed, so a word can be assembled out
        // of quoted and unquoted runs with no separator between them.
        assert_eq!(parse("a'b'c"), (0, 1, vec_of(&["abc"])));

        // Each kind is inert inside the other: `Q_single` and `Q_double` emit
        // the opposite quote instead of changing state.
        assert_eq!(parse("'a\"b'"), (0, 1, vec_of(&["a\"b"])));
        assert_eq!(parse("\"a'b\""), (0, 1, vec_of(&["a'b"])));
    }

    /// `keep` — the C's `TOK_KEEP` — is what makes an empty quoted word exist
    /// at all: without it `tok_finish` publishes only when the write pointer
    /// moved, so `''` — a deliberate empty argument — would vanish instead of
    /// becoming an empty `argv` element. A run of separators, which sets no
    /// such flag, does collapse.
    // [spec:libedit:sem:tokenizer.fun-tok-finish-fn/test]
    #[test]
    fn an_empty_quoted_word_is_published_and_a_run_of_separators_is_not() {
        assert_eq!(parse("a '' b"), (0, 3, vec_of(&["a", "", "b"])));
        assert_eq!(parse("\"\""), (0, 1, vec_of(&[""])));
        assert_eq!(parse("  a  "), (0, 1, vec_of(&["a"])));
        assert_eq!(parse("   "), (0, 0, vec![]));

        // A lone backslash sets it too, so `\` at end of a word keeps that
        // word even when it contributed no character of its own.
        assert_eq!(parse("a \\"), (0, 2, vec_of(&["a", ""])));
    }

    /// ERR-input-41, reproduced: inside double quotes a backslash is kept
    /// before anything ordinary — the one dispatch arm that emits two
    /// elements in a single pass — but is DROPPED before a quote character,
    /// where sh(1) keeps it. The asymmetry is the defect.
    // [spec:libedit:sem:tokenizer.quote-t/test]
    #[test]
    fn a_backslash_inside_double_quotes_survives_only_before_an_ordinary_character() {
        assert_eq!(parse("\"\\x\""), (0, 1, vec_of(&["\\x"])));
        assert_eq!(
            parse("\"\\'\""),
            (0, 1, vec_of(&["'"])),
            "the backslash is lost"
        );
        assert_eq!(parse("\"\\\"\""), (0, 1, vec_of(&["\""])));
        assert_eq!(parse("\"\\\\\""), (0, 1, vec_of(&["\\"])));

        // Inside SINGLE quotes the backslash is never special at all: both
        // characters come through, and it does not defend the closing quote
        // either — `'a\'` is a finished word, not an escaped apostrophe.
        assert_eq!(parse("'\\x'"), (0, 1, vec_of(&["\\x"])));
        assert_eq!(parse("'a\\'"), (0, 1, vec_of(&["a\\"])));
    }

    /// A backslash-newline at end of input is the "quoted return" that tells
    /// the caller to read another line: the return is 3, `argc` is left
    /// untouched, and the tokenizer's state is deliberately NOT reset — so
    /// the next call continues the very word that was interrupted.
    // [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]
    // [spec:libedit:sem:tokenizer.fun-tok-str-fn/test]
    #[test]
    fn a_backslash_newline_asks_for_another_line_and_resumes_the_same_word() {
        let mut tok = tok_winit(None).unwrap();
        let mut argc = -1;
        assert_eq!(tok_wstr(&mut tok, &wide("echo a\\\n"), &mut argc), 3);
        assert_eq!(argc, -1, "an early return never writes argc");

        assert_eq!(tok_wstr(&mut tok, &wide("b\n"), &mut argc), 0);
        assert_eq!(argc, 2);
        assert_eq!(words(&tok), ["echo", "ab"]);
        tok_wend(tok);

        // Inside quotes a newline is not a continuation, it is a character:
        // the word carries it and the caller is told the quote is still open.
        assert_eq!(parse("'a\nb"), (1, -1, vec![]));
        assert_eq!(parse("\"a\nb"), (2, -1, vec![]));
        assert_eq!(parse("'a\nb'"), (0, 1, vec_of(&["a\nb"])));
    }

    /// ERR-input-39, reproduced. The newline is a member of the DEFAULT `ifs`
    /// and yet it never splits a word, because the five dispatched elements
    /// are matched before the separator test — in `Q_none` it ends the line
    /// instead, and everything after it is silently dropped.
    // [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]
    #[test]
    fn a_newline_ends_the_line_rather_than_separating_words() {
        assert_eq!(parse("a\nb"), (0, 1, vec_of(&["a"])));
        assert_eq!(parse("\n"), (0, 0, vec![]));
        // The line ends whether or not a word was pending.
        assert_eq!(parse("ab\ncd ef"), (0, 1, vec_of(&["ab"])));
    }

    /// Unterminated quotes are reported rather than closed: 1 for a single
    /// quote and 2 for a double, and in both cases `argc` and every partial
    /// word are left where they are for a following call to continue.
    // [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]
    #[test]
    fn an_unterminated_quote_is_reported_by_its_own_return_code() {
        assert_eq!(parse("'a"), (1, -1, vec![]));
        assert_eq!(parse("\"a"), (2, -1, vec![]));

        // An embedded NUL is the same end of input as running out of buffer,
        // so it truncates the line rather than being tokenized.
        let mut tok = tok_winit(None).unwrap();
        let mut argc = -1;
        assert_eq!(tok_wstr(&mut tok, &[0x61, 0, 0x62], &mut argc), 0);
        assert_eq!(argc, 1);
        assert_eq!(words(&tok), ["a"]);
        tok_wend(tok);
    }

    /// The separator set is the caller's, and it is the ONLY thing `ifs`
    /// controls: the five dispatched elements are matched first, so naming
    /// one of them a separator has no effect whatever.
    // [spec:libedit:sem:tokenizer.fun-tok-init-fn/test]
    #[test]
    fn the_separator_set_is_replaceable_but_cannot_reach_the_dispatch() {
        let tok = tok_winit(None).unwrap();
        assert_eq!(tok.ifs, wide("\t \n"));
        // The two buffers are the C's `amax` and `wmax` blocks; their lengths
        // are what those two members held.
        assert_eq!(tok.argv.len(), AINCR);
        assert_eq!(tok.wspace.len(), WINCR);
        tok_wend(tok);

        let ifs = wide(",");
        let mut tok = tok_winit(Some(&ifs)).unwrap();
        let mut argc = -1;
        assert_eq!(tok_wstr(&mut tok, &wide("a,b c"), &mut argc), 0);
        assert_eq!(words(&tok), ["a", "b c"]);
        tok_wend(tok);

        // A quote named as a separator still quotes, and the line still ends
        // unterminated.
        let ifs = wide("'");
        let mut tok = tok_winit(Some(&ifs)).unwrap();
        let mut argc = -1;
        assert_eq!(tok_wstr(&mut tok, &wide("a'b"), &mut argc), 1);
        tok_wend(tok);
    }

    /// Both buffers grow, and the growth is what makes the unchecked `emit`
    /// safe: it runs once per pass with four elements of slack, which covers
    /// the two the `Q_doubleone` arm can emit together plus `tok_finish`'s
    /// terminator.
    // [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]
    #[test]
    fn the_word_and_argv_buffers_grow_past_their_initial_sizes() {
        let long = "x".repeat(60);
        assert_eq!(parse(&long), (0, 1, vec![long]));

        let many = "a ".repeat(30);
        let (rv, argc, w) = parse(&many);
        assert_eq!((rv, argc), (0, 30));
        assert_eq!(w, vec!["a"; 30]);

        // The two-element arm, thirty times over, so the slack is exercised
        // rather than merely reserved.
        let pairs = format!("\"{}\"", "\\z".repeat(30));
        assert_eq!(parse(&pairs), (0, 1, vec!["\\z".repeat(30)]));
    }
}
