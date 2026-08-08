use super::*;
use crate::chared::DELETE;
use crate::map::map_init_emacs;
use crate::read::el_wpush;
use crate::testkit::{headless_editor, killed, set_line, text};

/// The shared editor under the emacs bindings, with `s` in the line and
/// the cursor at `at`.
///
/// `headless_editor` leaves the shipped default, vi insert mode, so
/// `map_init_emacs` runs over the top of it. That call is also the only
/// thing that installs `el_map.wordchars` — the `*?_-.[]~=` set every word
/// test here consults, and under which `.` is a word character — so a
/// fixture without it would be testing an editor `el_init` never produces.
fn el_with(s: &str, at: usize) -> EditLine {
    let mut el = headless_editor(80, 24);
    map_init_emacs(&mut el);
    set_line(&mut el, s, at);
    el
}

/// The stashed live line as `el_history.last` describes it.
fn stash(el: &EditLine) -> String {
    el.el_history.buf[..el.el_history.last]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// Queue `s` as terminal input. `el_wgetc` drains the macro queue before
/// it ever touches the tty, so a test never has to own a terminal.
fn feed(el: &mut EditLine, s: &str) {
    let chars: Vec<u32> = s.chars().map(u32::from).collect();
    el_wpush(el, Some(&chars));
}

// [spec:libedit:sem:common.ed-end-of-file-fn/test]
/// `^D` on an empty line writes the terminator and nothing else: the
/// cursor and `lastchar` are where they were, so the caller sees the line
/// exactly as the user left it alongside the end-of-input report.
#[test]
fn end_of_file_terminates_the_line_without_editing_it() {
    let mut el = el_with("abc", 1);
    // The slot at `lastchar` holds whatever earlier editing left there;
    // only a sentinel can tell "wrote a NUL" from "was already NUL".
    el.el_line.buffer[3] = u32::from(b'Z');

    assert_eq!(ed_end_of_file(&mut el, 0), CC_EOF);
    assert_eq!(el.el_line.buffer[3], 0);
    assert_eq!(el.el_line.cursor, 1);
    assert_eq!(el.el_line.lastchar, 3);
    assert_eq!(text(&el), "abc");
}

// [spec:libedit:sem:common.ed-delete-prev-word-fn/test]
/// `^W` cuts back to the start of the word before the cursor and leaves
/// the cursor there. The word test is `ce_is_word`, which consults
/// `el_map.wordchars` — so under the emacs set a `.` is *inside* the word
/// and the whole of `ab.cd` goes, which is the shape of ERR-modes-53.
#[test]
fn delete_prev_word_cuts_back_to_the_start_of_the_word() {
    let mut el = el_with("foo bar", 7);
    assert_eq!(ed_delete_prev_word(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "foo ");
    assert_eq!(el.el_line.cursor, 4);
    // ERR-modes-47: the hand-rolled copy and `c_delbefore`'s `cv_yank`
    // write the identical span, so only the content is observable.
    assert_eq!(killed(&el), "bar");
    // ERR-modes-19: `c_delbefore`'s keymap test is a tautology, so an
    // emacs-mode delete still leaves a vi undo snapshot behind.
    assert_eq!(el.el_chared.c_undo.len, 7);

    let mut el = el_with("ab.cd", 5);
    ed_delete_prev_word(&mut el, 0);
    assert_eq!(text(&el), "", "`.` is in the emacs word set");
}

// [spec:libedit:sem:common.ed-delete-prev-word-fn/test]
/// At the head of the line there is no previous word, and the refusal is
/// total — no kill, no undo snapshot.
#[test]
fn delete_prev_word_at_the_head_of_the_line_changes_nothing() {
    let mut el = el_with("foo", 0);
    assert_eq!(ed_delete_prev_word(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "foo");
    assert_eq!(el.el_chared.c_kill.last, 0);
    assert_eq!(el.el_chared.c_undo.len, -1);
}

// [spec:libedit:sem:common.ed-delete-next-char-fn/test]
/// The end-of-line guard is the only mode-dependent part: emacs refuses,
/// vi steps back onto the last character and deletes that instead, which
/// is what makes vi `x` work at the end of a line. An empty line refuses
/// in both.
#[test]
fn delete_next_char_at_end_of_line_only_works_in_vi() {
    let mut el = el_with("abc", 3);
    assert_eq!(ed_delete_next_char(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "abc");

    let mut el = el_with("abc", 3);
    el.el_map.r#type = MAP_VI;
    assert_eq!(ed_delete_next_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "ab");
    // The vi fix-up pulls the cursor back onto the new last character
    // rather than leaving it at `lastchar`.
    assert_eq!(el.el_line.cursor, 1);

    let mut el = el_with("", 0);
    el.el_map.r#type = MAP_VI;
    assert_eq!(ed_delete_next_char(&mut el, 0), CC_ERROR);
}

// [spec:libedit:sem:common.ed-delete-next-char-fn/test]
/// The count is clamped by `c_delafter`, so an over-large one deletes to
/// the end of the line instead of failing, and ERR-modes-19 means the
/// removed text lands in the kill buffer even under the emacs keymap.
#[test]
fn delete_next_char_clamps_its_count_and_still_fills_the_kill_buffer() {
    let mut el = el_with("abcdef", 2);
    el.el_state.argument = 99;
    assert_eq!(ed_delete_next_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "ab");
    assert_eq!(killed(&el), "cdef");
    assert_eq!(el.el_line.cursor, 2, "emacs leaves the cursor at the end");

    let mut el = el_with("abcdef", 2);
    el.el_map.r#type = MAP_VI;
    el.el_state.argument = 99;
    ed_delete_next_char(&mut el, 0);
    assert_eq!(el.el_line.cursor, 1, "vi pulls it back onto a character");
}

// [spec:libedit:sem:common.ed-quoted-insert-fn/test]
/// `^V` delivers the next character literally, and it goes through
/// `ed_insert` — so the repeat count in force applies to it, which is not
/// obvious from the name.
#[test]
fn quoted_insert_inserts_the_next_character_argument_times() {
    let mut el = el_with("ab", 2);
    el_wpush(&mut el, Some(&[u32::from(b'\t')]));
    el.el_state.argument = 3;

    assert_eq!(ed_quoted_insert(&mut el, 0), CC_NORM);
    assert_eq!(text(&el), "ab\t\t\t");
    assert_eq!(el.el_line.cursor, 5);
}

// [spec:libedit:sem:common.ed-digit-fn/test]
/// A digit is only a count while one is being entered; otherwise it is
/// ordinary text. The `EM_UNIVERSAL_ARGUMENT` case *replaces* the
/// accumulated count rather than appending to it, so `^U 5` means five and
/// not twenty-something.
#[test]
fn a_digit_is_text_unless_a_count_is_being_entered() {
    let mut el = el_with("x", 1);
    assert_eq!(ed_digit(&mut el, u32::from(b'7')), CC_NORM);
    assert_eq!(text(&el), "x7");

    let mut el = el_with("", 0);
    el.el_state.doingarg = 1;
    el.el_state.argument = 4;
    assert_eq!(ed_digit(&mut el, u32::from(b'2')), CC_ARGHACK);
    assert_eq!(el.el_state.argument, 42);

    let mut el = el_with("", 0);
    el.el_state.doingarg = 1;
    el.el_state.argument = 4;
    el.el_state.lastcmd = EM_UNIVERSAL_ARGUMENT;
    assert_eq!(ed_digit(&mut el, u32::from(b'2')), CC_ARGHACK);
    assert_eq!(el.el_state.argument, 2);

    let mut el = el_with("", 0);
    assert_eq!(ed_digit(&mut el, u32::from(b'z')), CC_ERROR);
}

// [spec:libedit:sem:common.ed-digit-fn/test]
// [spec:libedit:sem:common.ed-argument-digit-fn/test]
/// ERR-modes-49: both accumulators test the cap *before* multiplying, so a
/// count of exactly 1000000 is allowed to grow to eight digits. 10000009
/// is the largest value either can hold, and the next digit is refused
/// outright rather than saturating.
#[test]
fn the_count_cap_is_tested_before_the_multiply_so_it_overshoots() {
    for accumulate in [ed_digit, ed_argument_digit] {
        let mut el = el_with("", 0);
        el.el_state.doingarg = 1;
        el.el_state.argument = 1_000_000;
        assert_eq!(accumulate(&mut el, u32::from(b'9')), CC_ARGHACK);
        assert_eq!(el.el_state.argument, 10_000_009);

        assert_eq!(accumulate(&mut el, u32::from(b'0')), CC_ERROR);
        assert_eq!(el.el_state.argument, 10_000_009, "left where it was");
    }
}

// [spec:libedit:sem:common.ed-argument-digit-fn/test]
/// The first digit *replaces* the count and turns `doingarg` on; later
/// ones append. There is no `EM_UNIVERSAL_ARGUMENT` special case here and
/// no fall-through to `ed_insert`, which is the whole difference from
/// `ed_digit`.
#[test]
fn argument_digit_starts_a_count_and_never_inserts_text() {
    let mut el = el_with("x", 1);
    assert_eq!(ed_argument_digit(&mut el, u32::from(b'0')), CC_ARGHACK);
    assert_eq!(el.el_state.argument, 0, "a leading zero really means zero");
    assert_eq!(el.el_state.doingarg, 1);
    assert_eq!(text(&el), "x", "nothing was inserted");

    assert_eq!(ed_argument_digit(&mut el, u32::from(b'5')), CC_ARGHACK);
    assert_eq!(el.el_state.argument, 5);

    // Where `ed_digit` would have replaced the count with the digit.
    el.el_state.lastcmd = EM_UNIVERSAL_ARGUMENT;
    assert_eq!(ed_argument_digit(&mut el, u32::from(b'3')), CC_ARGHACK);
    assert_eq!(el.el_state.argument, 53);

    assert_eq!(ed_argument_digit(&mut el, u32::from(b'z')), CC_ERROR);
}

// [spec:libedit:sem:common.ed-unassigned-fn/test]
// [spec:libedit:sem:common.ed-ignore-fn/test]
// [spec:libedit:sem:common.ed-sequence-lead-in-fn/test]
// [spec:libedit:sem:common.ed-redisplay-fn/test]
/// The four commands that do nothing at all differ only in what they ask
/// the dispatcher to do next: beep, stay silent, stay silent, or repaint.
/// `ed_ignore` and `ed_sequence_lead_in` are behaviourally identical and
/// exist separately only so `bind` can name them apart.
#[test]
fn the_do_nothing_commands_differ_only_in_their_return_value() {
    for (command, expected) in [
        (
            ed_unassigned as fn(&mut EditLine, u32) -> ElActionT,
            CC_ERROR,
        ),
        (ed_ignore, CC_NORM),
        (ed_sequence_lead_in, CC_NORM),
        (ed_redisplay, CC_REDISPLAY),
    ] {
        let mut el = el_with("abc", 1);
        el.el_state.argument = 4;
        assert_eq!(command(&mut el, u32::from(b'q')), expected);
        assert_eq!(text(&el), "abc");
        assert_eq!(el.el_line.cursor, 1);
        assert_eq!(el.el_state.argument, 4);
        assert_eq!(el.el_chared.c_undo.len, -1);
    }
}

// [spec:libedit:sem:common.ed-delete-prev-char-fn/test]
/// Backspace deletes `argument` characters and drags the cursor back over
/// them. ERR-modes-04: when the count overshoots, `c_delbefore`'s clamp
/// and the cursor's saturation agree on the head of the line — the C gets
/// there by forming a pointer below the buffer and clamping afterwards.
#[test]
fn delete_prev_char_moves_the_cursor_back_over_what_it_removed() {
    let mut el = el_with("abcdef", 4);
    el.el_state.argument = 2;
    assert_eq!(ed_delete_prev_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "abef");
    assert_eq!(el.el_line.cursor, 2);
    // ERR-modes-19 again: the emacs keymap does not suppress the yank.
    assert_eq!(killed(&el), "cd");

    let mut el = el_with("abcdef", 3);
    el.el_state.argument = 99;
    assert_eq!(ed_delete_prev_char(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "def");
    assert_eq!(el.el_line.cursor, 0);

    let mut el = el_with("abc", 0);
    assert_eq!(ed_delete_prev_char(&mut el, 0), CC_ERROR);
    assert_eq!(text(&el), "abc");
}

// [spec:libedit:sem:common.ed-clear-screen-fn/test]
/// `^L` forgets the on-screen image as well as clearing the terminal, so
/// the `CC_REFRESH` that follows repaints from scratch instead of diffing
/// against rows that are no longer displayed.
#[test]
fn clear_screen_discards_the_remembered_screen_image() {
    let mut el = el_with("abc", 1);
    for row in &mut el.el_display {
        row[0] = u32::from(b'X');
    }
    el.el_refresh.r_oldcv = 5;
    el.el_cursor.v = 3;

    assert_eq!(ed_clear_screen(&mut el, 0), CC_REFRESH);
    assert!(el.el_display.iter().all(|row| row[0] == 0));
    assert_eq!(el.el_refresh.r_oldcv, 0);
    assert_eq!(el.el_cursor.v, 0);
    assert_eq!(text(&el), "abc", "the line itself is untouched");
}

// [spec:libedit:sem:common.ed-start-over-fn/test]
/// `^G` resets the editor but deliberately keeps the kill buffer's
/// *contents* — only the mark goes back to the head of the line — so a
/// yank after `^G` still pastes the pre-`^G` kill.
#[test]
fn start_over_resets_the_editor_but_not_the_kill_buffer() {
    let mut el = el_with("abcdef", 4);
    el.el_chared.c_kill.buf[..3].copy_from_slice(&[
        u32::from(b'x'),
        u32::from(b'y'),
        u32::from(b'z'),
    ]);
    el.el_chared.c_kill.last = 3;
    el.el_chared.c_kill.mark = 4;
    el.el_chared.c_vcmd.action = DELETE;
    el.el_state.argument = 7;
    el.el_state.doingarg = 1;
    el.el_history.eventno = 3;

    assert_eq!(ed_start_over(&mut el, 0), CC_REFRESH);
    assert_eq!(killed(&el), "xyz");
    assert_eq!(el.el_chared.c_kill.mark, 0);
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
    assert_eq!(el.el_line.lastchar, 0);
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_state.argument, 1);
    assert_eq!(el.el_state.doingarg, 0);
    assert_eq!(el.el_history.eventno, 0);
}

// [spec:libedit:sem:common.ed-prev-line-fn/test]
// [spec:libedit:sem:common.ed-next-line-fn/test]
/// These move between lines *embedded in the edit buffer*, not between
/// history entries, and they aim for the column the cursor is already in —
/// stopping at the target line's own end when it is shorter.
#[test]
fn the_line_motions_keep_the_column_or_stop_at_the_line_end() {
    // Column 5 of the second line; the first line has only three
    // characters, so the cursor lands on its terminating newline.
    let mut el = el_with("abc\ndefgh", 9);
    assert_eq!(ed_prev_line(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 3);

    // Column 1 of the first line, straight down to column 1 of the second.
    let mut el = el_with("abc\ndefgh", 1);
    assert_eq!(ed_next_line(&mut el, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 5);
}

// [spec:libedit:sem:common.ed-prev-line-fn/test]
// [spec:libedit:sem:common.ed-next-line-fn/test]
/// With no line to move to the cursor stays put and the command errors.
/// `el_state.argument` is left partially decremented, which is harmless
/// only because the dispatcher resets it after every command.
#[test]
fn the_line_motions_refuse_when_the_buffer_has_only_one_line() {
    let mut el = el_with("abc", 1);
    assert_eq!(ed_prev_line(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_line.cursor, 1);

    let mut el = el_with("abc", 1);
    assert_eq!(ed_next_line(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_line.cursor, 1);
}

// [spec:libedit:sem:common.ed-prev-line-fn/test]
// [spec:libedit:sem:common.ed-next-line-fn/test]
/// ERR-modes-02 and ERR-modes-20, defined rather than reproduced: in the C
/// a non-positive count walks the scan off one end of the buffer and then
/// dereferences the position it lands on, because the "not enough lines"
/// guard tests `argument > 0` and a count that started at zero never
/// exceeds it. Reachable only by binding these and prefixing them with
/// `ESC 0`. Here they simply do not move, and the count is left alone
/// instead of being partially consumed.
#[test]
fn a_zero_count_moves_neither_line_motion_off_the_buffer() {
    for motion in [
        ed_prev_line as fn(&mut EditLine, u32) -> ElActionT,
        ed_next_line,
    ] {
        let mut el = el_with("abc\ndef", 5);
        el.el_state.argument = 0;
        assert_eq!(motion(&mut el, 0), CC_CURSOR);
        assert_eq!(el.el_line.cursor, 5);
        assert_eq!(el.el_state.argument, 0);
    }
}

// [spec:libedit:sem:common.ed-command-fn/test]
/// `:` reads one editline command over the top of whatever the user was
/// editing and never puts it back — `c_gets` clears the line on every exit
/// path, so invoking this destroys the line whether or not the command
/// parses. It also forces the primary keymap back on, which in vi is what
/// leaves command mode, and forgets the screen image because the command's
/// own output may have scrolled it.
#[test]
fn the_command_prompt_destroys_the_line_and_leaves_command_mode() {
    let mut el = el_with("some line", 4);
    // Unrecognised, so `parse_line` answers -1 and this beeps; that is
    // still the `CC_REFRESH` path.
    feed(&mut el, "nosuchcommand\r");
    el.el_map.current = ElMapCurrent::Alt;
    el.el_refresh.r_oldcv = 4;

    assert_eq!(ed_command(&mut el, 0), CC_REFRESH);
    assert_eq!(el.el_line.lastchar, 0);
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_map.current, ElMapCurrent::Key);
    assert_eq!(el.el_refresh.r_oldcv, 0);
}

// [spec:libedit:sem:common.ed-search-prev-history-fn/test]
/// The backward search drops any pending vi operator and invalidates the
/// undo snapshot before it does anything else, and it stashes the live
/// line on the way off event 0 so that walking back to it restores what
/// the user was typing. ERR-history-10: `last` is the recorded length,
/// because the C's `wcsncpy` leaves no terminator when the line fills the
/// stash.
#[test]
fn searching_backwards_stashes_the_live_line_first() {
    let mut el = el_with("hello", 5);
    el.el_chared.c_vcmd.action = DELETE;
    el.el_chared.c_undo.len = 3;

    // No history store is attached, so the search itself cannot run —
    // everything above happens first regardless.
    assert_eq!(ed_search_prev_history(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
    assert_eq!(el.el_chared.c_undo.len, -1);
    assert_eq!(stash(&el), "hello");
    assert_eq!(el.el_history.last, 5);
}

// [spec:libedit:sem:common.ed-search-prev-history-fn/test]
/// A negative event number is a state that should not happen; it is
/// repaired to 0 and reported as an error rather than searched from, and
/// the repair happens before the live line would have been stashed.
#[test]
fn a_negative_event_number_is_repaired_rather_than_searched() {
    let mut el = el_with("hello", 5);
    el.el_history.eventno = -1;

    assert_eq!(ed_search_prev_history(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_history.eventno, 0);
    assert_eq!(el.el_history.last, 0, "the stash was never reached");
}

// [spec:libedit:sem:common.ed-search-next-history-fn/test]
/// The forward search refuses at event 0 — there is nothing newer than the
/// line being typed — and, unlike the backward form, it never stashes the
/// live line at all: only `ed_prev_history` and `ed_search_prev_history`
/// do that. The vi operator and the undo snapshot are still cleared first.
#[test]
fn searching_forwards_refuses_at_the_live_line_and_saves_nothing() {
    let mut el = el_with("hello", 5);
    el.el_chared.c_vcmd.action = DELETE;
    el.el_chared.c_undo.len = 3;

    assert_eq!(ed_search_next_history(&mut el, 0), CC_ERROR);
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
    assert_eq!(el.el_chared.c_undo.len, -1);
    assert_eq!(el.el_history.last, 0);

    // Somewhere in the history, but still with no store attached.
    el.el_history.eventno = 2;
    assert_eq!(ed_search_next_history(&mut el, 0), CC_ERROR);
}
