use crate::domain::{
    Action, ArgumentCommand, Binding, CharacterSearch, CharacterSearchLanding, CommandSequence,
    Direction, EditTarget, InputMode, KeymapMode, Motion, Outcome, SearchRepetition, TerminalMode,
    Text, TextIndex, TextSpan, TextUnit, ViInsertPlacement, ViOperator, ViSequence, ViSubstitution,
};
use crate::editor::{CommandStep, Editor, TerminalControl};

use super::{DriverError, ReadDriver, ReadStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepeatArgument {
    Explicit(usize),
    Multiplied(usize),
}

impl RepeatArgument {
    const fn value(self) -> usize {
        match self {
            Self::Explicit(value) | Self::Multiplied(value) => value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingUnit {
    KeySequenceContinuation,
    QuotedInsert {
        count: usize,
    },
    Replace {
        count: usize,
    },
    CharacterSearch {
        search: CharacterSearch,
        count: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PendingOperator {
    pub(super) operator: ViOperator,
    pub(super) anchor: TextIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StoredCharacterSearch {
    search: CharacterSearch,
    target: TextUnit,
}

#[derive(Debug, Clone)]
pub(super) enum ReplayStep {
    Invocation {
        binding: Binding,
        invoking: TextUnit,
        count: usize,
    },
    ContinuationUnit(TextUnit),
}

#[derive(Debug)]
pub(super) struct ChangeRecording {
    before: Text,
    steps: Vec<ReplayStep>,
    cost: usize,
}

#[derive(Debug, Clone)]
pub(super) struct ChangeReplay {
    steps: Vec<ReplayStep>,
    cost: usize,
}

// [spec:nshedit:req:core.command-sequences] driver-owned continuation execution
impl ReadDriver {
    pub(super) fn capture_count(
        &mut self,
        mode: KeymapMode,
        unit: TextUnit,
    ) -> Result<bool, DriverError> {
        let TextUnit::Scalar(character) = unit else {
            return Ok(false);
        };
        if mode != KeymapMode::ViCommand || !self.key_sequence.is_empty() {
            return Ok(false);
        }
        let Some(digit) = character.to_digit(10).map(|digit| digit as usize) else {
            return Ok(false);
        };
        if digit == 0 && self.repeat_argument.is_none() {
            return Ok(false);
        }
        self.append_argument_digit(digit)?;
        Ok(true)
    }

    pub(super) fn take_repeat(&mut self, explicit: Option<usize>) -> usize {
        explicit.unwrap_or_else(|| self.repeat_argument.take().map_or(1, RepeatArgument::value))
    }

    pub(super) fn take_optional_repeat(&mut self, explicit: Option<usize>) -> Option<usize> {
        explicit.or_else(|| self.repeat_argument.take().map(RepeatArgument::value))
    }

    pub(super) fn take_meta_unit(&mut self, unit: TextUnit) -> TextUnit {
        if !std::mem::take(&mut self.meta_next) {
            return unit;
        }
        match unit {
            TextUnit::Scalar(character) if (character as u32) < 0x100 => {
                TextUnit::from_wide((character as u32) | 0x80)
            }
            TextUnit::RawByte(byte) => TextUnit::RawByte(byte | 0x80),
            other => other,
        }
    }

    pub(super) fn dispatch_sequence<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        sequence: CommandSequence,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        match sequence {
            CommandSequence::Argument(command) => self.apply_argument(editor, command, invoking),
            CommandSequence::QuotedInsert => {
                self.begin_quoted_insert(editor, sequence, invoking, explicit_repeat)
            }
            CommandSequence::MetaNext => {
                self.meta_next = true;
                self.advance(editor)
            }
            CommandSequence::Vi(sequence) => {
                self.dispatch_vi_sequence(editor, sequence, invoking, explicit_repeat)
            }
        }
    }

    pub(super) fn dispatch_operator_action<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        action: Action,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        if let Action::Move(motion) = action {
            self.record_invocation(Binding::Action(Action::Move(motion)), invoking, count)
                .map_err(|error| self.fail(editor, error))?;
            self.apply_operator_motion(editor, motion, count)
        } else {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            self.schedule_command_display(editor)
        }
    }

    pub(super) fn consume_pending_unit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        unit: TextUnit,
    ) -> Option<Result<ReadStep, DriverError>> {
        let pending = self.pending_unit.take()?;
        Some(self.consume_unit(editor, pending, unit))
    }

    pub(super) fn cancel_pending_sequence<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<(), DriverError> {
        if matches!(self.pending_unit, Some(PendingUnit::QuotedInsert { .. })) {
            editor
                .set_terminal_mode(TerminalMode::Editing)
                .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
        }
        self.pending_unit = None;
        self.pending_operator = None;
        self.meta_next = false;
        self.finish_change(editor);
        Ok(())
    }

    pub(super) fn prepare_action_recording<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        action: &Action,
        invoking: TextUnit,
        count: usize,
    ) -> Result<(), DriverError> {
        if self.replaying_change {
            return Ok(());
        }
        let vi_mode = matches!(
            editor.keymap_mode(),
            KeymapMode::ViInsert | KeymapMode::ViCommand
        );
        if self.change_recording.is_none() && vi_mode && action_changes_line(action) {
            self.begin_change(editor);
        }
        if self.change_recording.is_some() && action_belongs_in_replay(action) {
            self.record_invocation(Binding::Action(action.clone()), invoking, count)?;
        }
        Ok(())
    }

    pub(super) fn finish_change_if_complete<T: TerminalControl>(&mut self, editor: &mut Editor<T>) {
        if editor.keymap_mode() == KeymapMode::ViCommand
            && self.pending_operator.is_none()
            && self.pending_unit.is_none()
            && !self.replaying_change
        {
            self.finish_change(editor);
        }
    }

    pub(super) fn finish_change<T: TerminalControl>(&mut self, editor: &mut Editor<T>) {
        let Some(recording) = self.change_recording.take() else {
            return;
        };
        editor.finish_edit_group();
        if editor.line() != &recording.before && !recording.steps.is_empty() {
            self.last_change = Some(ChangeReplay {
                steps: recording.steps,
                cost: recording.cost,
            });
        }
    }

    pub(super) fn advance_semantic_replay<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Option<Result<ReadStep, DriverError>> {
        if let Some(step) = self.semantic_replay.pop_front() {
            let result = match step {
                ReplayStep::Invocation {
                    binding,
                    invoking,
                    count,
                } => self.dispatch_resolved_binding(editor, binding, invoking, Some(count)),
                ReplayStep::ContinuationUnit(unit) => match self.consume_pending_unit(editor, unit)
                {
                    Some(result) => result,
                    None => Err(self.fail(editor, DriverError::InvalidSequenceState)),
                },
            };
            return Some(result);
        }
        if self.replaying_change {
            self.replaying_change = false;
            editor.finish_edit_group();
        }
        None
    }

    fn dispatch_vi_sequence<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        sequence: ViSequence,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        match sequence {
            ViSequence::Operator(operator) => {
                self.begin_operator(editor, operator, invoking, explicit_repeat)
            }
            ViSequence::Insert(placement) => {
                self.begin_vi_insert(editor, placement, invoking, explicit_repeat)
            }
            ViSequence::CommandMode => self.enter_vi_command_mode(editor, invoking),
            ViSequence::ReplaceNext => self.begin_replacement(editor, invoking, explicit_repeat),
            ViSequence::ReplaceMode => self.begin_replace_mode(editor, invoking, explicit_repeat),
            ViSequence::Substitute(target) => {
                self.substitute(editor, target, invoking, explicit_repeat)
            }
            ViSequence::CharacterSearch(search) => {
                self.begin_character_search(editor, search, invoking, explicit_repeat)
            }
            ViSequence::RepeatCharacterSearch(repetition) => {
                self.repeat_character_search(editor, repetition, invoking, explicit_repeat)
            }
            ViSequence::RepeatChange if self.pending_operator.is_none() => {
                self.replay_change(editor, explicit_repeat)
            }
            _ => {
                self.pending_operator = None;
                self.finish_change(editor);
                self.repeat_argument = None;
                self.queue_beep();
                self.schedule_command_display(editor)
            }
        }
    }

    pub(super) fn apply_argument<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        command: ArgumentCommand,
        invoking: TextUnit,
    ) -> Result<ReadStep, DriverError> {
        match command {
            ArgumentCommand::DigitOrInsert if self.repeat_argument.is_none() => {
                let Some(_) = decimal_digit(invoking) else {
                    self.queue_beep();
                    return self.schedule_command_display(editor);
                };
                self.prepare_action_recording(
                    editor,
                    &Action::Insert(std::iter::once(invoking).collect()),
                    invoking,
                    1,
                )?;
                self.dispatch_action(
                    editor,
                    Action::Insert(std::iter::once(invoking).collect()),
                    invoking,
                    1,
                )
            }
            ArgumentCommand::DigitOrInsert | ArgumentCommand::StartDigit => {
                let Some(digit) = decimal_digit(invoking) else {
                    self.queue_beep();
                    return self.schedule_command_display(editor);
                };
                let replace = matches!(
                    (command, self.repeat_argument),
                    (
                        ArgumentCommand::DigitOrInsert,
                        Some(RepeatArgument::Multiplied(_))
                    )
                );
                if replace {
                    self.repeat_argument = Some(RepeatArgument::Explicit(digit));
                } else {
                    self.append_argument_digit(digit)
                        .map_err(|error| self.fail(editor, error))?;
                }
                self.advance(editor)
            }
            ArgumentCommand::Multiply(factor) => {
                let count = self
                    .repeat_argument
                    .map_or(1, RepeatArgument::value)
                    .checked_mul(factor.get())
                    .filter(|count| *count <= self.work_limit)
                    .ok_or(DriverError::WorkLimitExceeded {
                        limit: self.work_limit,
                    })
                    .map_err(|error| self.fail(editor, error))?;
                self.repeat_argument = Some(RepeatArgument::Multiplied(count));
                self.advance(editor)
            }
        }
    }

    fn append_argument_digit(&mut self, digit: usize) -> Result<(), DriverError> {
        let count = self
            .repeat_argument
            .map_or(0, RepeatArgument::value)
            .checked_mul(10)
            .and_then(|count| count.checked_add(digit))
            .filter(|count| *count <= self.work_limit)
            .ok_or(DriverError::WorkLimitExceeded {
                limit: self.work_limit,
            })?;
        self.repeat_argument = Some(RepeatArgument::Explicit(count));
        Ok(())
    }

    fn begin_quoted_insert<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        sequence: CommandSequence,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        if editor.keymap_mode() == KeymapMode::ViInsert {
            self.begin_change(editor);
        }
        self.record_invocation(Binding::Sequence(sequence), invoking, count)
            .map_err(|error| self.fail(editor, error))?;
        editor
            .set_terminal_mode(TerminalMode::Quoted)
            .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
        self.pending_unit = Some(PendingUnit::QuotedInsert { count });
        self.advance(editor)
    }

    fn begin_operator<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        operator: ViOperator,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let repeat = explicit_repeat
            .map(RepeatArgument::Explicit)
            .or_else(|| self.repeat_argument.take());
        let count = repeat.map_or(1, RepeatArgument::value);
        let sequence = CommandSequence::Vi(ViSequence::Operator(operator));
        if let Some(pending) = self.pending_operator {
            if pending.operator != operator {
                self.pending_operator = None;
                self.finish_change(editor);
                self.queue_beep();
                return self.schedule_command_display(editor);
            }
            self.record_invocation(Binding::Sequence(sequence), invoking, count)
                .map_err(|error| self.fail(editor, error))?;
            return self.apply_operator_target(editor, EditTarget::Buffer);
        }

        self.begin_change(editor);
        self.record_invocation(Binding::Sequence(sequence), invoking, count)
            .map_err(|error| self.fail(editor, error))?;
        self.pending_operator = Some(PendingOperator {
            operator,
            anchor: editor.cursor(),
        });
        self.repeat_argument = repeat.map(|_| RepeatArgument::Explicit(count));
        self.advance(editor)
    }

    fn begin_vi_insert<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        placement: ViInsertPlacement,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        self.begin_change(editor);
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::Insert(placement))),
            invoking,
            count,
        )
        .map_err(|error| self.fail(editor, error))?;

        let destination = match placement {
            ViInsertPlacement::AtCursor => Ok(editor.cursor()),
            ViInsertPlacement::AfterCursor => editor.line().index(
                editor
                    .cursor()
                    .get()
                    .saturating_add(1)
                    .min(editor.line().len()),
            ),
            ViInsertPlacement::StartOfLine => editor.motion_destination(Motion::StartOfLine, 1),
            ViInsertPlacement::EndOfLine => editor.motion_destination(Motion::EndOfLine, 1),
        }
        .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
        self.apply_sequence_action(
            editor,
            Action::SetModes {
                input: InputMode::Insert,
                keymap: KeymapMode::ViInsert,
            },
        )?;
        self.schedule_command_display(editor)
    }

    fn enter_vi_command_mode<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
    ) -> Result<ReadStep, DriverError> {
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::CommandMode)),
            invoking,
            1,
        )
        .map_err(|error| self.fail(editor, error))?;
        self.pending_operator = None;
        self.repeat_argument = None;
        self.apply_sequence_action(
            editor,
            Action::SetModes {
                input: InputMode::Insert,
                keymap: KeymapMode::ViCommand,
            },
        )?;
        if editor.cursor() != TextIndex::START {
            self.apply_sequence_action(
                editor,
                Action::Move(Motion::Character(Direction::Previous)),
            )?;
        }
        self.schedule_command_display(editor)
    }

    fn begin_replacement<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        if editor.cursor().get() >= editor.line().len() {
            self.repeat_argument = None;
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        let count = self.take_repeat(explicit_repeat);
        self.begin_change(editor);
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::ReplaceNext)),
            invoking,
            count,
        )
        .map_err(|error| self.fail(editor, error))?;
        self.pending_unit = Some(PendingUnit::Replace { count });
        self.advance(editor)
    }

    fn begin_replace_mode<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        self.begin_change(editor);
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::ReplaceMode)),
            invoking,
            count,
        )
        .map_err(|error| self.fail(editor, error))?;
        self.apply_sequence_action(
            editor,
            Action::SetModes {
                input: InputMode::Replace,
                keymap: KeymapMode::ViInsert,
            },
        )?;
        self.schedule_command_display(editor)
    }

    fn substitute<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        substitution: ViSubstitution,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        self.begin_change(editor);
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::Substitute(substitution))),
            invoking,
            count,
        )
        .map_err(|error| self.fail(editor, error))?;
        let target = match substitution {
            ViSubstitution::Characters => {
                let start = editor.cursor().get();
                let end = start.saturating_add(count).min(editor.line().len());
                EditTarget::Span(
                    editor
                        .line()
                        .span(start..end)
                        .map_err(|error| self.fail(editor, DriverError::Editor(error)))?,
                )
            }
            ViSubstitution::Line => EditTarget::Buffer,
            ViSubstitution::ToEndOfLine => EditTarget::Motion(Motion::EndOfLine),
        };
        self.apply_sequence_action(editor, Action::Kill(target))?;
        self.apply_sequence_action(
            editor,
            Action::SetModes {
                input: InputMode::Insert,
                keymap: KeymapMode::ViInsert,
            },
        )?;
        self.schedule_command_display(editor)
    }

    fn begin_character_search<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        search: CharacterSearch,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::CharacterSearch(search))),
            invoking,
            count,
        )
        .map_err(|error| self.fail(editor, error))?;
        self.pending_unit = Some(PendingUnit::CharacterSearch { search, count });
        self.advance(editor)
    }

    fn repeat_character_search<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        repetition: SearchRepetition,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let count = self.take_repeat(explicit_repeat);
        self.record_invocation(
            Binding::Sequence(CommandSequence::Vi(ViSequence::RepeatCharacterSearch(
                repetition,
            ))),
            invoking,
            count,
        )
        .map_err(|error| self.fail(editor, error))?;
        let Some(stored) = self.last_character_search else {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        };
        let search = match repetition {
            SearchRepetition::SameDirection => stored.search,
            SearchRepetition::OppositeDirection => stored.search.reversed(),
        };
        self.run_character_search(editor, search, stored.target, count)
    }

    fn consume_unit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: PendingUnit,
        unit: TextUnit,
    ) -> Result<ReadStep, DriverError> {
        self.record_step(ReplayStep::ContinuationUnit(unit), 1)
            .map_err(|error| self.fail(editor, error))?;
        match pending {
            PendingUnit::KeySequenceContinuation => {
                self.queue_beep();
                self.process_unit(editor, unit)
            }
            PendingUnit::QuotedInsert { count } => {
                editor
                    .set_terminal_mode(TerminalMode::Editing)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                let inserted = std::iter::repeat_n(unit, count).collect();
                self.apply_sequence_action(editor, Action::Insert(inserted))?;
                if count > 1 {
                    self.clamp_vi_command_cursor(editor)?;
                }
                self.schedule_command_display(editor)
            }
            PendingUnit::Replace { count } => self.replace_with_unit(editor, unit, count),
            PendingUnit::CharacterSearch { search, count } => {
                self.last_character_search = Some(StoredCharacterSearch {
                    search,
                    target: unit,
                });
                self.run_character_search(editor, search, unit, count)
            }
        }
    }

    pub(super) fn replace_with_unit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        unit: TextUnit,
        count: usize,
    ) -> Result<ReadStep, DriverError> {
        let start = editor.cursor().get();
        let replaced = count.min(editor.line().len().saturating_sub(start));
        let span = editor
            .line()
            .span(start..start + replaced)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let replacement: Text = std::iter::repeat_n(unit, replaced).collect();
        editor
            .replace(span, replacement)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        self.apply_sequence_action(
            editor,
            Action::SetModes {
                input: InputMode::Insert,
                keymap: KeymapMode::ViCommand,
            },
        )?;
        if replaced != 0 {
            let cursor = editor
                .line()
                .index(start + replaced - 1)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            self.apply_sequence_action(editor, Action::Move(Motion::Absolute(cursor)))?;
        }
        self.schedule_command_display(editor)
    }

    fn run_character_search<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        search: CharacterSearch,
        target: TextUnit,
        count: usize,
    ) -> Result<ReadStep, DriverError> {
        let Some(destination) =
            character_destination(editor.line(), editor.cursor(), search, target, count)
        else {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        };
        if self.pending_operator.is_some() {
            self.apply_operator_character_destination(editor, destination)
        } else {
            self.apply_sequence_action(editor, Action::Move(Motion::Absolute(destination)))?;
            self.schedule_command_display(editor)
        }
    }

    fn apply_operator_motion<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        motion: Motion,
        motion_count: usize,
    ) -> Result<ReadStep, DriverError> {
        let pending = self
            .pending_operator
            .ok_or_else(|| self.fail(editor, DriverError::InvalidSequenceState))?;
        let destination = editor
            .motion_destination(motion, motion_count)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        if destination == pending.anchor
            && matches!(
                motion,
                Motion::Character(_) | Motion::Word { .. } | Motion::Line(_)
            )
        {
            self.pending_operator = None;
            self.finish_change(editor);
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        let span = operator_span(editor.line(), pending.anchor, destination, false)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        self.apply_operator_span(editor, span)
    }

    fn apply_operator_character_destination<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        destination: TextIndex,
    ) -> Result<ReadStep, DriverError> {
        let pending = self
            .pending_operator
            .ok_or_else(|| self.fail(editor, DriverError::InvalidSequenceState))?;
        let include_destination = destination.get() >= pending.anchor.get();
        let span = operator_span(
            editor.line(),
            pending.anchor,
            destination,
            include_destination,
        )
        .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        self.apply_operator_span(editor, span)
    }

    pub(super) fn apply_operator_span<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        span: TextSpan,
    ) -> Result<ReadStep, DriverError> {
        self.apply_operator_target(editor, EditTarget::Span(span))
    }

    fn apply_operator_target<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        target: EditTarget,
    ) -> Result<ReadStep, DriverError> {
        let pending = self
            .pending_operator
            .take()
            .ok_or_else(|| self.fail(editor, DriverError::InvalidSequenceState))?;
        let action = match pending.operator {
            ViOperator::Delete | ViOperator::Change => Action::Kill(target),
            ViOperator::Yank => Action::Copy(target),
        };
        self.apply_sequence_action(editor, action)?;
        if pending.operator == ViOperator::Change {
            self.apply_sequence_action(
                editor,
                Action::SetModes {
                    input: InputMode::Insert,
                    keymap: KeymapMode::ViInsert,
                },
            )?;
        }
        self.schedule_command_display(editor)
    }

    fn replay_change<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        let repeat = self.take_repeat(explicit_repeat);
        let Some(change) = self.last_change.clone() else {
            self.queue_beep();
            return self.schedule_command_display(editor);
        };
        let work = change
            .cost
            .checked_mul(repeat)
            .filter(|work| *work <= self.work_limit)
            .ok_or(DriverError::WorkLimitExceeded {
                limit: self.work_limit,
            })
            .map_err(|error| self.fail(editor, error))?;
        self.semantic_replay.reserve(work);
        for _ in 0..repeat {
            self.semantic_replay.extend(change.steps.iter().cloned());
        }
        editor.begin_edit_group();
        self.replaying_change = true;
        self.advance(editor)
    }

    fn begin_change<T: TerminalControl>(&mut self, editor: &mut Editor<T>) {
        if self.change_recording.is_none() && !self.replaying_change {
            editor.begin_edit_group();
            self.change_recording = Some(ChangeRecording {
                before: editor.line().clone(),
                steps: Vec::new(),
                cost: 0,
            });
        }
    }

    fn record_invocation(
        &mut self,
        binding: Binding,
        invoking: TextUnit,
        count: usize,
    ) -> Result<(), DriverError> {
        self.record_step(
            ReplayStep::Invocation {
                binding,
                invoking,
                count,
            },
            count,
        )
    }

    fn record_step(&mut self, step: ReplayStep, cost: usize) -> Result<(), DriverError> {
        if self.replaying_change {
            return Ok(());
        }
        let Some(recording) = &mut self.change_recording else {
            return Ok(());
        };
        let next_cost = recording
            .cost
            .checked_add(cost)
            .filter(|cost| *cost <= self.work_limit)
            .ok_or(DriverError::WorkLimitExceeded {
                limit: self.work_limit,
            })?;
        recording.steps.push(step);
        recording.cost = next_cost;
        Ok(())
    }

    pub(super) fn apply_sequence_action<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        action: Action,
    ) -> Result<Outcome, DriverError> {
        let step = editor
            .execute(action)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let CommandStep::Applied(outcome) = step else {
            return Err(self.fail(editor, DriverError::InvalidSequenceState));
        };
        self.note_outcome(editor, outcome.clone());
        Ok(outcome)
    }
}

