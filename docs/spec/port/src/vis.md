# src/vis.c, src/vis.h

> [spec:libedit:def:vis.do-hvis-fn]
> static wchar_t * do_hvis(wchar_t *dst, wint_t c, int flags, wint_t nextc, const wchar_t *extra)

> [spec:libedit:sem:vis.do-hvis-fn]
> Per-character encoder for HTTP / RFC 1808 URL style. Chosen by
> `[spec:libedit:sem:vis.getvisfun-fn]` when `VIS_HTTPSTYLE` (0x0080, the
> same bit as `VIS_HTTP1808`) is set. `c` is one **whole wide character**,
> not a byte. Writes wide characters at `dst` and returns the advanced
> cursor. It never returns NULL and never bounds-checks — the caller
> guarantees room.
>
> Classify `c` as *URL-safe* if any of:
>
> - `iswalnum(c)` — locale-dependent, see below;
> - `c` is one of the RFC 1808 "safe" set `$ - _ . +`;
> - `c` is one of the RFC 1808 "extra" set `! * ' ( ) ,`.
>
> That is eleven literal characters plus whatever the current locale calls
> alphanumeric. Note the list is checked *literally*: `%` is not in it, so
> `%` self-escapes to `%25`.
>
> - **URL-safe** → tail-call `[spec:libedit:sem:vis.do-svis-fn]` with the
>   same `dst`, `c`, `flags`, `nextc` and `extra`, and return whatever it
>   returns. `do_svis` may still escape the character — most importantly
>   if it is in `extra` — so "safe" means "not percent-escaped", not
>   "emitted literally". Example: `strsvis(dst, "a", VIS_HTTPSTYLE, "a")`
>   yields `\141`, not `a`.
> - **Otherwise** → emit exactly three wide characters and return
>   `dst + 3`: `%`, then the hex digit for `(c >> 4) & 0xf`, then the hex
>   digit for `c & 0xf`, both indexed out of `"0123456789abcdef"` —
>   **lowercase** hex. (The decoder,
>   `[spec:libedit:sem:unvis.unvis-fn]` state `S_HEX1`, accepts either
>   case, so this is one-way asymmetric but not a mismatch.)
>
> Defects and locale dependence to reproduce:
>
> 1. **The percent form is lossy above U+00FF.** Only the low 8 bits of
>    `c` reach the hex digits; the multi-byte splitting that
>    `[spec:libedit:sem:vis.do-svis-fn]` performs is not applied here, and
>    there is no `%uXXXX` or UTF-8 form. Verified: in a UTF-8 locale
>    `strvis(dst, "\xcd\xb8", VIS_HTTPSTYLE)` (U+0378, unassigned, so not
>    graphic and not alnum) yields `%78` — the character is silently
>    destroyed and decodes back as ASCII `x`. In the C locale the same
>    input is two separate bytes and yields `%cd%b8`.
> 2. **`iswalnum` is not gated by `VIS_NOLOCALE`.** Unlike the graphic
>    test, which goes through the `ISGRAPH` macro described in
>    `[spec:libedit:sem:vis.iscgraph-fn]`, this call always uses the
>    current `LC_CTYPE`. In a UTF-8 locale accented letters are alnum and
>    therefore reach `do_svis`, which passes them through literally, so
>    HTTP-style output can contain raw non-ASCII bytes. Under
>    `VIS_NOLOCALE` the input has already been split into individual
>    bytes, so `iswalnum` is being asked about byte values.
>
> `nextc` is passed through untouched and is used only by `do_svis` /
> `do_mbyte`; `do_hvis` itself never looks at it.

> [spec:libedit:def:vis.do-mbyte-fn]
> static wchar_t * do_mbyte(wchar_t *dst, wint_t c, int flags, wint_t nextc, int iswextra)

