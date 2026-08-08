//! Tests for the ported `src/search.c`.
//!
//! Everything here runs headless. The commands that read a keystroke go
//! through `el_wgetc`, which drains the macro queue `el_wpush` fills before it
//! reaches the tty, so [`feed`] is the whole of "the user typed this".

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::chared::DELETE;
use crate::histedit::{CC_EOF, CC_NEWLINE};
use crate::history::OwnedHistoryW;
use crate::map::map_init_emacs;
use crate::testkit::headless_editor;

/// The shared editor in emacs mode, which is where `^R`, `^S` and the
/// incremental search are typed.
fn editor() -> EditLine {
    let mut el = headless_editor(80, 24);
    map_init_emacs(&mut el);
    el
}

/// The same editor in vi command mode, which is where `/`, `?`, `n`, `N` and
/// the character searches are typed. `headless_editor` already left the vi
/// tables installed — that is `map_init`'s own last step — so only the mode
/// moves.
fn vi_editor() -> EditLine {
    let mut el = headless_editor(80, 24);
    el.el_map.current = ElMapCurrent::Alt;
    el
}

fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

fn set_line(el: &mut EditLine, s: &str, at: usize) {
    let w = wide(s);
    el.el_line.buffer[..w.len()].copy_from_slice(&w);
    el.el_line.buffer[w.len()] = 0;
    el.el_line.lastchar = w.len();
    el.el_line.cursor = at;
}

fn text(el: &EditLine) -> String {
    el.el_line.buffer[..el.el_line.lastchar]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// The pattern as `patlen` describes it — `patbuf` carries storage past that
/// point, so the length field is the only thing that ends it.
fn pattern(el: &EditLine) -> String {
    el.el_search.patbuf[..el.el_search.patlen]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

fn with_history(el: &mut EditLine, entries: &[&str]) {
    // `with_size`, not `new`: a store with `max == 0` trims on every insert
    // and would leave the search nothing to find.
    let mut h = OwnedHistoryW::with_size(32);
    for s in entries {
        h.enter(&wide(s));
    }
    el.set_history(Rc::new(RefCell::new(h)));
}

/// Install a stored pattern without going through a search, the way an
/// earlier `c_setpat` or `cv_search` would have left one.
fn set_pattern(el: &mut EditLine, p: &str) {
    let w = wide(p);
    el.el_search.patbuf[..w.len()].copy_from_slice(&w);
    el.el_search.patbuf[w.len()] = 0;
    el.el_search.patlen = w.len();
}

// ---------------------------------------------------------------------------
// regerror
// ---------------------------------------------------------------------------

/// `regerror` is a stub with no caller: its definition sits inside `#ifdef
/// REGEXP`, `src/sys.h` picks `REGEX` instead, and the POSIX branch swallows a
/// bad pattern inside `el_match`. The whole of its contract is that the
/// message parameter is never read — the C body is empty and the parameter is
/// marked `/*ARGSUSED*/` — which is why a NULL, or any other address that
/// could not be dereferenced, is a legal argument.
// [spec:libedit:sem:search.regerror-fn/test]
#[test]
fn the_regexp_error_hook_never_reads_its_message() {
    regerror(core::ptr::null());
    regerror(core::ptr::without_provenance(1));
}

// ---------------------------------------------------------------------------
// c_hmatch and c_setpat
// ---------------------------------------------------------------------------

/// The argument order is subject-first, pattern-second, and getting it round
/// the other way inverts the entire history search while still producing
/// plausible-looking matches. The reversed case here is the discriminator: a
/// pattern longer than the candidate can only match if the two are swapped.
// [spec:libedit:sem:search.c-hmatch-fn/test]
#[test]
fn a_history_candidate_is_the_subject_and_the_pattern_buffer_is_the_pattern() {
    let mut el = editor();
    let candidate = wide("echo hello\0");
    let short = wide("echo\0");

    set_pattern(&mut el, "echo");
    assert_eq!(c_hmatch(&mut el, candidate.as_ptr()), 1);

    set_pattern(&mut el, "echo hello");
    assert_eq!(
        c_hmatch(&mut el, short.as_ptr()),
        0,
        "the candidate is not searched for inside the pattern"
    );

    // An empty pattern matches everything, which is not an edge case: it is
    // what `c_setpat` produces whenever the cursor sits at column zero, and
    // it is why `M-p` on an empty line walks the whole history.
    set_pattern(&mut el, "");
    assert_eq!(c_hmatch(&mut el, short.as_ptr()), 1);
}

/// The guard is the whole point of the function: a run of history searches
/// keeps the pattern the first one established, which is why
/// `ce_inc_search`, `cv_search` and `cv_repeat_srch` all fake `lastcmd`
/// before dispatching.
// [spec:libedit:sem:search.c-setpat-fn/test]
#[test]
fn a_second_history_search_keeps_the_first_ones_pattern() {
    let mut el = editor();
    set_line(&mut el, "echo hi", 4);
    set_pattern(&mut el, "kept");

    el.el_state.lastcmd = ED_SEARCH_PREV_HISTORY;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "kept");

    el.el_state.lastcmd = ED_SEARCH_NEXT_HISTORY;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "kept");

    // Any other previous command, and the line up to the cursor replaces it.
    el.el_state.lastcmd = ED_INSERT;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "echo");
}

