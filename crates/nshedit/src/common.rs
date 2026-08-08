//! Ported from `src/common.c`; rules live in `docs/spec/port/src/common.md`.

use crate::chared::{
    MODE_INSERT, MODE_REPLACE_1, NOP, c_delafter, c_delbefore, c_gets, c_hpos, c_insert,
    c_prev_word, ce_is_word, ch_enlargebufs, ch_reset,
};
use crate::el::{EL_BUFSIZ, EditLine, ElActionT};
use crate::fcns::EM_UNIVERSAL_ARGUMENT;
use crate::hist::{hist_first, hist_get, hist_next};
use crate::histedit::{
    CC_ARGHACK, CC_CURSOR, CC_EOF, CC_ERROR, CC_NEWLINE, CC_NORM, CC_REDISPLAY, CC_REFRESH,
    CC_REFRESH_BEEP,
};
use crate::locale;
use crate::map::{ElMapCurrent, MAP_VI};
use crate::parse::parse_line;
use crate::read::el_wgetc;
use crate::refresh::{re_clear_display, re_fastaddc, re_goto_bottom, re_refresh};
use crate::search::{c_hmatch, c_setpat};
use crate::terminal::{terminal_beep, terminal_clear_screen, terminal_putc};
use crate::tty::{tty_noquotemode, tty_quotemode};
use crate::vi::{end_motion, end_vi_motion, vi_command_mode};

/// C: the bare `1000000` each of the three repeat-count accumulators tests
/// `el_state.argument` against — the two here and `em_universal_argument`.
///
/// ERR-modes-49: all three test it *before* they multiply, so this is not the
/// largest count they will hold. A digit accumulator reaches 10000009 and
/// `em_universal_argument` reaches 4000000. Naming the number does not change
/// that; it keeps the three from drifting apart.
pub(crate) const ARGUMENT_CAP: i32 = 1_000_000;

/// C: `iswdigit`.
///
/// Belongs in [`crate::locale`] beside `iswspace`/`iswalnum`, which is where a
/// second caller should find it; it is private here because this module may
/// not add to that one.
///
/// ASCII `0`-`9` in both of the port's charsets. POSIX pins the `digit` class
/// to exactly those ten characters in every locale, and that is what the
/// `iswalnum` translation already assumes ("`digit` is only ASCII `0`-`9` in
/// every locale"). ERR-modes-48 — `c - '0'` being meaningless for a non-ASCII
/// decimal digit — is therefore unreachable here rather than reproduced: no
/// such character passes this test. It takes no [`locale::Charset`] for that
/// reason: the answer is the same in both.
fn iswdigit(c: u32) -> bool {
    (0x30..=0x39).contains(&c)
}

/// C: `*p = c` for a line position the C stores through unchecked.
///
/// Every call site is a position the C knows is writable: `cursor` inside the
/// text, or `lastchar`/`lastchar + 1`, which `el_line.limit` keeps two slots
/// below the end of the allocation precisely so those two stores are in
/// bounds. The bounds test is what keeps that reasoning from becoming a panic
/// if some caller ever breaks the invariant.
fn line_put(el: &mut EditLine, at: usize, c: u32) {
    if let Some(slot) = el.el_line.buffer.get_mut(at) {
        *slot = c;
    }
}

/// C: `*p` for a line position the C reads through unchecked, with the same
/// reasoning as [`line_put`].
///
/// A read past the end answers `L'\0'`, which every caller here treats as
/// "not whitespace and not a newline" and so as a stop.
fn line_get(el: &EditLine, at: usize) -> u32 {
    el.el_line.buffer.get(at).copied().unwrap_or(0)
}

/// C: `*el->el_line.lastchar = '\0'` — the "just in case" terminator several
/// of these commands write before handing the line to something that reads it
/// as a string.
fn line_terminate(el: &mut EditLine) {
    let at = el.el_line.lastchar;
    line_put(el, at, 0);
}

/// C: the `for (p = …, kp = el->el_chared.c_kill.buf; p < …; p++) *kp++ = *p`
/// loops that fill the kill buffer from a range of the line, followed by
/// `el->el_chared.c_kill.last = kp` — the one shape every kill and copy in
/// this module and in [`crate::emacs`] uses.
///
/// `kp` starts at index 0 every time, which is ERR-modes-54: kills never
/// accumulate in either direction, and saving a zero-length span sets `last`
/// to 0 and so *empties* the kill buffer, after which `em_yank` returns
/// `CC_NORM` and inserts nothing.
///
/// The C bounds-checks nothing and needs to: the kill buffer is allocated and
/// reallocated to the same size as the line buffer, and every range handed
/// here is a subrange of that allocation — `em_kill_region` reaches here with
/// a mark that may sit above `lastchar`, but a mark is only ever a former
/// cursor value or the head of the line. The clamp never bites.
pub(crate) fn kill_save(el: &mut EditLine, from: usize, to: usize) {
    let mut kp = 0;
    let mut p = from;
    while p < to {
        let Some(&c) = el.el_line.buffer.get(p) else {
            break;
        };
        let Some(slot) = el.el_chared.c_kill.buf.get_mut(kp) else {
            break;
        };
        *slot = c;
        kp += 1;
        p += 1;
    }
    el.el_chared.c_kill.last = kp;
}

