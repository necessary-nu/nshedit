//! Typed suspension points for every host-controlled editor operation.
//!
//! A [`Suspension`] owns its request and contains no borrow of [`Editor`].
//! Host code can therefore run, including permitted reentrant operations,
//! before the driver borrows the editor again to [`Editor::resume`].

use std::ffi::OsString;
use std::fmt;
use std::sync::Arc;

use crate::domain::{CommandName, Direction, Outcome, Prompt, ScreenSize, Signal, Text, TextUnit};

use super::{CompletionCandidates, CompletionQuery, Editor, TerminalControl};

mod sealed {
    pub trait Sealed {}
}

// [spec:nshedit:req:core.effect-hooks]
/// A closed host request with one statically paired response type.
pub trait Effect: sealed::Sealed {
    /// The only response shape accepted when this request resumes.
    type Response;
}

/// Which prompt the editor needs rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PromptSide {
    /// Prompt preceding the editable line.
    Left,
    /// Prompt aligned at the right edge of the display.
    Right,
}

/// Ask the host to produce a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromptEffect {
    /// Which prompt the host must supply.
    pub side: PromptSide,
}

impl sealed::Sealed for PromptEffect {}

impl Effect for PromptEffect {
    type Response = EffectResult<Prompt>;
}

// [spec:nshedit:req:core.read-driver]
/// Ask the host for input, end of input, a signal, or a prefix timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReadEffect {
    /// Wait normally for fresh input.
    #[default]
    Input,
    /// Wait long enough to disambiguate a key-sequence prefix.
    KeySequence,
}

impl sealed::Sealed for ReadEffect {}

impl Effect for ReadEffect {
    type Response = EffectResult<ReadOutcome>;
}

/// A successful host read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReadOutcome {
    /// An owned byte chunk for the driver's incremental UTF-8 decoder.
    Bytes(Box<[u8]>),
    /// One unit already decoded by a compatibility boundary.
    Unit(TextUnit),
    /// A semantic signal observed by the host's safe platform layer.
    Signal(Signal),
    /// No continuation arrived before a key-prefix deadline.
    TimedOut,
    /// The input source ended without another byte or unit.
    EndOfInput,
}

/// Ask the host history to move in one direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HistoryNavigateEffect {
    /// Which neighbouring entry to request.
    pub direction: Direction,
}

impl sealed::Sealed for HistoryNavigateEffect {}

impl Effect for HistoryNavigateEffect {
    type Response = EffectResult<HistoryResponse>;
}

// [spec:nshedit:req:core.history+1]
/// The host's typed result of navigating native history.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HistoryResponse {
    /// Replace the edited line with this owned history entry.
    Entry(Text),
    /// Navigation moved past the newest entry to the saved live line.
    Live,
    /// No entry exists in the requested direction.
    Boundary,
}

/// Ask the host history to retain an accepted line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HistoryRecordEffect {
    /// Accepted logical line to retain.
    pub line: Text,
}

impl sealed::Sealed for HistoryRecordEffect {}

impl Effect for HistoryRecordEffect {
    type Response = EffectResult<()>;
}

/// Ask the host to expand one command alias.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AliasEffect {
    /// Command word whose alias is requested.
    pub name: CommandName,
}

impl sealed::Sealed for AliasEffect {}

impl Effect for AliasEffect {
    type Response = EffectResult<Option<Text>>;
}

/// Ask the host for the current terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ResizeEffect;

impl sealed::Sealed for ResizeEffect {}

impl Effect for ResizeEffect {
    type Response = EffectResult<ScreenSize>;
}

// [spec:nshedit:req:core.read-driver]
/// Ask the host to propagate a signal after the editor made the tty safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignalEffect {
    /// The semantic signal whose previous disposition should run.
    pub signal: Signal,
}

impl sealed::Sealed for SignalEffect {}

impl Effect for SignalEffect {
    type Response = EffectResult<()>;
}

/// Ask the host to complete the logical line at a checked cursor boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompletionEffect {
    /// Snapshot-bound completion input produced by the native tokenizer.
    pub query: CompletionQuery,
}

impl sealed::Sealed for CompletionEffect {}

impl Effect for CompletionEffect {
    type Response = EffectResult<CompletionCandidates>;
}

/// Ask the host to look up an environment value without assuming Unicode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnvironmentEffect {
    /// Environment key in the host's native string representation.
    pub name: OsString,
}

impl sealed::Sealed for EnvironmentEffect {}

