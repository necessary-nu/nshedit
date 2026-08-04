# src/literal.c, src/literal.h

> [spec:libedit:def:literal.el-literal-t]
> typedef struct el_literal_t

> [spec:libedit:def:literal.literal-add-fn]
> libedit_private wint_t literal_add(EditLine *el, const wchar_t *buf, const wchar_t *end, int *wp)

> [spec:libedit:sem:literal.literal-add-fn]
> Stores one *literal* — a run of wide characters that must be sent to
> the terminal but occupy zero display columns, in practice an SGR colour
> escape embedded in a prompt — in the per-`EditLine` side table
> `el->el_literal`, and returns a magic `wint_t` sentinel to be written
> into the display buffer in its place. When the display layer later
> prints that sentinel (`terminal__putc`), the stored byte string is
> emitted instead of a character.
>
> Argument shape. `buf` points at the first wide character of the
> invisible sequence. `end` points one past its last character, i.e. at
> the closing delimiter; the character at `*end` is not part of the
> sequence and is never read. `end[1]` — one further still — is the first
> *visible* character after the sequence. It is appended to the stored
> byte string, and it is the only thing that contributes any display
> width. The function dereferences `end[1]` unconditionally, so the
> caller must guarantee both `end` and `end + 1` are in bounds;
> `prompt_print` enforces that by declining to call when either the
> closing delimiter or the character after it is the string terminator
> (silently dropping such a trailing literal). `buf == end`, an empty
> sequence, is legal and is not special-cased.
>
> Step 1. `w = wcwidth(end[1])` and `*wp = (int)w`. The out-parameter is
> written first, unconditionally, before any early return, so it is
> always set on every path including the failure paths. It reports the
> column width of the *visible* character alone: -1 if that character is
> non-printable in the current locale, 0 for a zero-width or combining
> character, otherwise 1 or 2. It is never the width of the escape
> sequence, which is zero by construction — that is the entire point of
> the mechanism.
>
> Step 2. If `w < 0`, return 0 at once. Nothing is allocated, nothing is
> appended to the table, and `*wp` retains the negative value. The caller
> distinguishes this from an allocation failure by inspecting `*wp`:
> `re_putliteral` tests `c == 0 || w < 0` and abandons the literal
> entirely in either case.
>
> Step 3. Compute the encoded byte length. `len = end - buf` (an element
> count, possibly 0). `w` is then reused as a byte counter: the sum of
> `ct_enc_width(buf[i])` for `i` in `[0, len)`, plus
> `ct_enc_width(end[1])`. `ct_enc_width` is the number of bytes `wcrtomb`
> produces from a freshly zeroed conversion state, or 0 when the
> character is not representable in the current locale's charset.
>
> Step 4. `b = el_malloc(w + 1)`. On failure return 0. `*wp` has by then
> been set to the non-negative width, so a 0 return with `*wp >= 0` means
> allocation failure and nothing else.
>
> Step 5. Encode into `b`. `n = 0`; for each `i` in `[0, len)`,
> `n += ct_encode_char(b + n, w - n, buf[i])`; then
> `n += ct_encode_char(b + n, w - n, end[1])`; then `b[n] = '\0'`. The
> stored string is therefore the invisible sequence followed by the
> visible character, all in the locale's multibyte encoding, NUL
> terminated. No return value is checked.
>
> Encoding hazard, undefined behaviour. `ct_enc_width` measures with
> `wcrtomb` from a zeroed `mbstate_t`, while `ct_encode_char` writes with
> `wctomb`, which carries process-global conversion state. In a stateful
> encoding (ISO-2022-JP and relatives) the two disagree: `wctomb` may
> emit shift sequences the measuring pass did not budget for, `n` runs
> past `w`, and the writes overflow the heap allocation. The length guard
> inside `ct_encode_char` cannot catch it, because the remaining-space
> argument is computed as `(size_t)(w - n)` and a negative difference
> casts to an enormous `size_t`. Symmetrically, a `-1` return from
> `ct_encode_char` *decrements* `n`, which can drive `b + n` before the
> start of the allocation. In a single-byte or UTF-8 locale the measured
> length is exact and none of this fires. The Rust port should encode
> once and use the length actually produced, rather than measuring and
> then re-encoding.
>
> Step 6. Append to the table, growing it if needed. If
> `l_idx == l_len` — which includes the first-ever call, where both are
> 0 — set `l_len += 4` and `el_realloc` `l_buf` to `l_len` `char *`
> elements. Growth is a fixed +4 elements per reallocation (4, 8, 12,
> 16, …), linear rather than doubling, so N literals cost about N/4
> reallocations. New slots are left uninitialised. If the reallocation
> fails, free `b`, restore `l_len -= 4`, and return 0; the table is left
> byte-for-byte as it was and no entry is added. (This path calls libc
> `free` directly rather than `el_free`; with el.h's current macros the
> two are the same function, so there is no observable difference, but it
> is a latent inconsistency if the allocator macros are ever redefined.)
>
> Step 7. `l_buf[l_idx++] = b`, then return `EL_LITERAL | (wint_t)(l_idx - 1)`,
> the index of the slot just filled.
>
> Sentinel encoding. `EL_LITERAL` is `(wint_t)0x80000000` — bit 31 alone.
> A successful return is that bit set, bitwise-ORed with the table index,
> which therefore occupies bits 0..30. The representable index range is
> 0..0x7FFFFFFF, so **at most 2^31 = 2147483648 literals** can be
> addressed by one table. There is no bound check anywhere: beyond that
> the index would alias the marker bit, and the `size_t` → `wint_t`
> narrowing cast truncates on LP64. Reaching it requires 2^31 successful
> `malloc`s so it is unreachable in practice, but a port should either
> bound the index explicitly or fail rather than silently wrap. A return
> of 0 is the sole error sentinel and is unambiguous, because every
> success has bit 31 set and is therefore `>= 0x80000000`.
>
> Distinguishing a sentinel from a real character. `terminal__putc` tests
> `c == MB_FILL_CHAR` *first*, and only then `c & EL_LITERAL`. That order
> is load-bearing: `MB_FILL_CHAR` is `(wint_t)-1` = `0xFFFFFFFF`, which
> also has bit 31 set and would otherwise be decoded as literal index
> `0x7FFFFFFF`. Any other value with bit 31 set is treated as a literal
> reference with no further validation. Genuine Unicode scalar values top
> out at U+10FFFF, so no real character collides — but that holds only
> because the values reaching the screen image are Unicode code points
> (see the chartype rules), not by any check performed here.
>
> Invalidation. Sentinels carry an *index*, never a pointer, so growing
> the table — including a `realloc` that relocates `l_buf` — does not
> invalidate any sentinel already handed out. The only thing that
> invalidates them is `literal_clear`, which `re_refresh` calls at the
> top of every full redraw; see
> `[spec:libedit:sem:literal.literal-clear-fn]` for the stale-sentinel
> hazard that creates.
>
> Ownership. The table owns `b`; nothing else frees it and it is never
> reallocated or rewritten in place. `literal_get` hands out a borrowed
> pointer with no lifetime beyond the next `literal_clear`.
>
> Degenerate cases. There is no "table full" state — the table grows
> until an allocator call fails, and every allocator failure returns 0.
> An empty sequence (`buf == end`) still allocates, still consumes a
> table slot, and still returns a sentinel; the stored string is just the
> encoding of the visible character. If every character involved is
> unrepresentable in the locale, every `ct_enc_width` returns 0, `w` is 0,
> `el_malloc(1)` succeeds, and the stored string is empty — the sentinel
> then prints nothing at all, so the visible character is silently
> dropped from the output while still being charged `*wp` columns in the
> display buffer.
>
> `wchar_t`/`wint_t` width assumptions, and what breaks in Rust.
> `EL_LITERAL` requires `wint_t` to be at least 32 bits wide; on
> glibc/POSIX it is `unsigned int`, exactly 32. On a platform with a
> 16-bit `wchar_t`/`wint_t` the constant does not fit and the scheme
> collapses outright. Worse, `el_display` and `el_vdisplay` are declared
> `wint_t **` but refresh.c casts them to `wchar_t *` before passing them
> to `re_update_line`, `re__copy_and_pad` and `terminal_overwrite`. On
> glibc `wchar_t` is `int` (signed 32-bit) and `wint_t` is
> `unsigned int`, so a sentinel written as `0x80000000` is read back
> through a `wchar_t` lvalue as a negative `int` and converted back to
> `wint_t` at the `terminal__putc` call site. That round trip is a strict
> aliasing violation, and before C23 the signed conversion is only
> implementation-defined; it works solely because the two types share a
> size and the machine is two's complement. The Rust port must carry the
> screen image as a single unsigned 32-bit element type (`u32`) end to
> end, never `char`: `0x80000000` is not a Unicode scalar value, so
> `char` cannot hold a sentinel at all. Test the marker bit explicitly
> and keep the `MB_FILL_CHAR`-before-`EL_LITERAL` ordering.

