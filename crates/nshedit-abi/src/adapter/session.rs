//! Session policy, callback, command, and native-editor adaptation.

use super::*;

mod signal;

impl EditLine {
    pub(crate) fn new(init: SessionInit<'_>) -> Result<Box<Self>, SessionInitError> {
        let SessionInit { program, streams } = init;
        let program = CString::new(program).map_err(SessionInitError::ProgramName)?;
        let terminal_name = secure_environment("TERM")
            .and_then(|name| CString::new(name).ok())
            .unwrap_or_else(|| c"dumb".to_owned());
        let (terminal, terminal_state) =
            AbiTerminal::new(streams.input.descriptor, streams.output.descriptor);
        let config = EditorConfig::default().with_signal_policy(SignalPolicy::Ignore);
        let mut session_editor =
            Editor::new(config, terminal).map_err(SessionInitError::Terminal)?;
        let _ = session_editor.execute(Action::SetMark);
        let _ = session_editor.set_terminal_mode(TerminalMode::Cooked);
        let lookup = nshterm::TermInfo::from_name(terminal_name.to_str().unwrap_or("dumb"));
        let window_size = with_borrowed_descriptor(streams.input.descriptor, terminal::screen_size)
            .and_then(Result::ok)
            .filter(|(rows, columns)| *rows != 0 && *columns != 0);
        let terminal_capabilities = TerminalCapabilities::new(
            terminal_name.to_str().unwrap_or("dumb"),
            lookup.as_ref().ok(),
            window_size,
        );
        let rows = terminal_capabilities.rows;
        let columns = terminal_capabilities.columns;
        let profile = terminal_capabilities.profile(None);
        let size = ScreenSize::new(rows, columns).map_err(SessionInitError::Display)?;
        session_editor.configure_display(profile, size);
        let mut editor = Box::new(Self {
            editor: session_editor,
            driver: ReadDriver::default(),
            boundary: EditLineBoundary::new(
                program,
                streams,
                terminal_state,
                terminal_name.clone(),
                terminal_capabilities,
            ),
        });
        if let Err(error) = &lookup {
            editor.report_terminal_lookup_failure(terminal_name.as_c_str(), error);
        }
        editor.initialize_terminal_bindings();
        editor.reset_bindings(EditingMode::Emacs);
        Ok(editor)
    }

    pub(crate) fn editor(&self) -> &Editor<AbiTerminal> {
        &self.editor
    }

    pub(crate) fn editor_mut(&mut self) -> &mut Editor<AbiTerminal> {
        &mut self.editor
    }

    pub(crate) fn split_editor_driver(&mut self) -> (&mut Editor<AbiTerminal>, &mut ReadDriver) {
        (&mut self.editor, &mut self.driver)
    }

    pub(crate) fn reset_line(&mut self) {
        self.editor.reset_line();
        let _ = self.editor.execute(Action::SetMark);
        self.boundary.history.depth = 0;
        self.boundary.history.live_line.clear();
    }

    pub(crate) fn reconfigure(&mut self) {
        let config = EditorConfig::default()
            .with_editing_mode(if self.editor_is_vi() {
                EditingMode::Vi
            } else {
                EditingMode::Emacs
            })
            .with_signal_policy(self.boundary.policy.signals)
            .with_buffering(self.boundary.policy.buffering);
        self.editor.reconfigure(config);
    }

    pub(crate) fn set_editor(&mut self, mode: EditingMode) {
        self.reset_bindings(mode);
        self.boundary.word_characters = None;
    }

    pub(crate) fn editor_is_vi(&self) -> bool {
        self.editor.config().editing_mode() == EditingMode::Vi
    }

    pub(crate) fn program(&self) -> &std::ffi::CStr {
        &self.boundary.program
    }

    pub(crate) fn handle_signals(&self) -> bool {
        self.boundary.policy.signals == SignalPolicy::Handle
    }

    pub(crate) fn editing_enabled(&self) -> bool {
        self.boundary.policy.availability == EditingAvailability::Enabled
    }

    pub(crate) fn set_editing_enabled(&mut self, enabled: bool) {
        self.boundary.policy.availability = if enabled {
            EditingAvailability::Enabled
        } else {
            EditingAvailability::Disabled
        };
    }

