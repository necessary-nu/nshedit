// Copyright 2019 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! ncurses-compatible compiled terminfo format parsing (term(5))

use std::collections::HashMap;
use std::io;
use std::io::prelude::*;

use crate::CapabilityState;
use crate::Error::*;
use crate::NameTable;
use crate::Result;
use crate::TermInfo;

pub use crate::parser::names::*;

const LEGACY_MAGIC: u16 = 0x011a;
const EXTENDED_NUMBER_MAGIC: u16 = 0x021e;

fn read_le_i16(r: &mut dyn io::Read) -> io::Result<i32> {
    let mut buf = [0; 2];
    r.read_exact(&mut buf)
        .map(|()| i32::from(i16::from_le_bytes(buf)))
}

fn read_le_i32(r: &mut dyn io::Read) -> io::Result<i32> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf).map(|()| i32::from_le_bytes(buf))
}

fn read_byte(r: &mut dyn io::Read) -> io::Result<u8> {
    let mut byte = [0u8; 1];
    match r.read(&mut byte)? {
        0 => Err(io::Error::other("end of file")),
        _ => Ok(byte[0]),
    }
}

fn decode_number(value: i32) -> Result<CapabilityState<u32>> {
    match value {
        -1 => Ok(CapabilityState::Absent),
        -2 => Ok(CapabilityState::Cancelled),
        0.. => Ok(CapabilityState::Value(value as u32)),
        _ => Err(InvalidNumber(value)),
    }
}

fn decode_string_offset(value: i16) -> Result<CapabilityState<usize>> {
    match value {
        -1 => Ok(CapabilityState::Absent),
        -2 => Ok(CapabilityState::Cancelled),
        0.. => Ok(CapabilityState::Value(value as usize)),
        _ => Err(InvalidStringOffset(value)),
    }
}

fn decode_boolean(value: u8) -> Result<CapabilityState<bool>> {
    match value {
        0 => Ok(CapabilityState::Absent),
        1 => Ok(CapabilityState::Value(true)),
        0xfe => Ok(CapabilityState::Cancelled),
        _ => Err(InvalidBoolean(value)),
    }
}

