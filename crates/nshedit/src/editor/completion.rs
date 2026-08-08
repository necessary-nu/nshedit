//! Typed completion queries, candidate sets, and atomic line edits.

use std::collections::BTreeSet;

use crate::domain::{Error, Text, TextIndex, TextSpan, TextUnit};

use super::token::{QuoteStyle, Tokenizer};
use super::{Editor, TerminalControl, line};

/// An owned completion request bound to one exact editor snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionQuery {
    line: Text,
    cursor: TextIndex,
    replacement: TextSpan,
    stem: Text,
    quote: QuoteStyle,
    separators: Text,
}

impl CompletionQuery {
    /// The complete line observed when completion was requested.
    #[must_use]
    pub fn line(&self) -> &Text {
        &self.line
    }

    /// The checked cursor observed when completion was requested.
    #[must_use]
    pub const fn cursor(&self) -> TextIndex {
        self.cursor
    }

    /// The exact source syntax to replace when candidates return.
    #[must_use]
    pub const fn replacement(&self) -> TextSpan {
        self.replacement
    }

    /// The cooked logical prefix a provider should complete.
    #[must_use]
    pub fn stem(&self) -> &Text {
        &self.stem
    }

    /// The quote syntax active at the request cursor.
    #[must_use]
    pub const fn quote(&self) -> QuoteStyle {
        self.quote
    }
}

/// One provider-owned completion candidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionCandidate {
    insertion: Text,
    display: Option<Text>,
    suffix: Option<Text>,
}

impl CompletionCandidate {
    /// Build a candidate whose display spelling is its insertion text.
    #[must_use]
    pub fn new(insertion: impl Into<Text>) -> Self {
        Self {
            insertion: insertion.into(),
            display: None,
            suffix: None,
        }
    }

    /// Use a separate spelling when candidates are presented to the user.
    #[must_use]
    pub fn with_display(mut self, display: impl Into<Text>) -> Self {
        let display = display.into();
        self.display = (display != self.insertion).then_some(display);
        self
    }

    /// Append logical line text after a unique completed value.
    #[must_use]
    pub fn with_suffix(mut self, suffix: impl Into<Text>) -> Self {
        let suffix = suffix.into();
        self.suffix = (!suffix.is_empty()).then_some(suffix);
        self
    }

    /// The unquoted logical value used for prefix matching and insertion.
    #[must_use]
    pub fn insertion(&self) -> &Text {
        &self.insertion
    }

    /// The spelling used in a candidate list.
    #[must_use]
    pub fn display(&self) -> &Text {
        self.display.as_ref().unwrap_or(&self.insertion)
    }

    /// Verbatim logical line text appended after a unique match.
    #[must_use]
    pub fn suffix(&self) -> Option<&Text> {
        self.suffix.as_ref()
    }
}

/// An owned response from a completion provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CompletionCandidates(Vec<CompletionCandidate>);

impl CompletionCandidates {
    /// Number of candidates supplied by the provider.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the provider supplied no candidates.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrow candidates in provider order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &CompletionCandidate> {
        self.0.iter()
    }

    fn matching(self, stem: &Text) -> Vec<CompletionCandidate> {
        let mut seen = BTreeSet::new();
        self.0
            .into_iter()
            .filter(|candidate| candidate.insertion.as_units().starts_with(stem.as_units()))
            .filter(|candidate| seen.insert(candidate.insertion.clone()))
            .collect()
    }
}

impl FromIterator<CompletionCandidate> for CompletionCandidates {
    fn from_iter<T: IntoIterator<Item = CompletionCandidate>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl From<Vec<CompletionCandidate>> for CompletionCandidates {
    fn from(candidates: Vec<CompletionCandidate>) -> Self {
        Self(candidates)
    }
}

/// The exact atomic line replacement performed by completion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionEdit {
    span: TextSpan,
    replacement: Text,
}

impl CompletionEdit {
    /// Source range replaced in the queried line.
    #[must_use]
    pub const fn span(&self) -> TextSpan {
        self.span
    }

