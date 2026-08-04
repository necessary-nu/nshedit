//! The readline compatibility surface; rules in `docs/spec/port/src/readline.md`
//! and `docs/spec/port/src/editline/readline.md`.
//!
//! `readline.c` is a compatibility layer in the C itself, so this is its
//! faithful placement rather than an early idiomatization — see
//! `plan/decisions/idiomatic-core.md`. Everything the GNU readline API
//! exposes and Rust would never choose lives here: the exported mutable
//! statics, the process-global editor and history pair, and the returned
//! pointers that stay valid exactly until the next call.
//!
//! Functions that are `static` in `readline.c` stay private plain Rust
//! functions with no `#[unsafe(no_mangle)]` and no `extern "C"`: they are
//! not part of the ABI, only of the implementation.

use core::ffi::{c_char, c_int, c_uchar, c_void};

use nshedit::editline::readline::{HistEntry, HistdataT, HistoryState, Keymap};
use nshedit::el::{CFile, EditLine};

/// C: `rl_command_func_t *` — the application's keystroke handler.
///
/// Spelled out here rather than taken from
/// `nshedit::editline::readline::RlCommandFuncT`, which is a *Rust*-ABI `fn`
/// and so cannot cross an `extern "C"` boundary. Collapse the two when the
/// header types move into this crate.
type RlCommandFunc = unsafe extern "C" fn(c_int, c_int) -> c_int;

/// C: `rl_compentry_func_t *` — a completion generator, called repeatedly
/// with an increasing `state` until it returns NULL. See [`RlCommandFunc`]
/// for why this is not the core's `RlCompentryFuncT`.
type RlCompentryFunc = unsafe extern "C" fn(*const c_char, c_int) -> *mut c_char;

/// C: `rl_vcpfunc_t *` — the callback-interface line handler, which takes
/// ownership of the line. See [`RlCommandFunc`] for why this is not the
/// core's `RlVcpfuncT`.
type RlVcpfunc = unsafe extern "C" fn(*mut c_char);

// ---------------------------------------------------------------------------
// Exported mutable statics declared by `editline/readline.h`
//
// These are data symbols, not functions: the application writes them and
// libedit reads them. `plan/decisions/idiomatic-core.md` keeps them here and
// out of the core.
// ---------------------------------------------------------------------------

/// C: `extern char *(*rl_completion_word_break_hook)(void);`
///
/// A process-global, application-writable hook that widens the set of
/// characters `rl_complete` stops at when it decides which word the cursor
/// is in. `NULL` in the C, `None` here. Only `rl_complete` reads it.
// [spec:libedit:def:readline.rl-completion-word-break-hook-fn]
// [spec:libedit:sem:readline.rl-completion-word-break-hook-fn]
#[unsafe(no_mangle)]
pub static mut rl_completion_word_break_hook: Option<unsafe extern "C" fn() -> *mut c_char> = None;

/// C: `extern int (*rl_getc_function)(FILE *);`
///
/// A process-global, application-writable character reader. It sits under
/// the header's "not implemented" banner, but that comment is wrong for this
/// entry: `rl_initialize` samples it once for non-NULL-ness and, if set,
/// installs `_getc_function` as the editor's `EL_GETCFN`. Setting it after
/// initialisation has no effect.
// [spec:libedit:def:readline.rl-getc-function-fn]
// [spec:libedit:sem:readline.rl-getc-function-fn]
#[unsafe(no_mangle)]
pub static mut rl_getc_function: Option<unsafe extern "C" fn(CFile) -> c_int> = None;

// ---------------------------------------------------------------------------
// `readline.c`, in source order.
// ---------------------------------------------------------------------------

/// C: `static char *_get_prompt(EditLine *el);` — the `EL_PROMPT_ESC`
/// callback, handing libedit the application's `rl_prompt`.
// [spec:libedit:def:readline.get-prompt-fn]
// [spec:libedit:sem:readline.get-prompt-fn]
fn _get_prompt(el: *mut EditLine) -> *mut c_char {
    todo!()
}

