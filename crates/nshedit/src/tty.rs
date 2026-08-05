//! Ported from `src/tty.c`; rules live in `docs/spec/port/src/tty.md`.

use std::io::Write;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;

use crate::chartype::{ct_encode_char, ct_encode_string};
use crate::el::{EDIT_DISABLED, EL_BUFSIZ, EditLine, ElActionT, NO_RESET};
use crate::fcns::{
    ED_DELETE_PREV_WORD, ED_INSERT, ED_PREV_CHAR, ED_PREV_WORD, ED_QUOTED_INSERT, ED_REDISPLAY,
    ED_UNASSIGNED, EM_DELETE_OR_LIST, EM_DELETE_PREV_CHAR, EM_KILL_LINE, VI_DELETE_PREV_CHAR,
    VI_KILL_LINE_PREV, VI_LIST_OR_EOF,
};
use crate::keymacro::keymacro_clear;
use crate::locale::MB_LEN_MAX;
use crate::map::{ElMapCurrent, MAP_VI, N_KEYS};
use crate::parse::parse__escape;

/// C: `#define NN_IO 3` — the number of I/O modes (`ED_IO`, `EX_IO`,
/// `QU_IO`).
pub const NN_IO: usize = 3;
/// C: `#define MD_NN 5` — the number of termios mode words per I/O mode
/// (`MD_INP`, `MD_OUT`, `MD_CTL`, `MD_LIN`, `MD_CHAR`). Do not re-order.
pub const MD_NN: usize = 5;
/// C: `#define C_NCC 25` — the number of control characters libedit tracks.
pub const C_NCC: usize = 25;

// The rest of `tty.h`'s index constants. The C reaches them through the
// header; nothing publishes them yet, so they live with the module that owns
// the header. They are `usize` because every use but one is an array
// subscript; the exceptions cast, as the C's `int` parameters do.

/// C: `#define EX_IO 0` — while we are executing.
pub const EX_IO: usize = 0;
/// C: `#define ED_IO 1` — while we are editing.
pub const ED_IO: usize = 1;
/// C: `#define TS_IO 2` — the scratch row a terminal snapshot is read into.
pub const TS_IO: usize = 2;
/// C: `#define QU_IO 2` — quoted-insert mode.
///
/// The same slot as [`TS_IO`], and `t_qu` is `#define t_qu t_ts`, so entering
/// quote mode overwrites the terminal-snapshot termios. Harmless only because
/// `tty_rawmode` re-reads it; see [`tty_quotemode`].
pub const QU_IO: usize = 2;

/// C: `#define MD_INP 0` — `c_iflag`.
pub const MD_INP: usize = 0;
/// C: `#define MD_OUT 1` — `c_oflag`.
pub const MD_OUT: usize = 1;
/// C: `#define MD_CTL 2` — `c_cflag`.
pub const MD_CTL: usize = 2;
/// C: `#define MD_LIN 3` — `c_lflag`.
pub const MD_LIN: usize = 3;
/// C: `#define MD_CHAR 4` — the control-character column, which has no
/// `tcflag_t` behind it.
pub const MD_CHAR: usize = 4;

/// The `C_*` control-character indices of `tty.h`, in their fixed order. They
/// index [`TtycharT`] rows and are **not** the termios `V*` subscripts; the
/// two coincide only by accident, which is the whole of ERR-terminal-37.
pub const C_INTR: usize = 0;
/// C: `#define C_QUIT 1`.
pub const C_QUIT: usize = 1;
/// C: `#define C_ERASE 2`.
pub const C_ERASE: usize = 2;
/// C: `#define C_KILL 3`.
pub const C_KILL: usize = 3;
/// C: `#define C_EOF 4`.
pub const C_EOF: usize = 4;
/// C: `#define C_EOL 5`.
pub const C_EOL: usize = 5;
/// C: `#define C_EOL2 6`.
pub const C_EOL2: usize = 6;
/// C: `#define C_SWTCH 7`.
pub const C_SWTCH: usize = 7;
/// C: `#define C_DSWTCH 8`.
pub const C_DSWTCH: usize = 8;
/// C: `#define C_ERASE2 9`.
pub const C_ERASE2: usize = 9;
/// C: `#define C_START 10`.
pub const C_START: usize = 10;
/// C: `#define C_STOP 11`.
pub const C_STOP: usize = 11;
/// C: `#define C_WERASE 12`.
pub const C_WERASE: usize = 12;
/// C: `#define C_SUSP 13`.
pub const C_SUSP: usize = 13;
/// C: `#define C_DSUSP 14`.
pub const C_DSUSP: usize = 14;
/// C: `#define C_REPRINT 15`.
pub const C_REPRINT: usize = 15;
/// C: `#define C_DISCARD 16`.
pub const C_DISCARD: usize = 16;
/// C: `#define C_LNEXT 17`.
pub const C_LNEXT: usize = 17;
/// C: `#define C_STATUS 18`.
pub const C_STATUS: usize = 18;
/// C: `#define C_PAGE 19`.
pub const C_PAGE: usize = 19;
/// C: `#define C_PGOFF 20`.
pub const C_PGOFF: usize = 20;
/// C: `#define C_KILL2 21`.
pub const C_KILL2: usize = 21;
/// C: `#define C_BRK 22`.
///
/// Inert in both directions: neither `tty__getchar` nor `tty__setchar` has a
/// `VBRK` assignment on any platform, and `tty__getcharindex` has no case for
/// it (ERR-terminal-05).
pub const C_BRK: usize = 22;
/// C: `#define C_MIN 23`.
pub const C_MIN: usize = 23;
/// C: `#define C_TIME 24`.
pub const C_TIME: usize = 24;

/// C: `#define C_SH(A) ((unsigned int)(1 << (A)))` — the `MD_CHAR` mask bit
/// for one `C_*` index. `C_NCC` is 25, so every index fits.
const fn c_sh(a: usize) -> u32 {
    1u32 << a
}

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

/// The platform's `NCCS`: how many `c_cc` slots a [`Termios`] carries.
///
/// Republished from [`plat`] so that whoever builds an [`ElTtyT`] — `el_init`
/// in `el.rs`, today — can size the four rows without reaching into a private
/// module. Use [`termios_zeroed`] rather than sizing by hand.
pub const NCCS: usize = plat::NCCS;

/// A [`Termios`] with every field zero and `c_cc` sized to [`NCCS`].
///
/// The C never needs this: its four `struct termios` are members of
/// `el_tty_t` and come into being with the `EditLine`'s `calloc`. The port's
/// `c_cc` is a `Vec`, so it has to be built, and it has to be built at
/// exactly [`NCCS`] or every `V*` subscript above the length silently reads
/// as 0 and swallows its write (see [`cc_get`]/[`cc_set`]).
#[must_use]
pub fn termios_zeroed() -> Termios {
    Termios {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_cc: vec![0; NCCS],
    }
}

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

/// The POSIX termios primitives this module is written against, the numeric
/// ABI it is written against — and the one place the port cannot reach.
///
/// `plan/decisions/no-c-ffi.md` bars the `libc` crate and Rust's standard
/// library exposes no termios API, so `tcgetattr` and `tcsetattr` have no
/// caller available here. `sig.rs` hit the same wall and answered it the same
/// way: the operations are named exactly, one function each, and the two that
/// need a syscall report failure today rather than pretending. That is not a
/// silent no-op — "every `tcgetattr` failed" is a state the C itself defines
/// and every caller in this file already handles, so the module degrades to
/// *nothing captured, nothing pushed, nothing restored*, exactly as the C
/// does against a descriptor that is not a terminal. `tty_init` therefore
/// returns -1 and `el_init` raises `NO_TTY`, which is the same outcome
/// libedit reaches today when its input is a pipe.
///
/// Three things here are **not** stubs, and it matters which:
///
/// - [`isatty`] is real. `std::io::IsTerminal` answers it without libc, so
///   the guard `tty_setup` opens with is the C's guard, and the failure moves
///   to the syscall that is genuinely missing rather than being masked by a
///   fake "not a terminal".
/// - The four `cf*` speed accessors are real, because they are pure functions
///   over the struct rather than syscalls. They are implemented against the
///   Linux/glibc encoding, where the line speed lives in the `CBAUD` bits of
///   `c_cflag`; see [`cfgetispeed`] for what that costs.
/// - The constants are real, and they are this platform's.
///
/// **Scope of the numbers.** Everything below is the Linux/glibc termios ABI:
/// the `V*` subscripts, `NCCS`, the four flag-word bit sets, the `TCSA*`
/// actions and the set of names `ttymodes[]` carries. The BSDs use a
/// different termios ABI throughout — different `V*` numbering, different
/// flag bits, a `struct termios` with separate `c_ispeed`/`c_ospeed` — and
/// this module does not carry it. `plan/decisions/posix-only-scope.md` puts
/// POSIX on the target and the numbers are not POSIX's to give, so following
/// `sig.rs`'s precedent the platform ABI is transcribed rather than guessed.
/// The one place the split is reproduced is [`VDISABLE`], because
/// `sem:tty.tty-bind-char-fn` requires it: the disable byte reaches the key
/// map, and it is 0 on glibc and 0xff on the BSDs.
///
/// What has to arrive for the module to function is `tcgetattr` and
/// `tcsetattr`, issued without libc. Nothing else in this file is waiting on
/// anything.
mod plat {
    use std::io::IsTerminal;
    use std::os::fd::BorrowedFd;

    use super::Termios;

    /// `_POSIX_VDISABLE`. POSIX defines the constant but not its value;
    /// glibc/Linux uses 0 and the BSDs and macOS use 0xff. `tty.h` falls back
    /// to `(unsigned char)-1` where the platform defines neither
    /// `_POSIX_VDISABLE` nor `VDISABLE`, which is the 0xff arm.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub(super) const VDISABLE: u8 = 0;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    pub(super) const VDISABLE: u8 = 0xff;

    /// `NCCS` — the length of `c_cc`.
    pub(super) const NCCS: usize = 32;

    /// `TCSANOW` — apply immediately.
    pub(super) const TCSANOW: i32 = 0;
    /// `TCSADRAIN` — apply after queued output has drained, keeping queued
    /// input. Every call in this file but `tty_end`'s passes this.
    pub(super) const TCSADRAIN: i32 = 1;
    /// `TCSAFLUSH` — drain output, then discard unread input.
    pub(super) const TCSAFLUSH: i32 = 2;

    // `c_iflag` bits.
    pub(super) const IGNBRK: u32 = 0o0000001;
    pub(super) const BRKINT: u32 = 0o0000002;
    pub(super) const IGNPAR: u32 = 0o0000004;
    pub(super) const PARMRK: u32 = 0o0000010;
    pub(super) const INPCK: u32 = 0o0000020;
    pub(super) const ISTRIP: u32 = 0o0000040;
    pub(super) const INLCR: u32 = 0o0000100;
    pub(super) const IGNCR: u32 = 0o0000200;
    pub(super) const ICRNL: u32 = 0o0000400;
    /// Legacy SysV, Linux only. Note it is the *same bit* as [`ECHOCTL`],
    /// which is what makes ERR-terminal-36 the silent no-op it is.
    pub(super) const IUCLC: u32 = 0o0001000;
    pub(super) const IXON: u32 = 0o0002000;
    pub(super) const IXANY: u32 = 0o0004000;
    pub(super) const IXOFF: u32 = 0o0010000;
    pub(super) const IMAXBEL: u32 = 0o0020000;

