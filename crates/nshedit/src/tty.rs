//! Ported from `src/tty.c`; rules live in `docs/spec/port/src/tty.md`.

// Every function body below is still `todo!()`, so every parameter is unused.
// Remove this once the bodies land.
#![allow(unused_variables)]

use crate::el::{EditLine, ElActionT};

/// C: `#define NN_IO 3` — the number of I/O modes (`ED_IO`, `EX_IO`,
/// `QU_IO`).
pub const NN_IO: usize = 3;
/// C: `#define MD_NN 5` — the number of termios mode words per I/O mode
/// (`MD_INP`, `MD_OUT`, `MD_CTL`, `MD_LIN`, `MD_CHAR`). Do not re-order.
pub const MD_NN: usize = 5;
/// C: `#define C_NCC 25` — the number of control characters libedit tracks.
pub const C_NCC: usize = 25;

/// Stand-in for POSIX `struct termios`.
///
/// `plan/decisions/no-c-ffi.md` bars linking libc, so there is no
/// `struct termios` to borrow and no `tcgetattr`/`tcsetattr` to call. The
/// POSIX field set is spelled out here because the `sem` rules address it by
/// name — `tty_bind_char` reads `t_ed.c_cc` by `V*` subscript and
/// `tty_stty` masks the four flag words — but the syscall mechanism is a
/// decision for the `tty.c` translation, and `NCCS` is platform-dependent.
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    /// Indexed by the termios `V*` subscripts, not by libedit's `C_*` ones.
    pub c_cc: Vec<u8>,
}

/// Stand-in for POSIX `speed_t`. Same reasoning as [`Termios`]; the C
/// carries the encoded `B*` value, not a baud number.
pub type SpeedT = u32;

// [spec:libedit:def:tty.ttymodes-t]
/// One row of the mode-name table `ttymodes[]`: the name a user types at
/// `stty`, the bit it sets, and which mode word it belongs to.
pub struct TtymodesT {
    pub m_name: &'static str,
    pub m_value: u32,
    /// `MD_INP`, `MD_OUT`, `MD_CTL`, `MD_LIN` or `MD_CHAR`.
    pub m_type: i32,
}

// [spec:libedit:def:tty.ttymap-t]
/// One row of `tty_map[]`, tying a tty control character to the editor
/// actions it should invoke.
pub struct TtymapT {
    /// C: `wint_t nch` — libedit's own `C_*` index into `t_c[ED_IO]`. The
    /// table's sentinel row uses `(wint_t)-1`, which is why this is `u32`
    /// and not a smaller type.
    pub nch: u32,
    /// C: `wint_t och` — the termios `V*` subscript into `t_ed.c_cc`.
    pub och: u32,
    /// Bindings, indexed 0 = emacs, 1 = vi insert, 2 = vi command.
    pub bind: [ElActionT; 3],
}

/// One cell of [`TtypermT`]: the set/clear masks for one mode word of one
/// I/O mode.
///
/// The C leaves this struct anonymous inside the array typedef; Rust cannot,
/// so it is named. It belongs to
/// `def:tty.ttyperm-t-nn-io-md-nn`.
pub struct TtypermEntry {
    pub t_name: &'static str,
    pub t_setmask: u32,
    pub t_clrmask: u32,
}

// [spec:libedit:def:tty.ttyperm-t-nn-io-md-nn]
/// C: `typedef struct { ... } ttyperm_t[NN_IO][MD_NN];` — an array typedef,
/// so the element struct is [`TtypermEntry`] and this names the array. Rows
/// are I/O modes, columns are termios mode words.
pub type TtypermT = [[TtypermEntry; MD_NN]; NN_IO];

// [spec:libedit:def:tty.ttychar-t-nn-io-c-ncc]
/// C: `typedef unsigned char ttychar_t[NN_IO][C_NCC];` — the control
/// characters libedit wants in each I/O mode, indexed by the `C_*`
/// constants.
pub type TtycharT = [[u8; C_NCC]; NN_IO];

// [spec:libedit:def:tty.el-tty-t]
/// Everything libedit knows about the terminal line discipline.
pub struct ElTtyT {
    pub t_t: TtypermT,
    pub t_c: TtycharT,
    /// The original mode, saved at `tty_init` and restored at `tty_end`.
    pub t_or: Termios,
    /// The "execute" (cooked) mode.
    pub t_ex: Termios,
    /// The "edit" (raw) mode.
    pub t_ed: Termios,
    /// The "quote" mode.
    pub t_ts: Termios,
    pub t_tabs: i32,
    pub t_eight: i32,
    pub t_speed: SpeedT,
    /// Which of `ED_IO`/`EX_IO`/`QU_IO` is currently installed.
    pub t_mode: u8,
    /// The `_POSIX_VDISABLE` value for this terminal.
    pub t_vdisable: u8,
    pub t_initialized: u8,
}

// [spec:libedit:def:tty.tty-getty-fn]
// [spec:libedit:sem:tty.tty-getty-fn]
/// C: `static int tty_getty(EditLine *el, struct termios *t)` — `tcgetattr`
/// on `el->el_infd`, retried on EINTR.
///
/// Every call site passes a field of `el->el_tty` as `t`, which Rust will not
/// let the caller borrow while `el` is borrowed; resolving that is the body
/// translation's problem, and the signature stays the C's.
fn tty_getty(el: &mut EditLine, t: &mut Termios) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-setty-fn]
// [spec:libedit:sem:tty.tty-setty-fn]
/// C: `static int tty_setty(EditLine *el, int action, const struct termios
/// *t)` — `tcsetattr` on `el->el_infd`, retried on EINTR. Same aliasing note
/// as [`tty_getty`].
fn tty_setty(el: &mut EditLine, action: i32, t: &Termios) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-setup-fn]
// [spec:libedit:sem:tty.tty-setup-fn]
fn tty_setup(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-init-fn]
// [spec:libedit:sem:tty.tty-init-fn]
pub(crate) fn tty_init(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-end-fn]
// [spec:libedit:sem:tty.tty-end-fn]
pub(crate) fn tty_end(el: &mut EditLine, how: i32) {
    todo!()
}

