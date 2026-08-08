//! Line editing, history and tokenization — a Rust re-implementation of
//! libedit, and the library nsh links.
//!
//! [`domain`] and [`editor`] are the Rust-native API being built around
//! private typed state. The remaining public modules are the transitional
//! compatibility engine: they still mirror the C source file for file so
//! that each `sem` rule in `docs/spec/port/src/` has one obvious home until
//! its replacement concern takes over and the compatibility adapter stops
//! reaching it.
//!
//! In that transitional engine, text is carried as `u32`, not `char`, and
//! the screen image stores sentinel values that are not Unicode scalars — see
//! `docs/spec/port/src/literal.md` — and the C admits `wchar_t` values that
//! `char` forbids.
//!
//! # Representation conventions for the ported types
//!
//! The types below are a literal re-implementation of the C, so they keep its
//! field names, field order and grouping. Where a C type has no direct Rust
//! equivalent the following conventions apply, uniformly:
//!
//! - **`wchar_t` / `wint_t` → `u32`.** Never `char`. Lone surrogates,
//!   `(wint_t)-1` and the `EL_LITERAL` sentinel (bit 31) all reach these
//!   fields; see `docs/spec/port/src/chartype.md` and
//!   `docs/spec/port/src/literal.md`.
//! - **An owned heap buffer → `Vec<T>` / `String`.** The C's `malloc`ed
//!   `wchar_t *`, `char *` and `T **` arrays become owning containers. The
//!   C's separate size/capacity fields were kept through the translation,
//!   because the bodies refer to them by name. That reason expired with the
//!   translation, so a field that is *exactly* the container's own length now
//!   goes: it is an invariant nothing can enforce, and every reader ends up
//!   hedging with `.min(buf.len())` against a state no writer can produce.
//!   `ElHistoryT::sz` was the one such field and it is gone. Fields that
//!   merely *relate* to the length stay — `el_line.limit` is
//!   `buffer.len() - EL_LEAVE` and `c_redo.lim` deliberately lags a
//!   reallocation (ERR-buffer-20), so neither is derivable.
//! - **A pointer *into* an owned buffer → an index (`usize`).** This covers
//!   every place the C rebases after `realloc`, which
//!   `sem:chared.ch-enlargebufs-fn` records as undefined
//!   behaviour in the original. `el_line.cursor`, `c_kill.last`,
//!   `c_redo.pos`, the tokenizer's `argv` slots and friends are all offsets.
//!   Each such field says which buffer it indexes, because the C is not
//!   always consistent about that (`c_kill.mark` indexes the *line*, not the
//!   kill buffer).
//! - **A pointer to a static table → `&'static [T]`.** Concrete `'static`
//!   only; no ported type is generic or takes a lifetime parameter.
//! - **A `const wchar_t *` that is a static literal in some entries and an
//!   owned `wcsdup` in others → `Cow<'static, [u32]>`**, which is exactly
//!   that distinction (see `el_bindings_t`).
//! - **An opaque client cookie (`void *`), a `FILE *`, or a `char *` that
//!   crosses the C ABI borrowed → a raw pointer.** These are values the
//!   library stores and hands back untouched; the ABI freezes them, so there
//!   is nothing here to own. See `plan/decisions/no-c-ffi.md`.
//! - **A callback typedef → `unsafe extern "C" fn` with the C's own
//!   parameters.** Every one of libedit's callback typedefs names a slot an
//!   application fills through `el_set`, `el_wset` or `history`, so the value
//!   that lands in it is a C function pointer and nothing else can be stored
//!   there: `EditLine *` stays `*mut EditLine` and not `&mut EditLine`,
//!   out-parameters stay raw pointers, and calling one is `unsafe`. That
//!   covers `el_pfunc_t`, `el_rfunc_t`, `el_zfunc_t`, `el_afunc_t`,
//!   `el_func_t`, `func_t`, the four `history` vtable types and the variadic
//!   `hist_fun_t`.
//!
//!   The port's *own* implementations of those slots are not bound by it. Two
//!   — `prompt_default`/`prompt_default_r` — and the builtin reader and the
//!   ten `history_def_*` functions are written in the C shape directly,
//!   because they are few and their bodies are already pointer work. The 96
//!   editor commands are not: they stay ordinary Rust functions taking
//!   `&mut EditLine`, and `map::el_func!` builds the one-line `extern "C"`
//!   shim each table row needs. `plan/decisions/idiomatic-core.md` is what
//!   makes that the right side of the trade — the C ABI is a property of the
//!   boundary, not of the command set.
//! - **A C `union` → a Rust `enum`** where the discriminant lives in a
//!   neighbouring field (`keymacro_value_t`, tagged by `type`). A C *integer
//!   flag* stays an integer.
//! - **A POSIX kernel type with no Rust counterpart (`struct termios`,
//!   `struct sigaction`, `sigset_t`) → a transcription in `nshedit-plat`,
//!   the one crate in the workspace that issues a syscall
//!   (`plan/decisions/platform-layer.md`).** `struct termios` keeps a shape
//!   of its own here, `tty::Termios`, because `def:tty.el-tty-t` freezes
//!   libedit's; the other two are used as the platform crate declares them.
//!   The core still names no libc symbol.
//!
//! - **A C `(int argc, const wchar_t **argv)` pair → `&[&[u32]]`.** The slice
//!   carries the count, so the separate `argc` goes away. Used by
//!   `el_editmode`, `map_bind`, `terminal_settc`, `terminal_telltc`,
//!   `terminal_echotc`, `tty_stty` and `hist_command` — all of which the C
//!   reaches through the same `el_set` list-op path, so they share one shape.
//!   Note the C's own `map_bind` ignores its `argc` and relies on a NULL
//!   terminator, which is why `sem:map.map-bind-fn` records a read past the
//!   end when a caller passes a full argument list; the slice form cannot
//!   express that defect, and the body translation says so where it bites.
//!
//! Anything that departs from the C's own shape says so at the field.