    // `c_oflag` bits.
    pub(super) const OPOST: u32 = 0o0000001;
    pub(super) const OLCUC: u32 = 0o0000002;
    pub(super) const ONLCR: u32 = 0o0000004;
    pub(super) const OCRNL: u32 = 0o0000010;
    pub(super) const ONOCR: u32 = 0o0000020;
    pub(super) const ONLRET: u32 = 0o0000040;
    pub(super) const OFILL: u32 = 0o0000100;
    pub(super) const OFDEL: u32 = 0o0000200;
    pub(super) const NLDLY: u32 = 0o0000400;
    pub(super) const CRDLY: u32 = 0o0003000;
    pub(super) const TABDLY: u32 = 0o0014000;
    /// The XSI `TABDLY` value meaning "expand tabs to spaces". `tty.h`
    /// aliases it to `OXTABS` on the BSDs and to 0 where neither exists — the
    /// degenerate case `sem:tty.tty-rawmode-fn` warns about, in which
    /// `(x & 0) == 0` is always true and `t_tabs` is forced to 0. It is
    /// **not** degenerate here: glibc defines `TAB3`, so the `EL_CAN_TAB`
    /// branch of `tty_rawmode` is live.
    pub(super) const TAB3: u32 = 0o0014000;
    /// `XTABS`, which glibc gives the same value as [`TAB3`]. `ttymodes[]`
    /// carries both names, so `+xtabs` and `+tabdly` interact.
    pub(super) const XTABS: u32 = 0o0014000;
    pub(super) const BSDLY: u32 = 0o0020000;
    pub(super) const VTDLY: u32 = 0o0040000;
    pub(super) const FFDLY: u32 = 0o0100000;

    // `c_cflag` bits.
    pub(super) const CBAUD: u32 = 0o0010017;
    pub(super) const CBAUDEX: u32 = 0o0010000;
    pub(super) const CSIZE: u32 = 0o0000060;
    pub(super) const CS8: u32 = 0o0000060;
    pub(super) const CSTOPB: u32 = 0o0000100;
    pub(super) const CREAD: u32 = 0o0000200;
    pub(super) const PARENB: u32 = 0o0000400;
    pub(super) const PARODD: u32 = 0o0001000;
    pub(super) const HUPCL: u32 = 0o0002000;
    pub(super) const CLOCAL: u32 = 0o0004000;
    pub(super) const CIBAUD: u32 = 0o02003600000;
    pub(super) const CRTSCTS: u32 = 0o020000000000;

    // `c_lflag` bits.
    pub(super) const ISIG: u32 = 0o0000001;
    pub(super) const ICANON: u32 = 0o0000002;
    pub(super) const XCASE: u32 = 0o0000004;
    pub(super) const ECHO: u32 = 0o0000010;
    pub(super) const ECHOE: u32 = 0o0000020;
    pub(super) const ECHOK: u32 = 0o0000040;
    pub(super) const ECHONL: u32 = 0o0000100;
    pub(super) const NOFLSH: u32 = 0o0000200;
    pub(super) const TOSTOP: u32 = 0o0000400;
    /// Echo control characters as `^X`. A `c_lflag` bit, and on glibc the
    /// same value as the `c_iflag` bit [`IUCLC`] — the coincidence
    /// ERR-terminal-36 turns into a permanent -1.
    pub(super) const ECHOCTL: u32 = 0o0001000;
    pub(super) const ECHOPRT: u32 = 0o0002000;
    pub(super) const ECHOKE: u32 = 0o0004000;
    pub(super) const FLUSHO: u32 = 0o0010000;
    pub(super) const PENDIN: u32 = 0o0040000;
    pub(super) const IEXTEN: u32 = 0o0100000;
    pub(super) const EXTPROC: u32 = 0o0200000;

    // The termios `V*` subscripts this platform defines, as the C sees them
    // after `tty.h`'s aliasing. glibc has no `VSWTCH` (only `VSWTC`, which
    // `tty.c` never names), no `VDSWTCH`, `VERASE2`, `VDSUSP`, `VSTATUS`,
    // `VPAGE`, `VPGOFF`, `VKILL2` or `VBRK`, so those rows of every table in
    // this file are simply absent — which is what `#ifdef`ing them out means.
    pub(super) const VINTR: usize = 0;
    pub(super) const VQUIT: usize = 1;
    pub(super) const VERASE: usize = 2;
    pub(super) const VKILL: usize = 3;
    pub(super) const VEOF: usize = 4;
    pub(super) const VTIME: usize = 5;
    pub(super) const VMIN: usize = 6;
    pub(super) const VSTART: usize = 8;
    pub(super) const VSTOP: usize = 9;
    pub(super) const VSUSP: usize = 10;
    pub(super) const VEOL: usize = 11;
    pub(super) const VREPRINT: usize = 12;
    pub(super) const VDISCARD: usize = 13;
    pub(super) const VWERASE: usize = 14;
    pub(super) const VLNEXT: usize = 15;
    pub(super) const VEOL2: usize = 16;

    // The `C_*` control-character defaults, in `C_*` order. These come from
    // `<sys/ttydefaults.h>`, which glibc copied verbatim from BSD, so unlike
    // the numbering above they *are* portable — parameterised only by
    // [`VDISABLE`]. `tty.h`'s own fallbacks supply the six the header does
    // not define. Note `CMIN`/`CTIME` are 1 and 0 here, not the nonsense
    // `CEOF`/`CEOL` of `tty.h`'s fallback (ERR-terminal-43), which is
    // therefore unreachable on any platform that ships the header.
    const fn ctrl(c: u8) -> u8 {
        c & 0o37
    }
    pub(super) const CINTR: u8 = ctrl(b'c');
    pub(super) const CQUIT: u8 = 0o34;
    pub(super) const CERASE: u8 = 0o177;
    pub(super) const CKILL: u8 = ctrl(b'u');
    pub(super) const CEOF: u8 = ctrl(b'd');
    pub(super) const CEOL: u8 = VDISABLE;
    pub(super) const CEOL2: u8 = VDISABLE;
    pub(super) const CSWTCH: u8 = VDISABLE;
    pub(super) const CDSWTCH: u8 = VDISABLE;
    pub(super) const CERASE2: u8 = VDISABLE;
    pub(super) const CSTART: u8 = ctrl(b'q');
    pub(super) const CSTOP: u8 = ctrl(b's');
    pub(super) const CWERASE: u8 = ctrl(b'w');
    pub(super) const CSUSP: u8 = ctrl(b'z');
    pub(super) const CDSUSP: u8 = ctrl(b'y');
    pub(super) const CREPRINT: u8 = ctrl(b'r');
    pub(super) const CDISCARD: u8 = ctrl(b'o');
    pub(super) const CLNEXT: u8 = ctrl(b'v');
    pub(super) const CSTATUS: u8 = VDISABLE;
    pub(super) const CPAGE: u8 = b' ';
    pub(super) const CPGOFF: u8 = ctrl(b'm');
    pub(super) const CKILL2: u8 = VDISABLE;
    pub(super) const CBRK: u8 = VDISABLE;
    pub(super) const CMIN: u8 = 1;
    pub(super) const CTIME: u8 = 0;

    /// The three signal numbers `tty_get_signal_character` switches on.
    ///
    /// `sig.rs` carries the same numbers in its own private `signo` module
    /// for the same reason — POSIX names signals without fixing their
    /// numbers, and the libc that would define them is barred. Idiomatization
    /// should hoist one copy somewhere both modules can see it. `SIGINFO`,
    /// the fourth arm, is BSD-only and is not compiled here; neither is
    /// `VSTATUS`, so the arm needs both halves it does not have.
    pub(super) const SIGINT: i32 = 2;
    pub(super) const SIGQUIT: i32 = 3;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    pub(super) const SIGTSTP: i32 = 20;
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    pub(super) const SIGTSTP: i32 = 18;

    /// `isatty(fd)`. Real: `std::io::IsTerminal` answers it without libc.
    pub(super) fn isatty(fd: i32) -> bool {
        if fd < 0 {
            return false;
        }
        // SAFETY: the descriptor is the application's and stays open for the
        // life of the `EditLine`; `BorrowedFd` does not own or close it.
        unsafe { BorrowedFd::borrow_raw(fd) }.is_terminal()
    }

    /// `tcgetattr(fd, t)`. Returns the C's 0 or -1.
    ///
    /// **Unreachable.** This is the gap: there is no libc to call and no
    /// std equivalent. It reports the C's -1, which every caller in this file
    /// already handles as "the terminal would not answer".
    pub(super) fn tcgetattr(_fd: i32, _t: &mut Termios) -> i32 {
        -1
    }

    /// `tcsetattr(fd, action, t)`. Returns the C's 0 or -1. Unreachable for
    /// the same reason as [`tcgetattr`], and reporting failure for the same
    /// reason.
    pub(super) fn tcsetattr(_fd: i32, _action: i32, _t: &Termios) -> i32 {
        -1
    }

    /// `cfgetospeed(t)` — the encoded `B*` value in the `CBAUD` bits of
    /// `c_cflag`, which is where Linux keeps it.
    pub(super) fn cfgetospeed(t: &Termios) -> u32 {
        t.c_cflag & (CBAUD | CBAUDEX)
    }

    /// `cfgetispeed(t)`.
    ///
    /// **A known divergence, and the reason it is here rather than hidden.**
    /// [`Termios`] is a `def`-rule type this translation may not change, and
    /// it has no `c_ispeed`/`c_ospeed` members — so the port can carry only
    /// one speed. glibc encodes "input speed 0" in a private `c_iflag` bit
    /// and the BSDs keep a separate `c_ispeed` field; neither is expressible
    /// here. The consequence is confined and stated: `tty__getspeed`'s
    /// `spd == 0` branch, which exists to spell POSIX's "an input speed of
    /// `B0` means the input speed equals the output speed", can never be
    /// taken, because this always answers the output speed already. The value
    /// `tty__getspeed` returns is the same either way.
    pub(super) fn cfgetispeed(t: &Termios) -> u32 {
        cfgetospeed(t)
    }

    /// `cfsetospeed(t, speed)`. The C discards the return; so does every
    /// caller here.
    pub(super) fn cfsetospeed(t: &mut Termios, speed: u32) {
        t.c_cflag = (t.c_cflag & !(CBAUD | CBAUDEX)) | (speed & (CBAUD | CBAUDEX));
    }

    /// `cfsetispeed(t, speed)`. With one stored speed this is
    /// [`cfsetospeed`]; see [`cfgetispeed`].
    pub(super) fn cfsetispeed(t: &mut Termios, speed: u32) {
        cfsetospeed(t, speed);
    }
}

