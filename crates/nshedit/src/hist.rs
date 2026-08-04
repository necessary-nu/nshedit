//! Ported from `src/hist.c`; rules live in `docs/spec/port/src/hist.md`.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::CStr;
use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;
use std::ptr;

use crate::chared::ch_enlargebufs;
use crate::chartype::{ct_decode_string, ct_encode_string};
use crate::el::{EditLine, ElActionT};
use crate::histedit::{HistEvent, HistEventW};
use crate::history::{HistoryArg, HistoryW, history_w};
use crate::vis::strnvis;

// Constants the C reaches through its headers. None of `el.h`, `map.h`,
// `histedit.h` or `vis.h` has a Rust home that publishes these yet, so they
// are private here; idiomatization should fold each into the module that ends
// up owning its header.

/// C: `el.h` — `#define NARROW_HISTORY 0x040`.
const NARROW_HISTORY: i32 = 0x040;
/// C: `el.h` — `#define EL_BUFSIZ ((size_t)1024)`.
const EL_BUFSIZ: usize = 1024;
/// C: `map.h` — `#define MAP_VI 1`.
const MAP_VI: i32 = 1;
/// C: `histedit.h` — `#define CC_REFRESH 4`.
const CC_REFRESH: ElActionT = 4;
/// C: `histedit.h` — `#define CC_ERROR 6`.
const CC_ERROR: ElActionT = 6;
/// C: `histedit.h` — `#define H_SETSIZE 1`.
const H_SETSIZE: i32 = 1;
/// C: `histedit.h` — `#define H_FIRST 3`.
const H_FIRST: i32 = 3;
/// C: `histedit.h` — `#define H_LAST 4`.
const H_LAST: i32 = 4;
/// C: `histedit.h` — `#define H_PREV 5`.
const H_PREV: i32 = 5;
/// C: `histedit.h` — `#define H_NEXT 6`.
const H_NEXT: i32 = 6;
/// C: `histedit.h` — `#define H_SETUNIQUE 20`.
const H_SETUNIQUE: i32 = 20;
/// C: `vis.h` — `#define VIS_NL 0x0010`, the newline half of `VIS_WHITE`.
const VIS_NL: i32 = 0x0010;

// [spec:libedit:def:hist.hist-fun-t-void-hist-event-w-int]
/// C: `typedef int (*hist_fun_t)(void *, HistEventW *, int, ...);`
///
/// The history dispatch hook installed by `el_set(el, EL_HIST, fun, ptr)`,
/// normally `history_w`. This is the one callback in the port that has to be
/// `extern "C"` and variadic: the C ABI genuinely passes a variadic function
/// pointer here, and libedit calls it through `HIST_FUN` with zero or one
/// trailing argument depending on the operation. Rust has no safe variadic
/// `fn` pointer, so calls through it are `unsafe`, exactly as the C's are
/// unchecked.
pub type HistFunT = unsafe extern "C" fn(*mut c_void, *mut HistEventW, c_int, ...) -> c_int;

// [spec:libedit:def:hist.el-history-t]
/// The `EditLine`'s view of the history: a stash for the line being edited,
/// plus the hook that reaches the actual history object.
pub struct ElHistoryT {
    /// C: `wchar_t *buf` — the history buffer, owned. Holds the live line
    /// while the user walks the history.
    pub buf: Vec<u32>,
    /// C: `size_t sz` — allocated `wchar_t` count of `buf`.
    pub sz: usize,
    /// C: `wchar_t *last` — offset into `buf`, one past the saved line.
    ///
    /// `sem:common.ed-search-next-history-fn` notes that the
    /// C's `wcsncpy` into `buf` does not NUL-terminate when the saved line
    /// exactly fills it, so the port must use this length rather than a
    /// terminator.
    pub last: usize,
    /// Event we are looking for.
    pub eventno: i32,
    /// C: `void *ref` — argument for the history functions, a client cookie
    /// libedit stores and hands back untouched. `el_end` does not free it.
    pub r#ref: *mut c_void,
    /// Event access.
    pub fun: Option<HistFunT>,
    /// Event cookie. `ev.str` is borrowed from the history entry the last
    /// operation returned.
    pub ev: HistEventW,
}

