//! [`NativeHistory`], a history whose entries carry application data.
//!
//! Not a port of anything. `history.c`'s entry has a `void *data` the caller
//! owns and libedit hands back untouched — no length, so nothing that could be
//! persisted even in principle, which is why the C ABI path has no blob and
//! never will. This is the Rust-native store for callers that want one: entries
//! are [`histfile::Record`]s, so what is held in memory is exactly what is
//! written to disk, and a shell can hang its own per-command state off a line
//! and get it back after a restart.
//!
//! It implements [`EditorHistory`], so attaching one is
//! [`crate::el::EditLine::set_history`] and nothing else. The text is bytes,
//! which is what a shell has and what the file format stores; the editor
//! decodes at its own edge.

use std::collections::VecDeque;
use std::io::{self, Write};

use crate::hist::{EditorHistory, HistLine, HistText};
use crate::histfile::{self, Record};

/// A history of [`Record`]s, newest first.
///
/// # Event numbers
///
/// Ids are allocated in entry order and never reused, so the newest entry has
/// the highest — the same arrangement `history.c` uses, and what makes 0 a
/// usable "no such event" sentinel. `vi_to_history_line` does arithmetic on
/// the number this hands back, so it is the id and not the position.
pub struct NativeHistory {
    /// Newest at the front, which is the order every walk starts from.
    entries: VecDeque<(i32, Record)>,
    /// Where the last walk left off. `None` before the first one.
    cursor: Option<usize>,
    /// How many entries to keep. 0 keeps everything, which is the opposite of
    /// the C store's 0 and is deliberate — see [`NativeHistory::new`].
    max: usize,
    /// Whether a line equal to the newest entry is dropped rather than added.
    unique: bool,
    /// Next id to hand out.
    next_id: i32,
}

impl NativeHistory {
    /// An unbounded history.
    ///
    /// The C store's size starts at 0 meaning "retain nothing", so a fresh one
    /// silently discards everything until `H_SETSIZE`. That is a trap rather
    /// than a default, and this is not the C, so 0 means unbounded here and a
    /// bound is something a caller asks for.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: None,
            max: 0,
            unique: false,
            next_id: 1,
        }
    }

    /// A history that keeps at most `max` entries, evicting the oldest.
    #[must_use]
    pub fn with_size(max: usize) -> Self {
        let mut h = Self::new();
        h.max = max;
        h
    }

    /// Keep at most `max` entries; 0 is unbounded. Takes effect on the next
    /// [`NativeHistory::enter`] rather than evicting now, as the C's does.
    pub fn set_size(&mut self, max: usize) {
        self.max = max;
    }

    /// Whether a line equal to the newest entry is dropped rather than stored.
    ///
    /// Compares against the newest entry only, so `a b a` keeps all three —
    /// the same rule the C store applies, and the one a shell user expects.
    pub fn set_unique(&mut self, on: bool) {
        self.unique = on;
    }

    /// How many entries are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the history holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop every entry. Ids are not reused, so the next one continues where
    /// the last left off.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cursor = None;
    }

    /// Add `rec` as the newest entry. Answers whether it was stored — `false`
    /// means [`NativeHistory::set_unique`] is on and `rec.text` equals the
    /// entry already at the front.
    ///
    /// Only the text takes part in that comparison. Two entries with the same
    /// text and different blobs are the same line typed twice, and suppressing
    /// the second would silently discard the newer blob.
    pub fn enter(&mut self, rec: Record) -> bool {
        if self.unique
            && self
                .entries
                .front()
                .is_some_and(|(_, r)| r.text == rec.text)
        {
            return false;
        }
        self.entries.push_front((self.next_id, rec));
        self.next_id = self.next_id.wrapping_add(1);
        if self.max != 0 {
            self.entries.truncate(self.max);
        }
        self.cursor = None;
        true
    }

    /// Every entry, newest first.
    pub fn iter(&self) -> impl Iterator<Item = &Record> {
        self.entries.iter().map(|(_, r)| r)
    }

    /// Replace the contents from a history file, newest last in the file.
    ///
    /// Reads either format: [`histfile::read_any`] sniffs the header, so a
    /// caller never migrates a file by hand. Returns whatever the reader could
    /// not make sense of, having kept everything it could — a truncated file
    /// costs the last entry, not the history.
    pub fn load(&mut self, bytes: &[u8]) -> Option<histfile::Error> {
        let (records, err) = histfile::read_any(bytes);
        self.clear();
        // The file is oldest-first and this store is newest-first.
        for rec in records {
            self.entries.push_front((self.next_id, rec));
            self.next_id = self.next_id.wrapping_add(1);
        }
        if self.max != 0 {
            self.entries.truncate(self.max);
        }
        err
    }

    /// Write every entry to `w` in the native format, oldest first.
    ///
    /// Whole-file, header included, so `w` must be empty or about to be
    /// truncated. Appending one entry to a file that already has a header is
    /// [`histfile::append`] and does not go through here.
    pub fn save<W: Write>(&self, w: &mut W) -> io::Result<()> {
        histfile::write_header(w)?;
        for (_, rec) in self.entries.iter().rev() {
            histfile::append(w, rec)?;
        }
        Ok(())
    }

    /// The entry at `at`, as the editor wants it.
    fn line(&self, at: usize) -> Option<HistLine> {
        self.entries.get(at).map(|(num, rec)| HistLine {
            num: *num,
            // The store is bytes and so is the file format. The editor decodes
            // once at its own edge rather than this transcoding on the way out
            // and the editor transcoding back.
            text: HistText::Narrow(rec.text.to_vec()),
        })
    }

    /// Move to `at` and answer what is there, leaving the cursor alone if
    /// there is nothing.
    fn walk_to(&mut self, at: usize) -> Option<HistLine> {
        let line = self.line(at)?;
        self.cursor = Some(at);
        Some(line)
    }
}

