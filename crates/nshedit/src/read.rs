//! Ported from `src/read.c`; rules live in `docs/spec/port/src/read.md`.
//!
//! # Two blocks of the C are deliberately absent
//!
//! Both are conditionally compiled on macros `read.c` never gets a definition
//! for, so neither exists in the build this port is measured against
//! (ERR-input-43, disposition `fix — not ported`):
//!
//! - `el_wgets`'s `FIONREAD` typeahead pre-check. `read.c` includes neither
//!   `<sys/ioctl.h>` nor any header that transitively defines `FIONREAD`, so
//!   on a glibc build the block and its unique exit path — NULL with
//!   `*nread == 0` and `errno` forced to 0 — do not exist, and the switch to
//!   raw mode is left to [`el_wgetc`]'s lazy `tty_rawmode`. Porting it would
//!   *add* an exit path a C caller cannot currently observe, so it is not
//!   ported and [`el_wgets`] has no `*nread == 0`-with-NULL return.
//! - `read_fixio`'s `FIONBIO` sub-block, for the same header reason. The
//!   surviving `fcntl` half is live; see [`read_fixio`].

use core::ffi::c_int;
use core::ptr;
use core::sync::atomic::Ordering;
use std::fs::File;
use std::io::Read;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;

use crate::chared::{NOP, ch_enlargebufs, ch_reset};
use crate::chartype::{ct_enc_width, ct_encode_string};
use crate::el::{EDIT_DISABLED, EditLine, ElActionT, FIXIO, HANDLE_SIGNALS, NO_TTY, UNBUFFERED};
use crate::errno::{self, EBADF, EILSEQ, EINTR, EIO, EWOULDBLOCK};
use crate::fcns::{ED_INSERT, ED_SEQUENCE_LEAD_IN, VI_DELETE_PREV_CHAR};
use crate::histedit::{
    CC_ARGHACK, CC_CURSOR, CC_EOF, CC_FATAL, CC_NEWLINE, CC_NORM, CC_REDISPLAY, CC_REFRESH,
    CC_REFRESH_BEEP, ElRfuncT,
};
use crate::keymacro::{KeymacroValueT, XK_CMD, XK_NOD, XK_STR, keymacro_get};
use crate::locale;
use crate::map::{ElMapCurrent, MAP_VI, N_KEYS};
use crate::refresh::{re_clear_display, re_clear_lines, re_refresh, re_refresh_cursor};
use crate::sig::{sig_clr, sig_handler, sig_set, signo};
use crate::terminal::{terminal_beep, terminal_flush};
use crate::tty::{tty_cookedmode, tty_rawmode};

/// C: `#define EL_MAXMACRO 10` — the macro nesting limit.
pub const EL_MAXMACRO: usize = 10;

/// C: `CONTROL('d')` — what the `UNBUFFERED` `CC_EOF` arm appends.
const CONTROL_D: u32 = 0x04;

// [spec:libedit:def:read.macros]
/// The macro pushback stack.
pub struct Macros {
    /// C: `wchar_t **macro` — up to `EL_MAXMACRO` owned strings, innermost
    /// last. `macro` is a Rust keyword, so the name is written `r#macro`;
    /// it is still the C's field name.
    ///
    /// The C allocates ten slots once and uses `level` to say how many are
    /// live; here the vector holds *exactly* the live entries, so the
    /// invariant is `level == r#macro.len() as i32 - 1` and "empty" is the
    /// default value of the field. Two things fall out of that, both of them
    /// asked for by the rules: `read_init`'s uninitialised-`level` undefined
    /// behaviour (ERR-input-01) becomes unrepresentable, and the freed slots
    /// `read_clearmacros` leaves dangling and the stale alias `read_pop`
    /// leaves above `level` (ERR-input-20, disposition `fix`) cannot exist.
    pub r#macro: Vec<Vec<u32>>,
    /// Index of the innermost live macro, -1 when none is running.
    pub level: i32,
    /// Read position within `macro[level]`.
    pub offset: i32,
}

// [spec:libedit:def:read.el-read-t]
/// The character-reading state, hung off `EditLine::el_read`.
pub struct ElReadT {
    pub macros: Macros,
    /// Function to read a character.
    pub read_char: Option<ElRfuncT>,
    /// The `errno` the last read failed with, surfaced through
    /// `EL_GETCFN`'s error reporting.
    pub read_errno: i32,
}

// [spec:libedit:def:read.read-init-fn]
// [spec:libedit:sem:read.read-init-fn]
/// Initialize the read stuff. 0 on success, -1 if an allocation failed.
pub(crate) fn read_init(el: &mut EditLine) -> i32 {
    // Steps 1-4 collapse into one initialiser, and the whole of step 5 — the
    // `out:` label — goes with them.
    //
    // ERR-input-01 (UB, disposition `define`): the C assigns `ma->level = -1`
    // *after* the slot allocation that can fail, and its failure path runs
    // `read_end` -> `read_clearmacros` over an indeterminate `level` from
    // `malloc`, freeing garbage pointers out of a NULL array. The definition
    // chosen is the one the rule names: a representation whose default *is*
    // "empty". `Macros::r#macro` holds only live entries, so an empty vector
    // and `level == -1` are the same state and there is no window in which
    // `level` is anything else.
    //
    // Neither allocation can fail either: `Box` and `Vec` abort on
    // out-of-memory rather than returning null, so the C's -1 is unreachable
    // and this function always reports success. That is what makes
    // ERR-core-api-03 — `read_init` failure being unsurvivable because
    // `el_end` then calls `read_end` on a NULL `el_read` — unreachable as
    // well; `read_end` tolerates it regardless.
    //
    // ERR-input-40 (disposition `fix`): the C's `el_read` comes from `malloc`
    // and never assigns `read_errno`, leaving it indeterminate until
    // `el_wgets` zeroes it. Zeroed here.
    el.el_read = Some(Box::new(ElReadT {
        macros: Macros {
            r#macro: Vec::new(),
            level: -1,
            offset: 0,
        },
        read_char: Some(read_char as ElRfuncT),
        read_errno: 0,
    }));
    0
}

// [spec:libedit:def:read.read-end-fn]
// [spec:libedit:sem:read.read-end-fn]
/// Free the data structures used by the read stuff.
pub(crate) fn read_end(el: &mut EditLine) {
    // ERR-input-16 (UB, disposition `define — tolerate an uninitialised read
    // subsystem`): the C dereferences `el->el_read` with no NULL check, so a
    // double call or a call before `read_init` faults. `Option` makes the
    // check structural and the definition is "there was nothing to tear down".
    if let Some(rd) = el.el_read.as_deref_mut() {
        // Step 1.
        read_clearmacros(&mut rd.macros);
    }
    // Steps 2 and 3. Dropping the box frees the slot array with it, and the
    // `Option` is the C's two NULL assignments; the type makes the dangling
    // `el_read` the C leaves on other paths unrepresentable.
    el.el_read = None;
}

// [spec:libedit:def:read.el-read-setfn-fn]
// [spec:libedit:sem:read.el-read-setfn-fn]
/// Set the read-char function to the one provided. `None` is the C's
/// `EL_BUILTIN_GETCFN` — a NULL `el_rfunc_t` — and restores [`read_char`].
pub fn el_read_setfn(el_read: &mut ElReadT, rc: Option<ElRfuncT>) -> i32 {
    // Steps 1 and 2. No validation of any kind, exactly as the C.
    el_read.read_char = Some(rc.unwrap_or(read_char as ElRfuncT));
    // Step 3: unconditional, so `el_set(EL_GETCFN, ...)` always succeeds.
    0
}

// [spec:libedit:def:read.el-read-getfn-fn]
// [spec:libedit:sem:read.el-read-getfn-fn]
/// Return the current read-char function, or `None` when it is the builtin
/// one — the C's `EL_BUILTIN_GETCFN`.
pub fn el_read_getfn(el_read: &mut ElReadT) -> Option<ElRfuncT> {
    // The exact inverse of `el_read_setfn`, so a get/set round trip is
    // lossless and the builtin's address is never handed out.
    //
    // `fn_addr_eq` rather than `==`: comparing function pointers directly is
    // what `clippy::fn_address_comparisons` exists to reject, and this is the
    // sanctioned spelling of the C's `el_read->read_char == read_char`.
    match el_read.read_char {
        Some(f) if std::ptr::fn_addr_eq(f, read_char as ElRfuncT) => None,
        // `None` — only reachable before `read_init` — answers `None` too,
        // which is the same "the builtin is installed" report the caller
        // would get after initialisation.
        other => other,
    }
}

