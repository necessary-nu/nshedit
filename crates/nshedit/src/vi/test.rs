use super::*;
use crate::chared::ch_init;
use crate::common::ed_insert;
use crate::el::blank_editline;
use crate::histedit::CC_NEWLINE;
use crate::histedit::{H_FIRST, HistEventW};
use crate::map::ElFuncT;
use crate::read::read_init;
use crate::search::search_init;
use core::ffi::{c_int, c_void};
use std::sync::OnceLock;

/// An editor in vi command mode, in the state `el_init` plus a `vi`
/// `el_set(EL_EDITOR)` leaves behind, with `s` in the line and the cursor
/// at `at`.
///
/// `ch_init` sizes the line, undo, redo and kill buffers at `EL_BUFSIZ`
/// and puts `limit` two slots below the end of the line, which is the
/// slack the insert paths shift into. The screen has to be real too:
/// `re_refresh` walks `el_display` under `t_size`, and a zero-sized
/// terminal makes it recurse until the stack runs out.
fn el_with(s: &str, at: usize) -> EditLine {
    let mut el = blank_editline();
    ch_init(&mut el);
    let chars: Vec<u32> = s.chars().map(u32::from).collect();
    el.el_line.buffer[..chars.len()].copy_from_slice(&chars);
    el.el_line.lastchar = chars.len();
    el.el_line.cursor = at;
    // `el_init` allocates the pattern buffer; `cv_search` writes through
    // it, and `chadir`/`chacha` start from here too.
    search_init(&mut el);
    // Nothing here has a terminal to talk to, and descriptor 0 is the
    // test runner's. `write_fd` already treats a negative one as "no
    // destination", so the editor writes into the void instead of
    // spraying escape sequences over the test output.
    el.el_infd = -1;
    el.el_outfd = -1;
    el.el_errfd = -1;
    el.el_map.r#type = MAP_VI;
    // `el_map.alt` is the vi COMMAND keymap; `ch_init` leaves `key` — the
    // insert one — current, which is not where these commands are typed.
    el.el_map.current = ElMapCurrent::Alt;
    // C: `wcsdup(L"_")` from `map_init_vi`. `ch_init` does not install it
    // and every word test here consults it.
    el.el_map.wordchars = Some(vec![u32::from(b'_')]);
    el.el_terminal.t_size.h = 80;
    el.el_terminal.t_size.v = 24;
    el.el_display = vec![vec![0u32; 81]; 24];
    el.el_vdisplay = vec![vec![0u32; 81]; 24];
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

/// Fill the kill buffer, so that a command which is supposed to *empty* it
/// can be told apart from one that leaves it alone.
fn preload_kill(el: &mut EditLine, s: &str) {
    let chars: Vec<u32> = s.chars().map(u32::from).collect();
    el.el_chared.c_kill.buf[..chars.len()].copy_from_slice(&chars);
    el.el_chared.c_kill.last = chars.len();
}

/// Arm the pending vi operator by hand, as the first keystroke of `dw` or
/// `y%` would have done.
fn pending(el: &mut EditLine, action: i32) {
    el.el_chared.c_vcmd.action = action;
    el.el_chared.c_vcmd.pos = el.el_line.cursor;
}

/// Queue `s` as terminal input. The commands that read a character do it
/// through `el_wgetc`, which drains the macro queue before it ever touches
/// the tty — so a test never has to own a terminal to feed one.
fn feed(el: &mut EditLine, s: &str) {
    let chars: Vec<u32> = s.chars().map(u32::from).collect();
    el_wpush(el, Some(&chars));
}

/// Drain `n` characters back out of the macro queue. Reading past what was
/// pushed would fall through to the real tty, so the count is the caller's
/// responsibility.
fn drain(el: &mut EditLine, n: usize) -> String {
    let mut out = String::new();
    for _ in 0..n {
        let mut c: u32 = 0;
        assert_eq!(el_wgetc(el, &mut c), 1);
        out.push(char::from_u32(c).unwrap());
    }
    out
}

// [spec:libedit:sem:vi.cv-action-fn/test]
// [spec:libedit:sem:vi.vi-delete-meta-fn/test]
/// The first `d` only records the anchor and the operator and returns
/// `CC_ARGHACK`, which is the one return the dispatcher answers by
/// skipping its per-command reset — that is what carries the pending
/// operator into the next keystroke.
#[test]
fn an_operator_arms_itself_and_edits_nothing() {
    let mut el = el_with("abcdef", 2);
    assert_eq!(vi_delete_meta(&mut el, 0), CC_ARGHACK);
    assert_eq!(el.el_chared.c_vcmd.action, DELETE);
    assert_eq!(el.el_chared.c_vcmd.pos, 2);
    assert_eq!(text(&el), "abcdef");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(el.el_chared.c_undo.len, -1);
}

// [spec:libedit:sem:vi.cv-action-fn/test]
// [spec:libedit:sem:vi.vi-delete-meta-fn/test]
// [spec:libedit:sem:vi.vi-change-meta-fn/test]
// [spec:libedit:sem:vi.vi-yank-fn/test]
/// Doubling the operator acts on the whole line. `dd` and `cc` empty it,
/// `yy` leaves it alone; `cc` additionally drops into insert mode, and
/// `yy` takes no undo snapshot at all — the `YANK` bit suppresses it,
/// which is why `u` cannot follow a `yy`.
#[test]
fn a_doubled_operator_acts_on_the_whole_line() {
    let mut el = el_with("abcdef", 3);
    vi_delete_meta(&mut el, 0);
    assert_eq!(vi_delete_meta(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(killed(&el), "abcdef");
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
    assert_eq!(el.el_chared.c_undo.len, 6);
    assert!(
        el.el_map.current == ElMapCurrent::Alt,
        "`dd` stays in command mode"
    );

    let mut el = el_with("abcdef", 3);
    vi_change_meta(&mut el, 0);
    assert_eq!(vi_change_meta(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "");
    assert!(
        el.el_map.current == ElMapCurrent::Key,
        "`cc` enters insert mode"
    );

    let mut el = el_with("abcdef", 3);
    vi_yank(&mut el, 0);
    assert_eq!(vi_yank(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abcdef", "`yy` copies, it does not cut");
    assert_eq!(killed(&el), "abcdef");
    assert_eq!(
        el.el_chared.c_undo.len, -1,
        "`YANK` suppresses the snapshot"
    );
}

// [spec:libedit:sem:vi.cv-action-fn/test]
/// Two *different* operators are rejected outright. Nothing at all is
/// touched — not even the pending operator, which the dispatcher clears
/// on the `CC_ERROR` instead.
#[test]
fn mismatched_operators_are_refused_without_side_effects() {
    let mut el = el_with("abcdef", 2);
    vi_delete_meta(&mut el, 0);
    assert_eq!(vi_yank(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "abcdef");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(el.el_chared.c_vcmd.action, DELETE);
    assert_eq!(el.el_chared.c_vcmd.pos, 2);
    assert_eq!(el.el_chared.c_kill.last, 0);
    assert_eq!(el.el_chared.c_undo.len, -1);
}

// [spec:libedit:sem:vi.cv-paste-fn/test]
// [spec:libedit:sem:vi.vi-paste-next-fn/test]
// [spec:libedit:sem:vi.vi-paste-prev-fn/test]
/// `p` pastes after the character under the cursor and `P` at it. The
/// cursor is left on the *first* pasted character either way, where real
/// vi leaves it on the last (ERR-modes-58), and ERR-modes-68 means the
/// count chooses nothing: `3p` pastes one copy.
#[test]
fn paste_puts_the_text_after_or_at_the_cursor() {
    let mut el = el_with("abc", 0);
    preload_kill(&mut el, "XY");
    el.el_state.argument = 3;
    assert_eq!(vi_paste_next(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "aXYbc");
    assert_eq!(el.el_line.cursor, 1);
    // The snapshot is of the line as it was, taken before anything moved.
    assert_eq!(el.el_chared.c_undo.len, 3);
    assert_eq!(el.el_chared.c_undo.cursor, 0);

    let mut el = el_with("abc", 0);
    preload_kill(&mut el, "XY");
    assert_eq!(vi_paste_prev(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "XYabc");
    assert_eq!(el.el_line.cursor, 0);

    // The kill buffer is not consumed, so a second paste repeats it.
    assert_eq!(killed(&el), "XY");
}

// [spec:libedit:sem:vi.cv-paste-fn/test]
// [spec:libedit:sem:vi.vi-paste-next-fn/test]
/// At the end of the line there is no character to paste *after*, so the
/// cursor does not advance and `p` behaves exactly as `P`. An empty kill
/// buffer is refused before anything happens at all.
#[test]
fn paste_after_at_end_of_line_degrades_to_paste_at() {
    let mut el = el_with("abc", 3);
    preload_kill(&mut el, "XY");
    assert_eq!(vi_paste_next(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abcXY");
    assert_eq!(el.el_line.cursor, 3);

    let mut el = el_with("abc", 1);
    assert_eq!(vi_paste_next(&mut el, 0), CC_ERROR);
    assert_eq!(vi_paste_prev(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "abc");
    assert_eq!(el.el_chared.c_undo.len, -1);
}

// [spec:libedit:sem:vi.vi-prev-word-fn/test]
// [spec:libedit:sem:vi.vi-prev-big-word-fn/test]
/// `b` stops where the character class changes and `B` only at
/// whitespace, so a punctuation run is a word of its own to `b` and part
/// of the surrounding word to `B`. Both refuse at the head of the line.
#[test]
fn the_backward_word_motions_differ_over_punctuation() {
    let mut el = el_with("foo.bar baz", 7);
    assert_eq!(vi_prev_word(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 4, "stopped at the `.`");

    let mut el = el_with("foo.bar baz", 7);
    assert_eq!(vi_prev_big_word(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 0, "swallowed the `.`");

    for motion in [
        vi_prev_word as fn(&mut EditLine, u32) -> ElActionT,
        vi_prev_big_word,
    ] {
        let mut el = el_with("foo", 0);
        assert_eq!(motion(&mut el, 0), CC_ERROR);
        assert_eq!(el.el_line.cursor, 0);
    }
}

// [spec:libedit:sem:vi.vi-prev-big-word-fn/test]
/// A backward motion under an operator deletes back to the anchor and
/// stops there, so the character the cursor started on survives — the
/// range is `[landing, anchor)`. Note there is no `MAP_VI` guard on this
/// path, unlike the forward motions.
#[test]
fn a_backward_motion_under_an_operator_is_exclusive_of_the_anchor() {
    let mut el = el_with("foo.bar baz", 7);
    pending(&mut el, DELETE);
    assert_eq!(vi_prev_big_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), " baz");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(killed(&el), "foo.bar");
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
}

// [spec:libedit:sem:vi.vi-next-word-fn/test]
// [spec:libedit:sem:vi.vi-next-big-word-fn/test]
/// `w` stops where the class changes; `W` runs to the next blank and then
/// past it. ERR-modes-06: the guard is "fewer than two characters remain",
/// so both fail on the *last* character of the line as well as on an empty
/// one — the C writes `cursor >= lastchar - 1`, which would form
/// `buffer - 1` on an empty line.
#[test]
fn the_forward_word_motions_stop_where_the_class_changes() {
    let mut el = el_with("foo.bar baz", 0);
    assert_eq!(vi_next_word(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 3);

    let mut el = el_with("foo.bar baz", 0);
    assert_eq!(vi_next_big_word(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 8);

    for motion in [
        vi_next_word as fn(&mut EditLine, u32) -> ElActionT,
        vi_next_big_word,
    ] {
        let mut el = el_with("abc", 2);
        assert_eq!(motion(&mut el, 0), CC_ERROR, "on the last character");
        let mut el = el_with("", 0);
        assert_eq!(motion(&mut el, 0), CC_ERROR, "on an empty line");
    }
}

// [spec:libedit:sem:vi.vi-next-word-fn/test]
/// `cw` is `ce`: `cv_next_word` suppresses its trailing-blank skip on the
/// last iteration when the pending operator is exactly `DELETE|INSERT`, so
/// the space after the word survives a change and does not survive a
/// delete. That suppression is why `c` and `d` disagree here and nowhere
/// else.
#[test]
fn change_word_keeps_the_trailing_blank_that_delete_word_eats() {
    let mut el = el_with("foo bar", 0);
    pending(&mut el, DELETE | INSERT);
    assert_eq!(vi_next_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), " bar");
    assert!(el.el_map.current == ElMapCurrent::Key);

    let mut el = el_with("foo bar", 0);
    pending(&mut el, DELETE);
    assert_eq!(vi_next_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "bar");
    assert!(el.el_map.current == ElMapCurrent::Alt);
}

// [spec:libedit:sem:vi.vi-end-word-fn/test]
// [spec:libedit:sem:vi.vi-end-big-word-fn/test]
/// `e` and `E` land *on* the last character of the word rather than past
/// it, and under an operator they step one further so the span is
/// inclusive of it — that extra step is the whole difference from `w`.
/// Both refuse only when the cursor is already at `lastchar`, so unlike
/// `w` they still work on the last character of the line.
#[test]
fn the_end_of_word_motions_land_on_the_last_character() {
    let mut el = el_with("foo.bar baz", 0);
    assert_eq!(vi_end_word(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 2);

    let mut el = el_with("foo.bar baz", 0);
    assert_eq!(vi_end_big_word(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 6);

    // `de` keeps the blank the word ended before; `dw` would have eaten it.
    let mut el = el_with("foo bar", 0);
    pending(&mut el, DELETE);
    assert_eq!(vi_end_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), " bar");

    for motion in [
        vi_end_word as fn(&mut EditLine, u32) -> ElActionT,
        vi_end_big_word,
    ] {
        let mut el = el_with("abc", 3);
        assert_eq!(motion(&mut el, 0), CC_ERROR);
        let mut el = el_with("abc", 2);
        assert_eq!(motion(&mut el, 0), CC_CURSOR, "the last character is fine");
    }
}

// [spec:libedit:sem:vi.vi-change-case-fn/test]
/// `~` flips the case of `argument` characters and walks over them, but
/// the walk is clamped to the last character rather than to `lastchar`, so
/// an over-large count leaves the cursor sitting on the final character
/// instead of past it. Characters with no case still consume an iteration.
#[test]
fn change_case_walks_the_count_and_clamps_onto_the_last_character() {
    let mut el = el_with("aBc", 0);
    el.el_state.argument = 2;
    assert_eq!(vi_change_case(&mut el, 0), CC_NORM);
    assert_eq!(text(&el), "Abc");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(el.el_chared.c_undo.len, 3, "one snapshot covers the run");

    let mut el = el_with("aB", 0);
    el.el_state.argument = 5;
    vi_change_case(&mut el, 0);
    assert_eq!(text(&el), "Ab");
    assert_eq!(el.el_line.cursor, 1);

    let mut el = el_with("a1b", 0);
    el.el_state.argument = 3;
    vi_change_case(&mut el, 0);
    assert_eq!(text(&el), "A1B");

    // Nothing under the cursor, and no snapshot is taken.
    let mut el = el_with("", 0);
    assert_eq!(vi_change_case(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_chared.c_undo.len, -1);
}

// [spec:libedit:sem:vi.vi-insert-at-bol-fn/test]
/// ERR-modes-60: `I` goes to column zero, not to the first non-blank
/// character as real vi does. The snapshot is taken after the move, so it
/// records the cursor the undo will restore.
#[test]
fn insert_at_bol_goes_to_column_zero_not_to_the_first_word() {
    let mut el = el_with("   ab", 4);
    assert_eq!(vi_insert_at_bol(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 0);
    assert!(el.el_map.current == ElMapCurrent::Key);
    assert_eq!(el.el_chared.c_undo.cursor, 0);
    assert_eq!(el.el_chared.c_undo.len, 5);
}

// [spec:libedit:sem:vi.vi-replace-char-fn/test]
/// `r` is only half a command: it arms `MODE_REPLACE_1` and returns
/// `CC_ARGHACK` so the count survives, and `ed_insert` does the actual
/// overwrite and hands back to command mode with the cursor left on the
/// character it replaced.
#[test]
fn replace_char_arms_a_one_shot_overwrite_that_ed_insert_completes() {
    let mut el = el_with("abc", 1);
    assert_eq!(vi_replace_char(&mut el, 0), CC_ARGHACK);
    assert_eq!(el.el_state.inputmode, MODE_REPLACE_1);
    assert!(el.el_map.current == ElMapCurrent::Key);
    assert_eq!(el.el_chared.c_undo.len, 3);

    assert_eq!(ed_insert(&mut el, u32::from(b'X')), CC_CURSOR);
    assert_eq!(text(&el), "aXc");
    assert_eq!(el.el_line.cursor, 1);
    assert_eq!(el.el_state.inputmode, MODE_INSERT);
    assert!(el.el_map.current == ElMapCurrent::Alt);

    // Nothing under the cursor: refused before anything is armed.
    let mut el = el_with("abc", 3);
    assert_eq!(vi_replace_char(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_state.inputmode, MODE_INSERT);
    assert!(el.el_map.current == ElMapCurrent::Alt);
}

// [spec:libedit:sem:vi.vi-replace-mode-fn/test]
/// `R` is the sticky form and, unlike `r`, has no guard at all: it arms
/// overwrite mode on an empty line too, where there is nothing to
/// overwrite yet.
#[test]
fn replace_mode_is_sticky_and_unguarded() {
    let mut el = el_with("", 0);
    assert_eq!(vi_replace_mode(&mut el, 0), CC_NORM);
    assert_eq!(el.el_state.inputmode, MODE_REPLACE);
    assert!(el.el_map.current == ElMapCurrent::Key);
    assert_eq!(el.el_chared.c_undo.len, 0);
}

// [spec:libedit:sem:vi.vi-substitute-char-fn/test]
/// `s` deletes forward and drops into insert mode. There is no error path:
/// at the end of the line it deletes nothing, still empties the kill
/// buffer, still snapshots undo and still enters insert mode.
#[test]
fn substitute_char_has_no_failure_case() {
    let mut el = el_with("abcdef", 2);
    el.el_state.argument = 2;
    assert_eq!(vi_substitute_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abef");
    assert_eq!(killed(&el), "cd");
    assert!(el.el_map.current == ElMapCurrent::Key);
    assert_eq!(el.el_chared.c_undo.len, 6);

    let mut el = el_with("abc", 3);
    preload_kill(&mut el, "zz");
    assert_eq!(vi_substitute_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abc");
    assert_eq!(el.el_chared.c_kill.last, 0, "the kill buffer is emptied");
    assert!(el.el_map.current == ElMapCurrent::Key);
}

// [spec:libedit:sem:vi.vi-substitute-line-fn/test]
// [spec:libedit:sem:vi.vi-change-to-eol-fn/test]
/// `S` replaces the whole line and `C` only the part from the cursor on;
/// both leave the removed text in the kill buffer, snapshot undo and enter
/// insert mode, and neither has an error path.
#[test]
fn the_line_substitutions_cut_and_enter_insert_mode() {
    let mut el = el_with("abcdef", 3);
    assert_eq!(vi_substitute_line(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(killed(&el), "abcdef");
    assert_eq!(el.el_chared.c_undo.len, 6);
    assert!(el.el_map.current == ElMapCurrent::Key);

    let mut el = el_with("abcdef", 2);
    assert_eq!(vi_change_to_eol(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "ab");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(killed(&el), "cdef");
    assert!(el.el_map.current == ElMapCurrent::Key);

    let mut el = el_with("", 0);
    preload_kill(&mut el, "zz");
    assert_eq!(vi_change_to_eol(&mut el, 0), CC_REFRESH);
    assert_eq!(el.el_chared.c_kill.last, 0);
    assert!(el.el_map.current == ElMapCurrent::Key);
}

// [spec:libedit:sem:vi.vi-insert-fn/test]
// [spec:libedit:sem:vi.vi-add-fn/test]
// [spec:libedit:sem:vi.vi-add-at-eol-fn/test]
/// The three ways into insert mode differ only in where they leave the
/// cursor, and each snapshots undo *after* the move so that `u` restores
/// the position insert mode started from. ERR-modes-68: `3a` is `a`.
#[test]
fn the_insert_mode_entries_differ_only_in_the_cursor() {
    let mut el = el_with("abc", 1);
    assert_eq!(vi_insert(&mut el, 0), CC_NORM);
    assert_eq!(el.el_line.cursor, 1);
    assert!(el.el_map.current == ElMapCurrent::Key);
    assert_eq!(el.el_chared.c_undo.cursor, 1);

    let mut el = el_with("abc", 1);
    el.el_state.argument = 3;
    assert_eq!(vi_add(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(el.el_chared.c_undo.cursor, 2, "snapshotted after the move");

    // At the end of the line there is nothing to move past.
    let mut el = el_with("abc", 3);
    assert_eq!(vi_add(&mut el, 0), CC_NORM);
    assert_eq!(el.el_line.cursor, 3);

    let mut el = el_with("abc", 1);
    assert_eq!(vi_add_at_eol(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 3);
    assert_eq!(el.el_chared.c_undo.cursor, 3);
    assert!(el.el_map.current == ElMapCurrent::Key);
}

// [spec:libedit:sem:vi.vi-undo-fn/test]
/// `u` *swaps* the line and undo buffers rather than copying, which is
/// what makes it its own inverse: a second `u` returns to the edited line.
/// Before the first snapshot there is nothing to undo, and that is the
/// only time `len == -1`.
#[test]
fn undo_swaps_the_line_with_the_snapshot_and_is_its_own_inverse() {
    let mut el = el_with("abcdef", 3);
    assert_eq!(vi_undo(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "abcdef");

    cv_undo(&mut el);
    // The no-yank delete, so the snapshot above is the only one taken.
    c_delbefore1(&mut el);
    el.el_line.cursor -= 1;
    assert_eq!(text(&el), "abdef");

    assert_eq!(vi_undo(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abcdef");
    assert_eq!(el.el_line.cursor, 3);

    assert_eq!(vi_undo(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abdef");
    assert_eq!(el.el_line.cursor, 2);
}

// [spec:libedit:sem:vi.vi-command-mode-fn/test]
/// Leaving insert mode moves the cursor left onto the last character
/// typed, cancels any pending operator, and switches to the `alt` keymap —
/// which under an emacs keymap is all `ED_UNASSIGNED`, so binding this
/// there makes the keyboard inert. `el_state.argument` is deliberately not
/// reset; the dispatcher does that on return.
#[test]
fn command_mode_steps_back_onto_the_last_character_typed() {
    let mut el = el_with("abc", 2);
    el.el_map.current = ElMapCurrent::Key;
    el.el_state.inputmode = MODE_REPLACE;
    el.el_state.doingarg = 1;
    el.el_state.argument = 5;
    pending(&mut el, DELETE);

    assert_eq!(vi_command_mode(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1);
    assert!(el.el_map.current == ElMapCurrent::Alt);
    assert_eq!(el.el_state.inputmode, MODE_INSERT);
    assert_eq!(el.el_state.doingarg, 0);
    assert_eq!(el.el_state.argument, 5);
    assert_eq!(el.el_chared.c_vcmd.action, NOP);

    // At column zero there is nothing to step back onto.
    let mut el = el_with("abc", 0);
    assert_eq!(vi_command_mode(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 0);
}

// [spec:libedit:sem:vi.vi-zero-fn/test]
/// `0` is two commands: another digit while a count is being entered, and
/// "go to column zero" otherwise. Under an operator the landing position
/// is left of the anchor, so `d0` deletes `[buffer, anchor)` and the
/// character the cursor was on survives.
#[test]
fn zero_is_a_digit_while_a_count_is_being_entered() {
    let mut el = el_with("abcdef", 3);
    el.el_state.doingarg = 1;
    el.el_state.argument = 4;
    assert_eq!(vi_zero(&mut el, u32::from(b'0')), CC_ARGHACK);
    assert_eq!(el.el_state.argument, 40);
    assert_eq!(el.el_line.cursor, 3, "no motion at all");

    let mut el = el_with("abcdef", 3);
    assert_eq!(vi_zero(&mut el, u32::from(b'0')), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 0);

    let mut el = el_with("abcdef", 3);
    pending(&mut el, DELETE);
    assert_eq!(vi_zero(&mut el, u32::from(b'0')), CC_REFRESH);
    assert_eq!(text(&el), "def");
    assert_eq!(el.el_line.cursor, 0);

    // No error path: `0` on an empty line simply stays put.
    let mut el = el_with("", 0);
    assert_eq!(vi_zero(&mut el, u32::from(b'0')), CC_CURSOR);
}

// [spec:libedit:sem:vi.vi-delete-prev-char-fn/test]
/// The no-yank backspace: exactly one character however large the count,
/// and neither the kill buffer nor the undo snapshot is written — which is
/// what separates it from `ed_delete_prev_char`, and why `u` cannot
/// recover from it.
#[test]
fn vi_backspace_leaves_no_undo_and_no_kill() {
    let mut el = el_with("abcdef", 3);
    preload_kill(&mut el, "zz");
    el.el_state.argument = 5;
    assert_eq!(vi_delete_prev_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abdef");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(killed(&el), "zz");
    assert_eq!(el.el_chared.c_undo.len, -1);

    let mut el = el_with("abc", 0);
    assert_eq!(vi_delete_prev_char(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "abc");
}

// [spec:libedit:sem:vi.vi-list-or-eof-fn/test]
/// End of input is reported only from a completely empty line. Everywhere
/// else — including mid-line, where the C's completion listing is behind
/// `#ifdef notyet` — the command just beeps and errors, and the line is
/// never touched.
#[test]
fn list_or_eof_reports_end_of_input_only_on_an_empty_line() {
    let mut el = el_with("", 0);
    assert_eq!(vi_list_or_eof(&mut el, u32::from(b'\x04')), CC_EOF);

    for at in [1usize, 3] {
        let mut el = el_with("abc", at);
        assert_eq!(vi_list_or_eof(&mut el, u32::from(b'\x04')), CC_ERROR);
        assert_eq!(text(&el), "abc");
        assert_eq!(el.el_line.cursor, at);
    }
}

// [spec:libedit:sem:vi.vi-kill-line-prev-fn/test]
/// `^U` cuts everything before the cursor. ERR-modes-47: the hand-rolled
/// copy into the kill buffer is redundant because `c_delbefore` writes the
/// identical span over it — but `c_delbefore` is also what takes the undo
/// snapshot, which is what makes this undoable where `em_kill_line` is
/// not. At column zero it still empties the kill buffer and still
/// snapshots.
#[test]
fn kill_line_prev_cuts_behind_the_cursor_and_is_undoable() {
    let mut el = el_with("abcdef", 4);
    assert_eq!(vi_kill_line_prev(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "ef");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(killed(&el), "abcd");
    assert_eq!(el.el_chared.c_undo.len, 6);

    let mut el = el_with("abcdef", 0);
    preload_kill(&mut el, "zz");
    assert_eq!(vi_kill_line_prev(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abcdef");
    assert_eq!(el.el_chared.c_kill.last, 0);
    assert_eq!(el.el_chared.c_undo.len, 6);
}

// [spec:libedit:sem:vi.vi-repeat-search-next-fn/test]
// [spec:libedit:sem:vi.vi-repeat-search-prev-fn/test]
/// `n` and `N` refuse before any `/` or `?` has run, and the refusal is
/// early: `cv_repeat_srch` truncates the line to empty before it searches,
/// so reaching it with no pattern would have destroyed the line for
/// nothing.
#[test]
fn repeating_a_history_search_needs_a_pattern_first() {
    for repeat in [
        vi_repeat_search_next as fn(&mut EditLine, u32) -> ElActionT,
        vi_repeat_search_prev,
    ] {
        let mut el = el_with("abc", 1);
        assert_eq!(el.el_search.patlen, 0);
        assert_eq!(repeat(&mut el, 0), CC_ERROR);
        assert_eq!(text(&el), "abc", "the line survives the refusal");
        assert_eq!(el.el_line.cursor, 1);
    }
}

// [spec:libedit:sem:vi.vi-next-char-fn/test]
// [spec:libedit:sem:vi.vi-prev-char-fn/test]
// [spec:libedit:sem:vi.vi-to-next-char-fn/test]
// [spec:libedit:sem:vi.vi-to-prev-char-fn/test]
/// `f` and `F` land on the target; `t` and `T` stop one short of it,
/// against the direction of travel. All four read the target from the
/// terminal, which is what the `(wint_t)-1` sentinel in the shared helper
/// asks for.
#[test]
fn the_character_searches_land_on_or_beside_the_target() {
    for (search, from, want) in [
        (
            vi_next_char as fn(&mut EditLine, u32) -> ElActionT,
            0usize,
            2usize,
        ),
        (vi_to_next_char, 0, 1),
    ] {
        let mut el = el_with("abcabc", from);
        read_init(&mut el);
        feed(&mut el, "c");
        assert_eq!(search(&mut el, 0), CC_CURSOR);
        assert_eq!(el.el_line.cursor, want);
    }

    for (search, from, want) in [
        (
            vi_prev_char as fn(&mut EditLine, u32) -> ElActionT,
            5usize,
            3usize,
        ),
        (vi_to_prev_char, 5, 4),
    ] {
        let mut el = el_with("abcabc", from);
        read_init(&mut el);
        feed(&mut el, "a");
        assert_eq!(search(&mut el, 0), CC_CURSOR);
        assert_eq!(el.el_line.cursor, want);
    }
}

// [spec:libedit:sem:vi.vi-next-char-fn/test]
/// ERR-modes-40: the search is recorded for `;` and `,` *before* it runs,
/// so a failed `f` still rebinds what a following `;` will look for even
/// though the cursor did not move.
#[test]
fn a_failed_character_search_still_rebinds_the_repeat() {
    let mut el = el_with("abcabc", 0);
    read_init(&mut el);
    feed(&mut el, "z");
    assert_eq!(vi_next_char(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_search.chacha, u32::from(b'z'));
    assert_eq!(el.el_search.chadir, CHAR_FWD);
}

// [spec:libedit:sem:vi.vi-repeat-next-char-fn/test]
// [spec:libedit:sem:vi.vi-repeat-prev-char-fn/test]
/// `;` repeats the last `f`/`F`/`t`/`T` and `,` reverses it, and neither
/// prompts. `,` restores the recorded direction afterwards, so it does not
/// become the new default and repeated `,` does not alternate. Before any
/// character search both refuse, because the remembered target is still
/// `L'\0'`.
#[test]
fn the_character_search_repeats_reuse_the_remembered_target() {
    let mut el = el_with("abcabc", 0);
    assert_eq!(vi_repeat_next_char(&mut el, 0), CC_ERROR);
    assert_eq!(vi_repeat_prev_char(&mut el, 0), CC_ERROR);

    let mut el = el_with("abcabc", 0);
    read_init(&mut el);
    feed(&mut el, "c");
    vi_next_char(&mut el, 0);
    assert_eq!(el.el_line.cursor, 2);

    assert_eq!(vi_repeat_next_char(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 5);

    assert_eq!(vi_repeat_prev_char(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(
        el.el_search.chadir, CHAR_FWD,
        "`,` did not become the default"
    );
}

// [spec:libedit:sem:vi.vi-to-next-char-fn/test]
// [spec:libedit:sem:vi.vi-repeat-next-char-fn/test]
/// ERR-modes-64, reproduced: `;` after a `t` re-finds the same occurrence
/// and does not move. The "do not re-find the character already under the
/// cursor" step only fires when the cursor is *on* the target, which `t`
/// never leaves it, so the repeat has nothing to skip past.
#[test]
fn repeating_a_till_search_gets_stuck_on_the_same_occurrence() {
    let mut el = el_with("abcabc", 0);
    read_init(&mut el);
    feed(&mut el, "c");
    assert_eq!(vi_to_next_char(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1);

    assert_eq!(vi_repeat_next_char(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1, "did not advance");
}

// [spec:libedit:sem:vi.vi-match-fn/test]
/// `%` finds the first bracket at or after the cursor and jumps to its
/// partner, counting nesting on the way. ERR-modes-66: the search for the
/// bracket only ever looks *forward*, so a cursor past every bracket on
/// the line fails rather than matching the one behind it.
#[test]
fn match_jumps_between_partnered_brackets() {
    let mut el = el_with("a(bc)d", 0);
    assert_eq!(vi_match(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 4);

    let mut el = el_with("a(bc)d", 4);
    assert_eq!(vi_match(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1);

    let mut el = el_with("((a))", 0);
    assert_eq!(vi_match(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 4, "nesting is counted");

    // Running off the end leaves the cursor exactly where it was.
    let mut el = el_with("(ab", 0);
    assert_eq!(vi_match(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_line.cursor, 0);

    let mut el = el_with("(ab)c", 4);
    assert_eq!(vi_match(&mut el, 0), CC_ERROR, "never looks backwards");
}

// [spec:libedit:sem:vi.vi-match-fn/test]
/// The C's step-6 `cursor++` is guarded by `delta > 0` with `delta`
/// declared `size_t`, so the backward direction — written as
/// `1 - (delta & 1) * 2`, which is `SIZE_MAX` — passes the test and the
/// increment is unconditional. What that produces for a backward `d%` is
/// the range `[matched_open + 1, anchor)`: the matched opening bracket
/// survives, and so does the closing bracket the cursor was on.
#[test]
fn a_backward_match_under_an_operator_spares_both_brackets() {
    let mut el = el_with("a(bc)d", 4);
    pending(&mut el, DELETE);
    assert_eq!(vi_match(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "a()d");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(killed(&el), "bc");
}

// [spec:libedit:sem:vi.vi-undo-line-fn/test]
/// ERR-modes-62: `U` reloads whatever history event is selected, where
/// real vi's `U` restores the line as first entered. At event zero that is
/// the stashed live line. The snapshot is taken first, so a following `u`
/// gets back to the line `U` discarded.
#[test]
fn undo_line_reloads_the_selected_history_event() {
    let mut el = el_with("new", 3);
    el.el_history.buf = vec![0u32; EL_BUFSIZ];
    let stash: Vec<u32> = "old line".chars().map(u32::from).collect();
    el.el_history.buf[..stash.len()].copy_from_slice(&stash);
    el.el_history.sz = EL_BUFSIZ;
    el.el_history.last = stash.len();
    el.el_history.eventno = 0;

    assert_eq!(vi_undo_line(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "old line");
    assert_eq!(el.el_line.cursor, 0, "vi puts the cursor at the head");
    assert_eq!(el.el_chared.c_undo.len, 3, "the discarded line is undoable");
}

// [spec:libedit:sem:vi.vi-to-column-fn/test]
/// ERR-modes-63: `n|` counts characters, POSIX's definition, rather than
/// screen columns. ERR-modes-50: it goes through `ed_next_char`, which
/// clamps to `lastchar` and not `lastchar - 1`, so an over-large column
/// parks the vi cursor one past the last character — a position
/// `ed_next_char`'s own entry guard refuses to move to.
#[test]
fn to_column_counts_characters_and_overshoots_off_the_end() {
    let mut el = el_with("abcdef", 4);
    el.el_state.argument = 3;
    assert_eq!(vi_to_column(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 2, "column 3 is offset 2");

    let mut el = el_with("abcdef", 0);
    el.el_state.argument = 1;
    assert_eq!(vi_to_column(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 0);

    let mut el = el_with("abcdef", 0);
    el.el_state.argument = 999;
    assert_eq!(vi_to_column(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, el.el_line.lastchar);
}

// [spec:libedit:sem:vi.vi-yank-end-fn/test]
/// ERR-modes-59: `Y` is `y$`, where real vi's yanks the whole line.
/// Nothing is deleted, the cursor does not move, no undo snapshot is
/// taken, and ERR-modes-68 means the count is ignored. At the end of the
/// line it yanks nothing, which empties the kill buffer and so makes the
/// next `p` fail.
#[test]
fn yank_end_copies_only_to_the_end_of_the_line() {
    let mut el = el_with("abcdef", 2);
    el.el_state.argument = 3;
    assert_eq!(vi_yank_end(&mut el, 0), CC_REFRESH);
    assert_eq!(killed(&el), "cdef");
    assert_eq!(text(&el), "abcdef");
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(el.el_chared.c_undo.len, -1);

    let mut el = el_with("abc", 3);
    preload_kill(&mut el, "zz");
    assert_eq!(vi_yank_end(&mut el, 0), CC_REFRESH);
    assert_eq!(el.el_chared.c_kill.last, 0);
    assert_eq!(vi_paste_next(&mut el, 0), CC_ERROR);
}

// [spec:libedit:sem:vi.vi-yank-fn/test]
/// Because `YANK` is set, `cv_delfini` copies the spanned text and leaves
/// the line alone — but it still resets the cursor to the anchor, so `yw`
/// finishes where `y` was pressed rather than at the end of the word.
#[test]
fn yank_with_a_motion_copies_and_returns_to_the_anchor() {
    let mut el = el_with("foo bar", 0);
    assert_eq!(vi_yank(&mut el, 0), CC_ARGHACK);
    assert_eq!(el.el_chared.c_vcmd.action, YANK);

    assert_eq!(vi_next_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "foo bar");
    assert_eq!(killed(&el), "foo ");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
}

// [spec:libedit:sem:vi.vi-comment-out-fn/test]
/// `#` prefixes the line and submits it in one keystroke, so the text
/// lands in the history without being run. No undo snapshot is taken, so
/// `u` cannot recover from a mistyped `#`, and the `CC_NEWLINE` means
/// there is no chance to.
#[test]
fn comment_out_prefixes_the_line_and_submits_it() {
    let mut el = el_with("ls", 2);
    assert_eq!(vi_comment_out(&mut el, 0), CC_NEWLINE);
    assert_eq!(text(&el), "#ls\n");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_chared.c_undo.len, -1);
}

/// The `EL_ALIAS_TEXT` hook a test installs: `_a` expands, everything else
/// is unknown.
unsafe extern "C" fn alias_hook(_arg: *mut c_void, name: *const c_char) -> *const c_char {
    // SAFETY: `vi_alias` passes its three-element local, NUL-terminated
    // and live across the call, which is `el_afunc_t`'s contract.
    let name = unsafe { CStr::from_ptr(name) };
    if name.to_bytes() == b"_a" {
        c"expanded".as_ptr()
    } else {
        ptr::null()
    }
}

// [spec:libedit:sem:vi.vi-alias-fn/test]
/// `_x` reads one more character, asks the application what it expands to,
/// and pushes the answer back as input rather than inserting it — so the
/// expansion goes through the keymap and can contain commands.
/// ERR-modes-65: the keymap is deliberately *not* switched to insert mode,
/// against POSIX.
#[test]
fn an_alias_expansion_is_pushed_back_as_input() {
    let mut el = el_with("", 0);
    read_init(&mut el);
    el.el_chared.c_aliasfun = Some(alias_hook);
    feed(&mut el, "a");

    assert_eq!(vi_alias(&mut el, 0), CC_NORM);
    assert_eq!(drain(&mut el, 8), "expanded");
    assert!(el.el_map.current == ElMapCurrent::Alt);
}

// [spec:libedit:sem:vi.vi-alias-fn/test]
/// With no hook installed there is nothing to ask, and an alias the
/// application does not recognise is silently ignored rather than being an
/// error — the two cases return different things for the same visible
/// outcome.
#[test]
fn an_unknown_alias_is_ignored_rather_than_refused() {
    let mut el = el_with("", 0);
    read_init(&mut el);
    assert_eq!(vi_alias(&mut el, 0), CC_ERROR);

    let mut el = el_with("", 0);
    read_init(&mut el);
    el.el_chared.c_aliasfun = Some(alias_hook);
    feed(&mut el, "z");
    assert_eq!(vi_alias(&mut el, 0), CC_NORM);
    assert_eq!(
        el.el_read.as_ref().unwrap().macros.level,
        -1,
        "nothing was pushed"
    );
}

/// Where [`redo_probe`] records the character it was dispatched with.
static REDO_CH: AtomicU64 = AtomicU64::new(0);

/// A stand-in for one of `EL_FUNC`'s shims, so a test can see which entry
/// `vi_redo` reached and with what.
unsafe extern "C" fn redo_probe(_el: *mut EditLine, ch: u32) -> ElActionT {
    REDO_CH.store(u64::from(ch), Ordering::Relaxed);
    CC_NORM
}

/// The same, for a test that does not care which character arrived. It
/// records nothing, so the two tests cannot race over [`REDO_CH`] when the
/// harness runs them on different threads.
unsafe extern "C" fn redo_sink(_el: *mut EditLine, _ch: u32) -> ElActionT {
    CC_NORM
}

// [spec:libedit:sem:vi.vi-redo-fn/test]
/// `.` restores the recorded count, operator and invoking character, then
/// dispatches the recorded command through the keymap's function table.
/// The recorded insert-mode text is pushed *before* the command runs, so a
/// replayed command that reads a character of its own consumes it from the
/// pushback rather than from the terminal.
#[test]
fn redo_replays_the_recorded_command_through_the_function_table() {
    REDO_CH.store(0, Ordering::Relaxed);
    let mut el = el_with("abcdef", 3);
    read_init(&mut el);
    el.el_map.func = vec![redo_probe as ElFuncT; 4];
    el.el_chared.c_redo.cmd = 2;
    el.el_chared.c_redo.ch = u32::from(b'q');
    el.el_chared.c_redo.count = 7;
    el.el_chared.c_redo.action = DELETE;
    el.el_chared.c_redo.buf[..2].copy_from_slice(&[u32::from(b'x'), u32::from(b'y')]);
    el.el_chared.c_redo.pos = 2;

    assert_eq!(vi_redo(&mut el, 0), CC_NORM);
    assert_eq!(REDO_CH.load(Ordering::Relaxed), u64::from(b'q'));
    assert_eq!(el.el_state.argument, 7);
    assert_eq!(el.el_state.doingarg, 1);
    assert_eq!(el.el_state.thiscmd, 2);
    assert_eq!(el.el_state.thisch, u32::from(b'q'));
    assert_eq!(el.el_chared.c_vcmd.action, DELETE);
    assert_eq!(el.el_chared.c_vcmd.pos, 3, "re-anchored at the cursor");
    assert_eq!(drain(&mut el, 2), "xy");
}

// [spec:libedit:sem:vi.vi-redo-fn/test]
/// A count typed on the `.` itself overrides the recorded one — the
/// recorded count is only consulted when none is in progress.
#[test]
fn a_count_on_the_dot_overrides_the_recorded_one() {
    let mut el = el_with("abcdef", 3);
    read_init(&mut el);
    el.el_map.func = vec![redo_sink as ElFuncT; 4];
    el.el_chared.c_redo.cmd = 1;
    el.el_chared.c_redo.count = 7;
    el.el_state.doingarg = 1;
    el.el_state.argument = 2;

    assert_eq!(vi_redo(&mut el, 0), CC_NORM);
    assert_eq!(el.el_state.argument, 2);
}

/// The stored search pattern as `patlen` describes it.
fn pattern(el: &EditLine) -> String {
    el.el_search.patbuf[..el.el_search.patlen]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// A history stash of the size `hist_init` gives it, which is what the
/// save-and-restore paths write through.
fn with_stash(el: &mut EditLine) {
    el.el_history.buf = vec![0u32; EL_BUFSIZ];
    el.el_history.sz = EL_BUFSIZ;
    el.el_history.last = 0;
}

/// The stashed live line as `el_history.last` describes it.
fn stash(el: &EditLine) -> String {
    el.el_history.buf[..el.el_history.last]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

// [spec:libedit:sem:vi.vi-search-prev-fn/test]
// [spec:libedit:sem:vi.vi-search-next-fn/test]
/// ERR-modes-67: `/` searches *backwards* through the history, towards
/// older entries, and `?` forwards — the opposite of what the prompt
/// characters suggest. The typed text is wrapped in `".*"` at both ends
/// and becomes the stored pattern, and the line the user was editing is
/// destroyed by the prompt the moment the key is pressed: before the
/// search runs, and whether or not it finds anything.
#[test]
fn the_history_searches_wrap_the_pattern_and_eat_the_line() {
    for (search, dir) in [
        (
            vi_search_prev as fn(&mut EditLine, u32) -> ElActionT,
            ED_SEARCH_PREV_HISTORY,
        ),
        (vi_search_next, ED_SEARCH_NEXT_HISTORY),
    ] {
        let mut el = el_with("editing this", 5);
        read_init(&mut el);
        with_stash(&mut el);
        feed(&mut el, "foo\r");

        // No history store is attached, so the search cannot succeed;
        // everything below has already happened by then.
        assert_eq!(search(&mut el, 0), CC_ERROR);
        assert_eq!(el.el_search.patdir, i32::from(dir));
        assert_eq!(pattern(&el), ".*foo.*");
        assert_eq!(el.el_line.lastchar, 0);
    }
}

// [spec:libedit:sem:vi.vi-to-history-line-fn/test]
/// `G` stashes the live line before it moves, so that a later return to
/// event 0 finds what the user was typing. Without a count it asks for the
/// oldest entry by aiming at INT_MAX and letting `hist_get`'s clamp settle
/// where the history really ends; a failure on that path puts the event
/// number back where it was.
#[test]
fn to_history_line_stashes_the_live_line_and_restores_on_failure() {
    let mut el = el_with("abc", 2);
    with_stash(&mut el);
    el.el_history.eventno = 0;

    assert_eq!(vi_to_history_line(&mut el, 0), CC_ERROR);
    assert_eq!(stash(&el), "abc");
    assert_eq!(el.el_history.eventno, 0, "restored");
}

// [spec:libedit:sem:vi.vi-to-history-line-fn/test]
/// With a count `G` has to read `ev.num` first, because the count runs the
/// other way round from every other history index here — it matches
/// `fc -l`. The first `hist_get` exists only to populate that cookie, and
/// when it fails the event number is left at 1 rather than restored: the
/// only path out of this function that does not put it back.
#[test]
fn a_counted_to_history_line_leaves_the_event_number_at_one_on_failure() {
    let mut el = el_with("abc", 2);
    with_stash(&mut el);
    el.el_history.eventno = 0;
    el.el_state.doingarg = 1;
    el.el_state.argument = 2;

    assert_eq!(vi_to_history_line(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_history.eventno, 1);
}

/// The single entry [`hist_hook`] answers `H_FIRST` with, NUL-terminated
/// because that is how `hist_fun` measures what the store hands back.
fn hist_entry() -> &'static [u32] {
    static E: OnceLock<Vec<u32>> = OnceLock::new();
    E.get_or_init(|| "one two three\0".chars().map(u32::from).collect())
}

/// A stand-in for `history_w`. Variadic because `hist_fun_t` is: libedit
/// calls through it with zero or one trailing argument depending on the
/// operation, and only `H_FIRST` is needed here.
unsafe extern "C" fn hist_hook(_ref: *mut c_void, ev: *mut HistEventW, op: c_int, _: ...) -> c_int {
    if op != H_FIRST {
        return -1;
    }
    // SAFETY: `hist_fun` passes `&mut el.el_history.ev`, which outlives
    // the call, and the entry string is `'static`.
    unsafe {
        (*ev).num = 1;
        (*ev).str = hist_entry().as_ptr();
    }
    0
}

// [spec:libedit:sem:vi.vi-history-word-fn/test]
/// ERR-modes-61: `_` appends a word from the previous history entry, which
/// is libedit's own invention — in real vi `_` is a line motion. With no
/// count it takes the *last* word; with one it counts words from the left.
/// A separating space is inserted before the word and the cursor is left
/// just past it, in insert mode.
#[test]
fn history_word_appends_a_word_from_the_previous_entry() {
    let mut el = el_with("ab", 1);
    el.el_history.fun = Some(hist_hook);
    assert_eq!(vi_history_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "ab three");
    assert_eq!(el.el_line.cursor, 8);
    assert!(el.el_map.current == ElMapCurrent::Key);
    assert_eq!(
        el.el_chared.c_undo.len, 2,
        "the pre-append line is undoable"
    );

    let mut el = el_with("ab", 1);
    el.el_history.fun = Some(hist_hook);
    el.el_state.doingarg = 1;
    el.el_state.argument = 2;
    assert_eq!(vi_history_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "ab two");
}

// [spec:libedit:sem:vi.vi-history-word-fn/test]
/// A count larger than the number of words on the entry fails, and fails
/// clean: the scan runs to the end of the entry and the check that the
/// count was fully consumed happens before anything is modified.
#[test]
fn history_word_refuses_a_count_the_entry_cannot_satisfy() {
    let mut el = el_with("ab", 1);
    el.el_history.fun = Some(hist_hook);
    el.el_state.doingarg = 1;
    el.el_state.argument = 9;

    assert_eq!(vi_history_word(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "ab");
    assert_eq!(el.el_chared.c_undo.len, -1);

    // No history at all is the other refusal.
    let mut el = el_with("ab", 1);
    assert_eq!(vi_history_word(&mut el, 0), CC_ERROR);
}

/// An `EL_GETENV` hook that names a command which edits nothing, so the
/// round trip through the temporary file is the only thing under test.
unsafe extern "C" fn getenv_true(_name: *const c_char) -> *mut c_char {
    c"/bin/true".as_ptr().cast_mut()
}

// [spec:libedit:sem:vi.vi-histedit-fn/test]
/// `v` writes the line to a temporary file, runs `$EDITOR` over it, reads
/// it back and **submits it immediately** — the C carries a commented-out
/// `return CC_REFRESH` showing the alternative it did not take. The
/// trailing newline written to the file is stripped on the way back in and
/// then put straight back by `ed_newline`, so an editor that changes
/// nothing round-trips the line exactly. No undo snapshot is taken and the
/// keymap is unchanged, so there is no way back from a mistaken `v`.
#[test]
fn histedit_round_trips_the_line_through_the_editor_and_submits_it() {
    let mut el = el_with("echo hi", 3);
    el.el_getenv = Some(getenv_true);

    assert_eq!(vi_histedit(&mut el, 0), CC_NEWLINE);
    assert_eq!(text(&el), "echo hi\n");
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_chared.c_undo.len, -1);
    assert!(el.el_map.current == ElMapCurrent::Alt);
}
