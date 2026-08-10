//! Scoped ownership of the process signal dispositions used by an editor.

use core::fmt;
use core::marker::PhantomData;
use std::rc::Rc;

use super::{
    Installed, PENDING_SLOT, SigAction, SigSet, Signal, SignalActivation, install_handler,
    raise_default, restore_handler, sigmask_block, sigmask_set,
};

/// Why a scoped signal-handler operation could not preserve its ownership
/// contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalError {
    /// Another editor already owns the process dispositions.
    AlreadyActive,
    /// Blocking or restoring the calling thread's signal mask failed.
    SignalMask,
    /// The private trampoline was present without a live scoped owner.
    UnscopedHandler(Signal),
    /// Installing the private disposition failed.
    InstallFailed(Signal),
    /// This owner never displaced a disposition for the requested signal.
    NotArmed(Signal),
    /// Reinstalling the caller's previous disposition failed.
    RestoreFailed(Signal),
    /// Re-raising a signal for the previous disposition failed.
    RaiseFailed(Signal),
    /// The process-global registration changed outside this owner.
    OwnershipLost,
}

impl fmt::Display for SignalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyActive => formatter.write_str("signal handlers are already active"),
            Self::SignalMask => formatter.write_str("could not preserve the signal mask"),
            Self::UnscopedHandler(signal) => {
                write!(formatter, "an unscoped {signal:?} handler is installed")
            }
            Self::InstallFailed(signal) => {
                write!(formatter, "could not install the {signal:?} disposition")
            }
            Self::NotArmed(signal) => write!(formatter, "{signal:?} was not armed"),
            Self::RestoreFailed(signal) => {
                write!(formatter, "could not restore the {signal:?} disposition")
            }
            Self::RaiseFailed(signal) => write!(formatter, "could not raise {signal:?}"),
            Self::OwnershipLost => formatter.write_str("signal-handler ownership was lost"),
        }
    }
}

impl std::error::Error for SignalError {}

