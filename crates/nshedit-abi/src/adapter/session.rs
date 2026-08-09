//! Session policy, callback, command, and native-editor adaptation.

use super::*;

impl EditLine {
    pub(crate) fn new(
        program: &str,
        input: CFile,
        output: CFile,
        diagnostics: CFile,
        input_descriptor: c_int,
        output_descriptor: c_int,
        diagnostics_descriptor: c_int,
    ) -> Option<Box<Self>> {
        let program = CString::new(program).ok()?;
        let terminal_name = secure_environment("TERM")
            .and_then(|name| CString::new(name).ok())
            .unwrap_or_else(|| c"dumb".to_owned());
        let (terminal, terminal_state) = AbiTerminal::new(input_descriptor, output_descriptor);
        let config = EditorConfig::default().with_signal_policy(SignalPolicy::Ignore);
        let mut native = Editor::new(config, terminal).ok()?;
        let _ = native.set_terminal_mode(TerminalMode::Cooked);
        let profile = nshterm::TermInfo::from_name(terminal_name.to_str().unwrap_or("dumb"))
            .map_or_else(
                |_| TerminalProfile::plain(),
                |entry| TerminalProfile::from_terminfo(&entry),
            );
        let (rows, columns) = termios::window_size(input_descriptor)
            .map(|(rows, columns)| (usize::from(rows), usize::from(columns)))
            .filter(|(rows, columns)| *rows != 0 && *columns != 0)
            .unwrap_or((24, 80));
        let size = ScreenSize::new(rows, columns).ok()?;
        native.configure_display(profile, size);
        Some(Box::new(Self {
            native,
            driver: ReadDriver::default(),
            boundary: EditLineBoundary::new(
                program,
                Streams {
                    files: [input, output, diagnostics],
                    descriptors: [input_descriptor, output_descriptor, diagnostics_descriptor],
                },
                terminal_state,
                terminal_name,
            ),
        }))
    }

    pub(crate) fn native(&self) -> &Editor<AbiTerminal> {
        &self.native
    }

    pub(crate) fn native_mut(&mut self) -> &mut Editor<AbiTerminal> {
        &mut self.native
    }

    pub(crate) fn split_driver(&mut self) -> (&mut Editor<AbiTerminal>, &mut ReadDriver) {
        (&mut self.native, &mut self.driver)
    }

    pub(crate) fn reset_line(&mut self) {
        self.native.reset_line();
        self.boundary.history_depth = 0;
    }

    pub(crate) fn reconfigure(&mut self) {
        let config = EditorConfig::default()
            .with_editing_mode(if self.editor_is_vi() {
                EditingMode::Vi
            } else {
                EditingMode::Emacs
            })
            .with_signal_policy(if self.boundary.policy.handle_signals {
                SignalPolicy::Handle
            } else {
                SignalPolicy::Ignore
            })
            .with_buffering(if self.boundary.policy.unbuffered {
                Buffering::Character
            } else {
                Buffering::Line
            });
        self.native.reconfigure(config);
    }

    pub(crate) fn set_editor(&mut self, mode: EditingMode) {
        let config = self.native.config().with_editing_mode(mode);
        self.native.reconfigure(config);
        self.boundary.word_characters = None;
    }

    pub(crate) fn editor_is_vi(&self) -> bool {
        self.native.config().editing_mode() == EditingMode::Vi
    }

    pub(crate) fn program(&self) -> &std::ffi::CStr {
        &self.boundary.program
    }

    pub(crate) fn handle_signals(&self) -> bool {
        self.boundary.policy.handle_signals
    }

    pub(crate) fn set_handle_signals(&mut self, enabled: bool) {
        self.boundary.policy.handle_signals = enabled;
        self.reconfigure();
    }

    pub(crate) fn editing_enabled(&self) -> bool {
        self.boundary.policy.editing_enabled
    }

    pub(crate) fn set_editing_enabled(&mut self, enabled: bool) {
        self.boundary.policy.editing_enabled = enabled;
    }

    pub(crate) fn unbuffered(&self) -> bool {
        self.boundary.policy.unbuffered
    }

    pub(crate) fn set_unbuffered(&mut self, enabled: bool) {
        self.boundary.policy.unbuffered = enabled;
        self.reconfigure();
    }

    pub(crate) fn safe_read(&self) -> bool {
        self.boundary.policy.safe_read
    }

    pub(crate) fn set_safe_read(&mut self, enabled: bool) {
        self.boundary.policy.safe_read = enabled;
    }

    pub(crate) fn narrow_history(&self) -> bool {
        self.boundary.policy.narrow_history
    }

    pub(crate) fn set_narrow_history(&mut self, enabled: bool) {
        self.boundary.policy.narrow_history = enabled;
    }

    pub(crate) fn publishing_narrow_line(&self) -> bool {
        self.boundary.policy.publishing_narrow_line
    }

    pub(crate) fn set_publishing_narrow_line(&mut self, active: bool) {
        self.boundary.policy.publishing_narrow_line = active;
    }

    pub(crate) fn set_prompt_wide(
        &mut self,
        right: bool,
        callback: Option<WidePromptCallback>,
        escape: u32,
    ) {
        let callback = callback.unwrap_or(if right {
            default_right_prompt
        } else {
            default_left_prompt
        });
        self.boundary.prompts[usize::from(right)] = PromptSpec {
            callback: PromptCallback::Wide(callback),
            escape,
        };
    }

