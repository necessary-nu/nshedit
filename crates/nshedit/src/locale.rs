//! `LC_CTYPE` without libc: the locale queries the C makes through
//! `<wchar.h>` and `<wctype.h>`, reimplemented.
//!
//! This module has no C counterpart. `plan/decisions/no-c-ffi.md` bars linking
//! libc, so every `mbstowcs`, `mbrtowc`, `wcrtomb`, `wctomb`, `iswcntrl`,
//! `iswprint`, `iswgraph`, `iswalnum`, `iswspace` and `wcwidth` in the C has to
//! be supplied here. It is the one place they live: `chartype`, `literal`,
//! `vis`, `refresh`, `map` and `search` all reach for the same predicates, and
//! two independent copies of this module existed before it was hoisted (one in
//! `chartype`, one in `vis`), which is exactly the drift it exists to prevent.
//!
//! # What is modelled
//!
//! - **Two charsets**, [`Charset::Utf8`] and [`Charset::Ascii`]. `Ascii` is the
//!   C/POSIX locale, whose charmap on glibc is ANSI_X3.4-1968: nothing above
//!   U+007F encodes, and `iswprint` is true only for U+0020..U+007E, exactly as
//!   `sem:chartype.ct-chr-class-fn` describes. Any other named codeset
//!   (ISO-8859-1, EUC-JP, the stateful ISO-2022 family) falls back to `Ascii`,
//!   which renders the affected characters `\U+nnnn` rather than mis-encoding
//!   them. **Consequence: no stateful encoding exists in this port**, which is
//!   what makes `ERR-encoding-02`, `ERR-encoding-12` and `ERR-encoding-16`
//!   unreachable here, and what lets [`encode`] be reentrant and need no reset
//!   on the failure path.
//! - **Which locale.** There is no `setlocale` to consult, so the charset comes
//!   from `LC_ALL`, then `LC_CTYPE`, then `LANG`, in POSIX order, and C/POSIX
//!   if none is set — i.e. the port behaves as if the program had called
//!   `setlocale(LC_ALL, "")`, which every interactive libedit consumer does. A
//!   C program that never calls `setlocale` stays in the C locale no matter
//!   what the environment says, and `sem:el.el-init-fn` depends on that; the
//!   difference is not observable from here.
//! - **The charset is a per-call snapshot.** [`charset`] caches in a
//!   thread-local so a per-character query costs no environment lookup, and
//!   [`refresh`] re-reads. `vis`'s public entry points call `refresh` on the way
//!   in, which is what gives them the C's "one `LC_CTYPE` for the duration of
//!   one call" and what lets a test exercise both locales in one process. That
//!   per-call read is load-bearing and must not be collapsed back into a
//!   resolve-once.
//!
//! # The UTF-8 codec is glibc's, not Unicode's
//!
//! glibc's UTF-8 converter implements the *original* encoding, not the range
//! Unicode later fixed: measured on glibc 2.41, `mbrtowc` accepts five- and
//! six-byte sequences up to U+7FFFFFFF and `wcrtomb` encodes them back,
//! rejecting only overlong forms and surrogates, and `MB_CUR_MAX` is 6 rather
//! than 4. [`mbrtowc`] and [`wcrtomb`] reproduce that range, and everything
//! else here is derived from them so the two can never disagree.
//!
//! This is observable and was measured, not assumed: `strvisx(dst,
//! "\xf8\x88\x80\x80\x80", 5, 0)` is **one** character in the C, not five
//! invalid bytes, and a codec capped at U+10FFFF silently changes what `vis`
//! encodes. The consequence for the conversion layer is that `ct_encode_char`
//! can now be handed a character needing six bytes where the C's own
//! `ct_encode_string` passes five, which is `ERR-encoding-12` becoming
//! reachable — see the call site.
//!
//! # What is approximate
//!
//! Two limitations are shared by every predicate below and are not fixable
//! here:
//!
//! - **Unassigned code points.** glibc's `graph` and `print` classes are
//!   restricted to assigned code points, and Rust's standard library exposes no
//!   general-category table, so U+0378 and friends are reported graphic and
//!   printable here and are not by glibc. This is the only divergence a
//!   differential run against the compiled C finds in a UTF-8 locale. Closing
//!   it needs a generated Unicode table, which is a dependency decision and not
//!   a translation one.
//! - **`wcwidth` is table-driven.** [`ZERO_WIDTH`] and [`WIDE`] cover combining
//!   marks, format characters and the East Asian wide blocks; they are not a
//!   Unicode database, and they predate several emoji blocks that a current
//!   glibc reports double-width. Left as it is deliberately: widening a column
//!   table changes rendered geometry everywhere in `refresh`, so it belongs
//!   with a generated table, not with this consolidation.

