//! Typed host-effect execution for the native read driver.

use super::*;
use std::ffi::{CString, OsString};
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, Write};
use std::os::unix::ffi::OsStringExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use nshedit::domain::{KeymapMode, RepeatCount};
use nshedit::editor::effect::{
    AliasEffect, AliasResponse, EditorCommandEffect, EditorCommandResponse, ExternalEditEffect,
    HistoryLineEffect, HistoryMatch, HistoryNavigateEffect, HistoryPosition, HistorySearchEffect,
    HistorySearchInput, HistorySearchResponse, HistoryWordEffect, HistoryWordPosition,
    HistoryWordResponse,
};

mod signal;

use signal::{DirectReadOutcome, ReadSignals, native_signal};

/// A safe writer over the caller-owned output stream for one driver step.
struct CompatibilityOutput {
    stream: CFile,
}

impl CompatibilityOutput {
    const fn new(stream: CFile) -> Self {
        Self { stream }
    }
}

impl Write for CompatibilityOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        crate::cstdio::write(self.stream, buffer)?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        crate::cstdio::flush(self.stream)
    }
}

fn terminal_bytes(units: &[TextUnit]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in units {
        match unit {
            TextUnit::Scalar(character) => {
                let mut encoded = [0; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            TextUnit::RawByte(byte) => bytes.push(*byte),
            TextUnit::OpaqueCodePoint(value) => {
                bytes.extend_from_slice("\u{fffd}".as_bytes());
                let _ = value;
            }
        }
    }
    bytes
}

pub(super) fn text_from_bytes(bytes: &[u8]) -> Text {
    let mut text = Text::default();
    let mut remaining = bytes;
    while !remaining.is_empty() {
        match core::str::from_utf8(remaining) {
            Ok(valid) => {
                text.extend(valid.chars().map(TextUnit::Scalar));
                break;
            }
            Err(error) => {
                let valid = &remaining[..error.valid_up_to()];
                text.extend(
                    core::str::from_utf8(valid)
                        .expect("valid_up_to identifies valid UTF-8")
                        .chars()
                        .map(TextUnit::Scalar),
                );
                remaining = &remaining[error.valid_up_to()..];
                let invalid = error.error_len().unwrap_or(remaining.len());
                text.extend(remaining[..invalid].iter().copied().map(TextUnit::RawByte));
                remaining = &remaining[invalid..];
            }
        }
    }
    text
}

fn prompt_from_units(units: &[TextUnit], escape: u32) -> Prompt {
    if escape == 0 {
        return Prompt::from(units.iter().copied().collect::<Text>());
    }
    let marker = TextUnit::from_code_point(escape);
    let mut prompt = Prompt::default();
    let mut literal = false;
    for part in units.split(|unit| *unit == marker) {
        if literal {
            prompt.push_literal(TerminalLiteral::from(terminal_bytes(part)));
        } else {
            prompt.push_text(part.iter().copied().collect::<Text>());
        }
        literal = !literal;
    }
    prompt
}

/// Invoke a prompt callback with no live Rust borrow of its editor.
unsafe fn host_prompt(el: *mut EditLine, side: PromptSide) -> Prompt {
    let right = side == PromptSide::Right;
    let (callback, escape) = unsafe { (&*el).prompt_callback(right) };
    let units: Vec<TextUnit> = match callback {
        PromptCallback::Wide(callback) => {
            let pointer = unsafe { callback(el) };
            unsafe { wstr(pointer) }
                .unwrap_or(&[])
                .iter()
                .copied()
                .map(TextUnit::from_code_point)
                .collect()
        }
        PromptCallback::Narrow(callback) => {
            let pointer = unsafe { callback(el) };
            text_from_bytes(unsafe { cbytes(pointer) }.unwrap_or(&[]))
                .as_units()
                .to_vec()
        }
    };
    prompt_from_units(&units, escape)
}

/// Obtain one owned input response without retaining an editor borrow while
/// an application callback runs.
unsafe fn host_read(
    el: *mut EditLine,
    signals: &mut ReadSignals,
) -> Result<ReadOutcome, HostFailure> {
    if let Some(signal) = unsafe { signals.take_pending(el) } {
        return Ok(ReadOutcome::Signal(native_signal(signal)));
    }
    if let Some(unit) = unsafe { (&mut *el).pop_input() } {
        let _ = unsafe { signals.resume_pending_direct(el) }?;
        return Ok(ReadOutcome::Unit(unit));
    }
    if let Some(callback) = unsafe { (&*el).read_callback() } {
        let mut value = 0;
        let result = unsafe { callback(el, &raw mut value) };
        let _ = unsafe { signals.resume_pending_direct(el) }?;
        return match result {
            result if result > 0 => Ok(ReadOutcome::Unit(TextUnit::from_code_point(value))),
            0 => Ok(ReadOutcome::EndOfInput),
            _ => Err(HostFailure::Failed("input callback failed".into())),
        };
    }

    if unsafe { (&*el).descriptor(0) }.is_none_or(|descriptor| descriptor < 0) {
        let _ = unsafe { signals.resume_pending_direct(el) }?;
        return Ok(ReadOutcome::EndOfInput);
    }

    let mut bytes = [0; 64];
    loop {
        if let Some(signal) = unsafe { signals.take_pending(el) } {
            return Ok(ReadOutcome::Signal(native_signal(signal)));
        }
        match unsafe { (&*el).read_input(&mut bytes) } {
            Ok(0) => {
                let _ = unsafe { signals.resume_pending_direct(el) }?;
                return Ok(ReadOutcome::EndOfInput);
            }
            Ok(length) => {
                let _ = unsafe { signals.resume_pending_direct(el) }?;
                return Ok(ReadOutcome::Bytes(bytes[..length].into()));
            }
            Err(error) => {
                if let Some(signal) = unsafe { signals.take_pending(el) } {
                    return Ok(ReadOutcome::Signal(native_signal(signal)));
                }
                if error.kind() == std::io::ErrorKind::Interrupted && unsafe { (&*el).safe_read() }
                {
                    continue;
                }
                return Err(HostFailure::Failed(error.to_string().into_boxed_str()));
            }
        }
    }
}

/// Read exactly one wide input unit for the direct `el_wgetc` ABI.
///
/// This path deliberately does not share the line driver's chunk decoder:
/// `el_wgetc` must never consume bytes belonging to the next character, and
/// pushed input must bypass terminal activation entirely.
pub(super) unsafe fn read_wide_character(el: *mut EditLine, wc: *mut WcharT) -> c_int {
    let _ = unsafe { (&*el).flush_output() };
    let mut signals = ReadSignals::empty();

    if let Some(unit) = unsafe { (&mut *el).pop_input() } {
        unsafe { *wc = crate::adapter::unit_to_wide(unit) };
        let _ = unsafe { signals.resume_pending_direct(el) };
        return 1;
    }

    if !unsafe { (&*el).is_tty() }
        || unsafe { (&mut *el).set_terminal_mode(nshedit::domain::TerminalMode::Editing) }.is_err()
    {
        return 0;
    }

    if let Some(callback) = unsafe { (&*el).read_callback() } {
        let result = unsafe { callback(el, wc) };
        let _ = unsafe { signals.resume_pending_direct(el) };
        return result;
    }

    let mut pending = [0; 6];
    let mut used = 0usize;
    loop {
        match crate::conversion::decode_prefix(&pending[..used]) {
            crate::conversion::PrefixDecode::Complete(value) => {
                unsafe { *wc = value };
                if unsafe { signals.resume_pending_direct(el) }.is_err() {
                    unsafe { *wc = 0 };
                    return -1;
                }
                return 1;
            }
            crate::conversion::PrefixDecode::Incomplete => {}
            crate::conversion::PrefixDecode::Invalid if used <= 1 => {
                used = 0;
            }
            crate::conversion::PrefixDecode::Invalid => {
                pending[0] = pending[used - 1];
                used = 1;
                continue;
            }
        }

        if used == crate::conversion::max_multibyte_length() {
            crate::errno::set(EILSEQ);
            unsafe { *wc = 0 };
            return -1;
        }

        let next = &mut pending[used..=used];
        if unsafe { signals.resume_pending_direct(el) }.is_err() {
            unsafe { *wc = 0 };
            return -1;
        }
        match unsafe { (&*el).read_input(next) } {
            Ok(0) => {
                if unsafe { signals.resume_pending_direct(el) }.is_err() {
                    unsafe { *wc = 0 };
                    return -1;
                }
                unsafe { *wc = 0 };
                return 0;
            }
            Ok(_) => {
                used += 1;
                if unsafe { signals.resume_pending_direct(el) }.is_err() {
                    unsafe { *wc = 0 };
                    return -1;
                }
            }
            Err(error) => {
                if let Some(signal) = unsafe { signals.take_pending(el) } {
                    match unsafe { signals.resume_direct(el, signal) } {
                        Ok(DirectReadOutcome::Resume) => continue,
                        Ok(DirectReadOutcome::Interrupt) | Err(_) => {
                            unsafe { *wc = 0 };
                            return -1;
                        }
                    }
                }
                if error.kind() == std::io::ErrorKind::Interrupted && unsafe { (&*el).safe_read() }
                {
                    continue;
                }
                unsafe { *wc = 0 };
                return -1;
            }
        }
    }
}

enum HistoryFetch {
    Entry(Text),
    Missing {
        last_depth: usize,
        last_entry: Option<Text>,
    },
}

struct HistoryItem {
    number: c_int,
    line: Text,
}

unsafe fn history_item(el: *mut EditLine, operation: c_int) -> Option<HistoryItem> {
    let (callback, cookie) = unsafe { (&*el).history_callback() }?;
    if cookie.is_null() {
        return None;
    }
    let (number, mut line): (c_int, Text) = if unsafe { (&*el).narrow_history() } {
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
        let mut conversion = crate::conversion::ConversionBuffer::new();
        let line = crate::conversion::decode_bytes(Some(bytes), &mut conversion)?
            .iter()
            .copied()
            .map(TextUnit::from_code_point)
            .collect();
        (event.num, line)
    } else {
        let mut event = HistEventW {
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
        let line = unsafe { wstr(event.str) }?
            .iter()
            .copied()
            .map(TextUnit::from_code_point)
            .collect();
        (event.num, line)
    };

    let mut end = line.len();
    if line.as_units().get(end.wrapping_sub(1)) == Some(&TextUnit::Scalar('\n')) {
        end -= 1;
    }
    if line.as_units().get(end.wrapping_sub(1)) == Some(&TextUnit::Scalar(' ')) {
        end -= 1;
    }
    if end != line.len() {
        line = line.as_units()[..end].iter().copied().collect();
    }
    Some(HistoryItem { number, line })
}

unsafe fn fetch_history(el: *mut EditLine, depth: usize) -> HistoryFetch {
    debug_assert!(depth > 0);
    let Some(first) = (unsafe { history_item(el, H_FIRST) }) else {
        return HistoryFetch::Missing {
            last_depth: 0,
            last_entry: None,
        };
    };
    let mut line = first.line;
    let mut reached = 1;
    while reached < depth {
        let Some(next) = (unsafe { history_item(el, H_NEXT) }) else {
            return HistoryFetch::Missing {
                last_depth: reached,
                last_entry: Some(line),
            };
        };
        line = next.line;
        reached += 1;
    }
    HistoryFetch::Entry(line)
}

unsafe fn host_history(
    el: *mut EditLine,
    request: &HistoryNavigateEffect,
) -> Result<HistoryResponse, HostFailure> {
    if unsafe { (&*el).history_callback() }.is_none_or(|(_, cookie)| cookie.is_null()) {
        return Err(HostFailure::Unavailable);
    }
    let depth = unsafe { (&*el).history_depth() };
    let count = request.count.get();
    match request.direction {
        Direction::Previous => {
            if depth == 0 {
                unsafe { (&mut *el).save_history_live_line() };
            }
            let requested = depth.saturating_add(count);
            match unsafe { fetch_history(el, requested) } {
                HistoryFetch::Entry(line) => {
                    unsafe { (&mut *el).set_history_depth(requested) };
                    Ok(HistoryResponse::entry(line))
                }
                HistoryFetch::Missing {
                    last_depth,
                    last_entry,
                } => {
                    let selected = if unsafe { (&*el).editor_is_vi() } {
                        depth
                    } else if last_depth == 0 {
                        requested
                    } else {
                        last_depth
                    };
                    unsafe { (&mut *el).set_history_depth(selected) };
                    let retried = if selected == 0 {
                        Some(unsafe { (&*el).history_live_line().clone() })
                    } else {
                        match unsafe { fetch_history(el, selected) } {
                            HistoryFetch::Entry(line) => Some(line),
                            HistoryFetch::Missing { last_entry, .. } => last_entry,
                        }
                    }
                    .or(last_entry);
                    Ok(retried.map_or_else(HistoryResponse::boundary, |line| {
                        HistoryResponse::entry(line).at_boundary()
                    }))
                }
            }
        }
        Direction::Next if depth == 0 => {
            Ok(HistoryResponse::entry(unsafe { (&*el).history_live_line().clone() }).at_boundary())
        }
        Direction::Next if count >= depth => {
            unsafe { (&mut *el).set_history_depth(0) };
            let response = HistoryResponse::entry(unsafe { (&*el).history_live_line().clone() });
            Ok(if count > depth {
                response.at_boundary()
            } else {
                response
            })
        }
        Direction::Next => {
            let requested = depth - count;
            match unsafe { fetch_history(el, requested) } {
                HistoryFetch::Entry(line) => {
                    unsafe { (&mut *el).set_history_depth(requested) };
                    Ok(HistoryResponse::entry(line))
                }
                HistoryFetch::Missing { last_depth, .. } => {
                    if last_depth != 0 {
                        unsafe { (&mut *el).set_history_depth(last_depth) };
                    }
                    Ok(HistoryResponse::boundary())
                }
            }
        }
    }
}

const MAX_HOST_TEXT: usize = 4096;
const MAX_HISTORY_SCAN: usize = 4096;
static NEXT_EDIT_FILE: AtomicU64 = AtomicU64::new(0);

unsafe fn read_host_text(
    el: *mut EditLine,
    prompt: &Text,
    cancel_on_escape: bool,
) -> Result<Text, HostFailure> {
    unsafe { (&*el).write_compatibility_stream(1, &terminal_bytes(prompt.as_units())) };
    let mut text = Text::default();
    loop {
        let mut value = 0;
        match unsafe { read_wide_character(el, &raw mut value) } {
            1 => {}
            0 => return Err(HostFailure::Cancelled),
            _ => return Err(HostFailure::Failed("command input failed".into())),
        }
        let unit = TextUnit::from_code_point(value);
        match unit {
            TextUnit::Scalar('\r' | '\n') => {
                unsafe { (&*el).write_compatibility_stream(1, b"\n") };
                return Ok(text);
            }
            TextUnit::Scalar('\u{1b}') if cancel_on_escape => {
                return Err(HostFailure::Cancelled);
            }
            TextUnit::Scalar('\u{7}') => return Err(HostFailure::Cancelled),
            TextUnit::Scalar('\u{8}' | '\u{7f}') => {
                if !text.is_empty() {
                    let end = text.len();
                    let span = text
                        .span(end - 1..end)
                        .expect("the final unit is inside the owned command text");
                    let _removed = text
                        .remove(span)
                        .expect("the checked final-unit span remains valid");
                    unsafe { (&*el).write_compatibility_stream(1, b"\x08 \x08") };
                }
            }
            unit => {
                if text.len() == MAX_HOST_TEXT {
                    return Err(HostFailure::Failed("command input is too long".into()));
                }
                text.push(unit);
                unsafe { (&*el).write_compatibility_stream(1, &terminal_bytes(&[unit])) };
            }
        }
    }
}

unsafe fn history_items(el: *mut EditLine) -> Result<Vec<HistoryItem>, HostFailure> {
    if unsafe { (&*el).history_callback() }.is_none_or(|(_, cookie)| cookie.is_null()) {
        return Err(HostFailure::Unavailable);
    }
    let mut items = Vec::new();
    let mut operation = H_FIRST;
    while let Some(item) = unsafe { history_item(el, operation) } {
        if items.len() == MAX_HISTORY_SCAN {
            return Err(HostFailure::Failed("history scan limit exceeded".into()));
        }
        items.push(item);
        operation = H_NEXT;
    }
    Ok(items)
}

unsafe fn host_history_search(
    el: *mut EditLine,
    request: &HistorySearchEffect,
) -> Result<HistorySearchResponse, HostFailure> {
    let pattern = match &request.input {
        HistorySearchInput::Pattern(pattern) => pattern.clone(),
        HistorySearchInput::Prompted => {
            let prompt = match request.direction {
                Direction::Previous => Text::from("\n/"),
                Direction::Next => Text::from("\n?"),
            };
            unsafe { read_host_text(el, &prompt, true) }?
        }
        HistorySearchInput::Incremental(KeymapMode::ViCommand) => {
            let prompt = match request.direction {
                Direction::Previous => Text::from("\nbck: "),
                Direction::Next => Text::from("\nfwd: "),
            };
            unsafe { (&*el).write_compatibility_stream(1, &terminal_bytes(prompt.as_units())) };
            let mut value = 0;
            match unsafe { read_wide_character(el, &raw mut value) } {
                1 if unsafe { (&mut *el).push_input(&[value]) } => {
                    return Ok(HistorySearchResponse {
                        history: HistoryResponse::unchanged(),
                        pattern: Text::default(),
                    });
                }
                1 => return Err(HostFailure::Failed("input pushback is full".into())),
                0 => return Err(HostFailure::Cancelled),
                _ => {
                    return Err(HostFailure::Failed(
                        "incremental search input failed".into(),
                    ));
                }
            }
        }
        HistorySearchInput::Incremental(_) => {
            let prompt = match request.direction {
                Direction::Previous => Text::from("\nbck: "),
                Direction::Next => Text::from("\nfwd: "),
            };
            unsafe { read_host_text(el, &prompt, true) }?
        }
    };
    let items = unsafe { history_items(el) }?;
    let depth = unsafe { (&*el).history_depth() };
    if depth == 0 {
        unsafe { (&mut *el).save_history_live_line() };
    }

    let found = match request.direction {
        Direction::Previous => items
            .iter()
            .enumerate()
            .skip(depth)
            .find(|(_, item)| history_matches(&item.line, &pattern, request.matching)),
        Direction::Next => items
            .iter()
            .enumerate()
            .take(depth.saturating_sub(1))
            .rev()
            .find(|(_, item)| history_matches(&item.line, &pattern, request.matching)),
    };
    let history = if let Some((index, item)) = found {
        unsafe { (&mut *el).set_history_depth(index + 1) };
        HistoryResponse::entry(item.line.clone())
    } else if request.direction == Direction::Next
        && depth != 0
        && history_matches(
            unsafe { (&*el).history_live_line() },
            &pattern,
            request.matching,
        )
    {
        unsafe { (&mut *el).set_history_depth(0) };
        HistoryResponse::live()
    } else {
        HistoryResponse::boundary()
    };
    Ok(HistorySearchResponse { history, pattern })
}

fn history_matches(line: &Text, pattern: &Text, matching: HistoryMatch) -> bool {
    let line = line.as_units();
    let pattern = pattern.as_units();
    if pattern.is_empty() {
        return true;
    }
    match matching {
        HistoryMatch::Prefix => line.starts_with(pattern) && line != pattern,
        HistoryMatch::Contains => line.windows(pattern.len()).any(|window| window == pattern),
    }
}

unsafe fn host_history_line(
    el: *mut EditLine,
    request: &HistoryLineEffect,
) -> Result<HistoryResponse, HostFailure> {
    if request.position() == HistoryPosition::Current {
        unsafe { (&mut *el).set_history_depth(0) };
        return Ok(HistoryResponse::entry(
            unsafe { (&*el).history_live_line() }.clone(),
        ));
    }
    let items = unsafe { history_items(el) }?;
    let original_depth = unsafe { (&*el).history_depth() };
    if original_depth == 0 {
        unsafe { (&mut *el).save_history_live_line() };
    }
    if let HistoryPosition::Number(number) = request.position()
        && unsafe { (&*el).narrow_history() }
    {
        if number == RepeatCount::ONE {
            unsafe { (&mut *el).set_history_depth(0) };
            return Ok(HistoryResponse::live());
        }
        unsafe { (&mut *el).set_history_depth(original_depth) };
        return Ok(items
            .first()
            .map_or_else(HistoryResponse::boundary, |item| {
                HistoryResponse::entry(item.line.clone()).at_boundary()
            }));
    }
    let selected = match request.position() {
        HistoryPosition::Current => unreachable!("handled before scanning retained history"),
        HistoryPosition::Oldest => items.iter().enumerate().next_back(),
        HistoryPosition::Number(number) => items
            .iter()
            .enumerate()
            .find(|(_, item)| usize::try_from(item.number).ok() == Some(number.get())),
    };
    let Some((index, item)) = selected else {
        return Ok(HistoryResponse::boundary());
    };
    unsafe { (&mut *el).set_history_depth(index + 1) };
    Ok(HistoryResponse::entry(item.line.clone()))
}

unsafe fn host_history_word(
    el: *mut EditLine,
    request: &HistoryWordEffect,
) -> Result<HistoryWordResponse, HostFailure> {
    if unsafe { (&*el).history_callback() }.is_none_or(|(_, cookie)| cookie.is_null()) {
        return Err(HostFailure::Unavailable);
    }
    let Some(item) = (unsafe { history_item(el, H_FIRST) }) else {
        return Ok(HistoryWordResponse::Missing);
    };
    let words: Vec<&[TextUnit]> = item
        .line
        .as_units()
        .split(is_history_space)
        .filter(|word| !word.is_empty())
        .collect();
    let selected = match request.position {
        HistoryWordPosition::Last => words.last().copied(),
        HistoryWordPosition::Number(number) => words.get(number.get() - 1).copied(),
    };
    Ok(selected.map_or(HistoryWordResponse::Missing, |word| {
        HistoryWordResponse::Word(word.iter().copied().collect())
    }))
}

fn is_history_space(unit: &TextUnit) -> bool {
    match unit {
        TextUnit::Scalar(character) => character.is_whitespace(),
        TextUnit::RawByte(byte) => byte.is_ascii_whitespace(),
        TextUnit::OpaqueCodePoint(_) => false,
    }
}

unsafe fn host_alias(
    el: *mut EditLine,
    request: &AliasEffect,
) -> Result<AliasResponse, HostFailure> {
    let Some((callback, cookie)) = (unsafe { (&*el).alias_callback() }) else {
        return Err(HostFailure::Unavailable);
    };
    let name = CString::new(terminal_bytes(request.name.as_units()))
        .map_err(|_| HostFailure::Failed("alias name contains a null byte".into()))?;
    let expansion = unsafe { callback(cookie, name.as_ptr()) };
    let Some(bytes) = (unsafe { cbytes(expansion) }) else {
        return Ok(AliasResponse::Missing);
    };
    Ok(AliasResponse::Expansion(text_from_bytes(bytes)))
}

unsafe fn host_editor_command(
    el: *mut EditLine,
    request: &EditorCommandEffect,
) -> Result<EditorCommandResponse, HostFailure> {
    let command = unsafe { read_host_text(el, &request.prompt, true) }?;
    Ok(
        if unsafe { parse_editrc_line(el, command.as_units()) } == 0 {
            EditorCommandResponse::Applied
        } else {
            EditorCommandResponse::Rejected
        },
    )
}

unsafe fn host_external_edit(
    el: *mut EditLine,
    request: &ExternalEditEffect,
) -> Result<Text, HostFailure> {
    let serial = NEXT_EDIT_FILE.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("nshedit-{}-{serial}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()))?;
    let result = (|| {
        file.write_all(&terminal_bytes(request.line.as_units()))
            .and_then(|()| file.write_all(b"\n"))
            .and_then(|()| file.flush())
            .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()))?;
        let editor = unsafe { environment_value(el, "EDITOR") }
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| b"vi".to_vec());
        let executable = OsString::from_vec(editor);
        Command::new(executable)
            .arg(&path)
            .status()
            .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()))?;
        file.rewind()
            .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()))?;
        let mut edited = Vec::new();
        file.read_to_end(&mut edited)
            .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()))?;
        if edited.last() == Some(&b'\n') {
            edited.pop();
        }
        Ok(text_from_bytes(&edited))
    })();
    drop(file);
    let _cleanup = std::fs::remove_file(path);
    result
}

