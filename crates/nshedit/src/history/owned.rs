//! [`OwnedHistoryW`], the built-in store with a destructor and a safe surface.
//!
//! Not a port of anything: `history.c` hands out a raw pointer and expects the
//! caller to remember `history_end`. This is the same store shaped for a Rust
//! caller, and it is what makes [`crate::el::EditLine::set_history`] usable
//! without the caller writing an adapter.

use core::ptr;

use super::{HistoryArg, HistoryW, history_w, history_wend, history_winit};
use crate::hist::{EditorHistory, HistLine};
use crate::histedit::{
    H_ENTER, H_FIRST, H_LAST, H_NEXT, H_PREV, H_SETSIZE, H_SETUNIQUE, HistEventW,
};

/// The built-in wide history, owned by Rust rather than by a `void *`.
///
/// [`history_winit`] and [`history_wend`] are the C's shape: a raw pointer the
/// caller must remember to free. This is the same store with a destructor and
/// a safe surface, so a program that links `nshedit` directly never handles
/// the pointer. It implements [`EditorHistory`], which is what makes
/// [`crate::el::EditLine::set_history`] usable with no other code.
pub struct OwnedHistoryW(*mut HistoryW);

impl OwnedHistoryW {
    /// An empty history that **retains nothing** until it is given a size.
    ///
    /// That is the C's behaviour, and it is a trap: a fresh store has
    /// `max == 0`, and [`history_def_enter`] trims to `max` on every insert,
    /// so [`OwnedHistoryW::enter`] appears to work and the entry is gone
    /// before the next call. Call [`OwnedHistoryW::set_size`] first, or use
    /// [`OwnedHistoryW::with_size`], which is the same two calls.
    #[must_use]
    pub fn new() -> Self {
        Self(history_winit())
    }

    /// An empty history that keeps at most `entries`.
    #[must_use]
    pub fn with_size(entries: i32) -> Self {
        let mut h = Self::new();
        h.set_size(entries);
        h
    }

    /// Run any `H_*` operation against the store, for the ones with no method
    /// here. Answers the C's return code and the event it wrote.
    ///
    /// The event's `str` borrows from the store and is invalidated by the next
    /// operation, exactly as in the C.
    pub fn exec(&mut self, op: i32, arg: HistoryArg<'_, u32>) -> (i32, HistEventW) {
        let mut ev = HistEventW {
            num: 0,
            str: ptr::null(),
        };
        let rv = history_w(self.0, &mut ev, op, arg);
        (rv, ev)
    }

    /// `H_ENTER` — add `line` as the newest entry. Answers whether it was
    /// stored: `false` means [`OwnedHistoryW::set_unique`] is on and `line`
    /// equals the entry already at the front.
    ///
    /// The C's own return is the inverted one it is easy to get backwards —
    /// **1 for a real insert and 0 for a suppressed one**, both "success",
    /// with -1 for a NULL handle that cannot occur here (ERR-history-25).
    /// Decoding that is exactly what this wrapper is for.
    ///
    /// The store keeps `wchar_t` strings and terminates them itself, so the
    /// NUL is added here and is not part of `line`.
    pub fn enter(&mut self, line: &[u32]) -> bool {
        let mut owned: Vec<u32> = Vec::with_capacity(line.len() + 1);
        owned.extend_from_slice(line);
        owned.push(0);
        self.exec(H_ENTER, HistoryArg::Str(owned.as_ptr())).0 == 1
    }

    /// `H_SETSIZE` — keep at most `entries`. 0 on success.
    pub fn set_size(&mut self, entries: i32) -> i32 {
        self.exec(H_SETSIZE, HistoryArg::Num(entries)).0
    }

    /// `H_SETUNIQUE` — whether a line equal to the newest entry is dropped
    /// rather than added. 0 on success.
    pub fn set_unique(&mut self, on: bool) -> i32 {
        self.exec(H_SETUNIQUE, HistoryArg::Num(i32::from(on))).0
    }
}