// [spec:libedit:def:hist.hist-init-fn]
// [spec:libedit:sem:hist.hist-init-fn]
/// C: `libedit_private int hist_init(EditLine *el)`
pub(crate) fn hist_init(el: &mut EditLine) -> i32 {
    el.el_history.fun = None;
    el.el_history.r#ref = ptr::null_mut();

    // `el_calloc(EL_BUFSIZ, sizeof(wchar_t))`. The C's NULL on failure is an
    // empty stash here; `try_reserve` is what keeps the failure a failure
    // rather than an abort, and it leaves `sz` and `last` untouched — 0 and 0
    // in a freshly zeroed `EditLine` — exactly as the C's early return does.
    el.el_history.buf = Vec::new();
    if el.el_history.buf.try_reserve_exact(EL_BUFSIZ).is_err() {
        // ERR-history-11: the sole caller discards this, and must keep
        // discarding it. The editor runs on with an empty stash and the first
        // `ch_enlargebufs` repairs it through `hist_enlargebuf`.
        return -1;
    }
    el.el_history.buf.resize(EL_BUFSIZ, 0);

    el.el_history.sz = EL_BUFSIZ;
    // C: `last = buf` — a saved line of length 0. The C's pointer is an
    // offset here, so "empty" is 0.
    el.el_history.last = 0;
    0
}

// [spec:libedit:def:hist.hist-end-fn]
// [spec:libedit:sem:hist.hist-end-fn]
/// C: `libedit_private void hist_end(EditLine *el)`
pub(crate) fn hist_end(el: &mut EditLine) {
    // C: `el_free(el->el_history.buf); el->el_history.buf = NULL;`
    el.el_history.buf = Vec::new();

    // ERR-history-29: the C leaves `sz` at its last value and `last` pointing
    // into the memory just released. The rule directs the port to clear both;
    // it is unobservable across the C ABI (only `el_end` calls this, and it
    // drops the `EditLine` immediately) and it removes the trap where a
    // `hist_get` at `eventno == 0` would read from a freed stash.
    el.el_history.sz = 0;
    el.el_history.last = 0;

    // Deliberately not touched, as in the C: `fun`, `ref`, `eventno` and
    // `ev`. The application owns the history store and `history_end` is the
    // application's to call.
}

// [spec:libedit:def:hist.hist-set-fn]
// [spec:libedit:sem:hist.hist-set-fn]
/// C: `libedit_private int hist_set(EditLine *el, hist_fun_t fun, void *ptr)`
///
/// `fun` is `Option<HistFunT>` because the C stores it straight into
/// [`ElHistoryT::fun`] with no NULL check, and `el_set(EL_HIST, NULL, NULL)`
/// is how a caller detaches the history.
pub(crate) fn hist_set(el: &mut EditLine, fun: Option<HistFunT>, ptr: *mut c_void) -> i32 {
    // ERR-history-04, defined here: the C accepts a NULL `fun` alongside a
    // non-NULL `ptr` and every guard in this file then tests `ref` only, so
    // the next history access is a NULL indirect call. The rule says to
    // reject the combination rather than define it, so this is the one
    // failure path the C does not have. `fun == None` with a NULL `ptr` stays
    // the supported "no history installed" state, and `Some(fun)` with a NULL
    // `ptr` stays the supported detach.
    if fun.is_none() && !ptr.is_null() {
        return -1;
    }

    el.el_history.r#ref = ptr;
    el.el_history.fun = fun;

    // Not touched, as in the C: `buf`, `sz`, `last`, `eventno` and `ev`. A
    // stale `eventno` is resolved against the new store by the next
    // `hist_get`. `NARROW_HISTORY` is the caller's business — the narrow
    // `el_set` sets it and the wide `el_wset` clears it, each around its own
    // call to this function.
    0
}

