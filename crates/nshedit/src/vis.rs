//! Ported from `src/vis.c`; rules live in `docs/spec/port/src/vis.md`.

// The `vis`/`strvis` family takes caller-supplied buffers as bare pointers
// with no length — that is the C ABI this crate exists to match, and the
// safety contract is the C one, stated in the `sem` rules (`strvis` needs
// `4 * strlen(src) + 1` bytes, the bounded forms need `dlen`). Marking the
// functions `unsafe` would change the exported signatures.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_char;
use std::cell::Cell;

// ---------------------------------------------------------------------------
// `errno`
//
// `[dec:libedit:no-c-ffi]` bars linking libc, so the C's `errno` has no home
// here yet. The engine sets this thread-local wherever the C sets `errno`, so
// the values the `sem` rules promise (`ENOSPC` for an undersized destination,
// `ENOMEM` for the overflow guard) are recorded rather than lost; the crate
// that exports the C ABI has to publish it. The numbers are Linux's.
// ---------------------------------------------------------------------------

/// C: `ENOMEM`.
const ENOMEM: i32 = 12;
/// C: `ENOSPC`.
const ENOSPC: i32 = 28;

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
}

/// Records what the C would have stored in `errno`.
fn set_errno(e: i32) {
    ERRNO.with(|c| c.set(e));
}

/// Reads back what the last failing call recorded. The ABI crate needs this
/// to satisfy the `errno` half of the `sem` rules.
pub(crate) fn errno() -> i32 {
    ERRNO.with(Cell::get)
}

// ---------------------------------------------------------------------------
// `vis.h` flag bits. Values from `src/vis.h`; the `sem` rules quote them.
// ---------------------------------------------------------------------------

const VIS_OCTAL: i32 = 0x0001;
const VIS_CSTYLE: i32 = 0x0002;
const VIS_SP: i32 = 0x0004;
const VIS_TAB: i32 = 0x0008;
const VIS_NL: i32 = 0x0010;
const VIS_SAFE: i32 = 0x0020;
const VIS_NOSLASH: i32 = 0x0040;
const VIS_HTTPSTYLE: i32 = 0x0080;
const VIS_MIMESTYLE: i32 = 0x0100;
const VIS_GLOB: i32 = 0x1000;
const VIS_SHELL: i32 = 0x2000;
const VIS_NOLOCALE: i32 = 0x4000;
const VIS_DQ: i32 = 0x8000;

/// C: `#define MAXEXTRAS 30`.
const MAXEXTRAS: usize = 30;

/// C: `static const wchar_t char_shell[]`.
const CHAR_SHELL: &[u32] = &[
    b'\'' as u32,
    b'`' as u32,
    b'"' as u32,
    b';' as u32,
    b'&' as u32,
    b'<' as u32,
    b'>' as u32,
    b'(' as u32,
    b')' as u32,
    b'|' as u32,
    b'{' as u32,
    b'}' as u32,
    b']' as u32,
    b'\\' as u32,
    b'$' as u32,
    b'!' as u32,
    b'^' as u32,
    b'~' as u32,
];

/// C: `static const wchar_t char_glob[]`.
const CHAR_GLOB: &[u32] = &[b'*' as u32, b'?' as u32, b'[' as u32, b'#' as u32];

/// C: `#define xtoa(c) L"0123456789abcdef"[c]`.
fn xtoa(c: u32) -> u32 {
    b"0123456789abcdef"[(c & 0xf) as usize] as u32
}

/// C: `#define XTOA(c) L"0123456789ABCDEF"[c]`.
fn xtoa_upper(c: u32) -> u32 {
    b"0123456789ABCDEF"[(c & 0xf) as usize] as u32
}

/// C: `#define iswoctal(c) (((u_char)(c)) >= L'0' && ((u_char)(c)) <= L'7')`.
///
/// ERR-encoding-21: the `(u_char)` cast truncates a whole wide character to
/// its low byte before the test, so U+0130 (low byte 0x30) counts as an octal
/// digit. Reproduced — `c as u8` is that truncation.
fn iswoctal(c: u32) -> bool {
    (b'0'..=b'7').contains(&(c as u8))
}

/// C: `#define iswwhite(c) (c == L' ' || c == L'\t' || c == L'\n')`.
fn iswwhite(c: u32) -> bool {
    c == 0x20 || c == 0x09 || c == 0x0a
}

/// C: `#define iswsafe(c) (c == L'\b' || c == BELL || c == L'\r')`.
fn iswsafe(c: u32) -> bool {
    c == 0x08 || c == 0x07 || c == 0x0d
}

/// C: `wcschr(s, c) != NULL` over a NUL-terminated wide string.
///
/// The terminator is part of the search, so `c == 0` always matches. That is
/// load-bearing in `[spec:libedit:sem:vis.do-svis-fn]` step 1 — it is what
/// keeps a literal NUL out of the wide staging buffer — and it is why
/// `[spec:libedit:sem:vis.do-mvis-fn]` condition C is true for NUL.
///
/// # Safety
///
/// `s` must point at a NUL-terminated `u32` array.
unsafe fn wcschr(s: *const u32, c: u32) -> bool {
    let mut p = s;
    loop {
        let v = unsafe { *p };
        if v == c {
            return true;
        }
        if v == 0 {
            return false;
        }
        p = unsafe { p.add(1) };
    }
}

/// Length of a NUL-terminated C string, or 0 for NULL.
///
/// The C calls `strlen(mbextra)` unconditionally, so a NULL `mbextra` is a
/// crash there (`_DIAGASSERT(mbextra != NULL)`). That is undefined behaviour;
/// defined here as the empty string.
///
/// # Safety
///
/// `s` must be NULL or point at a NUL-terminated byte string.
unsafe fn cstr_bytes<'a>(s: *const c_char) -> &'a [u8] {
    if s.is_null() {
        return &[];
    }
    let mut n = 0usize;
    while unsafe { *s.add(n) } != 0 {
        n += 1;
    }
    unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n) }
}

