//! Owned tokenization over logical editor text.

use crate::domain::{Error, Text, TextIndex, TextSpan, TextUnit};

/// The quote syntax active at a token cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QuoteStyle {
    /// The cursor is outside quotes.
    #[default]
    Unquoted,
    /// The cursor is inside a single-quoted section.
    Single,
    /// The cursor is inside a double-quoted section.
    Double,
}

/// Why a tokenized line needs more input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Continuation {
    /// A single quote has not been closed.
    SingleQuote,
    /// A double quote has not been closed.
    DoubleQuote,
    /// A trailing escape needs one more logical unit.
    Escape(QuoteStyle),
}

/// One cooked token and the source syntax that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Token {
    value: Text,
    source: TextSpan,
}

impl Token {
    /// The owned token after quote and escape syntax has been removed.
    #[must_use]
    pub fn value(&self) -> &Text {
        &self.value
    }

    /// The checked source range, including any quote and escape syntax.
    #[must_use]
    pub const fn source(&self) -> TextSpan {
        self.source
    }
}

/// A checked position in an owned token vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenIndex(usize);

impl TokenIndex {
    /// The zero-based token position. It may equal the token count when the
    /// cursor is between tokens.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// A checked logical-unit offset within a cooked token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenOffset(usize);

impl TokenOffset {
    /// The zero-based logical-unit offset.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Where the source cursor landed in cooked token space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenCursor {
    token: TokenIndex,
    offset: TokenOffset,
    quote: QuoteStyle,
}

impl TokenCursor {
    /// The token at the cursor, or the insertion position between tokens.
    #[must_use]
    pub const fn token(self) -> TokenIndex {
        self.token
    }

    /// The cooked offset within that token.
    #[must_use]
    pub const fn offset(self) -> TokenOffset {
        self.offset
    }

    /// Quote syntax active immediately before the source cursor.
    #[must_use]
    pub const fn quote(self) -> QuoteStyle {
        self.quote
    }
}

/// Owned tokens and the translated cursor for one parse.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TokenizedLine {
    tokens: Vec<Token>,
    cursor: TokenCursor,
}

impl TokenizedLine {
    /// Borrow the cooked tokens in source order.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// The source cursor translated into cooked token space.
    #[must_use]
    pub const fn cursor(&self) -> TokenCursor {
        self.cursor
    }
}

/// The typed result of parsing one logical line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Tokenization {
    /// An unquoted newline or the end of input completed the line.
    Complete(TokenizedLine),
    /// The owned partial result needs more input for this syntax.
    Incomplete {
        line: TokenizedLine,
        continuation: Continuation,
    },
}

impl Tokenization {
    /// Borrow the owned tokens and cursor whether or not input is complete.
    #[must_use]
    pub const fn line(&self) -> &TokenizedLine {
        match self {
            Self::Complete(line) | Self::Incomplete { line, .. } => line,
        }
    }

    /// The continuation reason, if the source ended inside syntax.
    #[must_use]
    pub const fn continuation(&self) -> Option<Continuation> {
        match self {
            Self::Complete(_) => None,
            Self::Incomplete { continuation, .. } => Some(*continuation),
        }
    }
}

/// A reusable tokenizer with an owned scalar separator policy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tokenizer {
    separators: Text,
}

impl Default for Tokenizer {
    fn default() -> Self {
        Self {
            separators: Text::from(" \t"),
        }
    }
}

impl Tokenizer {
    /// Own a separator set, removing duplicates and values reserved for quote,
    /// escape, or newline syntax. Non-scalar compatibility values remain data.
    #[must_use]
    pub fn new(separators: impl Into<Text>) -> Self {
        let mut normalized = Text::default();
        for unit in separators.into() {
            let TextUnit::Scalar(character) = unit else {
                continue;
            };
            if matches!(character, '\'' | '"' | '\\' | '\n')
                || normalized.as_units().contains(&unit)
            {
                continue;
            }
            normalized.push(unit);
        }
        Self {
            separators: normalized,
        }
    }

    /// Borrow the normalized separator policy.
    #[must_use]
    pub fn separators(&self) -> &Text {
        &self.separators
    }

