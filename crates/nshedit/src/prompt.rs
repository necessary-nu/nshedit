//! Ported from `src/prompt.c`; rules live in `docs/spec/port/src/prompt.md`.

use core::ptr;

use crate::chartype::ct_decode_string;
use crate::el::{CoordT, EditLine};
use crate::refresh::{re_putc, re_putliteral};

// The four `el_set`/`el_get` ops this module tests `op` against. They belong
// to `histedit.h`, which has no Rust counterpart yet; these are private so
// that adopting the public constants later is a mechanical substitution.
/// C: `#define EL_PROMPT 0`.
const EL_PROMPT: i32 = 0;
/// C: `#define EL_PROMPT_ESC 21`.
const EL_PROMPT_ESC: i32 = 21;

/// C: `static wchar_t a[3] = L"? ";` inside [`prompt_default`].
///
/// Program lifetime, shared by every `EditLine`, never freed and never
/// written to by libedit — which is why this is an immutable `static` where
/// the C's is mutable. [`ElPfuncT`] hands back the C's `wchar_t *`, so the
/// callback casts away the `const`; nothing in libedit or in a conforming
/// application writes through it, and the C's own callers never do either.
static PROMPT_DEFAULT: [u32; 3] = ['?' as u32, ' ' as u32, 0];

/// C: `static wchar_t a[1] = L"";` inside [`prompt_default_r`]. Same
/// lifetime and ownership as [`PROMPT_DEFAULT`].
static PROMPT_DEFAULT_R: [u32; 1] = [0];

// [spec:libedit:def:prompt.el-pfunc-t-edit-line]
/// C: `typedef wchar_t *(*el_pfunc_t)(EditLine *);`
///
/// The prompt hook installed by `EL_PROMPT`/`EL_RPROMPT`. A C application
/// hands this in through `el_set`, so it is the C ABI's shape and not a Rust
/// `fn`: `EditLine *` is `*mut EditLine`, the `wchar_t *` result is
/// `*mut u32`, and calling one is `unsafe` because the pointer came from
/// outside. The parameter is not `&mut EditLine` for the same reason — the
/// callee is C code, which has no `&mut` and is entitled to re-enter libedit
/// through the handle it was given.
///
/// The returned string is NUL-terminated, borrowed and not freed by libedit
/// — the application owns the storage, and
/// `sem:prompt.prompt-default-fn` states that contract: it must
/// stay valid and unchanged for the duration of the `prompt_print` call that
/// asked for it.
pub type ElPfuncT = unsafe extern "C" fn(*mut EditLine) -> *mut u32;

// [spec:libedit:def:prompt.el-prompt-t]
/// One prompt (left or right) and where it left the cursor.
pub struct ElPromptT {
    /// Function to return the prompt.
    pub p_func: Option<ElPfuncT>,
    /// Position in the line after the prompt.
    pub p_pos: CoordT,
    /// C: `wchar_t p_ignore` — character that starts and ends a literal
    /// run. 0 means "no literal marker"; see
    /// `sem:prompt.prompt-print-fn`.
    pub p_ignore: u32,
    pub p_wide: i32,
}

// [spec:libedit:def:prompt.prompt-default-fn]
// [spec:libedit:sem:prompt.prompt-default-fn]
/// Signature is fixed by [`ElPfuncT`]: this is installed as a prompt
/// callback and is reached by the same indirect call an application's own
/// hook is, so it is `unsafe extern "C"` and takes the raw handle.
///
/// # Safety
///
/// Nothing is required of `el`: the C ignores its argument completely and so
/// does this, including when it is NULL.
unsafe extern "C" fn prompt_default(el: *mut EditLine) -> *mut u32 {
    // The C ignores its argument completely, so it is never dereferenced.
    let _ = el;
    PROMPT_DEFAULT.as_ptr().cast_mut()
}

// [spec:libedit:def:prompt.prompt-default-r-fn]
// [spec:libedit:sem:prompt.prompt-default-r-fn]
/// Signature and safety are as for [`prompt_default`].
///
/// # Safety
///
/// As [`prompt_default`]: `el` is never dereferenced.
unsafe extern "C" fn prompt_default_r(el: *mut EditLine) -> *mut u32 {
    // Ignored, as in `prompt_default`.
    let _ = el;
    PROMPT_DEFAULT_R.as_ptr().cast_mut()
}

