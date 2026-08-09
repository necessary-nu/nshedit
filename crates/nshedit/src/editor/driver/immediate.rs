use crate::domain::{
    Action, ArgumentCommand, Direction, EditTarget, ImmediateCommand, InputMode, Motion, Outcome,
    Text, TextIndex, TextTransform, TextUnit, WordKind, WordTraversal, YankPlacement,
};
use crate::editor::{CommandStep, Editor, TerminalControl};

use super::sequence::operator_span;
use super::{DisplayKind, DriverError, ReadDriver, ReadResult, ReadStep};

pub(super) fn action_repeats_with_count(action: &Action) -> bool {
    matches!(
        action,
        Action::Insert(_)
            | Action::DeleteOrEndOfInput
            | Action::Kill(EditTarget::Word { .. })
            | Action::Transform {
                target: EditTarget::Word { .. },
                ..
            }
    )
}

// [spec:nshedit:req:abi.binding-dispatch]
impl ReadDriver {
    pub(super) fn dispatch_immediate<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        command: ImmediateCommand,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        if self.pending_operator.is_some() {
            match command {
                ImmediateCommand::EndOfWord(kind) => {
                    return self.move_to_word_end(editor, kind, explicit_repeat);
                }
                ImmediateCommand::MatchDelimiter => {
                    return self.match_delimiter(editor, explicit_repeat);
                }
                ImmediateCommand::MoveToColumn => {
                    return self.move_to_column(editor, explicit_repeat);
                }
                _ => {}
            }
        }

