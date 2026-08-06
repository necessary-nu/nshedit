//! Ported from `src/search.c`; rules live in `docs/spec/port/src/search.md`.

use core::cell::Cell;
use core::ffi::c_char;

use crate::chared::{CHAR_FWD, NOP};
use crate::chared::{c__next_word, c_gets, ce__isword, cv_delfini};
use crate::common::{ed_end_of_file, ed_newline, ed_search_next_history, ed_search_prev_history};
use crate::el::EL_BUFSIZ;
use crate::el::{EditLine, ElActionT};
use crate::fcns::{
    ED_DELETE_PREV_CHAR, ED_DIGIT, ED_INSERT, ED_SEARCH_NEXT_HISTORY, ED_SEARCH_PREV_HISTORY,
    EM_DELETE_PREV_CHAR, EM_INC_SEARCH_NEXT, EM_INC_SEARCH_PREV,
};
use crate::hist::hist_get;
use crate::histedit::{CC_CURSOR, CC_ERROR, CC_NORM, CC_REFRESH};
use crate::map::ElMapCurrent;
use crate::map::MAP_VI;
use crate::read::{el_wgetc, el_wpush};
use crate::refresh::re_refresh;
use crate::terminal::terminal_beep;

// ---------------------------------------------------------------------------
// Constants the C takes from headers this crate has not grown yet.
//
// `CC_*` are `histedit.h`; `EL_BUFSIZ` and `ANCHOR` are `el.h`; `MAP_VI` is
// `map.h`; `NOP` and `CHAR_FWD` are `chared.h`. The `ED_*`/`EM_*` command codes
// are `fcns.h`, which `src/makelist` generates; they come from
// [`crate::fcns`] and are no longer restated here.
//
// The two history-search codes double as this file's `dir`/`newdir` values and
// as the sentinel `c_setpat` tests for, all of which are the C's `int`, so they
// are widened with `i32::from` at each comparison and narrowed with
// `as ElActionT` wherever one is stored into `el_state.lastcmd`. The C's
// `#define` is untyped and needs neither.
// ---------------------------------------------------------------------------

/// C: `search.c` — `LEN` is 2, because `ANCHOR` is unconditionally defined in
/// `el.h`, so `patbuf` always carries a two-character `".*"` prefix.
const LEN: usize = 2;

// [spec:libedit:def:search.el-search-t]
/// Incremental- and character-search state.
pub struct ElSearchT {
    /// C: `wchar_t *patbuf` — the pattern buffer, owned.
    pub patbuf: Vec<u32>,
    /// C: `size_t patlen` — length of the pattern currently in `patbuf`,
    /// which is not the allocation size.
    pub patlen: usize,
    /// Direction of the last search.
    pub patdir: i32,
    /// Character search direction.
    pub chadir: i32,
    /// C: `wchar_t chacha` — the character we are looking for.
    pub chacha: u32,
    /// C: `char chatflg` — 0 if `f`, 1 if `t`. A byte-sized flag in the C,
    /// kept as one.
    pub chatflg: u8,
}

// ---------------------------------------------------------------------------
// Small helpers standing in for the C's pointer arithmetic and libc calls.
// ---------------------------------------------------------------------------

/// The NUL-terminated string held in `s`, as the C's `wchar_t *` would be
/// read. Everything past the first `L'\0'` is storage, not text.
fn wcs(s: &[u32]) -> &[u32] {
    match s.iter().position(|&c| c == 0) {
        Some(n) => &s[..n],
        None => s,
    }
}

/// The C's `const wchar_t *` argument as a slice.
///
/// # Safety
/// `p` must be a valid, NUL-terminated wide string that stays alive and
/// unmutated for the lifetime of the result. Both call sites are C-ABI
/// entry points whose contract is exactly that.
unsafe fn wcs_from_ptr<'a>(p: *const u32) -> &'a [u32] {
    if p.is_null() {
        return &[];
    }
    let mut n = 0usize;
    // SAFETY: the caller guarantees a NUL terminator, so the walk stops
    // inside the object.
    unsafe {
        while *p.add(n) != 0 {
            n += 1;
        }
        core::slice::from_raw_parts(p, n)
    }
}

/// C: `chared.h` — `#define isglob(a) (strchr("*[]?", (a)) != NULL)`.
///
/// Two properties of that macro are reproduced deliberately rather than
/// tidied away, because `ce_inc_search`'s `^W` handler is its only caller and
/// both are reachable there:
///
/// - `strchr`'s second parameter is an `int` converted to `char`, so a wide
///   character is matched on its **low byte**: U+012A ends up compared as
///   `'*'`. This is the same truncation as ERR-modes-32, and non-ASCII does
///   reach `patbuf` (see [`ce_inc_search`]).
/// - `strchr(s, 0)` finds the terminator, so `isglob(L'\0')` is *true*.
fn isglob(a: u32) -> bool {
    let b = (a & 0xff) as u8;
    b == 0 || b == b'*' || b == b'[' || b == b']' || b == b'?'
}

/// Store into `patbuf` at `i`, dropping the write when it falls outside the
/// `EL_BUFSIZ` allocation.
///
/// The C has no such guard, and two of its paths run off the end of the
/// buffer: `ce_inc_search`'s trailing-anchor append can reach
/// `patbuf[EL_BUFSIZ]` with a 1020-character pattern, and `cv_search`'s
/// pattern-reuse path can reach `patbuf[EL_BUFSIZ + 2]` when the stored
/// pattern is already at the `c_setpat` clamp of `EL_BUFSIZ - 1`. Neither is
/// in `docs/errata.md`; both are undefined behaviour, so per
/// `plan/decisions/conformance-policy.md` they are *defined* here as "the
/// write past the buffer does not happen". In the anchor case the pattern
/// text still fills `patbuf` exactly, so only the terminator is lost and
/// [`wcs`] reads the whole buffer as the pattern — the same string the C
/// would have produced.
fn patbuf_put(el: &mut EditLine, i: usize, c: u32) {
    if i < el.el_search.patbuf.len() {
        el.el_search.patbuf[i] = c;
    }
}

/// `patbuf[patlen] = L'\0'`, bounded as in [`patbuf_put`].
fn patbuf_terminate(el: &mut EditLine) {
    let i = el.el_search.patlen;
    patbuf_put(el, i, 0);
}

/// `wcsncpy(&patbuf[at], src, n)`, padding rule included: copy up to `src`'s
/// own NUL, then fill the rest of the `n` slots with NUL rather than reading
/// on. Bounded as in [`patbuf_put`].
fn patbuf_wcsncpy(el: &mut EditLine, at: usize, src: &[u32], n: usize) {
    let mut i = 0;
    while i < n {
        let c = src.get(i).copied().unwrap_or(0);
        if c == 0 {
            break;
        }
        patbuf_put(el, at + i, c);
        i += 1;
    }
    while i < n {
        patbuf_put(el, at + i, 0);
        i += 1;
    }
}

