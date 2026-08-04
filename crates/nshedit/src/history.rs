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

// Every function below is a signature with a `todo!()` body, so no parameter
// is read yet. Remove this once the translations land.
#![allow(unused_variables)]

use core::ffi::c_void;

use crate::el::CFile;
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

// [spec:libedit:def:history.history-def-first-fn]
// [spec:libedit:sem:history.history-def-first-fn]
/// C: `static int history_def_first(void *p, TYPE(HistEvent) *ev)`
fn history_def_first(p: *mut c_void, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-last-fn]
// [spec:libedit:sem:history.history-def-last-fn]
/// C: `static int history_def_last(void *p, TYPE(HistEvent) *ev)`
fn history_def_last(p: *mut c_void, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-next-fn]
// [spec:libedit:sem:history.history-def-next-fn]
/// C: `static int history_def_next(void *p, TYPE(HistEvent) *ev)`
fn history_def_next(p: *mut c_void, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-prev-fn]
// [spec:libedit:sem:history.history-def-prev-fn]
/// C: `static int history_def_prev(void *p, TYPE(HistEvent) *ev)`
fn history_def_prev(p: *mut c_void, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-curr-fn]
// [spec:libedit:sem:history.history-def-curr-fn]
/// C: `static int history_def_curr(void *p, TYPE(HistEvent) *ev)`
fn history_def_curr(p: *mut c_void, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-set-fn]
// [spec:libedit:sem:history.history-def-set-fn]
/// C: `static int history_def_set(void *p, TYPE(HistEvent) *ev, const int n)`
fn history_def_set(p: *mut c_void, ev: &mut HistEventW, n: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-set-nth-fn]
// [spec:libedit:sem:history.history-set-nth-fn]
/// C: `static int history_set_nth(void *p, TYPE(HistEvent) *ev, int n)`
fn history_set_nth(p: *mut c_void, ev: &mut HistEventW, n: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-add-fn]
// [spec:libedit:sem:history.history-def-add-fn]
/// C: `static int history_def_add(void *p, TYPE(HistEvent) *ev, const Char *str)`
fn history_def_add(p: *mut c_void, ev: &mut HistEventW, str: *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-deldata-nth-fn]
// [spec:libedit:sem:history.history-deldata-nth-fn]
/// C: `static int history_deldata_nth(history_t *h, TYPE(HistEvent) *ev, int num,
/// void **data)`
///
/// `data` is raw and not `&mut *mut c_void`: `(void **)-1` is a documented
/// magic value meaning "position only, do not delete", so the pointer is not
/// always dereferenceable.
fn history_deldata_nth(
    h: &mut HistoryT,
    ev: &mut HistEventW,
    num: i32,
    data: *mut *mut c_void,
) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-del-fn]
// [spec:libedit:sem:history.history-def-del-fn]
/// C: `static int history_def_del(void *p, TYPE(HistEvent) *ev, const int num)`
fn history_def_del(p: *mut c_void, ev: &mut HistEventW, num: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-delete-fn]
// [spec:libedit:sem:history.history-def-delete-fn]
/// C: `static void history_def_delete(history_t *h, TYPE(HistEvent) *ev,
/// hentry_t *hp)`
fn history_def_delete(h: &mut HistoryT, ev: &mut HistEventW, hp: *mut HentryT) {
    todo!()
}

// [spec:libedit:def:history.history-def-insert-fn]
// [spec:libedit:sem:history.history-def-insert-fn]
/// C: `static int history_def_insert(history_t *h, TYPE(HistEvent) *ev,
/// const Char *str)`
///
/// The four-pointer link order this performs is the reason `HentryT::next` and
/// `HentryT::prev` are still raw.
fn history_def_insert(h: &mut HistoryT, ev: &mut HistEventW, str: *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-enter-fn]
// [spec:libedit:sem:history.history-def-enter-fn]
/// C: `static int history_def_enter(void *p, TYPE(HistEvent) *ev, const Char *str)`
fn history_def_enter(p: *mut c_void, ev: &mut HistEventW, str: *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-init-fn]
// [spec:libedit:sem:history.history-def-init-fn]
/// C: `static int history_def_init(void **p, TYPE(HistEvent) *ev, int n)`
///
/// `p` is an out parameter — the only place a `void **` here is written rather
/// than read — and is never installed as a callback, so it can be a reference.
fn history_def_init(p: &mut *mut c_void, ev: &mut HistEventW, n: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-def-clear-fn]
// [spec:libedit:sem:history.history-def-clear-fn]
/// C: `static void history_def_clear(void *p, TYPE(HistEvent) *ev)`
fn history_def_clear(p: *mut c_void, ev: &mut HistEventW) {
    todo!()
}

