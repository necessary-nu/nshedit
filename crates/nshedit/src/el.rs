//! Ported from `src/el.c`; rules live in `docs/spec/port/src/el.md`.
//!
//! # No codeset detection lives here
//!
//! `el.c` includes `<langinfo.h>` and `<locale.h>` and uses neither:
//! `nl_langinfo` is never called, nothing from `<locale.h>` is referenced, and
//! `MAXPATHLEN` is defined at the top of the file and never read. The single
//! locale-sensitive decision in the C file is the `MB_CUR_MAX` test in
//! `el_wset`'s `EL_HIST` arm, and `el_wset` is not in this module — the
//! varargs dispatch belongs to the ABI crate. So none of the three carries
//! over; the locale-sensitive work that does happen here is the multibyte
//! conversion in `ct_decode_string`/`ct_encode_string` and the `iswspace`
//! skip in [`el_source`], all of it served by `crate::locale`.
//!
//! `sem:el.el-init-internal-fn` records that this file never calls
//! `setlocale`, so a C `EditLine` constructed before `setlocale(LC_CTYPE, "")`
//! decodes its program name in the C locale. `crate::locale` already documents
//! why the port cannot reproduce that (it resolves the charset from the
//! environment, i.e. as if the program had called `setlocale(LC_ALL, "")`), and
//! why the difference is not observable from here.

use core::ffi::{c_char, c_void};
use core::ptr;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::sync::OnceLock;

/// C: `#define EL_BUFSIZ ((size_t)1024)` — the initial line-buffer size, and
/// the bound several unrelated routines reuse as a scratch limit.
pub(crate) const EL_BUFSIZ: usize = 1024;

// `el_flags` bits. C: `el.h`. Kept an integer flag word rather than becoming
// a bitflags type, so the literal port reads as the C does.
pub(crate) const HANDLE_SIGNALS: i32 = 0x001;
pub(crate) const NO_TTY: i32 = 0x002;
pub(crate) const EDIT_DISABLED: i32 = 0x004;
pub(crate) const UNBUFFERED: i32 = 0x008;
/// Set by the narrow `el_set(EL_HIST)` and by the readline layer, and the
/// reason the history bridge routes through its conversion path. Note the
/// wide setter only clears it in a single-byte locale, which
/// `sem:el.el-wset-fn` records as a defect.
///
/// Hidden, and the only one of these bits that is `pub` at all. It is public
/// so `nshedit-abi`'s narrow `el_set(EL_HIST)` can raise it, and there is
/// nothing a Rust caller can do with it but harm: raising it asserts that the
/// installed store is a C `history()` reading `char *`, which is false of
/// every store [`EditLine::set_history`] can attach, and `hist_command` tests
/// the bit *before* it asks the store anything — so `history size` and
/// `history unique` in an `.editrc` would answer -1 for a store that would
/// otherwise have handled them.
#[doc(hidden)]
pub const NARROW_HISTORY: i32 = 0x040;
pub(crate) const NO_RESET: i32 = 0x080;
/// Selects the EINTR-recovery path in the read loop; `el_get` reports this
/// bit raw rather than as a boolean, which `sem:histedit.el-get-fn` records.
pub(crate) const FIXIO: i32 = 0x100;
use crate::chared::{CKillT, CRedoT, CUndoT, CVcmdT, ElCharedT, ch_end, ch_init, ch_reset};
use crate::chartype::{CtBufferT, ct_decode_string, ct_encode_string};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hist::{EditorHistory, ElHistoryT, HistSource, hist_end, hist_init};
use crate::histedit::HistEventW;
use crate::keymacro::{ElKeymacroT, KeymacroValueT, keymacro_end, keymacro_init};
use crate::literal::{ElLiteralT, literal_end, literal_init};
use crate::locale;
use crate::map::{ElMapCurrent, ElMapT, map_end, map_init};
use crate::parse::parse_line;
use crate::prompt::{ElPromptT, prompt_end, prompt_init};
use crate::read::{ElReadT, read_end, read_init};
use crate::refresh::ElRefreshT;
use crate::search::{ElSearchT, search_end, search_init};
use crate::sig::{ElSignalT, sig_end, sig_init};
use crate::terminal::{
    ElTerminalT, block_sigwinch, set_sigmask, terminal_beep, terminal_change_size, terminal_end,
    terminal_get_size, terminal_init,
};
use crate::tty::{
    C_NCC, ElTtyT, NN_IO, Termios, TtypermEntry, termios_zeroed, tty_cookedmode, tty_end, tty_init,
    tty_rawmode,
};

/// C: `TCSAFLUSH`, the `tcsetattr` action [`el_end`] hands `tty_end`: restore
/// after pending output drains and discard pending unread input.
///
/// `tty.c`'s translation has no home for the POSIX `TCSA*` constants yet and
/// `tty_end`'s `how` is the raw POSIX action, so the value is spelled out
/// here — Linux's, per `plan/decisions/posix-only-scope.md`. Private on
/// purpose; it belongs in `crate::tty` once that module publishes them, the
/// same disposition `hist.rs` records for the header constants it carries.
const TCSAFLUSH: i32 = 2;

/// Stand-in for the C's `FILE *`.
///
/// The three streams an `EditLine` holds are caller-owned: libedit never
/// closes or frees them (`sem:histedit.el-end-fn`), and
/// `EL_GETFP`/`EL_SETFP` round-trip them through the C ABI unchanged, so
/// there is nothing here to own and no Rust handle that would survive the
/// trip.
///
/// Nothing in this crate ever reads or writes through one. Every byte the C
/// would have put in a stream goes to the matching `el_infd`/`el_outfd`/
/// `el_errfd` descriptor instead, through [`crate::stdio`] — which is why the
/// `EditLine` carries both, and what makes three null streams a complete
/// answer for a Rust caller building an editor with [`el_init_fd`]. The
/// corollary is the trap: storing a real stream in `el_outfile` and leaving
/// `el_outfd` at its zero does not redirect output, it sends it to whatever
/// descriptor 0 happens to be.
pub type CFile = *mut c_void;

// [spec:libedit:def:el.func-t-const-char]
/// C: `typedef char * (*func_t)(const char *);`
///
/// The type `el_set`/`el_get` cast `EL_GETENV`'s argument through. Same
/// shape as the `el_getenv` member of [`EditLine`]; `el.c` declares it
/// separately only because the member is spelled out inline there.
///
/// This is `getenv(3)`'s own signature and the value crosses the C ABI in
/// both directions — the application installs one and `el_get(EL_GETENV)`
/// hands it back — so it is `unsafe extern "C"` rather than a Rust `fn`.
/// Both strings stay raw pointers: the argument is a borrowed NUL-terminated
/// name, and the result is borrowed from storage the hook owns.
pub type FuncT = unsafe extern "C" fn(*const c_char) -> *mut c_char;

// [spec:libedit:def:el.el-action-t]
/// C: `typedef unsigned char el_action_t;` — index into the command array.
pub type ElActionT = u8;

// [spec:libedit:def:el.coord-t]
/// Position on the screen.
///
/// Note that `el_terminal.t_size` stores columns in `v` and lines in `h`,
/// the reverse of what the names suggest. That is the C's doing, not a slip
/// here; see `sem:terminal.terminal-set-fn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordT {
    pub h: i32,
    pub v: i32,
}

// [spec:libedit:def:el.el-line-t]
/// The current line.
///
/// The C is four pointers, three of them into the allocation the first one
/// owns. They are offsets here: `ch_enlargebufs` rebases them by reading the
/// old pointer values *after* the `realloc` that invalidated them, which
/// `sem:chared.ch-enlargebufs-fn` calls out as undefined
/// behaviour the port must not inherit.
///
/// Field order is load-bearing and must not change: `el_wline` hands out a
/// `LineInfoW` view onto this struct, and
/// `sem:el.el-wline-fn` requires that to be a genuine
/// borrowed view of the first three members.
pub struct ElLineT {
    /// C: `wchar_t *buffer` — the input line, owned.
    pub buffer: Vec<u32>,
    /// C: `wchar_t *cursor` — offset into `buffer`.
    pub cursor: usize,
    /// C: `wchar_t *lastchar` — offset into `buffer`, one past the last
    /// character. The buffer is not NUL-terminated there.
    pub lastchar: usize,
    /// C: `const wchar_t *limit` — offset into `buffer` of the highest
    /// writable position, `buffer.len() - EL_LEAVE` once published.
    pub limit: usize,
}

