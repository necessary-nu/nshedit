//! The exported entry points of `filecomplete.c`; rules in
//! `docs/spec/port/src/filecomplete.md`.
//!
//! `filecomplete.h` is not installed (`src/Makefile.am:55`), and only
//! `_el_fn_complete` and `_el_fn_sh_complete` — which live in
//! [`crate::histedit`] with the rest of `histedit.h` — are declared to
//! applications. The five below are declared nowhere a consumer can include,
//! and are exported all the same: none carries `libedit_private`, our oracle
//! exports all five, and so does Debian's `libedit.so.2`. The symbol table is
//! the contract, so a consumer that declared them itself and links against us
//! must find them.
//!
//! `completion_matches` is the sixth name in that header. It is readline's,
//! not this file's — the header says so, `/* XXX: readline */` — and it is
//! exported from [`crate::readline`] alongside its own rules.
//!
//! # Callbacks the caller supplies
//!
//! Three of these parameters are C function pointers, and
//! `plan/decisions/idiomatic-core.md` gives the core Rust-shaped ones
//! instead: a `&mut dyn FnMut` generator, and bare `fn` pointers for the
//! attempted-completion and append-character hooks. A generator becomes a
//! closure and needs nothing further. The other two are `fn` pointers with
//! nowhere to carry a captured value, so the caller's pointer is parked in a
//! thread-local for the duration of the call and a fixed adapter reads it
//! back — [`HookGuard`], which saves and restores so a callback that
//! re-enters `fn_complete2` does not lose its own hooks.
//!
//! That is the same shape [`crate::readline`] already uses for
//! `rl_attempted_completion_function` and `rl_completion_append_character`,
//! except that there the hooks are exported globals and the parking is the C's
//! own.

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use nshedit::el::EditLine;
use nshedit::filecomplete::{self, FilenameCompletionState};
use nshedit::histedit::CC_ERROR;

use crate::readline::{c_bytes, c_bytes_opt, c_dup, c_free_array, c_free_str};

/// C: `char *(*)(const char *, int)` — the match generator `fn_complete2`
/// calls until it answers NULL. The C frees what it returns.
pub type CompleteFuncC = unsafe extern "C" fn(*const c_char, c_int) -> *mut c_char;

/// C: `char **(*)(const char *, int, int)` — the application's own completion
/// hook, tried before the generator. The C takes ownership of the array and
/// of every string in it.
pub type AttemptedCompletionFuncC =
    unsafe extern "C" fn(*const c_char, c_int, c_int) -> *mut *mut c_char;

/// C: `const char *(*)(const char *)` — chooses what is appended after a
/// completed name. The result must not be freed, so it points at storage the
/// hook owns.
pub type AppFuncC = unsafe extern "C" fn(*const c_char) -> *const c_char;

thread_local! {
    /// The `app_func` argument of the call in progress, for [`app_func_adapter`].
    static APP_FUNC: Cell<Option<AppFuncC>> = const { Cell::new(None) };

    /// The `attempted_completion_function` argument of the call in progress,
    /// for [`attempted_adapter`].
    static ATTEMPTED: Cell<Option<AttemptedCompletionFuncC>> = const { Cell::new(None) };

    /// Every distinct string an `app_func` has returned, leaked so the core's
    /// `&'static str` is honest.
    ///
    /// The C's contract for that return is *a literal the caller must not
    /// free*, which is a `&'static str` in all but the type; the copy is what
    /// makes it one, since the hook may hand back a shared buffer it
    /// overwrites on the next call — `readline.c`'s own
    /// `_rl_completion_append_character_function` does exactly that. Distinct
    /// values are interned rather than copied per call, so a well-behaved hook
    /// leaks its handful of literals once and no more.
    static INTERNED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());

    /// The file-statics inside `fn_filename_completion_function`, which the
    /// core made an explicit state object.
    ///
    /// One per process, as in the C: the generator's scan cursor is a
    /// function-level `static DIR *` there, so two interleaved scans corrupt
    /// each other. See `sem:filecomplete.fn-filename-completion-function-fn`.
    static FILENAME_SCAN: RefCell<Option<FilenameCompletionState>> = const { RefCell::new(None) };
}

/// Installs the caller's two `fn`-pointer hooks for the duration of one call
/// and puts back whatever was there.
///
/// Restoring rather than clearing is what makes re-entry safe: an
/// `attempted_completion_function` is free to call `fn_complete2` again, and
/// the C's arguments are per-call, so the inner call's hooks must not outlive
/// it.
struct HookGuard(Option<AppFuncC>, Option<AttemptedCompletionFuncC>);

