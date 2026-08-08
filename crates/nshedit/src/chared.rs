//! Ported from `src/chared.c`; rules live in `docs/spec/port/src/chared.md`.

use core::ffi::{c_char, c_void};
use core::ptr;

use crate::common::ed_end_of_file;
use crate::el::{EL_BUFSIZ, EditLine, ElActionT};
use crate::fcns::ED_UNASSIGNED;
use crate::hist::hist_enlargebuf;
use crate::locale;
use crate::map::ElMapCurrent;
use crate::read::el_wgetc;
use crate::refresh::{re_refresh, re_refresh_cursor};
use crate::terminal::terminal_beep;

// Pending vi operator, held in `c_vcmd.action`. C: `chared.h`. DELETE,
// INSERT and YANK combine, and the exact combination DELETE|INSERT is what
// makes `cw` behave like `ce` — see `sem:vi.cv-next-word-fn`.
pub(crate) const NOP: i32 = 0x00;
pub(crate) const DELETE: i32 = 0x01;
pub(crate) const INSERT: i32 = 0x02;
pub(crate) const YANK: i32 = 0x04;

// Editing mode, held in `el_state.inputmode`. C: `chared.h`. `ch_init` and
// `ch_reset` install MODE_INSERT, `vi_replace_mode`/`vi_replace_char` raise
// the other two, and `ed_insert` is the one command that reads them.
pub(crate) const MODE_INSERT: i32 = 0;
pub(crate) const MODE_REPLACE: i32 = 1;
pub(crate) const MODE_REPLACE_1: i32 = 2;

/// C: `#define CHAR_FWD (+1)` — search direction.
pub(crate) const CHAR_FWD: i32 = 1;
/// C: `#define CHAR_BACK (-1)`.
pub(crate) const CHAR_BACK: i32 = -1;

/// C: `#define EL_LEAVE 2` — file-local to `chared.c`. The count of slots at
/// the end of the line buffer that are deliberately left unused, so that
/// `lastchar[1]` is always inside the allocation and `el_line.limit` is
/// always `buffer.len() - EL_LEAVE`.
const EL_LEAVE: usize = 2;

// [spec:libedit:def:chared.c-undo-t]
/// Undo information for vi — there is no undo in emacs (yet).
pub struct CUndoT {
    /// C: `ssize_t len` — length of the saved line, or -1 for "nothing
    /// saved". The sentinel is why this stays signed.
    pub len: isize,
    /// Position of the saved cursor. Already an index in the C, so
    /// `ch_enlargebufs` has nothing to rebase here.
    pub cursor: i32,
    /// C: `wchar_t *buf` — full saved text, owned.
    pub buf: Vec<u32>,
}

// [spec:libedit:def:chared.c-redo-t]
/// Redo for vi.
pub struct CRedoT {
    /// C: `wchar_t *buf` — redo insert key sequence, owned.
    pub buf: Vec<u32>,
    /// C: `wchar_t *pos` — write position, offset into `buf`.
    pub pos: usize,
    /// C: `wchar_t *lim` — usable limit, offset into `buf`. Note that
    /// `ch_enlargebufs` keeps the *old* offset here even as the allocation
    /// grows, so the redo buffer's usable limit does not grow with it; see
    /// `sem:chared.ch-enlargebufs-fn` step 7.
    pub lim: usize,
    /// Command to redo.
    pub cmd: ElActionT,
    /// C: `wchar_t ch` — char that invoked it.
    pub ch: u32,
    pub count: i32,
    /// From `cv_action()`.
    pub action: i32,
}

// [spec:libedit:def:chared.c-vcmd-t]
/// Current action information for vi.
pub struct CVcmdT {
    pub action: i32,
    /// C: `wchar_t *pos` — offset into `el_line.buffer`, not into any
    /// buffer of this struct.
    pub pos: usize,
}

// [spec:libedit:def:chared.c-kill-t]
/// Kill buffer for emacs.
pub struct CKillT {
    /// C: `wchar_t *buf` — the kill buffer, owned.
    pub buf: Vec<u32>,
    /// C: `wchar_t *last` — offset into `buf`.
    pub last: usize,
    /// C: `wchar_t *mark` — offset into **`el_line.buffer`**, not into
    /// `buf`. The asymmetry is the C's: `ch_enlargebufs` rebases `last`
    /// against the old kill base and `mark` against the old line base.
    ///
    /// `sem:emacs.em-set-mark-fn` records the mark's
    /// properties: it starts at the head of the line and is never NULL,
    /// which is why the NULL guards in `em_kill_region` and
    /// `em_copy_region` never fire; nothing but `ch_enlargebufs` and the
    /// explicit setters ever adjusts it, so editing moves text out from
    /// under it and it can end up above `lastchar`.
    pub mark: usize,
}

// [spec:libedit:def:chared.el-zfunc-t-edit-line-void]
/// C: `typedef void (*el_zfunc_t)(EditLine *, void *);`
///
/// The line-resize hook installed by `EL_RESIZE`, called once
/// `ch_enlargebufs` has published the new capacity so the application can
/// re-derive any pointers it holds into the line.
///
/// The application supplies it through `el_set`, so it is the C ABI's shape:
/// `unsafe extern "C"`, with `EditLine *` as `*mut EditLine` rather than
/// `&mut EditLine`. The hook is entitled to call back into libedit through
/// that handle — `el_line` is the documented reason to install one — which is
/// precisely what a `&mut` would forbid.
pub type ElZfuncT = unsafe extern "C" fn(*mut EditLine, *mut c_void);

// [spec:libedit:def:chared.el-afunc-t-void-const-char]
/// C: `typedef const char *(*el_afunc_t)(void *, const char *);`
///
/// The alias-text hook installed by `EL_ALIAS_TEXT`. Application-supplied and
/// so `unsafe extern "C"`; both strings are narrow and borrowed across the C
/// ABI, so they stay raw pointers.
pub type ElAfuncT = unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char;

// [spec:libedit:def:chared.el-chared-t]
/// Both the emacs and the vi state, because the user can bind commands from
/// both editors.
pub struct ElCharedT {
    pub c_undo: CUndoT,
    pub c_kill: CKillT,
    pub c_redo: CRedoT,
    pub c_vcmd: CVcmdT,
    pub c_resizefun: Option<ElZfuncT>,
    pub c_aliasfun: Option<ElAfuncT>,
    /// C: `void *c_resizearg` — client cookie passed back to
    /// `c_resizefun`, never inspected.
    pub c_resizearg: *mut c_void,
    /// C: `void *c_aliasarg` — client cookie passed back to `c_aliasfun`,
    /// never inspected.
    pub c_aliasarg: *mut c_void,
}

/// The character at a line offset — the C's `*p` for a position in
/// `el_line.buffer`.
///
/// Every offset the scanners below form is inside the allocation, because
/// their bounds come from `el_line` itself and the two reserved trailing slots
/// keep `lastchar[1]` in range. The fallback is only what keeps a caller that
/// hands in a bound of its own from panicking where the C would have read
/// whatever was there; `L'\0'` is also what those slots hold after `ch_init`.
fn line_at(el: &EditLine, i: usize) -> u32 {
    el.el_line.buffer.get(i).copied().unwrap_or(0)
}

/// The C's `(*wtest)(el, *p)`: classify the character at a line offset.
fn wtest_at(el: &mut EditLine, i: usize, wtest: fn(&mut EditLine, u32) -> i32) -> i32 {
    let c = line_at(el, i);
    wtest(el, c)
}

/// C: `wcschr(el->el_map.wordchars, p) != NULL`.
///
/// ERR-buffer-21: `wcschr` matches the terminating NUL, so `p == L'\0'` is
/// reported as a word character whatever the set holds. The rule says a port
/// that classifies characters at the buffer edge must reproduce this to stay
/// bit-identical, and ERR-buffer-07's defined behaviour is exactly such a
/// classification, so it is reproduced here rather than tidied away.
///
/// `wordchars` is `None` only before `map_init` has installed a set and after
/// `map_end`, where the C would dereference NULL; defined here as the empty
/// set, which still matches `L'\0'` for the same reason.
fn wordchars_has(el: &EditLine, p: u32) -> bool {
    if p == 0 {
        return true;
    }
    match &el.el_map.wordchars {
        Some(w) => w.iter().take_while(|&&c| c != 0).any(|&c| c == p),
        None => false,
    }
}

