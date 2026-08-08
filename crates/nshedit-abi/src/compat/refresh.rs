//! Ported from `src/refresh.c`; rules live in
//! `docs/spec/port/src/refresh.md`.

use crate::chartype::{
    CHTYPE_ASCIICTL, CHTYPE_NL, CHTYPE_NONPRINT, CHTYPE_PRINT, CHTYPE_TAB, MB_FILL_CHAR,
    VISUAL_WIDTH_MAX, ct_chr_class, ct_visual_char, ct_visual_width,
};
use crate::el::{CoordT, EditLine};
use crate::literal::{literal_add, literal_clear};
use crate::locale;
use crate::map::ElMapCurrent;
use crate::prompt::{PromptSide, prompt_print};
use crate::terminal::{
    terminal_clear_eol, terminal_deletechars, terminal_flush, terminal_insertwrite,
    terminal_move_to_char, terminal_move_to_line, terminal_overwrite, terminal_putc,
};

// The `el_terminal.t_flags` bits this module tests, via the C's `EL_CAN_*` /
// `EL_HAS_*` convenience macros. C: `src/terminal.h`. Private because
// `crate::terminal` has no counterpart yet; adopting one later is then a
// mechanical substitution.
/// C: `#define TERM_CAN_INSERT 0x001`.
const TERM_CAN_INSERT: i32 = 0x001;
/// C: `#define TERM_CAN_DELETE 0x002`.
const TERM_CAN_DELETE: i32 = 0x002;
/// C: `#define TERM_CAN_CEOL 0x004`.
const TERM_CAN_CEOL: i32 = 0x004;
/// C: `#define TERM_HAS_AUTO_MARGINS 0x080`.
const TERM_HAS_AUTO_MARGINS: i32 = 0x080;
/// C: `#define TERM_HAS_MAGIC_MARGINS 0x100`.
const TERM_HAS_MAGIC_MARGINS: i32 = 0x100;

/// C: `#define MIN_END_KEEP 4` — the minimum trailing or middle run
/// [`re_update_line`] thinks is worth preserving. The C describes it as about
/// half the cost of entering insert mode, inserting a character and leaving
/// again, notes it "should really be calculated from the termcap data", and
/// hardcodes 4 as a good value for ANSI terminals. It stays a constant here.
const MIN_END_KEEP: i32 = 4;

/// The space the screen images are padded and trimmed with.
const SPACE: u32 = b' ' as u32;

/// One cell of a screen row, answering NUL for anything past its end.
///
/// The C walks rows as `wchar_t *` and relies on every row carrying a
/// terminator inside its `t_size.h + 1` cells — `re_copy_and_pad` writes one
/// at `[t_size.h]`, and `re_putc`/`re_putliteral` write one there before every
/// wrap. Reading past the end is undefined in the C and is defined here as
/// reading the terminator that should have been there, so a row left
/// unterminated (`re_delete` step 4 can produce one, until `re_refresh` pads it
/// back out) truncates rather than walking off the end.
fn cell(row: &[u32], i: i32) -> u32 {
    if i < 0 {
        return 0;
    }
    row.get(i as usize).copied().unwrap_or(0)
}

/// The C's `wcslen` over a screen row, bounded by the row itself for the
/// reason [`cell`] gives. Safe over `MB_FILL_CHAR` cells because
/// `MB_FILL_CHAR` is `(wint_t)-1` and never zero.
fn wcslen(row: &[u32]) -> usize {
    row.iter().position(|&c| c == 0).unwrap_or(row.len())
}

/// Write one cell of the virtual image, or nothing at all when the drawing
/// cursor names a cell the image does not have.
///
/// [`re_putc`] and [`re_putliteral`] address `el_vdisplay[v][h]` from the same
/// cursor and the C indexes it unconditionally, so a cursor outside the image
/// is an out-of-bounds write there and a panic here. Every reachable state
/// keeps it inside — `re_nextline` clamps the row to `t_size.v` and both
/// writers wrap the column at `t_size.h` — so this defines what the C left
/// undefined rather than guarding something that happens, which is the
/// treatment [`cell`] already gives the reading side.
fn vput(el: &mut EditLine, v: i32, h: i32, c: u32) {
    let (Ok(v), Ok(h)) = (usize::try_from(v), usize::try_from(h)) else {
        return;
    };
    if let Some(cell) = el.el_vdisplay.get_mut(v).and_then(|row| row.get_mut(h)) {
        *cell = c;
    }
}

// [spec:libedit:def:refresh.el-refresh-t]
/// Where the refresh machinery believes the cursor is, and how tall the
/// display was last time round.
pub struct ElRefreshT {
    /// Refresh cursor position.
    pub r_cursor: CoordT,
    /// Vertical locations: rows used by the previous refresh.
    pub r_oldcv: i32,
    /// Rows used by this refresh.
    pub r_newcv: i32,
}

// [spec:libedit:def:refresh.re-printstr-fn]
// [spec:libedit:sem:refresh.re-printstr-fn]
/// The C's `f` and `t` delimit a half-open range of one screen row; the
/// range is the argument here, so the pair collapses to a single slice.
///
/// Debug-only in the C: this function, the twelve calls to it in
/// `re_update_line` and the `ELRE_DEBUG`/`ELRE_ASSERT` macros it depends on
/// all live behind `DEBUG_REFRESH`, which no shipped build defines. The rule
/// lets a port either implement it as tracing or omit it, because it has no
/// observable behaviour across the C ABI either way.
///
/// Implemented, and left with no call sites. Restoring the twelve would spray
/// a dump into `el_errfile` on every redraw of every frame, which is a shipped
/// build doing what only a `DEBUG_REFRESH` one may; leaving the formatter
/// written and tested means turning tracing on is adding a call rather than
/// reconstructing the format from the rule.
#[cfg(test)]
fn re_printstr(el: &EditLine, str: &str, f: &[u32]) {
    // C: `fprintf(__F, "%s:\"", str)`, then `"%c"` per character, then
    // `"\"\r\n"`. The `0177` mask folds every wide character into the ASCII
    // range, so the dump is legible for ASCII content and meaningless for
    // anything else: `MB_FILL_CHAR`, being `(wint_t)-1`, prints as `\177`, and
    // a NUL inside the range is written out like any other cell rather than
    // ending it.
    let mut out = Vec::with_capacity(str.len() + f.len() + 5);
    out.extend_from_slice(str.as_bytes());
    out.extend_from_slice(b":\"");
    out.extend(f.iter().map(|&c| (c & 0o177) as u8));
    out.extend_from_slice(b"\"\r\n");
    el.write_errfile(&out);
}