impl Effect for EnvironmentEffect {
    type Response = EffectResult<Option<OsString>>;
}

/// Ask the host to run a registered command with owned arguments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserCommandEffect {
    /// Registered command to invoke.
    pub name: CommandName,
    /// Owned logical arguments supplied to the command.
    pub arguments: Vec<Text>,
}

impl sealed::Sealed for UserCommandEffect {}

impl Effect for UserCommandEffect {
    type Response = EffectResult<Outcome>;
}

/// A response from host-controlled work.
pub type EffectResult<T> = Result<T, HostFailure>;

/// Typed reasons a host operation did not produce its normal value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostFailure {
    /// The host intentionally cancelled the operation.
    Cancelled,
    /// A signal or equivalent interruption stopped the operation.
    Interrupted,
    /// The host does not provide this operation.
    Unavailable,
    /// The host operation failed with an owned diagnostic.
    Failed(Box<str>),
}

impl fmt::Display for HostFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("host operation was cancelled"),
            Self::Interrupted => formatter.write_str("host operation was interrupted"),
            Self::Unavailable => formatter.write_str("host operation is unavailable"),
            Self::Failed(message) => write!(formatter, "host operation failed: {message}"),
        }
    }
}

impl std::error::Error for HostFailure {}

/// Misuse or exhaustion of the editor's suspension state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectStateError {
    /// A second request was attempted before the first resumed.
    AlreadySuspended,
    /// A response was supplied when no request was waiting.
    NoSuspendedEffect,
    /// The suspension was issued by another editor instance.
    DifferentEditor,
    /// A newer request superseded this suspension sequence.
    StaleSuspension,
    /// No further unique sequence can be represented.
    SequenceExhausted,
}

impl fmt::Display for EffectStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadySuspended => {
                formatter.write_str("the editor already has a suspended effect")
            }
            Self::NoSuspendedEffect => formatter.write_str("the editor has no suspended effect"),
            Self::DifferentEditor => {
                formatter.write_str("the suspension belongs to another editor")
            }
            Self::StaleSuspension => formatter.write_str("the suspension is no longer current"),
            Self::SequenceExhausted => formatter.write_str("the effect sequence is exhausted"),
        }
    }
}

impl std::error::Error for EffectStateError {}

/// An owned host request that does not retain an editor borrow.
pub struct Suspension<E: Effect> {
    owner: Arc<()>,
    sequence: u64,
    request: E,
}

impl<E: Effect> Suspension<E> {
    /// Inspect the request while the editor remains independently borrowable.
    #[must_use]
    pub fn request(&self) -> &E {
        &self.request
    }
}

#[derive(Debug, Default)]
pub(super) struct Runtime {
    owner: Arc<()>,
    next_sequence: u64,
    pending_sequence: Option<u64>,
}

impl<T: TerminalControl> Editor<T> {
    /// Suspend one host request without retaining this mutable borrow.
    pub fn suspend<E: Effect>(&mut self, request: E) -> Result<Suspension<E>, EffectStateError> {
        if self.effects.pending_sequence.is_some() {
            return Err(EffectStateError::AlreadySuspended);
        }
        let Some(sequence) = self.effects.next_sequence.checked_add(1) else {
            return Err(EffectStateError::SequenceExhausted);
        };
        self.effects.next_sequence = sequence;
        self.effects.pending_sequence = Some(sequence);

        Ok(Suspension {
            owner: Arc::clone(&self.effects.owner),
            sequence,
            request,
        })
    }