    pub(crate) fn set_prompt_narrow(
        &mut self,
        right: bool,
        callback: Option<NarrowPromptCallback>,
        escape: u32,
    ) {
        let callback = callback.unwrap_or_else(|| {
            // C function pointers of these two signatures have the same
            // representation; the prompt's width tag decides how to read
            // the returned storage.
            unsafe {
                core::mem::transmute::<WidePromptCallback, NarrowPromptCallback>(if right {
                    default_right_prompt
                } else {
                    default_left_prompt
                })
            }
        });
        self.boundary.prompts[usize::from(right)] = PromptSpec {
            callback: PromptCallback::Narrow(callback),
            escape,
        };
    }

    pub(crate) fn prompt_wide(&self, right: bool) -> (WidePromptCallback, u32) {
        let prompt = self.boundary.prompts[usize::from(right)];
        let callback = match prompt.callback {
            PromptCallback::Wide(callback) => callback,
            PromptCallback::Narrow(callback) => {
                // See [`Self::set_prompt_narrow`].
                unsafe {
                    core::mem::transmute::<NarrowPromptCallback, WidePromptCallback>(callback)
                }
            }
        };
        (callback, prompt.escape)
    }

    pub(crate) fn prompt_narrow(&self, right: bool) -> (NarrowPromptCallback, u32) {
        let prompt = self.boundary.prompts[usize::from(right)];
        let callback = match prompt.callback {
            PromptCallback::Narrow(callback) => callback,
            PromptCallback::Wide(callback) => {
                // See [`Self::set_prompt_narrow`].
                unsafe {
                    core::mem::transmute::<WidePromptCallback, NarrowPromptCallback>(callback)
                }
            }
        };
        (callback, prompt.escape)
    }

    pub(crate) fn prompt_callback(&self, right: bool) -> (PromptCallback, u32) {
        let prompt = self.boundary.prompts[usize::from(right)];
        (prompt.callback, prompt.escape)
    }

    pub(crate) fn set_resize_callback(
        &mut self,
        callback: Option<ResizeCallback>,
        cookie: *mut c_void,
    ) {
        self.boundary.callbacks.resize = callback.map(|callback| (callback, cookie));
    }

    pub(crate) fn resize_callback(&self) -> Option<(ResizeCallback, *mut c_void)> {
        self.boundary.callbacks.resize
    }

    pub(crate) fn set_alias_callback(
        &mut self,
        callback: Option<AliasCallback>,
        cookie: *mut c_void,
    ) {
        self.boundary.callbacks.alias = callback.map(|callback| (callback, cookie));
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
        narrow: bool,
    ) -> bool {
        if callback.is_none() && !cookie.is_null() {
            return false;
        }
        self.boundary.callbacks.history = callback.map(|callback| (callback, cookie));
        self.boundary.policy.narrow_history = narrow;
        self.boundary.history_depth = 0;
        true
    }

    pub(crate) fn history_callback(&self) -> Option<(HistoryCallback, *mut c_void)> {
        self.boundary.callbacks.history
    }

    pub(crate) fn history_depth(&self) -> usize {
        self.boundary.history_depth
    }

    pub(crate) fn set_history_depth(&mut self, depth: usize) {
        self.boundary.history_depth = depth;
    }

    pub(crate) fn take_completion_pending_listing(&mut self) -> bool {
        std::mem::replace(&mut self.boundary.completion_pending_listing, true)
    }

    pub(crate) fn clear_completion_pending_listing(&mut self) {
        self.boundary.completion_pending_listing = false;
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
        self.boundary.commands.insert(
            name,
            HostCommand {
                callback,
                _help: help.iter().copied().map(TextUnit::from_wide).collect(),
            },
        );
        true
    }

    pub(crate) fn command_callback(&self, name: &CommandName) -> Option<CommandCallback> {
        self.boundary
            .commands
            .get(name)
            .map(|command| command.callback)
    }

    pub(crate) fn bind_command(&mut self, arguments: &[&[u32]]) -> c_int {
        let args: Vec<String> = arguments
            .iter()
            .filter_map(|argument| wide_string(argument))
            .collect();
        if args.len() < 2 {
            return -1;
        }
        match args[1].as_str() {
            "-e" => {
                self.set_editor(EditingMode::Emacs);
                return 0;
            }
            "-v" => {
                self.set_editor(EditingMode::Vi);
                return 0;
            }
            _ => {}
        }
        let (macro_binding, key, command) = if args.get(1).is_some_and(|arg| arg == "-s") {
            let (Some(key), Some(value)) = (args.get(2), args.get(3)) else {
                return -1;
            };
            (true, key.as_str(), value.as_str())
        } else {
            let (Some(key), Some(command)) = (args.get(1), args.get(2)) else {
                return -1;
            };
            (false, key.as_str(), command.as_str())
        };
        let Some(key) = decode_key_sequence(key) else {
            return -1;
        };
        let Ok(key) = KeySequence::new(key) else {
            return -1;
        };
        let binding = if macro_binding {
            let Some(value) = decode_key_sequence(command) else {
                return -1;
            };
            Binding::Macro(value)
        } else if let Some(action) = named_action(command) {
            Binding::Action(action)
        } else {
            let Ok(name) = CommandName::new(command) else {
                return -1;
            };
            if !self.boundary.commands.contains_key(&name) {
                return -1;
            }
            Binding::Action(Action::User(name))
        };
        let mode = self.native.keymap_mode();
        self.native.bind(mode, key, binding);
        0
    }

    pub(crate) fn bind_byte_to_insert(&mut self, byte: u8) {
        let unit = TextUnit::Scalar(char::from(byte));
        let text = Text::from_iter([unit]);
        if let Ok(key) = KeySequence::new(text) {
            self.native.bind(
                self.native.keymap_mode(),
                key,
                Binding::Action(Action::Insert(Text::from_iter([unit]))),
            );
        }
    }
}

#[cfg(test)]
mod tests;
