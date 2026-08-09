//! ABI adaptation for the native driver's typed signal protocol.

use nshedit::domain::{ScreenSize, Signal as EditorSignal, TerminalMode};
use nshedit::editor::effect::{HostFailure, ResizeEffect};
use nshedit_plat::signal::{BlockedSignals, Signal as PlatformSignal, SignalError, SignalHandlers};

use crate::adapter::EditLine;
#[cfg(test)]
use crate::adapter::SessionInit;

// [spec:nshedit:req:abi.signal-lifecycle]
/// Signal ownership associated with one ABI read invocation.
///
/// Buffered reads own a local guard. Unbuffered reads use the guard retained
/// by the opaque handle from `EL_UNBUFFERED` activation until deactivation.
pub(super) struct ReadSignals {
    scoped: Option<SignalHandlers>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DirectReadOutcome {
    Resume,
    Interrupt,
}

impl ReadSignals {
    /// Arm a normal edited read when `EL_SIGNAL` requests it.
    pub(super) unsafe fn edited(el: *mut EditLine) -> Result<Self, ()> {
        if !unsafe { (&*el).handle_signals() } {
            return Ok(Self::empty());
        }
        if unsafe { (&*el).unbuffered() } {
            unsafe { (&mut *el).arm_persistent_signal_handlers() }.map_err(|_| ())?;
            Ok(Self::empty())
        } else {
            SignalHandlers::with_signals(&PlatformSignal::EDITOR)
                .map(|handlers| Self {
                    scoped: Some(handlers),
                })
                .map_err(|_| ())
        }
    }

    /// Arm a non-editing reader only where the detailed read lifecycle does.
    pub(super) unsafe fn unedited(el: *mut EditLine) -> Result<Self, ()> {
        if !unsafe { (&*el).handle_signals() } {
            return Ok(Self::empty());
        }
        if unsafe { (&*el).unbuffered() } {
            unsafe { (&mut *el).arm_persistent_signal_handlers() }.map_err(|_| ())?;
            return Ok(Self::empty());
        }
        if unsafe { (&*el).is_tty() } {
            return SignalHandlers::with_signals(&PlatformSignal::EDITOR)
                .map(|handlers| Self {
                    scoped: Some(handlers),
                })
                .map_err(|_| ());
        }
        Ok(Self::empty())
    }

    /// Observe an already-active unbuffered scope without arming a direct
    /// `el_wgetc` call of its own.
    pub(super) unsafe fn take_pending(&self, el: *mut EditLine) -> Option<PlatformSignal> {
        if let Some(handlers) = self.scoped.as_ref() {
            handlers.take_pending()
        } else {
            unsafe { (&*el).signal_handlers() }.and_then(SignalHandlers::take_pending)
        }
    }

    /// Complete a delivery that raced with a successful direct operation.
    ///
    /// The operation's result remains authoritative, as it does when the C
    /// handler returns to an in-flight callback or system call.
    pub(super) unsafe fn resume_pending_direct(
        &mut self,
        el: *mut EditLine,
    ) -> Result<Option<DirectReadOutcome>, HostFailure> {
        let Some(signal) = (unsafe { self.take_pending(el) }) else {
            return Ok(None);
        };
        unsafe { self.resume_direct(el, signal) }.map(Some)
    }

    /// Finish a buffered signal scope and report disposition-restoration
    /// failures. Persistent unbuffered ownership is intentionally retained.
    pub(super) unsafe fn finish(mut self, el: *mut EditLine) -> Result<(), HostFailure> {
        let _ = unsafe { self.resume_pending_direct(el) }?;
        self.scoped
            .take()
            .map_or(Ok(()), SignalHandlers::restore)
            .map_err(signal_failure)
    }

    pub(super) unsafe fn propagate(
        &mut self,
        el: *mut EditLine,
        signal: EditorSignal,
    ) -> Result<(), HostFailure> {
        unsafe { self.propagate_platform(el, platform_signal(signal)) }
    }

    pub(super) unsafe fn resize(
        &self,
        el: *mut EditLine,
        reason: ResizeEffect,
    ) -> Result<ScreenSize, HostFailure> {
        if matches!(reason, ResizeEffect::Prepare | ResizeEffect::Signal) {
            let blocked =
                BlockedSignals::block(&[PlatformSignal::Resize]).map_err(signal_failure)?;
            unsafe { (&mut *el).resize_display() };
            blocked.restore().map_err(signal_failure)?;
        }
        unsafe { (&*el).screen_size() }.ok_or(HostFailure::Unavailable)
    }

    /// Complete terminal work and previous-disposition chaining for a direct
    /// reader that does not use the native driver.
    pub(super) unsafe fn resume_direct(
        &mut self,
        el: *mut EditLine,
        signal: PlatformSignal,
    ) -> Result<DirectReadOutcome, HostFailure> {
        match signal {
            PlatformSignal::Resize => {
                unsafe { self.resize(el, ResizeEffect::Signal) }?;
            }
            PlatformSignal::Continue => unsafe {
                set_terminal_mode(el, TerminalMode::Editing)?;
            },
            PlatformSignal::Hangup
            | PlatformSignal::Interrupt
            | PlatformSignal::Quit
            | PlatformSignal::Terminate
            | PlatformSignal::Suspend => unsafe {
                set_terminal_mode(el, TerminalMode::Cooked)?;
            },
        }

        unsafe { self.propagate_platform(el, signal) }?;
        if signal == PlatformSignal::Suspend {
            unsafe { set_terminal_mode(el, TerminalMode::Editing)? };
            unsafe { self.propagate_platform(el, PlatformSignal::Continue) }?;
        }
        Ok(
            if matches!(
                signal,
                PlatformSignal::Suspend | PlatformSignal::Continue | PlatformSignal::Resize
            ) {
                DirectReadOutcome::Resume
            } else {
                DirectReadOutcome::Interrupt
            },
        )
    }

