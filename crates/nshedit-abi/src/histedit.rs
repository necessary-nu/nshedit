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
//! over `char`. History uses the native core store while the opaque ABI owner
//! keeps event numbers, callbacks, persistence, and narrow/wide representation
//! mechanics private. Tokenization likewise uses the native core parser. The
//! exported pairs differ only in which boundary character type they pin.
//!
//! Nothing in this module is blocked on the core any more. The two causes that
//! used to abort here are both closed: the narrow instantiations exist, and
//! `el_wset(EL_SETFP)`'s facility over a caller's opaque `FILE *` is
//! [`crate::cstdio`], the third site `plan/decisions/no-c-ffi.md` enumerates.

use core::ffi::{VaList, c_char, c_int, c_uchar, c_void};
use std::cell::RefCell;
use std::ffi::{CString, OsStr};
use std::io::BufRead;
use std::os::unix::ffi::OsStrExt;

use nshedit::domain::{Direction, Outcome, Prompt, Refresh, TerminalLiteral, Text, TextUnit};
use nshedit::editor::effect::{HistoryResponse, HostFailure, PromptSide, ReadOutcome};
use nshedit::editor::{
    ReadResult, ReadStep, Tokenization as NativeTokenization, Tokenizer as NativeTokenizer,
};

// Renamed on import so the signatures below read as `histedit.h` writes
// them; see the note on `LineInfoWide`.
use crate::adapter::{
    AliasCallback as ElAfuncT, BoundaryContinuation, CommandCallback as ElFuncT, EditLine,
    EnvironmentCallback as FuncT, HistoryCallback as HistFunT, PromptCallback,
    ReadCallback as ElRfuncT, ResizeCallback as ElZfuncT, TokenizeOutcome, Tokenizer, TokenizerW,
    WidePromptCallback as ElPfuncT,
};
use crate::cdecl::handles::{History, HistoryW};
use crate::cdecl::histedit::{
    CC_EOF, CC_NEWLINE, CC_REDISPLAY, CC_REFRESH_BEEP, CFile, H_FIRST, H_NEXT, HistEvent,
    HistEventGen, HistEventWide as HistEventW, LineInfo, LineInfoGen, LineInfoWide as LineInfoW,
    WcharT,
};
use crate::cstdio::{self, CFileWriter};
use crate::history::{
    CallbackSet, ClearCallback, DispatchArg, EnterCallback, GetCallback, HistoryChar,
    HistoryHandle, HistoryOwner, HistoryWideOwner, SaveStream, SelectCallback,
};

mod driver;
mod editrc;

use driver::{drive_read, read_unedited, read_wide_character, text_from_bytes};
use editrc::{dispatch_editrc, environment_value, parse_editrc_line};

// ---------------------------------------------------------------------------
// `el_set`/`el_get` operation codes. C: `histedit.h`, which defines them as
// untyped `#define`s carrying no rule of their own. They live here because
// they select the ABI-owned varargs dispatch; the native editor never sees
// them. The numbering is ABI: a consumer passes these integers directly.
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
// The rest of that entry is about the `H_*` opcodes declared below.
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

/// The bit `el_wget(EL_SAFEREAD)` stores raw — 256, not 1. See
/// `sem:histedit.el-wget-fn`.
const FIXIO: i32 = 0x100;
const EILSEQ: c_int = 84;

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
// Helpers for the C ABI boundary.
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

/// Borrow the region described by a C `LineInfo` and translate its cursor to
/// an optional element offset. A cursor at `lastchar` is deliberately absent:
/// the reference tokenizer substitutes end-of-input before comparing it.
///
/// # Safety
/// The three pointers must describe one live allocation as required by the
/// corresponding `LineInfo` contract, with `lastchar >= buffer`.
unsafe fn tokenizer_input<'a, C>(line: &LineInfoGen<C>) -> (&'a [C], Option<usize>) {
    // SAFETY: the caller guarantees both pointers belong to one allocation.
    let len = usize::try_from(unsafe { line.lastchar.offset_from(line.buffer) }).unwrap_or(0);
    // SAFETY: the caller guarantees this `buffer..lastchar` region is live.
    let input = unsafe { core::slice::from_raw_parts(line.buffer, len) };
    let cursor = if line.cursor.is_null() {
        None
    } else {
        // SAFETY: the caller guarantees the cursor belongs to the same line
        // allocation. Negative and end offsets do not match the C loop.
        usize::try_from(unsafe { line.cursor.offset_from(line.buffer) })
            .ok()
            .filter(|offset| *offset < len)
    };
    (input, cursor)
}

/// Map a typed tokenizer result onto the C integer protocol and output slots.
///
/// # Safety
/// `argc` and `argv` must be writable. The two cursor pointers may be NULL;
/// when non-NULL they must be writable too.
unsafe fn publish_tokenize_outcome<C>(
    outcome: TokenizeOutcome<C>,
    argc: *mut c_int,
    argv: *mut *mut *const C,
    cursorc: *mut c_int,
    cursoro: *mut c_int,
) -> c_int {
    let published = match outcome {
        TokenizeOutcome::Published(published) => published,
        TokenizeOutcome::Incomplete(BoundaryContinuation::SingleQuote) => return 1,
        TokenizeOutcome::Incomplete(BoundaryContinuation::DoubleQuote) => return 2,
        TokenizeOutcome::Incomplete(BoundaryContinuation::EscapedNewline) => return 3,
        TokenizeOutcome::Failed => return -1,
    };

    // SAFETY: the success-path output contract is stated above.
    unsafe {
        *argc = published.count;
        *argv = published.words;
        if !cursorc.is_null() {
            *cursorc = published.cursor_word;
        }
        if !cursoro.is_null() {
            *cursoro = published.cursor_offset;
        }
    }
    0
}

