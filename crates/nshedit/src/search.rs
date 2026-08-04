//! Ported from `src/search.c`; rules live in `docs/spec/port/src/search.md`.

// [spec:libedit:def:search.el-search-t]
/// Incremental- and character-search state.
pub struct ElSearchT {
    /// C: `wchar_t *patbuf` — the pattern buffer, owned.
    pub patbuf: Vec<u32>,
    /// C: `size_t patlen` — length of the pattern currently in `patbuf`,
    /// which is not the allocation size.
    pub patlen: usize,
    /// Direction of the last search.
    pub patdir: i32,
    /// Character search direction.
    pub chadir: i32,
    /// C: `wchar_t chacha` — the character we are looking for.
    pub chacha: u32,
    /// C: `char chatflg` — 0 if `f`, 1 if `t`. A byte-sized flag in the C,
    /// kept as one.
    pub chatflg: u8,
}
