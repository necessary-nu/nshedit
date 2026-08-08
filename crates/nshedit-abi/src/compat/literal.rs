//! Ported from `src/literal.c`; rules live in
//! `docs/spec/port/src/literal.md`.

use crate::chartype::{MB_FILL_CHAR, ct_encode_char};
use crate::locale::{self, MB_LEN_MAX};

/// C: `EL_LITERAL` in `src/literal.h` — `(wint_t)0x80000000`, bit 31 alone.
///
/// A successful [`literal_add`] returns this bit ORed with the table index,
/// so every success is `>= 0x8000_0000` and a return of `0` is an unambiguous
/// failure. `pub(crate)` because the sentinel is not this module's private
/// business: `terminal_putc` has to test the bit, and `refresh`'s screen
/// image has to be able to carry a value that is not a Unicode scalar.
pub(crate) const EL_LITERAL: u32 = 0x8000_0000;

/// Highest table index a sentinel may carry: `0x7FFF_FFFE`.
///
/// Derived from `chartype`'s [`MB_FILL_CHAR`], which is `(wint_t)-1` and so
/// has bit 31 set: `terminal_putc` **must** keep testing
/// `c == MB_FILL_CHAR` *before* `c & EL_LITERAL` — see
/// `sem:literal.literal-add-fn`, "Distinguishing a sentinel from a real
/// character". That ordering is load-bearing, and [`literal_add`] holds up its
/// end of it by never issuing the one sentinel the earlier test would swallow.
///
/// The rule states the representable range as `0..=0x7FFF_FFFF`, but the top
/// value is not *usable*: `EL_LITERAL | 0x7FFF_FFFF` is `0xFFFF_FFFF`, which
/// is `MB_FILL_CHAR`, and `terminal_putc` tests that first. A literal parked
/// at that index would be consumed as multibyte padding and never printed.
/// The C bound-checks nothing at all — past the range the index aliases the
/// marker bit and the `size_t` → `wint_t` narrowing truncates on LP64 — and
/// `sem:literal.literal-add-fn` directs a port to "bound the index explicitly
/// or fail rather than silently wrap". [`literal_add`] fails past this.
const LITERAL_INDEX_MAX: u32 = (MB_FILL_CHAR & !EL_LITERAL) - 1;

// [spec:libedit:def:literal.el-literal-t]
/// The literal table: the invisible byte sequences the prompt asked to be
/// emitted without occupying screen columns.
///
/// A successful `literal_add` returns `EL_LITERAL | index`, where
/// `EL_LITERAL` is `(wint_t)0x80000000` — bit 31 alone. That is why the
/// screen image is `u32` throughout and never `char`: the sentinel is not a
/// Unicode scalar value, so `char` cannot hold it at all. See
/// `sem:literal.literal-add-fn`.
pub struct ElLiteralT {
    /// C: `char **l_buf` — array of owned byte strings, each the multibyte
    /// encoding of one literal sequence plus its trailing visible character.
    /// The NUL the C appended is not stored; the length is the length.
    pub l_buf: Vec<Vec<u8>>,
    /// C: `size_t l_idx` — max in use, which is `l_buf.len()` for as long as
    /// this module is the only writer. Kept because the `sem` rules index by
    /// it and because a test can drive it out of step with `l_buf` to model
    /// the stale sentinel of ERR-terminal-09, which is a state the C reaches
    /// through a dangling pointer and this port cannot otherwise reach at all.
    pub l_idx: usize,
    /// C: `size_t l_len` — max allocated, and the *requested* capacity rather
    /// than the granted one, which is where it stops duplicating
    /// `l_buf.capacity()`.
    ///
    /// Growth is a fixed +4 slots per reallocation, not doubling, and this
    /// field is the only place that cadence is visible: drop it and
    /// `literal_add` becomes a bare `push` with the `Vec`'s geometric growth,
    /// which is a behaviour `sem:literal.literal-add-fn` states. It is also
    /// what `literal_clear`'s early return tests.
    pub l_len: usize,
}

