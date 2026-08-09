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
//! had that property. Host-provided lookup policy belongs to an editor
//! session, not a process-global replacement table in this syscall layer.

use core::ffi::{CStr, c_char, c_int};
use std::ffi::{CString, OsString};
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::UserId;

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
compile_error!("nshedit-plat has no struct passwd transcription for this target");

static USER_DATABASE: Mutex<()> = Mutex::new(());

/// `sem:filecomplete.fn-tilde-expand-fn`'s fixed scratch buffer. The size is
/// load-bearing rather than a tuning choice: an entry too large for it comes
/// back `ERANGE`, which the rule requires be read as *no such user*, so a
/// bigger buffer would expand names the C does not.
const BUFLEN: usize = 1024;

/// The host's `<pwd.h>` `struct passwd`, transcribed.
///
/// Only `pw_name` and `pw_dir` are read; underscore-prefixed fields retain the
/// remaining libc layout without pretending they carry application state.
#[repr(C)]
struct Passwd {
    pw_name: *mut c_char,
    _password: *mut c_char,
    _uid: u32,
    _gid: u32,
    #[cfg(target_os = "macos")]
    _change: i64,
    #[cfg(target_os = "macos")]
    _class: *mut c_char,
    _gecos: *mut c_char,
    pw_dir: *mut c_char,
    _shell: *mut c_char,
    #[cfg(target_os = "macos")]
    _expire: i64,
}

impl Passwd {
    const ZEROED: Self = Self {
        pw_name: core::ptr::null_mut(),
        _password: core::ptr::null_mut(),
        _uid: 0,
        _gid: 0,
        #[cfg(target_os = "macos")]
        _change: 0,
        #[cfg(target_os = "macos")]
        _class: core::ptr::null_mut(),
        _gecos: core::ptr::null_mut(),
        pw_dir: core::ptr::null_mut(),
        _shell: core::ptr::null_mut(),
        #[cfg(target_os = "macos")]
        _expire: 0,
    };
}

/// Checked rather than trusted against glibc's `<pwd.h>` layout.
#[cfg(any(target_os = "linux", target_os = "android"))]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};

    assert!(size_of::<Passwd>() == 48);
    assert!(align_of::<Passwd>() == 8);
    assert!(offset_of!(Passwd, pw_name) == 0);
    assert!(offset_of!(Passwd, _password) == 8);
    assert!(offset_of!(Passwd, _uid) == 16);
    assert!(offset_of!(Passwd, _gid) == 20);
    assert!(offset_of!(Passwd, _gecos) == 24);
    assert!(offset_of!(Passwd, pw_dir) == 32);
    assert!(offset_of!(Passwd, _shell) == 40);
};

/// Darwin's published `<pwd.h>` layout. Unlike glibc it carries account
/// change/expiry times and the login class between the ids and GECOS.
// [spec:nshedit:req:platform.per-os-layouts]
#[cfg(target_os = "macos")]
const _: () = {
    use core::mem::{align_of, offset_of, size_of};

    assert!(size_of::<Passwd>() == 72);
    assert!(align_of::<Passwd>() == 8);
    assert!(offset_of!(Passwd, pw_name) == 0);
    assert!(offset_of!(Passwd, _password) == 8);
    assert!(offset_of!(Passwd, _uid) == 16);
    assert!(offset_of!(Passwd, _gid) == 20);
    assert!(offset_of!(Passwd, _change) == 24);
    assert!(offset_of!(Passwd, _class) == 32);
    assert!(offset_of!(Passwd, _gecos) == 40);
    assert!(offset_of!(Passwd, pw_dir) == 48);
    assert!(offset_of!(Passwd, _shell) == 56);
    assert!(offset_of!(Passwd, _expire) == 64);
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

/// A scoped walk over user names known to the platform's configured name
/// service.
///
/// Dropping the value closes the libc cursor. The value is thread-bound
/// because the underlying enumeration cursor is process-global.
#[must_use = "dropping the scan closes the user database cursor"]
pub struct UserNames {
    exhausted: bool,
    _exclusive: MutexGuard<'static, ()>,
}

impl UserNames {
    /// Rewind and open the platform user database.
    pub fn open() -> Self {
        let exclusive = USER_DATABASE.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: no arguments. The returned owner serializes use of this
        // cursor within the editor session and closes it on drop.
        unsafe { sys::setpwent() };
        Self {
            exhausted: false,
            _exclusive: exclusive,
        }
    }

    /// Rewind this scan to the first user.
    pub fn rewind(&mut self) {
        // SAFETY: no arguments; this value owns the active scan.
        unsafe { sys::setpwent() };
        self.exhausted = false;
    }
}

impl Iterator for UserNames {
    type Item = OsString;

    fn next(&mut self) -> Option<Self::Item> {
        if self.exhausted {
            return None;
        }
        let name = next_user_name();
        if name.is_none() {
            self.exhausted = true;
        }
        name
    }
}

impl Drop for UserNames {
    fn drop(&mut self) {
        // SAFETY: no arguments; closes the cursor opened by this owner.
        unsafe { sys::endpwent() };
    }
}

/// Look up a named user's home directory.
///
/// A successful `None` means that the user does not exist. Platform failures
/// remain distinct so native callers can report or recover from them; the C
/// adapter deliberately flattens them where compatibility requires it.
pub fn home_directory_named(name: &str) -> io::Result<Option<PathBuf>> {
    // An interior NUL cannot reach here from the C's `char *`, which stops at
    // the first one; a Rust caller that manages it gets *no such user*, which
    // is what the C's truncated lookup would most likely answer too.
    let cname = CString::new(name).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "user name contains a NUL byte")
    })?;
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

