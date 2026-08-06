use super::*;
use nshedit::prompt::ElPfuncT;
use std::ffi::CStr;

/// A live handle with all three descriptors deliberately unusable: -1
/// makes every `tcgetattr` fail, which is what keeps these tests off a
/// terminal without changing any path they exercise.
///
/// The streams are stored verbatim and never dereferenced by
/// `el_init_fd` — it is `EL_SETFP` alone that hands one to `fileno` — so a
/// caller that wants to tell the three apart may pass sentinels.
fn editline_with(fin: CFile, fout: CFile, ferr: CFile) -> *mut EditLine {
    let prog = CString::new("nshedit-test").unwrap();
    // SAFETY: an ASCII program name; `sem:histedit.el-init-fd-fn` stores
    // the three streams without dereferencing them.
    let el = unsafe { el_init_fd(prog.as_ptr(), fin, fout, ferr, -1, -1, -1) };
    assert!(!el.is_null());
    el
}

fn editline() -> *mut EditLine {
    editline_with(
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
    )
}

/// Drop a handle from [`editline`].
fn done(el: *mut EditLine) {
    // SAFETY: `el` came from `el_init_fd` and is not used again.
    unsafe { el_end(el) };
}

/// The C's `L"..."`, NUL-terminated, for the wide entry points.
fn wcs(s: &str) -> Vec<u32> {
    s.chars()
        .map(u32::from)
        .chain(core::iter::once(0))
        .collect()
}

/// A scratch file to point a descriptor at. The bell and the read path
/// have no return value and no state between them, so a real file is the
/// only place their bytes can be seen.
fn scratch_path(tag: &str) -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut p = std::env::temp_dir();
    p.push(format!("nshedit-abi-{tag}-{}-{n}", std::process::id()));
    p
}

/// What `body` wrote through `el`'s output descriptor.
fn output_of(el: *mut EditLine, tag: &str, body: impl FnOnce(*mut EditLine)) -> Vec<u8> {
    use std::os::fd::AsRawFd;
    let path = scratch_path(tag);
    let file = std::fs::File::create(&path).unwrap();
    // SAFETY: `el` is live for the whole of this function.
    let saved = unsafe { (*el).el_outfd };
    unsafe { (*el).el_outfd = file.as_raw_fd() };
    body(el);
    unsafe { (*el).el_outfd = saved };
    drop(file);
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    bytes
}

// -----------------------------------------------------------------
// el_reset, el_beep
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-reset-fn/test]
/// What `el_reset` throws away and, more usefully, what it does not.
///
/// The rule is explicit that the pushed-back macro queue survives and that
/// history is untouched, and both are easy to get wrong by reaching for a
/// "reset everything" teardown. The line contents above `lastchar` survive
/// too — only the positions move — because `ch_reset` clears no memory.
#[test]
fn resetting_empties_the_line_but_keeps_the_pushed_back_input() {
    let el = editline();
    let text = wcs("hello");
    // SAFETY: `el` is live and `text` is NUL-terminated.
    assert_eq!(unsafe { el_winsertstr(el, text.as_ptr()) }, 0);
    let pushed = wcs("xy");
    unsafe { el_wpush(el, pushed.as_ptr()) };

    // SAFETY: `el` is live.
    let e = unsafe { &mut *el };
    e.el_state.doingarg = 1;
    e.el_state.argument = 4;
    e.el_chared.c_kill.mark = 3;
    e.el_history.eventno = 7;
    assert_eq!(e.el_line.lastchar, 5);

    unsafe { el_reset(el) };

    let e = unsafe { &mut *el };
    assert_eq!((e.el_line.cursor, e.el_line.lastchar), (0, 0));
    assert_eq!(
        &e.el_line.buffer[..5],
        &wcs("hello")[..5],
        "the text is still in the buffer above lastchar; only the \
         positions were reset"
    );
    assert_eq!((e.el_state.doingarg, e.el_state.argument), (0, 1));
    assert_eq!(e.el_chared.c_kill.mark, 0);
    assert_eq!(e.el_history.eventno, 0);
    assert_eq!(
        e.el_read.as_ref().unwrap().macros.r#macro.len(),
        1,
        "el_reset does not clear the macro queue"
    );
    done(el);
}

