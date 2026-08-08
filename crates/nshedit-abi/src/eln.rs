//! The narrow-character entry points of `histedit.h`; rules in
//! `docs/spec/port/src/eln.md`.
//!
//! `eln.c` is a compatibility layer in the C itself — every function here
//! converts between the caller's multibyte strings and the wide interior,
//! through the `EditLine`'s legacy conversion buffer — so
//! `plan/decisions/idiomatic-core.md` places it in this crate rather than the
//! core. The pointers these functions hand back live in that buffer and stay
//! valid exactly until the next narrow call on the same editor.
//!
//! `histedit.h` declares the same nine symbols, with rules of its own. Those
//! declaration rules are carried in [`crate::histedit`], which re-exports
//! these definitions so that exactly one symbol of each name exists.
//!
//! # The shared buffer, and how the contract is honoured here
//!
//! `el_gets`, `el_line` and `el_get(EL_EDITOR / EL_WORDCHARS)` all hand back
//! `el->el_lgcyconv.cbuff` *itself*. That is not an accident to be papered
//! over with a copy: the C caller is promised a pointer that stays valid until
//! the next call which writes the narrow half, and code in the wild relies on
//! it (`readline.c`'s own `_resize_fun` does). So every one of them returns
//! [`nshedit::chartype::ct_encode_string`]'s slice `as_ptr()` — the start of
//! the adapter's `Vec<u8>` — and the next encode into the same `EditLine`
//! overwrites it, or reallocates and dangles it, exactly as `realloc` does in
//! the C. Nothing here copies, caches or leaks a second buffer, and the
//! decoding calls (`el_push`, `el_parse`, `el_insertstr`, `el_replacestr`)
//! touch only the wide half, so a string handed out earlier survives them.
//!
//! Both halves of the compatibility layer are complete: [`el_set`] forwards
//! the operations that need no conversion, decodes the string operations and
//! handles narrow callbacks; [`el_get`] mirrors that split and encodes its two
//! string results into the editor's shared legacy buffer.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use nshedit::chartype::{ct_decode_string, ct_enc_width, ct_encode_char, ct_encode_string};
use nshedit::el::NARROW_HISTORY;
use nshedit::hist::HistFunT;
use nshedit::prompt::ElPfuncT;

use crate::adapter::EditLine;
use crate::cdecl::histedit::LineInfo;
use crate::histedit::{
    EL_BIND, EL_CLIENTDATA, EL_ECHOTC, EL_EDITMODE, EL_EDITOR, EL_GETCFN, EL_GETFP, EL_GETTC,
    EL_HIST, EL_PREP_TERM, EL_PROMPT, EL_PROMPT_ESC, EL_RPROMPT, EL_RPROMPT_ESC, EL_SAFEREAD,
    EL_SETFP, EL_SETTC, EL_SETTY, EL_SIGNAL, EL_TELLTC, EL_TERMINAL, EL_UNBUFFERED, EL_WORDCHARS,
    el_wget_va, el_wgetc, el_wgets, el_winsertstr, el_wline, el_wparse, el_wpush, el_wreplacestr,
    el_wset_va,
};
use core::ffi::VaList;

/// C: `#define FROM_ELLINE 0x200` (`el.h`) — [`el_line`]'s re-entrancy guard.
///
/// Spelled out here because the core's copy is `pub(crate)`. It is the one
/// flag bit this file touches, it is set nowhere else in the library, and
/// `el_flags` itself is a public field, so duplicating the constant is enough.
const FROM_ELLINE: i32 = 0x200;

/// `wctob` through the core's locale-aware single-character encoder.
///
/// `None` is the C's `EOF`: `c` has no single-byte representation in the
/// initial shift state of the current `LC_CTYPE`.
fn wctob(c: u32) -> Option<u8> {
    let mut byte = [0u8; 1];
    (ct_encode_char(&mut byte, c) == 1).then_some(byte[0])
}

/// The C's `const wchar_t *` as a slice: everything up to, and not including,
/// the terminating `L'\0'`.
///
/// The walk is unbounded, which is the C's own reach — `ct_encode_string` and
/// `wcslen` read to the terminator and no further information exists at this
/// layer. `sem:eln.el-gets-fn` and `sem:eln.el-line-fn` both depend on that
/// reach: under `EL_UNBUFFERED` the terminator is past `lastchar`, and the
/// string running on into stale characters is the reproduced defect
/// (ERR-core-api-26).
///
/// # Safety
///
/// `p` must be non-NULL and point at a `L'\0'`-terminated wide string that
/// outlives the returned slice.
unsafe fn wide_upto_nul<'a>(p: *const u32) -> &'a [u32] {
    let mut n = 0usize;
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    unsafe { core::slice::from_raw_parts(p, n) }
}