// [spec:libedit:def:refresh.re-nextline-fn]
// [spec:libedit:sem:refresh.re-nextline-fn]
fn re_nextline(el: &mut EditLine) {
    // Step 1.
    el.el_refresh.r_cursor.h = 0;

    // Step 2: no next row, so emulate a scroll by rotating the row pointers
    // instead of advancing. The C saves `el_vdisplay[0]`, shifts rows 1..lins
    // down by one and reinstalls the saved row as the last one; on a `Vec` of
    // row buffers that is exactly `rotate_left(1)` over the first `lins`
    // rows, and it moves the buffers rather than copying their contents, as
    // the C's comment asks ("we avoid memcpy()").
    if el.el_refresh.r_cursor.v + 1 >= el.el_terminal.t_size.v {
        // `terminal_change_size` clamps the row count to at least 1 and
        // `terminal_alloc_buffer` allocates exactly that many rows, so `lins`
        // never exceeds the image. The C indexes `el_vdisplay[0]`
        // unconditionally and would read a NULL row array if it did; clamping
        // defines that as "no rows, nothing to scroll".
        let lins = (el.el_terminal.t_size.v.max(0) as usize).min(el.el_vdisplay.len());
        if lins > 0 {
            el.el_vdisplay[..lins].rotate_left(1);
            // Only the first cell of the recycled row is cleared, so the rest
            // keeps stale content; drawing into it is sequential and always
            // re-terminates. With `lins == 1` this clears the row the cursor
            // is still on, which is what the C's `el_vdisplay[i - 1]` with
            // `i == 1` also does.
            el.el_vdisplay[lins - 1][0] = 0;
        }
        // `r_cursor.v` is deliberately left at `t_size.v - 1`.
        //
        // ERR-terminal-47, disposition `reproduce`: only `el_vdisplay` is
        // rotated. `el_display`, the image of what is physically on screen, is
        // not, so once the input is longer than the terminal `re_update_line`
        // diffs row *i* of the new virtual image against a stale row *i* of
        // the real one. Repairing it here would change what is emitted.
    } else {
        // Step 3.
        el.el_refresh.r_cursor.v += 1;
    }

    // The C's trailing `ELRE_ASSERT(... abort())` is compiled out of normal
    // builds and is not ported.
}

// [spec:libedit:def:refresh.re-addc-fn]
// [spec:libedit:sem:refresh.re-addc-fn]
fn re_addc(el: &mut EditLine, c: u32) {
    match ct_chr_class(c) {
        CHTYPE_TAB => {
            // Emit one space at a time and stop on the next 8-column tab
            // stop. The test is after the write, so at least one space always
            // lands. If a space wraps the row `re_putc` resets the column to
            // 0, which satisfies `h & 7 == 0` and ends the run immediately —
            // a tab crossing the right margin therefore fills only up to the
            // margin and does not continue on the new row.
            //
            // ERR-terminal-46, disposition `reproduce`: `re_refresh_cursor`
            // accounts for the same tab by advancing to the next multiple of
            // 8 first and subtracting the width afterwards. The two agree
            // only when the terminal width is a multiple of 8. Both sides are
            // left as they are, as the rule requires.
            loop {
                re_putc(el, SPACE, 1);
                if el.el_refresh.r_cursor.h & 0o7 == 0 {
                    break;
                }
            }
        }
        CHTYPE_NL => {
            let oldv = el.el_refresh.r_cursor.v;
            // Terminate the virtual row at the current column — not at column
            // 0 — without advancing the drawing cursor.
            re_putc(el, 0, 0);
            // ERR-terminal-65: the guard is always true, because the no-shift
            // form of `re_putc` returns before touching the cursor at all; the
            // C flags it `XXX`. Kept, because it is what the C evaluates and
            // it costs nothing.
            if oldv == el.el_refresh.r_cursor.v {
                re_nextline(el);
            }
        }
        CHTYPE_PRINT => re_putc(el, c, 1),
        _ => {
            // `CHTYPE_ASCIICTL` and `CHTYPE_NONPRINT`: `^X` form for ASCII
            // controls, `\U+nnnn` / `\U+nnnnn` for non-printables.
            let mut visbuf = [0u32; VISUAL_WIDTH_MAX];
            let n = ct_visual_char(&mut visbuf, c);
            // A -1 return ("would not fit") makes the loop body never execute
            // and the character is silently dropped. Because each visual
            // character gets its own `re_putc`, a long expansion that reaches
            // the right margin is split across the row break wherever the
            // margin falls; it is not kept whole.
            for &v in visbuf.iter().take(n.max(0) as usize) {
                re_putc(el, v, 1);
            }
        }
    }
}

// [spec:libedit:def:refresh.re-putliteral-fn]
// [spec:libedit:sem:refresh.re-putliteral-fn]
/// C: `begin` and `end` are two pointers into the prompt string the
/// application's callback returned — `begin` at the first character inside the
/// delimiters, `end` at the closing one, and `end[1]` the visible character
/// the sequence decorates and whose width decides whether the literal is kept
/// at all. Those two things are the arguments here.
///
/// They are not a slice and an index into it, because that pair is a trap:
/// [`crate::literal::literal_add`] encodes `buf[..end]` and `buf[end + 1]` and
/// never `buf[end]`, so a caller that put the closing delimiter one place
/// further along would compile, run, and intern the wrong bytes in silence.
/// A sequence and a character cannot be got wrong that way and need no
/// arithmetic at the call site.
pub(crate) fn re_putliteral(el: &mut EditLine, sequence: &[u32], visible: u32) {
    let sizeh = el.el_terminal.t_size.h;

    // Step 1. `literal_add` still takes the C's `(buf, end)` pair, so the pair
    // is rebuilt right here, where the sequence and the index into it cannot
    // drift apart. The pushed 0 occupies the slot `end` names — the closing
    // delimiter, which `literal_add` skips — and exists only to keep its
    // indexing scheme; the encoded output is the sequence followed by
    // `visible`, and `w` comes back as the number of columns `visible`
    // occupies.
    let mut buf = Vec::with_capacity(sequence.len() + 2);
    buf.extend_from_slice(sequence);
    buf.push(0);
    buf.push(visible);
    let mut w: i32 = 0;
    let c = literal_add(el, &buf, sequence.len(), &mut w);

    // Step 2: 0 is allocation failure, a negative width is a non-printable
    // visible character. Either way the sequence is dropped without changing
    // anything.
    if c == 0 || w < 0 {
        return;
    }

    // The C holds `cur` as a pointer to `el->el_refresh.r_cursor`, so every
    // read below sees the live value; `literal_add` does not touch it.
    let v = el.el_refresh.r_cursor.v;
    let h = el.el_refresh.r_cursor.h;

    // Step 3.
    vput(el, v, h, c);

    // Step 4: fill the cells the sequence's visible character covers, clamped
    // so the fill cannot leave the row. The C is `i = w; if (i > sizeh -
    // cur->h) i = sizeh - cur->h; while (--i > 0) ...`, which writes offsets
    // `i - 1` down to 1; the reversed range is that, and it is empty for the
    // zero- and single-column cases and for a negative clamp.
    let mut i = w;
    if i > sizeh - h {
        i = sizeh - h;
    }
    for k in (1..i).rev() {
        vput(el, v, h + k, MB_FILL_CHAR);
    }

    // Step 5: the same "zero width still costs a column" rule as `re_putc`
    // (ERR-terminal-45).
    el.el_refresh.r_cursor.h += if w != 0 { w } else { 1 };

    // Step 6. Unlike `re_putc` there is no pre-padding loop, so a wide
    // literal straddling the right margin is not pushed to the next row: its
    // magic cell is written where it falls, its fill cells are truncated at
    // the margin, and `r_cursor.h` may overshoot `sizeh` before `re_nextline`
    // resets it to 0. The overshoot is absorbed silently.
    if el.el_refresh.r_cursor.h >= sizeh {
        vput(el, v, sizeh, 0);
        re_nextline(el);
    }
}

