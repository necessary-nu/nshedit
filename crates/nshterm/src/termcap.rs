//! Termcap-visible projections that are not plain capability-name aliases.

use crate::TermInfo;
use crate::parm::{Param, Variables, expand};
use crate::parser::names::{STRING_CODES, STRING_NAMES};

fn capname(code: &str) -> Option<&'static str> {
    STRING_CODES
        .iter()
        .position(|candidate| *candidate == code)
        .map(|index| STRING_NAMES[index])
}

// [spec:nshedit:req:abi.termcap-view]
pub(super) fn string(entry: &TermInfo, code: &str) -> Option<Vec<u8>> {
    let name = capname(code)?;
    let raw = entry.strings.get(name)?;
    if code == "me" {
        Some(trim_attribute_reset(entry).unwrap_or_else(|| raw.clone()))
    } else {
        Some(raw.clone())
    }
}

fn trim_attribute_reset(entry: &TermInfo) -> Option<Vec<u8>> {
    let raw = entry.strings.get("sgr0")?;
    let set_attributes = entry.strings.get("sgr")?;
    let mut off = expand_attributes(set_attributes, 0)?;
    let mut on = expand_attributes(set_attributes, 1)?;
    let mut end = raw.clone();

    move_prefix_to_end(&mut on, entry.strings.get("smacs").map(Vec::as_slice));
    move_prefix_to_end(&mut off, entry.strings.get("rmacs").map(Vec::as_slice));
    move_prefix_to_end(&mut end, entry.strings.get("rmacs").map(Vec::as_slice));
    if !similar_sgr(&off, &end) || similar_sgr(&off, &on) {
        return Some(raw.clone());
    }

    let mut changed = entry
        .strings
        .get("rmacs")
        .and_then(|part| remove_part(&mut off, part));
    if changed.is_none() {
        changed = remove_sgr_ten(&mut off);
    }
    let result = if changed.is_some() {
        off
    } else if let Some(start) = find_subslice(&end, &off).filter(|_| end != off) {
        end.drain(start..start + off.len());
        end
    } else {
        raw.clone()
    };
    Some(if result == *raw {
        result
    } else {
        without_padding(&result)
    })
}

fn expand_attributes(sequence: &[u8], alternate_character_set: i32) -> Option<Vec<u8>> {
    let mut parameters: [Param; 9] = std::array::from_fn(|_| Param::Number(0));
    parameters[8] = Param::Number(alternate_character_set);
    expand(sequence, &parameters, &mut Variables::new()).ok()
}

fn move_prefix_to_end(sequence: &mut [u8], prefix: Option<&[u8]>) {
    let Some(prefix) = prefix.filter(|prefix| sequence.len() > prefix.len()) else {
        return;
    };
    if sequence.starts_with(prefix) {
        sequence.rotate_left(prefix.len());
    }
}

fn csi_length(sequence: &[u8]) -> usize {
    if sequence.first() == Some(&0x9b) {
        1
    } else if sequence.starts_with(b"\x1b[") {
        2
    } else {
        0
    }
}

fn skip_zero(sequence: &[u8], index: usize) -> usize {
    if sequence.get(index) != Some(&b'0') {
        return index;
    }
    match sequence.get(index + 1) {
        Some(b';') => index + 2,
        Some(next) if next.is_ascii_alphabetic() => index + 1,
        _ => index,
    }
}

fn similar_sgr(left: &[u8], right: &[u8]) -> bool {
    let mut left_start = 0;
    let mut right_start = 0;
    let left_csi = csi_length(left);
    let right_csi = csi_length(right);
    if left_csi != 0 && left_csi == right_csi {
        left_start = left_csi;
        right_start = right_csi;
        if left.get(left_start) != right.get(right_start) {
            left_start = skip_zero(left, left_start);
            right_start = skip_zero(right, right_start);
        }
    }
    let left = &left[left_start..];
    let right = &right[right_start..];
    !left.is_empty() && !right.is_empty() && (left.starts_with(right) || right.starts_with(left))
}

fn skip_delay(sequence: &[u8], start: usize) -> usize {
    if sequence.get(start..start + 2) != Some(b"$<") {
        return start;
    }
    let mut index = start + 2;
    let mut saw_digit = false;
    while sequence
        .get(index)
        .is_some_and(|byte| byte.is_ascii_digit() || matches!(*byte, b'.' | b'*' | b'/'))
    {
        saw_digit |= sequence[index].is_ascii_digit();
        index += 1;
    }
    if saw_digit && sequence.get(index) == Some(&b'>') {
        index + 1
    } else {
        start
    }
}