use std::cell::Cell;
use std::cmp::Ordering;

/// The subset of `LC_CTYPE` codesets this port implements.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Charset {
    /// The C/POSIX locale: ANSI_X3.4-1968, one byte, ASCII only.
    Ascii,
    /// glibc's UTF-8: the full original encoding, up to six bytes and 31 bits,
    /// rejecting overlong forms and surrogates.
    Utf8,
}

thread_local! {
    static CHARSET: Cell<Option<Charset>> = const { Cell::new(None) };
}

/// Re-reads the environment.
///
/// Called once per public entry point in `vis`, so the charset is a snapshot
/// for the duration of one call exactly as the C's `LC_CTYPE` is, without an
/// environment lookup per character.
pub(crate) fn refresh() {
    CHARSET.with(|c| c.set(Some(from_env())));
}

/// The active charset: the cached snapshot, or a fresh read if this thread has
/// not looked yet. Pass the result down explicitly rather than re-querying per
/// character, so that a locale query is visible as one.
pub(crate) fn charset() -> Charset {
    CHARSET.with(|c| match c.get() {
        Some(cs) => cs,
        None => {
            let cs = from_env();
            c.set(Some(cs));
            cs
        }
    })
}

fn from_env() -> Charset {
    for key in ["LC_ALL", "LC_CTYPE", "LANG"] {
        match std::env::var_os(key) {
            Some(v) if !v.is_empty() => return charset_of(v.as_encoded_bytes()),
            _ => {}
        }
    }
    Charset::Ascii
}

/// Parses a POSIX locale name — `language[_TERRITORY][.codeset][@modifier]` —
/// for its codeset.
///
/// Only UTF-8 is distinguished from the C locale; anything else is treated as
/// C/POSIX, which renders the affected characters `\U+nnnn` rather than
/// mis-encoding them. Comparison ignores case and the `-`/`_` that separate
/// "UTF-8" from "utf8".
pub(crate) fn charset_of(name: &[u8]) -> Charset {
    let name = match name.iter().position(|&b| b == b'@') {
        Some(i) => &name[..i],
        None => name,
    };
    let codeset = match name.iter().position(|&b| b == b'.') {
        Some(i) => &name[i + 1..],
        None => return Charset::Ascii,
    };
    let mut squashed = [0u8; 8];
    let mut n = 0;
    for &b in codeset {
        if b == b'-' || b == b'_' {
            continue;
        }
        if n == squashed.len() {
            return Charset::Ascii;
        }
        squashed[n] = b.to_ascii_uppercase();
        n += 1;
    }
    if &squashed[..n] == b"UTF8" {
        Charset::Utf8
    } else {
        Charset::Ascii
    }
}

/// C: `MB_LEN_MAX`. 16 on glibc — the size of `istrsenvisx`'s scratch buffer,
/// the cap on the `mbrtowc` input window, the per-character budget in `vis`'s
/// unbounded output bound, and `literal`'s encode scratch.
pub(crate) const MB_LEN_MAX: usize = 16;

/// C: `MB_CUR_MAX`. glibc reports 6 in a UTF-8 locale, 1 in the C locale.
///
/// `el.c` and `hist.c` test it against 1 to decide whether history can be
/// narrow; `vis`'s output loop uses it to decide whether a conversion has room
/// to go straight into the destination.
pub(crate) fn mb_cur_max(cs: Charset) -> usize {
    match cs {
        Charset::Ascii => 1,
        Charset::Utf8 => 6,
    }
}

/// What `mbrtowc` reports.
///
/// `(size_t)-1` (EILSEQ) and `(size_t)-2` (incomplete) are one variant: every
/// caller tests `clen < 0` and treats them alike.
pub(crate) enum Mb {
    /// A whole character and the bytes it consumed. A NUL is `Char(0, 1)`; the
    /// C's 0 return is normalised by the caller.
    Char(u32, usize),
    /// Invalid or incomplete.
    Bad,
}