// The capability orders are ncurses' standard tables. The header counts let
// older readers stop before any later additions.
// [spec:nshedit:req:terminal.typed-api]
// [spec:nshedit:req:terminal.compiled-capability-state]
/// Parse a compiled terminfo entry, keying its capabilities by `names`.
pub fn parse(file: &mut dyn io::Read, names: NameTable) -> Result<TermInfo> {
    let (bnames, snames, nnames) = match names {
        NameTable::VariableNames => (BOOL_LONG_NAMES, STRING_LONG_NAMES, NUMBER_LONG_NAMES),
        NameTable::Capnames => (BOOL_NAMES, STRING_NAMES, NUMBER_NAMES),
    };

    // Check magic number
    let mut buf = [0; 2];
    file.read_exact(&mut buf)?;
    let magic = u16::from_le_bytes(buf);

    let read_number = match magic {
        LEGACY_MAGIC => read_le_i16,
        EXTENDED_NUMBER_MAGIC => read_le_i32,
        _ => return Err(BadMagic(magic)),
    };

    // Header sizes are signed 16-bit fields. Minus one means that the section
    // is absent; treating it as zero is exact because no bytes follow.
    macro_rules! read_nonneg {
        () => {{
            match read_le_i16(file)? {
                n if n >= 0 => n as usize,
                -1 => 0,
                _ => return Err(InvalidLength),
            }
        }};
    }

    let names_bytes = read_nonneg!();
    let bools_bytes = read_nonneg!();
    let numbers_count = read_nonneg!();
    let string_offsets_count = read_nonneg!();
    let string_table_bytes = read_nonneg!();

    if names_bytes == 0 {
        return Err(ShortNames);
    }

    if bools_bytes > BOOL_NAMES.len() {
        return Err(TooManyBools);
    }

    if numbers_count > NUMBER_NAMES.len() {
        return Err(TooManyNumbers);
    }

    if string_offsets_count > STRING_NAMES.len() {
        return Err(TooManyStrings);
    }

    // don't read NUL
    let mut bytes = Vec::new();
    file.take((names_bytes - 1) as u64)
        .read_to_end(&mut bytes)?;
    let names_str = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => return Err(NotUtf8(e.utf8_error())),
    };

    let term_names: Vec<String> = names_str.split('|').map(|s| s.to_owned()).collect();
    // consume NUL
    if read_byte(file)? != b'\0' {
        return Err(NamesMissingNull);
    }

    let bools_map = (0..bools_bytes)
        .map(|i| Ok((bnames[i], decode_boolean(read_byte(file)?)?)))
        .collect::<Result<HashMap<_, _>>>()?;

    if (bools_bytes + names_bytes) % 2 == 1 {
        read_byte(file)?; // compensate for padding
    }

    let numbers_map = (0..numbers_count)
        .map(|i| Ok((nnames[i], decode_number(read_number(file)?)?)))
        .collect::<Result<HashMap<_, _>>>()?;

    let string_map: HashMap<&str, CapabilityState<Vec<u8>>> = if string_offsets_count > 0 {
        let string_offsets = (0..string_offsets_count)
            .map(|_| {
                let mut buf = [0; 2];
                file.read_exact(&mut buf)?;
                decode_string_offset(i16::from_le_bytes(buf))
            })
            .collect::<Result<Vec<_>>>()?;

        // `read_exact` rather than `take(..).read_to_end(..)`: a file that
        // ends inside its own string table has to fail here, the same way
        // every other short read in this format does. Reading what is there
        // and trusting the declared length instead left the slices below out
        // of range. The length is a signed 16-bit header value accepted only
        // when non-negative, so the allocation is bounded by 32767 bytes.
        let mut string_table = vec![0; string_table_bytes];
        file.read_exact(&mut string_table)?;

        string_offsets
            .into_iter()
            .enumerate()
            .map(|(i, offset)| {
                let name = if snames[i] == "_" {
                    STRING_LONG_NAMES[i]
                } else {
                    snames[i]
                };

                let offset = match offset {
                    CapabilityState::Absent => {
                        return Ok((name, CapabilityState::Absent));
                    }
                    CapabilityState::Cancelled => {
                        return Ok((name, CapabilityState::Cancelled));
                    }
                    CapabilityState::Value(offset) => offset,
                };

                // The offset is a claim the file makes about itself, and
                // nothing above has measured it against the table that is
                // actually there; indexing on trust panicked.
                let tail = string_table.get(offset..).ok_or(StringOffsetOutOfRange)?;

                // Find the offset of the NUL we want to go to
                match tail.iter().position(|&b| b == 0) {
                    Some(len) => Ok((name, CapabilityState::Value(tail[..len].to_vec()))),
                    None => Err(StringsMissingNull),
                }
            })
            .collect::<Result<HashMap<_, _>>>()?
    } else {
        HashMap::new()
    };

    // And that's all there is to it
    Ok(TermInfo {
        names: term_names,
        bools: bools_map,
        numbers: numbers_map,
        strings: string_map,
    })
}

#[cfg(test)]
mod test {
    // The `Entry` builder and everything below `test_veclens` is our work,
    // not `term` 1.2.1's — upstream shipped `test_veclens` alone, and left
    // the compiled fixtures in `tests/data` with no consumer at all.

    use std::path::PathBuf;

    use super::{
        BOOL_LONG_NAMES, BOOL_NAMES, EXTENDED_NUMBER_MAGIC, LEGACY_MAGIC, NUMBER_LONG_NAMES,
        NUMBER_NAMES, STRING_LONG_NAMES, STRING_NAMES, parse,
    };
    use crate::{CapabilityName, CapabilityState, Error, Result, TermInfo};

    #[test]
    fn test_veclens() {
        assert_eq!(BOOL_LONG_NAMES.len(), BOOL_NAMES.len());
        assert_eq!(NUMBER_LONG_NAMES.len(), NUMBER_NAMES.len());
        assert_eq!(STRING_LONG_NAMES.len(), STRING_NAMES.len());
    }

