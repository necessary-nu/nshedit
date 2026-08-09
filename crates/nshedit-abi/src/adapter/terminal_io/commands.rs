//! Observable terminal capability commands at the ABI boundary.

use super::profile::{CapabilityValueKind, local_value_capability, string_capability_name};
use super::*;
use nshterm::parser::names::BOOL_NAMES;
use std::io::Write;

/// Linux `speed_t` projection required by the `telltc baud` compatibility
/// command. The platform API intentionally exposes semantic rates instead of
/// this encoding.
const fn compatibility_baud_encoding(speed: OutputSpeed) -> u32 {
    let OutputSpeed::BitsPerSecond(rate) = speed else {
        return 0o0010000;
    };
    match rate {
        0 => 0o0000000,
        50 => 0o0000001,
        75 => 0o0000002,
        110 => 0o0000003,
        134 => 0o0000004,
        150 => 0o0000005,
        200 => 0o0000006,
        300 => 0o0000007,
        600 => 0o0000010,
        1_200 => 0o0000011,
        1_800 => 0o0000012,
        2_400 => 0o0000013,
        4_800 => 0o0000014,
        9_600 => 0o0000015,
        19_200 => 0o0000016,
        38_400 => 0o0000017,
        57_600 => 0o0010001,
        115_200 => 0o0010002,
        230_400 => 0o0010003,
        460_800 => 0o0010004,
        500_000 => 0o0010005,
        576_000 => 0o0010006,
        921_600 => 0o0010007,
        1_000_000 => 0o0010010,
        1_152_000 => 0o0010011,
        1_500_000 => 0o0010012,
        2_000_000 => 0o0010013,
        2_500_000 => 0o0010014,
        3_000_000 => 0o0010015,
        3_500_000 => 0o0010016,
        4_000_000 => 0o0010017,
        _ => 0o0010000,
    }
}

const LOCAL_STRING_CAPABILITIES: &[(&str, &str)] = &[
    ("al", "add new blank line"),
    ("bl", "audible bell"),
    ("cd", "clear to bottom"),
    ("ce", "clear to end of line"),
    ("ch", "cursor to horiz pos"),
    ("cl", "clear screen"),
    ("dc", "delete a character"),
    ("dl", "delete a line"),
    ("dm", "start delete mode"),
    ("ed", "end delete mode"),
    ("ei", "end insert mode"),
    ("fs", "cursor from status line"),
    ("ho", "home cursor"),
    ("ic", "insert character"),
    ("im", "start insert mode"),
    ("ip", "insert padding"),
    ("kd", "sends cursor down"),
    ("kl", "sends cursor left"),
    ("kr", "sends cursor right"),
    ("ku", "sends cursor up"),
    ("md", "begin bold"),
    ("me", "end attributes"),
    ("nd", "non destructive space"),
    ("se", "end standout"),
    ("so", "begin standout"),
    ("ts", "cursor to status line"),
    ("up", "cursor up one"),
    ("us", "begin underline"),
    ("ue", "end underline"),
    ("vb", "visible bell"),
    ("DC", "delete multiple chars"),
    ("DO", "cursor down multiple"),
    ("IC", "insert multiple chars"),
    ("LE", "cursor left multiple"),
    ("RI", "cursor right multiple"),
    ("UP", "cursor up multiple"),
    ("kh", "send cursor home"),
    ("@7", "send cursor end"),
    ("kD", "send cursor delete"),
];

fn local_string_capability_name(code: &str) -> Option<&'static str> {
    LOCAL_STRING_CAPABILITIES
        .iter()
        .any(|(candidate, _)| *candidate == code)
        .then(|| string_capability_name(code))
        .flatten()
}

fn visual_capability(bytes: &[u8]) -> String {
    let mut output = String::new();
    for character in String::from_utf8_lossy(bytes).chars() {
        match character {
            '\u{7f}' => output.push_str("^?"),
            control if control.is_ascii_control() => {
                output.push('^');
                output.push(char::from((control as u8) | 0x40));
            }
            printable => output.push(printable),
        }
    }
    output
}

fn decimal_argument(value: &str) -> Option<i32> {
    if value.is_empty() {
        return Some(0);
    }
    value.parse::<i64>().ok().map(|value| value as i32)
}

