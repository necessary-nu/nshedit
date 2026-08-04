//! The narrow-character entry points of `histedit.h`; rules in
//! `docs/spec/port/src/eln.md`.
//!
//! `eln.c` is a compatibility layer in the C itself — every function here
//! converts between the caller's multibyte strings and the wide interior,
//! through the `EditLine`'s legacy conversion buffer — so
//! `plan/decisions/idiomatic-core.md` places it in this crate rather than the
//! core. The pointers these functions hand back live in that buffer and stay
//! valid exactly until the next narrow call on the same editor.
//!
//! `histedit.h` declares the same nine symbols, with rules of its own. Those
//! declaration rules are carried in [`crate::histedit`], which re-exports
//! these definitions so that exactly one symbol of each name exists.

use core::ffi::{c_char, c_int};

use nshedit::histedit::{EditLine, LineInfo};

// [spec:libedit:def:eln.el-getc-fn]
// [spec:libedit:sem:eln.el-getc-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_getc(el: *mut EditLine, cp: *mut c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:eln.el-push-fn]
// [spec:libedit:sem:eln.el-push-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_push(el: *mut EditLine, str_: *const c_char) {
    todo!()
}

// [spec:libedit:def:eln.el-gets-fn]
// [spec:libedit:sem:eln.el-gets-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_gets(el: *mut EditLine, nread: *mut c_int) -> *const c_char {
    todo!()
}

// [spec:libedit:def:eln.el-parse-fn]
// [spec:libedit:sem:eln.el-parse-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_parse(
    el: *mut EditLine,
    argc: c_int,
    argv: *mut *const c_char,
) -> c_int {
    todo!()
}

/// C: `int el_set(EditLine *el, int op, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:eln.el-set-fn]
// [spec:libedit:sem:eln.el-set-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_set(el: *mut EditLine, op: c_int) -> c_int {
    todo!()
}

/// C: `int el_get(EditLine *el, int op, ...);`
///
/// Rust cannot yet *define* a C-variadic function on stable (`c_variadic`,
/// rust-lang/rust#44930), so the trailing `...` is absent from the Rust
/// signature. The exported symbol is still the C one and the fixed arguments
/// are passed identically; reading the variadic tail is left to the body.
// [spec:libedit:def:eln.el-get-fn]
// [spec:libedit:sem:eln.el-get-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_get(el: *mut EditLine, op: c_int) -> c_int {
    todo!()
}

// [spec:libedit:def:eln.el-line-fn]
// [spec:libedit:sem:eln.el-line-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_line(el: *mut EditLine) -> *const LineInfo {
    todo!()
}

// [spec:libedit:def:eln.el-insertstr-fn]
// [spec:libedit:sem:eln.el-insertstr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_insertstr(el: *mut EditLine, str_: *const c_char) -> c_int {
    todo!()
}

// [spec:libedit:def:eln.el-replacestr-fn]
// [spec:libedit:sem:eln.el-replacestr-fn]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn el_replacestr(el: *mut EditLine, str_: *const c_char) -> c_int {
    todo!()
}