    // -----------------------------------------------------------------
    // synthesised entries — aiming at one guard at a time
    // -----------------------------------------------------------------

    /// A compiled entry assembled byte for byte, so each guard in `parse`
    /// can be reached deliberately. `term(5)` fixes the layout: a magic
    /// number, five section sizes, then the sections in order, with a pad
    /// byte after the booleans when the names and booleans together occupy
    /// an odd number of bytes.
    struct Entry {
        magic: u16,
        /// Without the terminating NUL, which `bytes` appends.
        names: Vec<u8>,
        bools: Vec<u8>,
        numbers: Vec<i32>,
        offsets: Vec<i16>,
        table: Vec<u8>,
    }

    /// Byte offset of each `u16` header field, `0` being the magic number.
    fn header_field(field: usize) -> usize {
        field * 2
    }

    impl Entry {
        /// A well-formed entry holding one absent and one present capability
        /// of every kind, so that both branches of each section's filter run.
        fn minimal() -> Entry {
            Entry {
                magic: LEGACY_MAGIC,
                names: b"nsh|nshterm test entry".to_vec(),
                bools: vec![0, 1],
                numbers: vec![80],
                offsets: vec![-1, 0],
                table: b"\x07\0".to_vec(),
            }
        }

        fn bytes(&self) -> Vec<u8> {
            let names_bytes = self.names.len() + 1;
            let mut v = Vec::new();
            v.extend(self.magic.to_le_bytes());
            for n in [
                names_bytes,
                self.bools.len(),
                self.numbers.len(),
                self.offsets.len(),
                self.table.len(),
            ] {
                v.extend((n as u16).to_le_bytes());
            }
            v.extend(&self.names);
            v.push(0);
            v.extend(&self.bools);
            if (self.bools.len() + names_bytes) % 2 == 1 {
                v.push(0);
            }
            for n in &self.numbers {
                if self.magic == EXTENDED_NUMBER_MAGIC {
                    v.extend(n.to_le_bytes());
                } else {
                    v.extend(
                        i16::try_from(*n)
                            .expect("legacy fixture number must fit i16")
                            .to_le_bytes(),
                    );
                }
            }
            for o in &self.offsets {
                v.extend(o.to_le_bytes());
            }
            v.extend(&self.table);
            v
        }
    }

    use crate::NameTable;

    fn parse_bytes(bytes: &[u8]) -> Result<TermInfo> {
        let mut reader = bytes;
        parse(&mut reader, NameTable::Capnames)
    }

    /// Parse, asserting failure, and return the error. `TermInfo` is not
    /// `PartialEq`, so the `Result` cannot be compared whole.
    fn parse_err(bytes: &[u8]) -> Error {
        match parse_bytes(bytes) {
            Ok(term) => panic!("unexpectedly parsed as {:?}", term.names),
            Err(e) => e,
        }
    }

    /// Overwrite one header field after the fact, to declare a size the
    /// builder would never produce.
    fn with_header(bytes: &[u8], field: usize, value: u16) -> Vec<u8> {
        let mut v = bytes.to_vec();
        let at = header_field(field);
        v[at..at + 2].copy_from_slice(&value.to_le_bytes());
        v
    }

    #[test]
    fn a_minimal_entry_round_trips() {
        let term = parse_bytes(&Entry::minimal().bytes()).expect("minimal entry should parse");
        assert_eq!(term.names, ["nsh", "nshterm test entry"]);
        assert_eq!(term.number(CapabilityName::Terminfo("cols")), Some(80));
        assert_eq!(
            term.string(CapabilityName::Terminfo("bel")).as_deref(),
            Some(&b"\x07"[..])
        );
        assert_eq!(
            term.boolean_state(CapabilityName::Terminfo("am")),
            CapabilityState::Value(true)
        );
        assert_eq!(
            term.boolean_state(CapabilityName::Terminfo("bw")),
            CapabilityState::Absent
        );
    }