impl HookGuard {
    fn install(app: Option<AppFuncC>, attempted: Option<AttemptedCompletionFuncC>) -> Self {
        Self(APP_FUNC.replace(app), ATTEMPTED.replace(attempted))
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        APP_FUNC.set(self.0);
        ATTEMPTED.set(self.1);
    }
}

/// A `&'static str` with the same bytes as `b`, allocated at most once per
/// distinct value.
///
/// Invalid UTF-8 is replaced rather than rejected: the core's `AppFunc`
/// returns `&'static str` and there is no error channel, and the C would have
/// appended the bytes unexamined. `sem:filecomplete.escape-filename-fn`
/// already records what a non-ASCII append byte does there.
fn intern(b: &[u8]) -> &'static str {
    INTERNED.with_borrow_mut(|set| {
        let s = String::from_utf8_lossy(b);
        if let Some(&hit) = set.get(&*s) {
            return hit;
        }
        let leaked: &'static str = Box::leak(s.into_owned().into_boxed_str());
        set.insert(leaked);
        leaked
    })
}

/// The caller's `app_func` in the shape the core's [`filecomplete::AppFunc`]
/// wants.
///
/// A NULL return is the C's undefined case — `escape_filename` indexes
/// `append_char[0]` and `fn_display_match_list` hands it to `fprintf("%s")` —
/// and is defined here as appending nothing.
fn app_func_adapter(name: &str) -> &'static str {
    let Some(f) = APP_FUNC.get() else {
        return "";
    };
    let mut arg = Vec::with_capacity(name.len() + 1);
    arg.extend_from_slice(name.as_bytes());
    arg.push(0);
    // SAFETY: `arg` is NUL-terminated and outlives the call; the C hands this
    // hook a pointer into its own buffer the same way, and the hook must not
    // free it. What comes back is borrowed, never freed here, as the C's
    // contract for it says.
    let p = unsafe { f(arg.as_ptr().cast::<c_char>()) };
    if p.is_null() {
        return "";
    }
    // SAFETY: a non-NULL return is a NUL-terminated string.
    intern(unsafe { CStr::from_ptr(p) }.to_bytes())
}

/// The caller's `attempted_completion_function` in the shape the core's
/// [`filecomplete::AttemptedCompletionFunc`] wants.
///
/// Ownership follows the C: `fn_complete2` takes the returned array, so the
/// array and its strings are released here once copied into the `Vec`.
fn attempted_adapter(text: &str, start: i32, end: i32) -> Option<Vec<String>> {
    let hook = ATTEMPTED.get()?;
    // SAFETY: the hook is the caller's own function pointer, called with a
    // NUL-terminated copy of `text` exactly as the C calls it.
    unsafe {
        let ctext = c_dup(text.as_bytes());
        if ctext.is_null() {
            return None;
        }
        let matches = hook(ctext, start, end);
        c_free_str(ctext);
        if matches.is_null() {
            return None;
        }
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let m = *matches.add(i);
            if m.is_null() {
                break;
            }
            out.push(String::from_utf8_lossy(c_bytes(m)).into_owned());
            c_free_str(m);
            i += 1;
        }
        c_free_array(matches, i + 1);
        Some(out)
    }
}

/// The C's `const wchar_t *` as a slice, up to but not including the
/// terminating `L'\0'`; `None` for its NULL.
///
/// # Safety
///
/// `p` must be NULL or point at a `L'\0'`-terminated wide string that
/// outlives the slice.
unsafe fn wide_upto_nul<'a>(p: *const u32) -> Option<&'a [u32]> {
    if p.is_null() {
        return None;
    }
    let mut n = 0usize;
    // SAFETY: the caller guarantees a terminated string.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: as above.
    Some(unsafe { core::slice::from_raw_parts(p, n) })
}

/// The C's `int *` out-parameter as the core's `Option<&mut i32>`.
///
/// # Safety
///
/// `p` must be NULL or writable for the life of the borrow.
unsafe fn out<'a>(p: *mut c_int) -> Option<&'a mut c_int> {
    if p.is_null() {
        None
    } else {
        // SAFETY: the caller guarantees a writable location.
        Some(unsafe { &mut *p })
    }
}

