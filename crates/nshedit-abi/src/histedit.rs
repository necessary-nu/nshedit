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
//!
//! # The varargs entry points
//!
//! `el_set`, `el_get`, `el_wset`, `el_wget`, `history` and `history_w` are
//! `...` functions in C. Rust cannot *define* one on stable (`c_variadic`,
//! rust-lang/rust#44930), so the four defined here are declared with **fixed
//! arity**: enough trailing `*mut c_void` parameters for the widest op, read
//! positionally by the dispatch. Every op code has a known argument shape —
//! that enumeration is `el_wset`/`el_wget`/`history_w` below — so nothing is
//! lost, and the exported symbol is still the C one.
//!
//! This is correct on **x86-64 System V** and on **AArch64 AAPCS**, where a
//! variadic call places its arguments in exactly the registers and stack
//! slots a fixed-arity call of the same shape would: the first six (x86-64)
//! or eight (AArch64) integer/pointer arguments in registers, the rest on the
//! stack, with `al`/nothing carrying the vector-register count that no op
//! here uses.
//!
//! It is **wrong on AArch64 Apple** (`arm64-apple-darwin`), whose calling
//! convention passes *every* variadic argument on the stack, one 8-byte slot
//! each, immediately after the named parameters. A fixed-arity callee there
//! would read the caller's `a1..a19` out of x2..x7 and never look at the
//! stack. Supporting that target needs a different body for these four
//! functions: either a hand-written naked-function trampoline that spills
//! x0..x7 into a frame the dispatch then reads from the stack, or a
//! `#[cfg(target_vendor = "apple")]` module built with `c_variadic` once it
//! stabilises. Nothing here is gated on a nightly feature.
//!
//! # Core gaps
//!
//! Some bodies below are one call into `nshedit` that this crate cannot make,
//! because the entry point is `pub(crate)` there or because its type is not a
//! C ABI type. Those are marked with [`core_gap`] rather than guessed at; see
//! that function's documentation for the full list.

use core::ffi::{c_char, c_int, c_uchar, c_void};
use std::cell::RefCell;
use std::collections::HashMap;

use nshedit::el::CFile;
use nshedit::histedit::{
    EditLine, HistEvent, HistEventW, History, HistoryW, LineInfo, LineInfoW, Tokenizer, TokenizerW,
};
use nshedit::history::HistoryArg;

// ---------------------------------------------------------------------------
// `el_set`/`el_get` operation codes. C: `histedit.h`, which defines them as
// untyped `#define`s carrying no rule of their own. They live here and not in
// `nshedit::histedit` because they exist only to select an arm of the varargs
// dispatch, and `plan/decisions/idiomatic-core.md` puts that dispatch in this
// crate. The numbering is ABI: a consumer passes these integers directly.
// ---------------------------------------------------------------------------

const EL_PROMPT: c_int = 0;
const EL_TERMINAL: c_int = 1;
const EL_EDITOR: c_int = 2;
const EL_SIGNAL: c_int = 3;
const EL_BIND: c_int = 4;
const EL_TELLTC: c_int = 5;
const EL_SETTC: c_int = 6;
const EL_ECHOTC: c_int = 7;
const EL_SETTY: c_int = 8;
const EL_ADDFN: c_int = 9;
const EL_HIST: c_int = 10;
const EL_EDITMODE: c_int = 11;
const EL_RPROMPT: c_int = 12;
const EL_GETCFN: c_int = 13;
const EL_CLIENTDATA: c_int = 14;
const EL_UNBUFFERED: c_int = 15;
const EL_PREP_TERM: c_int = 16;
const EL_GETTC: c_int = 17;
const EL_GETFP: c_int = 18;
const EL_SETFP: c_int = 19;
const EL_REFRESH: c_int = 20;
const EL_PROMPT_ESC: c_int = 21;
const EL_RPROMPT_ESC: c_int = 22;
const EL_RESIZE: c_int = 23;
const EL_ALIAS_TEXT: c_int = 24;
const EL_SAFEREAD: c_int = 25;
const EL_WORDCHARS: c_int = 26;
const EL_GETENV: c_int = 27;

// `el_flags` bits. C: `el.h`. `nshedit::el` declares the same constants
// `pub(crate)`, so they are restated here rather than imported; the ops that
// read and write them are inline in the C's own `el_wset`/`el_wget` bodies,
// which is why this crate touches `el_flags` at all.
const HANDLE_SIGNALS: i32 = 0x001;
const EDIT_DISABLED: i32 = 0x004;
const UNBUFFERED: i32 = 0x008;
/// The bit `el_wget(EL_SAFEREAD)` stores raw — 256, not 1. See
/// `sem:histedit.el-wget-fn`.
const FIXIO: i32 = 0x100;

// ---------------------------------------------------------------------------
// Helpers: the C-shaped plumbing this crate exists to own.
// ---------------------------------------------------------------------------

/// A body that is one call into `nshedit`, where the call cannot be written
/// from this crate. Diverging silently would be worse than stopping, so this
/// stops — an `extern "C"` function aborts rather than unwinding, so no
/// unwind crosses the ABI.
///
/// Two causes, both of them things `nshedit` owes this crate:
///
/// 1. **Visibility.** `prompt_set`, `prompt_get`, `ch_resizefun`,
///    `ch_aliasfun`, `terminal_set`, `terminal_get`, `terminal_gettc`,
///    `terminal_telltc`, `terminal_settc`, `terminal_echotc`,
///    `terminal__flush`, `map_set_editor`, `map_get_editor`, `map_bind`,
///    `map_addfunc`, `map_set_wordchars`, `map_get_wordchars`, `tty_stty`,
///    `tty_rawmode`, `tty_cookedmode`, `hist_set`, `el_read_setfn`,
///    `el_read_getfn`, `read_prepare`, `read_finish`, `re_clear_display` and
///    `re_refresh` are all `pub(crate)` in `nshedit`. Every one of them is
///    the whole body of an `el_wset`/`el_wget` arm in `el.c`.
/// 2. **Callback types.** `ElPfuncT`, `ElZfuncT`, `ElAfuncT`, `ElFuncT`,
///    `ElRfuncT`, `FuncT` and the four `history` vtable typedefs
///    (`HistoryGfunT`, `HistoryEfunT`, `HistoryVfunT`, `HistorySfunT`) are
///    plain Rust `fn` types in `nshedit`. A C caller hands us an
///    `extern "C"` function pointer, and there is no way to turn one into a
///    Rust-ABI `fn` value. These typedefs have to become
///    `unsafe extern "C" fn(...)` — as `nshedit::hist::HistFunT` already is —
///    before the ops that install them can be written.
/// 3. **The narrow instantiations.** `historyn.c` and `tokenizern.c` are
///    `history.c`/`tokenizer.c` recompiled with `Char = char`. `nshedit`
///    declares `History` and `Tokenizer` as opaque placeholders and leaves
///    those bodies "to the history translation" / "to the tokenizer
///    translation". Until they exist there is nothing for the narrow
///    `history`/`tok_*` entry points to call, and synthesising them over the
///    wide store would change defined behaviour (byte semantics become wide
///    semantics), which `dec:libedit:conformance-policy` reserves for a
///    recorded decision rather than an implementation choice.
#[cold]
fn core_gap(needs: &str) -> ! {
    panic!("nshedit-abi: this entry point is blocked on `nshedit` — needs {needs}");
}

