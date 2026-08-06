//! The termios ABI, the window-size query, and `isatty`.
//!
//! No libc here: `tcgetattr`, `tcsetattr`, `tcgetwinsize` and `isatty` all go
//! through rustix, which on Linux issues the `ioctl`s directly. What rustix
//! cannot supply portably — the flag-word bit values, the `V*` subscripts,
//! `NCCS`, the `TCSA*` actions, `_POSIX_VDISABLE` and the `<sys/ttydefaults.h>`
//! control-character defaults — is transcribed for Linux/glibc, because the
//! `sem` rules address those numbers by name and POSIX does not fix them.
//!
//! **Scope of the numbers.** The BSDs use a different termios ABI throughout
//! — different `V*` numbering, different flag bits, a `struct termios` with
//! separate `c_ispeed`/`c_ospeed` — and this module does not carry it.
//! `plan/decisions/posix-only-scope.md` puts POSIX on the target and the
//! numbers are not POSIX's to give. The one place the split is reproduced is
//! [`VDISABLE`], because `sem:tty.tty-bind-char-fn` requires it: the disable
//! byte reaches the key map, and it is 0 on glibc and 0xff on the BSDs.

use rustix::termios::{OptionalActions, SpecialCodeIndex};

/// `_POSIX_VDISABLE`. POSIX defines the constant but not its value;
/// glibc/Linux uses 0 and the BSDs and macOS use 0xff. `tty.h` falls back to
/// `(unsigned char)-1` where the platform defines neither `_POSIX_VDISABLE`
/// nor `VDISABLE`, which is the 0xff arm.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub const VDISABLE: u8 = 0;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub const VDISABLE: u8 = 0xff;

/// `NCCS` — the length of a `struct termios`'s `c_cc`.
///
/// This is **glibc's** 32, which is the array libedit's own `struct termios`
/// carries and therefore the one the `def` rules describe. The kernel's
/// `struct termios` is 19 long, and glibc's `tcgetattr` copies those 19 and
/// fills the tail with `_POSIX_VDISABLE`; [`tcgetattr`] below does the same,
/// which on Linux means leaving it zero.
pub const NCCS: usize = 32;

/// `TCSANOW` — apply immediately.
pub const TCSANOW: i32 = 0;
/// `TCSADRAIN` — apply after queued output has drained, keeping queued
/// input.
pub const TCSADRAIN: i32 = 1;
/// `TCSAFLUSH` — drain output, then discard unread input.
pub const TCSAFLUSH: i32 = 2;

// `c_iflag` bits.
pub const IGNBRK: u32 = 0o0000001;
pub const BRKINT: u32 = 0o0000002;
pub const IGNPAR: u32 = 0o0000004;
pub const PARMRK: u32 = 0o0000010;
pub const INPCK: u32 = 0o0000020;
pub const ISTRIP: u32 = 0o0000040;
pub const INLCR: u32 = 0o0000100;
pub const IGNCR: u32 = 0o0000200;
pub const ICRNL: u32 = 0o0000400;
/// Legacy SysV, Linux only. Note it is the *same bit* as [`ECHOCTL`], which
/// is what makes ERR-terminal-36 the silent no-op it is.
pub const IUCLC: u32 = 0o0001000;
pub const IXON: u32 = 0o0002000;
pub const IXANY: u32 = 0o0004000;
pub const IXOFF: u32 = 0o0010000;
pub const IMAXBEL: u32 = 0o0020000;

// `c_oflag` bits.
pub const OPOST: u32 = 0o0000001;
pub const OLCUC: u32 = 0o0000002;
pub const ONLCR: u32 = 0o0000004;
pub const OCRNL: u32 = 0o0000010;
pub const ONOCR: u32 = 0o0000020;
pub const ONLRET: u32 = 0o0000040;
pub const OFILL: u32 = 0o0000100;
pub const OFDEL: u32 = 0o0000200;
pub const NLDLY: u32 = 0o0000400;
pub const CRDLY: u32 = 0o0003000;
pub const TABDLY: u32 = 0o0014000;
/// The XSI `TABDLY` value meaning "expand tabs to spaces". `tty.h` aliases it
/// to `OXTABS` on the BSDs and to 0 where neither exists — the degenerate
/// case `sem:tty.tty-rawmode-fn` warns about, in which `(x & 0) == 0` is
/// always true and `t_tabs` is forced to 0. It is **not** degenerate here:
/// glibc defines `TAB3`, so the `EL_CAN_TAB` branch of `tty_rawmode` is live.
pub const TAB3: u32 = 0o0014000;
/// `XTABS`, which glibc gives the same value as [`TAB3`]. `ttymodes[]`
/// carries both names, so `+xtabs` and `+tabdly` interact.
pub const XTABS: u32 = 0o0014000;
pub const BSDLY: u32 = 0o0020000;
pub const VTDLY: u32 = 0o0040000;
pub const FFDLY: u32 = 0o0100000;