// [spec:libedit:def:refresh.re-putc-fn]
// [spec:libedit:sem:refresh.re-putc-fn]
#[doc(hidden)]
pub fn re_putc(el: &mut EditLine, c: u32, shift: i32) {
    // Step 1. `wcwidth` is evaluated once, before any padding, exactly as the
    // C's initialiser is.
    let mut w = locale::wcwidth(locale::charset(), c);
    let sizeh = el.el_terminal.t_size.h;
    if w == -1 {
        w = 0;
    }

    // Step 2: pad the tail of the row with spaces so a double-width character
    // is not split across the right margin. Each space advances the column
    // and, at the margin, triggers step 6, which resets the column to 0 and
    // ends the loop. For `w <= 1` the condition is false on entry. The loop
    // would not terminate if `w` could exceed `sizeh`; `terminal_change_size`
    // forces at least 2 columns and `wcwidth` never returns more than 2.
    while shift != 0 && el.el_refresh.r_cursor.h + w > sizeh {
        re_putc(el, SPACE, 1);
    }

    let v = el.el_refresh.r_cursor.v;
    let h = el.el_refresh.r_cursor.h;

    // Step 3: the character, then `w - 1` fill cells — none when `w` is 0 or
    // 1. The C's comment notes the no-shift form "assumes !shift is only used
    // for single-column chars"; the one extra cell every row carries absorbs
    // the write when it is not.
    vput(el, v, h, c);
    for k in (1..w).rev() {
        vput(el, v, h + k, MB_FILL_CHAR);
    }

    // Step 4: `re_putc(el, '\0', 0)` is exactly "terminate the virtual row at
    // the current column"; that is how `re_refresh` and `re_addc` end a row.
    if shift == 0 {
        return;
    }

    // Step 5. ERR-terminal-45, disposition `reproduce`: a zero-width printable
    // — a combining mark, and everything `wcwidth` rejects — still occupies a
    // full column here, while `re_refresh_cursor` charges it none because
    // `ct_visual_width` returns `wcwidth`. The two disagree by one column per
    // such character and the cursor lands too far left. The rule is explicit
    // that a fix must change both sides together or neither, so both stand.
    el.el_refresh.r_cursor.h += if w != 0 { w } else { 1 };

    // Step 6: the one extra cell every row is allocated with.
    if el.el_refresh.r_cursor.h >= sizeh {
        vput(el, v, sizeh, 0);
        re_nextline(el);
    }
}

// [spec:libedit:def:refresh.re-refresh-fn]
// [spec:libedit:sem:refresh.re-refresh-fn]
pub fn re_refresh(el: &mut EditLine) {
    // Step 1: the magic characters about to be written index a freshly
    // numbered table.
    //
    // ERR-terminal-09, disposition `define`: `el_display` still holds the
    // *previous* frame's sentinels at this point, and `terminal_move_to_char`
    // re-emits them through `literal_get`, which is where the port defines
    // what happens (it answers an empty byte string for a stale index).
    literal_clear(el);

    // Step 2.
    el.el_refresh.r_cursor.h = 0;
    el.el_refresh.r_cursor.v = 0;
    terminal_move_to_char(el, 0);

    // Step 3: draw the rprompt into the virtual image purely so that
    // `prompt_print` leaves `el_rprompt.p_pos` holding its width and row
    // count; nothing is emitted to the terminal. Then zero the drawing cursor
    // again, so the real prompt overwrites those cells.
    //
    // ERR-terminal-60, disposition `reproduce`: this throwaway pass has real
    // side effects — it registers its literals in the table (indices the
    // drawing pass never uses), and an rprompt wider than the terminal
    // scrolls the virtual display through `re_nextline` before any real
    // content is drawn.
    prompt_print(el, PromptSide::Right);
    el.el_refresh.r_cursor.h = 0;
    el.el_refresh.r_cursor.v = 0;

    // Step 4: clamp the logical cursor. `el_map.current == el_map.alt` is vi
    // command mode; `lastchar != buffer` is a non-empty line.
    if el.el_line.cursor >= el.el_line.lastchar {
        if el.el_map.current == ElMapCurrent::Alt && el.el_line.lastchar != 0 {
            el.el_line.cursor = el.el_line.lastchar - 1;
        } else {
            el.el_line.cursor = el.el_line.lastchar;
        }
    }

    // Step 5: `h = -1` is the "not yet found" sentinel.
    let mut cur = CoordT { h: -1, v: 0 };

    // Step 6.
    prompt_print(el, PromptSide::Left);

    // Step 7: walk the input buffer. The C's `#if notyet` block, which would
    // have started this walk partway in for lines longer than the screen, is
    // not compiled and is not ported (ERR-terminal-65).
    let mut cp = 0usize;
    while cp < el.el_line.lastchar {
        let c = el.el_line.buffer[cp];
        if cp == el.el_line.cursor {
            let w = locale::wcwidth(locale::charset(), c);
            cur.h = el.el_refresh.r_cursor.h;
            cur.v = el.el_refresh.r_cursor.v;
            // Being at a line-broken double-width character: it will be
            // pushed to the next row, so the saved position follows it.
            if w > 1 && el.el_refresh.r_cursor.h + w > el.el_terminal.t_size.h {
                cur.h = 0;
                cur.v += 1;
            }
        }
        re_addc(el, c);
        cp += 1;
    }

    // Step 8: still unset means the cursor is at end of line.
    if cur.h == -1 {
        cur.h = el.el_refresh.r_cursor.h;
        cur.v = el.el_refresh.r_cursor.v;
    }

    // Step 9: decide whether the right prompt fits.
    let mut rhdiff = el.el_terminal.t_size.h - el.el_refresh.r_cursor.h - el.el_rprompt.p_pos.h;
    if el.el_rprompt.p_pos.h != 0
        && el.el_rprompt.p_pos.v == 0
        && el.el_refresh.r_cursor.v == 0
        && rhdiff > 1
    {
        // Pad out with `rhdiff - 1` spaces — the C's `while (--rhdiff > 0)` —
        // leaving at least one blank column between the input and the prompt
        // and stopping one column short of the right margin, so the row never
        // wraps.
        rhdiff -= 1;
        while rhdiff > 0 {
            re_putc(el, SPACE, 1);
            rhdiff -= 1;
        }
        // Note the side effect: this second `prompt_print` overwrites
        // `el_rprompt.p_pos` with the drawing cursor *after* the rprompt, so
        // from here until the next refresh `p_pos.h` is `t_size.h - 1` — the
        // end column — and no longer the rprompt's width. `re_fastaddc` reads
        // it in that state, which is what makes its guard vacuous
        // (ERR-terminal-49).
        prompt_print(el, PromptSide::Right);
    } else {
        // The flag "not using the right prompt".
        el.el_rprompt.p_pos.h = 0;
        el.el_rprompt.p_pos.v = 0;
    }

    // Step 10: terminate the virtual row at the current column, no shift.
    re_putc(el, 0, 0);

    // Step 11.
    el.el_refresh.r_newcv = el.el_refresh.r_cursor.v;

    // Step 12: drive the real image towards the virtual one, row by row. Both
    // arrays are mutated in place by `re_update_line`, which truncates each at
    // its last non-blank character; the pad afterwards restores a full-width
    // row, and is what makes `el_display` safe for `terminal_move_to_char` to
    // replay.
    let newcv = el.el_refresh.r_newcv;
    let width = el.el_terminal.t_size.h.max(0) as usize;
    for i in 0..=newcv {
        let row = i as usize;
        re_update_line(el, row, row, i);
        // Disjoint fields, so the two borrows coexist.
        re_copy_and_pad(&mut el.el_display[row], &el.el_vdisplay[row], width);
    }

    // Step 13: the previous image was taller, so erase the surplus rows. The
    // C reuses the loop counter, which is `r_newcv + 1` on exit.
    if el.el_refresh.r_oldcv > newcv {
        let oldcv = el.el_refresh.r_oldcv;
        for i in (newcv + 1)..=oldcv {
            // `r_oldcv` can exceed the last row: `re_fastputc`'s non-scroll
            // case pre-increments it independently of `el_cursor.v`
            // (ERR-terminal-48), and the C would then index past the row
            // array. Defined here as "no such row, nothing to erase".
            if i < 0 || i as usize >= el.el_display.len() {
                continue;
            }
            terminal_move_to_line(el, i);
            terminal_move_to_char(el, 0);
            let len = wcslen(&el.el_display[i as usize]) as i32;
            terminal_clear_eol(el, len);
            el.el_display[i as usize][0] = 0;
        }
    }

    // Step 14.
    el.el_refresh.r_oldcv = el.el_refresh.r_newcv;

    // Step 15: no flush is done here; callers flush.
    terminal_move_to_line(el, cur.v);
    terminal_move_to_char(el, cur.h);
}

