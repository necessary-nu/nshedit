//! Safe interactive-terminal control without exposing termios representation.

use std::io;
use std::os::fd::BorrowedFd;

use crate::termios::{self, Termios};

/// When a terminal attribute change takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyWhen {
    /// Apply without waiting for queued output.
    Immediately,
    /// Wait for queued output to drain.
    AfterOutput,
    /// Drain output and discard unread input.
    AfterOutputAndDiscardInput,
}

impl ApplyWhen {
    const fn rustix(self) -> rustix::termios::OptionalActions {
        match self {
            Self::Immediately => rustix::termios::OptionalActions::Now,
            Self::AfterOutput => rustix::termios::OptionalActions::Drain,
            Self::AfterOutputAndDiscardInput => rustix::termios::OptionalActions::Flush,
        }
    }
}

/// A configurable terminal behavior.
///
/// The enum is the public vocabulary; platform bit positions remain private.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalFlag {
    IgnoreBreak,
    SignalBreak,
    IgnoreParityErrors,
    MarkParityErrors,
    CheckInputParity,
    StripInputHighBit,
    MapNewlineToCarriageReturn,
    IgnoreCarriageReturn,
    MapCarriageReturnToNewline,
    MapUppercaseInputToLowercase,
    EnableOutputFlowControl,
    AllowAnyCharacterToRestartOutput,
    EnableInputFlowControl,
    RingBellOnInputOverflow,
    PostProcessOutput,
    DiscardEndOfTransmissionOnOutput,
    MapLowercaseOutputToUppercase,
    MapNewlineToCarriageReturnNewline,
    MapCarriageReturnToNewlineOnOutput,
    DiscardCarriageReturnAtColumnZero,
    NewlinePerformsCarriageReturn,
    UseFillCharacters,
    UseDeleteForFill,
    NewlineDelay,
    CarriageReturnDelay,
    TabDelay,
    ExpandTabs,
    BackspaceDelay,
    VerticalTabDelay,
    FormFeedDelay,
    OutputSpeedBits,
    IgnoreControlFlags,
    TwoStopBits,
    EnableReceiver,
    EnableParity,
    OddParity,
    HangUpOnClose,
    IgnoreModemControl,
    InputSpeedBits,
    HardwareFlowControl,
    CtsOutputFlowControl,
    RtsInputFlowControl,
    ModemBufferFlowControl,
    GenerateSignals,
    CanonicalInput,
    CanonicalUppercase,
    AlternateWordErase,
    EchoInput,
    EchoErase,
    EchoKill,
    EchoNewline,
    DisableFlush,
    StopBackgroundOutput,
    EchoControlCharacters,
    EchoErasedCharacters,
    VisuallyEraseKilledLine,
    OutputBeingFlushed,
    SuppressKernelStatus,
    PendingInput,
    ExtendedProcessing,
    ExternalProcessing,
}

#[derive(Clone, Copy)]
enum FlagWord {
    Input,
    Output,
    Control,
    Local,
}

use TerminalFlag::*;