// [spec:libedit:def:hist.hist-get-fn]
// [spec:libedit:sem:hist.hist-get-fn]
/// C: `libedit_private el_action_t hist_get(EditLine *el)`
pub(crate) fn hist_get(el: &mut EditLine) -> ElActionT {
    // This function never writes `el_history.buf`. Saving is the callers' job
    // (`ed_prev_history`, `ed_search_prev_history`, `vi_to_history_line`) and
    // all of them do it only while `eventno` is still 0, which is why edits
    // made to a recalled entry are discarded on the next step: there is one
    // slot and it holds the user's own line (ERR-history-33, reproduced by
    // leaving the write where the C has it).
    //
    // Branch A — the line the user was actually typing.
    if el.el_history.eventno == 0 {
        // C: `wcsncpy(el_line.buffer, el_history.buf, el_history.sz)` — the
        // saved line up to its first NUL, then NUL padding out to `sz`.
        // `ch_enlargebufs` and `hist_enlargebuf` keep `sz` no larger than the
        // line buffer; the `min` stands in for the C's unchecked write.
        let stash = &el.el_history.buf;
        let n = el.el_history.sz.min(el.el_line.buffer.len());
        let copy = stash[..n.min(stash.len())]
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(n.min(stash.len()));
        el.el_line.buffer[..copy].copy_from_slice(&el.el_history.buf[..copy]);
        el.el_line.buffer[copy..n].fill(0);

        // The restored length is the stash's recorded offset, not a `wcslen`
        // of what was just copied: an embedded NUL leaves the tail as NULs
        // but `lastchar` still lands at the full original length.
        el.el_line.lastchar = el.el_history.last.min(el.el_line.buffer.len());

        // KSHVI is unconditionally defined in `el.h`, so both arms are live.
        el.el_line.cursor = if el.el_map.r#type == MAP_VI {
            0
        } else {
            el.el_line.lastchar
        };

        return CC_REFRESH;
    }

    // Branch B — fetch from the history store. Note the C has no guard on a
    // negative `eventno` (ERR-history-26): the walk below never runs, `hp`
    // stays on event 1, and the epilogue rewrites the field to 1.
    if el.el_history.r#ref.is_null() {
        return CC_ERROR;
    }

    // ERR-history-27: an empty history returns here *without* resetting
    // `eventno`, which is what leaves the emacs `^P` on a phantom event 1.
    let Some(mut hp) = hist_first(el) else {
        return CC_ERROR;
    };

    let mut h: i32 = 1;
    while h < el.el_history.eventno {
        match hist_next(el) {
            Some(next) => hp = next,
            // C: `goto out` with `h` holding the ordinal of the last entry
            // that was reachable.
            None => {
                el.el_history.eventno = h;
                return CC_ERROR;
            }
        }
        h += 1;
    }

    // C: `hlen = wcslen(hp) + 1`, `blen = el_line.limit - el_line.buffer`.
    // `hlen` is a total length passed where `ch_enlargebufs` expects an
    // additional one, so the growth is conservative; that is the C's and it
    // never under-allocates.
    let hlen = hp.len() + 1;
    let blen = el.el_line.limit;
    if hlen > blen && ch_enlargebufs(el, hlen) == 0 {
        el.el_history.eventno = h;
        return CC_ERROR;
    }

    // ERR-history-28, defined here: `hp` is already a copy of the entry taken
    // before `ch_enlargebufs` ran, so an application `c_resizefun` that
    // itself uses the history or the scratch buffer cannot pull the string
    // out from under this. The C reads on through a pointer the callback may
    // have invalidated.
    let line = &mut el.el_line.buffer;
    let copy = hp.len().min(line.len().saturating_sub(1));
    line[..copy].copy_from_slice(&hp[..copy]);
    if copy < line.len() {
        // The C's `memcpy` carries the entry's own terminator across.
        line[copy] = 0;
    }
    el.el_line.lastchar = copy;

    // Trim at most one newline and then at most one space, in that fixed
    // order. Only `lastchar` moves — no NUL is written at the new position,
    // so the trimmed characters are still in the buffer above it, which is
    // what the C leaves behind and what `common.c` terminates for itself.
    if el.el_line.lastchar > 0 && el.el_line.buffer[el.el_line.lastchar - 1] == u32::from(b'\n') {
        el.el_line.lastchar -= 1;
    }
    if el.el_line.lastchar > 0 && el.el_line.buffer[el.el_line.lastchar - 1] == u32::from(b' ') {
        el.el_line.lastchar -= 1;
    }

    el.el_line.cursor = if el.el_map.r#type == MAP_VI {
        0
    } else {
        el.el_line.lastchar
    };

    CC_REFRESH
}

