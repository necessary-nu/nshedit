//! Tests for the ported `src/history.c`.
//!
//! The two link-level functions are exercised against a bare
//! [`HistoryTGen`] — the store `history_def_init` produces, without the
//! [`HistoryGen`] vtable around it — because that is the only place their
//! contract is visible: `history_def_enter` calls the raw bodies, so the
//! wrappers named by the rules have no in-tree caller of their own.
//!
//! The file-format tests are `cfg`d on the `bsd` feature, as `histfile`'s own
//! are: without it `vis_encode` and `vis_decode_into` are stubs and the
//! `_HiStOrY_V2_` format is one this build neither writes nor reads.

use core::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::hist::{EditorHistory, HistText};
use crate::histedit::{
    H_APPEND, H_CLEAR, H_DEL, H_GETSIZE, H_LOAD, H_NEXT_STR, H_PREV_STR, H_SAVE_FP, H_SET,
};

fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

/// A NUL-terminated wide string, which is what every `const Char *` in this
/// file is.
fn cstr(s: &str) -> Vec<u32> {
    let mut v = wide(s);
    v.push(0);
    v
}

/// The builtin store on its own, with a destructor.
///
/// `history_def_init` hands back a `void *` the caller must remember to clear
/// and free; wrapping it is what keeps these tests from leaking a list apiece.
struct Builtin<C: HistChar> {
    p: *mut c_void,
    _c: PhantomData<C>,
}

impl<C: HistChar> Builtin<C> {
    fn new(max: i32) -> Self {
        let mut p: *mut c_void = ptr::null_mut();
        let mut ev = scratch_ev::<C>();
        assert_eq!(history_def_init::<C>(&mut p, &mut ev, max), 0);
        Self { p, _c: PhantomData }
    }

    fn store(&mut self) -> &mut HistoryTGen<C> {
        // SAFETY: `p` is what `history_def_init` produced above, and nothing
        // else holds a reference to it.
        unsafe { &mut *self.p.cast::<HistoryTGen<C>>() }
    }

    /// The embedded sentinel, which is the one node that is not an entry.
    fn sentinel(&mut self) -> *mut HentryGen<C> {
        ptr::from_mut(&mut self.store().list)
    }

    /// The entry text, newest first.
    fn entries(&mut self) -> Vec<String> {
        let list = self.sentinel();
        let mut out = Vec::new();
        // SAFETY: the list is circular through the sentinel and every node on
        // it is one this module allocated.
        unsafe {
            let mut p = (*list).next;
            while p != list {
                out.push(text((*p).ev.str.cast::<u32>()));
                p = (*p).next;
            }
        }
        out
    }
}

impl<C: HistChar> Drop for Builtin<C> {
    fn drop(&mut self) {
        let mut ev = scratch_ev::<C>();
        // SAFETY: `p` is still the store `history_def_init` produced; the
        // clear frees every entry and the free releases the store itself.
        unsafe {
            history_def_clear::<C>(self.p, &mut ev);
            free_alloc(self.p.cast::<HistoryTGen<C>>());
        }
    }
}

/// A `Char *` the store owns, as a `String`.
fn text(p: *const u32) -> String {
    if p.is_null() {
        return "<null>".into();
    }
    let mut s = String::new();
    let mut i = 0;
    // SAFETY: every string this module stores is NUL-terminated.
    unsafe {
        while *p.add(i) != 0 {
            s.push(char::from_u32(*p.add(i)).unwrap());
            i += 1;
        }
    }
    s
}

fn scratch_path(tag: &str) -> std::path::PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "nshedit-history-{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

// ---------------------------------------------------------------------------
// history_def_insert
// ---------------------------------------------------------------------------

