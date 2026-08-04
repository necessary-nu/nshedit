//! Ported from `src/unvis.c`; rules live in `docs/spec/port/src/unvis.md`.

use core::ffi::c_char;
use std::cell::Cell;

// ---------------------------------------------------------------------------
// `errno`
// ---------------------------------------------------------------------------

/// `EINVAL` and `ENOSPC` as Linux numbers them. The port links no libc
/// (`plan/decisions/no-c-ffi.md`), so it cannot write the real thread-local
/// `errno`; the value is parked in [`errno`] instead, and re-publishing it to a
/// C caller is the ABI crate's job. If a shared errno facility lands later,
/// this pair of items is what it replaces.
const EINVAL: i32 = 22;
const ENOSPC: i32 = 28;

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
}

/// The last `errno` this module set. Matching the C, it is written only on the
/// two failure paths and never cleared on success, so it is meaningful only
/// immediately after a -1 return.
pub fn errno() -> i32 {
    ERRNO.with(Cell::get)
}

fn set_errno(e: i32) {
    ERRNO.with(|slot| slot.set(e));
}

// ---------------------------------------------------------------------------
// State machine constants
// ---------------------------------------------------------------------------

const S_GROUND: u8 = 0; // haven't seen escape char
const S_START: u8 = 1; // start decoding special sequence
const S_META: u8 = 2; // metachar started (M)
const S_META1: u8 = 3; // metachar more, regular char (-)
const S_CTRL: u8 = 4; // control char started (^)
const S_OCTAL2: u8 = 5; // octal digit 2
const S_OCTAL3: u8 = 6; // octal digit 3
const S_HEX: u8 = 7; // mandatory hex digit
const S_HEX1: u8 = 8; // http hex digit
const S_HEX2: u8 = 9; // http hex digit 2
const S_MIME1: u8 = 10; // mime hex digit 1
const S_MIME2: u8 = 11; // mime hex digit 2
const S_EATCRNL: u8 = 12; // mime eating CRNL
const S_AMP: u8 = 13; // seen &
const S_NUMBER: u8 = 14; // collecting number
const S_STRING: u8 = 15; // collecting string

// `vis.h` return codes. `UNVIS_ERROR` (-2) is declared there but this decoder
// never returns it, so it has no constant here.
const UNVIS_VALID: i32 = 1;
const UNVIS_VALIDPUSH: i32 = 2;
const UNVIS_NOCHAR: i32 = 3;
const UNVIS_SYNBAD: i32 = -1;

// The `vis.h` flag bits `unvis` reads; every other bit of `flag` is ignored.
const VIS_HTTP1808: i32 = 0x0080;
const VIS_MIMESTYLE: i32 = 0x0100;
const VIS_HTTP1866: i32 = 0x0200;
const VIS_NOESCAPE: i32 = 0x0400;
const UNVIS_END: i32 = 0x0800;

/// `GS(a)`: the state-machine state, the low 8 bits of `*astate`.
fn gs(a: i32) -> u8 {
    (a & 0xff) as u8
}

/// `GI(a)`: the entity-name index, the top 8 bits of `*astate`.
fn gi(a: i32) -> u8 {
    ((a as u32) >> 24) as u8
}

/// `SS(a, b)`: pack name index `a` and state `b` back into `*astate`.
fn ss(a: u8, b: u8) -> i32 {
    (((a as u32) << 24) | b as u32) as i32
}

// ---------------------------------------------------------------------------
// Byte classification
// ---------------------------------------------------------------------------
//
// The C calls `<ctype.h>` on the byte widened to `unsigned char`, so these all
// take a `u8`. They are written against the C locale, which is what
// `docs/spec/port/src/unvis.md` specifies: no locale is consulted, and no
// locale in practice moves the digit, hex-digit or uppercase sets used here.

/// `isoctal` from `src/unvis.c`.
fn isoctal(c: u8) -> bool {
    (b'0'..=b'7').contains(&c)
}

/// `xtod`: value of a hex digit of either case.
fn xtod(c: u8) -> u8 {
    if c.is_ascii_digit() {
        c - b'0'
    } else {
        c.to_ascii_lowercase() - b'a' + 10
    }
}

/// `XTOD`: value of an *uppercase* hex digit, the MIME quoted-printable form.
fn xtod_upper(c: u8) -> u8 {
    if c.is_ascii_digit() {
        c - b'0'
    } else {
        c - b'A' + 10
    }
}

