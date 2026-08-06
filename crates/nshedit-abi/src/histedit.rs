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
//! `...` functions in C, and the four defined here are `...` functions in
//! Rust. Each declares exactly the leading parameters `histedit.h` declares
//! and walks its tail through a [`core::ffi::VaList`], reading the arguments
//! its `sem` rule enumerates for the selected op — in order, and no others.
//! That is what the C's `va_arg` chain does, so an op that reads two
//! arguments reads two here and one that reads none reads none.
//!
//! The exported function only forwards. The per-op work lives in an ordinary
//! function taking the `VaList` by value — `el_wset_va`, `el_wget_va`,
//! [`history_dispatch`] — which is what lets the narrow and wide halves of
//! `history` share one dispatch, and keeps the arm bodies out of a variadic
//! frame.
//!
//! This replaces a fixed-arity shim: enough trailing `*mut c_void` parameters
//! for the widest op, read positionally. That shim was correct on **x86-64
//! System V** and **AArch64 AAPCS**, where a variadic call places its
//! arguments in the same registers and stack slots a fixed-arity call of the
//! same shape would, and **wrong on AArch64 Apple**
//! (`arm64-apple-darwin`), whose convention passes *every* variadic argument
//! on the stack, one 8-byte slot each, immediately after the named
//! parameters — so the callee read the caller's `a1..a19` out of x2..x7 and
//! never looked at the stack. Silent corruption rather than a link error.
//! Real variadics leave no target where caller and callee disagree about
//! where an argument lives.
//!
//! One consequence is a defect coming back rather than going away, and it is
//! deliberate. A caller that omits the NULL sentinel from `EL_BIND` and its
//! four neighbours makes the collection loop read past the end of its own
//! argument list — undefined behaviour, ERR-core-api-07's other half, which
//! the fixed arity had defined away by construction.
//! `plan/decisions/conformance-policy.md` reproduces the C's defined defects
//! rather than repairing them, and a genuine `...` reproduces this one for
//! free — the shim had been accidentally safer than what it imitates.
//!
//! # The narrow and wide halves
//!
//! `history.c` and `tokenizer.c` are each compiled twice in the C, so each
//! declares two families here: `history_winit`/`history_wend`/`history_w` and
//! `tok_w*` over `wchar_t`, `history_init`/`history_end`/`history` and `tok_*`
//! over `char`. `nshedit` carries both instantiations of one generic source
//! (`nshedit::history::HistChar`, `nshedit::tokenizer::TokChar`), so the
//! bodies here are pairs too — [`history_dispatch`] and [`argv_ptrs`] are the
//! shared halves, and the ten exported functions differ only in which
//! character type they pin.
//!
//! Nothing in this module is blocked on the core any more. The two causes that
//! used to abort here are both closed: the narrow instantiations exist, and
//! `el_wset(EL_SETFP)`'s facility over a caller's opaque `FILE *` is
//! [`crate::cstdio`], the third site `plan/decisions/no-c-ffi.md` enumerates.

use core::ffi::{VaList, c_char, c_int, c_uchar, c_void};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;

use nshedit::chared::{ElAfuncT, ElZfuncT};
use nshedit::el::{CFile, FuncT};
use nshedit::hist::HistFunT;
use nshedit::histedit::{
    EditLine, ElRfuncT, HistEvent, History, HistoryW, LineInfo, Tokenizer, TokenizerW,
};
use nshedit::history::{
    HistoryArg, HistoryEfunT, HistoryGfunT, HistorySfunT, HistoryVfunT, SaveStream,
};
use nshedit::map::ElFuncT;
use nshedit::prompt::ElPfuncT;

// Renamed on import so the signatures below read as `histedit.h` writes
// them; see the note on `LineInfoWide`.
use crate::cdecl::histedit::{HistEventWide as HistEventW, LineInfoWide as LineInfoW, wchar_t};
use crate::cstdio::{self, CFileWriter};

// ---------------------------------------------------------------------------
// `el_set`/`el_get` operation codes. C: `histedit.h`, which defines them as
// untyped `#define`s carrying no rule of their own. They live here and not in
// `nshedit::histedit` because they exist only to select an arm of the varargs
// dispatch, and `plan/decisions/idiomatic-core.md` puts that dispatch in this
// crate. The numbering is ABI: a consumer passes these integers directly.
//
// `pub` because the shipped `histedit.h` is generated from them. They must
// stay `#define`s in that header and not an `enum`: consumers write
// `#ifdef EL_PROMPT`, which an enumerator would silently answer no to.
//
// The doc line on each is the argument list, and it is what `histedit.h`
// prints in a trailing comment beside every opcode. Under `el_wset`/`el_wget`
// `Char` is `wchar_t` and `prompt_func` is `el_wpfunc_t`; under
// `el_set`/`el_get` they are `char` and `el_pfunc_t`. The types shown are for
// "set"; for "get" each is a pointer to it, so `EL_EDITMODE` takes an `int`
// set and an `int *` get. Ops that only get are marked so.
//
// TWO OF THESE ANNOTATIONS ARE CORRECTED, not copied. ERR-core-api-34 records
// both, with the disposition "fix the documentation", and its status has been
// stuck at partial because the port shipped no header to carry the fix:
//
//   - EL_ADDFN's second argument is a help STRING in both APIs, where
//     `histedit.h` annotates it `const Char`, a single character.
//   - EL_GETTC's argument is `char *` in both APIs, where `histedit.h`
//     annotates it `const Char *` — so under the wide entry point the
//     original's annotation is wrong twice over.
//
// The rest of that entry is about the `H_*` opcodes, whose constants live in
// `nshedit::histedit` and are annotated there or not at all.
// ---------------------------------------------------------------------------

/// `, prompt_func);` — set/get. The prompt callback.
pub const EL_PROMPT: c_int = 0;
/// `, const char *);` — set/get. The terminal type.
pub const EL_TERMINAL: c_int = 1;
/// `, const Char *);` — set/get. `"emacs"` or `"vi"`.
pub const EL_EDITOR: c_int = 2;
/// `, int);` — set/get. Whether libedit installs signal handlers.
pub const EL_SIGNAL: c_int = 3;
/// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
pub const EL_BIND: c_int = 4;
/// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
pub const EL_TELLTC: c_int = 5;
/// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
pub const EL_SETTC: c_int = 6;
/// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
pub const EL_ECHOTC: c_int = 7;
/// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
pub const EL_SETTY: c_int = 8;
/// `, const Char *name, const Char *help, el_func_t);` — set. `help` is a STRING, not the single `const Char` `histedit.h` annotates: ERR-core-api-34.
pub const EL_ADDFN: c_int = 9;
/// `, hist_fun_t, const void *);` — set. The history callback and its cookie.
pub const EL_HIST: c_int = 10;
/// `, int);` — set/get. Zero makes `el_gets` bypass the editing loop.
pub const EL_EDITMODE: c_int = 11;
/// `, prompt_func);` — set/get. The right-hand prompt callback.
pub const EL_RPROMPT: c_int = 12;
/// `, el_rfunc_t);` — set/get. `EL_BUILTIN_GETCFN` restores the default.
pub const EL_GETCFN: c_int = 13;
/// `, void *);` — set/get. Application data, stored and handed back.
pub const EL_CLIENTDATA: c_int = 14;
/// `, int);` — set/get. Return each character as it arrives.
pub const EL_UNBUFFERED: c_int = 15;
/// `, int);` — set. Put the terminal in or out of editing mode.
pub const EL_PREP_TERM: c_int = 16;
/// `, char *, ..., NULL);` — get only. `char *` in BOTH APIs, not the `const Char *` `histedit.h` annotates: ERR-core-api-34.
pub const EL_GETTC: c_int = 17;
/// `, int, FILE **);` — get only. The stream for one of the three fds.
pub const EL_GETFP: c_int = 18;
/// `, int, FILE *);` — set. The stream for one of the three fds.
pub const EL_SETFP: c_int = 19;
/// `, void);` — set. Redraw the line.
pub const EL_REFRESH: c_int = 20;
/// `, prompt_func, Char);` — set/get. Prompt, plus its literal-run marker.
pub const EL_PROMPT_ESC: c_int = 21;
/// `, prompt_func, Char);` — set/get. As `EL_PROMPT_ESC`, right-hand side.
pub const EL_RPROMPT_ESC: c_int = 22;
/// `, el_zfunc_t, void *);` — set. The window-size-change callback.
pub const EL_RESIZE: c_int = 23;
/// `, el_afunc_t, void *);` — set. The line-alias callback.
pub const EL_ALIAS_TEXT: c_int = 24;
/// `, int);` — set/get. Restart reads interrupted by a signal.
pub const EL_SAFEREAD: c_int = 25;
/// `, const Char *);` — set/get. The word-constituent character set.
pub const EL_WORDCHARS: c_int = 26;
/// `, char *(*func)(const char *));` — set/get. The environment accessor.
pub const EL_GETENV: c_int = 27;