/// C: `el_realloc(buf, newsz * sizeof(wchar_t))` followed by
/// `memset(&newbuf[sz], 0, (newsz - sz) * sizeof(wchar_t))` — grow to `newsz`
/// characters, zeroing only the newly added tail and leaving the old contents
/// in place.
///
/// `false` is the C's NULL return, on which the field keeps the block it
/// already had — which is what lets [`ch_enlargebufs`] abandon the growth
/// without leaking or dangling.
fn grow(buf: &mut Vec<u32>, newsz: usize) -> bool {
    if newsz <= buf.len() {
        return true;
    }
    if buf.try_reserve(newsz - buf.len()).is_err() {
        return false;
    }
    buf.resize(newsz, 0);
    true
}

// [spec:libedit:def:chared.cv-undo-fn]
// [spec:libedit:sem:chared.cv-undo-fn]
pub(crate) fn cv_undo(el: &mut EditLine) {
    // Undo half. C: `size = lastchar - buffer`, the line length.
    let size = el.el_line.lastchar;
    // -1 is the "nothing saved" marker `ch_init`/`ch_reset` install; 0 is a
    // legitimately saved empty line, which is why this stays signed.
    el.el_chared.c_undo.len = size as isize;
    // C: `vu->cursor = (int)(cursor - buffer)` — an index in the C as well,
    // so that it survives a `ch_enlargebufs`.
    el.el_chared.c_undo.cursor = el.el_line.cursor as i32;
    // C: `memcpy(vu->buf, el->el_line.buffer, size * sizeof(*vu->buf))`. No
    // terminator is written; `c_undo.len` is the only length. Both buffers
    // start at EL_BUFSIZ and `ch_enlargebufs` grows them together, so the
    // copy always fits and the two `min`s are no-ops — they are here so that
    // the one state where the invariant does not hold, after `ch_end` has
    // released both, truncates instead of panicking.
    let n = size
        .min(el.el_line.buffer.len())
        .min(el.el_chared.c_undo.buf.len());
    el.el_chared.c_undo.buf[..n].copy_from_slice(&el.el_line.buffer[..n]);

    // Redo half: the numeric prefix if one was typed, else 0.
    el.el_chared.c_redo.count = if el.el_state.doingarg != 0 {
        el.el_state.argument
    } else {
        0
    };
    el.el_chared.c_redo.action = el.el_chared.c_vcmd.action;
    // C: `r->pos = r->buf` — rewind the write pointer so that recording of
    // the inserted key sequence starts fresh. The previous contents are not
    // cleared, only orphaned.
    el.el_chared.c_redo.pos = 0;
    el.el_chared.c_redo.cmd = el.el_state.thiscmd;
    el.el_chared.c_redo.ch = el.el_state.thisch;
    // `c_redo.lim` is not touched, and neither is the line.
}

// [spec:libedit:def:chared.cv-yank-fn]
// [spec:libedit:sem:chared.cv-yank-fn]
/// C: `const wchar_t *ptr` — an offset into `el_line.buffer`. Every caller
/// passes `el_line.buffer`, `el_line.cursor`, or the cursor displaced by a
/// count, so this is a line position and not a string of its own.
pub(crate) fn cv_yank(el: &mut EditLine, ptr: usize, size: i32) {
    // ERR-buffer-09: the C computes the copy length as
    // `(size_t)size * sizeof(*k->buf)`, so a negative `size` — which
    // `c_delafter` and `c_delbefore` hand over before their own `num > 0`
    // test rejects it — becomes an enormous length. Defined as the errata
    // asks: the count is required to be non-negative, and a negative one
    // copies nothing, leaving the kill buffer empty.
    let size = size.max(0) as usize;

    // C: `memcpy(k->buf, ptr, size * sizeof(*k->buf))` — always from the
    // START of the kill buffer, so this is a replace and never an append.
    // `ptr` is a position in the *line*; the kill buffer is a separate
    // allocation grown in lockstep with it, and every caller derives `size`
    // from a span of the line, so the clamps below are no-ops.
    let ptr = ptr.min(el.el_line.buffer.len());
    let n = size
        .min(el.el_line.buffer.len() - ptr)
        .min(el.el_chared.c_kill.buf.len());
    el.el_chared.c_kill.buf[..n].copy_from_slice(&el.el_line.buffer[ptr..ptr + n]);
    // C: `k->last = k->buf + size` — what gives the kill buffer its length.
    // `size == 0` therefore empties it, which is exactly what
    // `c_delafter`/`c_delbefore` produce when their clamp reaches zero.
    el.el_chared.c_kill.last = n;
    // `c_kill.mark` is untouched, and so is the line.
}

// [spec:libedit:def:chared.c-insert-fn]
// [spec:libedit:sem:chared.c-insert-fn]
pub(crate) fn c_insert(el: &mut EditLine, num: i32) {
    // ERR-buffer-12: the C does not defend against a negative `num` — the
    // shift below would run the tail LEFT and step 3 would shrink the line,
    // writing below `cursor` and possibly below `buffer`. The only way to
    // produce one is `el_winsertstr`'s `size_t`-to-`int` cast, which carries
    // an unsigned count here, so no caller can; defined as no movement.
    let Ok(num) = usize::try_from(num) else {
        return;
    };

    // Note the `>=`: growth triggers when the projected end merely reaches
    // `limit`. A failed enlargement returns having changed nothing, and is
    // indistinguishable from success to the caller — which is the root of
    // ERR-modes-22, ERR-modes-24 and ERR-modes-41.
    if el.el_line.lastchar + num >= el.el_line.limit && ch_enlargebufs(el, num) == 0 {
        return;
    }

    if el.el_line.cursor < el.el_line.lastchar {
        // C: `for (cp = lastchar; cp >= cursor; cp--) cp[num] = *cp;` — the
        // descending order is what makes the overlapping move correct, and
        // `copy_within`'s memmove semantics give that directly. The range is
        // `[cursor, lastchar]` inclusive, so the slot at `lastchar` moves too
        // and `lastchar - cursor + 1` characters end up at
        // `[cursor + num, lastchar + num]`.
        let (cursor, lastchar) = (el.el_line.cursor, el.el_line.lastchar);
        // The enlargement above leaves `lastchar + num` inside the
        // allocation; the guard defines the state where the C would write
        // past the end as "the tail does not move".
        if lastchar + num < el.el_line.buffer.len() {
            el.el_line
                .buffer
                .copy_within(cursor..=lastchar, cursor + num);
        }
    }
    // ERR-buffer-19: the gap is never blanked, so the `num` slots at
    // `[cursor, cursor + num)` keep whatever was there — shifted-away text,
    // or zeros from the initial allocation. The caller fills it. When
    // `cursor == lastchar` the shift is skipped entirely and appending simply
    // exposes `num` stale slots.
    el.el_line.lastchar += num;
    // `el_line.cursor` is unchanged, and nothing is returned.
}

