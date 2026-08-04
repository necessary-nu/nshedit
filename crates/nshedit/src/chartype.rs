//! Ported from `src/chartype.c`; rules live in
//! `docs/spec/port/src/chartype.md`.

// The function bodies are still `todo!()`, so every parameter reads as
// unused. Remove this once the translations land.
#![allow(unused_variables)]

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

// [spec:libedit:def:chartype.ct-conv-cbuff-resize-fn]
// [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]
/// Returns the C's status: 0 on success, -1 on allocation failure.
fn ct_conv_cbuff_resize(conv: &mut CtBufferT, csize: usize) -> i32 {
    todo!()
}

// [spec:libedit:def:chartype.ct-conv-wbuff-resize-fn]
// [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn]
/// Returns the C's status: 0 on success, -1 on allocation failure.
fn ct_conv_wbuff_resize(conv: &mut CtBufferT, wsize: usize) -> i32 {
    todo!()
}

// [spec:libedit:def:chartype.ct-encode-string-fn]
// [spec:libedit:sem:chartype.ct-encode-string-fn]
/// `s` is the C's NUL-terminated `const wchar_t *`, `None` for its NULL.
///
/// The result borrows `conv.cbuff`, which is what the C hands back and what
/// makes its pointer valid only until the next call on the same `conv`.
/// `None` is the C's NULL return.
pub fn ct_encode_string<'a>(s: Option<&[u32]>, conv: &'a mut CtBufferT) -> Option<&'a [u8]> {
    todo!()
}

// [spec:libedit:def:chartype.ct-decode-string-fn]
// [spec:libedit:sem:chartype.ct-decode-string-fn]
/// `s` is the C's NUL-terminated `const char *`, `None` for its NULL; the
/// result borrows `conv.wbuff`.
pub fn ct_decode_string<'a>(s: Option<&[u8]>, conv: &'a mut CtBufferT) -> Option<&'a [u32]> {
    todo!()
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
    todo!()
}

// [spec:libedit:def:chartype.ct-enc-width-fn]
// [spec:libedit:sem:chartype.ct-enc-width-fn]
pub(crate) fn ct_enc_width(c: u32) -> usize {
    todo!()
}

// [spec:libedit:def:chartype.ct-encode-char-fn]
// [spec:libedit:sem:chartype.ct-encode-char-fn]
/// `dst` carries the C's `len`: `ct_encode_string` passes the five bytes the
/// C passes, not the whole remaining buffer.
///
/// Returns the C's `ssize_t`: bytes written, 0 if `c` has no representation
/// in this locale, -1 if `dst` is too short.
pub(crate) fn ct_encode_char(dst: &mut [u8], c: u32) -> isize {
    todo!()
}

// [spec:libedit:def:chartype.ct-visual-string-fn]
// [spec:libedit:sem:chartype.ct-visual-string-fn]
/// The result borrows `conv.wbuff`, as the C's returned pointer does.
pub(crate) fn ct_visual_string<'a>(
    s: Option<&[u32]>,
    conv: &'a mut CtBufferT,
) -> Option<&'a [u32]> {
    todo!()
}

// [spec:libedit:def:chartype.ct-visual-width-fn]
// [spec:libedit:sem:chartype.ct-visual-width-fn]
pub(crate) fn ct_visual_width(c: u32) -> i32 {
    todo!()
}

// [spec:libedit:def:chartype.ct-visual-char-fn]
// [spec:libedit:sem:chartype.ct-visual-char-fn]
/// `dst` carries the C's `len`, a `wchar_t` count. Returns the C's
/// `ssize_t`: characters written, or -1 if `dst` is too short.
pub(crate) fn ct_visual_char(dst: &mut [u32], c: u32) -> isize {
    todo!()
}

// [spec:libedit:def:chartype.ct-chr-class-fn]
// [spec:libedit:sem:chartype.ct-chr-class-fn]
/// Returns one of the C's `CHTYPE_*` constants.
pub(crate) fn ct_chr_class(c: u32) -> i32 {
    todo!()
}
