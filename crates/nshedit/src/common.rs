//! Ported from `src/common.c`; rules live in `docs/spec/port/src/common.md`.

// Every body below is still `todo!()`, so no parameter is read yet. Remove this
// once the function translations land.
#![allow(unused_variables)]

use crate::el::{EditLine, ElActionT};

// [spec:libedit:def:common.ed-end-of-file-fn]
// [spec:libedit:sem:common.ed-end-of-file-fn]
pub(crate) fn ed_end_of_file(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-insert-fn]
// [spec:libedit:sem:common.ed-insert-fn]
pub(crate) fn ed_insert(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-delete-prev-word-fn]
// [spec:libedit:sem:common.ed-delete-prev-word-fn]
pub(crate) fn ed_delete_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-delete-next-char-fn]
// [spec:libedit:sem:common.ed-delete-next-char-fn]
pub(crate) fn ed_delete_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-kill-line-fn]
// [spec:libedit:sem:common.ed-kill-line-fn]
pub(crate) fn ed_kill_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-move-to-end-fn]
// [spec:libedit:sem:common.ed-move-to-end-fn]
pub(crate) fn ed_move_to_end(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-move-to-beg-fn]
// [spec:libedit:sem:common.ed-move-to-beg-fn]
pub(crate) fn ed_move_to_beg(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-transpose-chars-fn]
// [spec:libedit:sem:common.ed-transpose-chars-fn]
pub(crate) fn ed_transpose_chars(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-next-char-fn]
// [spec:libedit:sem:common.ed-next-char-fn]
pub(crate) fn ed_next_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-prev-word-fn]
// [spec:libedit:sem:common.ed-prev-word-fn]
pub(crate) fn ed_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-prev-char-fn]
// [spec:libedit:sem:common.ed-prev-char-fn]
pub(crate) fn ed_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-quoted-insert-fn]
// [spec:libedit:sem:common.ed-quoted-insert-fn]
pub(crate) fn ed_quoted_insert(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-digit-fn]
// [spec:libedit:sem:common.ed-digit-fn]
pub(crate) fn ed_digit(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-argument-digit-fn]
// [spec:libedit:sem:common.ed-argument-digit-fn]
pub(crate) fn ed_argument_digit(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-unassigned-fn]
// [spec:libedit:sem:common.ed-unassigned-fn]
pub(crate) fn ed_unassigned(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-ignore-fn]
// [spec:libedit:sem:common.ed-ignore-fn]
pub(crate) fn ed_ignore(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-newline-fn]
// [spec:libedit:sem:common.ed-newline-fn]
pub(crate) fn ed_newline(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-delete-prev-char-fn]
// [spec:libedit:sem:common.ed-delete-prev-char-fn]
pub(crate) fn ed_delete_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-clear-screen-fn]
// [spec:libedit:sem:common.ed-clear-screen-fn]
pub(crate) fn ed_clear_screen(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-redisplay-fn]
// [spec:libedit:sem:common.ed-redisplay-fn]
pub(crate) fn ed_redisplay(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-start-over-fn]
// [spec:libedit:sem:common.ed-start-over-fn]
pub(crate) fn ed_start_over(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-sequence-lead-in-fn]
// [spec:libedit:sem:common.ed-sequence-lead-in-fn]
pub(crate) fn ed_sequence_lead_in(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-prev-history-fn]
// [spec:libedit:sem:common.ed-prev-history-fn]
pub(crate) fn ed_prev_history(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-next-history-fn]
// [spec:libedit:sem:common.ed-next-history-fn]
pub(crate) fn ed_next_history(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-search-prev-history-fn]
// [spec:libedit:sem:common.ed-search-prev-history-fn]
pub(crate) fn ed_search_prev_history(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-search-next-history-fn]
// [spec:libedit:sem:common.ed-search-next-history-fn]
pub(crate) fn ed_search_next_history(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-prev-line-fn]
// [spec:libedit:sem:common.ed-prev-line-fn]
pub(crate) fn ed_prev_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-next-line-fn]
// [spec:libedit:sem:common.ed-next-line-fn]
pub(crate) fn ed_next_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:common.ed-command-fn]
// [spec:libedit:sem:common.ed-command-fn]
pub(crate) fn ed_command(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}