/// `*el->el_line.lastchar++ = c`.
///
/// ERR-modes-08: the C never checks these appends against
/// `el->el_line.limit`. Bounding them at the line capacity is the defined
/// behaviour the errata asks for; on the paths where the entry check in
/// [`ce_inc_search`] already reserved room the bound never fires, so this is
/// observationally the C on every reachable input.
fn line_put(el: &mut EditLine, c: u32) {
    let i = el.el_line.lastchar;
    if i < el.el_line.limit {
        el.el_line.buffer[i] = c;
        el.el_line.lastchar = i + 1;
    }
}

/// `*el->el_line.lastchar = L'\0'` without advancing.
fn line_terminate(el: &mut EditLine) {
    let i = el.el_line.lastchar;
    if i < el.el_line.buffer.len() {
        el.el_line.buffer[i] = 0;
    }
}

/// The NUL-terminated line text starting at `cp`, the subject `el_match` is
/// handed by [`ce_search_line`].
fn line_at(el: &EditLine, cp: usize) -> &[u32] {
    if cp >= el.el_line.buffer.len() {
        return &[];
    }
    wcs(&el.el_line.buffer[cp..])
}

/// `EL_CURSOR(el)` — the cursor, plus one position when vi command mode is
/// current, so the character under the cursor is included.
///
/// `sem:search.c-setpat-fn` requires the result to be clamped to `lastchar`:
/// in vi command mode at end of line the C's macro yields `lastchar + 1` and
/// relies on the caller having written a terminator there, which makes the
/// effective pattern the line text either way.
fn el_cursor(el: &EditLine) -> usize {
    let vi = el.el_map.r#type == MAP_VI && el.el_map.current == ElMapCurrent::Alt;
    let c = el.el_line.cursor + usize::from(vi);
    c.min(el.el_line.lastchar)
}

// [spec:libedit:def:search.search-init-fn]
// [spec:libedit:sem:search.search-init-fn]
/// C: `libedit_private int search_init(EditLine *el)`
pub(crate) fn search_init(el: &mut EditLine) -> i32 {
    // Step 1. `el_calloc(EL_BUFSIZ, sizeof(wchar_t))`. Step 2's -1 return is
    // unreachable here: a failed Rust allocation aborts rather than yielding
    // NULL, so the C's "leave the rest of the fields untouched" path has no
    // counterpart. Callers still get the C's 0, and ERR-core-api-02 records
    // that `el_init_internal` discards it anyway.
    el.el_search.patbuf = vec![0u32; EL_BUFSIZ];
    el.el_search.patbuf[0] = 0;
    el.el_search.patlen = 0;
    el.el_search.patdir = -1;
    el.el_search.chacha = 0;
    el.el_search.chadir = CHAR_FWD;
    el.el_search.chatflg = 0;
    0
}

// [spec:libedit:def:search.search-end-fn]
// [spec:libedit:sem:search.search-end-fn]
/// C: `libedit_private void search_end(EditLine *el)`
pub(crate) fn search_end(el: &mut EditLine) {
    // `el_free(patbuf); patbuf = NULL`. Releasing the allocation and leaving
    // an empty buffer behind is the closest thing to the C's NULL, and it is
    // idempotent for the same reason the C is. Deliberately nothing else:
    // the rule is explicit that `patlen`, `patdir`, `chacha`, `chadir` and
    // `chatflg` keep whatever they held, so `patlen` can outlive the buffer.
    el.el_search.patbuf = Vec::new();
}

// [spec:libedit:def:search.regerror-fn]
// [spec:libedit:sem:search.regerror-fn]
/// C: `void regerror(const char *msg)`
///
/// Nothing is ported for this rule. The definition sits inside `#ifdef
/// REGEXP`, `src/sys.h` undefines `REGEXP` in favour of `REGEX`, and
/// `plan/decisions/posix-only-scope.md` puts the BSD `regexp` branch out of
/// scope — so this is never reached and has no caller. It is private, and
/// spelled out only so the rule has a home; the POSIX branch swallows a bad
/// pattern in `el_match` instead.
fn regerror(msg: *const c_char) {
    // The C body is empty and the parameter is marked `/*ARGSUSED*/`.
    let _ = msg;
}

// [spec:libedit:def:search.el-match-fn]
// [spec:libedit:sem:search.el-match-fn]
/// C: `libedit_private int el_match(const wchar_t *str, const wchar_t *pat)`
///
/// Both arguments are NUL-terminated wide strings the caller owns: `str` is a
/// history entry's `ev.str` and `pat` is `el_search.patbuf`, so neither is a
/// slice at the call sites.
pub(crate) fn el_match(str: *const u32, pat: *const u32) -> i32 {
    // SAFETY: the C contract for this function is two live, NUL-terminated
    // `const wchar_t *`; every caller (`c_hmatch`, `ce_search_line`,
    // `el_wparse`) passes a buffer it owns and does not mutate for the
    // duration of the call.
    let subject = unsafe { wcs_from_ptr(str) };
    let pattern = unsafe { wcs_from_ptr(pat) };
    i32::from(el_match_wcs(subject, pattern))
}

/// The body of [`el_match`], over slices.
///
/// Split out so the in-crate callers do not have to launder their buffers
/// through raw pointers; the pointer form above is kept because `el_match` is
/// reached from `parse.c`'s `el_wparse` with a `const wchar_t *` that is not
/// a slice at the call site.
///
/// # Divergence from the C, recorded per ERR-modes-69
///
/// The C encodes both operands to multibyte with `ct_encode_string` before
/// handing them to `regcomp`/`regexec`, which makes matching depend on
/// `LC_CTYPE` and **silently drops** every character the locale cannot
/// encode: in a `C`/`POSIX` locale a pattern of accented text is reduced to
/// the empty string, which then matches everything. This port matches
/// natively over `u32`, so no character is ever dropped. The errata's
/// disposition is explicit that this divergence is an improvement to be
/// recorded rather than emulated. It also disposes of ERR-modes-16 — there is
/// no encode step to fail, so no NULL can reach the compiler.
fn el_match_wcs(str: &[u32], pat: &[u32]) -> bool {
    // Step 1. The literal-substring fast path, tried FIRST and load-bearing:
    // it is why a pattern that cannot compile still matches its literal
    // occurrence, and why the empty pattern matches everything — which is
    // live, because `c_setpat` produces an empty pattern whenever the cursor
    // sits at column zero.
    if wcsstr(str, pat) {
        return true;
    }

    // Steps 2-4. The POSIX branch, the only one `sys.h` compiles.
    bre_match(pat, str)
}

