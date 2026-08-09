use core::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, AtomicUsize, Ordering::Relaxed};
use std::sync::{Mutex, MutexGuard};

use nshedit::domain::Text;

use crate::cdecl::readline::RlCommandFuncT;

use super::*;

/// Everything below reaches the same sixty exported statics and the same
/// process-global editor, so no two of these may run at once — and
/// `cargo test` runs them on a thread pool by default. Serialising here
/// rather than with `--test-threads=1` keeps the constraint in the file
/// that has it.
///
/// A failing test poisons the mutex; the poison is discarded because the
/// state each test needs is the state it sets itself on the way in, and
/// letting one failure cascade into a dozen misattributed ones hides the
/// real defect.
static GLOBALS: Mutex<()> = Mutex::new(());

fn globals() -> MutexGuard<'static, ()> {
    GLOBALS.lock().unwrap_or_else(|e| e.into_inner())
}

// -----------------------------------------------------------------------
// Pure functions; no global state, so no lock.
// -----------------------------------------------------------------------

/// The exported comparator delegates to `strcoll`, whose result promises a
/// sign rather than a particular negative or positive magnitude.
///
/// Nothing in libedit calls it: `rl_completion_matches` sorts through a
/// cast `strcmp` instead, which is ERR-readline-01. It is tested here
/// because a consumer that reaches into readline's private namespace for
/// it — the reason it is exported at all — gets this and not the sort
/// libedit actually performs.
// [spec:libedit:sem:readline.rl-qsort-string-compare-fn/test]
#[test]
fn unused_comparator_uses_strcoll() {
    // `qsort` hands a comparator the *addresses* of the elements, which
    // is why both parameters are `char **` and both are dereferenced.
    let mut apple = c"apple".as_ptr().cast_mut();
    let mut banana = c"banana".as_ptr().cast_mut();
    let mut apples = c"apples".as_ptr().cast_mut();
    let mut upper = c"Zebra".as_ptr().cast_mut();

    // SAFETY: every argument points at a live `*mut c_char` holding a
    // NUL-terminated string.
    unsafe {
        assert!(_rl_qsort_string_compare(&raw mut apple, &raw mut banana) < 0);
        assert!(_rl_qsort_string_compare(&raw mut banana, &raw mut apple) > 0);
        assert_eq!(_rl_qsort_string_compare(&raw mut apple, &raw mut apple), 0);

        // A prefix sorts before what extends it.
        assert!(_rl_qsort_string_compare(&raw mut apple, &raw mut apples) < 0);
        assert!(_rl_qsort_string_compare(&raw mut apples, &raw mut apple) > 0);

        // The test process starts in the C locale, where byte ordering puts
        // uppercase ASCII before lowercase. Other locales are covered by the
        // differential conformance matrix.
        assert!(_rl_qsort_string_compare(&raw mut upper, &raw mut apple) < 0);
    }
}

/// `free_history_entry` frees nothing and always answers NULL, so the
/// documented readline idiom `free_history_entry(remove_history(i))`
/// leaks the entry, its line, and the caller's `histdata_t` with it
/// (ERR-readline-15). Reproduced deliberately: freeing here would turn
/// the leak into a double free in programs that already free the entry.
// [spec:libedit:sem:readline.free-history-entry-fn/test]
#[test]
fn freeing_a_history_entry_frees_nothing_and_never_returns_the_data() {
    let line = c"cd /tmp";
    let mut payload: c_int = 0x5eed;
    let mut he = HistEntry {
        line: line.as_ptr(),
        data: (&raw mut payload).cast::<c_void>(),
    };

    // SAFETY: `he` is a live entry; the callee reads neither member.
    let out = unsafe { free_history_entry(&raw mut he) };

    // GNU readline hands back `he->data` so the caller can release it.
    // Here it is NULL whether or not the entry carried one, which is the
    // half of the leak a caller could otherwise have noticed.
    assert!(out.is_null());
    assert!(!he.data.is_null());

    // Both members survive the call untouched — the whole C body is
    // `return he ? NULL : NULL;`.
    assert_eq!(he.line, line.as_ptr());
    assert_eq!(payload, 0x5eed);

    // The `he ?` arm is decoration: NULL in, NULL out.
    // SAFETY: NULL is the case the C's conditional exists for.
    assert!(unsafe { free_history_entry(ptr::null_mut()) }.is_null());
}

/// The timeout is neither stored nor reported. GNU readline returns the
/// *previous* value, so the save-and-restore idiom
/// `old = rl_set_keyboard_input_timeout(n); ...;
/// rl_set_keyboard_input_timeout(old)` restores 0 here — "no wait at
/// all" — instead of what was configured (ERR-readline-54).
// [spec:libedit:sem:readline.rl-set-keyboard-input-timeout-fn/test]
#[test]
fn the_keyboard_timeout_is_never_stored_and_never_reported_back() {
    // SAFETY: the stub touches nothing.
    unsafe {
        assert_eq!(rl_set_keyboard_input_timeout(100_000), 0);
        // This is the call GNU readline would answer 100000 to.
        assert_eq!(rl_set_keyboard_input_timeout(50), 0);
        assert_eq!(rl_set_keyboard_input_timeout(0), 0);
        assert_eq!(rl_set_keyboard_input_timeout(-1), 0);
    }
}

// -----------------------------------------------------------------------
// The exported globals
// -----------------------------------------------------------------------

/// `rl_abort` aborts nothing: the C body is `count && key ? 0 : 0`, so
/// every argument gives 0, no bell is rung and `rl_done` is left alone.
/// `_rl_abort_internal` is the one that acts, despite the name
/// (ERR-readline-54).
// [spec:libedit:sem:readline.rl-abort-fn/test]
#[test]
fn rl_abort_is_inert_and_leaves_rl_done_alone() {
    let _g = globals();
    // SAFETY: single-threaded under the lock above.
    unsafe {
        rl_done = 0;
        assert_eq!(rl_abort(0, 0), 0);
        assert_eq!(rl_abort(1, c_int::from(b'\x07')), 0);
        assert_eq!(rl_abort(-1, c_int::MAX), 0);
        let done = rl_done;
        assert_eq!(done, 0);
    }
}