/// The C's `const wchar_t *`: a NUL-terminated wide string, or `NULL`.
///
/// # Safety
/// `p` is either null or points at a NUL-terminated `wchar_t` sequence, as
/// the corresponding `sem` rule requires of the caller.
unsafe fn wstr<'a>(p: *const u32) -> Option<&'a [u32]> {
    if p.is_null() {
        return None;
    }
    let mut n = 0usize;
    // SAFETY: the caller's contract is a NUL-terminated string.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `n` elements were just read, so the range is valid.
    Some(unsafe { core::slice::from_raw_parts(p, n) })
}

/// The C's `const char *`: a NUL-terminated byte string, or `NULL`.
///
/// # Safety
/// As [`wstr`], for bytes.
unsafe fn cbytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        return None;
    }
    let mut n = 0usize;
    // SAFETY: the caller's contract is a NUL-terminated string.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `n` bytes were just read, so the range is valid.
    Some(unsafe { core::slice::from_raw_parts(p.cast::<u8>(), n) })
}

/// The program name `el_init`/`el_init_fd` take.
///
/// C decodes it from the current locale's multibyte encoding and duplicates
/// the result, dereferencing `NULL` on the way (`sem:histedit.el-init-fd-fn`
/// step 4 calls that undefined). Defined here as "a `NULL` or undecodable
/// name is a failed construction", which is the C's own reaction to a decode
/// failure and turns the undefined case into the documented `NULL` return.
///
/// `nshedit::el::el_init` takes `&str`, so the decode is UTF-8 rather than
/// the process locale; see the crate report.
///
/// # Safety
/// As [`cbytes`].
unsafe fn prog_name<'a>(p: *const c_char) -> Option<&'a str> {
    core::str::from_utf8(unsafe { cbytes(p) }?).ok()
}

// Storage for the two views this crate hands back as raw pointers, keyed by
// the object they belong to.
//
// libedit handles are documented as not thread-safe and must not be shared
// between threads, so thread-local storage is the whole synchronisation story
// here — and it keeps the raw pointers inside out of any `Send` bound.
thread_local! {
    // `el_wline`'s `LineInfoW`. The C returns `&el->el_line` reinterpreted;
    // `nshedit`'s `ElLineT` holds offsets, so the view is materialised here
    // and owned per editor. Boxed so its address survives map growth.
    static WLINE: RefCell<HashMap<usize, Box<LineInfoW>>> = RefCell::new(HashMap::new());
    // `tok_wline`/`tok_wstr`'s `argv`. The C hands back an alias of the
    // tokenizer's own array; `nshedit`'s slots are offsets into `wspace`, so
    // the pointer array is materialised here and replaced — invalidating the
    // previous one, exactly as the rule says — on every call.
    static TOKARGV: RefCell<HashMap<usize, Vec<*const u32>>> = RefCell::new(HashMap::new());
}

// [spec:libedit:def:histedit.el-init-fn]
// [spec:libedit:sem:histedit.el-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_init(
    prog: *const c_char,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
) -> *mut EditLine {
    // SAFETY: `prog` is the caller's NUL-terminated program name.
    let Some(prog) = (unsafe { prog_name(prog) }) else {
        return core::ptr::null_mut();
    };
    // The core derives the three descriptors with `fileno`, as the C does.
    nshedit::el::el_init(prog, fin, fout, ferr).map_or(core::ptr::null_mut(), Box::into_raw)
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
    // SAFETY: `prog` is the caller's NUL-terminated program name.
    let Some(prog) = (unsafe { prog_name(prog) }) else {
        return core::ptr::null_mut();
    };
    nshedit::el::el_init_fd(prog, fin, fout, ferr, fdin, fdout, fderr)
        .map_or(core::ptr::null_mut(), Box::into_raw)
}

// [spec:libedit:def:histedit.el-end-fn]
// [spec:libedit:sem:histedit.el-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_end(el: *mut EditLine) {
    // The one NULL-tolerant entry point in the editing API.
    if el.is_null() {
        nshedit::el::el_end(None);
        return;
    }
    // Every pointer this handle ever handed out is dangling after this, the
    // `el_wline` view included, so drop our copy of it too.
    WLINE.with_borrow_mut(|m| m.remove(&(el as usize)));
    // SAFETY: the caller gives us a live handle from `el_init`/`el_init_fd`,
    // which is exactly the `Box` those returned. A second `el_end` on the
    // same handle is the C's double free and stays the caller's error.
    nshedit::el::el_end(Some(unsafe { Box::from_raw(el) }));
}

// [spec:libedit:def:histedit.el-reset-fn]
// [spec:libedit:sem:histedit.el-reset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_reset(el: *mut EditLine) {
    // SAFETY: `el` must be non-NULL; the C has no check and neither has this.
    nshedit::el::el_reset(unsafe { &mut *el });
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
    // SAFETY: `el` must be non-NULL; there is no check in the C.
    nshedit::el::el_beep(unsafe { &mut *el });
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
    // SAFETY: `el` must be non-NULL. `ch` is ignored, as in the C.
    nshedit::filecomplete::_el_fn_complete(unsafe { &mut *el }, ch)
}