unsafe fn host_command(
    el: *mut EditLine,
    name: &nshedit::domain::CommandName,
    invoking: nshedit::domain::TextUnit,
) -> Result<Outcome, HostFailure> {
    let Some(callback) = (unsafe { (&*el).command_callback(name) }) else {
        return Err(HostFailure::Unavailable);
    };
    let result = unsafe { callback(el, crate::adapter::unit_to_wide(invoking)) };
    Ok(match result {
        CC_NEWLINE => Outcome::Accepted(unsafe { (&*el).native().line().clone() }),
        CC_EOF => Outcome::EndOfInput,
        crate::cdecl::histedit::CC_REFRESH => Outcome::Refresh(Refresh::Redraw),
        CC_REDISPLAY => Outcome::Refresh(Refresh::Redisplay),
        CC_REFRESH_BEEP => Outcome::Refresh(Refresh::Beep),
        0 => Outcome::Continue,
        _ => Outcome::Refresh(Refresh::Beep),
    })
}

pub(super) unsafe fn drive_read(el: *mut EditLine) -> Result<ReadResult, ()> {
    let mut signals = unsafe { ReadSignals::edited(el) }?;

    let mut step = {
        let (editor, driver) = unsafe { (&mut *el).split_driver() };
        driver.begin(editor).map_err(|_| ())?
    };
    loop {
        step = match step {
            ReadStep::Prompt(pending) => {
                let prompt = unsafe { host_prompt(el, pending.request().side) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_prompt(editor, &pending, Ok(prompt))
                    .map_err(|_| ())?
            }
            ReadStep::Resize(pending) => {
                let response = unsafe { signals.resize(el, *pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_resize(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::Read(pending) => {
                let response = unsafe { host_read(el, &mut signals) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_read(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::History(pending) => {
                let response = unsafe { host_history(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::HistorySearch(pending) => {
                let response = unsafe { host_history_search(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history_search(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::HistoryLine(pending) => {
                let response = unsafe { host_history_line(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history_line(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::HistoryWord(pending) => {
                let response = unsafe { host_history_word(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history_word(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::Alias(pending) => {
                let response = unsafe { host_alias(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_alias(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::EditorCommand(pending) => {
                let response = unsafe { host_editor_command(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_editor_command(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::ExternalEdit(pending) => {
                let response = unsafe { host_external_edit(el, pending.request()) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_external_edit(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::RecordHistory(pending) => {
                // libedit returns an accepted line to the application and
                // leaves H_ENTER to that caller. The native effect is still
                // resumed explicitly so the core retains one typed protocol.
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history_record(editor, &pending, Ok(()))
                    .map_err(|_| ())?
            }
            ReadStep::Completion(pending) => {
                let response = Ok(crate::filecomplete::builtin_candidates(
                    &pending.request().query,
                ));
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_completion(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::UserCommand(pending) => {
                let request = pending.request();
                let response = unsafe { host_command(el, &request.name, request.invoking) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_user_command(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::Signal(pending) => {
                let response = unsafe { signals.propagate(el, pending.request().signal) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_signal(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::Display(display) => {
                let stream = unsafe { (&*el).stream(1) }.unwrap_or(core::ptr::null_mut());
                let mut output = CompatibilityOutput::new(stream);
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .display(editor, &display, &mut output)
                    .map_err(|_| ())?
            }
            ReadStep::Complete(result) => {
                unsafe { signals.finish(el) }.map_err(|_| ())?;
                return Ok(result);
            }
        };
    }
}

pub(super) unsafe fn read_unedited(el: *mut EditLine) -> Result<bool, ()> {
    let mut signals = unsafe { ReadSignals::unedited(el) }?;
    let callback = unsafe { (&*el).read_callback() };
    let unbuffered = unsafe { (&*el).unbuffered() };
    let mut line = Text::default();
    let mut bytes = Vec::new();
    loop {
        if let Some(callback) = callback {
            let _ = unsafe { signals.resume_pending_direct(el) }.map_err(|_| ())?;
            let mut value = 0;
            let result = unsafe { callback(el, &raw mut value) };
            let delivery = unsafe { signals.resume_pending_direct(el) }.map_err(|_| ())?;
            match result {
                result if result > 0 => {
                    let unit = TextUnit::from_code_point(value);
                    line.push(unit);
                    if unbuffered || matches!(unit, TextUnit::Scalar('\r' | '\n')) {
                        break;
                    }
                }
                0 => break,
                _ => {
                    if delivery.is_some() {
                        unsafe { (&mut *el).reset_line() };
                    }
                    return Err(());
                }
            }
        } else {
            let mut byte = [0];
            let _ = unsafe { signals.resume_pending_direct(el) }.map_err(|_| ())?;
            match unsafe { (&*el).read_input(&mut byte) } {
                Ok(1) => {
                    let _ = unsafe { signals.resume_pending_direct(el) }.map_err(|_| ())?;
                    bytes.push(byte[0]);
                    if unbuffered || matches!(byte[0], b'\r' | b'\n') {
                        break;
                    }
                }
                Ok(0) => {
                    let _ = unsafe { signals.resume_pending_direct(el) }.map_err(|_| ())?;
                    break;
                }
                Ok(_) => unreachable!("the one-byte buffer cannot read more than one byte"),
                Err(error) => {
                    if let Some(signal) = unsafe { signals.take_pending(el) } {
                        if unsafe { signals.resume_direct(el, signal) }.map_err(|_| ())?
                            == DirectReadOutcome::Resume
                        {
                            continue;
                        }
                        unsafe { (&mut *el).reset_line() };
                        return Err(());
                    }
                    if error.kind() == std::io::ErrorKind::Interrupted
                        && unsafe { (&*el).safe_read() }
                    {
                        continue;
                    }
                    return Err(());
                }
            }
        }
    }
    unsafe { signals.finish(el) }.map_err(|_| ())?;
    if callback.is_none() {
        line = text_from_bytes(&bytes);
    }
    let has_line = !line.is_empty();
    unsafe { (&mut *el).replace_line(line) }
        .then_some(())
        .ok_or(())?;
    Ok(has_line)
}

#[cfg(test)]
mod command_effect_tests;