// `el_flags` bits. C: `el.h`. `nshedit::el` declares the same constants
// `pub(crate)`, so they are restated here rather than imported; the ops that
// read and write them are inline in the C's own `el_wset`/`el_wget` bodies,
// which is why this crate touches `el_flags` at all.
const HANDLE_SIGNALS: i32 = 0x001;
const EDIT_DISABLED: i32 = 0x004;
const UNBUFFERED: i32 = 0x008;
/// Cleared — never set — by `el_wset(EL_HIST)`, and only in a single-byte
/// locale. The narrow `el_set(EL_HIST)` in [`crate::eln`] is the one place it
/// is set. ERR-core-api-16.
const NARROW_HISTORY: i32 = 0x040;
/// The bit `el_wget(EL_SAFEREAD)` stores raw — 256, not 1. See
/// `sem:histedit.el-wget-fn`.
const FIXIO: i32 = 0x100;

// ---------------------------------------------------------------------------
// Wide literals. The C spells these `L"..."` inline; here they are `[u32]`,
// and the two that cross the ABI carry the terminating NUL a C caller reads to
// while the rest, which are consumed by `nshedit` as slices, do not.
// ---------------------------------------------------------------------------

/// The C's `L"..."` for an ASCII literal.
const fn wide<const N: usize>(s: &[u8; N]) -> [u32; N] {
    let mut out = [0u32; N];
    let mut i = 0;
    while i < N {
        out[i] = s[i] as u32;
        i += 1;
    }
    out
}

/// `argv[0]` for the five list ops, which is what their diagnostics print.
static BIND: [u32; 4] = wide(b"bind");
static TELLTC: [u32; 6] = wide(b"telltc");
static SETTC: [u32; 5] = wide(b"settc");
static ECHOTC: [u32; 6] = wide(b"echotc");
static SETTY: [u32; 5] = wide(b"setty");

/// C: `static char name[] = "gettc"` — `el_wget(EL_GETTC)`'s `argv[0]`.
/// `terminal_gettc` never reads it; it is built because the C builds it.
static GETTC: [u8; 6] = *b"gettc\0";

/// The two answers `el_wget(EL_EDITOR)` hands out. `sem:map.map-get-editor-fn`
/// makes them process-lifetime statics the caller must not free, so they are
/// statics here rather than anything materialised per call — `nshedit`'s own
/// `EDITOR_EMACS`/`EDITOR_VI` are Rust slices and carry no terminator.
static EDITOR_EMACS: [u32; 6] = wide(b"emacs\0");
static EDITOR_VI: [u32; 3] = wide(b"vi\0");

// ---------------------------------------------------------------------------
// Helpers: the C-shaped plumbing this crate exists to own.
// ---------------------------------------------------------------------------

