//! Ported from `src/hist.c`; rules live in `docs/spec/port/src/hist.md`.

use core::ffi::{c_int, c_void};
use std::cell::RefCell;
use std::ffi::CStr;
use std::ptr;
use std::rc::Rc;

use crate::chared::ch_enlargebufs;
use crate::chartype::{ct_decode_string, ct_encode_string};
use crate::el::{EL_BUFSIZ, NARROW_HISTORY};
use crate::el::{EditLine, ElActionT};
use crate::histedit::{
    CC_ERROR, CC_REFRESH, H_FIRST, H_LAST, H_NEXT, H_PREV, H_SETSIZE, H_SETUNIQUE,
};
use crate::histedit::{HistEvent, HistEventW};
use crate::map::MAP_VI;

// Constants the C reaches through its headers. None of `el.h`, `map.h`,
// `histedit.h` or `vis.h` has a Rust home that publishes these yet, so they
// are private here; idiomatization should fold each into the module that ends
// up owning its header.

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

/// The one history call `hist_command` makes that does **not** go through
/// [`HistFunT`].
///
/// C: `history_w(el->el_history.ref, &ev, op, num)`, issued directly for
/// `history size` and `history unique` in an `.editrc` — the dispatcher is
/// deliberately bypassed and the opaque handle reinterpreted as libedit's own
/// wide store, whatever it really is (ERR-history-05).
///
/// The core cannot make that call itself. Naming the wide store means naming
/// `history_gen`, which is the C ABI's opcode dispatcher and belongs in
/// `nshedit-abi` under `dec:libedit:idiomatic-core` — and `nshedit` cannot
/// depend on `nshedit-abi`. So the pun stays on the ABI side of the boundary
/// and reaches the core as a function pointer installed beside the dispatcher.
///
/// Non-variadic on purpose: the C's version carries one `int` and nothing
/// else, so there is no reason to inherit a `...` that stable Rust cannot
/// define. `op` is `H_SETSIZE` or `H_SETUNIQUE`; the answer is the C's.
pub type HistSettingsT = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;

/// An entry's text, in whichever width the store keeps.
///
/// Both exist because both are real. The C has two instantiations of its
/// history and `NARROW_HISTORY` records which one an editor was given; a
/// byte-oriented store is what a shell actually has, and demanding wide text
/// from one would mean it transcoding on the way out and the editor
/// transcoding back. [`Narrow`](HistText::Narrow) is decoded here through the
/// same `ct_decode_string` the C's narrow path already uses, so the two agree
/// on what an undecodable byte means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistText {
    /// One character per `u32`, no terminator. What the editor's line buffer
    /// holds, so this arm costs nothing.
    Wide(Vec<u32>),
    /// Bytes in the current locale's encoding, no terminator.
    Narrow(Vec<u8>),
}

/// One entry, as the editor asks for it: the event number and the text.
///
/// The C hands back a borrowed `wchar_t *` in a shared `HistEventW` that the
/// next call overwrites. This owns its text instead, because the borrow is
/// unrepresentable across a trait object and the editor copies it anyway —
/// [`hist_fun`] already ends in `to_vec`.
pub struct HistLine {
    /// C: `ev.num`. `vi_to_history_line` computes an event number from it.
    pub num: i32,
    /// The entry.
    pub text: HistText,
}

/// A history the editor can walk, for callers that are not the C ABI.
///
/// The editor performs exactly four operations on a history — the C issues
/// them as `H_FIRST`, `H_LAST`, `H_NEXT` and `H_PREV`, each with no trailing
/// argument, which is why [`HistFunT`]'s variadic tail is unused by every
/// call libedit itself makes. Those four are the whole of what recall and
/// search need.
///
/// The two settings below reach only the `history size` and `history unique`
/// builtins in an `.editrc`. They default to -1, the same answer the C gives
/// for a narrow store, so an implementation that does not care about `.editrc`
/// need not write them.
pub trait EditorHistory {
    /// The most recent entry.
    fn first(&mut self) -> Option<HistLine>;
    /// The oldest entry.
    fn last(&mut self) -> Option<HistLine>;
    /// One step towards the oldest, from wherever the last call left off.
    fn next(&mut self) -> Option<HistLine>;
    /// One step towards the most recent.
    fn prev(&mut self) -> Option<HistLine>;

    /// `history size N`. 0 on success, -1 if unsupported.
    fn set_size(&mut self, _entries: i32) -> i32 {
        -1
    }
    /// `history unique N`. 0 on success, -1 if unsupported.
    fn set_unique(&mut self, _on: bool) -> i32 {
        -1
    }
}

