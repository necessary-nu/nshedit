//! Ported from `src/emacs.c`; rules live in `docs/spec/port/src/emacs.md`.

use crate::chared::{
    MODE_INSERT, MODE_REPLACE, c_delafter, c_delafter1, c_delbefore, c_delbefore1, c_insert,
    c_next_word, c_prev_word, ce_is_word,
};
use crate::common::{ARGUMENT_CAP, kill_save};
use crate::el::{EditLine, ElActionT};
use crate::fcns::{ED_SEARCH_NEXT_HISTORY, ED_SEARCH_PREV_HISTORY};
use crate::histedit::{CC_ARGHACK, CC_CURSOR, CC_EOF, CC_ERROR, CC_NORM, CC_REFRESH};
use crate::locale::{self, iswalpha, iswlower, iswupper, towlower, towupper};
use crate::search::ce_inc_search;
use crate::terminal::{terminal_beep, terminal_writec};
use crate::vi::end_vi_motion;

// [spec:libedit:def:emacs.em-delete-or-list-fn]
// [spec:libedit:sem:emacs.em-delete-or-list-fn]
/// Delete character under cursor or list completions if at end of line
/// `[^D]`.
pub(crate) fn em_delete_or_list(el: &mut EditLine, c: u32) -> ElActionT {
    if el.el_line.cursor == el.el_line.lastchar {
        // At the end.
        if el.el_line.cursor == 0 {
            // ... and at the beginning: the line is empty, so this is EOF.
            // The invoking key is echoed through `ct_visual_char`, which
            // renders the default binding as `^D`.
            terminal_writec(el, c);
            CC_EOF
        } else {
            // The listing behaviour the function's own comment header
            // promises is not implemented (ERR-modes-71). ERR-modes-52: the
            // read loop beeps again for `CC_ERROR`, so this path beeps
            // twice.
            terminal_beep(el);
            CC_ERROR
        }
    } else {
        if el.el_state.doingarg != 0 {
            // Clamps to `lastchar - cursor`, so an over-large count deletes
            // to end of line rather than erroring. ERR-modes-19: the
            // `current != emacs` test inside is a tautology, so this also
            // takes a vi undo snapshot and rewrites the kill buffer with the
            // doomed text — which contradicts this rule's closing claim that
            // nothing is ever written to the kill buffer here.
            c_delafter(el, el.el_state.argument);
        } else {
            // Exactly one character, no clamping and no vi bookkeeping.
            c_delafter1(el);
        }
        if el.el_line.cursor > el.el_line.lastchar {
            el.el_line.cursor = el.el_line.lastchar; // bounds check
        }
        CC_REFRESH
    }
}

// [spec:libedit:def:emacs.em-delete-next-word-fn]
// [spec:libedit:sem:emacs.em-delete-next-word-fn]
/// Cut from cursor to end of current word `[M-d]`.
pub(crate) fn em_delete_next_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    if el.el_line.cursor == el.el_line.lastchar {
        return CC_ERROR;
    }

    let cp = c_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce_is_word,
    );

    // Save the text. Overwrites whatever the kill buffer held; see
    // [`kill_save`].
    kill_save(el, el.el_line.cursor, cp);

    // Delete after dot. ERR-modes-19 makes `c_delafter`'s `cv_yank` run
    // here too, which writes the same span over the save just made — the
    // net kill-buffer contents are unchanged.
    c_delafter(el, (cp - el.el_line.cursor) as i32);
    if el.el_line.cursor > el.el_line.lastchar {
        el.el_line.cursor = el.el_line.lastchar; // bounds check
    }
    // The mark is deliberately not adjusted for the text that just moved
    // (ERR-modes-23).
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-yank-fn]
// [spec:libedit:sem:emacs.em-yank-fn]
/// Paste cut buffer at cursor position `[^Y]`.
pub(crate) fn em_yank(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // `last == buf`: the kill buffer is empty. Silent — no beep, no redraw.
    if el.el_chared.c_kill.last == 0 {
        return CC_NORM;
    }

    let n = el.el_chared.c_kill.last;
    // `>=`, so a yank landing exactly on `limit` is refused. The line is
    // NOT grown to make room; this is a hard refusal.
    if el.el_line.lastchar + n >= el.el_line.limit {
        return CC_ERROR;
    }

    // Before any text moves, so the mark ends up at the START of the yanked
    // text. Any previous mark is destroyed.
    el.el_chared.c_kill.mark = el.el_line.cursor;

    // Open the space. `c_insert`'s `ch_enlargebufs` path is unreachable from
    // here: the test above was the identical one.
    c_insert(el, n as i32);
    let mut cp = el.el_line.cursor;
    // Copy the chars. The kill buffer itself is untouched, so repeated
    // yanks paste the same text.
    for kp in 0..n {
        let ch = el.el_chared.c_kill.buf[kp];
        el.el_line.buffer[cp] = ch;
        cp += 1;
    }

    // ERR-modes-55: the repeat count chooses only where the cursor lands,
    // never how many copies are pasted. If an arg was given, cursor at the
    // beginning (i.e. left where it was); else cursor at the end.
    if el.el_state.argument == 1 {
        el.el_line.cursor = cp;
    }

    CC_REFRESH
}