// [spec:nshedit:req:platform.darwin-termios]
const FLAG_REPRESENTATIONS: &[(TerminalFlag, FlagWord, termios::FlagBits)] = &[
    (IgnoreBreak, FlagWord::Input, termios::IGNBRK),
    (SignalBreak, FlagWord::Input, termios::BRKINT),
    (IgnoreParityErrors, FlagWord::Input, termios::IGNPAR),
    (MarkParityErrors, FlagWord::Input, termios::PARMRK),
    (CheckInputParity, FlagWord::Input, termios::INPCK),
    (StripInputHighBit, FlagWord::Input, termios::ISTRIP),
    (MapNewlineToCarriageReturn, FlagWord::Input, termios::INLCR),
    (IgnoreCarriageReturn, FlagWord::Input, termios::IGNCR),
    (MapCarriageReturnToNewline, FlagWord::Input, termios::ICRNL),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (
        MapUppercaseInputToLowercase,
        FlagWord::Input,
        termios::IUCLC,
    ),
    (EnableOutputFlowControl, FlagWord::Input, termios::IXON),
    (
        AllowAnyCharacterToRestartOutput,
        FlagWord::Input,
        termios::IXANY,
    ),
    (EnableInputFlowControl, FlagWord::Input, termios::IXOFF),
    (RingBellOnInputOverflow, FlagWord::Input, termios::IMAXBEL),
    (PostProcessOutput, FlagWord::Output, termios::OPOST),
    #[cfg(target_os = "macos")]
    (
        DiscardEndOfTransmissionOnOutput,
        FlagWord::Output,
        termios::ONOEOT,
    ),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (
        MapLowercaseOutputToUppercase,
        FlagWord::Output,
        termios::OLCUC,
    ),
    (
        MapNewlineToCarriageReturnNewline,
        FlagWord::Output,
        termios::ONLCR,
    ),
    (
        MapCarriageReturnToNewlineOnOutput,
        FlagWord::Output,
        termios::OCRNL,
    ),
    (
        DiscardCarriageReturnAtColumnZero,
        FlagWord::Output,
        termios::ONOCR,
    ),
    (
        NewlinePerformsCarriageReturn,
        FlagWord::Output,
        termios::ONLRET,
    ),
    (UseFillCharacters, FlagWord::Output, termios::OFILL),
    (UseDeleteForFill, FlagWord::Output, termios::OFDEL),
    (NewlineDelay, FlagWord::Output, termios::NLDLY),
    (CarriageReturnDelay, FlagWord::Output, termios::CRDLY),
    (TabDelay, FlagWord::Output, termios::TABDLY),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (ExpandTabs, FlagWord::Output, termios::XTABS),
    (BackspaceDelay, FlagWord::Output, termios::BSDLY),
    (VerticalTabDelay, FlagWord::Output, termios::VTDLY),
    (FormFeedDelay, FlagWord::Output, termios::FFDLY),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (OutputSpeedBits, FlagWord::Control, termios::CBAUD),
    #[cfg(target_os = "macos")]
    (IgnoreControlFlags, FlagWord::Control, termios::CIGNORE),
    (TwoStopBits, FlagWord::Control, termios::CSTOPB),
    (EnableReceiver, FlagWord::Control, termios::CREAD),
    (EnableParity, FlagWord::Control, termios::PARENB),
    (OddParity, FlagWord::Control, termios::PARODD),
    (HangUpOnClose, FlagWord::Control, termios::HUPCL),
    (IgnoreModemControl, FlagWord::Control, termios::CLOCAL),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (InputSpeedBits, FlagWord::Control, termios::CIBAUD),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (HardwareFlowControl, FlagWord::Control, termios::CRTSCTS),
    #[cfg(target_os = "macos")]
    (CtsOutputFlowControl, FlagWord::Control, termios::CCTS_OFLOW),
    #[cfg(target_os = "macos")]
    (RtsInputFlowControl, FlagWord::Control, termios::CRTS_IFLOW),
    #[cfg(target_os = "macos")]
    (ModemBufferFlowControl, FlagWord::Control, termios::MDMBUF),
    (GenerateSignals, FlagWord::Local, termios::ISIG),
    (CanonicalInput, FlagWord::Local, termios::ICANON),
    #[cfg(any(target_os = "linux", target_os = "android"))]
    (CanonicalUppercase, FlagWord::Local, termios::XCASE),
    #[cfg(target_os = "macos")]
    (AlternateWordErase, FlagWord::Local, termios::ALTWERASE),
    (EchoInput, FlagWord::Local, termios::ECHO),
    (EchoErase, FlagWord::Local, termios::ECHOE),
    (EchoKill, FlagWord::Local, termios::ECHOK),
    (EchoNewline, FlagWord::Local, termios::ECHONL),
    (DisableFlush, FlagWord::Local, termios::NOFLSH),
    (StopBackgroundOutput, FlagWord::Local, termios::TOSTOP),
    (EchoControlCharacters, FlagWord::Local, termios::ECHOCTL),
    (EchoErasedCharacters, FlagWord::Local, termios::ECHOPRT),
    (VisuallyEraseKilledLine, FlagWord::Local, termios::ECHOKE),
    (OutputBeingFlushed, FlagWord::Local, termios::FLUSHO),
    #[cfg(target_os = "macos")]
    (SuppressKernelStatus, FlagWord::Local, termios::NOKERNINFO),
    (PendingInput, FlagWord::Local, termios::PENDIN),
    (ExtendedProcessing, FlagWord::Local, termios::IEXTEN),
    (ExternalProcessing, FlagWord::Local, termios::EXTPROC),
];

impl TerminalFlag {
    fn representation(self) -> Option<(FlagWord, termios::FlagBits)> {
        FLAG_REPRESENTATIONS
            .iter()
            .find(|(flag, _, _)| *flag == self)
            .map(|(_, word, mask)| (*word, *mask))
    }

