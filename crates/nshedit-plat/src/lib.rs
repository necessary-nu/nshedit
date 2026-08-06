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
/// `read__fixio`'s two sub-blocks are one condition there.
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
