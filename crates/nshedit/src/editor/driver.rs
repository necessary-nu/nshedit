//! Resumable input preparation, decoding, dispatch, and completion.

mod decode;
mod error;
mod sequence;

use std::collections::VecDeque;
use std::io::Write;
use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::domain::{
    Action, Binding, Buffering, KeyLookup, KeySequence, KeymapMode, Outcome, Prompt, Refresh,
    Signal, SignalPolicy, TerminalMode, Text, TextUnit,
};

use super::effect::{
    CompletionEffect, Effect, EffectResult, HistoryNavigateEffect, HistoryRecordEffect,
    HistorySelection, HostFailure, PromptEffect, PromptSide, ReadEffect, ReadOutcome, ResizeEffect,
    SignalEffect, Suspension, UserCommandEffect,
};
use super::{CompletionOutcome, Editor, RenderError, TerminalControl, Tokenizer};
use decode::Decoder;
pub use error::DriverError;

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
    RefreshAndBeep,
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
    meta_next: bool,
    last_character_search: Option<sequence::StoredCharacterSearch>,
    change_recording: Option<sequence::ChangeRecording>,
    last_change: Option<sequence::ChangeReplay>,
    semantic_replay: VecDeque<sequence::ReplayStep>,
    replaying_change: bool,
    expanded_units: usize,
    eof_pending: bool,
    beep_pending: bool,
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
            meta_next: false,
            last_character_search: None,
            change_recording: None,
            last_change: None,
            semantic_replay: VecDeque::new(),
            replaying_change: false,
            expanded_units: 0,
            eof_pending: false,
            beep_pending: false,
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
        self.clear_transient(editor.config().buffering() == Buffering::Character);
        self.phase = Phase::Advancing;
        self.pending(editor, ResizeEffect, EffectKind::Resize)
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

    /// Resume a terminal-size request and rebuild the display.
    pub fn resume_resize<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<ResizeEffect>,
        response: <ResizeEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let response = self.accept(editor, pending, EffectKind::Resize, response)?;
        match response {
            Ok(size) => editor
                .resize_display(size)
                .map_err(|error| self.fail(editor, DriverError::Render(error)))?,
            Err(HostFailure::Unavailable) => {}
            Err(HostFailure::Interrupted) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host));
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }
        self.schedule_display(editor, DisplayKind::Refresh)
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
            DisplayKind::RefreshAndBeep => editor
                .render_to(&display.left, display.right.as_ref(), output)
                .and_then(|_| editor.beep(output).map(|_| ())),
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
        match response {
            Ok(response) => {
                let redraw = !matches!(response.selection(), HistorySelection::Unchanged);
                let line = match response.selection() {
                    HistorySelection::Entry(line) => Some(line.clone()),
                    HistorySelection::Live => self.live_line.clone(),
                    HistorySelection::Unchanged => None,
                };
                if let Some(line) = line {
                    editor
                        .restore_history_line(line)
                        .map_err(|error| self.fail(editor, DriverError::Editor(error)))?;
                }
                if redraw {
                    editor.request_redraw();
                }
                self.beep_pending |= response.reached_boundary();
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
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
                self.beep_pending |= !matches!(outcome, CompletionOutcome::Unique { .. });
            }
            Err(HostFailure::Unavailable | HostFailure::Cancelled) => {
                self.beep_pending = true;
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
                self.beep_pending = true;
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

    /// Resume host signal propagation.
    pub fn resume_signal<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<SignalEffect>,
        response: <SignalEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let signal = pending.request().signal;
        let response = self.accept(editor, pending, EffectKind::Signal, response)?;
        if let Err(HostFailure::Failed(message)) = response {
            return Err(self.fail(editor, DriverError::Host(HostFailure::Failed(message))));
        }
        if signal == Signal::Suspend {
            editor
                .set_terminal_mode(TerminalMode::Editing)
                .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
            self.pending(editor, ResizeEffect, EffectKind::Resize)
                .map(ReadStep::Resize)
        } else {
            self.complete(
                editor,
                ReadResult::Interrupted(ReadInterrupt::Signal(signal)),
            )
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
        self.repeat_argument = None;
        if insert {
            let action = Action::Insert(std::iter::once(unit).collect());
            self.prepare_action_recording(editor, &action, unit, 1)?;
            self.dispatch_action(editor, action, unit, 1)
        } else {
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
            Binding::Action(action) if self.pending_operator.is_some() => {
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
            Binding::Macro(text) => {
                let repeat = self.take_repeat(explicit_repeat);
                self.expand_macro(text, repeat)
                    .map_err(|error| self.fail(editor, error))?;
                self.advance(editor)
            }
        }
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
        self.beep_pending = false;
        if repeat == 0 {
            return self.schedule_command_display(editor);
        }
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
                            HistoryNavigateEffect { direction },
                            EffectKind::History,
                        )
                        .map(ReadStep::History);
                }
                super::CommandStep::NeedsUserCommand(name) => {
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
                    return self
                        .pending(
                            editor,
                            UserCommandEffect {
                                name,
                                invoking,
                                arguments,
                            },
                            EffectKind::UserCommand,
                        )
                        .map(ReadStep::UserCommand);
                }
            }
        }
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

    fn note_outcome<T: TerminalControl>(&mut self, editor: &mut Editor<T>, outcome: Outcome) {
        match outcome {
            Outcome::Refresh(Refresh::Beep) => self.beep_pending = true,
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
        self.finish_change_if_complete(editor);
        let kind = if std::mem::take(&mut self.beep_pending) {
            DisplayKind::RefreshAndBeep
        } else {
            DisplayKind::Refresh
        };
        self.schedule_display(editor, kind)
    }

    fn handle_signal<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        signal: Signal,
    ) -> Result<ReadStep, DriverError> {
        if editor.config().signal_policy() == SignalPolicy::Ignore {
            return self.complete(
                editor,
                ReadResult::Interrupted(ReadInterrupt::Signal(signal)),
            );
        }
        match signal {
            Signal::Resize => self
                .pending(editor, ResizeEffect, EffectKind::Resize)
                .map(ReadStep::Resize),
            Signal::Continue => {
                editor
                    .set_terminal_mode(TerminalMode::Editing)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.pending(editor, ResizeEffect, EffectKind::Resize)
                    .map(ReadStep::Resize)
            }
            signal => {
                editor
                    .set_terminal_mode(TerminalMode::Cooked)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.pending(editor, SignalEffect { signal }, EffectKind::Signal)
                    .map(ReadStep::Signal)
            }
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
        let preserve_input = matches!(result, ReadResult::Character(_));
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
        }
        self.replay.clear();
        self.key_sequence.clear();
        self.ambiguous = None;
        self.repeat_argument = None;
        self.repetition = None;
        self.pending_unit = None;
        self.pending_operator = None;
        self.meta_next = false;
        self.change_recording = None;
        self.semantic_replay.clear();
        self.replaying_change = false;
        self.expanded_units = 0;
        self.beep_pending = false;
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
