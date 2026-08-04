//! Ported from `src/keymacro.c`; rules live in
//! `docs/spec/port/src/keymacro.md`.

use crate::el::ElActionT;

// [spec:libedit:def:keymacro.keymacro-value-t]
/// What a key sequence resolves to.
///
/// C: a `union { el_action_t cmd; wchar_t *str; }` discriminated by the
/// neighbouring `type` field (`XK_CMD` 0, `XK_STR` 1, `XK_NOD` 2). A C union
/// with an owning pointer in it has no safe Rust spelling, and the tag is
/// already there, so it becomes an enum. The `type` field stays where the C
/// put it — nothing is collapsed — and `XK_NOD` is the C's "the union holds
/// a NULL `str`" state, which is `Str` with an empty buffer.
pub enum KeymacroValueT {
    /// C: `el_action_t cmd` — read when `type == XK_CMD`.
    Cmd(ElActionT),
    /// C: `wchar_t *str` — read when `type == XK_STR`; owned, and freed by
    /// `keymacro_end`.
    Str(Vec<u32>),
}

// [spec:libedit:def:keymacro.keymacro-node-t]
/// A node of `el->el_keymacro.map`, the trie of bound key sequences.
///
/// `next` and `sibling` are single-owner links — a node is reachable from
/// exactly one parent — so `Box` is both safe and literal here, unlike the
/// history list.
pub struct KeymacroNodeT {
    /// C: `wchar_t ch` — single character of the key.
    pub ch: u32,
    /// Node type: `XK_CMD`, `XK_STR` or `XK_NOD`.
    pub r#type: i32,
    /// Command code or macro string, if this is a leaf.
    pub val: KeymacroValueT,
    /// Next char of this key.
    pub next: Option<Box<KeymacroNodeT>>,
    /// Another key with the same prefix.
    pub sibling: Option<Box<KeymacroNodeT>>,
}

// [spec:libedit:def:keymacro.el-keymacro-t]
/// The key-macro trie and its scratch space.
pub struct ElKeymacroT {
    /// C: `wchar_t *buf` — key print buffer, owned.
    pub buf: Vec<u32>,
    /// C: `keymacro_node_t *map` — the key map, owned.
    pub map: Option<Box<KeymacroNodeT>>,
    /// Local conversion buffer: where `keymacro_get` leaves the value it
    /// found. Its discriminant is `keymacro_get`'s return value, not a
    /// stored `type` field.
    pub val: KeymacroValueT,
}
