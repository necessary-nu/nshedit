//! Compatibility binding grammar, command inventory, and map projection.

use std::collections::BTreeSet;

use nshedit::domain::{Outcome, Refresh};
use nshedit::editor::CommandStep;

use super::*;

mod catalog;
mod codec;

use catalog::{BUILTIN_COMMANDS, TERMINAL_KEYS, action_name, is_builtin, named_action};
use codec::{decode_key_sequence, text_bytes, visual_text, wide_bytes};

#[derive(Default)]
struct BindOptions {
    alternate: bool,
    macro_binding: bool,
    terminal_key: bool,
    remove: bool,
}

impl EditLine {
    pub(in crate::adapter) fn initialize_terminal_bindings(&mut self) {
        for (index, key) in TERMINAL_KEYS.iter().enumerate() {
            self.boundary.terminal_bindings[index] =
                named_action(key.default_command).map(Binding::Action);
        }
    }

    pub(in crate::adapter) fn reset_compatibility_bindings(&mut self, mode: EditingMode) {
        self.native.reset_bindings(mode);
        if mode == EditingMode::Emacs {
            self.native.clear_bindings(KeymapMode::ViCommand);
        }
        self.install_terminal_bindings();
    }

    pub(in crate::adapter) fn install_terminal_bindings(&mut self) {
        let mode = if self.editor_is_vi() {
            KeymapMode::ViCommand
        } else {
            KeymapMode::Emacs
        };
        let mut projected = Vec::new();
        for (index, terminal_key) in TERMINAL_KEYS.iter().enumerate() {
            let mut sequences: Vec<Text> = terminal_key
                .fallback
                .iter()
                .map(|sequence| Text::from(*sequence))
                .collect();
            if let Some(capability) = self
                .boundary
                .terminal_capabilities
                .string(terminal_key.capability)
            {
                let sequence: Text = capability
                    .to_bytes()
                    .iter()
                    .copied()
                    .map(|byte| TextUnit::Scalar(char::from(byte)))
                    .collect();
                if !sequence.is_empty() && !sequences.contains(&sequence) {
                    sequences.push(sequence);
                }
            }
            projected.push((sequences, self.boundary.terminal_bindings[index].clone()));
        }

        for (sequences, binding) in projected {
            for sequence in sequences {
                let Ok(sequence) = KeySequence::new(sequence) else {
                    continue;
                };
                match &binding {
                    Some(binding) => {
                        self.native.bind(mode, sequence, binding.clone());
                    }
                    None => {
                        self.native.unbind(mode, &sequence);
                    }
                }
            }
        }
    }

    // [spec:nshedit:req:abi.bindings]
    pub(crate) fn bind_command(&mut self, arguments: &[&[u32]]) -> c_int {
        let Some(command_name) = arguments.first().copied() else {
            return -1;
        };
        let mut options = BindOptions::default();
        let mut index = 1;
        while let Some(argument) = arguments.get(index) {
            if argument.first() != Some(&(b'-' as u32)) {
                break;
            }
            let flag = argument.get(1).copied().unwrap_or(0);
            match flag {
                value if value == b'a' as u32 => options.alternate = true,
                value if value == b's' as u32 => options.macro_binding = true,
                value if value == b'k' as u32 => options.terminal_key = true,
                value if value == b'r' as u32 => options.remove = true,
                value if value == b'v' as u32 => {
                    self.set_editor(EditingMode::Vi);
                    return 0;
                }
                value if value == b'e' as u32 => {
                    self.set_editor(EditingMode::Emacs);
                    return 0;
                }
                value if value == b'l' as u32 => {
                    self.print_command_inventory();
                    return 0;
                }
                invalid => self.report_invalid_switch(command_name, invalid),
            }
            index += 1;
        }

        let mode = self.binding_mode(options.alternate);
        let Some(raw_key) = arguments.get(index).copied() else {
            self.print_all_bindings();
            return 0;
        };
        index += 1;

        if options.remove {
            if options.terminal_key {
                if let Some(index) = terminal_key_index(raw_key) {
                    self.boundary.terminal_bindings[index] = None;
                }
                return -1;
            }
            let Some(sequence) = decode_key_sequence(raw_key) else {
                self.report_bad_binding_escape(command_name, true);
                return -1;
            };
            let Ok(sequence) = KeySequence::new(sequence) else {
                return -1;
            };
            self.remove_legacy_binding(mode, &sequence);
            return 0;
        }

        let Some(raw_value) = arguments.get(index).copied() else {
            if options.terminal_key {
                self.print_terminal_binding(raw_key);
            } else {
                let Some(sequence) = decode_key_sequence(raw_key) else {
                    self.report_bad_binding_escape(command_name, true);
                    return -1;
                };
                self.print_key_binding(mode, &sequence);
            }
            return 0;
        };

        let binding = if options.macro_binding {
            let Some(expansion) = decode_key_sequence(raw_value) else {
                self.report_bad_binding_escape(command_name, false);
                return -1;
            };
            Binding::Macro(expansion)
        } else {
            let Some(name) = wide_string(raw_value) else {
                self.report_invalid_command(command_name, raw_value);
                return -1;
            };
            let builtin = is_builtin(&name);
            let registered = self
                .boundary
                .commands
                .iter()
                .any(|command| command.name.as_str() == name);
            if !builtin && !registered {
                self.report_invalid_command(command_name, raw_value);
                return -1;
            }
            match named_action(&name) {
                Some(action) => Binding::Action(action),
                None => Binding::Action(Action::User(
                    CommandName::new(name).expect("validated command names are non-empty"),
                )),
            }
        };

        if options.terminal_key {
            let Some(terminal_index) = terminal_key_index(raw_key) else {
                return 0;
            };
            self.boundary.terminal_bindings[terminal_index] = Some(binding);
            if options.macro_binding {
                self.clobber_terminal_name_lead(mode, raw_key);
            }
            return 0;
        }

        let Some(sequence) = decode_key_sequence(raw_key) else {
            self.report_bad_binding_escape(command_name, true);
            return -1;
        };
        let Ok(sequence) = KeySequence::new(sequence) else {
            return -1;
        };
        self.replace_legacy_binding(mode, sequence, binding);
        0
    }

