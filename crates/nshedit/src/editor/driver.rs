//! Resumable input preparation, decoding, dispatch, and completion.

mod command_effect;
mod decode;
mod error;
mod immediate;
mod sequence;
mod signal;

use std::collections::VecDeque;
use std::io::Write;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::domain::{
    Action, Binding, Buffering, Direction, EditTarget, KeyLookup, KeySequence, KeymapMode, Motion,
    Outcome, Prompt, Refresh, RepeatCount, Signal, TerminalMode, Text, TextUnit,
};

use super::effect::{
    AliasEffect, CompletionEffect, EditorCommandEffect, Effect, EffectResult, ExternalEditEffect,
    HistoryLineEffect, HistoryNavigateEffect, HistoryRecordEffect, HistorySearchEffect,
    HistoryWordEffect, HostFailure, PromptEffect, PromptSide, ReadEffect, ReadOutcome,
    ResizeEffect, SignalEffect, Suspension, UserCommandEffect,
};
use super::{CompletionOutcome, Editor, RenderError, TerminalControl, Tokenizer};
use decode::Decoder;
pub use error::DriverError;
use immediate::action_repeats_with_count;

/// Why a native read stopped without accepting text or reaching EOF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReadInterrupt {
    /// The host cancelled its blocking read.
    Cancelled,
    /// The host reported an interruption without a particular signal.
    Host,
    /// A semantic terminal signal interrupted this read.
    Signal(Signal),
}

/// A completed invocation of the native read driver.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReadResult {
    /// The line editor accepted this owned line snapshot.
    Accepted(Text),
    /// Character buffering returned one logical input unit.
    Character(TextUnit),
    /// Command buffering dispatched one complete keymap command.
    Command,
    /// The input source ended.
    EndOfInput,
    /// Input was cancelled or interrupted.
    Interrupted(ReadInterrupt),
}

/// The terminal output operation requested by the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DisplayKind {
    /// Rebuild and emit the prompt, line, and cursor.
    Refresh,
    /// Emit only the configured audible or visible notification.
    Beep,
    /// Refresh first, then emit the notification.
    RefreshAndBeep(usize),
    /// Emit the accepted-line break before completing the read.
    FinishLine,
    /// Echo the invoking unit before completing end of input.
    Echo(TextUnit),
}

/// An owned display request that can be performed with any safe writer.
pub struct Display {
    owner: Arc<()>,
    step: u64,
    kind: DisplayKind,
    left: Prompt,
    right: Option<Prompt>,
}

/// A typed host effect tied to one live driver step.
pub struct Pending<E: Effect> {
    owner: Arc<()>,
    step: u64,
    suspension: Suspension<E>,
}

impl<E: Effect> Pending<E> {
    /// Inspect the owned request while editor and driver are unborrowed.
    #[must_use]
    pub fn request(&self) -> &E {
        self.suspension.request()
    }
}