pub(super) fn required_parameters(sequence: &[u8]) -> usize {
    let mut maximum = 0;
    let mut legacy = 0;
    let mut index = 0;
    while index < sequence.len() {
        if sequence[index] != b'%' || index + 1 >= sequence.len() {
            index += 1;
            continue;
        }
        match sequence[index + 1] {
            b'p' if sequence.get(index + 2).is_some_and(u8::is_ascii_digit) => {
                maximum = maximum.max(usize::from(sequence[index + 2] - b'0'));
                index += 3;
            }
            b'd' | b'2' | b'3' | b'.' | b'+' => {
                legacy += 1;
                index += if sequence[index + 1] == b'+' { 3 } else { 2 };
            }
            _ => index += 2,
        }
    }
    if maximum == 0 { legacy } else { maximum }
}

pub(super) fn expand_legacy_sequence(sequence: &[u8], column: i32, row: i32) -> Vec<u8> {
    let mut values = [row, column];
    let mut parameter = 0usize;
    let mut output = Vec::with_capacity(sequence.len());
    let mut index = 0usize;
    while index < sequence.len() {
        if sequence[index] != b'%' || index + 1 >= sequence.len() {
            output.push(sequence[index]);
            index += 1;
            continue;
        }
        let operation = sequence[index + 1];
        index += 2;
        let current = parameter.min(1);
        match operation {
            b'%' => output.push(b'%'),
            b'd' => {
                output.extend_from_slice(values[current].to_string().as_bytes());
                parameter += 1;
            }
            b'2' | b'3' => {
                let width = usize::from(operation - b'0');
                output.extend_from_slice(format!("{:0width$}", values[current]).as_bytes());
                parameter += 1;
            }
            b'.' => {
                output.push(values[current] as u8);
                parameter += 1;
            }
            b'+' => {
                let Some(addend) = sequence.get(index).copied() else {
                    output.extend_from_slice(b"%+");
                    continue;
                };
                index += 1;
                output.push(values[current].wrapping_add(i32::from(addend)) as u8);
                parameter += 1;
            }
            b'i' => {
                values[0] = values[0].wrapping_add(1);
                values[1] = values[1].wrapping_add(1);
            }
            b'r' => values.swap(0, 1),
            b'n' => {
                values[0] ^= 0o140;
                values[1] ^= 0o140;
            }
            b'B' => values[current] = (values[current] / 10) * 16 + values[current] % 10,
            b'D' => values[current] -= 2 * (values[current] % 16),
            b'>' => {
                let Some((&threshold, tail)) =
                    sequence.get(index..).and_then(|tail| tail.split_first())
                else {
                    output.extend_from_slice(b"%>");
                    continue;
                };
                let Some(&increment) = tail.first() else {
                    output.extend_from_slice(b"%>");
                    output.push(threshold);
                    continue;
                };
                index += 2;
                if values[current] > i32::from(threshold) {
                    values[current] = values[current].wrapping_add(i32::from(increment));
                }
            }
            unknown => {
                output.push(b'%');
                output.push(unknown);
            }
        }
    }
    output
}

impl EditLine {
    // [spec:nshedit:req:abi.terminal-controls+1]
    /// Dispatch a terminal or tty editrc command at the ABI boundary.
    pub(crate) fn terminal_command(&mut self, arguments: &[&[u32]]) -> c_int {
        let Some(command) = arguments.first().and_then(|word| wide_string(word)) else {
            return -1;
        };
        match command.as_str() {
            "telltc" => self.tell_terminal_capabilities(),
            "settc" => self.set_terminal_capability(arguments),
            "echotc" => self.echo_terminal_capability(arguments),
            "setty" => self.set_tty_modes(arguments),
            _ => -1,
        }
    }

    fn terminal_flags(&self) -> (bool, bool, bool, bool) {
        let capabilities = &self.boundary.terminal.capabilities;
        let physical_tabs = self
            .boundary
            .terminal
            .state
            .borrow()
            .original
            .as_ref()
            .is_some_and(|attributes| !attributes.flag(TerminalFlag::ExpandTabs));
        let tabs =
            physical_tabs && capabilities.boolean("pt") && !capabilities.derived_destructive_tabs;
        let meta = capabilities.boolean("km") || capabilities.derived_meta_extension;
        (
            tabs,
            meta,
            capabilities.boolean("am"),
            capabilities.boolean("xn"),
        )
    }