    /// Whether the active platform defines this behavior.
    #[must_use]
    pub fn is_supported(self) -> bool {
        self.representation().is_some()
    }
}

/// A terminal control character slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlCharacter {
    Interrupt,
    Quit,
    Erase,
    Kill,
    EndOfFile,
    Timeout,
    MinimumBytes,
    Start,
    Stop,
    Suspend,
    DeferredSuspend,
    Status,
    EndOfLine,
    Reprint,
    Discard,
    WordErase,
    LiteralNext,
    AlternateEndOfLine,
}

const CONTROL_CHARACTER_REPRESENTATIONS: &[(ControlCharacter, usize)] = &[
    (ControlCharacter::Interrupt, termios::VINTR),
    (ControlCharacter::Quit, termios::VQUIT),
    (ControlCharacter::Erase, termios::VERASE),
    (ControlCharacter::Kill, termios::VKILL),
    (ControlCharacter::EndOfFile, termios::VEOF),
    (ControlCharacter::Timeout, termios::VTIME),
    (ControlCharacter::MinimumBytes, termios::VMIN),
    (ControlCharacter::Start, termios::VSTART),
    (ControlCharacter::Stop, termios::VSTOP),
    (ControlCharacter::Suspend, termios::VSUSP),
    #[cfg(target_os = "macos")]
    (ControlCharacter::DeferredSuspend, termios::VDSUSP),
    #[cfg(target_os = "macos")]
    (ControlCharacter::Status, termios::VSTATUS),
    (ControlCharacter::EndOfLine, termios::VEOL),
    (ControlCharacter::Reprint, termios::VREPRINT),
    (ControlCharacter::Discard, termios::VDISCARD),
    (ControlCharacter::WordErase, termios::VWERASE),
    (ControlCharacter::LiteralNext, termios::VLNEXT),
    (ControlCharacter::AlternateEndOfLine, termios::VEOL2),
];

impl ControlCharacter {
    fn index(self) -> Option<usize> {
        CONTROL_CHARACTER_REPRESENTATIONS
            .iter()
            .find(|(character, _)| *character == self)
            .map(|(_, index)| *index)
    }

    /// Whether the active platform defines this control-character slot.
    #[must_use]
    pub fn is_supported(self) -> bool {
        self.index().is_some()
    }

    /// The platform's default value for this role.
    #[must_use]
    pub const fn default_value(self) -> u8 {
        match self {
            Self::Interrupt => termios::CINTR,
            Self::Quit => termios::CQUIT,
            Self::Erase => termios::CERASE,
            Self::Kill => termios::CKILL,
            Self::EndOfFile => termios::CEOF,
            Self::Timeout => termios::CTIME,
            Self::MinimumBytes => termios::CMIN,
            Self::Start => termios::CSTART,
            Self::Stop => termios::CSTOP,
            Self::Suspend => termios::CSUSP,
            Self::DeferredSuspend => termios::CDSUSP,
            Self::Status => termios::CSTATUS,
            Self::EndOfLine => termios::CEOL,
            Self::Reprint => termios::CREPRINT,
            Self::Discard => termios::CDISCARD,
            Self::WordErase => termios::CWERASE,
            Self::LiteralNext => termios::CLNEXT,
            Self::AlternateEndOfLine => termios::CEOL2,
        }
    }
}

/// A baud-rate observation that does not expose the platform's encoded bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputSpeed {
    BitsPerSecond(u32),
    Custom,
}

/// An opaque snapshot of terminal attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalAttributes(Termios);

impl TerminalAttributes {
    /// Test a semantic terminal behavior.
    #[must_use]
    pub fn flag(&self, flag: TerminalFlag) -> bool {
        let Some((word, mask)) = flag.representation() else {
            return false;
        };
        self.word(word) & mask == mask
    }

    /// Enable or disable a semantic terminal behavior.
    pub fn set_flag(&mut self, flag: TerminalFlag, enabled: bool) {
        let Some((word, mask)) = flag.representation() else {
            return;
        };
        let value = self.word_mut(word);
        if enabled {
            *value |= mask;
        } else {
            *value &= !mask;
        }
    }

    /// Read a terminal control character.
    #[must_use]
    pub fn control_character(&self, character: ControlCharacter) -> u8 {
        character
            .index()
            .map_or(termios::VDISABLE, |index| self.0.c_cc[index])
    }

    /// Set a terminal control character.
    pub fn set_control_character(&mut self, character: ControlCharacter, value: u8) {
        if let Some(index) = character.index() {
            self.0.c_cc[index] = value;
        }
    }

