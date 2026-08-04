# src/unvis.c

> [spec:libedit:def:unvis.nv]
> struct nv {
>   char name[7];
>   uint8_t value;
> }

> [spec:libedit:def:unvis.strnunvis-fn]
> int strnunvis(char *dst, size_t dlen, const char *src)

> [spec:libedit:sem:unvis.strnunvis-fn]
> Exactly `strnunvisx(dst, dlen, src, 0)` — a bounded decode with flag
> `0`. Flag `0` means the only escape introducer recognised is backslash:
> `%`, `&` and `=` are ordinary bytes, because `VIS_HTTP1808`,
> `VIS_HTTP1866` and `VIS_MIMESTYLE` are all clear, and `VIS_NOESCAPE` is
> clear so `\` is decoded. All behaviour — the per-byte loop, the
> push-back retry, the `UNVIS_END` flush, the bounds check on `dlen`
> (which must be at least decoded length + 1, counting the terminating
> NUL), the NUL termination, and the return value (decoded length, or -1
> with `errno` set to `EINVAL` for a malformed escape or `ENOSPC` for an
> undersized buffer) — is that of rule
> `[spec:libedit:sem:unvis.strnunvisx-fn]`.

> [spec:libedit:def:unvis.strnunvisx-fn]
> int strnunvisx(char *dst, size_t dlen, const char *src, int flag)

> [spec:libedit:sem:unvis.strnunvisx-fn]
> Decodes the NUL-terminated string `src` into the buffer `dst` of `dlen`
> bytes by driving [spec:libedit:sem:unvis.unvis-fn] one byte at a time
> under `flag`. Returns the number of decoded bytes written, not counting
> the terminating NUL; returns -1 on error with `errno` set. `dst` is NUL
> terminated on success only.
>
> Setup. A decoder state variable is initialised to `0` (state
> `S_GROUND`, name index 0) and a one-byte scratch cell `t` is declared;
> `t`'s initial value is irrelevant because every state that reads it
> back has written it first. A cursor `dst` starts at the front of the
> buffer; the returned count is its final displacement.
>
> Main loop. For each byte `c` of `src`, from the first up to but not
> including the terminating NUL, call `unvis(&t, c, &state, flag)` and
> dispatch on the result:
>
> - `UNVIS_VALID`: reserve one output byte (see Bounds), then append `t`
>   and advance to the next input byte.
> - `UNVIS_VALIDPUSH`: reserve one output byte, append `t`, then
>   immediately re-call `unvis` with the *same* byte `c` and dispatch the
>   new result identically. This is the push-back protocol; it can fire
>   at most once per input byte, because `unvis` is always left in
>   `S_GROUND` when it returns `UNVIS_VALIDPUSH` and `S_GROUND` never
>   returns it.
> - `UNVIS_NOCHAR` (or `0`): nothing is emitted; advance to the next
>   input byte. `unvis` never actually returns `0`; the case exists
>   defensively.
> - `UNVIS_SYNBAD`, or any other value: set `errno = EINVAL` and return
>   -1 at once. `dst` is left partially written and *not* NUL terminated,
>   and the bytes decoded so far are not recoverable through the return
>   value.
>
> Flush. After the loop, call `unvis(&t, c, &state, UNVIS_END)` exactly
> once; the byte argument is ignored under `UNVIS_END`. If and only if it
> returns `UNVIS_VALID`, reserve one output byte and append `t`. Every
> other return, `UNVIS_SYNBAD` included, is silently discarded. So input
> that stops in the middle of an escape is **not** an error: the
> incomplete sequence is dropped and everything decoded before it is
> kept. `"abc\"` decodes to `"abc"` (3 bytes); `"&am"` under
> `VIS_HTTP1866` decodes to the empty string (0 bytes); but a numeric
> escape cut short by end of input is completed, so `"abc\12"` decodes to
> `"abc"` + `0x0A` (4 bytes) and `"a\xA"` to `"a"` + `0x0A`.
>
> Termination and return. Reserve one final output byte, store `'\0'` at
> the cursor *without* advancing it, and return the cursor displacement
> as an `int`. The result is therefore the decoded length, one less than
> the bytes touched in `dst`. It is 0 for an empty `src` or for input
> that decodes to nothing. The cast to `int` truncates for outputs longer
> than `INT_MAX`.
>
> Bounds. "Reserve one output byte" means: if the remaining `dlen` is
> zero, set `errno = ENOSPC` and return -1; otherwise decrement `dlen`.
> The reservation happens before every byte stored, *including* the
> terminating NUL, so a successful call needs `dlen >= decoded length +
> 1` and `dlen == 0` always fails, even for an empty `src`. On `ENOSPC`
> the buffer is left partially written and unterminated.
>
> `errno` is written only on the two failure paths; it is not cleared on
> success.
>
> Byte width note. The C reads `src` through a plain `char`, so on a
> signed-`char` platform any input byte >= 0x80 reaches `unvis` as a
> negative `int`. `unvis` re-widens it to `unsigned char` for every
> classification except one (see the UB note in rule
> `[spec:libedit:sem:unvis.unvis-fn]`); a Rust port that passes bytes as
> `0..=255` must reproduce that single exception's observable outcome,
> not the sign extension itself.

