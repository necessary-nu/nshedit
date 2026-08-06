use super::*;
use std::ffi::{CStr as StdCStr, CString};

use crate::histedit::{EL_HIST, EL_RESIZE, el_wget, el_wpush, el_wset};

/// A live handle whose descriptors cannot be a terminal, so `tty_init`
/// fails, `NO_TTY` goes up, and `el_gets` takes the unedited read path.
fn editline() -> *mut EditLine {
    let prog = CString::new("nshedit-test").unwrap();
    // SAFETY: an ASCII program name and three NULL streams, which the
    // constructor stores without dereferencing.
    let el = unsafe {
        crate::histedit::el_init_fd(
            prog.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            -1,
            -1,
            -1,
        )
    };
    assert!(!el.is_null());
    el
}

fn done(el: *mut EditLine) {
    // SAFETY: `el` came from `el_init_fd` and is not used again.
    unsafe { crate::histedit::el_end(el) };
}

/// The C's `const char *` result as a Rust string.
fn narrow(p: *const c_char) -> String {
    assert!(!p.is_null());
    // SAFETY: every pointer these entry points hand back is
    // NUL-terminated, and nothing has invalidated it yet.
    unsafe { StdCStr::from_ptr(p) }
        .to_string_lossy()
        .into_owned()
}

/// A scratch file holding `content`, opened for reading, with its
/// descriptor installed as `el`'s input. The `File` is returned because
/// closing it would take the descriptor with it.
fn feed(el: *mut EditLine, content: &[u8]) -> std::fs::File {
    use std::os::fd::AsRawFd;
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("nshedit-eln-{}-{n}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    // SAFETY: `el` is live.
    unsafe { (*el).el_infd = file.as_raw_fd() };
    file
}

// -----------------------------------------------------------------
// el_push / el_getc
// -----------------------------------------------------------------

// [spec:libedit:sem:eln.el-push-fn/test]
// [spec:libedit:sem:histedit.el-push-fn/test]
// [spec:libedit:sem:eln.el-getc-fn/test]
// [spec:libedit:sem:histedit.el-getc-fn/test]
/// The narrow pair, end to end: bytes in through `el_push`, bytes out
/// through `el_getc`, with the wide macro queue in between.
///
/// The second half is the one worth pinning. `el_getc` stores through `cp`
/// *before* it decides whether anything was read, so a caller that
/// pre-loaded the byte finds it zeroed on end of file — ERR-core-api-27,
/// disposition reproduce. End of file is what descriptor -1 produces here:
/// it cannot be put into raw mode, and `el_wgetc` reports that as 0 rather
/// than as an error (ERR-input-24).
#[test]
fn narrow_pushback_comes_back_a_byte_at_a_time() {
    let el = editline();
    let text = CString::new("hi").unwrap();
    // SAFETY: `el` is live and `text` is NUL-terminated.
    unsafe { el_push(el, text.as_ptr()) };

    let mut c: c_char = 0;
    for want in *b"hi" {
        // SAFETY: `el` is live and `c` is writable.
        assert_eq!(unsafe { el_getc(el, &raw mut c) }, 1);
        assert_eq!(c as u8, want);
    }

    c = b'Z' as c_char;
    // SAFETY: as above.
    assert_eq!(unsafe { el_getc(el, &raw mut c) }, 0);
    assert_eq!(c, 0, "the store happens before the early return");
    done(el);
}

// [spec:libedit:sem:eln.el-getc-fn/test]
/// A character with no single-byte form is consumed and lost.
///
/// ERR-core-api-27: `el_getc` converts with `wctob`, so it can only ever
/// deliver a character the locale encodes in one byte. `el_wgetc` has
/// already popped the character off the macro queue by then and this
/// entry point has no pushback, so the caller is told -1/`ERANGE` and the
/// input is gone. U+4E2D is the test case in every locale the port
/// models: multi-byte where the charset can hold it and unencodable where
/// it cannot.
#[test]
fn a_character_with_no_single_byte_form_is_lost_not_queued() {
    let el = editline();
    let wide: [u32; 2] = [0x4e2d, 0];
    // SAFETY: `el` is live and the string is NUL-terminated.
    unsafe { el_wpush(el, wide.as_ptr()) };

    let mut c: c_char = b'Z' as c_char;
    // SAFETY: as above.
    assert_eq!(unsafe { el_getc(el, &raw mut c) }, -1);
    assert_eq!(c, 0);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(nshedit::errno::ERANGE),
        "the one errno this layer produces itself"
    );
    // SAFETY: `el` is live.
    assert!(
        unsafe { (*el).el_read.as_ref().unwrap().macros.r#macro.is_empty() },
        "no pushback: the character is not put back for the next read"
    );
    done(el);
}

// -----------------------------------------------------------------
// el_gets
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-gets-fn/test]
/// A line comes back as bytes, with `*nread` rewritten from a wide count
/// to a byte count — and a NULL `nread` is survivable here where the C
/// faults.
///
/// ERR-core-api-11, disposition define: the wide entry point tolerates a
/// NULL count by substituting a local, and the narrow one dereferences it
/// the moment a line arrives. A null dereference is undefined rather than
/// defined-but-wrong, so the rewrite is skipped instead; everything else
/// about the call is unchanged, which is what the second half asserts.
#[test]
fn a_line_comes_back_as_bytes_with_or_without_somewhere_to_report_its_length() {
    let el = editline();
    // SAFETY: `el` is live. Descriptor -1 could not be queried by
    // `tty_init`, so the unedited read path is already selected.
    assert_ne!(unsafe { (*el).el_flags } & 0x002, 0, "NO_TTY");

    let _fd = feed(el, b"hi\nbye\n");
    let mut n: c_int = -99;
    // SAFETY: `el` is live and `n` is writable.
    let line = unsafe { el_gets(el, &raw mut n) };
    assert_eq!(narrow(line), "hi\n");
    assert_eq!(n, 3, "bytes, not the wide count the read reported");

    // SAFETY: as above, with the NULL the C would have dereferenced.
    let line = unsafe { el_gets(el, ptr::null_mut()) };
    assert_eq!(narrow(line), "bye\n");

    // End of input: `noedit_wgets` reports zero characters and a NULL
    // line, and the encoder short-circuits on it, so the previous
    // string in `el_lgcyconv.cbuff` is left alone rather than cleared.
    let mut n: c_int = -99;
    // SAFETY: as above.
    assert!(unsafe { el_gets(el, &raw mut n) }.is_null());
    assert_eq!(n, 0);
    done(el);
}