// [spec:libedit:def:refresh.re-goto-bottom-fn]
// [spec:libedit:sem:refresh.re-goto-bottom-fn]
pub(crate) fn re_goto_bottom(el: &mut EditLine) {
    // Step 1: the last row the previous refresh used.
    terminal_move_to_line(el, el.el_refresh.r_oldcv);
    // Step 2: no `'\r'` — libedit relies on the tty's ONLCR translation to
    // turn this into a CR/LF, the same assumption `terminal_move_to_line`
    // makes when it moves downwards.
    terminal_putc(el, u32::from(b'\n'));
    // Step 3: from here on libedit treats the current physical line as the new
    // origin of the region it owns, so the recorded cursor becomes (0, 0)
    // rather than a row further down.
    re_clear_display(el);
    // Step 4.
    terminal_flush(el);
}

// [spec:libedit:def:refresh.re-insert-fn]
// [spec:libedit:sem:refresh.re-insert-fn]
/// `d` is the `el_display` row being edited and `s` a position within the
/// matching `el_vdisplay` row; both are borrowed out of the caller's
/// `EditLine`, so the C's `el` parameter — unused outside debug builds —
/// cannot be passed alongside them and is dropped.
fn re_insert(d: &mut [u32], dat: i32, dlen: i32, s: &[u32], num: i32) {
    // Step 1.
    if num <= 0 {
        return;
    }
    // Step 2: clamp. If `dat` exceeds `dlen` this makes `num` negative; every
    // remaining step is guarded on `num > 0`, so nothing happens in that case.
    let mut num = num;
    if num > dlen - dat {
        num = dlen - dat;
    }

    // Step 3: open the gap by copying right-to-left.
    //
    // ERR-terminal-08, disposition `define — express the loop over indices`:
    // the C's exit test is `a >= &d[dat]`, which forms a pointer one before
    // the start of the array when `dat` is 0. The signed indices below compute
    // the same sequence of moves with no pointer to form, which is what the
    // rule directs. The `num` cells that fall off the end are discarded.
    if num > 0 {
        let mut b = dlen - 1;
        let mut a = b - num;
        while a >= dat {
            d[b as usize] = d[a as usize];
            b -= 1;
            a -= 1;
        }
        d[dlen as usize] = 0;
    }

    // Step 4: copy the new content in. Unlike `re_strncopy` this does not
    // stop at a NUL in `s`, so exactly `num` cells are written subject to the
    // `d + dlen` bound. Every in-tree call passes `s` running to the end of an
    // `el_vdisplay` row, which by the lockstep-prefix invariant is always at
    // least `num` cells long.
    let mut a = dat;
    let mut si = 0usize;
    while a < dlen && num > 0 {
        d[a as usize] = s[si];
        a += 1;
        si += 1;
        num -= 1;
    }
}

// [spec:libedit:def:refresh.re-delete-fn]
// [spec:libedit:sem:refresh.re-delete-fn]
/// `d` and the dropped `el` parameter are as in [`re_insert`].
fn re_delete(d: &mut [u32], dat: i32, dlen: i32, num: i32) {
    // Step 1.
    if num <= 0 {
        return;
    }
    // Step 2: the deletion reaches or passes the end of the row, so everything
    // from `dat` onwards is simply truncated and no shifting is done.
    if dat + num >= dlen {
        d[dat as usize] = 0;
        return;
    }

    // Step 3. The C re-tests `num > 0` here, which step 1 already established;
    // the dead test is not ported.
    let mut b = dat;
    let mut a = dat + num;
    while a < dlen {
        d[b as usize] = d[a as usize];
        b += 1;
        a += 1;
    }

    // Step 4. The last `num` cells before `d[dlen]` keep stale copies; in
    // practice the string's own terminator is shifted down with the rest, so
    // the row still reads correctly. The exception is a row that was full to
    // `dlen`, which is left unterminated until `re_refresh` rebuilds it with
    // `re_copy_and_pad`.
    d[dlen as usize] = 0;
}

// [spec:libedit:def:refresh.re-strncopy-fn]
// [spec:libedit:sem:refresh.re-strncopy-fn]
/// `a` is a position in an `el_display` row, `b` the matching position in
/// an `el_vdisplay` row. Different fields of the same `EditLine`, so the
/// two borrows coexist.
fn re_strncopy(a: &mut [u32], b: &[u32], n: usize) {
    // The C's `while (n-- && *b) *a++ = *b++;` — at most `n` cells, stopping
    // at the first NUL in `b`, and unlike `strncpy` it neither pads the
    // destination nor writes a terminator. That is deliberate:
    // `re_update_line` uses it to mirror into `el_display` exactly the run it
    // has just written to the terminal, and the rest of the row must survive
    // untouched.
    //
    // The C bounds the copy only by `n`, trusting the caller; every in-tree
    // call stays inside both rows, so clamping to the two slices is a no-op
    // there and merely defines the out-of-range case rather than running off
    // the end of a row.
    let n = n.min(a.len()).min(b.len());
    for i in 0..n {
        if b[i] == 0 {
            break;
        }
        a[i] = b[i];
    }
}

// [spec:libedit:def:refresh.re-clear-eol-fn]
// [spec:libedit:sem:refresh.re-clear-eol-fn]
fn re_clear_eol(el: &mut EditLine, fx: i32, sx: i32, diff: i32) {
    // Steps 1 to 3: `max(diff, |fx|, |sx|)`, an upper bound on how much stale
    // text can remain to the right of what was just written.
    //
    // `wrapping_abs` rather than `abs` or unary minus: the C's `fx = -fx` on
    // `INT_MIN` is undefined, and both Rust spellings panic in a debug build.
    // The values here are bounded by the terminal width, so the wrap is
    // unreachable; it only defines what the C left undefined.
    let fx = fx.wrapping_abs();
    let sx = sx.wrapping_abs();
    let mut diff = diff;
    if fx > diff {
        diff = fx;
    }
    if sx > diff {
        diff = sx;
    }

    // Step 4. ERR-terminal-53, disposition `reproduce`: the maxima are not
    // clamped at zero and the incoming `diff` may be negative. Both call sites
    // are reached only when the corresponding `fx` or `sx` is strictly
    // negative, so the result is always positive in practice; a port must not
    // rely on that if it reuses the helper, since `terminal_clear_eol` with a
    // negative count writes nothing but still moves its recorded column
    // backwards.
    terminal_clear_eol(el, diff);
}