/// C: `wcsncpy(el->el_history.buf, el->el_line.buffer, el->el_history.sz)`
/// followed by `el->el_history.last = el->el_history.buf + (lastchar - buffer)`
/// — the stash of the live line that `ed_prev_history` and
/// `ed_search_prev_history` take on their way off event 0.
///
/// `wcsncpy` semantics, padding included: copy the source up to its own NUL,
/// then NUL-fill the rest of the stash. The callers write `*lastchar = '\0'`
/// first, so the source string is the line up to `lastchar`.
///
/// ERR-history-10, defined here: the C's `wcsncpy` leaves the destination
/// unterminated when the source fills it, and `ed_search_next_history` then
/// runs `wcsstr`/`regexec` off the end of it. The rule directs the port to
/// carry the length explicitly, so `last` is the record and [`hist_saved_line`]
/// is the only reader. `last` is additionally clamped to what was actually
/// stored; the C computes it from the line length even when the copy was
/// truncated, which would leave the recorded length describing storage the
/// stash does not have.
fn hist_save_line(el: &mut EditLine) {
    let n = el.el_history.buf.len();
    let mut i = 0;
    while i < n {
        let c = el.el_line.buffer.get(i).copied().unwrap_or(0);
        if c == 0 {
            break;
        }
        el.el_history.buf[i] = c;
        i += 1;
    }
    while i < n {
        el.el_history.buf[i] = 0;
        i += 1;
    }
    el.el_history.last = el.el_line.lastchar.min(n);
}

/// The stashed live line as `c_hmatch` would read it — the recorded length per
/// ERR-history-10, truncated at an embedded NUL, which is where the C's
/// `wcsstr`/`regexec` would stop.
fn hist_saved_line(el: &EditLine) -> &[u32] {
    let n = el.el_history.last.min(el.el_history.buf.len());
    let s = &el.el_history.buf[..n];
    match s.iter().position(|&c| c == 0) {
        Some(k) => &s[..k],
        None => s,
    }
}

/// The current line as the history search compares against it: the C's
/// `el->el_line.buffer` bounded by `lastchar - buffer`, which the caller has
/// just NUL-terminated.
fn line_prefix(el: &EditLine) -> &[u32] {
    let n = el.el_line.lastchar.min(el.el_line.buffer.len());
    &el.el_line.buffer[..n]
}

/// C: `c_hmatch(el, hp)` for a candidate the port holds as a slice.
///
/// `c_hmatch` takes the C's `const wchar_t *`, so the candidate is copied into
/// a NUL-terminated local for the call.
fn hmatch(el: &mut EditLine, s: &[u32]) -> bool {
    let mut z: Vec<u32> = Vec::with_capacity(s.len() + 1);
    z.extend_from_slice(s);
    z.push(0);
    // `z` is a NUL-terminated wide string that outlives the call, which is
    // `c_hmatch`'s contract for the pointer; it only reads through it.
    c_hmatch(el, z.as_ptr()) != 0
}

/// C: `wcsncmp(hp, el->el_line.buffer, lastchar - buffer) ||
/// hp[lastchar - buffer]` — the history search's "this candidate is not the
/// line already in the buffer" filter.
///
/// `line` is `buffer[..lastchar]`; `hp` is the candidate, already cut at its
/// own NUL. The C's index into `hp` is evaluated only when the prefixes
/// compare equal, which guarantees `hp` is at least that long, so "is strictly
/// longer" is the whole of that second test.
fn hist_entry_differs(hp: &[u32], line: &[u32]) -> bool {
    for (i, &b) in line.iter().enumerate() {
        let a = hp.get(i).copied().unwrap_or(0);
        if a != b {
            return true;
        }
        // `wcsncmp` stops at a NUL shared by both operands.
        if a == 0 {
            return false;
        }
    }
    hp.len() > line.len()
}

// [spec:libedit:def:common.ed-end-of-file-fn]
// [spec:libedit:sem:common.ed-end-of-file-fn]
/// C: `libedit_private el_action_t ed_end_of_file(EditLine *el, wint_t c)`
pub(crate) fn ed_end_of_file(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    re_goto_bottom(el);
    // Neither `lastchar` nor `cursor` moves; only the terminator is written.
    line_terminate(el);
    CC_EOF
}

// [spec:libedit:def:common.ed-insert-fn]
// [spec:libedit:sem:common.ed-insert-fn]
/// C: `libedit_private el_action_t ed_insert(EditLine *el, wint_t c)`
pub(crate) fn ed_insert(el: &mut EditLine, c: u32) -> ElActionT {
    // 1. The count is snapshotted on entry; the `count != 1` branch consumes
    //    the snapshot while still passing the *field* to `c_insert`.
    let mut count = el.el_state.argument;

    if c == 0 {
        return CC_ERROR;
    }

    // 2. `lastchar + argument >= limit`, in the C's pointer arithmetic. A
    //    negative argument moves the test backwards rather than wrapping,
    //    which is what the signed intermediate reproduces.
    let need = el.el_line.lastchar as isize + el.el_state.argument as isize;
    if need >= el.el_line.limit as isize {
        // C: `ch_enlargebufs(el, (size_t)count)` — the same sign extension a
        // negative count gets there, and the same allocation failure out of it.
        if ch_enlargebufs(el, count as usize) == 0 {
            return CC_ERROR;
        }
    }

    if count == 1 {
        // 3. Both replace modes overwrite here — but only with the cursor
        //    still inside the text. At end of line even a replace mode opens a
        //    slot and appends.
        if el.el_state.inputmode == MODE_INSERT || el.el_line.cursor >= el.el_line.lastchar {
            c_insert(el, 1);
        }
        let at = el.el_line.cursor;
        line_put(el, at, c);
        el.el_line.cursor += 1;
        re_fastaddc(el);
    } else {
        // 4. The asymmetry the rule calls out: with any count but 1, only
        //    `MODE_REPLACE_1` skips the insert, so an ordinary `MODE_REPLACE`
        //    with a count behaves as an *insert* rather than an overwrite.
        if el.el_state.inputmode != MODE_REPLACE_1 {
            c_insert(el, el.el_state.argument);
        }

        // C: `while (count-- && cursor < lastchar) *cursor++ = c`. The
        // post-decrement is what makes the body run exactly `count` times; the
        // `cursor < lastchar` guard is live only in `MODE_REPLACE_1`, where it
        // caps the writes at the characters actually remaining to the right,
        // and as the backstop for a `c_insert` that silently failed to grow
        // the buffer.
        loop {
            let more = count != 0;
            count = count.wrapping_sub(1);
            if !more || el.el_line.cursor >= el.el_line.lastchar {
                break;
            }
            let at = el.el_line.cursor;
            line_put(el, at, c);
            el.el_line.cursor += 1;
        }
        re_refresh(el);
    }

    // 5. `vi_command_mode` returns `CC_CURSOR`, and steps the cursor back one,
    //    which is what leaves vi's `r` sitting on the character it replaced.
    if el.el_state.inputmode == MODE_REPLACE_1 {
        return vi_command_mode(el, 0);
    }

    CC_NORM
}

