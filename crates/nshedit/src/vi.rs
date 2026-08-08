//! Ported from `src/vi.c`; rules live in `docs/spec/port/src/vi.md`.

use core::ffi::c_char;
use core::ptr;
use std::ffi::{CStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::chared::{
    CHAR_BACK, CHAR_FWD, DELETE, INSERT, MODE_INSERT, MODE_REPLACE, MODE_REPLACE_1, NOP, YANK,
    c_delafter, c_delbefore, c_delbefore1, c_insert, cv__endword, cv__isWord, cv__isword,
    cv_delfini, cv_next_word, cv_prev_word, cv_undo, cv_yank,
};
use crate::chartype::ct_decode_string;
use crate::common::{ed_argument_digit, ed_kill_line, ed_newline, ed_next_char};
use crate::el::{EL_BUFSIZ, EditLine, ElActionT};
use crate::emacs::em_kill_line;
use crate::errno::{ERANGE, set_errno};
use crate::fcns::{ED_SEARCH_NEXT_HISTORY, ED_SEARCH_PREV_HISTORY};
use crate::hist::{hist_first, hist_get};
use crate::histedit::{CC_ARGHACK, CC_CURSOR, CC_EOF, CC_ERROR, CC_NORM, CC_REFRESH};
use crate::locale::{self, iswlower, iswupper, towlower, towupper};
use crate::map::{ElMapCurrent, MAP_VI};
use crate::read::{el_wgetc, el_wpush};
use crate::refresh::{re_fastaddc, re_refresh};
use crate::search::{cv_csearch, cv_repeat_srch, cv_search};
use crate::terminal::{terminal_beep, terminal_writec};

// ---------------------------------------------------------------------------
// Constants and libc calls the C reaches through headers and libraries this
// crate has no home for yet. Each is noted where it is used; none of them is a
// re-declaration of something `crate::chared`, `crate::el`, `crate::map`,
// `crate::histedit`, `crate::locale` or `crate::errno` already publishes.
// ---------------------------------------------------------------------------

/// C: `vi.c` — `#define TMP_BUFSIZ (EL_BUFSIZ * MB_LEN_MAX)`.
const TMP_BUFSIZ: usize = EL_BUFSIZ * locale::MB_LEN_MAX;

/// C: `el_getc(el, cp)` from `eln.c`, per `sem:eln.el-getc-fn`.
///
/// `eln.c` has no Rust module yet, and `lib.rs` is not this translation's to
/// extend, so its one caller in this file carries a private stand-in. It is the
/// narrow wrapper over [`el_wgetc`] verbatim: read one wide character, clear
/// `*cp` on every path, and deliver the character only when it has a
/// single-byte representation in the initial shift state of the current
/// `LC_CTYPE` — `wctob`, expressed here through the one encoder
/// [`locale::wcrtomb`] so it cannot disagree with the rest of the crate.
///
/// The `errno = ERANGE` write on the no-single-byte-form path is recorded in
/// [`crate::errno`] like every other errno the port sets. Nothing in this
/// crate reads it and [`vi_alias`] — the sole caller — folds -1 and 0 into the
/// same `CC_ERROR`, so it is unobservable here; it is written anyway so that
/// this stand-in and the ABI crate's `el_getc`, which publishes the same value
/// to a C caller, do not diverge.
fn el_getc(el: &mut EditLine, cp: &mut u8) -> i32 {
    let mut wc: u32 = 0;
    let num_read = el_wgetc(el, &mut wc);
    *cp = 0;
    if num_read <= 0 {
        return num_read;
    }
    let mut scratch = [0u8; locale::MB_LEN_MAX];
    match locale::wcrtomb(locale::charset(), wc, &mut scratch) {
        // `wctob` answers with the byte only for a one-byte encoding.
        Some(1) => {
            *cp = scratch[0];
            1
        }
        // `wctob` returned `EOF`. The character has already been consumed and
        // is lost.
        _ => {
            set_errno(ERANGE);
            -1
        }
    }
}

/// C: `(el->el_getenv)(name)`, per `sem:el.editline.el-getenv-fn`.
///
/// The hook and the built-in default have different shapes — [`crate::el`]'s
/// own note says the two "are reconciled where the hook is consulted, not
/// here", and this is the first consult site in the crate. An installed hook
/// hands back a borrowed `char *` across the C ABI and is called through;
/// `None` is the built-in [`crate::el::secure_getenv`], which owns what it
/// returns. Both answers are copied out, because the C only ever uses the
/// value before the next hook call.
fn el_getenv(el: &EditLine, name: &CStr) -> Option<OsString> {
    match el.el_getenv {
        Some(f) => {
            // SAFETY: `f` is what an application installed through
            // `el_set(EL_GETENV, ...)`; `def:el.editline.el-getenv-fn` makes
            // it a C function taking one NUL-terminated name, and `name` is
            // exactly that and outlives the call.
            let value = unsafe { f(name.as_ptr()) };
            if value.is_null() {
                return None;
            }
            // SAFETY: the hook's contract is a NUL-terminated value that stays
            // valid at least until the next hook call; this copies it out
            // immediately and retains nothing.
            let bytes = unsafe { CStr::from_ptr(value) }.to_bytes().to_vec();
            Some(OsString::from_vec(bytes))
        }
        None => crate::el::secure_getenv(&name.to_string_lossy()),
    }
}

/// The tail every motion command ends on: with an operator armed, the span
/// from `c_vcmd.pos` to wherever the cursor has just landed is deleted or
/// yanked and the line is redrawn; with none armed the cursor is all that
/// moved.
///
/// `cv_delfini` clears `c_vcmd.action` itself, so a motion cannot complete two
/// operators, and it is what decides the direction of the span — every caller
/// has already written the landing position into `el_line.cursor`.
pub(crate) fn end_motion(el: &mut EditLine) -> ElActionT {
    if el.el_chared.c_vcmd.action != NOP {
        cv_delfini(el);
        return CC_REFRESH;
    }
    CC_CURSOR
}

/// [`end_motion`] behind the C's `el_map.type == MAP_VI` test.
///
/// Which motions carry that test is not a distinction the C draws on purpose:
/// `w`/`W` have it and `b`/`B`/`e`/`E`/`0`/`%` do not, and the ones in
/// [`crate::common`] and [`crate::emacs`] all do. Since the test is on the
/// keymap *type* and not on which map is currently active, the ones that carry
/// it complete a pending vi operator whenever the editor has been put in vi
/// mode — including when the key that reached them was bound in an emacs map.
/// The two spellings are kept apart so a reader can see at each call site
/// which one the C wrote.
pub(crate) fn end_vi_motion(el: &mut EditLine) -> ElActionT {
    if el.el_map.r#type == MAP_VI {
        end_motion(el)
    } else {
        CC_CURSOR
    }
}