// `c_cflag` bits.
pub const CBAUD: u32 = 0o0010017;
pub const CBAUDEX: u32 = 0o0010000;
pub const CSIZE: u32 = 0o0000060;
pub const CS8: u32 = 0o0000060;
pub const CSTOPB: u32 = 0o0000100;
pub const CREAD: u32 = 0o0000200;
pub const PARENB: u32 = 0o0000400;
pub const PARODD: u32 = 0o0001000;
pub const HUPCL: u32 = 0o0002000;
pub const CLOCAL: u32 = 0o0004000;
pub const CIBAUD: u32 = 0o02003600000;
pub const CRTSCTS: u32 = 0o020000000000;

// `c_lflag` bits.
pub const ISIG: u32 = 0o0000001;
pub const ICANON: u32 = 0o0000002;
pub const XCASE: u32 = 0o0000004;
pub const ECHO: u32 = 0o0000010;
pub const ECHOE: u32 = 0o0000020;
pub const ECHOK: u32 = 0o0000040;
pub const ECHONL: u32 = 0o0000100;
pub const NOFLSH: u32 = 0o0000200;
pub const TOSTOP: u32 = 0o0000400;
/// Echo control characters as `^X`. A `c_lflag` bit, and on glibc the same
/// value as the `c_iflag` bit [`IUCLC`] — the coincidence ERR-terminal-36
/// turns into a permanent -1.
pub const ECHOCTL: u32 = 0o0001000;
pub const ECHOPRT: u32 = 0o0002000;
pub const ECHOKE: u32 = 0o0004000;
pub const FLUSHO: u32 = 0o0010000;
pub const PENDIN: u32 = 0o0040000;
pub const IEXTEN: u32 = 0o0100000;
pub const EXTPROC: u32 = 0o0200000;

// The termios `V*` subscripts this platform defines, as the C sees them after
// `tty.h`'s aliasing. glibc has no `VSWTCH` (only `VSWTC`, which `tty.c`
// never names), no `VDSWTCH`, `VERASE2`, `VDSUSP`, `VSTATUS`, `VPAGE`,
// `VPGOFF`, `VKILL2` or `VBRK`, so those rows of every table in `tty.rs` are
// simply absent — which is what `#ifdef`ing them out means.
pub const VINTR: usize = 0;
pub const VQUIT: usize = 1;
pub const VERASE: usize = 2;
pub const VKILL: usize = 3;
pub const VEOF: usize = 4;
pub const VTIME: usize = 5;
pub const VMIN: usize = 6;
pub const VSTART: usize = 8;
pub const VSTOP: usize = 9;
pub const VSUSP: usize = 10;
pub const VEOL: usize = 11;
pub const VREPRINT: usize = 12;
pub const VDISCARD: usize = 13;
pub const VWERASE: usize = 14;
pub const VLNEXT: usize = 15;
pub const VEOL2: usize = 16;

// The `C_*` control-character defaults, in `C_*` order. These come from
// `<sys/ttydefaults.h>`, which glibc copied verbatim from BSD, so unlike the
// numbering above they *are* portable — parameterised only by [`VDISABLE`].
// `tty.h`'s own fallbacks supply the six the header does not define. Note
// `CMIN`/`CTIME` are 1 and 0 here, not the nonsense `CEOF`/`CEOL` of
// `tty.h`'s fallback (ERR-terminal-43), which is therefore unreachable on any
// platform that ships the header.
const fn ctrl(c: u8) -> u8 {
    c & 0o37
}
pub const CINTR: u8 = ctrl(b'c');
pub const CQUIT: u8 = 0o34;
pub const CERASE: u8 = 0o177;
pub const CKILL: u8 = ctrl(b'u');
pub const CEOF: u8 = ctrl(b'd');
pub const CEOL: u8 = VDISABLE;
pub const CEOL2: u8 = VDISABLE;
pub const CSWTCH: u8 = VDISABLE;
pub const CDSWTCH: u8 = VDISABLE;
pub const CERASE2: u8 = VDISABLE;
pub const CSTART: u8 = ctrl(b'q');
pub const CSTOP: u8 = ctrl(b's');
pub const CWERASE: u8 = ctrl(b'w');
pub const CSUSP: u8 = ctrl(b'z');
pub const CDSUSP: u8 = ctrl(b'y');
pub const CREPRINT: u8 = ctrl(b'r');
pub const CDISCARD: u8 = ctrl(b'o');
pub const CLNEXT: u8 = ctrl(b'v');
pub const CSTATUS: u8 = VDISABLE;
pub const CPAGE: u8 = b' ';
pub const CPGOFF: u8 = ctrl(b'm');
pub const CKILL2: u8 = VDISABLE;
pub const CBRK: u8 = VDISABLE;
pub const CMIN: u8 = 1;
pub const CTIME: u8 = 0;

