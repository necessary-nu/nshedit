use super::*;

const HISTORY_SCRATCH_DIRECTORY: &str = "/tmp";

/// Create the truncation scratch file privately, then remove its name before
/// any history bytes are copied into it.
///
/// `NamedTempFile` supplies the exclusive random-name creation and the 0600
/// mode. Converting it into a `File` immediately unlinks that name, so cleanup
/// is owned by the descriptor even if unwinding or process termination skips
/// Rust destructors.
fn history_scratch_file_in(directory: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::PermissionsExt;

    tempfile::Builder::new()
        .prefix(".history")
        .permissions(std::fs::Permissions::from_mode(0o600))
        .tempfile_in(directory)
        .map(tempfile::NamedTempFile::into_file)
}

fn history_scratch_file() -> std::io::Result<std::fs::File> {
    history_scratch_file_in(std::path::Path::new(HISTORY_SCRATCH_DIRECTORY))
}

/// The C's `return errno;` after a failing stdio call.
fn errno_of(e: &std::io::Error) -> c_int {
    e.raw_os_error().unwrap_or(EINVAL)
}

/// The path argument these four entry points share: the caller's, or
/// `_default_history_file()`, or the C's `return errno` when there is
/// neither.
///
/// # Safety
///
/// `filename` must be NULL or a NUL-terminated string. The result borrows
/// from either the caller's string or the process-lifetime cache
/// `_default_history_file` keeps.
unsafe fn history_file_name(filename: *const c_char) -> Result<*const c_char, c_int> {
    if !filename.is_null() {
        return Ok(filename);
    }
    let d = _default_history_file();
    if d.is_null() {
        // The C's "whatever `getpwuid` left", read from the C's own `errno`
        // because that is where the failing lookup wrote.
        return Err(crate::errno::get());
    }
    Ok(d)
}

/// [`history_file_name`] for the two entry points that open the file
/// themselves rather than handing the name to `history()`.
///
/// # Safety
///
/// As [`history_file_name`].
unsafe fn history_file_path(filename: *const c_char) -> Result<std::path::PathBuf, c_int> {
    // SAFETY: the caller guarantees the string.
    let name = unsafe { c_bytes(history_file_name(filename)?) };
    Ok(std::path::PathBuf::from(std::ffi::OsStr::from_bytes(name)))
}

// [spec:libedit:def:readline.history-truncate-file-fn+1]
// [spec:libedit:sem:readline.history-truncate-file-fn+1]
#[unsafe(no_mangle)]
#[doc = include_str!("../ffi_safety.md")]
pub unsafe extern "C" fn history_truncate_file(filename: *const c_char, nlines: c_int) -> c_int {
    // SAFETY: `filename` is NULL or a NUL-terminated string.
    let path = match unsafe { history_file_path(filename) } {
        Ok(p) => p,
        Err(e) => return e,
    };
    let mut fp = match std::fs::File::options().read(true).write(true).open(&path) {
        Ok(f) => f,
        Err(e) => return errno_of(&e),
    };
    let mut tp = match history_scratch_file() {
        Ok(file) => file,
        Err(e) => return errno_of(&e),
    };

    truncate_through_temp(&mut fp, &mut tp, nlines)
}

