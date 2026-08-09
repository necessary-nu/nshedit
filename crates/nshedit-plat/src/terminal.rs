//! Safe interactive-terminal control without exposing termios representation.

use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};

use crate::termios::{self, Termios};

/// An activated terminal's exactly-once restoration state.
///
/// The caller owns this value and decides whether explicit cleanup or its own
/// RAII owner invokes [`Self::restore`]. This type performs no cleanup in
/// `Drop`, so it can sit beneath a higher-level owner without competing for
/// the restoration obligation.
pub struct TerminalController<'fd> {
    input: BorrowedFd<'fd>,
    output: BorrowedFd<'fd>,
    original: Option<Termios>,
    editing: Option<Termios>,
    quoted: Option<Termios>,
    restoration_due: bool,
}

impl<'fd> TerminalController<'fd> {
    /// Borrow the terminal descriptors without taking ownership of them.
    #[must_use]
    pub const fn new(input: BorrowedFd<'fd>, output: BorrowedFd<'fd>) -> Self {
        Self {
            input,
            output,
            original: None,
            editing: None,
            quoted: None,
            restoration_due: false,
        }
    }

    /// Capture the current state and enter interactive editing mode.
    pub fn activate(&mut self) -> io::Result<()> {
        if !termios::isatty(self.output.as_raw_fd()) {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "editor output is not a terminal",
            ));
        }
        let original = termios::tcgetattr(self.input.as_raw_fd()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                "editor input is not a terminal",
            )
        })?;
        let editing = editing_attributes(original);
        let quoted = quoted_attributes(editing);
        self.original = Some(original);
        self.editing = Some(editing);
        self.quoted = Some(quoted);
        self.restoration_due = true;
        apply(self.input, termios::TCSADRAIN, &editing)
    }

    /// Restore normal terminal processing without consuming the obligation.
    pub fn enter_cooked_mode(&mut self) -> io::Result<()> {
        apply_optional(self.input, termios::TCSADRAIN, self.original.as_ref())
    }

    /// Re-enter interactive editing mode.
    pub fn enter_editing_mode(&mut self) -> io::Result<()> {
        apply_optional(self.input, termios::TCSADRAIN, self.editing.as_ref())
    }

    /// Temporarily read the next input unit without signal or flow handling.
    pub fn enter_quoted_mode(&mut self) -> io::Result<()> {
        apply_optional(self.input, termios::TCSADRAIN, self.quoted.as_ref())
    }

    /// Consume and restore the state captured by [`Self::activate`].
    pub fn restore(&mut self) -> io::Result<()> {
        if !self.restoration_due {
            return Ok(());
        }
        self.restoration_due = false;
        apply_optional(self.input, termios::TCSAFLUSH, self.original.as_ref())
    }
}

/// Read a terminal's current `(rows, columns)` dimensions.
pub fn screen_size(output: BorrowedFd<'_>) -> io::Result<(usize, usize)> {
    termios::window_size(output.as_raw_fd())
        .map(|(rows, columns)| (usize::from(rows), usize::from(columns)))
        .ok_or_else(|| io::Error::other("could not read terminal dimensions"))
}

/// Count bytes that can be read immediately without blocking.
pub fn bytes_ready(input: BorrowedFd<'_>) -> io::Result<u64> {
    crate::bytes_ready_to_read(input.as_raw_fd())
        .ok_or_else(|| io::Error::other("could not inspect pending terminal input"))
}

fn apply_optional(
    descriptor: BorrowedFd<'_>,
    action: i32,
    attributes: Option<&Termios>,
) -> io::Result<()> {
    attributes.map_or(Ok(()), |attributes| apply(descriptor, action, attributes))
}

