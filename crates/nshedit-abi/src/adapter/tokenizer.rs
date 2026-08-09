//! C tokenizer storage around the native owned tokenizer.

use super::*;

const NARROW_DEFAULT_SEPARATORS: [c_char; 3] = [b'\t' as c_char, b' ' as c_char, b'\n' as c_char];
const WIDE_DEFAULT_SEPARATORS: [u32; 3] = [0x09, 0x20, 0x0a];

/// Conversion between one C tokenizer element and one native logical unit.
///
/// This is deliberately private: the native core has one [`TextUnit`] model;
/// signed bytes and non-scalar `wchar_t` values are boundary encodings, not a
/// generic character abstraction.
pub(crate) trait BoundaryChar: Copy + PartialEq {
    const NUL: Self;

    fn into_unit(self) -> TextUnit;
    fn from_unit(unit: TextUnit) -> Self;
}

impl BoundaryChar for c_char {
    const NUL: Self = 0;

    fn into_unit(self) -> TextUnit {
        let byte = self as u8;
        if byte.is_ascii() {
            TextUnit::Scalar(char::from(byte))
        } else {
            TextUnit::RawByte(byte)
        }
    }

    fn from_unit(unit: TextUnit) -> Self {
        match unit {
            TextUnit::Scalar(character) => u32::from(character) as u8 as c_char,
            TextUnit::RawByte(byte) => byte as c_char,
            TextUnit::OpaqueCodePoint(value) => value.get() as u8 as c_char,
        }
    }
}

impl BoundaryChar for u32 {
    const NUL: Self = 0;

    fn into_unit(self) -> TextUnit {
        TextUnit::from_code_point(self)
    }

    fn from_unit(unit: TextUnit) -> Self {
        match unit {
            TextUnit::Scalar(character) => u32::from(character),
            TextUnit::RawByte(byte) => u32::from(byte),
            TextUnit::OpaqueCodePoint(value) => value.get(),
        }
    }
}

// [spec:libedit:def:tokenizer.quote-t]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum BoundaryState {
    #[default]
    Plain,
    Single,
    Double,
    EscapePlain,
    EscapeDouble,
}

struct AdaptedInput {
    text: Text,
    cursor: usize,
    cursor_was_captured: bool,
    ordinary_newline: Option<usize>,
    ends_with_escaped_newline: bool,
}

/// A C continuation code expressed without an integer protocol inside the
/// owner. The exported functions perform the final ABI mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundaryContinuation {
    SingleQuote,
    DoubleQuote,
    EscapedNewline,
}

/// Stable C storage published by one successful tokenizer call.
pub(crate) struct PublishedTokens<C> {
    pub(crate) count: c_int,
    pub(crate) words: *mut *const C,
    pub(crate) cursor_word: c_int,
    pub(crate) cursor_offset: c_int,
}

pub(crate) enum TokenizeOutcome<C> {
    Published(PublishedTokens<C>),
    Incomplete(BoundaryContinuation),
    Failed,
}

// [spec:nshedit:req:abi.opaque-owner]
/// Allocation behind either incomplete C tokenizer handle.
///
/// Parsing belongs exclusively to the native [`NativeTokenizer`]. This owner
/// retains only boundary state: physical-line accumulation, non-scalar C
/// separators, NUL-terminated word storage, and the lent `argv` array.
pub struct TokenizerHandle<C> {
    native: NativeTokenizer,
    extra_separators: Vec<TextUnit>,
    escape_plain_space: bool,
    pending: Text,
    reported_escaped_newline_at: Option<usize>,
    published: Vec<Vec<C>>,
    active_count: usize,
    argv: Vec<*const C>,
}

pub type Tokenizer = TokenizerHandle<c_char>;
pub type TokenizerW = TokenizerHandle<u32>;

impl TokenizerHandle<c_char> {
    pub(crate) fn from_narrow(separators: Option<&[c_char]>) -> Box<Self> {
        Self::new(separators.unwrap_or(&NARROW_DEFAULT_SEPARATORS))
    }
}

impl TokenizerHandle<u32> {
    pub(crate) fn from_wide(separators: Option<&[u32]>) -> Box<Self> {
        Self::new(separators.unwrap_or(&WIDE_DEFAULT_SEPARATORS))
    }
}

