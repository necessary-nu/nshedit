//! Ported from `src/history.c`; rules live in
//! `docs/spec/port/src/history.md`.
//!
//! The C compiles this file twice — once wide (`Char = wchar_t`,
//! `TYPE(x) = xW`) and once narrow via `historyn.c`, whose entire content is
//! `#define NARROWCHAR` followed by `#include "history.c"`. The port does the
//! same thing with the same source: every type and function here is generic
//! over [`HistChar`], and the two instantiations are `C = u32` (the wide
//! build's `wchar_t`) and `C = c_char` (the narrow build's `char`). The
//! handles they produce are [`HistoryW`] and [`History`], which are distinct
//! Rust types over separate stores exactly as the C's two incomplete struct
//! types are.
//!
//! [`HistChar`] is the port's `#ifdef NARROWCHAR` block: `Strlen`, `Strdup`,
//! `Strcmp`, `Strncmp`, `he_errlist`'s `STR()` literals, and the two
//! conversions `ct_decode_string`/`ct_encode_string` that the narrow build
//! `#define`s to the identity. Everything else in this file is shared source
//! and cannot drift between the two, which is the property `historyn.c` has
//! and a hand-copied narrow implementation would not.
//!
//! The builtin history is a circular doubly-linked list with a sentinel
//! embedded in the owner (`history_t.list`), and every entry links back to
//! that sentinel. The links stay raw pointers: they are neither rebased by a
//! `realloc` (each node is its own allocation) nor expressible as offsets
//! into any one buffer, and a safe re-shaping — an arena, or `Rc`/`Weak` —
//! is a structural change this wave is not making. Whichever the history
//! translation picks, `sem:history.history-def-insert-fn`'s
//! four-pointer link order is the behaviour to preserve.

// The three public entry points take the C's raw handle and dereference it:
// `history_gen` because `H_END` frees it and because internal callers reach it
// through `el_history.ref` (a `void *`), `history_init_gen`/`history_end_gen`
// because they are `malloc`/`free` in the C. The lint is about the public
// face, and this face is the C ABI's.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Seek, Write};
use std::mem::ManuallyDrop;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;

use crate::chartype::{CtBufferT, ct_decode_string, ct_encode_string};
use crate::histedit::{
    H_ADD, H_APPEND, H_CLEAR, H_CURR, H_DEL, H_DELDATA, H_END, H_ENTER, H_FIRST, H_FUNC, H_GETSIZE,
    H_GETUNIQUE, H_LAST, H_LOAD, H_NEXT, H_NEXT_EVDATA, H_NEXT_EVENT, H_NEXT_STR, H_NSAVE_FP,
    H_PREV, H_PREV_EVENT, H_PREV_STR, H_REPLACE, H_SAVE, H_SAVE_FP, H_SET, H_SETSIZE, H_SETUNIQUE,
    HistEvent, HistEventGen, HistEventW,
};

// ---------------------------------------------------------------------------
// The `#ifdef NARROWCHAR` block: everything `history.c` redefines between its
// two compilations, and nothing else.
// ---------------------------------------------------------------------------

/// The C's `Char` — `wchar_t` in `history.c`, `char` in `historyn.c` — and
/// the six macros whose expansion differs between them.
///
/// Not a ported C type: it is the `#ifdef NARROWCHAR` table itself, made a
/// Rust trait so the one source below can be instantiated twice. Only the two
/// implementations in this file exist and no third is meaningful, so the trait
/// is deliberately not `pub`-usable as an extension point.
///
/// The three members are exactly the three places the two builds *differ*
/// rather than merely substitute one character type for another:
///
/// - [`HistChar::NUL`] and the string helpers built on it are substitution:
///   `Strlen`/`Strdup`/`Strcmp`/`Strncmp` become the `wcs*` or the `str*`
///   family and behave identically over their own element type.
/// - [`HistChar::errlist`] is substitution too — `STR("OK")` is `L"OK"` or
///   `"OK"` — but a `const` table cannot be built generically, so each
///   instantiation carries its own copy of the same sixteen strings.
/// - [`HistChar::decode`] and [`HistChar::encode`] are the genuine
///   difference. `historyn.c` defines `ct_decode_string(s, b)` and
///   `ct_encode_string(s, b)` as `(s)`, so the history file's bytes *are* the
///   entry text and no conversion happens at all; the wide build runs
///   `mbstowcs`/`wcstombs` through a scratch buffer, which can fail. See
///   `history_load` and `history_save_out` for what that costs.
pub trait HistChar: Copy + PartialEq + 'static {
    /// C: `'\0'` as a `Char` — the terminator every string here carries.
    const NUL: Self;

    /// C: `he_errlist[]`, built from `STR()` literals.
    ///
    /// A function rather than an associated `const` because the table is
    /// sixteen `&'static [Self]`, which each implementation defines as its own
    /// `static`; `ev->str` borrows an element's first character, so these must
    /// be process-lifetime storage the caller never frees.
    fn errlist() -> &'static [&'static [Self]; 16];

    /// C: `ct_decode_string(s, conv)` — `mbstowcs` into `conv` in the wide
    /// build, `(s)` in the narrow one.
    ///
    /// `s` and `conv` share a lifetime because the result borrows from `conv`
    /// in one build and from `s` in the other. `None` is the C's NULL, which
    /// `history_load` reads as "skip this line" — a state the narrow build
    /// cannot reach.
    fn decode<'a>(s: Option<&'a [u8]>, conv: &'a mut CtBufferT) -> Option<&'a [Self]>;

    /// C: `ct_encode_string(s, conv)` — `wcstombs` through `conv` in the wide
    /// build, `(s)` in the narrow one.
    fn encode<'a>(s: Option<&'a [Self]>, conv: &'a mut CtBufferT) -> Option<&'a [u8]>;
}

/// The sixteen `he_errlist` strings, written once and instantiated per
/// character type.
///
/// The text is ABI: `ev->str` points into one of these tables after any
/// failing operation, and `sem:history.funw-history-fn` pins every string.
macro_rules! errlists {
    ($($s:literal),+ $(,)?) => {
        /// C: `static const Char *const he_errlist[]` with `Char = wchar_t`.
        static HE_ERRLIST_W: [&[u32]; 16] = [$({
            const N: usize = $s.len() + 1;
            const W: [u32; N] = {
                let b = $s.as_bytes();
                let mut w = [0u32; N];
                let mut i = 0;
                while i < b.len() {
                    w[i] = b[i] as u32;
                    i += 1;
                }
                w
            };
            &W
        }),+];

        /// C: `static const Char *const he_errlist[]` with `Char = char`.
        static HE_ERRLIST_N: [&[c_char]; 16] = [$({
            const N: usize = $s.len() + 1;
            const B: [c_char; N] = {
                let b = $s.as_bytes();
                let mut w = [0 as c_char; N];
                let mut i = 0;
                while i < b.len() {
                    w[i] = b[i] as c_char;
                    i += 1;
                }
                w
            };
            &B
        }),+];
    };
}

errlists!(
    "OK",
    "unknown error",
    "malloc() failed",
    "first event not found",
    "last event not found",
    "empty list",
    "no next event",
    "no previous event",
    "current event is invalid",
    "event not found",
    "can't read history from file",
    "can't write history",
    "required parameter(s) not supplied",
    "history size negative",
    "function not allowed with other history-functions-set the default",
    "bad parameters",
);

/// `Char = wchar_t`: `history.c` compiled as itself.
impl HistChar for u32 {
    const NUL: Self = 0;

    fn errlist() -> &'static [&'static [Self]; 16] {
        &HE_ERRLIST_W
    }

    fn decode<'a>(s: Option<&'a [u8]>, conv: &'a mut CtBufferT) -> Option<&'a [Self]> {
        ct_decode_string(s, conv)
    }

    fn encode<'a>(s: Option<&'a [Self]>, conv: &'a mut CtBufferT) -> Option<&'a [u8]> {
        ct_encode_string(s, conv)
    }
}

/// `Char = char`: `historyn.c`.
impl HistChar for c_char {
    const NUL: Self = 0;

    fn errlist() -> &'static [&'static [Self]; 16] {
        &HE_ERRLIST_N
    }

    fn decode<'a>(s: Option<&'a [u8]>, conv: &'a mut CtBufferT) -> Option<&'a [Self]> {
        // C: `#define ct_decode_string(s, b) (s)`. The buffer is not even
        // declared in the narrow build (`#ifndef NARROWCHAR`), so it is
        // untouched here. `char` and `u8` differ only in signedness, which is
        // not a representation difference.
        let _ = conv;
        // SAFETY: `c_char` and `u8` have the same size and alignment and every
        // bit pattern is valid for both, so this is a relabelling of the same
        // bytes and not a conversion — which is what the C's macro is.
        s.map(|s| unsafe { core::slice::from_raw_parts(s.as_ptr().cast::<c_char>(), s.len()) })
    }

    fn encode<'a>(s: Option<&'a [Self]>, conv: &'a mut CtBufferT) -> Option<&'a [u8]> {
        // C: `#define ct_encode_string(s, b) (s)`, as above. A NULL `s` comes
        // straight back out as NULL, which is where ERR-history-08 bites in
        // this build too — see `history_save_out`.
        let _ = conv;
        // SAFETY: as in `decode`.
        s.map(|s| unsafe { core::slice::from_raw_parts(s.as_ptr().cast::<u8>(), s.len()) })
    }
}

/// C: `TYPE(History)` with `Char = wchar_t` — `historyW`, the wide handle.
pub type HistoryW = HistoryGen<u32>;

/// C: `TYPE(History)` with `Char = char` — `history`, the narrow handle
/// `historyn.c` produces and `crate::histedit::History` names.
pub type History = HistoryGen<c_char>;

// The four vtable typedefs. All of them are C ABI types and not Rust `fn`s:
// `history(h, ev, H_FUNC, ptr, first, next, ...)` is how an application
// installs its own ten, so the values that reach [`HistoryGen`]'s slots are
// `extern "C"` function pointers and every dispatch through one is `unsafe`.
// The `TYPE(HistEvent) *` out-parameter stays a raw pointer for the same
// reason — the callee is C code, which the borrow rules do not reach.

// [spec:libedit:def:history.history-gfun-t-void-type-hist-event]
/// C: `typedef int (*history_gfun_t)(void *, TYPE(HistEvent) *);`
///
/// A "get an element" operation: first/next/last/prev/curr. The `void *` is
/// the implementation's own cookie (`h_ref`), which is the `history_t` for
/// the builtin implementation and anything at all for one installed through
/// `H_FUNC`, so it stays opaque.
pub type HistoryGfunT<C> = unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>) -> c_int;

// [spec:libedit:def:history.history-efun-t-void-type-hist-event-const-char]
/// C: `typedef int (*history_efun_t)(void *, TYPE(HistEvent) *, const Char *);`
///
/// An "enter this text" operation: enter/add. The string is borrowed for the
/// duration of the call and NUL-terminated, as in the C.
pub type HistoryEfunT<C> =
    unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>, *const C) -> c_int;

// [spec:libedit:def:history.history-vfun-t-void-type-hist-event]
/// C: `typedef void (*history_vfun_t)(void *, TYPE(HistEvent) *);`
///
/// The "clear the list" operation, which reports nothing.
pub type HistoryVfunT<C> = unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>);

// [spec:libedit:def:history.history-sfun-t-void-type-hist-event-const-int]
/// C: `typedef int (*history_sfun_t)(void *, TYPE(HistEvent) *, const int);`
///
/// An operation taking an event number: set/del. The C's `const int` is a
/// top-level qualifier on a by-value parameter, which does not survive into
/// the ABI and has no Rust counterpart.
pub type HistorySfunT<C> = unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>, c_int) -> c_int;

