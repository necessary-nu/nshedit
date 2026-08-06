//! Ported from `src/emacs.c`; rules live in `docs/spec/port/src/emacs.md`.

use crate::chared::{
    NOP, c__next_word, c__prev_word, c_delafter, c_delafter1, c_delbefore, c_delbefore1, c_insert,
    ce__isword, cv_delfini,
};
use crate::el::{EditLine, ElActionT};
use crate::fcns::{ED_SEARCH_NEXT_HISTORY, ED_SEARCH_PREV_HISTORY};
use crate::histedit::{CC_ARGHACK, CC_CURSOR, CC_EOF, CC_ERROR, CC_NORM, CC_REFRESH};
use crate::locale::{self, iswalpha, iswlower, iswupper, towlower, towupper};
use crate::map::MAP_VI;
use crate::search::ce_inc_search;
use crate::terminal::{terminal_beep, terminal_writec};

/// C: `#define MODE_INSERT 0` (`chared.h`), the value `el_state.inputmode`
/// holds outside vi's replace states.
///
/// Spelled out here because `crate::chared` does not export the `MODE_*`
/// constants yet; they belong there, next to the rest of `chared.h`.
const MODE_INSERT: i32 = 0;
/// C: `#define MODE_REPLACE 1` (`chared.h`). `MODE_REPLACE_1` (2) has no
/// name here because [`em_toggle_overwrite`] never produces it — see its
/// rule.
const MODE_REPLACE: i32 = 1;

/// `while (cp < to) *kp++ = *cp++; el->el_chared.c_kill.last = kp;` — the
/// one shape every kill and copy in this file uses.
///
/// `kp` starts at index 0 of `c_kill.buf` every time, which is ERR-modes-54:
/// kills never accumulate in either direction, and saving a zero-length span
/// sets `last` to 0 and so *empties* the kill buffer, after which
/// [`em_yank`] returns `CC_NORM` and inserts nothing.
///
/// Unbounded, exactly as the C, and for the C's reason: `c_kill.buf` is
/// allocated with the same element count as the line buffer and grown in
/// lockstep with it by `ch_enlargebufs`, so a whole-line save always fits.
/// `to` may sit above `lastchar` — `em_kill_region` reaches here with a
/// stale mark — but never above the allocation, since the mark is only ever
/// a former cursor value or the head of the line.
fn kill_save(el: &mut EditLine, from: usize, to: usize) {
    let mut kp = 0;
    let mut cp = from;
    while cp < to {
        let ch = el.el_line.buffer[cp];
        el.el_chared.c_kill.buf[kp] = ch;
        kp += 1;
        cp += 1;
    }
    el.el_chared.c_kill.last = kp;
}

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

    let cp = c__next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce__isword,
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
pub(crate) fn em_kill_line(el: &mut EditLine, c: u32) -> ElActionT {
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

    el.el_line.cursor = c__next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce__isword,
    );

    // The test is on the keymap *type*, not on which map is currently
    // active, so this emacs function completes a pending vi operator
    // whenever the editor has been put in vi mode.
    if el.el_map.r#type == MAP_VI && el.el_chared.c_vcmd.action != NOP {
        cv_delfini(el);
        return CC_REFRESH;
    }
    CC_CURSOR
}

