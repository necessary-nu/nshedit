//! Ported from `src/terminal.c`; rules live in
//! `docs/spec/port/src/terminal.md`.
//!
//! Capabilities come from terminfo through the `term` crate, not from a
//! linked termcap provider, and are addressed by terminfo long name — see
//! `plan/decisions/terminal-caps-via-term-crate.md`. The two table structs
//! below keep their C names (`termcapstr`, `termcapval`) because the rules
//! do, but the `name` they carry is the terminfo one.

// Every function body below is still `todo!()`, so every parameter is unused.
// Remove this once the bodies land.
#![allow(unused_variables)]

use core::ffi::c_char;
use std::io::Write;

use term::terminfo::TermInfo;

use crate::el::{CoordT, EditLine};
use crate::keymacro::KeymacroValueT;
use crate::tty::SpeedT;

// [spec:libedit:def:terminal.termcapstr]
/// One row of the string-capability table: 39 rows, in the order the
/// `T_*` indices define.
pub struct Termcapstr {
    /// The capability's name. Termcap two-letter code in the C
    /// (`"al"`, `"bl"`, …), terminfo long name here.
    pub name: &'static str,
    /// Human-readable description, shown by `telltc`.
    pub long_name: &'static str,
}

// [spec:libedit:def:terminal.termcapval]
/// One row of the flag/numeric-capability table: 8 rows, in the order the
/// `T_*` indices define. Structurally identical to [`Termcapstr`]; the C
/// declares it separately and so does this.
pub struct Termcapval {
    /// Termcap two-letter code in the C (`"am"`, `"pt"`, …), terminfo long
    /// name here. `MT`, `pt` and `xt` have no clean terminfo counterpart and
    /// are resolved per capability during the port.
    pub name: &'static str,
    /// Human-readable description, shown by `telltc`.
    pub long_name: &'static str,
}

// [spec:libedit:def:terminal.funckey-t]
/// A symbolic function-key binding: one row of `el_terminal.t_fkey`, indexed
/// by `A_K_DN` … `A_K_DE` (7 rows).
pub struct FunckeyT {
    /// C: `const wchar_t *name` — name of the key (`L"up"`, `L"down"`, …).
    /// Only ever a compiled-in literal, and NULL in a freshly zeroed table,
    /// which `sem:terminal.terminal-init-fn` notes the first
    /// `terminal_bind_arrow` actually sees.
    pub name: Option<&'static [u32]>,
    /// Index into the string-capability table.
    pub key: i32,
    /// Function bound to it.
    pub fun: KeymacroValueT,
    /// Type of function: `XK_CMD`, `XK_STR` or `XK_NOD`.
    pub r#type: i32,
}

// [spec:libedit:def:terminal.el-terminal-t]
/// The loaded terminal description and everything derived from it.
pub struct ElTerminalT {
    /// C: `const char *t_name` — the terminal name. The C aliases whatever
    /// string `terminal_set` was handed (an argument, the `TERM` value, or
    /// the literal `"dumb"`); an owned copy avoids inheriting that lifetime.
    pub t_name: Option<String>,
    /// Screen size. Beware: `terminal_set` stores columns in `v` and lines
    /// in `h`, the reverse of the field names.
    pub t_size: CoordT,
    /// C: `int t_flags` — the `TERM_CAN_*`/`TERM_HAS_*` bit set. Kept an
    /// integer flag word, as in the C.
    pub t_flags: i32,
    /// C: `char *t_buf` — the TC_BUFSIZE (2048) byte capability string
    /// pool. `sem:terminal.terminal-alloc-fn` directs the
    /// port not to reproduce the pool at all — an owned string per `t_str`
    /// slot removes the overflow and the corrupting compaction together —
    /// so this and `t_loc` are expected to fall out of use.
    pub t_buf: Vec<u8>,
    /// C: `size_t t_loc` — high-water mark within `t_buf`.
    pub t_loc: usize,
    /// C: `char **t_str` — the string capabilities, 39 slots. `None` is the
    /// C's NULL slot, meaning "capability absent". Owned per slot rather
    /// than pointing into `t_buf`, per the `terminal_alloc` rule above.
    ///
    /// Bytes, not a string: a capability may legitimately carry a byte above
    /// 0x7f — 8-bit CSI is the obvious case — and `term` hands them back as
    /// bytes for that reason. Decoding here would mangle those entries, and
    /// nothing in this layer needs them to be text.
    pub t_str: Vec<Option<Vec<u8>>>,
    /// C: `int *t_val` — the flag and numeric capabilities, 8 slots.
    pub t_val: Vec<i32>,
    /// C: `char *t_cap` — the TC_BUFSIZE scratch area the C's `tgetent`
    /// copies the raw terminal entry into. libedit never reads it;
    /// `sem:terminal.tgetent-fn` says the terminfo
    /// replacement takes no such buffer and the field can go with it.
    pub t_cap: Vec<u8>,
    /// The loaded terminfo entry. No C counterpart: the C leaves the entry in
    /// the termcap library's global state, so nothing had to hold it. We do,
    /// because `terminal_echotc` looks capabilities up with no preceding
    /// load and has nowhere else to find them.
    pub t_entry: Option<TermInfo>,
    /// C: `funckey_t *t_fkey` — the function-key table, `A_K_NKEYS` (7)
    /// entries.
    pub t_fkey: Vec<FunckeyT>,
}

