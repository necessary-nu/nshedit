//! Safe host-terminal integration for native Rust consumers.

use std::io;
#[cfg(unix)]
use std::os::fd::BorrowedFd as BorrowedIo;
#[cfg(windows)]
use std::os::windows::io::BorrowedHandle as BorrowedIo;

use crate::domain::{EditorConfig, ScreenSize, TerminalMode};

use super::TerminalControl;

/// A safe terminal controller over caller-owned descriptors.
///
/// The descriptors are borrowed for the controller's lifetime. The editor
/// owns this controller and therefore owns its single restoration obligation;
/// input and output streams remain with the caller.
pub struct SystemTerminal<'io> {
    inner: nshedit_plat::terminal::TerminalController<'io>,
}

impl<'io> SystemTerminal<'io> {
    /// Borrow the input and output terminal descriptors or handles.
    #[must_use]
    pub const fn new(input: BorrowedIo<'io>, output: BorrowedIo<'io>) -> Self {
        Self {
            inner: nshedit_plat::terminal::TerminalController::new(input, output),
        }
    }

    /// Read validated display dimensions from a terminal descriptor.
    pub fn screen_size(output: BorrowedIo<'_>) -> io::Result<ScreenSize> {
        let (rows, columns) = nshedit_plat::terminal::screen_size(output)?;
        ScreenSize::new(rows, columns)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }

    /// Count input bytes available without blocking.
    #[cfg(unix)]
    pub fn bytes_ready(input: BorrowedIo<'_>) -> io::Result<u64> {
        nshedit_plat::terminal::bytes_ready(input)
    }
}

impl TerminalControl for SystemTerminal<'_> {
    fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
        self.inner.activate()
    }

    fn set_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
        match mode {
            TerminalMode::Cooked => self.inner.enter_cooked_mode(),
            TerminalMode::Editing => self.inner.enter_editing_mode(),
            TerminalMode::Quoted => self.inner.enter_quoted_mode(),
        }
    }

    fn restore(&mut self) -> io::Result<()> {
        self.inner.restore()
    }
}