> [spec:libedit:sem:vis.do-mbyte-fn]
> Emits the escape for **one byte** `c` (always in 0..255; it is one byte
> of a wide character, extracted by
> `[spec:libedit:sem:vis.do-svis-fn]`). Writes wide characters at `dst`
> and returns the advanced cursor; never returns NULL, never bounds-checks.
> This is the only place any escape text is produced, so it defines the
> entire output alphabet apart from `%XX` (`do_hvis`) and `=XX`
> (`do_mvis`).
>
> `iswextra` is the caller's already-computed "this character is in the
> extra set" boolean for the *whole* wide character, so every byte of a
> multi-byte character inherits it. `nextc` is the whole next wide
> character of the input (or 0 at end of input) and is consulted in
> exactly one place.
>
> Three stages, in this order. The first one that emits and returns wins.
>
> ## Stage 1 — `VIS_CSTYLE` (0x0002), skipped entirely if the flag is clear
>
> Dispatch on `c`:
>
> - 0x0A LF → `\n` (backslash, letter n), return.
> - 0x0D CR → `\r`, return.
> - 0x08 BS → `\b`, return.
> - 0x07 BEL → `\a`, return.
> - 0x0B VT → `\v`, return.
> - 0x09 TAB → `\t`, return.
> - 0x0C FF → `\f`, return.
> - 0x20 SPACE → `\s`, return.
> - 0x00 NUL → emit `\` then `0`; **then, if and only if `nextc` is an
>   octal digit, emit two more `0`s**, giving `\000`. Return. This is the
>   whole of the "next character" lookahead: `\0` would otherwise run
>   together with a following literal `0`–`7` and be misread as a longer
>   octal escape. The digit test is `iswoctal`, defined as
>   `(u_char)nextc >= '0' && (u_char)nextc <= '7'` — **it truncates
>   `nextc` to its low byte first**, so a wide character at, say, U+0130
>   (low byte 0x30) counts as an octal digit and forces the 3-digit form
>   even though the character that will actually follow in the output is
>   not a digit. Verified: `strvisx(dst, "\0\xc4\xb0", 3, VIS_CSTYLE)` in a
>   UTF-8 locale gives `\000` followed by the literal U+0130. The
>   over-padded form still decodes to NUL, so this is observable only in
>   the byte count / return value.
> - `n`, `r`, `b`, `a`, `v`, `t`, `f`, `s`, `0`, `M`, `^`, `$` (the
>   letters that already have a meaning after a backslash, plus `$` which
>   `vis(1) -l` uses as a hidden end-of-line marker) → **fall through to
>   stage 2**. They cannot use the `\c` form because `\n` must mean
>   newline, not the letter n.
> - anything else → if `ISGRAPH(flags, c)` (see
>   `[spec:libedit:sem:vis.iscgraph-fn]`) **and** `c` is not an octal
>   digit `0`–`7`, emit `\` followed by `c` itself and return. Otherwise
>   fall through to stage 2.
>
> Consequences worth stating explicitly: because stage 1 runs before the
> `iswextra` test, a character in the extra set that is graphic and not a
> reserved letter gets the short `\c` form rather than octal — verified,
> `strsvis(dst, "qQ", VIS_CSTYLE, "qQ")` gives `\q\Q`, and `VIS_GLOB |
> VIS_CSTYLE` turns `*?[#` into `\*\?\[\#` where `VIS_GLOB` alone gives
> `\052\077\133\043`. Conversely `strsvis(dst, "n", VIS_CSTYLE, "n")`
> gives `\156`, because `n` is reserved.
>
> ## Stage 2 — octal
>
> If `iswextra` **or** `(c & 0177) == 0x20` **or** `flags & VIS_OCTAL`
> (0x0001):
>
> emit exactly four wide characters — `\`, then three octal digits of the
> low byte of `c`, always three, always zero-padded, most significant
> first: `'0' + ((c >> 6) & 3)`, `'0' + ((c >> 3) & 7)`, `'0' + (c & 7)`.
> Range `\000` to `\377`. Return.
>
> The middle test `(c & 0177) == 0x20` is a masked comparison, so it is
> true for byte 0x20 **and for byte 0xA0**. Byte 0xA0 therefore always
> comes out as `\240` even with none of `VIS_OCTAL`, `VIS_SP` or an extra
> set in play, while its neighbour 0xA1 comes out as `\M-!`. Verified:
> `strvis(dst, "\xa0\xa1", VIS_NOLOCALE)` gives `\240\M-!`. This
> asymmetry is real and must be reproduced.
>
> Note also that the leading backslash here is **not** suppressed by
> `VIS_NOSLASH`; see stage 3.
>
> ## Stage 3 — meta / control
>
> Otherwise:
>
> 1. If `VIS_NOSLASH` (0x0040) is **clear**, emit `\`. If it is set, emit
>    nothing here — this is the only place `VIS_NOSLASH` has any effect.
> 2. If bit 7 of `c` is set (`c & 0200`), clear it (`c &= 0177`) and emit
>    `M`.
> 3. If `iswcntrl(c)` on the now-7-bit value: emit `^`, then `?` if `c ==
>    0177`, else the character `c + '@'` (i.e. `c + 0x40`, mapping 0x01 →
>    `A` … 0x1A → `Z`, 0x1B → `[`, 0x1F → `_`, 0x00 → `@`).
> 4. Else: emit `-`, then `c`.
>
> So the forms are `\^X`, `\^?`, `\M^X`, `\M^?`, `\-x` and `\M-x`, or the
> same five without the leading backslash under `VIS_NOSLASH`. Verified
> examples: 0x01 → `\^A`; 0x7F → `\^?`; 0x85 → `\M^E`; 0xA1 → `\M-!`;
> 0x78 reached as a byte of a non-graphic wide character → `\-x`; under
> `VIS_NOSLASH`, 0x01 → `^A` and 0x7F → `^?`.
>
> `iswcntrl` is locale-dependent, but by the time control reaches here the
> value is 0..127, and in every locale glibc supports that agrees with the
> C locale (0x00–0x1F and 0x7F).
>
> The `\-x` branch is only reachable for a byte that is not a control
> character, not graphic under the effective locale, and not caught by the
> space mask — in practice only as a byte of a decomposed multi-byte wide
> character, since `do_svis` would have passed a graphic byte through
> literally. Do not treat it as dead code: `strvis(dst, "\xcd\xb8", 0)` in
> a UTF-8 locale yields `\^C\-x`.
>
> ## Round-trip defect
>
> The stage-1 default arm can emit `\x`, because `x` is not in the
> reserved-letter list, but `\x` is the decoder's hex introducer
> (`[spec:libedit:sem:unvis.unvis-fn]`, state `S_HEX`). `\x` at end of
> input is **silently dropped** by `strunvis` (the `UNVIS_END` flush
> returns `UNVIS_SYNBAD`, which the driver discards) and `\x` followed by
> a hex digit decodes as that hex value. Verified: `strsvis(dst, "x",
> VIS_CSTYLE, "x")` produces `\x`, and `strunvis` of `\x` returns 0 bytes.
> Reachable without an explicit extra set too — `VIS_CSTYLE` on U+0378 in
> a UTF-8 locale gives `\^C\x`. It cannot affect the history file, which
> does not use `VIS_CSTYLE`.

> [spec:libedit:def:vis.do-mvis-fn]
> static wchar_t * do_mvis(wchar_t *dst, wint_t c, int flags, wint_t nextc, const wchar_t *extra)

> [spec:libedit:sem:vis.do-mvis-fn]
> Per-character encoder for RFC 2045 quoted-printable MIME. Chosen by
> `[spec:libedit:sem:vis.getvisfun-fn]` when `VIS_MIMESTYLE` (0x0100) is
> set and `VIS_HTTPSTYLE` is clear. `c` is one whole wide character.
> Writes wide characters at `dst`, returns the advanced cursor, never
> returns NULL, never bounds-checks. It implements only the character
> escaping: **no line-length limiting and no CRLF handling**, so it does
> not by itself produce conformant quoted-printable.
>
> Escape `c` as `=XX` if and only if `c != 0x0A` **and** at least one of:
>
> - **A (trailing whitespace):** `iswspace(c)` and `nextc` is CR (0x0D) or
>   LF (0x0A). This is the only use of the lookahead — quoted-printable
>   forbids a literal space or tab at end of line.
> - **B (out of range):** `!iswspace(c)` and (`c < 33` or `c == 61` or
>   `c > 126`). The source writes the middle test as
>   `(c > 60 && c < 62)`, which is just `c == 61`, i.e. `=` self-escapes.
> - **C (specific characters):** `c` is one of the twelve characters
>   `#`, `$`, `@`, `[`, `\`, `]`, `^`, `` ` ``, `{`, `|`, `}`, `~`.
>   Condition C is tested regardless of whether `c` is whitespace.
>
> The escape is exactly three wide characters: `=`, then the hex digit for
> `(c >> 4) & 0xf`, then the hex digit for `c & 0xf`, indexed out of
> `"0123456789ABCDEF"` — **uppercase**, which the decoder's `S_MIME1` /
> `S_MIME2` states require (they reject lowercase).
>
> Otherwise tail-call `[spec:libedit:sem:vis.do-svis-fn]` with the same
> arguments and return its result. As with `do_hvis`, that is not the same
> as emitting the character literally: `do_svis` still applies the extra
> set and the graphic test. So `VIS_MIMESTYLE | VIS_NL` escapes a newline
> as `\012`, not as a literal newline.
>
> Verified behaviour (identical in the C and UTF-8 locales for ASCII):
>
> - the twelve specials plus `=`, in the order `= # $ @ [ ] ^` backtick
>   `{ | } ~`, encode to `=3D=23=24=40=5B=5D=5E=60=7B=7C=7D=7E`
> - `"a \r\nb"` → `a=20=0D` + literal LF + `b` — the space is escaped
>   because CR follows, the CR is escaped because LF follows (CR is
>   whitespace), the LF passes to `do_svis` and is emitted literally.
> - `"a \nb"` → `a=20` + LF + `b`; `"a\t\nb"` → `a=09` + LF + `b`.
> - `"a b"` → `a b`: a space not at end of line is left alone.
>
> Defects and locale dependence:
>
> 1. **The `=XX` form is lossy above U+00FF**, exactly as in
>    `[spec:libedit:sem:vis.do-hvis-fn]`: only the low byte is encoded and
>    the multi-byte split of `do_svis` is not applied. In a UTF-8 locale
>    U+20AC (€) is encoded as `=AC`; in the C locale the same input is
>    three bytes and gives `=E2=82=AC`.
> 2. **`iswspace` is not gated by `VIS_NOLOCALE`**, so which characters
>    take branch A/B depends on `LC_CTYPE`.
> 3. Condition C is implemented as a `wcschr` over the twelve-character
>    string, and `wcschr` returns the address of the string's terminator
>    when asked for `c == 0`, so condition C is technically true for NUL.
>    Harmless: condition B already covers NUL.

> [spec:libedit:def:vis.do-svis-fn]
> static wchar_t * do_svis(wchar_t *dst, wint_t c, int flags, wint_t nextc, const wchar_t *extra)

> [spec:libedit:sem:vis.do-svis-fn]
> The core of the encoder: decides whether one wide character passes
> through untouched or is escaped, and if escaped, splits it into bytes
> and hands each byte to `[spec:libedit:sem:vis.do-mbyte-fn]`. It is both
> the default encoder (chosen when neither `VIS_HTTPSTYLE` nor
> `VIS_MIMESTYLE` is set) and the fallback that `do_hvis` and `do_mvis`
> delegate to. Writes wide characters at `dst`, returns the advanced
> cursor, never returns NULL, never bounds-checks.
>
> ## Step 1 — extra-set membership
>
> `iswextra = wcschr(extra, c) != NULL`, where `extra` is the wide string
> built by `[spec:libedit:sem:vis.makeextralist-fn]`.
>
> **`c == 0` is always `iswextra`**, because `wcschr` matches the
> terminating NUL of any string. That is load-bearing, not incidental: it
> guarantees NUL is never emitted literally into the wide staging buffer,
> which is what lets the caller's output loop measure the result with
> `wcslen`. A Rust port must special-case NUL as "always escaped", or the
> byte-length accounting in
> `[spec:libedit:sem:vis.istrsenvisx-fn]` breaks.
>
> ## Step 2 — literal pass-through
>
> If `!iswextra` **and** any of:
>
> - `ISGRAPH(flags, c)` — `iswgraph(c)` normally, `isgraph(c)` when
>   `VIS_NOLOCALE` (0x4000) is set; see
>   `[spec:libedit:sem:vis.iscgraph-fn]`;
> - `c` is space (0x20), tab (0x09) or newline (0x0A);
> - `VIS_SAFE` (0x0020) is set and `c` is backspace (0x08), BEL (0x07) or
>   CR (0x0D);
>
> then store `c` unchanged at `dst` and return `dst + 1`.
>
> Notes on this test:
>
> - The whitespace clause is why the default behaviour leaves space, tab
>   and newline alone, and why `VIS_SP` / `VIS_TAB` / `VIS_NL` work by
>   putting those characters into `extra` rather than by clearing a bit
>   here — `iswextra` is checked first and wins.
> - `VIS_SAFE` does exactly one thing: it adds BS, BEL and CR to the
>   pass-through set. It does not reduce escaping of anything else, and it
>   is also overridden by `extra`. Verified: `strvis(dst, "\b\a\r\v", 0)`
>   gives `\^H\^G\^M\^K`; with `VIS_SAFE` it gives literal BS, BEL, CR
>   then `\^K`; `strsvis(dst, "\r", VIS_SAFE, "\r")` gives `\015`.
> - The graphic test is where the locale enters the encoding. In a UTF-8
>   locale `iswgraph(U+20AC)` is true, so € is written literally; in the C
>   locale the same input arrives as three separate bytes, none of them
>   graphic, and comes out as `\M-b\M^B\M-,`. See the warning in
>   `[spec:libedit:sem:vis.strvis-fn]`.
>
> ## Step 3 — byte decomposition
>
> Otherwise decompose `c` into big-endian bytes and call `do_mbyte` once
> per byte, most significant first, skipping leading zero bytes:
>
> ```
> wmsk = 0
> for i in 7, 6, 5, 4, 3, 2, 1, 0:
>     shft = i * 8
>     bmsk = 0xff << shft
>     wmsk |= bmsk
>     if (c & wmsk) != 0 or i == 0:
>         dst = do_mbyte(dst, (c & bmsk) >> shft, flags, nextc, iswextra)
> ```
>
> Equivalently: let `k` be the index of the highest non-zero byte of `c`
> (0 if `c == 0`); emit bytes `k, k-1, …, 0`. Every call gets the same
> `flags`, the same `nextc` (the whole next wide character, undecomposed)
> and the same `iswextra`.
>
> The bytes are the **code point's own bytes**, not its multibyte
> encoding. In a UTF-8 locale a non-graphic U+0378 decomposes to 0x03,
> 0x78 and yields `\^C\-x` — not the UTF-8 bytes 0xCD 0xB8. `wint_t` is 32
> bits, so bytes 4..7 are always zero in practice and the loop is really a
> 1-to-3-byte split for real Unicode; the 8-byte form exists only to
> mirror the identical loop in the caller's output path. A negative
> `wchar_t` would sign-extend into the top bytes and produce eight calls,
> but `mbrtowc` never yields one.
>
> Because `iswextra` is computed once for the whole character, *all* bytes
> of a multi-byte character take the octal branch of `do_mbyte` if the
> character is in `extra`.

> [spec:libedit:def:vis.getvisfun-fn]
> static visfun_t getvisfun(int flags)

> [spec:libedit:sem:vis.getvisfun-fn]
> Selects the per-character encoder, checked in this fixed order:
>
> 1. `flags & VIS_HTTPSTYLE` (0x0080, same bit as `VIS_HTTP1808`) →
>    `do_hvis`, rule `[spec:libedit:sem:vis.do-hvis-fn]`.
> 2. `flags & VIS_MIMESTYLE` (0x0100) → `do_mvis`, rule
>    `[spec:libedit:sem:vis.do-mvis-fn]`.
> 3. otherwise → `do_svis`, rule `[spec:libedit:sem:vis.do-svis-fn]`.
>
> The order is observable: with both bits set, HTTP wins. Verified,
> `strvis(dst, "a b", VIS_HTTPSTYLE|VIS_MIMESTYLE)` gives `a%20b`.
>
> `VIS_HTTP1866` (0x0200, the `&name;` / `&#ddd;` form) is **not handled
> here or anywhere else in the encoder**. The decoder in `unvis.c`
> understands it, but nothing in libedit can produce it: setting the bit
> has no effect at all on `vis` output. Verified,
> `strvis(dst, "a<b", VIS_HTTP1866)` gives `a<b`. A port must not invent
> an encoder for it.
>
> Pure function of `flags`; no state, no allocation, cannot fail.

> [spec:libedit:def:vis.iscgraph-fn]
> static int iscgraph(int c)

> [spec:libedit:sem:vis.iscgraph-fn]
> "Is `c` a graphic character **in the C locale**" — the intended
> locale-independent classifier behind `VIS_NOLOCALE` (0x4000). It is
> reached only through the macro
> `ISGRAPH(flags, c) = (flags & VIS_NOLOCALE) ? iscgraph(c) : iswgraph(c)`,
> which is the single graphic test used by
> `[spec:libedit:sem:vis.do-svis-fn]` and by the `VIS_CSTYLE` default arm
> of `[spec:libedit:sem:vis.do-mbyte-fn]`.
>
> `iscgraph` has three forms in the source, selected at compile time:
>
> 1. On platforms defining `LC_C_LOCALE` (a NetBSD extension) it is the
>    macro `isgraph_l(c, LC_C_LOCALE)` — a genuine C-locale test.
> 2. Otherwise, and this is what libedit actually compiles on Linux and
>    every other POSIX host, it is the macro `isgraph(c)` — a test in the
>    **current** locale, with the comment "Keep it simple for now, no
>    locale stuff".
> 3. The function this rule's signature names is guarded by
>    `#ifdef notyet` and is never compiled. Its body: save the current
>    `LC_CTYPE` by calling `setlocale(LC_CTYPE, "C")` (which both switches
>    to C and returns the previous setting), call `isgraph(c)`, restore the
>    previous setting with a second `setlocale` if the first returned
>    non-NULL, and return the saved result. That is not thread-safe and
>    not reentrant, which is presumably why it is disabled.
>
> **The port should implement form 1's semantics: graphic in the C locale,
> i.e. exactly the byte range 0x21..0x7E, false for everything else.**
> That is what the flag promises and what forms 1 and 3 deliver. Form 2 is
> a known incompleteness, not a behaviour to preserve; under
> `VIS_NOLOCALE` the input has already been reduced to individual bytes,
> and on glibc `isgraph` on a byte in any UTF-8 locale already answers
> 0x21..0x7E, so forms 1 and 2 agree there. They diverge only in
> single-byte non-ASCII locales such as ISO-8859-1, where form 2 would
> call 0xA1..0xFF graphic and leave them unescaped — which is precisely
> what `VIS_NOLOCALE` exists to prevent.
>
> Argument convention: `isgraph` takes an `int` that must be
> representable as `unsigned char` or equal `EOF`. Every call site passes
> a value already reduced to 0..255, so there is no negative-argument UB
> here (unlike `[spec:libedit:sem:unvis.unvis-fn]`).

> [spec:libedit:def:vis.istrsenvisx-fn]
> static int istrsenvisx(char **mbdstp, size_t *dlen, const char *mbsrc, size_t mblength, int flags, const char *mbextra, int *cerr_ptr)

> [spec:libedit:sem:vis.istrsenvisx-fn]
> The single engine behind every public `vis` entry point. Encodes
> `mblength` **bytes** starting at `mbsrc` into the buffer at `*mbdstp`,
> allocating that buffer if `*mbdstp` is NULL. Returns the number of bytes
> written, not counting the terminating NUL, or -1 with `errno` set.
>
> Pipeline: multibyte input → wide characters → per-character encoding in
> wide characters → multibyte output. The round trip through `wchar_t` is
> deliberate; it is what lets the graphic test be locale-aware, so that
> (as the source comment puts it) French text is not displayed as
> `M-foo`.
>
> ## Parameters
>
> - `mbdstp` — must not be NULL. If `*mbdstp` is NULL a destination buffer
>   is allocated (see Allocation) and stored there; otherwise `*mbdstp` is
>   the caller's buffer and is never rewritten.
> - `dlen` — NULL for the unbounded variants; otherwise `*dlen` is the
>   destination capacity in bytes, *including* the terminating NUL, and is
>   read once. It is never written back.
> - `mbsrc` — may be NULL only if `mblength == 0`.
> - `mblength` — byte count, not character count. NULs inside the range
>   are data, not terminators.
> - `mbextra` — must not be NULL (`strlen` is called on it); `""` means no
>   extra characters.
> - `cerr_ptr` — optional multibyte-conversion-error flag; see Conversion
>   error latch.
>
> ## Step 1 — lookahead fudge
>
> `mbslength = mblength`; **if `mbslength == 1`, increment it to 2.** The
> single-character entry points need the character after the one they
> encode, and they pass a 2-byte stack array. Any other caller that passes
> `mblength == 1` gets a one-byte **out-of-bounds read** of `mbsrc[1]`,
> and that byte can change the output. Verified: `strvisx(dst, buf, 1,
> VIS_CSTYLE)` on a NUL byte gives `\000` when `buf[1]` is `'7'` and `\0`
> when it is `'x'`. This is UB in the C; a port should read the extra byte
> only where the caller actually supplied one, and must otherwise treat
> the lookahead as 0.
>
> ## Step 2 — overflow guard and allocation
>
> If `mbslength > (SIZE_MAX - 1) / 16`, set `errno = ENOMEM` and return -1.
>
> Then allocate, all zeroed:
>
> - `psrc`: `mbslength + 1` `wchar_t` — the decoded input. Failure returns
>   -1 immediately with `*mbdstp` untouched.
> - `pdst`: `16 * mbslength + 1` `wchar_t` — the wide staging buffer. The
>   16× factor is the *only* bound protecting this buffer; nothing checks
>   it during encoding.
> - if `*mbdstp == NULL`: `16 * mbslength + 1` bytes, stored into
>   `*mbdstp`.
>
> Failure of the second or third goes to the cleanup path: free everything
> allocated (including the destination buffer) and return -1.
>
> ## Step 3 — conversion error latch
>
> `cerr` starts as 1 if `VIS_NOLOCALE` is set (forcing byte-at-a-time
> handling), else as `*cerr_ptr` if `cerr_ptr` is non-NULL, else 0. Once
> set to 1 it is **never cleared**, and it governs both the input and the
> output loop.
>
> At the end, `*cerr_ptr` is written back **only if `VIS_NOLOCALE` is
> set** — which is exactly the case in which it was *not* read. So the
> in/out flag is useless as designed: `strsenvisx` / `strenvisx` callers
> can seed it (without `VIS_NOLOCALE`) or read it (with `VIS_NOLOCALE`),
> never both. Reproduce the asymmetry.
>
> ## Step 4 — input loop
>
> While `mbslength > 0`:
>
> - if `cerr` is 0, `clen = mbrtowc(src, mbsrc, min(mbslength,
>   MB_LEN_MAX), &mbstate)`, taken as a signed value;
> - if `cerr` is 1, or `clen < 0` (both `(size_t)-1` EILSEQ and
>   `(size_t)-2` incomplete), then store `(wchar_t)(unsigned char)*mbsrc`
>   into `*src`, set `clen = 1`, and **set `cerr = 1`**;
> - if `clen == 0` (an embedded NUL, which `mbrtowc` reports as 0 after
>   storing L'\0'), set `clen = 1`;
> - advance `src` by one wide character, `mbsrc` by `clen` bytes, and
>   decrease `mbslength` by `clen`.
>
> The loop is byte-count driven and does not stop at NUL, so a block
> containing NULs is fully processed. `mbstate` is not reset after an
> error, but `cerr` has latched so `mbrtowc` is not called again.
>
> Then `len = src - psrc` (wide-character count), `src = psrc`, and **if
> `mblength < len`, `len = mblength`** — this is what discards the
> lookahead character in the single-character case.
>
> ## Step 5 — extra list
>
> `extra = makeextralist(flags, mbextra)`, rule
> `[spec:libedit:sem:vis.makeextralist-fn]`. If it returns NULL
> (allocation failure):
>
> - if `dlen` is non-NULL and `*dlen == 0`, set `errno = ENOSPC` and go to
>   the cleanup path (return -1);
> - otherwise write `'\0'` to the front of the destination, set the return
>   value to **0**, and go to the cleanup path.
>
> **Bug:** the cleanup path frees the destination buffer when this
> function allocated it. So `stravis` on this path returns 0 (success) with
> `*mbdstp` pointing at freed memory that was written to just before the
> free. A port must return an error, or not free, but must not hand back a
> dangling pointer.
>
> ## Step 6 — encoding loop
>
> `f = getvisfun(flags)`. Remember `start = dst`. For each of the `len`
> wide characters: `dst = f(dst, c, flags, *src, extra)` where `c` is the
> current character and `*src` is the *next* one — which for the last
> character is the zeroed slot at the end of `psrc`, i.e. `L'\0'`. (The
> source writes `len >= 1 ? *src : L'\0'`; inside a `len > 0` loop the
> condition is always true, so the alternative is dead code.) The
> subsequent `if (dst == NULL) { errno = ENOSPC; … }` is likewise dead —
> no encoder ever returns NULL.
>
> **Nothing bounds-checks `pdst` here.** Correctness rests entirely on 16
> wide characters per input byte being enough, which it is: the widest
> per-character expansion is 4 output characters per input byte (`\ooo`).
>
> Finally store `L'\0'` at `dst`.
>
> ## Step 7 — output loop
>
> `len = wcslen(start)` — safe only because no encoder ever emits a bare
> `L'\0'` (see `[spec:libedit:sem:vis.do-svis-fn]` step 1).
>
> Compute the byte budget `maxolen`:
>
> - if `dlen` is non-NULL: `maxolen = *dlen`, and if it is 0 set
>   `errno = ENOSPC` and fail;
> - otherwise: fail with `ENOSPC` if `len > (SIZE_MAX - 1) / MB_LEN_MAX`,
>   else `maxolen = len * MB_LEN_MAX + 1`. **This is a computed bound, not
>   the caller's actual buffer size** — the unbounded variants never
>   protect the caller at all.
>
> Then for each of the `len` wide characters, with `olen` the bytes
> written so far:
>
> - if `cerr` is 0: convert with `wcrtomb`. If `maxolen - olen >
>   MB_CUR_MAX` the conversion is done straight into the destination (room
>   is guaranteed); otherwise it goes to a scratch `char[MB_LEN_MAX]`, and
>   then if `clen > 0` and `olen + clen >= maxolen` the call fails with
>   `ENOSPC`, else the bytes are copied over.
> - if `cerr` is 1, or `wcrtomb` failed: write the wide character's own
>   bytes big-endian, skipping leading zero bytes, using the same
>   8-iteration mask loop as `[spec:libedit:sem:vis.do-svis-fn]` step 3;
>   before each byte, fail with `ENOSPC` if `olen + clen + 1 >= maxolen`.
>   Then set `cerr = 1`.
> - advance the destination pointer and `olen` by `clen`.
>
> Store `'\0'` at the end and return `(int)olen`, truncating for outputs
> beyond `INT_MAX`.
>
> Both bounds tests reserve the terminating NUL, so a successful call
> needs `*dlen >= return value + 1`. Verified: `strnvis(dst, d, "a b",
> VIS_WHITE)` returns 6 for `d >= 7` and -1/`ENOSPC` for `d <= 6`. On
> `ENOSPC` the destination is left partially written and **not** NUL
> terminated.
>
> **Bug (data corruption, reachable from the history path):** `cerr`
> latches across the whole string, and the output loop tests it once per
> character rather than per conversion. If a valid multibyte prefix is
> followed by an invalid byte anywhere later, the *earlier* characters are
> written out as raw code-point bytes instead of their multibyte encoding.
> Verified in a UTF-8 locale: `strvis(dst, "\xe2\x82\xac\xffz",
> VIS_WHITE)` — U+20AC then byte 0xFF then `z` — produces the 4 bytes
> `20 AC FF 7A`, silently mangling the €. The same input without the 0xFF
> produces `E2 82 AC 7A`. A port should decide deliberately whether to
> reproduce this; it cannot round-trip through `strunvis`.
>
> ## Cleanup path
>
> Free the extra list, the wide staging buffer, the wide input buffer and
> — if this function allocated it — the destination buffer, then return
> the error value (-1, except the `makeextralist` case above which returns
> 0). `*mbdstp` is **not** reset to NULL, so an allocating caller is left
> holding a dangling pointer on every failure after the destination was
> allocated.

> [spec:libedit:def:vis.istrsenvisxl-fn]
> static int istrsenvisxl(char **mbdstp, size_t *dlen, const char *mbsrc, int flags, const char *mbextra, int *cerr_ptr)

> [spec:libedit:sem:vis.istrsenvisxl-fn]
> NUL-terminated-input adapter over
> `[spec:libedit:sem:vis.istrsenvisx-fn]`: calls it with
> `mblength = mbsrc != NULL ? strlen(mbsrc) : 0`, passing every other
> argument through unchanged, and returns its result.
>
> Two consequences. A NULL `mbsrc` is accepted and treated as the empty
> string — verified, `strvis(dst, NULL, 0)` returns 0 and writes `""`
> (subject to `dlen` being at least 1 when bounded). And a source
> containing an embedded NUL is truncated there; callers that need to
> encode a block with NULs in it must use the `x` variants, which take an
> explicit length.
>
> Note the interaction with the lookahead fudge in
> `[spec:libedit:sem:vis.istrsenvisx-fn]` step 1: for a one-character
> string, `strlen` is 1, so the engine reads `mbsrc[1]` — which here is
> the caller's own NUL terminator, in bounds and not an octal digit. So
> the `l` variants are safe from that defect, unlike `strvisx` with
> `len == 1`.

> [spec:libedit:def:vis.makeextralist-fn]
> static wchar_t * makeextralist(int flags, const char *src)

> [spec:libedit:sem:vis.makeextralist-fn]
> Builds the NUL-terminated wide string of "characters to escape even
> though they would otherwise pass through", from the caller's `src` plus
> the character-set flags. Returns a freshly allocated buffer the caller
> must free, or NULL on allocation failure — the only failure mode.
>
> 1. `len = strlen(src)`. Allocate and zero `len + 30` `wchar_t`
>    (`MAXEXTRAS` is 30). Return NULL if that fails.
> 2. Convert `src` to wide characters. If `VIS_NOLOCALE` (0x4000) is set,
>    **or** `mbsrtowcs(dst, &src, len, &state)` from a zeroed state
>    returns `(size_t)-1`, fall back to a 1:1 byte map:
>    `dst[i] = (wchar_t)(unsigned char)src[i]` for `i < len`, and the
>    append cursor lands at `dst + len`. Otherwise the append cursor lands
>    at `dst + wcslen(dst)`. Passing `len` (a byte count) as the wide
>    character limit is safe because a multibyte string of `len` bytes can
>    never decode to more than `len` characters, and the buffer is zeroed
>    so termination is guaranteed even when the limit is hit exactly.
> 3. Append, in this order:
>    - if `VIS_GLOB` (0x1000): `*`, `?`, `[`, `#` — 4 characters;
>    - if `VIS_SHELL` (0x2000): `'`, `` ` ``, `"`, `;`, `&`, `<`, `>`,
>      `(`, `)`, `|`, `{`, `}`, `]`, `\`, `$`, `!`, `^`, `~` — 18
>      characters, in that order. Note it contains `]` but not `[`, and
>      contains `\` and `"`;
>    - if `VIS_SP` (0x0004): space;
>    - if `VIS_TAB` (0x0008): tab;
>    - if `VIS_NL` (0x0010): newline;
>    - if `VIS_DQ` (0x8000): `"`;
>    - if `VIS_NOSLASH` (0x0040) is **clear**: `\`.
> 4. Store a terminating `L'\0'` and return the buffer.
>
> `VIS_WHITE` is the union `VIS_SP | VIS_TAB | VIS_NL` = 0x001C, so it
> contributes space, tab and newline. `VIS_META` is
> `VIS_WHITE | VIS_GLOB | VIS_SHELL`.
>
> Duplicates are possible (`VIS_SHELL` already supplies `"` and `\`) and
> are harmless; the result is used only as a membership set, via `wcschr`,
> by `[spec:libedit:sem:vis.do-svis-fn]`.
>
> The 30-slot headroom is exactly sufficient and must not be reduced: the
> worst case is 4 + 18 + 1 + 1 + 1 + 1 + 1 = 27 appended characters plus
> the terminator, 28 of 30. A port that adds a character class to this
> function must grow the constant.
>
> The default `\` membership is the reason a backslash in the input is
> escaped as `\134` (or `\\` under `VIS_CSTYLE`) rather than being emitted
> raw, and the reason `VIS_NOSLASH` output can contain literal
> backslashes. Verified: `strvis(dst, "a\\b", VIS_NOSLASH)` gives `a\b`.

> [spec:libedit:def:vis.nvis-fn]
> char * nvis(char *mbdst, size_t dlen, int c, int flags, int nextc)

> [spec:libedit:sem:vis.nvis-fn]
> Bounded single-character encode: exactly
> `[spec:libedit:sem:vis.snvis-fn]` with an empty extra string. Builds the
> 2-byte array `{ (char)c, (char)nextc }`, calls
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mblength = 1`,
> `*dlen = dlen`, `mbextra = ""` and no `cerr_ptr`; returns NULL if that
> returns negative (`errno` is `ENOSPC` when `dlen` is too small),
> otherwise `mbdst + return value` — a pointer to the terminating NUL,
> i.e. the place to write the next character. `mbdst` itself is never
> reassigned because it is non-NULL on entry.
>
> `dlen` must be at least the encoded length plus 1; `dlen == 0` always
> fails. All the caveats of `[spec:libedit:sem:vis.vis-fn]` apply
> unchanged — the `int` to `char` truncation, `nextc` being lookahead only
> and never encoded, and the multibyte-recombination quirk that can make
> this write **more than one character's worth** of output.

> [spec:libedit:def:vis.snvis-fn]
> char * snvis(char *mbdst, size_t dlen, int c, int flags, int nextc, const char *mbextra)

> [spec:libedit:sem:vis.snvis-fn]
> Bounded single-character encode with a caller-supplied extra set. Copies
> `c` and `nextc` into a 2-byte array `cc = { (char)c, (char)nextc }`,
> then calls `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mbdstp = &mbdst`,
> `dlen = &dlen`, `mbsrc = cc`, `mblength = 1`, the caller's `flags` and
> `mbextra`, and `cerr_ptr = NULL`.
>
> Returns NULL if the engine returns negative, leaving `errno` set
> (`ENOSPC` for an insufficient `dlen`, `ENOMEM` for allocation failure);
> otherwise returns `mbdst + n` where `n` is the byte count written — a
> pointer to the terminating NUL. The engine writes the NUL, so the
> returned pointer always dereferences to `'\0'` on success. `dlen` counts
> that NUL, so it must be at least `n + 1`.
>
> Semantics of the encoding itself: `mblength == 1` makes the engine bump
> its internal byte count to 2 so that `cc[1]` is available as the
> lookahead, then clamp the encoded character count back to 1. So `nextc`
> influences the output (the `\0` vs `\000` choice of
> `[spec:libedit:sem:vis.do-mbyte-fn]`, and the trailing-whitespace test
> of `[spec:libedit:sem:vis.do-mvis-fn]`) but is not itself encoded —
> *except* under the quirk below.
>
> Quirks:
>
> 1. **`int` to `char` truncation.** Only the low 8 bits of `c` and
>    `nextc` survive; there is no way to pass a wide character in.
> 2. **Multibyte recombination.** The two bytes are fed to `mbrtowc`
>    together. In a multibyte locale, if `c` is a lead byte and `nextc`
>    completes the sequence, they decode to *one* wide character, and that
>    character — both bytes of it — is what gets encoded. Verified:
>    `vis(dst, 0xC3, VIS_WHITE, 0xA9)` in a UTF-8 locale writes the two
>    bytes `C3 A9` (é, graphic, passed through) and returns `dst + 2`,
>    whereas in the C locale it writes `\M-C` and returns `dst + 4`. A
>    single-character API that can emit the *next* character too is
>    surprising; it is nonetheless the behaviour.
> 3. If `c` is a lead byte that `nextc` does *not* complete, `mbrtowc`
>    reports an incomplete sequence, the engine latches its
>    conversion-error flag and falls back to byte-at-a-time, and only
>    `c`'s byte is encoded — with `nextc`'s byte as the lookahead.

> [spec:libedit:def:vis.stravis-fn]
> int stravis(char **mbdstp, const char *mbsrc, int flags)

> [spec:libedit:sem:vis.stravis-fn]
> Allocating form of `[spec:libedit:sem:vis.strvis-fn]`. Sets `*mbdstp` to
> NULL, then calls `[spec:libedit:sem:vis.istrsenvisxl-fn]` with
> `dlen = NULL`, `mbextra = ""` and `cerr_ptr = NULL`, and returns its
> result. Because `*mbdstp` is NULL the engine allocates the destination —
> zeroed, `16 * strlen(mbsrc) + 1` bytes — stores it in `*mbdstp`, and the
> **caller must `free` it**.
>
> Returns the encoded length in bytes, not counting the NUL, with
> `*mbdstp` holding a NUL-terminated result; or -1 with `errno` set.
> Encoding is identical to `strvis` in every respect, including the flag
> handling and the locale dependence — see
> `[spec:libedit:sem:vis.strvis-fn]`.
>
> Failure states, which differ from every other variant and are worth
> handling explicitly in a port:
>
> - If the first internal allocation fails, -1 is returned with `*mbdstp`
>   still NULL. Safe.
> - If a later allocation fails, or the extra-list allocation fails, the
>   destination buffer is freed but `*mbdstp` is **left pointing at it**.
>   The caller sees a dangling non-NULL pointer.
> - The extra-list failure path is worse still: it returns **0**, not -1,
>   after writing a NUL byte into the buffer it then frees. See
>   `[spec:libedit:sem:vis.istrsenvisx-fn]` step 5.
>
> A Rust port should return a fresh owned `String`/`Vec` and simply not
> reproduce the dangling-pointer states; they are unobservable to a
> correct caller except as a crash.

> [spec:libedit:def:vis.strenvisx-fn]
> int strenvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags, int *cerr_ptr)

> [spec:libedit:sem:vis.strenvisx-fn]
> The most explicit `vis` variant: bounded, counted, with the
> multibyte-conversion-error flag exposed. Exactly
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mbdstp = &mbdst`,
> `dlen = &dlen`, the caller's `mbsrc`, `len`, `flags` and `cerr_ptr`, and
> `mbextra = ""` (no extra characters beyond what the flags add).
>
> Encodes exactly `len` bytes, embedded NULs included, into at most `dlen`
> bytes counting the terminating NUL. Returns the byte count written, not
> counting the NUL, or -1 with `errno` set to `ENOSPC` (destination too
> small, or `dlen == 0`) or `ENOMEM`. On `ENOSPC` the destination is
> partially written and unterminated.
>
> `cerr_ptr` is the only distinguishing feature and it does not work as
> its name suggests, per
> `[spec:libedit:sem:vis.istrsenvisx-fn]` step 3: a non-NULL `*cerr_ptr`
> is *read* as the initial "treat input as raw bytes" flag only when
> `VIS_NOLOCALE` is **clear**, and is *written* with the final state of
> that flag only when `VIS_NOLOCALE` is **set**. So it is an input in one
> mode and an output in the other, never both. Setting `*cerr_ptr` to a
> non-zero value before the call, without `VIS_NOLOCALE`, forces
> byte-at-a-time input handling for the whole string — which is the only
> practical use.
>
> Everything else, including the `len == 1` out-of-bounds lookahead read
> flagged in `[spec:libedit:sem:vis.strvisx-fn]`, applies unchanged.

> [spec:libedit:def:vis.strnunvis-fn]
> int strnunvis(char *, size_t, const char *)

> [spec:libedit:sem:vis.strnunvis-fn]
> `vis.h` prototype for the bounded decoder; implemented in `unvis.c`.
> Full semantics are rule `[spec:libedit:sem:unvis.strnunvis-fn]`.
>
> ABI contract as declared here: `strnunvis(char *dst, size_t dlen, const
> char *src)` decodes the NUL-terminated `src` into `dst`, writing at most
> `dlen` bytes including the terminating NUL, and returns the decoded
> length not counting that NUL, or -1 with `errno` set to `EINVAL` for a
> malformed escape or `ENOSPC` for an undersized buffer. Equivalent to
> `strnunvisx(dst, dlen, src, 0)`, so backslash is the only escape
> introducer recognised — `%`, `&` and `=` are literal data. This is the
> bounded counterpart of the decoder used for history lines.
>
> Note the argument order: `(dst, dlen, src)`. Some BSDs historically
> declared the `n` forms as `(dst, src, dlen, …)`, and the two are not
> distinguishable by the compiler. The order in this header is
> authoritative for the port's C ABI.

> [spec:libedit:def:vis.strnunvisx-fn]
> int strnunvisx(char *, size_t, const char *, int)

> [spec:libedit:sem:vis.strnunvisx-fn]
> `vis.h` prototype for the bounded, flagged decoder; implemented in
> `unvis.c`. Full semantics are rule
> `[spec:libedit:sem:unvis.strnunvisx-fn]` — this is the function every
> other decode entry point in the library funnels into.
>
> ABI contract as declared here: `strnunvisx(char *dst, size_t dlen, const
> char *src, int flag)` decodes the NUL-terminated `src` into `dst` by
> driving `[spec:libedit:sem:unvis.unvis-fn]` one byte at a time under
> `flag`, writes at most `dlen` bytes including the terminating NUL, NUL
> terminates on success only, and returns the decoded length not counting
> the NUL. On failure it returns -1 with `errno` set to `EINVAL` (a
> malformed escape) or `ENOSPC` (`dlen` smaller than decoded length + 1;
> `dlen == 0` always fails), leaving `dst` partially written and
> unterminated.
>
> The `flag` argument takes the `VIS_*` bits that select escape
> introducers — `VIS_NOESCAPE`, `VIS_HTTP1808`/`VIS_HTTPSTYLE`,
> `VIS_MIMESTYLE`, `VIS_HTTP1866` — not the encoder's character-selection
> bits, which are meaningless here. An input that ends mid-escape is not
> an error; the partial sequence is discarded, except for numeric escapes,
> which are completed.
>
> Argument order is `(dst, dlen, src, flag)`; see the note in
> `[spec:libedit:sem:vis.strnunvis-fn]`.

> [spec:libedit:def:vis.strnvis-fn]
> int strnvis(char *mbdst, size_t dlen, const char *mbsrc, int flags)

> [spec:libedit:sem:vis.strnvis-fn]
> Bounded form of `[spec:libedit:sem:vis.strvis-fn]`: exactly
> `[spec:libedit:sem:vis.istrsenvisxl-fn]` with `mbdstp = &mbdst`,
> `dlen = &dlen`, `mbextra = ""` and `cerr_ptr = NULL`.
>
> Encodes the NUL-terminated `mbsrc` under `flags` — the encoding is
> identical in every respect to `strvis`, see that rule for the full flag
> semantics — into at most `dlen` bytes **including** the terminating NUL.
>
> Return value and failure, which is where this differs from `strvis` and
> from other BSDs:
>
> - Success: the number of bytes written, not counting the NUL, and
>   `mbdst` is NUL terminated. Requires `dlen >= result + 1`.
> - Overflow: **-1 with `errno = ENOSPC`**, and `mbdst` is left partially
>   written and **not NUL terminated**. It does **not** truncate, and it
>   does **not** return the length that would have been needed. Code
>   ported from OpenBSD/FreeBSD, whose `strnvis` truncates and returns the
>   would-be length like `snprintf`, will misbehave against this one.
> - `dlen == 0` always fails with `ENOSPC`, even for an empty source.
>
> Verified: `strnvis(dst, d, "a b", VIS_WHITE)` returns 6 with `dst` =
> `a\040b` for every `d >= 7`, and -1/`ENOSPC` for every `d <= 6`, leaving
> a partial prefix such as `a\040` in the buffer.
>
> Argument order is `(dst, dlen, src, flags)`. NetBSD historically used
> `(dst, src, dlen, flags)`; both compile, so a port exporting this
> symbol must match the order in this header.

> [spec:libedit:def:vis.strnvisx-fn]
> int strnvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags)

> [spec:libedit:sem:vis.strnvisx-fn]
> Bounded, counted encode: exactly
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mbdstp = &mbdst`,
> `dlen = &dlen`, the caller's `mbsrc`, `len` and `flags`,
> `mbextra = ""`, and `cerr_ptr = NULL`. It is
> `[spec:libedit:sem:vis.strvisx-fn]` plus a destination bound, or
> equivalently `[spec:libedit:sem:vis.strnvis-fn]` with an explicit source
> length instead of NUL termination.
>
> Encodes exactly `len` bytes from `mbsrc`, embedded NULs included (a NUL
> encodes as `\000`, or `\0`/`\000` under `VIS_CSTYLE` depending on the
> following character), into at most `dlen` bytes counting the terminating
> NUL. Flag semantics are those of `[spec:libedit:sem:vis.strvis-fn]`.
>
> Returns the number of bytes written, not counting the NUL; or -1 with
> `errno = ENOSPC` if `dlen` is less than that plus one (`dlen == 0`
> always fails), or `ENOMEM` on allocation failure. It never truncates,
> and on failure `mbdst` is partially written and not NUL terminated.
>
> The `len == 1` case reads `mbsrc[1]`, one byte past what the caller
> declared — see `[spec:libedit:sem:vis.istrsenvisx-fn]` step 1.

> [spec:libedit:def:vis.strsenvisx-fn]
> int strsenvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags, const char *mbextra, int *cerr_ptr)

