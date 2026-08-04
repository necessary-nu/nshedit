//! Ported from `src/filecomplete.c`; rules live in `docs/spec/port/src/filecomplete.md`.
//!
//! Signatures only for now — every body is `todo!()`.
//!
//! Two shapes here depart from the C deliberately, and both are forced by
//! `plan/decisions/idiomatic-core.md`: the core carries no globals, so the
//! match generator's file-statics become an explicit
//! [`FilenameCompletionState`] the caller owns, and the generator callback
//! type widens from a bare function pointer to `&mut dyn FnMut` so a
//! generator that needs state can carry it. The C's stateful face — one
//! process-wide scan, restarted by `state == 0` — is the ABI crate's to
//! present.

// Bodies are not written yet, so every parameter is unused. Remove this once
// the translations land.
#![allow(unused_variables)]

use crate::el::EditLine;

/// C: `char *(*)(const char *, int)` — a match generator.
///
/// The C spells this out at each use and has no typedef for it, so there is
/// no rule to carry here; it is named only because the parameter types are
/// otherwise repeated four times. `FnMut` rather than a bare `fn` pointer
/// because a generator's scan state is its own to carry — see
/// [`FilenameCompletionState`].
pub type CompleteFunc = dyn FnMut(&str, i32) -> Option<String>;

/// C: `char **(*)(const char *, int, int)` — the application's own
/// completion hook, tried before the generator. Stateless in the C, so a
/// plain function pointer. No typedef there, hence no rule here.
pub type AttemptedCompletionFunc = fn(&str, i32, i32) -> Option<Vec<String>>;

/// C: `const char *(*)(const char *)` — chooses the string appended after a
/// completed name. The return is a literal the caller must not free, hence
/// `&'static str`. No typedef in the C, hence no rule here.
pub type AppFunc = fn(&str) -> &'static str;

// [spec:libedit:def:filecomplete.fn-tilde-expand-fn]
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn]
/// C: `char * fn_tilde_expand(const char *txt)`.
///
/// `None` is the C's NULL, which here means only an allocation failure: a
/// `txt` that does not start with `~`, and an unknown user name, both come
/// back as a copy of `txt`.
pub fn fn_tilde_expand(txt: &str) -> Option<String> {
    todo!()
}

// [spec:libedit:def:filecomplete.needs-escaping-fn]
// [spec:libedit:sem:filecomplete.needs-escaping-fn]
/// C: `static int needs_escaping(wchar_t c)` — 1 or 0, kept an `int`.
fn needs_escaping(c: u32) -> i32 {
    todo!()
}

// [spec:libedit:def:filecomplete.needs-dquote-escaping-fn]
// [spec:libedit:sem:filecomplete.needs-dquote-escaping-fn]
/// C: `static int needs_dquote_escaping(char c)`. A byte of a narrow
/// filename, so `u8` and not `u32`.
fn needs_dquote_escaping(c: u8) -> i32 {
    todo!()
}

// [spec:libedit:def:filecomplete.unescape-string-fn]
// [spec:libedit:sem:filecomplete.unescape-string-fn]
/// C: `static wchar_t * unescape_string(const wchar_t *string, size_t
/// length)`. The C's `(pointer, length)` pair is the slice; there is no
/// separate `length`. `None` is the C's allocation failure.
fn unescape_string(string: &[u32]) -> Option<Vec<u32>> {
    todo!()
}

// [spec:libedit:def:filecomplete.escape-filename-fn]
// [spec:libedit:sem:filecomplete.escape-filename-fn]
/// C: `static char * escape_filename(EditLine *el, const char *filename, int
/// single_match, const char *(*app_func)(const char *))`.
///
/// The C's `filename == NULL` guard is unrepresentable — a `&str` is never
/// null — so only the allocation-failure `None` survives.
fn escape_filename(
    el: &mut EditLine,
    filename: &str,
    single_match: i32,
    app_func: Option<AppFunc>,
) -> Option<String> {
    todo!()
}