// [spec:libedit:def:prompt.prompt-print-fn]
// [spec:libedit:sem:prompt.prompt-print-fn]
pub(crate) fn prompt_print(el: &mut EditLine, op: i32) {
    // Step 1: select the record. Only `EL_PROMPT` names the left-hand
    // prompt; the `_ESC` ops are not recognised here at all, unlike in
    // `prompt_set`. `op` is kept as a bool because the record cannot stay
    // borrowed across the callback below.
    let right = op != EL_PROMPT;
    let (p_func, p_wide, p_ignore) = {
        let elp = if right { &el.el_rprompt } else { &el.el_prompt };
        (elp.p_func, elp.p_wide, elp.p_ignore)
    };

    // Step 2: obtain the prompt text, by exactly one call to the callback.
    // No caching between calls and no memoisation of the previous result:
    // `re_refresh` calls this once per refresh for the left prompt and twice
    // for the right one, and callers depend on that count (ERR-terminal-60).
    //
    // The C makes the same indirect call in both branches and casts the
    // result in the narrow one, calling a `char *(*)(EditLine *)` through an
    // incompatible prototype (ERR-terminal-17, UB). That is defined here as
    // the tagged pair the errata prescribes: one pointer plus `p_wide`
    // saying how to read what it points at. Reading a wide callback's
    // `wchar_t *` as bytes — the state `prompt_init` leaves both records in,
    // ERR-terminal-57 — therefore still yields the C's observable result,
    // and stays as endianness-dependent as the C is: `L"? "` decodes to `?`
    // on a little-endian host and to nothing on a big-endian one.
    //
    // The result is copied out into an owned buffer rather than walked in
    // place. In the narrow case the C walks `el->el_scratch`, which
    // `ct_decode_string` borrows and which the walk below cannot hold while
    // it calls `re_putc`/`re_putliteral` with `&mut el`. Nothing in the walk
    // writes to `el_scratch` or re-enters the callback, so decoding eagerly
    // is what the C does; the rule's warning is only against deferring it.
    let prompt: Vec<u32> = match p_func {
        // Step 3, extended. The C cannot store a NULL `p_func` — `prompt_set`
        // installs a default instead — so this is the C's NULL indirect
        // call, undefined and never reached. Defined as "no string", the
        // same treatment a NULL return gets below.
        None => Vec::new(),
        Some(f) => {
            // SAFETY: `f` is either one of this module's two defaults or a
            // hook the application installed through `el_set(EL_PROMPT, ...)`,
            // whose contract (`def:prompt.el-pfunc-t-edit-line`) is a C
            // function taking the `EditLine *` libedit is currently driving.
            // `el` is that handle, live and exclusively borrowed here, and
            // the borrow is released for the duration of the call.
            let s = unsafe { f(ptr::from_mut(el)) };
            if s.is_null() {
                // Step 3: the C never checks, and dereferences NULL
                // (ERR-terminal-16). Defined here as an empty string: render
                // nothing, then still record `p_pos` at step 5.
                Vec::new()
            } else if p_wide != 0 {
                let mut w = Vec::new();
                // SAFETY: the wide branch's contract is that the callback
                // returned a NUL-terminated `wchar_t` string it owns, valid
                // and unchanged for the duration of this call. That is the
                // ownership contract in `sem:prompt.prompt-default-fn`, and
                // it is the same precondition the C's walk relies on.
                unsafe {
                    let mut q = s;
                    loop {
                        let c = q.read();
                        if c == 0 {
                            break;
                        }
                        w.push(c);
                        q = q.add(1);
                    }
                }
                w
            } else {
                let mut bytes = Vec::new();
                // SAFETY: with `p_wide` clear the pointer is a narrow
                // callback's NUL-terminated `char *`, so it is read as
                // bytes and never as `u32`: no alignment beyond 1 is
                // assumed. Where the pointer really is a `wchar_t *` — the
                // `prompt_init` default, ERR-terminal-57 — the scan still
                // stays inside the allocation, because every terminating
                // wide NUL contributes four zero bytes and no scan can pass
                // it.
                unsafe {
                    let mut q = s.cast::<u8>();
                    loop {
                        let c = q.read();
                        if c == 0 {
                            break;
                        }
                        bytes.push(c);
                        q = q.add(1);
                    }
                }
                // A NULL from `ct_decode_string` — bytes that are not a
                // valid multibyte string in this locale, or a scratch-buffer
                // allocation failure — is the other half of ERR-terminal-16
                // and gets the same defined treatment.
                match ct_decode_string(Some(bytes.as_slice()), &mut el.el_scratch) {
                    Some(w) => w.iter().copied().take_while(|&c| c != 0).collect(),
                    None => Vec::new(),
                }
            }
        }
    };

    // Step 4: walk the string. `prompt` stops at the first NUL, so a zero
    // `p_ignore` — the state after `prompt_init` and after any non-`_ESC`
    // `prompt_set` — can never match and branch (a) is disabled outright,
    // exactly as the C's comparison against the characters of a
    // NUL-terminated string is.
    let mut i = 0;
    while i < prompt.len() {
        let c = prompt[i];
        if p_ignore == c {
            // (a) A literal region opens here. `litstart` is just past the
            // delimiter; scan to the NUL or to the next delimiter.
            let litstart = i + 1;
            let mut j = litstart;
            while j < prompt.len() && prompt[j] != p_ignore {
                j += 1;
            }
            // The C's `!*p || !p[1]`: the region was never closed, or the
            // closing delimiter is the final character and so has no visible
            // character to glue itself to. Either way the opening delimiter,
            // the region and the closing delimiter are all discarded and the
            // whole walk is abandoned — "XXX: We lose the last literal",
            // ERR-terminal-58.
            if j == prompt.len() || j + 1 == prompt.len() {
                break;
            }
            // `re_putliteral(el, litstart, p)` with `p` at the closing
            // delimiter. The slice runs to `j + 1` inclusive rather than
            // stopping at `j`, because `literal_add` reads the delimiter at
            // `end` and the glued visible character at `end + 1`; the guard
            // above has already established both are in bounds.
            re_putliteral(el, &prompt[litstart..=j + 1], j - litstart);
            // The C's `p++` in the call and the `p++` of the enclosing
            // `for`: `p` lands two past the closing delimiter, so the
            // character glued to the literal is consumed by it and is never
            // rendered separately. Neither delimiter is ever rendered.
            i = j + 2;
            continue;
        }
        // (b) Everything outside a region goes through `re_putc` with
        // shifting — no `ct_visual_char` expansion, no tab-stop handling and
        // no newline handling, so a control character costs exactly one
        // column and a tab or newline desynchronises the accounting for the
        // rest of the session (ERR-terminal-59).
        re_putc(el, c, 1);
        i += 1;
    }

    // Step 5: record where the drawing cursor ended up. For `EL_PROMPT` this
    // is where the input text begins and the origin `re_refresh_cursor`
    // works from; for `EL_RPROMPT` `p_pos.h` doubles as the "rprompt in use"
    // flag that `re_refresh` and `re_fastaddc` test against 0.
    let v = el.el_refresh.r_cursor.v;
    let h = el.el_refresh.r_cursor.h;
    let elp = if right {
        &mut el.el_rprompt
    } else {
        &mut el.el_prompt
    };
    elp.p_pos.v = v;
    elp.p_pos.h = h;
}