/// The next operation required to continue a native read.
pub enum ReadStep {
    /// Produce a left or right prompt.
    Prompt(Pending<PromptEffect>),
    /// Report the current terminal dimensions.
    Resize(Pending<ResizeEffect>),
    /// Supply bytes, a decoded compatibility unit, EOF, timeout, or signal.
    Read(Pending<ReadEffect>),
    /// Navigate the host's independent history cursor.
    History(Pending<HistoryNavigateEffect>),
    /// Search host-owned history using an owned logical pattern.
    HistorySearch(Pending<HistorySearchEffect>),
    /// Select one exact host history record.
    HistoryLine(Pending<HistoryLineEffect>),
    /// Select one word from the newest host history record.
    HistoryWord(Pending<HistoryWordEffect>),
    /// Expand an owned alias selector.
    Alias(Pending<AliasEffect>),
    /// Collect and execute one editor configuration command.
    EditorCommand(Pending<EditorCommandEffect>),
    /// Edit an owned line using a host facility.
    ExternalEdit(Pending<ExternalEditEffect>),
    /// Retain a line after it is accepted.
    RecordHistory(Pending<HistoryRecordEffect>),
    /// Complete the snapshot-bound token at the cursor.
    Completion(Pending<CompletionEffect>),
    /// Run a registered host command with owned logical arguments.
    UserCommand(Pending<UserCommandEffect>),
    /// Propagate a signal after the terminal has entered cooked mode.
    Signal(Pending<SignalEffect>),
    /// Emit terminal output through a caller-supplied safe writer.
    Display(Display),
    /// The read invocation is complete and the driver is reusable.
    Complete(ReadResult),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectKind {
    PromptLeft,
    PromptRight,
    Resize,
    Read,
    History,
    HistorySearch,
    HistoryLine,
    HistoryWord,
    Alias,
    EditorCommand,
    ExternalEdit,
    RecordHistory,
    Completion,
    UserCommand,
    Signal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Idle,
    Advancing,
    Effect {
        step: u64,
        kind: EffectKind,
    },
    Display {
        step: u64,
    },
}

#[derive(Debug, Clone)]
struct Repetition {
    action: Action,
    invoking: TextUnit,
    remaining: usize,
}

enum OwnedLookup {
    Exact(Binding),
    Ambiguous(Binding),
    Prefix,
    Unbound,
}

// [spec:nshedit:req:core.read-driver]
/// Reusable native driver for one editor, with no stored streams or callbacks.
pub struct ReadDriver {
    owner: Arc<()>,
    next_step: u64,
    phase: Phase,
    tokenizer: Tokenizer,
    work_limit: usize,
    decoder: Decoder,
    replay: VecDeque<TextUnit>,
    key_sequence: Text,
    ambiguous: Option<(Binding, usize, TextUnit)>,
    repeat_argument: Option<sequence::RepeatArgument>,
    repetition: Option<Repetition>,
    pending_unit: Option<sequence::PendingUnit>,
    pending_operator: Option<sequence::PendingOperator>,
    alias_selector_pending: bool,
    after_history_line: Option<command_effect::AfterHistoryLine>,
    meta_next: bool,
    last_character_search: Option<sequence::StoredCharacterSearch>,
    last_history_search: Option<command_effect::StoredHistorySearch>,
    change_recording: Option<sequence::ChangeRecording>,
    last_change: Option<sequence::ChangeReplay>,
    semantic_replay: VecDeque<sequence::ReplayStep>,
    replaying_change: bool,
    expanded_units: usize,
    eof_pending: bool,
    signal_after_resize: Option<Signal>,
    pending_beeps: usize,
    left_prompt: Prompt,
    right_prompt: Option<Prompt>,
    display_kind: DisplayKind,
    live_line: Option<Text>,
    completion: Option<ReadResult>,
}

impl Default for ReadDriver {
    fn default() -> Self {
        Self::new(Tokenizer::default())
    }
}

impl ReadDriver {
    /// Build a driver with an owned tokenization policy.
    #[must_use]
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self {
            owner: Arc::new(()),
            next_step: 0,
            phase: Phase::Idle,
            tokenizer,
            work_limit: 4096,
            decoder: Decoder::default(),
            replay: VecDeque::new(),
            key_sequence: Text::default(),
            ambiguous: None,
            repeat_argument: None,
            repetition: None,
            pending_unit: None,
            pending_operator: None,
            alias_selector_pending: false,
            after_history_line: None,
            meta_next: false,
            last_character_search: None,
            last_history_search: None,
            change_recording: None,
            last_change: None,
            semantic_replay: VecDeque::new(),
            replaying_change: false,
            expanded_units: 0,
            eof_pending: false,
            signal_after_resize: None,
            pending_beeps: 0,
            left_prompt: Prompt::default(),
            right_prompt: None,
            display_kind: DisplayKind::Refresh,
            live_line: None,
            completion: None,
        }
    }

    /// Bound repeat counts and recursively replayed macro units.
    #[must_use]
    pub fn with_work_limit(mut self, limit: NonZeroUsize) -> Self {
        self.work_limit = limit.get();
        self
    }

