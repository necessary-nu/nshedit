//! What pins this encoder is measurement, not reading.
//!
//! Two oracles, because neither one alone reaches both locales:
//!
//! - **`bsd::vis`, called in-process.** A byte-for-byte differential over a
//!   corpus built in code, so it cannot drift out of step with the encoder.
//!   It can only speak for the C locale: `bsd::vis` classifies and converts
//!   through the C library, and a Rust process never calls `setlocale`, so its
//!   `LC_CTYPE` is `C` for the life of the test binary. Switching it would be
//!   a process-wide side effect on every test running beside this one — and
//!   `plan/decisions/no-c-ffi.md` bars the core crate from naming the symbol
//!   that would do it.
//! - **A real `strvis`, run out of process.** The UTF-8 answers below were
//!   produced by `strvisx(dst, src, len, VIS_NL)` under `LC_ALL=C.UTF-8` and
//!   copied here verbatim. They are transcribed measurements, not derivations
//!   from the same reading the encoder came from, which is the only reason
//!   they are worth asserting.
//!
//! Both oracles are libbsd's lineage rather than this tree's `src/vis.c`, so
//! the two have to agree for either to count. Over the corpus in
//! `conformance/aux/vis_corpus.c` and over each of the 256 byte values alone,
//! in both locales, they do — and `conformance/vis-cross.sh` is what keeps
//! checking it.
//!
//! Between them every byte value is covered in both charsets: `0x00`..=`0x9F`
//! encode identically under `C` and `C.UTF-8` and are differentially checked,
//! and the `0xA0`..=`0xFF` range where the two disagree is pinned on both
//! sides.

use super::*;

/// Inputs the encoder has to survive, built rather than listed so that adding
/// a case cannot silently skip one: every single byte alone, each byte framed
/// by graphic characters, each byte as a run, the whole byte space in one
/// string, the empty string, embedded NULs, control runs, well-formed
/// multi-byte UTF-8 out to a non-BMP character and glibc's five-byte
/// extension, and byte sequences `mbrtowc` rejects.
fn corpus() -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];

    for b in 0..=u8::MAX {
        out.push(vec![b]);
        // Framed, because a character's own encoding must not depend on what
        // surrounds it — except through the latch, which the invalid
        // sequences below are what exercise.
        out.push(vec![b'a', b, b'z']);
        out.push(vec![b, b, b]);
    }
    out.push((0..=u8::MAX).collect());

    // Control runs, whole and interleaved.
    out.push((0x00..=0x1f).collect());
    out.push((0x7f..=0x9f).collect());
    out.push(b"ctrl\x01\x02\x1f\x7fend".to_vec());

    // NULs, which are their own case: the C's extra-list search finds one
    // whether or not the caller asked for it.
    out.push(b"nul\0here".to_vec());
    out.push(vec![0, 0, 0]);
    out.push(b"\0lead".to_vec());
    out.push(b"trail\0".to_vec());

    // Well-formed multi-byte UTF-8: two, three and four byte forms, the
    // non-BMP emoji, and the five-byte form only glibc's converter accepts.
    out.push("café".as_bytes().to_vec());
    out.push("中文".as_bytes().to_vec());
    out.push("😀 grin".as_bytes().to_vec());
    out.push("\u{a0}nbsp".as_bytes().to_vec());
    out.push(b"\xf8\x88\x80\x80\x80".to_vec());

    // Rejected by `mbrtowc`: a lone 0xFF, a truncated sequence, a stray
    // continuation byte, an overlong NUL, and a surrogate encoding. The
    // fourth is the latch case — good UTF-8 after a bad byte is *not*
    // re-decoded.
    out.push(b"\xff\xfe".to_vec());
    out.push(b"caf\xc3".to_vec());
    out.push(b"a\xc3b".to_vec());
    out.push(b"caf\xff\xc3\xa9".to_vec());
    out.push(b"\x80\x9f".to_vec());
    out.push(b"\xc0\x80".to_vec());
    out.push(b"\xed\xa0\x80".to_vec());

    // Shapes a history entry actually takes.
    out.push(b"git commit -m 'fix it'".to_vec());
    out.push(b"printf 'two\nlines'".to_vec());
    out.push(b"grep '\\\\' file".to_vec());
    out.push(b"trailing space ".to_vec());

    out
}

