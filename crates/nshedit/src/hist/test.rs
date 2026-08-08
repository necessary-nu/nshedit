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

/// Growing the stash keeps the saved line and zeroes only what it added.
///
/// The C rebases `last` across the reallocation because it is a pointer; here
/// it is already an offset, so the rebase is nothing at all and the recorded
/// length has to come through untouched.
// [spec:libedit:sem:hist.hist-enlargebuf-fn/test]
#[test]
fn enlarging_the_stash_preserves_the_saved_line() {
    let mut el = editor();
    assert_eq!(hist_init(&mut el), 0);
    let saved = wide("echo saved");
    el.el_history.buf[..saved.len()].copy_from_slice(&saved);
    el.el_history.last = saved.len();

    assert_eq!(hist_enlargebuf(&mut el, 4 * EL_BUFSIZ), 1, "1 is success");
    assert_eq!(el.el_history.sz, 4 * EL_BUFSIZ);
    assert_eq!(el.el_history.buf.len(), el.el_history.sz);
    assert_eq!(&el.el_history.buf[..saved.len()], &saved[..]);
    assert!(
        el.el_history.buf[saved.len()..].iter().all(|&c| c == 0),
        "everything past the saved line is NUL"
    );
    assert_eq!(el.el_history.last, saved.len(), "the length is preserved");
}

/// The stash is never shrunk, and because the convention is inverted the
/// refusal to shrink is reported as success — 1, not 0. Nothing is
/// reallocated and nothing is zeroed, so a saved line survives a run of
/// pointless requests.
// [spec:libedit:sem:hist.hist-enlargebuf-fn/test]
#[test]
fn a_request_no_larger_than_the_stash_succeeds_without_growing() {
    let mut el = editor();
    hist_init(&mut el);
    el.el_history.buf[0] = u32::from(b'x');
    el.el_history.last = 1;

    for newsz in [0, 1, EL_BUFSIZ - 1, EL_BUFSIZ] {
        assert_eq!(hist_enlargebuf(&mut el, newsz), 1, "newsz {newsz}");
        assert_eq!(el.el_history.sz, EL_BUFSIZ);
        assert_eq!(el.el_history.buf.len(), EL_BUFSIZ);
    }
    assert_eq!(el.el_history.buf[0], u32::from(b'x'));
    assert_eq!(el.el_history.last, 1);
}

/// An impossible allocation returns 0 — failure — and leaves every field as
/// it was, with the old stash still holding the saved line.
///
/// `ch_enlargebufs` is built on that guarantee: it turns the 0 into its own 0
/// and the editor carries on, having already grown the line buffer. The stash
/// is then smaller than the line, which `hist_get` tolerates because it copies
/// `sz` characters into a buffer at least that large.
// [spec:libedit:sem:hist.hist-enlargebuf-fn/test]
#[test]
fn a_refused_allocation_leaves_the_stash_exactly_as_it_was() {
    let mut el = editor();
    hist_init(&mut el);
    let saved = wide("still here");
    el.el_history.buf[..saved.len()].copy_from_slice(&saved);
    el.el_history.last = saved.len();

    assert_eq!(hist_enlargebuf(&mut el, usize::MAX), 0, "0 is failure");
    assert_eq!(el.el_history.sz, EL_BUFSIZ);
    assert_eq!(el.el_history.buf.len(), EL_BUFSIZ);
    assert_eq!(&el.el_history.buf[..saved.len()], &saved[..]);
    assert_eq!(el.el_history.last, saved.len());

    // Still usable, which is what "the old buffer is still valid" means.
    assert_eq!(hist_enlargebuf(&mut el, 2 * EL_BUFSIZ), 1);
    assert_eq!(&el.el_history.buf[..saved.len()], &saved[..]);
}

/// A `hist_init` whose allocation failed leaves an empty stash and `sz` at 0,
/// and the sole caller discards that error deliberately: the first line-buffer
/// growth allocates from nothing and silently repairs the state.
///
/// `editor()` is that state — it runs `ch_init` and `search_init` and never
/// `hist_init` — so this is also what an editor assembled without the stash
/// recovers to.
// [spec:libedit:sem:hist.hist-enlargebuf-fn/test]
#[test]
fn the_first_growth_repairs_a_stash_that_was_never_allocated() {
    let mut el = editor();
    assert_eq!((el.el_history.sz, el.el_history.last), (0, 0));
    assert!(el.el_history.buf.is_empty());

    assert_eq!(hist_enlargebuf(&mut el, EL_BUFSIZ), 1);
    assert_eq!(el.el_history.sz, EL_BUFSIZ);
    assert_eq!(el.el_history.buf, vec![0u32; EL_BUFSIZ]);
    assert_eq!(el.el_history.last, 0);
}