// ---------------------------------------------------------------------------
// The `LC_CTYPE` layer.
//
// The C gets `mbrtowc`, `wcrtomb`, `iswgraph`, `iswalnum`, `iswspace`,
// `iswcntrl` and `MB_CUR_MAX` from libc, and every one of them is a locale
// query: `[spec:libedit:sem:vis.strvis-fn]` calls the resulting locale
// dependence "the single biggest hazard in the file", because `strvis(...,
// VIS_WHITE)` is the on-disk history format. `[dec:libedit:no-c-ffi]` bars
// calling libc, so the part of `LC_CTYPE` that `vis` observes is
// reimplemented here.
//
// `crate::chartype::locale` is the sibling of this module and wants merging
// with it: it carries `iswcntrl`, `iswprint` and `wcwidth`, this one carries
// `iswgraph`, `iswalnum`, `iswspace` and a resumable `mbrtowc`. They are not
// interchangeable as they stand — that module models UTF-8 as Unicode defines
// it, while glibc's UTF-8 converter is the *original* encoding: measured on
// glibc 2.41, `mbrtowc` accepts five- and six-byte sequences up to
// U+7FFFFFFF and `wcrtomb` encodes them back, rejecting only overlong forms
// and surrogates. `vis` sees the difference (`strvisx(dst,
// "\xf8\x88\x80\x80\x80", 5, 0)` is one character, `\040\^@\^@`, not five
// invalid bytes), so this module reproduces the glibc range.
//
// Three deliberate choices:
//
//  1. **Which locale.** There is no `setlocale` to consult, so the charset
//     comes from `LC_ALL`, then `LC_CTYPE`, then `LANG` — i.e. the port
//     behaves as if the program called `setlocale(LC_ALL, "")`, which every
//     interactive libedit consumer does. A C program that never calls
//     `setlocale` stays in the C locale no matter what the environment says;
//     that case is not distinguishable from here.
//  2. **Which charsets.** UTF-8 and the C/POSIX single-byte charset only.
//     Every other codeset (ISO-8859-x, EUC, Shift-JIS) is treated as C/POSIX,
//     because reproducing it needs charmap tables. Faithful on any modern
//     host, wrong on a legacy single-byte locale.
//  3. **Which characters are graphic.** Measured against glibc rather than
//     assumed: its UTF-8 `graph` class is every assigned code point that is
//     neither in its `cntrl` class nor in its `space` class — and its `space`
//     class deliberately excludes the non-breaking spaces U+00A0, U+2007 and
//     U+202F, which are therefore *graphic* and pass through `strvis`
//     untouched. Both sets are reproduced exactly. "Assigned" is the one part
//     that cannot be: Rust's standard library exposes no general-category
//     table, so unassigned code points (U+0378 and friends) are reported
//     graphic here and are not by glibc. See the report note; it needs a
//     Unicode table this module has no business carrying.
// ---------------------------------------------------------------------------

mod locale {
    use std::cell::Cell;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Charset {
        /// The C/POSIX locale: ASCII, one byte per character.
        Ascii,
        /// glibc's UTF-8: the full original encoding, up to six bytes and
        /// 31 bits, rejecting overlong forms and surrogates.
        Utf8,
    }

    thread_local! {
        static CHARSET: Cell<Option<Charset>> = const { Cell::new(None) };
    }

    /// Re-reads the environment. Called once per public entry point, so the
    /// charset is a snapshot for the duration of one call exactly as the C's
    /// `LC_CTYPE` is, without an environment lookup per character.
    pub fn refresh() {
        CHARSET.with(|c| c.set(Some(from_env())));
    }

