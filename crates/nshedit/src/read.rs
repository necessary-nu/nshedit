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
//! - `read__fixio`'s `FIONBIO` sub-block, for the same header reason. See
//!   [`plat`] for what the surviving `fcntl` half needs.

use core::sync::atomic::Ordering;
use std::fs::File;
use std::io::Read;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;

use crate::chared::{NOP, ch_enlargebufs, ch_reset};
use crate::el::{EDIT_DISABLED, EditLine, ElActionT, FIXIO, HANDLE_SIGNALS, NO_TTY, UNBUFFERED};
use crate::errno;
use crate::fcns::{ED_INSERT, ED_SEQUENCE_LEAD_IN, VI_DELETE_PREV_CHAR};
use crate::histedit::{
    CC_ARGHACK, CC_CURSOR, CC_EOF, CC_FATAL, CC_NEWLINE, CC_NORM, CC_REDISPLAY, CC_REFRESH,
    CC_REFRESH_BEEP, ElRfuncT,
};
use crate::keymacro::{KeymacroValueT, keymacro_get};
use crate::locale;
use crate::map::{ElMapCurrent, MAP_VI, N_KEYS};
use crate::refresh::{re_clear_display, re_clear_lines, re_refresh, re_refresh_cursor};
use crate::sig::{sig_clr, sig_handler, sig_set, signo};
use crate::terminal::{terminal__flush, terminal_beep};
use crate::tty::{tty_cookedmode, tty_rawmode};

/// C: `#define EL_MAXMACRO 10` — the macro nesting limit.
pub const EL_MAXMACRO: usize = 10;

// `errno` values this module tests and stores. `crate::errno` carries only the
// three the `vis`/`unvis` layer needed, and nothing here may extend it, so the
// four the read path needs are named locally; they are Linux's, matching that
// module's numbering and `plan/decisions/posix-only-scope.md`. **These belong
// in `crate::errno` and should be hoisted the moment that module is touched.**

/// C: `EINTR`.
const EINTR: i32 = 4;
/// C: `EILSEQ` — what step 4d reports for an over-long multibyte sequence.
const EILSEQ: i32 = 84;
/// C: `EWOULDBLOCK`. On Linux `EAGAIN` has the same value, which is why
/// [`read__fixio`] needs only one label for the would-block condition.
const EWOULDBLOCK: i32 = 11;
/// C: `EBADF` — what `read(2)` reports for the descriptor a half-built
/// `EditLine` carries.
const EBADF: i32 = 9;
/// C: `EIO`. Only a fallback: every `io::Error` a raw `read(2)` produces on
/// Unix carries its `errno`, so [`read_byte`] never actually reports this.
const EIO: i32 = 5;

// `keymacro.h`'s node types. `crate::keymacro` models the union as an enum and
// so declares no constants, but [`keymacro_get`] still *returns* the C's `int`
// type code, so the three values have to be spelled somewhere. **They belong
// in `crate::keymacro`.**

/// C: `#define XK_CMD 0`.
const XK_CMD: i32 = 0;
/// C: `#define XK_STR 1`.
const XK_STR: i32 = 1;
/// C: `#define XK_NOD 2`.
const XK_NOD: i32 = 2;

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
pub(crate) fn el_read_setfn(el_read: &mut ElReadT, rc: Option<ElRfuncT>) -> i32 {
    // Steps 1 and 2. No validation of any kind, exactly as the C.
    el_read.read_char = Some(rc.unwrap_or(read_char as ElRfuncT));
    // Step 3: unconditional, so `el_set(EL_GETCFN, ...)` always succeeds.
    0
}