> [spec:libedit:def:literal.literal-clear-fn]
> libedit_private void literal_clear(EditLine *el)

> [spec:libedit:sem:literal.literal-clear-fn]
> Releases the entire literal table and returns `el->el_literal` to the
> state `literal_init` leaves it in. Returns nothing and cannot fail.
>
> Step 1. If `l_len == 0`, return immediately. The guard is on `l_len`,
> the allocated capacity, not on `l_idx`, the in-use count — so a table
> that has capacity but no live entries still runs the body and frees
> `l_buf`. This guard is what makes the function safe to call on a
> freshly zeroed (or already cleared) store, and therefore idempotent.
>
> Step 2. Free `l_buf[i]` for `i` in `[0, l_idx)` — the in-use prefix
> only. Slots in `[l_idx, l_len)` were produced by `el_realloc` and never
> written, so their contents are indeterminate and freeing them would be
> undefined behaviour. This is precisely why "max in use" is tracked
> separately from "max allocated".
>
> Step 3. Free `l_buf` itself.
>
> Step 4. Set `l_buf = NULL`, `l_len = 0`, `l_idx = 0`.
>
> This is a full deallocation, not a capacity-preserving reset: the next
> `literal_add` starts over from a fresh four-element allocation, and
> indices restart at 0.
>
> What it invalidates. Every sentinel ever returned by `literal_add`
> becomes stale the instant this runs. The stored byte strings are freed
> and the index space is reused from zero. There is no generation
> counter, no tombstone, and no way for `literal_get` to detect staleness
> beyond its `l_idx > idx` assertion — which only catches indices that
> happen to fall outside the *new* in-use prefix.
>
> Call sites and lifetime. The only caller inside the editor is
> `re_refresh`, at the top of every full redraw, immediately before the
> prompt is re-rendered; `literal_end` calls it once more at teardown. So
> the table's lifetime is exactly one refresh cycle. It is *not* called
> by `el_reset`, nor by the fast single-character path (`re_fastaddc` /
> `re_fastputc`), so the table survives those unchanged.
>
> Stale-sentinel hazard, reachable undefined behaviour. `re_refresh`
> clears the table and then re-renders the prompt into `el_vdisplay`, so
> every sentinel in the *virtual* screen image is freshly issued. But
> `el_display`, the model of what is physically on screen, still holds
> the *previous* frame's sentinels, copied there by `re__copy_and_pad` at
> the end of the last refresh — and `terminal_move_to_char` re-emits
> characters straight out of `el_display` through `terminal_overwrite` →
> `terminal__putc` → `literal_get`. Those old sentinels are correct only
> because a prompt that renders identically re-issues the same indices in
> the same order, so index *i* maps to a string with the same bytes as
> before. When the prompt function returns something different between
> refreshes — a clock, a git-branch indicator, a changing colour — index
> *i* can map to different bytes and the wrong escape is emitted; and if
> the new frame produced fewer literals than the old, the index is `>=
> l_idx` and `literal_get` either trips its assertion or, under `NDEBUG`,
> reads out of bounds (or dereferences a `NULL` `l_buf`) and passes the
> result to `fputs`. The libedit build does not define `NDEBUG` itself,
> but downstream packagers commonly do. The port must confront this
> deliberately: the *observable* behaviour for a stable prompt is frozen
> by [dec:libedit:no-c-ffi], but the undefined behaviour for an unstable
> one is not something a safe Rust implementation can reproduce, so
> choose a defined fallback (skip the cell, or emit nothing) rather than
> replicate the fault.