// [spec:libedit:def:chared.c-delafter-fn]
// [spec:libedit:sem:chared.c-delafter-fn]
pub(crate) fn c_delafter(el: &mut EditLine, num: i32) {
    // 1. Clamp from above only: at most the characters from the cursor to the
    //    end of the line are removed. There is no clamp from below, so a
    //    negative `num` passes through unchanged — and so does the negative
    //    count a cursor above `lastchar` would produce here, exactly as the
    //    C's pointer difference does.
    let mut num = num;
    if el.el_line.cursor as isize + num as isize > el.el_line.lastchar as isize {
        num = (el.el_line.lastchar as isize - el.el_line.cursor as isize) as i32;
    }

    // 2. C: `if (el->el_map.current != el->el_map.emacs) { cv_undo(el);
    //    cv_yank(el, el->el_line.cursor, num); }`.
    //
    //    ERR-modes-19: that test reads as "not in emacs mode" and is not.
    //    `el_map.current` is only ever assigned `el_map.key` or `el_map.alt`,
    //    both heap copies made by `map_init`, whereas `el_map.emacs` is the
    //    static const default table — the two pointers are never equal, so
    //    the condition is a tautology and the undo snapshot plus the
    //    kill-buffer write happen on EVERY call, in emacs mode as much as in
    //    vi. It is observable: backspace over a character in emacs mode and
    //    `^Y` pastes it back, and `el_deletestr` makes it an ABI-visible side
    //    effect of a public API call. Reproduced by having no test at all —
    //    `map::ElMapCurrent` deliberately carries no `Emacs` variant so this
    //    cannot be quietly "fixed" into a real mode check.
    //
    //    This runs after the clamp and before any mutation, and it runs even
    //    when `num` is 0, which leaves the kill buffer empty.
    cv_undo(el);
    cv_yank(el, el.el_line.cursor, num);

    // 3.
    if num > 0 {
        let num = num as usize;
        let (cursor, lastchar) = (el.el_line.cursor, el.el_line.lastchar);
        // C: `for (cp = cursor; cp <= lastchar; cp++) *cp = cp[num];`
        //
        // ERR-buffer-01: that loop runs `num` iterations past the useful end,
        // and its reads reach as far as `lastchar + num` — for a large `num`
        // on a nearly full line, up to roughly a whole buffer past the
        // allocation. Defined as the errata asks: only the in-range tail is
        // copied, `[cursor + num, lastchar]` down onto
        // `[cursor, lastchar - num]`, and the contents left above the new
        // `lastchar` are unspecified. The clamp in step 1 guarantees
        // `cursor + num <= lastchar`.
        el.el_line
            .buffer
            .copy_within(cursor + num..=lastchar, cursor);
        el.el_line.lastchar = lastchar - num;
    }
    // 4. `el_line.cursor` is not moved, so the character formerly at
    //    `cursor + num` is now under it.
}

// [spec:libedit:def:chared.c-delafter1-fn]
// [spec:libedit:sem:chared.c-delafter1-fn]
pub(crate) fn c_delafter1(el: &mut EditLine) {
    let (cursor, lastchar) = (el.el_line.cursor, el.el_line.lastchar);
    // C: `for (cp = cursor; cp <= lastchar; cp++) *cp = cp[1];`
    //
    // `[cursor + 1, lastchar]` slides onto `[cursor, lastchar - 1]`. The
    // final iteration writes at `lastchar`, above the new end, so that slot
    // is left unspecified rather than written; the C's read of `lastchar[1]`
    // stays inside the allocation because of the two reserved trailing slots,
    // but its value can only ever land above the new end.
    //
    // No undo snapshot and no kill-buffer write: that is the whole difference
    // from `c_delafter(el, 1)`.
    if cursor < lastchar {
        el.el_line.buffer.copy_within(cursor + 1..=lastchar, cursor);
    }
    // ERR-buffer-03: there are no guards of any kind. With
    // `cursor == lastchar` the C still decrements, leaving
    // `lastchar == cursor - 1` — a cursor past the end of the line — which is
    // defined behaviour and is reproduced. On an empty line `lastchar` would
    // end up before `buffer` entirely, which is the undefined part; defined
    // here as staying at the start of the line. The caller must guarantee
    // `cursor < lastchar`, and both in-tree callers do.
    el.el_line.lastchar = lastchar.saturating_sub(1);
}

// [spec:libedit:def:chared.c-delbefore-fn]
// [spec:libedit:sem:chared.c-delbefore-fn]
pub(crate) fn c_delbefore(el: &mut EditLine, num: i32) {
    // 1. Clamp: at most everything from the start of the line up to the
    //    cursor is removed. No clamp from below.
    let mut num = num;
    if (el.el_line.cursor as isize) - (num as isize) < 0 {
        num = el.el_line.cursor as i32;
    }

    // 2. The same tautological keymap test as `c_delafter` step 2
    //    (ERR-modes-19) — always true, so the undo snapshot and the
    //    kill-buffer write happen on every call whatever the editing mode.
    //    After the clamp, before any mutation, and even when `num` is 0.
    let start = (el.el_line.cursor as isize - num as isize).max(0) as usize;
    cv_undo(el);
    cv_yank(el, start, num);

    // 3.
    if num > 0 {
        let num = num as usize;
        let (cursor, lastchar) = (el.el_line.cursor, el.el_line.lastchar);
        // C: `for (cp = cursor - num; &cp[num] <= lastchar; cp++)
        // *cp = cp[num];` — `lastchar - cursor + 1` characters, the range
        // `[cursor, lastchar]` including the slot at `lastchar` itself, down
        // onto `[cursor - num, lastchar - num]`. The loop bound keeps every
        // read at or below `lastchar`, so unlike `c_delafter` there is no
        // out-of-range access to define away.
        if cursor <= lastchar {
            el.el_line
                .buffer
                .copy_within(cursor..=lastchar, cursor - num);
        }
        // `num <= cursor` after the clamp, so this only saturates for a
        // cursor already above `lastchar`, where the C would drive `lastchar`
        // below the start of the line.
        el.el_line.lastchar = lastchar.saturating_sub(num);
    }
    // 4. `el_line.cursor` is deliberately NOT adjusted: the text has slid
    //    left underneath it, and the caller must do `cursor -= num` itself
    //    with the same clamped `num`. `el_deletestr` and `cv_delfini` both
    //    do exactly that.
}

// [spec:libedit:def:chared.c-delbefore1-fn]
// [spec:libedit:sem:chared.c-delbefore1-fn]
pub(crate) fn c_delbefore1(el: &mut EditLine) {
    let (cursor, lastchar) = (el.el_line.cursor, el.el_line.lastchar);
    // C: `for (cp = cursor - 1; cp <= lastchar; cp++) *cp = cp[1];`
    //
    // `[cursor, lastchar]` slides onto `[cursor - 1, lastchar - 1]`; as in
    // `c_delafter1` the final write lands at `lastchar`, above the new end,
    // so that slot is left unspecified. No undo snapshot and no kill-buffer
    // write, which is the difference from `c_delbefore(el, 1)`.
    //
    // ERR-buffer-02: with `cursor == buffer` the C forms the pointer
    // `buffer - 1` and its first assignment writes one element before the
    // line buffer. Defined here as "nothing is written below the start of the
    // line": the remaining, in-range part of that loop still runs, so the
    // line still slides left by one and still loses its first character. The
    // caller must guarantee `cursor > buffer`, and both in-tree callers do.
    let dst = cursor.saturating_sub(1);
    if dst < lastchar {
        el.el_line.buffer.copy_within(dst + 1..=lastchar, dst);
    }
    // `el_line.cursor` is not adjusted, so the caller must decrement it
    // itself to stay on the same character.
    el.el_line.lastchar = lastchar.saturating_sub(1);
}

// [spec:libedit:def:chared.ce-isword-fn]
// [spec:libedit:sem:chared.ce-isword-fn]
pub(crate) fn ce_is_word(el: &mut EditLine, p: u32) -> i32 {
    // C: `return iswalnum(p) || wcschr(el->el_map.wordchars, p) != NULL;` —
    // a `||`, so the result is exactly 0 or 1 and never the raw `iswalnum`
    // value. That matters to `c_next_word`/`c_prev_word`, which use it as a
    // boolean, and to nothing else: the emacs test is never compared for
    // equality the way `cv_is_word` is.
    i32::from(locale::iswalnum(locale::charset(), p) || wordchars_has(el, p))
}