// [spec:libedit:def:hist.hist-command-fn]
// [spec:libedit:sem:hist.hist-command-fn]
/// C: `libedit_private int hist_command(EditLine *el, int argc, const wchar_t **argv)`
///
/// `argv` stays a raw array of NUL-terminated wide strings: it is the shared
/// shape of every builtin command function (`map_bind`, `tty_stty`,
/// `terminal_telltc`, …), handed straight through from the tokenizer, and
/// nothing here owns it.
pub(crate) fn hist_command(el: &mut EditLine, argc: i32, argv: *const *const u32) -> i32 {
    if el.el_history.r#ref.is_null() {
        return -1;
    }

    // The C indexes `argv` without checking it. A NULL array and a negative
    // `argc` are both reachable only by a caller that does not exist in the
    // tree; defined here as "nothing to do", which is the -1 every other
    // malformed invocation gets.
    if argv.is_null() || argc < 0 {
        return -1;
    }
    // SAFETY: `argv` is the tokenizer's array, which has at least `argc`
    // entries; every read below is guarded by the same `argc` the C tests.
    let arg = |i: i32| -> *const u32 { unsafe { *argv.add(i as usize) } };

    // List form. The C's test is `argc == 1 || wcscmp(argv[1], L"list") == 0`,
    // so `argc == 0` would read `argv[1]` off the end; defined here as the
    // list form, the same thing `argc == 1` gets.
    if argc <= 1 || unsafe { wcs_eq_ascii(arg(1), "list") } {
        let mut maxlen: usize = 0;
        let mut buf: Vec<u8> = Vec::new();
        let mut hno: i32 = 1;

        // Oldest first: `HIST_LAST` then repeated `HIST_PREV`, stopping at the
        // first NULL. With libedit's own store this leaves the traversal
        // cursor on the most recent entry, which is an observable side effect
        // on the history object and is part of the behaviour.
        let mut entry = hist_last(el);
        while let Some(str) = entry {
            // C: `ptr = ct_encode_string(str, &el->el_scratch)`, then
            // `strlen(ptr)`. ERR-history-06: the C dereferences the result
            // without checking it, which is a crash on OOM; the rule says to
            // treat a NULL as a -1 return instead.
            let mut src = {
                let Some(bytes) = ct_encode_string(Some(str.as_slice()), &mut el.el_scratch) else {
                    return -1;
                };
                let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                bytes[..len].to_vec()
            };

            // Strip exactly one trailing newline — the terminator of the line
            // the user entered. The C does this in place in the scratch
            // buffer, which nothing observes because every reader of
            // `el_scratch.cbuff` rewrites it first.
            if src.last() == Some(&b'\n') {
                src.pop();
            }
            let len = src.len();
            // `strnvis` needs the C's NUL-terminated source.
            src.push(0);

            // C: `len = len * 4 + 1`, the worst-case escaped size, then grow
            // `buf` by `len + 1024` whenever it no longer fits. `maxlen`
            // never shrinks, so the buffer grows monotonically across the
            // listing. ERR-history-07, defined here: the C's `4` is an
            // assumption that one input byte never escapes to more than four
            // output bytes, and `strvis` bounds-checks nothing, so a locale
            // that breaks the assumption smashes the heap. The sizing is the
            // engine's own internal guarantee of 16 bytes per input byte, and
            // the call is the bounded `strnvis`, so the bound is enforced
            // rather than assumed.
            let need = len.saturating_mul(16).saturating_add(1);
            if need >= maxlen {
                maxlen = need.saturating_add(1024);
                if buf.try_reserve(maxlen.saturating_sub(buf.len())).is_err() {
                    // C: `el_free(buf); return -1;` — `buf` drops here.
                    return -1;
                }
                buf.resize(maxlen, 0);
            }

            // C: `strvis(buf, ptr, VIS_NL)`. `VIS_NL` and not `VIS_WHITE`:
            // this is a human-readable listing, not `history.c`'s on-disk
            // format, and the only escaping the format needs is the newline
            // that would otherwise split an entry across printed lines.
            // Spaces and tabs stay literal on purpose.
            let n = strnvis(
                buf.as_mut_ptr().cast::<c_char>(),
                buf.len(),
                src.as_ptr().cast::<c_char>(),
                VIS_NL,
            );
            if n < 0 {
                // Unreachable under the sizing above except on an internal
                // allocation failure, which gets the same -1 as ERR-history-06.
                return -1;
            }

            // C: `fprintf(el->el_outfile, "%d\t%s\n", hno++, buf)`. The number
            // is a fresh 1-based counter over this walk, not `ev.num`, and
            // nothing ever reads it back. Write errors are not checked.
            let mut out = format!("{hno}\t").into_bytes();
            out.extend_from_slice(&buf[..n as usize]);
            out.push(b'\n');
            write_outfile(el, &out);
            hno += 1;

            entry = hist_prev(el);
        }
        return 0;
    }

    if argc != 3 {
        return -1;
    }

    // C: `num = (int)wcstol(argv[2], NULL, 0)`. No error checking at all: a
    // non-numeric argument yields 0 and a value outside `int` is truncated by
    // the cast.
    let num = unsafe { wcstol_base0(arg(2)) };

    // SAFETY: `arg(1)` is `argv[1]`, which `argc == 3` guarantees.
    let size = unsafe { wcs_eq_ascii(arg(1), "size") };
    let unique = unsafe { wcs_eq_ascii(arg(1), "unique") };
    if !size && !unique {
        return -1;
    }
    let op = if size { H_SETSIZE } else { H_SETUNIQUE };

    // ERR-history-05. The C calls `history_w(el->el_history.ref, &ev, op,
    // num)` directly, bypassing the installed dispatcher, so the handle is
    // reinterpreted as libedit's own *wide* store whatever it really is.
    //
    // - Narrow store — what the narrow `el_set(EL_HIST, …)` and the readline
    //   layer install, i.e. the common case: the two structs are layout
    //   compatible, so the C reaches `history_setsize`/`history_setunique`,
    //   which find the narrow `next` hook where they expect the wide one and
    //   return -1. `history size` and `history unique` in an `.editrc` are
    //   therefore silently inoperative. That -1 is defined behaviour and is
    //   reproduced here as a checked dispatch instead of a punned pointer —
    //   the C ABI freezes it, so it must not be "fixed" into working by
    //   dispatching through `fun`, which would make the narrow store succeed.
    // - Custom store installed through the narrow entry point: the C is type
    //   confused, which is undefined; defined here as the same -1.
    if el.el_flags & NARROW_HISTORY != 0 {
        return -1;
    }

    // The wide path. `ref` was installed through the wide `el_wset(EL_HIST,
    // …)`, where the C's assumption holds for libedit's own store and this is
    // the defined behaviour to preserve: `history size 100` in an `.editrc`
    // works for a wide-API application.
    //
    // The remaining hole is a *custom* store installed through the wide entry
    // point, where the C is type confused and the rule says to fail rather
    // than invent a meaning. The core cannot tell that case from the builtin
    // one: `el_history.ref` is an opaque `void *` and `el_history.fun` is an
    // `extern "C"` pointer to the ABI crate's shim, which `nshedit` cannot
    // name. Closing it needs `history.rs` to publish a way to recognise its
    // own handle — see the note in this module's report.
    let mut ev = HistEventW {
        num: 0,
        str: ptr::null(),
    };
    // C: `ev` is an uninitialised local, filled by the callee and discarded,
    // so the error string is thrown away.
    history_w(
        el.el_history.r#ref.cast::<HistoryW>(),
        &mut ev,
        op,
        HistoryArg::Num(num),
    )
}

