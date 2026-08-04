//! The generated command table: `src/fcns.h`, `src/func.h` and `src/help.h`.
//!
//! These three headers have no checked-in source. `src/makelist` — a shell
//! script driving `awk` — scans the `/* name(): */` doc comments of `vi.c`,
//! `emacs.c` and `common.c` at build time and emits them, so the command
//! numbering is a property of those comments and of nothing else. This module
//! is the port's copy of that output, and `sem:map.map-init-fn` requires it to
//! stay identical: the numbers, the `EL_FUNC` ordering and the `EL_FUNC_HELP`
//! *source* ordering are all observable across the C ABI, through `EL_ADDFN`
//! numbering and through the ordering of `bind -l` output.
//!
//! # Two orderings, and only one of them is the numbering
//!
//! - [`EL_FUNC`] and the `const`s below come from `fcns.h`/`func.h`, which are
//!   **sorted by name**. Index equals command number: `EL_FUNC[N]` is the
//!   handler for command `N`, and that correspondence is the whole numbering
//!   scheme.
//! - [`EL_FUNC_HELP`] comes from `help.h`, which is in **source order** — all
//!   of `vi.c`, then `emacs.c`, then `common.c`. Its index is unrelated to the
//!   command number, so every lookup must compare the `func` field and must
//!   never index by it.
//!
//! # Regenerating
//!
//! Run the real generator; do not hand-edit this file and do not transcribe
//! from memory. From the repository root:
//!
//! ```sh
//! cd "$(mktemp -d)"
//! S=/path/to/libedit/src
//! for m in vi emacs common; do AWK=awk sh $S/makelist -h $S/$m.c > $m.h; done
//! AWK=awk sh $S/makelist -fh vi.h emacs.h common.h   # the `const`s
//! AWK=awk sh $S/makelist -fc vi.h emacs.h common.h   # EL_FUNC
//! AWK=awk sh $S/makelist -bh $S/vi.c $S/emacs.c $S/common.c   # EL_FUNC_HELP
//! ```
//!
//! Then transcribe mechanically: `#define NAME n` becomes `const NAME:
//! ElActionT = n`, `EL_NUM_FCNS` becomes [`EL_NUM_FCNS`], the `el_func[]`
//! entries become paths to the same function names in [`crate::vi`],
//! [`crate::emacs`] and [`crate::common`] (the module is the `.c` file the
//! prototype came from), and each `el_func_help[]` row becomes an
//! [`ElBindingsT`] whose `L"..."` strings become `&'static [u32]`. Finish with
//! `cargo fmt -p nshedit`, which is the only thing that touches this file by
//! hand-shaped rules rather than by transcription.
//!
//! Regenerated from libedit 20260512-3.1: 96 commands, numbered
//! 0..95.

use std::borrow::Cow;

use crate::el::ElActionT;
use crate::map::{ElBindingsT, ElFuncT};