/// C: `mbrtowc(&wc, bytes, bytes.len(), &state)` from a zeroed conversion
/// state.
///
/// The permissive glibc range: five- and six-byte sequences decode, up to
/// U+7FFFFFFF. Overlong forms, surrogate encodings, stray continuation bytes,
/// `0xFE`/`0xFF` and truncated sequences are all EILSEQ.
pub(crate) fn mbrtowc(cs: Charset, bytes: &[u8]) -> Mb {
    let Some(&b0) = bytes.first() else {
        return Mb::Bad;
    };
    match cs {
        Charset::Ascii => {
            if b0 < 0x80 {
                Mb::Char(u32::from(b0), 1)
            } else {
                Mb::Bad
            }
        }
        Charset::Utf8 => {
            let (len, mut value, min) = match b0 {
                0x00..=0x7f => return Mb::Char(u32::from(b0), 1),
                0xc2..=0xdf => (2usize, u32::from(b0 & 0x1f), 0x80u32),
                0xe0..=0xef => (3, u32::from(b0 & 0x0f), 0x800),
                0xf0..=0xf7 => (4, u32::from(b0 & 0x07), 0x1_0000),
                0xf8..=0xfb => (5, u32::from(b0 & 0x03), 0x20_0000),
                0xfc..=0xfd => (6, u32::from(b0 & 0x01), 0x400_0000),
                // 0x80..=0xbf is a stray continuation byte, 0xc0/0xc1 are
                // overlong two-byte forms, 0xfe/0xff are not UTF-8.
                _ => return Mb::Bad,
            };
            for i in 1..len {
                let Some(&b) = bytes.get(i) else {
                    return Mb::Bad;
                };
                if !(0x80..=0xbf).contains(&b) {
                    return Mb::Bad;
                }
                value = (value << 6) | u32::from(b & 0x3f);
            }
            if value < min || (0xd800..=0xdfff).contains(&value) {
                return Mb::Bad;
            }
            Mb::Char(value, len)
        }
    }
}

/// C: `wcrtomb(out, c, &state)` from a zeroed conversion state. `None` is its
/// `(size_t)-1`/EILSEQ return.
///
/// The one encoder in the crate: [`enc_width`] and [`encode`] are both defined
/// in terms of it, so the measured width and the written bytes cannot disagree
/// (`ERR-encoding-02`).
pub(crate) fn wcrtomb(cs: Charset, c: u32, out: &mut [u8; MB_LEN_MAX]) -> Option<usize> {
    match cs {
        Charset::Ascii => {
            if c < 0x80 {
                out[0] = c as u8;
                Some(1)
            } else {
                None
            }
        }
        Charset::Utf8 => {
            if (0xd800..=0xdfff).contains(&c) || c > 0x7fff_ffff {
                return None;
            }
            let (len, lead_mask) = match c {
                0x0000_0000..=0x0000_007f => {
                    out[0] = c as u8;
                    return Some(1);
                }
                0x0000_0080..=0x0000_07ff => (2usize, 0xc0u8),
                0x0000_0800..=0x0000_ffff => (3, 0xe0),
                0x0001_0000..=0x001f_ffff => (4, 0xf0),
                0x0020_0000..=0x03ff_ffff => (5, 0xf8),
                _ => (6, 0xfc),
            };
            for i in (1..len).rev() {
                out[i] = 0x80 | ((c >> (6 * (len - 1 - i))) & 0x3f) as u8;
            }
            out[0] = lead_mask | (c >> (6 * (len - 1))) as u8;
            Some(len)
        }
    }
}

/// C: `wcrtomb` from the initial conversion state, length only. `None` is its
/// `(size_t)-1`/`EILSEQ`.
///
/// `c == 0` answers 1 — `wcrtomb` writes the null byte — so this is not a
/// string-length primitive.
pub(crate) fn enc_width(cs: Charset, c: u32) -> Option<usize> {
    let mut scratch = [0u8; MB_LEN_MAX];
    wcrtomb(cs, c, &mut scratch)
}

/// C: `wctomb`. Writes `c` into `dst` and returns the byte count; `None` for a
/// character the charset cannot represent, or for a `dst` too short to hold it.
///
/// Unlike `wctomb` this carries no state, so it is reentrant and needs no reset
/// on the failure path (`ERR-encoding-16`).
pub(crate) fn encode(cs: Charset, dst: &mut [u8], c: u32) -> Option<usize> {
    let mut scratch = [0u8; MB_LEN_MAX];
    let n = wcrtomb(cs, c, &mut scratch)?;
    if dst.len() < n {
        return None;
    }
    dst[..n].copy_from_slice(&scratch[..n]);
    Some(n)
}

/// C: `mbstowcs(NULL, s, 0)` — how many wide characters `s` would produce, not
/// counting the terminator. `None` is its `(size_t)-1`.
///
/// An invalid or incomplete sequence *anywhere* rejects the whole string: no
/// partial count, no replacement character.
pub(crate) fn mbstowcs_len(cs: Charset, s: &[u8]) -> Option<usize> {
    let mut at = 0usize;
    let mut count = 0usize;
    while at < s.len() {
        let Mb::Char(_, used) = mbrtowc(cs, &s[at..]) else {
            return None;
        };
        at += used;
        count += 1;
    }
    Some(count)
}