// [spec:libedit:def:literal.literal-init-fn]
// [spec:libedit:sem:literal.literal-init-fn]
pub(crate) fn literal_init(el: &mut crate::el::EditLine) {
    // The C is `memset(&el->el_literal, 0, sizeof(*l))`, which for these
    // three fields means a NULL table and both counters 0. No table is
    // allocated, so the first `literal_add` is the one that pays for the
    // initial four slots. Written field by field rather than as a memset,
    // as the rule directs: the memset relies on all-bits-zero being a null
    // `char **` representation.
    let l = &mut el.el_literal;
    l.l_buf = Vec::new();
    l.l_idx = 0;
    l.l_len = 0;

    // The C frees nothing here: run on a store that already holds entries it
    // overwrites `l_buf` and leaks the pointer array together with every
    // byte string in it. Nothing in libedit does that — the sole caller is
    // `el_init_internal`, once per `EditLine` — and a leak is not observable
    // across the C ABI, so the assignment above drops the old table instead.
    // Per [dec:libedit:conformance-policy] that is a `fix`, not a defect
    // reproduced.
}

// [spec:libedit:def:literal.literal-end-fn]
// [spec:libedit:sem:literal.literal-end-fn]
pub(crate) fn literal_end(el: &mut crate::el::EditLine) {
    // The whole body, in the C too: the table and the byte strings it points
    // at are the store's only resources. Idempotent, because `literal_clear`
    // leaves `l_len == 0` and a second call takes its early return, and it
    // leaves the store in exactly the state `literal_init` produces.
    //
    // The rule suggests this becomes a `Drop` impl in Rust. The signature is
    // fixed for this wave, so the explicit teardown call stays; `ElLiteralT`
    // owning its `Vec` already means dropping the `EditLine` releases the
    // table whether or not anyone calls this.
    literal_clear(el);
}

// [spec:libedit:def:literal.literal-clear-fn]
// [spec:libedit:sem:literal.literal-clear-fn]
pub(crate) fn literal_clear(el: &mut crate::el::EditLine) {
    let l = &mut el.el_literal;

    // Step 1. The guard is on `l_len`, the allocated capacity, not on
    // `l_idx`, the in-use count — so a table that has capacity but no live
    // entries still runs the body. This is what makes the function safe on a
    // freshly initialised (or already cleared) store, and therefore
    // idempotent.
    if l.l_len == 0 {
        return;
    }

    // Steps 2 and 3. The C frees `l_buf[i]` for `i` in `[0, l_idx)` — the
    // in-use prefix only, because the slots between `l_idx` and `l_len` came
    // from `el_realloc` and were never written, so their contents are
    // indeterminate and freeing them would be undefined. That distinction is
    // why "max in use" is tracked separately from "max allocated". Rust has
    // no such slots: `l_buf` holds exactly `l_idx` owned strings and the
    // extra capacity holds nothing, so replacing it drops each string and
    // then the array, in that order.
    //
    // Step 4. A full deallocation, not a capacity-preserving reset: the next
    // `literal_add` starts over from a fresh four-element table and indices
    // restart at 0. `Vec::new()` rather than `clear()`, for exactly that.
    l.l_buf = Vec::new();
    l.l_len = 0;
    l.l_idx = 0;

    // Every sentinel ever issued is stale from here. There is no generation
    // counter and no tombstone, and `re_refresh` calls this at the top of
    // every full redraw while `el_display` still holds the *previous*
    // frame's sentinels — which `terminal_move_to_char` then re-emits
    // through `terminal_overwrite` → `terminal_putc` → `literal_get`. That
    // is ERR-terminal-09; `literal_get` is where the port defines what
    // happens.
}

