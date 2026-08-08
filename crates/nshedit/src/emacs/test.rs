use super::*;
use crate::chared::MODE_REPLACE_1;
use crate::map::map_init_emacs;
use crate::testkit::{headless_editor, killed, set_line, text};

/// The shared editor under the emacs bindings, with `s` in the line and the
/// cursor at `at`.
///
/// `headless_editor` leaves the shipped default, vi insert mode, so
/// `map_init_emacs` runs over the top of it. That call is also the only thing
/// that installs `el_map.wordchars` — the `*?_-.[]~=` set `ce__isword`
/// consults, under which a `.` sits *inside* a word — so a fixture without it
/// would be testing an editor `el_init` never produces.
fn el_with(s: &str, at: usize) -> EditLine {
    let mut el = headless_editor(80, 24);
    map_init_emacs(&mut el);
    set_line(&mut el, s, at);
    el
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

    el.el_state.inputmode = MODE_REPLACE_1;
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