/// The pattern is the line up to the cursor, and in vi command mode the
/// character *under* the cursor is included — so the same keystroke yields a
/// pattern one character longer in vi than in emacs. At end of line the C's
/// macro would yield `lastchar + 1`; the port clamps, which makes the
/// effective pattern the whole line either way.
// [spec:libedit:sem:search.c-setpat-fn/test]
#[test]
fn the_pattern_is_the_typed_prefix_and_vi_includes_the_cursor() {
    let mut el = editor();
    set_line(&mut el, "echo hi", 4);
    el.el_state.lastcmd = ED_INSERT;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "echo", "emacs stops short of the cursor");

    // Column zero gives the empty pattern that matches everything, which is
    // why `M-p` on an untouched line walks the whole history.
    el.el_line.cursor = 0;
    el.el_state.lastcmd = ED_INSERT;
    c_setpat(&mut el);
    assert_eq!(el.el_search.patlen, 0);

    let mut el = vi_editor();
    set_line(&mut el, "echo hi", 4);
    el.el_state.lastcmd = ED_INSERT;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "echo ", "vi includes the character under it");

    el.el_line.cursor = el.el_line.lastchar;
    el.el_state.lastcmd = ED_INSERT;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "echo hi", "and is clamped at end of line");

    // The adjustment applies at column zero too, so vi has no way to produce
    // the empty pattern from a non-empty line.
    el.el_line.cursor = 0;
    el.el_state.lastcmd = ED_INSERT;
    c_setpat(&mut el);
    assert_eq!(pattern(&el), "e");
}

// ---------------------------------------------------------------------------
// ce_search_line
// ---------------------------------------------------------------------------

