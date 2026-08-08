//! Rust-native editor-domain values.
//!
//! These types are deliberately independent of the translated compatibility
//! engine. They contain no C scalar aliases, raw pointers, operation codes,
//! flag words, errno values, or sentinel encodings. The safe editor shell is
//! built from this vocabulary; the ABI adapter translates at its boundary.

mod screen;
mod text;

pub use screen::{LiteralId, LiteralTable, Screen, ScreenCell, ScreenPosition, ScreenSize};
pub use text::{NonScalarWide, Text, TextIndex, TextSpan, TextUnit};

use std::fmt;

/// The editing command family selected for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditingMode {
    /// Emacs-style bindings and editing behaviour.
    #[default]
    Emacs,
    /// Vi insert and command modes.
    Vi,
}

/// How newly entered text changes the current line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum InputMode {
    /// Insert before the text at the cursor.
    #[default]
    Insert,
    /// Replace existing text until the mode changes.
    Replace,
    /// Replace one logical unit, then return to insertion.
    ReplaceOnce,
}

/// Whether the editor manages interactive terminal signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SignalPolicy {
    /// Install and restore the editor's signal handling.
    #[default]
    Handle,
    /// Leave signal handling entirely to the host.
    Ignore,
}

/// When input is returned to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Buffering {
    /// Accumulate input until a complete line is accepted.
    #[default]
    Line,
    /// Yield after each input unit.
    Character,
}

// [spec:nshedit:req:core.typed-domain+1]
/// Typed construction policy for a native editor session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EditorConfig {
    editing_mode: EditingMode,
    signal_policy: SignalPolicy,
    buffering: Buffering,
}

impl EditorConfig {
    /// Select the editing command family.
    #[must_use]
    pub const fn with_editing_mode(mut self, mode: EditingMode) -> Self {
        self.editing_mode = mode;
        self
    }

    /// Select how the session cooperates with host signal handling.
    #[must_use]
    pub const fn with_signal_policy(mut self, policy: SignalPolicy) -> Self {
        self.signal_policy = policy;
        self
    }

    /// Select line or character-at-a-time delivery.
    #[must_use]
    pub const fn with_buffering(mut self, buffering: Buffering) -> Self {
        self.buffering = buffering;
        self
    }

    /// The configured command family.
    #[must_use]
    pub const fn editing_mode(self) -> EditingMode {
        self.editing_mode
    }

    /// The configured signal policy.
    #[must_use]
    pub const fn signal_policy(self) -> SignalPolicy {
        self.signal_policy
    }

    /// The configured delivery policy.
    #[must_use]
    pub const fn buffering(self) -> Buffering {
        self.buffering
    }
}

/// A direction in logical text or history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Previous,
    Next,
}

/// Which word-classification rule a motion uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WordKind {
    /// Words and punctuation form separate runs.
    Word,
    /// Every non-whitespace run is one word.
    BigWord,
}

/// A cursor motion expressed without integer command identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    Character(Direction),
    Word {
        direction: Direction,
        kind: WordKind,
    },
    Line(Direction),
    StartOfLine,
    EndOfLine,
}

/// The semantic target of a deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeleteTarget {
    Character(Direction),
    Word {
        direction: Direction,
        kind: WordKind,
    },
    Line,
    Motion(Motion),
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

/// One semantic editing request.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    Insert(Text),
    Move(Motion),
    Delete(DeleteTarget),
    AcceptLine,
    EndOfInput,
    Complete,
    History(Direction),
    Undo,
    Redo,
    Refresh,
    User(CommandName),
}

/// The kind of redraw requested after an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refresh {
    Full,
    Redisplay,
    Beep,
}

/// A successful step of the editor state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Accepted(Text),
    EndOfInput,
    CursorMoved(TextIndex),
    Refresh(Refresh),
}

/// The result of applying one [`Action`].
pub type ActionResult = Result<Outcome, Error>;

/// A domain failure with all relevant values preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    EmptyCommandName,
    ScalarWideValue(u32),
    TextIndexOutOfBounds {
        index: usize,
        len: usize,
    },
    InvalidTextSpan {
        start: usize,
        end: usize,
        len: usize,
    },
    InvalidScreenSize {
        rows: usize,
        columns: usize,
    },
    ScreenTooLarge {
        rows: usize,
        columns: usize,
    },
    ScreenPositionOutOfBounds {
        row: usize,
        column: usize,
        rows: usize,
        columns: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::EmptyCommandName => formatter.write_str("a command name cannot be empty"),
            Self::ScalarWideValue(value) => {
                write!(formatter, "U+{value:04X} is a Unicode scalar value")
            }
            Self::TextIndexOutOfBounds { index, len } => {
                write!(formatter, "text index {index} exceeds length {len}")
            }
            Self::InvalidTextSpan { start, end, len } => {
                write!(
                    formatter,
                    "text span {start}..{end} is invalid for length {len}"
                )
            }
            Self::InvalidScreenSize { rows, columns } => {
                write!(
                    formatter,
                    "screen size {rows}x{columns} has an empty dimension"
                )
            }
            Self::ScreenTooLarge { rows, columns } => {
                write!(
                    formatter,
                    "screen size {rows}x{columns} cannot be represented"
                )
            }
            Self::ScreenPositionOutOfBounds {
                row,
                column,
                rows,
                columns,
            } => write!(
                formatter,
                "screen position ({row}, {column}) is outside {rows}x{columns}"
            ),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:nshedit:req:core.typed-domain+1/test]
    #[test]
    fn config_uses_typed_policies() {
        let config = EditorConfig::default()
            .with_editing_mode(EditingMode::Vi)
            .with_signal_policy(SignalPolicy::Ignore)
            .with_buffering(Buffering::Character);

        assert_eq!(config.editing_mode(), EditingMode::Vi);
        assert_eq!(config.signal_policy(), SignalPolicy::Ignore);
        assert_eq!(config.buffering(), Buffering::Character);
    }

    #[test]
    fn actions_carry_domain_values() {
        let action = Action::Move(Motion::Word {
            direction: Direction::Next,
            kind: WordKind::BigWord,
        });
        assert_eq!(
            action,
            Action::Move(Motion::Word {
                direction: Direction::Next,
                kind: WordKind::BigWord,
            })
        );
    }

    #[test]
    fn empty_user_command_is_an_error() {
        assert_eq!(CommandName::new(""), Err(Error::EmptyCommandName));
        assert_eq!(CommandName::new("transpose").unwrap().as_str(), "transpose");
    }

    #[test]
    fn outcomes_carry_owned_text() {
        let accepted = Outcome::Accepted(Text::from("echo hello"));
        assert_eq!(accepted, Outcome::Accepted(Text::from("echo hello")));
    }
}