impl<C> TokenizerHandle<C> {
    // [spec:libedit:def:tokenizer.fun-tok-init-fn]
    // [spec:libedit:sem:tokenizer.fun-tok-init-fn]
    fn new(separators: &[C]) -> Box<Self>
    where
        C: BoundaryChar,
    {
        let requested: Text = separators
            .iter()
            .copied()
            .map(BoundaryChar::into_unit)
            .collect();
        let extra_separators: Vec<_> = requested
            .as_units()
            .iter()
            .copied()
            .filter(|unit| !matches!(unit, TextUnit::Scalar(_)))
            .collect();
        let mut native = NativeTokenizer::new(requested);
        let escape_plain_space = !extra_separators.is_empty()
            && !native
                .separators()
                .as_units()
                .contains(&TextUnit::Scalar(' '));
        if !extra_separators.is_empty() {
            let mut separators = native.separators().clone();
            separators.push(TextUnit::Scalar(' '));
            native = NativeTokenizer::new(separators);
        }

        Box::new(Self {
            native,
            extra_separators,
            escape_plain_space,
            pending: Text::default(),
            reported_escaped_newline_at: None,
            published: Vec::new(),
            active_count: 0,
            argv: vec![core::ptr::null()],
        })
    }

    // [spec:libedit:def:tokenizer.fun-tok-reset-fn]
    // [spec:libedit:sem:tokenizer.fun-tok-reset-fn]
    pub(crate) fn reset(&mut self) {
        self.pending.clear();
        self.reported_escaped_newline_at = None;
        self.active_count = 0;
        // Deliberately retain both allocations and `argv[0]`. The C reset
        // forgets its count without restoring the NULL terminator; callers
        // can observe that historical defect after a following empty parse.
    }

    // [spec:libedit:def:tokenizer.fun-tok-line-fn]
    // [spec:libedit:sem:tokenizer.fun-tok-line-fn]
    // [spec:libedit:def:tokenizer.fun-tok-str-fn]
    // [spec:libedit:sem:tokenizer.fun-tok-str-fn]
    pub(crate) fn tokenize(&mut self, input: &[C], cursor: Option<usize>) -> TokenizeOutcome<C>
    where
        C: BoundaryChar,
    {
        let input_end = input
            .iter()
            .position(|unit| *unit == C::NUL)
            .unwrap_or(input.len());
        let pending_start = self.pending.len();
        let cursor = cursor
            .filter(|index| *index < input.len() && *index <= input_end)
            .map(|index| pending_start + index);
        self.pending.extend(
            input[..input_end]
                .iter()
                .copied()
                .map(BoundaryChar::into_unit),
        );

        let adapted = self.adapt_pending(cursor);
        let Ok(cursor) = adapted.text.index(adapted.cursor) else {
            return TokenizeOutcome::Failed;
        };
        let Ok(mut tokenization) = self.native.tokenize(&adapted.text, cursor) else {
            return TokenizeOutcome::Failed;
        };

        match tokenization.continuation() {
            Some(Continuation::SingleQuote) => {
                return TokenizeOutcome::Incomplete(BoundaryContinuation::SingleQuote);
            }
            Some(Continuation::DoubleQuote) => {
                return TokenizeOutcome::Incomplete(BoundaryContinuation::DoubleQuote);
            }
            Some(Continuation::Escape(QuoteStyle::Double)) => {
                let mut pending: Vec<_> = core::mem::take(&mut self.pending).into_iter().collect();
                if let Some(last) = pending.last_mut() {
                    *last = TextUnit::Scalar('\0');
                }
                self.pending = pending.into_iter().collect();
                return TokenizeOutcome::Incomplete(BoundaryContinuation::DoubleQuote);
            }
            Some(Continuation::Escape(QuoteStyle::Unquoted)) => {
                let mut completed = adapted.text.clone();
                completed.push(TextUnit::Scalar('\0'));
                let cursor = if adapted.cursor_was_captured {
                    adapted.cursor
                } else {
                    completed.len()
                };
                let Ok(cursor) = completed.index(cursor) else {
                    return TokenizeOutcome::Failed;
                };
                let Ok(completed) = self.native.tokenize(&completed, cursor) else {
                    return TokenizeOutcome::Failed;
                };
                tokenization = completed;
            }
            Some(Continuation::Escape(QuoteStyle::Single)) => {
                return TokenizeOutcome::Failed;
            }
            None => {}
        }

        if adapted.ends_with_escaped_newline
            && self.reported_escaped_newline_at != Some(self.pending.len())
        {
            self.reported_escaped_newline_at = Some(self.pending.len());
            return TokenizeOutcome::Incomplete(BoundaryContinuation::EscapedNewline);
        }

        let Tokenization::Complete(line) = tokenization else {
            return TokenizeOutcome::Failed;
        };
        let mut cursor_word = line.cursor().token().get();
        let mut cursor_offset = line.cursor().offset().get();
        if let Some(newline) = adapted.ordinary_newline
            && !adapted.cursor_was_captured
        {
            let pending_word = line
                .tokens()
                .last()
                .filter(|token| token.source().end().get() == newline);
            match pending_word {
                Some(token) => {
                    cursor_word = line.tokens().len().saturating_sub(1);
                    cursor_offset = token.value().len();
                }
                None => {
                    cursor_word = line.tokens().len();
                    cursor_offset = 0;
                }
            }
        }

        self.publish(
            line.tokens().iter().map(|token| token.value()),
            cursor_word,
            cursor_offset,
        )
    }

