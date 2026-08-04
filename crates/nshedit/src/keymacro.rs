//! Ported from `src/keymacro.c`; rules live in
//! `docs/spec/port/src/keymacro.md`.

// The signatures land before the bodies, so every parameter is unused until
// its `todo!()` is replaced. Remove this with the last one.
#![allow(unused_variables)]

use crate::el::{EditLine, ElActionT};
use crate::map::ElMapCurrent;

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

// [spec:libedit:def:keymacro.keymacro-init-fn]
// [spec:libedit:sem:keymacro.keymacro-init-fn]
/// Initialize the key maps. 0 on success, -1 if the print buffer could not
/// be allocated.
pub(crate) fn keymacro_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-end-fn]
// [spec:libedit:sem:keymacro.keymacro-end-fn]
/// Free the key maps.
pub(crate) fn keymacro_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-map-cmd-fn]
// [spec:libedit:sem:keymacro.keymacro-map-cmd-fn]
/// Package a command number as a [`KeymacroValueT`] for [`keymacro_add`] or
/// `terminal_set_arrow`.
///
/// The C returns `&el->el_keymacro.val`, a per-`EditLine` scratch slot whose
/// only purpose is to give that pointer an address; the idiom is always
/// build-and-consume in one expression. Rust cannot lend that slot out while
/// the consumer also takes `el`, so the value is returned by value.
pub(crate) fn keymacro_map_cmd(el: &mut EditLine, cmd: i32) -> KeymacroValueT {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-map-str-fn]
// [spec:libedit:sem:keymacro.keymacro-map-str-fn]
/// Package a macro expansion string as a [`KeymacroValueT`]. Returned by
/// value for the reason given on [`keymacro_map_cmd`]; note that
/// [`KeymacroValueT::Str`] owns its buffer, so this copies where the C
/// stored a pointer that dangled as soon as `map_bind` returned.
pub(crate) fn keymacro_map_str(el: &mut EditLine, str: &[u32]) -> KeymacroValueT {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-reset-fn]
// [spec:libedit:sem:keymacro.keymacro-reset-fn]
/// Drop the whole trie, leaving no bound sequences.
pub(crate) fn keymacro_reset(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-get-fn]
// [spec:libedit:sem:keymacro.keymacro-get-fn]
/// Read characters until a key sequence matches or mismatches. Returns the
/// `XK_` type of the match; the last character read is left in `ch`.
pub(crate) fn keymacro_get(el: &mut EditLine, ch: &mut u32, val: &mut KeymacroValueT) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-add-fn]
// [spec:libedit:sem:keymacro.keymacro-add-fn]
/// Bind `key` to `val`. `val` is borrowed and copied into the trie; it is
/// not always the scratch slot — `terminal.c` passes `&arrow[i].fun`.
pub(crate) fn keymacro_add(el: &mut EditLine, key: &[u32], val: &KeymacroValueT, ntype: i32) {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-clear-fn]
// [spec:libedit:sem:keymacro.keymacro-clear-fn]
/// Drop a sequence binding whose lead-in is being rebound. `map` is the C's
/// `el_action_t *map`, always `el_map.key` or `el_map.alt`, so it is the
/// selector rather than a second alias of `el`.
pub(crate) fn keymacro_clear(el: &mut EditLine, map: ElMapCurrent, r#in: &[u32]) {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-delete-fn]
// [spec:libedit:sem:keymacro.keymacro-delete-fn]
/// Delete `key` and every longer key starting with it.
pub(crate) fn keymacro_delete(el: &mut EditLine, key: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-print-fn]
// [spec:libedit:sem:keymacro.keymacro-print-fn]
/// Print the binding for `key`, or the whole trie for an empty key.
pub(crate) fn keymacro_print(el: &mut EditLine, key: &[u32]) {
    todo!()
}

// [spec:libedit:def:keymacro.node-trav-fn]
// [spec:libedit:sem:keymacro.node-trav-fn]
/// Walk the trie from `ptr` until a match or a mismatch, reading more
/// characters as needed. `ptr` is `Option` because the C dereferences
/// `el->el_keymacro.map` here without checking it.
fn node_trav(
    el: &mut EditLine,
    ptr: Option<&KeymacroNodeT>,
    ch: &mut u32,
    val: &mut KeymacroValueT,
) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.node-try-fn]
// [spec:libedit:sem:keymacro.node-try-fn]
/// Find the node matching `str` or allocate one, then store `val` there.
#[allow(non_snake_case)]
fn node__try(
    el: &mut EditLine,
    ptr: &mut KeymacroNodeT,
    str: &[u32],
    val: &KeymacroValueT,
    ntype: i32,
) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.node-delete-fn]
// [spec:libedit:sem:keymacro.node-delete-fn]
/// Delete the node matching `str`. `inptr` is the C's `keymacro_node_t **`:
/// the link slot itself, so the node can be unlinked and dropped.
#[allow(non_snake_case)]
fn node__delete(el: &mut EditLine, inptr: &mut Option<Box<KeymacroNodeT>>, str: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.node-put-fn]
// [spec:libedit:sem:keymacro.node-put-fn]
/// Free a whole subtree. Takes the node by value: the C's `el_free` chain is
/// a drop here.
#[allow(non_snake_case)]
fn node__put(el: &mut EditLine, ptr: Option<Box<KeymacroNodeT>>) {
    todo!()
}

