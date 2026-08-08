//! The syscalls, in one crate.
//!
//! `plan/decisions/platform-layer.md` makes this the only place in the
//! workspace that issues one. Both `nshedit` — the core, which is what nsh
//! links — and `nshedit-abi` depend on it; nothing else does. Keeping the
//! surface here rather than in a `pub` module of the core is what keeps
//! `tcsetattr` and `sigaction` out of the namespace
//! `plan/decisions/idiomatic-core.md` makes a deliverable in its own right.
//!
//! # How the kernel is reached
//!
//! `rustix` wherever `rustix` reaches, and the platform's libc for the two
//! families it declines.
//!
//! rustix covers `tcgetattr`, `tcsetattr`, `tcgetwinsize` (`TIOCGWINSZ`),
//! `fcntl` with `F_GETFL`/`F_SETFL`, `isatty` and the four uid/gid queries.
//! On x86_64 and aarch64 Linux it selects its `linux_raw` backend and issues
//! those syscalls directly, so that part of this crate goes nowhere near a C
//! library.
//!
//! It does not cover signals, and not by omission: rustix's
//! `not_implemented.rs` lists `sigaction`, `sigprocmask` and `sigwait` as
//! deliberately out of scope because a libc expects to be involved, and its
//! `runtime` module's replacements are documented as undefined behaviour in
//! a process that has one. `nshedit-abi` builds a `cdylib` loaded into
//! exactly such processes and nsh links `std`, so there is no consumer for
//! whom they are defined. NSS backends are `dlopen`ed C objects with no
//! pure-Rust route at all. So the **signal family** ([`signal`]) and the
//! **passwd family** ([`passwd`]) are libc symbols, declared here under the
//! second site on `plan/decisions/no-c-ffi.md`'s enumeration.
//!
//! The whole libc surface of this workspace is therefore two `extern`
//! blocks, one in each of those two modules, and a reader can count them.
//! The core crate names no libc symbol and no `build.rs` anywhere hunts for
//! a library.
//!
//! # Scope of the numbers
//!
//! Linux-shaped, following `plan/decisions/posix-only-scope.md`: the termios
//! ABI, the `V*` subscripts, `_POSIX_VDISABLE`, the signal numbers and the
//! `struct sigaction`/`sigset_t`/`struct passwd` layouts are all transcribed
//! for Linux/glibc. rustix supplies the ones it can portably; the rest stay
//! transcribed. A non-Linux build is not supported — rustix would fall back
//! to its libc backend and the transcribed numbers would be wrong anyway.

pub mod passwd;
pub mod signal;
pub mod termios;

use std::os::fd::BorrowedFd;

/// Borrow a raw descriptor for the duration of one call.
///
/// `None` for a negative descriptor, which the C's own calls would answer
/// `EBADF` for and which every caller here already treats as failure. The
/// port hands out -1 for a stream with no descriptor, so this is a live
/// path rather than a defensive one.
///
/// # Safety
///
/// The descriptor must be open for the duration of the call. Every caller in
/// the workspace passes one the application owns and libedit never closes.
fn borrow(fd: i32) -> Option<BorrowedFd<'static>> {
    if fd < 0 {
        return None;
    }
    // SAFETY: as documented above — the descriptor is the application's and
    // outlives the call; `BorrowedFd` neither owns nor closes it.
    Some(unsafe { BorrowedFd::borrow_raw(fd) })
}

// ---------------------------------------------------------------------------
// Descriptor flags
// ---------------------------------------------------------------------------

/// `O_NDELAY`. Linux gives it the same value as `O_NONBLOCK`, which is why
/// `read_fixio`'s two sub-blocks are one condition there.
pub const O_NDELAY: i32 = rustix::fs::OFlags::NONBLOCK.bits() as i32;

/// `fcntl(fd, F_GETFL, 0)`. `None` is the C's -1.
#[must_use]
pub fn fcntl_getfl(fd: i32) -> Option<i32> {
    let fd = borrow(fd)?;
    rustix::fs::fcntl_getfl(fd).ok().map(|f| f.bits() as i32)
}