// [spec:libedit:def:common.ed-delete-prev-word-fn]
// [spec:libedit:sem:common.ed-delete-prev-word-fn]
/// C: `libedit_private el_action_t ed_delete_prev_word(EditLine *el, wint_t c)`
pub(crate) fn ed_delete_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // ERR-modes-53: `ce_is_word` is the *emacs* word test, used here even when
    // the vi command map is active, so `^W` in vi has emacs word semantics.
    let cp = c_prev_word(el, el.el_line.cursor, 0, el.el_state.argument, ce_is_word);

    // ERR-modes-47, reproduced: this copy is observably redundant. The
    // `c_delbefore` below yanks the same range into the same buffer, because
    // ERR-modes-19 makes its `el_map.current != el_map.emacs` test a
    // tautology, so the kill buffer ends up with byte-identical content either
    // way.
    kill_save(el, cp, el.el_line.cursor);

    // ERR-modes-19: `c_delbefore` also takes a full-line `cv_undo` snapshot,
    // in emacs mode as much as in vi.
    let n = el.el_line.cursor.saturating_sub(cp) as i32;
    c_delbefore(el, n);
    el.el_line.cursor = cp;
    // The C's `if (cursor < buffer) cursor = buffer` bounds check is dead —
    // `c_prev_word` already clamps to `buffer` (ERR-modes-71) — and an
    // unsigned offset cannot express it at all.
    CC_REFRESH
}

// [spec:libedit:def:common.ed-delete-next-char-fn]
// [spec:libedit:sem:common.ed-delete-next-char-fn]
/// C: `libedit_private el_action_t ed_delete_next_char(EditLine *el, wint_t c)`
pub(crate) fn ed_delete_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // The `DEBUG_EDIT` `fprintf` at the top of the C is diagnostic only and is
    // not ported.

    // 1. The end-of-line guard, and only it, is mode-dependent. `KSHVI` is
    //    `#define`d unconditionally in `el.h` with no build switch, so the
    //    `#else` arm — write the character out with `terminal_writec` and
    //    return `CC_EOF` — is dead code and is deliberately not ported
    //    (ERR-modes-71).
    if el.el_line.cursor == el.el_line.lastchar {
        if el.el_map.r#type != MAP_VI {
            return CC_ERROR;
        }
        if el.el_line.cursor == 0 {
            return CC_ERROR;
        }
        // What makes vi `x` at end of line delete the last character rather
        // than failing.
        el.el_line.cursor -= 1;
    }

    // 2. `c_delafter` clamps to `lastchar - cursor`, so an oversized count
    //    deletes to end of line and never errors. ERR-modes-19: it also takes
    //    a `cv_undo` snapshot and writes the kill buffer through `cv_yank`, in
    //    emacs mode too.
    c_delafter(el, el.el_state.argument);

    // 3. Vi bounds fix-up. `cursor > buffer` with `cursor >= lastchar` implies
    //    `lastchar >= 1` — `c_delafter` never lowers `lastchar` below `cursor`
    //    — so the subtraction cannot go below zero.
    if el.el_map.r#type == MAP_VI
        && el.el_line.cursor >= el.el_line.lastchar
        && el.el_line.cursor > 0
    {
        el.el_line.cursor = el.el_line.lastchar.saturating_sub(1);
    }
    CC_REFRESH
}

// [spec:libedit:def:common.ed-kill-line-fn]
// [spec:libedit:sem:common.ed-kill-line-fn]
/// C: `libedit_private el_action_t ed_kill_line(EditLine *el, wint_t c)`
pub(crate) fn ed_kill_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. The kill buffer is not NUL-terminated; `c_kill.last` is the length.
    kill_save(el, el.el_line.cursor, el.el_line.lastchar);

    // 2. Truncate. The killed characters stay physically in the line buffer
    //    above the new `lastchar` — unobservable, but it is what leaves
    //    `ed_move_to_beg`'s vi scan stale text to walk over (ERR-modes-01).
    el.el_line.lastchar = el.el_line.cursor;

    // The cursor does not move, `argument` is ignored, and ERR-modes-51: no vi
    // undo snapshot is taken, so `u` cannot restore this.
    CC_REFRESH
}

