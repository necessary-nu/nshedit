use std::num::NonZeroUsize;

use super::Direction;

/// A non-zero number of times to apply one semantic command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepeatCount(NonZeroUsize);

impl RepeatCount {
    /// One application of a command.
    pub const ONE: Self = Self(NonZeroUsize::MIN);

    /// Validate a caller-supplied repeat count.
    #[must_use]
    pub const fn new(count: usize) -> Option<Self> {
        match NonZeroUsize::new(count) {
            Some(count) => Some(Self(count)),
            None => None,
        }
    }

    /// The validated count as a plain integer.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

impl Default for RepeatCount {
    fn default() -> Self {
        Self::ONE
    }
}

/// How a repeat argument prefix changes the driver's current count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArgumentCommand {
    /// Add the invoking decimal digit to an existing argument, or insert it
    /// normally when no argument is active.
    DigitOrInsert,
    /// Start or extend an argument with the invoking decimal digit.
    StartDigit,
    /// Multiply the current argument, using one when none is active.
    Multiply(RepeatCount),
}

/// Where a Vi insertion session begins relative to the current line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViInsertPlacement {
    /// Insert at the checked cursor boundary.
    AtCursor,
    /// Insert after the logical unit under the cursor.
    AfterCursor,
    /// Insert at the beginning of the logical line.
    StartOfLine,
    /// Insert at the end of the logical line.
    EndOfLine,
}

/// The line operation composed with a following Vi motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViOperator {
    /// Remove the selected text into the kill register.
    Delete,
    /// Remove the selected text and begin inserting its replacement.
    Change,
    /// Copy the selected text into the kill register.
    Yank,
}

/// Where a character-search motion lands relative to its match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharacterSearchLanding {
    /// Land on the matching unit.
    OnTarget,
    /// Land one logical unit before the match in the search direction.
    BeforeTarget,
}

/// A semantic Vi character-search motion awaiting its target unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CharacterSearch {
    direction: Direction,
    landing: CharacterSearchLanding,
}

impl CharacterSearch {
    /// Describe a character search without supplying its later target unit.
    #[must_use]
    pub const fn new(direction: Direction, landing: CharacterSearchLanding) -> Self {
        Self { direction, landing }
    }

    /// The direction in which occurrences are selected.
    #[must_use]
    pub const fn direction(self) -> Direction {
        self.direction
    }

    /// Whether the cursor lands on or immediately before the target.
    #[must_use]
    pub const fn landing(self) -> CharacterSearchLanding {
        self.landing
    }

    /// Search in the opposite direction while retaining the landing rule.
    #[must_use]
    pub const fn reversed(self) -> Self {
        let direction = match self.direction {
            Direction::Previous => Direction::Next,
            Direction::Next => Direction::Previous,
        };
        Self {
            direction,
            landing: self.landing,
        }
    }
}

/// Whether a stored character search keeps or reverses its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchRepetition {
    /// Repeat in the direction of the stored explicit search.
    SameDirection,
    /// Repeat opposite to the stored explicit search.
    OppositeDirection,
}

/// Which region a Vi substitution replaces with a new insertion session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViSubstitution {
    /// Replace a counted run of units beginning at the cursor.
    Characters,
    /// Replace the complete logical line.
    Line,
    /// Replace from the cursor through the end of the logical line.
    ToEndOfLine,
}

/// A Vi interaction implemented as a driver-owned typed continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViSequence {
    /// Await and compose a semantic motion with a line operator.
    Operator(ViOperator),
    /// Begin an insertion session at a semantic placement.
    Insert(ViInsertPlacement),
    /// Leave insertion or replacement and select the command keymap.
    CommandMode,
    /// Await one unit and replace a counted run with it.
    ReplaceNext,
    /// Begin a persistent replacement session.
    ReplaceMode,
    /// Delete a semantic region and begin inserting its replacement.
    Substitute(ViSubstitution),
    /// Await a target unit for a character-search motion.
    CharacterSearch(CharacterSearch),
    /// Reapply the stored character-search motion.
    RepeatCharacterSearch(SearchRepetition),
    /// Replay the last recorded semantic change at the current cursor.
    RepeatChange,
}

// [spec:nshedit:req:core.command-sequences]
/// A closed interaction protocol that may consume later logical input.
///
/// Compatibility command names and numeric command identifiers are resolved
/// before entering this type. The read driver owns each live continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandSequence {
    /// Update a bounded repeat argument.
    Argument(ArgumentCommand),
    /// Read and insert the next unit without key dispatch.
    QuotedInsert,
    /// Apply the meta bit to the next dispatch unit.
    MetaNext,
    /// Run a Vi-specific multi-step interaction.
    Vi(ViSequence),
}
