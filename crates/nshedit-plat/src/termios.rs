//! The termios ABI, the window-size query, and `isatty`.
//!
//! No libc here: `tcgetattr`, `tcsetattr`, `tcgetwinsize` and `isatty` all go
//! through rustix. What rustix cannot supply as public constants — the flag
//! bits, `V*` subscripts, `NCCS`, `_POSIX_VDISABLE`, and the complete C
//! `struct termios` shape — is selected from an explicit Linux or Darwin
//! transcription. POSIX fixes the names but not those representations.

use std::io;
use std::os::fd::BorrowedFd;

use rustix::termios::OptionalActions;
#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
use rustix::termios::SpecialCodeIndex;

// [spec:nshedit:req:platform.per-os-layouts]
#[cfg(any(target_os = "linux", target_os = "android"))]
#[path = "termios/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "termios/darwin.rs"]
mod platform;
#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
compile_error!("nshedit-plat has no termios ABI transcription for this target");

pub(crate) use platform::*;

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
pub(crate) const CDSUSP: u8 = ctrl(b'y');
pub(crate) const CSTATUS: u8 = ctrl(b't');
pub(crate) const CMIN: u8 = 1;
pub(crate) const CTIME: u8 = 0;

/// The platform `<termios.h>` structure projected into libedit's snapshot.
///
/// Linux/glibc carries four 32-bit words and `NCCS == 32`; Darwin carries
/// four 64-bit words, `NCCS == 20`, and explicit input/output speed fields.
/// It remains deliberately distinct from rustix's private representation.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Termios {
    pub(crate) c_iflag: FlagBits,
    pub(crate) c_oflag: FlagBits,
    pub(crate) c_cflag: FlagBits,
    pub(crate) c_lflag: FlagBits,
    /// Indexed by the `V*` subscripts above, not by libedit's `C_*` ones.
    pub(crate) c_cc: [u8; NCCS],
    #[cfg(target_os = "macos")]
    pub(crate) c_ispeed: u64,
    #[cfg(target_os = "macos")]
    pub(crate) c_ospeed: u64,
}

impl Default for Termios {
    fn default() -> Self {
        Self {
            c_iflag: 0,
            c_oflag: 0,
            c_cflag: 0,
            c_lflag: 0,
            c_cc: [VDISABLE; NCCS],
            #[cfg(target_os = "macos")]
            c_ispeed: 0,
            #[cfg(target_os = "macos")]
            c_ospeed: 0,
        }
    }
}

/// glibc's published 64-bit `<termios.h>` layout.
#[cfg(any(target_os = "linux", target_os = "android"))]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};

    assert!(size_of::<Termios>() == 48);
    assert!(align_of::<Termios>() == 4);
    assert!(offset_of!(Termios, c_iflag) == 0);
    assert!(offset_of!(Termios, c_oflag) == 4);
    assert!(offset_of!(Termios, c_cflag) == 8);
    assert!(offset_of!(Termios, c_lflag) == 12);
    assert!(offset_of!(Termios, c_cc) == 16);
};

/// Darwin's published 64-bit `<sys/termios.h>` layout.
#[cfg(target_os = "macos")]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};

    assert!(size_of::<Termios>() == 72);
    assert!(align_of::<Termios>() == 8);
    assert!(offset_of!(Termios, c_iflag) == 0);
    assert!(offset_of!(Termios, c_oflag) == 8);
    assert!(offset_of!(Termios, c_cflag) == 16);
    assert!(offset_of!(Termios, c_lflag) == 24);
    assert!(offset_of!(Termios, c_cc) == 32);
    assert!(offset_of!(Termios, c_ispeed) == 56);
    assert!(offset_of!(Termios, c_ospeed) == 64);
};

/// The Linux `speed_t` encoding carried by these attributes.
#[must_use]
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) const fn encoded_baud_rate(attributes: &Termios) -> u32 {
    attributes.c_cflag & CBAUD
}

/// Translate Linux's `speed_t` encoding into transmitted bits per second.
///
/// `BOTHER` is intentionally absent: this compact compatibility structure
/// carries the encoding bits but not the separate arbitrary-speed fields of
/// `termios2`, so no truthful semantic rate can be recovered for it.
#[must_use]
#[cfg(any(target_os = "linux", target_os = "android"))]
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

/// Darwin carries the actual output speed in `c_ospeed`, not a `CBAUD` mask.
#[must_use]
#[cfg(target_os = "macos")]
pub(crate) const fn baud_rate(attributes: &Termios) -> Option<u32> {
    if attributes.c_ospeed <= u32::MAX as u64 {
        Some(attributes.c_ospeed as u32)
    } else {
        None
    }
}

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
        c_cc: [VDISABLE; NCCS],
        #[cfg(target_os = "macos")]
        c_ispeed: u64::from(raw.input_speed()),
        #[cfg(target_os = "macos")]
        c_ospeed: u64::from(raw.output_speed()),
    };
    for (v, index) in CC_INDICES {
        t.c_cc[v] = raw.special_codes[index];
    }
    Ok(t)
}