// [spec:libedit:def:el.el-state-t]
/// Editor state.
pub struct ElStateT {
    /// What mode are we in?
    pub inputmode: i32,
    /// Are we getting an argument?
    pub doingarg: i32,
    /// Numeric argument.
    pub argument: i32,
    /// Is the next char a meta char?
    pub metanext: i32,
    /// Previous command.
    pub lastcmd: ElActionT,
    /// This command.
    pub thiscmd: ElActionT,
    /// C: `wchar_t thisch` — char that generated it.
    pub thisch: u32,
}

// [spec:libedit:def:el.editline]
/// The editor. `histedit.h` names it `EditLine`; see
/// `def:histedit.edit-line`.
pub struct EditLine {
    /// C: `wchar_t *el_prog` — the program name, owned.
    pub el_prog: Vec<u32>,
    /// C: `FILE *el_infile` — caller-owned, never closed.
    pub el_infile: CFile,
    /// C: `FILE *el_outfile` — caller-owned, never closed.
    pub el_outfile: CFile,
    /// C: `FILE *el_errfile` — caller-owned, never closed.
    pub el_errfile: CFile,
    /// Input file descriptor.
    pub el_infd: i32,
    /// Output file descriptor.
    pub el_outfd: i32,
    /// Error file descriptor.
    pub el_errfd: i32,
    /// Various flags.
    pub el_flags: i32,
    /// Cursor location.
    pub el_cursor: CoordT,
    /// C: `wint_t **el_display` — the real screen image, one owned row per
    /// screen line. `u32` and not `char`: rows carry `MB_FILL_CHAR` and the
    /// `EL_LITERAL` sentinel (bit 31), neither of them a Unicode scalar
    /// value. See `sem:literal.literal-add-fn`.
    pub el_display: Vec<Vec<u32>>,
    /// C: `wint_t **el_vdisplay` — the virtual screen image, same shape.
    pub el_vdisplay: Vec<Vec<u32>>,
    /// C: `void *el_data` — client data, stored and handed back untouched.
    pub el_data: *mut c_void,
    /// The current line information.
    pub el_line: ElLineT,
    /// Current editor state.
    pub el_state: ElStateT,
    /// Terminal dependent stuff.
    pub el_terminal: ElTerminalT,
    /// Tty dependent stuff.
    pub el_tty: ElTtyT,
    /// Refresh stuff.
    pub el_refresh: ElRefreshT,
    /// Prompt stuff.
    pub el_prompt: ElPromptT,
    /// Right-hand prompt stuff.
    pub el_rprompt: ElPromptT,
    /// Prompt literal bits.
    pub el_literal: ElLiteralT,
    /// Character editor stuff.
    pub el_chared: ElCharedT,
    /// Key mapping stuff.
    pub el_map: ElMapT,
    /// Key binding stuff.
    pub el_keymacro: ElKeymacroT,
    /// History stuff.
    pub el_history: ElHistoryT,
    /// Search stuff.
    pub el_search: ElSearchT,
    /// Signal handling stuff.
    pub el_signal: ElSignalT,
    /// C: `struct el_read_t *el_read` — character reading stuff, owned.
    /// `None` before `read_init` and after `read_end`.
    pub el_read: Option<Box<ElReadT>>,
    /// Buffer for displayable strings.
    pub el_visual: CtBufferT,
    /// Scratch conversion buffer.
    pub el_scratch: CtBufferT,
    // [spec:libedit:def:el.editline.el-getenv-fn]
    /// C: `char *(*el_getenv)(const char *)` — the environment-lookup hook.
    /// Defaults to `secure_getenv`, which is load-bearing for set-uid
    /// processes; `el_set(EL_GETENV, fn)` replaces it with no NULL check, so
    /// this stays nullable. `sem:el.editline.el-getenv-fn`
    /// lists the four lookups that must route through it.
    pub el_getenv: Option<FuncT>,
}

impl EditLine {
    /// Attach a history the editor can recall from and search.
    ///
    /// This is the Rust-facing counterpart to `el_set(el, EL_HIST, history,
    /// h)`, and the only one a program that does not link `nshedit-abi` can
    /// use: the C route takes a [`crate::hist::HistFunT`], which is variadic,
    /// and stable Rust cannot define a variadic function. Without this an
    /// editor built on the library directly has no history at all — `^P`,
    /// `^N`, vi's `k` and `j`, and `^R` all do nothing.
    ///
    /// # The editor borrows the history; it does not take it
    ///
    /// They have independent lifetimes and a shell depends on that: `set +o
    /// emacs` ends the editor and keeps the history, and the C says the same
    /// thing by making `el_history.ref` a client cookie `el_end` never frees.
    /// So this takes a shared handle rather than a value, and dropping the
    /// editor leaves the caller's history intact.
    ///
    /// [`crate::history::OwnedHistoryW`] implements [`EditorHistory`], so the
    /// built-in store needs no adapter:
    ///
    /// ```no_run
    /// # use std::cell::RefCell;
    /// # use std::rc::Rc;
    /// # use nshedit::el::EditLine;
    /// # use nshedit::history::OwnedHistoryW;
    /// # fn f(el: &mut EditLine) {
    /// let history = Rc::new(RefCell::new(OwnedHistoryW::with_size(100)));
    /// el.set_history(history.clone());
    /// // `el` may now be dropped; `history` outlives it and keeps its entries.
    /// # }
    /// ```
    ///
    /// Replaces whatever was attached, C or Rust.
    pub fn set_history(&mut self, history: Rc<RefCell<dyn EditorHistory>>) {
        self.el_history.src = HistSource::Rust(history);
    }

    /// The attached Rust history, if one was set by [`EditLine::set_history`].
    ///
    /// A second handle to the caller's own history, not the editor's copy —
    /// there is no copy. Answers `None` for a history installed through the C
    /// ABI, which is an opaque cookie this crate cannot safely hand out.
    #[must_use]
    pub fn history(&self) -> Option<Rc<RefCell<dyn EditorHistory>>> {
        match &self.el_history.src {
            HistSource::Rust(h) => Some(Rc::clone(h)),
            _ => None,
        }
    }
}

/// C: `MB_CUR_MAX`, for the one place in `el.c` that reads it.
///
/// The module documentation above records that `el_wset`'s `EL_HIST` arm is
/// the file's only locale-sensitive decision, and that the arm itself belongs
/// to the ABI crate — the varargs dispatch is
/// `plan/decisions/idiomatic-core.md`'s, not the core's. That leaves the
/// dispatch needing the answer and having no way to ask: `crate::locale` is
/// `pub(crate)`, and `plan/decisions/no-c-ffi.md` bars `nshedit-abi` from
/// naming libc's `MB_CUR_MAX` (the ration is spent on the `errno` accessor).
/// So the core answers, in one line, and this is the whole of it.
///
/// Hidden: it is not core API, it is the ABI crate's one locale question.
/// `sem:histedit.el-wset-fn` and `sem:hist.hist-set-fn` are what require it —
/// the wide `EL_HIST` clears `NARROW_HISTORY` only when this is 1, which is
/// ERR-core-api-16.
///
/// Divergence, inherited from `crate::locale` and observable exactly here:
/// the C's `MB_CUR_MAX` follows `setlocale`, so a program that never calls it
/// reads 1 whatever the environment says, while this reads the environment as
/// if `setlocale(LC_ALL, "")` had run. A C application that sets no locale and
/// runs under a UTF-8 `LANG` therefore has `el_set(EL_HIST, ...)` followed by
/// `el_wset(EL_HIST, ...)` clear `NARROW_HISTORY` where the port leaves it
/// set. `crate::locale` records why the port cannot follow `setlocale` at all.
#[doc(hidden)]
pub fn mb_cur_max() -> usize {
    locale::mb_cur_max(locale::charset())
}