/// The pattern is read from `patbuf[2]` onwards and anchored with `'^'`, so
/// the cursor lands on the *start* of the match rather than anywhere inside
/// it. Backward tries the cursor position first and walks down to zero;
/// forward walks up.
///
/// The last assertion is ERR-modes-35, fixed as the rule directs: the C
/// overwrites `patbuf[1]` with `'^'` for the duration of the call, leaving the
/// shared pattern buffer corrupt while it runs, and restores it on the way
/// out. Building the anchored pattern separately is not observable to a
/// correct caller — this pins that it stays that way.
// [spec:libedit:sem:search.ce-search-line-fn/test]
#[test]
fn a_line_search_lands_on_the_start_of_the_match() {
    let mut el = editor();
    set_line(&mut el, "hello hello", 11);
    set_pattern(&mut el, ".*ell");
    let before = el.el_search.patbuf.clone();

    assert_eq!(
        ce_search_line(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_NORM
    );
    assert_eq!(el.el_line.cursor, 7, "the nearest match at or below 11");

    el.el_line.cursor = 0;
    assert_eq!(
        ce_search_line(&mut el, i32::from(ED_SEARCH_NEXT_HISTORY)),
        CC_NORM
    );
    assert_eq!(el.el_line.cursor, 1, "the nearest match at or above 0");

    assert_eq!(
        before, el.el_search.patbuf,
        "the shared pattern buffer is never written through"
    );
}

/// A miss leaves the cursor exactly where it was, in both directions, and the
/// empty pattern — anchored to a bare `'^'` — matches at once wherever the
/// cursor happens to be, which is what makes `c_setpat`'s column-zero pattern
/// harmless here.
// [spec:libedit:sem:search.ce-search-line-fn/test]
#[test]
fn a_line_search_that_misses_moves_nothing() {
    let mut el = editor();
    set_line(&mut el, "hello", 3);
    set_pattern(&mut el, ".*zz");
    assert_eq!(
        ce_search_line(&mut el, i32::from(ED_SEARCH_NEXT_HISTORY)),
        CC_ERROR
    );
    assert_eq!(el.el_line.cursor, 3);
    assert_eq!(
        ce_search_line(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_ERROR
    );
    assert_eq!(el.el_line.cursor, 3);

    set_pattern(&mut el, ".*");
    assert_eq!(
        ce_search_line(&mut el, i32::from(ED_SEARCH_NEXT_HISTORY)),
        CC_NORM
    );
    assert_eq!(el.el_line.cursor, 3, "matched where it started");
}

// ---------------------------------------------------------------------------
// ce_inc_search
// ---------------------------------------------------------------------------

/// The emacs `^R` loop, one keystroke per recursion level. Typing four
/// characters and pressing ESC leaves the matched history entry in the line,
/// the cursor on the match inside it, and the pattern still loaded — ESC
/// terminates the search *keeping* what it found, because step 9 skips the
/// unwind restore for `CC_REFRESH`.
///
/// The cursor being 0 rather than at the end is the second `ce_search_line`
/// call inside step 8b(iii), which repositions onto the match within the
/// newly loaded entry.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn an_incremental_search_keeps_the_entry_it_landed_on() {
    let mut el = editor();
    with_history(&mut el, &["echo one", "ls -l", "echo two"]);
    el_wpush(&mut el, Some(&wide("echo\x1b")));

    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(text(&el), "echo two", "the newest matching entry");
    assert_eq!(el.el_history.eventno, 1);
    assert_eq!(el.el_line.cursor, 0, "positioned on the match");
    assert_eq!(
        pattern(&el),
        ".*echo",
        "the leading anchor stays, the trailing one is stripped"
    );
}

/// A second `^R` at the same direction is step 8b(i): the search advances past
/// the current match instead of finding it again, so it walks to the
/// next-older entry that matches. The two `echo` entries are separated by one
/// that does not match, which is what makes this a search rather than a step.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn repeating_the_direction_advances_past_the_current_match() {
    let mut el = editor();
    with_history(&mut el, &["echo one", "ls -l", "echo two"]);
    // "echo", then ^R, then ESC.
    el_wpush(&mut el, Some(&wide("echo\x12\x1b")));

    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(text(&el), "echo one", "skipped over the first match");
    assert_eq!(el.el_history.eventno, 3, "and over the entry between them");
}

/// ERR-modes-33, fixed as the rule directs. The search draws its prompt into
/// the live line buffer, and the C's end-of-file path returns straight out of
/// the loop leaving the user's text with `"\nbck:<pattern>"` stuck on the end
/// of it. The prompt is stripped first here, so the line the caller is
/// abandoning is the line the user typed.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn end_of_file_strips_the_search_prompt_off_the_line() {
    let mut el = editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "live", 4);
    // Nothing queued, and no readable descriptor, so the first `el_wgetc`
    // reports end of file.
    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_EOF
    );
    assert_eq!(text(&el), "live");
    assert_eq!(el.el_line.lastchar, 4);
}