// [spec:nshedit:req:workspace.no-legacy-allows]
// [spec:nshedit:req:core.typed-domain+1]
/// Rust-native editor values shared by the safe editor shell and its hosts.
pub mod domain;

// [spec:nshedit:req:core.raii-lifecycle]
// [spec:nshedit:req:core.rust-io+1]
// [spec:nshedit:req:core.effect-hooks]
// [spec:nshedit:req:core.line-commands]
// [spec:nshedit:req:core.terminal-render+1]
// [spec:nshedit:req:core.token-completion+1]
// [spec:nshedit:req:core.read-driver]
/// Safe native editor sessions and their borrowed I/O capabilities.
pub mod editor;

// Temporary compatibility definitions still consumed by the translated
// engine. Installed C declarations belong exclusively to `nshedit-abi`.
#[path = "../../nshedit-abi/src/compat/histedit.rs"]
pub mod histedit;

// Host facilities the port has to supply itself. None has a C counterpart:
// `plan/decisions/no-c-ffi.md` bars linking libc, so the `LC_CTYPE` queries,
// the `errno` the C makes through it, and the writes the C aims at
// `el_outfile` and `el_errfile` have nowhere else to come from. Every module
// that needs one takes it from here — two independent copies of the locale
// layer, two of `errno` and fifteen of the output writers existed before they
// were hoisted.
#[path = "../../nshedit-abi/src/compat/errno.rs"]
pub mod errno;
#[path = "../../nshedit-abi/src/compat/locale.rs"]
pub(crate) mod locale;
#[path = "../../nshedit-abi/src/compat/stdio.rs"]
pub(crate) mod stdio;