/// C: `unsigned char _el_fn_sh_complete(EditLine *, int);` — the
/// shell-quoting variant. Its body is `filecomplete.c`'s, under
/// `sem:filecomplete.el-fn-sh-complete-fn`.
// [spec:libedit:def:histedit.el-fn-sh-complete-fn]
// [spec:libedit:sem:histedit.el-fn-sh-complete-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn _el_fn_sh_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    // A distinct exported symbol that forwards both arguments unchanged; the
    // two are behaviourally identical and must stay separate symbols.
    // SAFETY: `el` must be non-NULL.
    nshedit::filecomplete::_el_fn_sh_complete(unsafe { &mut *el }, ch)
}

// [spec:libedit:def:histedit.el-source-fn]
// [spec:libedit:sem:histedit.el-source-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_source(el: *mut EditLine, fname: *const c_char) -> c_int {
    use std::os::unix::ffi::OsStrExt;

    // `NULL` selects the `$EDITRC` / `$HOME/.editrc` fallback chain, which is
    // the core's to walk. A supplied name is bytes, not text: `.editrc` paths
    // need not be UTF-8, so it goes through `OsStr` rather than `str`.
    // SAFETY: `fname` is null or a NUL-terminated path.
    let bytes = unsafe { cbytes(fname) };
    let path = bytes.map(|b| std::path::Path::new(std::ffi::OsStr::from_bytes(b)));
    // SAFETY: `el` must be non-NULL.
    nshedit::el::el_source(unsafe { &mut *el }, path)
}

// [spec:libedit:def:histedit.el-resize-fn]
// [spec:libedit:sem:histedit.el-resize-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_resize(el: *mut EditLine) {
    // SAFETY: `el` must be non-NULL. Not async-signal-safe, as in the C.
    nshedit::el::el_resize(unsafe { &mut *el });
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
    // Also exported as `el_wdeletestr`, which the header `#define`s onto this
    // name — one function, counting wide characters under either spelling.
    // A refusal is indistinguishable from a success; there is no return.
    // SAFETY: `el` must be non-NULL.
    nshedit::chared::el_deletestr(unsafe { &mut *el }, count);
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
    // The return is `end - start` whatever was actually removed, and the
    // cursor is only clamped at the low end — ERR-buffer-15, ERR-buffer-16,
    // ERR-buffer-17 and ERR-buffer-18, all reproduced in the core because
    // `rl_delete_text` is layered on this call.
    // SAFETY: `el` must be non-NULL.
    nshedit::chared::el_deletestr1(unsafe { &mut *el }, start, end)
}

// [spec:libedit:def:histedit.history-init-fn]
// [spec:libedit:sem:histedit.history-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_init() -> *mut History {
    // `History` is `history.c` compiled with `Char = char`, a separate store
    // from the wide one with byte strings throughout. See `core_gap` cause 3.
    core_gap("the narrow `historyn.c` instantiation behind `nshedit::histedit::History`")
}

// [spec:libedit:def:histedit.history-end-fn]
// [spec:libedit:sem:histedit.history-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_end(h: *mut History) {
    core_gap("the narrow `historyn.c` instantiation behind `nshedit::histedit::History`")
}

/// C: `int history(History *, HistEvent *, int, ...);`
///
/// Declared with fixed arity — three named parameters plus the eleven
/// `H_FUNC` takes, the widest op — and read positionally. See the module
/// documentation for why that is correct on x86-64 SysV and AArch64 AAPCS and
/// wrong on AArch64 Apple.
// [spec:libedit:def:histedit.history-fn]
// [spec:libedit:sem:histedit.history-fn]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn history(
    h: *mut History,
    ev: *mut HistEvent,
    op: c_int,
    a1: *mut c_void,
    a2: *mut c_void,
    a3: *mut c_void,
    a4: *mut c_void,
    a5: *mut c_void,
    a6: *mut c_void,
    a7: *mut c_void,
    a8: *mut c_void,
    a9: *mut c_void,
    a10: *mut c_void,
    a11: *mut c_void,
) -> c_int {
    // Same op codes, argument shapes, error codes and ownership rules as
    // `history_w` below — `sem:histedit.history-fn` is the rule `history_w`'s
    // is written against — but over a byte store. See `core_gap` cause 3.
    core_gap("the narrow `historyn.c` instantiation behind `nshedit::histedit::History`")
}

// [spec:libedit:def:histedit.tok-init-fn]
// [spec:libedit:sem:histedit.tok-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_init(ifs: *const c_char) -> *mut Tokenizer {
    // `Tokenizer` is `tokenizer.c` compiled with `Char = char`: a byte word
    // space, byte `argv` slots and a byte IFS. See `core_gap` cause 3.
    core_gap("the narrow `tokenizern.c` instantiation behind `nshedit::histedit::Tokenizer`")
}

// [spec:libedit:def:histedit.tok-end-fn]
// [spec:libedit:sem:histedit.tok-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_end(tok: *mut Tokenizer) {
    core_gap("the narrow `tokenizern.c` instantiation behind `nshedit::histedit::Tokenizer`")
}

// [spec:libedit:def:histedit.tok-reset-fn]
// [spec:libedit:sem:histedit.tok-reset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_reset(tok: *mut Tokenizer) {
    core_gap("the narrow `tokenizern.c` instantiation behind `nshedit::histedit::Tokenizer`")
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
    core_gap("the narrow `tokenizern.c` instantiation behind `nshedit::histedit::Tokenizer`")
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
    core_gap("the narrow `tokenizern.c` instantiation behind `nshedit::histedit::Tokenizer`")
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
    // `nread` may be NULL, in which case the core substitutes its own scratch
    // count and discards it.
    // SAFETY: `el` must be non-NULL; `nread` is null or writable.
    let nread = if nread.is_null() {
        None
    } else {
        Some(unsafe { &mut *nread })
    };
    // The returned line is libedit's internal buffer, not a copy: it does not
    // survive the next `el_wgets` or any call that grows the line buffer.
    // SAFETY: `el` must be non-NULL.
    nshedit::read::el_wgets(unsafe { &mut *el }, nread).map_or(core::ptr::null(), <[u32]>::as_ptr)
}

