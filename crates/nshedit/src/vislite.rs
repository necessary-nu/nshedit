//! `strvis(dst, src, VIS_NL)`, and nothing else.
//!
//! # Why a subset rather than `vis(3)`
//!
//! One call site in this crate escapes for *display*: `hist_command`'s
//! `history` / `history list` builtin, which prints each entry and needs only
//! the guarantee that an entry cannot split itself across printed lines. That
//! is `VIS_NL`, and the C reaches it through `strvis`.
//!
//! Routing it through the `bsd` crate made a human-readable listing depend on
//! an optional feature. `bsd` is off by default, the seam in
//! [`crate::histfile`] answers `None` when it is off, and `hist_command` turns
//! a `None` into the C's -1 — so on the build that actually ships, the
//! `history` builtin printed nothing and reported failure. A listing is not
//! the place to carry an optional dependency.
//!
//! What `VIS_NL` needs is a small fraction of `vis(3)`: no `VIS_CSTYLE`, no
//! `VIS_HTTPSTYLE` or `VIS_MIMESTYLE`, no `VIS_GLOB`/`VIS_SHELL`, no caller
//! extra list, no bound to truncate at, and no decoder. Reading history files
//! still needs the whole engine in the other direction, and
//! [`crate::histfile::vis_decode_into`] still takes it from `bsd`; this module
//! does not touch that path.
//!
//! # Why it lives here and not in `histfile`
//!
//! `histfile` is the native container format, and its `bsd` seam exists for
//! one thing: reading a legacy `_HiStOrY_V2_` file somebody already has. The
//! listing escape is neither a file format nor legacy. Keeping it apart leaves
//! that seam as the four `cfg`s it is documented to be, and gives the encoder
//! somewhere to carry its own derivation and its own differential.
//!
//! # The algorithm, and where it came from
//!
//! Transcribed from `src/vis.c`: `strvis` → `istrsenvisx` → `do_svis` →
//! `do_mbyte`, with the extra list from `makeextralist`. Every claim below was
//! then measured against a real `strvis`, byte for byte — see the tests.
//!
//! Three stages, as the C has:
//!
//! 1. **Widen.** `mbrtowc` over the input. A conversion failure *latches*: the
//!    C sets `cerr` and never calls `mbrtowc` again for the rest of the call,
//!    so one bad byte turns every byte after it into its own character even
//!    when what follows is perfectly good UTF-8.
//! 2. **Encode** each wide character, producing more wide characters.
//! 3. **Narrow** with `wcrtomb` — unless the widening latched, in which case
//!    each character is written as its own bytes, most significant first.
//!
//! # Two things that are easy to get backwards
//!
//! **A newline comes out as `\012`, not as `\n`.** `VIS_NL` does not mean
//! "spell the newline `\n`"; it puts `'\n'` in the *extra* list, and a
//! character in the extra list takes `do_mbyte`'s octal branch. `\n` would
//! need `VIS_CSTYLE`, which this call site does not pass. A backslash is in
//! the extra list for the same reason — `VIS_NOSLASH` is unset — and comes out
//! `\134`.
//!
//! **A NUL is always in the extra list.** The C tests membership with
//! `wcschr(extra, c)`, and searching for `L'\0'` finds the list's own
//! terminator, so it always succeeds. That is why a NUL escapes as `\000`
//! rather than as the `\^@` its control-character shape would suggest.

use crate::locale::{self, Charset, MB_LEN_MAX, Mb};

/// Which of `vis(3)`'s whitespace flags are set.
///
/// The whole flag word is not modelled because only two combinations are
/// reachable from this crate, and a flag nobody passes is a branch nobody
/// tests. `VIS_NOSLASH` is never set and `VIS_CSTYLE` never is either, which
/// is what fixes the escape forms below.
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum Escape {
    /// `VIS_NL` — the `history` listing. A newline would otherwise split one
    /// entry across two printed lines; a space or tab in an entry is content
    /// and stays literal.
    Nl,
    /// `VIS_WHITE`, i.e. `VIS_SP | VIS_TAB | VIS_NL` — the legacy history file,
    /// where one line is exactly one entry and any literal whitespace would
    /// make the parse ambiguous.
    White,
}

impl Escape {
    /// The list `makeextralist` builds, in its order: the whitespace flags
    /// first, then the backslash that `VIS_NOSLASH` being unset appends.
    ///
    /// A NUL is not in it and does not need to be — see [`is_extra`].
    fn extra(self) -> &'static [u32] {
        const NL: [u32; 2] = [b'\n' as u32, b'\\' as u32];
        const WHITE: [u32; 4] = [b' ' as u32, b'\t' as u32, b'\n' as u32, b'\\' as u32];
        match self {
            Escape::Nl => &NL,
            Escape::White => &WHITE,
        }
    }
}

/// C: `strvis(dst, src, flags)`.
///
/// Byte string in, byte string out, no terminator on either side, and no
/// destination to size — which is what removes `ERR-history-07`'s `len * 4 + 1`
/// assumption rather than merely widening it.
///
/// The locale is the crate's cached `LC_CTYPE` snapshot, as every other
/// character-classifying call in the crate takes it.
pub(crate) fn encode(esc: Escape, src: &[u8]) -> Vec<u8> {
    encode_in(locale::charset(), esc, src)
}

