//! The `histedit.h` public types; rules live in
//! `docs/spec/port/src/histedit.md`.
//!
//! The C header is the ABI surface, so the structs it defines are frozen in
//! layout and are marked `#[repr(C)]` (`plan/decisions/no-c-ffi.md`). Their
//! character pointers stay raw pointers for the same reason: they are
//! documented as borrowed views into libedit's own storage, invalidated by
//! the next operation, and a C caller reads them as `const char *` /
//! `const wchar_t *`.
//!
//! The header also forward-declares five handles as incomplete types. Rust
//! has no incomplete types, so each is named here and resolved to the module
//! that defines its body in C: `EditLine`, `HistoryW` and `TokenizerW` are
//! re-exports, while the two narrow handles — `History` and `Tokenizer`,
//! which the C produces by recompiling `history.c` and `tokenizer.c` with
//! `Char = char` — are left as opaque placeholders, because those narrow
//! instantiations are not separate symbols in the port manifest.

use core::ffi::c_char;

// [spec:libedit:def:histedit.edit-line]
/// C: `typedef struct editline EditLine;` — the editor handle. Its body is
/// `def:el.editline`, in [`crate::el`].
pub use crate::el::EditLine;
// [spec:libedit:def:histedit.history-w]
/// C: `typedef struct historyW HistoryW;` — the wide history handle. Its
/// body is `history.c`'s `struct TYPE(history)`, which the C defines without
/// a rule of its own; see [`crate::history::HistoryW`].
pub use crate::history::HistoryW;
// [spec:libedit:def:histedit.tokenizer-w]
/// C: `typedef struct tokenizerW TokenizerW;` — the wide tokenizer handle.
/// Its body is `tokenizer.c`'s `struct TYPE(tokenizer)`, which the C defines
/// without a rule of its own; see [`crate::tokenizer::TokenizerW`].
pub use crate::tokenizer::TokenizerW;

// [spec:libedit:def:histedit.lineinfo]
// [spec:libedit:def:histedit.line-info]
/// The narrow user-function line view. The C carries both rules on the one
/// declaration (`typedef struct lineinfo { ... } LineInfo;`), so both sit
/// here.
///
/// Embedded in `EditLine` as `el_lgcylinfo` and returned by `el_line`. The
/// three pointers are into `el_lgcyconv` and are invalidated by the next
/// narrow call; see `sem:eln.el-line-fn`.
#[repr(C)]
pub struct LineInfo {
    pub buffer: *const c_char,
    pub cursor: *const c_char,
    pub lastchar: *const c_char,
}

// [spec:libedit:def:histedit.history]
/// C: `typedef struct history History;` — the opaque narrow history handle.
///
/// `struct history` is `history.c` compiled with `Char = char`
/// (`historyn.c`). That narrow instantiation is not a separate symbol in the
/// port manifest, so its members are left to the history translation; this
/// declaration exists so the header's incomplete type has a name.
pub struct History {
    _opaque: (),
}

// [spec:libedit:def:histedit.hist-event]
/// A narrow history event.
///
/// `str` is borrowed from the history entry that produced it and is
/// invalidated when that entry is deleted or replaced; see
/// `sem:histedit.history-fn`.
#[repr(C)]
pub struct HistEvent {
    pub num: i32,
    pub str: *const c_char,
}

// [spec:libedit:def:histedit.tokenizer]
/// C: `typedef struct tokenizer Tokenizer;` — the opaque narrow tokenizer
/// handle.
///
/// `struct tokenizer` is `tokenizer.c` compiled with `Char = char`
/// (`tokenizern.c`), which the port manifest does not carry separately. The
/// wide body is [`crate::tokenizer::TokenizerW`]; the narrow one is left to
/// the tokenizer translation.
pub struct Tokenizer {
    _opaque: (),
}

// [spec:libedit:def:histedit.lineinfow]
// [spec:libedit:def:histedit.line-info-w]
/// The wide user-function line view — both rules sit on the one C
/// declaration.
///
/// `el_wline` returns a live alias of `el->el_line`, not a snapshot: the
/// three pointers change as the user edits, `lastchar` is one past the last
/// character with no NUL there, and all three are invalidated when the line
/// buffer grows. `sem:el.el-wline-fn` requires the port to
/// produce this as a genuine borrowed view rather than a transmute, which is
/// why `def:el.el-line-t`'s field order is frozen.
#[repr(C)]
pub struct LineInfoW {
    pub buffer: *const u32,
    pub cursor: *const u32,
    pub lastchar: *const u32,
}

// [spec:libedit:def:histedit.el-rfunc-t-edit-line-wchar-t]
/// C: `typedef int (*el_rfunc_t)(EditLine *, wchar_t *);`
///
/// The character-reading hook installed by `EL_GETCFN`. The second argument
/// is a one-element out parameter, so it is `&mut u32` rather than a
/// pointer. Returns 1 for a character read, 0 for end of input, -1 for an
/// error.
pub type ElRfuncT = fn(&mut EditLine, &mut u32) -> i32;

// [spec:libedit:def:histedit.histevent-w]
// [spec:libedit:def:histedit.hist-event-w]
/// A wide history event — both rules sit on the one C declaration.
///
/// Embedded in `el_history_t` as the event cookie, and filled in by every
/// `history_w` operation. `str` is borrowed from the history entry that owns
/// it and is invalidated when that entry is deleted or replaced.
#[repr(C)]
pub struct HistEventW {
    pub num: i32,
    pub str: *const u32,
}
