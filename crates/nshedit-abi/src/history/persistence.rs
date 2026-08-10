use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, Write};
use std::mem::ManuallyDrop;
use std::os::fd::{FromRawFd, RawFd};
use std::path::Path;

use crate::conversion::ConversionBuffer;

use super::*;

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
pub(super) fn load<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    path: Option<&Path>,
) -> HistoryResult<C> {
    let Some(path) = path else {
        return Err(HistoryErrorKind::ReadFailed.into());
    };
    let Ok(file) = File::open(path) else {
        return Err(HistoryErrorKind::ReadFailed.into());
    };
    let mut input = BufReader::new(file);
    let nshedit_format = matches!(
        input.fill_buf(),
        Ok(head) if nshedit::history_file::detect(head) == nshedit::history_file::Format::Nshedit
    );
    if nshedit_format {
        load_nshedit(history, &mut input)
    } else {
        load_libedit(history, &mut input)
    }
    .map(HistoryReply::Count)
}

fn load_nshedit<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    input: &mut dyn Read,
) -> Result<usize, HistoryError<C>> {
    let mut bytes = Vec::new();
    let mut failed = input.read_to_end(&mut bytes).is_err();
    let (records, fault) = nshedit::history_file::read_nshedit(&bytes);
    failed |= fault.is_some();
    let mut conversion = ConversionBuffer::default();
    let mut count = 0usize;

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
        history.execute(HistoryRequest::Enter(decoded))?;
        count = count.saturating_add(1);
    }
    if failed {
        Err(HistoryErrorKind::ReadFailed.into())
    } else {
        Ok(count)
    }
}

fn load_libedit<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    input: &mut dyn BufRead,
) -> Result<usize, HistoryError<C>> {
    let mut line = Vec::new();
    let size = match input.read_until(b'\n', &mut line) {
        Ok(0) | Err(_) => return Err(HistoryErrorKind::ReadFailed.into()),
        Ok(size) => size,
    };
    if !cookie_prefix_matches(&line[..size]) {
        return Err(HistoryErrorKind::ReadFailed.into());
    }

    let mut capacity = 1024usize;
    let mut decoded = vec![0; capacity];
    let mut conversion = ConversionBuffer::default();
    let mut count = 0usize;

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
                return Err(HistoryErrorKind::ReadFailed.into());
            }
            decoded.resize(wanted, 0);
            capacity = wanted;
        }

        let decoded_length = decode_libedit_entry(&mut decoded, &line);
        let Some(text) = C::decode(Some(&decoded[..decoded_length]), &mut conversion) else {
            count = count.saturating_add(1);
            continue;
        };
        history.execute(HistoryRequest::Enter(text))?;
        count = count.saturating_add(1);
    }
    Ok(count)
}

fn cookie_prefix_matches(line: &[u8]) -> bool {
    for (index, &byte) in line.iter().enumerate() {
        let expected = nshedit::history_file::LIBEDIT_V2_HEADER
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

// [spec:libedit:def:history.history-save-fp-fn+1]
// [spec:libedit:sem:history.history-save-fp-fn+1]
pub(super) fn save_stream<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    count: usize,
    stream: SaveStream<'_>,
) -> HistoryResult<C> {
    publish_save(save_to(history, count, stream.output, stream.at_start))
}

fn publish_save<C>(result: io::Result<usize>) -> HistoryResult<C> {
    match result {
        Ok(count) => Ok(HistoryReply::Count(count)),
        Err(error) => {
            if let Some(errno) = error.raw_os_error()
                && errno != 0
            {
                crate::errno::set(errno);
            }
            Err(HistoryErrorKind::WriteFailed.into())
        }
    }
}

fn save_to<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    count: usize,
    output: &mut dyn Write,
    at_start: bool,
) -> io::Result<usize> {
    if at_start {
        output.write_all(nshedit::history_file::LIBEDIT_V2_HEADER)?;
    }

    let mut conversion = ConversionBuffer::default();
    let mut result;
    if count != usize::MAX {
        let mut remaining = count;
        result = history.execute(HistoryRequest::Move(HistoryMove::Newest));
        while result.is_ok() && remaining > 0 {
            remaining -= 1;
            result = history.execute(HistoryRequest::Move(HistoryMove::Older));
        }
    } else {
        result = Err(HistoryErrorKind::NotFound.into());
    }
    if result.is_err() {
        result = history.execute(HistoryRequest::Move(HistoryMove::Oldest));
    }

    let mut written = 0usize;
    while let Ok(reply) = result {
        let HistoryReply::Event(event) = reply else {
            return Err(io::Error::other("history traversal returned no event"));
        };
        let Some(bytes) = C::encode(event.text.as_deref(), &mut conversion) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "history entry is not encodable",
            ));
        };
        let mut encoded = encode_libedit_entry(bytes);
        encoded.push(b'\n');
        output.write_all(&encoded)?;
        written = written.saturating_add(1);
        result = history.execute(HistoryRequest::Move(HistoryMove::Newer));
    }
    output.flush()?;
    Ok(written)
}

pub(crate) fn save_fd<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    count: usize,
    descriptor: RawFd,
) -> HistoryResult<C> {
    if descriptor < 0 {
        return Err(HistoryErrorKind::WriteFailed.into());
    }
    // SAFETY: the descriptor is borrowed and `ManuallyDrop` prevents close.
    let mut file = ManuallyDrop::new(unsafe { File::from_raw_fd(descriptor) });
    let at_start = matches!(file.stream_position(), Ok(0));
    let result = {
        let mut output = BufWriter::new(&mut *file);
        save_to(history, count, &mut output, at_start)
    };
    publish_save(result)
}

fn save_path<C: HistoryChar>(history: &mut HistoryHandle<C>, path: &Path) -> io::Result<usize> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".nshedit-history-")
        .tempfile_in(parent)?;

    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let written = {
        let mut output = BufWriter::new(temporary.as_file_mut());
        save_to(history, usize::MAX, &mut output, true)?
    };
    temporary.as_file().sync_all()?;
    let persisted = temporary.persist(path).map_err(|failure| failure.error)?;
    drop(persisted);
    Ok(written)
}

// [spec:libedit:def:history.history-save-fn+1]
// [spec:libedit:sem:history.history-save-fn+1]
pub(super) fn save<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    path: Option<&Path>,
) -> HistoryResult<C> {
    let Some(path) = path else {
        return Err(HistoryErrorKind::WriteFailed.into());
    };
    publish_save(save_path(history, path))
}