> [spec:libedit:sem:vis.strsenvisx-fn]
> The full-surface entry point: every parameter the engine accepts, passed
> straight through. Exactly `[spec:libedit:sem:vis.istrsenvisx-fn]` with
> `mbdstp = &mbdst` and `dlen = &dlen`; `mbsrc`, `len`, `flags`,
> `mbextra` and `cerr_ptr` are the caller's, unmodified. Every other
> public function in this file is this one with arguments fixed.
>
> Encodes exactly `len` bytes (embedded NULs included) into at most `dlen`
> bytes counting the terminating NUL, escaping additionally every
> character in the NUL-terminated multibyte string `mbextra` — which must
> not be NULL; pass `""` for none. `mbextra` is decoded in the current
> locale, or byte-by-byte under `VIS_NOLOCALE` or on a decoding failure;
> see `[spec:libedit:sem:vis.makeextralist-fn]`.
>
> Returns the byte count written, not counting the NUL, or -1 with `errno`
> set to `ENOSPC` or `ENOMEM`. Flag semantics are those of
> `[spec:libedit:sem:vis.strvis-fn]`, and the `cerr_ptr` in/out asymmetry
> is that of `[spec:libedit:sem:vis.strenvisx-fn]`.
>
> Remember that characters in `mbextra` are escaped in **octal**
> (`\ooo`) by default, because `[spec:libedit:sem:vis.do-mbyte-fn]` routes
> the extra set to its octal branch; only with `VIS_CSTYLE` do graphic
> non-reserved extras get the compact `\c` form. Verified:
> `strsvis(dst, "qQ", 0, "qQ")` gives `\161\121`, while
> `strsvis(dst, "qQ", VIS_CSTYLE, "qQ")` gives `\q\Q`.

