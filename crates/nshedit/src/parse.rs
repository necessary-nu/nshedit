//! Ported from `src/parse.c`; rules live in `docs/spec/port/src/parse.md`.

// The signatures land before the bodies, so every parameter is unused until
// its `todo!()` is replaced. Remove this with the last one.
#![allow(unused_variables)]

use crate::el::EditLine;

// [spec:libedit:def:parse.func-fn]
// [spec:libedit:sem:parse.func-fn]
/// C: `int (*func)(EditLine *, int, const wchar_t **)` — the handler member
/// of the file-static `cmds[]` dispatch table, which is the whole editrc
/// command vocabulary.
///
/// The C declares it inside an anonymous struct, so there is nothing to name
/// but the pointer type; the table itself is data and belongs with
/// [`el_wparse`], its only reader. Every handler returns 0 on success and -1
/// on failure, and receives `el_wparse`'s own `argc` and `argv` unchanged.
pub(crate) type ParseFuncT = fn(&mut EditLine, i32, &[&[u32]]) -> i32;

// [spec:libedit:def:parse.parse-line-fn]
// [spec:libedit:sem:parse.parse-line-fn]
/// Tokenize one editrc line and dispatch it through [`el_wparse`].
pub(crate) fn parse_line(el: &mut EditLine, line: &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:parse.el-wparse-fn]
// [spec:libedit:sem:parse.el-wparse-fn]
/// Command dispatcher: match `argv[0]` (after any `prog:` qualifier)
/// against the command table and run the handler, negating its result. The
/// C's NULL terminator on `argv` is the slice length here.
pub fn el_wparse(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    todo!()
}

// [spec:libedit:def:parse.parse-escape-fn]
// [spec:libedit:sem:parse.parse-escape-fn]
/// Decode one `^<char>`, `\<odigit>`, `\<char>` or `\U+xxxx` escape and
/// return its value, or -1 if the escape is malformed. `ptr` is the C's
/// `const wchar_t **`: a cursor the call advances past what it consumed.
#[allow(non_snake_case)]
pub(crate) fn parse__escape(ptr: &mut &[u32]) -> i32 {
    todo!()
}

// [spec:libedit:def:parse.parse-string-fn]
// [spec:libedit:sem:parse.parse-string-fn]
/// Decode a whole key-binding string from `in` into `out`, returning the
/// written prefix of `out`, or `None` if any escape was malformed. There is
/// no output bound in the C either; `out` must hold `in.len() + 1`.
#[allow(non_snake_case)]
pub(crate) fn parse__string<'a>(out: &'a mut [u32], r#in: &[u32]) -> Option<&'a [u32]> {
    todo!()
}

// [spec:libedit:def:parse.parse-cmd-fn]
// [spec:libedit:sem:parse.parse-cmd-fn]
/// Return the command number for a command name, or -1 if there is none.
pub(crate) fn parse_cmd(el: &mut EditLine, cmd: &[u32]) -> i32 {
    todo!()
}
