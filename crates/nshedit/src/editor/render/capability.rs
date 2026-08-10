use std::num::NonZeroU32;

use nshterm::parm::{Error as ExpansionError, Param, Variables, expand};
use nshterm::{CapabilityName, TermInfo};

/// A real line speed in transmitted bits per second.
///
/// This deliberately does not expose an encoded POSIX `speed_t`. Native
/// rendering needs the semantic rate only to realise terminfo padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BaudRate(NonZeroU32);

impl BaudRate {
    /// Construct a non-zero bit rate.
    #[must_use]
    pub const fn new(bits_per_second: u32) -> Option<Self> {
        match NonZeroU32::new(bits_per_second) {
            Some(rate) => Some(Self(rate)),
            None => None,
        }
    }

    /// Recover the semantic bit rate.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// A terminal operation whose terminfo expression may fail to expand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityKind {
    ClearToEndOfLine,
    Bell,
    CarriageReturn,
    SaveCursor,
    RestoreCursor,
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
}

impl CapabilityKind {
    pub(super) const fn description(self) -> &'static str {
        match self {
            Self::ClearToEndOfLine => "clear to end of line",
            Self::Bell => "bell",
            Self::CarriageReturn => "carriage return",
            Self::SaveCursor => "save cursor",
            Self::RestoreCursor => "restore cursor",
            Self::CursorUp => "cursor up",
            Self::CursorDown => "cursor down",
            Self::CursorLeft => "cursor left",
            Self::CursorRight => "cursor right",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Capability(Box<[u8]>);

impl Capability {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<&[u8]> for Capability {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.into())
    }
}

impl From<Box<[u8]>> for Capability {
    fn from(bytes: Box<[u8]>) -> Self {
        Self(bytes)
    }
}

// [spec:nshedit:req:core.terminal-render+1]
/// Owned terminal capabilities used by the native renderer.
///
/// Capability values remain bytes because terminfo may contain non-UTF-8
/// control sequences. Every field is private and no global terminfo entry or
/// output destination is installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalProfile {
    name: Box<str>,
    clear_to_end: Option<Capability>,
    bell: Capability,
    carriage_return: Capability,
    save_cursor: Option<Capability>,
    restore_cursor: Option<Capability>,
    cursor_up: Option<Capability>,
    cursor_down: Option<Capability>,
    cursor_left: Capability,
    cursor_right: Option<Capability>,
    pad_byte: u8,
    flow_controlled: bool,
    baud_rate: Option<BaudRate>,
}

impl TerminalProfile {
    /// A portable ANSI profile suitable when ANSI support is already known.
    #[must_use]
    pub fn ansi() -> Self {
        Self {
            name: "ansi".into(),
            clear_to_end: Some(Capability::from(&b"\x1b[K"[..])),
            bell: Capability::from(&b"\x07"[..]),
            carriage_return: Capability::from(&b"\r"[..]),
            save_cursor: Some(Capability::from(&b"\x1b7"[..])),
            restore_cursor: Some(Capability::from(&b"\x1b8"[..])),
            cursor_up: Some(Capability::from(&b"\x1b[A"[..])),
            cursor_down: Some(Capability::from(&b"\x1b[B"[..])),
            cursor_left: Capability::from(&b"\x08"[..]),
            cursor_right: Some(Capability::from(&b"\x1b[C"[..])),
            pad_byte: 0,
            flow_controlled: false,
            baud_rate: None,
        }
    }

    /// A one-line profile with no cursor-addressing assumption.
    #[must_use]
    pub fn plain() -> Self {
        Self {
            name: "plain".into(),
            clear_to_end: None,
            bell: Capability::from(&b"\x07"[..]),
            carriage_return: Capability::from(&b"\r"[..]),
            save_cursor: None,
            restore_cursor: None,
            cursor_up: None,
            cursor_down: None,
            cursor_left: Capability::from(&b"\x08"[..]),
            cursor_right: None,
            pad_byte: 0,
            flow_controlled: false,
            baud_rate: None,
        }
    }