// [spec:libedit:def:vi.cv-action-fn]
// [spec:libedit:sem:vi.cv-action-fn]
/// C: `static el_action_t cv_action(EditLine *el, wint_t c)`
///
/// `c` is the operator bitmask (`DELETE`, `DELETE|INSERT`, `YANK`), not a
/// keystroke; see `sem:vi.cv-action-fn`.
fn cv_action(el: &mut EditLine, c: u32) -> ElActionT {
    // Second call — an operator is already pending, so this is `dd`/`cc`/`yy`.
    if el.el_chared.c_vcmd.action != NOP {
        // A different operator (`dy`, `cd`) is rejected outright: no line, no
        // cursor, no kill buffer, no undo buffer and no `c_vcmd` change. The
        // dispatcher beeps and then clears the pending operator for us.
        if c != el.el_chared.c_vcmd.action as u32 {
            return CC_ERROR;
        }

        // ERR-modes-68: `el_state.argument` is never read on this path, so
        // there is no `3dd`. The count is silently discarded.
        if c & YANK as u32 == 0 {
            cv_undo(el);
        }
        // The whole line `[buffer, lastchar)` replaces the kill buffer; a
        // zero-length line empties it.
        cv_yank(el, 0, el.el_line.lastchar as i32);
        el.el_chared.c_vcmd.action = NOP;
        // C: `c_vcmd.pos = 0`, a null pointer rather than `buffer`.
        // `def:chared.c-vcmd-t` made `pos` an offset, which collapses NULL
        // onto `buffer`; unobservable here because `action` is cleared in the
        // same breath, which is the other half of `cv_delfini`'s guard.
        el.el_chared.c_vcmd.pos = 0;
        if c & YANK as u32 == 0 {
            el.el_line.lastchar = 0;
            el.el_line.cursor = 0;
        }
        if c & INSERT as u32 != 0 {
            el.el_map.current = ElMapCurrent::Key;
        }

        return CC_REFRESH;
    }

    // First call — record the anchor the following motion will operate back to,
    // and the operator itself. `CC_ARGHACK` is load-bearing: it is the only
    // return the dispatcher answers by skipping its per-command reset of
    // `argument`, `doingarg` and `c_vcmd.action`, so both the operator and any
    // count typed before it survive into the next keystroke.
    el.el_chared.c_vcmd.pos = el.el_line.cursor;
    el.el_chared.c_vcmd.action = c as i32;
    CC_ARGHACK
}

// [spec:libedit:def:vi.cv-paste-fn]
// [spec:libedit:sem:vi.cv-paste-fn]
/// C: `static el_action_t cv_paste(EditLine *el, wint_t c)`
///
/// `c` is a boolean: zero pastes after the cursor, non-zero pastes at it.
/// See `sem:vi.cv-paste-fn`.
fn cv_paste(el: &mut EditLine, c: u32) -> ElActionT {
    // 1. C: `len = k->last - k->buf`. The C's `k->buf == NULL` and this
    //    empty-`Vec` test are the same state.
    let len = el.el_chared.c_kill.last;
    if el.el_chared.c_kill.buf.is_empty() || len == 0 {
        return CC_ERROR;
    }
    // The `DEBUG_PASTE` trace is compiled out and is not ported.

    // 2.
    cv_undo(el);

    // 3. `p` (`c == 0`) lands the text after the character under the cursor;
    //    at end of line the cursor does not move, so `p` there behaves as `P`.
    if c == 0 && el.el_line.cursor < el.el_line.lastchar {
        el.el_line.cursor += 1;
    }

    // 4.
    c_insert(el, len as i32);

    // 5. ERR-modes-42, reproduced: this fires only when `c_insert` silently
    //    failed to grow the buffers, by which point `cv_undo` has run and the
    //    cursor may already have moved. The error path is not side-effect free.
    if el.el_line.cursor + len > el.el_line.lastchar {
        return CC_ERROR;
    }

    // 6. ERR-modes-68: the kill buffer is pasted exactly once — `3p` pastes one
    //    copy — and is not consumed. ERR-modes-58: the cursor is left on the
    //    *first* pasted character, where real vi leaves it on the last.
    let at = el.el_line.cursor;
    el.el_line.buffer[at..at + len].copy_from_slice(&el.el_chared.c_kill.buf[..len]);

    // 7.
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-paste-next-fn]
// [spec:libedit:sem:vi.vi-paste-next-fn]
pub(crate) fn vi_paste_next(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    cv_paste(el, 0)
}

// [spec:libedit:def:vi.vi-paste-prev-fn]
// [spec:libedit:sem:vi.vi-paste-prev-fn]
pub(crate) fn vi_paste_prev(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    cv_paste(el, 1)
}

// [spec:libedit:def:vi.vi-prev-big-word-fn]
// [spec:libedit:sem:vi.vi-prev-big-word-fn]
pub(crate) fn vi_prev_big_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    el.el_line.cursor = cv_prev_word(el, el.el_line.cursor, 0, el.el_state.argument, cv__isWord);

    // No `el_map.type == MAP_VI` guard here, unlike `vi_next_big_word`. The
    // landing position is left of `c_vcmd.pos`, so `cv_delfini` takes its
    // negative-size branch: `dB` deletes `[new_cursor, c_vcmd.pos)`, exclusive
    // of the character that was under the cursor.
    end_motion(el)
}

// [spec:libedit:def:vi.vi-prev-word-fn]
// [spec:libedit:sem:vi.vi-prev-word-fn]
pub(crate) fn vi_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // `cv__isword` — three classes, and a word is a maximal run of one class,
    // so a punctuation run is its own word here where `B` swallows it.
    el.el_line.cursor = cv_prev_word(el, el.el_line.cursor, 0, el.el_state.argument, cv__isword);

    end_motion(el)
}

// [spec:libedit:def:vi.vi-next-big-word-fn]
// [spec:libedit:sem:vi.vi-next-big-word-fn]
pub(crate) fn vi_next_big_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // ERR-modes-06, defined as the rule directs: the C writes
    // `cursor >= lastchar - 1`, which forms `buffer - 1` on an empty line.
    // "Fewer than two characters remain at or after the cursor" is the same
    // test everywhere the C's is defined, and answers `CC_ERROR` where it is
    // not. Note `W` therefore fails on the *last* character of the line.
    if el.el_line.cursor + 1 >= el.el_line.lastchar {
        return CC_ERROR;
    }

    el.el_line.cursor = cv_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        cv__isWord,
    );

    // No `cursor++`: `dW`/`cW`/`yW` is exclusive of the character at the
    // landing position, which is the key difference from `E`. The `MAP_VI`
    // guard is the C's and means a pending operator is ignored when this is
    // invoked from an emacs-type map.
    end_vi_motion(el)
}

// [spec:libedit:def:vi.vi-next-word-fn]
// [spec:libedit:sem:vi.vi-next-word-fn]
pub(crate) fn vi_next_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // ERR-modes-06, as in `vi_next_big_word`.
    if el.el_line.cursor + 1 >= el.el_line.lastchar {
        return CC_ERROR;
    }

    // `cv_next_word` suppresses its trailing-whitespace skip on the last
    // iteration when `c_vcmd.action` is exactly `DELETE|INSERT`, which is what
    // makes `cw` behave like `ce`; it does not apply to `d` or `y`.
    el.el_line.cursor = cv_next_word(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        cv__isword,
    );

    end_vi_motion(el)
}