/// Where an [`EditLine`] gets its entries.
///
/// The C has two fields for this — a `void *ref` cookie and a variadic
/// `hist_fun_t` — and "no history" is the pair being NULL. That encoding can
/// only hold a C function pointer, and stable Rust cannot *define* a `...`
/// function (rust-lang/rust#44930), so the only values that could ever reach
/// it were the `history`/`history_w` symbols `nshedit-abi` exports. A program
/// linking `nshedit` directly therefore had no way to attach a history at all,
/// and its editor had no recall and no search.
pub enum HistSource {
    /// Nothing attached. Recall and search report an empty history.
    None,
    /// The C ABI's dispatcher and the opaque cookie it is called with. This
    /// is what `el_set(el, EL_HIST, history, h)` installs, and its shape is
    /// frozen by the ABI.
    CAbi {
        /// C: `el_history.fun`.
        fun: HistFunT,
        /// C: `el_history.ref`, handed back untouched. `el_end` does not free
        /// it.
        cookie: *mut c_void,
        /// The `.editrc` settings path, which the C reaches by punning
        /// `cookie` rather than by dispatching. See [`HistSettingsT`].
        ///
        /// `None` leaves `history size` and `history unique` answering -1,
        /// which is what a caller that installed a history without one gets.
        settings: Option<HistSettingsT>,
    },
    /// A Rust implementation, **shared** with the caller rather than owned by
    /// the editor.
    ///
    /// The sharing is the requirement, not a convenience. A history outlives
    /// the editor that reads it: the C's `el_history.ref` is a client cookie
    /// and `el_end` explicitly does not free it, and a shell relies on that —
    /// `set +o emacs` ends the editor and keeps the history, so the two have
    /// genuinely independent lifetimes. A setter taking the history by value
    /// cannot express that.
    ///
    /// `Rc<RefCell<_>>` rather than a borrow because a borrow would put a
    /// lifetime parameter on [`EditLine`], which is threaded through every
    /// signature in this crate. The editor takes its handle out before
    /// borrowing, and calls the history from exactly one place, so the
    /// `RefCell` is never re-entered — a caller that reaches back into its own
    /// history from inside one of these methods is the one way to panic here.
    Rust(Rc<RefCell<dyn EditorHistory>>),
}

impl HistSource {
    /// Whether a history is attached at all.
    ///
    /// C: `el->el_history.ref != NULL`, which is the guard every recall path
    /// tests before walking. Note it tests the cookie and not the function,
    /// so `el_set(EL_HIST, history, NULL)` reads as detached *here* while
    /// still dispatching from the paths that have no such guard — the C is
    /// inconsistent about that pair and this reproduces it rather than
    /// tidying it, because `vi_history_word` is one of the unguarded paths.
    pub fn is_attached(&self) -> bool {
        match self {
            HistSource::None => false,
            HistSource::CAbi { cookie, .. } => !cookie.is_null(),
            HistSource::Rust(_) => true,
        }
    }
}

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
    /// Where entries come from. C: the `ref`/`fun` pair, which is one of the
    /// three states this enum names and cannot express the third.
    pub src: HistSource,
    /// Event cookie. `ev.str` is borrowed from the history entry the last
    /// operation returned.
    pub ev: HistEventW,
}

