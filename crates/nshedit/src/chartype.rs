//! Ported from `src/chartype.c`; rules live in
//! `docs/spec/port/src/chartype.md`.
//!
//! # `LC_CTYPE` without libc
//!
//! Every rule in this file is written against libc: `mbstowcs`, `wctomb`,
//! `wcrtomb`, `iswcntrl`, `iswprint` and `wcwidth`, each reading the process's
//! `LC_CTYPE`. `plan/decisions/no-c-ffi.md` bars linking libc, so the port has
//! to supply them, and `crate::locale` is that supply — for this module,
//! for `literal`, `vis`, `refresh`, `map` and `search` alike. What it does and
//! does not model is documented there; two facts matter to the rules in this
//! file:
//!
//! - **The UTF-8 codec is glibc's**, which is the *original* UTF-8: five- and
//!   six-byte sequences decode and re-encode, up to U+7FFFFFFF, and
//!   `MB_CUR_MAX` is 6. `ct_decode_string` therefore accepts input the Unicode
//!   range would reject, and `ct_encode_char` can be handed a character needing
//!   six bytes where `ct_encode_string` passes five — see `ERR-encoding-12` at
//!   that call site.
//! - **`iswprint` cannot test "unassigned"**, so it errs printable where glibc
//!   would say no, and `wcwidth` is a table, not a Unicode database.
//!
//! # Buffer invariant
//!
//! `CtBufferT`'s `csize`/`wsize` are the C's *allocated element counts*. The
//! two resize helpers below establish and maintain `cbuff.len() == csize` and
//! `wbuff.len() == wsize`, which is what lets the translations index with the
//! C's own size arithmetic. An all-zero `ct_buffer_t` — the C's `calloc`ed
//! starting state — is `Vec::new()` with `0`, which satisfies it.
//!
//! # Returned slices
//!
//! `ct_encode_string`, `ct_decode_string` and `ct_visual_string` return the
//! string *content*, excluding the terminator the C writes. The terminator is
//! still in the buffer, one element past the end of the returned slice, so an
//! ABI shim can hand out `cbuff.as_ptr()`/`wbuff.as_ptr()` unchanged and the
//! returned length is the `strlen`/`wcslen` a C caller would compute.

use crate::locale;

// [spec:libedit:def:chartype.ct-buffer-t]
/// Conversion buffer: a byte half and a wide half, each grown independently.
///
/// The wide half is `u32` and never `char`. Surrogates, `(wint_t)-1` and
/// values above U+10FFFF all reach these buffers, and Rust `char` forbids
/// all three; see `sem:chartype.ct-decode-string-fn`.
///
/// The C's size fields are retained even though `Vec` tracks its own length,
/// because the `sem` rules name them: `csize`/`wsize` are the *allocated
/// element counts* that `ct_conv_cbuff_resize` and `ct_conv_wbuff_resize`
/// compare against, not the amount in use.
pub struct CtBufferT {
    /// C: `char *cbuff` — the byte half, owned.
    pub cbuff: Vec<u8>,
    /// C: `size_t csize` — allocated `char` count.
    pub csize: usize,
    /// C: `wchar_t *wbuff` — the wide half, owned.
    pub wbuff: Vec<u32>,
    /// C: `size_t wsize` — allocated `wchar_t` count.
    pub wsize: usize,
}

/// C: `#define CT_BUFSIZ ((size_t)1024)` — the flat growth step. Growth is
/// linear, not geometric.
const CT_BUFSIZ: usize = 1024;

/// C: `chartype.h` — printable character.
pub(crate) const CHTYPE_PRINT: i32 = 0;
/// C: `chartype.h` — control character inside the ASCII portion of the set.
pub(crate) const CHTYPE_ASCIICTL: i32 = -1;
/// C: `chartype.h` — a `\t`.
pub(crate) const CHTYPE_TAB: i32 = -2;
/// C: `chartype.h` — a `\n`.
pub(crate) const CHTYPE_NL: i32 = -3;
/// C: `chartype.h` — non-printable character.
pub(crate) const CHTYPE_NONPRINT: i32 = -4;

/// C: `#define VISUAL_WIDTH_MAX ((size_t)8)` — the widest expansion
/// `ct_visual_char` can produce, `\U+12345`.
pub(crate) const VISUAL_WIDTH_MAX: usize = 8;

