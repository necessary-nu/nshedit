//! A native editor session with one owner for terminal restoration.
//!
//! The editor owns its private domain state and terminal lifecycle, but not
//! its input and output streams. [`SessionIo`] stays with the driver so the
//! editor borrow can end before an effect performs host-controlled work.

use std::fmt;
use std::io::{self, Read, Write};
use std::os::fd::BorrowedFd;

use crate::domain::{EditorConfig, Text, TextIndex};

// [spec:nshedit:req:core.effect-hooks]
pub mod effect;

/// Safe terminal lifecycle operations owned by one editor session.
///
/// `activate` may partially change terminal state before it returns an error.
/// The editor therefore calls `restore` once after every failed activation.
/// Implementations must tolerate that call. For a successfully activated
/// session, the editor calls `restore` at most once, from explicit finish or
/// from `Drop`.
pub trait TerminalControl {
    /// Prepare the terminal for this session's editing policy.
    fn activate(&mut self, config: EditorConfig) -> io::Result<()>;

    /// Restore the state that preceded the activation attempt.
    fn restore(&mut self) -> io::Result<()>;
}

/// Borrowed descriptors associated with a session's safe streams.
///
/// A stream need not have a descriptor. When it does, [`BorrowedFd`] carries
/// the lifetime without transferring ownership or admitting an invalid raw
/// descriptor.
#[derive(Debug, Clone, Copy, Default)]
pub struct IoDescriptors<'a> {
    /// Descriptor borrowed from the input stream, when it has one.
    pub input: Option<BorrowedFd<'a>>,
    /// Descriptor borrowed from the normal output stream, when it has one.
    pub output: Option<BorrowedFd<'a>>,
    /// Descriptor borrowed from the diagnostic stream, when it has one.
    pub diagnostics: Option<BorrowedFd<'a>>,
}

// [spec:nshedit:req:core.rust-io+1]
/// The safe I/O capabilities used to drive an editor.
///
/// This value deliberately remains separate from [`Editor`]. A driver may
/// borrow the editor until it yields, end that borrow, and only then use these
/// streams or descriptors. C streams and foreign callbacks are adapted or
/// handled outside this module; they are never stored here.
pub struct SessionIo<'a> {
    /// Source of input bytes.
    pub input: &'a mut dyn Read,
    /// Destination for prompts, redisplay, and accepted echo.
    pub output: &'a mut dyn Write,
    /// Destination for diagnostics that are not part of the edited line.
    pub diagnostics: &'a mut dyn Write,
    /// Non-owning descriptors for operations that require a terminal handle.
    pub descriptors: IoDescriptors<'a>,
}

/// Failure to activate a new editor, including cleanup of a partial attempt.
#[derive(Debug)]
pub struct StartError {
    activation: io::Error,
    restoration: Option<io::Error>,
}

impl StartError {
    /// The error reported while preparing the terminal.
    #[must_use]
    pub fn activation(&self) -> &io::Error {
        &self.activation
    }

    /// A second error when cleanup of the partial activation also failed.
    #[must_use]
    pub fn restoration(&self) -> Option<&io::Error> {
        self.restoration.as_ref()
    }
}

impl fmt::Display for StartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not activate editor terminal: {}",
            self.activation
        )?;
        if let Some(restoration) = &self.restoration {
            write!(formatter, "; restoration also failed: {restoration}")?;
        }
        Ok(())
    }
}

impl std::error::Error for StartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.activation)
    }
}

#[derive(Debug)]
struct EditorState {
    line: Text,
    cursor: TextIndex,
}

// [spec:nshedit:req:core.raii-lifecycle]
/// A Rust-native editor with private state and an exactly-once cleanup owner.
pub struct Editor<T: TerminalControl> {
    config: EditorConfig,
    state: EditorState,
    terminal: Option<T>,
    effects: effect::Runtime,
}

impl<T: TerminalControl> Editor<T> {
    /// Activate a terminal and create an empty editing session.
    ///
    /// A failed activation is treated as partially applied: restoration is
    /// attempted once before the error returns.
    pub fn new(config: EditorConfig, mut terminal: T) -> Result<Self, StartError> {
        if let Err(activation) = terminal.activate(config) {
            let restoration = terminal.restore().err();
            return Err(StartError {
                activation,
                restoration,
            });
        }

        Ok(Self {
            config,
            state: EditorState {
                line: Text::default(),
                cursor: TextIndex::START,
            },
            terminal: Some(terminal),
            effects: effect::Runtime::default(),
        })
    }

    /// The immutable policy selected when this session was created.
    #[must_use]
    pub const fn config(&self) -> EditorConfig {
        self.config
    }

    /// The current logical line.
    #[must_use]
    pub fn line(&self) -> &Text {
        &self.state.line
    }

    /// The cursor boundary within [`Self::line`].
    #[must_use]
    pub const fn cursor(&self) -> TextIndex {
        self.state.cursor
    }

    /// Restore the terminal, reporting any failure to the caller.
    ///
    /// This consumes the editor. Its following `Drop` observes that the
    /// restoration obligation has already been taken and does nothing.
    pub fn finish(mut self) -> io::Result<()> {
        match self.terminal.take() {
            Some(mut terminal) => terminal.restore(),
            None => Ok(()),
        }
    }

