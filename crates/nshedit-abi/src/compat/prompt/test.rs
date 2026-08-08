//! Tests for the ported `src/prompt.c`.
//!
//! The two built-in callbacks and `prompt_get` are reached the way an
//! application reaches them — through the stored `p_func` slot — because the
//! whole of what this module gets wrong is *which* slot a given `op` selects.

use core::cell::Cell;
use core::ptr;

use super::*;
use crate::el::blank_editline;
use crate::locale;

/// C: `#define EL_RPROMPT 12`, the op `prompt_set` and `prompt_get` both treat
/// as "not the left-hand prompt". `prompt_print` takes a [`PromptSide`].
const EL_RPROMPT: i32 = 12;

/// An editor with a screen to draw a prompt onto and no descriptors.
///
/// `prompt_print` writes through `re_putc` into `el_vdisplay`, and
/// `re_nextline` reads `t_size.v`, so both have to be real; descriptor 0 is
/// the test runner's own stdout, hence the -1s.
fn editor() -> EditLine {
    let mut el = blank_editline();
    el.el_infd = -1;
    el.el_outfd = -1;
    el.el_errfd = -1;
    el.el_terminal.t_size = CoordT { h: 20, v: 4 };
    el.el_display = vec![vec![0u32; 21]; 4];
    el.el_vdisplay = vec![vec![0u32; 21]; 4];
    el
}