// [spec:libedit:def:vi.vi-change-case-fn]
// [spec:libedit:sem:vi.vi-change-case-fn]
pub(crate) fn vi_change_case(el: &mut EditLine, c: u32) -> ElActionT {
    // The C reuses its `c` parameter as the scratch variable; the incoming
    // value is ignored either way.
    let _ = c;

    // 1. Nothing under the cursor, the empty line included. No undo snapshot.
    if el.el_line.cursor >= el.el_line.lastchar {
        return CC_ERROR;
    }
    // 2. One snapshot covers the whole run.
    cv_undo(el);

    let cs = locale::charset();
    // 3.
    let mut i = 0;
    while i < el.el_state.argument {
        let ch = el.el_line.buffer[el.el_line.cursor];
        // Characters that are neither upper nor lower case are left alone, but
        // the iteration is still consumed and the cursor still advances.
        if iswupper(cs, ch) {
            el.el_line.buffer[el.el_line.cursor] = towlower(cs, ch);
        } else if iswlower(cs, ch) {
            el.el_line.buffer[el.el_line.cursor] = towupper(cs, ch);
        }

        el.el_line.cursor += 1;
        if el.el_line.cursor >= el.el_line.lastchar {
            // Clamped to the last character, never to `lastchar`.
            el.el_line.cursor -= 1;
            re_fastaddc(el);
            break;
        }
        re_fastaddc(el);
        i += 1;
    }
    // 4. `re_fastaddc` degrades to a full `re_refresh` on every call here (the
    //    cursor is never at `lastchar` at the point of call), so the redraw has
    //    already happened and `CC_NORM` asks for no further one.
    CC_NORM
}

// [spec:libedit:def:vi.vi-change-meta-fn]
// [spec:libedit:sem:vi.vi-change-meta-fn]
pub(crate) fn vi_change_meta(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // Delete with insert == change: first we delete and then we leave in
    // insert mode. This exact mask is also what `cv_next_word` tests for.
    cv_action(el, (DELETE | INSERT) as u32)
}

// [spec:libedit:def:vi.vi-insert-at-bol-fn]
// [spec:libedit:sem:vi.vi-insert-at-bol-fn]
pub(crate) fn vi_insert_at_bol(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // ERR-modes-60: column 0, not the first non-blank character as in real vi.
    el.el_line.cursor = 0;
    // After the move, so the snapshot's recorded cursor offset is 0.
    cv_undo(el);
    el.el_map.current = ElMapCurrent::Key;
    CC_CURSOR
}

// [spec:libedit:def:vi.vi-replace-char-fn]
// [spec:libedit:sem:vi.vi-replace-char-fn]
pub(crate) fn vi_replace_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // Only the *first* character is checked, not `argument` of them.
    if el.el_line.cursor >= el.el_line.lastchar {
        return CC_ERROR;
    }

    el.el_map.current = ElMapCurrent::Key;
    el.el_state.inputmode = MODE_REPLACE_1;
    cv_undo(el);
    // `CC_ARGHACK` is what carries the count through to `ed_insert`, which is
    // where the rest of `r` lives: `3rx` reaches it with a count of 3.
    CC_ARGHACK
}

// [spec:libedit:def:vi.vi-replace-mode-fn]
// [spec:libedit:sem:vi.vi-replace-mode-fn]
pub(crate) fn vi_replace_mode(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_map.current = ElMapCurrent::Key;
    // Sticky overwrite, as against `r`'s one-shot `MODE_REPLACE_1`; it lasts
    // until `vi_command_mode` resets it.
    el.el_state.inputmode = MODE_REPLACE;
    cv_undo(el);
    CC_NORM
}

// [spec:libedit:def:vi.vi-substitute-char-fn]
// [spec:libedit:sem:vi.vi-substitute-char-fn]
pub(crate) fn vi_substitute_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // ERR-modes-19: `c_delafter` clamps the count and then always takes the
    // undo snapshot and always writes the kill buffer, so there is no error
    // path here — with the cursor at `lastchar` nothing is deleted, the kill
    // buffer is emptied, undo is still snapshotted and insert mode is still
    // entered.
    c_delafter(el, el.el_state.argument);
    el.el_map.current = ElMapCurrent::Key;
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-substitute-line-fn]
// [spec:libedit:sem:vi.vi-substitute-line-fn]
pub(crate) fn vi_substitute_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    cv_undo(el);
    // ERR-modes-47: `em_kill_line` copies the identical span over this one, so
    // the first yank is redundant; the end state is what matters.
    cv_yank(el, 0, el.el_line.lastchar as i32);
    // Empties the line and sets `c_kill.last`; its `CC_REFRESH` is discarded.
    let _ = em_kill_line(el, 0);
    el.el_map.current = ElMapCurrent::Key;
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-change-to-eol-fn]
// [spec:libedit:sem:vi.vi-change-to-eol-fn]
pub(crate) fn vi_change_to_eol(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    cv_undo(el);
    // ERR-modes-47 again: `ed_kill_line` re-yanks the same span.
    let size = (el.el_line.lastchar - el.el_line.cursor) as i32;
    cv_yank(el, el.el_line.cursor, size);
    let _ = ed_kill_line(el, 0);
    el.el_map.current = ElMapCurrent::Key;
    // No error path: on an empty line this yanks nothing, deletes nothing and
    // still enters insert mode.
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-insert-fn]
// [spec:libedit:sem:vi.vi-insert-fn]
pub(crate) fn vi_insert(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_map.current = ElMapCurrent::Key;
    cv_undo(el);
    // Nothing visible changed, so no redraw at all.
    CC_NORM
}

// [spec:libedit:def:vi.vi-add-fn]
// [spec:libedit:sem:vi.vi-add-fn]
pub(crate) fn vi_add(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_map.current = ElMapCurrent::Key;
    let ret = if el.el_line.cursor < el.el_line.lastchar {
        el.el_line.cursor += 1;
        // The C follows this with `if (cursor > lastchar) cursor = lastchar`,
        // which cannot fire: the branch condition already guaranteed
        // `cursor < lastchar` before the increment.
        CC_CURSOR
    } else {
        CC_NORM
    };

    // After the cursor move, so the snapshot records the post-move offset.
    cv_undo(el);

    // ERR-modes-68: no `3a`.
    ret
}

// [spec:libedit:def:vi.vi-add-at-eol-fn]
// [spec:libedit:sem:vi.vi-add-at-eol-fn]
pub(crate) fn vi_add_at_eol(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_map.current = ElMapCurrent::Key;
    el.el_line.cursor = el.el_line.lastchar;
    // After the cursor move, so the snapshot's cursor offset is end of line.
    cv_undo(el);
    // The text did not change, so the cursor is all that needs redrawing.
    CC_CURSOR
}

// [spec:libedit:def:vi.vi-delete-meta-fn]
// [spec:libedit:sem:vi.vi-delete-meta-fn]
pub(crate) fn vi_delete_meta(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    cv_action(el, DELETE as u32)
}

// [spec:libedit:def:vi.vi-end-big-word-fn]
// [spec:libedit:sem:vi.vi-end-big-word-fn]
pub(crate) fn vi_end_big_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // `==`, not `>=`; this covers the empty line.
    if el.el_line.cursor == el.el_line.lastchar {
        return CC_ERROR;
    }

    el.el_line.cursor = cv__endword(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        cv__isWord,
    );

    // The `+1` makes `dE`/`cE`/`yE` inclusive of the character landed on,
    // which is the whole reason an end-of-word motion differs from `W`. No
    // `MAP_VI` guard here, unlike `vi_next_big_word`.
    if el.el_chared.c_vcmd.action != NOP {
        el.el_line.cursor += 1;
    }
    end_motion(el)
}

