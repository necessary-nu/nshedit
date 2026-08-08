//! The user database, and the second of this workspace's two libc `extern`
//! blocks.
//!
//! `getpwnam_r`, `getpwuid_r` and the `setpwent`/`getpwent`/`endpwent`
//! cursor. There is no pure-Rust route: NSS backends are `dlopen`ed C shared
//! objects with a C ABI and nothing else, and speaking SSSD's socket or
//! systemd's `io.systemd.UserDatabase` varlink protocol directly would cover
//! two backends, miss `nss_ldap`, NIS and the rest, and amount to writing a
//! name-service client to avoid a function call.
//!
//! **Not `/etc/passwd`.** On a workstation joined to a directory — LDAP with
//! `nss_ldap`, SSSD, AD, `nss_systemd` for `systemd-homed`, or NIS — accounts
//! are not in that file, and the *invoking* user usually is not either, so a
//! parse breaks bare `~` and `~/…` for the person at the keyboard and not
//! merely `~alice`. `sem:filecomplete.fn-tilde-expand-fn` makes the failure
//! silent by specification, so the caller cannot tell. Two `/etc/passwd`
//! parsers existed in this port and `plan/decisions/platform-layer.md`
//! retires both outright rather than keeping one as a fallback: a parse
//! sitting behind `getpwnam_r` would disagree with the C in exactly the case
//! the rule pins, where any non-zero return — `ERANGE` included — must read
//! as *no such user*, and a hand parser has no 1024-byte limit to hit.
//!
//! The accepted cost is the one the rule already names: a lookup can block on
//! a network name service, inside a completion keystroke. The C has always
//! had that property. [`set_passwd_ops`] is there for a caller that must not
//! pay it.

use core::ffi::{CStr, c_char, c_int};
use core::sync::atomic::{AtomicPtr, Ordering};
use std::ffi::CString;

/// `sem:filecomplete.fn-tilde-expand-fn`'s fixed scratch buffer. The size is
/// load-bearing rather than a tuning choice: an entry too large for it comes
/// back `ERANGE`, which the rule requires be read as *no such user*, so a
/// bigger buffer would expand names the C does not.
const BUFLEN: usize = 1024;

/// POSIX `struct passwd`, transcribed.
///
/// Only `pw_name` and `pw_dir` are read; underscore-prefixed fields retain the
/// remaining libc layout without pretending they carry application state.
#[repr(C)]
struct Passwd {
    pw_name: *mut c_char,
    _password: *mut c_char,
    _uid: u32,
    _gid: u32,
    _gecos: *mut c_char,
    pw_dir: *mut c_char,
    _shell: *mut c_char,
}

impl Passwd {
    const ZEROED: Self = Self {
        pw_name: core::ptr::null_mut(),
        _password: core::ptr::null_mut(),
        _uid: 0,
        _gid: 0,
        _gecos: core::ptr::null_mut(),
        pw_dir: core::ptr::null_mut(),
        _shell: core::ptr::null_mut(),
    };
}

/// Checked rather than trusted: `sizeof(struct passwd)` is 48 on 64-bit
/// glibc, five pointers and two 32-bit ids with no padding of its own.
const _: () = {
    assert!(size_of::<Passwd>() == 5 * size_of::<usize>() + 2 * size_of::<u32>());
};

/// The one libc `extern` block this module has.
mod sys {
    use core::ffi::{c_char, c_int};

    use super::Passwd;

    unsafe extern "C" {
        pub(super) fn getpwnam_r(
            name: *const c_char,
            pwd: *mut Passwd,
            buf: *mut c_char,
            buflen: usize,
            result: *mut *mut Passwd,
        ) -> c_int;
        pub(super) fn getpwuid_r(
            uid: u32,
            pwd: *mut Passwd,
            buf: *mut c_char,
            buflen: usize,
            result: *mut *mut Passwd,
        ) -> c_int;
        pub(super) fn setpwent();
        pub(super) fn getpwent() -> *mut Passwd;
        pub(super) fn endpwent();
    }
}

