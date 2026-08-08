//! The two exported entry points of `chartype.c`; rules in
//! `docs/spec/port/src/chartype.md`.
//!
//! `ct_encode_string` and `ct_decode_string` are declared in `chartype.h`
//! without `libedit_private`, so they are exported symbols of `libedit.so`
//! even though `histedit.h` never mentions them — both `sem` rules say so in
//! as many words, and Debian's `libedit.so.2` exports both. The symbol table
//! is the contract, so they are exported here.
//!
//! Everything else is private boundary implementation in
//! [`crate::conversion`].
//!
//! # The caller's `ct_buffer_t`
//!
//! `chartype.h` is not installed (`src/Makefile.am:55` installs `histedit.h`
//! and `editline/readline.h` and nothing else), so a caller reaching these
//! two symbols declared them itself, and what it declared is the C's
//! four-word struct. [`CtBufferC`] is that struct; the owning buffers live
//! *in it*, exactly as the C's `el_realloc`ed blocks do, so a caller can read
//! `conv.cbuff` back and find the same pointer the call returned.
//!
//! The owning [`crate::conversion::ConversionBuffer`] does not have that
//! layout, so each call lifts the four words into one and lowers it back.

use core::ffi::c_char;
use core::ptr;

use crate::conversion::{ConversionBuffer, decode_bytes, encode_wide};

/// C: `typedef struct ct_buffer_t { char *cbuff; size_t csize; wchar_t
/// *wbuff; size_t wsize; } ct_buffer_t;` — `def:chartype.ct-buffer-t`, in the
/// layout a C caller declares.
///
/// The boundary carries the blocks as owning `Vec`s; this is their ABI face,
/// bridged per call rather than transmuted.
///
/// `csize` and `wsize` are the C's *allocated element counts*, not the amount
/// in use, and each is the length of the block its pointer names.
#[repr(C)]
pub struct CtBufferC {
    /// C: `char *cbuff` — the byte half, or NULL when none is allocated.
    pub cbuff: *mut c_char,
    /// C: `size_t csize` — allocated `char` count.
    pub csize: usize,
    /// C: `wchar_t *wbuff` — the wide half, or NULL when none is allocated.
    pub wbuff: *mut u32,
    /// C: `size_t wsize` — allocated `wchar_t` count.
    pub wsize: usize,
}

/// The caller's four words as the core's owning struct.
///
/// # What this requires of the caller
///
/// That the struct started all-zero and has only ever been passed to the two
/// functions below. That is the C's own contract for it — every `ct_buffer_t`
/// in libedit is either a `static` or a `calloc`ed member of `EditLine`, and
/// the header that would let a caller build one another way is not installed.
/// A struct carrying a block this library did not allocate cannot be honoured:
/// the block's allocator is unknown, so it is ignored rather than reallocated
/// or freed, which leaks it. A non-NULL pointer with a zero size is that case
/// and is the only shape a caller can produce that the C would have handled.
///
/// # Safety
///
/// `conv` must be non-NULL and point at a live, correctly aligned `CtBufferC`
/// whose two blocks came from [`lower`].
unsafe fn lift(conv: *mut CtBufferC) -> ConversionBuffer {
    // SAFETY: the caller guarantees a live struct.
    let c = unsafe { &mut *conv };
    // SAFETY: as the function's contract.
    let bytes = unsafe { take(c.cbuff.cast::<u8>(), c.csize) };
    // SAFETY: as above.
    let wide = unsafe { take(c.wbuff, c.wsize) };
    ConversionBuffer::from_parts(bytes, wide)
}

/// The core's owning struct back into the caller's four words.
///
/// The two size fields are the core's `csize`/`wsize`, which it keeps equal to
/// the buffer lengths, so the pointer and the count a caller reads out always
/// describe one block. A buffer the core emptied — its allocation-failure
/// path, which sets the size to 0 and drops the `Vec` — lowers to NULL and 0,
/// which is the C's "freed and NULLed" state exactly.
///
/// # Safety
///
/// `conv` must be non-NULL and point at a live, correctly aligned
/// `CtBufferC`, whose current blocks `buf` already owns.
unsafe fn lower(conv: *mut CtBufferC, buffer: ConversionBuffer) {
    let (bytes, wide) = buffer.into_parts();
    let (cp, cn) = give(bytes);
    let (wp, wn) = give(wide);
    // SAFETY: the caller guarantees a live struct.
    let c = unsafe { &mut *conv };
    c.cbuff = cp.cast::<c_char>();
    c.csize = cn;
    c.wbuff = wp;
    c.wsize = wn;
}

