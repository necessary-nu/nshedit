use core::ffi::c_int;
use core::ptr;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, Write};
use std::mem::ManuallyDrop;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::conversion::ConversionBuffer;

use super::{
    EnterOperation, GetOperation, HistEventGen, HistoryChar, HistoryHandle, SaveStream, input,
};

fn empty_event<C>() -> HistEventGen<C> {
    HistEventGen {
        num: 0,
        str: ptr::null(),
    }
}

fn decode_libedit_entry(dst: &mut [u8], src: &[u8]) -> usize {
    dst.fill(0);
    let _ = bsd::vis::decode_into(dst, src, bsd::vis::Flags::NONE);
    dst.iter().position(|&byte| byte == 0).unwrap_or(dst.len())
}

fn encode_libedit_entry(bytes: &[u8]) -> Vec<u8> {
    bsd::vis::Encoder::new(bsd::vis::Flags::WHITE).encode(bytes)
}

// [spec:libedit:def:history.history-load-fn]
// [spec:libedit:sem:history.history-load-fn]
pub(super) fn load<C: HistoryChar>(history: &mut HistoryHandle<C>, path: &Path) -> c_int {
    let Ok(file) = File::open(path) else {
        return -1;
    };
    let mut input = BufReader::new(file);
    let native = matches!(
        input.fill_buf(),
        Ok(head) if nshedit::histfile::detect(head) == nshedit::histfile::Format::Native
    );
    if native {
        load_native(history, &mut input)
    } else {
        load_libedit(history, &mut input)
    }
}

fn load_native<C: HistoryChar>(history: &mut HistoryHandle<C>, input: &mut dyn Read) -> c_int {
    let mut bytes = Vec::new();
    let mut failed = input.read_to_end(&mut bytes).is_err();
    let (records, fault) = nshedit::histfile::read_all(&bytes);
    failed |= fault.is_some();
    let mut conversion = ConversionBuffer::default();
    let mut event = empty_event();
    let mut count: c_int = 0;

    for record in records {
        if record.text.contains(&0) {
            failed = true;
            continue;
        }
        let mut text: Vec<u8> = record.text.into();
        let length = text.len();
        text.push(0);
        let Some(decoded) = C::decode(Some(&text[..length]), &mut conversion) else {
            count = count.saturating_add(1);
            continue;
        };
        if history.enter_backend(&mut event, EnterOperation::Enter, decoded.as_ptr()) == -1 {
            return -1;
        }
        count = count.saturating_add(1);
    }
    if failed { -1 } else { count }
}

fn load_libedit<C: HistoryChar>(history: &mut HistoryHandle<C>, input: &mut dyn BufRead) -> c_int {
    let mut line = Vec::new();
    let size = match input.read_until(b'\n', &mut line) {
        Ok(0) | Err(_) => return -1,
        Ok(size) => size,
    };
    if !cookie_prefix_matches(&line[..size]) {
        return -1;
    }

    let mut capacity = 1024usize;
    let mut decoded = vec![0; capacity];
    let mut conversion = ConversionBuffer::default();
    let mut event = empty_event();
    let mut count: c_int = 0;

    loop {
        line.clear();
        let mut size = match input.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(size) => size,
        };
        if size > 0 && line[size - 1] == b'\n' {
            size -= 1;
            line.truncate(size);
        }
        line.push(0);
        if capacity <= size {
            let wanted = (size + 1024) & !1023usize;
            if decoded.try_reserve(wanted - decoded.len()).is_err() {
                return -1;
            }
            decoded.resize(wanted, 0);
            capacity = wanted;
        }

        let decoded_length = decode_libedit_entry(&mut decoded, &line);
        let Some(text) = C::decode(Some(&decoded[..decoded_length]), &mut conversion) else {
            count = count.saturating_add(1);
            continue;
        };
        if history.enter_backend(&mut event, EnterOperation::Enter, text.as_ptr()) == -1 {
            return -1;
        }
        count = count.saturating_add(1);
    }
    count
}

fn cookie_prefix_matches(line: &[u8]) -> bool {
    for (index, &byte) in line.iter().enumerate() {
        let expected = nshedit::histfile::LIBEDIT_V2_HEADER
            .get(index)
            .copied()
            .unwrap_or(0);
        if byte != expected {
            return false;
        }
        if byte == 0 {
            return true;
        }
    }
    true
}

// [spec:libedit:def:history.history-save-fp-fn]
// [spec:libedit:sem:history.history-save-fp-fn]
pub(super) fn save_stream<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    count: usize,
    stream: SaveStream<'_>,
) -> c_int {
    save_to(history, count, stream.output, stream.at_start)
}

fn save_to<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    count: usize,
    output: &mut dyn Write,
    at_start: bool,
) -> c_int {
    if at_start
        && output
            .write_all(nshedit::histfile::LIBEDIT_V2_HEADER)
            .is_err()
    {
        return -1;
    }

    let mut conversion = ConversionBuffer::default();
    let mut event = empty_event();
    let mut result;
    if count != usize::MAX {
        let mut remaining = count;
        result = history.get_backend(&mut event, GetOperation::First);
        while result != -1 && remaining > 0 {
            remaining -= 1;
            result = history.get_backend(&mut event, GetOperation::Next);
        }
    } else {
        result = -1;
    }
    if result == -1 {
        result = history.get_backend(&mut event, GetOperation::Last);
    }

    let mut written: c_int = 0;
    while result != -1 {
        let text = if event.str.is_null() {
            None
        } else {
            // SAFETY: a successful history callback lends a terminated
            // string until the entry is changed.
            Some(unsafe { input(event.str) })
        };
        let Some(bytes) = C::encode(text, &mut conversion) else {
            return -1;
        };
        let mut encoded = encode_libedit_entry(bytes);
        encoded.push(b'\n');
        let _ = output.write_all(&encoded);
        written = written.saturating_add(1);
        result = history.get_backend(&mut event, GetOperation::Previous);
    }
    written
}

pub(crate) fn save_fd<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    count: usize,
    descriptor: RawFd,
) -> c_int {
    if descriptor < 0 {
        return -1;
    }
    // SAFETY: the descriptor is borrowed and `ManuallyDrop` prevents close.
    let mut file = ManuallyDrop::new(unsafe { File::from_raw_fd(descriptor) });
    let at_start = matches!(file.stream_position(), Ok(0));
    let mut output = BufWriter::new(&mut *file);
    let result = save_to(history, count, &mut output, at_start);
    let _ = output.flush();
    result
}

// [spec:libedit:def:history.history-save-fn]
// [spec:libedit:sem:history.history-save-fn]
pub(super) fn save<C: HistoryChar>(history: &mut HistoryHandle<C>, path: &Path) -> c_int {
    let Ok(file) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
    else {
        return -1;
    };
    let mut output = BufWriter::new(file);
    let result = save_to(history, usize::MAX, &mut output, true);
    let _ = output.flush();
    result
}