/// C: `mbstowcs(dst, s, dst.len())`. `s` is the source without its terminator;
/// the `L'\0'` is written when it fits, as the C's `n`-limited form does.
/// Returns the count of wide characters written, terminator excluded; `None` is
/// its `(size_t)-1`.
pub(crate) fn mbstowcs(cs: Charset, dst: &mut [u32], s: &[u8]) -> Option<usize> {
    let mut at = 0usize;
    let mut count = 0usize;
    while at < s.len() && count < dst.len() {
        let Mb::Char(c, used) = mbrtowc(cs, &s[at..]) else {
            return None;
        };
        dst[count] = c;
        at += used;
        count += 1;
    }
    if count < dst.len() {
        dst[count] = 0;
    }
    Some(count)
}

/// C: `iswcntrl`. glibc's `cntrl` class: the C0 and C1 controls, plus the line
/// and paragraph separators in a UTF-8 locale.
///
/// Total over `u32`: values that are not code points answer false rather than
/// being undefined (`ERR-encoding-01`).
pub(crate) fn iswcntrl(cs: Charset, c: u32) -> bool {
    match cs {
        Charset::Ascii => c < 0x20 || c == 0x7F,
        // The `Zl` and `Zp` separators are above U+00FF, so
        // `ct_chr_class`'s `c < 0x100` guard hides them; they are here for the
        // other callers of this predicate.
        Charset::Utf8 => c < 0x20 || (0x7F..=0x9F).contains(&c) || c == 0x2028 || c == 0x2029,
    }
}

/// C: `iswspace`. glibc's `space` class.
///
/// Note what is *not* here: U+0085, U+00A0, U+2007 and U+202F are whitespace to
/// Unicode but not to glibc, and `sem:vis.do-mvis-fn`'s
/// trailing-whitespace test and [`iswgraph`] both turn on the difference — the
/// non-breaking spaces are *graphic* to glibc and pass through `strvis`
/// untouched.
pub(crate) fn iswspace(cs: Charset, c: u32) -> bool {
    match cs {
        Charset::Ascii => (0x09..=0x0d).contains(&c) || c == 0x20,
        Charset::Utf8 => {
            (0x09..=0x0d).contains(&c)
                || c == 0x20
                || c == 0x1680
                || (0x2000..=0x2006).contains(&c)
                || (0x2008..=0x200a).contains(&c)
                || c == 0x2028
                || c == 0x2029
                || c == 0x205f
                || c == 0x3000
        }
    }
}

/// C: `iswgraph`. glibc's UTF-8 `graph` class is every assigned code point that
/// is in neither its `cntrl` class nor its `space` class; both of those are
/// reproduced exactly.
///
/// "Assigned" is the part that cannot be — see the module documentation:
/// unassigned code points are reported graphic here.
pub(crate) fn iswgraph(cs: Charset, c: u32) -> bool {
    match cs {
        Charset::Ascii => (0x21..=0x7e).contains(&c),
        Charset::Utf8 => {
            // Surrogates and anything above U+10FFFF are unassigned, and glibc
            // answers false for them; `char::from_u32` is that test.
            if char::from_u32(c).is_none() {
                return false;
            }
            // Noncharacters are unassigned too. The rest of the unassigned
            // range is not detectable without a general category table.
            if (0xfdd0..=0xfdef).contains(&c) || (c & 0xfffe) == 0xfffe {
                return false;
            }
            !iswcntrl(cs, c) && !iswspace(cs, c)
        }
    }
}

/// C: `iswalnum`.
///
/// glibc's `alnum` is its `alpha` class plus its `digit` class, and `digit` is
/// only ASCII `0`-`9` in every locale, while `alpha` is the Unicode Alphabetic
/// property *plus* the non-ASCII decimal digits. Plain `is_alphanumeric` is the
/// wrong shape for it below U+0100 — that would make the Latin-1 fractions and
/// superscripts (`½ ¼ ¾ ¹ ² ³`, category No) alphanumeric, which glibc does
/// not, and those are exactly the values `vis`'s conversion-error fallback
/// produces from raw bytes. Measured against glibc over the whole code space,
/// this form answers true for every code point glibc calls alphanumeric, and
/// additionally for the numeric-but-not-decimal characters above U+00FF.
pub(crate) fn iswalnum(cs: Charset, c: u32) -> bool {
    match cs {
        Charset::Ascii => c < 0x80 && (c as u8).is_ascii_alphanumeric(),
        Charset::Utf8 => char::from_u32(c).is_some_and(|ch| {
            ch.is_alphabetic() || ch.is_ascii_digit() || (c >= 0x100 && ch.is_numeric())
        }),
    }
}