/// Insertion is at the *head*, so "first" is the most recent entry, and the
/// four pointer writes have to leave the circle intact from both ends. Ids
/// come off a counter that only ever increases.
///
/// Nothing is trimmed here: `max` is 0 and both entries survive, because
/// eviction belongs to `history_def_enter` and this function has none. That
/// separation is what makes ERR-history-01 a defect of the *enter* path.
// [spec:libedit:sem:history.history-def-insert-fn/test]
#[test]
fn an_insert_links_at_the_head_and_takes_its_own_copy() {
    let mut b = Builtin::<u32>::new(0);
    let mut ev = scratch_ev::<u32>();

    let src = cstr("first");
    assert_eq!(history_def_insert(b.store(), &mut ev, src.as_ptr()), 0);
    assert_eq!(ev.num, 1, "ids start at 1, so 0 can mean 'no event'");
    assert_eq!(text(ev.str), "first");
    assert_ne!(
        ev.str,
        src.as_ptr(),
        "the store duplicates the caller's string rather than borrowing it"
    );

    let src2 = cstr("second");
    assert_eq!(history_def_insert(b.store(), &mut ev, src2.as_ptr()), 0);
    assert_eq!(ev.num, 2);

    assert_eq!(b.entries(), vec!["second", "first"], "newest first");

    let list = b.sentinel();
    let st = b.store();
    assert_eq!(st.cur, 2);
    assert_eq!(st.eventid, 2);
    assert_eq!(
        st.cursor, st.list.next,
        "an insert always repositions onto the new entry"
    );
    // SAFETY: both nodes are live entries of this store's list.
    unsafe {
        assert_eq!((*st.list.next).prev, list, "the head links back");
        assert_eq!((*st.list.prev).next, list, "and the tail links round");
    }
}

/// A NULL string is defined as `STR("")` rather than reproduced as the C's
/// unchecked `Strdup` (ERR-history-40): the entry exists, it is empty, and
/// every later walk over it is on a defined route.
// [spec:libedit:sem:history.history-def-insert-fn/test]
#[test]
fn a_null_string_becomes_an_empty_entry_rather_than_a_fault() {
    let mut b = Builtin::<u32>::new(0);
    let mut ev = scratch_ev::<u32>();
    assert_eq!(history_def_insert(b.store(), &mut ev, ptr::null()), 0);
    assert_eq!(ev.num, 1);
    assert_eq!(text(ev.str), "");
    assert_eq!(b.store().cur, 1);
}

// ---------------------------------------------------------------------------
// history_def_delete
// ---------------------------------------------------------------------------

/// Deleting is an unlink plus a cursor repair, and the repair is the subtle
/// half: the cursor moves to `prev` — toward *newer* entries — and falls
/// through to `next` only when that would be the sentinel, so deleting the
/// newest entry lands on the second-newest rather than on nothing.
///
/// `eventid` is deliberately not adjusted, so the ids of deleted events are
/// never handed out again; `ev` is accepted and never written, which is why
/// `history_def_del` has to fill it in itself.
// [spec:libedit:sem:history.history-def-delete-fn/test]
#[test]
fn a_delete_repairs_the_cursor_toward_the_newer_entry() {
    let mut b = Builtin::<u32>::new(0);
    let mut ev = scratch_ev::<u32>();
    for s in ["oldest", "middle", "newest"] {
        let z = cstr(s);
        history_def_insert(b.store(), &mut ev, z.as_ptr());
    }

    // A value the delete must leave alone.
    let mut untouched = HistEventGen::<u32> {
        num: 4242,
        str: ptr::null(),
    };

    // The cursor is on "newest" — the insert put it there — so deleting that
    // entry has to move it rather than leave it dangling.
    let list = b.sentinel();
    // SAFETY: the head of a three-entry list is a real entry.
    let newest = unsafe { (*list).next };
    history_def_delete(b.store(), &mut untouched, newest);

    assert_eq!(untouched.num, 4242, "`ev` is threaded through and ignored");
    assert!(untouched.str.is_null());
    assert_eq!(b.entries(), vec!["middle", "oldest"]);
    // SAFETY: the cursor is on a live entry, as the repair guarantees.
    assert_eq!(text(unsafe { (*b.store().cursor).ev.str }), "middle");
    assert_eq!(b.store().cur, 2);
    assert_eq!(
        b.store().eventid,
        3,
        "the id counter does not rewind, so ids are never reused"
    );

    // An entry the cursor is not on leaves the cursor where it is.
    // SAFETY: the tail of a two-entry list is a real entry.
    let oldest = unsafe { (*list).prev };
    history_def_delete(b.store(), &mut untouched, oldest);
    assert_eq!(b.entries(), vec!["middle"]);
    // SAFETY: as above.
    assert_eq!(text(unsafe { (*b.store().cursor).ev.str }), "middle");

    // And the sole remaining entry leaves it parked on the sentinel, which is
    // the "no current event" state.
    // SAFETY: as above.
    let sole = unsafe { (*list).next };
    history_def_delete(b.store(), &mut untouched, sole);
    assert!(b.entries().is_empty());
    assert_eq!(b.store().cur, 0);
    assert_eq!(b.store().cursor, list);
    assert_eq!(b.store().eventid, 3);
}