    pub(super) const fn empty() -> Self {
        Self { scoped: None }
    }

    unsafe fn propagate_platform(
        &mut self,
        el: *mut EditLine,
        signal: PlatformSignal,
    ) -> Result<(), HostFailure> {
        let handlers = if let Some(handlers) = self.scoped.as_mut() {
            handlers
        } else {
            unsafe { (&mut *el).signal_handlers_mut() }.ok_or(HostFailure::Unavailable)?
        };
        handlers.propagate(signal).map_err(signal_failure)?;
        if signal == PlatformSignal::Suspend {
            match handlers.take_pending() {
                Some(PlatformSignal::Continue) => {}
                Some(signal) => {
                    return Err(HostFailure::Failed(
                        format!("unexpected {signal:?} while resuming from Suspend")
                            .into_boxed_str(),
                    ));
                }
                None => return Err(HostFailure::Interrupted),
            }
        }
        if matches!(
            signal,
            PlatformSignal::Suspend | PlatformSignal::Continue | PlatformSignal::Resize
        ) {
            handlers.rearm().map_err(signal_failure)?;
        }
        Ok(())
    }
}

fn platform_signal(signal: EditorSignal) -> PlatformSignal {
    match signal {
        EditorSignal::Hangup => PlatformSignal::Hangup,
        EditorSignal::Interrupt => PlatformSignal::Interrupt,
        EditorSignal::Quit => PlatformSignal::Quit,
        EditorSignal::Terminate => PlatformSignal::Terminate,
        EditorSignal::Suspend => PlatformSignal::Suspend,
        EditorSignal::Continue => PlatformSignal::Continue,
        EditorSignal::Resize => PlatformSignal::Resize,
    }
}

/// The editor's signal for a platform one.
///
/// Two signal vocabularies genuinely coexist here — the platform's delivery
/// and the editor's domain — so each conversion is named for the side it
/// answers in, and the pair reads as the round trip it is.
// [spec:nshedit:req:workspace.semantic-naming]
pub(super) fn editor_signal(signal: PlatformSignal) -> EditorSignal {
    match signal {
        PlatformSignal::Hangup => EditorSignal::Hangup,
        PlatformSignal::Interrupt => EditorSignal::Interrupt,
        PlatformSignal::Quit => EditorSignal::Quit,
        PlatformSignal::Terminate => EditorSignal::Terminate,
        PlatformSignal::Suspend => EditorSignal::Suspend,
        PlatformSignal::Continue => EditorSignal::Continue,
        PlatformSignal::Resize => EditorSignal::Resize,
    }
}

fn signal_failure(error: SignalError) -> HostFailure {
    HostFailure::Failed(format!("signal lifecycle: {error}").into_boxed_str())
}

unsafe fn set_terminal_mode(el: *mut EditLine, mode: TerminalMode) -> Result<(), HostFailure> {
    unsafe { (&mut *el).set_terminal_mode(mode) }
        .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Box<EditLine> {
        EditLine::new(SessionInit::inert("signal-adapter-test"))
            .expect("construct an editor over inert descriptors")
    }

    #[test]
    fn signals_round_trip_across_boundary() {
        for platform in PlatformSignal::EDITOR {
            assert_eq!(platform_signal(editor_signal(platform)), platform);
        }
    }

    #[test]
    fn signal_errors_keep_context() {
        for error in [
            SignalError::AlreadyActive,
            SignalError::SignalMask,
            SignalError::UnscopedHandler(PlatformSignal::Resize),
            SignalError::InstallFailed(PlatformSignal::Resize),
            SignalError::NotArmed(PlatformSignal::Resize),
            SignalError::RestoreFailed(PlatformSignal::Resize),
            SignalError::RaiseFailed(PlatformSignal::Resize),
            SignalError::OwnershipLost,
        ] {
            let expected = format!("signal lifecycle: {error}");
            let HostFailure::Failed(actual) = signal_failure(error) else {
                panic!("signal errors must retain an owned explanation");
            };
            assert_eq!(actual.as_ref(), expected);
        }
    }

    #[test]
    fn disabled_reads_claim_nothing() {
        let mut editor = editor();
        let pointer = core::ptr::from_mut(editor.as_mut());

        let edited = unsafe { ReadSignals::edited(pointer) }.unwrap();
        let unedited = unsafe { ReadSignals::unedited(pointer) }.unwrap();
        assert!(edited.scoped.is_none());
        assert!(unedited.scoped.is_none());
        assert!(editor.signal_handlers().is_none());
    }

    #[test]
    fn resume_resize_reads_committed_geometry() {
        let mut editor = editor();
        let expected = editor.screen_size().unwrap();
        let pointer = core::ptr::from_mut(editor.as_mut());
        let signals = ReadSignals::empty();

        assert_eq!(
            unsafe { signals.resize(pointer, ResizeEffect::Resume) },
            Ok(expected)
        );
        assert_eq!(editor.screen_size(), Some(expected));
    }
}
