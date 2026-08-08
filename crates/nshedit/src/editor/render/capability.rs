use std::num::NonZeroU32;

use nshterm::TermInfo;
use nshterm::parm::{Error as ExpansionError, Param, Variables, expand};

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
    ClearScreen,
    ClearToEndOfLine,
    CursorAddress,
    Bell,
    CarriageReturn,
}

impl CapabilityKind {
    pub(super) const fn description(self) -> &'static str {
        match self {
            Self::ClearScreen => "clear screen",
            Self::ClearToEndOfLine => "clear to end of line",
            Self::CursorAddress => "cursor address",
            Self::Bell => "bell",
            Self::CarriageReturn => "carriage return",
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
    clear_screen: Option<Capability>,
    clear_to_end: Option<Capability>,
    cursor_address: Option<Capability>,
    bell: Capability,
    carriage_return: Capability,
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
            clear_screen: Some(Capability::from(&b"\x1b[H\x1b[2J"[..])),
            clear_to_end: Some(Capability::from(&b"\x1b[K"[..])),
            cursor_address: Some(Capability::from(&b"\x1b[%i%p1%d;%p2%dH"[..])),
            bell: Capability::from(&b"\x07"[..]),
            carriage_return: Capability::from(&b"\r"[..]),
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
            clear_screen: None,
            clear_to_end: None,
            cursor_address: None,
            bell: Capability::from(&b"\x07"[..]),
            carriage_return: Capability::from(&b"\r"[..]),
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
                .strings
                .get(name)
                .filter(|bytes| !bytes.is_empty())
                .cloned()
                .map(|bytes| Capability::from(bytes.into_boxed_slice()))
        };
        Self {
            name: entry
                .names
                .first()
                .map_or_else(|| Box::<str>::from("terminfo"), |name| name.clone().into()),
            clear_screen: cap("clear"),
            clear_to_end: cap("el"),
            cursor_address: cap("cup"),
            bell: cap("bel").unwrap_or_else(|| Capability::from(&b"\x07"[..])),
            carriage_return: cap("cr").unwrap_or_else(|| Capability::from(&b"\r"[..])),
            pad_byte: entry
                .strings
                .get("pad")
                .and_then(|bytes| bytes.first().copied())
                .unwrap_or(0),
            flow_controlled: entry.bools.get("xon").copied().unwrap_or(false),
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

    pub(super) fn has_cursor_address(&self) -> bool {
        self.cursor_address.is_some()
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
            CapabilityKind::ClearScreen => self.clear_screen.as_ref(),
            CapabilityKind::ClearToEndOfLine => self.clear_to_end.as_ref(),
            CapabilityKind::CursorAddress => self.cursor_address.as_ref(),
            CapabilityKind::Bell => Some(&self.bell),
            CapabilityKind::CarriageReturn => Some(&self.carriage_return),
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
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn terminfo_profile_owns_capabilities() {
        let mut strings = HashMap::new();
        strings.insert("cup", b"[%p1%d,%p2%d]".to_vec());
        let entry = TermInfo {
            names: vec!["test".into()],
            bools: HashMap::new(),
            numbers: HashMap::new(),
            strings,
        };
        let profile = TerminalProfile::from_terminfo(&entry);
        assert_eq!(profile.name(), "test");
        assert!(profile.has_cursor_address());
        assert!(profile.clear_screen.is_none());
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
}