/// C: `#define MB_FILL_CHAR ((wint_t)-1)` — the screen-image filler for the
/// second and later columns of a multi-column character.
///
/// This is why the wide side is `u32`: it is not a Unicode scalar value, and
/// it reaches `ct_chr_class` and `ct_visual_char` from `refresh.c`.
pub(crate) const MB_FILL_CHAR: u32 = u32::MAX;

/// The C reads to the first `L'\0'`; a Rust slice also has an end. Whichever
/// comes first wins. The C would read past the end of a slice that carries no
/// terminator, which is the one thing the slice form cannot express.
fn upto_nul_wide(s: &[u32]) -> &[u32] {
    &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
}

/// Byte twin of [`upto_nul_wide`].
fn upto_nul_byte(s: &[u8]) -> &[u8] {
    &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
}

// [spec:libedit:def:chartype.ct-conv-cbuff-resize-fn]
// [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]
/// Returns the C's status: 0 on success, -1 on allocation failure.
fn ct_conv_cbuff_resize(conv: &mut CtBufferT, csize: usize) -> i32 {
    // Step 1: grow-only.
    if csize <= conv.csize {
        return 0;
    }

    // Step 2: the C commits the new size before it knows the allocation
    // succeeded, and only the failure path below can observe that.
    conv.csize = csize;

    // Steps 3 and 5. `try_reserve` is the fallible `realloc`; `Vec::resize`
    // alone would abort the process on OOM and there would be no -1 to
    // return. ERR-encoding-08: the C's failure path frees the old buffer and
    // NULLs the pointer, dangling every pointer ever handed out into it. The
    // dangling is UB and is not reproduced — the borrow checker has already
    // ruled it out — but the *observable* outcome is: the struct returns to
    // its all-zero state and stays reusable.
    let additional = csize.saturating_sub(conv.cbuff.len());
    if conv.cbuff.try_reserve(additional).is_err() {
        conv.csize = 0;
        conv.cbuff = Vec::new();
        return -1;
    }

    // Step 4. The C leaves the added tail uninitialised; reading it would be
    // an indeterminate read, so the port zeroes it instead. Nothing observes
    // the difference: every caller writes before it reads.
    conv.cbuff.resize(csize, 0);
    0
}

// [spec:libedit:def:chartype.ct-conv-wbuff-resize-fn]
// [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn]
/// Returns the C's status: 0 on success, -1 on allocation failure.
fn ct_conv_wbuff_resize(conv: &mut CtBufferT, wsize: usize) -> i32 {
    // Step 1: grow-only.
    if wsize <= conv.wsize {
        return 0;
    }

    // Step 2.
    conv.wsize = wsize;

    // Steps 3 and 5, as in `ct_conv_cbuff_resize`. ERR-encoding-04: the C's
    // `wsize * sizeof(wchar_t)` is an unchecked multiplication that wraps
    // above `SIZE_MAX / sizeof(wchar_t)`; `try_reserve` does the
    // multiplication checked and reports `CapacityOverflow` as the same -1
    // the caller already handles, which is the errata's `define` disposition.
    let additional = wsize.saturating_sub(conv.wbuff.len());
    if conv.wbuff.try_reserve(additional).is_err() {
        conv.wsize = 0;
        conv.wbuff = Vec::new();
        return -1;
    }

    // Step 4.
    conv.wbuff.resize(wsize, 0);
    0
}