    // [spec:nshedit:req:core.token-completion+1]
    /// Parse logical text without retaining a borrow or scratch-buffer alias.
    pub fn tokenize(&self, input: &Text, cursor: TextIndex) -> Result<Tokenization, Error> {
        input.index(cursor.get())?;
        let mut parser = Parser::default();
        let mut ended = false;

        for (source_index, &unit) in input.as_units().iter().enumerate() {
            parser.capture_cursor(source_index, cursor);
            if parser.consume(self, input, source_index, unit)? {
                ended = true;
                break;
            }
        }

        if parser.cursor.is_none() {
            parser.capture();
        }
        if !ended {
            parser.finish(input, input.len())?;
        }

        let continuation = if ended {
            None
        } else {
            parser.state.continuation()
        };
        let line = TokenizedLine {
            tokens: parser.tokens,
            cursor: parser.cursor.expect("the cursor is captured before return"),
        };
        Ok(match continuation {
            Some(continuation) => Tokenization::Incomplete { line, continuation },
            None => Tokenization::Complete(line),
        })
    }

    fn is_separator(&self, unit: TextUnit) -> bool {
        matches!(unit, TextUnit::Scalar(_)) && self.separators.as_units().contains(&unit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum State {
    #[default]
    Plain,
    Single,
    Double,
    EscapePlain,
    EscapeDouble,
}

impl State {
    const fn quote(self) -> QuoteStyle {
        match self {
            Self::Plain | Self::EscapePlain => QuoteStyle::Unquoted,
            Self::Single => QuoteStyle::Single,
            Self::Double | Self::EscapeDouble => QuoteStyle::Double,
        }
    }

    const fn continuation(self) -> Option<Continuation> {
        match self {
            Self::Plain => None,
            Self::Single => Some(Continuation::SingleQuote),
            Self::Double => Some(Continuation::DoubleQuote),
            Self::EscapePlain => Some(Continuation::Escape(QuoteStyle::Unquoted)),
            Self::EscapeDouble => Some(Continuation::Escape(QuoteStyle::Double)),
        }
    }
}

#[derive(Debug, Default)]
struct Parser {
    state: State,
    tokens: Vec<Token>,
    word: Text,
    word_start: Option<usize>,
    word_present: bool,
    cursor: Option<TokenCursor>,
}

impl Parser {
    fn capture_cursor(&mut self, source_index: usize, cursor: TextIndex) {
        if source_index == cursor.get() {
            self.capture();
        }
    }

    fn capture(&mut self) {
        if self.cursor.is_none() {
            self.cursor = Some(TokenCursor {
                token: TokenIndex(self.tokens.len()),
                offset: TokenOffset(self.word.len()),
                quote: self.state.quote(),
            });
        }
    }

    fn touch(&mut self, source_index: usize) {
        self.word_present = true;
        self.word_start.get_or_insert(source_index);
    }

    fn emit(&mut self, source_index: usize, unit: TextUnit) {
        self.touch(source_index);
        self.word.push(unit);
    }

    fn finish(&mut self, input: &Text, source_end: usize) -> Result<(), Error> {
        if !self.word_present {
            return Ok(());
        }
        let start = self.word_start.unwrap_or(source_end);
        self.tokens.push(Token {
            value: std::mem::take(&mut self.word),
            source: input.span(start..source_end)?,
        });
        self.word_start = None;
        self.word_present = false;
        Ok(())
    }

    fn consume(
        &mut self,
        tokenizer: &Tokenizer,
        input: &Text,
        source_index: usize,
        unit: TextUnit,
    ) -> Result<bool, Error> {
        if self.state == State::EscapePlain {
            self.state = State::Plain;
            if unit != TextUnit::Scalar('\n') {
                self.emit(source_index, unit);
            }
            return Ok(false);
        }
        if self.state == State::EscapeDouble {
            self.state = State::Double;
            match unit {
                TextUnit::Scalar('\'' | '"' | '\\') => self.emit(source_index, unit),
                TextUnit::Scalar('\n') => {}
                _ => {
                    self.emit(source_index, TextUnit::Scalar('\\'));
                    self.emit(source_index, unit);
                }
            }
            return Ok(false);
        }

        match unit {
            TextUnit::Scalar('\'') if self.state == State::Plain => {
                self.touch(source_index);
                self.state = State::Single;
            }
            TextUnit::Scalar('\'') if self.state == State::Single => {
                self.touch(source_index);
                self.state = State::Plain;
            }
            TextUnit::Scalar('"') if self.state == State::Plain => {
                self.touch(source_index);
                self.state = State::Double;
            }
            TextUnit::Scalar('"') if self.state == State::Double => {
                self.touch(source_index);
                self.state = State::Plain;
            }
            TextUnit::Scalar('\\') if self.state == State::Plain => {
                self.touch(source_index);
                self.state = State::EscapePlain;
            }
            TextUnit::Scalar('\\') if self.state == State::Double => {
                self.touch(source_index);
                self.state = State::EscapeDouble;
            }
            TextUnit::Scalar('\n') if self.state == State::Plain => {
                self.finish(input, source_index)?;
                return Ok(true);
            }
            _ if self.state == State::Plain && tokenizer.is_separator(unit) => {
                self.finish(input, source_index)?;
            }
            _ => self.emit(source_index, unit),
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NonScalarWide;

    // [spec:nshedit:req:core.token-completion+1/test]
    #[test]
    fn tokens_own_text_and_source_spans() {
        let input = Text::from("one 'two three' four");
        let result = Tokenizer::default()
            .tokenize(&input, input.index(input.len()).unwrap())
            .unwrap();
        let line = result.line();

        let values: Vec<_> = line.tokens().iter().map(Token::value).collect();
        assert_eq!(
            values,
            [
                &Text::from("one"),
                &Text::from("two three"),
                &Text::from("four")
            ]
        );
        assert_eq!(line.tokens()[1].source().start().get(), 4);
        assert_eq!(line.tokens()[1].source().end().get(), 15);
        assert_eq!(line.cursor().token().get(), 2);
        assert_eq!(line.cursor().offset().get(), 4);
        assert_eq!(result.continuation(), None);
    }

    #[test]
    fn empty_quotes_keep_compatibility_data() {
        let input: Text = [
            TextUnit::Scalar('\''),
            TextUnit::Scalar('\''),
            TextUnit::RawByte(b' '),
            TextUnit::CompatibilityWide(NonScalarWide::new(0xd800).unwrap()),
        ]
        .into_iter()
        .collect();
        let result = Tokenizer::default()
            .tokenize(&input, input.index(input.len()).unwrap())
            .unwrap();

        assert_eq!(result.line().tokens().len(), 1);
        assert_eq!(
            result.line().tokens()[0].value().as_units(),
            &input.as_units()[2..]
        );
    }

    #[test]
    fn incomplete_syntax_has_semantic_reasons() {
        let cases = [
            ("'word", Continuation::SingleQuote),
            ("\"word", Continuation::DoubleQuote),
            ("word\\", Continuation::Escape(QuoteStyle::Unquoted)),
            ("\"word\\", Continuation::Escape(QuoteStyle::Double)),
        ];
        for (source, expected) in cases {
            let input = Text::from(source);
            let result = Tokenizer::default()
                .tokenize(&input, input.index(input.len()).unwrap())
                .unwrap();
            assert_eq!(result.continuation(), Some(expected));
        }
    }

    #[test]
    fn cursor_maps_into_cooked_token() {
        let input = Text::from("'ab cd' tail");
        let result = Tokenizer::default()
            .tokenize(&input, input.index(4).unwrap())
            .unwrap();
        let cursor = result.line().cursor();

        assert_eq!(cursor.token().get(), 0);
        assert_eq!(cursor.offset().get(), 3);
        assert_eq!(cursor.quote(), QuoteStyle::Single);
    }

    #[test]
    fn escape_newline_joins_physical_lines() {
        let input = Text::from("one\\\ntwo");
        let result = Tokenizer::default()
            .tokenize(&input, input.index(input.len()).unwrap())
            .unwrap();

        assert_eq!(result.continuation(), None);
        assert_eq!(result.line().tokens()[0].value(), &Text::from("onetwo"));
    }

    #[test]
    fn separators_are_owned_and_normalized() {
        let mut separators = Text::from("::,");
        separators.push(TextUnit::RawByte(b';'));
        let tokenizer = Tokenizer::new(separators);
        let input = Text::from("a:b,c");
        let result = tokenizer
            .tokenize(&input, input.index(input.len()).unwrap())
            .unwrap();

        assert_eq!(tokenizer.separators(), &Text::from(":,"));
        let values: Vec<_> = result.line().tokens().iter().map(Token::value).collect();
        assert_eq!(
            values,
            [&Text::from("a"), &Text::from("b"), &Text::from("c")]
        );
    }
}