/// The C's `_rl_abort_internal` ends in `longjmp(topbuf, 1)` and never
/// returns; with no `readline()` frame live that jump lands in a dead
/// frame (ERR-readline-10). Rust has no `longjmp`, so the port raises the
/// flag `readline()` consumes and ends the editor loop through `rl_done`
/// — which is what this pins, since the flag is the whole substitute
/// mechanism.
// [spec:libedit:sem:readline.rl-abort-internal-fn/test]
#[test]
fn the_internal_abort_raises_a_flag_where_the_c_jumps() {
    let _g = globals();
    // SAFETY: single-threaded under the lock. Clearing the runtime first means
    // the `el_beep` the C issues with no guard is skipped.
    unsafe {
        release_runtime_session();
        rl_done = 0;
        READLINE_RUNTIME.abort_pending.store(false, Relaxed);

        // The C's declared `int` is never produced — it cannot return at
        // all — so 0 is the port's own answer.
        assert_eq!(_rl_abort_internal(), 0);

        let done = rl_done;
        assert_eq!(done, 1);
        // Raised with no `readline()` running it simply waits to be
        // consumed by the next one, which is the port's definition of the
        // C's dead jump.
        assert!(READLINE_RUNTIME.abort_pending.swap(false, Relaxed));

        rl_done = 0;
    }
}

/// An empty stub. The name promises the line is gone; nothing about it
/// changes, so a program that clears the display this way and then reads
/// `rl_line_buffer` still sees the old text (ERR-readline-54).
// [spec:libedit:sem:readline.rl-erase-entire-line-fn/test]
#[test]
fn erasing_the_entire_line_erases_nothing() {
    let _g = globals();
    let mut line = *b"echo hi\0";
    // SAFETY: single-threaded under the lock. `rl_line_buffer` is put
    // back to NULL before `line` goes out of scope.
    unsafe {
        let saved = rl_line_buffer;
        rl_line_buffer = line.as_mut_ptr().cast::<c_char>();
        rl_point = 4;
        rl_end = 7;

        _rl_erase_entire_line();

        let (point, end) = (rl_point, rl_end);
        assert_eq!(point, 4);
        assert_eq!(end, 7);
        assert_eq!(&line, b"echo hi\0");

        rl_line_buffer = saved;
        rl_point = 0;
        rl_end = 0;
    }
}

/// The kill reaches EditLine's own kill buffer — recoverable with
/// `em-yank`, not with readline's kill ring, which this layer has no
/// trace of — and none of the position globals is republished
/// afterwards, so `rl_point` and `rl_end` go on describing the line that
/// was just killed until something else refreshes them.
///
// [spec:libedit:sem:readline.rl-kill-full-line-fn/test]
#[test]
fn full_line_kill_keeps_position_globals() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock; the runtime owns the fixture's
    // editor.
    unsafe {
        assert_eq!(
            crate::eln::el_insertstr(runtime_editor(), c"some text".as_ptr()),
            0
        );
        assert_eq!((&*runtime_editor()).editor().line().len(), 9);
        rl_point = 3;
        rl_end = 9;

        // Both parameters are unused, which is why any values do.
        assert_eq!(rl_kill_full_line(7, c_int::from(b'u')), 0);

        let (point, end) = (rl_point, rl_end);
        assert_eq!(point, 3);
        assert_eq!(end, 9);
        assert_eq!((&*runtime_editor()).editor().cursor().get(), 0);
        assert!((&*runtime_editor()).editor().line().is_empty());
        assert_eq!(
            (&*runtime_editor()).editor().kill_buffer(),
            Some(&Text::from("some text"))
        );

        rl_point = 0;
        rl_end = 0;
    }
}

// -----------------------------------------------------------------------
// The terminal hooks
// -----------------------------------------------------------------------

static PREP_CALLS: AtomicUsize = AtomicUsize::new(0);
static PREP_LAST_ARG: AtomicI32 = AtomicI32::new(c_int::MIN);

/// Stands in for the application's `rl_prep_term_function` so the calls
/// through it can be counted.
unsafe extern "C" fn recording_prep_term(meta_flag: c_int) {
    PREP_CALLS.fetch_add(1, Relaxed);
    PREP_LAST_ARG.store(meta_flag, Relaxed);
}

/// `rl_cleanup_after_signal` is an empty stub while its conventional
/// partner `rl_reset_after_signal` does call the prep hook. A signal
/// handler written against GNU readline — cleanup on the way in, reset on
/// the way out — therefore performs only the second half here, and gets
/// no terminal restoration before the signal's default action runs.
// [spec:libedit:sem:readline.rl-cleanup-after-signal-fn/test]
#[test]
fn cleanup_after_signal_never_reaches_the_terminal_hook() {
    let _g = globals();
    // SAFETY: single-threaded under the lock; the hook is restored below.
    unsafe {
        let saved = rl_prep_term_function;
        rl_prep_term_function = Some(recording_prep_term);
        PREP_CALLS.store(0, Relaxed);

        rl_cleanup_after_signal();
        assert_eq!(PREP_CALLS.load(Relaxed), 0);

        // The asymmetry, in the same breath so it cannot drift: the same
        // hook, the same state, and the reset half does call it.
        rl_reset_after_signal();
        assert_eq!(PREP_CALLS.load(Relaxed), 1);

        rl_prep_term_function = saved;
    }
}

/// `rl_reset_after_signal` calls `rl_prep_term_function(1)` and nothing
/// else: the display is not repainted and the line is not redrawn. The
/// argument is readline's `meta_flag`, hardcoded — an application's own
/// hook can never learn what the caller wanted.
// [spec:libedit:sem:readline.rl-reset-after-signal-fn/test]
#[test]
fn reset_after_signal_preps_the_terminal_through_the_exported_hook() {
    let _g = globals();
    // SAFETY: single-threaded under the lock; the hook is restored below.
    unsafe {
        let saved = rl_prep_term_function;

        rl_prep_term_function = Some(recording_prep_term);
        PREP_CALLS.store(0, Relaxed);
        PREP_LAST_ARG.store(c_int::MIN, Relaxed);

        rl_reset_after_signal();
        assert_eq!(PREP_CALLS.load(Relaxed), 1);
        assert_eq!(PREP_LAST_ARG.load(Relaxed), 1);

        // An application may clear the hook; the NULL test is the C's own
        // and this must stay silent rather than falling back to the
        // default `rl_prep_terminal`.
        rl_prep_term_function = None;
        rl_reset_after_signal();
        assert_eq!(PREP_CALLS.load(Relaxed), 1);

        rl_prep_term_function = saved;
    }
}

