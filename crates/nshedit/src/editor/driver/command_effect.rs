use crate::domain::{
    Action, Direction, EffectCommand, HistorySearchCommand, HistorySearchRepetition, InputMode,
    KeymapMode, RepeatCount, TerminalMode, Text, TextUnit,
};
use crate::editor::effect::{
    AliasEffect, AliasResponse, EditorCommandEffect, EditorCommandResponse, Effect,
    ExternalEditEffect, HistoryLineEffect, HistoryMatch, HistoryPosition, HistoryResponse,
    HistorySearchEffect, HistorySearchInput, HistorySearchResponse, HistorySelection,
    HistoryWordEffect, HistoryWordPosition, HistoryWordResponse, HostFailure,
};
use crate::editor::{Editor, TerminalControl};

use super::{DriverError, EffectKind, ReadDriver, ReadStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AfterHistoryLine {
    ExternalEdit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StoredHistorySearch {
    pattern: Text,
    direction: Direction,
}

// [spec:nshedit:req:core.command-effects]
impl ReadDriver {
    pub(super) fn dispatch_effect_command<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        command: EffectCommand,
        _invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.beep_pending = false;
        let repeat = self.take_optional_repeat(explicit_repeat);
        match command {
            EffectCommand::SearchHistory(command) => self.dispatch_history_search(editor, command),
            EffectCommand::ExpandAlias => {
                self.alias_selector_pending = true;
                self.advance(editor)
            }
            EffectCommand::SelectHistoryLine => {
                let Some(position) = history_position(repeat) else {
                    self.beep_pending = true;
                    return self.schedule_command_display(editor);
                };
                self.request_history_line(editor, position)
            }
            EffectCommand::InsertHistoryWord => {
                let Some(position) = history_word_position(repeat) else {
                    self.beep_pending = true;
                    return self.schedule_command_display(editor);
                };
                self.pending(
                    editor,
                    HistoryWordEffect { position },
                    EffectKind::HistoryWord,
                )
                .map(ReadStep::HistoryWord)
            }
            EffectCommand::ReadEditorCommand => self
                .pending(
                    editor,
                    EditorCommandEffect {
                        prompt: Text::from("\n: "),
                    },
                    EffectKind::EditorCommand,
                )
                .map(ReadStep::EditorCommand),
            EffectCommand::EditHistory => match repeat {
                Some(count) => {
                    let Some(count) = RepeatCount::new(count) else {
                        self.beep_pending = true;
                        return self.schedule_command_display(editor);
                    };
                    self.after_history_line = Some(AfterHistoryLine::ExternalEdit);
                    self.request_history_line(editor, HistoryPosition::Number(count))
                }
                None => self.request_external_edit(editor),
            },
        }
    }

    pub(super) fn consume_pending_effect_input<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        unit: TextUnit,
    ) -> Option<Result<ReadStep, DriverError>> {
        if !std::mem::take(&mut self.alias_selector_pending) {
            return None;
        }
        let mut name = Text::from("_");
        name.push(unit);
        Some(
            self.pending(editor, AliasEffect { name }, EffectKind::Alias)
                .map(ReadStep::Alias),
        )
    }

    pub(super) fn reject_effect_as_motion<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        self.pending_operator = None;
        self.repeat_argument = None;
        self.finish_change(editor);
        self.beep_pending = true;
        self.schedule_command_display(editor)
    }

    pub(super) fn cancel_pending_effect_command(&mut self) {
        self.alias_selector_pending = false;
        self.after_history_line = None;
    }

    /// Resume a pattern-based host history search.
    pub fn resume_history_search<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &super::Pending<HistorySearchEffect>,
        response: <HistorySearchEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let direction = pending.request().direction;
        let response = self.accept(editor, pending, EffectKind::HistorySearch, response)?;
        match response {
            Ok(HistorySearchResponse { history, pattern }) => {
                self.last_history_search = Some(StoredHistorySearch { pattern, direction });
                self.apply_history_response(editor, &history)?;
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(
                    editor,
                    super::ReadResult::Interrupted(super::ReadInterrupt::Host),
                );
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.after_host_action(editor)
    }

    /// Resume exact history-line selection, including an external-edit chain.
    pub fn resume_history_line<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &super::Pending<HistoryLineEffect>,
        response: <HistoryLineEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::HistoryLine, response)?;
        let continuation = self.after_history_line.take();
        match response {
            Ok(response) => {
                let selected = !matches!(response.selection(), HistorySelection::Unchanged);
                self.apply_history_response(editor, &response)?;
                if selected && continuation == Some(AfterHistoryLine::ExternalEdit) {
                    return self.request_external_edit(editor);
                }
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(
                    editor,
                    super::ReadResult::Interrupted(super::ReadInterrupt::Host),
                );
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.after_host_action(editor)
    }

    /// Resume selection of one word from the newest history line.
    pub fn resume_history_word<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &super::Pending<HistoryWordEffect>,
        response: <HistoryWordEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::HistoryWord, response)?;
        match response {
            Ok(HistoryWordResponse::Word(word)) => {
                let mut insertion = Text::from(" ");
                insertion.extend(word.as_units().iter().copied());
                let position = editor.cursor().get()
                    + usize::from(editor.cursor().get() < editor.line().len());
                let span = editor
                    .line()
                    .span(position..position)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                editor
                    .replace(span, insertion)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                editor
                    .execute(Action::SetModes {
                        input: InputMode::Insert,
                        keymap: KeymapMode::ViInsert,
                    })
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                editor.request_redraw();
            }
            Ok(HistoryWordResponse::Missing)
            | Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(
                    editor,
                    super::ReadResult::Interrupted(super::ReadInterrupt::Host),
                );
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.after_host_action(editor)
    }

    /// Resume an alias lookup and reprocess a successful expansion as input.
    pub fn resume_alias<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &super::Pending<AliasEffect>,
        response: <AliasEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::Alias, response)?;
        match response {
            Ok(AliasResponse::Expansion(expansion)) => {
                self.expand_macro(expansion, 1)
                    .map_err(|error| self.fail(editor, error))?;
                self.advance(editor)
            }
            Ok(AliasResponse::Missing) | Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
                self.after_host_action(editor)
            }
            Err(HostFailure::Interrupted) => self.complete(
                editor,
                super::ReadResult::Interrupted(super::ReadInterrupt::Host),
            ),
            Err(error) => Err(self.fail(editor, DriverError::Host(error))),
        }
    }

    /// Resume one host-owned editor command interaction.
    pub fn resume_editor_command<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &super::Pending<EditorCommandEffect>,
        response: <EditorCommandEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::EditorCommand, response)?;
        match response {
            Ok(EditorCommandResponse::Applied) => editor.request_redraw(),
            Ok(EditorCommandResponse::Rejected)
            | Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(
                    editor,
                    super::ReadResult::Interrupted(super::ReadInterrupt::Host),
                );
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.after_host_action(editor)
    }

    /// Resume host-side external editing and accept the edited line.
    pub fn resume_external_edit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &super::Pending<ExternalEditEffect>,
        response: <ExternalEditEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::ExternalEdit, response)?;
        match response {
            Ok(line) => {
                editor
                    .restore_history_line(line.clone())
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                self.after_outcome(editor, crate::domain::Outcome::Accepted(line))
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                editor
                    .set_terminal_mode(TerminalMode::Editing)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.beep_pending = true;
                self.after_host_action(editor)
            }
            Err(HostFailure::Interrupted) => self.complete(
                editor,
                super::ReadResult::Interrupted(super::ReadInterrupt::Host),
            ),
            Err(error) => Err(self.fail(editor, DriverError::Host(error))),
        }
    }

    pub(super) fn apply_history_response<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        response: &HistoryResponse,
    ) -> Result<(), DriverError> {
        let line = match response.selection() {
            HistorySelection::Entry(line) => Some(line.clone()),
            HistorySelection::Live => self.live_line.clone(),
            HistorySelection::Unchanged => None,
        };
        if let Some(line) = line {
            editor
                .restore_history_line(line)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            editor.request_redraw();
        }
        self.beep_pending |= response.reached_boundary();
        Ok(())
    }

    fn dispatch_history_search<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        command: HistorySearchCommand,
    ) -> Result<ReadStep, DriverError> {
        let (input, direction, matching) = match command {
            HistorySearchCommand::Prefix(direction) => {
                let end = editor.cursor().get();
                let pattern = editor.line().as_units()[..end].iter().copied().collect();
                (
                    HistorySearchInput::Pattern(pattern),
                    direction,
                    HistoryMatch::Prefix,
                )
            }
            HistorySearchCommand::Prompt(direction) => (
                HistorySearchInput::Prompted,
                direction,
                HistoryMatch::Contains,
            ),
            HistorySearchCommand::Incremental(direction) => (
                HistorySearchInput::Incremental,
                direction,
                HistoryMatch::Contains,
            ),
            HistorySearchCommand::Repeat(repetition) => {
                let Some(stored) = &self.last_history_search else {
                    self.beep_pending = true;
                    return self.schedule_command_display(editor);
                };
                let direction = match repetition {
                    HistorySearchRepetition::SameDirection => stored.direction,
                    HistorySearchRepetition::OppositeDirection => opposite(stored.direction),
                };
                (
                    HistorySearchInput::Pattern(stored.pattern.clone()),
                    direction,
                    HistoryMatch::Contains,
                )
            }
        };
        self.live_line.get_or_insert_with(|| editor.line().clone());
        self.pending(
            editor,
            HistorySearchEffect {
                input,
                direction,
                matching,
            },
            EffectKind::HistorySearch,
        )
        .map(ReadStep::HistorySearch)
    }

    fn request_history_line<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        position: HistoryPosition,
    ) -> Result<ReadStep, DriverError> {
        self.live_line.get_or_insert_with(|| editor.line().clone());
        self.pending(
            editor,
            HistoryLineEffect { position },
            EffectKind::HistoryLine,
        )
        .map(ReadStep::HistoryLine)
    }

    fn request_external_edit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        editor
            .set_terminal_mode(TerminalMode::Cooked)
            .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
        let line = editor.line().clone();
        self.pending(
            editor,
            ExternalEditEffect { line },
            EffectKind::ExternalEdit,
        )
        .map(ReadStep::ExternalEdit)
    }
}

fn history_position(repeat: Option<usize>) -> Option<HistoryPosition> {
    match repeat {
        None => Some(HistoryPosition::Oldest),
        Some(count) => RepeatCount::new(count).map(HistoryPosition::Number),
    }
}

fn history_word_position(repeat: Option<usize>) -> Option<HistoryWordPosition> {
    match repeat {
        None => Some(HistoryWordPosition::Last),
        Some(count) => RepeatCount::new(count).map(HistoryWordPosition::Number),
    }
}

const fn opposite(direction: Direction) -> Direction {
    match direction {
        Direction::Previous => Direction::Next,
        Direction::Next => Direction::Previous,
    }
}