/// C: `va_arg(ap, <some function pointer type>)`.
///
/// `Option<F>` for a function-pointer `F` has the same size and the same
/// null representation as a data pointer, so the read is a reinterpretation
/// and not a conversion: a NULL slot becomes `None`, which is how every op
/// here spells "the caller passed no function".
///
/// # Safety
/// `F` must be a function-pointer type, and the slot must really hold a
/// function of that exact signature — which is the op's own contract with the
/// caller, and undefined in the C too when it is broken.
pub(crate) unsafe fn fn_arg<F: Copy>(ap: &mut VaList<'_>) -> Option<F> {
    assert_eq!(size_of::<Option<F>>(), size_of::<*mut c_void>());
    // SAFETY: the caller's contract, above.
    let p = unsafe { ap.next_arg::<*mut c_void>() };
    // SAFETY: the size is checked above and the caller guarantees `F` is a
    // function-pointer type, whose `Option` is null-niche optimised.
    unsafe { core::mem::transmute_copy::<*mut c_void, Option<F>>(&p) }
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

/// [`cbytes`] as the narrow instantiation's `Char`.
///
/// The narrow tokenizer's element type is `c_char`, not `u8` — it is the C's
/// `char` — so the two entry points that hand it a caller's string need the
/// slice at that type rather than as bytes.
///
/// # Safety
/// As [`cbytes`].
unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a [c_char]> {
    // SAFETY: the caller's contract, forwarded.
    let b = unsafe { cbytes(p) }?;
    // SAFETY: `c_char` and `u8` have the same size and alignment and every
    // bit pattern is valid for both; this relabels the same bytes.
    Some(unsafe { core::slice::from_raw_parts(b.as_ptr().cast::<c_char>(), b.len()) })
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
    // `tok_line`/`tok_str`'s `argv`, for the narrow tokenizer — the same job
    // as `TOKARGV` over `const char *` instead of `const wchar_t *`. Two maps
    // rather than one because the value type differs, which also means the two
    // handle families cannot be confused by a shared key space.
    static TOKARGV_N: RefCell<HashMap<usize, Vec<*const c_char>>> = RefCell::new(HashMap::new());
    // `el_wget(EL_TERMINAL)`'s `const char *`. The C hands out
    // `el_terminal.t_name`, a borrowed `char *` a later `terminal_set`
    // replaces; `nshedit` keeps a `String`, which has no terminator, so the
    // NUL-terminated copy is made here and owned per editor.
    static TERMNAME: RefCell<HashMap<usize, CString>> = RefCell::new(HashMap::new());
    // `el_wget(EL_WORDCHARS)`'s `const wchar_t *`, for the same reason:
    // `nshedit::map::map_get_wordchars` hands back an owned `Vec<u32>` with no
    // terminator, and the C hands back its own buffer.
    static WORDCHARS: RefCell<HashMap<usize, Vec<u32>>> = RefCell::new(HashMap::new());
    // [`default_getenv`]'s last answer, kept alive until the next call through
    // it. That is `getenv(3)`'s own contract and is at least as strong as the
    // one `def:el.editline.el-getenv-fn` puts on the hook.
    static GETENV_VALUE: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// The C's `const wchar_t *argv[20]` for `EL_BIND`, `EL_TELLTC`, `EL_SETTC`,
/// `EL_ECHOTC` and `EL_SETTY`, collected out of the varargs tail.
///
/// C: `for (i = 1; i < 20; i++) if ((argv[i] = va_arg(ap, wchar_t *)) == NULL)
/// break;` — so at most nineteen arguments are read, `cmd` becomes `argv[0]`,
/// and the returned length *is* the C's `i`, the `argc` it passes on. That
/// count is what every one of the five handlers ignores; `sem:map.map-bind-fn`
/// records that `map_bind` overwrites it with 1 and iterates to the terminator
/// instead (ERR-modes-27), which is why the vector is cut at the NULL rather
/// than at the caller's count.
///
/// ERR-core-api-07, disposition `define — always terminate`: with all
/// nineteen slots non-NULL the C's array is full, unterminated, and passed
/// with `argc == 20`, and a handler scanning for the terminator then reads a
/// twenty-first element that does not exist. A `Vec` ends where it ends, so
/// that over-read cannot arise; the definition is that the list is always
/// terminated. The *other* half of the same defect is reproduced and not
/// defined away: a caller that omits the sentinel with fewer than nineteen
/// arguments makes this loop read past the end of its own argument list,
/// exactly as the C does.
///
/// # Safety
/// The tail holds a NULL-terminated list of wide strings, each of which
/// outlives the call — the op's contract with the caller, and undefined in the
/// C too when it is broken.
unsafe fn list_args<'a>(cmd: &'a [u32], ap: &mut VaList<'_>) -> Vec<&'a [u32]> {
    const ARGV_LEN: usize = 20;
    let mut argv: Vec<&[u32]> = Vec::with_capacity(ARGV_LEN);
    argv.push(cmd);
    for _ in 1..ARGV_LEN {
        // SAFETY: the caller's contract, above.
        let p = unsafe { ap.next_arg::<*mut c_void>() };
        // SAFETY: a non-NULL slot is a NUL-terminated wide string.
        let Some(s) = (unsafe { wstr(p.cast::<u32>()) }) else {
            break;
        };
        argv.push(s);
    }
    argv
}

/// C: `el->el_getenv = secure_getenv` — the accessor `el_init` installs, and
/// therefore the address `el_get(EL_GETENV)` reports for a handle nobody has
/// called `el_set(EL_GETENV, ...)` on.
///
/// `sem:el.editline.el-getenv-fn` fixes both ends: `el_init_internal` sets
/// the hook to `secure_getenv`, and `el_get(el, EL_GETENV, &fn)` reads it
/// back. But `sem:el.secure-getenv-fn` has `nshedit::el::secure_getenv`
/// return an owned `OsString`, which cannot itself be a `func_t`, so the core
/// stores `None` for "no application hook" and calls it directly — and its
/// note on `el_getenv` assigns the missing address to this crate. This is it:
/// the C-callable face of the same lookup, for the one purpose of having
/// something to hand out.
///
/// # Safety
/// `name` must be NULL or a NUL-terminated byte string, as `getenv(3)`
/// requires.
unsafe extern "C" fn default_getenv(name: *const c_char) -> *mut c_char {
    // SAFETY: the caller's contract, above.
    let Some(bytes) = (unsafe { cbytes(name) }) else {
        return core::ptr::null_mut();
    };
    // The core takes `&str`. A name that is not UTF-8 is none of the four
    // libedit ever looks up, and is reported unset rather than guessed at.
    let Ok(name) = core::str::from_utf8(bytes) else {
        return core::ptr::null_mut();
    };
    let Some(value) = nshedit::el::secure_getenv(name) else {
        return core::ptr::null_mut();
    };
    // An environment value cannot contain a NUL, so this cannot fail; a
    // failure would be reported as unset regardless.
    let Ok(value) = CString::new(value.into_vec()) else {
        return core::ptr::null_mut();
    };
    // The previous answer is dropped here, which is exactly the invalidation
    // point the contract names.
    GETENV_VALUE.with_borrow_mut(|slot| slot.insert(value).as_ptr().cast_mut())
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
    // `sem:el.el-init-fn`: the entire body is one tail call to `el_init_fd`
    // with `fileno` of each stream. The `fileno` is this crate's, not the
    // core's — a `FILE *` is the C library's own object and only this crate
    // may reach into it (`plan/decisions/no-c-ffi.md`), which is exactly what
    // `nshedit::el::el_init`'s own documentation asks the ABI to do. The
    // evaluation order of the three calls is unspecified in C and has no
    // observable consequence; left to right here.
    nshedit::el::el_init_fd(
        prog,
        fin,
        fout,
        ferr,
        cstdio::fileno_of(fin),
        cstdio::fileno_of(fout),
        cstdio::fileno_of(ferr),
    )
    .map_or(core::ptr::null_mut(), Box::into_raw)
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
    // Every pointer this handle ever handed out is dangling after this — the
    // `el_wline` view and the two `el_wget` copies included — so drop ours too.
    WLINE.with_borrow_mut(|m| m.remove(&(el as usize)));
    TERMNAME.with_borrow_mut(|m| m.remove(&(el as usize)));
    WORDCHARS.with_borrow_mut(|m| m.remove(&(el as usize)));
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
    // `History` is `history.c` compiled with `Char = char`: a separate store
    // from the wide one, with byte strings throughout and no locale anywhere
    // in it. As `history_winit`, the handle is raw all the way through —
    // `H_END` frees it from inside `history` — and NULL is the C's allocation
    // failure. The retained maximum starts at 0, so `H_SETSIZE` is required
    // before any `H_ENTER` keeps anything.
    nshedit::history::history_init()
}

// [spec:libedit:def:histedit.history-end-fn]
// [spec:libedit:sem:histedit.history-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_end(h: *mut History) {
    // `h` must be non-NULL; there is no check and calling it twice is a double
    // free. Every `HistEvent.str` from this handle is dangling afterwards
    // except those from `H_DEL`/`H_DELDATA`, which the caller owns.
    nshedit::history::history_end(h);
}

/// C: `int history(History *, HistEvent *, int, ...);`
///
/// The tail is walked by [`history_dispatch`], which is the same table
/// `history_w` uses with `Char` set to `char`.
// [spec:libedit:def:histedit.history-fn]
// [spec:libedit:sem:histedit.history-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history(h: *mut History, ev: *mut HistEvent, op: c_int, ap: ...) -> c_int {
    // Same op codes, argument shapes, error codes and ownership rules as
    // `history_w` — `sem:histedit.history-fn` is the rule `history_w`'s is
    // written against — but over a byte store, so `H_ADD`, `H_ENTER`,
    // `H_APPEND`, `H_NEXT_STR`, `H_PREV_STR` and `H_REPLACE` take
    // `const char *` and `ev.str` comes back as one. `H_LOAD`/`H_SAVE` are
    // unchanged: the path was already narrow in both instantiations, and so
    // was the file.
    // SAFETY: this function's own contract, forwarded unchanged.
    unsafe { history_dispatch::<c_char>(h, ev, op, ap) }
}

// [spec:libedit:def:histedit.tok-init-fn]
// [spec:libedit:sem:histedit.tok-init-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_init(ifs: *const c_char) -> *mut Tokenizer {
    // `Tokenizer` is `tokenizer.c` compiled with `Char = char`: a byte word
    // space, byte `argv` slots and a byte IFS. NULL selects the default
    // `"\t \n"`; the caller keeps ownership of its string, and the tokenizer
    // owns everything it later hands back.
    // SAFETY: `ifs` is null or a NUL-terminated byte string.
    nshedit::tokenizer::tok_init(unsafe { cstr(ifs) }).map_or(core::ptr::null_mut(), Box::into_raw)
}

// [spec:libedit:def:histedit.tok-end-fn]
// [spec:libedit:sem:histedit.tok-end-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_end(tok: *mut Tokenizer) {
    // `tok` must be non-NULL (no check) and must be a `Tokenizer`; every
    // `argv` array and word pointer from this tokenizer dangles afterwards,
    // so the materialised array goes with it.
    TOKARGV_N.with_borrow_mut(|m| m.remove(&(tok as usize)));
    // SAFETY: `tok` is the handle `tok_init` returned, i.e. that `Box`.
    nshedit::tokenizer::tok_end(unsafe { Box::from_raw(tok) });
}

// [spec:libedit:def:histedit.tok-reset-fn]
// [spec:libedit:sem:histedit.tok-reset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_reset(tok: *mut Tokenizer) {
    // Five assignments; nothing is freed and the grown capacities are kept.
    // In particular `argv[0]` is not restored to NULL, so a following parse
    // that publishes no word leaves the array unterminated (ERR-input-38).
    // SAFETY: `tok` must be non-NULL.
    nshedit::tokenizer::tok_reset(unsafe { &mut *tok });
}