// [spec:libedit:def:hist.hist-enlargebuf-fn]
// [spec:libedit:sem:hist.hist-enlargebuf-fn]
/// C: `libedit_private int hist_enlargebuf(EditLine *el, size_t newsz)`
pub(crate) fn hist_enlargebuf(el: &mut EditLine, newsz: usize) -> i32 {
    // The return convention is inverted relative to most of libedit: 1 is
    // success, 0 is failure.
    let oldsz = el.el_history.sz;
    if newsz <= oldsz {
        // The stash is never shrunk; an equal or smaller request is a
        // successful no-op.
        return 1;
    }

    // C: `el_realloc(buf, newsz * sizeof(wchar_t))`. On failure every field
    // is left unchanged and the old buffer is still allocated and still
    // valid, which is what lets `ch_enlargebufs` turn this into its own 0 and
    // carry on.
    if el
        .el_history
        .buf
        .try_reserve(newsz.saturating_sub(el.el_history.buf.len()))
        .is_err()
    {
        return 0;
    }
    el.el_history.buf.resize(newsz, 0);
    // C: `memset(&newbuf[oldsz], '\0', (newsz - oldsz) * sizeof(wchar_t))`.
    // The first `oldsz` elements are left alone, so a saved line survives the
    // growth. This is a no-op after the `resize` unless `hist_init`'s
    // allocation failed and left a stash shorter than `sz` claims.
    el.el_history.buf[oldsz..newsz].fill(0);

    // C: `last = newbuf + (last - oldbuf)`. `last` is already an offset here,
    // so the rebase across the reallocation is nothing at all.
    el.el_history.sz = newsz;

    1
}