    /// Quote-safe logical line text inserted in its place.
    #[must_use]
    pub fn replacement(&self) -> &Text {
        &self.replacement
    }
}

/// The semantic result of applying a completion response.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CompletionOutcome {
    /// No distinct candidate began with the requested stem.
    NoMatch,
    /// One distinct candidate was inserted, including its optional suffix.
    Unique {
        edit: CompletionEdit,
        candidate: CompletionCandidate,
    },
    /// Several candidates remain; their longer common prefix may have been
    /// inserted, and the owned list is available for presentation.
    Ambiguous {
        edit: Option<CompletionEdit>,
        candidates: CompletionCandidates,
    },
}

// [spec:nshedit:req:core.token-completion+1]
impl<T: TerminalControl> Editor<T> {
    /// Snapshot the current completion stem and its checked source range.
    pub fn completion_query(&self, tokenizer: &Tokenizer) -> Result<CompletionQuery, Error> {
        let line = &self.state.line;
        let cursor = self.state.cursor;
        line.index(cursor.get())?;
        let line_start = line.as_units()[..cursor.get()]
            .iter()
            .rposition(|unit| *unit == TextUnit::Scalar('\n'))
            .map_or(0, |position| position + 1);
        let prefix: Text = line.as_units()[line_start..cursor.get()]
            .iter()
            .copied()
            .collect();
        let parsed = tokenizer.tokenize(&prefix, prefix.index(prefix.len())?)?;
        let parsed = parsed.line();
        let token_cursor = parsed.cursor();

        let (replacement_start, stem) = parsed.tokens().get(token_cursor.token().get()).map_or(
            (cursor.get(), Text::default()),
            |token| {
                (
                    line_start + token.source().start().get(),
                    token.value().clone(),
                )
            },
        );
        Ok(CompletionQuery {
            line: line.clone(),
            cursor,
            replacement: line.span(replacement_start..cursor.get())?,
            stem,
            quote: token_cursor.quote(),
            separators: tokenizer.separators().clone(),
        })
    }

    /// Apply a host completion response if its query still names this line.
    pub fn apply_completion(
        &mut self,
        query: &CompletionQuery,
        candidates: CompletionCandidates,
    ) -> Result<CompletionOutcome, Error> {
        if self.state.line != query.line || self.state.cursor != query.cursor {
            return Err(Error::StaleCompletionResponse);
        }

        let mut candidates = candidates.matching(&query.stem);
        match candidates.len() {
            0 => Ok(CompletionOutcome::NoMatch),
            1 => {
                let candidate = candidates.pop().expect("one candidate was counted");
                let mut replacement =
                    encode(&candidate.insertion, query.quote, &query.separators, true);
                if let Some(suffix) = &candidate.suffix {
                    replacement.extend(suffix.as_units().iter().copied());
                }
                let edit = apply_edit(&mut self.state, query.replacement, replacement)?;
                Ok(CompletionOutcome::Unique { edit, candidate })
            }
            _ => {
                let prefix = common_prefix(&candidates);
                let edit = if prefix.len() > query.stem.len() {
                    let replacement = encode(&prefix, query.quote, &query.separators, false);
                    Some(apply_edit(&mut self.state, query.replacement, replacement)?)
                } else {
                    None
                };
                Ok(CompletionOutcome::Ambiguous {
                    edit,
                    candidates: CompletionCandidates(candidates),
                })
            }
        }
    }
}

fn apply_edit(
    state: &mut line::State,
    span: TextSpan,
    replacement: Text,
) -> Result<CompletionEdit, Error> {
    let edit = CompletionEdit {
        span,
        replacement: replacement.clone(),
    };
    state.replace_at(replacement, span.start().get(), span.end().get())?;
    Ok(edit)
}