/// C: `#define TERM_CAN_TAB 0x008` and `#define EL_CAN_TAB (EL_FLAGS &
/// TERM_CAN_TAB)`, from `terminal.h`.
///
/// `terminal.rs` does not publish the `TERM_*` flag bits yet and is being
/// written in parallel, so the one bit this module needs is spelled out here
/// rather than added to a file that is not mine. Idiomatization should fold
/// it into `terminal.rs` and delete this.
const TERM_CAN_TAB: i32 = 0x008;

/// C: `EL_CAN_TAB` — the terminfo capability bit for "the terminal can use
/// hardware tabs".
fn el_can_tab(el: &EditLine) -> bool {
    el.el_terminal.t_flags & TERM_CAN_TAB != 0
}

/// C: `static const ttyperm_t ttyperm` — the compiled-in mode-permission
/// table `tty_init` copies into `el_tty.t_t`.
///
/// A function rather than a `static`, because [`TtypermEntry`] is neither
/// `Copy` nor `Clone` and the C's use of it is a `memcpy` into a *writable*
/// per-`EditLine` copy. Building a fresh one is that copy.
///
/// Two entries are surprising but intended, and both are the C's:
/// `ED_IO` sets `INLCR` *and* `ICRNL`, so while editing a received CR is
/// delivered as NL and a received NL as CR; and `ED_IO` sets `ISIG`, so the
/// signal characters stay live in the kernel during editing.
///
/// Public because [`TtypermEntry`] is neither `Default` nor `Clone`, so
/// whoever builds an [`ElTtyT`] has no other way to fill `t_t`. [`tty_init`]
/// overwrites it unconditionally, so using this as the initial value is both
/// the cheapest choice and the one the C reaches a moment later anyway.
pub const fn ttyperm() -> TtypermT {
    use plat::*;
    [
        // EX_IO — execute / cooked.
        [
            TtypermEntry {
                t_name: "iflag:",
                t_setmask: ICRNL,
                t_clrmask: INLCR | IGNCR,
            },
            TtypermEntry {
                t_name: "oflag:",
                t_setmask: OPOST | ONLCR,
                t_clrmask: ONLRET,
            },
            TtypermEntry {
                t_name: "cflag:",
                t_setmask: 0,
                t_clrmask: 0,
            },
            TtypermEntry {
                t_name: "lflag:",
                t_setmask: ISIG | ICANON | ECHO | ECHOE | ECHOCTL | IEXTEN,
                t_clrmask: NOFLSH | ECHONL | EXTPROC | FLUSHO,
            },
            TtypermEntry {
                t_name: "chars:",
                t_setmask: 0,
                t_clrmask: 0,
            },
        ],
        // ED_IO — editing / raw.
        [
            TtypermEntry {
                t_name: "iflag:",
                t_setmask: INLCR | ICRNL,
                t_clrmask: IGNCR,
            },
            TtypermEntry {
                t_name: "oflag:",
                t_setmask: OPOST | ONLCR,
                t_clrmask: ONLRET,
            },
            TtypermEntry {
                t_name: "cflag:",
                t_setmask: 0,
                t_clrmask: 0,
            },
            TtypermEntry {
                t_name: "lflag:",
                t_setmask: ISIG,
                t_clrmask: NOFLSH | ICANON | ECHO | ECHOK | ECHONL | EXTPROC | IEXTEN | FLUSHO,
            },
            TtypermEntry {
                t_name: "chars:",
                t_setmask: c_sh(C_MIN)
                    | c_sh(C_TIME)
                    | c_sh(C_SWTCH)
                    | c_sh(C_DSWTCH)
                    | c_sh(C_SUSP)
                    | c_sh(C_DSUSP)
                    | c_sh(C_EOL)
                    | c_sh(C_DISCARD)
                    | c_sh(C_PGOFF)
                    | c_sh(C_PAGE)
                    | c_sh(C_STATUS),
                t_clrmask: 0,
            },
        ],
        // QU_IO — quoted insert.
        [
            TtypermEntry {
                t_name: "iflag:",
                t_setmask: 0,
                t_clrmask: IXON | IXOFF | INLCR | ICRNL,
            },
            TtypermEntry {
                t_name: "oflag:",
                t_setmask: 0,
                t_clrmask: 0,
            },
            TtypermEntry {
                t_name: "cflag:",
                t_setmask: 0,
                t_clrmask: 0,
            },
            TtypermEntry {
                t_name: "lflag:",
                t_setmask: 0,
                t_clrmask: ISIG | IEXTEN,
            },
            TtypermEntry {
                t_name: "chars:",
                t_setmask: 0,
                t_clrmask: 0,
            },
        ],
    ]
}

/// C: `static const ttychar_t ttychar` — the control characters libedit
/// wants in each mode, columns in `C_*` order.
///
/// `ED_IO` keeps interrupt, quit, erase, kill, flow control, suspend and
/// discard live in the kernel, takes EOF, word-erase, reprint and
/// literal-next over itself by disabling them, and asks for
/// one-byte-at-a-time reads with no timer. Row 2 is `TS_IO`'s scratch row,
/// which is only ever written from the terminal, so its zeros are just an
/// initial value.
const TTYCHAR: TtycharT = {
    use plat::*;
    [
        [
            CINTR, CQUIT, CERASE, CKILL, CEOF, CEOL, CEOL2, CSWTCH, CDSWTCH, CERASE2, CSTART,
            CSTOP, CWERASE, CSUSP, CDSUSP, CREPRINT, CDISCARD, CLNEXT, CSTATUS, CPAGE, CPGOFF,
            CKILL2, CBRK, CMIN, CTIME,
        ],
        [
            CINTR, CQUIT, CERASE, CKILL, VDISABLE, VDISABLE, VDISABLE, VDISABLE, VDISABLE, CERASE2,
            CSTART, CSTOP, VDISABLE, CSUSP, VDISABLE, VDISABLE, CDISCARD, VDISABLE, VDISABLE,
            VDISABLE, VDISABLE, VDISABLE, VDISABLE, 1, 0,
        ],
        [0; C_NCC],
    ]
};

/// C: `static const ttymap_t tty_map[]` — which editor action each tty
/// control character should invoke.
///
/// The C's `{(wint_t)-1, (wint_t)-1, ...}` sentinel is the slice length here.
/// Rows exist only where the platform defines the `V*` subscript, so the
/// `VERASE2` and `VKILL2` rows — BSD-only — are absent, and those two
/// characters are simply never bound.
static TTY_MAP: [TtymapT; 6] = [
    TtymapT {
        nch: C_ERASE as u32,
        och: plat::VERASE as u32,
        bind: [EM_DELETE_PREV_CHAR, VI_DELETE_PREV_CHAR, ED_PREV_CHAR],
    },
    TtymapT {
        nch: C_KILL as u32,
        och: plat::VKILL as u32,
        bind: [EM_KILL_LINE, VI_KILL_LINE_PREV, ED_UNASSIGNED],
    },
    TtymapT {
        nch: C_EOF as u32,
        och: plat::VEOF as u32,
        bind: [EM_DELETE_OR_LIST, VI_LIST_OR_EOF, ED_UNASSIGNED],
    },
    TtymapT {
        nch: C_WERASE as u32,
        och: plat::VWERASE as u32,
        bind: [ED_DELETE_PREV_WORD, ED_DELETE_PREV_WORD, ED_PREV_WORD],
    },
    TtymapT {
        nch: C_REPRINT as u32,
        och: plat::VREPRINT as u32,
        bind: [ED_REDISPLAY, ED_INSERT, ED_REDISPLAY],
    },
    TtymapT {
        nch: C_LNEXT as u32,
        och: plat::VLNEXT as u32,
        bind: [ED_QUOTED_INSERT, ED_QUOTED_INSERT, ED_UNASSIGNED],
    },
];

/// C: `static const ttymodes_t ttymodes[]` — the `setty` vocabulary.
///
/// The C's `{NULL, 0, -1}` sentinel is the slice length. Entries exist only
/// where the platform defines the macro, and the grouping by `m_type` in
/// `MD_INP`, `MD_OUT`, `MD_CTL`, `MD_LIN`, `MD_CHAR` order is load-bearing:
/// [`tty_stty`]'s display form starts a new labelled group every time
/// `m_type` changes, so a re-ordered table prints repeated headings. This is
/// what the "Don't re-order" comment on the `MD_*` constants protects.
///
/// The BSD-only names — `onoeot`, `pageout`, `wrap`, `cignore`, `loblk`,
/// `ccts_oflow`, `crts_iflow`, `cdtrcts`, `mdmbuf`, `rcv1en`, `xmt1en`,
/// `defecho`, `nokerninfo`, `altwerase` — are absent, as are the `MD_CHAR`
/// entries whose `V*` subscript this platform lacks: `swtch`, `dswtch`,
/// `erase2`, `dsusp`, `status`, `page`, `pgoff`, `kill2` and `brk`. `setty`
/// answers "Invalid argument" for each, which is exactly what the C compiled
/// on this platform does.
///
/// Two hazards inherited from the C and worth knowing at the call site:
/// `cbaud` and `cibaud` overlap the line-speed encoding, so flipping them
/// corrupts the speed; and `xtabs` is a `TABDLY` value rather than an
/// independent bit, so it interacts with `tabdly`.
static TTYMODES: [TtymodesT; 70] = {
    use plat::*;
    const fn inp(m_name: &'static str, m_value: u32) -> TtymodesT {
        TtymodesT {
            m_name,
            m_value,
            m_type: MD_INP as i32,
        }
    }
    const fn out(m_name: &'static str, m_value: u32) -> TtymodesT {
        TtymodesT {
            m_name,
            m_value,
            m_type: MD_OUT as i32,
        }
    }
    const fn ctl(m_name: &'static str, m_value: u32) -> TtymodesT {
        TtymodesT {
            m_name,
            m_value,
            m_type: MD_CTL as i32,
        }
    }
    const fn lin(m_name: &'static str, m_value: u32) -> TtymodesT {
        TtymodesT {
            m_name,
            m_value,
            m_type: MD_LIN as i32,
        }
    }
    const fn chr(m_name: &'static str, c: usize) -> TtymodesT {
        TtymodesT {
            m_name,
            m_value: c_sh(c),
            m_type: MD_CHAR as i32,
        }
    }
    [
        inp("ignbrk", IGNBRK),
        inp("brkint", BRKINT),
        inp("ignpar", IGNPAR),
        inp("parmrk", PARMRK),
        inp("inpck", INPCK),
        inp("istrip", ISTRIP),
        inp("inlcr", INLCR),
        inp("igncr", IGNCR),
        inp("icrnl", ICRNL),
        inp("iuclc", IUCLC),
        inp("ixon", IXON),
        inp("ixany", IXANY),
        inp("ixoff", IXOFF),
        inp("imaxbel", IMAXBEL),
        out("opost", OPOST),
        out("olcuc", OLCUC),
        out("onlcr", ONLCR),
        out("ocrnl", OCRNL),
        out("onocr", ONOCR),
        out("onlret", ONLRET),
        out("ofill", OFILL),
        out("ofdel", OFDEL),
        out("nldly", NLDLY),
        out("crdly", CRDLY),
        out("tabdly", TABDLY),
        out("xtabs", XTABS),
        out("bsdly", BSDLY),
        out("vtdly", VTDLY),
        out("ffdly", FFDLY),
        ctl("cbaud", CBAUD),
        ctl("cstopb", CSTOPB),
        ctl("cread", CREAD),
        ctl("parenb", PARENB),
        ctl("parodd", PARODD),
        ctl("hupcl", HUPCL),
        ctl("clocal", CLOCAL),
        ctl("cibaud", CIBAUD),
        ctl("crtscts", CRTSCTS),
        lin("isig", ISIG),
        lin("icanon", ICANON),
        lin("xcase", XCASE),
        lin("echo", ECHO),
        lin("echoe", ECHOE),
        lin("echok", ECHOK),
        lin("echonl", ECHONL),
        lin("noflsh", NOFLSH),
        lin("tostop", TOSTOP),
        lin("echoctl", ECHOCTL),
        lin("echoprt", ECHOPRT),
        lin("echoke", ECHOKE),
        lin("flusho", FLUSHO),
        lin("pendin", PENDIN),
        lin("iexten", IEXTEN),
        lin("extproc", EXTPROC),
        chr("intr", C_INTR),
        chr("quit", C_QUIT),
        chr("erase", C_ERASE),
        chr("kill", C_KILL),
        chr("eof", C_EOF),
        chr("eol", C_EOL),
        chr("eol2", C_EOL2),
        chr("start", C_START),
        chr("stop", C_STOP),
        chr("werase", C_WERASE),
        chr("susp", C_SUSP),
        chr("reprint", C_REPRINT),
        chr("discard", C_DISCARD),
        chr("lnext", C_LNEXT),
        chr("min", C_MIN),
        chr("time", C_TIME),
    ]
};