    pub fn charset() -> Charset {
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
                Some(v) if !v.is_empty() => return from_name(v.as_encoded_bytes()),
                _ => {}
            }
        }
        Charset::Ascii
    }

    /// `lang_territory.codeset@modifier` — only the codeset matters, and only
    /// UTF-8 is distinguished from the C locale. Comparison ignores case and
    /// the `-`/`_` that separate "UTF-8" from "utf8".
    fn from_name(name: &[u8]) -> Charset {
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

    /// C: `MB_LEN_MAX`. 16 on glibc — the size of `istrsenvisx`'s scratch
    /// buffer, the cap on the `mbrtowc` input window, and the per-character
    /// budget in the unbounded output bound.
    pub const MB_LEN_MAX: usize = 16;

    /// C: `MB_CUR_MAX`. glibc reports 6 in a UTF-8 locale, 1 in the C locale.
    pub fn mb_cur_max() -> usize {
        match charset() {
            Charset::Ascii => 1,
            Charset::Utf8 => 6,
        }
    }

    /// What `mbrtowc` reports.
    ///
    /// `(size_t)-1` (EILSEQ) and `(size_t)-2` (incomplete) are one variant:
    /// every caller in this file tests `clen < 0` and treats them alike.
    pub enum Mb {
        /// A whole character and the bytes it consumed. A NUL is
        /// `Char(0, 1)`; the C's 0 return is normalised by the caller.
        Char(u32, usize),
        /// Invalid or incomplete.
        Bad,
    }

    /// C: `mbrtowc(&wc, bytes, bytes.len(), &state)` from a zeroed state.
    /// `bytes` is never empty.
    pub fn mbrtowc(bytes: &[u8]) -> Mb {
        let b0 = bytes[0];
        match charset() {
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

    /// C: `wcrtomb(out, c, &state)` from a zeroed state. `None` is its
    /// `(size_t)-1`/EILSEQ return.
    pub fn wcrtomb(c: u32, out: &mut [u8; MB_LEN_MAX]) -> Option<usize> {
        match charset() {
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

    /// glibc's `cntrl` class: the C0 and C1 controls, plus the line and
    /// paragraph separators in a UTF-8 locale.
    pub fn is_cntrl(c: u32) -> bool {
        match charset() {
            Charset::Ascii => c < 0x20 || c == 0x7f,
            Charset::Utf8 => c < 0x20 || (0x7f..=0x9f).contains(&c) || c == 0x2028 || c == 0x2029,
        }
    }

    /// glibc's `space` class. Note what is *not* here: U+0085, U+00A0,
    /// U+2007 and U+202F are whitespace to Unicode but not to glibc, and
    /// `[spec:libedit:sem:vis.do-mvis-fn]`'s trailing-whitespace test and the
    /// graphic test below both turn on the difference.
    pub fn is_space(c: u32) -> bool {
        match charset() {
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

    /// C: `iswgraph(c)`.
    pub fn is_graph(c: u32) -> bool {
        match charset() {
            Charset::Ascii => (0x21..=0x7e).contains(&c),
            Charset::Utf8 => {
                // Surrogates and anything above U+10FFFF are unassigned, and
                // glibc answers false for them; `char::from_u32` is that test.
                if char::from_u32(c).is_none() {
                    return false;
                }
                // Noncharacters are unassigned too. The rest of the
                // unassigned range is not detectable without a general
                // category table, and is reported graphic here.
                if (0xfdd0..=0xfdef).contains(&c) || (c & 0xfffe) == 0xfffe {
                    return false;
                }
                !is_cntrl(c) && !is_space(c)
            }
        }
    }

    /// C: `iswalnum(c)`.
    ///
    /// glibc's `alnum` is its `alpha` class plus its `digit` class, and
    /// `digit` is only ASCII `0`-`9` in every locale, while `alpha` is the
    /// Unicode Alphabetic property *plus* the non-ASCII decimal digits.
    /// Plain `is_alphanumeric` is the wrong shape for it below U+0100 — that
    /// would make the Latin-1 fractions and superscripts (`½ ¼ ¾ ¹ ² ³`,
    /// category No) alphanumeric, which glibc does not, and those are exactly
    /// the values the conversion-error fallback produces from raw bytes.
    /// Measured against glibc over the whole code space, this form answers
    /// true for every code point glibc calls alphanumeric, and additionally
    /// for the numeric-but-not-decimal characters above U+00FF.
    pub fn is_alnum(c: u32) -> bool {
        match charset() {
            Charset::Ascii => c < 0x80 && (c as u8).is_ascii_alphanumeric(),
            Charset::Utf8 => char::from_u32(c).is_some_and(|ch| {
                ch.is_alphabetic() || ch.is_ascii_digit() || (c >= 0x100 && ch.is_numeric())
            }),
        }
    }
}

use locale::{MB_LEN_MAX, Mb};

// [spec:libedit:def:vis.visfun-t-wchar-t-wint-t-int-wint-t-const-wchar-t]
/// C: `typedef wchar_t *(*visfun_t)(wchar_t *, wint_t, int, wint_t, const wchar_t *);`
///
/// The encoder `getvisfun` selects from the flags: destination cursor, the
/// character to encode, the flags, the next character (for lookahead), and
/// the "extra" set of characters to escape. It returns the advanced
/// destination cursor.
///
/// The pointers stay raw because the C's contract is raw pointer
/// arithmetic on a caller-supplied buffer with no length: the `vis` entry
/// points hand out an interior cursor and every encoder advances it. The
/// `vis.c` translation may narrow this to slices as long as the rule stays
/// annotated at whatever replaces it.
pub type VisfunT = fn(*mut u32, u32, i32, u32, *const u32) -> *mut u32;

// [spec:libedit:def:vis.iscgraph-fn]
// [spec:libedit:sem:vis.iscgraph-fn]
/// The `#ifdef notyet` fallback: `isgraph` under the C locale. Reached only
/// where the build has neither `LC_C_LOCALE` nor the macro form.
fn iscgraph(c: i32) -> i32 {
    // ERR-encoding-23 (logic, fix): on every POSIX host the C compiles form 2
    // of this function, plain `isgraph`, which tests the *current* locale and
    // so makes `VIS_NOLOCALE` a no-op in a single-byte non-ASCII locale. The
    // rule directs the port to implement form 1 instead — graphic in the C
    // locale — which is what the flag promises and what forms 1 and 3
    // deliver. Every call site has already reduced `c` to 0..255.
    i32::from((0x21..=0x7e).contains(&c))
}

/// C: `#define ISGRAPH(flags, c) (((flags) & VIS_NOLOCALE) ? iscgraph(c) : iswgraph(c))`.
fn is_graph(flags: i32, c: u32) -> bool {
    if flags & VIS_NOLOCALE != 0 {
        iscgraph(c as i32) != 0
    } else {
        locale::is_graph(c)
    }
}

// [spec:libedit:def:vis.do-hvis-fn]
// [spec:libedit:sem:vis.do-hvis-fn]
/// Shaped by `VisfunT`: `dst` is a cursor into a caller-supplied buffer with
/// no length, advanced and returned.
fn do_hvis(dst: *mut u32, c: u32, flags: i32, nextc: u32, extra: *const u32) -> *mut u32 {
    // `iswalnum` is not gated by VIS_NOLOCALE — unlike the graphic test, it
    // always asks the current LC_CTYPE, so accented letters are URL-safe in a
    // UTF-8 locale and HTTP-style output can carry raw non-ASCII bytes.
    if locale::is_alnum(c)
        // safe
        || c == b'$' as u32
        || c == b'-' as u32
        || c == b'_' as u32
        || c == b'.' as u32
        || c == b'+' as u32
        // extra
        || c == b'!' as u32
        || c == b'*' as u32
        || c == b'\'' as u32
        || c == b'(' as u32
        || c == b')' as u32
        || c == b',' as u32
    {
        // Not "emitted literally": do_svis still applies the extra set and
        // the graphic test, so `strsvis(dst, "a", VIS_HTTPSTYLE, "a")` is
        // `\141`.
        do_svis(dst, c, flags, nextc, extra)
    } else {
        // ERR-encoding-19 (logic, reproduce): only the low 8 bits of the
        // whole wide character reach the hex digits — do_svis's multi-byte
        // split is not applied and there is no `%uXXXX` form — so U+0378
        // encodes as `%78` and decodes back as ASCII `x`.
        unsafe {
            *dst = b'%' as u32;
            *dst.add(1) = xtoa((c >> 4) & 0xf);
            *dst.add(2) = xtoa(c & 0xf);
            dst.add(3)
        }
    }
}

// [spec:libedit:def:vis.do-mvis-fn]
// [spec:libedit:sem:vis.do-mvis-fn]
fn do_mvis(dst: *mut u32, c: u32, flags: i32, nextc: u32, extra: *const u32) -> *mut u32 {
    // Condition C's twelve characters. `wcschr` matches the terminator, so
    // this is also true for c == 0 — harmless, condition B already covers it.
    const SPECIALS: [u32; 13] = [
        b'#' as u32,
        b'$' as u32,
        b'@' as u32,
        b'[' as u32,
        b'\\' as u32,
        b']' as u32,
        b'^' as u32,
        b'`' as u32,
        b'{' as u32,
        b'|' as u32,
        b'}' as u32,
        b'~' as u32,
        0,
    ];
    let is_special = unsafe { wcschr(SPECIALS.as_ptr(), c) };
    // `iswspace` is not gated by VIS_NOLOCALE either.
    let space = locale::is_space(c);
    if c != 0x0a
        && ((space && (nextc == 0x0d || nextc == 0x0a))
            // The C writes the middle test as `(c > 60 && c < 62)`, which is
            // just `c == 61`: `=` self-escapes.
            || (!space && (c < 33 || c == 61 || c > 126))
            || is_special)
    {
        // ERR-encoding-19 (logic, reproduce): lossy above U+00FF exactly as
        // in do_hvis — U+20AC encodes as `=AC` in a UTF-8 locale.
        unsafe {
            *dst = b'=' as u32;
            *dst.add(1) = xtoa_upper((c >> 4) & 0xf);
            *dst.add(2) = xtoa_upper(c & 0xf);
            dst.add(3)
        }
    } else {
        do_svis(dst, c, flags, nextc, extra)
    }
}

// [spec:libedit:def:vis.do-mbyte-fn]
// [spec:libedit:sem:vis.do-mbyte-fn]
fn do_mbyte(dst: *mut u32, c: u32, flags: i32, nextc: u32, iswextra: i32) -> *mut u32 {
    let mut c = c;
    let mut dst = dst;
    // Emit one wide character and advance.
    let mut put = |v: u32| unsafe {
        *dst = v;
        dst = dst.add(1);
    };

    // Stage 1 — VIS_CSTYLE.
    if flags & VIS_CSTYLE != 0 {
        let named = match c {
            0x0a => Some(b'n'),
            0x0d => Some(b'r'),
            0x08 => Some(b'b'),
            0x07 => Some(b'a'), // BELL
            0x0b => Some(b'v'),
            0x09 => Some(b't'),
            0x0c => Some(b'f'),
            0x20 => Some(b's'),
            _ => None,
        };
        if let Some(letter) = named {
            put(b'\\' as u32);
            put(u32::from(letter));
            return dst;
        }
        if c == 0 {
            put(b'\\' as u32);
            put(b'0' as u32);
            // The whole of the lookahead: `\0` would otherwise run together
            // with a following literal octal digit. ERR-encoding-21 lives in
            // `iswoctal`, which truncates `nextc` to its low byte first.
            if iswoctal(nextc) {
                put(b'0' as u32);
                put(b'0' as u32);
            }
            return dst;
        }
        // `n r b a v t f s 0 M ^ $` already mean something after a
        // backslash, so they fall through to stage 2.
        let reserved = matches!(
            c,
            0x6e | 0x72 | 0x62 | 0x61 | 0x76 | 0x74 | 0x66 | 0x73 | 0x30 | 0x4d | 0x5e | 0x24
        );
        if !reserved && is_graph(flags, c) && !iswoctal(c) {
            // ERR-encoding-20 (logic, reproduce): `x` is not reserved, so
            // this arm can emit `\x` — the decoder's hex introducer. The
            // output does not round-trip: `strunvis` drops a trailing `\x`.
            put(b'\\' as u32);
            put(c);
            return dst;
        }
    }

    // Stage 2 — octal.
    //
    // ERR-encoding-22 (logic, reproduce): `(c & 0177) == L' '` is a masked
    // comparison, so it is true for 0x20 *and* 0xA0. Byte 0xA0 is therefore
    // always `\240` while 0xA1 is `\M-!`, with no relevant flag set. The
    // on-disk history format depends on this.
    if iswextra != 0 || (c & 0o177) == 0x20 || flags & VIS_OCTAL != 0 {
        let b = c as u8;
        put(b'\\' as u32);
        put(u32::from((b >> 6) & 0o3) + b'0' as u32);
        put(u32::from((b >> 3) & 0o7) + b'0' as u32);
        put((c & 0o7) + b'0' as u32);
        return dst;
    }

    // Stage 3 — meta / control. The leading backslash here is the only thing
    // VIS_NOSLASH suppresses; stage 2's is not.
    if flags & VIS_NOSLASH == 0 {
        put(b'\\' as u32);
    }
    if c & 0o200 != 0 {
        c &= 0o177;
        put(b'M' as u32);
    }
    if locale::is_cntrl(c) {
        put(b'^' as u32);
        if c == 0o177 {
            put(b'?' as u32);
        } else {
            put(c + b'@' as u32);
        }
    } else {
        // Reachable only for a byte of a decomposed multi-byte character:
        // `strvis(dst, "\xcd\xb8", 0)` in a UTF-8 locale gives `\^C\-x`.
        put(b'-' as u32);
        put(c);
    }
    dst
}

// [spec:libedit:def:vis.do-svis-fn]
// [spec:libedit:sem:vis.do-svis-fn]
fn do_svis(dst: *mut u32, c: u32, flags: i32, nextc: u32, extra: *const u32) -> *mut u32 {
    // c == 0 is always "extra", because wcschr matches the terminator. That
    // guarantees a literal NUL never reaches the wide staging buffer, which
    // is what lets istrsenvisx measure the result with wcslen.
    let iswextra = i32::from(unsafe { wcschr(extra, c) });
    if iswextra == 0 && (is_graph(flags, c) || iswwhite(c) || (flags & VIS_SAFE != 0 && iswsafe(c)))
    {
        unsafe {
            *dst = c;
            return dst.add(1);
        }
    }

    // Byte decomposition: the code point's own bytes, big-endian, skipping
    // leading zero bytes — not its multibyte encoding. The C's mask loop runs
    // over eight bytes of a uint64_t to mirror the identical loop in
    // istrsenvisx's output path; a 32-bit wint_t makes the top four
    // iterations dead, and they are kept for that symmetry.
    let mut dst = dst;
    let mut wmsk: u64 = 0;
    for i in (0..8).rev() {
        let shft = i * 8;
        let bmsk: u64 = 0xffu64 << shft;
        wmsk |= bmsk;
        if (u64::from(c) & wmsk) != 0 || i == 0 {
            // Every byte gets the same flags, the same undecomposed nextc and
            // the same iswextra, so a multi-byte character in the extra set
            // has *all* its bytes octal-escaped.
            dst = do_mbyte(
                dst,
                ((u64::from(c) & bmsk) >> shft) as u32,
                flags,
                nextc,
                iswextra,
            );
        }
    }
    dst
}

// [spec:libedit:def:vis.getvisfun-fn]
// [spec:libedit:sem:vis.getvisfun-fn]
fn getvisfun(flags: i32) -> VisfunT {
    // The order is observable: with both bits set, HTTP wins. VIS_HTTP1866
    // is not handled here or anywhere else in the encoder — setting it has no
    // effect at all, and a port must not invent an encoder for it.
    if flags & VIS_HTTPSTYLE != 0 {
        return do_hvis;
    }
    if flags & VIS_MIMESTYLE != 0 {
        return do_mvis;
    }
    do_svis
}

// [spec:libedit:def:vis.makeextralist-fn]
// [spec:libedit:sem:vis.makeextralist-fn]
/// The C returns a `calloc`ed wide string the caller frees, so this owns it;
/// `None` is its NULL return.
fn makeextralist(flags: i32, src: *const c_char) -> Option<Vec<u32>> {
    let bytes = unsafe { cstr_bytes(src) };
    let len = bytes.len();
    // The C allocates len + MAXEXTRAS and relies on the headroom: worst case
    // 4 + 18 + 1 + 1 + 1 + 1 + 1 = 27 appended characters plus the
    // terminator. Reserving the same amount keeps that documented bound
    // visible; a port that adds a character class must grow the constant.
    let mut dst: Vec<u32> = Vec::with_capacity(len + MAXEXTRAS);

    // Passing `len` (a byte count) as the wide-character limit is safe
    // because a multibyte string of `len` bytes never decodes to more than
    // `len` characters, so the limit is never the thing that stops the
    // conversion.
    let mut converted = false;
    if flags & VIS_NOLOCALE == 0 {
        let mut i = 0usize;
        let mut wide = Vec::with_capacity(len);
        loop {
            if i == len {
                converted = true;
                break;
            }
            match locale::mbrtowc(&bytes[i..]) {
                // mbsrtowcs stops at the terminating NUL, which strlen has
                // already excluded, so a NUL cannot appear here.
                Mb::Char(c, n) => {
                    wide.push(c);
                    i += n;
                }
                // mbsrtowcs returned (size_t)-1.
                Mb::Bad => break,
            }
        }
        if converted {
            dst = wide;
        }
    }
    if !converted {
        // VIS_NOLOCALE, or a conversion failure: 1:1 byte map.
        dst.clear();
        dst.extend(bytes.iter().map(|&b| u32::from(b)));
    }

    if flags & VIS_GLOB != 0 {
        dst.extend_from_slice(CHAR_GLOB);
    }
    if flags & VIS_SHELL != 0 {
        dst.extend_from_slice(CHAR_SHELL);
    }
    if flags & VIS_SP != 0 {
        dst.push(b' ' as u32);
    }
    if flags & VIS_TAB != 0 {
        dst.push(b'\t' as u32);
    }
    if flags & VIS_NL != 0 {
        dst.push(b'\n' as u32);
    }
    if flags & VIS_DQ != 0 {
        dst.push(b'"' as u32);
    }
    if flags & VIS_NOSLASH == 0 {
        // Why a backslash in the input comes out as `\134`, and why
        // VIS_NOSLASH output can contain literal backslashes.
        dst.push(b'\\' as u32);
    }
    dst.push(0);

    // The C's only failure is calloc returning NULL. Rust's allocator aborts
    // instead, so this never answers None; the caller's handling of None is
    // kept because the rule describes it.
    Some(dst)
}

// [spec:libedit:def:vis.istrsenvisx-fn]
// [spec:libedit:sem:vis.istrsenvisx-fn]
/// `mbdstp` is the C's `char **`: an in/out cursor the function may replace
/// with a buffer it allocates. `dlen` and `cerr_ptr` are its nullable in/out
/// parameters. Returns the C's `int`: bytes written, or -1 with `errno` set.
fn istrsenvisx(
    mbdstp: &mut *mut c_char,
    dlen: Option<&mut usize>,
    mbsrc: *const c_char,
    mblength: usize,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    // Only the bytes the caller declared are readable, so the lookahead fudge
    // of step 1 gets nothing to read here. See ERR-encoding-06 at
    // `istrsenvisx_engine`.
    //
    // A NULL `mbsrc` with a non-zero `mblength` is `_DIAGASSERT`-guarded in
    // the C, i.e. undefined; defined here as the empty input.
    let src: &[u8] = if mbsrc.is_null() || mblength == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(mbsrc.cast::<u8>(), mblength) }
    };
    istrsenvisx_engine(mbdstp, dlen, src, mblength, flags, mbextra, cerr_ptr)
}

/// The body of `[spec:libedit:sem:vis.istrsenvisx-fn]`.
///
/// Split out only to carry one extra fact the C reads out of bounds instead:
/// `src` is what is actually readable, which for the single-character entry
/// points is the two-byte array they build and for everyone else is exactly
/// `mblength` bytes. `mblength` is still the declared count that step 4 clamps
/// the character count back to.
fn istrsenvisx_engine(
    mbdstp: &mut *mut c_char,
    dlen: Option<&mut usize>,
    src: &[u8],
    mblength: usize,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    // One LC_CTYPE snapshot per call, as the C has for the duration of a call.
    locale::refresh();

    // `dlen` is read once and never written back.
    let dlen: Option<usize> = dlen.map(|d| *d);

    // Step 1 — lookahead fudge.
    //
    // ERR-encoding-06 (UB, define): the C bumps `mbslength` from 1 to 2 so the
    // single-character entry points can see the character after the one they
    // encode, and reads `mbsrc[1]` whether or not the caller supplied it —
    // `strvisx(dst, buf, 1, VIS_CSTYLE)` on a NUL gives `\000` or `\0`
    // depending on a byte past the end of `buf`. Defined here as the rule
    // directs: read the extra byte only where the caller actually supplied
    // one, and otherwise treat the lookahead as absent, which lands on the
    // zeroed slot at the end of `psrc` and so reads as `L'\0'`.
    //
    // The clamp to `src.len()` also covers the C's other `_DIAGASSERT`:
    // `mbsrc == NULL` with a non-zero `mblength` reads through a null
    // pointer there, and is defined here as the empty input.
    let mut mbslength = mblength.min(src.len());
    if mbslength == 1 {
        mbslength = 2.min(src.len());
    }

    // Step 2 — overflow guard and allocation.
    if mbslength > (usize::MAX - 1) / 16 {
        set_errno(ENOMEM);
        return -1;
    }

    // The decoded input, and the wide staging buffer. The 16x factor is the
    // only bound protecting `pdst`; nothing checks it during encoding, and it
    // holds because the widest expansion is 4 output characters per input
    // byte (`\ooo`). Rust's allocator aborts rather than returning NULL, so
    // the C's two allocation-failure paths are unreachable.
    let mut psrc: Vec<u32> = vec![0; mbslength + 1];
    let mut pdst: Vec<u32> = vec![0; 16 * mbslength + 1];

    // The destination the C allocates when the caller passed NULL. The
    // caller (`stravis`) frees it with C `free`, which works because Rust's
    // default global allocator is malloc-backed; the ABI crate has to keep
    // that true or export a deallocator.
    let mut allocated: Option<usize> = None;
    if mbdstp.is_null() {
        let n = 16 * mbslength + 1;
        let mut v: Vec<u8> = vec![0; n];
        let p = v.as_mut_ptr();
        core::mem::forget(v);
        allocated = Some(n);
        *mbdstp = p.cast::<c_char>();
    }
    let mut mbdst: *mut u8 = (*mbdstp).cast::<u8>();

    // Step 3 — conversion error latch. Once 1 it is never cleared, and it
    // governs both the input and the output loop.
    let mut cerr = if flags & VIS_NOLOCALE != 0 {
        1
    } else {
        cerr_ptr.as_deref().copied().unwrap_or(0)
    };

    let outcome: Option<i32> = 'out: {
        // Step 4 — input loop. Byte-count driven; it does not stop at NUL, so
        // a block containing NULs is fully processed.
        let mut pos = 0usize; // index into src
        let mut nsrc = 0usize; // wide characters produced
        let mut remaining = mbslength;
        while remaining > 0 {
            let mut clen: isize = 0;
            let mut ok = false;
            if cerr == 0 {
                let window = remaining.min(MB_LEN_MAX);
                match locale::mbrtowc(&src[pos..pos + window]) {
                    Mb::Char(c, n) => {
                        psrc[nsrc] = c;
                        clen = n as isize;
                        ok = true;
                    }
                    Mb::Bad => clen = -1,
                }
            }
            if cerr != 0 || !ok {
                // Conversion error: process as a byte instead, and latch.
                psrc[nsrc] = u32::from(src[pos]);
                clen = 1;
                cerr = 1;
            }
            if clen == 0 {
                // The C's `mbrtowc` returns 0 for an embedded NUL after
                // storing `L'\0'`, and this turns that back into one byte
                // consumed. `locale::mbrtowc` reports the byte directly, so
                // the normalisation never fires here; it is kept because the
                // rule names it as a step.
                clen = 1;
            }
            let clen = clen as usize;
            nsrc += 1;
            pos += clen;
            remaining -= clen;
        }
        let mut len = nsrc;
        // Discards the lookahead character in the single-character case.
        if mblength < len {
            len = mblength;
        }

        // Step 5 — extra list.
        let Some(extra) = makeextralist(flags, mbextra) else {
            // ERR-encoding-09 (memory, define): the C writes a NUL to the
            // front of the destination, sets the return value to 0
            // (*success*) and then takes the cleanup path, which frees that
            // destination when this function allocated it — so `stravis`
            // returns 0 with `*mbdstp` dangling. Defined here as: the
            // caller-supplied-buffer case keeps the C's observable result
            // (empty string, return 0), and the allocating case returns an
            // error instead of a freed pointer. Unreachable in Rust either
            // way, since `makeextralist` cannot fail.
            if dlen == Some(0) {
                set_errno(ENOSPC);
                break 'out None;
            }
            unsafe { *mbdst = 0 };
            if allocated.is_some() {
                set_errno(ENOMEM);
                break 'out None;
            }
            break 'out Some(0);
        };
        let extra_ptr = extra.as_ptr();

        // Step 6 — encoding loop. Nothing bounds-checks `pdst`. The C's
        // `len >= 1 ? *src : L'\0'` is always true inside a `len > 0` loop,
        // and its `dst == NULL` check afterwards is dead — no encoder ever
        // returns NULL — so neither is translated.
        let f = getvisfun(flags);
        let start: *mut u32 = pdst.as_mut_ptr();
        let mut dst = start;
        for i in 0..len {
            let c = psrc[i];
            // The next character, undecomposed; for the last one this is the
            // zeroed slot at the end of psrc.
            let next = psrc[i + 1];
            dst = f(dst, c, flags, next, extra_ptr);
        }
        unsafe { *dst = 0 };

        // Step 7 — output loop.
        //
        // wcslen is safe only because no encoder emits a bare L'\0'; kept as
        // a walk rather than a pointer difference so that stays visible.
        let mut len = 0usize;
        while unsafe { *start.add(len) } != 0 {
            len += 1;
        }

        let maxolen = match dlen {
            Some(0) => {
                set_errno(ENOSPC);
                break 'out None;
            }
            Some(d) => d,
            None => {
                if len > (usize::MAX - 1) / MB_LEN_MAX {
                    set_errno(ENOSPC);
                    break 'out None;
                }
                // A computed bound, not the caller's actual buffer size: the
                // unbounded variants never protect the caller at all.
                len * MB_LEN_MAX + 1
            }
        };

        let mut olen = 0usize;
        let mut mbbuf = [0u8; MB_LEN_MAX];
        let mut clen: isize = 0;
        for i in 0..len {
            let wc = unsafe { *start.add(i) };
            if cerr == 0 {
                // With at least MB_CUR_MAX bytes of room the conversion goes
                // straight into the destination and needs no check; nearer
                // the end it goes to scratch and is checked.
                let inplace = maxolen.saturating_sub(olen) > locale::mb_cur_max();
                clen = match locale::wcrtomb(wc, &mut mbbuf) {
                    Some(n) => n as isize,
                    None => -1,
                };
                if clen > 0 {
                    let n = clen as usize;
                    if !inplace && olen + n >= maxolen {
                        // maxolen counts the NUL, so nothing may be written
                        // past maxolen - 1.
                        set_errno(ENOSPC);
                        break 'out None;
                    }
                    unsafe { core::ptr::copy_nonoverlapping(mbbuf.as_ptr(), mbdst, n) };
                }
            }
            if cerr != 0 || clen < 0 {
                // ERR-encoding-14 (logic, reproduce): `cerr` latches for the
                // whole string while this loop tests it once per character,
                // so a valid multibyte prefix followed by *any* later invalid
                // byte is written out as raw code-point bytes instead of its
                // multibyte encoding. `strvis(dst, "\xe2\x82\xac\xffz",
                // VIS_WHITE)` gives the four bytes `20 AC FF 7A`, silently
                // mangling the euro sign, and the result cannot round-trip
                // through strunvis.
                //
                // Reproduced, not defined away: the behaviour is fully
                // defined in the C (no UB is involved — it is a plain logic
                // error), the conformance policy therefore reaches it as
                // "reproduce", and the byte stream crosses the ABI into the
                // history file. Fixing it belongs in idiomatization, against
                // ERR-encoding-14.
                clen = 0;
                let mut wmsk: u64 = 0;
                for j in (0..8).rev() {
                    let shft = j * 8;
                    let bmsk: u64 = 0xffu64 << shft;
                    wmsk |= bmsk;
                    if (u64::from(wc) & wmsk) != 0 || j == 0 {
                        if olen + clen as usize + 1 >= maxolen {
                            set_errno(ENOSPC);
                            break 'out None;
                        }
                        unsafe {
                            *mbdst.add(clen as usize) = ((u64::from(wc) & bmsk) >> shft) as u8;
                        }
                        clen += 1;
                    }
                }
                cerr = 1;
            }
            let n = clen as usize;
            mbdst = unsafe { mbdst.add(n) };
            olen += n;
        }

        unsafe { *mbdst = 0 };

        // The in/out flag is useless as designed: it is read only when
        // VIS_NOLOCALE is clear and written only when it is set, so a caller
        // can seed it or read it, never both. Reproduced.
        if flags & VIS_NOLOCALE != 0 {
            if let Some(p) = cerr_ptr {
                *p = cerr;
            }
        }

        // Truncates for outputs beyond INT_MAX, as the C's `(int)olen` does.
        Some(olen as i32)
    };

    match outcome {
        Some(n) => n,
        None => {
            // Cleanup path. psrc, pdst and the extra list are freed by
            // scope; the destination is freed only if this function
            // allocated it.
            //
            // ERR-encoding-10 (memory, define): the C frees that destination
            // and leaves `*mbdstp` pointing at it, so every failure after
            // allocation hands the caller a dangling non-NULL pointer.
            // Defined here as NULL.
            if let Some(n) = allocated {
                let p = (*mbdstp).cast::<u8>();
                drop(unsafe { Vec::from_raw_parts(p, n, n) });
                *mbdstp = core::ptr::null_mut();
            }
            -1
        }
    }
}

// [spec:libedit:def:vis.istrsenvisxl-fn]
// [spec:libedit:sem:vis.istrsenvisxl-fn]
fn istrsenvisxl(
    mbdstp: &mut *mut c_char,
    dlen: Option<&mut usize>,
    mbsrc: *const c_char,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    // A NULL mbsrc is accepted and treated as the empty string; an embedded
    // NUL truncates the input, which is why the `x` variants exist.
    //
    // The lookahead fudge is harmless here: for a one-character string the
    // byte the C reads is the caller's own NUL terminator, in bounds and not
    // an octal digit — exactly the `L'\0'` this port substitutes.
    let mblength = if mbsrc.is_null() {
        0
    } else {
        unsafe { cstr_bytes(mbsrc) }.len()
    };
    istrsenvisx(mbdstp, dlen, mbsrc, mblength, flags, mbextra, cerr_ptr)
}

// [spec:libedit:def:vis.svis-fn]
// [spec:libedit:sem:vis.svis-fn]
/// Returns the advanced cursor, or NULL on failure — a caller-supplied
/// buffer with no length, so the pointers stay raw.
pub fn svis(
    mbdst: *mut c_char,
    c: i32,
    flags: i32,
    nextc: i32,
    mbextra: *const c_char,
) -> *mut c_char {
    // Only the low 8 bits of c and nextc survive. The two bytes are fed to
    // the decoder together, so in a multibyte locale a lead byte in `c` that
    // `nextc` completes is encoded as one character and this
    // "single character" function emits the lookahead too.
    let cc = [c as u8, nextc as u8];
    let mut dst = mbdst;
    let ret = istrsenvisx_engine(&mut dst, None, &cc, 1, flags, mbextra, None);
    if ret < 0 {
        return core::ptr::null_mut();
    }
    // mbdst is never reassigned, because it is non-NULL on entry.
    unsafe { mbdst.add(ret as usize) }
}

// [spec:libedit:def:vis.snvis-fn]
// [spec:libedit:sem:vis.snvis-fn]
pub fn snvis(
    mbdst: *mut c_char,
    dlen: usize,
    c: i32,
    flags: i32,
    nextc: i32,
    mbextra: *const c_char,
) -> *mut c_char {
    let cc = [c as u8, nextc as u8];
    let mut dst = mbdst;
    let mut dlen = dlen;
    let ret = istrsenvisx_engine(&mut dst, Some(&mut dlen), &cc, 1, flags, mbextra, None);
    if ret < 0 {
        return core::ptr::null_mut();
    }
    unsafe { mbdst.add(ret as usize) }
}

// [spec:libedit:def:vis.strsvis-fn]
// [spec:libedit:sem:vis.strsvis-fn]
pub fn strsvis(
    mbdst: *mut c_char,
    mbsrc: *const c_char,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    let mut dst = mbdst;
    istrsenvisxl(&mut dst, None, mbsrc, flags, mbextra, None)
}

// [spec:libedit:def:vis.strsnvis-fn]
// [spec:libedit:sem:vis.strsnvis-fn]
pub fn strsnvis(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    let mut dst = mbdst;
    let mut dlen = dlen;
    istrsenvisxl(&mut dst, Some(&mut dlen), mbsrc, flags, mbextra, None)
}

// [spec:libedit:def:vis.strsvisx-fn]
// [spec:libedit:sem:vis.strsvisx-fn]
pub fn strsvisx(
    mbdst: *mut c_char,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    let mut dst = mbdst;
    istrsenvisx(&mut dst, None, mbsrc, len, flags, mbextra, None)
}

// [spec:libedit:def:vis.strsnvisx-fn]
// [spec:libedit:sem:vis.strsnvisx-fn]
pub fn strsnvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    let mut dst = mbdst;
    let mut dlen = dlen;
    istrsenvisx(&mut dst, Some(&mut dlen), mbsrc, len, flags, mbextra, None)
}

// [spec:libedit:def:vis.strsenvisx-fn]
// [spec:libedit:sem:vis.strsenvisx-fn]
pub fn strsenvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    let mut dst = mbdst;
    let mut dlen = dlen;
    istrsenvisx(
        &mut dst,
        Some(&mut dlen),
        mbsrc,
        len,
        flags,
        mbextra,
        cerr_ptr,
    )
}

// [spec:libedit:def:vis.vis-fn]
// [spec:libedit:sem:vis.vis-fn]
pub fn vis(mbdst: *mut c_char, c: i32, flags: i32, nextc: i32) -> *mut c_char {
    svis(mbdst, c, flags, nextc, c"".as_ptr())
}

// [spec:libedit:def:vis.nvis-fn]
// [spec:libedit:sem:vis.nvis-fn]
pub fn nvis(mbdst: *mut c_char, dlen: usize, c: i32, flags: i32, nextc: i32) -> *mut c_char {
    snvis(mbdst, dlen, c, flags, nextc, c"".as_ptr())
}

// [spec:libedit:def:vis.strvis-fn]
// [spec:libedit:sem:vis.strvis-fn]
pub fn strvis(mbdst: *mut c_char, mbsrc: *const c_char, flags: i32) -> i32 {
    // With VIS_WHITE this is the on-disk history format, and the format is
    // locale-dependent: the pass-through test is `iswgraph` under the
    // caller's LC_CTYPE, so the same entry is 30 bytes in a UTF-8 locale and
    // 39 in the C locale. Both round-trip through strunvis; neither is
    // hardcoded here.
    let mut dst = mbdst;
    istrsenvisxl(&mut dst, None, mbsrc, flags, c"".as_ptr(), None)
}

// [spec:libedit:def:vis.strnvis-fn]
// [spec:libedit:sem:vis.strnvis-fn]
pub fn strnvis(mbdst: *mut c_char, dlen: usize, mbsrc: *const c_char, flags: i32) -> i32 {
    // ERR-encoding-26 (divergence, reproduce): this does not truncate. On
    // overflow it returns -1/ENOSPC and leaves the destination partially
    // written and unterminated, unlike OpenBSD's and FreeBSD's snprintf-
    // shaped contract, and the argument order is (dst, dlen, src, flags).
    let mut dst = mbdst;
    let mut dlen = dlen;
    istrsenvisxl(&mut dst, Some(&mut dlen), mbsrc, flags, c"".as_ptr(), None)
}

// [spec:libedit:def:vis.stravis-fn]
// [spec:libedit:sem:vis.stravis-fn]
/// `mbdstp` is the C's `char **` out-parameter: the function allocates the
/// destination and stores it there.
pub fn stravis(mbdstp: &mut *mut c_char, mbsrc: *const c_char, flags: i32) -> i32 {
    *mbdstp = core::ptr::null_mut();
    istrsenvisxl(mbdstp, None, mbsrc, flags, c"".as_ptr(), None)
}

// [spec:libedit:def:vis.strvisx-fn]
// [spec:libedit:sem:vis.strvisx-fn]
pub fn strvisx(mbdst: *mut c_char, mbsrc: *const c_char, len: usize, flags: i32) -> i32 {
    let mut dst = mbdst;
    istrsenvisx(&mut dst, None, mbsrc, len, flags, c"".as_ptr(), None)
}

// [spec:libedit:def:vis.strnvisx-fn]
// [spec:libedit:sem:vis.strnvisx-fn]
pub fn strnvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
) -> i32 {
    let mut dst = mbdst;
    let mut dlen = dlen;
    istrsenvisx(
        &mut dst,
        Some(&mut dlen),
        mbsrc,
        len,
        flags,
        c"".as_ptr(),
        None,
    )
}