/// The passwd family, as one replaceable table.
///
/// Nothing installs one and nothing has to. The slot is here for an embedder
/// that must answer these from a cache rather than let a blocking NSS call
/// happen inside a keystroke.
pub struct PasswdOps {
    pub home_dir_by_name: fn(&str) -> Option<Vec<u8>>,
    pub home_dir_by_uid: fn(u32) -> Option<Vec<u8>>,
    pub setpwent: fn(),
    pub getpwent_name: fn() -> Option<Vec<u8>>,
    pub endpwent: fn(),
}

const BUILTIN_OPS: PasswdOps = PasswdOps {
    home_dir_by_name: home_dir_by_name_default,
    home_dir_by_uid: home_dir_by_uid_default,
    setpwent: setpwent_default,
    getpwent_name: getpwent_name_default,
    endpwent: endpwent_default,
};

static OPS: AtomicPtr<PasswdOps> = AtomicPtr::new(core::ptr::null_mut());

/// Install an override for the whole family. Idempotent, and there is no way
/// to take one back off.
pub fn set_passwd_ops(ops: &'static PasswdOps) {
    OPS.store(core::ptr::from_ref(ops).cast_mut(), Ordering::Release);
}

fn ops() -> &'static PasswdOps {
    let p = OPS.load(Ordering::Acquire);
    if p.is_null() {
        return &BUILTIN_OPS;
    }
    // SAFETY: `set_passwd_ops` is the only writer and takes a `&'static`.
    unsafe { &*p }
}

/// `getpwnam_r(name, &pwres, buf, sizeof buf, &result)->pw_dir`.
///
/// `None` is `sem:filecomplete.fn-tilde-expand-fn`'s *no such user*, which
/// deliberately lumps together a genuine absence, a zero return with a NULL
/// result, and an entry too large for [`BUFLEN`].
#[must_use]
pub fn home_dir_by_name(name: &str) -> Option<Vec<u8>> {
    (ops().home_dir_by_name)(name)
}

/// `getpwuid_r(uid, &pwres, buf, sizeof buf, &result)->pw_dir`. Same
/// conflation as [`home_dir_by_name`].
#[must_use]
pub fn home_dir_by_uid(uid: u32) -> Option<Vec<u8>> {
    (ops().home_dir_by_uid)(uid)
}

/// `setpwent()` — rewind the enumeration cursor.
pub fn setpwent() {
    (ops().setpwent)();
}

/// `getpwent()->pw_name`, or `None` at the end of the database.
///
/// The name is copied out before returning, because `getpwent` hands back a
/// pointer into storage the next call overwrites.
#[must_use]
pub fn getpwent_name() -> Option<Vec<u8>> {
    (ops().getpwent_name)()
}

/// `endpwent()` — close the enumeration.
pub fn endpwent() {
    (ops().endpwent)();
}

fn home_dir_by_name_default(name: &str) -> Option<Vec<u8>> {
    // An interior NUL cannot reach here from the C's `char *`, which stops at
    // the first one; a Rust caller that manages it gets *no such user*, which
    // is what the C's truncated lookup would most likely answer too.
    let cname = CString::new(name).ok()?;
    let mut pwres = Passwd::ZEROED;
    let mut buf: [c_char; BUFLEN] = [0; BUFLEN];
    let mut result: *mut Passwd = core::ptr::null_mut();
    // SAFETY: `pwres`, `buf` and `result` are live, exclusively borrowed and
    // correctly sized for the call; `cname` is NUL-terminated and outlives
    // it. On success `result` aliases `pwres` and the strings point into
    // `buf`, both of which outlive the copy below.
    let rv = unsafe {
        sys::getpwnam_r(
            cname.as_ptr(),
            &raw mut pwres,
            buf.as_mut_ptr(),
            BUFLEN,
            &raw mut result,
        )
    };
    pw_dir(rv, result)
}