    fn tell_terminal_capabilities(&self) -> c_int {
        let capabilities = &self.boundary.terminal.capabilities;
        let (tabs, meta, automatic_margins, magic_margins) = self.terminal_flags();
        let mut output = Vec::new();
        writeln!(output, "\n\tYour terminal has the").expect("Vec writes cannot fail");
        writeln!(output, "\tfollowing characteristics:\n").expect("Vec writes cannot fail");
        writeln!(
            output,
            "\tIt has {} columns and {} lines",
            capabilities.columns, capabilities.rows
        )
        .expect("Vec writes cannot fail");
        writeln!(
            output,
            "\tIt has {} meta key",
            if meta { "a" } else { "no" }
        )
        .expect("Vec writes cannot fail");
        writeln!(
            output,
            "\tIt can{}use tabs",
            if tabs { " " } else { "not " }
        )
        .expect("Vec writes cannot fail");
        writeln!(
            output,
            "\tIt {} automatic margins",
            if automatic_margins {
                "has"
            } else {
                "does not have"
            }
        )
        .expect("Vec writes cannot fail");
        if automatic_margins {
            writeln!(
                output,
                "\tIt {} magic margins",
                if magic_margins {
                    "has"
                } else {
                    "does not have"
                }
            )
            .expect("Vec writes cannot fail");
        }
        for &(code, description) in LOCAL_STRING_CAPABILITIES {
            let value = capabilities
                .string(code)
                .filter(|value| !value.to_bytes().is_empty())
                .map_or_else(
                    || "(empty)".to_owned(),
                    |value| visual_capability(value.to_bytes()),
                );
            writeln!(output, "\t{description:>25} ({code}) == {value}")
                .expect("Vec writes cannot fail");
        }
        output.push(b'\n');
        self.write_compatibility_stream(StreamKind::Output, &output);
        0
    }

    fn set_terminal_capability(&mut self, arguments: &[&[u32]]) -> c_int {
        let (Some(command), Some(name), Some(value)) = (
            arguments.first().and_then(|word| wide_string(word)),
            arguments.get(1).and_then(|word| wide_string(word)),
            arguments.get(2).and_then(|word| wide_string(word)),
        ) else {
            return -1;
        };
        let mut name = name.into_bytes();
        let mut value = value.into_bytes();
        name.truncate(7);
        value.truncate(7);
        let Ok(name_text) = core::str::from_utf8(&name) else {
            return self.bad_terminal_value(&command, "capability", &name);
        };

        if let Some(capname) = local_string_capability_name(name_text) {
            if value.is_empty() {
                self.boundary.terminal.capabilities.strings.remove(capname);
            } else {
                let value = CString::new(value).expect("wide input contains no NUL");
                self.boundary
                    .terminal
                    .capabilities
                    .strings
                    .insert(capname, value);
            }
            self.boundary.terminal.capabilities.refresh_derived_flags();
            self.configure_terminal_display();
            return 0;
        }

        let Some((kind, capname)) = local_value_capability(name_text) else {
            let mut diagnostic = format!("{command}: Bad capability `").into_bytes();
            diagnostic.extend_from_slice(&name);
            diagnostic.extend_from_slice(b"'.\n");
            self.write_compatibility_stream(StreamKind::Diagnostics, &diagnostic);
            return -1;
        };
        match kind {
            CapabilityValueKind::Boolean => match value.as_slice() {
                b"yes" => {
                    self.boundary
                        .terminal
                        .capabilities
                        .bools
                        .insert(capname, true);
                }
                b"no" => {
                    self.boundary
                        .terminal
                        .capabilities
                        .bools
                        .insert(capname, false);
                }
                _ => return self.bad_terminal_value(&command, "value", &value),
            },
            CapabilityValueKind::Number => {
                let Ok(value_text) = core::str::from_utf8(&value) else {
                    return self.bad_terminal_value(&command, "value", &value);
                };
                let Some(number) = decimal_argument(value_text) else {
                    return self.bad_terminal_value(&command, "value", &value);
                };
                self.boundary
                    .terminal
                    .capabilities
                    .numbers
                    .insert(capname, number);
                if BOOL_NAMES.contains(&capname) {
                    self.boundary
                        .terminal
                        .capabilities
                        .bools
                        .insert(capname, number != 0);
                }
                if name_text == "co" || name_text == "li" {
                    let rows = if name_text == "li" {
                        usize::try_from(number)
                            .ok()
                            .filter(|&rows| rows >= 1)
                            .unwrap_or(24)
                    } else {
                        self.boundary.terminal.capabilities.rows
                    };
                    let columns = if name_text == "co" {
                        usize::try_from(number)
                            .ok()
                            .filter(|&columns| columns >= 2)
                            .unwrap_or(80)
                    } else {
                        self.boundary.terminal.capabilities.columns
                    };
                    self.boundary.terminal.capabilities.set_size(rows, columns);
                }
            }
        }
        if matches!(kind, CapabilityValueKind::Boolean) {
            self.boundary.terminal.capabilities.refresh_derived_flags();
        }
        self.configure_terminal_display();
        0
    }

