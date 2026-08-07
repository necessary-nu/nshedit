use super::*;
use crate::chared::ch_init;
use crate::common::{ed_next_history, ed_prev_history};
use crate::el::blank_editline;
use crate::histedit::CC_REFRESH_BEEP;
use crate::history::OwnedHistoryW;
use crate::search::search_init;

/// An editor with a line buffer, a screen and no descriptors, in the state
/// `el_init` leaves behind. Same shape as the one in `vi/test.rs`, and for the
/// same reasons: `re_refresh` walks `el_display` under `t_size` and recurses to
/// a stack overflow on a zero-sized terminal, and descriptor 0 is the test
/// runner's own stdout.
fn editor() -> EditLine {
    let mut el = blank_editline();
    ch_init(&mut el);
    search_init(&mut el);
    el.el_infd = -1;
    el.el_outfd = -1;
    el.el_errfd = -1;
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

fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

/// A history with no store behind it, so the seam can be tested without also
/// testing `history.c`. It records what it was asked, because the editor
/// issuing the wrong opcode is a failure mode that still produces plausible
/// text.
#[derive(Default)]
struct Recorder {
    entries: Vec<&'static str>,
    at: usize,
    asked: Vec<&'static str>,
}

impl Recorder {
    /// `i` counts back from the newest, and the store numbers the newest 1 —
    /// which is what `vi_to_history_line` arithmetic depends on.
    fn line(&self, i: usize) -> Option<HistLine> {
        self.entries.get(i).map(|s| HistLine {
            num: i32::try_from(i).unwrap_or(i32::MAX) + 1,
            text: wide(s),
        })
    }
}

impl EditorHistory for Recorder {
    fn first(&mut self) -> Option<HistLine> {
        self.asked.push("first");
        self.at = 0;
        self.line(0)
    }
    fn last(&mut self) -> Option<HistLine> {
        self.asked.push("last");
        self.at = self.entries.len().saturating_sub(1);
        self.line(self.at)
    }
    fn next(&mut self) -> Option<HistLine> {
        self.asked.push("next");
        self.at += 1;
        self.line(self.at)
    }
    fn prev(&mut self) -> Option<HistLine> {
        self.asked.push("prev");
        self.at = self.at.checked_sub(1)?;
        self.line(self.at)
    }
}

/// Reads back what the editor asked a `Recorder` for. `history_mut` hands out
/// `&mut dyn`, which cannot be downcast without `Any`, and adding `Any` to the
/// public trait to satisfy a test would be the test dictating the API.
fn asked_of(el: &EditLine) -> Vec<&'static str> {
    match &el.el_history.src {
        HistSource::Rust(h) => {
            let ptr: *const dyn EditorHistory = &**h;
            // SAFETY: every caller installs a `Recorder` immediately above.
            unsafe { &*ptr.cast::<Recorder>() }.asked.clone()
        }
        _ => unreachable!("the test installed a Rust history"),
    }
}

/// The state that used to be unreachable: a history installed by a caller that
/// cannot name a variadic function. Nothing in `EditorHistory`'s signature is
/// `extern "C"`, which is the whole point of it.
#[test]
fn a_rust_history_attaches_where_only_the_c_abi_could() {
    let mut el = editor();
    assert!(
        !el.el_history.src.is_attached(),
        "a fresh editor has no history, as the C's NULL ref says"
    );

    el.set_history(Recorder {
        entries: vec!["one"],
        ..Recorder::default()
    });
    assert!(el.el_history.src.is_attached());
    assert!(el.history_mut().is_some(), "and the caller gets it back");
}