// [spec:libedit:def:literal.literal-add-fn]
// [spec:libedit:sem:literal.literal-add-fn]
/// `end` is the C's `end` pointer expressed as an index into `buf`, the two
/// being pointers into the same string: the literal sequence is `buf[..end]`
/// and the visible character the C reads as `end[1]` is `buf[end + 1]`.
///
/// Returns the C's `wint_t`: `EL_LITERAL | index` on success, 0 on failure.
pub(crate) fn literal_add(
    el: &mut crate::el::EditLine,
    buf: &[u32],
    end: usize,
    wp: &mut i32,
) -> u32 {
    // The C dereferences `end[1]` unconditionally and relies on the caller to
    // have guaranteed that both `end` and `end + 1` are in bounds;
    // `prompt_print` enforces it by declining to call when the closing
    // delimiter or the character after it is the string terminator (dropping
    // such a trailing literal silently — ERR-terminal-58). A caller that has
    // not is undefined behaviour in the C and a panic here, so it is defined
    // instead as the same outcome `prompt_print` already produces: the
    // literal is dropped. `*wp` is written first, as on every other path, and
    // -1 makes `re_putliteral`'s `c == 0 || w < 0` test abandon the literal
    // rather than read this as an allocation failure.
    let Some(visible) = end.checked_add(1).and_then(|i| buf.get(i)).copied() else {
        *wp = -1;
        return 0;
    };

    // Step 1. The out-parameter is written unconditionally, before any early
    // return, so it is set on every path including the failure paths. It
    // reports the column width of the *visible* character alone; the escape
    // sequence is zero columns by construction, which is the entire point of
    // the mechanism.
    let w = locale::wcwidth(locale::charset(), visible);
    *wp = w;

    // Step 2. Non-printable visible character: return at once. Nothing is
    // allocated, nothing is appended, and `*wp` keeps the negative value —
    // which is how `re_putliteral` tells this apart from an allocation
    // failure.
    if w < 0 {
        return 0;
    }

    // Steps 3, 4 and 5, fused. The C measures the byte length with
    // `ct_enc_width` (`wcrtomb` from a zeroed `mbstate_t`), allocates
    // `w + 1`, then fills it with `ct_encode_char` (`wctomb`, which carries
    // libc's process-global conversion state). In a stateful encoding the two
    // disagree: `wctomb` emits shift sequences the measuring pass did not
    // budget for, `n` runs past `w`, and the writes overflow the heap block.
    // The guard inside `ct_encode_char` cannot catch it because the
    // remaining-space argument is `(size_t)(w - n)` and a negative difference
    // casts to an enormous `size_t`. That is undefined, so it is defined here
    // the way ERR-terminal-10 directs: encode once and use the length
    // actually produced. In a single-byte or UTF-8 locale the measured length
    // was exact anyway, so the stored bytes are identical to the C's.
    //
    // The visible character is appended by the same loop that walks the
    // sequence, because the C encodes it with the same call.
    //
    // No NUL is appended — the C's `b[n] = '\0'` exists because the string is
    // handed to `fputs`; here the length is the length.
    let mut b = Vec::new();
    for c in buf[..end].iter().copied().chain([visible]) {
        // The C's step-4 `el_malloc` failure, which returns 0 with `*wp`
        // already holding a non-negative width — the signal `re_putliteral`
        // reads as "abandon this literal, not the prompt".
        if !encode_onto(&mut b, c) {
            return 0;
        }
    }

    let l = &mut el.el_literal;

    // Sentinel capacity, defined where the C wraps. See `LITERAL_INDEX_MAX`.
    // Reaching this needs 2^31 successful allocations inside one refresh
    // cycle, so it is unreachable in practice; failing is still better than
    // handing back a sentinel that aliases `MB_FILL_CHAR`. A 0 return with
    // `*wp >= 0` is the C's allocation-failure signal, and `re_putliteral`
    // already abandons the literal on it.
    if l.l_idx > LITERAL_INDEX_MAX as usize {
        return 0;
    }

    // Step 6. Growth is a fixed +4 elements per reallocation (4, 8, 12,
    // 16, …), linear rather than doubling, so N literals cost about N/4
    // reallocations; `try_reserve_exact` is what keeps that cadence, where a
    // bare `push` would double. The C's uninitialised new slots have no
    // counterpart — spare capacity in a `Vec` holds nothing.
    //
    // The C restores `l_len` when the realloc fails, so the commit happens
    // after the reservation and not before. ERR-terminal-19 — that failure
    // path calling libc `free` on `b` instead of `el_free` — has no
    // counterpart: `b` is dropped by its own allocator.
    if l.l_idx == l.l_len {
        let extra = (l.l_len + 4).saturating_sub(l.l_buf.len());
        if l.l_buf.try_reserve_exact(extra).is_err() {
            return 0;
        }
        l.l_len += 4;
    }

    // Step 7. `l_buf[l_idx++] = b`, then return the index just filled with
    // the marker bit set. Sentinels carry an index and never a pointer, so
    // growing the table does not invalidate one already handed out; only
    // `literal_clear` does.
    //
    // The push cannot allocate: `l_idx` is now below `l_len`, and the block
    // above reserved through `l_len` without ever shrinking it.
    l.l_buf.push(b);
    l.l_idx += 1;
    EL_LITERAL | (l.l_idx - 1) as u32
}