// [spec:libedit:def:prompt.prompt-init-fn]
// [spec:libedit:sem:prompt.prompt-init-fn]
pub(crate) fn prompt_init(el: &mut EditLine) -> i32 {
    el.el_prompt.p_func = Some(prompt_default);
    el.el_prompt.p_pos.v = 0;
    el.el_prompt.p_pos.h = 0;
    el.el_prompt.p_ignore = 0;
    el.el_rprompt.p_func = Some(prompt_default_r);
    el.el_rprompt.p_pos.v = 0;
    el.el_rprompt.p_pos.h = 0;
    el.el_rprompt.p_ignore = 0;
    // GAP, reproduced deliberately: neither record's `p_wide` is assigned
    // (ERR-terminal-57). This is not undefined — `el_init_internal` allocates
    // the `EditLine` with `el_calloc`, so the value read later is a
    // determinate 0, meaning "narrow", while both functions installed above
    // return wide strings. The observable consequence is that an application
    // which never sets a prompt gets `?` rather than `? `. Reproducing it
    // requires the constructor to leave `p_wide` at 0 here.
    0
}

// [spec:libedit:def:prompt.prompt-end-fn]
// [spec:libedit:sem:prompt.prompt-end-fn]
pub(crate) fn prompt_end(el: &mut EditLine) {
    // Empty, because the module owns no heap: `p_func` points at caller code
    // or at one of the two statics above, the prompt string belongs to the
    // callback, and the literal byte strings belong to the literal table.
    // Note what it does *not* do — restore the defaults, or clear `p_ignore`
    // — which is unobservable because nothing may use the handle afterwards.
    let _ = el;
}