// -----------------------------------------------------------------------
// The EditLine-facing callbacks
// -----------------------------------------------------------------------

/// Every call marks the prompt as emitted. `readline()` clears
/// `rl_already_prompted` immediately before `el_gets`, so an application
/// reading it inside a completion or event hook learns whether libedit
/// has drawn the prompt for the line being edited.
// [spec:libedit:sem:readline.get-prompt-fn/test]
#[test]
fn asking_for_the_prompt_marks_it_as_already_emitted() {
    let _g = globals();
    let mut prompt = *b"$ \0";
    // SAFETY: single-threaded under the lock. `rl_prompt` is restored
    // before `prompt` goes out of scope.
    unsafe {
        let saved = rl_prompt;
        rl_prompt = prompt.as_mut_ptr().cast::<c_char>();
        rl_already_prompted = 0;

        // The editor argument is unused, so NULL is as good as a live
        // one: nothing here reaches the editor.
        let p = _get_prompt(ptr::null_mut());

        // Borrowed, not copied — libedit must not free it, and
        // `rl_set_prompt` may replace it before the next call.
        let (current, prompted) = (rl_prompt, rl_already_prompted);
        assert_eq!(p, current);
        assert_eq!(prompted, 1);

        // NULL is passed straight through: `rl_prompt` is NULL until
        // `rl_set_prompt` has succeeded once, and libedit is handed that
        // NULL rather than an empty string.
        rl_prompt = ptr::null_mut();
        rl_already_prompted = 0;
        assert!(_get_prompt(ptr::null_mut()).is_null());
        let prompted = rl_already_prompted;
        assert_eq!(prompted, 1);

        rl_prompt = saved;
        rl_already_prompted = 0;
    }
}

static GETC_CALLS: AtomicUsize = AtomicUsize::new(0);
static GETC_ANSWER: AtomicI32 = AtomicI32::new(0);
static GETC_STREAM: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());

/// Stands in for the application's `rl_getc_function`, recording which
/// stream it was handed and answering whatever the test staged.
unsafe extern "C" fn recording_getc(f: CFile) -> c_int {
    GETC_CALLS.fetch_add(1, Relaxed);
    GETC_STREAM.store(f.cast::<c_void>(), Relaxed);
    GETC_ANSWER.load(Relaxed)
}

/// The `EL_GETCFN` adapter widens the hook's `int` straight to a wide
/// character with no multibyte decoding, so one call is exactly one wide
/// character and a byte-oriented hook ported from GNU readline produces
/// mojibake under a UTF-8 locale (ERR-readline-32). Only *exactly* -1 is
/// end of input; any other negative value is stored as a character, where
/// it goes on to index the dispatch table out of range (ERR-input-44).
// [spec:libedit:sem:readline.getc-function-fn/test]
#[test]
fn the_getc_adapter_widens_one_int_and_only_minus_one_is_eof() {
    let _g = globals();
    let mut c: u32 = 0xdead;
    // SAFETY: single-threaded under the lock; both globals are restored
    // below. The sentinel stream is only ever compared, never read.
    unsafe {
        let saved_hook = rl_getc_function;
        let saved_stream = rl_instream;

        rl_getc_function = Some(recording_getc);
        // The hook is handed `rl_instream` as it stands at the moment of
        // the call, not the stream the editor was built on, so
        // redirecting it after `rl_initialize` redirects the hook too.
        let sentinel = ptr::without_provenance_mut::<c_void>(0x5f11e);
        rl_instream = sentinel;
        GETC_CALLS.store(0, Relaxed);

        GETC_ANSWER.store(c_int::from(b'q'), Relaxed);
        assert_eq!(_getc_function(ptr::null_mut(), &raw mut c), 1);
        assert_eq!(c, u32::from(b'q'));
        assert_eq!(GETC_CALLS.load(Relaxed), 1);
        assert_eq!(GETC_STREAM.load(Relaxed), sentinel);

        // Exactly -1 ends input, and the out parameter is left as it was
        // rather than being cleared.
        c = 0xbeef;
        GETC_ANSWER.store(-1, Relaxed);
        assert_eq!(_getc_function(ptr::null_mut(), &raw mut c), 0);
        assert_eq!(c, 0xbeef);

        // -2 is a character. On the C's signed `wchar_t` this is a
        // negative wide character, which `read_getcmd`'s `>= N_KEYS`
        // guard does not exclude.
        GETC_ANSWER.store(-2, Relaxed);
        assert_eq!(_getc_function(ptr::null_mut(), &raw mut c), 1);
        assert_eq!(c, (-2i32) as u32);

        // A UTF-8 lead byte becomes a whole wide character of its own
        // rather than the start of a sequence: no decoding happens.
        GETC_ANSWER.store(0xce, Relaxed);
        assert_eq!(_getc_function(ptr::null_mut(), &raw mut c), 1);
        assert_eq!(c, 0xce);

        // The C dereferences the hook with no NULL check, so an
        // application that clears it after `rl_initialize` calls NULL.
        // The port defines that as end of input.
        rl_getc_function = None;
        c = 0xbeef;
        assert_eq!(_getc_function(ptr::null_mut(), &raw mut c), 0);
        assert_eq!(c, 0xbeef);

        rl_getc_function = saved_hook;
        rl_instream = saved_stream;
    }
}

static EVENT_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Stands in for the application's `rl_event_hook`.
unsafe extern "C" fn recording_event_hook() -> c_int {
    EVENT_CALLS.fetch_add(1, Relaxed);
    0
}

