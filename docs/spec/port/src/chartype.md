# src/chartype.c, src/chartype.h

> [spec:libedit:def:chartype.ct-buffer-t]
> typedef struct ct_buffer_t

> [spec:libedit:def:chartype.ct-chr-class-fn]
> libedit_private int ct_chr_class(wchar_t c)

> [spec:libedit:sem:chartype.ct-chr-class-fn]
> Total classification of one wide character into exactly one of the five
> `CHTYPE_*` constants. The tests are applied in this order, and the
> order is load-bearing because the tests overlap:
>
> 1. `c == L'\t'` → `CHTYPE_TAB` (-2).
> 2. `c == L'\n'` → `CHTYPE_NL` (-3).
> 3. `c < 0x100 && iswcntrl(c)` → `CHTYPE_ASCIICTL` (-1). Tab and newline
>    also satisfy this test, which is why they are peeled off first.
> 4. `iswprint(c)` → `CHTYPE_PRINT` (0).
> 5. otherwise → `CHTYPE_NONPRINT` (-4).
>
> The `c < 0x100` guard confines the control class to the first 256 code
> points. A control character above U+00FF (U+2028, U+200B-style format
> characters, anything else `iswcntrl` accepts) is *not* `CHTYPE_ASCIICTL`
> — it falls through to `iswprint` and lands in `CHTYPE_PRINT` or
> `CHTYPE_NONPRINT`.
>
> Locale dependence. Steps 3 and 4 are `LC_CTYPE` queries and nothing
> else; the function hard-codes no Unicode property table. In the
> C/POSIX locale `iswprint` is true only for U+0020..U+007E, so every
> character above U+007E classifies as `CHTYPE_NONPRINT` and the display
> layer renders it `\U+nnnn`. In a UTF-8 locale most of Unicode is
> printable. In a glibc UTF-8 locale `iswcntrl` is also true across
> U+0080..U+009F (the C1 controls), and those are below 0x100, so they
> classify as `CHTYPE_ASCIICTL` and get the caret rendering described in
> rule `[spec:libedit:sem:chartype.ct-visual-char-fn]` — which for that
> range produces a caret followed by a Latin-1 accented letter rather
> than anything caret-escape-like. The port must query the active locale
> rather than substitute Rust's `char::is_control` /
> `char::is_alphanumeric`-style predicates, because the classification
> is observable through the rendered line.
>
> Signedness and UB. `wchar_t` is signed on many platforms. A negative
> `c` satisfies `c < 0x100` and is then passed to `iswcntrl`, whose
> argument must be representable as `wint_t` or be exactly `WEOF`; a
> negative value that is not `WEOF` is undefined behaviour. This is
> reachable: `MB_FILL_CHAR` is `(wint_t)-1`, refresh.c stores it in the
> screen-image cells and then passes those cells to this function.
> glibc's `iswcntrl` accepts `WEOF` and answers false, after which
> `iswprint(WEOF)` is also false and the result is `CHTYPE_NONPRINT`, but
> that is glibc behaviour, not a standard guarantee.
>
> `wchar_t` assumptions, and why Rust's `char` is not a substitute.
> chartype.h raises `#error wchar_t must store ISO 10646 characters`
> unless `__STDC_ISO_10646__` is defined, with an exemption list of
> NetBSD, Solaris, Tru64, macOS, OpenBSD, FreeBSD and DragonFly (which
> are simply trusted). So the whole module assumes `wchar_t` values *are*
> Unicode code points, and at bare minimum that the first 127 are ASCII.
> chartype.h additionally raises `#warning Build environment does not
> support non-BMP characters` when `WCHAR_MAX < INT32_MAX`; on such a
> platform a `wchar_t` is a UTF-16 code unit, lone surrogates included,
> and the `c > 0xffff` branches elsewhere in this file are dead. Neither
> guard makes a `wchar_t` a Unicode *scalar value*: surrogates,
> `(wint_t)-1`, and values above U+10FFFF all reach these functions. Rust
> `char` forbids all three, so the port must carry these values as `u32`
> (or the platform `wchar_t`) on the ABI boundary and at every point that
> touches the screen image, converting to `char` only where the value has
> been proven to be a scalar value.

> [spec:libedit:def:chartype.ct-conv-cbuff-resize-fn]
> static int ct_conv_cbuff_resize(ct_buffer_t *conv, size_t csize)