/// The entry bound reserves room for the prompt — `"\nfwd:"` plus the pattern
/// — before anything is drawn, so a line with no space for it is refused
/// cleanly. The 4 is `sizeof(L"fwd") / sizeof(wchar_t)`.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn a_line_with_no_room_for_the_prompt_refuses_before_touching_anything() {
    let mut el = editor();
    with_history(&mut el, &["echo one"]);
    let limit = el.el_line.limit;
    el.el_line.lastchar = limit - 5;

    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_ERROR
    );
    assert_eq!(el.el_line.lastchar, limit - 5, "nothing was drawn");
    assert_eq!(el.el_search.patlen, 0, "and no pattern was started");
}

/// `^W` extends the pattern with the rest of the word under the cursor
/// instead of a single keystroke. The copy is bounded by the `'\n'` that
/// opens the prompt step 3 appended, which is what stops it swallowing the
/// prompt itself, and the cursor is put back where it started.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn control_w_extends_the_pattern_by_a_whole_word() {
    let mut el = editor();
    with_history(&mut el, &["hello world"]);
    set_line(&mut el, "hello", 1);
    el_wpush(&mut el, Some(&wide("\x17\x1b")));

    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(
        pattern(&el),
        ".*ello",
        "the tail of the word, not one character"
    );
    assert_eq!(el.el_line.cursor, 1);
    assert_eq!(text(&el), "hello", "which matched in the live line");
}

/// Step 8d and the `pchar` it keys off, which is ERR-modes-34's whole
/// substance. A `^G` pressed while the search is failing is absorbed by the
/// last recursion level that was still succeeding, which then resumes its
/// loop — so the failing character is dropped and the search carries on from
/// the last good match.
///
/// The absorption is conditional on `oldpchar == ':'`, and `pchar` is a
/// function-level static in the C that no return path resets on `CC_REFRESH`.
/// A search terminated by ESC while failing therefore leaves `'?'` behind and
/// changes what the *next* search does with its first `^G`. The port keeps
/// that in a thread-local rather than losing it to a fresh per-call value,
/// and this is the difference being pinned: identical input, opposite answers,
/// decided by what an earlier search left behind.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn a_failed_search_changes_what_the_next_ones_first_abort_does() {
    // A search that ends while failing leaves the failure marker set.
    let mut el = editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "live", 4);
    el_wpush(&mut el, Some(&wide("zq\x1b")));
    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(pchar_get(), u32::from(b'?'));

    // With it set, the outermost level no longer absorbs a `^G`, so the abort
    // propagates and the queued ESC is never read.
    let mut el = editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "live", 4);
    el.el_search.patlen = 0;
    el_wpush(&mut el, Some(&wide("z\x07\x1b")));
    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_ERROR
    );

    // Cleared, as a fresh process has it, the same keystrokes are answered the
    // other way: the `^G` is swallowed, the level resumes, and the ESC ends it.
    pchar_set(u32::from(b':'));
    let mut el = editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "live", 4);
    el_wpush(&mut el, Some(&wide("z\x07\x1b")));
    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(text(&el), "live", "the failing search was rolled back");
    assert_eq!(el.el_line.cursor, 4);
}

/// A `^G` that is not absorbed unwinds every level, and the outermost one —
/// the only one whose `oldpatlen` is 0 — restores the pattern, the history
/// position and the cursor. Two `^G`s in a row are what it takes when the
/// first one lands on a failing search.
// [spec:libedit:sem:search.ce-inc-search-fn/test]
#[test]
fn a_second_abort_rolls_the_whole_search_back() {
    let mut el = editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "live", 4);
    el_wpush(&mut el, Some(&wide("echoz\x07\x07")));

    assert_eq!(
        ce_inc_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_ERROR
    );
    assert_eq!(text(&el), "live", "the line the user was typing is back");
    assert_eq!(el.el_line.cursor, 4);
    assert_eq!(el.el_search.patlen, 0);
    assert_eq!(el.el_history.eventno, 0);
    assert_eq!(pchar_get(), u32::from(b':'), "and the marker with it");
}

