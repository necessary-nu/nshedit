use crate::domain::{Error, Text, WordPolicy};

use super::{Editor, TerminalControl};

impl<T: TerminalControl> Editor<T> {
    /// The configured classifier used by word-oriented commands.
    #[must_use]
    pub const fn word_policy(&self) -> &WordPolicy {
        self.state.word_policy()
    }

    /// Replace the additional characters used for word classification.
    pub fn set_word_policy(&mut self, policy: WordPolicy) {
        self.state.set_word_policy(policy);
    }

    /// Insert host-provided text at the cursor without adding an undo entry.
    ///
    /// This is intended for session setup and host synchronization. Normal
    /// interactive edits should use a semantic insert action so they remain
    /// undoable.
    pub fn insert_untracked(&mut self, text: Text) -> Result<(), Error> {
        self.state.insert_untracked(text)
    }

    /// Replace the complete line from a host snapshot without adding an undo
    /// entry. The cursor and mark are retained where the new line permits.
    pub fn replace_line_untracked(&mut self, line: Text) -> Result<(), Error> {
        self.state.replace_line_untracked(line)
    }
}