/// [`encode`] against an explicitly named charset.
///
/// Separate so a test can drive both charsets in one process without touching
/// the environment: `LC_CTYPE` is process-global and the charset snapshot is
/// per-thread, so a test that set one would be changing what its siblings
/// measure.
pub(crate) fn encode_in(cs: Charset, esc: Escape, src: &[u8]) -> Vec<u8> {
    let (wide, latched) = widen(cs, src);

    let mut encoded: Vec<u32> = Vec::with_capacity(wide.len() * 4);
    for &c in &wide {
        encode_char(&mut encoded, cs, esc, c);
    }

    narrow(cs, &encoded, latched)
}

/// C: `wcschr(extra, c) != NULL`.
///
/// The NUL arm is not an addition: `wcschr` searching for `L'\0'` matches the
/// terminator of the list itself, so the C answers true for it whatever the
/// flags were.
fn is_extra(esc: Escape, c: u32) -> bool {
    c == 0 || esc.extra().contains(&c)
}

/// C: `iswwhite`. Unconditional, and deliberately so — `VIS_SP`, `VIS_TAB` and
/// `VIS_NL` act through the extra list and not through this test, which is why
/// `VIS_NL` still escapes a newline that this predicate calls white.
fn is_white(c: u32) -> bool {
    c == u32::from(b' ') || c == u32::from(b'\t') || c == u32::from(b'\n')
}

/// C: `istrsenvisx`'s input loop.
///
/// Returns the wide characters and whether the conversion latched. A NUL is a
/// character like any other — the C's comment is explicit that it does not stop
/// at one, because it may be encoding a block that contains them.
fn widen(cs: Charset, src: &[u8]) -> (Vec<u32>, bool) {
    let mut out = Vec::with_capacity(src.len());
    let mut latched = false;
    let mut at = 0;
    while at < src.len() {
        let decoded = if latched {
            Mb::Bad
        } else {
            locale::mbrtowc(cs, &src[at..])
        };
        match decoded {
            Mb::Char(c, used) => {
                out.push(c);
                at += used;
            }
            Mb::Bad => {
                // The latch is for the rest of the call, and what was already
                // converted is not reconsidered.
                latched = true;
                out.push(u32::from(src[at]));
                at += 1;
            }
        }
    }
    (out, latched)
}

/// C: `do_svis`.
fn encode_char(out: &mut Vec<u32>, cs: Charset, esc: Escape, c: u32) {
    let extra = is_extra(esc, c);
    if !extra && (locale::iswgraph(cs, c) || is_white(c)) {
        out.push(c);
        return;
    }
    for_each_significant_byte(c, |b| do_mbyte(out, cs, b, extra));
}

/// C: `do_mbyte` with no `VIS_CSTYLE`, no `VIS_OCTAL` and no `VIS_NOSLASH`.
///
/// `c` is one byte of a possibly multi-byte character, and `extra` describes
/// the whole character rather than this byte — a multi-byte character in the
/// extra list has *every* one of its bytes octal-escaped.
fn do_mbyte(out: &mut Vec<u32>, cs: Charset, c: u32, extra: bool) {
    // `(c & 0177) == ' '` and not `c == ' '`: 0xA0 has the same low seven bits
    // as a space and takes this branch too, which is why a raw 0xA0 in the C
    // locale is `\240` and not `\M- `.
    if extra || (c & 0o177) == u32::from(b' ') {
        out.push(u32::from(b'\\'));
        out.push(((c >> 6) & 0o3) + u32::from(b'0'));
        out.push(((c >> 3) & 0o7) + u32::from(b'0'));
        out.push((c & 0o7) + u32::from(b'0'));
        return;
    }

    out.push(u32::from(b'\\'));
    let mut c = c;
    if c & 0o200 != 0 {
        c &= 0o177;
        out.push(u32::from(b'M'));
    }
    if locale::iswcntrl(cs, c) {
        out.push(u32::from(b'^'));
        out.push(if c == 0o177 {
            u32::from(b'?')
        } else {
            c + u32::from(b'@')
        });
    } else {
        out.push(u32::from(b'-'));
        out.push(c);
    }
}

/// C: `istrsenvisx`'s output loop.
///
/// `latched` carries the input loop's conversion error in, because the C shares
/// one `cerr` between the two: a widening failure anywhere in the input makes
/// the *whole* output byte-at-a-time, including the characters that converted
/// cleanly before it.
fn narrow(cs: Charset, encoded: &[u32], latched: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(encoded.len());
    let mut latched = latched;
    let mut scratch = [0u8; MB_LEN_MAX];
    for &c in encoded {
        if !latched {
            if let Some(n) = locale::wcrtomb(cs, c, &mut scratch) {
                out.extend_from_slice(&scratch[..n]);
                continue;
            }
            latched = true;
        }
        for_each_significant_byte(c, |b| out.push(b as u8));
    }
    out
}

/// The byte walk both `do_svis` and the output loop's error arm perform: most
/// significant byte first, skipping leading zero bytes, always emitting the
/// least significant one. `0x0000a264` is `a2 64`; `0x1f00a264` is
/// `1f 00 a2 64`, the interior zero surviving because a higher byte was set.
fn for_each_significant_byte(c: u32, mut f: impl FnMut(u32)) {
    let mut seen = false;
    for i in (0..4).rev() {
        let b = (c >> (i * 8)) & 0xff;
        seen |= b != 0;
        if seen || i == 0 {
            f(b);
        }
    }
}

#[cfg(test)]
mod test;