// [spec:libedit:def:literal.literal-get-fn]
// [spec:libedit:sem:literal.literal-get-fn]
/// `idx` still carries the `EL_LITERAL` bit, which the C asserts on and then
/// masks off. The result borrows `el.el_literal.l_buf`, as the C's
/// `const char *` does.
pub(crate) fn literal_get(el: &mut crate::el::EditLine, idx: u32) -> &[u8] {
    let l = &el.el_literal;

    // Step 1. The C is `assert(idx & EL_LITERAL)`, a bitwise test, so
    // `literal_add`'s error return of 0 trips it. The sole caller,
    // `terminal_putc`, has already established the bit and has already
    // excluded `MB_FILL_CHAR` — that exclusion must stay ahead of the
    // `EL_LITERAL` test at the call site, because `MB_FILL_CHAR` is
    // `0xFFFF_FFFF` and has bit 31 set too. Any other value with bit 31 set
    // is a literal reference with no further validation; genuine scalar
    // values top out at U+10FFFF, so nothing real collides.
    //
    // The assert vanishes under `NDEBUG`, which distro packagers commonly
    // define, so this is defined rather than asserted: no bit, no bytes.
    if idx & EL_LITERAL == 0 {
        return &[];
    }

    // Step 2. Clear bit 31; bits 0..30 are the table index.
    let idx = (idx & !EL_LITERAL) as usize;

    // Step 3. The C is `assert(l_idx > (size_t)idx)` — the index must fall
    // inside the in-use prefix, not merely inside the allocated capacity.
    // Also compiled out under `NDEBUG`, and reachable in ordinary operation
    // rather than only through caller error: `re_refresh` calls
    // `literal_clear` and re-renders the prompt into `el_vdisplay`, but
    // `el_display` still holds the previous frame's sentinels and
    // `terminal_move_to_char` re-emits them straight through here
    // (ERR-terminal-09). Those old sentinels are correct only because a
    // prompt that renders identically re-issues the same indices in the same
    // order. A prompt that varies between refreshes — a clock, a git branch,
    // a changing colour — maps index *i* to different bytes; a prompt that
    // produced fewer literals this frame puts *i* past `l_idx`, where the C
    // reads out of bounds (or dereferences a NULL `l_buf`) and hands the
    // result to `fputs`.
    //
    // Undefined, so defined, and the signature settles which way: there is no
    // `Option` to return, and an empty slice is exactly the errata's "emit
    // nothing" fallback — the cell prints no bytes and the frame is otherwise
    // undisturbed. Observable behaviour for a stable prompt is untouched,
    // which is the part [dec:libedit:conformance-policy] freezes.
    //
    // This guard is not discriminating and cannot be: the C's two hazards —
    // an index between `l_idx` and `l_len`, reading an uninitialised `char *`,
    // and an index past the allocation entirely — have one answer here,
    // because a `Vec`'s spare capacity holds nothing and the lookup below
    // already misses. Deleting it would pass every test. It stays because the
    // rule names the in-use prefix as the bound, and because it is what keeps
    // `l_idx` authoritative when a caller has driven it out of step with
    // `l_buf`.
    if idx >= l.l_idx {
        return &[];
    }

    // Step 4. A borrowed, NUL-free byte string in the locale's multibyte
    // encoding, owned by the table and valid until the next `literal_clear`.
    // It holds both halves of the original input — the invisible sequence and
    // the encoding of the visible character that followed it — so printing it
    // advances the real cursor by that character's width, which is what the
    // display buffer already recorded when it laid down the sentinel cell
    // followed by `MB_FILL_CHAR` padding.
    //
    // It can legitimately be empty: if every character involved was
    // unrepresentable in the locale then `ct_encode_char` produced nothing
    // for all of them (ERR-encoding-15), and the sentinel prints nothing
    // while the visible character is still charged its columns.
    l.l_buf.get(idx).map_or(&[], Vec::as_slice)
}