    fn adapt_pending(&self, cursor: Option<usize>) -> AdaptedInput {
        let mut text = Text::default();
        let mut state = BoundaryState::Plain;
        let mut adapted_cursor = None;
        let mut ordinary_newline = None;
        let mut ends_with_escaped_newline = false;

        for (source_index, unit) in self.pending.as_units().iter().copied().enumerate() {
            if cursor == Some(source_index) {
                adapted_cursor = Some(text.len());
            }
            ends_with_escaped_newline = false;

            match state {
                BoundaryState::EscapePlain => {
                    state = BoundaryState::Plain;
                    ends_with_escaped_newline = unit == TextUnit::Scalar('\n');
                    text.push(unit);
                }
                BoundaryState::EscapeDouble => {
                    state = BoundaryState::Double;
                    text.push(unit);
                }
                BoundaryState::Single => {
                    if unit == TextUnit::Scalar('\'') {
                        state = BoundaryState::Plain;
                    }
                    text.push(unit);
                }
                BoundaryState::Double => {
                    state = match unit {
                        TextUnit::Scalar('"') => BoundaryState::Plain,
                        TextUnit::Scalar('\\') => BoundaryState::EscapeDouble,
                        _ => BoundaryState::Double,
                    };
                    text.push(unit);
                }
                BoundaryState::Plain => match unit {
                    TextUnit::Scalar('\'') => {
                        state = BoundaryState::Single;
                        text.push(unit);
                    }
                    TextUnit::Scalar('"') => {
                        state = BoundaryState::Double;
                        text.push(unit);
                    }
                    TextUnit::Scalar('\\') => {
                        state = BoundaryState::EscapePlain;
                        text.push(unit);
                    }
                    TextUnit::Scalar('\n') => {
                        ordinary_newline = Some(text.len());
                        text.push(unit);
                        break;
                    }
                    _ if self.extra_separators.contains(&unit) => {
                        text.push(TextUnit::Scalar(' '));
                    }
                    TextUnit::Scalar(' ') if self.escape_plain_space => {
                        text.push(TextUnit::Scalar('\\'));
                        text.push(unit);
                    }
                    _ => text.push(unit),
                },
            }
        }

        if ordinary_newline.is_none() && cursor == Some(self.pending.len()) {
            adapted_cursor = Some(text.len());
        }
        let cursor_was_captured = adapted_cursor.is_some();
        AdaptedInput {
            cursor: adapted_cursor.unwrap_or_else(|| text.len()),
            text,
            cursor_was_captured,
            ordinary_newline,
            ends_with_escaped_newline,
        }
    }