// [spec:libedit:def:emacs.em-kill-line-fn]
// [spec:libedit:sem:emacs.em-kill-line-fn]
/// Cut the entire line and save in cut buffer `[^U]`.
///
/// ERR-modes-57: the whole line, both sides of the cursor — GNU emacs binds
/// `^U` to `universal-argument` and kills only forwards from point.
#[doc(hidden)]
pub fn em_kill_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    kill_save(el, 0, el.el_line.lastchar);
    // Zap! -- delete all of it, by pointer assignment only. No chared.c
    // helper is used, so ERR-modes-51: no vi undo state is recorded and no
    // vi yank buffer is filled even under a vi keymap. The old characters
    // stay in the buffer storage above the new `lastchar`.
    el.el_line.lastchar = 0;
    el.el_line.cursor = 0;
    // ERR-modes-23: `c_kill.mark` is deliberately NOT reset, so a mark above
    // the head of the line is left above the new `lastchar`.
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-kill-region-fn]
// [spec:libedit:sem:emacs.em-kill-region-fn]
/// Cut area between mark and cursor and save in cut buffer `[^W]`.
///
/// Not bound in the default emacs keymap: `^W` there is
/// `ED_DELETE_PREV_WORD`.
pub(crate) fn em_kill_region(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // The C's `if (!el->el_chared.c_kill.mark) return CC_ERROR` is dead code
    // (ERR-modes-71) and is not ported: `ch_init` and `ch_reset` point the
    // mark at the head of the line and nothing ever unsets it, so the mark
    // is modelled as always set and an "unset" mark behaves as a mark at
    // index 0.
    let mark = el.el_chared.c_kill.mark;

    if mark > el.el_line.cursor {
        // Region to the right of the cursor. Note `mark` may sit above
        // `lastchar`; the save reads that far, and `c_delafter` then clamps.
        kill_save(el, el.el_line.cursor, mark);
        c_delafter(el, (mark - el.el_line.cursor) as i32);
        // The cursor is NOT moved: it now sits at the start of the text that
        // followed the region.
    } else {
        // Mark is before cursor. `mark == cursor` lands here: nothing is
        // copied, so the kill buffer is EMPTIED, `c_delbefore(el, 0)`
        // deletes nothing, the cursor does not move, and `CC_REFRESH` is
        // still returned.
        kill_save(el, mark, el.el_line.cursor);
        c_delbefore(el, (el.el_line.cursor - mark) as i32);
        el.el_line.cursor = mark;
    }
    // The mark keeps its old value in both branches (ERR-modes-23).
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-copy-region-fn]
// [spec:libedit:sem:emacs.em-copy-region-fn]
/// Copy area between mark and cursor to cut buffer `[M-W]`.
pub(crate) fn em_copy_region(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // As in [`em_kill_region`], the NULL-mark guard is dead code
    // (ERR-modes-71) and is not ported.
    let mark = el.el_chared.c_kill.mark;

    if mark > el.el_line.cursor {
        kill_save(el, el.el_line.cursor, mark);
    } else {
        kill_save(el, mark, el.el_line.cursor);
    }
    // `CC_NORM`: no redraw, no reposition, no beep, no message. The user
    // gets no feedback that anything happened. Nothing else is modified.
    CC_NORM
}

