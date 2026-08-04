//! Ported from `src/emacs.c`; rules live in `docs/spec/port/src/emacs.md`.

// Every body below is still `todo!()`, so no parameter is read yet. Remove this
// once the function translations land.
#![allow(unused_variables)]

use crate::el::{EditLine, ElActionT};

// [spec:libedit:def:emacs.em-delete-or-list-fn]
// [spec:libedit:sem:emacs.em-delete-or-list-fn]
pub(crate) fn em_delete_or_list(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-delete-next-word-fn]
// [spec:libedit:sem:emacs.em-delete-next-word-fn]
pub(crate) fn em_delete_next_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-yank-fn]
// [spec:libedit:sem:emacs.em-yank-fn]
pub(crate) fn em_yank(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-kill-line-fn]
// [spec:libedit:sem:emacs.em-kill-line-fn]
pub(crate) fn em_kill_line(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-kill-region-fn]
// [spec:libedit:sem:emacs.em-kill-region-fn]
pub(crate) fn em_kill_region(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-copy-region-fn]
// [spec:libedit:sem:emacs.em-copy-region-fn]
pub(crate) fn em_copy_region(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-gosmacs-transpose-fn]
// [spec:libedit:sem:emacs.em-gosmacs-transpose-fn]
pub(crate) fn em_gosmacs_transpose(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-next-word-fn]
// [spec:libedit:sem:emacs.em-next-word-fn]
pub(crate) fn em_next_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-upper-case-fn]
// [spec:libedit:sem:emacs.em-upper-case-fn]
pub(crate) fn em_upper_case(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-capitol-case-fn]
// [spec:libedit:sem:emacs.em-capitol-case-fn]
pub(crate) fn em_capitol_case(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-lower-case-fn]
// [spec:libedit:sem:emacs.em-lower-case-fn]
pub(crate) fn em_lower_case(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-set-mark-fn]
// [spec:libedit:sem:emacs.em-set-mark-fn]
pub(crate) fn em_set_mark(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-exchange-mark-fn]
// [spec:libedit:sem:emacs.em-exchange-mark-fn]
pub(crate) fn em_exchange_mark(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-universal-argument-fn]
// [spec:libedit:sem:emacs.em-universal-argument-fn]
pub(crate) fn em_universal_argument(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-meta-next-fn]
// [spec:libedit:sem:emacs.em-meta-next-fn]
pub(crate) fn em_meta_next(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-toggle-overwrite-fn]
// [spec:libedit:sem:emacs.em-toggle-overwrite-fn]
pub(crate) fn em_toggle_overwrite(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-copy-prev-word-fn]
// [spec:libedit:sem:emacs.em-copy-prev-word-fn]
pub(crate) fn em_copy_prev_word(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-inc-search-next-fn]
// [spec:libedit:sem:emacs.em-inc-search-next-fn]
pub(crate) fn em_inc_search_next(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-inc-search-prev-fn]
// [spec:libedit:sem:emacs.em-inc-search-prev-fn]
pub(crate) fn em_inc_search_prev(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}

// [spec:libedit:def:emacs.em-delete-prev-char-fn]
// [spec:libedit:sem:emacs.em-delete-prev-char-fn]
pub(crate) fn em_delete_prev_char(el: &mut EditLine, c: u32) -> ElActionT {
    todo!()
}