/// The kernel's `struct termios`, as libedit's four flag words and `c_cc`.
///
/// Deliberately **not** rustix's `Termios`: `def:tty.el-tty-t` freezes
/// libedit's own shape, whose `c_cc` is [`NCCS`] long and which carries no
/// `c_ispeed`/`c_ospeed` because glibc's `cfgetospeed` reads the `CBAUD` bits
/// of `c_cflag` and its `tcsetattr` sends the kernel a struct with no speed
/// fields at all. Only `CBAUD` is load-bearing, which is exactly what this
/// carries.
#[derive(Clone, Copy)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    /// Indexed by the `V*` subscripts above, not by libedit's `C_*` ones.
    pub c_cc: [u8; NCCS],
}

impl Default for Termios {
    fn default() -> Self {
        Self {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_cc: [0; NCCS],
        }
    }
}

/// Every `V*` subscript Linux names, paired with rustix's index token.
///
/// rustix's `SpecialCodeIndex` has a private field and only named constants,
/// so the array cannot be walked by number; this table is the walk. Linux
/// defines 0 through 16 and `NCCS` is 19 in the kernel, so slots 17 and 18
/// are reserved, never populated and not carried. `VSWTC` (7) has no libedit
/// use — `tty.c` never names it — but it round-trips here so that the
/// original modes `tty_end` restores are the ones `tty_setup` captured.
const CC_INDICES: [(usize, SpecialCodeIndex); 17] = [
    (VINTR, SpecialCodeIndex::VINTR),
    (VQUIT, SpecialCodeIndex::VQUIT),
    (VERASE, SpecialCodeIndex::VERASE),
    (VKILL, SpecialCodeIndex::VKILL),
    (VEOF, SpecialCodeIndex::VEOF),
    (VTIME, SpecialCodeIndex::VTIME),
    (VMIN, SpecialCodeIndex::VMIN),
    (7, SpecialCodeIndex::VSWTC),
    (VSTART, SpecialCodeIndex::VSTART),
    (VSTOP, SpecialCodeIndex::VSTOP),
    (VSUSP, SpecialCodeIndex::VSUSP),
    (VEOL, SpecialCodeIndex::VEOL),
    (VREPRINT, SpecialCodeIndex::VREPRINT),
    (VDISCARD, SpecialCodeIndex::VDISCARD),
    (VWERASE, SpecialCodeIndex::VWERASE),
    (VLNEXT, SpecialCodeIndex::VLNEXT),
    (VEOL2, SpecialCodeIndex::VEOL2),
];

/// `isatty(fd)`.
#[must_use]
pub fn isatty(fd: i32) -> bool {
    crate::borrow(fd).is_some_and(rustix::termios::isatty)
}

/// `tcgetattr(fd, t)`, retried on `EINTR` as `tty_getty` does. `None` is the
/// C's -1.
#[must_use]
pub fn tcgetattr(fd: i32) -> Option<Termios> {
    let raw = tcgetattr_raw(fd)?;
    let mut t = Termios {
        c_iflag: raw.input_modes.bits(),
        c_oflag: raw.output_modes.bits(),
        c_cflag: raw.control_modes.bits(),
        c_lflag: raw.local_modes.bits(),
        c_cc: [0; NCCS],
    };
    for (v, index) in CC_INDICES {
        t.c_cc[v] = raw.special_codes[index];
    }
    Some(t)
}