// [spec:libedit:def:chartype.ct-encode-string-fn]
// [spec:libedit:sem:chartype.ct-encode-string-fn]
/// `s` is the C's NUL-terminated `const wchar_t *`, `None` for its NULL.
///
/// The result borrows `conv.cbuff`, which is what the C hands back and what
/// makes its pointer valid only until the next call on the same `conv`.
/// `None` is the C's NULL return.
pub fn ct_encode_string<'a>(s: Option<&[u32]>, conv: &'a mut CtBufferT) -> Option<&'a [u8]> {
    // Step 1.
    let s = upto_nul_wide(s?);

    // Step 2. The C's `dst` is a pointer into `cbuff`; here it is the offset
    // the C recomputes as `dst - conv->cbuff` at the top of every pass.
    let mut used = 0usize;
    let mut i = 0usize;

    // Step 3.
    loop {
        // Headroom: the C's hard-coded 5, unrelated to `MB_CUR_MAX`. On a
        // virgin `conv` this fires immediately (`0 - 0 < 5`) and allocates the
        // first 1024 bytes. On -1 the buffer has already been destroyed by the
        // helper, so there is nothing to hand back.
        if conv.csize - used < 5 && ct_conv_cbuff_resize(conv, conv.csize + CT_BUFSIZ) == -1 {
            return None;
        }
        if i == s.len() {
            break;
        }

        // The literal 5 is the C's, not the real remaining space; the
        // headroom rule above is what makes it safe.
        let n = ct_encode_char(&mut conv.cbuff[used..used + 5], s[i]);
        if n == -1 {
            // ERR-encoding-12 (needs decision). The C calls `abort()` here —
            // a library killing the process because one character needed more
            // than 5 bytes. Defined here as the C's own NULL return, which
            // every caller of this function already handles.
            //
            // Reachable, and reachable in the C too: the UTF-8 this port
            // models is glibc's, whose `MB_CUR_MAX` is 6, so a character at or
            // above U+4000000 needs six bytes and fails this five-byte
            // headroom. The C aborts on exactly the same input. Below that
            // boundary the two charsets need at most five and four bytes
            // respectively, so ordinary text never reaches here.
            return None;
        }
        // A return of 0 is a character the locale cannot encode: `used` does
        // not advance, the character is silently dropped, and the output is
        // shorter than the input with no way for the caller to tell
        // (ERR-encoding-15, disposition `reproduce`).
        i += 1;
        used += n as usize;
    }

    // Step 4. The headroom rule guarantees the space for the terminator.
    conv.cbuff[used] = b'\0';
    Some(&conv.cbuff[..used])
}

// [spec:libedit:def:chartype.ct-decode-string-fn]
// [spec:libedit:sem:chartype.ct-decode-string-fn]
/// `s` is the C's NUL-terminated `const char *`, `None` for its NULL; the
/// result borrows `conv.wbuff`.
pub fn ct_decode_string<'a>(s: Option<&[u8]>, conv: &'a mut CtBufferT) -> Option<&'a [u32]> {
    let cs = locale::charset();

    // Step 1.
    let s = upto_nul_byte(s?);

    // Steps 2 and 3: `mbstowcs(NULL, s, 0)`, the sizing query. An invalid or
    // incomplete sequence *anywhere* rejects the whole string — no partial
    // decode, no replacement character, nothing written into `conv`. The C
    // leaves `errno == EILSEQ`; there is no errno here, so the three failure
    // causes (NULL, EILSEQ, OOM) are indistinguishable exactly as they are to
    // a C caller reading only the return value.
    let mut len = locale::mbstowcs_len(cs, s)?;

    // Step 4.
    len += 1;
    if conv.wsize < len && ct_conv_wbuff_resize(conv, len + CT_BUFSIZ) == -1 {
        return None;
    }

    // Step 5. The C passes the whole buffer size and discards the result: it
    // cannot fail (the sizing pass validated the same input under the same
    // locale) and cannot truncate (`wsize >= len` counts the terminator).
    let written = locale::mbstowcs(cs, &mut conv.wbuff[..conv.wsize], s)?;

    // Step 6.
    Some(&conv.wbuff[..written])
}