// The POSIX descriptor-flag calls [`read_fixio`]'s would-block arm is
// written against.
//
// `plan/decisions/platform-layer.md` put `fcntl` in `nshedit-plat`, through
// rustix, so the stub module that used to stand here — with both calls
// reporting a permanent failure, which landed this arm on the C's
// both-sub-blocks-absent -1 — is gone and the name is an import.
//
// With the syscall present the arm recovers and — this is the point, not an
// accident — **permanently clears `O_NONBLOCK`/`O_NDELAY` on the caller's
// input descriptor**, normally the process's shared standard input, saving
// and restoring nothing (ERR-input-21). `sem:read.read-fixio-fn` says a port
// must either reproduce that side effect or register it as a divergence; it
// is reproduced.
use nshedit_plat as plat;

// [spec:libedit:def:read.read-fixio-fn]
// [spec:libedit:sem:read.read-fixio-fn]
/// Try to recover from a failed read; `e` is the `errno` it failed with.
fn read_fixio(fd: i32, e: i32) -> i32 {
    match e {
        // The C's `case -1:` label is never a real `errno` value; it exists
        // only so the compiler cannot prove the block unreachable when every
        // other label is preprocessed away. It shares this arm with the
        // would-block case, so it is kept.
        //
        // `EAGAIN` earns its own C label only when the platform makes it
        // distinct from `EWOULDBLOCK` *and* `POSIX` is defined; libedit's
        // build never defines `POSIX` and on Linux the two values are equal,
        // so one label is the whole condition.
        -1 | EWOULDBLOCK => {
            // The descriptor is non-blocking and had nothing to give. C: seed
            // `e = 0`, then let each compiled sub-block raise it to 1, and
            // `return e ? 0 : -1`. With the `fcntl` sub-block present that is
            // an unconditional 0 on the path that reaches the end, so the
            // seed only ever decides the both-sub-blocks-absent case, which
            // is `fcntl_getfl` answering `None` here.
            let Some(fl) = plat::fcntl_getfl(fd) else {
                return -1;
            };
            if !plat::fcntl_setfl(fd, fl & !plat::O_NDELAY) {
                return -1;
            }
            // The `FIONBIO` sub-block would go here; it does not compile on
            // glibc and is not ported (ERR-input-43).
            0
        }
        // A pure "retry me": no side effects whatsoever. This is the only arm
        // that does anything today, and the only one `EL_SAFEREAD` was
        // realistically enabled for.
        EINTR => 0,
        _ => -1,
    }
}

// [spec:libedit:def:read.el-wpush-fn]
// [spec:libedit:sem:read.el-wpush-fn]
/// Push a macro onto the back of the pending queue. `None` is the C's NULL
/// `str`, a live call site from `read_getcmd` whose only effect is the beep.
pub fn el_wpush(el: &mut EditLine, str: Option<&[u32]>) {
    // IMPORTANT, and the one thing about this module a reader will get wrong
    // from the API shape: despite the "level" vocabulary the queue is FIFO,
    // not LIFO (ERR-input-42). This writes at the BACK while `el_wgetc`
    // always reads `macro[0]`, the FRONT, so a push issued while a macro is
    // already draining is queued BEHIND the remainder of that macro rather
    // than spliced in ahead of it. `sem:histedit.el-wpush-fn` claims the
    // opposite ("the most recently pushed string is consumed first"); the C
    // is what settles it and this rule is the one derived from the C. Do not
    // "fix" this into a stack.
    let pushed = match (str, el.el_read.as_deref_mut()) {
        // Step 1: at most `EL_MAXMACRO` entries, levels 0 through 9.
        (Some(s), Some(rd)) if rd.macros.level + 1 < EL_MAXMACRO as i32 => {
            // Step 1b, `wcsdup`: an owning copy, so the caller's storage may
            // be freed or reused immediately. `wcsdup` stops at the first
            // NUL, so an embedded one truncates here as it does there.
            let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            rd.macros.level += 1;
            rd.macros.r#macro.push(s[..n].to_vec());
            // Step 1d — decrementing `level` back after a failed `wcsdup` —
            // is unreachable: `Vec` aborts on out-of-memory. The C's claim
            // that a full queue and an allocation failure are observationally
            // identical therefore still holds, with only the first reachable.
            true
        }
        _ => false,
    };
    if pushed {
        // Step 1c. `ma->offset` is deliberately NOT touched: it is the read
        // cursor into `macro[0]`, the entry currently draining, and only
        // `read_pop` and `read_clearmacros` reset it.
        return;
    }
    // Step 2: `str` was NULL, or the queue was full. The function returns
    // void, so the beep is the only signal a caller ever gets that the push
    // was dropped. An empty string is *not* a failure — it takes a slot and
    // `el_wgetc` pops it unread.
    terminal_beep(el);
    terminal_flush(el);
}

// [spec:libedit:def:read.read-getcmd-fn]
// [spec:libedit:sem:read.read-getcmd-fn]
/// Get the next command from the input stream: 0 on success, -1 on EOF or
/// error.
fn read_getcmd(el: &mut EditLine, cmdnum: &mut ElActionT, ch: &mut u32) -> i32 {
    /// C: `static const wchar_t meta = (wchar_t)0x80;`
    const META: u32 = 0x80;

    let cmd;
    loop {
        // Step 1. Note the test is `!= 1`, so a `tty_rawmode` failure
        // reported as end of file (ERR-input-24) lands here too. Neither
        // out-parameter is meaningful after this -1.
        if el_wgetc(el, ch) != 1 {
            return -1;
        }

        // Step 2. Before the map lookup, so `ESC a` looks up 0xE1. For a
        // character that already has bit 7 set the OR is a no-op, which
        // silently merges the meta and non-meta bindings for those.
        //
        // The `KANJI` variant that would sit above this is never defined
        // anywhere in the tree and is not ported (ERR-input-43).
        if el.el_state.metanext != 0 {
            el.el_state.metanext = 0;
            *ch |= META;
        }

        // Step 3. Wide characters above U+00FF are never looked up in a key
        // map: they go straight to self-insert and cannot be rebound. The C's
        // `(unsigned char)` cast below it is redundant, since this test has
        // already excluded everything above 255.
        let mut action = if *ch >= N_KEYS as u32 {
            ED_INSERT
        } else {
            let i = *ch as usize;
            match el.el_map.current {
                ElMapCurrent::Key => el.el_map.key[i],
                ElMapCurrent::Alt => el.el_map.alt[i],
            }
        };

        // Step 4. A multi-key binding starts here. `keymacro_get` walks the
        // trie from the root, consuming further characters through
        // `el_wgetc` with no timeout of any kind, and leaves the LAST
        // character it read in `*ch`.
        if action == ED_SEQUENCE_LEAD_IN {
            let mut val = KeymacroValueT::Str(Vec::new());
            match keymacro_get(el, ch, &mut val) {
                XK_CMD => match val {
                    // A complete binding resolving to a command. `*ch` is now
                    // the FINAL character of the sequence, and that is what
                    // reaches the command function and `el_state.thisch`.
                    KeymacroValueT::Cmd(c) => action = c,
                    // Not producible by `keymacro_get`; defined the same way
                    // as the C's `EL_ABORT` default below.
                    KeymacroValueT::Str(_) => return -1,
                },
                XK_STR => {
                    // Either a real string binding, or a MISMATCH — the trie
                    // answers `XK_STR` with a NULL `val.str` when no sibling
                    // matches at some depth, and sets `*ch` to `L'\0'`. The C
                    // calls `el_wpush` for both; the null pointer merely
                    // beeps, and the characters the trie walk already
                    // consumed are DISCARDED, so an unrecognised escape
                    // sequence swallows its own bytes.
                    //
                    // `crate::keymacro` represents that NULL as `Str` with an
                    // empty buffer, so an empty string binding and a mismatch
                    // are the same value here and both take the beep. That
                    // conflation is the enum's, not this function's — a
                    // genuinely empty `bind -s` value would occupy a slot in
                    // the C and be popped unread, and does not here.
                    match &val {
                        KeymacroValueT::Str(s) if !s.is_empty() => {
                            el_wpush(el, Some(s.as_slice()));
                        }
                        _ => el_wpush(el, None),
                    }
                    // The action is left as `ED_SEQUENCE_LEAD_IN`, which is
                    // precisely what makes the do/while repeat. A
                    // self-referential string binding loops forever, which
                    // the C does not guard against either; a push that fails
                    // because the queue is full still makes progress, because
                    // the next pass reads a genuinely new character.
                }
                // `el_wgetc` failed part-way through the sequence.
                XK_NOD => return -1,
                // C: `EL_ABORT((el->el_errfile, "Bad XK_ type \n"))`, i.e.
                // `abort()`. Unreachable — the trie only ever yields the
                // three values above — and defined here as abandoning the
                // command rather than killing the process.
                _ => return -1,
            }
        }

        // The C's `while (cmd == ED_SEQUENCE_LEAD_IN)`. Note this tests the
        // action AFTER the trie has had its say, so a command binding that
        // resolves to the lead-in action itself also repeats.
        if action != ED_SEQUENCE_LEAD_IN {
            cmd = action;
            break;
        }
    }
    // Step 5. `*cmdnum` is written only on this path.
    *cmdnum = cmd;
    0
}