// [spec:libedit:def:common.ed-move-to-end-fn]
// [spec:libedit:sem:common.ed-move-to-end-fn]
/// C: `libedit_private el_action_t ed_move_to_end(EditLine *el, wint_t c)`
pub(crate) fn ed_move_to_end(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_line.cursor = el.el_line.lastchar;
    if el.el_map.r#type != MAP_VI {
        return CC_CURSOR;
    }
    if el.el_chared.c_vcmd.action != NOP {
        // The operator's range reaches `lastchar`, because the back-step below
        // is skipped on this path.
        return end_motion(el);
    }
    // `VI_MOVE` is unconditionally defined in `chared.h`, so this is live: it
    // leaves the vi cursor *on* the last character.
    if el.el_line.cursor > 0 {
        el.el_line.cursor -= 1;
    }
    CC_CURSOR
}

// [spec:libedit:def:common.ed-move-to-beg-fn]
// [spec:libedit:sem:common.ed-move-to-beg-fn]
/// C: `libedit_private el_action_t ed_move_to_beg(EditLine *el, wint_t c)`
pub(crate) fn ed_move_to_beg(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_line.cursor = 0;

    if el.el_map.r#type == MAP_VI {
        // ERR-modes-01, defined here. The C is `while (iswspace(*cursor))
        // cursor++` with no upper bound at all: it stops only at the first
        // non-whitespace wide character it reads, so after `ed_kill_line` at
        // column zero it walks the stale text left above `lastchar` and parks
        // the cursor past the end of what the user sees, and on a line of pure
        // whitespace it runs to whatever follows in the allocation. The rule
        // directs the port to bound the scan at `lastchar`, which is what the
        // `cursor < lastchar` test does; on a whitespace-only line the cursor
        // now lands exactly on `lastchar` rather than beyond it.
        let cs = locale::charset();
        while el.el_line.cursor < el.el_line.lastchar
            && locale::iswspace(cs, line_get(el, el.el_line.cursor))
        {
            el.el_line.cursor += 1;
        }
    }
    end_vi_motion(el)
}

// [spec:libedit:def:common.ed-transpose-chars-fn]
// [spec:libedit:sem:common.ed-transpose-chars-fn]
/// C: `libedit_private el_action_t ed_transpose_chars(EditLine *el, wint_t c)`
///
/// The incoming `c` is ignored; the C reuses the parameter as the scratch
/// variable for the swap, which a local does here.
pub(crate) fn ed_transpose_chars(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. Step right first, so the command swaps the character *under* the
    //    cursor with the one before it.
    if el.el_line.cursor < el.el_line.lastchar {
        if el.el_line.lastchar <= 1 {
            return CC_ERROR;
        }
        el.el_line.cursor += 1;
    }

    // 2. Two characters must precede the cursor.
    let at = el.el_line.cursor;
    if at > 1 {
        // The cursor never exceeds `lastchar`, so the pair is in bounds; the
        // length test only keeps that reasoning from becoming a panic, and
        // does not move the branch the C takes.
        if at <= el.el_line.buffer.len() {
            el.el_line.buffer.swap(at - 2, at - 1);
        }
        CC_REFRESH
    } else {
        // ERR-modes-21, reproduced: step 1's `cursor++` is *not* undone here.
        // With the cursor at `buffer` on a line of two or more characters this
        // returns `CC_ERROR` with the cursor already one to the right, and the
        // dispatcher only beeps on `CC_ERROR` — it does not refresh — so the
        // internal and displayed cursors disagree until something forces a
        // redraw.
        CC_ERROR
    }
}

// [spec:libedit:def:common.ed-next-char-fn]
// [spec:libedit:sem:common.ed-next-char-fn]
/// C: `libedit_private el_action_t ed_next_char(EditLine *el, wint_t c)`
pub(crate) fn ed_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    let lim = el.el_line.lastchar;

    // 1. The second disjunct is only ever evaluated with `cursor < lim`, so
    //    `lim - 1` is in range; with an operator pending the move *is* allowed
    //    so the operator can consume the final character.
    if el.el_line.cursor >= lim
        || (el.el_line.cursor + 1 == lim
            && el.el_map.r#type == MAP_VI
            && el.el_chared.c_vcmd.action == NOP)
    {
        return CC_ERROR;
    }

    // 2-3. ERR-modes-50, reproduced: the clamp is to `lastchar`, not
    //      `lastchar - 1`, even in vi, so an overshooting count parks the vi
    //      cursor one past the last character — the position step 1 forbids.
    //      There is no clamp below in the C; a negative argument would form an
    //      out-of-range cursor and is not reachable through the key
    //      dispatcher, so it is defined here as a clamp to `buffer`.
    let moved = el.el_line.cursor as isize + el.el_state.argument as isize;
    el.el_line.cursor = moved.clamp(0, lim as isize) as usize;

    // 4.
    end_vi_motion(el)
}

// [spec:libedit:def:common.ed-prev-word-fn]
// [spec:libedit:sem:common.ed-prev-word-fn]
/// C: `libedit_private el_action_t ed_prev_word(EditLine *el, wint_t c)`
pub(crate) fn ed_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // ERR-modes-53: the emacs word test again, so vi `b` does not split on
    // punctuation the way real vi does.
    el.el_line.cursor = c_prev_word(el, el.el_line.cursor, 0, el.el_state.argument, ce_is_word);

    end_vi_motion(el)
}

// [spec:libedit:def:common.ed-prev-char-fn]
// [spec:libedit:sem:common.ed-prev-char-fn]
/// C: `libedit_private el_action_t ed_prev_char(EditLine *el, wint_t c)`
pub(crate) fn ed_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // The clamp below is the C's; there is no clamp above, so a negative
    // argument would move the cursor right unchecked and past `lastchar`.
    // Not reachable through the key dispatcher, and defined here as a clamp to
    // `lastchar` — a no-op for every reachable argument, since a non-negative
    // one only ever moves the cursor left.
    let moved = el.el_line.cursor as isize - el.el_state.argument as isize;
    el.el_line.cursor = moved.clamp(0, el.el_line.lastchar as isize) as usize;

    // Unlike `ed_next_char` there is no vi "must stay on a character" guard:
    // moving left onto `buffer` is always legal.
    end_vi_motion(el)
}