// [spec:libedit:def:prompt.prompt-set-fn]
// [spec:libedit:sem:prompt.prompt-set-fn]
/// `prf` is optional because a NULL function is the documented way to ask
/// for the built-in default back.
pub(crate) fn prompt_set(
    el: &mut EditLine,
    prf: Option<ElPfuncT>,
    c: u32,
    op: i32,
    wide: i32,
) -> i32 {
    // Step 1: unlike `prompt_print` and `prompt_get`, this one does
    // recognise `EL_PROMPT_ESC` as the left-hand prompt. `EL_RPROMPT`,
    // `EL_RPROMPT_ESC` and any unrecognised `op` land on the right.
    let left = op == EL_PROMPT || op == EL_PROMPT_ESC;
    let p = if left {
        &mut el.el_prompt
    } else {
        &mut el.el_rprompt
    };

    // Step 2: a NULL function restores the built-in default for the selected
    // side, chosen by the same test as step 1.
    p.p_func = match prf {
        None => {
            if left {
                Some(prompt_default)
            } else {
                Some(prompt_default_r)
            }
        }
        Some(f) => Some(f),
    };

    // Step 3: unconditional. Both `el_set` and `el_wset` pass 0 for the
    // non-`_ESC` ops, so setting a prompt with `EL_PROMPT`/`EL_RPROMPT`
    // clears an escape character installed earlier with the `_ESC` form.
    p.p_ignore = c;

    // Step 4: for the rprompt this is what stops the previous callback's
    // width from leaking into the next redraw's fit test.
    p.p_pos.v = 0;
    p.p_pos.h = 0;

    // Step 5: the only place `p_wide` is ever assigned — 1 from `el_wset`,
    // 0 from `el_set`. `prompt_init` leaves it alone.
    p.p_wide = wide;

    0
}

// [spec:libedit:def:prompt.prompt-get-fn]
// [spec:libedit:sem:prompt.prompt-get-fn]
/// Both out-parameters keep the C's nullability: a NULL `prf` is the one
/// failure path, and a NULL `c` simply skips the escape-character store.
pub(crate) fn prompt_get(
    el: &mut EditLine,
    prf: Option<&mut Option<ElPfuncT>>,
    c: Option<&mut u32>,
    op: i32,
) -> i32 {
    // Step 1: the only failure path, taken before anything is written.
    let Some(prf) = prf else {
        return -1;
    };

    // Step 2. BUG, reproduced: this is missing the `EL_PROMPT_ESC` arm that
    // `prompt_set` has, so `el_get`/`el_wget` with `EL_PROMPT_ESC` — the op
    // most likely to be used to read an escape character back — report the
    // *right-hand* prompt's callback and escape character. Since both
    // getters also pass a NULL `c` for plain `EL_PROMPT`, there is no route
    // through the public API to retrieve `el_prompt.p_ignore` at all.
    // ERR-core-api-14.
    let p = if op == EL_PROMPT {
        &el.el_prompt
    } else {
        &el.el_rprompt
    };

    // Step 3. The C re-tests `prf` against NULL here; step 1 already
    // guaranteed it is non-NULL, so that test is dead and is not ported.
    // What comes back is the raw stored pointer, `prompt_default`/
    // `prompt_default_r` included, with no indication of `p_wide`.
    *prf = p.p_func;

    // Step 4: a NULL `c` is legal and simply skips the store.
    if let Some(c) = c {
        *c = p.p_ignore;
    }

    // Step 5.
    0
}