// [spec:libedit:def:vis.strenvisx-fn]
// [spec:libedit:sem:vis.strenvisx-fn]
pub fn strenvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    let mut dst = mbdst;
    let mut dlen = dlen;
    istrsenvisx(
        &mut dst,
        Some(&mut dlen),
        mbsrc,
        len,
        flags,
        c"".as_ptr(),
        cerr_ptr,
    )
}

// The five `vis.h` prototypes below are declarations only: the decoder is
// `src/unvis.c`, ported in `crate::unvis`, and a header prototype has no
// separate Rust definition. Re-exporting keeps `vis::` the name the header
// publishes without a second implementation of each function.

// [spec:libedit:def:vis.strunvis-fn]
// [spec:libedit:sem:vis.strunvis-fn]
pub use crate::unvis::strunvis;

// [spec:libedit:def:vis.strnunvis-fn]
// [spec:libedit:sem:vis.strnunvis-fn]
pub use crate::unvis::strnunvis;

// [spec:libedit:def:vis.strunvisx-fn]
// [spec:libedit:sem:vis.strunvisx-fn]
pub use crate::unvis::strunvisx;

// [spec:libedit:def:vis.strnunvisx-fn]
// [spec:libedit:sem:vis.strnunvisx-fn]
pub use crate::unvis::strnunvisx;

// [spec:libedit:def:vis.unvis-fn]
// [spec:libedit:sem:vis.unvis-fn]
pub use crate::unvis::unvis;