/// C-locale `isgraph`, i.e. ASCII 0x21..=0x7E.
///
/// ERR-encoding-07: the C's `S_START` default arm hands `isgraph` the raw
/// `int c`, not the `unsigned char` widening it uses everywhere else, so a
/// signed-`char` caller passes a negative value and the call is undefined.
/// Defining what the C leaves undefined, per
/// `plan/decisions/conformance-policy.md`: this is the C-locale answer, which
/// is what glibc gives in practice for both the negative and the unsigned
/// spelling of a byte >= 0x80 — false, so `\` followed by such a byte is
/// `UNVIS_SYNBAD` either way.
fn isgraph_c(c: i32) -> bool {
    (0x21..=0x7e).contains(&c)
}

// [spec:libedit:def:unvis.nv]
/// One row of the RFC 1866 HTML entity table `nv[]`, searched by
/// `strunvis`'s `\&entity;` decoding.
///
/// The C's `char name[7]` is a fixed seven-byte field, NUL-padded: the
/// longest entity names are six characters, so every row is NUL-terminated
/// with room to spare. Kept a fixed array so the table stays a plain static.
pub struct Nv {
    pub name: [u8; 7],
    pub value: u8,
}

/// Width of `Nv::name`. The `S_STRING` scan indexes `name[is]` directly and is
/// in bounds only because no name is longer than `NAME_LEN - 1`; a port that
/// widens the table must widen this with it.
const NAME_LEN: usize = 7;

/// NUL-pads a name into the C's fixed `char[7]` field.
const fn nv(name: &[u8], value: u8) -> Nv {
    let mut padded = [0u8; NAME_LEN];
    let mut i = 0;
    while i < name.len() {
        padded[i] = name[i];
        i += 1;
    }
    Nv {
        name: padded,
        value,
    }
}