    fn restore_terminal(&mut self) -> io::Result<()> {
        let Some(mut terminal) = self.terminal.take() else {
            return Ok(());
        };
        terminal.restore()
    }
}

impl<T: TerminalControl> Drop for Editor<T> {
    fn drop(&mut self) {
        // Drop cannot report an I/O error. Taking the controller before its
        // call makes this the one best-effort attempt even when it fails.
        let _ = self.restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs::File;
    use std::io::{Cursor, ErrorKind};
    use std::os::fd::AsFd;
    use std::rc::Rc;

    use super::*;
    use crate::domain::{Buffering, EditingMode};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        Activate(EditorConfig),
        Restore,
    }

    struct MockTerminal {
        events: Rc<RefCell<Vec<Event>>>,
        fail_activation: bool,
        fail_restoration: bool,
    }

    impl MockTerminal {
        fn recording(events: &Rc<RefCell<Vec<Event>>>) -> Self {
            Self {
                events: Rc::clone(events),
                fail_activation: false,
                fail_restoration: false,
            }
        }
    }

    impl TerminalControl for MockTerminal {
        fn activate(&mut self, config: EditorConfig) -> io::Result<()> {
            self.events.borrow_mut().push(Event::Activate(config));
            if self.fail_activation {
                Err(io::Error::other("activation failed"))
            } else {
                Ok(())
            }
        }

        fn restore(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push(Event::Restore);
            if self.fail_restoration {
                Err(io::Error::other("restoration failed"))
            } else {
                Ok(())
            }
        }
    }

    // [spec:nshedit:req:core.raii-lifecycle/test]
    #[test]
    fn finish_restores_exactly_once() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let config = EditorConfig::default()
            .with_editing_mode(EditingMode::Vi)
            .with_buffering(Buffering::Character);
        let editor = Editor::new(config, MockTerminal::recording(&events)).unwrap();

        assert_eq!(editor.config(), config);
        assert!(editor.line().is_empty());
        assert_eq!(editor.cursor(), TextIndex::START);
        editor.finish().unwrap();
        assert_eq!(*events.borrow(), [Event::Activate(config), Event::Restore]);
    }

    #[test]
    fn finish_reports_restoration_failure() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut terminal = MockTerminal::recording(&events);
        terminal.fail_restoration = true;
        let editor = Editor::new(EditorConfig::default(), terminal).unwrap();

        assert_eq!(editor.finish().unwrap_err().kind(), ErrorKind::Other);
        assert_eq!(
            events.borrow().as_slice(),
            [Event::Activate(EditorConfig::default()), Event::Restore]
        );
    }

    #[test]
    fn drop_ignores_restoration_failure() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut terminal = MockTerminal::recording(&events);
        terminal.fail_restoration = true;
        {
            let _editor = Editor::new(EditorConfig::default(), terminal).unwrap();
        }

        assert_eq!(
            events.borrow().as_slice(),
            [Event::Activate(EditorConfig::default()), Event::Restore]
        );
    }

    #[test]
    fn failed_start_restores_partial_state() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut terminal = MockTerminal::recording(&events);
        terminal.fail_activation = true;
        terminal.fail_restoration = true;

        let error = match Editor::new(EditorConfig::default(), terminal) {
            Ok(_) => panic!("failed terminal activation created an editor"),
            Err(error) => error,
        };
        assert_eq!(error.activation().kind(), ErrorKind::Other);
        assert_eq!(error.restoration().unwrap().kind(), ErrorKind::Other);
        assert_eq!(
            events.borrow().as_slice(),
            [Event::Activate(EditorConfig::default()), Event::Restore]
        );
    }

    #[test]
    fn repeated_cleanup_restores_once() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut editor =
            Editor::new(EditorConfig::default(), MockTerminal::recording(&events)).unwrap();

        editor.restore_terminal().unwrap();
        editor.restore_terminal().unwrap();
        drop(editor);
        assert_eq!(
            events.borrow().as_slice(),
            [Event::Activate(EditorConfig::default()), Event::Restore]
        );
    }

    // [spec:nshedit:req:core.rust-io+1/test]
    #[test]
    fn io_uses_safe_rust_capabilities() {
        let mut input = Cursor::new(b"x".to_vec());
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        let descriptor_file = File::open("/dev/null").unwrap();
        let descriptor = descriptor_file.as_fd();
        {
            let io = SessionIo {
                input: &mut input,
                output: &mut output,
                diagnostics: &mut diagnostics,
                descriptors: IoDescriptors {
                    input: Some(descriptor),
                    output: None,
                    diagnostics: None,
                },
            };

            let mut byte = [0];
            io.input.read_exact(&mut byte).unwrap();
            io.output.write_all(&byte).unwrap();
            io.output.flush().unwrap();
            io.diagnostics.write_all(b"diagnostic").unwrap();

            assert_eq!(byte, *b"x");
            assert!(io.descriptors.input.is_some());
        }
        assert_eq!(output, b"x");
        assert_eq!(diagnostics, b"diagnostic");
    }
}