    /// Start reading the editor's current line.
    pub fn begin<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        if self.phase != Phase::Idle {
            return Err(DriverError::Busy);
        }
        if editor.terminal_profile().is_none() {
            let error = DriverError::Render(RenderError::DisplayNotConfigured);
            return Err(self.fail(editor, error));
        }
        editor
            .set_terminal_mode(TerminalMode::Editing)
            .map_err(DriverError::Terminal)?;
        self.clear_transient(editor.config().buffering() != Buffering::Line);
        self.phase = Phase::Advancing;
        self.pending(editor, ResizeEffect::Prepare, EffectKind::Resize)
            .map(ReadStep::Resize)
    }

    /// Resume either prompt request.
    pub fn resume_prompt<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<PromptEffect>,
        response: EffectResult<Prompt>,
    ) -> Result<ReadStep, DriverError> {
        let kind = match pending.request().side {
            PromptSide::Left => EffectKind::PromptLeft,
            PromptSide::Right => EffectKind::PromptRight,
        };
        let response = self.accept(editor, pending, kind, response)?;
        let prompt = match response {
            Ok(prompt) => prompt,
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => Prompt::default(),
            Err(HostFailure::Interrupted) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host));
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        };

        match kind {
            EffectKind::PromptLeft => {
                self.left_prompt = prompt;
                self.request_prompt(editor, PromptSide::Right)
            }
            EffectKind::PromptRight => {
                self.right_prompt = (!prompt.is_empty()).then_some(prompt);
                self.make_display()
            }
            _ => unreachable!("the effect kind came from a prompt side"),
        }
    }

    /// Perform one display step, then continue input processing.
    pub fn display<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        display: &Display,
        output: &mut dyn Write,
    ) -> Result<ReadStep, DriverError> {
        self.validate_display(display)?;
        let emitted = match display.kind {
            DisplayKind::Refresh => editor
                .render_to(&display.left, display.right.as_ref(), output)
                .map(|_| ()),
            DisplayKind::Beep => editor.beep(output).map(|_| ()),
            DisplayKind::RefreshAndBeep(count) => (|| -> Result<(), RenderError> {
                editor.render_to(&display.left, display.right.as_ref(), output)?;
                for _ in 0..count {
                    editor.beep(output)?;
                }
                Ok(())
            })(),
            DisplayKind::FinishLine => editor.renderer.finish_line(output).map(|_| ()),
            DisplayKind::Echo(unit) => write_echo(unit, output),
        };
        if let Err(error) = emitted {
            return Err(self.fail(editor, DriverError::Render(error)));
        }
        self.phase = Phase::Advancing;
        self.advance(editor)
    }

    /// Resume a host input request.
    pub fn resume_read<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<ReadEffect>,
        response: EffectResult<ReadOutcome>,
    ) -> Result<ReadStep, DriverError> {
        let purpose = *pending.request();
        let response = self.accept(editor, pending, EffectKind::Read, response)?;
        let outcome = match response {
            Ok(outcome) => outcome,
            Err(HostFailure::Cancelled) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Cancelled));
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host));
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        };

        match outcome {
            ReadOutcome::Bytes(bytes) => {
                self.expanded_units = 0;
                self.decoder.push(&bytes);
                self.advance(editor)
            }
            ReadOutcome::Unit(unit) => {
                self.expanded_units = 0;
                self.process_unit(editor, unit)
            }
            ReadOutcome::Signal(signal) => self.handle_signal(editor, signal),
            ReadOutcome::TimedOut if purpose == ReadEffect::KeySequence => {
                self.handle_timeout(editor)
            }
            ReadOutcome::TimedOut => Err(self.fail(editor, DriverError::UnexpectedTimeout)),
            ReadOutcome::EndOfInput => {
                self.decoder.finish();
                self.eof_pending = true;
                self.advance(editor)
            }
        }
    }

    /// Resume history navigation and atomically replace the line when needed.
    pub fn resume_history<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<HistoryNavigateEffect>,
        response: <HistoryNavigateEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::History, response)?;
        let notify_boundary = self.repetition.is_none();
        match response {
            Ok(response) => {
                self.apply_history_response(editor, &response, notify_boundary)?;
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host));
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.after_host_action(editor)
    }

    /// Resume snapshot-bound completion.
    pub fn resume_completion<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<CompletionEffect>,
        response: <CompletionEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let query = pending.request().query.clone();
        let response = self.accept(editor, pending, EffectKind::Completion, response)?;
        match response {
            Ok(candidates) => {
                let outcome = editor
                    .apply_completion(&query, candidates)
                    .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                self.queue_beep_if(!matches!(outcome, CompletionOutcome::Unique { .. }));
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
            }
            Err(HostFailure::Interrupted) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host));
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.after_host_action(editor)
    }

    /// Resume a registered user command.
    pub fn resume_user_command<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<UserCommandEffect>,
        response: <UserCommandEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::UserCommand, response)?;
        match response {
            Ok(outcome) => self.after_outcome(editor, outcome),
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.queue_beep();
                self.after_host_action(editor)
            }
            Err(HostFailure::Interrupted) => {
                self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host))
            }
            Err(error) => Err(self.fail(editor, DriverError::Host(error))),
        }
    }

    /// Resume accepted-line history recording.
    pub fn resume_history_record<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<HistoryRecordEffect>,
        response: <HistoryRecordEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let line = pending.request().line.clone();
        let response = self.accept(editor, pending, EffectKind::RecordHistory, response)?;
        match response {
            Ok(()) | Err(HostFailure::Unavailable) => {
                self.completion = Some(ReadResult::Accepted(line));
                self.schedule_display(editor, DisplayKind::FinishLine)
            }
            Err(error) => Err(self.fail(editor, DriverError::Host(error))),
        }
    }

    fn advance<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        if let Some(result) = self.completion.take() {
            return self.complete(editor, result);
        }
        if let Some(step) = self.advance_semantic_replay(editor) {
            return step;
        }
        if let Some(unit) = self.replay.pop_front().or_else(|| self.decoder.pop()) {
            return self.process_unit(editor, unit);
        }
        if self.eof_pending {
            return self.handle_eof(editor);
        }
        let effect = if self.key_sequence.is_empty() {
            ReadEffect::Input
        } else {
            ReadEffect::KeySequence
        };
        self.pending(editor, effect, EffectKind::Read)
            .map(ReadStep::Read)
    }

    fn process_unit<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        unit: TextUnit,
    ) -> Result<ReadStep, DriverError> {
        if editor.config().buffering() == Buffering::Character {
            return self.complete(editor, ReadResult::Character(unit));
        }
        if let Some(step) = self.consume_pending_effect_input(editor, unit) {
            return step;
        }
        if let Some(step) = self.consume_pending_unit(editor, unit) {
            return step;
        }
        let unit = self.take_meta_unit(unit);
        match self.capture_count(editor.keymap_mode(), unit) {
            Ok(true) => return self.advance(editor),
            Ok(false) => {}
            Err(error) => return Err(self.fail(editor, error)),
        }

        self.key_sequence.push(unit);
        let sequence = KeySequence::new(self.key_sequence.clone())
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let lookup = match editor.key_binding(&sequence) {
            KeyLookup::Exact(binding) => OwnedLookup::Exact(binding.clone()),
            KeyLookup::Ambiguous(binding) => OwnedLookup::Ambiguous(binding.clone()),
            KeyLookup::Prefix => OwnedLookup::Prefix,
            KeyLookup::Unbound => OwnedLookup::Unbound,
        };
        match lookup {
            OwnedLookup::Exact(binding) => {
                self.ambiguous = None;
                self.dispatch_binding(editor, binding, unit)
            }
            OwnedLookup::Ambiguous(binding) => {
                self.ambiguous = Some((binding, self.key_sequence.len(), unit));
                self.advance(editor)
            }
            OwnedLookup::Prefix => self.advance(editor),
            OwnedLookup::Unbound => self.handle_unbound(editor, unit),
        }
    }

    fn handle_unbound<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        unit: TextUnit,
    ) -> Result<ReadStep, DriverError> {
        if let Some((binding, prefix_len, invoking)) = self.ambiguous.take() {
            let suffix: Vec<_> = self.key_sequence.as_units()[prefix_len..].to_vec();
            self.key_sequence.clear();
            for unit in suffix.into_iter().rev() {
                self.replay.push_front(unit);
            }
            return self.dispatch_binding(editor, binding, invoking);
        }

        let insert = self.key_sequence.len() == 1
            && editor.keymap_mode() != KeymapMode::ViCommand
            && match unit {
                TextUnit::Scalar(character) => !character.is_control(),
                TextUnit::RawByte(_) | TextUnit::CompatibilityWide(_) => true,
            };
        self.key_sequence.clear();
        if insert {
            let repeat = self.take_repeat(None);
            let action = Action::Insert(std::iter::once(unit).collect());
            self.prepare_action_recording(editor, &action, unit, repeat)?;
            self.dispatch_action(editor, action, unit, repeat)
        } else {
            self.repeat_argument = None;
            self.complete_command_after_display(editor);
            self.schedule_display(editor, DisplayKind::Beep)
        }
    }

    fn handle_timeout<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        self.key_sequence.clear();
        if let Some((binding, _, invoking)) = self.ambiguous.take() {
            self.dispatch_binding(editor, binding, invoking)
        } else {
            self.repeat_argument = None;
            self.complete_command_after_display(editor);
            self.schedule_display(editor, DisplayKind::Beep)
        }
    }

    fn handle_eof<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        if let Some((binding, _, invoking)) = self.ambiguous.take() {
            self.key_sequence.clear();
            return self.dispatch_binding(editor, binding, invoking);
        }
        self.key_sequence.clear();
        self.repeat_argument = None;
        self.cancel_pending_sequence(editor)?;
        self.cancel_pending_effect_command();
        self.complete(editor, ReadResult::EndOfInput)
    }

    fn dispatch_binding<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        binding: Binding,
        invoking: TextUnit,
    ) -> Result<ReadStep, DriverError> {
        self.key_sequence.clear();
        self.ambiguous = None;
        self.dispatch_resolved_binding(editor, binding, invoking, None)
    }

    fn dispatch_resolved_binding<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        binding: Binding,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        match binding {
            Binding::Action(action @ Action::Move(_)) if self.pending_operator.is_some() => {
                self.dispatch_operator_action(editor, action, invoking, explicit_repeat)
            }
            Binding::Action(action) => {
                let repeat = self.take_repeat(explicit_repeat);
                self.prepare_action_recording(editor, &action, invoking, repeat)?;
                self.dispatch_action(editor, action, invoking, repeat)
            }
            Binding::Sequence(sequence) => {
                self.dispatch_sequence(editor, sequence, invoking, explicit_repeat)
            }
            Binding::Effect(command) => {
                self.dispatch_effect_command(editor, command, invoking, explicit_repeat)
            }
            Binding::Immediate(command) => {
                self.dispatch_immediate(editor, command, invoking, explicit_repeat)
            }
            Binding::User(name) => {
                self.dispatch_user_command(editor, name, invoking, explicit_repeat)
            }
            Binding::Macro(text) => {
                let repeat = self.take_repeat(explicit_repeat);
                self.expand_macro(text, repeat)
                    .map_err(|error| self.fail(editor, error))?;
                self.advance(editor)
            }
        }
    }

    fn dispatch_user_command<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        name: crate::domain::CommandName,
        invoking: TextUnit,
        explicit_repeat: Option<usize>,
    ) -> Result<ReadStep, DriverError> {
        self.take_repeat(explicit_repeat);
        let parsed = self
            .tokenizer
            .tokenize(editor.line(), editor.cursor())
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let arguments = parsed
            .line()
            .tokens()
            .iter()
            .map(|token| token.value().clone())
            .collect();
        self.pending(
            editor,
            UserCommandEffect {
                name,
                invoking,
                arguments,
            },
            EffectKind::UserCommand,
        )
        .map(ReadStep::UserCommand)
    }

    fn expand_macro(&mut self, text: Text, repeat: usize) -> Result<(), DriverError> {
        let added = text
            .len()
            .checked_mul(repeat)
            .and_then(|added| self.expanded_units.checked_add(added))
            .filter(|total| *total <= self.work_limit)
            .ok_or(DriverError::WorkLimitExceeded {
                limit: self.work_limit,
            })?;
        let mut units = Vec::with_capacity(text.len().saturating_mul(repeat));
        for _ in 0..repeat {
            units.extend_from_slice(text.as_units());
        }
        for unit in units.into_iter().rev() {
            self.replay.push_front(unit);
        }
        self.expanded_units = added;
        Ok(())
    }

    fn dispatch_action<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        action: Action,
        invoking: TextUnit,
        repeat: usize,
    ) -> Result<ReadStep, DriverError> {
        if repeat == 0 {
            return self.schedule_command_display(editor);
        }
        if let Action::Move(motion) = action {
            return self.dispatch_counted_move(editor, motion, repeat);
        }
        if let Action::Delete(EditTarget::Character(direction)) = action {
            return self.dispatch_counted_character_delete(editor, direction, repeat);
        }
        if let Action::History(direction) = action {
            self.live_line.get_or_insert_with(|| editor.line().clone());
            return self
                .pending(
                    editor,
                    HistoryNavigateEffect {
                        direction,
                        count: RepeatCount::new(repeat)
                            .expect("dispatch rejects a zero repeat before effect creation"),
                    },
                    EffectKind::History,
                )
                .map(ReadStep::History);
        }
        if action == Action::TransposeCharacters {
            return self.dispatch_transpose(editor);
        }
        let repeat = if action_repeats_with_count(&action) {
            repeat
        } else {
            1
        };
        self.repetition = Some(Repetition {
            action,
            invoking,
            remaining: repeat,
        });
        self.continue_repetition(editor)
    }

    fn continue_repetition<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        loop {
            let mut repetition = self
                .repetition
                .take()
                .expect("dispatch installs a positive repetition");
            let action = repetition.action.clone();
            let refreshes_cursor = action_refreshes_cursor(&action);
            let invoking = repetition.invoking;
            let echoes_end_of_input = matches!(&action, Action::DeleteOrEndOfInput);
            repetition.remaining -= 1;
            if repetition.remaining != 0 {
                self.repetition = Some(repetition);
            }
            let step = editor
                .execute(action)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            match step {
                super::CommandStep::Applied(outcome) => {
                    if echoes_end_of_input && outcome == Outcome::EndOfInput {
                        self.repetition = None;
                        self.completion = Some(ReadResult::EndOfInput);
                        return self.schedule_display(editor, DisplayKind::Echo(invoking));
                    }
                    if matches!(outcome, Outcome::Accepted(_) | Outcome::EndOfInput) {
                        self.repetition = None;
                        return self.after_outcome(editor, outcome);
                    }
                    self.note_outcome(editor, outcome);
                    if self.repetition.is_none() {
                        if refreshes_cursor {
                            self.clamp_vi_command_cursor(editor)?;
                        }
                        return self.schedule_command_display(editor);
                    }
                }
                super::CommandStep::NeedsCompletion => {
                    let query = editor
                        .completion_query(&self.tokenizer)
                        .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                    return self
                        .pending(editor, CompletionEffect { query }, EffectKind::Completion)
                        .map(ReadStep::Completion);
                }
                super::CommandStep::NeedsHistory(direction) => {
                    self.live_line.get_or_insert_with(|| editor.line().clone());
                    return self
                        .pending(
                            editor,
                            HistoryNavigateEffect {
                                direction,
                                count: RepeatCount::ONE,
                            },
                            EffectKind::History,
                        )
                        .map(ReadStep::History);
                }
            }
        }
    }

    fn dispatch_counted_move<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        motion: Motion,
        repeat: usize,
    ) -> Result<ReadStep, DriverError> {
        let origin = editor.cursor();
        let vi_right_boundary = editor.keymap_mode() == KeymapMode::ViCommand
            && matches!(
                motion,
                Motion::Character(Direction::Next)
                    | Motion::Word {
                        direction: Direction::Next,
                        ..
                    }
            )
            && !editor.line().is_empty()
            && origin.get() >= editor.line().len().saturating_sub(1);
        let destination = if vi_right_boundary {
            origin
        } else {
            editor
                .motion_destination(motion, repeat)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?
        };
        if destination == origin
            && matches!(
                motion,
                Motion::Character(_) | Motion::Word { .. } | Motion::Line(_)
            )
        {
            self.queue_beep();
        } else {
            let step = editor
                .execute(Action::Move(Motion::Absolute(destination)))
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            let super::CommandStep::Applied(outcome) = step else {
                return Err(self.fail(editor, DriverError::InvalidSequenceState));
            };
            self.note_outcome(editor, outcome);
        }
        self.clamp_vi_command_cursor(editor)?;
        self.schedule_command_display(editor)
    }

    fn dispatch_counted_character_delete<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        direction: Direction,
        repeat: usize,
    ) -> Result<ReadStep, DriverError> {
        let origin = editor.cursor();
        let destination = editor
            .motion_destination(Motion::Character(direction), repeat)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        if destination == origin {
            self.queue_beep();
        } else {
            let range = match direction {
                Direction::Previous => destination.get()..origin.get(),
                Direction::Next => origin.get()..destination.get(),
            };
            let span = editor
                .line()
                .span(range)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            let step = editor
                .execute(Action::Delete(EditTarget::Span(span)))
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            let super::CommandStep::Applied(outcome) = step else {
                return Err(self.fail(editor, DriverError::InvalidSequenceState));
            };
            self.note_outcome(editor, outcome);
        }
        self.clamp_vi_command_cursor(editor)?;
        self.schedule_command_display(editor)
    }

    fn dispatch_transpose<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        if editor.line().len() < 2 {
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        if editor.cursor().get() == 0 {
            let destination = editor
                .line()
                .index(1)
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            editor
                .execute(Action::Move(Motion::Absolute(destination)))
                .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
            self.queue_beep();
            return self.schedule_command_display(editor);
        }
        let step = editor
            .execute(Action::TransposeCharacters)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        let super::CommandStep::Applied(outcome) = step else {
            return Err(self.fail(editor, DriverError::InvalidSequenceState));
        };
        self.note_outcome(editor, outcome);
        self.clamp_vi_command_cursor(editor)?;
        self.schedule_command_display(editor)
    }

    fn clamp_vi_command_cursor<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<(), DriverError> {
        if editor.keymap_mode() != KeymapMode::ViCommand
            || editor.line().is_empty()
            || editor.cursor().get() < editor.line().len()
        {
            return Ok(());
        }
        let destination = editor
            .line()
            .index(editor.line().len() - 1)
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        editor
            .execute(Action::Move(Motion::Absolute(destination)))
            .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
        Ok(())
    }

    fn after_outcome<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        outcome: Outcome,
    ) -> Result<ReadStep, DriverError> {
        match outcome {
            Outcome::Accepted(line) => {
                self.repetition = None;
                self.pending(
                    editor,
                    HistoryRecordEffect { line },
                    EffectKind::RecordHistory,
                )
                .map(ReadStep::RecordHistory)
            }
            Outcome::EndOfInput => {
                self.repetition = None;
                self.complete(editor, ReadResult::EndOfInput)
            }
            other => {
                self.note_outcome(editor, other);
                self.after_host_action(editor)
            }
        }
    }

    fn queue_beep(&mut self) {
        self.pending_beeps = self.pending_beeps.saturating_add(1);
    }

    fn queue_beep_if(&mut self, condition: bool) {
        if condition {
            self.queue_beep();
        }
    }

    fn note_outcome<T: TerminalControl>(&mut self, editor: &mut Editor<T>, outcome: Outcome) {
        match outcome {
            Outcome::Refresh(Refresh::Beep) => self.queue_beep(),
            Outcome::Refresh(Refresh::Redraw) => editor.request_redraw(),
            Outcome::Refresh(Refresh::Full | Refresh::Redisplay) => editor.invalidate_display(),
            _ => {}
        }
    }

    fn after_host_action<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        if self.repetition.is_some() {
            self.continue_repetition(editor)
        } else {
            self.schedule_command_display(editor)
        }
    }

    fn schedule_command_display<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
    ) -> Result<ReadStep, DriverError> {
        self.pending_operator = None;
        self.finish_change_if_complete(editor);
        self.complete_command_after_display(editor);
        let beeps = std::mem::take(&mut self.pending_beeps);
        let kind = if beeps != 0 {
            DisplayKind::RefreshAndBeep(beeps)
        } else {
            DisplayKind::Refresh
        };
        self.schedule_display(editor, kind)
    }

    fn complete_command_after_display<T: TerminalControl>(&mut self, editor: &Editor<T>) {
        if editor.config().buffering() == Buffering::Command && self.completion.is_none() {
            self.completion = Some(ReadResult::Command);
        }
    }

    fn schedule_display<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        kind: DisplayKind,
    ) -> Result<ReadStep, DriverError> {
        self.display_kind = kind;
        if matches!(
            kind,
            DisplayKind::Beep | DisplayKind::FinishLine | DisplayKind::Echo(_)
        ) {
            self.left_prompt = Prompt::default();
            self.right_prompt = None;
            self.make_display()
        } else {
            self.request_prompt(editor, PromptSide::Left)
        }
    }

    fn make_display(&mut self) -> Result<ReadStep, DriverError> {
        let step = self.issue_step()?;
        self.phase = Phase::Display { step };
        Ok(ReadStep::Display(Display {
            owner: Arc::clone(&self.owner),
            step,
            kind: self.display_kind,
            left: self.left_prompt.clone(),
            right: self.right_prompt.clone(),
        }))
    }

    fn request_prompt<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        side: PromptSide,
    ) -> Result<ReadStep, DriverError> {
        let kind = match side {
            PromptSide::Left => EffectKind::PromptLeft,
            PromptSide::Right => EffectKind::PromptRight,
        };
        self.pending(editor, PromptEffect { side }, kind)
            .map(ReadStep::Prompt)
    }

    fn pending<T: TerminalControl, E: Effect>(
        &mut self,
        editor: &mut Editor<T>,
        effect: E,
        kind: EffectKind,
    ) -> Result<Pending<E>, DriverError> {
        let step = self.issue_step()?;
        let suspension = match editor.suspend(effect) {
            Ok(suspension) => suspension,
            Err(error) => {
                self.phase = Phase::Idle;
                return Err(DriverError::Effect(error));
            }
        };
        self.phase = Phase::Effect { step, kind };
        Ok(Pending {
            owner: Arc::clone(&self.owner),
            step,
            suspension,
        })
    }

    fn accept<T: TerminalControl, E: Effect>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<E>,
        kind: EffectKind,
        response: E::Response,
    ) -> Result<E::Response, DriverError> {
        if !Arc::ptr_eq(&self.owner, &pending.owner) {
            return Err(DriverError::DifferentDriver);
        }
        if self.phase
            != (Phase::Effect {
                step: pending.step,
                kind,
            })
        {
            return Err(DriverError::StaleStep);
        }
        let response = editor
            .resume(&pending.suspension, response)
            .map_err(DriverError::Effect)?;
        self.phase = Phase::Advancing;
        Ok(response)
    }

    fn validate_display(&self, display: &Display) -> Result<(), DriverError> {
        if !Arc::ptr_eq(&self.owner, &display.owner) {
            return Err(DriverError::DifferentDriver);
        }
        if self.phase != (Phase::Display { step: display.step }) {
            return Err(DriverError::StaleStep);
        }
        Ok(())
    }

    fn issue_step(&mut self) -> Result<u64, DriverError> {
        let Some(step) = self.next_step.checked_add(1) else {
            self.phase = Phase::Idle;
            self.clear_transient(false);
            return Err(DriverError::SequenceExhausted);
        };
        self.next_step = step;
        Ok(step)
    }

    fn complete<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        result: ReadResult,
    ) -> Result<ReadStep, DriverError> {
        self.finish_change(editor);
        editor.finish_all_edit_groups();
        let preserve_input = editor.config().buffering() != Buffering::Line;
        if !preserve_input {
            if let Err(error) = editor.set_terminal_mode(TerminalMode::Cooked) {
                self.phase = Phase::Idle;
                self.clear_transient(false);
                return Err(DriverError::Terminal(error));
            }
        }
        self.phase = Phase::Idle;
        self.clear_transient(preserve_input);
        Ok(ReadStep::Complete(result))
    }

    fn fail<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        error: DriverError,
    ) -> DriverError {
        self.finish_change(editor);
        editor.finish_all_edit_groups();
        self.phase = Phase::Idle;
        self.clear_transient(false);
        let _ = editor.set_terminal_mode(TerminalMode::Cooked);
        error
    }

    fn clear_transient(&mut self, preserve_input: bool) {
        if !preserve_input {
            self.decoder.clear();
            self.eof_pending = false;
            self.replay.clear();
        }
        self.key_sequence.clear();
        self.ambiguous = None;
        self.repeat_argument = None;
        self.repetition = None;
        self.pending_unit = None;
        self.pending_operator = None;
        self.alias_selector_pending = false;
        self.after_history_line = None;
        self.meta_next = false;
        self.change_recording = None;
        self.semantic_replay.clear();
        self.replaying_change = false;
        self.expanded_units = 0;
        self.signal_after_resize = None;
        self.pending_beeps = 0;
        self.left_prompt = Prompt::default();
        self.right_prompt = None;
        self.live_line = None;
        self.completion = None;
    }
}