> [spec:libedit:def:vis.strsnvis-fn]
> int strsnvis(char *mbdst, size_t dlen, const char *mbsrc, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsnvis-fn]
> Bounded encode of a NUL-terminated string with a caller-supplied extra
> set: exactly `[spec:libedit:sem:vis.istrsenvisxl-fn]` with
> `mbdstp = &mbdst`, `dlen = &dlen`, the caller's `mbsrc`, `flags` and
> `mbextra`, and `cerr_ptr = NULL`.
>
> It is `[spec:libedit:sem:vis.strsvis-fn]` with a destination bound, or
> `[spec:libedit:sem:vis.strnvis-fn]` with an extra set. Source length is
> `strlen(mbsrc)`, so an embedded NUL truncates the input; a NULL `mbsrc`
> is treated as empty.
>
> Returns the number of bytes written, not counting the terminating NUL,
> and requires `dlen >= result + 1`. On overflow returns -1 with
> `errno = ENOSPC`, without truncating and without NUL terminating; `dlen
> == 0` always fails. `ENOMEM` on allocation failure. Encoding, flags and
> extra-set handling are those of
> `[spec:libedit:sem:vis.strsenvisx-fn]`.

> [spec:libedit:def:vis.strsnvisx-fn]
> int strsnvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsnvisx-fn]
> Bounded, counted encode with a caller-supplied extra set: exactly
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mbdstp = &mbdst`,
> `dlen = &dlen`, the caller's `mbsrc`, `len`, `flags` and `mbextra`, and
> `cerr_ptr = NULL`. Equivalently,
> `[spec:libedit:sem:vis.strsenvisx-fn]` with the conversion-error flag
> fixed at NULL.
>
> Encodes exactly `len` bytes, embedded NULs included, into at most `dlen`
> bytes counting the terminating NUL. Returns the byte count written, not
> counting the NUL, or -1 with `errno = ENOSPC` (needs
> `dlen >= result + 1`; `dlen == 0` always fails) or `ENOMEM`. Never
> truncates; on failure the destination is partial and unterminated.
>
> Flags, extra-set handling and the `len == 1` lookahead over-read are as
> described in `[spec:libedit:sem:vis.strsenvisx-fn]` and
> `[spec:libedit:sem:vis.istrsenvisx-fn]`.

> [spec:libedit:def:vis.strsvis-fn]
> int strsvis(char *mbdst, const char *mbsrc, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsvis-fn]
> Unbounded encode of a NUL-terminated string with a caller-supplied extra
> set: exactly `[spec:libedit:sem:vis.istrsenvisxl-fn]` with
> `mbdstp = &mbdst`, `dlen = NULL`, the caller's `mbsrc`, `flags` and
> `mbextra`, and `cerr_ptr = NULL`. It is
> `[spec:libedit:sem:vis.strvis-fn]` plus the extra set.
>
> Source length is `strlen(mbsrc)`; embedded NULs truncate; NULL `mbsrc`
> means empty. `mbextra` must not be NULL — `strlen` is called on it
> unconditionally — and is itself decoded in the current locale (or
> byte-wise under `VIS_NOLOCALE`).
>
> **No bound is applied to `mbdst`.** The caller must supply at least
> `4 * strlen(mbsrc) + 1` bytes; four bytes per input byte (`\ooo`) is the
> worst case for every flag combination this file can produce. Returns the
> number of bytes written, not counting the terminating NUL, which is
> always written on success. Returns -1 with `errno = ENOMEM` only on
> allocation failure.
>
> Encoding, flag semantics and extra-set behaviour are those of
> `[spec:libedit:sem:vis.strsenvisx-fn]` and, for the flags themselves,
> `[spec:libedit:sem:vis.strvis-fn]`. In particular: extras are escaped in
> octal unless `VIS_CSTYLE` is set, and `VIS_GLOB` / `VIS_SHELL` /
> `VIS_DQ` are just shorthands for appending fixed characters to
> `mbextra`.

> [spec:libedit:def:vis.strsvisx-fn]
> int strsvisx(char *mbdst, const char *mbsrc, size_t len, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsvisx-fn]
> Unbounded, counted encode with a caller-supplied extra set: exactly
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mbdstp = &mbdst`,
> `dlen = NULL`, the caller's `mbsrc`, `len`, `flags` and `mbextra`, and
> `cerr_ptr = NULL`.
>
> Encodes exactly `len` bytes from `mbsrc` — embedded NULs are data and
> encode as `\000` (or `\0`/`\000` under `VIS_CSTYLE`, depending on the
> following character) — so this is the variant for escaping a binary
> block. Returns the number of bytes written, not counting the terminating
> NUL, which is always written on success; -1 with `errno = ENOMEM` on
> allocation failure.
>
> **No bound is applied to `mbdst`**: the caller must supply at least
> `4 * len + 1` bytes. Flags and extra-set semantics are those of
> `[spec:libedit:sem:vis.strsenvisx-fn]`.
>
> `len == 1` reads `mbsrc[1]`, one byte beyond the caller's buffer, and
> that byte can change the output — see
> `[spec:libedit:sem:vis.istrsenvisx-fn]` step 1. On NetBSD this symbol is
> also weakly aliased as `_strsvisx`; the alias carries no separate
> behaviour and is out of scope for the port under
> `[dec:libedit:posix-only-scope]`.