// [spec:libedit:def:hist.hist-init-fn]
// [spec:libedit:sem:hist.hist-init-fn]
/// C: `libedit_private int hist_init(EditLine *el)`
pub(crate) fn hist_init(el: &mut EditLine) -> i32 {
    el.el_history.src = HistSource::None;

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
///
/// Public only so `nshedit-abi` can write `el_wset`'s `EL_HIST` arm, and
/// hidden because a Rust caller cannot supply the argument: [`HistFunT`] is
/// C-variadic, and stable Rust cannot *define* a `...` function
/// (rust-lang/rust#44930). A Rust caller wants [`EditLine::set_history`],
/// which takes an [`EditorHistory`] and needs no variadic anything.
#[doc(hidden)]
pub fn hist_set(
    el: &mut EditLine,
    fun: Option<HistFunT>,
    ptr: *mut c_void,
    settings: Option<HistSettingsT>,
) -> i32 {
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

    // C: both fields are written unconditionally, and a NULL `fun` is what
    // makes the pair uncallable. The cookie is carried whatever it is —
    // including NULL, which the guards read as detached and the unguarded
    // paths do not. See [`HistSource::is_attached`].
    el.el_history.src = match fun {
        Some(fun) => HistSource::CAbi {
            fun,
            cookie: ptr,
            settings,
        },
        None => HistSource::None,
    };

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
    if !el.el_history.src.is_attached() {
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
    if !el.el_history.src.is_attached() {
        return -1;
    }

    // The C indexes `argv` without checking it. A NULL array and a negative
    // `argc` are both reachable only by a caller that does not exist in the
    // tree; defined here as "nothing to do", which is the -1 every other
    // malformed invocation gets.
    if argv.is_null() || argc < 0 {
        return -1;
    }
    // The whole array, once, so the reads below are ordinary indexing and the
    // two helpers that consume them need no pointers.
    // SAFETY: `argv` is the tokenizer's array, which has at least `argc`
    // entries each a NUL-terminated wide string; `argc` is the same bound the
    // C indexes within, and the strings outlive this call.
    let argv: Vec<&[u32]> = (0..argc)
        .map(|i| unsafe { wcs_in(*argv.add(i as usize)) })
        .collect();

    // List form. The C's test is `argc == 1 || wcscmp(argv[1], L"list") == 0`,
    // so `argc == 0` would read `argv[1]` off the end; defined here as the
    // list form, the same thing `argc == 1` gets.
    if argc <= 1 || wcs_eq_ascii(argv[1], "list") {
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
            // C: `strvis(buf, ptr, VIS_NL)`. `VIS_NL` and not `VIS_WHITE`:
            // this is a human-readable listing, not `history.c`'s on-disk
            // format, and the only escaping the format needs is the newline
            // that would otherwise split an entry across printed lines.
            // Spaces and tabs stay literal on purpose.
            //
            // The C sizes a destination at `len * 4 + 1` and calls the
            // unbounded `strvis` into it — ERR-history-07, an assumption that
            // one input byte never escapes past four, which a locale can
            // break into a heap smash. Nothing here sizes anything:
            // `vislite::encode` returns what it produced, so there is no destination
            // to size.
            //
            // `crate::vislite` and not the `bsd` seam: `bsd` is off by default,
            // and routing a human-readable listing through it made the builtin
            // return -1 and print nothing on every build that ships.
            let buf = crate::vislite::encode(crate::vislite::Escape::Nl, &src);

            // C: `fprintf(el->el_outfile, "%d\t%s\n", hno++, buf)`. The number
            // is a fresh 1-based counter over this walk, not `ev.num`, and
            // nothing ever reads it back. Write errors are not checked.
            let mut out = format!("{hno}\t").into_bytes();
            out.extend_from_slice(&buf);
            out.push(b'\n');
            el.write_outfile(&out);
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
    let num = wcstol_base0(argv[2]);

    let size = wcs_eq_ascii(argv[1], "size");
    let unique = wcs_eq_ascii(argv[1], "unique");
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

    // A Rust history is asked rather than punned. The C reinterprets the
    // cookie because it has nothing else to go on; here the implementation
    // answers for itself, and the default is the same -1 the narrow store
    // gives, so an implementation that ignores `.editrc` is not penalised.
    if let HistSource::Rust(h) = &el.el_history.src {
        let h = Rc::clone(h);
        let mut h = h.borrow_mut();
        return if size {
            h.set_size(num)
        } else {
            h.set_unique(num != 0)
        };
    }

    // The wide path, which the C reaches by punning `ref` into its own wide
    // store. That pun cannot live here: naming the wide store means naming
    // `history_gen`, the C ABI's opcode dispatcher, and
    // `dec:libedit:idiomatic-core` puts that in `nshedit-abi` — which this
    // crate cannot depend on. So the call arrives as [`HistSettingsT`],
    // installed alongside the dispatcher by whoever installed the history.
    //
    // The behaviour is unchanged, including the part that is wrong: for
    // libedit's own store the C's assumption holds and `history size 100` in
    // an `.editrc` works, and for a *custom* store installed through the wide
    // entry point the C is type confused. Recognising that case needs the
    // store to publish a way to identify its own handle, which is a separate
    // question from where the call lives — moving the pun does not close it
    // and must not appear to.
    let HistSource::CAbi {
        cookie, settings, ..
    } = el.el_history.src
    else {
        return -1;
    };
    // No settings hook means the installer did not provide one, which reads
    // the same way the narrow store does: the builtin is inoperative here.
    let Some(settings) = settings else {
        return -1;
    };
    // SAFETY: `settings` and `cookie` were installed together by `hist_set`,
    // which is the same precondition the dispatch calls rely on.
    unsafe { settings(cookie, op as c_int, num as c_int) }
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
    //
    // A [`HistSource::Rust`] history is never narrow — `NARROW_HISTORY` is
    // set only by the narrow `el_set` and the readline layer, both of which
    // install a C dispatcher — so [`hist_fun`] never routes one here.
    let HistSource::CAbi { fun, cookie, .. } = el.el_history.src else {
        return None;
    };

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
    // SAFETY: `fun` and `cookie` were installed together by `hist_set`, which
    // is the C's own precondition for this call; `ev` outlives it.
    let rv = unsafe {
        fun(
            cookie,
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
    // The Rust seam is checked before the width split, not after it. Each
    // entry carries its own width in [`HistText`], so a byte-oriented Rust
    // store needs neither `NARROW_HISTORY` nor the C's two instantiations —
    // and a narrow editor with a Rust history would otherwise fall into
    // `hist_convert_str`, which can only reach a C dispatcher.
    if let HistSource::Rust(h) = &el.el_history.src {
        // The handle comes out first: the borrow of `el` has to end before
        // the decode below, which needs `el_scratch`.
        let h = Rc::clone(h);
        let line = {
            let mut h = h.borrow_mut();
            match r#fn {
                H_FIRST => h.first(),
                H_LAST => h.last(),
                H_NEXT => h.next(),
                H_PREV => h.prev(),
                // Loud rather than silent. `hist_command` handles the two
                // settings itself and no editor path issues any other opcode
                // — measured, not assumed: H_CURR, H_SET, H_PREV_STR and
                // H_NEXT_STR exist but nothing outside `history.rs` names
                // them, and the search commands walk through `hist_first` and
                // `hist_next` like everything else. If that ever stops being
                // true, an unmapped opcode would make an editing command
                // quietly do nothing, which is the worst way to find out.
                other => {
                    debug_assert!(false, "no EditorHistory mapping for opcode {other}");
                    None
                }
            }
        }?;
        el.el_history.ev.num = line.num;
        // The C's `ev.str` borrows from the store and stays valid until the
        // entry is replaced. There is no such pointer here, and the one
        // reader of the cookie wants `num`, so the field is left alone rather
        // than made to dangle.
        return Some(match line.text {
            HistText::Wide(w) => w,
            // The same decode the C's narrow path takes, so an undecodable
            // byte means here what it means there.
            HistText::Narrow(b) => ct_decode_string(Some(&b), &mut el.el_scratch)?.to_vec(),
        });
    }

    if el.el_flags & NARROW_HISTORY != 0 {
        return hist_convert_str(el, r#fn, arg).map(<[u32]>::to_vec);
    }

    // C: `HIST_FUN_INTERNAL` — the call writes the shared event cookie, and
    // that write is what `vi_to_history_line` reads back as `ev.num`.
    let HistSource::CAbi { fun, cookie, .. } = el.el_history.src else {
        return None;
    };
    // SAFETY: as in `hist_convert_str`; the cookie is part of the `EditLine`
    // and outlives the call.
    let rv = unsafe {
        fun(
            cookie,
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
    Some(unsafe { wcs_in(str) }.to_vec())
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

/// A NUL-terminated wide string, as the characters before its terminator.
///
/// The C hands `wchar_t *` from the tokenizer's `argv` and from the history
/// store straight to `wcslen`/`wcscmp`/`wcstol`; converting once here is what
/// lets the three helpers below be ordinary slice code with no pointers in
/// their signatures.
///
/// **A NULL is the empty string.** The C would dereference it, and every
/// caller here already reads an empty string as "no match" or "no number",
/// which is the -1 an unrecognised subcommand gets anyway.
///
/// # Safety
///
/// `s` must be NULL or point at a NUL-terminated wide string that outlives
/// `'a`.
unsafe fn wcs_in<'a>(s: *const u32) -> &'a [u32] {
    if s.is_null() {
        return &[];
    }
    let mut len = 0usize;
    // SAFETY: the caller guarantees a terminator.
    while unsafe { *s.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` characters were just read, and the string is contiguous.
    unsafe { core::slice::from_raw_parts(s, len) }
}

/// C: `wcscmp(s, L"…") == 0` against an ASCII literal.
fn wcs_eq_ascii(s: &[u32], lit: &str) -> bool {
    s.len() == lit.len() && s.iter().zip(lit.bytes()).all(|(&c, b)| c == u32::from(b))
}

/// C: `(int)wcstol(s, NULL, 0)`.
///
/// Base 0, so `10`, `012` and `0xa` all parse. There is no error checking in
/// the C at all: a non-numeric argument yields 0, `errno` is never consulted,
/// and a value outside `int` is truncated by the cast — `long` is saturated
/// by `wcstol` first, which is why the saturation happens here too.
fn wcstol_base0(s: &[u32]) -> i32 {
    let mut i = 0usize;
    // Reading past the end is the C reading its terminator, and stops every
    // scan below for the same reason.
    let at = |i: usize| -> u32 { s.get(i).copied().unwrap_or(0) };

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

#[cfg(test)]
mod test;
