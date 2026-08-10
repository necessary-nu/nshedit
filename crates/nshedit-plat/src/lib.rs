//! The syscalls, in one crate.
//!
//! `plan/decisions/platform-layer.md` makes this the only place in the
//! workspace that issues one. The editor, terminal-capability discovery, and
//! C adapter consume its safe operations. Keeping the surface here rather
//! than in a `pub` module of the core is what keeps
//! `tcsetattr` and `sigaction` out of the namespace
//! `plan/decisions/idiomatic-core.md` makes a deliverable in its own right.
//!
//! # How the kernel is reached
//!
//! POSIX targets use `rustix` wherever `rustix` reaches, and the platform's
//! libc for the two families it declines. Other targets expose only the safe
//! facilities they implement; they do not receive placeholder POSIX layouts.
//!
//! rustix covers terminal attributes, window size, pending input, and process
//! credentials.
//! On x86_64 and aarch64 Linux it selects its `linux_raw` backend and issues
//! those syscalls directly. On macOS it uses rustix's libc backend, which is
//! the supported Darwin route for the same typed operations.
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
//! Linux/glibc and macOS/Darwin each select their own transcribed termios,
//! signal, and passwd representations. Windows has none of those POSIX
//! representations. There is no catch-all arm that lets one platform inherit
//! another platform's constants.

// [spec:nshedit:req:platform.typed-boundary]
#[cfg(unix)]
pub mod passwd;
#[cfg(unix)]
pub mod signal;
#[cfg(unix)]
pub mod terminal;
#[cfg(windows)]
#[path = "windows.rs"]
pub mod terminal;
#[cfg(unix)]
mod termios;

// ---------------------------------------------------------------------------
// Process credentials
// ---------------------------------------------------------------------------

/// A platform user identifier, kept distinct from group identifiers and raw
/// ABI integers.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(u32);

#[cfg(unix)]
impl UserId {
    /// Project the identifier for the private NSS call.
    #[must_use]
    pub(crate) const fn as_raw(self) -> u32 {
        self.0
    }
}

/// The real user that invoked this process.
#[must_use]
#[cfg(unix)]
pub fn current_user() -> UserId {
    UserId(rustix::process::getuid().as_raw())
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
#[cfg(unix)]
pub fn is_elevated() -> bool {
    rustix::process::getuid() != rustix::process::geteuid()
        || rustix::process::getgid() != rustix::process::getegid()
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
#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
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
    /// `#if` arms select between platforms rather than between values.
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

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod tests {
    use super::*;

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
        assert_eq!(current_user().as_raw(), uid);
        assert_eq!(is_elevated(), uid != euid || gid != egid);
    }
}