// [spec:libedit:def:hist.hist-convert-fn]
// [spec:libedit:sem:hist.hist-convert-fn]
/// C: `libedit_private wchar_t *hist_convert(EditLine *el, int fn, void *arg)`
///
/// `fn` is a Rust keyword, so the parameter is spelled `r#fn`, the way
/// `ElHistoryT::r#ref` spells the C's `ref`. The return is a raw pointer
/// because it is `ct_decode_string`'s view into `el->el_scratch`, valid only
/// until the next conversion.
pub(crate) fn hist_convert(el: &mut EditLine, r#fn: i32, arg: *mut c_void) -> *mut u32 {
    // `ct_decode_string` NUL-terminates `el_scratch.wbuff`, so the slice's
    // base pointer is the C wide string the C hands back.
    match hist_convert_str(el, r#fn, arg) {
        Some(s) => s.as_ptr().cast_mut(),
        None => ptr::null_mut(),
    }
}

/// The body of [`hist_convert`], keeping the borrow the C throws away.
///
/// Split out so the callers inside this file — the [`hist_first`] family —
/// can consume the decoded string as a slice instead of walking a raw pointer
/// back to its NUL. [`hist_convert`] itself is the C's shape, and the
/// lifetime here is exactly what makes that shape's "valid until the next
/// conversion" contract visible.
fn hist_convert_str(el: &mut EditLine, r#fn: i32, arg: *mut c_void) -> Option<&[u32]> {
    // ERR-history-04: the C calls through `fun` with no NULL check, relying
    // on call sites having tested `ref`. [`hist_set`] rejects the pair that
    // makes those two disagree, so this can only be the "no history at all"
    // state, which every caller already reads as an empty history.
    let fun = el.el_history.fun?;

    // ERR-history-03, defined here: the C declares a `HistEventW` and lets
    // the narrow store write a `HistEvent` through it, then reinterprets
    // `ev.str` as a `char *`. The event *is* narrow on this path — that is
    // what `NARROW_HISTORY` means — so it is declared narrow. The pointer
    // cast at the call is the C ABI's, not a reinterpretation of the value:
    // the installed dispatcher is the narrow `history()`, whose second
    // parameter really is `HistEvent *`.
    let mut ev = HistEvent {
        num: 0,
        str: ptr::null(),
    };
    let r#ref = el.el_history.r#ref;
    // SAFETY: `fun` and `ref` were installed together by `hist_set`, which is
    // the C's own precondition for this call; `ev` outlives it.
    let rv = unsafe {
        fun(
            r#ref,
            ptr::from_mut(&mut ev).cast::<HistEventW>(),
            r#fn as c_int,
            arg,
        )
    };
    if rv == -1 {
        // The event is discarded. Callers cannot distinguish this from an
        // empty history; both surface as NULL.
        return None;
    }

    // ERR-history-18, decided here: **reproduce**. The wide path writes
    // `el->el_history.ev`, this one writes a local, so under narrow history
    // the shared cookie keeps its all-zero initial value forever. Its one
    // reader, `vi_to_history_line`, then computes `eventno = 1 + ev.num -
    // argument` from a stale `num` of 0 and derives a negative event, so vi
    // `G` with a count is inoperative for every narrow-API application. That
    // is defined behaviour and wrong, which `[dec:libedit:conformance-policy]`
    // says to reproduce and fix in idiomatization; writing the cookie here
    // would change `vi_to_history_line`'s result silently and in the middle of
    // a translation wave. `el.el_history.ev` is deliberately not touched.

    let bytes = if ev.str.is_null() {
        // C: `ct_decode_string(NULL, …)` returns NULL.
        None
    } else {
        // SAFETY: the store's event string is NUL-terminated and borrowed for
        // as long as the entry lives, which is past this decode.
        Some(unsafe { CStr::from_ptr(ev.str) }.to_bytes())
    };
    ct_decode_string(bytes, &mut el.el_scratch)
}