fn apply(descriptor: BorrowedFd<'_>, action: i32, attributes: &Termios) -> io::Result<()> {
    if termios::tcsetattr(descriptor.as_raw_fd(), action, attributes) {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn editing_attributes(mut attributes: Termios) -> Termios {
    attributes.c_iflag |= termios::INLCR | termios::ICRNL;
    attributes.c_iflag &= !termios::IGNCR;
    attributes.c_oflag |= termios::OPOST | termios::ONLCR;
    attributes.c_oflag &= !termios::ONLRET;
    attributes.c_lflag |= termios::ISIG;
    attributes.c_lflag &= !(termios::NOFLSH
        | termios::ICANON
        | termios::ECHO
        | termios::ECHOK
        | termios::ECHONL
        | termios::EXTPROC
        | termios::IEXTEN
        | termios::FLUSHO);
    attributes.c_cc[termios::VEOF] = termios::VDISABLE;
    attributes.c_cc[termios::VEOL] = termios::VDISABLE;
    attributes.c_cc[termios::VEOL2] = termios::VDISABLE;
    attributes.c_cc[termios::VWERASE] = termios::VDISABLE;
    attributes.c_cc[termios::VREPRINT] = termios::VDISABLE;
    attributes.c_cc[termios::VLNEXT] = termios::VDISABLE;
    attributes.c_cc[termios::VMIN] = 1;
    attributes.c_cc[termios::VTIME] = 0;
    attributes
}

fn quoted_attributes(mut attributes: Termios) -> Termios {
    attributes.c_iflag &= !(termios::IXON | termios::IXOFF | termios::INLCR | termios::ICRNL);
    attributes.c_lflag &= !(termios::ISIG | termios::IEXTEN);
    attributes
}

#[cfg(test)]
mod tests {
    use std::os::fd::{AsFd, AsRawFd, OwnedFd};

    use super::*;

    struct Pty {
        _user: OwnedFd,
        terminal: OwnedFd,
    }

    impl Pty {
        fn open() -> Self {
            use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};

            let user = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("/dev/ptmx");
            grantpt(&user).expect("grantpt");
            unlockpt(&user).expect("unlockpt");
            let name = ptsname(&user, Vec::new()).expect("ptsname");
            let terminal = rustix::fs::open(
                name.as_c_str(),
                rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY,
                rustix::fs::Mode::empty(),
            )
            .expect("the terminal side");
            Self {
                _user: user,
                terminal,
            }
        }

        fn attributes(&self) -> Termios {
            termios::tcgetattr(self.terminal.as_raw_fd()).expect("tcgetattr")
        }
    }

    #[test]
    fn modes_restore_original_once() {
        let pty = Pty::open();
        let original = pty.attributes();
        let mut controller = TerminalController::new(pty.terminal.as_fd(), pty.terminal.as_fd());

        controller.activate().expect("activate");
        let editing = pty.attributes();
        assert_eq!(editing.c_lflag & (termios::ICANON | termios::ECHO), 0);
        assert_ne!(editing.c_lflag & termios::ISIG, 0);
        assert_eq!(editing.c_cc[termios::VMIN], 1);
        assert_eq!(editing.c_cc[termios::VTIME], 0);

        controller.enter_quoted_mode().expect("quoted mode");
        let quoted = pty.attributes();
        assert_eq!(quoted.c_lflag & (termios::ISIG | termios::IEXTEN), 0);
        assert_eq!(
            quoted.c_iflag & (termios::IXON | termios::IXOFF | termios::INLCR | termios::ICRNL),
            0
        );

        controller.enter_cooked_mode().expect("cooked mode");
        assert_same_attributes(pty.attributes(), original);
        controller.enter_editing_mode().expect("editing mode");
        assert_same_attributes(pty.attributes(), editing);

        controller.restore().expect("restore");
        assert_same_attributes(pty.attributes(), original);
        controller.restore().expect("idempotent restore");
        assert_same_attributes(pty.attributes(), original);
    }

    #[test]
    fn non_terminal_needs_no_restoration() {
        let null = std::fs::File::open("/dev/null").expect("/dev/null");
        let mut controller = TerminalController::new(null.as_fd(), null.as_fd());

        assert_eq!(
            controller.activate().expect_err("not a terminal").kind(),
            io::ErrorKind::NotConnected
        );
        controller.restore().expect("nothing to restore");
    }

    fn assert_same_attributes(actual: Termios, expected: Termios) {
        assert_eq!(actual.c_iflag, expected.c_iflag);
        assert_eq!(actual.c_oflag, expected.c_oflag);
        assert_eq!(actual.c_cflag, expected.c_cflag);
        assert_eq!(actual.c_lflag, expected.c_lflag);
        assert_eq!(actual.c_cc, expected.c_cc);
    }
}
