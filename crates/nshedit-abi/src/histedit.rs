//! The `histedit.h` surface; rules in `docs/spec/port/src/histedit.md`.
//!
//! libedit's own C API: the `el_*`, `history*` and `tok_*` entry points, in
//! header order. Both the narrow and the wide halves are here, because the
//! header declares both.
//!
//! Nine of the header's declarations — `el_gets`, `el_getc`, `el_push`,
//! `el_parse`, `el_set`, `el_get`, `el_line`, `el_insertstr` and
//! `el_replacestr` — are defined by `eln.c`, which carries rules of its own.
//! Rust cannot define one symbol twice, so those nine are defined in
//! [`crate::eln`] and re-exported from here, each re-export carrying the
//! header's `def`/`sem` pair.

use core::ffi::{c_char, c_int, c_uchar};

use nshedit::el::CFile;
use nshedit::histedit::{
    EditLine, HistEvent, HistEventW, History, HistoryW, LineInfo, LineInfoW, Tokenizer, TokenizerW,
};

// [spec:libedit:def:histedit.el-init-fn]
// [spec:libedit:sem:histedit.el-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_init(
    prog: *const c_char,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
) -> *mut EditLine {
    todo!()
}

// [spec:libedit:def:histedit.el-init-fd-fn]
// [spec:libedit:sem:histedit.el-init-fd-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_init_fd(
    prog: *const c_char,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
    fdin: c_int,
    fdout: c_int,
    fderr: c_int,
) -> *mut EditLine {
    todo!()
}

// [spec:libedit:def:histedit.el-end-fn]
// [spec:libedit:sem:histedit.el-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_end(el: *mut EditLine) {
    todo!()
}

// [spec:libedit:def:histedit.el-reset-fn]
// [spec:libedit:sem:histedit.el-reset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_reset(el: *mut EditLine) {
    todo!()
}

/// C: `const char *el_gets(EditLine *, int *);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-gets-fn]
// [spec:libedit:sem:histedit.el-gets-fn]
pub use crate::eln::el_gets;

/// C: `int el_getc(EditLine *, char *);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-getc-fn]
// [spec:libedit:sem:histedit.el-getc-fn]
pub use crate::eln::el_getc;

/// C: `void el_push(EditLine *, const char *);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-push-fn]
// [spec:libedit:sem:histedit.el-push-fn]
pub use crate::eln::el_push;

// [spec:libedit:def:histedit.el-beep-fn]
// [spec:libedit:sem:histedit.el-beep-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_beep(el: *mut EditLine) {
    todo!()
}

/// C: `int el_parse(EditLine *, int, const char **);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-parse-fn]
// [spec:libedit:sem:histedit.el-parse-fn]
pub use crate::eln::el_parse;

/// C: `int el_set(EditLine *, int, ...);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-set-fn]
// [spec:libedit:sem:histedit.el-set-fn]
pub use crate::eln::el_set;

/// C: `int el_get(EditLine *, int, ...);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-get-fn]
// [spec:libedit:sem:histedit.el-get-fn]
pub use crate::eln::el_get;

/// C: `unsigned char _el_fn_complete(EditLine *, int);` — the built-in
/// filename completion command. Its body is `filecomplete.c`'s, under
/// `sem:filecomplete.el-fn-complete-fn`.
// [spec:libedit:def:histedit.el-fn-complete-fn]
// [spec:libedit:sem:histedit.el-fn-complete-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _el_fn_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    todo!()
}

/// C: `unsigned char _el_fn_sh_complete(EditLine *, int);` — the
/// shell-quoting variant. Its body is `filecomplete.c`'s, under
/// `sem:filecomplete.el-fn-sh-complete-fn`.
// [spec:libedit:def:histedit.el-fn-sh-complete-fn]
// [spec:libedit:sem:histedit.el-fn-sh-complete-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _el_fn_sh_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    todo!()
}

// [spec:libedit:def:histedit.el-source-fn]
// [spec:libedit:sem:histedit.el-source-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_source(el: *mut EditLine, fname: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.el-resize-fn]
// [spec:libedit:sem:histedit.el-resize-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_resize(el: *mut EditLine) {
    todo!()
}

/// C: `const LineInfo *el_line(EditLine *);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-line-fn]
// [spec:libedit:sem:histedit.el-line-fn]
pub use crate::eln::el_line;

/// C: `int el_insertstr(EditLine *, const char *);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-insertstr-fn]
// [spec:libedit:sem:histedit.el-insertstr-fn]
pub use crate::eln::el_insertstr;

// [spec:libedit:def:histedit.el-deletestr-fn]
// [spec:libedit:sem:histedit.el-deletestr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_deletestr(el: *mut EditLine, count: c_int) {
    todo!()
}

/// C: `int el_replacestr(EditLine *, const char *);`
///
/// Defined in [`crate::eln`]; re-exported so the header's rules sit with
/// the rest of `histedit.h`.
// [spec:libedit:def:histedit.el-replacestr-fn]
// [spec:libedit:sem:histedit.el-replacestr-fn]
pub use crate::eln::el_replacestr;

// [spec:libedit:def:histedit.el-deletestr1-fn]
// [spec:libedit:sem:histedit.el-deletestr1-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_deletestr1(el: *mut EditLine, start: c_int, end: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.history-init-fn]
// [spec:libedit:sem:histedit.history-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_init() -> *mut History {
    todo!()
}

// [spec:libedit:def:histedit.history-end-fn]
// [spec:libedit:sem:histedit.history-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_end(h: *mut History) {
    todo!()
}

