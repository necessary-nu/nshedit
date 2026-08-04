//! Ported from `src/vis.c`; rules live in `docs/spec/port/src/vis.md`.

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