/// The program name `el_init`/`el_init_fd` take.
///
/// C decodes it from the current locale's multibyte encoding and duplicates
/// the result, dereferencing `NULL` on the way (`sem:histedit.el-init-fd-fn`
/// step 4 calls that undefined). Defined here as "a `NULL` or undecodable
/// name is a failed construction", which is the C's own reaction to a decode
/// failure and turns the undefined case into the documented `NULL` return.
///
/// The native constructor takes `&str`, so the decode is UTF-8 rather than the
/// process locale; see the crate report.
///
/// # Safety
/// As [`cbytes`].
unsafe fn prog_name<'a>(p: *const c_char) -> Option<&'a str> {
    core::str::from_utf8(unsafe { cbytes(p) }?).ok()
}

thread_local! {
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
/// back. The native secure lookup returns an owned value, which cannot itself
/// be a `func_t`, so the adapter stores `None` for "no application hook" and
/// calls it directly. This is the C-callable face of the same lookup, for the
/// one purpose of having something to hand out.
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
    let Some(value) = crate::adapter::secure_environment(name) else {
        return core::ptr::null_mut();
    };
    // An environment value cannot contain a NUL, so this cannot fail; a
    // failure would be reported as unset regardless.
    let Ok(value) = CString::new(value) else {
        return core::ptr::null_mut();
    };
    // The previous answer is dropped here, which is exactly the invalidation
    // point the contract names.
    GETENV_VALUE.with_borrow_mut(|slot| slot.insert(value).as_ptr().cast_mut())
}

// [spec:libedit:def:el.el-init-fn]
// [spec:libedit:sem:el.el-init-fn]
// [spec:libedit:def:histedit.el-init-fn]
// [spec:libedit:sem:histedit.el-init-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
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
    // the compatibility engine cannot do itself. The evaluation order of the
    // three calls is unspecified in C and has no observable consequence;
    // left to right here.
    EditLine::new(
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
#[doc = include_str!("ffi_safety.md")]
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
    EditLine::new(prog, fin, fout, ferr, fdin, fdout, fderr)
        .map_or(core::ptr::null_mut(), Box::into_raw)
}

// [spec:libedit:def:histedit.el-end-fn]
// [spec:libedit:sem:histedit.el-end-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_end(el: *mut EditLine) {
    // The one NULL-tolerant entry point in the editing API.
    if el.is_null() {
        return;
    }
    // SAFETY: the caller gives us a live handle from `el_init`/`el_init_fd`,
    // which is exactly the ABI-owned `Box` those returned. Dropping the
    // native editor discharges its terminal-restoration obligation.
    drop(unsafe { Box::from_raw(el) });
}

// [spec:libedit:def:histedit.el-reset-fn]
// [spec:libedit:sem:histedit.el-reset-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_reset(el: *mut EditLine) {
    // SAFETY: `el` must be non-NULL; the C has no check and neither has this.
    unsafe { &mut *el }.reset_line();
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_beep(el: *mut EditLine) {
    // SAFETY: `el` must be non-NULL; there is no check in the C.
    unsafe { &mut *el }.beep();
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn _el_fn_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    // SAFETY: `el` must be non-NULL. `ch` is ignored, as in the C.
    crate::filecomplete::complete_builtin(unsafe { &mut *el }, ch)
}

/// C: `unsigned char _el_fn_sh_complete(EditLine *, int);` — the
/// shell-quoting variant. Its body is `filecomplete.c`'s, under
/// `sem:filecomplete.el-fn-sh-complete-fn`.
// [spec:libedit:def:histedit.el-fn-sh-complete-fn]
// [spec:libedit:sem:histedit.el-fn-sh-complete-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn _el_fn_sh_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    // A distinct exported symbol that forwards both arguments unchanged; the
    // two are behaviourally identical and must stay separate symbols.
    // SAFETY: `el` must be non-NULL.
    crate::filecomplete::complete_builtin(unsafe { &mut *el }, ch)
}

// [spec:libedit:def:histedit.el-source-fn]
// [spec:libedit:sem:histedit.el-source-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_source(el: *mut EditLine, fname: *const c_char) -> c_int {
    if el.is_null() {
        return -1;
    }
    let name = if let Some(explicit) = unsafe { cbytes(fname) } {
        explicit.to_vec()
    } else if let Some(editrc) = unsafe { environment_value(el, "EDITRC") } {
        editrc
    } else {
        let mut home = unsafe { environment_value(el, "HOME") }.unwrap_or_default();
        if !home.is_empty() && !home.ends_with(b"/") {
            home.push(b'/');
        }
        home.extend_from_slice(b".editrc");
        home
    };
    let name = name.split(|byte| *byte == 0).next().unwrap_or(&[]);
    if name.is_empty() {
        return -1;
    }
    let Ok(file) = std::fs::File::open(OsStr::from_bytes(name)) else {
        return -1;
    };
    let mut result = 0;
    let mut line = Vec::new();
    let mut reader = std::io::BufReader::new(file);
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line == b"\n" {
            continue;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        let decoded = text_from_bytes(&line);
        let first = decoded
            .as_units()
            .iter()
            .position(|unit| match unit {
                TextUnit::Scalar(character) => !character.is_whitespace(),
                TextUnit::RawByte(byte) => !byte.is_ascii_whitespace(),
                TextUnit::CompatibilityWide(_) => true,
            })
            .unwrap_or(decoded.len());
        let content = &decoded.as_units()[first..];
        if content.first() == Some(&TextUnit::Scalar('#')) {
            continue;
        }
        result = unsafe { parse_editrc_line(el, content) };
        if result == -1 {
            break;
        }
    }
    result
}