        match command {
            ImmediateCommand::InsertInvoking => {
                self.insert_invoking(editor, invoking, explicit_repeat)
            }
            ImmediateCommand::KeySequenceLeadIn => {
                if let Some(repeat) = explicit_repeat {
                    self.repeat_argument = Some(super::sequence::RepeatArgument::Explicit(repeat));
                }
                self.pending_unit = Some(super::sequence::PendingUnit::KeySequenceContinuation);
                self.advance(editor)
            }
            ImmediateCommand::TraverseWords {
                direction,
                operation,
            } => self.traverse_words(editor, direction, operation, invoking, explicit_repeat),
            ImmediateCommand::EndOfWord(kind) => {
                self.move_to_word_end(editor, kind, explicit_repeat)
            }
            ImmediateCommand::EndOfInputIfEmpty => {
                self.end_of_input_if_empty(editor, invoking, explicit_repeat)
            }
            ImmediateCommand::MatchDelimiter => self.match_delimiter(editor, explicit_repeat),
            ImmediateCommand::MoveToColumn => self.move_to_column(editor, explicit_repeat),
            ImmediateCommand::CommentAndAccept => self.comment_and_accept(editor, explicit_repeat),
            ImmediateCommand::TransposeBeforeCursor => {
                self.transpose_before_cursor(editor, invoking, explicit_repeat)
            }
            ImmediateCommand::DeletePreviousUnit => {
                self.delete_previous_unit(editor, invoking, explicit_repeat)
            }
            ImmediateCommand::PasteRegister(placement) => {
                self.paste_register(editor, placement, invoking, explicit_repeat)
            }
            ImmediateCommand::ToggleCaseAndAdvance => {
                self.toggle_case_and_advance(editor, invoking, explicit_repeat)
            }
            ImmediateCommand::StartOfLineOrArgument => {
                self.start_of_line_or_argument(editor, invoking, explicit_repeat)
            }
            ImmediateCommand::UndoRequired => self.undo_required(editor, invoking, explicit_repeat),
            ImmediateCommand::DeleteFollowingOrEndOfInput => {
                self.delete_following_or_end_of_input(editor, invoking, explicit_repeat)
            }
        }
    }

    fn insert_invoking<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        if invoking == TextUnit::Scalar('\0') || count == 0 {
            self.queue_beep_if(invoking == TextUnit::Scalar('\0'));
            return self.schedule_command_display(editor);
        }
        let inserted: Text = std::iter::repeat_n(invoking, count).collect();
        self.prepare_action_recording(editor, &Action::Insert(inserted.clone()), invoking, 1)?;

        match editor.input_mode() {
            InputMode::ReplaceOnce => self.replace_with_unit(editor, invoking, count),
            InputMode::Replace if count != 1 => {
                let cursor = editor.cursor().get();
                let span = editor
                    .line()
                    .span(cursor..cursor)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                editor
                    .replace(span, inserted)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                self.schedule_command_display(editor)
            }
            InputMode::Insert | InputMode::Replace => {
                self.apply_sequence_action(editor, Action::Insert(inserted))?;
                if count > 1 {
                    self.clamp_vi_command_cursor(editor)?;
                }
                self.schedule_command_display(editor)
            }
        }
    }

    fn traverse_words<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        direction: Direction,
        operation: WordTraversal,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        let origin = editor.cursor();
        let at_boundary = match direction {
            Direction::Previous => origin == TextIndex::START,
            Direction::Next => origin.get() == editor.line().len(),
        };
        if at_boundary && !matches!(operation, WordTraversal::Transform(_)) {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        let destination = traversed_word_boundary(editor, direction, count)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;

        match operation {
            WordTraversal::Move if self.pending_operator.is_some() => {
                let pending = self
                    .pending_operator
                    .expect("the guarded operator remains pending");
                let span = operator_span(editor.line(), pending.anchor, destination, false)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                self.apply_operator_span(editor, span)
            }
            WordTraversal::Move => {
                self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
                self.clamp_vi_command_cursor(editor)?;
                self.schedule_command_display(editor)
            }
            WordTraversal::Kill => {
                let span = editor
                    .line()
                    .span(origin.get().min(destination.get())..origin.get().max(destination.get()))
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                let action = Action::Kill(EditTarget::Span(span));
                self.prepare_action_recording(editor, &action, invoking, 1)?;
                self.apply_sequence_action(editor, action)?;
                self.clamp_vi_command_cursor(editor)?;
                self.schedule_command_display(editor)
            }
            WordTraversal::Duplicate => {
                let source_span = editor
                    .line()
                    .span(destination.get()..origin.get())
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                let source = match editor.line().slice(source_span) {
                    Ok(units) => units.iter().copied().collect::<Text>(),
                    Err(error) => return Err(self.fail(editor, DriverError::Editor(error))),
                };
                let insertion = editor
                    .line()
                    .span(origin.get()..origin.get())
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                let action = Action::Insert(source.clone());
                self.prepare_action_recording(editor, &action, invoking, 1)?;
                editor
                    .replace(insertion, source)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                self.schedule_command_display(editor)
            }
            WordTraversal::Transform(transform) => {
                let span = editor
                    .line()
                    .span(origin.get().min(destination.get())..origin.get().max(destination.get()))
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                let action = Action::Transform {
                    target: EditTarget::Span(span),
                    transform,
                };
                self.prepare_action_recording(editor, &action, invoking, 1)?;
                self.apply_sequence_action(editor, action)?;
                self.clamp_vi_command_cursor(editor)?;
                self.schedule_command_display(editor)
            }
        }
    }

    fn move_to_word_end<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        kind: WordKind,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let motion_count = self.take_repeat(explicit_repeat);
        let count = motion_count;
        let Some(destination) = word_end(editor.line(), editor.cursor(), kind, count) else {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        };
        if let Some(pending) = self.pending_operator {
            let span = operator_span(editor.line(), pending.anchor, destination, true)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            self.apply_operator_span(editor, span)
        } else {
            self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
            self.schedule_command_display(editor)
        }
    }

    fn end_of_input_if_empty<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        if editor.line().is_empty() {
            self.completion = Some(ReadResult::EndOfInput);
            self.schedule_display(editor, DisplayKind::Echo(invoking))
        } else {
            self.queue_beep();
            self.queue_beep();
            self.schedule_command_display(editor)
        }
    }

    fn match_delimiter<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        let Some((destination, direction)) = matching_delimiter(editor.line(), editor.cursor())
        else {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        };
        if let Some(pending) = self.pending_operator {
            let adjusted = match direction {
                Direction::Next => destination,
                Direction::Previous => editor
                    .line()
                    .index(destination.get().saturating_add(1))
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?,
            };
            let span = operator_span(
                editor.line(),
                pending.anchor,
                adjusted,
                direction == Direction::Next,
            )
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            self.apply_operator_span(editor, span)
        } else {
            self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
            self.schedule_command_display(editor)
        }
    }

    fn move_to_column<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let motion_count = self.take_repeat(explicit_repeat);
        if editor.line().is_empty() || (editor.line().len() == 1 && self.pending_operator.is_none())
        {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        let count = motion_count;
        let destination = editor
            .line()
            .index(count.saturating_sub(1).min(editor.line().len()))
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        if let Some(pending) = self.pending_operator {
            let span = operator_span(editor.line(), pending.anchor, destination, false)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            self.apply_operator_span(editor, span)
        } else {
            self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
            self.schedule_command_display(editor)
        }
    }

    fn comment_and_accept<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        let start = editor
            .line()
            .span(0..0)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        editor
            .replace(start, Text::from("#"))
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        self.apply_sequence_action(editor, Action::Move(Motion::StartOfBuffer))?;
        let step = editor
            .execute(Action::AcceptLine)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let CommandStep::Applied(outcome @ Outcome::Accepted(_)) = step else {
            return Err(self.fail(editor, DriverError::InvalidSequenceState));
        };
        self.after_outcome(editor, outcome)
    }

    fn transpose_before_cursor<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        let cursor = editor.cursor().get();
        if cursor < 2 {
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        self.prepare_action_recording(editor, &Action::TransposeCharacters, invoking, 1)?;
        let span = editor
            .line()
            .span(cursor - 2..cursor)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let replacement = match editor.line().slice(span) {
            Ok(units) => [units[1], units[0]].into_iter().collect(),
            Err(error) => return Err(self.fail(editor, DriverError::Editor(error))),
        };
        editor
            .replace(span, replacement)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        self.schedule_command_display(editor)
    }

    fn delete_previous_unit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        let action = Action::Delete(EditTarget::Character(Direction::Previous));
        self.prepare_action_recording(editor, &action, invoking, 1)?;
        self.dispatch_counted_character_delete(editor, Direction::Previous, 1)
    }

    fn paste_register<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        placement: YankPlacement,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        if editor.kill_buffer().is_none_or(Text::is_empty) {
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        let action = Action::Yank(placement);
        self.prepare_action_recording(editor, &action, invoking, 1)?;
        self.dispatch_action(editor, action, invoking, 1)
    }

    fn toggle_case_and_advance<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        let start = editor.cursor().get();
        if start >= editor.line().len() {
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        if count == 0 {
            return self.schedule_command_display(editor);
        }
        let end = start.saturating_add(count).min(editor.line().len());
        let reaches_end = end == editor.line().len();
        let span = editor
            .line()
            .span(start..end)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let action = Action::Transform {
            target: EditTarget::Span(span),
            transform: TextTransform::ToggleCase,
        };
        self.prepare_action_recording(editor, &action, invoking, 1)?;
        self.apply_sequence_action(editor, action)?;
        if reaches_end {
            let destination = editor
                .line()
                .index(editor.line().len().saturating_sub(1))
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
        }
        self.schedule_command_display(editor)
    }

    fn start_of_line_or_argument<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        if self.repeat_argument.is_some() || explicit_repeat.is_some() {
            return self.apply_argument(editor, ArgumentCommand::StartDigit, invoking);
        }
        if self.pending_operator.is_some() {
            return self.dispatch_operator_action(
                editor,
                Action::Move(Motion::StartOfLine),
                invoking,
                None,
            );
        }
        self.dispatch_action(editor, Action::Move(Motion::StartOfLine), invoking, 1)
    }

    fn undo_required<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        if !editor.can_undo() {
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        self.dispatch_action(editor, Action::Undo, invoking, 1)
    }

    fn delete_following_or_end_of_input<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        let start = editor.cursor().get();
        if editor.line().is_empty() {
            self.completion = Some(ReadResult::EndOfInput);
            return self.schedule_display(editor, DisplayKind::Echo(invoking));
        }
        if start == editor.line().len() {
            self.queue_beep();
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        if count != 0 {
            let end = start.saturating_add(count).min(editor.line().len());
            let span = editor
                .line()
                .span(start..end)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            let action = Action::Delete(EditTarget::Span(span));
            self.prepare_action_recording(editor, &action, invoking, 1)?;
            self.apply_sequence_action(editor, action)?;
            self.clamp_vi_command_cursor(editor)?;
        }
        self.schedule_command_display(editor)
    }
}