// [spec:libedit:def:refresh.re-update-line-fn]
// [spec:libedit:sem:refresh.re-update-line-fn]
/// C: `wchar_t *old, wchar_t *new` — row pointers, always `el_display[i]`
/// and `el_vdisplay[i]`. They are row indices here, since the function also
/// needs `el` for the terminal calls and so cannot hold the rows borrowed
/// across them. All three arguments carry the same value, exactly as in
/// the C.
fn re_update_line(el: &mut EditLine, old: usize, new: usize, i: i32) {
    let sizeh = el.el_terminal.t_size.h;

    // The twelve pointers are `i32` offsets from the start of their row, so
    // every `p - q` below is the C's `ptrdiff_t` with the C's sign. Making
    // them `usize` would underflow at the two places the C legitimately forms
    // a negative difference (`oe - ols` when the trailing-run scan never ran,
    // and `fx`/`sx` whenever the new region is shorter).
    //
    // `new` is copied out once. Nothing between here and the end of the
    // routine reads `el_vdisplay`, and the C never writes to `new` after the
    // truncation below, so a single copy serves every `terminal_overwrite`
    // argument while `el` is mutably borrowed. `old` is edited *in place*
    // instead, and must be: `terminal_move_to_char` moves rightwards by
    // replaying `el_display`'s own cells, so it has to see each mirrored edit
    // as it happens.
    let nrow: Vec<u32> = el.el_vdisplay[new].clone();
    let mut nrow = nrow;
    let olen = el.el_display[old].len() as i32;
    let nlen = nrow.len() as i32;

    // ---- Phase 1: find the boundaries. ----

    // Step 1: advance in lockstep while the old character is non-NUL and the
    // two agree. Testing only `old`'s terminator suffices, since `new`'s
    // cannot equal a non-NUL old character.
    let mut o = 0i32;
    let mut n = 0i32;
    while cell(&el.el_display[old], o) != 0 && cell(&el.el_display[old], o) == cell(&nrow, n) {
        o += 1;
        n += 1;
    }
    let ofd = o;
    let nfd = n;

    // Step 2: find the end of each row, back up over trailing spaces without
    // going below the first difference, and truncate in place. Trailing blanks
    // are dropped because the terminal is assumed already blank to the right
    // of the text; this in-place truncation is exactly why `re_refresh` must
    // pad `el_display` back out afterwards, and why `el_vdisplay` must be
    // rebuilt rather than reused.
    while cell(&el.el_display[old], o) != 0 {
        o += 1;
    }
    while ofd < o && cell(&el.el_display[old], o - 1) == SPACE {
        o -= 1;
    }
    let oe = o;
    // The C stores unconditionally, relying on the row carrying a terminator
    // within its cells; see `cell`. An index at the very end means the row had
    // neither, and writing there is out of bounds in the C — defined here as
    // leaving the (already full) row alone.
    if oe < olen {
        el.el_display[old][oe as usize] = 0;
    }

    while cell(&nrow, n) != 0 {
        n += 1;
    }
    while nfd < n && cell(&nrow, n - 1) == SPACE {
        n -= 1;
    }
    let ne = n;
    if ne < nlen {
        // Both the working copy and the caller's row: the truncation is
        // observable, because `re_refresh` then copies `el_vdisplay[i]` into
        // `el_display[i]` and stops at this NUL.
        nrow[ne as usize] = 0;
        el.el_vdisplay[new][ne as usize] = 0;
    }

    // Step 3: identical after trimming. Return having emitted nothing and, in
    // particular, without moving the cursor to row `i` — not moving is the
    // whole point, an unchanged row must cost nothing.
    if cell(&el.el_display[old], ofd) == 0 && cell(&nrow, nfd) == 0 {
        return;
    }

    // Step 4: the common trailing run. The increment is applied both on the
    // mismatch exit and on the "ran back to ofd/nfd" exit, so this is the
    // longest common suffix except that at least one character is always left
    // in each middle: `ols > ofd` and `nls > nfd` always hold.
    loop {
        if !(o > ofd && n > nfd) {
            break;
        }
        o -= 1;
        n -= 1;
        if cell(&el.el_display[old], o) != cell(&nrow, n) {
            break;
        }
    }
    let mut ols = o + 1;
    let mut nls = n + 1;

    // ---- Phase 2: find the common run inside the middle. ----

    // All four start as an empty run pinned at the far end.
    let mut osb = ols;
    let mut nsb = nls;
    let mut ose = ols;
    let mut nse = nls;

    // Case 1 — insert: scan from `nfd` to `nls` looking for `*ofd`. The old
    // side is anchored at the first difference, because an insertion does not
    // move old content.
    if cell(&el.el_display[old], ofd) != 0 {
        let c = cell(&el.el_display[old], ofd);
        let mut nn = nfd;
        while nn < nls {
            if c == cell(&nrow, nn) {
                let mut oo = ofd;
                let mut p = nn;
                while p < nls && oo < ols && cell(&el.el_display[old], oo) == cell(&nrow, p) {
                    oo += 1;
                    p += 1;
                }
                // Strictly longer than the best so far, and longer than half
                // the number of new characters that would have to be inserted
                // ahead of it.
                if (nse - nsb) < (p - nn) && 2 * (p - nn) > nn - nfd {
                    nsb = nn;
                    nse = p;
                    osb = ofd;
                    ose = oo;
                }
            }
            nn += 1;
        }
    }

    // Case 2 — delete: scan from `ofd` to `ols` looking for `*nfd`. It runs
    // after case 1 and compares against whatever case 1 left, so a deletion
    // interpretation must be strictly longer to displace an insertion one.
    if cell(&nrow, nfd) != 0 {
        let c = cell(&nrow, nfd);
        let mut oo = ofd;
        while oo < ols {
            if c == cell(&el.el_display[old], oo) {
                let mut nn = nfd;
                let mut p = oo;
                while p < ols && nn < nls && cell(&el.el_display[old], p) == cell(&nrow, nn) {
                    p += 1;
                    nn += 1;
                }
                if (ose - osb) < (p - oo) && 2 * (p - oo) > oo - ofd {
                    nsb = nfd;
                    nse = nn;
                    osb = oo;
                    ose = p;
                }
            }
            oo += 1;
        }
    }

    // ---- Phase 3: pragmatics. ----

    // Pragmatic I: preserving a two- or three-character tail is not worth the
    // cursor motion.
    if (oe - ols) < MIN_END_KEEP {
        ols = oe;
        nls = ne;
    }

    // Pragmatic II: degrade the plan to what the terminal can actually do.
    // `fx` is the net number of characters to insert (positive) or delete
    // (negative) at the first difference; `sx` is the same for the second.
    let mut fx = (nsb - nfd) - (osb - ofd);
    let mut sx = (nls - nse) - (ols - ose);

    if el.el_terminal.t_flags & TERM_CAN_INSERT == 0 {
        if fx > 0 {
            osb = ols;
            ose = ols;
            nsb = nls;
            nse = nls;
        }
        if sx > 0 {
            ols = oe;
            nls = ne;
        }
        if (ols - ofd) < (nls - nfd) {
            ols = oe;
            nls = ne;
        }
    }
    // `fx` and `sx` are *not* recomputed between the two blocks, so this one
    // tests the values that were current before the insert block ran. That
    // ordering is reproduced literally.
    if el.el_terminal.t_flags & TERM_CAN_DELETE == 0 {
        if fx < 0 {
            osb = ols;
            ose = ols;
            nsb = nls;
            nse = nls;
        }
        if sx < 0 {
            ols = oe;
            nls = ne;
        }
        if (ols - ofd) > (nls - nfd) {
            ols = oe;
            nls = ne;
        }
    }

    // Pragmatic III: if the middle run that survived is too short, collapse it
    // as well. (The C notes this test replaced an older `osb == ose`.)
    if (ose - osb) < MIN_END_KEEP {
        osb = ols;
        ose = ols;
        nsb = nls;
        nse = nls;
    }

    // From here on `fx` and `sx` are the plan.
    fx = (nsb - nfd) - (osb - ofd);
    sx = (nls - nse) - (ols - ose);

    // The C's twelve `re_printstr` region dumps and its four
    // `ELRE_DEBUG(!EL_CAN_INSERT, ...)` assertions are `DEBUG_REFRESH`-only
    // and are not ported; see `re_printstr`.

    // ---- Phase 4: emit. ----

    // This happens here and not earlier precisely so that the early return in
    // phase 1 step 3 leaves the cursor alone.
    terminal_move_to_line(el, i);

    // The last old character position that still matters.
    let p = if ols != oe { oe } else { ose };

    // 4a — early first-difference insert. Taken when the new side really does
    // have extra leading characters, there is a net insert, and inserting
    // `fx` columns will not push `p` off the right edge.
    if (nsb != nfd) && fx > 0 && (p + fx <= sizeh) {
        terminal_move_to_char(el, nfd);
        if nsb != ne {
            // There is content beyond the insertion worth keeping.
            //
            // The C re-tests `fx > 0` here, which the branch condition already
            // established; kept because the mirror branch in 4b keeps its own
            // (the C's comment: "we check for code symmetry").
            if fx > 0 {
                terminal_insertwrite(el, &nrow[nfd as usize..], fx);
                re_insert(
                    &mut el.el_display[old],
                    ofd,
                    sizeh,
                    &nrow[nfd as usize..],
                    fx,
                );
            }
            // `len` equals `osb - ofd` and so is never negative.
            let len = ((nsb - nfd) - fx) as usize;
            terminal_overwrite(el, &nrow[(nfd + fx) as usize..], len);
            re_strncopy(
                &mut el.el_display[old][(ofd + fx) as usize..],
                &nrow[(nfd + fx) as usize..],
                len,
            );
            // Fall through to 4c with `fx` still positive.
        } else {
            // Nothing beyond: the row is finished.
            let len = (nsb - nfd) as usize;
            terminal_overwrite(el, &nrow[nfd as usize..], len);
            re_strncopy(
                &mut el.el_display[old][ofd as usize..],
                &nrow[nfd as usize..],
                len,
            );
            return;
        }
    } else if fx < 0 {
        // 4b — first-difference delete.
        terminal_move_to_char(el, ofd);
        if osb != oe {
            // Old content past the deletion must be preserved. `fx` is less
            // than zero *always* here; the C tests it anyway for symmetry.
            if fx < 0 {
                terminal_deletechars(el, -fx);
                re_delete(&mut el.el_display[old], ofd, sizeh, -fx);
            }
            let len = (nsb - nfd) as usize;
            terminal_overwrite(el, &nrow[nfd as usize..], len);
            re_strncopy(
                &mut el.el_display[old][ofd as usize..],
                &nrow[nfd as usize..],
                len,
            );
            // Fall through to 4c with `fx` still negative.
        } else {
            let len = (nsb - nfd) as usize;
            terminal_overwrite(el, &nrow[nfd as usize..], len);
            re_clear_eol(el, fx, sx, oe - ne);
            return;
        }
    } else {
        // Neither applied. This is a flag as much as a value — it records "no
        // early first-difference edit was performed" — and it is
        // simultaneously the column shift 4c must add. An insert rejected by
        // 4a's width test lands here and is retried in 4d.
        fx = 0;
    }

    // 4c — second-difference delete.
    //
    // The column arithmetic is the subtlest part of the routine and is
    // deliberate. By the invariants `nse - new` equals `(ose - old) +
    // fx_true`, where `fx_true` is the real first-difference shift. When 4a or
    // 4b already performed that shift, `fx` still holds `fx_true` and this
    // addresses the row in its post-shift coordinates; when the shift was
    // deferred (4a rejected on width), `fx` is 0 and this addresses the row in
    // its *pre*-shift coordinates, and 4d's later insert pushes what 4c wrote
    // into its correct final column. Either way the content lands at
    // `nse - new`. Recomputing `fx` here would misplace it by `fx` columns.
    if sx < 0 && (ose + fx) < sizeh {
        terminal_move_to_char(el, ose + fx);
        if ols != oe {
            // The C re-tests `sx < 0`; a duplicate, as its comment says.
            if sx < 0 {
                terminal_deletechars(el, -sx);
            }
            terminal_overwrite(el, &nrow[nse as usize..], (nls - nse) as usize);
        } else {
            terminal_overwrite(el, &nrow[nse as usize..], (nls - nse) as usize);
            re_clear_eol(el, fx, sx, oe - ne);
        }
        // Neither branch mirrors anything into `old`; from here on the old
        // image is stale. That is safe only because `re_refresh` rewrites the
        // whole row with `re_copy_and_pad` immediately afterwards. It is
        // *not* safe to reorder these steps: `terminal_move_to_char` moves
        // rightwards by replaying characters out of `el_display`, so `old`
        // must still be accurate for every column to the left of the next
        // move — which is exactly what the mirroring in 4a and 4b protects.
    }
    // ERR-terminal-51, disposition `reproduce`: if `sx < 0` but the on-screen
    // test above fails, the second difference is never applied at all — 4e
    // requires `sx >= 0`, so neither branch runs — and yet `re_refresh` then
    // declares `el_display` equal to `el_vdisplay`. Screen and model diverge
    // silently until something forces a full redraw. The rule calls it a real
    // hole in the algorithm and still says to reproduce it.

    // 4d — late first-difference insert: a first-difference edit is still owed
    // and 4a/4b did not do it, either because `fx` was genuinely 0 (a pure
    // overwrite) or because 4a's width test rejected the insert. This runs
    // *after* 4c, so when both fire the second difference is edited before the
    // first: deletes at the end first, insert at the front second. That order
    // is what keeps the intermediate row from overflowing, and it is what
    // makes 4c's pre-shift addressing come out right.
    if (nsb != nfd) && (osb - ofd) <= (nsb - nfd) && fx == 0 {
        terminal_move_to_char(el, nfd);
        if nsb != ne {
            // Recompute `fx`, since it was zeroed above as a flag.
            fx = (nsb - nfd) - (osb - ofd);
            if fx > 0 {
                terminal_insertwrite(el, &nrow[nfd as usize..], fx);
                re_insert(
                    &mut el.el_display[old],
                    ofd,
                    sizeh,
                    &nrow[nfd as usize..],
                    fx,
                );
            }
            let len = ((nsb - nfd) - fx) as usize;
            terminal_overwrite(el, &nrow[(nfd + fx) as usize..], len);
            re_strncopy(
                &mut el.el_display[old][(ofd + fx) as usize..],
                &nrow[(nfd + fx) as usize..],
                len,
            );
        } else {
            let len = (nsb - nfd) as usize;
            terminal_overwrite(el, &nrow[nfd as usize..], len);
            re_strncopy(
                &mut el.el_display[old][ofd as usize..],
                &nrow[nfd as usize..],
                len,
            );
        }
    }

    // 4e — second-difference insert or overwrite, which includes `sx == 0`,
    // the plain-overwrite case. The line is now NEW up to `nse`.
    if sx >= 0 {
        terminal_move_to_char(el, nse);
        if ols != oe {
            if sx > 0 {
                terminal_insertwrite(el, &nrow[nse as usize..], sx);
            }
            // That length equals `ols - ose` and so is never negative.
            terminal_overwrite(
                el,
                &nrow[(nse + sx) as usize..],
                ((nls - nse) - sx) as usize,
            );
        } else {
            // No clear-to-end-of-line is needed, because with no trailing run
            // to preserve this write has covered everything the old row had.
            terminal_overwrite(el, &nrow[nse as usize..], (nls - nse) as usize);
        }
    }
}