// [spec:libedit:def:histedit.el-resize-fn]
// [spec:libedit:sem:histedit.el-resize-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_resize(el: *mut EditLine) {
    // SAFETY: `el` must be non-NULL. Not async-signal-safe, as in the C.
    let callback = unsafe { (&*el).resize_callback() };
    unsafe { &mut *el }.resize_display();
    if let Some((callback, cookie)) = callback {
        // SAFETY: the callback and cookie were installed by this caller for
        // this live handle. No Rust editor borrow crosses the foreign call.
        unsafe { callback(el, cookie) };
    }
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_deletestr(el: *mut EditLine, count: c_int) {
    // Also exported as `el_wdeletestr`, which the header `#define`s onto this
    // name — one function, counting wide characters under either spelling.
    // A refusal is indistinguishable from a success; there is no return.
    // SAFETY: `el` must be non-NULL.
    unsafe { &mut *el }.delete_before_cursor(count);
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_deletestr1(el: *mut EditLine, start: c_int, end: c_int) -> c_int {
    // The return is `end - start` whatever was actually removed, and the
    // cursor is only clamped at the low end — ERR-buffer-15, ERR-buffer-16,
    // ERR-buffer-17 and ERR-buffer-18, all reproduced in the core because
    // `rl_delete_text` is layered on this call.
    // SAFETY: `el` must be non-NULL.
    unsafe { &mut *el }.delete_range(start, end)
}

// [spec:libedit:def:histedit.history-init-fn]
// [spec:libedit:sem:histedit.history-init-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn history_init() -> *mut History {
    // `History` is `history.c` compiled with `Char = char`: a separate store
    // from the wide one, with byte strings throughout and no locale anywhere
    // in it. As `history_winit`, the handle is raw all the way through —
    // `H_END` frees it from inside `history` — and NULL is the C's allocation
    // failure. The retained maximum starts at 0, so `H_SETSIZE` is required
    // before any `H_ENTER` keeps anything.
    HistoryOwner::new_raw().cast()
}

// [spec:libedit:def:histedit.history-end-fn]
// [spec:libedit:sem:histedit.history-end-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn history_end(h: *mut History) {
    // `h` must be non-NULL; there is no check and calling it twice is a double
    // free. Every `HistEvent.str` from this handle is dangling afterwards
    // except those from `H_DEL`/`H_DELDATA`, which the caller owns.
    // SAFETY: `h` is NULL or the allocation returned by `history_init`.
    unsafe { crate::history::end(h.cast::<HistoryOwner>()) };
}

/// C: `int history(History *, HistEvent *, int, ...);`
///
/// The tail is walked by [`history_dispatch`], which is the same table
/// `history_w` uses with `Char` set to `char`.
// [spec:libedit:def:histedit.history-fn]
// [spec:libedit:sem:histedit.history-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn history(h: *mut History, ev: *mut HistEvent, op: c_int, ap: ...) -> c_int {
    // Same op codes, argument shapes, error codes and ownership rules as
    // `history_w` — `sem:histedit.history-fn` is the rule `history_w`'s is
    // written against — but over a byte store, so `H_ADD`, `H_ENTER`,
    // `H_APPEND`, `H_NEXT_STR`, `H_PREV_STR` and `H_REPLACE` take
    // `const char *` and `ev.str` comes back as one. `H_LOAD`/`H_SAVE` are
    // unchanged: the path was already narrow in both instantiations, and so
    // was the file.
    // SAFETY: this function's own contract, forwarded unchanged.
    unsafe { history_dispatch::<c_char>(h.cast::<HistoryOwner>(), ev, op, ap) }
}

// [spec:libedit:def:histedit.tok-init-fn]
// [spec:libedit:sem:histedit.tok-init-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_init(ifs: *const c_char) -> *mut Tokenizer {
    // `Tokenizer` is `tokenizer.c` compiled with `Char = char`: a byte word
    // space, byte `argv` slots and a byte IFS. NULL selects the default
    // `"\t \n"`; the caller keeps ownership of its string, and the tokenizer
    // owns everything it later hands back.
    // SAFETY: `ifs` is null or a NUL-terminated byte string.
    let separators = unsafe { cstr(ifs) };
    Box::into_raw(Tokenizer::from_narrow(separators))
}

// [spec:libedit:def:histedit.tok-end-fn]
// [spec:libedit:sem:histedit.tok-end-fn]
// [spec:libedit:def:tokenizer.fun-tok-end-fn]
// [spec:libedit:sem:tokenizer.fun-tok-end-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_end(tok: *mut Tokenizer) {
    // `tok` must be non-NULL (no check) and must be a `Tokenizer`; every
    // `argv` array and word pointer from this tokenizer dangles afterwards,
    // so the materialised array goes with it.
    // SAFETY: `tok` is the ABI allocation `tok_init` returned.
    drop(unsafe { Box::from_raw(tok) });
}

// [spec:libedit:def:histedit.tok-reset-fn]
// [spec:libedit:sem:histedit.tok-reset-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_reset(tok: *mut Tokenizer) {
    // Five assignments; nothing is freed and the grown capacities are kept.
    // In particular `argv[0]` is not restored to NULL, so a following parse
    // that publishes no word leaves the array unterminated (ERR-input-38).
    // SAFETY: `tok` must be non-NULL.
    unsafe { &mut *tok }.reset();
}

