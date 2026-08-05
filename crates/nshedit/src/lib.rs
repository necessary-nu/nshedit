//! Line editing, history and tokenization — a Rust re-implementation of
//! libedit, and the library nsh links.
//!
//! Modules mirror the C source file for file, so that each `sem` rule in
//! `docs/spec/port/src/` has one obvious home and the two implementations can
//! be diffed against each other. Idiomatic shape comes later, once the ported
//! behaviour is under test.
//!
//! Text is carried as `u32`, not `char`. The screen image stores sentinel
//! values that are not Unicode scalar values — see
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
//!   C's separate size/capacity fields are kept anyway, because the
//!   translations refer to them by name.
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
//!   `struct sigaction`, `sigset_t`) → a placeholder declared in the module
//!   that owns it.** `plan/decisions/no-c-ffi.md` bars linking libc, so the
//!   concrete carrier is a decision for that module's function translation.
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

// Types have no constructors yet, so almost every one of them is unused.
// Remove this once the function translations land and start building them.
#![allow(dead_code)]

// Public headers.
pub mod editline;
pub mod histedit;

// Host facilities the port has to supply itself. Neither has a C counterpart:
// `plan/decisions/no-c-ffi.md` bars linking libc, so the `LC_CTYPE` queries and
// the `errno` the C makes through it have nowhere else to come from. Every
// module that needs one takes it from here — two independent copies of the
// locale layer and two of `errno` existed before they were hoisted.
pub mod errno;
pub(crate) mod locale;

// Encoding and escaping.
pub mod chartype;
pub mod literal;
pub mod unvis;
pub mod vis;

// Terminal capability and tty control.
pub mod terminal;
pub mod tty;

// Line buffer and screen refresh.
pub mod chared;
pub mod prompt;
pub mod refresh;

// Input dispatch and key binding.
pub mod keymacro;
pub mod map;
pub mod parse;
pub mod read;
pub mod sig;

// History storage and search.
pub mod hist;
pub mod history;
pub mod search;

// Editor command sets, and the command table `src/makelist` generates from
// their doc comments.
pub mod common;
pub mod emacs;
pub(crate) mod fcns;
pub mod vi;

// Completion and tokenization.
pub mod filecomplete;
pub mod tokenizer;

// EditLine lifecycle.
pub mod el;