    /// Validate a suspension and release its typed response to the driver.
    ///
    /// A rejected response does not clear the current suspension, so the
    /// caller can retry with the right editor or token.
    pub fn resume<E: Effect>(
        &mut self,
        suspension: &Suspension<E>,
        response: E::Response,
    ) -> Result<E::Response, EffectStateError> {
        if !Arc::ptr_eq(&self.effects.owner, &suspension.owner) {
            return Err(EffectStateError::DifferentEditor);
        }
        match self.effects.pending_sequence {
            None => Err(EffectStateError::NoSuspendedEffect),
            Some(sequence) if sequence != suspension.sequence => {
                Err(EffectStateError::StaleSuspension)
            }
            Some(_) => {
                self.effects.pending_sequence = None;
                Ok(response)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::EditorConfig;
    use std::io;

    struct TestTerminal;

    impl TerminalControl for TestTerminal {
        fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
            Ok(())
        }

        fn set_mode(&mut self, _mode: crate::domain::TerminalMode) -> io::Result<()> {
            Ok(())
        }

        fn restore(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn editor() -> Editor<TestTerminal> {
        Editor::new(EditorConfig::default(), TestTerminal).unwrap()
    }

    // [spec:nshedit:req:core.effect-hooks/test]
    #[test]
    fn suspension_releases_editor_borrow() {
        let mut editor = editor();
        let prompt = editor
            .suspend(PromptEffect {
                side: PromptSide::Left,
            })
            .unwrap();

        assert_eq!(prompt.request().side, PromptSide::Left);
        assert_eq!(editor.config(), EditorConfig::default());
        assert_eq!(
            editor.suspend(ReadEffect::default()).err(),
            Some(EffectStateError::AlreadySuspended)
        );

        let response = editor
            .resume(&prompt, Ok(Prompt::from("prompt> ")))
            .unwrap();
        assert_eq!(response, Ok(Prompt::from("prompt> ")));
        assert!(editor.suspend(ReadEffect::default()).is_ok());
    }

    #[test]
    fn wrong_editor_cannot_resume_effect() {
        let mut first = editor();
        let mut second = editor();
        let pending = first.suspend(ReadEffect::default()).unwrap();

        assert_eq!(
            second.resume(&pending, Ok(ReadOutcome::EndOfInput)),
            Err(EffectStateError::DifferentEditor)
        );
        assert_eq!(
            first.resume(&pending, Ok(ReadOutcome::EndOfInput)),
            Ok(Ok(ReadOutcome::EndOfInput))
        );
    }

    #[test]
    fn stale_suspension_cannot_replace_current() {
        let mut editor = editor();
        let old = editor.suspend(ReadEffect::default()).unwrap();
        assert_eq!(
            editor.resume(&old, Ok(ReadOutcome::EndOfInput)),
            Ok(Ok(ReadOutcome::EndOfInput))
        );
        let current = editor.suspend(ReadEffect::default()).unwrap();

        assert_eq!(
            editor.resume(&old, Ok(ReadOutcome::EndOfInput)),
            Err(EffectStateError::StaleSuspension)
        );
        assert_eq!(
            editor.resume(&current, Err(HostFailure::Cancelled)),
            Ok(Err(HostFailure::Cancelled))
        );
    }

    #[test]
    fn host_boundaries_have_typed_responses() {
        fn accepts<E: Effect>(_effect: E, _response: E::Response) {}

        accepts(
            PromptEffect {
                side: PromptSide::Right,
            },
            Ok(Prompt::from("prompt")),
        );
        accepts(
            ReadEffect::default(),
            Ok(ReadOutcome::Unit(TextUnit::Scalar('x'))),
        );
        accepts(
            HistoryNavigateEffect {
                direction: Direction::Previous,
            },
            Ok(HistoryResponse::Entry(Text::from("old"))),
        );
        accepts(
            HistoryNavigateEffect {
                direction: Direction::Next,
            },
            Ok(HistoryResponse::Live),
        );
        accepts(
            HistoryNavigateEffect {
                direction: Direction::Previous,
            },
            Ok(HistoryResponse::Boundary),
        );
        accepts(
            HistoryRecordEffect {
                line: Text::from("new"),
            },
            Ok(()),
        );
        accepts(
            AliasEffect {
                name: CommandName::new("ll").unwrap(),
            },
            Ok(Some(Text::from("ls -l"))),
        );
        accepts(ResizeEffect, Ok(ScreenSize::new(24, 80).unwrap()));
        accepts(
            SignalEffect {
                signal: Signal::Interrupt,
            },
            Ok(()),
        );
        let completion_editor = editor();
        let query = completion_editor
            .completion_query(&super::super::Tokenizer::default())
            .unwrap();
        accepts(
            CompletionEffect { query },
            Ok(vec![super::super::CompletionCandidate::new("echo")].into()),
        );
        accepts(
            EnvironmentEffect {
                name: OsString::from("TERM"),
            },
            Ok(Some(OsString::from("xterm"))),
        );
        accepts(
            UserCommandEffect {
                name: CommandName::new("transpose").unwrap(),
                arguments: vec![Text::from("word")],
            },
            Ok(Outcome::Continue),
        );
    }

    #[test]
    fn sequence_exhaustion_is_reported() {
        let mut editor = editor();
        editor.effects.next_sequence = u64::MAX;

        assert_eq!(
            editor.suspend(ReadEffect::default()).err(),
            Some(EffectStateError::SequenceExhausted)
        );
    }
}
