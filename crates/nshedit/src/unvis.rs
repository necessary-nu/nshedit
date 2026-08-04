//! Ported from `src/unvis.c`; rules live in `docs/spec/port/src/unvis.md`.

// The function bodies are still `todo!()`, so every parameter reads as
// unused. Remove this once the translations land.
#![allow(unused_variables)]

use core::ffi::c_char;

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

// [spec:libedit:def:unvis.unvis-fn]
// [spec:libedit:sem:unvis.unvis-fn]
/// `cp` is the C's one-byte output slot and `astate` its in/out state word,
/// both of which the C takes as pointers to single objects. Returns one of
/// the `UNVIS_*` results.
pub fn unvis(cp: &mut c_char, c: i32, astate: &mut i32, flag: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:unvis.strnunvisx-fn]
// [spec:libedit:sem:unvis.strnunvisx-fn]
/// `dst` and `src` stay raw: `strunvisx` and `strunvis` pass `(size_t)~0` for
/// `dlen`, so the destination is a caller-supplied buffer the C has no real
/// length for. Returns bytes decoded, or -1 with `errno` set.
pub fn strnunvisx(dst: *mut c_char, dlen: usize, src: *const c_char, flag: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:unvis.strunvisx-fn]
// [spec:libedit:sem:unvis.strunvisx-fn]
pub fn strunvisx(dst: *mut c_char, src: *const c_char, flag: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:unvis.strunvis-fn]
// [spec:libedit:sem:unvis.strunvis-fn]
pub fn strunvis(dst: *mut c_char, src: *const c_char) -> i32 {
    todo!()
}

// [spec:libedit:def:unvis.strnunvis-fn]
// [spec:libedit:sem:unvis.strnunvis-fn]
pub fn strnunvis(dst: *mut c_char, dlen: usize, src: *const c_char) -> i32 {
    todo!()
}