> [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]
> Grows the byte half of a conversion buffer — the `cbuff`/`csize` pair
> of `ct_buffer_t` — to hold at least `csize` `char`s. `csize` is an
> element count, and since the element is `char` it is also a byte count.
> Returns 0 on success, -1 on allocation failure.
>
> Step 1: if `csize <= conv->csize`, return 0 and change nothing. The
> buffer is grow-only; it is never shrunk and never released here.
> Step 2: commit the new size into `conv->csize` *before* attempting the
> allocation.
> Step 3: `realloc(conv->cbuff, conv->csize * sizeof(char))`. A zeroed
> `ct_buffer_t` has `cbuff == NULL` and `csize == 0`, so the first call
> takes the realloc-from-NULL path, which is a plain allocation. Existing
> contents up to the old size are preserved; the newly added tail is
> uninitialised.
> Step 4 (success): store the new pointer in `conv->cbuff` and return 0.
> Step 5 (failure, realloc returned NULL): set `conv->csize = 0`, free
> the *old* `conv->cbuff` (realloc left it valid), set
> `conv->cbuff = NULL`, and return -1.
>
> The failure path is the part that matters for the port: an allocation
> failure does not leave the caller with the previous buffer intact, it
> destroys it. Every byte previously held there is gone and every pointer
> ever handed out into it is dangling from that moment. The struct is
> left in the same all-zero state it started in, so it is still usable
> for a later attempt.
>
> Ownership. The `ct_buffer_t` belongs to whoever declared it, not to
> this module. libedit has three per `EditLine` (`el_visual`,
> `el_scratch`, `el_lgcyconv`, all zeroed by the `calloc` in
> `el_init_internal` and all six pointers freed in `el_end`) plus five
> function-scope `static` ones in search.c, history.c and readline.c that
> are never freed at all and leak by design for the process lifetime. A
> zeroed struct is the valid "empty" state; this function requires no
> other initialisation.
>
> The only caller is `ct_encode_string`, which always asks for
> `conv->csize + CT_BUFSIZ` where `CT_BUFSIZ` is 1024. Growth is
> therefore linear in 1024-byte steps, not geometric.

> [spec:libedit:def:chartype.ct-conv-wbuff-resize-fn]
> static int ct_conv_wbuff_resize(ct_buffer_t *conv, size_t wsize)

> [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn]
> The wide-character twin of rule
> `[spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]`: grows the
> `wbuff`/`wsize` pair of a `ct_buffer_t` to hold at least `wsize`
> `wchar_t`s. `wsize` is an element count, so the allocation is
> `wsize * sizeof(wchar_t)` bytes. Returns 0 on success, -1 on failure.
>
> Step 1: if `wsize <= conv->wsize`, return 0 and change nothing.
> Grow-only, never shrinks.
> Step 2: commit `conv->wsize = wsize` before allocating.
> Step 3: `realloc(conv->wbuff, conv->wsize * sizeof(wchar_t))`. Realloc
> from NULL on the first call, contents preserved, new tail
> uninitialised.
> Step 4 (success): store the pointer in `conv->wbuff`, return 0.
> Step 5 (failure): set `conv->wsize = 0`, free the old `conv->wbuff`,
> set `conv->wbuff = NULL`, return -1 — the same destructive failure
> behaviour as the byte half, and the same invalidation of every pointer
> previously handed out into `wbuff`.
>
> Unlike the byte version, the size computation is a real multiplication:
> `wsize * sizeof(wchar_t)` is unchecked and would wrap for a `wsize`
> above `SIZE_MAX / sizeof(wchar_t)`. Not reachable from libedit's own
> callers, which derive `wsize` from string lengths plus 1024, but a Rust
> port should use a checked allocation rather than reproduce the
> unchecked one.
>
> Callers, each asking only for what it needs plus `CT_BUFSIZ` (1024):
> `ct_decode_string` (decoded length + 1 + 1024), `ct_decode_argv` (total
> argv byte length + 1 + 1024), and `ct_visual_string` (a flat 1024 up
> front, then +1024 per retry). The `cbuff` and `wbuff` halves of a
> `ct_buffer_t` are independent allocations, which is what lets a caller
> hold a live result from one half while overwriting the other.

