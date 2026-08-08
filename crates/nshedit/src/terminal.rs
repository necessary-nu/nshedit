//! Ported from `src/terminal.c`; rules live in
//! `docs/spec/port/src/terminal.md`.
//!
//! Capabilities come from terminfo through the `term` crate, not from a
//! linked termcap provider, and are addressed by terminfo long name — see
//! `plan/decisions/terminal-caps-via-term-crate.md`. The two table structs
//! below keep their C names (`termcapstr`, `termcapval`) because the rules
//! do, but the `name` they carry is the terminfo one.
//!
//! # Which terminfo name
//!
//! The decision says "long name"; `term` keys its `bools`/`numbers`/`strings`
//! maps by the terminfo **capname** (the short form — `el`, `cuu1`, `kcud1`),
//! because `TermInfo::from_path` parses with `longnames: false`. So the table
//! `name` field is the capname: it is what the terminfo lookup uses and what
//! `telltc` prints back. Each row's doc records the long name.
//!
//! On the way *in* it is not the only spelling accepted. What a user types at
//! `settc`/`gettc`/`echotc` is normally the C's termcap code, so
//! [`resolve_cap`] takes either and that is where the two namespaces meet.
//!
//! Two capabilities have no terminfo counterpart at all — the C's `pt`
//! (physical tabs) and `MT` (a termcap-only meta-key extension). Their rows
//! keep the C's termcap spelling, which is not a terminfo capname, so the
//! lookup misses and the value is always 0. That is exactly what ncurses does
//! for them today (ERR-terminal-61, ERR-terminal-62), so it reproduces rather
//! than invents.

use core::ffi::c_char;
use std::io::Write;

use nshterm::TermInfo;
use nshterm::parm::{Param, Variables, expand};

use crate::chartype::{
    MB_FILL_CHAR, VISUAL_WIDTH_MAX, ct_decode_string, ct_encode_char, ct_encode_string,
    ct_visual_char, ct_visual_string,
};
use crate::el::{CoordT, EDIT_DISABLED, EditLine, ElActionT};
use crate::fcns::{
    ED_DELETE_NEXT_CHAR, ED_MOVE_TO_BEG, ED_MOVE_TO_END, ED_NEXT_CHAR, ED_NEXT_HISTORY,
    ED_PREV_CHAR, ED_PREV_HISTORY, ED_SEQUENCE_LEAD_IN, ED_UNASSIGNED,
};
use crate::keymacro::{KeymacroValueT, keymacro_add, keymacro_clear, keymacro_kprint};
use crate::literal::{EL_LITERAL, literal_get};
use crate::locale;
use crate::map::{ElMapCurrent, MAP_VI};
use crate::refresh::re_clear_display;
use crate::stdio::write_fd;
use crate::tty::SpeedT;

/// C: `#define TC_BUFSIZE ((size_t)2048)`.
const TC_BUFSIZE: usize = 2048;

// `t_flags` bits. C: `src/terminal.h`. Kept an integer flag word, as the C
// does; `EL_CAN_*`/`EL_HAS_*` are macros over `t_flags` there and are spelled
// out at each test here.
/// C: `TERM_CAN_INSERT` — has an insert capability.
pub(crate) const TERM_CAN_INSERT: i32 = 0x001;
/// C: `TERM_CAN_DELETE` — has a delete capability.
pub(crate) const TERM_CAN_DELETE: i32 = 0x002;
/// C: `TERM_CAN_CEOL` — has clear-to-end-of-line.
pub(crate) const TERM_CAN_CEOL: i32 = 0x004;
/// C: `TERM_CAN_TAB` — can use tabs. Effectively dead: see
/// [`T_PT`] and ERR-terminal-61.
pub(crate) const TERM_CAN_TAB: i32 = 0x008;
/// C: `TERM_CAN_ME` — one sequence turns every attribute off.
pub(crate) const TERM_CAN_ME: i32 = 0x010;
/// C: `TERM_CAN_UP` — can move the cursor up.
pub(crate) const TERM_CAN_UP: i32 = 0x020;
/// C: `TERM_HAS_META` — has a meta key.
pub(crate) const TERM_HAS_META: i32 = 0x040;
/// C: `TERM_HAS_AUTO_MARGINS` — wraps by itself at the right margin.
pub(crate) const TERM_HAS_AUTO_MARGINS: i32 = 0x080;
/// C: `TERM_HAS_MAGIC_MARGINS` — defers the wrap until the next character.
pub(crate) const TERM_HAS_MAGIC_MARGINS: i32 = 0x100;

// Function-key table indices. C: `src/terminal.h`.
/// C: `A_K_DN`.
pub(crate) const A_K_DN: usize = 0;
/// C: `A_K_UP`.
pub(crate) const A_K_UP: usize = 1;
/// C: `A_K_LT`.
pub(crate) const A_K_LT: usize = 2;
/// C: `A_K_RT`.
pub(crate) const A_K_RT: usize = 3;
/// C: `A_K_HO`.
pub(crate) const A_K_HO: usize = 4;
/// C: `A_K_EN`.
pub(crate) const A_K_EN: usize = 5;
/// C: `A_K_DE`.
pub(crate) const A_K_DE: usize = 6;
/// C: `A_K_NKEYS`.
pub(crate) const A_K_NKEYS: usize = 7;

// C: `src/keymacro.h`. Declared here because `keymacro.rs` does not carry
// them yet and this file cannot add them there; they belong next to
// [`KeymacroValueT`] and should move once that module lands.
/// C: `#define XK_CMD 0`.
const XK_CMD: i32 = 0;
/// C: `#define XK_NOD 2`.
const XK_NOD: i32 = 2;

// String-capability table indices. C: the `#define T_xx` run interleaved with
// `tstr[]`.
/// C: `T_al` — terminfo `insert_line`.
pub(crate) const T_AL: usize = 0;
/// C: `T_bl` — terminfo `bell`.
pub(crate) const T_BL: usize = 1;
/// C: `T_cd` — terminfo `clr_eos`.
pub(crate) const T_CD: usize = 2;
/// C: `T_ce` — terminfo `clr_eol`.
pub(crate) const T_CE: usize = 3;
/// C: `T_ch` — terminfo `column_address`.
pub(crate) const T_CH: usize = 4;
/// C: `T_cl` — terminfo `clear_screen`.
pub(crate) const T_CL: usize = 5;
/// C: `T_dc` — terminfo `delete_character`.
pub(crate) const T_DC1: usize = 6;
/// C: `T_dl` — terminfo `delete_line`.
pub(crate) const T_DL: usize = 7;
/// C: `T_dm` — terminfo `enter_delete_mode`.
pub(crate) const T_DM: usize = 8;
/// C: `T_ed` — terminfo `exit_delete_mode`.
pub(crate) const T_ED: usize = 9;
/// C: `T_ei` — terminfo `exit_insert_mode`.
pub(crate) const T_EI: usize = 10;
/// C: `T_fs` — terminfo `from_status_line`.
pub(crate) const T_FS: usize = 11;
/// C: `T_ho` — terminfo `cursor_home`.
pub(crate) const T_HO: usize = 12;
/// C: `T_ic` — terminfo `insert_character`.
pub(crate) const T_IC1: usize = 13;
/// C: `T_im` — terminfo `enter_insert_mode`.
pub(crate) const T_IM: usize = 14;
/// C: `T_ip` — terminfo `insert_padding`.
pub(crate) const T_IP: usize = 15;
/// C: `T_kd` — terminfo `key_down`.
pub(crate) const T_KD: usize = 16;
/// C: `T_kl` — terminfo `key_left`.
pub(crate) const T_KL: usize = 17;
/// C: `T_kr` — terminfo `key_right`.
pub(crate) const T_KR: usize = 18;
/// C: `T_ku` — terminfo `key_up`.
pub(crate) const T_KU: usize = 19;
/// C: `T_md` — terminfo `enter_bold_mode`.
pub(crate) const T_MD: usize = 20;
/// C: `T_me` — terminfo `exit_attribute_mode`.
pub(crate) const T_ME: usize = 21;
/// C: `T_nd` — terminfo `cursor_right`.
pub(crate) const T_ND: usize = 22;
/// C: `T_se` — terminfo `exit_standout_mode`.
pub(crate) const T_SE: usize = 23;
/// C: `T_so` — terminfo `enter_standout_mode`.
pub(crate) const T_SO: usize = 24;
/// C: `T_ts` — terminfo `to_status_line`.
pub(crate) const T_TS: usize = 25;
/// C: `T_up` — terminfo `cursor_up`.
pub(crate) const T_UP1: usize = 26;
/// C: `T_us` — terminfo `enter_underline_mode`.
pub(crate) const T_US: usize = 27;
/// C: `T_ue` — terminfo `exit_underline_mode`.
pub(crate) const T_UE: usize = 28;
/// C: `T_vb` — terminfo `flash_screen`.
pub(crate) const T_VB: usize = 29;
/// C: `T_DC` — terminfo `parm_dch`.
pub(crate) const T_DC: usize = 30;
/// C: `T_DO` — terminfo `parm_down_cursor`.
pub(crate) const T_DO: usize = 31;
/// C: `T_IC` — terminfo `parm_ich`.
pub(crate) const T_IC: usize = 32;
/// C: `T_LE` — terminfo `parm_left_cursor`.
pub(crate) const T_LE: usize = 33;
/// C: `T_RI` — terminfo `parm_right_cursor`.
pub(crate) const T_RI: usize = 34;
/// C: `T_UP` — terminfo `parm_up_cursor`.
pub(crate) const T_UP: usize = 35;
/// C: `T_kh` — terminfo `key_home`.
pub(crate) const T_KH: usize = 36;
/// C: `T_at7` — terminfo `key_end`.
pub(crate) const T_AT7: usize = 37;
/// C: `T_kD` — terminfo `key_dc`.
pub(crate) const T_KD_DEL: usize = 38;
/// C: `T_str` — the number of string capabilities.
pub(crate) const T_STR: usize = 39;

// Flag/numeric-capability table indices. C: the `#define T_xx` run
// interleaved with `tval[]`.
/// C: `T_am` — terminfo `auto_right_margin`.
pub(crate) const T_AM: usize = 0;
/// C: `T_pt` — physical tabs. **No terminfo counterpart**; always 0.
pub(crate) const T_PT: usize = 1;
/// C: `T_li` — terminfo `lines`.
pub(crate) const T_LI: usize = 2;
/// C: `T_co` — terminfo `columns`.
pub(crate) const T_CO: usize = 3;
/// C: `T_km` — terminfo `has_meta_key`.
pub(crate) const T_KM: usize = 4;
/// C: `T_xt` — terminfo `dest_tabs_magic_smso`.
pub(crate) const T_XT: usize = 5;
/// C: `T_xn` — terminfo `eat_newline_glitch`.
pub(crate) const T_XN: usize = 6;
/// C: `T_MT` — a termcap-only meta flag. **No terminfo counterpart**;
/// always 0.
pub(crate) const T_MT: usize = 7;
/// C: `T_val` — the number of flag/numeric capabilities.
pub(crate) const T_VAL: usize = 8;

// [spec:libedit:def:terminal.termcapstr]
/// One row of the string-capability table: 39 rows, in the order the
/// `T_*` indices define.
pub struct Termcapstr {
    /// The capability's name. Termcap two-letter code in the C
    /// (`"al"`, `"bl"`, …), terminfo capname here.
    pub name: &'static str,
    /// Human-readable description, shown by `telltc`.
    pub long_name: &'static str,
}

// [spec:libedit:def:terminal.termcapval]
/// One row of the flag/numeric-capability table: 8 rows, in the order the
/// `T_*` indices define. Structurally identical to [`Termcapstr`]; the C
/// declares it separately and so does this.
pub struct Termcapval {
    /// Termcap two-letter code in the C (`"am"`, `"pt"`, …), terminfo capname
    /// here. `MT`, `pt` and `xt` have no clean terminfo counterpart and
    /// are resolved per capability during the port.
    pub name: &'static str,
    /// Human-readable description, shown by `telltc`.
    pub long_name: &'static str,
}

/// Table-literal shorthand for [`Termcapstr`]: 39 braced literals read as
/// noise where the C's are one line each.
const fn cs(name: &'static str, long_name: &'static str) -> Termcapstr {
    Termcapstr { name, long_name }
}

/// Table-literal shorthand for [`Termcapval`], as [`cs`].
const fn cv(name: &'static str, long_name: &'static str) -> Termcapval {
    Termcapval { name, long_name }
}