/// C: `struct TYPE(history)` — the history object, named `HistoryW` by
/// `def:histedit.history-w` and `History` by `def:histedit.history`.
///
/// The C defines this body in `history.c` with no rule of its own, which is
/// why there is no annotation here. It is a vtable plus a cookie: `h_ref` is
/// the implementation's state (the [`HistoryTGen`] below for the builtin one)
/// and the ten function pointers are what `history_gen` dispatches through.
pub struct HistoryGen<C> {
    /// Argument for the history functions.
    pub h_ref: *mut c_void,
    /// Last entry point for history — the `H_APPEND` anchor, initialised
    /// to -1.
    pub h_ent: i32,
    /// Get the first element.
    pub h_first: Option<HistoryGfunT<C>>,
    /// Get the next element.
    pub h_next: Option<HistoryGfunT<C>>,
    /// Get the last element.
    pub h_last: Option<HistoryGfunT<C>>,
    /// Get the previous element.
    pub h_prev: Option<HistoryGfunT<C>>,
    /// Get the current element.
    pub h_curr: Option<HistoryGfunT<C>>,
    /// Set the current element.
    pub h_set: Option<HistorySfunT<C>>,
    /// Delete the given element.
    pub h_del: Option<HistorySfunT<C>>,
    /// Clear the history list.
    pub h_clear: Option<HistoryVfunT<C>>,
    /// Add an element.
    pub h_enter: Option<HistoryEfunT<C>>,
    /// Append to an element.
    pub h_add: Option<HistoryEfunT<C>>,
}

// [spec:libedit:def:history.hist-event-private]
/// A layout-compatible twin of [`HistEventGen`] whose `str` member is not
/// `const`.
///
/// It exists only so `history_def_add` can get a mutable handle on an
/// entry's string; see `sem:history.history-def-add-fn`. The
/// C reaches it by casting, so the layout must stay identical.
#[repr(C)]
pub struct HistEventPrivateGen<C> {
    pub num: i32,
    pub str: *mut C,
}

// [spec:libedit:def:history.hentry-t]
/// One entry in the builtin history list.
pub struct HentryGen<C> {
    /// What we return. `ev.str` is the entry's own `Strdup`ed copy, which
    /// the entry owns and frees; every `HistEventGen` handed to a caller
    /// borrows it.
    pub ev: HistEventGen<C>,
    /// C: `void *data` — per-entry client data, stored and handed back
    /// untouched.
    pub data: *mut c_void,
    /// Next entry. Circular: the last entry points at the owner's sentinel.
    pub next: *mut HentryGen<C>,
    /// Previous entry. Circular in the same way.
    pub prev: *mut HentryGen<C>,
}

// [spec:libedit:def:history.history-t]
/// The builtin history implementation's state, reached through
/// [`HistoryGen::h_ref`].
pub struct HistoryTGen<C> {
    /// Fake list header element. Both links point at it when the list is
    /// empty, and `cursor == &list` is the "no current event" state.
    pub list: HentryGen<C>,
    /// Current element in the list, or `&list`.
    pub cursor: *mut HentryGen<C>,
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

/// C: `#define H_UNIQUE 1` — the only bit in [`HistoryTGen::flags`], declared
/// inside `struct history_t` in the C.
const H_UNIQUE: i32 = 1;

/// C: `static const char hist_cookie[] = "_HiStOrY_V2_\n";`
///
/// The whole of the on-disk header: 13 bytes, no length field, no count, no
/// byte-order mark, no footer. Frozen by `[dec:libedit:no-c-ffi]`.
const HIST_COOKIE: &[u8] = b"_HiStOrY_V2_\n";

/// The C's error codes. `he_seterrev` writes the code into `ev->num` and the
/// matching string into `ev->str`, so both halves cross the ABI.
const _HE_OK: i32 = 0;
const _HE_UNKNOWN: i32 = 1;
const _HE_MALLOC_FAILED: i32 = 2;
const _HE_FIRST_NOTFOUND: i32 = 3;
const _HE_LAST_NOTFOUND: i32 = 4;
const _HE_EMPTY_LIST: i32 = 5;
const _HE_END_REACHED: i32 = 6;
const _HE_START_REACHED: i32 = 7;
const _HE_CURR_INVALID: i32 = 8;
const _HE_NOT_FOUND: i32 = 9;
const _HE_HIST_READ: i32 = 10;
const _HE_HIST_WRITE: i32 = 11;
const _HE_PARAM_MISSING: i32 = 12;
const _HE_NOT_ALLOWED: i32 = 14;
const _HE_BAD_PARAM: i32 = 15;

/// C: `he_seterrev(evp, code)` — `evp->num = code; evp->str =
/// he_errlist[code]`.
fn he_seterrev<C: HistChar>(ev: &mut HistEventGen<C>, code: i32) {
    ev.num = code;
    ev.str = C::errlist()[code as usize].as_ptr();
}

/// The C's uninitialised `TYPE(HistEvent) ev` scratch local.
///
/// `FUN(history,init)`, `FUN(history,end)` and `history_set_fun` each declare
/// one purely to have an address to pass; nothing reads it back. Zeroed here
/// because Rust has no uninitialised locals worth reaching for.
fn scratch_ev<C: HistChar>() -> HistEventGen<C> {
    HistEventGen {
        num: 0,
        str: ptr::null(),
    }
}

/// C: `*ev = <entry>->ev` — a by-value copy of the event, so `ev->str` is a
/// *borrowed* pointer to the entry's own string.
fn ev_copy<C: HistChar>(dst: &mut HistEventGen<C>, src: &HistEventGen<C>) {
    dst.num = src.num;
    dst.str = src.str;
}

// The allocation helpers. The C's `h_malloc`/`h_free` failure paths are
// observable — `_HE_MALLOC_FAILED` crosses the ABI — so every allocation here
// is fallible, which rules out plain `Box::new`.

/// C: `h_malloc(sizeof(*p))` followed by field assignment, as one fallible
/// step. `None` is the C's NULL.
fn try_alloc<T>(value: T) -> Option<*mut T> {
    let mut v: Vec<T> = Vec::new();
    v.try_reserve_exact(1).ok()?;
    v.push(value);
    Some(Box::into_raw(v.into_boxed_slice()).cast::<T>())
}

/// C: `h_free(p)` for a pointer [`try_alloc`] produced.
///
/// # Safety
///
/// `p` must be a live pointer from [`try_alloc`], not freed since.
unsafe fn free_alloc<T>(p: *mut T) {
    drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(p, 1)) });
}

/// C: `h_malloc(n * sizeof(Char))`, zero filled.
///
/// Every string this module owns is allocated here and is exactly
/// `Strlen + 1` elements long, which is what lets [`s_free`] recover the
/// length by scanning.
fn s_alloc<C: HistChar>(n: usize) -> Option<*mut C> {
    let mut v: Vec<C> = Vec::new();
    v.try_reserve_exact(n).ok()?;
    v.resize(n, C::NUL);
    Some(Box::into_raw(v.into_boxed_slice()).cast::<C>())
}

/// C: `h_free(str)` for an entry's own string.
///
/// # Safety
///
/// `s` must be NULL or a live pointer from [`s_alloc`]/[`s_dup`] whose
/// allocation is `s_len(s) + 1` elements.
unsafe fn s_free<C: HistChar>(s: *mut C) {
    if s.is_null() {
        return;
    }
    let n = unsafe { s_len(s) } + 1;
    drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(s, n)) });
}

/// C: `Strdup(s)` — `wcsdup` wide, `strdup` narrow. `None` is its NULL.
///
/// A NULL `s` duplicates the empty string; see the NULL-string note on
/// [`s_len`].
fn s_dup<C: HistChar>(s: *const C) -> Option<*mut C> {
    // SAFETY: `s` is NULL or a NUL-terminated string, which is this module's
    // invariant for every `Char *` it stores or is handed.
    let len = unsafe { s_len(s) };
    let p = s_alloc(len + 1)?;
    if len > 0 {
        // SAFETY: `len` elements were just measured in `s`, and `p` was just
        // allocated with `len + 1`.
        unsafe { ptr::copy_nonoverlapping(s, p, len) };
    }
    // SAFETY: as above; the terminator slot is the last one allocated.
    unsafe { *p.add(len) = C::NUL };
    Some(p)
}

/// C: `Strlen(s)` — `wcslen` wide, `strlen` narrow. Counts `Char`s, so the
/// narrow build counts **bytes** where the wide one counts characters.
///
/// **A NULL `s` is the empty string.** The C passes an unchecked caller
/// pointer to `Strlen`/`Strdup`/`Strcmp` in `history_def_add`,
/// `history_def_insert` and the two string searches, so a NULL there is
/// undefined behaviour; this module defines it once, here, as `STR("")`. That
/// keeps every one of those paths on a defined route (an empty append, an
/// empty entry, a prefix that matches everything) without inventing an error
/// code the C never returns.
///
/// # Safety
///
/// `s` must be NULL or point at a NUL-terminated string.
unsafe fn s_len<C: HistChar>(s: *const C) -> usize {
    if s.is_null() {
        return 0;
    }
    let mut n = 0;
    // SAFETY: the caller guarantees a terminator.
    while unsafe { *s.add(n) } != C::NUL {
        n += 1;
    }
    n
}

/// C: `Strcmp(a, b) == 0`. Only equality is ever tested.
///
/// # Safety
///
/// Both must be NULL or NUL-terminated strings.
unsafe fn s_cmp_eq<C: HistChar>(a: *const C, b: *const C) -> bool {
    let mut i = 0;
    loop {
        // SAFETY: both walks stop at the first difference or terminator.
        let (x, y) = unsafe { (s_at(a, i), s_at(b, i)) };
        if x != y {
            return false;
        }
        if x == C::NUL {
            return true;
        }
        i += 1;
    }
}

/// C: `Strncmp(a, b, n) == 0`, the prefix test of the two string searches.
///
/// # Safety
///
/// Both must be NULL or NUL-terminated strings.
unsafe fn s_ncmp_eq<C: HistChar>(a: *const C, b: *const C, n: usize) -> bool {
    for i in 0..n {
        // SAFETY: the loop stops at the first difference or terminator, both
        // of which are within the caller's strings.
        let (x, y) = unsafe { (s_at(a, i), s_at(b, i)) };
        if x != y {
            return false;
        }
        if x == C::NUL {
            return true;
        }
    }
    true
}

/// One character of a string, with a NULL string reading as `STR("")`.
///
/// # Safety
///
/// `s` must be NULL or a NUL-terminated string, and `i` at most its
/// terminator's index.
unsafe fn s_at<C: HistChar>(s: *const C, i: usize) -> C {
    if s.is_null() {
        C::NUL
    } else {
        // SAFETY: the caller's bound.
        unsafe { *s.add(i) }
    }
}

// The callback table. C: the `HNEXT`/`HFIRST`/… macros, each
// `(*(h)->h_x)((h)->h_ref, …)`. The C dereferences the slot unchecked; a NULL
// slot is impossible for a handle this module produced (`history_init_gen`
// installs all ten and `history_set_fun` rejects a set with any NULL), so the
// `None` arms are defined as the C's generic failure rather than a panic.

// Every call below is `unsafe` for the same reason, stated once here rather
// than four times: the slot holds either one of this file's ten
// `history_def_*` functions or ten an application installed through `H_FUNC`,
// and in both cases `def:history.history-gfun-t-void-type-hist-event` and its
// three siblings make the contract "a C function taking this store's cookie
// and a writable event". `r` is `h_ref`, which `history_init_gen` and
// `history_set_fun` are the only writers of and which is exactly the cookie
// the slot was installed beside (ERR-history-17 notwithstanding — that defect
// hands the *builtin* cookie to a caller's functions, which is a wrong value
// of the right type, not an invalid pointer). `ev` is the caller's live
// out-parameter, exclusively borrowed here and released for the call.

/// C: `HFIRST`/`HNEXT`/`HLAST`/`HPREV`/`HCURR`.
fn hg<C: HistChar>(f: Option<HistoryGfunT<C>>, r: *mut c_void, ev: &mut HistEventGen<C>) -> i32 {
    match f {
        // SAFETY: see the note above this function.
        Some(f) => unsafe { f(r, ptr::from_mut(ev)) },
        None => {
            he_seterrev(ev, _HE_UNKNOWN);
            -1
        }
    }
}

/// C: `HSET`/`HDEL`.
fn hs<C: HistChar>(
    f: Option<HistorySfunT<C>>,
    r: *mut c_void,
    ev: &mut HistEventGen<C>,
    n: i32,
) -> i32 {
    match f {
        // SAFETY: see the note above `hg`.
        Some(f) => unsafe { f(r, ptr::from_mut(ev), n) },
        None => {
            he_seterrev(ev, _HE_UNKNOWN);
            -1
        }
    }
}