/// C: `HIST_FUN(el, fn, arg)` from `hist.h`.
///
/// Chooses between the two shapes the way the macro does: under
/// `NARROW_HISTORY` the installed store's event string is really `char *` and
/// goes through [`hist_convert`]; otherwise it is already `wchar_t *` and is
/// read straight out of `el->el_history.ev`, which the call fills.
///
/// The C hands back a borrowed pointer — into the store's own storage on the
/// wide path, into `el_scratch.wbuff` on the narrow one — that the next
/// history operation invalidates. This copies instead. Every caller in the C
/// holds only the most recent value, so the copy is unobservable, and it is
/// what lets `hist_get` survive the `ch_enlargebufs` in its own step 4
/// (ERR-history-28).
fn hist_fun(el: &mut EditLine, r#fn: i32, arg: *mut c_void) -> Option<Vec<u32>> {
    if el.el_flags & NARROW_HISTORY != 0 {
        return hist_convert_str(el, r#fn, arg).map(<[u32]>::to_vec);
    }

    // C: `HIST_FUN_INTERNAL` — the call writes the shared event cookie, and
    // that write is what `vi_to_history_line` reads back as `ev.num`.
    let fun = el.el_history.fun?;
    let r#ref = el.el_history.r#ref;
    // SAFETY: as in `hist_convert_str`; the cookie is part of the `EditLine`
    // and outlives the call.
    let rv = unsafe {
        fun(
            r#ref,
            ptr::from_mut(&mut el.el_history.ev),
            r#fn as c_int,
            arg,
        )
    };
    if rv == -1 {
        return None;
    }
    let str = el.el_history.ev.str;
    if str.is_null() {
        // The C hands the NULL straight back and every caller tests for it.
        return None;
    }
    // SAFETY: the store's event string is NUL-terminated and stays valid
    // until the entry is deleted or replaced, which no operation here does.
    Some(unsafe { wcs_to_vec(str) })
}

/// C: `HIST_FIRST(el)` — the most recent entry.
pub(crate) fn hist_first(el: &mut EditLine) -> Option<Vec<u32>> {
    hist_fun(el, H_FIRST, ptr::null_mut())
}

/// C: `HIST_LAST(el)` — the oldest entry.
pub(crate) fn hist_last(el: &mut EditLine) -> Option<Vec<u32>> {
    hist_fun(el, H_LAST, ptr::null_mut())
}

/// C: `HIST_NEXT(el)` — one step toward the past.
pub(crate) fn hist_next(el: &mut EditLine) -> Option<Vec<u32>> {
    hist_fun(el, H_NEXT, ptr::null_mut())
}

/// C: `HIST_PREV(el)` — one step toward the present.
pub(crate) fn hist_prev(el: &mut EditLine) -> Option<Vec<u32>> {
    hist_fun(el, H_PREV, ptr::null_mut())
}

/// C: `fprintf(el->el_outfile, …)` for an already-formatted byte string.
///
/// The stream is a caller-owned `FILE *` the port cannot write through, so
/// this goes to the matching descriptor, which the `EditLine` carries for
/// exactly this reason (`def:el.editline`). Errors are discarded, as the C
/// discards `fprintf`'s result.
fn write_outfile(el: &EditLine, bytes: &[u8]) {
    if el.el_outfd < 0 {
        return;
    }
    // SAFETY: `el_outfd` is the application's descriptor and stays open for
    // the life of the `EditLine`; `ManuallyDrop` is what keeps this borrow
    // from closing it, which libedit never does.
    let mut out = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(el.el_outfd) });
    let _ = out.write_all(bytes);
}