// [spec:libedit:def:histedit.tok-line-fn]
// [spec:libedit:sem:histedit.tok-line-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
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
    // SAFETY: the caller's live `LineInfo` contract is stated above.
    let (input, cursor) = unsafe { tokenizer_input(line) };
    let outcome = tok.tokenize(input, cursor);
    // SAFETY: the caller provides the mandatory writable outputs; cursor
    // outputs are nullable by contract.
    unsafe { publish_tokenize_outcome(outcome, argc, argv, cursorc, cursoro) }
}

// [spec:libedit:def:histedit.tok-str-fn]
// [spec:libedit:sem:histedit.tok-str-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
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
    let input = unsafe { cstr(line) }.unwrap_or(&[]);
    let outcome = tok.tokenize(input, None);
    // SAFETY: `argc` and `argv` are mandatory writable outputs.
    unsafe {
        publish_tokenize_outcome(
            outcome,
            argc,
            argv,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    }
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_wgets(el: *mut EditLine, nread: *mut c_int) -> *const WcharT {
    // `nread` may be NULL, in which case the core substitutes its own scratch
    // count and discards it.
    // SAFETY: `el` must be non-NULL; `nread` is null or writable.
    let mut ignored = 0;
    let nread = if nread.is_null() {
        &mut ignored
    } else {
        unsafe { &mut *nread }
    };
    *nread = 0;
    if el.is_null() {
        return core::ptr::null();
    }
    if !unsafe { (&*el).is_tty() } || !unsafe { (&*el).editing_enabled() } {
        if !unsafe { (&*el).unbuffered() } || !unsafe { (&*el).is_tty() } {
            unsafe { (&mut *el).reset_line() };
        }
        match unsafe { read_unedited(el) } {
            Ok(true) => {
                let line = unsafe { (&mut *el).publish_wide_line() };
                *nread = unsafe { (*line).lastchar.offset_from((*line).buffer) } as c_int;
                return unsafe { (*line).buffer };
            }
            Ok(false) => return core::ptr::null(),
            Err(()) => {
                *nread = if unsafe { (&*el).is_tty() } { -1 } else { 0 };
                return core::ptr::null();
            }
        }
    }
    if !unsafe { (&*el).unbuffered() } {
        unsafe { (&mut *el).reset_line() };
    }
    let result = match unsafe { drive_read(el) } {
        Ok(result) => result,
        Err(()) => {
            *nread = -1;
            return core::ptr::null();
        }
    };
    match result {
        ReadResult::Accepted(line) => {
            if !unsafe { (&mut *el).finish_accepted_line(line) } {
                *nread = -1;
                return core::ptr::null();
            }
        }
        ReadResult::Character(unit) => {
            if !unsafe { (&mut *el).replace_line(core::iter::once(unit).collect()) } {
                *nread = -1;
                return core::ptr::null();
            }
        }
        ReadResult::EndOfInput => return core::ptr::null(),
        ReadResult::Interrupted(_) => {
            *nread = -1;
            return core::ptr::null();
        }
    }
    let line = unsafe { (&mut *el).publish_wide_line() };
    *nread = unsafe { (*line).lastchar.offset_from((*line).buffer) } as c_int;
    unsafe { (*line).buffer }
}

// [spec:libedit:def:histedit.el-wgetc-fn]
// [spec:libedit:sem:histedit.el-wgetc-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_wgetc(el: *mut EditLine, wc: *mut WcharT) -> c_int {
    // Returned verbatim, including the 0 the core reports when `tty_rawmode`
    // fails — a terminal-setup failure indistinguishable from end of file
    // (ERR-input-24). Not corrected to -1 here.
    // SAFETY: `el` and `wc` must both be non-NULL.
    if el.is_null() || wc.is_null() {
        return -1;
    }
    unsafe { read_wide_character(el, wc) }
}

// [spec:libedit:def:histedit.el-wpush-fn]
// [spec:libedit:sem:histedit.el-wpush-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_wpush(el: *mut EditLine, str_: *const WcharT) {
    // A NULL string, a full stack or a failed duplication are all reported to
    // the user as a beep and to the caller not at all.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    let pushed = unsafe { wstr(str_) }.is_some_and(|input| unsafe { (&mut *el).push_input(input) });
    if !pushed {
        unsafe { (&mut *el).beep() };
    }
}