// [spec:libedit:def:vi.vi-end-word-fn]
// [spec:libedit:sem:vi.vi-end-word-fn]
pub(crate) fn vi_end_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == el.el_line.lastchar {
        return CC_ERROR;
    }

    // The three-class `cv__isword`, so `foo.bar` is three words here and one
    // for `E`.
    el.el_line.cursor = cv__endword(
        el,
        el.el_line.cursor,
        el.el_line.lastchar,
        el.el_state.argument,
        cv__isword,
    );

    if el.el_chared.c_vcmd.action != NOP {
        el.el_line.cursor += 1;
    }
    end_motion(el)
}

// [spec:libedit:def:vi.vi-undo-fn]
// [spec:libedit:sem:vi.vi-undo-fn]
pub(crate) fn vi_undo(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1/2. `len == -1` only between `ch_init`/`ch_reset` and the first
    //      `cv_undo`, so this is "nothing to undo yet" and never fires again.
    let un_len = el.el_chared.c_undo.len;
    let un_cursor = el.el_chared.c_undo.cursor;
    if un_len == -1 {
        return CC_ERROR;
    }

    // 3/4. The C does not copy text: it *swaps* the line and undo buffers,
    //      which is what makes `u` its own inverse.
    let cur_len = el.el_line.lastchar as isize;
    let cur_cursor = el.el_line.cursor as i32;
    core::mem::swap(&mut el.el_line.buffer, &mut el.el_chared.c_undo.buf);
    el.el_chared.c_undo.len = cur_len;
    el.el_chared.c_undo.cursor = cur_cursor;
    el.el_line.cursor = un_cursor as usize;
    el.el_line.lastchar = un_len as usize;
    // C: `el_line.limit = un.buf + (el_line.limit - el_line.buffer)`. `limit`
    // is an offset here, so re-basing it onto the other allocation leaves the
    // same number; the assignment is a no-op and is not written out. It is
    // only sound at all because `ch_enlargebufs` grows the line and undo
    // allocations together and keeps them the same size.

    // ERR-modes-43, reproduced: `c_kill.mark` and `c_vcmd.pos` still index the
    // buffer that has just become the undo buffer, and `c_redo` is untouched,
    // so `.` still replays the command `u` just reverted.
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-command-mode-fn]
// [spec:libedit:sem:vi.vi-command-mode-fn]
pub(crate) fn vi_command_mode(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // [Esc] cancels pending action.
    el.el_chared.c_vcmd.action = NOP;
    el.el_chared.c_vcmd.pos = 0;

    // Note `el_state.argument` is deliberately *not* reset; the dispatcher
    // does that on return.
    el.el_state.doingarg = 0;

    el.el_state.inputmode = MODE_INSERT;
    // In vi mode `alt` is the command keymap and `key` the insert one, so this
    // is the mode switch. Under emacs `alt` is all `ED_UNASSIGNED`, which is
    // why binding this function there makes the keyboard inert.
    el.el_map.current = ElMapCurrent::Alt;
    // `VI_MOVE` is defined in `chared.h`, so this is live: leaving insert mode
    // moves the cursor left onto the last character typed.
    if el.el_line.cursor > 0 {
        el.el_line.cursor -= 1;
    }
    CC_CURSOR
}

// [spec:libedit:def:vi.vi-zero-fn]
// [spec:libedit:sem:vi.vi-zero-fn]
pub(crate) fn vi_zero(el: &mut EditLine, c: u32) -> ElActionT {
    // `0` is overloaded: the digit while a count is being entered, the
    // "go to column 0" motion otherwise. `ed_argument_digit` returns
    // `CC_ARGHACK`, which keeps the count and any pending operator alive.
    if el.el_state.doingarg != 0 {
        return ed_argument_digit(el, c);
    }

    el.el_line.cursor = 0;
    // The landing position is at or left of `c_vcmd.pos`, so `cv_delfini`
    // takes the negative branch: `d0` deletes `[buffer, c_vcmd.pos)`,
    // exclusive of the character under the cursor. No error path: `0` on an
    // empty line simply stays at `buffer`.
    end_motion(el)
}

// [spec:libedit:def:vi.vi-delete-prev-char-fn]
// [spec:libedit:sem:vi.vi-delete-prev-char-fn]
pub(crate) fn vi_delete_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_line.cursor == 0 {
        return CC_ERROR;
    }

    // The no-yank variant: no kill buffer write and no undo snapshot, unlike
    // `c_delbefore`. Exactly one character per invocation — the count is
    // ignored.
    c_delbefore1(el);
    el.el_line.cursor -= 1;
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-list-or-eof-fn]
// [spec:libedit:sem:vi.vi-list-or-eof-fn]
pub(crate) fn vi_list_or_eof(el: &mut EditLine, c: u32) -> ElActionT {
    // Despite the C's `/*ARGSUSED*/`, `c` *is* used: it is echoed on the EOF
    // path.
    if el.el_line.cursor == el.el_line.lastchar {
        if el.el_line.cursor == 0 {
            // Echo the character in its visual form (`^D`) and flush, then
            // hand end-of-input to the dispatcher.
            terminal_writec(el, c);
            return CC_EOF;
        }
        // Here we could list completions, but it is an error right now.
        // ERR-modes-52: the dispatcher beeps again for `CC_ERROR`, so this
        // path beeps twice.
        terminal_beep(el);
        return CC_ERROR;
    }
    // ERR-modes-71: the mid-line branch's completion listing is behind
    // `#ifdef notyet` and is not compiled, so it is not ported. What is left
    // is behaviourally identical to the branch above.
    terminal_beep(el);
    CC_ERROR
}

// [spec:libedit:def:vi.vi-kill-line-prev-fn]
// [spec:libedit:sem:vi.vi-kill-line-prev-fn]
pub(crate) fn vi_kill_line_prev(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. `[buffer, cursor)` copied into the kill buffer by hand, not through
    //    `cv_yank`. The C bounds this by nothing; `ch_enlargebufs` keeps the
    //    kill and line buffers the same size, so the `min` is that invariant
    //    written down rather than a behaviour change.
    let n = el.el_line.cursor.min(el.el_chared.c_kill.buf.len());
    el.el_chared.c_kill.buf[..n].copy_from_slice(&el.el_line.buffer[..n]);
    el.el_chared.c_kill.last = n;

    // 2. ERR-modes-19 and ERR-modes-47: `c_delbefore`'s `current != emacs`
    //    test is a tautology, so it always snapshots undo and re-yanks the
    //    same span over the copy just made. That is what makes `^U` undoable.
    c_delbefore(el, el.el_line.cursor as i32);

    // 3. Zap.
    el.el_line.cursor = 0;
    // No error path: at `buffer` this deletes nothing, empties the kill
    // buffer, still snapshots undo and still returns `CC_REFRESH`.
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-search-prev-fn]
// [spec:libedit:sem:vi.vi-search-prev-fn]
pub(crate) fn vi_search_prev(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // ERR-modes-67: this is the key bound to `/`, and it searches *backward*
    // through history (towards older entries) with the prompt `"\n/"`.
    cv_search(el, i32::from(ED_SEARCH_PREV_HISTORY))
}

// [spec:libedit:def:vi.vi-search-next-fn]
// [spec:libedit:sem:vi.vi-search-next-fn]
pub(crate) fn vi_search_next(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // ERR-modes-67: bound to `?`, searching *forward* through history.
    cv_search(el, i32::from(ED_SEARCH_NEXT_HISTORY))
}

