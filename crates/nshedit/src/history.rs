//! Ported from `src/history.c`; rules live in
//! `docs/spec/port/src/history.md`.
//!
//! The C compiles this file twice — once wide (`Char = wchar_t`,
//! `TYPE(x) = xW`) and once narrow via `historyn.c`. Only the wide
//! instantiation carries rules in the port manifest, so only it is named
//! here; the narrow handle is [`crate::histedit::History`].
//!
//! The builtin history is a circular doubly-linked list with a sentinel
//! embedded in the owner (`history_t.list`), and every entry links back to
//! that sentinel. The links stay raw pointers: they are neither rebased by a
//! `realloc` (each node is its own allocation) nor expressible as offsets
//! into any one buffer, and a safe re-shaping — an arena, or `Rc`/`Weak` —
//! is a structural change this wave is not making. Whichever the history
//! translation picks, `sem:history.history-def-insert-fn`'s
//! four-pointer link order is the behaviour to preserve.

use core::ffi::c_void;

use crate::histedit::HistEventW;

// [spec:libedit:def:history.history-gfun-t-void-type-hist-event]
/// C: `typedef int (*history_gfun_t)(void *, TYPE(HistEvent) *);`
///
/// A "get an element" operation: first/next/last/prev/curr. The `void *` is
/// the implementation's own cookie (`h_ref`), which is the `history_t` for
/// the builtin implementation and anything at all for one installed through
/// `H_FUNC`, so it stays opaque.
pub type HistoryGfunT = fn(*mut c_void, &mut HistEventW) -> i32;

// [spec:libedit:def:history.history-efun-t-void-type-hist-event-const-char]
/// C: `typedef int (*history_efun_t)(void *, TYPE(HistEvent) *, const Char *);`
///
/// An "enter this text" operation: enter/add. The string is borrowed for the
/// duration of the call and NUL-terminated, as in the C.
pub type HistoryEfunT = fn(*mut c_void, &mut HistEventW, *const u32) -> i32;

// [spec:libedit:def:history.history-vfun-t-void-type-hist-event]
/// C: `typedef void (*history_vfun_t)(void *, TYPE(HistEvent) *);`
///
/// The "clear the list" operation, which reports nothing.
pub type HistoryVfunT = fn(*mut c_void, &mut HistEventW);

// [spec:libedit:def:history.history-sfun-t-void-type-hist-event-const-int]
/// C: `typedef int (*history_sfun_t)(void *, TYPE(HistEvent) *, const int);`
///
/// An operation taking an event number: set/del.
pub type HistorySfunT = fn(*mut c_void, &mut HistEventW, i32) -> i32;

/// C: `struct TYPE(history)` — the wide history object, named `HistoryW` by
/// `def:histedit.history-w`.
///
/// The C defines this body in `history.c` with no rule of its own, which is
/// why there is no annotation here. It is a vtable plus a cookie: `h_ref` is
/// the implementation's state (the [`HistoryT`] below for the builtin one)
/// and the ten function pointers are what `history_w` dispatches through.
pub struct HistoryW {
    /// Argument for the history functions.
    pub h_ref: *mut c_void,
    /// Last entry point for history — the `H_APPEND` anchor, initialised
    /// to -1.
    pub h_ent: i32,
    /// Get the first element.
    pub h_first: Option<HistoryGfunT>,
    /// Get the next element.
    pub h_next: Option<HistoryGfunT>,
    /// Get the last element.
    pub h_last: Option<HistoryGfunT>,
    /// Get the previous element.
    pub h_prev: Option<HistoryGfunT>,
    /// Get the current element.
    pub h_curr: Option<HistoryGfunT>,
    /// Set the current element.
    pub h_set: Option<HistorySfunT>,
    /// Delete the given element.
    pub h_del: Option<HistorySfunT>,
    /// Clear the history list.
    pub h_clear: Option<HistoryVfunT>,
    /// Add an element.
    pub h_enter: Option<HistoryEfunT>,
    /// Append to an element.
    pub h_add: Option<HistoryEfunT>,
}

// [spec:libedit:def:history.hist-event-private]
/// A layout-compatible twin of [`HistEventW`] whose `str` member is not
/// `const`.
///
/// It exists only so `history_def_add` can get a mutable handle on an
/// entry's string; see `sem:history.history-def-add-fn`. The
/// C reaches it by casting, so the layout must stay identical.
#[repr(C)]
pub struct HistEventPrivate {
    pub num: i32,
    pub str: *mut u32,
}

// [spec:libedit:def:history.hentry-t]
/// One entry in the builtin history list.
pub struct HentryT {
    /// What we return. `ev.str` is the entry's own `Strdup`ed copy, which
    /// the entry owns and frees; every `HistEventW` handed to a caller
    /// borrows it.
    pub ev: HistEventW,
    /// C: `void *data` — per-entry client data, stored and handed back
    /// untouched.
    pub data: *mut c_void,
    /// Next entry. Circular: the last entry points at the owner's sentinel.
    pub next: *mut HentryT,
    /// Previous entry. Circular in the same way.
    pub prev: *mut HentryT,
}

// [spec:libedit:def:history.history-t]
/// The builtin history implementation's state, reached through
/// [`HistoryW::h_ref`].
pub struct HistoryT {
    /// Fake list header element. Both links point at it when the list is
    /// empty, and `cursor == &list` is the "no current event" state.
    pub list: HentryT,
    /// Current element in the list, or `&list`.
    pub cursor: *mut HentryT,
    /// Maximum number of events. Starts at 0, so nothing is retained until
    /// the caller issues `H_SETSIZE`.
    pub max: i32,
    /// Current number of events.
    pub cur: i32,
    /// For generation of unique event ids. Ids start at 1 and are never
    /// reused, which is what makes 0 usable as an "invalid id" sentinel.
    pub eventid: i32,
    /// C: `int flags` — `H_UNIQUE` (1) is the only bit. Left an integer
    /// because the C treats it as a flag word.
    pub flags: i32,
}