/// [`publish_argv`]'s narrow twin — see there for why the array is
/// materialised at all.
fn publish_argv_n(tok: &Tokenizer, argc: c_int) -> *mut *const c_char {
    let out = argv_ptrs(tok, argc);
    TOKARGV_N.with_borrow_mut(|m| {
        let slot = m.entry(core::ptr::from_ref(tok) as usize).or_default();
        *slot = out;
        slot.as_mut_ptr()
    })
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
    // `tok` and `line` must be non-NULL and `line->buffer` must be non-NULL;
    // none is checked. The tokenizer is *not* reset — this appends, which is
    // how multi-line continuation works. Everything else is `tok_wline`'s
    // text: the same quoting machine over `char` instead of `wchar_t`, so a
    // multibyte character is several elements here and one there, and neither
    // consults the locale to decide.
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
    let rv = nshedit::tokenizer::tok_line(tok, line, &mut n, cc, co);
    if rv != 0 {
        // On any non-zero return none of the four out-parameters is written.
        return rv;
    }
    let words = publish_argv_n(tok, n);
    // SAFETY: the success path writes both out-parameters, as in the C.
    unsafe {
        *argc = n;
        *argv = words;
    }
    0
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
    // A NUL-terminated string with no cursor: the core builds the `LineInfo`
    // with `cursor == lastchar`, so the cursor never matches and both cursor
    // out-parameters are NULL. Does not reset the tokenizer either.
    // SAFETY: `tok` must be non-NULL, `line` non-NULL and NUL-terminated.
    let tok = unsafe { &mut *tok };
    let s = unsafe { cstr(line) }.unwrap_or(&[]);
    let mut n: c_int = 0;
    let rv = nshedit::tokenizer::tok_str(tok, s, &mut n);
    if rv != 0 {
        return rv;
    }
    let words = publish_argv_n(tok, n);
    // SAFETY: the success path writes both out-parameters, as in the C.
    unsafe {
        *argc = n;
        *argv = words;
    }
    0
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
pub unsafe extern "C" fn el_wgets(el: *mut EditLine, nread: *mut c_int) -> *const wchar_t {
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
pub unsafe extern "C" fn el_wgetc(el: *mut EditLine, wc: *mut wchar_t) -> c_int {
    // Returned verbatim, including the 0 the core reports when `tty_rawmode`
    // fails — a terminal-setup failure indistinguishable from end of file
    // (ERR-input-24). Not corrected to -1 here.
    // SAFETY: `el` and `wc` must both be non-NULL.
    nshedit::read::el_wgetc(unsafe { &mut *el }, unsafe { &mut *wc })
}

// [spec:libedit:def:histedit.el-wpush-fn]
// [spec:libedit:sem:histedit.el-wpush-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wpush(el: *mut EditLine, str_: *const wchar_t) {
    // A NULL string, a full stack or a failed duplication are all reported to
    // the user as a beep and to the caller not at all.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    nshedit::read::el_wpush(unsafe { &mut *el }, unsafe { wstr(str_) });
}

// [spec:libedit:def:histedit.el-wparse-fn]
// [spec:libedit:sem:histedit.el-wparse-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wparse(
    el: *mut EditLine,
    argc: c_int,
    argv: *mut *const wchar_t,
) -> c_int {
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
/// Only the arguments the selected op defines are ever read; supplying fewer
/// or differently typed arguments for an op stays the caller's undefined
/// behaviour, as in the C.
// [spec:libedit:def:histedit.el-wset-fn]
// [spec:libedit:sem:histedit.el-wset-fn]
// [spec:libedit:def:el.el-wset-fn]
// [spec:libedit:sem:el.el-wset-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wset(el: *mut EditLine, op: c_int, ap: ...) -> c_int {
    // A NULL editor is rejected before the tail is started, as in the C: the
    // check sits above `va_start`.
    if el.is_null() {
        return -1;
    }
    // SAFETY: `el` is non-null and is the caller's live handle, and the tail
    // carries what `op` says it carries.
    unsafe { el_wset_va(&mut *el, op, ap) }
}