// [spec:libedit:def:histedit.el-wparse-fn]
// [spec:libedit:sem:histedit.el-wparse-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_wparse(
    el: *mut EditLine,
    argc: c_int,
    argv: *mut *const WcharT,
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
    if el.is_null() || argc < 1 {
        return -1;
    }
    unsafe { dispatch_editrc(el, &words) }
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
#[doc = include_str!("ffi_safety.md")]
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
            el.set_prompt_wide(op == EL_RPROMPT, p, 0);
            0
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
            el.set_prompt_wide(op == EL_RPROMPT_ESC, p, c as u32);
            0
        }

        // An `el_zfunc_t` then a `void *`: the resize callback and its
        // cookie. Always 0. Invoked from `el_resize`, from buffer growth and
        // from `el_line`.
        EL_RESIZE => {
            // SAFETY: the op's first argument is an `el_zfunc_t`, per the
            // header; the second is an opaque cookie stored verbatim.
            let p = unsafe { fn_arg::<ElZfuncT>(&mut ap) };
            let arg = unsafe { ap.next_arg::<*mut c_void>() };
            el.set_resize_callback(p, arg);
            0
        }

        // An `el_afunc_t` then a `void *`: the alias-expansion callback and
        // its cookie — narrow `char` even in the wide API. Always 0.
        EL_ALIAS_TEXT => {
            // SAFETY: the op's first argument is an `el_afunc_t`; the second
            // is an opaque cookie stored verbatim.
            let p = unsafe { fn_arg::<ElAfuncT>(&mut ap) };
            let arg = unsafe { ap.next_arg::<*mut c_void>() };
            el.set_alias_callback(p, arg);
            0
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
            let name = bytes
                .map(String::from_utf8_lossy)
                .map(std::borrow::Cow::into_owned)
                .or_else(|| {
                    crate::adapter::secure_environment("TERM")
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                })
                .unwrap_or_else(|| "dumb".to_owned());
            el.set_terminal_name(&name)
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
                Some(e) if e == &EDITOR_EMACS[..5] => {
                    el.set_editor(nshedit::domain::EditingMode::Emacs);
                    0
                }
                Some(e) if e == &EDITOR_VI[..2] => {
                    el.set_editor(nshedit::domain::EditingMode::Vi);
                    0
                }
                _ => -1,
            }
        }

        // One `int`. Inline in the C's own dispatch, so inline here.
        EL_SIGNAL => {
            // SAFETY: the op's argument is an `int`.
            el.set_handle_signals(unsafe { ap.next_arg::<c_int>() } != 0);
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
            let _ = argc;
            match op {
                EL_BIND => el.bind_command(&argv),
                EL_SETTY => el.set_tty_modes(&argv),
                _ => el.terminal_command(&argv),
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
            c_int::from(!el.add_command(name, help, func)).wrapping_neg()
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
            let narrow = el.narrow_history() && crate::conversion::max_multibyte_length() != 1;
            let rv = if el.set_history_callback(f, ptr, narrow) {
                0
            } else {
                -1
            };
            // The flag clear is not conditional on `rv` in the C either.
            if crate::conversion::max_multibyte_length() == 1 {
                el.set_narrow_history(false);
            }
            rv
        }

        // One `int`, the EINTR-recovery flag. Inline in the C.
        EL_SAFEREAD => {
            // SAFETY: the op's argument is an `int`.
            el.set_safe_read(unsafe { ap.next_arg::<c_int>() } != 0);
            0
        }

        // One `int`, inverted: non-zero enables editing. Inline in the C.
        EL_EDITMODE => {
            // SAFETY: the op's argument is an `int`.
            el.set_editing_enabled(unsafe { ap.next_arg::<c_int>() } != 0);
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
            el.set_read_callback(rc);
            0
        }

        // One `void *`, stored verbatim and never dereferenced. Inline in the
        // C, and this arm leaves `rv` at 0.
        EL_CLIENTDATA => {
            // SAFETY: the op's argument is a `void *`.
            el.set_client_data(unsafe { ap.next_arg::<*mut c_void>() });
            0
        }

        // One `int`. A 0 -> non-zero transition sets UNBUFFERED and runs the
        // read-prepare sequence; the reverse clears it and runs read-finish;
        // setting it to the value it already holds does nothing. Always 0.
        EL_UNBUFFERED => {
            // SAFETY: the op's argument is an `int`.
            let on = unsafe { ap.next_arg::<c_int>() } != 0;
            let was = el.unbuffered();
            // The flag is written *before* the sequence runs, and both
            // sequences read it: `read_prepare` enters raw mode only when
            // UNBUFFERED is set and editing is enabled, and `read_finish`
            // leaves the tty raw when it is set — which is why
            // `read_finish` here returns it to cooked mode.
            if on && !was {
                el.set_unbuffered(true);
                if el.editing_enabled() {
                    let _ = el.set_terminal_mode(nshedit::domain::TerminalMode::Editing);
                }
            } else if !on && was {
                el.set_unbuffered(false);
                let _ = el.set_terminal_mode(nshedit::domain::TerminalMode::Cooked);
            }
            0
        }

        // One `int`: non-zero raw, zero cooked, tty errors discarded.
        // Always 0. There is no matching get.
        EL_PREP_TERM => {
            // Both results are discarded, so a terminal that refused the mode
            // change is indistinguishable from one that took it.
            // SAFETY: the op's argument is an `int`.
            if unsafe { ap.next_arg::<c_int>() } != 0 {
                let _ = el.set_terminal_mode(nshedit::domain::TerminalMode::Editing);
            } else {
                let _ = el.set_terminal_mode(nshedit::domain::TerminalMode::Cooked);
            }
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
            if (0..=2).contains(&what) && el.set_stream(what as usize, fp, cstdio::fileno_of(fp)) {
                0
            } else {
                -1
            }
        }

        // No further arguments: clear the recorded display, redraw prompt and
        // line, flush. Returns 0 — the arm assigns nothing to `rv`.
        EL_REFRESH => {
            // The next driver display is a complete native frame, so there
            // is no separate screen cache to clear or flush here.
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
                Some(w) => {
                    el.set_word_characters(w);
                    0
                }
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
            el.set_environment_callback(
                f.filter(|f| !core::ptr::fn_addr_eq(*f, default_getenv as FuncT)),
            );
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
#[doc = include_str!("ffi_safety.md")]
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
            let Some(prf) = prf else {
                return -1;
            };
            let (callback, _) = el.prompt_wide(op != EL_PROMPT);
            *prf = Some(callback);
            0
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
            let Some(prf) = prf else {
                return -1;
            };
            let (callback, escape) = el.prompt_wide(op != EL_PROMPT);
            *prf = Some(callback);
            if let Some(c) = c {
                *c = escape;
            }
            0
        }

        // One `const wchar_t **`, set to the static `L\"emacs\"`/`L\"vi\"`.
        EL_EDITOR => {
            // SAFETY: the op's argument is a `const wchar_t **`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            // `map_get_editor`'s NULL check, which its `&mut` cannot make.
            if out.is_null() {
                return -1;
            }
            let p = if el.editor_is_vi() {
                EDITOR_VI.as_ptr()
            } else {
                EDITOR_EMACS.as_ptr()
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
            unsafe { *out.cast::<c_int>() = c_int::from(el.handle_signals()) };
            0
        }

        // One `int *`, set to the logical negation of EDIT_DISABLED, so
        // genuinely 0 or 1 and inverted to match the setter's polarity.
        EL_EDITMODE => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = c_int::from(el.editing_enabled()) };
            0
        }

        // One `int *`, set to the raw FIXIO bit — **256**, not 1. A caller
        // comparing it against 1 gets the wrong answer. Frozen behaviour;
        // deliberately not normalised here.
        EL_SAFEREAD => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = if el.safe_read() { FIXIO } else { 0 } };
            0
        }

        // One `const char **`, set to the loaded terminal type name — narrow
        // bytes even in the wide API. Always 0.
        EL_TERMINAL => {
            // SAFETY: the op's argument is a `const char **`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            // The C hands out `t_name` itself, a borrowed pointer a later
            // `terminal_set` replaces; the core's `String` carries no
            // terminator, so a NUL-terminated copy is owned here per editor.
            // Divergence worth naming: this copy is replaced on every call,
            // so a pointer from an earlier `el_wget(EL_TERMINAL)` is
            // invalidated by a later one — where the C's survives until the
            // type is reloaded. A name containing an interior NUL, which the
            // C could not have produced, reports NULL.
            let p = el.terminal_name_ptr();
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
            let _ = GETTC;
            let Some(name) = (unsafe { cbytes(name.cast()) }) else {
                return -1;
            };
            unsafe { el.get_terminal_capability(name, out) }
        }

        // One `el_rfunc_t *`, set to `EL_BUILTIN_GETCFN` (NULL) when the
        // builtin reader is installed — so a set/get round trip normalises
        // the builtin to NULL rather than reporting its address.
        EL_GETCFN => {
            // As in the setter, an uninitialised read subsystem is the C's
            // NULL dereference and is defined here; it reports the builtin,
            // which is what a reader installed after `read_init` would be.
            let f = el.read_callback();
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
            unsafe { *out.cast::<*mut c_void>() = el.client_data() };
            0
        }

        // One `int *`, normalised to 0 or 1 — unlike EL_SIGNAL and
        // EL_SAFEREAD.
        EL_UNBUFFERED => {
            // SAFETY: the op's argument is an `int *` the caller supplied.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            unsafe { *out.cast::<c_int>() = c_int::from(el.unbuffered()) };
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
            let Some(fp) = usize::try_from(what)
                .ok()
                .and_then(|index| el.stream(index))
            else {
                return -1;
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
            // As `EL_TERMINAL`: the C lends out its own buffer, the core
            // hands back an owned copy with no terminator, and the
            // terminated one is owned here per editor and replaced on every
            // call. The C's buffer instead survives until the set is
            // reinstalled — by `EL_WORDCHARS`, `bind -v`/`bind -e`, an
            // `EL_EDITOR` switch or `el_end`, each of which frees it and any
            // of which the C's caller must not hold a pointer across.
            let p = el.publish_word_characters();
            // SAFETY: the out-pointer read above.
            unsafe { *out.cast::<*const u32>() = p };
            0
        }

        // One `func_t *`, set to the installed environment accessor. Always 0.
        // The C stores `secure_getenv` itself at construction, so a fresh
        // handle reports a non-NULL address here; the core keeps `None` for
        // that state, which is why [`default_getenv`] exists.
        EL_GETENV => {
            let f: FuncT = el.environment_callback().unwrap_or(default_getenv);
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
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_cursor(el: *mut EditLine, n: c_int) -> c_int {
    // The C advances the cursor pointer and only then clamps, transiently
    // forming an out-of-range pointer (ERR-buffer-11, undefined). The core
    // computes the saturating offset instead, which is the same observable
    // result: a character index in `0 ..= lastchar - buffer`.
    // SAFETY: `el` must be non-NULL.
    unsafe { &mut *el }.move_cursor(n)
}

// [spec:libedit:def:histedit.el-wline-fn]
// [spec:libedit:sem:histedit.el-wline-fn]
// [spec:libedit:def:el.el-wline-fn]
// [spec:libedit:sem:el.el-wline-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
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
    el.publish_wide_line()
}

// [spec:libedit:def:histedit.el-winsertstr-fn]
// [spec:libedit:sem:histedit.el-winsertstr-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_winsertstr(el: *mut EditLine, str_: *const WcharT) -> c_int {
    // A NULL string and an empty one are the same -1, so NULL becomes the
    // empty slice rather than a separate check.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    unsafe { &mut *el }.insert_wide(unsafe { wstr(str_) }.unwrap_or(&[]))
}

