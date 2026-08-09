//! Typed signal delivery and continuation state for the native read driver.

use crate::domain::{Signal, SignalPolicy, TerminalMode};
use crate::editor::effect::{Effect, HostFailure, ResizeEffect, SignalEffect};
use crate::editor::{Editor, TerminalControl};

use super::{
    DisplayKind, DriverError, EffectKind, Pending, ReadDriver, ReadInterrupt, ReadResult, ReadStep,
};

// [spec:nshedit:req:abi.signal-lifecycle]
impl ReadDriver {
    /// Resume a terminal-size request and continue its typed signal state, if
    /// the resize was caused by a delivery.
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
        if let Some(signal) = self.signal_after_resize.take() {
            self.pending(editor, SignalEffect { signal }, EffectKind::Signal)
                .map(ReadStep::Signal)
        } else {
            self.schedule_display(editor, DisplayKind::Refresh)
        }
    }

    /// Resume propagation of the caller's previous signal disposition.
    pub fn resume_signal<T: TerminalControl>(
        &mut self,
        editor: &mut Editor<T>,
        pending: &Pending<SignalEffect>,
        response: <SignalEffect as Effect>::Response,
    ) -> Result<ReadStep, DriverError> {
        let signal = pending.request().signal;
        let response = self.accept(editor, pending, EffectKind::Signal, response)?;
        match response {
            Ok(()) => {}
            Err(HostFailure::Interrupted) => {
                return self.complete(editor, ReadResult::Interrupted(ReadInterrupt::Host));
            }
            Err(error) => return Err(self.fail(editor, DriverError::Host(error))),
        }

        match signal {
            Signal::Resize => self.schedule_display(editor, DisplayKind::Refresh),
            Signal::Suspend => {
                editor
                    .set_terminal_mode(TerminalMode::Editing)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.pending(
                    editor,
                    SignalEffect {
                        signal: Signal::Continue,
                    },
                    EffectKind::Signal,
                )
                .map(ReadStep::Signal)
            }
            Signal::Continue => self
                .pending(editor, ResizeEffect::Resume, EffectKind::Resize)
                .map(ReadStep::Resize),
            Signal::Interrupt | Signal::Quit | Signal::Hangup | Signal::Terminate => self.complete(
                editor,
                ReadResult::Interrupted(ReadInterrupt::Signal(signal)),
            ),
        }
    }

    pub(super) fn handle_signal<T: TerminalControl>(
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
            Signal::Resize => {
                self.signal_after_resize = Some(signal);
                self.pending(editor, ResizeEffect::Signal, EffectKind::Resize)
                    .map(ReadStep::Resize)
            }
            Signal::Continue => {
                editor
                    .set_terminal_mode(TerminalMode::Editing)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.pending(editor, SignalEffect { signal }, EffectKind::Signal)
                    .map(ReadStep::Signal)
            }
            Signal::Interrupt
            | Signal::Quit
            | Signal::Hangup
            | Signal::Terminate
            | Signal::Suspend => {
                editor
                    .set_terminal_mode(TerminalMode::Cooked)
                    .map_err(|error| self.fail(editor, DriverError::Terminal(error)))?;
                self.pending(editor, SignalEffect { signal }, EffectKind::Signal)
                    .map(ReadStep::Signal)
            }
        }
    }
}