// [spec:libedit:def:read.el-read-getfn-fn]
// [spec:libedit:sem:read.el-read-getfn-fn]
/// Return the current read-char function, or `None` when it is the builtin
/// one — the C's `EL_BUILTIN_GETCFN`.
pub(crate) fn el_read_getfn(el_read: &mut ElReadT) -> Option<ElRfuncT> {
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

/// The POSIX descriptor-flag calls [`read__fixio`]'s would-block arm is
/// written against, and the one place this module cannot reach.
///
/// `plan/decisions/no-c-ffi.md` bars the `libc` crate, and Rust's standard
/// library exposes `fcntl` for no file descriptor and `O_NONBLOCK` control
/// only through the socket types. So the two calls are named exactly, one
/// function each, and both report failure today — which lands this arm on an
/// outcome the rule already spells out: "Because `e` was seeded to 0, a build
/// where neither sub-block compiles returns -1 from this arm
/// unconditionally." That is a configuration of the C itself, not an invented
/// behaviour, and it is reached here through `F_GETFL` failing rather than
/// through the sub-block being preprocessed away.
///
/// What has to arrive is one thing: `fcntl(2)` issued without libc. With it,
/// the arm recovers and — this is the point, not an accident —
/// **permanently clears `O_NONBLOCK`/`O_NDELAY` on the caller's input
/// descriptor**, normally the process's shared standard input, saving and
/// restoring nothing (ERR-input-21). The structure below is written so that
/// dropping in the syscall reproduces that side effect rather than diverging
/// from it; today the recovery simply does not happen and a would-block error
/// reaches the caller as -1, which is also what `EL_SAFEREAD` being off — the
/// default — produces.
mod plat {
    /// `fcntl(fd, F_GETFL, 0)`. `None` is the C's -1.
    pub(super) fn fcntl_getfl(_fd: i32) -> Option<i32> {
        None
    }

    /// `fcntl(fd, F_SETFL, fl & ~O_NDELAY)` — the call that clears the
    /// caller's non-blocking bit and never puts it back. `false` is the C's
    /// -1.
    pub(super) fn fcntl_setfl_clearing_ndelay(_fd: i32, _fl: i32) -> bool {
        false
    }
}

// [spec:libedit:def:read.read-fixio-fn]
// [spec:libedit:sem:read.read-fixio-fn]
/// Try to recover from a failed read; `e` is the `errno` it failed with.
#[allow(non_snake_case)]
fn read__fixio(fd: i32, e: i32) -> i32 {
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
            if !plat::fcntl_setfl_clearing_ndelay(fd, fl) {
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
    terminal__flush(el);
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
/// signature is that type's: 1 for a character, 0 for end of input, -1 for
/// an error.
fn read_char(el: &mut EditLine, cp: &mut u32) -> i32 {
    // Read the initialiser carefully — the sense is inverted from what the
    // name suggests. `FIXIO` (set by `el_set(EL_SAFEREAD, 1)`) makes `tried`
    // start false, which is what ENABLES the recovery path. With `FIXIO`
    // clear — the DEFAULT — it starts true and `read__fixio` is never
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
            // arm does return — through a `read__fixio` retry — and a second
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
                    terminal__flush(el);
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
            if !tried && read__fixio(el.el_infd, e) == 0 {
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
        Some(f) => f(el, cp),
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
    terminal__flush(el);

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
    // can restore it after cleanup (`terminal__flush`, `tty_cookedmode`,
    // `sig_clr`) that may clobber `errno`. Never cleared here; `el_wgets`
    // zeroes it on entry and nothing else writes it.
    if num_read < 0 {
        if let Some(rd) = el.el_read.as_deref_mut() {
            rd.read_errno = errno::errno();
        }
    }

    // Step 6, unchanged. `*cp` holds whatever the callback left; the builtin
    // stores `L'\0'` on both 0 and -1, but a client callback need not.
    num_read
}

// [spec:libedit:def:read.read-prepare-fn]
// [spec:libedit:sem:read.read-prepare-fn]
/// Set up for a read: signals, raw mode, resize, and the prompt.
pub(crate) fn read_prepare(el: &mut EditLine) {
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
        terminal__flush(el);
    }
    // The macro queue and `read_errno` are NOT touched, so pushed macros
    // survive a `read_prepare` and are consumed by the line it prepares.
}

// [spec:libedit:def:read.read-finish-fn]
// [spec:libedit:sem:read.read-finish-fn]
/// Undo [`read_prepare`]: cooked mode and the signal handlers.
pub(crate) fn read_finish(el: &mut EditLine) {
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
        // `terminal__flush` is never called per character; and `read_errno`
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
    // prompt; no `read_finish`; no final `terminal__flush`; and no conversion
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
        terminal__flush(el);
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
        let retval = func(el, ch);
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
                terminal__flush(el);
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
    terminal__flush(el);
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
            // stayed 0 and `errno` holds whatever `terminal__flush`,
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