    /// The configured output speed, without exposing the platform encoding.
    #[must_use]
    pub const fn output_speed(&self) -> OutputSpeed {
        match termios::baud_rate(&self.0) {
            Some(rate) => OutputSpeed::BitsPerSecond(rate),
            None => OutputSpeed::Custom,
        }
    }

    /// Derive the ordinary interactive editing attributes.
    #[must_use]
    pub fn for_editing(mut self) -> Self {
        for flag in [
            TerminalFlag::MapNewlineToCarriageReturn,
            TerminalFlag::MapCarriageReturnToNewline,
            TerminalFlag::PostProcessOutput,
            TerminalFlag::MapNewlineToCarriageReturnNewline,
            TerminalFlag::GenerateSignals,
        ] {
            self.set_flag(flag, true);
        }
        for flag in [
            TerminalFlag::IgnoreCarriageReturn,
            TerminalFlag::NewlinePerformsCarriageReturn,
            TerminalFlag::DisableFlush,
            TerminalFlag::CanonicalInput,
            TerminalFlag::EchoInput,
            TerminalFlag::EchoKill,
            TerminalFlag::EchoNewline,
            TerminalFlag::ExternalProcessing,
            TerminalFlag::ExtendedProcessing,
            TerminalFlag::OutputBeingFlushed,
        ] {
            self.set_flag(flag, false);
        }
        for character in [
            ControlCharacter::EndOfFile,
            ControlCharacter::EndOfLine,
            ControlCharacter::AlternateEndOfLine,
            ControlCharacter::WordErase,
            ControlCharacter::Reprint,
            ControlCharacter::LiteralNext,
        ] {
            self.set_control_character(character, termios::VDISABLE);
        }
        self.set_control_character(ControlCharacter::MinimumBytes, 1);
        self.set_control_character(ControlCharacter::Timeout, 0);
        self
    }

    /// Derive attributes for reading the next input literally.
    #[must_use]
    pub fn for_quoted_input(mut self) -> Self {
        for flag in [
            TerminalFlag::EnableOutputFlowControl,
            TerminalFlag::EnableInputFlowControl,
            TerminalFlag::MapNewlineToCarriageReturn,
            TerminalFlag::MapCarriageReturnToNewline,
            TerminalFlag::GenerateSignals,
            TerminalFlag::ExtendedProcessing,
        ] {
            self.set_flag(flag, false);
        }
        self
    }

    fn word(&self, word: FlagWord) -> termios::FlagBits {
        match word {
            FlagWord::Input => self.0.c_iflag,
            FlagWord::Output => self.0.c_oflag,
            FlagWord::Control => self.0.c_cflag,
            FlagWord::Local => self.0.c_lflag,
        }
    }

    fn word_mut(&mut self, word: FlagWord) -> &mut termios::FlagBits {
        match word {
            FlagWord::Input => &mut self.0.c_iflag,
            FlagWord::Output => &mut self.0.c_oflag,
            FlagWord::Control => &mut self.0.c_cflag,
            FlagWord::Local => &mut self.0.c_lflag,
        }
    }
}

/// Read an opaque terminal attribute snapshot.
pub fn read_attributes(input: BorrowedFd<'_>) -> io::Result<TerminalAttributes> {
    termios::read(input).map(TerminalAttributes)
}

/// Apply an opaque terminal attribute snapshot.
pub fn apply_attributes(
    input: BorrowedFd<'_>,
    when: ApplyWhen,
    attributes: &TerminalAttributes,
) -> io::Result<()> {
    termios::apply(input, when.rustix(), &attributes.0)
}

/// Whether a descriptor refers to a terminal.
pub fn is_terminal(descriptor: BorrowedFd<'_>) -> io::Result<bool> {
    termios::is_terminal(descriptor)
}

