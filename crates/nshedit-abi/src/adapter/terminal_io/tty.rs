//! Tty-mode override parsing and projection at the ABI boundary.

use super::*;

#[derive(Clone, Copy)]
struct TtyFlag {
    name: &'static str,
    group: usize,
    flag: TerminalFlag,
}

impl TtyFlag {
    const fn new(name: &'static str, group: usize, flag: TerminalFlag) -> Self {
        Self { name, group, flag }
    }
}

const TTY_FLAGS: &[TtyFlag] = &[
    TtyFlag::new("ignbrk", 0, TerminalFlag::IgnoreBreak),
    TtyFlag::new("brkint", 0, TerminalFlag::SignalBreak),
    TtyFlag::new("ignpar", 0, TerminalFlag::IgnoreParityErrors),
    TtyFlag::new("parmrk", 0, TerminalFlag::MarkParityErrors),
    TtyFlag::new("inpck", 0, TerminalFlag::CheckInputParity),
    TtyFlag::new("istrip", 0, TerminalFlag::StripInputHighBit),
    TtyFlag::new("inlcr", 0, TerminalFlag::MapNewlineToCarriageReturn),
    TtyFlag::new("igncr", 0, TerminalFlag::IgnoreCarriageReturn),
    TtyFlag::new("icrnl", 0, TerminalFlag::MapCarriageReturnToNewline),
    TtyFlag::new("iuclc", 0, TerminalFlag::MapUppercaseInputToLowercase),
    TtyFlag::new("ixon", 0, TerminalFlag::EnableOutputFlowControl),
    TtyFlag::new("ixany", 0, TerminalFlag::AllowAnyCharacterToRestartOutput),
    TtyFlag::new("ixoff", 0, TerminalFlag::EnableInputFlowControl),
    TtyFlag::new("imaxbel", 0, TerminalFlag::RingBellOnInputOverflow),
    TtyFlag::new("opost", 1, TerminalFlag::PostProcessOutput),
    TtyFlag::new("olcuc", 1, TerminalFlag::MapLowercaseOutputToUppercase),
    TtyFlag::new("onlcr", 1, TerminalFlag::MapNewlineToCarriageReturnNewline),
    TtyFlag::new("ocrnl", 1, TerminalFlag::MapCarriageReturnToNewlineOnOutput),
    TtyFlag::new("onocr", 1, TerminalFlag::DiscardCarriageReturnAtColumnZero),
    TtyFlag::new("onlret", 1, TerminalFlag::NewlinePerformsCarriageReturn),
    TtyFlag::new("ofill", 1, TerminalFlag::UseFillCharacters),
    TtyFlag::new("ofdel", 1, TerminalFlag::UseDeleteForFill),
    TtyFlag::new("nldly", 1, TerminalFlag::NewlineDelay),
    TtyFlag::new("crdly", 1, TerminalFlag::CarriageReturnDelay),
    TtyFlag::new("tabdly", 1, TerminalFlag::TabDelay),
    TtyFlag::new("xtabs", 1, TerminalFlag::ExpandTabs),
    TtyFlag::new("bsdly", 1, TerminalFlag::BackspaceDelay),
    TtyFlag::new("vtdly", 1, TerminalFlag::VerticalTabDelay),
    TtyFlag::new("ffdly", 1, TerminalFlag::FormFeedDelay),
    TtyFlag::new("cbaud", 2, TerminalFlag::OutputSpeedBits),
    TtyFlag::new("cstopb", 2, TerminalFlag::TwoStopBits),
    TtyFlag::new("cread", 2, TerminalFlag::EnableReceiver),
    TtyFlag::new("parenb", 2, TerminalFlag::EnableParity),
    TtyFlag::new("parodd", 2, TerminalFlag::OddParity),
    TtyFlag::new("hupcl", 2, TerminalFlag::HangUpOnClose),
    TtyFlag::new("clocal", 2, TerminalFlag::IgnoreModemControl),
    TtyFlag::new("cibaud", 2, TerminalFlag::InputSpeedBits),
    TtyFlag::new("crtscts", 2, TerminalFlag::HardwareFlowControl),
    TtyFlag::new("isig", 3, TerminalFlag::GenerateSignals),
    TtyFlag::new("icanon", 3, TerminalFlag::CanonicalInput),
    TtyFlag::new("xcase", 3, TerminalFlag::CanonicalUppercase),
    TtyFlag::new("echo", 3, TerminalFlag::EchoInput),
    TtyFlag::new("echoe", 3, TerminalFlag::EchoErase),
    TtyFlag::new("echok", 3, TerminalFlag::EchoKill),
    TtyFlag::new("echonl", 3, TerminalFlag::EchoNewline),
    TtyFlag::new("noflsh", 3, TerminalFlag::DisableFlush),
    TtyFlag::new("tostop", 3, TerminalFlag::StopBackgroundOutput),
    TtyFlag::new("echoctl", 3, TerminalFlag::EchoControlCharacters),
    TtyFlag::new("echoprt", 3, TerminalFlag::EchoErasedCharacters),
    TtyFlag::new("echoke", 3, TerminalFlag::VisuallyEraseKilledLine),
    TtyFlag::new("flusho", 3, TerminalFlag::OutputBeingFlushed),
    TtyFlag::new("pendin", 3, TerminalFlag::PendingInput),
    TtyFlag::new("iexten", 3, TerminalFlag::ExtendedProcessing),
    TtyFlag::new("extproc", 3, TerminalFlag::ExternalProcessing),
];

