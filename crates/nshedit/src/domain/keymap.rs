use super::{Action, CommandSequence, EffectCommand, Error, Text};

// [spec:nshedit:req:core.line-commands]
/// A non-empty logical input sequence used as one keymap key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeySequence(Text);

impl KeySequence {
    /// Validate and own a logical input sequence.
    pub fn new(sequence: Text) -> Result<Self, Error> {
        if sequence.is_empty() {
            Err(Error::EmptyKeySequence)
        } else {
            Ok(Self(sequence))
        }
    }

    /// Borrow the complete logical sequence.
    #[must_use]
    pub fn as_text(&self) -> &Text {
        &self.0
    }

    pub(crate) fn starts_with(&self, prefix: &Self) -> bool {
        self.0.as_units().starts_with(prefix.0.as_units())
    }
}

impl TryFrom<Text> for KeySequence {
    type Error = Error;

    fn try_from(sequence: Text) -> Result<Self, Self::Error> {
        Self::new(sequence)
    }
}

impl TryFrom<&str> for KeySequence {
    type Error = Error;

    fn try_from(sequence: &str) -> Result<Self, Self::Error> {
        Self::new(Text::from(sequence))
    }
}

/// What a complete key sequence causes the driver to do.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Binding {
    /// Run one typed semantic action.
    Action(Action),
    /// Begin a closed driver-owned interaction protocol.
    Sequence(CommandSequence),
    /// Begin a closed command that yields an owned host effect.
    Effect(EffectCommand),
    /// Reprocess owned logical input through the active keymap.
    Macro(Text),
}

impl From<Action> for Binding {
    fn from(action: Action) -> Self {
        Self::Action(action)
    }
}

impl From<CommandSequence> for Binding {
    fn from(sequence: CommandSequence) -> Self {
        Self::Sequence(sequence)
    }
}

impl From<EffectCommand> for Binding {
    fn from(command: EffectCommand) -> Self {
        Self::Effect(command)
    }
}

/// Result of matching a typed sequence against the active keymap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyLookup<'a> {
    /// A complete binding with no longer sequence sharing this prefix.
    Exact(&'a Binding),
    /// A complete binding that is also the prefix of a longer binding.
    Ambiguous(&'a Binding),
    /// No complete binding yet, but a longer sequence may match.
    Prefix,
    /// Neither a binding nor a prefix.
    Unbound,
}