// [spec:libedit:def:emacs.em-gosmacs-transpose-fn]
// [spec:libedit:sem:emacs.em-gosmacs-transpose-fn]
/// Exchange the two characters before the cursor — Gosling emacs transpose
/// chars. NOT bound in the default emacs keymap, where `^T` is
/// `ED_TRANSPOSE_CHARS`.
pub(crate) fn em_gosmacs_transpose(el: &mut EditLine, c: u32) -> ElActionT {
    // The C declares `c` as the invoking key but uses it purely as the swap
    // temporary, discarding the incoming value.
    let _ = c;

    if el.el_line.cursor > 1 {
        // Must have at least two chars entered.
        let cursor = el.el_line.cursor;
        el.el_line.buffer.swap(cursor - 2, cursor - 1);
        // ERR-modes-56: point does NOT advance, unlike GNU emacs
        // `transpose-chars`, and `el_state.argument` is ignored entirely.
        CC_REFRESH
    } else {
        CC_ERROR
    }
}

// [spec:libedit:def:emacs.em-next-word-fn]
// [spec:libedit:sem:emacs.em-next-word-fn]
/// Move next to end of current word `[M-f]`.
pub(crate) fn em_next_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    if el.el_line.cursor == el.el_line.lastchar {
        return CC_ERROR;
    }

    el.el_line.cursor = c_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce_is_word,
    );

    // The test is on the keymap *type*, not on which map is currently
    // active, so this emacs function completes a pending vi operator
    // whenever the editor has been put in vi mode.
    end_vi_motion(el)
}

// [spec:libedit:def:emacs.em-upper-case-fn]
// [spec:libedit:sem:emacs.em-upper-case-fn]
/// Uppercase the characters from cursor to end of current word `[M-u]`.
pub(crate) fn em_upper_case(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    let cs = locale::charset();
    let ep = c_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce_is_word,
    );

    let mut cp = el.el_line.cursor;
    while cp < ep {
        let ch = el.el_line.buffer[cp];
        if iswlower(cs, ch) {
            el.el_line.buffer[cp] = towupper(cs, ch);
        }
        cp += 1;
    }

    // The C's following `if (cursor > lastchar) cursor = lastchar` is dead
    // code (ERR-modes-71) — `c_next_word` has already clamped `ep` — and is
    // not ported. No error return: an empty span still refreshes.
    el.el_line.cursor = ep;
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-capitol-case-fn]
// [spec:libedit:sem:emacs.em-capitol-case-fn]
/// Capitalize the characters from cursor to end of current word `[M-c]`.
pub(crate) fn em_capitol_case(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    let cs = locale::charset();
    let ep = c_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce_is_word,
    );

    // First pass: skip to the first alphabetic character, upcase it if it is
    // lowercase, step one past it and stop. A span with no alphabetic
    // character leaves `cp` at `ep`.
    let mut cp = el.el_line.cursor;
    while cp < ep {
        let ch = el.el_line.buffer[cp];
        if iswalpha(cs, ch) {
            if iswlower(cs, ch) {
                el.el_line.buffer[cp] = towupper(cs, ch);
            }
            cp += 1;
            break;
        }
        cp += 1;
    }
    // Second pass: downcase everything remaining.
    while cp < ep {
        let ch = el.el_line.buffer[cp];
        if iswupper(cs, ch) {
            el.el_line.buffer[cp] = towlower(cs, ch);
        }
        cp += 1;
    }

    // The redundant re-clamp is dead code (ERR-modes-71) and is not ported.
    el.el_line.cursor = ep;
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-lower-case-fn]
// [spec:libedit:sem:emacs.em-lower-case-fn]
/// Lowercase the characters from cursor to end of current word `[M-l]`.
pub(crate) fn em_lower_case(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    let cs = locale::charset();
    let ep = c_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce_is_word,
    );

    let mut cp = el.el_line.cursor;
    while cp < ep {
        let ch = el.el_line.buffer[cp];
        if iswupper(cs, ch) {
            el.el_line.buffer[cp] = towlower(cs, ch);
        }
        cp += 1;
    }

    // The redundant re-clamp is dead code (ERR-modes-71) and is not ported.
    el.el_line.cursor = ep;
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-set-mark-fn]
// [spec:libedit:sem:emacs.em-set-mark-fn]
/// Set the mark at cursor `[^@]`.
pub(crate) fn em_set_mark(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // There is no mark ring: the single slot is overwritten. `CC_NORM`, so
    // no redraw and no "Mark set" message.
    el.el_chared.c_kill.mark = el.el_line.cursor;
    CC_NORM
}

