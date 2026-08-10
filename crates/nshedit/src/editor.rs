//! A native editor session with one owner for terminal restoration.
//!
//! The editor owns its private domain state and terminal lifecycle, but not
//! its input and output streams. [`SessionIo`](crate::editor::SessionIo) stays
//! with the driver so the editor borrow can end before an effect performs
//! host-controlled work.

use std::fmt;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::fd::BorrowedFd as BorrowedIo;
#[cfg(windows)]
use std::os::windows::io::BorrowedHandle as BorrowedIo;

use crate::domain::{
    Action, Binding, EditingMode, EditorConfig, Error, InputMode, KeyLookup, KeySequence,
    KeymapMode, Outcome, Prompt, Screen, ScreenPosition, ScreenSize, TerminalMode, Text, TextIndex,
    TextSpan,
};

// [spec:nshedit:req:core.effect-hooks]
pub mod effect;

mod driver;

// [spec:nshedit:req:core.token-completion+1]
mod completion;

// [spec:nshedit:req:core.line-commands]
mod line;

mod host;

// [spec:nshedit:req:core.terminal-render+1]
mod render;

mod system_input;
mod system_terminal;
mod token;

pub use completion::{
    CompletionCandidate, CompletionCandidates, CompletionEdit, CompletionOutcome, CompletionQuery,
};
pub use driver::{Display, DriverError, Pending, ReadDriver, ReadInterrupt, ReadResult, ReadStep};
pub use render::{BaudRate, CapabilityKind, RenderError, RenderSummary, TerminalProfile};
pub use system_input::SystemInput;
pub use system_terminal::SystemTerminal;
pub use token::{
    Continuation, QuoteStyle, Token, TokenCursor, TokenIndex, TokenOffset, Tokenization,
    TokenizedLine, Tokenizer,
};

/// Safe terminal lifecycle operations owned by one editor session.
///
/// `activate` may partially change terminal state before it returns an error.
/// The editor therefore calls `restore` once after every failed activation.
/// Implementations must tolerate that call. For a successfully activated
/// session, the editor calls `restore` at most once, from explicit finish or
/// from `Drop`.
pub trait TerminalControl {
    /// Prepare the terminal for this session's editing policy and enter
    /// [`TerminalMode::Editing`].
    fn activate(&mut self, config: EditorConfig) -> io::Result<()>;

    /// Transition the active tty between semantic input modes. A failed call
    /// must leave the previously committed mode usable.
    fn set_mode(&mut self, mode: TerminalMode) -> io::Result<()>;

    /// Restore the state that preceded the activation attempt.
    fn restore(&mut self) -> io::Result<()>;
}

/// Borrowed descriptors associated with a session's safe streams.
///
/// A stream need not expose an operating-system I/O object. When it does, the
/// borrowed descriptor or handle carries its lifetime without transferring
/// ownership or admitting an invalid raw value.
#[derive(Debug, Clone, Copy, Default)]
pub struct IoDescriptors<'a> {
    /// Descriptor borrowed from the input stream, when it has one.
    pub input: Option<BorrowedIo<'a>>,
    /// Descriptor borrowed from the normal output stream, when it has one.
    pub output: Option<BorrowedIo<'a>>,
    /// Descriptor borrowed from the diagnostic stream, when it has one.
    pub diagnostics: Option<BorrowedIo<'a>>,
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