// [spec:libedit:def:vi.vi-repeat-search-next-fn]
// [spec:libedit:sem:vi.vi-repeat-search-next-fn]
pub(crate) fn vi_repeat_search_next(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_search.patlen == 0 {
        return CC_ERROR;
    }
    // ERR-modes-68: the count is ignored, so `3n` performs one search.
    // `cv_repeat_srch` truncates the line before searching, so a failure
    // leaves it empty.
    cv_repeat_srch(el, el.el_search.patdir as u32)
}

// [spec:libedit:def:vi.vi-repeat-search-prev-fn]
// [spec:libedit:sem:vi.vi-repeat-search-prev-fn]
pub(crate) fn vi_repeat_search_prev(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    if el.el_search.patlen == 0 {
        return CC_ERROR;
    }
    // `patdir` is deliberately not updated by `cv_repeat_srch`, so repeated
    // `N` keeps going the same way rather than alternating.
    let d = if el.el_search.patdir == i32::from(ED_SEARCH_PREV_HISTORY) {
        ED_SEARCH_NEXT_HISTORY
    } else {
        ED_SEARCH_PREV_HISTORY
    };
    cv_repeat_srch(el, u32::from(d))
}

// [spec:libedit:def:vi.vi-next-char-fn]
// [spec:libedit:sem:vi.vi-next-char-fn]
pub(crate) fn vi_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // `(wint_t)-1` is the "prompt for the target now" sentinel; `tflag = 0`
    // lands on the match. A pending operator makes this inclusive of the
    // target, because the direction is positive.
    cv_csearch(el, CHAR_FWD, u32::MAX, el.el_state.argument, 0)
}

// [spec:libedit:def:vi.vi-prev-char-fn]
// [spec:libedit:sem:vi.vi-prev-char-fn]
pub(crate) fn vi_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // Negative direction, so no `cursor++`: `dF<ch>` deletes
    // `[match, original_cursor)`.
    cv_csearch(el, CHAR_BACK, u32::MAX, el.el_state.argument, 0)
}

// [spec:libedit:def:vi.vi-to-next-char-fn]
// [spec:libedit:sem:vi.vi-to-next-char-fn]
pub(crate) fn vi_to_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // `tflag = 1` backs the landing position off by one against the direction.
    // ERR-modes-64: a following `;` re-finds the same occurrence and does not
    // move, because the "skip an adjacent match" test only fires when the
    // cursor is *on* the target, which `t` never leaves it.
    cv_csearch(el, CHAR_FWD, u32::MAX, el.el_state.argument, 1)
}

// [spec:libedit:def:vi.vi-to-prev-char-fn]
// [spec:libedit:sem:vi.vi-to-prev-char-fn]
pub(crate) fn vi_to_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    cv_csearch(el, CHAR_BACK, u32::MAX, el.el_state.argument, 1)
}

// [spec:libedit:def:vi.vi-repeat-next-char-fn]
// [spec:libedit:sem:vi.vi-repeat-next-char-fn]
pub(crate) fn vi_repeat_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // The stored direction, target and "till" flag, but a fresh count. The
    // helper rewrites the same three values before scanning, so `;` is
    // idempotent with respect to them, and answers `CC_ERROR` when `chacha` is
    // still 0 — no character search has been performed yet.
    cv_csearch(
        el,
        el.el_search.chadir,
        el.el_search.chacha,
        el.el_state.argument,
        i32::from(el.el_search.chatflg),
    )
}

// [spec:libedit:def:vi.vi-repeat-prev-char-fn]
// [spec:libedit:sem:vi.vi-repeat-prev-char-fn]
pub(crate) fn vi_repeat_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    let dir = el.el_search.chadir;
    let r = cv_csearch(
        el,
        -dir,
        el.el_search.chacha,
        el.el_state.argument,
        i32::from(el.el_search.chatflg),
    );
    // Undo the helper's overwrite of `chadir`, on the error paths too, so a
    // following `;` still goes the original way and repeated `,` does not
    // alternate.
    el.el_search.chadir = dir;
    r
}

// [spec:libedit:def:vi.vi-match-fn]
// [spec:libedit:sem:vi.vi-match-fn]
pub(crate) fn vi_match(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // C: `const wchar_t match_chars[] = L"()[]{}"`. That is the whole bracket
    // alphabet — no quotes, no comment awareness.
    const MATCH_CHARS: [u32; 6] = [
        b'(' as u32,
        b')' as u32,
        b'[' as u32,
        b']' as u32,
        b'{' as u32,
        b'}' as u32,
    ];

    // 1. NUL-terminate for the `wcs*` calls that follow. `lastchar` is at most
    //    `limit`, which leaves two spare slots, so the slot exists; the guard
    //    is a defined answer for a state the buffer invariants forbid.
    if let Some(slot) = el.el_line.buffer.get_mut(el.el_line.lastchar) {
        *slot = 0;
    }

    // 2. `wcscspn(cursor, match_chars)` — the first bracket at or after the
    //    cursor. ERR-modes-66: the search only ever looks *forward*, even when
    //    the bracket it finds is a closing one.
    let mut i = el.el_line.cursor;
    let found = loop {
        match el.el_line.buffer.get(i).copied() {
            None | Some(0) => break None,
            Some(ch) => match MATCH_CHARS.iter().position(|&m| m == ch) {
                Some(k) => break Some((i, ch, k)),
                None => i += 1,
            },
        }
    };
    let Some((at, o_ch, k)) = found else {
        return CC_ERROR;
    };

    // 3. The partner pairs 0<->1, 2<->3, 4<->5, and the scan runs forward from
    //    an opening bracket (even index) and backward from a closing one.
    let c_ch = MATCH_CHARS[k ^ 1];
    let mut count = 1usize;
    let delta: isize = if k % 2 == 0 { 1 } else { -1 };

    // 4. Scan with proper nesting. Running off either end is `CC_ERROR` with
    //    the cursor and the line untouched.
    let last = el.el_line.lastchar as isize;
    let mut cp = at as isize;
    while count != 0 {
        cp += delta;
        if cp < 0 || cp >= last {
            return CC_ERROR;
        }
        let ch = el.el_line.buffer[cp as usize];
        if ch == o_ch {
            count += 1;
        } else if ch == c_ch {
            count -= 1;
        }
    }

    // 5.
    el.el_line.cursor = cp as usize;

    // 6. The C reads:
    //
    //        if (delta > 0)
    //                el->el_line.cursor++;
    //
    //    with `delta` declared `size_t`. The backward direction is produced by
    //    `delta = 1 - (delta & 1) * 2`, which in `size_t` arithmetic is
    //    `SIZE_MAX`, not -1 — so `delta > 0` is **always true** and the
    //    increment is unconditional (ERR-modes-73). Measured, not inferred:
    //    `1 - (delta & 1) * 2` compiles to 18446744073709551615 for an odd
    //    `delta` on LP64, and `cp += delta` still steps backward because the
    //    pointer arithmetic converts it to -1.
    //
    //    ERR-modes-66 is the consequence: for a backward match the range ends
    //    up `[matched_open + 1, c_vcmd.pos)`, so the matched opening bracket
    //    is *not* deleted — which is what the C's own comment says POSIX
    //    wants, but by accident of the type rather than by the dead guard it
    //    annotates. The range is anchored at `c_vcmd.pos`, not at the bracket,
    //    so anything between the anchor and the bracket is swept in too.
    //    ERR-modes-68: `3%` is `%`.
    if el.el_chared.c_vcmd.action != NOP {
        el.el_line.cursor += 1;
    }
    end_motion(el)
}