fn home_dir_by_uid_default(uid: u32) -> Option<Vec<u8>> {
    let mut pwres = Passwd::ZEROED;
    let mut buf: [c_char; BUFLEN] = [0; BUFLEN];
    let mut result: *mut Passwd = core::ptr::null_mut();
    // SAFETY: as `home_dir_by_name_default`.
    let rv = unsafe {
        sys::getpwuid_r(
            uid,
            &raw mut pwres,
            buf.as_mut_ptr(),
            BUFLEN,
            &raw mut result,
        )
    };
    pw_dir(rv, result)
}

/// The rule's "treat ANY non-zero return as no such user", plus the zero
/// return with a NULL result it also names.
fn pw_dir(rv: c_int, result: *mut Passwd) -> Option<Vec<u8>> {
    if rv != 0 || result.is_null() {
        return None;
    }
    // SAFETY: a zero return with a non-NULL result means the libc filled
    // `*result`, whose `pw_dir` is a NUL-terminated string in the scratch
    // buffer, live until that buffer is reused.
    let dir = unsafe { (*result).pw_dir };
    if dir.is_null() {
        return None;
    }
    // SAFETY: as above.
    Some(unsafe { CStr::from_ptr(dir) }.to_bytes().to_vec())
}

fn setpwent_default() {
    // SAFETY: no arguments to get wrong. The cursor is process-global, which
    // is what `readline.c`'s use of it already assumes.
    unsafe { sys::setpwent() };
}

fn getpwent_name_default() -> Option<Vec<u8>> {
    // SAFETY: as above.
    let p = unsafe { sys::getpwent() };
    if p.is_null() {
        return None;
    }
    // SAFETY: a non-NULL return points at static storage valid until the next
    // `getpwent`/`endpwent`, and `pw_name` is a NUL-terminated string in it.
    let name = unsafe { (*p).pw_name };
    if name.is_null() {
        return None;
    }
    // SAFETY: as above.
    Some(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec())
}

fn endpwent_default() {
    // SAFETY: no arguments to get wrong.
    unsafe { sys::endpwent() };
}

#[cfg(test)]
mod tests {
    use core::mem::offset_of;

    use super::*;
    use crate::cheader;

    /// The field offsets of `struct passwd`, which no header states and only a
    /// compiler can answer.
    ///
    /// Produced by gcc 15 on x86_64 glibc and carried here rather than probed,
    /// because a `build.rs` that compiles a program to find out what the
    /// platform looks like is what `plan/decisions/no-c-ffi.md` forbids:
    ///
    /// ```c
    /// printf("%zu %zu %zu %zu %zu %zu %zu %zu\n", sizeof(struct passwd),
    ///        offsetof(struct passwd, pw_name), offsetof(struct passwd, pw_passwd),
    ///        offsetof(struct passwd, pw_uid),  offsetof(struct passwd, pw_gid),
    ///        offsetof(struct passwd, pw_gecos), offsetof(struct passwd, pw_dir),
    ///        offsetof(struct passwd, pw_shell));
    /// // 48 0 8 16 20 24 32 40
    /// ```
    ///
    /// The size assertion beside the type cannot catch a reorder, and only two
    /// of these fields are ever read — so a `pw_dir` at the wrong offset would
    /// hand back `pw_shell`, which is also an absolute path and would look
    /// entirely plausible in a completion.
    #[test]
    fn the_struct_passwd_layout_is_the_one_gcc_lays_out() {
        assert_eq!(size_of::<Passwd>(), 48);
        assert_eq!(offset_of!(Passwd, pw_name), 0);
        assert_eq!(offset_of!(Passwd, _password), 8);
        assert_eq!(offset_of!(Passwd, _uid), 16);
        assert_eq!(offset_of!(Passwd, _gid), 20);
        assert_eq!(offset_of!(Passwd, _gecos), 24);
        assert_eq!(offset_of!(Passwd, pw_dir), 32);
        assert_eq!(offset_of!(Passwd, _shell), 40);
    }