> [spec:libedit:def:unvis.strunvis-fn]
> int strunvis(char *dst, const char *src)

> [spec:libedit:sem:unvis.strunvis-fn]
> Exactly `strnunvisx(dst, (size_t)~0, src, 0)` — an unbounded decode
> with flag `0`. `(size_t)~0` is `SIZE_MAX`, so the `ENOSPC` check of
> rule `[spec:libedit:sem:unvis.strnunvisx-fn]` can never fire and the
> caller alone is responsible for `dst` being large enough; the worst
> case is one output byte per input byte, plus the terminating NUL. Flag
> `0` decodes backslash escapes only, leaving `%`, `&` and `=` literal.
> Return value and error behaviour are those of rule
> `[spec:libedit:sem:unvis.strnunvisx-fn]`: decoded length, or -1 with
> `errno = EINVAL` on a malformed escape.
>
> This is the function `history.c` uses to read a history entry back
> after writing it with `strvis(ptr, str, VIS_WHITE)`, so this exact
> pairing — encode with `VIS_WHITE`, decode with flag `0` — *is* the
> on-disk history line format. It must round-trip byte for byte against
> history files written by the C library.

> [spec:libedit:def:unvis.strunvisx-fn]
> int strunvisx(char *dst, const char *src, int flag)

> [spec:libedit:sem:unvis.strunvisx-fn]
> Exactly `strnunvisx(dst, (size_t)~0, src, flag)` — the unbounded form
> of rule `[spec:libedit:sem:unvis.strnunvisx-fn]` with the caller's flags
> passed through unchanged. `(size_t)~0` is `SIZE_MAX`, so the `ENOSPC`
> check can never fire; the caller must size `dst` for the worst case,
> one output byte per input byte plus the terminating NUL. Decoding,
> return value (decoded length, not counting the NUL) and error
> behaviour (-1 with `errno = EINVAL` on a malformed escape) are those of
> rule `[spec:libedit:sem:unvis.strnunvisx-fn]`.

> [spec:libedit:def:unvis.unvis-fn]
> int unvis(char *cp, int c, int *astate, int flag)

