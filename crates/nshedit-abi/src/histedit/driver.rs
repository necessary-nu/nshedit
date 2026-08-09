//! Typed host-effect execution for the native read driver.

use super::*;
use std::io::{self, Write};

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
            TextUnit::CompatibilityWide(value) => {
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
    let marker = TextUnit::from_wide(escape);
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
                .map(TextUnit::from_wide)
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
unsafe fn host_read(el: *mut EditLine) -> Result<ReadOutcome, HostFailure> {
    if let Some(unit) = unsafe { (&mut *el).pop_input() } {
        return Ok(ReadOutcome::Unit(unit));
    }
    if let Some(callback) = unsafe { (&*el).read_callback() } {
        let mut value = 0;
        return match unsafe { callback(el, &raw mut value) } {
            result if result > 0 => Ok(ReadOutcome::Unit(TextUnit::from_wide(value))),
            0 => Ok(ReadOutcome::EndOfInput),
            _ => Err(HostFailure::Failed("input callback failed".into())),
        };
    }

    if unsafe { (&*el).descriptor(0) }.is_none_or(|descriptor| descriptor < 0) {
        return Ok(ReadOutcome::EndOfInput);
    }

    let mut bytes = [0; 64];
    loop {
        match unsafe { (&*el).read_input(&mut bytes) } {
            Ok(0) => return Ok(ReadOutcome::EndOfInput),
            Ok(length) => return Ok(ReadOutcome::Bytes(bytes[..length].into())),
            Err(error)
                if error.kind() == std::io::ErrorKind::Interrupted
                    && unsafe { (&*el).safe_read() } => {}
            Err(error) => return Err(HostFailure::Failed(error.to_string().into_boxed_str())),
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

    if let Some(unit) = unsafe { (&mut *el).pop_input() } {
        unsafe { *wc = crate::adapter::unit_to_wide(unit) };
        return 1;
    }

    if !unsafe { (&*el).is_tty() }
        || unsafe { (&mut *el).set_terminal_mode(nshedit::domain::TerminalMode::Editing) }.is_err()
    {
        return 0;
    }

    if let Some(callback) = unsafe { (&*el).read_callback() } {
        return unsafe { callback(el, wc) };
    }

    let mut pending = [0; 6];
    let mut used = 0usize;
    loop {
        match crate::conversion::decode_prefix(&pending[..used]) {
            crate::conversion::PrefixDecode::Complete(value) => {
                unsafe { *wc = value };
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
        match unsafe { (&*el).read_input(next) } {
            Ok(0) => {
                unsafe { *wc = 0 };
                return 0;
            }
            Ok(_) => used += 1,
            Err(error)
                if error.kind() == std::io::ErrorKind::Interrupted
                    && unsafe { (&*el).safe_read() } => {}
            Err(_) => {
                unsafe { *wc = 0 };
                return -1;
            }
        }
    }
}

unsafe fn host_history(
    el: *mut EditLine,
    direction: Direction,
) -> Result<HistoryResponse, HostFailure> {
    let Some((callback, cookie)) = (unsafe { (&*el).history_callback() }) else {
        return Err(HostFailure::Unavailable);
    };
    let depth = unsafe { (&*el).history_depth() };
    if direction == Direction::Next && depth == 0 {
        return Ok(HistoryResponse::Live);
    }
    let operation = match (direction, depth) {
        (Direction::Previous, 0) => H_FIRST,
        (Direction::Previous, _) => H_NEXT,
        (Direction::Next, _) => H_PREV,
    };
    let narrow = unsafe { (&*el).narrow_history() };
    let line = if narrow {
        let mut event = HistEvent {
            num: 0,
            str: core::ptr::null(),
        };
        let result = unsafe { callback(cookie, (&raw mut event).cast(), operation) };
        if result != 0 {
            return Ok(HistoryResponse::Boundary);
        }
        text_from_bytes(unsafe { cbytes(event.str) }.unwrap_or(&[]))
    } else {
        let mut event = HistEventW {
            num: 0,
            str: core::ptr::null(),
        };
        let result = unsafe { callback(cookie, &raw mut event, operation) };
        if result != 0 {
            return Ok(HistoryResponse::Boundary);
        }
        unsafe { wstr(event.str) }
            .unwrap_or(&[])
            .iter()
            .copied()
            .map(TextUnit::from_wide)
            .collect()
    };
    let next_depth = match direction {
        Direction::Previous => depth.saturating_add(1),
        Direction::Next => depth.saturating_sub(1),
    };
    unsafe { (&mut *el).set_history_depth(next_depth) };
    Ok(HistoryResponse::Entry(line))
}

unsafe fn host_command(
    el: *mut EditLine,
    name: &nshedit::domain::CommandName,
) -> Result<Outcome, HostFailure> {
    let Some(callback) = (unsafe { (&*el).command_callback(name) }) else {
        return Err(HostFailure::Unavailable);
    };
    let result = unsafe { callback(el, 0) };
    Ok(match result {
        CC_NEWLINE => Outcome::Accepted(unsafe { (&*el).native().line().clone() }),
        CC_EOF => Outcome::EndOfInput,
        crate::cdecl::histedit::CC_REFRESH | CC_REDISPLAY => Outcome::Refresh(Refresh::Full),
        CC_REFRESH_BEEP => Outcome::Refresh(Refresh::Beep),
        0 => Outcome::Continue,
        _ => Outcome::Refresh(Refresh::Beep),
    })
}

pub(super) unsafe fn drive_read(el: *mut EditLine) -> Result<ReadResult, ()> {
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
                let response = unsafe { (&*el).screen_size() }.ok_or(HostFailure::Unavailable);
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_resize(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::Read(pending) => {
                let response = unsafe { host_read(el) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_read(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::History(pending) => {
                let response = unsafe { host_history(el, pending.request().direction) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::RecordHistory(pending) => {
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_history_record(editor, &pending, Err(HostFailure::Unavailable))
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
                let response = unsafe { host_command(el, &pending.request().name) };
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_user_command(editor, &pending, response)
                    .map_err(|_| ())?
            }
            ReadStep::Signal(pending) => {
                let (editor, driver) = unsafe { (&mut *el).split_driver() };
                driver
                    .resume_signal(editor, &pending, Ok(()))
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
            ReadStep::Complete(result) => return Ok(result),
        };
    }
}

pub(super) unsafe fn read_unedited(el: *mut EditLine) -> Result<bool, ()> {
    let callback = unsafe { (&*el).read_callback() };
    let unbuffered = unsafe { (&*el).unbuffered() };
    let mut line = Text::default();
    let mut bytes = Vec::new();
    loop {
        if let Some(callback) = callback {
            let mut value = 0;
            match unsafe { callback(el, &raw mut value) } {
                result if result > 0 => {
                    let unit = TextUnit::from_wide(value);
                    line.push(unit);
                    if unbuffered || matches!(unit, TextUnit::Scalar('\r' | '\n')) {
                        break;
                    }
                }
                0 => break,
                _ => return Err(()),
            }
        } else {
            let mut byte = [0];
            match unsafe { (&*el).read_input(&mut byte) } {
                Ok(1) => {
                    bytes.push(byte[0]);
                    if unbuffered || matches!(byte[0], b'\r' | b'\n') {
                        break;
                    }
                }
                Ok(0) => break,
                Ok(_) => unreachable!("the one-byte buffer cannot read more than one byte"),
                Err(error)
                    if error.kind() == std::io::ErrorKind::Interrupted
                        && unsafe { (&*el).safe_read() } => {}
                Err(_) => return Err(()),
            }
        }
    }
    if callback.is_none() {
        line = text_from_bytes(&bytes);
    }
    let has_line = !line.is_empty();
    unsafe { (&mut *el).replace_line(line) }
        .then_some(())
        .ok_or(())?;
    Ok(has_line)
}