/// A block this crate handed out through [`give`], back as the `Vec` that
/// owns it.
///
/// `Vec::from(Box<[T]>)` neither reallocates nor copies, and the box's
/// capacity is exactly its length, so the round trip through [`give`] and
/// back preserves the pointer as long as nothing grows.
///
/// # Safety
///
/// `p` must be NULL, or a block from [`give`] of exactly `n` elements that is
/// not taken twice.
unsafe fn take<T>(p: *mut T, n: usize) -> Vec<T> {
    if p.is_null() || n == 0 {
        return Vec::new();
    }
    // SAFETY: the caller guarantees the block and its length.
    Vec::from(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(p, n)) })
}

/// The inverse of [`take`]: a `Vec` as a raw block plus its element count.
///
/// `into_boxed_slice` shrinks the capacity to the length, which is what makes
/// the pointer round-trippable — [`take`] has no third word to record a
/// capacity in, so the block's length has to *be* its capacity. It is not a
/// second allocation in the ordinary case: the core grows these buffers by
/// `try_reserve` followed by `resize` to the same figure, and `Vec`'s
/// amortized growth lands on exactly that figure from a capacity-equals-length
/// start, so there is nothing to shrink.
///
/// An empty `Vec` gives NULL rather than the dangling pointer `Box` would
/// yield for a zero-length slice, because the C's counterpart of that state is
/// a NULL `cbuff` and a caller may test it.
fn give<T>(v: Vec<T>) -> (*mut T, usize) {
    if v.is_empty() {
        return (ptr::null_mut(), 0);
    }
    let b = v.into_boxed_slice();
    let n = b.len();
    (Box::into_raw(b).cast::<T>(), n)
}

/// The C's `const wchar_t *` as a slice, up to but not including the
/// terminating `L'\0'`.
///
/// # Safety
///
/// `p` must be non-NULL and point at a `L'\0'`-terminated wide string that
/// outlives the slice.
unsafe fn wide_upto_nul<'a>(p: *const u32) -> &'a [u32] {
    let mut n = 0usize;
    // SAFETY: the caller guarantees a terminated string.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: as above.
    unsafe { core::slice::from_raw_parts(p, n) }
}

// [spec:libedit:def:chartype.ct-encode-string-fn]
// [spec:libedit:sem:chartype.ct-encode-string-fn]
/// C: `char *ct_encode_string(const wchar_t *s, ct_buffer_t *conv);`
///
/// Returns `conv->cbuff` — the buffer itself, not a copy, mutable, and valid
/// until the next `ct_encode_string` on the same `conv`. `ct_decode_string`
/// does not invalidate it: it writes only the wide half, and the lift/lower
/// pair keeps the two halves independent exactly as the C's two `realloc`
/// blocks are.
///
/// NULL on a NULL `s` — with `conv` untouched, so a pointer handed out
/// earlier survives — and on a buffer that could not be grown, where the core
/// has already emptied the byte half and this leaves `cbuff` NULL and `csize`
/// 0, which is the C's own post-failure state.
///
/// A NULL `conv` is undefined behaviour in the C, which dereferences it
/// immediately; here it is NULL, treated as the caller error it is.
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn ct_encode_string(s: *const u32, conv: *mut CtBufferC) -> *mut c_char {
    if conv.is_null() {
        return ptr::null_mut();
    }
    // Step 1: `conv` is not touched, so nothing is lifted.
    if s.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `conv` is non-NULL and, by this module's contract, carries only
    // blocks `lower` put there.
    let mut buf = unsafe { lift(conv) };
    // SAFETY: `s` is a terminated wide string.
    let ok = encode_wide(Some(unsafe { wide_upto_nul(s) }), &mut buf).is_some();
    // SAFETY: `buf` owns exactly the blocks `lift` took.
    unsafe { lower(conv, buf) };
    if ok {
        // The core writes the terminator one past the end of the slice it
        // returns, so the base pointer is the NUL-terminated C string.
        // SAFETY: `conv` is non-NULL.
        unsafe { (*conv).cbuff }
    } else {
        ptr::null_mut()
    }
}