/// C: `wcscmp(s, L"…") == 0` against an ASCII literal.
///
/// A NULL `s` compares unequal. The C would dereference it — `argv` entries
/// are NULL-terminated by the tokenizer — and this is the same -1 the caller
/// reaches for every other unrecognised subcommand.
///
/// # Safety
///
/// `s` must be NULL or point at a NUL-terminated wide string.
unsafe fn wcs_eq_ascii(s: *const u32, lit: &str) -> bool {
    if s.is_null() {
        return false;
    }
    for (i, b) in lit.bytes().enumerate() {
        // SAFETY: the caller's string is NUL-terminated, and the loop stops
        // at the first mismatch, so the NUL is never read past.
        if unsafe { *s.add(i) } != u32::from(b) {
            return false;
        }
    }
    // SAFETY: every byte up to here matched, so this index is at most the
    // terminator.
    unsafe { *s.add(lit.len()) == 0 }
}

/// C: `(int)wcstol(s, NULL, 0)`.
///
/// Base 0, so `10`, `012` and `0xa` all parse. There is no error checking in
/// the C at all: a non-numeric argument yields 0, `errno` is never consulted,
/// and a value outside `int` is truncated by the cast — `long` is saturated
/// by `wcstol` first, which is why the saturation happens here too.
///
/// # Safety
///
/// `s` must be NULL or point at a NUL-terminated wide string.
unsafe fn wcstol_base0(s: *const u32) -> i32 {
    if s.is_null() {
        return 0;
    }
    let mut i = 0usize;
    // SAFETY: every read below stops at the terminator, which the caller
    // guarantees is present.
    let at = |i: usize| -> u32 { unsafe { *s.add(i) } };

    // Leading whitespace, as `iswspace` accepts it.
    while matches!(at(i), 0x20 | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d) {
        i += 1;
    }

    let negative = match at(i) {
        c if c == u32::from(b'-') => {
            i += 1;
            true
        }
        c if c == u32::from(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };

    // Base 0: `0x`/`0X` is hex, a leading `0` is octal, anything else decimal.
    let mut base = 10u32;
    if at(i) == u32::from(b'0') {
        let next = at(i + 1);
        if next == u32::from(b'x') || next == u32::from(b'X') {
            base = 16;
            i += 2;
        } else {
            base = 8;
            i += 1;
        }
    }

    // `wcstol` saturates at `LONG_MIN`/`LONG_MAX`; the C's cast then keeps the
    // low 32 bits.
    let mut acc: i64 = 0;
    let mut saturated = false;
    while let Some(digit) = char::from_u32(at(i)).and_then(|c| c.to_digit(base)) {
        i += 1;
        if saturated {
            continue;
        }
        match acc
            .checked_mul(i64::from(base))
            .and_then(|a| a.checked_add(i64::from(digit)))
        {
            Some(next) => acc = next,
            None => saturated = true,
        }
    }
    if saturated {
        acc = if negative { i64::MIN } else { i64::MAX };
    } else if negative {
        acc = -acc;
    }
    acc as i32
}

/// C: `wcslen` followed by the copy the C does not need.
///
/// # Safety
///
/// `p` must be non-NULL and point at a NUL-terminated wide string.
unsafe fn wcs_to_vec(p: *const u32) -> Vec<u32> {
    let mut len = 0usize;
    // SAFETY: the caller guarantees a terminator.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` characters were just read, and the string is contiguous.
    unsafe { core::slice::from_raw_parts(p, len) }.to_vec()
}
