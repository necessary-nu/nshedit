//! Ported from `src/map.c`; rules live in `docs/spec/port/src/map.md`.

// The signatures land before the bodies, so every parameter is unused until
// its `todo!()` is replaced. Remove this with the last one.
#![allow(unused_variables)]

use std::borrow::Cow;

use crate::el::{EditLine, ElActionT};

/// C: `#define N_KEYS 256` — the size of every key map.
pub const N_KEYS: usize = 256;

// [spec:libedit:def:map.el-func-t-edit-line-wint-t]
/// C: `typedef el_action_t (*el_func_t)(EditLine *, wint_t);`
///
/// An editor command. The `wint_t` is the character that invoked it, so it
/// is `u32`: `(wint_t)-1` reaches these functions.
pub type ElFuncT = fn(&mut EditLine, u32) -> ElActionT;

// [spec:libedit:def:map.el-bindings-t]
/// One row of the help table, for the `bind` shell command.
///
/// `name` and `description` are `Cow` because the C's are: `map_init`
/// `memcpy`s the generated static `el_func_help[]`, whose strings are wide
/// literals, and `map_addfunc` appends rows whose strings are `wcsdup`ed
/// from the caller. `map_end` frees only the appended ones — the borrowed
/// versus owned distinction the C makes by index, made structural.
pub struct ElBindingsT {
    /// Function numeric value.
    pub func: i32,
    /// C: `const wchar_t *name` — function name for the bind command.
    pub name: Cow<'static, [u32]>,
    /// C: `const wchar_t *description` — description of the function.
    pub description: Cow<'static, [u32]>,
}

/// Which of `el_map_t`'s two live maps `current` designates.
///
/// C: `el_action_t *current` aliases either `key` or `alt`. Rust cannot hold
/// a second mutable alias, so the alias becomes a selector. Note that
/// `chared.c` twice tests `el->el_map.current != el->el_map.emacs`, which is
/// unconditionally true in the C because `current` is only ever `key` or
/// `alt`; leaving `Emacs` out of this enum preserves that outcome rather
/// than inviting a translation that could make the test meaningful.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ElMapCurrent {
    /// `current == el_map.key`.
    Key,
    /// `current == el_map.alt`.
    Alt,
}

// [spec:libedit:def:map.el-map-t]
/// The key maps and the editor function tables.
pub struct ElMapT {
    /// C: `el_action_t *alt` — the current alternate key map, owned,
    /// `N_KEYS` entries.
    pub alt: Vec<ElActionT>,
    /// C: `el_action_t *key` — the current normal key map, owned, `N_KEYS`
    /// entries.
    pub key: Vec<ElActionT>,
    /// The keymap we are using — an alias of `key` or `alt`, so a selector.
    pub current: ElMapCurrent,
    /// C: `const el_action_t *emacs` — the default emacs key map, a
    /// compiled-in table. `map_end` sets it to NULL, hence the `Option`.
    pub emacs: Option<&'static [ElActionT; N_KEYS]>,
    /// The vi command mode key map, likewise compiled in.
    pub vic: Option<&'static [ElActionT; N_KEYS]>,
    /// The vi insert mode key map, likewise compiled in.
    pub vii: Option<&'static [ElActionT; N_KEYS]>,
    /// C: `int type` — `MAP_EMACS` (0) or `MAP_VI` (1). Left an integer;
    /// the C treats it as one.
    pub r#type: i32,
    /// The help for the editor functions, owned, `nfunc` entries.
    pub help: Vec<ElBindingsT>,
    /// List of available functions, owned, `nfunc` entries.
    pub func: Vec<ElFuncT>,
    /// The number of functions/help items. Retained even though `help` and
    /// `func` know their own lengths, because the `sem` rules index by it
    /// and because the C lets the two arrays and this counter disagree on
    /// `map_addfunc`'s failure paths.
    pub nfunc: usize,
    /// C: `wchar_t *wordchars` — the word character separators, owned.
    /// NULL until `map_init` runs and after `map_end`;
    /// `sem:map.el-get-fn` notes that
    /// `el_get(EL_WORDCHARS, &p)` hands out a pointer into this, so the
    /// port must copy rather than alias.
    pub wordchars: Option<Vec<u32>>,
}