> [spec:libedit:def:chartype.ct-decode-argv-fn]
> libedit_private wchar_t ** ct_decode_argv(int argc, const char *argv[], ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-decode-argv-fn]
> Decodes an `argc`-element array of multibyte strings into a freshly
> allocated, NULL-terminated array of wide strings whose *text* is packed
> end to end inside `conv->wbuff`. Used by the legacy narrow-char wrappers
> in eln.c to hand an `argv` through to the wide API.
>
> Step 1 — sizing. Compute `bufspace` as the sum over `i` in `[0, argc)`
> of `strlen(argv[i]) + 1` for each non-NULL entry, contributing 0 for
> each NULL entry, then add one more. This byte total is used as a
> **`wchar_t` count**, which is a safe over-estimate: a multibyte string
> never decodes to more wide characters than it has bytes. If
> `conv->wsize < bufspace`, grow the wide half to `bufspace + CT_BUFSIZ`
> (+1024); on failure return NULL.
> Step 2 — allocate `argc + 1` `wchar_t *` slots, zero-filled. On failure
> return NULL, leaving `conv` grown but otherwise untouched.
> Step 3 — decode. Walk `i` from 0 to `argc - 1` with a cursor `p`
> starting at `conv->wbuff`:
> - If `argv[i]` is NULL, store NULL in `wargv[i]` and continue without
>   consuming any space (`mbstowcs` is deliberately not called with a
>   NULL source). The step-1 sum excluded it too, so the accounting
>   stays consistent.
> - Otherwise store `wargv[i] = p` and call `mbstowcs(p, argv[i],
>   bufspace)`, whose result is taken as an `ssize_t`.
> - If that is -1 (i.e. `(size_t)-1`: an invalid or incomplete multibyte
>   sequence), free the pointer array and return NULL immediately. Whole
>   call fails; there is no partial result. `conv->wbuff` keeps whatever
>   earlier elements were decoded into it, which is garbage the caller
>   cannot see or use. Note the comparison relies on `(size_t)-1` cast
>   to `ssize_t` being exactly -1, i.e. on two's complement.
> - Otherwise add 1 to the count to cover the `L'\0'` `mbstowcs` wrote,
>   subtract that from `bufspace`, and advance `p` by the same amount.
>   `bufspace` cannot underflow: each element consumes at most
>   `strlen + 1`, which is exactly what step 1 budgeted for it, and step
>   1 added a spare 1 on top.
> Step 4 — store NULL in `wargv[argc]` (the array has `argc + 1` slots,
> so this is the last one) and return `wargv`.
>
> Return values: NULL if the wide buffer could not be grown, if the
> pointer array could not be allocated, or if any element contained an
> invalid multibyte sequence. Otherwise a pointer to `argc + 1` slots,
> the last NULL, with `wargv[i] == NULL` exactly where `argv[i]` was
> NULL.
>
> Split ownership — the sharp edge. The pointer array is heap-allocated
> and **must be freed by the caller** (the header says so; eln.c calls
> `el_free(wargv)` after each use). The strings it points at must **not**
> be freed: they are interior pointers into `conv->wbuff`. They are all
> invalidated at once by the next `ct_decode_string`, `ct_decode_argv` or
> `ct_visual_string` on the same `conv`, and a reallocation of `wbuff`
> moves every entry simultaneously, so the array becomes a set of
> dangling pointers as a unit. The caller must be done with `wargv`
> before touching that `conv` again. In Rust this is one owned
> `Vec<*const wchar_t>` whose elements borrow from the `ct_buffer_t` —
> two different lifetimes in one return value, which is why it cannot be
> modelled as a `Vec<String>` without changing the observable
> free-it-yourself contract at the C ABI.
>
> Degenerate cases. `argc == 0` yields `bufspace == 1`, a one-slot array
> holding only the NULL terminator, and a wide buffer grown to at least
> 1025 elements. A negative `argc` makes `argc + 1` wrap in the
> allocation count, which is undefined in practice; libedit's own callers
> pass counts they computed themselves, so it is not reachable from
> inside the library, but it is reachable across the public C ABI via
> `el_parse`.

> [spec:libedit:def:chartype.ct-decode-string-fn]
> wchar_t * ct_decode_string(const char *s, ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-decode-string-fn]
> Converts the NUL-terminated multibyte string `s`, interpreted under the
> current `LC_CTYPE`, into a wide string stored in the wide half of
> `conv`, and returns `conv->wbuff`.
>
> Step 1: if `s` is NULL, return NULL immediately. `conv` is not touched.
> Step 2: size it — `len = mbstowcs(NULL, s, 0)`. With a NULL destination
> this is the C99/POSIX "how many wide characters would this produce"
> query, not counting the terminator, and it starts from the initial
> conversion state.
> Step 3: if it returned `(size_t)-1`, return NULL. That is the sole
> error behaviour for malformed input: an invalid or incomplete multibyte
> sequence **anywhere** in `s` rejects the entire string. There is no
> partial decode, no replacement character, no lossy fallback, and
> nothing is written into `conv`. `errno` is left as `mbstowcs` set it
> (`EILSEQ`); this function neither clears nor sets it. Note the
> asymmetry with the encode direction, which drops bad characters
> silently and keeps going.
> Step 4: increment `len` to include the terminating `L'\0'`. If
> `conv->wsize < len`, grow to `len + CT_BUFSIZ` (i.e. +1024 wide
> characters) via the wide resize helper; on -1 return NULL, with
> `wbuff` by then freed and NULLed.
> Step 5: `mbstowcs(conv->wbuff, s, conv->wsize)`. The whole buffer size
> is passed, which may be far larger than needed; `mbstowcs` stops at the
> source NUL regardless. The return value is discarded and unchecked —
> it cannot fail, because the sizing pass already validated the same
> input under the same locale, and it cannot truncate, because
> `conv->wsize >= len` counts the terminator. The result is therefore
> always NUL-terminated. If the locale changed between steps 2 and 5 the
> two conversions could disagree; nothing defends against that.
> Step 6: return `conv->wbuff`.
>
> Return values: NULL for a NULL input, for input containing an invalid
> multibyte sequence, or on allocation failure — the three are not
> distinguishable by the caller. Otherwise `conv->wbuff`, a mutable
> NUL-terminated wide string.
>
> Lifetime of the returned pointer. It *is* `conv->wbuff`. It is
> invalidated by the next `ct_decode_string`, `ct_decode_argv` or
> `ct_visual_string` on the same `conv` — all three write `wbuff` and any
> of them may realloc it or, on OOM, free it. It is **not** invalidated
> by `ct_encode_string` on the same `conv`, which touches only `cbuff`.
> Callers are on a short leash and know it: el.c:128 immediately
> `wcsdup`s the result because `el->el_prog` must outlive the buffer,
> whereas `hist_convert` returns the raw pointer out of hist.c, so its
> caller must consume it before the next `el_scratch` decode. Two live
> decoded strings from one `conv` are impossible.
>
> Assumptions. Each `wchar_t` written is one wide character as `mbstowcs`
> defines it, and the sizing query and the write agree exactly. On a
> platform with 16-bit `wchar_t` a non-BMP character occupies two
> `wchar_t`s and the counts still agree, but the "one element, one
> character" reading does not — see the `WCHAR_MAX < INT32_MAX` warning
> in chartype.h. The result can contain lone surrogates and other
> non-scalar values, so it does not map onto Rust `String`/`char`
> directly.
>
> Symbol visibility: declared without `libedit_private`, so it is an
> exported symbol of the shared library despite being absent from
> `histedit.h`.