fn matching_length(part: &[u8], whole: &[u8]) -> Option<usize> {
    let mut part_index = 0;
    let mut whole_index = 0;
    let mut used = 0;
    let mut delayed = 0;
    while part_index < part.len() {
        if part.get(part_index) != whole.get(whole_index) {
            return None;
        }
        if delayed != 0 {
            used += delayed;
            delayed = 0;
        }
        if part[part_index] == b'$' {
            let next_part = skip_delay(part, part_index);
            let next_whole = skip_delay(whole, whole_index);
            if next_part != part_index && next_whole != whole_index {
                delayed += next_whole - whole_index;
                part_index = next_part;
                whole_index = next_whole;
                continue;
            }
        }
        part_index += 1;
        whole_index += 1;
        used += 1;
    }
    Some(used)
}

fn remove_part(whole: &mut Vec<u8>, part: &[u8]) -> Option<()> {
    if part.is_empty() {
        return None;
    }
    for start in 0..whole.len() {
        if let Some(length) = matching_length(part, &whole[start..]).filter(|&length| length != 0) {
            whole.drain(start..start + length);
            return Some(());
        }
    }
    None
}

fn remove_sgr_ten(sequence: &mut Vec<u8>) -> Option<()> {
    let csi = csi_length(sequence);
    if csi == 0 || sequence.last() != Some(&b'm') {
        return None;
    }
    let ten = skip_zero(sequence, csi);
    if sequence.get(ten) != Some(&b'1') || skip_zero(sequence, ten + 1) == ten + 1 {
        return None;
    }
    let start = ten.saturating_sub(usize::from(sequence.get(ten - 1) == Some(&b';')));
    let end = skip_zero(sequence, ten + 1);
    sequence.drain(start..end);
    Some(())
}

fn find_subslice(whole: &[u8], part: &[u8]) -> Option<usize> {
    (!part.is_empty())
        .then(|| whole.windows(part.len()).position(|window| window == part))
        .flatten()
}

fn without_padding(sequence: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(sequence.len());
    let mut index = 0;
    while index < sequence.len() {
        let next = skip_delay(sequence, index);
        if next == index {
            result.push(sequence[index]);
            index += 1;
        } else {
            index = next;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapabilityName, TermInfoBuilder};

    fn entry(sgr0: &[u8], sgr: &[u8], smacs: &[u8], rmacs: &[u8]) -> TermInfoBuilder {
        TermInfoBuilder::default()
            .named("test")
            .string("sgr0", sgr0)
            .string("sgr", sgr)
            .string("smacs", smacs)
            .string("rmacs", rmacs)
    }

    /// What `tgetstr` would answer for `code`.
    fn termcap(term: &TermInfo, code: &str) -> Option<Vec<u8>> {
        term.string(CapabilityName::Termcap(code))
            .map(std::borrow::Cow::into_owned)
    }

    // [spec:nshedit:req:abi.termcap-view/test]
    #[test]
    fn xterm_reset_preserves_charset() {
        let term = entry(
            b"\x1b(B\x1b[m",
            b"%?%p9%t\x1b(0%e\x1b(B%;\x1b[0m",
            b"\x1b(0",
            b"\x1b(B",
        )
        .build();
        assert_eq!(termcap(&term, "me"), Some(b"\x1b[0m".to_vec()));
    }

    #[test]
    fn ansi_reset_removes_sgr_ten() {
        let term = entry(
            b"\x1b[0;10m",
            b"\x1b[0;10%?%p9%t;11%;m",
            b"\x1b[11m",
            b"\x1b[10m",
        )
        .build();
        assert_eq!(termcap(&term, "me"), Some(b"\x1b[0m".to_vec()));
    }

    #[test]
    fn linux_reset_keeps_distinct_sequence() {
        let term = entry(
            b"\x1b[m\x0f",
            b"\x1b[0;10m%?%p9%t\x0e%e\x0f%;",
            b"\x0e",
            b"\x0f",
        )
        .build();
        assert_eq!(termcap(&term, "me"), Some(b"\x1b[m\x0f".to_vec()));
    }

    #[test]
    fn ordinary_strings_map_directly() {
        let term = entry(b"reset", b"attributes", b"in", b"out")
            .string("bel", b"bell")
            .build();
        assert_eq!(termcap(&term, "bl"), Some(b"bell".to_vec()));
        assert_eq!(termcap(&term, "zz"), None);
    }

    #[test]
    fn removes_decimal_padding_forms() {
        assert_eq!(without_padding(b"a$<2.5*/>b"), b"ab");
    }
}