/// C: `L"..."` — a wide string literal as the crate carries them. ASCII only,
/// which every generated name and description is.
const fn wide<const N: usize>(s: &[u8; N]) -> [u32; N] {
    let mut out = [0u32; N];
    let mut i = 0;
    while i < N {
        out[i] = s[i] as u32;
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// `fcns.h`: the command numbers, sorted by name and numbered from zero.
// ---------------------------------------------------------------------------

pub(crate) const ED_ARGUMENT_DIGIT: ElActionT = 0;
pub(crate) const ED_CLEAR_SCREEN: ElActionT = 1;
pub(crate) const ED_COMMAND: ElActionT = 2;
pub(crate) const ED_DELETE_NEXT_CHAR: ElActionT = 3;
pub(crate) const ED_DELETE_PREV_CHAR: ElActionT = 4;
pub(crate) const ED_DELETE_PREV_WORD: ElActionT = 5;
pub(crate) const ED_DIGIT: ElActionT = 6;
pub(crate) const ED_END_OF_FILE: ElActionT = 7;
pub(crate) const ED_IGNORE: ElActionT = 8;
pub(crate) const ED_INSERT: ElActionT = 9;
pub(crate) const ED_KILL_LINE: ElActionT = 10;
pub(crate) const ED_MOVE_TO_BEG: ElActionT = 11;
pub(crate) const ED_MOVE_TO_END: ElActionT = 12;
pub(crate) const ED_NEWLINE: ElActionT = 13;
pub(crate) const ED_NEXT_CHAR: ElActionT = 14;
pub(crate) const ED_NEXT_HISTORY: ElActionT = 15;
pub(crate) const ED_NEXT_LINE: ElActionT = 16;
pub(crate) const ED_PREV_CHAR: ElActionT = 17;
pub(crate) const ED_PREV_HISTORY: ElActionT = 18;
pub(crate) const ED_PREV_LINE: ElActionT = 19;
pub(crate) const ED_PREV_WORD: ElActionT = 20;
pub(crate) const ED_QUOTED_INSERT: ElActionT = 21;
pub(crate) const ED_REDISPLAY: ElActionT = 22;
pub(crate) const ED_SEARCH_NEXT_HISTORY: ElActionT = 23;
pub(crate) const ED_SEARCH_PREV_HISTORY: ElActionT = 24;
pub(crate) const ED_SEQUENCE_LEAD_IN: ElActionT = 25;
pub(crate) const ED_START_OVER: ElActionT = 26;
pub(crate) const ED_TRANSPOSE_CHARS: ElActionT = 27;
pub(crate) const ED_UNASSIGNED: ElActionT = 28;
pub(crate) const EM_CAPITOL_CASE: ElActionT = 29;
pub(crate) const EM_COPY_PREV_WORD: ElActionT = 30;
pub(crate) const EM_COPY_REGION: ElActionT = 31;
pub(crate) const EM_DELETE_NEXT_WORD: ElActionT = 32;
pub(crate) const EM_DELETE_OR_LIST: ElActionT = 33;
pub(crate) const EM_DELETE_PREV_CHAR: ElActionT = 34;
pub(crate) const EM_EXCHANGE_MARK: ElActionT = 35;
pub(crate) const EM_GOSMACS_TRANSPOSE: ElActionT = 36;
pub(crate) const EM_INC_SEARCH_NEXT: ElActionT = 37;
pub(crate) const EM_INC_SEARCH_PREV: ElActionT = 38;
pub(crate) const EM_KILL_LINE: ElActionT = 39;
pub(crate) const EM_KILL_REGION: ElActionT = 40;
pub(crate) const EM_LOWER_CASE: ElActionT = 41;
pub(crate) const EM_META_NEXT: ElActionT = 42;
pub(crate) const EM_NEXT_WORD: ElActionT = 43;
pub(crate) const EM_SET_MARK: ElActionT = 44;
pub(crate) const EM_TOGGLE_OVERWRITE: ElActionT = 45;
pub(crate) const EM_UNIVERSAL_ARGUMENT: ElActionT = 46;
pub(crate) const EM_UPPER_CASE: ElActionT = 47;
pub(crate) const EM_YANK: ElActionT = 48;
pub(crate) const VI_ADD: ElActionT = 49;
pub(crate) const VI_ADD_AT_EOL: ElActionT = 50;
pub(crate) const VI_ALIAS: ElActionT = 51;
pub(crate) const VI_CHANGE_CASE: ElActionT = 52;
pub(crate) const VI_CHANGE_META: ElActionT = 53;
pub(crate) const VI_CHANGE_TO_EOL: ElActionT = 54;
pub(crate) const VI_COMMAND_MODE: ElActionT = 55;
pub(crate) const VI_COMMENT_OUT: ElActionT = 56;
pub(crate) const VI_DELETE_META: ElActionT = 57;
pub(crate) const VI_DELETE_PREV_CHAR: ElActionT = 58;
pub(crate) const VI_END_BIG_WORD: ElActionT = 59;
pub(crate) const VI_END_WORD: ElActionT = 60;
pub(crate) const VI_HISTEDIT: ElActionT = 61;
pub(crate) const VI_HISTORY_WORD: ElActionT = 62;
pub(crate) const VI_INSERT: ElActionT = 63;
pub(crate) const VI_INSERT_AT_BOL: ElActionT = 64;
pub(crate) const VI_KILL_LINE_PREV: ElActionT = 65;
pub(crate) const VI_LIST_OR_EOF: ElActionT = 66;
pub(crate) const VI_MATCH: ElActionT = 67;
pub(crate) const VI_NEXT_BIG_WORD: ElActionT = 68;
pub(crate) const VI_NEXT_CHAR: ElActionT = 69;
pub(crate) const VI_NEXT_WORD: ElActionT = 70;
pub(crate) const VI_PASTE_NEXT: ElActionT = 71;
pub(crate) const VI_PASTE_PREV: ElActionT = 72;
pub(crate) const VI_PREV_BIG_WORD: ElActionT = 73;
pub(crate) const VI_PREV_CHAR: ElActionT = 74;
pub(crate) const VI_PREV_WORD: ElActionT = 75;
pub(crate) const VI_REDO: ElActionT = 76;
pub(crate) const VI_REPEAT_NEXT_CHAR: ElActionT = 77;
pub(crate) const VI_REPEAT_PREV_CHAR: ElActionT = 78;
pub(crate) const VI_REPEAT_SEARCH_NEXT: ElActionT = 79;
pub(crate) const VI_REPEAT_SEARCH_PREV: ElActionT = 80;
pub(crate) const VI_REPLACE_CHAR: ElActionT = 81;
pub(crate) const VI_REPLACE_MODE: ElActionT = 82;
pub(crate) const VI_SEARCH_NEXT: ElActionT = 83;
pub(crate) const VI_SEARCH_PREV: ElActionT = 84;
pub(crate) const VI_SUBSTITUTE_CHAR: ElActionT = 85;
pub(crate) const VI_SUBSTITUTE_LINE: ElActionT = 86;
pub(crate) const VI_TO_COLUMN: ElActionT = 87;
pub(crate) const VI_TO_HISTORY_LINE: ElActionT = 88;
pub(crate) const VI_TO_NEXT_CHAR: ElActionT = 89;
pub(crate) const VI_TO_PREV_CHAR: ElActionT = 90;
pub(crate) const VI_UNDO: ElActionT = 91;
pub(crate) const VI_UNDO_LINE: ElActionT = 92;
pub(crate) const VI_YANK: ElActionT = 93;
pub(crate) const VI_YANK_END: ElActionT = 94;
pub(crate) const VI_ZERO: ElActionT = 95;

/// C: `#define EL_NUM_FCNS 96` — the number of built-in commands, and the
/// initial `el_map.nfunc`. `map_addfunc` appends past it.
pub(crate) const EL_NUM_FCNS: usize = 96;

// ---------------------------------------------------------------------------
// `func.h`: the handlers, in the same sorted order, so the index is the
// command number.
// ---------------------------------------------------------------------------

/// C: `static const el_func_t el_func[]`.
///
/// `EL_FUNC[N]` is the handler for command number `N`; `map_init` copies the
/// whole table into `el_map.func`.
pub(crate) static EL_FUNC: [ElFuncT; EL_NUM_FCNS] = [
    crate::common::ed_argument_digit,
    crate::common::ed_clear_screen,
    crate::common::ed_command,
    crate::common::ed_delete_next_char,
    crate::common::ed_delete_prev_char,
    crate::common::ed_delete_prev_word,
    crate::common::ed_digit,
    crate::common::ed_end_of_file,
    crate::common::ed_ignore,
    crate::common::ed_insert,
    crate::common::ed_kill_line,
    crate::common::ed_move_to_beg,
    crate::common::ed_move_to_end,
    crate::common::ed_newline,
    crate::common::ed_next_char,
    crate::common::ed_next_history,
    crate::common::ed_next_line,
    crate::common::ed_prev_char,
    crate::common::ed_prev_history,
    crate::common::ed_prev_line,
    crate::common::ed_prev_word,
    crate::common::ed_quoted_insert,
    crate::common::ed_redisplay,
    crate::common::ed_search_next_history,
    crate::common::ed_search_prev_history,
    crate::common::ed_sequence_lead_in,
    crate::common::ed_start_over,
    crate::common::ed_transpose_chars,
    crate::common::ed_unassigned,
    crate::emacs::em_capitol_case,
    crate::emacs::em_copy_prev_word,
    crate::emacs::em_copy_region,
    crate::emacs::em_delete_next_word,
    crate::emacs::em_delete_or_list,
    crate::emacs::em_delete_prev_char,
    crate::emacs::em_exchange_mark,
    crate::emacs::em_gosmacs_transpose,
    crate::emacs::em_inc_search_next,
    crate::emacs::em_inc_search_prev,
    crate::emacs::em_kill_line,
    crate::emacs::em_kill_region,
    crate::emacs::em_lower_case,
    crate::emacs::em_meta_next,
    crate::emacs::em_next_word,
    crate::emacs::em_set_mark,
    crate::emacs::em_toggle_overwrite,
    crate::emacs::em_universal_argument,
    crate::emacs::em_upper_case,
    crate::emacs::em_yank,
    crate::vi::vi_add,
    crate::vi::vi_add_at_eol,
    crate::vi::vi_alias,
    crate::vi::vi_change_case,
    crate::vi::vi_change_meta,
    crate::vi::vi_change_to_eol,
    crate::vi::vi_command_mode,
    crate::vi::vi_comment_out,
    crate::vi::vi_delete_meta,
    crate::vi::vi_delete_prev_char,
    crate::vi::vi_end_big_word,
    crate::vi::vi_end_word,
    crate::vi::vi_histedit,
    crate::vi::vi_history_word,
    crate::vi::vi_insert,
    crate::vi::vi_insert_at_bol,
    crate::vi::vi_kill_line_prev,
    crate::vi::vi_list_or_eof,
    crate::vi::vi_match,
    crate::vi::vi_next_big_word,
    crate::vi::vi_next_char,
    crate::vi::vi_next_word,
    crate::vi::vi_paste_next,
    crate::vi::vi_paste_prev,
    crate::vi::vi_prev_big_word,
    crate::vi::vi_prev_char,
    crate::vi::vi_prev_word,
    crate::vi::vi_redo,
    crate::vi::vi_repeat_next_char,
    crate::vi::vi_repeat_prev_char,
    crate::vi::vi_repeat_search_next,
    crate::vi::vi_repeat_search_prev,
    crate::vi::vi_replace_char,
    crate::vi::vi_replace_mode,
    crate::vi::vi_search_next,
    crate::vi::vi_search_prev,
    crate::vi::vi_substitute_char,
    crate::vi::vi_substitute_line,
    crate::vi::vi_to_column,
    crate::vi::vi_to_history_line,
    crate::vi::vi_to_next_char,
    crate::vi::vi_to_prev_char,
    crate::vi::vi_undo,
    crate::vi::vi_undo_line,
    crate::vi::vi_yank,
    crate::vi::vi_yank_end,
    crate::vi::vi_zero,
];

// ---------------------------------------------------------------------------
// `help.h`: the same commands in source order — vi.c, then emacs.c, then
// common.c. The index here is *not* the command number.
// ---------------------------------------------------------------------------

/// C: `static const struct el_bindings_t el_func_help[]`.
///
/// Source order, not sorted: `map_init` copies it wholesale into
/// `el_map.help`, and `parse.c` and `map.c` find a row by scanning for a
/// matching `func`, never by indexing.
pub(crate) static EL_FUNC_HELP: [ElBindingsT; EL_NUM_FCNS] = [
    ElBindingsT {
        func: VI_PASTE_NEXT as i32,
        name: Cow::Borrowed(&const { wide(b"vi-paste-next") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi paste previous deletion to the right of the cursor") },
        ),
    },
    ElBindingsT {
        func: VI_PASTE_PREV as i32,
        name: Cow::Borrowed(&const { wide(b"vi-paste-prev") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi paste previous deletion to the left of the cursor") },
        ),
    },
    ElBindingsT {
        func: VI_PREV_BIG_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-prev-big-word") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi move to the previous space delimited word") },
        ),
    },
    ElBindingsT {
        func: VI_PREV_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-prev-word") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the previous word") }),
    },
    ElBindingsT {
        func: VI_NEXT_BIG_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-next-big-word") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the next space delimited word") }),
    },
    ElBindingsT {
        func: VI_NEXT_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-next-word") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the next word") }),
    },
    ElBindingsT {
        func: VI_CHANGE_CASE as i32,
        name: Cow::Borrowed(&const { wide(b"vi-change-case") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi change case of character under the cursor and advance one character") },
        ),
    },
    ElBindingsT {
        func: VI_CHANGE_META as i32,
        name: Cow::Borrowed(&const { wide(b"vi-change-meta") }),
        description: Cow::Borrowed(&const { wide(b"Vi change prefix command") }),
    },
    ElBindingsT {
        func: VI_INSERT_AT_BOL as i32,
        name: Cow::Borrowed(&const { wide(b"vi-insert-at-bol") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi enter insert mode at the beginning of line") },
        ),
    },
    ElBindingsT {
        func: VI_REPLACE_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-replace-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi replace character under the cursor with the next character typed") },
        ),
    },
    ElBindingsT {
        func: VI_REPLACE_MODE as i32,
        name: Cow::Borrowed(&const { wide(b"vi-replace-mode") }),
        description: Cow::Borrowed(&const { wide(b"Vi enter replace mode") }),
    },
    ElBindingsT {
        func: VI_SUBSTITUTE_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-substitute-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi replace character under the cursor and enter insert mode") },
        ),
    },
    ElBindingsT {
        func: VI_SUBSTITUTE_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"vi-substitute-line") }),
        description: Cow::Borrowed(&const { wide(b"Vi substitute entire line") }),
    },
    ElBindingsT {
        func: VI_CHANGE_TO_EOL as i32,
        name: Cow::Borrowed(&const { wide(b"vi-change-to-eol") }),
        description: Cow::Borrowed(&const { wide(b"Vi change to end of line") }),
    },
    ElBindingsT {
        func: VI_INSERT as i32,
        name: Cow::Borrowed(&const { wide(b"vi-insert") }),
        description: Cow::Borrowed(&const { wide(b"Vi enter insert mode") }),
    },
    ElBindingsT {
        func: VI_ADD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-add") }),
        description: Cow::Borrowed(&const { wide(b"Vi enter insert mode after the cursor") }),
    },
    ElBindingsT {
        func: VI_ADD_AT_EOL as i32,
        name: Cow::Borrowed(&const { wide(b"vi-add-at-eol") }),
        description: Cow::Borrowed(&const { wide(b"Vi enter insert mode at end of line") }),
    },
    ElBindingsT {
        func: VI_DELETE_META as i32,
        name: Cow::Borrowed(&const { wide(b"vi-delete-meta") }),
        description: Cow::Borrowed(&const { wide(b"Vi delete prefix command") }),
    },
    ElBindingsT {
        func: VI_END_BIG_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-end-big-word") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi move to the end of the current space delimited word") },
        ),
    },
    ElBindingsT {
        func: VI_END_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-end-word") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the end of the current word") }),
    },
    ElBindingsT {
        func: VI_UNDO as i32,
        name: Cow::Borrowed(&const { wide(b"vi-undo") }),
        description: Cow::Borrowed(&const { wide(b"Vi undo last change") }),
    },
    ElBindingsT {
        func: VI_COMMAND_MODE as i32,
        name: Cow::Borrowed(&const { wide(b"vi-command-mode") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi enter command mode (use alternative key bindings)") },
        ),
    },
    ElBindingsT {
        func: VI_ZERO as i32,
        name: Cow::Borrowed(&const { wide(b"vi-zero") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the beginning of line") }),
    },
    ElBindingsT {
        func: VI_DELETE_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-delete-prev-char") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to previous character (backspace)") }),
    },
    ElBindingsT {
        func: VI_LIST_OR_EOF as i32,
        name: Cow::Borrowed(&const { wide(b"vi-list-or-eof") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi list choices for completion or indicate end of file if empty line") },
        ),
    },
    ElBindingsT {
        func: VI_KILL_LINE_PREV as i32,
        name: Cow::Borrowed(&const { wide(b"vi-kill-line-prev") }),
        description: Cow::Borrowed(&const { wide(b"Vi cut from beginning of line to cursor") }),
    },
    ElBindingsT {
        func: VI_SEARCH_PREV as i32,
        name: Cow::Borrowed(&const { wide(b"vi-search-prev") }),
        description: Cow::Borrowed(&const { wide(b"Vi search history previous") }),
    },
    ElBindingsT {
        func: VI_SEARCH_NEXT as i32,
        name: Cow::Borrowed(&const { wide(b"vi-search-next") }),
        description: Cow::Borrowed(&const { wide(b"Vi search history next") }),
    },
    ElBindingsT {
        func: VI_REPEAT_SEARCH_NEXT as i32,
        name: Cow::Borrowed(&const { wide(b"vi-repeat-search-next") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi repeat current search in the same search direction") },
        ),
    },
    ElBindingsT {
        func: VI_REPEAT_SEARCH_PREV as i32,
        name: Cow::Borrowed(&const { wide(b"vi-repeat-search-prev") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi repeat current search in the opposite search direction") },
        ),
    },
    ElBindingsT {
        func: VI_NEXT_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-next-char") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the character specified next") }),
    },
    ElBindingsT {
        func: VI_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-prev-char") }),
        description: Cow::Borrowed(&const { wide(b"Vi move to the character specified previous") }),
    },
    ElBindingsT {
        func: VI_TO_NEXT_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-to-next-char") }),
        description: Cow::Borrowed(&const { wide(b"Vi move up to the character specified next") }),
    },
    ElBindingsT {
        func: VI_TO_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-to-prev-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi move up to the character specified previous") },
        ),
    },
    ElBindingsT {
        func: VI_REPEAT_NEXT_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-repeat-next-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi repeat current character search in the same search direction") },
        ),
    },
    ElBindingsT {
        func: VI_REPEAT_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"vi-repeat-prev-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Vi repeat current character search in the opposite search direction") },
        ),
    },
    ElBindingsT {
        func: VI_MATCH as i32,
        name: Cow::Borrowed(&const { wide(b"vi-match") }),
        description: Cow::Borrowed(&const { wide(b"Vi go to matching () {} or []") }),
    },
    ElBindingsT {
        func: VI_UNDO_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"vi-undo-line") }),
        description: Cow::Borrowed(&const { wide(b"Vi undo all changes to line") }),
    },
    ElBindingsT {
        func: VI_TO_COLUMN as i32,
        name: Cow::Borrowed(&const { wide(b"vi-to-column") }),
        description: Cow::Borrowed(&const { wide(b"Vi go to specified column") }),
    },
    ElBindingsT {
        func: VI_YANK_END as i32,
        name: Cow::Borrowed(&const { wide(b"vi-yank-end") }),
        description: Cow::Borrowed(&const { wide(b"Vi yank to end of line") }),
    },
    ElBindingsT {
        func: VI_YANK as i32,
        name: Cow::Borrowed(&const { wide(b"vi-yank") }),
        description: Cow::Borrowed(&const { wide(b"Vi yank") }),
    },
    ElBindingsT {
        func: VI_COMMENT_OUT as i32,
        name: Cow::Borrowed(&const { wide(b"vi-comment-out") }),
        description: Cow::Borrowed(&const { wide(b"Vi comment out current command") }),
    },
    ElBindingsT {
        func: VI_ALIAS as i32,
        name: Cow::Borrowed(&const { wide(b"vi-alias") }),
        description: Cow::Borrowed(&const { wide(b"Vi include shell alias") }),
    },
    ElBindingsT {
        func: VI_TO_HISTORY_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"vi-to-history-line") }),
        description: Cow::Borrowed(&const { wide(b"Vi go to specified history file line.") }),
    },
    ElBindingsT {
        func: VI_HISTEDIT as i32,
        name: Cow::Borrowed(&const { wide(b"vi-histedit") }),
        description: Cow::Borrowed(&const { wide(b"Vi edit history line with vi") }),
    },
    ElBindingsT {
        func: VI_HISTORY_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"vi-history-word") }),
        description: Cow::Borrowed(&const { wide(b"Vi append word from previous input line") }),
    },
    ElBindingsT {
        func: VI_REDO as i32,
        name: Cow::Borrowed(&const { wide(b"vi-redo") }),
        description: Cow::Borrowed(&const { wide(b"Vi redo last non-motion command") }),
    },
    ElBindingsT {
        func: EM_DELETE_OR_LIST as i32,
        name: Cow::Borrowed(&const { wide(b"em-delete-or-list") }),
        description: Cow::Borrowed(
            &const { wide(b"Delete character under cursor or list completions if at end of line") },
        ),
    },
    ElBindingsT {
        func: EM_DELETE_NEXT_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"em-delete-next-word") }),
        description: Cow::Borrowed(&const { wide(b"Cut from cursor to end of current word") }),
    },
    ElBindingsT {
        func: EM_YANK as i32,
        name: Cow::Borrowed(&const { wide(b"em-yank") }),
        description: Cow::Borrowed(&const { wide(b"Paste cut buffer at cursor position") }),
    },
    ElBindingsT {
        func: EM_KILL_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"em-kill-line") }),
        description: Cow::Borrowed(&const { wide(b"Cut the entire line and save in cut buffer") }),
    },
    ElBindingsT {
        func: EM_KILL_REGION as i32,
        name: Cow::Borrowed(&const { wide(b"em-kill-region") }),
        description: Cow::Borrowed(
            &const { wide(b"Cut area between mark and cursor and save in cut buffer") },
        ),
    },
    ElBindingsT {
        func: EM_COPY_REGION as i32,
        name: Cow::Borrowed(&const { wide(b"em-copy-region") }),
        description: Cow::Borrowed(
            &const { wide(b"Copy area between mark and cursor to cut buffer") },
        ),
    },
    ElBindingsT {
        func: EM_GOSMACS_TRANSPOSE as i32,
        name: Cow::Borrowed(&const { wide(b"em-gosmacs-transpose") }),
        description: Cow::Borrowed(
            &const { wide(b"Exchange the two characters before the cursor") },
        ),
    },
    ElBindingsT {
        func: EM_NEXT_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"em-next-word") }),
        description: Cow::Borrowed(&const { wide(b"Move next to end of current word") }),
    },
    ElBindingsT {
        func: EM_UPPER_CASE as i32,
        name: Cow::Borrowed(&const { wide(b"em-upper-case") }),
        description: Cow::Borrowed(
            &const { wide(b"Uppercase the characters from cursor to end of current word") },
        ),
    },
    ElBindingsT {
        func: EM_CAPITOL_CASE as i32,
        name: Cow::Borrowed(&const { wide(b"em-capitol-case") }),
        description: Cow::Borrowed(
            &const { wide(b"Capitalize the characters from cursor to end of current word") },
        ),
    },
    ElBindingsT {
        func: EM_LOWER_CASE as i32,
        name: Cow::Borrowed(&const { wide(b"em-lower-case") }),
        description: Cow::Borrowed(
            &const { wide(b"Lowercase the characters from cursor to end of current word") },
        ),
    },
    ElBindingsT {
        func: EM_SET_MARK as i32,
        name: Cow::Borrowed(&const { wide(b"em-set-mark") }),
        description: Cow::Borrowed(&const { wide(b"Set the mark at cursor") }),
    },
    ElBindingsT {
        func: EM_EXCHANGE_MARK as i32,
        name: Cow::Borrowed(&const { wide(b"em-exchange-mark") }),
        description: Cow::Borrowed(&const { wide(b"Exchange the cursor and mark") }),
    },
    ElBindingsT {
        func: EM_UNIVERSAL_ARGUMENT as i32,
        name: Cow::Borrowed(&const { wide(b"em-universal-argument") }),
        description: Cow::Borrowed(&const { wide(b"Universal argument (argument times 4)") }),
    },
    ElBindingsT {
        func: EM_META_NEXT as i32,
        name: Cow::Borrowed(&const { wide(b"em-meta-next") }),
        description: Cow::Borrowed(&const { wide(b"Add 8th bit to next character typed") }),
    },
    ElBindingsT {
        func: EM_TOGGLE_OVERWRITE as i32,
        name: Cow::Borrowed(&const { wide(b"em-toggle-overwrite") }),
        description: Cow::Borrowed(
            &const { wide(b"Switch from insert to overwrite mode or vice versa") },
        ),
    },
    ElBindingsT {
        func: EM_COPY_PREV_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"em-copy-prev-word") }),
        description: Cow::Borrowed(&const { wide(b"Copy current word to cursor") }),
    },
    ElBindingsT {
        func: EM_INC_SEARCH_NEXT as i32,
        name: Cow::Borrowed(&const { wide(b"em-inc-search-next") }),
        description: Cow::Borrowed(&const { wide(b"Emacs incremental next search") }),
    },
    ElBindingsT {
        func: EM_INC_SEARCH_PREV as i32,
        name: Cow::Borrowed(&const { wide(b"em-inc-search-prev") }),
        description: Cow::Borrowed(&const { wide(b"Emacs incremental reverse search") }),
    },
    ElBindingsT {
        func: EM_DELETE_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"em-delete-prev-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Delete the character to the left of the cursor") },
        ),
    },
    ElBindingsT {
        func: ED_END_OF_FILE as i32,
        name: Cow::Borrowed(&const { wide(b"ed-end-of-file") }),
        description: Cow::Borrowed(&const { wide(b"Indicate end of file") }),
    },
    ElBindingsT {
        func: ED_INSERT as i32,
        name: Cow::Borrowed(&const { wide(b"ed-insert") }),
        description: Cow::Borrowed(&const { wide(b"Add character to the line") }),
    },
    ElBindingsT {
        func: ED_DELETE_PREV_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"ed-delete-prev-word") }),
        description: Cow::Borrowed(
            &const { wide(b"Delete from beginning of current word to cursor") },
        ),
    },
    ElBindingsT {
        func: ED_DELETE_NEXT_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"ed-delete-next-char") }),
        description: Cow::Borrowed(&const { wide(b"Delete character under cursor") }),
    },
    ElBindingsT {
        func: ED_KILL_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"ed-kill-line") }),
        description: Cow::Borrowed(&const { wide(b"Cut to the end of line") }),
    },
    ElBindingsT {
        func: ED_MOVE_TO_END as i32,
        name: Cow::Borrowed(&const { wide(b"ed-move-to-end") }),
        description: Cow::Borrowed(&const { wide(b"Move cursor to the end of line") }),
    },
    ElBindingsT {
        func: ED_MOVE_TO_BEG as i32,
        name: Cow::Borrowed(&const { wide(b"ed-move-to-beg") }),
        description: Cow::Borrowed(&const { wide(b"Move cursor to the beginning of line") }),
    },
    ElBindingsT {
        func: ED_TRANSPOSE_CHARS as i32,
        name: Cow::Borrowed(&const { wide(b"ed-transpose-chars") }),
        description: Cow::Borrowed(
            &const { wide(b"Exchange the character to the left of the cursor with the one under it") },
        ),
    },
    ElBindingsT {
        func: ED_NEXT_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"ed-next-char") }),
        description: Cow::Borrowed(&const { wide(b"Move to the right one character") }),
    },
    ElBindingsT {
        func: ED_PREV_WORD as i32,
        name: Cow::Borrowed(&const { wide(b"ed-prev-word") }),
        description: Cow::Borrowed(&const { wide(b"Move to the beginning of the current word") }),
    },
    ElBindingsT {
        func: ED_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"ed-prev-char") }),
        description: Cow::Borrowed(&const { wide(b"Move to the left one character") }),
    },
    ElBindingsT {
        func: ED_QUOTED_INSERT as i32,
        name: Cow::Borrowed(&const { wide(b"ed-quoted-insert") }),
        description: Cow::Borrowed(&const { wide(b"Add the next character typed verbatim") }),
    },
    ElBindingsT {
        func: ED_DIGIT as i32,
        name: Cow::Borrowed(&const { wide(b"ed-digit") }),
        description: Cow::Borrowed(&const { wide(b"Adds to argument or enters a digit") }),
    },
    ElBindingsT {
        func: ED_ARGUMENT_DIGIT as i32,
        name: Cow::Borrowed(&const { wide(b"ed-argument-digit") }),
        description: Cow::Borrowed(&const { wide(b"Digit that starts argument") }),
    },
    ElBindingsT {
        func: ED_UNASSIGNED as i32,
        name: Cow::Borrowed(&const { wide(b"ed-unassigned") }),
        description: Cow::Borrowed(&const { wide(b"Indicates unbound character") }),
    },
    ElBindingsT {
        func: ED_IGNORE as i32,
        name: Cow::Borrowed(&const { wide(b"ed-ignore") }),
        description: Cow::Borrowed(&const { wide(b"Input characters that have no effect") }),
    },
    ElBindingsT {
        func: ED_NEWLINE as i32,
        name: Cow::Borrowed(&const { wide(b"ed-newline") }),
        description: Cow::Borrowed(&const { wide(b"Execute command") }),
    },
    ElBindingsT {
        func: ED_DELETE_PREV_CHAR as i32,
        name: Cow::Borrowed(&const { wide(b"ed-delete-prev-char") }),
        description: Cow::Borrowed(
            &const { wide(b"Delete the character to the left of the cursor") },
        ),
    },
    ElBindingsT {
        func: ED_CLEAR_SCREEN as i32,
        name: Cow::Borrowed(&const { wide(b"ed-clear-screen") }),
        description: Cow::Borrowed(
            &const { wide(b"Clear screen leaving current line at the top") },
        ),
    },
    ElBindingsT {
        func: ED_REDISPLAY as i32,
        name: Cow::Borrowed(&const { wide(b"ed-redisplay") }),
        description: Cow::Borrowed(&const { wide(b"Redisplay everything") }),
    },
    ElBindingsT {
        func: ED_START_OVER as i32,
        name: Cow::Borrowed(&const { wide(b"ed-start-over") }),
        description: Cow::Borrowed(&const { wide(b"Erase current line and start from scratch") }),
    },
    ElBindingsT {
        func: ED_SEQUENCE_LEAD_IN as i32,
        name: Cow::Borrowed(&const { wide(b"ed-sequence-lead-in") }),
        description: Cow::Borrowed(&const { wide(b"First character in a bound sequence") }),
    },
    ElBindingsT {
        func: ED_PREV_HISTORY as i32,
        name: Cow::Borrowed(&const { wide(b"ed-prev-history") }),
        description: Cow::Borrowed(&const { wide(b"Move to the previous history line") }),
    },
    ElBindingsT {
        func: ED_NEXT_HISTORY as i32,
        name: Cow::Borrowed(&const { wide(b"ed-next-history") }),
        description: Cow::Borrowed(&const { wide(b"Move to the next history line") }),
    },
    ElBindingsT {
        func: ED_SEARCH_PREV_HISTORY as i32,
        name: Cow::Borrowed(&const { wide(b"ed-search-prev-history") }),
        description: Cow::Borrowed(
            &const { wide(b"Search previous in history for a line matching the current") },
        ),
    },
    ElBindingsT {
        func: ED_SEARCH_NEXT_HISTORY as i32,
        name: Cow::Borrowed(&const { wide(b"ed-search-next-history") }),
        description: Cow::Borrowed(
            &const { wide(b"Search next in history for a line matching the current") },
        ),
    },
    ElBindingsT {
        func: ED_PREV_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"ed-prev-line") }),
        description: Cow::Borrowed(&const { wide(b"Move up one line") }),
    },
    ElBindingsT {
        func: ED_NEXT_LINE as i32,
        name: Cow::Borrowed(&const { wide(b"ed-next-line") }),
        description: Cow::Borrowed(&const { wide(b"Move down one line") }),
    },
    ElBindingsT {
        func: ED_COMMAND as i32,
        name: Cow::Borrowed(&const { wide(b"ed-command") }),
        description: Cow::Borrowed(&const { wide(b"Editline extended command") }),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_number_has_exactly_one_help_row() {
        // `sem:map.map-init-fn` copies `EL_NUM_FCNS` rows out of
        // `EL_FUNC_HELP` and indexes `EL_FUNC` by command number, and both are
        // correct only because the three tables come from one scan of the same
        // doc comments. A regeneration that dropped a description line would
        // leave the help table short, which in the C is a `memcpy` reading past
        // the end of it.
        let mut seen = [false; EL_NUM_FCNS];
        for row in &EL_FUNC_HELP {
            let n = usize::try_from(row.func).expect("a help row names a command");
            assert!(!seen[n], "command {n} has two help rows");
            seen[n] = true;
        }
        assert!(seen.iter().all(|&s| s), "a command has no help row");
    }

    #[test]
    fn the_help_table_is_in_source_order_and_not_command_order() {
        // Its index is unrelated to the command number: every lookup must
        // compare `func`, never index by it. `vi.c` is scanned first, so row
        // zero is a `VI_*` command and command zero is not.
        assert_eq!(EL_FUNC_HELP[0].func, i32::from(VI_PASTE_NEXT));
        assert_ne!(EL_FUNC_HELP[0].func, 0);
    }
}
