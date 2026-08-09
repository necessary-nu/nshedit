//! Tty-mode override parsing and projection at the ABI boundary.

use super::*;

#[derive(Clone, Copy)]
struct TtyFlag {
    name: &'static str,
    group: usize,
    bit: u32,
}

impl TtyFlag {
    const fn new(name: &'static str, group: usize, bit: u32) -> Self {
        Self { name, group, bit }
    }
}

const TTY_FLAGS: &[TtyFlag] = &[
    TtyFlag::new("ignbrk", 0, termios::IGNBRK),
    TtyFlag::new("brkint", 0, termios::BRKINT),
    TtyFlag::new("ignpar", 0, termios::IGNPAR),
    TtyFlag::new("parmrk", 0, termios::PARMRK),
    TtyFlag::new("inpck", 0, termios::INPCK),
    TtyFlag::new("istrip", 0, termios::ISTRIP),
    TtyFlag::new("inlcr", 0, termios::INLCR),
    TtyFlag::new("igncr", 0, termios::IGNCR),
    TtyFlag::new("icrnl", 0, termios::ICRNL),
    TtyFlag::new("iuclc", 0, termios::IUCLC),
    TtyFlag::new("ixon", 0, termios::IXON),
    TtyFlag::new("ixany", 0, termios::IXANY),
    TtyFlag::new("ixoff", 0, termios::IXOFF),
    TtyFlag::new("imaxbel", 0, termios::IMAXBEL),
    TtyFlag::new("opost", 1, termios::OPOST),
    TtyFlag::new("olcuc", 1, termios::OLCUC),
    TtyFlag::new("onlcr", 1, termios::ONLCR),
    TtyFlag::new("ocrnl", 1, termios::OCRNL),
    TtyFlag::new("onocr", 1, termios::ONOCR),
    TtyFlag::new("onlret", 1, termios::ONLRET),
    TtyFlag::new("ofill", 1, termios::OFILL),
    TtyFlag::new("ofdel", 1, termios::OFDEL),
    TtyFlag::new("nldly", 1, termios::NLDLY),
    TtyFlag::new("crdly", 1, termios::CRDLY),
    TtyFlag::new("tabdly", 1, termios::TABDLY),
    TtyFlag::new("xtabs", 1, termios::XTABS),
    TtyFlag::new("bsdly", 1, termios::BSDLY),
    TtyFlag::new("vtdly", 1, termios::VTDLY),
    TtyFlag::new("ffdly", 1, termios::FFDLY),
    TtyFlag::new("cbaud", 2, termios::CBAUD),
    TtyFlag::new("cstopb", 2, termios::CSTOPB),
    TtyFlag::new("cread", 2, termios::CREAD),
    TtyFlag::new("parenb", 2, termios::PARENB),
    TtyFlag::new("parodd", 2, termios::PARODD),
    TtyFlag::new("hupcl", 2, termios::HUPCL),
    TtyFlag::new("clocal", 2, termios::CLOCAL),
    TtyFlag::new("cibaud", 2, termios::CIBAUD),
    TtyFlag::new("crtscts", 2, termios::CRTSCTS),
    TtyFlag::new("isig", 3, termios::ISIG),
    TtyFlag::new("icanon", 3, termios::ICANON),
    TtyFlag::new("xcase", 3, termios::XCASE),
    TtyFlag::new("echo", 3, termios::ECHO),
    TtyFlag::new("echoe", 3, termios::ECHOE),
    TtyFlag::new("echok", 3, termios::ECHOK),
    TtyFlag::new("echonl", 3, termios::ECHONL),
    TtyFlag::new("noflsh", 3, termios::NOFLSH),
    TtyFlag::new("tostop", 3, termios::TOSTOP),
    TtyFlag::new("echoctl", 3, termios::ECHOCTL),
    TtyFlag::new("echoprt", 3, termios::ECHOPRT),
    TtyFlag::new("echoke", 3, termios::ECHOKE),
    TtyFlag::new("flusho", 3, termios::FLUSHO),
    TtyFlag::new("pendin", 3, termios::PENDIN),
    TtyFlag::new("iexten", 3, termios::IEXTEN),
    TtyFlag::new("extproc", 3, termios::EXTPROC),
];

#[derive(Clone, Copy)]
struct TtyCharacter {
    name: &'static str,
    mask: u32,
    index: usize,
}

impl TtyCharacter {
    const fn new(name: &'static str, mask: u32, index: usize) -> Self {
        Self { name, mask, index }
    }
}

