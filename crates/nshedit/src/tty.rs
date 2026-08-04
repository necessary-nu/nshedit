//! Ported from `src/tty.c`; rules live in `docs/spec/port/src/tty.md`.

use crate::el::ElActionT;

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