fn common_prefix(candidates: &[CompletionCandidate]) -> Text {
    let first = candidates
        .first()
        .expect("common prefixes require at least one candidate")
        .insertion
        .as_units();
    let length = candidates
        .iter()
        .skip(1)
        .fold(first.len(), |length, candidate| {
            first[..length]
                .iter()
                .zip(candidate.insertion.as_units())
                .take_while(|(left, right)| left == right)
                .count()
        });
    first[..length].iter().copied().collect()
}

fn encode(value: &Text, quote: QuoteStyle, separators: &Text, close: bool) -> Text {
    let mut encoded = Text::default();
    match quote {
        QuoteStyle::Unquoted => {
            for &unit in value {
                if unit == TextUnit::Scalar('\n') {
                    encoded.extend("'\n'".chars().map(TextUnit::Scalar));
                } else {
                    if needs_unquoted_escape(unit, separators) {
                        encoded.push(TextUnit::Scalar('\\'));
                    }
                    encoded.push(unit);
                }
            }
        }
        QuoteStyle::Single => {
            encoded.push(TextUnit::Scalar('\''));
            for &unit in value {
                if unit == TextUnit::Scalar('\'') {
                    encoded.extend("'\\''".chars().map(TextUnit::Scalar));
                } else {
                    encoded.push(unit);
                }
            }
            if close {
                encoded.push(TextUnit::Scalar('\''));
            }
        }
        QuoteStyle::Double => {
            encoded.push(TextUnit::Scalar('"'));
            for &unit in value {
                if matches!(unit, TextUnit::Scalar('"' | '\\')) {
                    encoded.push(TextUnit::Scalar('\\'));
                }
                encoded.push(unit);
            }
            if close {
                encoded.push(TextUnit::Scalar('"'));
            }
        }
    }
    encoded
}

