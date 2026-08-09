//! History forms in the editrc command language.

use core::ffi::c_int;

use crate::adapter::{EditLine, HistoryPolicy};
use crate::conversion::{ConversionBuffer, encode_wide};
use crate::history::HistoryMove;

fn word_is(word: &[u32], expected: &str) -> bool {
    word.iter().copied().eq(expected.bytes().map(u32::from))
}

fn parse_number(word: &[u32]) -> c_int {
    let mut index = word
        .iter()
        .position(|value| char::from_u32(*value).is_none_or(|ch| !ch.is_whitespace()))
        .unwrap_or(word.len());
    let negative = match word.get(index).copied() {
        Some(value) if value == u32::from(b'-') => {
            index += 1;
            true
        }
        Some(value) if value == u32::from(b'+') => {
            index += 1;
            false
        }
        _ => false,
    };
    let mut base = 10u32;
    if word.get(index) == Some(&u32::from(b'0')) {
        base = 8;
        if matches!(word.get(index + 1), Some(value) if *value == u32::from(b'x') || *value == u32::from(b'X'))
            && word
                .get(index + 2)
                .and_then(|value| char::from_u32(*value))
                .and_then(|value| value.to_digit(16))
                .is_some()
        {
            base = 16;
            index += 2;
        }
    }

    let mut value = 0u128;
    let mut any = false;
    while let Some(digit) = word
        .get(index)
        .and_then(|value| char::from_u32(*value))
        .and_then(|value| value.to_digit(base))
    {
        any = true;
        value = value
            .saturating_mul(u128::from(base))
            .saturating_add(u128::from(digit));
        index += 1;
    }
    if !any {
        return 0;
    }
    let signed = if negative {
        -(value.min(i64::MAX as u128 + 1) as i128)
    } else {
        value.min(i64::MAX as u128) as i128
    };
    (signed as i64) as c_int
}

unsafe fn history_text(el: *mut EditLine, movement: HistoryMove) -> Option<Vec<u32>> {
    let source = unsafe { (&*el).history_source() }?;
    // SAFETY: the editor stores the callback together with its live cookie.
    Some(unsafe { source.item(movement) }?.into_boundary_text())
}

unsafe fn list(el: *mut EditLine) -> c_int {
    let mut movement = HistoryMove::Oldest;
    let mut number = 1;
    let mut conversion = ConversionBuffer::new();
    while let Some(wide) = unsafe { history_text(el, movement) } {
        let Some(encoded) = encode_wide(Some(&wide), &mut conversion) else {
            return -1;
        };
        let mut encoded = encoded.to_vec();
        if encoded.last() == Some(&b'\n') {
            encoded.pop();
        }
        let visible = bsd::vis::Encoder::new(bsd::vis::Flags::NL).encode(&encoded);
        let mut output = format!("{number}\t").into_bytes();
        output.extend_from_slice(&visible);
        output.push(b'\n');
        unsafe { (&*el).write_compatibility_stream(1, &output) };
        number += 1;
        movement = HistoryMove::Newer;
    }
    0
}

unsafe fn set_policy(el: *mut EditLine, policy: HistoryPolicy) -> c_int {
    let Some(source) = (unsafe { (&*el).history_source() }) else {
        return -1;
    };
    c_int::from(!unsafe { source.set_policy(policy) }).wrapping_neg()
}

// [spec:nshedit:req:abi.history-effects+1]
pub(super) unsafe fn history_command(el: *mut EditLine, words: &[&[u32]]) -> c_int {
    if unsafe { (&*el).history_source() }.is_none_or(|source| !source.is_available()) {
        return -1;
    }
    if words.len() == 1 || words.get(1).is_some_and(|word| word_is(word, "list")) {
        return unsafe { list(el) };
    }
    if words.len() != 3 {
        return -1;
    }
    let value = parse_number(words[2]);
    if word_is(words[1], "size") {
        unsafe { set_policy(el, HistoryPolicy::Limit(value)) }
    } else if word_is(words[1], "unique") {
        unsafe { set_policy(el, HistoryPolicy::Unique(value != 0)) }
    } else {
        -1
    }
}