/// C: `static int _getc_function(EditLine *el, wchar_t *c);` — the
/// `EL_GETCFN` shim that forwards to `rl_getc_function`.
// [spec:libedit:def:readline.getc-function-fn]
// [spec:libedit:sem:readline.getc-function-fn]
fn _getc_function(el: *mut EditLine, c: *mut u32) -> c_int {
    todo!()
}

/// C: `static void _resize_fun(EditLine *el, void *a);` — the `EL_RESIZE`
/// callback that republishes the line into `rl_line_buffer`.
// [spec:libedit:def:readline.resize-fun-fn]
// [spec:libedit:sem:readline.resize-fun-fn]
fn _resize_fun(el: *mut EditLine, a: *mut c_void) {
    todo!()
}

/// C: `static const char *_default_history_file(void);` — `$HOME/.history`,
/// cached in a file-static buffer.
// [spec:libedit:def:readline.default-history-file-fn]
// [spec:libedit:sem:readline.default-history-file-fn]
fn _default_history_file() -> *const c_char {
    todo!()
}

// [spec:libedit:def:readline.rl-set-prompt-fn]
// [spec:libedit:sem:readline.rl-set-prompt-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_prompt(prompt: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-save-prompt-fn]
// [spec:libedit:sem:readline.rl-save-prompt-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_save_prompt() {
    todo!()
}

// [spec:libedit:def:readline.rl-restore-prompt-fn]
// [spec:libedit:sem:readline.rl-restore-prompt-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_restore_prompt() {
    todo!()
}

// [spec:libedit:def:readline.rl-initialize-fn]
// [spec:libedit:sem:readline.rl-initialize-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_initialize() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.readline-fn]
// [spec:libedit:sem:readline.readline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn readline(p: *const c_char) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.using-history-fn]
// [spec:libedit:sem:readline.using-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn using_history() {
    todo!()
}

/// C: `static char *_rl_compat_sub(const char *str, const char *what,
/// const char *with, int globally);`
// [spec:libedit:def:readline.rl-compat-sub-fn]
// [spec:libedit:sem:readline.rl-compat-sub-fn]
fn _rl_compat_sub(
    str_: *const c_char,
    what: *const c_char,
    with: *const c_char,
    globally: c_int,
) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.get-history-event-fn]
// [spec:libedit:sem:readline.get-history-event-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_history_event(
    cmd: *const c_char,
    cindex: *mut c_int,
    qchar: c_int,
) -> *const c_char {
    todo!()
}

/// C: `static int getfrom(const char **cmdp, char **fromp,
/// const char *search, int delim);`
// [spec:libedit:def:readline.getfrom-fn]
// [spec:libedit:sem:readline.getfrom-fn]
fn getfrom(
    cmdp: *mut *const c_char,
    fromp: *mut *mut c_char,
    search: *const c_char,
    delim: c_int,
) -> c_int {
    todo!()
}

/// C: `static int getto(const char **cmdp, char **top, const char *from,
/// int delim);`
// [spec:libedit:def:readline.getto-fn]
// [spec:libedit:sem:readline.getto-fn]
fn getto(
    cmdp: *mut *const c_char,
    top: *mut *mut c_char,
    from: *const c_char,
    delim: c_int,
) -> c_int {
    todo!()
}

/// C: `static void replace(char **tmp, int c);`
// [spec:libedit:def:readline.replace-fn]
// [spec:libedit:sem:readline.replace-fn]
fn replace(tmp: *mut *mut c_char, c: c_int) {
    todo!()
}

/// C: `static int _history_expand_command(const char *command, size_t offs,
/// size_t cmdlen, char **result);`
// [spec:libedit:def:readline.history-expand-command-fn]
// [spec:libedit:sem:readline.history-expand-command-fn]
fn _history_expand_command(
    command: *const c_char,
    offs: usize,
    cmdlen: usize,
    result: *mut *mut c_char,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-expand-fn]