/// What `mbrtowc(3)` distinguishes and [`locale::mbrtowc`] does not.
///
/// `crate::locale` folds `(size_t)-1` and `(size_t)-2` into one `Bad`, because
/// every other caller in the crate tests `clen < 0` and treats them alike.
/// [`read_char`] is the exception and the difference is load-bearing there:
/// invalid resynchronises and discards, incomplete reads another byte.
enum Decoded {
    /// A whole character.
    Char(u32),
    /// C: `(size_t)-1` — an invalid sequence, and no longer or shorter input
    /// would help.
    Invalid,
    /// C: `(size_t)-2` — a valid initial subsequence of some character.
    Incomplete,
}

/// Splits [`locale::mbrtowc`]'s `Bad` back into the two answers `mbrtowc(3)`
/// gives, without a second copy of the codec.
///
/// The test for "incomplete" is definitional rather than table-driven: bytes
/// are an initial subsequence exactly when *some* continuation completes them,
/// so the accumulator is padded with continuation bytes and re-offered to the
/// same decoder. A longer decode means the input was a prefix; anything else
/// means it was invalid outright. That keeps overlong forms and surrogate
/// encodings — which glibc rejects as soon as they are detectable, before the
/// sequence is complete — on the `Invalid` side, as measured: `\xE0\x80` and
/// `\xED\xA0` are `(size_t)-1` there and are here too.
fn decode(cs: locale::Charset, bytes: &[u8]) -> Decoded {
    match locale::mbrtowc(cs, bytes) {
        locale::Mb::Char(c, _) => Decoded::Char(c),
        locale::Mb::Bad => {
            let mut padded = [0x80u8; locale::MB_LEN_MAX * 2];
            padded[..bytes.len()].copy_from_slice(bytes);
            match locale::mbrtowc(cs, &padded) {
                locale::Mb::Char(_, used) if used > bytes.len() => Decoded::Incomplete,
                _ => Decoded::Invalid,
            }
        }
    }
}

/// C: `read(fd, buf, (size_t)1)` — always exactly one byte.
///
/// `Ok(0)` is end of file and `Err` carries the `errno` the C reads out of the
/// global. Going through `File` rather than the syscall is what
/// `plan/decisions/no-c-ffi.md` leaves available; `ManuallyDrop` is what keeps
/// the borrow from closing the application's descriptor, which libedit never
/// does. `Read::read` is a single `read(2)` and does not retry on `EINTR`,
/// which the recovery path below depends on.
fn read_byte(fd: i32, out: &mut u8) -> Result<usize, i32> {
    if fd < 0 {
        // What `read(2)` would answer for the descriptor a half-built
        // `EditLine` carries; `el_init_fd` stores a `fileno` of -1
        // undiagnosed, so this is reachable.
        return Err(EBADF);
    }
    // SAFETY: `el_infd` is the application's descriptor and stays open for
    // the life of the `EditLine`; `ManuallyDrop` is what keeps this borrow
    // from closing it.
    let mut f = ManuallyDrop::new(unsafe { File::from_raw_fd(fd) });
    match f.read(core::slice::from_mut(out)) {
        Ok(n) => Ok(n),
        // Every `io::Error` from a raw `read(2)` on Unix carries its `errno`.
        Err(err) => Err(err.raw_os_error().unwrap_or(EIO)),
    }
}

// [spec:libedit:def:read.read-char-fn]
// [spec:libedit:sem:read.read-char-fn]
/// Read a character from the tty. This is the builtin [`ElRfuncT`], so its
/// signature is that type's — `unsafe extern "C"` with the C's raw parameters,
/// because it sits in the same slot an application's `EL_GETCFN` callback
/// does and is reached by the same indirect call. 1 for a character, 0 for
/// end of input, -1 for an error.
///
/// # Safety
///
/// `el` must be a live `EditLine` and `cp` a writable `wchar_t`, which is
/// what `sem:read.el-wgetc-fn` passes and what any caller reaching this
/// through `el_read.read_char` has.
unsafe extern "C" fn read_char(el: *mut EditLine, cp: *mut u32) -> c_int {
    // SAFETY: the caller's obligation above, plus the C's own requirement
    // that the two do not alias — `el_wgetc` passes storage of its caller's,
    // never a field of `*el`.
    let el = unsafe { &mut *el };
    // SAFETY: as above.
    let cp = unsafe { &mut *cp };
    // Read the initialiser carefully — the sense is inverted from what the
    // name suggests. `FIXIO` (set by `el_set(EL_SAFEREAD, 1)`) makes `tried`
    // start false, which is what ENABLES the recovery path. With `FIXIO`
    // clear — the DEFAULT — it starts true and `read_fixio` is never
    // consulted, so every `read` failure, `EINTR` included, fails the call at
    // once.
    let mut tried = (el.el_flags & FIXIO) == 0;
    // The only read-ahead in the module, and it never survives the call: at
    // most one byte is carried across a resynchronisation *inside* the call,
    // and nothing at all between calls. `read` is always issued for exactly
    // one byte, so libedit never consumes input it does not decode.
    let mut cbuf = [0u8; locale::MB_LEN_MAX];
    let mut cbp: usize = 0;
    // Restored after a successful recovery, so a recovered error leaves no
    // trace in `errno`.
    let save_errno = errno::errno();
    let cs = locale::charset();

    'again: loop {
        // Step 1. The C dereferences `el->el_signal` unchecked; defined here
        // as "no signal state, so nothing is pending and nothing to clear".
        if let Some(sig) = el.el_signal.as_deref() {
            sig.sig_no.store(0, Ordering::Relaxed);
        }

        // Step 2.
        let num_read = loop {
            let e = match read_byte(el.el_infd, &mut cbuf[cbp]) {
                Ok(n) => break n,
                Err(e) => e,
            };

            // Step 2b, and the module's half of the deferred-signal design.
            //
            // `sem:sig.sig-handler-fn` no longer describes an installed
            // handler: the ABI crate's real handler does nothing but store
            // `signo` into this atomic, and `crate::sig::sig_handler` is the
            // *body*, to be run from ordinary context by whoever notices. The
            // read loop is that whoever — it is the only place in the library
            // that polls `sig_no` — so the call goes here, before the
            // SIGCONT/SIGWINCH dispatch the C already had. Everything the C
            // did inside async-signal context happens at this line instead:
            // the tty is put back into cooked mode (or raw, after a resume),
            // the displaced disposition is restored, and the signal is
            // re-raised.
            //
            // It runs for ALL seven trapped signals, not just the two the C
            // names below. The rule does not mention the terminating and
            // stopping ones because the C never comes back from them — the
            // handler re-raises from inside the `read` and the process dies
            // or stops there — but deferred, they arrive here as an `EINTR`
            // with `sig_no` set and nothing else would ever chain them. The
            // cost is stated rather than hidden: the process now dies just
            // after the interrupted `read` returns instead of inside it.
            //
            // `swap` rather than a load: the pending signal is consumed. The
            // C leaves `sig_no` set until the next `again` clears it, which
            // it can afford because the only arm that falls through to (c)
            // with it still set is the one it never returns from. Here that
            // arm does return — through a `read_fixio` retry — and a second
            // failure would otherwise chain the same signal twice, restoring
            // and re-raising it again.
            let pending = el
                .el_signal
                .as_deref()
                .map_or(0, |sig| sig.sig_no.swap(0, Ordering::Relaxed));
            if pending != 0 {
                sig_handler(el, pending);
            }
            match pending {
                signo::SIGCONT => {
                    // C: `el_wset(el, EL_REFRESH)`, which is exactly these
                    // three calls (`el.c`'s `EL_REFRESH` case). The varargs
                    // setter itself lives in the ABI crate, so the option's
                    // body is written out.
                    re_clear_display(el);
                    re_refresh(el);
                    terminal_flush(el);
                    // FALLTHROUGH, as the C marks it.
                    sig_set(el);
                    continue 'again;
                }
                signo::SIGWINCH => {
                    // Re-arm every handler: `sig_handler` restores the
                    // previous disposition before returning, which makes
                    // libedit's handlers one-shot. Then re-issue the read
                    // from step 1, discarding `e`. This retry is NOT counted
                    // against `tried` and is not bounded — an unending stream
                    // of `SIGWINCH` spins here forever, redoing the redisplay
                    // each time.
                    sig_set(el);
                    continue 'again;
                }
                // Anything else, including 0, falls through to (c).
                _ => {}
            }

            // Step 2c. Granted AT MOST ONCE PER CALL, and `tried` survives
            // the `goto again` the decoder takes, so a multi-byte character
            // whose bytes are interrupted twice fails on the second. Note the
            // loop back is to the read, NOT to `again`, so `sig_no` is not
            // re-zeroed.
            if !tried && read_fixio(el.el_infd, e) == 0 {
                errno::set_errno(save_errno);
                tried = true;
            } else {
                // Step 2d.
                errno::set_errno(e);
                *cp = 0;
                return -1;
            }
        };

        // Step 3. End of file is only ever detected on a fresh byte, so an
        // EOF in the middle of a partial sequence discards the accumulated
        // bytes and reports plain EOF.
        if num_read == 0 {
            *cp = 0;
            return 0;
        }

        // Step 4. One byte now sits at `cbuf[cbp]`.
        loop {
            cbp += 1;
            // The WHOLE accumulator is re-decoded from a clean conversion
            // state on every added byte. The C notes this "only works because
            // UTF-8 is stateless"; for a genuinely stateful encoding the
            // conversion is wrong and no shift state is carried, between
            // bytes or between characters (ERR-input-26, disposition
            // `reproduce` — the rule calls it specified behaviour the port
            // inherits). `crate::locale` models no stateful codec at all, so
            // there is nothing here that could carry state even by accident.
            match decode(cs, &cbuf[..cbp]) {
                // Step 4e. ANY other return, including 0: a return of 0 means
                // a NUL wide character was decoded from an embedded NUL byte,
                // and it is reported as a SUCCESSFUL read of `L'\0'` —
                // distinguishable from end of file only by this 1 versus 0,
                // never by `*cp`.
                Decoded::Char(c) => {
                    *cp = c;
                    return 1;
                }
                Decoded::Invalid => {
                    if cbp > 1 {
                        // Step 4c, first half: resynchronise on the last
                        // byte and re-decode it alone. The earlier bytes of
                        // the bad sequence are discarded silently.
                        cbuf[0] = cbuf[cbp - 1];
                        cbp = 0;
                        continue;
                    }
                    // Step 4c, second half: the lone byte is itself invalid.
                    // Discard it and read a fresh one. NO error is reported
                    // and nothing is returned to the caller — invalid input
                    // is skipped, never surfaced, and a stream of garbage
                    // bytes makes this function block indefinitely without
                    // ever returning (ERR-input-25, disposition `reproduce`).
                    cbp = 0;
                    continue 'again;
                }
                Decoded::Incomplete => {
                    // Step 4d. The bound is exact: the read always targets
                    // `cbuf[cbp]` with `cbp < MB_LEN_MAX`, so the accumulator
                    // cannot overflow.
                    if cbp >= locale::MB_LEN_MAX {
                        errno::set_errno(EILSEQ);
                        *cp = 0;
                        return -1;
                    }
                    continue 'again;
                }
            }
        }
    }
}