/// The sentinel is the list header embedded in the store, not an entry, and
/// unlinking it would fold the list into itself. The C calls `abort()`; a
/// panic is the same contract, and no caller in this file can reach it.
// [spec:libedit:sem:history.history-def-delete-fn/test]
#[test]
#[should_panic(expected = "sentinel")]
fn deleting_the_list_header_is_a_programming_error() {
    let mut b = Builtin::<u32>::new(0);
    let mut ev = scratch_ev::<u32>();
    let list = b.sentinel();
    history_def_delete(b.store(), &mut ev, list);
}

// ---------------------------------------------------------------------------
// Event numbering, uniqueness and the dispatcher
// ---------------------------------------------------------------------------

/// Ids are a strictly increasing counter, not indices: deleting an entry
/// leaves a hole the next insert does not fill. Only `H_CLEAR` resets the
/// counter, and it resets it all the way, so ids after a clear collide with
/// ids from before it — which is why an application must not cache them
/// across one.
///
/// `H_GETSIZE` reports the number of events *stored* rather than the maximum
/// `H_SETSIZE` configured (ERR-history-34); no opcode answers the maximum.
// [spec:libedit:sem:history.history-def-enter-fn/test]
// [spec:libedit:sem:history.history-def-clear-fn/test]
// [spec:libedit:sem:history.history-getsize-fn/test]
#[test]
fn event_ids_are_never_reused_until_the_history_is_cleared() {
    let mut h = OwnedHistoryW::with_size(8);
    for s in ["a", "b", "c"] {
        assert!(h.enter(&wide(s)));
    }

    let (rv, ev) = h.exec(H_DEL, HistoryArg::Num(2));
    assert_eq!(rv, 0);
    assert_eq!((ev.num, text(ev.str)), (2, "b".to_string()));

    assert!(h.enter(&wide("d")));
    let mut nums = Vec::new();
    let mut cur = h.first();
    while let Some(l) = cur {
        nums.push(l.num);
        cur = h.next();
    }
    assert_eq!(nums, vec![4, 3, 1], "2 is gone and nothing took its place");

    let (rv, ev) = h.exec(H_GETSIZE, HistoryArg::None);
    assert_eq!(rv, 0);
    assert_eq!(
        ev.num, 3,
        "the count of stored events, not the maximum of 8"
    );

    h.exec(H_CLEAR, HistoryArg::None);
    assert!(h.enter(&wide("after clear")));
    assert_eq!(h.first().unwrap().num, 1, "the counter restarted");
}

/// ERR-history-25, reproduced. `H_ENTER` answers 1 for a real insert and 0
/// for one uniqueness suppressed — both success — and the dispatcher stores
/// `ev->num` into `h_ent` whenever the answer is not -1. A suppressed enter
/// never wrote `*ev`, so what gets stored is the `_HE_OK` prologue's 0, which
/// matches no event because ids start at 1. The next `H_APPEND` then fails
/// against an entry that is sitting right there.
// [spec:libedit:sem:history.funw-history-fn/test]
#[test]
fn a_suppressed_duplicate_breaks_the_following_append() {
    let mut h = OwnedHistoryW::with_size(8);
    h.set_unique(true);
    assert!(h.enter(&wide("same")));
    assert!(!h.enter(&wide("same")), "the repeat was suppressed");

    let more = cstr(" appended");
    let (rv, ev) = h.exec(H_APPEND, HistoryArg::Str(more.as_ptr()));
    assert_eq!(rv, -1);
    assert_eq!(text(ev.str), "event not found");
    assert_eq!(ev.num, 9);

    // The entry itself is untouched, so the failure is purely the lost anchor.
    assert_eq!(h.first().unwrap().text, HistText::Wide(wide("same")));
}

