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

use bstr::BString;
#[cfg(feature = "bsd")]
use bstr::ByteSlice;
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
///
/// The two fields have different types on purpose. [`text`](Record::text) is a
/// [`BString`] — bytes that are *conventionally* text, without the UTF-8
/// promise a `String` would make and cannot keep about what someone typed. It
/// prints readably in a test failure and carries `bstr`'s string operations,
/// which the search and listing paths want. [`blob`](Record::blob) is a plain
/// `Vec<u8>` because it is genuinely opaque: nothing here may treat it as text
/// or look inside it at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Record {
    /// The entry itself, exactly as it was entered. Arbitrary bytes.
    pub text: BString,
    /// Application-defined. Empty when there is none — notably on the C ABI
    /// path, where `HentryGen::data` is a bare `*mut c_void` with no length
    /// and therefore nothing that could be persisted even in principle.
    pub blob: Vec<u8>,
}

impl Record {
    /// A record carrying only the entry text.
    pub fn new(text: impl Into<BString>) -> Self {
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
    /// The file carries neither format's signature. Reported rather than
    /// guessed at: see [`Format::Unknown`].
    UnrecognisedFormat,
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
            Error::UnrecognisedFormat => f.write_str("not a history file we recognise"),
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

/// libedit's own file starts with this, and has since it was V1.
/// Signature line used by libedit's legacy V2 history files.
///
/// This is public because compatibility adapters can read and write the
/// format without importing libedit's C-shaped history implementation.
pub const LIBEDIT_V2_HEADER: &[u8] = b"_HiStOrY_V2_\n";

/// Which history file this is.
///
/// Both variants are recognised by a signature the format *puts there*, never
/// by elimination. A file that is neither is [`Unknown`](Format::Unknown)
/// rather than being decoded on the assumption that it must be one of them —
/// guessing wrong here means presenting one user's file as another's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Ours: COBS-framed records behind a [`MAGIC`] header frame.
    Native,
    /// libedit's `_HiStOrY_V2_`: one vis-encoded entry per line.
    LibeditV2,
    /// Neither signature is present. A GNU readline file lands here, since
    /// its format is raw lines with nothing identifying them.
    Unknown,
}

/// Identifies a file from its first few bytes.
///
/// Reading is deliberately not behind a feature or an option. Whichever
/// format we write, a user's existing file has to keep opening — otherwise
/// changing the default silently loses everybody's history, and every switch
/// becomes a migration someone has to be told about.
pub fn detect(head: &[u8]) -> Format {
    if head.starts_with(LIBEDIT_V2_HEADER) {
        Format::LibeditV2
    } else if is_native(head) {
        Format::Native
    } else {
        Format::Unknown
    }
}

/// Reads a history file of whichever format it turns out to be.
///
/// Same contract as [`read_all`]: whatever could be read, plus whatever
/// stopped it, because a partial history beats none.
pub fn read_any(bytes: &[u8]) -> (Vec<Record>, Option<Error>) {
    match detect(bytes) {
        Format::Native => read_all(bytes),
        #[cfg(feature = "bsd")]
        Format::LibeditV2 => read_libedit(bytes),
        #[cfg(not(feature = "bsd"))]
        Format::LibeditV2 => (Vec::new(), Some(Error::UnrecognisedFormat)),
        Format::Unknown => (Vec::new(), Some(Error::UnrecognisedFormat)),
    }
}

/// Reads libedit's `_HiStOrY_V2_`: the cookie line, then one vis-encoded
/// entry per line.
///
/// Entries get an empty [`Record::blob`], which is not a loss — the format
/// has nowhere to put one. `H_SAVE` writes `ev.str` and drops the entry's
/// `void *data`, so a libedit file never carried application data to begin
/// with.
#[cfg(feature = "bsd")]
fn read_libedit(bytes: &[u8]) -> (Vec<Record>, Option<Error>) {
    let mut out = Vec::new();
    let mut fault = None;
    for (i, line) in bytes.lines().enumerate().skip(1) {
        // The C skips empty lines rather than entering an empty event.
        if line.is_empty() {
            continue;
        }
        match unvis_line(line) {
            Some(text) => out.push(Record::new(text)),
            // Local, like the native reader's: one unreadable line costs one
            // entry. The C is even more forgiving — `history.c:869` casts
            // `strunvis`'s return to void and enters whatever it produced —
            // but entering a half-decoded line is worse than saying so.
            None => {
                fault.get_or_insert(Error::BadRecord(i));
            }
        }
    }
    (out, fault)
}

/// One vis-encoded line, decoded. The single seam onto `vis(3)`.
#[cfg(feature = "bsd")]
fn unvis_line(line: &[u8]) -> Option<Vec<u8>> {
    bsd::vis::decode(line, bsd::vis::Flags::NONE).ok()
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
    Some(Record {
        text: text.into(),
        blob,
    })
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

// ---------------------------------------------------------------------------
// The `bsd` seam
// ---------------------------------------------------------------------------
//
// Reading the legacy `_HiStOrY_V2_` format, and nothing else. What is left
// here is the one direction `crate::vislite` does not cover: `unvis` has to
// accept every escape form `vis` can emit, including the `VIS_CSTYLE` and
// `VIS_HTTPSTYLE` ones nothing in this crate produces, so it is a decoder for
// the whole language rather than a subset of it.
//
// Writing does not come through here any more. Both writers want an encoder —
// the listing `strvis(…, VIS_NL)`, the save `strvis(…, VIS_WHITE)` — and
// `vislite` supplies both with no feature behind them. That is not a
// convenience: `vis_encode` answering `None` on a default build made
// `history_save` truncate the user's file and then fail, and made the
// `history` builtin do nothing at all.

/// `strnunvis(dst, dlen, src)`. `dst` is zeroed first and the decoded length
/// found by scanning, because a bad escape leaves the successfully decoded
/// prefix — the C's policy — and `decode_into` writes that prefix without
/// measuring it.
#[cfg(feature = "bsd")]
pub fn vis_decode_into(dst: &mut [u8], src: &[u8]) -> usize {
    dst.fill(0);
    let _ = bsd::vis::decode_into(dst, src, bsd::vis::Flags::NONE);
    dst.iter().position(|&b| b == 0).unwrap_or(dst.len())
}

#[cfg(not(feature = "bsd"))]
pub fn vis_decode_into(_dst: &mut [u8], _src: &[u8]) -> usize {
    0
}

/// Encode one entry for libedit's V2 line-oriented history format.
///
/// The returned bytes contain no trailing newline. Space, tab, newline, and
/// backslash are escaped exactly as `strvis(3)` with `VIS_WHITE` does.
#[must_use]
pub fn encode_libedit_entry(bytes: &[u8]) -> Vec<u8> {
    crate::vislite::encode(crate::vislite::Escape::White, bytes)
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
                text: b"with a blob".into(),
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
    /// The point of sniffing: a file a user already has must keep opening,
    /// whichever format we happen to write today.
    ///
    /// These bytes are a real `_HiStOrY_V2_` file, produced by the in-tree C
    /// through `write_history(3)` and copied here verbatim, escapes and all.
    #[test]
    #[cfg(feature = "bsd")]
    fn a_real_libedit_file_reads_back() {
        let libedit: &[u8] = b"_HiStOrY_V2_\n\
            echo\\040plain\n\
            echo\\040two\\012lines\n\
            echo\\011tab\\040and\\040star\n\
            echo\\040trailing\\040space\\040\n";

        assert_eq!(detect(libedit), Format::LibeditV2);
        let (back, fault) = read_any(libedit);
        assert_eq!(fault, None);
        let texts: Vec<&[u8]> = back.iter().map(|r| r.text.as_slice()).collect();
        assert_eq!(
            texts,
            vec![
                &b"echo plain"[..],
                &b"echo two\nlines"[..],
                &b"echo\ttab and star"[..],
                &b"echo trailing space "[..],
            ]
        );
        // The format has nowhere to put one, so every blob is empty rather
        // than the reader having invented something.
        assert!(back.iter().all(|r| r.blob.is_empty()));
    }

    /// The escaping libedit applies is exactly what the decoder undoes: the
    /// newline that made the format need `vis` in the first place comes back
    /// as a newline, inside a single entry rather than splitting it in two.
    #[test]
    #[cfg(feature = "bsd")]
    fn the_embedded_newline_survives_the_legacy_path() {
        let libedit: &[u8] = b"_HiStOrY_V2_\necho\\040two\\012lines\n";
        let (back, _) = read_any(libedit);
        assert_eq!(back.len(), 1, "one entry, not two");
        assert_eq!(back[0].text, b"echo two\nlines");
    }

    #[test]
    fn each_format_is_recognised_by_its_own_signature() {
        let native = file(&[Record::new(&b"x"[..])]);
        assert_eq!(detect(&native), Format::Native);
        assert_eq!(detect(b"_HiStOrY_V2_\nfoo\n"), Format::LibeditV2);

        // A GNU readline file is raw lines with nothing identifying them, so
        // it is Unknown rather than being decoded as either.
        assert_eq!(detect(b"echo plain\necho other\n"), Format::Unknown);
        assert_eq!(detect(b""), Format::Unknown);
        assert_eq!(read_any(b"echo plain\n").1, Some(Error::UnrecognisedFormat));
    }

    /// The simplest legacy file: no escapes, so nothing to undo.
    #[test]
    #[cfg(feature = "bsd")]
    fn an_unescaped_legacy_file_reads_back() {
        let libedit: &[u8] = b"_HiStOrY_V2_\nls\npwd\n";
        let (back, fault) = read_any(libedit);
        assert_eq!(fault, None);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].text, b"ls");
    }