/// The match generator's scan state.
///
/// The C keeps all five of these in function-level `static`s inside
/// `fn_filename_completion_function`, one set per process; see the hazards
/// in `sem:filecomplete.fn-filename-completion-function-fn`. The core has no
/// globals, so the caller owns the state and hands it in, which is also what
/// makes two interleaved scans possible.
///
/// The real iteration cursor is the open directory stream, not the `state`
/// argument: a scan restarts when `state == 0` **or** when `dir` is `None`.
#[derive(Default)]
pub struct FilenameCompletionState {
    /// C: `static DIR *dir` — the open directory stream, positioned just
    /// past the last entry returned. `None` between scans.
    dir: Option<std::fs::ReadDir>,
    /// C: `static char *filename` — the trailing component of `text` that
    /// entries must be prefixed by. `None` when `text` was empty.
    filename: Option<String>,
    /// C: `static char *dirname` — the directory prefix exactly as the user
    /// typed it, including its trailing slash and any unexpanded `~`. It is
    /// this, not `dirpath`, that is prepended to a match.
    dirname: Option<String>,
    /// C: `static char *dirpath` — the path actually handed to `opendir`,
    /// after tilde expansion.
    dirpath: Option<String>,
    /// C: `static size_t filename_len` — byte length of `filename`.
    filename_len: usize,
}

// [spec:libedit:def:filecomplete.fn-filename-completion-function-fn]
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]
/// C: `char * fn_filename_completion_function(const char *text, int state)`.
///
/// The default match generator. `scan` is the C's file-statics made
/// explicit; `state` keeps its C meaning — a restart flag, not a sequence
/// number — and `text` is ignored entirely on a continuation.
pub fn fn_filename_completion_function(
    scan: &mut FilenameCompletionState,
    text: &str,
    state: i32,
) -> Option<String> {
    todo!()
}

// [spec:libedit:def:filecomplete.append-char-function-fn]
// [spec:libedit:sem:filecomplete.append-char-function-fn]
/// C: `static const char * append_char_function(const char *name)`.
///
/// The default `app_func`. The return is one of two string literals the
/// caller must not free, hence `&'static str`.
fn append_char_function(name: &str) -> &'static str {
    todo!()
}

// [spec:libedit:def:filecomplete.completion-matches-fn]
// [spec:libedit:sem:filecomplete.completion-matches-fn]
/// C: `char ** completion_matches(const char *text, char *(*genfunc)(const
/// char *, int))`.
///
/// The returned vector is the C's array with its NULL terminator dropped:
/// element 0 is the longest common prefix, elements 1.. are the matches in
/// generator order. `None` is the C's NULL — no matches, or an allocation
/// failure.
///
/// `genfunc` is `&mut dyn FnMut` rather than a function pointer because a
/// generator's scan state is now its own to carry; see
/// [`FilenameCompletionState`].
pub fn completion_matches(text: &str, genfunc: &mut CompleteFunc) -> Option<Vec<String>> {
    todo!()
}

// [spec:libedit:def:filecomplete.fn-qsort-string-compare-fn]
// [spec:libedit:sem:filecomplete.fn-qsort-string-compare-fn]
/// C: `static int _fn_qsort_string_compare(const void *i1, const void *i2)`.
///
/// The `qsort` callback of [`fn_display_match_list`], which loads a
/// `char *` out of each array element and returns `strcasecmp`. Typed here
/// as the elements themselves; the `strcasecmp` sign convention is kept.
fn _fn_qsort_string_compare(i1: &str, i2: &str) -> i32 {
    todo!()
}

// [spec:libedit:def:filecomplete.fn-display-match-list-fn]
// [spec:libedit:sem:filecomplete.fn-display-match-list-fn]
/// C: `void fn_display_match_list(EditLine *el, char **matches, size_t num,
/// size_t width, const char *(*app_func)(const char *))`.
///
/// `matches` is `&mut` because the C sorts it in place. `num` stays a
/// parameter: it counts `matches[0]`, which is not one of the strings
/// printed, so it is not the slice length.
pub fn fn_display_match_list(
    el: &mut EditLine,
    matches: &mut [String],
    num: usize,
    width: usize,
    app_func: Option<AppFunc>,
) {
    todo!()
}

