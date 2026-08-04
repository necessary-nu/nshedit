//! Ported from `src/vis.c`; rules live in `docs/spec/port/src/vis.md`.

// The function bodies are still `todo!()`, so every parameter reads as
// unused. Remove this once the translations land.
#![allow(unused_variables)]

use core::ffi::c_char;

// [spec:libedit:def:vis.visfun-t-wchar-t-wint-t-int-wint-t-const-wchar-t]
/// C: `typedef wchar_t *(*visfun_t)(wchar_t *, wint_t, int, wint_t, const wchar_t *);`
///
/// The encoder `getvisfun` selects from the flags: destination cursor, the
/// character to encode, the flags, the next character (for lookahead), and
/// the "extra" set of characters to escape. It returns the advanced
/// destination cursor.
///
/// The pointers stay raw because the C's contract is raw pointer
/// arithmetic on a caller-supplied buffer with no length: the `vis` entry
/// points hand out an interior cursor and every encoder advances it. The
/// `vis.c` translation may narrow this to slices as long as the rule stays
/// annotated at whatever replaces it.
pub type VisfunT = fn(*mut u32, u32, i32, u32, *const u32) -> *mut u32;

// [spec:libedit:def:vis.iscgraph-fn]
// [spec:libedit:sem:vis.iscgraph-fn]
/// The `#ifdef notyet` fallback: `isgraph` under the C locale. Reached only
/// where the build has neither `LC_C_LOCALE` nor the macro form.
fn iscgraph(c: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.do-hvis-fn]
// [spec:libedit:sem:vis.do-hvis-fn]
/// Shaped by `VisfunT`: `dst` is a cursor into a caller-supplied buffer with
/// no length, advanced and returned.
fn do_hvis(dst: *mut u32, c: u32, flags: i32, nextc: u32, extra: *const u32) -> *mut u32 {
    todo!()
}

// [spec:libedit:def:vis.do-mvis-fn]
// [spec:libedit:sem:vis.do-mvis-fn]
fn do_mvis(dst: *mut u32, c: u32, flags: i32, nextc: u32, extra: *const u32) -> *mut u32 {
    todo!()
}

// [spec:libedit:def:vis.do-mbyte-fn]
// [spec:libedit:sem:vis.do-mbyte-fn]
fn do_mbyte(dst: *mut u32, c: u32, flags: i32, nextc: u32, iswextra: i32) -> *mut u32 {
    todo!()
}

// [spec:libedit:def:vis.do-svis-fn]
// [spec:libedit:sem:vis.do-svis-fn]
fn do_svis(dst: *mut u32, c: u32, flags: i32, nextc: u32, extra: *const u32) -> *mut u32 {
    todo!()
}

// [spec:libedit:def:vis.getvisfun-fn]
// [spec:libedit:sem:vis.getvisfun-fn]
fn getvisfun(flags: i32) -> VisfunT {
    todo!()
}

// [spec:libedit:def:vis.makeextralist-fn]
// [spec:libedit:sem:vis.makeextralist-fn]
/// The C returns a `calloc`ed wide string the caller frees, so this owns it;
/// `None` is its NULL return.
fn makeextralist(flags: i32, src: *const c_char) -> Option<Vec<u32>> {
    todo!()
}

// [spec:libedit:def:vis.istrsenvisx-fn]
// [spec:libedit:sem:vis.istrsenvisx-fn]
/// `mbdstp` is the C's `char **`: an in/out cursor the function may replace
/// with a buffer it allocates. `dlen` and `cerr_ptr` are its nullable in/out
/// parameters. Returns the C's `int`: bytes written, or -1 with `errno` set.
fn istrsenvisx(
    mbdstp: &mut *mut c_char,
    dlen: Option<&mut usize>,
    mbsrc: *const c_char,
    mblength: usize,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.istrsenvisxl-fn]
// [spec:libedit:sem:vis.istrsenvisxl-fn]
fn istrsenvisxl(
    mbdstp: &mut *mut c_char,
    dlen: Option<&mut usize>,
    mbsrc: *const c_char,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.svis-fn]