// -----------------------------------------------------------------
// el_line
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-line-fn/test]
// [spec:libedit:sem:histedit.el-insertstr-fn/test]
// [spec:libedit:sem:histedit.el-replacestr-fn/test]
/// Insert at the cursor, replace the whole line, and read either back as
/// bytes through the one `LineInfo` the editor owns.
///
/// The three offsets are recomputed rather than measured, so the check
/// that matters is that `cursor` and `lastchar` land at byte positions
/// consistent with the string `buffer` points at. The struct itself is at
/// a fixed address — it is embedded in the `EditLine` — so two live
/// `LineInfo` views of one editor are impossible, which is the second
/// assertion.
#[test]
fn the_narrow_line_view_tracks_what_the_narrow_editing_calls_did() {
    let el = editline();
    let abc = CString::new("abc").unwrap();
    // SAFETY: `el` is live and the strings are NUL-terminated.
    assert_eq!(unsafe { el_insertstr(el, abc.as_ptr()) }, 0);

    let first = unsafe { el_line(el) };
    // SAFETY: `el_line` published all three fields.
    let (buf, cur, last) = unsafe { ((*first).buffer, (*first).cursor, (*first).lastchar) };
    assert_eq!(narrow(buf), "abc");
    // SAFETY: both are offsets into the string just measured.
    assert_eq!(
        unsafe { cur.offset_from(buf) },
        3,
        "insertion left the cursor after the text"
    );
    assert_eq!(unsafe { last.offset_from(buf) }, 3);

    // A NULL and an empty string are the same refusal, and neither
    // disturbs the line.
    // SAFETY: `el` is live.
    assert_eq!(unsafe { el_insertstr(el, ptr::null()) }, -1);
    assert_eq!(unsafe { el_replacestr(el, ptr::null()) }, -1);

    let longer = CString::new("zzzz").unwrap();
    // SAFETY: as above.
    assert_eq!(unsafe { el_replacestr(el, longer.as_ptr()) }, 0);
    let second = unsafe { el_line(el) };
    assert_eq!(
        second, first,
        "one embedded LineInfo, shared by every caller"
    );
    // SAFETY: `el_line` republished all three fields.
    let (buf, last) = unsafe { ((*second).buffer, (*second).lastchar) };
    assert_eq!(narrow(buf), "zzzz");
    // SAFETY: an offset into the string just measured.
    assert_eq!(unsafe { last.offset_from(buf) }, 4);
    done(el);
}