// [spec:libedit:def:tty.tty-getspeed-fn]
// [spec:libedit:sem:tty.tty-getspeed-fn]
/// C: `static speed_t tty__getspeed(struct termios *td)` — the pointer is
/// non-const but only read.
///
/// The C's doubled underscore is not snake case to rustc; the name stays,
/// here and in the four below.
#[allow(non_snake_case)]
fn tty__getspeed(td: &Termios) -> SpeedT {
    todo!()
}

// [spec:libedit:def:tty.tty-getcharindex-fn]
// [spec:libedit:sem:tty.tty-getcharindex-fn]
/// Maps one of libedit's `C_*` indices to the termios `V*` subscript, or -1
/// when the platform has no such control character.
#[allow(non_snake_case)]
fn tty__getcharindex(i: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-getchar-fn]
// [spec:libedit:sem:tty.tty-getchar-fn]
/// C: `static void tty__getchar(struct termios *td, unsigned char *s)` —
/// reads `td->c_cc` by `V*` subscript into `s` by `C_*` index. `s` is one row
/// of [`TtycharT`].
#[allow(non_snake_case)]
fn tty__getchar(td: &Termios, s: &mut [u8]) {
    todo!()
}

// [spec:libedit:def:tty.tty-setchar-fn]
// [spec:libedit:sem:tty.tty-setchar-fn]
/// The inverse of [`tty__getchar`]: writes `s`, indexed by `C_*`, into
/// `td->c_cc`, indexed by `V*`.
#[allow(non_snake_case)]
fn tty__setchar(td: &mut Termios, s: &[u8]) {
    todo!()
}

// [spec:libedit:def:tty.tty-bind-char-fn]
// [spec:libedit:sem:tty.tty-bind-char-fn]
pub(crate) fn tty_bind_char(el: &mut EditLine, force: i32) {
    todo!()
}

// [spec:libedit:def:tty.tty-get-flag-fn]
// [spec:libedit:sem:tty.tty-get-flag-fn]
/// C: `static tcflag_t * tty__get_flag(struct termios *t, int kind)` — picks
/// one of the four mode words by `MD_INP`/`MD_OUT`/`MD_CTL`/`MD_LIN`, and
/// aborts on anything else. `tcflag_t` is the `u32` [`Termios`] uses.
#[allow(non_snake_case)]
fn tty__get_flag(t: &mut Termios, kind: i32) -> &mut u32 {
    todo!()
}

// [spec:libedit:def:tty.tty-update-flag-fn]
// [spec:libedit:sem:tty.tty-update-flag-fn]
fn tty_update_flag(el: &mut EditLine, f: u32, mode: i32, kind: i32) -> u32 {
    todo!()
}

// [spec:libedit:def:tty.tty-update-flags-fn]
// [spec:libedit:sem:tty.tty-update-flags-fn]
fn tty_update_flags(el: &mut EditLine, kind: i32) {
    todo!()
}

// [spec:libedit:def:tty.tty-update-char-fn]
// [spec:libedit:sem:tty.tty-update-char-fn]
fn tty_update_char(el: &mut EditLine, mode: i32, c: i32) {
    todo!()
}

// [spec:libedit:def:tty.tty-rawmode-fn]
// [spec:libedit:sem:tty.tty-rawmode-fn]
pub(crate) fn tty_rawmode(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-cookedmode-fn]
// [spec:libedit:sem:tty.tty-cookedmode-fn]
pub(crate) fn tty_cookedmode(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-quotemode-fn]
// [spec:libedit:sem:tty.tty-quotemode-fn]
pub(crate) fn tty_quotemode(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-noquotemode-fn]
// [spec:libedit:sem:tty.tty-noquotemode-fn]
pub(crate) fn tty_noquotemode(el: &mut EditLine) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-stty-fn]
// [spec:libedit:sem:tty.tty-stty-fn]
/// The `setty` editrc command handler, sharing the C's
/// `int (*)(EditLine *, int, const wchar_t **)` shape with the three `*tc`
/// handlers in `terminal.rs`; the C's NULL-terminated `wchar_t **` becomes a
/// slice of wide strings.
pub(crate) fn tty_stty(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    todo!()
}

// [spec:libedit:def:tty.tty-printchar-fn]
// [spec:libedit:sem:tty.tty-printchar-fn]
/// Debug dump of the control characters; the C guards it with `#ifdef notyet`
/// and never calls it. `s` is one row of [`TtycharT`], read only.
fn tty_printchar(el: &mut EditLine, s: &[u8]) {
    todo!()
}

// [spec:libedit:def:tty.tty-setup-flags-fn]
// [spec:libedit:sem:tty.tty-setup-flags-fn]
/// C: `static void tty_setup_flags(EditLine *el, struct termios *tios, int
/// mode)`. Same aliasing note as [`tty_getty`]: `tios` is always a field of
/// `el->el_tty` at the call sites.
fn tty_setup_flags(el: &mut EditLine, tios: &mut Termios, mode: i32) {
    todo!()
}

// [spec:libedit:def:tty.tty-get-signal-character-fn]
// [spec:libedit:sem:tty.tty-get-signal-character-fn]
pub(crate) fn tty_get_signal_character(el: &mut EditLine, sig: i32) -> i32 {
    todo!()
}