/// The body [`fn_complete`] and [`fn_complete2`] share.
///
/// `flags` is `None` for `fn_complete`, which derives it from whether an
/// attempted-completion hook was supplied, and `Some` for `fn_complete2`,
/// which is handed it.
///
/// # Safety
///
/// As the two entry points.
#[allow(clippy::too_many_arguments)]
unsafe fn complete(
    el: *mut EditLine,
    complete_func: Option<CompleteFuncC>,
    attempted_completion_function: Option<AttemptedCompletionFuncC>,
    word_break: *const u32,
    special_prefixes: *const u32,
    app_func: Option<AppFuncC>,
    query_items: usize,
    completion_type: *mut c_int,
    over: *mut c_int,
    point: *mut c_int,
    end: *mut c_int,
    flags: Option<c_uint>,
) -> c_int {
    // The C dereferences `el` at once, through `el_wline`. Defined here as
    // the caller error it is; every other argument has a documented NULL.
    if el.is_null() {
        return c_int::from(CC_ERROR);
    }

    let _hooks = HookGuard::install(app_func, attempted_completion_function);

    // `word_break` reaches `wcschr` unchecked in the C, so a NULL one faults;
    // the core takes a slice and an empty one is the same "no character
    // breaks a word" answer without the fault. `special_prefixes` is NULL-
    // checked in the C and stays an `Option`.
    // SAFETY: both are NULL or terminated wide strings, per the C's contract.
    let word_break = unsafe { wide_upto_nul(word_break) }.unwrap_or(&[]);
    // SAFETY: as above.
    let special = unsafe { wide_upto_nul(special_prefixes) };

    // `move` so the closure captures the function pointer by copy and borrows
    // nothing: the core's `CompleteFunc` is a `dyn` object with the default
    // `'static` bound, which a closure holding a reference to this frame could
    // not satisfy.
    let mut generator = move |text: &str, state: i32| -> Option<String> {
        let f = complete_func?;
        // SAFETY: the hook is the caller's own pointer, called with a
        // NUL-terminated copy exactly as the C calls it. The C's
        // `completion_matches` frees what comes back, so this does too.
        unsafe {
            let ctext = c_dup(text.as_bytes());
            if ctext.is_null() {
                return None;
            }
            let m = f(ctext, state);
            c_free_str(ctext);
            if m.is_null() {
                return None;
            }
            let owned = String::from_utf8_lossy(c_bytes(m)).into_owned();
            c_free_str(m);
            Some(owned)
        }
    };
    // A NULL `complete_func` is not "a generator that answers nothing": the C
    // substitutes `fn_filename_completion_function`, which the core does for
    // itself when handed `None`.
    let generator: Option<&mut filecomplete::CompleteFunc> = if complete_func.is_some() {
        Some(&mut generator)
    } else {
        None
    };
    let attempted = attempted_completion_function
        .map(|_| attempted_adapter as filecomplete::AttemptedCompletionFunc);
    let app = app_func.map(|_| app_func_adapter as filecomplete::AppFunc);

    // SAFETY: `el` is non-NULL, and the four out-parameters are NULL or
    // writable, which is the C's own contract for them.
    unsafe {
        let el = &mut *el;
        let (ct, ov, po, en) = (out(completion_type), out(over), out(point), out(end));
        match flags {
            Some(flags) => filecomplete::fn_complete2(
                el,
                generator,
                attempted,
                word_break,
                special,
                app,
                query_items,
                ct,
                ov,
                po,
                en,
                flags,
            ),
            None => filecomplete::fn_complete(
                el,
                generator,
                attempted,
                word_break,
                special,
                app,
                query_items,
                ct,
                ov,
                po,
                en,
            ),
        }
    }
}

// [spec:libedit:def:filecomplete.fn-complete2-fn]
// [spec:libedit:sem:filecomplete.fn-complete2-fn]
/// C: `int fn_complete2(EditLine *el, char *(*complete_func)(const char *,
/// int), char **(*attempted_completion_function)(const char *, int, int),
/// const wchar_t *word_break, const wchar_t *special_prefixes, const char
/// *(*app_func)(const char *), size_t query_items, int *completion_type, int
/// *over, int *point, int *end, unsigned int flags);`
///
/// Returns a `CC_*` code, not a readline status, because the function doubles
/// as an editor command's body. The only flag is `FN_QUOTE_MATCH` (1).
///
/// A NULL `complete_func`, `attempted_completion_function` or `app_func`
/// selects the built-in; a NULL `special_prefixes` means there are none; each
/// of the four `int *` may be NULL. A NULL `el` is the one the C faults on
/// and is `CC_ERROR` here.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn fn_complete2(
    el: *mut EditLine,
    complete_func: Option<CompleteFuncC>,
    attempted_completion_function: Option<AttemptedCompletionFuncC>,
    word_break: *const u32,
    special_prefixes: *const u32,
    app_func: Option<AppFuncC>,
    query_items: usize,
    completion_type: *mut c_int,
    over: *mut c_int,
    point: *mut c_int,
    end: *mut c_int,
    flags: c_uint,
) -> c_int {
    // SAFETY: as documented above.
    unsafe {
        complete(
            el,
            complete_func,
            attempted_completion_function,
            word_break,
            special_prefixes,
            app_func,
            query_items,
            completion_type,
            over,
            point,
            end,
            Some(flags),
        )
    }
}

