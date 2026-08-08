use super::*;

/// C: `static const char _history_tmp_template[] = "/tmp/.historyXXXXXX";`
const HISTORY_TMP_TEMPLATE: &str = "/tmp/.historyXXXXXX";

/// `mkstemp(template)` — a fresh file in `/tmp`, created exclusively.
///
/// The scratch file is always in `/tmp` regardless of where the history file
/// lives, so private history contents transit a world-writable directory and
/// the operation fails if `/tmp` is not writable. That is observable, so it
/// is preserved rather than quietly moved next to the target
/// (ERR-readline-14).
fn mkstemp() -> std::io::Result<(std::fs::File, std::path::PathBuf)> {
    let stem = HISTORY_TMP_TEMPLATE.trim_end_matches('X');
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos() as u64 ^ d.as_secs());
    let mut last = std::io::Error::from(std::io::ErrorKind::AlreadyExists);
    for _ in 0..64 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let name = format!("{stem}{:06}", seed % 1_000_000);
        match std::fs::File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&name)
        {
            Ok(f) => return Ok((f, std::path::PathBuf::from(name))),
            Err(e) => last = e,
        }
    }
    Err(last)
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
    Ok(std::path::PathBuf::from(
        String::from_utf8_lossy(name).into_owned(),
    ))
}

// [spec:libedit:def:readline.history-truncate-file-fn]
// [spec:libedit:sem:readline.history-truncate-file-fn]
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
    let (mut tp, template) = match mkstemp() {
        Ok(t) => t,
        Err(e) => return errno_of(&e),
    };

    // The whole operation, with the temporary always unlinked on the way out —
    // the C's `out3`/`out2`/`out1` chain, which the success path falls through
    // as well.
    let ret = truncate_through_temp(&mut fp, &mut tp, nlines);
    drop(tp);
    let _ = std::fs::remove_file(&template);
    ret
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
    let mut ev = EMPTY_EVENT;
    // SAFETY: single-threaded module state; `filename` is NULL or a
    // NUL-terminated string.
    unsafe {
        lazy_init();
        let name = match history_file_name(filename) {
            Ok(n) => n,
            Err(e) => return e,
        };
        // The C's `errno = 0`, cleared in both homes so that neither a stale
        // platform value nor a stale core one can be mistaken for this call's
        // failure. The sample is taken after it, or the clear itself would
        // count as something to publish.
        crate::errno::set(0);
        let mark = crate::errno::mark();
        if history_va(H, &mut ev, H_LOAD, name) == -1 {
            // Whatever failed wrote one of the two homes — a decoder in the
            // history layer writes the core's, a failing `open` or `read`
            // writes the platform's — so they are reconciled before the read.
            crate::errno::publish(mark);
            let e = crate::errno::get();
            return if e != 0 { e } else { EINVAL };
        }
        if history_va(H, &mut ev, H_GETSIZE) == 0 {
            // `history_base` and `history_offset` are *not* adjusted, so
            // callers are expected to follow with `using_history()`
            // (ERR-readline-40).
            history_length = ev.num;
        }
        if history_length < 0 {
            return EINVAL;
        }
        0
    }
}

// [spec:libedit:def:readline.write-history-fn]
// [spec:libedit:sem:readline.write-history-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("../ffi_safety.md")]
pub unsafe extern "C" fn write_history(filename: *const c_char) -> c_int {
    let mut ev = EMPTY_EVENT;
    // SAFETY: as `read_history`.
    unsafe {
        lazy_init();
        let name = match history_file_name(filename) {
            Ok(n) => n,
            Err(e) => return e,
        };
        // No `errno = 0` here — unlike `read_history` the C does not clear it,
        // so a value left over from before the call is what a failure with no
        // `errno` of its own reports. Reproduced by sampling without clearing.
        let mark = crate::errno::mark();
        // H_SAVE truncates or creates the file and writes the signature line
        // and every event `strvis`-escaped — the frozen on-disk format.
        if history_va(H, &mut ev, H_SAVE, name) == -1 {
            crate::errno::publish(mark);
            let e = crate::errno::get();
            if e != 0 { e } else { EINVAL }
        } else {
            0
        }
    }
}

// [spec:libedit:def:readline.append-history-fn]
// [spec:libedit:sem:readline.append-history-fn]
#[unsafe(no_mangle)]
#[doc = include_str!("../ffi_safety.md")]
pub unsafe extern "C" fn append_history(n: c_int, filename: *const c_char) -> c_int {
    // No [`EMPTY_EVENT`] here. The C declares one for its `H_NSAVE_FP` call,
    // and this is the one history entry point that does not make that call —
    // see the note on `history_save_fd` below — so there is no out-parameter
    // to declare.
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
        // `nshedit::history::history_save_fd`, which is
        // `history_save_fp` with the caller's stream replaced by a descriptor
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
        // Seek it to the end first, and this is load-bearing rather than
        // tidiness. `history_save_fd` decides whether to write the
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
        let _ = fp.seek(SeekFrom::End(0));
        // As `write_history`: sampled, not cleared.
        let mark = crate::errno::mark();
        let rc = nshedit::history::history_save_fd(
            (&mut *H).compatibility_mut(),
            n as usize,
            fp.as_raw_fd(),
        );
        // The C captures `errno` before `fclose`, which can overwrite it.
        let e = if rc == -1 {
            crate::errno::publish(mark);
            crate::errno::get()
        } else {
            0
        };
        // The file this function opened is the one it closes.
        drop(fp);
        if rc == -1 {
            return if e != 0 { e } else { EINVAL };
        }
        0
    }
}
