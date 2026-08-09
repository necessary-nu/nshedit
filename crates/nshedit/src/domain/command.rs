use super::{EditingMode, Error, InputMode, Text, TextIndex, TextSpan, TextUnit};

/// A direction in logical text or history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Toward earlier text or older history.
    Previous,
    /// Toward later text or newer history.
    Next,
}

/// A terminal-facing signal reported without exposing platform signal numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Signal {
    /// The interactive interrupt request, normally `SIGINT`.
    Interrupt,
    /// The interactive quit request, normally `SIGQUIT`.
    Quit,
    /// Loss of the controlling session, normally `SIGHUP`.
    Hangup,
    /// A termination request, normally `SIGTERM`.
    Terminate,
    /// A job-control stop request, normally `SIGTSTP`.
    Suspend,
    /// Resumption after a job-control stop, normally `SIGCONT`.
    Continue,
    /// A terminal-size change, normally `SIGWINCH`.
    Resize,
}

/// Which word-classification rule a motion uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WordKind {
    /// Alphanumeric scalars and underscore form words; punctuation is a
    /// separate run.
    Word,
    /// Every non-whitespace run is one word.
    BigWord,
}

/// Configurable classification used by word-oriented commands.
///
/// Alphanumeric scalars are always words. This policy owns the additional
/// logical units that should be treated as word characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WordPolicy {
    additional: Text,
}

impl WordPolicy {
    /// Build a policy from an owned set of additional word characters.
    #[must_use]
    pub const fn new(additional: Text) -> Self {
        Self { additional }
    }

    /// The conventional policy for an editing family.
    #[must_use]
    pub fn for_editing_mode(mode: EditingMode) -> Self {
        let additional = match mode {
            EditingMode::Emacs => Text::from("*?_-.[]~="),
            EditingMode::Vi => Text::from("_"),
        };
        Self { additional }
    }

    /// Whether a logical unit belongs to a word under this policy.
    #[must_use]
    pub fn is_word(&self, unit: TextUnit) -> bool {
        matches!(unit, TextUnit::Scalar(character) if character.is_alphanumeric())
            || self.additional.as_units().contains(&unit)
    }

    /// Borrow the additional word characters owned by this policy.
    #[must_use]
    pub const fn additional(&self) -> &Text {
        &self.additional
    }
}

impl Default for WordPolicy {
    fn default() -> Self {
        Self::for_editing_mode(EditingMode::Emacs)
    }
}

/// What to do with the span reached by a configured word traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WordTraversal {
    /// Move to the reached word boundary.
    Move,
    /// Delete the traversed span into the kill register.
    Kill,
    /// Duplicate the traversed span at the cursor.
    Duplicate,
    /// Transform the traversed span.
    Transform(TextTransform),
}

/// A cursor motion expressed without integer command identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    /// Move by one logical text unit.
    Character(Direction),
    /// Move by one classified word.
    Word {
        direction: Direction,
        kind: WordKind,
    },
    /// Move vertically by one embedded logical line, preserving the column
    /// where possible.
    Line(Direction),
    /// Move to the start of the current embedded logical line.
    StartOfLine,
    /// Move to the end of the current embedded logical line.
    EndOfLine,
    /// Move to the first boundary in the complete edit buffer.
    StartOfBuffer,
    /// Move to the end boundary in the complete edit buffer.
    EndOfBuffer,
    /// Move to an already checked boundary, which is revalidated against the
    /// current buffer when the command runs.
    Absolute(TextIndex),
}

/// A region resolved against the line and cursor when an action runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditTarget {
    /// One logical unit on the selected side of the cursor.
    Character(Direction),
    /// Text between the cursor and the next classified word boundary.
    Word {
        direction: Direction,
        kind: WordKind,
    },
    /// Text between the cursor and a semantic motion destination.
    Motion(Motion),
    /// The current embedded logical line, excluding its newline delimiter.
    Line,
    /// The complete edit buffer.
    Buffer,
    /// A caller-supplied checked span, revalidated against the current line.
    Span(TextSpan),
    /// The ordered region between the cursor and the saved mark.
    MarkedRegion,
}

/// A Unicode-aware transformation applied to scalar text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextTransform {
    /// Apply Unicode lowercase mapping.
    Lowercase,
    /// Apply Unicode uppercase mapping.
    Uppercase,
    /// Exchange lowercase and uppercase scalar characters.
    ToggleCase,
    /// Uppercase the first alphabetic scalar and lowercase later uppercase
    /// scalars in the selected text.
    Capitalize,
}

/// Where a yank inserts its owned register contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum YankPlacement {
    /// Insert at the current cursor boundary.
    #[default]
    AtCursor,
    /// Insert after the logical unit beginning at the cursor, or at the end
    /// when there is no following unit.
    AfterCursor,
}

/// A validated host-defined command name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandName(Box<str>);

impl CommandName {
    /// Validate and own a command name.
    pub fn new(name: impl Into<Box<str>>) -> Result<Self, Error> {
        let name = name.into();
        if name.is_empty() {
            Err(Error::EmptyCommandName)
        } else {
            Ok(Self(name))
        }
    }