// [spec:libedit:def:histedit.el-wgetc-fn]
// [spec:libedit:sem:histedit.el-wgetc-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wgetc(el: *mut EditLine, wc: *mut u32) -> c_int {
    // Returned verbatim, including the 0 the core reports when `tty_rawmode`
    // fails — a terminal-setup failure indistinguishable from end of file
    // (ERR-input-24). Not corrected to -1 here.
    // SAFETY: `el` and `wc` must both be non-NULL.
    nshedit::read::el_wgetc(unsafe { &mut *el }, unsafe { &mut *wc })
}

// [spec:libedit:def:histedit.el-wpush-fn]
// [spec:libedit:sem:histedit.el-wpush-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wpush(el: *mut EditLine, str_: *const u32) {
    // A NULL string, a full stack or a failed duplication are all reported to
    // the user as a beep and to the caller not at all.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    nshedit::read::el_wpush(unsafe { &mut *el }, unsafe { wstr(str_) });
}

// [spec:libedit:def:histedit.el-wparse-fn]
// [spec:libedit:sem:histedit.el-wparse-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wparse(el: *mut EditLine, argc: c_int, argv: *mut *const u32) -> c_int {
    // The core takes `&[&[u32]]`, which cannot carry the C's NULL entries;
    // `sem:histedit.el-wparse-fn` says they are passed through as NULL wide
    // pointers and that only `argv[0]` is dereferenced, so a NULL slot
    // becomes the empty slice here. See the crate report.
    let n = if argc > 0 { argc as usize } else { 0 };
    let mut words: Vec<&[u32]> = Vec::with_capacity(n);
    for i in 0..n {
        // SAFETY: the caller supplies `argc` entries, each null or a
        // NUL-terminated wide string.
        words.push(unsafe { wstr(*argv.add(i)) }.unwrap_or(&[]));
    }
    // `argc` is passed through rather than derived from the slice: the C
    // reads it as given and the two are the same number here.
    // SAFETY: `el` must be non-NULL.
    nshedit::parse::el_wparse(unsafe { &mut *el }, argc, &words)
}

