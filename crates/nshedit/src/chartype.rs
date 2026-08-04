//! Ported from `src/chartype.c`; rules live in
//! `docs/spec/port/src/chartype.md`.
//!
//! # `LC_CTYPE` without libc
//!
//! Every rule in this file is written against libc: `mbstowcs`, `wctomb`,
//! `wcrtomb`, `iswcntrl`, `iswprint` and `wcwidth`, each reading the process's
//! `LC_CTYPE`. `plan/decisions/no-c-ffi.md` bars linking libc, so the port has
//! to supply them, and the `locale` module below is that supply. It is
//! deliberately narrow, and the narrowness is a port decision, not an
//! oversight:
//!
//! - **Two charsets are modelled**, `Utf8` and `Ascii`. `Ascii` is the
//!   C/POSIX locale, whose charmap on glibc is ANSI_X3.4-1968: `wcrtomb`
//!   fails above U+007F and `iswprint` is true only for U+0020..U+007E,
//!   exactly as `sem:chartype.ct-chr-class-fn` describes. Any other named
//!   codeset (ISO-8859-1, EUC-JP, the stateful ISO-2022 family) falls back to
//!   `Ascii`, which renders the affected characters as `\U+nnnn` rather than
//!   mis-encoding them. **Consequence: no stateful encoding exists in this
//!   port**, which is what makes `ERR-encoding-02`, `ERR-encoding-12` and
//!   `ERR-encoding-16` unreachable here.
//! - **The charset is resolved once from the environment** (`LC_ALL`,
//!   `LC_CTYPE`, `LANG`, in POSIX order; C/POSIX if none is set), because
//!   there is no libc global to read. This diverges from the C in one visible
//!   way: a program that never calls `setlocale(LC_CTYPE, "")` runs in the C
//!   locale no matter what the environment says, and `sem:el.el-init-fn`
//!   depends on that. The port behaves as if the application had always
//!   called `setlocale(LC_CTYPE, "")`.
//! - **`wcwidth` is table-driven and approximate.** The tables cover
//!   combining marks, format characters and the East Asian wide blocks; they
//!   are not a Unicode database. `iswprint` cannot test "unassigned" at all,
//!   so it errs printable where glibc would say no.
//!
//! `locale` is `pub(crate)` because it is not really this module's property:
//! `refresh.c` calls `wcwidth` directly, `map.c` calls `iswprint`, `search.c`
//! and `vis.c` call other `LC_CTYPE` predicates. It wants hoisting into a
//! module of its own once a second consumer appears.
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
            // every caller of this function already handles. Unreachable in
            // both charsets this port models (UTF-8 needs at most 4 bytes,
            // C/POSIX 1), so nothing observable changes today; it becomes
            // reachable only if a stateful encoding is ever added.
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

/// The `LC_CTYPE` queries the C makes through libc, reimplemented; see the
/// module documentation for what is and is not modelled.
///
/// `pub(crate)` because `refresh.c`, `map.c`, `search.c` and `vis.c` reach for
/// the same predicates. The active charset is passed explicitly at every call
/// so that the queries are visible as locale queries — and so that they are
/// testable without mutating the process environment.
pub(crate) mod locale {
    use std::cmp::Ordering;
    use std::sync::OnceLock;

