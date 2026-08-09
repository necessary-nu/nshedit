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
//! Each exported wrapper copies its C callbacks into scoped Rust closures.
//! Completion snapshots the editor before invoking them and applies their
//! owned response afterwards, so re-entry never overlaps an editor borrow.

use core::ffi::{c_char, c_int, c_uint};
use core::ptr;
use std::cell::RefCell;
use std::fs::ReadDir;
use std::os::unix::ffi::OsStrExt;

use nshedit::domain::{Text, TextUnit};
use nshedit::editor::{CompletionCandidate, CompletionCandidates, CompletionQuery};

use crate::adapter::EditLine;
#[cfg(test)]
use crate::adapter::SessionInit;
use crate::cdecl::histedit::{CC_ERROR, CC_NORM, CC_REDISPLAY, CC_REFRESH};
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

pub(crate) const FN_QUOTE_MATCH: c_uint = 1;

const BREAK_CHARACTERS: &[u32] = &[
    b' ' as u32,
    b'\t' as u32,
    b'\n' as u32,
    b'"' as u32,
    b'\\' as u32,
    b'\'' as u32,
    b'`' as u32,
    b'@' as u32,
    b'$' as u32,
    b'>' as u32,
    b'<' as u32,
    b'=' as u32,
    b';' as u32,
    b'|' as u32,
    b'&' as u32,
    b'{' as u32,
    b'(' as u32,
];

#[derive(Default)]
pub(crate) struct FilenameCompletionState {
    directory: Option<ReadDir>,
    filename: String,
    dirname: String,
}

thread_local! {
    /// The file-statics inside `fn_filename_completion_function`, which the
    /// core made an explicit state object.
    ///
    /// One per process, as in the C: the generator's scan cursor is a
    /// function-level `static DIR *` there, so two interleaved scans corrupt
    /// each other. See `sem:filecomplete.fn-filename-completion-function-fn`.
    static FILENAME_SCAN: RefCell<Option<FilenameCompletionState>> = const { RefCell::new(None) };
}

fn text_bytes(text: &Text) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in text.as_units() {
        match unit {
            TextUnit::Scalar(character) => {
                let mut encoded = [0; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            TextUnit::RawByte(byte) => bytes.push(*byte),
            TextUnit::OpaqueCodePoint(_) => bytes.extend_from_slice("\u{fffd}".as_bytes()),
        }
    }
    bytes
}

fn tilde_expand_string(text: &str) -> Option<String> {
    if !text.starts_with('~') {
        return Some(text.to_owned());
    }
    let (name, rest) = match text[1..].find('/') {
        Some(index) => (&text[1..index + 1], &text[index + 2..]),
        None => (&text[1..], text),
    };
    let home = if name.is_empty() {
        nshedit_plat::passwd::home_directory(nshedit_plat::current_user())
    } else {
        nshedit_plat::passwd::home_directory_named(name)
    }
    .ok()
    .flatten()?;
    let home = home.into_os_string().into_string().ok()?;
    Some(format!("{home}/{rest}"))
}

fn completion_suffix(name: &str) -> String {
    let path = if name.starts_with('~') {
        tilde_expand_string(name).unwrap_or_else(|| name.to_owned())
    } else {
        name.to_owned()
    };
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
        "/".to_owned()
    } else {
        " ".to_owned()
    }
}

pub(crate) fn filename_completion(
    scan: &mut FilenameCompletionState,
    text: &str,
    state: usize,
) -> Option<String> {
    if state == 0 || scan.directory.is_none() {
        let (dirname, filename) = text.rsplit_once('/').map_or_else(
            || (String::new(), text.to_owned()),
            |(directory, filename)| (format!("{directory}/"), filename.to_owned()),
        );
        let path = if dirname.is_empty() {
            "./".to_owned()
        } else if dirname.starts_with('~') {
            tilde_expand_string(&dirname)?
        } else {
            dirname.clone()
        };
        scan.directory = std::fs::read_dir(path).ok();
        scan.dirname = dirname;
        scan.filename = filename;
    }

    let directory = scan.directory.as_mut()?;
    for entry in directory.by_ref() {
        let Ok(entry) = entry else {
            break;
        };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name != "." && name != ".." && name.starts_with(&scan.filename) {
            return Some(format!("{}{}", scan.dirname, name));
        }
    }
    scan.directory = None;
    None
}