// ---------------------------------------------------------------------------
// cv_search
// ---------------------------------------------------------------------------

/// vi's `/`: read a pattern, wrap it in `".*"` on both sides, and run one
/// non-incremental history search. `patdir` is recorded — this is the only
/// place outside `search_init` that ever writes it — and CR leaves the match
/// in the buffer for further editing.
// [spec:libedit:sem:search.cv-search-fn/test]
#[test]
fn a_vi_search_stores_the_wrapped_pattern_and_recalls_the_match() {
    let mut el = vi_editor();
    with_history(&mut el, &["echo one", "ls -l", "echo two"]);
    el_wpush(&mut el, Some(&wide("echo\r")));

    assert_eq!(
        cv_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(text(&el), "echo two");
    assert_eq!(pattern(&el), ".*echo.*");
    assert_eq!(el.el_search.patdir, i32::from(ED_SEARCH_PREV_HISTORY));
    assert_eq!(el.el_history.eventno, 1);
}

/// The inverse of the naive expectation, and the reason `c_gets` treats ESC,
/// CR and LF alike (ERR-modes-67): **ESC accepts and submits** the matched
/// entry through `ed_newline`, while CR leaves it in the buffer. The
/// terminating keystroke is read out of `tmpbuf[tmplen]`, one past the text
/// `c_gets` returned.
// [spec:libedit:sem:search.cv-search-fn/test]
#[test]
fn escape_submits_the_matched_entry_where_carriage_return_does_not() {
    let mut el = vi_editor();
    with_history(&mut el, &["echo one", "ls -l", "echo two"]);
    el_wpush(&mut el, Some(&wide("echo\x1b")));

    assert_eq!(
        cv_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_NEWLINE
    );
    assert_eq!(text(&el), "echo two\n", "submitted, terminator and all");
}

/// ERR-modes-38 and the `c_gets` prompt together: pressing `/` destroys the
/// line the user was editing before anything can be typed, and it stays
/// destroyed however the search ends. `patdir` is assigned *before* the read,
/// so even a cancelled search redirects the next `n`.
///
/// The cancel here is `c_gets` answering -1 for a backspace on empty input,
/// which ERR-modes-37 records as indistinguishable from end of file — the
/// `CC_EOF` is discarded and both come out as `CC_REFRESH`.
// [spec:libedit:sem:search.cv-search-fn/test]
#[test]
fn opening_a_vi_search_destroys_the_line_whatever_happens_next() {
    let mut el = vi_editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "the live line", 4);
    el_wpush(&mut el, Some(&wide("\x7f")));

    assert_eq!(
        cv_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(el.el_line.lastchar, 0, "the line is gone");
    assert_eq!(el.el_search.patlen, 0, "and no pattern was stored");
    assert_eq!(
        el.el_search.patdir,
        i32::from(ED_SEARCH_PREV_HISTORY),
        "but the direction was recorded before the read"
    );
}

/// Empty input reuses the stored pattern — and **ERR-modes-36, reproduced**:
/// shifting the old text right by two to make room for the `".*"` prefix
/// costs two positions but `patlen` gains only one, so the trailing `'.'`
/// overwrites the last character of the old pattern.
///
/// The history here is what makes that observable rather than cosmetic:
/// reusing `abc` produces `".*ab.*"`, which matches `abx`. A pattern built the
/// way the rule's prose says it should be — `".*abc.*"` — would not.
// [spec:libedit:sem:search.cv-search-fn/test]
#[test]
fn reusing_a_stored_pattern_loses_its_last_character() {
    let mut el = vi_editor();
    with_history(&mut el, &["abx"]);
    set_pattern(&mut el, "abc");
    el_wpush(&mut el, Some(&wide("\r")));

    assert_eq!(
        cv_search(&mut el, i32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(pattern(&el), ".*ab.*", "the 'c' was overwritten");
    assert_eq!(
        text(&el),
        "abx",
        "which is why an entry the real pattern excludes is matched"
    );
}

/// Empty input with nothing stored is the one path that reports an outright
/// error, and it reports it before the line-emptying step — but `c_gets` has
/// already cleared the line, so the user is left with neither their text nor a
/// search.
// [spec:libedit:sem:search.cv-search-fn/test]
#[test]
fn a_vi_search_with_no_pattern_at_all_fails() {
    let mut el = vi_editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "typing", 6);
    el_wpush(&mut el, Some(&wide("\r")));

    assert_eq!(
        cv_search(&mut el, i32::from(ED_SEARCH_NEXT_HISTORY)),
        CC_ERROR
    );
    assert_eq!(el.el_line.lastchar, 0);
}

// ---------------------------------------------------------------------------
// cv_repeat_srch
// ---------------------------------------------------------------------------

/// vi's `n` and `N`. The line is truncated to empty so the history search's
/// "this candidate is not the line already in the buffer" filter is vacuous
/// and the stored pattern alone decides; `lastcmd` is faked so the `c_setpat`
/// inside the search leaves that pattern alone. `patdir` is deliberately not
/// updated, so `N` searches the other way without becoming the new default.
// [spec:libedit:sem:search.cv-repeat-srch-fn/test]
#[test]
fn repeating_a_search_runs_it_off_the_stored_pattern_alone() {
    let mut el = vi_editor();
    with_history(&mut el, &["echo one", "ls -l", "echo two"]);
    set_line(&mut el, "whatever", 3);
    set_pattern(&mut el, ".*echo.*");
    el.el_search.patdir = i32::from(ED_SEARCH_PREV_HISTORY);

    assert_eq!(
        cv_repeat_srch(&mut el, u32::from(ED_SEARCH_PREV_HISTORY)),
        CC_REFRESH
    );
    assert_eq!(text(&el), "echo two");
    assert_eq!(el.el_history.eventno, 1);
    assert_eq!(
        el.el_state.lastcmd, ED_SEARCH_PREV_HISTORY,
        "the pattern is protected by faking the previous command"
    );
    assert_eq!(pattern(&el), ".*echo.*", "and so was not rebuilt");
}

/// ERR-modes-39, fixed as the rule directs: the C empties the line by moving
/// `lastchar` alone, so on the failure path the cursor is left pointing past
/// the end of a zero-length line. Only the success path hides it, because
/// `hist_get` reassigns both.
///
/// A command code that is neither of the two history searches is rejected —
/// after the line has already been emptied, which is the C's order.
// [spec:libedit:sem:search.cv-repeat-srch-fn/test]
#[test]
fn a_failed_repeat_leaves_the_cursor_inside_the_emptied_line() {
    let mut el = vi_editor();
    with_history(&mut el, &["echo one"]);
    set_line(&mut el, "whatever", 3);
    set_pattern(&mut el, ".*nomatch.*");

    assert_eq!(
        cv_repeat_srch(&mut el, u32::from(ED_SEARCH_PREV_HISTORY)),
        CC_ERROR
    );
    assert_eq!(el.el_line.lastchar, 0);
    assert_eq!(el.el_line.cursor, 0, "not left past `lastchar`");

    let mut el = vi_editor();
    set_line(&mut el, "whatever", 3);
    assert_eq!(cv_repeat_srch(&mut el, u32::from(ED_INSERT)), CC_ERROR);
    assert_eq!(
        el.el_line.lastchar, 0,
        "emptied before the code was checked"
    );
}

// ---------------------------------------------------------------------------
// cv_csearch
// ---------------------------------------------------------------------------

/// vi's `f` and `F`: move to the `count`-th occurrence of a character within
/// the current line. No regular expressions and no history. `t` and `T` set
/// `tflag` and stop one short of the target.
// [spec:libedit:sem:search.cv-csearch-fn/test]
#[test]
fn a_character_search_moves_within_the_line_only() {
    let mut el = editor();
    set_line(&mut el, "abcabc", 0);
    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 1, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 2);

    // `F` from the far end, backwards.
    set_line(&mut el, "abcabc", 5);
    assert_eq!(cv_csearch(&mut el, -1, u32::from(b'a'), 1, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 3);

    // A count of 0 runs the loop zero times and moves nothing; a count that
    // runs off the end fails and leaves the cursor alone.
    set_line(&mut el, "abcabc", 3);
    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 0, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 3);
    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 2, 0), CC_ERROR);
    assert_eq!(el.el_line.cursor, 3);

    // `chacha` starts as `L'\0'`, so `;` before any `f` fails here.
    assert_eq!(cv_csearch(&mut el, 1, 0, 1, 0), CC_ERROR);
}