    fn bad_terminal_value(&self, command: &str, noun: &str, value: &[u8]) -> c_int {
        let mut diagnostic = format!("{command}: Bad {noun} `").into_bytes();
        diagnostic.extend_from_slice(value);
        diagnostic.extend_from_slice(b"'.\n");
        self.write_compatibility_stream(StreamKind::Diagnostics, &diagnostic);
        -1
    }

    /// The value of the numeric capability `name`, or `None` when the
    /// terminal has no numeric capability by that name.
    // [spec:nshedit:req:abi.rust-internals]
    pub(crate) fn terminal_capability_number(&self, name: &[u8]) -> Option<c_int> {
        let name = core::str::from_utf8(name).ok()?;
        match local_value_capability(name) {
            Some((CapabilityValueKind::Number, _)) => {
                Some(self.boundary.terminal.capabilities.number(name))
            }
            _ => None,
        }
    }

    /// Query one capability through `EL_GETTC`'s capability-dependent out pointer.
    ///
    /// # Safety
    /// `output` must point to `char *` storage for string and boolean
    /// capabilities, and to `int` storage for numeric capabilities.
    pub(crate) unsafe fn get_terminal_capability(&self, name: &[u8], output: *mut c_void) -> c_int {
        if output.is_null() {
            return -1;
        }
        let Ok(name) = core::str::from_utf8(name) else {
            return -1;
        };
        let capabilities = &self.boundary.terminal.capabilities;
        if let Some(capname) = local_string_capability_name(name) {
            let value = capabilities
                .strings
                .get(capname)
                .map_or(core::ptr::null(), |value| value.as_ptr());
            unsafe { *output.cast::<*const c_char>() = value };
            return 0;
        }
        let Some((kind, capname)) = local_value_capability(name) else {
            return -1;
        };
        match kind {
            CapabilityValueKind::Boolean => {
                let value = if capabilities.bools.get(capname).copied().unwrap_or(false) {
                    c"yes"
                } else {
                    c"no"
                };
                unsafe { *output.cast::<*const c_char>() = value.as_ptr() };
            }
            CapabilityValueKind::Number => {
                unsafe { *output.cast::<c_int>() = capabilities.number(name) };
            }
        }
        0
    }