/// The C's `td->c_cc[VXXX]` as a read.
///
/// The port's `c_cc` is a `Vec`, so a row shorter than [`NCCS`] — which only
/// a caller who built a [`Termios`] by hand can produce — reads as 0 instead
/// of running off the end. [`termios_zeroed`] sizes every row this module
/// creates, so the fallback is not reachable from inside it.
fn cc_get(td: &Termios, v: usize) -> u8 {
    td.c_cc.get(v).copied().unwrap_or(0)
}

/// The C's `td->c_cc[VXXX] = b`. Same bounds note as [`cc_get`]: a write past
/// the end is dropped rather than being the C's out-of-bounds store.
fn cc_set(td: &mut Termios, v: usize, b: u8) {
    if let Some(slot) = td.c_cc.get_mut(v) {
        *slot = b;
    }
}

/// C: `#define tty__gettabs(td) ((((td)->c_oflag & TAB3) == TAB3) ? 0 : 1)`.
#[allow(non_snake_case)]
fn tty__gettabs(td: &Termios) -> i32 {
    i32::from(td.c_oflag & plat::TAB3 != plat::TAB3)
}

/// C: `#define tty__geteightbit(td) (((td)->c_cflag & CSIZE) == CS8)`.
#[allow(non_snake_case)]
fn tty__geteightbit(td: &Termios) -> i32 {
    i32::from(td.c_cflag & plat::CSIZE == plat::CS8)
}

/// C: `#define tty__cooked_mode(td) ((td)->c_lflag & ICANON)`.
#[allow(non_snake_case)]
fn tty__cooked_mode(td: &Termios) -> bool {
    td.c_lflag & plat::ICANON != 0
}

/// Which of `el_tty`'s four `struct termios` a call operates on.
#[derive(Clone, Copy)]
enum Tios {
    /// `t_or` — the original, captured once by `tty_setup`.
    Or,
    /// `t_ex` — execute / cooked.
    Ex,
    /// `t_ed` — edit / raw.
    Ed,
    /// `t_ts` — the terminal snapshot, and `t_qu` under its other name.
    Ts,
}

fn tios_slot(el: &mut EditLine, which: Tios) -> &mut Termios {
    match which {
        Tios::Or => &mut el.el_tty.t_or,
        Tios::Ex => &mut el.el_tty.t_ex,
        Tios::Ed => &mut el.el_tty.t_ed,
        Tios::Ts => &mut el.el_tty.t_ts,
    }
}

/// Run `f` with one of `el_tty`'s termios moved out of the `EditLine`.
///
/// Three signatures in this module take both `&mut EditLine` and a
/// `&(mut) Termios`, and at every call site the second is a field of the
/// first — an aliasing pair Rust will not form. The signatures are `def`-rule
/// text and stay the C's, so the call sites move the field out for the
/// duration of the call and put it back.
///
/// The invariant that makes this exact rather than merely safe: while `f`
/// runs, the slot holds a zero-length placeholder. Every callee is checked
/// against that — `tty_getty` and `tty_setty` read only `el_infd`, and
/// `tty_setup_flags` reads only `el_tty.t_t` — so none of them can observe
/// the hole. Nothing else may be called through here without the same check.
fn with_tios<R>(
    el: &mut EditLine,
    which: Tios,
    f: impl FnOnce(&mut EditLine, &mut Termios) -> R,
) -> R {
    let placeholder = Termios {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_cc: Vec::new(),
    };
    let mut t = std::mem::replace(tios_slot(el, which), placeholder);
    let r = f(el, &mut t);
    *tios_slot(el, which) = t;
    r
}

/// The C's whole-struct `*dst = *src` on a `struct termios`.
fn termios_copy(dst: &mut Termios, src: &Termios) {
    dst.c_iflag = src.c_iflag;
    dst.c_oflag = src.c_oflag;
    dst.c_cflag = src.c_cflag;
    dst.c_lflag = src.c_lflag;
    dst.c_cc.clear();
    dst.c_cc.extend_from_slice(&src.c_cc);
}

/// C: `fprintf(el->el_outfile, …)` for an already-formatted byte string.
///
/// The stream is a caller-owned `FILE *` the port cannot write through, so
/// this goes to the matching descriptor, which the `EditLine` carries for
/// exactly this reason (`def:el.editline`). Errors are discarded, as the C
/// discards `fprintf`'s result. `hist.rs` carries a private twin; one of them
/// should survive idiomatization.
fn write_outfile(el: &EditLine, bytes: &[u8]) {
    write_fd(el.el_outfd, bytes);
}

/// C: `fprintf(el->el_errfile, …)`. Same reasoning as [`write_outfile`].
fn write_errfile(el: &EditLine, bytes: &[u8]) {
    write_fd(el.el_errfd, bytes);
}

fn write_fd(fd: i32, bytes: &[u8]) {
    if fd < 0 {
        return;
    }
    // SAFETY: the descriptor is the application's and stays open for the life
    // of the `EditLine`; `ManuallyDrop` is what keeps this borrow from
    // closing it, which libedit never does.
    let mut out = ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) });
    let _ = out.write_all(bytes);
}

/// The C's `s[i]` on a NUL-terminated `const wchar_t *`, made total: the
/// slices this module receives carry content only, so one past the end reads
/// as the terminator.
fn wcs(s: &[u32]) -> &[u32] {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    &s[..n]
}

/// `ct_encode_string`, copied out of the scratch buffer.
///
/// The C hands the caller a pointer into `el->el_scratch` that stays valid
/// only until the next call on the same buffer; every use in `tty_stty`
/// either compares it immediately or prints it, so an owned copy is not
/// observable and it is what lets the rest of the `EditLine` stay borrowable.
///
/// `ct_encode_string` returns NULL for a NULL input or an allocation failure
/// and the C checks for neither, handing it to `strlcpy`, `strcmp` and
/// `fprintf` (ERR-terminal-07). Defined here as the empty string.
fn encode(el: &mut EditLine, s: &[u32]) -> Vec<u8> {
    match ct_encode_string(Some(s), &mut el.el_scratch) {
        Some(b) => b.to_vec(),
        None => Vec::new(),
    }
}

/// `strncmp(a, b, n) == 0` over two strings that are not NUL terminated in
/// this port. Reading past either end as the terminator is what makes this
/// the C's comparison exactly, including `n == 0` matching everything.
fn strncmp_eq(a: &[u8], b: &[u8], n: usize) -> bool {
    for i in 0..n {
        let ca = a.get(i).copied().unwrap_or(0);
        let cb = b.get(i).copied().unwrap_or(0);
        if ca != cb {
            return false;
        }
        if ca == 0 {
            return true;
        }
    }
    true
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
    // Note the descriptor: the attributes read are the **input** fd's, even
    // though `tty_setup`'s `isatty` guard tests `el_outfd` (ERR-terminal-35,
    // disposition `reproduce`).
    //
    // The C's `while (rv == -1 && errno == EINTR) continue;` has no loop to
    // run here: `plat::tcgetattr` is not a syscall and cannot be interrupted,
    // so it either succeeds or fails once. The retry returns with the
    // primitive.
    plat::tcgetattr(el.el_infd, t)
}

// [spec:libedit:def:tty.tty-setty-fn]
// [spec:libedit:sem:tty.tty-setty-fn]
/// C: `static int tty_setty(EditLine *el, int action, const struct termios
/// *t)` — `tcsetattr` on `el->el_infd`, retried on EINTR. Same aliasing note
/// as [`tty_getty`].
fn tty_setty(el: &mut EditLine, action: i32, t: &Termios) -> i32 {
    // `action` is `TCSANOW`/`TCSADRAIN`/`TCSAFLUSH` and is observable timing
    // behaviour, so it is passed through untouched. Writes go to the **input**
    // fd, matching `tty_getty`.
    //
    // Caveat inherited from POSIX and not compensated for anywhere: a
    // successful `tcsetattr` means *some* of the requested changes were
    // applied, not all, and libedit never reads the settings back to check.
    //
    // Same note on the EINTR loop as `tty_getty`.
    plat::tcsetattr(el.el_infd, action, t)
}