/// ERR-modes-64, reproduced rather than fixed. The "never re-find the
/// character already under the cursor" step runs on every iteration and
/// applies to `t`/`T` as well as `f`/`F` — but `t` leaves the cursor one
/// *before* the target, so the skip never fires and the repeat re-finds the
/// same occurrence. vi's `t` followed by `;` therefore does not move.
// [spec:libedit:sem:search.cv-csearch-fn/test]
#[test]
fn a_till_search_repeated_by_semicolon_does_not_move() {
    let mut el = editor();
    set_line(&mut el, "abcabc", 0);
    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 1, 1), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1, "one short of the 'c' at 2");

    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 1, 1), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1, "and the repeat is stuck there");

    // `f` does move on, because the skip fires when the cursor is *on* the
    // target.
    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 1, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 2);
    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 1, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 5);
}

/// ERR-modes-40, reproduced: the search is recorded for `;` and `,` *before*
/// it runs, so a failed `f` still redefines what a following `;` looks for.
// [spec:libedit:sem:search.cv-csearch-fn/test]
#[test]
fn a_failed_character_search_still_becomes_the_one_semicolon_repeats() {
    let mut el = editor();
    set_line(&mut el, "abcabc", 0);
    el.el_search.chacha = u32::from(b'z');
    el.el_search.chadir = 1;
    el.el_search.chatflg = 0;

    assert_eq!(cv_csearch(&mut el, -1, u32::from(b'q'), 1, 1), CC_ERROR);
    assert_eq!(el.el_line.cursor, 0);
    assert_eq!(el.el_search.chacha, u32::from(b'q'));
    assert_eq!(el.el_search.chadir, -1);
    assert_eq!(el.el_search.chatflg, 1);
}