/// C: `HENTER`/`HADD`.
fn he<C: HistChar>(
    f: Option<HistoryEfunT<C>>,
    r: *mut c_void,
    ev: &mut HistEventGen<C>,
    str: *const C,
) -> i32 {
    match f {
        // SAFETY: see the note above `hg`; `str` is the caller's
        // NUL-terminated string, borrowed for the call only.
        Some(f) => unsafe { f(r, ptr::from_mut(ev), str) },
        None => {
            he_seterrev(ev, _HE_UNKNOWN);
            -1
        }
    }
}

/// C: `HCLEAR`. Returns nothing, which is why `H_CLEAR` can never fail.
fn hv<C: HistChar>(f: Option<HistoryVfunT<C>>, r: *mut c_void, ev: &mut HistEventGen<C>) {
    if let Some(f) = f {
        // SAFETY: see the note above `hg`.
        unsafe { f(r, ptr::from_mut(ev)) };
    }
}

/// C: `h->h_next == history_def_next` — the identity test this file uses
/// everywhere to decide "is the builtin implementation still installed?".
fn is_def_next<C: HistChar>(f: Option<HistoryGfunT<C>>) -> bool {
    matches!(f, Some(f) if ptr::fn_addr_eq(f, history_def_next::<C> as HistoryGfunT<C>))
}

/// Does `h` still carry the builtin implementation?
///
/// Not a ported function: the C spells this `h->h_next == history_def_next`
/// at each of the six sites that needs it, which a caller outside this module
/// cannot write because `history_def_next` is `static`. The port needs it to
/// *be* callable from outside — `crate::hist`'s `hist_command` has to tell a
/// caller-supplied store from libedit's own before it dispatches
/// `history size` / `history unique` (ERR-history-05), and `el_history.ref`
/// is an opaque `void *` there. A NULL handle is not one of ours.
pub fn is_builtin<C: HistChar>(h: *mut HistoryGen<C>) -> bool {
    if h.is_null() {
        return false;
    }
    // SAFETY: a non-NULL handle is one `history_init_gen` returned and
    // `history_end_gen` has not yet freed — the same contract every other entry
    // point in this file has with its caller.
    is_def_next(unsafe { (*h).h_next })
}

// [spec:libedit:def:history.history-def-first-fn]
// [spec:libedit:sem:history.history-def-first-fn]
/// C: `static int history_def_first(void *p, TYPE(HistEvent) *ev)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_first<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: `p` is the `h_ref` this module allocated in `history_def_init`
    // and installed alongside these ten callbacks; the C's own precondition.
    unsafe {
        let list = &raw mut (*h).list;
        // Unconditional, before any emptiness test: an empty list parks the
        // cursor on the sentinel.
        (*h).cursor = (*list).next;
        if (*h).cursor == list {
            he_seterrev(ev, _HE_FIRST_NOTFOUND);
            return -1;
        }
        // "First" is the most recently entered event: insertion is at the
        // head. `ev->str` borrows the entry's own buffer.
        ev_copy(ev, &(*(*h).cursor).ev);
    }
    0
}

// [spec:libedit:def:history.history-def-last-fn]
// [spec:libedit:sem:history.history-def-last-fn]
/// C: `static int history_def_last(void *p, TYPE(HistEvent) *ev)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_last<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        (*h).cursor = (*list).prev;
        if (*h).cursor == list {
            he_seterrev(ev, _HE_LAST_NOTFOUND);
            return -1;
        }
        // "Last" is the *oldest* event.
        ev_copy(ev, &(*(*h).cursor).ev);
    }
    0
}

// [spec:libedit:def:history.history-def-next-fn]
// [spec:libedit:sem:history.history-def-next-fn]
/// C: `static int history_def_next(void *p, TYPE(HistEvent) *ev)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_next<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        if (*h).cursor == list {
            // ERR-history-35: "empty list" is also what a cursor merely
            // parked on the sentinel of a *non-empty* list reports.
            he_seterrev(ev, _HE_EMPTY_LIST);
            return -1;
        }
        if (*(*h).cursor).next == list {
            // Already on the oldest entry. The cursor is deliberately not
            // moved, so a following `H_PREV` walks back correctly.
            he_seterrev(ev, _HE_END_REACHED);
            return -1;
        }
        // "Next" is toward *older* entries.
        (*h).cursor = (*(*h).cursor).next;
        ev_copy(ev, &(*(*h).cursor).ev);
    }
    0
}

// [spec:libedit:def:history.history-def-prev-fn]
// [spec:libedit:sem:history.history-def-prev-fn]
/// C: `static int history_def_prev(void *p, TYPE(HistEvent) *ev)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_prev<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        if (*h).cursor == list {
            // ERR-history-35: `_HE_END_REACHED` is "no next event", reported
            // here by the *previous* function. Observable through `ev->str`.
            he_seterrev(
                ev,
                if (*h).cur > 0 {
                    _HE_END_REACHED
                } else {
                    _HE_EMPTY_LIST
                },
            );
            return -1;
        }
        if (*(*h).cursor).prev == list {
            he_seterrev(ev, _HE_START_REACHED);
            return -1;
        }
        // "Previous" is toward *newer* entries.
        (*h).cursor = (*(*h).cursor).prev;
        ev_copy(ev, &(*(*h).cursor).ev);
    }
    0
}

// [spec:libedit:def:history.history-def-curr-fn]
// [spec:libedit:sem:history.history-def-curr-fn]
/// C: `static int history_def_curr(void *p, TYPE(HistEvent) *ev)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_curr<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`. The cursor never moves here.
    unsafe {
        let list = &raw mut (*h).list;
        if (*h).cursor == list {
            he_seterrev(
                ev,
                if (*h).cur > 0 {
                    _HE_CURR_INVALID
                } else {
                    _HE_EMPTY_LIST
                },
            );
            return -1;
        }
        // The only way to read the event a successful `H_SET` positioned on,
        // because `history_def_set` never writes `*ev`.
        ev_copy(ev, &(*(*h).cursor).ev);
    }
    0
}

// [spec:libedit:def:history.history-def-set-fn]
// [spec:libedit:sem:history.history-def-set-fn]
/// C: `static int history_def_set(void *p, TYPE(HistEvent) *ev, const int n)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_set<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
    n: c_int,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        if (*h).cur == 0 {
            he_seterrev(ev, _HE_EMPTY_LIST);
            return -1;
        }
        // Fast path: the cursor is already on the wanted *id* (not index).
        if (*h).cursor == list || (*(*h).cursor).ev.num != n {
            // ERR-history-24: the scan assigns the cursor on every iteration,
            // so a failed search leaves it parked on the sentinel — a failed
            // `H_SET` invalidates the position rather than preserving it.
            (*h).cursor = (*list).next;
            while (*h).cursor != list {
                if (*(*h).cursor).ev.num == n {
                    break;
                }
                (*h).cursor = (*(*h).cursor).next;
            }
        }
        if (*h).cursor == list {
            he_seterrev(ev, _HE_NOT_FOUND);
            return -1;
        }
    }
    // `*ev` is deliberately not written on success.
    0
}

// [spec:libedit:def:history.history-set-nth-fn]
// [spec:libedit:sem:history.history-set-nth-fn]
/// C: `static int history_set_nth(void *p, TYPE(HistEvent) *ev, int n)`
fn history_set_nth<C: HistChar>(p: *mut c_void, ev: &mut HistEventGen<C>, n: i32) -> i32 {
    let h = p.cast::<HistoryTGen<C>>();
    let mut n = n;
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        if (*h).cur == 0 {
            he_seterrev(ev, _HE_EMPTY_LIST);
            return -1;
        }
        // Index addressing from the *oldest* end, walking toward newer.
        // C: `if (n-- <= 0) break;` — the test reads the pre-decrement value,
        // so any negative `n` behaves exactly like 0 and selects the oldest.
        (*h).cursor = (*list).prev;
        while (*h).cursor != list {
            if n <= 0 {
                break;
            }
            n -= 1;
            (*h).cursor = (*(*h).cursor).prev;
        }
        if (*h).cursor == list {
            he_seterrev(ev, _HE_NOT_FOUND);
            return -1;
        }
    }
    // `*ev` is not written on success.
    0
}

// [spec:libedit:def:history.history-def-add-fn]
// [spec:libedit:sem:history.history-def-add-fn]
/// C: `static int history_def_add(void *p, TYPE(HistEvent) *ev, const Char *str)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_add<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
    str: *const C,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`. `evp` in the C is the non-`const`
    // alias of the same event, which a raw pointer already is here.
    unsafe {
        let list = &raw mut (*h).list;
        if (*h).cursor == list {
            // No current event: delegate entirely, so `H_ADD` on a fresh or
            // invalidated history creates an event and returns 1, not 0. Its
            // preconditions are this function's own three parameters,
            // forwarded unchanged, so they are already met.
            return history_def_enter(p, ev, str);
        }
        let cur = (*h).cursor;
        let old = (*cur).ev.str;
        let elen = s_len(old);
        let slen = s_len(str);
        let len = elen + slen + 1;
        let Some(s) = s_alloc(len) else {
            // The existing entry is left completely unchanged.
            he_seterrev(ev, _HE_MALLOC_FAILED);
            return -1;
        };
        if elen > 0 {
            ptr::copy_nonoverlapping(old, s, elen);
        }
        if slen > 0 {
            ptr::copy_nonoverlapping(str, s.add(elen), slen);
        }
        // The old string's own terminator is deliberately not copied.
        *s.add(len - 1) = C::NUL;
        s_free(old.cast_mut());
        (*cur).ev.str = s;
        // Neither the entry nor the cursor moves, and eviction is not re-run,
        // so appending can never drop an entry. Any `HistEvent` the caller
        // captured before this call now holds a freed pointer.
        ev_copy(ev, &(*cur).ev);
    }
    0
}

// [spec:libedit:def:history.history-deldata-nth-fn]
// [spec:libedit:sem:history.history-deldata-nth-fn]
/// C: `static int history_deldata_nth(history_t *h, TYPE(HistEvent) *ev, int num,
/// void **data)`
///
/// `data` is raw and not `&mut *mut c_void`: `(void **)-1` is a documented
/// magic value meaning "position only, do not delete", so the pointer is not
/// always dereferenceable.
fn history_deldata_nth<C: HistChar>(
    h: &mut HistoryTGen<C>,
    ev: &mut HistEventGen<C>,
    num: i32,
    data: *mut *mut c_void,
) -> i32 {
    let h: *mut HistoryTGen<C> = ptr::from_mut(h);
    if history_set_nth(h.cast::<c_void>(), ev, num) != 0 {
        // `ev` carries `_HE_EMPTY_LIST` or `_HE_NOT_FOUND`, and the cursor
        // may have been left on the sentinel.
        return -1;
    }
    // The documented magic value: position only, do not delete. `*ev` is not
    // written either. `src/readline.c` relies on this for positional lookup.
    if data.addr() == usize::MAX {
        return 0;
    }
    // SAFETY: `history_set_nth` returned 0, so the cursor is on a real entry.
    unsafe {
        let cursor = (*h).cursor;
        // ERR-history-15: an unchecked `Strdup` whose ownership passes to the
        // caller and which the library never frees. A failed allocation
        // stores NULL and the deletion still proceeds.
        ev.str = s_dup((*cursor).ev.str).unwrap_or(ptr::null_mut());
        ev.num = (*cursor).ev.num;
        if !data.is_null() {
            // The `void *` attached by `H_REPLACE`, borrowed straight out;
            // ownership of whatever it points at passes to the caller.
            *data = (*cursor).data;
        }
        history_def_delete_raw(h, cursor, true);
    }
    0
}

// [spec:libedit:def:history.history-def-del-fn]
// [spec:libedit:sem:history.history-def-del-fn]
/// C: `static int history_def_del(void *p, TYPE(HistEvent) *ev, const int num)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_del<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
    num: c_int,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // Position by event id. The C's `ev` parameter is annotated
    // `__attribute__((__unused__))` and the body uses it anyway.
    // SAFETY: `p` and `ev` are this function's own parameters, forwarded
    // unchanged, so `history_def_set`'s contract is the one already met here.
    if unsafe { history_def_set(p, ev, num) } != 0 {
        return -1;
    }
    // SAFETY: `history_def_set` returned 0, so the cursor is on a real entry.
    unsafe {
        let cursor = (*h).cursor;
        // ERR-history-15, as in `history_deldata_nth`.
        ev.str = s_dup((*cursor).ev.str).unwrap_or(ptr::null_mut());
        ev.num = (*cursor).ev.num;
        // The entry's `data` pointer is discarded without being returned or
        // freed; `H_DELDATA` is the opcode that hands it back.
        history_def_delete_raw(h, cursor, true);
    }
    0
}