/// `wcsstr(str, pat) != NULL` — an unanchored comparison of code units, with
/// the empty pattern matching at position zero.
fn wcsstr(str: &[u32], pat: &[u32]) -> bool {
    if pat.is_empty() {
        return true;
    }
    if pat.len() > str.len() {
        return false;
    }
    str.windows(pat.len()).any(|w| w == pat)
}

/// `regcomp(&re, pat, 0)` then `regexec(&re, str, 0, NULL, 0) == 0`, with a
/// pattern that fails to compile reported as "no match".
///
/// # A deliberate change of dialect
///
/// The C's `cflags` is `0`, so its dialect is POSIX **basic** regular
/// expressions, where `+`, `?`, `|`, `(`, `)`, `{` and `}` are ordinary
/// literals, grouping is `\(`…`\)`, repetition is `\{m,n\}`, and `\1`…`\9`
/// are back-references. This uses the `regex` crate's dialect instead, which
/// reads almost every one of those the other way round and has no
/// back-references at all. That is a decision, taken on the record, not an
/// approximation that slipped in: POSIX BRE is a dialect nobody wants to type
/// and every other tool has moved away from.
///
/// # Why the blast radius is small
///
/// [`el_match_wcs`] tries `wcsstr` FIRST and returns on a hit, so every
/// pattern that occurs literally in the subject is answered before this
/// function is reached. What changes meaning is only a pattern that contains
/// a metacharacter *and* does not occur literally — and of `el_match`'s four
/// call sites, three do not carry patterns at all:
///
/// - `el_wparse`'s `prog:` qualifier is a program name, `emacs` or `Xmodmap`;
/// - `c_hmatch`'s pattern is `c_setpat`'s copy of the current line up to the
///   cursor, which is a command the user typed, not a pattern they wrote.
///   Regex there is a hazard rather than a feature: `ls *.c` in the buffer
///   makes `*` an operator in a search the user never asked to be one.
///
/// The one site where somebody deliberately writes a pattern is vi's `/` and
/// `?` through `cv_search`, and that is where the dialect is worth having.
///
/// # What does not change
///
/// A pattern the engine rejects is "no match", which is what the C does with
/// a `regcomp` that failed — so a `.editrc` or a vi search carrying stray
/// BRE punctuation degrades to no-match rather than to a panic or a wrong
/// hit. And `nmatch` is 0 with `pmatch` NULL in the C, so only the boolean is
/// ever used: leftmost-longest against leftmost-first decides which match is
/// reported, never whether one exists.
///
/// Matching stays over code points rather than bytes, per the divergence
/// [`el_match_wcs`] records. A `u32` that is not a scalar value — a lone
/// surrogate, or the `EL_LITERAL` sentinel — cannot appear in a Rust `str`,
/// so a subject or pattern containing one is reported as no match rather than
/// being silently rewritten.
fn bre_match(pat: &[u32], str: &[u32]) -> bool {
    let (Some(pattern), Some(subject)) = (scalars(pat), scalars(str)) else {
        return false;
    };
    regex::Regex::new(&pattern).is_ok_and(|re| re.is_match(&subject))
}

/// A wide string as a Rust `String`, or `None` if it holds a value `char`
/// forbids.
///
/// `None` rather than a replacement character: substituting U+FFFD would make
/// two different sentinels compare equal, and this port stores sentinels in
/// the same `u32` array as text (`docs/spec/port/src/literal.md`).
fn scalars(w: &[u32]) -> Option<String> {
    w.iter().copied().map(char::from_u32).collect()
}

// [spec:libedit:def:search.c-hmatch-fn]
// [spec:libedit:sem:search.c-hmatch-fn]
/// C: `libedit_private int c_hmatch(EditLine *el, const wchar_t *str)`
pub(crate) fn c_hmatch(el: &mut EditLine, str: *const u32) -> i32 {
    // The `SDEBUG` trace is the only other use of `el` and is compiled out of
    // every shipped build; it is not ported.
    //
    // Argument order is subject-first, pattern-second: `str` is the
    // candidate, `patbuf` is the pattern. Reversing it inverts the whole
    // history search.
    //
    // SAFETY: `str` is the caller's NUL-terminated wide string — a history
    // entry's `ev.str`, or `el_history.buf` for the uncommitted line.
    let subject = unsafe { wcs_from_ptr(str) };
    let pattern = wcs(&el.el_search.patbuf);
    i32::from(el_match_wcs(subject, pattern))
}

// [spec:libedit:def:search.c-setpat-fn]
// [spec:libedit:sem:search.c-setpat-fn]
/// C: `libedit_private void c_setpat(EditLine *el)`
pub(crate) fn c_setpat(el: &mut EditLine) {
    // Step 1. The whole guard: a previous history search keeps its pattern.
    // `ce_inc_search`, `cv_search` and `cv_repeat_srch` all fake `lastcmd`
    // precisely to reach it.
    let lastcmd = el.el_state.lastcmd as i32;
    if lastcmd == i32::from(ED_SEARCH_PREV_HISTORY) || lastcmd == i32::from(ED_SEARCH_NEXT_HISTORY)
    {
        return;
    }

    // Steps 2-4. ERR-modes-17: the C's `size_t` subtraction has no guard for
    // a cursor below `buffer`. Offsets from zero cannot wrap, so the defect
    // is defined out of existence rather than reproduced.
    let mut patlen = el_cursor(el);
    if patlen >= EL_BUFSIZ {
        patlen = EL_BUFSIZ - 1;
    }

    // Step 5. `wcsncpy` semantics, padding included: copy up to the source's
    // own NUL, then fill the remainder of `patlen` with NUL.
    let mut i = 0;
    while i < patlen {
        let c = el.el_line.buffer.get(i).copied().unwrap_or(0);
        if c == 0 {
            break;
        }
        patbuf_put(el, i, c);
        i += 1;
    }
    while i < patlen {
        patbuf_put(el, i, 0);
        i += 1;
    }

    el.el_search.patlen = patlen;
    // Step 6.
    patbuf_terminate(el);

    // Step 7's `SDEBUG` dump is compiled out and is not ported.
    //
    // `patdir` is deliberately untouched; only `cv_search` writes it.
    //
    // ERR-modes-69: the pattern is the raw typed prefix, unanchored and
    // unescaped, so `M-p`/`M-n` and vi `K`/`J` are a substring-or-BRE test
    // rather than the prefix test the C's own comment claims.
}

// ---------------------------------------------------------------------------
// `ce_inc_search`'s `pchar`.
// ---------------------------------------------------------------------------