// [spec:libedit:def:read.read-pop-fn]
// [spec:libedit:sem:read.read-pop-fn]
/// Drop the draining macro and shuffle the queue down.
fn read_pop(ma: &mut Macros) {
    // ERR-input-17 (UB, disposition `define — make the precondition
    // explicit`): the C has no `ma->level >= 0` guard, so calling this on an
    // empty queue frees `macro[0]` a second time and drives `level` to -2.
    // Both live call sites in `el_wgetc` satisfy the precondition, so this is
    // a latent hazard rather than a live bug; the definition chosen is that
    // popping an empty queue does nothing.
    if ma.level < 0 {
        return;
    }
    // Steps 1 and 2 together. `remove(0)` frees the front entry's string and
    // shifts every remaining entry one slot toward the front — the queue is
    // FIFO, so the FRONT is what leaves. The stale alias the C's shift leaves
    // above the new `level` (ERR-input-20) cannot exist here.
    ma.r#macro.remove(0);
    // Step 3.
    ma.level -= 1;
    // Step 4, so the new front entry is read from its first character.
    ma.offset = 0;
    // `level == 0` works out exactly as the C says: nothing shifts, `level`
    // becomes -1, and the queue now tests as empty.
}

// [spec:libedit:def:read.read-clearmacros-fn]
// [spec:libedit:sem:read.read-clearmacros-fn]
/// Discard every queued macro.
fn read_clearmacros(ma: &mut Macros) {
    // Step 1. The C frees back to front and leaves the slots holding dangling
    // pointers; the order of the frees is not observable and the dangling
    // slots are ERR-input-20, whose disposition is `fix — clear the slots`.
    // Clearing the vector is both.
    ma.r#macro.clear();
    ma.level = -1;
    // Step 2.
    ma.offset = 0;
}

/// C: `(*el->el_read->read_char)(el, cp)` — the installed callback, read out
/// fresh at each call because a client callback may replace itself.
///
/// The C dereferences `el_read` unchecked and would call through a NULL
/// function pointer if `read_init` had not run. Defined here as 0, end of
/// file: it is the same answer `el_wgetc` already gives for the other
/// cannot-proceed condition, and every caller in the library tests `!= 1`.
fn read_char_cb(el: &mut EditLine, cp: &mut u32) -> i32 {
    let f = el.el_read.as_deref().and_then(|rd| rd.read_char);
    match f {
        // SAFETY: `f` is [`read_char`] or a hook installed through
        // `el_set(EL_GETCFN, ...)`, whose contract
        // (`def:histedit.el-rfunc-t-edit-line-wchar-t`) is a C function taking
        // the `EditLine *` it was registered against and a writable
        // `wchar_t *`. `el` and `cp` are exactly those, live, exclusively
        // borrowed here and non-aliasing; both borrows are released for the
        // duration of the call, which the hook may use to re-enter libedit.
        Some(f) => unsafe { f(ptr::from_mut(el), ptr::from_mut(cp)) },
        None => 0,
    }
}

// [spec:libedit:def:read.el-wgetc-fn]
// [spec:libedit:sem:read.el-wgetc-fn]
/// Read a wide character, from the macro queue while one is draining and
/// from the tty otherwise.
pub fn el_wgetc(el: &mut EditLine, cp: &mut u32) -> i32 {
    // Step 1: unconditional, and before anything else — including when the
    // character will come from a macro and no I/O is about to block. This is
    // what guarantees the prompt and any redisplay are on the wire before the
    // process waits for a key.
    terminal_flush(el);

    // Step 2. The queue is always read from `macro[0]`, the OLDEST entry.
    if let Some(rd) = el.el_read.as_deref_mut() {
        let ma = &mut rd.macros;
        loop {
            // (a) The queue is empty.
            if ma.level < 0 {
                break;
            }
            // (b) The front entry is exhausted — the C's `L'\0'` test, which
            // here is the end of the copied string. This also silently
            // disposes of empty pushed strings.
            if ma.offset as usize >= ma.r#macro[0].len() {
                read_pop(ma);
                continue;
            }
            // (c)
            *cp = ma.r#macro[0][ma.offset as usize];
            ma.offset += 1;
            // (d) The character just taken was the last one, so pop
            // IMMEDIATELY rather than waiting for the next call. The C
            // comments this "Needed for QuoteMode On": it guarantees
            // `ma->level` has already dropped by the time the caller acts on
            // the character, which is what the quoted-insert path and the
            // `macros.level < 0` test in `el_wgets` observe.
            if ma.offset as usize >= ma.r#macro[0].len() {
                read_pop(ma);
            }
            // (e) Macro characters bypass steps 3-6 entirely: the tty is NOT
            // switched to raw mode while a macro is draining, `read_errno` is
            // not touched, and the character is delivered verbatim —
            // including values the multibyte decoder could never have
            // produced, since `el_wpush` copies arbitrary `wchar_t`.
            return 1;
        }
    }

    // Step 3. A terminal-setup failure is reported as end of file and is
    // indistinguishable from one by any caller (ERR-input-24, disposition
    // `reproduce`). `read_errno` is NOT set here, `*cp` is NOT written, and
    // `errno` is left as whatever `tty_rawmode` produced.
    if tty_rawmode(el) < 0 {
        return 0;
    }

    // Step 4.
    let num_read = read_char_cb(el, cp);

    // Step 5. Remember the original reason for a read failure, so `el_wgets`
    // can restore it after cleanup (`terminal_flush`, `tty_cookedmode`,
    // `sig_clr`) that may clobber `errno`. Never cleared here; `el_wgets`
    // zeroes it on entry and nothing else writes it.
    if num_read < 0
        && let Some(rd) = el.el_read.as_deref_mut()
    {
        rd.read_errno = errno::errno();
    }

    // Step 6, unchanged. `*cp` holds whatever the callback left; the builtin
    // stores `L'\0'` on both 0 and -1, but a client callback need not.
    num_read
}