// [spec:libedit:def:history.history-def-delete-fn]
// [spec:libedit:sem:history.history-def-delete-fn]
/// C: `static void history_def_delete(history_t *h, TYPE(HistEvent) *ev,
/// hentry_t *hp)`
fn history_def_delete<C: HistChar>(
    h: &mut HistoryTGen<C>,
    ev: &mut HistEventGen<C>,
    hp: *mut HentryGen<C>,
) {
    // `ev` is accepted and never touched, exactly as in the C.
    let _ = ev;
    // SAFETY: `hp` is a node of `h`'s list, which is this function's C
    // precondition; every caller passes `h->cursor` or `h->list.prev`.
    unsafe { history_def_delete_raw(ptr::from_mut(h), hp, true) }
}

/// The body of [`history_def_delete`], with the one knob
/// `sem:history.history-def-enter-fn` forces the port to grow.
///
/// `free_str` is false for exactly one caller: the eviction loop in
/// [`history_def_enter`] when the entry being dropped is the one it just
/// inserted and therefore the one `*ev` already points at. See ERR-history-01
/// there. Everything else — the unlink, the node free, `cur--`, the cursor
/// repair — is identical either way, so the list state the C leaves behind is
/// reproduced exactly.
///
/// Works on a raw store rather than `&mut HistoryTGen<C>` because the list
/// nodes hold raw pointers *into* that same object (the sentinel is
/// `h->list`), and re-deriving a unique reference around them is precisely
/// what a raw intrusive list cannot promise.
///
/// # Safety
///
/// `h` must be a live builtin store and `hp` one of its list nodes.
unsafe fn history_def_delete_raw<C: HistChar>(
    h: *mut HistoryTGen<C>,
    hp: *mut HentryGen<C>,
    free_str: bool,
) {
    // SAFETY: the caller's contract.
    unsafe {
        let list = &raw mut (*h).list;
        if hp == list {
            // C: `abort()`. Deleting the sentinel is a programming error the
            // C terminates the process for; unreachable from every caller in
            // this file, and a panic is the Rust equivalent of that contract.
            panic!("history_def_delete: the list sentinel is not an entry");
        }
        // Cursor repair, before the unlink: deleting the newest entry moves
        // the cursor to the second-newest, and deleting the sole entry leaves
        // it on the sentinel.
        if (*h).cursor == hp {
            (*h).cursor = (*hp).prev;
            if (*h).cursor == list {
                (*h).cursor = (*hp).next;
            }
        }
        // Two writes; the list is circular through the sentinel, so the ends
        // need no special case.
        (*(*hp).prev).next = (*hp).next;
        (*(*hp).next).prev = (*hp).prev;
        if free_str {
            s_free((*hp).ev.str.cast_mut());
        }
        // `hp->data` is deliberately not freed: that pointer is the caller's
        // property and is simply dropped.
        free_alloc(hp);
        // `h->eventid` is not adjusted, so ids of deleted events are never
        // reused.
        (*h).cur -= 1;
    }
}

// [spec:libedit:def:history.history-def-insert-fn]
// [spec:libedit:sem:history.history-def-insert-fn]
/// C: `static int history_def_insert(history_t *h, TYPE(HistEvent) *ev,
/// const Char *str)`
///
/// The four-pointer link order this performs is the reason `HentryGen::next`
/// and `HentryGen::prev` are still raw.
fn history_def_insert<C: HistChar>(
    h: &mut HistoryTGen<C>,
    ev: &mut HistEventGen<C>,
    str: *const C,
) -> i32 {
    // SAFETY: `h` is the live builtin store; the nodes below are this
    // module's own allocations.
    unsafe { history_def_insert_raw(ptr::from_mut(h), ev, str) }
}

/// The body of [`history_def_insert`], on the raw store — see
/// [`history_def_delete_raw`] for why.
///
/// # Safety
///
/// `h` must be a live builtin store.
unsafe fn history_def_insert_raw<C: HistChar>(
    h: *mut HistoryTGen<C>,
    ev: &mut HistEventGen<C>,
    str: *const C,
) -> i32 {
    // The history takes ownership of this copy; the caller's `str` is not
    // retained. A NULL `str` duplicates `L""` — see [`s_len`].
    let Some(s) = s_dup(str) else {
        he_seterrev(ev, _HE_MALLOC_FAILED);
        return -1;
    };
    let node = HentryGen {
        ev: HistEventGen { num: 0, str: s },
        data: ptr::null_mut(),
        next: ptr::null_mut(),
        prev: ptr::null_mut(),
    };
    let Some(c) = try_alloc(node) else {
        // SAFETY: `s` was just allocated here and nothing else holds it.
        unsafe { s_free(s) };
        he_seterrev(ev, _HE_MALLOC_FAILED);
        return -1;
    };
    // SAFETY: the caller's contract, plus `c` from `try_alloc`.
    unsafe {
        let list = &raw mut (*h).list;
        // Ids start at 1, increase strictly monotonically and are never
        // reused; only `history_def_clear` resets the counter. No event ever
        // has id 0, which is what makes the `_HE_OK` prologue's `ev->num = 0`
        // usable as an "invalid id" sentinel.
        (*h).eventid += 1;
        (*c).ev.num = (*h).eventid;
        // Link at the head, four pointer writes in this order. Works
        // unmodified on an empty list, where `list.next` is the sentinel.
        (*c).next = (*list).next;
        (*c).prev = list;
        (*(*list).next).prev = c;
        (*list).next = c;
        (*h).cur += 1;
        // Insertion always repositions the cursor onto the new entry.
        (*h).cursor = c;
        // `ev->str` borrows the entry's own buffer.
        ev_copy(ev, &(*c).ev);
    }
    0
}

// [spec:libedit:def:history.history-def-enter-fn]
// [spec:libedit:sem:history.history-def-enter-fn]
/// C: `static int history_def_enter(void *p, TYPE(HistEvent) *ev, const Char *str)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_enter<C: HistChar>(
    p: *mut c_void,
    ev: *mut HistEventGen<C>,
    str: *const C,
) -> c_int {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        // Deduplication against the single most recent entry only, so `a b a`
        // stores all three. `*ev` is not written: the caller sees the
        // dispatcher's 0/"OK", and ERR-history-25 is that `H_ENTER` then
        // stores `h_ent = 0`, which matches no event.
        if (*h).flags & H_UNIQUE != 0
            && (*list).next != list
            && s_cmp_eq((*(*list).next).ev.str, str)
        {
            return 0;
        }

        if history_def_insert_raw(h, ev, str) == -1 {
            // Keep the `_HE_MALLOC_FAILED` message the insert set.
            return -1;
        }
        let inserted = (*h).cursor;

        // Eviction, always from the tail — the oldest first. The source
        // comment above this loop claims it "always keeps at least one
        // entry"; the condition does not (ERR-history-37).
        //
        // Spelled `loop`/`break` rather than `while`: the counter it tests is
        // decremented through a raw pointer inside the delete, which
        // `clippy::while_immutable_condition` cannot see.
        loop {
            if (*h).cur <= (*h).max || (*h).cur <= 0 {
                break;
            }
            let tail = (*list).prev;
            // ERR-history-01, **defined here**. With `max == 0` — the state
            // of every history until `H_SETSIZE` is issued — the entry just
            // inserted *is* the tail, and the C frees the very string
            // `history_def_insert` has already published through `*ev`. That
            // is a use-after-free across the public API, so it is not
            // reproducible: the port keeps the eviction (the list, `cur`,
            // `cursor` and `eventid` all end where the C leaves them, and
            // `ev->num` stays the stale id of an event that no longer
            // exists) but *leaks* that one string instead of freeing it, so
            // the caller's borrowed `ev->str` stays readable. The divergence
            // is one unreachable allocation per evicted-on-insert entry.
            history_def_delete_raw(h, tail, tail != inserted);
        }
    }
    // 1 on a real insert, not 0.
    1
}

// [spec:libedit:def:history.history-def-init-fn]
// [spec:libedit:sem:history.history-def-init-fn]
/// C: `static int history_def_init(void **p, TYPE(HistEvent) *ev, int n)`
///
/// `p` is an out parameter — the only place a `void **` here is written rather
/// than read — and is never installed as a callback, so it can be a reference.
fn history_def_init<C: HistChar>(p: &mut *mut c_void, ev: &mut HistEventGen<C>, n: i32) -> i32 {
    // `ev` is declared unused and genuinely is: notably, the
    // allocation-failure path sets no error event.
    let _ = ev;
    // Negative maxima are impossible from here on.
    let n = if n <= 0 { 0 } else { n };
    let store = HistoryTGen {
        // The `list` member is an embedded sentinel, never a real entry and
        // never separately allocated. `data` is uninitialised in the C and is
        // never read, because `history_def_delete` aborts rather than process
        // the sentinel.
        list: HentryGen {
            ev: HistEventGen {
                num: 0,
                str: ptr::null::<C>(),
            },
            data: ptr::null_mut(),
            next: ptr::null_mut(),
            prev: ptr::null_mut(),
        },
        cursor: ptr::null_mut(),
        max: n,
        cur: 0,
        // So the first entry gets id 1.
        eventid: 0,
        // `H_UNIQUE` off.
        flags: 0,
    };
    let Some(h) = try_alloc(store) else {
        // `*p` and `*ev` are both left untouched.
        return -1;
    };
    // SAFETY: `h` is the allocation just made; the self-links can only be
    // written once it has its final address.
    unsafe {
        let list = &raw mut (*h).list;
        // The empty circular list, and a cursor that starts invalid.
        (*list).next = list;
        (*list).prev = list;
        (*h).cursor = list;
    }
    *p = h.cast::<c_void>();
    0
}

// [spec:libedit:def:history.history-def-clear-fn]
// [spec:libedit:sem:history.history-def-clear-fn]
/// C: `static void history_def_clear(void *p, TYPE(HistEvent) *ev)`
///
/// # Safety
///
/// `p` must be the `history_t` cookie this file's `history_def_init` produced
/// and `ev` a writable event; that is the pairing `history_init_gen` installs and
/// the precondition every `H_*` dispatch through [`HistoryGen`] carries.
unsafe extern "C" fn history_def_clear<C: HistChar>(p: *mut c_void, ev: *mut HistEventGen<C>) {
    // SAFETY: `ev` is the caller's out-parameter, per this function's own
    // contract; nothing in the body aliases it.
    let ev = unsafe { &mut *ev };
    // Threaded through the deletes and never written by any of them.
    let _ = ev;
    let h = p.cast::<HistoryTGen<C>>();
    // SAFETY: as in `history_def_first`.
    unsafe {
        let list = &raw mut (*h).list;
        // Repeatedly remove the tail — the oldest entry — freeing its string
        // and its node. `loop`/`break` for the reason `history_def_enter`'s
        // eviction gives: the delete relinks through raw pointers.
        loop {
            let tail = (*list).prev;
            if tail == list {
                break;
            }
            history_def_delete_raw(h, tail, true);
        }
        (*h).cursor = list;
        // The id counter restarts, so events entered after a clear reuse
        // numbers earlier events had.
        (*h).eventid = 0;
        (*h).cur = 0;
        // `max` and `flags` are deliberately not reset, the `history_t`
        // itself is not freed, and per-entry `data` pointers are not freed.
    }
}