// [spec:libedit:def:histedit.el-wreplacestr-fn]
// [spec:libedit:sem:histedit.el-wreplacestr-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_wreplacestr(el: *mut EditLine, str_: *const WcharT) -> c_int {
    // As `el_winsertstr`: NULL and empty are both -1.
    // SAFETY: `el` must be non-NULL; `str_` is null or NUL-terminated.
    unsafe { &mut *el }.replace_wide(unsafe { wstr(str_) }.unwrap_or(&[]))
}

// [spec:libedit:def:histedit.history-winit-fn]
// [spec:libedit:sem:histedit.history-winit-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn history_winit() -> *mut HistoryW {
    // The handle is raw all the way through: `H_END` frees it from inside
    // `history_w`, which no borrow could express. NULL is the C's allocation
    // failure. The retained maximum starts at 0, so `H_SETSIZE` is required
    // before any `H_ENTER` keeps anything.
    HistoryWideOwner::new_raw().cast()
}

// [spec:libedit:def:histedit.history-wend-fn]
// [spec:libedit:sem:histedit.history-wend-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn history_wend(h: *mut HistoryW) {
    // `h` must be non-NULL; there is no check and calling it twice is a
    // double free. Every `HistEventW.str` from this handle is dangling
    // afterwards except those from `H_DEL`/`H_DELDATA`, which the caller owns.
    // SAFETY: `h` is NULL or the allocation returned by `history_winit`.
    unsafe { crate::history::end(h.cast::<HistoryWideOwner>()) };
}