/// [`el_wset`]'s dispatch, out of the variadic frame.
///
/// # Safety
/// The tail must carry the arguments the selected `op` defines, in order.
pub(crate) unsafe fn el_wset_va(el: &mut EditLine, op: c_int, mut ap: VaList<'_>) -> c_int {
    match op {
        // One `el_pfunc_t`. `prompt_set(el, p, 0, op, 1)`: installs the left
        // prompt for EL_PROMPT and the right otherwise, marks it wide, resets
        // the cached prompt position, and restores the built-in default for a
        // NULL function. Always 0.
        //
        // The escape character is passed as 0 unconditionally, so this op
        // erases one installed earlier through the `_ESC` form
        // (ERR-core-api-36, reproduced in `prompt_set` itself).
        EL_PROMPT | EL_RPROMPT => {
            // SAFETY: the op's argument is an `el_pfunc_t`, per the header.
            let p = unsafe { fn_arg::<ElPfuncT>(&mut ap) };
            nshedit::prompt::prompt_set(el, p, 0, op, 1)
        }

        // An `el_pfunc_t` then an `int` narrowed to `wchar_t`: the literal
        // escape character bracketing zero-width prompt runs. Always 0. Note
        // `prompt_set` does treat EL_PROMPT_ESC as the left prompt, which is
        // the half of the asymmetry `prompt_get` gets wrong.
        EL_PROMPT_ESC | EL_RPROMPT_ESC => {
            // SAFETY: as above.
            let p = unsafe { fn_arg::<ElPfuncT>(&mut ap) };
            // C: `(wchar_t)c` on an `int` vararg — the low 32 bits kept and
            // reinterpreted, which is what the core's `u32` holds.
            // SAFETY: the op's second argument is an `int`.
            let c = unsafe { ap.next_arg::<c_int>() };
            nshedit::prompt::prompt_set(el, p, c as u32, op, 1)
        }

        // An `el_zfunc_t` then a `void *`: the resize callback and its
        // cookie. Always 0. Invoked from `el_resize`, from buffer growth and
        // from `el_line`.
        EL_RESIZE => {
            // SAFETY: the op's first argument is an `el_zfunc_t`, per the
            // header; the second is an opaque cookie stored verbatim.
            let p = unsafe { fn_arg::<ElZfuncT>(&mut ap) };
            let arg = unsafe { ap.next_arg::<*mut c_void>() };
            nshedit::chared::ch_resizefun(el, p, arg)
        }

        // An `el_afunc_t` then a `void *`: the alias-expansion callback and
        // its cookie — narrow `char` even in the wide API. Always 0.
        EL_ALIAS_TEXT => {
            // SAFETY: the op's first argument is an `el_afunc_t`; the second
            // is an opaque cookie stored verbatim.
            let p = unsafe { fn_arg::<ElAfuncT>(&mut ap) };
            let arg = unsafe { ap.next_arg::<*mut c_void>() };
            nshedit::chared::ch_aliasfun(el, p, arg)
        }

        // One `char *` terminal type, bytes even here. NULL means `$TERM`
        // through the environment hook; `"emacs"` additionally sets
        // EDIT_DISABLED. 0, or -1 if the display arrays could not be grown —
        // and also -1 whenever the capability lookup failed, however usable
        // the dumb-terminal fallback it then installed is (ERR-terminal-22,
        // reproduced by `terminal_set` and propagated here; both
        // `sem:histedit.el-wset-fn` and `sem:el.el-wset-fn` say so).
        EL_TERMINAL => {
            // SAFETY: the op's argument is null or a NUL-terminated byte
            // string.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            let bytes = unsafe { cbytes(p.cast::<c_char>()) };
            // The core takes `&str`. A type name that is not UTF-8 is passed
            // on lossily rather than rejected — unlike `el_init`'s program
            // name and `H_LOAD`'s filename, which fail the call. The reason
            // is that this string is only ever a lookup key: the C's own
            // outcome for a name the terminfo database has no entry for is
            // the diagnostic, the hardcoded dumb terminal and -1, and running
            // that path is closer than refusing to configure anything.
            let name = bytes.map(String::from_utf8_lossy);
            nshedit::terminal::terminal_set(el, name.as_deref())
        }

        // One `wchar_t *`: `L\"emacs\"` or `L\"vi\"`, anything else -1. Also
        // resets the word-character set to the map's default.
        EL_EDITOR => {
            // ERR-core-api-08, disposition `define — reject NULL`: the C
            // hands the argument straight to `wcscmp`, which dereferences it.
            // SAFETY: the op's argument is null or a NUL-terminated wide
            // string.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            match unsafe { wstr(p.cast::<u32>()) } {
                Some(e) => nshedit::map::map_set_editor(el, e),
                None => -1,
            }
        }

        // One `int`. Inline in the C's own dispatch, so inline here.
        EL_SIGNAL => {
            // SAFETY: the op's argument is an `int`.
            if unsafe { ap.next_arg::<c_int>() } != 0 {
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
        // non-NULL strings leave the C's array unterminated with `argc == 20`,
        // which handlers that scan for a terminator then read past
        // (ERR-modes-27, and ERR-core-api-07 for this collection loop's own
        // half of it) — defined away by [`list_args`], which always
        // terminates. The handler's 0/-1 is the result.
        EL_BIND | EL_TELLTC | EL_SETTC | EL_ECHOTC | EL_SETTY => {
            let cmd: &[u32] = match op {
                EL_BIND => &BIND,
                EL_TELLTC => &TELLTC,
                EL_SETTC => &SETTC,
                EL_ECHOTC => &ECHOTC,
                // The C's inner `default` is an `EL_ABORT` the outer match
                // makes unreachable; this arm is EL_SETTY.
                _ => &SETTY,
            };
            // SAFETY: the tail is a NULL-terminated list of wide strings the
            // caller keeps alive across the call.
            let argv = unsafe { list_args(cmd, &mut ap) };
            // The C's `i`: where the scan stopped, not how many strings the
            // caller passed. Every handler ignores it.
            let argc = argv.len() as c_int;
            match op {
                EL_BIND => nshedit::map::map_bind(el, argc, &argv),
                EL_TELLTC => nshedit::terminal::terminal_telltc(el, argc, &argv),
                EL_SETTC => nshedit::terminal::terminal_settc(el, argc, &argv),
                EL_ECHOTC => nshedit::terminal::terminal_echotc(el, argc, &argv),
                _ => nshedit::tty::tty_stty(el, argc, &argv),
            }
        }

        // `wchar_t *name`, `wchar_t *help`, `el_func_t`. -1 if any is NULL or
        // either table reallocation fails, else 0. Both strings are
        // duplicated, so the caller keeps its own.
        EL_ADDFN => {
            // The C's three NULL checks, which `map_addfunc` cannot make: a
            // `&[u32]` is never null and `ElFuncT` is not nullable. All three
            // arguments are read before any is checked, as in the C.
            // SAFETY: the first two are null or NUL-terminated wide strings;
            // the third is an `el_func_t`, per the header.
            let name = unsafe { wstr(ap.next_arg::<*mut c_void>().cast::<u32>()) };
            let help = unsafe { wstr(ap.next_arg::<*mut c_void>().cast::<u32>()) };
            let func = unsafe { fn_arg::<ElFuncT>(&mut ap) };
            let (Some(name), Some(help), Some(func)) = (name, help, func) else {
                return -1;
            };
            nshedit::map::map_addfunc(el, name, help, func)
        }

        // A `hist_fun_t` then its `void *` handle. Then, and only when
        // `MB_CUR_MAX == 1`, clear NARROW_HISTORY — so a narrow
        // `el_set(EL_HIST, ...)` is not undone in a multibyte locale
        // (ERR-core-api-16). Always 0; libedit does not own the handle.
        EL_HIST => {
            // SAFETY: the op's first argument is a `hist_fun_t`, per the
            // header; the second is the opaque handle it is called with.
            let f = unsafe { fn_arg::<HistFunT>(&mut ap) };
            let ptr = unsafe { ap.next_arg::<*mut c_void>() };
            // ERR-history-04, defined by the core: a NULL function with a
            // non-NULL handle is -1 here rather than the C's armed NULL
            // indirect call. Every other combination is the C's 0.
            let rv = nshedit::hist::hist_set(el, f, ptr);
            // The flag clear is not conditional on `rv` in the C either.
            if nshedit::el::mb_cur_max() == 1 {
                el.el_flags &= !NARROW_HISTORY;
            }
            rv
        }

        // One `int`, the EINTR-recovery flag. Inline in the C.
        EL_SAFEREAD => {
            // SAFETY: the op's argument is an `int`.
            if unsafe { ap.next_arg::<c_int>() } != 0 {
                el.el_flags |= FIXIO;
            } else {
                el.el_flags &= !FIXIO;
            }
            0
        }

        // One `int`, inverted: non-zero enables editing. Inline in the C.
        EL_EDITMODE => {
            // SAFETY: the op's argument is an `int`.
            if unsafe { ap.next_arg::<c_int>() } != 0 {
                el.el_flags &= !EDIT_DISABLED;
            } else {
                el.el_flags |= EDIT_DISABLED;
            }
            0
        }

        // One `el_rfunc_t`; `EL_BUILTIN_GETCFN` (NULL) restores the builtin.
        // Always 0.
        EL_GETCFN => {
            // SAFETY: the op's argument is an `el_rfunc_t`, per the header.
            let rc = unsafe { fn_arg::<ElRfuncT>(&mut ap) };
            // The C dereferences `el->el_read` unchecked. `Option` makes the
            // uninitialised case representable (ERR-input-16), and there is
            // nothing to install into: 0 is what the op always returns.
            el.el_read
                .as_deref_mut()
                .map_or(0, |rd| nshedit::read::el_read_setfn(rd, rc))
        }

        // One `void *`, stored verbatim and never dereferenced. Inline in the
        // C, and this arm leaves `rv` at 0.
        EL_CLIENTDATA => {
            // SAFETY: the op's argument is a `void *`.
            el.el_data = unsafe { ap.next_arg::<*mut c_void>() };
            0
        }

        // One `int`. A 0 -> non-zero transition sets UNBUFFERED and runs the
        // read-prepare sequence; the reverse clears it and runs read-finish;
        // setting it to the value it already holds does nothing. Always 0.
        EL_UNBUFFERED => {
            // SAFETY: the op's argument is an `int`.
            let on = unsafe { ap.next_arg::<c_int>() } != 0;
            let was = el.el_flags & UNBUFFERED != 0;
            // The flag is written *before* the sequence runs, and both
            // sequences read it: `read_prepare` enters raw mode only when
            // UNBUFFERED is set and editing is enabled, and `read_finish`
            // leaves the tty raw when it is set — which is why
            // `read_finish` here returns it to cooked mode.
            if on && !was {
                el.el_flags |= UNBUFFERED;
                nshedit::read::read_prepare(el);
            } else if !on && was {
                el.el_flags &= !UNBUFFERED;
                nshedit::read::read_finish(el);
            }
            0
        }

        // One `int`: non-zero raw, zero cooked, tty errors discarded.
        // Always 0. There is no matching get.
        EL_PREP_TERM => {
            // Both results are discarded, so a terminal that refused the mode
            // change is indistinguishable from one that took it.
            // SAFETY: the op's argument is an `int`.
            let _ = if unsafe { ap.next_arg::<c_int>() } != 0 {
                nshedit::tty::tty_rawmode(el)
            } else {
                nshedit::tty::tty_cookedmode(el)
            };
            0
        }

        // An `int what` then a `FILE *`, installed together with its
        // `fileno`. 0 for what in {0,1,2}, -1 otherwise. `fileno` is called
        // with no NULL check, which the C leaves undefined; `fileno_of`
        // answers -1 for a NULL stream instead, which is the descriptor the
        // C stores for a stream that has none.
        //
        // Both varargs are consumed before `what` is validated, so a rejected
        // `what` still leaves the tail walked past the stream. The previously
        // installed stream is neither flushed nor closed — the caller owns the
        // old one and the new.
        EL_SETFP => {
            // SAFETY: the op's arguments are an `int` then a `FILE *`.
            let what = unsafe { ap.next_arg::<c_int>() };
            let fp = unsafe { ap.next_arg::<*mut c_void>() };
            match what {
                0 => {
                    el.el_infile = fp;
                    el.el_infd = cstdio::fileno_of(fp);
                }
                1 => {
                    el.el_outfile = fp;
                    el.el_outfd = cstdio::fileno_of(fp);
                }
                2 => {
                    el.el_errfile = fp;
                    el.el_errfd = cstdio::fileno_of(fp);
                }
                // Any other `what` changes nothing.
                _ => return -1,
            }
            0
        }

        // No further arguments: clear the recorded display, redraw prompt and
        // line, flush. Returns 0 — the arm assigns nothing to `rv`.
        EL_REFRESH => {
            nshedit::refresh::re_clear_display(el);
            nshedit::refresh::re_refresh(el);
            // A no-op in the port: nothing is buffered on this side, because
            // the core writes through `el_outfd` rather than the caller's
            // `FILE *`. Called anyway, so the sequence stays the C's.
            nshedit::terminal::terminal__flush(el);
            0
        }

        // One `wchar_t *`: frees the previous set and installs a duplicate.
        // Always 0 even when the duplication fails or the argument is NULL —
        // the latter is dereferenced by the duplication, undefined in the C.
        EL_WORDCHARS => {
            // ERR-core-api-08, disposition `define — reject NULL`: the C
            // hands the argument to `wcsdup`, which dereferences it. -1 is
            // the only failure this op has to report it with; every
            // well-formed call still returns 0, the duplication failing
            // included (ERR-core-api-30).
            // SAFETY: the op's argument is null or a NUL-terminated wide
            // string.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            match unsafe { wstr(p.cast::<u32>()) } {
                Some(w) => nshedit::map::map_set_wordchars(el, w),
                None => -1,
            }
        }

        // One `char *(*)(const char *)`. Always 0. A NULL accessor is
        // installed as-is and every later lookup calls through it, which the
        // C leaves undefined. ERR-core-api-08, disposition `define — reject
        // NULL`: `None` is the core's "no application hook", so a NULL
        // argument leaves the built-in `secure_getenv` in force rather than
        // arming an indirect call through null. Installing
        // [`default_getenv`] — the address `el_get(EL_GETENV)` reports for a
        // fresh handle — round-trips onto that same state.
        EL_GETENV => {
            // SAFETY: the op's argument is a `func_t`, per the header.
            let f = unsafe { fn_arg::<FuncT>(&mut ap) };
            el.el_getenv = f.filter(|f| !core::ptr::fn_addr_eq(*f, default_getenv as FuncT));
            0
        }

        // Every other op, EL_GETTC and EL_GETFP included, reads no arguments.
        _ => -1,
    }
}