fn traversed_word_boundary<T: TerminalControl>(
    editor: &Editor<T>,
    direction: Direction,
    count: usize,
) -> Result<TextIndex, crate::domain::Error> {
    let units = editor.line().as_units();
    let mut position = editor.cursor().get();
    for _ in 0..count {
        match direction {
            Direction::Previous => {
                while position > 0 && !editor.word_policy().is_word(units[position - 1]) {
                    position -= 1;
                }
                while position > 0 && editor.word_policy().is_word(units[position - 1]) {
                    position -= 1;
                }
            }
            Direction::Next => {
                while position < units.len() && !editor.word_policy().is_word(units[position]) {
                    position += 1;
                }
                while position < units.len() && editor.word_policy().is_word(units[position]) {
                    position += 1;
                }
            }
        }
    }
    editor.line().index(position)
}

fn word_end(line: &Text, cursor: TextIndex, kind: WordKind, count: usize) -> Option<TextIndex> {
    let units = line.as_units();
    if cursor.get() >= units.len() {
        return None;
    }
    if count == 0 {
        return Some(cursor);
    }
    let mut position = cursor.get();
    for _ in 0..count {
        position = position.saturating_add(1);
        while position < units.len() && is_space(units[position]) {
            position += 1;
        }
        if position >= units.len() {
            position = units.len() - 1;
            break;
        }
        let class = word_class(units[position], kind);
        while position < units.len() && word_class(units[position], kind) == class {
            position += 1;
        }
        position = position.saturating_sub(1);
    }
    line.index(position).ok()
}

