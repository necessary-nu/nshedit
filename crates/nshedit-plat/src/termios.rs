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

use std::io;
use std::os::fd::BorrowedFd;

use rustix::termios::{OptionalActions, SpecialCodeIndex};

/// `_POSIX_VDISABLE`. POSIX defines the constant but not its value;
/// glibc/Linux uses 0 and the BSDs and macOS use 0xff. `tty.h` falls back to
/// `(unsigned char)-1` where the platform defines neither `_POSIX_VDISABLE`
/// nor `VDISABLE`, which is the 0xff arm.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) const VDISABLE: u8 = 0;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub(crate) const VDISABLE: u8 = 0xff;

/// `NCCS` — the length of a `struct termios`'s `c_cc`.
///
/// This is **glibc's** 32, which is the array libedit's own `struct termios`
/// carries and therefore the one the `def` rules describe. The kernel's
/// `struct termios` is 19 long, and glibc's `tcgetattr` copies those 19 and
/// fills the tail with `_POSIX_VDISABLE`; [`tcgetattr`] below does the same,
/// which on Linux means leaving it zero.
pub(crate) const NCCS: usize = 32;

// `c_iflag` bits.
pub(crate) const IGNBRK: u32 = 0o0000001;
pub(crate) const BRKINT: u32 = 0o0000002;
pub(crate) const IGNPAR: u32 = 0o0000004;
pub(crate) const PARMRK: u32 = 0o0000010;
pub(crate) const INPCK: u32 = 0o0000020;
pub(crate) const ISTRIP: u32 = 0o0000040;
pub(crate) const INLCR: u32 = 0o0000100;
pub(crate) const IGNCR: u32 = 0o0000200;
pub(crate) const ICRNL: u32 = 0o0000400;
/// Legacy SysV, Linux only. Note it is the *same bit* as [`ECHOCTL`], which
/// is what makes ERR-terminal-36 the silent no-op it is.
pub(crate) const IUCLC: u32 = 0o0001000;
pub(crate) const IXON: u32 = 0o0002000;
pub(crate) const IXANY: u32 = 0o0004000;
pub(crate) const IXOFF: u32 = 0o0010000;
pub(crate) const IMAXBEL: u32 = 0o0020000;

// `c_oflag` bits.
pub(crate) const OPOST: u32 = 0o0000001;
pub(crate) const OLCUC: u32 = 0o0000002;
pub(crate) const ONLCR: u32 = 0o0000004;
pub(crate) const OCRNL: u32 = 0o0000010;
pub(crate) const ONOCR: u32 = 0o0000020;
pub(crate) const ONLRET: u32 = 0o0000040;
pub(crate) const OFILL: u32 = 0o0000100;
pub(crate) const OFDEL: u32 = 0o0000200;
pub(crate) const NLDLY: u32 = 0o0000400;
pub(crate) const CRDLY: u32 = 0o0003000;
pub(crate) const TABDLY: u32 = 0o0014000;
/// Expand tabs to spaces. Linux gives `XTABS`, `TAB3`, and `TABDLY` the same
/// mask, so the compatibility names interact.
pub(crate) const XTABS: u32 = 0o0014000;
pub(crate) const BSDLY: u32 = 0o0020000;
pub(crate) const VTDLY: u32 = 0o0040000;
pub(crate) const FFDLY: u32 = 0o0100000;

// `c_cflag` bits.
pub(crate) const CBAUD: u32 = 0o0010017;
pub(crate) const CSTOPB: u32 = 0o0000100;
pub(crate) const CREAD: u32 = 0o0000200;
pub(crate) const PARENB: u32 = 0o0000400;
pub(crate) const PARODD: u32 = 0o0001000;
pub(crate) const HUPCL: u32 = 0o0002000;
pub(crate) const CLOCAL: u32 = 0o0004000;
pub(crate) const CIBAUD: u32 = 0o02003600000;
pub(crate) const CRTSCTS: u32 = 0o020000000000;

// `c_lflag` bits.
pub(crate) const ISIG: u32 = 0o0000001;
pub(crate) const ICANON: u32 = 0o0000002;
pub(crate) const XCASE: u32 = 0o0000004;
pub(crate) const ECHO: u32 = 0o0000010;
pub(crate) const ECHOE: u32 = 0o0000020;
pub(crate) const ECHOK: u32 = 0o0000040;
pub(crate) const ECHONL: u32 = 0o0000100;
pub(crate) const NOFLSH: u32 = 0o0000200;
pub(crate) const TOSTOP: u32 = 0o0000400;
/// Echo control characters as `^X`. A `c_lflag` bit, and on glibc the same
/// value as the `c_iflag` bit [`IUCLC`] — the coincidence ERR-terminal-36
/// turns into a permanent -1.
pub(crate) const ECHOCTL: u32 = 0o0001000;
pub(crate) const ECHOPRT: u32 = 0o0002000;
pub(crate) const ECHOKE: u32 = 0o0004000;
pub(crate) const FLUSHO: u32 = 0o0010000;
pub(crate) const PENDIN: u32 = 0o0040000;
pub(crate) const IEXTEN: u32 = 0o0100000;
pub(crate) const EXTPROC: u32 = 0o0200000;

