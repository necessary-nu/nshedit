//! Ported from `src/literal.c`; rules live in
//! `docs/spec/port/src/literal.md`.

// [spec:libedit:def:literal.el-literal-t]
/// The literal table: the invisible byte sequences the prompt asked to be
/// emitted without occupying screen columns.
///
/// A successful `literal_add` returns `EL_LITERAL | index`, where
/// `EL_LITERAL` is `(wint_t)0x80000000` — bit 31 alone. That is why the
/// screen image is `u32` throughout and never `char`: the sentinel is not a
/// Unicode scalar value, so `char` cannot hold it at all. See
/// `sem:literal.literal-add-fn`.
pub struct ElLiteralT {
    /// C: `char **l_buf` — array of owned byte strings, each the multibyte
    /// encoding of one literal sequence plus its trailing visible character.
    /// The NUL the C appended is not stored; the length is the length.
    pub l_buf: Vec<Vec<u8>>,
    /// C: `size_t l_idx` — max in use. Kept alongside `l_buf` because the
    /// `sem` rules index by it and because `literal_clear`'s guard tests
    /// `l_len`, not this.
    pub l_idx: usize,
    /// C: `size_t l_len` — max allocated. Grows by a fixed +4 slots per
    /// reallocation, not by doubling.
    pub l_len: usize,
}
