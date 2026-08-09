//! Locale-sensitive conversion owned by the C boundary.
//!
//! The native editor stores [`nshedit::domain::Text`]. Multibyte strings,
//! `wchar_t` strings, grow-only conversion storage, and pointers borrowed
//! from that storage are properties of the libedit ABI, so they are kept in
//! this crate and never cross into the core.

use std::cell::Cell;

const GROWTH: usize = 1024;
const MAX_MULTIBYTE_LENGTH: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrefixDecode {
    Complete(u32),
    Incomplete,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Ascii,
    Utf8,
}

thread_local! {
    static ENCODING: Cell<Option<Encoding>> = const { Cell::new(None) };
}

fn current_encoding() -> Encoding {
    ENCODING.with(|cached| match cached.get() {
        Some(encoding) => encoding,
        None => {
            let encoding = ["LC_ALL", "LC_CTYPE", "LANG"]
                .into_iter()
                .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
                .map_or(Encoding::Ascii, |name| {
                    encoding_from_locale(name.as_encoded_bytes())
                });
            cached.set(Some(encoding));
            encoding
        }
    })
}

pub(crate) fn max_multibyte_length() -> usize {
    match current_encoding() {
        Encoding::Ascii => 1,
        Encoding::Utf8 => MAX_MULTIBYTE_LENGTH,
    }
}

/// Classify one locale-encoded character without consuming a following one.
///
/// The direct `el_wgetc` boundary reads a byte at a time. Keeping this
/// distinction here prevents that ABI operation from borrowing the native
/// driver's UTF-8 decoder or discarding the rest of a larger descriptor read.
pub(crate) fn decode_prefix(input: &[u8]) -> PrefixDecode {
    let Some(&first) = input.first() else {
        return PrefixDecode::Incomplete;
    };
    match current_encoding() {
        Encoding::Ascii => {
            if first < 0x80 {
                PrefixDecode::Complete(u32::from(first))
            } else {
                PrefixDecode::Invalid
            }
        }
        Encoding::Utf8 => {
            let length = match first {
                0x00..=0x7f => return PrefixDecode::Complete(u32::from(first)),
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf7 => 4,
                0xf8..=0xfb => 5,
                0xfc..=0xfd => 6,
                _ => return PrefixDecode::Invalid,
            };
            if input
                .iter()
                .take(length)
                .skip(1)
                .any(|byte| !(0x80..=0xbf).contains(byte))
            {
                return PrefixDecode::Invalid;
            }
            if input.len() < length {
                return PrefixDecode::Incomplete;
            }
            decode_value(Encoding::Utf8, input).map_or(PrefixDecode::Invalid, |(value, _)| {
                PrefixDecode::Complete(value)
            })
        }
    }
}

fn encoding_from_locale(locale: &[u8]) -> Encoding {
    let without_modifier = locale.split(|byte| *byte == b'@').next().unwrap_or(locale);
    let Some(separator) = without_modifier.iter().position(|byte| *byte == b'.') else {
        return Encoding::Ascii;
    };
    let codeset = &without_modifier[separator + 1..];
    let utf8 = codeset
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b'-' | b'_'))
        .map(|byte| byte.to_ascii_uppercase())
        .eq(b"UTF8".iter().copied());
    if utf8 {
        Encoding::Utf8
    } else {
        Encoding::Ascii
    }
}

/// The two independent grow-only buffers whose addresses the narrow ABI may
/// lend to a caller.
#[derive(Debug, Default)]
pub(crate) struct ConversionBuffer {
    bytes: Vec<u8>,
    wide: Vec<u32>,
}

impl ConversionBuffer {
    pub(crate) const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            wide: Vec::new(),
        }
    }

    pub(crate) fn from_parts(bytes: Vec<u8>, wide: Vec<u32>) -> Self {
        Self { bytes, wide }
    }

    pub(crate) fn into_parts(self) -> (Vec<u8>, Vec<u32>) {
        (self.bytes, self.wide)
    }

    pub(crate) fn byte_allocation(&self) -> usize {
        self.bytes.len()
    }

    fn grow_bytes(&mut self, new_length: usize) -> Option<()> {
        if new_length <= self.bytes.len() {
            return Some(());
        }
        let additional = new_length.checked_sub(self.bytes.len())?;
        if self.bytes.try_reserve_exact(additional).is_err() {
            self.bytes = Vec::new();
            return None;
        }
        self.bytes.resize(new_length, 0);
        Some(())
    }

    fn grow_wide(&mut self, new_length: usize) -> Option<()> {
        if new_length <= self.wide.len() {
            return Some(());
        }
        let additional = new_length.checked_sub(self.wide.len())?;
        if self.wide.try_reserve_exact(additional).is_err() {
            self.wide = Vec::new();
            return None;
        }
        self.wide.resize(new_length, 0);
        Some(())
    }
}

/// Encode a possibly NUL-terminated wide string into the buffer's byte half.
///
/// The returned contents exclude the terminator, which is nevertheless
/// written immediately after them for the C caller. Values unavailable in
/// the active encoding are dropped, matching libedit's historical boundary.
pub(crate) fn encode_wide<'a>(
    input: Option<&[u32]>,
    buffer: &'a mut ConversionBuffer,
) -> Option<&'a [u8]> {
    let input = before_nul(input?);
    let encoding = current_encoding();
    let mut used = 0usize;

    for &value in input {
        if buffer.bytes.len().saturating_sub(used) < 5 {
            let grown = buffer.bytes.len().checked_add(GROWTH)?;
            buffer.grow_bytes(grown)?;
        }
        let written = encode_value(encoding, value, &mut buffer.bytes[used..used + 5]);
        if written < 0 {
            return None;
        }
        used = used.checked_add(written as usize)?;
    }

    if buffer.bytes.len().saturating_sub(used) < 5 {
        let grown = buffer.bytes.len().checked_add(GROWTH)?;
        buffer.grow_bytes(grown)?;
    }
    buffer.bytes[used] = 0;
    Some(&buffer.bytes[..used])
}