/// The event reader clears its out parameter, then runs the application
/// hook *before* it attempts a read — that ordering is the whole point of
/// the hook, and it is why an application gets called even when no input
/// is pending. With no hook installed the loop never runs at all and the
/// descriptor is never touched.
// [spec:libedit:sem:readline.rl-event-read-char-fn/test]
#[test]
fn the_event_reader_runs_the_hook_before_it_reads() {
    let _g = globals();
    let mut wc: u32 = 0xdead;
    // SAFETY: single-threaded under the lock; the NULL editor is the
    // port's own guard, where the C would dereference it.
    unsafe {
        let saved = rl_event_hook;

        // No hook: the `while` never executes, so nothing is read and the
        // cleared out parameter is what EditLine sees.
        rl_event_hook = None;
        assert_eq!(_rl_event_read_char(ptr::null_mut(), &raw mut wc), 0);
        assert_eq!(wc, 0);

        // With one, the hook runs first; the NULL editor stops the port
        // where the read would be, so this counts exactly the hook calls
        // that precede the first read attempt.
        rl_event_hook = Some(recording_event_hook);
        EVENT_CALLS.store(0, Relaxed);
        wc = 0xdead;
        assert_eq!(_rl_event_read_char(ptr::null_mut(), &raw mut wc), -1);
        assert_eq!(EVENT_CALLS.load(Relaxed), 1);
        assert_eq!(wc, 0);

        rl_event_hook = saved;
    }
}

/// The adapter projects the C append byte into the core's UTF-8 string type.
/// ASCII survives directly; zero and bytes without a one-byte UTF-8 spelling
/// mean "append nothing".
// [spec:libedit:sem:readline.rl-completion-append-character-function-fn/test]
#[test]
fn append_character_projects_into_core_text() {
    let _g = globals();
    // SAFETY: single-threaded under the lock; the global is restored.
    unsafe {
        let saved = rl_completion_append_character;

        rl_completion_append_character = c_int::from(b' ');
        assert_eq!(readline_completion_suffix(""), " ");

        // 0 is readline's "append nothing". It gives the empty string
        // rather than a NULL, which is the input that makes the core's
        // filename escaping produce an embedded NUL (ERR-completion-10).
        rl_completion_append_character = 0;
        assert_eq!(readline_completion_suffix(""), "");

        // U+03BB would truncate to the invalid lone byte 0xBB in the C
        // helper. The core accepts only valid text, so the ABI adapter drops
        // that unrepresentable append value.
        rl_completion_append_character = 0x3bb;
        assert_eq!(readline_completion_suffix(""), "");

        rl_completion_append_character = saved;
    }
}

static BOUND_COUNT: AtomicI32 = AtomicI32::new(-1);
static BOUND_KEY: AtomicI32 = AtomicI32::new(-1);
static BOUND_SETS_DONE: AtomicBool = AtomicBool::new(false);
static REENTRANT_BIND_RESULT: AtomicI32 = AtomicI32::new(c_int::MIN);

/// Stands in for an application command installed by `rl_add_defun`.
unsafe extern "C" fn recording_command(count: c_int, key: c_int) -> c_int {
    BOUND_COUNT.store(count, Relaxed);
    BOUND_KEY.store(key, Relaxed);
    if BOUND_SETS_DONE.load(Relaxed) {
        // SAFETY: single-threaded module state; the caller holds the lock.
        unsafe { rl_done = 1 };
    }
    // A failure return, which the wrapper discards.
    -1
}

unsafe extern "C" fn reentrant_binding_command(_: c_int, _: c_int) -> c_int {
    let result = unsafe {
        rl_add_defun(
            c"reentrant-command".as_ptr(),
            Some(recording_command),
            c_int::from(b'y'),
        )
    };
    REENTRANT_BIND_RESULT.store(result, Relaxed);
    result
}

/// The wrapper is what stands between EditLine's keystroke dispatch and a
/// `rl_command_func_t` installed by `rl_add_defun`. It hardcodes readline's
/// `count` to 1, so a command bound this way can never see a numeric
/// argument, and it throws the command's return value away — leaving
/// `rl_done` as the only channel a readline command has for reporting
/// anything at all.
// [spec:libedit:sem:readline.rl-bind-wrapper-fn/test]
#[test]
fn the_bind_wrapper_hardcodes_a_count_of_one_and_discards_the_result() {
    let _g = globals();
    let key = b'x';
    // SAFETY: single-threaded under the lock; the table slot and
    // `rl_done` are restored below. `_rl_update_pos` returns early on the
    // NULL editor, so no line state is needed.
    unsafe {
        let saved = READLINE_RUNTIME.access(|runtime| runtime.commands[key as usize]);

        // An unbound byte is CC_ERROR and the table is not consulted
        // further — this is the only failure the wrapper can report.
        READLINE_RUNTIME.access(|runtime| runtime.commands[key as usize] = None);
        assert_eq!(rl_bind_wrapper(ptr::null_mut(), key), CC_ERROR);

        READLINE_RUNTIME.access(|runtime| runtime.commands[key as usize] = Some(recording_command));
        BOUND_COUNT.store(-1, Relaxed);
        BOUND_KEY.store(-1, Relaxed);
        BOUND_SETS_DONE.store(false, Relaxed);
        rl_done = 0;

        assert_eq!(rl_bind_wrapper(ptr::null_mut(), key), CC_NORM);
        assert_eq!(BOUND_COUNT.load(Relaxed), 1);
        assert_eq!(BOUND_KEY.load(Relaxed), c_int::from(key));

        // The command's -1 above did not become CC_ERROR; setting
        // `rl_done` is what the editor loop actually notices.
        BOUND_SETS_DONE.store(true, Relaxed);
        assert_eq!(rl_bind_wrapper(ptr::null_mut(), key), CC_EOF);

        READLINE_RUNTIME.access(|runtime| runtime.commands[key as usize] = saved);
        BOUND_SETS_DONE.store(false, Relaxed);
        rl_done = 0;
    }
}

// [spec:nshedit:req:abi.typed-session/test]
#[test]
fn command_callback_can_reenter_the_runtime() {
    let _g = globals();
    let _editor = Piped::install();
    let invoking = b'x';
    let nested = b'y';
    // SAFETY: the global test lock serializes the runtime. The callback is
    // copied out before invocation, so its nested registration cannot overlap
    // a borrow of the command table.
    unsafe {
        let saved_invoking = READLINE_RUNTIME.access(|runtime| runtime.commands[invoking as usize]);
        let saved_nested = READLINE_RUNTIME.access(|runtime| runtime.commands[nested as usize]);
        READLINE_RUNTIME.access(|runtime| {
            runtime.commands[invoking as usize] = Some(reentrant_binding_command);
        });
        REENTRANT_BIND_RESULT.store(c_int::MIN, Relaxed);

        assert_eq!(
            rl_bind_wrapper(runtime_editor(), invoking),
            CC_NORM,
            "the outer dispatch completes normally"
        );
        assert_eq!(REENTRANT_BIND_RESULT.load(Relaxed), 0);
        assert!(
            READLINE_RUNTIME
                .access(|runtime| runtime.commands[nested as usize])
                .is_some_and(|callback| {
                    core::ptr::fn_addr_eq(callback, recording_command as RlCommandFuncT)
                })
        );

        READLINE_RUNTIME.access(|runtime| {
            runtime.commands[invoking as usize] = saved_invoking;
            runtime.commands[nested as usize] = saved_nested;
        });
    }
}