// Encoding and escaping.
#[path = "../../nshedit-abi/src/compat/chartype.rs"]
pub mod chartype;
#[path = "../../nshedit-abi/src/compat/literal.rs"]
pub mod literal;
/// `strvis(dst, src, VIS_NL)` alone, so that escaping a `history` listing for
/// display does not depend on the optional `bsd` feature. The rest of `vis(3)`
/// — every other flag word, and the whole decoder — still comes from `bsd`.
#[path = "../../nshedit-abi/src/compat/vislite.rs"]
pub(crate) mod vislite;
/// `vis(3)`/`unvis(3)`, re-exported from the `bsd` crate.
///
/// Our own 1,860-line translation used to live here, in `vis.rs` and
/// `unvis.rs`. It is gone: the sibling libbsd port has the same NetBSD source
/// with a safe API, and two translations of one C file in one workspace meant
/// two places to fix whenever either was wrong.
///
/// # These symbols carry no port rules any more
///
/// They used to. Seventy `def` and `sem` annotations sat on this one
/// re-export, because it became the only place the port provided the
/// functions after the bodies were deleted. That was defensible for the two
/// facets a re-export can satisfy and impossible for the third: a `/test`
/// facet here would claim that this tree verifies an implementation living
/// in another repository, which has its own `[spec:libbsd:...]` corpus and
/// its own tests. Thirty-four rules would have sat permanently
/// unverified, or been covered by tests that only run under an optional
/// feature — a number that says "tested" about code we do not build by
/// default.
///
/// So the rules are retired rather than parked. `docs/spec/port/src/vis.md`
/// and `unvis.md` are deleted and `src/vis.c`, `src/vis.h` and `src/unvis.c`
/// leave the `source-impl` scan, the same way `strlcpy.c` and the other libc
/// gap-fillers already had. The port's denominator counts what this tree
/// implements.
///
/// Nothing about what we ship changes. `nshedit-abi` reaches these through
/// `nshedit::vis`, and the C ABI never exported them in the first place —
/// Debian's own `libedit.so.2` *imports* `strvis@LIBBSD_0.0` rather than
/// providing it, so `conformance-abi-shape` sees no difference either way.
#[cfg(feature = "bsd")]
pub use bsd::vis;

// Terminal capability and tty control.
#[path = "../../nshedit-abi/src/compat/terminal.rs"]
pub mod terminal;
#[path = "../../nshedit-abi/src/compat/tty.rs"]
pub mod tty;

// Line buffer and screen refresh.
#[path = "../../nshedit-abi/src/compat/chared.rs"]
pub mod chared;
#[path = "../../nshedit-abi/src/compat/prompt.rs"]
pub mod prompt;
#[path = "../../nshedit-abi/src/compat/refresh.rs"]
pub mod refresh;

// Input dispatch and key binding.
#[path = "../../nshedit-abi/src/compat/keymacro.rs"]
pub mod keymacro;
#[path = "../../nshedit-abi/src/compat/map.rs"]
pub mod map;
#[path = "../../nshedit-abi/src/compat/parse.rs"]
pub mod parse;
#[path = "../../nshedit-abi/src/compat/read.rs"]
pub mod read;
#[path = "../../nshedit-abi/src/compat/sig.rs"]
pub mod sig;

// History storage and search.
#[path = "../../nshedit-abi/src/compat/hist.rs"]
pub mod hist;
pub mod histfile;
// [spec:nshedit:req:core.history+1]
pub mod history;
#[path = "../../nshedit-abi/src/compat/search.rs"]
pub mod search;

// Editor command sets, and the command table `src/makelist` generates from
// their doc comments.
#[path = "../../nshedit-abi/src/compat/common.rs"]
pub mod common;
#[path = "../../nshedit-abi/src/compat/emacs.rs"]
pub mod emacs;
#[path = "../../nshedit-abi/src/compat/fcns.rs"]
pub(crate) mod fcns;
#[path = "../../nshedit-abi/src/compat/vi.rs"]
pub mod vi;

// Completion and tokenization.
#[path = "../../nshedit-abi/src/compat/filecomplete.rs"]
pub mod filecomplete;
pub mod tokenizer;

// EditLine lifecycle.
#[path = "../../nshedit-abi/src/compat/el.rs"]
pub mod el;

// The one editor the concern tests are built on. Here rather than in any one
// of them because five modules were each constructing their own and no two
// agreed on which subsystem a headless editor still needs.
#[cfg(test)]
#[path = "../../nshedit-abi/src/compat/testkit.rs"]
mod testkit;