fn needs_unquoted_escape(unit: TextUnit, separators: &Text) -> bool {
    let TextUnit::Scalar(character) = unit else {
        return false;
    };
    separators.as_units().contains(&unit)
        || matches!(
            character,
            '\'' | '"'
                | '\\'
                | '`'
                | '@'
                | '$'
                | '>'
                | '<'
                | '='
                | ';'
                | '|'
                | '&'
                | '{'
                | '}'
                | '('
                | ')'
                | '?'
                | '#'
                | '*'
                | '['
                | ']'
        )
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::domain::{Action, EditorConfig};

    struct TestTerminal;

    impl TerminalControl for TestTerminal {
        fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
            Ok(())
        }

        fn set_mode(&mut self, _mode: crate::domain::TerminalMode) -> io::Result<()> {
            Ok(())
        }

        fn restore(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn editor(source: &str) -> Editor<TestTerminal> {
        let mut editor = Editor::new(EditorConfig::default(), TestTerminal).unwrap();
        editor.state.line = Text::from(source);
        editor.state.cursor = editor.state.line.index(editor.state.line.len()).unwrap();
        editor
    }

    fn query_for(editor: &Editor<TestTerminal>) -> CompletionQuery {
        editor.completion_query(&Tokenizer::default()).unwrap()
    }

    // [spec:nshedit:req:core.token-completion+1/test]
    #[test]
    fn query_owns_a_checked_word_snapshot() {
        let editor = editor("echo one\\ two");
        let query = query_for(&editor);

        assert_eq!(query.line(), &Text::from("echo one\\ two"));
        assert_eq!(query.stem(), &Text::from("one two"));
        assert_eq!(query.replacement().start().get(), 5);
        assert_eq!(query.replacement().end(), query.cursor());
        assert_eq!(query.quote(), QuoteStyle::Unquoted);
    }

    #[test]
    fn unique_completion_is_one_undoable_edit() {
        let mut editor = editor("ec");
        let query = query_for(&editor);
        let candidates = vec![CompletionCandidate::new("echo").with_suffix(" ")].into();

        let outcome = editor.apply_completion(&query, candidates).unwrap();
        assert!(matches!(outcome, CompletionOutcome::Unique { .. }));
        assert_eq!(editor.line(), &Text::from("echo "));
        assert!(editor.can_undo());
        editor.execute(Action::Undo).unwrap();
        assert_eq!(editor.line(), &Text::from("ec"));
    }

    #[test]
    fn completion_preserves_text_after_cursor() {
        let mut editor = editor("foXX");
        editor.state.cursor = editor.state.line.index(2).unwrap();
        let query = query_for(&editor);

        editor
            .apply_completion(&query, vec![CompletionCandidate::new("foobar")].into())
            .unwrap();

        assert_eq!(editor.line(), &Text::from("foobarXX"));
        assert_eq!(editor.cursor().get(), 6);
    }

    #[test]
    fn ambiguous_completion_extends_common_prefix() {
        let mut editor = editor("fo");
        let query = query_for(&editor);
        let candidates = vec![
            CompletionCandidate::new("foobar"),
            CompletionCandidate::new("foobaz"),
            CompletionCandidate::new("foobar").with_display("duplicate"),
            CompletionCandidate::new("unrelated"),
        ]
        .into();

        let outcome = editor.apply_completion(&query, candidates).unwrap();
        let CompletionOutcome::Ambiguous { edit, candidates } = outcome else {
            panic!("two distinct matching candidates must remain ambiguous");
        };
        assert!(edit.is_some());
        assert_eq!(candidates.len(), 2);
        assert_eq!(editor.line(), &Text::from("fooba"));
    }

    #[test]
    fn no_match_creates_no_edit() {
        let mut editor = editor("zz");
        let query = query_for(&editor);
        let outcome = editor
            .apply_completion(&query, vec![CompletionCandidate::new("other")].into())
            .unwrap();

        assert_eq!(outcome, CompletionOutcome::NoMatch);
        assert_eq!(editor.line(), &Text::from("zz"));
        assert!(!editor.can_undo());
    }

    #[test]
    fn quote_encoding_round_trips_the_candidate() {
        let mut editor = editor("'fo");
        let query = query_for(&editor);
        let candidate = CompletionCandidate::new("foo'bar").with_suffix(" ");
        editor
            .apply_completion(&query, vec![candidate].into())
            .unwrap();

        assert_eq!(editor.line(), &Text::from("'foo'\\''bar' "));
        let parsed = Tokenizer::default()
            .tokenize(editor.line(), editor.cursor())
            .unwrap();
        assert_eq!(parsed.line().tokens()[0].value(), &Text::from("foo'bar"));
    }

    #[test]
    fn quote_styles_preserve_candidate_text() {
        let cases = [
            ("fo", Text::from("foo bar\nbaz")),
            ("\"fo", Text::from("foo\"bar\\baz\nnext")),
        ];
        for (source, candidate) in cases {
            let mut editor = editor(source);
            let query = query_for(&editor);
            editor
                .apply_completion(
                    &query,
                    vec![CompletionCandidate::new(candidate.clone())].into(),
                )
                .unwrap();
            let parsed = Tokenizer::default()
                .tokenize(editor.line(), editor.cursor())
                .unwrap();
            assert_eq!(parsed.line().tokens()[0].value(), &candidate);
        }
    }

    #[test]
    fn stale_response_cannot_change_reentrant_edits() {
        let mut editor = editor("ec");
        let query = query_for(&editor);
        editor.execute(Action::Insert(Text::from("h"))).unwrap();

        assert_eq!(
            editor.apply_completion(&query, vec![CompletionCandidate::new("echo")].into()),
            Err(Error::StaleCompletionResponse)
        );
        assert_eq!(editor.line(), &Text::from("ech"));
    }

    #[test]
    fn display_and_suffix_are_owned_separately() {
        let candidate = CompletionCandidate::new("src/main.rs")
            .with_display("main.rs")
            .with_suffix(" ");
        assert_eq!(candidate.insertion(), &Text::from("src/main.rs"));
        assert_eq!(candidate.display(), &Text::from("main.rs"));
        assert_eq!(candidate.suffix(), Some(&Text::from(" ")));
    }
}