/// Sum of [`ct_enc_width`] over the wide characters in `[from, to)` — the C's
/// two identical `for (p = winfo->buffer; p < ...; p++)` loops.
///
/// # Safety
///
/// `from` and `to` must point into one wide string, with `to` reachable from
/// `from`.
unsafe fn sum_enc_widths(from: *const u32, to: *const u32) -> usize {
    let mut total = 0usize;
    let mut p = from;
    while p < to {
        total += ct_enc_width(unsafe { *p });
        p = unsafe { p.add(1) };
    }
    total
}

/// The C's `const char *` as a byte slice, `None` for its NULL — the argument
/// `ct_decode_string` and `ct_decode_argv` take.
///
/// # Safety
///
/// `p` must be NULL or point at a NUL-terminated string that outlives the
/// returned slice.
unsafe fn bytes_upto_nul<'a>(p: *const c_char) -> Option<&'a [u8]> {
    if p.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(p) }.to_bytes())
    }
}

/// Decode one narrow string through `el->el_lgcyconv`, as every decoding entry
/// point in this file does, and hand back the C's `const wchar_t *`.
///
/// `ct_decode_string` returns the content without the terminator, which the
/// core still writes one element past the end, so the raw pointer is the
/// NUL-terminated wide string the wide entry points expect. NULL covers all
/// three of the C's failure causes — a NULL `str`, a byte sequence invalid in
/// the current locale, and a buffer that could not be grown — which the C
/// cannot tell apart either.
///
/// # Safety
///
/// `el` must be a valid `EditLine`, and `str_` NULL or a NUL-terminated
/// string. The result borrows `el->el_lgcyconv.wbuff` and is invalidated by
/// the next call that writes it.
unsafe fn decode_through_lgcyconv(el: *mut EditLine, str_: *const c_char) -> *const u32 {
    let bytes = unsafe { bytes_upto_nul(str_) };
    let conv = unsafe { (&mut *el).narrow_conversion_mut() };
    ct_decode_string(bytes, conv).map_or(ptr::null(), <[u32]>::as_ptr)
}

// [spec:libedit:def:eln.el-getc-fn]
// [spec:libedit:sem:eln.el-getc-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_getc(el: *mut EditLine, cp: *mut c_char) -> c_int {
    // Everything the read records — the `EILSEQ` of `sem:read.read-char-fn`,
    // the failing `read`'s own value — reaches the caller's `errno` through
    // this sample, and so does the `ERANGE` step 4 sets. The publish is on
    // every path because the C's writes are not all on failing ones: the
    // retry path of `sem:read.read-char-fn` restores `errno` mid-call and
    // returns a character.
    let mark = crate::errno::mark();

    // Step 1. `el` is not checked for NULL here and is not dereferenced here
    // either; the contract for a NULL editor belongs to `el_wgetc`.
    let mut wc: u32 = 0;
    let num_read = unsafe { el_wgetc(el, &raw mut wc) };

    // Step 2. The C stores through `cp` unconditionally and never checks it
    // (`sem:eln.el-getc-fn` step 2), so `el_getc(el, NULL)` is a null
    // dereference: undefined, hence defined here as storing nothing.
    // Everything else about the store is reproduced, including that it happens
    // before the early return — a caller that pre-loaded `*cp` finds it
    // clobbered even on EOF (ERR-core-api-27).
    if !cp.is_null() {
        unsafe { *cp = 0 };
    }

    // Step 3. 0 is "nothing available", negative is a read error, and `errno`
    // is left exactly as the underlying read set it.
    let ret = if num_read <= 0 {
        num_read
    } else {
        // Step 4. This is not a multibyte interface: in a UTF-8 locale every
        // character outside US-ASCII fails here, and the character is consumed
        // and lost, since `el_wgetc` has already popped it and there is no
        // pushback (ERR-core-api-27, disposition reproduce).
        match wctob(wc) {
            // The C's `errno = ERANGE`, the only errno this file produces
            // itself. It is written to the core's copy as well as the C's, so
            // that a later core read cannot disagree with what the caller
            // sees.
            None => {
                crate::errno::set(nshedit::errno::ERANGE);
                -1
            }
            // Returns 1, not `num_read` — the same value, since `el_wgetc`
            // only ever reports 1 on success. The sign of the stored `char`
            // for byte values above 127 is the platform's, as in the C.
            Some(b) => {
                if !cp.is_null() {
                    unsafe { *cp = b as c_char };
                }
                1
            }
        }
    };

    crate::errno::publish(mark);
    ret
}