// [spec:libedit:def:refresh.re-copy-and-pad-fn]
// [spec:libedit:sem:refresh.re-copy-and-pad-fn]
fn re_copy_and_pad(dst: &mut [u32], src: &[u32], width: usize) {
    // Step 1: copy until `width` cells or the source terminator, whichever
    // comes first. `MB_FILL_CHAR` cells are copied like any other; being
    // `(wint_t)-1` they are non-zero and never terminate the copy. The C
    // advances both pointers together, so source and destination share the
    // index. `re_fastputc` calls this with an empty source, which stops here
    // immediately and blanks the whole row.
    let mut i = 0usize;
    while i < width {
        let c = src.get(i).copied().unwrap_or(0);
        if c == 0 {
            break;
        }
        dst[i] = c;
        i += 1;
    }

    // Step 2: space-fill the rest. The padding is load-bearing:
    // `terminal_move_to_char` moves rightwards by replaying the display row's
    // own characters, so every cell it might replay has to hold a real space
    // rather than a NUL or a leftover from a previous line.
    while i < width {
        dst[i] = SPACE;
        i += 1;
    }

    // Step 3: exactly `width + 1` cells are written, which is exactly how
    // large a display row is allocated (`t_size.h + 1`), so a full-width row
    // still receives its terminator.
    dst[width] = 0;
}