// [spec:libedit:def:chartype.ct-decode-argv-fn]
// [spec:libedit:sem:chartype.ct-decode-argv-fn]
/// The C's `argc` is the slice length, and a `None` element is one of its
/// NULL `argv` entries.
///
/// Each `Some(i)` in the result indexes `conv.wbuff`, where the decoded
/// strings are packed end to end — the C returns interior pointers into that
/// buffer, in an array the caller owns and must free. `None` elements mark
/// the slots the C left NULL.
pub(crate) fn ct_decode_argv(
    argv: &[Option<&[u8]>],
    conv: &mut CtBufferT,
) -> Option<Vec<Option<usize>>> {
    let cs = locale::charset();

    // Step 1. A byte total used as a `wchar_t` count: a safe over-estimate,
    // since a multibyte string never decodes to more wide characters than it
    // has bytes. NULL entries contribute nothing, here and in the loop below.
    //
    // ERR-encoding-05 (`argc + 1` wrapping for a negative `argc`) cannot
    // arise: the count is a slice length. The errata's `define — reject a
    // negative count` is discharged by the signature.
    let mut bufspace = 1usize;
    for a in argv {
        bufspace += a.map_or(0, |s| upto_nul_byte(s).len() + 1);
    }
    if conv.wsize < bufspace && ct_conv_wbuff_resize(conv, bufspace + CT_BUFSIZ) == -1 {
        return None;
    }

    // Step 2: the C's `el_calloc(argc + 1, ...)`. The trailing NULL slot is
    // not carried: a `Vec` knows its own length, so the terminator is the ABI
    // shim's business, not the core's. The result has exactly one entry per
    // `argv` element.
    let mut wargv: Vec<Option<usize>> = Vec::new();
    if wargv.try_reserve(argv.len()).is_err() {
        return None;
    }

    // Step 3.
    let mut p = 0usize;
    for a in argv {
        let Some(bytes) = *a else {
            // The C deliberately does not hand a NULL source to `mbstowcs`,
            // and consumes no buffer space for it.
            wargv.push(None);
            continue;
        };
        wargv.push(Some(p));
        // `bufspace` is the C's limit argument, not the buffer end; the two
        // stay consistent because `p + bufspace` is invariant and no greater
        // than `wsize`.
        let bytes = upto_nul_byte(bytes);
        // -1 from `mbstowcs` fails the whole call: the pointer array is
        // dropped and there is no partial result. `conv.wbuff` keeps whatever
        // earlier elements decoded into it, which is garbage no caller can
        // reach.
        let n = locale::mbstowcs(cs, &mut conv.wbuff[p..p + bufspace], bytes)?;
        let n = n + 1; // include the `L'\0'` in the count
        bufspace -= n;
        p += n;
    }

    // Step 4: the C's final NULL slot — see step 2 for why it is absent.
    Some(wargv)
}

// [spec:libedit:def:chartype.ct-enc-width-fn]
// [spec:libedit:sem:chartype.ct-enc-width-fn]
pub(crate) fn ct_enc_width(c: u32) -> usize {
    // The C measures with `wcrtomb` from a freshly zeroed `mbstate_t`: this
    // is the *context-free* width, and 0 means "no representation in this
    // locale" (ERR-encoding-15, disposition `reproduce` — the 0 crosses the C
    // ABI as a byte count and must not become a replacement-character width).
    // Both charsets modelled here are stateless, so the initial-state
    // qualifier costs nothing.
    locale::enc_width(locale::charset(), c).unwrap_or(0)
}

// [spec:libedit:def:chartype.ct-encode-char-fn]
// [spec:libedit:sem:chartype.ct-encode-char-fn]
/// `dst` carries the C's `len`: `ct_encode_string` passes the five bytes the
/// C passes, not the whole remaining buffer.
///
/// Returns the C's `ssize_t`: bytes written, 0 if `c` has no representation
/// in this locale, -1 if `dst` is too short.
pub(crate) fn ct_encode_char(dst: &mut [u8], c: u32) -> isize {
    // ERR-encoding-03 (`define — reject len == 0 outright`). `ct_enc_width`
    // returns 0 for an unencodable `c`, so the C's `len < width` test is
    // false for every `len` including 0 and `wctomb` is handed a pointer with
    // no guaranteed space. Rejecting an empty `dst` first is the defined
    // behaviour; it is only reachable for an unencodable `c`, since any
    // encodable one fails the width test below anyway.
    if dst.is_empty() {
        return -1;
    }

    // Step 1.
    if dst.len() < ct_enc_width(c) {
        return -1;
    }

    // Steps 2-4. ERR-encoding-02 (`define — encode once and use the length
    // actually produced`): the C bounds-checks with `wcrtomb` from the
    // initial state and then writes with `wctomb`, which continues from
    // libc's global state, so a stateful encoding can write past `len`. Here
    // the encoder is stateless and writes into a bounded slice, so the check
    // and the write cannot disagree.
    match locale::encode(locale::charset(), dst, c) {
        Some(n) => n as isize,
        // Unencodable. The C resets libc's global encoder state here
        // (`wctomb(NULL, L'\0')`); this port holds no such state
        // (ERR-encoding-16, disposition `fix`). Nothing is written and the
        // caller drops the character.
        None => 0,
    }
}