thread_local! {
    /// C: `static wchar_t pchar = L':'` inside `ce_inc_search` — the prompt
    /// punctuation, `':'` for a search that succeeded and `'?'` for one that
    /// failed.
    ///
    /// ERR-modes-34 records that the C's `pchar` is a *function-level static*
    /// shared by every recursion level and every `EditLine` in the process.
    /// The rule suggests threading it through the invocation chain instead,
    /// and calls that observationally identical for a single editor — but it
    /// is not quite: step 9 skips its restore on `CC_REFRESH` and `CC_EOF`,
    /// so a search terminated by `ESC` while failing leaves `pchar == '?'`
    /// behind, and the *next* search's outermost level then reads
    /// `oldpchar == '?'` and stops absorbing a `^G` at step 8d. A fresh
    /// per-call value would lose that. Thread-local state keeps the
    /// cross-invocation persistence and the cross-instance aliasing the C
    /// has, and drops only the cross-thread race, which the C does not
    /// define anyway.
    ///
    /// The signature cannot take it as a parameter and `ElSearchT` is not
    /// this module's to extend, so this is where it lives.
    static PCHAR: Cell<u32> = const { Cell::new(':' as u32) };
}

fn pchar_get() -> u32 {
    PCHAR.with(|c| c.get())
}

fn pchar_set(v: u32) {
    PCHAR.with(|c| c.set(v));
}

/// Step 7 — strip the search prompt back off the live line.
///
/// Walks down from `lastchar` clearing until the `'\n'` that opened the
/// prompt, then clears that too, leaving `lastchar` exactly where the
/// iteration found it. Nothing the search machinery appends can contain a
/// `'\n'`, so the scan cannot stop early.
fn strip_prompt(el: &mut EditLine) {
    while el.el_line.lastchar > 0 && el.el_line.buffer[el.el_line.lastchar] != '\n' as u32 {
        el.el_line.buffer[el.el_line.lastchar] = 0;
        el.el_line.lastchar -= 1;
    }
    el.el_line.buffer[el.el_line.lastchar] = 0;
}