/// The stash and the line buffer grow together. That lockstep is what makes
/// `hist_get`'s restore safe: it copies `sz` characters into the line buffer
/// with no bound of its own, so `sz` must never outrun the line's allocation.
/// `ch_enlargebufs` is the one caller and this is the whole of its last step.
// [spec:libedit:sem:hist.hist-enlargebuf-fn/test]
#[test]
fn the_stash_and_the_line_buffer_grow_in_lockstep() {
    let mut el = editor();
    hist_init(&mut el);
    assert_eq!(el.el_history.sz, EL_BUFSIZ);

    assert_eq!(ch_enlargebufs(&mut el, 1), 1);
    assert!(el.el_history.sz > EL_BUFSIZ, "the stash grew too");
    assert_eq!(el.el_history.sz, el.el_line.buffer.len());
}

/// A narrow C history: the shape `NARROW_HISTORY` exists to describe, where
/// the dispatcher fills a `HistEvent` carrying a `char *` through a pointer
/// the caller declared `HistEventW *`.
///
/// The four traversal opcodes answer differently so that one hook reaches
/// every path out of `hist_convert`. Variadic because `hist_fun_t` is; the
/// trailing argument is never read, since reading one needs `VaListImpl`,
/// and every call libedit itself makes passes NULL for it.
unsafe extern "C" fn narrow_hook(
    _ref: *mut c_void,
    ev: *mut HistEventW,
    op: c_int,
    _: ...
) -> c_int {
    let ev = ev.cast::<HistEvent>();
    // SAFETY: the pointer is the local `hist_convert_str` declared as the
    // narrow event it really is, and it outlives this call. Both string
    // literals are `'static`.
    unsafe {
        match op {
            H_FIRST => {
                (*ev).num = 7;
                (*ev).str = c"echo hi".as_ptr();
                0
            }
            // `0xFF` is not a lead byte in either charset, so the decode
            // fails whatever the environment says.
            H_LAST => {
                (*ev).num = 8;
                (*ev).str = c"\xff".as_ptr();
                0
            }
            H_NEXT => {
                (*ev).num = 9;
                (*ev).str = ptr::null();
                0
            }
            _ => -1,
        }
    }
}

/// `wide`, NUL-terminated, for a store whose event string really is wide.
static WIDE_ENTRY: [u32; 5] = [b'w' as u32, b'i' as u32, b'd' as u32, b'e' as u32, 0];

/// The other half of the `NARROW_HISTORY` split: a store the editor reads
/// straight out of the shared event cookie, with no conversion at all.
unsafe extern "C" fn wide_hook(_ref: *mut c_void, ev: *mut HistEventW, op: c_int, _: ...) -> c_int {
    if op != H_FIRST {
        return -1;
    }
    // SAFETY: `hist_fun` passes `&mut el.el_history.ev`, which outlives the
    // call, and the entry is `'static`.
    unsafe {
        (*ev).num = 7;
        (*ev).str = WIDE_ENTRY.as_ptr();
    }
    0
}

/// Installs `fun` as the C ABI dispatcher with a NULL cookie.
///
/// NULL on purpose: `hist_convert` calls through `fun` without consulting the
/// cookie, while every recall path guards on the cookie alone. The C is
/// inconsistent about that pair and the port reproduces it, so a dispatcher
/// the guards read as detached still answers here.
fn attach_c(el: &mut EditLine, fun: HistFunT) {
    el.el_history.src = HistSource::CAbi {
        fun,
        cookie: ptr::null_mut(),
    };
}

/// A narrow store's bytes reach the editor decoded, through `el_scratch`.
///
/// ERR-history-03: the C declares the event wide and lets the narrow store
/// write a `HistEvent` through it, then reinterprets `ev.str`. The port
/// declares it narrow, which is what it is, and `narrow_hook` writes it as
/// the narrow one — so the pointer cast at the call is the C ABI's shape and
/// not a reinterpretation of the value.
///
/// `hist_convert` hands back the raw pointer the C does — the base of
/// `el_scratch.wbuff`, with the terminator `ct_decode_string` wrote sitting
/// one past the end of the text.
// [spec:libedit:sem:hist.hist-convert-fn/test]
#[test]
fn a_narrow_entry_is_decoded_through_the_scratch_buffer() {
    let mut el = editor();
    attach_c(&mut el, narrow_hook);

    let decoded = hist_convert_str(&mut el, H_FIRST, ptr::null_mut());
    assert_eq!(decoded, Some(&wide("echo hi")[..]));

    let p = hist_convert(&mut el, H_FIRST, ptr::null_mut());
    assert!(!p.is_null());
    assert_eq!(p.cast_const(), el.el_scratch.wbuff.as_ptr());
    assert_eq!(el.el_scratch.wbuff[7], 0, "NUL-terminated for the C caller");
}

