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

use crate::Error::*;
use crate::NameTable;
use crate::Result;
use crate::TermInfo;

pub use crate::parser::names::*;

// These are the orders ncurses uses in its compiled format (as of 5.9). Not
// sure if portable.

fn read_le_u16(r: &mut dyn io::Read) -> io::Result<u32> {
    let mut buf = [0; 2];
    r.read_exact(&mut buf)
        .map(|()| u32::from(u16::from_le_bytes(buf)))
}

fn read_le_u32(r: &mut dyn io::Read) -> io::Result<u32> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf).map(|()| u32::from_le_bytes(buf))
}

fn read_byte(r: &mut dyn io::Read) -> io::Result<u8> {
    let mut byte = [0u8; 1];
    match r.read(&mut byte)? {
        0 => Err(io::Error::other("end of file")),
        _ => Ok(byte[0]),
    }
}

// [spec:nshedit:req:terminal.typed-api]
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
        0x011A => read_le_u16,
        0x021e => read_le_u32,
        _ => return Err(BadMagic(magic)),
    };

    // According to the spec, these fields must be >= -1 where -1 means that the
    // feature is not
    // supported. Using 0 instead of -1 works because we skip sections with length
    // 0.
    macro_rules! read_nonneg {
        () => {{
            match read_le_u16(file)? as i16 {
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
        .filter_map(|i| match read_byte(file) {
            Err(e) => Some(Err(e)),
            Ok(1) => Some(Ok((bnames[i], true))),
            Ok(_) => None,
        })
        .collect::<io::Result<HashMap<_, _>>>()?;

    if (bools_bytes + names_bytes) % 2 == 1 {
        read_byte(file)?; // compensate for padding
    }

    let numbers_map = (0..numbers_count)
        .filter_map(|i| match read_number(file) {
            Ok(0xFFFF) => None,
            Ok(n) => Some(Ok((nnames[i], n))),
            Err(e) => Some(Err(e)),
        })
        .collect::<io::Result<HashMap<_, _>>>()?;

    let string_map: HashMap<&str, Vec<u8>> = if string_offsets_count > 0 {
        let string_offsets = (0..string_offsets_count)
            .map(|_| {
                let mut buf = [0; 2];
                file.read_exact(&mut buf).map(|()| u16::from_le_bytes(buf))
            })
            .collect::<io::Result<Vec<_>>>()?;

        // `read_exact` rather than `take(..).read_to_end(..)`: a file that
        // ends inside its own string table has to fail here, the same way
        // every other short read in this format does. Reading what is there
        // and trusting the declared length instead left the slices below out
        // of range. The length is a `u16` read through `read_nonneg!`, so
        // the allocation is bounded by 32767 bytes.
        let mut string_table = vec![0; string_table_bytes];
        file.read_exact(&mut string_table)?;

        string_offsets
            .into_iter()
            .enumerate()
            .filter(|&(_, offset)| {
                // non-entry
                offset != 0xFFFF
            })
            .map(|(i, offset)| {
                let offset = offset as usize;

                let name = if snames[i] == "_" {
                    STRING_LONG_NAMES[i]
                } else {
                    snames[i]
                };

                if offset == 0xFFFE {
                    // undocumented: FFFE indicates cap@, which means the capability
                    // is not present
                    // unsure if the handling for this is correct
                    return Ok((name, Vec::new()));
                }

                // The offset is a claim the file makes about itself, and
                // nothing above has measured it against the table that is
                // actually there; indexing on trust panicked.
                let tail = string_table.get(offset..).ok_or(StringOffsetOutOfRange)?;

                // Find the offset of the NUL we want to go to
                match tail.iter().position(|&b| b == 0) {
                    Some(len) => Ok((name, tail[..len].to_vec())),
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
        BOOL_LONG_NAMES, BOOL_NAMES, NUMBER_LONG_NAMES, NUMBER_NAMES, STRING_LONG_NAMES,
        STRING_NAMES, parse,
    };
    use crate::{Error, Result, TermInfo};

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
        numbers: Vec<u16>,
        offsets: Vec<u16>,
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
                magic: 0x011A,
                names: b"nsh|nshterm test entry".to_vec(),
                bools: vec![0, 1],
                numbers: vec![80],
                offsets: vec![0xFFFF, 0],
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
                v.extend(n.to_le_bytes());
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
        assert_eq!(term.numbers["cols"], 80);
        assert_eq!(term.strings["bel"], b"\x07");
        // A boolean stored as 0 is absent, not present-and-false; callers
        // read these with `get(..).is_some()`.
        assert_eq!(term.bools.get("am"), Some(&true));
        assert_eq!(term.bools.get("bw"), None);
    }

    #[test]
    fn the_pad_byte_after_the_booleans_is_honoured() {
        // One fewer name byte flips the parity, so the parser has to consume
        // the alignment byte or every number afterwards is shifted by one.
        let mut e = Entry::minimal();
        e.names.pop();
        let term = parse_bytes(&e.bytes()).expect("odd-length entry should parse");
        assert_eq!(term.numbers["cols"], 80);
        assert_eq!(term.strings["bel"], b"\x07");
    }

    #[test]
    fn the_32_bit_number_format_is_accepted() {
        // Magic 0x021e widens the number section to `u32`, for `cps` and the
        // other capabilities that outgrew 16 bits.
        let e = Entry::minimal();
        let mut bytes = with_header(&e.bytes(), 0, 0x021e);
        // Widen the single number in place.
        let numbers_at = bytes.len() - (e.numbers.len() * 2 + e.offsets.len() * 2 + e.table.len());
        bytes.splice(numbers_at + 2..numbers_at + 2, [0, 0]);
        let term = parse_bytes(&bytes).expect("32-bit entry should parse");
        assert_eq!(term.numbers["cols"], 80);
    }

    #[test]
    fn absent_capabilities_stay_out_of_the_maps() {
        let mut e = Entry::minimal();
        // 0xFFFF is "not in this entry" for both numbers and strings.
        e.numbers = vec![0xFFFF];
        e.offsets = vec![0xFFFF, 0xFFFF];
        let term = parse_bytes(&e.bytes()).expect("entry should parse");
        assert!(term.numbers.is_empty());
        assert!(term.strings.is_empty());
    }

    #[test]
    fn a_cancelled_capability_reads_as_an_empty_string() {
        // Offset 0xFFFE is terminfo's `cap@` — the capability is explicitly
        // cancelled rather than merely missing.
        let mut e = Entry::minimal();
        e.offsets = vec![0xFFFE, 0];
        let term = parse_bytes(&e.bytes()).expect("entry should parse");
        assert_eq!(term.strings["cbt"], b"");
        assert_eq!(term.strings["bel"], b"\x07");
    }

    #[test]
    fn a_wrong_magic_number_is_rejected() {
        let bytes = with_header(&Entry::minimal().bytes(), 0, 0x1234);
        assert_eq!(parse_err(&bytes), Error::BadMagic(0x1234));
    }

    #[test]
    fn an_entry_with_no_names_is_rejected() {
        let bytes = with_header(&Entry::minimal().bytes(), 1, 0);
        assert_eq!(parse_err(&bytes), Error::ShortNames);
    }

    #[test]
    fn a_section_length_below_minus_one_is_rejected() {
        // term(5) allows -1, meaning "absent"; anything more negative is
        // corruption. The fields are read as `u16` and reinterpreted, so
        // 0xFFFE is -2.
        for field in 1..=5 {
            let bytes = with_header(&Entry::minimal().bytes(), field, 0xFFFE);
            assert_eq!(
                parse_err(&bytes),
                Error::InvalidLength,
                "header field {field}"
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
            assert_eq!(parse_err(&bytes), expected, "header field {field}");
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
        assert_eq!(parse_err(&bytes), Error::NamesMissingNull);
    }

    #[test]
    fn a_string_table_without_its_terminator_is_rejected() {
        let mut e = Entry::minimal();
        e.offsets = vec![0];
        e.table = b"bel".to_vec();
        assert_eq!(parse_err(&e.bytes()), Error::StringsMissingNull);
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
        assert_eq!(fixture("xterm").strings["cup"], b"\x1b[%i%p1%d;%p2%dH");
        assert_eq!(fixture("linux").strings["setaf"], b"\x1b[3%p1%dm");
        // dumb is the degenerate entry: a handful of strings, no parameters.
        assert_eq!(fixture("dumb").strings["bel"], b"\x07");
    }

    #[test]
    fn padding_markers_reach_the_caller_unexpanded() {
        // sem:libedit:terminal.tgetent-fn: a capability's stored value "is
        // raw bytes, and it must **not** be parameter-expanded here", and "it
        // still carries its `$<...>` padding markers, which is what `tputs`
        // needs". The parser is where that could be lost, so pin it.
        assert_eq!(fixture("vt100").strings["cup"], b"\x1b[%i%p1%d;%p2%dH$<5>");
        assert_eq!(
            fixture("xterm").strings["flash"],
            b"\x1b[?5h$<100/>\x1b[?5l"
        );
    }

    #[test]
    fn numeric_and_boolean_capabilities_are_read() {
        assert_eq!(fixture("linux").numbers["colors"], 8);
        assert_eq!(fixture("xterm-256color").numbers["colors"], 256);
        assert_eq!(fixture("xterm-256color").numbers["pairs"], 65536);
        assert_eq!(fixture("dumb").numbers["cols"], 80);
        assert_eq!(fixture("xterm").bools.get("am"), Some(&true));
        // `dumb` is not auto-margin and carries no colour count at all.
        assert_eq!(fixture("dumb").bools.get("mir"), None);
        assert_eq!(fixture("dumb").numbers.get("colors"), None);
    }
}