// [spec:libedit:def:common.ed-quoted-insert-fn]
// [spec:libedit:sem:common.ed-quoted-insert-fn]
/// C: `libedit_private el_action_t ed_quoted_insert(EditLine *el, wint_t c)`
pub(crate) fn ed_quoted_insert(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // Both tty results are discarded, as in the C: a failure to switch the
    // special characters off is not reported and the read proceeds anyway.
    let _ = tty_quotemode(el);
    let mut ch: u32 = 0;
    let num = el_wgetc(el, &mut ch);
    // Restored before the branch, so on the failure path too.
    let _ = tty_noquotemode(el);

    if num == 1 {
        // The current `argument` is the repeat count, so `ESC 4 ^V x` inserts
        // four `x`s.
        ed_insert(el, ch)
    } else {
        // "Could not set the tty raw" and "read error" are both reported as
        // end of file; the distinction is lost.
        ed_end_of_file(el, 0)
    }
}

// [spec:libedit:def:common.ed-digit-fn]
// [spec:libedit:sem:common.ed-digit-fn]
/// C: `libedit_private el_action_t ed_digit(EditLine *el, wint_t c)`
pub(crate) fn ed_digit(el: &mut EditLine, c: u32) -> ElActionT {
    if !iswdigit(c) {
        return CC_ERROR;
    }

    if el.el_state.doingarg != 0 {
        let digit = c as i32 - i32::from(b'0');
        if el.el_state.lastcmd == EM_UNIVERSAL_ARGUMENT {
            // *Replaces* the universal argument rather than appending to it,
            // so `^U 5` means 5. Only the immediately preceding `^U` does
            // this: a second digit sees `lastcmd == ED_DIGIT` and appends.
            el.el_state.argument = digit;
        } else {
            // ERR-modes-49, which is also why the multiply and add below
            // cannot overflow.
            if el.el_state.argument > ARGUMENT_CAP {
                return CC_ERROR;
            }
            el.el_state.argument = el.el_state.argument * 10 + digit;
        }
        return CC_ARGHACK;
    }

    // No count in progress: the digit is ordinary text, inserted with the
    // current `argument` as its repeat count.
    ed_insert(el, c)
}

// [spec:libedit:def:common.ed-argument-digit-fn]
// [spec:libedit:sem:common.ed-argument-digit-fn]
/// C: `libedit_private el_action_t ed_argument_digit(EditLine *el, wint_t c)`
pub(crate) fn ed_argument_digit(el: &mut EditLine, c: u32) -> ElActionT {
    if !iswdigit(c) {
        return CC_ERROR;
    }

    let digit = c as i32 - i32::from(b'0');
    if el.el_state.doingarg != 0 {
        // ERR-modes-49 again, and no `em_universal_argument` special case:
        // this one never falls through to `ed_insert` either.
        if el.el_state.argument > ARGUMENT_CAP {
            return CC_ERROR;
        }
        el.el_state.argument = el.el_state.argument * 10 + digit;
    } else {
        // *Replaces* whatever `argument` held rather than multiplying, so a
        // leading `0` sets it to 0.
        el.el_state.argument = digit;
        el.el_state.doingarg = 1;
    }
    CC_ARGHACK
}

// [spec:libedit:def:common.ed-unassigned-fn]
// [spec:libedit:sem:common.ed-unassigned-fn]
/// C: `libedit_private el_action_t ed_unassigned(EditLine *el, wint_t c)`
///
/// The entry every unbound key holds. `CC_ERROR` is what makes the dispatcher
/// beep; the only difference from [`ed_ignore`] is that return value.
pub(crate) fn ed_unassigned(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = (el, c);
    CC_ERROR
}

// [spec:libedit:def:common.ed-ignore-fn]
// [spec:libedit:sem:common.ed-ignore-fn]
/// C: `libedit_private el_action_t ed_ignore(EditLine *el, wint_t c)`
///
/// The binding for keys that must be swallowed silently rather than rejected.
pub(crate) fn ed_ignore(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = (el, c);
    CC_NORM
}

// [spec:libedit:def:common.ed-newline-fn]
// [spec:libedit:sem:common.ed-newline-fn]
/// C: `libedit_private el_action_t ed_newline(EditLine *el, wint_t c)`
pub(crate) fn ed_newline(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    re_goto_bottom(el);
    // The returned line *includes* this newline. Both stores are unchecked in
    // the C and need no check: `limit` sits two slots below the end of the
    // allocation precisely to leave room for them.
    let at = el.el_line.lastchar;
    line_put(el, at, u32::from(b'\n'));
    el.el_line.lastchar = at + 1;
    line_terminate(el);
    CC_NEWLINE
}

// [spec:libedit:def:common.ed-delete-prev-char-fn]
// [spec:libedit:sem:common.ed-delete-prev-char-fn]
/// C: `libedit_private el_action_t ed_delete_prev_char(EditLine *el, wint_t c)`
pub(crate) fn ed_delete_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // ERR-modes-19: `c_delbefore` clamps the count to `cursor - buffer`, and
    // its always-true emacs test means the `cv_undo` snapshot and the
    // `cv_yank` into the kill buffer happen in emacs mode as well as vi. It
    // deliberately does not move the cursor.
    c_delbefore(el, el.el_state.argument);

    // The raw, unclamped argument, then the clamp. The two clamps agree: when
    // the argument overshoots, `c_delbefore` deletes exactly `cursor - buffer`
    // characters and the cursor lands on `buffer`, where the surviving text
    // now starts.
    let moved = el.el_line.cursor as isize - el.el_state.argument as isize;
    el.el_line.cursor = moved.max(0) as usize;
    CC_REFRESH
}