> [spec:libedit:def:vis.strunvis-fn]
> int strunvis(char *, const char *)

> [spec:libedit:sem:vis.strunvis-fn]
> `vis.h` prototype for the plain unbounded decoder; implemented in
> `unvis.c`. Full semantics are rule
> `[spec:libedit:sem:unvis.strunvis-fn]`.
>
> ABI contract as declared here: `strunvis(char *dst, const char *src)`
> decodes the NUL-terminated `src` into `dst`, NUL terminating it, and
> returns the decoded length not counting the NUL, or -1 with
> `errno = EINVAL` on a malformed escape. Equivalent to
> `strnunvisx(dst, SIZE_MAX, src, 0)`: no bound is enforced, so the caller
> must size `dst` for the worst case of one output byte per input byte
> plus the NUL, and only backslash escapes are recognised.
>
> **This is the read side of the history file.** `history.c` writes each
> entry with `strvis(ptr, str, VIS_WHITE)` and reads it back with
> `strunvis`, so this pairing is the on-disk format that
> `[dec:libedit:no-c-ffi]` freezes. It must round-trip byte for byte
> against files written by the C library — see
> `[spec:libedit:sem:vis.strvis-fn]` for the encoder half and the exact
> escape table.

> [spec:libedit:def:vis.strunvisx-fn]
> int strunvisx(char *, const char *, int)