// [spec:libedit:def:vi.vi-undo-line-fn]
// [spec:libedit:sem:vi.vi-undo-line-fn]
pub(crate) fn vi_undo_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // Snapshot the edited line first, so a following `u` can get back to it —
    // including on the failure path, where the undo buffer has still been
    // overwritten. ERR-modes-62: this reloads whatever history event is
    // currently selected, where real vi's `U` restores the line as first
    // entered.
    cv_undo(el);
    hist_get(el)
}

// [spec:libedit:def:vi.vi-to-column-fn]
// [spec:libedit:sem:vi.vi-to-column-fn]
pub(crate) fn vi_to_column(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_line.cursor = 0;
    // 1-based column to 0-based offset. The dispatcher resets `argument` to 1
    // on return, so mutating the shared count is not observable afterwards.
    el.el_state.argument -= 1;
    // ERR-modes-63: `n|` counts *characters*, POSIX's definition, where NetBSD
    // vi goes to screen column n. ERR-modes-50: `ed_next_char` clamps to
    // `lastchar` rather than `lastchar - 1`, so `999|` parks the cursor one
    // past the last character.
    ed_next_char(el, 0)
}

// [spec:libedit:def:vi.vi-yank-end-fn]
// [spec:libedit:sem:vi.vi-yank-end-fn]
pub(crate) fn vi_yank_end(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // ERR-modes-59: `[cursor, lastchar)` only — libedit's `Y` is `y$`, where
    // real vi's yanks the whole line. Nothing is deleted, the cursor does not
    // move and no undo snapshot is taken. ERR-modes-68: the count is ignored.
    let size = (el.el_line.lastchar - el.el_line.cursor) as i32;
    cv_yank(el, el.el_line.cursor, size);
    // No error path: at `lastchar` this yanks zero characters, which leaves an
    // empty kill buffer and so makes the next `p`/`P` fail.
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-yank-fn]
// [spec:libedit:sem:vi.vi-yank-fn]
pub(crate) fn vi_yank(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;
    // Because `YANK` is set, `cv_delfini` copies the spanned text and leaves
    // the line alone, but still resets the cursor to `c_vcmd.pos` — so `yw`
    // leaves the cursor where `y` was pressed.
    cv_action(el, YANK as u32)
}

// [spec:libedit:def:vi.vi-comment-out-fn]
// [spec:libedit:sem:vi.vi-comment-out-fn]
pub(crate) fn vi_comment_out(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    el.el_line.cursor = 0;
    // ERR-modes-24, reproduced: if the buffers cannot be grown `c_insert` does
    // nothing and the store below *overwrites* the first character instead of
    // inserting before it. There is no check.
    c_insert(el, 1);
    if let Some(slot) = el.el_line.buffer.get_mut(el.el_line.cursor) {
        *slot = u32::from(b'#');
    }
    // Redraw explicitly so the `#` is visible before the line scrolls away.
    re_refresh(el);
    // No undo snapshot is taken, so `u` cannot recover from `#`. `ed_newline`
    // returns `CC_NEWLINE`, so the line is always accepted.
    ed_newline(el, 0)
}

// [spec:libedit:def:vi.vi-alias-fn]
// [spec:libedit:sem:vi.vi-alias-fn]
pub(crate) fn vi_alias(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. No `el_set(EL_ALIAS_TEXT)` hook installed.
    let Some(aliasfun) = el.el_chared.c_aliasfun else {
        return CC_ERROR;
    };

    // 2/3. `name` is `"_?"`; `el_getc` has already stored `'\0'` into the
    //      middle byte on any non-1 result.
    let mut alias_name: [c_char; 3] = [b'_' as c_char, 0, 0];
    let mut byte: u8 = 0;
    if el_getc(el, &mut byte) != 1 {
        return CC_ERROR;
    }
    alias_name[1] = byte as c_char;

    // 4.
    let aliasarg = el.el_chared.c_aliasarg;
    // SAFETY: `aliasfun` and `aliasarg` were installed together by
    // `ch_aliasfun` from `el_set(EL_ALIAS_TEXT, f, a)`, whose contract
    // (`def:chared.el-afunc-t-void-const-char`) is a C function taking that
    // cookie and a NUL-terminated narrow name. `alias_name` is the
    // three-element local above, NUL-terminated and live across the call.
    let alias_text = unsafe { aliasfun(aliasarg, alias_name.as_ptr()) };

    // 5. Decode from the locale's multibyte encoding and push onto the macro
    //    stack so the expansion is re-read as input. `ct_decode_string` yields
    //    `None` on an invalid sequence, and `el_wpush` answers a `None` string
    //    — or a macro nesting overflow — by beeping and flushing.
    if !alias_text.is_null() {
        // SAFETY: `el_afunc_t`'s contract is a NUL-terminated narrow string
        // the hook owns; this reads it and retains nothing.
        let bytes = unsafe { CStr::from_ptr(alias_text) }.to_bytes().to_vec();
        let decoded = ct_decode_string(Some(&bytes), &mut el.el_scratch).map(<[u32]>::to_vec);
        el_wpush(el, decoded.as_deref());
    }

    // 6. `CC_NORM` unconditionally, including for an unknown alias (silently
    //    ignored, not an error) and for a failed push. ERR-modes-65: the
    //    keymap is deliberately not switched to insert mode, against POSIX.
    CC_NORM
}

// [spec:libedit:def:vi.vi-to-history-line-fn]
// [spec:libedit:sem:vi.vi-to-history-line-fn]
pub(crate) fn vi_to_history_line(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1.
    let sv_event_no = el.el_history.eventno;

    // 2. Still editing the fresh, unsaved line: stash it so it can be returned
    //    to. ERR-history-30, reproduced: the copy length is the compile-time
    //    `EL_BUFSIZ` and not `el_history.sz`, while `last` records the true
    //    length, so a line longer than 1024 comes back with a stale tail. A
    //    sibling — `hist_get`'s `eventno == 0` branch — reads `last` and
    //    depends on exactly this.
    if el.el_history.eventno == 0 {
        // `wcsncpy(dst, src, n)`: copy up to the source's NUL, then NUL-pad
        // out to `n`. The source is the line buffer, which carries no
        // terminator at `lastchar`, so this reads on through whatever is above
        // it — stale but in-allocation data, as in the C.
        let n = EL_BUFSIZ
            .min(el.el_history.buf.len())
            .min(el.el_line.buffer.len());
        let copy = el.el_line.buffer[..n]
            .iter()
            .position(|&ch| ch == 0)
            .unwrap_or(n);
        el.el_history.buf[..copy].copy_from_slice(&el.el_line.buffer[..copy]);
        el.el_history.buf[copy..n].fill(0);
        el.el_history.last = el.el_line.lastchar;
    }

    if el.el_state.doingarg == 0 {
        // 3. Lack of a 'count' means oldest, not 1: `hist_get`'s failure path
        //    leaves `eventno` at the last reachable event index.
        el.el_history.eventno = 0x7fff_ffff;
        let _ = hist_get(el);
    } else {
        // 4. This is brain dead: all the rest of this code counts upwards
        //    going into the past, and here we need the count in the other
        //    direction, to match the output of `fc -l`. The first `hist_get`
        //    exists only to populate `el_history.ev`.
        el.el_history.eventno = 1;
        if hist_get(el) == CC_ERROR {
            // `eventno` is left at 1 on this path; it is not restored.
            return CC_ERROR;
        }
        // ERR-history-18: under `NARROW_HISTORY` — the default for every
        // narrow-API and readline-layer application — nothing ever writes the
        // shared cookie, so `ev.num` is 0 and every `nG` derives a negative
        // event and fails below. Reproduced; `hist.rs` records the same.
        // Wrapping arithmetic defines the C's signed overflow for an absurd
        // `num`/`argument` pair rather than panicking in a debug build.
        el.el_history.eventno = 1i32
            .wrapping_add(el.el_history.ev.num)
            .wrapping_sub(el.el_state.argument);
        if el.el_history.eventno < 0 {
            el.el_history.eventno = sv_event_no;
            return CC_ERROR;
        }
    }

    // 5/6. The call that actually loads the line. `hist_get` puts the cursor
    //      at `buffer` because `KSHVI` is defined and the map type is
    //      `MAP_VI`. No undo snapshot, no keymap change, kill buffer
    //      untouched.
    let rval = hist_get(el);
    if rval == CC_ERROR {
        el.el_history.eventno = sv_event_no;
    }
    rval
}

