//! The native history file: a container, not a schema.
//!
//! # What a record is
//!
//! Two byte strings. The text of the entry, and a blob the application
//! defines and this crate never looks inside.
//!
//! That split is deliberate and it is the whole design. A history library has
//! no business deciding whether an entry carries a timestamp, an exit status,
//! a working directory or a session id — that is the application's schema, and
//! every format that decided it centrally has had to break compatibility to
//! change its mind. So `nshedit` stores bytes and `nsh` picks what goes in
//! them, which is why this module has no `serde` derive and nothing to
//! version.
//!
//! The text is **not** a `str`. A history entry is whatever the user typed,
//! and that is not required to be valid UTF-8 in any locale; making it a
//! `String` here would push a lossy conversion onto the one path that must not
//! lose anything.
//!
//! # Why the entry text is stored raw
//!
//! libedit's own format, `_HiStOrY_V2_`, stores one entry per line and so has
//! to guarantee that an entry contains no newline. It gets that from `vis(3)`,
//! which rewrites the bytes: a space becomes `\040`, a newline becomes `\012`.
//! That works, and it costs you the file:
//!
//! ```text
//! grep -c 'git commit'   this format      1
//! grep -c 'git commit'   _HiStOrY_V2_     0     stored as: git\040commit
//! ```
//!
//! Measured, on a file libedit wrote. A history file you cannot grep for a
//! command containing a space is one you cannot grep, and it fails silently —
//! you conclude you never ran the command.
//!
//! COBS gets the same guarantee the other way around. Rather than rewriting
//! the payload so it cannot contain the delimiter, it picks a delimiter the
//! payload cannot contain: `0x00`. Text with no NUL in it — which is every
//! shell command, since `execve` will not accept one — passes through
//! byte-for-byte, so the file stays greppable and the encoding stops depending
//! on the locale.
//!
//! # Why COBS rather than a length prefix
//!
//! Both frame. Only COBS resynchronises. A length prefix that is itself
//! corrupt desynchronises the remainder of the file, and a history file is
//! exactly the kind that gets truncated by a full disk or a killed shell —
//! which is why `bash` ships `histappend` and `zsh` ships `HIST_FCNTL_LOCK`.
//! With a delimiter you scan forward to the next `0x00` and lose one record.
//!
//! It also makes appending trivial, which matters when several shells share a
//! file: one `write(2)` of one frame under `PIPE_BUF` is atomic on the
//! platforms we target, so records cannot interleave.
//!
//! # Layout
//!
//! ```text
//! <header frame> 00 <record frame> 00 <record frame> 00 ...
//! ```
//!
//! The header is a frame like any other, so a reader that does not care about
//! it can skip one frame rather than a magic number of bytes. It carries
//! [`MAGIC`] and a format version, and it exists so that a native file is
//! *positively identified* rather than being whatever failed to look like
//! something else.

use std::io::{self, Write};

/// Identifies a native history file. Checked, not assumed: a file that does
/// not open with this is not ours, and we would rather say so than decode
/// somebody else's bytes into somebody's history.
pub const MAGIC: &[u8] = b"nshedit-history";

/// The container version — the framing and the record shape, not the
/// application's schema, which lives inside [`Record::blob`] and is none of
/// this module's business.
pub const VERSION: u8 = 1;

/// The frame delimiter. COBS guarantees an encoded frame contains no zero
/// byte, which is what makes this scannable.
const DELIM: u8 = 0;

/// One history entry: what was typed, and whatever the application attached.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    /// The entry itself, exactly as it was entered. Arbitrary bytes.
    pub text: Vec<u8>,
    /// Application-defined. Empty when there is none — notably on the C ABI
    /// path, where `HentryGen::data` is a bare `*mut c_void` with no length
    /// and therefore nothing that could be persisted even in principle.
    pub blob: Vec<u8>,
}

impl Record {
    /// A record carrying only the entry text.
    pub fn new(text: impl Into<Vec<u8>>) -> Self {
        Self {
            text: text.into(),
            blob: Vec::new(),
        }
    }
}

