use nshedit::domain::{Text, TextUnit};

const MAX_BINDING_UNITS: usize = 1023;

pub(super) fn decode_key_sequence(input: &[u32]) -> Option<Text> {
    let mut output = Text::default();
    let mut index = 0;
    while index < input.len() {
        if output.len() == MAX_BINDING_UNITS {
            return None;
        }
        let current = input[index];
        if current == b'M' as u32
            && input.get(index + 1) == Some(&(b'-' as u32))
            && input.get(index + 2).is_some()
        {
            output.push(TextUnit::Scalar('\u{1b}'));
            index += 2;
            continue;
        }
        if current == b'^' as u32 {
            let next = *input.get(index + 1)?;
            let value = if next == b'?' as u32 {
                0x7f
            } else {
                next & 0x9f
            };
            output.push(TextUnit::from_code_point(value));
            index += 2;
            continue;
        }
        if current != b'\\' as u32 {
            output.push(TextUnit::from_code_point(current));
            index += 1;
            continue;
        }

        let escaped = *input.get(index + 1)?;
        let named = match escaped {
            value if value == b'a' as u32 => Some(0x07),
            value if value == b'b' as u32 => Some(0x08),
            value if value == b't' as u32 => Some(0x09),
            value if value == b'n' as u32 => Some(0x0a),
            value if value == b'v' as u32 => Some(0x0b),
            value if value == b'f' as u32 => Some(0x0c),
            value if value == b'r' as u32 => Some(0x0d),
            value if value == b'e' as u32 => Some(0x1b),
            _ => None,
        };
        if let Some(value) = named {
            output.push(TextUnit::from_code_point(value));
            index += 2;
            continue;
        }
        if (b'0' as u32..=b'7' as u32).contains(&escaped) {
            let mut value = 0;
            let mut digits = 0;
            while digits < 3 {
                let Some(&digit) = input.get(index + 1 + digits) else {
                    break;
                };
                if !(b'0' as u32..=b'7' as u32).contains(&digit) {
                    break;
                }
                value = (value << 3) | (digit - b'0' as u32);
                digits += 1;
            }
            if value > 0xff {
                return None;
            }
            output.push(TextUnit::from_code_point(value));
            index += 1 + digits;
            continue;
        }
        if escaped == b'U' as u32 {
            if input.get(index + 2) != Some(&(b'+' as u32)) {
                return None;
            }
            let digits = &input.get(index + 3..)?;
            let count = digits
                .iter()
                .take(5)
                .take_while(|digit| hex_value(**digit).is_some())
                .count();
            if count < 4 {
                return None;
            }
            let mut value = 0;
            for &digit in &digits[..count] {
                value = (value << 4) | hex_value(digit)?;
            }
            if value > 0x10ffff {
                return None;
            }
            output.push(TextUnit::from_code_point(value));
            index += 3 + count;
            continue;
        }
        output.push(TextUnit::from_code_point(escaped));
        index += 2;
    }
    Some(output)
}

fn hex_value(value: u32) -> Option<u32> {
    match value {
        value if (b'0' as u32..=b'9' as u32).contains(&value) => Some(value - b'0' as u32),
        value if (b'A' as u32..=b'F' as u32).contains(&value) => Some(value - b'A' as u32 + 10),
        _ => None,
    }
}

pub(super) fn visual_text(text: &Text, quoted: bool) -> String {
    let mut output = String::new();
    if quoted {
        output.push('"');
    }
    for unit in text {
        match *unit {
            TextUnit::Scalar('\u{7f}') => output.push_str("^?"),
            TextUnit::Scalar(character) if character <= '\u{1f}' => {
                output.push('^');
                output.push(char::from_u32(u32::from(character) | 0x40).unwrap_or('?'));
            }
            TextUnit::Scalar('"') if quoted => output.push_str("\\\""),
            TextUnit::Scalar('\\') => output.push_str("\\\\"),
            TextUnit::Scalar(character) => output.push(character),
            TextUnit::RawByte(byte) => output.push_str(&format!("\\{byte:03o}")),
            TextUnit::OpaqueCodePoint(value) => {
                output.push_str(&format!("\\U+{:05X}", value.get()));
            }
        }
    }
    if quoted {
        output.push('"');
    }
    output
}