/// `^Z` in readline emulation mode raises SIGTSTP at the calling thread
/// and answers CC_NORM, so editing resumes on the same line when the
/// process is continued. Nothing about the terminal is touched here —
/// that is `EL_SIGNAL`'s handler, and if the application never turned
/// `EL_SIGNAL` on, nothing puts the tty back into cooked mode before the
/// stop.
///
/// The signal is real, so the test cannot let its default action run. A
/// one-signal scoped owner observes the raise and restores the previous
/// disposition even if the assertion unwinds.
// [spec:libedit:sem:readline.el-rl-tstp-fn/test]
#[test]
fn suspending_raises_sigtstp_at_this_thread_and_resumes_normally() {
    use nshedit_plat::signal::{Signal, SignalHandlers};

    let _g = globals();
    let handlers = SignalHandlers::with_signals(&[Signal::Suspend]).unwrap();

    // Both parameters are unused.
    let rc = _el_rl_tstp(ptr::null_mut(), c_int::from(b'\x1a'));

    // `raise` is `pthread_kill(pthread_self(), ...)`, so delivery has
    // already happened by the time it returns.
    assert_eq!(handlers.take_pending(), Some(Signal::Suspend));
    assert_eq!(rc, CC_NORM);
}

// -----------------------------------------------------------------------
// The entry points that need a live editor
// -----------------------------------------------------------------------

/// The module's process-global editor and history, stood up over pipes.
///
/// Without this every entry point below would run `lazy_init`, and
/// `rl_initialize` builds its editor out of `rl_instream`/`rl_outstream`
/// — NULL under `cargo test`, which means the test runner's *own*
/// descriptor 0. That would query and later reset the developer's
/// terminal. Installing an editor here first means `lazy_init` finds one
/// and does nothing, and every read comes from a pipe the test fills.
///
/// The descriptors are not a terminal, so libedit raises `NO_TTY` and
/// leaves the tty layer alone; that is why `rl_prep_terminal` and
/// `rl_deprep_terminal` are not exercised here.
struct Piped {
    /// What the test writes for the editor to read. `Option` so it can be
    /// closed mid-test, which is the only way to produce end of input.
    input: Option<std::io::PipeWriter>,
    /// Held open so the editor's descriptors stay valid; [`Drop`] runs
    /// before these do, so teardown can still touch them.
    _in_read: std::io::PipeReader,
    _out_write: std::io::PipeWriter,
    _out_read: std::io::PipeReader,
}

impl Piped {
    fn install() -> Self {
        use std::os::fd::AsRawFd;

        let (in_read, input) = std::io::pipe().expect("pipe");
        let (out_read, out_write) = std::io::pipe().expect("pipe");
        // SAFETY: single-threaded under the test lock, which every caller
        // holds. The descriptors outlive the editor because `Drop` ends it
        // before the fields are closed.
        unsafe {
            release_runtime_session();
            let editor = crate::histedit::el_init_fd(
                c"nshedit-test".as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                in_read.as_raw_fd(),
                out_write.as_raw_fd(),
                out_write.as_raw_fd(),
            );
            let editor = NonNull::new(editor).expect("el_init_fd");
            let history = NonNull::new(crate::histedit::history_init()).expect("history_init");
            READLINE_RUNTIME.install(editor, history);
        }
        Self {
            input: Some(input),
            _in_read: in_read,
            _out_write: out_write,
            _out_read: out_read,
        }
    }

    /// Stage bytes for the editor to read.
    fn feed(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let w = self.input.as_mut().expect("input still open");
        w.write_all(bytes).expect("write");
        w.flush().expect("flush");
    }

    /// Close the editor's input, which is the only way to reach end of
    /// input on a pipe.
    fn close_input(&mut self) {
        self.input = None;
    }

    /// One character out of the editor's pushback queue, which is what
    /// shows whether `el_push` was reached.
    ///
    /// Only the queue. `el_wgetc` switches the terminal to raw mode
    /// before it will read a descriptor, and on a pipe that fails and is
    /// reported as end of file (ERR-input-24) — so once the queue is
    /// empty this answers 0 whatever is waiting on the pipe. Line reads
    /// go through `el_wgets`, which takes the `NO_TTY` path instead and
    /// does reach the descriptor; that is why `readline()` works here and
    /// this does not.
    fn next_key(&self) -> u8 {
        let mut buf = [0 as c_char; 2];
        // SAFETY: the caller holds the test lock and the runtime owns this
        // fixture's editor.
        let rc = unsafe { crate::eln::el_getc(runtime_editor(), buf.as_mut_ptr()) };
        assert_eq!(rc, 1, "expected a queued character");
        buf[0] as u8
    }

    /// Prove the pushback queue is empty by pushing a sentinel and
    /// reading it straight back.
    ///
    /// Sound because the queue is a *queue*: `el_wgetc` always takes from
    /// the oldest entry, so anything left over would be handed back
    /// before the sentinel. Reading a byte off the pipe instead would not
    /// work at all — see `next_key`.
    fn assert_no_more_input(&mut self) {
        // SAFETY: as `next_key`.
        unsafe { crate::eln::el_push(runtime_editor(), c"\x1f".as_ptr()) };
        assert_eq!(self.next_key(), 0x1f, "input was left in the queue");
    }
}

impl Drop for Piped {
    fn drop(&mut self) {
        // SAFETY: the descriptors are still open — the fields drop after
        // this — and the caller still holds the test lock.
        unsafe { release_runtime_session() }
    }
}