/// The varargs tail of `history_w`/`history`, walked into the ABI adapter's
/// closed [`DispatchArg`] form.
///
/// The op enumeration below is the whole of this function: each code names how
/// many trailing arguments it has and what they are, which is what the core's
/// [`DispatchArg`] is a closed form of, and it is also exactly how many `va_arg`
/// reads the C makes. Only one entry in that table depends on the
/// instantiation — the five string ops, whose argument is `const wchar_t *`
/// wide and `const char *` narrow — so the table is written once and `C` picks
/// the spelling. `H_LOAD` and `H_SAVE` take a `const char *` path in *both*,
/// because the on-disk format is bytes and is frozen.
///
/// # Safety
/// `h` and `ev` must be a live handle and a writable event of this
/// instantiation, and the tail must carry what the op code says.
unsafe fn history_dispatch<C: HistoryChar>(
    h: *mut HistoryHandle<C>,
    ev: *mut HistEventGen<C>,
    op: c_int,
    mut ap: VaList<'_>,
) -> c_int {
    use crate::cdecl::histedit::{
        H_ADD, H_APPEND, H_CLEAR, H_CURR, H_DEL, H_DELDATA, H_END, H_ENTER, H_FIRST, H_FUNC,
        H_GETSIZE, H_GETUNIQUE, H_LAST, H_LOAD, H_NEXT, H_NEXT_EVDATA, H_NEXT_EVENT, H_NEXT_STR,
        H_NSAVE_FP, H_PREV, H_PREV_EVENT, H_PREV_STR, H_REPLACE, H_SAVE, H_SAVE_FP, H_SET,
        H_SETSIZE, H_SETUNIQUE,
    };

    // Neither `h` nor `ev` may be NULL; both are dereferenced unchecked, as
    // in the C.
    // The caller's stream for `H_SAVE_FP`/`H_NSAVE_FP`, hoisted for the same
    // reason: the adapter borrows it for the length of the call and never
    // past it, so no `FILE *` outlives the operation that was handed it.
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
            DispatchArg::Callbacks(CallbackSet {
                reference: unsafe { ap.next_arg::<*mut c_void>() },
                first: unsafe { fn_arg::<GetCallback<C>>(&mut ap) },
                next: unsafe { fn_arg::<GetCallback<C>>(&mut ap) },
                last: unsafe { fn_arg::<GetCallback<C>>(&mut ap) },
                previous: unsafe { fn_arg::<GetCallback<C>>(&mut ap) },
                current: unsafe { fn_arg::<GetCallback<C>>(&mut ap) },
                select: unsafe { fn_arg::<SelectCallback<C>>(&mut ap) },
                clear: unsafe { fn_arg::<ClearCallback<C>>(&mut ap) },
                enter: unsafe { fn_arg::<EnterCallback<C>>(&mut ap) },
                add: unsafe { fn_arg::<EnterCallback<C>>(&mut ap) },
                delete: unsafe { fn_arg::<SelectCallback<C>>(&mut ap) },
            })
        }

        // One `int`.
        // SAFETY: the op's argument is an `int`.
        H_SETSIZE | H_SET | H_SETUNIQUE | H_DEL | H_NEXT_EVENT | H_PREV_EVENT => {
            DispatchArg::Number(unsafe { ap.next_arg::<c_int>() })
        }

        // No trailing argument. `H_CURR` included: the header comment's
        // `, const int)` is wrong.
        H_GETSIZE | H_FIRST | H_LAST | H_PREV | H_NEXT | H_CURR | H_END | H_CLEAR | H_GETUNIQUE => {
            DispatchArg::None
        }

        // One `const wchar_t *`.
        // SAFETY: the op's argument is a NUL-terminated string of this
        // instantiation's character type.
        H_ADD | H_ENTER | H_APPEND | H_NEXT_STR | H_PREV_STR => DispatchArg::Text(
            unsafe { ap.next_arg::<*mut c_void>() }
                .cast::<C>()
                .cast_const(),
        ),

        // One `const char *` filename — narrow in both instantiations,
        // because the on-disk format is bytes and is frozen.
        //
        H_LOAD | H_SAVE => {
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt;

            // SAFETY: the op's argument is a NUL-terminated path.
            let p = unsafe { ap.next_arg::<*mut c_void>() };
            let path = unsafe { cbytes(p.cast::<c_char>()) }
                .map(|bytes| std::path::Path::new(OsStr::from_bytes(bytes)));
            DispatchArg::Path(path)
        }

        // One `FILE *`, which the caller keeps and must close. The stream is
        // read and written here rather than in the core: see
        // [`crate::cstdio`] for why the descriptor behind it is not a
        // substitute.
        H_SAVE_FP => {
            // SAFETY: the op's argument is the caller's `FILE *`.
            let fp = unsafe { ap.next_arg::<*mut c_void>() };
            DispatchArg::Stream(SaveStream {
                at_start: cstdio::at_start(fp),
                output: fp_out.insert(CFileWriter::new(fp)),
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
            DispatchArg::LimitedStream(
                n,
                SaveStream {
                    at_start: cstdio::at_start(fp),
                    output: fp_out.insert(CFileWriter::new(fp)),
                },
            )
        }

        // `int` then `void **`. The pointer stays raw: `H_DELDATA` accepts
        // the magic `(void **)-1` meaning "position the cursor only".
        H_NEXT_EVDATA | H_DELDATA => {
            // SAFETY: the op's arguments are an `int` then a `void **`.
            let num = unsafe { ap.next_arg::<c_int>() };
            let d = unsafe { ap.next_arg::<*mut c_void>() };
            DispatchArg::EventData(num, d.cast::<*mut c_void>())
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
            DispatchArg::Replace(line.cast::<C>().cast_const(), d)
        }

        // Anything else reads no argument and comes back -1 with `ev` set to
        // code 1, "unknown error" — which the core's default arm does, so it
        // is dispatched rather than short-circuited here.
        _ => DispatchArg::None,
    };

    // Ownership on the way out is implemented by the ABI owner:
    // `H_DEL` and `H_DELDATA` hand the caller a string it owns and must free
    // (and which is NULL on an allocation failure); every other op's
    // `ev.str` points into libedit's storage or at a static message and must
    // not be freed.
    // SAFETY: this function's own handle/event/tail contract, now expressed
    // as one closed typed argument.
    unsafe { crate::history::dispatch(h, ev, op, arg) }
}