// ---------------------------------------------------------------------------
// The terminfo capability boundary.
//
// The six functions below are `extern` declarations in the C — libedit calls
// into ncurses for them — and are ours here, backed by the `term` crate's
// terminfo reader. `plan/decisions/terminal-caps-via-term-crate.md` makes
// their rules the contract for that layer. Two shape changes run through all
// six. The loaded entry is a [`TermInfo`] rather than process-global curses
// state, and it is passed explicitly because `ElTerminalT` has no field to
// hold it (`t_cap`, the C's raw-entry buffer, is not one: the rules retire
// it). Capability strings are `String`/`&str` here, matching
// `ElTerminalT::t_str`, so the conversion from `term`'s raw `Vec<u8>` happens
// at this boundary and nowhere else.
// ---------------------------------------------------------------------------

// [spec:libedit:def:terminal.tgetent-fn]
// [spec:libedit:sem:terminal.tgetent-fn]
/// Load the capability entry for terminal type `name` and make it the entry
/// the three lookups below resolve against.
///
/// C: `extern int tgetent(char *, const char *)`. The C's first argument is a
/// caller-supplied 2048-byte buffer that receives the raw termcap entry and
/// that libedit never reads; the terminfo replacement has no such buffer, so
/// the loaded entry itself takes that out-parameter's place.
///
/// Returns > 0 when an entry of that name was found and loaded, 0 when the
/// database is readable but has no such entry, and -1 when the database
/// cannot be read at all. `terminal_set` distinguishes all three.
pub(crate) fn tgetent(entry: &mut Option<TermInfo>, name: &str) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.tgetflag-fn]
// [spec:libedit:sem:terminal.tgetflag-fn]
/// Boolean capability lookup: 1 if `entry` defines `name`, 0 if it does not,
/// if `name` is not a capability at all, or if no entry is loaded. There is
/// no error return. `name` is the terminfo name, not the C's termcap code.
pub(crate) fn tgetflag(entry: Option<&TermInfo>, name: &str) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.tgetnum-fn]
// [spec:libedit:sem:terminal.tgetnum-fn]
/// Numeric capability lookup. Returns -1, *not* 0, when the capability is
/// absent, cancelled, or no entry is loaded: `terminal_set` reads that -1 as
/// "absent" and clamps it to the 80x24 default.
pub(crate) fn tgetnum(entry: Option<&TermInfo>, name: &str) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.tgetstr-fn]
// [spec:libedit:sem:terminal.tgetstr-fn]
/// String capability lookup, returning the raw unexpanded value — parameter
/// expansion is [`tgoto`]'s job, and the `$<...>` padding runs must survive
/// to [`tputs`]. `None` is the C's NULL: capability absent, or no entry
/// loaded.
///
/// C: `extern char* tgetstr(char*, char**)`. The second argument is an in/out
/// cursor into a caller-supplied scratch arena; the port returns an owned
/// value and the parameter disappears with the arena.
///
/// Bytes, not text: a capability may carry a byte above 0x7f, and `term`
/// returns them as bytes for that reason.
pub(crate) fn tgetstr(entry: Option<&TermInfo>, name: &str) -> Option<Vec<u8>> {
    todo!()
}