    pub(crate) fn run_builtin_binding(
        &mut self,
        name: &CommandName,
        invoking: TextUnit,
    ) -> Option<Outcome> {
        if !is_builtin(name.as_str()) {
            return None;
        }
        let action = match name.as_str() {
            "ed-insert" | "ed-digit" | "ed-argument-digit" => {
                Action::Insert(std::iter::once(invoking).collect())
            }
            _ => return Some(Outcome::Refresh(Refresh::Beep)),
        };
        Some(match self.native.execute(action) {
            Ok(CommandStep::Applied(outcome)) => outcome,
            Ok(_) | Err(_) => Outcome::Refresh(Refresh::Beep),
        })
    }

    fn binding_mode(&self, alternate: bool) -> KeymapMode {
        if alternate {
            KeymapMode::ViCommand
        } else if self.editor_is_vi() {
            KeymapMode::ViInsert
        } else {
            KeymapMode::Emacs
        }
    }

    fn replace_legacy_binding(
        &mut self,
        mode: KeymapMode,
        sequence: KeySequence,
        binding: Binding,
    ) {
        let units = sequence.as_text().as_units();
        let conflicts: Vec<KeySequence> = self
            .native
            .bindings(mode)
            .filter_map(|(candidate, _)| {
                let candidate_units = candidate.as_text().as_units();
                (candidate != &sequence
                    && (candidate_units.starts_with(units) || units.starts_with(candidate_units)))
                .then(|| candidate.clone())
            })
            .collect();
        for conflict in conflicts {
            self.native.unbind(mode, &conflict);
        }
        self.native.bind(mode, sequence, binding);
    }

    fn remove_legacy_binding(&mut self, mode: KeymapMode, sequence: &KeySequence) {
        let units = sequence.as_text().as_units();
        let removals: Vec<KeySequence> = self
            .native
            .bindings(mode)
            .filter_map(|(candidate, _)| {
                let remove = candidate == sequence
                    || (units.len() == 1 && candidate.as_text().as_units().starts_with(units));
                remove.then(|| candidate.clone())
            })
            .collect();
        for removal in removals {
            self.native.unbind(mode, &removal);
        }
    }

    fn clobber_terminal_name_lead(&mut self, mode: KeymapMode, raw_name: &[u32]) {
        let Some(&first) = raw_name.first() else {
            return;
        };
        let key = Text::from_iter([TextUnit::from_wide(first & 0xff)]);
        if let Ok(key) = KeySequence::new(key) {
            self.replace_legacy_binding(mode, key, Binding::Action(Action::Refresh(Refresh::Beep)));
        }
    }

    fn print_command_inventory(&self) {
        let mut output = Vec::new();
        for command in BUILTIN_COMMANDS {
            output.extend_from_slice(command.name.as_bytes());
            output.extend_from_slice(b"\n\t");
            output.extend_from_slice(command.description.as_bytes());
            output.push(b'\n');
        }
        for command in &self.boundary.commands {
            output.extend_from_slice(command.name.as_str().as_bytes());
            output.extend_from_slice(b"\n\t");
            output.extend_from_slice(&text_bytes(&command.help));
            output.push(b'\n');
        }
        self.write_compatibility_stream(1, &output);
    }