// [spec:libedit:def:search.ce-inc-search-fn]
// [spec:libedit:sem:search.ce-inc-search-fn]
/// C: `libedit_private el_action_t ce_inc_search(EditLine *el, int dir)`
///
/// The emacs `^R`/`^S` read-and-match loop. It **calls itself** once per
/// keystroke and the recursion is the undo stack: each level's saved locals
/// are the state one backspace rolls back to, and step 8d is what lets the
/// first `^G` un-fail a failed search instead of aborting it.
pub(crate) fn ce_inc_search(el: &mut EditLine, dir: i32) -> ElActionT {
    const STRFWD: [u32; 3] = ['f' as u32, 'w' as u32, 'd' as u32];
    const STRBCK: [u32; 3] = ['b' as u32, 'c' as u32, 'k' as u32];

    // Per-level saved state, captured before anything else.
    let ocursor = el.el_line.cursor;
    let oldpchar = pchar_get();
    let ohisteventno = el.el_history.eventno;
    let oldpatlen = el.el_search.patlen;
    let mut newdir = dir;
    let mut ret: ElActionT = CC_NORM;

    // Entry bound check. The 4 is `sizeof(L"fwd") / sizeof(wchar_t)`. Nothing
    // has been modified yet, so this failure is clean.
    if el.el_line.lastchar + 4 + 2 + el.el_search.patlen >= el.el_line.limit {
        return CC_ERROR;
    }

    loop {
        // 1. First round.
        if el.el_search.patlen == 0 {
            pchar_set(':' as u32);
            patbuf_put(el, 0, '.' as u32);
            patbuf_put(el, 1, '*' as u32);
            el.el_search.patlen = 2;
        }
        // 2.
        let mut done = 0;
        let mut redo = 0;

        // 3. Draw the prompt into the line buffer itself: the search UI is a
        //    second display line appended to the user's real line, and
        //    `lastchar` temporarily includes it.
        line_put(el, '\n' as u32);
        let word: &[u32] = if newdir == i32::from(ED_SEARCH_PREV_HISTORY) {
            &STRBCK
        } else {
            &STRFWD
        };
        for &c in word {
            line_put(el, c);
        }
        line_put(el, pchar_get());
        for i in LEN..el.el_search.patlen {
            let c = el.el_search.patbuf[i];
            line_put(el, c);
        }
        line_terminate(el);
        // 4.
        re_refresh(el);

        // 5.
        let mut ch: u32 = 0;
        if el_wgetc(el, &mut ch) != 1 {
            // ERR-modes-33 — fixed, as the rule directs: the C returns here
            // leaving the line holding the user's text plus
            // `"\nbck:<pattern>"`. Strip the prompt first. The cursor,
            // `patlen` and `eventno` are still left unrestored at this and
            // every outer level, because `CC_EOF` is abandoning the line
            // anyway and unwinding them would need the recursion to
            // cooperate.
            strip_prompt(el);
            return ed_end_of_file(el, 0);
        }

        // 6. Dispatch on `el->el_map.current[(unsigned char) ch]`.
        //
        // ERR-modes-32, decided here rather than inherited: the truncation to
        // the low byte is *reproduced*, deliberately. It is defined behaviour
        // in the C (a value-preserving conversion to `unsigned char`), not
        // undefined, so `plan/decisions/conformance-policy.md` says reproduce;
        // the errata only asks that the choice be made on purpose.
        //
        // Two notes on what it actually does, because the errata's summary is
        // narrower than the C. The claim "the default emacs map has
        // ED_UNASSIGNED for indices 128-255, so a non-ASCII keystroke
        // terminates the search" holds for the compiled-in `el_map_emacs`
        // table, but `map_init_emacs` then runs `map_init_nls`, which sets
        // `key[i] = ED_INSERT` for every `i` in 128..=255 with `iswprint(i)`.
        // So in a UTF-8 locale U+00E9 *does* extend the pattern — and so do
        // U+01E9, U+02E9 and every other character sharing that low byte,
        // each of which is appended to `patbuf` in full width. The truncation
        // is an aliasing bug, not simply a rejection. Only in a `C` locale,
        // where `iswprint` fails for 128-255, does it reduce to "non-ASCII
        // terminates the search".
        let idx = (ch & 0xff) as usize;
        let cmd = match el.el_map.current {
            ElMapCurrent::Key => el.el_map.key[idx],
            ElMapCurrent::Alt => el.el_map.alt[idx],
        };

        match cmd {
            ED_INSERT | ED_DIGIT => {
                if el.el_search.patlen >= EL_BUFSIZ - LEN {
                    terminal_beep(el);
                } else {
                    let n = el.el_search.patlen;
                    patbuf_put(el, n, ch);
                    el.el_search.patlen = n + 1;
                    line_put(el, ch);
                    line_terminate(el);
                    // Echo the keystroke before the search runs.
                    re_refresh(el);
                }
            }

            EM_INC_SEARCH_NEXT => {
                newdir = i32::from(ED_SEARCH_NEXT_HISTORY);
                redo += 1;
            }

            EM_INC_SEARCH_PREV => {
                newdir = i32::from(ED_SEARCH_PREV_HISTORY);
                redo += 1;
            }

            EM_DELETE_PREV_CHAR | ED_DELETE_PREV_CHAR => {
                // Deletes nothing directly: setting `done` returns `CC_NORM`
                // after the restore in step 9, dropping the caller back to
                // its own pattern, one character shorter. Backspace is
                // implemented purely by unwinding one recursion level.
                if el.el_search.patlen > LEN {
                    done += 1;
                } else {
                    terminal_beep(el);
                }
            }

            // Anything else dispatches a second time, on the raw wide `ch`.
            _ => match ch {
                0o7 => {
                    // ^G: abort.
                    ret = CC_ERROR;
                    done += 1;
                }

                0o27 => {
                    // ^W: append the rest of the current word to the pattern.
                    // Refused outright if the pattern holds a globbing
                    // character, because then the cursor is not reliably on a
                    // literal match.
                    let globbed =
                        (LEN..el.el_search.patlen).any(|i| isglob(el.el_search.patbuf[i]));
                    if globbed {
                        terminal_beep(el);
                    } else if el.el_line.cursor != 0 {
                        // ERR-modes-07: the C advances by
                        // `patlen - LEN - 1`, computed in `size_t`, which
                        // wraps to SIZE_MAX when `^W` is the very first
                        // keystroke. Defined here as the errata directs —
                        // "start at the cursor" — which is what the
                        // saturation gives. The clamp to `lastchar` bounds
                        // the C's unchecked displacement; note `lastchar` is
                        // the *inflated* one that includes the prompt step 3
                        // just appended, which is also what `c__next_word`
                        // is handed, and the `'\n'` test below is what stops
                        // the copy from running into that prompt.
                        let adv = el.el_search.patlen.saturating_sub(LEN + 1);
                        el.el_line.cursor = (el.el_line.cursor + adv).min(el.el_line.lastchar);
                        let end =
                            c__next_word(el, el.el_line.cursor, el.el_line.lastchar, 1, ce__isword);
                        while el.el_line.cursor < end
                            && el.el_line.buffer[el.el_line.cursor] != '\n' as u32
                        {
                            if el.el_search.patlen >= EL_BUFSIZ - LEN {
                                terminal_beep(el);
                                break;
                            }
                            // ERR-modes-08: the C bounds this append against
                            // `patbuf` only, never against
                            // `el->el_line.limit`, and the entry check left
                            // only about three spare slots past the prompt —
                            // so `^W` on a nearly full line writes past the
                            // allocation. Defined here as "stop and beep",
                            // matching the `patbuf`-full arm above.
                            if el.el_line.lastchar >= el.el_line.limit {
                                terminal_beep(el);
                                break;
                            }
                            let c = el.el_line.buffer[el.el_line.cursor];
                            let n = el.el_search.patlen;
                            patbuf_put(el, n, c);
                            el.el_search.patlen = n + 1;
                            line_put(el, c);
                            el.el_line.cursor += 1;
                        }
                        el.el_line.cursor = ocursor;
                        line_terminate(el);
                        re_refresh(el);
                    }
                    // `cursor == buffer` does nothing at all — no restore, no
                    // refresh — exactly as the C's `break` does.
                }

                0o33 => {
                    // ESC: terminate.
                    ret = CC_REFRESH;
                    done += 1;
                }

                _ => {
                    // Terminate and execute cmd. The C stores `ch` in the
                    // static `endcmd[2]` and falls through into the ESC case;
                    // `el_wpush` duplicates the string, so a local is
                    // equivalent and drops half of ERR-modes-34.
                    let endcmd = [ch, 0];
                    el_wpush(el, Some(wcs(&endcmd)));
                    ret = CC_REFRESH;
                    done += 1;
                }
            },
        }

        // 7.
        strip_prompt(el);

        // 8.
        if done == 0 {
            // a. Unmatched-`[` check: `chk` ends up `'['` exactly when the
            //    last bracket character in the pattern is an opening one.
            let mut chk = ']' as u32;
            let mut i = el.el_search.patlen;
            while i > LEN {
                i -= 1;
                let c = el.el_search.patbuf[i];
                if c == '[' as u32 || c == ']' as u32 {
                    chk = c;
                    break;
                }
            }

            // b. An unmatched `'['` skips the search for this keystroke,
            //    leaving display and history where they are so the user can
            //    finish typing the bracket expression.
            if el.el_search.patlen > LEN && chk != '[' as u32 {
                // i. Advance past the current match, or wrap — only on a
                //    repeat of the direction this level started with.
                //
                //    ERR-modes-09 (adjacent): the C's `cursor += -1` forms
                //    `buffer - 1` at column zero and then relies on its own
                //    `cursor < buffer` test in step iii to notice. Offsets
                //    are unsigned here, so that state is carried in `below`.
                let mut below = false;
                if redo != 0 && newdir == dir {
                    if pchar_get() == '?' as u32 {
                        // The previous search failed: wrap around.
                        el.el_history.eventno = if newdir == i32::from(ED_SEARCH_PREV_HISTORY) {
                            0
                        } else {
                            0x7fff_ffff
                        };
                        if hist_get(el) == CC_ERROR {
                            // The first call clamped `eventno` to the last
                            // real event as a side effect of failing; the
                            // second then loads it.
                            let _ = hist_get(el);
                        }
                        el.el_line.cursor = if newdir == i32::from(ED_SEARCH_PREV_HISTORY) {
                            el.el_line.lastchar
                        } else {
                            0
                        };
                    } else if newdir == i32::from(ED_SEARCH_PREV_HISTORY) {
                        if el.el_line.cursor == 0 {
                            below = true;
                        } else {
                            el.el_line.cursor -= 1;
                        }
                    } else {
                        el.el_line.cursor += 1;
                    }
                }

                // ii. Append the trailing anchor: the matcher sees
                //     `".*" + <typed> + ".*"`.
                let n = el.el_search.patlen;
                patbuf_put(el, n, '.' as u32);
                patbuf_put(el, n + 1, '*' as u32);
                el.el_search.patlen = n + 2;
                patbuf_terminate(el);

                // iii. Search the current line first, then history. The C's
                //      `||` short-circuits, so an out-of-range cursor skips
                //      `ce_search_line` entirely.
                let mut to_history = below || el.el_line.cursor > el.el_line.lastchar;
                if !to_history {
                    ret = ce_search_line(el, newdir);
                    to_history = ret == CC_ERROR;
                }
                if to_history {
                    // Stop `c_setpat` from overwriting the pattern.
                    el.el_state.lastcmd = newdir as ElActionT;
                    ret = if newdir == i32::from(ED_SEARCH_PREV_HISTORY) {
                        ed_search_prev_history(el, 0)
                    } else {
                        ed_search_next_history(el, 0)
                    };
                    if ret != CC_ERROR {
                        el.el_line.cursor = if newdir == i32::from(ED_SEARCH_PREV_HISTORY) {
                            el.el_line.lastchar
                        } else {
                            0
                        };
                        // Place the cursor on the match within the newly
                        // loaded history line; the result is discarded.
                        let _ = ce_search_line(el, newdir);
                    }
                }

                // iv. Strip the trailing anchor. The leading `".*"` stays.
                el.el_search.patlen -= LEN;
                patbuf_terminate(el);

                // v. Record success or failure.
                if ret == CC_ERROR {
                    terminal_beep(el);
                    if el.el_history.eventno != ohisteventno {
                        el.el_history.eventno = ohisteventno;
                        if hist_get(el) == CC_ERROR {
                            return CC_ERROR;
                        }
                    }
                    el.el_line.cursor = ocursor;
                    pchar_set('?' as u32);
                } else {
                    pchar_set(':' as u32);
                }
            }

            // c. Recurse: this is where the next keystroke is read. The
            //    direction passed down is `newdir`, so a `^R` after a `^S`
            //    counts as a flip at this level and a repeat at the next.
            ret = ce_inc_search(el, newdir);

            // d. Break abort of a failed search at the last non-failed level:
            //    a `^G` pressed while the search is failing propagates up
            //    through every level whose own search failed and is absorbed
            //    by the last one that was still succeeding, which then
            //    resumes its loop. A second `^G` aborts for real.
            if ret == CC_ERROR && pchar_get() == '?' as u32 && oldpchar == ':' as u32 {
                ret = CC_NORM;
            }
        }

        // 9. Restore on unwind. `CC_REFRESH` and `CC_EOF` deliberately skip
        //    this, so a terminated search keeps the history entry it landed
        //    on, the cursor position of the match, and the pattern.
        if ret == CC_NORM || (ret == CC_ERROR && oldpatlen == 0) {
            pchar_set(oldpchar);
            el.el_search.patlen = oldpatlen;
            if el.el_history.eventno != ohisteventno {
                el.el_history.eventno = ohisteventno;
                if hist_get(el) == CC_ERROR {
                    return CC_ERROR;
                }
            }
            el.el_line.cursor = ocursor;
            if ret == CC_ERROR {
                re_refresh(el);
            }
        }

        // 10.
        if done != 0 || ret != CC_NORM {
            return ret;
        }
    }
}

