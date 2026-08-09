//! Raw-free completion transaction for the ABI adapter.
//!
//! The exported wrappers snapshot the editor, then pass scoped providers and
//! a short-lived apply closure here. Foreign work therefore runs without an
//! editor borrow, and the returned report owns every edit and presentation
//! effect.

use nshedit::domain::{Error, Text, TextUnit};
use nshedit::editor::{
    CompletionCandidate, CompletionCandidates, CompletionOutcome, CompletionQuery,
    Tokenizer as EditorTokenizer,
};

use crate::adapter::{CompletionInvocation, EditLine};

use super::{
    BREAK_CHARACTERS, FilenameCompletionState, collect_candidates, completion_suffix,
    filename_completion, format_match_list, text_bytes,
};

pub(crate) type CandidateGenerator<'a> = dyn FnMut(&str, usize) -> Option<String> + 'a;
pub(crate) type AttemptedProvider<'a> = dyn FnMut(&str, usize, usize) -> AttemptedCompletion + 'a;
pub(crate) type SuffixProvider<'a> = dyn FnMut(&str) -> String + 'a;
type CompletionApplier<'a> =
    dyn FnMut(&CompletionQuery, CompletionCandidates) -> Result<CompletionOutcome, Error> + 'a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptedFallback {
    Allow,
    Suppress,
}

pub(crate) struct AttemptedCompletion {
    candidates: Option<Vec<String>>,
    fallback: AttemptedFallback,
}

impl AttemptedCompletion {
    pub(crate) const fn new(candidates: Option<Vec<String>>, fallback: AttemptedFallback) -> Self {
        Self {
            candidates,
            fallback,
        }
    }
}

pub(crate) struct CompletionProviders<'a> {
    generator: Option<&'a mut CandidateGenerator<'a>>,
    attempted: Option<&'a mut AttemptedProvider<'a>>,
    suffix: Option<&'a mut SuffixProvider<'a>>,
}

impl<'a> CompletionProviders<'a> {
    pub(crate) const fn new(generator: Option<&'a mut CandidateGenerator<'a>>) -> Self {
        Self {
            generator,
            attempted: None,
            suffix: None,
        }
    }

    pub(crate) fn with_attempted(
        mut self,
        attempted: Option<&'a mut AttemptedProvider<'a>>,
    ) -> Self {
        self.attempted = attempted;
        self
    }