/// C: `static const struct termcapstr tstr[]`.
///
/// The C's trailing `{ NULL, NULL }` sentinel is the end of the array here.
/// `long_name` is libedit's own wording and is reproduced byte for byte —
/// `telltc` prints it. `name` is the terminfo capname, replacing the C's
/// termcap code; `sem:terminal.terminal-telltc-fn` records that this is a
/// deliberate, user-visible substitution.
static TSTR: [Termcapstr; T_STR] = [
    cs("il1", "add new blank line"),
    cs("bel", "audible bell"),
    cs("ed", "clear to bottom"),
    cs("el", "clear to end of line"),
    cs("hpa", "cursor to horiz pos"),
    cs("clear", "clear screen"),
    cs("dch1", "delete a character"),
    cs("dl1", "delete a line"),
    cs("smdc", "start delete mode"),
    cs("rmdc", "end delete mode"),
    cs("rmir", "end insert mode"),
    cs("fsl", "cursor from status line"),
    cs("home", "home cursor"),
    cs("ich1", "insert character"),
    cs("smir", "start insert mode"),
    cs("ip", "insert padding"),
    cs("kcud1", "sends cursor down"),
    cs("kcub1", "sends cursor left"),
    cs("kcuf1", "sends cursor right"),
    cs("kcuu1", "sends cursor up"),
    cs("bold", "begin bold"),
    cs("sgr0", "end attributes"),
    cs("cuf1", "non destructive space"),
    cs("rmso", "end standout"),
    cs("smso", "begin standout"),
    cs("tsl", "cursor to status line"),
    cs("cuu1", "cursor up one"),
    cs("smul", "begin underline"),
    cs("rmul", "end underline"),
    cs("flash", "visible bell"),
    cs("dch", "delete multiple chars"),
    cs("cud", "cursor down multiple"),
    cs("ich", "insert multiple chars"),
    cs("cub", "cursor left multiple"),
    cs("cuf", "cursor right multiple"),
    cs("cuu", "cursor up multiple"),
    cs("khome", "send cursor home"),
    cs("kend", "send cursor end"),
    cs("kdch1", "send cursor delete"),
];

/// C: `static const struct termcapval tval[]`.
///
/// `pt` and `MT` keep the C's termcap spelling because terminfo has no
/// counterpart for either; neither string is a terminfo boolean capname, so
/// [`tgetflag`] misses and the slot stays 0 (ERR-terminal-61,
/// ERR-terminal-62). `xt` needs no translation: termcap `xt` and terminfo
/// `dest_tabs_magic_smso` share the capname, which is the mapping ncurses
/// already makes.
static TVAL: [Termcapval; T_VAL] = [
    cv("am", "has automatic margins"),
    cv("pt", "has physical tabs"),
    cv("lines", "Number of lines"),
    cv("cols", "Number of columns"),
    cv("km", "Has meta key"),
    cv("xt", "Tab chars destructive"),
    cv("xenl", "newline ignored at right margin"),
    cv("MT", "Has meta key"),
];

/// Which of the two capability tables [`resolve_cap`] is searching.
#[derive(Clone, Copy)]
enum CapTable {
    /// [`TSTR`], whose indices are the `T_AL` … `T_KD_DEL` constants.
    Str,
    /// [`TVAL`], whose indices are the `T_AM` … `T_MT` constants.
    Val,
}