> [spec:libedit:def:chartype.ct-enc-width-fn]
> libedit_private size_t ct_enc_width(wchar_t c)

> [spec:libedit:sem:chartype.ct-enc-width-fn]
> Returns how many bytes `c` occupies in the current `LC_CTYPE` locale's
> multibyte encoding **when encoded starting from the initial conversion
> state**, or 0 if `c` cannot be encoded at all.
>
> Step 1: declare a local `char buf[MB_LEN_MAX]` and an `mbstate_t`, and
> `memset` the `mbstate_t` to all zero bytes — the initial conversion
> state.
> Step 2: `size = wcrtomb(buf, c, &mbs)`.
> Step 3: if `size == (size_t)-1`, `c` has no representation in this
> locale (`wcrtomb` set `errno` to `EILSEQ`); return 0. `errno` is
> neither cleared before nor examined after, so the caller sees only the
> 0. A successful encoding never yields 0, so 0 is an unambiguous error
> sentinel by value even though it is not typed as one.
> Step 4: otherwise return `size`.
>
> The encoded bytes are discarded; only the length escapes. `buf` is
> `MB_LEN_MAX` bytes, which is the standard's guaranteed upper bound on
> what `wcrtomb` writes, so the local cannot overflow.
>
> For `c == L'\0'` the answer is at least 1: `wcrtomb` writes the null
> byte, preceded in a stateful encoding by whatever sequence returns the
> converter to the initial state. So this is not a "string length"
> primitive; it counts the terminator too when handed one.
>
> The fresh `mbstate_t` is the load-bearing detail. This is the
> *context-free* width. `ct_encode_char`, which uses this function as its
> bounds check, then performs the actual encoding with `wctomb`, which
> carries libc's process-global, persistent conversion state. In a
> stateless encoding — UTF-8 and every single-byte locale — the two
> always agree. In a stateful encoding they can differ, because `wctomb`
> may need to emit a shift sequence that the initial-state query never
> accounted for; see the overflow note in rule
> `[spec:libedit:sem:chartype.ct-encode-char-fn]`. This function is
> thread-safe (private state); `ct_encode_char` is not.
>
> Callers use it as a byte-offset oracle, so the 0-for-unencodable answer
> silently under-counts: `eln.c:el_gets` sums it across the wide result
> to report `*nread` in bytes to legacy narrow callers,
> `eln.c:el_line`/`el_get(EL_GETLINEINFO)` uses it to translate wide
> cursor positions into byte offsets in the `LineInfo` it hands out, and
> `literal.c` uses it to pre-size a buffer it then fills with
> `ct_encode_char`. Every one of those is a byte count that crosses the
> C ABI, so the port must reproduce the 0 rather than substituting a
> replacement-character width.

> [spec:libedit:def:chartype.ct-encode-char-fn]
> libedit_private ssize_t ct_encode_char(char *dst, size_t len, wchar_t c)