/// `tcsetattr(fd, action, t)`, retried on `EINTR` as `tty_setty` does.
/// `false` is the C's -1.
///
/// `action` is [`TCSANOW`], [`TCSADRAIN`] or [`TCSAFLUSH`]; anything else
/// fails, where the C would pass it to the kernel and be given `EINVAL`.
///
/// The current settings are read first and the four flag words and `c_cc`
/// written over them. That is not belt-and-braces: rustix uses `TCSETS2`,
/// which carries `c_ispeed`/`c_ospeed`, and libedit's `struct termios` has
/// neither. Seeding from the live settings is precisely what the kernel does
/// for glibc's `TCSETS` (`tmp_termios = tty->termios` before the copy in),
/// so an arbitrary `BOTHER` line speed survives a call that does not change
/// `CBAUD` — where a zeroed seed would hang the line up.
#[must_use]
pub fn tcsetattr(fd: i32, action: i32, t: &Termios) -> bool {
    let actions = match action {
        TCSANOW => OptionalActions::Now,
        TCSADRAIN => OptionalActions::Drain,
        TCSAFLUSH => OptionalActions::Flush,
        _ => return false,
    };
    let Some(borrowed) = crate::borrow(fd) else {
        return false;
    };
    let Some(mut raw) = tcgetattr_raw(fd) else {
        return false;
    };
    raw.input_modes = rustix::termios::InputModes::from_bits_retain(t.c_iflag);
    raw.output_modes = rustix::termios::OutputModes::from_bits_retain(t.c_oflag);
    raw.control_modes = rustix::termios::ControlModes::from_bits_retain(t.c_cflag);
    raw.local_modes = rustix::termios::LocalModes::from_bits_retain(t.c_lflag);
    for (v, index) in CC_INDICES {
        raw.special_codes[index] = t.c_cc[v];
    }
    loop {
        match rustix::termios::tcsetattr(borrowed, actions, &raw) {
            Ok(()) => return true,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return false,
        }
    }
}

/// `ioctl(fd, TIOCGWINSZ, &ws)`, returning `(ws_row, ws_col)`. `None` is the
/// C's -1, which `terminal_get_size` ignores.
///
/// The C also tries `TIOCGSIZE`, the older BSD `struct ttysize` ioctl. Linux
/// does not define it — `plan/decisions/posix-only-scope.md` makes Linux the
/// whole target — so that block does not compile there and has no
/// counterpart here.
#[must_use]
pub fn window_size(fd: i32) -> Option<(u16, u16)> {
    let fd = crate::borrow(fd)?;
    let ws = rustix::termios::tcgetwinsize(fd).ok()?;
    Some((ws.ws_row, ws.ws_col))
}