// [spec:libedit:def:search.cv-search-fn]
// [spec:libedit:sem:search.cv-search-fn]
/// C: `libedit_private el_action_t cv_search(EditLine *el, int dir)`
///
/// The vi `/` and `?` history search: one non-incremental round trip.
pub(crate) fn cv_search(el: &mut EditLine, dir: i32) -> ElActionT {
    let mut tmpbuf = [0u32; EL_BUFSIZ];

    // 1. Seed the `".*"` prefix. (The C's `tmplen = LEN` here is dead: step 3
    //    overwrites it with `c_gets`' return.)
    tmpbuf[0] = '.' as u32;
    tmpbuf[1] = '*' as u32;

    // 2. The only place `patdir` is ever assigned outside `search_init`.
    el.el_search.patdir = dir;

    // 3. Note the prompt mapping: a *backward* history search is prompted
    //    with `/`. `c_gets` renders it by overwriting `el_line.buffer`, so
    //    the line the user was editing is destroyed the moment `/` is
    //    pressed, whatever happens afterwards.
    let prompt: [u32; 3] = if dir == i32::from(ED_SEARCH_PREV_HISTORY) {
        ['\n' as u32, '/' as u32, 0]
    } else {
        ['\n' as u32, '?' as u32, 0]
    };
    let n = c_gets(el, &mut tmpbuf[LEN..], Some(wcs(&prompt)));
    if n == -1 {
        // 4. ERR-modes-37: `c_gets` returns -1 both for "backspaced past the
        //    start of an empty pattern" and for end of file, having already
        //    called `ed_end_of_file` and discarded the `CC_EOF` in the latter
        //    case. The rule asks the port to propagate the EOF — but the two
        //    are indistinguishable from a bare -1, and `c_gets`' signature
        //    belongs to `chared`. Reproduced as-is; fixing it needs
        //    `c_gets` to report which happened.
        return CC_REFRESH;
    }

    // 5. `c_gets` stores the terminating keystroke one past the text.
    let mut tmplen = n as usize + LEN;
    let ch = tmpbuf[tmplen];
    tmpbuf[tmplen] = 0;

    if tmplen == LEN {
        // 6. The user entered nothing, so reuse the previous pattern.
        if el.el_search.patlen == 0 {
            re_refresh(el);
            return CC_ERROR;
        }
        let first = el.el_search.patbuf.first().copied().unwrap_or(0);
        if first != '.' as u32 && first != '*' as u32 {
            // The stored pattern carries no `".*"` prefix, so it came from
            // `c_setpat` via vi's `K`/`J` or emacs' `M-p`/`M-n`. Wrap it.
            //
            // `wcsncpy(tmpbuf, patbuf, EL_BUFSIZ - 1)`.
            let old = wcs(&el.el_search.patbuf).to_vec();
            let cap = EL_BUFSIZ - 1;
            for (i, slot) in tmpbuf[..cap].iter_mut().enumerate() {
                *slot = old.get(i).copied().unwrap_or(0);
            }
            patbuf_put(el, 0, '.' as u32);
            patbuf_put(el, 1, '*' as u32);
            // `wcsncpy(&patbuf[2], tmpbuf, EL_BUFSIZ - 3)`.
            patbuf_wcsncpy(el, 2, &tmpbuf, EL_BUFSIZ - 3);
            // ERR-modes-36, reproduced: shifting the old text right by two
            // costs two positions but `patlen` gains only ONE, so the
            // trailing `'.'` lands on the last character of the old pattern.
            // Reusing `abc` yields `".*ab.*"`, not `".*abc.*"`. The errata's
            // disposition is `reproduce`; note the rule's prose says the port
            // "should add 2", which conflicts — the register and
            // `plan/decisions/conformance-policy.md` win for translation, and
            // idiomatization is where the +1 becomes a +2.
            el.el_search.patlen += 1;
            let mut p = el.el_search.patlen;
            patbuf_put(el, p, '.' as u32);
            p += 1;
            patbuf_put(el, p, '*' as u32);
            p += 1;
            el.el_search.patlen = p;
            patbuf_terminate(el);
        }
        // A pattern already starting `'.'` or `'*'` is assumed wrapped and is
        // reused untouched.
    } else {
        // 7. The user typed something. The stored pattern is
        //    `".*" + <typed> + ".*"`, read as a POSIX BRE.
        tmpbuf[tmplen] = '.' as u32;
        tmplen += 1;
        tmpbuf[tmplen] = '*' as u32;
        tmplen += 1;
        tmpbuf[tmplen] = 0;
        // `wcsncpy(patbuf, tmpbuf, EL_BUFSIZ - 1)`.
        patbuf_wcsncpy(el, 0, &tmpbuf, EL_BUFSIZ - 1);
        el.el_search.patlen = tmplen;
    }

    // 8. Avoid `c_setpat`.
    el.el_state.lastcmd = dir as ElActionT;

    // 9. ERR-modes-38, reproduced: emptying the line before the search is
    //    what makes the history search's prefix comparison vacuous, and also
    //    what leaves a failed `/` with an empty line and an empty stash at
    //    `eventno == 0`.
    el.el_line.cursor = 0;
    el.el_line.lastchar = 0;

    // 10.
    let found = if dir == i32::from(ED_SEARCH_PREV_HISTORY) {
        ed_search_prev_history(el, 0)
    } else {
        ed_search_next_history(el, 0)
    };
    if found == CC_ERROR {
        re_refresh(el);
        return CC_ERROR;
    }

    // 11. The inverse of the naive expectation: ESC accepts and submits the
    //     matched entry, CR/LF leaves it in the buffer for further editing.
    if ch == 0o33 {
        re_refresh(el);
        return ed_newline(el, 0);
    }
    CC_REFRESH
}