// [spec:libedit:def:tty.tty-setup-fn]
// [spec:libedit:sem:tty.tty-setup-fn]
fn tty_setup(el: &mut EditLine) -> i32 {
    // Step 1. Whether libedit may write the terminal's control characters at
    // startup. The readline compatibility layer sets `NO_RESET`.
    let rst = el.el_flags & NO_RESET == 0;

    // Step 2. Success, but nothing done — and `t_initialized` stays 0, so
    // `tty_end` will decline to restore anything later either.
    if el.el_flags & EDIT_DISABLED != 0 {
        return 0;
    }

    // Step 3. Only `tty_init` clears the flag, so setup runs exactly once per
    // `tty_init`.
    if el.el_tty.t_initialized != 0 {
        return -1;
    }

    // Step 4. ERR-terminal-35: the guard tests the **output** fd while every
    // termios call below uses the **input** fd. When they differ and input is
    // not a terminal, step 5 fails and setup still returns -1, so the
    // asymmetry degrades safely. Reproduced, including the descriptor.
    if !plat::isatty(el.el_outfd) {
        return -1;
    }

    // Step 5. The sole capture of the original state; `t_or` is exactly what
    // `tty_end` writes back and nothing else ever assigns it.
    if with_tios(el, Tios::Or, tty_getty) == -1 {
        return -1;
    }

    // Step 6. Three whole-struct copies, so all four termios start identical.
    {
        let tty = &mut el.el_tty;
        termios_copy(&mut tty.t_ex, &tty.t_or);
    }
    {
        let tty = &mut el.el_tty;
        termios_copy(&mut tty.t_ed, &tty.t_or);
    }
    {
        let tty = &mut el.el_tty;
        termios_copy(&mut tty.t_ts, &tty.t_or);
    }

    // Step 7. Derived from `t_ex`, which at this point still equals `t_or`.
    el.el_tty.t_speed = tty__getspeed(&el.el_tty.t_ex);
    el.el_tty.t_tabs = tty__gettabs(&el.el_tty.t_ex);
    el.el_tty.t_eight = tty__geteightbit(&el.el_tty.t_ex);

    // Step 8.
    with_tios(el, Tios::Ex, |el, t| tty_setup_flags(el, t, EX_IO as i32));

    // Step 9. Reset the terminal's control characters to sane values.
    if rst {
        // 9a. Only trust the terminal's characters if it was left canonical.
        if tty__cooked_mode(&el.el_tty.t_ts) {
            tty__getchar(&el.el_tty.t_ts, &mut el.el_tty.t_c[TS_IO]);

            let vdis = el.el_tty.t_vdisable;
            // `0 .. C_NCC-2`, excluding `C_MIN` and `C_TIME`, so edit mode
            // keeps `VMIN == 1`, `VTIME == 0`. The editor row adopts the
            // user's character only where *both* values are enabled: a
            // character libedit deliberately disables in edit mode stays
            // disabled, and one the user disabled is not propagated.
            for i in 0..C_NCC - 2 {
                if el.el_tty.t_c[TS_IO][i] != vdis && el.el_tty.t_c[ED_IO][i] != vdis {
                    el.el_tty.t_c[ED_IO][i] = el.el_tty.t_c[TS_IO][i];
                }
            }
            // All indices this time.
            for i in 0..C_NCC {
                if el.el_tty.t_c[TS_IO][i] != vdis {
                    el.el_tty.t_c[EX_IO][i] = el.el_tty.t_c[TS_IO][i];
                }
            }
        }

        // 9b.
        {
            let tty = &mut el.el_tty;
            let row = tty.t_c[EX_IO];
            tty__setchar(&mut tty.t_ex, &row);
        }

        // 9c. The only terminal write `tty_setup` performs, and it happens
        // **before** `t_initialized` is set — so a failure here leaves the
        // terminal possibly modified while `tty_end` will decline to restore
        // it (ERR-terminal-41, disposition `reproduce`).
        if with_tios(el, Tios::Ex, |el, t| tty_setty(el, plat::TCSADRAIN, t)) == -1 {
            return -1;
        }
    }
    // If `rst` is false: no `tcsetattr` at all, the `EX_IO` row keeps the
    // compiled-in defaults, and the terminal is left untouched until the
    // first `tty_rawmode`.

    // Step 10. Always done, `rst` or not.
    with_tios(el, Tios::Ed, |el, t| tty_setup_flags(el, t, ED_IO as i32));

    // Step 11.
    {
        let tty = &mut el.el_tty;
        let row = tty.t_c[ED_IO];
        tty__setchar(&mut tty.t_ed, &row);
    }

    // Step 12. Forced, because step 11 has just made `t_ed.c_cc` agree with
    // `t_c[ED_IO]` for every mapped character, so a non-forced call would
    // find no differences and bind nothing.
    tty_bind_char(el, 1);

    // Step 13. `t_mode` is not assigned here — `tty_init` set it before
    // calling.
    el.el_tty.t_initialized = 1;
    0
}

// [spec:libedit:def:tty.tty-init-fn]
// [spec:libedit:sem:tty.tty-init-fn]
pub(crate) fn tty_init(el: &mut EditLine) -> i32 {
    // Step 1. The mode is *asserted*, not observed: if the terminal happens
    // to be raw at this point, libedit now disagrees with reality.
    el.el_tty.t_mode = EX_IO as u8;

    // Step 2. Platform-conditional, and the split is observable — see
    // `plat::VDISABLE` and ERR-terminal-42.
    el.el_tty.t_vdisable = plat::VDISABLE;

    // Step 3. The only place the flag is ever cleared, which is what makes
    // re-initialisation work.
    el.el_tty.t_initialized = 0;

    // Steps 4 and 5. Writable per-`EditLine` copies of the two compiled-in
    // tables. Because they are unconditional, calling `tty_init` again
    // discards every `setty` customisation and reverts to the built-in
    // defaults; the readline layer does exactly this on each `readline()`.
    el.el_tty.t_t = ttyperm();
    el.el_tty.t_c = TTYCHAR;

    // Step 6.
    tty_setup(el)
}

// [spec:libedit:def:tty.tty-end-fn]
// [spec:libedit:sem:tty.tty-end-fn]
pub(crate) fn tty_end(el: &mut EditLine, how: i32) {
    // Step 1.
    if el.el_flags & EDIT_DISABLED != 0 {
        return;
    }

    // Step 2. The "restoration skipped" path: setup either never ran or
    // failed, in which case libedit never changed the terminal either, so it
    // is left as the application had it.
    if el.el_tty.t_initialized == 0 {
        return;
    }

    // Step 3. `how` is the caller's `TCSANOW`/`TCSADRAIN`/`TCSAFLUSH` and is
    // observable timing behaviour, so it is passed through unchanged: `el_end`
    // uses `TCSAFLUSH`, the readline layer `TCSADRAIN`. A -1 is swallowed.
    with_tios(el, Tios::Or, |el, t| tty_setty(el, how, t));

    // Deliberately not done, and observable (ERR-terminal-39, disposition
    // `reproduce`): `t_initialized` is not cleared and `t_mode` is not reset.
    // If this runs while `t_mode` is `ED_IO` or `QU_IO` the terminal is cooked
    // again but libedit still believes it is raw, and the next `tty_rawmode`
    // returns 0 without re-applying anything — leaving the terminal cooked
    // during editing. The normal shutdown path avoids it because `el_end`
    // calls `el_reset` -> `tty_cookedmode` first; the readline layer's
    // per-`readline()` `tty_end(e, TCSADRAIN)` is the exposed route.
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
    // The zero test is POSIX's convention that an input speed of `B0` means
    // "the input speed equals the output speed"; a genuine `B0` *output*
    // speed (hang up the line) is therefore reported as speed 0. The port
    // carries one speed rather than two, so the branch cannot be taken — see
    // `plat::cfgetispeed`, which is where that limitation is stated. The
    // value returned is the same either way.
    let spd = plat::cfgetispeed(td);
    if spd == 0 { plat::cfgetospeed(td) } else { spd }
}

// [spec:libedit:def:tty.tty-getcharindex-fn]
// [spec:libedit:sem:tty.tty-getcharindex-fn]
/// Maps one of libedit's `C_*` indices to the termios `V*` subscript, or -1
/// when the platform has no such control character.
///
/// [`VSUB`] is the whole function: a total table over `0 .. C_NCC`, with
/// `None` for every index the platform does not define — and for `C_BRK`,
/// which the C has no case for on *any* platform even where `VBRK` exists.
/// That gap is what ERR-terminal-05 turns into an out-of-bounds write; see
/// [`tty_stty`] for how the port defines it.
#[allow(non_snake_case)]
fn tty__getcharindex(i: i32) -> i32 {
    match usize::try_from(i).ok().and_then(|i| VSUB.get(i).copied()) {
        Some(Some(v)) => v as i32,
        // Out of range, an index the platform has no subscript for, `C_BRK`,
        // and `C_NCC` itself — which is a count, not an index.
        _ => -1,
    }
}

/// The `C_*` -> `V*` map, which is `tty__getcharindex`, `tty__getchar` and
/// `tty__setchar` all three: the C spells the same conditional pair list out
/// in each, and the three lists are identical.
const VSUB: [Option<usize>; C_NCC] = {
    use plat::*;
    let mut v = [None; C_NCC];
    v[C_INTR] = Some(VINTR);
    v[C_QUIT] = Some(VQUIT);
    v[C_ERASE] = Some(VERASE);
    v[C_KILL] = Some(VKILL);
    v[C_EOF] = Some(VEOF);
    v[C_EOL] = Some(VEOL);
    v[C_EOL2] = Some(VEOL2);
    // C_SWTCH, C_DSWTCH, C_ERASE2: no VSWTCH / VDSWTCH / VERASE2 here.
    v[C_START] = Some(VSTART);
    v[C_STOP] = Some(VSTOP);
    v[C_WERASE] = Some(VWERASE);
    v[C_SUSP] = Some(VSUSP);
    // C_DSUSP: no VDSUSP here.
    v[C_REPRINT] = Some(VREPRINT);
    v[C_DISCARD] = Some(VDISCARD);
    v[C_LNEXT] = Some(VLNEXT);
    // C_STATUS, C_PAGE, C_PGOFF, C_KILL2: no VSTATUS / VPAGE / VPGOFF /
    // VKILL2 here. C_BRK: no case in the C, on any platform.
    v[C_MIN] = Some(VMIN);
    v[C_TIME] = Some(VTIME);
    v
};

// [spec:libedit:def:tty.tty-getchar-fn]
// [spec:libedit:sem:tty.tty-getchar-fn]
/// C: `static void tty__getchar(struct termios *td, unsigned char *s)` —
/// reads `td->c_cc` by `V*` subscript into `s` by `C_*` index. `s` is one row
/// of [`TtycharT`].
#[allow(non_snake_case)]
fn tty__getchar(td: &Termios, s: &mut [u8]) {
    // Slots whose `V*` subscript the platform lacks are **not written** —
    // they keep whatever the destination row already held, which is why the
    // caller must treat an unwritten slot as stale rather than as zero.
    // `C_BRK` is one of them on every platform.
    //
    // `VMIN`/`VTIME` are only meaningful in non-canonical mode and POSIX
    // permits them to share storage with `VEOF`/`VEOL`; all four are copied
    // unconditionally, so on a platform with that aliasing the row's
    // `C_EOF`/`C_EOL` and `C_MIN`/`C_TIME` entries carry the same bytes.
    for (c, sub) in VSUB.iter().enumerate() {
        if let (Some(v), Some(slot)) = (*sub, s.get_mut(c)) {
            *slot = cc_get(td, v);
        }
    }
}

// [spec:libedit:def:tty.tty-setchar-fn]
// [spec:libedit:sem:tty.tty-setchar-fn]
/// The inverse of [`tty__getchar`]: writes `s`, indexed by `C_*`, into
/// `td->c_cc`, indexed by `V*`.
#[allow(non_snake_case)]
fn tty__setchar(td: &mut Termios, s: &[u8]) {
    // Only `c_cc` is touched — no flag word, no speed — and nothing is pushed
    // to the terminal, so the change reaches it only when a caller writes the
    // struct out. Slots whose `V*` subscript is absent are not written, and
    // there is no `VBRK` assignment on any platform, so `C_BRK` is inert in
    // this direction too.
    for (c, sub) in VSUB.iter().enumerate() {
        if let (Some(v), Some(&b)) = (*sub, s.get(c)) {
            cc_set(td, v, b);
        }
    }
}