/// `fcntl(fd, F_SETFL, flags)`. `false` is the C's -1.
///
/// `sem:read.read-fixio-fn`'s headline side effect goes through here: the
/// recovery **permanently** clears `O_NONBLOCK`/`O_NDELAY` on the caller's
/// input descriptor — normally the process's shared standard input — saving
/// and restoring nothing (ERR-input-21). Nothing in this crate compensates
/// for that; reproducing it is the point.
#[must_use]
pub fn fcntl_setfl(fd: i32, flags: i32) -> bool {
    let Some(fd) = borrow(fd) else {
        return false;
    };
    let flags = rustix::fs::OFlags::from_bits_retain(flags as u32);
    rustix::fs::fcntl_setfl(fd, flags).is_ok()
}

// ---------------------------------------------------------------------------
// Pending input
// ---------------------------------------------------------------------------

/// Number of bytes a read can consume without blocking (`FIONREAD`).
///
/// `None` is the ioctl's `-1`. A zero count is a successful observation and
/// is deliberately distinct: readline's event-hook reader busy-spins and
/// calls the application hook again when no byte is ready.
#[must_use]
pub fn bytes_ready_to_read(fd: i32) -> Option<u64> {
    let fd = borrow(fd)?;
    rustix::io::ioctl_fionread(fd).ok()
}

// ---------------------------------------------------------------------------
// Process credentials
// ---------------------------------------------------------------------------

/// `getuid()` — the real user id.
#[must_use]
pub fn getuid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// `geteuid()` — the effective user id.
#[must_use]
pub fn geteuid() -> u32 {
    rustix::process::geteuid().as_raw()
}

/// `getgid()` — the real group id.
#[must_use]
pub fn getgid() -> u32 {
    rustix::process::getgid().as_raw()
}

/// `getegid()` — the effective group id.
#[must_use]
pub fn getegid() -> u32 {
    rustix::process::getegid().as_raw()
}

/// Whether this process is running with privileges its invoker did not have —
/// set-uid or set-gid — and must therefore not trust the environment it was
/// handed.
///
/// This is ncurses' `is_elevated()`, from `ncurses/tinfo/access.c:190`, in the
/// form it falls back to when `issetugid(2)` and `getauxval(AT_SECURE)` are
/// both unavailable:
///
/// ```c
/// #define is_posix_elevated() \
///         (getuid() != geteuid() \
///          || getgid() != getegid())
/// ```
///
/// The POSIX form and not the other two because rustix exposes neither:
/// `issetugid` is BSD and macOS, and `AT_SECURE` needs `getauxval`, which
/// `dec:libedit:no-c-ffi` gives no route to. The difference matters in one
/// case and it is worth naming: `AT_SECURE` is also set when a program gains
/// capabilities or crosses a `nosuid` boundary without any uid changing, so a
/// process elevated *only* that way reads as unelevated here. That is a
/// narrower guard than glibc's `secure_getenv`, not a wrong one.
///
/// What this deliberately does NOT test is whether the process is root.
/// ncurses guards that separately, behind `--disable-root-environ`, and
/// Debian does not pass it — `debian/rules:137` passes
/// `--disable-setuid-environ` and nothing about root. So a root shell on a
/// deployed system does honour `TERMINFO`, and matching what is actually
/// installed beats matching a build nobody ships.
#[must_use]
pub fn is_elevated() -> bool {
    getuid() != geteuid() || getgid() != getegid()
}

/// Reading the C's own headers, so that a number transcribed into this crate
/// is checked against the authority it came from.
///
/// Every constant in this crate has one — the kernel's `asm-generic` headers,
/// glibc's `bits/*.h`, `<sys/ttydefaults.h>` — and the failure mode here is a
/// value typed wrong once. A test that restates the value the implementation
/// already holds would then be typed wrong twice and pass, so the expectations
/// are read out of the headers at test time instead.
///
/// No compiler is involved. A `#define` is text, and reading it is not the
/// library hunt `plan/decisions/no-c-ffi.md` forbids a `build.rs` from doing.
#[cfg(test)]
pub(crate) mod cheader {
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Every `#define NAME <constant>` in the named headers, resolved to a
    /// number.
    pub(crate) fn defines(relative: &[&str]) -> HashMap<String, i64> {
        Defines::new().read(relative).resolve()
    }

    /// A set of headers being read together, so that one can define a name in
    /// terms of another's.
    ///
    /// Preprocessor conditionals are ignored and the last definition of a name
    /// wins — both within a file and across the ones read into the same set,
    /// in the order they were read. That is safe for the headers here, whose
    /// `#if` arms select between platforms rather than between values, and it
    /// gives a system header the last word over the port's own `#ifndef`
    /// fallbacks, which is the order the preprocessor sees them in too.
    #[derive(Default)]
    pub(crate) struct Defines {
        raw: HashMap<String, String>,
    }

