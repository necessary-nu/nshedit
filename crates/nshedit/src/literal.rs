//! Ported from `src/literal.c`; rules live in
//! `docs/spec/port/src/literal.md`.

use crate::chartype::{MB_FILL_CHAR, ct_encode_char};
use crate::locale::{self, MB_LEN_MAX};

/// C: `EL_LITERAL` in `src/literal.h` — `(wint_t)0x80000000`, bit 31 alone.
///
/// A successful [`literal_add`] returns this bit ORed with the table index,
/// so every success is `>= 0x8000_0000` and a return of `0` is an unambiguous
/// failure. `pub(crate)` because the sentinel is not this module's private
/// business: `terminal__putc` has to test the bit, and `refresh`'s screen
/// image has to be able to carry a value that is not a Unicode scalar.
pub(crate) const EL_LITERAL: u32 = 0x8000_0000;

/// Highest table index a sentinel may carry: `0x7FFF_FFFE`.
///
/// Derived from `chartype`'s [`MB_FILL_CHAR`], which is `(wint_t)-1` and so
/// has bit 31 set: `terminal__putc` **must** keep testing
/// `c == MB_FILL_CHAR` *before* `c & EL_LITERAL` — see
/// `sem:literal.literal-add-fn`, "Distinguishing a sentinel from a real
/// character". That ordering is load-bearing, and [`literal_add`] holds up its
/// end of it by never issuing the one sentinel the earlier test would swallow.
///
/// The rule states the representable range as `0..=0x7FFF_FFFF`, but the top
/// value is not *usable*: `EL_LITERAL | 0x7FFF_FFFF` is `0xFFFF_FFFF`, which
/// is `MB_FILL_CHAR`, and `terminal__putc` tests that first. A literal parked
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
    /// C: `size_t l_idx` — max in use. Kept alongside `l_buf` because the
    /// `sem` rules index by it and because `literal_clear`'s guard tests
    /// `l_len`, not this.
    pub l_idx: usize,
    /// C: `size_t l_len` — max allocated. Grows by a fixed +4 slots per
    /// reallocation, not by doubling.
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
    // through `terminal_overwrite` → `terminal__putc` → `literal_get`. That
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
    // No NUL is appended — the C's `b[n] = '\0'` exists because the string is
    // handed to `fputs`; here the length is the length.
    let mut b = Vec::new();
    for &c in &buf[..end] {
        encode_onto(&mut b, c);
    }
    encode_onto(&mut b, visible);

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
    // reallocations. `l_len` is kept because the rules name it and because
    // `literal_clear`'s guard tests it; `reserve_exact` mirrors the C's
    // realloc so the allocation pattern matches. The C's uninitialised new
    // slots have no counterpart — spare capacity in a `Vec` holds nothing.
    //
    // The C's two allocation-failure returns (step 4's `el_malloc` and this
    // step's `el_realloc`) cannot be reproduced: Rust's global allocator
    // aborts rather than reporting failure, so `literal_add` never returns 0
    // for OOM. ERR-terminal-19 — that failure path calling libc `free`
    // instead of `el_free` — is moot for the same reason, and was already
    // dispositioned `fix` as unobservable.
    if l.l_idx == l.l_len {
        l.l_len += 4;
        let extra = l.l_len.saturating_sub(l.l_buf.len());
        l.l_buf.reserve_exact(extra);
    }

    // Step 7. `l_buf[l_idx++] = b`, then return the index just filled with
    // the marker bit set. Sentinels carry an index and never a pointer, so
    // growing the table does not invalidate one already handed out; only
    // `literal_clear` does.
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
    // `terminal__putc`, has already established the bit and has already
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
    match l.l_buf.get(idx) {
        Some(b) => b.as_slice(),
        None => &[],
    }
}

/// Appends the locale's multibyte encoding of one character to `out`.
///
/// The C measures with `ct_enc_width` and then writes with `ct_encode_char`;
/// this only writes, which is ERR-terminal-10's defined resolution. See the
/// call site in [`literal_add`].
fn encode_onto(out: &mut Vec<u8>, c: u32) {
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
    if n > 0 {
        let n = (n as usize).min(scratch.len());
        out.extend_from_slice(&scratch[..n]);
    }
}

// `wcwidth`, `MB_LEN_MAX` and the two interval tables that used to live here
// are `crate::locale`'s. `literal_add` calls `wcwidth` directly in the C — not
// `chartype::ct_visual_width`, which is a different function with a different
// contract (it answers per `ct_chr_class` and returns 7 or 8 for a
// non-printable) — and `refresh.c` calls it directly too, ahead of
// `ct_visual_width`, to decide whether a double-width character must be pushed
// to the next line. One implementation, and it is locale-aware as libc's is:
// in the C locale nothing above U+007E has a width at all.