> [spec:libedit:sem:unvis.unvis-fn]
> Incremental, one-byte-at-a-time decoder for the escaping produced by
> `vis`. Each call consumes at most one input byte `c` and reports
> whether a decoded byte is now available in `*cp`. It never touches more
> than the single byte at `*cp`; all cross-call state lives in `*astate`
> plus that partially-accumulated byte.
>
> ## Calling contract
>
> `*astate` is an `int` carrying two fields: the low 8 bits are the
> state-machine state; the top 8 bits (`(uint32_t)*astate >> 24`) are the
> index into the entity name being matched and are used only by
> `S_STRING`. Bits 8..23 are always zero. The caller MUST set `*astate`
> to `0` (state `S_GROUND`, index 0) before the first call of a sequence
> and MUST NOT otherwise write it.
>
> The caller MUST also leave `*cp` alone between calls of a sequence:
> `unvis` accumulates the in-progress byte there — octal/hex digits so
> far, the `0x80` meta bit, the current `nv` array index — and reads it
> back on the next call. Reading `*cp` is only meaningful after a
> `UNVIS_VALID` or `UNVIS_VALIDPUSH` return.
>
> Return codes:
>
> - `UNVIS_VALID` (1): `*cp` holds one decoded byte; the input byte was
>   consumed.
> - `UNVIS_VALIDPUSH` (2): `*cp` holds one decoded byte **and** the input
>   byte was *not* consumed. The caller must emit `*cp` and then call
>   `unvis` again with the same byte before advancing. The state is
>   always `S_GROUND` when this is returned, and `S_GROUND` never returns
>   it, so exactly one retry always suffices — a re-implementation may
>   assume the push-back depth is 1. This is how numeric escapes of
>   variable length (`\1`, `\12`, `\xA`) terminate: the byte that ended
>   the run has not been looked at yet.
> - `UNVIS_NOCHAR` (3): the byte was consumed, no output byte yet, the
>   sequence is still in progress.
> - `UNVIS_SYNBAD` (-1): malformed sequence. `*astate` is reset to
>   `S_GROUND` (the one exception is the `UNVIS_END` path below), the
>   contents of `*cp` are meaningless, and every byte the failed sequence
>   already consumed is lost.
> - `UNVIS_ERROR` (-2) is declared in `vis.h` but this implementation
>   never returns it.
>
> Canonical driver loop: for each input byte `b`, call
> `unvis(&t, b, &st, flag)`; on `UNVIS_VALID` emit `t`; on
> `UNVIS_VALIDPUSH` emit `t` and repeat with the same `b`; on
> `UNVIS_NOCHAR` continue; on `UNVIS_SYNBAD` fail. After the last byte,
> call once more with `flag = UNVIS_END`.
>
> ## Flags
>
> Only these bits of `flag` are read; all others are ignored.
>
> - `VIS_NOESCAPE` (0x0400) — do not treat `\` as an introducer; it
>   decodes to itself.
> - `VIS_HTTP1808` a.k.a. `VIS_HTTPSTYLE` (0x0080) — `%` introduces
>   `%HH`.
> - `VIS_MIMESTYLE` (0x0100) — `=` introduces `=HH` (uppercase hex) and
>   quoted-printable soft line breaks.
> - `VIS_HTTP1866` (0x0200) — `&` introduces `&name;` and `&#ddd;`.
> - `UNVIS_END` (= `_VIS_END`, 0x0800) — end-of-input flush, below.
>
> With `flag == 0` only backslash escapes are decoded; that is the mode
> the history file is read in.
>
> ## End-of-input flush
>
> If `flag & UNVIS_END` is set, the byte argument `c` is ignored
> completely and only the state matters:
>
> - `S_OCTAL2`, `S_OCTAL3`, `S_HEX2`: set state to `S_GROUND` and return
>   `UNVIS_VALID`. `*cp` holds the final byte of a numeric escape that
>   was terminated by end of input rather than by a non-digit. This is
>   why a string ending in `\12` or `\xA` still yields its byte.
> - `S_GROUND`: return `UNVIS_NOCHAR`; nothing was pending.
> - every other state: return `UNVIS_SYNBAD` and leave `*astate`
>   **unchanged** — the single place where `UNVIS_SYNBAD` does not reset
>   the state. A caller that ignores this return (as `strnunvisx` does)
>   must not reuse the state variable without re-zeroing it.
>
> ## States
>
> The state values are `S_GROUND` 0, `S_START` 1, `S_META` 2, `S_META1`
> 3, `S_CTRL` 4, `S_OCTAL2` 5, `S_OCTAL3` 6, `S_HEX` 7, `S_HEX1` 8,
> `S_HEX2` 9, `S_MIME1` 10, `S_MIME2` 11, `S_EATCRNL` 12, `S_AMP` 13,
> `S_NUMBER` 14, `S_STRING` 15. Every state transition below also clears
> the top-8-bit name index to 0 unless stated otherwise. Classification
> of the input byte (octal digit, hex digit, digit, uppercase, graphic)
> is done on the byte widened to `unsigned char`, except for the one
> `isgraph` call noted under Defects.
>
> **`S_GROUND` (0)** — no escape in progress. First store `0` into `*cp`
> (unconditionally; several states rely on starting from zero). Then, in
> this order:
>
> - `VIS_NOESCAPE` clear and byte is `\` (0x5C) → `S_START`,
>   `UNVIS_NOCHAR`.
> - `VIS_HTTP1808` set and byte is `%` → `S_HEX1`, `UNVIS_NOCHAR`.
> - `VIS_HTTP1866` set and byte is `&` → `S_AMP`, `UNVIS_NOCHAR`.
> - `VIS_MIMESTYLE` set and byte is `=` → `S_MIME1`, `UNVIS_NOCHAR`.
> - otherwise → store the byte into `*cp`, stay in `S_GROUND`, return
>   `UNVIS_VALID`. The four introducers are distinct bytes, so the test
>   order is not observable.
>
> **`S_START` (1)** — a `\` was seen. Dispatch on the byte:
>
> - `\` → emit 0x5C; `S_GROUND`, `UNVIS_VALID`.
> - `0`–`7` → `*cp = byte - '0'`; `S_OCTAL2`, `UNVIS_NOCHAR`. (`\0` is
>   just the octal path with value 0.)
> - `M` → `*cp = 0x80`; `S_META`, `UNVIS_NOCHAR`.
> - `^` → `S_CTRL`, `UNVIS_NOCHAR`, leaving `*cp` at the 0 that
>   `S_GROUND` stored.
> - `x` → `S_HEX`, `UNVIS_NOCHAR`.
> - the C-style letters, each emitting one byte and returning to
>   `S_GROUND` with `UNVIS_VALID`: `n` → 0x0A, `r` → 0x0D, `b` → 0x08,
>   `a` → 0x07, `v` → 0x0B, `t` → 0x09, `f` → 0x0C, `s` → 0x20 (space),
>   `E` → 0x1B (escape).
> - LF (0x0A) → hidden newline: `S_GROUND`, `UNVIS_NOCHAR`, no byte
>   produced. This unfolds the line continuations `vis` inserts.
> - `$` → hidden marker (the `vis(1) -l` end-of-line mark): `S_GROUND`,
>   `UNVIS_NOCHAR`, no byte produced.
> - any byte not matched above that is graphic (`isgraph`, i.e. ASCII
>   0x21..0x7E in the C locale) → emit it verbatim; `S_GROUND`,
>   `UNVIS_VALID`.
>   This is the path that decodes `\-` to `-`, `\%` to `%`, `\&` to `&`,
>   `\"` to `"`, and also `\q` to `q`: any escaped graphic character not
>   given a meaning above is simply unescaped, never an error. There is
>   no separate `\-` rule; `-` is meaningful only in `\M-`.
> - anything else — space, control characters, and (in the C locale)
>   every byte >= 0x80 → `UNVIS_SYNBAD`, state reset to `S_GROUND`. So
>   `"\ "` and `"\<0xC3>"` are errors.
>
> **`S_META` (2)** — `\M` seen, `*cp` == 0x80.
>
> - `-` → `S_META1`, `UNVIS_NOCHAR`.
> - `^` → `S_CTRL`, `UNVIS_NOCHAR`.
> - anything else → `UNVIS_SYNBAD`, `S_GROUND`.
>
> **`S_META1` (3)** — `\M-` seen. Accepts *any* byte with no validation:
> `*cp |= byte`; `S_GROUND`, `UNVIS_VALID`. So `\M-A` → 0xC1, `\M-<LF>`
> → 0x8A, and a byte that already has bit 7 set passes through unchanged
> (`\M-<0xC3>` → 0xC3).
>
> **`S_CTRL` (4)** — reached from `\^` (`*cp` == 0) or from `\M^` (`*cp`
> == 0x80). Accepts any byte with no validation:
>
> - `?` → `*cp |= 0x7F`
> - otherwise → `*cp |= byte & 0x1F`
>
> then `S_GROUND`, `UNVIS_VALID`. So `\^A` → 0x01, `\^?` → 0x7F, `\M^A`
> → 0x81, `\M^?` → 0xFF, and `\^<LF>` → 0x0A.
>
> **`S_OCTAL2` (5)** — one octal digit is accumulated in `*cp`.
>
> - octal digit (`0`–`7`) → `*cp = (*cp << 3) + (byte - '0')`;
>   `S_OCTAL3`, `UNVIS_NOCHAR`.
> - anything else → `S_GROUND`, `UNVIS_VALIDPUSH`: the one-digit value in
>   `*cp` is the decoded byte and the byte just passed must be fed again.
>
> **`S_OCTAL3` (6)** — two octal digits accumulated. The state is set to
> `S_GROUND` on every path first.
>
> - octal digit, with the overflow guard applied *before* accumulating:
>   if `*cp & 0x20` (i.e. the two-digit value is >= 32, so a third digit
>   would exceed 255) → `UNVIS_SYNBAD`. The two digits already decoded
>   are discarded and the third digit is consumed, so `\400` through
>   `\777` are hard errors rather than truncations. Otherwise
>   `*cp = (*cp << 3) + (byte - '0')`, `UNVIS_VALID`. `\377` = 0xFF is
>   the largest accepted value.
> - anything else → `UNVIS_VALIDPUSH`: the two-digit value is the decoded
>   byte and the byte is pushed back.
>
> An octal escape therefore consumes at most three digits; accumulation
> ends at the third digit, at the first non-octal byte (push-back), or at
> end of input (`UNVIS_END` flush).
>
> **`S_HEX` (7)** — `\x` seen; the first hex digit is mandatory here. If
> the byte is not a hex digit → `UNVIS_SYNBAD`, `S_GROUND` (so `"\xz"` is
> an error). Otherwise fall into `S_HEX1` and apply its rule to the same
> byte.
>
> **`S_HEX1` (8)** — entered directly by `%` under `VIS_HTTP1808`, or by
> fallthrough from `S_HEX`.
>
> - hex digit (`0`–`9`, `A`–`F`, `a`–`f`, either case) → `*cp` = its
>   value 0..15; `S_HEX2`, `UNVIS_NOCHAR`.
> - anything else → `S_GROUND`, `UNVIS_VALIDPUSH`. Only reachable from
>   `%`, and see the defect note: `*cp` is still the 0 that `S_GROUND`
>   stored, so this emits a NUL byte, not the `%`.
>
> **`S_HEX2` (9)** — one hex digit accumulated. The state is set to
> `S_GROUND` first on every path.
>
> - hex digit → `*cp = (*cp << 4) | value`; `UNVIS_VALID`.
> - anything else → `UNVIS_VALIDPUSH`: the single digit is the decoded
>   byte and the byte is pushed back.
>
> A hex escape thus consumes at most two digits and accepts one: `\xAz`
> decodes to 0x0A followed by `z`.
>
> **`S_MIME1` (10)** — `=` seen under `VIS_MIMESTYLE` (RFC 2045
> quoted-printable).
>
> - LF or CR → soft line break: `S_EATCRNL`, `UNVIS_NOCHAR`.
> - `0`–`9` or `A`–`F` — uppercase hex only → `*cp` = its value;
>   `S_MIME2`, `UNVIS_NOCHAR`.
> - anything else, lowercase `a`–`f` included → `UNVIS_SYNBAD`,
>   `S_GROUND`. `"=4a"` is an error where `"=4A"` decodes to `J`.
>
> **`S_MIME2` (11)** — one uppercase hex digit accumulated.
>
> - `0`–`9` or `A`–`F` → `*cp = (*cp << 4) | value`; `S_GROUND`,
>   `UNVIS_VALID`.
> - anything else → `UNVIS_SYNBAD`, `S_GROUND`. Both digits are
>   mandatory; there is no push-back path out of a MIME escape.
>
> **`S_EATCRNL` (12)** — swallowing the CR/LF of a soft line break.
>
> - CR or LF → `UNVIS_NOCHAR`, stay in `S_EATCRNL` (so CRLF and bare
>   CR/LF and any run of them are all absorbed).
> - `=` → `S_MIME1`, `UNVIS_NOCHAR` (a second escape may follow the
>   break directly).
> - anything else → store the byte into `*cp`; `S_GROUND`,
>   `UNVIS_VALID`. See defects: this emits the byte verbatim without
>   re-running the `S_GROUND` dispatch.
>
> **`S_AMP` (13)** — `&` seen under `VIS_HTTP1866`. Store `0` into `*cp`,
> then:
>
> - `#` → `S_NUMBER`, `UNVIS_NOCHAR`.
> - anything else → set the state to `S_STRING` with name index 0 and
>   apply the `S_STRING` rule to *this same byte* immediately (a
>   fallthrough inside one call, not a new call).
>
> **`S_STRING` (15)** — matching a named entity against the `nv` table
> of rule `[spec:libedit:def:unvis.nv]`: 100 entries sorted in ASCII
> order by name, each name at most 6 characters, stored in a `char[7]`
> and NUL-padded, so reading `name[k]` for `k` past the name's length
> yields 0 and index 6 is always 0. Let `ia = *cp` (index of the first
> table entry that matched the prefix so far), `is` = the top 8 bits of
> `*astate` (number of name characters matched so far), and
> `lc = is == 0 ? 0 : nv[ia].name[is - 1]` (the previously matched
> character). Let `uc` be the input byte with `;` replaced by 0 — a NUL
> is what terminates the name, and the padding makes it match one past
> the name's end. Then scan `ia` upward from its current value:
>
> - if `is != 0` and `nv[ia].name[is - 1] != lc`, stop with
>   `UNVIS_SYNBAD`, `S_GROUND`;
> - if `nv[ia].name[is] == uc`, stop successfully at this `ia`;
> - if `ia` runs off the end of the table, `UNVIS_SYNBAD`, `S_GROUND`.
>
> On success: if `uc != 0` the name is unfinished — store `ia` into `*cp`
> and set `*astate` to `((is + 1) << 24) | S_STRING`, return
> `UNVIS_NOCHAR`. If `uc == 0` the name is complete — store
> `nv[ia].value` into `*cp`, go to `S_GROUND`, return `UNVIS_VALID`. The
> name index can never exceed 6 (it would require a non-NUL byte at
> `name[6]`), so the array read is in bounds only because every name is
> at most 6 characters — a port that widens the table must widen the
> bound with it.
>
> **`S_NUMBER` (14)** — `&#` seen; `*cp` accumulates the decimal value,
> starting at 0.
>
> - `;` → return `UNVIS_VALID` with the accumulated value in `*cp`, but
>   **without** resetting the state (see defects).
> - not an ASCII digit → `UNVIS_SYNBAD`, `S_GROUND`.
> - digit, with the overflow guard applied before accumulating: if
>   `(*cp & 0xFF) * 10 > 255 - (byte - '0')` → `UNVIS_SYNBAD`,
>   `S_GROUND`. Otherwise `*cp = (*cp & 0xFF) * 10 + (byte - '0')`,
>   `UNVIS_NOCHAR`. Any number of digits may be written (leading zeros
>   included) as long as the running value stays within 0..255; `&#256;`
>   and above are errors.
>
> **Any other state value** — reachable only from an uninitialised or
> corrupted `*astate` → `UNVIS_SYNBAD`, `S_GROUND`.
>
> ## Defects, UB and quirks to reproduce
>
> These are real, observable through the public ABI, and frozen under
> `[dec:libedit:no-c-ffi]`: a drop-in `unvis` must reproduce the
> observable result even where the C is relying on UB or is plainly
> wrong. Defects 2–5 need the HTTP or MIME flags and so cannot affect the
> history file; 1, 6 and 7 are reachable with `flag == 0`, though none of
> them can be triggered by output `vis` itself produced, only by a
> corrupt or hand-written history line.
>
> 1. **UB: `isgraph` on a possibly negative `int`.** The `S_START`
>    default arm classifies the raw `int c` rather than the
>    `unsigned char` widening used everywhere else. A caller passing a
>    signed `char` therefore hands `isgraph` a negative value, which is
>    undefined behaviour for anything but `EOF`. glibc tolerates it and,
>    in the C locale, answers the same as for the unsigned value, so the
>    observable result is that `\` followed by a byte >= 0x80 yields
>    `UNVIS_SYNBAD`. A Rust port should implement C-locale
>    `0x21..=0x7E`, matching that outcome; the behaviour under other
>    locales is not something the C itself defines.
> 2. **`S_NUMBER` never leaves its state.** The `;` arm returns
>    `UNVIS_VALID` without setting `*astate` back to `S_GROUND`, so the
>    decoder stays in `S_NUMBER` after a completed `&#ddd;`. Observable
>    consequences, all verified against the C: `"&#65;X"` fails with
>    `EINVAL`; `"&#65;;"` decodes to `"AA"` — the byte is emitted twice;
>    `"&#65;6;"` fails with `EINVAL` (65*10 overflows the guard). Only a
>    `&#ddd;` at the very end of the input decodes cleanly, because the
>    `UNVIS_END` flush from `S_NUMBER` returns `UNVIS_SYNBAD` and
>    `strnunvisx` discards that.
> 3. **`%` followed by a non-hex byte emits NUL.** `S_HEX1` reaches its
>    `UNVIS_VALIDPUSH` arm with `*cp` still holding the 0 that
>    `S_GROUND` stored, so the `%` is not restored: `"a%qb"` under
>    `VIS_HTTP1808` decodes to `0x61 0x00 0x71 0x62`, four bytes.
> 4. **The `S_STRING` prefix test is one character deep.** It compares
>    only `name[is-1]` against the previously matched character and
>    relies on the table's sorted order rather than re-checking the whole
>    prefix. An exhaustive walk of the machine shows all 100 real names
>    decode to their correct values, but 15 non-names are also accepted,
>    each yielding the value of whichever entry the forward scan landed
>    on: `&Aril;` → 196, `&Arilde;` → 195, `&Atil;` → 196, `&Iumlde;` →
>    209, `&Ogrash;` → 216, `&Otil;` → 214, `&aril;` → 228, `&arilde;` →
>    227, `&atil;` → 228, `&macro;` → 181, `&macrot;` → 183, `&microt;`
>    → 183, `&otil;` → 246, `&plund;` → 163, `&rect;` → 167. The defect
>    only makes the decoder lenient, never wrong about a real name.
> 5. **A soft line break suppresses the next escape.** The `S_EATCRNL`
>    default arm writes the byte straight out instead of re-entering the
>    `S_GROUND` dispatch, so a `\`, `%`, `&` or `=` in that position is
>    emitted literally: `"a=<CR><LF>\n"` under `VIS_MIMESTYLE` decodes to
>    `0x61 0x5C 0x6E`, with the backslash escape not honoured.
> 6. **`UNVIS_END` leaves a bad state in place.** As noted above, the
>    `UNVIS_SYNBAD` return from the flush path does not reset `*astate`,
>    unlike every other `UNVIS_SYNBAD`.
> 7. `S_HEX2` writes the bare constant `S_GROUND` into `*astate` rather
>    than the pack-and-store used elsewhere. Since `S_GROUND` is 0 and
>    the pack of index 0 with state 0 is also 0, this is harmless — noted
>    only so a port does not read significance into the asymmetry.