// [spec:libedit:def:tty.tty-bind-char-fn]
// [spec:libedit:sem:tty.tty-bind-char-fn]
pub(crate) fn tty_bind_char(el: &mut EditLine, force: i32) {
    // The maps are `N_KEYS` long from `map_init` onwards, and `map_init` runs
    // before `tty_init` in `el_init`, so this is a guard against a caller
    // order the C does not have rather than a reachable state. The C would
    // write through whatever `el_map.key` held.
    if el.el_map.key.len() < N_KEYS || el.el_map.alt.len() < N_KEYS {
        return;
    }

    // `MAP_VI == 1`, `MAP_EMACS == 0`, and `el_map.type` is never anything
    // else, which is what makes `bind[type]` and `bind[type + 1]` in range.
    let vi = el.el_map.r#type == MAP_VI;
    let ty = usize::from(vi);
    let dmap = if vi { el.el_map.vii } else { el.el_map.emacs };
    let dalt = if vi { el.el_map.vic } else { None };

    for tp in &TTY_MAP {
        let nch = tp.nch as usize;
        let och = tp.och as usize;

        // Both are bytes widened to `wchar_t`, so both are in 0..255 and
        // every map subscript below is in range.
        //
        // Nothing special-cases a *disabled* character (ERR-terminal-42,
        // disposition `reproduce`, platform split included): a disabled
        // character holds `t_vdisable`, so with the compiled-in
        // `t_c[ED_IO][C_EOF]` this binds byte 0x00 (NUL) to
        // `EM_DELETE_OR_LIST` / `VI_LIST_OR_EOF` on glibc, and byte 0xff on
        // the BSDs. It is observable through the key map.
        let newb = el.el_tty.t_c[ED_IO][nch];
        let oldb = cc_get(&el.el_tty.t_ed, och);

        // `force` processes the row even when nothing changed — that is how
        // `tty_setup` installs the initial bindings, since it writes
        // `t_ed.c_cc` from `t_c[ED_IO]` immediately before calling here,
        // making every pair equal.
        if newb == oldb && force == 0 {
            continue;
        }

        // The C's `wchar_t new[2], old[2]` with `new[1] = old[1] = '\0'`.
        let newk = [u32::from(newb), 0];
        let oldk = [u32::from(oldb), 0];

        // Order within a row matters: the old byte is restored to its
        // compiled-in default *before* the new byte is bound, so when the two
        // are the same byte the net effect is just the new binding.
        keymacro_clear(el, ElMapCurrent::Key, &oldk);
        // The C dereferences `dmap` with no NULL check; `map_end` sets these
        // to NULL, and nothing calls here afterwards. Defined as "no default
        // to restore, so leave the live binding alone".
        if let Some(d) = dmap {
            el.el_map.key[oldb as usize] = d[oldb as usize];
        }
        keymacro_clear(el, ElMapCurrent::Key, &newk);
        el.el_map.key[newb as usize] = tp.bind[ty];

        // vi only: `bind[type + 1]` is `bind[2]`, the command-mode action.
        if let Some(d) = dalt {
            keymacro_clear(el, ElMapCurrent::Alt, &oldk);
            el.el_map.alt[oldb as usize] = d[oldb as usize];
            keymacro_clear(el, ElMapCurrent::Alt, &newk);
            el.el_map.alt[newb as usize] = tp.bind[ty + 1];
        }
    }
}

// [spec:libedit:def:tty.tty-get-flag-fn]
// [spec:libedit:sem:tty.tty-get-flag-fn]
/// C: `static tcflag_t * tty__get_flag(struct termios *t, int kind)` — picks
/// one of the four mode words by `MD_INP`/`MD_OUT`/`MD_CTL`/`MD_LIN`, and
/// aborts on anything else. `tcflag_t` is the `u32` [`Termios`] uses.
///
/// ERR-terminal-44's disposition is `fix`: express the mapping as a total
/// function over the four flag words rather than reproduce the `abort()`.
/// The signature is `def`-rule text and keeps the C's `int`, so the totality
/// is enforced at the call sites instead — [`tty_setup_flags`] and
/// [`tty_update_flags`] are the only ones, both loop `MD_INP..=MD_LIN`, and
/// [`tty_get_signal_character`] passes a literal. `MD_CHAR` is a bitmap over
/// `C_*` indices with no flag word behind it and is never fed here.
#[allow(non_snake_case)]
fn tty__get_flag(t: &mut Termios, kind: i32) -> &mut u32 {
    match kind {
        k if k == MD_INP as i32 => &mut t.c_iflag,
        k if k == MD_OUT as i32 => &mut t.c_oflag,
        k if k == MD_CTL as i32 => &mut t.c_cflag,
        k if k == MD_LIN as i32 => &mut t.c_lflag,
        _ => unreachable!("tty__get_flag: {kind} is not a flag word (ERR-terminal-44)"),
    }
}

// [spec:libedit:def:tty.tty-update-flag-fn]
// [spec:libedit:sem:tty.tty-update-flag-fn]
fn tty_update_flag(el: &mut EditLine, f: u32, mode: i32, kind: i32) -> u32 {
    // Clear happens before set, so a bit in both masks is set. Bits in
    // neither are passed through unchanged — that is the mechanism by which
    // the terminal's own settings survive into libedit's modes.
    //
    // The table read is the live per-`EditLine` copy, so any `setty` edit is
    // reflected on the next call.
    let e = &el.el_tty.t_t[mode as usize][kind as usize];
    (f & !e.t_clrmask) | e.t_setmask
}

// [spec:libedit:def:tty.tty-update-flags-fn]
// [spec:libedit:sem:tty.tty-update-flags-fn]
fn tty_update_flags(el: &mut EditLine, kind: i32) {
    let tt = *tty__get_flag(&mut el.el_tty.t_ts, kind);
    let ed = *tty__get_flag(&mut el.el_tty.t_ed, kind);
    let ex = *tty__get_flag(&mut el.el_tty.t_ex, kind);

    // If the terminal's word still equals the execute word, nothing changed
    // since libedit last wrote it, so there is nothing to adopt. The extra
    // `MD_CTL` condition exists because libedit itself writes `c_cflag` — the
    // speed bits, via `cfsetispeed`/`cfsetospeed` in `tty_rawmode` — into
    // both stored words, so a `c_cflag` difference is only believed when the
    // snapshot differs from *both*. The C does not document this; it must not
    // be "simplified".
    if tt != ex && (kind != MD_CTL as i32 || tt != ed) {
        // Both new values derive from the snapshot, not from the previous
        // stored word, so any bit libedit had set that is in neither mask of
        // the target mode is discarded in favour of the terminal's value.
        let ned = tty_update_flag(el, tt, ED_IO as i32, kind);
        let nex = tty_update_flag(el, tt, EX_IO as i32, kind);
        *tty__get_flag(&mut el.el_tty.t_ed, kind) = ned;
        *tty__get_flag(&mut el.el_tty.t_ex, kind) = nex;
    }
}

// [spec:libedit:def:tty.tty-update-char-fn]
// [spec:libedit:sem:tty.tty-update-char-fn]
fn tty_update_char(el: &mut EditLine, mode: i32, c: i32) {
    let m = mode as usize;
    let i = c as usize;
    let bit = c_sh(i);

    // Step 1. A `t_setmask` bit in the `MD_CHAR` column marks the character
    // as libedit's own for that mode, so user changes are not adopted; and
    // the change is only interesting if the terminal's current value differs
    // from what libedit last pushed as the execute-mode value.
    if el.el_tty.t_t[m][MD_CHAR].t_setmask & bit == 0
        && el.el_tty.t_c[TS_IO][i] != el.el_tty.t_c[EX_IO][i]
    {
        el.el_tty.t_c[m][i] = el.el_tty.t_c[TS_IO][i];
    }

    // Step 2. Clear beats adopt: this overwrites whatever step 1 may have
    // just written.
    if el.el_tty.t_t[m][MD_CHAR].t_clrmask & bit != 0 {
        el.el_tty.t_c[m][i] = el.el_tty.t_vdisable;
    }
}

// [spec:libedit:def:tty.tty-rawmode-fn]
// [spec:libedit:sem:tty.tty-rawmode-fn]
pub fn tty_rawmode(el: &mut EditLine) -> i32 {
    // Step 1. Already raw or quoting: no syscall issued.
    if el.el_tty.t_mode == ED_IO as u8 || el.el_tty.t_mode == QU_IO as u8 {
        return 0;
    }

    // Step 2.
    if el.el_flags & EDIT_DISABLED != 0 {
        return 0;
    }

    // Step 3. Snapshot the terminal into the `TS_IO` scratch termios.
    if with_tios(el, Tios::Ts, tty_getty) == -1 {
        return -1;
    }

    // Step 4. Speed and the eight-bit setting are always believed; everything
    // else is believed only when the terminal was left canonical.
    el.el_tty.t_eight = tty__geteightbit(&el.el_tty.t_ts);
    el.el_tty.t_speed = tty__getspeed(&el.el_tty.t_ts);

    // Step 5. Note this forces input speed equal to output speed on both
    // structures even if only one of them was stale. Return values ignored.
    let speed = el.el_tty.t_speed;
    if tty__getspeed(&el.el_tty.t_ex) != speed || tty__getspeed(&el.el_tty.t_ed) != speed {
        plat::cfsetispeed(&mut el.el_tty.t_ex, speed);
        plat::cfsetospeed(&mut el.el_tty.t_ex, speed);
        plat::cfsetispeed(&mut el.el_tty.t_ed, speed);
        plat::cfsetospeed(&mut el.el_tty.t_ed, speed);
    }

    // Step 6. "The terminal is in cooked mode, so believe what we see."
    if tty__cooked_mode(&el.el_tty.t_ts) {
        // 6a.
        for kind in MD_INP..=MD_LIN {
            tty_update_flags(el, kind as i32);
        }

        // 6b. Note this inspects `t_ex`, which 6a may just have rewritten,
        // not the raw snapshot.
        if tty__gettabs(&el.el_tty.t_ex) == 0 {
            el.el_tty.t_tabs = 0;
        } else {
            el.el_tty.t_tabs = i32::from(el_can_tab(el));
        }

        // 6c.
        tty__getchar(&el.el_tty.t_ts, &mut el.el_tty.t_c[TS_IO]);

        // 6d. Did the user change anything? If not, the whole propagation
        // block is skipped.
        let changed = (0..C_NCC).any(|i| el.el_tty.t_c[TS_IO][i] != el.el_tty.t_c[EX_IO][i]);

        if changed {
            for i in 0..C_NCC {
                tty_update_char(el, ED_IO as i32, i as i32);
            }

            // Non-forced, so it rebinds only the keys whose edit-mode
            // character actually differs from what `t_ed.c_cc` still holds.
            // This **must** run before the next line, because it diffs the
            // new `t_c[ED_IO]` against the *old* `t_ed.c_cc`.
            tty_bind_char(el, 0);

            {
                let tty = &mut el.el_tty;
                let row = tty.t_c[ED_IO];
                tty__setchar(&mut tty.t_ed, &row);
            }

            // `EX_IO`'s `MD_CHAR` masks are both 0, so this loop reduces to
            // copying the scratch row into the execute row wholesale.
            for i in 0..C_NCC {
                tty_update_char(el, EX_IO as i32, i as i32);
            }

            {
                let tty = &mut el.el_tty;
                let row = tty.t_c[EX_IO];
                tty__setchar(&mut tty.t_ex, &row);
            }
        }
    }
    // If step 6 was skipped — the terminal was already non-canonical — `t_ed`
    // is pushed exactly as it stands, without re-reading anything from the
    // terminal beyond speed and character size.

    // Step 7. `TCSADRAIN` waits for queued output to drain and does not
    // discard pending input, so type-ahead typed before editing started
    // survives into the editor.
    if with_tios(el, Tios::Ed, |el, t| tty_setty(el, plat::TCSADRAIN, t)) == -1 {
        return -1;
    }

    // Step 8.
    el.el_tty.t_mode = ED_IO as u8;
    0
}