/// The `EINTR` retry loop `sem:tty.tty-getty-fn` specifies, around rustix's
/// typed `tcgetattr`.
fn tcgetattr_raw(fd: i32) -> Option<rustix::termios::Termios> {
    let fd = crate::borrow(fd)?;
    loop {
        match rustix::termios::tcgetattr(fd) {
            Ok(t) => return Some(t),
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::os::fd::{AsRawFd, OwnedFd};

    use super::*;
    use crate::cheader;

    /// Every `V*` name Linux defines, against the rustix token that carries
    /// the same name.
    ///
    /// This is deliberately **not** [`CC_INDICES`], and it must not be derived
    /// from it: it is the independent side of the comparison. Only names are
    /// written here — one from the C's header, one from rustix's — and the
    /// number each stands for is read out of the header at run time. Sized
    /// from `CC_INDICES` so that an entry appearing there and not here fails
    /// to compile.
    const V_TOKENS: [(&str, SpecialCodeIndex); CC_INDICES.len()] = [
        ("VINTR", SpecialCodeIndex::VINTR),
        ("VQUIT", SpecialCodeIndex::VQUIT),
        ("VERASE", SpecialCodeIndex::VERASE),
        ("VKILL", SpecialCodeIndex::VKILL),
        ("VEOF", SpecialCodeIndex::VEOF),
        ("VTIME", SpecialCodeIndex::VTIME),
        ("VMIN", SpecialCodeIndex::VMIN),
        ("VSWTC", SpecialCodeIndex::VSWTC),
        ("VSTART", SpecialCodeIndex::VSTART),
        ("VSTOP", SpecialCodeIndex::VSTOP),
        ("VSUSP", SpecialCodeIndex::VSUSP),
        ("VEOL", SpecialCodeIndex::VEOL),
        ("VREPRINT", SpecialCodeIndex::VREPRINT),
        ("VDISCARD", SpecialCodeIndex::VDISCARD),
        ("VWERASE", SpecialCodeIndex::VWERASE),
        ("VLNEXT", SpecialCodeIndex::VLNEXT),
        ("VEOL2", SpecialCodeIndex::VEOL2),
    ];

    /// A pseudo-terminal, held open for as long as the test needs one.
    ///
    /// The user side keeps the pair alive; every call under test goes to the
    /// terminal side, which is what an application's standard input would be.
    struct Pty {
        _user: OwnedFd,
        terminal: OwnedFd,
    }

    impl Pty {
        fn open() -> Self {
            use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
            let user = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("/dev/ptmx");
            grantpt(&user).expect("grantpt");
            unlockpt(&user).expect("unlockpt");
            let name = ptsname(&user, Vec::new()).expect("ptsname");
            let terminal = rustix::fs::open(
                name.as_c_str(),
                rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY,
                rustix::fs::Mode::empty(),
            )
            .expect("the terminal side");
            Self {
                _user: user,
                terminal,
            }
        }

        fn fd(&self) -> i32 {
            self.terminal.as_raw_fd()
        }

        /// The kernel's `struct termios`, reached without going through
        /// anything this module defines.
        fn raw(&self) -> rustix::termios::Termios {
            rustix::termios::tcgetattr(&self.terminal).expect("tcgetattr")
        }

        fn set_raw(&self, t: &rustix::termios::Termios) {
            rustix::termios::tcsetattr(&self.terminal, OptionalActions::Now, t).expect("tcsetattr");
        }
    }

    /// Every `V*` name this module defines a subscript for, against glibc's
    /// own `bits/termios-c_cc.h` — the header libedit compiles against, and
    /// therefore the numbering its `c_cc` accesses mean.
    ///
    /// An off-by-one in this table announces nothing; it silently rebinds the
    /// user's erase or interrupt character to whatever sits next to it.
    #[test]
    fn the_v_subscripts_are_the_ones_the_header_defines() {
        let h = v_defines();
        for (name, ours) in [
            ("VINTR", VINTR),
            ("VQUIT", VQUIT),
            ("VERASE", VERASE),
            ("VKILL", VKILL),
            ("VEOF", VEOF),
            ("VTIME", VTIME),
            ("VMIN", VMIN),
            ("VSTART", VSTART),
            ("VSTOP", VSTOP),
            ("VSUSP", VSUSP),
            ("VEOL", VEOL),
            ("VREPRINT", VREPRINT),
            ("VDISCARD", VDISCARD),
            ("VWERASE", VWERASE),
            ("VLNEXT", VLNEXT),
            ("VEOL2", VEOL2),
        ] {
            assert_eq!(h[name], ours as i64, "{name}");
        }
        // `VSWTC` has no constant of its own — `tty.c` never names it — so its
        // subscript is written inline in `CC_INDICES` and is read back out of
        // the table here rather than restated. A wrong one loses a byte of the
        // original modes `tty_end` puts back.
        let (vswtc, _) = CC_INDICES
            .iter()
            .find(|(_, token)| *token == SpecialCodeIndex::VSWTC)
            .expect("VSWTC is in the table");
        assert_eq!(h["VSWTC"], *vswtc as i64);
    }

    /// The other half of the table: that rustix's token for a name really does
    /// index the subscript the header gives that name.
    ///
    /// `SpecialCodeIndex` holds a private number and offers no way to read it,
    /// so the only way to find out is to make the kernel show us. A distinct
    /// byte is written at each token, and each is required to come back at the
    /// header's subscript for that token's name — the write goes in by token
    /// and the read comes out by number, so a pairing that has drifted cannot
    /// cancel itself out.
    #[test]
    fn each_rustix_token_indexes_the_subscript_the_header_gives_its_name() {
        let h = v_defines();
        let pty = Pty::open();

        let mut raw = pty.raw();
        for (i, (_, token)) in V_TOKENS.iter().enumerate() {
            raw.special_codes[*token] = marker(i);
        }
        pty.set_raw(&raw);

        let got = tcgetattr(pty.fd()).expect("tcgetattr");
        for (i, (name, _)) in V_TOKENS.iter().enumerate() {
            let subscript = usize::try_from(h[*name]).expect("a subscript");
            assert_eq!(
                got.c_cc[subscript],
                marker(i),
                "{name} did not land at c_cc[{subscript}]"
            );
        }

        // Linux numbers 0 through 16 and its kernel `c_cc` is 19 long, so 17
        // and 18 are reserved and never populated. glibc's `tcgetattr` fills
        // the tail of its own 32-byte array with `_POSIX_VDISABLE`; so does
        // ours, which on Linux means leaving it zero.
        assert!(got.c_cc[17..].iter().all(|&b| b == VDISABLE));
    }

    /// And the same pairing in the direction `tcsetattr` uses it: written by
    /// number, read back by token.
    #[test]
    fn a_control_character_set_by_subscript_reaches_the_token_of_that_name() {
        let h = v_defines();
        let pty = Pty::open();

        let mut t = tcgetattr(pty.fd()).expect("tcgetattr");
        for (i, (name, _)) in V_TOKENS.iter().enumerate() {
            let subscript = usize::try_from(h[*name]).expect("a subscript");
            t.c_cc[subscript] = marker(i);
        }
        assert!(tcsetattr(pty.fd(), TCSANOW, &t));

        let raw = pty.raw();
        for (i, (name, token)) in V_TOKENS.iter().enumerate() {
            assert_eq!(raw.special_codes[*token], marker(i), "{name}");
        }
    }

    /// The four flag words, `NCCS`, the three `TCSA*` actions and
    /// `_POSIX_VDISABLE`, against the headers that define them.
    ///
    /// These are addressed by name from the `sem` rules and are not POSIX's to
    /// fix, so every one of them is a number somebody typed in. A wrong bit
    /// here is a mode that quietly never takes effect.
    #[test]
    fn the_flag_words_are_the_ones_the_headers_define() {
        let h = flag_defines();
        for (name, ours) in [
            ("IGNBRK", IGNBRK),
            ("BRKINT", BRKINT),
            ("IGNPAR", IGNPAR),
            ("PARMRK", PARMRK),
            ("INPCK", INPCK),
            ("ISTRIP", ISTRIP),
            ("INLCR", INLCR),
            ("IGNCR", IGNCR),
            ("ICRNL", ICRNL),
            ("IUCLC", IUCLC),
            ("IXON", IXON),
            ("IXANY", IXANY),
            ("IXOFF", IXOFF),
            ("IMAXBEL", IMAXBEL),
            ("OPOST", OPOST),
            ("OLCUC", OLCUC),
            ("ONLCR", ONLCR),
            ("OCRNL", OCRNL),
            ("ONOCR", ONOCR),
            ("ONLRET", ONLRET),
            ("OFILL", OFILL),
            ("OFDEL", OFDEL),
            ("NLDLY", NLDLY),
            ("CRDLY", CRDLY),
            ("TABDLY", TABDLY),
            ("TAB3", TAB3),
            ("XTABS", XTABS),
            ("BSDLY", BSDLY),
            ("VTDLY", VTDLY),
            ("FFDLY", FFDLY),
            ("CBAUD", CBAUD),
            ("CBAUDEX", CBAUDEX),
            ("CSIZE", CSIZE),
            ("CS8", CS8),
            ("CSTOPB", CSTOPB),
            ("CREAD", CREAD),
            ("PARENB", PARENB),
            ("PARODD", PARODD),
            ("HUPCL", HUPCL),
            ("CLOCAL", CLOCAL),
            ("CIBAUD", CIBAUD),
            ("CRTSCTS", CRTSCTS),
            ("ISIG", ISIG),
            ("ICANON", ICANON),
            ("XCASE", XCASE),
            ("ECHO", ECHO),
            ("ECHOE", ECHOE),
            ("ECHOK", ECHOK),
            ("ECHONL", ECHONL),
            ("NOFLSH", NOFLSH),
            ("TOSTOP", TOSTOP),
            ("ECHOCTL", ECHOCTL),
            ("ECHOPRT", ECHOPRT),
            ("ECHOKE", ECHOKE),
            ("FLUSHO", FLUSHO),
            ("PENDIN", PENDIN),
            ("IEXTEN", IEXTEN),
            ("EXTPROC", EXTPROC),
        ] {
            assert_eq!(h[name], i64::from(ours), "{name}");
        }

        assert_eq!(h["NCCS"], NCCS as i64);
        assert_eq!(h["TCSANOW"], i64::from(TCSANOW));
        assert_eq!(h["TCSADRAIN"], i64::from(TCSADRAIN));
        assert_eq!(h["TCSAFLUSH"], i64::from(TCSAFLUSH));
        assert_eq!(h["_POSIX_VDISABLE"], i64::from(VDISABLE));
    }

    /// Three collisions this module's documentation asserts, and which the
    /// port's behaviour is built on. Checked against the headers rather than
    /// against the transcription, because each one changes what code does.
    #[test]
    fn the_flag_collisions_the_port_relies_on_are_real() {
        let h = flag_defines();
        // ERR-terminal-36: `IUCLC` is a `c_iflag` bit and `ECHOCTL` a
        // `c_lflag` one, and on glibc they are the same bit, which is what
        // makes that defect a permanent -1 rather than an occasional one.
        assert_eq!(h["IUCLC"], h["ECHOCTL"]);
        // `ttymodes[]` carries both names, so `+xtabs` and `+tabdly` interact.
        assert_eq!(h["TAB3"], h["XTABS"]);
        // `CS8` is the whole of `CSIZE`, which is why setting the character
        // size to 8 bits is a plain assignment rather than a masked one.
        assert_eq!(h["CS8"], h["CSIZE"]);
        // And `TAB3` is not zero, so the `EL_CAN_TAB` branch of `tty_rawmode`
        // is live here — the degenerate case `sem:tty.tty-rawmode-fn` warns
        // about, where `(x & 0) == 0` is always true, does not arise.
        assert_ne!(h["TAB3"], 0);
    }

    /// The `C_*` control-character defaults, against `<sys/ttydefaults.h>` —
    /// which glibc copied verbatim from BSD, so unlike the subscripts these
    /// really are portable — and against `src/tty.h`'s own fallbacks for the
    /// seven the header does not define.
    #[test]
    fn the_control_character_defaults_are_the_ones_the_headers_define() {
        let h = cchar_defines();
        for (name, ours) in [
            ("CINTR", CINTR),
            ("CQUIT", CQUIT),
            ("CERASE", CERASE),
            ("CKILL", CKILL),
            ("CEOF", CEOF),
            ("CEOL", CEOL),
            ("CEOL2", CEOL2),
            ("CSWTCH", CSWTCH),
            ("CDSWTCH", CDSWTCH),
            ("CERASE2", CERASE2),
            ("CSTART", CSTART),
            ("CSTOP", CSTOP),
            ("CWERASE", CWERASE),
            ("CSUSP", CSUSP),
            ("CDSUSP", CDSUSP),
            ("CREPRINT", CREPRINT),
            ("CDISCARD", CDISCARD),
            ("CLNEXT", CLNEXT),
            ("CSTATUS", CSTATUS),
            ("CPAGE", CPAGE),
            ("CPGOFF", CPGOFF),
            ("CKILL2", CKILL2),
            ("CBRK", CBRK),
            ("CMIN", CMIN),
            ("CTIME", CTIME),
        ] {
            assert_eq!(h[name], i64::from(ours), "{name}");
        }

        // ERR-terminal-43 says `tty.h`'s `CMIN`/`CTIME` fallbacks are the
        // nonsense `CEOF`/`CEOL`. They are unreachable on any platform that
        // ships `<sys/ttydefaults.h>`, and this is the assertion that says so:
        // the header's own values are 1 and 0, which is what we carry.
        assert_eq!((CMIN, CTIME), (1, 0));
    }

    /// A line at an arbitrary speed survives a `tcsetattr` that never meant to
    /// touch the speed.
    ///
    /// This is why [`tcsetattr`] reads before it writes. rustix issues
    /// `TCSETS2`, which carries `c_ispeed`/`c_ospeed`; libedit's `struct
    /// termios` has neither, because glibc's `tcsetattr` sends a struct with
    /// no speed fields at all and the speed lives in the `CBAUD` bits of
    /// `c_cflag`. Seed the call from a zeroed struct instead of the live
    /// settings and a `BOTHER` line loses its speed — which, at zero, is a
    /// hangup.
    #[test]
    fn an_arbitrary_line_speed_survives_a_call_that_does_not_touch_it() {
        const ODD_SPEED: u32 = 12_345;

        let pty = Pty::open();
        let mut raw = pty.raw();
        raw.set_speed(ODD_SPEED).expect("an arbitrary speed");
        pty.set_raw(&raw);
        assert_eq!(
            pty.raw().output_speed(),
            ODD_SPEED,
            "the pty took the speed"
        );

        // A change to something else entirely, of the shape `tty_rawmode`
        // makes: one bit of `c_lflag`, nothing about the line.
        let mut t = tcgetattr(pty.fd()).expect("tcgetattr");
        t.c_lflag &= !ECHO;
        assert!(tcsetattr(pty.fd(), TCSANOW, &t));

        let after = pty.raw();
        assert_eq!(after.output_speed(), ODD_SPEED);
        assert_eq!(after.input_speed(), ODD_SPEED);
        assert_eq!(
            tcgetattr(pty.fd()).expect("tcgetattr").c_lflag & ECHO,
            0,
            "the change we did ask for did not happen"
        );
    }

    /// The window size is rows then columns, in that order. `TIOCGWINSZ`
    /// hands back a `struct winsize` whose first two fields are `ws_row` and
    /// `ws_col`, and a pair swapped here would be an editor that wraps at 24
    /// columns on an 80-column terminal.
    #[test]
    fn the_window_size_is_rows_then_columns() {
        let pty = Pty::open();
        // Deliberately not 24x80: two numbers that cannot be mistaken for one
        // another, and neither of them a plausible default.
        let want = rustix::termios::Winsize {
            ws_row: 37,
            ws_col: 113,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        rustix::termios::tcsetwinsize(&pty.terminal, want).expect("TIOCSWINSZ");
        assert_eq!(window_size(pty.fd()), Some((37, 113)));
    }

    /// `isatty` answers for what the descriptor is, not for whether it is a
    /// character device — `/dev/null` is one and is not a terminal.
    #[test]
    fn isatty_distinguishes_a_terminal_from_another_character_device() {
        let pty = Pty::open();
        assert!(isatty(pty.fd()));

        let null = std::fs::File::open("/dev/null").expect("/dev/null");
        assert!(!isatty(null.as_raw_fd()));
    }

    /// The port hands out -1 for a stream with no descriptor
    /// (`sem:el.el-init-fn`, ERR-core-api-06), so this is a live path rather
    /// than a defensive one, and every one of these must answer the C's
    /// failure rather than reach a syscall with a bad argument.
    #[test]
    fn a_negative_descriptor_fails_the_way_the_c_does() {
        assert!(!isatty(-1));
        assert!(tcgetattr(-1).is_none());
        assert!(!tcsetattr(-1, TCSADRAIN, &Termios::default()));
        assert!(window_size(-1).is_none());
    }

    /// `tcsetattr` maps libedit's `action` onto rustix's enum by value, and
    /// anything outside the three is the C's `EINVAL`.
    #[test]
    fn an_unknown_tcsetattr_action_is_rejected() {
        assert!(!tcsetattr(0, 99, &Termios::default()));
    }

    /// A `tcsetattr` action the C would accept still fails on a descriptor
    /// that is not a terminal, rather than being reported as applied.
    #[test]
    fn a_non_terminal_descriptor_cannot_be_configured() {
        let null = std::fs::File::open("/dev/null").expect("/dev/null");
        let fd = null.as_raw_fd();
        assert!(tcgetattr(fd).is_none());
        assert!(!tcsetattr(fd, TCSAFLUSH, &Termios::default()));
        assert!(window_size(fd).is_none());
    }

    /// A distinct, printable, non-zero byte per table position, so that a
    /// value landing in the wrong slot names the slot it came from.
    fn marker(i: usize) -> u8 {
        b'A' + u8::try_from(i).expect("a table position")
    }

    fn v_defines() -> HashMap<String, i64> {
        cheader::defines(&["bits/termios-c_cc.h"])
    }

    fn flag_defines() -> HashMap<String, i64> {
        cheader::defines(&[
            "bits/termios-c_iflag.h",
            "bits/termios-c_oflag.h",
            "bits/termios-c_cflag.h",
            "bits/termios-c_lflag.h",
            "bits/termios-baud.h",
            "bits/termios-struct.h",
            "bits/termios-tcflow.h",
            "bits/posix_opt.h",
        ])
    }

    /// `<sys/ttydefaults.h>` is the authority for eighteen of the `C_*`
    /// defaults. The other seven — `CEOL2`, `CSWTCH`, `CDSWTCH`, `CERASE2`,
    /// `CPAGE`, `CPGOFF`, `CKILL2` — no header defines, so `src/tty.h`
    /// supplies them and is read first, letting the system header win wherever
    /// both have an opinion. That is exactly the order the C preprocessor sees
    /// them in, since `tty.h` guards every one with `#ifndef`.
    fn cchar_defines() -> HashMap<String, i64> {
        let tty_h = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/tty.h");
        let text =
            std::fs::read_to_string(&tty_h).unwrap_or_else(|e| panic!("{}: {e}", tty_h.display()));
        cheader::Defines::new()
            .read_text(&text)
            .read(&["bits/posix_opt.h", "sys/ttydefaults.h"])
            .resolve()
    }
}