// [spec:libedit:def:eln.el-push-fn]
// [spec:libedit:sem:eln.el-push-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_push(el: *mut EditLine, str_: *const c_char) {
    // The C does not check `el` and dereferences it for the conversion buffer
    // (ERR-core-api-05); defined here as doing nothing.
    if el.is_null() {
        return;
    }

    // Steps 1-3. Multibyte-to-wide decoding is used unconditionally — it does
    // the right thing under single-byte locales too — and the NULL that a NULL
    // `str`, an invalid byte sequence or a failed growth all produce is passed
    // straight through. `el_wpush` treats it as failure and beeps, so
    // `el_push(el, NULL)` is well-defined and audible. Only the wide half of
    // `el_lgcyconv` is written, so a `const char *` handed out earlier by
    // `el_gets`, `el_line` or `el_get` survives this call.
    let w = unsafe { decode_through_lgcyconv(el, str_) };
    unsafe { el_wpush(el, w) };
}

// [spec:libedit:def:eln.el-gets-fn]
// [spec:libedit:sem:eln.el-gets-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_gets(el: *mut EditLine, nread: *mut c_int) -> *const c_char {
    // The C dereferences `el` for the conversion buffer without checking it
    // (ERR-core-api-05); defined here as a NULL return.
    if el.is_null() {
        return ptr::null();
    }

    // `sem:read.el-wgets-fn` step 14 restores the failing read's `errno` on
    // the `*nread == -1` exit, and `sem:eln.el-gets-fn` step 1 states that
    // contract for this entry point too, so whatever the read recorded is
    // copied to the caller on the way out.
    let mark = crate::errno::mark();

    // Step 1. The wide call sets `*nread` to a count of wide characters, or
    // returns NULL having set it to 0 or -1. It tolerates a NULL `nread` by
    // substituting a local.
    let tmp = unsafe { el_wgets(el, nread) };

    if !tmp.is_null() {
        // Step 2, and the divergence the rule leads with: the wide entry point
        // tolerates `nread == NULL`, this one dereferences it the moment a
        // line comes back, so `el_gets(el, NULL)` faults where
        // `el_wgets(el, NULL)` is well defined (ERR-core-api-11). A null
        // dereference is undefined, not defined-but-wrong, so it is *defined*
        // here rather than reproduced: with nowhere to put the byte count the
        // rewrite is simply skipped, which is the same substitution the wide
        // side makes. Nothing else about the function changes — the line is
        // still read, still encoded, still returned.
        if !nread.is_null() {
            // Exactly `*nread` wide characters are measured, which is *not*
            // the length of the string step 3 encodes: that runs to the wide
            // terminator. They agree only when the line is terminated at
            // `*nread`, which the buffered read path guarantees and the
            // `EL_UNBUFFERED` path does not — there the returned byte string
            // runs past the reported length into characters left over from an
            // earlier, longer line (ERR-core-api-26, disposition reproduce).
            let n = unsafe { *nread };
            let mut nwread = 0usize;
            let mut i = 0;
            while i < n {
                // A character the locale cannot encode contributes 0, matching
                // step 3 dropping it.
                nwread += ct_enc_width(unsafe { *tmp.add(i as usize) });
                i += 1;
            }
            // The count excludes the terminator `ct_encode_string` appends.
            unsafe { *nread = nwread as c_int };
        }
    }

    // Step 3. The returned pointer is `el_lgcyconv.cbuff` itself, not a copy:
    // the next `el_gets`, `el_line`, `el_get(EL_EDITOR)` or
    // `el_get(EL_WORDCHARS)` on this editor overwrites the bytes and may move
    // the buffer. A NULL `tmp` short-circuits inside the encoder, so a failed
    // read leaves the buffer — and any pointer previously handed out — alone.
    // Allocation failure returns NULL *after* `*nread` has already been
    // rewritten, so a NULL return does not imply `*nread <= 0`.
    let s = if tmp.is_null() {
        None
    } else {
        Some(unsafe { wide_upto_nul(tmp) })
    };
    let conv = unsafe { (&mut *el).narrow_conversion_mut() };
    let out = ct_encode_string(s, conv).map_or(ptr::null(), |b| b.as_ptr().cast::<c_char>());

    crate::errno::publish(mark);
    out
}