    /// A bad escape costs one entry, matching the native reader. libedit
    /// itself is looser — `history.c:869` casts `strunvis`'s return to void
    /// and enters whatever came out — but a half-decoded command is worse
    /// than a reported gap.
    ///
    /// `\M!` is the realistic corruption, not an invented one: libedit writes
    /// a UTF-8 byte as `\M-b`, so a single flipped byte in a history file
    /// containing any non-ASCII produces exactly this. Most malformed input
    /// does NOT reach here — `\z` decodes to `z` and `\888` to `888`, since
    /// `unvis` drops the backslash rather than failing, measured against the
    /// `bsd` decoder.
    #[test]
    #[cfg(feature = "bsd")]
    fn a_bad_escape_costs_one_entry() {
        let libedit: &[u8] = b"_HiStOrY_V2_\ngood\\040one\n\\M!bad\ngood\\040two\n";
        let (back, fault) = read_any(libedit);
        assert!(matches!(fault, Some(Error::BadRecord(_))), "{fault:?}");
        let texts: Vec<&[u8]> = back.iter().map(|r| r.text.as_slice()).collect();
        assert_eq!(texts, vec![&b"good one"[..], &b"good two"[..]]);
    }
    /// Without `bsd` there is no vis, so a libedit file is a format this
    /// build does not read.
    #[test]
    #[cfg(not(feature = "bsd"))]
    fn a_legacy_file_is_unreadable_without_the_bsd_feature() {
        let libedit: &[u8] = b"_HiStOrY_V2_\nls\necho\\040plain\n";
        let (back, fault) = read_any(libedit);
        assert_eq!(fault, Some(Error::UnrecognisedFormat));
        assert!(back.is_empty());
    }
}