    // [spec:libedit:def:tokenizer.fun-tok-finish-fn]
    // [spec:libedit:sem:tokenizer.fun-tok-finish-fn]
    fn publish<'a>(
        &mut self,
        tokens: impl IntoIterator<Item = &'a Text>,
        cursor_word: usize,
        cursor_offset: usize,
    ) -> TokenizeOutcome<C>
    where
        C: BoundaryChar,
    {
        let base = self.active_count;
        let encoded: Vec<Vec<C>> = tokens
            .into_iter()
            .map(|token| {
                token
                    .as_units()
                    .iter()
                    .copied()
                    .map(BoundaryChar::from_unit)
                    .chain(core::iter::once(C::NUL))
                    .collect()
            })
            .collect();

        if base == 0 && !encoded.is_empty() {
            self.published.clear();
        }
        self.active_count = base + encoded.len();
        self.published.extend(encoded);

        if self.active_count == 0 {
            if let Some(first) = self.published.first_mut().and_then(|word| word.first_mut()) {
                *first = C::NUL;
            }
        } else {
            self.argv = self
                .published
                .iter()
                .take(self.active_count)
                .map(|word| word.as_ptr())
                .chain(core::iter::once(core::ptr::null()))
                .collect();
        }

        self.pending.clear();
        self.reported_escaped_newline_at = None;
        TokenizeOutcome::Published(PublishedTokens {
            count: self.active_count as c_int,
            words: self.argv.as_mut_ptr(),
            cursor_word: (base + cursor_word) as c_int,
            cursor_offset: cursor_offset as c_int,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundaryContinuation, PublishedTokens, TokenizeOutcome, TokenizerW};

    fn wide(source: &str) -> Vec<u32> {
        source.chars().map(u32::from).collect()
    }

    fn published(outcome: TokenizeOutcome<u32>) -> PublishedTokens<u32> {
        match outcome {
            TokenizeOutcome::Published(published) => published,
            TokenizeOutcome::Incomplete(_) | TokenizeOutcome::Failed => {
                panic!("expected a completed tokenization")
            }
        }
    }

    fn wide_words(published: &PublishedTokens<u32>) -> Vec<Vec<u32>> {
        (0..published.count as usize)
            .map(|index| {
                // SAFETY: the owner published `count` live word pointers.
                let word = unsafe { *published.words.add(index) };
                let mut length = 0;
                // SAFETY: every published word has an owner-appended NUL.
                while unsafe { *word.add(length) } != 0 {
                    length += 1;
                }
                // SAFETY: the loop established this live content range.
                unsafe { core::slice::from_raw_parts(word, length) }.to_vec()
            })
            .collect()
    }

    #[test]
    fn continues_physical_lines() {
        let mut tokenizer = TokenizerW::from_wide(None);
        assert!(matches!(
            tokenizer.tokenize(&wide("echo a\\\n"), None),
            TokenizeOutcome::Incomplete(BoundaryContinuation::EscapedNewline)
        ));

        let result = published(tokenizer.tokenize(&wide("b\n"), None));
        assert_eq!(wide_words(&result), [wide("echo"), wide("ab")]);
    }

    #[test]
    fn trailing_escape_counts_nul() {
        let mut tokenizer = TokenizerW::from_wide(None);
        let result = published(tokenizer.tokenize(&wide("a\\"), None));

        assert_eq!(wide_words(&result), [wide("a")]);
        assert_eq!((result.cursor_word, result.cursor_offset), (0, 2));
    }

    #[test]
    fn non_scalar_separator_preserves_space() {
        let mut tokenizer = TokenizerW::from_wide(Some(&[0xd800]));
        let input = [
            u32::from(b'a'),
            0xd800,
            u32::from(b'b'),
            0x20,
            u32::from(b'c'),
        ];
        let result = published(tokenizer.tokenize(&input, None));

        assert_eq!(wide_words(&result), [wide("a"), wide("b c")]);
    }

    // [spec:libedit:sem:tokenizer.fun-tok-reset-fn/test]
    #[test]
    fn reset_retains_stale_argv() {
        let mut tokenizer = TokenizerW::from_wide(None);
        let first = published(tokenizer.tokenize(&wide("a b"), None));
        assert_eq!(first.count, 2);

        tokenizer.reset();
        let empty = published(tokenizer.tokenize(&[], None));
        assert_eq!(empty.count, 0);
        // SAFETY: even at count zero the returned array has its first slot.
        let stale = unsafe { *empty.words };
        assert!(!stale.is_null());
        // The empty parse writes a NUL at the reset word-buffer origin while
        // deliberately failing to restore the stale pointer to NULL.
        assert_eq!(unsafe { *stale }, 0);
    }
}