// [spec:libedit:def:el.secure-getenv-fn]
// [spec:libedit:sem:el.secure-getenv-fn]
/// C: `char *secure_getenv(char const *name)`.
///
/// The default environment hook. Returns the variable's value, or `None`
/// when the process gained privilege from its executable — real and
/// effective uid differ, real and effective gid differ, or the loader marked
/// the process secure. That denial is what stops a set-uid program from
/// being fed an `.editrc`, a terminal description or an editor of the
/// attacker's choosing; it is the reason this, and not a bare environment
/// lookup, is what [`el_init_internal`] installs.
///
/// `OsString` rather than `String`: POSIX environment values are bytes, and
/// three of the four names libedit looks up (`EDITRC`, `HOME`, `EDITOR`)
/// yield paths or command lines that need not be UTF-8.
///
/// The C's degenerate build — no `secure_getenv`, no `__secure_getenv`, no
/// `issetugid`, so `issetugid()` is `1` and libedit reads no environment at
/// all — is a portability artefact and is deliberately not reproduced.
///
/// Note this does not have the shape of [`FuncT`]: an application-supplied
/// hook hands back a borrowed `char *` across the C ABI, whereas the
/// built-in default owns what it returns. The two are reconciled where the
/// hook is consulted, not here.
pub fn secure_getenv(name: &str) -> Option<OsString> {
    // Step 1. `issetugid()` — see [`process_is_secure`] for what replaces it.
    if process_is_secure() {
        return None;
    }
    // Step 2. `getenv(name)`. The C hands back a borrowed pointer into the
    // process environment; `var_os` copies, which is strictly stronger than
    // the "valid until the next hook call" the hook contract asks for.
    std::env::var_os(name)
}

/// The port's `issetugid()`: did this process gain privilege from the
/// executable it ran?
///
/// ERR-core-api-25, disposition `fix`. The C picks one of four
/// implementations at configure time and the last of them `#define`s
/// `issetugid()` to the constant `1`, so libedit reads *no* environment at
/// all on such a host. That branch is a portability artefact and is
/// deliberately not reproduced; `sem:el.secure-getenv-fn` spells out the real
/// test the port must perform instead, and this is it — the three conditions
/// it names, OR'ed:
///
/// - the loader marked the process secure (`AT_SECURE` in the auxiliary
///   vector, which also covers file capabilities and the other ways an exec
///   can be privileged without the ids differing), or
/// - the real uid differs from the effective uid, or
/// - the real gid differs from the effective gid.
///
/// The id comparisons are `getuid`/`geteuid`/`getgid`/`getegid` through
/// `nshedit-plat`, which reaches them without a libc. `AT_SECURE` still comes
/// out of `/proc/self/auxv`: rustix exposes the auxiliary vector only through
/// the same `runtime` module the signal family is barred from, so the read
/// stays (`plan/decisions/platform-layer.md`, group 11). The answer is
/// cached, because glibc computes `__libc_enable_secure` once during startup
/// and `secure_getenv` reads that cached value — so a later
/// `setuid()`/`setgid()` does not change what it reports, and neither does
/// this.
///
/// The two thirds of the old exposure that a missing `/proc` cost are gone:
/// the ids are now always answerable, so a host with `hidepid` or no `/proc`
/// loses only `AT_SECURE` and this reports whatever the ids say. Only if the
/// ids *and* the auxv were both unavailable would it answer `true` and deny
/// every lookup, which is unreachable on any target this builds for — and
/// would in any case be the default hook failing closed on a platform that
/// genuinely cannot answer, not the C's degenerate always-deny branch.
fn process_is_secure() -> bool {
    static SECURE: OnceLock<bool> = OnceLock::new();
    *SECURE.get_or_init(|| match (auxv_at_secure(), ids_differ()) {
        (None, None) => true,
        (a, b) => a.unwrap_or(false) || b.unwrap_or(false),
    })
}

/// `AT_SECURE` from the auxiliary vector, `None` if it could not be read.
///
/// The vector is a flat array of native-endian `unsigned long` key/value
/// pairs terminated by an `AT_NULL` key, which is the layout `getauxval`
/// walks.
fn auxv_at_secure() -> Option<bool> {
    /// `AT_NULL` — end of the vector.
    const AT_NULL: usize = 0;
    /// `AT_SECURE` — "the loader considers this a secure execution".
    const AT_SECURE: usize = 23;

    let raw = std::fs::read("/proc/self/auxv").ok()?;
    let w = size_of::<usize>();
    for pair in raw.chunks_exact(2 * w) {
        let key = native_word(&pair[..w]);
        if key == AT_NULL {
            return None;
        }
        if key == AT_SECURE {
            return Some(native_word(&pair[w..]) != 0);
        }
    }
    None
}

/// One native-endian `unsigned long` out of `/proc/self/auxv`.
fn native_word(bytes: &[u8]) -> usize {
    let mut w = [0u8; size_of::<usize>()];
    w.copy_from_slice(bytes);
    usize::from_ne_bytes(w)
}

/// Whether real and effective uid, or real and effective gid, differ.
///
/// `Option` only because [`process_is_secure`] pairs it with a source that
/// can genuinely be unavailable; four syscalls that cannot fail always answer
/// `Some`.
fn ids_differ() -> Option<bool> {
    Some(
        nshedit_plat::getuid() != nshedit_plat::geteuid()
            || nshedit_plat::getgid() != nshedit_plat::getegid(),
    )
}

// [spec:libedit:sem:el.editline.el-getenv-fn]
/// One environment lookup on behalf of `el`, through whatever hook it holds.
///
/// Every environment variable libedit reads goes through here, and the rule
/// is explicit that the port must route exactly four lookups through it and
/// no others: `"EDITRC"` and `"HOME"` from [`el_source`], `"TERM"` from
/// `terminal_set` when it is called with a NULL name, and `"EDITOR"` from
/// `vi_histedit`. An application that installs a hook is entitled to see
/// precisely that call pattern, so the other two modules must call this
/// rather than reading the environment themselves.
///
/// # Reconciling the default with the hook type
///
/// The C's step 3 is `el->el_getenv = secure_getenv`, i.e. the default *is* a
/// value of the hook type. That does not survive translation:
/// `sem:el.secure-getenv-fn` has [`secure_getenv`] return an owned
/// `Option<OsString>`, while [`FuncT`] is the C ABI shape
/// `unsafe extern "C" fn(*const c_char) -> *mut c_char`, whose result is
/// borrowed from storage the hook owns. Neither side may change — the rule
/// fixes the Rust signature, the ABI fixes the pointer one.
///
/// So the field carries only what a C *application* installed, and `None`
/// means "no application hook — the built-in default is in force". That is
/// what [`el_init_internal`] leaves behind, and it is the C's step 3
/// faithfully: after construction, a lookup reaches `secure_getenv`.
///
/// Two consequences, both intentional:
/// - `el_get(EL_GETENV)` must report the *address* of `secure_getenv` on a
///   freshly constructed `EditLine`. There is no such address here, so the
///   ABI crate synthesises one — the same division of labour `sig.rs` uses
///   for the file-static `sel`, and the reason `el_wset`/`el_wget` are not in
///   this module.
/// - The C distinguishes "default" from "the caller stored NULL"; this does
///   not, and collapses the second onto the first. Storing NULL makes the
///   next lookup an indirect call through a null pointer — undefined
///   behaviour, ERR-core-api-08, disposition `define`, "reject NULL". Falling
///   back to the built-in is that definition; the ABI crate may additionally
///   reject the `el_set` outright, and nothing here depends on which.
///
/// The result is copied out of the hook's storage before returning, so the
/// "valid at least until the next hook call" clause is satisfied with room to
/// spare and no caller can retain a borrow across a later lookup.
pub(crate) fn el_getenv(el: &EditLine, name: &str) -> Option<OsString> {
    let Some(hook) = el.el_getenv else {
        return secure_getenv(name);
    };
    // The four names are compile-time ASCII literals, so this never fails;
    // the C passes them as string literals for the same reason.
    let cname = CString::new(name).ok()?;
    // SAFETY: `hook` is what an application installed through
    // `el_set(EL_GETENV, ...)`; `def:el.editline.el-getenv-fn` makes it a C
    // function taking one NUL-terminated name, and `cname` is exactly that
    // and outlives the call.
    let value = unsafe { hook(cname.as_ptr()) };
    if value.is_null() {
        // The hook's "unset, or I decline to answer".
        return None;
    }
    // SAFETY: `def:el.editline.el-getenv-fn` is the contract the application
    // accepted when it installed the hook — a NUL-terminated value, valid at
    // least until the next call through the hook. The copy is taken here and
    // now, so nothing outlives that window.
    let bytes = unsafe { CStr::from_ptr(value) }.to_bytes().to_vec();
    Some(OsString::from_vec(bytes))
}