/// C: `int el_wset(EditLine *, int, ...);`
///
/// Declared with fixed arity — two named parameters plus nineteen, which is
/// what the `EL_BIND` family reads (slots 1..19 of a 20-entry array) — and
/// read positionally. See the module documentation for why that is correct on
/// x86-64 SysV and AArch64 AAPCS and wrong on AArch64 Apple.
///
/// Only the arguments the selected op defines are ever touched; supplying
/// fewer or differently typed arguments for an op stays the caller's
/// undefined behaviour, as in the C.
// [spec:libedit:def:histedit.el-wset-fn]
// [spec:libedit:sem:histedit.el-wset-fn]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn el_wset(
    el: *mut EditLine,
    op: c_int,
    a1: *mut c_void,
    a2: *mut c_void,
    a3: *mut c_void,
    a4: *mut c_void,
    a5: *mut c_void,
    a6: *mut c_void,
    a7: *mut c_void,
    a8: *mut c_void,
    a9: *mut c_void,
    a10: *mut c_void,
    a11: *mut c_void,
    a12: *mut c_void,
    a13: *mut c_void,
    a14: *mut c_void,
    a15: *mut c_void,
    a16: *mut c_void,
    a17: *mut c_void,
    a18: *mut c_void,
    a19: *mut c_void,
) -> c_int {
    // A NULL editor is rejected before any argument is read.
    if el.is_null() {
        return -1;
    }
    // SAFETY: `el` is non-null and is the caller's live handle.
    let el = unsafe { &mut *el };

    // An `int` vararg arrives in the low half of an argument slot.
    let int_arg = |p: *mut c_void| p as usize as u32 as c_int;

    match op {
        // One `el_wpfunc_t`. `prompt_set(el, p, 0, op, 1)`: installs the left
        // prompt for EL_PROMPT and the right otherwise, marks it wide, resets
        // the cached prompt position, and restores the built-in default for a
        // NULL function. Always 0.
        EL_PROMPT | EL_RPROMPT => core_gap("`prompt::prompt_set` and a C-ABI `ElPfuncT`"),

        // An `el_wpfunc_t` then an `int` narrowed to `wchar_t`: the literal
        // escape character bracketing zero-width prompt runs. Always 0. Note
        // `prompt_set` does treat EL_PROMPT_ESC as the left prompt, which is
        // the half of the asymmetry `prompt_get` gets wrong.
        EL_PROMPT_ESC | EL_RPROMPT_ESC => core_gap("`prompt::prompt_set` and a C-ABI `ElPfuncT`"),

        // An `el_zfunc_t` then a `void *`: the resize callback and its
        // cookie. Always 0. Invoked from `el_resize`, from buffer growth and
        // from `el_line`.
        EL_RESIZE => core_gap("`chared::ch_resizefun` and a C-ABI `ElZfuncT`"),

        // An `el_afunc_t` then a `void *`: the alias-expansion callback and
        // its cookie — narrow `char` even in the wide API. Always 0.
        EL_ALIAS_TEXT => core_gap("`chared::ch_aliasfun` and a C-ABI `ElAfuncT`"),

        // One `char *` terminal type, bytes even here. NULL means `$TERM`
        // through the environment hook; `"emacs"` additionally sets
        // EDIT_DISABLED. 0, or -1 if the display arrays could not be grown.
        EL_TERMINAL => core_gap("`terminal::terminal_set`"),

        // One `wchar_t *`: `L\"emacs\"` or `L\"vi\"`, anything else -1. Also
        // resets the word-character set to the map's default.
        EL_EDITOR => core_gap("`map::map_set_editor`"),

        // One `int`. Inline in the C's own dispatch, so inline here.
        EL_SIGNAL => {
            if int_arg(a1) != 0 {
                el.el_flags |= HANDLE_SIGNALS;
            } else {
                el.el_flags &= !HANDLE_SIGNALS;
            }
            // `rv` is left at its initial 0 by this arm, as in the C.
            0
        }

        // A NULL-terminated list of `wchar_t *`, read into slots 1..19 of a
        // 20-entry array and stopping at the first NULL; slot 0 becomes the
        // command name and `argc` the index the scan stopped at. Nineteen
        // non-NULL strings leave the array unterminated with `argc == 20`,
        // which handlers that scan for a terminator then read past
        // (ERR-map-01). The handler's 0/-1 is the result.
        EL_BIND => core_gap("`map::map_bind`"),
        EL_TELLTC => core_gap("`terminal::terminal_telltc`"),
        EL_SETTC => core_gap("`terminal::terminal_settc`"),
        EL_ECHOTC => core_gap("`terminal::terminal_echotc`"),
        EL_SETTY => core_gap("`tty::tty_stty`"),

        // `wchar_t *name`, `wchar_t *help`, `el_func_t`. -1 if any is NULL or
        // either table reallocation fails, else 0. Both strings are
        // duplicated, so the caller keeps its own.
        EL_ADDFN => core_gap("`map::map_addfunc` and a C-ABI `ElFuncT`"),

        // A `hist_fun_t` then its `void *` handle. Then, and only when
        // `MB_CUR_MAX == 1`, clear NARROW_HISTORY — so a narrow
        // `el_set(EL_HIST, ...)` is not undone in a multibyte locale
        // (ERR-history-19). Always 0; libedit does not own the handle.
        EL_HIST => core_gap("`hist::hist_set`"),

        // One `int`, the EINTR-recovery flag. Inline in the C.
        EL_SAFEREAD => {
            if int_arg(a1) != 0 {
                el.el_flags |= FIXIO;
            } else {
                el.el_flags &= !FIXIO;
            }
            0
        }

        // One `int`, inverted: non-zero enables editing. Inline in the C.
        EL_EDITMODE => {
            if int_arg(a1) != 0 {
                el.el_flags &= !EDIT_DISABLED;
            } else {
                el.el_flags |= EDIT_DISABLED;
            }
            0
        }

        // One `el_rfunc_t`; `EL_BUILTIN_GETCFN` (NULL) restores the builtin.
        // Always 0.
        EL_GETCFN => core_gap("`read::el_read_setfn` and a C-ABI `ElRfuncT`"),

        // One `void *`, stored verbatim and never dereferenced. Inline in the
        // C, and this arm leaves `rv` at 0.
        EL_CLIENTDATA => {
            el.el_data = a1;
            0
        }

        // One `int`. A 0 -> non-zero transition sets UNBUFFERED and runs the
        // read-prepare sequence; the reverse clears it and runs read-finish;
        // setting it to the value it already holds does nothing. Always 0.
        EL_UNBUFFERED => core_gap("`read::read_prepare` and `read::read_finish`"),

        // One `int`: non-zero raw, zero cooked, tty errors discarded.
        // Always 0. There is no matching get.
        EL_PREP_TERM => core_gap("`tty::tty_rawmode` and `tty::tty_cookedmode`"),

        // An `int what` then a `FILE *`, installed together with its
        // `fileno`. 0 for what in {0,1,2}, -1 otherwise. `fileno` is called
        // with no NULL check, which the C leaves undefined.
        EL_SETFP => core_gap("`nshedit`'s `fileno` equivalent for `CFile`"),

        // No further arguments: clear the recorded display, redraw prompt and
        // line, flush. Returns 0.
        EL_REFRESH => core_gap(
            "`refresh::re_clear_display`, `refresh::re_refresh` and `terminal::terminal__flush`",
        ),

        // One `wchar_t *`: frees the previous set and installs a duplicate.
        // Always 0 even when the duplication fails or the argument is NULL —
        // the latter is dereferenced by the duplication, undefined in the C.
        EL_WORDCHARS => core_gap("`map::map_set_wordchars`"),

        // One `char *(*)(const char *)`. Always 0. A NULL accessor is
        // installed as-is and every later lookup calls through it, which the
        // C leaves undefined.
        EL_GETENV => core_gap("a C-ABI `el::FuncT`"),

        // Every other op, EL_GETTC and EL_GETFP included, reads no arguments.
        _ => -1,
    }
}