> [spec:libedit:sem:chartype.ct-encode-char-fn]
> Encodes the single wide character `c` into `dst` under the current
> `LC_CTYPE`, provided `len` bytes of space are enough. Returns the
> number of bytes written.
>
> Step 1: if `len < ct_enc_width(c)`, return -1 having written nothing.
> This is the "insufficient space" signal.
> Step 2: `l = wctomb(dst, c)`.
> Step 3: if `l < 0` — `c` is not representable in this locale — call
> `wctomb(NULL, L'\0')` to reset libc's internal conversion state to the
> initial state (with a NULL first argument the second argument is
> ignored and the return value, which reports whether the encoding is
> state-dependent, is discarded), then set `l = 0`.
> Step 4: return `l`.
>
> Return values, all three of which callers must handle distinctly:
> - `>= 1`: that many bytes were written at `dst`.
> - `0`: `c` cannot be encoded in this locale. Nothing is written (in
>   practice — see below), and libc's encoder state has been reset to
>   initial as a side effect. This is a silent drop, not an error the
>   caller is expected to report; `ct_encode_string` just skips the
>   character.
> - `-1`: `len` was too small. Nothing is written and no state changes.
>
> No NUL terminator is ever appended; the caller owns termination.
>
> Bounds-check hazards the port must not inherit blindly:
> - The check compares against `ct_enc_width(c)`, which measures the
>   encoding **from the initial shift state** (rule
>   `[spec:libedit:sem:chartype.ct-enc-width-fn]`), while the write is
>   done by `wctomb`, which continues from libc's **global persistent**
>   state. In a stateful encoding `wctomb` can emit shift bytes the query
>   never counted and write past `len`. Stateless encodings (UTF-8, all
>   single-byte locales) are safe because the two always agree there.
> - Because `ct_enc_width` returns 0 for an unencodable `c`, the test
>   `len < 0` is false for any `len` including 0, so step 2 runs and
>   passes `dst` to `wctomb` with zero guaranteed space. Safety here
>   rests entirely on `wctomb` failing for the same character and not
>   writing before it fails — implementation behaviour, not a guarantee.
>   The port should reject `len == 0` outright.
> - `wctomb` keeps process-global state and is not thread-safe. libedit
>   mixes it with `ct_enc_width`'s thread-safe `wcrtomb` against the same
>   logical output stream, and never resets the state except on the
>   failure path above.
>
> Callers pass very different `len` values, which is why the -1 path is
> live: `ct_encode_string` passes a hard-coded 5 (and turns -1 into
> `abort()`); `terminal.c:terminal__putc` passes `MB_LEN_MAX`;
> `keymacro.c` and `literal.c` pass the genuine remaining space.

> [spec:libedit:def:chartype.ct-encode-string-fn]
> char * ct_encode_string(const wchar_t *s, ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-encode-string-fn]
> Converts the NUL-terminated wide string `s` into the current
> `LC_CTYPE` locale's multibyte encoding, storing the result in the byte
> half of `conv`, and returns `conv->cbuff`.
>
> Step 1: if `s` is NULL, return NULL immediately. `conv` is not touched.
> Step 2: set a cursor `dst = conv->cbuff` (which may be NULL on a virgin
> `conv`).
> Step 3: loop forever:
> - Let `used = dst - conv->cbuff`, the bytes written so far.
> - Headroom check: if `conv->csize - used < 5`, call
>   `ct_conv_cbuff_resize(conv, conv->csize + CT_BUFSIZ)` (i.e. +1024
>   bytes). On -1, return NULL — and note the resize helper has by then
>   freed and NULLed `cbuff`, so the buffer is destroyed, not merely
>   unextended. On success re-derive `dst = conv->cbuff + used`, because
>   realloc may have moved the block. On a virgin `conv` this fires on
>   the first pass (`0 - 0 = 0 < 5`) and allocates the initial 1024
>   bytes. The threshold 5 is a hard-coded constant, unrelated to
>   `MB_CUR_MAX`.
> - If `*s == L'\0'`, break out of the loop. The terminator is written
>   after the loop, never inside it.
> - Encode one character: `used = ct_encode_char(dst, 5, *s)`. The length
>   argument is the literal constant 5, *not* the real remaining space.
>   The headroom rule guarantees at least 5 free bytes, so this is not an
>   overflow by itself, but it caps the per-character encoding at 5
>   bytes.
> - If `ct_encode_char` returns -1 — which means `ct_enc_width(*s) > 5`,
>   i.e. this character needs more than 5 bytes in this locale — the
>   function calls `abort()` and the process dies. Unreachable in UTF-8
>   (max 4 bytes) and in any single-byte locale, but `MB_LEN_MAX` is 16
>   on glibc and a stateful encoding that emits shift sequences can
>   exceed 5. A Rust port must decide what to do here; it is a real,
>   locale-reachable process abort in the C.
> - Otherwise advance `s` by one wide character and `dst` by the returned
>   byte count. A return of 0 means the character has no representation
>   in this locale: nothing is written, `dst` does not move, the
>   character is **silently dropped**, and no error is reported anywhere.
>   The output is then shorter than the input with no way for the caller
>   to detect it.
> Step 4: store `'\0'` at `dst` (the `< 5` headroom rule guarantees the
> space) and return `conv->cbuff`.
>
> Return values: NULL if `s` was NULL or if the buffer could not be
> grown; otherwise `conv->cbuff`, a NUL-terminated byte string. It is
> **mutable**, and callers mutate it — hist.c strips a trailing newline
> in place via `ptr[--len] = '\0'` on the returned pointer.
>
> Lifetime of the returned pointer. It *is* `conv->cbuff`, not a copy.
> It stays valid until the next call that touches the same `conv`'s byte
> half, which means the next `ct_encode_string` on that `conv` — the
> only function that writes `cbuff`. That next call may realloc (moving
> the data) or, on OOM, free it outright. It is **not** invalidated by
> `ct_decode_string`, `ct_decode_argv` or `ct_visual_string` on the same
> `conv`, because those touch only `wbuff`. terminal.c leans on exactly
> that: `ct_encode_string(ct_visual_string(ct_decode_string(*ts,
> &el->el_scratch), &el->el_visual), &el->el_scratch)` decodes into
> `el_scratch.wbuff`, expands into a *different* buffer's `wbuff`, and
> encodes back out into `el_scratch.cbuff`. Conversely, two live results
> from the same `conv`'s byte half cannot coexist: refresh.c has a debug
> block permanently disabled behind `#ifdef notyet` with the comment
> that it "can't conveniently encode both d & s here". In Rust this is a
> `&mut [u8]` borrowed from the `ct_buffer_t`; the borrow checker
> enforces the single-live-result rule natively, and the C ABI shim must
> both keep the owning buffer alive for the caller's use and refuse to
> hand out two overlapping pointers.
>
> Encoder state. `ct_encode_char` uses `wctomb`, which carries libc's
> process-global, non-thread-safe conversion state. This function does
> **not** reset that state before it starts, so in a stateful encoding a
> string may be encoded starting from whatever shift state a previous
> call left behind; and an unencodable character mid-string resets the
> state back to initial as a side effect (see rule
> `[spec:libedit:sem:chartype.ct-encode-char-fn]`).
>
> Growth is monotonic: the buffer is never shrunk, so one long line
> permanently inflates the owning `EditLine`.
>
> Symbol visibility: declared without `libedit_private`, so unlike most
> of this file it is an exported symbol of the shared library even
> though it does not appear in `histedit.h`.