    /// The subset of `LC_CTYPE` codesets this port implements.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) enum Charset {
        /// The C/POSIX locale: ANSI_X3.4-1968, one byte, ASCII only.
        Ascii,
        /// UTF-8, as glibc's `*.UTF-8` locales define it.
        Utf8,
    }

    /// The process's `LC_CTYPE`, resolved once. See the module documentation:
    /// there is no libc global to read, so this reads the environment and
    /// therefore behaves as if the application had called
    /// `setlocale(LC_CTYPE, "")`.
    pub(crate) fn charset() -> Charset {
        static CHARSET: OnceLock<Charset> = OnceLock::new();
        *CHARSET.get_or_init(|| {
            for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
                let value = std::env::var(var).unwrap_or_default();
                if !value.is_empty() {
                    return charset_of(&value);
                }
            }
            Charset::Ascii
        })
    }

    /// Parses a POSIX locale name — `language[_TERRITORY][.codeset][@modifier]`
    /// — for its codeset. Anything not recognisable as UTF-8 is treated as the
    /// C locale, which renders the affected characters `\U+nnnn` rather than
    /// mis-encoding them.
    pub(super) fn charset_of(spec: &str) -> Charset {
        let spec = spec.split('@').next().unwrap_or("");
        let codeset = spec.split_once('.').map_or("", |(_, cs)| cs);
        let normalised: String = codeset
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        if normalised == "utf8" {
            Charset::Utf8
        } else {
            Charset::Ascii
        }
    }

    /// C: `MB_CUR_MAX`. `el.c` and `hist.c` test it against 1 to decide
    /// whether history can be narrow.
    pub(crate) fn mb_cur_max(cs: Charset) -> usize {
        match cs {
            Charset::Ascii => 1,
            Charset::Utf8 => 4,
        }
    }

    /// C: `wcrtomb` from the initial conversion state, length only. `None` is
    /// its `(size_t)-1`/`EILSEQ`.
    ///
    /// `c == 0` answers 1 — `wcrtomb` writes the null byte — so this is not a
    /// string-length primitive.
    pub(crate) fn enc_width(cs: Charset, c: u32) -> Option<usize> {
        match cs {
            Charset::Ascii => (c < 0x80).then_some(1),
            // glibc's UTF-8 converter rejects surrogates and anything above
            // U+10FFFF outright.
            Charset::Utf8 => match c {
                0x0000..=0x007F => Some(1),
                0x0080..=0x07FF => Some(2),
                0xD800..=0xDFFF => None,
                0x0800..=0xFFFF => Some(3),
                0x10000..=0x10FFFF => Some(4),
                _ => None,
            },
        }
    }

    /// C: `wctomb`. Writes `c` into `dst` and returns the byte count; `None`
    /// for a character the charset cannot represent, or for a `dst` too short
    /// to hold it (which the caller has already excluded via [`enc_width`]).
    ///
    /// Unlike `wctomb` this carries no state, so it is reentrant and needs no
    /// reset on the failure path (ERR-encoding-16).
    pub(crate) fn encode(cs: Charset, dst: &mut [u8], c: u32) -> Option<usize> {
        let width = enc_width(cs, c)?;
        if dst.len() < width {
            return None;
        }
        match width {
            1 => dst[0] = c as u8,
            2 => {
                dst[0] = 0xC0 | (c >> 6) as u8;
                dst[1] = 0x80 | (c & 0x3F) as u8;
            }
            3 => {
                dst[0] = 0xE0 | (c >> 12) as u8;
                dst[1] = 0x80 | ((c >> 6) & 0x3F) as u8;
                dst[2] = 0x80 | (c & 0x3F) as u8;
            }
            _ => {
                dst[0] = 0xF0 | (c >> 18) as u8;
                dst[1] = 0x80 | ((c >> 12) & 0x3F) as u8;
                dst[2] = 0x80 | ((c >> 6) & 0x3F) as u8;
                dst[3] = 0x80 | (c & 0x3F) as u8;
            }
        }
        Some(width)
    }

    /// C: one `mbrtowc` step from the initial state. Returns the character
    /// and the bytes consumed; `None` for an invalid or incomplete sequence.
    ///
    /// Strict, as glibc is: overlong forms, surrogate encodings, anything
    /// above U+10FFFF and truncated sequences are all `EILSEQ`.
    fn decode_one(cs: Charset, s: &[u8]) -> Option<(u32, usize)> {
        let b0 = *s.first()?;
        if cs == Charset::Ascii {
            return (b0 < 0x80).then_some((u32::from(b0), 1));
        }
        let (len, mut acc) = match b0 {
            0x00..=0x7F => return Some((u32::from(b0), 1)),
            0xC2..=0xDF => (2, u32::from(b0 & 0x1F)),
            0xE0..=0xEF => (3, u32::from(b0 & 0x0F)),
            0xF0..=0xF4 => (4, u32::from(b0 & 0x07)),
            // 0x80..=0xC1 is a stray continuation byte or an overlong
            // two-byte lead; 0xF5..=0xFF is above U+10FFFF.
            _ => return None,
        };
        if s.len() < len {
            return None;
        }
        for &b in &s[1..len] {
            if b & 0xC0 != 0x80 {
                return None;
            }
            acc = (acc << 6) | u32::from(b & 0x3F);
        }
        let overlong = (len == 3 && acc < 0x800) || (len == 4 && acc < 0x10000);
        if overlong || acc > 0x10FFFF || (0xD800..=0xDFFF).contains(&acc) {
            return None;
        }
        Some((acc, len))
    }

    /// C: `mbstowcs(NULL, s, 0)` — how many wide characters `s` would
    /// produce, not counting the terminator. `None` is its `(size_t)-1`.
    pub(crate) fn mbstowcs_len(cs: Charset, s: &[u8]) -> Option<usize> {
        let mut at = 0usize;
        let mut count = 0usize;
        while at < s.len() {
            let (_, used) = decode_one(cs, &s[at..])?;
            at += used;
            count += 1;
        }
        Some(count)
    }

    /// C: `mbstowcs(dst, s, dst.len())`. `s` is the source without its
    /// terminator; the `L'\0'` is written when it fits, as the C's `n`-limited
    /// form does. Returns the count of wide characters written, terminator
    /// excluded; `None` is its `(size_t)-1`.
    pub(crate) fn mbstowcs(cs: Charset, dst: &mut [u32], s: &[u8]) -> Option<usize> {
        let mut at = 0usize;
        let mut count = 0usize;
        while at < s.len() && count < dst.len() {
            let (c, used) = decode_one(cs, &s[at..])?;
            dst[count] = c;
            at += used;
            count += 1;
        }
        if count < dst.len() {
            dst[count] = 0;
        }
        Some(count)
    }

    /// C: `iswcntrl`. Total over `u32`: values that are not code points
    /// answer false rather than being undefined (ERR-encoding-01).
    pub(crate) fn iswcntrl(cs: Charset, c: u32) -> bool {
        match cs {
            Charset::Ascii => c < 0x20 || c == 0x7F,
            // glibc's `cntrl` class is the C0 and C1 controls plus the `Zl`
            // and `Zp` separators. The last two are above U+00FF, so
            // `ct_chr_class`'s `c < 0x100` guard hides them; they are here for
            // the other callers of this predicate.
            Charset::Utf8 => c < 0x20 || (0x7F..=0x9F).contains(&c) || c == 0x2028 || c == 0x2029,
        }
    }

    /// C: `iswprint`. Total over `u32` (ERR-encoding-01).
    ///
    /// The C locale answer is exact. The UTF-8 answer is glibc's rule —
    /// printable is anything with a Unicode name that is not a control —
    /// minus the part that needs a character database: unassigned code points
    /// have no name and glibc calls them unprintable, whereas this says
    /// printable. Surrogates, noncharacters and out-of-range values are
    /// excluded, which covers every value the screen image can hold.
    pub(crate) fn iswprint(cs: Charset, c: u32) -> bool {
        match cs {
            // In the C locale everything above U+007E is unprintable, so
            // every non-ASCII character renders as `\U+nnnn`.
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
    /// double-width one, and **-1 for a character the locale calls
    /// unprintable**. `ct_visual_width` passes that -1 straight through
    /// (ERR-encoding-17).
    ///
    /// The tables below are an approximation, not a Unicode database; see the
    /// module documentation.
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
        if in_ranges(ZERO_WIDTH, c) {
            0
        } else if in_ranges(WIDE, c) {
            2
        } else {
            1
        }
    }

    /// Both tables are sorted and disjoint, which the tests assert.
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

    /// Combining marks and format characters: one cell in the screen image,
    /// zero terminal columns.
    #[rustfmt::skip]
    pub(super) const ZERO_WIDTH: &[(u32, u32)] = &[
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
    /// double-width: one cell in the screen image, two terminal columns, the
    /// second of which the display layer fills with `MB_FILL_CHAR`.
    #[rustfmt::skip]
    pub(super) const WIDE: &[(u32, u32)] = &[
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
}

#[cfg(test)]
mod tests {
    use super::locale::Charset;
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

    #[test]
    fn charset_is_parsed_from_a_posix_locale_name() {
        for name in ["en_US.UTF-8", "C.utf8", "de_DE.utf-8@euro"] {
            assert_eq!(locale::charset_of(name), Charset::Utf8, "{name}");
        }
        for name in ["C", "POSIX", "en_US", "en_US.ISO-8859-1", "ja_JP.eucJP"] {
            assert_eq!(locale::charset_of(name), Charset::Ascii, "{name}");
        }
    }

    #[test]
    fn utf8_conversion_is_strict() {
        let cs = Charset::Utf8;
        assert_eq!(locale::mbstowcs_len(cs, "€".as_bytes()), Some(1));
        assert_eq!(locale::enc_width(cs, 0x20AC), Some(3));
        let mut dst = [0u8; 4];
        assert_eq!(locale::encode(cs, &mut dst, 0x20AC), Some(3));
        assert_eq!(&dst[..3], "€".as_bytes());
        // Overlong, surrogate, truncated, stray continuation: all EILSEQ.
        for bad in [
            &b"\xC0\xAF"[..],
            &b"\xED\xA0\x80"[..],
            &b"\xE2\x82"[..],
            &b"\x80"[..],
        ] {
            assert_eq!(locale::mbstowcs_len(cs, bad), None);
        }
        // The C locale rejects every byte above 0x7F.
        assert_eq!(locale::mbstowcs_len(Charset::Ascii, "é".as_bytes()), None);
        assert_eq!(locale::enc_width(Charset::Ascii, 0xE9), None);
    }

    #[test]
    fn locale_predicates_follow_the_charset() {
        // C1 controls are `iswcntrl` in a UTF-8 locale and below 0x100, so
        // they take the caret arm: U+0085 renders as `^` U+00C5.
        assert!(locale::iswcntrl(Charset::Utf8, 0x85));
        assert!(!locale::iswcntrl(Charset::Ascii, 0x85));
        // In the C locale nothing above U+007E is printable.
        assert!(!locale::iswprint(Charset::Ascii, 0xE9));
        assert!(locale::iswprint(Charset::Utf8, 0xE9));
        assert!(!locale::iswprint(Charset::Utf8, MB_FILL_CHAR));
        assert_eq!(locale::wcwidth(Charset::Utf8, 0x4E00), 2);
        assert_eq!(locale::wcwidth(Charset::Utf8, 0x0301), 0);
        assert_eq!(locale::wcwidth(Charset::Utf8, u32::from(b'a')), 1);
        // ERR-encoding-17: the -1 `ct_visual_width` passes through.
        assert_eq!(locale::wcwidth(Charset::Ascii, 0xE9), -1);
    }

    #[test]
    fn width_tables_are_sorted_and_disjoint() {
        for table in [locale::ZERO_WIDTH, locale::WIDE] {
            for pair in table.windows(2) {
                assert!(pair[0].0 <= pair[0].1);
                assert!(pair[0].1 < pair[1].0, "{:x?} then {:x?}", pair[0], pair[1]);
            }
        }
    }
}