// [spec:libedit:def:read.read-prepare-fn]
// [spec:libedit:sem:read.read-prepare-fn]
/// Set up for a read: signals, raw mode, resize, and the prompt.
pub fn read_prepare(el: &mut EditLine) {
    // Step 1. Runs FIRST, and importantly runs even for `NO_TTY`.
    if (el.el_flags & HANDLE_SIGNALS) != 0 {
        sig_set(el);
    }
    // Step 2. Nothing below runs — no resize, no prompt, no line reset. In
    // practice `el_wgets` never reaches here under `NO_TTY`, but
    // `el_set(EL_UNBUFFERED, 1)` can.
    if (el.el_flags & NO_TTY) != 0 {
        return;
    }
    // Step 3. In every other configuration the switch to raw mode is left to
    // `el_wgetc`, which does it lazily on the first character not supplied by
    // a macro. The result is discarded; if it failed, `el_wgetc` hits it
    // again and reports it as end of file.
    if (el.el_flags & (UNBUFFERED | EDIT_DISABLED)) == UNBUFFERED {
        let _ = tty_rawmode(el);
    }
    // Step 4. Unconditional on every line, not cached: the C notes this is
    // "relatively cheap, and things go terribly wrong if we have the wrong
    // size".
    crate::el::el_resize(el);
    // Step 5.
    re_clear_display(el);
    // Step 6.
    ch_reset(el);
    // Step 7 — this is what prints the prompt.
    re_refresh(el);
    // Step 8. In buffered mode the flush is left to `el_wgetc`, which flushes
    // before every blocking read.
    if (el.el_flags & UNBUFFERED) != 0 {
        terminal_flush(el);
    }
    // The macro queue and `read_errno` are NOT touched, so pushed macros
    // survive a `read_prepare` and are consumed by the line it prepares.
}

// [spec:libedit:def:read.read-finish-fn]
// [spec:libedit:sem:read.read-finish-fn]
/// Undo [`read_prepare`]: cooked mode and the signal handlers.
pub fn read_finish(el: &mut EditLine) {
    // Step 1. When `UNBUFFERED` is set the tty is deliberately LEFT in raw
    // mode, because the caller is mid-line and will call back in. Neither of
    // libedit's own callers reaches this branch — `el_wgets` calls this only
    // when the flag is clear, and `el_set(EL_UNBUFFERED, 0)` clears it first.
    if (el.el_flags & UNBUFFERED) == 0 {
        let _ = tty_cookedmode(el);
    }
    // Step 2. NOT gated on `UNBUFFERED`: the handlers come off even on the
    // path that leaves the tty raw.
    if (el.el_flags & HANDLE_SIGNALS) != 0 {
        sig_clr(el);
    }
    // Nothing else is touched — the line buffer, the macro queue and
    // `read_errno` all survive.
}

// [spec:libedit:def:read.noedit-wgets-fn]
// [spec:libedit:sem:read.noedit-wgets-fn]
/// Read a line with editing disabled. The C returns `el_line.buffer` or
/// NULL, so this borrows `el` for as long as the caller holds the line.
fn noedit_wgets<'a>(el: &'a mut EditLine, nread: &mut i32) -> Option<&'a [u32]> {
    // This function does NOT reset `lastchar`; `el_wgets` has already decided
    // whether to.
    let mut num;
    loop {
        // Step 1. The read callback DIRECTLY, not through `el_wgetc`.
        // Consequences: the macro queue is never consulted, so `el_wpush` has
        // no effect on this path at all; `tty_rawmode` is never called;
        // `terminal_flush` is never called per character; and `read_errno`
        // is never written.
        //
        // The C hands the callback `lp->lastchar`, so the decoded character
        // lands straight in the line buffer. Rust will not lend a slot of
        // `el.el_line.buffer` out while `el` is also borrowed, so it lands in
        // a local and is stored immediately below — before the space check,
        // which is the order that matters (step 1a).
        let mut c: u32 = 0;
        num = read_char_cb(el, &mut c);
        // Step 2. The loop also ends when the callback returns 0 (end of
        // file) or a negative value (error).
        if num != 1 {
            break;
        }

        // Step 1a. The character has ALREADY been stored; the space check
        // comes after. Writing at `lp->limit` is in bounds — the line
        // allocation always keeps `EL_LEAVE` unused slots above `limit`.
        el.el_line.buffer[el.el_line.lastchar] = c;
        if el.el_line.lastchar + 1 >= el.el_line.limit && ch_enlargebufs(el, 2) == 0 {
            // BREAK without advancing `lastchar`, so the character just read
            // is silently lost and step 4 overwrites it with the NUL
            // terminator (ERR-input-28, disposition `reproduce`).
            break;
        }
        // Step 1b.
        el.el_line.lastchar += 1;
        // Step 1c. The terminator is KEPT in the buffer and counted in
        // `*nread`.
        let last = el.el_line.buffer[el.el_line.lastchar - 1];
        if (el.el_flags & UNBUFFERED) != 0 || last == u32::from(b'\r') || last == u32::from(b'\n') {
            break;
        }
    }

    // Step 3. Discards the ENTIRE partial line accumulated by this call — not
    // just the last character. No other `errno` value is special-cased: on
    // any other error the partial line is kept and returned, and on end of
    // file it is likewise kept. This is a data-loss path (ERR-input-27,
    // disposition `reproduce`), and the test reads the global `errno` rather
    // than anything the callback returned, so a client callback that returns
    // -1 with a stale `EINTR` left there triggers the discard.
    if num == -1 && errno::errno() == EINTR {
        el.el_line.lastchar = 0;
    }
    // Step 4.
    el.el_line.cursor = el.el_line.lastchar;
    el.el_line.buffer[el.el_line.lastchar] = 0;
    // Step 5. NEVER negative on this path: end of file, an interrupted read
    // and a failed first allocation all report 0 with a NULL return, so a
    // `NO_TTY`/`EDIT_DISABLED` caller cannot distinguish them and cannot use
    // the `*nread == -1` convention the editing path provides.
    let n = el.el_line.lastchar;
    *nread = n as i32;
    // Step 6.
    if n == 0 {
        None
    } else {
        Some(&el.el_line.buffer[..n])
    }
}