/// `rl_message` delegates the erased argument list to the platform's C
/// formatter, then applies libedit's fixed 159-byte payload limit.
// [spec:libedit:sem:readline.rl-message-fn/test]
#[test]
fn messages_format_and_truncate() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: each format string matches its argument list, and the editor is
    // live for the forced redisplay.
    unsafe {
        rl_message(c"%s:%d".as_ptr(), c"item".as_ptr(), 42 as c_int);
        assert_eq!(c_bytes(rl_prompt), b"item:42");

        let long = std::ffi::CString::new(vec![b'x'; 200]).expect("no NUL");
        rl_message(c"%s".as_ptr(), long.as_ptr());
        assert_eq!(c_bytes(rl_prompt), vec![b'x'; MAX_MESSAGE - 1]);
    }
}

/// `rl_insert` does not insert. It pushes the character back onto the
/// pending-input queue `count` times, so what happens next is whatever
/// that key is *bound* to; GNU readline's `rl_insert` puts the character
/// in the line. The two are swapped relative to readline
/// (ERR-readline-42), and a program that calls it to prefill a line gets
/// its keymap re-run instead.
// [spec:libedit:sem:readline.rl-insert-fn/test]
#[test]
fn rl_insert_pushes_the_key_back_instead_of_inserting_it() {
    let _g = globals();
    let mut ed = Piped::install();
    // SAFETY: single-threaded under the lock; the runtime owns the fixture.
    unsafe {
        assert_eq!(rl_insert(3, c_int::from(b'x')), 0);

        // The line is untouched, which is the whole divergence.
        let li = crate::eln::el_line(runtime_editor());
        assert_eq!((*li).lastchar, (*li).buffer);

        // A non-positive count pushes nothing at all rather than once.
        assert_eq!(rl_insert(0, c_int::from(b'q')), 0);
        assert_eq!(rl_insert(-5, c_int::from(b'q')), 0);

        assert_eq!(ed.next_key(), b'x');
        assert_eq!(ed.next_key(), b'x');
        assert_eq!(ed.next_key(), b'x');
        ed.assert_no_more_input();
    }
}

/// `rl_newline` ignores both arguments and pushes a single newline back,
/// so it accepts the line only if `\n` still has its default binding —
/// rebind it and this does whatever the new binding does
/// (ERR-readline-44).
// [spec:libedit:sem:readline.rl-newline-fn/test]
#[test]
fn rl_newline_pushes_one_newline_whatever_it_is_asked_for() {
    let _g = globals();
    let mut ed = Piped::install();
    // SAFETY: single-threaded under the lock.
    unsafe {
        // Readline-4.0 ignores the args, and so does this: a count of 5
        // still pushes exactly one.
        assert_eq!(rl_newline(5, c_int::from(b'Z')), 0);

        assert_eq!(ed.next_key(), b'\n');
        ed.assert_no_more_input();
    }
}

/// `rl_get_previous_history` moves through no history at all: it pushes
/// the invoking key back `count` times and leaves every history global
/// where it was (ERR-readline-44). It therefore only recalls anything if
/// `key` happens to be bound to a history command.
// [spec:libedit:sem:readline.rl-get-previous-history-fn/test]
#[test]
fn getting_the_previous_history_entry_only_pushes_the_key_back() {
    let _g = globals();
    let mut ed = Piped::install();
    // SAFETY: single-threaded under the lock; `history_length` is
    // restored below.
    unsafe {
        let saved = history_length;
        history_length = 7;

        assert_eq!(rl_get_previous_history(2, c_int::from(b'p')), 0);

        let after = history_length;
        assert_eq!(after, 7);
        assert_eq!(ed.next_key(), b'p');
        assert_eq!(ed.next_key(), b'p');
        ed.assert_no_more_input();

        history_length = saved;
    }
}

/// `rl_read_key` returns `el_getc`'s *status*, never the key: 1 for every
/// successful read whatever was pressed, 0 at end of input, -1 on error
/// (ERR-readline-23). The character it read is written into a stack
/// buffer and dropped, so a program using this to read a keystroke cannot
/// tell which key it was.
// [spec:libedit:sem:readline.rl-read-key-fn/test]
#[test]
fn reading_a_key_answers_the_status_and_throws_the_key_away() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock.
    unsafe {
        crate::eln::el_push(runtime_editor(), c"z".as_ptr());
        assert_eq!(rl_read_key(), 1);

        // Not 'z' (0x7a), and not 0x1b either — the same 1 comes back for
        // a different key.
        crate::eln::el_push(runtime_editor(), c"\x1b".as_ptr());
        assert_eq!(rl_read_key(), 1);

        // With the queue empty this is 0, and it would be 0 with a
        // keystroke waiting on the descriptor too: `el_wgetc` reports a
        // failed switch to raw mode as end of file (ERR-input-24), so on
        // anything that is not a terminal `rl_read_key` reads exactly the
        // characters that were pushed back and nothing else.
        assert_eq!(rl_read_key(), 0);
    }
}

/// `rl_reset_terminal` ignores the terminal name it is handed — readline
/// re-queries terminfo for it, this does not, so a name no terminfo
/// database has still succeeds — and forwards to `el_reset`, which puts
/// the terminal back into cooked mode and resets the *line* state.
///
/// What it does not reset is pending pushback: `ch_reset` never touches
/// the macro queue, so a key pushed before the reset is still delivered
/// after it. That was the surprise here — "reset the terminal" reads like
/// it should drop typeahead, and it does not.
// [spec:libedit:sem:readline.rl-reset-terminal-fn/test]
#[test]
fn resetting_the_terminal_ignores_the_name_and_keeps_pending_input() {
    let _g = globals();
    let ed = Piped::install();
    // SAFETY: single-threaded under the lock.
    unsafe {
        assert!(crate::eln::el_insertstr(runtime_editor(), c"abc".as_ptr()) >= 0);
        crate::eln::el_push(runtime_editor(), c"z".as_ptr());

        assert_eq!(rl_reset_terminal(c"no-such-terminal-anywhere".as_ptr()), 0);

        // The line is empty again — the old text is still in the buffer,
        // but above `lastchar`, so nothing reads it.
        let li = crate::eln::el_line(runtime_editor());
        assert_eq!((*li).lastchar, (*li).buffer);

        // The pushed key survived.
        assert_eq!(ed.next_key(), b'z');

        // NULL is how readline is asked to re-read $TERM; here it is the
        // same nothing as any other name.
        assert_eq!(rl_reset_terminal(ptr::null()), 0);
    }
}