/// C: `fileno(3)`.
///
/// A [`CFile`] is an opaque C `FILE *`; its descriptor lives inside the C
/// library's own object, which `plan/decisions/no-c-ffi.md` forbids this
/// crate from linking against and therefore from reading. There is no route
/// to the real answer here, so this reports -1 — which is exactly what the C
/// stores for a stream that has no underlying descriptor, and
/// `sem:el.el-init-fn` and ERR-core-api-06 (disposition: reproduce the
/// -1-descriptor outcome) describe what follows from it: construction still
/// reports success, `tty_init` then fails, `NO_TTY` gets set, and nothing
/// else notices.
///
/// The real `fileno` belongs to the ABI crate, which is what owns the
/// `FILE *` in the first place, and that is where it now lives: the exported
/// `el_init` calls it and goes straight to [`el_init_fd`], so no C caller
/// reaches this stub. It stays so the C's call graph has a Rust counterpart
/// and so [`el_init`] below reads as the rule writes it; it is not a working
/// stdio bridge, and a Rust caller with a real stream has a descriptor for it
/// and should call [`el_init_fd`].
fn fileno(_stream: CFile) -> i32 {
    -1
}

// [spec:libedit:def:el.el-init-fn]
// [spec:libedit:sem:el.el-init-fn]
/// C: `EditLine *el_init(const char *prog, FILE *fin, FILE *fout, FILE *ferr)`.
///
/// Constructs an editor from three streams, deriving the three descriptors
/// from them. One tail call to [`el_init_fd`] with `fileno` of each stream;
/// `None` is the C's NULL return.
///
/// `prog` is a `&str` because the C's NULL and undecodable cases are
/// undefined behaviour rather than behaviour to reproduce. The streams stay
/// [`CFile`]: they are borrowed for the whole lifetime of the editor, never
/// duplicated and never closed, so there is nothing here to own.
///
/// # Not the entry point. Use [`el_init_fd`]
///
/// Hidden for the same reason [`crate::hist::hist_set`] is: it is here so
/// `nshedit-abi` has a Rust counterpart to call, and a Rust caller cannot use
/// it correctly. A `FILE *` is the C library's object and
/// `plan/decisions/no-c-ffi.md` reserves reaching into one for the ABI crate,
/// which does it properly through `cstdio::fileno_of`. The [`fileno`] this
/// calls is a stub that answers -1 unconditionally.
///
/// So an editor built here has all three descriptors at -1. Every read is
/// `EBADF`, `el_wgets` reports that as end of file (ERR-input-24), and the
/// editor exits before a key is pressed — no prompt, no echo, and no
/// diagnostic, because the C stores a `fileno` of -1 undiagnosed and still
/// reports success. That weak reporting is faithful and is exactly what makes
/// this silent: in the C the -1 means "this stream had no descriptor", and
/// here it means nothing at all.
///
/// A Rust caller has real descriptors already. [`el_init_fd`] takes them.
#[doc(hidden)]
pub fn el_init(prog: &str, fin: CFile, fout: CFile, ferr: CFile) -> Option<Box<EditLine>> {
    // The whole body, one tail call. Nothing is validated: the C's `fileno`
    // runs before any check, so a NULL stream is a null dereference
    // (ERR-core-api-06, disposition `define` for that half — [`CFile`] is a
    // raw pointer here and is never dereferenced, so a null one is simply
    // stored). The evaluation order of the three calls is unspecified in C
    // and has no observable consequence; left to right here.
    el_init_fd(
        prog,
        fin,
        fout,
        ferr,
        fileno(fin),
        fileno(fout),
        fileno(ferr),
    )
}

// [spec:libedit:def:el.el-init-internal-fn]
// [spec:libedit:sem:el.el-init-internal-fn]
/// C: `libedit_private EditLine *el_init_internal(const char *prog, FILE *fin,
/// FILE *fout, FILE *ferr, int fdin, int fdout, int fderr, int flags)`.
///
/// The real constructor: records the streams and descriptors, copies the
/// program name, and brings up every subsystem in the order the C marks
/// "Order is important!!!". `flags` is the initial `el_flags` word and is
/// stored before any subsystem init, so that `terminal_init` raising
/// `EDIT_DISABLED` and `tty_init` raising `NO_TTY` survive.
///
/// `Option<Box<EditLine>>` is the C's `EditLine *`/NULL, and the return type
/// is deliberately no stronger than that. `sem:el.el-init-fn` records that
/// the C reports success while handing back an object whose subsystems
/// silently failed to allocate, and that the one path documented as
/// returning NULL faults instead; reproducing and then correcting that is
/// the body's job and idiomatization's, not the signature's.
// Eight parameters because the C has eight; collapsing them into a builder or
// a stream triple is idiomatization's call, not the translation's.
#[expect(
    clippy::too_many_arguments,
    reason = "the translated constructor keeps three borrowed streams, three descriptors, and initial flags distinct"
)]
pub(crate) fn el_init_internal(
    prog: &str,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
    fdin: i32,
    fdout: i32,
    fderr: i32,
    flags: i32,
) -> Option<Box<EditLine>> {
    // Step 1. `el_calloc(1, sizeof(*el))`: zero-filled, so every pointer
    // field starts NULL, every counter 0 and every embedded subsystem struct
    // blank. Rust aborts rather than returning null on allocation failure, so
    // the C's "if it returns NULL, return NULL" has no counterpart.
    let mut el = Box::new(blank_editline());

    // Step 2. Recorded, not duplicated, not validated, no ownership taken.
    el.el_infile = fin;
    el.el_outfile = fout;
    el.el_errfile = ferr;
    el.el_infd = fdin;
    el.el_outfd = fdout;
    el.el_errfd = fderr;

    // Step 3. C: `el->el_getenv = secure_getenv`. `None` is that default —
    // "no application hook installed, so lookups reach the built-in". See
    // [`el_getenv`] for why the default cannot be stored as a [`FuncT`] and
    // what the ABI crate owes as a result. Already `None` from step 1; the
    // assignment is written out because the rule numbers it a step and
    // because the field's initial value is load-bearing for security.
    el.el_getenv = None;

    // Step 4. C: `el->el_prog = wcsdup(ct_decode_string(prog, &el->el_scratch))`.
    // Decode into the object's shared scratch buffer, then take a private
    // copy the object owns until `el_end`.
    //
    // ERR-core-api-01, disposition `define`: the C's NULL test covers only
    // `wcsdup`'s allocation failure, and `ct_decode_string`'s NULL — a NULL
    // `prog`, or one that is not a valid multibyte string in the current
    // `LC_CTYPE` — goes straight into `wcsdup` and is dereferenced. NULL
    // cannot arrive through a `&str`; an undecodable `prog` can, and fails
    // construction here rather than crashing.
    //
    // ERR-core-api-12, disposition `fix`: the C's `el_free(el)` on this path
    // releases the object but not the `el_scratch.wbuff` this step just
    // allocated, which only `el_end` ever frees. Dropping the box takes the
    // buffer with it, so there is no leak to reproduce.
    let prog_wide = ct_decode_string(Some(prog.as_bytes()), &mut el.el_scratch)?.to_vec();
    el.el_prog = prog_wide;

    // Step 5, and it happens *before* any subsystem init on purpose:
    // `terminal_init` can raise `EDIT_DISABLED` and `tty_init` can raise
    // `NO_TTY`, and both would be wiped out by a later assignment.
    el.el_flags = flags;

    // Step 6. "Order is important!!!", says the C, and it is: `map_init` must
    // follow `terminal_init` (whose `terminal_bind_arrow` bails out while
    // `el_map.key` is still NULL, which is what stops it installing garbage
    // bindings), and `ch_init` must follow `map_init` (it sets
    // `el_map.current = el_map.key`).
    //
    // ERR-core-api-02, disposition `define`: the C discards seven of these
    // results and hands the caller a "successful" object with NULL buffers
    // in it, each dereferenced without a guard the first time its feature is
    // used. Construction fails here instead. The one documented exception is
    // `hist_init`, whose rule is explicit that its failure must NOT be made
    // fatal — `ch_enlargebufs` repairs the state through `hist_enlargebuf`
    // (ERR-history-11) — so that result stays discarded, as in the C.
    //
    // Failing construction drops the box, which runs each already-initialised
    // subsystem's `Drop` rather than its `*_end`. That is sound for every one
    // of them: at this point none has touched anything outside the object —
    // `terminal_init` releases its own partial allocations before returning
    // -1, and `sig_init` installs no handler.

    // 6.1. The only init whose failure is fatal in the C too.
    if terminal_init(&mut el) == -1 {
        return None;
    }
    // 6.2.
    if keymacro_init(&mut el) == -1 {
        return None;
    }
    // 6.3.
    if map_init(&mut el) == -1 {
        return None;
    }
    // 6.4. The only init whose failure is *recorded* rather than ignored, and
    // the flag is consulted exactly once — by `el_end`, to decide whether to
    // restore the terminal modes. Note `tty_init` returns 0 without doing
    // anything while `EDIT_DISABLED` is set, so a `TERM=emacs` EditLine ends
    // up with `NO_TTY` clear and `el_tty.t_initialized` still 0.
    if tty_init(&mut el) == -1 {
        el.el_flags |= NO_TTY;
    }
    // 6.5.
    if ch_init(&mut el) == -1 {
        return None;
    }
    // 6.6.
    if search_init(&mut el) == -1 {
        return None;
    }
    // 6.7. Deliberately discarded; see ERR-history-11 above.
    let _ = hist_init(&mut el);
    // 6.8. Cannot fail.
    if prompt_init(&mut el) == -1 {
        return None;
    }
    // 6.9.
    if sig_init(&mut el) == -1 {
        return None;
    }
    // 6.10. Returns void.
    literal_init(&mut el);
    // 6.11. ERR-core-api-03, disposition `define`. In the C this outcome is
    // unreachable: `read_init` returns -1 either before allocating `el_read`
    // or after calling `read_end` itself, so `el->el_read` is NULL either
    // way, and `el_end` then calls `read_end` unconditionally — whose first
    // act dereferences it. The process faults instead of returning NULL, and
    // on one of the two paths it is a double `read_end` as well. Here
    // `el_read` is an `Option` that `read_end` reads as "nothing to tear
    // down", so the teardown below is safe to run and this actually returns
    // `None`, which is what the rule requires.
    if read_init(&mut el) == -1 {
        el_end(Some(el));
        return None;
    }

    // Step 7.
    Some(el)
}