/// C: `mkstemp("/tmp/histedit.XXXXXXXXXX")`.
///
/// `plan/decisions/no-c-ffi.md` bars linking libc and the crate has no
/// temporary-file dependency, so the template is expanded here: ten characters
/// from `mkstemp`'s own alphabet, `O_CREAT|O_EXCL` so an existing name is
/// retried rather than opened, and mode 0600 as `mkstemp` guarantees. The
/// directory is the C's hardcoded `/tmp` — ERR-modes-46, reproduced:
/// `TMPDIR` is not consulted.
///
/// The returned handle is the C's `fd`, and it is the *same* descriptor the
/// read-back later uses, which is ERR-modes-45.
fn mkstemp() -> Option<(File, PathBuf)> {
    const ALPHABET: &[u8; 62] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0u64, |d| d.as_nanos() as u64);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut seed =
        now ^ (u64::from(std::process::id()) << 32) ^ nth.wrapping_mul(0x9e37_79b9_7f4a_7c15);

    // `TMP_MAX` attempts in the C library; a bounded retry is the same
    // contract and cannot hang.
    for _ in 0..1000 {
        let mut name = String::from("histedit.");
        for _ in 0..10 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            name.push(char::from(ALPHABET[((seed >> 33) % 62) as usize]));
        }
        let path = PathBuf::from("/tmp").join(name);
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Some((file, path)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

// [spec:libedit:def:vi.vi-histedit-fn]
// [spec:libedit:sem:vi.vi-histedit-fn]
pub(crate) fn vi_histedit(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. A count selects a history entry to edit first.
    if el.el_state.doingarg != 0 && vi_to_history_line(el, 0) == CC_ERROR {
        return CC_ERROR;
    }

    // 2. Through the environment hook, so a set-uid process reads nothing.
    let editor = el_getenv(el, c"EDITOR").unwrap_or_else(|| OsString::from("vi"));

    // 3. ERR-modes-46: the hardcoded `/tmp` template. Nothing to clean up on
    //    failure.
    let Some((mut file, tempfile)) = mkstemp() else {
        return CC_ERROR;
    };

    // 4/5. C: `wcsncpy(line, buffer, len)`, `line[len] = 0`,
    //      `wcstombs(cp, line, TMP_BUFSIZ - 1)`, `len = strlen(cp)`. The
    //      `wcstombs` return is unchecked: on an unconvertible character it
    //      answers `(size_t)-1` having written a prefix, and because `cp` was
    //      zero-filled the following `strlen` measures exactly that prefix.
    //      Encoding until the first failure or until the byte budget runs out
    //      is that same prefix. The two `el_calloc`s cannot fail here.
    let cs = locale::charset();
    let mut bytes: Vec<u8> = Vec::with_capacity(TMP_BUFSIZ);
    for &wc in &el.el_line.buffer[..el.el_line.lastchar] {
        // `wcstombs` stops at the wide string's own terminator.
        if wc == 0 {
            break;
        }
        let mut scratch = [0u8; locale::MB_LEN_MAX];
        let Some(n) = locale::wcrtomb(cs, wc, &mut scratch) else {
            break;
        };
        if bytes.len() + n > TMP_BUFSIZ - 1 {
            break;
        }
        bytes.extend_from_slice(&scratch[..n]);
    }

    // 6. ERR-modes-46: the C discards both `write` results. `write_all` closes
    //    the short-write half of that gap; a hard error still has nowhere to
    //    go, exactly as in the C.
    let _ = file.write_all(&bytes);
    let _ = file.write_all(b"\n");

    // 7. C: `fork()`, then `execlp` in the child and a `waitpid` spin in the
    //    parent. `Command::status` is that pair: the child inherits the
    //    raw-mode terminal and the three streams, the descriptor above is
    //    close-on-exec so the child does not see it (the C closes it by hand),
    //    and the wait is bounded — ERR-modes-44's infinite `waitpid` spin is
    //    the one part of that entry the disposition says to bound rather than
    //    reproduce. The exit status is still never examined.
    //
    //    The C cannot tell a failed `execlp` from a successful edit: the child
    //    `exit(0)`s and the parent re-reads the unmodified file. `Command`
    //    reports the child's exec failure back to us, which is strictly more
    //    than the C knows, so the split is made explicit: only a failure to
    //    *create* the child takes the C's `fork() == -1` error exit; anything
    //    else is the child's exec failing, and takes the C's path of reading
    //    the file back unchanged and submitting it.
    match Command::new(&editor).arg(&tempfile).status() {
        Ok(_status) => {}
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::OutOfMemory) => {
            drop(file);
            let _ = fs::remove_file(&tempfile);
            return CC_ERROR;
        }
        Err(_) => {}
    }

    // ERR-modes-45, reproduced: the read-back goes through the *original*
    // descriptor, so an editor that saves by writing a new file and renaming
    // leaves this reading the stale original contents.
    let mut buf = vec![0u8; TMP_BUFSIZ - 1];
    let st = if file.seek(SeekFrom::Start(0)).is_err() {
        // C: `lseek`'s result is unchecked, but a failed seek leaves the
        // descriptor at end of file and the following `read` returns 0.
        0
    } else {
        file.read(&mut buf).unwrap_or(0)
    };

    let len = if st > 0 {
        // C: `len = limit - buffer`, i.e. the line's capacity, then
        // `mbstowcs` decodes straight over the line buffer.
        let cap = el.el_line.limit.min(el.el_line.buffer.len());
        // ERR-modes-05 — UB, defined here as the rule directs. The C does not
        // check `mbstowcs` for `(size_t)-1`, so an edited file that is not a
        // valid multibyte string makes `len` `SIZE_MAX`, `buffer[len - 1]` a
        // wild read and `lastchar` `buffer + SIZE_MAX`. A decode failure is
        // treated as an empty result instead.
        let mut len = locale::mbstowcs(cs, &mut el.el_line.buffer[..cap], &buf[..st]).unwrap_or(0);
        if len > 0 && el.el_line.buffer[len - 1] == u32::from(b'\n') {
            len -= 1;
        }
        len
    } else {
        // A read error produces the same answer as an empty file: the line
        // becomes empty.
        0
    };
    el.el_line.cursor = 0;
    el.el_line.lastchar = len;

    // 8. C: `close(fd)`, `unlink(tempfile)`, then `ed_newline`. The edited
    //    text is submitted immediately, not returned to the editor — the C
    //    carries a commented-out `return CC_REFRESH;` showing the
    //    alternative. No undo snapshot is taken and the keymap is unchanged.
    drop(file);
    let _ = fs::remove_file(&tempfile);
    ed_newline(el, 0)
}