// [spec:libedit:def:eln.el-parse-fn]
// [spec:libedit:sem:eln.el-parse-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_parse(
    el: *mut EditLine,
    argc: c_int,
    argv: *mut *const c_char,
) -> c_int {
    // Neither `el` nor `argv` is checked in the C: a NULL `el` is dereferenced
    // for the conversion buffer and a NULL `argv` with `argc > 0` faults
    // inside `ct_decode_argv` (ERR-core-api-05). Both are undefined, so both
    // are defined here as the -1 the caller already has to handle. A negative
    // `argc` is rejected for the same reason `ct_decode_argv`'s signature
    // rejects it (ERR-encoding-05); the C reaches `el_wparse`, which answers
    // -1 for `argc < 1` anyway.
    if el.is_null() || argc < 0 || (argc > 0 && argv.is_null()) {
        return -1;
    }

    // Step 1. The core's `ct_decode_argv` is `pub(crate)`, so the argument
    // vector is decoded here one string at a time through the same buffer —
    // `el_lgcyconv.wbuff` is still written and still grown, and `cbuff` is
    // still untouched, so a `const char *` handed out earlier by `el_gets`,
    // `el_line` or `el_get` survives an `el_parse`. What differs is invisible
    // to a C caller: the strings are copied out into owned vectors instead of
    // being left packed end to end in `wbuff`, because a slice borrowed from
    // that buffer cannot survive the decode of the next argument. Nothing
    // retains a pointer into `wbuff` past this call in the C either — the
    // parser's callees copy — so the only difference is what garbage the
    // buffer holds afterwards.
    //
    // A NULL element is passed through as a NULL element rather than being
    // handed to `mbstowcs`, and there is no 20-entry cap: exactly `argc`
    // arguments are decoded.
    // Grown rather than reserved up front: the C's `el_calloc(argc + 1, ...)`
    // answers NULL for an absurd `argc` and falls out at step 2, where
    // `Vec::with_capacity` would abort the process instead.
    let mut owned: Vec<Option<Vec<u32>>> = Vec::new();
    for i in 0..argc as usize {
        let Some(bytes) = (unsafe { bytes_upto_nul(*argv.add(i)) }) else {
            owned.push(None);
            continue;
        };
        let conv = unsafe { (&mut *el).narrow_conversion_mut() };
        match ct_decode_string(Some(bytes), conv) {
            Some(w) => {
                let mut v = Vec::with_capacity(w.len() + 1);
                v.extend_from_slice(w);
                v.push(0);
                owned.push(Some(v));
            }
            // Step 2: allocation failure, or an argument invalid in the
            // current locale, is -1 without calling `el_wparse`.
            None => return -1,
        }
    }

    // The C's `calloc`ed, NULL-terminated array of `argc + 1` pointers.
    let mut wargv: Vec<*const u32> = owned
        .iter()
        .map(|a| a.as_ref().map_or(ptr::null(), |v| v.as_ptr()))
        .collect();
    wargv.push(ptr::null());

    // Steps 3 to 5. `el_parse` interprets none of what the wide parser does
    // with the vector, and returns its status unchanged. Step 4's `el_free`
    // frees the pointer array only; here both it and the decoded strings go
    // out of scope, which no caller can observe.
    unsafe { el_wparse(el, argc, wargv.as_mut_ptr()) }
}

/// C: `int el_set(EditLine *el, int op, ...);`
///
/// Genuinely variadic, as `histedit.h` declares it.
// [spec:libedit:def:eln.el-set-fn]
// [spec:libedit:sem:eln.el-set-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_set(el: *mut EditLine, op: c_int, ap: ...) -> c_int {
    // Step 1: a NULL editor is -1 without touching the varargs, the check
    // sitting above `va_start` in the C. Reproduced exactly.
    if el.is_null() {
        return -1;
    }

    // SAFETY: `el` is non-null and is the caller's live handle, and the tail
    // carries what `op` says it carries.
    unsafe { el_set_va(&mut *el, op, ap) }
}

/// The ops `sem:eln.el-set-fn` forwards to `el_wset` with nothing converted.
///
/// The C's arms for these are `return el_wset(el, op, va_arg(...))` — one
/// argument read and handed straight on — so forwarding the whole variadic
/// tail is the same thing without reading it twice. `el_wset_va` then reads
/// it with the type its own arm declares, which is where the type is known.
///
/// `EL_GETCFN` is in this set and is worth the note: `el_rfunc_t` is
/// `int (*)(EditLine *, wchar_t *)` in BOTH APIs, so a reader installed
/// through the narrow entry point is still called with a `wchar_t *` to fill
/// in. The C carries the same asymmetry.
const FORWARDED_TO_WSET: &[c_int] = &[
    EL_TERMINAL,
    EL_SIGNAL,
    EL_EDITMODE,
    EL_SAFEREAD,
    EL_UNBUFFERED,
    EL_PREP_TERM,
    EL_GETCFN,
    EL_CLIENTDATA,
    EL_SETFP,
];

/// The C's `const char *argv[20]` bounds the collection loop at
/// `__arraycount(argv) - 1`, so eighteen caller arguments plus the command
/// word. ERR-core-api-07 records that `el_wset`'s cap is nineteen and that
/// the two differ; both are reproduced rather than reconciled.
const MAX_LIST_ARGS: usize = 19;