/// The virtual screen row as the terminal would read it: up to the first NUL.
fn drawn(el: &EditLine, v: usize) -> String {
    el.el_vdisplay[v]
        .iter()
        .take_while(|&&c| c != 0)
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// The NUL-terminated wide string a callback handed back, read the way
/// `prompt_print`'s wide branch reads it.
///
/// # Safety
///
/// `p` must be a live, NUL-terminated `wchar_t` string, which is exactly the
/// ownership contract `sem:prompt.prompt-default-fn` puts on every prompt
/// callback's result.
unsafe fn wide_at(p: *mut u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut q = p;
    // SAFETY: the caller's precondition; the walk stops at the terminator the
    // contract guarantees.
    unsafe {
        while q.read() != 0 {
            out.push(q.read());
            q = q.add(1);
        }
    }
    out
}

thread_local! {
    /// How many times [`counting`] has been called; a prompt getter must never
    /// move it. Per-thread, because the test runner runs tests in parallel and
    /// other tests here do call the same hook.
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

static COUNTING_TEXT: [u32; 2] = ['C' as u32, 0];

unsafe extern "C" fn counting(_el: *mut EditLine) -> *mut u32 {
    CALLS.with(|n| n.set(n.get() + 1));
    COUNTING_TEXT.as_ptr().cast_mut()
}

static RIGHT_TEXT: [u32; 2] = ['R' as u32, 0];

unsafe extern "C" fn right_hook(_el: *mut EditLine) -> *mut u32 {
    RIGHT_TEXT.as_ptr().cast_mut()
}

// ---------------------------------------------------------------------------
// prompt_default
// ---------------------------------------------------------------------------

/// The storage is a function-local `static wchar_t a[3]` — program lifetime,
/// shared by every `EditLine`, and the same address on every call. That is the
/// ownership contract the rule states for prompt callbacks generally: the
/// callee owns the string and libedit only reads it, so a callback returning a
/// stack buffer or freeing between calls would be the caller's bug and not
/// libedit's.
// [spec:libedit:sem:prompt.prompt-default-fn/test]
#[test]
fn the_built_in_prompt_is_one_shared_static_question_mark_and_space() {
    // SAFETY: the C ignores its argument completely and so does the port, so
    // every argument below — including a null handle and a live editor — is
    // within contract.
    let (a, b, c) = unsafe {
        let mut el = editor();
        (
            prompt_default(ptr::null_mut()),
            prompt_default(ptr::null_mut()),
            prompt_default(ptr::from_mut(&mut el)),
        )
    };

    assert_eq!(a, b, "the same static on every call");
    assert_eq!(a, c, "and it does not depend on the editor");
    // SAFETY: a program-lifetime static, terminated by the C's `L"? "`.
    assert_eq!(unsafe { wide_at(a) }, ['?' as u32, ' ' as u32]);
}

/// ERR-terminal-57, and it is what a user sees rather than an internal detail.
/// `prompt_init` never assigns `p_wide`, so an application that never sets a
/// prompt reaches this wide callback through `prompt_print`'s *narrow* branch,
/// which reinterprets `L"? "` as a multibyte string. On a little-endian
/// four-byte-`wchar_t` host the byte image is `3F 00 …`, so the decode stops
/// after one byte and the default prompt renders as a bare `?` in one column
/// — not `? ` in two.
// [spec:libedit:sem:prompt.prompt-default-fn/test]
#[test]
fn the_default_prompt_loses_its_trailing_space_to_the_narrow_branch() {
    let mut el = editor();
    prompt_init(&mut el);
    assert_eq!(
        el.el_prompt.p_wide, 0,
        "the unassigned field ERR-terminal-57"
    );

    prompt_print(&mut el, PromptSide::Left);

    if cfg!(target_endian = "little") {
        assert_eq!(drawn(&el, 0), "?");
        assert_eq!(el.el_prompt.p_pos.h, 1, "one column, not two");
    } else {
        // The leading byte of `L'?'` is 0 there, so the scan yields no bytes
        // at all and the default prompt is empty.
        assert_eq!(drawn(&el, 0), "");
        assert_eq!(el.el_prompt.p_pos.h, 0);
    }
    assert_eq!(el.el_prompt.p_pos.v, 0);

    // Declaring the same callback wide is the whole fix, and it is a visible
    // behaviour change: two columns rather than one.
    let mut el = editor();
    prompt_init(&mut el);
    el.el_prompt.p_wide = 1;
    prompt_print(&mut el, PromptSide::Left);
    assert_eq!(drawn(&el, 0), "? ");
    assert_eq!(el.el_prompt.p_pos.h, 2);
}

/// A NULL function for the left-hand side reinstalls this one, and what comes
/// back through `prompt_get` is the same libedit-internal pointer an
/// application has no declaration for — which is the rule's point about the
/// getter handing out the raw slot.
// [spec:libedit:sem:prompt.prompt-default-fn/test]
#[test]
fn clearing_the_left_prompt_reinstalls_the_built_in_one() {
    let mut el = editor();
    prompt_set(&mut el, Some(counting), u32::from(b'%'), EL_PROMPT, 1);
    prompt_set(&mut el, None, 0, EL_PROMPT, 1);

    let mut f = None;
    assert_eq!(prompt_get(&mut el, Some(&mut f), None, EL_PROMPT), 0);
    // SAFETY: the slot holds a prompt callback, and both candidates ignore
    // the handle.
    let text = unsafe { wide_at(f.unwrap()(ptr::null_mut())) };
    assert_eq!(
        text,
        ['?' as u32, ' ' as u32],
        "the default, not `counting`"
    );
}

// ---------------------------------------------------------------------------
// prompt_get
// ---------------------------------------------------------------------------

/// The only failure path, and it is taken before anything is written: a NULL
/// `prf` leaves the escape-character out-parameter alone rather than filling
/// it and then failing.
// [spec:libedit:sem:prompt.prompt-get-fn/test]
#[test]
fn a_null_function_out_parameter_fails_without_writing_the_other_one() {
    let mut el = editor();
    prompt_init(&mut el);
    el.el_prompt.p_ignore = u32::from(b'#');

    let mut c = 0xDEAD;
    assert_eq!(prompt_get(&mut el, None, Some(&mut c), EL_PROMPT), -1);
    assert_eq!(c, 0xDEAD, "nothing was stored");
}

/// ERR-core-api-14: step 2 is missing the `EL_PROMPT_ESC` arm `prompt_set`
/// has, so the op most likely to be used to read an escape character back
/// reports the *right-hand* prompt's. Everything except a bare `EL_PROMPT`
/// selects the rprompt, including values that name no op at all.
// [spec:libedit:sem:prompt.prompt-get-fn/test]
#[test]
fn only_a_bare_left_prompt_op_reads_the_left_prompt() {
    let mut el = editor();
    prompt_set(&mut el, Some(counting), u32::from(b'<'), EL_PROMPT, 1);
    prompt_set(&mut el, Some(right_hook), u32::from(b'>'), EL_RPROMPT, 1);

    // Identify the record by what its callback returns, not by comparing
    // function addresses: the point is which slot was read, and the slot is
    // only observable through the value it holds.
    let read = |el: &mut EditLine, op: i32| {
        let mut f = None;
        let mut c = 0;
        assert_eq!(prompt_get(el, Some(&mut f), Some(&mut c), op), 0);
        // SAFETY: both hooks below ignore the handle they are passed.
        let text = unsafe { wide_at(f.unwrap()(ptr::null_mut())) };
        (text, c)
    };

    assert_eq!(
        read(&mut el, EL_PROMPT),
        (vec!['C' as u32], u32::from(b'<'))
    );
    assert_eq!(
        read(&mut el, EL_PROMPT_ESC),
        (vec!['R' as u32], u32::from(b'>')),
        "ERR-core-api-14: the escape op reads the wrong side"
    );
    assert_eq!(
        read(&mut el, EL_RPROMPT),
        (vec!['R' as u32], u32::from(b'>'))
    );
    assert_eq!(
        read(&mut el, -7),
        (vec!['R' as u32], u32::from(b'>')),
        "an unrecognised op is not rejected either"
    );
}

/// A NULL `c` is legal and simply skips the store — which is what both
/// `el_get` and `el_wget` pass for a plain `EL_PROMPT`, and therefore why
/// `el_prompt.p_ignore` cannot be retrieved through the public API at all.
/// The getter also never invokes the callback: it hands the pointer over
/// untouched.
// [spec:libedit:sem:prompt.prompt-get-fn/test]
#[test]
fn the_escape_character_store_is_optional_and_the_callback_is_never_run() {
    let mut el = editor();
    prompt_set(&mut el, Some(counting), u32::from(b'%'), EL_PROMPT, 1);
    let before = CALLS.with(Cell::get);

    let mut f = None;
    assert_eq!(prompt_get(&mut el, Some(&mut f), None, EL_PROMPT), 0);
    assert!(f.is_some(), "the function is stored even with a NULL `c`");
    assert_eq!(
        CALLS.with(Cell::get),
        before,
        "reading the slot must not call what is in it"
    );

    // The reachable route to the left-hand escape character is the field, not
    // the getter — which is the shape ERR-core-api-14 leaves the API in.
    assert_eq!(el.el_prompt.p_ignore, u32::from(b'%'));
}

/// The getter reports no `p_wide`, so a caller cannot tell a narrow callback
/// installed through `el_set` from a wide one installed through `el_wset`,
/// even though the two disagree about how the returned pointer is read. The
/// same stored pointer renders differently depending on a flag the getter
/// does not expose.
// [spec:libedit:sem:prompt.prompt-get-fn/test]
#[test]
fn what_comes_back_does_not_say_how_the_string_will_be_read() {
    let mut el = editor();
    prompt_set(&mut el, Some(counting), 0, EL_PROMPT, 0);
    let mut narrow = None;
    prompt_get(&mut el, Some(&mut narrow), None, EL_PROMPT);

    prompt_set(&mut el, Some(counting), 0, EL_PROMPT, 1);
    let mut wide = None;
    prompt_get(&mut el, Some(&mut wide), None, EL_PROMPT);

    assert!(narrow.is_some() && wide.is_some());
    // Same slot value, and the only thing that distinguishes the two readings
    // stayed behind in the record.
    assert_eq!(el.el_prompt.p_wide, 1);

    // What the difference costs: the narrow reading of `L"C"` stops at the
    // first zero byte.
    let mut el = editor();
    prompt_set(&mut el, Some(counting), 0, EL_PROMPT, 0);
    prompt_print(&mut el, PromptSide::Left);
    let narrow_columns = el.el_prompt.p_pos.h;

    let mut el = editor();
    prompt_set(&mut el, Some(counting), 0, EL_PROMPT, 1);
    prompt_print(&mut el, PromptSide::Left);
    assert_eq!(drawn(&el, 0), "C");
    assert_eq!(el.el_prompt.p_pos.h, 1);

    if cfg!(target_endian = "little") {
        assert_eq!(
            narrow_columns, 1,
            "one ASCII character survives the reinterpretation"
        );
    } else {
        assert_eq!(narrow_columns, 0);
    }

    // The charset is not what makes the two agree here: `C` is one byte in
    // either, so the divergence above is the pointer type alone.
    assert_eq!(locale::wcwidth(locale::charset(), 'C' as u32), 1);
}
