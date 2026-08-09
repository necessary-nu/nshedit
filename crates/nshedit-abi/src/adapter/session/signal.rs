//! Signal policy and persistent unbuffered ownership.

use super::*;
use nshedit_plat::signal::Signal;

impl EditLine {
    pub(crate) fn set_handle_signals(&mut self, enabled: bool) {
        self.boundary.policy.handle_signals = enabled;
        if enabled && self.unbuffered() {
            let _ = self.arm_persistent_signal_handlers();
        } else if !enabled {
            let _ = self.disarm_persistent_signal_handlers();
        }
        self.reconfigure();
    }

    pub(crate) fn signal_handlers(&self) -> Option<&SignalHandlers> {
        self.boundary.signal_handlers.as_ref()
    }

    pub(crate) fn signal_handlers_mut(&mut self) -> Option<&mut SignalHandlers> {
        self.boundary.signal_handlers.as_mut()
    }

    pub(crate) fn arm_persistent_signal_handlers(
        &mut self,
    ) -> Result<(), nshedit_plat::signal::SignalError> {
        if self.boundary.signal_handlers.is_none() {
            self.boundary.signal_handlers = Some(SignalHandlers::with_signals(&Signal::EDITOR)?);
        }
        Ok(())
    }

    pub(crate) fn disarm_persistent_signal_handlers(
        &mut self,
    ) -> Result<(), nshedit_plat::signal::SignalError> {
        self.boundary
            .signal_handlers
            .take()
            .map_or(Ok(()), SignalHandlers::restore)
    }
}