// [spec:libedit:def:history.fun-history-init-fn]
// [spec:libedit:sem:history.fun-history-init-fn]
/// C: `TYPE(History) *FUN(history,init)(void)` — `history_winit` in the wide
/// build, declared in `histedit.h`.
///
/// The handle is returned raw rather than as `Option<Box<HistoryW>>` because
/// its lifetime is the caller's: `history_wend` frees it, and `H_END` frees it
/// from inside [`history_w`], which no borrow can express. Null is the C's
/// allocation failure.
pub fn history_winit() -> *mut HistoryW {
    todo!()
}

// [spec:libedit:def:history.fun-history-end-fn]
// [spec:libedit:sem:history.fun-history-end-fn]
/// C: `void FUN(history,end)(TYPE(History) *h)` — `history_wend` in the wide
/// build, declared in `histedit.h`. Frees `h`; the caller must not touch it
/// again.
pub fn history_wend(h: *mut HistoryW) {
    todo!()
}

// [spec:libedit:def:history.history-setsize-fn]
// [spec:libedit:sem:history.history-setsize-fn]
/// C: `static int history_setsize(TYPE(History) *h, TYPE(HistEvent) *ev, int num)`
fn history_setsize(h: &mut HistoryW, ev: &mut HistEventW, num: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-getsize-fn]
// [spec:libedit:sem:history.history-getsize-fn]
/// C: `static int history_getsize(TYPE(History) *h, TYPE(HistEvent) *ev)`
fn history_getsize(h: &mut HistoryW, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-setunique-fn]
// [spec:libedit:sem:history.history-setunique-fn]
/// C: `static int history_setunique(TYPE(History) *h, TYPE(HistEvent) *ev, int uni)`
fn history_setunique(h: &mut HistoryW, ev: &mut HistEventW, uni: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-getunique-fn]
// [spec:libedit:sem:history.history-getunique-fn]
/// C: `static int history_getunique(TYPE(History) *h, TYPE(HistEvent) *ev)`
fn history_getunique(h: &mut HistoryW, ev: &mut HistEventW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-set-fun-fn]
// [spec:libedit:sem:history.history-set-fun-fn]
/// C: `static int history_set_fun(TYPE(History) *h, TYPE(History) *nh)`
///
/// `nh` is the caller's assembled vtable — the C's stack-local `hf` — and is
/// only read, so it is a shared borrow even though the C declares it
/// non-`const`.
fn history_set_fun(h: &mut HistoryW, nh: &HistoryW) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-load-fn]
// [spec:libedit:sem:history.history-load-fn]
/// C: `static int history_load(TYPE(History) *h, const char *fname)`
///
/// The path is narrow `char` in both builds. Nothing keeps it past the call,
/// so it is borrowed rather than raw.
fn history_load(h: &mut HistoryW, fname: &str) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-save-fp-fn]
// [spec:libedit:sem:history.history-save-fp-fn]
/// C: `static int history_save_fp(TYPE(History) *h, size_t nelem, FILE *fp)`
///
/// `fp` is the caller's stream, neither flushed nor closed here.
fn history_save_fp(h: &mut HistoryW, nelem: usize, fp: CFile) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-save-fn]
// [spec:libedit:sem:history.history-save-fn]
/// C: `static int history_save(TYPE(History) *h, const char *fname)`
fn history_save(h: &mut HistoryW, fname: &str) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-prev-event-fn]
// [spec:libedit:sem:history.history-prev-event-fn]
/// C: `static int history_prev_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)`
fn history_prev_event(h: &mut HistoryW, ev: &mut HistEventW, num: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-next-evdata-fn]
// [spec:libedit:sem:history.history-next-evdata-fn]
/// C: `static int history_next_evdata(TYPE(History) *h, TYPE(HistEvent) *ev,
/// int num, void **d)`
fn history_next_evdata(
    h: &mut HistoryW,
    ev: &mut HistEventW,
    num: i32,
    d: *mut *mut c_void,
) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-next-event-fn]
// [spec:libedit:sem:history.history-next-event-fn]
/// C: `static int history_next_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)`
fn history_next_event(h: &mut HistoryW, ev: &mut HistEventW, num: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-prev-string-fn]
// [spec:libedit:sem:history.history-prev-string-fn]
/// C: `static int history_prev_string(TYPE(History) *h, TYPE(HistEvent) *ev,
/// const Char *str)`
fn history_prev_string(h: &mut HistoryW, ev: &mut HistEventW, str: *const u32) -> i32 {
    todo!()
}