// [spec:libedit:def:emacs.em-exchange-mark-fn]
// [spec:libedit:sem:emacs.em-exchange-mark-fn]
/// Exchange the cursor and mark `[^X^X]` — registered by `map_init_emacs` as
/// a two-key macro rather than as a keymap entry.
pub(crate) fn em_exchange_mark(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // Unconditional and unvalidated: no NULL test, no clamp to
    // `[buffer, lastchar]`, no error return. ERR-modes-23: the mark is never
    // adjusted when the line is edited and `em_kill_line` can leave it above
    // `lastchar`, so this can strand the cursor beyond the live line. That
    // is the hazard to preserve — no clamp is introduced here.
    // (The C spells the swap out through a temporary `cp`.)
    std::mem::swap(&mut el.el_line.cursor, &mut el.el_chared.c_kill.mark);
    CC_CURSOR
}

// [spec:libedit:def:emacs.em-universal-argument-fn]
// [spec:libedit:sem:emacs.em-universal-argument-fn]
/// Universal argument — multiply the current argument by 4.
///
/// ERR-modes-57: not bound anywhere in the default emacs or vi keymaps
/// (`^U` is `EM_KILL_LINE` here), so it is reachable only through an
/// explicit user binding.
pub(crate) fn em_universal_argument(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // ERR-modes-49: the cap is tested BEFORE the multiply, so the argument
    // can legitimately reach 4000000 through this function. Nothing is
    // changed on the error path — not `doingarg`, not `argument`.
    if el.el_state.argument > ARGUMENT_CAP {
        return CC_ERROR;
    }
    el.el_state.doingarg = 1;
    // Unconditional, so the first invocation turns the reset value 1 into 4.
    el.el_state.argument *= 4;
    // `CC_ARGHACK` makes the read loop `continue`, skipping the post-command
    // reset of `argument`, `doingarg` and `c_vcmd.action`. That is what lets
    // the count accumulate: 4, 16, 64, 256, ...
    CC_ARGHACK
}

// [spec:libedit:def:emacs.em-meta-next-fn]
// [spec:libedit:sem:emacs.em-meta-next-fn]
/// Add the 8th bit to the next character typed `[<ESC>]`.
pub(crate) fn em_meta_next(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // The prefix is consumed by `read_getcmd`, not here: it clears
    // `metanext` and ORs 0x80 into the next character before the keymap
    // lookup.
    el.el_state.metanext = 1;
    // `CC_ARGHACK` again, and load-bearing for the same reason: the read
    // loop skips its post-command reset, so a repeat count typed before
    // `ESC` survives into the command `ESC` prefixes.
    CC_ARGHACK
}

// [spec:libedit:def:emacs.em-toggle-overwrite-fn]
// [spec:libedit:sem:emacs.em-toggle-overwrite-fn]
/// Switch from insert to overwrite mode or vice versa. NOT bound in either
/// default keymap.
pub(crate) fn em_toggle_overwrite(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // "Otherwise" includes `MODE_REPLACE_1` (2), vi's single-character
    // replace state, which is therefore folded back to `MODE_INSERT` rather
    // than to `MODE_REPLACE`. `argument` is ignored, so an even count does
    // not cancel out.
    el.el_state.inputmode = if el.el_state.inputmode == MODE_INSERT {
        MODE_REPLACE
    } else {
        MODE_INSERT
    };
    CC_NORM
}