pub(crate) fn collect_candidates(
    text: &str,
    generator: &mut CandidateGenerator<'_>,
) -> Vec<String> {
    let mut matches = Vec::new();
    let mut state = 0;
    while let Some(candidate) = generator(text, state) {
        matches.push(candidate);
        state = state.saturating_add(1);
    }
    matches
}

pub(crate) fn compatibility_matches(mut candidates: Vec<String>) -> Option<Vec<String>> {
    let first = candidates.first()?;
    let mut prefix_length = first.len();
    for candidate in candidates.iter().skip(1) {
        prefix_length = first
            .as_bytes()
            .iter()
            .zip(candidate.as_bytes())
            .take(prefix_length)
            .take_while(|(left, right)| left == right)
            .count();
    }
    while !first.is_char_boundary(prefix_length) {
        prefix_length -= 1;
    }
    let mut result = Vec::with_capacity(candidates.len() + 1);
    result.push(first[..prefix_length].to_owned());
    result.append(&mut candidates);
    Some(result)
}

/// Resolve the native driver's typed completion request with the ABI's
/// default filename provider.
pub(crate) fn builtin_candidates(query: &CompletionQuery) -> CompletionCandidates {
    let stem = String::from_utf8_lossy(&text_bytes(query.stem())).into_owned();
    let mut scan = FilenameCompletionState::default();
    let mut state = 0;
    let mut candidates = Vec::new();
    while let Some(candidate) = filename_completion(&mut scan, &stem, state) {
        let suffix = completion_suffix(&candidate);
        candidates.push(CompletionCandidate::new(candidate).with_suffix(suffix));
        state = state.saturating_add(1);
    }
    candidates.into()
}

pub(crate) fn format_match_list(
    matches: &mut [String],
    width: usize,
    columns: usize,
    suffix: &mut SuffixProvider<'_>,
) -> Vec<u8> {
    if matches.is_empty() {
        return Vec::new();
    }
    matches.sort_by_key(|entry| entry.to_ascii_lowercase());
    let per_line = (columns / width.saturating_add(2)).max(1);
    let lines = matches.len().div_ceil(per_line);
    let mut output = Vec::new();
    for line in 0..lines {
        for column in 0..per_line {
            let index = line + column * lines;
            if index >= matches.len() {
                break;
            }
            let entry = &matches[index];
            if column != 0 {
                output.push(b' ');
            }
            output.extend_from_slice(entry.as_bytes());
            output.extend_from_slice(suffix(entry).as_bytes());
            output.resize(output.len() + width.saturating_sub(entry.len()), b' ');
        }
        output.push(b'\n');
    }
    output
}

mod completion;

pub(crate) use crate::adapter::CompletionInvocation;
pub(crate) use completion::{
    AttemptedCompletion, AttemptedFallback, AttemptedProvider, AttemptedState, CandidateGenerator,
    CompletionCommand, CompletionPolicy, CompletionProviders, CompletionRequest, SuffixProvider,
    UniqueSuffix, complete_filename, observe_completion, resolve_completion,
};
#[cfg(test)]
use completion::{CompletionListing, CompletionPositions};