// [spec:libedit:sem:readline.history-expand-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_expand(str_: *mut c_char, output: *mut *mut c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-arg-extract-fn]
// [spec:libedit:sem:readline.history-arg-extract-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_arg_extract(
    start: c_int,
    end: c_int,
    str_: *const c_char,
) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.history-tokenize-fn]
// [spec:libedit:sem:readline.history-tokenize-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_tokenize(str_: *const c_char) -> *mut *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.stifle-history-fn]
// [spec:libedit:sem:readline.stifle-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stifle_history(max: c_int) {
    todo!()
}

// [spec:libedit:def:readline.unstifle-history-fn]
// [spec:libedit:sem:readline.unstifle-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unstifle_history() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-is-stifled-fn]
// [spec:libedit:sem:readline.history-is-stifled-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_is_stifled() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-truncate-file-fn]
// [spec:libedit:sem:readline.history-truncate-file-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_truncate_file(filename: *const c_char, nlines: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.read-history-fn]
// [spec:libedit:sem:readline.read-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read_history(filename: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.write-history-fn]
// [spec:libedit:sem:readline.write-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn write_history(filename: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.append-history-fn]
// [spec:libedit:sem:readline.append-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn append_history(n: c_int, filename: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-get-fn]
// [spec:libedit:sem:readline.history-get-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_get(num: c_int) -> *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.add-history-fn]
// [spec:libedit:sem:readline.add-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn add_history(line: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.remove-history-fn]
// [spec:libedit:sem:readline.remove-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remove_history(num: c_int) -> *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.replace-history-entry-fn]
// [spec:libedit:sem:readline.replace-history-entry-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn replace_history_entry(
    num: c_int,
    line: *const c_char,
    data: HistdataT,
) -> *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.clear-history-fn]
// [spec:libedit:sem:readline.clear-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clear_history() {
    todo!()
}

// [spec:libedit:def:readline.where-history-fn]
// [spec:libedit:sem:readline.where-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn where_history() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-list-fn]
// [spec:libedit:sem:readline.history-list-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_list() -> *mut *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.current-history-fn]
// [spec:libedit:sem:readline.current-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn current_history() -> *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.history-total-bytes-fn]
// [spec:libedit:sem:readline.history-total-bytes-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_total_bytes() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-set-pos-fn]
// [spec:libedit:sem:readline.history-set-pos-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_set_pos(pos: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.previous-history-fn]
// [spec:libedit:sem:readline.previous-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn previous_history() -> *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.next-history-fn]
// [spec:libedit:sem:readline.next-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn next_history() -> *mut HistEntry {
    todo!()
}

// [spec:libedit:def:readline.history-search-fn]
// [spec:libedit:sem:readline.history-search-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_search(str_: *const c_char, direction: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-search-prefix-fn]
// [spec:libedit:sem:readline.history-search-prefix-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_search_prefix(str_: *const c_char, direction: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-search-pos-fn]
// [spec:libedit:sem:readline.history-search-pos-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_search_pos(
    str_: *const c_char,
    direction: c_int,
    pos: c_int,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.tilde-expand-fn]
// [spec:libedit:sem:readline.tilde-expand-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tilde_expand(name: *mut c_char) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.filename-completion-function-fn]
// [spec:libedit:sem:readline.filename-completion-function-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filename_completion_function(
    name: *const c_char,
    state: c_int,
) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.username-completion-function-fn]
// [spec:libedit:sem:readline.username-completion-function-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn username_completion_function(
    text: *const c_char,
    state: c_int,
) -> *mut c_char {
    todo!()
}

/// C: `static unsigned char _el_rl_tstp(EditLine *el, int ch);` — the
/// `ED_TTY_SIGTSTP`-alike bound to `^Z`.
// [spec:libedit:def:readline.el-rl-tstp-fn]
// [spec:libedit:sem:readline.el-rl-tstp-fn]
fn _el_rl_tstp(el: *mut EditLine, ch: c_int) -> c_uchar {
    todo!()
}