fn show(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) && b != b'\\' {
                (b as char).to_string()
            } else {
                format!("\\x{b:02x}")
            }
        })
        .collect()
}

/// Twenty bytes either side of `at`, so that a disagreement inside the
/// all-256-bytes input reports the disagreement rather than the input.
fn around(bytes: &[u8], at: usize) -> String {
    let lo = at.saturating_sub(20);
    let hi = (at + 20).min(bytes.len());
    format!(
        "{}{}{}",
        if lo > 0 { "…" } else { "" },
        show(&bytes[lo..hi]),
        if hi < bytes.len() { "…" } else { "" }
    )
}

/// The C-locale arm against `bsd::vis`, byte for byte over the whole corpus.
///
/// This is the check the encoder exists to pass: it was written by reading
/// `src/vis.c`, and reading is a guess until something measures it.
#[test]
#[cfg(feature = "bsd")]
fn the_c_locale_arm_matches_bsd_vis_byte_for_byte() {
    let oracle = bsd::vis::Encoder::new(bsd::vis::Flags::NL);

    // The comparison is only meaningful while the C library is in the C
    // locale, which is where a Rust process starts and stays. If something
    // ever calls `setlocale`, this says so rather than reporting a wrong
    // encoder.
    assert_eq!(
        oracle.encode("café".as_bytes()),
        b"caf\\M-C\\M-)",
        "bsd::vis is not answering in the C locale, so it cannot be compared \
         against the Charset::Ascii arm"
    );

    let mut wrong = Vec::new();
    for src in corpus() {
        let ours = encode_nl_in(Charset::Ascii, &src);
        let theirs = oracle.encode(&src);
        if ours != theirs {
            let at = ours
                .iter()
                .zip(&theirs)
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| ours.len().min(theirs.len()));
            wrong.push(format!(
                "\n  in   ={}\n  ours ={}\n  bsd  ={}\n  (first difference at output byte {at})",
                around(&src, at),
                around(&ours, at),
                around(&theirs, at),
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} inputs disagree with bsd::vis:{}",
        wrong.len(),
        corpus().len(),
        wrong.concat()
    );
}

/// Measured `strvis` output under `LC_ALL=C.UTF-8`, transcribed.
///
/// The UTF-8 arm's whole content is here: what decodes, what does not, what
/// the latch does to good bytes that follow a bad one, and which high bytes
/// glibc calls graphic.
const UTF8_GOLDEN: &[(&[u8], &[u8])] = &[
    (b"", b""),
    (b"plain text", b"plain text"),
    (b"with space", b"with space"),
    (b"tab\there", b"tab\there"),
    (b"newline\nhere", b"newline\\012here"),
    (b"back\\slash", b"back\\134slash"),
    (b"nul\0here", b"nul\\000here"),
    (b"\0\0\0", b"\\000\\000\\000"),
    (
        b"\x01\x02\x03\x04\x05\x06\x07\x08\x0b\x0c\x0e\x0f",
        b"\\^A\\^B\\^C\\^D\\^E\\^F\\^G\\^H\\^K\\^L\\^N\\^O",
    ),
    (b"\x7fdel", b"\\^?del"),
    // Decoded, graphic, and re-encoded to the bytes it came from.
    (b"caf\xc3\xa9", b"caf\xc3\xa9"),
    (b"\xe4\xb8\xad\xe6\x96\x87", b"\xe4\xb8\xad\xe6\x96\x87"),
    (b"\xf0\x9f\x98\x80", b"\xf0\x9f\x98\x80"),
    (b"\xc2\xa0nbsp", b"\xc2\xa0nbsp"),
    // glibc decodes this five-byte form to U+200000, which is above the
    // Unicode range and so not graphic. Its three significant bytes are
    // 0x20, 0x00, 0x00 — and the first takes the octal branch because
    // `(c & 0177) == ' '`.
    (b"\xf8\x88\x80\x80\x80", b"\\040\\^@\\^@"),
    // Not decodable, so each byte becomes its own character — and U+00FF and
    // U+00FE are graphic, so they pass straight back out.
    (b"\xff\xfe", b"\xff\xfe"),
    (b"\xc3", b"\xc3"),
    (b"a\xc3b", b"a\xc3b"),
    // The latch: 0xFF fails, and the well-formed é after it is never offered
    // to the converter again.
    (b"caf\xff\xc3\xa9", b"caf\xff\xc3\xa9"),
    (b"\x80\x9f", b"\\M^@\\M^_"),
    (b"\xc0\x80", b"\xc0\\M^@"),
    (b"\xed\xa0\x80", b"\xed\xa0\\M^@"),
    (b" ", b" "),
    (b"\xa0", b"\xa0"),
    (b"a b\tc\nd\\e", b"a b\tc\\012d\\134e"),
];

/// The UTF-8 arm against a real `strvis` run in a UTF-8 locale.
#[test]
fn the_utf8_arm_matches_a_measured_strvis() {
    for &(src, want) in UTF8_GOLDEN {
        assert_eq!(
            encode_nl_in(Charset::Utf8, src),
            want,
            "in={} want={} got={}",
            show(src),
            show(want),
            show(&encode_nl_in(Charset::Utf8, src))
        );
    }
}

/// Every byte on its own in a UTF-8 locale, as two facts read off the
/// `LC_ALL=C.UTF-8` sweep.
///
/// Below `0xA0` the two charsets answer identically, which is what lets the
/// C-locale differential stand for both; from `0xA0` up every byte is graphic
/// Latin-1 to glibc and passes through untouched, which is exactly the range
/// where the C locale escapes it instead.
#[test]
fn every_byte_alone_in_a_utf8_locale() {
    for b in 0..=0x9fu8 {
        assert_eq!(
            encode_nl_in(Charset::Utf8, &[b]),
            encode_nl_in(Charset::Ascii, &[b]),
            "byte {b:#04x} should encode alike in both charsets"
        );
    }
    for b in 0xa0..=0xffu8 {
        assert_eq!(
            encode_nl_in(Charset::Utf8, &[b]),
            vec![b],
            "byte {b:#04x} is graphic to glibc in a UTF-8 locale"
        );
    }
}

/// The three escapes a reader of `VIS_NL` is most likely to predict wrongly.
///
/// `VIS_NL` puts the newline in the *extra* list rather than giving it a
/// spelling, so it takes the octal branch; `\n` would need `VIS_CSTYLE`, which
/// `hist_command` does not pass. The backslash is there for the same reason.
/// The NUL is there because the C's `wcschr` finds the extra list's own
/// terminator.
#[test]
fn the_three_octal_escapes_are_octal_and_not_lettered() {
    for cs in [Charset::Ascii, Charset::Utf8] {
        assert_eq!(encode_nl_in(cs, b"\n"), b"\\012");
        assert_eq!(encode_nl_in(cs, b"\\"), b"\\134");
        assert_eq!(encode_nl_in(cs, b"\0"), b"\\000");
        // And what `VIS_NL` deliberately leaves alone: this is a listing for a
        // person to read, not the on-disk format, so a space stays a space.
        assert_eq!(encode_nl_in(cs, b"a b\tc"), b"a b\tc");
    }
}

/// No output of this encoder can contain a newline, which is the one property
/// the listing depends on: `hist_command` prints one entry per line.
#[test]
fn nothing_encodes_to_a_newline() {
    for cs in [Charset::Ascii, Charset::Utf8] {
        for src in corpus() {
            let out = encode_nl_in(cs, &src);
            assert!(
                !out.contains(&b'\n'),
                "a newline survived: in={} out={}",
                show(&src),
                show(&out)
            );
        }
    }
}