// [spec:libedit:def:vi.vi-history-word-fn]
// [spec:libedit:sem:vi.vi-history-word-fn]
pub(crate) fn vi_history_word(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. ERR-modes-61: `_` means "the whole current line" in real vi; this is
    //    libedit's own invention, and why `cc` is documented as a synonym for
    //    `c_`.
    let Some(wp) = hist_first(el) else {
        return CC_ERROR;
    };

    // 2. Word scan splitting on `iswspace` only — no `cv__isword` classes
    //    here, so punctuation is part of the word. With no count the loop runs
    //    to the end and `[wsp, wep)` is the *last* word; with a count `n` it
    //    runs at most `n` times and yields the n-th word from the left. The
    //    count is consumed destructively.
    let cs = locale::charset();
    let mut wsp: Option<usize> = None;
    let mut wep = 0usize;
    let mut p = 0usize;
    loop {
        while p < wp.len() && locale::iswspace(cs, wp[p]) {
            p += 1;
        }
        // C: `if (*wp == 0) break;` — the copy carries no terminator, so the
        // end of the slice is the terminator.
        if p >= wp.len() {
            break;
        }
        wsp = Some(p);
        while p < wp.len() && !locale::iswspace(cs, wp[p]) {
            p += 1;
        }
        wep = p;

        // C: `while ((!doingarg || --argument > 0) && *wp != 0)`. The
        // decrement is short-circuited away when no count was given.
        let counted = if el.el_state.doingarg == 0 {
            true
        } else {
            el.el_state.argument -= 1;
            el.el_state.argument > 0
        };
        if !(counted && p < wp.len()) {
            break;
        }
    }

    // 3. Nothing found, or the line had fewer than `n` words. Nothing has been
    //    modified at this point.
    let Some(wsp) = wsp else {
        return CC_ERROR;
    };
    if el.el_state.doingarg != 0 && el.el_state.argument != 0 {
        return CC_ERROR;
    }

    // 4/5.
    cv_undo(el);
    let len = wep - wsp;

    // 6. `a` positioning: the text is appended after the character under the
    //    cursor, and at end of line the cursor stays put.
    if el.el_line.cursor < el.el_line.lastchar {
        el.el_line.cursor += 1;
    }

    // 7. One extra slot for the separating space.
    c_insert(el, len as i32 + 1);

    // 8. ERR-modes-41, reproduced: `c_insert` advanced `lastchar` by `len + 1`
    //    unconditionally while this loop stops at `limit`, and if `c_insert`
    //    could not grow the buffers it moved `lastchar` not at all and these
    //    writes land on existing text. Neither case is detected.
    let mut cp = el.el_line.cursor;
    let lim = el.el_line.limit.min(el.el_line.buffer.len());
    if cp < lim {
        el.el_line.buffer[cp] = u32::from(b' ');
        cp += 1;
    }
    let mut s = wsp;
    while s < wep && cp < lim {
        el.el_line.buffer[cp] = wp[s];
        cp += 1;
        s += 1;
    }
    // Just past the last character written.
    el.el_line.cursor = cp;

    // 9/10.
    el.el_map.current = ElMapCurrent::Key;
    CC_REFRESH
}

// [spec:libedit:def:vi.vi-redo-fn]
// [spec:libedit:sem:vi.vi-redo-fn]
pub(crate) fn vi_redo(el: &mut EditLine, c: u32) -> ElActionT {
    let _ = c;

    // 1. A count typed on the `.` itself overrides the recorded one.
    let count = el.el_chared.c_redo.count;
    if el.el_state.doingarg == 0 && count != 0 {
        el.el_state.doingarg = 1;
        el.el_state.argument = count;
    }

    // 2. Re-anchor the operator at the current cursor and restore the mask
    //    that was in force when the command ran.
    el.el_chared.c_vcmd.pos = el.el_line.cursor;
    el.el_chared.c_vcmd.action = el.el_chared.c_redo.action;

    // 3. Insert-mode text was recorded, so push it onto the macro stack to be
    //    re-read as input. The push happens *before* the command is invoked,
    //    so a redone command that reads a character itself (`r`) consumes it
    //    from the pushback.
    if el.el_chared.c_redo.pos != 0 {
        if el.el_chared.c_redo.pos + 1 > el.el_chared.c_redo.lim {
            // C: `r->pos = r->lim - 1` — sanity. `saturating_sub` defines the
            // `lim == 0` case the C would express as a pointer before the
            // buffer; `ch_init` never leaves `lim` at 0.
            el.el_chared.c_redo.pos = el.el_chared.c_redo.lim.saturating_sub(1);
        }
        let pos = el.el_chared.c_redo.pos.min(el.el_chared.c_redo.buf.len());
        if let Some(slot) = el.el_chared.c_redo.buf.get_mut(pos) {
            *slot = 0;
        }
        // `el_wpush` copies the string, so handing it a copy is unobservable;
        // it is what lets the push borrow `el` mutably.
        let text = el.el_chared.c_redo.buf[..pos].to_vec();
        el_wpush(el, Some(&text));
    }

    // 4. So that a nested `cv_undo` re-records the same command rather than
    //    `VI_REDO`. This is also why `r->cmd` can never become `VI_REDO` and
    //    the replay cannot recurse.
    el.el_state.thiscmd = el.el_chared.c_redo.cmd;
    el.el_state.thisch = el.el_chared.c_redo.ch;

    // 5. `c_redo.cmd` starts as `ED_UNASSIGNED`, whose handler returns
    //    `CC_ERROR`, so `.` before any undoable command beeps. The C does not
    //    re-validate the index — `el_wgets` bounds-checks `thiscmd` against
    //    `el_map.nfunc` before dispatch — so the miss below is unreachable;
    //    it defines what the C would leave as a call through a garbage
    //    function pointer.
    let cmd = el.el_chared.c_redo.cmd;
    let ch = el.el_chared.c_redo.ch;
    match el.el_map.func.get(cmd as usize).copied() {
        // SAFETY: as at the dispatch in `sem:read.el-wgets-fn` — `f` is one of
        // `EL_FUNC`'s shims or a command registered through
        // `el_set(EL_ADDFN, ...)`, and `def:map.el-func-t-edit-line-wint-t`
        // makes it a C function taking the `EditLine *` it was registered
        // against. `el` is that handle, live and exclusively borrowed here.
        Some(f) => unsafe { f(ptr::from_mut(el), ch) },
        None => CC_ERROR,
    }
}

#[cfg(test)]
mod test;