// [spec:libedit:def:refresh.re-refresh-cursor-fn]
// [spec:libedit:sem:refresh.re-refresh-cursor-fn]
pub(crate) fn re_refresh_cursor(el: &mut EditLine) {
    // Step 1: the same clamp `re_refresh` does.
    if el.el_line.cursor >= el.el_line.lastchar {
        if el.el_map.current == ElMapCurrent::Alt && el.el_line.lastchar != 0 {
            el.el_line.cursor = el.el_line.lastchar - 1;
        } else {
            el.el_line.cursor = el.el_line.lastchar;
        }
    }

    // Step 2: start from just after the prompt. Those coordinates were
    // recorded by the last `re_refresh`, so this is only correct if a refresh
    // has already run.
    let mut h = el.el_prompt.p_pos.h;
    let mut v = el.el_prompt.p_pos.v;
    let th = el.el_terminal.t_size.h;

    // Step 3: walk to the cursor. The C's `w` is a function-scope local that
    // the rule notes is only ever read after being assigned in the same
    // expression, so the apparently uninitialised use in step 4 is not a
    // defect; two block-locals are exactly equivalent and say so structurally.
    let mut cp = 0usize;
    while cp < el.el_line.cursor {
        let c = el.el_line.buffer[cp];
        match ct_chr_class(c) {
            CHTYPE_NL => {
                // Handle a newline in the data part too.
                h = 0;
                v += 1;
            }
            CHTYPE_TAB => {
                // C: `while (++h & 07) continue;` — pre-increment to the next
                // 8-column tab stop.
                //
                // ERR-terminal-46, disposition `reproduce`: `re_addc` instead
                // emits spaces one at a time and stops the moment a wrap
                // resets the column to 0, ending at column 0 of the next row,
                // whereas this advances to the next multiple of 8 first and
                // applies the wrap subtraction below afterwards, ending at
                // `h_next_tabstop - th`. The two agree only when `th` is a
                // multiple of 8.
                h += 1;
                while h & 0o7 != 0 {
                    h += 1;
                }
            }
            _ => {
                let w = locale::wcwidth(locale::charset(), c);
                if w > 1 && h + w > th {
                    // Won't fit on the line.
                    h = 0;
                    v += 1;
                }
                // ERR-terminal-45, disposition `reproduce`: `ct_visual_width`
                // returns `wcwidth` for a printable, i.e. 0 for a combining
                // mark, while `re_putc` charged it a full column. The two
                // disagree by one column per zero-width character and the
                // cursor lands too far left. Both sides stay as they are.
                h += ct_visual_width(c);
            }
        }

        // The ordinary wrap, whichever branch ran. It subtracts `th` exactly
        // once, so a single expansion spanning more than one full row would be
        // mis-accounted; the C's comment notes this is also where over-long
        // tabs are meant to be caught.
        if h >= th {
            h -= th;
            v += 1;
        }
        cp += 1;
    }

    // Step 4: the character under the cursor will have been pushed to the next
    // row if it is double-width and does not fit, so the cursor follows it.
    if cp < el.el_line.lastchar {
        let w = locale::wcwidth(locale::charset(), el.el_line.buffer[cp]);
        if w > 1 && h + w > th {
            h = 0;
            v += 1;
        }
    }

    // Step 5.
    terminal_move_to_line(el, v);
    terminal_move_to_char(el, h);
    terminal_flush(el);
}

// [spec:libedit:def:refresh.re-fastputc-fn]
// [spec:libedit:sem:refresh.re-fastputc-fn]
fn re_fastputc(el: &mut EditLine, c: u32) {
    // Step 1.
    let w = locale::wcwidth(locale::charset(), c);

    // Step 2: pad out the tail of the row so a double-width character is not
    // split across the margin. Each space advances the column and, on reaching
    // the margin, performs the wrap of step 5, after which the test fails and
    // the loop ends. Only `w > 1` pads, so zero- and single-width characters
    // never do.
    while w > 1 && el.el_cursor.h + w > el.el_terminal.t_size.h {
        re_fastputc(el, SPACE);
    }

    // Step 3: `terminal_putc` writes nothing at all for `MB_FILL_CHAR` and
    // expands an `EL_LITERAL` magic character to its saved byte string.
    terminal_putc(el, c);

    // Step 4: a character of width `w` advances the column by `max(w, 1)` —
    // widths 0 and -1 still consume a whole cell and a whole column, the same
    // convention as `re_putc` (ERR-terminal-45).
    let v = el.el_cursor.v as usize;
    let h = el.el_cursor.h as usize;
    el.el_display[v][h] = c;
    el.el_cursor.h += 1;
    // C: `while (--w > 0)`, i.e. `w - 1` fill cells, none for `w <= 1`.
    for _ in 1..w {
        let v = el.el_cursor.v as usize;
        let h = el.el_cursor.h as usize;
        el.el_display[v][h] = MB_FILL_CHAR;
        el.el_cursor.h += 1;
    }

    // Step 5: wrap.
    if el.el_cursor.h >= el.el_terminal.t_size.h {
        el.el_cursor.h = 0;

        // Which row to blank.
        let lastline: Option<usize>;
        if el.el_cursor.v + 1 >= el.el_terminal.t_size.v {
            // (a) Already on the last row, so emulate a scroll by rotating the
            // `el_display` row pointers up by one; the recycled row 0 becomes
            // the new last row and is the one to blank. `el_cursor.v` is
            // deliberately left where it is and `r_oldcv` is not touched.
            //
            // ERR-terminal-48, disposition `reproduce`: only `el_display` is
            // rotated — `el_vdisplay` is never scrolled to match — so after a
            // scroll the two images no longer describe the same rows.
            let lins = (el.el_terminal.t_size.v.max(0) as usize).min(el.el_display.len());
            if lins > 0 {
                el.el_display[..lins].rotate_left(1);
                lastline = Some(lins - 1);
            } else {
                lastline = None;
            }
        } else {
            // (b) Note this indexes by `r_oldcv`, not by `el_cursor.v`; the
            // two coincide only when the counters agree (ERR-terminal-48).
            el.el_cursor.v += 1;
            el.el_refresh.r_oldcv += 1;
            let idx = el.el_refresh.r_oldcv;
            // The C indexes unconditionally, and `r_oldcv` is not bounded by
            // the same test `el_cursor.v` just passed, so it can run past the
            // last row. Defined here as "no such row, blank nothing".
            lastline = if idx >= 0 && (idx as usize) < el.el_display.len() {
                Some(idx as usize)
            } else {
                None
            };
        }

        if let Some(row) = lastline {
            let width = el.el_terminal.t_size.h.max(0) as usize;
            re_copy_and_pad(&mut el.el_display[row], &[], width);
        }

        // Force the physical wrap. With auto margins the terminal wraps by
        // itself; with magic margins too (terminfo `eat_newline_glitch`, where
        // the wrap is deferred until the next character arrives) a space and a
        // backspace make the pending wrap resolve now.
        if el.el_terminal.t_flags & TERM_HAS_AUTO_MARGINS != 0 {
            if el.el_terminal.t_flags & TERM_HAS_MAGIC_MARGINS != 0 {
                terminal_putc(el, SPACE);
                terminal_putc(el, u32::from(b'\x08'));
            }
        } else {
            terminal_putc(el, u32::from(b'\r'));
            terminal_putc(el, u32::from(b'\n'));
        }
    }
}