// [spec:libedit:sem:histedit.el-line-fn/test]
/// `el_line` runs the application's `EL_RESIZE` callback, and a callback
/// that calls back in gets the finished `LineInfo` instead of recursing.
///
/// This is the divergence from `el_wline` that matters: the wide form is a
/// cast with no side effects, while every non-nested `el_line` invokes the
/// hook. `FROM_ELLINE` is set nowhere else in the library, so it fires
/// exactly here — and without it the callback's own `el_line` would
/// re-enter the hook forever.
#[test]
fn the_narrow_line_view_calls_the_resize_hook_once_and_survives_re_entry() {
    thread_local! {
        static CALLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        static NESTED: std::cell::Cell<*const LineInfo> =
            const { std::cell::Cell::new(ptr::null()) };
    }

    unsafe extern "C" fn hook(el: *mut EditLine, _arg: *mut c_void) {
        CALLS.with(|c| c.set(c.get() + 1));
        // SAFETY: `el` is the handle the hook was installed against.
        let nested = unsafe { el_line(el) };
        NESTED.with(|c| c.set(nested));
    }

    let el = editline();
    let f: nshedit::chared::ElZfuncT = hook;
    // SAFETY: `EL_RESIZE` takes an `el_zfunc_t` and an opaque cookie.
    assert_eq!(
        unsafe { el_wset(el, EL_RESIZE, f, ptr::null_mut::<c_void>()) },
        0
    );

    let info = unsafe { el_line(el) };
    assert_eq!(
        CALLS.with(std::cell::Cell::get),
        1,
        "once, not once per nesting level"
    );
    assert_eq!(NESTED.with(std::cell::Cell::get), info);
    // SAFETY: `el` is live.
    assert_eq!(
        unsafe { (*el).el_flags } & FROM_ELLINE,
        0,
        "the guard is cleared on the way out"
    );
    done(el);
}

// -----------------------------------------------------------------
// el_parse
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-parse-fn/test]
/// One `.editrc` line, given as an argument vector: the command runs, an
/// unknown one is -1, and a `prog:` qualifier that does not match this
/// editor's program name silently succeeds.
///
/// The qualifier is the trap. It is a regex match with the program name as
/// the *subject*, not an equality test, so `sh:` applies to a program
/// called `nshedit-test` only if it matches as a pattern — and a
/// non-matching qualifier is 0, indistinguishable from a command that ran.
#[test]
fn parsing_an_argument_vector_runs_one_editrc_command() {
    let el = editline();
    let run = |words: &[&str]| -> c_int {
        let owned: Vec<CString> = words.iter().map(|w| CString::new(*w).unwrap()).collect();
        let mut argv: Vec<*const c_char> = owned.iter().map(|w| w.as_ptr()).collect();
        // SAFETY: `el` is live and `argv` holds `argc` NUL-terminated
        // strings that outlive the call.
        unsafe { el_parse(el, argv.len() as c_int, argv.as_mut_ptr()) }
    };

    assert_eq!(run(&["edit", "off"]), 0);
    // SAFETY: `el` is live. EDIT_DISABLED is 0x004.
    assert_ne!(
        unsafe { (*el).el_flags } & 0x004,
        0,
        "the builtin really ran"
    );
    assert_eq!(run(&["edit", "on"]), 0);
    assert_eq!(unsafe { (*el).el_flags } & 0x004, 0);

    assert_eq!(run(&["nosuchcommand"]), -1);
    assert_eq!(run(&["edit"]), 1, "the builtin's -1, negated by el_wparse");
    assert_eq!(
        run(&["zzz:edit", "off"]),
        0,
        "not this program, so nothing runs — and that is not an error"
    );
    assert_eq!(unsafe { (*el).el_flags } & 0x004, 0);

    // The guards `el_parse` adds because the C dereferences instead.
    // SAFETY: `el` is live; a zero or negative count reads no argv.
    unsafe {
        assert_eq!(el_parse(el, 0, ptr::null_mut()), -1);
        assert_eq!(el_parse(el, -1, ptr::null_mut()), -1);
        assert_eq!(el_parse(el, 1, ptr::null_mut()), -1);
    }
    done(el);
}

// -----------------------------------------------------------------
// el_set / el_get
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-set-fn/test]
/// The narrow setter decodes what the wide one takes wide, and marks the
/// prompt narrow — which is the only reason the prompt ops are not simply
/// forwarded.
#[test]
fn the_narrow_setter_decodes_and_records_that_it_was_narrow() {
    unsafe extern "C" fn never_called(_: *mut EditLine) -> *mut u32 {
        ptr::null_mut()
    }

    let el = editline();
    let vi = CString::new("vi").unwrap();
    let wordchars = CString::new("_@").unwrap();
    // SAFETY: each op is called with exactly the arguments its rule
    // defines, against a live handle.
    unsafe {
        assert_eq!(el_set(el, EL_EDITOR, vi.as_ptr()), 0);
        let mut out: *const u32 = ptr::null();
        assert_eq!(el_wget(el, EL_EDITOR, &raw mut out), 0);
        assert_eq!(*out, u32::from(b'v'));

        assert_eq!(el_set(el, EL_WORDCHARS, wordchars.as_ptr()), 0);
        assert_eq!((*el).el_map.wordchars.as_deref(), Some(&[0x5f, 0x40][..]));

        // ERR-core-api-09, disposition define: the C hands the decode
        // result straight to `wcscmp`/`wcsdup` without checking it.
        assert_eq!(el_set(el, EL_EDITOR, ptr::null::<c_char>()), -1);
        assert_eq!(el_set(el, EL_WORDCHARS, ptr::null::<c_char>()), -1);

        let p: ElPfuncT = never_called;
        assert_eq!(el_set(el, EL_PROMPT, p), 0);
        assert_eq!(
            (*el).el_prompt.p_wide,
            0,
            "the narrow setter is the only thing that stores 0 here"
        );
        assert_eq!(crate::histedit::el_wset(el, EL_PROMPT, p), 0);
        assert_eq!((*el).el_prompt.p_wide, 1);
    }
    done(el);
}