/// The three phases of [`history_truncate_file`], with the file handles
/// already open.
fn truncate_through_temp(fp: &mut std::fs::File, tp: &mut std::fs::File, nlines: c_int) -> c_int {
    // Phase 1 — copy the whole file to the temporary. The C does this in
    // 4096-byte blocks and keeps the tail block; the block bookkeeping is not
    // observable, so the copy is whole here.
    let mut buf = Vec::new();
    if let Err(e) = fp.read_to_end(&mut buf) {
        return errno_of(&e);
    }
    if let Err(e) = tp.write_all(&buf) {
        return errno_of(&e);
    }
    if let Err(e) = tp.flush() {
        return errno_of(&e);
    }

    // `nlines <= 0` cannot terminate the C's backward scan — the first
    // decrement makes it negative, it never reaches 0, and the cut point ends
    // up one byte before a block boundary, i.e. arbitrary (ERR-readline-14,
    // UB). Defined here as leaving the file alone.
    if nlines <= 0 || buf.is_empty() {
        return 0;
    }

    // Phase 2 — walk backwards counting newlines. The scan starts one before
    // the last byte when that byte is a newline, so a file's final newline is
    // not counted, and stops just past the newline that brings the count to
    // zero.
    let mut nlines = nlines;
    let mut i = if *buf.last().unwrap() == b'\n' {
        buf.len() - 1
    } else {
        buf.len()
    };
    let mut cut = None;
    while i > 0 {
        i -= 1;
        if buf[i] == b'\n' {
            nlines -= 1;
            if nlines == 0 {
                cut = Some(i + 1);
                break;
            }
        }
    }
    // "File shorter than requested" is a silent success that changes nothing.
    let Some(cut) = cut else {
        return 0;
    };

    // Phase 3 — copy the retained tail back over the original. The C inspects
    // `ferror(fp)` where it should inspect `ferror(tp)`, so a read error on
    // the temporary is reported as success (ERR-readline-14, reproduced): the
    // read here is from the buffer already in hand, which cannot fail, and
    // the equivalent silent success is the empty-tail case.
    let mut ret = 0;
    if let Err(e) = fp.seek(SeekFrom::Start(0)) {
        return errno_of(&e);
    }
    if let Err(e) = fp.write_all(&buf[cut..]) {
        ret = errno_of(&e);
    }
    let _ = fp.flush();
    // The rewrite is not atomic: a crash between the seek and the truncate
    // leaves a corrupted history file, which is the C's exposure too.
    if let Ok(off) = fp.stream_position()
        && off > 0
    {
        let _ = fp.set_len(off);
    }
    ret
}

// [spec:libedit:def:readline.read-history-fn]
// [spec:libedit:sem:readline.read-history-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("../ffi_safety.md")]
pub unsafe extern "C" fn read_history(filename: *const c_char) -> c_int {
    // SAFETY: single-threaded module state; `filename` is NULL or a
    // NUL-terminated string.
    unsafe {
        lazy_init();
        let name = match history_file_name(filename) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(c_bytes(name)));
        // The C's `errno = 0`, cleared in both homes so that neither a stale
        // platform value nor a stale core one can be mistaken for this call's
        // failure. The sample is taken after it, or the clear itself would
        // count as something to publish.
        crate::errno::set(0);
        let mark = crate::errno::mark();
        if history::execute(HistoryRequest::Load(Some(path))).is_err() {
            // Whatever failed wrote one of the two homes — a decoder in the
            // history layer writes the core's, a failing `open` or `read`
            // writes the platform's — so they are reconciled before the read.
            crate::errno::publish(mark);
            let e = crate::errno::get();
            return if e != 0 { e } else { EINVAL };
        }
        if let Ok(reply) = history::execute(HistoryRequest::Size)
            && let Some(size) = history::size(reply)
        {
            // `history_base` and `history_offset` are *not* adjusted, so
            // callers are expected to follow with `using_history()`
            // (ERR-readline-40).
            history_length = size;
        }
        if history_length < 0 {
            return EINVAL;
        }
        0
    }
}

// [spec:libedit:def:readline.write-history-fn+1]
// [spec:libedit:sem:readline.write-history-fn+1]
#[unsafe(no_mangle)]
#[doc = include_str!("../ffi_safety.md")]
pub unsafe extern "C" fn write_history(filename: *const c_char) -> c_int {
    // SAFETY: as `read_history`.
    unsafe {
        lazy_init();
        let name = match history_file_name(filename) {
            Ok(n) => n,
            Err(e) => return e,
        };
        let path = std::path::Path::new(std::ffi::OsStr::from_bytes(c_bytes(name)));
        // No `errno = 0` here — unlike `read_history` the C does not clear it,
        // so a value left over from before the call is what a failure with no
        // `errno` of its own reports. Reproduced by sampling without clearing.
        let mark = crate::errno::mark();
        // H_SAVE replaces the file only after the complete encoded history is
        // flushed successfully.
        if history::execute(HistoryRequest::Save(Some(path))).is_err() {
            crate::errno::publish(mark);
            let e = crate::errno::get();
            if e != 0 { e } else { EINVAL }
        } else {
            0
        }
    }
}