// [spec:libedit:def:history.fun-history-init-fn]
// [spec:libedit:sem:history.fun-history-init-fn]
/// C: `TYPE(History) *FUN(history,init)(void)` — `history_winit` in the wide
/// build and `history_init` in the narrow one, both declared in `histedit.h`.
/// The two spellings are [`history_winit`] and [`history_init`].
///
/// The handle is returned raw rather than as `Option<Box<HistoryGen<C>>>`
/// because its lifetime is the caller's: [`history_end_gen`] frees it, and
/// `H_END` frees it from inside [`history_gen`], which no borrow can express.
/// Null is the C's allocation failure.
pub fn history_init_gen<C: HistChar>() -> *mut HistoryGen<C> {
    let Some(h) = try_alloc(HistoryGen {
        h_ref: ptr::null_mut(),
        // Overwritten below; the C leaves it uninitialised until then.
        h_ent: 0,
        h_first: None,
        h_next: None,
        h_last: None,
        h_prev: None,
        h_curr: None,
        h_set: None,
        h_del: None,
        h_clear: None,
        h_enter: None,
        h_add: None,
    }) else {
        return ptr::null_mut();
    };
    // The C's uninitialised `ev`, passed only as a formal argument.
    let mut ev: HistEventGen<C> = scratch_ev();
    // SAFETY: `h` is the allocation just made and nothing else refers to it.
    unsafe {
        // `n = 0`: the initial maximum size is **0**, not unlimited, so until
        // the caller issues `H_SETSIZE` every `H_ENTER` inserts an entry and
        // immediately evicts it (ERR-history-01).
        if history_def_init(&mut (*h).h_ref, &mut ev, 0) == -1 {
            free_alloc(h);
            return ptr::null_mut();
        }
        // The "no event has been entered yet" marker `H_APPEND` keys off.
        (*h).h_ent = -1;
        // All ten slots. `h_next` doubles as the identity test the rest of
        // the file uses to recognise the builtin implementation.
        (*h).h_next = Some(history_def_next::<C>);
        (*h).h_first = Some(history_def_first::<C>);
        (*h).h_last = Some(history_def_last::<C>);
        (*h).h_prev = Some(history_def_prev::<C>);
        (*h).h_curr = Some(history_def_curr::<C>);
        (*h).h_set = Some(history_def_set::<C>);
        (*h).h_clear = Some(history_def_clear::<C>);
        (*h).h_enter = Some(history_def_enter::<C>);
        (*h).h_add = Some(history_def_add::<C>);
        (*h).h_del = Some(history_def_del::<C>);
    }
    h
}

// [spec:libedit:def:history.fun-history-end-fn]
// [spec:libedit:sem:history.fun-history-end-fn]
/// C: `void FUN(history,end)(TYPE(History) *h)` — `history_wend` in the wide
/// build and `history_end` in the narrow one, both declared in `histedit.h`.
/// The two spellings are [`history_wend`] and [`history_end`]. Frees `h`; the
/// caller must not touch it again.
pub fn history_end_gen<C: HistChar>(h: *mut HistoryGen<C>) {
    if h.is_null() {
        // The C does not check, so a NULL `h` dereferences — undefined, not a
        // defined no-op. Defined here as the no-op it looks like.
        return;
    }
    // The C's uninitialised scratch `ev`, used only by the clear callback,
    // which never writes it.
    let mut ev: HistEventGen<C> = scratch_ev();
    // SAFETY: a non-NULL `h` is one `history_init_gen` returned and this call
    // has not yet freed — the C's contract, and `h` is dangling afterwards.
    // The `history_def_clear` inside is guarded by the identity test, which
    // is what proves `h_ref` is this file's own builtin store.
    unsafe {
        if is_def_next((*h).h_next) {
            // Every entry is unlinked, its string freed and its node freed.
            // Per-entry `data` pointers are not freed: that memory belongs to
            // the caller and is simply forgotten. Any `HistEvent` the caller
            // still holds has a dangling `str` after this.
            history_def_clear((*h).h_ref, &mut ev);
        }
        // ERR-history-12: unconditional, custom function set or not. Because
        // `history_set_fun` never copies the caller's reference
        // (ERR-history-17, reproduced), `h_ref` is always the builtin store
        // this module allocated, so the free is well typed here — and stays
        // consistent with that choice, as the rule requires.
        if !(*h).h_ref.is_null() {
            free_alloc((*h).h_ref.cast::<HistoryTGen<C>>());
        }
        free_alloc(h);
    }
}

// [spec:libedit:def:history.history-setsize-fn]
// [spec:libedit:sem:history.history-setsize-fn]
/// C: `static int history_setsize(TYPE(History) *h, TYPE(HistEvent) *ev, int num)`
fn history_setsize<C: HistChar>(h: &mut HistoryGen<C>, ev: &mut HistEventGen<C>, num: i32) -> i32 {
    if !is_def_next(h.h_next) {
        he_seterrev(ev, _HE_NOT_ALLOWED);
        return -1;
    }
    if num < 0 {
        he_seterrev(ev, _HE_BAD_PARAM);
        return -1;
    }
    // C: `history_def_setsize(h->h_ref, num)`. No immediate eviction — the
    // list is trimmed only on the next enter, so a history can sit above its
    // configured maximum indefinitely. `num == 0` is legal and means "retain
    // nothing".
    // SAFETY: the identity test above proves `h_ref` is the builtin store.
    unsafe { (*h.h_ref.cast::<HistoryTGen<C>>()).max = num };
    // `*ev` is not written, so the caller still sees 0/"OK".
    0
}

// [spec:libedit:def:history.history-getsize-fn]
// [spec:libedit:sem:history.history-getsize-fn]
/// C: `static int history_getsize(TYPE(History) *h, TYPE(HistEvent) *ev)`
fn history_getsize<C: HistChar>(h: &mut HistoryGen<C>, ev: &mut HistEventGen<C>) -> i32 {
    if !is_def_next(h.h_next) {
        he_seterrev(ev, _HE_NOT_ALLOWED);
        return -1;
    }
    // ERR-history-34: the **current number of stored events**, not the
    // maximum `H_SETSIZE` configured. There is no way to query the maximum.
    // SAFETY: the identity test above proves `h_ref` is the builtin store.
    ev.num = unsafe { (*h.h_ref.cast::<HistoryTGen<C>>()).cur };
    // ERR-history-38: the C's `if (ev->num < -1)` `_HE_SIZE_NEGATIVE` branch
    // is unreachable, because `cur` is never negative. Not ported.
    // `ev->str` is left at the prologue's "OK".
    0
}

// [spec:libedit:def:history.history-setunique-fn]
// [spec:libedit:sem:history.history-setunique-fn]
/// C: `static int history_setunique(TYPE(History) *h, TYPE(HistEvent) *ev, int uni)`
fn history_setunique<C: HistChar>(
    h: &mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    uni: i32,
) -> i32 {
    if !is_def_next(h.h_next) {
        he_seterrev(ev, _HE_NOT_ALLOWED);
        return -1;
    }
    // Removes nothing already stored; it only affects future enters, and even
    // then only against the single newest entry.
    // SAFETY: the identity test above proves `h_ref` is the builtin store.
    unsafe {
        let store = h.h_ref.cast::<HistoryTGen<C>>();
        if uni != 0 {
            (*store).flags |= H_UNIQUE;
        } else {
            (*store).flags &= !H_UNIQUE;
        }
    }
    // Returns without writing `*ev`.
    0
}

// [spec:libedit:def:history.history-getunique-fn]
// [spec:libedit:sem:history.history-getunique-fn]
/// C: `static int history_getunique(TYPE(History) *h, TYPE(HistEvent) *ev)`
fn history_getunique<C: HistChar>(h: &mut HistoryGen<C>, ev: &mut HistEventGen<C>) -> i32 {
    if !is_def_next(h.h_next) {
        he_seterrev(ev, _HE_NOT_ALLOWED);
        return -1;
    }
    // Normalised to exactly 1 or 0, not the raw flag word.
    // SAFETY: the identity test above proves `h_ref` is the builtin store.
    ev.num = i32::from(unsafe { (*h.h_ref.cast::<HistoryTGen<C>>()).flags } & H_UNIQUE != 0);
    // `ev->str` keeps the prologue's "OK".
    0
}

// [spec:libedit:def:history.history-set-fun-fn]
// [spec:libedit:sem:history.history-set-fun-fn]
/// C: `static int history_set_fun(TYPE(History) *h, TYPE(History) *nh)`
///
/// `nh` is the caller's assembled vtable — the C's stack-local `hf` — and is
/// only read, so it is a shared borrow even though the C declares it
/// non-`const`.
fn history_set_fun<C: HistChar>(h: &mut HistoryGen<C>, nh: &HistoryGen<C>) -> i32 {
    // The C's uninitialised scratch, for `history_def_init`/`_clear`.
    let mut ev: HistEventGen<C> = scratch_ev();

    if nh.h_first.is_none()
        || nh.h_next.is_none()
        || nh.h_last.is_none()
        || nh.h_prev.is_none()
        || nh.h_curr.is_none()
        || nh.h_set.is_none()
        || nh.h_enter.is_none()
        || nh.h_add.is_none()
        || nh.h_clear.is_none()
        || nh.h_del.is_none()
        || nh.h_ref.is_null()
    {
        // Rejected — and, as a side effect, `h` is forced back onto the
        // builtin implementation if it was not already on it.
        if !is_def_next(h.h_next) {
            let old = h.h_ref;
            // A fresh, empty store with `max = 0`.
            if history_def_init(&mut h.h_ref, &mut ev, 0) == -1 {
                // `h` is left completely unchanged, `old` still installed.
                return -1;
            }
            // ERR-history-14, half fixed: the C overwrites `h_ref` without
            // freeing what was there. Because ERR-history-17 is reproduced,
            // `old` is this module's own builtin store and nothing else
            // refers to it once it is replaced, so freeing it is invisible
            // across the ABI. (The *other* half of that leak — the store the
            // acceptance path below abandons — cannot be fixed while
            // ERR-history-17 stands, because the caller's callbacks are
            // handed that very pointer.)
            if !old.is_null() {
                // SAFETY: `old` is the builtin store; see above.
                unsafe { free_alloc(old.cast::<HistoryTGen<C>>()) };
            }
            h.h_first = Some(history_def_first::<C>);
            h.h_next = Some(history_def_next::<C>);
            h.h_last = Some(history_def_last::<C>);
            h.h_prev = Some(history_def_prev::<C>);
            h.h_curr = Some(history_def_curr::<C>);
            h.h_set = Some(history_def_set::<C>);
            h.h_clear = Some(history_def_clear::<C>);
            h.h_enter = Some(history_def_enter::<C>);
            h.h_add = Some(history_def_add::<C>);
            h.h_del = Some(history_def_del::<C>);
        }
        // `h_ent` is *not* reset on this path.
        return -1;
    }

    if is_def_next(h.h_next) {
        // Every stored entry is deleted and freed. The `history_t` itself is
        // not freed — see ERR-history-14 above.
        // SAFETY: the identity test above proves `h_ref` is this file's own
        // builtin store, which is `history_def_clear`'s precondition; `ev` is
        // the local scratch.
        unsafe { history_def_clear(h.h_ref, &mut ev) };
    }

    h.h_ent = -1;
    // Exactly ten fields.
    h.h_first = nh.h_first;
    h.h_next = nh.h_next;
    h.h_last = nh.h_last;
    h.h_prev = nh.h_prev;
    h.h_curr = nh.h_curr;
    h.h_set = nh.h_set;
    h.h_clear = nh.h_clear;
    h.h_enter = nh.h_enter;
    h.h_add = nh.h_add;
    h.h_del = nh.h_del;

    // **ERR-history-17, reproduced deliberately**: `h->h_ref` is never
    // assigned from `nh->h_ref`, which was read above only to test it against
    // NULL. Every dispatch is `(*h->h_x)(h->h_ref, …)`, so the caller's ten
    // functions are invoked with the *old* reference — the builtin store the
    // clear above just emptied — and `H_FUNC` cannot work for any
    // non-trivial custom backend. `[dec:libedit:conformance-policy]` names
    // this as one of its six behavioural forks and defaults it to reproduce;
    // assigning `h_ref` here would also make `history_end_gen`'s unconditional
    // `free(h->h_ref)` (ERR-history-12) start freeing caller-owned memory.
    0
}