/// RFC 1866, transcribed from `src/unvis.c`. Sorted in ASCII order by name,
/// which the `S_STRING` scan depends on: it walks forward from the last match
/// and tests only one character of the prefix.
static NV: [Nv; 100] = [
    nv(b"AElig", 198),  // capital AE diphthong (ligature)
    nv(b"Aacute", 193), // capital A, acute accent
    nv(b"Acirc", 194),  // capital A, circumflex accent
    nv(b"Agrave", 192), // capital A, grave accent
    nv(b"Aring", 197),  // capital A, ring
    nv(b"Atilde", 195), // capital A, tilde
    nv(b"Auml", 196),   // capital A, dieresis or umlaut mark
    nv(b"Ccedil", 199), // capital C, cedilla
    nv(b"ETH", 208),    // capital Eth, Icelandic
    nv(b"Eacute", 201), // capital E, acute accent
    nv(b"Ecirc", 202),  // capital E, circumflex accent
    nv(b"Egrave", 200), // capital E, grave accent
    nv(b"Euml", 203),   // capital E, dieresis or umlaut mark
    nv(b"Iacute", 205), // capital I, acute accent
    nv(b"Icirc", 206),  // capital I, circumflex accent
    nv(b"Igrave", 204), // capital I, grave accent
    nv(b"Iuml", 207),   // capital I, dieresis or umlaut mark
    nv(b"Ntilde", 209), // capital N, tilde
    nv(b"Oacute", 211), // capital O, acute accent
    nv(b"Ocirc", 212),  // capital O, circumflex accent
    nv(b"Ograve", 210), // capital O, grave accent
    nv(b"Oslash", 216), // capital O, slash
    nv(b"Otilde", 213), // capital O, tilde
    nv(b"Ouml", 214),   // capital O, dieresis or umlaut mark
    nv(b"THORN", 222),  // capital THORN, Icelandic
    nv(b"Uacute", 218), // capital U, acute accent
    nv(b"Ucirc", 219),  // capital U, circumflex accent
    nv(b"Ugrave", 217), // capital U, grave accent
    nv(b"Uuml", 220),   // capital U, dieresis or umlaut mark
    nv(b"Yacute", 221), // capital Y, acute accent
    nv(b"aacute", 225), // small a, acute accent
    nv(b"acirc", 226),  // small a, circumflex accent
    nv(b"acute", 180),  // acute accent
    nv(b"aelig", 230),  // small ae diphthong (ligature)
    nv(b"agrave", 224), // small a, grave accent
    nv(b"amp", 38),     // ampersand
    nv(b"aring", 229),  // small a, ring
    nv(b"atilde", 227), // small a, tilde
    nv(b"auml", 228),   // small a, dieresis or umlaut mark
    nv(b"brvbar", 166), // broken (vertical) bar
    nv(b"ccedil", 231), // small c, cedilla
    nv(b"cedil", 184),  // cedilla
    nv(b"cent", 162),   // cent sign
    nv(b"copy", 169),   // copyright sign
    nv(b"curren", 164), // general currency sign
    nv(b"deg", 176),    // degree sign
    nv(b"divide", 247), // divide sign
    nv(b"eacute", 233), // small e, acute accent
    nv(b"ecirc", 234),  // small e, circumflex accent
    nv(b"egrave", 232), // small e, grave accent
    nv(b"eth", 240),    // small eth, Icelandic
    nv(b"euml", 235),   // small e, dieresis or umlaut mark
    nv(b"frac12", 189), // fraction one-half
    nv(b"frac14", 188), // fraction one-quarter
    nv(b"frac34", 190), // fraction three-quarters
    nv(b"gt", 62),      // greater than
    nv(b"iacute", 237), // small i, acute accent
    nv(b"icirc", 238),  // small i, circumflex accent
    nv(b"iexcl", 161),  // inverted exclamation mark
    nv(b"igrave", 236), // small i, grave accent
    nv(b"iquest", 191), // inverted question mark
    nv(b"iuml", 239),   // small i, dieresis or umlaut mark
    nv(b"laquo", 171),  // angle quotation mark, left
    nv(b"lt", 60),      // less than
    nv(b"macr", 175),   // macron
    nv(b"micro", 181),  // micro sign
    nv(b"middot", 183), // middle dot
    nv(b"nbsp", 160),   // no-break space
    nv(b"not", 172),    // not sign
    nv(b"ntilde", 241), // small n, tilde
    nv(b"oacute", 243), // small o, acute accent
    nv(b"ocirc", 244),  // small o, circumflex accent
    nv(b"ograve", 242), // small o, grave accent
    nv(b"ordf", 170),   // ordinal indicator, feminine
    nv(b"ordm", 186),   // ordinal indicator, masculine
    nv(b"oslash", 248), // small o, slash
    nv(b"otilde", 245), // small o, tilde
    nv(b"ouml", 246),   // small o, dieresis or umlaut mark
    nv(b"para", 182),   // pilcrow (paragraph sign)
    nv(b"plusmn", 177), // plus-or-minus sign
    nv(b"pound", 163),  // pound sterling sign
    nv(b"quot", 34),    // double quote
    nv(b"raquo", 187),  // angle quotation mark, right
    nv(b"reg", 174),    // registered sign
    nv(b"sect", 167),   // section sign
    nv(b"shy", 173),    // soft hyphen
    nv(b"sup1", 185),   // superscript one
    nv(b"sup2", 178),   // superscript two
    nv(b"sup3", 179),   // superscript three
    nv(b"szlig", 223),  // small sharp s, German (sz ligature)
    nv(b"thorn", 254),  // small thorn, Icelandic
    nv(b"times", 215),  // multiply sign
    nv(b"uacute", 250), // small u, acute accent
    nv(b"ucirc", 251),  // small u, circumflex accent
    nv(b"ugrave", 249), // small u, grave accent
    nv(b"uml", 168),    // umlaut (dieresis)
    nv(b"uuml", 252),   // small u, dieresis or umlaut mark
    nv(b"yacute", 253), // small y, acute accent
    nv(b"yen", 165),    // yen sign
    nv(b"yuml", 255),   // small y, dieresis or umlaut mark
];