// [spec:libedit:def:terminal.tgoto-fn]
// [spec:libedit:sem:terminal.tgoto-fn]
/// Substitute two parameters into a capability string — **column first, row
/// second**, `tgoto`'s order and not terminfo's — and return the expansion.
///
/// C: `extern char* tgoto(const char*, int, int)`, returning a pointer into a
/// static buffer that the next call overwrites; every call site consumes it
/// immediately, so the port returns an owned string. The expansion must
/// preserve any `$<...>` padding runs, which `term`'s expander discards.
pub(crate) fn tgoto(cap: &[u8], col: i32, row: i32) -> Vec<u8> {
    todo!()
}

// [spec:libedit:def:terminal.tputs-fn]
// [spec:libedit:sem:terminal.tputs-fn]
/// Write an already-expanded capability string to `out`, turning its embedded
/// `$<...>` padding into real delay. The one of the six with no counterpart
/// in the `term` crate at all.
///
/// C: `extern int tputs(const char *, int, int (*)(int))`. The callback takes
/// no user data, which is why the C's caller has to park the destination
/// `FILE *` in a global behind a mutex; a writer parameter retires both.
///
/// `affcnt` is the caller's count of affected screen lines and feeds only the
/// `*` form of the padding grammar. `entry` supplies the pad character
/// (terminfo `pad_char`, NUL when absent) and the xon/xoff flag that makes an
/// advisory delay skippable. `baud` is the tty's output speed as
/// `el_tty.t_speed` records it — an encoded `speed_t`, so the body decodes it
/// to bits per second, and treats zero or unknown as "emit no padding".
///
/// The C returns OK/0 and libedit discards it; the i32 stays for now.
pub(crate) fn tputs(
    out: &mut dyn Write,
    cap: &[u8],
    affcnt: i32,
    entry: Option<&TermInfo>,
    baud: SpeedT,
) -> i32 {
    todo!()
}

// ---------------------------------------------------------------------------
// libedit's own terminal layer.
// ---------------------------------------------------------------------------

// [spec:libedit:def:terminal.terminal-setflags-fn]
// [spec:libedit:sem:terminal.terminal-setflags-fn]
fn terminal_setflags(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-init-fn]
// [spec:libedit:sem:terminal.terminal-init-fn]
pub(crate) fn terminal_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-end-fn]
// [spec:libedit:sem:terminal.terminal-end-fn]
pub(crate) fn terminal_end(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-alloc-fn]
// [spec:libedit:sem:terminal.terminal-alloc-fn]
/// Store capability string `cap` in string-capability slot `t`; `None` (the
/// C's NULL) and the empty string both clear the slot.
///
/// C: `static void terminal_alloc(EditLine *el, const struct termcapstr *t,
/// const char *cap)`, where `t` points into the static table and the slot
/// index is recovered by pointer subtraction. The index is the only thing the
/// C reads out of `t`, so it is passed directly. The C's `t_buf` string pool
/// is not to be reproduced — see the rule — so this is a plain assignment
/// into `el_terminal.t_str`.
fn terminal_alloc(el: &mut EditLine, t: usize, cap: Option<&str>) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-rebuffer-display-fn]
// [spec:libedit:sem:terminal.terminal-rebuffer-display-fn]
fn terminal_rebuffer_display(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-alloc-buffer-fn]
// [spec:libedit:sem:terminal.terminal-alloc-buffer-fn]
/// Allocate one screen image at the current `t_size`, `t_size.v` rows of
/// `t_size.h + 1` cells.
///
/// C: `static wint_t ** terminal_alloc_buffer(EditLine *el)`, NULL on
/// allocation failure. `None` keeps that failure path reachable for
/// `terminal_alloc_display`, which the rules describe, even though a Rust
/// allocation failure aborts instead. The C's NULL row terminator is the
/// `Vec`'s length here.
fn terminal_alloc_buffer(el: &mut EditLine) -> Option<Vec<Vec<u32>>> {
    todo!()
}