/// ERR-history-36: `H_PREV_STR` walks toward *older* entries and `H_NEXT_STR`
/// toward *newer* ones — the exact opposite pairing from `H_PREV_EVENT` and
/// `H_NEXT_EVENT`, and the opposite of what the names say. The test itself is
/// a prefix test that includes the current event, so the direction is the only
/// thing distinguishing these two answers.
// [spec:libedit:sem:history.history-prev-string-fn/test]
// [spec:libedit:sem:history.history-next-string-fn/test]
#[test]
fn the_two_string_searches_walk_the_directions_their_names_deny() {
    let mut h = OwnedHistoryW::with_size(8);
    for s in ["echo old", "middle", "echo new"] {
        h.enter(&wide(s));
    }
    let pat = cstr("echo");
    let miss = cstr("zzz");

    h.exec(H_SET, HistoryArg::Num(2));
    let (rv, ev) = h.exec(H_PREV_STR, HistoryArg::Str(pat.as_ptr()));
    assert_eq!(rv, 0);
    assert_eq!((ev.num, text(ev.str)), (1, "echo old".to_string()));

    h.exec(H_SET, HistoryArg::Num(2));
    let (rv, ev) = h.exec(H_NEXT_STR, HistoryArg::Str(pat.as_ptr()));
    assert_eq!(rv, 0);
    assert_eq!((ev.num, text(ev.str)), (3, "echo new".to_string()));

    h.exec(H_SET, HistoryArg::Num(2));
    let (rv, ev) = h.exec(H_PREV_STR, HistoryArg::Str(miss.as_ptr()));
    assert_eq!(rv, -1);
    assert_eq!(text(ev.str), "event not found");
}

// ---------------------------------------------------------------------------
// The on-disk format
// ---------------------------------------------------------------------------

/// The whole grammar in one assertion: a 13-byte cookie, then one
/// `strvis(…, VIS_WHITE)` line per entry, **oldest first**. Writing oldest
/// first is what makes `history_load`'s top-to-bottom enter restore the
/// original order, and `VIS_WHITE` is what guarantees no encoded entry can
/// contain a literal LF — which is what makes one line one entry.
// [spec:libedit:sem:history.history-save-fp-fn/test]
#[cfg(feature = "bsd")]
#[test]
fn the_saved_file_is_a_cookie_and_one_escaped_line_per_entry() {
    let mut h = OwnedHistoryW::with_size(8);
    for s in ["one two", "back\\slash", "tab\there"] {
        h.enter(&wide(s));
    }
    let mut out: Vec<u8> = Vec::new();
    let (rv, _) = h.exec(
        H_SAVE_FP,
        HistoryArg::Fp(SaveStream {
            at_start: true,
            out: &mut out,
        }),
    );
    assert_eq!(rv, 3);
    assert_eq!(
        out,
        b"_HiStOrY_V2_\none\\040two\nback\\134slash\ntab\\011here\n"
    );
}

/// ERR-history-20, reproduced by leaving the decision with the caller: the
/// cookie is written only when the stream is at offset 0. On a pipe or socket
/// `ftell` answers -1, the header is skipped, and `history_load` later rejects
/// the file outright — a silent data-loss path that a tidier port would have
/// closed, so it is pinned here instead.
// [spec:libedit:sem:history.history-save-fp-fn/test]
#[cfg(feature = "bsd")]
#[test]
fn a_stream_that_cannot_report_its_position_gets_no_header() {
    let mut h = OwnedHistoryW::with_size(8);
    h.enter(&wide("only"));
    let mut out: Vec<u8> = Vec::new();
    let (rv, _) = h.exec(
        H_SAVE_FP,
        HistoryArg::Fp(SaveStream {
            at_start: false,
            out: &mut out,
        }),
    );
    assert_eq!(rv, 1);
    assert_eq!(out, b"only\n", "the entry, and nothing to identify it");
}

/// ERR-history-19, reproduced. `H_NSAVE_FP`'s positioning loop tests a
/// *post*-decrement, so it stops **on** the entry `nelem` back from the newest
/// and the write loop then emits that entry plus every newer one:
/// `min(nelem + 1, size)` entries, not `nelem`. `nelem == 0` writes one.
// [spec:libedit:sem:history.history-save-fp-fn/test]
#[cfg(feature = "bsd")]
#[test]
fn the_bounded_save_writes_one_more_entry_than_asked_for() {
    use crate::histedit::H_NSAVE_FP;

    let mut h = OwnedHistoryW::with_size(8);
    for s in ["a", "b", "c", "d"] {
        h.enter(&wide(s));
    }
    let expected: [&[u8]; 5] = [
        b"d\n",
        b"c\nd\n",
        b"b\nc\nd\n",
        b"a\nb\nc\nd\n",
        b"a\nb\nc\nd\n",
    ];
    for (n, want) in expected.iter().enumerate() {
        let mut out: Vec<u8> = Vec::new();
        let (rv, _) = h.exec(
            H_NSAVE_FP,
            HistoryArg::NSaveFp(
                n,
                SaveStream {
                    at_start: false,
                    out: &mut out,
                },
            ),
        );
        assert_eq!(out, *want, "nelem = {n}");
        assert_eq!(
            usize::try_from(rv).unwrap(),
            want.iter().filter(|&&b| b == b'\n').count()
        );
    }
}