/// A scoped addition to the calling thread's signal mask.
///
/// The previous mask is restored on drop. [`Self::restore`] is available to
/// callers that need to observe restoration failure rather than relying on
/// best-effort cleanup.
#[must_use = "dropping the guard restores the previous signal mask"]
pub struct BlockedSignals {
    previous: Option<SigSet>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl BlockedSignals {
    /// Block `signals` on the calling thread until this guard is restored or
    /// dropped.
    pub fn block(signals: &[Signal]) -> Result<Self, SignalError> {
        if signals.is_empty() {
            return Ok(Self {
                previous: None,
                _thread_bound: PhantomData,
            });
        }

        let mut mask = SigSet::empty();
        for &signal in signals {
            mask.add(signal.number());
        }
        let previous = sigmask_block(&mask).ok_or(SignalError::SignalMask)?;
        Ok(Self {
            previous: Some(previous),
            _thread_bound: PhantomData,
        })
    }

    /// Restore the mask that was active before this guard was created.
    pub fn restore(mut self) -> Result<(), SignalError> {
        self.restore_previous()
    }

    fn restore_previous(&mut self) -> Result<(), SignalError> {
        let Some(previous) = self.previous.take() else {
            return Ok(());
        };
        if sigmask_set(&previous) {
            Ok(())
        } else {
            Err(SignalError::SignalMask)
        }
    }
}

impl Drop for BlockedSignals {
    fn drop(&mut self) {
        let _ = self.restore_previous();
    }
}

// [spec:nshedit:req:abi.signal-lifecycle]
/// RAII ownership of a selected set of editor signal dispositions.
///
/// The guard is bound to its constructing thread because signal masks and
/// `raise` delivery are thread-local even though dispositions are global.
/// Dropping it restores every disposition it displaced.
pub struct SignalHandlers {
    enabled: [bool; Signal::EDITOR.len()],
    saved: [Option<SigAction>; Signal::EDITOR.len()],
    mask: SigSet,
    activation: Option<SignalActivation>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl SignalHandlers {
    /// Scope an explicit subset of the editor signal family.
    ///
    /// This is useful to hosts that own some dispositions themselves. An
    /// empty subset is a no-op guard and does not claim global ownership.
    pub fn with_signals(signals: &[Signal]) -> Result<Self, SignalError> {
        let mut enabled = [false; Signal::EDITOR.len()];
        let mut mask = SigSet::empty();
        for &signal in signals {
            enabled[signal.index()] = true;
            mask.add(signal.number());
        }
        let mut handlers = Self {
            enabled,
            saved: [None; Signal::EDITOR.len()],
            mask,
            activation: None,
            _thread_bound: PhantomData,
        };
        if signals.is_empty() {
            return Ok(handlers);
        }

        let previous_mask = sigmask_block(&handlers.mask).ok_or(SignalError::SignalMask)?;
        let Some(activation) = PENDING_SLOT.activate() else {
            let _ = sigmask_set(&previous_mask);
            return Err(SignalError::AlreadyActive);
        };
        handlers.activation = Some(activation);

        if let Err(error) = handlers.install_missing() {
            let _ = handlers.restore_saved();
            let _ = handlers.withdraw();
            let _ = sigmask_set(&previous_mask);
            return Err(error);
        }
        if !sigmask_set(&previous_mask) {
            let _ = handlers.restore_saved();
            let _ = handlers.withdraw();
            return Err(SignalError::SignalMask);
        }
        Ok(handlers)
    }

    /// Take the most recently recorded delivery, if any.
    pub fn take_pending(&self) -> Option<Signal> {
        self.activation
            .and_then(|activation| PENDING_SLOT.take(activation))
            .and_then(Signal::from_number)
    }

    /// Restore every displaced disposition and report cleanup failures.
    ///
    /// Dropping a guard remains sufficient for unwind safety. Call this when
    /// normal control flow needs to distinguish successful restoration from
    /// best-effort cleanup.
    pub fn restore(mut self) -> Result<(), SignalError> {
        self.disarm()
    }

    /// Restore and invoke the disposition displaced for `signal`.
    ///
    /// The signal remains blocked between restoration and `raise`, so the
    /// previous disposition runs as the caller's mask is restored, matching
    /// the chaining order of the C implementation without doing editor work
    /// in the trampoline.
    pub fn propagate(&mut self, signal: Signal) -> Result<(), SignalError> {
        let previous_mask = sigmask_block(&self.mask).ok_or(SignalError::SignalMask)?;
        let result = self.propagate_blocked(signal);
        if !sigmask_set(&previous_mask) {
            return Err(SignalError::SignalMask);
        }
        result
    }

    /// Reinstall this guard for every selected signal whose previous
    /// disposition was consumed by propagation.
    pub fn rearm(&mut self) -> Result<(), SignalError> {
        let previous_mask = sigmask_block(&self.mask).ok_or(SignalError::SignalMask)?;
        let result = self.install_missing();
        if !sigmask_set(&previous_mask) {
            return Err(SignalError::SignalMask);
        }
        result
    }

    fn propagate_blocked(&mut self, signal: Signal) -> Result<(), SignalError> {
        let index = signal.index();
        if !self.enabled[index] {
            return Err(SignalError::NotArmed(signal));
        }
        let Some(previous) = self.saved[index].take() else {
            return Err(SignalError::NotArmed(signal));
        };
        if !restore_handler(signal.number(), previous) {
            self.saved[index] = Some(previous);
            return Err(SignalError::RestoreFailed(signal));
        }
        if !raise_default(signal.number()) {
            return Err(SignalError::RaiseFailed(signal));
        }
        Ok(())
    }

    fn install_missing(&mut self) -> Result<(), SignalError> {
        for signal in Signal::EDITOR {
            let index = signal.index();
            if !self.enabled[index] || self.saved[index].is_some() {
                continue;
            }
            match install_handler(signal.number(), &self.mask) {
                Installed::Displaced(previous) => self.saved[index] = Some(previous),
                Installed::AlreadyOurs => return Err(SignalError::UnscopedHandler(signal)),
                Installed::Failed => return Err(SignalError::InstallFailed(signal)),
            }
        }
        Ok(())
    }

    fn restore_saved(&mut self) -> Result<(), SignalError> {
        let mut first_error = None;
        for signal in Signal::EDITOR {
            let Some(previous) = self.saved[signal.index()].take() else {
                continue;
            };
            if !restore_handler(signal.number(), previous) && first_error.is_none() {
                first_error = Some(SignalError::RestoreFailed(signal));
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn withdraw(&mut self) -> Result<(), SignalError> {
        let Some(activation) = self.activation.take() else {
            return Ok(());
        };
        PENDING_SLOT
            .deactivate(activation)
            .then_some(())
            .ok_or(SignalError::OwnershipLost)
    }

    fn disarm(&mut self) -> Result<(), SignalError> {
        if self.activation.is_none() {
            return Ok(());
        }
        let previous_mask = sigmask_block(&self.mask);
        let mut first_error = previous_mask.is_none().then_some(SignalError::SignalMask);
        if let Err(error) = self.restore_saved()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Err(error) = self.withdraw()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(previous_mask) = previous_mask
            && !sigmask_set(&previous_mask)
            && first_error.is_none()
        {
            first_error = Some(SignalError::SignalMask);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for SignalHandlers {
    fn drop(&mut self) {
        let _ = self.disarm();
    }
}
