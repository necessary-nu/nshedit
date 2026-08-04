//! Ported from `src/vi.c`; rules live in `docs/spec/port/src/vi.md`.

// Every body below is still `todo!()`, so no parameter is read yet. Remove this
// once the function translations land.
#![allow(unused_variables)]

use crate::el::{EditLine, ElActionT};

// [spec:libedit:def:vi.cv-action-fn]
// [spec:libedit:sem:vi.cv-action-fn]
/// C: `static el_action_t cv_action(EditLine *el, wint_t c)`
///
/// `c` is the operator bitmask (`DELETE`, `DELETE|INSERT`, `YANK`), not a
/// keystroke; see `sem:vi.cv-action-fn`.
fn cv_action(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.cv-paste-fn]
// [spec:libedit:sem:vi.cv-paste-fn]
/// C: `static el_action_t cv_paste(EditLine *el, wint_t c)`
///
/// `c` is a boolean: zero pastes after the cursor, non-zero pastes at it.
/// See `sem:vi.cv-paste-fn`.
fn cv_paste(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-paste-next-fn]
// [spec:libedit:sem:vi.vi-paste-next-fn]
pub(crate) fn vi_paste_next(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-paste-prev-fn]
// [spec:libedit:sem:vi.vi-paste-prev-fn]
pub(crate) fn vi_paste_prev(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-prev-big-word-fn]
// [spec:libedit:sem:vi.vi-prev-big-word-fn]
pub(crate) fn vi_prev_big_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-prev-word-fn]
// [spec:libedit:sem:vi.vi-prev-word-fn]
pub(crate) fn vi_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-next-big-word-fn]
// [spec:libedit:sem:vi.vi-next-big-word-fn]
pub(crate) fn vi_next_big_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-next-word-fn]
// [spec:libedit:sem:vi.vi-next-word-fn]
pub(crate) fn vi_next_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-change-case-fn]
// [spec:libedit:sem:vi.vi-change-case-fn]
pub(crate) fn vi_change_case(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-change-meta-fn]
// [spec:libedit:sem:vi.vi-change-meta-fn]
pub(crate) fn vi_change_meta(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-insert-at-bol-fn]
// [spec:libedit:sem:vi.vi-insert-at-bol-fn]
pub(crate) fn vi_insert_at_bol(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-replace-char-fn]
// [spec:libedit:sem:vi.vi-replace-char-fn]
pub(crate) fn vi_replace_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-replace-mode-fn]
// [spec:libedit:sem:vi.vi-replace-mode-fn]
pub(crate) fn vi_replace_mode(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-substitute-char-fn]
// [spec:libedit:sem:vi.vi-substitute-char-fn]
pub(crate) fn vi_substitute_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-substitute-line-fn]
// [spec:libedit:sem:vi.vi-substitute-line-fn]
pub(crate) fn vi_substitute_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-change-to-eol-fn]
// [spec:libedit:sem:vi.vi-change-to-eol-fn]
pub(crate) fn vi_change_to_eol(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-insert-fn]
// [spec:libedit:sem:vi.vi-insert-fn]
pub(crate) fn vi_insert(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-add-fn]
// [spec:libedit:sem:vi.vi-add-fn]
pub(crate) fn vi_add(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-add-at-eol-fn]
// [spec:libedit:sem:vi.vi-add-at-eol-fn]
pub(crate) fn vi_add_at_eol(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-delete-meta-fn]
// [spec:libedit:sem:vi.vi-delete-meta-fn]
pub(crate) fn vi_delete_meta(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-end-big-word-fn]
// [spec:libedit:sem:vi.vi-end-big-word-fn]
pub(crate) fn vi_end_big_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-end-word-fn]
// [spec:libedit:sem:vi.vi-end-word-fn]
pub(crate) fn vi_end_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-undo-fn]
// [spec:libedit:sem:vi.vi-undo-fn]
pub(crate) fn vi_undo(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-command-mode-fn]
// [spec:libedit:sem:vi.vi-command-mode-fn]
pub(crate) fn vi_command_mode(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-zero-fn]
// [spec:libedit:sem:vi.vi-zero-fn]
pub(crate) fn vi_zero(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-delete-prev-char-fn]
// [spec:libedit:sem:vi.vi-delete-prev-char-fn]
pub(crate) fn vi_delete_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-list-or-eof-fn]
// [spec:libedit:sem:vi.vi-list-or-eof-fn]
pub(crate) fn vi_list_or_eof(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-kill-line-prev-fn]
// [spec:libedit:sem:vi.vi-kill-line-prev-fn]
pub(crate) fn vi_kill_line_prev(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-search-prev-fn]
// [spec:libedit:sem:vi.vi-search-prev-fn]
pub(crate) fn vi_search_prev(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-search-next-fn]
// [spec:libedit:sem:vi.vi-search-next-fn]
pub(crate) fn vi_search_next(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-repeat-search-next-fn]
// [spec:libedit:sem:vi.vi-repeat-search-next-fn]
pub(crate) fn vi_repeat_search_next(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-repeat-search-prev-fn]
// [spec:libedit:sem:vi.vi-repeat-search-prev-fn]
pub(crate) fn vi_repeat_search_prev(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-next-char-fn]
// [spec:libedit:sem:vi.vi-next-char-fn]
pub(crate) fn vi_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-prev-char-fn]
// [spec:libedit:sem:vi.vi-prev-char-fn]
pub(crate) fn vi_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-to-next-char-fn]
// [spec:libedit:sem:vi.vi-to-next-char-fn]
pub(crate) fn vi_to_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-to-prev-char-fn]
// [spec:libedit:sem:vi.vi-to-prev-char-fn]
pub(crate) fn vi_to_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-repeat-next-char-fn]
// [spec:libedit:sem:vi.vi-repeat-next-char-fn]
pub(crate) fn vi_repeat_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-repeat-prev-char-fn]
// [spec:libedit:sem:vi.vi-repeat-prev-char-fn]
pub(crate) fn vi_repeat_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-match-fn]
// [spec:libedit:sem:vi.vi-match-fn]
pub(crate) fn vi_match(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-undo-line-fn]
// [spec:libedit:sem:vi.vi-undo-line-fn]
pub(crate) fn vi_undo_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-to-column-fn]
// [spec:libedit:sem:vi.vi-to-column-fn]
pub(crate) fn vi_to_column(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-yank-end-fn]
// [spec:libedit:sem:vi.vi-yank-end-fn]
pub(crate) fn vi_yank_end(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-yank-fn]
// [spec:libedit:sem:vi.vi-yank-fn]
pub(crate) fn vi_yank(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-comment-out-fn]
// [spec:libedit:sem:vi.vi-comment-out-fn]
pub(crate) fn vi_comment_out(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-alias-fn]
// [spec:libedit:sem:vi.vi-alias-fn]
pub(crate) fn vi_alias(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-to-history-line-fn]
// [spec:libedit:sem:vi.vi-to-history-line-fn]
pub(crate) fn vi_to_history_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-histedit-fn]
// [spec:libedit:sem:vi.vi-histedit-fn]
pub(crate) fn vi_histedit(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-history-word-fn]
// [spec:libedit:sem:vi.vi-history-word-fn]
pub(crate) fn vi_history_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:vi.vi-redo-fn]
// [spec:libedit:sem:vi.vi-redo-fn]
pub(crate) fn vi_redo(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}