// [spec:libedit:def:read.el-wgets-fn]
// [spec:libedit:sem:read.el-wgets-fn]
/// Read a line. `nread` is `None` for the C's NULL out-parameter, which it
/// retargets at a local. The result is a view of `el_line.buffer`, valid
/// until the next call, which is what the borrow of `el` expresses.
pub fn el_wgets<'a>(el: &'a mut EditLine, nread: Option<&mut i32>) -> Option<&'a [u32]> {
    // Step 1. C: `if (nread == NULL) nread = &nrb;` — the value is then
    // discarded, and the rest of the body writes through it unconditionally.
    let mut nrb = 0;
    let nread: &mut i32 = match nread {
        Some(n) => n,
        None => &mut nrb,
    };
    // Step 2.
    *nread = 0;
    if let Some(rd) = el.el_read.as_deref_mut() {
        rd.read_errno = 0;
    }

    // Step 3. The reset is unconditional, `UNBUFFERED` or not, so each call
    // starts a fresh line even in unbuffered mode. NOTHING else in this
    // function runs — no `read_prepare`, therefore no `sig_set` and no
    // prompt; no `read_finish`; no final `terminal_flush`; and no conversion
    // of `*nread` to -1. A `NO_TTY` caller never sees -1.
    if (el.el_flags & NO_TTY) != 0 {
        el.el_line.lastchar = 0;
        return noedit_wgets(el, nread);
    }

    // Step 4, the `FIONREAD` typeahead pre-check, is not ported; see the
    // module documentation.

    // Step 5. Deliberately skipped under `UNBUFFERED`, because
    // `el_set(EL_UNBUFFERED, 1)` already ran `read_prepare` once and the line
    // must persist across calls.
    if (el.el_flags & UNBUFFERED) == 0 {
        read_prepare(el);
    }

    // Step 6. This happens AFTER `read_prepare`, so the prompt has been
    // printed and the handlers installed — but `read_finish` is never reached
    // on this path, so those handlers stay installed after `el_wgets` returns
    // and the tty is never put back into cooked mode. `EDIT_DISABLED` leaks
    // signal dispositions on every call (ERR-input-22). The rule asks the
    // port to decide; reproduced, because it is observable — the
    // application's own `SIGINT` handler is displaced and stays displaced,
    // and an application that reinstates it between calls would see a fix as
    // a behaviour change. `docs/errata.md` carries the fix.
    if (el.el_flags & EDIT_DISABLED) != 0 {
        if (el.el_flags & UNBUFFERED) == 0 {
            el.el_line.lastchar = 0;
        }
        terminal_flush(el);
        return noedit_wgets(el, nread);
    }

    // Step 7. `num` is the "line is finished, and this is its length" signal;
    // while it is -1 the line is still being edited.
    let mut num: i32 = -1;
    let mut cmdnum: ElActionT = 0;
    let mut ch: u32 = 0;
    while num == -1 {
        // (a) EOF, read error, or a key sequence abandoned mid-way: break
        // with `num` still -1.
        if read_getcmd(el, &mut cmdnum, &mut ch) == -1 {
            break;
        }
        // (b) The C's "BUG CHECK": the map slot holds an action index with no
        // function behind it. Read another command without touching any
        // per-command state.
        if cmdnum as usize >= el.el_map.nfunc {
            continue;
        }
        // (c) Recorded before dispatch, because vi's redo machinery reads
        // them from several levels down.
        el.el_state.thiscmd = cmdnum;
        el.el_state.thisch = ch;
        // (d) vi redo recording, BEFORE the command executes. In vi, `key`
        // holds the INSERT map and `alt` the command map, so `current == key`
        // means "vi insert mode".
        if el.el_map.r#type == MAP_VI
            && el.el_map.current == ElMapCurrent::Key
            && el.el_chared.c_redo.pos < el.el_chared.c_redo.lim
        {
            let cs = locale::charset();
            let redo = &mut el.el_chared.c_redo;
            if cmdnum == VI_DELETE_PREV_CHAR
                && redo.pos != 0
                && locale::iswprint(cs, redo.buf[redo.pos - 1])
            {
                // A backspace un-records the character it erases.
                redo.pos -= 1;
            } else {
                // In bounds: `pos < lim` and `lim` is an offset into `buf`.
                redo.buf[redo.pos] = ch;
                redo.pos += 1;
            }
        }
        // (e) `ch` is the character that resolved the binding, which for a
        // multi-key sequence is the LAST character of the sequence.
        let func = el.el_map.func[cmdnum as usize];
        // SAFETY: `func` is either one of `EL_FUNC`'s shims — which want
        // exactly this handle — or a command an application registered
        // through `el_set(EL_ADDFN, ...)`, whose contract
        // (`def:map.el-func-t-edit-line-wint-t`) is a C function taking the
        // `EditLine *` it was registered against. `el` is that handle, live
        // and exclusively borrowed here; the borrow is released for the call
        // so the command may re-enter libedit, as every builtin one does.
        let retval = unsafe { func(ptr::from_mut(el), ch) };
        // (f) AFTER the call, so a command function observes the PREVIOUS
        // command in `lastcmd` — yank-pop, vi repeat and the argument logic
        // all depend on that.
        el.el_state.lastcmd = cmdnum;

        // Step 8.
        match retval {
            // Move the physical cursor to agree with `el_line.cursor`, no
            // redraw.
            CC_CURSOR => re_refresh_cursor(el),
            // FALLTHROUGH into `CC_REFRESH`.
            CC_REDISPLAY => {
                re_clear_lines(el);
                re_clear_display(el);
                re_refresh(el);
            }
            CC_REFRESH => re_refresh(el),
            CC_REFRESH_BEEP => {
                re_refresh(el);
                terminal_beep(el);
            }
            // Nothing at all.
            CC_NORM => {}
            // Jumps straight back to (a), SKIPPING the per-command resets in
            // step 9 AND the `UNBUFFERED` break. That is the entire mechanism
            // by which digit arguments accumulate, and the reason an argument
            // prefix does not cause an `UNBUFFERED` caller to return.
            CC_ARGHACK => continue,
            CC_EOF => {
                if (el.el_flags & UNBUFFERED) == 0 {
                    // Ends the loop and ultimately returns NULL with
                    // `*nread == 0`.
                    num = 0;
                } else if num == -1 {
                    // ERR-input-02 (UB, disposition `define — bound the
                    // append`): the C appends at `lastchar++` with no `limit`
                    // check and no `ch_enlargebufs`, overrunning the line
                    // buffer if the line is already at capacity. Bounded here
                    // with `noedit_wgets`'s own idiom, so that wherever the C
                    // is defined — which is everywhere the line is short, and
                    // `UNBUFFERED` returns after every command — the two
                    // agree exactly.
                    if el.el_line.lastchar + 1 >= el.el_line.limit {
                        let _ = ch_enlargebufs(el, 2);
                    }
                    if el.el_line.lastchar < el.el_line.buffer.len() {
                        el.el_line.buffer[el.el_line.lastchar] = CONTROL_D;
                        el.el_line.lastchar += 1;
                    }
                    el.el_line.cursor = el.el_line.lastchar;
                    num = 1;
                }
            }
            // The newline-producing commands append `'\n'` themselves before
            // returning this, so an "empty" line still yields `num == 1` and
            // a one-character buffer `L"\n"`.
            CC_NEWLINE => num = el.el_line.lastchar as i32,
            CC_FATAL => {
                // Put the (real) cursor in a known place, reset the input
                // pointers, and discard ALL pending macro input — this is the
                // only place that happens mid-line.
                re_clear_display(el);
                ch_reset(el);
                if let Some(rd) = el.el_read.as_deref_mut() {
                    read_clearmacros(&mut rd.macros);
                }
                // Print the prompt again.
                re_refresh(el);
            }
            // `CC_ERROR` and `default` — any value the switch does not name,
            // including anything a client function invents.
            _ => {
                terminal_beep(el);
                terminal_flush(el);
            }
        }
        // Step 9: for every case except `CC_ARGHACK`.
        el.el_state.argument = 1;
        el.el_state.doingarg = 0;
        el.el_chared.c_vcmd.action = NOP;
        // Step 10: one command per call.
        if (el.el_flags & UNBUFFERED) != 0 {
            break;
        }
    }

    // Step 11: flush whatever the last command wrote.
    terminal_flush(el);
    // Step 12.
    if (el.el_flags & UNBUFFERED) == 0 {
        read_finish(el);
        *nread = if num != -1 { num } else { 0 };
    } else {
        // No `read_finish`: the tty stays raw and the handlers stay
        // installed, by design, until `el_set(EL_UNBUFFERED, 0)`. And this is
        // the CUMULATIVE length of the line so far, not the number of
        // characters added by this call.
        *nread = el.el_line.lastchar as i32;
    }

    if *nread == 0 {
        // Step 14. `num == -1` means the loop was broken by `read_getcmd`
        // failing, not by a command finishing the line.
        //
        // Under `UNBUFFERED` it also means the line is still empty and
        // nothing failed: any command that neither inserts text nor completes
        // the line — a cursor move, a beep, a failed search — run as the
        // first keystroke reports end of file to the caller. That is
        // ERR-input-23, a genuine trap, and its disposition is `reproduce`.
        if num == -1 {
            *nread = -1;
            // The only place `errno` is written on any surviving exit path.
            // On a clean end of file the callback returned 0, so `read_errno`
            // stayed 0 and `errno` holds whatever `terminal_flush`,
            // `tty_cookedmode` or `sig_clr` incidentally left — UNSPECIFIED.
            let saved = el.el_read.as_deref().map_or(0, |rd| rd.read_errno);
            if saved != 0 {
                errno::set_errno(saved);
            }
        }
        None
    } else {
        // Step 13. `*nread` and `lastchar` agree on every path that gets
        // here: the buffered path only leaves `num` non-zero through
        // `CC_NEWLINE`, which sets it from `lastchar`, and the unbuffered one
        // reads `lastchar` directly.
        Some(&el.el_line.buffer[..el.el_line.lastchar])
    }
}