/// With completion inhibited the invoking key is inserted literally and
/// CC_REFRESH comes back — a libedit action code, not readline's
/// success/failure status, because the same function is also the editor
/// command bound to Tab.
///
/// Only this branch is pinned here. The completion path proper needs a
/// filesystem and a generator and is what `conformance/driver/readline_api.c`
/// is for; what no driver reaches is the inhibited one.
// [spec:libedit:sem:readline.rl-complete-fn/test]
#[test]
fn inhibited_completion_inserts_the_invoking_key_literally() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock; the global is restored.
    unsafe {
        let saved = rl_inhibit_completion;
        rl_inhibit_completion = 1;

        assert_eq!(rl_complete(0, c_int::from(b'\t')), c_int::from(CC_REFRESH));

        // Inserted into the line, not pushed back — the opposite of what
        // `rl_insert` does with its argument.
        let li = crate::eln::el_line(runtime_editor());
        assert_eq!(c_bytes((*li).buffer), b"\t");

        rl_inhibit_completion = saved;
    }
}

/// The editor command bound to `^I` is a thin adapter over `rl_complete`:
/// readline's ignored `count` is hardcoded to 0 and the `int` result is
/// narrowed to the `unsigned char` the editor loop wants. Every CC_* code
/// is small, so the narrowing is lossless.
// [spec:libedit:sem:readline.el-rl-complete-fn/test]
#[test]
fn the_tab_command_narrows_what_rl_complete_returns() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock; the global is restored.
    unsafe {
        let saved = rl_inhibit_completion;
        rl_inhibit_completion = 1;

        // The editor argument is unused; `rl_complete` reaches the module
        // statics for the editor it actually needs.
        assert_eq!(
            _el_rl_complete(ptr::null_mut(), c_int::from(b'\t')),
            CC_REFRESH
        );

        rl_inhibit_completion = saved;
    }
}

/// `rl_redisplay` pushes the terminal's reprint character back as *input*
/// and then forces a repaint, so the line is drawn twice — once by the
/// forced update and once when whatever `^R` is bound to consumes the
/// pushed character (ERR-readline-26).
// [spec:libedit:sem:readline.rl-redisplay-fn/test]
#[test]
fn redisplaying_pushes_the_reprint_key_back_as_input() {
    let _g = globals();
    let mut ed = Piped::install();
    // SAFETY: single-threaded under the lock.
    unsafe {
        let reprint = (&*runtime_editor()).control_reprint();

        rl_redisplay();

        assert_eq!(ed.next_key(), reprint);
        ed.assert_no_more_input();
    }
}

static CALLBACK_LINES: Mutex<Vec<Option<Vec<u8>>>> = Mutex::new(Vec::new());

/// Stands in for the application's `rl_linefunc`, recording what it was
/// handed and releasing it — the callback owns the block.
unsafe extern "C" fn recording_linefunc(line: *mut c_char) {
    // SAFETY: the contract is a `malloc`ed NUL-terminated string or NULL.
    let seen = unsafe {
        if line.is_null() {
            None
        } else {
            Some(c_bytes(line).to_vec())
        }
    };
    CALLBACK_LINES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(seen);
    // SAFETY: as above; `c_free_str` tolerates NULL.
    unsafe { c_free_str(line) };
}

unsafe extern "C" fn other_linefunc(line: *mut c_char) {
    // SAFETY: as `recording_linefunc`.
    unsafe { c_free_str(line) };
}

/// Installing a callback handler sets the prompt, records the line
/// callback and puts the editor into unbuffered mode. There is no stack:
/// a second install silently replaces the first, and the return value of
/// the prompt copy is discarded, so an allocation failure goes unnoticed.
// [spec:libedit:sem:readline.rl-callback-handler-install-fn/test]
#[test]
fn installing_a_callback_handler_replaces_rather_than_stacks() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock; the module statics are the
    // fixture's.
    unsafe {
        rl_callback_handler_install(c"cb> ".as_ptr(), Some(recording_linefunc));

        let prompt = rl_prompt;
        assert!(!prompt.is_null());
        assert_eq!(c_bytes(prompt), b"cb> ");
        let installed = rl_linefunc.map(|f| f as usize);
        assert_eq!(installed, Some(recording_linefunc as *const () as usize));
        assert!((&*runtime_editor()).unbuffered());

        // The second install overwrites both without a word.
        rl_callback_handler_install(c"two> ".as_ptr(), Some(other_linefunc));
        let prompt = rl_prompt;
        assert_eq!(c_bytes(prompt), b"two> ");
        let installed = rl_linefunc.map(|f| f as usize);
        assert_eq!(installed, Some(other_linefunc as *const () as usize));

        rl_callback_handler_remove();
    }
}

/// Removing the handler clears the line callback and leaves unbuffered
/// mode, and does nothing else: the prompt is neither restored nor freed,
/// a half-typed line is not discarded and the display is left as it is.
// [spec:libedit:sem:readline.rl-callback-handler-remove-fn/test]
#[test]
fn removing_a_callback_handler_leaves_the_prompt_behind() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock.
    unsafe {
        rl_callback_handler_install(c"cb> ".as_ptr(), Some(recording_linefunc));

        rl_callback_handler_remove();

        let installed = rl_linefunc.map(|f| f as usize);
        assert_eq!(installed, None);
        assert!(!(&*runtime_editor()).unbuffered());

        // Still the callback prompt, so a program alternating between
        // callback mode and `readline()` carries it across.
        let prompt = rl_prompt;
        assert_eq!(c_bytes(prompt), b"cb> ");
    }
}