/// C: `int el_wget(EditLine *, int, ...);`
///
/// The read side of [`el_wset`]: every argument is a pointer to the type the
/// corresponding set op takes by value, and the result is stored through it.
/// The widest get op reads two (`EL_PROMPT_ESC`, `EL_GETTC`, `EL_GETFP`);
/// most read one and the set-only ops read none.
// [spec:libedit:def:histedit.el-wget-fn]
// [spec:libedit:sem:histedit.el-wget-fn]
// [spec:libedit:def:el.el-wget-fn]
// [spec:libedit:sem:el.el-wget-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wget(el: *mut EditLine, op: c_int, ap: ...) -> c_int {
    // As `el_wset`: the NULL check sits above `va_start` in the C.
    if el.is_null() {
        return -1;
    }
    // SAFETY: `el` is non-null and is the caller's live handle, and the tail
    // carries what `op` says it carries.
    unsafe { el_wget_va(&mut *el, op, ap) }
}

/// [`el_wget`]'s dispatch, out of the variadic frame.
///
/// # Safety
/// The tail must carry the out-pointers the selected `op` defines, in order.
pub(crate) unsafe fn el_wget_va(el: &mut EditLine, op: c_int, mut ap: VaList<'_>) -> c_int {
    match op {
        // One `el_pfunc_t *`. -1 if NULL, else 0. The value may be the
        // internal default rather than anything the application installed.
        EL_PROMPT | EL_RPROMPT => {
            // SAFETY: the op's argument is an `el_pfunc_t *`. A slot holding
            // a possibly-NULL function pointer and an `Option<ElPfuncT>` are
            // the same object: `Option` of a function pointer is null-niche
            // optimised, which [`fn_arg`] asserts for the reverse direction.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            let prf = unsafe { p.cast::<Option<ElPfuncT>>().as_mut() };
            // The C passes a NULL escape-character pointer here, which is why
            // `el_prompt.p_ignore` has no route out of the library at all
            // (the other half of ERR-core-api-14).
            nshedit::prompt::prompt_get(el, prf, None, op)
        }

        // An `el_pfunc_t *` then a `wchar_t *`, the latter optional.
        // `prompt_get` selects the left prompt only for `op == EL_PROMPT`, so
        // EL_PROMPT_ESC reads the *right* prompt's function and escape
        // character — ERR-core-api-14, frozen, and the reason set/get through
        // EL_PROMPT_ESC does not round-trip. `op` is passed through unchanged,
        // so the core's own reproduction of the defect is what decides.
        EL_PROMPT_ESC | EL_RPROMPT_ESC => {
            // SAFETY: as above.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            let prf = unsafe { p.cast::<Option<ElPfuncT>>().as_mut() };
            // SAFETY: the op's second argument is a `wchar_t *`, which the
            // rule allows to be NULL; the store is then skipped. It is read
            // before `prompt_get` runs, as in the C.
            let c = unsafe { ap.next_arg::<*mut c_void>() };
            let c = unsafe { c.cast::<u32>().as_mut() };
            nshedit::prompt::prompt_get(el, prf, c, op)
        }

        // One `const wchar_t **`, set to the static `L\"emacs\"`/`L\"vi\"`.
        EL_EDITOR => {
            // SAFETY: the op's argument is a `const wchar_t **`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            // `map_get_editor`'s NULL check, which its `&mut` cannot make.
            if out.is_null() {
                return -1;
            }
            let mut editor: &'static [u32] = &[];
            let rv = nshedit::map::map_get_editor(el, &mut editor);
            if rv != 0 {
                // The unreachable third map type: nothing is stored.
                return rv;
            }
            // Back onto the terminated statics. The core's answer is one of
            // its own two literals and nothing else, so the fall-through is
            // as dead as the -1 above (ERR-modes-71).
            let p = if editor == &EDITOR_EMACS[..5] {
                EDITOR_EMACS.as_ptr()
            } else if editor == &EDITOR_VI[..2] {
                EDITOR_VI.as_ptr()
            } else {
                return -1;
            };
            // SAFETY: the op's argument is a `const wchar_t **`.
            unsafe { *out.cast::<*const u32>() = p };
            0
        }

        // One `int *`, set to the raw HANDLE_SIGNALS bit. Not normalised —
        // it reads as 1 only because that bit happens to be 0x001.
        EL_SIGNAL => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = el.el_flags & HANDLE_SIGNALS };
            0
        }

        // One `int *`, set to the logical negation of EDIT_DISABLED, so
        // genuinely 0 or 1 and inverted to match the setter's polarity.
        EL_EDITMODE => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = c_int::from(el.el_flags & EDIT_DISABLED == 0) };
            0
        }

        // One `int *`, set to the raw FIXIO bit — **256**, not 1. A caller
        // comparing it against 1 gets the wrong answer. Frozen behaviour;
        // deliberately not normalised here.
        EL_SAFEREAD => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = el.el_flags & FIXIO };
            0
        }

        // One `const char **`, set to the loaded terminal type name — narrow
        // bytes even in the wide API. Always 0.
        EL_TERMINAL => {
            // SAFETY: the op's argument is a `const char **`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            let key = core::ptr::from_mut(el) as usize;
            let mut name: Option<&str> = None;
            nshedit::terminal::terminal_get(el, &mut name);
            // The C hands out `t_name` itself, a borrowed pointer a later
            // `terminal_set` replaces; the core's `String` carries no
            // terminator, so a NUL-terminated copy is owned here per editor.
            // Divergence worth naming: this copy is replaced on every call,
            // so a pointer from an earlier `el_wget(EL_TERMINAL)` is
            // invalidated by a later one — where the C's survives until the
            // type is reloaded. A name containing an interior NUL, which the
            // C could not have produced, reports NULL.
            let p = TERMNAME.with_borrow_mut(|m| {
                match name.and_then(|s| CString::new(s).ok()) {
                    Some(c) => {
                        let slot = m.entry(key).or_default();
                        *slot = c;
                        slot.as_ptr()
                    }
                    // `t_name` is NULL until the first `terminal_set`.
                    None => {
                        m.remove(&key);
                        core::ptr::null()
                    }
                }
            });
            // The C has no NULL check here and neither has this.
            // SAFETY: the out-pointer read above.
            unsafe { *out.cast::<*const c_char>() = p };
            0
        }

        // A `char *` capability name then a capability-dependent out pointer,
        // built into the argv `{\"gettc\", name, out}`. Exactly two arguments
        // are read despite the header's `..., NULL`. A string capability and
        // the boolean-ish `pt`/`km`/`am`/`xn` want a `char **`; every other
        // numeric one wants an `int *`, and passing the wrong one is a
        // type-confusing store the C leaves undefined.
        EL_GETTC => {
            // C: `argv[0] = name; argv[1] = va_arg(char *); argv[2] =
            // va_arg(void *)`, then `terminal_gettc(el, 3, argv)`. The count
            // is the literal 3 and the handler ignores it. Exactly two
            // arguments are read: no sentinel is consumed, despite the
            // header's `..., NULL` (ERR-core-api-29).
            // SAFETY: the op's arguments are a `char *` then a `void *`.
            let name = unsafe { ap.next_arg::<*mut c_void>() };
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            let argv = [
                GETTC.as_ptr().cast::<c_char>().cast_mut(),
                name.cast::<c_char>(),
                out.cast::<c_char>(),
            ];
            nshedit::terminal::terminal_gettc(el, 3, &argv)
        }

        // One `el_rfunc_t *`, set to `EL_BUILTIN_GETCFN` (NULL) when the
        // builtin reader is installed — so a set/get round trip normalises
        // the builtin to NULL rather than reporting its address.
        EL_GETCFN => {
            // As in the setter, an uninitialised read subsystem is the C's
            // NULL dereference and is defined here; it reports the builtin,
            // which is what a reader installed after `read_init` would be.
            let f = el
                .el_read
                .as_deref_mut()
                .and_then(nshedit::read::el_read_getfn);
            // SAFETY: the op's argument is an `el_rfunc_t *`, and an
            // `Option<ElRfuncT>` is that slot's exact representation.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<Option<ElRfuncT>>() = f };
            0
        }

        // One `void **`, set to the registered client pointer.
        EL_CLIENTDATA => {
            // SAFETY: the op's argument is a `void **` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<*mut c_void>() = el.el_data };
            0
        }

        // One `int *`, normalised to 0 or 1 — unlike EL_SIGNAL and
        // EL_SAFEREAD.
        EL_UNBUFFERED => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = c_int::from(el.el_flags & UNBUFFERED != 0) };
            0
        }

        // An `int what` then a `FILE **`: input 0, output 1, error 2. Any
        // other `what` returns -1 with the caller's storage untouched. Both
        // are read before `what` is validated, as in the C. The descriptors
        // cannot be read back, only the streams.
        EL_GETFP => {
            // SAFETY: the op's arguments are an `int` then a `FILE **`.
            let what = unsafe { ap.next_arg::<c_int>() };
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            let fp = match what {
                0 => el.el_infile,
                1 => el.el_outfile,
                2 => el.el_errfile,
                _ => return -1,
            };
            // SAFETY: the out-pointer read above.
            unsafe { *out.cast::<CFile>() = fp };
            0
        }

        // One `const wchar_t **`, set to the word-character set. NULL means
        // "the built-in defaults are in use", not "empty".
        EL_WORDCHARS => {
            // SAFETY: the op's argument is a `const wchar_t **`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            // `map_get_wordchars`'s NULL check, which its `&mut` cannot make.
            if out.is_null() {
                return -1;
            }
            let key = core::ptr::from_mut(el) as usize;
            let mut wordchars: Option<Vec<u32>> = None;
            let rv = nshedit::map::map_get_wordchars(el, &mut wordchars);
            if rv != 0 {
                return rv;
            }
            // As `EL_TERMINAL`: the C lends out its own buffer, the core
            // hands back an owned copy with no terminator, and the
            // terminated one is owned here per editor and replaced on every
            // call. The C's buffer instead survives until the set is
            // reinstalled — by `EL_WORDCHARS`, `bind -v`/`bind -e`, an
            // `EL_EDITOR` switch or `el_end`, each of which frees it and any
            // of which the C's caller must not hold a pointer across.
            let p = WORDCHARS.with_borrow_mut(|m| match wordchars {
                Some(mut w) => {
                    w.push(0);
                    let slot = m.entry(key).or_default();
                    *slot = w;
                    slot.as_ptr()
                }
                None => {
                    m.remove(&key);
                    core::ptr::null()
                }
            });
            // SAFETY: the out-pointer read above.
            unsafe { *out.cast::<*const u32>() = p };
            0
        }

        // One `func_t *`, set to the installed environment accessor. Always 0.
        // The C stores `secure_getenv` itself at construction, so a fresh
        // handle reports a non-NULL address here; the core keeps `None` for
        // that state, which is why [`default_getenv`] exists.
        EL_GETENV => {
            let f: FuncT = el.el_getenv.unwrap_or(default_getenv);
            // SAFETY: the op's argument is a `func_t *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<FuncT>() = f };
            0
        }

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
// [spec:libedit:def:el.el-wline-fn]
// [spec:libedit:sem:el.el-wline-fn]
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
pub unsafe extern "C" fn el_winsertstr(el: *mut EditLine, str_: *const wchar_t) -> c_int {
    // A NULL string and an empty one are the same -1, so NULL becomes the
    // empty slice rather than a separate check.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    nshedit::chared::el_winsertstr(unsafe { &mut *el }, unsafe { wstr(str_) }.unwrap_or(&[]))
}