// [spec:libedit:def:search.ce-search-line-fn]
// [spec:libedit:sem:search.ce-search-line-fn]
/// C: `libedit_private el_action_t ce_search_line(EditLine *el, int dir)`
///
/// Finds the pattern inside the line currently in the edit buffer and moves
/// the cursor to the start of the match.
pub(crate) fn ce_search_line(el: &mut EditLine, dir: i32) -> ElActionT {
    // Step 2. ERR-modes-35 — fixed, as the rule directs. The C overwrites
    // `patbuf[1]` (the `'*'` of the `".*"` prefix that `ce_inc_search` wrote)
    // with `'^'` and matches against the string starting at `patbuf[1]`,
    // restoring the byte on every return path; for the duration of the call
    // the shared pattern buffer is corrupt. Building the anchored pattern as
    // a separate value is not observable to a correct caller.
    //
    // The precondition — `patbuf` begins with a two-character throwaway
    // prefix — is not checked in the C either; taking `patbuf[2..]` up to its
    // NUL reproduces what the C reads out of a `calloc`ed buffer even when
    // the pattern is shorter than the prefix.
    let mut anchored: Vec<u32> = Vec::with_capacity(el.el_search.patbuf.len());
    anchored.push('^' as u32);
    if el.el_search.patbuf.len() > LEN {
        anchored.extend_from_slice(wcs(&el.el_search.patbuf[LEN..]));
    }

    if dir == i32::from(ED_SEARCH_PREV_HISTORY) {
        // ERR-modes-09: the C's backward walk forms `buffer - 1` before the
        // guard rejects it. Defined here with a bounded index; the cursor
        // position itself is still tried first, and position zero last.
        let mut cp = el.el_line.cursor;
        loop {
            if el_match_wcs(line_at(el, cp), &anchored) {
                el.el_line.cursor = cp;
                return CC_NORM;
            }
            if cp == 0 {
                break;
            }
            cp -= 1;
        }
        CC_ERROR
    } else {
        // The forward loop's stop conditions in the C's own order: the NUL
        // test comes first, so it halts at the line terminator, and the
        // fallback bound is `limit`, not `lastchar`.
        let mut cp = el.el_line.cursor;
        while el.el_line.buffer.get(cp).copied().unwrap_or(0) != 0 && cp < el.el_line.limit {
            if el_match_wcs(line_at(el, cp), &anchored) {
                el.el_line.cursor = cp;
                return CC_NORM;
            }
            cp += 1;
        }
        CC_ERROR
    }
}

// [spec:libedit:def:search.cv-repeat-srch-fn]
// [spec:libedit:sem:search.cv-repeat-srch-fn]
/// C: `libedit_private el_action_t cv_repeat_srch(EditLine *el, wint_t c)`
pub(crate) fn cv_repeat_srch(el: &mut EditLine, c: u32) -> ElActionT {
    // Step 1's `SDEBUG` trace is compiled out and is not ported.

    // 2. The standard trick to stop the `c_setpat` inside the history search
    //    from replacing the pattern. `el_action_t` is an `unsigned char`, so
    //    the value is truncated to 8 bits; both command codes fit.
    el.el_state.lastcmd = c as ElActionT;

    // 3. Truncate the line to empty so the prefix comparison inside the
    //    history search is vacuous and the pattern alone decides.
    el.el_line.lastchar = 0;
    // ERR-modes-39 — fixed, as the rule directs. The C moves `lastchar` and
    // not `cursor`, so on the `CC_ERROR` paths below the cursor is left
    // pointing past `lastchar`; the success path only hides it because
    // `hist_get` reassigns both.
    el.el_line.cursor = 0;

    // 4. `patdir` is deliberately not updated here, so `N` searches the
    //    opposite way without making that the new default direction.
    if c == u32::from(ED_SEARCH_NEXT_HISTORY) {
        ed_search_next_history(el, 0)
    } else if c == u32::from(ED_SEARCH_PREV_HISTORY) {
        ed_search_prev_history(el, 0)
    } else {
        CC_ERROR
    }
}