/// Every way the narrow path can fail is the same NULL, and a caller cannot
/// tell them apart: the dispatcher reporting -1, an entry with no string at
/// all, and bytes the locale cannot decode. All three surface to `hist_get`
/// and the search commands as "no such entry".
// [spec:libedit:sem:hist.hist-convert-fn/test]
#[test]
fn every_narrow_failure_is_the_same_null() {
    let mut el = editor();
    attach_c(&mut el, narrow_hook);

    assert!(
        hist_convert(&mut el, H_PREV, ptr::null_mut()).is_null(),
        "the dispatcher's -1"
    );
    assert!(
        hist_convert(&mut el, H_NEXT, ptr::null_mut()).is_null(),
        "a NULL event string: `ct_decode_string(NULL, …)` is NULL"
    );
    assert!(
        hist_convert(&mut el, H_LAST, ptr::null_mut()).is_null(),
        "an invalid multibyte sequence rejects the whole entry"
    );
}

/// ERR-history-18, reproduced. The narrow path fills a local event, so
/// `el_history.ev` keeps the all-zero value the `EditLine` started with no
/// matter how many entries are fetched. The wide path writes the cookie
/// instead, and that is the contrast: `vi_to_history_line` reads `ev.num` to
/// turn a count into an event number, so vi `G` with a count works for a wide
/// application and is inoperative for a narrow one.
// [spec:libedit:sem:hist.hist-convert-fn/test]
#[test]
fn the_narrow_path_never_writes_the_shared_event_cookie() {
    let mut el = editor();
    el.el_flags |= crate::el::NARROW_HISTORY;
    attach_c(&mut el, narrow_hook);

    assert_eq!(hist_first(&mut el), Some(wide("echo hi")));
    assert_eq!(el.el_history.ev.num, 0, "the hook filled a local");
    assert!(el.el_history.ev.str.is_null());
    // Twice, in case the first fetch were the only one that missed it.
    assert_eq!(hist_first(&mut el), Some(wide("echo hi")));
    assert_eq!(el.el_history.ev.num, 0);

    let mut el = editor();
    attach_c(&mut el, wide_hook);
    assert_eq!(hist_first(&mut el), Some(wide("wide")));
    assert_eq!(el.el_history.ev.num, 7, "the wide path publishes it");
}

/// `hist_convert` reaches the C ABI's dispatcher and nothing else. With no
/// history it is NULL, and with a Rust history it is NULL as well — the Rust
/// seam is taken in `hist_fun` before the width split, so a Rust store is
/// decoded without ever coming through here, `NARROW_HISTORY` or not.
///
/// The C's precondition is ERR-history-04: it calls through `fun` with no
/// NULL check, relying on the call sites having tested `ref`. `hist_set`
/// rejects the pair that makes those two disagree, so the only thing left for
/// this function to meet is the state below, where there is no dispatcher at
/// all.
// [spec:libedit:sem:hist.hist-convert-fn/test]
#[test]
fn only_a_c_dispatcher_is_reachable_from_here() {
    let mut el = editor();
    assert!(
        hist_convert(&mut el, H_FIRST, ptr::null_mut()).is_null(),
        "no history at all"
    );

    let mut el = editor();
    attach(&mut el, &["echo hi"]);
    assert!(hist_convert(&mut el, H_FIRST, ptr::null_mut()).is_null());
    assert_eq!(
        hist_first(&mut el),
        Some(wide("echo hi")),
        "and the same store still reaches the editor"
    );

    // The flag is the only thing that could route a fetch through
    // `hist_convert`, and it does not apply to a Rust store.
    el.el_flags |= crate::el::NARROW_HISTORY;
    assert_eq!(hist_first(&mut el), Some(wide("echo hi")));
}

/// The decoded string *is* `el_scratch.wbuff`, so two conversions hand back
/// one pointer and the first result is gone the moment anything else decodes
/// into that buffer.
///
/// This is why `hist_convert_str` exists beside `hist_convert`: inside this
/// file the borrow is kept and the compiler enforces what the C left to the
/// caller to remember.
// [spec:libedit:sem:hist.hist-convert-fn/test]
#[test]
fn the_decoded_string_is_the_scratch_buffer_and_dies_with_the_next_decode() {
    let mut el = editor();
    attach_c(&mut el, narrow_hook);

    let first = hist_convert(&mut el, H_FIRST, ptr::null_mut());
    let second = hist_convert(&mut el, H_FIRST, ptr::null_mut());
    assert_eq!(first, second, "one buffer, not two");

    let replaced = ct_decode_string(Some(b"zzzzzzz"), &mut el.el_scratch)
        .expect("ASCII decodes in either charset")
        .to_vec();
    assert_eq!(replaced, wide("zzzzzzz"));
    assert_eq!(
        first.cast_const(),
        el.el_scratch.wbuff.as_ptr(),
        "grow-only, and both strings fit, so nothing was reallocated"
    );
    // SAFETY: `first` is the base of `el_scratch.wbuff`, just confirmed
    // unmoved and longer than one element.
    assert_eq!(
        unsafe { *first },
        u32::from(b'z'),
        "the entry the first call returned is no longer there"
    );
}