fn write_echo(unit: TextUnit, output: &mut dyn Write) -> Result<(), RenderError> {
    match unit {
        TextUnit::Scalar(character) => {
            write_visual_scalar(character, output)?;
        }
        TextUnit::RawByte(byte) => {
            if byte.is_ascii_control() {
                let visual = if byte == 0x7f { b'?' } else { byte | 0x40 };
                output.write_all(&[b'^', visual])?;
            } else {
                output.write_all(&[byte])?;
            }
        }
        TextUnit::CompatibilityWide(_) => output.write_all("\u{fffd}".as_bytes())?,
    }
    output.flush()?;
    Ok(())
}

fn action_refreshes_cursor(action: &Action) -> bool {
    matches!(
        action,
        Action::Delete(_)
            | Action::DeleteOrEndOfInput
            | Action::Kill(_)
            | Action::Yank(_)
            | Action::ExchangeMark
            | Action::Transform { .. }
            | Action::TransposeCharacters
            | Action::Undo
            | Action::Redo
            | Action::Refresh(_)
    )
}

fn write_visual_scalar(character: char, output: &mut dyn Write) -> Result<(), RenderError> {
    let scalar = character as u32;
    if scalar <= 0xff && character.is_control() {
        output.write_all(b"^")?;
        let visual = if scalar == 0x7f {
            '?'
        } else {
            char::from_u32(scalar | 0x40).unwrap_or('\u{fffd}')
        };
        let mut encoded = [0; 4];
        output.write_all(visual.encode_utf8(&mut encoded).as_bytes())?;
    } else {
        let mut encoded = [0; 4];
        output.write_all(character.encode_utf8(&mut encoded).as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