// [spec:libedit:def:history.history-next-string-fn]
// [spec:libedit:sem:history.history-next-string-fn]
/// C: `static int history_next_string(TYPE(History) *h, TYPE(HistEvent) *ev,
/// const Char *str)`
fn history_next_string(h: &mut HistoryW, ev: &mut HistEventW, str: *const u32) -> i32 {
    todo!()
}

/// The trailing argument of one [`history_w`] operation.
///
/// Not a ported C type: it is what the varargs tail of `history_w` becomes
/// once `va_list` is dropped. `plan/decisions/idiomatic-core.md` puts the
/// varargs face in the ABI crate, so the ABI shim is what reads the `va_list`
/// and hands the core one of these; every opcode's argument list from
/// `sem:history.funw-history-fn` has a variant here, including the two-argument
/// ones the C reads in a fixed order.
pub enum HistoryArg<'a> {
    /// No trailing argument: `H_GETSIZE`, `H_FIRST`, `H_LAST`, `H_PREV`,
    /// `H_NEXT`, `H_CURR`, `H_END`, `H_CLEAR`, `H_GETUNIQUE`.
    None,
    /// One `int`: `H_SETSIZE`, `H_SET`, `H_SETUNIQUE`, `H_DEL`,
    /// `H_NEXT_EVENT`, `H_PREV_EVENT`.
    Num(i32),
    /// One `const Char *`: `H_ADD`, `H_ENTER`, `H_APPEND`, `H_NEXT_STR`,
    /// `H_PREV_STR`.
    Str(*const u32),
    /// One `const char *` path, narrow in both builds: `H_LOAD`, `H_SAVE`.
    Path(&'a str),
    /// One `FILE *`: `H_SAVE_FP`.
    Fp(CFile),
    /// `size_t nelem` then `FILE *`: `H_NSAVE_FP`.
    NSaveFp(usize, CFile),
    /// `int num` then `void **d`: `H_NEXT_EVDATA`, `H_DELDATA`. The pointer
    /// stays raw because `H_DELDATA` accepts the magic `(void **)-1`.
    EvData(i32, *mut *mut c_void),
    /// `const Char *line` then `void *data`: `H_REPLACE`.
    Replace(*const u32, *mut c_void),
    /// `H_FUNC`'s eleven varargs, already collected the way the C collects
    /// them into its stack-local `TYPE(History) hf`: the cookie plus the ten
    /// callbacks. `h_ent` is not part of the argument list and is never read.
    Funcs(&'a HistoryW),
}

// [spec:libedit:def:history.funw-history-fn]
// [spec:libedit:sem:history.funw-history-fn]
/// C: `int FUNW(history)(TYPE(History) *h, TYPE(HistEvent) *ev, int fun, ...)`
/// — `history_w` in the wide build, declared in `histedit.h`.
///
/// The varargs tail becomes a single [`HistoryArg`]; `fun` stays the raw `int`
/// opcode, whose numbering is ABI. `h` is raw because the C does not check it
/// for NULL, because internal callers reach it through `el_history.ref` (a
/// `void *`), and because `H_END` frees it here.
pub fn history_w(h: *mut HistoryW, ev: &mut HistEventW, fun: i32, arg: HistoryArg<'_>) -> i32 {
    todo!()
}