/// The round trip, which is the property the format exists for: what a save
/// writes, a load reads back as the same entries in the same order. Only the
/// text survives — no ids, no timestamps, no per-entry data — so the reloaded
/// store renumbers from 1.
///
/// The non-ASCII entry is the interesting one. The file is bytes in both
/// builds, so the wide store encodes to the locale on the way out and decodes
/// on the way back in, and `VIS_WHITE` escapes each of those bytes
/// individually.
// [spec:libedit:sem:history.history-save-fn/test]
// [spec:libedit:sem:history.history-load-fn/test]
#[cfg(feature = "bsd")]
#[test]
fn a_saved_history_reloads_into_the_same_entries_in_the_same_order() {
    use crate::histedit::H_SAVE;

    let path = scratch_path("roundtrip");
    let p = path.to_str().unwrap();
    let entries = ["one two", "échò", "tab\there"];

    let mut h = OwnedHistoryW::with_size(8);
    for s in entries {
        h.enter(&wide(s));
    }
    assert_eq!(h.exec(H_SAVE, HistoryArg::Path(p)).0, 3);

    let mut g = OwnedHistoryW::with_size(8);
    assert_eq!(g.exec(H_LOAD, HistoryArg::Path(p)).0, 3);
    let _ = std::fs::remove_file(&path);

    let mut got = Vec::new();
    let mut cur = g.first();
    while let Some(l) = cur {
        got.push((l.num, l.text));
        cur = g.next();
    }
    let want: Vec<_> = entries
        .iter()
        .rev()
        .enumerate()
        .map(|(i, s)| {
            (
                i32::try_from(entries.len() - i).unwrap(),
                HistText::Wide(wide(s)),
            )
        })
        .collect();
    assert_eq!(got, want);
}

/// ERR-history-22, reproduced exactly, `strncmp`'s NUL rule included: the
/// cookie check compares only as many bytes as the first line actually held,
/// so a file whose entire content is a proper prefix of the cookie is
/// *accepted* and loads zero entries. A first line longer than the cookie can
/// never match, because the cookie's own terminator stops the comparison at a
/// mismatch — and that rejection is `H_LOAD`'s only error, reported through
/// `ev` as "can't read history from file".
// [spec:libedit:sem:history.history-load-fn/test]
#[test]
fn a_truncated_cookie_is_accepted_and_a_wrong_one_is_not() {
    let path = scratch_path("cookie");
    let mut h = OwnedHistoryW::with_size(8);

    std::fs::write(&path, b"_HiS").unwrap();
    let (rv, _) = h.exec(H_LOAD, HistoryArg::Path(path.to_str().unwrap()));
    assert_eq!(rv, 0, "accepted, with nothing after it to read");

    std::fs::write(&path, b"_HiStOrY_V3_\nfoo\n").unwrap();
    let (rv, ev) = h.exec(H_LOAD, HistoryArg::Path(path.to_str().unwrap()));
    let _ = std::fs::remove_file(&path);
    assert_eq!(rv, -1);
    assert_eq!(text(ev.str), "can't read history from file");
    assert_eq!(ev.num, 10);
}

/// Without the `bsd` feature there is no vis, so `_HiStOrY_V2_` is a format
/// this build cannot write: the header goes out and the first entry stops the
/// save dead. Pinned rather than left implicit, because the failure is quiet
/// at the call site — the result is -1 and a file holding nothing but a
/// header, which `history_load` will then accept and read as empty.
// [spec:libedit:sem:history.history-save-fp-fn/test]
#[cfg(not(feature = "bsd"))]
#[test]
fn a_build_without_vis_cannot_write_the_legacy_format() {
    let mut h = OwnedHistoryW::with_size(8);
    h.enter(&wide("only"));
    let mut out: Vec<u8> = Vec::new();
    let (rv, ev) = h.exec(
        H_SAVE_FP,
        HistoryArg::Fp(SaveStream {
            at_start: true,
            out: &mut out,
        }),
    );
    assert_eq!(rv, -1);
    assert_eq!(text(ev.str), "can't write history");
    assert_eq!(out, b"_HiStOrY_V2_\n", "the header, and no entry at all");
}