/// Appends the locale's multibyte encoding of one character to `out`, and
/// answers false if that could not be allocated.
///
/// The C measures with `ct_enc_width` and then writes with `ct_encode_char`;
/// this only writes, which is ERR-terminal-10's defined resolution. See the
/// call site in [`literal_add`].
fn encode_onto(out: &mut Vec<u8>, c: u32) -> bool {
    // A fixed `MB_LEN_MAX` scratch. The C's `ct_encode_string` hard-codes
    // five bytes and `abort()`s past them (ERR-encoding-12); `literal_add`
    // instead passes its own measured remainder, which is the overflow
    // hazard. Neither applies to a buffer that is always large enough.
    let mut scratch = [0u8; MB_LEN_MAX];
    let n = ct_encode_char(&mut scratch, c);

    // `ct_encode_char` returns bytes written, 0 when `c` has no
    // representation in this locale, and -1 when the destination is too
    // short. Zero is ordinary: ERR-encoding-15 — the character is silently
    // dropped from the output while the display buffer is still charged its
    // columns, and the C reaches the same result because `ct_enc_width`
    // reports 0 for it as well. The -1 cannot happen with an `MB_LEN_MAX`
    // buffer; the C would *subtract* it from its running offset and drive
    // `b + n` before the start of the allocation, the second half of
    // ERR-terminal-10. Defined here as contributing nothing.
    if n <= 0 {
        return true;
    }

    // `try_reserve` and not a bare `extend_from_slice`: the C's `el_malloc`
    // for this string can fail and `literal_add` has a return value for it, so
    // growing under the global allocator — which aborts the process instead of
    // reporting — would throw that signal away.
    let n = (n as usize).min(scratch.len());
    if out.try_reserve(n).is_err() {
        return false;
    }
    out.extend_from_slice(&scratch[..n]);
    true
}

// `wcwidth`, `MB_LEN_MAX` and the two interval tables that used to live here
// are `crate::locale`'s. `literal_add` calls `wcwidth` directly in the C — not
// `chartype::ct_visual_width`, which is a different function with a different
// contract (it answers per `ct_chr_class` and returns 7 or 8 for a
// non-printable) — and `refresh.c` calls it directly too, ahead of
// `ct_visual_width`, to decide whether a double-width character must be pushed
// to the next line. One implementation, and it is locale-aware as libc's is:
// in the C locale nothing above U+007E has a width at all.

#[cfg(test)]
mod test {
    use super::*;
    use crate::el::{EditLine, blank_editline};

    /// `literal_add` takes the sequence as `buf[..end]` and the VISIBLE
    /// character as `buf[end]`, so a call needs one more element than `end`.
    fn add(el: &mut EditLine, seq: &[u32], visible: u32) -> (u32, i32) {
        let mut buf: Vec<u32> = seq.to_vec();
        buf.push(0); // the slot `end` indexes
        buf.push(visible);
        let mut w = 0;
        let r = literal_add(el, &buf, seq.len(), &mut w);
        (r, w)
    }

    /// Every success carries bit 31 and an index below it, which is what lets
    /// the screen image hold sentinels and real characters in one `u32`.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn a_sentinel_is_the_marker_bit_or_an_index() {
        let mut el = blank_editline();
        let (a, wa) = add(
            &mut el,
            &[0x1b, b'[' as u32, b'1' as u32, b'm' as u32],
            b'X' as u32,
        );
        let (b, wb) = add(
            &mut el,
            &[0x1b, b'[' as u32, b'0' as u32, b'm' as u32],
            b'Y' as u32,
        );