/// Resolves a name a *user* typed to an index in one of the capability
/// tables, accepting either spelling.
///
/// libedit's own tables are keyed by termcap two-letter code — `co`, `li`,
/// `cl` — and this port's by terminfo capname — `cols`, `lines`, `clear`.
/// That substitution is deliberate and `sem:terminal.terminal-telltc-fn`
/// records it, because `telltc` PRINTS these names and printing terminfo
/// names from a port that reads terminfo is the honest thing to do.
///
/// It is the wrong answer for input. `settc` and `echotc` take a name
/// somebody typed, at a prompt or in a `.editrc`, and what people type is
/// termcap: that is what libedit's own documentation shows and what every
/// `.editrc` already on disk contains. Before this, `settc co 132` — a line
/// straight out of the manual — was answered with "Bad capability".
///
/// So the capname is tried first, then the argument is retried as a termcap
/// code through [`nshterm`], whose mapping is generated from ncurses'
/// `include/Caps`. Accepting both means a `.editrc` written against this port
/// keeps working as well as one written against libedit, and it costs a
/// linear scan of ~500 entries on a path that runs once per line of a
/// configuration file.
///
/// The bytes are compared directly first, so a capability name that is not
/// valid UTF-8 still matches a table entry exactly as it did before; only the
/// termcap fallback needs a `str`.
///
/// The table is named rather than handed in as a lookup closure because the
/// three entry points that resolve a user's name — `settc`, `gettc`,
/// `echotc` — have to agree on what a name means, and `gettc` was left behind
/// twice while each of them carried its own scan. With the scan in here there
/// is nowhere else for one to live.
fn resolve_cap(table: CapTable, what: &[u8]) -> Option<usize> {
    let find = |n: &[u8]| match table {
        CapTable::Str => TSTR.iter().position(|t| t.name.as_bytes() == n),
        CapTable::Val => TVAL.iter().position(|t| t.name.as_bytes() == n),
    };
    if let Some(i) = find(what) {
        return Some(i);
    }
    let code = std::str::from_utf8(what).ok()?;
    let capname = nshterm::parser::names::capname_for_termcap(code)?;
    find(capname.as_bytes())
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
// Host facilities this layer reaches through the platform crate.
// ---------------------------------------------------------------------------

// The two kernel operations `terminal.c` performs.
//
// `plan/decisions/platform-layer.md` put both in `nshedit-plat`, so the stub
// module that used to stand here — with `ioctl` and `sigprocmask` reporting a
// permanent failure, and its own third copy of `sigset_t` — is gone. The
// `ioctl` is rustix's; the mask calls are libc's, because rustix declines the
// signal family on principle.
use nshedit_plat::signal::{self, SigSet, signo};
use nshedit_plat::termios::window_size;

/// `sigemptyset(&nset)`, `sigaddset(&nset, SIGWINCH)`,
/// `sigprocmask(SIG_BLOCK, &nset, &oset)`. `None` is the C's -1.
pub(crate) fn block_sigwinch() -> Option<SigSet> {
    signal::sigmask_block_one(signo::SIGWINCH)
}

/// `sigprocmask(SIG_SETMASK, oset, NULL)`. `false` is the C's -1, which every
/// caller here discards, as the C does.
pub(crate) fn set_sigmask(oset: &SigSet) -> bool {
    signal::sigmask_set(oset)
}

/// The C reads a `wchar_t *` to its first `L'\0'`; a slice also has an end.
/// Whichever comes first wins, so a caller may pass either convention.
fn wcs(s: &[u32]) -> &[u32] {
    &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
}

/// C: `wcscmp(a, b) == 0`.
fn wcs_eq(a: &[u32], b: &[u32]) -> bool {
    wcs(a) == wcs(b)
}

/// C: `Str(a)` — the capability string, or NULL when absent.
///
/// The stored value carries a trailing NUL that is not part of the
/// capability; see [`terminal_alloc_bytes`]. This strips it, so every reader
/// in this file sees exactly the bytes the C's `strlen` would measure.
fn cap_str(el: &EditLine, idx: usize) -> Option<&[u8]> {
    match el.el_terminal.t_str.get(idx) {
        Some(Some(v)) if !v.is_empty() => Some(&v[..v.len() - 1]),
        _ => None,
    }
}

/// C: `GoodStr(a)` — the slot is neither NULL nor the empty string. The C's
/// `terminal_alloc` stores NULL for an empty capability, so a non-NULL slot
/// is always non-empty; the second half of the test is kept anyway, as the C
/// keeps it.
fn good_str(el: &EditLine, idx: usize) -> bool {
    cap_str(el, idx).is_some_and(|s| !s.is_empty())
}

/// C: `Str(a)`, copied out. Every emit site needs the bytes while also
/// borrowing the `EditLine` mutably to write them, which is the one shape
/// the C gets for free and Rust does not.
fn cap_owned(el: &EditLine, idx: usize) -> Option<Vec<u8>> {
    cap_str(el, idx).map(<[u8]>::to_vec)
}

/// C: `Val(a)`.
fn val(el: &EditLine, idx: usize) -> i32 {
    el.el_terminal.t_val.get(idx).copied().unwrap_or(0)
}

/// C: `Val(a) = v`.
fn set_val(el: &mut EditLine, idx: usize, v: i32) {
    if let Some(slot) = el.el_terminal.t_val.get_mut(idx) {
        *slot = v;
    }
}

/// C: `terminal_tputs(el, Str(a), n)` where the capability is present.
fn tputs_str(el: &mut EditLine, idx: usize, affcnt: i32) {
    if let Some(cap) = cap_owned(el, idx) {
        tputs_cap(el, &cap, affcnt);
    }
}

/// Emit an already-expanded capability. Capability values are bytes and
/// [`terminal_tputs`]'s rule-given signature takes `&str`, so this routes the
/// UTF-8 case — every ASCII capability, which is nearly all of them — through
/// the annotated function and the rest through its byte body.
fn tputs_cap(el: &mut EditLine, cap: &[u8], affcnt: i32) {
    match std::str::from_utf8(cap) {
        Ok(s) => terminal_tputs(el, s, affcnt),
        Err(_) => terminal_tputs_bytes(el, cap, affcnt),
    }
}

/// C: `terminal_tputs(el, tgoto(Str(a), x, y), n)`.
fn tputs_goto(el: &mut EditLine, idx: usize, col: i32, row: i32, affcnt: i32) {
    if let Some(cap) = cap_owned(el, idx) {
        let expanded = tgoto(&cap, col, row);
        tputs_cap(el, &expanded, affcnt);
    }
}

/// C: `wcstol(s, &ep, 10)`, returning the value and the number of wide
/// characters consumed — the C's `ep - s`, which its callers test against the
/// string's end.
fn wcstol10(s: &[u32]) -> (i64, usize) {
    let cs = locale::charset();
    let mut i = 0usize;
    while i < s.len() && locale::iswspace(cs, s[i]) {
        i += 1;
    }
    let neg = match s.get(i) {
        Some(&c) if c == u32::from(b'-') => {
            i += 1;
            true
        }
        Some(&c) if c == u32::from(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let digits = i;
    let mut acc: i64 = 0;
    while let Some(&c) = s.get(i) {
        if !(u32::from(b'0')..=u32::from(b'9')).contains(&c) {
            break;
        }
        acc = acc
            .saturating_mul(10)
            .saturating_add(i64::from(c - u32::from(b'0')));
        i += 1;
    }
    if i == digits {
        // No digits: `wcstol` leaves `ep` at the original string, so the
        // caller's "anything left over" test sees the whole string.
        return (0, 0);
    }
    (if neg { -acc } else { acc }, i)
}

/// C: `ct_encode_string(s, &el->el_scratch)`, copied out of the shared
/// buffer. A NULL return — the C's, for an unencodable string — becomes the
/// empty byte string, which is what `%s` of a NULL would print on glibc minus
/// the `(null)` marker.
fn encode(el: &mut EditLine, s: &[u32]) -> Vec<u8> {
    ct_encode_string(Some(s), &mut el.el_scratch).map_or_else(Vec::new, <[u8]>::to_vec)
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
    match TermInfo::from_name(name) {
        Ok(t) => {
            *entry = Some(t);
            1
        }
        // "Readable, but nothing for this type." `TERM` unset cannot arise —
        // `terminal_set` always passes a name — but it means the same thing.
        Err(nshterm::Error::TerminfoEntryNotFound | nshterm::Error::TermUnset) => {
            *entry = None;
            0
        }
        // "The database itself cannot be read." Note that `term`'s
        // `from_name` *skips* I/O errors while searching and then reports
        // `TerminfoEntryNotFound`, so an unreadable database reaches the 0
        // arm above and only a malformed entry lands here. libedit's only use
        // of the distinction is which of two diagnostics it prints before
        // installing dumb-terminal defaults either way.
        Err(_) => {
            *entry = None;
            -1
        }
    }
    // On failure no entry is loaded, so the three lookups below report
    // "absent" for everything — which is what the C's callers see once
    // `tgetent` has failed.
}

// [spec:libedit:def:terminal.tgetflag-fn]
// [spec:libedit:sem:terminal.tgetflag-fn]
/// Boolean capability lookup: 1 if `entry` defines `name`, 0 if it does not,
/// if `name` is not a capability at all, or if no entry is loaded. There is
/// no error return. `name` is the terminfo name, not the C's termcap code.
pub(crate) fn tgetflag(entry: Option<&TermInfo>, name: &str) -> i32 {
    // A name that is not a terminfo capname simply misses, which is how `pt`
    // and `MT` stay permanently false (ERR-terminal-61, ERR-terminal-62) —
    // the same answer ncurses gives for them.
    i32::from(entry.is_some_and(|e| e.bools.get(name).copied().unwrap_or(false)))
}

// [spec:libedit:def:terminal.tgetnum-fn]
// [spec:libedit:sem:terminal.tgetnum-fn]
/// Numeric capability lookup. Returns -1, *not* 0, when the capability is
/// absent, cancelled, or no entry is loaded: `terminal_set` reads that -1 as
/// "absent" and clamps it to the 80x24 default.
pub(crate) fn tgetnum(entry: Option<&TermInfo>, name: &str) -> i32 {
    // `term` drops cancelled and absent numbers alike, so both arrive here as
    // a missing key.
    match entry.and_then(|e| e.numbers.get(name).copied()) {
        Some(n) => i32::try_from(n).unwrap_or(i32::MAX),
        None => -1,
    }
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
    entry.and_then(|e| e.strings.get(name).cloned())
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
    // The order fix (ERR-terminal-63). `tgoto`'s arguments are (column, row)
    // and the historical implementation substitutes the *row* first, then the
    // column; terminfo's `cursor_address` is `%p1` = row, `%p2` = column. So
    // `row` becomes `%p1` and `col` becomes `%p2`, and the convention this
    // function exposes is terminfo's. The only path where both parameters are
    // meaningful is `terminal_echotc`'s two-argument form; every internal
    // caller passes one value twice.
    let params = [Param::Number(row), Param::Number(col)];
    let mut vars = Variables::new();
    let mut out = Vec::with_capacity(cap.len());
    let mut seg = 0usize;
    let mut i = 0usize;

    // `term`'s expander enters a delay state on `$` and skips to the next
    // `>`, discarding the padding `tputs` exists to realise — and it does
    // that for a bare `$` too. So the string is cut at every `$`: the runs
    // between are expanded, the padding runs are copied through untouched.
    while i < cap.len() {
        if cap[i] != b'$' {
            i += 1;
            continue;
        }
        let run_end = if cap.get(i + 1) == Some(&b'<') {
            cap[i + 2..]
                .iter()
                .position(|&b| b == b'>')
                .map(|p| i + 2 + p + 1)
        } else {
            None
        };
        expand_into(&mut out, &cap[seg..i], &params, &mut vars);
        match run_end {
            Some(end) => {
                out.extend_from_slice(&cap[i..end]);
                i = end;
            }
            // A `$` not opening a run, and an unterminated `$<`, are literal.
            None => {
                out.push(b'$');
                i += 1;
            }
        }
        seg = i;
    }
    expand_into(&mut out, &cap[seg..], &params, &mut vars);
    out
}

/// One expandable run of [`tgoto`]'s input.
///
/// A run that will not expand is emitted verbatim. The C's `tgoto` has no
/// error return either — it returns the literal `"OOPS"` only for a NULL
/// capability, and copies anything it does not understand straight through —
/// so passing the bytes on is the closer answer than dropping them.
///
/// Splitting does cost one thing: `%i` and the `%P`/`%g` variables apply
/// within a run, and `vars` is threaded across runs but the parameters are
/// re-supplied unincremented to each. No capability puts padding in the
/// middle of a `%` sequence, so no in-tree string is affected.
fn expand_into(out: &mut Vec<u8>, seg: &[u8], params: &[Param], vars: &mut Variables) {
    if seg.is_empty() {
        return;
    }
    match expand(seg, params, vars) {
        Ok(v) => out.extend_from_slice(&v),
        Err(_) => out.extend_from_slice(seg),
    }
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
    // The pad character: terminfo `pad_char`, first byte, NUL when absent.
    let padc = entry
        .and_then(|e| e.strings.get("pad"))
        .and_then(|v| v.first().copied())
        .unwrap_or(0);
    // terminfo `xon_xoff`: the terminal throttles itself, so an *advisory*
    // delay is skipped entirely.
    let xon = entry.is_some_and(|e| e.bools.get("xon").copied().unwrap_or(false));
    let bps = u64::from(baud_rate(baud));

    let mut i = 0usize;
    while i < cap.len() {
        if cap[i] != b'$' || cap.get(i + 1) != Some(&b'<') {
            // Everything outside a run is verbatim, including a `$` that does
            // not open one.
            let _ = out.write_all(&cap[i..i + 1]);
            i += 1;
            continue;
        }
        let Some(close) = cap[i + 2..]
            .iter()
            .position(|&b| b == b'>')
            .map(|p| i + 2 + p)
        else {
            // Unterminated `$<`: verbatim.
            let _ = out.write_all(&cap[i..i + 1]);
            i += 1;
            continue;
        };
        let Some((tenths, per_line, mandatory)) = parse_delay(&cap[i + 2..close]) else {
            // Malformed body: not a delay, so the whole run is verbatim.
            let _ = out.write_all(&cap[i..=close]);
            i = close + 1;
            continue;
        };
        i = close + 1;

        // Step 1: tenths of a millisecond, per affected line if `*`.
        let mut delay = u64::from(tenths);
        if per_line {
            // `affcnt` is taken as given: a caller passing 0 zeroes the
            // delay, which `terminal_echotc`'s two-argument form can do.
            delay = delay.saturating_mul(u64::try_from(affcnt).unwrap_or(0));
        }
        // Step 2: an advisory delay on a flow-controlled terminal is skipped.
        if !mandatory && xon {
            continue;
        }
        // Step 3: ten bits per transmitted character, so `D` tenths of a
        // millisecond costs `D * baud / 100000` characters. 5 ms at 9600 baud
        // is 4.8; step 4 says to pick a rounding and state it — this
        // truncates, matching ncurses.
        let count = delay.saturating_mul(bps) / 100_000;
        // A zero or unknown line speed makes `bps` 0 and emits nothing, which
        // is also what ncurses does with its unset `ospeed`.
        for _ in 0..count {
            let _ = out.write_all(&[padc]);
        }
    }
    0
}

/// The body of a `$<...>` run: `(tenths of a millisecond, `*`, `/`)`.
///
/// Decimal digits with at most one fractional digit after a `.`, then `*` and
/// `/` in either order. Anything else is not a delay.
fn parse_delay(body: &[u8]) -> Option<(u32, bool, bool)> {
    let mut i = 0usize;
    let mut ms: u32 = 0;
    let start = i;
    while i < body.len() && body[i].is_ascii_digit() {
        ms = ms
            .saturating_mul(10)
            .saturating_add(u32::from(body[i] - b'0'));
        i += 1;
    }
    if i == start {
        return None;
    }
    let mut tenths = ms.saturating_mul(10);
    if body.get(i) == Some(&b'.') {
        i += 1;
        // "an optional single fractional digit".
        let d = body.get(i).copied().filter(u8::is_ascii_digit)?;
        tenths = tenths.saturating_add(u32::from(d - b'0'));
        i += 1;
    }
    let mut per_line = false;
    let mut mandatory = false;
    while let Some(&c) = body.get(i) {
        match c {
            b'*' if !per_line => per_line = true,
            b'/' if !mandatory => mandatory = true,
            _ => return None,
        }
        i += 1;
    }
    Some((tenths, per_line, mandatory))
}

/// Decode `el_tty.t_speed` — a `speed_t`, which on Linux is the encoded `B*`
/// constant and not a baud number — into bits per second.
///
/// An unrecognised encoding is 0, which [`tputs`] treats as "emit no
/// padding". `B0` (hang up) is 0 for the same reason.
fn baud_rate(speed: SpeedT) -> u32 {
    match speed {
        1 => 50,
        2 => 75,
        3 => 110,
        4 => 134,
        5 => 150,
        6 => 200,
        7 => 300,
        8 => 600,
        9 => 1200,
        10 => 1800,
        11 => 2400,
        12 => 4800,
        13 => 9600,
        14 => 19200,
        15 => 38400,
        // `CBAUDEX`: the extended range, 0o010001 upwards.
        4097 => 57600,
        4098 => 115_200,
        4099 => 230_400,
        4100 => 460_800,
        4101 => 500_000,
        4102 => 576_000,
        4103 => 921_600,
        4104 => 1_000_000,
        4105 => 1_152_000,
        4106 => 1_500_000,
        4107 => 2_000_000,
        4108 => 2_500_000,
        4109 => 3_000_000,
        4110 => 3_500_000,
        4111 => 4_000_000,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// libedit's own terminal layer.
// ---------------------------------------------------------------------------

// [spec:libedit:def:terminal.terminal-setflags-fn]
// [spec:libedit:sem:terminal.terminal-setflags-fn]
fn terminal_setflags(el: &mut EditLine) {
    let mut flags = 0i32;

    // The tty layer has to report hardware tab expansion before the two
    // capability values are consulted at all. On the first initialisation
    // pass `t_tabs` is still 0, so the flag starts clear.
    if el.el_tty.t_tabs != 0 && val(el, T_PT) != 0 && val(el, T_XT) == 0 {
        flags |= TERM_CAN_TAB;
    }
    // T_PT is permanently 0 (ERR-terminal-61), so TERM_CAN_TAB never sets on
    // a terminfo system. That is what the C already does through ncurses;
    // `plan/decisions/conformance-policy.md` names this as one of the six
    // forks defaulting to reproduce.

    if val(el, T_KM) != 0 || val(el, T_MT) != 0 {
        flags |= TERM_HAS_META;
    }
    if good_str(el, T_CE) {
        flags |= TERM_CAN_CEOL;
    }
    if good_str(el, T_DC1) || good_str(el, T_DC) {
        flags |= TERM_CAN_DELETE;
    }
    if good_str(el, T_IM) || good_str(el, T_IC1) || good_str(el, T_IC) {
        flags |= TERM_CAN_INSERT;
    }
    if good_str(el, T_UP1) || good_str(el, T_UP) {
        flags |= TERM_CAN_UP;
    }
    if val(el, T_AM) != 0 {
        flags |= TERM_HAS_AUTO_MARGINS;
    }
    if val(el, T_XN) != 0 {
        flags |= TERM_HAS_MAGIC_MARGINS;
    }

    // Two independent byte-for-byte comparisons. The first can clear the bit
    // — a no-op, since nothing above sets it — and the second can only set
    // it, so a match on either pair is enough.
    if good_str(el, T_ME) && good_str(el, T_UE) {
        if cap_str(el, T_ME) == cap_str(el, T_UE) {
            flags |= TERM_CAN_ME;
        }
    } else {
        flags &= !TERM_CAN_ME;
    }
    if good_str(el, T_ME) && good_str(el, T_SE) && cap_str(el, T_ME) == cap_str(el, T_SE) {
        flags |= TERM_CAN_ME;
    }

    // The C's DEBUG_SCREEN warnings are not part of a normal build and the
    // rule does not require them.
    el.el_terminal.t_flags = flags;
}

// [spec:libedit:def:terminal.terminal-init-fn]
// [spec:libedit:sem:terminal.terminal-init-fn]
pub(crate) fn terminal_init(el: &mut EditLine) -> i32 {
    // Steps 1-6. Every C allocation is `el_calloc` and every failure path
    // goes to `terminal_end`; a Rust allocation failure aborts instead, so
    // the -1 returns and the cleanup path are unreachable and are not
    // written out. What survives is the *shape*: `t_buf` becomes non-empty,
    // which is the C's "the terminal subsystem is up" marker that
    // `terminal_bind_arrow` tests for NULL.
    el.el_terminal.t_buf = vec![0; TC_BUFSIZE];
    // Never read. The C's `tgetent` copies the raw termcap entry here and
    // libedit never looks; `sem:terminal.tgetent-fn` retires the buffer with
    // the parameter. Allocated anyway so `terminal_end` has the same work to
    // undo, and zeroed so `terminal_set`'s `memset` is already true.
    el.el_terminal.t_cap = vec![0; TC_BUFSIZE];
    el.el_terminal.t_fkey = (0..A_K_NKEYS)
        .map(|_| FunckeyT {
            // The C's `calloc`ed table: NULL name, capability index 0, and
            // type XK_CMD (0, not XK_NOD).
            name: None,
            key: 0,
            fun: KeymacroValueT::Cmd(0),
            r#type: XK_CMD,
        })
        .collect();
    // The string pool is not reproduced (ERR-terminal-01, ERR-terminal-02),
    // so the high-water mark never moves off 0.
    el.el_terminal.t_loc = 0;
    el.el_terminal.t_str = vec![None; T_STR];
    el.el_terminal.t_val = vec![0; T_VAL];

    // ERR-terminal-23, disposition `fix`: the C runs step 8 after step 7, so
    // its first `terminal_bind_arrow` — reached from inside `terminal_set` —
    // sees an all-zero function-key table, and that is inert only because
    // `map_init` has not run yet and the "is the key map built" guard fires
    // first. The defaults go in before the capability load here.
    terminal_init_arrow(el);

    // Step 7. The result is discarded: a missing or unknown terminal type
    // does not make initialisation fail, because dumb-terminal defaults are
    // installed in that case.
    let _ = terminal_set(el, None);

    0
}

// [spec:libedit:def:terminal.terminal-end-fn]
// [spec:libedit:sem:terminal.terminal-end-fn]
pub(crate) fn terminal_end(el: &mut EditLine) {
    el.el_terminal.t_buf = Vec::new();
    el.el_terminal.t_cap = Vec::new();
    el.el_terminal.t_loc = 0;
    el.el_terminal.t_str = Vec::new();
    el.el_terminal.t_val = Vec::new();
    el.el_terminal.t_fkey = Vec::new();
    // No C counterpart, and dropped with the rest: the loaded entry is this
    // port's replacement for the termcap library's global state.
    el.el_terminal.t_entry = None;
    terminal_free_display(el);
    // `t_name`, `t_size` and `t_flags` are deliberately left alone, as the C
    // leaves them.
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
    terminal_alloc_bytes(el, t, cap.map(str::as_bytes));
}

/// Byte form of [`terminal_alloc`], which is what the capability loader
/// actually has: `term` hands back raw bytes and a capability may carry a
/// byte above 0x7f, so it cannot cross the rule-given `&str` parameter.
///
/// Steps 2 to 7 of the rule are the string pool and are deliberately gone.
/// The append bound ignores the string's length and overflows `t_buf`
/// (ERR-terminal-01, `define`), and the compaction rewrites the buffer
/// without repointing any slot, silently corrupting every retained
/// capability (ERR-terminal-02, `define`). Both are undefined behaviour and
/// both disappear with the pool; the rule directs exactly this. The
/// "Out of termcap string space." diagnostic goes with them — it is only
/// reachable from the compaction path.
///
/// The stored value carries a trailing NUL. Nothing in this file wants it
/// ([`cap_str`] strips it), but `terminal_gettc` hands the slot out as a
/// `char *` across the C ABI, and the C's slots are NUL-terminated strings.
fn terminal_alloc_bytes(el: &mut EditLine, t: usize, cap: Option<&[u8]>) {
    let Some(slot) = el.el_terminal.t_str.get_mut(t) else {
        return;
    };
    // Step 1, plus the C's `strlen`: a NUL ends the capability.
    let cap = cap.unwrap_or_default();
    let cap = &cap[..cap.iter().position(|&b| b == 0).unwrap_or(cap.len())];
    if cap.is_empty() {
        *slot = None;
        return;
    }
    let mut v = Vec::with_capacity(cap.len() + 1);
    v.extend_from_slice(cap);
    v.push(0);
    *slot = Some(v);
}

// [spec:libedit:def:terminal.terminal-rebuffer-display-fn]
// [spec:libedit:sem:terminal.terminal-rebuffer-display-fn]
fn terminal_rebuffer_display(el: &mut EditLine) -> i32 {
    terminal_free_display(el);

    // The assignment that gives `t_size` its correct meaning: `.h` columns,
    // `.v` rows.
    el.el_terminal.t_size.h = val(el, T_CO);
    el.el_terminal.t_size.v = val(el, T_LI);

    if terminal_alloc_display(el) == -1 {
        return -1;
    }
    0
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
    let rows = usize::try_from(el.el_terminal.t_size.v).unwrap_or(0);
    let cols = usize::try_from(el.el_terminal.t_size.h).unwrap_or(0);
    // Rows are one cell longer than the column count so a full-width line can
    // still carry a terminating cell. Every cell starts at 0, so every row
    // initially reads as an empty line.
    //
    // The C's step 3 stores NULL at index `t_size.v`; that terminator is the
    // `Vec`'s own length here, which is what makes freeing independent of the
    // size current at allocation.
    Some(vec![vec![0u32; cols + 1]; rows])
}

// [spec:libedit:def:terminal.terminal-free-buffer-fn]
// [spec:libedit:sem:terminal.terminal-free-buffer-fn]
/// C: `static void terminal_free_buffer(wint_t ***bp)` — frees the rows and
/// the row array and NULLs the caller's field. `el_display` and
/// `el_vdisplay` are owning `Vec`s, so "NULL" is the empty `Vec`, and the
/// `Vec` itself has to be the parameter: a slice cannot be emptied.
#[allow(clippy::ptr_arg)]
fn terminal_free_buffer(bp: &mut Vec<Vec<u32>>) {
    // The C's steps 2-4 — clear the caller's field first, then release — are
    // one move here, and freeing stays idempotent because an already-empty
    // `Vec` has nothing to drop.
    *bp = Vec::new();
}

// [spec:libedit:def:terminal.terminal-alloc-display-fn]
// [spec:libedit:sem:terminal.terminal-alloc-display-fn]
fn terminal_alloc_display(el: &mut EditLine) -> i32 {
    // Both `None` arms are unreachable: a Rust allocation failure aborts, so
    // the C's half-built-pair cleanup can never run. The shape is kept
    // because the rule describes it and because `terminal_rebuffer_display`
    // propagates the -1.
    let Some(display) = terminal_alloc_buffer(el) else {
        terminal_free_display(el);
        return -1;
    };
    el.el_display = display;
    let Some(vdisplay) = terminal_alloc_buffer(el) else {
        terminal_free_display(el);
        return -1;
    };
    el.el_vdisplay = vdisplay;
    0
}

// [spec:libedit:def:terminal.terminal-free-display-fn]
// [spec:libedit:sem:terminal.terminal-free-display-fn]
fn terminal_free_display(el: &mut EditLine) {
    terminal_free_buffer(&mut el.el_display);
    terminal_free_buffer(&mut el.el_vdisplay);
}

// [spec:libedit:def:terminal.terminal-move-to-line-fn]
// [spec:libedit:sem:terminal.terminal-move-to-line-fn]
pub(crate) fn terminal_move_to_line(el: &mut EditLine, where_: i32) {
    if where_ == el.el_cursor.v {
        return;
    }
    // Note this bound is `>=`, unlike the `>` used by
    // `terminal_move_to_char`.
    if where_ >= el.el_terminal.t_size.v {
        return;
    }

    let del = where_ - el.el_cursor.v;
    if del > 0 {
        // The parameterised down capability is deliberately not used: some
        // terminals misbehave when the destination is below the bottom of the
        // screen.
        for _ in 0..del {
            terminal__putc(el, u32::from(b'\n'));
        }
        // The tty turns each `\n` into CR LF, so the cursor also returns to
        // column 0.
        el.el_cursor.h = 0;
    } else if good_str(el, T_UP) && (-del > 1 || !good_str(el, T_UP1)) {
        tputs_goto(el, T_UP, -del, -del, -del);
    } else if good_str(el, T_UP1) {
        for _ in 0..-del {
            tputs_str(el, T_UP1, 1);
        }
    }
    // ERR-terminal-26, disposition `reproduce`: when neither up capability
    // exists nothing is emitted, yet the model still moves.
    el.el_cursor.v = where_;
}

// [spec:libedit:def:terminal.terminal-move-to-char-fn]
// [spec:libedit:sem:terminal.terminal-move-to-char-fn]
pub(crate) fn terminal_move_to_char(el: &mut EditLine, where_: i32) {
    // The C's `mc_again` label; step 7b restarts here.
    loop {
        if where_ == el.el_cursor.h {
            return;
        }
        // Strictly greater, so a column equal to `t_size.h` — one past the
        // last real column — is accepted.
        if where_ > el.el_terminal.t_size.h {
            return;
        }
        if where_ == 0 {
            terminal__putc(el, u32::from(b'\r'));
            el.el_cursor.h = 0;
            return;
        }
        let del = where_ - el.el_cursor.h;

        // C: `del < -4 || del > 4` — the distance exceeds 4 in either
        // direction.
        if !(-4..=4).contains(&del) && good_str(el, T_CH) {
            // The value is passed twice, as the C does; a one-parameter
            // capability consumes only the row.
            tputs_goto(el, T_CH, where_, where_, where_);
        } else if del > 0 {
            if del > 4 && good_str(el, T_RI) {
                tputs_goto(el, T_RI, del, del, del);
            } else {
                if el.el_terminal.t_flags & TERM_CAN_TAB != 0 {
                    // ERR-terminal-25, disposition `fix`: the C compares
                    // `el_cursor.h & 0370` against `where & ~0x7` and indexes
                    // the display at `where & 0370`. `0370` also clears every
                    // bit above bit 7, so both are wrong past column 255.
                    // "Clear the low three bits", consistently.
                    let cur_stop = el.el_cursor.h & !0x7;
                    let tgt_stop = where_ & !0x7;
                    let stop_cell = el
                        .el_display
                        .get(usize::try_from(el.el_cursor.v).unwrap_or(0))
                        .and_then(|r| r.get(usize::try_from(tgt_stop).unwrap_or(0)))
                        .copied();
                    // A cell the display cannot supply counts as "not the
                    // interior of a double-width character"; the C reads it
                    // unconditionally and only an unallocated display puts it
                    // out of range.
                    if cur_stop != tgt_stop && stop_cell != Some(MB_FILL_CHAR) {
                        // One tab per stop crossed; both ends are already
                        // rounded down to a multiple of 8.
                        for _ in (cur_stop..tgt_stop).step_by(8) {
                            terminal__putc(el, u32::from(b'\t'));
                        }
                        el.el_cursor.h = tgt_stop;
                    }
                }
                // It is usually cheaper to rewrite the characters already
                // believed to be on screen than to emit a motion sequence.
                // NOTE: `terminal_overwrite` changes `el_cursor.h`.
                let row = usize::try_from(el.el_cursor.v).unwrap_or(0);
                let from = usize::try_from(el.el_cursor.h).unwrap_or(0);
                let n = usize::try_from(where_ - el.el_cursor.h).unwrap_or(0);
                // The C indexes `el_display[v][h]` for `n` cells with no
                // bounds check. Taking what the row actually holds is the
                // defined reading; every in-range request is unaffected.
                let run: Vec<u32> = el
                    .el_display
                    .get(row)
                    .and_then(|r| r.get(from..(from + n).min(r.len())))
                    .map(<[u32]>::to_vec)
                    .unwrap_or_default();
                let n = run.len();
                terminal_overwrite(el, &run, n);
            }
        } else if -del > 4 && good_str(el, T_LE) {
            tputs_goto(el, T_LE, -del, -del, -del);
        } else {
            // Compare the cost of backspacing against the cost of returning
            // to column 0 and coming back out. The C performs this in
            // unsigned arithmetic.
            let back = (-del) as u32;
            let cost = if el.el_terminal.t_flags & TERM_CAN_TAB != 0 {
                ((where_ as u32) >> 3) + ((where_ & 0o7) as u32)
            } else {
                where_ as u32
            };
            if back > cost {
                terminal__putc(el, u32::from(b'\r'));
                el.el_cursor.h = 0;
                continue;
            }
            for _ in 0..-del {
                terminal__putc(el, 0x08);
            }
        }
        break;
    }
    // ERR-terminal-26, disposition `reproduce`: unconditional, so the model
    // becomes authoritative even where nothing was emitted.
    el.el_cursor.h = where_;
}

// [spec:libedit:def:terminal.terminal-overwrite-fn]
// [spec:libedit:sem:terminal.terminal-overwrite-fn]
/// C: `libedit_private void terminal_overwrite(EditLine *el, const wchar_t
/// *cp, size_t n)`. `n` is kept alongside the slice: callers pass a buffer
/// longer than the run they mean to write.
pub(crate) fn terminal_overwrite(el: &mut EditLine, cp: &[u32], n: usize) {
    if n == 0 {
        return;
    }
    // A sanity guard, and also what catches a negative count that arrived as
    // an enormous unsigned value. The cast reproduces the C's
    // `(size_t)t_size.h`.
    if n > el.el_terminal.t_size.h as usize {
        return;
    }

    // `terminal__putc` emits nothing for MB_FILL_CHAR; incrementing on the
    // fill cells too is how the column count stays honest across
    // double-width characters.
    for i in 0..n {
        terminal__putc(el, cp.get(i).copied().unwrap_or(0));
        el.el_cursor.h += 1;
    }

    if el.el_cursor.h >= el.el_terminal.t_size.h {
        if el.el_terminal.t_flags & TERM_HAS_AUTO_MARGINS != 0 {
            el.el_cursor.h = 0;
            if el.el_cursor.v + 1 < el.el_terminal.t_size.v {
                el.el_cursor.v += 1;
            }
            if el.el_terminal.t_flags & TERM_HAS_MAGIC_MARGINS != 0 {
                // Force the wrap so the deferred state cannot confuse later
                // cursor motion. ERR-terminal-29, disposition `define`: the C
                // reads the cell as a `wint_t` and stores it into a `wchar_t`
                // before the recursive call; one 32-bit cell type carries it
                // whole here.
                let row = usize::try_from(el.el_cursor.v).unwrap_or(0);
                let col = usize::try_from(el.el_cursor.h).unwrap_or(0);
                let c = el
                    .el_display
                    .get(row)
                    .and_then(|r| r.get(col))
                    .copied()
                    .unwrap_or(0);
                if c != 0 {
                    // The recursion terminates because a single-character
                    // write cannot itself wrap unless the screen is one
                    // column wide.
                    terminal_overwrite(el, &[c], 1);
                    loop {
                        let col = usize::try_from(el.el_cursor.h).unwrap_or(0);
                        let fill = el.el_display.get(row).and_then(|r| r.get(col)).copied();
                        if fill == Some(MB_FILL_CHAR) {
                            el.el_cursor.h += 1;
                        } else {
                            break;
                        }
                    }
                } else {
                    terminal__putc(el, u32::from(b' '));
                    el.el_cursor.h = 1;
                }
            }
        } else {
            // No wrap, but the cursor stays on screen.
            el.el_cursor.h = el.el_terminal.t_size.h - 1;
        }
    }
    // Nothing is written into `el_display` here; the caller owns the model.
}

// [spec:libedit:def:terminal.terminal-deletechars-fn]
// [spec:libedit:sem:terminal.terminal-deletechars-fn]
pub(crate) fn terminal_deletechars(el: &mut EditLine, num: i32) {
    if num <= 0 {
        return;
    }
    if el.el_terminal.t_flags & TERM_CAN_DELETE == 0 {
        // The C's DEBUG_EDIT diagnostic is not part of a normal build.
        return;
    }
    if num > el.el_terminal.t_size.h {
        return;
    }
    // The cost heuristic: for a single deletion the one-character form is
    // assumed cheaper.
    if good_str(el, T_DC) && (num > 1 || !good_str(el, T_DC1)) {
        tputs_goto(el, T_DC, num, num, num);
        return;
    }
    if good_str(el, T_DM) {
        tputs_str(el, T_DM, 1);
    }
    if good_str(el, T_DC1) {
        for _ in 0..num {
            tputs_str(el, T_DC1, 1);
        }
    }
    if good_str(el, T_ED) {
        tputs_str(el, T_ED, 1);
    }
}

// [spec:libedit:def:terminal.terminal-insertwrite-fn]
// [spec:libedit:sem:terminal.terminal-insertwrite-fn]
/// C: `libedit_private void terminal_insertwrite(EditLine *el, wchar_t *cp,
/// int num)`. As with `terminal_overwrite`, `num` is the run length within
/// `cp`, not `cp`'s length.
pub(crate) fn terminal_insertwrite(el: &mut EditLine, cp: &[u32], num: i32) {
    if num <= 0 {
        return;
    }
    if el.el_terminal.t_flags & TERM_CAN_INSERT == 0 {
        return;
    }
    if num > el.el_terminal.t_size.h {
        return;
    }

    // Strategy A — parameterised insert.
    if good_str(el, T_IC) && (num > 1 || !good_str(el, T_IC1)) {
        tputs_goto(el, T_IC, num, num, num);
        // `terminal_overwrite` is what advances `el_cursor.h` and applies the
        // margin rules.
        terminal_overwrite(el, cp, usize::try_from(num).unwrap_or(0));
        return;
    }

    // Strategy B — insert mode.
    if good_str(el, T_IM) && good_str(el, T_EI) {
        tputs_str(el, T_IM, 1);
        // ERR-terminal-27, disposition `reproduce`: the column is advanced up
        // front and no wrap handling is applied at all.
        el.el_cursor.h += num;
        for i in 0..num {
            terminal__putc(el, cp.get(i as usize).copied().unwrap_or(0));
        }
        if good_str(el, T_IP) {
            // Once for the whole run, unlike strategy C.
            tputs_str(el, T_IP, 1);
        }
        tputs_str(el, T_EI, 1);
        return;
    }

    // Strategy C — one character at a time. ERR-terminal-28, disposition
    // `reproduce`: with enter-insert-mode but no exit-insert-mode and no
    // one-character insert, this degenerates into a plain overwrite.
    for i in 0..num {
        if good_str(el, T_IC1) {
            tputs_str(el, T_IC1, 1);
        }
        terminal__putc(el, cp.get(i as usize).copied().unwrap_or(0));
        el.el_cursor.h += 1;
        if good_str(el, T_IP) {
            tputs_str(el, T_IP, 1);
        }
    }
}

// [spec:libedit:def:terminal.terminal-clear-eol-fn]
// [spec:libedit:sem:terminal.terminal-clear-eol-fn]
/// The C's name, `EOL` and all, so it stays non-snake-case.
#[allow(non_snake_case)]
pub(crate) fn terminal_clear_EOL(el: &mut EditLine, num: i32) {
    if el.el_terminal.t_flags & TERM_CAN_CEOL != 0 && good_str(el, T_CE) {
        // The capability does not move the cursor, so `el_cursor.h` is left
        // alone — deliberately unlike the fallback.
        tputs_str(el, T_CE, 1);
    } else {
        for _ in 0..num {
            terminal__putc(el, u32::from(b' '));
        }
        // ERR-terminal-27, disposition `reproduce`: no wrap handling, and a
        // negative `num` moves the recorded column backwards.
        el.el_cursor.h += num;
    }
}

// [spec:libedit:def:terminal.terminal-clear-screen-fn]
// [spec:libedit:sem:terminal.terminal-clear-screen-fn]
pub(crate) fn terminal_clear_screen(el: &mut EditLine) {
    let lines = val(el, T_LI);
    if good_str(el, T_CL) {
        // A whole screen's worth of per-affected-line padding.
        tputs_str(el, T_CL, lines);
    } else if good_str(el, T_HO) && good_str(el, T_CD) {
        tputs_str(el, T_HO, lines);
        tputs_str(el, T_CD, lines);
    } else {
        // With no clearing capability at all, the best that can be done is to
        // scroll one line.
        terminal__putc(el, u32::from(b'\r'));
        terminal__putc(el, u32::from(b'\n'));
    }
    // None of the three updates `el_cursor`; the caller resynchronises,
    // normally through `re_clear_display`.
}

// [spec:libedit:def:terminal.terminal-beep-fn]
// [spec:libedit:sem:terminal.terminal-beep-fn]
pub(crate) fn terminal_beep(el: &mut EditLine) {
    if good_str(el, T_BL) {
        tputs_str(el, T_BL, 1);
    } else {
        terminal__putc(el, 0x07);
    }
}

// [spec:libedit:def:terminal.terminal-get-fn]
// [spec:libedit:sem:terminal.terminal-get-fn]
/// C: `libedit_private void terminal_get(EditLine *el, const char **term)` —
/// hands back `el_terminal.t_name` through an out-parameter, which stays one
/// here. The borrow is tied to `el` because the name is `t_name`'s own
/// storage, exactly as the C hands out its interior pointer.
pub fn terminal_get<'a>(el: &'a mut EditLine, term: &mut Option<&'a str>) {
    *term = el.el_terminal.t_name.as_deref();
}

// [spec:libedit:def:terminal.terminal-set-fn]
// [spec:libedit:sem:terminal.terminal-set-fn]
/// `term` is `None` for the C's NULL, which means "take the type from the
/// environment".
pub fn terminal_set(el: &mut EditLine, term: Option<&str>) -> i32 {
    // Step 1: block SIGWINCH so a resize cannot arrive while the capability
    // tables and the screen images are inconsistent. See `plat` for why this
    // reports failure today.
    let oset = block_sigwinch();

    // Step 2.
    let resolved = match term {
        Some(t) => t.to_string(),
        None => getenv(el, "TERM").unwrap_or_default(),
    };
    let name = if resolved.is_empty() {
        "dumb".to_string()
    } else {
        resolved
    };

    // Step 3: the Emacs inferior-shell terminal type, where line editing must
    // be off.
    if name == "emacs" {
        el.el_flags |= EDIT_DISABLED;
    }

    // Step 4. The C zeroes `t_cap` first; nothing reads it, and the terminfo
    // loader owns its own storage.
    el.el_terminal.t_cap.fill(0);
    let i = tgetent(&mut el.el_terminal.t_entry, &name);

    if i <= 0 {
        // Step 5: dumb-terminal settings.
        if i == -1 {
            el.write_errfile(b"Cannot read termcap database;\n");
        } else {
            el.write_errfile(format!("No entry for terminal type \"{name}\";\n").as_bytes());
        }
        el.write_errfile(b"using dumb terminal settings.\n");
        set_val(el, T_CO, 80);
        set_val(el, T_PT, 0);
        set_val(el, T_KM, 0);
        set_val(el, T_LI, 0);
        // A straight copy between two unrelated slots; on a freshly zeroed
        // EditLine it leaves both at 0 and is best read as "both false". The
        // margin values are deliberately left at whatever they held.
        let mt = val(el, T_MT);
        set_val(el, T_XT, mt);
        for t in 0..T_STR {
            terminal_alloc_bytes(el, t, None);
        }
    } else {
        // Step 6(a): flags, in the C's order.
        let flags = [
            (T_AM, TVAL[T_AM].name),
            (T_XN, TVAL[T_XN].name),
            (T_PT, TVAL[T_PT].name),
            (T_XT, TVAL[T_XT].name),
            (T_KM, TVAL[T_KM].name),
            (T_MT, TVAL[T_MT].name),
        ];
        for (slot, cap) in flags {
            let v = tgetflag(el.el_terminal.t_entry.as_ref(), cap);
            set_val(el, slot, v);
        }
        // Step 6(b): an absent number reads back as -1, not 0.
        let co = tgetnum(el.el_terminal.t_entry.as_ref(), TVAL[T_CO].name);
        set_val(el, T_CO, co);
        let li = tgetnum(el.el_terminal.t_entry.as_ref(), TVAL[T_LI].name);
        set_val(el, T_LI, li);
        // Step 6(c). The C's 2048-byte stack arena is gone with `tgetstr`'s
        // second parameter; the value is owned before it is interned.
        for (t, row) in TSTR.iter().enumerate() {
            let v = tgetstr(el.el_terminal.t_entry.as_ref(), row.name);
            terminal_alloc_bytes(el, t, v.as_deref());
        }
    }

    // Step 7: this is what turns the "absent" -1 into the classic 80x24
    // default, and it applies on both paths.
    if val(el, T_CO) < 2 {
        set_val(el, T_CO, 80);
    }
    if val(el, T_LI) < 1 {
        set_val(el, T_LI, 24);
    }

    // Step 8 is ERR-terminal-20, disposition `fix`: the C writes the column
    // count into `t_size.v` and the line count into `t_size.h`, the reverse
    // of the meaning used everywhere else. It is masked because step 10
    // reaches `terminal_rebuffer_display`, which overwrites both correctly
    // before anything reads them — "the port should simply not do this".

    // Step 9.
    terminal_setflags(el);

    // Step 10. The "did it change" result is ignored.
    let mut lins = 0;
    let mut cols = 0;
    let _ = terminal_get_size(el, &mut lins, &mut cols);
    if terminal_change_size(el, lins, cols) == -1 {
        // ERR-terminal-21, disposition `fix`: the C returns here *without*
        // restoring the mask, leaving SIGWINCH blocked for the rest of the
        // process. Restored on every exit path.
        if let Some(o) = oset.as_ref() {
            let _ = set_sigmask(o);
        }
        return -1;
    }
    // Step 11.
    if let Some(o) = oset.as_ref() {
        let _ = set_sigmask(o);
    }
    // Step 12.
    terminal_bind_arrow(el);
    // Step 13. The C keeps the caller's pointer without copying it; an owned
    // copy avoids inheriting that lifetime.
    el.el_terminal.t_name = Some(name);
    // Step 14. ERR-terminal-22, disposition `reproduce`: -1 even though
    // dumb-terminal defaults were installed successfully and the EditLine is
    // fully usable. The return value crosses the ABI.
    if i <= 0 { -1 } else { 0 }
}

/// C: `(el->el_getenv)(name)`.
///
/// The hook is a C-shaped `char *(*)(const char *)` and the built-in default
/// — `el.rs`'s `secure_getenv`, which owns what it returns — does not fit
/// that slot. `def:el.editline.el-getenv-fn` says the two are reconciled
/// where the hook is consulted, which is here: `None` is the default hook.
///
/// The C's NULL-hook crash (`el_set(EL_GETENV, NULL)` installs one with no
/// check) is defined as falling back to that default.
fn getenv(el: &EditLine, name: &str) -> Option<String> {
    match el.el_getenv {
        Some(f) => {
            let mut key: Vec<u8> = name.as_bytes().to_vec();
            key.push(0);
            // SAFETY: `f` is what an application installed through
            // `el_set(EL_GETENV, ...)`; `def:el.editline.el-getenv-fn` makes
            // it a C function taking one NUL-terminated name, and `key` is
            // exactly that and outlives the call.
            let p = unsafe { f(key.as_ptr().cast::<c_char>()) };
            if p.is_null() {
                return None;
            }
            // SAFETY: the hook's contract is the C one — it returns NULL or a
            // NUL-terminated string that outlives the call.
            let s = unsafe { std::ffi::CStr::from_ptr(p) };
            // `t_name` is a `String`, so a non-UTF-8 `TERM` is replaced
            // rather than carried; terminal type names are ASCII.
            Some(s.to_string_lossy().into_owned())
        }
        None => crate::el::secure_getenv(name).map(|v| v.to_string_lossy().into_owned()),
    }
}

// [spec:libedit:def:terminal.terminal-get-size-fn]
// [spec:libedit:sem:terminal.terminal-get-size-fn]
pub(crate) fn terminal_get_size(el: &mut EditLine, lins: &mut i32, cols: &mut i32) -> i32 {
    // Step 1: the values loaded from the terminal database or set by a
    // previous resize, and what is returned if the kernel cannot be asked.
    *cols = val(el, T_CO);
    *lins = val(el, T_LI);

    // Step 2. This is the only place the terminal layer touches the tty
    // device directly, and it queries the *input* descriptor
    // (ERR-terminal-35, disposition `reproduce`). A zero field means "the
    // kernel does not know" and leaves the seeded value in place; an ioctl
    // failure is silently ignored and errno is not inspected.
    if let Some((rows, columns)) = window_size(el.el_infd) {
        if columns != 0 {
            *cols = i32::from(columns);
        }
        if rows != 0 {
            *lins = i32::from(rows);
        }
    }
    // Step 3, TIOCGSIZE, is not compiled on this target; see `plat`.

    // Step 4: non-zero means "the size changed".
    i32::from(val(el, T_CO) != *cols || val(el, T_LI) != *lins)
}

// [spec:libedit:def:terminal.terminal-change-size-fn]
// [spec:libedit:sem:terminal.terminal-change-size-fn]
pub(crate) fn terminal_change_size(el: &mut EditLine, lins: i32, cols: i32) -> i32 {
    // Step 1. `coord_t` is two plain integers; the C copies the struct.
    let cur = CoordT {
        h: el.el_cursor.h,
        v: el.el_cursor.v,
    };

    // Step 2: degenerate sizes are clamped to the classic 80x24 default
    // rather than rejected.
    set_val(el, T_CO, if cols < 2 { 80 } else { cols });
    set_val(el, T_LI, if lins < 1 { 24 } else { lins });

    // Step 3. On -1 both images are left empty and the saved cursor is not
    // restored.
    if terminal_rebuffer_display(el) == -1 {
        return -1;
    }
    // Step 4.
    re_clear_display(el);
    // Steps 5 and 6. ERR-terminal-32, disposition `reproduce`: nothing
    // revalidates the restored cursor against the new dimensions, so after a
    // shrink it may name a position off the screen.
    el.el_cursor = cur;
    0
}

// C: the wide literals `terminal_init_arrow` and `terminal_reset_arrow`
// declare. `wchar_t` is `u32` throughout this crate, so a literal is a slice.
/// C: `L"down"`.
const A_NAME_DN: &[u32] = &[0x64, 0x6f, 0x77, 0x6e];
/// C: `L"up"`.
const A_NAME_UP: &[u32] = &[0x75, 0x70];
/// C: `L"left"`.
const A_NAME_LT: &[u32] = &[0x6c, 0x65, 0x66, 0x74];
/// C: `L"right"`.
const A_NAME_RT: &[u32] = &[0x72, 0x69, 0x67, 0x68, 0x74];
/// C: `L"home"`.
const A_NAME_HO: &[u32] = &[0x68, 0x6f, 0x6d, 0x65];
/// C: `L"end"`.
const A_NAME_EN: &[u32] = &[0x65, 0x6e, 0x64];
/// C: `L"delete"`.
const A_NAME_DE: &[u32] = &[0x64, 0x65, 0x6c, 0x65, 0x74, 0x65];

// [spec:libedit:def:terminal.terminal-init-arrow-fn]
// [spec:libedit:sem:terminal.terminal-init-arrow-fn]
fn terminal_init_arrow(el: &mut EditLine) {
    let defaults: [(usize, &'static [u32], usize, ElActionT); A_K_NKEYS] = [
        (A_K_DN, A_NAME_DN, T_KD, ED_NEXT_HISTORY),
        (A_K_UP, A_NAME_UP, T_KU, ED_PREV_HISTORY),
        (A_K_LT, A_NAME_LT, T_KL, ED_PREV_CHAR),
        (A_K_RT, A_NAME_RT, T_KR, ED_NEXT_CHAR),
        (A_K_HO, A_NAME_HO, T_KH, ED_MOVE_TO_BEG),
        (A_K_EN, A_NAME_EN, T_AT7, ED_MOVE_TO_END),
        (A_K_DE, A_NAME_DE, T_KD_DEL, ED_DELETE_NEXT_CHAR),
    ];
    for (slot, name, key, cmd) in defaults {
        let Some(arrow) = el.el_terminal.t_fkey.get_mut(slot) else {
            return;
        };
        arrow.name = Some(name);
        arrow.key = i32::try_from(key).unwrap_or(0);
        arrow.fun = KeymacroValueT::Cmd(cmd);
        arrow.r#type = XK_CMD;
    }
    // The table is only filled; nothing reaches a key map until
    // `terminal_bind_arrow` runs.
}

// C: the twelve `static const wchar_t` sequences in `terminal_reset_arrow`.
/// C: `L"\033[A"`.
const SEQ_CSI_A: &[u32] = &[0x1b, 0x5b, 0x41];
/// C: `L"\033[B"`.
const SEQ_CSI_B: &[u32] = &[0x1b, 0x5b, 0x42];
/// C: `L"\033[C"`.
const SEQ_CSI_C: &[u32] = &[0x1b, 0x5b, 0x43];
/// C: `L"\033[D"`.
const SEQ_CSI_D: &[u32] = &[0x1b, 0x5b, 0x44];
/// C: `L"\033[H"`.
const SEQ_CSI_H: &[u32] = &[0x1b, 0x5b, 0x48];
/// C: `L"\033[F"`.
const SEQ_CSI_F: &[u32] = &[0x1b, 0x5b, 0x46];
/// C: `L"\033OA"`.
const SEQ_SS3_A: &[u32] = &[0x1b, 0x4f, 0x41];
/// C: `L"\033OB"`.
const SEQ_SS3_B: &[u32] = &[0x1b, 0x4f, 0x42];
/// C: `L"\033OC"`.
const SEQ_SS3_C: &[u32] = &[0x1b, 0x4f, 0x43];
/// C: `L"\033OD"`.
const SEQ_SS3_D: &[u32] = &[0x1b, 0x4f, 0x44];
/// C: `L"\033OH"`.
const SEQ_SS3_H: &[u32] = &[0x1b, 0x4f, 0x48];
/// C: `L"\033OF"`.
const SEQ_SS3_F: &[u32] = &[0x1b, 0x4f, 0x46];

// [spec:libedit:def:terminal.terminal-reset-arrow-fn]
// [spec:libedit:sem:terminal.terminal-reset-arrow-fn]
fn terminal_reset_arrow(el: &mut EditLine) {
    // The CSI forms first, then the SS3 forms a terminal in
    // application-cursor-key mode sends. Each is bound to the *current*
    // function value and type of its slot, so a prior `terminal_set_arrow` or
    // `terminal_clear_arrow` is honoured.
    let seqs: [(&'static [u32], usize); 12] = [
        (SEQ_CSI_A, A_K_UP),
        (SEQ_CSI_B, A_K_DN),
        (SEQ_CSI_C, A_K_RT),
        (SEQ_CSI_D, A_K_LT),
        (SEQ_CSI_H, A_K_HO),
        (SEQ_CSI_F, A_K_EN),
        (SEQ_SS3_A, A_K_UP),
        (SEQ_SS3_B, A_K_DN),
        (SEQ_SS3_C, A_K_RT),
        (SEQ_SS3_D, A_K_LT),
        (SEQ_SS3_H, A_K_HO),
        (SEQ_SS3_F, A_K_EN),
    ];
    for (seq, slot) in seqs {
        add_arrow(el, seq, slot);
    }

    if el.el_map.r#type != MAP_VI {
        return;
    }
    // In vi command mode the ESC is consumed as the mode switch and the
    // remainder arrives on its own, so the same twelve are bound bare.
    for (seq, slot) in seqs {
        add_arrow(el, &seq[1..], slot);
    }
}

/// C: `keymacro_add(el, seq, &arrow[slot].fun, arrow[slot].type)`.
///
/// The C hands `keymacro_add` a pointer into the function-key table while
/// also passing `el`; Rust cannot, so the value is cloned first — which is
/// the reason [`KeymacroValueT`] is `Clone`.
fn add_arrow(el: &mut EditLine, seq: &[u32], slot: usize) {
    let Some(arrow) = el.el_terminal.t_fkey.get(slot) else {
        return;
    };
    let (fun, ntype) = (arrow.fun.clone(), arrow.r#type);
    keymacro_add(el, seq, &fun, ntype);
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
    // `take` is the C's fixed-size array in both directions: it stops at
    // `A_K_NKEYS` and it stops early on a table `terminal_init` has not
    // filled, which is what the C's own bound cannot do.
    for arrow in el.el_terminal.t_fkey.iter_mut().take(A_K_NKEYS) {
        // A NULL table name — the C would dereference it — never matches.
        if arrow.name.is_some_and(|n| wcs_eq(name, n)) {
            arrow.fun = fun;
            arrow.r#type = type_;
            return 0;
        }
    }
    -1
}

// [spec:libedit:def:terminal.terminal-clear-arrow-fn]
// [spec:libedit:sem:terminal.terminal-clear-arrow-fn]
pub(crate) fn terminal_clear_arrow(el: &mut EditLine, name: &[u32]) -> i32 {
    for arrow in el.el_terminal.t_fkey.iter_mut().take(A_K_NKEYS) {
        if arrow.name.is_some_and(|n| wcs_eq(name, n)) {
            // The bound function value is left untouched; only the type
            // changes, and the key map is not modified until
            // `terminal_bind_arrow` next runs.
            arrow.r#type = XK_NOD;
            return 0;
        }
    }
    -1
}

// [spec:libedit:def:terminal.terminal-print-arrow-fn]
// [spec:libedit:sem:terminal.terminal-print-arrow-fn]
pub(crate) fn terminal_print_arrow(el: &mut EditLine, name: &[u32]) {
    // The empty wide string means "print them all".
    let all = wcs(name).is_empty();
    // Selected first, printed second, because printing needs the `EditLine`
    // the table is a field of. `keymacro_kprint` only writes output, so the
    // rows it sees and the order they come out in are the walk's.
    let matched: Vec<(&'static [u32], KeymacroValueT, i32)> = el
        .el_terminal
        .t_fkey
        .iter()
        .take(A_K_NKEYS)
        .filter_map(|arrow| Some((arrow.name?, arrow.fun.clone(), arrow.r#type)))
        .filter(|&(aname, _, ntype)| (all || wcs_eq(name, aname)) && ntype != XK_NOD)
        .collect();
    for (aname, fun, ntype) in matched {
        keymacro_kprint(el, aname, Some(&fun), ntype);
    }
    // A name matching nothing produces no output, and there is no diagnostic.
}

// [spec:libedit:def:terminal.terminal-bind-arrow-fn]
// [spec:libedit:sem:terminal.terminal-bind-arrow-fn]
pub(crate) fn terminal_bind_arrow(el: &mut EditLine) {
    // Step 1. Both subsystems must exist. `t_buf` is the C's "terminal is up"
    // marker and the key map is NULL until `map_init` runs, which is after
    // `terminal_init` in the library's initialisation order — this guard is
    // what stops the very first call.
    if el.el_terminal.t_buf.is_empty() || el.el_map.key.is_empty() {
        return;
    }

    // Step 2. The live map and the reference map holding the mode's factory
    // defaults, used to detect whether the user has changed a binding.
    let vi = el.el_map.r#type == MAP_VI;
    let live = if vi {
        ElMapCurrent::Alt
    } else {
        ElMapCurrent::Key
    };

    // Step 3.
    terminal_reset_arrow(el);

    // Step 4. Each row is copied out before its body runs, because the body
    // rebinds through `el` and the table is a field of it. Nothing below
    // touches `t_fkey`, so the copies stay current.
    let rows: Vec<(usize, i32, KeymacroValueT)> = el
        .el_terminal
        .t_fkey
        .iter()
        .take(A_K_NKEYS)
        .map(|a| (usize::try_from(a.key).unwrap_or(0), a.r#type, a.fun.clone()))
        .collect();
    for (key, ntype, fun) in rows {
        // (a).
        let Some(p) = cap_owned(el, key) else {
            continue;
        };
        if p.is_empty() {
            continue;
        }

        // (b). ERR-terminal-03, disposition `define`: the C copies while
        // `n < VISUAL_WIDTH_MAX`, so a capability of 8 bytes or more leaves
        // the buffer unterminated and the `keymacro_*` calls read past its
        // end. Bounded to 7 and always terminated — which for a slice means
        // the sequence simply stops at 7 elements.
        //
        // ERR-terminal-24, disposition `fix`: `p[n]` is a plain `char`, so a
        // byte >= 0x80 widens to a negative wide character on a signed-`char`
        // platform. Widened through an unsigned byte.
        let seq: Vec<u32> = p
            .iter()
            .take(VISUAL_WIDTH_MAX - 1)
            .map(|&b| u32::from(b))
            .collect();

        // (c).
        let j = usize::from(p[0]);

        if ntype == XK_NOD {
            // (d): the binding was explicitly cleared by
            // `terminal_clear_arrow`.
            keymacro_clear(el, live, &seq);
            continue;
        }

        let live_j = map_get(el, vi, j);
        // A reference map that is gone — `map_end` NULLs both — cannot agree
        // with the live one, so the "still at its factory default" half of
        // the test is false. The C would dereference NULL.
        let dmap_j = if vi { el.el_map.vic } else { el.el_map.emacs }.map(|m| m[j]);

        if p.len() > 1 && (dmap_j == Some(live_j) || live_j == ED_SEQUENCE_LEAD_IN) {
            // (e). This is what lets ESC-prefixed arrow sequences work while
            // leaving a user-rebound ESC alone.
            keymacro_add(el, &seq, &fun, ntype);
            map_set(el, vi, j, ED_SEQUENCE_LEAD_IN);
        } else if live_j == ED_UNASSIGNED {
            // (f).
            keymacro_clear(el, live, &seq);
            if ntype == XK_CMD {
                if let KeymacroValueT::Cmd(cmd) = fun {
                    map_set(el, vi, j, cmd);
                }
            } else {
                keymacro_add(el, &seq, &fun, ntype);
            }
        }
        // (g): the user has bound that leading byte themselves; do nothing.
    }
}

/// C: `map[j]`, where `map` aliases `el_map.alt` or `el_map.key`.
fn map_get(el: &EditLine, vi: bool, j: usize) -> ElActionT {
    let m = if vi { &el.el_map.alt } else { &el.el_map.key };
    m.get(j).copied().unwrap_or(ED_UNASSIGNED)
}

/// C: `map[j] = v`.
fn map_set(el: &mut EditLine, vi: bool, j: usize, v: ElActionT) {
    let m = if vi {
        &mut el.el_map.alt
    } else {
        &mut el.el_map.key
    };
    if let Some(slot) = m.get_mut(j) {
        *slot = v;
    }
}

// [spec:libedit:def:terminal.terminal-tputs-fn]
// [spec:libedit:sem:terminal.terminal-tputs-fn]
/// Emit an already-expanded capability string to `el->el_outfile`, honouring
/// its padding. Reduces to a call to [`tputs`] with this `EditLine`'s writer,
/// pad source and line speed; the C's file-static `FILE *` and its mutex have
/// no counterpart here.
fn terminal_tputs(el: &mut EditLine, cap: &str, affcnt: i32) {
    terminal_tputs_bytes(el, cap.as_bytes(), affcnt);
}

/// Byte form of [`terminal_tputs`].
///
/// Capability values are bytes — a capability may carry a byte above 0x7f —
/// and the rule-given signature above takes `&str`, so this carries the real
/// body and [`tputs_cap`] picks between the two. ERR-terminal-34,
/// disposition `fix`: the global destination stream and the mutex
/// serialising it are gone, so two `EditLine`s on different streams are safe
/// to use concurrently, which the C could not guarantee.
fn terminal_tputs_bytes(el: &mut EditLine, cap: &[u8], affcnt: i32) {
    let fd = el.el_outfd;
    if fd < 0 {
        // The C's callback returns -1 when the stream is NULL; `tputs`'s
        // return value is discarded either way.
        return;
    }
    let baud = el.el_tty.t_speed;
    let entry = el.el_terminal.t_entry.as_ref();
    // SAFETY: as `write_fd` — the descriptor is the application's, stays open
    // for the life of the `EditLine`, and `ManuallyDrop` keeps this borrow
    // from closing it.
    let mut out = std::mem::ManuallyDrop::new(unsafe {
        <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd)
    });
    let _ = tputs(&mut *out, cap, affcnt, entry, baud);
}

// [spec:libedit:def:terminal.terminal-putc-fn]
// [spec:libedit:sem:terminal.terminal-putc-fn]
/// The C's doubled underscore is not snake case to rustc; the name stays.
#[allow(non_snake_case)]
pub(crate) fn terminal__putc(el: &mut EditLine, c: u32) -> i32 {
    // Step 1: the column-padding sentinel writes nothing, which is what lets
    // the callers count columns while the byte stream stays correct. It must
    // be tested before the literal bit, because `MB_FILL_CHAR` has bit 31
    // set too.
    if c == MB_FILL_CHAR {
        return 0;
    }
    // Step 2: a handle into the literal-string table rather than a
    // character. Literals occupy no columns and must not be re-encoded.
    if c & EL_LITERAL != 0 {
        let fd = el.el_outfd;
        let bytes = literal_get(el, c);
        return write_fd(fd, bytes);
    }
    // Steps 3 and 4.
    let mut buf = [0u8; locale::MB_LEN_MAX];
    let i = ct_encode_char(&mut buf, c);
    if i <= 0 {
        // Unencodable in the current locale, or an empty result: the
        // encoder's non-positive value is returned unchanged, having written
        // nothing.
        return i32::try_from(i).unwrap_or(-1);
    }
    let n = usize::try_from(i).unwrap_or(0);
    el.write_outfile(&buf[..n])
}

// [spec:libedit:def:terminal.terminal-flush-fn]
// [spec:libedit:sem:terminal.terminal-flush-fn]
#[allow(non_snake_case)]
pub fn terminal__flush(el: &mut EditLine) {
    // C: `(void) fflush(el->el_outfile)`, its result discarded, so a write
    // error at this point is not reported to anyone.
    //
    // Nothing is buffered on this side. The C's destination is a caller-owned
    // `FILE *` this port cannot write through, so every byte goes straight to
    // `el_outfd` (see `write_fd`) and there is nothing left to push. That
    // trades one write per character for the C's buffering; a buffer would
    // need somewhere to live, and `el_terminal` has no field for one.
    let _ = el;
}

// [spec:libedit:def:terminal.terminal-writec-fn]
// [spec:libedit:sem:terminal.terminal-writec-fn]
pub(crate) fn terminal_writec(el: &mut EditLine, c: u32) {
    // The buffer is one element longer than VISUAL_WIDTH_MAX so the
    // terminator is always in bounds even for a maximum-width rendering.
    let mut visbuf = [0u32; VISUAL_WIDTH_MAX + 1];
    let vcnt = ct_visual_char(&mut visbuf[..VISUAL_WIDTH_MAX], c);
    // A negative count — the character could not be rendered — is treated as
    // 0, which makes the write a no-op.
    let vcnt = usize::try_from(vcnt).unwrap_or(0);
    visbuf[vcnt] = 0;
    terminal_overwrite(el, &visbuf, vcnt);
    terminal__flush(el);
}

// [spec:libedit:def:terminal.terminal-telltc-fn]
// [spec:libedit:sem:terminal.terminal-telltc-fn]
/// One of the four editrc command handlers, all sharing the C's
/// `int (*)(EditLine *, int, const wchar_t **)` shape. The C's
/// NULL-terminated `wchar_t **` becomes a slice of wide strings; `argc` is
/// kept because the C passes it, even though this handler ignores both.
pub fn terminal_telltc(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    let _ = (argc, argv);

    el.write_outfile(b"\n\tYour terminal has the\n");
    el.write_outfile(b"\tfollowing characteristics:\n\n");
    let (co, li) = (val(el, T_CO), val(el, T_LI));
    el.write_outfile(format!("\tIt has {co} columns and {li} lines\n").as_bytes());

    let flags = el.el_terminal.t_flags;
    let meta = if flags & TERM_HAS_META != 0 {
        "a"
    } else {
        "no"
    };
    el.write_outfile(format!("\tIt has {meta} meta key\n").as_bytes());
    let tabs = if flags & TERM_CAN_TAB != 0 {
        " "
    } else {
        "not "
    };
    el.write_outfile(format!("\tIt can{tabs}use tabs\n").as_bytes());
    let am = if flags & TERM_HAS_AUTO_MARGINS != 0 {
        "has"
    } else {
        "does not have"
    };
    el.write_outfile(format!("\tIt {am} automatic margins\n").as_bytes());
    if flags & TERM_HAS_AUTO_MARGINS != 0 {
        let xn = if flags & TERM_HAS_MAGIC_MARGINS != 0 {
            "has"
        } else {
            "does not have"
        };
        el.write_outfile(format!("\tIt {xn} magic margins\n").as_bytes());
    }

    // One line per string capability, walking the table and the slots in
    // lockstep. The parenthesised token is the terminfo capname where the C
    // printed its termcap code — a deliberate, user-visible substitution the
    // rule calls for.
    for (i, t) in TSTR.iter().enumerate() {
        let ub = match cap_owned(el, i) {
            // The stored value rendered visually: decoded, passed through the
            // display escaping, then re-encoded, so control characters appear
            // printably. A NULL anywhere on that chain — the C would hand it
            // to `%s` — becomes empty here.
            Some(bytes) => {
                let dec = ct_decode_string(Some(&bytes), &mut el.el_scratch).map(<[u32]>::to_vec);
                let vis = dec.and_then(|w| {
                    ct_visual_string(Some(&w), &mut el.el_visual).map(<[u32]>::to_vec)
                });
                vis.map(|v| encode(el, &v)).unwrap_or_default()
            }
            None => b"(empty)".to_vec(),
        };
        let mut line = format!("\t{:>25} ({}) == ", t.long_name, t.name).into_bytes();
        line.extend_from_slice(&ub);
        line.push(b'\n');
        el.write_outfile(&line);
    }
    el.write_outfile(b"\n");
    0
}

// [spec:libedit:def:terminal.terminal-settc-fn]
// [spec:libedit:sem:terminal.terminal-settc-fn]
pub fn terminal_settc(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    let _ = argc;

    // Step 1. The C tests `argv`, `argv[1]` and `argv[2]` for NULL; the
    // NULL-terminated vector makes that "fewer than three arguments".
    if argv.len() < 3 {
        return -1;
    }

    // Step 2. ERR-terminal-30, disposition `needs decision`: `strlcpy` into
    // `char[8]` silently cuts both the name and the value to 7 bytes, which
    // also caps the *string* form — a capability longer than 7 bytes cannot
    // be installed through this interface. The rule says the port must keep
    // that limit or change it deliberately; `plan/decisions/conformance-policy.md`
    // makes reproduce the default, so the limit stays.
    let what = truncate7(encode(el, argv[1]));
    let how = truncate7(encode(el, argv[2]));

    // Step 3: the strings first.
    if let Some(idx) = resolve_cap(CapTable::Str, &what) {
        match std::str::from_utf8(&how) {
            Ok(s) => terminal_alloc(el, idx, Some(s)),
            // Encoded bytes that are not UTF-8 cannot cross `terminal_alloc`'s
            // rule-given `&str`; they are interned unchanged instead.
            Err(_) => terminal_alloc_bytes(el, idx, Some(&how)),
        }
        terminal_setflags(el);
        return 0;
    }

    // Step 4: the numeric ones second.
    let Some(idx) = resolve_cap(CapTable::Val, &what) else {
        let mut msg = encode(el, argv[0]);
        msg.extend_from_slice(b": Bad capability `");
        msg.extend_from_slice(&what);
        msg.extend_from_slice(b"'.\n");
        el.write_errfile(&msg);
        return -1;
    };

    // Step 5: the four entries treated as boolean.
    if idx == T_PT || idx == T_KM || idx == T_AM || idx == T_XN {
        if how == b"yes" {
            set_val(el, idx, 1);
        } else if how == b"no" {
            set_val(el, idx, 0);
        } else {
            bad_value(el, argv[0], &how);
            return -1;
        }
        terminal_setflags(el);
        return 0;
    }

    // Step 6: everything else is numeric — including the destructive-tabs
    // flag and the MT meta flag, both booleans by nature. ERR-terminal-31,
    // disposition `reproduce`: an empty value consumes nothing, leaves the
    // terminator at the first position and is therefore *accepted* as 0.
    let (v, consumed) = strtol10(&how);
    if consumed != how.len() {
        bad_value(el, argv[0], &how);
        return -1;
    }
    // C: `(int) i` — a truncating narrowing conversion.
    set_val(el, idx, v as i32);

    // Step 7 is ERR-terminal-20, disposition `fix`, exactly as in
    // `terminal_set`: the C writes `t_size.v` from the column count and
    // `t_size.h` from the line count, both swapped, and both are overwritten
    // correctly by the `terminal_rebuffer_display` that step 8 reaches. What
    // survives is whether a size slot was written at all.
    let size_changed = idx == T_CO || idx == T_LI;

    // Step 8. Note the line count is passed first.
    if size_changed && terminal_change_size(el, val(el, T_LI), val(el, T_CO)) == -1 {
        return -1;
    }
    // ERR-terminal-31 again: `terminal_setflags` is *not* called on this
    // path, so changing the destructive-tabs value does not update
    // TERM_CAN_TAB until some later event recomputes the flags.
    0
}

/// C: `strlcpy(dst, src, sizeof(char[8]))` — truncate to 7 bytes plus the
/// terminator the byte string does not carry.
fn truncate7(mut s: Vec<u8>) -> Vec<u8> {
    s.truncate(7);
    s
}

/// C: `fprintf(el->el_errfile, "%ls: Bad value `%s'.\n", argv[0], how)`.
fn bad_value(el: &mut EditLine, cmd: &[u32], how: &[u8]) {
    let mut msg = encode(el, cmd);
    msg.extend_from_slice(b": Bad value `");
    msg.extend_from_slice(how);
    msg.extend_from_slice(b"'.\n");
    el.write_errfile(&msg);
}

/// C: `strtol(s, &ep, 10)`, returning the value and `ep - s`.
fn strtol10(s: &[u8]) -> (i64, usize) {
    let wide: Vec<u32> = s.iter().map(|&b| u32::from(b)).collect();
    wcstol10(&wide)
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
///
/// Public only so `nshedit-abi` can write `el_wget`'s `EL_GETTC` arm, and
/// hidden because the signature *is* that smuggled out-pointer: `argv[2]` is
/// a `char **` for some capability names and an `int *` for others, and
/// getting it wrong is a type-confusing store. Idiomatization owes the core a
/// capability query that returns a value.
#[doc(hidden)]
pub fn terminal_gettc(el: &mut EditLine, argc: i32, argv: &[*mut c_char]) -> i32 {
    let _ = argc;

    // Step 1.
    if argv.len() < 3 || argv[1].is_null() || argv[2].is_null() {
        return -1;
    }
    // SAFETY: `argv[1]` is the C caller's NUL-terminated capability name.
    let what = unsafe { std::ffi::CStr::from_ptr(argv[1]) }
        .to_bytes()
        .to_vec();
    let how = argv[2];

    // Step 2. The caller receives libedit's own interior pointer, which may
    // be NULL when the capability is absent and which any later
    // `terminal_set`/`terminal_settc` invalidates — which is why the stored
    // value carries the terminator the C's does (see `terminal_alloc_bytes`).
    if let Some(idx) = resolve_cap(CapTable::Str, &what) {
        let p = match el.el_terminal.t_str.get(idx) {
            Some(Some(v)) => v.as_ptr().cast::<c_char>().cast_mut(),
            _ => std::ptr::null_mut(),
        };
        // SAFETY: `argv[2]` is the caller's `char **`, per the documented
        // `el_get(EL_GETTC, name, &p)` contract.
        unsafe { *how.cast::<*mut c_char>() = p };
        return 0;
    }

    // Step 3.
    let Some(idx) = resolve_cap(CapTable::Val, &what) else {
        return -1;
    };

    // Step 4. Both strings are statically allocated and remain valid after
    // the call.
    if idx == T_PT || idx == T_KM || idx == T_AM || idx == T_XN {
        static YES: &[u8] = b"yes\0";
        static NO: &[u8] = b"no\0";
        let s = if val(el, idx) != 0 { YES } else { NO };
        // SAFETY: as above — a `char **` for the boolean capabilities.
        unsafe { *how.cast::<*mut c_char>() = s.as_ptr().cast::<c_char>().cast_mut() };
        return 0;
    }

    // Step 5. The destructive-tabs flag and the MT meta flag are booleans by
    // nature but are reported as integers here, matching `terminal_settc`.
    // SAFETY: `argv[2]` is the caller's `int *` for these entries.
    unsafe { *how.cast::<i32>() = val(el, idx) };
    0
}

// [spec:libedit:def:terminal.terminal-echotc-fn]
// [spec:libedit:sem:terminal.terminal-echotc-fn]
pub fn terminal_echotc(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    let _ = argc;

    let mut verbose = false;
    let mut silent = false;

    // Step 1. `argv[1] == NULL` is "only the command name".
    if argv.len() < 2 {
        return -1;
    }
    // Step 2.
    let mut a = 1usize;

    // Step 3: at most one option, and an unrecognised letter is ignored with
    // no diagnostic.
    if wcs(argv[a]).first() == Some(&u32::from(b'-')) {
        match wcs(argv[a]).get(1).copied() {
            Some(c) if c == u32::from(b'v') => verbose = true,
            Some(c) if c == u32::from(b's') => silent = true,
            _ => {}
        }
        a += 1;
    }
    // Step 4.
    if a >= argv.len() || wcs(argv[a]).is_empty() {
        return 0;
    }
    let arg = wcs(argv[a]).to_vec();

    // Step 5: pseudo-capabilities. The yes/no lines use "%s\n" and the
    // numeric ones "%d\n". Answering one of these is the whole of the step —
    // none of the seven names ever reaches the capability tables below — so
    // the line is built first and the single write is what returns.
    let flags = el.el_terminal.t_flags;
    let yesno = |b: bool| if b { "yes" } else { "no" };
    let pseudo = if wcs_eq(&arg, A_TABS) {
        Some(format!("{}\n", yesno(flags & TERM_CAN_TAB != 0)))
    } else if wcs_eq(&arg, A_META) {
        // From the meta-key value slot directly, not from TERM_HAS_META, so
        // the separate MT slot is deliberately not considered here.
        Some(format!("{}\n", yesno(val(el, T_KM) != 0)))
    } else if wcs_eq(&arg, A_XN) {
        Some(format!("{}\n", yesno(flags & TERM_HAS_MAGIC_MARGINS != 0)))
    } else if wcs_eq(&arg, A_AM) {
        Some(format!("{}\n", yesno(flags & TERM_HAS_AUTO_MARGINS != 0)))
    } else if wcs_eq(&arg, A_BAUD) {
        // The raw `speed_t`, as the C prints it: on Linux that is the encoded
        // `B*` constant, not a baud number.
        Some(format!("{}\n", el.el_tty.t_speed as i32))
    } else if wcs_eq(&arg, A_ROWS) || wcs_eq(&arg, A_LINES) {
        Some(format!("{}\n", val(el, T_LI)))
    } else if wcs_eq(&arg, A_COLS) {
        Some(format!("{}\n", val(el, T_CO)))
    } else {
        None
    };
    if let Some(line) = pseudo {
        el.write_outfile(line.as_bytes());
        return 0;
    }

    // Step 6: try the local definition first, then fall back to the terminal
    // database. Note the fallback is reached only when the *name* is unknown:
    // a known name whose slot is empty does not fall back.
    let narrow = encode(el, &arg);
    let scap: Option<Vec<u8>> = match resolve_cap(CapTable::Str, &narrow) {
        Some(idx) => cap_owned(el, idx),
        None => std::str::from_utf8(&narrow)
            .ok()
            .and_then(|n| tgetstr(el.el_terminal.t_entry.as_ref(), n)),
    };

    // Step 7.
    let scap = match scap {
        Some(s) if !s.is_empty() => s,
        _ => {
            if !silent {
                let name = encode(el, &arg);
                let mut msg = b"echotc: Termcap parameter `".to_vec();
                msg.extend_from_slice(&name);
                msg.extend_from_slice(b"' not found.\n");
                el.write_errfile(&msg);
            }
            return -1;
        }
    };

    // Step 8: count how many values this capability needs. ERR-terminal-04,
    // disposition `define`: the C reads the character after `%`
    // unconditionally, so a capability ending in a bare `%` reads the
    // terminating NUL and then steps one position past it. The scan stops at
    // the end here.
    let mut arg_need = 0i32;
    let mut rest = scap.iter().copied();
    while let Some(c) = rest.next() {
        if c != b'%' {
            continue;
        }
        // Taking the next byte from the same cursor is what consumes the
        // whole two-byte sequence, so a `%%` is one escape rather than two
        // introducers.
        let Some(c) = rest.next() else {
            break;
        };
        match c {
            b'd' | b'2' | b'3' | b'.' | b'+' => arg_need += 1,
            b'%' | b'>' | b'i' | b'r' | b'n' | b'B' | b'D' => {}
            _ => {
                // hpux has lots of them. This is bad, but I won't complain.
                if verbose {
                    let mut msg = b"echotc: Warning: unknown termcap % `".to_vec();
                    msg.push(c);
                    msg.extend_from_slice(b"'.\n");
                    el.write_errfile(&msg);
                }
            }
        }
    }

    // Step 9. Every diagnostic phrased as a "Warning:" nonetheless returns
    // -1; `-s` suppresses the messages but changes no return value.
    match arg_need {
        0 => {
            if echotc_extra(el, argv, a + 1, silent) {
                return -1;
            }
            tputs_cap(el, &scap, 1);
        }
        1 => {
            // ERR-terminal-33, disposition `reproduce`: the column is forced
            // to 0 and the single user-supplied value becomes the row — the
            // *second* `tgoto` argument, not the first.
            let arg_cols = 0;
            let Some(arg_rows) = echotc_number(el, argv, &mut a, silent, "rows") else {
                return -1;
            };
            if echotc_extra(el, argv, a + 1, silent) {
                return -1;
            }
            let expanded = tgoto(&scap, arg_cols, arg_rows);
            tputs_cap(el, &expanded, 1);
        }
        _ => {
            // Any count greater than 2 falls through to the two-parameter
            // case, warning first when verbose.
            if arg_need > 2 && verbose {
                el.write_errfile(
                    format!("echotc: Warning: Too many required arguments ({arg_need}).\n")
                        .as_bytes(),
                );
            }
            let Some(arg_cols) = echotc_number(el, argv, &mut a, silent, "cols") else {
                return -1;
            };
            let Some(arg_rows) = echotc_number(el, argv, &mut a, silent, "rows") else {
                return -1;
            };
            // ERR-terminal-65: the C re-tests the same parse result a third
            // time here. It can never fire, so it is not ported.

            if echotc_extra(el, argv, a + 1, silent) {
                return -1;
            }
            // ERR-terminal-33 again: the affected-line count is the row
            // value, which can be 0 and then zeroes any per-affected-line
            // padding.
            let expanded = tgoto(&scap, arg_cols, arg_rows);
            tputs_cap(el, &expanded, arg_rows);
        }
    }
    // Step 10.
    0
}

// The wide literals `terminal_echotc` compares against.
/// C: `L"tabs"`.
const A_TABS: &[u32] = &[0x74, 0x61, 0x62, 0x73];
/// C: `L"meta"`.
const A_META: &[u32] = &[0x6d, 0x65, 0x74, 0x61];
/// C: `L"xn"`.
const A_XN: &[u32] = &[0x78, 0x6e];
/// C: `L"am"`.
const A_AM: &[u32] = &[0x61, 0x6d];
/// C: `L"baud"`.
const A_BAUD: &[u32] = &[0x62, 0x61, 0x75, 0x64];
/// C: `L"rows"`.
const A_ROWS: &[u32] = &[0x72, 0x6f, 0x77, 0x73];
/// C: `L"lines"`.
const A_LINES: &[u32] = &[0x6c, 0x69, 0x6e, 0x65, 0x73];
/// C: `L"cols"`.
const A_COLS: &[u32] = &[0x63, 0x6f, 0x6c, 0x73];

/// Advance `a` and read the argument there as a non-negative decimal count,
/// diagnosing a missing or malformed one as `what` — `"rows"` or `"cols"`.
///
/// The one- and two-parameter forms of step 9 spell this out three times
/// between them, differing only in that word, and the empty-argument test has
/// to be the same at each: an empty string is *missing*, not a bad value.
/// `None` is the caller's -1.
fn echotc_number(
    el: &mut EditLine,
    argv: &[&[u32]],
    a: &mut usize,
    silent: bool,
    what: &str,
) -> Option<i32> {
    *a += 1;
    let Some(v) = argv.get(*a).filter(|s| !wcs(s).is_empty()) else {
        warn_missing(el, silent);
        return None;
    };
    let v = wcs(v).to_vec();
    let (n, consumed) = wcstol10(&v);
    if consumed != v.len() || n < 0 {
        bad_echotc_value(el, silent, &v, what);
        return None;
    }
    Some(i32::try_from(n).unwrap_or(i32::MAX))
}

/// True — the caller's -1 — when position `a` holds one more non-empty
/// argument than the capability asked for, having said so.
///
/// Each of step 9's three arms ends with this test, and `a` is dead
/// afterwards, so the position is passed rather than the cursor advanced.
fn echotc_extra(el: &mut EditLine, argv: &[&[u32]], a: usize, silent: bool) -> bool {
    let Some(extra) = argv.get(a).filter(|s| !wcs(s).is_empty()) else {
        return false;
    };
    let extra = extra.to_vec();
    warn_extra(el, silent, &extra);
    true
}

/// C: `"echotc: Warning: Extra argument `%ls'.\n"`.
fn warn_extra(el: &mut EditLine, silent: bool, arg: &[u32]) {
    if silent {
        return;
    }
    let name = encode(el, arg);
    let mut msg = b"echotc: Warning: Extra argument `".to_vec();
    msg.extend_from_slice(&name);
    msg.extend_from_slice(b"'.\n");
    el.write_errfile(&msg);
}

/// C: `"echotc: Warning: Missing argument.\n"`.
fn warn_missing(el: &mut EditLine, silent: bool) {
    if silent {
        return;
    }
    el.write_errfile(b"echotc: Warning: Missing argument.\n");
}

/// C: `"echotc: Bad value `%ls' for rows.\n"` and its `cols` twin.
fn bad_echotc_value(el: &mut EditLine, silent: bool, arg: &[u32], what: &str) {
    if silent {
        return;
    }
    let name = encode(el, arg);
    let mut msg = b"echotc: Bad value `".to_vec();
    msg.extend_from_slice(&name);
    msg.extend_from_slice(format!("' for {what}.\n").as_bytes());
    el.write_errfile(&msg);
}

#[cfg(test)]
mod test;