/// The C's `el_calloc(1, sizeof(struct editline))`: an all-zero `EditLine`.
///
/// Written out because none of the ported subsystem types derives `Default` —
/// they are literal translations of C structs whose only constructor was
/// `calloc` — and because several of them have no zero value that Rust can
/// spell on its own. Where that happens the choice is the C's zero state
/// under this crate's representation conventions:
///
/// - a `char *`/`wchar_t *`/`T **` that `calloc` left NULL is an empty
///   `Vec`, which is what every `*_init` in the crate also leaves behind on
///   its own failure path;
/// - `el_map.current` is a selector, not a pointer, and has no NULL: `Key` is
///   the value `ch_init` installs, so nothing observes the difference;
/// - `el_keymacro.val` is the C's union, whose zeroed state is the `XK_NOD`
///   "the union holds a NULL `str`" case that `def:keymacro.keymacro-value-t`
///   maps to `Str` with an empty buffer;
/// - `el_tty.t_t` is an array of structs carrying a `&'static str` name, and
///   `tty_init` `memcpy`s the compiled-in `ttyperm` table over the whole
///   thing before anything reads it.
pub(crate) fn blank_editline() -> EditLine {
    EditLine {
        el_prog: Vec::new(),
        el_infile: ptr::null_mut(),
        el_outfile: ptr::null_mut(),
        el_errfile: ptr::null_mut(),
        el_infd: 0,
        el_outfd: 0,
        el_errfd: 0,
        el_flags: 0,
        el_cursor: CoordT { h: 0, v: 0 },
        el_display: Vec::new(),
        el_vdisplay: Vec::new(),
        el_data: ptr::null_mut(),
        el_line: ElLineT {
            buffer: Vec::new(),
            cursor: 0,
            lastchar: 0,
            limit: 0,
        },
        el_state: ElStateT {
            inputmode: 0,
            doingarg: 0,
            argument: 0,
            metanext: 0,
            lastcmd: 0,
            thiscmd: 0,
            thisch: 0,
        },
        el_terminal: ElTerminalT {
            t_name: None,
            t_size: CoordT { h: 0, v: 0 },
            t_flags: 0,
            t_buf: Vec::new(),
            t_loc: 0,
            t_str: Vec::new(),
            t_val: Vec::new(),
            t_cap: Vec::new(),
            t_entry: None,
            t_fkey: Vec::new(),
        },
        el_tty: ElTtyT {
            t_t: std::array::from_fn(|_| {
                std::array::from_fn(|_| TtypermEntry {
                    t_name: "",
                    t_setmask: 0,
                    t_clrmask: 0,
                })
            }),
            t_c: [[0; C_NCC]; NN_IO],
            t_or: blank_termios(),
            t_ex: blank_termios(),
            t_ed: blank_termios(),
            t_ts: blank_termios(),
            t_tabs: 0,
            t_eight: 0,
            t_speed: 0,
            t_mode: 0,
            t_vdisable: 0,
            t_initialized: 0,
        },
        el_refresh: ElRefreshT {
            r_cursor: CoordT { h: 0, v: 0 },
            r_oldcv: 0,
            r_newcv: 0,
        },
        el_prompt: blank_prompt(),
        el_rprompt: blank_prompt(),
        el_literal: ElLiteralT {
            l_buf: Vec::new(),
            l_idx: 0,
            l_len: 0,
        },
        el_chared: ElCharedT {
            c_undo: CUndoT {
                len: 0,
                cursor: 0,
                buf: Vec::new(),
            },
            c_kill: CKillT {
                buf: Vec::new(),
                last: 0,
                mark: 0,
            },
            c_redo: CRedoT {
                buf: Vec::new(),
                pos: 0,
                lim: 0,
                cmd: 0,
                ch: 0,
                count: 0,
                action: 0,
            },
            c_vcmd: CVcmdT { action: 0, pos: 0 },
            c_resizefun: None,
            c_aliasfun: None,
            c_resizearg: ptr::null_mut(),
            c_aliasarg: ptr::null_mut(),
        },
        el_map: ElMapT {
            alt: Vec::new(),
            key: Vec::new(),
            current: ElMapCurrent::Key,
            emacs: None,
            vic: None,
            vii: None,
            r#type: 0,
            help: Vec::new(),
            func: Vec::new(),
            nfunc: 0,
            wordchars: None,
        },
        el_keymacro: ElKeymacroT {
            buf: Vec::new(),
            map: None,
            val: KeymacroValueT::Str(Vec::new()),
        },
        el_history: ElHistoryT {
            buf: Vec::new(),
            last: 0,
            eventno: 0,
            src: HistSource::None,
            ev: HistEventW {
                num: 0,
                str: ptr::null(),
            },
        },
        el_search: ElSearchT {
            patbuf: Vec::new(),
            patlen: 0,
            patdir: 0,
            chadir: 0,
            chacha: 0,
            chatflg: 0,
        },
        el_signal: None,
        el_read: None,
        el_visual: blank_ct_buffer(),
        el_scratch: blank_ct_buffer(),
        el_getenv: None,
    }
}