    pub(crate) fn unbuffered(&self) -> bool {
        self.boundary.policy.buffering == Buffering::Command
    }

    pub(crate) fn set_unbuffered(&mut self, enabled: bool) {
        self.boundary.policy.buffering = if enabled {
            Buffering::Command
        } else {
            Buffering::Line
        };
        self.reconfigure();
    }

    /// Enter or leave unbuffered reading, running the transition the mode
    /// change implies.
    ///
    /// A 0 -> non-zero transition arms the persistent signal handlers, resets
    /// the line and takes the terminal into editing mode; the reverse returns
    /// it to cooked mode and disarms them. Setting the mode it already holds
    /// does nothing. The flag is written *before* either sequence runs, and
    /// both sequences read it.
    // [spec:nshedit:req:abi.rust-internals]
    pub(crate) fn set_unbuffered_reading(&mut self, enabled: bool) {
        let was = self.unbuffered();
        if enabled && !was {
            self.set_unbuffered(true);
            if self.handle_signals() {
                let _ = self.arm_persistent_signal_handlers();
            }
            self.reset_line();
            if self.editing_enabled() {
                let _ = self.set_terminal_mode(TerminalMode::Editing);
            }
        } else if !enabled && was {
            self.set_unbuffered(false);
            let _ = self.set_terminal_mode(TerminalMode::Cooked);
            let _ = self.disarm_persistent_signal_handlers();
        }
    }

    pub(crate) fn safe_read(&self) -> bool {
        self.boundary.policy.interrupted_read == InterruptedRead::Retry
    }

    pub(crate) fn set_safe_read(&mut self, enabled: bool) {
        self.boundary.policy.interrupted_read = if enabled {
            InterruptedRead::Retry
        } else {
            InterruptedRead::Report
        };
    }

    pub(crate) fn history_encoding(&self) -> HistoryEncoding {
        self.boundary.history.encoding
    }

    pub(crate) fn published_line_encoding(&self) -> BoundaryEncoding {
        self.boundary.lines.published
    }

    pub(crate) fn set_published_line_encoding(&mut self, encoding: BoundaryEncoding) {
        self.boundary.lines.published = encoding;
    }

    pub(crate) fn set_prompt_wide(
        &mut self,
        side: PromptSide,
        callback: Option<WidePromptCallback>,
        escape: u32,
    ) {
        let callback = callback.unwrap_or(match side {
            PromptSide::Left => default_left_prompt,
            PromptSide::Right => default_right_prompt,
        });
        self.boundary.prompts.set(
            side,
            PromptSpec {
                callback: PromptCallback::Wide(callback),
                escape,
            },
        );
    }

    pub(crate) fn set_prompt_narrow(
        &mut self,
        side: PromptSide,
        callback: Option<NarrowPromptCallback>,
        escape: u32,
    ) {
        let callback = callback.unwrap_or(match side {
            PromptSide::Left => default_left_prompt_narrow,
            PromptSide::Right => default_right_prompt_narrow,
        });
        self.boundary.prompts.set(
            side,
            PromptSpec {
                callback: PromptCallback::Narrow(callback),
                escape,
            },
        );
    }

    pub(crate) fn prompt(&self, side: PromptSide) -> (PromptCallback, u32) {
        let prompt = self.boundary.prompts.get(side);
        (prompt.callback, prompt.escape)
    }

    pub(crate) fn set_resize_callback(
        &mut self,
        callback: Option<ResizeCallback>,
        cookie: *mut c_void,
    ) {
        self.boundary.callbacks.resize =
            callback.map(|callback| CallbackRegistration { callback, cookie });
    }

    pub(crate) fn resize_callback(&self) -> Option<CallbackRegistration<ResizeCallback>> {
        self.boundary.callbacks.resize
    }

    pub(crate) fn set_alias_callback(
        &mut self,
        callback: Option<AliasCallback>,
        cookie: *mut c_void,
    ) {
        self.boundary.callbacks.alias =
            callback.map(|callback| CallbackRegistration { callback, cookie });
    }