// [spec:libedit:def:filecomplete.find-word-to-complete-fn]
// [spec:libedit:sem:filecomplete.find-word-to-complete-fn]
/// C: `static wchar_t * find_word_to_complete(const wchar_t *cursor, const
/// wchar_t *buffer, const wchar_t *word_break, const wchar_t
/// *special_prefixes, size_t *length, int do_unescape)`.
///
/// `cursor` is an offset into `buffer`, per the pointer-into-a-buffer
/// convention in the crate docs. `word_break` is never NULL in the C —
/// `wcschr` is called on it unguarded — while `special_prefixes` is
/// NULL-checked.
fn find_word_to_complete(
    cursor: usize,
    buffer: &[u32],
    word_break: &[u32],
    special_prefixes: Option<&[u32]>,
    length: &mut usize,
    do_unescape: i32,
) -> Option<Vec<u32>> {
    todo!()
}

// [spec:libedit:def:filecomplete.fn-complete2-fn]
// [spec:libedit:sem:filecomplete.fn-complete2-fn]
/// C: `int fn_complete2(EditLine *el, char *(*complete_func)(const char *,
/// int), char **(*attempted_completion_function)(const char *, int, int),
/// const wchar_t *word_break, const wchar_t *special_prefixes, const char
/// *(*app_func)(const char *), size_t query_items, int *completion_type, int
/// *over, int *point, int *end, unsigned int flags)`.
///
/// Every NULL-checked parameter is an `Option`; the return is the C's
/// `CC_*` code, so it stays an `i32`.
#[allow(clippy::too_many_arguments)]
pub fn fn_complete2(
    el: &mut EditLine,
    complete_func: Option<&mut CompleteFunc>,
    attempted_completion_function: Option<AttemptedCompletionFunc>,
    word_break: &[u32],
    special_prefixes: Option<&[u32]>,
    app_func: Option<AppFunc>,
    query_items: usize,
    completion_type: Option<&mut i32>,
    over: Option<&mut i32>,
    point: Option<&mut i32>,
    end: Option<&mut i32>,
    flags: u32,
) -> i32 {
    todo!()
}

// [spec:libedit:def:filecomplete.fn-complete-fn]
// [spec:libedit:sem:filecomplete.fn-complete-fn]
/// C: `int fn_complete(EditLine *el, ...)` — [`fn_complete2`] with `flags`
/// derived from whether an `attempted_completion_function` was supplied.
#[allow(clippy::too_many_arguments)]
pub fn fn_complete(
    el: &mut EditLine,
    complete_func: Option<&mut CompleteFunc>,
    attempted_completion_function: Option<AttemptedCompletionFunc>,
    word_break: &[u32],
    special_prefixes: Option<&[u32]>,
    app_func: Option<AppFunc>,
    query_items: usize,
    completion_type: Option<&mut i32>,
    over: Option<&mut i32>,
    point: Option<&mut i32>,
    end: Option<&mut i32>,
) -> i32 {
    todo!()
}

// [spec:libedit:def:filecomplete.el-fn-complete-fn]
// [spec:libedit:sem:filecomplete.el-fn-complete-fn]
/// C: `unsigned char _el_fn_complete(EditLine *el, int ch)` — the editor
/// command wrapper, bound as a key action. `ch` is unused, as in the C.
pub fn _el_fn_complete(el: &mut EditLine, ch: i32) -> u8 {
    todo!()
}

// [spec:libedit:def:filecomplete.el-fn-sh-complete-fn]
// [spec:libedit:sem:filecomplete.el-fn-sh-complete-fn]
/// C: `unsigned char _el_fn_sh_complete(EditLine *el, int ch)`.
pub fn _el_fn_sh_complete(el: &mut EditLine, ch: i32) -> u8 {
    todo!()
}