/// What went wrong reading a file.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The file does not open with a header frame naming [`MAGIC`].
    NotNativeFormat,
    /// The header names a container version this build does not implement.
    /// The version is reported so the message can say which.
    UnsupportedVersion(u8),
    /// A frame was not valid COBS, or its contents were not a valid record.
    /// Carries the zero-based index of the record, counting the header as 0.
    BadRecord(usize),
    /// The last frame has no terminator: the file was truncated mid-write.
    /// Everything before it is still returned — see [`read_all`].
    Truncated,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotNativeFormat => f.write_str("not an nshedit history file"),
            Error::UnsupportedVersion(v) => {
                write!(f, "history container version {v} is newer than this build")
            }
            Error::BadRecord(i) => write!(f, "record {i} is corrupt"),
            Error::Truncated => f.write_str("the file ends inside a record"),
        }
    }
}

impl std::error::Error for Error {}

/// True if `head` is the start of a native history file.
///
/// Give it at least [`sniff_len`] bytes; fewer is not an error, it just
/// answers `false`, because a file too short to hold a header cannot be one.
pub fn is_native(head: &[u8]) -> bool {
    let Some(frame) = head.split(|&b| b == DELIM).next() else {
        return false;
    };
    // A frame that is still being written has no delimiter yet, and
    // `split` would hand back the whole slice. Only accept a terminated one.
    if frame.len() == head.len() {
        return false;
    }
    match cobs_decode(frame) {
        Some(bytes) => bytes.starts_with(MAGIC),
        None => false,
    }
}

/// Enough bytes for [`is_native`] to reach a verdict: the header frame, its
/// COBS overhead and its delimiter.
pub fn sniff_len() -> usize {
    MAGIC.len() + 8
}

/// Writes the header frame. Call once, when creating the file; appending to
/// an existing one must not repeat it.
pub fn write_header<W: Write>(w: &mut W) -> io::Result<()> {
    let mut head = MAGIC.to_vec();
    head.push(VERSION);
    write_frame(w, &head)
}

/// Appends one record. Emits a single `write_all`, so that under `O_APPEND`
/// two shells writing at once interleave whole records rather than bytes.
pub fn append<W: Write>(w: &mut W, rec: &Record) -> io::Result<()> {
    let body = postcard::to_stdvec(&(rec.text.as_slice(), rec.blob.as_slice()))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    write_frame(w, &body)
}

fn write_frame<W: Write>(w: &mut W, body: &[u8]) -> io::Result<()> {
    let mut frame = cobs_encode(body);
    frame.push(DELIM);
    w.write_all(&frame)
}

/// Reads every record in `bytes`.
///
/// Returns what it could read *and* whatever stopped it, rather than one or
/// the other. A truncated final record is the ordinary outcome of a shell
/// being killed mid-write, and losing the other nine hundred entries over it
/// would be the wrong answer; so would silently pretending the file was
/// complete.
pub fn read_all(bytes: &[u8]) -> (Vec<Record>, Option<Error>) {
    let mut frames = bytes.split(|&b| b == DELIM);

    let Some(header) = frames.next() else {
        return (Vec::new(), Some(Error::NotNativeFormat));
    };
    let Some(header) = cobs_decode(header) else {
        return (Vec::new(), Some(Error::NotNativeFormat));
    };
    let Some(rest) = header.strip_prefix(MAGIC) else {
        return (Vec::new(), Some(Error::NotNativeFormat));
    };
    match rest.first() {
        Some(&VERSION) => {}
        Some(&v) => return (Vec::new(), Some(Error::UnsupportedVersion(v))),
        None => return (Vec::new(), Some(Error::NotNativeFormat)),
    }

    let mut out = Vec::new();
    let mut index = 1;
    let mut fault = None;
    for frame in frames {
        // `split` yields an empty tail after the final delimiter. That is a
        // complete file, not a fault.
        if frame.is_empty() {
            continue;
        }
        // A frame that reaches the end of the input without a delimiter was
        // still being written.
        if frame.as_ptr_range().end == bytes.as_ptr_range().end && !bytes.ends_with(&[DELIM]) {
            fault = Some(Error::Truncated);
            break;
        }
        match cobs_decode(frame).and_then(|body| decode_record(&body)) {
            Some(rec) => out.push(rec),
            None => {
                // Keep going. The delimiter is what makes that possible: one
                // corrupt record costs one record, and this is the property
                // a length-prefixed format would not have.
                fault.get_or_insert(Error::BadRecord(index));
            }
        }
        index += 1;
    }
    (out, fault)
}