fn matching_delimiter(line: &Text, cursor: TextIndex) -> Option<(TextIndex, Direction)> {
    let units = line.as_units();
    let (origin, opening, closing, direction) = units
        .iter()
        .enumerate()
        .skip(cursor.get())
        .find_map(|(position, unit)| delimiter(*unit).map(|pair| (position, pair)))
        .map(|(position, (opening, closing, direction))| (position, opening, closing, direction))?;
    let mut depth = 1usize;
    let destination = match direction {
        Direction::Next => {
            let mut found = None;
            for (position, unit) in units.iter().enumerate().skip(origin + 1) {
                if *unit == opening {
                    depth += 1;
                } else if *unit == closing {
                    depth -= 1;
                    if depth == 0 {
                        found = Some(position);
                        break;
                    }
                }
            }
            found?
        }
        Direction::Previous => {
            let mut found = None;
            for position in (0..origin).rev() {
                if units[position] == closing {
                    depth += 1;
                } else if units[position] == opening {
                    depth -= 1;
                    if depth == 0 {
                        found = Some(position);
                        break;
                    }
                }
            }
            found?
        }
    };
    line.index(destination).ok().map(|index| (index, direction))
}

fn delimiter(unit: TextUnit) -> Option<(TextUnit, TextUnit, Direction)> {
    let scalar = |character| TextUnit::Scalar(character);
    match unit {
        TextUnit::Scalar('(') => Some((scalar('('), scalar(')'), Direction::Next)),
        TextUnit::Scalar(')') => Some((scalar('('), scalar(')'), Direction::Previous)),
        TextUnit::Scalar('[') => Some((scalar('['), scalar(']'), Direction::Next)),
        TextUnit::Scalar(']') => Some((scalar('['), scalar(']'), Direction::Previous)),
        TextUnit::Scalar('{') => Some((scalar('{'), scalar('}'), Direction::Next)),
        TextUnit::Scalar('}') => Some((scalar('{'), scalar('}'), Direction::Previous)),
        TextUnit::Scalar(_) | TextUnit::RawByte(_) | TextUnit::CompatibilityWide(_) => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Space,
    Word,
    Other,
}

fn word_class(unit: TextUnit, kind: WordKind) -> WordClass {
    if is_space(unit) {
        WordClass::Space
    } else if kind == WordKind::BigWord
        || matches!(unit, TextUnit::Scalar(character) if character.is_alphanumeric() || character == '_')
    {
        WordClass::Word
    } else {
        WordClass::Other
    }
}

fn is_space(unit: TextUnit) -> bool {
    matches!(unit, TextUnit::Scalar(character) if character.is_whitespace())
}