// [spec:libedit:def:histedit.el-wreplacestr-fn]
// [spec:libedit:sem:histedit.el-wreplacestr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_wreplacestr(el: *mut EditLine, str_: *const wchar_t) -> c_int {
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

/// The varargs tail of `history_w`/`history`, walked and handed to the core as
/// a `HistoryArg`.
///
/// The op enumeration below is the whole of this function: each code names how
/// many trailing arguments it has and what they are, which is what the core's
/// `HistoryArg` is a closed form of, and it is also exactly how many `va_arg`
/// reads the C makes. Only one entry in that table depends on the
/// instantiation — the five string ops, whose argument is `const wchar_t *`
/// wide and `const char *` narrow — so the table is written once and `C` picks
/// the spelling. `H_LOAD` and `H_SAVE` take a `const char *` path in *both*,
/// because the on-disk format is bytes and is frozen.
///
/// # Safety
/// `h` and `ev` must be a live handle and a writable event of this
/// instantiation, and the tail must carry what the op code says.
unsafe fn history_dispatch<C: nshedit::history::HistChar>(
    h: *mut nshedit::history::HistoryGen<C>,
    ev: *mut nshedit::histedit::HistEventGen<C>,
    op: c_int,
    mut ap: VaList<'_>,
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

    // `H_FUNC`'s assembled vtable. C: `TYPE(History) hf`, a stack local in
    // `FUNW(history)`; hoisted out of the arm below only so the borrow the
    // `HistoryArg` takes outlives the match.
    let hf;

    // The caller's stream for `H_SAVE_FP`/`H_NSAVE_FP`, hoisted for the same
    // reason: the core borrows it for the length of the call and never past
    // it, so no `FILE *` outlives the operation that was handed it.
    let mut fp_out: Option<CFileWriter> = None;

    let arg = match op {
        // `void *ptr` then ten function pointers: first, next, last, prev,
        // curr, set, clear, enter, add, del — eleven arguments, one more than
        // the manual documents. The C reads `ptr`, validates it, and never
        // stores it, so the installed functions are called with libedit's own
        // builtin state pointer (ERR-history-04, frozen).
        H_FUNC => {
            // SAFETY: for this op the eleven slots carry the state pointer and
            // then the ten vtable functions in exactly this order, per the
            // header and `sem:histedit.history-w-fn`.
            hf = nshedit::history::HistoryGen::<C> {
                h_ref: unsafe { ap.next_arg::<*mut c_void>() },
                // Not read by anything this op reaches: the C's `hf` is an
                // uninitialised stack local except for the eleven fields it
                // fills, and `history_set_fun` copies only the ten callbacks.
                // The dispatch's own `h->h_ent = -1` is the core's.
                h_ent: 0,
                h_first: unsafe { fn_arg::<HistoryGfunT<C>>(&mut ap) },
                h_next: unsafe { fn_arg::<HistoryGfunT<C>>(&mut ap) },
                h_last: unsafe { fn_arg::<HistoryGfunT<C>>(&mut ap) },
                h_prev: unsafe { fn_arg::<HistoryGfunT<C>>(&mut ap) },
                h_curr: unsafe { fn_arg::<HistoryGfunT<C>>(&mut ap) },
                h_set: unsafe { fn_arg::<HistorySfunT<C>>(&mut ap) },
                h_clear: unsafe { fn_arg::<HistoryVfunT<C>>(&mut ap) },
                h_enter: unsafe { fn_arg::<HistoryEfunT<C>>(&mut ap) },
                h_add: unsafe { fn_arg::<HistoryEfunT<C>>(&mut ap) },
                h_del: unsafe { fn_arg::<HistorySfunT<C>>(&mut ap) },
            };
            HistoryArg::Funcs(&hf)
        }

        // One `int`.
        // SAFETY: the op's argument is an `int`.
        H_SETSIZE | H_SET | H_SETUNIQUE | H_DEL | H_NEXT_EVENT | H_PREV_EVENT => {
            HistoryArg::Num(unsafe { ap.next_arg::<c_int>() })
        }

        // No trailing argument. `H_CURR` included: the header comment's
        // `, const int)` is wrong.
        H_GETSIZE | H_FIRST | H_LAST | H_PREV | H_NEXT | H_CURR | H_END | H_CLEAR | H_GETUNIQUE => {
            HistoryArg::None
        }

        // One `const wchar_t *`.
        // SAFETY: the op's argument is a NUL-terminated string of this
        // instantiation's character type.
        H_ADD | H_ENTER | H_APPEND | H_NEXT_STR | H_PREV_STR => HistoryArg::Str(
            unsafe { ap.next_arg::<*mut c_void>() }
                .cast::<C>()
                .cast_const(),
        ),

        // One `const char *` filename — narrow in both instantiations,
        // because the on-disk format is bytes and is frozen.
        //
        // The core takes `&str`, so a path that is not UTF-8 cannot be passed
        // on; it is reported as the op's failure rather than opened under a
        // different name. See the crate report — `history_load`/`history_save`
        // want `&Path`.
        H_LOAD | H_SAVE => {
            // SAFETY: the op's argument is a NUL-terminated path.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            let path =
                unsafe { cbytes(p.cast::<c_char>()) }.and_then(|b| core::str::from_utf8(b).ok());
            match path {
                Some(p) => HistoryArg::Path(p),
                None => return -1,
            }
        }

        // One `FILE *`, which the caller keeps and must close. The stream is
        // read and written here rather than in the core: see
        // [`crate::cstdio`] for why the descriptor behind it is not a
        // substitute.
        H_SAVE_FP => {
            // SAFETY: the op's argument is the caller's `FILE *`.
            let fp = unsafe { ap.next_arg::<*mut c_void>() };
            HistoryArg::Fp(SaveStream {
                at_start: cstdio::at_start(fp),
                out: fp_out.insert(CFileWriter::new(fp)),
            })
        }

        // `size_t n` then `FILE *`. `n` is passed through unchanged: the walk
        // takes `n` steps back from the newest and then writes forward, so it
        // emits **n + 1** entries and `n == 0` writes one (ERR-history-19).
        // Not corrected here.
        H_NSAVE_FP => {
            // SAFETY: the op's arguments are a `size_t` then a `FILE *`.
            let n = unsafe { ap.next_arg::<usize>() };
            let fp = unsafe { ap.next_arg::<*mut c_void>() };
            HistoryArg::NSaveFp(
                n,
                SaveStream {
                    at_start: cstdio::at_start(fp),
                    out: fp_out.insert(CFileWriter::new(fp)),
                },
            )
        }

        // `int` then `void **`. The pointer stays raw: `H_DELDATA` accepts
        // the magic `(void **)-1` meaning "position the cursor only".
        H_NEXT_EVDATA | H_DELDATA => {
            // SAFETY: the op's arguments are an `int` then a `void **`.
            let num = unsafe { ap.next_arg::<c_int>() };
            let d = unsafe { ap.next_arg::<*mut c_void>() };
            HistoryArg::EvData(num, d.cast::<*mut c_void>())
        }

        // `const wchar_t *line` then `void *data`. It does not free the
        // string it overwrites, so every call leaks one (ERR-history-08),
        // and it reaches into the builtin state without checking that one is
        // installed. Both reproduced.
        H_REPLACE => {
            // SAFETY: the op's arguments are a NUL-terminated string of this
            // instantiation's character type then an opaque cookie.
            let line = unsafe { ap.next_arg::<*mut c_void>() };
            let d = unsafe { ap.next_arg::<*mut c_void>() };
            HistoryArg::Replace(line.cast::<C>().cast_const(), d)
        }

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
    nshedit::history::history_gen(h, ev, op, arg)
}

/// C: `int history_w(HistoryW *, HistEventW *, int, ...);`
// [spec:libedit:def:histedit.history-w-fn]
// [spec:libedit:sem:histedit.history-w-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn history_w(
    h: *mut HistoryW,
    ev: *mut HistEventW,
    op: c_int,
    ap: ...
) -> c_int {
    // Ownership on the way out, reproduced by the core and not touched here:
    // `H_DEL` and `H_DELDATA` hand the caller a string it owns and must free
    // (and which is NULL on an allocation failure); every other op's `ev.str`
    // points into libedit's storage or at a static message and must not be
    // freed.
    // SAFETY: this function's own contract, forwarded unchanged.
    unsafe { history_dispatch::<u32>(h, ev, op, ap) }
}