// The termios `V*` subscripts this platform defines, as the C sees them after
// `tty.h`'s aliasing. glibc has no `VSWTCH` (only `VSWTC`, which `tty.c`
// never names), no `VDSWTCH`, `VERASE2`, `VDSUSP`, `VSTATUS`, `VPAGE`,
// `VPGOFF`, `VKILL2` or `VBRK`, so those rows of every table in `tty.rs` are
// simply absent — which is what `#ifdef`ing them out means.
pub(crate) const VINTR: usize = 0;
pub(crate) const VQUIT: usize = 1;
pub(crate) const VERASE: usize = 2;
pub(crate) const VKILL: usize = 3;
pub(crate) const VEOF: usize = 4;
pub(crate) const VTIME: usize = 5;
pub(crate) const VMIN: usize = 6;
pub(crate) const VSTART: usize = 8;
pub(crate) const VSTOP: usize = 9;
pub(crate) const VSUSP: usize = 10;
pub(crate) const VEOL: usize = 11;
pub(crate) const VREPRINT: usize = 12;
pub(crate) const VDISCARD: usize = 13;
pub(crate) const VWERASE: usize = 14;
pub(crate) const VLNEXT: usize = 15;
pub(crate) const VEOL2: usize = 16;

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
pub(crate) const CINTR: u8 = ctrl(b'c');
pub(crate) const CQUIT: u8 = 0o34;
pub(crate) const CERASE: u8 = 0o177;
pub(crate) const CKILL: u8 = ctrl(b'u');
pub(crate) const CEOF: u8 = ctrl(b'd');
pub(crate) const CEOL: u8 = VDISABLE;
pub(crate) const CEOL2: u8 = VDISABLE;
pub(crate) const CSTART: u8 = ctrl(b'q');
pub(crate) const CSTOP: u8 = ctrl(b's');
pub(crate) const CWERASE: u8 = ctrl(b'w');
pub(crate) const CSUSP: u8 = ctrl(b'z');
pub(crate) const CREPRINT: u8 = ctrl(b'r');
pub(crate) const CDISCARD: u8 = ctrl(b'o');
pub(crate) const CLNEXT: u8 = ctrl(b'v');
pub(crate) const CMIN: u8 = 1;
pub(crate) const CTIME: u8 = 0;

/// The kernel's `struct termios`, as libedit's four flag words and `c_cc`.
///
/// Deliberately **not** rustix's `Termios`: `def:tty.el-tty-t` freezes
/// libedit's own shape, whose `c_cc` is [`NCCS`] long and which carries no
/// `c_ispeed`/`c_ospeed` because glibc's `cfgetospeed` reads the `CBAUD` bits
/// of `c_cflag` and its `tcsetattr` sends the kernel a struct with no speed
/// fields at all. Only `CBAUD` is load-bearing, which is exactly what this
/// carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Termios {
    pub(crate) c_iflag: u32,
    pub(crate) c_oflag: u32,
    pub(crate) c_cflag: u32,
    pub(crate) c_lflag: u32,
    /// Indexed by the `V*` subscripts above, not by libedit's `C_*` ones.
    pub(crate) c_cc: [u8; NCCS],
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

/// The Linux `speed_t` encoding carried by these attributes.
#[must_use]
pub(crate) const fn encoded_baud_rate(attributes: &Termios) -> u32 {
    attributes.c_cflag & CBAUD
}