// [spec:libedit:def:tty.tty-cookedmode-fn]
// [spec:libedit:sem:tty.tty-cookedmode-fn]
pub fn tty_cookedmode(el: &mut EditLine) -> i32 {
    // Step 1. Already cooked: no syscall issued.
    if el.el_tty.t_mode == EX_IO as u8 {
        return 0;
    }

    // Step 2. ERR-terminal-40, disposition `reproduce`: this test comes
    // *after* the mode test, so an `EditLine` that was put into `ED_IO` and
    // then had editing disabled never gets its terminal restored here.
    if el.el_flags & EDIT_DISABLED != 0 {
        return 0;
    }

    // Step 3. `TCSADRAIN`: the change takes effect only after queued output
    // has been written, and queued input is kept. This does not reload or
    // re-derive `t_ex`; it pushes whatever `t_ex` currently holds. On -1
    // `t_mode` is left as it was, so the recorded mode stays `ED_IO`/`QU_IO`.
    if with_tios(el, Tios::Ex, |el, t| tty_setty(el, plat::TCSADRAIN, t)) == -1 {
        return -1;
    }

    // Step 4. Reachable from both `ED_IO` and `QU_IO`; from quote mode it
    // goes straight to execute mode without passing through `ED_IO`.
    el.el_tty.t_mode = EX_IO as u8;
    0
}

// [spec:libedit:def:tty.tty-quotemode-fn]
// [spec:libedit:sem:tty.tty-quotemode-fn]
pub(crate) fn tty_quotemode(el: &mut EditLine) -> i32 {
    // Step 1. No `EDIT_DISABLED` guard here, unlike `tty_rawmode` and
    // `tty_cookedmode`.
    if el.el_tty.t_mode == QU_IO as u8 {
        return 0;
    }

    // Step 2. `t_qu` is `#define t_qu t_ts`, so this whole-struct copy
    // **overwrites the `TS_IO` scratch termios**: there are only four
    // `struct termios` in `el_tty_t` and quote mode shares the last one with
    // the terminal snapshot. Reproduced. It is harmless only because
    // `tty_rawmode` re-reads `t_ts` from the terminal before using it, and
    // `tty_noquotemode` does not repair it either.
    {
        // Note the base is `t_ed`, not the current terminal state: entering
        // quote mode from `EX_IO` — which the mode test permits — also puts
        // the terminal into raw editing settings while `t_mode` records
        // `QU_IO`, and leaving via `tty_noquotemode` then lands in `ED_IO`.
        let tty = &mut el.el_tty;
        termios_copy(&mut tty.t_ts, &tty.t_ed);
    }

    // Steps 3 and 4. The `QU_IO` masks clear `IXON | IXOFF | INLCR | ICRNL`
    // from `c_iflag` and `ISIG | IEXTEN` from `c_lflag`, set nothing, and
    // leave `c_oflag` and `c_cflag` alone. The control characters are not
    // touched, so `c_cc` stays as edit mode left it — `VMIN == 1`,
    // `VTIME == 0` — and the very next byte reaches the reader untouched.
    if with_tios(el, Tios::Ts, |el, t| {
        tty_setup_flags(el, t, QU_IO as i32);
        tty_setty(el, plat::TCSADRAIN, t)
    }) == -1
    {
        return -1;
    }

    // Step 5.
    el.el_tty.t_mode = QU_IO as u8;
    0
}

// [spec:libedit:def:tty.tty-noquotemode-fn]
// [spec:libedit:sem:tty.tty-noquotemode-fn]
pub(crate) fn tty_noquotemode(el: &mut EditLine) -> i32 {
    // Step 1. There is no `EDIT_DISABLED` check — the mode test is the only
    // guard, and it suffices because quote mode can only be entered via
    // `tty_quotemode`.
    if el.el_tty.t_mode != QU_IO as u8 {
        return 0;
    }

    // Step 2. On -1 `t_mode` stays `QU_IO`, so the terminal keeps the
    // quote-mode flags and libedit knows it.
    if with_tios(el, Tios::Ed, |el, t| tty_setty(el, plat::TCSADRAIN, t)) == -1 {
        return -1;
    }

    // Step 3. `t_ts`, which `tty_quotemode` overwrote, is not repaired;
    // nothing depends on the old contents because `tty_rawmode` re-reads it.
    el.el_tty.t_mode = ED_IO as u8;
    0
}

// [spec:libedit:def:tty.tty-stty-fn]
// [spec:libedit:sem:tty.tty-stty-fn]
/// The `setty` editrc command handler, sharing the C's
/// `int (*)(EditLine *, int, const wchar_t **)` shape with the three `*tc`
/// handlers in `terminal.rs`; the C's NULL-terminated `wchar_t **` becomes a
/// slice of wide strings.
pub fn tty_stty(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    // `argc` is completely ignored, as in the C.
    let _ = argc;

    // Steps 1 and 2. The C's `argv == NULL` and its unchecked
    // `strlcpy(name, ct_encode_string(*argv++, ...))`, which dereferences
    // NULL for an empty vector (ERR-terminal-07, disposition `define`). Both
    // become "there is no command name here", which is the C's own -1.
    let Some(&arg0) = argv.first() else {
        return -1;
    };
    let mut name = encode(el, wcs(arg0));
    // `strlcpy` into a `char[EL_BUFSIZ]`. Used only in error messages.
    name.truncate(EL_BUFSIZ - 1);

    // Step 3.
    let mut which = Tios::Ex;
    let mut z = EX_IO;
    let mut aflag = 0;

    // Step 4. The C's loop condition is `argv[0][0] == '-' && argv[0][2] ==
    // '\0'`, which for the one-character token `L"-"` reads one element past
    // the end of the string (ERR-terminal-06). Its disposition is `define —
    // test the length`, so that is the test: a token that is not exactly two
    // wide characters is not an option, and `setty -` falls through to the
    // argument loop below, where it is reported as an invalid argument.
    let mut ai = 1usize;
    while ai < argv.len() {
        let s = wcs(argv[ai]);
        if s.len() != 2 || s[0] != u32::from(b'-') {
            break;
        }
        match u8::try_from(s[1]) {
            // Show every mode name, not only those with an explicit `+`/`-`.
            Ok(b'a') => {
                aflag += 1;
                ai += 1;
            }
            Ok(b'd') => {
                ai += 1;
                which = Tios::Ed;
                z = ED_IO;
            }
            Ok(b'x') => {
                ai += 1;
                which = Tios::Ex;
                z = EX_IO;
            }
            // `QU_IO` and `TS_IO` are both 2 and `t_qu` is `t_ts`, so this is
            // the quote-mode row and the scratch termios at once.
            Ok(b'q') => {
                ai += 1;
                which = Tios::Ts;
                z = QU_IO;
            }
            // Note the C does not advance here; it returns.
            _ => {
                let mut msg = name.clone();
                msg.extend_from_slice(b": Unknown switch `");
                msg.extend_from_slice(&encode_char(s[1]));
                msg.extend_from_slice(b"'.\n");
                write_errfile(el, &msg);
                return -1;
            }
        }
    }

    // Step 5. Display form.
    if ai >= argv.len() {
        tty_stty_display(el, z, aflag);
        return 0;
    }

    // Steps 6 to 9. `tios` is a field of `el`, which is why it is moved out
    // for the duration; see `with_tios`.
    with_tios(el, which, |el, tios| {
        tty_stty_edit(el, tios, z, argv, ai, &name)
    })
}

/// [`tty_stty`] step 5: walk `ttymodes[]` and print the mask state for mode
/// `z`, wrapping at the terminal width.
///
/// The C emits this with one `fprintf` per token; the port formats into one
/// buffer and writes it once, which a `FILE *` would have done anyway and
/// which keeps `el` borrowable while the text is built.
fn tty_stty_display(el: &mut EditLine, z: usize, aflag: i32) {
    // `i` is the group last printed, `len` the current output column, `st`
    // the indent to use when wrapping.
    let mut i: i32 = -1;
    let mut len: usize = 0;
    let mut st: usize = 0;
    let mut out: Vec<u8> = Vec::new();

    for m in &TTYMODES {
        if m.m_type != i {
            // A newline before every group but the first.
            if i != -1 {
                out.push(b'\n');
            }
            // One of "iflag:", "oflag:", "cflag:", "lflag:", "chars:". This
            // relies on `ttymodes` being grouped by `m_type` in `MD_*` order.
            let label = el.el_tty.t_t[z][m.m_type as usize].t_name;
            out.extend_from_slice(label.as_bytes());
            i = m.m_type;
            len = label.len();
            st = len;
        }

        // The C's `i == -1` fallback branch is unreachable: the first entry
        // always assigns a non-negative `m_type`.
        let ent = &el.el_tty.t_t[z][i as usize];
        // Clear wins in the display.
        let mut x = if ent.t_setmask & m.m_value != 0 {
            b'+'
        } else {
            0
        };
        if ent.t_clrmask & m.m_value != 0 {
            x = b'-';
        }

        if x != 0 || aflag != 0 {
            let cu = m.m_name.len() + usize::from(x != 0) + 1;
            // The C's `(size_t)el->el_terminal.t_size.h`: a negative width
            // becomes an enormous `size_t` and the wrap never fires, which
            // the sign-extending cast reproduces exactly.
            if len + cu >= el.el_terminal.t_size.h as usize {
                out.push(b'\n');
                out.resize(out.len() + st, b' ');
                len = st + cu;
            } else {
                len += cu;
            }
            if x != 0 {
                out.push(x);
            }
            out.extend_from_slice(m.m_name.as_bytes());
            out.push(b' ');
        }
    }
    out.push(b'\n');
    write_outfile(el, &out);
}