/// An activated terminal's exactly-once restoration state.
///
/// The caller owns this value and decides whether explicit cleanup or its own
/// RAII owner invokes [`Self::restore`]. This type performs no cleanup in
/// `Drop`, so it can sit beneath a higher-level owner without competing for
/// the restoration obligation.
pub struct TerminalController<'fd> {
    input: BorrowedFd<'fd>,
    output: BorrowedFd<'fd>,
    original: Option<TerminalAttributes>,
    editing: Option<TerminalAttributes>,
    quoted: Option<TerminalAttributes>,
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
        if !is_terminal(self.output)? {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "editor output is not a terminal",
            ));
        }
        let original = read_attributes(self.input).map_err(|error| {
            io::Error::new(
                io::ErrorKind::NotConnected,
                format!("editor input: {error}"),
            )
        })?;
        let editing = original.for_editing();
        let quoted = editing.for_quoted_input();
        self.original = Some(original);
        self.editing = Some(editing);
        self.quoted = Some(quoted);
        self.restoration_due = true;
        apply_attributes(self.input, ApplyWhen::AfterOutput, &editing)
    }

    /// Restore normal terminal processing without consuming the obligation.
    pub fn enter_cooked_mode(&mut self) -> io::Result<()> {
        apply_optional(self.input, ApplyWhen::AfterOutput, self.original.as_ref())
    }

    /// Re-enter interactive editing mode.
    pub fn enter_editing_mode(&mut self) -> io::Result<()> {
        apply_optional(self.input, ApplyWhen::AfterOutput, self.editing.as_ref())
    }

    /// Temporarily read the next input unit without signal or flow handling.
    pub fn enter_quoted_mode(&mut self) -> io::Result<()> {
        apply_optional(self.input, ApplyWhen::AfterOutput, self.quoted.as_ref())
    }

    /// Consume and restore the state captured by [`Self::activate`].
    pub fn restore(&mut self) -> io::Result<()> {
        if !self.restoration_due {
            return Ok(());
        }
        self.restoration_due = false;
        apply_optional(
            self.input,
            ApplyWhen::AfterOutputAndDiscardInput,
            self.original.as_ref(),
        )
    }
}

/// Read a terminal's current `(rows, columns)` dimensions.
pub fn screen_size(output: BorrowedFd<'_>) -> io::Result<(usize, usize)> {
    termios::screen_size(output).map(|(rows, columns)| (usize::from(rows), usize::from(columns)))
}

/// Count bytes that can be read immediately without blocking.
pub fn bytes_ready(input: BorrowedFd<'_>) -> io::Result<u64> {
    rustix::io::ioctl_fionread(input).map_err(Into::into)
}

fn apply_optional(
    descriptor: BorrowedFd<'_>,
    when: ApplyWhen,
    attributes: Option<&TerminalAttributes>,
) -> io::Result<()> {
    attributes.map_or(Ok(()), |attributes| {
        apply_attributes(descriptor, when, attributes)
    })
}

#[cfg(test)]
mod tests {
    use std::os::fd::{AsFd, OwnedFd};

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

        fn attributes(&self) -> TerminalAttributes {
            read_attributes(self.terminal.as_fd()).expect("read terminal attributes")
        }
    }

    #[test]
    // [spec:nshedit:req:platform.typed-boundary/test]
    fn modes_restore_original_once() {
        let pty = Pty::open();
        let original = pty.attributes();
        let mut controller = TerminalController::new(pty.terminal.as_fd(), pty.terminal.as_fd());

        controller.activate().expect("activate");
        let editing = pty.attributes();
        assert!(!editing.flag(TerminalFlag::CanonicalInput));
        assert!(!editing.flag(TerminalFlag::EchoInput));
        assert!(editing.flag(TerminalFlag::GenerateSignals));
        assert_eq!(editing.control_character(ControlCharacter::MinimumBytes), 1);
        assert_eq!(editing.control_character(ControlCharacter::Timeout), 0);

        controller.enter_quoted_mode().expect("quoted mode");
        let quoted = pty.attributes();
        assert!(!quoted.flag(TerminalFlag::GenerateSignals));
        assert!(!quoted.flag(TerminalFlag::ExtendedProcessing));
        assert!(!quoted.flag(TerminalFlag::EnableOutputFlowControl));
        assert!(!quoted.flag(TerminalFlag::EnableInputFlowControl));
        assert!(!quoted.flag(TerminalFlag::MapNewlineToCarriageReturn));
        assert!(!quoted.flag(TerminalFlag::MapCarriageReturnToNewline));

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

    fn assert_same_attributes(actual: TerminalAttributes, expected: TerminalAttributes) {
        // XNU owns PENDIN as kernel state, so a tcsetattr/tcgetattr round trip
        // can change it independently of the attributes the caller restored.
        #[cfg(target_os = "macos")]
        let actual = {
            let mut actual = actual;
            actual.set_flag(TerminalFlag::PendingInput, false);
            actual
        };
        #[cfg(target_os = "macos")]
        let expected = {
            let mut expected = expected;
            expected.set_flag(TerminalFlag::PendingInput, false);
            expected
        };
        assert_eq!(actual, expected);
    }
}