fn decode_record(body: &[u8]) -> Option<Record> {
    let (text, blob): (Vec<u8>, Vec<u8>) = postcard::from_bytes(body).ok()?;
    Some(Record { text, blob })
}

// ---------------------------------------------------------------------------
// COBS
// ---------------------------------------------------------------------------
//
// Consistent Overhead Byte Stuffing, from Cheshire & Baker 1999. Every run of
// up to 254 non-zero bytes is emitted behind a code byte holding its length
// plus one; a zero in the input becomes the end of a run rather than a byte in
// the output. So the output contains no zero at all, and one zero byte can
// delimit frames unambiguously.
//
// Written here rather than taken from the `cobs` crate, which postcard already
// pulls in: postcard exposes `to_stdvec_cobs` for encoding but nothing for
// decoding a frame back, and taking a second direct dependency to get the
// other half is more surface than the twenty lines it saves.

/// Cost of framing: one code byte per 254 payload bytes, plus one to start.
fn cobs_encode(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + body.len() / 254 + 2);
    for block in body.split(|&b| b == 0) {
        let mut block = block;
        // A run longer than 253 needs splitting, and the 255 code says
        // "254 bytes follow and no zero ended them".
        while block.len() > 253 {
            out.push(255);
            out.extend_from_slice(&block[..254]);
            block = &block[254..];
        }
        out.push(block.len() as u8 + 1);
        out.extend_from_slice(block);
    }
    out
}

/// `None` if `frame` is not valid COBS: a code byte overrunning the frame, or
/// a zero byte, which cannot occur inside one.
fn cobs_decode(frame: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(frame.len());
    let mut i = 0;
    while i < frame.len() {
        let code = frame[i];
        if code == 0 {
            return None;
        }
        let len = code as usize - 1;
        let start = i + 1;
        let end = start.checked_add(len)?;
        if end > frame.len() {
            return None;
        }
        out.extend_from_slice(&frame[start..end]);
        i = end;
        // A full block ran to 254 bytes without a zero terminating it, so no
        // zero is reinserted. Any shorter block was ended by one — except the
        // last, which was ended by the frame.
        if code != 255 && i < frame.len() {
            out.push(0);
        }
    }
    Some(out)
}

#[cfg(test)]
mod test {
    use super::*;

    fn file(records: &[Record]) -> Vec<u8> {
        let mut out = Vec::new();
        write_header(&mut out).unwrap();
        for r in records {
            append(&mut out, r).unwrap();
        }
        out
    }

    /// The claim the format exists for. `_HiStOrY_V2_` stores this as
    /// `git\040commit\040-m\040'fix\040it'` and `grep` finds nothing.
    #[test]
    fn a_command_survives_verbatim_and_is_greppable() {
        let cmd = b"git commit -m 'fix it'";
        let bytes = file(&[Record::new(&cmd[..])]);
        assert!(
            bytes.windows(cmd.len()).any(|w| w == cmd),
            "the command does not appear verbatim in the file"
        );
    }

    /// The property `vis` was there to provide, obtained without touching the
    /// payload: an embedded newline is stored raw and still round-trips.
    #[test]
    fn arbitrary_bytes_round_trip() {
        let awkward: Vec<Record> = vec![
            Record::new(&b"echo two\nlines"[..]),
            Record::new(&b"trailing space  "[..]),
            Record::new(&b"tab\there"[..]),
            Record::new(&b"nul\0inside"[..]),
            Record::new(&b"\xff\xfe not utf-8"[..]),
            Record::new(&b""[..]),
            Record {
                text: b"with a blob".to_vec(),
                blob: vec![0, 1, 2, 0, 0, 255],
            },
        ];
        let (back, fault) = read_all(&file(&awkward));
        assert_eq!(fault, None);
        assert_eq!(back, awkward);
    }