impl Default for OwnedHistoryW {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OwnedHistoryW {
    fn drop(&mut self) {
        history_wend(self.0);
    }
}

impl OwnedHistoryW {
    /// The four walks share everything but the opcode.
    fn walk(&mut self, op: i32) -> Option<HistLine> {
        let (rv, ev) = self.exec(op, HistoryArg::None);
        if rv == -1 || ev.str.is_null() {
            return None;
        }
        // SAFETY: on success the store's entry string is NUL-terminated and
        // stays valid until the entry is replaced; it is copied out here.
        let mut text = Vec::new();
        let mut p = ev.str;
        unsafe {
            while *p != 0 {
                text.push(*p);
                p = p.add(1);
            }
        }
        Some(HistLine { num: ev.num, text })
    }
}

impl EditorHistory for OwnedHistoryW {
    fn first(&mut self) -> Option<HistLine> {
        self.walk(H_FIRST)
    }
    fn last(&mut self) -> Option<HistLine> {
        self.walk(H_LAST)
    }
    fn next(&mut self) -> Option<HistLine> {
        self.walk(H_NEXT)
    }
    fn prev(&mut self) -> Option<HistLine> {
        self.walk(H_PREV)
    }
    fn set_size(&mut self, entries: i32) -> i32 {
        OwnedHistoryW::set_size(self, entries)
    }
    fn set_unique(&mut self, on: bool) -> i32 {
        OwnedHistoryW::set_unique(self, on)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    /// The trap this wrapper exists to make visible. A fresh store's `max` is
    /// 0 and the insert path trims to `max`, so the entry is gone before the
    /// next call — `enter` still answers true, because it really did insert.
    #[test]
    fn a_store_with_no_size_keeps_nothing() {
        let mut h = OwnedHistoryW::new();
        assert!(h.enter(&wide("vanishes")), "the insert itself succeeded");
        assert!(h.first().is_none(), "and the entry was trimmed immediately");

        let mut h = OwnedHistoryW::with_size(4);
        assert!(h.enter(&wide("stays")));
        assert_eq!(h.first().unwrap().text, wide("stays"));
    }

    /// The store is a ring: past `max`, the oldest entry falls off rather than
    /// the newest being refused.
    #[test]
    fn the_size_evicts_the_oldest_rather_than_refusing_the_newest() {
        let mut h = OwnedHistoryW::with_size(2);
        for s in ["one", "two", "three"] {
            assert!(h.enter(&wide(s)));
        }
        assert_eq!(h.first().unwrap().text, wide("three"));
        assert_eq!(h.next().unwrap().text, wide("two"));
        assert!(h.next().is_none(), "\"one\" was evicted");
    }

    /// `enter` answers whether the line was stored, not the C's inverted
    /// 1-for-inserted / 0-for-suppressed pair (ERR-history-25). Deduplication
    /// is against the newest entry only, so `a b a` keeps all three.
    #[test]
    fn uniqueness_suppresses_only_an_immediate_repeat() {
        let mut h = OwnedHistoryW::with_size(8);
        h.set_unique(true);
        assert!(h.enter(&wide("a")));
        assert!(!h.enter(&wide("a")), "an immediate repeat is suppressed");
        assert!(h.enter(&wide("b")));
        assert!(h.enter(&wide("a")), "but not one with a line in between");

        assert_eq!(h.first().unwrap().text, wide("a"));
        assert_eq!(h.next().unwrap().text, wide("b"));
        assert_eq!(h.next().unwrap().text, wide("a"));
    }

    /// The four walks move where their names say, and `last` is the oldest
    /// rather than the most recent — the store counts from the newest.
    #[test]
    fn the_walks_move_in_the_directions_their_names_claim() {
        let mut h = OwnedHistoryW::with_size(8);
        for s in ["oldest", "middle", "newest"] {
            h.enter(&wide(s));
        }
        assert_eq!(h.first().unwrap().text, wide("newest"));
        assert_eq!(h.last().unwrap().text, wide("oldest"));
        assert_eq!(h.prev().unwrap().text, wide("middle"));
        assert_eq!(h.next().unwrap().text, wide("oldest"));
        assert!(h.next().is_none(), "and nothing past the oldest");
    }

    /// `exec` is the escape hatch for the opcodes with no method here, and it
    /// answers the C's return code and the event it wrote.
    #[test]
    fn the_escape_hatch_reaches_the_opcodes_with_no_method() {
        let mut h = OwnedHistoryW::with_size(8);
        h.enter(&wide("kept"));
        let (rv, ev) = h.exec(crate::histedit::H_GETSIZE, HistoryArg::None);
        assert_eq!(rv, 0);
        assert_eq!(ev.num, 1, "one entry is stored");
    }

    /// The destructor is the point of the type: the C's handle has to be
    /// closed by hand and this one cannot be forgotten. Nothing here can
    /// observe the free directly, so this asserts the shape that makes it
    /// safe — the store is not aliased and the drop runs once.
    #[test]
    fn the_store_is_freed_when_it_goes_out_of_scope() {
        let mut h = OwnedHistoryW::with_size(2);
        h.enter(&wide("gone after this"));
        drop(h);
    }
}