// [spec:nshedit:req:core.raii-lifecycle]
/// A Rust-native editor with private state and an exactly-once cleanup owner.
pub struct Editor<T: TerminalControl> {
    config: EditorConfig,
    state: line::State,
    terminal: Option<T>,
    terminal_mode: TerminalMode,
    renderer: render::State,
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
            state: line::State::new(config),
            terminal: Some(terminal),
            terminal_mode: TerminalMode::Editing,
            renderer: render::State::default(),
        })
    }

    /// The policy currently governing this session.
    #[must_use]
    pub const fn config(&self) -> EditorConfig {
        self.config
    }

    /// Change the session policy without rebuilding its line state.
    ///
    /// Switching editing families selects that family's insertion keymap.
    /// The line, cursor, registers, undo history, and custom bindings remain
    /// owned by this editor. Signal and buffering policy are observed by the
    /// read driver on its next step.
    pub fn reconfigure(&mut self, config: EditorConfig) {
        if self.config.editing_mode() != config.editing_mode() {
            self.state.select_editing_mode(config.editing_mode());
        }
        self.config = config;
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

    /// The insertion or replacement policy for following text.
    #[must_use]
    pub const fn input_mode(&self) -> InputMode {
        self.state.input_mode()
    }

    /// The keymap currently used by input dispatch.
    #[must_use]
    pub const fn keymap_mode(&self) -> KeymapMode {
        self.state.keymap_mode()
    }

    /// The checked saved mark, when one has been set.
    #[must_use]
    pub const fn mark(&self) -> Option<TextIndex> {
        self.state.mark()
    }

    /// The text most recently copied or killed.
    #[must_use]
    pub fn kill_buffer(&self) -> Option<&Text> {
        self.state.kill_buffer()
    }

    /// The exact logical-text pattern remembered for repeat search.
    #[must_use]
    pub fn search_pattern(&self) -> Option<&Text> {
        self.state.search_pattern()
    }

    /// Whether an earlier command-level text mutation can be restored.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.state.can_undo()
    }

    /// Whether a previously undone text mutation can be reapplied.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.state.can_redo()
    }

    /// Apply one typed semantic action to the private editor state.
    pub fn execute(&mut self, action: Action) -> Result<Outcome, Error> {
        self.state.execute(action)
    }

    fn begin_edit_group(&mut self) {
        self.state.begin_edit_group();
    }

    fn finish_edit_group(&mut self) {
        self.state.finish_edit_group();
    }

    fn finish_all_edit_groups(&mut self) {
        self.state.finish_all_edit_groups();
    }

    fn motion_destination(
        &self,
        motion: crate::domain::Motion,
        count: usize,
    ) -> Result<TextIndex, Error> {
        self.state.motion_destination(motion, count)
    }

    /// Replace a checked portion of the line as one undoable edit.
    pub fn replace(&mut self, span: TextSpan, replacement: Text) -> Result<(), Error> {
        self.state
            .replace_at(replacement, span.start().get(), span.end().get())
            .map(|_| ())
    }

    fn restore_history_line(&mut self, line: Text) -> Result<(), Error> {
        self.state
            .restore_history(line, self.config.editing_mode() == EditingMode::Vi)
    }

    /// Begin a fresh logical line while preserving session policy, bindings,
    /// and long-lived registers.
    pub fn reset_line(&mut self) {
        self.state.reset_line(self.config);
    }

    /// Install or replace a typed binding in one keymap.
    pub fn bind(
        &mut self,
        mode: KeymapMode,
        sequence: KeySequence,
        binding: Binding,
    ) -> Option<Binding> {
        self.state.bind(mode, sequence, binding)
    }

    /// Remove a typed binding from one keymap.
    pub fn unbind(&mut self, mode: KeymapMode, sequence: &KeySequence) -> Option<Binding> {
        self.state.unbind(mode, sequence)
    }

    /// Restore the built-in maps and select an editing family.
    pub fn reset_bindings(&mut self, mode: EditingMode) {
        self.state.reset_bindings(mode);
        self.config = self.config.with_editing_mode(mode);
    }

    /// Remove every binding from a selected map without changing modes.
    pub fn clear_bindings(&mut self, mode: KeymapMode) {
        self.state.clear_bindings(mode);
    }

    /// Inspect an exact binding in a selected keymap without activating it.
    #[must_use]
    pub fn binding(&self, mode: KeymapMode, sequence: &KeySequence) -> Option<&Binding> {
        self.state.binding(mode, sequence)
    }

    /// Iterate over a selected keymap in logical sequence order.
    pub fn bindings(&self, mode: KeymapMode) -> impl Iterator<Item = (&KeySequence, &Binding)> {
        self.state.bindings(mode)
    }

    /// Match a non-empty logical sequence against the active keymap.
    #[must_use]
    pub fn key_binding(&self, sequence: &KeySequence) -> KeyLookup<'_> {
        self.state.lookup(sequence)
    }

    /// The tty mode currently committed by this editor.
    #[must_use]
    pub const fn terminal_mode(&self) -> TerminalMode {
        self.terminal_mode
    }

    /// Change tty mode transactionally through the owned controller.
    ///
    /// A failed controller operation leaves the editor's committed mode
    /// unchanged.
    pub fn set_terminal_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
        if mode == self.terminal_mode {
            return Ok(());
        }
        let terminal = self
            .terminal
            .as_mut()
            .ok_or_else(|| io::Error::other("editor terminal is no longer active"))?;
        terminal.set_mode(mode)?;
        self.terminal_mode = mode;
        Ok(())
    }

    /// Install an owned terminal profile without abandoning an established
    /// physical editor region.
    pub fn configure_display(&mut self, profile: TerminalProfile, size: ScreenSize) {
        self.renderer.configure(profile, size);
    }

    /// Resize the configured display and repair its owned rows on the next
    /// render.
    pub fn resize_display(&mut self, size: ScreenSize) -> Result<(), RenderError> {
        self.renderer.resize(size)
    }

    /// The terminal profile currently driving native rendering.
    #[must_use]
    pub fn terminal_profile(&self) -> Option<&TerminalProfile> {
        self.renderer.profile()
    }

    /// The last screen image committed after a successful flush.
    #[must_use]
    pub fn screen(&self) -> Option<&Screen> {
        self.renderer.screen()
    }

    /// The cursor in the last successfully committed screen image.
    #[must_use]
    pub fn screen_cursor(&self) -> Option<ScreenPosition> {
        self.renderer.cursor()
    }

    /// Render the current private line and cursor through a safe writer.
    pub fn render_to(
        &mut self,
        left_prompt: &Prompt,
        right_prompt: Option<&Prompt>,
        output: &mut dyn Write,
    ) -> Result<RenderSummary, RenderError> {
        self.renderer.present(
            left_prompt,
            right_prompt,
            &self.state.line,
            self.state.cursor,
            output,
        )
    }

    /// Emit the configured terminal's notification capability.
    pub fn beep(&mut self, output: &mut dyn Write) -> Result<usize, RenderError> {
        self.renderer.beep(output)
    }

    fn request_redraw(&mut self) {
        self.renderer.redraw();
    }

    fn invalidate_display(&mut self) {
        self.renderer.damage();
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
    #[cfg(unix)]
    use std::os::fd::AsFd;
    #[cfg(windows)]
    use std::os::windows::io::AsHandle;
    use std::rc::Rc;

    use super::*;
    use crate::domain::{Buffering, EditingMode, TerminalLiteral};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Event {
        Activate(EditorConfig),
        SetMode(TerminalMode),
        Restore,
    }

    struct MockTerminal {
        events: Rc<RefCell<Vec<Event>>>,
        fail_activation: bool,
        fail_mode: bool,
        fail_restoration: bool,
    }

    impl MockTerminal {
        fn recording(events: &Rc<RefCell<Vec<Event>>>) -> Self {
            Self {
                events: Rc::clone(events),
                fail_activation: false,
                fail_mode: false,
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

        fn set_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
            self.events.borrow_mut().push(Event::SetMode(mode));
            if self.fail_mode {
                Err(io::Error::other("mode change failed"))
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

    #[test]
    fn tty_modes_are_typed_and_transactional() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut editor =
            Editor::new(EditorConfig::default(), MockTerminal::recording(&events)).unwrap();

        assert_eq!(editor.terminal_mode(), TerminalMode::Editing);
        editor.set_terminal_mode(TerminalMode::Quoted).unwrap();
        editor.set_terminal_mode(TerminalMode::Quoted).unwrap();
        assert_eq!(editor.terminal_mode(), TerminalMode::Quoted);
        assert_eq!(
            events.borrow().as_slice(),
            [
                Event::Activate(EditorConfig::default()),
                Event::SetMode(TerminalMode::Quoted)
            ]
        );

        let failed_events = Rc::new(RefCell::new(Vec::new()));
        let mut terminal = MockTerminal::recording(&failed_events);
        terminal.fail_mode = true;
        let mut failed = Editor::new(EditorConfig::default(), terminal).unwrap();
        assert!(failed.set_terminal_mode(TerminalMode::Cooked).is_err());
        assert_eq!(failed.terminal_mode(), TerminalMode::Editing);
    }

    #[test]
    fn reconfiguration_preserves_session_state() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut editor =
            Editor::new(EditorConfig::default(), MockTerminal::recording(&events)).unwrap();
        editor.execute(Action::Insert(Text::from("line"))).unwrap();
        editor.execute(Action::SetMark).unwrap();

        let config = EditorConfig::default()
            .with_editing_mode(EditingMode::Vi)
            .with_buffering(Buffering::Character);
        editor.reconfigure(config);

        assert_eq!(editor.config(), config);
        assert_eq!(editor.line(), &Text::from("line"));
        let end = editor.line().index(4).unwrap();
        assert_eq!(editor.cursor(), end);
        assert_eq!(editor.mark(), Some(end));
        assert_eq!(editor.input_mode(), InputMode::Insert);
        assert_eq!(editor.keymap_mode(), KeymapMode::ViInsert);
        assert!(editor.can_undo());
    }

    // [spec:nshedit:req:core.terminal-render+1/test]
    #[test]
    fn editor_owns_its_render_state() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut editor =
            Editor::new(EditorConfig::default(), MockTerminal::recording(&events)).unwrap();
        editor.configure_display(TerminalProfile::ansi(), ScreenSize::new(2, 12).unwrap());
        editor.execute(Action::Insert(Text::from("line"))).unwrap();
        let mut prompt = Prompt::from("p> ");
        prompt.push_literal(TerminalLiteral::from(&b"\x1b[1m"[..]));
        let mut output = Vec::new();

        let summary = editor.render_to(&prompt, None, &mut output).unwrap();

        assert_eq!(summary.cursor().column(), 7);
        assert_eq!(editor.screen_cursor(), Some(summary.cursor()));
        assert!(output.windows(7).any(|bytes| bytes == b"p> \x1b[1m"));
    }

    #[test]
    fn completion_query_and_candidates_are_owned() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut editor =
            Editor::new(EditorConfig::default(), MockTerminal::recording(&events)).unwrap();
        editor.execute(Action::Insert(Text::from("ec"))).unwrap();
        let query = editor.completion_query(&Tokenizer::default()).unwrap();
        let candidates = vec![CompletionCandidate::new("echo").with_suffix(" ")].into();

        editor.apply_completion(&query, candidates).unwrap();
        assert_eq!(editor.line(), &Text::from("echo "));
    }

    // [spec:nshedit:req:core.rust-io+1/test]
    #[test]
    fn io_uses_safe_rust_capabilities() {
        let mut input = Cursor::new(b"x".to_vec());
        let mut output = Vec::new();
        let mut diagnostics = Vec::new();
        #[cfg(unix)]
        let descriptor_file = File::open("/dev/null").unwrap();
        #[cfg(windows)]
        let descriptor_file = File::open("NUL").unwrap();
        #[cfg(unix)]
        let descriptor = descriptor_file.as_fd();
        #[cfg(windows)]
        let descriptor = descriptor_file.as_handle();
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