// [spec:libedit:def:refresh.re-fastaddc-fn]
// [spec:libedit:sem:refresh.re-fastaddc-fn]
pub(crate) fn re_fastaddc(el: &mut EditLine) {
    // Bail-out 1: there is no character before the cursor to have been added.
    if el.el_line.cursor == 0 {
        re_refresh(el);
        return;
    }
    let c = el.el_line.buffer[el.el_line.cursor - 1];

    // Bail-out 2: a tab's width depends on the column, and an insertion in the
    // middle of the line shifts everything after it; both are too hard to
    // handle here.
    if c == u32::from(b'\t') || el.el_line.cursor != el.el_line.lastchar {
        re_refresh(el);
        return;
    }

    // Bail-out 3. ERR-terminal-49, disposition `reproduce`: the test is
    // vacuous, and that is what makes it correct. `el_rprompt.p_pos.h` does
    // not hold the rprompt's width at this point — `re_refresh` step 9 left it
    // equal to the column *after* the rprompt, `t_size.h - 1` — so the
    // expression evaluates to `1 - el_cursor.h`, which is always less than 3.
    // Whenever a right prompt is displayed this bail-out therefore always
    // fires and the fast path is never taken, which is exactly what makes the
    // fast path safe. Do not "repair" it.
    let rhdiff = el.el_terminal.t_size.h - el.el_cursor.h - el.el_rprompt.p_pos.h;
    if el.el_rprompt.p_pos.h != 0 && rhdiff < 3 {
        // Clear out the rprompt if less than one character gap.
        re_refresh(el);
        return;
    }
    // Otherwise: end of line only, and no tab.

    match ct_chr_class(c) {
        // Already handled by bail-out 2; should never happen here.
        CHTYPE_TAB => {}
        CHTYPE_NL | CHTYPE_PRINT => {
            // ERR-terminal-50, disposition `reproduce`: the newline case is a
            // genuine inconsistency with the slow path. `re_fastputc` writes
            // the `'\n'` straight out and stores it in the display cell,
            // advancing the recorded column by one, whereas `re_addc` would
            // have terminated the virtual row and moved to the next one. With
            // the tty's ONLCR translation the terminal performs a CR/LF, so
            // the recorded cursor and the screen diverge. Reachable only by
            // pushing a literal newline into the buffer, never by Return.
            re_fastputc(el, c);
        }
        CHTYPE_ASCIICTL | CHTYPE_NONPRINT => {
            let mut visbuf = [0u32; VISUAL_WIDTH_MAX];
            let n = ct_visual_char(&mut visbuf, c);
            // A -1 return emits nothing.
            for &v in visbuf.iter().take(n.max(0) as usize) {
                re_fastputc(el, v);
            }
        }
        // The C's switch lists exactly the five classes and has no default;
        // `ct_chr_class` is total over them, so this is unreachable and, like
        // the C, does nothing.
        _ => {}
    }

    terminal_flush(el);
}

// [spec:libedit:def:refresh.re-clear-display-fn]
// [spec:libedit:sem:refresh.re-clear-display-fn]
pub fn re_clear_display(el: &mut EditLine) {
    // Step 1: assert that the real terminal cursor is at the home position of
    // the region. The caller is responsible for having actually put it there —
    // see `re_goto_bottom`, which writes the newline first.
    el.el_cursor.v = 0;
    el.el_cursor.h = 0;

    // Step 2: only the first cell of each row is touched, so the rest keeps
    // stale content; every row now reads as the empty string, which is all
    // that `re_update_line` and `wcslen` care about. `take` clamps to the row
    // array, which the C indexes by `t_size.v` alone.
    let rows = el.el_terminal.t_size.v.max(0) as usize;
    for row in el.el_display.iter_mut().take(rows) {
        row[0] = 0;
    }

    // Step 3: so the next `re_refresh` believes the previously drawn line
    // occupied exactly one row and does not try to erase rows below it.
    el.el_refresh.r_oldcv = 0;

    // `el_vdisplay` is not touched.
}

/// One bare CR/LF pair, the only motion either branch of [`re_clear_lines`]
/// has in common.
///
/// ERR-terminal-52, disposition `reproduce`: it goes straight out through
/// `terminal_putc`, so `el_cursor` never learns the terminal moved and the
/// `terminal_move_to_line` that follows computes its motion from a stale
/// value. The sequence is only coherent because the sole caller runs
/// `re_clear_display` immediately afterwards, forcing the recorded cursor to
/// (0, 0), and then redraws everything. The byte sequence a terminal receives
/// is the observable behaviour, so this is reproduced rather than "repaired".
fn scroll_one_line(el: &mut EditLine) {
    terminal_putc(el, u32::from(b'\r'));
    terminal_putc(el, u32::from(b'\n'));
}

/// Really blank rows `0..=rows`, bottom-up, one clear-to-end-of-line each.
///
/// Row 0 is included, so a previous line that occupied a single row still
/// costs a clear; the CR/LF that walks to the row above is what is skipped for
/// that last iteration alone.
fn clear_rows_bottom_up(el: &mut EditLine, rows: i32) {
    for i in (0..=rows).rev() {
        if i > 0 {
            scroll_one_line(el);
        }
        terminal_move_to_line(el, i);
        terminal_move_to_char(el, 0);
        let h = el.el_terminal.t_size.h;
        terminal_clear_eol(el, h);
    }
}

/// Push `rows` lines of old text up out of the way and leave the terminal on a
/// fresh line below them.
///
/// Nothing is erased: without the clear-to-end-of-line capability the old text
/// is still on the screen, merely scrolled above the region libedit is about
/// to redraw. The trailing move-to-last-line plus pair is the C's "go to last
/// line, go to BOL, go to a new line".
fn scroll_rows_away(el: &mut EditLine, rows: i32) {
    for _ in 1..=rows {
        scroll_one_line(el);
    }
    terminal_move_to_line(el, rows);
    scroll_one_line(el);
}

// [spec:libedit:def:refresh.re-clear-lines-fn]
// [spec:libedit:sem:refresh.re-clear-lines-fn]
pub(crate) fn re_clear_lines(el: &mut EditLine) {
    // The flag decides whether the previous line is erased or only shoved out
    // of sight; both walk the same `r_oldcv` rows and emit nothing else.
    let rows = el.el_refresh.r_oldcv;
    if el.el_terminal.t_flags & TERM_CAN_CEOL != 0 {
        clear_rows_bottom_up(el, rows);
    } else {
        scroll_rows_away(el, rows);
    }
}

#[cfg(test)]
#[path = "refresh/test.rs"]
mod test;