// [spec:libedit:def:map.map-init-fn]
// [spec:libedit:sem:map.map-init-fn]
/// Initialize and allocate the maps. 0 on success, -1 if any allocation
/// failed, after tearing the rest back down.
pub(crate) fn map_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:map.map-end-fn]
// [spec:libedit:sem:map.map-end-fn]
/// Free the space taken by the editor maps.
pub(crate) fn map_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:map.map-init-nls-fn]
// [spec:libedit:sem:map.map-init-nls-fn]
/// Bind every printable high key to self-insert.
fn map_init_nls(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:map.map-init-meta-fn]
// [spec:libedit:sem:map.map-init-meta-fn]
/// Bind the meta keys to the matching `ESC`-prefixed sequences.
fn map_init_meta(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:map.map-init-vi-fn]
// [spec:libedit:sem:map.map-init-vi-fn]
/// Install the vi bindings and make them current.
pub(crate) fn map_init_vi(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:map.map-init-emacs-fn]
// [spec:libedit:sem:map.map-init-emacs-fn]
/// Install the emacs bindings and make them current.
pub(crate) fn map_init_emacs(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:map.map-set-editor-fn]
// [spec:libedit:sem:map.map-set-editor-fn]
/// Switch to the named editor: 0 for `emacs` or `vi`, -1 for anything else.
pub(crate) fn map_set_editor(el: &mut EditLine, editor: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:map.map-get-editor-fn]
// [spec:libedit:sem:map.map-get-editor-fn]
/// Report the current editor. The C's two answers are static wide literals,
/// so the out-parameter is a `&'static` one; its NULL check has no Rust
/// counterpart, a reference being non-null.
pub(crate) fn map_get_editor(el: &mut EditLine, editor: &mut &'static [u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:map.map-set-wordchars-fn]
// [spec:libedit:sem:map.map-set-wordchars-fn]
/// Replace the word-separator set with a copy of `wordchars`.
pub(crate) fn map_set_wordchars(el: &mut EditLine, wordchars: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:map.map-get-wordchars-fn]
// [spec:libedit:sem:map.map-get-wordchars-fn]
/// Hand out the word-separator set. The C lends its own buffer out; the
/// out-parameter mirrors the field's own type so the port can copy, and so
/// that the C's legitimately-NULL set stays distinguishable from an empty
/// one.
pub(crate) fn map_get_wordchars(el: &mut EditLine, wordchars: &mut Option<Vec<u32>>) -> i32 {
    todo!()
}

// [spec:libedit:def:map.map-print-key-fn]
// [spec:libedit:sem:map.map-print-key-fn]
/// Print the function description for one key. `map` is the C's
/// `el_action_t *map`, always `el_map.key` or `el_map.alt`, so it is the
/// selector rather than a second alias of `el`.
fn map_print_key(el: &mut EditLine, map: ElMapCurrent, r#in: &[u32]) {
    todo!()
}

// [spec:libedit:def:map.map-print-some-keys-fn]
// [spec:libedit:sem:map.map-print-some-keys-fn]
/// Print the binding shared by the keys `first` through `last`.
fn map_print_some_keys(el: &mut EditLine, map: ElMapCurrent, first: u32, last: u32) {
    todo!()
}

// [spec:libedit:def:map.map-print-all-keys-fn]
// [spec:libedit:sem:map.map-print-all-keys-fn]
/// Print the function description for all keys, both maps, the trie and the
/// arrow keys.
fn map_print_all_keys(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:map.map-bind-fn]
// [spec:libedit:sem:map.map-bind-fn]
/// The `bind` builtin: add, remove, change or show bindings. `argc` is the
/// C's — reassigned on entry and ignored — and the C's NULL terminator on
/// `argv` is the slice length here.
pub(crate) fn map_bind(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    todo!()
}

// [spec:libedit:def:map.map-addfunc-fn]
// [spec:libedit:sem:map.map-addfunc-fn]
/// Append a user-defined editor function and its help entry.
pub(crate) fn map_addfunc(el: &mut EditLine, name: &[u32], help: &[u32], func: ElFuncT) -> i32 {
    todo!()
}
