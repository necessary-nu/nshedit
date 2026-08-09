//! Linux/glibc termios constants and `V*` subscripts.

use rustix::termios::SpecialCodeIndex;

pub(crate) type FlagBits = u32;

pub(crate) const VDISABLE: u8 = 0;
pub(crate) const NCCS: usize = 32;

// `<bits/termios-c_iflag.h>`.
pub(crate) const IGNBRK: FlagBits = 0o0000001;
pub(crate) const BRKINT: FlagBits = 0o0000002;
pub(crate) const IGNPAR: FlagBits = 0o0000004;
pub(crate) const PARMRK: FlagBits = 0o0000010;
pub(crate) const INPCK: FlagBits = 0o0000020;
pub(crate) const ISTRIP: FlagBits = 0o0000040;
pub(crate) const INLCR: FlagBits = 0o0000100;
pub(crate) const IGNCR: FlagBits = 0o0000200;
pub(crate) const ICRNL: FlagBits = 0o0000400;
pub(crate) const IUCLC: FlagBits = 0o0001000;
pub(crate) const IXON: FlagBits = 0o0002000;
pub(crate) const IXANY: FlagBits = 0o0004000;
pub(crate) const IXOFF: FlagBits = 0o0010000;
pub(crate) const IMAXBEL: FlagBits = 0o0020000;

// `<bits/termios-c_oflag.h>`.
pub(crate) const OPOST: FlagBits = 0o0000001;
pub(crate) const OLCUC: FlagBits = 0o0000002;
pub(crate) const ONLCR: FlagBits = 0o0000004;
pub(crate) const OCRNL: FlagBits = 0o0000010;
pub(crate) const ONOCR: FlagBits = 0o0000020;
pub(crate) const ONLRET: FlagBits = 0o0000040;
pub(crate) const OFILL: FlagBits = 0o0000100;
pub(crate) const OFDEL: FlagBits = 0o0000200;
pub(crate) const NLDLY: FlagBits = 0o0000400;
pub(crate) const CRDLY: FlagBits = 0o0003000;
pub(crate) const TABDLY: FlagBits = 0o0014000;
pub(crate) const XTABS: FlagBits = 0o0014000;
pub(crate) const BSDLY: FlagBits = 0o0020000;
pub(crate) const VTDLY: FlagBits = 0o0040000;
pub(crate) const FFDLY: FlagBits = 0o0100000;

// `<bits/termios-c_cflag.h>`.
pub(crate) const CBAUD: FlagBits = 0o0010017;
pub(crate) const CSTOPB: FlagBits = 0o0000100;
pub(crate) const CREAD: FlagBits = 0o0000200;
pub(crate) const PARENB: FlagBits = 0o0000400;
pub(crate) const PARODD: FlagBits = 0o0001000;
pub(crate) const HUPCL: FlagBits = 0o0002000;
pub(crate) const CLOCAL: FlagBits = 0o0004000;
pub(crate) const CIBAUD: FlagBits = 0o02003600000;
pub(crate) const CRTSCTS: FlagBits = 0o020000000000;

// `<bits/termios-c_lflag.h>`.
pub(crate) const ISIG: FlagBits = 0o0000001;
pub(crate) const ICANON: FlagBits = 0o0000002;
pub(crate) const XCASE: FlagBits = 0o0000004;
pub(crate) const ECHO: FlagBits = 0o0000010;
pub(crate) const ECHOE: FlagBits = 0o0000020;
pub(crate) const ECHOK: FlagBits = 0o0000040;
pub(crate) const ECHONL: FlagBits = 0o0000100;
pub(crate) const NOFLSH: FlagBits = 0o0000200;
pub(crate) const TOSTOP: FlagBits = 0o0000400;
pub(crate) const ECHOCTL: FlagBits = 0o0001000;
pub(crate) const ECHOPRT: FlagBits = 0o0002000;
pub(crate) const ECHOKE: FlagBits = 0o0004000;
pub(crate) const FLUSHO: FlagBits = 0o0010000;
pub(crate) const PENDIN: FlagBits = 0o0040000;
pub(crate) const IEXTEN: FlagBits = 0o0100000;
pub(crate) const EXTPROC: FlagBits = 0o0200000;

// `<bits/termios-c_cc.h>`.
pub(crate) const VINTR: usize = 0;
pub(crate) const VQUIT: usize = 1;
pub(crate) const VERASE: usize = 2;
pub(crate) const VKILL: usize = 3;
pub(crate) const VEOF: usize = 4;
pub(crate) const VTIME: usize = 5;
pub(crate) const VMIN: usize = 6;
pub(crate) const VSWTC: usize = 7;
pub(crate) const VSTART: usize = 8;
pub(crate) const VSTOP: usize = 9;
pub(crate) const VSUSP: usize = 10;
pub(crate) const VEOL: usize = 11;
pub(crate) const VREPRINT: usize = 12;
pub(crate) const VDISCARD: usize = 13;
pub(crate) const VWERASE: usize = 14;
pub(crate) const VLNEXT: usize = 15;
pub(crate) const VEOL2: usize = 16;

/// Every Linux `V*` slot rustix exposes. `VSWTC` has no libedit vocabulary,
/// but is copied so restoring an original terminal snapshot does not lose it.
pub(crate) const CC_INDICES: [(usize, SpecialCodeIndex); 17] = [
    (VINTR, SpecialCodeIndex::VINTR),
    (VQUIT, SpecialCodeIndex::VQUIT),
    (VERASE, SpecialCodeIndex::VERASE),
    (VKILL, SpecialCodeIndex::VKILL),
    (VEOF, SpecialCodeIndex::VEOF),
    (VTIME, SpecialCodeIndex::VTIME),
    (VMIN, SpecialCodeIndex::VMIN),
    (VSWTC, SpecialCodeIndex::VSWTC),
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
