//! Ported from `src/chartype.c`; rules live in
//! `docs/spec/port/src/chartype.md`.

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