// [spec:libedit:def:chartype.ct-visual-string-fn]
// [spec:libedit:sem:chartype.ct-visual-string-fn]
/// The result borrows `conv.wbuff`, as the C's returned pointer does.
pub(crate) fn ct_visual_string<'a>(
    s: Option<&[u32]>,
    conv: &'a mut CtBufferT,
) -> Option<&'a [u32]> {
    // Step 1.
    let s = upto_nul_wide(s?);

    // Step 2. Grow-only, so a `conv` already larger than 1024 keeps its size
    // for the space calculations below.
    if ct_conv_wbuff_resize(conv, CT_BUFSIZ) == -1 {
        return None;
    }

    // Step 3.
    let mut dst = 0usize;
    let mut i = 0usize;
    while i < s.len() {
        // The genuine remaining capacity, unlike the fixed 5 that
        // `ct_encode_string` passes on the byte side.
        let used = ct_visual_char(&mut conv.wbuff[dst..conv.wsize], s[i]);
        if used > 0 {
            i += 1;
            dst += used as usize;
            continue;
        }
        if used == 0 {
            // Unreachable: `ct_chr_class` is total over the four arms that
            // return non-zero, so the C's dead "any other class" arm
            // (ERR-encoding-28) never fires. The C would spin here forever;
            // skipping the character keeps the loop total, as
            // `sem:chartype.ct-visual-string-fn` asks.
            i += 1;
            continue;
        }
        // -1: not enough room for this character's expansion. Grow and retry
        // the *same* source character — `i` does not advance. Progress is
        // guaranteed because one expansion never needs more than
        // `VISUAL_WIDTH_MAX` cells and each growth adds 1024. The C
        // re-derives `dst` from the possibly-moved block here; an offset
        // needs no re-deriving.
        if ct_conv_wbuff_resize(conv, conv.wsize + CT_BUFSIZ) == -1 {
            return None;
        }
    }

    // Step 4, the C's `/* sigh */`: the loop can leave `dst` exactly at the
    // end of the buffer, and the terminator needs one more cell. It can never
    // be past the end, because `ct_visual_char` was given the true remaining
    // length.
    if dst >= conv.wsize && ct_conv_wbuff_resize(conv, conv.wsize + CT_BUFSIZ) == -1 {
        return None;
    }

    // Step 5.
    conv.wbuff[dst] = 0;
    Some(&conv.wbuff[..dst])
}

// [spec:libedit:def:chartype.ct-visual-width-fn]
// [spec:libedit:sem:chartype.ct-visual-width-fn]
pub(crate) fn ct_visual_width(c: u32) -> i32 {
    match ct_chr_class(c) {
        CHTYPE_ASCIICTL => 2, // ^@ ^? etc.
        // ERR-encoding-28: dead. Every caller intercepts tabs first —
        // `re_addc` expands to the next tab stop itself and `re_goto_bottom`
        // has its own `CHTYPE_TAB` arm — but 1 is what the function returns,
        // and it disagrees with `ct_visual_char`'s two cells for `^I`.
        CHTYPE_TAB => 1,
        // Likewise intercepted by the callers, and likewise disagreeing with
        // `ct_visual_char`'s `^J` (ERR-encoding-25).
        CHTYPE_NL => 0,
        // ERR-encoding-17, disposition `reproduce`: `wcwidth`'s -1 passes
        // straight through, and `refresh.c` adds it into its column
        // accumulator. Reachable whenever the locale's `iswprint` and
        // `wcwidth` disagree. Not screened, because screening it would change
        // the rendered geometry.
        CHTYPE_PRINT => locale::wcwidth(locale::charset(), c),
        // The only two possible values, which is why `VISUAL_WIDTH_MAX` is 8.
        CHTYPE_NONPRINT => {
            if c > 0xffff {
                8 // \U+12345
            } else {
                7 // \U+1234
            }
        }
        _ => 0, // should not happen: `ct_chr_class` is total
    }
}