    pub(crate) fn alias_callback(&self) -> Option<CallbackRegistration<AliasCallback>> {
        self.boundary.callbacks.alias
    }

    pub(crate) fn set_read_callback(&mut self, callback: Option<ReadCallback>) {
        self.boundary.callbacks.read = callback;
    }

    pub(crate) fn read_callback(&self) -> Option<ReadCallback> {
        self.boundary.callbacks.read
    }

    pub(crate) fn set_history_callback(
        &mut self,
        callback: Option<HistoryCallback>,
        cookie: *mut c_void,
        encoding: HistoryEncoding,
    ) -> Result<(), HistoryRegistrationError> {
        if callback.is_none() && !cookie.is_null() {
            return Err(HistoryRegistrationError::CallbackMissing);
        }
        self.boundary.history.source =
            callback.map(|callback| HistorySource::new(callback, cookie, encoding));
        self.boundary.history.encoding = encoding;
        Ok(())
    }

    pub(crate) fn history_source(&self) -> Option<HistorySource> {
        self.boundary.history.source
    }

    pub(crate) fn history_depth(&self) -> usize {
        self.boundary.history.depth
    }

    pub(crate) fn set_history_depth(&mut self, depth: usize) {
        self.boundary.history.depth = depth;
    }

    pub(crate) fn save_history_live_line(&mut self) {
        self.boundary.history.live_line = self.editor.line().clone();
    }

    pub(crate) fn history_live_line(&self) -> &Text {
        &self.boundary.history.live_line
    }

    pub(crate) fn begin_completion(&mut self) -> CompletionInvocation {
        let invocation = self.boundary.completion.next;
        self.boundary.completion.next = CompletionInvocation::List;
        invocation
    }

    pub(crate) fn clear_completion_pending_listing(&mut self) {
        self.boundary.completion.next = CompletionInvocation::Insert;
    }

    pub(crate) fn set_environment_callback(&mut self, callback: Option<EnvironmentCallback>) {
        self.boundary.callbacks.environment = callback;
    }

    pub(crate) fn environment_callback(&self) -> Option<EnvironmentCallback> {
        self.boundary.callbacks.environment
    }

    pub(crate) fn set_client_data(&mut self, data: *mut c_void) {
        self.boundary.client_data = data;
    }

    pub(crate) fn client_data(&self) -> *mut c_void {
        self.boundary.client_data
    }

    pub(crate) fn set_word_characters(&mut self, characters: &[u32]) {
        let policy = characters
            .iter()
            .copied()
            .map(TextUnit::from_code_point)
            .collect();
        self.editor.set_word_policy(WordPolicy::new(policy));
        let mut characters = characters.to_vec();
        characters.push(0);
        self.boundary.word_characters = Some(characters);
    }

    pub(crate) fn word_characters(&self) -> Option<&[u32]> {
        self.boundary
            .word_characters
            .as_deref()
            .map(|characters| &characters[..characters.len().saturating_sub(1)])
    }

    pub(crate) fn add_command(
        &mut self,
        name: &[u32],
        help: &[u32],
        callback: CommandCallback,
    ) -> bool {
        let Some(name) = wide_string(name) else {
            return false;
        };
        let Ok(name) = CommandName::new(name) else {
            return false;
        };
        self.boundary.commands.push(HostCommand {
            name,
            callback,
            help: help
                .iter()
                .copied()
                .map(TextUnit::from_code_point)
                .collect(),
        });
        true
    }

    pub(crate) fn command_callback(&self, name: &CommandName) -> Option<CommandCallback> {
        self.boundary
            .commands
            .iter()
            .find(|command| command.name == *name)
            .map(|command| command.callback)
    }

    pub(crate) fn bind_byte_to_insert(&mut self, byte: u8) {
        let unit = TextUnit::Scalar(char::from(byte));
        let text = Text::from_iter([unit]);
        if let Ok(key) = KeySequence::new(text) {
            self.editor.bind(
                self.editor.keymap_mode(),
                key,
                Binding::Action(Action::Insert(Text::from_iter([unit]))),
            );
        }
    }
}

#[cfg(test)]
mod tests;