/// Copy a C `const wchar_t *` into the native text model.
///
/// # Safety
///
/// `p` must be NULL or point at a `L'\0'`-terminated wide string.
unsafe fn copy_wide_text(p: *const u32) -> Text {
    if p.is_null() {
        return Text::default();
    }
    let mut n = 0usize;
    // SAFETY: the caller guarantees a terminated string.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: as above. The copy ends the foreign pointer's participation in
    // completion before any callback can re-enter.
    unsafe { core::slice::from_raw_parts(p, n) }
        .iter()
        .copied()
        .map(TextUnit::from_code_point)
        .collect()
}

unsafe fn call_generator(hook: CompleteFuncC, text: &str, state: usize) -> Option<String> {
    // SAFETY: the hook receives a NUL-terminated owned copy exactly as in C.
    unsafe {
        let ctext = c_dup(text.as_bytes());
        if ctext.is_null() {
            return None;
        }
        let state = c_int::try_from(state).unwrap_or(c_int::MAX);
        let candidate = hook(ctext, state);
        c_free_str(ctext);
        if candidate.is_null() {
            return None;
        }
        let owned = String::from_utf8_lossy(c_bytes(candidate)).into_owned();
        c_free_str(candidate);
        Some(owned)
    }
}

unsafe fn call_attempted(
    hook: AttemptedCompletionFuncC,
    text: &str,
    start: usize,
    end: usize,
) -> Option<Vec<String>> {
    // SAFETY: the hook receives the same owned text and saturated positions as
    // the C boundary. Its returned array and strings transfer ownership here.
    unsafe {
        let ctext = c_dup(text.as_bytes());
        if ctext.is_null() {
            return None;
        }
        let start = c_int::try_from(start).unwrap_or(c_int::MAX);
        let end = c_int::try_from(end).unwrap_or(c_int::MAX);
        let matches = hook(ctext, start, end);
        c_free_str(ctext);
        if matches.is_null() {
            return None;
        }
        let mut owned = Vec::new();
        let mut index = 0;
        loop {
            let candidate = *matches.add(index);
            if candidate.is_null() {
                break;
            }
            owned.push(String::from_utf8_lossy(c_bytes(candidate)).into_owned());
            c_free_str(candidate);
            index += 1;
        }
        c_free_array(matches, index + 1);
        if owned.len() > 1 {
            owned.remove(0);
        }
        Some(owned)
    }
}