/// C: `int el_wget(EditLine *, int, ...);`
///
/// Declared with fixed arity — two named parameters plus two, which is what
/// the widest get op (`EL_PROMPT_ESC`, `EL_GETTC`, `EL_GETFP`) reads — and
/// read positionally. See the module documentation for the Apple-ABI caveat.
// [spec:libedit:def:histedit.el-wget-fn]
// [spec:libedit:sem:histedit.el-wget-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wget(
    el: *mut EditLine,
    op: c_int,
    a1: *mut c_void,
    a2: *mut c_void,
) -> c_int {
    if el.is_null() {
        return -1;
    }
    // SAFETY: `el` is non-null and is the caller's live handle.
    let el = unsafe { &mut *el };

    match op {
        // One `el_wpfunc_t *`. -1 if NULL, else 0. The value may be the
        // internal default rather than anything the application installed.
        EL_PROMPT | EL_RPROMPT => core_gap("`prompt::prompt_get` and a C-ABI `ElPfuncT`"),

        // An `el_wpfunc_t *` then a `wchar_t *`, the latter optional.
        // `prompt_get` selects the left prompt only for `op == EL_PROMPT`, so
        // EL_PROMPT_ESC reads the *right* prompt's function and escape
        // character — ERR-prompt-02, frozen, and the reason set/get through
        // EL_PROMPT_ESC does not round-trip.
        EL_PROMPT_ESC | EL_RPROMPT_ESC => core_gap("`prompt::prompt_get` and a C-ABI `ElPfuncT`"),

        // One `const wchar_t **`, set to the static `L\"emacs\"`/`L\"vi\"`.
        EL_EDITOR => core_gap("`map::map_get_editor`"),

        // One `int *`, set to the raw HANDLE_SIGNALS bit. Not normalised —
        // it reads as 1 only because that bit happens to be 0x001.
        EL_SIGNAL => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            unsafe { *a1.cast::<c_int>() = el.el_flags & HANDLE_SIGNALS };
            0
        }

        // One `int *`, set to the logical negation of EDIT_DISABLED, so
        // genuinely 0 or 1 and inverted to match the setter's polarity.
        EL_EDITMODE => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            unsafe { *a1.cast::<c_int>() = c_int::from(el.el_flags & EDIT_DISABLED == 0) };
            0
        }

        // One `int *`, set to the raw FIXIO bit — **256**, not 1. A caller
        // comparing it against 1 gets the wrong answer. Frozen behaviour;
        // deliberately not normalised here.
        EL_SAFEREAD => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            unsafe { *a1.cast::<c_int>() = el.el_flags & FIXIO };
            0
        }

        // One `const char **`, set to the loaded terminal type name — narrow
        // bytes even in the wide API. Always 0.
        EL_TERMINAL => core_gap("`terminal::terminal_get`"),

        // A `char *` capability name then a capability-dependent out pointer,
        // built into the argv `{\"gettc\", name, out}`. Exactly two arguments
        // are read despite the header's `..., NULL`. A string capability and
        // the boolean-ish `pt`/`km`/`am`/`xn` want a `char **`; every other
        // numeric one wants an `int *`, and passing the wrong one is a
        // type-confusing store the C leaves undefined.
        EL_GETTC => core_gap("`terminal::terminal_gettc`"),

        // One `el_rfunc_t *`, set to `EL_BUILTIN_GETCFN` (NULL) when the
        // builtin reader is installed — so a set/get round trip normalises
        // the builtin to NULL rather than reporting its address.
        EL_GETCFN => core_gap("`read::el_read_getfn` and a C-ABI `ElRfuncT`"),

        // One `void **`, set to the registered client pointer.
        EL_CLIENTDATA => {
            // SAFETY: the op's argument is a `void **` the caller supplied.
            unsafe { *a1.cast::<*mut c_void>() = el.el_data };
            0
        }

        // One `int *`, normalised to 0 or 1 — unlike EL_SIGNAL and
        // EL_SAFEREAD.
        EL_UNBUFFERED => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            unsafe { *a1.cast::<c_int>() = c_int::from(el.el_flags & UNBUFFERED != 0) };
            0
        }

        // An `int what` then a `FILE **`: input 0, output 1, error 2. Any
        // other `what` returns -1 with the caller's storage untouched. The
        // descriptors cannot be read back, only the streams.
        EL_GETFP => {
            let fp = match a1 as usize as u32 as c_int {
                0 => el.el_infile,
                1 => el.el_outfile,
                2 => el.el_errfile,
                _ => return -1,
            };
            // SAFETY: the op's second argument is a `FILE **`.
            unsafe { *a2.cast::<CFile>() = fp };
            0
        }

        // One `const wchar_t **`, set to the word-character set. NULL means
        // "the built-in defaults are in use", not "empty".
        EL_WORDCHARS => core_gap("`map::map_get_wordchars`"),

        // One `func_t *`, set to the installed environment accessor.
        EL_GETENV => core_gap("a C-ABI `el::FuncT`"),

        // Everything else, which is every set-only op: EL_BIND, EL_TELLTC,
        // EL_SETTC, EL_ECHOTC, EL_SETTY, EL_ADDFN, EL_HIST, EL_PREP_TERM,
        // EL_SETFP, EL_REFRESH, EL_RESIZE and EL_ALIAS_TEXT. No argument is
        // read.
        _ => -1,
    }
}

// [spec:libedit:def:histedit.el-cursor-fn]
// [spec:libedit:sem:histedit.el-cursor-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_cursor(el: *mut EditLine, n: c_int) -> c_int {
    // The C advances the cursor pointer and only then clamps, transiently
    // forming an out-of-range pointer (ERR-buffer-11, undefined). The core
    // computes the saturating offset instead, which is the same observable
    // result: a character index in `0 ..= lastchar - buffer`.
    // SAFETY: `el` must be non-NULL.
    nshedit::chared::el_cursor(unsafe { &mut *el }, n)
}

// [spec:libedit:def:histedit.el-wline-fn]
// [spec:libedit:sem:histedit.el-wline-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wline(el: *mut EditLine) -> *const LineInfoW {
    // The C's whole body is `(const LineInfoW *)(void *)&el->el_line`: a
    // relabelling of the live editing state, which is why `el_wline(NULL)`
    // yields a small bogus pointer instead of faulting. That is undefined
    // behaviour and `sem:histedit.el-wline-fn` says not to reproduce it, so
    // `el` is dereferenced here and a NULL handle faults.
    //
    // `nshedit`'s `ElLineT` holds `cursor` and `lastchar` as offsets into an
    // owned `Vec<u32>`, so there is no `LineInfoW` in the core to point at.
    // The view is built from those offsets and kept in this crate, one per
    // editor, so the pointer stays valid until the next call or `el_end`.
    //
    // Divergence worth naming: the C's struct is *live*, and a caller that
    // stashes the pointer sees later edits through it. This one is refreshed
    // on each call. It cannot be live — the offsets have to be resolved
    // against a buffer that moves — and a stashed C pointer is invalidated by
    // the same buffer growth anyway.
    // SAFETY: `el` must be non-NULL.
    let el = unsafe { &mut *el };
    let key = (el as *mut EditLine) as usize;
    let buffer = el.el_line.buffer.as_ptr();
    let view = LineInfoW {
        buffer,
        // SAFETY: both offsets index the same live `buffer` allocation, and
        // `lastchar` is allowed to be one past the last character.
        cursor: unsafe { buffer.add(el.el_line.cursor) },
        lastchar: unsafe { buffer.add(el.el_line.lastchar) },
    };
    WLINE.with_borrow_mut(|m| {
        let slot = m.entry(key).or_insert_with(|| {
            Box::new(LineInfoW {
                buffer: core::ptr::null(),
                cursor: core::ptr::null(),
                lastchar: core::ptr::null(),
            })
        });
        **slot = view;
        core::ptr::from_ref::<LineInfoW>(&**slot)
    })
}

// [spec:libedit:def:histedit.el-winsertstr-fn]
// [spec:libedit:sem:histedit.el-winsertstr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_winsertstr(el: *mut EditLine, str_: *const u32) -> c_int {
    // A NULL string and an empty one are the same -1, so NULL becomes the
    // empty slice rather than a separate check.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    nshedit::chared::el_winsertstr(unsafe { &mut *el }, unsafe { wstr(str_) }.unwrap_or(&[]))
}