> [spec:libedit:def:chartype.ct-visual-char-fn]
> libedit_private ssize_t ct_visual_char(wchar_t *dst, size_t len, wchar_t c)

> [spec:libedit:sem:chartype.ct-visual-char-fn]
> Writes the printable visual representation of the single wide character
> `c` into `dst`, whose capacity `len` is counted in `wchar_t` (not
> bytes), and returns how many `wchar_t` were written. Never appends a
> NUL. Classify `c` with `ct_chr_class`, then:
>
> **`CHTYPE_TAB`, `CHTYPE_NL` and `CHTYPE_ASCIICTL` — one shared arm.**
> If `len < 2`, return -1 having written nothing. Otherwise write
> `L'^'`, then: if `c == L'\177'` (0x7F, DEL) write `L'?'`, else write
> `c | 0100` — that is, `c | 0x40`, the classic "uncontrolify". Return 2.
> Worked examples: 0x00 → `^@`, 0x01 → `^A`, 0x09 (TAB) → `^I`, 0x0A (LF)
> → `^J`, 0x1B → `^[`, 0x1F → `^_`, 0x7F → `^?`.
> The subtle case: `ct_chr_class` puts *anything* below 0x100 that
> `iswcntrl` accepts into `CHTYPE_ASCIICTL`, which in a glibc UTF-8 or
> Latin-1 locale includes the C1 range U+0080..U+009F. For those,
> `c | 0x40` yields U+00C0..U+00DF, so U+0085 renders as `^` followed by
> U+00C5 (`Å`). That is not a caret escape in any meaningful sense, it is
> locale-dependent, and it is what the C emits — the port must reproduce
> it rather than "fix" it.
>
> **`CHTYPE_PRINT`.** If `len < 1`, return -1. Otherwise write `c`
> unchanged and return 1. Exactly one cell, whatever `wcwidth` says about
> its column count — see the cells-versus-columns note below.
>
> **`CHTYPE_NONPRINT`.** If `(ssize_t)len < ct_visual_width(c)` (7 or 8),
> return -1 having written nothing. Otherwise write `L'\\'`, `L'U'`,
> `L'+'`, then uppercase hex digits indexed out of the literal
> `"0123456789ABCDEF"` using `(unsigned int)c`:
> - if `c > 0xffff`: five digits, from `(c >> 16) & 0xf`, `(c >> 12) &
>   0xf`, `(c >> 8) & 0xf`, `(c >> 4) & 0xf`, `c & 0xf`. Return 8. Form:
>   `\U+12345`.
> - otherwise: four digits, from `(c >> 12) & 0xf`, `(c >> 8) & 0xf`,
>   `(c >> 4) & 0xf`, `c & 0xf`. Return 7. Form: `\U+1234`.
> The source comment explains the split as preferring "standard 4-byte
> display over 5-byte".
>
> **Any other class.** Return 0, writing nothing. Unreachable, because
> `ct_chr_class` is total over the four classes above plus `CHTYPE_TAB`;
> if it *were* reachable, `ct_visual_string` would spin forever on it.
> A `/*FALLTHROUGH*/` comment sits after an unconditional `return` in
> this arm and is dead and misleading.
>
> Bugs and UB in the non-printable arm:
> - **Only five hex digits are ever emitted**, so any code point at or
>   above U+100000 loses every bit above 0x0FFFFF. U+10FFFF renders as
>   `\U+0FFFF`, i.e. plane 16 is displayed as if it were plane 0. The
>   port has to decide whether to reproduce this (it is observable
>   through the rendered line and through `terminal_telltc`) or widen the
>   field, which would also change `ct_visual_width` and every column
>   calculation downstream.
> - The `(unsigned int)` cast: where `wchar_t` is 32-bit signed, a
>   negative `c` becomes a large unsigned value and the shifts, while
>   well-defined, produce meaningless digits. This is reachable —
>   `MB_FILL_CHAR` is `(wint_t)-1` and refresh.c classifies screen-image
>   cells that can hold it. It is also an implicit narrowing wherever
>   `wchar_t` is wider than `unsigned int`.
> - The `(ssize_t)len` cast makes any `len` above `SSIZE_MAX` compare as
>   negative and spuriously return -1. Not reachable from libedit's
>   callers.
>
> Buffer sizing. `VISUAL_WIDTH_MAX` is 8, exactly the largest possible
> return, and refresh.c (`re_addc`, `re_fastaddc`) and terminal.c
> (`terminal_writec`) all pass a stack `wchar_t visbuf[VISUAL_WIDTH_MAX]`.
> `ct_visual_string` and keymacro.c pass the genuine remaining space and
> handle -1 by growing.
>
> Cells versus columns. This function counts `wchar_t` cells written;
> `ct_visual_width` counts terminal columns. They agree for
> `CHTYPE_ASCIICTL` (2 and 2) and `CHTYPE_NONPRINT` (7/8 and 7/8). They
> do **not** agree for `CHTYPE_PRINT`, where this writes 1 cell but the
> character may occupy 0 columns (combining marks) or 2 (East Asian
> wide); the display layer pads the extra columns with `MB_FILL_CHAR` in
> the screen image. They do not agree for TAB (2 cells written, width
> reports 1) or NL (2 cells written, width reports 0) either; those two
> classes are expanded by the caller and never reach the display through
> this pairing. The header's claim that this function "match[es] the
> width given by ct_visual_width()" is therefore false in general, and
> the port must keep the two quantities as distinct types.