// [spec:libedit:def:history.history-load-fn]
// [spec:libedit:sem:history.history-load-fn]
/// C: `static int history_load(TYPE(History) *h, const char *fname)`
///
/// The path is narrow `char` in both builds. Nothing keeps it past the call,
/// so it is borrowed rather than raw.
fn history_load<C: HistChar>(h: &mut HistoryGen<C>, fname: &str) -> i32 {
    // The grammar this reads, and `history_save_fp` writes, is frozen by
    // `[dec:libedit:no-c-ffi]`:
    //
    //     file    := cookie entry*
    //     cookie  := "_HiStOrY_V2_" LF          ; exactly 13 bytes
    //     entry   := vis-encoded-text LF        ; final LF optional on read
    //
    // One line is always exactly one entry, because `strvis(…, VIS_WHITE)`
    // can never emit a literal LF. Nothing but the text is persisted: no
    // timestamp, no count, no event id, no flags, no per-entry `data`. Event
    // ids are regenerated by the insertion counter. Oldest first.

    // `i` is pre-initialised to -1, which is what every early return yields.
    let mut i: i32 = -1;

    let Ok(fp) = File::open(fname) else {
        return i;
    };
    let mut fp = BufReader::new(fp);

    // C: `getline(&line, &llen, fp)`, which keeps the LF and NUL-terminates.
    let mut line: Vec<u8> = Vec::new();
    let sz = match fp.read_until(b'\n', &mut line) {
        // -1 from `getline`: an empty file, or a read error.
        Ok(0) | Err(_) => return i,
        Ok(n) => n,
    };

    // ERR-history-22: the comparison length is the length of the line read,
    // not the length of the cookie, so any first line that is a proper prefix
    // of the cookie is accepted — a file whose entire content is `_HiS`
    // passes and then loads zero entries. A first line longer than the cookie
    // can never match, because the cookie's own NUL stops `strncmp` at a
    // mismatch. Reproduced exactly, `strncmp`'s NUL rule included.
    if !cookie_prefix_matches(&line[..sz]) {
        return i;
    }

    let mut max_size: usize = 1024;
    let mut ptr: Vec<u8> = Vec::new();
    if ptr.try_reserve_exact(max_size).is_err() {
        return i;
    }
    ptr.resize(max_size, 0);

    // The reused scratch decode buffer.
    // ERR-encoding-11, fixed as the errata directs: the C's function-`static`
    // `ct_buffer_t` leaks for the process lifetime and makes this function
    // non-thread-safe. An owned per-call buffer is not ABI-observable — the
    // decoded string is consumed before this returns.
    let mut conv = CtBufferT {
        cbuff: Vec::new(),
        csize: 0,
        wbuff: Vec::new(),
        wsize: 0,
    };
    let mut ev: HistEventGen<C> = scratch_ev();

    // `i` counts every line read, skipped ones included (ERR-history-23).
    i = 0;
    loop {
        line.clear();
        let mut sz = match fp.read_until(b'\n', &mut line) {
            // EOF, or a read error mid-file: the C's `getline` returns -1 for
            // both, which merely ends the loop and returns the count so far.
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        // A final line with no LF is processed unchanged, so a truncated file
        // still contributes its last partial entry.
        if sz > 0 && line[sz - 1] == b'\n' {
            sz -= 1;
            line.truncate(sz);
        }
        // `strunvis` reads `line` as a C string, so an embedded NUL silently
        // truncates the entry there and the rest of the line is lost.
        line.push(0);

        if max_size <= sz {
            // The grown size is always at least `sz + 1`, which is enough
            // because unvis output is never longer than its input.
            let want = (sz + 1024) & !1023usize;
            if ptr.try_reserve(want - ptr.len()).is_err() {
                i = -1;
                break;
            }
            ptr.resize(want, 0);
            max_size = want;
        }

        // ERR-history-09, **defined here**. The C ignores `strunvis`'s return
        // and `strunvis` does not write the terminating NUL when it rejects a
        // sequence, so the never-zeroed, reused buffer is then read as a C
        // string: the decoded prefix followed by the previous line's tail —
        // or uninitialised heap — up to whatever NUL happens to be there, and
        // off the end of the allocation if there is none. Zeroing the scratch
        // before each line makes that read exactly "the successfully decoded
        // prefix", which is the least surprising policy and the one the rule
        // names. Crucially the load **continues**: the C does not treat a
        // malformed line as fatal, and a port that rejected the file would
        // refuse input the C accepts.
        //
        // The bounded `strnunvis` rather than the C's unbounded `strunvis`,
        // for the same reason `history_save_out` uses `strnvis`: the growth
        // above proves the buffer holds `sz + 1` bytes and unvis output is
        // never longer than its input, so the bound never fires — but it is
        // enforced rather than assumed. `ENOSPC` would land on the same
        // decoded-prefix policy as a bad escape.
        // `decode` reports rather than truncating, and on a bad escape the
        // C's policy is the successfully decoded prefix — which `decode_into`
        // writes but does not measure, so the zero-fill-and-scan stays.
        let dlen = ptr.len();
        let decoded_len = crate::histfile::vis_decode_into(&mut ptr[..dlen], &line);

        // **The narrow/wide fork.** Wide: `mbstowcs` into the conversion
        // buffer, and NULL — `ptr` is not a valid multibyte string in the
        // current locale — silently skips the line, though the loop increment
        // still counts it (ERR-history-23). Narrow: `ct_decode_string(s, b)`
        // is `(s)`, so the strunvis output *is* the entry text, the C's
        // `if (decode_result == NULL) continue;` is dead code — `ptr` was
        // `h_malloc`ed and checked at the top — and **no line is ever
        // skipped**. A history file holding bytes that are invalid in
        // `LC_CTYPE` therefore loads completely through `history_init` and
        // partially through `history_winit`, which is the whole reason the
        // narrow store cannot be layered on the wide one.
        let Some(decoded) = C::decode(Some(&ptr[..decoded_len]), &mut conv) else {
            i = i.saturating_add(1);
            continue;
        };

        // Enter the decoded string as a new newest event; the history takes
        // its own copy. Wide, `ct_decode_string` NUL-terminates `conv.wbuff`;
        // narrow, the terminator is `ptr[decoded_len]`, which the `fill(0)`
        // above put there and which the slice bound stops just short of.
        // Either way the base pointer is the C string the C hands on. A
        // successful enter runs the normal eviction, so a file with more
        // lines than the configured maximum keeps only the last `max`.
        let str = decoded.as_ptr();
        if he(h.h_enter, h.h_ref, &mut ev, str) == -1 {
            i = -1;
            break;
        }
        i = i.saturating_add(1);
    }

    // The existing history is not cleared first, empty lines are entered as
    // empty events, and `h->h_ent` is left unchanged.
    i
}

/// C: `strncmp(line, hist_cookie, (size_t)sz) != 0`, where `sz` is the length
/// of the first line as `getline` returned it.
///
/// `strncmp`'s own NUL rule is part of the behaviour: the comparison stops at
/// the first NUL byte in either operand, so the cookie's terminator makes any
/// longer first line mismatch — unless that line itself holds a NUL at the
/// same offset, which the C accepts and so does this.
fn cookie_prefix_matches(line: &[u8]) -> bool {
    for (i, &b) in line.iter().enumerate() {
        let c = HIST_COOKIE.get(i).copied().unwrap_or(0);
        if b != c {
            return false;
        }
        if b == 0 {
            return true;
        }
    }
    true
}

/// A caller's `FILE *`, reduced to the two facilities
/// `sem:history.history-save-fp-fn` uses one for.
///
/// Not a ported C type. `H_SAVE_FP` and `H_NSAVE_FP` put a raw `FILE *` on
/// the public varargs ABI, and a `FILE *` is an opaque handle into the C
/// library's own object — its buffer, its position bookkeeping and its
/// descriptor all live in memory whose layout is that library's private
/// business. `[dec:libedit:no-c-ffi]` keeps this crate out of libc entirely,
/// so the stream is not named here at all: the ABI crate, which is where
/// that decision's third enumerated site lives, calls `ftell` and `fputs` on
/// the caller's object and hands the core just their two answers.
///
/// The split matters for more than tidiness. Writing through the descriptor
/// behind the stream instead — `fileno` plus `File::from_raw_fd` — answers a
/// *different* question: the stream carries a userspace buffer the descriptor
/// knows nothing about, so `ftell` and `lseek` disagree whenever anything is
/// pending, and a raw write reaches the file ahead of bytes the caller wrote
/// through the stream earlier. See [`history_save_fd`], which is that route
/// and is offered to Rust callers who have a descriptor and no stream.
pub struct SaveStream<'a> {
    /// C: `ftell(fp) == 0` — write the cookie, or skip it.
    ///
    /// False covers both the intended case (a stream already positioned past
    /// a header) and ERR-history-20: on a pipe or socket `ftell` returns -1,
    /// so the file comes out headerless and `history_load` later rejects it.
    pub at_start: bool,
    /// The stream as a byte sink: one call per `fputs`, `Err` for its `EOF`.
    ///
    /// It is neither flushed nor closed — the caller owns it, and the C
    /// leaves everything it wrote sitting in the stream's own buffer.
    pub out: &'a mut dyn Write,
}

// [spec:libedit:def:history.history-save-fp-fn]
// [spec:libedit:sem:history.history-save-fp-fn]
/// C: `static int history_save_fp(TYPE(History) *h, size_t nelem, FILE *fp)`
///
/// `fp` is the caller's stream, neither flushed nor closed here.
fn history_save_fp<C: HistChar>(h: &mut HistoryGen<C>, nelem: usize, fp: SaveStream<'_>) -> i32 {
    history_save_out(h, nelem, fp.out, fp.at_start)
}

/// The whole of `sem:history.history-save-fp-fn`, against a writer.
///
/// `at_start` is the C's `ftell(fp) == 0`: true writes the cookie first,
/// false skips it. Returns the number of entries written, or -1.
fn history_save_out<C: HistChar>(
    h: &mut HistoryGen<C>,
    nelem: usize,
    out: &mut dyn Write,
    at_start: bool,
) -> i32 {
    // Pre-initialised to -1, which is what the two failure exits yield.
    let mut i: i32 = -1;

    // ERR-history-20: on a non-seekable stream `ftell` returns -1, so the
    // cookie is *not* written and the result is a headerless file that
    // `history_load` later rejects outright. Reproduced by leaving the
    // decision with the caller, which is where the `ftell` lives.
    if at_start && out.write_all(HIST_COOKIE).is_err() {
        return i;
    }

    // ERR-encoding-11, as in `history_load`: an owned per-call buffer instead
    // of the C's function-`static` one.
    let mut conv = CtBufferT {
        cbuff: Vec::new(),
        csize: 0,
        wbuff: Vec::new(),
        wsize: 0,
    };
    let mut ev: HistEventGen<C> = scratch_ev();

    // Positioning.
    let mut retval;
    if nelem != usize::MAX {
        // ERR-history-19: the C's condition is `retval != -1 && nelem-- > 0`,
        // a *post*-decrement, so the walk stops **on** the entry at index
        // `nelem` from the newest and the write loop below then emits that
        // entry plus every newer one — `min(nelem + 1, size)` entries, not
        // `nelem`. `nelem == 0` writes one entry, the newest.
        let mut left = nelem;
        retval = hg(h.h_first, h.h_ref, &mut ev);
        while retval != -1 && left > 0 {
            left -= 1;
            retval = hg(h.h_next, h.h_ref, &mut ev);
        }
    } else {
        retval = -1;
    }
    if retval == -1 {
        // Either `nelem == (size_t)-1`, or the list ran out first — in which
        // case everything is saved.
        retval = hg(h.h_last, h.h_ref, &mut ev);
    }

    // The write loop: from the positioned entry toward newer entries, so the
    // file comes out **oldest first, newest last**, which is what makes
    // `history_load`'s top-to-bottom enter restore the original ordering.
    i = 0;
    while retval != -1 {
        // SAFETY: `ev.str` is the store's own NUL-terminated entry text,
        // valid until that entry is deleted or replaced.
        let str = if ev.str.is_null() {
            None
        } else {
            Some(unsafe { core::slice::from_raw_parts(ev.str, s_len(ev.str)) })
        };
        // **The narrow/wide fork's other half.** Wide: `wcstombs` through the
        // conversion buffer, which can fail on an allocation or on a character
        // the locale cannot encode. Narrow: `ct_encode_string(s, b)` is `(s)`,
        // so the entry's own bytes go straight to `strvis` and there is
        // nothing to fail — the only NULL reachable is `ev.str` itself, from a
        // caller-supplied function set.
        //
        // ERR-history-08, **defined here** and identically for both: the C
        // does not check the return before `strlen(str)`, so either cause is a
        // NULL dereference. Defined as the same stop-and-report-failure the
        // C's own allocation failure below takes.
        //
        // What is *not* forked is the file: the cookie, the `VIS_WHITE`
        // escaping and the one-line-per-entry framing are bytes in both
        // builds, which is why a history file written through `history_w` and
        // one written through `history` are the same file for the same content
        // in the same locale.
        let Some(bytes) = C::encode(str, &mut conv) else {
            i = -1;
            break;
        };

        // `VIS_WHITE` is the on-disk flag: space, tab, newline and backslash
        // become three-digit octal escapes, everything else graphic passes
        // through, and no encoded entry can ever contain a literal LF.
        //
        // The C sizes a buffer at `strlen(str) * 4 + 1` and calls the
        // unbounded `strvis` into it — ERR-history-07's sibling, an
        // assumption a locale can break into a heap smash. `encode` returns
        // what it produced, so there is no size to get wrong. Neither the
        // buffer nor the growth step was ever observable.
        let Some(mut ptr) = crate::histfile::vis_encode(bytes, crate::histfile::VisFlags::WHITE)
        else {
            i = -1;
            break;
        };
        let n = ptr.len();

        // C: `fprintf(fp, "%s\n", ptr)` — one line per entry. ERR-history-21:
        // the return is ignored, so `ENOSPC`, `EIO` and a full pipe are
        // invisible and the function still reports success.
        //
        // One write, not two: `strnvis` terminates at `ptr[n]`, so writing the
        // newline over that terminator hands the sink exactly the bytes the
        // C's `"%s\n"` produces, in exactly one call — which is what makes a
        // sink that is a `FILE *` do one `fputs` per entry, as the C does one
        // `fprintf`. The slot is always there: `strnvis` wrote its NUL into
        // it.
        ptr.push(b'\n');
        let _ = out.write_all(&ptr[..n + 1]);

        i = i.saturating_add(1);
        retval = hg(h.h_prev, h.h_ref, &mut ev);
    }

    // The stream is neither flushed nor closed — the caller owns it.
    i
}