/// Read a line as bytes, encoded in the current locale.
///
/// The byte-oriented counterpart to [`el_wgets`], and the one a shell wants:
/// it hands back what the user typed in the encoding the rest of the program
/// speaks, with `nread` counting bytes rather than wide characters.
///
/// This is `eln.c`'s `el_gets`, hoisted into the core because every Rust
/// consumer otherwise reimplements it — encode through `el_lgcyconv`, then
/// rewrite the wide count as a byte count — and the buffer choice is not
/// obvious. Getting it wrong breaks the caller's lifetime contract silently.
///
/// The returned slice borrows `el_lgcyconv`, so the next [`el_gets`],
/// `el_line`, `el_get(EL_EDITOR)` or `el_get(EL_WORDCHARS)` on this editor
/// invalidates it. The borrow of `el` says so; the C ABI's version cannot and
/// documents it in prose instead.
///
/// # The count and the string can disagree
///
/// `nread` counts the encoding of exactly the wide characters the read
/// reported, and the string runs to the wide terminator. Those agree on the
/// buffered path, which terminates at the reported length, and not under
/// `EL_UNBUFFERED`, where the returned bytes run past the reported count into
/// characters left over from an earlier, longer line (ERR-core-api-26,
/// reproduced). A caller that trusts the count is right in both cases; one
/// that trusts the terminator is not.
///
/// A character the locale cannot encode contributes nothing to the count, so
/// that it matches the encoder dropping it. The measurement is clamped to the
/// line rather than trusting the count to be inside it — the C ABI's copy
/// walks `*nread` characters from the buffer start whatever `*nread` says.
pub fn el_gets<'a>(el: &'a mut EditLine, nread: Option<&mut i32>) -> Option<&'a [u8]> {
    let mut nrb = 0;
    let nread: &mut i32 = match nread {
        Some(n) => n,
        None => &mut nrb,
    };

    // Copied out rather than kept borrowed: the encode below needs
    // `el_lgcyconv` mutably and the wide line is a view of the same editor.
    // The C reaches around that with two raw pointers into one object; a line
    // is one allocation per keypress-terminated read, which is not where this
    // library spends anything.
    let line: Vec<u32> = el_wgets(el, Some(nread))?.to_vec();

    // Exactly `*nread` characters are measured, which is not the length of
    // the string encoded below. See the note above.
    let counted = usize::try_from(*nread).unwrap_or(0).min(line.len());
    let bytes: usize = line[..counted].iter().copied().map(ct_enc_width).sum();
    *nread = i32::try_from(bytes).unwrap_or(i32::MAX);

    ct_encode_string(Some(&line), &mut el.el_lgcyconv)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::os::fd::AsRawFd;

    use super::*;
    use crate::testkit::headless_editor;

    thread_local! {
        /// What [`feed`] hands back, one call at a time. Thread local because
        /// the test runner runs tests on threads of its own and the hook has
        /// nowhere else to reach — the C's `el_rfunc_t` carries no user data,
        /// which is the same constraint the real callback lives under.
        static FEED: RefCell<VecDeque<(c_int, u32)>> =
            const { RefCell::new(VecDeque::new()) };
    }

    /// A test `EL_GETCFN` hook. An exhausted queue is end of file, which is
    /// what every loop below terminates on.
    unsafe extern "C" fn feed(_el: *mut EditLine, cp: *mut u32) -> c_int {
        match FEED.with(|q| q.borrow_mut().pop_front()) {
            Some((rv, ch)) => {
                // SAFETY: `cp` is the caller's `wchar_t`, which every call site
                // in this module passes as a live exclusive borrow.
                unsafe { *cp = ch };
                rv
            }
            None => 0,
        }
    }

    /// Queue `s`, one successful character per call, then end of file.
    fn feed_chars(s: &str) {
        FEED.with(|q| {
            let mut q = q.borrow_mut();
            q.clear();
            q.extend(s.chars().map(|c| (1, u32::from(c))));
        });
    }

    /// Append a failed read to whatever [`feed_chars`] queued.
    fn feed_error() {
        FEED.with(|q| q.borrow_mut().push_back((-1, 0)));
    }

    fn w(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    /// The shared headless editor. Its line buffer is `ch_init`'s, with the
    /// slack above `lastchar` that the insert paths shift into — this module
    /// used to reproduce that by hand, which is a second opinion about a
    /// buffer whose shape `noedit_wgets` depends on. The screen size is
    /// arbitrary: nothing below draws.
    ///
    /// `el_infd` at -1 is load-bearing rather than tidiness. It is what makes
    /// the tty unreadable, so [`el_wgetc`] reports end of file the moment the
    /// macro queue runs dry instead of blocking on the test runner's stdin.
    fn el() -> EditLine {
        headless_editor(80, 24)
    }

    fn macros(el: &mut EditLine) -> &mut Macros {
        &mut el.el_read.as_deref_mut().expect("read_init ran").macros
    }

    fn getc(el: &mut EditLine) -> (i32, u32) {
        let mut c = 0;
        let rv = el_wgetc(el, &mut c);
        (rv, c)
    }

    /// The setter takes anything and reports success unconditionally, and the
    /// getter is its exact inverse — so the builtin's address never escapes,
    /// whether it was never replaced or was installed deliberately. An
    /// application therefore cannot tell "the builtin is in place" from "I put
    /// the builtin back", and `EL_GETCFN` round-trips losslessly.
    // [spec:libedit:sem:read.el-read-setfn-fn/test]
    // [spec:libedit:sem:read.el-read-getfn-fn/test]
    #[test]
    fn the_builtin_reader_is_reported_as_absent_however_it_was_installed() {
        let mut el = el();
        let rd = el.el_read.as_deref_mut().expect("read_init ran");

        assert!(
            el_read_getfn(rd).is_none(),
            "read_init installs the builtin"
        );

        assert_eq!(el_read_setfn(rd, Some(feed as ElRfuncT)), 0);
        let got = el_read_getfn(rd).expect("a client hook is handed back");
        assert!(std::ptr::fn_addr_eq(got, feed as ElRfuncT));

        // The C's `EL_BUILTIN_GETCFN`, a NULL `el_rfunc_t`.
        assert_eq!(el_read_setfn(rd, None), 0);
        assert!(el_read_getfn(rd).is_none());

        // And the same answer for the builtin installed by address.
        assert_eq!(el_read_setfn(rd, Some(feed as ElRfuncT)), 0);
        assert_eq!(el_read_setfn(rd, Some(read_char as ElRfuncT)), 0);
        assert!(
            el_read_getfn(rd).is_none(),
            "the builtin's address is never handed out"
        );
    }

    /// The would-block recovery **permanently clears** `O_NONBLOCK` on the
    /// caller's descriptor — normally the process's shared standard input —
    /// saving and restoring nothing. That is ERR-input-21, and reproducing it
    /// is the specified behaviour rather than an oversight; nothing else in
    /// the library compensates.
    ///
    /// `EINTR` is the arm that actually runs today (`EL_SAFEREAD` was
    /// realistically enabled for nothing else) and it is a pure "retry me"
    /// with no side effect at all. Everything else is unrecoverable.
    // [spec:libedit:sem:read.read-fixio-fn/test]
    #[test]
    fn the_would_block_recovery_clears_nonblock_and_never_puts_it_back() {
        let f = std::fs::File::open("/dev/null").expect("/dev/null");
        let fd = f.as_raw_fd();
        let orig = plat::fcntl_getfl(fd).expect("F_GETFL");

        assert!(plat::fcntl_setfl(fd, orig | plat::O_NDELAY), "F_SETFL");
        assert_ne!(plat::fcntl_getfl(fd).expect("F_GETFL") & plat::O_NDELAY, 0);

        assert_eq!(read_fixio(fd, EWOULDBLOCK), 0);
        assert_eq!(
            plat::fcntl_getfl(fd).expect("F_GETFL") & plat::O_NDELAY,
            0,
            "the flag is cleared on the caller's descriptor and left cleared"
        );

        // The C's `case -1:` label shares this arm. It is never a real errno
        // value; it exists so the compiler cannot prove the block unreachable
        // when every other label is preprocessed away.
        assert!(plat::fcntl_setfl(fd, orig | plat::O_NDELAY), "F_SETFL");
        assert_eq!(read_fixio(fd, -1), 0);
        assert_eq!(plat::fcntl_getfl(fd).expect("F_GETFL") & plat::O_NDELAY, 0);

        // `EINTR` touches nothing, so a descriptor left non-blocking stays so.
        assert!(plat::fcntl_setfl(fd, orig | plat::O_NDELAY), "F_SETFL");
        assert_eq!(read_fixio(fd, EINTR), 0);
        assert_ne!(
            plat::fcntl_getfl(fd).expect("F_GETFL") & plat::O_NDELAY,
            0,
            "the retry arm has no side effects"
        );
        assert!(plat::fcntl_setfl(fd, orig), "F_SETFL");

        // Anything else is unrecoverable, and so is a would-block on a
        // descriptor `fcntl` cannot answer for — which is the only way the
        // C's both-sub-blocks-absent -1 is still reachable.
        assert_eq!(read_fixio(fd, EBADF), -1);
        assert_eq!(read_fixio(fd, EIO), -1);
        assert_eq!(read_fixio(-1, EWOULDBLOCK), -1);
    }

    /// Despite the "level" vocabulary the queue is **FIFO**: a push writes at
    /// the back while `el_wgetc` always reads the front, so text pushed while
    /// a macro is draining is queued behind the remainder of that macro
    /// rather than spliced in ahead of it (ERR-input-42).
    /// `sem:histedit.el-wpush-fn` claims the opposite and is wrong. This is
    /// the assertion that fails if somebody "fixes" this into a stack.
    // [spec:libedit:sem:read.el-wpush-fn/test]
    #[test]
    fn pushed_macros_are_consumed_oldest_first() {
        let mut el = el();
        el_wpush(&mut el, Some(&w("ab")));
        el_wpush(&mut el, Some(&w("cd")));

        assert_eq!(getc(&mut el), (1, u32::from(b'a')));
        // A push mid-drain does not disturb the read cursor and does not jump
        // the queue: `offset` belongs to the entry being drained and only
        // `read_pop` and `read_clearmacros` reset it.
        el_wpush(&mut el, Some(&w("ef")));
        assert_eq!(macros(&mut el).offset, 1);
        for c in "bcdef".chars() {
            assert_eq!(getc(&mut el), (1, u32::from(c)));
        }
        assert_eq!(macros(&mut el).level, -1);

        // An empty string is not a failure: it takes a slot and is popped
        // unread, which is observable only as the slot it occupies.
        el_wpush(&mut el, Some(&[]));
        assert_eq!(macros(&mut el).level, 0);
        assert_eq!(getc(&mut el).0, 0, "nothing to read, and no tty either");

        // `wcsdup` stops at the first NUL, so an embedded one truncates.
        el_wpush(&mut el, Some(&[u32::from(b'x'), 0, u32::from(b'y')]));
        assert_eq!(macros(&mut el).r#macro[0], w("x"));
    }

    /// The queue holds at most `EL_MAXMACRO` entries — levels 0 through 9 —
    /// and the overflow is silent: the function returns void, so the beep is
    /// the only signal a caller gets, and a NULL string produces exactly the
    /// same one. The C notes that a full queue and a failed `wcsdup` are
    /// observationally identical; only the first is reachable here.
    #[test]
    fn the_macro_queue_drops_the_eleventh_push() {
        let mut el = el();
        for i in 0..EL_MAXMACRO {
            el_wpush(&mut el, Some(&w("z")));
            assert_eq!(macros(&mut el).level, i as i32);
        }
        el_wpush(&mut el, Some(&w("overflow")));
        assert_eq!(macros(&mut el).level, (EL_MAXMACRO - 1) as i32);
        assert_eq!(macros(&mut el).r#macro.len(), EL_MAXMACRO);

        // The C's NULL `str`, a live call site in `read_getcmd`: the beep and
        // nothing else.
        el_wpush(&mut el, None);
        assert_eq!(macros(&mut el).r#macro.len(), EL_MAXMACRO);
    }

    /// The FRONT entry is what leaves, `offset` is reset so the new front is
    /// read from its first character, and popping an empty queue does
    /// nothing. The C has no guard there: it frees `macro[0]` a second time
    /// and drives `level` to -2 (ERR-input-17). Both live call sites satisfy
    /// the precondition, so the guard is what keeps a latent hazard latent.
    // [spec:libedit:sem:read.read-pop-fn/test]
    #[test]
    fn popping_takes_the_front_and_tolerates_an_empty_queue() {
        let mut el = el();
        let ma = macros(&mut el);
        read_pop(ma);
        assert_eq!(ma.level, -1, "not -2");
        assert!(ma.r#macro.is_empty());
        assert_eq!(ma.offset, 0);

        el_wpush(&mut el, Some(&w("ab")));
        el_wpush(&mut el, Some(&w("cd")));
        let ma = macros(&mut el);
        ma.offset = 1;
        read_pop(ma);
        assert_eq!(ma.level, 0);
        assert_eq!(ma.offset, 0, "the new front is read from the start");
        assert_eq!(ma.r#macro[0], w("cd"), "the front left, not the back");

        // The last one: nothing shifts, and the queue tests as empty again.
        read_pop(ma);
        assert_eq!(ma.level, -1);
        assert!(ma.r#macro.is_empty());
    }

    /// The unedited read path calls the hook **directly**, never through
    /// `el_wgetc`: the macro queue is not consulted, so `el_wpush` has no
    /// effect here at all, and `read_errno` is never written. The line
    /// terminator is kept in the buffer and counted in `*nread`.
    ///
    /// `*nread` is never negative on this path — end of file, an interrupted
    /// read and an empty line are all 0 with a NULL return — so a
    /// `NO_TTY`/`EDIT_DISABLED` caller cannot use the -1 convention the
    /// editing path provides.
    // [spec:libedit:sem:read.noedit-wgets-fn/test]
    #[test]
    fn the_unedited_read_keeps_the_newline_and_ignores_the_macro_queue() {
        let mut el = el();
        let rd = el.el_read.as_deref_mut().expect("read_init ran");
        assert_eq!(el_read_setfn(rd, Some(feed as ElRfuncT)), 0);

        el_wpush(&mut el, Some(&w("PUSHED")));
        feed_chars("hi\nrest");
        let mut nread = -7;
        assert_eq!(noedit_wgets(&mut el, &mut nread), Some(&w("hi\n")[..]));
        assert_eq!(nread, 3, "the newline is counted");
        assert_eq!(macros(&mut el).level, 0, "the pushed macro was not touched");
        assert_eq!(el.el_read.as_deref().expect("read_init ran").read_errno, 0);

        // End of file with nothing read: NULL and zero, not -1.
        el.el_line.lastchar = 0;
        feed_chars("");
        let mut nread = -7;
        assert_eq!(noedit_wgets(&mut el, &mut nread), None);
        assert_eq!(nread, 0);

        // A carriage return ends the line too, and is likewise kept.
        el.el_line.lastchar = 0;
        feed_chars("ab\rcd");
        let mut nread = 0;
        assert_eq!(noedit_wgets(&mut el, &mut nread), Some(&w("ab\r")[..]));
        assert_eq!(nread, 3);

        // Unbuffered mode stops after a single character, terminator or not.
        el.el_line.lastchar = 0;
        el.el_flags |= UNBUFFERED;
        feed_chars("xyz");
        let mut nread = 0;
        assert_eq!(noedit_wgets(&mut el, &mut nread), Some(&w("x")[..]));
        assert_eq!(nread, 1);
        el.el_flags &= !UNBUFFERED;
    }

    /// ERR-input-27, a data-loss path with `reproduce` for a disposition: a
    /// read that fails while `errno` reads `EINTR` discards the **entire**
    /// partial line, not merely the character that failed. The test is on the
    /// global `errno` rather than on anything the hook returned, so a client
    /// callback that reports -1 with a stale `EINTR` still lying there
    /// triggers the discard — and any other `errno` keeps the line.
    #[test]
    fn an_interrupted_read_throws_away_the_whole_partial_line() {
        let mut el = el();
        let rd = el.el_read.as_deref_mut().expect("read_init ran");
        assert_eq!(el_read_setfn(rd, Some(feed as ElRfuncT)), 0);

        feed_chars("abc");
        feed_error();
        errno::set_errno(EINTR);
        let mut nread = -7;
        assert_eq!(noedit_wgets(&mut el, &mut nread), None);
        assert_eq!(nread, 0);
        assert_eq!(el.el_line.lastchar, 0);

        // The same failure under any other errno keeps what was read.
        el.el_line.lastchar = 0;
        feed_chars("abc");
        feed_error();
        errno::set_errno(EIO);
        let mut nread = 0;
        assert_eq!(noedit_wgets(&mut el, &mut nread), Some(&w("abc")[..]));
        assert_eq!(nread, 3);
        errno::set_errno(0);
    }

    /// `el_gets` is the byte-oriented entry point a shell wants, and until now
    /// only `nshedit-abi` had one — every direct Rust consumer reimplemented
    /// the encode-and-recount from `eln.c`. Under `NO_TTY` the read path
    /// bypasses `el_wgetc` entirely and reaches the descriptor, which is what
    /// makes it drivable here.
    #[test]
    fn the_byte_reader_returns_the_line_and_a_byte_count() {
        let mut el = el();
        el.el_flags |= NO_TTY;
        el.el_read.as_deref_mut().unwrap().read_char = Some(feed);
        feed_chars("echo hi\n");

        let mut nread = 0;
        let line = el_gets(&mut el, Some(&mut nread)).expect("a line was typed");
        assert_eq!(line, b"echo hi\n");
        assert_eq!(nread, 8, "bytes, not wide characters");
    }

    /// A multi-byte character makes the two counts differ, which is the whole
    /// reason the rewrite exists: `el_wgets` would have reported 3.
    #[test]
    fn the_count_is_bytes_where_the_wide_one_would_be_characters() {
        let mut el = el();
        el.el_flags |= NO_TTY;
        el.el_read.as_deref_mut().unwrap().read_char = Some(feed);
        feed_chars("aé\n");

        let mut nread = 0;
        let line = el_gets(&mut el, Some(&mut nread)).expect("a line was typed");
        // Whatever the harness locale encodes to, the count is the length of
        // what came back rather than the number of characters read.
        assert_eq!(nread as usize, line.len());
        assert!(line.ends_with(b"\n"));
    }

    /// End of input is `None` with a zero count, the same shape `el_wgets`
    /// reports, so a caller need not learn a second convention.
    #[test]
    fn the_byte_reader_reports_end_of_input_as_no_line() {
        let mut el = el();
        el.el_flags |= NO_TTY;
        el.el_read.as_deref_mut().unwrap().read_char = Some(feed);
        feed_chars("");

        let mut nread = -7;
        assert!(el_gets(&mut el, Some(&mut nread)).is_none());
        assert_eq!(nread, 0, "and the count is the read's, not the rewrite's");
    }

    /// The out-parameter is optional here where the C ABI's `el_gets`
    /// dereferences it the moment a line comes back (ERR-core-api-11).
    #[test]
    fn the_byte_reader_tolerates_no_count_at_all() {
        let mut el = el();
        el.el_flags |= NO_TTY;
        el.el_read.as_deref_mut().unwrap().read_char = Some(feed);
        feed_chars("x\n");

        assert_eq!(el_gets(&mut el, None).expect("a line"), b"x\n");
    }
}