    /// Every COBS implementation that breaks, breaks at 254.
    #[test]
    fn cobs_handles_the_block_boundary() {
        for len in [0usize, 1, 252, 253, 254, 255, 256, 507, 508, 509, 1000] {
            for fill in [b'a', 0u8] {
                let body = vec![fill; len];
                let framed = cobs_encode(&body);
                assert!(
                    !framed.contains(&0),
                    "len {len} fill {fill}: encoded frame contains a delimiter"
                );
                assert_eq!(
                    cobs_decode(&framed).as_deref(),
                    Some(body.as_slice()),
                    "len {len} fill {fill}: did not round-trip"
                );
            }
        }
    }

    #[test]
    fn cobs_round_trips_every_short_pattern() {
        // Exhaustive over the shapes that actually differ: where the zeros are.
        for n in 0u32..(1 << 12) {
            let body: Vec<u8> = (0..12)
                .map(|i| if n >> i & 1 == 1 { 0 } else { b'x' })
                .collect();
            let framed = cobs_encode(&body);
            assert!(!framed.contains(&0));
            assert_eq!(cobs_decode(&framed).as_deref(), Some(body.as_slice()));
        }
    }

    /// A shell killed mid-write. The point of the format is that this costs
    /// one entry, not the file.
    #[test]
    fn a_truncated_tail_keeps_everything_before_it() {
        let whole = file(&[
            Record::new(&b"first"[..]),
            Record::new(&b"second"[..]),
            Record::new(&b"third"[..]),
        ]);
        // Cut inside the last record.
        let cut = whole.len() - 3;
        let (back, fault) = read_all(&whole[..cut]);
        assert_eq!(fault, Some(Error::Truncated));
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].text, b"first");
        assert_eq!(back[1].text, b"second");
    }

    /// The property a length-prefixed format would not have: corruption is
    /// local, because the reader resynchronises on the delimiter.
    #[test]
    fn a_corrupt_record_costs_one_record() {
        let mut bytes = file(&[
            Record::new(&b"before"[..]),
            Record::new(&b"CORRUPTME"[..]),
            Record::new(&b"after"[..]),
        ]);
        // Overwrite the middle record's COBS code byte with one that overruns.
        let at = bytes
            .windows(9)
            .position(|w| w == b"CORRUPTME")
            .expect("the record should be verbatim in the file");
        bytes[at - 1] = 200;

        let (back, fault) = read_all(&bytes);
        assert_eq!(fault, Some(Error::BadRecord(2)));
        assert_eq!(back.len(), 2, "should have kept the two good records");
        assert_eq!(back[0].text, b"before");
        assert_eq!(back[1].text, b"after");
    }

    #[test]
    fn a_libedit_file_is_not_mistaken_for_ours() {
        let libedit = b"_HiStOrY_V2_\necho\\040plain\n";
        assert!(!is_native(libedit));
        assert_eq!(read_all(libedit).1, Some(Error::NotNativeFormat));

        // Nor is a readline file, which has no header at all.
        let readline = b"echo plain\necho other\n";
        assert!(!is_native(readline));
        assert_eq!(read_all(readline).1, Some(Error::NotNativeFormat));
    }

    #[test]
    fn our_own_file_is_recognised_from_the_first_few_bytes() {
        let bytes = file(&[Record::new(&b"anything"[..])]);
        assert!(is_native(&bytes));
        assert!(
            is_native(&bytes[..sniff_len().min(bytes.len())]),
            "sniff_len() must be enough to reach a verdict"
        );
        // A header still being written is not yet a native file.
        assert!(!is_native(&bytes[..4]));
    }

    #[test]
    fn a_newer_container_version_is_named_rather_than_guessed_at() {
        let mut bytes = file(&[Record::new(&b"x"[..])]);
        // The version byte sits at the end of the header frame's payload,
        // which is the byte before the first delimiter.
        let delim = bytes.iter().position(|&b| b == DELIM).unwrap();
        bytes[delim - 1] = VERSION + 1;
        assert_eq!(
            read_all(&bytes).1,
            Some(Error::UnsupportedVersion(VERSION + 1))
        );
    }

    #[test]
    fn an_empty_file_is_a_clear_answer_not_a_panic() {
        assert_eq!(read_all(b"").1, Some(Error::NotNativeFormat));
        let mut header = Vec::new();
        write_header(&mut header).unwrap();
        let (back, fault) = read_all(&header);
        assert_eq!(fault, None);
        assert!(back.is_empty());
    }
}