// [spec:libedit:sem:histedit.el-beep-fn/test]
/// The bell reaches the handle's *output* descriptor and nothing else
/// moves — in particular the cursor record and the line are untouched, and
/// there is no flush to wait for.
///
/// `t_str[T_BL]` is cleared first so the outcome does not depend on
/// whatever `bel` the host's terminfo happens to carry; that is the
/// "no bell capability" branch, which writes the literal 0x07.
#[test]
fn the_bell_goes_to_the_output_descriptor() {
    let el = editline();
    // SAFETY: `el` is live.
    let e = unsafe { &mut *el };
    // `T_BL` is `pub(crate)` in the core; slot 1 is the bell string.
    e.el_terminal.t_str[1] = None;
    e.el_cursor.h = 5;

    let out = output_of(el, "beep", |el| unsafe { el_beep(el) });
    assert_eq!(out, [0x07]);
    // SAFETY: `el` is live.
    assert_eq!(unsafe { (*el).el_cursor.h }, 5);
    assert_eq!(unsafe { (*el).el_line.lastchar }, 0);
    done(el);
}

// -----------------------------------------------------------------
// el_wpush / el_wgetc
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-wpush-fn/test]
// [spec:libedit:sem:histedit.el-wgetc-fn/test]
/// Pushes queue, they do not nest, despite the field being called a level.
///
/// This is the claim in the rule that a reader is most likely to get
/// backwards: `el_wpush` appends at `macro[++level]` while `el_wgetc`
/// always reads `macro[0]`, so a string pushed while an earlier one is
/// still draining is consumed *after* it. Reading straight through two
/// pushes is the only way to see the order.
#[test]
fn pushed_back_input_comes_back_first_in_first_out() {
    let el = editline();
    let first = wcs("ab");
    let second = wcs("cd");
    // SAFETY: `el` is live and both strings are NUL-terminated.
    unsafe {
        el_wpush(el, first.as_ptr());
        el_wpush(el, second.as_ptr());
    }

    let mut got = String::new();
    for _ in 0..4 {
        let mut wc: u32 = 0;
        // SAFETY: `el` is live and `wc` is writable.
        assert_eq!(unsafe { el_wgetc(el, &raw mut wc) }, 1);
        got.push(char::from_u32(wc).unwrap());
    }
    assert_eq!(got, "abcd");

    // The queue empties as the last character of each entry is taken, so
    // by now there is nothing left and the read falls through to the tty.
    // SAFETY: `el` is live.
    assert!(unsafe { (*el).el_read.as_ref().unwrap().macros.r#macro.is_empty() });
    let mut wc: u32 = 0xdead;
    // Descriptor -1 cannot be put into raw mode, and `el_wgetc` reports
    // that as end of file rather than as an error (ERR-input-24), leaving
    // `*cp` alone.
    assert_eq!(unsafe { el_wgetc(el, &raw mut wc) }, 0);
    assert_eq!(wc, 0xdead);
    done(el);
}

/// A NULL string is a beep and nothing else: no slot is taken, and the
/// caller is told nothing at all.
#[test]
fn pushing_nothing_is_reported_only_to_the_user() {
    let el = editline();
    // SAFETY: `el` is live; `sem:histedit.el-wpush-fn` allows a NULL
    // string and defines it as the failure path.
    unsafe { &mut *el }.el_terminal.t_str[1] = None;
    let out = output_of(el, "push", |el| unsafe {
        el_wpush(el, core::ptr::null());
    });
    assert_eq!(out, [0x07]);
    // SAFETY: `el` is live.
    let ma = unsafe { &(*el).el_read.as_ref().unwrap().macros };
    assert!(ma.r#macro.is_empty());
    assert_eq!(ma.level, -1, "the level increment is undone");
    done(el);
}

// -----------------------------------------------------------------
// el_wset / el_wget
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-wset-fn/test]
// [spec:libedit:sem:el.el-wset-fn/test]
// [spec:libedit:sem:histedit.el-wget-fn/test]
// [spec:libedit:sem:el.el-wget-fn/test]
/// The three flag ops report their bit rather than a boolean, and only one
/// of the three normalises.
///
/// `EL_SIGNAL` looks like it round-trips, and it does — but only because
/// `HANDLE_SIGNALS` happens to be 0x001. `EL_SAFEREAD` is the same code
/// over `FIXIO`, which is 0x100, so `el_wget` answers **256** and a caller
/// that compares it against 1 concludes the flag is off. Frozen
/// behaviour, deliberately not normalised. `EL_UNBUFFERED` and
/// `EL_EDITMODE` do normalise, which is what makes the pair inconsistent
/// rather than merely raw.
#[test]
fn the_flag_getters_report_bits_where_the_c_reported_bits() {
    let el = editline();
    let mut out: c_int = -1;
    // SAFETY: each op is called with exactly the argument its rule
    // defines, against a live handle.
    unsafe {
        assert_eq!(el_wset(el, EL_SIGNAL, 1 as c_int), 0);
        assert_eq!(el_wget(el, EL_SIGNAL, &raw mut out), 0);
        assert_eq!(out, 1, "HANDLE_SIGNALS is 0x001, so the raw bit is 1");

        assert_eq!(el_wset(el, EL_SAFEREAD, 1 as c_int), 0);
        assert_eq!(el_wget(el, EL_SAFEREAD, &raw mut out), 0);
        assert_eq!(out, 256, "the raw FIXIO bit, not a boolean");

        assert_eq!(el_wset(el, EL_UNBUFFERED, 7 as c_int), 0);
        assert_eq!(el_wget(el, EL_UNBUFFERED, &raw mut out), 0);
        assert_eq!(out, 1, "normalised, unlike the two above");

        // Inverted: a zero `EL_EDITMODE` raises EDIT_DISABLED.
        assert_eq!(el_wset(el, EL_EDITMODE, 0 as c_int), 0);
        assert_eq!(el_wget(el, EL_EDITMODE, &raw mut out), 0);
        assert_eq!(out, 0);
        assert_ne!((*el).el_flags & EDIT_DISABLED, 0);
    }
    done(el);
}

/// Client data is stored and handed back with no interpretation at all,
/// and an op the dispatch does not know is -1 having read nothing.
#[test]
fn client_data_round_trips_and_an_unknown_op_reads_no_arguments() {
    let el = editline();
    let cookie = 0xfeed_face_usize as *mut c_void;
    let mut back: *mut c_void = core::ptr::null_mut();
    // SAFETY: as above.
    unsafe {
        assert_eq!(el_wset(el, EL_CLIENTDATA, cookie), 0);
        assert_eq!(el_wget(el, EL_CLIENTDATA, &raw mut back), 0);
        assert_eq!(back, cookie);

        // `EL_REFRESH` has no `el_wget` arm, and neither has an op code
        // outside the table. Both are -1 with the tail untouched, which is
        // why passing no argument at all here is well defined.
        assert_eq!(el_wget(el, EL_REFRESH), -1);
        assert_eq!(el_wget(el, 999), -1);
        assert_eq!(el_wset(el, 999), -1);
    }
    done(el);
}

/// `EL_EDITOR` accepts exactly two spellings and hands back the library's
/// own statics; a NULL argument is rejected rather than dereferenced.
#[test]
fn the_editor_op_answers_with_its_own_literals() {
    let el = editline();
    let vi = wcs("vi");
    let junk = wcs("ed");
    let mut out: *const u32 = core::ptr::null();
    // SAFETY: as above; each string is NUL-terminated and outlives the
    // call.
    unsafe {
        assert_eq!(el_wset(el, EL_EDITOR, vi.as_ptr()), 0);
        assert_eq!(el_wget(el, EL_EDITOR, &raw mut out), 0);
        assert!(!out.is_null());
        assert_eq!(wstr(out), Some(&wcs("vi")[..2]));

        assert_eq!(el_wset(el, EL_EDITOR, junk.as_ptr()), -1);
        // ERR-core-api-08, disposition `define — reject NULL`: the C hands
        // this straight to `wcscmp`.
        assert_eq!(el_wset(el, EL_EDITOR, core::ptr::null::<u32>()), -1);
        // The rejected calls changed nothing.
        assert_eq!(el_wget(el, EL_EDITOR, &raw mut out), 0);
        assert_eq!(wstr(out), Some(&wcs("vi")[..2]));
    }
    done(el);
}

/// The word-character set comes back NUL-terminated, and `EL_EDITOR`
/// resets it — which is the coupling between the two ops that makes
/// setting the editor after the word characters lose them.
#[test]
fn the_word_characters_survive_a_round_trip_and_an_editor_switch_resets_them() {
    let el = editline();
    let chars = wcs("_-");
    let emacs = wcs("emacs");
    let mut out: *const u32 = core::ptr::null();
    // SAFETY: as above.
    unsafe {
        assert_eq!(el_wset(el, EL_WORDCHARS, chars.as_ptr()), 0);
        assert_eq!(el_wget(el, EL_WORDCHARS, &raw mut out), 0);
        assert_eq!(wstr(out), Some(&wcs("_-")[..2]));

        assert_eq!(el_wset(el, EL_EDITOR, emacs.as_ptr()), 0);
        assert_eq!(el_wget(el, EL_WORDCHARS, &raw mut out), 0);
        assert_ne!(wstr(out), Some(&wcs("_-")[..2]));

        assert_eq!(el_wset(el, EL_WORDCHARS, core::ptr::null::<u32>()), -1);
    }
    done(el);
}

/// A prompt installed through `EL_PROMPT_ESC` cannot be read back through
/// `EL_PROMPT_ESC`.
///
/// ERR-core-api-14, frozen: `prompt_set` counts `EL_PROMPT_ESC` as the
/// left-hand prompt and `prompt_get` does not, so the setter writes the
/// left record and the getter reads the right one. The consequence is that
/// `el_prompt.p_ignore` has no route out of the library at all — plain
/// `EL_PROMPT` passes a NULL escape-character pointer.
#[test]
fn the_escape_form_of_the_prompt_op_does_not_round_trip() {
    unsafe extern "C" fn never_called(_: *mut EditLine) -> *mut u32 {
        core::ptr::null_mut()
    }

    let el = editline();
    let installed: ElPfuncT = never_called;
    let mut left: Option<ElPfuncT> = None;
    let mut right: Option<ElPfuncT> = None;
    let mut esc: u32 = 0xffff;
    // SAFETY: as above; `never_called` has the `el_pfunc_t` shape and is
    // never invoked, since nothing here draws a prompt.
    unsafe {
        assert_eq!(
            el_wset(el, EL_PROMPT_ESC, installed, c_int::from(b'\x01')),
            0
        );
        assert_eq!(el_wget(el, EL_PROMPT, &raw mut left), 0);
        assert_eq!(el_wget(el, EL_PROMPT_ESC, &raw mut right, &raw mut esc), 0);
    }
    assert!(
        left.is_some_and(|f| core::ptr::fn_addr_eq(f, installed)),
        "the setter wrote the LEFT prompt"
    );
    assert!(
        !right.is_some_and(|f| core::ptr::fn_addr_eq(f, installed)),
        "the getter read the RIGHT one"
    );
    assert_eq!(
        esc, 0,
        "so the escape character read back is the right \
                        prompt's, never the one just installed"
    );
    done(el);
}

/// `EL_GETENV` reports an address for a handle nobody has configured, and
/// installing that address back is a no-op rather than an application
/// hook.
///
/// The C stores `secure_getenv` itself at construction, so its
/// `el_get(EL_GETENV)` is never NULL; the core keeps `None` for that state
/// because its `secure_getenv` returns an owned `OsString` and cannot be a
/// `func_t`. `default_getenv` is the address invented to close that gap,
/// and the round trip is what stops it becoming a real hook.
#[test]
fn the_environment_accessor_of_a_fresh_handle_is_an_address_not_null() {
    let el = editline();
    let mut hook: Option<FuncT> = None;
    // SAFETY: as above.
    unsafe {
        assert_eq!(el_wget(el, EL_GETENV, &raw mut hook), 0);
        let reported = hook.expect("never NULL, unlike the core's `None`");
        assert_eq!(el_wset(el, EL_GETENV, reported), 0);
        assert!(
            (*el).el_getenv.is_none(),
            "installing the reported default must not arm an indirect call"
        );
        // ERR-core-api-08: a NULL hook leaves the built-in in force.
        assert_eq!(el_wset(el, EL_GETENV, core::ptr::null::<c_void>()), 0);
        assert!((*el).el_getenv.is_none());
    }
    done(el);
}

/// `EL_GETFP` hands back the streams the handle was built with, `EL_SETFP`
/// replaces one, and both read their whole tail before validating `what` —
/// so a rejected `what` has still walked past the caller's second
/// argument.
///
/// Only the streams travel: there is no op that reads a descriptor back,
/// even though `EL_SETFP` derives one from every stream it installs.
#[test]
fn the_stream_ops_carry_the_streams_and_nothing_else() {
    // Sentinels, not real `FILE *`s: nothing on this path dereferences a
    // stream except `EL_SETFP`'s `fileno`, which is only reached below
    // with NULL.
    let err = 0x1000_usize as CFile;
    let el = editline_with(core::ptr::null_mut(), core::ptr::null_mut(), err);
    let mut back: *mut c_void = core::ptr::null_mut();
    // SAFETY: each op is called with exactly the arguments its rule
    // defines, against a live handle.
    unsafe {
        assert_eq!(el_wget(el, EL_GETFP, 2 as c_int, &raw mut back), 0);
        assert_eq!(back, err, "the constructor's stream, unmodified");

        // A NULL stream is the one `EL_SETFP` argument that can be tested
        // without a real one: `fileno_of` answers -1 for it rather than
        // dereferencing, which is the descriptor the C stores for a stream
        // that has none (ERR-core-api-08).
        assert_eq!(
            el_wset(el, EL_SETFP, 2 as c_int, core::ptr::null::<c_void>()),
            0
        );
        assert_eq!((*el).el_errfd, -1);
        assert_eq!(el_wget(el, EL_GETFP, 2 as c_int, &raw mut back), 0);
        assert!(back.is_null());

        // `what` is validated only after both varargs have been read, so
        // these leave the caller's storage alone but not the tail.
        back = err;
        assert_eq!(el_wget(el, EL_GETFP, 3 as c_int, &raw mut back), -1);
        assert_eq!(back, err, "a rejected `what` writes nothing");
        assert_eq!(
            el_wset(el, EL_SETFP, 3 as c_int, core::ptr::null::<c_void>()),
            -1
        );
    }
    done(el);
}

// -----------------------------------------------------------------
// _el_fn_complete / _el_fn_sh_complete
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.el-fn-complete-fn/test]
// [spec:libedit:sem:histedit.el-fn-sh-complete-fn/test]
/// The two exported completion commands are one behaviour under two
/// symbols, and the ABI needs both to stay exported and to stay
/// indistinguishable.
///
/// ERR-completion-23: `_el_fn_sh_complete` forwards both arguments and
/// returns the result unchanged, with nothing shell-specific about it
/// despite the name. Running the same completion twice — once through each
/// symbol, from the same starting line — is the only way to assert that
/// rather than trusting the forwarding to stay a forward.
///
/// The completion itself is a unique-match one, deliberately: a single
/// candidate is the one case that neither lists nor prompts, so nothing is
/// written to the output stream and the `Display all N possibilities?`
/// read from C `stdin` — which is a real defect on any handle not driven
/// from stdin — is not reached.
#[test]
fn the_two_completion_commands_are_one_behaviour_under_two_symbols() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nshedit-complete-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("uniquetarget"), b"").unwrap();
    let prefix = CString::new(format!("{}/unique", dir.display())).unwrap();
    let want = format!("{}/uniquetarget ", dir.display());

    let drive = |f: unsafe extern "C" fn(*mut EditLine, c_int) -> c_uchar| -> (u8, String) {
        let el = editline();
        // SAFETY: `el` is live and `prefix` is NUL-terminated.
        assert_eq!(unsafe { el_insertstr(el, prefix.as_ptr()) }, 0);
        // SAFETY: `el` is live.
        let e = unsafe { &mut *el };
        // The driver treats a repeat of the same command as "list the
        // possibilities"; these must differ for it to complete.
        e.el_state.lastcmd = 1;
        e.el_state.thiscmd = 2;
        // SAFETY: `el` is live; the second argument is ignored.
        let rv = unsafe { f(el, 0) };
        // SAFETY: `el` is live.
        let e = unsafe { &*el };
        let line: String = e.el_line.buffer[..e.el_line.lastchar]
            .iter()
            .filter_map(|&c| char::from_u32(c))
            .collect();
        done(el);
        (rv, line)
    };

    let plain = drive(_el_fn_complete);
    assert_eq!(plain.1, want, "the unique match, with the appended space");
    assert_eq!(drive(_el_fn_sh_complete), plain);

    let _ = std::fs::remove_dir_all(&dir);
}