/// [`history_save_fp`] against a descriptor rather than a `FILE *`.
///
/// Not a ported function, and **not** the route `H_SAVE_FP` takes: it is the
/// C's `history_save_fp` with the caller's stream replaced by the descriptor
/// behind it, which is a different object. A stream carries a userspace
/// buffer the descriptor knows nothing about, so `ftell` and `lseek` disagree
/// whenever anything is pending and a write here reaches the file ahead of
/// bytes the caller wrote through the stream earlier. That is why the ABI
/// crate calls `ftell`/`fputs` on the stream itself and hands the core a
/// [`SaveStream`]; this entry point exists for a Rust caller that has a
/// descriptor and no stream, which is every caller of this crate that is not
/// the C ABI. `nelem == usize::MAX` is the C's `(size_t)-1`, "all entries".
///
/// The descriptor is borrowed: it is neither closed nor left with buffered
/// bytes. The cookie is written only when the descriptor is at offset 0,
/// which is ERR-history-20 in descriptor terms — a pipe or socket cannot
/// report a position, so it gets no header.
pub fn history_save_fd<C: HistChar>(h: &mut HistoryGen<C>, nelem: usize, fd: RawFd) -> i32 {
    if fd < 0 {
        return -1;
    }
    // SAFETY: `fd` is the caller's descriptor and stays open for the duration
    // of this call; `ManuallyDrop` is what keeps this borrow from closing it,
    // which the C never does either.
    let mut file = ManuallyDrop::new(unsafe { File::from_raw_fd(fd) });
    // C: `ftell(fp) == 0`. An error — the descriptor is not seekable — is the
    // C's -1 return, i.e. "not at the start", so no cookie.
    let at_start = matches!(file.stream_position(), Ok(0));
    let mut out = BufWriter::new(&mut *file);
    let i = history_save_out(h, nelem, &mut out, at_start);
    // The C leaves the stream unflushed for the caller to deal with, but a
    // `BufWriter` that goes out of scope with bytes in it would drop them
    // silently; flushing here is what makes this the C's *unbuffered*
    // `fprintf` sequence. The result is discarded, as the C discards
    // `fprintf`'s (ERR-history-21).
    let _ = out.flush();
    i
}

// [spec:libedit:def:history.history-save-fn]
// [spec:libedit:sem:history.history-save-fn]
/// C: `static int history_save(TYPE(History) *h, const char *fname)`
fn history_save<C: HistChar>(h: &mut HistoryGen<C>, fname: &str) -> i32 {
    // C: `open(fname, O_WRONLY|O_CREAT|O_TRUNC, S_IRUSR|S_IWUSR)` — mode
    // 0600, and `O_TRUNC`, so an existing file is destroyed before anything
    // is written. No temp file and rename, no lock, no backup: an interrupted
    // save leaves a truncated history file.
    let Ok(file) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(fname)
    else {
        return -1;
    };
    // ERR-history-16: the C's `fdopen` failure returns -1 without closing the
    // descriptor it opened, leaking it. The rule recommends diverging and
    // closing; there is no separate `fdopen` step here at all, and the `File`
    // closes itself on every path, so the leak cannot happen.
    let mut out = BufWriter::new(file);

    // C: `history_save_fp(h, (size_t)-1, fp)`. The stream is fresh at offset
    // 0, so the cookie is always written, and `(size_t)-1` means "all
    // entries, oldest first". The file is this function's own — the C's
    // `fdopen` of the descriptor it just opened, not a caller's stream — so
    // it calls the shared body directly with the `ftell` answer it already
    // knows, instead of building a [`SaveStream`] to say the same thing.
    let i = history_save_out(h, usize::MAX, &mut out, true);

    // C: `fclose(fp)` with the **return value ignored**, so a failure to
    // flush the final buffer is silently swallowed and success is still
    // reported (ERR-history-21).
    let _ = out.flush();
    i
}

// [spec:libedit:def:history.history-prev-event-fn]
// [spec:libedit:sem:history.history-prev-event-fn]
/// C: `static int history_prev_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)`
fn history_prev_event<C: HistChar>(
    h: &mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    num: i32,
) -> i32 {
    // If the cursor is invalid `HCURR` fails immediately and the body never
    // runs. `HPREV` walks toward the head, i.e. toward newer entries.
    let mut retval = hg(h.h_curr, h.h_ref, ev);
    while retval != -1 {
        if ev.num == num {
            return 0;
        }
        retval = hg(h.h_prev, h.h_ref, ev);
    }
    // ERR-history-24: the cursor is left wherever the scan stopped — on the
    // newest entry after an exhausted search — and is not restored.
    he_seterrev(ev, _HE_NOT_FOUND);
    -1
}

// [spec:libedit:def:history.history-next-evdata-fn]
// [spec:libedit:sem:history.history-next-evdata-fn]
/// C: `static int history_next_evdata(TYPE(History) *h, TYPE(HistEvent) *ev,
/// int num, void **d)`
fn history_next_evdata<C: HistChar>(
    h: &mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    num: i32,
    d: *mut *mut c_void,
) -> i32 {
    // Despite the name this does not advance to a "next" event: `HPREV` walks
    // toward *newer* entries, exactly as `history_prev_event` does.
    let mut retval = hg(h.h_curr, h.h_ref, ev);
    while retval != -1 {
        if ev.num == num {
            if !d.is_null() {
                // ERR-history-02, **defined here**: the C casts `h->h_ref` to
                // `history_t *` unconditionally, which is type-confused
                // memory access under a caller-supplied function set. The
                // errata's disposition is to restrict the operation to the
                // builtin backend, so a custom set gets the refusal the other
                // builtin-only operations already return.
                if !is_def_next(h.h_next) {
                    he_seterrev(ev, _HE_NOT_ALLOWED);
                    return -1;
                }
                // The `data` pointer is borrowed, not copied; ownership stays
                // with whoever set it via `H_REPLACE`.
                // SAFETY: the identity test proves `h_ref` is the builtin
                // store, and `HCURR` succeeding proves the cursor is on a
                // real entry.
                unsafe { *d = (*(*h.h_ref.cast::<HistoryTGen<C>>()).cursor).data };
            }
            return 0;
        }
        retval = hg(h.h_prev, h.h_ref, ev);
    }
    // `*d` is left untouched.
    he_seterrev(ev, _HE_NOT_FOUND);
    -1
}

// [spec:libedit:def:history.history-next-event-fn]
// [spec:libedit:sem:history.history-next-event-fn]
/// C: `static int history_next_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)`
fn history_next_event<C: HistChar>(
    h: &mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    num: i32,
) -> i32 {
    // `HNEXT` is one step toward the tail, i.e. toward older entries.
    let mut retval = hg(h.h_curr, h.h_ref, ev);
    while retval != -1 {
        if ev.num == num {
            return 0;
        }
        retval = hg(h.h_next, h.h_ref, ev);
    }
    // The cursor is left on the oldest entry after an exhausted search.
    he_seterrev(ev, _HE_NOT_FOUND);
    -1
}

// [spec:libedit:def:history.history-prev-string-fn]
// [spec:libedit:sem:history.history-prev-string-fn]
/// C: `static int history_prev_string(TYPE(History) *h, TYPE(HistEvent) *ev,
/// const Char *str)`
fn history_prev_string<C: HistChar>(
    h: &mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    str: *const C,
) -> i32 {
    // Computed once, in `Char`s.
    // SAFETY: `str` is NULL or a NUL-terminated string; a NULL reads as
    // `L""`, which makes the prefix test match the current event at once.
    let len = unsafe { s_len(str) };
    let mut retval = hg(h.h_curr, h.h_ref, ev);
    while retval != -1 {
        // A prefix test, not a substring or equality test. An empty `str`
        // matches the very first candidate — the current event — and the scan
        // includes the current event, so an already-selected event can match
        // itself.
        // SAFETY: as above, and `ev->str` is the store's own entry text.
        if unsafe { s_ncmp_eq(str, ev.str, len) } {
            return 0;
        }
        // ERR-history-36: `history_prev_string` walks with `HNEXT` (toward
        // older) while `history_next_string` walks with `HPREV` (toward
        // newer) — the exact opposite pairing from the two event-id searches.
        // Real, observable, and not to be "corrected".
        retval = hg(h.h_next, h.h_ref, ev);
    }
    he_seterrev(ev, _HE_NOT_FOUND);
    -1
}

// [spec:libedit:def:history.history-next-string-fn]
// [spec:libedit:sem:history.history-next-string-fn]
/// C: `static int history_next_string(TYPE(History) *h, TYPE(HistEvent) *ev,
/// const Char *str)`
fn history_next_string<C: HistChar>(
    h: &mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    str: *const C,
) -> i32 {
    // SAFETY: as in `history_prev_string`.
    let len = unsafe { s_len(str) };
    let mut retval = hg(h.h_curr, h.h_ref, ev);
    while retval != -1 {
        // SAFETY: as in `history_prev_string`.
        if unsafe { s_ncmp_eq(str, ev.str, len) } {
            return 0;
        }
        // ERR-history-36: toward *newer* entries, the reverse of
        // `history_prev_string`'s direction.
        retval = hg(h.h_prev, h.h_ref, ev);
    }
    he_seterrev(ev, _HE_NOT_FOUND);
    -1
}