    fn print_all_bindings(&self) {
        let normal = self.binding_mode(false);
        let alternate = self.binding_mode(true);
        let mut output = b"Standard key bindings\n".to_vec();
        append_single_bindings(&mut output, self.native(), normal);
        output.extend_from_slice(b"Alternative key bindings\n");
        append_single_bindings(&mut output, self.native(), alternate);
        output.extend_from_slice(b"Multi-character bindings\n");
        append_multi_bindings(&mut output, self.native(), normal, alternate);
        output.extend_from_slice(b"Arrow key bindings\n");
        for (index, key) in TERMINAL_KEYS.iter().enumerate() {
            if let Some(binding) = &self.boundary.terminal_bindings[index] {
                append_binding_line(&mut output, key.name, binding);
            }
        }
        self.write_compatibility_stream(1, &output);
    }

    fn print_key_binding(&self, mode: KeymapMode, sequence: &Text) {
        let Ok(key) = KeySequence::new(sequence.clone()) else {
            return;
        };
        if sequence.len() <= 1 {
            if let Some(binding) = self.native.binding(mode, &key) {
                let rendered = visual_text(sequence, false);
                let description = match binding {
                    Binding::Macro(_) => "ed-sequence-lead-in".to_owned(),
                    _ => binding_description(binding),
                };
                let line = format!("{rendered}\t->\t{description}\n");
                self.write_compatibility_stream(1, line.as_bytes());
            }
            return;
        }

        let mut matches = 0;
        let mut output = Vec::new();
        for (candidate, binding) in self.native.bindings(mode) {
            if candidate
                .as_text()
                .as_units()
                .starts_with(sequence.as_units())
            {
                append_binding_line(
                    &mut output,
                    &visual_text(candidate.as_text(), true),
                    binding,
                );
                matches += 1;
            }
        }
        if matches == 0 {
            let line = format!(
                "Unbound extended key \"{}\"\n",
                visual_text(sequence, false)
            );
            self.write_compatibility_stream(2, line.as_bytes());
        } else {
            self.write_compatibility_stream(1, &output);
        }
    }

    fn print_terminal_binding(&self, raw_name: &[u32]) {
        let Some(index) = terminal_key_index(raw_name) else {
            return;
        };
        let Some(binding) = &self.boundary.terminal_bindings[index] else {
            return;
        };
        let mut output = Vec::new();
        append_binding_line(&mut output, TERMINAL_KEYS[index].name, binding);
        self.write_compatibility_stream(1, &output);
    }

    fn report_invalid_switch(&self, command: &[u32], invalid: u32) {
        let mut output = wide_bytes(command);
        output.extend_from_slice(b": Invalid switch `");
        output.extend_from_slice(&wide_bytes(&[invalid]));
        output.extend_from_slice(b"'.\n");
        self.write_compatibility_stream(2, &output);
    }

    fn report_bad_binding_escape(&self, command: &[u32], input: bool) {
        let mut output = wide_bytes(command);
        output.extend_from_slice(if input {
            b": Invalid \\ or ^ in instring.\n"
        } else {
            b": Invalid \\ or ^ in outstring.\n"
        });
        self.write_compatibility_stream(2, &output);
    }

    fn report_invalid_command(&self, command: &[u32], value: &[u32]) {
        let mut output = wide_bytes(command);
        output.extend_from_slice(b": Invalid command `");
        output.extend_from_slice(&wide_bytes(value));
        output.extend_from_slice(b"'.\n");
        self.write_compatibility_stream(2, &output);
    }
}

fn terminal_key_index(name: &[u32]) -> Option<usize> {
    let name = wide_string(name)?;
    TERMINAL_KEYS.iter().position(|key| key.name == name)
}

fn binding_description(binding: &Binding) -> String {
    match binding {
        Binding::Action(action) => action_name(action).unwrap_or("ed-unassigned").to_owned(),
        Binding::Macro(expansion) => visual_text(expansion, true),
    }
}

fn append_single_bindings<T: TerminalControl>(
    output: &mut Vec<u8>,
    editor: &Editor<T>,
    mode: KeymapMode,
) {
    for (sequence, binding) in editor.bindings(mode) {
        if sequence.as_text().len() == 1 {
            match binding {
                Binding::Macro(_) => append_named_line(
                    output,
                    &visual_text(sequence.as_text(), true),
                    "ed-sequence-lead-in",
                ),
                _ => append_binding_line(output, &visual_text(sequence.as_text(), true), binding),
            }
        }
    }
}