#[derive(Clone, Copy)]
struct TtyCharacter {
    name: &'static str,
    character: ControlCharacter,
}

impl TtyCharacter {
    const fn new(name: &'static str, character: ControlCharacter) -> Self {
        Self { name, character }
    }
}

const TTY_CHARACTERS: &[TtyCharacter] = &[
    TtyCharacter::new("intr", ControlCharacter::Interrupt),
    TtyCharacter::new("quit", ControlCharacter::Quit),
    TtyCharacter::new("erase", ControlCharacter::Erase),
    TtyCharacter::new("kill", ControlCharacter::Kill),
    TtyCharacter::new("eof", ControlCharacter::EndOfFile),
    TtyCharacter::new("eol", ControlCharacter::EndOfLine),
    TtyCharacter::new("eol2", ControlCharacter::AlternateEndOfLine),
    TtyCharacter::new("start", ControlCharacter::Start),
    TtyCharacter::new("stop", ControlCharacter::Stop),
    TtyCharacter::new("werase", ControlCharacter::WordErase),
    TtyCharacter::new("susp", ControlCharacter::Suspend),
    TtyCharacter::new("reprint", ControlCharacter::Reprint),
    TtyCharacter::new("discard", ControlCharacter::Discard),
    TtyCharacter::new("lnext", ControlCharacter::LiteralNext),
    TtyCharacter::new("min", ControlCharacter::MinimumBytes),
    TtyCharacter::new("time", ControlCharacter::Timeout),
];

pub(super) fn tty_mode_index(mode: TerminalMode) -> usize {
    match mode {
        TerminalMode::Cooked => 0,
        TerminalMode::Editing => 1,
        TerminalMode::Quoted => 2,
    }
}

fn tty_attributes(state: &TerminalState, mode: usize) -> Option<&TerminalAttributes> {
    match mode {
        0 => state.original.as_ref(),
        1 => state.editing.as_ref(),
        _ => state.quoted.as_ref(),
    }
}

fn tty_attributes_mut(state: &mut TerminalState, mode: usize) -> Option<&mut TerminalAttributes> {
    match mode {
        0 => state.original.as_mut(),
        1 => state.editing.as_mut(),
        _ => state.quoted.as_mut(),
    }
}