/// `^P` is the whole reason this exists. It walked a history that no Rust
/// caller could install, so for one it always found an empty one and did
/// nothing.
#[test]
fn previous_history_recalls_the_newest_entry() {
    let mut el = editor();
    el.set_history(Recorder {
        entries: vec!["newest", "middle", "oldest"],
        ..Recorder::default()
    });
    el.el_state.argument = 1;

    assert_eq!(ed_prev_history(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "newest");
    assert_eq!(el.el_history.eventno, 1);

    // Again, to the one behind it.
    assert_eq!(ed_prev_history(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "middle");
    assert_eq!(el.el_history.eventno, 2);

    // And forward again.
    assert_eq!(ed_next_history(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "newest");
    assert_eq!(el.el_history.eventno, 1);
}

/// Walking past the oldest entry beeps. `ed_prev_history` has no `CC_ERROR`
/// path at all — it sets a flag, retries `hist_get` and discards the second
/// answer (ERR-history-31), then reports success-with-beep either way. The
/// recalled line survives the refusal rather than being cleared.
#[test]
fn walking_past_the_oldest_entry_beeps_rather_than_failing() {
    let mut el = editor();
    el.set_history(Recorder {
        entries: vec!["only"],
        ..Recorder::default()
    });
    el.el_state.argument = 1;

    assert_eq!(ed_prev_history(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "only");
    assert_eq!(ed_prev_history(&mut el, 0), CC_REFRESH_BEEP);
    assert_eq!(text(&el), "only", "the recalled line survives the refusal");
}

/// The editor issues exactly the four opcodes the trait has methods for. A
/// fifth would fall to the catch-all in `hist_fun` and read as an empty
/// history, which is silent, so this pins the set rather than the behaviour.
#[test]
fn the_editor_asks_only_for_the_four_walks() {
    let mut el = editor();
    el.set_history(Recorder {
        entries: vec!["a", "b", "c"],
        ..Recorder::default()
    });
    el.el_state.argument = 1;
    ed_prev_history(&mut el, 0);
    ed_prev_history(&mut el, 0);
    ed_next_history(&mut el, 0);

    let asked = asked_of(&el);
    assert!(
        asked
            .iter()
            .all(|a| ["first", "last", "next", "prev"].contains(a)),
        "unexpected opcode: {asked:?}"
    );
    assert!(
        asked.contains(&"first"),
        "every walk restarts from the newest and counts forward"
    );
}

/// `history size` and `history unique` in an `.editrc` reach the trait's two
/// settings, which default to -1 — the same answer the C gives for a narrow
/// store, so an implementation that ignores `.editrc` is not punished for it.
#[test]
fn the_editrc_settings_default_to_the_answer_the_c_gives_a_narrow_store() {
    let mut h = Recorder::default();
    assert_eq!(EditorHistory::set_size(&mut h, 100), -1);
    assert_eq!(EditorHistory::set_unique(&mut h, true), -1);
}

/// The built-in store needs no adapter written by the caller: it implements
/// the trait itself, so attaching one is a single call.
#[test]
fn the_builtin_store_walks_through_the_seam() {
    let mut h = OwnedHistoryW::new();
    assert_eq!(h.set_size(16), 0);
    assert!(h.enter(&wide("first typed")));
    assert!(h.enter(&wide("second typed")));

    let newest = h.first().expect("a store with two entries has a first");
    assert_eq!(newest.text, wide("second typed"));
    assert_eq!(
        newest.num, 2,
        "the store numbers entries as it created them"
    );

    let older = h.next().expect("and one behind it");
    assert_eq!(older.text, wide("first typed"));
    assert!(h.next().is_none(), "and nothing behind that");

    let mut el = editor();
    el.set_history(h);
    el.el_state.argument = 1;
    assert_eq!(ed_prev_history(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "second typed");
}

/// `H_SETUNIQUE` reaches the store through the same wrapper, so a repeated
/// line is dropped rather than stored twice.
#[test]
fn the_owned_store_forwards_the_settings_it_advertises() {
    // Without a size the store keeps nothing, so this would pass for the
    // wrong reason: `first` would be `None` because everything was trimmed.
    let mut h = OwnedHistoryW::with_size(16);
    assert_eq!(h.set_unique(true), 0);
    assert!(h.enter(&wide("same")), "the first one is stored");
    assert!(
        !h.enter(&wide("same")),
        "and the second is suppressed rather than failing"
    );
    assert_eq!(h.first().unwrap().text, wide("same"));
    assert!(h.next().is_none(), "the duplicate was not stored");
}