// [spec:libedit:def:terminal.terminal-free-buffer-fn]
// [spec:libedit:sem:terminal.terminal-free-buffer-fn]
/// C: `static void terminal_free_buffer(wint_t ***bp)` — frees the rows and
/// the row array and NULLs the caller's field. `el_display` and
/// `el_vdisplay` are owning `Vec`s, so "NULL" is the empty `Vec`, and the
/// `Vec` itself has to be the parameter: a slice cannot be emptied.
#[allow(clippy::ptr_arg)]
fn terminal_free_buffer(bp: &mut Vec<Vec<u32>>) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-alloc-display-fn]
// [spec:libedit:sem:terminal.terminal-alloc-display-fn]
fn terminal_alloc_display(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-free-display-fn]
// [spec:libedit:sem:terminal.terminal-free-display-fn]
fn terminal_free_display(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-move-to-line-fn]
// [spec:libedit:sem:terminal.terminal-move-to-line-fn]
pub(crate) fn terminal_move_to_line(el: &mut EditLine, where_: i32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-move-to-char-fn]
// [spec:libedit:sem:terminal.terminal-move-to-char-fn]
pub(crate) fn terminal_move_to_char(el: &mut EditLine, where_: i32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-overwrite-fn]
// [spec:libedit:sem:terminal.terminal-overwrite-fn]
/// C: `libedit_private void terminal_overwrite(EditLine *el, const wchar_t
/// *cp, size_t n)`. `n` is kept alongside the slice: callers pass a buffer
/// longer than the run they mean to write.
pub(crate) fn terminal_overwrite(el: &mut EditLine, cp: &[u32], n: usize) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-deletechars-fn]
// [spec:libedit:sem:terminal.terminal-deletechars-fn]
pub(crate) fn terminal_deletechars(el: &mut EditLine, num: i32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-insertwrite-fn]
// [spec:libedit:sem:terminal.terminal-insertwrite-fn]
/// C: `libedit_private void terminal_insertwrite(EditLine *el, wchar_t *cp,
/// int num)`. As with `terminal_overwrite`, `num` is the run length within
/// `cp`, not `cp`'s length.
pub(crate) fn terminal_insertwrite(el: &mut EditLine, cp: &[u32], num: i32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-clear-eol-fn]
// [spec:libedit:sem:terminal.terminal-clear-eol-fn]
/// The C's name, `EOL` and all, so it stays non-snake-case.
#[allow(non_snake_case)]
pub(crate) fn terminal_clear_EOL(el: &mut EditLine, num: i32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-clear-screen-fn]
// [spec:libedit:sem:terminal.terminal-clear-screen-fn]
pub(crate) fn terminal_clear_screen(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-beep-fn]
// [spec:libedit:sem:terminal.terminal-beep-fn]
pub(crate) fn terminal_beep(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-get-fn]
// [spec:libedit:sem:terminal.terminal-get-fn]
/// C: `libedit_private void terminal_get(EditLine *el, const char **term)` —
/// hands back `el_terminal.t_name` through an out-parameter, which stays one
/// here. The borrow is tied to `el` because the name is `t_name`'s own
/// storage, exactly as the C hands out its interior pointer.
pub(crate) fn terminal_get<'a>(el: &'a mut EditLine, term: &mut Option<&'a str>) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-set-fn]
// [spec:libedit:sem:terminal.terminal-set-fn]
/// `term` is `None` for the C's NULL, which means "take the type from the
/// environment".
pub(crate) fn terminal_set(el: &mut EditLine, term: Option<&str>) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-get-size-fn]
// [spec:libedit:sem:terminal.terminal-get-size-fn]
pub(crate) fn terminal_get_size(el: &mut EditLine, lins: &mut i32, cols: &mut i32) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-change-size-fn]
// [spec:libedit:sem:terminal.terminal-change-size-fn]
pub(crate) fn terminal_change_size(el: &mut EditLine, lins: i32, cols: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-init-arrow-fn]
// [spec:libedit:sem:terminal.terminal-init-arrow-fn]
fn terminal_init_arrow(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-reset-arrow-fn]
// [spec:libedit:sem:terminal.terminal-reset-arrow-fn]
fn terminal_reset_arrow(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-set-arrow-fn]
// [spec:libedit:sem:terminal.terminal-set-arrow-fn]
/// C: `libedit_private int terminal_set_arrow(EditLine *el, const wchar_t
/// *name, keymacro_value_t *fun, int type)`. The C copies `*fun` into the
/// function-key table; [`KeymacroValueT`] owns a `Vec` in its `Str` arm, so
/// the copy is a move and `fun` is taken by value.
pub(crate) fn terminal_set_arrow(
    el: &mut EditLine,
    name: &[u32],
    fun: KeymacroValueT,
    type_: i32,
) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-clear-arrow-fn]
// [spec:libedit:sem:terminal.terminal-clear-arrow-fn]
pub(crate) fn terminal_clear_arrow(el: &mut EditLine, name: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-print-arrow-fn]
// [spec:libedit:sem:terminal.terminal-print-arrow-fn]
pub(crate) fn terminal_print_arrow(el: &mut EditLine, name: &[u32]) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-bind-arrow-fn]
// [spec:libedit:sem:terminal.terminal-bind-arrow-fn]
pub(crate) fn terminal_bind_arrow(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-tputs-fn]
// [spec:libedit:sem:terminal.terminal-tputs-fn]
/// Emit an already-expanded capability string to `el->el_outfile`, honouring
/// its padding. Reduces to a call to [`tputs`] with this `EditLine`'s writer,
/// pad source and line speed; the C's file-static `FILE *` and its mutex have
/// no counterpart here.
fn terminal_tputs(el: &mut EditLine, cap: &str, affcnt: i32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-putc-fn]
// [spec:libedit:sem:terminal.terminal-putc-fn]
/// The C's doubled underscore is not snake case to rustc; the name stays.
#[allow(non_snake_case)]
pub(crate) fn terminal__putc(el: &mut EditLine, c: u32) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-flush-fn]
// [spec:libedit:sem:terminal.terminal-flush-fn]
#[allow(non_snake_case)]
pub(crate) fn terminal__flush(el: &mut EditLine) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-writec-fn]
// [spec:libedit:sem:terminal.terminal-writec-fn]
pub(crate) fn terminal_writec(el: &mut EditLine, c: u32) {
    todo!()
}