    // [spec:nshedit:req:terminal.compiled-capability-state/test]
    #[test]
    fn boolean_states_follow_compiled_encoding() {
        let mut entry = Entry::minimal();
        entry.bools = vec![0, 0xfe, 1];
        let term = parse_bytes(&entry.bytes()).expect("boolean states should parse");

        assert_eq!(
            term.boolean_state(CapabilityName::Terminfo("bw")),
            CapabilityState::Absent
        );
        assert_eq!(
            term.boolean_state(CapabilityName::Terminfo("am")),
            CapabilityState::Cancelled
        );
        assert_eq!(
            term.boolean_state(CapabilityName::Terminfo("xsb")),
            CapabilityState::Value(true)
        );
        assert_eq!(term.booleans().collect::<Vec<_>>(), [("xsb", true)]);
    }

    #[test]
    fn the_pad_byte_after_the_booleans_is_honoured() {
        // One fewer name byte flips the parity, so the parser has to consume
        // the alignment byte or every number afterwards is shifted by one.
        let mut e = Entry::minimal();
        e.names.pop();
        let term = parse_bytes(&e.bytes()).expect("odd-length entry should parse");
        assert_eq!(term.number(CapabilityName::Terminfo("cols")), Some(80));
        assert_eq!(
            term.string(CapabilityName::Terminfo("bel")).as_deref(),
            Some(&b"\x07"[..])
        );
    }

    // [spec:nshedit:req:terminal.compiled-capability-state/test]
    #[test]
    fn numeric_states_decode_at_both_widths() {
        for magic in [LEGACY_MAGIC, EXTENDED_NUMBER_MAGIC] {
            let mut entry = Entry::minimal();
            entry.magic = magic;
            entry.numbers = vec![-1, -2, 0];
            let term = parse_bytes(&entry.bytes()).expect("numeric states should parse");

            assert_eq!(
                term.number_state(CapabilityName::Terminfo("cols")),
                CapabilityState::Absent,
                "magic {magic:#06x}"
            );
            assert_eq!(
                term.number_state(CapabilityName::Terminfo("it")),
                CapabilityState::Cancelled,
                "magic {magic:#06x}"
            );
            assert_eq!(
                term.number_state(CapabilityName::Terminfo("lines")),
                CapabilityState::Value(0),
                "magic {magic:#06x}"
            );
            assert_eq!(term.numbers().collect::<Vec<_>>(), [("lines", 0)]);
        }
    }

    // [spec:nshedit:req:terminal.compiled-capability-state/test]
    #[test]
    fn string_states_distinguish_cancelled_and_empty() {
        let mut entry = Entry::minimal();
        entry.offsets = vec![-1, -2, 0, 1];
        entry.table = b"\0X\0".to_vec();
        let term = parse_bytes(&entry.bytes()).expect("string states should parse");

        assert_eq!(
            term.string_state(CapabilityName::Terminfo("cbt")),
            CapabilityState::Absent
        );
        assert_eq!(
            term.string_state(CapabilityName::Terminfo("bel")),
            CapabilityState::Cancelled
        );
        assert_eq!(
            term.string_state(CapabilityName::Terminfo("cr")),
            CapabilityState::Value(std::borrow::Cow::Borrowed(&b""[..]))
        );
        assert_eq!(
            term.string_state(CapabilityName::Terminfo("csr")),
            CapabilityState::Value(std::borrow::Cow::Borrowed(&b"X"[..]))
        );
        assert_eq!(term.strings().count(), 2);
        assert_eq!(term.string(CapabilityName::Terminfo("bel")), None);
    }