/// Translate Linux's `speed_t` encoding into transmitted bits per second.
///
/// `BOTHER` is intentionally absent: this compact compatibility structure
/// carries the encoding bits but not the separate arbitrary-speed fields of
/// `termios2`, so no truthful semantic rate can be recovered for it.
#[must_use]
pub(crate) const fn baud_rate(attributes: &Termios) -> Option<u32> {
    match encoded_baud_rate(attributes) {
        0o0000000 => Some(0),
        0o0000001 => Some(50),
        0o0000002 => Some(75),
        0o0000003 => Some(110),
        0o0000004 => Some(134),
        0o0000005 => Some(150),
        0o0000006 => Some(200),
        0o0000007 => Some(300),
        0o0000010 => Some(600),
        0o0000011 => Some(1_200),
        0o0000012 => Some(1_800),
        0o0000013 => Some(2_400),
        0o0000014 => Some(4_800),
        0o0000015 => Some(9_600),
        0o0000016 => Some(19_200),
        0o0000017 => Some(38_400),
        0o0010001 => Some(57_600),
        0o0010002 => Some(115_200),
        0o0010003 => Some(230_400),
        0o0010004 => Some(460_800),
        0o0010005 => Some(500_000),
        0o0010006 => Some(576_000),
        0o0010007 => Some(921_600),
        0o0010010 => Some(1_000_000),
        0o0010011 => Some(1_152_000),
        0o0010012 => Some(1_500_000),
        0o0010013 => Some(2_000_000),
        0o0010014 => Some(2_500_000),
        0o0010015 => Some(3_000_000),
        0o0010016 => Some(3_500_000),
        0o0010017 => Some(4_000_000),
        _ => None,
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

/// Whether a borrowed descriptor names a terminal, preserving failures other
/// than the ordinary `NOTTY` answer.
pub(crate) fn is_terminal(fd: BorrowedFd<'_>) -> io::Result<bool> {
    loop {
        match rustix::termios::tcgetattr(fd) {
            Ok(_) => return Ok(true),
            Err(rustix::io::Errno::NOTTY) => return Ok(false),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

/// Read terminal attributes, retrying interrupted calls.
pub(crate) fn read(fd: BorrowedFd<'_>) -> io::Result<Termios> {
    let raw = read_raw(fd)?;
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
    Ok(t)
}

/// Apply terminal attributes, retrying interrupted calls.
///
/// The current settings are read first and the four flag words and `c_cc`
/// written over them. That is not belt-and-braces: rustix uses `TCSETS2`,
/// which carries `c_ispeed`/`c_ospeed`, and libedit's `struct termios` has
/// neither. Seeding from the live settings is precisely what the kernel does
/// for glibc's `TCSETS` (`tmp_termios = tty->termios` before the copy in),
/// so an arbitrary `BOTHER` line speed survives a call that does not change
/// `CBAUD` — where a zeroed seed would hang the line up.
pub(crate) fn apply(fd: BorrowedFd<'_>, action: OptionalActions, t: &Termios) -> io::Result<()> {
    let mut raw = read_raw(fd)?;
    raw.input_modes = rustix::termios::InputModes::from_bits_retain(t.c_iflag);
    raw.output_modes = rustix::termios::OutputModes::from_bits_retain(t.c_oflag);
    raw.control_modes = rustix::termios::ControlModes::from_bits_retain(t.c_cflag);
    raw.local_modes = rustix::termios::LocalModes::from_bits_retain(t.c_lflag);
    for (v, index) in CC_INDICES {
        raw.special_codes[index] = t.c_cc[v];
    }
    loop {
        match rustix::termios::tcsetattr(fd, action, &raw) {
            Ok(()) => return Ok(()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
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
pub(crate) fn screen_size(fd: BorrowedFd<'_>) -> io::Result<(u16, u16)> {
    let ws = rustix::termios::tcgetwinsize(fd)?;
    Ok((ws.ws_row, ws.ws_col))
}

/// The `EINTR` retry loop `sem:tty.tty-getty-fn` specifies, around rustix's
/// typed `tcgetattr`.
fn read_raw(fd: BorrowedFd<'_>) -> io::Result<rustix::termios::Termios> {
    loop {
        match rustix::termios::tcgetattr(fd) {
            Ok(t) => return Ok(t),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

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

        fn fd(&self) -> BorrowedFd<'_> {
            self.terminal.as_fd()
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

        let got = read(pty.fd()).expect("read terminal attributes");
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

        let mut t = read(pty.fd()).expect("read terminal attributes");
        for (i, (name, _)) in V_TOKENS.iter().enumerate() {
            let subscript = usize::try_from(h[*name]).expect("a subscript");
            t.c_cc[subscript] = marker(i);
        }
        apply(pty.fd(), OptionalActions::Now, &t).expect("apply terminal attributes");

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
            ("XTABS", XTABS),
            ("BSDLY", BSDLY),
            ("VTDLY", VTDLY),
            ("FFDLY", FFDLY),
            ("CBAUD", CBAUD),
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
            ("CSTART", CSTART),
            ("CSTOP", CSTOP),
            ("CWERASE", CWERASE),
            ("CSUSP", CSUSP),
            ("CREPRINT", CREPRINT),
            ("CDISCARD", CDISCARD),
            ("CLNEXT", CLNEXT),
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
        let mut t = read(pty.fd()).expect("read terminal attributes");
        t.c_lflag &= !ECHO;
        apply(pty.fd(), OptionalActions::Now, &t).expect("apply terminal attributes");

        let after = pty.raw();
        assert_eq!(after.output_speed(), ODD_SPEED);
        assert_eq!(after.input_speed(), ODD_SPEED);
        assert_eq!(
            read(pty.fd()).expect("read terminal attributes").c_lflag & ECHO,
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
        assert_eq!(screen_size(pty.fd()).expect("read screen size"), (37, 113));
    }

    /// `isatty` answers for what the descriptor is, not for whether it is a
    /// character device — `/dev/null` is one and is not a terminal.
    #[test]
    fn isatty_distinguishes_a_terminal_from_another_character_device() {
        let pty = Pty::open();
        assert!(is_terminal(pty.fd()).expect("terminal query"));

        let null = std::fs::File::open("/dev/null").expect("/dev/null");
        assert!(!is_terminal(null.as_fd()).expect("terminal query"));
    }

    /// A borrowed non-terminal descriptor cannot be read or configured as a
    /// terminal.
    #[test]
    fn a_non_terminal_descriptor_cannot_be_configured() {
        let null = std::fs::File::open("/dev/null").expect("/dev/null");
        let fd = null.as_fd();
        assert!(read(fd).is_err());
        assert!(apply(fd, OptionalActions::Flush, &Termios::default()).is_err());
        assert!(screen_size(fd).is_err());
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