// [spec:libedit:def:unvis.unvis-fn]
// [spec:libedit:sem:unvis.unvis-fn]
/// `cp` is the C's one-byte output slot and `astate` its in/out state word,
/// both of which the C takes as pointers to single objects. Returns one of
/// the `UNVIS_*` results.
pub fn unvis(cp: &mut c_char, c: i32, astate: &mut i32, flag: i32) -> i32 {
    // `unsigned char uc = (unsigned char)c;`
    let uc = c as u8;
    let st = gs(*astate);

    if flag & UNVIS_END != 0 {
        match st {
            S_OCTAL2 | S_OCTAL3 | S_HEX2 => {
                *astate = ss(0, S_GROUND);
                return UNVIS_VALID;
            }
            S_GROUND => return UNVIS_NOCHAR,
            // ERR-encoding-24(e): the one `UNVIS_SYNBAD` that leaves
            // `*astate` alone, so the caller inherits the stuck state.
            _ => return UNVIS_SYNBAD,
        }
    }

    match st {
        S_GROUND => {
            *cp = 0;
            if flag & VIS_NOESCAPE == 0 && c == b'\\' as i32 {
                *astate = ss(0, S_START);
                return UNVIS_NOCHAR;
            }
            if flag & VIS_HTTP1808 != 0 && c == b'%' as i32 {
                *astate = ss(0, S_HEX1);
                return UNVIS_NOCHAR;
            }
            if flag & VIS_HTTP1866 != 0 && c == b'&' as i32 {
                *astate = ss(0, S_AMP);
                return UNVIS_NOCHAR;
            }
            if flag & VIS_MIMESTYLE != 0 && c == b'=' as i32 {
                *astate = ss(0, S_MIME1);
                return UNVIS_NOCHAR;
            }
            // `*cp = c`: the C assigns the `int` to a `char`, i.e. the low
            // eight bits.
            *cp = uc as c_char;
            return UNVIS_VALID;
        }

        S_START => {
            // The C switches on the raw `int c`, and every case label is one
            // ASCII byte, so any `c` outside 0..=0xFF — a sign-extended
            // `char`, say — reaches the default arm. `u8::try_from` is that
            // test.
            match u8::try_from(c) {
                Ok(b'\\') => {
                    *cp = b'\\' as c_char;
                    *astate = ss(0, S_GROUND);
                    return UNVIS_VALID;
                }
                Ok(d @ b'0'..=b'7') => {
                    *cp = (d - b'0') as c_char;
                    *astate = ss(0, S_OCTAL2);
                    return UNVIS_NOCHAR;
                }
                Ok(b'M') => {
                    *cp = 0o200u8 as c_char;
                    *astate = ss(0, S_META);
                    return UNVIS_NOCHAR;
                }
                Ok(b'^') => {
                    // `*cp` keeps the 0 that `S_GROUND` stored.
                    *astate = ss(0, S_CTRL);
                    return UNVIS_NOCHAR;
                }
                // The C-style letters, one byte each.
                Ok(e @ (b'n' | b'r' | b'b' | b'a' | b'v' | b't' | b'f' | b's' | b'E')) => {
                    *cp = match e {
                        b'n' => 0x0a,
                        b'r' => 0x0d,
                        b'b' => 0x08,
                        b'a' => 0x07,
                        b'v' => 0x0b,
                        b't' => 0x09,
                        b'f' => 0x0c,
                        b's' => 0x20,
                        _ => 0x1b, // 'E'
                    };
                    *astate = ss(0, S_GROUND);
                    return UNVIS_VALID;
                }
                Ok(b'x') => {
                    *astate = ss(0, S_HEX);
                    return UNVIS_NOCHAR;
                }
                Ok(b'\n') => {
                    // Hidden newline: unfolds the continuations `vis` inserts.
                    *astate = ss(0, S_GROUND);
                    return UNVIS_NOCHAR;
                }
                Ok(b'$') => {
                    // Hidden marker (`vis(1) -l`'s end-of-line mark).
                    *astate = ss(0, S_GROUND);
                    return UNVIS_NOCHAR;
                }
                _ => {
                    // Any other graphic byte is simply unescaped: `\-`, `\%`,
                    // `\"`, `\q`. See `isgraph_c` for the C's UB here.
                    if isgraph_c(c) {
                        *cp = uc as c_char;
                        *astate = ss(0, S_GROUND);
                        return UNVIS_VALID;
                    }
                }
            }
            // Space, controls and (in the C locale) every byte >= 0x80 fall
            // through to `bad`.
        }

        S_META => {
            if c == b'-' as i32 {
                *astate = ss(0, S_META1);
            } else if c == b'^' as i32 {
                *astate = ss(0, S_CTRL);
            } else {
                // falls through to `bad`
                *astate = ss(0, S_GROUND);
                return UNVIS_SYNBAD;
            }
            return UNVIS_NOCHAR;
        }

        S_META1 => {
            // No validation at all: any byte is OR'd into the 0x80.
            *astate = ss(0, S_GROUND);
            *cp = (*cp as u8 | uc) as c_char;
            return UNVIS_VALID;
        }

        S_CTRL => {
            // Also unvalidated. `*cp` is 0 from `\^` or 0x80 from `\M^`.
            let v = *cp as u8;
            *cp = if c == b'?' as i32 {
                (v | 0o177) as c_char
            } else {
                (v | (uc & 0o37)) as c_char
            };
            *astate = ss(0, S_GROUND);
            return UNVIS_VALID;
        }

        S_OCTAL2 => {
            // Second possible octal digit.
            if isoctal(uc) {
                // Yes — and maybe a third. One digit is at most 7, so the
                // shift cannot overflow the byte here.
                *cp = ((*cp as u8) << 3).wrapping_add(uc - b'0') as c_char;
                *astate = ss(0, S_OCTAL3);
                return UNVIS_NOCHAR;
            }
            // No — done with the current sequence, push the byte back.
            *astate = ss(0, S_GROUND);
            return UNVIS_VALIDPUSH;
        }

        S_OCTAL3 => {
            // Third possible octal digit.
            *astate = ss(0, S_GROUND);
            if isoctal(uc) {
                // Overflow guard, applied before accumulating: two digits
                // worth >= 32 cannot take a third, so `\400`..`\777` are hard
                // errors rather than truncations.
                if *cp as u8 & 0o40 != 0 {
                    return UNVIS_SYNBAD;
                }
                *cp = (((*cp as u8) << 3) + (uc - b'0')) as c_char;
                return UNVIS_VALID;
            }
            // We were done; push the byte back.
            return UNVIS_VALIDPUSH;
        }

        S_HEX | S_HEX1 => {
            // `S_HEX` demands the first digit and otherwise falls through into
            // `S_HEX1`; `S_HEX1` is also entered directly by `%`.
            if st == S_HEX && !uc.is_ascii_hexdigit() {
                // falls through to `bad`: `"\xz"` is an error.
                *astate = ss(0, S_GROUND);
                return UNVIS_SYNBAD;
            }
            if uc.is_ascii_hexdigit() {
                *cp = xtod(uc) as c_char;
                *astate = ss(0, S_HEX2);
                return UNVIS_NOCHAR;
            }
            // ERR-encoding-24(b): reachable only from `%`, and `*cp` is still
            // the 0 that `S_GROUND` stored, so this emits a NUL rather than
            // restoring the `%`.
            *astate = ss(0, S_GROUND);
            return UNVIS_VALIDPUSH;
        }

        S_HEX2 => {
            // ERR-encoding-24 note 7: the C stores the bare `S_GROUND` here
            // rather than the usual pack. Both are 0, so nothing observable
            // hangs on it.
            *astate = S_GROUND as i32;
            if uc.is_ascii_hexdigit() {
                *cp = (xtod(uc) | ((*cp as u8) << 4)) as c_char;
                return UNVIS_VALID;
            }
            return UNVIS_VALIDPUSH;
        }

        S_MIME1 => {
            if uc == b'\n' || uc == b'\r' {
                *astate = ss(0, S_EATCRNL);
                return UNVIS_NOCHAR;
            }
            // Uppercase hex only, so `"=4a"` is an error where `"=4A"` is `J`.
            if uc.is_ascii_hexdigit() && (uc.is_ascii_digit() || uc.is_ascii_uppercase()) {
                *cp = xtod_upper(uc) as c_char;
                *astate = ss(0, S_MIME2);
                return UNVIS_NOCHAR;
            }
            // falls through to `bad`
        }

        S_MIME2 => {
            if uc.is_ascii_hexdigit() && (uc.is_ascii_digit() || uc.is_ascii_uppercase()) {
                *astate = ss(0, S_GROUND);
                *cp = (xtod_upper(uc) | ((*cp as u8) << 4)) as c_char;
                return UNVIS_VALID;
            }
            // Both digits are mandatory; no push-back path out of a MIME
            // escape. Falls through to `bad`.
        }

        S_EATCRNL => {
            match uc {
                b'\r' | b'\n' => return UNVIS_NOCHAR,
                b'=' => {
                    *astate = ss(0, S_MIME1);
                    return UNVIS_NOCHAR;
                }
                _ => {
                    // ERR-encoding-24(d): the byte goes straight out instead
                    // of re-entering the `S_GROUND` dispatch, so a `\`, `%`,
                    // `&` or `=` right after a soft line break is literal.
                    *cp = uc as c_char;
                    *astate = ss(0, S_GROUND);
                    return UNVIS_VALID;
                }
            }
        }

        S_AMP | S_STRING => {
            if st == S_AMP {
                *cp = 0;
                if uc == b'#' {
                    *astate = ss(0, S_NUMBER);
                    return UNVIS_NOCHAR;
                }
                *astate = ss(0, S_STRING);
                // The C falls through into `S_STRING` inside this same call.
            }

            let mut ia = *cp as u8; // index in the array
            let is = gi(*astate); // index in the string

            // Defining what the C leaves undefined, per
            // `plan/decisions/conformance-policy.md`. Under the documented
            // contract `ia` is only ever an index this machine stored (< 100)
            // and `is` never exceeds 6, so the C's `nv[ia].name[is]` is in
            // bounds. A caller that writes `*cp` or `*astate` itself — which
            // the rule forbids — makes both reads run off their arrays in the
            // C. Here that is simply a malformed sequence: the safe reading,
            // and unreachable through the public entry points.
            if ia as usize >= NV.len() || is as usize >= NAME_LEN {
                *astate = ss(0, S_GROUND);
                return UNVIS_SYNBAD;
            }

            // Last character matched. ERR-encoding-24(c): this one-character
            // prefix test is the whole check, so 15 non-names decode too.
            let lc = if is == 0 {
                0
            } else {
                NV[ia as usize].name[(is - 1) as usize]
            };

            // A NUL terminates the name, and the padding makes it match one
            // past the name's end.
            let uc = if uc == b';' { 0 } else { uc };

            let mut matched = false;
            'scan: {
                while (ia as usize) < NV.len() {
                    if is != 0 && NV[ia as usize].name[(is - 1) as usize] != lc {
                        break 'scan; // goto bad
                    }
                    if NV[ia as usize].name[is as usize] == uc {
                        matched = true;
                        break 'scan;
                    }
                    ia += 1;
                }
                // Ran off the end of the table: goto bad.
            }

            if matched {
                if uc != 0 {
                    // Name unfinished: remember where the scan landed.
                    *cp = ia as c_char;
                    *astate = ss(is + 1, S_STRING);
                    return UNVIS_NOCHAR;
                }
                *cp = NV[ia as usize].value as c_char;
                *astate = ss(0, S_GROUND);
                return UNVIS_VALID;
            }
            // falls through to `bad`
        }

        S_NUMBER => {
            if uc == b';' {
                // ERR-encoding-24(a): returns without resetting the state, so
                // the decoder stays in `S_NUMBER`. `"&#65;X"` then fails,
                // `"&#65;;"` emits the byte twice, and only a `&#ddd;` at the
                // very end of the input decodes cleanly — the `UNVIS_END`
                // flush from here returns `UNVIS_SYNBAD`, which `strnunvisx`
                // discards.
                return UNVIS_VALID;
            }
            if uc.is_ascii_digit() {
                // `*cp & UCHAR_MAX` is the byte value even where `char` is
                // signed, so this reads `*cp` as a `u8`.
                let v = *cp as u8 as i32;
                let d = (uc - b'0') as i32;
                // Overflow guard before accumulating: `&#256;` and above are
                // errors, but any number of leading zeros is fine.
                if v * 10 <= 255 - d {
                    *cp = ((v * 10 + d) as u8) as c_char;
                    return UNVIS_NOCHAR;
                }
            }
            // falls through to `bad`
        }

        // The C's `default:`, which shares the `bad:` block: an uninitialised
        // or corrupted `*astate`.
        _ => {}
    }

    // `bad:` — decoder in unknown state, or a malformed sequence. Every byte
    // the failed sequence consumed is lost.
    *astate = ss(0, S_GROUND);
    UNVIS_SYNBAD
}