// [spec:libedit:def:chartype.ct-visual-char-fn]
// [spec:libedit:sem:chartype.ct-visual-char-fn]
/// `dst` carries the C's `len`, a `wchar_t` count. Returns the C's
/// `ssize_t`: characters written, or -1 if `dst` is too short.
pub(crate) fn ct_visual_char(dst: &mut [u32], c: u32) -> isize {
    match ct_chr_class(c) {
        CHTYPE_TAB | CHTYPE_NL | CHTYPE_ASCIICTL => {
            if dst.len() < 2 {
                return -1; // insufficient space
            }
            dst[0] = u32::from(b'^');
            dst[1] = if c == 0o177 {
                u32::from(b'?') // DEL -> ^?
            } else {
                // "Uncontrolify it". `ct_chr_class` admits anything below
                // 0x100 that `iswcntrl` accepts, so in a UTF-8 or Latin-1
                // locale the C1 range U+0080..U+009F lands here too and comes
                // out as `^` followed by U+00C0..U+00DF — U+0085 renders as
                // `^Å`. That is not a caret escape in any useful sense, and
                // it is what the C emits.
                c | 0o100
            };
            2
        }
        CHTYPE_PRINT => {
            if dst.is_empty() {
                return -1; // insufficient space
            }
            // Exactly one cell, whatever `wcwidth` says about its columns;
            // the display layer pads the rest with `MB_FILL_CHAR`.
            dst[0] = c;
            1
        }
        CHTYPE_NONPRINT => {
            // ERR-encoding-18 (`define`): the C compares `(ssize_t)len`
            // against `ct_visual_width(c)`, which makes any `len` above
            // `SSIZE_MAX` compare as negative and spuriously return -1. The
            // width in this arm is only ever 7 or 8, so the comparison is
            // done in `usize` and no negative width can be formed.
            let width = if c > 0xffff { 8 } else { 7 };
            if dst.len() < width {
                return -1; // insufficient space
            }
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            let digit = |shift: u32| u32::from(HEX[((c >> shift) & 0xf) as usize]);
            dst[0] = u32::from(b'\\');
            dst[1] = u32::from(b'U');
            dst[2] = u32::from(b'+');
            if c > 0xffff {
                // ERR-encoding-13 (needs decision), reproduced per
                // `plan/decisions/conformance-policy.md`: defined behaviour,
                // defects included. Only five hex digits are ever emitted, so
                // every bit above 0x0FFFFF is lost and U+10FFFF renders as
                // `\U+0FFFF` — plane 16 displayed as plane 0. Widening the
                // field would change `ct_visual_width` and every column
                // calculation downstream, so it is not a local fix.
                dst[3] = digit(16);
                dst[4] = digit(12);
                dst[5] = digit(8);
                dst[6] = digit(4);
                dst[7] = digit(0);
                8
            } else {
                dst[3] = digit(12);
                dst[4] = digit(8);
                dst[5] = digit(4);
                dst[6] = digit(0);
                7
            }
            // The C's `/*FALLTHROUGH*/` after this `return` is dead and
            // misleading (ERR-encoding-28).
        }
        // ERR-encoding-28: unreachable, and `ct_visual_string` would spin on
        // it if it were not.
        _ => 0,
    }
}