/// The trailing argument of one [`history_gen`] operation.
///
/// Not a ported C type: it is what the varargs tail of `FUNW(history)` becomes
/// once `va_list` is dropped. `plan/decisions/idiomatic-core.md` puts the
/// varargs face in the ABI crate, so the ABI shim is what reads the `va_list`
/// and hands the core one of these; every opcode's argument list from
/// `sem:history.funw-history-fn` has a variant here, including the two-argument
/// ones the C reads in a fixed order.
pub enum HistoryArg<'a, C> {
    /// No trailing argument: `H_GETSIZE`, `H_FIRST`, `H_LAST`, `H_PREV`,
    /// `H_NEXT`, `H_CURR`, `H_END`, `H_CLEAR`, `H_GETUNIQUE`.
    None,
    /// One `int`: `H_SETSIZE`, `H_SET`, `H_SETUNIQUE`, `H_DEL`,
    /// `H_NEXT_EVENT`, `H_PREV_EVENT`.
    Num(i32),
    /// One `const Char *`: `H_ADD`, `H_ENTER`, `H_APPEND`, `H_NEXT_STR`,
    /// `H_PREV_STR`.
    Str(*const C),
    /// One `const char *` path, narrow in both builds: `H_LOAD`, `H_SAVE`.
    Path(&'a str),
    /// One `FILE *`: `H_SAVE_FP`. The stream arrives as the two answers the
    /// ABI crate read off it; see [`SaveStream`].
    Fp(SaveStream<'a>),
    /// `size_t nelem` then `FILE *`: `H_NSAVE_FP`.
    NSaveFp(usize, SaveStream<'a>),
    /// `int num` then `void **d`: `H_NEXT_EVDATA`, `H_DELDATA`. The pointer
    /// stays raw because `H_DELDATA` accepts the magic `(void **)-1`.
    EvData(i32, *mut *mut c_void),
    /// `const Char *line` then `void *data`: `H_REPLACE`.
    Replace(*const C, *mut c_void),
    /// `H_FUNC`'s eleven varargs, already collected the way the C collects
    /// them into its stack-local `TYPE(History) hf`: the cookie plus the ten
    /// callbacks. `h_ent` is not part of the argument list and is never read.
    Funcs(&'a HistoryGen<C>),
}

// [spec:libedit:def:history.funw-history-fn]
// [spec:libedit:sem:history.funw-history-fn]
/// C: `int FUNW(history)(TYPE(History) *h, TYPE(HistEvent) *ev, int fun, ...)`
/// — `history_w` in the wide build and `history` in the narrow one, both
/// declared in `histedit.h`. The two spellings are [`history_w`] and
/// [`history`].
///
/// The varargs tail becomes a single [`HistoryArg`]; `fun` stays the raw `int`
/// opcode, whose numbering is ABI. `h` is raw because the C does not check it
/// for NULL, because internal callers reach it through `el_history.ref` (a
/// `void *`), and because `H_END` frees it here.
pub fn history_gen<C: HistChar>(
    h: *mut HistoryGen<C>,
    ev: &mut HistEventGen<C>,
    fun: i32,
    arg: HistoryArg<'_, C>,
) -> i32 {
    // The prologue, always: `he_seterrev(ev, _HE_OK)`. Any opcode whose
    // handler does not itself write `*ev` therefore leaves the caller looking
    // at 0/"OK" — true of `H_SET`, `H_CLEAR`, `H_REPLACE` and the successful
    // paths of `H_SETSIZE`/`H_SETUNIQUE`, while `H_GETSIZE`/`H_GETUNIQUE`
    // overwrite only `num`.
    he_seterrev(ev, _HE_OK);

    if h.is_null() {
        // The C checks neither `h` nor `ev`, so a NULL handle dereferences —
        // undefined, not a defined error. Defined here as the dispatcher's
        // own generic failure.
        he_seterrev(ev, _HE_UNKNOWN);
        return -1;
    }

    // Lifted out of the match because it frees `h`: the caller must not touch
    // it again, and no borrow can span the free. `ev` remains valid and reads
    // 0/"OK".
    if fun == H_END {
        history_end_gen(h);
        return 0;
    }

    // SAFETY: `h` is non-NULL and, per the C's contract, is a live handle
    // from `history_init_gen` that `history_end_gen` has not freed.
    let h = unsafe { &mut *h };

    // A recognised opcode whose trailing argument has the wrong shape. In the
    // C that is a `va_arg` type mismatch — undefined behaviour, unchecked —
    // and the ABI shim is what builds `arg`, so this is defined as the
    // "required parameter(s) not supplied" the `H_FUNC` validator already
    // uses for a malformed argument list.
    macro_rules! bad_arg {
        () => {{
            he_seterrev(ev, _HE_PARAM_MISSING);
            return -1;
        }};
    }

    match fun {
        H_GETSIZE => history_getsize(h, ev),

        H_SETSIZE => match arg {
            HistoryArg::Num(n) => history_setsize(h, ev, n),
            _ => bad_arg!(),
        },

        H_GETUNIQUE => history_getunique(h, ev),

        H_SETUNIQUE => match arg {
            HistoryArg::Num(n) => history_setunique(h, ev, n),
            _ => bad_arg!(),
        },

        H_ADD => match arg {
            HistoryArg::Str(str) => he(h.h_add, h.h_ref, ev, str),
            _ => bad_arg!(),
        },

        H_DEL => match arg {
            HistoryArg::Num(n) => hs(h.h_del, h.h_ref, ev, n),
            _ => bad_arg!(),
        },

        H_ENTER => match arg {
            HistoryArg::Str(str) => {
                let retval = he(h.h_enter, h.h_ref, ev, str);
                if retval != -1 {
                    // ERR-history-25: the builtin enter returns 1 on a real
                    // insert and 0 when `H_UNIQUE` suppressed it — both
                    // `!= -1` — and a suppressed enter never wrote `*ev`, so
                    // this stores the prologue's 0, which matches no event.
                    // A following `H_APPEND` then fails with
                    // `_HE_NOT_FOUND`.
                    h.h_ent = ev.num;
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_APPEND => match arg {
            HistoryArg::Str(str) => {
                // With `h_ent == -1` (nothing entered yet) the set fails and
                // the append is skipped.
                let mut retval = hs(h.h_set, h.h_ref, ev, h.h_ent);
                if retval != -1 {
                    retval = he(h.h_add, h.h_ref, ev, str);
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_FIRST => hg(h.h_first, h.h_ref, ev),
        H_NEXT => hg(h.h_next, h.h_ref, ev),
        H_LAST => hg(h.h_last, h.h_ref, ev),
        H_PREV => hg(h.h_prev, h.h_ref, ev),
        H_CURR => hg(h.h_curr, h.h_ref, ev),

        H_SET => match arg {
            HistoryArg::Num(n) => hs(h.h_set, h.h_ref, ev, n),
            _ => bad_arg!(),
        },

        H_CLEAR => {
            // The callback returns `void`, so `H_CLEAR` can never fail.
            hv(h.h_clear, h.h_ref, ev);
            0
        }

        H_LOAD => match arg {
            HistoryArg::Path(path) => {
                let retval = history_load(h, path);
                if retval == -1 {
                    he_seterrev(ev, _HE_HIST_READ);
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_SAVE => match arg {
            HistoryArg::Path(path) => {
                let retval = history_save(h, path);
                if retval == -1 {
                    he_seterrev(ev, _HE_HIST_WRITE);
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_SAVE_FP => match arg {
            HistoryArg::Fp(fp) => {
                // The stream is neither flushed nor closed.
                let retval = history_save_fp(h, usize::MAX, fp);
                if retval == -1 {
                    he_seterrev(ev, _HE_HIST_WRITE);
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_NSAVE_FP => match arg {
            // The C reads the `size_t` into a local first, so the argument
            // evaluation order is well defined; the tuple already is.
            HistoryArg::NSaveFp(nelem, fp) => {
                let retval = history_save_fp(h, nelem, fp);
                if retval == -1 {
                    he_seterrev(ev, _HE_HIST_WRITE);
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_PREV_EVENT => match arg {
            HistoryArg::Num(n) => history_prev_event(h, ev, n),
            _ => bad_arg!(),
        },

        H_NEXT_EVENT => match arg {
            HistoryArg::Num(n) => history_next_event(h, ev, n),
            _ => bad_arg!(),
        },

        H_PREV_STR => match arg {
            HistoryArg::Str(str) => history_prev_string(h, ev, str),
            _ => bad_arg!(),
        },

        H_NEXT_STR => match arg {
            HistoryArg::Str(str) => history_next_string(h, ev, str),
            _ => bad_arg!(),
        },

        H_FUNC => match arg {
            HistoryArg::Funcs(nh) => {
                // The C assigns this in the middle of reading the eleven
                // varargs, immediately after `ref`; the arguments are already
                // collected here, so the position is not observable. It
                // happens whether or not the install is accepted.
                h.h_ent = -1;
                let retval = history_set_fun(h, nh);
                if retval == -1 {
                    he_seterrev(ev, _HE_PARAM_MISSING);
                }
                retval
            }
            _ => bad_arg!(),
        },

        H_NEXT_EVDATA => match arg {
            HistoryArg::EvData(num, d) => history_next_evdata(h, ev, num, d),
            _ => bad_arg!(),
        },

        H_DELDATA => match arg {
            HistoryArg::EvData(num, d) => {
                // ERR-history-02, defined as in `history_next_evdata`: the
                // whole operation needs the builtin store, so a custom
                // function set is refused rather than type-confused.
                if !is_def_next(h.h_next) {
                    he_seterrev(ev, _HE_NOT_ALLOWED);
                    return -1;
                }
                // `d == (void **)-1` is the documented magic value meaning
                // "position only, do not delete"; `history_deldata_nth`
                // handles it.
                // SAFETY: the identity test proves `h_ref` is the builtin
                // store, and nothing else borrows it here.
                let store = unsafe { &mut *h.h_ref.cast::<HistoryTGen<C>>() };
                history_deldata_nth(store, ev, num, d)
            }
            _ => bad_arg!(),
        },

        // Documented as usable only immediately after `H_NEXT_EVDATA`.
        H_REPLACE => match arg {
            HistoryArg::Replace(line, data) => {
                // ERR-history-02 again.
                if !is_def_next(h.h_next) {
                    he_seterrev(ev, _HE_NOT_ALLOWED);
                    return -1;
                }
                if line.is_null() {
                    // `ev` is left at 0/"OK", so a failed `H_REPLACE` reports
                    // no error string.
                    return -1;
                }
                let Some(s) = s_dup(line) else {
                    return -1;
                };
                // SAFETY: the identity test proves `h_ref` is the builtin
                // store; `cursor` is either a live entry or the sentinel.
                unsafe {
                    let store = h.h_ref.cast::<HistoryTGen<C>>();
                    let cursor = (*store).cursor;
                    if cursor == &raw mut (*store).list {
                        // ERR-history-13, second half: the C writes the
                        // duplicate into the *sentinel's* `ev.str`, silently
                        // corrupting the list header. The errata's
                        // disposition is not to reproduce that, so the
                        // request is refused and the duplicate released.
                        s_free(s);
                        return -1;
                    }
                    // ERR-history-13, first half, reproduced: the previous
                    // string is overwritten **without being freed**. That
                    // leak is the observable contract — an `he->line` a
                    // caller took from an earlier operation stays valid
                    // indefinitely — so the old pointer is deliberately
                    // dropped on the floor rather than passed to `s_free`.
                    (*cursor).ev.str = s;
                    (*cursor).data = data;
                }
                0
            }
            _ => bad_arg!(),
        },

        _ => {
            he_seterrev(ev, _HE_UNKNOWN);
            -1
        }
    }
}

// ---------------------------------------------------------------------------
// The two instantiations. C: `history.c` compiled as itself, and `historyn.c`
// compiling it again under `NARROWCHAR`. Each of the six is one call into the
// shared source above with the character type pinned; the `def`/`sem` rules
// for them belong to `histedit.h`, which is where the declarations are, so
// they are claimed at the ABI boundary rather than here.
// ---------------------------------------------------------------------------

/// C: `HistoryW *history_winit(void)` — `FUN(history,init)` with
/// `Char = wchar_t`.
pub fn history_winit() -> *mut HistoryW {
    history_init_gen::<u32>()
}

/// C: `void history_wend(HistoryW *)` — `FUN(history,end)` with
/// `Char = wchar_t`.
pub fn history_wend(h: *mut HistoryW) {
    history_end_gen::<u32>(h);
}

/// C: `int history_w(HistoryW *, HistEventW *, int, ...)` — `FUNW(history)`
/// with `Char = wchar_t`.
pub fn history_w(h: *mut HistoryW, ev: &mut HistEventW, fun: i32, arg: HistoryArg<'_, u32>) -> i32 {
    history_gen::<u32>(h, ev, fun, arg)
}

/// C: `History *history_init(void)` — `FUN(history,init)` with `Char = char`,
/// i.e. the whole of `historyn.c`'s contribution to this entry point.
///
/// The store is bytes: entry text is `strdup`ed and compared byte for byte,
/// `ev->str` is a `char *`, and nothing here consults the locale. A byte
/// sequence that is not valid in `LC_CTYPE` round-trips through this history
/// unchanged, which is the reason it cannot be layered on the wide one.
pub fn history_init() -> *mut History {
    history_init_gen::<c_char>()
}

/// C: `void history_end(History *)` — `FUN(history,end)` with `Char = char`.
pub fn history_end(h: *mut History) {
    history_end_gen::<c_char>(h);
}

/// C: `int history(History *, HistEvent *, int, ...)` — `FUNW(history)` with
/// `Char = char`.
pub fn history(h: *mut History, ev: &mut HistEvent, fun: i32, arg: HistoryArg<'_, c_char>) -> i32 {
    history_gen::<c_char>(h, ev, fun, arg)
}

mod owned;
pub use owned::OwnedHistoryW;