    /// Copy the capabilities needed for line rendering from one parsed entry.
    #[must_use]
    pub fn from_terminfo(entry: &TermInfo) -> Self {
        let cap = |name| {
            entry
                .string(CapabilityName::Terminfo(name))
                .filter(|bytes| !bytes.is_empty())
                .map(|bytes| Capability::from(bytes.into_owned().into_boxed_slice()))
        };
        Self {
            name: entry
                .names()
                .first()
                .map_or_else(|| Box::<str>::from("terminfo"), |name| name.clone().into()),
            clear_to_end: cap("el"),
            bell: cap("bel").unwrap_or_else(|| Capability::from(&b"\x07"[..])),
            carriage_return: cap("cr").unwrap_or_else(|| Capability::from(&b"\r"[..])),
            save_cursor: cap("sc"),
            restore_cursor: cap("rc"),
            cursor_up: cap("cuu1"),
            cursor_down: cap("cud1"),
            cursor_left: cap("cub1").unwrap_or_else(|| Capability::from(&b"\x08"[..])),
            cursor_right: cap("cuf1"),
            pad_byte: entry
                .string(CapabilityName::Terminfo("pad"))
                .and_then(|bytes| bytes.first().copied())
                .unwrap_or(0),
            flow_controlled: entry
                .boolean(CapabilityName::Terminfo("xon"))
                .unwrap_or(false),
            baud_rate: None,
        }
    }

    /// Set the semantic line speed used to realise padding runs.
    #[must_use]
    pub const fn with_baud_rate(mut self, baud_rate: Option<BaudRate>) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    /// The terminal name carried by this profile.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The configured semantic line speed, if known.
    #[must_use]
    pub const fn baud_rate(&self) -> Option<BaudRate> {
        self.baud_rate
    }

    /// Expand one owned terminal sequence and realise its padding markers.
    ///
    /// Terminal databases carry byte strings rather than text, and some
    /// applications need capabilities beyond the renderer's fixed semantic
    /// operations. Numeric parameters stay ordinary Rust values; the
    /// terminfo stack language and padding calculation remain encapsulated.
    pub fn expand_sequence(
        &self,
        sequence: &[u8],
        parameters: &[i32],
        affected_lines: usize,
    ) -> Result<Vec<u8>, ExpansionError> {
        let parameters: Vec<Param> = parameters.iter().copied().map(Param::Number).collect();
        let expanded = expand(sequence, &parameters, &mut Variables::new())?;
        let mut output = Vec::with_capacity(expanded.len());
        self.append_padded(&mut output, &expanded, affected_lines);
        Ok(output)
    }

    pub(super) fn has_relative_region_addressing(&self) -> bool {
        self.save_cursor.is_some()
            && self.restore_cursor.is_some()
            && self.cursor_up.is_some()
            && self.cursor_down.is_some()
    }

    pub(super) fn has_cursor_right(&self) -> bool {
        self.cursor_right.is_some()
    }

    pub(super) fn append(
        &self,
        output: &mut Vec<u8>,
        kind: CapabilityKind,
        params: &[Param],
        affected_lines: usize,
        variables: &mut Variables,
    ) -> Result<bool, ExpansionError> {
        let capability = match kind {
            CapabilityKind::ClearToEndOfLine => self.clear_to_end.as_ref(),
            CapabilityKind::Bell => Some(&self.bell),
            CapabilityKind::CarriageReturn => Some(&self.carriage_return),
            CapabilityKind::SaveCursor => self.save_cursor.as_ref(),
            CapabilityKind::RestoreCursor => self.restore_cursor.as_ref(),
            CapabilityKind::CursorUp => self.cursor_up.as_ref(),
            CapabilityKind::CursorDown => self.cursor_down.as_ref(),
            CapabilityKind::CursorLeft => Some(&self.cursor_left),
            CapabilityKind::CursorRight => self.cursor_right.as_ref(),
        };
        let Some(capability) = capability else {
            return Ok(false);
        };
        let expanded = expand(capability.as_bytes(), params, variables)?;
        self.append_padded(output, &expanded, affected_lines);
        Ok(true)
    }

