//! Ported from `src/map.c`; rules live in `docs/spec/port/src/map.md`.

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