// [spec:libedit:def:chared.cv-isword-fn]
// [spec:libedit:sem:chared.cv-isword-fn]
pub(crate) fn cv_is_word(el: &mut EditLine, p: u32) -> i32 {
    // Three-valued, and the two nonzero values are not interchangeable:
    // `cv_next_word`, `cv_prev_word` and `cv_end_word` compare this result
    // for EQUALITY to find runs of a single class, which is how vi's `w`, `b`
    // and `e` stop at the boundary between a word and adjacent punctuation.
    let cs = locale::charset();
    if locale::iswalnum(cs, p) || wordchars_has(el, p) {
        return 1;
    }
    // A printable non-space, non-word character, i.e. punctuation.
    if locale::iswgraph(cs, p) {
        return 2;
    }
    // Whitespace and non-printables.
    0
}

// [spec:libedit:def:chared.cv-is-word-fn]
// [spec:libedit:sem:chared.cv-is-word-fn]
/// The capital `W` is the C's: this is the vi big-word test, `cv_is_word`'s
/// coarser sibling.
pub(crate) fn cv_is_big_word(el: &mut EditLine, p: u32) -> i32 {
    // C: `cv_is_big_word(EditLine *el __attribute__((__unused__)), wint_t p)` —
    // the editor is genuinely unread. The parameter stays because this is one
    // of the interchangeable `wtest` predicates.
    let _ = el;
    // `!iswspace(p)`: exactly 1 or 0, so every non-space character falls into
    // a single class. That is what makes vi's `W`, `B` and `E` treat
    // punctuation as part of the surrounding word where `cv_is_word` splits
    // it out.
    i32::from(!locale::iswspace(locale::charset(), p))
}

// [spec:libedit:def:chared.c-prev-word-fn]
// [spec:libedit:sem:chared.c-prev-word-fn]
/// `p` and `low` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn c_prev_word(
    el: &mut EditLine,
    p: usize,
    low: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    // ERR-buffer-05: the C's working pointer sits at `low - 1` whenever a
    // scan runs off the front, and again after the initial `p--` when the
    // caller's position is `low` itself. Scanned over a signed index here, as
    // the errata asks, so that position is never formed as a pointer.
    let low = low as isize;
    // C: `p--` — scanning starts on the character before the caller's
    // position, so the character at the caller's `p` is never examined.
    let mut p = p as isize - 1;

    // ERR-buffer-22: the C consumes the count as `while (n--)`, so a negative
    // `n` counts down toward INT_MIN rather than stopping — an effectively
    // unbounded loop. Defined as the errata asks: a non-positive count is no
    // movement, which with the `p--`/`p++` pair returns the caller's position
    // clamped to `low`. Nothing reachable through the key dispatcher passes
    // one.
    for _ in 0..n.max(0) {
        // Skip the gap, then skip the word. `wtest` is used as a BOOLEAN
        // here, so with `cv_is_word` punctuation counts as word material —
        // which is ERR-modes-53, since `^W` and `M-b` reach this with
        // `ce_is_word` even in vi.
        while p >= low && wtest_at(el, p as usize, wtest) == 0 {
            p -= 1;
        }
        while p >= low && wtest_at(el, p as usize, wtest) != 0 {
            p -= 1;
        }
    }

    // C: `p++` — now on the first character of the word rather than one
    // before it — and then the clamp, which is live here (unlike
    // `c_next_word`'s).
    p += 1;
    if p < low { low as usize } else { p as usize }
}

// [spec:libedit:def:chared.c-next-word-fn]
// [spec:libedit:sem:chared.c-next-word-fn]
/// `p` and `high` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn c_next_word(
    el: &mut EditLine,
    p: usize,
    high: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    let mut p = p;
    // ERR-buffer-22, as in `c_prev_word`: a non-positive count is defined as
    // no movement, where the C's `while (n--)` would spin on a negative one.
    for _ in 0..n.max(0) {
        // Skip the gap, then skip the word. Nothing at or beyond `high` is
        // dereferenced, because both loops test the bound first.
        while p < high && wtest_at(el, p, wtest) == 0 {
            p += 1;
        }
        while p < high && wtest_at(el, p, wtest) != 0 {
            p += 1;
        }
    }
    // ERR-buffer-23: the C's trailing `if (p > high) p = high;` is dead —
    // both loops stop at `high` — and is not ported.
    p
}

// [spec:libedit:def:chared.cv-next-word-fn]
// [spec:libedit:sem:chared.cv-next-word-fn]
/// `p` and `high` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn cv_next_word(
    el: &mut EditLine,
    p: usize,
    high: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    let cs = locale::charset();
    let mut p = p;
    // C: `while (n--)`, so inside the body `n` already means "iterations
    // still to come after this one" — step 3 depends on that. A non-positive
    // count is no movement (ERR-buffer-22's definition for the identical
    // construct; note the `sem` rule's "with `n <= 0` the body never runs"
    // holds of the C only for `n == 0`).
    let mut n = n;
    while n > 0 {
        n -= 1;

        // 1./2. Classify the character at the current position, then consume
        //       the run it started inside, whatever its class. `wtest`'s
        //       result is compared for EQUALITY, so a run of punctuation
        //       counts as a word of its own.
        //
        //       ERR-buffer-07: the C classifies `*p` without first testing
        //       `p < high`, so a cursor at the end of the line reads the
        //       reserved slot at `lastchar`. Defined as the errata asks:
        //       already at `high` classifies nothing. The value cannot change
        //       the result, because the run loop's guard fails immediately.
        if p < high {
            let test = wtest_at(el, p, wtest);
            while p < high && wtest_at(el, p, wtest) == test {
                p += 1;
            }
        }

        // 3. vi historically deletes with `cw` only the word, preserving the
        //    trailing whitespace — not what `w` does. So on the FINAL
        //    iteration of a pending change-word (`action == DELETE|INSERT`)
        //    the blanks are left alone, while plain `w` and every non-final
        //    iteration consume them. The pending action is read, never
        //    written.
        if n != 0 || el.el_chared.c_vcmd.action != (DELETE | INSERT) {
            while p < high && locale::iswspace(cs, line_at(el, p)) {
                p += 1;
            }
        }
    }

    // C: `if (p > high) return high; else return p;`
    p.min(high)
}

// [spec:libedit:def:chared.cv-prev-word-fn]
// [spec:libedit:sem:chared.cv-prev-word-fn]
/// `p` and `low` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn cv_prev_word(
    el: &mut EditLine,
    p: usize,
    low: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    let cs = locale::charset();
    let lowi = low as isize;
    // C: `p--` — the character the cursor sits on is not considered.
    let mut p = p as isize - 1;

    // A non-positive count is no movement, as in `cv_next_word`.
    for _ in 0..n.max(0) {
        // a. Note the strict `>`: the scan stops at `low` without testing it.
        while p > lowi && locale::iswspace(cs, line_at(el, p as usize)) {
            p -= 1;
        }

        // b. ERR-buffer-06: with `p == low` on entry the `p--` above has
        //    already produced `low - 1`, loop (a)'s guard does not fire, and
        //    the C classifies `*(low - 1)` — one element before the line
        //    buffer. The value never reaches the result, because loop (c)'s
        //    `p >= low` guard stops at once and step (d) returns `low`.
        //    Defined as the errata asks: return `low` as soon as the position
        //    falls below it, and never form that position at all.
        if p < lowi {
            return low;
        }
        let test = wtest_at(el, p as usize, wtest);

        // c. As in `cv_next_word`, the class is compared for equality.
        while p >= lowi && wtest_at(el, p as usize, wtest) == test {
            p -= 1;
        }

        // d. The run reached the start of the buffer: return immediately,
        //    skipping any remaining iterations and the `p++` below.
        if p < lowi {
            return low;
        }
    }

    // C: `p++` — from one-before-the-word onto its first character.
    p += 1;
    // ERR-modes-71: the C's final `if (p < low) return low;` is unreachable
    // after the early return above, and is not ported.
    p as usize
}