// [spec:libedit:def:histedit.tok-winit-fn]
// [spec:libedit:sem:histedit.tok-winit-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tok_winit(ifs: *const wchar_t) -> *mut TokenizerW {
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

/// Resolve a tokenizer's `argv` offsets into the pointer array the C hands
/// back, for either instantiation.
///
/// The C's `*argv = tok->argv` aliases the tokenizer's own array; `nshedit`
/// stores offsets into `wspace` instead, so the pointer array is built here
/// and owned per tokenizer. It is replaced on every successful call, which is
/// the C's "invalidated by the next `tok_line`, `tok_str` or `tok_reset`".
///
/// Slot `argc` is materialised from whatever the tokenizer has there rather
/// than forced to NULL, so `tok_reset`'s stale terminator survives into the
/// array exactly as it does in the C (ERR-input-38).
fn argv_ptrs<C: nshedit::tokenizer::TokChar>(
    tok: &nshedit::tokenizer::TokenizerGen<C>,
    argc: c_int,
) -> Vec<*const C> {
    let n = if argc > 0 { argc as usize } else { 0 };
    let base = tok.wspace.as_ptr();
    let mut out: Vec<*const C> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let p = match tok.argv.get(i).copied().flatten() {
            // SAFETY: published slots are offsets into `wspace`, which is
            // where `base` points and which is at least that long.
            Some(off) => unsafe { base.add(off) },
            None => core::ptr::null(),
        };
        out.push(p);
    }
    out
}

/// Materialise the `const wchar_t **` a successful `tok_wline`/`tok_wstr`
/// hands back, and keep it alive until the next call on this tokenizer.
fn publish_argv(tok: &TokenizerW, argc: c_int) -> *mut *const u32 {
    let out = argv_ptrs(tok, argc);
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
    argv: *mut *mut *const wchar_t,
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
    line: *const wchar_t,
    argc: *mut c_int,
    argv: *mut *mut *const wchar_t,
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

#[cfg(test)]
mod tests;