/// The command word the C writes into `wargv[0]` for each list op, or `None`
/// when `op` is not one of them.
fn list_op_command(op: c_int) -> Option<&'static [u32]> {
    const BIND: &[u32] = &[0x62, 0x69, 0x6e, 0x64];
    const TELLTC: &[u32] = &[0x74, 0x65, 0x6c, 0x6c, 0x74, 0x63];
    const SETTC: &[u32] = &[0x73, 0x65, 0x74, 0x74, 0x63];
    const ECHOTC: &[u32] = &[0x65, 0x63, 0x68, 0x6f, 0x74, 0x63];
    const SETTY: &[u32] = &[0x73, 0x65, 0x74, 0x74, 0x79];
    Some(match op {
        EL_BIND => BIND,
        EL_TELLTC => TELLTC,
        EL_SETTC => SETTC,
        EL_ECHOTC => ECHOTC,
        EL_SETTY => SETTY,
        _ => return None,
    })
}

/// [`el_set`]'s dispatch, out of the variadic frame.
///
/// # Safety
/// The tail must carry the arguments the selected `op` defines, in order.
unsafe fn el_set_va(el: &mut EditLine, op: c_int, mut ap: VaList<'_>) -> c_int {
    if FORWARDED_TO_WSET.contains(&op) {
        // SAFETY: the tail is untouched and carries what this op declares,
        // which is the same argument the wide arm reads.
        return unsafe { el_wset_va(el, op, ap) };
    }

    // The list ops. The C cannot forward these to `el_wset` — it says so in
    // a comment — because the decoded vector is a `wchar_t **` and there is
    // no portable way to pass one back through a `...`. So its narrow arm
    // reimplements the body, and so does this.
    //
    // ERR-core-api-07 lives here: the loop runs `i < __arraycount(argv) - 1`
    // over a `const char *argv[20]`, so at most EIGHTEEN caller arguments are
    // read, and `el_wset`'s own cap is nineteen. Both are reproduced. A
    // caller that passes no NULL sentinel is the undefined half of that
    // entry, and the cap is what bounds it.
    if let Some(cmd) = list_op_command(op) {
        let mut owned: Vec<Vec<u32>> = Vec::new();
        for _ in 1..MAX_LIST_ARGS {
            // SAFETY: the op's tail is a NULL-terminated run of byte strings.
            let p = unsafe { ap.next_arg::<*const c_char>() };
            if p.is_null() {
                break;
            }
            // SAFETY: `p` is non-null and NUL-terminated.
            let Some(bytes) = (unsafe { bytes_upto_nul(p) }) else {
                return -1;
            };
            let el_ptr: *mut EditLine = el;
            // SAFETY: `el_ptr` is live for the call.
            let conv = unsafe { (&mut *el_ptr).narrow_conversion_mut() };
            // C: `ct_decode_argv`, whose NULL return is -1 without calling
            // the handler.
            let Some(w) = ct_decode_string(Some(bytes), conv) else {
                return -1;
            };
            let mut v = Vec::with_capacity(w.len() + 1);
            v.extend_from_slice(w);
            v.push(0);
            owned.push(v);
        }

        // C: `wargv[0] = L"bind"` and friends, overwriting the NULL the
        // collection loop left at index 0. `argc` is `i`, the number of
        // arguments read plus the command word.
        let mut argv: Vec<&[u32]> = Vec::with_capacity(owned.len() + 1);
        argv.push(cmd);
        argv.extend(owned.iter().map(|v| &v[..v.len() - 1]));
        let argc = argv.len() as c_int;

        return match op {
            EL_BIND => nshedit::map::map_bind(el, argc, &argv),
            EL_TELLTC => nshedit::terminal::terminal_telltc(el, argc, &argv),
            EL_SETTC => nshedit::terminal::terminal_settc(el, argc, &argv),
            EL_ECHOTC => nshedit::terminal::terminal_echotc(el, argc, &argv),
            _ => nshedit::tty::tty_stty(el, argc, &argv),
        };
    }

    // C: decode, then hand the wide string to the same core item the wide arm
    // calls. ERR-core-api-09, disposition define: the C forwards unchecked and
    // dereferences NULL inside `wcscmp`/`wcsdup` when the argument is NULL or
    // does not decode in the current locale, so both are rejected here with -1
    // instead.
    //
    // The decode goes through `el_lgcyconv`, and that is the point rather than
    // a convenience: `sem:eln.el-set-fn` requires every conversion to use the
    // WIDE half, so no `el_set` invalidates a `const char *` a caller is still
    // holding from `el_gets`, `el_line` or `el_get`.
    // `EL_HIST` calls `hist_set` directly rather than going through
    // `el_wset`, and then sets `NARROW_HISTORY` UNCONDITIONALLY. That is the
    // flag's only set site: `el_wset` never sets it and clears it when
    // `MB_CUR_MAX == 1`. ERR-core-api-16, disposition reproduce — a program
    // that installs its history through the narrow entry point gets narrow
    // history whatever the locale says, and one that uses the wide entry
    // point gets it cleared.
    if op == EL_HIST {
        // SAFETY: the op's arguments are a `hist_fun_t` and an opaque cookie.
        let f = unsafe { crate::histedit::fn_arg::<HistFunT>(&mut ap) };
        let ptr = unsafe { ap.next_arg::<*mut c_void>() };
        // The same settings hook the wide arm installs: `hist_command` reaches
        // it whichever entry point set the history, and the narrow store's -1
        // comes from the NARROW_HISTORY check ahead of it rather than from an
        // absent hook.
        let rv = nshedit::hist::hist_set(el, f, ptr, Some(crate::history::hist_settings));
        el.el_flags |= NARROW_HISTORY;
        return rv;
    }

    // The prompt ops. Not forwardable, and the reason is one argument: the
    // narrow arm passes `wide = 0` where `el_wset` passes 1, which is the
    // only difference between them and decides how `prompt_print` measures
    // what it draws.
    //
    // The escape character arrives as a character CODE, not a byte to
    // decode — `el_pfunc_t` is narrow in both APIs, so nothing here is
    // converted. `EL_PROMPT`/`EL_RPROMPT` pass 0 for it unconditionally,
    // which erases one installed earlier through the `_ESC` form
    // (ERR-core-api-36, reproduced inside `prompt_set`).
    if op == EL_PROMPT || op == EL_RPROMPT || op == EL_PROMPT_ESC || op == EL_RPROMPT_ESC {
        // SAFETY: the op's first argument is an `el_pfunc_t`, per the header.
        let f = unsafe { crate::histedit::fn_arg::<ElPfuncT>(&mut ap) };
        let esc = if op == EL_PROMPT_ESC || op == EL_RPROMPT_ESC {
            // SAFETY: the op's second argument is an `int`.
            unsafe { ap.next_arg::<c_int>() as u32 }
        } else {
            0
        };
        return nshedit::prompt::prompt_set(el, f, esc, op, 0);
    }

    if op == EL_EDITOR || op == EL_WORDCHARS {
        // SAFETY: the op's argument is null or a NUL-terminated byte string.
        let p = unsafe { ap.next_arg::<*const c_char>() };
        if p.is_null() {
            return -1;
        }
        let el_ptr: *mut EditLine = el;
        // SAFETY: `el_ptr` is live and `p` is non-null and NUL-terminated.
        let wide = unsafe { decode_through_lgcyconv(el_ptr, p) };
        if wide.is_null() {
            return -1;
        }
        // SAFETY: the decode returns a NUL-terminated string in
        // `el_lgcyconv.wbuff`, which outlives this call.
        let s = unsafe { wide_upto_nul(wide) };
        return if op == EL_EDITOR {
            nshedit::map::map_set_editor(el, s)
        } else {
            nshedit::map::map_set_wordchars(el, s)
        };
    }

    -1
}