> [spec:libedit:sem:vis.strunvisx-fn]
> `vis.h` prototype for the unbounded, flagged decoder; implemented in
> `unvis.c`. Full semantics are rule
> `[spec:libedit:sem:unvis.strunvisx-fn]`.
>
> ABI contract as declared here: `strunvisx(char *dst, const char *src,
> int flag)` decodes the NUL-terminated `src` into `dst` under `flag`, NUL
> terminates it, and returns the decoded length not counting the NUL, or
> -1 with `errno = EINVAL` on a malformed escape. Equivalent to
> `strnunvisx(dst, SIZE_MAX, src, flag)`, so no bound is enforced and the
> caller must size `dst` for one output byte per input byte plus the NUL;
> `ENOSPC` can never be returned.
>
> `flag` takes the escape-introducer bits — `VIS_NOESCAPE`,
> `VIS_HTTP1808`/`VIS_HTTPSTYLE`, `VIS_MIMESTYLE`, `VIS_HTTP1866` — and
> ignores the encoder's character-selection bits. Note the encoder can
> produce `%XX` and `=XX` but never `&name;`/`&#ddd;`, so
> `VIS_HTTP1866` decoding has no counterpart in this library; see
> `[spec:libedit:sem:vis.getvisfun-fn]`.

> [spec:libedit:def:vis.strvis-fn]
> int strvis(char *mbdst, const char *mbsrc, int flags)