/// `readline()` returns a fresh `malloc`ed copy for the caller to free,
/// with a single trailing newline removed. End of input is a NULL return
/// — and *only* end of input is, so a blank line comes back as a
/// non-NULL empty string that a caller must still free.
///
/// CRLF is where this bites. The read stops at the `\r`, and the strip
/// only ever removes a `\n`, so `"crlf\r\n"` is delivered as the line
/// `"crlf\r"` — carriage return included — followed by an empty line for
/// the `\n` that was left behind. A program reading a file with DOS line
/// endings through this sees twice as many lines as the file has, every
/// other one blank.
///
/// `history_length` is refreshed from the history on the way out even
/// though `readline()` never adds anything to it.
// [spec:libedit:sem:readline.readline-fn/test]
#[test]
fn readline_returns_a_caller_owned_copy_without_the_newline() {
    let _g = globals();
    let mut ed = Piped::install();
    // SAFETY: single-threaded under the lock; every returned block is
    // this module's own allocation and is released below.
    unsafe {
        history_length = 99;
        ed.feed(b"echo hi\n");

        let line = readline(c"> ".as_ptr());
        assert!(!line.is_null());
        assert_eq!(c_bytes(line), b"echo hi");
        c_free_str(line);

        // Nothing was added, and the stale 99 is gone all the same.
        let len = history_length;
        assert_eq!(len, 0);

        ed.feed(b"crlf\r\n");
        let line = readline(c"> ".as_ptr());
        assert!(!line.is_null());
        assert_eq!(c_bytes(line), b"crlf\r");
        c_free_str(line);

        // The orphaned `\n`, which strips to nothing.
        let line = readline(c"> ".as_ptr());
        assert!(!line.is_null(), "a blank line is not end of input");
        assert_eq!(c_bytes(line), b"");
        c_free_str(line);

        ed.close_input();
        assert!(readline(c"> ".as_ptr()).is_null());
    }
}

/// Callback mode delivers one keystroke per call, and the line callback
/// fires only when the character read terminates the line. The block it
/// is handed is the callback's to `free()` — nothing else will — and NULL
/// means end of input, indistinguishably from a copy that failed to
/// allocate.
///
/// Two things are worth reading twice. `RL_STATE_DONE` is raised here and
/// cleared only by `rl_initialize`, so once a line completes it stays
/// raised for the rest of the session. And on a descriptor that is not a
/// terminal — which is what this fixture has, and what a program piped
/// into has — libedit restarts the line on every read, so the callback
/// receives only the final chunk: the newline, which the trim then
/// removes, leaving the empty string. The characters typed before it are
/// not delivered at all.
// [spec:libedit:sem:readline.rl-callback-read-char-fn/test]
#[test]
fn the_callback_reader_takes_one_key_per_call_and_fires_on_the_newline() {
    let _g = globals();
    let mut ed = Piped::install();

    let seen = || {
        CALLBACK_LINES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    };

    // SAFETY: single-threaded under the lock; the runtime session is restored
    // before the fixture tears it down.
    unsafe {
        CALLBACK_LINES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        rl_readline_state = RL_STATE_NONE;
        rl_callback_handler_install(c"cb> ".as_ptr(), Some(recording_linefunc));
        ed.feed(b"hi\n");

        rl_callback_read_char();
        rl_callback_read_char();
        assert!(seen().is_empty(), "fired before the line ended");
        let state = rl_readline_state;
        assert_eq!(state & RL_STATE_DONE, 0);

        rl_callback_read_char();
        assert_eq!(seen(), vec![Some(Vec::new())]);
        let state = rl_readline_state;
        assert_ne!(state & RL_STATE_DONE, 0);

        // No lazy-init guard, unlike almost every other entry point: the
        // C hands the NULL editor straight to `el_gets` (ERR-readline-11)
        // and the port returns instead. The handler is still installed, so
        // a call that got past the guard would show up as a fourth line.
        let saved = READLINE_RUNTIME.take_session();
        rl_callback_read_char();
        if let RuntimeSession::Ready { editor, history } = saved {
            READLINE_RUNTIME.install(editor, history);
        }
        assert_eq!(seen().len(), 1);

        rl_callback_handler_remove();
    }
}

static WBREAK_CALLS: AtomicUsize = AtomicUsize::new(0);
static WBREAK_SAW_POINT: AtomicI32 = AtomicI32::new(0);
static WBREAK_SAW_END: AtomicI32 = AtomicI32::new(0);

/// Stands in for the application's `rl_completion_word_break_hook`,
/// recording the position globals as they stood when it was called.
unsafe extern "C" fn recording_word_break_hook() -> *mut c_char {
    WBREAK_CALLS.fetch_add(1, Relaxed);
    // SAFETY: single-threaded module state; the test holds the lock.
    unsafe {
        WBREAK_SAW_POINT.store(rl_point, Relaxed);
        WBREAK_SAW_END.store(rl_end, Relaxed);
    }
    // Borrowed: `rl_complete` uses the pointer directly and never frees
    // it, so a hook returning a static string is correct.
    c":".as_ptr().cast_mut()
}

/// The hook is consulted fresh on every completion — nothing is cached —
/// and it runs *before* `_rl_update_pos`, so `rl_point` and `rl_end`
/// inside it still hold whatever was there before, not the line being
/// completed (ERR-readline-50). An application that reads them to decide
/// which break characters to return is reading the previous line.
// [spec:libedit:sem:readline.rl-completion-word-break-hook-fn/test]
#[test]
fn the_word_break_hook_sees_stale_positions_and_is_asked_every_time() {
    let _g = globals();
    let _ed = Piped::install();
    // SAFETY: single-threaded under the lock; both globals are restored.
    unsafe {
        let saved_hook = rl_completion_word_break_hook;
        let saved_inhibit = rl_inhibit_completion;
        rl_inhibit_completion = 0;
        rl_completion_word_break_hook = Some(recording_word_break_hook);
        WBREAK_CALLS.store(0, Relaxed);

        // A prefix nothing on disk can complete, so the attempt finds no
        // match and returns without displaying a list or asking anything.
        assert!(crate::eln::el_insertstr(runtime_editor(), c"zzqqnosuchprefix".as_ptr()) >= 0);
        rl_point = -1;
        rl_end = -1;

        rl_complete(0, c_int::from(b'\t'));

        assert_eq!(WBREAK_CALLS.load(Relaxed), 1);
        assert_eq!(
            WBREAK_SAW_POINT.load(Relaxed),
            -1,
            "the hook saw fresh positions"
        );
        assert_eq!(WBREAK_SAW_END.load(Relaxed), -1, "the hook saw a fresh end");

        // Asked again on the next completion; there is no cache.
        rl_complete(0, c_int::from(b'\t'));
        assert_eq!(WBREAK_CALLS.load(Relaxed), 2);
        // By the second call the first `_rl_update_pos` has run, so the
        // hook now sees the line — stale by one completion rather than
        // uninitialised.
        assert_eq!(WBREAK_SAW_POINT.load(Relaxed), 16);
        assert_eq!(WBREAK_SAW_END.load(Relaxed), 16);

        rl_completion_word_break_hook = saved_hook;
        rl_inhibit_completion = saved_inhibit;
    }
}