> [spec:libedit:def:chartype.ct-visual-string-fn]
> libedit_private const wchar_t * ct_visual_string(const wchar_t *s, ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-visual-string-fn]
> Expands every character of the wide string `s` into its printable
> visual form (rule `[spec:libedit:sem:chartype.ct-visual-char-fn]`) and
> returns the NUL-terminated result in the wide half of `conv`.
>
> Step 1: if `s` is NULL, return NULL. `conv` is not touched.
> Step 2: `ct_conv_wbuff_resize(conv, CT_BUFSIZ)` — ensure at least 1024
> `wchar_t`. Because the resize helper never shrinks, this is a no-op on
> a `conv` that is already at least that large, and such a `conv` keeps
> its full existing size for step 3's space calculations. On -1 return
> NULL (the helper has by then freed and NULLed `wbuff`).
> Step 3: set `dst = conv->wbuff` and loop while `*s != L'\0'`:
> - `used = ct_visual_char(dst, conv->wsize - (dst - conv->wbuff), *s)`.
>   The length passed is the genuine remaining capacity in `wchar_t`,
>   unlike the fixed 5 that `ct_encode_string` passes on the byte side.
> - If `used != -1`: advance `s` by one character and `dst` by `used`,
>   and continue.
> - If `used == -1` (not enough room for this character's expansion):
>   save the current offset `dst - conv->wbuff`, grow by another
>   `CT_BUFSIZ` (`ct_conv_wbuff_resize(conv, conv->wsize + 1024)`),
>   return NULL if that fails, re-derive `dst = conv->wbuff + offset`
>   because realloc may have moved the block, and **retry the same source
>   character** — `s` is not advanced. Progress is guaranteed because a
>   single expansion never needs more than `VISUAL_WIDTH_MAX` (8) cells
>   and each growth adds 1024.
> - A return of 0 from `ct_visual_char` would advance neither `s` nor
>   `dst` and spin forever. It cannot happen: `ct_chr_class` classifies
>   every input into one of the four arms that return non-zero. The port
>   should still make the exhaustive match total rather than rely on it.
> Step 4: the terminator needs one more cell, and the loop may have left
> `dst` exactly at the end of the buffer. If `dst >= conv->wbuff +
> conv->wsize`, grow by another `CT_BUFSIZ`, return NULL on failure, and
> re-derive `dst` from the possibly-moved `wbuff`. (`dst` can never
> exceed the end, because `ct_visual_char` was given the true remaining
> length; only exact equality is possible. The source marks this arm
> `/* sigh */`.)
> Step 5: store `L'\0'` at `dst` and return `conv->wbuff`.
>
> Return values: NULL if `s` was NULL or if any growth failed; otherwise
> `conv->wbuff`. An empty input yields an empty (single-`L'\0'`) result,
> after allocating at least 1024 wide characters.
>
> Lifetime of the returned pointer. It *is* `conv->wbuff`, the same
> storage `ct_decode_string` and `ct_decode_argv` return, and it is
> invalidated by the next call to any of those three on the same `conv`.
> It is not invalidated by `ct_encode_string` on that `conv`. The return
> type is `const wchar_t *` while `ct_decode_string` returns a mutable
> `wchar_t *` into the identical field, so the constness is advisory
> only — it does not indicate a different or longer-lived storage class.
> terminal.c's `terminal_telltc` deliberately passes `el_visual` here and
> `el_scratch` to the surrounding decode and encode calls precisely so
> that a decoded string and its visual expansion can be live at the same
> time. The header's comment "Uses a static buffer, so not threadsafe" is
> stale — the buffer is caller-supplied — but the aliasing warning it
> implies is exactly right, and two live visual expansions from one
> `conv` are impossible.
>
> Cells, not columns. The result is measured in `wchar_t` cells. A
> double-width printable occupies one cell here and two terminal columns;
> a combining character occupies one cell and zero columns. Column counts
> come from `ct_visual_width`, and the two must not be conflated.

> [spec:libedit:def:chartype.ct-visual-width-fn]
> libedit_private int ct_visual_width(wchar_t c)

> [spec:libedit:sem:chartype.ct-visual-width-fn]
> Returns the number of terminal **columns** `c` occupies once rendered
> the way `ct_visual_char` renders it. Classify `c` with `ct_chr_class`
> and switch:
>
> - `CHTYPE_ASCIICTL` → 2, the caret form `^X`. Agrees with
>   `ct_visual_char`.
> - `CHTYPE_TAB` → 1. This does **not** agree with `ct_visual_char`,
>   which renders a tab as the two characters `^I`. The source concedes
>   the point ("Hmm, this really need to be handled outside!"), and the
>   callers do handle it outside: refresh.c's `re_addc` expands a tab to
>   the next multiple-of-8 tab stop itself, and `re_goto_bottom`'s switch
>   has a dedicated `CHTYPE_TAB` arm that advances `h` to the next tab
>   stop and never calls this function. So the value 1 is effectively
>   dead, but it is what the function returns.
> - `CHTYPE_NL` → 0, with a source comment questioning whether it should
>   be 1. Also disagrees with `ct_visual_char`, which renders `\n` as
>   `^J` (2 cells). Newlines are likewise intercepted by the callers.
> - `CHTYPE_PRINT` → `wcwidth(c)`, passed straight through. Locale
>   dependent: 0 for combining marks and other zero-width characters, 1
>   for ordinary characters, 2 for East Asian wide and other double-width
>   characters, and **-1 if `wcwidth` judges `c` non-printable**. The -1
>   is not screened out. `ct_chr_class` reached this arm via `iswprint`,
>   so a locale whose `iswprint` and `wcwidth` disagree makes this
>   function return a negative width, which refresh.c adds directly into
>   its column accumulator `h`. The port must reproduce the pass-through,
>   including the negative, unless it is prepared to change the rendered
>   geometry.
> - `CHTYPE_NONPRINT` → 8 if `c > 0xffff`, else 7 — the widths of
>   `\U+12345` and `\U+1234` respectively. These are the only two
>   possible values, which is why `VISUAL_WIDTH_MAX` is 8.
> - anything else → 0, commented "should not happen". Unreachable,
>   because `ct_chr_class` is total over the five classes.
>
> Return type is `int`, whereas `ct_visual_char` returns `ssize_t`; the
> non-printable arm of `ct_visual_char` compares the two directly.
>
> The `CHTYPE_PRINT` arm is where columns and cells part company:
> `ct_visual_char` always writes exactly one `wchar_t` for a printable,
> but this reports 0 or 2 columns for zero-width and double-width
> characters. The display layer reconciles them by writing the character
> into one screen-image cell and filling each additional occupied column
> with `MB_FILL_CHAR` (`(wint_t)-1`). refresh.c also uses `wcwidth`
> directly, ahead of this call, to decide whether a double-width
> character must be pushed to the next line rather than split across the
> right margin. A Rust port must therefore keep three distinct notions —
> bytes (`ct_enc_width`), cells (`ct_visual_char`) and columns (this
> function) — and must obtain columns from a locale-aware `wcwidth`
> equivalent, not from `char` counting.