// [spec:libedit:def:filecomplete.fn-complete-fn]
// [spec:libedit:sem:filecomplete.fn-complete-fn]
/// C: `int fn_complete(EditLine *, ...);` — [`fn_complete2`] with `flags`
/// derived rather than passed: `FN_QUOTE_MATCH` when the caller supplied no
/// `attempted_completion_function`, and 0 when it did, on the reading that an
/// application producing its own match has already quoted it.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn fn_complete(
    el: *mut EditLine,
    complete_func: Option<CompleteFuncC>,
    attempted_completion_function: Option<AttemptedCompletionFuncC>,
    word_break: *const u32,
    special_prefixes: *const u32,
    app_func: Option<AppFuncC>,
    query_items: usize,
    completion_type: *mut c_int,
    over: *mut c_int,
    point: *mut c_int,
    end: *mut c_int,
) -> c_int {
    // SAFETY: as [`fn_complete2`].
    unsafe {
        complete(
            el,
            complete_func,
            attempted_completion_function,
            word_break,
            special_prefixes,
            app_func,
            query_items,
            completion_type,
            over,
            point,
            end,
            None,
        )
    }
}

// [spec:libedit:def:filecomplete.fn-display-match-list-fn]
// [spec:libedit:sem:filecomplete.fn-display-match-list-fn]
/// C: `void fn_display_match_list(EditLine *el, char **matches, size_t num,
/// size_t width, const char *(*app_func)(const char *));`
///
/// `matches[0]` is the common prefix and is neither printed nor sorted;
/// `matches[1..num]` are. The array is **sorted in place**, so the caller's
/// pointers come back permuted — `fn_complete2` depends on that, because its
/// free loop still has to reach every string.
///
/// `num == 0` prints nothing. The C's `num--` underflows there and walks the
/// array; `sem:filecomplete.fn-display-match-list-fn` defines it as a caller
/// error instead (ERR-completion-02).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fn_display_match_list(
    el: *mut EditLine,
    matches: *mut *mut c_char,
    num: usize,
    width: usize,
    app_func: Option<AppFuncC>,
) {
    if el.is_null() || matches.is_null() || num == 0 {
        return;
    }
    let _hooks = HookGuard::install(app_func, None);
    let app = app_func.map(|_| app_func_adapter as filecomplete::AppFunc);

    // SAFETY: `matches` holds `num` pointers, each NULL or NUL-terminated,
    // which is what the C reads through the same indices.
    unsafe {
        // The core takes owned strings and sorts them, so the caller's array
        // is permuted afterwards to match — which is what the C's in-place
        // `qsort` leaves behind.
        let mut owned: Vec<String> = Vec::with_capacity(num);
        for i in 0..num {
            let p = *matches.add(i);
            owned.push(match c_bytes_opt(p) {
                Some(b) => String::from_utf8_lossy(b).into_owned(),
                None => String::new(),
            });
        }

        filecomplete::fn_display_match_list(&mut *el, &mut owned, num, width, app);

        permute_to_match(matches, &owned);
    }
}