unsafe fn call_suffix(hook: AppFuncC, candidate: &str) -> String {
    // SAFETY: this creates the NUL-terminated argument owned below.
    let argument = unsafe { c_dup(candidate.as_bytes()) };
    if argument.is_null() {
        return String::new();
    }
    // SAFETY: the hook borrows the NUL-terminated argument for this call. Its
    // return remains hook-owned and is copied before re-entry can overwrite it.
    let suffix = unsafe { hook(argument) };
    // SAFETY: ownership of the argument copy remains here.
    unsafe { c_free_str(argument) };
    // SAFETY: a non-NULL hook result is NUL-terminated by the C contract.
    unsafe { c_bytes_opt(suffix) }
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
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
#[doc = include_str!("ffi_safety.md")]
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
    if el.is_null() {
        return c_int::from(CC_ERROR);
    }

    // SAFETY: the two pointers are NULL or terminated wide strings. Both are
    // copied before a callback can re-enter this wrapper.
    let mut separators = unsafe { copy_wide_text(word_break) };
    // SAFETY: as above.
    let special = unsafe { copy_wide_text(special_prefixes) };
    separators.extend(special.as_units().iter().copied());

    // SAFETY: `el` is non-NULL. This borrow ends with the snapshot, before any
    // foreign function can run.
    let snapshot = unsafe { observe_completion(&mut *el, separators) };
    let invocation = snapshot.invocation();
    let positions = snapshot.positions();
    // SAFETY: each non-NULL out pointer is writable by the C contract. These
    // are individual stores, never Rust references held across callbacks.
    unsafe {
        if !completion_type.is_null() {
            *completion_type = match invocation {
                CompletionInvocation::Insert => b'\t'.into(),
                CompletionInvocation::List => b'?'.into(),
            };
        }
        if !point.is_null() {
            *point = c_int::try_from(positions.cursor).unwrap_or(c_int::MAX);
        }
        if !end.is_null() {
            *end = c_int::try_from(positions.line_end).unwrap_or(c_int::MAX);
        }
    }

    let mut generator_adapter = |text: &str, state: usize| {
        complete_func.and_then(|hook| {
            // SAFETY: the exported callback contract is documented above.
            unsafe { call_generator(hook, text, state) }
        })
    };
    let generator = complete_func.map(|_| &mut generator_adapter as &mut CandidateGenerator<'_>);

    let mut attempted_adapter = |text: &str, start: usize, finish: usize| {
        let candidates = attempted_completion_function.and_then(|hook| {
            // SAFETY: the exported callback contract is documented above.
            unsafe { call_attempted(hook, text, start, finish) }
        });
        let fallback = if over.is_null() {
            AttemptedFallback::Suppress
        } else {
            // SAFETY: the caller keeps this output slot writable throughout
            // the call, and no Rust reference to it exists.
            if unsafe { *over } == 0 {
                AttemptedFallback::Allow
            } else {
                AttemptedFallback::Suppress
            }
        };
        AttemptedCompletion::new(candidates, fallback)
    };
    let attempted =
        attempted_completion_function.map(|_| &mut attempted_adapter as &mut AttemptedProvider<'_>);

    let mut suffix_adapter = |candidate: &str| {
        app_func.map_or_else(String::new, |hook| {
            // SAFETY: the exported callback contract is documented above.
            unsafe { call_suffix(hook, candidate) }
        })
    };
    let suffix = app_func.map(|_| &mut suffix_adapter as &mut SuffixProvider<'_>);

    let target = el;
    let mut apply = move |query: &CompletionQuery, candidates: CompletionCandidates| {
        // SAFETY: the typed engine calls this only after all candidate-provider
        // callbacks have returned. The borrow lasts only for the core apply.
        unsafe {
            (&mut *target)
                .editor_mut()
                .apply_completion(query, candidates)
        }
    };
    let providers = CompletionProviders::new(generator)
        .with_attempted(attempted)
        .with_suffix(suffix);
    let unique_suffix = if flags & FN_QUOTE_MATCH != 0 {
        UniqueSuffix::Append
    } else {
        UniqueSuffix::Omit
    };
    let report = resolve_completion(CompletionRequest::new(
        snapshot,
        providers,
        CompletionPolicy::new(query_items, unique_suffix),
        &mut apply,
    ));

    // SAFETY: as for the earlier stores; provider callbacks are now finished.
    unsafe {
        let invocation = report.invocation();
        let positions = report.positions();
        if !completion_type.is_null() {
            *completion_type = match invocation {
                CompletionInvocation::Insert => b'\t'.into(),
                CompletionInvocation::List => b'?'.into(),
            };
        }
        if !point.is_null() {
            *point = c_int::try_from(positions.cursor).unwrap_or(c_int::MAX);
        }
        if !end.is_null() {
            *end = c_int::try_from(positions.line_end).unwrap_or(c_int::MAX);
        }
        if report.attempted_state() == AttemptedState::Reset && !over.is_null() {
            *over = 0;
        }
        report.apply_effects(&mut *el);
    }
    match report.command() {
        CompletionCommand::Normal => c_int::from(CC_NORM),
        CompletionCommand::Refresh => c_int::from(CC_REFRESH),
        CompletionCommand::Redisplay => c_int::from(CC_REDISPLAY),
        CompletionCommand::Error => c_int::from(CC_ERROR),
    }
}

// [spec:libedit:def:filecomplete.fn-complete-fn]
// [spec:libedit:sem:filecomplete.fn-complete-fn]
/// C: `int fn_complete(EditLine *, ...);` — [`fn_complete2`] with `flags`
/// derived rather than passed: `FN_QUOTE_MATCH` when the caller supplied no
/// `attempted_completion_function`, and 0 when it did, on the reading that an
/// application producing its own match has already quoted it.
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
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
    let flags = if attempted_completion_function.is_some() {
        0
    } else {
        FN_QUOTE_MATCH
    };
    // SAFETY: this wrapper has the same argument contracts as `fn_complete2`.
    unsafe {
        fn_complete2(
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
            flags,
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
#[doc = include_str!("ffi_safety.md")]
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

        let columns = (&*el).screen_size().map_or(80, |size| size.columns());
        let mut suffix = |candidate: &str| {
            app_func.map_or_else(
                || completion_suffix(candidate),
                |hook| call_suffix(hook, candidate),
            )
        };
        let output = format_match_list(&mut owned[1..], width, columns, &mut suffix);
        let _ = (&*el).write_output(&output);

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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn fn_tilde_expand(txt: *const c_char) -> *mut c_char {
    // SAFETY: `txt` is NULL or a NUL-terminated string.
    let Some(bytes) = (unsafe { c_bytes_opt(txt) }) else {
        return ptr::null_mut();
    };
    if bytes.first() != Some(&b'~') {
        // SAFETY: the block is handed to the caller, who frees it. Paths are
        // bytes on the supported POSIX ABI, so no UTF-8 conversion belongs on
        // this pass-through path.
        return unsafe { c_dup(bytes) };
    }

    // Locate the account name and preserve the C's `len == 0` defect when no
    // slash is present: after a successful lookup, `~`/`~user` is appended in
    // full after the home directory.
    let (name, rest_at) = match bytes[1..].iter().position(|&byte| byte == b'/') {
        None => (&bytes[1..], 0),
        Some(relative) => (&bytes[1..relative + 1], relative + 2),
    };
    let home = if name.is_empty() {
        nshedit_plat::passwd::home_directory(nshedit_plat::current_user())
            .ok()
            .flatten()
    } else {
        core::str::from_utf8(name).ok().and_then(|name| {
            nshedit_plat::passwd::home_directory_named(name)
                .ok()
                .flatten()
        })
    };
    let Some(home) = home else {
        // Unknown (including non-UTF-8) account names are copied unchanged.
        return unsafe { c_dup(bytes) };
    };

    let home = home.as_os_str().as_bytes();
    let rest = &bytes[rest_at..];
    let mut expanded = Vec::new();
    if expanded
        .try_reserve_exact(home.len() + 1 + rest.len())
        .is_err()
    {
        return ptr::null_mut();
    }
    expanded.extend_from_slice(home);
    expanded.push(b'/');
    expanded.extend_from_slice(rest);
    // SAFETY: the block is handed to the caller, who frees it.
    unsafe { c_dup(&expanded) }
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn fn_filename_completion_function(
    text: *const c_char,
    state: c_int,
) -> *mut c_char {
    // SAFETY: `text` is NULL or a NUL-terminated string.
    let bytes = unsafe { c_bytes_opt(text) }.unwrap_or(b"");
    let text = String::from_utf8_lossy(bytes).into_owned();
    let state = if state == 0 {
        0
    } else {
        usize::try_from(state).unwrap_or(1)
    };
    FILENAME_SCAN.with_borrow_mut(|scan| {
        let scan = scan.get_or_insert_with(FilenameCompletionState::default);
        match filename_completion(scan, &text, state) {
            // SAFETY: the block is handed to the caller, who frees it.
            Some(s) => unsafe { c_dup(s.as_bytes()) },
            None => ptr::null_mut(),
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use core::ffi::CStr;
    use nshedit::editor::CompletionOutcome;

    fn editor() -> EditLine {
        *EditLine::new(SessionInit::inert("completion-test"))
            .expect("construct an editor over inert descriptors")
    }

    fn text(value: &str) -> Text {
        value.chars().map(TextUnit::Scalar).collect()
    }

    // [spec:nshedit:req:abi.typed-completion/test]
    #[test]
    fn reentry_yields_a_stale_report() {
        let editor = Rc::new(RefCell::new(editor()));
        assert!(editor.borrow_mut().replace_line(text("fo")));
        let snapshot = observe_completion(&mut editor.borrow_mut(), Text::default());

        let provider_editor = Rc::clone(&editor);
        let mut generator = move |_stem: &str, state: usize| {
            if state != 0 {
                return None;
            }
            assert!(provider_editor.borrow_mut().replace_line(text("changed")));
            Some("food".to_owned())
        };
        let apply_editor = Rc::clone(&editor);
        let mut apply = move |query: &CompletionQuery, candidates: CompletionCandidates| {
            apply_editor
                .borrow_mut()
                .editor_mut()
                .apply_completion(query, candidates)
        };
        let report = resolve_completion(CompletionRequest::new(
            snapshot,
            CompletionProviders::new(Some(&mut generator)),
            CompletionPolicy::new(100, UniqueSuffix::Omit),
            &mut apply,
        ));

        assert_eq!(report.command(), CompletionCommand::Error);
        assert_eq!(report.listing(), CompletionListing::Pending);
        assert!(report.outcome().is_none());
        assert_eq!(editor.borrow().editor().line(), &text("changed"));
    }

    #[test]
    fn report_owns_suffix_and_positions() {
        let editor = Rc::new(RefCell::new(editor()));
        assert!(editor.borrow_mut().replace_line(text("fo")));
        let snapshot = observe_completion(&mut editor.borrow_mut(), Text::default());

        let mut generator = |_stem: &str, state: usize| (state == 0).then(|| "folder".to_owned());
        let observed = Rc::new(RefCell::new(Vec::new()));
        let suffix_observed = Rc::clone(&observed);
        let mut suffix = move |candidate: &str| {
            suffix_observed.borrow_mut().push(candidate.to_owned());
            "/".to_owned()
        };
        let apply_editor = Rc::clone(&editor);
        let mut apply = move |query: &CompletionQuery, candidates: CompletionCandidates| {
            apply_editor
                .borrow_mut()
                .editor_mut()
                .apply_completion(query, candidates)
        };
        let report = resolve_completion(CompletionRequest::new(
            snapshot,
            CompletionProviders::new(Some(&mut generator)).with_suffix(Some(&mut suffix)),
            CompletionPolicy::new(100, UniqueSuffix::Append),
            &mut apply,
        ));

        assert_eq!(report.command(), CompletionCommand::Refresh);
        assert_eq!(
            report.positions(),
            CompletionPositions {
                cursor: 2,
                line_end: 2
            }
        );
        assert_eq!(report.listing(), CompletionListing::Cleared);
        let Some(CompletionOutcome::Unique { candidate, .. }) = report.outcome() else {
            panic!("one candidate must produce a unique report");
        };
        assert_eq!(candidate.suffix(), Some(&text("/")));
        assert_eq!(&*observed.borrow(), &["folder"]);
        assert_eq!(editor.borrow().editor().line(), &text("folder/"));
    }

    /// A POSIX path is bytes, so the ABI's copy-through route must not insert
    /// UTF-8 replacement characters before handing ownership back to C.
    // [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test]
    #[test]
    fn tilde_preserves_non_utf8_bytes() {
        let input = [b'x', 0xff, b'y', 0];
        // SAFETY: `input` is NUL-terminated; the returned block is freed below.
        let expanded = unsafe { fn_tilde_expand(input.as_ptr().cast()) };
        assert!(!expanded.is_null());
        // SAFETY: the function returns a NUL-terminated allocation.
        assert_eq!(unsafe { CStr::from_ptr(expanded) }.to_bytes(), &input[..3]);
        // SAFETY: ownership of the allocation is ours.
        unsafe { c_free_str(expanded) };
    }
}