fn apply_tty_overrides(attributes: &mut TerminalAttributes, overrides: &TtyFlagOverrides) {
    for (&flag, &state) in &overrides.flags {
        attributes.set_flag(flag, state == TtyOverride::Enable);
    }
}

pub(super) fn parse_tty_character(value: &str) -> u8 {
    let bytes = value.as_bytes();
    let Some(&first) = bytes.first() else {
        return ControlCharacter::EndOfLine.default_value();
    };
    if bytes.len() == 1 {
        return u8::MAX;
    }
    if first == b'^' {
        return bytes.get(1).map_or(
            u8::MAX,
            |&byte| {
                if byte == b'?' { 0x7f } else { byte & 0x1f }
            },
        );
    }
    if first != b'\\' {
        return value
            .chars()
            .next()
            .map_or(u8::MAX, |character| character as u32 as u8);
    }
    let Some(&escaped) = bytes.get(1) else {
        return u8::MAX;
    };
    match escaped {
        b'a' => 0x07,
        b'b' => 0x08,
        b't' => b'\t',
        b'n' => b'\n',
        b'v' => 0x0b,
        b'f' => 0x0c,
        b'r' => b'\r',
        b'e' => 0x1b,
        b'0'..=b'7' => {
            let mut result = 0u16;
            for &digit in bytes[1..].iter().take(3) {
                if !(b'0'..=b'7').contains(&digit) {
                    break;
                }
                result = (result << 3) | u16::from(digit - b'0');
            }
            u8::try_from(result).unwrap_or(u8::MAX)
        }
        b'U' if bytes.get(2) == Some(&b'+') => {
            let digits = &value[3..];
            if !(4..=5).contains(&digits.len())
                || !digits
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
            {
                return u8::MAX;
            }
            u32::from_str_radix(digits, 16)
                .ok()
                .filter(|&character| character <= 0x10ffff)
                .map_or(u8::MAX, |character| character as u8)
        }
        literal => literal,
    }
}