    #[test]
    fn a_wrong_magic_number_is_rejected() {
        let bytes = with_header(&Entry::minimal().bytes(), 0, 0x1234);
        match parse_err(&bytes) {
            Error::BadMagic(magic) => assert_eq!(magic, 0x1234),
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    #[test]
    fn an_entry_with_no_names_is_rejected() {
        let bytes = with_header(&Entry::minimal().bytes(), 1, 0);
        let error = parse_err(&bytes);
        assert!(matches!(error, Error::ShortNames), "got {error:?}");
    }

    #[test]
    fn a_section_length_below_minus_one_is_rejected() {
        // term(5) allows -1, meaning "absent"; anything more negative is
        // corruption. The fields are read as `u16` and reinterpreted, so
        // 0xFFFE is -2.
        for field in 1..=5 {
            let bytes = with_header(&Entry::minimal().bytes(), field, 0xFFFE);
            let error = parse_err(&bytes);
            assert!(
                matches!(error, Error::InvalidLength),
                "header field {field}: got {error:?}"
            );
        }
    }

    #[test]
    fn sections_longer_than_this_crate_understands_are_rejected() {
        let base = Entry::minimal().bytes();
        let too_many = [
            (2, BOOL_NAMES.len(), Error::TooManyBools),
            (3, NUMBER_NAMES.len(), Error::TooManyNumbers),
            (4, STRING_NAMES.len(), Error::TooManyStrings),
        ];
        for (field, known, expected) in too_many {
            let bytes = with_header(&base, field, known as u16 + 1);
            let error = parse_err(&bytes);
            assert_eq!(
                std::mem::discriminant(&error),
                std::mem::discriminant(&expected),
                "header field {field}: got {error:?}"
            );
            // Exactly as many as we know about is still fine to attempt; it
            // fails later on a short read, not on the bound.
            let bytes = with_header(&base, field, known as u16);
            assert!(!matches!(parse_err(&bytes), Error::TooManyBools));
        }
    }

    #[test]
    fn a_names_section_without_its_terminator_is_rejected() {
        // Declaring the section one byte short leaves a name byte where the
        // NUL should be.
        let e = Entry::minimal();
        let bytes = with_header(&e.bytes(), 1, e.names.len() as u16);
        let error = parse_err(&bytes);
        assert!(matches!(error, Error::NamesMissingNull), "got {error:?}");
    }

    #[test]
    fn a_string_table_without_its_terminator_is_rejected() {
        let mut e = Entry::minimal();
        e.offsets = vec![0];
        e.table = b"bel".to_vec();
        let error = parse_err(&e.bytes());
        assert!(matches!(error, Error::StringsMissingNull), "got {error:?}");
    }

    #[test]
    fn names_that_are_not_utf8_are_rejected() {
        let mut e = Entry::minimal();
        e.names = vec![0xFF, 0xFE, 0xFD];
        assert!(matches!(parse_err(&e.bytes()), Error::NotUtf8(_)));
    }

    #[test]
    fn a_truncated_entry_is_an_io_error_rather_than_a_panic() {
        let e = Entry::minimal();
        let full = e.bytes();
        // Every cut before the string table starts; the table's own short
        // read is `a_truncated_string_table_is_an_error_not_a_panic`, below.
        for len in 0..full.len() - e.table.len() {
            match parse_bytes(&full[..len]) {
                Err(Error::Io(_)) => {}
                other => panic!(
                    "truncating to {len} bytes should be an I/O error, got {:?}",
                    other.map(|t| t.names)
                ),
            }
        }
    }

    #[test]
    fn a_truncated_string_table_is_an_error_not_a_panic() {
        // `parse` used to trust the declared `string_table_bytes` when
        // slicing the table it actually read:
        //     string_table[offset..string_table_bytes]
        // `read_to_end` stops at EOF, so a file that ended inside its own
        // string table left that slice out of range and the index panicked.
        // Every other short read in the format is an `Error::Io`; this one
        // took the process down.
        //
        // It matters because the bytes come from a file the caller did
        // not write — `TermInfo::from_env` opens whatever the terminfo
        // search path resolves to — so a corrupt or hostile entry is
        // reachable input, not a hypothetical.
        let e = Entry::minimal();
        let full = e.bytes();
        for len in full.len() - e.table.len()..full.len() {
            assert!(
                parse_bytes(&full[..len]).is_err(),
                "truncating to {len} bytes should be an error"
            );
        }
    }

    #[test]
    fn a_string_offset_past_the_table_is_an_error_not_a_panic() {
        // The same slice, reached the other way: a complete file whose
        // offset table points outside the string table. Nothing used to
        // validate `offset` against the table's length.
        let mut e = Entry::minimal();
        e.offsets = vec![99];
        assert!(parse_bytes(&e.bytes()).is_err());
    }

    // -----------------------------------------------------------------
    // the compiled fixtures in tests/data
    // -----------------------------------------------------------------

    fn fixture_dir() -> PathBuf {
        [env!("CARGO_MANIFEST_DIR"), "tests", "data"]
            .iter()
            .collect()
    }

    fn fixture(name: &str) -> TermInfo {
        TermInfo::from_path(fixture_dir().join(name))
            .unwrap_or_else(|e| panic!("parsing fixture {name}: {e}"))
    }

    #[test]
    fn every_fixture_parses_and_names_itself() {
        let mut count = 0;
        for entry in std::fs::read_dir(fixture_dir()).expect("tests/data is missing") {
            let name = entry.expect("reading tests/data").file_name();
            let name = name.to_string_lossy();
            let term = fixture(&name);
            // ncurses keys the database by the entry's first name, so a
            // mismatch here means the file is filed under the wrong key.
            assert_eq!(term.names[0], name, "fixture {name} names itself wrongly");
            assert!(!term.strings.is_empty(), "fixture {name} has no strings");
            count += 1;
        }
        assert_eq!(count, 30, "expected the 30 fixtures term 1.2.1 shipped");
    }

    #[test]
    fn aliases_after_the_first_name_are_preserved() {
        // The names section is a single `|`-separated field; the last element
        // is the long description, not an alias, and is kept as-is.
        assert_eq!(
            fixture("vt100").names,
            ["vt100", "vt100-am", "dec vt100 (w/advanced video)"]
        );
    }

    #[test]
    fn capability_values_survive_the_string_table() {
        assert_eq!(
            fixture("xterm")
                .string(CapabilityName::Terminfo("cup"))
                .as_deref(),
            Some(&b"\x1b[%i%p1%d;%p2%dH"[..])
        );
        assert_eq!(
            fixture("linux")
                .string(CapabilityName::Terminfo("setaf"))
                .as_deref(),
            Some(&b"\x1b[3%p1%dm"[..])
        );
        // dumb is the degenerate entry: a handful of strings, no parameters.
        assert_eq!(
            fixture("dumb")
                .string(CapabilityName::Terminfo("bel"))
                .as_deref(),
            Some(&b"\x07"[..])
        );
    }

    #[test]
    fn padding_markers_reach_the_caller_unexpanded() {
        // sem:libedit:terminal.tgetent-fn: a capability's stored value "is
        // raw bytes, and it must **not** be parameter-expanded here", and "it
        // still carries its `$<...>` padding markers, which is what `tputs`
        // needs". The parser is where that could be lost, so pin it.
        assert_eq!(
            fixture("vt100")
                .string(CapabilityName::Terminfo("cup"))
                .as_deref(),
            Some(&b"\x1b[%i%p1%d;%p2%dH$<5>"[..])
        );
        assert_eq!(
            fixture("xterm")
                .string(CapabilityName::Terminfo("flash"))
                .as_deref(),
            Some(&b"\x1b[?5h$<100/>\x1b[?5l"[..])
        );
    }

    #[test]
    fn numeric_and_boolean_capabilities_are_read() {
        assert_eq!(
            fixture("linux").number(CapabilityName::Terminfo("colors")),
            Some(8)
        );
        assert_eq!(
            fixture("xterm-256color").number(CapabilityName::Terminfo("colors")),
            Some(256)
        );
        assert_eq!(
            fixture("xterm-256color").number(CapabilityName::Terminfo("pairs")),
            Some(65_536)
        );
        assert_eq!(
            fixture("dumb").number(CapabilityName::Terminfo("cols")),
            Some(80)
        );
        assert_eq!(
            fixture("xterm").boolean(CapabilityName::Terminfo("am")),
            Some(true)
        );
        // `dumb` is not auto-margin and carries no colour count at all.
        assert_eq!(
            fixture("dumb").boolean(CapabilityName::Terminfo("mir")),
            None
        );
        assert_eq!(
            fixture("dumb").number(CapabilityName::Terminfo("colors")),
            None
        );
    }
}