// [spec:libedit:def:unvis.strnunvisx-fn]
// [spec:libedit:sem:unvis.strnunvisx-fn]
/// `dst` and `src` stay raw: `strunvisx` and `strunvis` pass `(size_t)~0` for
/// `dlen`, so the destination is a caller-supplied buffer the C has no real
/// length for. Returns bytes decoded, or -1 with `errno` set.
// The signature is fixed by the C's, so the raw pointers are read and written
// from a safe fn; the caller's obligations are the `sem` rule's.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub fn strnunvisx(dst: *mut c_char, dlen: usize, src: *const c_char, flag: i32) -> i32 {
    let mut dlen = dlen;
    let mut src = src;
    // The C's `dst - start`; an index, so the return is a plain count.
    let mut n: usize = 0;
    // `t`'s initial value is irrelevant: every state that reads it back has
    // written it first.
    let mut t: c_char = 0;
    let mut state: i32 = 0;

    // `CHECKSPACE()`: reserve one output byte. The reservation happens before
    // every stored byte, the terminating NUL included, so `dlen == 0` always
    // fails. On failure the buffer is left partially written and unterminated.
    macro_rules! checkspace {
        () => {
            if dlen == 0 {
                set_errno(ENOSPC);
                return -1;
            }
            dlen -= 1;
        };
    }

    loop {
        // The C reads `src` through a plain `char`, so on a signed-`char`
        // platform a byte >= 0x80 reaches `unvis` sign-extended. We pass
        // 0..=255 instead, which the rule's byte-width note allows: the only
        // classification that could see the difference is the `isgraph` of
        // ERR-encoding-07, and `isgraph_c` answers the same for both.
        let c = unsafe { *src } as u8;
        src = unsafe { src.add(1) };
        if c == 0 {
            break;
        }
        loop {
            match unvis(&mut t, c as i32, &mut state, flag) {
                UNVIS_VALID => {
                    checkspace!();
                    unsafe { *dst.add(n) = t };
                    n += 1;
                    break;
                }
                UNVIS_VALIDPUSH => {
                    checkspace!();
                    unsafe { *dst.add(n) = t };
                    n += 1;
                    // The C's `goto again`: same byte, once more. It can fire
                    // at most once per input byte, because `unvis` is always
                    // left in `S_GROUND` when it pushes back.
                }
                // `0` is the C's defensive case; `unvis` never returns it.
                0 | UNVIS_NOCHAR => break,
                // `UNVIS_SYNBAD`, and the C's `default:` arm, which does the
                // same after a `_DIAGASSERT`.
                _ => {
                    set_errno(EINVAL);
                    return -1;
                }
            }
        }
    }

    // The flush. The byte is ignored under `UNVIS_END`; the C passes the
    // `'\0'` its loop stopped on. Anything but `UNVIS_VALID` — `UNVIS_SYNBAD`
    // included — is silently discarded, so input that stops mid-escape is not
    // an error.
    if unvis(&mut t, 0, &mut state, UNVIS_END) == UNVIS_VALID {
        checkspace!();
        unsafe { *dst.add(n) = t };
        n += 1;
    }
    // The last `CHECKSPACE()`, spelled out: the C's `dlen--` here writes a
    // local nothing reads again, so only the test survives the translation.
    if dlen == 0 {
        set_errno(ENOSPC);
        return -1;
    }
    // Stored without advancing the cursor, so the NUL is not counted.
    unsafe { *dst.add(n) = 0 };
    // The C's `(int)(dst - start)`, which truncates above `INT_MAX`.
    n as i32
}

// [spec:libedit:def:unvis.strunvisx-fn]
// [spec:libedit:sem:unvis.strunvisx-fn]
pub fn strunvisx(dst: *mut c_char, src: *const c_char, flag: i32) -> i32 {
    // `(size_t)~0`: the `ENOSPC` check can never fire, so the caller alone is
    // responsible for `dst` being large enough.
    strnunvisx(dst, usize::MAX, src, flag)
}

// [spec:libedit:def:unvis.strunvis-fn]
// [spec:libedit:sem:unvis.strunvis-fn]
pub fn strunvis(dst: *mut c_char, src: *const c_char) -> i32 {
    // Flag 0 decodes backslash escapes only. This is the pairing `history.c`
    // uses against `strvis(ptr, str, VIS_WHITE)`, so it *is* the on-disk
    // history line format.
    strnunvisx(dst, usize::MAX, src, 0)
}

// [spec:libedit:def:unvis.strnunvis-fn]
// [spec:libedit:sem:unvis.strnunvis-fn]
pub fn strnunvis(dst: *mut c_char, dlen: usize, src: *const c_char) -> i32 {
    strnunvisx(dst, dlen, src, 0)
}