/// C: `int el_get(EditLine *el, int op, ...);`
///
/// Genuinely variadic, as `histedit.h` declares it.
// [spec:libedit:def:eln.el-get-fn]
// [spec:libedit:sem:eln.el-get-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_get(el: *mut EditLine, op: c_int, ap: ...) -> c_int {
    // Step 1, reproduced exactly: a NULL editor is -1 without touching the
    // varargs.
    if el.is_null() {
        return -1;
    }

    // SAFETY: `el` is non-null and is the caller's live handle, and the tail
    // carries what `op` says it carries.
    unsafe { el_get_va(&mut *el, op, ap) }
}

/// The `el_get` ops that forward to `el_wget` with nothing converted.
///
/// [`FORWARDED_TO_WSET`] minus `EL_SETFP`, which is set-only and so has no
/// `el_get` arm to reach.
const FORWARDED_TO_WGET: &[c_int] = &[
    EL_TERMINAL,
    EL_SIGNAL,
    EL_EDITMODE,
    EL_SAFEREAD,
    EL_UNBUFFERED,
    EL_PREP_TERM,
    EL_GETCFN,
    EL_CLIENTDATA,
    // Two more than the set on the way in, and neither is a narrowing.
    //
    // `EL_GETTC`'s narrow arm in the C builds a `char *argv[3]` and calls
    // `terminal_gettc` — which is the wide arm's body as well, because
    // `terminal_gettc` takes narrow `char *` in both APIs. There is nothing
    // to convert. `rl_get_screen_size` goes through it, and until this it
    // took the default arm and wrote through neither out-parameter:
    // `rl_get_screen_size(&rows, &cols)` left both at whatever the caller
    // had, where the C answers 24 and 80. Found by the readline driver.
    EL_GETTC,
    // `EL_GETFP` is `(int, FILE **)` and the C's narrow arm is
    // `el_wget(el, op, what, fpp)` with both read straight through.
    EL_GETFP,
];