// [spec:libedit:def:keymacro.node-get-fn]
// [spec:libedit:sem:keymacro.node-get-fn]
/// Allocate one unlinked `XK_NOD` node for `ch`. `Option` keeps the C's
/// allocation-failure return, which none of its callers check.
#[allow(non_snake_case)]
fn node__get(ch: u32) -> Option<Box<KeymacroNodeT>> {
    todo!()
}

// [spec:libedit:def:keymacro.node-free-fn]
// [spec:libedit:sem:keymacro.node-free-fn]
/// Free a node and its `next`/`sibling` chains, without touching the macro
/// strings — the leak `sem:keymacro.node-free-fn` records.
#[allow(non_snake_case)]
fn node__free(k: Option<Box<KeymacroNodeT>>) {
    todo!()
}

// [spec:libedit:def:keymacro.node-lookup-fn]
// [spec:libedit:sem:keymacro.node-lookup-fn]
/// Look for `str` from node `ptr`, printing the binding at the leaf. `str`
/// is `Option` because the C tests `!str`.
fn node_lookup(
    el: &mut EditLine,
    str: Option<&[u32]>,
    ptr: Option<&KeymacroNodeT>,
    cnt: usize,
) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.node-enum-fn]
// [spec:libedit:sem:keymacro.node-enum-fn]
/// Print every binding at or below `ptr`, accumulating the key into
/// `el_keymacro.buf` at offset `cnt`.
fn node_enum(el: &mut EditLine, ptr: Option<&KeymacroNodeT>, cnt: usize) -> i32 {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-kprint-fn]
// [spec:libedit:sem:keymacro.keymacro-kprint-fn]
/// Print `key` and the function or string `val` binds it to. `val` is
/// `Option` because the C prints "no input" for a NULL one.
pub(crate) fn keymacro_kprint(
    el: &mut EditLine,
    key: &[u32],
    val: Option<&KeymacroValueT>,
    ntype: i32,
) {
    todo!()
}

// [spec:libedit:def:keymacro.keymacro-decode-str-fn]
// [spec:libedit:sem:keymacro.keymacro-decode-str-fn]
/// Make a printable, `sep`-wrapped narrow version of `str` in `buf`,
/// returning the length it wanted — which may exceed `len`. `len` is kept
/// alongside the slice because the rule indexes by it.
#[allow(non_snake_case)]
pub(crate) fn keymacro__decode_str(str: &[u32], buf: &mut [u8], len: usize, sep: &[u8]) -> usize {
    todo!()
}
