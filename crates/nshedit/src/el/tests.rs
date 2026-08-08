use super::*;
use std::os::fd::AsRawFd;

/// A unique scratch path. The bell has no return value and no state to
/// inspect, so the only way to see it is to point `el_outfd` at something
/// readable; `write_fd` refuses a negative descriptor, so it has to be a
/// real file.
fn scratch_path(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!("nshedit-el-{tag}-{}-{n}", std::process::id()));
    p
}

/// What `body` wrote through `el`'s output descriptor.
fn output_of(el: &mut EditLine, tag: &str, body: impl FnOnce(&mut EditLine)) -> Vec<u8> {
    let path = scratch_path(tag);
    let file = File::create(&path).unwrap();
    let saved = el.el_outfd;
    el.el_outfd = file.as_raw_fd();
    body(el);
    el.el_outfd = saved;
    drop(file);
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    bytes
}

// [spec:libedit:sem:el.el-init-fn/test]
/// Construction reports success while storing three descriptors that
/// cannot be used, and the only trace it leaves is `NO_TTY`.
///
/// ERR-core-api-06: the C runs `fileno` on all three streams before any
/// validation and stores whatever comes back, undiagnosed. This module's
/// `fileno` has no `FILE *` to ask — a `CFile` is the C library's object
/// and only the ABI crate may reach into it — so it answers -1, which is
/// exactly what the C stores for a stream with no descriptor. The point
/// of the test is the pair: -1 everywhere *and* a `Some`.
#[test]
fn construction_succeeds_with_three_descriptors_it_cannot_use() {
    let el = el_init("nshedit", ptr::null_mut(), ptr::null_mut(), ptr::null_mut())
        .expect("a decodable program name always constructs");

    assert_eq!((el.el_infd, el.el_outfd, el.el_errfd), (-1, -1, -1));
    assert!(
        el.el_flags & NO_TTY != 0,
        "tty_init cannot query descriptor -1, and the flag it sets is the \
         only place the bad descriptor is recorded"
    );
    // Step 2 records the streams without duplicating or validating them.
    assert!(el.el_infile.is_null() && el.el_outfile.is_null() && el.el_errfile.is_null());
    // Step 3: no application hook, so lookups reach `secure_getenv`.
    assert!(el.el_getenv.is_none());
    // Step 4 copied the name into storage the object owns.
    assert_eq!(el.el_prog, b"nshedit".map(u32::from));

    // Teardown is what proves the subsystems above really were brought
    // up: `el_end` calls every `*_end` unconditionally, `read_end`
    // included, and a constructor that had left one of them half-built
    // would fault here rather than in the assertions above.
    el_end(Some(el));
}

// [spec:libedit:sem:el.el-beep-fn/test]
/// The bell is one literal ASCII BEL when the loaded description has no
/// bell string, it goes to the output descriptor, and it disturbs nothing.
///
/// A blank `EditLine` is the "no capabilities loaded" case exactly:
/// `t_str` is empty, so `good_str(T_BL)` is false and the fallback is what
/// runs. Nothing is flushed either — there is nothing to flush in the port
/// — so the byte is already on the descriptor when this returns.
#[test]
fn the_bell_falls_back_to_one_literal_byte_and_touches_nothing_else() {
    let mut el = blank_editline();
    el.el_line.buffer = vec![0; 8];
    el.el_line.cursor = 3;
    el.el_line.lastchar = 5;
    el.el_cursor.h = 7;
    el.el_cursor.v = 2;

    let out = output_of(&mut el, "beep", el_beep);
    assert_eq!(out, [0x07]);

    // `sem:el.el-beep-fn`: no cursor movement and no editing state.
    assert_eq!(el.el_cursor, CoordT { h: 7, v: 2 });
    assert_eq!((el.el_line.cursor, el.el_line.lastchar), (3, 5));
}

thread_local! {
    /// What [`env_hook`] answers for `HOME`.
    ///
    /// Parked here rather than returned from a local so that the pointer the
    /// hook hands back stays valid past the call, which is the contract
    /// `def:el.editline.el-getenv-fn` puts on an application hook.
    static HOME: RefCell<CString> = RefCell::new(CString::default());
}

/// An `el_getenv` hook: `HOME` is whatever [`HOME`] holds and every other
/// name is unset.
///
/// `EDITRC` is among those, so the resolution falls through to step 3 the way
/// it does for a process that has none — and it goes through the hook rather
/// than the real environment, which is what keeps the test from depending on
/// whoever ran it.
unsafe extern "C" fn env_hook(name: *const c_char) -> *mut c_char {
    // SAFETY: `el_getenv` calls the hook with one NUL-terminated name.
    if unsafe { CStr::from_ptr(name) }.to_bytes() != b"HOME" {
        return ptr::null_mut();
    }
    HOME.with_borrow(|h| h.as_ptr().cast_mut())
}

// [spec:libedit:sem:el.el-source-fn/test]
/// Which file `.editrc` means, at the two edges the C's pointer arithmetic
/// hides.
///
/// An empty `HOME` is the one that surprises. The C's
/// `elpath + (*ptr == '\0')` skips the leading `/`, so the path it builds is
/// the *relative* `.editrc` — and that is the only way `el_source` ever looks
/// at the current directory, despite `histedit.h` advertising a `$PWD`
/// lookup that does not exist.
///
/// The other is that a name is a C string and so ends at its first NUL. A
/// caller passing `"\0/etc/passwd"` has passed the empty name, which step 5
/// rejects; the bytes after it never reach `fopen`.
#[test]
fn the_editrc_name_ends_at_the_first_nul_and_goes_relative_for_an_empty_home() {
    let mut el = blank_editline();
    el.el_getenv = Some(env_hook);

    // Step 3 with `HOME=""`, i.e. step 4's skipped separator.
    assert_eq!(editrc_path(&el, None).as_deref(), Some(&b".editrc"[..]));

    // Step 4 with a real `HOME`: joined with exactly one separator.
    HOME.replace(CString::new("/home/u").unwrap());
    assert_eq!(
        editrc_path(&el, None).as_deref(),
        Some(&b"/home/u/.editrc"[..])
    );

    // Step 1. A caller-supplied name is used verbatim — no `~` expansion, no
    // search path, no directory prefix — and `HOME` is not consulted at all.
    assert_eq!(
        editrc_path(&el, Some(Path::new("rc"))).as_deref(),
        Some(&b"rc"[..])
    );

    // Step 5. The empty name is all it rejects, and the truncation above is
    // what turns the first of these into one. A constructed path never gets
    // here: one always ends in `.editrc`.
    let leading_nul = Path::new(OsStr::from_bytes(b"\0/etc/passwd"));
    assert_eq!(editrc_path(&el, Some(leading_nul)), None);
    assert_eq!(editrc_path(&el, Some(Path::new(""))), None);
}