    /// `getpwnam_r` distinguishes "no such user" from "your buffer was too
    /// small"; `sem:filecomplete.fn-tilde-expand-fn` requires the port not to.
    /// Any non-zero return is *no such user*, `ERANGE` included, and so is a
    /// zero return with a NULL result.
    ///
    /// The conflation is not a shrug: a bigger buffer, or an `ERANGE` retry,
    /// would expand names the C leaves alone, which is a behavioural
    /// divergence in a completion the caller cannot see. Driven through
    /// [`pw_dir`] directly because an entry too large for [`BUFLEN`] cannot be
    /// conjured out of the real database.
    ///
    /// `ERANGE` is read from the kernel's `asm-generic/errno-base.h` rather
    /// than written here, since the whole point is that the number is the
    /// libc's and not ours.
    // [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test]
    #[test]
    fn any_non_zero_return_reads_as_no_such_user() {
        let erange = c_int::try_from(cheader::defines(&["asm-generic/errno-base.h"])["ERANGE"])
            .expect("ERANGE");

        let home = CString::new("/home/example").expect("a path");
        let mut found = Passwd::ZEROED;
        found.pw_dir = home.as_ptr().cast_mut();
        let result: *mut Passwd = &raw mut found;

        // The control: this exact entry, returned successfully, does expand.
        assert_eq!(
            pw_dir(0, result).as_deref(),
            Some(b"/home/example".as_slice())
        );

        // And the same entry, reported through any failing return, does not —
        // even though everything needed to expand it is right there.
        assert_eq!(pw_dir(erange, result), None, "ERANGE");
        assert_eq!(pw_dir(-1, result), None, "a negative return");
        assert_eq!(pw_dir(2, result), None, "ENOENT");

        // The zero-return-with-NULL-result case the rule also names, which is
        // how glibc reports a genuine absence.
        assert_eq!(pw_dir(0, core::ptr::null_mut()), None);

        // An entry with no home directory at all is not an empty expansion.
        let mut homeless = Passwd::ZEROED;
        assert_eq!(pw_dir(0, &raw mut homeless), None);
    }

    /// A name that cannot be in any database is *no such user* rather than a
    /// panic or an empty expansion — including one carrying an interior NUL,
    /// which cannot reach here from the C's `char *` but can from a Rust
    /// caller.
    #[test]
    fn a_name_no_database_can_hold_is_no_such_user() {
        assert_eq!(home_dir_by_name("no-such-user-4b8f2c1e"), None);
        assert_eq!(home_dir_by_name("has\0an interior nul"), None);
        assert_eq!(home_dir_by_name(""), None);
        // The uid space's top value is reserved as "nobody/invalid" by
        // convention and is not allocated to a real account.
        assert_eq!(home_dir_by_uid(u32::MAX), None);
    }

    /// The layout, against the live name service rather than against a
    /// compiler: every name the enumeration hands out has to resolve by name,
    /// which it can only do if `pw_name` and `pw_dir` are being read from the
    /// offsets the libc wrote them to.
    ///
    /// Vacuous where the database is empty — a container with no `/etc/passwd`
    /// and no NSS backend — and that is the honest outcome there, since the
    /// C's own `~` expansion would answer nothing too.
    #[test]
    fn every_name_the_enumeration_hands_out_resolves_by_name() {
        setpwent();
        // Bounded: `getpwent` walks every backend, and a directory service can
        // hold a great many accounts.
        let names: Vec<Vec<u8>> = core::iter::from_fn(getpwent_name).take(64).collect();
        endpwent();

        let mut absolute = 0;
        for raw in &names {
            let Ok(name) = core::str::from_utf8(raw) else {
                continue;
            };
            assert!(!name.is_empty(), "the enumeration yielded an empty name");
            let home = home_dir_by_name(name)
                .unwrap_or_else(|| panic!("{name} was enumerated but does not resolve"));
            if home.first() == Some(&b'/') {
                absolute += 1;
            }
        }
        assert!(
            names.is_empty() || absolute > 0,
            "no account in the database has an absolute home directory"
        );
    }
}