    /// Borrow the spelling supplied by the host.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// [spec:nshedit:req:core.line-commands]
/// One semantic editor action.
///
/// Every action is applied immediately to private line state. Commands that
/// require a host service are represented separately by [`EffectCommand`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    /// Leave editor state unchanged.
    Noop,
    /// Insert or replace logical text according to the current input mode.
    Insert(Text),
    /// Move the checked cursor without changing text.
    Move(Motion),
    /// Delete text without changing the kill register.
    Delete(EditTarget),
    /// Delete the next logical unit, or report end of input when the line is
    /// empty.
    DeleteOrEndOfInput,
    /// Delete text and replace the kill register with it.
    Kill(EditTarget),
    /// Copy text into the kill register without deleting it.
    Copy(EditTarget),
    /// Insert the current kill register at a semantic placement.
    Yank(YankPlacement),
    /// Save the current cursor as the mark.
    SetMark,
    /// Exchange the current cursor and saved mark.
    ExchangeMark,
    /// Apply a Unicode case transformation to a resolved region.
    Transform {
        target: EditTarget,
        transform: TextTransform,
    },
    /// Exchange the logical units around the cursor.
    TransposeCharacters,
    /// Set and execute an exact logical-text search.
    Search { pattern: Text, direction: Direction },
    /// Repeat the stored exact search in the selected direction.
    RepeatSearch(Direction),
    /// Select how following inserted text changes the line.
    SetInputMode(InputMode),
    /// Exchange insert and replace input modes.
    ToggleInputMode,
    /// Select the active typed keymap.
    SetKeymap(KeymapMode),
    /// Change input and keymap modes as one command transition.
    SetModes {
        input: InputMode,
        keymap: KeymapMode,
    },
    /// Accept an owned snapshot of the current line.
    AcceptLine,
    /// Report end of input.
    EndOfInput,
    /// Restore the most recent command-level text mutation.
    Undo,
    /// Reapply the most recently undone text mutation.
    Redo,
    /// Request a particular kind of redraw.
    Refresh(Refresh),
}

// [spec:nshedit:req:abi.binding-dispatch]
/// A closed, immediate command whose final action depends on dispatch context.
///
/// These values carry semantic intent rather than compatibility command names.
/// The read driver supplies the invoking unit, repeat count, current mode, and
/// pending Vi operator before applying checked line actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImmediateCommand {
    /// Insert copies of the unit that selected this binding.
    InsertInvoking,
    /// Swallow an incomplete key-sequence prefix without changing state.
    KeySequenceLeadIn,
    /// Traverse configured words and apply one semantic operation to the
    /// reached span.
    TraverseWords {
        /// Direction in which word boundaries are traversed.
        direction: Direction,
        /// Operation applied to the traversed span.
        operation: WordTraversal,
    },
    /// Move to the final unit of a classified word.
    EndOfWord(WordKind),
    /// Report end of input only when the editable line is empty.
    EndOfInputIfEmpty,
    /// Match the first paired delimiter at or after the cursor.
    MatchDelimiter,
    /// Move to the one-based logical column supplied as the repeat count.
    MoveToColumn,
    /// Prefix the line with a comment marker and accept it.
    CommentAndAccept,
    /// Exchange the two logical units immediately before the cursor.
    TransposeBeforeCursor,
    /// Delete exactly one logical unit immediately before the cursor.
    DeletePreviousUnit,
    /// Insert the kill register, failing when it has no contents.
    PasteRegister(YankPlacement),
    /// Toggle case over the repeat span and leave the cursor on the final
    /// transformed unit when the span reaches the end of the line.
    ToggleCaseAndAdvance,
    /// Move to the start of the line unless an argument is being entered, in
    /// which case the invoking unit extends that argument.
    StartOfLineOrArgument,
    /// Restore the most recent edit, failing when no snapshot exists.
    UndoRequired,
    /// Delete a counted following span, report end of input for an empty
    /// line, or notify twice when invoked at the end of non-empty input.
    DeleteFollowingOrEndOfInput,
}

/// The active family of key bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KeymapMode {
    /// Emacs-style editing bindings.
    #[default]
    Emacs,
    /// Vi text-entry bindings.
    ViInsert,
    /// Vi command bindings.
    ViCommand,
}

/// The kind of redraw requested after an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refresh {
    /// Redraw the complete editor frame without forgetting its terminal
    /// contents.
    Redraw,
    /// Recompute and draw the complete display.
    Full,
    /// Draw from the current logical damage point.
    Redisplay,
    /// Notify the user without changing the line.
    Beep,
}

/// A successful step of the editor state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The action completed without a more specific result.
    Continue,
    /// A snapshot of the line was accepted.
    Accepted(Text),
    /// The input source ended.
    EndOfInput,
    /// The cursor moved to a checked boundary.
    CursorMoved(TextIndex),
    /// The display needs the selected refresh operation.
    Refresh(Refresh),
}