impl Default for NativeHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorHistory for NativeHistory {
    fn first(&mut self) -> Option<HistLine> {
        self.walk_to(0)
    }

    fn last(&mut self) -> Option<HistLine> {
        self.walk_to(self.entries.len().checked_sub(1)?)
    }

    fn next(&mut self) -> Option<HistLine> {
        // "Next" is towards the oldest, matching `H_NEXT`. Without a previous
        // walk there is nowhere to step from, and the C's cursor sits on its
        // list header in that state, so this answers nothing rather than
        // guessing at the front.
        let at = self.cursor?.checked_add(1)?;
        self.walk_to(at)
    }

    fn prev(&mut self) -> Option<HistLine> {
        let at = self.cursor?.checked_sub(1)?;
        self.walk_to(at)
    }

    fn set_size(&mut self, entries: i32) -> i32 {
        let Ok(n) = usize::try_from(entries) else {
            return -1;
        };
        NativeHistory::set_size(self, n);
        0
    }

    fn set_unique(&mut self, on: bool) -> i32 {
        NativeHistory::set_unique(self, on);
        0
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn rec(text: &str, blob: &[u8]) -> Record {
        Record {
            text: text.into(),
            blob: blob.to_vec(),
        }
    }

    fn text_of(line: &HistLine) -> Vec<u8> {
        match &line.text {
            HistText::Narrow(b) => b.clone(),
            HistText::Wide(_) => panic!("this store is byte-oriented"),
        }
    }

    /// The point of the whole node: an entry holds application data, and it
    /// survives the round trip to a file and back. The C ABI's `void *data`
    /// has no length, so nothing on that path could do this even in principle.
    #[test]
    fn a_blob_survives_a_save_and_a_load() {
        let mut h = NativeHistory::new();
        h.enter(rec("git commit", b"\x00\x01exit=0"));
        h.enter(rec("ls -la", b"exit=2"));

        let mut file = Vec::new();
        h.save(&mut file).expect("a Vec never fails to write");

        let mut back = NativeHistory::new();
        assert_eq!(back.load(&file), None);
        let entries: Vec<_> = back.iter().cloned().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], rec("ls -la", b"exit=2"), "newest first");
        assert_eq!(entries[1], rec("git commit", b"\x00\x01exit=0"));
    }

    /// A blob holding a NUL and a frame-delimiter byte is why the file is
    /// COBS-framed rather than line-oriented. Nothing here may treat the blob
    /// as text.
    #[test]
    fn a_blob_may_hold_any_bytes_at_all() {
        let mut h = NativeHistory::new();
        let awkward: Vec<u8> = (0u8..=255).collect();
        h.enter(rec("x", &awkward));

        let mut file = Vec::new();
        h.save(&mut file).unwrap();
        let mut back = NativeHistory::new();
        assert_eq!(back.load(&file), None);
        assert_eq!(back.iter().next().unwrap().blob, awkward);
    }

    /// Entry text is bytes too, so a sequence that is not valid in the current
    /// locale round-trips unchanged — the reason the format is locale-free.
    #[test]
    fn undecodable_text_round_trips_rather_than_being_repaired() {
        let mut h = NativeHistory::new();
        h.enter(rec("caf\u{e9}", b""));
        let raw = vec![0x63, 0xff, 0xfe];
        h.enter(Record {
            text: raw.clone().into(),
            blob: Vec::new(),
        });

        let mut file = Vec::new();
        h.save(&mut file).unwrap();
        let mut back = NativeHistory::new();
        back.load(&file);
        assert_eq!(back.iter().next().unwrap().text.as_slice(), &raw[..]);
    }

    /// The four walks the editor issues, and the directions their names claim.
    /// `next` is towards the oldest, as `H_NEXT` is.
    #[test]
    fn the_walks_move_in_the_directions_their_names_claim() {
        let mut h = NativeHistory::new();
        for s in ["oldest", "middle", "newest"] {
            h.enter(rec(s, b""));
        }
        assert_eq!(text_of(&h.first().unwrap()), b"newest");
        assert_eq!(text_of(&h.next().unwrap()), b"middle");
        assert_eq!(text_of(&h.next().unwrap()), b"oldest");
        assert!(h.next().is_none(), "and nothing past the oldest");
        assert_eq!(text_of(&h.prev().unwrap()), b"middle");
        assert_eq!(text_of(&h.last().unwrap()), b"oldest");
    }

    /// A walk with no previous position answers nothing rather than guessing
    /// at the front — the C's cursor sits on its list header in that state.
    #[test]
    fn a_step_with_no_previous_walk_has_nowhere_to_step_from() {
        let mut h = NativeHistory::new();
        h.enter(rec("only", b""));
        assert!(h.next().is_none());
        assert!(h.prev().is_none());
        assert!(
            h.first().is_some(),
            "but a walk that names its own start does"
        );
    }

    /// Ids are allocated in entry order and never reused, so the newest has
    /// the highest. `vi_to_history_line` does arithmetic on this, so it is the
    /// id rather than the position.
    #[test]
    fn event_ids_are_never_reused() {
        let mut h = NativeHistory::with_size(2);
        for s in ["a", "b", "c"] {
            h.enter(rec(s, b""));
        }
        assert_eq!(h.first().unwrap().num, 3);
        assert_eq!(h.next().unwrap().num, 2);
        h.clear();
        h.enter(rec("d", b""));
        assert_eq!(h.first().unwrap().num, 4, "clearing does not reset them");
    }

    /// Unbounded by default, which is the opposite of the C store's 0-means-
    /// retain-nothing and is the reason this type exists rather than wrapping
    /// that one.
    #[test]
    fn a_fresh_store_keeps_what_it_is_given() {
        let mut h = NativeHistory::new();
        for i in 0..1000 {
            h.enter(rec(&format!("line {i}"), b""));
        }
        assert_eq!(h.len(), 1000);
    }

    /// A bound evicts the oldest rather than refusing the newest.
    #[test]
    fn a_bound_evicts_the_oldest() {
        let mut h = NativeHistory::with_size(2);
        for s in ["one", "two", "three"] {
            assert!(h.enter(rec(s, b"")));
        }
        assert_eq!(h.len(), 2);
        assert_eq!(text_of(&h.first().unwrap()), b"three");
        assert_eq!(text_of(&h.next().unwrap()), b"two");
        assert!(h.next().is_none(), "\"one\" was evicted");
    }

    /// Uniqueness compares text and ignores the blob: the same line typed
    /// twice is one line, and suppressing it because the blobs differ would
    /// keep a duplicate the user did not ask for. The reverse — suppressing a
    /// repeat and silently dropping its newer blob — is what the comparison
    /// being text-only makes explicit rather than accidental.
    #[test]
    fn uniqueness_compares_the_text_and_not_the_blob() {
        let mut h = NativeHistory::new();
        h.set_unique(true);
        assert!(h.enter(rec("same", b"first")));
        assert!(!h.enter(rec("same", b"second")), "still a repeat");
        assert!(h.enter(rec("other", b"")));
        assert!(h.enter(rec("same", b"third")), "not an immediate repeat");

        assert_eq!(h.len(), 3);
        assert_eq!(h.iter().next().unwrap().blob, b"third");
    }

    /// Loading replaces rather than merges, and honours a bound set first by
    /// keeping the newest.
    #[test]
    fn loading_replaces_the_contents_and_honours_the_bound() {
        let mut src = NativeHistory::new();
        for s in ["a", "b", "c"] {
            src.enter(rec(s, b""));
        }
        let mut file = Vec::new();
        src.save(&mut file).unwrap();

        let mut h = NativeHistory::with_size(2);
        h.enter(rec("discarded", b""));
        assert_eq!(h.load(&file), None);
        assert_eq!(h.len(), 2);
        assert_eq!(text_of(&h.first().unwrap()), b"c", "the newest of the file");
    }

    /// An empty history writes a header and nothing else, and reads back as
    /// empty rather than as an error.
    #[test]
    fn an_empty_history_round_trips() {
        let h = NativeHistory::new();
        let mut file = Vec::new();
        h.save(&mut file).unwrap();
        assert!(!file.is_empty(), "the header is still written");

        let mut back = NativeHistory::new();
        assert_eq!(back.load(&file), None);
        assert!(back.is_empty());
    }
}
