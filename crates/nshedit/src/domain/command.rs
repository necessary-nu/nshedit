use super::{Error, InputMode, Text, TextIndex, TextSpan};

/// A direction in logical text or history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Toward earlier text or older history.
    Previous,
    /// Toward later text or newer history.
    Next,
}

// [spec:nshedit:req:core.read-driver]
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
/// Pure line actions are applied immediately. Actions that require a host
/// service are returned as a typed command step for the driver to suspend.
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
    /// Ask the host completion service to inspect the current line.
    Complete,
    /// Ask the host history service for an adjacent entry.
    History(Direction),
    /// Restore the most recent command-level text mutation.
    Undo,
    /// Reapply the most recently undone text mutation.
    Redo,
    /// Request a particular kind of redraw.
    Refresh(Refresh),
    /// Ask the host to execute a registered command.
    User(CommandName),
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