/// C: `int history_w(HistoryW *, HistEventW *, int, ...);`
// [spec:libedit:def:histedit.history-w-fn]
// [spec:libedit:sem:histedit.history-w-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
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
    unsafe { history_dispatch::<u32>(h.cast::<HistoryWideOwner>(), ev, op, ap) }
}

// [spec:libedit:def:histedit.tok-winit-fn]
// [spec:libedit:sem:histedit.tok-winit-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_winit(ifs: *const WcharT) -> *mut TokenizerW {
    // NULL selects the default `L"\t \n"`. The caller keeps ownership of its
    // string; the tokenizer owns everything it later hands back.
    // SAFETY: `ifs` is null or a NUL-terminated wide string.
    let separators = unsafe { wstr(ifs) };
    Box::into_raw(TokenizerW::from_wide(separators))
}

// [spec:libedit:def:histedit.tok-wend-fn]
// [spec:libedit:sem:histedit.tok-wend-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_wend(tok: *mut TokenizerW) {
    // `tok` must be non-NULL (no check) and must be a `TokenizerW`; every
    // `argv` array and word pointer from this tokenizer dangles afterwards,
    // so the materialised array goes with it.
    // SAFETY: `tok` is the ABI allocation `tok_winit` returned.
    drop(unsafe { Box::from_raw(tok) });
}

// [spec:libedit:def:histedit.tok-wreset-fn]
// [spec:libedit:sem:histedit.tok-wreset-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_wreset(tok: *mut TokenizerW) {
    // Five assignments; nothing is freed and the grown capacities are kept.
    // In particular `argv[0]` is not restored to NULL, so a following parse
    // that publishes no word leaves the array unterminated (ERR-input-38).
    // SAFETY: `tok` must be non-NULL.
    unsafe { &mut *tok }.reset();
}

// [spec:libedit:def:histedit.tok-wline-fn]
// [spec:libedit:sem:histedit.tok-wline-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_wline(
    tok: *mut TokenizerW,
    line: *const LineInfoW,
    argc: *mut c_int,
    argv: *mut *mut *const WcharT,
    cursorc: *mut c_int,
    cursoro: *mut c_int,
) -> c_int {
    // `tok` and `line` must be non-NULL and `line->buffer` must be non-NULL;
    // none is checked. The tokenizer is *not* reset — this appends, which is
    // how multi-line continuation works.
    // SAFETY: both are the caller's live objects.
    let tok = unsafe { &mut *tok };
    let line = unsafe { &*line };
    // SAFETY: the caller's live `LineInfoW` contract is stated above.
    let (input, cursor) = unsafe { tokenizer_input(line) };
    let outcome = tok.tokenize(input, cursor);
    // SAFETY: the caller provides the mandatory writable outputs; cursor
    // outputs are nullable by contract.
    unsafe { publish_tokenize_outcome(outcome, argc, argv, cursorc, cursoro) }
}

// [spec:libedit:def:histedit.tok-wstr-fn]
// [spec:libedit:sem:histedit.tok-wstr-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn tok_wstr(
    tok: *mut TokenizerW,
    line: *const WcharT,
    argc: *mut c_int,
    argv: *mut *mut *const WcharT,
) -> c_int {
    // A NUL-terminated string with no cursor: the core builds the `LineInfoW`
    // with `cursor == lastchar`, so the cursor never matches and both cursor
    // out-parameters are NULL. Does not reset the tokenizer either.
    // SAFETY: `tok` must be non-NULL, `line` non-NULL and NUL-terminated.
    let tok = unsafe { &mut *tok };
    let input = unsafe { wstr(line) }.unwrap_or(&[]);
    let outcome = tok.tokenize(input, None);
    // SAFETY: `argc` and `argv` are mandatory writable outputs.
    unsafe {
        publish_tokenize_outcome(
            outcome,
            argc,
            argv,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    }
}

#[cfg(test)]
mod tests;