// [spec:libedit:def:emacs.em-upper-case-fn]
// [spec:libedit:sem:emacs.em-upper-case-fn]
/// Uppercase the characters from cursor to end of current word `[M-u]`.
pub(crate) fn em_upper_case(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c; // C: `wint_t c __attribute__((__unused__))`.

    let cs = locale::charset();
    let ep = c__next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce__isword,
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
    // code (ERR-modes-71) — `c__next_word` has already clamped `ep` — and is
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
    let ep = c__next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce__isword,
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
    let ep = c__next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        ce__isword,
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
    if el.el_state.argument > 1000000 {
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
    let mut cp = c__prev_word(el, el.el_line.cursor, 0, el.el_state.argument, ce__isword);

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
mod test {
    use super::*;
    use crate::chared::ch_init;
    use crate::el::blank_editline;
    use crate::map::MAP_EMACS;
    use crate::search::search_init;

    /// C: `wcsdup(L"*?_-.[]~=")` from `map_init_emacs`. `ch_init` leaves
    /// `el_map.wordchars` unset and `ce__isword` consults it, so an editor
    /// without it is one `el_init` never produces.
    const WORDCHARS_EMACS: &str = "*?_-.[]~=";

    /// An editor in the state `el_init` leaves behind, with `s` in the line
    /// and the cursor at `at`. `ch_init` sizes all four buffers at
    /// `EL_BUFSIZ` and puts `limit` two slots below the end, which is the
    /// slack `c_insert` shifts into.
    fn el_with(s: &str, at: usize) -> EditLine {
        let mut el = blank_editline();
        ch_init(&mut el);
        let chars: Vec<u32> = s.chars().map(u32::from).collect();
        el.el_line.buffer[..chars.len()].copy_from_slice(&chars);
        el.el_line.lastchar = chars.len();
        el.el_line.cursor = at;
        // `el_init` allocates the pattern buffer, which the incremental
        // search reads and writes.
        search_init(&mut el);
        // Nothing here has a terminal to talk to, and descriptor 0 is the
        // test runner's. `write_fd` already treats a negative one as "no
        // destination", so the editor writes into the void instead of
        // spraying escape sequences over the test output.
        el.el_infd = -1;
        el.el_outfd = -1;
        el.el_errfd = -1;
        el.el_map.r#type = MAP_EMACS;
        el.el_map.wordchars = Some(WORDCHARS_EMACS.chars().map(u32::from).collect());
        el
    }

    fn text(el: &EditLine) -> String {
        el.el_line.buffer[..el.el_line.lastchar]
            .iter()
            .filter_map(|&c| char::from_u32(c))
            .collect()
    }

    /// The kill buffer as `c_kill.last` describes it; it carries no
    /// terminator, so the length field is the only thing that ends it.
    fn killed(el: &EditLine) -> String {
        el.el_chared.c_kill.buf[..el.el_chared.c_kill.last]
            .iter()
            .filter_map(|&c| char::from_u32(c))
            .collect()
    }

    /// Fill the kill buffer, so that a command which is supposed to *empty*
    /// it can be told apart from one that leaves it alone.
    fn preload_kill(el: &mut EditLine, s: &str) {
        let chars: Vec<u32> = s.chars().map(u32::from).collect();
        el.el_chared.c_kill.buf[..chars.len()].copy_from_slice(&chars);
        el.el_chared.c_kill.last = chars.len();
    }

    // [spec:libedit:sem:emacs.em-delete-next-word-fn/test]
    /// `M-d` cuts forward to the end of the current word and leaves the cursor
    /// where the word began. ERR-modes-23: the mark is not adjusted for the
    /// text that just moved out from under it.
    #[test]
    fn delete_next_word_cuts_forward_and_leaves_the_cursor_in_place() {
        let mut el = el_with("foo bar", 0);
        el.el_chared.c_kill.mark = 5;

        assert_eq!(em_delete_next_word(&mut el, 0), CC_REFRESH);
        assert_eq!(text(&el), " bar");
        assert_eq!(el.el_line.cursor, 0);
        // ERR-modes-19 makes `c_delafter` re-yank the identical span over the
        // hand-rolled save, so only the content is observable.
        assert_eq!(killed(&el), "foo");
        assert_eq!(el.el_chared.c_kill.mark, 5, "still pointing at old text");

        let mut el = el_with("foo", 3);
        assert_eq!(em_delete_next_word(&mut el, 0), CC_ERROR);
        assert_eq!(text(&el), "foo");
    }

    // [spec:libedit:sem:emacs.em-kill-region-fn/test]
    /// `^W` cuts between mark and cursor whichever way round they are, but the
    /// cursor ends up in a different place: cutting forward leaves it where it
    /// was, cutting backward moves it to the mark. Either way the mark keeps
    /// its old value (ERR-modes-23), which after a forward cut leaves it above
    /// `lastchar`.
    #[test]
    fn kill_region_works_in_both_directions_but_lands_differently() {
        let mut el = el_with("abcdef", 1);
        el.el_chared.c_kill.mark = 4;
        assert_eq!(em_kill_region(&mut el, 0), CC_REFRESH);
        assert_eq!(text(&el), "aef");
        assert_eq!(el.el_line.cursor, 1);
        assert_eq!(killed(&el), "bcd");
        assert_eq!(el.el_chared.c_kill.mark, 4);
        assert!(el.el_chared.c_kill.mark > el.el_line.lastchar);

        let mut el = el_with("abcdef", 4);
        el.el_chared.c_kill.mark = 1;
        assert_eq!(em_kill_region(&mut el, 0), CC_REFRESH);
        assert_eq!(text(&el), "aef");
        assert_eq!(el.el_line.cursor, 1);
        assert_eq!(killed(&el), "bcd");
    }

    // [spec:libedit:sem:emacs.em-kill-region-fn/test]
    /// A mark sitting on the cursor takes the "mark is before cursor" branch
    /// with a zero-width region, and the zero-length save *empties* the kill
    /// buffer rather than leaving it alone — so `^W` with no region destroys
    /// the previous kill and still reports success.
    #[test]
    fn an_empty_kill_region_wipes_the_kill_buffer() {
        let mut el = el_with("abcdef", 3);
        preload_kill(&mut el, "xyz");
        el.el_chared.c_kill.mark = 3;

        assert_eq!(em_kill_region(&mut el, 0), CC_REFRESH);
        assert_eq!(el.el_chared.c_kill.last, 0);
        assert_eq!(text(&el), "abcdef");
        assert_eq!(el.el_line.cursor, 3);
    }

    // [spec:libedit:sem:emacs.em-copy-region-fn/test]
    /// `M-W` fills the kill buffer and changes nothing else — not the line,
    /// not the cursor, not the mark — and returns `CC_NORM`, so the user gets
    /// no feedback at all that it happened.
    #[test]
    fn copy_region_is_completely_silent() {
        for (cursor, mark) in [(1usize, 4usize), (4, 1)] {
            let mut el = el_with("abcdef", cursor);
            el.el_chared.c_kill.mark = mark;
            assert_eq!(em_copy_region(&mut el, 0), CC_NORM);
            assert_eq!(killed(&el), "bcd");
            assert_eq!(text(&el), "abcdef");
            assert_eq!(el.el_line.cursor, cursor);
            assert_eq!(el.el_chared.c_kill.mark, mark);
            assert_eq!(el.el_chared.c_undo.len, -1, "no undo snapshot either");
        }
    }

    // [spec:libedit:sem:emacs.em-gosmacs-transpose-fn/test]
    /// Gosling transpose swaps the two characters *before* the cursor.
    /// ERR-modes-56: point does not advance afterwards — GNU emacs
    /// `transpose-chars` does — and `el_state.argument` is ignored entirely,
    /// so a count does not repeat the swap.
    #[test]
    fn gosmacs_transpose_swaps_behind_the_cursor_without_advancing() {
        let mut el = el_with("abc", 2);
        el.el_state.argument = 5;
        assert_eq!(em_gosmacs_transpose(&mut el, u32::from(b'T')), CC_REFRESH);
        assert_eq!(text(&el), "bac");
        assert_eq!(el.el_line.cursor, 2);

        // One character behind the cursor is not two.
        let mut el = el_with("abc", 1);
        assert_eq!(em_gosmacs_transpose(&mut el, 0), CC_ERROR);
        assert_eq!(text(&el), "abc");
    }

    // [spec:libedit:sem:emacs.em-set-mark-fn/test]
    /// There is one mark slot and no ring: setting it overwrites whatever was
    /// there. `CC_NORM` means no redraw and no "Mark set" message.
    #[test]
    fn set_mark_overwrites_the_single_mark_slot() {
        let mut el = el_with("abcdef", 4);
        el.el_chared.c_kill.mark = 1;
        assert_eq!(em_set_mark(&mut el, 0), CC_NORM);
        assert_eq!(el.el_chared.c_kill.mark, 4);
        assert_eq!(el.el_line.cursor, 4);
        assert_eq!(text(&el), "abcdef");
    }

    // [spec:libedit:sem:emacs.em-exchange-mark-fn/test]
    /// `^X^X` swaps cursor and mark unconditionally — no validation, no clamp.
    /// ERR-modes-23 is what makes that a hazard: `em_kill_line` empties the
    /// line without touching the mark, so the exchange then strands the cursor
    /// beyond `lastchar`, which no other command in the file will produce.
    #[test]
    fn exchange_mark_is_unvalidated_and_can_strand_the_cursor() {
        let mut el = el_with("abcdef", 2);
        el.el_chared.c_kill.mark = 5;
        assert_eq!(em_exchange_mark(&mut el, 0), CC_CURSOR);
        assert_eq!(el.el_line.cursor, 5);
        assert_eq!(el.el_chared.c_kill.mark, 2);

        let mut el = el_with("abcdef", 6);
        el.el_chared.c_kill.mark = 4;
        em_kill_line(&mut el, 0);
        assert_eq!(el.el_line.lastchar, 0);
        em_exchange_mark(&mut el, 0);
        assert_eq!(el.el_line.cursor, 4);
        assert!(el.el_line.cursor > el.el_line.lastchar);
    }

    // [spec:libedit:sem:emacs.em-universal-argument-fn/test]
    /// The count is multiplied by four unconditionally, so the first
    /// invocation turns the reset value 1 into 4 and the sequence accumulates
    /// 4, 16, 64. ERR-modes-49: the cap is tested before the multiply, so the
    /// argument can legitimately reach 4000000 through this function.
    #[test]
    fn universal_argument_quadruples_the_count() {
        let mut el = el_with("", 0);
        assert_eq!(em_universal_argument(&mut el, 0), CC_ARGHACK);
        assert_eq!(el.el_state.argument, 4);
        assert_eq!(el.el_state.doingarg, 1);
        assert_eq!(em_universal_argument(&mut el, 0), CC_ARGHACK);
        assert_eq!(el.el_state.argument, 16);

        let mut el = el_with("", 0);
        el.el_state.argument = 1_000_000;
        assert_eq!(em_universal_argument(&mut el, 0), CC_ARGHACK);
        assert_eq!(el.el_state.argument, 4_000_000);

        // Nothing at all changes on the error path — not `doingarg`.
        let mut el = el_with("", 0);
        el.el_state.argument = 1_000_001;
        assert_eq!(em_universal_argument(&mut el, 0), CC_ERROR);
        assert_eq!(el.el_state.argument, 1_000_001);
        assert_eq!(el.el_state.doingarg, 0);
    }

    // [spec:libedit:sem:emacs.em-meta-next-fn/test]
    /// `ESC` only arms the flag; `read_getcmd` is what consumes it and ORs
    /// 0x80 into the next character. `CC_ARGHACK` is load-bearing — it makes
    /// the read loop skip its post-command reset, so a count typed before
    /// `ESC` survives into the command `ESC` prefixes.
    #[test]
    fn meta_next_arms_the_flag_and_preserves_a_pending_count() {
        let mut el = el_with("abc", 1);
        el.el_state.argument = 7;
        el.el_state.doingarg = 1;

        assert_eq!(em_meta_next(&mut el, 0), CC_ARGHACK);
        assert_eq!(el.el_state.metanext, 1);
        assert_eq!(el.el_state.argument, 7);
        assert_eq!(el.el_state.doingarg, 1);
        assert_eq!(text(&el), "abc");
    }

    // [spec:libedit:sem:emacs.em-toggle-overwrite-fn/test]
    /// Only `MODE_INSERT` maps to `MODE_REPLACE`; everything else maps back to
    /// `MODE_INSERT`. That "everything else" includes vi's single-character
    /// replace state, which is therefore folded to insert rather than to
    /// sticky overwrite. The count is ignored, so an even one does not cancel.
    #[test]
    fn toggle_overwrite_folds_vis_one_shot_replace_back_to_insert() {
        let mut el = el_with("abc", 1);
        el.el_state.argument = 2;
        assert_eq!(em_toggle_overwrite(&mut el, 0), CC_NORM);
        assert_eq!(el.el_state.inputmode, MODE_REPLACE);
        assert_eq!(em_toggle_overwrite(&mut el, 0), CC_NORM);
        assert_eq!(el.el_state.inputmode, MODE_INSERT);

        // `MODE_REPLACE_1` is 2.
        el.el_state.inputmode = 2;
        em_toggle_overwrite(&mut el, 0);
        assert_eq!(el.el_state.inputmode, MODE_INSERT);
    }

    // [spec:libedit:sem:emacs.em-copy-prev-word-fn/test]
    /// `M-^_` copies rather than moves: the source word stays where it is, a
    /// duplicate is inserted at the cursor, and the cursor ends just past the
    /// copy. No separator is added, so at the end of a word the two run
    /// together. Neither the mark nor the kill buffer is touched.
    #[test]
    fn copy_prev_word_duplicates_the_word_without_a_separator() {
        let mut el = el_with("foo bar", 7);
        el.el_chared.c_kill.mark = 2;
        assert_eq!(em_copy_prev_word(&mut el, 0), CC_REFRESH);
        assert_eq!(text(&el), "foo barbar");
        assert_eq!(el.el_line.cursor, 10);
        assert_eq!(el.el_chared.c_kill.last, 0);
        assert_eq!(el.el_chared.c_kill.mark, 2);

        // Mid-line the copied span runs from the start of the previous word up
        // to the cursor, so it carries the intervening blank with it.
        let mut el = el_with("foo bar baz", 8);
        assert_eq!(em_copy_prev_word(&mut el, 0), CC_REFRESH);
        assert_eq!(text(&el), "foo bar bar baz");
        assert_eq!(el.el_line.cursor, 12);

        let mut el = el_with("foo", 0);
        assert_eq!(em_copy_prev_word(&mut el, 0), CC_ERROR);
        assert_eq!(text(&el), "foo");
    }

    // [spec:libedit:sem:emacs.em-inc-search-next-fn/test]
    // [spec:libedit:sem:emacs.em-inc-search-prev-fn/test]
    /// Both entry points reset only the pattern *length*, never the pattern
    /// buffer itself, and then hand back whatever the shared helper returns
    /// without inspecting it. Zeroing the length is what makes the helper take
    /// its "first round" path and start from an empty pattern; the stale text
    /// left in `patbuf` above it is never read again.
    ///
    /// The helper's entry bound check is the one path into it that needs
    /// neither a terminal nor a populated keymap, which is why it is the one
    /// used here. It also means the direction constant the two functions
    /// differ by is not observable: nothing on this path consults it.
    #[test]
    fn the_incremental_searches_reset_the_pattern_length_and_pass_the_result_back() {
        for start in [
            em_inc_search_prev as fn(&mut EditLine, u32) -> ElActionT,
            em_inc_search_next,
        ] {
            let mut el = el_with("abcdef", 3);
            let stale: Vec<u32> = "xyz".chars().map(u32::from).collect();
            el.el_search.patbuf[..3].copy_from_slice(&stale);
            el.el_search.patlen = 3;
            // No room for the search's own prompt line, so the helper refuses
            // before it touches anything.
            el.el_line.limit = 8;

            assert_eq!(start(&mut el, 0), CC_ERROR);
            assert_eq!(el.el_search.patlen, 0);
            assert_eq!(el.el_search.patbuf[..3], stale[..], "patbuf is not cleared");
            assert_eq!(text(&el), "abcdef");
            assert_eq!(el.el_line.cursor, 3);
        }
    }
}