        assert_eq!(wa, 1, "the visible character's width is reported");
        assert_eq!(wb, 1);
        assert_eq!(a, EL_LITERAL, "first index is 0");
        assert_eq!(b, EL_LITERAL | 1);
        assert!(a & EL_LITERAL != 0 && b & EL_LITERAL != 0);
        // No genuine scalar value collides: Unicode stops at U+10FFFF.
        assert!(a > 0x10_FFFF && b > 0x10_FFFF);
    }

    /// The bytes come back exactly, sequence then visible character, and the
    /// C's trailing NUL is not stored — the length is the length.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    // [spec:libedit:sem:literal.literal-get-fn/test]
    #[test]
    fn the_stored_bytes_are_the_sequence_then_the_visible_character() {
        let mut el = blank_editline();
        let (r, _) = add(
            &mut el,
            &[0x1b, b'[' as u32, b'1' as u32, b'm' as u32],
            b'X' as u32,
        );
        assert_eq!(literal_get(&mut el, r), b"\x1b[1mX");
    }

    /// `literal_get` is defined where the C asserts, because the assert
    /// vanishes under `NDEBUG` — which distro packagers commonly define.
    // [spec:libedit:sem:literal.literal-get-fn/test]
    #[test]
    fn a_reference_with_no_marker_bit_yields_nothing() {
        let mut el = blank_editline();
        let (r, _) = add(&mut el, &[0x1b], b'X' as u32);
        assert_eq!(literal_get(&mut el, r & !EL_LITERAL), b"");
        assert_eq!(literal_get(&mut el, 0), b"", "add's failure return");
        assert_eq!(literal_get(&mut el, b'a' as u32), b"");
    }

    /// Past the in-use prefix, not merely past the allocation. Reachable in
    /// ordinary operation rather than only by caller error: ERR-terminal-09
    /// has `el_display` still holding the previous frame's sentinels after
    /// `literal_clear`.
    // [spec:libedit:sem:literal.literal-get-fn/test]
    #[test]
    fn a_stale_sentinel_yields_nothing_rather_than_old_bytes() {
        let mut el = blank_editline();
        let (r, _) = add(&mut el, &[0x1b], b'X' as u32);
        assert_eq!(literal_get(&mut el, r), b"\x1bX");

        literal_clear(&mut el);
        assert_eq!(
            literal_get(&mut el, r),
            b"",
            "the index survived the clear; the bytes must not"
        );
        assert_eq!(literal_get(&mut el, EL_LITERAL | 99), b"");
    }

    /// `end + 1` must be in bounds: the visible character is read from
    /// `buf[end]`, and there is no character to measure without it.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn a_missing_visible_character_fails_with_width_minus_one() {
        let mut el = blank_editline();
        let buf = [0x1b, b'[' as u32];
        let mut w = 0;
        assert_eq!(literal_add(&mut el, &buf, buf.len(), &mut w), 0);
        assert_eq!(w, -1);

        // Empty buffer, same shape.
        let mut w = 0;
        assert_eq!(literal_add(&mut el, &[], 0, &mut w), 0);
        assert_eq!(w, -1);
    }

    /// A visible character with no width is not a literal: `wcwidth` says -1
    /// and nothing is stored.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn a_visible_character_with_no_width_is_refused() {
        let mut el = blank_editline();
        // A lone surrogate has no width in any charset.
        let (r, w) = add(&mut el, &[0x1b], 0xD800);
        assert_eq!(r, 0);
        assert!(w < 0, "width was {w}");
        assert_eq!(el.el_literal.l_idx, 0, "nothing was stored");
    }

    /// The table grows by a fixed four slots, not by doubling. Observable
    /// only through `l_len`, which the `sem` rules index by.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn the_table_grows_by_four() {
        let mut el = blank_editline();
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (0, 0));
        for i in 1..=9usize {
            add(&mut el, &[0x1b], b'X' as u32);
            assert_eq!(el.el_literal.l_idx, i);
            assert_eq!(
                el.el_literal.l_len,
                i.div_ceil(4) * 4,
                "after {i} adds the allocation should be the next multiple of four"
            );
        }
    }

    /// `literal_clear` is a no-op on an untouched table — the C's guard tests
    /// `l_len`, not `l_idx` — and resets both counters otherwise.
    #[test]
    fn clear_is_a_no_op_when_nothing_was_allocated() {
        let mut el = blank_editline();
        literal_clear(&mut el);
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (0, 0));

        add(&mut el, &[0x1b], b'X' as u32);
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (1, 4));
        literal_end(&mut el);
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (0, 0));
        assert!(el.el_literal.l_buf.is_empty());
    }

    /// The index can never reach `MB_FILL_CHAR`.
    ///
    /// `terminal_putc` tests `c == MB_FILL_CHAR` BEFORE `c & EL_LITERAL`,
    /// and `MB_FILL_CHAR` is `0xFFFF_FFFF` — which has bit 31 set. A literal
    /// parked at index `0x7FFF_FFFF` would be swallowed as multibyte padding
    /// and never printed, so `literal_add` refuses that index rather than
    /// issuing a sentinel the caller cannot distinguish.
    ///
    /// The refusal is the C's allocation-failure shape and not its
    /// non-printable shape: `*wp` still carries the visible character's real
    /// width, so `re_putliteral` reads a 0 with `w >= 0` and abandons the
    /// literal rather than the whole prompt.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn the_unusable_top_index_is_never_issued() {
        assert_eq!(EL_LITERAL | LITERAL_INDEX_MAX, MB_FILL_CHAR - 1);
        assert_ne!(EL_LITERAL | LITERAL_INDEX_MAX, MB_FILL_CHAR);

        let mut el = blank_editline();
        // Past the bound the add fails rather than wrapping into the marker.
        el.el_literal.l_idx = LITERAL_INDEX_MAX as usize + 1;
        let (r, w) = add(&mut el, &[0x1b], b'X' as u32);
        assert_eq!(r, 0);
        assert_eq!(w, 1, "the width is reported even on the refusal");
    }

    /// `buf == end` — an empty sequence — is legal and not special-cased: it
    /// still allocates, still takes a table slot and still yields a sentinel.
    /// The stored string is just the visible character, which is why a
    /// sentinel always advances the real cursor even when it carries no
    /// escape at all.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn an_empty_sequence_still_consumes_a_slot() {
        let mut el = blank_editline();
        let (r, w) = add(&mut el, &[], b'Z' as u32);
        assert_eq!(r, EL_LITERAL, "index 0, the first slot");
        assert_eq!(w, 1);
        assert_eq!(literal_get(&mut el, r), b"Z");
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (1, 4));
    }

    /// The character at `end` is the closing delimiter and is never read: the
    /// sequence is `buf[..end]` and the visible character is `buf[end + 1]`,
    /// so the delimiter falls in the gap between them.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn the_closing_delimiter_is_skipped_rather_than_stored() {
        let mut el = blank_editline();
        // `\x01` sits where `prompt_print` puts the closing delimiter.
        let buf = [0x1b, b'[' as u32, 0x01, b'X' as u32];
        let mut w = 0;
        let r = literal_add(&mut el, &buf, 2, &mut w);
        assert_eq!(literal_get(&mut el, r), b"\x1b[X");
        assert_eq!(w, 1);
    }

    /// A character the locale cannot encode contributes no bytes, so a
    /// sequence made only of such characters stores an empty string — and the
    /// sentinel is still issued, still occupies a slot, and still charges the
    /// visible character its columns. ERR-encoding-15, reproduced: the byte
    /// count silently under-counts and no caller can tell.
    ///
    /// A lone surrogate is unencodable in both charsets, so this reads the
    /// same whatever `LC_CTYPE` says.
    ///
    /// The stored length is whatever the encoder produced, which is
    /// ERR-terminal-10's resolution: the C measures with one encoder and
    /// writes with another, and here there is only the one pass, so a
    /// character that contributes no bytes contributes no length either.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    // [spec:libedit:sem:literal.literal-get-fn/test]
    #[test]
    fn a_sequence_the_locale_cannot_encode_stores_nothing_at_all() {
        let mut el = blank_editline();
        let (r, w) = add(&mut el, &[0xD800, 0xDFFF], b'X' as u32);
        assert_eq!(r, EL_LITERAL);
        assert_eq!(w, 1, "the visible character is still charged its column");
        assert_eq!(literal_get(&mut el, r), b"X");

        // And with an unencodable visible character too, the stored string is
        // empty — indistinguishable from the fallback `literal_get` returns
        // for a reference it refuses.
        let mut el = blank_editline();
        el.el_literal.l_buf.push(Vec::new());
        el.el_literal.l_idx = 1;
        el.el_literal.l_len = 4;
        assert_eq!(literal_get(&mut el, EL_LITERAL), b"");
    }

    /// `literal_clear` deallocates rather than resetting a cursor, so the next
    /// `literal_add` starts over from index 0 and a fresh four-slot table.
    /// That is what makes a stale sentinel from the previous frame resolve to
    /// a *different* entry when the prompt renders differently — the hazard
    /// ERR-terminal-09 names.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn indices_restart_from_zero_after_a_clear() {
        let mut el = blank_editline();
        let (first, _) = add(&mut el, &[0x1b, b'a' as u32], b'X' as u32);
        let (second, _) = add(&mut el, &[0x1b, b'b' as u32], b'Y' as u32);
        assert_eq!(literal_get(&mut el, first), b"\x1baX");
        assert_eq!(literal_get(&mut el, second), b"\x1bbY");

        literal_clear(&mut el);
        let (again, _) = add(&mut el, &[0x1b, b'c' as u32], b'Z' as u32);
        assert_eq!(again, first, "the same sentinel, a different entry");
        assert_eq!(literal_get(&mut el, again), b"\x1bcZ");
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (1, 4));
    }

    /// The bound is the in-use prefix, not the allocation. One add leaves
    /// three allocated slots that were never written — the C's indeterminate
    /// pointers — and a reference into them yields nothing rather than the
    /// C's read of an uninitialised `char *`.
    ///
    /// `MB_FILL_CHAR` lands here too. `terminal_putc` must exclude it before
    /// testing the marker bit, but if it ever stops doing so the index it
    /// decodes to is far past any real entry, so the failure is silence
    /// rather than an out-of-bounds read.
    // [spec:libedit:sem:literal.literal-get-fn/test]
    #[test]
    fn a_reference_inside_the_allocation_but_past_the_entries_yields_nothing() {
        let mut el = blank_editline();
        let (r, _) = add(&mut el, &[0x1b], b'X' as u32);
        assert_eq!((el.el_literal.l_idx, el.el_literal.l_len), (1, 4));
        assert_eq!(literal_get(&mut el, r), b"\x1bX");

        for idx in 1..4u32 {
            assert_eq!(literal_get(&mut el, EL_LITERAL | idx), b"", "index {idx}");
        }
        assert_eq!(literal_get(&mut el, MB_FILL_CHAR), b"");

        // `l_idx` ahead of the entries it counts is not a state the module can
        // reach on its own, and the C would read past the end of `l_buf` for
        // it. Defined here as the same empty slice.
        el.el_literal.l_idx = 4;
        assert_eq!(literal_get(&mut el, EL_LITERAL | 3), b"");
    }

    /// `*wp` is `wcwidth` of the visible character and nothing else, so it
    /// spans the whole range that function has: 0 for a combining mark and 2
    /// for a double-width character.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn the_reported_width_is_the_visible_characters_alone() {
        let _cs = locale::pin_charset(locale::Charset::Utf8);
        let mut el = blank_editline();
        let seq = [0x1b, b'[' as u32, b'm' as u32];
        // U+0301 COMBINING ACUTE ACCENT, U+4E00 CJK UNIFIED IDEOGRAPH-4E00.
        let (combining, wc) = add(&mut el, &seq, 0x0301);
        let (wide, ww) = add(&mut el, &seq, 0x4E00);

        assert_eq!(wc, 0, "a combining mark occupies no column");
        assert_eq!(ww, 2, "a CJK ideograph occupies two");
        assert_eq!(literal_get(&mut el, combining), "\x1b[m\u{0301}".as_bytes());
        assert_eq!(literal_get(&mut el, wide), "\x1b[m\u{4e00}".as_bytes());
    }

    /// The C locale calls every non-ASCII character unprintable, so a coloured
    /// prompt whose visible character is not ASCII gets no literal there at
    /// all and the escape is dropped with it. The escape survives when the
    /// visible character is ASCII, which is what keeps an ordinary coloured
    /// prompt working in the C locale.
    ///
    /// Pinned rather than read from the environment, so one run covers the
    /// branch that the ambient `LC_CTYPE` would otherwise hide.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    #[test]
    fn a_non_ascii_visible_character_gets_no_literal_in_the_c_locale() {
        let _cs = locale::pin_charset(locale::Charset::Ascii);
        let mut el = blank_editline();
        let seq = [0x1b, b'[' as u32, b'm' as u32];
        for visible in [0x0301, 0x4E00, 0xE9] {
            let (r, w) = add(&mut el, &seq, visible);
            assert_eq!(w, -1, "U+{visible:04X} has no width in ASCII");
            assert_eq!(r, 0, "so the literal is dropped");
        }
        assert_eq!(el.el_literal.l_idx, 0, "and nothing was stored");

        // The escape itself is unaffected: an ASCII visible character still
        // interns, and the sequence bytes still come back.
        let (r, w) = add(&mut el, &seq, b'X' as u32);
        assert_eq!((r, w), (EL_LITERAL, 1));
        assert_eq!(literal_get(&mut el, r), b"\x1b[mX");
    }

    /// The stored bytes are the locale's multibyte encoding, so the same
    /// literal is three bytes in UTF-8 and refused outright in the C locale.
    // [spec:libedit:sem:literal.literal-add-fn/test]
    // [spec:libedit:sem:literal.literal-get-fn/test]
    #[test]
    fn the_stored_encoding_follows_the_charset() {
        let mut el = blank_editline();
        {
            let _cs = locale::pin_charset(locale::Charset::Utf8);
            // U+00E9 is one byte of source and two of UTF-8.
            let (r, w) = add(&mut el, &[0xE9], b'X' as u32);
            assert_eq!(w, 1);
            assert_eq!(literal_get(&mut el, r), "\u{e9}X".as_bytes());
        }

        let _cs = locale::pin_charset(locale::Charset::Ascii);
        // ERR-encoding-15: unencodable here, so it contributes no bytes and
        // the sentinel carries the visible character alone.
        let (r, w) = add(&mut el, &[0xE9], b'X' as u32);
        assert_eq!(w, 1);
        assert_eq!(literal_get(&mut el, r), b"X");
    }
}