/// `(wint_t)-1` is the in-band sentinel `f`/`F`/`t`/`T` pass to mean "read the
/// target character from the terminal now"; `;` and `,` pass the remembered
/// one instead. End of file during that read is the C's `ed_end_of_file`.
// [spec:libedit:sem:search.cv-csearch-fn/test]
#[test]
fn the_minus_one_sentinel_reads_the_target_from_the_terminal() {
    let mut el = editor();
    set_line(&mut el, "abcabc", 0);
    el_wpush(&mut el, Some(&wide("b")));

    assert_eq!(cv_csearch(&mut el, 1, u32::MAX, 1, 0), CC_CURSOR);
    assert_eq!(el.el_line.cursor, 1);

    // Nothing left queued, and no readable descriptor.
    assert_eq!(cv_csearch(&mut el, 1, u32::MAX, 1, 0), CC_EOF);
}

/// With a vi operator pending the character search is that operator's motion,
/// and a forward one is made inclusive of the target before `cv_delfini` runs
/// — so `dfc` deletes the `c` as well.
// [spec:libedit:sem:search.cv-csearch-fn/test]
#[test]
fn a_pending_operator_makes_a_forward_search_its_inclusive_motion() {
    let mut el = vi_editor();
    set_line(&mut el, "abcabc", 0);
    el.el_chared.c_vcmd.action = DELETE;
    el.el_chared.c_vcmd.pos = 0;

    assert_eq!(cv_csearch(&mut el, 1, u32::from(b'c'), 1, 0), CC_REFRESH);
    assert_eq!(text(&el), "abc", "the target character went too");
    assert_eq!(el.el_chared.c_vcmd.action, NOP, "and the operator is spent");
}
