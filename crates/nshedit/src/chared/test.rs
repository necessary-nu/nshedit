use super::*;
use crate::el::blank_editline;
use crate::read::el_wpush;
use crate::testkit::{headless_editor, set_line};

/// The shared editor holding `s`, cursor at `at`.
fn editor(s: &str, at: usize) -> EditLine {
    let mut el = headless_editor(80, 24);
    set_line(&mut el, s, at);
    el
}

fn text(el: &EditLine) -> String {
    el.el_line.buffer[..el.el_line.lastchar]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// `cv__isword` is THREE-valued and the two nonzero values are not
/// interchangeable: `cv_next_word`, `cv_prev_word` and `cv__endword`
/// compare it for EQUALITY to find runs of one class, which is how vi's
/// `w`, `b` and `e` stop at the boundary between a word and adjacent
/// punctuation. A predicate that merely returned "truthy" would pass every
/// differential trace and break that stop.
// [spec:libedit:sem:chared.cv-isword-fn/test]
#[test]
fn the_vi_word_test_separates_words_from_punctuation() {
    let mut el = blank_editline();
    assert_eq!(cv__isword(&mut el, u32::from(b'a')), 1);
    assert_eq!(cv__isword(&mut el, u32::from(b'7')), 1);
    assert_eq!(
        cv__isword(&mut el, u32::from(b'.')),
        2,
        "punctuation is its own class"
    );
    assert_eq!(cv__isword(&mut el, u32::from(b'-')), 2);
    assert_eq!(cv__isword(&mut el, u32::from(b' ')), 0);
    assert_eq!(cv__isword(&mut el, u32::from(b'\t')), 0);
    assert_ne!(
        cv__isword(&mut el, u32::from(b'a')),
        cv__isword(&mut el, u32::from(b'.')),
        "the equality the walkers rely on must distinguish these"
    );
}

/// `cv__isWord` is the coarse sibling — exactly 0 or 1 — which is what
/// makes vi's `W`, `B` and `E` treat punctuation as part of the
/// surrounding word where the lowercase forms split it out.
// [spec:libedit:sem:chared.cv-is-word-fn/test]
#[test]
fn the_vi_big_word_test_puts_everything_non_space_in_one_class() {
    let mut el = blank_editline();
    for &c in b"a7.-/" {
        assert_eq!(cv__isWord(&mut el, u32::from(c)), 1, "{}", c as char);
    }
    for &c in b" \t\n" {
        assert_eq!(cv__isWord(&mut el, u32::from(c)), 0, "{}", c as char);
    }
}

/// `ce__isword` is the emacs test and is a `||`, so it is exactly 0 or 1
/// and never the raw `iswalnum` value — which the C's `c__next_word` and
/// `c__prev_word` use as a boolean and nothing compares for equality.
// [spec:libedit:sem:chared.ce-isword-fn/test]
#[test]
fn the_emacs_word_test_is_boolean() {
    let mut el = blank_editline();
    assert_eq!(ce__isword(&mut el, u32::from(b'a')), 1);
    assert_eq!(
        ce__isword(&mut el, u32::from(b'.')),
        0,
        "punctuation is not a word"
    );
    assert_eq!(ce__isword(&mut el, u32::from(b' ')), 0);
}

/// `c_hpos` is the column within the current line, so it counts back to
/// the last newline and not to the start of the buffer. A buffer with an
/// embedded newline is what tells the two apart.
// [spec:libedit:sem:chared.c-hpos-fn/test]
#[test]
fn hpos_is_the_column_within_the_line_not_the_buffer() {
    let mut el = editor("abc", 0);
    assert_eq!(c_hpos(&mut el), 0);
    el.el_line.cursor = 3;
    assert_eq!(c_hpos(&mut el), 3);

    let mut el = editor("abc\ndefgh", 9);
    assert_eq!(c_hpos(&mut el), 5, "counts from after the newline");
    el.el_line.cursor = 4;
    assert_eq!(c_hpos(&mut el), 0, "just past the newline is column zero");
    el.el_line.cursor = 3;
    assert_eq!(
        c_hpos(&mut el),
        3,
        "the newline itself ends the previous line"
    );
}

/// `c_delafter` clamps to what is actually there rather than deleting
/// past `lastchar`, and a count of zero is a no-op.
// [spec:libedit:sem:chared.c-delafter-fn/test]
#[test]
fn delete_after_clamps_to_the_end_of_the_line() {
    let mut el = editor("abcdef", 2);
    c_delafter(&mut el, 2);
    assert_eq!(text(&el), "abef");
    assert_eq!(el.el_line.cursor, 2, "the cursor does not move");

    // More than remains: clamped, not overrun.
    c_delafter(&mut el, 99);
    assert_eq!(text(&el), "ab");
    assert_eq!(el.el_line.lastchar, 2);

    c_delafter(&mut el, 0);
    assert_eq!(text(&el), "ab");
}

/// `c_delafter1` and `c_delbefore1` are the single-character forms, and
/// ERR-buffer-02 and -03 record that neither guards its end of the
/// buffer in the C — both callers happen to check first. Whatever they do
/// here, they must not corrupt the line.
// [spec:libedit:sem:chared.c-delafter1-fn/test]
// [spec:libedit:sem:chared.c-delbefore1-fn/test]
#[test]
fn the_single_character_deletes_stay_inside_the_buffer() {
    let mut el = editor("abc", 1);
    c_delafter1(&mut el);
    assert_eq!(text(&el), "ac");

    let mut el = editor("abc", 1);
    c_delbefore1(&mut el);
    assert_eq!(text(&el), "bc");
    // Neither form touches `cursor`: the text slides left underneath it
    // and the caller decrements to stay on the same character. Asserting 0
    // here was reading emacs-mode backspace into the primitive.
    assert_eq!(el.el_line.cursor, 1, "the cursor is the caller's to adjust");

    // The unguarded ends: cursor at the very start and the very end.
    let mut el = editor("abc", 0);
    c_delbefore1(&mut el);
    assert!(el.el_line.lastchar <= 3, "the line did not grow");

    let mut el = editor("abc", 3);
    c_delafter1(&mut el);
    assert!(el.el_line.lastchar <= 3);
}

/// vi's `w` and `b` over a line mixing words, punctuation and spaces.
/// The stops are what the three-valued predicate buys.
// [spec:libedit:sem:chared.cv-next-word-fn/test]
// [spec:libedit:sem:chared.cv-prev-word-fn/test]
#[test]
fn the_vi_word_walkers_stop_at_class_boundaries() {
    let line = "foo.bar baz";
    //          0123456789
    let mut el = editor(line, 0);
    let end = el.el_line.lastchar;

    // `w` from 0: over "foo", stopping at the punctuation.
    let a = cv_next_word(&mut el, 0, end, 1, cv__isword);
    assert_eq!(a, 3, "stops at the dot, not past it");
    // Again: over "." to "bar".
    let b = cv_next_word(&mut el, a, end, 1, cv__isword);
    assert_eq!(b, 4);
    // `W` from 0 treats it all as one word and lands on "baz".
    let big = cv_next_word(&mut el, 0, end, 1, cv__isWord);
    assert_eq!(big, 8, "big-word skips the punctuation entirely");

    // `b` back from the end.
    let p = cv_prev_word(&mut el, end, 0, 1, cv__isword);
    assert_eq!(p, 8, "back to the start of baz");
}

/// vi's `e` lands on the LAST character of the run, not one past it, and
/// leads with a `p++` so the character already under the cursor cannot end
/// the word. The same three-valued class equality applies, so `e` stops on
/// the last letter of `foo` where `E` runs on to the end of `foo.bar`.
// [spec:libedit:sem:chared.cv-endword-fn/test]
#[test]
fn the_end_of_word_walker_lands_on_the_last_character_of_the_run() {
    let line = "foo.bar baz";
    //          0123456789
    let mut el = editor(line, 0);
    let end = el.el_line.lastchar;

    assert_eq!(
        cv__endword(&mut el, 0, end, 1, cv__isword),
        2,
        "the last o of foo"
    );
    assert_eq!(
        cv__endword(&mut el, 2, end, 1, cv__isword),
        3,
        "the dot is a word of its own, so it both starts and ends one"
    );
    // Leading whitespace is skipped before the run is classified.
    assert_eq!(
        cv__endword(&mut el, 6, end, 1, cv__isword),
        10,
        "the z of baz"
    );
    assert_eq!(
        cv__endword(&mut el, 0, end, 1, cv__isWord),
        6,
        "big-word runs through the dot to the r of bar"
    );

    // The leading `p++` and the trailing `p--` cancel, so a count that
    // never enters the loop gives the caller's own position back.
    assert_eq!(cv__endword(&mut el, 5, end, 0, cv__isword), 5);
}

/// `ch_enlargebufs` doubles until the NEW space alone covers `addlen`, and
/// keeps all four line-sized buffers the same length — an invariant
/// `cv_undo`, `cv_yank` and `em_kill_region` index against. ERR-buffer-20:
/// `c_redo.lim` is deliberately left at its old offset, so the redo
/// buffer's usable limit does not grow with its allocation.
// [spec:libedit:sem:chared.ch-enlargebufs-fn/test]
#[test]
fn enlarging_the_buffers_doubles_them_and_leaves_the_redo_limit_behind() {
    let mut el = blank_editline();
    ch_init(&mut el);
    assert_eq!(el.el_line.limit, EL_BUFSIZ - 2);
    el.el_line.buffer[0] = u32::from(b'h');
    el.el_line.lastchar = 1;

    // `sz` is `limit + EL_LEAVE` == EL_BUFSIZ, and an `addlen` within that
    // needs a single doubling.
    assert_eq!(ch_enlargebufs(&mut el, 1), 1, "1 is success here, not 0");
    assert_eq!(el.el_line.limit, 2 * EL_BUFSIZ - 2);
    for buf in [
        &el.el_line.buffer,
        &el.el_chared.c_kill.buf,
        &el.el_chared.c_undo.buf,
        &el.el_chared.c_redo.buf,
    ] {
        assert_eq!(buf.len(), 2 * EL_BUFSIZ);
    }
    assert_eq!(el.el_history.sz, 2 * EL_BUFSIZ, "the stash grows in step");
    assert_eq!(el.el_chared.c_redo.lim, EL_BUFSIZ, "ERR-buffer-20");
    assert_eq!(el.el_line.buffer[0], u32::from(b'h'), "the line survives");

    // `addlen` above `sz` doubles until the added space alone covers it:
    // from sz == 2048, 4096 adds only 2048, so it goes round again.
    let mut el = blank_editline();
    ch_init(&mut el);
    assert_eq!(ch_enlargebufs(&mut el, 3 * EL_BUFSIZ), 1);
    assert_eq!(el.el_line.limit, 4 * EL_BUFSIZ - 2);
}

/// `ch_resizefun` and `ch_aliasfun` are unconditional stores that cannot
/// fail, and `None` — the C's NULL — switches the hook back off.
// [spec:libedit:sem:chared.ch-resizefun-fn/test]
// [spec:libedit:sem:chared.ch-aliasfun-fn/test]
#[test]
fn the_hook_setters_store_the_pair_and_none_clears_it() {
    unsafe extern "C" fn resize(_el: *mut EditLine, _a: *mut c_void) {}
    unsafe extern "C" fn alias(_a: *mut c_void, _s: *const c_char) -> *const c_char {
        ptr::null()
    }

    let mut el = blank_editline();
    let arg = ptr::without_provenance_mut::<c_void>(0x1234);

    assert_eq!(ch_resizefun(&mut el, Some(resize), arg), 0);
    assert!(el.el_chared.c_resizefun.is_some());
    assert_eq!(el.el_chared.c_resizearg, arg);
    assert_eq!(ch_resizefun(&mut el, None, ptr::null_mut()), 0);
    assert!(el.el_chared.c_resizefun.is_none());

    assert_eq!(ch_aliasfun(&mut el, Some(alias), arg), 0);
    assert!(el.el_chared.c_aliasfun.is_some());
    assert_eq!(el.el_chared.c_aliasarg, arg);
    assert_eq!(ch_aliasfun(&mut el, None, ptr::null_mut()), 0);
    assert!(el.el_chared.c_aliasfun.is_none());
}

/// `sem:chared.ch-resizefun-fn` makes the ordering part of the hook's
/// contract: it runs last, after `el_line.limit` has been published, so an
/// application re-deriving its saved positions sees the enlarged capacity
/// and not the pre-call one.
// [spec:libedit:sem:chared.ch-resizefun-fn/test]
#[test]
fn the_resize_hook_runs_with_the_new_limit_already_published() {
    unsafe extern "C" fn record(el: *mut EditLine, a: *mut c_void) {
        // SAFETY: the test installs this against the `EditLine` it then
        // enlarges, and passes `a` as a pointer to a live `usize` that
        // outlives the call.
        unsafe { *a.cast::<usize>() = (*el).el_line.limit };
    }

    let mut seen = 0usize;
    let mut el = blank_editline();
    ch_init(&mut el);
    ch_resizefun(&mut el, Some(record), ptr::from_mut(&mut seen).cast());

    assert_eq!(ch_enlargebufs(&mut el, 1), 1);
    assert_eq!(seen, 2 * EL_BUFSIZ - 2, "not the pre-call limit");

    // Cleared, so the next enlargement calls nothing.
    ch_resizefun(&mut el, None, ptr::null_mut());
    seen = 0;
    assert_eq!(ch_enlargebufs(&mut el, 1), 1);
    assert_eq!(seen, 0);
}

fn killed(el: &EditLine) -> String {
    el.el_chared.c_kill.buf[..el.el_chared.c_kill.last]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// Stage a pending vi operator the way `cv_action` leaves one: an anchor
/// position and the bitmask of what to do with the span it will cover.
fn pending(el: &mut EditLine, action: i32, anchor: usize) {
    el.el_chared.c_vcmd.action = action;
    el.el_chared.c_vcmd.pos = anchor;
}

/// A forward motion deletes `[anchor, cursor)` and leaves the cursor on the
/// anchor, and the removed text is yankable afterwards because `c_delafter`
/// fills the kill buffer whatever the editing mode (ERR-modes-19). This is the
/// only one of the three branches that refreshes the cursor, so it is also the
/// only one that moves `el_cursor`.
// [spec:libedit:sem:chared.cv-delfini-fn/test]
#[test]
fn a_forward_operator_deletes_up_to_the_cursor_and_redraws() {
    let mut el = editor("abcdef", 4);
    el.el_cursor.h = 42;
    pending(&mut el, DELETE, 1);

    cv_delfini(&mut el);

    assert_eq!(text(&el), "aef");
    assert_eq!(el.el_line.cursor, 1, "back on the anchor");
    assert_eq!(killed(&el), "bcd", "a delete leaves the text yankable");
    assert_eq!(el.el_chared.c_undo.len, 6, "the pre-delete line was saved");
    assert_eq!(el.el_chared.c_vcmd.action, NOP);
    assert_eq!(el.el_chared.c_vcmd.pos, 1, "the anchor is left as it was");
    assert_eq!(el.el_cursor.h, 1, "re_refresh_cursor ran");
}

/// A backward motion is not the mirror image. Step 6 puts the cursor on the
/// anchor, which is the HIGH end of the span here, and `c_delbefore`
/// deliberately does not move it — so `cv_delfini` has to add the negative
/// span itself to land on the low end. Neither this branch nor the yank branch
/// refreshes the cursor; they rely on the caller's return code.
// [spec:libedit:sem:chared.cv-delfini-fn/test]
#[test]
fn a_backward_operator_deletes_from_the_cursor_and_leaves_the_redraw_alone() {
    let mut el = editor("abcdef", 1);
    el.el_cursor.h = 42;
    pending(&mut el, DELETE, 4);

    cv_delfini(&mut el);

    assert_eq!(text(&el), "aef");
    assert_eq!(el.el_line.cursor, 1);
    assert_eq!(killed(&el), "bcd");
    assert_eq!(
        el.el_cursor.h, 42,
        "no re_refresh_cursor on this branch, so the screen model is stale"
    );
}

/// The zero-width case is not a no-op: `size == 0` is rewritten to 1 before
/// the branch is chosen, so an operator whose motion did not move still
/// removes the single character under the cursor.
// [spec:libedit:sem:chared.cv-delfini-fn/test]
#[test]
fn a_motion_that_did_not_move_still_deletes_one_character() {
    let mut el = editor("abc", 1);
    pending(&mut el, DELETE, 1);

    cv_delfini(&mut el);

    assert_eq!(text(&el), "ac");
    assert_eq!(killed(&el), "b");
    assert_eq!(el.el_line.cursor, 1);
}

/// A yank copies the span and leaves the line alone — and in both directions
/// it leaves the cursor on the anchor, because only the delete branch has the
/// compensating adjustment. So `yb` finishes to the RIGHT of the text it
/// copied, which is where `y` was pressed. No undo snapshot is taken either:
/// `cv_yank` alone does not call `cv_undo`, so `c_undo.len` keeps `ch_init`'s
/// "nothing saved" marker.
// [spec:libedit:sem:chared.cv-delfini-fn/test]
#[test]
fn a_yank_copies_the_span_and_finishes_on_the_anchor_either_way() {
    let mut el = editor("abcdef", 4);
    pending(&mut el, YANK, 1);
    cv_delfini(&mut el);
    assert_eq!(text(&el), "abcdef", "nothing is removed");
    assert_eq!(killed(&el), "bcd");
    assert_eq!(el.el_line.cursor, 1);
    assert_eq!(el.el_chared.c_undo.len, -1, "no snapshot on the yank path");

    // Backward: the same span, addressed from its lower end so the length
    // stays positive, and the cursor still lands on the anchor above it.
    let mut el = editor("abcdef", 1);
    pending(&mut el, YANK, 4);
    cv_delfini(&mut el);
    assert_eq!(text(&el), "abcdef");
    assert_eq!(killed(&el), "bcd");
    assert_eq!(el.el_line.cursor, 4);
}

/// The `INSERT` bit is what turns a delete into a change, and it is consumed
/// before anything is edited: the keymap switches to insert mode even on the
/// branches that touch nothing. `YANK` beats `DELETE` when both are set,
/// because the yank arm is tested first, and `INSERT` on its own still
/// deletes — the mask decides the mode, the absence of `YANK` decides the
/// edit.
// [spec:libedit:sem:chared.cv-delfini-fn/test]
#[test]
fn the_insert_bit_switches_the_keymap_whatever_the_operator_does() {
    let mut el = editor("abcdef", 4);
    el.el_map.current = ElMapCurrent::Alt;
    pending(&mut el, DELETE | INSERT, 1);
    cv_delfini(&mut el);
    assert_eq!(
        el.el_map.current,
        ElMapCurrent::Key,
        "a change opens insert"
    );
    assert_eq!(text(&el), "aef");

    // Yank wins the arm, and the mode switch still happens.
    let mut el = editor("abcdef", 4);
    el.el_map.current = ElMapCurrent::Alt;
    pending(&mut el, YANK | INSERT, 1);
    cv_delfini(&mut el);
    assert_eq!(el.el_map.current, ElMapCurrent::Key);
    assert_eq!(
        text(&el),
        "abcdef",
        "the yank arm is tested before the delete"
    );
    assert_eq!(killed(&el), "bcd");

    // A plain yank leaves the mode alone.
    let mut el = editor("abcdef", 4);
    el.el_map.current = ElMapCurrent::Alt;
    pending(&mut el, YANK, 1);
    cv_delfini(&mut el);
    assert_eq!(el.el_map.current, ElMapCurrent::Alt);

    // `INSERT` alone is not a mode switch with nothing attached: the delete
    // arm is the `else`, so the span goes whether `DELETE` was set or not.
    let mut el = editor("abcdef", 4);
    el.el_map.current = ElMapCurrent::Alt;
    pending(&mut el, INSERT, 1);
    cv_delfini(&mut el);
    assert_eq!(el.el_map.current, ElMapCurrent::Key);
    assert_eq!(text(&el), "aef");
}

/// A run of wide characters, for both the keystrokes fed in and the buffers
/// read back.
fn chars(s: &[u32]) -> String {
    s.iter().filter_map(|&c| char::from_u32(c)).collect()
}

/// Queue `keys` as the input `c_gets` will read. `el_wgetc` drains the macro
/// queue before it ever reaches the tty, so this is the whole of "the user
/// typed this" — and once the queue empties the read fails, which is the EOF
/// path.
fn feed(el: &mut EditLine, keys: &str) {
    let w: Vec<u32> = keys.chars().map(u32::from).collect();
    el_wpush(el, Some(&w));
}

/// Everything the editor wrote to its output descriptor while `f` ran.
///
/// The port writes straight to a descriptor rather than through a `FILE *`, so
/// reading the byte stream back needs a real file.
fn emitted(el: &mut EditLine, f: impl FnOnce(&mut EditLine)) -> Vec<u8> {
    use std::io::{Read, Seek};
    use std::os::fd::AsRawFd;
    use std::sync::atomic::{AtomicU32, Ordering};

    // Tests share a process and run in parallel, so the capture file cannot be
    // named after the process alone.
    static N: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "nshedit-chared-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    el.el_outfd = file.as_raw_fd();
    f(el);
    el.el_outfd = -1;
    let mut bytes = Vec::new();
    file.rewind().unwrap();
    file.read_to_end(&mut bytes).unwrap();
    drop(file);
    let _ = std::fs::remove_file(path);
    bytes
}

/// The prompt is copied over the START of the line buffer, so whatever the
/// user was editing is destroyed, and the typed characters land both in the
/// caller's buffer and in the line behind the prompt. On the way out only
/// `buffer[0]` is cleared — the prompt, the typed text and the tail of the
/// destroyed line are all still sitting above `lastchar`.
// [spec:libedit:sem:chared.c-gets-fn/test]
#[test]
fn a_short_read_draws_through_the_line_buffer_and_leaves_it_behind() {
    let mut el = editor("previous line", 3);
    let mut buf = [0u32; EL_BUFSIZ];
    feed(&mut el, "abc\r");

    let prompt: Vec<u32> = "? ".chars().map(u32::from).collect();
    assert_eq!(c_gets(&mut el, &mut buf, Some(&prompt)), 3);

    assert_eq!(chars(&buf[..3]), "abc");
    assert_eq!(
        buf[3],
        u32::from('\r'),
        "the terminator is stored at buf[len] and not counted"
    );

    assert_eq!(el.el_line.buffer[0], 0, "only the first cell is cleared");
    assert_eq!(el.el_line.lastchar, 0);
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(
        chars(&el.el_line.buffer[1..6]),
        " abc ",
        "the prompt's tail, the typed text, and the blank the cursor sat on"
    );
    assert_eq!(
        el.el_line.buffer[6],
        u32::from('u'),
        "and the part of the destroyed line the prompt did not reach"
    );
}

/// A NULL prompt starts the text at column 0, and all three terminators are
/// alike: ESC ends the read exactly as CR and LF do, which is what lets ESC
/// submit a vi search pattern (ERR-modes-67).
// [spec:libedit:sem:chared.c-gets-fn/test]
#[test]
fn escape_carriage_return_and_newline_all_end_the_read() {
    for term in ['\r', '\n', '\u{1b}'] {
        let mut el = editor("", 0);
        let mut buf = [0u32; EL_BUFSIZ];
        feed(&mut el, &format!("hi{term}"));
        assert_eq!(c_gets(&mut el, &mut buf, None), 2, "{term:?}");
        assert_eq!(chars(&buf[..2]), "hi");
        assert_eq!(buf[2], u32::from(term), "stored without being counted");
        assert_eq!(el.el_line.buffer[1], u32::from('i'), "no prompt offset");
    }
}

/// Backspace uncounts a character rather than erasing it: `len` and the draw
/// position both step back, the character stays in the caller's buffer above
/// `len`, and the next keystroke overwrites it. On empty input the same key
/// aborts the whole read with -1 — indistinguishable from EOF to the caller.
// [spec:libedit:sem:chared.c-gets-fn/test]
#[test]
fn backspace_uncounts_a_character_and_aborts_the_read_on_an_empty_one() {
    let mut el = editor("", 0);
    let mut buf = [0u32; EL_BUFSIZ];
    feed(&mut el, "ab\u{8}c\r");
    assert_eq!(c_gets(&mut el, &mut buf, None), 2);
    assert_eq!(chars(&buf[..2]), "ac");

    // DEL is the same key here as ^H.
    let mut el = editor("", 0);
    let mut buf = [0u32; EL_BUFSIZ];
    feed(&mut el, "ab\u{7f}\u{7f}\u{7f}");
    assert_eq!(c_gets(&mut el, &mut buf, None), -1);
    assert_eq!(
        chars(&buf[..2]),
        "ab",
        "the uncounted characters are still in the caller's buffer"
    );
}

/// Running out of input is a read error, and `c_gets` reports it the same -1
/// a backspace-on-empty gives. The line is still reset on the way out, so the
/// caller cannot tell from `el_line` either.
// [spec:libedit:sem:chared.c-gets-fn/test]
#[test]
fn exhausted_input_ends_the_read_the_same_way_a_cancel_does() {
    let mut el = editor("previous line", 3);
    let mut buf = [0u32; EL_BUFSIZ];
    feed(&mut el, "ab");

    assert_eq!(c_gets(&mut el, &mut buf, None), -1);
    assert_eq!(chars(&buf[..2]), "ab", "what was typed is still there");
    assert_eq!(el.el_line.buffer[0], 0);
    assert_eq!(el.el_line.lastchar, 0);
    assert_eq!(el.el_line.cursor, 0);
}

/// The cap is the one decision in the dispatch that turns on how much has
/// already been accepted rather than on the keystroke, and it is `>=` — so the
/// 1008th character is stored and the 1009th is not. Nothing but the character
/// and the count decides any of the five outcomes, which is why the boundary
/// is reachable without a thousand keystrokes.
// [spec:libedit:sem:chared.c-gets-fn/test]
#[test]
fn the_length_cap_is_tested_before_the_store_not_after() {
    let cap = EL_BUFSIZ - 16;
    assert_eq!(c_gets_classify(u32::from('x'), cap - 1), Keystroke::Store);
    assert_eq!(c_gets_classify(u32::from('x'), cap), Keystroke::TooLong);

    // The cap gates ordinary characters alone: a full read can still be
    // backspaced and still be submitted.
    assert_eq!(c_gets_classify(0o177, cap), Keystroke::Erase);
    assert_eq!(c_gets_classify(u32::from('\r'), cap), Keystroke::Terminate);
    // And an empty one aborts on the same key that erases.
    assert_eq!(c_gets_classify(0x08, 0), Keystroke::Abort);
    assert_eq!(c_gets_classify(0x08, 1), Keystroke::Erase);
}

/// The same cap driven end to end: 1008 characters fit, the 1009th and 1010th
/// are beeped away rather than truncating the read, and the terminator then
/// lands on `buf[1008]` — which is why the rule requires `EL_BUFSIZ - 16 + 1`
/// characters of caller storage.
///
/// ERR-buffer-10 is the other half of this bound and is not reached here: the
/// C checks `len` but never `el_line.limit`, so a prompt longer than 15
/// characters plus a maximal read walks off the initial line buffer. The two
/// in-tree callers use prompts of 2 and 3 characters.
// [spec:libedit:sem:chared.c-gets-fn/test]
#[test]
fn the_length_cap_beeps_the_overflow_away_instead_of_truncating() {
    let mut el = editor("", 0);
    let mut buf = vec![0u32; EL_BUFSIZ];
    let cap = EL_BUFSIZ - 16;
    feed(&mut el, &format!("{}\r", "x".repeat(cap + 2)));

    let prompt: Vec<u32> = "? ".chars().map(u32::from).collect();
    let mut n = 0;
    let out = emitted(&mut el, |el| n = c_gets(el, &mut buf, Some(&prompt)));

    assert_eq!(n, cap as i32);
    assert!(buf[..cap].iter().all(|&c| c == u32::from('x')));
    assert_eq!(buf[cap], u32::from('\r'), "the terminator, not a 1009th x");
    assert_eq!(
        out.iter().filter(|&&b| b == 0x07).count(),
        2,
        "one beep per discarded character"
    );
    // The prompt is two characters, so the last store lands well inside
    // `limit` and ERR-buffer-10's overrun is not reachable from here.
    assert!(2 + cap < el.el_line.limit);
}
