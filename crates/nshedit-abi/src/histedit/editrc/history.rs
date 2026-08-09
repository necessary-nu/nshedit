//! History forms in the editrc command language.

use core::ffi::{c_int, c_void};

use crate::adapter::EditLine;
use crate::cdecl::histedit::{H_LAST, H_PREV, H_SETSIZE, H_SETUNIQUE, HistEvent, HistEventWide};
use crate::conversion::{ConversionBuffer, decode_bytes, encode_wide};

use super::super::{cbytes, wstr};

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

unsafe fn history_text(el: *mut EditLine, operation: c_int) -> Option<Vec<u32>> {
    let (callback, cookie) = unsafe { (&*el).history_callback() }?;
    if cookie.is_null() {
        return None;
    }
    if unsafe { (&*el).narrow_history() } {
        let mut event = HistEvent {
            num: 0,
            str: core::ptr::null(),
        };
        if unsafe {
            callback(
                cookie,
                (&raw mut event).cast(),
                operation,
                core::ptr::null_mut::<c_void>(),
            )
        } == -1
        {
            return None;
        }
        let bytes = unsafe { cbytes(event.str) }?;
        let mut conversion = ConversionBuffer::new();
        Some(decode_bytes(Some(bytes), &mut conversion)?.to_vec())
    } else {
        let mut event = HistEventWide {
            num: 0,
            str: core::ptr::null(),
        };
        if unsafe {
            callback(
                cookie,
                &raw mut event,
                operation,
                core::ptr::null_mut::<c_void>(),
            )
        } == -1
        {
            return None;
        }
        Some(unsafe { wstr(event.str) }?.to_vec())
    }
}

unsafe fn list(el: *mut EditLine) -> c_int {
    let mut operation = H_LAST;
    let mut number = 1;
    let mut conversion = ConversionBuffer::new();
    while let Some(wide) = unsafe { history_text(el, operation) } {
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
        operation = H_PREV;
    }
    0
}

unsafe fn set_policy(el: *mut EditLine, operation: c_int, value: c_int) -> c_int {
    let Some((callback, cookie)) = (unsafe { (&*el).history_callback() }) else {
        return -1;
    };
    if cookie.is_null() || unsafe { (&*el).narrow_history() } {
        return -1;
    }
    if callback as *const () != crate::histedit::history_w as *const () {
        return -1;
    }
    let mut event = HistEventWide {
        num: 0,
        str: core::ptr::null(),
    };
    unsafe { callback(cookie, &raw mut event, operation, value) }
}

// [spec:nshedit:req:abi.history-effects+1]
pub(super) unsafe fn history_command(el: *mut EditLine, words: &[&[u32]]) -> c_int {
    if unsafe { (&*el).history_callback() }.is_none_or(|(_, cookie)| cookie.is_null()) {
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
        unsafe { set_policy(el, H_SETSIZE, value) }
    } else if word_is(words[1], "unique") {
        unsafe { set_policy(el, H_SETUNIQUE, value) }
    } else {
        -1
    }
}