/// C: `iswprint`.
///
/// The C locale answer is exact: everything above U+007E is unprintable, so
/// every non-ASCII character renders as `\U+nnnn`. The UTF-8 answer is glibc's
/// rule — printable is anything with a Unicode name that is not a control —
/// minus the general-category test the module documentation describes.
/// Surrogates, noncharacters and out-of-range values are excluded, which covers
/// every value the screen image can hold.
///
/// Total over `u32` (`ERR-encoding-01`).
pub(crate) fn iswprint(cs: Charset, c: u32) -> bool {
    match cs {
        Charset::Ascii => (0x20..=0x7E).contains(&c),
        Charset::Utf8 => {
            c > 0x1F
                && c <= 0x10FFFF
                && !(0x7F..=0x9F).contains(&c)
                && !(0xD800..=0xDFFF).contains(&c)
                && !(0xFDD0..=0xFDEF).contains(&c)
                && (c & 0xFFFE) != 0xFFFE
        }
    }
}

/// C: `wcwidth` — terminal columns, 0 for a zero-width character, 2 for a
/// double-width one, and **-1 for a character the locale calls unprintable**.
/// `ct_visual_width` passes that -1 straight through (`ERR-encoding-17`), and
/// `literal_add` uses it to decline a literal outright.
///
/// `MB_FILL_CHAR` and every `EL_LITERAL` sentinel answer -1, because they are
/// not code points and [`iswprint`] is total.
///
/// **The interval tables are an approximation, not a Unicode database**, and
/// they predate emoji blocks a current glibc reports double-width; see the
/// module documentation. Do not extend them piecemeal — the column arithmetic
/// in `refresh` depends on this answer, and the fix is a generated table.
pub(crate) fn wcwidth(cs: Charset, c: u32) -> i32 {
    if c == 0 {
        return 0;
    }
    if !iswprint(cs, c) {
        return -1;
    }
    if cs == Charset::Ascii {
        return 1;
    }
    // Combining first, then wide: the two tables can overlap (the Hangul Jamo
    // and CJK ranges do) and zero wins.
    if in_ranges(ZERO_WIDTH, c) {
        0
    } else if in_ranges(WIDE, c) {
        2
    } else {
        1
    }
}

