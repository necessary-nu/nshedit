//! Private typed state and pure commands for one editable logical line.

mod keymap;
mod motion;

use crate::domain::{
    Action, Binding, Direction, EditTarget, EditingMode, EditorConfig, Error, InputMode, KeyLookup,
    KeySequence, KeymapMode, Motion, Outcome, Text, TextIndex, TextSpan, TextTransform, TextUnit,
    YankPlacement,
};

use super::CommandStep;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    line: Text,
    cursor: TextIndex,
    mark: Option<TextIndex>,
    input_mode: InputMode,
}

// [spec:nshedit:req:core.line-commands]
#[derive(Debug)]
pub(super) struct State {
    pub(super) line: Text,
    pub(super) cursor: TextIndex,
    mark: Option<TextIndex>,
    kill: Option<Text>,
    search_pattern: Option<Text>,
    input_mode: InputMode,
    keymap_mode: KeymapMode,
    keymaps: keymap::Keymaps,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl State {
    pub(super) fn new(config: EditorConfig) -> Self {
        let keymap_mode = initial_keymap(config);
        Self {
            line: Text::default(),
            cursor: TextIndex::START,
            mark: None,
            kill: None,
            search_pattern: None,
            input_mode: InputMode::Insert,
            keymap_mode,
            keymaps: keymap::Keymaps::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub(super) fn reset_line(&mut self, config: EditorConfig) {
        self.line.clear();
        self.cursor = TextIndex::START;
        self.mark = None;
        self.input_mode = InputMode::Insert;
        self.keymap_mode = initial_keymap(config);
        self.undo.clear();
        self.redo.clear();
    }

    pub(super) const fn input_mode(&self) -> InputMode {
        self.input_mode
    }

    pub(super) const fn keymap_mode(&self) -> KeymapMode {
        self.keymap_mode
    }

    pub(super) const fn mark(&self) -> Option<TextIndex> {
        self.mark
    }

    pub(super) fn kill_buffer(&self) -> Option<&Text> {
        self.kill.as_ref()
    }

    pub(super) fn search_pattern(&self) -> Option<&Text> {
        self.search_pattern.as_ref()
    }

    pub(super) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(super) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(super) fn bind(
        &mut self,
        mode: KeymapMode,
        sequence: KeySequence,
        binding: Binding,
    ) -> Option<Binding> {
        self.keymaps.bind(mode, sequence, binding)
    }

    pub(super) fn unbind(&mut self, mode: KeymapMode, sequence: &KeySequence) -> Option<Binding> {
        self.keymaps.unbind(mode, sequence)
    }

    pub(super) fn lookup(&self, sequence: &KeySequence) -> KeyLookup<'_> {
        self.keymaps.lookup(self.keymap_mode, sequence)
    }

    pub(super) fn execute(&mut self, action: Action) -> Result<CommandStep, Error> {
        let outcome = match action {
            Action::Insert(text) => self.insert(text)?,
            Action::Move(motion) => self.move_cursor(motion)?,
            Action::Delete(target) => self.delete(target, false)?,
            Action::Kill(target) => self.delete(target, true)?,
            Action::Copy(target) => self.copy(target)?,
            Action::Yank(placement) => self.yank(placement)?,
            Action::SetMark => self.set_mark(),
            Action::ExchangeMark => self.exchange_mark()?,
            Action::Transform { target, transform } => self.transform(target, transform)?,
            Action::TransposeCharacters => self.transpose()?,
            Action::Search { pattern, direction } => self.search(pattern, direction)?,
            Action::RepeatSearch(direction) => self.repeat_search(direction)?,
            Action::SetInputMode(mode) => {
                self.input_mode = mode;
                Outcome::Continue
            }
            Action::SetKeymap(mode) => {
                self.keymap_mode = mode;
                Outcome::Continue
            }
            Action::SetModes { input, keymap } => {
                self.input_mode = input;
                self.keymap_mode = keymap;
                Outcome::Continue
            }
            Action::AcceptLine => Outcome::Accepted(self.line.clone()),
            Action::EndOfInput => Outcome::EndOfInput,
            Action::Complete => return Ok(CommandStep::NeedsCompletion),
            Action::History(direction) => return Ok(CommandStep::NeedsHistory(direction)),
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Refresh(refresh) => Outcome::Refresh(refresh),
            Action::User(name) => return Ok(CommandStep::NeedsUserCommand(name)),
        };
        Ok(CommandStep::Applied(outcome))
    }

    fn insert(&mut self, inserted: Text) -> Result<Outcome, Error> {
        if inserted.is_empty() {
            return Ok(Outcome::Continue);
        }
        let start = self.cursor.get();
        let replaced_end = match self.input_mode {
            InputMode::Insert => start,
            InputMode::Replace => start.saturating_add(inserted.len()).min(self.line.len()),
            InputMode::ReplaceOnce => start.saturating_add(1).min(self.line.len()),
        };
        let reset_once = self.input_mode == InputMode::ReplaceOnce;
        let outcome = self.replace_at(inserted, start, replaced_end)?;
        if reset_once {
            self.input_mode = InputMode::Insert;
        }
        Ok(outcome)
    }

    fn move_cursor(&mut self, movement: Motion) -> Result<Outcome, Error> {
        self.cursor = motion::destination(&self.line, self.cursor, movement)?;
        Ok(Outcome::CursorMoved(self.cursor))
    }

    fn delete(&mut self, target: EditTarget, save: bool) -> Result<Outcome, Error> {
        let span = motion::target_span(&self.line, self.cursor, self.mark, target)?;
        let removed = self.record_edit(|state| {
            let removed = state.line.remove(span)?;
            state.adjust_mark(span, 0)?;
            state.cursor = state.line.index(span.start().get())?;
            Ok(removed)
        })?;
        if save {
            self.kill = Some(removed);
        }
        Ok(Outcome::Continue)
    }

    fn copy(&mut self, target: EditTarget) -> Result<Outcome, Error> {
        let span = motion::target_span(&self.line, self.cursor, self.mark, target)?;
        self.kill = Some(self.line.slice(span)?.iter().copied().collect());
        Ok(Outcome::Continue)
    }

    fn yank(&mut self, placement: YankPlacement) -> Result<Outcome, Error> {
        let Some(killed) = self.kill.clone() else {
            return Ok(Outcome::Continue);
        };
        let start = match placement {
            YankPlacement::AtCursor => self.cursor.get(),
            YankPlacement::AfterCursor => self.cursor.get().saturating_add(1).min(self.line.len()),
        };
        self.replace_at(killed, start, start)
    }

    fn set_mark(&mut self) -> Outcome {
        self.mark = Some(self.cursor);
        Outcome::Continue
    }

    fn exchange_mark(&mut self) -> Result<Outcome, Error> {
        let mark = self.mark.ok_or(Error::MarkNotSet)?;
        self.line.index(mark.get())?;
        self.mark = Some(self.cursor);
        self.cursor = mark;
        Ok(Outcome::CursorMoved(mark))
    }

    fn transform(
        &mut self,
        target: EditTarget,
        transform: TextTransform,
    ) -> Result<Outcome, Error> {
        let span = motion::target_span(&self.line, self.cursor, self.mark, target)?;
        let replacement = transformed(self.line.slice(span)?, transform);
        self.record_edit(|state| {
            state.line.replace(span, &replacement)?;
            state.adjust_mark(span, replacement.len())?;
            let end = span
                .start()
                .get()
                .checked_add(replacement.len())
                .ok_or(Error::TextLengthOverflow)?;
            state.cursor = state.line.index(end)?;
            Ok(Outcome::Continue)
        })
    }

    fn transpose(&mut self) -> Result<Outcome, Error> {
        let cursor = self.cursor.get();
        if cursor == 0 || self.line.len() < 2 {
            return Ok(Outcome::Continue);
        }
        let first = if cursor == self.line.len() {
            cursor - 2
        } else {
            cursor - 1
        };
        let span = self.line.span(first..first + 2)?;
        let units = self.line.slice(span)?;
        let replacement = [units[1], units[0]].into_iter().collect();
        self.record_edit(|state| {
            state.line.replace(span, &replacement)?;
            state.cursor = state
                .line
                .index(cursor.saturating_add(1).min(state.line.len()))?;
            Ok(Outcome::Continue)
        })
    }

    fn search(&mut self, pattern: Text, direction: Direction) -> Result<Outcome, Error> {
        if pattern.is_empty() {
            return Err(Error::EmptySearchPattern);
        }
        let found = motion::find_pattern(&self.line, &pattern, self.cursor, direction, true);
        self.search_pattern = Some(pattern);
        self.search_outcome(found)
    }

    fn repeat_search(&mut self, direction: Direction) -> Result<Outcome, Error> {
        let pattern = self
            .search_pattern
            .as_ref()
            .ok_or(Error::SearchPatternNotSet)?;
        let found = motion::find_pattern(&self.line, pattern, self.cursor, direction, false);
        self.search_outcome(found)
    }

    fn search_outcome(&mut self, found: Option<usize>) -> Result<Outcome, Error> {
        match found {
            Some(position) => {
                self.cursor = self.line.index(position)?;
                Ok(Outcome::CursorMoved(self.cursor))
            }
            None => Ok(Outcome::Refresh(crate::domain::Refresh::Beep)),
        }
    }

    fn undo(&mut self) -> Outcome {
        let Some(previous) = self.undo.pop() else {
            return Outcome::Continue;
        };
        let current = self.snapshot();
        self.restore(previous);
        self.redo.push(current);
        Outcome::CursorMoved(self.cursor)
    }

    fn redo(&mut self) -> Outcome {
        let Some(next) = self.redo.pop() else {
            return Outcome::Continue;
        };
        let current = self.snapshot();
        self.restore(next);
        self.undo.push(current);
        Outcome::CursorMoved(self.cursor)
    }

    pub(super) fn replace_at(
        &mut self,
        replacement: Text,
        start: usize,
        replaced_end: usize,
    ) -> Result<Outcome, Error> {
        self.record_edit(|state| {
            let span = state.line.span(start..replaced_end)?;
            state.line.replace(span, &replacement)?;
            state.adjust_mark(span, replacement.len())?;
            let end = start
                .checked_add(replacement.len())
                .ok_or(Error::TextLengthOverflow)?;
            state.cursor = state.line.index(end)?;
            Ok(Outcome::Continue)
        })
    }

    fn record_edit<R>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<R, Error>,
    ) -> Result<R, Error> {
        let before = self.snapshot();
        match operation(self) {
            Ok(result) => {
                if self.line != before.line {
                    self.undo.push(before);
                    self.redo.clear();
                }
                Ok(result)
            }
            Err(error) => {
                self.restore(before);
                Err(error)
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            line: self.line.clone(),
            cursor: self.cursor,
            mark: self.mark,
            input_mode: self.input_mode,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.line = snapshot.line;
        self.cursor = snapshot.cursor;
        self.mark = snapshot.mark;
        self.input_mode = snapshot.input_mode;
    }

    fn adjust_mark(&mut self, replaced: TextSpan, inserted: usize) -> Result<(), Error> {
        let Some(mark) = self.mark else {
            return Ok(());
        };
        let start = replaced.start().get();
        let end = replaced.end().get();
        let position = if replaced.is_empty() && mark.get() > start {
            mark.get()
                .checked_add(inserted)
                .ok_or(Error::TextLengthOverflow)?
        } else if mark.get() < start {
            mark.get()
        } else if mark.get() <= end {
            start
                .checked_add((mark.get() - start).min(inserted))
                .ok_or(Error::TextLengthOverflow)?
        } else {
            (mark.get() - replaced.len())
                .checked_add(inserted)
                .ok_or(Error::TextLengthOverflow)?
        };
        self.mark = Some(self.line.index(position)?);
        Ok(())
    }
}

fn initial_keymap(config: EditorConfig) -> KeymapMode {
    match config.editing_mode() {
        EditingMode::Emacs => KeymapMode::Emacs,
        EditingMode::Vi => KeymapMode::ViInsert,
    }
}

fn transformed(units: &[TextUnit], transform: TextTransform) -> Text {
    let mut result = Text::default();
    for &unit in units {
        let TextUnit::Scalar(character) = unit else {
            result.push(unit);
            continue;
        };
        match transform {
            TextTransform::Lowercase => {
                result.extend(character.to_lowercase().map(TextUnit::Scalar));
            }
            TextTransform::Uppercase => {
                result.extend(character.to_uppercase().map(TextUnit::Scalar));
            }
            TextTransform::ToggleCase if character.is_lowercase() => {
                result.extend(character.to_uppercase().map(TextUnit::Scalar));
            }
            TextTransform::ToggleCase => {
                result.extend(character.to_lowercase().map(TextUnit::Scalar));
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