/// Reorders `matches[1..]` so it holds the same pointers in the order the
/// core's sort put the strings, which is what the C's in-place `qsort` on the
/// caller's array leaves behind.
///
/// Element 0 is the common prefix and is excluded from the sort at both ends.
/// Duplicate names are matched off one at a time, so an array holding the same
/// string twice keeps both pointers.
///
/// Shared with [`crate::readline::rl_display_match_list`], which reaches the
/// same core function with the readline layer's own append-character hook.
///
/// # Safety
///
/// `matches` must hold `sorted.len()` writable pointers, each NULL or
/// NUL-terminated.
pub(crate) unsafe fn permute_to_match(matches: *mut *mut c_char, sorted: &[String]) {
    let n = sorted.len();
    let mut used = vec![false; n];
    for (i, want) in sorted.iter().enumerate().take(n).skip(1) {
        let want = want.as_bytes();
        for j in 1..n {
            // SAFETY: `j` is in range and the caller guarantees the array.
            let p = unsafe { *matches.add(j) };
            // SAFETY: as above.
            if !used[j] && unsafe { c_bytes_opt(p) } == Some(want) {
                used[j] = true;
                if j != i {
                    // SAFETY: both indices are in range.
                    unsafe {
                        let tmp = *matches.add(i);
                        *matches.add(i) = *matches.add(j);
                        *matches.add(j) = tmp;
                    }
                    used.swap(i, j);
                }
                break;
            }
        }
    }
}

// [spec:libedit:def:filecomplete.fn-tilde-expand-fn]
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn]
/// C: `char *fn_tilde_expand(const char *txt);`
///
/// A newly allocated copy the caller frees. NULL only on allocation failure:
/// a `txt` that does not begin with `~`, and a `~user` naming no account, are
/// both copied through unchanged.
///
/// A NULL `txt` reaches `txt[0]` in the C. Defined here as NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fn_tilde_expand(txt: *const c_char) -> *mut c_char {
    // SAFETY: `txt` is NULL or a NUL-terminated string.
    let Some(bytes) = (unsafe { c_bytes_opt(txt) }) else {
        return ptr::null_mut();
    };
    // The core takes a `&str`, so a path that is not valid UTF-8 cannot be
    // handed through unchanged; reported as a core-signature gap.
    let txt = String::from_utf8_lossy(bytes).into_owned();
    match filecomplete::fn_tilde_expand(&txt) {
        // SAFETY: the block is handed to the caller, who frees it.
        Some(s) => unsafe { c_dup(s.as_bytes()) },
        None => ptr::null_mut(),
    }
}

// [spec:libedit:def:filecomplete.fn-filename-completion-function-fn]
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]
/// C: `char *fn_filename_completion_function(const char *text, int state);`
///
/// The default match generator. `state == 0` restarts the scan; any other
/// value continues it and `text` is not read at all. The scan also restarts
/// whenever the directory stream is closed, so calling again after a NULL
/// return starts over rather than answering NULL again (ERR-completion-16) —
/// a caller that does not stop at the first NULL loops forever.
///
/// The result is a newly allocated string the caller frees.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fn_filename_completion_function(
    text: *const c_char,
    state: c_int,
) -> *mut c_char {
    // SAFETY: `text` is NULL or a NUL-terminated string.
    let bytes = unsafe { c_bytes_opt(text) }.unwrap_or(b"");
    let text = String::from_utf8_lossy(bytes).into_owned();
    FILENAME_SCAN.with_borrow_mut(|scan| {
        let scan = scan.get_or_insert_with(FilenameCompletionState::default);
        match filecomplete::fn_filename_completion_function(scan, &text, state) {
            // SAFETY: the block is handed to the caller, who frees it.
            Some(s) => unsafe { c_dup(s.as_bytes()) },
            None => ptr::null_mut(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{HookGuard, app_func_adapter, intern};
    use core::ffi::c_char;

    /// Interning is per distinct value, so a hook that returns a shared
    /// buffer does not leak once per call.
    #[test]
    fn interning_is_per_value() {
        let a = intern(b"/");
        let b = intern(b"/");
        assert_eq!(a, "/");
        assert!(std::ptr::eq(a, b));
    }

    unsafe extern "C" fn slash(_name: *const c_char) -> *const c_char {
        c"/".as_ptr()
    }

    unsafe extern "C" fn nothing(_name: *const c_char) -> *const c_char {
        core::ptr::null()
    }

    /// The adapter reads the hook parked by the guard, and the guard puts the
    /// previous one back — which is what lets a callback re-enter.
    #[test]
    fn the_guard_nests() {
        assert_eq!(app_func_adapter("x"), "", "no hook appends nothing");
        {
            let _outer = HookGuard::install(Some(slash), None);
            assert_eq!(app_func_adapter("x"), "/");
            {
                let _inner = HookGuard::install(Some(nothing), None);
                assert_eq!(app_func_adapter("x"), "", "a NULL return appends nothing");
            }
            assert_eq!(app_func_adapter("x"), "/", "the outer hook is restored");
        }
        assert_eq!(app_func_adapter("x"), "");
    }
}