    fn append_padded(&self, output: &mut Vec<u8>, bytes: &[u8], affected_lines: usize) {
        let bits_per_second = self.baud_rate.map_or(0, BaudRate::get);
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'$' || bytes.get(index + 1) != Some(&b'<') {
                output.push(bytes[index]);
                index += 1;
                continue;
            }
            let Some(close) = bytes[index + 2..]
                .iter()
                .position(|&byte| byte == b'>')
                .map(|offset| index + 2 + offset)
            else {
                output.push(bytes[index]);
                index += 1;
                continue;
            };
            let Some(delay) = parse_delay(&bytes[index + 2..close]) else {
                output.extend_from_slice(&bytes[index..=close]);
                index = close + 1;
                continue;
            };
            index = close + 1;
            if !delay.mandatory && self.flow_controlled {
                continue;
            }
            let line_factor = if delay.per_line { affected_lines } else { 1 };
            let tenths = u64::from(delay.tenths).saturating_mul(line_factor as u64);
            let count = tenths.saturating_mul(u64::from(bits_per_second)) / 100_000;
            output.extend(std::iter::repeat_n(self.pad_byte, count as usize));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Delay {
    tenths: u32,
    per_line: bool,
    mandatory: bool,
}

fn parse_delay(body: &[u8]) -> Option<Delay> {
    let mut index = 0;
    let mut milliseconds = 0_u32;
    while body.get(index).is_some_and(u8::is_ascii_digit) {
        milliseconds = milliseconds
            .saturating_mul(10)
            .saturating_add(u32::from(body[index] - b'0'));
        index += 1;
    }
    if index == 0 {
        return None;
    }
    let mut tenths = milliseconds.saturating_mul(10);
    if body.get(index) == Some(&b'.') {
        index += 1;
        let digit = body.get(index).copied().filter(u8::is_ascii_digit)?;
        tenths = tenths.saturating_add(u32::from(digit - b'0'));
        index += 1;
    }
    let mut per_line = false;
    let mut mandatory = false;
    for &modifier in &body[index..] {
        match modifier {
            b'*' if !per_line => per_line = true,
            b'/' if !mandatory => mandatory = true,
            _ => return None,
        }
    }
    Some(Delay {
        tenths,
        per_line,
        mandatory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nshterm::TermInfoBuilder;

    #[test]
    fn terminfo_profile_owns_capabilities() {
        let entry = TermInfoBuilder::default()
            .named("test")
            .string("sc", b"<save>")
            .string("rc", b"<restore>")
            .string("cuu1", b"<up>")
            .string("cud1", b"<down>")
            .build();
        let profile = TerminalProfile::from_terminfo(&entry);
        assert_eq!(profile.name(), "test");
        assert!(profile.has_relative_region_addressing());
    }

    #[test]
    fn padding_uses_semantic_baud_rate() {
        let profile = TerminalProfile {
            bell: Capability::from(&b"x$<5/>y"[..]),
            pad_byte: b'.',
            ..TerminalProfile::plain().with_baud_rate(BaudRate::new(9_600))
        };
        let mut output = Vec::new();
        profile
            .append(
                &mut output,
                CapabilityKind::Bell,
                &[],
                1,
                &mut Variables::new(),
            )
            .unwrap();
        assert_eq!(output, b"x....y");
    }

    #[test]
    fn malformed_padding_stays_verbatim() {
        let profile = TerminalProfile {
            bell: Capability::from(&b"x$<bad>y$<5"[..]),
            ..TerminalProfile::plain()
        };
        let mut output = Vec::new();
        profile
            .append(
                &mut output,
                CapabilityKind::Bell,
                &[],
                1,
                &mut Variables::new(),
            )
            .unwrap();
        assert_eq!(output, b"x$<bad>y$<5");
    }

    #[test]
    fn expands_typed_parameters_with_padding() {
        let profile = TerminalProfile {
            pad_byte: b'.',
            ..TerminalProfile::plain().with_baud_rate(BaudRate::new(9_600))
        };
        let output = profile
            .expand_sequence(b"[%p1%d,%p2%d]$<5/>", &[4, 12], 1)
            .unwrap();
        assert_eq!(output, b"[4,12]....");
    }
}