// [spec:libedit:def:common.ed-clear-screen-fn]
// [spec:libedit:sem:common.ed-clear-screen-fn]
/// C: `libedit_private el_action_t ed_clear_screen(EditLine *el, wint_t c)`
pub(crate) fn ed_clear_screen(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    terminal_clear_screen(el);
    // Forget the on-screen image, so the refresh `CC_REFRESH` triggers redraws
    // everything rather than diffing against a stale one.
    re_clear_display(el);
    CC_REFRESH
}

// [spec:libedit:def:common.ed-redisplay-fn]
// [spec:libedit:sem:common.ed-redisplay-fn]
/// C: `libedit_private el_action_t ed_redisplay(EditLine *el, wint_t c)`
///
/// All the work is in the return value: `CC_REDISPLAY` makes the dispatcher
/// erase the lines this input occupies, forget the on-screen image and then
/// repaint, leaving the scrollback above it alone.
pub(crate) fn ed_redisplay(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = (el, c);
    CC_REDISPLAY
}

// [spec:libedit:def:common.ed-start-over-fn]
// [spec:libedit:sem:common.ed-start-over-fn]
/// C: `libedit_private el_action_t ed_start_over(EditLine *el, wint_t c)`
///
/// `ch_reset` drops vi command mode, discards the history position and leaves
/// the kill buffer's *contents* alone — only `c_kill.mark` is reset — so a
/// yank after `^G` still pastes the pre-`^G` kill.
pub(crate) fn ed_start_over(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    ch_reset(el);
    CC_REFRESH
}

// [spec:libedit:def:common.ed-sequence-lead-in-fn]
// [spec:libedit:sem:common.ed-sequence-lead-in-fn]
/// C: `libedit_private el_action_t ed_sequence_lead_in(EditLine *el, wint_t c)`
///
/// The placeholder bound to the first character of a multi-character key
/// sequence. Reaching it means the sequence did not complete, and the right
/// response is to swallow the character silently. Behaviourally identical to
/// [`ed_ignore`]; the two are separate table entries so `bind` can tell them
/// apart.
pub(crate) fn ed_sequence_lead_in(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = (el, c);
    CC_NORM
}

// [spec:libedit:def:common.ed-prev-history-fn]
// [spec:libedit:sem:common.ed-prev-history-fn]
/// C: `libedit_private el_action_t ed_prev_history(EditLine *el, wint_t c)`
pub(crate) fn ed_prev_history(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    let mut beep = false;
    let sv_event = el.el_history.eventno;

    // History motion is not undoable.
    el.el_chared.c_undo.len = -1;
    line_terminate(el);

    // Leaving the live line: stash it.
    if el.el_history.eventno == 0 {
        hist_save_line(el);
    }
    el.el_history.eventno += el.el_state.argument;

    if hist_get(el) == CC_ERROR {
        // The mode dependency the rule calls out: vi throws away `hist_get`'s
        // clamp and stays where it was, emacs keeps it and lands on the oldest
        // entry. Combined with ERR-history-27 — `hist_get` returning early on
        // an empty history without resetting `eventno` — emacs is then left
        // believing it is on a phantom event 1.
        if el.el_map.r#type == MAP_VI {
            el.el_history.eventno = sv_event;
        }
        beep = true;
        // ERR-history-31, reproduced: this second result is discarded. When it
        // also fails — `el_history.ref` being NULL makes both calls fail
        // identically — the line is left untouched, `eventno` keeps the bumped
        // value in emacs mode, and this still reports success-with-beep.
        let _ = hist_get(el);
    }

    // There is no `CC_ERROR` path out of this function at all.
    if beep { CC_REFRESH_BEEP } else { CC_REFRESH }
}

// [spec:libedit:def:common.ed-next-history-fn]
// [spec:libedit:sem:common.ed-next-history-fn]
/// C: `libedit_private el_action_t ed_next_history(EditLine *el, wint_t c)`
pub(crate) fn ed_next_history(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    let mut beep = CC_REFRESH;

    el.el_chared.c_undo.len = -1;
    line_terminate(el);

    el.el_history.eventno -= el.el_state.argument;

    // Walking forward past the newest entry lands on the saved live line and
    // beeps rather than erroring. ERR-history-32: this function never *saves*
    // the live line — only `ed_prev_history` and `ed_search_prev_history` do —
    // so a `^N` with no prior `^P` reloads the empty stash and silently wipes
    // what the user was typing.
    if el.el_history.eventno < 0 {
        el.el_history.eventno = 0;
        beep = CC_REFRESH_BEEP;
    }
    let rval = hist_get(el);
    if rval == CC_REFRESH {
        return beep;
    }
    // In practice `CC_ERROR`, which also discards the pending beep.
    rval
}