> [spec:libedit:def:literal.literal-end-fn]
> libedit_private void literal_end(EditLine *el)

> [spec:libedit:sem:literal.literal-end-fn]
> Teardown for the literal store. The whole body is a call to
> `literal_clear(el)`; there is nothing else to release, because the
> table and the byte strings it points at are the store's only resources.
> Returns nothing and cannot fail.
>
> Consequences worth stating explicitly. Because `literal_clear` leaves
> `l_buf = NULL`, `l_len = 0`, `l_idx = 0`, this function is idempotent —
> a second call takes the `l_len == 0` early return — and it leaves the
> store in exactly the state `literal_init` produces, so an `EditLine`
> could legally be re-initialised and reused afterwards without a further
> `literal_init`. Every sentinel previously issued is invalidated, with
> the same hazards described in
> `[spec:libedit:sem:literal.literal-clear-fn]`.
>
> Called once, from `el_end`, after `prompt_end` and `sig_end` and before
> the standalone `el_free` calls that release the `EditLine`'s own
> buffers. The ordering matters only in that it must run before the
> `EditLine` itself is freed; it depends on no other subsystem.
>
> In Rust this is a `Drop` impl on the literal store (or simply nothing,
> if the store owns its `Vec<CString>` directly) rather than an
> explicitly sequenced teardown call.