pub(super) fn text_bytes(text: &Text) -> Vec<u8> {
    let mut output = Vec::new();
    for unit in text {
        match *unit {
            TextUnit::Scalar(character) => {
                let mut encoded = [0; 4];
                output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            TextUnit::RawByte(byte) => output.push(byte),
            TextUnit::OpaqueCodePoint(_) => output.extend_from_slice("�".as_bytes()),
        }
    }
    output
}

pub(super) fn wide_bytes(input: &[u32]) -> Vec<u8> {
    text_bytes(
        &input
            .iter()
            .copied()
            .map(TextUnit::from_code_point)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(input: &str) -> Vec<u32> {
        input.chars().map(u32::from).collect()
    }

    fn decode(input: &str) -> Option<Text> {
        decode_key_sequence(&wide(input))
    }

    fn scalars(input: &str) -> Text {
        input.chars().map(TextUnit::Scalar).collect()
    }

    #[test]
    fn named_escapes_decode() {
        let decoded = decode("\\a\\b\\t\\n\\v\\f\\r\\e").unwrap();

        assert_eq!(
            decoded,
            Text::from_iter([
                TextUnit::Scalar('\u{7}'),
                TextUnit::Scalar('\u{8}'),
                TextUnit::Scalar('\t'),
                TextUnit::Scalar('\n'),
                TextUnit::Scalar('\u{b}'),
                TextUnit::Scalar('\u{c}'),
                TextUnit::Scalar('\r'),
                TextUnit::Scalar('\u{1b}'),
            ])
        );
    }

    #[test]
    fn controls_decode() {
        assert_eq!(
            decode("^@^A^Z^?"),
            Some(Text::from_iter([
                TextUnit::Scalar('\0'),
                TextUnit::Scalar('\u{1}'),
                TextUnit::Scalar('\u{1a}'),
                TextUnit::Scalar('\u{7f}'),
            ]))
        );
        assert_eq!(decode("^"), None);
    }

    #[test]
    fn meta_prefixes_decode() {
        assert_eq!(
            decode("M-xM-^A"),
            Some(Text::from_iter([
                TextUnit::Scalar('\u{1b}'),
                TextUnit::Scalar('x'),
                TextUnit::Scalar('\u{1b}'),
                TextUnit::Scalar('\u{1}'),
            ]))
        );
        assert_eq!(decode("M-"), Some(scalars("M-")));
    }

    #[test]
    fn octal_escapes_decode() {
        assert_eq!(
            decode("\\0\\07\\101\\377"),
            Some(Text::from_iter([
                TextUnit::Scalar('\0'),
                TextUnit::Scalar('\u{7}'),
                TextUnit::Scalar('A'),
                TextUnit::Scalar('\u{ff}'),
            ]))
        );
        assert_eq!(decode("\\400"), None);
        assert_eq!(decode("\\777"), None);
    }

    #[test]
    fn wide_escapes_decode() {
        assert_eq!(
            decode("\\U+0041\\U+1F642"),
            Some(Text::from_iter([
                TextUnit::Scalar('A'),
                TextUnit::Scalar('🙂'),
            ]))
        );
        assert_eq!(
            decode("\\U+D800"),
            Some(Text::from_iter([TextUnit::from_code_point(0xd800)]))
        );
        assert_eq!(decode("\\U+123"), None);
        assert_eq!(decode("\\U-0041"), None);
        assert_eq!(decode("\\U+00ff"), None);
    }

    #[test]
    fn ordinary_escapes_decode() {
        assert_eq!(decode("\\\\\\\"\\q"), Some(scalars("\\\"q")));
        assert_eq!(decode("trailing\\"), None);
        assert_eq!(decode("plain"), Some(scalars("plain")));
    }

    #[test]
    fn decoder_is_bounded() {
        let at_limit = vec![u32::from(b'x'); MAX_BINDING_UNITS];
        let over_limit = vec![u32::from(b'x'); MAX_BINDING_UNITS + 1];

        assert_eq!(
            decode_key_sequence(&at_limit).map(|text| text.len()),
            Some(MAX_BINDING_UNITS)
        );
        assert_eq!(decode_key_sequence(&over_limit), None);

        let mut expanded = vec![u32::from(b'x'); MAX_BINDING_UNITS - 1];
        expanded.extend(wide("M-x"));
        assert_eq!(decode_key_sequence(&expanded), None);
    }

    #[test]
    fn visual_controls_are_stable() {
        let text = Text::from_iter([
            TextUnit::Scalar('\0'),
            TextUnit::Scalar('\u{7f}'),
            TextUnit::Scalar('"'),
            TextUnit::Scalar('\\'),
            TextUnit::RawByte(0xff),
            TextUnit::from_code_point(0xd800),
            TextUnit::Scalar('é'),
        ]);

        assert_eq!(visual_text(&text, true), "\"^@^?\\\"\\\\\\377\\U+0D800é\"");
        assert_eq!(visual_text(&scalars("text"), false), "text");
    }

    #[test]
    fn visual_form_normalizes_bytes() {
        let original = Text::from_iter([
            TextUnit::Scalar('\u{1b}'),
            TextUnit::Scalar('A'),
            TextUnit::RawByte(0xff),
            TextUnit::from_code_point(0xd800),
        ]);
        let visual = visual_text(&original, false);

        assert_eq!(
            decode(&visual),
            Some(Text::from_iter([
                TextUnit::Scalar('\u{1b}'),
                TextUnit::Scalar('A'),
                TextUnit::Scalar('\u{ff}'),
                TextUnit::from_code_point(0xd800),
            ]))
        );
    }

    #[test]
    fn byte_projection_is_explicit() {
        let text = Text::from_iter([
            TextUnit::Scalar('A'),
            TextUnit::Scalar('é'),
            TextUnit::RawByte(0xff),
            TextUnit::from_code_point(0xd800),
        ]);

        assert_eq!(
            text_bytes(&text),
            [b"A".as_slice(), "é".as_bytes(), &[0xff], "�".as_bytes()].concat()
        );
        assert_eq!(
            wide_bytes(&[u32::from(b'A'), 0xd800, u32::from('é')]),
            [b"A".as_slice(), "�".as_bytes(), "é".as_bytes()].concat()
        );
    }
}