// [spec:libedit:def:common.ed-search-prev-history-fn]
// [spec:libedit:sem:common.ed-search-prev-history-fn]
/// C: `libedit_private el_action_t ed_search_prev_history(EditLine *el,
/// wint_t c)`
pub(crate) fn ed_search_prev_history(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // The `DEBUG_EDIT` and `SDEBUG` traces are compiled out and are not ported.

    let mut found = false;

    // 1-3. Drop any pending vi operator, invalidate the undo snapshot, and
    //      terminate the line so it can serve as both pattern source and
    //      comparison string.
    el.el_chared.c_vcmd.action = NOP;
    el.el_chared.c_undo.len = -1;
    line_terminate(el);

    // 4. Should never happen.
    if el.el_history.eventno < 0 {
        el.el_history.eventno = 0;
        return CC_ERROR;
    }
    // 5.
    if el.el_history.eventno == 0 {
        hist_save_line(el);
    }
    // 6.
    if !el.el_history.src.is_attached() {
        return CC_ERROR;
    }
    // 7. Position on event 1, the newest entry.
    let Some(first) = hist_first(el) else {
        return CC_ERROR;
    };

    // 8. Skipped when the previous command was itself a history search, so a
    //    run of searches reuses the first one's pattern.
    c_setpat(el);

    // 9. Skip past everything at or newer than the current position. There is
    //    no NULL check inside this loop in the C: if `eventno` exceeds the
    //    history length `HIST_NEXT` keeps returning NULL and step 10 handles
    //    it. `h` ends at `eventno + 1`, naming the entry `hp` now holds.
    let mut hp = Some(first);
    let mut h: i32 = 1;
    while h <= el.el_history.eventno {
        hp = hist_next(el);
        h += 1;
    }

    // 10. The first match wins — this direction breaks.
    while let Some(entry) = hp {
        let differs = hist_entry_differs(&entry, line_prefix(el));
        if differs && hmatch(el, &entry) {
            found = true;
            break;
        }
        h += 1;
        hp = hist_next(el);
    }

    // 11.
    if !found {
        return CC_ERROR;
    }
    // 12.
    el.el_history.eventno = h;
    hist_get(el)
}

// [spec:libedit:def:common.ed-search-next-history-fn]
// [spec:libedit:sem:common.ed-search-next-history-fn]
/// C: `libedit_private el_action_t ed_search_next_history(EditLine *el,
/// wint_t c)`
pub(crate) fn ed_search_next_history(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // The `SDEBUG` traces are compiled out and are not ported.

    // `found` is both the flag and the event number, and 0 is also the
    // live-line event number — the "no match but the live line matches" case
    // and "matched event 0" are deliberately the same path, since a real match
    // always has `h >= 1`.
    let mut found: i32 = 0;

    // 1-3.
    el.el_chared.c_vcmd.action = NOP;
    el.el_chared.c_undo.len = -1;
    line_terminate(el);

    // 4. Already on the live line: nothing newer to find.
    if el.el_history.eventno == 0 {
        return CC_ERROR;
    }
    // 5.
    if !el.el_history.src.is_attached() {
        return CC_ERROR;
    }
    // 6.
    let Some(first) = hist_first(el) else {
        return CC_ERROR;
    };

    // 7.
    c_setpat(el);

    // 8. Walk events 1 .. `eventno - 1`, everything newer than the current
    //    position. There is **no break**: `found` ends up holding the largest
    //    matching `h`, which is the match nearest the current position and so
    //    the correct "next" one.
    let mut hp = Some(first);
    let mut h: i32 = 1;
    while h < el.el_history.eventno {
        let Some(entry) = hp else {
            break;
        };
        let differs = hist_entry_differs(&entry, line_prefix(el));
        if differs && hmatch(el, &entry) {
            found = h;
        }
        hp = hist_next(el);
        h += 1;
    }

    // 9. Fall back to the saved live line. ERR-history-10: the C runs
    //    `wcsstr`/`regexec` over `el_history.buf`, which `wcsncpy` may have
    //    left unterminated; [`hist_saved_line`] uses the recorded length
    //    instead.
    if found == 0 {
        let saved = hist_saved_line(el).to_vec();
        if !hmatch(el, &saved) {
            return CC_ERROR;
        }
    }

    // 10. `found == 0` selects the live line, restoring the stash.
    el.el_history.eventno = found;
    hist_get(el)
}

// [spec:libedit:def:common.ed-prev-line-fn]
// [spec:libedit:sem:common.ed-prev-line-fn]
/// C: `libedit_private el_action_t ed_prev_line(EditLine *el, wint_t c)`
///
/// Moves up one *embedded* line, across a literal `'\n'` inside the edit
/// buffer. Not bound by default in either map.
pub(crate) fn ed_prev_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. The current column.
    let mut nchars = c_hpos(el);

    // ERR-modes-02, defined here. With a non-positive `argument` and no `'\n'`
    // at or before the cursor, the C's step-3 scan runs off the front leaving
    // `ptr == buffer - 1` while `argument == 0`, so the step-4 guard does not
    // fire; step 5 then reaches `buffer - 2` and step 6's `ptr++` makes it
    // `buffer - 1`, which the loop condition immediately dereferences — an
    // out-of-bounds read below the line buffer, after which the cursor may be
    // set there. The rule directs the port to treat a non-positive argument as
    // producing no movement, so it is rejected here, before the scan, and
    // `argument` is left alone rather than partially decremented. Reachable
    // only by binding `ed-prev-line` and prefixing it with `ESC 0`.
    if el.el_state.argument <= 0 {
        return CC_CURSOR;
    }

    // 2. Keep a cursor sitting *on* a newline from counting that newline as
    //    the boundary it is looking for. ERR-modes-03: with
    //    `cursor == lastchar` this reads the slot at `lastchar`, which is in
    //    bounds — `limit` leaves two spare slots — but holds whatever earlier
    //    editing left there unless something wrote a terminator. That is the
    //    same determinate-but-stale value the C reads, so it is defined here
    //    as reading it.
    let mut ptr = el.el_line.cursor as isize;
    if line_get(el, el.el_line.cursor) == u32::from(b'\n') {
        ptr -= 1;
    }

    // 3. Scan backwards for the `argument`-th newline, decrementing
    //    `el_state.argument` in place once per newline crossed, exactly as the
    //    C does.
    while ptr >= 0 {
        if line_get(el, ptr as usize) == u32::from(b'\n') {
            el.el_state.argument -= 1;
            if el.el_state.argument <= 0 {
                break;
            }
        }
        ptr -= 1;
    }

    // 4. Not enough newlines above the cursor. `argument` is left partially
    //    decremented, which is harmless: the dispatcher resets it after the
    //    command. A single-line buffer with the default argument of 1 always
    //    takes this path.
    if el.el_state.argument > 0 {
        return CC_ERROR;
    }

    // 5. Back to the start of the preceding line; `ptr` ends on the newline
    //    before the target line, or at `buffer - 1`.
    ptr -= 1;
    while ptr >= 0 && line_get(el, ptr as usize) != u32::from(b'\n') {
        ptr -= 1;
    }

    // 6. Forward to the target column, or that line's end if it is shorter.
    ptr += 1;
    while nchars > 0
        && (ptr as usize) < el.el_line.lastchar
        && line_get(el, ptr as usize) != u32::from(b'\n')
    {
        nchars -= 1;
        ptr += 1;
    }

    // 7.
    el.el_line.cursor = ptr as usize;
    CC_CURSOR
}

