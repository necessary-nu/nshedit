//! Process privilege state reduced to one environment-trust policy.

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::Read;

#[cfg(target_os = "linux")]
const AT_NULL: usize = 0;
#[cfg(target_os = "linux")]
const AT_SECURE: usize = 23;

/// Whether process environment variables may steer editor behaviour.
///
/// This is a policy result rather than a raw syscall result. In particular,
/// an unavailable or malformed platform observation is [`Ignored`][Self::Ignored],
/// so callers cannot accidentally turn an unknown security state into trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnvironmentTrust {
    /// Read the inherited environment.
    Honoured,
    /// Ignore the inherited environment and use fixed or caller-supplied
    /// defaults instead.
    Ignored,
}

impl EnvironmentTrust {
    /// Derive the policy for the current process.
    ///
    /// Linux combines the kernel's `AT_SECURE` auxiliary-vector entry with
    /// real/effective uid and gid comparisons. Darwin combines `issetugid(2)`
    /// with the same credential comparisons. Windows has no set-id executable
    /// transition and is explicitly classified as ordinary. Any other target
    /// without a proved observation, or a Linux process whose auxiliary vector
    /// cannot be read completely, is treated as untrusted.
    // [spec:libedit:sem:el.secure-getenv-fn]
    #[must_use]
    pub fn for_process() -> Self {
        classify(platform_secure_execution(), credentials_changed())
    }

    /// Whether a consumer may read the inherited environment.
    #[must_use]
    pub const fn permits_environment(self) -> bool {
        matches!(self, Self::Honoured)
    }
}

const fn classify(platform_secure: Option<bool>, credentials_changed: bool) -> EnvironmentTrust {
    match (platform_secure, credentials_changed) {
        (Some(false), false) => EnvironmentTrust::Honoured,
        _ => EnvironmentTrust::Ignored,
    }
}

#[cfg(unix)]
fn credentials_changed() -> bool {
    rustix::process::getuid() != rustix::process::geteuid()
        || rustix::process::getgid() != rustix::process::getegid()
}

#[cfg(not(unix))]
const fn credentials_changed() -> bool {
    false
}

/// Linux publishes the loader's decision as pairs of native-width words in
/// `/proc/self/auxv`. Reading the entry directly preserves a real failure
/// state, unlike `getauxval(AT_SECURE)`, whose zero return means either
/// "ordinary process" or "entry unavailable" unless libc's errno storage is
/// also added to this boundary.
#[cfg(target_os = "linux")]
fn platform_secure_execution() -> Option<bool> {
    const MAX_AUXV_BYTES: u64 = 4096;

    let file = File::open("/proc/self/auxv").ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_AUXV_BYTES).read_to_end(&mut bytes).ok()?;
    parse_linux_auxv(&bytes)
}

#[cfg(target_os = "linux")]
fn parse_linux_auxv(bytes: &[u8]) -> Option<bool> {
    const WORD_BYTES: usize = core::mem::size_of::<usize>();
    const ENTRY_BYTES: usize = WORD_BYTES * 2;

    let (entries, remainder) = bytes.as_chunks::<ENTRY_BYTES>();
    if !remainder.is_empty() {
        return None;
    }
    let mut secure = None;
    for entry in entries {
        let kind = usize::from_ne_bytes(entry[..WORD_BYTES].try_into().ok()?);
        let value = usize::from_ne_bytes(entry[WORD_BYTES..].try_into().ok()?);
        match kind {
            AT_NULL => return secure,
            AT_SECURE => secure = Some(value != 0),
            _ => {}
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn platform_secure_execution() -> Option<bool> {
    unsafe extern "C" {
        fn issetugid() -> core::ffi::c_int;
    }

    // SAFETY: `issetugid` takes no pointers, owns no resources, and reports
    // process state without an error return.
    Some(unsafe { issetugid() } != 0)
}

#[cfg(windows)]
const fn platform_secure_execution() -> Option<bool> {
    Some(false)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
const fn platform_secure_execution() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::{EnvironmentTrust, classify};

    // [spec:libedit:sem:el.secure-getenv-fn/test]
    // [spec:nshedit:req:platform.typed-boundary/test]
    #[test]
    fn environment_trust_is_fail_closed() {
        assert_eq!(classify(Some(false), false), EnvironmentTrust::Honoured);
        assert_eq!(classify(Some(true), false), EnvironmentTrust::Ignored);
        assert_eq!(classify(Some(false), true), EnvironmentTrust::Ignored);
        assert_eq!(classify(None, false), EnvironmentTrust::Ignored);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_auxv_requires_secure_marker() {
        use super::{AT_NULL, AT_SECURE, parse_linux_auxv};

        fn auxv(entries: &[(usize, usize)]) -> Vec<u8> {
            entries
                .iter()
                .flat_map(|(kind, value)| kind.to_ne_bytes().into_iter().chain(value.to_ne_bytes()))
                .collect()
        }

        assert_eq!(
            parse_linux_auxv(&auxv(&[(AT_SECURE, 0), (AT_NULL, 0)])),
            Some(false)
        );
        assert_eq!(
            parse_linux_auxv(&auxv(&[(AT_SECURE, 1), (AT_NULL, 0)])),
            Some(true)
        );
        assert_eq!(parse_linux_auxv(&auxv(&[(AT_NULL, 0)])), None);
        assert_eq!(parse_linux_auxv(&[0]), None);
    }
}