// [spec:libedit:def:histedit.el-wreplacestr-fn]
// [spec:libedit:sem:histedit.el-wreplacestr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wreplacestr(el: *mut EditLine, str_: *const u32) -> c_int {
    // As `el_winsertstr`: NULL and empty are both -1.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    nshedit::chared::el_wreplacestr(unsafe { &mut *el }, unsafe { wstr(str_) }.unwrap_or(&[]))
}

// [spec:libedit:def:histedit.history-winit-fn]
// [spec:libedit:sem:histedit.history-winit-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_winit() -> *mut HistoryW {
    // The handle is raw all the way through: `H_END` frees it from inside
    // `history_w`, which no borrow could express. NULL is the C's allocation
    // failure. The retained maximum starts at 0, so `H_SETSIZE` is required
    // before any `H_ENTER` keeps anything.
    nshedit::history::history_winit()
}

// [spec:libedit:def:histedit.history-wend-fn]
// [spec:libedit:sem:histedit.history-wend-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_wend(h: *mut HistoryW) {
    // `h` must be non-NULL; there is no check and calling it twice is a
    // double free. Every `HistEventW.str` from this handle is dangling
    // afterwards except those from `H_DEL`/`H_DELDATA`, which the caller owns.
    nshedit::history::history_wend(h);
}

/// C: `int history_w(HistoryW *, HistEventW *, int, ...);`
///
/// Declared with fixed arity — three named parameters plus the eleven
/// `H_FUNC` takes, the widest op — and read positionally. See the module
/// documentation for why that is correct on x86-64 SysV and AArch64 AAPCS and
/// wrong on AArch64 Apple.
///
/// The op enumeration below is the whole of this function: each code names
/// how many trailing arguments it has and what they are, which is what the
/// core's `HistoryArg` is a closed form of.
// [spec:libedit:def:histedit.history-w-fn]
// [spec:libedit:sem:histedit.history-w-fn]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn history_w(
    h: *mut HistoryW,
    ev: *mut HistEventW,
    op: c_int,
    a1: *mut c_void,
    a2: *mut c_void,
    a3: *mut c_void,
    a4: *mut c_void,
    a5: *mut c_void,
    a6: *mut c_void,
    a7: *mut c_void,
    a8: *mut c_void,
    a9: *mut c_void,
    a10: *mut c_void,
    a11: *mut c_void,
) -> c_int {
    use nshedit::histedit::{
        H_ADD, H_APPEND, H_CLEAR, H_CURR, H_DEL, H_DELDATA, H_END, H_ENTER, H_FIRST, H_FUNC,
        H_GETSIZE, H_GETUNIQUE, H_LAST, H_LOAD, H_NEXT, H_NEXT_EVDATA, H_NEXT_EVENT, H_NEXT_STR,
        H_NSAVE_FP, H_PREV, H_PREV_EVENT, H_PREV_STR, H_REPLACE, H_SAVE, H_SAVE_FP, H_SET,
        H_SETSIZE, H_SETUNIQUE,
    };

    // Neither `h` nor `ev` may be NULL; both are dereferenced unchecked, as
    // in the C.
    // SAFETY: `ev` is the caller's out-parameter.
    let ev = unsafe { &mut *ev };

    // An `int` vararg arrives in the low half of an argument slot; a `size_t`
    // fills one.
    let int_arg = |p: *mut c_void| p as usize as u32 as c_int;

    // Filenames are `const char *` in both instantiations. The core takes
    // `&str`, so a path that is not UTF-8 cannot be passed on; it is reported
    // as the op's failure rather than opened under a different name. See the
    // crate report — `history_load`/`history_save` want `&Path`.
    // SAFETY: `a1` is a NUL-terminated path for the two ops that use it.
    let path = || unsafe { cbytes(a1.cast::<c_char>()) }.and_then(|b| core::str::from_utf8(b).ok());

    let arg = match op {
        // `void *ptr` then ten function pointers: first, next, last, prev,
        // curr, set, clear, enter, add, del — eleven arguments, one more than
        // the manual documents. The C reads `ptr`, validates it, and never
        // stores it, so the installed functions are called with libedit's own
        // builtin state pointer (ERR-history-04, frozen).
        H_FUNC => core_gap("C-ABI `history` vtable typedefs (`HistoryGfunT` and friends)"),

        // One `int`.
        H_SETSIZE => HistoryArg::Num(int_arg(a1)),
        H_SET => HistoryArg::Num(int_arg(a1)),
        H_SETUNIQUE => HistoryArg::Num(int_arg(a1)),
        H_DEL => HistoryArg::Num(int_arg(a1)),
        H_NEXT_EVENT => HistoryArg::Num(int_arg(a1)),
        H_PREV_EVENT => HistoryArg::Num(int_arg(a1)),

        // No trailing argument. `H_CURR` included: the header comment's
        // `, const int)` is wrong.
        H_GETSIZE | H_FIRST | H_LAST | H_PREV | H_NEXT | H_CURR | H_END | H_CLEAR | H_GETUNIQUE => {
            HistoryArg::None
        }

        // One `const wchar_t *`.
        // SAFETY: `a1` is a NUL-terminated wide string for these ops.
        H_ADD | H_ENTER | H_APPEND | H_NEXT_STR | H_PREV_STR => {
            HistoryArg::Str(a1.cast::<u32>().cast_const())
        }

        // One `const char *` filename — narrow in both instantiations,
        // because the on-disk format is bytes and is frozen.
        H_LOAD | H_SAVE => match path() {
            Some(p) => HistoryArg::Path(p),
            None => return -1,
        },

        // One `FILE *`, which the caller keeps and must close.
        H_SAVE_FP => HistoryArg::Fp(a1),

        // `size_t n` then `FILE *`. `n` is passed through unchanged: the walk
        // takes `n` steps back from the newest and then writes forward, so it
        // emits **n + 1** entries and `n == 0` writes one (ERR-history-15).
        // Not corrected here.
        H_NSAVE_FP => HistoryArg::NSaveFp(a1 as usize, a2),

        // `int` then `void **`. The pointer stays raw: `H_DELDATA` accepts
        // the magic `(void **)-1` meaning "position the cursor only".
        H_NEXT_EVDATA | H_DELDATA => HistoryArg::EvData(int_arg(a1), a2.cast::<*mut c_void>()),

        // `const wchar_t *line` then `void *data`. It does not free the
        // string it overwrites, so every call leaks one (ERR-history-08),
        // and it reaches into the builtin state without checking that one is
        // installed. Both reproduced.
        H_REPLACE => HistoryArg::Replace(a1.cast::<u32>().cast_const(), a2),

        // Anything else reads no argument and comes back -1 with `ev` set to
        // code 1, "unknown error" — which the core's default arm does, so it
        // is dispatched rather than short-circuited here.
        _ => HistoryArg::None,
    };

    // Ownership on the way out, reproduced by the core and not touched here:
    // `H_DEL` and `H_DELDATA` hand the caller a string it owns and must free
    // (and which is NULL on an allocation failure); every other op's
    // `ev.str` points into libedit's storage or at a static message and must
    // not be freed.
    nshedit::history::history_w(h, ev, op, arg)
}