> [spec:libedit:sem:vis.strvis-fn]
> Encodes the NUL-terminated string `mbsrc` into `mbdst` under `flags`,
> with no extra characters and no destination bound. Exactly
> `[spec:libedit:sem:vis.istrsenvisxl-fn]` with `mbdstp = &mbdst`,
> `dlen = NULL`, `mbextra = ""` and `cerr_ptr = NULL`.
>
> Returns the number of bytes written, not counting the terminating NUL,
> which is always written on success. Returns -1 with `errno = ENOMEM`
> only on internal allocation failure (there is no `ENOSPC` path — nothing
> knows how big `mbdst` is). **The caller must provide at least
> `4 * strlen(mbsrc) + 1` bytes**; four output bytes per input byte
> (`\ooo`) is the worst case across every flag combination. A NULL
> `mbsrc` is accepted and produces an empty result with return 0; an
> embedded NUL truncates the input (use `strvisx` for blocks).
>
> ## Flag summary
>
> Full detail is in `[spec:libedit:sem:vis.do-svis-fn]`,
> `[spec:libedit:sem:vis.do-mbyte-fn]` and
> `[spec:libedit:sem:vis.makeextralist-fn]`; in outline:
>
> - **Encoder choice** — `VIS_HTTPSTYLE` (0x0080) gives `%xx` lowercase,
>   `VIS_MIMESTYLE` (0x0100) gives `=XX` uppercase, HTTP winning if both
>   are set; otherwise the backslash forms. `VIS_HTTP1866` (0x0200) does
>   nothing on this side.
> - **What gets escaped** — by default, every character that is not
>   graphic in the current locale, except space, tab and newline.
>   `VIS_SP` / `VIS_TAB` / `VIS_NL` (and their union `VIS_WHITE`, 0x001C)
>   add those three back; `VIS_DQ` (0x8000) adds `"`; `VIS_GLOB` (0x1000)
>   adds `*?[#`; `VIS_SHELL` (0x2000) adds ``'`";&<>()|{}]\$!^~``;
>   `VIS_SAFE` (0x0020) *removes* BS, BEL and CR from the escaped set. NUL
>   is always escaped.
> - **Escape form** — `VIS_CSTYLE` (0x0002) prefers the named C escapes
>   `\n \r \b \a \v \t \f \s \0` and the compact `\c` form for graphic
>   extras; `VIS_OCTAL` (0x0001) forces `\ooo` for everything;
>   `VIS_NOSLASH` (0x0040) drops the leading backslash from the `M`/`^`
>   forms only (not from `\ooo`, not from the C escapes) and stops `\`
>   itself being escaped.
> - **Locale** — `VIS_NOLOCALE` (0x4000) makes the input be read one byte
>   at a time and swaps `iswgraph` for `isgraph`; see
>   `[spec:libedit:sem:vis.iscgraph-fn]`.
>
> ## `VIS_WHITE`: the on-disk history format
>
> `history.c` writes every history entry as
> `strvis(ptr, str, VIS_WHITE)` followed by a newline, and reads it back
> with `strunvis` (flag 0). Under
> `[dec:libedit:no-c-ffi]` and `[dec:libedit:posix-only-scope]` this
> encoding is frozen: a history file written by the Rust port must be
> readable by the C library and vice versa. `VIS_WHITE` is
> `VIS_SP|VIS_TAB|VIS_NL` = 0x001C — no `VIS_CSTYLE`, no `VIS_OCTAL`, no
> `VIS_NOSLASH`, no `VIS_SAFE`, no `VIS_NOLOCALE` — so the extra set is
> exactly `{ space, tab, newline, backslash }` plus the implicit NUL, and
> the rules reduce to, per input character:
>
> | input | output |
> | --- | --- |
> | NUL (0x00) | `\000` |
> | tab (0x09) | `\011` |
> | newline (0x0A) | `\012` |
> | space (0x20) | `\040` |
> | backslash (0x5C) | `\134` |
> | other control 0x01–0x1F | `\^X`, where X is the byte + 0x40 (`\^A` … `\^_`) |
> | DEL (0x7F) | `\^?` |
> | 0xA0 as a lone byte | `\240` (the `(c & 0177) == ' '` mask; see `[spec:libedit:sem:vis.do-mbyte-fn]`) |
> | other byte 0x80–0xFF that is not a valid character | `\M-x` if the low 7 bits are printable, `\M^X` if they are a control code |
> | any character graphic in the current locale | itself, verbatim |
>
> Verified end to end: `"a b\tc\nd\\e\x01\x7f€"` encodes in a UTF-8 locale
> to `a\040b\011c\012d\134e\^A\^?` followed by the literal UTF-8 bytes of
> €, 30 bytes, and `strunvis` restores the original exactly.
>
> **The format is locale-dependent, and this is the single biggest hazard
> in the file.** The literal-pass-through decision is `iswgraph` in the
> caller's `LC_CTYPE`. The same input encodes as `…\^A\^?` + `E2 82 AC` in
> a UTF-8 locale (30 bytes) and as `…\^A\^?\M-b\M^B\M-,` in the C locale
> (39 bytes). Both decode back to the same bytes, so the round trip is
> safe, but a history file is not byte-identical across locales and a port
> that hardcodes either behaviour will produce files that differ from the
> C library's under some locale. The port must consult the process's
> `LC_CTYPE` exactly as the C does.
>
> The second hazard is the conversion-error latch documented in
> `[spec:libedit:sem:vis.istrsenvisx-fn]` step 7: if the entry contains a
> valid multibyte sequence *followed later* by an invalid byte, the
> earlier characters are written as raw code-point bytes and the entry is
> silently corrupted. `history.c` feeds `strvis` the output of
> `ct_encode_string`, which should be well-formed, so this is a
> robustness concern rather than a routine one.

> [spec:libedit:def:vis.strvisx-fn]
> int strvisx(char *mbdst, const char *mbsrc, size_t len, int flags)

> [spec:libedit:sem:vis.strvisx-fn]
> Counted, unbounded encode: exactly
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mbdstp = &mbdst`,
> `dlen = NULL`, the caller's `mbsrc`, `len` and `flags`, `mbextra = ""`,
> and `cerr_ptr = NULL`. It is `[spec:libedit:sem:vis.strvis-fn]` with an
> explicit source length instead of NUL termination.
>
> Encodes exactly `len` bytes, so embedded NULs are data: a NUL encodes as
> `\000`, or as `\0` / `\000` under `VIS_CSTYLE` depending on whether the
> next character's low byte is an octal digit. This is the variant for
> escaping a block of binary data. Flag semantics are those of
> `[spec:libedit:sem:vis.strvis-fn]`.
>
> Returns the number of bytes written, not counting the terminating NUL,
> which is always written on success; -1 with `errno = ENOMEM` on
> allocation failure. **No bound is applied to `mbdst`** — the caller must
> supply at least `4 * len + 1` bytes. `len == 0` returns 0 and writes an
> empty string.
>
> **`len == 1` reads `mbsrc[1]`.** The engine bumps its internal byte
> count from 1 to 2 to obtain a lookahead character, which for this entry
> point is one byte past what the caller declared — an out-of-bounds read
> whose value is observable in the output. Verified with `VIS_CSTYLE` on a
> single NUL byte: `\000` when the byte after it is `'7'`, `\0` when it is
> `'x'`. A port must not read that byte; it should treat the lookahead as
> absent (equivalent to `L'\0'`, which is not an octal digit) when
> `len == 1`.
>
> On NetBSD this symbol is also weakly aliased as `_strvisx`; the alias is
> out of scope for the port under `[dec:libedit:posix-only-scope]`.