    impl Defines {
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// Header paths relative to an include root, such as
        /// `bits/termios-c_cc.h` or `asm-generic/fcntl.h`.
        pub(crate) fn read(mut self, relative: &[&str]) -> Self {
            for rel in relative {
                let path = find(rel);
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                self.absorb(&text);
            }
            self
        }

        /// A header that is not the system's — the C original in this tree,
        /// whose `#ifndef` fallbacks are the authority for the names no
        /// platform header defines.
        pub(crate) fn read_text(mut self, text: &str) -> Self {
            self.absorb(text);
            self
        }

        /// The names the parser can evaluate. Function-like macros and
        /// expressions such as `TTYDEF_IFLAG` are simply absent.
        pub(crate) fn resolve(&self) -> HashMap<String, i64> {
            self.raw
                .iter()
                .filter_map(|(name, body)| Some((name.clone(), eval(body, &self.raw, 0)?)))
                .collect()
        }

        fn absorb(&mut self, text: &str) {
            for line in text.lines() {
                if let Some((name, body)) = define_on(line) {
                    self.raw.insert(name, body);
                }
            }
        }
    }

    /// Where a header lives. The multiarch directory first, because on Debian
    /// that is the only place glibc's `bits/` exists; plain `/usr/include`
    /// next, which is where the distributions that do not split it keep both
    /// `bits/` and the kernel's `asm-generic/`.
    fn find(relative: &str) -> PathBuf {
        let arch = std::env::consts::ARCH;
        [
            format!("/usr/include/{arch}-linux-gnu"),
            "/usr/include".to_owned(),
            format!("/usr/include/{arch}-linux-musl"),
        ]
        .into_iter()
        .map(|root| PathBuf::from(root).join(relative))
        .find(|p| p.is_file())
        .unwrap_or_else(|| {
            panic!(
                "{relative} is not installed. These tests check this crate's \
                 constants against the C library's own headers, so the libc \
                 development package is required to run them."
            )
        })
    }

    /// `(name, body)` for an object-like `#define`, or `None` for anything
    /// else on the line.
    fn define_on(line: &str) -> Option<(String, String)> {
        let s = line.trim_start().strip_prefix('#')?.trim_start();
        let s = s.strip_prefix("define")?;
        if !s.starts_with(char::is_whitespace) {
            return None;
        }
        let s = s.trim_start();
        let end = s
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(s.len());
        let (name, rest) = s.split_at(end);
        // A `(` immediately after the name makes it function-like — `CTRL(x)`
        // — which has no value of its own.
        if name.is_empty() || rest.starts_with('(') {
            return None;
        }
        Some((name.to_owned(), uncomment(rest).trim().to_owned()))
    }

    fn uncomment(body: &str) -> &str {
        let end = [body.find("/*"), body.find("//")]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(body.len());
        &body[..end]
    }

    /// The four shapes these headers actually use: an integer literal, a
    /// character literal, `CTRL('c')`, and a bare alias for another name.
    fn eval(body: &str, raw: &HashMap<String, String>, depth: u32) -> Option<i64> {
        if depth > 8 {
            return None;
        }
        let t = body.trim();
        for macro_name in ["CTRL", "CONTROL"] {
            if let Some(arg) = t
                .strip_prefix(macro_name)
                .and_then(|r| r.trim_start().strip_prefix('('))
                .and_then(|r| r.strip_suffix(')'))
            {
                return char_literal(arg.trim()).map(|c| c & 0o37);
            }
        }
        char_literal(t)
            .or_else(|| integer(t))
            .or_else(|| raw.get(t).and_then(|aliased| eval(aliased, raw, depth + 1)))
    }

    fn char_literal(t: &str) -> Option<i64> {
        let inner = t.strip_prefix('\'')?.strip_suffix('\'')?;
        match inner {
            "\\0" => Some(0),
            _ if inner.chars().count() == 1 => Some(inner.chars().next()? as i64),
            _ => None,
        }
    }

