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
        let repeat = self.take_optional_repeat(explicit_repeat);
        match command {
            EffectCommand::SearchHistory(command) => self.dispatch_history_search(editor, command),
            EffectCommand::ExpandAlias => {
                self.alias_selector_pending = true;
                self.advance(editor)
            }
            EffectCommand::SelectHistoryLine => {
                let Some(position) = history_position(repeat) else {
                    self.queue_beep();
                    return self.schedule_command_display(editor);
                };
                self.request_history_line(editor, position)
            }
            EffectCommand::RestoreHistoryLine => {
                self.request_history_line(editor, HistoryPosition::Current)
            }
            EffectCommand::InsertHistoryWord => {
                let Some(position) = history_word_position(repeat) else {
                    self.queue_beep();
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
                        self.queue_beep();
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
        let clear_prompt_line = matches!(pending.request().input, HistorySearchInput::Prompted);
        let response = self.accept(editor, pending, EffectKind::HistorySearch, response)?;
        if clear_prompt_line {
            editor
                .replace_line_untracked(Text::default())
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        }
        match response {
            Ok(HistorySearchResponse { history, pattern }) => {
                self.last_history_search = Some(StoredHistorySearch { pattern, direction });
                self.apply_history_response(editor, &history, true)?;
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
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
                self.apply_history_response(editor, &response, true)?;
                if selected
                    && !response.reached_boundary()
                    && continuation == Some(AfterHistoryLine::ExternalEdit)
                {
                    return self.request_external_edit(editor);
                }
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
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
                self.queue_beep();
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
                if editor.config().buffering() == crate::domain::Buffering::Command {
                    self.schedule_command_display(editor)
                } else {
                    self.advance(editor)
                }
            }
            Ok(AliasResponse::Missing) => self.after_host_action(editor),
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
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
        editor
            .replace_line_untracked(Text::default())
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        match response {
            Ok(EditorCommandResponse::Applied) => editor.request_redraw(),
            Ok(EditorCommandResponse::Rejected)
            | Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
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
                self.apply_sequence_action(
                    editor,
                    Action::Move(crate::domain::Motion::Absolute(
                        crate::domain::TextIndex::START,
                    )),
                )?;
                self.after_outcome(editor, crate::domain::Outcome::Accepted(line))
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                editor
                    .set_terminal_mode(TerminalMode::Editing)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.queue_beep();
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
        notify_boundary: bool,
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
        self.queue_beep_if(notify_boundary && response.reached_boundary());
        Ok(())
    }

    fn dispatch_history_search<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        command: HistorySearchCommand,
    ) -> Result<ReadStep, DriverError> {
        let (input, direction, matching) = match command {
            HistorySearchCommand::Prefix(direction) => {
                let end = editor.cursor().get()
                    + usize::from(
                        editor.keymap_mode() == KeymapMode::ViCommand
                            && editor.cursor().get() < editor.line().len(),
                    );
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
                HistorySearchInput::Incremental(editor.keymap_mode()),
                direction,
                HistoryMatch::Contains,
            ),
            HistorySearchCommand::Repeat(repetition) => {
                let Some(stored) = &self.last_history_search else {
                    self.queue_beep();
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