/// C: `static const char *_rl_completion_append_character_function(
/// const char *dummy);` — renders `rl_completion_append_character` for
/// `fn_complete2`.
// [spec:libedit:def:readline.rl-completion-append-character-function-fn]
// [spec:libedit:sem:readline.rl-completion-append-character-function-fn]
fn _rl_completion_append_character_function(dummy: *const c_char) -> *const c_char {
    todo!()
}

// [spec:libedit:def:readline.rl-display-match-list-fn]
// [spec:libedit:sem:readline.rl-display-match-list-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_display_match_list(matches: *mut *mut c_char, len: c_int, max: c_int) {
    todo!()
}

// [spec:libedit:def:readline.rl-complete-fn]
// [spec:libedit:sem:readline.rl-complete-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_complete(ignore: c_int, invoking_key: c_int) -> c_int {
    todo!()
}

/// C: `static unsigned char _el_rl_complete(EditLine *el, int ch);` — the
/// editor command bound to TAB, which calls `rl_complete`.
// [spec:libedit:def:readline.el-rl-complete-fn]
// [spec:libedit:sem:readline.el-rl-complete-fn]
fn _el_rl_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    todo!()
}

// [spec:libedit:def:readline.rl-bind-key-fn]
// [spec:libedit:sem:readline.rl-bind-key-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_bind_key(c: c_int, func: Option<RlCommandFunc>) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-read-key-fn]
// [spec:libedit:sem:readline.rl-read-key-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_read_key() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-reset-terminal-fn]
// [spec:libedit:sem:readline.rl-reset-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_reset_terminal(p: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-insert-fn]
// [spec:libedit:sem:readline.rl-insert-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_insert(count: c_int, c: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-insert-text-fn]
// [spec:libedit:sem:readline.rl-insert-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_insert_text(text: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-newline-fn]
// [spec:libedit:sem:readline.rl-newline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_newline(count: c_int, c: c_int) -> c_int {
    todo!()
}

/// C: `static unsigned char rl_bind_wrapper(EditLine *el, unsigned char c);`
/// — dispatches an editor keystroke into the `map[]` table of
/// `rl_command_func_t`s installed by `rl_bind_key`.
// [spec:libedit:def:readline.rl-bind-wrapper-fn]
// [spec:libedit:sem:readline.rl-bind-wrapper-fn]
fn rl_bind_wrapper(el: *mut EditLine, c: c_uchar) -> c_uchar {
    todo!()
}

// [spec:libedit:def:readline.rl-add-defun-fn]
// [spec:libedit:sem:readline.rl-add-defun-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_add_defun(
    name: *const c_char,
    fun: Option<RlCommandFunc>,
    c: c_int,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-callback-read-char-fn]
// [spec:libedit:sem:readline.rl-callback-read-char-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_callback_read_char() {
    todo!()
}

// [spec:libedit:def:readline.rl-callback-handler-install-fn]
// [spec:libedit:sem:readline.rl-callback-handler-install-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_callback_handler_install(
    prompt: *const c_char,
    linefunc: Option<RlVcpfunc>,
) {
    todo!()
}

// [spec:libedit:def:readline.rl-callback-handler-remove-fn]
// [spec:libedit:sem:readline.rl-callback-handler-remove-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_callback_handler_remove() {
    todo!()
}

// [spec:libedit:def:readline.rl-redisplay-fn]
// [spec:libedit:sem:readline.rl-redisplay-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_redisplay() {
    todo!()
}

// [spec:libedit:def:readline.rl-get-previous-history-fn]
// [spec:libedit:sem:readline.rl-get-previous-history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_get_previous_history(count: c_int, key: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-prep-terminal-fn]
// [spec:libedit:sem:readline.rl-prep-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_prep_terminal(meta_flag: c_int) {
    todo!()
}

// [spec:libedit:def:readline.rl-deprep-terminal-fn]
// [spec:libedit:sem:readline.rl-deprep-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_deprep_terminal() {
    todo!()
}