> [spec:libedit:def:literal.literal-get-fn]
> libedit_private const char * literal_get(EditLine *el, wint_t idx)

> [spec:libedit:sem:literal.literal-get-fn]
> Resolves a sentinel back to the byte string it stands for. The
> argument is the value `literal_add` returned, taken verbatim out of a
> display-buffer cell — not a bare index.
>
> Step 1. `assert(idx & EL_LITERAL)`: the argument must have bit 31 set,
> i.e. must actually be a literal sentinel. Note this is a bitwise test,
> so `literal_add`'s error return of 0 trips it. The sole caller,
> `terminal__putc`, has already established the bit (and has already
> excluded `MB_FILL_CHAR`, `(wint_t)-1`, which also has bit 31 set —
> see `[spec:libedit:sem:literal.literal-add-fn]`).
>
> Step 2. `idx &= ~EL_LITERAL`: clear bit 31, leaving bits 0..30 as the
> table index. Since the result is at most `0x7FFFFFFF`, the subsequent
> widening to `size_t` is value-preserving on every POSIX target.
>
> Step 3. `assert(l_idx > (size_t)idx)`: the index must fall inside the
> in-use prefix, not merely inside the allocated capacity — slots between
> `l_idx` and `l_len` hold indeterminate pointers.
>
> Step 4. Return `l_buf[idx]`: a borrowed, NUL-terminated byte string in
> the locale's multibyte encoding, owned by the table. It is never `NULL`
> for a live index, because `literal_add` only ever stores a successful
> allocation. It stays valid until the next `literal_clear`, and no
> longer. The caller writes it with `fputs`.
>
> What the returned string contains. Both halves of the original input:
> the invisible sequence *and* the encoding of the visible character that
> followed it. So printing it advances the terminal's real cursor by the
> width of that one visible character, which is what the display buffer
> already recorded when it laid down the sentinel cell followed by
> `MB_FILL_CHAR` padding.
>
> No error path, and the undefined behaviour that leaves. Both checks are
> plain `assert`, so both vanish under `NDEBUG`. There is no return value
> reserved for "invalid index" — the function cannot report failure. With
> assertions compiled out, a stale or out-of-range sentinel reads
> `l_buf[idx]` past the end of the array, or dereferences a `NULL`
> `l_buf` if the table has been cleared, and hands the indeterminate
> result to `fputs`. This is reachable in ordinary operation, not only
> through caller error: see the `el_display` stale-sentinel path in
> `[spec:libedit:sem:literal.literal-clear-fn]`. Nothing validates that
> the index bits correspond to an entry that was issued by *this*
> generation of the table.
>
> Port note. The natural Rust shape is
> `fn get(&self, sentinel: u32) -> Option<&CStr>`, returning `None` when
> bit 31 is clear or the index is outside the live range. Because the C
> behaviour on an invalid index is undefined rather than specified, the
> port is free to choose any defined response there; what it must not
> change is the bytes yielded for a *valid* index, which cross the C ABI
> as terminal output.

> [spec:libedit:def:literal.literal-init-fn]
> libedit_private void literal_init(EditLine *el)

> [spec:libedit:sem:literal.literal-init-fn]
> Brings the literal store up empty. The whole body is
> `memset(&el->el_literal, 0, sizeof(el_literal_t))`, which for this
> three-field struct means `l_buf = NULL`, `l_idx = 0`, `l_len = 0` — no
> table is allocated, so the first `literal_add` is the one that pays for
> the initial four-element array. Returns nothing and cannot fail.
>
> Two details the memset hides. It zeroes the whole struct including any
> padding, which matters only for byte-wise comparison, not for
> behaviour. And it relies on all-bits-zero being a null pointer
> representation for `char **`; that holds on every POSIX target in scope
> under [dec:libedit:posix-only-scope], but a port should set the fields
> explicitly rather than reproduce the memset.
>
> It does not free anything. Calling it on a store that already holds
> entries overwrites `l_buf` and leaks the pointer array together with
> every byte string in it. Nothing in libedit does this — the sole call
> is from `el_init_internal`, once per `EditLine`, and it is the
> counterpart of `literal_end`. `el_reset` does not call it.
>
> No sentinel can be outstanding at this point, so nothing is
> invalidated; the state it produces is identical to the state
> `literal_clear` and `literal_end` leave behind.

