//! Ported from `src/terminal.c`; rules live in
//! `docs/spec/port/src/terminal.md`.
//!
//! Capabilities come from terminfo through the `term` crate, not from a
//! linked termcap provider, and are addressed by terminfo long name — see
//! `plan/decisions/terminal-caps-via-term-crate.md`. The two table structs
//! below keep their C names (`termcapstr`, `termcapval`) because the rules
//! do, but the `name` they carry is the terminfo one.

use crate::el::CoordT;
use crate::keymacro::KeymacroValueT;

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
    pub t_str: Vec<Option<String>>,
    /// C: `int *t_val` — the flag and numeric capabilities, 8 slots.
    pub t_val: Vec<i32>,
    /// C: `char *t_cap` — the TC_BUFSIZE scratch area the C's `tgetent`
    /// copies the raw terminal entry into. libedit never reads it;
    /// `sem:terminal.tgetent-fn` says the terminfo
    /// replacement takes no such buffer and the field can go with it.
    pub t_cap: Vec<u8>,
    /// C: `funckey_t *t_fkey` — the function-key table, `A_K_NKEYS` (7)
    /// entries.
    pub t_fkey: Vec<FunckeyT>,
}