const TTY_CHARACTERS: &[TtyCharacter] = &[
    TtyCharacter::new("intr", 1 << 0, termios::VINTR),
    TtyCharacter::new("quit", 1 << 1, termios::VQUIT),
    TtyCharacter::new("erase", 1 << 2, termios::VERASE),
    TtyCharacter::new("kill", 1 << 3, termios::VKILL),
    TtyCharacter::new("eof", 1 << 4, termios::VEOF),
    TtyCharacter::new("eol", 1 << 5, termios::VEOL),
    TtyCharacter::new("eol2", 1 << 6, termios::VEOL2),
    TtyCharacter::new("start", 1 << 10, termios::VSTART),
    TtyCharacter::new("stop", 1 << 11, termios::VSTOP),
    TtyCharacter::new("werase", 1 << 12, termios::VWERASE),
    TtyCharacter::new("susp", 1 << 13, termios::VSUSP),
    TtyCharacter::new("reprint", 1 << 15, termios::VREPRINT),
    TtyCharacter::new("discard", 1 << 16, termios::VDISCARD),
    TtyCharacter::new("lnext", 1 << 17, termios::VLNEXT),
    TtyCharacter::new("min", 1 << 23, termios::VMIN),
    TtyCharacter::new("time", 1 << 24, termios::VTIME),
];

pub(super) fn tty_mode_index(mode: TerminalMode) -> usize {
    match mode {
        TerminalMode::Cooked => 0,
        TerminalMode::Editing => 1,
        TerminalMode::Quoted => 2,
    }
}

fn tty_attributes(state: &TerminalState, mode: usize) -> Option<&Termios> {
    match mode {
        0 => state.original.as_ref(),
        1 => state.editing.as_ref(),
        _ => state.quoted.as_ref(),
    }
}

fn tty_attributes_mut(state: &mut TerminalState, mode: usize) -> Option<&mut Termios> {
    match mode {
        0 => state.original.as_mut(),
        1 => state.editing.as_mut(),
        _ => state.quoted.as_mut(),
    }
}

fn apply_tty_overrides(attributes: &mut Termios, overrides: TtyFlagOverrides) {
    for (value, group) in [
        (&mut attributes.c_iflag, 0),
        (&mut attributes.c_oflag, 1),
        (&mut attributes.c_cflag, 2),
        (&mut attributes.c_lflag, 3),
    ] {
        *value &= !overrides.clear[group];
        *value |= overrides.set[group];
    }
}

pub(super) fn parse_tty_character(value: &str) -> u8 {
    let bytes = value.as_bytes();
    let Some(&first) = bytes.first() else {
        return termios::VDISABLE;
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
            let overrides = state.overrides[mode];
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
                        let sign = if overrides.clear[group] & flag.bit != 0 {
                            Some('-')
                        } else if overrides.set[group] & flag.bit != 0 {
                            Some('+')
                        } else {
                            None
                        };
                        (sign.is_some() || show_all).then_some((sign, flag.name))
                    });
                append_tty_listing_group(&mut output, header, entries, columns, group == 0);
            }
            let characters = TTY_CHARACTERS.iter().filter_map(|character| {
                let sign = if overrides.char_clear & character.mask != 0 {
                    Some('-')
                } else if overrides.char_set & character.mask != 0 {
                    Some('+')
                } else {
                    None
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
                    attributes.c_cc[character.index] = byte;
                }
                continue;
            }

            if let Some(flag) = TTY_FLAGS.iter().find(|flag| flag.name == body) {
                let overrides = &mut self.boundary.terminal.borrow_mut().overrides[mode];
                match sign {
                    Some(true) => {
                        overrides.set[flag.group] |= flag.bit;
                        overrides.clear[flag.group] &= !flag.bit;
                    }
                    Some(false) => {
                        overrides.set[flag.group] &= !flag.bit;
                        overrides.clear[flag.group] |= flag.bit;
                    }
                    None => {
                        overrides.set[flag.group] &= !flag.bit;
                        overrides.clear[flag.group] &= !flag.bit;
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
                        overrides.char_set |= character.mask;
                        overrides.char_clear &= !character.mask;
                    }
                    Some(false) => {
                        overrides.char_set &= !character.mask;
                        overrides.char_clear |= character.mask;
                    }
                    None => {
                        overrides.char_set &= !character.mask;
                        overrides.char_clear &= !character.mask;
                    }
                }
                continue;
            }
            self.invalid_tty_argument(&command, body);
            return -1;
        }

        let mut state = self.boundary.terminal.borrow_mut();
        let overrides = state.overrides[mode];
        if let Some(attributes) = tty_attributes_mut(&mut state, mode) {
            apply_tty_overrides(attributes, overrides);
        }
        if tty_mode_index(state.active_mode) != mode {
            return 0;
        }
        let descriptor = state.input;
        let Some(attributes) = tty_attributes(&state, mode) else {
            return -1;
        };
        if termios::tcsetattr(descriptor, termios::TCSADRAIN, attributes) {
            0
        } else {
            -1
        }
    }

    fn invalid_tty_argument(&self, command: &str, argument: &str) {
        self.write_compatibility_stream(
            2,
            format!("{command}: Invalid argument `{argument}'.\n").as_bytes(),
        );
    }
}