fn append_tty_listing_group(
    output: &mut Vec<u8>,
    header: &str,
    entries: impl IntoIterator<Item = (Option<char>, &'static str)>,
    columns: usize,
    first: bool,
) {
    if !first {
        output.push(b'\n');
    }
    output.extend_from_slice(header.as_bytes());
    let indent = header.len();
    let mut length = indent;
    for (sign, name) in entries {
        let width = name.len() + usize::from(sign.is_some()) + 1;
        if length + width >= columns {
            output.push(b'\n');
            output.extend(std::iter::repeat_n(b' ', indent));
            length = indent + width;
        } else {
            length += width;
        }
        if let Some(sign) = sign {
            output.push(sign as u8);
        }
        output.extend_from_slice(name.as_bytes());
        output.push(b' ');
    }
}

impl EditLine {
    // [spec:nshedit:req:abi.tty-modes]
    pub(crate) fn set_tty_modes(&mut self, arguments: &[&[u32]]) -> c_int {
        let Some(command) = arguments.first().and_then(|word| wide_string(word)) else {
            return -1;
        };
        let Some(words) = arguments
            .iter()
            .map(|word| wide_string(word))
            .collect::<Option<Vec<_>>>()
        else {
            return -1;
        };
        let mut mode = 0usize;
        let mut show_all = false;
        let mut index = 1usize;
        while let Some(option) = words
            .get(index)
            .filter(|word| word.len() == 2 && word.as_bytes().first() == Some(&b'-'))
        {
            match option.as_bytes()[1] {
                b'a' => show_all = true,
                b'd' => mode = 1,
                b'x' => mode = 0,
                b'q' => mode = 2,
                switch => {
                    self.write_compatibility_stream(
                        2,
                        format!("{command}: Unknown switch `{}'.\n", char::from(switch)).as_bytes(),
                    );
                    return -1;
                }
            }
            index += 1;
        }

        if index == words.len() {
            let state = self.boundary.terminal.borrow();
            let overrides = &state.overrides[mode];
            let columns = self.boundary.terminal_capabilities.columns;
            let mut output = Vec::new();
            for (group, header) in ["iflag:", "oflag:", "cflag:", "lflag:"]
                .into_iter()
                .enumerate()
            {
                let entries = TTY_FLAGS
                    .iter()
                    .filter(move |flag| flag.group == group)
                    .filter_map(|flag| {
                        let sign = match overrides.flags.get(&flag.flag) {
                            Some(TtyOverride::Disable) => Some('-'),
                            Some(TtyOverride::Enable) => Some('+'),
                            None => None,
                        };
                        (sign.is_some() || show_all).then_some((sign, flag.name))
                    });
                append_tty_listing_group(&mut output, header, entries, columns, group == 0);
            }
            let characters = TTY_CHARACTERS.iter().filter_map(|character| {
                let sign = match overrides.characters.get(&character.character) {
                    Some(TtyOverride::Disable) => Some('-'),
                    Some(TtyOverride::Enable) => Some('+'),
                    None => None,
                };
                (sign.is_some() || show_all).then_some((sign, character.name))
            });
            append_tty_listing_group(&mut output, "chars:", characters, columns, false);
            output.push(b'\n');
            drop(state);
            self.write_compatibility_stream(1, &output);
            return 0;
        }

        for argument in &words[index..] {
            let (sign, body) = match argument.as_bytes().first() {
                Some(b'+') => (Some(true), &argument[1..]),
                Some(b'-') => (Some(false), &argument[1..]),
                _ => (None, argument.as_str()),
            };
            if let Some((name, value)) = body.split_once('=') {
                let Some(character) = TTY_CHARACTERS
                    .iter()
                    .find(|character| character.name.starts_with(name))
                else {
                    self.invalid_tty_argument(&command, body);
                    return -1;
                };
                let byte = parse_tty_character(value);
                let mut state = self.boundary.terminal.borrow_mut();
                if let Some(attributes) = tty_attributes_mut(&mut state, mode) {
                    attributes.set_control_character(character.character, byte);
                }
                continue;
            }

            if let Some(flag) = TTY_FLAGS.iter().find(|flag| flag.name == body) {
                let overrides = &mut self.boundary.terminal.borrow_mut().overrides[mode];
                match sign {
                    Some(true) => {
                        overrides.flags.insert(flag.flag, TtyOverride::Enable);
                    }
                    Some(false) => {
                        overrides.flags.insert(flag.flag, TtyOverride::Disable);
                    }
                    None => {
                        overrides.flags.remove(&flag.flag);
                    }
                }
                continue;
            }
            if let Some(character) = TTY_CHARACTERS
                .iter()
                .find(|character| character.name == body)
            {
                let overrides = &mut self.boundary.terminal.borrow_mut().overrides[mode];
                match sign {
                    Some(true) => {
                        overrides
                            .characters
                            .insert(character.character, TtyOverride::Enable);
                    }
                    Some(false) => {
                        overrides
                            .characters
                            .insert(character.character, TtyOverride::Disable);
                    }
                    None => {
                        overrides.characters.remove(&character.character);
                    }
                }
                continue;
            }
            self.invalid_tty_argument(&command, body);
            return -1;
        }

        let mut state = self.boundary.terminal.borrow_mut();
        let overrides = state.overrides[mode].clone();
        if let Some(attributes) = tty_attributes_mut(&mut state, mode) {
            apply_tty_overrides(attributes, &overrides);
        }
        if tty_mode_index(state.active_mode) != mode {
            return 0;
        }
        let descriptor = state.input;
        let Some(attributes) = tty_attributes(&state, mode) else {
            return -1;
        };
        -c_int::from(
            apply_terminal_attributes(descriptor, ApplyWhen::AfterOutput, attributes).is_err(),
        )
    }

    fn invalid_tty_argument(&self, command: &str, argument: &str) {
        self.write_compatibility_stream(
            2,
            format!("{command}: Invalid argument `{argument}'.\n").as_bytes(),
        );
    }
}