/// [`el_get`]'s dispatch, out of the variadic frame.
///
/// # Safety
/// The tail must carry the out-parameter the selected `op` defines.
unsafe fn el_get_va(el: &mut EditLine, op: c_int, mut ap: VaList<'_>) -> c_int {
    if FORWARDED_TO_WGET.contains(&op) {
        // SAFETY: the tail is untouched and carries what this op declares,
        // which is the same out-parameter the wide arm writes through.
        return unsafe { el_wget_va(el, op, ap) };
    }

    match op {
        // The callback is stored in the same slot for the narrow and wide
        // interfaces, so no conversion is involved.
        EL_PROMPT | EL_RPROMPT => {
            // SAFETY: the selected operation takes an `el_pfunc_t *`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            let callback = unsafe { out.cast::<Option<ElPfuncT>>().as_mut() };
            nshedit::prompt::prompt_get(el, callback, None, op)
        }

        // The C gets a wide escape character from the shared prompt helper,
        // then stores only its low byte. A null byte out-parameter is caller
        // UB there; define it as a skipped store here.
        EL_PROMPT_ESC | EL_RPROMPT_ESC => {
            // SAFETY: the operation takes `el_pfunc_t *` then `char *`.
            let out = unsafe { ap.next_arg::<*mut c_void>() };
            let callback = unsafe { out.cast::<Option<ElPfuncT>>().as_mut() };
            let escape_out = unsafe { ap.next_arg::<*mut c_void>() }.cast::<c_char>();
            let mut escape = 0u32;
            let result = nshedit::prompt::prompt_get(el, callback, Some(&mut escape), op);
            if !escape_out.is_null() {
                // SAFETY: a non-null `char *` is the operation's out slot.
                unsafe { *escape_out = escape as c_char };
            }
            result
        }

        // These are the only getter operations that overwrite the narrow
        // half of `el_lgcyconv`. The returned pointer is the start of that
        // buffer and therefore has exactly the C API's invalidation lifetime.
        EL_EDITOR | EL_WORDCHARS => {
            // SAFETY: the selected operation takes a `const char **`.
            let out = unsafe { ap.next_arg::<*mut c_void>() }.cast::<*const c_char>();
            if out.is_null() {
                return -1;
            }

            let (result, narrow) = if op == EL_EDITOR {
                let mut editor: &'static [u32] = &[];
                let result = nshedit::map::map_get_editor(el, &mut editor);
                let narrow = (result == 0)
                    .then(|| ct_encode_string(Some(editor), el.narrow_conversion_mut()))
                    .flatten()
                    .map_or(ptr::null(), |bytes| bytes.as_ptr().cast());
                (result, narrow)
            } else {
                let mut wordchars = None;
                let result = nshedit::map::map_get_wordchars(el, &mut wordchars);
                let narrow = (result == 0)
                    .then(|| ct_encode_string(wordchars.as_deref(), el.narrow_conversion_mut()))
                    .flatten()
                    .map_or(ptr::null(), |bytes| bytes.as_ptr().cast());
                (result, narrow)
            };
            // SAFETY: `out` was checked above and points at the caller's slot.
            unsafe { *out = narrow };
            if el.narrow_conversion_mut().csize == 0 {
                -1
            } else {
                result
            }
        }

        // Unsupported and set-only operations take the C's default arm.
        _ => -1,
    }
}