/// The zeroed `struct termios` `calloc` leaves in an `el_tty` slot.
///
/// `c_cc` has to be `NCCS` long, not empty: the C's `calloc` gives a
/// `cc_t c_cc[NCCS]` of zeros, and `tty.rs` reads and writes that row by `V*`
/// subscript through `cc_get`/`cc_set`, which are bounds-safe and therefore
/// *silent* — a short row reads every subscript as 0 and swallows every
/// write. That was invisible while `tcgetattr` was a stub, because nothing
/// ever filled a `c_cc` or pushed one; with the platform layer in place it
/// zeroes the terminal's whole control-character column, including `VMIN`,
/// so a raw-mode `read` returns immediately instead of blocking. Hence
/// `termios_zeroed`, which exists for exactly this call site.
fn blank_termios() -> Termios {
    termios_zeroed()
}

/// The zeroed `el_prompt_t` `calloc` leaves in `el_prompt` and `el_rprompt`.
fn blank_prompt() -> ElPromptT {
    ElPromptT {
        p_func: None,
        p_pos: CoordT { h: 0, v: 0 },
        p_ignore: 0,
        p_wide: 0,
    }
}

/// The zeroed `ct_buffer_t` `calloc` leaves in the three conversion buffers,
/// which `sem:chartype.ct-buffer-t` records as satisfying the module's
/// `cbuff.len() == csize` invariant.
fn blank_ct_buffer() -> CtBufferT {
    CtBufferT {
        cbuff: Vec::new(),
        csize: 0,
        wbuff: Vec::new(),
        wsize: 0,
    }
}

// [spec:libedit:def:el.el-init-fd-fn]
// [spec:libedit:sem:el.el-init-fd-fn]
/// C: `EditLine *el_init_fd(const char *prog, FILE *fin, FILE *fout,
/// FILE *ferr, int fdin, int fdout, int fderr)`.
///
/// Constructs an editor from three streams plus three explicitly supplied
/// descriptors, for callers whose streams and descriptors are not related by
/// `fileno`. One tail call to [`el_init_internal`] with `flags` of 0.
///
/// Nothing is validated and no consistency is enforced between the streams
/// and the descriptors: reads and writes go through the streams while
/// terminal size and mode queries go through the descriptors, whatever they
/// happen to refer to.
pub fn el_init_fd(
    prog: &str,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
    fdin: i32,
    fdout: i32,
    fderr: i32,
) -> Option<Box<EditLine>> {
    // The whole body, one tail call. The trailing 0 is the initial `el_flags`
    // word: every flag clear — signal handling off, tty assumed usable,
    // editing enabled, buffered, wide history, tty reset on setup enabled,
    // `FIXIO` off. The readline layer is the only caller that wants anything
    // else (`NO_RESET`), which is the entire reason `el_init_internal` exists
    // separately.
    el_init_internal(prog, fin, fout, ferr, fdin, fdout, fderr, 0)
}

/// Construct an editor without resetting terminal modes during tty setup.
///
/// This is the safe semantic distinction used by readline: the ordinary
/// libedit constructor resets the saved modes during setup, while readline
/// preserves them across repeated reads. Hidden until the native editor
/// builder replaces the translated constructor surface.
#[doc(hidden)]
pub fn el_init_fd_preserving_terminal(
    prog: &str,
    fin: CFile,
    fout: CFile,
    ferr: CFile,
    fdin: i32,
    fdout: i32,
    fderr: i32,
) -> Option<Box<EditLine>> {
    el_init_internal(prog, fin, fout, ferr, fdin, fdout, fderr, NO_RESET)
}

// [spec:libedit:def:el.el-end-fn]
// [spec:libedit:sem:el.el-end-fn]
/// C: `void el_end(EditLine *el)`.
///
/// Destroys an editor: [`el_reset`] first, so the terminal is restored while
/// the tty subsystem is still live, then every subsystem torn down in the
/// forward order of [`el_init_internal`]'s step 6 — *not* its reverse, and
/// not the order a derived `Drop` would give.
///
/// Consumes the editor, which is why it takes it by value. `None` is the C's
/// NULL argument, the one NULL-tolerant entry point in this file. The three
/// streams and three descriptors are borrowed, so they are neither flushed
/// nor closed; `el_data`, the `el_getenv` hook, and every callback installed
/// through `EL_PROMPT`, `EL_RESIZE`, `EL_ALIAS_TEXT`, `EL_HIST` and
/// `EL_GETCFN` are dropped without notification.
pub fn el_end(el: Option<Box<EditLine>>) {
    // Step 1. The one NULL-tolerant entry point in the file.
    let Some(mut el) = el else {
        return;
    };

    // Step 2. First, so the terminal modes are restored while the tty
    // subsystem is still live.
    el_reset(&mut el);

    // Step 3, in exactly this order. ERR-core-api-24, disposition "reproduce
    // the order explicitly": this is NOT the reverse of construction. It is
    // the same *forward* order as `el_init_internal`'s step 6, with one
    // exception — `read_end` runs sixth here and `read_init` runs eleventh
    // there. A `Drop` impl over fields declared in init order would run the
    // opposite order silently, which is why the sequence is spelled out as
    // calls and why the object is only dropped afterwards. Nothing in the
    // current subsystem set depends on it (every `*_end` touches only its own
    // fields and all but `read_end` are idempotent), but a future subsystem
    // interaction would differ.
    // 1. Also frees the display buffers, which is why `el_display` and
    //    `el_vdisplay` are not released separately below.
    terminal_end(&mut el);
    // 2.
    keymacro_end(&mut el);
    // 3. Also frees the name and description of every `EL_ADDFN` function.
    map_end(&mut el);
    // 4. Skipped entirely when `NO_TTY` is set — one of the three ways the
    //    original terminal modes can go unrestored, the other two being
    //    `tty_end`'s own early exits on `EDIT_DISABLED` and on
    //    `t_initialized == 0`. A `TERM=emacs` EditLine hits the last of those.
    if el.el_flags & NO_TTY == 0 {
        tty_end(&mut el, TCSAFLUSH);
    }
    // 5.
    ch_end(&mut el);
    // 6. Sixth, not eleventh; see above. Called unconditionally, as in the C:
    //    ERR-core-api-03 puts the "tolerate an uninitialised read subsystem"
    //    obligation on `read_end`, and `el_read`'s `Option` is where that
    //    tolerance is expressed (`None` before `read_init` and after
    //    `read_end`). The C's unguarded dereference is what makes the
    //    constructor's failure path fault; guarding it here instead would
    //    just hide the requirement one level up.
    read_end(&mut el);
    // 7.
    search_end(&mut el);
    // 8.
    hist_end(&mut el);
    // 9. A no-op.
    prompt_end(&mut el);
    // 10.
    sig_end(&mut el);
    // 11.
    literal_end(&mut el);

    // Step 4. The C then frees `el_prog`, the six conversion buffers and the
    // object itself; dropping the box does all of it, in one go. Ownership,
    // explicitly, and all of it is what dropping already gives:
    // - `el_infile`/`el_outfile`/`el_errfile` are NOT closed and
    //   `el_infd`/`el_outfd`/`el_errfd` are NOT closed; they were borrowed at
    //   construction and go back to the caller untouched. Both are raw
    //   values here, so dropping them does nothing, which is the point.
    // - `el_data` is neither freed nor inspected, and the `el_getenv` hook is
    //   neither called nor freed.
    // - Callbacks installed through `EL_PROMPT`, `EL_RESIZE`,
    //   `EL_ALIAS_TEXT`, `EL_HIST` and `EL_GETCFN`, and their opaque
    //   arguments, are dropped without notification: there is no destructor
    //   callback. The history object behind `EL_HIST` belongs to the caller
    //   and must be destroyed separately with `history_end`.
    // Not idempotent and not re-entrant in the C — the object is freed and
    // there is no "already ended" marker — which is what taking the box by
    // value expresses: a second `el_end` on the same object cannot be
    // written.
}

