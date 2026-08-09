//! Darwin's published `<sys/termios.h>` representation.

use rustix::termios::SpecialCodeIndex;

// [spec:nshedit:req:platform.darwin-termios]
pub(crate) type FlagBits = u64;

pub(crate) const VDISABLE: u8 = 0xff;
pub(crate) const NCCS: usize = 20;

// `c_iflag` bits from XNU's `bsd/sys/termios.h`.
pub(crate) const IGNBRK: FlagBits = 0x0000_0001;
pub(crate) const BRKINT: FlagBits = 0x0000_0002;
pub(crate) const IGNPAR: FlagBits = 0x0000_0004;
pub(crate) const PARMRK: FlagBits = 0x0000_0008;
pub(crate) const INPCK: FlagBits = 0x0000_0010;
pub(crate) const ISTRIP: FlagBits = 0x0000_0020;
pub(crate) const INLCR: FlagBits = 0x0000_0040;
pub(crate) const IGNCR: FlagBits = 0x0000_0080;
pub(crate) const ICRNL: FlagBits = 0x0000_0100;
pub(crate) const IXON: FlagBits = 0x0000_0200;
pub(crate) const IXOFF: FlagBits = 0x0000_0400;
pub(crate) const IXANY: FlagBits = 0x0000_0800;
pub(crate) const IMAXBEL: FlagBits = 0x0000_2000;

// `c_oflag` bits.
pub(crate) const OPOST: FlagBits = 0x0000_0001;
pub(crate) const ONLCR: FlagBits = 0x0000_0002;
pub(crate) const ONOEOT: FlagBits = 0x0000_0008;
pub(crate) const OCRNL: FlagBits = 0x0000_0010;
pub(crate) const ONOCR: FlagBits = 0x0000_0020;
pub(crate) const ONLRET: FlagBits = 0x0000_0040;
pub(crate) const OFILL: FlagBits = 0x0000_0080;
pub(crate) const NLDLY: FlagBits = 0x0000_0300;
pub(crate) const TABDLY: FlagBits = 0x0000_0c04;
pub(crate) const CRDLY: FlagBits = 0x0000_3000;
pub(crate) const FFDLY: FlagBits = 0x0000_4000;
pub(crate) const BSDLY: FlagBits = 0x0000_8000;
pub(crate) const VTDLY: FlagBits = 0x0001_0000;
pub(crate) const OFDEL: FlagBits = 0x0002_0000;

// `c_cflag` bits. Darwin has no CBAUD/CIBAUD projection: speeds are fields.
pub(crate) const CIGNORE: FlagBits = 0x0000_0001;
pub(crate) const CSTOPB: FlagBits = 0x0000_0400;
pub(crate) const CREAD: FlagBits = 0x0000_0800;
pub(crate) const PARENB: FlagBits = 0x0000_1000;
pub(crate) const PARODD: FlagBits = 0x0000_2000;
pub(crate) const HUPCL: FlagBits = 0x0000_4000;
pub(crate) const CLOCAL: FlagBits = 0x0000_8000;
pub(crate) const CCTS_OFLOW: FlagBits = 0x0001_0000;
pub(crate) const CRTS_IFLOW: FlagBits = 0x0002_0000;
pub(crate) const MDMBUF: FlagBits = 0x0010_0000;

// `c_lflag` bits.
pub(crate) const ECHOKE: FlagBits = 0x0000_0001;
pub(crate) const ECHOE: FlagBits = 0x0000_0002;
pub(crate) const ECHOK: FlagBits = 0x0000_0004;
pub(crate) const ECHO: FlagBits = 0x0000_0008;
pub(crate) const ECHONL: FlagBits = 0x0000_0010;
pub(crate) const ECHOPRT: FlagBits = 0x0000_0020;
pub(crate) const ECHOCTL: FlagBits = 0x0000_0040;
pub(crate) const ISIG: FlagBits = 0x0000_0080;
pub(crate) const ICANON: FlagBits = 0x0000_0100;
pub(crate) const ALTWERASE: FlagBits = 0x0000_0200;
pub(crate) const IEXTEN: FlagBits = 0x0000_0400;
pub(crate) const EXTPROC: FlagBits = 0x0000_0800;
pub(crate) const TOSTOP: FlagBits = 0x0040_0000;
pub(crate) const FLUSHO: FlagBits = 0x0080_0000;
pub(crate) const NOKERNINFO: FlagBits = 0x0200_0000;
pub(crate) const PENDIN: FlagBits = 0x2000_0000;
pub(crate) const NOFLSH: FlagBits = 0x8000_0000;

// BSD `V*` subscripts.
pub(crate) const VEOF: usize = 0;
pub(crate) const VEOL: usize = 1;
pub(crate) const VEOL2: usize = 2;
pub(crate) const VERASE: usize = 3;
pub(crate) const VWERASE: usize = 4;
pub(crate) const VKILL: usize = 5;
pub(crate) const VREPRINT: usize = 6;
pub(crate) const VINTR: usize = 8;
pub(crate) const VQUIT: usize = 9;
pub(crate) const VSUSP: usize = 10;
pub(crate) const VDSUSP: usize = 11;
pub(crate) const VSTART: usize = 12;
pub(crate) const VSTOP: usize = 13;
pub(crate) const VLNEXT: usize = 14;
pub(crate) const VDISCARD: usize = 15;
pub(crate) const VMIN: usize = 16;
pub(crate) const VTIME: usize = 17;
pub(crate) const VSTATUS: usize = 18;

/// Every named Darwin control-character slot. Positions 7 and 19 are
/// reserved by `<sys/termios.h>` and remain `_POSIX_VDISABLE` in snapshots.
pub(crate) const CC_INDICES: [(usize, SpecialCodeIndex); 18] = [
    (VEOF, SpecialCodeIndex::VEOF),
    (VEOL, SpecialCodeIndex::VEOL),
    (VEOL2, SpecialCodeIndex::VEOL2),
    (VERASE, SpecialCodeIndex::VERASE),
    (VWERASE, SpecialCodeIndex::VWERASE),
    (VKILL, SpecialCodeIndex::VKILL),
    (VREPRINT, SpecialCodeIndex::VREPRINT),
    (VINTR, SpecialCodeIndex::VINTR),
    (VQUIT, SpecialCodeIndex::VQUIT),
    (VSUSP, SpecialCodeIndex::VSUSP),
    (VDSUSP, SpecialCodeIndex::VDSUSP),
    (VSTART, SpecialCodeIndex::VSTART),
    (VSTOP, SpecialCodeIndex::VSTOP),
    (VLNEXT, SpecialCodeIndex::VLNEXT),
    (VDISCARD, SpecialCodeIndex::VDISCARD),
    (VMIN, SpecialCodeIndex::VMIN),
    (VTIME, SpecialCodeIndex::VTIME),
    (VSTATUS, SpecialCodeIndex::VSTATUS),
];