// [spec:libedit:def:eln.el-line-fn]
// [spec:libedit:sem:eln.el-line-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_line(el: *mut EditLine) -> *const LineInfo {
    // The C dereferences `el` for the flags word and the embedded `LineInfo`
    // without checking it (ERR-core-api-05, which singles out `el_line(NULL)`
    // as producing a small non-NULL garbage pointer). Undefined, so defined
    // here as a NULL return.
    if el.is_null() {
        return ptr::null();
    }

    // Step 1. `winfo` is the live wide line state, and `info` is the single
    // `LineInfo` embedded in the `EditLine` — always the same address, shared
    // by every caller, so two live `LineInfo` views of one editor are
    // impossible. The struct itself is stable; its three pointers are not.
    let winfo = unsafe { el_wline(el) };
    let info = unsafe { (&mut *el).narrow_line_ptr() };

    // Step 2. `FROM_ELLINE` is set nowhere else in the library, so this fires
    // exactly when the application's resize callback calls back in, and
    // returns `info` with whatever it already holds, converting nothing and
    // not re-entering the callback.
    if unsafe { (&*el).el_flags } & FROM_ELLINE != 0 {
        return info;
    }
    unsafe { (&mut *el).el_flags |= FROM_ELLINE };

    let (buffer, cursor, lastchar) =
        unsafe { ((*winfo).buffer, (*winfo).cursor, (*winfo).lastchar) };

    // Step 3. The encode runs to the wide string's terminating `L'\0'`, not to
    // `lastchar`, and `info->buffer` points at `el_lgcyconv.cbuff` itself —
    // not a copy. The next `el_line`, `el_gets`, `el_get(EL_EDITOR)` or
    // `el_get(EL_WORDCHARS)` overwrites those bytes and may move the buffer,
    // invalidating all three fields together.
    let s = if buffer.is_null() {
        None
    } else {
        Some(unsafe { wide_upto_nul(buffer) })
    };
    let conv = unsafe { (&mut *el).narrow_conversion_mut() };
    let encoded = ct_encode_string(s, conv).map_or(ptr::null(), |b| b.as_ptr().cast::<c_char>());
    unsafe { (*info).buffer = encoded };

    // Step 4: two independently recomputed byte offsets, not measurements of
    // the encoded string. `info->lastchar` is therefore the byte width of the
    // wide prefix `[buffer, lastchar)`, which under `EL_UNBUFFERED` lands in
    // the middle of a longer, still NUL-terminated byte string
    // (ERR-core-api-26, disposition reproduce). Characters the locale cannot
    // encode contribute 0 to both the string and the offsets, so the two stay
    // consistent.
    //
    // On an allocation failure the C computes `NULL + offset` — undefined, and
    // ERR-core-api-10's disposition is to treat the case as unspecified rather
    // than reproduce the arithmetic. Here the two derived fields stay NULL, so
    // a caller sees a `LineInfo` that is uniformly NULL.
    if encoded.is_null() {
        unsafe {
            (*info).cursor = ptr::null();
            (*info).lastchar = ptr::null();
        }
    } else {
        let to_cursor = unsafe { sum_enc_widths(buffer, cursor) };
        let to_lastchar = unsafe { sum_enc_widths(buffer, lastchar) };
        unsafe {
            (*info).cursor = encoded.add(to_cursor);
            (*info).lastchar = encoded.add(to_lastchar);
        }
    }

    // Step 5, and the divergence that matters most: `el_wline` is a cast with
    // no side effects, while every non-nested `el_line` runs the client's
    // `EL_RESIZE` callback. `ch_enlargebufs` is the only other caller of that
    // hook, and clients use the pair to learn that line pointers may have
    // moved, so the call is observable and is kept. It runs *after* `info` is
    // fully populated, and a nested `el_line` from inside it takes the step-2
    // shortcut and receives exactly this `info`.
    let resizefun = unsafe { (&*el).el_chared.c_resizefun };
    if let Some(f) = resizefun {
        let arg = unsafe { (&*el).el_chared.c_resizearg };
        // SAFETY: `f` and `arg` were installed together by
        // `el_set(EL_RESIZE, f, arg)` against this very handle, and
        // `def:chared.el-zfunc-t-edit-line-void` makes `f` a C function taking
        // it. `el` is the caller's live `EditLine`.
        unsafe { f((&mut *el).compatibility_ptr(), arg) };
    }

    // Step 6.
    unsafe { (&mut *el).el_flags &= !FROM_ELLINE };
    info
}

// [spec:libedit:def:eln.el-insertstr-fn]
// [spec:libedit:sem:eln.el-insertstr-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_insertstr(el: *mut EditLine, str_: *const c_char) -> c_int {
    // The C does not check `el` and dereferences it for the conversion buffer
    // (ERR-core-api-05); defined here as the -1 the caller already handles.
    if el.is_null() {
        return -1;
    }

    // Steps 1-3. The decode result — possibly NULL — goes straight to
    // `el_winsertstr`, which rejects NULL and the empty string with -1, so a
    // NULL, empty or invalidly-encoded `str` is indistinguishable from an
    // insert that failed for lack of buffer space. There is no `errno`
    // contract. The characters are copied into the line buffer, so nothing
    // retains a pointer into `el_lgcyconv.wbuff`, and `cbuff` is untouched: a
    // `const char *` from `el_gets`, `el_line` or `el_get` survives this call.
    let w = unsafe { decode_through_lgcyconv(el, str_) };
    unsafe { el_winsertstr(el, w) }
}

// [spec:libedit:def:eln.el-replacestr-fn]
// [spec:libedit:sem:eln.el-replacestr-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn el_replacestr(el: *mut EditLine, str_: *const c_char) -> c_int {
    // As `el_insertstr`: the C checks nothing (ERR-core-api-05).
    if el.is_null() {
        return -1;
    }

    // Steps 1-3, identical to `el_insertstr` at this layer save for the callee:
    // `el_wreplacestr` replaces the *entire* line rather than inserting at the
    // cursor. NULL, empty and invalidly-encoded input are all -1, and
    // indistinguishable from a buffer-growth failure.
    let w = unsafe { decode_through_lgcyconv(el, str_) };
    unsafe { el_wreplacestr(el, w) }
}

#[cfg(test)]
mod tests;