/// [`tty_stty`] steps 6 to 9: apply each remaining argument, re-derive the
/// selected termios from the (possibly just-modified) masks, and push it if
/// it is the mode currently installed.
fn tty_stty_edit(
    el: &mut EditLine,
    tios: &mut Termios,
    z: usize,
    argv: &[&[u32]],
    mut ai: usize,
    name: &[u8],
) -> i32 {
    while ai < argv.len() {
        let arg = wcs(argv[ai]);
        ai += 1;

        // 6a.
        let (x, s) = match arg.first().copied() {
            Some(c) if c == u32::from(b'+') => (b'+', &arg[1..]),
            Some(c) if c == u32::from(b'-') => (b'-', &arg[1..]),
            _ => (0u8, arg),
        };

        // 6b.
        let d = s;
        let eq = s.iter().position(|&c| c == u32::from(b'='));
        let enc = encode(el, d);

        // 6c. The length passed to `strncmp` counts *wide* characters while
        // the comparison operates on the *encoded* bytes; all table names are
        // ASCII, so this only misbehaves on non-ASCII input. A prefix match
        // also succeeds against any longer table name — `setty er=^H` sets
        // `erase` — and both are reproduced (ERR-terminal-38).
        let found = TTYMODES.iter().find(|m| match eq {
            Some(p) => strncmp_eq(m.m_name.as_bytes(), &enc, p) && m.m_type == MD_CHAR as i32,
            None => m.m_name.as_bytes() == enc.as_slice(),
        });

        // 6d. Arguments already processed keep their effect.
        let Some(m) = found else {
            let mut msg = name.to_vec();
            msg.extend_from_slice(b": Invalid argument `");
            msg.extend_from_slice(&enc);
            msg.extend_from_slice(b"'.\n");
            write_errfile(el, &msg);
            return -1;
        };

        // 6e. The `name=value` form, only reachable for `MD_CHAR` entries.
        if let Some(p) = eq {
            // `ffs((int)m->m_value) - 1`. `MD_CHAR` entries hold
            // `C_SH(C_XXX)`, i.e. `1 << C_XXX`, so this is the `C_*` index and
            // the C's `assert(c != 0)` holds by construction.
            let c = m.m_value.trailing_zeros() as usize;

            // `name=` with nothing after the `=` disables the character;
            // otherwise the text is parsed as an escape (`^X`, `\n`, `\033`,
            // a literal char, ...). `parse__escape` returns -1 for a
            // malformed escape and the result is stored as `(cc_t)-1` = 0xFF
            // with no check — reproduced, and it is why `setty erase=X` is
            // 0xFF while `setty erase=^H` works (ERR-input-36).
            let after = &d[p + 1..];
            let v = if after.is_empty() {
                i32::from(el.el_tty.t_vdisable)
            } else {
                let mut cur: &[u32] = after;
                parse__escape(&mut cur)
            };

            let sub = tty__getcharindex(c as i32);
            if sub >= 0 {
                // The write lands in the selected `struct termios` **only**;
                // `el->el_tty.t_c[z][...]` is not updated, so any later
                // `tty__setchar` from `t_c` — which `tty_rawmode` performs
                // whenever it detects a control-character change — silently
                // reverts it (ERR-terminal-38, disposition `reproduce`). This
                // is what makes `setty erase=^H` in an `.editrc` not stick.
                cc_set(tios, sub as usize, v as u8);
            }
            // ERR-terminal-05, disposition `define — reject the unmapped
            // index`: `tty__getcharindex` has no `C_BRK` case, so on a
            // platform whose `ttymodes` carries `brk` the C computes -1 and,
            // once `assert` is compiled out under `NDEBUG`, writes
            // `tios->c_cc[-1]`. Here the write is simply skipped. Unreachable
            // on this platform, which defines no `VBRK` and so has no `brk`
            // entry to match.
            continue;
        }

        // 6f. Apply the sign to the mask pair. No sign leaves the bit alone
        // at apply time, i.e. inherited from whatever the terminal has.
        let e = &mut el.el_tty.t_t[z][m.m_type as usize];
        match x {
            b'+' => {
                e.t_setmask |= m.m_value;
                e.t_clrmask &= !m.m_value;
            }
            b'-' => {
                e.t_setmask &= !m.m_value;
                e.t_clrmask |= m.m_value;
            }
            _ => {
                e.t_setmask &= !m.m_value;
                e.t_clrmask &= !m.m_value;
            }
        }
    }

    // Step 7.
    tty_setup_flags(el, tios, z as i32);

    // Step 8. If the edited mode is not the current one, the change only
    // takes effect the next time that mode is entered. Note `-q` edits are
    // transient anyway: `t_ts` is overwritten by `tty_rawmode` and by
    // `tty_quotemode`, so only the `t_t[QU_IO]` masks persist.
    if el.el_tty.t_mode == z as u8 && tty_setty(el, plat::TCSADRAIN, tios) == -1 {
        return -1;
    }

    // Step 9.
    0
}

/// The C's `"%lc"`: one wide character in the current locale's encoding.
fn encode_char(c: u32) -> Vec<u8> {
    let mut buf = [0u8; MB_LEN_MAX];
    match ct_encode_char(&mut buf, c) {
        n if n > 0 => buf[..n as usize].to_vec(),
        // Unencodable, or no room. `fprintf` writes nothing for `%lc` in that
        // case and reports the error the C never checks.
        _ => Vec::new(),
    }
}

// [spec:libedit:def:tty.tty-printchar-fn]
// [spec:libedit:sem:tty.tty-printchar-fn]
/// Debug dump of the control characters; the C guards it with `#ifdef notyet`
/// and never calls it. `s` is one row of [`TtycharT`], read only.
///
/// The C's body is dead, has no callers and **does not compile**: it declares
/// `ttyperm_t *m`, initialises it from `el->el_tty.t_t` — a two-dimensional
/// array type — and then reads `m->m_name`, `m->m_type` and `m->m_value`,
/// which are members of `ttymodes_t` (ERR-terminal-65). The rule says not to
/// transliterate that, so this is written fresh from the stated intent,
/// including the two quirks the rule records as part of it: the `i % 5 == 0`
/// newline fires on `i == 0`, so the output opens with a blank line and the
/// grouping is offset by one; and the caret rendering `s[i] + 'A' - 1`
/// produces garbage for any byte outside 1..26, the disable byte included.
fn tty_printchar(el: &mut EditLine, s: &[u8]) {
    let mut out: Vec<u8> = Vec::new();
    for (i, &b) in s.iter().enumerate().take(C_NCC) {
        if let Some(m) = TTYMODES
            .iter()
            .find(|m| m.m_type == MD_CHAR as i32 && m.m_value == c_sh(i))
        {
            out.extend_from_slice(m.m_name.as_bytes());
            out.extend_from_slice(b" ^");
            // The C's `s[i] + 'A' - 1` reaches `fprintf`'s `%c`, which
            // converts to `unsigned char`; the wrap is that conversion.
            out.push(b.wrapping_add(b'A' - 1));
            out.push(b' ');
        }
        if i % 5 == 0 {
            out.push(b'\n');
        }
    }
    out.push(b'\n');
    write_errfile(el, &out);
}

// [spec:libedit:def:tty.tty-setup-flags-fn]
// [spec:libedit:sem:tty.tty-setup-flags-fn]
/// C: `static void tty_setup_flags(EditLine *el, struct termios *tios, int
/// mode)`. Same aliasing note as [`tty_getty`]: `tios` is always a field of
/// `el->el_tty` at the call sites.
fn tty_setup_flags(el: &mut EditLine, tios: &mut Termios, mode: i32) {
    // Each of `c_iflag`, `c_oflag`, `c_cflag`, `c_lflag` has the mode's
    // `t_clrmask` bits cleared and then its `t_setmask` bits set. `MD_CHAR`
    // is deliberately excluded — control characters are never touched here,
    // which is also what keeps `tty__get_flag` total. Speed is not touched
    // either, and nothing is pushed to the terminal; the caller decides
    // whether and when to write the struct out.
    //
    // `mode` selects the row of `el_tty.t_t`, so the effect depends on the
    // *current*, possibly `setty`-modified, copy of the table rather than on
    // the compiled-in defaults.
    for kind in MD_INP..=MD_LIN {
        let f = *tty__get_flag(tios, kind as i32);
        let nf = tty_update_flag(el, f, mode, kind as i32);
        *tty__get_flag(tios, kind as i32) = nf;
    }
}

// [spec:libedit:def:tty.tty-get-signal-character-fn]
// [spec:libedit:sem:tty.tty-get-signal-character-fn]
/// Intended to return the control byte the terminal would echo for a given
/// signal, so `rl_echo_signal_char` can echo it manually. Returns a byte
/// 0..255, or -1 for "nothing to echo".
///
/// **Doubly broken, and reproduced.** The rule requires the choice to be
/// recorded rather than made silently; `plan/decisions/conformance-policy.md`
/// makes reproduce the default for a defect that is merely wrong, and both of
/// these are, so both stand:
///
/// - ERR-terminal-36 — the guard reads `t_ed.c_iflag` (`MD_INP`) when
///   `ECHOCTL` is a `c_lflag` bit; the column should be `MD_LIN`. On glibc
///   `ECHOCTL` has the same value as the input flag `IUCLC`, which libedit
///   never sets and which is off on normal terminals, so the guard is always
///   false and **this function always returns -1 on Linux** — making
///   `rl_echo_signal_char` a silent no-op. Where `tty.h` supplied
///   `ECHOCTL == 0` the guard is `(x & 0) == 0` and the answer is -1 for the
///   other reason.
/// - ERR-terminal-37 — the rows of `t_c` are indexed by the libedit `C_*`
///   constants, but the switch subscripts them with the termios `V*` values.
///   They coincide by accident for `VINTR == 0 == C_INTR` and
///   `VQUIT == 1 == C_QUIT`; `VSUSP` is 10 while `C_SUSP` is 13, so `SIGTSTP`
///   answers `t_c[ED_IO][C_START]` — the flow-control start character `^Q`,
///   not the suspend character. Masked today by the guard above, live the
///   moment it is fixed. The intended expressions are `t_c[ED_IO][C_INTR]`,
///   `[C_QUIT]`, `[C_STATUS]` and `[C_SUSP]`.
///
/// The consequence to carry forward: the readline echo-signal-char entry
/// point is dead on this platform, and a caller must not read anything into
/// its -1.
pub(crate) fn tty_get_signal_character(el: &mut EditLine, sig: i32) -> i32 {
    // ERR-terminal-36, exactly as written: `MD_INP`, not `MD_LIN`.
    let ed = *tty__get_flag(&mut el.el_tty.t_ed, MD_INP as i32);
    if ed & plat::ECHOCTL == 0 {
        return -1;
    }

    // ERR-terminal-37, exactly as written: `V*` subscripts into a `C_*`-keyed
    // row. The `SIGINFO`/`VSTATUS` arm is BSD-only and is not compiled here,
    // as it is not compiled on Linux in the C.
    let sub = match sig {
        plat::SIGINT => plat::VINTR,
        plat::SIGQUIT => plat::VQUIT,
        plat::SIGTSTP => plat::VSUSP,
        _ => return -1,
    };

    // No arm checks whether the character is disabled: if the slot holds
    // `t_vdisable` the disable byte itself is returned and the caller echoes
    // it. The return is an unsigned byte, so it can never be confused with
    // -1. `t_mode` is not consulted — the answer comes from the edit-mode row
    // whether or not the terminal is currently in edit mode.
    //
    // A `V*` subscript at or above `C_NCC` would be an out-of-bounds read of
    // the row in the C; defined here as -1, "nothing to echo". Not reachable
    // on this platform, where the largest of the three is `VSUSP == 10`.
    match el.el_tty.t_c[ED_IO].get(sub) {
        Some(&b) => i32::from(b),
        None => -1,
    }
}