> [spec:libedit:def:vis.svis-fn]
> char * svis(char *mbdst, int c, int flags, int nextc, const char *mbextra)

> [spec:libedit:sem:vis.svis-fn]
> Unbounded single-character encode with a caller-supplied extra set.
> Copies `c` and `nextc` into a 2-byte array `cc = { (char)c, (char)nextc
> }`, then calls `[spec:libedit:sem:vis.istrsenvisx-fn]` with
> `mbdstp = &mbdst`, `dlen = NULL`, `mbsrc = cc`, `mblength = 1`, the
> caller's `flags` and `mbextra`, and `cerr_ptr = NULL`.
>
> Returns NULL if the engine fails (`errno = ENOMEM`), otherwise
> `mbdst + n` where `n` is the number of bytes written — a pointer to the
> terminating NUL that the engine has already stored, so it is both the
> end of this character's encoding and the place to write the next one.
> The idiomatic loop is `dst = svis(dst, *s, flags, s[1], extra)`.
>
> No bound is applied: `mbdst` must have room for at least 5 bytes (4 for
> the widest single-byte escape, plus the NUL), or more in the
> recombination case below.
>
> Semantics: only `c` is encoded. `nextc` is the lookahead used by
> `[spec:libedit:sem:vis.do-mbyte-fn]` (the `\0` versus `\000` choice) and
> by `[spec:libedit:sem:vis.do-mvis-fn]` (the trailing-whitespace test).
> Extra-set and flag handling are those of
> `[spec:libedit:sem:vis.strsenvisx-fn]`.
>
> Quirks, all carried over from
> `[spec:libedit:sem:vis.snvis-fn]`:
>
> 1. `c` and `nextc` are truncated to `char`; only the low 8 bits matter.
> 2. In a multibyte locale the two bytes may decode to a **single** wide
>    character, in which case both are encoded together and `nextc` is
>    consumed as data. Verified: `vis(dst, 0xC3, VIS_WHITE, 0xA9)` in a
>    UTF-8 locale writes 2 bytes (`C3 A9`, é passed through literally) and
>    returns `dst + 2`; in the C locale it writes `\M-C` and returns
>    `dst + 4`.
> 3. If the two bytes form an incomplete sequence, the engine falls back
>    to byte-at-a-time and encodes only `c`'s byte.

> [spec:libedit:def:vis.unvis-fn]
> int unvis(char *, int, int *, int)

> [spec:libedit:sem:vis.unvis-fn]
> `vis.h` prototype for the incremental decoder; implemented in
> `unvis.c`. Full semantics — every state, every escape form, every defect
> — are rule `[spec:libedit:sem:unvis.unvis-fn]`. The declaration is
> guarded by `#ifndef __LIBC12_SOURCE__`, a NetBSD symbol-versioning
> artefact with no bearing on the port.
>
> ABI contract as declared here: `unvis(char *cp, int c, int *astate, int
> flag)` consumes at most the one input byte `c` and reports through its
> return value whether a decoded byte is now available in `*cp`. All
> cross-call state lives in `*astate` (low 8 bits: state machine state;
> top 8 bits: entity-name index) and in the partially accumulated byte at
> `*cp`; the caller must zero `*astate` before the first call of a
> sequence and must not touch either between calls.
>
> Return codes, defined in this header:
>
> - `UNVIS_VALID` (1) — `*cp` holds a decoded byte; the input byte was
>   consumed.
> - `UNVIS_VALIDPUSH` (2) — `*cp` holds a decoded byte and the input byte
>   was **not** consumed; emit `*cp`, then call again with the same byte.
>   Push-back depth is always exactly 1.
> - `UNVIS_NOCHAR` (3) — byte consumed, sequence still in progress.
> - `UNVIS_SYNBAD` (-1) — malformed sequence.
> - `UNVIS_ERROR` (-2) — declared but never returned by this
>   implementation.
>
> `flag` carries the escape-introducer bits `VIS_NOESCAPE` (0x0400),
> `VIS_HTTP1808`/`VIS_HTTPSTYLE` (0x0080), `VIS_MIMESTYLE` (0x0100) and
> `VIS_HTTP1866` (0x0200), plus `UNVIS_END` (= `_VIS_END`, 0x0800) which
> requests the end-of-input flush and makes the byte argument irrelevant.
> All other bits are ignored. `flag == 0` decodes backslash escapes only —
> the mode the history file is read in.

> [spec:libedit:def:vis.vis-fn]
> char * vis(char *mbdst, int c, int flags, int nextc)

> [spec:libedit:sem:vis.vis-fn]
> Unbounded single-character encode with no extra set: exactly
> `[spec:libedit:sem:vis.svis-fn]` with `mbextra = ""`. Builds the 2-byte
> array `{ (char)c, (char)nextc }` and calls
> `[spec:libedit:sem:vis.istrsenvisx-fn]` with `mblength = 1`,
> `dlen = NULL`, `mbextra = ""` and `cerr_ptr = NULL`.
>
> Returns NULL on failure (`errno = ENOMEM`), otherwise
> `mbdst + n` where `n` is the number of bytes written — a pointer to the
> terminating NUL the engine already wrote, ready for the next call.
> `mbdst` must have room for at least 5 bytes.
>
> `c` is the character encoded; `nextc` is lookahead only, used for the
> `\0` versus `\000` decision under `VIS_CSTYLE` and for the
> trailing-whitespace test under `VIS_MIMESTYLE`. Verified:
> `vis(dst, 0, VIS_CSTYLE, '7')` writes `\000` and returns `dst + 4`,
> while `vis(dst, 0, VIS_CSTYLE, 'x')` writes `\0` and returns `dst + 2`.
> Flag semantics are those of `[spec:libedit:sem:vis.strvis-fn]`.
>
> The two quirks of `[spec:libedit:sem:vis.svis-fn]` apply in full: `c`
> and `nextc` are truncated to `char`, and in a multibyte locale a lead
> byte in `c` completed by `nextc` is encoded as one character, so this
> "single character" function can consume and emit the lookahead byte too.

> [spec:libedit:def:vis.visfun-t-wchar-t-wint-t-int-wint-t-const-wchar-t]
> typedef wchar_t *(*visfun_t)(wchar_t *, wint_t, int, wint_t, const wchar_t *)