// [spec:libedit:def:chared.cv-delfini-fn]
// [spec:libedit:sem:chared.cv-delfini-fn]
pub(crate) fn cv_delfini(el: &mut EditLine) {
    // 1./2. A change operator drops into insert mode. This happens before the
    //       sanity check below, so it happens even when nothing is edited.
    let action = el.el_chared.c_vcmd.action;
    if action & INSERT != 0 {
        el.el_map.current = ElMapCurrent::Key;
    }

    // 3. C: `if (el->el_chared.c_vcmd.pos == 0) return;` — a NULL *pointer*
    //    test, and deliberately NOT translated as `pos == 0`. `c_vcmd.pos` is
    //    an offset into the line here, where 0 is the perfectly ordinary
    //    "operator anchored at the start of the line" that `ch_init` and
    //    `ch_reset` install and that `dw` on column 0 produces; a literal
    //    translation would silently drop every such edit.
    //
    //    The C's NULL is unreachable at this point anyway. The only two
    //    assignments of it — `cv_action`'s doubled-operator path (`dd`, `cc`)
    //    and `vi_command_mode` — set `c_vcmd.action = NOP` in the same
    //    breath, and every call site of `cv_delfini` is guarded by
    //    `action != NOP`. Note the C does not clear `c_vcmd.action` on this
    //    path, so there is nothing to reproduce there either.

    // 4./5. C: `size = cursor - c_vcmd.pos`, a signed character count —
    //       positive if the motion ran forward, negative if backward — and
    //       `size = 1` when it is zero, so a motion that did not move still
    //       affects the character under the cursor.
    //
    //       Held here as the span it names: a low end and a positive length,
    //       plus which end the anchor is. That is what the three arms below
    //       actually need, and both ends come straight from positions already
    //       inside the line, so the C's `cursor + size` is never formed. In
    //       signed arithmetic it is one invariant away from wrapping — a
    //       silent wrap where the C's pointer version is merely undefined —
    //       and the invariant that saves it, `cursor >= 0`, is the one thing
    //       `usize` already guarantees.
    let anchor = el.el_chared.c_vcmd.pos;
    let moved_to = el.el_line.cursor;
    let start = anchor.min(moved_to);
    let len = if anchor == moved_to {
        1
    } else {
        anchor.abs_diff(moved_to)
    };

    // 6. The cursor goes back to the anchor before any edit; each primitive
    //    below works outward from where it already is, which is why only one
    //    of them can be used for a given direction.
    el.el_line.cursor = anchor;

    // 7./8. Apply the operator to the span, and answer where the cursor
    //       belongs afterwards. Every arm answers, so no arm can quietly not
    //       move it — which is the shape the C's three-way asymmetry needs:
    //       the forward delete leaves it where it already is, the backward
    //       delete has to name the low end itself because `c_delbefore`
    //       deliberately does not, and the yank names the anchor because it
    //       moves nothing at all. That last one is why `yb` finishes to the
    //       RIGHT of the text it copied, where `y` was pressed.
    //
    //       `c_delafter`/`c_delbefore` also take the undo snapshot and fill
    //       the kill buffer, so a delete leaves the removed text yankable;
    //       `cv_yank` alone does neither.
    el.el_line.cursor = if action & YANK != 0 {
        cv_yank(el, start, len as i32);
        anchor
    } else if anchor == start {
        c_delafter(el, len as i32);
        // The one arm that redraws. The other two rely on the caller's return
        // code to drive redisplay, so a backward operator leaves the screen
        // model behind until it does.
        re_refresh_cursor(el);
        start
    } else {
        c_delbefore(el, len as i32);
        start
    };

    // 9. `c_vcmd.pos` is left as it was.
    el.el_chared.c_vcmd.action = NOP;
}

// [spec:libedit:def:chared.cv-endword-fn]
// [spec:libedit:sem:chared.cv-endword-fn]
/// `p` and `high` are offsets into `el_line.buffer`, and so is the result.
pub(crate) fn cv_end_word(
    el: &mut EditLine,
    p: usize,
    high: usize,
    n: i32,
    wtest: fn(&mut EditLine, u32) -> i32,
) -> usize {
    let cs = locale::charset();
    // C: `p++` — so the character the cursor already sits on cannot end the
    // first word.
    let mut p = p + 1;

    // A non-positive count is no movement, as in `cv_next_word`; the C's
    // `p++`/`p--` pair then returns the caller's position.
    for _ in 0..n.max(0) {
        // a.
        while p < high && locale::iswspace(cs, line_at(el, p)) {
            p += 1;
        }
        // b./c. Classify the character now under `p`, then consume its run.
        //       `wtest`'s result is compared for EQUALITY, so with
        //       `cv_is_word` a run of punctuation is a word in its own right.
        //
        //       ERR-buffer-07, as in `cv_next_word`: the C's classification
        //       is unguarded and reads the reserved slot at `lastchar` once
        //       the scan has reached `high`. Defined as classifying nothing
        //       there; loop (c) could not have advanced anyway.
        if p < high {
            let test = wtest_at(el, p, wtest);
            while p < high && wtest_at(el, p, wtest) == test {
                p += 1;
            }
        }
    }

    // C: `p--` — from one-past-the-word back onto its last character. There
    // is no clamp on the way out: a scan that reaches `high` gives
    // `high - 1`, which is the intended "last character of the line" when
    // `high` is `lastchar`.
    p - 1
}

// [spec:libedit:def:chared.ch-init-fn]
// [spec:libedit:sem:chared.ch-init-fn]
pub(crate) fn ch_init(el: &mut EditLine) -> i32 {
    // C: `el_calloc(EL_BUFSIZ, sizeof(wchar_t))` for each of the four
    // buffers, so all four start zeroed and the same size — an invariant
    // `ch_enlargebufs` maintains and that `cv_undo`, `cv_yank` and
    // `em_kill_region` all rely on.
    //
    // The C's three `-1` returns are allocation failures, which a Rust
    // allocation does not report — it aborts — so they are unreachable here,
    // and with them ERR-buffer-13's inconsistency (the step-3 failure
    // returning -1 without freeing the line buffer, leaking it while
    // `el_line.buffer` still points at live memory). Its disposition is
    // "fix", and the leak is not ABI-observable. Callers still get the C's 0;
    // ERR-core-api-02 records that `el_init_internal` discards it anyway.
    el.el_line.buffer = vec![0u32; EL_BUFSIZ];
    el.el_line.cursor = 0;
    el.el_line.lastchar = 0;
    // C: `&el->el_line.buffer[EL_BUFSIZ - EL_LEAVE]` — the last two slots are
    // reserved, which is what keeps `lastchar[1]` in the allocation.
    el.el_line.limit = EL_BUFSIZ - EL_LEAVE;

    el.el_chared.c_undo.buf = vec![0u32; EL_BUFSIZ];
    // The "nothing saved" marker.
    el.el_chared.c_undo.len = -1;
    el.el_chared.c_undo.cursor = 0;

    el.el_chared.c_redo.buf = vec![0u32; EL_BUFSIZ];
    el.el_chared.c_redo.pos = 0;
    // C: `c_redo.buf + EL_BUFSIZ` — one past the last slot, and the offset
    // `ch_enlargebufs` deliberately never grows (ERR-buffer-20).
    el.el_chared.c_redo.lim = EL_BUFSIZ;
    el.el_chared.c_redo.cmd = ED_UNASSIGNED;
    // `c_redo.count`, `c_redo.action` and `c_redo.ch` are left alone: the
    // whole `EditLine` came from `calloc` in `el_init_internal`, so they are
    // already zero.

    el.el_chared.c_vcmd.action = NOP;
    el.el_chared.c_vcmd.pos = 0;

    el.el_chared.c_kill.buf = vec![0u32; EL_BUFSIZ];
    // A position in the LINE, not in the kill buffer.
    el.el_chared.c_kill.mark = 0;
    // Kill buffer empty.
    el.el_chared.c_kill.last = 0;

    el.el_chared.c_resizefun = None;
    el.el_chared.c_resizearg = ptr::null_mut();
    el.el_chared.c_aliasfun = None;
    el.el_chared.c_aliasarg = ptr::null_mut();

    el.el_map.current = ElMapCurrent::Key;

    el.el_state.inputmode = MODE_INSERT;
    el.el_state.doingarg = 0;
    el.el_state.metanext = 0;
    el.el_state.argument = 1;
    el.el_state.lastcmd = ED_UNASSIGNED;

    0
}

