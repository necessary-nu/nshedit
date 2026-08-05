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
/// Only `pw_name` and `pw_dir` are ever read; the rest hold their places so
/// the layout the libc writes into is the one declared here, which is what
/// the `dead_code` waiver below is for.
#[repr(C)]
#[allow(dead_code)]
struct Passwd {
    pw_name: *mut c_char,
    pw_passwd: *mut c_char,
    pw_uid: u32,
    pw_gid: u32,
    pw_gecos: *mut c_char,
    pw_dir: *mut c_char,
    pw_shell: *mut c_char,
}

impl Passwd {
    const ZEROED: Self = Self {
        pw_name: core::ptr::null_mut(),
        pw_passwd: core::ptr::null_mut(),
        pw_uid: 0,
        pw_gid: 0,
        pw_gecos: core::ptr::null_mut(),
        pw_dir: core::ptr::null_mut(),
        pw_shell: core::ptr::null_mut(),
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