fn decimal_digit(unit: TextUnit) -> Option<usize> {
    let value = match unit {
        TextUnit::Scalar(character) if (character as u32) < 0x100 => character as u32,
        TextUnit::RawByte(byte) => u32::from(byte),
        TextUnit::Scalar(_) | TextUnit::CompatibilityWide(_) => return None,
    };
    char::from_u32(value & 0x7f)?
        .to_digit(10)
        .map(|digit| digit as usize)
}

fn character_destination(
    line: &Text,
    cursor: TextIndex,
    search: CharacterSearch,
    target: TextUnit,
    count: usize,
) -> Option<TextIndex> {
    let units = line.as_units();
    let mut position = cursor.get();
    for _ in 0..count {
        position = match search.direction() {
            Direction::Next => {
                units
                    .get(position.saturating_add(1)..)?
                    .iter()
                    .position(|unit| *unit == target)?
                    + position
                    + 1
            }
            Direction::Previous => units
                .get(..position)?
                .iter()
                .rposition(|unit| *unit == target)?,
        };
    }
    let landing = match (search.direction(), search.landing()) {
        (Direction::Next, CharacterSearchLanding::BeforeTarget) => position.saturating_sub(1),
        (Direction::Previous, CharacterSearchLanding::BeforeTarget) => {
            position.saturating_add(1).min(line.len())
        }
        (_, CharacterSearchLanding::OnTarget) => position,
    };
    line.index(landing).ok()
}

pub(super) fn operator_span(
    line: &Text,
    anchor: TextIndex,
    destination: TextIndex,
    include_destination: bool,
) -> Result<TextSpan, crate::domain::Error> {
    line.index(anchor.get())?;
    line.index(destination.get())?;
    let start = anchor.get().min(destination.get());
    let mut end = anchor.get().max(destination.get());
    if include_destination && end < line.len() {
        end += 1;
    }
    if start == end && start < line.len() {
        end += 1;
    }
    line.span(start..end)
}

fn action_changes_line(action: &Action) -> bool {
    matches!(
        action,
        Action::Insert(_)
            | Action::Delete(_)
            | Action::DeleteOrEndOfInput
            | Action::Kill(_)
            | Action::Yank(_)
            | Action::Transform { .. }
            | Action::TransposeCharacters
    )
}

fn action_belongs_in_replay(action: &Action) -> bool {
    !matches!(
        action,
        Action::Noop
            | Action::AcceptLine
            | Action::EndOfInput
            | Action::Complete
            | Action::History(_)
            | Action::Undo
            | Action::Redo
            | Action::Refresh(_)
    )
}