// [spec:libedit:def:search.cv-csearch-fn]
// [spec:libedit:sem:search.cv-csearch-fn]
/// C: `libedit_private el_action_t cv_csearch(EditLine *el, int direction,
/// wint_t ch, int count, int tflag)`
///
/// The vi single-character line search behind `f`, `F`, `t`, `T`, `;` and
/// `,`. No regular expressions and no history: it moves the cursor within the
/// current line only.
pub(crate) fn cv_csearch(
    el: &mut EditLine,
    direction: i32,
    ch: u32,
    count: i32,
    tflag: i32,
) -> ElActionT {
    // 1. Nothing remembered yet: `chacha` starts as `L'\0'`, so `;` or `,`
    //    before any `f`/`F`/`t`/`T` fails here.
    if ch == 0 {
        return CC_ERROR;
    }

    // 2. `(wint_t)-1` is the in-band sentinel for "read the target character
    //    from the terminal now", used by `f`/`F`/`t`/`T`. `el_wgetc` only
    //    reports success for a real character, so it cannot collide with
    //    input in practice.
    let mut ch = ch;
    if ch == u32::MAX {
        let mut c: u32 = 0;
        if el_wgetc(el, &mut c) != 1 {
            return ed_end_of_file(el, 0);
        }
        ch = c;
    }

    // 3. ERR-modes-40, reproduced: the search is remembered for `;` and `,`
    //    *before* it runs, so these are updated even when it fails below and
    //    even when a pending vi operator is left dangling.
    el.el_search.chacha = ch;
    el.el_search.chadir = direction;
    el.el_search.chatflg = tflag as u8;

    // 4. `while (count--)`, so a `count` of 0 does nothing and leaves `cp` at
    //    the cursor. Signed throughout: ERR-modes-09/-10 note that the C's
    //    backward walk forms `buffer - 1`, and that its first dereference
    //    happens before any bound check. Both are defined here with a bounded
    //    index — the read only happens when the index is inside the buffer,
    //    and the walk's own guards then keep it in range.
    let last = el.el_line.lastchar as isize;
    let mut cp = el.el_line.cursor as isize;
    let mut count = count;
    while count != 0 {
        count = count.wrapping_sub(1);

        // Never re-find the character already under the cursor. ERR-modes-64:
        // this runs on every iteration and applies to `t`/`T` too, which is
        // why `t` followed by `;` does not move. Reproduced, not fixed.
        let at_cursor = usize::try_from(cp)
            .ok()
            .and_then(|i| el.el_line.buffer.get(i).copied())
            == Some(ch);
        if at_cursor {
            cp += direction as isize;
        }

        loop {
            // The bounds are checked before each dereference, and the
            // position at `lastchar` (the terminator) is excluded, so only
            // real line characters can match.
            if cp >= last {
                return CC_ERROR;
            }
            if cp < 0 {
                return CC_ERROR;
            }
            if el.el_line.buffer[cp as usize] == ch {
                break;
            }
            cp += direction as isize;
        }
    }

    // 5. Step back off the target for `t`/`T`.
    if tflag != 0 {
        cp -= direction as isize;
    }

    // 6. A forward match is always found strictly after the cursor, so the
    //    step-back above cannot reach -1; the clamp is a defined answer for
    //    a state the C would express as `buffer - 1`.
    el.el_line.cursor = cp.max(0) as usize;

    // 7. A vi operator such as `d` or `c` is pending and this was its motion.
    if el.el_chared.c_vcmd.action != NOP {
        if direction > 0 {
            // Make the motion inclusive of the target character.
            el.el_line.cursor += 1;
        }
        cv_delfini(el);
        return CC_REFRESH;
    }

    // 8.
    CC_CURSOR
}

#[cfg(test)]
mod match_test {
    use super::el_match_wcs;

    fn w(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    fn m(pat: &str, subj: &str) -> bool {
        el_match_wcs(&w(subj), &w(pat))
    }

    /// The substring fast path runs before any engine, which is what keeps
    /// the dialect change off the three call sites that carry no pattern —
    /// `el_wparse`'s `prog:` qualifier and `c_hmatch`'s copy of the line the
    /// user is typing.
    ///
    /// Every case here contains punctuation the `regex` crate would read as
    /// an operator, and each is answered literally because it occurs
    /// literally.
    #[test]
    fn a_literal_is_answered_before_the_engine_sees_it() {
        assert!(m("ls *.c", "ls *.c"));
        assert!(m("a+b", "x a+b y"));
        assert!(m("(ab)", "f(ab)g"));
        assert!(m("a{2}", "a{2}"));
        assert!(m("[unclosed", "an [unclosed thing"));
        assert!(m("a|b", "a|b"));
        // The empty pattern matches anything, which `c_setpat` relies on when
        // the cursor sits at column zero.
        assert!(m("", "anything"));
        assert!(m("", ""));
    }

    /// The vi `/` and `?` case, which is the one site where somebody writes a
    /// pattern deliberately. These are the `regex` crate's readings, and
    /// several are where it and POSIX BRE disagree.
    #[test]
    fn a_pattern_that_is_not_literal_uses_the_engine() {
        assert!(m("a.c", "abc"));
        assert!(m("ab*c", "abbbc"));
        assert!(m("^abc$", "abc"));
        assert!(!m("^abc$", "xabc"));
        assert!(m("[0-9]+", "port 8080"));
        assert!(m("foo|bar", "a bar b"));
        assert!(m("colou?r", "color"));
        assert!(m("(ab)+", "ababab"));
        assert!(m("a{2,3}", "aaa"));

        // The visible half of the decision. POSIX BRE reads `a+b` as three
        // literal characters, so it matches "a+b" and nothing else; this
        // dialect reads `+` as a repeat, so it matches "aab" and not "a+b".
        // The fast path cannot rescue either case, because neither subject
        // contains the pattern literally.
        assert!(m("a+b", "aab"), "this dialect reads + as a repeat");
        assert!(!m("a+b", "xyz"));
    }

    /// A pattern the engine rejects is "no match", as a failed `regcomp` is
    /// in the C — never a panic, and never a hit.
    #[test]
    fn an_uncompilable_pattern_does_not_match() {
        assert!(!m("(unclosed", "anything at all"));
        assert!(!m("a{", "b"));
        assert!(!m("[z-a]", "q"));
        // `\C` is not a valid escape, and this is the `.inputrc` line that
        // reaches `el_match` through `el_wparse`'s `prog:` qualifier — it
        // used to abort the process.
        assert!(!m("\\C-a", "conformance"));
    }

    /// Matching is over code points. A `u32` that is not a scalar value —
    /// a lone surrogate, or the `EL_LITERAL` sentinel — has no `char`, so it
    /// is no match rather than being folded into U+FFFD, which would make two
    /// different sentinels compare equal.
    #[test]
    fn a_non_scalar_value_is_no_match_rather_than_a_replacement() {
        assert!(el_match_wcs(&w("café"), &w("caf.")));
        assert!(
            el_match_wcs(&[0x61, 0xD800], &[0xD800]),
            "the literal path still works"
        );
        assert!(
            !el_match_wcs(&[0x61, 0xD800], &w("a.")),
            "but the engine gets nothing"
        );
    }
}