fn append_multi_bindings<T: TerminalControl>(
    output: &mut Vec<u8>,
    editor: &Editor<T>,
    first: KeymapMode,
    second: KeymapMode,
) {
    let mut seen = BTreeSet::new();
    for mode in [first, second] {
        for (sequence, binding) in editor.bindings(mode) {
            if (sequence.as_text().len() > 1 || matches!(binding, Binding::Macro(_)))
                && seen.insert(sequence.as_text().clone())
            {
                append_binding_line(output, &visual_text(sequence.as_text(), true), binding);
            }
        }
    }
}

fn append_binding_line(output: &mut Vec<u8>, key: &str, binding: &Binding) {
    let description = binding_description(binding);
    append_named_line(output, key, &description);
}

fn append_named_line(output: &mut Vec<u8>, key: &str, description: &str) {
    output.extend_from_slice(format!("{key:<15}->  {description}\n").as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Box<EditLine> {
        EditLine::new(
            "binding-test",
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            -1,
            -1,
            -1,
        )
        .expect("construct an editor over inert descriptors")
    }

    fn bind(editor: &mut EditLine, arguments: &[&str]) -> c_int {
        let arguments: Vec<Vec<u32>> = arguments
            .iter()
            .map(|argument| argument.chars().map(u32::from).collect())
            .collect();
        let arguments: Vec<&[u32]> = arguments.iter().map(Vec::as_slice).collect();
        editor.bind_command(&arguments)
    }

    #[test]
    fn decoder_handles_binding_grammar() {
        let input: Vec<u32> = "M-^A\\e\\101\\U+0042".chars().map(u32::from).collect();
        assert_eq!(
            decode_key_sequence(&input),
            Some(Text::from_iter([
                TextUnit::Scalar('\u{1b}'),
                TextUnit::Scalar('\u{1}'),
                TextUnit::Scalar('\u{1b}'),
                TextUnit::Scalar('A'),
                TextUnit::Scalar('B'),
            ]))
        );
    }

    #[test]
    fn inventory_is_complete_and_unique() {
        assert_eq!(BUILTIN_COMMANDS.len(), 96);
        let names: BTreeSet<_> = BUILTIN_COMMANDS
            .iter()
            .map(|command| command.name)
            .collect();
        assert_eq!(names.len(), BUILTIN_COMMANDS.len());
        assert!(is_builtin("ed-start-over"));
        assert!(is_builtin("vi-redo"));
    }

    // [spec:nshedit:req:abi.bindings/test]
    #[test]
    fn forms_mutate_selected_maps() {
        let mut editor = editor();
        assert_eq!(bind(&mut editor, &["bind", "-s", "^Z", "hello"]), 0);
        let control_z = KeySequence::try_from("\u{1a}").unwrap();
        assert_eq!(
            editor.native().binding(KeymapMode::Emacs, &control_z),
            Some(&Binding::Macro(Text::from("hello")))
        );

        assert_eq!(
            bind(&mut editor, &["bind", "-a", "^A", "ed-move-to-end"]),
            0
        );
        let control_a = KeySequence::try_from("\u{1}").unwrap();
        assert!(matches!(
            editor.native().binding(KeymapMode::ViCommand, &control_a),
            Some(Binding::Action(Action::Move(Motion::EndOfBuffer)))
        ));

        assert_eq!(bind(&mut editor, &["bind", "-r", "^Z"]), 0);
        assert!(
            editor
                .native()
                .binding(KeymapMode::Emacs, &control_z)
                .is_none()
        );
        assert_eq!(bind(&mut editor, &["bind", "-e"]), 0);
        assert!(
            editor
                .native()
                .binding(KeymapMode::ViCommand, &control_a)
                .is_none()
        );
    }

    #[test]
    fn all_builtin_names_resolve() {
        let mut editor = editor();
        for command in BUILTIN_COMMANDS {
            assert_eq!(
                bind(&mut editor, &["bind", "^X", command.name]),
                0,
                "{} did not resolve",
                command.name
            );
        }
    }

    #[test]
    fn terminal_binding_applies_on_reset() {
        let mut editor = editor();
        assert_eq!(
            bind(&mut editor, &["bind", "-k", "up", "ed-move-to-end"]),
            0
        );
        assert_eq!(bind(&mut editor, &["bind", "-e"]), 0);
        let up = KeySequence::try_from("\u{1b}[A").unwrap();
        assert!(matches!(
            editor.native().binding(KeymapMode::Emacs, &up),
            Some(Binding::Action(Action::Move(Motion::EndOfBuffer)))
        ));
        assert_eq!(bind(&mut editor, &["bind", "-k", "-r", "up"]), -1);
    }
}
