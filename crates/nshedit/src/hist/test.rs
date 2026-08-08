use std::cell::RefCell;
use std::rc::Rc;

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
            text: HistText::Wide(wide(s)),
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

/// Attaches a `Recorder` holding `entries` and keeps a handle on it, which is
/// what the editor borrowing rather than owning makes possible: the test can
/// still read what the editor asked for.
fn attach(el: &mut EditLine, entries: &[&'static str]) -> Rc<RefCell<Recorder>> {
    let h = Rc::new(RefCell::new(Recorder {
        entries: entries.to_vec(),
        ..Recorder::default()
    }));
    el.set_history(h.clone());
    h
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

    let h = attach(&mut el, &["one"]);
    assert!(el.el_history.src.is_attached());
    assert!(el.history().is_some(), "and the caller gets a handle back");

    // The lifetimes are independent, which is the requirement a shell has:
    // `set +o emacs` ends the editor and keeps the history.
    drop(el);
    assert_eq!(h.borrow().entries, vec!["one"], "the history outlived it");
}

/// `^P` is the whole reason this exists. It walked a history that no Rust
/// caller could install, so for one it always found an empty one and did
/// nothing.
#[test]
fn previous_history_recalls_the_newest_entry() {
    let mut el = editor();
    attach(&mut el, &["newest", "middle", "oldest"]);
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
    attach(&mut el, &["only"]);
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
    let recorder = attach(&mut el, &["a", "b", "c"]);
    el.el_state.argument = 1;
    ed_prev_history(&mut el, 0);
    ed_prev_history(&mut el, 0);
    ed_next_history(&mut el, 0);

    let asked = recorder.borrow().asked.clone();
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
    assert_eq!(newest.text, HistText::Wide(wide("second typed")));
    assert_eq!(
        newest.num, 2,
        "the store numbers entries as it created them"
    );

    let older = h.next().expect("and one behind it");
    assert_eq!(older.text, HistText::Wide(wide("first typed")));
    assert!(h.next().is_none(), "and nothing behind that");

    let mut el = editor();
    el.set_history(Rc::new(RefCell::new(h)));
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
    assert_eq!(h.first().unwrap().text, HistText::Wide(wide("same")));
    assert!(h.next().is_none(), "the duplicate was not stored");
}

/// A byte-oriented store, which is what a shell actually has: dash keeps a
/// `HistoryGen<c_char>` and the C sets `NARROW_HISTORY` for it. The seam takes
/// the bytes and decodes them here, through the same `ct_decode_string` the
/// C's narrow path uses, so neither side transcodes twice.
struct ByteStore(Vec<&'static [u8]>);

impl EditorHistory for ByteStore {
    fn first(&mut self) -> Option<HistLine> {
        self.0.first().map(|b| HistLine {
            num: 1,
            text: HistText::Narrow(b.to_vec()),
        })
    }
    fn last(&mut self) -> Option<HistLine> {
        self.first()
    }
    fn next(&mut self) -> Option<HistLine> {
        None
    }
    fn prev(&mut self) -> Option<HistLine> {
        None
    }
}

/// A narrow store reaches the editor without NARROW_HISTORY being set, because
/// each entry carries its own width. The flag exists to tell the C's two
/// instantiations apart and has nothing to say about a Rust store.
#[test]
fn a_byte_oriented_history_is_decoded_rather_than_refused() {
    let mut el = editor();
    el.set_history(Rc::new(RefCell::new(ByteStore(vec![b"echo hi"]))));
    el.el_state.argument = 1;

    assert_eq!(el.el_flags & crate::el::NARROW_HISTORY, 0);
    assert_eq!(ed_prev_history(&mut el, 0), CC_REFRESH);
    assert_eq!(text(&el), "echo hi");
}

/// Runs `hist_command` in its list form and gives back what it printed.
///
/// A pipe rather than the test runner's own stdout: `write_outfile` writes to
/// `el_outfd`, and the whole point of the assertions below is the bytes that
/// come out of it.
fn list(el: &mut EditLine) -> (i32, Vec<u8>) {
    use std::io::Read;
    use std::os::fd::AsRawFd;

    let (mut reader, writer) = std::io::pipe().expect("a pipe");
    el.el_outfd = writer.as_raw_fd();
    // `argc == 1`, which is the C's own `history` with no subcommand.
    let argv: [*const u32; 1] = [ptr::null()];
    let rc = hist_command(el, 1, argv.as_ptr());
    el.el_outfd = -1;
    drop(writer);

    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .expect("the pipe closed cleanly");
    (rc, out)
}

/// The `history` builtin prints the history, oldest first, numbered from 1.
///
/// It printed nothing at all before the listing had an escape of its own: the
/// escape came from the `bsd` feature, which is off by default, so the seam
/// answered `None` and `hist_command` returned the C's -1 without writing a
/// byte. This runs on whatever features the build has, which is the point.
#[test]
fn the_history_builtin_lists_every_entry() {
    let mut el = editor();
    // The store numbers from the newest, and the listing walks the other way.
    attach(&mut el, &["echo newest", "echo oldest"]);

    let (rc, out) = list(&mut el);
    assert_eq!(rc, 0);
    assert_eq!(out, b"1\techo oldest\n2\techo newest\n");
}

/// An entry containing a newline stays one printed line, which is the only
/// thing `VIS_NL` is there for. The escape is octal — `\012`, not `\n` — and a
/// backslash goes with it as `\134`, because both are in the extra list that
/// `VIS_NL` and an unset `VIS_NOSLASH` build.
///
/// Spaces and tabs are deliberately untouched: this is a listing for a person,
/// not `history.c`'s on-disk format.
#[test]
fn a_multi_line_entry_is_escaped_into_one_printed_line() {
    let mut el = editor();
    attach(&mut el, &["printf 'a\nb'\tx \\ y"]);

    let (rc, out) = list(&mut el);
    assert_eq!(rc, 0);
    assert_eq!(out, b"1\tprintf 'a\\012b'\tx \\134 y\n");
    assert_eq!(
        out.iter().filter(|&&b| b == b'\n').count(),
        1,
        "one entry, one line"
    );
}

/// No history attached is the C's -1, and that stays a -1 — the failure the
/// listing used to give for a reason that had nothing to do with the history.
#[test]
fn the_builtin_still_refuses_when_no_history_is_attached() {
    let mut el = editor();
    let argv: [*const u32; 1] = [ptr::null()];
    assert_eq!(hist_command(&mut el, 1, argv.as_ptr()), -1);
}