// -----------------------------------------------------------------
// tok_line
// -----------------------------------------------------------------

// [spec:libedit:sem:histedit.tok-line-fn/test]
/// The narrow tokenizer over a `LineInfo`: quoting, the cursor report, and
/// the fact that it appends rather than resetting.
///
/// `tok_line` not resetting is the property the rule leads with and the
/// one a caller gets wrong: it is what makes multi-line continuation work
/// and what makes a second line silently extend the first if `tok_reset`
/// is forgotten.
#[test]
fn tokenizing_a_line_appends_to_whatever_was_there() {
    // SAFETY: NULL selects the default IFS.
    let tok = unsafe { tok_init(core::ptr::null()) };
    assert!(!tok.is_null());

    let text = CString::new("  foo 'bar baz' qux").unwrap();
    let words = |argc: c_int, argv: *mut *const c_char| -> Vec<String> {
        (0..argc as usize)
            .map(|i| {
                // SAFETY: the tokenizer published `argc` NUL-terminated
                // words and they are live until the next call.
                let p = unsafe { *argv.add(i) };
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            })
            .collect()
    };

    let base = text.as_ptr();
    let info = LineInfo {
        buffer: base,
        // Inside the quoted word, one character in.
        // SAFETY: both offsets are within `text`.
        cursor: unsafe { base.add(8) },
        lastchar: unsafe { base.add(text.as_bytes().len()) },
    };
    let mut argc: c_int = -1;
    let mut argv: *mut *const c_char = core::ptr::null_mut();
    let mut cursorc: c_int = -1;
    let mut cursoro: c_int = -1;
    // SAFETY: `tok` and `info` are live and the four out-parameters are
    // writable.
    let rv = unsafe {
        tok_line(
            tok,
            &info,
            &raw mut argc,
            &raw mut argv,
            &raw mut cursorc,
            &raw mut cursoro,
        )
    };
    assert_eq!(rv, 0);
    assert_eq!(words(argc, argv), ["foo", "bar baz", "qux"]);
    assert_eq!((cursorc, cursoro), (1, 1), "one character into word 1");

    // The same tokenizer again, with no `tok_reset` between: the second
    // line extends the first rather than replacing it.
    let more = CString::new("quux").unwrap();
    let base = more.as_ptr();
    let info = LineInfo {
        buffer: base,
        cursor: base,
        // SAFETY: within `more`.
        lastchar: unsafe { base.add(more.as_bytes().len()) },
    };
    // SAFETY: as above; the two cursor out-parameters are optional and
    // NULL is the documented way to decline them.
    let rv = unsafe {
        tok_line(
            tok,
            &info,
            &raw mut argc,
            &raw mut argv,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    assert_eq!(rv, 0);
    assert_eq!(words(argc, argv), ["foo", "bar baz", "qux", "quux"]);

    // SAFETY: `tok` came from `tok_init` and is not used again.
    unsafe { tok_end(tok) };
}