// [spec:libedit:def:chartype.ct-chr-class-fn]
// [spec:libedit:sem:chartype.ct-chr-class-fn]
/// Returns one of the C's `CHTYPE_*` constants.
pub(crate) fn ct_chr_class(c: u32) -> i32 {
    let cs = locale::charset();
    // The order is load-bearing: tab and newline also satisfy the control
    // test, which is why they are peeled off first.
    if c == u32::from(b'\t') {
        CHTYPE_TAB
    } else if c == u32::from(b'\n') {
        CHTYPE_NL
    } else if c < 0x100 && locale::iswcntrl(cs, c) {
        // The `< 0x100` guard confines this class to the first 256 code
        // points; a control character above U+00FF falls through to the
        // printability test instead.
        CHTYPE_ASCIICTL
    } else if locale::iswprint(cs, c) {
        CHTYPE_PRINT
    } else {
        // ERR-encoding-01 (`define — classify non-scalar cell values
        // explicitly rather than passing them to a locale predicate`). The C
        // hands a negative `wchar_t` to `iswcntrl`, which is UB unless the
        // value is exactly `WEOF`; it is reachable, because `MB_FILL_CHAR` is
        // `(wint_t)-1` and `refresh.c` classifies screen-image cells that
        // hold it. Here the predicates are total over `u32` and answer false
        // for everything that is not a code point — surrogates,
        // `MB_FILL_CHAR`, anything above U+10FFFF — so those land here, which
        // is also what glibc happens to do with `WEOF`.
        CHTYPE_NONPRINT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> CtBufferT {
        // The C's `calloc`ed `ct_buffer_t`: the valid empty state.
        CtBufferT {
            cbuff: Vec::new(),
            csize: 0,
            wbuff: Vec::new(),
            wsize: 0,
        }
    }

    // Every assertion below is either charset-independent (ASCII text and the
    // C0 controls classify the same in both) or passes the charset in
    // explicitly, so the process environment cannot change the outcome.

    #[test]
    fn resize_is_grow_only_and_keeps_the_len_invariant() {
        let mut conv = empty();
        assert_eq!(ct_conv_cbuff_resize(&mut conv, CT_BUFSIZ), 0);
        assert_eq!(conv.csize, CT_BUFSIZ);
        assert_eq!(conv.cbuff.len(), conv.csize);
        assert_eq!(ct_conv_cbuff_resize(&mut conv, 4), 0);
        assert_eq!(conv.csize, CT_BUFSIZ);

        assert_eq!(ct_conv_wbuff_resize(&mut conv, 7), 0);
        assert_eq!(conv.wsize, 7);
        assert_eq!(conv.wbuff.len(), 7);
    }

    #[test]
    fn resize_reports_an_impossible_allocation_rather_than_wrapping() {
        // ERR-encoding-04: the C multiplies unchecked.
        let mut conv = empty();
        assert_eq!(ct_conv_wbuff_resize(&mut conv, usize::MAX), -1);
        // ERR-encoding-08: back to the all-zero state, still usable.
        assert_eq!(conv.wsize, 0);
        assert!(conv.wbuff.is_empty());
        assert_eq!(ct_conv_wbuff_resize(&mut conv, 8), 0);
    }

    #[test]
    fn chr_class_peels_tab_and_newline_before_the_control_test() {
        assert_eq!(ct_chr_class(u32::from(b'\t')), CHTYPE_TAB);
        assert_eq!(ct_chr_class(u32::from(b'\n')), CHTYPE_NL);
        assert_eq!(ct_chr_class(0), CHTYPE_ASCIICTL);
        assert_eq!(ct_chr_class(0x1B), CHTYPE_ASCIICTL);
        assert_eq!(ct_chr_class(0x7F), CHTYPE_ASCIICTL);
        assert_eq!(ct_chr_class(u32::from(b'a')), CHTYPE_PRINT);
        // ERR-encoding-01: non-scalar values are classified, not passed to a
        // locale predicate.
        assert_eq!(ct_chr_class(MB_FILL_CHAR), CHTYPE_NONPRINT);
        assert_eq!(ct_chr_class(0xD800), CHTYPE_NONPRINT);
    }

    #[test]
    fn visual_char_writes_caret_forms() {
        let mut dst = [0u32; VISUAL_WIDTH_MAX];
        assert_eq!(ct_visual_char(&mut dst, 0), 2);
        assert_eq!(&dst[..2], &[u32::from(b'^'), u32::from(b'@')]);
        assert_eq!(ct_visual_char(&mut dst, u32::from(b'\t')), 2);
        assert_eq!(&dst[..2], &[u32::from(b'^'), u32::from(b'I')]);
        assert_eq!(ct_visual_char(&mut dst, u32::from(b'\n')), 2);
        assert_eq!(&dst[..2], &[u32::from(b'^'), u32::from(b'J')]);
        assert_eq!(ct_visual_char(&mut dst, 0x7F), 2);
        assert_eq!(&dst[..2], &[u32::from(b'^'), u32::from(b'?')]);
        assert_eq!(ct_visual_char(&mut dst, u32::from(b'x')), 1);
        assert_eq!(dst[0], u32::from(b'x'));
        // Insufficient space, per class.
        assert_eq!(ct_visual_char(&mut dst[..1], 0x1B), -1);
        assert_eq!(ct_visual_char(&mut [], u32::from(b'x')), -1);
    }

    #[test]
    fn visual_char_truncates_the_hex_field_to_five_digits() {
        // ERR-encoding-13, reproduced: U+10FFFF renders as plane 0.
        let mut dst = [0u32; VISUAL_WIDTH_MAX];
        assert_eq!(ct_visual_char(&mut dst, 0xD800), 7);
        let text: String = dst[..7]
            .iter()
            .map(|&c| char::from_u32(c).unwrap())
            .collect();
        assert_eq!(text, "\\U+D800");
        assert_eq!(ct_visual_width(0xD800), 7);

        assert_eq!(ct_visual_char(&mut dst, MB_FILL_CHAR), 8);
        let text: String = dst[..8]
            .iter()
            .map(|&c| char::from_u32(c).unwrap())
            .collect();
        assert_eq!(text, "\\U+FFFFF");
        assert_eq!(ct_visual_width(MB_FILL_CHAR), 8);
        assert_eq!(ct_visual_char(&mut dst[..7], MB_FILL_CHAR), -1);
    }

    #[test]
    fn visual_width_disagrees_with_visual_char_for_tab_and_newline() {
        // ERR-encoding-25.
        assert_eq!(ct_visual_width(u32::from(b'\t')), 1);
        assert_eq!(ct_visual_width(u32::from(b'\n')), 0);
        assert_eq!(ct_visual_width(0x1B), 2);
    }

    #[test]
    fn visual_string_expands_and_terminates() {
        let mut conv = empty();
        let s: Vec<u32> = "a\tb".chars().map(u32::from).collect();
        let out = ct_visual_string(Some(&s), &mut conv).unwrap();
        let text: String = out.iter().map(|&c| char::from_u32(c).unwrap()).collect();
        // `out` borrows `conv`, so its length has to be taken before `conv`
        // can be inspected again — the C's "valid until the next call"
        // contract, now enforced.
        let used = out.len();
        assert_eq!(text, "a^Ib");
        assert_eq!(conv.wbuff[used], 0);
        assert!(conv.wsize >= CT_BUFSIZ);

        assert!(ct_visual_string(None, &mut conv).is_none());
        assert!(ct_visual_string(Some(&[]), &mut conv).unwrap().is_empty());
    }

    #[test]
    fn visual_string_grows_until_the_expansion_fits() {
        let mut conv = empty();
        // 1024 controls expand to 2048 cells plus a terminator, so the
        // retry-after-growth path runs.
        let s = vec![0x01u32; CT_BUFSIZ];
        let out = ct_visual_string(Some(&s), &mut conv).unwrap();
        assert_eq!(out.len(), 2 * CT_BUFSIZ);
        assert!(
            out.chunks(2)
                .all(|p| p == [u32::from(b'^'), u32::from(b'A')])
        );
    }

    #[test]
    fn encode_and_decode_round_trip_ascii() {
        let mut conv = empty();
        let s: Vec<u32> = "hello".chars().map(u32::from).collect();
        assert_eq!(ct_encode_string(Some(&s), &mut conv).unwrap(), b"hello");
        // The terminator is past the end of the returned slice.
        assert_eq!(conv.cbuff[5], 0);
        assert!(ct_encode_string(None, &mut conv).is_none());

        assert_eq!(ct_decode_string(Some(b"hello"), &mut conv).unwrap(), &s[..]);
        assert_eq!(conv.wbuff[5], 0);
        assert!(ct_decode_string(None, &mut conv).is_none());
        // Encoding does not touch the wide half, which is what
        // `terminal_telltc` leans on.
        assert_eq!(&conv.wbuff[..5], &s[..]);
    }

    #[test]
    fn decode_argv_packs_end_to_end_and_keeps_null_slots() {
        let mut conv = empty();
        let argv = [Some(&b"one"[..]), None, Some(&b"two"[..])];
        let out = ct_decode_argv(&argv, &mut conv).unwrap();
        assert_eq!(out, vec![Some(0), None, Some(4)]);
        assert_eq!(&conv.wbuff[..8], &[111, 110, 101, 0, 116, 119, 111, 0]);
        assert_eq!(ct_decode_argv(&[], &mut conv).unwrap(), Vec::new());
    }

    #[test]
    fn encode_char_rejects_an_empty_destination() {
        // ERR-encoding-03.
        assert_eq!(ct_encode_char(&mut [], u32::from(b'a')), -1);
        assert_eq!(ct_encode_char(&mut [], MB_FILL_CHAR), -1);
        let mut dst = [0u8; 4];
        assert_eq!(ct_encode_char(&mut dst, u32::from(b'a')), 1);
        assert_eq!(dst[0], b'a');
        // Unencodable in either charset: dropped, nothing written.
        assert_eq!(ct_encode_char(&mut dst, 0xD800), 0);
        assert_eq!(ct_enc_width(0xD800), 0);
        assert_eq!(ct_enc_width(0), 1);
    }

    // The locale layer's own tests — the charset parser, the codec, the
    // predicates and the width tables — live with it in `crate::locale`.
}