/// C: `int history(History *, HistEvent *, int, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:histedit.history-fn]
// [spec:libedit:sem:histedit.history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history(h: *mut History, ev: *mut HistEvent, op: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.tok-init-fn]
// [spec:libedit:sem:histedit.tok-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_init(ifs: *const c_char) -> *mut Tokenizer {
    todo!()
}

// [spec:libedit:def:histedit.tok-end-fn]
// [spec:libedit:sem:histedit.tok-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_end(tok: *mut Tokenizer) {
    todo!()
}

// [spec:libedit:def:histedit.tok-reset-fn]
// [spec:libedit:sem:histedit.tok-reset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_reset(tok: *mut Tokenizer) {
    todo!()
}

// [spec:libedit:def:histedit.tok-line-fn]
// [spec:libedit:sem:histedit.tok-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_line(
    tok: *mut Tokenizer,
    line: *const LineInfo,
    argc: *mut c_int,
    argv: *mut *mut *const c_char,
    cursorc: *mut c_int,
    cursoro: *mut c_int,
) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.tok-str-fn]
// [spec:libedit:sem:histedit.tok-str-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_str(
    tok: *mut Tokenizer,
    line: *const c_char,
    argc: *mut c_int,
    argv: *mut *mut *const c_char,
) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.wcsdup-fn]
// [spec:libedit:sem:histedit.wcsdup-fn]
//
// C: `wchar_t *wcsdup(const wchar_t *str);`, declared only when the build
// found the platform's libc lacks it (`#ifndef HAVE_WCSDUP`).
//
// Deliberately not reproduced, and deliberately not exported. It is a libc
// gap-filler with no libedit-specific behaviour, `sem:histedit.wcsdup-fn`
// puts it outside the port's scope under `dec:libedit:posix-only-scope`, and
// exporting a symbol of this name from `libnshedit.so` would interpose on
// libc's for every other library in the process. The rules are claimed here
// so the omission is recorded rather than silent.

// [spec:libedit:def:histedit.el-wgets-fn]
// [spec:libedit:sem:histedit.el-wgets-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wgets(el: *mut EditLine, nread: *mut c_int) -> *const u32 {
    todo!()
}

// [spec:libedit:def:histedit.el-wgetc-fn]
// [spec:libedit:sem:histedit.el-wgetc-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wgetc(el: *mut EditLine, wc: *mut u32) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.el-wpush-fn]
// [spec:libedit:sem:histedit.el-wpush-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wpush(el: *mut EditLine, str_: *const u32) {
    todo!()
}

// [spec:libedit:def:histedit.el-wparse-fn]
// [spec:libedit:sem:histedit.el-wparse-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wparse(el: *mut EditLine, argc: c_int, argv: *mut *const u32) -> c_int {
    todo!()
}

/// C: `int el_wset(EditLine *, int, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:histedit.el-wset-fn]
// [spec:libedit:sem:histedit.el-wset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wset(el: *mut EditLine, op: c_int) -> c_int {
    todo!()
}

/// C: `int el_wget(EditLine *, int, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:histedit.el-wget-fn]
// [spec:libedit:sem:histedit.el-wget-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wget(el: *mut EditLine, op: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.el-cursor-fn]
// [spec:libedit:sem:histedit.el-cursor-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_cursor(el: *mut EditLine, n: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.el-wline-fn]
// [spec:libedit:sem:histedit.el-wline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wline(el: *mut EditLine) -> *const LineInfoW {
    todo!()
}

// [spec:libedit:def:histedit.el-winsertstr-fn]
// [spec:libedit:sem:histedit.el-winsertstr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_winsertstr(el: *mut EditLine, str_: *const u32) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.el-wreplacestr-fn]
// [spec:libedit:sem:histedit.el-wreplacestr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wreplacestr(el: *mut EditLine, str_: *const u32) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.history-winit-fn]
// [spec:libedit:sem:histedit.history-winit-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_winit() -> *mut HistoryW {
    todo!()
}

// [spec:libedit:def:histedit.history-wend-fn]
// [spec:libedit:sem:histedit.history-wend-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_wend(h: *mut HistoryW) {
    todo!()
}

/// C: `int history_w(HistoryW *, HistEventW *, int, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:histedit.history-w-fn]
// [spec:libedit:sem:histedit.history-w-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_w(h: *mut HistoryW, ev: *mut HistEventW, op: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.tok-winit-fn]
// [spec:libedit:sem:histedit.tok-winit-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_winit(ifs: *const u32) -> *mut TokenizerW {
    todo!()
}

// [spec:libedit:def:histedit.tok-wend-fn]
// [spec:libedit:sem:histedit.tok-wend-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_wend(tok: *mut TokenizerW) {
    todo!()
}

// [spec:libedit:def:histedit.tok-wreset-fn]
// [spec:libedit:sem:histedit.tok-wreset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_wreset(tok: *mut TokenizerW) {
    todo!()
}

// [spec:libedit:def:histedit.tok-wline-fn]
// [spec:libedit:sem:histedit.tok-wline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_wline(
    tok: *mut TokenizerW,
    line: *const LineInfoW,
    argc: *mut c_int,
    argv: *mut *mut *const u32,
    cursorc: *mut c_int,
    cursoro: *mut c_int,
) -> c_int {
    todo!()
}

// [spec:libedit:def:histedit.tok-wstr-fn]
// [spec:libedit:sem:histedit.tok-wstr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_wstr(
    tok: *mut TokenizerW,
    line: *const u32,
    argc: *mut c_int,
    argv: *mut *mut *const u32,
) -> c_int {
    todo!()
}