// [spec:libedit:def:terminal.terminal-telltc-fn]
// [spec:libedit:sem:terminal.terminal-telltc-fn]
/// One of the four editrc command handlers, all sharing the C's
/// `int (*)(EditLine *, int, const wchar_t **)` shape. The C's
/// NULL-terminated `wchar_t **` becomes a slice of wide strings; `argc` is
/// kept because the C passes it, even though this handler ignores both.
pub(crate) fn terminal_telltc(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-settc-fn]
// [spec:libedit:sem:terminal.terminal-settc-fn]
pub(crate) fn terminal_settc(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-gettc-fn]
// [spec:libedit:sem:terminal.terminal-gettc-fn]
/// C: `libedit_private int terminal_gettc(EditLine *el, int argc, char
/// **argv)`. Unlike its three neighbours this one takes *narrow* strings, and
/// `argv[2]` is not a string at all but a caller-supplied destination pointer
/// smuggled through the array — `char **` for the string and boolean
/// capabilities, `int *` for the numeric ones. It stays a raw pointer array:
/// it arrives from `el_get`'s varargs and crosses the C ABI unchanged, so the
/// body writes through it unsafely.
pub(crate) fn terminal_gettc(el: &mut EditLine, argc: i32, argv: &[*mut c_char]) -> i32 {
    todo!()
}

// [spec:libedit:def:terminal.terminal-echotc-fn]
// [spec:libedit:sem:terminal.terminal-echotc-fn]
pub(crate) fn terminal_echotc(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    todo!()
}