// [spec:libedit:def:emacs.em-copy-prev-word-fn]
// [spec:libedit:sem:emacs.em-copy-prev-word-fn]
/// Copy the previous word to the cursor `[M-^_]`.
pub(crate) fn em_copy_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // Does a bounds check. Text before the cursor is not moved by the insert
    // below, so `cp` stays valid across it.
    let mut cp = c_prev_word(el, el.el_line.cursor, 0, el.el_state.argument, ce_is_word);

    c_insert(el, (el.el_line.cursor - cp) as i32);
    let oldc = el.el_line.cursor;
    let mut dp = oldc;
    // ERR-modes-22: the loop is not conditioned on `c_insert` having
    // succeeded. When `ch_enlargebufs` fails, `c_insert` returns without
    // opening the gap and this OVERWRITES the text after the cursor,
    // stopping at the un-advanced `lastchar`; `lastchar` does not move and
    // the function still reports `CC_REFRESH`. Reproduced.
    while cp < oldc && dp < el.el_line.lastchar {
        let ch = el.el_line.buffer[cp];
        el.el_line.buffer[dp] = ch;
        dp += 1;
        cp += 1;
    }

    el.el_line.cursor = dp; // put cursor at end
    // Neither the mark nor the kill buffer is touched, and the source text
    // stays in place: this copies, it does not move.
    CC_REFRESH
}

// [spec:libedit:def:emacs.em-inc-search-next-fn]
// [spec:libedit:sem:emacs.em-inc-search-next-fn]
/// Emacs incremental next search. NOT bound in the default emacs keymap,
/// where `^S` is `ED_IGNORE`; reachable by explicit binding and from inside
/// an in-progress search, where it flips the direction to forwards.
pub(crate) fn em_inc_search_next(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    // Only the length is reset, so the helper takes its "first round" path
    // and starts from an empty pattern; `patbuf` itself is not cleared.
    el.el_search.patlen = 0;
    // Returned verbatim: `CC_ERROR`, `CC_NORM`, `CC_REFRESH` or `CC_EOF`.
    ce_inc_search(el, i32::from(ED_SEARCH_NEXT_HISTORY))
}

// [spec:libedit:def:emacs.em-inc-search-prev-fn]
// [spec:libedit:sem:emacs.em-inc-search-prev-fn]
/// Emacs incremental reverse search `[^R]`.
pub(crate) fn em_inc_search_prev(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    el.el_search.patlen = 0;
    ce_inc_search(el, i32::from(ED_SEARCH_PREV_HISTORY))
}

// [spec:libedit:def:emacs.em-delete-prev-char-fn]
// [spec:libedit:sem:emacs.em-delete-prev-char-fn]
/// Delete the character to the left of the cursor `[^H]`, `[^?]`.
pub(crate) fn em_delete_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    if el.el_state.doingarg != 0 {
        // Clamps to `cursor - buffer`. ERR-modes-19: the `current != emacs`
        // test inside is a tautology, so this also takes a vi undo snapshot
        // and rewrites the kill buffer with the doomed text — contradicting
        // this rule's closing claim that the kill buffer is never written.
        c_delbefore(el, el.el_state.argument);
    } else {
        // Exactly the one character before the cursor, no clamping and no vi
        // bookkeeping.
        c_delbefore1(el);
    }

    // Steps 3 and 4, fused. ERR-modes-04: the C moves the cursor by the
    // UNCLAMPED argument, forming a pointer before the start of the object,
    // and only then clamps it back to `buffer` — undefined behaviour with a
    // determinate net effect. Defined here as saturating arithmetic, which
    // yields exactly that net effect: an over-large count deletes everything
    // from the head of the line up to the old cursor and lands at `buffer`.
    // A negative `argument` is not reachable (every accumulator builds a
    // positive count) and is defined as no movement rather than as the C's
    // second flavour of out-of-bounds pointer.
    let n = usize::try_from(el.el_state.argument).unwrap_or(0);
    el.el_line.cursor = el.el_line.cursor.saturating_sub(n);
    // The mark is not adjusted even though text before it has shifted left.
    CC_REFRESH
}

#[cfg(test)]
#[path = "emacs/test.rs"]
mod test;