// [spec:libedit:sem:vis.svis-fn]
/// Returns the advanced cursor, or NULL on failure — a caller-supplied
/// buffer with no length, so the pointers stay raw.
pub fn svis(
    mbdst: *mut c_char,
    c: i32,
    flags: i32,
    nextc: i32,
    mbextra: *const c_char,
) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:vis.snvis-fn]
// [spec:libedit:sem:vis.snvis-fn]
pub fn snvis(
    mbdst: *mut c_char,
    dlen: usize,
    c: i32,
    flags: i32,
    nextc: i32,
    mbextra: *const c_char,
) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:vis.strsvis-fn]
// [spec:libedit:sem:vis.strsvis-fn]
pub fn strsvis(
    mbdst: *mut c_char,
    mbsrc: *const c_char,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strsnvis-fn]
// [spec:libedit:sem:vis.strsnvis-fn]
pub fn strsnvis(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strsvisx-fn]
// [spec:libedit:sem:vis.strsvisx-fn]
pub fn strsvisx(
    mbdst: *mut c_char,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strsnvisx-fn]
// [spec:libedit:sem:vis.strsnvisx-fn]
pub fn strsnvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    mbextra: *const c_char,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strsenvisx-fn]
// [spec:libedit:sem:vis.strsenvisx-fn]
pub fn strsenvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    mbextra: *const c_char,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.vis-fn]
// [spec:libedit:sem:vis.vis-fn]
pub fn vis(mbdst: *mut c_char, c: i32, flags: i32, nextc: i32) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:vis.nvis-fn]
// [spec:libedit:sem:vis.nvis-fn]
pub fn nvis(mbdst: *mut c_char, dlen: usize, c: i32, flags: i32, nextc: i32) -> *mut c_char {
    todo!()
}

// [spec:libedit:def:vis.strvis-fn]
// [spec:libedit:sem:vis.strvis-fn]
pub fn strvis(mbdst: *mut c_char, mbsrc: *const c_char, flags: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strnvis-fn]
// [spec:libedit:sem:vis.strnvis-fn]
pub fn strnvis(mbdst: *mut c_char, dlen: usize, mbsrc: *const c_char, flags: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.stravis-fn]
// [spec:libedit:sem:vis.stravis-fn]
/// `mbdstp` is the C's `char **` out-parameter: the function allocates the
/// destination and stores it there.
pub fn stravis(mbdstp: &mut *mut c_char, mbsrc: *const c_char, flags: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strvisx-fn]
// [spec:libedit:sem:vis.strvisx-fn]
pub fn strvisx(mbdst: *mut c_char, mbsrc: *const c_char, len: usize, flags: i32) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strnvisx-fn]
// [spec:libedit:sem:vis.strnvisx-fn]
pub fn strnvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
) -> i32 {
    todo!()
}

// [spec:libedit:def:vis.strenvisx-fn]
// [spec:libedit:sem:vis.strenvisx-fn]
pub fn strenvisx(
    mbdst: *mut c_char,
    dlen: usize,
    mbsrc: *const c_char,
    len: usize,
    flags: i32,
    cerr_ptr: Option<&mut i32>,
) -> i32 {
    todo!()
}

// The five `vis.h` prototypes below are declarations only: the decoder is
// `src/unvis.c`, ported in `crate::unvis`, and a header prototype has no
// separate Rust definition. Re-exporting keeps `vis::` the name the header
// publishes without a second implementation of each function.

// [spec:libedit:def:vis.strunvis-fn]
// [spec:libedit:sem:vis.strunvis-fn]
pub use crate::unvis::strunvis;

// [spec:libedit:def:vis.strnunvis-fn]
// [spec:libedit:sem:vis.strnunvis-fn]
pub use crate::unvis::strnunvis;

// [spec:libedit:def:vis.strunvisx-fn]
// [spec:libedit:sem:vis.strunvisx-fn]
pub use crate::unvis::strunvisx;

// [spec:libedit:def:vis.strnunvisx-fn]
// [spec:libedit:sem:vis.strnunvisx-fn]
pub use crate::unvis::strnunvisx;

// [spec:libedit:def:vis.unvis-fn]
// [spec:libedit:sem:vis.unvis-fn]
pub use crate::unvis::unvis;