    pub(crate) fn with_suffix(mut self, suffix: Option<&'a mut SuffixProvider<'a>>) -> Self {
        self.suffix = suffix;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UniqueSuffix {
    Append,
    Omit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletionPolicy {
    listing_limit: usize,
    unique_suffix: UniqueSuffix,
}

impl CompletionPolicy {
    pub(crate) const fn new(listing_limit: usize, unique_suffix: UniqueSuffix) -> Self {
        Self {
            listing_limit,
            unique_suffix,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletionPositions {
    pub(crate) cursor: usize,
    pub(crate) line_end: usize,
}

pub(crate) struct CompletionSnapshot {
    query: Result<CompletionQuery, Error>,
    invocation: CompletionInvocation,
    positions: CompletionPositions,
    columns: usize,
}

pub(crate) fn observe_completion(editor: &mut EditLine, separators: Text) -> CompletionSnapshot {
    let invocation = editor.begin_completion();
    let positions = CompletionPositions {
        cursor: editor.editor().cursor().get(),
        line_end: editor.editor().line().len(),
    };
    let columns = editor.screen_size().map_or(80, |size| size.columns());
    let query = editor
        .editor()
        .completion_query(&EditorTokenizer::new(separators));
    CompletionSnapshot {
        query,
        invocation,
        positions,
        columns,
    }
}

impl CompletionSnapshot {
    pub(crate) const fn invocation(&self) -> CompletionInvocation {
        self.invocation
    }

    pub(crate) const fn positions(&self) -> CompletionPositions {
        self.positions
    }
}

pub(crate) struct CompletionRequest<'a> {
    snapshot: CompletionSnapshot,
    providers: CompletionProviders<'a>,
    policy: CompletionPolicy,
    apply: &'a mut CompletionApplier<'a>,
}

impl<'a> CompletionRequest<'a> {
    pub(crate) const fn new(
        snapshot: CompletionSnapshot,
        providers: CompletionProviders<'a>,
        policy: CompletionPolicy,
        apply: &'a mut CompletionApplier<'a>,
    ) -> Self {
        Self {
            snapshot,
            providers,
            policy,
            apply,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionCommand {
    Normal,
    Refresh,
    Redisplay,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionListing {
    Pending,
    Cleared,
    Presented,
    OmittedByLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptedState {
    Preserve,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionSignal {
    Silent,
    Beep,
}

pub(crate) struct CompletionReport {
    command: CompletionCommand,
    invocation: CompletionInvocation,
    positions: CompletionPositions,
    listing: CompletionListing,
    attempted: AttemptedState,
    outcome: Option<CompletionOutcome>,
    signal: CompletionSignal,
    output: Vec<u8>,
}

impl CompletionReport {
    pub(crate) const fn command(&self) -> CompletionCommand {
        self.command
    }

    pub(crate) const fn invocation(&self) -> CompletionInvocation {
        self.invocation
    }

    pub(crate) const fn positions(&self) -> CompletionPositions {
        self.positions
    }

    pub(crate) const fn attempted_state(&self) -> AttemptedState {
        self.attempted
    }

    #[cfg(test)]
    pub(crate) const fn listing(&self) -> CompletionListing {
        self.listing
    }

    #[cfg(test)]
    pub(crate) fn outcome(&self) -> Option<&CompletionOutcome> {
        self.outcome.as_ref()
    }

    pub(crate) fn apply_effects(&self, editor: &mut EditLine) {
        if self.signal == CompletionSignal::Beep {
            editor.beep();
        }
        let completed = matches!(
            self.outcome,
            Some(CompletionOutcome::Unique { .. } | CompletionOutcome::NoMatch)
        );
        if self.listing == CompletionListing::Cleared && completed {
            editor.clear_completion_pending_listing();
        }
        if !self.output.is_empty() {
            let _ = editor.write_output(&self.output);
        }
    }
}

fn fallback_candidates(
    stem: &str,
    generator: Option<&mut CandidateGenerator<'_>>,
) -> Option<Vec<String>> {
    let candidates = if let Some(generator) = generator {
        collect_candidates(stem, generator)
    } else {
        let mut scan = FilenameCompletionState::default();
        let mut state = 0;
        let mut candidates = Vec::new();
        while let Some(candidate) = filename_completion(&mut scan, stem, state) {
            candidates.push(candidate);
            state = state.saturating_add(1);
        }
        candidates
    };
    (!candidates.is_empty()).then_some(candidates)
}

fn candidate_suffix(provider: &mut Option<&mut SuffixProvider<'_>>, candidate: &str) -> String {
    provider
        .as_deref_mut()
        .map_or_else(|| completion_suffix(candidate), |suffix| suffix(candidate))
}

fn report_without_outcome(
    snapshot: CompletionSnapshot,
    attempted: AttemptedState,
) -> CompletionReport {
    CompletionReport {
        command: CompletionCommand::Error,
        invocation: snapshot.invocation,
        positions: snapshot.positions,
        listing: CompletionListing::Pending,
        attempted,
        outcome: None,
        signal: CompletionSignal::Silent,
        output: Vec::new(),
    }
}

// [spec:nshedit:req:abi.typed-completion]
pub(crate) fn resolve_completion(request: CompletionRequest<'_>) -> CompletionReport {
    let CompletionRequest {
        snapshot,
        providers,
        policy,
        apply,
    } = request;
    let CompletionProviders {
        generator,
        mut attempted,
        mut suffix,
    } = providers;
    let Ok(query) = snapshot.query.as_ref() else {
        return report_without_outcome(snapshot, AttemptedState::Preserve);
    };
    let stem = String::from_utf8_lossy(&text_bytes(query.stem())).into_owned();
    let start = query.replacement().start().get();
    let finish = query.cursor().get();
    let attempted_supplied = attempted.is_some();
    let attempt = attempted
        .as_mut()
        .map(|provider| provider(&stem, start, finish));
    let matches = match attempt {
        Some(AttemptedCompletion {
            candidates: Some(candidates),
            ..
        }) => Some(candidates),
        Some(AttemptedCompletion {
            candidates: None,
            fallback: AttemptedFallback::Suppress,
        }) => None,
        _ => fallback_candidates(&stem, generator),
    };
    let Some(matches) = matches else {
        return CompletionReport {
            command: CompletionCommand::Normal,
            invocation: snapshot.invocation,
            positions: snapshot.positions,
            listing: CompletionListing::Cleared,
            attempted: AttemptedState::Reset,
            outcome: Some(CompletionOutcome::NoMatch),
            signal: CompletionSignal::Beep,
            output: Vec::new(),
        };
    };

    let append_unique =
        matches.len() == 1 && (policy.unique_suffix == UniqueSuffix::Append || attempted_supplied);
    let candidates: Vec<_> = matches
        .into_iter()
        .map(|candidate| {
            let suffix = append_unique.then(|| candidate_suffix(&mut suffix, &candidate));
            let value = CompletionCandidate::new(candidate);
            match suffix {
                Some(suffix) => value.with_suffix(suffix),
                None => value,
            }
        })
        .collect();
    let outcome = match apply(query, candidates.into()) {
        Ok(outcome) => outcome,
        Err(_) => return report_without_outcome(snapshot, AttemptedState::Reset),
    };

    let mut output = Vec::new();
    let (command, listing, signal) = match &outcome {
        CompletionOutcome::Unique { .. } => (
            CompletionCommand::Refresh,
            CompletionListing::Cleared,
            CompletionSignal::Silent,
        ),
        CompletionOutcome::NoMatch => (
            CompletionCommand::Normal,
            CompletionListing::Cleared,
            CompletionSignal::Beep,
        ),
        CompletionOutcome::Ambiguous { candidates, .. }
            if snapshot.invocation == CompletionInvocation::List =>
        {
            output.push(b'\n');
            if candidates.len() <= policy.listing_limit {
                let mut display: Vec<_> = candidates
                    .iter()
                    .map(|candidate| {
                        String::from_utf8_lossy(&text_bytes(candidate.display())).into_owned()
                    })
                    .collect();
                let width = display.iter().map(String::len).max().unwrap_or(0);
                let formatted = if let Some(provider) = suffix.as_mut() {
                    format_match_list(&mut display, width, snapshot.columns, *provider)
                } else {
                    format_match_list(
                        &mut display,
                        width,
                        snapshot.columns,
                        &mut completion_suffix,
                    )
                };
                output.extend(formatted);
                (
                    CompletionCommand::Redisplay,
                    CompletionListing::Presented,
                    CompletionSignal::Silent,
                )
            } else {
                (
                    CompletionCommand::Redisplay,
                    CompletionListing::OmittedByLimit,
                    CompletionSignal::Silent,
                )
            }
        }
        CompletionOutcome::Ambiguous { .. } => (
            CompletionCommand::Refresh,
            CompletionListing::Pending,
            CompletionSignal::Beep,
        ),
    };
    CompletionReport {
        command,
        invocation: snapshot.invocation,
        positions: snapshot.positions,
        listing,
        attempted: AttemptedState::Reset,
        outcome: Some(outcome),
        signal,
        output,
    }
}

pub(crate) fn complete_filename(editor: &mut EditLine) -> CompletionCommand {
    let separators = BREAK_CHARACTERS
        .iter()
        .copied()
        .map(TextUnit::from_code_point)
        .collect();
    let snapshot = observe_completion(editor, separators);
    let report = {
        let mut apply = |query: &CompletionQuery, candidates: CompletionCandidates| {
            editor.editor_mut().apply_completion(query, candidates)
        };
        resolve_completion(CompletionRequest::new(
            snapshot,
            CompletionProviders::new(None),
            CompletionPolicy::new(100, UniqueSuffix::Append),
            &mut apply,
        ))
    };
    report.apply_effects(editor);
    report.command()
}