// [spec:libedit:def:chared.ch-reset-fn]
// [spec:libedit:sem:chared.ch-reset-fn]
pub(crate) fn ch_reset(el: &mut EditLine) {
    // Every assignment here is unconditional; the function takes no
    // decisions. Note the line's contents are NOT cleared — the previous text
    // stays in the buffer above `lastchar` — and neither is `el_line.limit`.
    //
    // When called from `ch_end` the line buffer has already been released, so
    // the four positions below become 0 against an empty buffer, which is
    // that path's equivalent of the C's NULL.
    el.el_line.cursor = 0;
    el.el_line.lastchar = 0;

    el.el_chared.c_undo.len = -1;
    el.el_chared.c_undo.cursor = 0;

    el.el_chared.c_vcmd.action = NOP;
    el.el_chared.c_vcmd.pos = 0;

    // The kill buffer's *contents* and `c_kill.last` are deliberately not
    // touched, so a previous kill survives into the next line; only the mark
    // goes back to the head of the line.
    el.el_chared.c_kill.mark = 0;

    // `el_map.key` holds the emacs bindings in emacs mode and the vi INSERT
    // bindings in vi mode, so every line starts in insert mode.
    el.el_map.current = ElMapCurrent::Key;

    el.el_state.inputmode = MODE_INSERT;
    el.el_state.doingarg = 0;
    el.el_state.metanext = 0;
    el.el_state.argument = 1;
    el.el_state.lastcmd = ED_UNASSIGNED;

    // Back to the newest history entry.
    el.el_history.eventno = 0;
}

// [spec:libedit:def:chared.ch-enlargebufs-fn]
// [spec:libedit:sem:chared.ch-enlargebufs-fn]
pub(crate) fn ch_enlargebufs(el: &mut EditLine, addlen: usize) -> i32 {
    // Returns 1 on success and 0 on failure — the inverted convention
    // `hist_enlargebuf` shares.

    // 1./2./3. `sz` is the current allocation in characters, and the newly
    //          added space alone must cover `addlen`.
    let sz = el.el_line.limit + EL_LEAVE;
    let mut newsz = sz * 2;
    if addlen > sz {
        while newsz - sz < addlen {
            newsz *= 2;
        }
    }

    // ERR-buffer-08: the C rebases `el_line.cursor`, `el_line.lastchar`,
    // `c_kill.last`, `c_kill.mark` and `c_redo.pos`/`c_redo.lim` by reading
    // the OLD pointer values after the `realloc` that invalidated them. All
    // six are offsets here, so every one of those rebases is nothing at all —
    // including the asymmetric one the C gets right, `c_kill.mark` rebased
    // against the old LINE base because it is a position in the line and not
    // in the kill buffer.
    //
    // Each step below returns 0 immediately on failure, leaving the earlier
    // steps' growth in place: the growth is then partial but consistent, no
    // live position is stale, and `el_line.limit` still describes the
    // pre-call capacity because it is only published in step 9. That is the
    // whole observable point of the C's two-phase `limit` update — its first
    // phase, `limit = &newbuffer[sz - EL_LEAVE]`, restates the value `limit`
    // already holds as an offset, so there is nothing here to assign.

    // 4. Line buffer.
    if !grow(&mut el.el_line.buffer, newsz) {
        return 0;
    }
    // 5. Kill buffer.
    if !grow(&mut el.el_chared.c_kill.buf, newsz) {
        return 0;
    }
    // 6. Undo buffer. Nothing to rebase: `c_undo.cursor` is an index and
    //    `c_undo.len` a length in the C as well.
    if !grow(&mut el.el_chared.c_undo.buf, newsz) {
        return 0;
    }
    // 7. Redo buffer. ERR-buffer-20 records two deliberate asymmetries with
    //    the steps above. The first is observable and is reproduced: the C
    //    rebases `c_redo.lim` against the old base, so it keeps its old
    //    offset and the redo buffer's usable limit does not grow even though
    //    its allocation does — vi `.` sees the cap. Leaving the field alone
    //    is exactly that. The second is not: the C does not zero the new
    //    tail, which leaves `realloc`'s indeterminate bytes there, and zeros
    //    are one instance of indeterminate.
    if !grow(&mut el.el_chared.c_redo.buf, newsz) {
        return 0;
    }

    // 8.
    if hist_enlargebuf(el, newsz) == 0 {
        return 0;
    }

    // 9. Safe to publish the enlarged capacity.
    el.el_line.limit = newsz - EL_LEAVE;

    // 10. The application can now re-derive any positions it holds into the
    //     line. Note this runs after `limit` is published, which
    //     `sem:chared.ch-resizefun-fn` makes part of the hook's contract.
    if let Some(f) = el.el_chared.c_resizefun {
        let a = el.el_chared.c_resizearg;
        // SAFETY: `f` and `a` were installed together by `ch_resizefun` from
        // `el_set(EL_RESIZE, f, a)`, whose contract
        // (`def:chared.el-zfunc-t-edit-line-void`) is a C function taking the
        // `EditLine *` it was registered against. `el` is that handle, live
        // and exclusively borrowed here; the borrow is released for the call,
        // which the rule requires because the hook may re-enter `el_line`.
        unsafe { f(ptr::from_mut(el), a) };
    }
    1
}

// [spec:libedit:def:chared.ch-end-fn]
// [spec:libedit:sem:chared.ch-end-fn]
pub(crate) fn ch_end(el: &mut EditLine) {
    // Idempotent, as the C is: every buffer it releases it then empties, and
    // releasing an already-empty one is a no-op. `ch_init` calls this on its
    // own failure paths, which is why every step tolerates partially
    // initialised state.
    el.el_line.buffer = Vec::new();
    // C: `el->el_line.limit = NULL`.
    el.el_line.limit = 0;
    el.el_chared.c_undo.buf = Vec::new();
    el.el_chared.c_redo.buf = Vec::new();
    el.el_chared.c_redo.pos = 0;
    el.el_chared.c_redo.lim = 0;
    el.el_chared.c_redo.cmd = ED_UNASSIGNED;
    el.el_chared.c_kill.buf = Vec::new();
    // ERR-buffer-14, disposition "fix": the C leaves `c_kill.last` pointing
    // into the freed kill buffer, since neither this nor `ch_reset` touches
    // it. Nothing reads it before the next `ch_init` or `cv_yank` reassigns
    // it, so the rule states that nulling it is not observable.
    el.el_chared.c_kill.last = 0;
    // Re-derives `cursor`, `lastchar`, `c_vcmd.pos` and `c_kill.mark` from
    // the now-empty line buffer, and resets the editor state fields with
    // them.
    ch_reset(el);
}

// [spec:libedit:def:chared.el-winsertstr-fn]
// [spec:libedit:sem:chared.el-winsertstr-fn]
/// C: `const wchar_t *s` — a NUL-terminated string the caller owns and
/// libedit only reads, so the length comes from the slice rather than from
/// `wcslen`. The C's `s == NULL` and `wcslen(s) == 0` rejections are the
/// same case here: an empty slice.
pub fn el_winsertstr(el: &mut EditLine, s: &[u32]) -> i32 {
    // 1. C: `if (s == NULL || (len = wcslen(s)) == 0) return -1;` — an empty
    //    insert is an error, not a no-op.
    let len = s.len();
    if len == 0 {
        return -1;
    }

    // 2. Note the `>=`, as in `c_insert`.
    if el.el_line.lastchar + len >= el.el_line.limit && ch_enlargebufs(el, len) == 0 {
        return -1;
    }

    // 3. ERR-buffer-12: the C casts `len` from `size_t` to `int` here, and a
    //    string longer than INT_MAX would hand `c_insert` a negative count
    //    and corrupt the line. Unreachable in practice; defined here as a
    //    refusal, since `c_insert`'s parameter keeps the C's `int`.
    //    `c_insert` repeats step 2's capacity test internally, which is
    //    harmless.
    let Ok(n) = i32::try_from(len) else {
        return -1;
    };
    c_insert(el, n);

    // 4. C: `while (*s) *el->el_line.cursor++ = *s++;` — the gap is exactly
    //    filled and the cursor ends up immediately after the inserted text.
    //    No NUL is written into the line: `lastchar` is the only end marker.
    let start = el.el_line.cursor;
    if start + len <= el.el_line.buffer.len() {
        el.el_line.buffer[start..start + len].copy_from_slice(s);
    }
    el.el_line.cursor = start + len;
    0
}

