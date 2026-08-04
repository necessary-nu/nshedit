//! Ported from `src/unvis.c`; rules live in `docs/spec/port/src/unvis.md`.

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