// [spec:libedit:def:common.ed-next-line-fn]
// [spec:libedit:sem:common.ed-next-line-fn]
/// C: `libedit_private el_action_t ed_next_line(EditLine *el, wint_t c)`
///
/// Moves down one *embedded* line. Unlike [`ed_prev_line`] there is no
/// adjustment for a cursor already sitting on a `'\n'`: that newline counts as
/// the first one crossed, so this at end of line moves down one line rather
/// than two.
pub(crate) fn ed_next_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1.
    let mut nchars = c_hpos(el);

    // ERR-modes-20, defined here. With a non-positive `argument` and no
    // newline between the cursor and `lastchar`, the C's scan exits with
    // `ptr == lastchar` and `argument == 0`, the guard does not fire, and step
    // 4's `ptr++` leaves `cursor == lastchar + 1` — inside the allocation, but
    // an invalid cursor. Defined as producing no movement, per the rule, with
    // `argument` left alone. Reachable only by binding `ed-next-line` and
    // prefixing it with `ESC 0`.
    if el.el_state.argument <= 0 {
        return CC_CURSOR;
    }

    // 2. Forward to the `argument`-th newline, decrementing
    //    `el_state.argument` in place once per newline crossed.
    let mut ptr = el.el_line.cursor;
    while ptr < el.el_line.lastchar {
        if line_get(el, ptr) == u32::from(b'\n') {
            el.el_state.argument -= 1;
            if el.el_state.argument <= 0 {
                break;
            }
        }
        ptr += 1;
    }

    // 3. Not enough newlines below the cursor; the cursor is unchanged.
    if el.el_state.argument > 0 {
        return CC_ERROR;
    }

    // 4. Past the newline onto the next line, then to the target column — or
    //    that line's end if it is shorter. `ptr` is a newline index below
    //    `lastchar` here, so the step past it stays in bounds.
    ptr += 1;
    while nchars > 0 && ptr < el.el_line.lastchar && line_get(el, ptr) != u32::from(b'\n') {
        nchars -= 1;
        ptr += 1;
    }

    // 5.
    el.el_line.cursor = ptr;
    CC_CURSOR
}

// [spec:libedit:def:common.ed-command-fn]
// [spec:libedit:sem:common.ed-command-fn]
/// C: `libedit_private el_action_t ed_command(EditLine *el, wint_t c)`
///
/// Prompts for and executes one editline built-in command line — the same
/// language as `.editrc`.
pub(crate) fn ed_command(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // C: `wchar_t tmpbuf[EL_BUFSIZ]`, uninitialised. Zeroed here; every slot
    // the C reads is one `c_gets` wrote or the terminator written below.
    let mut tmpbuf = [0u32; EL_BUFSIZ];

    // 1. `c_gets` takes over the edit line to read the command, and **clears
    //    it** before returning, so invoking this destroys whatever the user
    //    was editing whether or not the command line parses. ERR-buffer-10 is
    //    a property of `c_gets` and this prompt is 3 characters, well inside
    //    its bound.
    let prompt = [u32::from(b'\n'), u32::from(b':'), u32::from(b' ')];
    let tmplen = c_gets(el, &mut tmpbuf, Some(&prompt));

    // 2. So the executed command does not sit on the prompt line.
    let _ = terminal_putc(el, u32::from(b'\n'));

    // 3. The C's `tmpbuf[tmplen] = 0` sits in a comma expression on the
    //    right-hand side of `||`, so it runs only when `tmplen >= 0`; an empty
    //    line does get terminated, at index 0. `c_gets` caps the length well
    //    below `EL_BUFSIZ`, so the store is in range.
    //
    //    Only `-1` beeps. `parse_line` returns the *negation* of the built-in's
    //    own result, so a built-in that fails with -1 yields +1 and does not
    //    beep: unrecognised commands beep, failing commands generally do not.
    if tmplen < 0 {
        terminal_beep(el);
    } else {
        if let Some(slot) = tmpbuf.get_mut(tmplen as usize) {
            *slot = 0;
        }
        if parse_line(el, &tmpbuf) == -1 {
            terminal_beep(el);
        }
    }

    // 4. Force the primary keymap back on — in vi this is what leaves command
    //    mode after `:`.
    el.el_map.current = ElMapCurrent::Key;
    // 5. The command's own output may have scrolled the screen.
    re_clear_display(el);
    // `CC_REFRESH` on every branch; there is no error return path.
    CC_REFRESH
}

#[cfg(test)]
mod test;