/// Decode a possibly NUL-terminated multibyte string into the buffer's wide
/// half. Invalid or incomplete input rejects the complete string.
pub(crate) fn decode_bytes<'a>(
    input: Option<&[u8]>,
    buffer: &'a mut ConversionBuffer,
) -> Option<&'a [u32]> {
    let input = before_nul(input?);
    let encoding = current_encoding();
    let mut decoded = Vec::new();
    let mut offset = 0usize;
    while offset < input.len() {
        let (value, consumed) = decode_value(encoding, &input[offset..])?;
        decoded.push(value);
        offset = offset.checked_add(consumed)?;
    }

    let required = decoded.len().checked_add(1)?;
    if buffer.wide.len() < required {
        buffer.grow_wide(required.checked_add(GROWTH)?)?;
    }
    buffer.wide[..decoded.len()].copy_from_slice(&decoded);
    buffer.wide[decoded.len()] = 0;
    Some(&buffer.wide[..decoded.len()])
}

/// Number of bytes one wide value occupies, or zero when it has no
/// representation in the active locale.
pub(crate) fn encoded_width(value: u32) -> usize {
    let mut scratch = [0; MAX_MULTIBYTE_LENGTH];
    encode_value(current_encoding(), value, &mut scratch).max(0) as usize
}

/// Encode one wide value. Zero means unrepresentable and -1 means the caller
/// supplied too little output space.
pub(crate) fn encode_one(output: &mut [u8], value: u32) -> isize {
    if output.is_empty() {
        return -1;
    }
    let needed = encoded_width(value);
    if output.len() < needed {
        return -1;
    }
    encode_value(current_encoding(), value, output)
}

fn before_nul<T: Copy + Default + PartialEq>(input: &[T]) -> &[T] {
    input
        .iter()
        .position(|value| *value == T::default())
        .map_or(input, |end| &input[..end])
}

fn decode_value(encoding: Encoding, input: &[u8]) -> Option<(u32, usize)> {
    let first = *input.first()?;
    match encoding {
        Encoding::Ascii => (first < 0x80).then_some((u32::from(first), 1)),
        Encoding::Utf8 => {
            let (length, mut value, minimum) = match first {
                0x00..=0x7f => return Some((u32::from(first), 1)),
                0xc2..=0xdf => (2usize, u32::from(first & 0x1f), 0x80u32),
                0xe0..=0xef => (3, u32::from(first & 0x0f), 0x800),
                0xf0..=0xf7 => (4, u32::from(first & 0x07), 0x1_0000),
                0xf8..=0xfb => (5, u32::from(first & 0x03), 0x20_0000),
                0xfc..=0xfd => (6, u32::from(first & 0x01), 0x400_0000),
                _ => return None,
            };
            for &continuation in input.get(1..length)? {
                if !(0x80..=0xbf).contains(&continuation) {
                    return None;
                }
                value = (value << 6) | u32::from(continuation & 0x3f);
            }
            if value < minimum || (0xd800..=0xdfff).contains(&value) {
                None
            } else {
                Some((value, length))
            }
        }
    }
}

fn encode_value(encoding: Encoding, value: u32, output: &mut [u8]) -> isize {
    match encoding {
        Encoding::Ascii => {
            if value < 0x80 {
                output[0] = value as u8;
                1
            } else {
                0
            }
        }
        Encoding::Utf8 => {
            if (0xd800..=0xdfff).contains(&value) || value > 0x7fff_ffff {
                return 0;
            }
            let (length, lead) = match value {
                0x0000_0000..=0x0000_007f => {
                    output[0] = value as u8;
                    return 1;
                }
                0x0000_0080..=0x0000_07ff => (2usize, 0xc0u8),
                0x0000_0800..=0x0000_ffff => (3, 0xe0),
                0x0001_0000..=0x001f_ffff => (4, 0xf0),
                0x0020_0000..=0x03ff_ffff => (5, 0xf8),
                _ => (6, 0xfc),
            };
            if output.len() < length {
                return -1;
            }
            for index in (1..length).rev() {
                output[index] = 0x80 | ((value >> (6 * (length - 1 - index))) & 0x3f) as u8;
            }
            output[0] = lead | (value >> (6 * (length - 1))) as u8;
            length as isize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_parser_recognises_explicit_utf8() {
        assert_eq!(encoding_from_locale(b"C"), Encoding::Ascii);
        assert_eq!(encoding_from_locale(b"POSIX"), Encoding::Ascii);
        assert_eq!(encoding_from_locale(b"C.UTF-8"), Encoding::Utf8);
        assert_eq!(encoding_from_locale(b"sv_SE.utf8@euro"), Encoding::Utf8);
        assert_eq!(encoding_from_locale(b"sv_SE.ISO-8859-1"), Encoding::Ascii);
    }

    #[test]
    fn extended_utf8_round_trips() {
        let mut encoded = [0; MAX_MULTIBYTE_LENGTH];
        for value in [
            0x7f, 0x80, 0x7ff, 0x800, 0xffff, 0x1_0000, 0x20_0000, 0x400_0000,
        ] {
            let length = encode_value(Encoding::Utf8, value, &mut encoded) as usize;
            assert_eq!(
                decode_value(Encoding::Utf8, &encoded[..length]),
                Some((value, length))
            );
        }
        assert_eq!(decode_value(Encoding::Utf8, &[0xc0, 0x80]), None);
        assert_eq!(decode_value(Encoding::Utf8, &[0xed, 0xa0, 0x80]), None);
    }
}