/// Membership test over a sorted, non-overlapping interval table. Both tables
/// are sorted and disjoint, which the tests assert.
fn in_ranges(table: &[(u32, u32)], c: u32) -> bool {
    table
        .binary_search_by(|&(lo, hi)| {
            if c < lo {
                Ordering::Greater
            } else if c > hi {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// Combining marks and format characters: one cell in the screen image, zero
/// terminal columns.
#[rustfmt::skip]
pub(crate) const ZERO_WIDTH: &[(u32, u32)] = &[
    (0x0300, 0x036F), (0x0483, 0x0489), (0x0591, 0x05BD), (0x05BF, 0x05BF),
    (0x05C1, 0x05C2), (0x05C4, 0x05C5), (0x05C7, 0x05C7), (0x0610, 0x061A),
    (0x064B, 0x065F), (0x0670, 0x0670), (0x06D6, 0x06DC), (0x06DF, 0x06E4),
    (0x06E7, 0x06E8), (0x06EA, 0x06ED), (0x0711, 0x0711), (0x0730, 0x074A),
    (0x07A6, 0x07B0), (0x07EB, 0x07F3), (0x0816, 0x0819), (0x081B, 0x0823),
    (0x0825, 0x0827), (0x0829, 0x082D), (0x0859, 0x085B), (0x08E3, 0x0902),
    (0x093A, 0x093A), (0x093C, 0x093C), (0x0941, 0x0948), (0x094D, 0x094D),
    (0x0951, 0x0957), (0x0962, 0x0963), (0x0981, 0x0981), (0x09BC, 0x09BC),
    (0x09C1, 0x09C4), (0x09CD, 0x09CD), (0x09E2, 0x09E3), (0x0A01, 0x0A02),
    (0x0A3C, 0x0A3C), (0x0A41, 0x0A42), (0x0A47, 0x0A48), (0x0A4B, 0x0A4D),
    (0x0A51, 0x0A51), (0x0A70, 0x0A71), (0x0A75, 0x0A75), (0x0A81, 0x0A82),
    (0x0ABC, 0x0ABC), (0x0AC1, 0x0AC5), (0x0AC7, 0x0AC8), (0x0ACD, 0x0ACD),
    (0x0AE2, 0x0AE3), (0x0B01, 0x0B01), (0x0B3C, 0x0B3C), (0x0B3F, 0x0B3F),
    (0x0B41, 0x0B44), (0x0B4D, 0x0B4D), (0x0B56, 0x0B56), (0x0B62, 0x0B63),
    (0x0B82, 0x0B82), (0x0BC0, 0x0BC0), (0x0BCD, 0x0BCD), (0x0C00, 0x0C00),
    (0x0C3E, 0x0C40), (0x0C46, 0x0C48), (0x0C4A, 0x0C4D), (0x0C55, 0x0C56),
    (0x0C62, 0x0C63), (0x0C81, 0x0C81), (0x0CBC, 0x0CBC), (0x0CBF, 0x0CBF),
    (0x0CC6, 0x0CC6), (0x0CCC, 0x0CCD), (0x0CE2, 0x0CE3), (0x0D01, 0x0D01),
    (0x0D41, 0x0D44), (0x0D4D, 0x0D4D), (0x0D62, 0x0D63), (0x0DCA, 0x0DCA),
    (0x0DD2, 0x0DD4), (0x0DD6, 0x0DD6), (0x0E31, 0x0E31), (0x0E34, 0x0E3A),
    (0x0E47, 0x0E4E), (0x0EB1, 0x0EB1), (0x0EB4, 0x0EB9), (0x0EBB, 0x0EBC),
    (0x0EC8, 0x0ECD), (0x0F18, 0x0F19), (0x0F35, 0x0F35), (0x0F37, 0x0F37),
    (0x0F39, 0x0F39), (0x0F71, 0x0F7E), (0x0F80, 0x0F84), (0x0F86, 0x0F87),
    (0x0F8D, 0x0F97), (0x0F99, 0x0FBC), (0x0FC6, 0x0FC6), (0x102D, 0x1030),
    (0x1032, 0x1037), (0x1039, 0x103A), (0x103D, 0x103E), (0x1058, 0x1059),
    (0x105E, 0x1060), (0x1071, 0x1074), (0x1082, 0x1082), (0x1085, 0x1086),
    (0x108D, 0x108D), (0x109D, 0x109D), (0x1160, 0x11FF), (0x135D, 0x135F),
    (0x1712, 0x1714), (0x1732, 0x1734), (0x1752, 0x1753), (0x1772, 0x1773),
    (0x17B4, 0x17B5), (0x17B7, 0x17BD), (0x17C6, 0x17C6), (0x17C9, 0x17D3),
    (0x17DD, 0x17DD), (0x180B, 0x180E), (0x18A9, 0x18A9), (0x1920, 0x1922),
    (0x1927, 0x1928), (0x1932, 0x1932), (0x1939, 0x193B), (0x1A17, 0x1A18),
    (0x1A60, 0x1A60), (0x1A75, 0x1A7C), (0x1A7F, 0x1A7F), (0x1AB0, 0x1AFF),
    (0x1B00, 0x1B03), (0x1B34, 0x1B34), (0x1B36, 0x1B3A), (0x1B3C, 0x1B3C),
    (0x1B42, 0x1B42), (0x1B6B, 0x1B73), (0x1B80, 0x1B81), (0x1BA2, 0x1BA5),
    (0x1BA8, 0x1BA9), (0x1BE6, 0x1BE6), (0x1BE8, 0x1BE9), (0x1BED, 0x1BED),
    (0x1BEF, 0x1BF1), (0x1C2C, 0x1C33), (0x1C36, 0x1C37), (0x1CD0, 0x1CD2),
    (0x1CD4, 0x1CE0), (0x1CE2, 0x1CE8), (0x1CED, 0x1CED), (0x1CF4, 0x1CF4),
    (0x1DC0, 0x1DFF), (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x2064),
    (0x2066, 0x206F), (0x20D0, 0x20F0), (0x2CEF, 0x2CF1), (0x2D7F, 0x2D7F),
    (0x2DE0, 0x2DFF), (0x302A, 0x302D), (0x3099, 0x309A), (0xA66F, 0xA672),
    (0xA674, 0xA67D), (0xA69E, 0xA69F), (0xA6F0, 0xA6F1), (0xA802, 0xA802),
    (0xA806, 0xA806), (0xA80B, 0xA80B), (0xA825, 0xA826), (0xA8C4, 0xA8C5),
    (0xA8E0, 0xA8F1), (0xA926, 0xA92D), (0xA947, 0xA951), (0xA980, 0xA982),
    (0xA9B3, 0xA9B3), (0xA9B6, 0xA9B9), (0xA9BC, 0xA9BC), (0xAA29, 0xAA2E),
    (0xAA31, 0xAA32), (0xAA35, 0xAA36), (0xAA43, 0xAA43), (0xAA4C, 0xAA4C),
    (0xAAB0, 0xAAB0), (0xAAB2, 0xAAB4), (0xAAB7, 0xAAB8), (0xAABE, 0xAABF),
    (0xAAC1, 0xAAC1), (0xAAEC, 0xAAED), (0xAAF6, 0xAAF6), (0xABE5, 0xABE5),
    (0xABE8, 0xABE8), (0xABED, 0xABED), (0xFB1E, 0xFB1E), (0xFE00, 0xFE0F),
    (0xFE20, 0xFE2F), (0xFEFF, 0xFEFF), (0xFFF9, 0xFFFB), (0x101FD, 0x101FD),
    (0x10376, 0x1037A), (0x10A01, 0x10A0F), (0x10A38, 0x10A3F), (0x11001, 0x11001),
    (0x11038, 0x11046), (0x1112D, 0x11134), (0x11180, 0x11181), (0x111B6, 0x111BE),
    (0x116AB, 0x116AD), (0x116B0, 0x116B7), (0x11C30, 0x11C3D), (0x1D165, 0x1D169),
    (0x1D16D, 0x1D172), (0x1D17B, 0x1D182), (0x1D185, 0x1D18B), (0x1D1AA, 0x1D1AD),
    (0x1D242, 0x1D244), (0xE0001, 0xE0001), (0xE0020, 0xE007F), (0xE0100, 0xE01EF),
];

/// East Asian Wide and Fullwidth, plus the emoji ranges terminals render
/// double-width: one cell in the screen image, two terminal columns, the second
/// of which the display layer fills with `MB_FILL_CHAR`.
#[rustfmt::skip]
pub(crate) const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x115F), (0x231A, 0x231B), (0x2329, 0x232A), (0x23E9, 0x23EC),
    (0x23F0, 0x23F0), (0x23F3, 0x23F3), (0x25FD, 0x25FE), (0x2614, 0x2615),
    (0x2648, 0x2653), (0x267F, 0x267F), (0x2693, 0x2693), (0x26A1, 0x26A1),
    (0x26AA, 0x26AB), (0x26BD, 0x26BE), (0x26C4, 0x26C5), (0x26CE, 0x26CE),
    (0x26D4, 0x26D4), (0x26EA, 0x26EA), (0x26F2, 0x26F3), (0x26F5, 0x26F5),
    (0x26FA, 0x26FA), (0x26FD, 0x26FD), (0x2705, 0x2705), (0x270A, 0x270B),
    (0x2728, 0x2728), (0x274C, 0x274C), (0x274E, 0x274E), (0x2753, 0x2755),
    (0x2757, 0x2757), (0x2795, 0x2797), (0x27B0, 0x27B0), (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C), (0x2B50, 0x2B50), (0x2B55, 0x2B55), (0x2E80, 0x303E),
    (0x3041, 0x33FF), (0x3400, 0x4DBF), (0x4E00, 0x9FFF), (0xA000, 0xA4CF),
    (0xA960, 0xA97F), (0xAC00, 0xD7A3), (0xF900, 0xFAFF), (0xFE10, 0xFE19),
    (0xFE30, 0xFE6F), (0xFF00, 0xFF60), (0xFFE0, 0xFFE6), (0x16FE0, 0x16FE4),
    (0x17000, 0x187F7), (0x18800, 0x18AFF), (0x1B000, 0x1B001), (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF), (0x1F18E, 0x1F18E), (0x1F191, 0x1F19A), (0x1F200, 0x1F202),
    (0x1F210, 0x1F23B), (0x1F240, 0x1F248), (0x1F250, 0x1F251), (0x1F300, 0x1F64F),
    (0x1F680, 0x1F6C5), (0x1F900, 0x1F9FF), (0x20000, 0x2FFFD), (0x30000, 0x3FFFD),
];

#[cfg(test)]
mod tests {
    use super::*;

    // Every assertion passes the charset in explicitly, so the process
    // environment cannot change the outcome.

    #[test]
    fn charset_is_parsed_from_a_posix_locale_name() {
        for name in ["en_US.UTF-8", "C.utf8", "de_DE.utf-8@euro"] {
            assert_eq!(charset_of(name.as_bytes()), Charset::Utf8, "{name}");
        }
        for name in ["C", "POSIX", "en_US", "en_US.ISO-8859-1", "ja_JP.eucJP"] {
            assert_eq!(charset_of(name.as_bytes()), Charset::Ascii, "{name}");
        }
    }

    #[test]
    fn the_utf8_codec_is_glibcs_and_not_unicodes() {
        let cs = Charset::Utf8;
        // Five and six byte sequences decode and re-encode: this is what `vis`
        // observes, and a codec capped at U+10FFFF would change its output.
        assert!(matches!(
            mbrtowc(cs, b"\xf8\x88\x80\x80\x80"),
            Mb::Char(0x20_0000, 5)
        ));
        assert!(matches!(
            mbrtowc(cs, b"\xfd\xbf\xbf\xbf\xbf\xbf"),
            Mb::Char(0x7fff_ffff, 6)
        ));
        let mut out = [0u8; MB_LEN_MAX];
        assert_eq!(wcrtomb(cs, 0x20_0000, &mut out), Some(5));
        assert_eq!(&out[..5], b"\xf8\x88\x80\x80\x80");
        assert_eq!(wcrtomb(cs, 0x7fff_ffff, &mut out), Some(6));
        assert_eq!(&out[..6], b"\xfd\xbf\xbf\xbf\xbf\xbf");
        assert_eq!(mb_cur_max(cs), 6);
        assert_eq!(mb_cur_max(Charset::Ascii), 1);
        // Past the encoder's range, and the surrogates it still refuses.
        assert_eq!(wcrtomb(cs, 0x8000_0000, &mut out), None);
        assert_eq!(wcrtomb(cs, 0xd800, &mut out), None);
    }

    #[test]
    fn the_utf8_codec_still_rejects_what_glibc_rejects() {
        let cs = Charset::Utf8;
        assert_eq!(mbstowcs_len(cs, "€".as_bytes()), Some(1));
        assert_eq!(enc_width(cs, 0x20AC), Some(3));
        let mut dst = [0u8; 4];
        assert_eq!(encode(cs, &mut dst, 0x20AC), Some(3));
        assert_eq!(&dst[..3], "€".as_bytes());
        // Overlong, surrogate, truncated, stray continuation, 0xFE: all EILSEQ.
        for bad in [
            &b"\xC0\xAF"[..],
            &b"\xED\xA0\x80"[..],
            &b"\xE2\x82"[..],
            &b"\x80"[..],
            &b"\xFE\x80\x80\x80\x80\x80"[..],
        ] {
            assert_eq!(mbstowcs_len(cs, bad), None);
        }
        // The C locale rejects every byte above 0x7F.
        assert_eq!(mbstowcs_len(Charset::Ascii, "é".as_bytes()), None);
        assert_eq!(enc_width(Charset::Ascii, 0xE9), None);
    }

    #[test]
    fn the_encoder_and_its_width_cannot_disagree() {
        // ERR-encoding-02: both are `wcrtomb`, so a measured width is always
        // the number of bytes `encode` writes.
        for cs in [Charset::Ascii, Charset::Utf8] {
            for c in [0u32, 0x41, 0x80, 0x7ff, 0x800, 0xffff, 0x10000, 0x20_0000] {
                let mut dst = [0u8; MB_LEN_MAX];
                assert_eq!(enc_width(cs, c), encode(cs, &mut dst, c), "{cs:?} {c:#x}");
            }
        }
    }

    #[test]
    fn predicates_follow_the_charset() {
        // C1 controls are `iswcntrl` in a UTF-8 locale and below 0x100, so they
        // take `ct_visual_char`'s caret arm: U+0085 renders as `^` U+00C5.
        assert!(iswcntrl(Charset::Utf8, 0x85));
        assert!(!iswcntrl(Charset::Ascii, 0x85));
        // In the C locale nothing above U+007E is printable.
        assert!(!iswprint(Charset::Ascii, 0xE9));
        assert!(iswprint(Charset::Utf8, 0xE9));
        assert!(!iswprint(Charset::Utf8, u32::MAX));
        // glibc's space class excludes the non-breaking spaces, which makes
        // them graphic and passes them through `strvis` untouched.
        for nbsp in [0xA0u32, 0x2007, 0x202F] {
            assert!(!iswspace(Charset::Utf8, nbsp));
            assert!(iswgraph(Charset::Utf8, nbsp));
        }
        assert!(iswspace(Charset::Utf8, 0x2028));
        assert!(!iswgraph(Charset::Utf8, 0x2028));
        assert!(!iswgraph(Charset::Utf8, 0xFDD0));
        assert!(!iswgraph(Charset::Ascii, 0xE9));
    }

    #[test]
    fn width_answers_the_locales_columns() {
        assert_eq!(wcwidth(Charset::Utf8, 0x4E00), 2);
        assert_eq!(wcwidth(Charset::Utf8, 0x0301), 0);
        assert_eq!(wcwidth(Charset::Utf8, u32::from(b'a')), 1);
        assert_eq!(wcwidth(Charset::Utf8, 0), 0);
        // ERR-encoding-17: the -1 `ct_visual_width` passes through, and the
        // answer `literal_add` declines on. `MB_FILL_CHAR` and the
        // `EL_LITERAL` sentinels land here.
        assert_eq!(wcwidth(Charset::Ascii, 0xE9), -1);
        assert_eq!(wcwidth(Charset::Utf8, u32::MAX), -1);
        assert_eq!(wcwidth(Charset::Utf8, 0xD800), -1);
        assert_eq!(wcwidth(Charset::Utf8, 0x1F), -1);
    }

    #[test]
    fn width_tables_are_sorted_and_disjoint() {
        for table in [ZERO_WIDTH, WIDE] {
            for pair in table.windows(2) {
                assert!(pair[0].0 <= pair[0].1);
                assert!(pair[0].1 < pair[1].0, "{:x?} then {:x?}", pair[0], pair[1]);
            }
        }
    }
}