// [spec:libedit:def:histedit.tok-winit-fn]
// [spec:libedit:sem:histedit.tok-winit-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_winit(ifs: *const u32) -> *mut TokenizerW {
    // NULL selects the default `L"\t \n"`. The caller keeps ownership of its
    // string; the tokenizer owns everything it later hands back.
    // SAFETY: `ifs` is null or a NUL-terminated wide string.
    nshedit::tokenizer::tok_winit(unsafe { wstr(ifs) }).map_or(core::ptr::null_mut(), Box::into_raw)
}

// [spec:libedit:def:histedit.tok-wend-fn]
// [spec:libedit:sem:histedit.tok-wend-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_wend(tok: *mut TokenizerW) {
    // `tok` must be non-NULL (no check) and must be a `TokenizerW`; every
    // `argv` array and word pointer from this tokenizer dangles afterwards,
    // so the materialised array goes with it.
    TOKARGV.with_borrow_mut(|m| m.remove(&(tok as usize)));
    // SAFETY: `tok` is the handle `tok_winit` returned, i.e. that `Box`.
    nshedit::tokenizer::tok_wend(unsafe { Box::from_raw(tok) });
}

// [spec:libedit:def:histedit.tok-wreset-fn]
// [spec:libedit:sem:histedit.tok-wreset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_wreset(tok: *mut TokenizerW) {
    // Five assignments; nothing is freed and the grown capacities are kept.
    // In particular `argv[0]` is not restored to NULL, so a following parse
    // that publishes no word leaves the array unterminated (ERR-input-38).
    // SAFETY: `tok` must be non-NULL.
    nshedit::tokenizer::tok_wreset(unsafe { &mut *tok });
}

/// Materialise the `const wchar_t **` a successful `tok_wline`/`tok_wstr`
/// hands back.
///
/// The C's `*argv = tok->argv` aliases the tokenizer's own array; `nshedit`
/// stores offsets into `wspace` instead, so the pointer array is built here
/// and owned per tokenizer. It is replaced on every successful call, which is
/// the C's "invalidated by the next `tok_line`, `tok_str` or `tok_reset`".
///
/// Slot `argc` is materialised from whatever the tokenizer has there rather
/// than forced to NULL, so `tok_wreset`'s stale terminator survives into the
/// array exactly as it does in the C (ERR-input-38).
fn publish_argv(tok: &TokenizerW, argc: c_int) -> *mut *const u32 {
    let n = if argc > 0 { argc as usize } else { 0 };
    let base = tok.wspace.as_ptr();
    let mut out: Vec<*const u32> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let p = match tok.argv.get(i).copied().flatten() {
            // SAFETY: published slots are offsets into `wspace`, which is
            // where `base` points and which is at least that long.
            Some(off) => unsafe { base.add(off) },
            None => core::ptr::null(),
        };
        out.push(p);
    }
    TOKARGV.with_borrow_mut(|m| {
        let slot = m.entry(core::ptr::from_ref(tok) as usize).or_default();
        *slot = out;
        slot.as_mut_ptr()
    })
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
    // `tok` and `line` must be non-NULL and `line->buffer` must be non-NULL;
    // none is checked. The tokenizer is *not* reset — this appends, which is
    // how multi-line continuation works.
    // SAFETY: both are the caller's live objects.
    let tok = unsafe { &mut *tok };
    let line = unsafe { &*line };
    let mut n: c_int = 0;
    // `cursorc` and `cursoro` are NULL-checked in the C, so they are optional
    // here; `argc` is written unconditionally on success.
    // SAFETY: each is null or writable.
    let cc = if cursorc.is_null() {
        None
    } else {
        Some(unsafe { &mut *cursorc })
    };
    let co = if cursoro.is_null() {
        None
    } else {
        Some(unsafe { &mut *cursoro })
    };
    let rv = nshedit::tokenizer::tok_wline(tok, line, &mut n, cc, co);
    if rv != 0 {
        // On any non-zero return none of the four out-parameters is written.
        return rv;
    }
    let words = publish_argv(tok, n);
    // SAFETY: the success path writes both out-parameters, as in the C.
    unsafe {
        *argc = n;
        *argv = words;
    }
    0
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
    // A NUL-terminated string with no cursor: the core builds the `LineInfoW`
    // with `cursor == lastchar`, so the cursor never matches and both cursor
    // out-parameters are NULL. Does not reset the tokenizer either.
    // SAFETY: `tok` must be non-NULL, `line` non-NULL and NUL-terminated.
    let tok = unsafe { &mut *tok };
    let s = unsafe { wstr(line) }.unwrap_or(&[]);
    let mut n: c_int = 0;
    let rv = nshedit::tokenizer::tok_wstr(tok, s, &mut n);
    if rv != 0 {
        return rv;
    }
    let words = publish_argv(tok, n);
    // SAFETY: the success path writes both out-parameters, as in the C.
    unsafe {
        *argc = n;
        *argv = words;
    }
    0
}