// [spec:libedit:def:chared.el-deletestr-fn]
// [spec:libedit:sem:chared.el-deletestr-fn]
pub fn el_deletestr(el: &mut EditLine, n: i32) {
    // 1.
    if n <= 0 {
        return;
    }
    let n = n as usize;

    // 2. C: `if (el->el_line.cursor < &el->el_line.buffer[n]) return;` —
    //    fewer than `n` characters exist before the cursor. All-or-nothing:
    //    it does NOT delete the characters that are available.
    if el.el_line.cursor < n {
        return;
    }

    // 3. Because of the tautological keymap test inside `c_delbefore`
    //    (ERR-modes-19), this also takes a vi undo snapshot and overwrites
    //    the kill buffer with the deleted text, in emacs mode as much as in
    //    vi — an ABI-visible side effect of calling `el_deletestr`, and one
    //    `[dec:libedit:no-c-ffi]` freezes.
    c_delbefore(el, n as i32);

    // 4. The caller-side adjustment `c_delbefore` requires.
    el.el_line.cursor -= n;
    // 5. ERR-buffer-23: the C's `if (cursor < buffer) cursor = buffer` is
    //    dead — step 2 already guarantees it cannot fire — and is not ported.
    //    No redisplay is triggered.
}

// [spec:libedit:def:chared.el-deletestr1-fn]
// [spec:libedit:sem:chared.el-deletestr1-fn]
pub fn el_deletestr1(el: &mut EditLine, start: i32, end: i32) -> i32 {
    // 1.
    if end <= start {
        return 0;
    }

    // 2.
    let line_length = el.el_line.lastchar as i32;

    // 3. ERR-buffer-16: the second test is `>=`, not `>`, so a range ending
    //    exactly at the end of the line is rejected and the final character
    //    of the line can never be deleted through this entry point.
    //    Reproduced.
    if start >= line_length || end >= line_length {
        return 0;
    }
    // A negative `start` survives those tests and sends the C's `p1` below
    // the line buffer, which is undefined; defined here as a rejected range.
    // `rl_delete_text` is the only caller and computes both from `rl_point`
    // and `rl_end`.
    if start < 0 {
        return 0;
    }

    // 4./5. ERR-buffer-15, disposition "reproduce": deleting `[start, end)`
    //       correctly requires moving the entire tail `[end, line_length)`
    //       down to `start` and shortening the line by `end - start`. The C
    //       instead moves `min(end - start, line_length - end)` characters
    //       and shortens the line by that same clamped count, decrementing
    //       `lastchar` once per character copied. Both failure modes are
    //       reachable: with a long tail the length ends up right and the
    //       content does not (`abcdefgh`, 1, 3 reads back `adedef`), and with
    //       a short tail the tail moves correctly but the line is left too
    //       long, exposing stale characters (`abcdefgh`, 1, 6 reads back
    //       `aghdef` where `agh` is correct).
    //
    //       `[dec:libedit:conformance-policy]` names this as one of the six
    //       forks defaulting to reproduce, and the rule forbids resolving it
    //       silently by writing the obvious correct loop. So: bug for bug.
    let mut len = (end - start) as usize;
    let tail = (line_length - end) as usize;
    if len > tail {
        len = tail;
    }
    let p1 = start as usize;
    let p2 = end as usize;
    el.el_line.buffer.copy_within(p2..p2 + len, p1);
    el.el_line.lastchar -= len;

    // 6. ERR-buffer-23: the C's `if (cursor < buffer) cursor = buffer` is
    //    dead code and is not ported. ERR-buffer-18: the cursor is NOT
    //    adjusted for the deletion, so it can be left pointing above the new
    //    `lastchar`. Reproduced by leaving it alone.

    // 7. ERR-buffer-17: the size of the range that was *requested*,
    //    regardless of how many characters were actually removed.
    end - start
}

// [spec:libedit:def:chared.el-wreplacestr-fn]
// [spec:libedit:sem:chared.el-wreplacestr-fn]
/// `s` is borrowed exactly as in [`el_winsertstr`].
pub fn el_wreplacestr(el: &mut EditLine, s: &[u32]) -> i32 {
    // 1. Clearing the line by passing an empty string is not supported.
    let len = s.len();
    if len == 0 {
        return -1;
    }

    // 2. The test is against `buffer`, not `lastchar`, because the new
    //    content starts at the beginning of the line.
    if len >= el.el_line.limit && ch_enlargebufs(el, len) == 0 {
        return -1;
    }

    // 3. The destination is taken AFTER the growth, so a reallocation there
    //    is handled correctly — which in this representation it is anyway.
    el.el_line.buffer[..len].copy_from_slice(s);
    // 4. In bounds because `limit` always leaves two unused slots at the end
    //    of the allocation.
    el.el_line.buffer[len] = 0;
    // 5.
    el.el_line.lastchar = len;
    // 6. Clamped downward only: otherwise the cursor keeps its absolute
    //    offset into the line, never moved to the end or to the start.
    if el.el_line.cursor > el.el_line.lastchar {
        el.el_line.cursor = el.el_line.lastchar;
    }
    // Any previous content beyond `len` stays in the buffer above `lastchar`
    // and is not visible. No redisplay is triggered.
    0
}

// [spec:libedit:def:chared.el-cursor-fn]
// [spec:libedit:sem:chared.el-cursor-fn]
pub fn el_cursor(el: &mut EditLine, n: i32) -> i32 {
    // 1. `n == 0` skips straight to the return, so no clamping happens on
    //    that path: a cursor somehow already out of range is reported as-is
    //    rather than corrected. `[dec:libedit:no-c-ffi]` freezes both that
    //    short-circuit and the clamped value below.
    if n != 0 {
        // 2./3./4. ERR-buffer-11: the C adds `n` to the cursor *pointer* and
        //          only then clamps, transiently forming a pointer far
        //          outside the line allocation. Saturating index arithmetic
        //          is the defined behaviour the errata asks for; the clamps
        //          apply low-then-high exactly as the C's do, so for any
        //          `n != 0` the result is in `[0, lastchar]` however large
        //          `n` is.
        let moved = (el.el_line.cursor as isize).saturating_add(n as isize);
        el.el_line.cursor = moved.clamp(0, el.el_line.lastchar as isize) as usize;
    }
    // 5. The zero-based offset of the cursor from the start of the line.
    //    Nothing else changes and no redisplay is triggered.
    el.el_line.cursor as i32
}

/// C: `EL_BUFSIZ - 16` — the most characters [`c_gets`] accepts. The test is
/// `>=` and runs BEFORE the store, so exactly this many fit and the next one
/// is beeped away rather than truncating the read; the terminator then lands
/// on `buf[C_GETS_MAX]`, which is why the caller's storage must hold one more.
const C_GETS_MAX: usize = EL_BUFSIZ - 16;

