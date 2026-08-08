//! `histedit.h` declarations with no natural Rust site, or whose Rust
//! spelling is a different C type from the one the header must declare.
//!
//! Every alias here is the *same Rust type* as the core item it restates.
//! The `_identical_to_the_core` block at the end is what makes that a
//! compiler-checked claim rather than a comment.

use core::ffi::{c_int, c_void};

// `EditLine` is imported rather than written out in full because cbindgen
// prints the token it reads: the name has to be the C's.
use nshedit::el::EditLine;
use nshedit::histedit::{HistEventGen, LineInfoGen};

/// C: `#define LIBEDIT_MAJOR 2`.
pub const LIBEDIT_MAJOR: c_int = 2;
/// C: `#define LIBEDIT_MINOR 11`.
pub const LIBEDIT_MINOR: c_int = 11;

/// C: `wchar_t`.
///
/// The core spells the wide character `u32` and cbindgen renders that
/// `uint32_t`, which is a **different C type**: `wchar_t` is `int` on Linux
/// and signed on every other target this library builds for, so
/// `uint32_t *` and `wchar_t *` are incompatible pointer types and a
/// consumer passing one where the other is declared gets a diagnostic. The
/// header must say `wchar_t`.
///
/// `u32` is the right Rust representation and stays; this alias only gives
/// the generator a name to print. The generator excludes it from the output,
/// because `<wchar.h>` already declares the real one.
pub type WcharT = u32;

/// C: `FILE *` — a stream the application owns.
///
/// Identical to [`nshedit::el::CFile`], which is `*mut c_void` and would be
/// rendered `void *`. `FILE *` and `void *` are not the same type to a C
/// compiler, so the generator renames this one; the alias is what there is
/// to rename. See [`crate::cstdio`] for why the stream is used as a stream.
pub type CFile = *mut c_void;

/// C: `typedef struct lineinfow { ... } LineInfoW;` — `def:histedit.lineinfow`.
///
/// The same Rust type as [`nshedit::histedit::LineInfoW`], restated over
/// [`WcharT`] so the three members render `const wchar_t *` rather than
/// `const uint32_t *`.
///
/// Named `LineInfoWide` and renamed to `LineInfoW` by the generator's config,
/// for one mechanical reason: the core's alias has to be excluded from the
/// output and cbindgen excludes by name, so two items called `LineInfoW`
/// would both go. Nothing else turns on the spelling.
pub type LineInfoWide = LineInfoGen<WcharT>;

/// C: `typedef struct histeventW { ... } HistEventW;` — `def:histedit.hist-event-w`.
///
/// As [`LineInfoWide`]: the same Rust type as
/// [`nshedit::histedit::HistEventW`], restated so `str` renders
/// `const wchar_t *`, and renamed to `HistEventW` for the same reason.
pub type HistEventWide = HistEventGen<WcharT>;

/// C: `typedef int (*el_rfunc_t)(EditLine *, wchar_t *);` —
/// `def:histedit.el-rfunc-t-edit-line-wchar-t`.
///
/// The character-reading hook `EL_GETCFN` installs. Nothing in the header's
/// own signatures mentions it — it reaches a consumer only through the
/// varargs of `el_set`/`el_wset` — so the generator is told to emit it
/// explicitly; it would otherwise be dropped as unreachable.
///
/// `EditLine` here is the core's real editor type, so this is the same Rust
/// type as [`nshedit::histedit::ElRfuncT`]. cbindgen maps this alias to
/// `el_rfunc_t`, while `EditLine` resolves in the generated header to
/// [`super::handles::EditLine`], the incomplete type the C declares.
pub type ElReadCallback = unsafe extern "C" fn(*mut EditLine, *mut WcharT) -> c_int;

/// Each of these compiles only if the alias above and the core's item are
/// the *same* type — not merely the same shape. Deleting one of these
/// functions would let this module start to drift from what the library
/// actually exports, which is the failure this whole approach exists to make
/// impossible.
const _: () = {
    fn line_info_w(x: LineInfoWide) -> nshedit::histedit::LineInfoW {
        x
    }
    fn hist_event_w(x: HistEventWide) -> nshedit::histedit::HistEventW {
        x
    }
    fn rfunc(x: ElReadCallback) -> nshedit::histedit::ElRfuncT {
        x
    }
    fn cfile(x: CFile) -> nshedit::el::CFile {
        x
    }
    let _ = (line_info_w, hist_event_w, rfunc, cfile);
};