/// Apply terminal attributes, retrying interrupted calls.
///
/// The current settings are read first, then the platform snapshot is written
/// over them. On Linux this preserves rustix's separate arbitrary-speed state
/// while applying the glibc `CBAUD` projection. On Darwin the snapshot's own
/// `c_ispeed` and `c_ospeed` fields are applied explicitly.
pub(crate) fn apply(fd: BorrowedFd<'_>, action: OptionalActions, t: &Termios) -> io::Result<()> {
    let mut raw = read_raw(fd)?;
    raw.input_modes = rustix::termios::InputModes::from_bits_retain(t.c_iflag);
    raw.output_modes = rustix::termios::OutputModes::from_bits_retain(t.c_oflag);
    raw.control_modes = rustix::termios::ControlModes::from_bits_retain(t.c_cflag);
    raw.local_modes = rustix::termios::LocalModes::from_bits_retain(t.c_lflag);
    #[cfg(target_os = "macos")]
    {
        let input_speed = u32::try_from(t.c_ispeed).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "input speed exceeds speed_t")
        })?;
        let output_speed = u32::try_from(t.c_ospeed).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "output speed exceeds speed_t")
        })?;
        raw.set_input_speed(input_speed)?;
        raw.set_output_speed(output_speed)?;
    }
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
/// The C also has conditional support for the older `TIOCGSIZE`/`ttysize`
/// interface. Both supported targets provide `TIOCGWINSZ`, which is the typed
/// interface rustix exposes here.
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
    use std::collections::HashMap;
    use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

    use super::*;
    #[cfg(any(target_os = "linux", target_os = "android"))]
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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

    /// Every represented Linux `V*` name against glibc's header.
    ///
    /// An off-by-one in this table announces nothing; it silently rebinds the
    /// user's erase or interrupt character to whatever sits next to it.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[test]
    fn the_v_subscripts_are_the_ones_the_header_defines() {
        let h = v_defines();
        for &(name, token) in &V_TOKENS {
            let (ours, _) = CC_INDICES
                .iter()
                .find(|(_, represented)| *represented == token)
                .unwrap_or_else(|| panic!("{name} is missing from the platform table"));
            assert_eq!(h[name], *ours as i64, "{name}");
        }
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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

        assert!(got.c_cc[17..].iter().all(|&byte| byte == VDISABLE));
    }

    /// And the same pairing in the direction `tcsetattr` uses it: written by
    /// number, read back by token.
    #[cfg(any(target_os = "linux", target_os = "android"))]
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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

    /// The `C_*` control-character defaults, against
    /// `<sys/ttydefaults.h>`. CEOL2 is nshedit's explicit disabled default
    /// because the system header does not define it.
    #[cfg(any(target_os = "linux", target_os = "android"))]
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
        assert_eq!(CEOL2, VDISABLE);

        // The system defaults are 1 and 0, which is what we carry.
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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

    /// Darwin stores input and output speeds in separate fields. Reading and
    /// applying an unrelated local-mode change must preserve both values.
    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_preserves_separate_speeds() {
        const INPUT_SPEED: u32 = 19_200;
        const OUTPUT_SPEED: u32 = 38_400;

        let pty = Pty::open();
        let mut raw = pty.raw();
        raw.set_input_speed(INPUT_SPEED).expect("input speed");
        raw.set_output_speed(OUTPUT_SPEED).expect("output speed");
        pty.set_raw(&raw);

        let mut attributes = read(pty.fd()).expect("read terminal attributes");
        assert_eq!(attributes.c_ispeed, u64::from(INPUT_SPEED));
        assert_eq!(attributes.c_ospeed, u64::from(OUTPUT_SPEED));
        attributes.c_lflag &= !ECHO;
        apply(pty.fd(), OptionalActions::Now, &attributes).expect("apply terminal attributes");

        let after = pty.raw();
        assert_eq!(after.input_speed(), INPUT_SPEED);
        assert_eq!(after.output_speed(), OUTPUT_SPEED);
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn marker(i: usize) -> u8 {
        b'A' + u8::try_from(i).expect("a table position")
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn v_defines() -> HashMap<String, i64> {
        cheader::defines(&["bits/termios-c_cc.h"])
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
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

    /// The platform headers are the authority for the defaults they publish.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    fn cchar_defines() -> HashMap<String, i64> {
        cheader::defines(&["bits/posix_opt.h", "sys/ttydefaults.h"])
    }
}