// [spec:libedit:def:readline.rl-read-init-file-fn]
// [spec:libedit:sem:readline.rl-read-init-file-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_read_init_file(s: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-parse-and-bind-fn]
// [spec:libedit:sem:readline.rl-parse-and-bind-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_parse_and_bind(line: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-variable-bind-fn]
// [spec:libedit:sem:readline.rl-variable-bind-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_variable_bind(var: *const c_char, value: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-stuff-char-fn]
// [spec:libedit:sem:readline.rl-stuff-char-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_stuff_char(c: c_int) -> c_int {
    todo!()
}

/// C: `static int _rl_event_read_char(EditLine *el, wchar_t *wc);` — the
/// `EL_GETCFN` shim that spins `rl_event_hook` while the read would block.
// [spec:libedit:def:readline.rl-event-read-char-fn]
// [spec:libedit:sem:readline.rl-event-read-char-fn]
fn _rl_event_read_char(el: *mut EditLine, wc: *mut u32) -> c_int {
    todo!()
}

/// C: `static void _rl_update_pos(void);` — republishes `el_line` into
/// `rl_point` and `rl_end`.
// [spec:libedit:def:readline.rl-update-pos-fn]
// [spec:libedit:sem:readline.rl-update-pos-fn]
fn _rl_update_pos() {
    todo!()
}

// [spec:libedit:def:readline.rl-copy-text-fn]
// [spec:libedit:sem:readline.rl-copy-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_copy_text(from: c_int, to: c_int) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.rl-replace-line-fn]
// [spec:libedit:sem:readline.rl-replace-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_replace_line(text: *const c_char, clear_undo: c_int) {
    todo!()
}

// [spec:libedit:def:readline.rl-delete-text-fn]
// [spec:libedit:sem:readline.rl-delete-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_delete_text(start: c_int, end: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-get-screen-size-fn]
// [spec:libedit:sem:readline.rl-get-screen-size-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_get_screen_size(rows: *mut c_int, cols: *mut c_int) {
    todo!()
}

/// C: `void rl_message(const char *format, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:readline.rl-message-fn]
// [spec:libedit:sem:readline.rl-message-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_message(format: *const c_char) {
    todo!()
}

// [spec:libedit:def:readline.rl-set-screen-size-fn]
// [spec:libedit:sem:readline.rl-set-screen-size-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_screen_size(rows: c_int, cols: c_int) {
    todo!()
}

// [spec:libedit:def:readline.rl-completion-matches-fn]
// [spec:libedit:sem:readline.rl-completion-matches-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_completion_matches(
    str_: *const c_char,
    fun: Option<RlCompentryFunc>,
) -> *mut *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.rl-filename-completion-function-fn]
// [spec:libedit:sem:readline.rl-filename-completion-function-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_filename_completion_function(
    text: *const c_char,
    state: c_int,
) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:readline.rl-forced-update-display-fn]
// [spec:libedit:sem:readline.rl-forced-update-display-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_forced_update_display() {
    todo!()
}

// [spec:libedit:def:readline.rl-abort-internal-fn]
// [spec:libedit:sem:readline.rl-abort-internal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rl_abort_internal() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-qsort-string-compare-fn]
// [spec:libedit:sem:readline.rl-qsort-string-compare-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rl_qsort_string_compare(
    s1: *mut *mut c_char,
    s2: *mut *mut c_char,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.history-get-history-state-fn]
// [spec:libedit:sem:readline.history-get-history-state-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_get_history_state() -> *mut HistoryState {
    todo!()
}

// [spec:libedit:def:readline.rl-kill-full-line-fn]
// [spec:libedit:sem:readline.rl-kill-full-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_kill_full_line(count: c_int, key: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-kill-text-fn]
// [spec:libedit:sem:readline.rl-kill-text-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_kill_text(from: c_int, to: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-make-bare-keymap-fn]
// [spec:libedit:sem:readline.rl-make-bare-keymap-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_make_bare_keymap() -> Keymap {
    todo!()
}

// [spec:libedit:def:readline.rl-get-keymap-fn]
// [spec:libedit:sem:readline.rl-get-keymap-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_get_keymap() -> Keymap {
    todo!()
}

// [spec:libedit:def:readline.rl-set-keymap-fn]
// [spec:libedit:sem:readline.rl-set-keymap-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_keymap(k: Keymap) {
    todo!()
}