/// Look up a user's home directory by typed identifier.
pub fn home_directory(user: UserId) -> io::Result<Option<PathBuf>> {
    let mut pwres = Passwd::ZEROED;
    let mut buf: [c_char; BUFLEN] = [0; BUFLEN];
    let mut result: *mut Passwd = core::ptr::null_mut();
    // SAFETY: as `home_dir_by_name_default`.
    let rv = unsafe {
        sys::getpwuid_r(
            user.as_raw(),
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
fn pw_dir(rv: c_int, result: *mut Passwd) -> io::Result<Option<PathBuf>> {
    if rv != 0 {
        return Err(io::Error::from_raw_os_error(rv));
    }
    if result.is_null() {
        return Ok(None);
    }
    // SAFETY: a zero return with a non-NULL result means the libc filled
    // `*result`, whose `pw_dir` is a NUL-terminated string in the scratch
    // buffer, live until that buffer is reused.
    let dir = unsafe { (*result).pw_dir };
    if dir.is_null() {
        return Ok(None);
    }
    // SAFETY: as above.
    let bytes = unsafe { CStr::from_ptr(dir) }.to_bytes().to_vec();
    Ok(Some(PathBuf::from(OsString::from_vec(bytes))))
}

fn next_user_name() -> Option<OsString> {
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
    Some(OsString::from_vec(
        unsafe { CStr::from_ptr(name) }.to_bytes().to_vec(),
    ))
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    use core::mem::offset_of;
    use std::os::unix::ffi::OsStrExt;

    use super::*;
    #[cfg(any(target_os = "linux", target_os = "android"))]
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
    #[cfg(any(target_os = "linux", target_os = "android"))]
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

    /// `getpwnam_r` distinguishes "no such user" from a platform failure.
    /// The platform layer preserves that distinction even though the C
    /// completion adapter later flattens both outcomes for compatibility.
    ///
    /// `ERANGE` is read from the kernel's `asm-generic/errno-base.h` rather
    /// than written here, since the whole point is that the number is the
    /// libc's and not ours.
    #[test]
    fn nonzero_results_remain_platform_errors() {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let erange = c_int::try_from(cheader::defines(&["asm-generic/errno-base.h"])["ERANGE"])
            .expect("ERANGE");

        let home = CString::new("/home/example").expect("a path");
        let mut found = Passwd::ZEROED;
        found.pw_dir = home.as_ptr().cast_mut();
        let result: *mut Passwd = &raw mut found;

        // The control: this exact entry, returned successfully, does expand.
        assert_eq!(
            pw_dir(0, result).expect("successful lookup").as_deref(),
            Some(std::path::Path::new("/home/example"))
        );

        // Platform failures remain errors here. The ABI adapter owns the
        // compatibility rule that flattens them to "no such user".
        #[cfg(any(target_os = "linux", target_os = "android"))]
        assert!(pw_dir(erange, result).is_err(), "ERANGE");
        assert!(pw_dir(-1, result).is_err(), "a negative return");
        assert!(pw_dir(2, result).is_err(), "ENOENT");

        // The zero-return-with-NULL-result case the rule also names, which is
        // how glibc reports a genuine absence.
        assert_eq!(pw_dir(0, core::ptr::null_mut()).expect("not found"), None);

        // An entry with no home directory at all is not an empty expansion.
        let mut homeless = Passwd::ZEROED;
        assert_eq!(pw_dir(0, &raw mut homeless).expect("empty home"), None);
    }

    /// A name that cannot be in any database is *no such user* rather than a
    /// panic or an empty expansion — including one carrying an interior NUL,
    /// which cannot reach here from the C's `char *` but can from a Rust
    /// caller.
    #[test]
    fn a_name_no_database_can_hold_is_no_such_user() {
        assert_eq!(
            home_directory_named("no-such-user-4b8f2c1e").expect("lookup"),
            None
        );
        assert!(home_directory_named("has\0an interior nul").is_err());
        assert_eq!(home_directory_named("").expect("lookup"), None);
        // The uid space's top value is reserved as "nobody/invalid" by
        // convention and is not allocated to a real account.
        assert_eq!(home_directory(UserId(u32::MAX)).expect("lookup"), None);
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
        // Bounded: `getpwent` walks every backend, and a directory service can
        // hold a great many accounts.
        let names: Vec<OsString> = UserNames::open().take(64).collect();

        let mut absolute = 0;
        for raw in &names {
            let Ok(name) = core::str::from_utf8(raw.as_bytes()) else {
                continue;
            };
            assert!(!name.is_empty(), "the enumeration yielded an empty name");
            let home = home_directory_named(name)
                .unwrap_or_else(|error| panic!("{name} lookup failed: {error}"))
                .unwrap_or_else(|| panic!("{name} was enumerated but does not resolve"));
            if home.is_absolute() {
                absolute += 1;
            }
        }
        assert!(
            names.is_empty() || absolute > 0,
            "no account in the database has an absolute home directory"
        );
    }
}