/// What one keystroke means to [`c_gets`].
///
/// Classifying is the whole of the C's `switch` and depends on nothing but the
/// character and how much has already been accepted — no editor, no screen and
/// no reader — which is what puts the length cap within reach of a test that
/// does not have to push 1010 keystrokes through `el_wgetc` and `re_refresh`
/// to get at it.
#[derive(Debug, PartialEq, Eq)]
enum Keystroke {
    /// Backspace or DEL with something typed: uncount the last character. It
    /// stays in `buf` above the count and the next store overwrites it.
    Erase,
    /// Backspace or DEL on empty input: abort the whole read.
    Abort,
    /// ESC, CR or LF. ERR-modes-67: `c_gets` treats all three alike, which is
    /// what lets ESC submit a vi search match. The terminator itself is stored
    /// at `buf[len]` without being counted.
    Terminate,
    /// An ordinary character: store it and redraw.
    Store,
    /// The cap is reached, so the character is beeped away and discarded.
    TooLong,
}

fn c_gets_classify(ch: u32, len: usize) -> Keystroke {
    match ch {
        // Delete and backspace.
        0x08 | 0o177 => {
            if len == 0 {
                Keystroke::Abort
            } else {
                Keystroke::Erase
            }
        }
        // ESC, CR, LF.
        0o33 | 0x0d | 0x0a => Keystroke::Terminate,
        _ if len >= C_GETS_MAX => Keystroke::TooLong,
        _ => Keystroke::Store,
    }
}

// [spec:libedit:def:chared.c-gets-fn]
// [spec:libedit:sem:chared.c-gets-fn]
/// `buf` is caller storage, not part of `el` — both callers pass a local
/// `wchar_t[EL_BUFSIZ]` — so it stays a borrowed slice rather than becoming
/// an index. It must hold [`C_GETS_MAX`] + 1 characters; a shorter one is a
/// refused read. `prompt` is optional because the C tests it against NULL.
pub(crate) fn c_gets(el: &mut EditLine, buf: &mut [u32], prompt: Option<&[u32]>) -> i32 {
    // The rule's contract on the caller's storage — room for `C_GETS_MAX + 1`
    // characters — taken once, as a window of exactly that size. Nothing below
    // needs a bounds test after this, because the largest `len` the loop can
    // reach is the last index of the window. A shorter slice is undefined in
    // the C and is a refused read here rather than the 1009 silently dropped
    // writes it used to be; `cv_search` passes 1022 characters and
    // `ed_command` 1024.
    let Some(buf) = buf.get_mut(..=C_GETS_MAX) else {
        return -1;
    };

    // ERR-buffer-10: the C never checks its writes into the line buffer
    // against `el_line.limit` — the draw position can reach
    // `buffer + wcslen(prompt) + 1008` and one more character is stored there,
    // so a prompt longer than 15 characters combined with maximal input runs
    // past the initial 1024-slot line buffer. Defined as the errata asks:
    // every store into the line is bounded by `limit`, and the cursor and
    // `lastchar` stop there too, so the display simply stops advancing while
    // typing continues into `buf`. The two in-tree callers use prompts of 2
    // and 3 characters, so nothing reachable today is clamped.

    // 1./2. The prompt goes to the START of the line buffer, destroying
    //       whatever the user was editing; the line is cleared again on exit.
    let prompt = prompt.unwrap_or(&[]);
    let copied = prompt.len().min(el.el_line.limit);
    el.el_line.buffer[..copied].copy_from_slice(&prompt[..copied]);

    // 3. The C also carries `cp`, the draw position in the line. It is
    //    `wcslen(prompt) + len` at every point of the loop — the two rise and
    //    fall together and neither moves when the cap beeps — so it is derived
    //    below instead of tracked, which leaves exactly one place where a line
    //    index is formed and therefore exactly one place to apply `limit`.
    let mut len = 0usize;
    let mut aborted = false;

    loop {
        // 4. The space is the blank the cursor sits on while typing.
        let at = (prompt.len() + len).min(el.el_line.limit);
        el.el_line.cursor = at;
        el.el_line.buffer[at] = u32::from(b' ');
        el.el_line.lastchar = at + 1;
        re_refresh(el);

        // 5. Anything other than exactly 1 is EOF or a read error.
        let mut ch = 0u32;
        if el_wgetc(el, &mut ch) != 1 {
            // ERR-modes-37: the `CC_EOF` this produces is discarded, and the
            // -1 below is indistinguishable from a cancelled read.
            ed_end_of_file(el, 0);
            aborted = true;
            break;
        }

        // 6.
        match c_gets_classify(ch, len) {
            Keystroke::Abort => {
                aborted = true;
                break;
            }
            Keystroke::Erase => len -= 1,
            Keystroke::Terminate => {
                // Stored WITHOUT incrementing `len`, so the return does not
                // count it; callers overwrite it with `L'\0'`. `buf` is never
                // NUL-terminated here.
                buf[len] = ch;
                break;
            }
            Keystroke::TooLong => terminal_beep(el),
            Keystroke::Store => {
                buf[len] = ch;
                // The same position the cursor space went to, before the count
                // advances past it. Clamped away entirely once the draw
                // position has run up against `limit`, which is ERR-buffer-10
                // above.
                if at < el.el_line.limit {
                    el.el_line.buffer[at] = ch;
                }
                len += 1;
            }
        }
    }

    // On every exit path, unconditionally. Only the first cell is cleared:
    // the prompt and the typed text remain in the buffer above `lastchar`.
    el.el_line.buffer[0] = 0;
    el.el_line.lastchar = 0;
    el.el_line.cursor = 0;
    // The number of characters written to `buf`, not counting the terminator
    // left at `buf[len]`; or -1 on EOF, read error, or backspace on empty
    // input.
    if aborted { -1 } else { len as i32 }
}

// [spec:libedit:def:chared.c-hpos-fn]
// [spec:libedit:sem:chared.c-hpos-fn]
pub(crate) fn c_hpos(el: &mut EditLine) -> i32 {
    // How many characters till the beginning of this line, where an embedded
    // `L'\n'` in the edit buffer starts a new physical line. Modifies nothing.
    if el.el_line.cursor == 0 {
        return 0;
    }
    // C: `for (ptr = cursor - 1; ptr >= buffer && *ptr != '\n'; ptr--)` then
    // `return cursor - ptr - 1`.
    //
    // ERR-buffer-04: a line with no embedded newline leaves `ptr` at
    // `buffer - 1`, and merely forming that pointer is undefined. Defined as
    // the errata asks: the scan runs over indices, and "no newline found" is
    // the column `cursor - buffer`. `i` here is one past where the C's `ptr`
    // stops, so the subtraction is the same count either way.
    let mut i = el.el_line.cursor;
    while i > 0 && line_at(el, i - 1) != u32::from(b'\n') {
        i -= 1;
    }
    (el.el_line.cursor - i) as i32
}

// [spec:libedit:def:chared.ch-resizefun-fn]
// [spec:libedit:sem:chared.ch-resizefun-fn]
/// `f` is optional because `el_set(EL_RESIZE, ...)` may pass NULL, which
/// stores NULL and so switches the hook back off.
pub fn ch_resizefun(el: &mut EditLine, f: Option<ElZfuncT>, a: *mut c_void) -> i32 {
    // Unconditional and unvalidated; `None` clears the hook. It cannot fail —
    // the `i32` exists only so `el_set(EL_RESIZE, f, a)` can propagate a
    // status.
    el.el_chared.c_resizefun = f;
    el.el_chared.c_resizearg = a;
    0
}

// [spec:libedit:def:chared.ch-aliasfun-fn]
// [spec:libedit:sem:chared.ch-aliasfun-fn]
/// `f` is optional for the same reason as in [`ch_resizefun`].
pub fn ch_aliasfun(el: &mut EditLine, f: Option<ElAfuncT>, a: *mut c_void) -> i32 {
    // As in `ch_resizefun`: unconditional, unvalidated, cannot fail. `None`
    // clears the hook, and `vi_alias` then returns `CC_ERROR` without calling
    // anything.
    el.el_chared.c_aliasfun = f;
    el.el_chared.c_aliasarg = a;
    0
}

#[cfg(test)]
mod test;