// [spec:libedit:def:readline.rl-generic-bind-fn]
// [spec:libedit:sem:readline.rl-generic-bind-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_generic_bind(
    type_: c_int,
    keyseq: *const c_char,
    data: *const c_char,
    k: Keymap,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-bind-key-in-map-fn]
// [spec:libedit:sem:readline.rl-bind-key-in-map-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_bind_key_in_map(
    key: c_int,
    fun: Option<RlCommandFunc>,
    k: Keymap,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-set-key-fn]
// [spec:libedit:sem:readline.rl-set-key-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_key(
    keyseq: *const c_char,
    function: Option<RlCommandFunc>,
    k: Keymap,
) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-cleanup-after-signal-fn]
// [spec:libedit:sem:readline.rl-cleanup-after-signal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_cleanup_after_signal() {
    todo!()
}

// [spec:libedit:def:readline.rl-on-new-line-fn]
// [spec:libedit:sem:readline.rl-on-new-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_on_new_line() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-free-line-state-fn]
// [spec:libedit:sem:readline.rl-free-line-state-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_free_line_state() {
    todo!()
}

// [spec:libedit:def:readline.rl-set-keyboard-input-timeout-fn]
// [spec:libedit:sem:readline.rl-set-keyboard-input-timeout-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_keyboard_input_timeout(u: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-resize-terminal-fn]
// [spec:libedit:sem:readline.rl-resize-terminal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_resize_terminal() {
    todo!()
}

// [spec:libedit:def:readline.rl-reset-after-signal-fn]
// [spec:libedit:sem:readline.rl-reset-after-signal-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_reset_after_signal() {
    todo!()
}

// [spec:libedit:def:readline.rl-echo-signal-char-fn]
// [spec:libedit:sem:readline.rl-echo-signal-char-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_echo_signal_char(sig: c_int) {
    todo!()
}

// [spec:libedit:def:readline.rl-crlf-fn]
// [spec:libedit:sem:readline.rl-crlf-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_crlf() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-ding-fn]
// [spec:libedit:sem:readline.rl-ding-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_ding() -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-abort-fn]
// [spec:libedit:sem:readline.rl-abort-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_abort(count: c_int, key: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.rl-set-keymap-name-fn]
// [spec:libedit:sem:readline.rl-set-keymap-name-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rl_set_keymap_name(name: *const c_char, k: Keymap) -> c_int {
    todo!()
}

// [spec:libedit:def:readline.free-history-entry-fn]
// [spec:libedit:sem:readline.free-history-entry-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free_history_entry(he: *mut HistEntry) -> HistdataT {
    todo!()
}

// [spec:libedit:def:readline.rl-erase-entire-line-fn]
// [spec:libedit:sem:readline.rl-erase-entire-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _rl_erase_entire_line() {
    todo!()
}

// ---------------------------------------------------------------------------
// Declared by `editline/readline.h`, defined elsewhere in the C.
// ---------------------------------------------------------------------------

/// C: `char **completion_matches(/* const */ char *, rl_compentry_func_t *);`
///
/// The pre-4.2 GNU readline generator-to-match-list entry point. The header
/// declares it, but `readline.c` `#define`s the name away around its own
/// `#include` and never defines it: the symbol a consumer links is
/// `filecomplete.c`'s, whose behaviour is `sem:filecomplete.completion-matches-fn`
/// and which libedit itself reaches from `fn_complete2`. A port must export
/// exactly one symbol of this name, so the export lives here — in the crate
/// that owns the ABI — and defers to the core's `filecomplete`.
///
/// Note the prototypes differ in `const`ness between the two headers. That is
/// formally incompatible and harmless at the ABI level; `text` is borrowed,
/// read-only and never retained.
// [spec:libedit:def:readline.completion-matches-fn]
// [spec:libedit:sem:readline.completion-matches-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn completion_matches(
    text: *mut c_char,
    genfunc: Option<RlCompentryFunc>,
) -> *mut *mut c_char {
    todo!()
}