// [spec:libedit:def:chartype.ct-decode-string-fn]
// [spec:libedit:sem:chartype.ct-decode-string-fn]
/// C: `wchar_t *ct_decode_string(const char *s, ct_buffer_t *conv);`
///
/// Returns `conv->wbuff`, with the same lifetime rule in the other direction:
/// invalidated by the next decode on the same `conv`, not by an encode.
///
/// NULL for a NULL `s`, for a `s` holding a sequence the current `LC_CTYPE`
/// rejects, and on allocation failure — three causes a C caller cannot tell
/// apart either. Only the third disturbs `conv`.
///
/// A NULL `conv` is NULL, as in [`ct_encode_string`].
#[unsafe(no_mangle)]
#[doc = include_str!("ffi_safety.md")]
pub unsafe extern "C" fn ct_decode_string(s: *const c_char, conv: *mut CtBufferC) -> *mut u32 {
    if conv.is_null() {
        return ptr::null_mut();
    }
    // Step 1, as above.
    if s.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: `s` is a NUL-terminated string.
    let bytes = unsafe { core::ffi::CStr::from_ptr(s) }.to_bytes();
    // SAFETY: as `ct_encode_string`.
    let mut buf = unsafe { lift(conv) };
    let ok = decode_bytes(Some(bytes), &mut buf).is_some();
    // SAFETY: as `ct_encode_string`.
    unsafe { lower(conv, buf) };
    if ok {
        // SAFETY: `conv` is non-NULL.
        unsafe { (*conv).wbuff }
    } else {
        ptr::null_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::{CtBufferC, ct_decode_string, ct_encode_string};
    use core::ptr;

    fn blank() -> CtBufferC {
        CtBufferC {
            cbuff: ptr::null_mut(),
            csize: 0,
            wbuff: ptr::null_mut(),
            wsize: 0,
        }
    }

    /// Frees whatever the two entry points left in the struct, the way this
    /// module's own lowering would take it back.
    fn release(conv: &mut CtBufferC) {
        // SAFETY: the blocks came from `give`, with these exact lengths.
        unsafe {
            let _ = super::take(conv.cbuff.cast::<u8>(), conv.csize);
            let _ = super::take(conv.wbuff, conv.wsize);
        }
        *conv = blank();
    }

    /// A virgin struct grows on first use, and the returned pointer *is*
    /// `conv.cbuff`, which is the contract both `sem` rules state.
    #[test]
    fn the_result_is_the_buffer_itself() {
        let mut conv = blank();
        let w = [b'h' as u32, b'i' as u32, 0];
        // SAFETY: both arguments are live.
        let p = unsafe { ct_encode_string(w.as_ptr(), &raw mut conv) };
        assert!(!p.is_null());
        assert_eq!(p, conv.cbuff);
        assert_eq!(conv.csize, 1024);
        // SAFETY: `p` is a NUL-terminated string this call just wrote.
        assert_eq!(unsafe { core::ffi::CStr::from_ptr(p) }.to_bytes(), b"hi");
        release(&mut conv);
    }

    /// The two halves are independent: a decode does not disturb a pointer an
    /// encode handed out, which `sem:chartype.ct-encode-string-fn` requires
    /// and `terminal.c` leans on.
    #[test]
    fn the_two_halves_do_not_disturb_each_other() {
        let mut conv = blank();
        let w = [b'a' as u32, 0];
        // SAFETY: live arguments throughout.
        unsafe {
            let enc = ct_encode_string(w.as_ptr(), &raw mut conv);
            let dec = ct_decode_string(c"abc".as_ptr(), &raw mut conv);
            assert!(!dec.is_null());
            assert_eq!(enc, conv.cbuff);
            assert_eq!(*enc, b'a' as core::ffi::c_char);
            assert_eq!(*dec.add(3), 0);
        }
        release(&mut conv);
    }

    /// A NULL string leaves `conv` alone; a NULL `conv` is the caller error
    /// the C faults on, defined here as NULL.
    #[test]
    fn null_arguments() {
        let mut conv = blank();
        // SAFETY: `conv` is live; the string pointers are deliberately NULL.
        unsafe {
            assert!(ct_encode_string(ptr::null(), &raw mut conv).is_null());
            assert!(ct_decode_string(ptr::null(), &raw mut conv).is_null());
            assert!(conv.cbuff.is_null() && conv.csize == 0);
            assert!(ct_encode_string([0u32].as_ptr(), ptr::null_mut()).is_null());
            assert!(ct_decode_string(c"".as_ptr(), ptr::null_mut()).is_null());
        }
    }

    /// Reuse across calls: growth is monotonic and the struct stays usable,
    /// which is what makes a caller's `static ct_buffer_t` work.
    #[test]
    fn the_buffer_is_reused_and_grows_monotonically() {
        let mut conv = blank();
        let mut long: Vec<u32> = vec![b'x' as u32; 4000];
        long.push(0);
        // SAFETY: live arguments.
        unsafe {
            let short = [b'q' as u32, 0];
            assert!(!ct_encode_string(short.as_ptr(), &raw mut conv).is_null());
            let first = conv.csize;
            assert!(!ct_encode_string(long.as_ptr(), &raw mut conv).is_null());
            assert!(conv.csize > first);
            let grown = conv.csize;
            let p = ct_encode_string(short.as_ptr(), &raw mut conv);
            assert_eq!(conv.csize, grown, "the buffer is never shrunk");
            assert_eq!(core::ffi::CStr::from_ptr(p).to_bytes(), b"q");
        }
        release(&mut conv);
    }
}