    fn echo_terminal_capability(&self, arguments: &[&[u32]]) -> c_int {
        if arguments.len() < 2 {
            return -1;
        }
        let mut index = 1usize;
        let mut verbose = false;
        let mut silent = false;
        let Some(mut name) = arguments.get(index).and_then(|word| wide_string(word)) else {
            return -1;
        };
        if name.starts_with('-') {
            verbose = name.as_bytes().get(1) == Some(&b'v');
            silent = name.as_bytes().get(1) == Some(&b's');
            index += 1;
            let Some(next) = arguments.get(index).and_then(|word| wide_string(word)) else {
                return 0;
            };
            name = next;
        }
        if name.is_empty() {
            return 0;
        }
        let capabilities = &self.boundary.terminal.capabilities;
        let (tabs, _, automatic_margins, magic_margins) = self.terminal_flags();
        let pseudo = match name.as_str() {
            "tabs" => Some(if tabs {
                "yes\n".to_owned()
            } else {
                "no\n".to_owned()
            }),
            "meta" => Some(if capabilities.boolean("km") {
                "yes\n".to_owned()
            } else {
                "no\n".to_owned()
            }),
            "xn" => Some(if magic_margins { "yes\n" } else { "no\n" }.to_owned()),
            "am" => Some(if automatic_margins { "yes\n" } else { "no\n" }.to_owned()),
            "baud" => {
                let speed = self
                    .boundary
                    .terminal
                    .state
                    .borrow()
                    .original
                    .as_ref()
                    .map_or(0, |attributes| {
                        compatibility_baud_encoding(attributes.output_speed())
                    });
                Some(format!("{speed}\n"))
            }
            "rows" | "lines" => Some(format!("{}\n", capabilities.rows)),
            "cols" => Some(format!("{}\n", capabilities.columns)),
            _ => None,
        };
        if let Some(pseudo) = pseudo {
            self.write_compatibility_stream(StreamKind::Output, pseudo.as_bytes());
            return 0;
        }

        let local = LOCAL_STRING_CAPABILITIES
            .iter()
            .any(|(candidate, _)| *candidate == name);
        let sequence = if local {
            capabilities.string(&name)
        } else {
            string_capability_name(&name)
                .and_then(|capname| capabilities.strings.get(capname))
                .map(CString::as_c_str)
        };
        let Some(sequence) = sequence.filter(|sequence| !sequence.to_bytes().is_empty()) else {
            if !silent {
                self.write_compatibility_stream(
                    StreamKind::Diagnostics,
                    format!("echotc: Termcap parameter `{name}' not found.\n").as_bytes(),
                );
            }
            return -1;
        };
        let needed = required_parameters(sequence.to_bytes());
        let values: Vec<String> = arguments[index + 1..]
            .iter()
            .filter_map(|word| wide_string(word))
            .collect();
        if values.len() > needed && values.get(needed).is_some_and(|value| !value.is_empty()) {
            if !silent {
                self.write_compatibility_stream(
                    StreamKind::Diagnostics,
                    format!("echotc: Warning: Extra argument `{}`.\n", values[needed]).as_bytes(),
                );
            }
            return -1;
        }
        if values.len() < needed || values.iter().take(needed).any(String::is_empty) {
            if !silent {
                self.write_compatibility_stream(
                    StreamKind::Diagnostics,
                    b"echotc: Warning: Missing argument.\n",
                );
            }
            return -1;
        }
        let mut parsed = Vec::with_capacity(needed);
        for (position, value) in values.iter().take(needed).enumerate() {
            let Some(number) = decimal_argument(value).filter(|&number| number >= 0) else {
                if !silent {
                    let dimension = if needed == 1 || position == 1 {
                        "rows"
                    } else {
                        "cols"
                    };
                    self.write_compatibility_stream(
                        StreamKind::Diagnostics,
                        format!("echotc: Bad value `{value}' for {dimension}.\n").as_bytes(),
                    );
                }
                return -1;
            };
            parsed.push(number);
        }
        if needed > 2 && verbose {
            self.write_compatibility_stream(
                StreamKind::Diagnostics,
                format!("echotc: Warning: Too many required arguments ({needed}).\n").as_bytes(),
            );
        }
        let column = if needed >= 2 { parsed[0] } else { 0 };
        let row = if needed == 0 {
            0
        } else {
            parsed[needed.min(2) - 1]
        };
        let affected_lines = if needed >= 2 {
            usize::try_from(row).unwrap_or(0)
        } else {
            1
        };
        let profile = capabilities.profile(self.terminal_baud_rate());
        let bytes = if sequence.to_bytes().windows(2).any(|window| window == b"%p") {
            profile.expand_sequence(sequence.to_bytes(), &[row, column], affected_lines)
        } else {
            let expanded = expand_legacy_sequence(sequence.to_bytes(), column, row);
            profile.expand_sequence(&expanded, &[], affected_lines)
        };
        let Ok(bytes) = bytes else {
            return -1;
        };
        self.write_compatibility_stream(StreamKind::Output, &bytes);
        0
    }
}