// [spec:libedit:sem:histedit.el-set-fn/test]
/// `el_set(EL_HIST)` raises `NARROW_HISTORY` unconditionally and
/// `el_wset(EL_HIST)` only ever lowers it, and then only in a single-byte
/// locale.
///
/// ERR-core-api-16, disposition reproduce: this is the flag's only set
/// site in the whole library, so which entry point installed the history
/// decides how the bridge converts for the rest of the editor's life —
/// and in a multibyte locale a wide `EL_HIST` after a narrow one does not
/// undo it.
#[test]
fn installing_history_through_the_narrow_setter_pins_the_bridge_narrow() {
    unsafe extern "C" fn hist(
        _: *mut c_void,
        _: *mut nshedit::histedit::HistEventW,
        _: c_int,
        _: ...
    ) -> c_int {
        0
    }

    let el = editline();
    let f: HistFunT = hist;
    // SAFETY: `EL_HIST` takes a `hist_fun_t` and the opaque handle it is
    // called with; the function is never invoked here.
    unsafe {
        assert_eq!(el_set(el, EL_HIST, f, ptr::dangling_mut::<c_void>()), 0);
        assert_ne!((*el).el_flags & NARROW_HISTORY, 0);

        assert_eq!(
            crate::histedit::el_wset(el, EL_HIST, f, ptr::dangling_mut::<c_void>()),
            0
        );
        let cleared = (*el).el_flags & NARROW_HISTORY == 0;
        assert_eq!(
            cleared,
            nshedit::el::mb_cur_max() == 1,
            "the wide setter clears the flag only in a single-byte locale"
        );
    }
    done(el);
}

// [spec:libedit:sem:histedit.el-get-fn/test]
/// What the narrow getter forwards, and what it still does not answer.
///
/// The forwarded ops are the ones whose argument needs no conversion, and
/// `EL_GETTC` is among them because capability names are `char *` in both
/// APIs — which is what `rl_get_screen_size` depends on. Everything else
/// still falls to the C's `default` arm: `EL_PREP_TERM` is a set-only op
/// this wrapper pretends to forward and always answers -1
/// (ERR-core-api-17), and `EL_EDITOR`/`EL_WORDCHARS`/the prompt family
/// have no narrow arm yet, where the C answers them. That last group is a
/// gap, not a reproduced defect, and this pins where it currently is.
#[test]
fn the_narrow_getter_forwards_what_needs_no_conversion() {
    let el = editline();
    let cookie = 0xabc_usize as *mut c_void;
    let mut back: *mut c_void = ptr::null_mut();
    let mut lines: c_int = -1;
    let li = CString::new("li").unwrap();
    // SAFETY: each op is called with exactly the out-parameter its rule
    // defines, against a live handle.
    unsafe {
        assert_eq!(el_set(el, EL_CLIENTDATA, cookie), 0);
        assert_eq!(el_get(el, EL_CLIENTDATA, &raw mut back), 0);
        assert_eq!(back, cookie);

        let mut mode: c_int = -1;
        assert_eq!(el_set(el, EL_EDITMODE, 0 as c_int), 0);
        assert_eq!(el_get(el, EL_EDITMODE, &raw mut mode), 0);
        assert_eq!(mode, 0);

        // `EL_GETTC` reads exactly two arguments and consumes no
        // sentinel, despite the header's `..., NULL` (ERR-core-api-29).
        assert_eq!(el_get(el, EL_GETTC, li.as_ptr(), &raw mut lines), 0);
        assert!(lines > 0, "the loaded description's line count");

        // ERR-core-api-17: forwarded to an op `el_wget` has no arm for.
        let mut junk: c_int = 0;
        assert_eq!(el_get(el, EL_PREP_TERM, &raw mut junk), -1);

        // Not yet implemented on this side; the C answers 0 here with a
        // pointer into `el_lgcyconv.cbuff`.
        let mut s: *const c_char = ptr::null();
        assert_eq!(el_get(el, EL_EDITOR, &raw mut s), -1);
        assert!(s.is_null());
    }
    done(el);
}