// [spec:libedit:def:readline.append-history-fn+1]
// [spec:libedit:sem:readline.append-history-fn+1]
#[unsafe(no_mangle)]
#[doc = include_str!("../ffi_safety.md")]
pub unsafe extern "C" fn append_history(n: c_int, filename: *const c_char) -> c_int {
    // This is the one history entry point that does not delegate to the
    // exported variadic history function; see the descriptor-save note below.
    // SAFETY: as `read_history`.
    unsafe {
        lazy_init();
        let path = match history_file_path(filename) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let mut fp = match std::fs::File::options()
            .append(true)
            .create(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) => return errno_of(&e),
        };
        // `crate::cstdio` reads and writes a stream the *application* owns;
        // this function instead makes a Rust file itself. Its route is
        // The ABI history owner's descriptor writer is
        // `H_NSAVE_FP` with the caller's stream replaced by a descriptor
        // — exactly the shape this function has: it opened the file ITSELF, so
        // there is no application-owned `FILE *` to respect and nothing on
        // `no-c-ffi`'s enumeration to reach for. That distinction is the whole
        // reason the decision permits this and not `fdopen`.
        //
        // Neither a NULL nor a fabricated cookie is an alternative. A NULL one
        // used to go over to `H_NSAVE_FP`, which then failed its cookie write,
        // so `append_history` returned EINVAL and the file it had just opened
        // was closed empty — measured by `conformance/driver/readline_api.c`,
        // which is what turned "a gap recorded here" into "this function does
        // not work". A fabricated pointer is worse: the dispatcher calls the
        // real `ftell` on whatever arrives, so a `Box<File>` cast to `CFile`
        // would be handed to the C library as a `FILE *`.
        //
        // The descriptor is borrowed: `fp` still owns it and closes it below,
        // as the C's `fclose` does.
        //
        // Seek it to the end first; this is load-bearing rather than
        // tidiness. The descriptor save path decides whether to write the
        // `_HiStOrY_V2_` cookie from `ftell(fp) == 0`, faithfully to the C.
        // The C reaches it through `fopen(filename, "a")`, and glibc
        // positions an append stream at EOF, so `ftell` reports the size. A
        // raw `O_APPEND` descriptor instead sits at offset 0 until its first
        // write — so without this, appending to a history file writes a
        // SECOND cookie into the middle of it, which `history_load` then
        // reads as an entry. Measured: the port did exactly that.
        //
        // `O_APPEND` already forces every write to the end, so seeking
        // changes only what the position REPORTS, which is precisely the
        // question `at_start` is asking.
        if let Err(error) = fp.seek(SeekFrom::End(0)) {
            return errno_of(&error);
        }
        // As `write_history`: sampled, not cleared.
        let mark = crate::errno::mark();
        let rc = crate::history::save_fd(runtime_history(), n as usize, fp.as_raw_fd());
        // The C captures `errno` before `fclose`, which can overwrite it.
        let e = if rc.is_err() {
            crate::errno::publish(mark);
            crate::errno::get()
        } else {
            0
        };
        // The file this function opened is the one it closes.
        drop(fp);
        if rc.is_err() {
            return if e != 0 { e } else { EINVAL };
        }
        0
    }
}

#[cfg(test)]
mod temporary_file_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // [spec:libedit:sem:readline.history-truncate-file-fn+1/test]
    #[test]
    fn history_scratch_private_and_anonymous() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let mut scratch = history_scratch_file_in(directory.path()).expect("history scratch file");

        assert_eq!(
            scratch.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            0,
            "the scratch name must be gone before history bytes are written"
        );

        scratch.write_all(b"private history").unwrap();
        scratch.rewind().unwrap();
        let mut round_trip = Vec::new();
        scratch.read_to_end(&mut round_trip).unwrap();
        assert_eq!(round_trip, b"private history");
    }
}