// [spec:libedit:def:el.el-reset-fn]
// [spec:libedit:sem:el.el-reset-fn]
/// C: `void el_reset(EditLine *el)`.
///
/// Abandons the line being edited and puts the terminal back the way the
/// application had it: `tty_cookedmode` then `ch_reset`, in that order,
/// because the cooked-mode switch is the last chance to drain output written
/// for the line about to be discarded. Both results are discarded, so a
/// failure to restore the modes is silent.
pub fn el_reset(el: &mut EditLine) {
    // Step 1. Restore the saved `EX_IO` termios with `TCSADRAIN`; a no-op if
    // the terminal is already in `EX_IO` mode or if `EDIT_DISABLED` is set.
    // The result is discarded, so a failure to restore is silent.
    //
    // Order is load-bearing: this must run *before* the editor state is
    // discarded, since it is the last chance to drain output written for the
    // line being abandoned.
    tty_cookedmode(el);

    // Step 2. The C carries an `XXX: Do we want that?` comment here; the
    // answer is frozen either way, because the behaviour crosses the ABI.
    ch_reset(el);
}

/// Steps 1 to 5 of [`el_source`]: which file `.editrc` means this time.
///
/// `None` is the -1 that both resolution failures produce — no `HOME` to
/// build a path from, and a name that is empty once truncated at its first
/// NUL. They are one answer here because they are one answer to the caller,
/// who cannot tell either of them from a file that would not open
/// (ERR-core-api-21).
///
/// The C carries a separate `path` pointer alongside `fname` purely so it can
/// `el_free` the constructed one on the way out; the owned `Vec` here is
/// both, and drops itself on every exit. ERR-core-api-22 is the disagreement
/// about whether the step-5 early return leaks that buffer —
/// `sem:el.el-source-fn` is right that it cannot, because that return is only
/// reachable while `path` is still NULL (a constructed path always ends in
/// `.editrc` and so is never empty). Either way there is nothing to leak.
///
/// ERR-core-api-33/35: the C initialises `fp = NULL` and then guards its only
/// `fopen` with a redundant `if (fp == NULL)`. That is the vestige of a
/// removed `./.editrc` attempt, and it is why `histedit.h` still claims
/// `el_source` reads "$PWD/.editrc or $HOME/.editrc". It does not: there is
/// no `$PWD` lookup and no attempt at `./.editrc`. Dead code, not ported.
fn editrc_path(el: &EditLine, fname: Option<&Path>) -> Option<Vec<u8>> {
    let mut name = match fname {
        // Step 1. Used exactly as given: no `~` expansion, no search path, no
        // directory prefix.
        Some(f) => f.as_os_str().as_bytes().to_vec(),
        None => match el_getenv(el, "EDITRC") {
            // Step 2, again verbatim. The default hook is `secure_getenv`, so
            // a set-uid/set-gid process gets `None` here and never honours
            // `EDITRC`.
            Some(editrc) => editrc.into_vec(),
            None => {
                // Step 3.
                let mut path = el_getenv(el, "HOME")?.into_vec();
                // Step 4. C: a `strlen(HOME) + sizeof("/.editrc")` buffer and
                // `snprintf(path, plen, "%s%s", ptr, elpath + (*ptr == '\0'))`
                // — exactly enough room, so nothing truncates. Skipping the
                // leading `/` when `HOME` is empty produces the *relative*
                // path `.editrc`, which `fopen` resolves against the current
                // working directory; that is the only way `el_source` ever
                // looks at the current directory. The C's allocation-failure
                // -1 has no counterpart (Rust aborts instead).
                let suffix: &[u8] = if path.is_empty() {
                    b".editrc"
                } else {
                    b"/.editrc"
                };
                path.extend_from_slice(suffix);
                path
            }
        },
    };

    // A C file name ends at its first NUL, so anything past one is invisible
    // to `fopen`; truncating here is what makes the step-5 test below the C's
    // `fname[0] == '\0'` rather than merely "empty".
    if let Some(nul) = name.iter().position(|&b| b == 0) {
        name.truncate(nul);
    }

    // Step 5. Only ever rejects a caller-supplied "" or an `EDITRC` set to
    // "": a constructed path is never empty.
    (!name.is_empty()).then_some(name)
}

// [spec:libedit:def:el.el-source-fn]
// [spec:libedit:sem:el.el-source-fn]
/// C: `int el_source(EditLine *el, const char *fname)`.
///
/// Reads an `.editrc` and runs each line through `parse_line`. `None` for
/// `fname` is the C's NULL: the file is then `EDITRC` from the environment
/// hook, or `$HOME/.editrc`, or — when `HOME` is the empty string — the
/// relative path `.editrc`. Despite what `histedit.h` claims, there is no
/// `$PWD` lookup.
///
/// The `i32` is the C's status and keeps its oddities: 0 for success or for
/// a file that executed nothing, +1 when the *last* executed builtin failed
/// (`el_wparse` negates), and -1 for an unknown command, a line that
/// tokenised to zero words (a whitespace-only line does exactly this, and
/// aborts the rest of the file), or any of the early exits. There is no way
/// for the caller to tell "could not open" from "a line failed".
pub fn el_source(el: &mut EditLine, fname: Option<&Path>) -> i32 {
    // Steps 1 to 5.
    let Some(name) = editrc_path(el, fname) else {
        return -1;
    };

    // Step 6. `errno` is left as the C's `fopen` set it but is not reported,
    // and there is no way for the caller to tell this apart from a line that
    // failed to parse — both are -1 (ERR-core-api-21).
    let Ok(file) = File::open(OsStr::from_bytes(&name)) else {
        return -1;
    };
    let mut reader = BufReader::new(file);

    // `error` is assigned by every line that reaches dispatch and is
    // therefore the result of the LAST such line, not an accumulation
    // (ERR-core-api-21, disposition `reproduce`). It stays 0 when no line
    // ever gets that far.
    let mut error = 0;

    // The C's `getline` with an initially NULL buffer: one heap buffer
    // allocated on the first line, reused and grown for the rest. `Ok(0)` is
    // its -1 at end of file; a read error ends the loop the same way, as the
    // C's does.
    let mut line: Vec<u8> = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        // a. Only the first byte is tested, so a truly empty line is skipped
        //    but a line of spaces is not, and neither is a CRLF file's
        //    "\r\n".
        if line[0] == b'\n' {
            continue;
        }
        // b. A final line with no trailing newline keeps all its bytes. No
        //    other trailing whitespace is stripped, and `'\r'` is not.
        if line.last() == Some(&b'\n') {
            line.pop();
        }

        // c. An invalid multibyte sequence or an allocation failure skips the
        //    line silently, with no diagnostic and no effect on the return
        //    value. An embedded NUL truncates the line as far as decoding is
        //    concerned; the remainder is never seen.
        //
        //    The C hands `parse_line` an interior pointer into `el_scratch`;
        //    that cannot be done here, because `parse_line` needs the
        //    `EditLine` mutably and the scratch buffer is part of it. The copy
        //    is the only departure, and it costs nothing observable: the
        //    decoded line still lands in `el_scratch`, so this call still
        //    invalidates anything else holding a pointer previously returned
        //    from `ct_decode_string` on the same `EditLine`.
        let Some(decoded) = ct_decode_string(Some(&line), &mut el.el_scratch) else {
            continue;
        };
        let decoded = decoded.to_vec();

        // d. Advance past leading `iswspace`, stopping at the terminating
        //    NUL — which is the end of the slice here, since
        //    `ct_decode_string` returns the content without its terminator.
        //
        //    Queried per line rather than once for the file, because the C's
        //    `iswspace` reads `LC_CTYPE` on every call and `crate::locale`
        //    keeps a snapshot that `locale::refresh` can replace.
        let cs = locale::charset();
        let mut dptr = 0;
        while dptr < decoded.len() && locale::iswspace(cs, decoded[dptr]) {
            dptr += 1;
        }

        // e. Comment.
        if decoded.get(dptr) == Some(&u32::from(b'#')) {
            continue;
        }

        // f. ERR-core-api-20, disposition `reproduce`: a whitespace-only line
        //    arrives here as an empty wide string, tokenises to zero words,
        //    and `el_wparse` rejects that with -1 — which breaks the loop, so
        //    every later line in the file is never read and `el_source`
        //    returns -1. Only a literally empty line is safe. The same
        //    applies to a typo part-way through an `.editrc`.
        error = parse_line(el, &decoded[dptr..]);
        if error == -1 {
            break;
        }
    }

    // Step 7. The C frees the `getline` buffer and the path buffer and
    // `fclose`s the file with the result discarded; all three drop here.
    error
}