    /// C integer literals: hex, octal by leading zero, decimal, with the
    /// `U`/`L` suffixes stripped.
    fn integer(t: &str) -> Option<i64> {
        let t = t.trim_end_matches(['u', 'U', 'l', 'L']);
        if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            i64::from_str_radix(hex, 16).ok()
        } else if t.len() > 1 && t.starts_with('0') {
            i64::from_str_radix(&t[1..], 8).ok()
        } else {
            t.parse().ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `read_fixio` recovery treats `O_NDELAY` and `O_NONBLOCK` as one
    /// condition, which is only sound because Linux gives them the same value.
    /// Checked against `asm-generic/fcntl.h`, where `O_NDELAY` is literally
    /// `#define`d to `O_NONBLOCK`.
    #[test]
    fn o_ndelay_is_the_kernels_o_nonblock() {
        let h = cheader::defines(&["asm-generic/fcntl.h"]);
        assert_eq!(h["O_NDELAY"], i64::from(O_NDELAY));
        assert_eq!(h["O_NONBLOCK"], i64::from(O_NDELAY));
    }

    /// The port hands out -1 for a stream with no descriptor, so every
    /// descriptor call has to answer the C's failure rather than reach a
    /// syscall with a bad argument.
    #[test]
    fn a_negative_descriptor_fails_the_way_the_c_does() {
        assert!(fcntl_getfl(-1).is_none());
        assert!(!fcntl_setfl(-1, 0));
        assert!(bytes_ready_to_read(-1).is_none());
    }

    /// The safe FIONREAD seam distinguishes an empty descriptor from one with
    /// pending bytes and tracks consumption without taking ownership.
    #[test]
    fn pending_input_reports_the_kernel_queue() {
        use std::io::{Read, Write};
        use std::os::fd::AsRawFd;
        use std::os::unix::net::UnixStream;

        let (mut reader, mut writer) = UnixStream::pair().expect("socket pair");
        assert_eq!(bytes_ready_to_read(reader.as_raw_fd()), Some(0));

        writer.write_all(b"abc").expect("write test bytes");
        assert_eq!(bytes_ready_to_read(reader.as_raw_fd()), Some(3));

        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).expect("read one byte");
        assert_eq!(byte, [b'a']);
        assert_eq!(bytes_ready_to_read(reader.as_raw_fd()), Some(2));
    }

    /// `sem:read.read-fixio-fn`'s side effect is a permanent clear of
    /// `O_NONBLOCK` on the caller's descriptor, so the bit has to survive the
    /// round trip through `F_GETFL`/`F_SETFL` unchanged — and the flag word
    /// read back has to be the one the kernel holds, not a re-encoding of it.
    ///
    /// `/dev/null` rather than the process's own standard input, which a test
    /// runner may have pointed anywhere and which nothing here should modify.
    #[test]
    fn the_nonblocking_bit_survives_a_get_set_round_trip() {
        let f = std::fs::File::open("/dev/null").expect("/dev/null");
        let fd = std::os::fd::AsRawFd::as_raw_fd(&f);

        let before = fcntl_getfl(fd).expect("F_GETFL");
        assert_eq!(before & O_NDELAY, 0, "a fresh open is not non-blocking");

        assert!(fcntl_setfl(fd, before | O_NDELAY));
        assert_eq!(fcntl_getfl(fd).expect("F_GETFL"), before | O_NDELAY);

        assert!(fcntl_setfl(fd, before));
        assert_eq!(fcntl_getfl(fd).expect("F_GETFL"), before);
    }

    /// The four credential queries and the elevation test they feed, against
    /// the kernel's own report of this process rather than against rustix.
    ///
    /// `/proc/self/status` spells `Uid:` and `Gid:` as real, effective, saved,
    /// filesystem — so the first two fields are exactly what `getuid` and
    /// `geteuid` are supposed to return.
    #[test]
    fn the_credentials_are_what_the_kernel_reports() {
        let status = std::fs::read_to_string("/proc/self/status").expect("/proc/self/status");
        let field = |key: &str| -> (u32, u32) {
            let line = status
                .lines()
                .find(|l| l.starts_with(key))
                .unwrap_or_else(|| panic!("no {key} line in /proc/self/status"));
            let mut ids = line
                .split_whitespace()
                .skip(1)
                .map(|w| w.parse::<u32>().expect("a numeric id"));
            (ids.next().expect("real"), ids.next().expect("effective"))
        };

        let (uid, euid) = field("Uid:");
        let (gid, egid) = field("Gid:");
        assert_eq!((getuid(), geteuid()), (uid, euid));
        assert_eq!((getgid(), getegid()), (gid, egid));
        assert_eq!(is_elevated(), uid != euid || gid != egid);
    }
}
