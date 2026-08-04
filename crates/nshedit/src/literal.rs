//! Ported from `src/literal.c`; rules live in
//! `docs/spec/port/src/literal.md`.

// The function bodies are still `todo!()`, so every parameter reads as
// unused. Remove this once the translations land.
#![allow(unused_variables)]

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

// [spec:libedit:def:literal.literal-init-fn]
// [spec:libedit:sem:literal.literal-init-fn]
pub(crate) fn literal_init(el: &mut crate::el::EditLine) {
    todo!()
}

// [spec:libedit:def:literal.literal-end-fn]
// [spec:libedit:sem:literal.literal-end-fn]
pub(crate) fn literal_end(el: &mut crate::el::EditLine) {
    todo!()
}

// [spec:libedit:def:literal.literal-clear-fn]
// [spec:libedit:sem:literal.literal-clear-fn]
pub(crate) fn literal_clear(el: &mut crate::el::EditLine) {
    todo!()
}

// [spec:libedit:def:literal.literal-add-fn]
// [spec:libedit:sem:literal.literal-add-fn]
/// `end` is the C's `end` pointer expressed as an index into `buf`, the two
/// being pointers into the same string: the literal sequence is `buf[..end]`
/// and the visible character the C reads as `end[1]` is `buf[end + 1]`.
///
/// Returns the C's `wint_t`: `EL_LITERAL | index` on success, 0 on failure.
pub(crate) fn literal_add(
    el: &mut crate::el::EditLine,
    buf: &[u32],
    end: usize,
    wp: &mut i32,
) -> u32 {
    todo!()
}

// [spec:libedit:def:literal.literal-get-fn]
// [spec:libedit:sem:literal.literal-get-fn]
/// `idx` still carries the `EL_LITERAL` bit, which the C asserts on and then
/// masks off. The result borrows `el.el_literal.l_buf`, as the C's
/// `const char *` does.
pub(crate) fn literal_get(el: &mut crate::el::EditLine, idx: u32) -> &[u8] {
    todo!()
}