// [spec:libedit:def:el.el-resize-fn]
// [spec:libedit:sem:el.el-resize-fn]
/// C: `void el_resize(EditLine *el)`.
///
/// Re-reads the terminal window size and rebuilds the display buffers if it
/// changed, with `SIGWINCH` blocked across the whole operation and the
/// caller's previous mask restored with `SIG_SETMASK` rather than
/// `SIG_UNBLOCK`. Blocking does not discard the signal, so resize events are
/// not lost; the block only makes query-and-rebuild atomic against libedit's
/// own handler.
///
/// The size query goes to the *input* descriptor. Every result on the path —
/// the mask calls, and `terminal_change_size`'s -1 — is discarded, so an
/// out-of-memory during a resize is silent.
pub fn el_resize(el: &mut EditLine) {
    // Step 1. `sigemptyset`/`sigaddset(SIGWINCH)`/`sigprocmask(SIG_BLOCK)`.
    //
    // The port's own `sig_handler` cannot re-enter this — it is deferred work
    // run from the read loop on the editing thread, which is what the
    // `&mut EditLine` in its signature commits to — so the reallocation below
    // is already safe from libedit's handler by the exclusive borrow. The
    // block is kept anyway, because it is not only libedit's handler that the
    // rule is written against: the mask is process-wide state a caller can
    // observe, an application's own `SIGWINCH` handler runs inside this
    // window too, and `sem:el.el-resize-fn` names the pair as the behaviour.
    // ERR-terminal-55 stands as it does in the C — this is `sigprocmask`, not
    // `pthread_sigmask`, so in a multi-threaded process the effect is
    // unspecified.
    let oset = block_sigwinch();

    // Step 2.
    let mut lins = 0;
    let mut cols = 0;
    // `terminal_get_size` seeds both from the currently loaded capability
    // values and then overrides them from `TIOCGWINSZ` on `el_infd` — the
    // *input* descriptor, not the output one `tty_setup` uses for its
    // `isatty` check (ERR-terminal-35) — ignoring any field the kernel
    // reports as zero and ignoring `ioctl` failure entirely. It returns
    // non-zero exactly when at least one of the two differs from the stored
    // capability value, so the rebuild below runs only on a real change.
    if terminal_get_size(el, &mut lins, &mut cols) != 0 {
        // ERR-core-api-23, disposition "reproduce the silent return": the -1
        // that `terminal_change_size` returns when the buffer rebuild fails
        // is discarded, so an out-of-memory during a resize is silent. Not
        // leaving the display state inconsistent afterwards is that
        // function's obligation, not this one's.
        terminal_change_size(el, lins, cols);
    }

    // Step 3. `SIG_SETMASK` to the mask captured at step 1, not
    // `SIG_UNBLOCK`, so a `SIGWINCH` the caller had already blocked stays
    // blocked. Blocking never discarded anything, so a resize that arrived
    // inside the window is delivered here.
    if let Some(oset) = oset.as_ref() {
        let _ = set_sigmask(oset);
    }
}

// [spec:libedit:def:el.el-beep-fn]
// [spec:libedit:sem:el.el-beep-fn]
/// C: `void el_beep(EditLine *el)`.
///
/// Rings the terminal bell — a public re-export of `terminal_beep` with no
/// added logic. Reports no errors, does not flush, and does not touch the
/// cursor or any editing state.
pub fn el_beep(el: &mut EditLine) {
    // The whole body. `terminal_beep` emits the terminal's audible-bell
    // capability if the loaded description has a non-empty one, and otherwise
    // writes a literal ASCII BEL (0x07). Nothing is flushed, so the bell may
    // not reach the terminal until the next refresh.
    terminal_beep(el);
}

// [spec:libedit:def:el.el-editmode-fn]
// [spec:libedit:sem:el.el-editmode-fn]
/// C: `libedit_private int el_editmode(EditLine *el, int argc,
/// const wchar_t **argv)`.
///
/// The `edit` builtin, dispatched from `.editrc` or `el_parse`, so
/// `argv[0]` is `edit` and `argv[1]` is the single required operand. `on`
/// clears `EDIT_DISABLED` and *then* calls `tty_rawmode`; `off` calls
/// `tty_cookedmode` and *then* sets the flag. Both orders are load-bearing —
/// each tty call returns immediately while `EDIT_DISABLED` is set — and both
/// tty results are discarded, so a failed mode change still reports success
/// and leaves `el_flags` out of step with the terminal.
///
/// The C's `argc` is dropped: its three rejections (`argv` NULL, `argc != 2`,
/// `argv[1]` NULL) all return -1 with no message and no state change, so
/// `argv.len() != 2` is observationally the same test. The wide operand is
/// `&[u32]` per the crate's `wchar_t` convention, compared for exact
/// equality — case-sensitive, no abbreviations.
pub(crate) fn el_editmode(el: &mut EditLine, argv: &[&[u32]]) -> i32 {
    // Step 1. Exactly one operand is required: bare `edit` and
    // `edit on somethingelse` are both rejected, with no message and no state
    // change. The C's three tests (`argv` NULL, `argc != 2`, `argv[1]` NULL)
    // all produce this same -1, so the slice length is the whole test.
    if argv.len() != 2 {
        return -1;
    }
    let how = argv[1];

    // Step 2. Exact wide string equality, case-sensitive, no abbreviations.
    if wcs_eq_ascii(how, "on") {
        // The order is required, not incidental: `tty_rawmode` returns 0
        // immediately while `EDIT_DISABLED` is set, so the flag has to come
        // down before the mode change is attempted.
        el.el_flags &= !EDIT_DISABLED;
        // ERR-core-api-19, disposition `reproduce`: the result is discarded,
        // so a failure to change the terminal mode still reports success with
        // `el_flags` already updated, leaving the flag and the actual
        // terminal state out of step.
        tty_rawmode(el);
    } else if wcs_eq_ascii(how, "off") {
        // Step 3, the mirror of the same constraint: `tty_cookedmode` also
        // bails out while `EDIT_DISABLED` is set, so the terminal must be
        // restored before the flag goes up. Result discarded as above.
        tty_cookedmode(el);
        el.el_flags |= EDIT_DISABLED;
    } else {
        // Step 4, the only case that produces output. C:
        // `fprintf(el->el_errfile, "edit: Bad value `%ls'.\n", how)`, with the
        // result discarded.
        //
        // `%ls` converts through the current `LC_CTYPE`, which is what
        // `ct_encode_string` does. A local conversion buffer, not one of the
        // `EditLine`'s: the C's `fprintf` uses stdio's own internal state and
        // does not disturb `el_scratch` or `el_visual`, and borrowing one of
        // those would invalidate pointers the C leaves alone.
        let mut conv = blank_ct_buffer();
        let mut msg = b"edit: Bad value `".to_vec();
        if let Some(encoded) = ct_encode_string(Some(how), &mut conv) {
            msg.extend_from_slice(encoded);
        }
        msg.extend_from_slice(b"'.\n");
        el.write_errfile(&msg);
        return -1;
    }

    // Step 5.
    0
}

/// C: `wcscmp(s, L"…") == 0` against an ASCII literal.
///
/// The C's `argv` entries are NUL-terminated; the tokenizer hands the port
/// slices that may or may not carry the terminator
/// (`sem:tokenizer.fun-tok-finish-fn`), so the comparison stops at the first
/// NUL exactly as `wcscmp` would.
fn wcs_eq_ascii(s: &[u32], lit: &str) -> bool {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    let s = &s[..end];
    s.len() == lit.len() && s.iter().zip(lit.bytes()).all(|(&c, b)| c == u32::from(b))
}

#[cfg(test)]
mod tests;
