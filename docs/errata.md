# libedit defects register (errata)

This file is the single collected list of defects in the C source of libedit,
as found during the Wave 1 markup pass. Every entry is derived from something a
`sem` rule under `docs/spec/port/src/*.md` actually says; nothing here comes
from general knowledge of libedit or from reading the C directly. Where a rule
was ambiguous about whether something is a defect or a deliberate design quirk,
the entry is included and the ambiguity is noted.

**Ids are stable.** `ERR-<concern>-<nn>` ids are referenced from tests. Numbering
runs within a concern; new findings are appended with the next free number and
existing entries are never renumbered, even when one is withdrawn.

**Ordering.** Within a concern, entries are grouped by class in the order
`UB`, `memory`, `logic`, `divergence`, `dead` — a rough severity proxy, not a
ranking.

**Classes.**
- `UB` — undefined behaviour: out-of-bounds access, invalid pointer arithmetic,
  type punning, uninitialised or indeterminate reads, invalid library arguments.
- `memory` — leak, double free, dangling pointer or use-after-free that is
  defined-but-wrong (a genuine UAF is filed as `UB`).
- `logic` — defined behaviour, wrong result.
- `divergence` — differs from GNU readline, from real vi, or from libedit's own
  documentation.
- `dead` — unreachable or vestigial code.

**Disposition** follows `plan/decisions/conformance-policy.md`:
- `define` — the construct is undefined in C, so it cannot be reproduced. The
  port picks a defined behaviour and the choice is recorded in the `sem` rule.
- `reproduce` — defined observable behaviour, bug included, is reproduced.
- `fix` — the rule itself directs the port to diverge, or the behaviour is not
  observable across the C ABI (most leaks and teardown-order hazards).
- `needs decision` — the policy does not settle it: a rule demands a deliberate
  choice, another decision flags the area unresolved, or two sources conflict.

The six behavioural forks the conformance policy names explicitly — the
physical-tabs capability, `H_FUNC`'s dropped ref pointer, `free_history_entry`'s
empty body, the pointer-sorting completion comparator, tilde expansion of a bare
tilde, and `el_deletestr1`'s arithmetic — are marked as such in their entries.

**Status** is `open` for every entry. Wave 3 should carry one conformance test
per entry: for `reproduce` entries a test that asserts the defective behaviour,
for `define` entries a test that asserts the chosen defined behaviour, and for
`needs decision` entries a test written after the decision is taken.

## Summary

380 entries. A handful carry a secondary class in parentheses (for example
"UB … / logic …"); the table counts the primary class only.

| concern | UB | memory | logic | divergence | dead | total |
|---|---|---|---|---|---|---|
| encoding | 7 | 4 | 13 | 3 | 1 | **28** |
| terminal | 17 | 2 | 41 | 4 | 1 | **65** |
| buffer | 12 | 2 | 8 | 0 | 1 | **23** |
| input | 17 | 3 | 20 | 2 | 1 | **43** |
| history | 11 | 5 | 17 | 4 | 1 | **38** |
| modes | 17 | 1 | 34 | 18 | 1 | **71** |
| completion | 5 | 1 | 13 | 3 | 1 | **23** |
| core-api | 11 | 2 | 17 | 4 | 1 | **35** |
| readline-compat | 14 | 7 | 20 | 12 | 1 | **54** |
| **total** | **111** | **27** | **183** | **50** | **9** | **380** |

### Entries marked `needs decision` (26)

- **ERR-encoding-12** — `ct_encode_string` calls `abort()` when a single character needs more than its hard-coded 5 bytes, killing the process. `MB_LEN_MAX` is 16 on glibc.
- **ERR-encoding-13** — `ct_visual_char` emits at most five hex digits for `CHTYPE_NONPRINT`, so any code point at or above U+100000 loses every bit above 0x0FFFFF:…
- **ERR-encoding-14** — `istrsenvisx`'s conversion-error flag latches for the whole string and the output loop tests it once per character, so a valid multibyte prefix…
- **ERR-encoding-27** — `ct_decode_string` and `ct_encode_string` are declared without `libedit_private`, so they are exported symbols of the shared library despite not…
- **ERR-terminal-30** — `terminal_settc` copies both the capability name and the value into 8-byte buffers with a truncating bounded copy, so a capability *string* longer…
- **ERR-terminal-36** — `tty_get_signal_character` tests `ECHOCTL` against `c_iflag` (`MD_INP`) when `ECHOCTL` is a `c_lflag` bit. On glibc `ECHOCTL` has the same value…
- **ERR-terminal-37** — the same function indexes `el_tty.t_c[ED_IO]` with termios `V*` subscripts instead of libedit's `C_*` constants. They coincide by accident for…
- **ERR-terminal-56** — `sig_handler` de-installs itself for the signal it handled, and `sig_no` is only consulted after a *failed* read. A signal arriving while no read…
- **ERR-terminal-61** — `tgetflag("pt")` has no terminfo counterpart: terminfo expresses hardware tabbing as the *string* capability `tab_to_next_stop`, not a boolean, so…
- **ERR-terminal-62** — `xt` (destructive tabs) maps only onto terminfo `dest_tabs_magic_smso`, which conflates tab destruction with the Teleray magic-standout quirk;…
- **ERR-terminal-63** — `tgoto` takes its parameters column-first while terminfo's two-parameter cursor addressing (`cup`) takes row first, a discrepancy the C's termcap…
- **ERR-input-21** — `read__fixio`'s would-block recovery **permanently** clears `O_NONBLOCK`/`O_NDELAY` on the caller's input descriptor, normally the process's…
- **ERR-input-22** — `el_wgets`'s `EDIT_DISABLED` path runs *after* `read_prepare` (so `sig_set` has installed handlers) but returns through `noedit_wgets` and never…
- **ERR-input-30** — `keymacro_add` passes any `ntype` other than `XK_CMD`/`XK_STR` through to `node__try`'s `EL_ABORT`, killing the process. This is reachable:…
- **ERR-input-37** — `parse__escape`'s `\U+` form always consumes **one character more** than the escape text, silently discarding whatever follows: `\U+0041x`…
- **ERR-input-42** — the macro/pushback queue is FIFO: `el_wpush` writes at the back (`macro[++level]`) while `el_wgetc` always reads from the front (`macro[0]`), so a…
- **ERR-history-01** — `history_def_enter` inserts the new entry, then evicts from the tail while `cur > max`. With `max == 0` — the state of every history until…
- **ERR-history-16** — `history_save` returns -1 when `fdopen` fails **without closing the descriptor it opened**, leaking a file descriptor.
- **ERR-history-18** — `hist_convert` fills a **local** `HistEventW`, so under `NARROW_HISTORY` the cookie `el->el_history.ev` is never written by any history operation…
- **ERR-modes-28** — `map_bind` stores the resolved command number through an `(el_action_t)` cast, i.e. modulo 256. With `EL_NUM_FCNS == 96` the 160th function…
- **ERR-modes-32** — `ce_inc_search` dispatches on `el->el_map.current[(unsigned char) ch]`, i.e. the **low byte** of the wide character. The default emacs map holds…
- **ERR-completion-17** — all of `fn_filename_completion_function`'s state lives in function-level statics (an open `DIR *`, the pattern, its length, the directory prefix…
- **ERR-core-api-22** — two rules disagree about `el_source`'s empty-file-name early return: `[spec:libedit:sem:el.el-source-fn]` says "a constructed path is never empty,…
- **ERR-readline-01** — `rl_completion_matches` sorts with `qsort(&list[1], len - 1, sizeof(*list), (int (*)(const void *, const void *))strcmp)`. That is wrong twice:…
- **ERR-readline-06** — `rl_copy_text` calls `strlcpy(out, li->buffer + from, len)`, but `strlcpy`'s third argument is the destination **size**, so it copies only `len -…
- **ERR-readline-14** — `history_truncate_file`'s copy-back phase checks `ferror(fp)` where it should check `ferror(tp)`, so a read error on the temporary file is…

The conformance policy names six behavioural forks that default to
`reproduce`; five are recorded here as `reproduce` (ERR-buffer-15,
ERR-completion-07, ERR-history-17, ERR-readline-15, and the physical-tabs
capability as ERR-terminal-61, which a second decision reopens). The sixth,
the pointer-sorting completion comparator, is ERR-readline-01, where the
policy and the `sem` rule give opposite instructions.

---

## encoding

`src/chartype.c`, `src/vis.c`, `src/unvis.c` — locale conversion, visual
rendering and the `vis`/`unvis` escaping that the history file format depends on.

**ERR-encoding-01** — `ct_chr_class` hands a negative `wchar_t` to `iswcntrl`, whose argument must be representable as `wint_t` or be exactly `WEOF`.
- rule: `[spec:libedit:sem:chartype.ct-chr-class-fn]` · C: `src/chartype.c` `ct_chr_class`
- class: UB · reach: hot — `MB_FILL_CHAR` is `(wint_t)-1`, refresh.c stores it in screen-image cells and then classifies those cells. glibc happens to answer false.
- disposition: define — classify non-scalar cell values explicitly rather than passing them to a locale predicate.
- status: open

**ERR-encoding-02** — `ct_encode_char` bounds-checks with `ct_enc_width` (measured from the initial shift state) but writes with `wctomb` (which continues from libc's global state), so in a stateful encoding it can write past `len`.
- rule: `[spec:libedit:sem:chartype.ct-encode-char-fn]` · C: `src/chartype.c` `ct_encode_char`
- class: UB · reach: cold — requires a stateful `LC_CTYPE` (ISO-2022 and relatives); impossible in UTF-8 or any single-byte locale.
- disposition: define — encode once and use the length actually produced.
- status: open

**ERR-encoding-03** — because `ct_enc_width` returns 0 for an unencodable character, the `len < width` test is false for every `len` including 0, so `wctomb` is handed `dst` with no guaranteed space.
- rule: `[spec:libedit:sem:chartype.ct-encode-char-fn]` · C: `src/chartype.c` `ct_encode_char`
- class: UB · reach: cold — needs a character the locale cannot encode and a caller passing a small `len`; safety rests on `wctomb` failing before it writes.
- disposition: define — reject `len == 0` outright.
- status: open

**ERR-encoding-04** — `ct_conv_wbuff_resize` computes `wsize * sizeof(wchar_t)` unchecked; the multiplication wraps above `SIZE_MAX / sizeof(wchar_t)`.
- rule: `[spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn]` · C: `src/chartype.c` `ct_conv_wbuff_resize`
- class: UB · reach: unreachable from libedit's own callers, which derive sizes from string lengths plus 1024.
- disposition: define — use a checked allocation.
- status: open

**ERR-encoding-05** — `ct_decode_argv` with a negative `argc` wraps the `argc + 1` allocation count.
- rule: `[spec:libedit:sem:chartype.ct-decode-argv-fn]` · C: `src/chartype.c` `ct_decode_argv`
- class: UB · reach: cold but real — not reachable inside the library, but reachable across the public C ABI through `el_parse`.
- disposition: define — reject a negative count.
- status: open

**ERR-encoding-06** — `istrsenvisx` bumps `mblength` from 1 to 2 to obtain a lookahead character, reading `mbsrc[1]` one byte past what the caller declared; the byte read is observable in the output.
- rule: `[spec:libedit:sem:vis.istrsenvisx-fn]` (step 1); also `[spec:libedit:sem:vis.strvisx-fn]`, `[spec:libedit:sem:vis.strsvisx-fn]`, `[spec:libedit:sem:vis.strnvisx-fn]`, `[spec:libedit:sem:vis.strsnvisx-fn]` · C: `src/vis.c` `istrsenvisx`
- class: UB · reach: cold — any caller of the counted `x` variants with `len == 1`; the `l` variants are safe because the extra byte is the caller's own NUL.
- disposition: define — treat the lookahead as absent (equivalent to `L'\0'`) when `len == 1`.
- status: open

**ERR-encoding-07** — `unvis`'s `S_START` default arm classifies the raw `int c` with `isgraph` instead of the `unsigned char` widening used everywhere else, so a signed-`char` caller passes a negative value.
- rule: `[spec:libedit:sem:unvis.unvis-fn]` (defect 1) · C: `src/unvis.c` `unvis`
- class: UB · reach: cold — `\` followed by a byte >= 0x80 in a hand-written or corrupt history line; not producible by `vis`.
- disposition: define — implement C-locale graphic as `0x21..=0x7E`, which matches the observed glibc outcome (`UNVIS_SYNBAD`).
- status: open

**ERR-encoding-08** — an allocation failure in `ct_conv_cbuff_resize` / `ct_conv_wbuff_resize` does not leave the previous buffer intact: it frees it, NULLs the pointer and zeroes the size, dangling every pointer ever handed out into it.
- rule: `[spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]`, `[spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn]` · C: `src/chartype.c`
- class: memory · reach: OOM only.
- disposition: reproduce the observable outcome (the struct returns to its empty state and is reusable); the dangling pointers are UB and are not reproduced.
- status: open

**ERR-encoding-09** — when `makeextralist` fails, `istrsenvisx` writes a NUL into the destination, sets the return value to **0** (success) and then takes the cleanup path, which frees that destination when the function allocated it. `stravis` therefore returns 0 with `*mbdstp` pointing at freed memory.
- rule: `[spec:libedit:sem:vis.istrsenvisx-fn]` (step 5); `[spec:libedit:sem:vis.stravis-fn]` · C: `src/vis.c` `istrsenvisx`
- class: memory · reach: OOM only.
- disposition: define — return an error rather than a dangling pointer.
- status: open

**ERR-encoding-10** — `istrsenvisx`'s cleanup path frees a destination it allocated but never resets `*mbdstp`, so every failure after allocation leaves the caller holding a dangling non-NULL pointer.
- rule: `[spec:libedit:sem:vis.istrsenvisx-fn]` (cleanup path); `[spec:libedit:sem:vis.stravis-fn]` · C: `src/vis.c` `istrsenvisx`
- class: memory · reach: OOM only.
- disposition: define.
- status: open

**ERR-encoding-11** — five function-scope `static ct_buffer_t` objects (in `search.c`, `history.c`, `readline.c`) are never freed; they grow to the largest string ever converted and leak for the process lifetime, and they make the functions that own them non-thread-safe.
- rule: `[spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]` (ownership); `[spec:libedit:sem:search.el-match-fn]`; `[spec:libedit:sem:history.history-load-fn]`; `[spec:libedit:sem:histedit.history-w-fn]` · C: `src/search.c`, `src/history.c`, `src/readline.c`
- class: memory · reach: hot — every history search, every history load/save.
- disposition: fix — a permanent allocation is not ABI-observable; the port uses owned per-instance buffers.
- status: open

**ERR-encoding-12** — `ct_encode_string` calls `abort()` when a single character needs more than its hard-coded 5 bytes, killing the process. `MB_LEN_MAX` is 16 on glibc.
- rule: `[spec:libedit:sem:chartype.ct-encode-string-fn]` · C: `src/chartype.c` `ct_encode_string`
- class: logic · reach: cold — unreachable in UTF-8 (max 4 bytes) and in single-byte locales; reachable in a stateful encoding that emits shift sequences.
- disposition: needs decision — the rule says "A Rust port must decide what to do here; it is a real, locale-reachable process abort in the C."
- status: open

**ERR-encoding-13** — `ct_visual_char` emits at most five hex digits for `CHTYPE_NONPRINT`, so any code point at or above U+100000 loses every bit above 0x0FFFFF: U+10FFFF renders as `\U+0FFFF`.
- rule: `[spec:libedit:sem:chartype.ct-visual-char-fn]` · C: `src/chartype.c` `ct_visual_char`
- class: logic · reach: cold — plane-16 code points that are non-printable in the active locale; observable through the rendered line and through `terminal_telltc`.
- disposition: needs decision — the rule says the port "has to decide whether to reproduce this ... or widen the field", and widening also changes `ct_visual_width` and every downstream column calculation.
- status: open

**ERR-encoding-14** — `istrsenvisx`'s conversion-error flag latches for the whole string and the output loop tests it once per character, so a valid multibyte prefix followed by *any* later invalid byte causes the earlier characters to be written as raw code-point bytes.
- rule: `[spec:libedit:sem:vis.istrsenvisx-fn]` (step 7); `[spec:libedit:sem:vis.strvis-fn]` · C: `src/vis.c` `istrsenvisx`
- class: logic · reach: cold for history (the input comes from `ct_encode_string` and should be well formed), hot for a caller passing arbitrary bytes. Verified in the rule: `"\xe2\x82\xac\xffz"` produces `20 AC FF 7A`.
- disposition: needs decision — the rule says "A port should decide deliberately whether to reproduce this; it cannot round-trip through `strunvis`."
- status: open

**ERR-encoding-15** — `ct_encode_string` silently drops any character the locale cannot encode, and `ct_enc_width` reports 0 for it, so byte counts crossing the C ABI (`el_gets`'s `*nread`, `el_line`'s offsets, `literal.c`'s sizing) under-count with no way for a caller to detect it.
- rule: `[spec:libedit:sem:chartype.ct-encode-string-fn]`, `[spec:libedit:sem:chartype.ct-enc-width-fn]` · C: `src/chartype.c`
- class: logic · reach: hot in the C/POSIX locale, where every non-ASCII character is unencodable.
- disposition: reproduce — the 0 crosses the ABI; do not substitute a replacement-character width.
- status: open

**ERR-encoding-16** — `ct_encode_char` uses `wctomb`, which carries libc's process-global, non-thread-safe conversion state; libedit mixes it with `ct_enc_width`'s thread-safe `wcrtomb` against the same logical stream and never resets it except on the unencodable-character path.
- rule: `[spec:libedit:sem:chartype.ct-encode-char-fn]`, `[spec:libedit:sem:chartype.ct-enc-width-fn]`, `[spec:libedit:sem:chartype.ct-encode-string-fn]` · C: `src/chartype.c`
- class: logic · reach: cold — only observable in a stateful encoding or under concurrency.
- disposition: fix — carry explicit per-conversion state; not observable in any stateless locale.
- status: open

**ERR-encoding-17** — `ct_visual_width` passes `wcwidth`'s -1 straight through for `CHTYPE_PRINT`, and refresh.c adds it directly into its column accumulator. The -1 is reachable whenever the locale's `iswprint` and `wcwidth` disagree.
- rule: `[spec:libedit:sem:chartype.ct-visual-width-fn]` · C: `src/chartype.c` `ct_visual_width`
- class: logic · reach: cold, locale-dependent.
- disposition: reproduce — the rule says the pass-through including the negative must be kept unless the rendered geometry is being changed deliberately.
- status: open

**ERR-encoding-18** — `ct_visual_char`'s `(unsigned int)c` cast turns a negative `wchar_t` into a large unsigned value and produces meaningless hex digits (and narrows implicitly where `wchar_t` is wider than `unsigned int`); its `(ssize_t)len` cast makes any `len` above `SSIZE_MAX` compare as negative and spuriously return -1.
- rule: `[spec:libedit:sem:chartype.ct-visual-char-fn]` · C: `src/chartype.c` `ct_visual_char`
- class: logic · reach: the first is reachable via `MB_FILL_CHAR` in the screen image; the second is not reachable from libedit's callers.
- disposition: define — the port carries screen cells as `u32` and never forms a negative width.
- status: open

**ERR-encoding-19** — `do_hvis`'s `%XX` and `do_mvis`'s `=XX` encode only the low 8 bits of a whole wide character; the byte decomposition `do_svis` performs is not applied, so U+0378 becomes `%78` and U+20AC becomes `=AC`.
- rule: `[spec:libedit:sem:vis.do-hvis-fn]`, `[spec:libedit:sem:vis.do-mvis-fn]` · C: `src/vis.c`
- class: logic · reach: cold — requires `VIS_HTTPSTYLE`/`VIS_MIMESTYLE`, which libedit itself never sets; reachable through the exported `strvis` family.
- disposition: reproduce — the encoding crosses the ABI.
- status: open

**ERR-encoding-20** — `do_mbyte`'s `VIS_CSTYLE` default arm can emit `\x`, which is the decoder's hex introducer, so the output does not round-trip: `strunvis` of a trailing `\x` yields 0 bytes and `\x` before a hex digit decodes as that value.
- rule: `[spec:libedit:sem:vis.do-mbyte-fn]` (round-trip defect) · C: `src/vis.c` `do_mbyte`
- class: logic · reach: cold — needs `VIS_CSTYLE`, which the history path does not use.
- disposition: reproduce.
- status: open

**ERR-encoding-21** — `do_mbyte`'s `iswoctal(nextc)` test truncates the whole next wide character to its low byte, so a following U+0130 (low byte 0x30) forces the three-digit `\000` form even though no digit follows in the output.
- rule: `[spec:libedit:sem:vis.do-mbyte-fn]` · C: `src/vis.c` `do_mbyte`
- class: logic · reach: cold; observable only in the byte count and return value, since `\000` still decodes to NUL.
- disposition: reproduce.
- status: open

**ERR-encoding-22** — `do_mbyte`'s stage-2 test `(c & 0177) == 0x20` is a masked comparison, so byte 0xA0 is always octal-escaped as `\240` while its neighbour 0xA1 comes out as `\M-!`, even with no relevant flag set.
- rule: `[spec:libedit:sem:vis.do-mbyte-fn]` · C: `src/vis.c` `do_mbyte`
- class: logic · reach: hot for the history file whenever a 0xA0 byte occurs.
- disposition: reproduce — the on-disk history format is frozen.
- status: open

**ERR-encoding-23** — `iscgraph`, the classifier `VIS_NOLOCALE` exists to provide, compiles on every POSIX host as plain `isgraph`, i.e. a test in the *current* locale; the genuine C-locale form is a NetBSD-only macro and the third form is behind `#ifdef notyet`.
- rule: `[spec:libedit:sem:vis.iscgraph-fn]` · C: `src/vis.c` `iscgraph` / `ISGRAPH`
- class: logic · reach: cold — the two forms diverge only in single-byte non-ASCII locales such as ISO-8859-1.
- disposition: fix — the rule directs the port to implement the C-locale semantics (`0x21..=0x7E`); form 2 is "a known incompleteness, not a behaviour to preserve".
- status: open

**ERR-encoding-24** — five defects in the `unvis` state machine, all observable through the public ABI: (a) `S_NUMBER`'s `;` arm returns `UNVIS_VALID` without resetting the state, so `&#65;X` fails and `&#65;;` emits the byte twice; (b) `%` followed by a non-hex byte emits a NUL instead of restoring the `%`; (c) `S_STRING`'s prefix test is one character deep and accepts 15 non-names; (d) `S_EATCRNL`'s default arm emits the byte without re-entering the `S_GROUND` dispatch, so an escape immediately after a soft line break is not honoured; (e) `UNVIS_END` returning `UNVIS_SYNBAD` leaves `*astate` unchanged, unlike every other `UNVIS_SYNBAD`.
- rule: `[spec:libedit:sem:unvis.unvis-fn]` (defects 2-6) · C: `src/unvis.c` `unvis`
- class: logic · reach: (a)-(d) need the HTTP/MIME flags and so cannot affect the history file; (e) is reachable with `flag == 0` from a corrupt history line.
- disposition: reproduce — frozen by `[dec:libedit:no-c-ffi]`; a drop-in `unvis` must produce the same results.
- status: open

**ERR-encoding-25** — `ct_visual_char` writes cells while `ct_visual_width` reports columns, and the two disagree for TAB (2 vs 1), NL (2 vs 0) and every `CHTYPE_PRINT` character whose `wcwidth` is not 1. The chartype.h claim that they match is false.
- rule: `[spec:libedit:sem:chartype.ct-visual-char-fn]`, `[spec:libedit:sem:chartype.ct-visual-width-fn]` · C: `src/chartype.h`
- class: divergence (documentation) · reach: hot for double-width and combining characters.
- disposition: reproduce the code; the port keeps cells and columns as distinct types and does not carry the header's claim forward.
- status: open

**ERR-encoding-26** — `strnvis`/`strnvisx`/`strsnvis`/`strsnvisx` do **not** truncate on overflow: they return -1 with `ENOSPC` and leave the destination partially written and unterminated, unlike OpenBSD/FreeBSD's `snprintf`-shaped contract. The argument order `(dst, dlen, src, ...)` also differs from historical NetBSD.
- rule: `[spec:libedit:sem:vis.strnvis-fn]`, `[spec:libedit:sem:vis.strnvisx-fn]`, `[spec:libedit:sem:vis.strnunvis-fn]` · C: `src/vis.c`, `src/vis.h`
- class: divergence · reach: hot for any consumer porting code from another BSD.
- disposition: reproduce — the exported ABI is frozen.
- status: open

**ERR-encoding-27** — `ct_decode_string` and `ct_encode_string` are declared without `libedit_private`, so they are exported symbols of the shared library despite not appearing in `histedit.h`.
- rule: `[spec:libedit:sem:chartype.ct-decode-string-fn]`, `[spec:libedit:sem:chartype.ct-encode-string-fn]` · C: `src/chartype.c`
- class: divergence · reach: link-time only.
- disposition: needs decision — whether the port exports these accidental symbols is an ABI-surface question the policy does not settle.
- status: open

**ERR-encoding-28** — dead and vestigial code in the encoding layer: `ct_visual_width`'s `CHTYPE_TAB` arm returns 1 but every caller intercepts tabs first; `ct_visual_char`'s "any other class" arm is unreachable because `ct_chr_class` is total, and carries a `/*FALLTHROUGH*/` comment after an unconditional `return`; `VIS_HTTP1866` is understood by the decoder but no encoder ever produces `&name;`/`&#ddd;`, so setting the bit has no effect at all.
- rule: `[spec:libedit:sem:chartype.ct-visual-width-fn]`, `[spec:libedit:sem:chartype.ct-visual-char-fn]`, `[spec:libedit:sem:vis.getvisfun-fn]` · C: `src/chartype.c`, `src/vis.c`
- class: dead · reach: unreachable.
- disposition: fix — not ported; the port makes the matches exhaustive instead.
- status: open

---

## terminal

`src/terminal.c`, `src/refresh.c`, `src/tty.c`, `src/sig.c`, `src/literal.c`,
`src/prompt.c` — capability handling, the screen model, tty modes, signals and
prompt rendering.

**ERR-terminal-01** — `terminal_alloc`'s append test is `t_loc + 3 < TC_BUFSIZE` and does not involve the string's length at all, so a capability longer than the remaining pool space writes past the end of the 2048-byte `t_buf`.
- rule: `[spec:libedit:sem:terminal.terminal-alloc-fn]` (step 4) · C: `src/terminal.c` `terminal_alloc`
- class: UB · reach: cold — needs the capability pool nearly full; 39 capabilities rarely total 2048 bytes.
- disposition: define — test that `clen + 1` fits; the port owns a string per slot and removes the pool entirely.
- status: open

**ERR-terminal-02** — `terminal_alloc`'s pool compaction copies the retained capability strings into a scratch buffer and back but never repoints the table slots, so after it runs every other slot addresses the wrong offset and reads a different capability. The scratch copy is itself unbounded.
- rule: `[spec:libedit:sem:terminal.terminal-alloc-fn]` (step 5) · C: `src/terminal.c` `terminal_alloc`
- class: UB · reach: cold — only when the 2048-byte pool is exhausted.
- disposition: define — the port does not reproduce the pool.
- status: open

**ERR-terminal-03** — `terminal_bind_arrow` widens a function-key capability into a `wchar_t[VISUAL_WIDTH_MAX]` (8 elements) by copying while `n < 8`, so a capability of 8 bytes or more leaves the buffer with no terminator and the `keymacro_*` calls below read past its end.
- rule: `[spec:libedit:sem:terminal.terminal-bind-arrow-fn]` (step 4b) · C: `src/terminal.c` `terminal_bind_arrow`
- class: UB · reach: cold — depends on the terminal entry; most arrow capabilities are 3-4 bytes.
- disposition: define — bound the copy to 7 elements and always terminate.
- status: open

**ERR-terminal-04** — `terminal_echotc`'s parameter-counting scan reads the character after `%` unconditionally, so a capability ending in a bare `%` reads the terminating NUL and the loop then steps one position past it.
- rule: `[spec:libedit:sem:terminal.terminal-echotc-fn]` (step 8) · C: `src/terminal.c` `terminal_echotc`
- class: UB · reach: cold — needs a malformed capability, reachable via `settc` or a hand-written terminfo entry.
- disposition: define — stop the scan at the NUL.
- status: open

**ERR-terminal-05** — `tty_stty`'s `name=value` form computes the `c_cc` subscript with `tty__getcharindex`, which has no `C_BRK` case, so `setty brk=X` yields -1 and writes `tios->c_cc[-1]` once the guarding `assert` is compiled out under `NDEBUG`.
- rule: `[spec:libedit:sem:tty.tty-stty-fn]` (step 6e), `[spec:libedit:sem:tty.tty-getcharindex-fn]` · C: `src/tty.c` `tty_stty`
- class: UB · reach: cold — needs a platform defining `VBRK`, an `NDEBUG` build (common in distro packaging) and a `setty brk=` line.
- disposition: define — reject the unmapped index.
- status: open

**ERR-terminal-06** — `tty_stty`'s option loop tests `argv[0][2] == L'\0'` without checking the length first, so a one-character token `L"-"` reads one element past the end of the string.
- rule: `[spec:libedit:sem:tty.tty-stty-fn]` (step 4) · C: `src/tty.c` `tty_stty`
- class: UB · reach: cold — `setty -` in an `.editrc`.
- disposition: define — test the length.
- status: open

**ERR-terminal-07** — `tty_stty` passes `ct_encode_string(*argv++, ...)` to `strlcpy` without checking for NULL, which it returns for a NULL input, so an empty argument vector dereferences NULL.
- rule: `[spec:libedit:sem:tty.tty-stty-fn]` (step 2) · C: `src/tty.c` `tty_stty`
- class: UB · reach: cold — requires a caller supplying an empty `argv` through `el_set(EL_SETTY, ...)`.
- disposition: define.
- status: open

**ERR-terminal-08** — `re_insert`'s gap-opening loop compares a read pointer against `d + dat - 1`, forming a pointer one before the start of the array when `dat` is 0.
- rule: `[spec:libedit:sem:refresh.re-insert-fn]` (step 3) · C: `src/refresh.c` `re_insert`
- class: UB · reach: hot — every insert at column 0 of a row.
- disposition: define — express the loop over indices.
- status: open

**ERR-terminal-09** — stale literal sentinels: `re_refresh` calls `literal_clear` and then re-renders the prompt into `el_vdisplay`, but `el_display` still holds the *previous* frame's sentinel indices, which `terminal_move_to_char` re-emits through `terminal__putc` → `literal_get`. A prompt whose text changes between refreshes maps index *i* to different bytes; a prompt that produces fewer literals makes the index exceed `l_idx`, tripping the assertion or, under `NDEBUG`, reading out of bounds or dereferencing a NULL `l_buf` and handing the result to `fputs`.
- rule: `[spec:libedit:sem:literal.literal-clear-fn]`, `[spec:libedit:sem:literal.literal-get-fn]` · C: `src/literal.c`, `src/refresh.c`
- class: UB · reach: hot for any dynamic prompt (a clock, a git branch, a changing colour) — i.e. ordinary operation, not caller error.
- disposition: define — the observable behaviour for a *stable* prompt is frozen; for an unstable one the port picks a defined fallback (skip the cell or emit nothing).
- status: open

**ERR-terminal-10** — `literal_add` sizes its allocation with `ct_enc_width` (`wcrtomb` from a zeroed state) and then fills it with `ct_encode_char` (`wctomb` from libc's global state). In a stateful encoding `n` runs past `w` and the writes overflow the heap block; the remaining-space argument is computed as `(size_t)(w - n)`, so a negative difference becomes an enormous size and the internal guard cannot catch it. Symmetrically, a -1 return from `ct_encode_char` *decrements* `n` and can drive `b + n` before the start of the allocation.
- rule: `[spec:libedit:sem:literal.literal-add-fn]` (step 5) · C: `src/literal.c` `literal_add`
- class: UB · reach: cold — stateful `LC_CTYPE` only; exact in UTF-8 and single-byte locales.
- disposition: define — encode once and use the produced length.
- status: open

**ERR-terminal-11** — `el_display` and `el_vdisplay` are declared `wint_t **` but refresh.c casts them to `wchar_t *` before passing them to `re_update_line`, `re__copy_and_pad` and `terminal_overwrite`. On glibc that is a strict-aliasing violation, and the `EL_LITERAL` sentinel `0x80000000` round-trips through a signed `int` lvalue, which before C23 is only implementation-defined.
- rule: `[spec:libedit:sem:literal.literal-add-fn]` (width assumptions) · C: `src/refresh.c`, `src/el.h`
- class: UB · reach: hot — every refresh of a prompt containing a literal.
- disposition: define — carry the screen image as a single unsigned 32-bit element type end to end.
- status: open

**ERR-terminal-12** — `sig_handler` linear-scans a `{SIGINT, SIGTSTP, SIGQUIT, SIGHUP, SIGTERM, SIGCONT, SIGWINCH, -1}` table for `signo`; a signal not in the table stops on the terminator with `i == 7`, one past `sig_action[7]`, and the next step reads and writes there.
- rule: `[spec:libedit:sem:sig.sig-handler-fn]` (step 5) · C: `src/sig.c` `sig_handler`
- class: UB · reach: unreachable in practice — the handler is `static` and only ever installed for those seven.
- disposition: define — make the lookup total.
- status: open

**ERR-terminal-13** — `sig_handler`'s disposition restore is unconditional, so a slot still holding the `SIG_ERR` sentinel (because `sig_set`'s `sigaction` failed for that signal) is handed to `sigaction` as a handler.
- rule: `[spec:libedit:sem:sig.sig-handler-fn]` (step 6) · C: `src/sig.c` `sig_handler`
- class: UB · reach: cold — needs a failed `sigaction` during arming.
- disposition: define — treat "nothing saved" as "leave the disposition alone".
- status: open

**ERR-terminal-14** — `sig_handler` is not async-signal-safe: `el_resize` reaches `free()`/`calloc()` and `ioctl(TIOCGWINSZ)`, `terminal__flush` is `fflush` on a `FILE *`, `tty_rawmode` can reach `keymacro_delete` → `free()`, and the empty `sa_mask` lets a second trapped signal nest and re-enter all of it.
- rule: `[spec:libedit:sem:sig.sig-handler-fn]` (async-signal-safety) · C: `src/sig.c` `sig_handler`
- class: UB · reach: hot whenever `EL_SIGNAL` is enabled and a resize or resume arrives mid-allocation.
- disposition: define — record the signal number in an atomic (or write a byte to a self-pipe) and do the work in the read loop, where `read_char` already handles it.
- status: open

**ERR-terminal-15** — `sig_set` assigns only three members of the `struct sigaction` it hands to the kernel and `sig_init` allocates the signal block with `malloc`, leaving `sig_no` indeterminate.
- rule: `[spec:libedit:sem:sig.sig-set-fn]` (step 1), `[spec:libedit:sem:sig.sig-init-fn]` · C: `src/sig.c`
- class: UB · reach: harmless in practice (`SA_SIGINFO` is not set; `sig_no` is zeroed by `read_char` before it is read) but a genuine uninitialised read.
- disposition: define — initialise the whole action and zero `sig_no`.
- status: open

**ERR-terminal-16** — `prompt_print` never checks the pointer it gets back from the prompt callback. A callback returning NULL, or a narrow callback whose bytes are not a valid multibyte string in the current locale (so `ct_decode_string` returns NULL), reaches the walk and dereferences NULL.
- rule: `[spec:libedit:sem:prompt.prompt-print-fn]` (step 3) · C: `src/prompt.c` `prompt_print`
- class: UB · reach: hot for an application whose prompt callback can fail.
- disposition: define — treat a missing string as empty, render nothing, still record `p_pos`.
- status: open

**ERR-terminal-17** — a narrow prompt callback installed through `el_set(EL_PROMPT, ...)` has type `char *(*)(EditLine *)` but is stored in and called through `el_pfunc_t` (`wchar_t *(*)(EditLine *)`), an incompatible function-pointer type.
- rule: `[spec:libedit:sem:prompt.prompt-set-fn]`, `[spec:libedit:sem:prompt.prompt-print-fn]` (step 2) · C: `src/prompt.c`
- class: UB · reach: hot — every narrow-API application with a prompt.
- disposition: define — model the callback as a tagged (pointer, narrow-or-wide) pair at the ABI boundary.
- status: open

**ERR-terminal-18** — `sig_end` frees `el->el_signal` without ever calling `sig_clr`, and neither `sig_clr` nor `sig_end` clears the file-static `sel`. If the `EditLine` is destroyed without a matching `read_finish` (a `longjmp` out of a read, an `el_end` from a handler, `EL_SIGNAL` toggled off mid-read), libedit's handler is still installed for up to seven signals when the state it dereferences is freed, and `sel` dangles for the rest of the process.
- rule: `[spec:libedit:sem:sig.sig-end-fn]`, `[spec:libedit:sem:sig.sig-handler-fn]` (global-instance note) · C: `src/sig.c`
- class: memory (use-after-free) · reach: cold but real.
- disposition: define — restore dispositions first, clear the registration second, free last, as a property of the destructor.
- status: open

**ERR-terminal-19** — `literal_add`'s table-growth failure path calls libc `free` directly rather than `el_free`. With el.h's current macros the two are the same function, so there is no observable difference today.
- rule: `[spec:libedit:sem:literal.literal-add-fn]` (step 6) · C: `src/literal.c` `literal_add`
- class: memory · reach: OOM only; latent.
- disposition: fix — not observable.
- status: open

**ERR-terminal-20** — `terminal_set` stores the screen size into `el_terminal.t_size` with the two fields swapped (columns into `.v`, lines into `.h`); `terminal_settc` does the same. Masked only because `terminal_rebuffer_display` overwrites both correctly before anything reads them.
- rule: `[spec:libedit:sem:terminal.terminal-set-fn]` (step 8), `[spec:libedit:sem:terminal.terminal-settc-fn]` (step 7) · C: `src/terminal.c`
- class: logic · reach: latent — never observable in the shipped call graph.
- disposition: fix — the port simply does not do it.
- status: open

**ERR-terminal-21** — `terminal_set` returns -1 from its `terminal_change_size` failure path **without restoring the saved signal mask**, leaving `SIGWINCH` blocked for the rest of the process.
- rule: `[spec:libedit:sem:terminal.terminal-set-fn]` (step 10) · C: `src/terminal.c` `terminal_set`
- class: logic · reach: OOM during a terminal reload.
- disposition: fix — the rule calls it a bug; restore the mask on every exit path.
- status: open

**ERR-terminal-22** — `terminal_set` returns -1 whenever the capability-database lookup failed, even though dumb-terminal defaults were installed successfully and the `EditLine` is fully usable. `el_wset(EL_TERMINAL, ...)` propagates that.
- rule: `[spec:libedit:sem:terminal.terminal-set-fn]` (step 14) · C: `src/terminal.c` `terminal_set`
- class: logic · reach: hot on any host with an unknown `TERM`.
- disposition: reproduce — the return value crosses the ABI.
- status: open

**ERR-terminal-23** — `terminal_init` calls `terminal_set` (which calls `terminal_bind_arrow`) *before* `terminal_init_arrow` fills the function-key table, so the first `terminal_bind_arrow` sees a zero-filled table: NULL names, capability index 0 (the add-blank-line capability) and type `XK_CMD`. It is inert only because `map_init` has not run yet and the "is the key map built" guard fires first.
- rule: `[spec:libedit:sem:terminal.terminal-init-fn]` · C: `src/terminal.c` `terminal_init`
- class: logic · reach: latent — ordering accident, not currently observable.
- disposition: fix — install the arrow defaults before the first capability load.
- status: open

**ERR-terminal-24** — `terminal_bind_arrow` widens the capability bytes through a plain `char`, so on a signed-`char` platform a byte >= 0x80 becomes a negative wide character.
- rule: `[spec:libedit:sem:terminal.terminal-bind-arrow-fn]` (step 4b) · C: `src/terminal.c` `terminal_bind_arrow`
- class: logic · reach: cold — needs a terminal entry with a high-bit byte in a key capability.
- disposition: fix — widen through an unsigned byte (the rule directs this).
- status: open

**ERR-terminal-25** — `terminal_move_to_char`'s tab optimisation compares `el_cursor.h & 0370` against `where & ~0x7` and indexes the display at `where & 0370`. The two masks are not the same operation: `0370` also clears every bit above bit 7, so both the comparison and the index are wrong for columns of 256 or more.
- rule: `[spec:libedit:sem:terminal.terminal-move-to-char-fn]` (step 6b) · C: `src/terminal.c` `terminal_move_to_char`
- class: logic · reach: cold — needs a terminal wider than 255 columns and `TERM_CAN_TAB`, which is itself effectively dead (see ERR-terminal-58).
- disposition: fix — clear the low three bits consistently.
- status: open

**ERR-terminal-26** — the recorded cursor is updated on paths that emitted nothing: `terminal_move_to_char` sets `el_cursor.h = where` unconditionally at the end, including when `terminal_overwrite` bailed out on an oversized count, and `terminal_move_to_line` sets `el_cursor.v = where` even when neither up-capability exists and nothing was written.
- rule: `[spec:libedit:sem:terminal.terminal-move-to-char-fn]` (step 8), `[spec:libedit:sem:terminal.terminal-move-to-line-fn]` (steps 5c, 6) · C: `src/terminal.c`
- class: logic · reach: cold — terminals without cursor-up; oversized column requests.
- disposition: reproduce — the emitted byte stream is the observable behaviour.
- status: open

**ERR-terminal-27** — no wrap handling on two write paths: `terminal_clear_EOL`'s fallback adds `num` to `el_cursor.h` with no margin check (and a negative `num` moves it backwards), and `terminal_insertwrite`'s insert-mode strategy adds `num` up front and lets the recorded column run past `t_size.h`.
- rule: `[spec:libedit:sem:terminal.terminal-clear-eol-fn]`, `[spec:libedit:sem:terminal.terminal-insertwrite-fn]` (strategy B) · C: `src/terminal.c`
- class: logic · reach: hot on terminals without `clr_eol`, or with insert mode but no parameterised insert.
- disposition: reproduce.
- status: open

**ERR-terminal-28** — `terminal_insertwrite`'s one-character-at-a-time strategy degenerates into a plain overwrite with no insertion at all when `TERM_CAN_INSERT` was set by enter-insert-mode alone, without a matching exit-insert-mode. It also emits the insert-padding capability per character, where the insert-mode strategy emits it once for the whole run.
- rule: `[spec:libedit:sem:terminal.terminal-insertwrite-fn]` (strategy C) · C: `src/terminal.c` `terminal_insertwrite`
- class: logic · reach: cold — needs a terminal entry with `smir` but no `rmir`.
- disposition: reproduce.
- status: open

**ERR-terminal-29** — `terminal_overwrite`'s magic-margin path reads the display cell as a `wint_t` and stores it into a `wchar_t` before the recursive call, truncating on any platform where `wchar_t` is narrower.
- rule: `[spec:libedit:sem:terminal.terminal-overwrite-fn]` (step 4b) · C: `src/terminal.c` `terminal_overwrite`
- class: logic · reach: cold — needs `eat_newline_glitch` and a narrow `wchar_t`.
- disposition: define — the port carries one 32-bit cell type, so the truncation cannot arise.
- status: open

**ERR-terminal-30** — `terminal_settc` copies both the capability name and the value into 8-byte buffers with a truncating bounded copy, so a capability *string* longer than 7 bytes cannot be installed through `settc` or `el_set(EL_SETTC, ...)`.
- rule: `[spec:libedit:sem:terminal.terminal-settc-fn]` (step 2) · C: `src/terminal.c` `terminal_settc`
- class: logic · reach: hot for anyone overriding a real capability string.
- disposition: needs decision — the rule says "The port must keep that limit or change it deliberately, since it is user-visible."
- status: open

**ERR-terminal-31** — `terminal_settc`'s numeric path never calls `terminal_setflags`, so changing the destructive-tabs value does not update `TERM_CAN_TAB` until some later event recomputes the flags; and an empty numeric value is *accepted* as 0, because the string-to-long conversion consumes nothing and leaves the terminator at the first position.
- rule: `[spec:libedit:sem:terminal.terminal-settc-fn]` (steps 6, 9) · C: `src/terminal.c` `terminal_settc`
- class: logic · reach: cold — `settc xt` / `settc li ""` in an `.editrc`.
- disposition: reproduce.
- status: open

**ERR-terminal-32** — `terminal_change_size` restores the saved `el_cursor` after `re_clear_display` zeroed it, without revalidating it against the new dimensions, so after a shrink the recorded cursor can name a position off the screen.
- rule: `[spec:libedit:sem:terminal.terminal-change-size-fn]` (steps 5, 6) · C: `src/terminal.c` `terminal_change_size`
- class: logic · reach: hot — any terminal shrink.
- disposition: reproduce.
- status: open

**ERR-terminal-33** — `terminal_echotc`'s one-argument form passes the single user-supplied value as the **second** expansion parameter (the row) and forces the column to 0; the two-argument form's affected-line count is the row value, which may be 0 and then zeroes any per-affected-line padding.
- rule: `[spec:libedit:sem:terminal.terminal-echotc-fn]` (step 9) · C: `src/terminal.c` `terminal_echotc`
- class: logic · reach: cold — `echotc` from an `.editrc`.
- disposition: reproduce.
- status: open

**ERR-terminal-34** — `terminal_tputs` routes output through a file-static `FILE *` guarded by one file-static mutex, because the C `tputs` callback takes no user data. Two `EditLine` instances on different streams cannot emit capabilities concurrently.
- rule: `[spec:libedit:sem:terminal.terminal-tputs-fn]` · C: `src/terminal.c` `terminal_tputs`
- class: logic · reach: only under concurrency.
- disposition: fix — the port passes the writer as a parameter; the rule states the port must make concurrent instances safe, which the C could not guarantee.
- status: open

**ERR-terminal-35** — descriptor asymmetry: `tty_setup` decides whether to run at all with `isatty(el_outfd)`, while every `tcgetattr`/`tcsetattr` in tty.c uses `el_infd`, and `terminal_get_size`/`el_resize` query `TIOCGWINSZ` on `el_infd`. When the two descriptors differ and input is not a terminal the next call fails with `ENOTTY`, so it degrades safely rather than corrupting anything — but the split is real.
- rule: `[spec:libedit:sem:tty.tty-setup-fn]` (step 4), `[spec:libedit:sem:tty.tty-getty-fn]`, `[spec:libedit:sem:terminal.terminal-get-size-fn]`, `[spec:libedit:sem:el.el-resize-fn]` · C: `src/tty.c`, `src/terminal.c`
- class: logic · reach: only for `el_init_fd` callers whose descriptors differ.
- disposition: reproduce.
- status: open

**ERR-terminal-36** — `tty_get_signal_character` tests `ECHOCTL` against `c_iflag` (`MD_INP`) when `ECHOCTL` is a `c_lflag` bit. On glibc `ECHOCTL` has the same value as the input flag `IUCLC`, which libedit never sets, so the guard is always false and the function always returns -1 — making `rl_echo_signal_char` a silent no-op.
- rule: `[spec:libedit:sem:tty.tty-get-signal-character-fn]` (step 1) · C: `src/tty.c` `tty_get_signal_character`
- class: logic · reach: hot — every `rl_echo_signal_char` call on Linux.
- disposition: needs decision — the rule says reproducing freezes a broken observable while fixing changes it, and demands the choice be recorded.
- status: open

**ERR-terminal-37** — the same function indexes `el_tty.t_c[ED_IO]` with termios `V*` subscripts instead of libedit's `C_*` constants. They coincide by accident for `VINTR`/`VQUIT`; `VSUSP` is 10 while `C_SUSP` is 13, so `SIGTSTP` returns `t_c[ED_IO][C_START]`, the flow-control start character `^Q`.
- rule: `[spec:libedit:sem:tty.tty-get-signal-character-fn]` (step 2) · C: `src/tty.c` `tty_get_signal_character`
- class: logic · reach: masked today by ERR-terminal-36; live the moment that is fixed.
- disposition: needs decision — same rule, same reasoning.
- status: open

**ERR-terminal-38** — `tty_stty`'s `name=value` form writes only into the selected `struct termios`; `el_tty.t_c[z]` is not updated, so the next `tty_rawmode` that detects a control-character change silently reverts it via `tty__setchar`. `parse__escape`'s -1 for a malformed escape is stored as `(cc_t)-1` unchecked, and the name match compares a *wide* character count against *encoded bytes* and accepts any prefix of a longer table name.
- rule: `[spec:libedit:sem:tty.tty-stty-fn]` (steps 6c, 6e) · C: `src/tty.c` `tty_stty`
- class: logic · reach: hot for anyone using `setty erase=^H`-style lines.
- disposition: reproduce the observable outcome; the port must decide the three sub-behaviours explicitly rather than inherit them.
- status: open

**ERR-terminal-39** — `tty_end` clears neither `t_initialized` nor `t_mode`. If it runs while `t_mode` is `ED_IO` or `QU_IO` the terminal is cooked again but libedit still believes it is raw, and the next `tty_rawmode` returns 0 without re-applying anything — leaving the terminal cooked during editing.
- rule: `[spec:libedit:sem:tty.tty-end-fn]` · C: `src/tty.c` `tty_end`
- class: logic · reach: cold — the normal shutdown path runs `el_reset` first; the readline layer's `tty_end(e, TCSADRAIN)` per `readline()` call is the exposed route.
- disposition: reproduce.
- status: open

**ERR-terminal-40** — `tty_cookedmode` tests `EDIT_DISABLED` *after* the mode test, so an `EditLine` that was put into `ED_IO` and then had editing disabled never gets its terminal restored by this function.
- rule: `[spec:libedit:sem:tty.tty-cookedmode-fn]` (step 2) · C: `src/tty.c` `tty_cookedmode`
- class: logic · reach: cold — `el_set(EL_EDITMODE, 0)` mid-session.
- disposition: reproduce.
- status: open

**ERR-terminal-41** — `tty_setup` performs its only terminal write **before** setting `t_initialized`, so a failure there leaves the terminal modified while `tty_end` will later decline to restore it.
- rule: `[spec:libedit:sem:tty.tty-setup-fn]` (steps 9c, 13) · C: `src/tty.c` `tty_setup`
- class: logic · reach: cold — a failing `tcsetattr` at startup.
- disposition: reproduce.
- status: open

**ERR-terminal-42** — `tty_bind_char` does not special-case a disabled control character, so it binds the `_POSIX_VDISABLE` byte itself: on glibc/Linux that is byte 0x00 (NUL), on the BSDs 0xff. Since `t_c[ED_IO][C_EOF]` is `_POSIX_VDISABLE` by default, NUL ends up bound to `EM_DELETE_OR_LIST` / `VI_LIST_OR_EOF` on Linux.
- rule: `[spec:libedit:sem:tty.tty-bind-char-fn]` · C: `src/tty.c` `tty_bind_char`
- class: logic · reach: hot — the default configuration on every platform.
- disposition: reproduce, including the platform split — it is observable through the key map.
- status: open

**ERR-terminal-43** — libedit's fallback `CMIN`/`CTIME` defaults are `CEOF` (`^D`) and the disable byte, which are meaningless as `VMIN`/`VTIME`; harmless only because `EX_IO` is canonical, where both are ignored.
- rule: `[spec:libedit:sem:tty.tty-init-fn]` · C: `src/tty.h`, `src/tty.c`
- class: logic · reach: cold — only on platforms lacking the real definitions.
- disposition: reproduce.
- status: open

**ERR-terminal-44** — `tty__get_flag` calls `abort()` for `MD_CHAR` and `MD_NN`, terminating the process rather than returning an error.
- rule: `[spec:libedit:sem:tty.tty-get-flag-fn]` · C: `src/tty.c` `tty__get_flag`
- class: logic · reach: unreachable — every in-tree caller loops `MD_INP..MD_LIN`.
- disposition: fix — express the mapping as a total function over a four-variant enum.
- status: open

**ERR-terminal-45** — `re_putc` advances the virtual column by 1 for a zero-width printable (a combining mark), while `re_refresh_cursor` recomputes the same position with `ct_visual_width`, which returns `wcwidth` and therefore 0. The two disagree by one column per zero-width character on the line, and the cursor lands too far left.
- rule: `[spec:libedit:sem:refresh.re-putc-fn]` (step 5), `[spec:libedit:sem:refresh.re-refresh-cursor-fn]` · C: `src/refresh.c`
- class: logic · reach: hot in any locale where combining marks are typed.
- disposition: reproduce — the rule is explicit that a fix must change both sides together or neither.
- status: open

**ERR-terminal-46** — tab column accounting differs between the drawing path and the cursor path: `re_addc` emits spaces one at a time and stops the moment a wrap resets the column to 0, ending at column 0 of the next row; `re_refresh_cursor` advances to the next multiple of 8 first and applies the wrap subtraction afterwards. They agree only when the terminal width is a multiple of 8.
- rule: `[spec:libedit:sem:refresh.re-refresh-cursor-fn]` (mismatch (a)), `[spec:libedit:sem:refresh.re-addc-fn]` · C: `src/refresh.c`
- class: logic · reach: hot — a tab near the right margin on a terminal whose width is not a multiple of 8.
- disposition: reproduce.
- status: open

**ERR-terminal-47** — `re_nextline` scrolls `el_vdisplay` by rotating its row pointers but does not scroll `el_display`, so once the input is longer than the screen `re_update_line` diffs row *i* of the new virtual image against a stale row *i* of the real one.
- rule: `[spec:libedit:sem:refresh.re-nextline-fn]`, `[spec:libedit:sem:refresh.re-refresh-fn]` · C: `src/refresh.c` `re_nextline`
- class: logic · reach: hot for any line longer than the terminal height.
- disposition: reproduce — the rule says not to silently repair it, since doing so changes what is emitted.
- status: open

**ERR-terminal-48** — the mirror-image defect in the fast path: `re_fastputc`'s scroll case rotates `el_display`'s row pointers without rotating `el_vdisplay`, and deliberately leaves `el_cursor.v` where it is. Its non-scroll case blanks `el_display[r_oldcv]` rather than `el_display[el_cursor.v]`; the two coincide only when the counters agree.
- rule: `[spec:libedit:sem:refresh.re-fastputc-fn]` (step 5) · C: `src/refresh.c` `re_fastputc`
- class: logic · reach: hot on the last screen row.
- disposition: reproduce.
- status: open

**ERR-terminal-49** — `re_fastaddc`'s right-prompt bail-out is vacuous. After `re_refresh` drew an rprompt, `el_rprompt.p_pos.h` holds the column *after* it (`t_size.h - 1`), not its width, so the test evaluates to `1 - el_cursor.h`, which is always less than 3 — the fast path is therefore never taken while an rprompt is displayed.
- rule: `[spec:libedit:sem:refresh.re-fastaddc-fn]` (bail-out 3), `[spec:libedit:sem:refresh.re-refresh-fn]` (step 9) · C: `src/refresh.c` `re_fastaddc`
- class: logic · reach: hot whenever an rprompt is in use.
- disposition: reproduce — the rule notes it is load-bearing for correctness even though the test is vacuous.
- status: open

**ERR-terminal-50** — `re_fastaddc`/`re_fastputc` handle an embedded newline differently from the slow path: the character is written straight out and the recorded column advances by one, whereas `re_addc` would terminate the virtual row and move to the next one. With ONLCR the terminal performs a CR/LF and the recorded cursor diverges from the screen.
- rule: `[spec:libedit:sem:refresh.re-fastaddc-fn]` · C: `src/refresh.c` `re_fastaddc`
- class: logic · reach: cold — only by pushing a literal newline into the buffer, never by pressing Return.
- disposition: reproduce.
- status: open

**ERR-terminal-51** — `re_update_line` skips the second difference entirely when `sx < 0` but the on-screen column test fails: nothing is emitted for it, yet `re_refresh` immediately declares `el_display` equal to `el_vdisplay`. Screen and model diverge silently until something forces a full redraw.
- rule: `[spec:libedit:sem:refresh.re-update-line-fn]` (phase 4c) · C: `src/refresh.c` `re_update_line`
- class: logic · reach: cold — needs a deletion near the right margin.
- disposition: reproduce — the rule calls it a real hole in the algorithm and still says to reproduce it.
- status: open

**ERR-terminal-52** — `re_clear_lines` writes bare `'\r'` and `'\n'` through `terminal__putc`, bypassing `el_cursor` entirely, so the `terminal_move_to_line` calls that follow compute their motion from a stale recorded position. Coherent only because the caller runs `re_clear_display` immediately afterwards.
- rule: `[spec:libedit:sem:refresh.re-clear-lines-fn]` · C: `src/refresh.c` `re_clear_lines`
- class: logic · reach: hot — every `CC_REDISPLAY`.
- disposition: reproduce — the byte sequence the terminal receives is the observable behaviour.
- status: open

**ERR-terminal-53** — `re_clear_eol` computes `max(diff, |fx|, |sx|)` without clamping at zero; `diff` may be negative and `terminal_clear_EOL` with a negative count writes nothing but still moves its recorded column backwards.
- rule: `[spec:libedit:sem:refresh.re-clear-eol-fn]` · C: `src/refresh.c` `re_clear_eol`
- class: logic · reach: unreachable at the two current call sites, where `|fx|` or `|sx|` is at least 1.
- disposition: reproduce; do not rely on the invariant if the helper is reused.
- status: open

**ERR-terminal-54** — `sig_clr` does not reset a restored slot to `SIG_ERR`, so the saved dispositions survive into the next round. An application that enables `EL_SIGNAL` part-way through a read reaches `sig_clr` with no matching `sig_set` and libedit re-installs dispositions captured during an *earlier* `el_wgets`, clobbering whatever the application installed since.
- rule: `[spec:libedit:sem:sig.sig-clr-fn]` · C: `src/sig.c` `sig_clr`
- class: logic · reach: cold — needs `EL_SIGNAL` toggled mid-read.
- disposition: fix — the rule says to model each slot as an option and consume it on restore.
- status: open

**ERR-terminal-55** — libedit uses `sigprocmask` rather than `pthread_sigmask` in `sig_set`, `sig_clr`, `sig_init` and `el_resize`. POSIX leaves `sigprocmask` unspecified in a multi-threaded process, and the mask is per-thread while dispositions are process-wide.
- rule: `[spec:libedit:sem:sig.sig-clr-fn]`, `[spec:libedit:sem:sig.sig-set-fn]`, `[spec:libedit:sem:el.el-resize-fn]` · C: `src/sig.c`, `src/el.c`
- class: logic · reach: only in a threaded program.
- disposition: fix — not observable single-threaded.
- status: open

**ERR-terminal-56** — `sig_handler` de-installs itself for the signal it handled, and `sig_no` is only consulted after a *failed* read. A signal arriving while no read is in flight, or after the read returned its byte, leaves `sig_no` set but unacted-on and libedit un-rearmed, so the next `SIGWINCH`/`SIGCONT` goes to whatever handler libedit displaced. Redisplay after a resize is timing-dependent and can be lost entirely.
- rule: `[spec:libedit:sem:sig.sig-handler-fn]`, `[spec:libedit:sem:read.read-char-fn]` (signal interaction) · C: `src/sig.c`, `src/read.c`
- class: logic · reach: hot — two resizes in the wrong window.
- disposition: needs decision — the rule says a faithful port keeps the race or documents its divergence.
- status: open

**ERR-terminal-57** — `prompt_init` never assigns `p_wide`, which is 0 (meaning "narrow") only because the `EditLine` was `calloc`ed — while both functions it installs return `wchar_t *`. Until the application sets a prompt, `prompt_print` calls a wide function and decodes its result as a multibyte string, so the built-in default prompt `L"? "` renders as a bare `?` on a little-endian 4-byte-`wchar_t` platform, and as nothing at all on a big-endian one.
- rule: `[spec:libedit:sem:prompt.prompt-init-fn]`, `[spec:libedit:sem:prompt.prompt-default-fn]` · C: `src/prompt.c` `prompt_init`
- class: logic · reach: hot — every application that does not set a prompt.
- disposition: reproduce — the rule flags it as a user-visible behaviour change to be decided deliberately, and the conformance policy's default for defined behaviour is reproduce.
- status: open

**ERR-terminal-58** — `prompt_print` discards a literal region whose closing delimiter is the last character of the prompt (or which is never closed): the opening delimiter, the region and the closing delimiter are all dropped and nothing further is rendered. The C marks it `XXX: We lose the last literal`.
- rule: `[spec:libedit:sem:prompt.prompt-print-fn]` (step 4a) · C: `src/prompt.c` `prompt_print`
- class: logic · reach: hot for a prompt ending in a colour-reset sequence.
- disposition: reproduce — this is why the manual says the escape character may not be the last character of a prompt.
- status: open

**ERR-terminal-59** — prompt text outside a literal region goes through `re_putc` with no `ct_visual_char` expansion, no tab-stop handling and no newline handling, and a control character (`wcwidth` -1, folded to 0 then advanced by 1) is counted as exactly one column. A tab or newline in a prompt silently desynchronises the column accounting for the rest of the session.
- rule: `[spec:libedit:sem:prompt.prompt-print-fn]` · C: `src/prompt.c` `prompt_print`
- class: logic · reach: hot for any prompt containing raw control characters.
- disposition: reproduce.
- status: open

**ERR-terminal-60** — `re_refresh`'s throwaway rprompt measuring pass has real side effects: it registers its literals in the literal table (indices the drawing pass then never uses), and an rprompt wider than the terminal scrolls the virtual display through `re_nextline` before any real content is drawn. A right-prompt callback that returns different strings on its two calls within one refresh is measured at one width and drawn at another.
- rule: `[spec:libedit:sem:prompt.prompt-print-fn]` (how often the callback runs), `[spec:libedit:sem:refresh.re-refresh-fn]` (step 3) · C: `src/refresh.c` `re_refresh`
- class: logic · reach: hot for any application using an rprompt.
- disposition: reproduce.
- status: open

**ERR-terminal-61** — `tgetflag("pt")` has no terminfo counterpart: terminfo expresses hardware tabbing as the *string* capability `tab_to_next_stop`, not a boolean, so on an ncurses-backed system the C already returns 0 for every terminal and `TERM_CAN_TAB` is effectively dead.
- rule: `[spec:libedit:sem:terminal.tgetflag-fn]` · C: `src/terminal.c` `terminal_set` / `terminal_setflags`
- class: divergence · reach: universal on terminfo systems.
- disposition: needs decision — `[dec:libedit:conformance-policy]` names the physical-tabs capability as one of the six forks defaulting to *reproduce* (always false), while `[dec:libedit:terminal-caps-via-term-crate]` flags the choice as unresolved.
- status: open

**ERR-terminal-62** — `xt` (destructive tabs) maps only onto terminfo `dest_tabs_magic_smso`, which conflates tab destruction with the Teleray magic-standout quirk; `MT` is a termcap-only meta-key extension with no terminfo counterpart and already reads 0 under ncurses.
- rule: `[spec:libedit:sem:terminal.tgetflag-fn]` · C: `src/terminal.c` `terminal_set`
- class: divergence · reach: universal on terminfo systems.
- disposition: needs decision — flagged unresolved by `[dec:libedit:terminal-caps-via-term-crate]`.
- status: open

**ERR-terminal-63** — `tgoto` takes its parameters column-first while terminfo's two-parameter cursor addressing (`cup`) takes row first, a discrepancy the C's termcap emulation hides by swapping at the boundary; and `term`'s parameter expander recognises `$<...>` delay markers and **discards** them, so a parameterised capability would silently lose its padding.
- rule: `[spec:libedit:sem:terminal.tgoto-fn]`, `[spec:libedit:sem:terminal.tputs-fn]` · C: `src/terminal.c` (all `tgoto` call sites)
- class: divergence · reach: the order is exposed only through user-supplied `echotc`; the padding loss affects every parameterised motion/insert/delete.
- disposition: needs decision — called out as unresolved in `[dec:libedit:terminal-caps-via-term-crate]`.
- status: open

**ERR-terminal-64** — `editline.3` lists `SIGSTOP` among the signals `EL_SIGNAL` traps. `sig_init` traps exactly seven and `SIGSTOP` is not among them; it cannot be caught or blocked.
- rule: `[spec:libedit:sem:sig.sig-init-fn]` (step 2) · C: `doc/editline.3`
- class: divergence (documentation) · reach: documentation only.
- disposition: fix the documentation; the code is correct.
- status: open

**ERR-terminal-65** — dead and disabled code in this layer: `sig_handler`'s `if (ed_redisplay(sel, 0) == CC_REFRESH) re_refresh(sel);` is unreachable because `ed_redisplay` always returns `CC_REDISPLAY`; `re_addc`'s `CHTYPE_NL` guard on an unchanged `r_cursor.v` is always true (flagged `XXX` in the C); `tty_printchar` is behind `#ifdef notyet`, has no callers and does not compile; `terminal_echotc`'s two-argument arm re-tests the same parse result a third time; `re_refresh`'s long-line start block is `#if notyet`; refresh.c's dual-encode debug block is permanently disabled.
- rule: `[spec:libedit:sem:sig.sig-handler-fn]`, `[spec:libedit:sem:refresh.re-addc-fn]`, `[spec:libedit:sem:tty.tty-printchar-fn]`, `[spec:libedit:sem:terminal.terminal-echotc-fn]`, `[spec:libedit:sem:refresh.re-refresh-fn]`, `[spec:libedit:sem:chartype.ct-encode-string-fn]` · C: `src/sig.c`, `src/refresh.c`, `src/tty.c`, `src/terminal.c`
- class: dead · reach: unreachable.
- disposition: fix — not ported.
- status: open

---

## buffer

`src/chared.c` — the line buffer and the kill, undo and redo buffers, plus the
public entry points that manipulate them.

**ERR-buffer-01** — `c_delafter`'s gap-closing loop runs from `cursor` to `lastchar` inclusive assigning `*cp = cp[num]`, so its reads reach as far as `lastchar + num`. For a large `num` on a nearly full line that is past the end of the line allocation — worst case, deleting a full-capacity line from offset 0 reads roughly one whole buffer past the end.
- rule: `[spec:libedit:sem:chared.c-delafter-fn]` · C: `src/chared.c` `c_delafter`
- class: UB · reach: hot — any `d$`, `x` with a large count, or `em_delete_next_word` near the start of a long line.
- disposition: define — copy only the in-range tail; the contents left above the new `lastchar` are unspecified.
- status: open

**ERR-buffer-02** — `c_delbefore1` starts its loop at `cursor - 1` with no guard, so `cursor == buffer` forms a pointer before the line buffer and the first assignment writes one element before its start.
- rule: `[spec:libedit:sem:chared.c-delbefore1-fn]` · C: `src/chared.c` `c_delbefore1`
- class: UB · reach: caller-guaranteed today (`em_delete_prev_char` and `vi_delete_prev_char` both check first), but the function itself defends against nothing.
- disposition: define — make the precondition explicit.
- status: open

**ERR-buffer-03** — `c_delafter1` has no guards either: with `cursor == lastchar` the loop still executes and `lastchar` still decrements, leaving `lastchar == cursor - 1`; on an empty line `lastchar` ends up before `buffer` entirely.
- rule: `[spec:libedit:sem:chared.c-delafter1-fn]` · C: `src/chared.c` `c_delafter1`
- class: UB · reach: caller-guaranteed today.
- disposition: define.
- status: open

**ERR-buffer-04** — `c_hpos` scans backwards while `ptr >= buffer`, so a line with no embedded newline leaves `ptr == buffer - 1`; merely forming that pointer is undefined.
- rule: `[spec:libedit:sem:chared.c-hpos-fn]` · C: `src/chared.c` `c_hpos`
- class: UB · reach: hot — every `ed_prev_line`/`ed_next_line` on a single-line buffer.
- disposition: define — scan over indices and treat "no newline found" as the column `cursor - buffer`.
- status: open

**ERR-buffer-05** — `c__prev_word` can leave its working pointer at `low - 1` between iterations; the C forms that pointer without dereferencing it.
- rule: `[spec:libedit:sem:chared.c-prev-word-fn]` · C: `src/chared.c` `c__prev_word`
- class: UB · reach: hot — `M-b`/`^W` at the start of the line.
- disposition: define — scan over a signed or saturating index.
- status: open

**ERR-buffer-06** — `cv_prev_word` step 2b samples `wtest(el, *p)` with only a `p > low` guard behind it, so entering with `p == low` reads `*(low - 1)`, one element before the line buffer. The value never reaches the result, but the read is out of bounds.
- rule: `[spec:libedit:sem:chared.cv-prev-word-fn]` · C: `src/chared.c` `cv_prev_word`
- class: UB · reach: hot — vi `b`/`B` with the cursor on the first character.
- disposition: define — return `low` as soon as the position falls below it and never form that position.
- status: open

**ERR-buffer-07** — `cv__endword` (step 2b) and `cv_next_word` (step 1) classify `*p` without first testing `p < high`, so a scan that has already reached `high` reads the reserved slot at `lastchar` — inside the allocation but holding stale data.
- rule: `[spec:libedit:sem:chared.cv-endword-fn]`, `[spec:libedit:sem:chared.cv-next-word-fn]` · C: `src/chared.c`
- class: UB (indeterminate read) · reach: hot — vi `w`/`e` with the cursor at end of line.
- disposition: define — treat "already at `high`" as classifying nothing; the value cannot affect the result.
- status: open

**ERR-buffer-08** — `ch_enlargebufs` rebases `cursor`, `lastchar`, `c_kill.last`, `c_kill.mark`, `c_redo.pos` and `c_redo.lim` by reading the *old* pointer values after the `realloc` that invalidated them.
- rule: `[spec:libedit:sem:chared.ch-enlargebufs-fn]` · C: `src/chared.c` `ch_enlargebufs`
- class: UB · reach: hot — every line that outgrows 1024 characters.
- disposition: define — hold offsets rather than pointers across the growth, keeping the two-phase `limit` update's observable effect (after a failure the capacity is still the pre-call one).
- status: open

**ERR-buffer-09** — `cv_yank` computes its `memcpy` length as `size * sizeof(wchar_t)` with `size` an `int`, so a negative `size` becomes an enormous `size_t`. A negative `num` survives `c_delafter`/`c_delbefore`'s clamp and is handed to `cv_yank` *before* step 3 rejects it.
- rule: `[spec:libedit:sem:chared.cv-yank-fn]`, `[spec:libedit:sem:chared.c-delafter-fn]`, `[spec:libedit:sem:chared.c-delbefore-fn]` · C: `src/chared.c` `cv_yank`
- class: UB · reach: no in-tree caller passes a negative count.
- disposition: define — require a non-negative count.
- status: open

**ERR-buffer-10** — `c_gets` writes into the line buffer without ever checking `el->el_line.limit`: `cp` can reach `buffer + wcslen(prompt) + 1008` and one more character is stored there, so a prompt longer than 15 characters combined with maximal input runs past the 1024-slot initial line buffer.
- rule: `[spec:libedit:sem:chared.c-gets-fn]` · C: `src/chared.c` `c_gets`
- class: UB · reach: cold — the two in-tree callers use prompts of 2 and 3 characters; reachable if `c_gets` is ever called with a longer one.
- disposition: define — bound the writes by `limit`.
- status: open

**ERR-buffer-11** — `el_cursor` adds `n` to the cursor pointer and only then clamps, transiently forming a pointer far outside the line allocation.
- rule: `[spec:libedit:sem:chared.el-cursor-fn]`, `[spec:libedit:sem:histedit.el-cursor-fn]` · C: `src/chared.c` `el_cursor`
- class: UB · reach: hot — any caller passing a large `n`.
- disposition: define — saturating index arithmetic; the clamped return value and the `n == 0` short-circuit are frozen at the ABI.
- status: open

**ERR-buffer-12** — `el_winsertstr` casts `wcslen(s)` from `size_t` to `int` for `c_insert`; a string longer than `INT_MAX` yields a negative count, and `c_insert` with a negative count shifts the tail left and writes below `cursor`, possibly below `buffer`.
- rule: `[spec:libedit:sem:chared.el-winsertstr-fn]`, `[spec:libedit:sem:chared.c-insert-fn]` · C: `src/chared.c`
- class: UB · reach: unreachable in practice.
- disposition: define — carry an unsigned count.
- status: open

**ERR-buffer-13** — `ch_init`'s error handling is inconsistent: the step-3 failure (undo buffer) returns -1 **without freeing the line buffer allocated in step 1**, leaking it while `el->el_line.buffer` still points at live memory, whereas the step-4 and step-6 failures unwind through `ch_end`.
- rule: `[spec:libedit:sem:chared.ch-init-fn]` · C: `src/chared.c` `ch_init`
- class: memory · reach: OOM only.
- disposition: fix — the rule says to note it rather than copy it blindly; the leak is not ABI-observable.
- status: open

**ERR-buffer-14** — `ch_end` frees `c_kill.buf` and NULLs it, but neither `ch_end` nor `ch_reset` touches `c_kill.last`, which is left pointing into the freed kill buffer.
- rule: `[spec:libedit:sem:chared.ch-end-fn]` · C: `src/chared.c` `ch_end`
- class: memory · reach: every `el_end`; nothing reads it before the next `ch_init` or `cv_yank`.
- disposition: fix — null it; the rule states that is not observable.
- status: open

**ERR-buffer-15** — `el_deletestr1`'s copy loop moves `min(end - start, line_length - end)` characters and shortens the line by that same clamped count, instead of moving the whole tail `[end, line_length)` and shortening by `end - start`. Both failure modes are reachable: with a long tail the length is right but the content is not (`abcdefgh`, `start=1`, `end=3` yields `adedef`); with a short tail the tail moves correctly but the line is left too long, exposing stale characters (`abcdefgh`, `start=1`, `end=6` yields `aghdef` where `agh` is correct).
- rule: `[spec:libedit:sem:chared.el-deletestr1-fn]`, `[spec:libedit:sem:histedit.el-deletestr1-fn]` · C: `src/chared.c` `el_deletestr1`
- class: logic · reach: hot — every `rl_delete_text` call from a readline consumer.
- disposition: reproduce — named explicitly in `[dec:libedit:conformance-policy]` as one of the six forks defaulting to reproduce; the rule forbids resolving it silently by writing the obvious correct loop.
- status: open

**ERR-buffer-16** — `el_deletestr1` rejects `end >= line_length` rather than clamping, so a range ending exactly at the end of the line is refused and the final character of the line can never be deleted through this entry point.
- rule: `[spec:libedit:sem:chared.el-deletestr1-fn]` (step 3) · C: `src/chared.c` `el_deletestr1`
- class: logic · reach: hot — `rl_delete_text(n, rl_end)`.
- disposition: reproduce.
- status: open

**ERR-buffer-17** — `el_deletestr1` returns `end - start`, the size of the range requested, regardless of how many characters were actually removed, so for a range near the end of the line the return over-reports.
- rule: `[spec:libedit:sem:chared.el-deletestr1-fn]` (step 7), `[spec:libedit:sem:histedit.el-deletestr1-fn]` · C: `src/chared.c` `el_deletestr1`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-buffer-18** — `el_deletestr1` never adjusts the cursor for the deletion; it only clamps at the low end, so the cursor can be left pointing above the new `lastchar` or at a different character than before.
- rule: `[spec:libedit:sem:chared.el-deletestr1-fn]` (step 6), `[spec:libedit:sem:histedit.el-deletestr1-fn]` · C: `src/chared.c` `el_deletestr1`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-buffer-19** — `c_insert` never blanks the gap it opens, so the `num` slots at `[cursor, cursor + num)` retain shifted-away text or `calloc` zeros. When `cursor == lastchar` the shift is skipped entirely and appending simply exposes `num` stale slots.
- rule: `[spec:libedit:sem:chared.c-insert-fn]` · C: `src/chared.c` `c_insert`
- class: logic · reach: hot; harmless while every caller fills the gap, exposed when one does not (see ERR-modes-24, ERR-modes-41).
- disposition: reproduce — the contents are unspecified either way.
- status: open

**ERR-buffer-20** — `ch_enlargebufs` treats the redo buffer differently from the other three: its newly added tail is **not** zeroed, and `c_redo.lim` keeps its old offset, so the redo buffer's usable limit does not grow even though its allocation does.
- rule: `[spec:libedit:sem:chared.ch-enlargebufs-fn]` (step 7) · C: `src/chared.c` `ch_enlargebufs`
- class: logic · reach: hot for vi users on lines longer than 1024 characters.
- disposition: reproduce — the rule records the asymmetry as deliberate to record, not necessarily to keep, but the capped redo limit is observable through vi `.`.
- status: open

**ERR-buffer-21** — `ce__isword` and `cv__isword` look the character up with `wcschr` in `el_map.wordchars`, and `wcschr` matches the terminating NUL, so `L'\0'` is reported as a word character.
- rule: `[spec:libedit:sem:chared.ce-isword-fn]`, `[spec:libedit:sem:chared.cv-isword-fn]` · C: `src/chared.c`
- class: logic · reach: cold — the word scanners do not normally look past `lastchar`, but any classification at the buffer edge hits it (see ERR-buffer-07).
- disposition: reproduce — the rule says a port that classifies characters at the buffer edge must reproduce this to stay bit-identical.
- status: open

**ERR-buffer-22** — `c__next_word` and `c__prev_word` consume the repeat count as `while (n--)`, so a negative `n` counts down toward `INT_MIN` rather than stopping — an effectively unbounded loop (`c__prev_word` would spin roughly 2^32 trivial iterations).
- rule: `[spec:libedit:sem:chared.c-next-word-fn]`, `[spec:libedit:sem:chared.c-prev-word-fn]`, `[spec:libedit:sem:common.ed-delete-prev-word-fn]` · C: `src/chared.c`
- class: logic · reach: not reachable through the key dispatcher.
- disposition: define — treat a non-positive count as no movement.
- status: open

**ERR-buffer-23** — dead defensive code in this layer: `el_deletestr1`'s `if (cursor < buffer) cursor = buffer`; `el_deletestr`'s step-5 clamp, which step 2 already guarantees cannot fire; `c__next_word`'s post-loop clamp to `high`, unreachable because both loops stop at `high`; `ed_delete_prev_word`'s re-clamp to `buffer` after `c__prev_word` has already clamped.
- rule: `[spec:libedit:sem:chared.el-deletestr1-fn]`, `[spec:libedit:sem:chared.el-deletestr-fn]`, `[spec:libedit:sem:chared.c-next-word-fn]`, `[spec:libedit:sem:common.ed-delete-prev-word-fn]` · C: `src/chared.c`, `src/common.c`
- class: dead · reach: unreachable.
- disposition: fix — not ported.
- status: open

---

## input

`src/read.c`, `src/keymacro.c`, `src/parse.c`, `src/tokenizer.c` — reading
characters, the multi-character binding trie, the editrc escape grammar and the
word splitter.

**ERR-input-01** — `read_init` assigns `ma->level = -1` *after* the macro-slot allocation that can fail. On that failure it calls `read_end` → `read_clearmacros`, which loops `while (ma->level >= 0) el_free(ma->macro[ma->level--])` on an indeterminate `level` from `malloc`, dereferencing a NULL `ma->macro` and/or passing garbage pointers to `free`.
- rule: `[spec:libedit:sem:read.read-init-fn]` · C: `src/read.c` `read_init`
- class: UB · reach: OOM only, but reachable purely by allocation failure.
- disposition: define — set `level` before anything can fail, or use a representation whose default is "empty".
- status: open

**ERR-input-02** — `el_wgets`'s `UNBUFFERED` `CC_EOF` arm appends `CONTROL('d')` at `el->el_line.lastchar++` with no `limit` check and no `ch_enlargebufs` call, overrunning the line buffer if the line is already at capacity.
- rule: `[spec:libedit:sem:read.el-wgets-fn]` (dispatch table, `CC_EOF`) · C: `src/read.c` `el_wgets`
- class: UB · reach: cold — `UNBUFFERED` returns after every command, so the line is normally short.
- disposition: define — bound the append.
- status: open

**ERR-input-03** — `node_trav` dereferences its node pointer with no NULL check, so `keymacro_get` on an empty trie (`el_keymacro.map == NULL`) dereferences NULL. The C relies on the invariant that a character only reaches the trie when its action table entry is `ED_SEQUENCE_LEAD_IN`, maintained by convention on the delete paths.
- rule: `[spec:libedit:sem:keymacro.keymacro-get-fn]`, `[spec:libedit:sem:keymacro.node-trav-fn]` · C: `src/keymacro.c`
- class: UB · reach: cold — needs the table and the trie to fall out of step, which `map_bind`'s multi-character `-r` path can do (it deletes the trie entry without clearing the lead-in marker).
- disposition: define — return "no match" for an empty trie.
- status: open

**ERR-input-04** — `node__get`'s NULL return is not checked at any of its three call sites (`keymacro_add` step 3, `node__try` steps 1b and 3); each immediately stores it into a link field and dereferences it.
- rule: `[spec:libedit:sem:keymacro.node-get-fn]`, `[spec:libedit:sem:keymacro.keymacro-add-fn]`, `[spec:libedit:sem:keymacro.node-try-fn]` · C: `src/keymacro.c`
- class: UB · reach: OOM during a bind.
- disposition: define — propagate the failure.
- status: open

**ERR-input-05** — `node_lookup` writes the closing quote and terminator at `px` and `px + 1` with no bounds check. `ct_visual_char` only guarantees `cnt + used <= KEY_BUFSIZ`, so a key whose rendering exactly fills the 1024-`wchar_t` buffer writes one or two elements past its end.
- rule: `[spec:libedit:sem:keymacro.node-lookup-fn]` · C: `src/keymacro.c` `node_lookup`
- class: UB · reach: cold — needs a bound sequence rendering to ~1024 characters.
- disposition: define — bounds-check before writing.
- status: open

**ERR-input-06** — `node_enum` does not check `ct_visual_char`'s return. Its own guard only reserves six free `wchar_t` while a non-BMP non-printable needs eight, so a -1 return makes `cnt + (size_t)used` equal `cnt - 1` and the subsequent writes land one slot early, over the previous character.
- rule: `[spec:libedit:sem:keymacro.node-enum-fn]` (step 3) · C: `src/keymacro.c` `node_enum`
- class: UB · reach: cold — a very long bound sequence ending in a non-BMP character.
- disposition: define — check for -1 and bail out the way `node_lookup` does.
- status: open

**ERR-input-07** — `keymacro_kprint`'s `XK_CMD` scan is written `for (fp = el->el_map.help; fp->name; fp++)`, terminating on a NULL `name` sentinel that the generated help table does not contain. `el_map.help` is an exactly-sized array of `el_map.nfunc` entries, so an unmatched command walks off the end and keeps reading until it happens on a zero word.
- rule: `[spec:libedit:sem:keymacro.keymacro-kprint-fn]` · C: `src/keymacro.c` `keymacro_kprint`
- class: UB · reach: cold — not reachable with a legitimately bound command, since every valid action has a help entry; reachable via the truncating command-number cast (ERR-modes-31).
- disposition: define — bound the scan by `el_map.nfunc`.
- status: open

**ERR-input-08** — `keymacro_kprint` converts the key with `ct_encode_string(key, &el->el_scratch)` and hands the result straight to `%s`; on allocation failure that is NULL.
- rule: `[spec:libedit:sem:keymacro.keymacro-kprint-fn]` · C: `src/keymacro.c` `keymacro_kprint`
- class: UB · reach: OOM only (glibc prints `(null)`).
- disposition: define.
- status: open

**ERR-input-09** — `keymacro__decode_str` does not handle `len == 0`: `eb == buf == b`, so the opening-separator `ADDC` pushes `b` past `eb`, step 3b then computes `(size_t)(eb - b)` as `SIZE_MAX` and `ct_encode_char` writes out of bounds, and the final forced termination writes `buf[-1]`.
- rule: `[spec:libedit:sem:keymacro.keymacro-decode-str-fn]` · C: `src/keymacro.c` `keymacro__decode_str`
- class: UB · reach: unreachable today — all in-tree callers pass `sizeof` of an `EL_BUFSIZ` array.
- disposition: define — reject `len == 0` explicitly.
- status: open

**ERR-input-10** — `parse__escape`'s `\U+` form looks each character up with `wcschr` in `L"0123456789ABCDEF"`, and `wcschr` counts the table's own terminating NUL, so an input NUL is accepted as hex digit **16**. Four hex digits at end of string therefore do not fail: `\U+0041` returns 0x410, `\U+ABCD` returns 0xABCD0, and the cursor is left two elements past the terminator. With one to three digits the remaining iterations dereference past the terminator outright.
- rule: `[spec:libedit:sem:parse.parse-escape-fn]` (form C) · C: `src/parse.c` `parse__escape`
- class: UB · reach: hot — an ordinary `bind '\U+0041' ed-insert` line in an `.editrc`.
- disposition: define — require four or five uppercase hex digits and treat end of string inside the run as a malformed escape.
- status: open

**ERR-input-11** — `parse__escape`'s first statement is `if (p[1] == 0) return -1;`, reading `p[1]` without first checking `p[0]`, so calling it on an empty string reads one element past the terminator.
- rule: `[spec:libedit:sem:parse.parse-escape-fn]` · C: `src/parse.c` `parse__escape`
- class: UB · reach: both in-tree callers guarantee `p[0] != 0`.
- disposition: define — treat "at least one character" as a precondition.
- status: open

**ERR-input-12** — `parse__string` resumes decoding past the end of its input after a `\U+` escape at end of string leaves `parse__escape`'s cursor beyond the terminator, and keeps decoding adjacent memory until it happens on a zero.
- rule: `[spec:libedit:sem:parse.parse-string-fn]` · C: `src/parse.c` `parse__string`
- class: UB · reach: inherited from ERR-input-10; same trigger.
- disposition: define — falls out of fixing ERR-input-10.
- status: open

**ERR-input-13** — `parse_line` does not check `tok_winit`'s NULL return; `tok_wstr` then dereferences NULL.
- rule: `[spec:libedit:sem:parse.parse-line-fn]` · C: `src/parse.c` `parse_line`
- class: UB · reach: OOM only.
- disposition: define — report the failure.
- status: open

**ERR-input-14** — `parse_line` **discards** `tok_wstr`'s return value. On an unmatched quote (1 or 2), a dangling backslash-newline (3) or an internal allocation failure (-1) the tokenizer returns without writing `*argc` or `*argv`, which `parse_line` declares as uninitialised locals and hands to `el_wparse`. An `.editrc` line such as `bind 'foo` passes an indeterminate `argc` and a wild `argv`.
- rule: `[spec:libedit:sem:parse.parse-line-fn]`, `[spec:libedit:sem:tokenizer.fun-tok-str-fn]` · C: `src/parse.c` `parse_line`
- class: UB · reach: hot — one mistyped quote in an `.editrc`.
- disposition: define — check the tokenizer result and report the malformed line as -1.
- status: open

**ERR-input-15** — `tok_line`'s end-of-input substitution points its cursor at a static one-element empty string and the loop then increments past it; whether the substitution fires again depends on comparing that out-of-object pointer against `line->lastchar`, which C does not define. When the input ends in `Q_one`/`Q_doubleone` — a trailing backslash with no newline — the loop walks off the literal and keeps tokenizing whatever read-only memory follows. Measured on gcc/glibc x86-64: a heap-allocated `"abc\"` reads a few hundred bytes past the literal.
- rule: `[spec:libedit:sem:tokenizer.fun-tok-line-fn]` (the end-of-input overrun) · C: `src/tokenizer.c` `tok_line`
- class: UB · reach: hot — any inputrc/editrc line ending in a backslash, via `rl_parse_and_bind` and `parse_line`.
- disposition: define — take the intended reading: a trailing backslash at end of input is dropped, the parse completes, 0 is returned; the extra embedded NUL (observable only in `co`) is the frozen part.
- status: open

**ERR-input-16** — `read_end` dereferences `el->el_read` without a NULL check, so calling it twice, or before `read_init` has run, faults. The step-3 assignment to NULL makes a double call fail loudly rather than double-free — but only because the null dereference in step 1 comes first.
- rule: `[spec:libedit:sem:read.read-end-fn]` · C: `src/read.c` `read_end`
- class: UB · reach: hot on the `read_init`-failure path (see ERR-core-api-03).
- disposition: define — tolerate an uninitialised read subsystem.
- status: open

**ERR-input-17** — `read_pop` has no `ma->level >= 0` guard: calling it on an empty queue frees `macro[0]` a second time and drives `level` to -2.
- rule: `[spec:libedit:sem:read.read-pop-fn]` · C: `src/read.c` `read_pop`
- class: UB · reach: latent — both call sites satisfy the precondition today.
- disposition: define — make the precondition explicit.
- status: open

**ERR-input-18** — `keymacro_end` frees the trie with `node__free`, which frees node structs only and never looks at `type`, so every macro string bound with `XK_STR` (each `wcsdup`ed by `node__try`) is leaked when the `EditLine` is destroyed. `keymacro_reset` and `keymacro_delete`, which go through `node__put`, do free them; only the teardown path leaks.
- rule: `[spec:libedit:sem:keymacro.keymacro-end-fn]`, `[spec:libedit:sem:keymacro.node-free-fn]` · C: `src/keymacro.c` `keymacro_end`
- class: memory · reach: hot — every `el_end` after any `bind -s`.
- disposition: fix — the rule states the absence of the leak is not observable across the C ABI.
- status: open

**ERR-input-19** — `keymacro_end` does not set `el_keymacro.map` to NULL after freeing the tree, so a second `keymacro_end` on the same `EditLine` double-frees the whole trie (the `buf` free is idempotent, the tree free is not).
- rule: `[spec:libedit:sem:keymacro.keymacro-end-fn]` · C: `src/keymacro.c` `keymacro_end`
- class: memory · reach: harmless in the shipped flow, since `el_end` frees the `EditLine` immediately afterwards.
- disposition: fix — clearing it is unobservable.
- status: open

**ERR-input-20** — `read_clearmacros` leaves the freed slots holding dangling pointers, and `read_pop`'s shift leaves a stale duplicate of the top pointer above the new `level`. Safe only because both are unreachable while `level` bounds them.
- rule: `[spec:libedit:sem:read.read-clearmacros-fn]`, `[spec:libedit:sem:read.read-pop-fn]` · C: `src/read.c`
- class: memory · reach: latent — a port that eagerly frees the whole array would double-free.
- disposition: fix — clear the slots.
- status: open

**ERR-input-21** — `read__fixio`'s would-block recovery **permanently** clears `O_NONBLOCK`/`O_NDELAY` on the caller's input descriptor, normally the process's shared standard input. Nothing is saved and nothing is restored on the way out of `el_wgets`.
- rule: `[spec:libedit:sem:read.read-fixio-fn]` · C: `src/read.c` `read__fixio`
- class: logic · reach: only with `EL_SAFEREAD` enabled and a non-blocking descriptor.
- disposition: needs decision — the rule says the port "must either reproduce this or treat it as a deliberate, documented divergence".
- status: open

**ERR-input-22** — `el_wgets`'s `EDIT_DISABLED` path runs *after* `read_prepare` (so `sig_set` has installed handlers) but returns through `noedit_wgets` and never reaches `read_finish`, so the handlers stay installed after `el_wgets` returns and the tty is never put back into cooked mode. `EDIT_DISABLED` leaks signal dispositions on every call.
- rule: `[spec:libedit:sem:read.el-wgets-fn]` (step 6), `[spec:libedit:sem:read.read-finish-fn]` · C: `src/read.c` `el_wgets`
- class: logic · reach: hot for any `EL_EDITMODE 0` application with `EL_SIGNAL` on.
- disposition: needs decision — the rule says the port must decide whether to reproduce the leak, since the application's own `SIGINT` handler is displaced and that is observable.
- status: open

**ERR-input-23** — under `EL_UNBUFFERED`, any first keystroke that neither inserts text nor completes the line (a cursor move, a beep, a failed search) leaves the line empty with `num == -1`, so `el_wgets` reports `*nread == -1` — end of file — even though nothing failed.
- rule: `[spec:libedit:sem:read.el-wgets-fn]` (`*nread` on every exit path) · C: `src/read.c` `el_wgets`
- class: logic · reach: hot for any unbuffered consumer.
- disposition: reproduce — the rule calls it a genuine trap and observable behaviour a port must reproduce.
- status: open

**ERR-input-24** — `el_wgetc` returns 0 when `tty_rawmode` fails, so a terminal-setup failure is reported as end of file and is indistinguishable from one by any caller. `read_errno` is not set and `*cp` is not written.
- rule: `[spec:libedit:sem:read.el-wgetc-fn]` (step 3) · C: `src/read.c` `el_wgetc`
- class: logic · reach: cold.
- disposition: reproduce — every caller tests `!= 1` anyway.
- status: open

**ERR-input-25** — `read_char` discards an invalid lone byte and `goto again`s without reporting anything, so invalid input is skipped rather than surfaced and a stream of garbage bytes makes the function block indefinitely without ever returning.
- rule: `[spec:libedit:sem:read.read-char-fn]` (step 4c) · C: `src/read.c` `read_char`
- class: logic · reach: hot on binary input.
- disposition: reproduce.
- status: open

**ERR-input-26** — `read_char` zeroes a fresh `mbstate_t` and re-decodes the whole accumulator on every added byte. The C notes this "only works because UTF-8 is stateless"; for a genuinely stateful encoding the conversion is wrong, and no shift state is carried between bytes or between characters.
- rule: `[spec:libedit:sem:read.read-char-fn]` (step 4b) · C: `src/read.c` `read_char`
- class: logic · reach: cold — stateful locales only.
- disposition: reproduce — the rule calls it specified behaviour the port inherits.
- status: open

**ERR-input-27** — `noedit_wgets` discards the **entire** partial line accumulated by the call when the last read returned -1 with `errno == EINTR`, and the test reads the global `errno` rather than anything the callback returned, so a client callback that returns -1 with a stale `EINTR` triggers the discard.
- rule: `[spec:libedit:sem:read.noedit-wgets-fn]` (step 3) · C: `src/read.c` `noedit_wgets`
- class: logic · reach: hot for `NO_TTY`/`EDIT_DISABLED` readers under signals.
- disposition: reproduce.
- status: open

**ERR-input-28** — `noedit_wgets` stores the decoded character into the line buffer *before* checking for space, so when `ch_enlargebufs` fails it breaks without advancing `lastchar` and the character just read is silently lost (then overwritten by the NUL terminator).
- rule: `[spec:libedit:sem:read.noedit-wgets-fn]` (step 1a) · C: `src/read.c` `noedit_wgets`
- class: logic · reach: OOM only.
- disposition: reproduce.
- status: open

**ERR-input-29** — `keymacro_clear`'s guard is `*in > N_KEYS` where `N_KEYS` is 256 and the tables have exactly 256 entries, so a `*in` of exactly 256 passes and `(unsigned char)256 == 0` takes the decision from slot 0. Where `wchar_t` is signed a negative `*in` also passes and the cast wraps it into range.
- rule: `[spec:libedit:sem:keymacro.keymacro-clear-fn]` · C: `src/keymacro.c` `keymacro_clear`
- class: logic · reach: cold — the accesses stay inside the arrays, so this is a correctness bug, not a memory-safety one.
- disposition: fix — the rule directs the port to test the whole code point against `N_KEYS`.
- status: open

**ERR-input-30** — `keymacro_add` passes any `ntype` other than `XK_CMD`/`XK_STR` through to `node__try`'s `EL_ABORT`, killing the process. This is reachable: `terminal_reset_arrow` passes `arrow[i].type` straight through, and `terminal_clear_arrow` (`bind -k -r up`) sets that field to `XK_NOD`, so a subsequent `map_init_emacs`/`map_init_vi` aborts.
- rule: `[spec:libedit:sem:keymacro.keymacro-add-fn]`, `[spec:libedit:sem:keymacro.node-try-fn]` · C: `src/keymacro.c` `keymacro_add`
- class: logic · reach: hot for anyone doing `bind -k -r up` followed by `bind -e`/`bind -v`.
- disposition: needs decision — the rule says "A port must decide whether to reproduce the abort or reject the call; the C's behaviour is a crash."
- status: open

**ERR-input-31** — `node__try` writes `ptr->type = ntype` **before** switching on it, so even the `EL_ABORT` path has already mutated the node; and a failed `wcsdup` on the `XK_STR` arm returns -1 leaving the node `XK_STR` with a NULL string, a state indistinguishable at lookup time from `node_trav`'s no-match answer. The -1 cannot escape: step 3 discards the recursive result and `keymacro_add` discards the outermost one.
- rule: `[spec:libedit:sem:keymacro.node-try-fn]` (step 2c), `[spec:libedit:sem:keymacro.keymacro-add-fn]` · C: `src/keymacro.c` `node__try`
- class: logic · reach: OOM during a bind.
- disposition: fix — propagate the failure rather than leaving a poisoned node.
- status: open

**ERR-input-32** — binding a key that is a proper prefix of existing longer keys silently **destroys** all of them (`node__try` step 2a calls `node__put` on the whole child level), and deleting a longer key prunes back through any node that carried a shadowed shorter binding, so binding `"ab"` then `"abc"` then deleting `"abc"` leaves neither bound and takes the `"a"` node with it.
- rule: `[spec:libedit:sem:keymacro.node-try-fn]` (steps 2a, 3), `[spec:libedit:sem:keymacro.node-delete-fn]` (step 4) · C: `src/keymacro.c`
- class: logic · reach: hot for anyone binding overlapping sequences.
- disposition: reproduce — the rule says "unshadowing" the shorter binding would be a behaviour change.
- status: open

**ERR-input-33** — `keymacro_get`'s "no match" answer is `XK_STR` with a NULL `val->str`, the same return code as a successful string binding, so the caller must test the pointer; and every character consumed during the failed trie walk is silently discarded with no pushback, so an unrecognised escape sequence swallows its own bytes.
- rule: `[spec:libedit:sem:keymacro.keymacro-get-fn]`, `[spec:libedit:sem:keymacro.node-trav-fn]`, `[spec:libedit:sem:read.read-getcmd-fn]` (step 4) · C: `src/keymacro.c`
- class: logic · reach: hot — any unbound escape sequence from the terminal.
- disposition: reproduce.
- status: open

**ERR-input-34** — `node_enum`'s buffer-exhaustion guard uses pre-increments, so the closing quote lands at `cnt + 1` and the terminator at `cnt + 2`, leaving `buf[cnt]` holding whatever was there before — a stale character from a previously printed key, or an uninitialised zero.
- rule: `[spec:libedit:sem:keymacro.node-enum-fn]` (step 1) · C: `src/keymacro.c` `node_enum`
- class: logic · reach: cold — needs a rendering approaching 1019 characters.
- disposition: fix — the rule says the intent was plainly `buf[cnt]` and `buf[cnt + 1]`.
- status: open

**ERR-input-35** — `keymacro__decode_str`'s return value is not the length the full rendering would have needed: it counts bytes actually written by the render loop plus separator and NUL bytes whether or not those were written, so it under-reports on truncation.
- rule: `[spec:libedit:sem:keymacro.keymacro-decode-str-fn]` · C: `src/keymacro.c` `keymacro__decode_str`
- class: logic · reach: every in-tree caller discards the value.
- disposition: reproduce.
- status: open

**ERR-input-36** — `parse__escape`'s blanket "there must be at least two characters here" test rejects an ordinary one-character literal at the end of a string, so `setty erase=X` decodes as -1 and stores `(cc_t)-1` = 0xFF into `c_cc`, whereas `setty erase=^H` works.
- rule: `[spec:libedit:sem:parse.parse-escape-fn]` (the two-character rule) · C: `src/parse.c` `parse__escape`
- class: logic · reach: hot — a plausible `.editrc` line.
- disposition: reproduce.
- status: open

**ERR-input-37** — `parse__escape`'s `\U+` form always consumes **one character more** than the escape text, silently discarding whatever follows: `\U+0041x` consumes 8 characters and drops the `x`. There is no way to write a `\U+` escape followed by another character and keep that character.
- rule: `[spec:libedit:sem:parse.parse-escape-fn]` (form C, cursor) · C: `src/parse.c` `parse__escape`
- class: logic · reach: hot for any multi-character binding containing a `\U+` escape.
- disposition: needs decision — the rule calls it a defect and says the port "must decide deliberately whether to freeze it".
- status: open

**ERR-input-38** — `tok_reset` does not restore `argv[0] = NULL`. If the next `tok_line` publishes zero words — an empty or all-separator line — `tok_finish` never runs its publishing branch and `argv[0]` is still the stale non-NULL pointer from before the reset, so the array is not NULL-terminated. Confirmed in the rule: `tok_str(t, "x y")`, `tok_reset(t)`, `tok_str(t, "   ")` yields `argc == 0` with a non-NULL `argv[0]`.
- rule: `[spec:libedit:sem:tokenizer.fun-tok-reset-fn]`, `[spec:libedit:sem:tokenizer.fun-tok-finish-fn]` · C: `src/tokenizer.c` `tok_reset`
- class: logic · reach: hot for any caller that walks `argv` looking for NULL instead of bounding by `*argc`.
- disposition: reproduce.
- status: open

**ERR-input-39** — the five quoting metacharacters are matched before the IFS test, so `'`, `"`, `\`, newline and NUL can never act as separators however `ifs` is set — and the default `ifs` *is* `"\t \n"`, whose newline is consumed by the newline case and ends the line in `Q_none` instead of separating a word. `tok_str("a\nb")` yields the single word `a` and silently discards `b`.
- rule: `[spec:libedit:sem:tokenizer.fun-tok-line-fn]` (separators) · C: `src/tokenizer.c` `tok_line`
- class: logic · reach: hot for any multi-line input handed to `tok_str`.
- disposition: reproduce.
- status: open

**ERR-input-40** — `el_read_t` comes from `malloc`, not `calloc`, and `read_errno` is never assigned in `read_init`, so it stays indeterminate until `el_wgets` zeroes it on entry.
- rule: `[spec:libedit:sem:read.read-init-fn]` · C: `src/read.c` `read_init`
- class: logic · reach: nothing reads it before then.
- disposition: fix — zero-initialise.
- status: open

**ERR-input-41** — `tok_line`'s `Q_doubleone` arm drops the backslash in `"\'"`, yielding a bare `'` where sh(1) keeps the backslash and yields `\'`.
- rule: `[spec:libedit:sem:tokenizer.fun-tok-line-fn]` (the `'` table) · C: `src/tokenizer.c` `tok_line`
- class: divergence · reach: hot for any quoted editrc/inputrc line.
- disposition: reproduce — the rule calls it deliberate-looking and says it must be preserved.
- status: open

**ERR-input-42** — the macro/pushback queue is FIFO: `el_wpush` writes at the back (`macro[++level]`) while `el_wgetc` always reads from the front (`macro[0]`), so a push issued while a macro is draining is queued *behind* the remainder of that macro. **Two rules disagree**: `[spec:libedit:sem:read.el-wpush-fn]` states the FIFO behaviour explicitly and says the port must reproduce it rather than the intuitive stack behaviour, while `[spec:libedit:sem:histedit.el-wpush-fn]` says "Pushes nest: the most recently pushed string is consumed first."
- rule: `[spec:libedit:sem:read.el-wpush-fn]` vs `[spec:libedit:sem:histedit.el-wpush-fn]` · C: `src/read.c` `el_wpush`, `el_wgetc`
- class: divergence · reach: hot — nested macro expansion, `vi_redo`, `rl_insert`.
- disposition: needs decision — the two rules must be reconciled before a conformance test can be written; the read.c rule is the one derived from the implementation.
- status: open

**ERR-input-43** — dead and stale material in this layer: `keymacro_kprint`'s `"[]"` separator arm is unreachable (the ternary sits inside the `XK_STR` case), a leftover from a removed `XK_EXE` type; `tok_line`'s `other` arm of the newline switch returns 0 without writing any out-parameter and is unreachable given the five-valued `quote_t`; `read.c`'s `FIONREAD` typeahead block and `read__fixio`'s `FIONBIO` sub-block do not compile on glibc because the needed headers are not included; `read_getcmd`'s `KANJI` variant is never defined anywhere; `keymacro_init`'s and `keymacro_reset`'s comments claim they bind the arrow keys, which neither does.
- rule: `[spec:libedit:sem:keymacro.keymacro-kprint-fn]`, `[spec:libedit:sem:tokenizer.fun-tok-line-fn]`, `[spec:libedit:sem:read.el-wgets-fn]`, `[spec:libedit:sem:read.read-fixio-fn]`, `[spec:libedit:sem:read.read-getcmd-fn]`, `[spec:libedit:sem:keymacro.keymacro-init-fn]`, `[spec:libedit:sem:keymacro.keymacro-reset-fn]` · C: `src/keymacro.c`, `src/tokenizer.c`, `src/read.c`
- class: dead · reach: unreachable.
- disposition: fix — not ported.
- status: open

---

## history

`src/history.c`, `src/hist.c`, and the history-recall commands in
`src/common.c` — the history store, the editor's bridge to it, and the on-disk
file format.

**ERR-history-01** — `history_def_enter` inserts the new entry, then evicts from the tail while `cur > max`. With `max == 0` — the state of every history until `H_SETSIZE` is issued — the entry just inserted *is* the tail and is deleted, but `history_def_insert` has already written `*ev` pointing at that entry's string. The caller is handed a dangling `ev->str` and a stale `ev->num` for an event that no longer exists.
- rule: `[spec:libedit:sem:history.history-def-enter-fn]`, `[spec:libedit:sem:history.fun-history-init-fn]`, `[spec:libedit:sem:histedit.history-fn]` · C: `src/history.c` `history_def_enter`
- class: UB (use-after-free exposed across the API) · reach: hot — every `H_ENTER` before `H_SETSIZE`.
- disposition: needs decision — the rule says "a port must decide what to do and record the divergence rather than assume the comment".
- status: open

**ERR-history-02** — `H_NEXT_EVDATA`, `H_DELDATA` and `H_REPLACE` all cast `h->h_ref` to `history_t *` with no check that the built-in implementation is installed, so with a caller-supplied function set they are type-confused memory accesses.
- rule: `[spec:libedit:sem:history.funw-history-fn]`, `[spec:libedit:sem:history.history-next-evdata-fn]`, `[spec:libedit:sem:histedit.history-fn]` · C: `src/history.c`
- class: UB · reach: cold — requires `H_FUNC`, which nothing in the C tree uses.
- disposition: define — restrict the operations to the built-in backend.
- status: open

**ERR-history-03** — `hist_convert` declares a local `HistEventW` and passes its address to the *narrow* store, which writes a `HistEvent` (`{int; const char *}`) through it, then reinterprets `ev.str` as a `char *`. The two are layout-compatible on every supported ABI but the standard does not license it.
- rule: `[spec:libedit:sem:hist.hist-convert-fn]` · C: `src/hist.c` `hist_convert`
- class: UB · reach: hot — the whole narrow-history path, which is what `el_set(EL_HIST, ...)` and the readline layer install.
- disposition: define — model the narrow event as the narrow event it is.
- status: open

**ERR-history-04** — every history access calls `el->el_history.fun` with no NULL check, while every guard in the file tests `el->el_history.ref == NULL` instead. `hist_set(el, NULL, ptr)` with a non-NULL `ptr` is accepted and makes the next access a NULL indirect call.
- rule: `[spec:libedit:sem:hist.hist-convert-fn]` (precondition), `[spec:libedit:sem:hist.hist-set-fn]` · C: `src/hist.c`
- class: UB · reach: cold — requires an application to install the pair inconsistently.
- disposition: define — reject the combination.
- status: open

**ERR-history-05** — `hist_command`'s `history size N` and `history unique N` call `history_w` directly on `el->el_history.ref` instead of dispatching through `el->el_history.fun`. With libedit's *narrow* store — what `el_set(EL_HIST, ...)` and the readline layer install, i.e. the common case — the structs are layout-compatible so nothing crashes, but `history_setsize`/`history_setunique` find the narrow `next` hook where they expect the wide one and return -1, so both subcommands are silently inoperative. With a caller-supplied function set it is straight type confusion.
- rule: `[spec:libedit:sem:hist.hist-command-fn]` · C: `src/hist.c` `hist_command`
- class: UB (custom store) / logic (narrow store) · reach: hot — any `history size` line in an `.editrc`.
- disposition: reproduce the observable -1 for a narrow store, expressed as a checked dispatch; fail rather than invent a meaning for a custom store.
- status: open

**ERR-history-06** — `hist_command`'s list loop dereferences `ct_encode_string`'s result immediately, and that result is NULL when growing the scratch buffer fails.
- rule: `[spec:libedit:sem:hist.hist-command-fn]` (step 2.i) · C: `src/hist.c` `hist_command`
- class: UB · reach: OOM only.
- disposition: define — the rule says to treat it as a -1 return, not a crash.
- status: open

**ERR-history-07** — `hist_command` sizes its `strvis` output buffer as `len * 4 + 1` and `strvis` does no bounds checking whatsoever; the sizing is an assumption rather than a guarantee in any locale where one input byte can decode to a wide character needing more than one significant byte to escape.
- rule: `[spec:libedit:sem:hist.hist-command-fn]` (the escaping) · C: `src/hist.c` `hist_command`
- class: UB · reach: cold, locale-dependent.
- disposition: define — bound the encode.
- status: open

**ERR-history-08** — `history_save_fp` does not check `ct_encode_string`'s return before `strlen(str)`; NULL arises from an allocation failure or from a NULL `ev.str` supplied by a caller function set.
- rule: `[spec:libedit:sem:history.history-save-fp-fn]` (step 4) · C: `src/history.c` `history_save_fp`
- class: UB · reach: OOM, or a custom store.
- disposition: define.
- status: open

**ERR-history-09** — `history_load` ignores `strunvis`'s return value. On the first malformed escape `strunvis` returns -1 **without writing the terminating NUL**, so the reused, never-zeroed 1024-byte scratch buffer is then read as a C string: the bytes decoded so far, followed by whatever the previous line left there — or, on the first malformed line, uninitialised heap — up to the first NUL that happens to be present. If none is present the read runs off the end.
- rule: `[spec:libedit:sem:history.history-load-fn]` (leniency and malformed input) · C: `src/history.c` `history_load`
- class: UB · reach: hot for a corrupted or hand-edited history file.
- disposition: define — keep the successfully decoded prefix, record the divergence, and **keep going**: the C does not abort the load, so a port that treats a malformed line as fatal refuses files the C accepts.
- status: open

**ERR-history-10** — `ed_prev_history` and `ed_search_prev_history` stash the live line with `wcsncpy(el->el_history.buf, el->el_line.buffer, el->el_history.sz)`, which does not NUL-terminate when the source fills the destination. `ed_search_next_history` then runs `c_hmatch` — `wcsstr`/`regexec` — over that unterminated buffer.
- rule: `[spec:libedit:sem:common.ed-prev-history-fn]`, `[spec:libedit:sem:common.ed-search-next-history-fn]`, `[spec:libedit:sem:common.ed-search-prev-history-fn]` · C: `src/common.c`
- class: UB · reach: cold — needs a line exactly filling the stash.
- disposition: define — keep the saved line's length explicitly; it is already tracked in `el_history.last`.
- status: open

**ERR-history-11** — `el_init_internal` discards `hist_init`'s return value, so an allocation failure leaves `buf == NULL`, `sz == 0`, `last == NULL`. `hist_get`'s `eventno == 0` branch then computes `last - buf` as `NULL - NULL`, which C does not define (universally 0 in practice).
- rule: `[spec:libedit:sem:hist.hist-init-fn]`, `[spec:libedit:sem:hist.hist-enlargebuf-fn]` · C: `src/hist.c` `hist_init`
- class: UB · reach: OOM only.
- disposition: define — the rule is explicit that the *failure must not be made fatal* (the first `ch_enlargebufs` silently repairs the state), so the port keeps the degrade-to-empty-line behaviour without the pointer arithmetic.
- status: open

**ERR-history-12** — `FUN(history,end)` calls `free(h->h_ref)` unconditionally, including when a caller-supplied function set is installed. Because `history_set_fun` never stores the caller's reference (ERR-history-17), what is actually freed is the built-in `history_t` — but a port that "fixes" `history_set_fun` inherits a free of caller-owned memory here.
- rule: `[spec:libedit:sem:history.fun-history-end-fn]`, `[spec:libedit:sem:histedit.history-end-fn]` · C: `src/history.c` `FUN(history,end)`
- class: memory · reach: cold — `H_FUNC` users only.
- disposition: reproduce — the rule says to treat the unconditional free as the specified behaviour and keep the two consistent.
- status: open

**ERR-history-13** — `H_REPLACE` stores the duplicated line into `cursor->ev.str` **without freeing the previous string**, so every call leaks the old entry text; and it does not check that the cursor is off the sentinel, so an invalid cursor writes the duplicate into the list header's `ev.str`.
- rule: `[spec:libedit:sem:history.funw-history-fn]` (`H_REPLACE`), `[spec:libedit:sem:histedit.history-fn]`, `[spec:libedit:sem:readline.replace-history-entry-fn]` · C: `src/history.c`
- class: memory · reach: hot for any `replace_history_entry` user.
- disposition: reproduce the leak's observable side (the returned `he->line` stays valid indefinitely); do not reproduce the sentinel write.
- status: open

**ERR-history-14** — `history_set_fun` leaks the built-in backing store on both paths: on acceptance it clears the entries but never frees the `history_t` itself, and on rejection it allocates a fresh one over `h_ref` without freeing what was there.
- rule: `[spec:libedit:sem:history.history-set-fun-fn]` · C: `src/history.c` `history_set_fun`
- class: memory · reach: cold — `H_FUNC` users only.
- disposition: fix — not ABI-observable.
- status: open

**ERR-history-15** — `history_def_del` and `history_deldata_nth` `Strdup` the deleted entry's text into `ev->str` unchecked (so it can be NULL on OOM while the deletion proceeds) and hand the caller a copy the library never frees, an ownership rule the public API does not document. In practice it leaks.
- rule: `[spec:libedit:sem:history.history-def-del-fn]`, `[spec:libedit:sem:history.history-deldata-nth-fn]`, `[spec:libedit:sem:histedit.history-fn]` (`H_DEL`) · C: `src/history.c`
- class: memory · reach: hot — `H_DEL`/`H_DELDATA`, and every `remove_history` from the readline layer.
- disposition: reproduce the ownership transfer (it is the documented `H_DEL` behaviour); the port must not double-free it, and see ERR-readline-14.
- status: open

**ERR-history-16** — `history_save` returns -1 when `fdopen` fails **without closing the descriptor it opened**, leaking a file descriptor.
- rule: `[spec:libedit:sem:history.history-save-fn]` (step 2), `[spec:libedit:sem:histedit.history-fn]` (`H_SAVE`) · C: `src/history.c` `history_save`
- class: memory · reach: OOM only.
- disposition: needs decision — the rule says "A port should close it and note the divergence", i.e. it recommends diverging.
- status: open

**ERR-history-17** — `history_set_fun` reads `nh->h_ref` only to test it against NULL and **never assigns it to `h->h_ref`**. Every callback macro is `(*(h)->h_next)((h)->h_ref, ev)`, so after a successful `H_FUNC` the caller's ten functions are invoked with the *old* reference — the built-in `history_t` that step 2 just emptied. `H_FUNC` as shipped cannot work for any non-trivial custom backend.
- rule: `[spec:libedit:sem:history.history-set-fun-fn]`, `[spec:libedit:sem:histedit.history-fn]` (`H_FUNC`) · C: `src/history.c` `history_set_fun`
- class: logic · reach: dormant — nothing in the C tree uses `H_FUNC`.
- disposition: reproduce — named explicitly in `[dec:libedit:conformance-policy]` as one of the six forks; the rule additionally requires the choice to be recorded rather than silently applied, and notes that assigning `h_ref` would make ERR-history-12's unconditional free start freeing caller-owned memory.
- status: open

**ERR-history-18** — `hist_convert` fills a **local** `HistEventW`, so under `NARROW_HISTORY` the cookie `el->el_history.ev` is never written by any history operation and keeps its all-zero `calloc` value. Its one reader, `vi_to_history_line`, computes `eventno = 1 + ev.num - argument` after a successful `hist_get`, so vi `G` with a count reads a stale `num` of 0 and derives a negative, rejected event. Narrow history is the default configuration.
- rule: `[spec:libedit:sem:hist.hist-convert-fn]`, `[spec:libedit:sem:vi.vi-to-history-line-fn]` · C: `src/hist.c` `hist_convert`
- class: logic · reach: hot — every `nG` in a readline-layer or narrow-API application.
- disposition: needs decision — the rule says the divergence is observable and "the port must decide about it deliberately rather than by accident".
- status: open

**ERR-history-19** — `H_NSAVE_FP`'s positioning loop uses a post-decrement in its condition, so the walk stops *on* the entry at index `nelem` and the write loop then emits that entry plus every newer one: `min(n + 1, size)` entries, not `n`. `n == 0` writes one entry.
- rule: `[spec:libedit:sem:history.history-save-fp-fn]` (step 3), `[spec:libedit:sem:histedit.history-fn]`, `[spec:libedit:sem:readline.append-history-fn]` · C: `src/history.c` `history_save_fp`
- class: logic · reach: hot — every `append_history(n, ...)`.
- disposition: reproduce.
- status: open

**ERR-history-20** — `history_save_fp` writes the `_HiStOrY_V2_` cookie only when `ftell(fp) == 0`. On a non-seekable stream `ftell` returns -1, so `H_SAVE_FP` to a pipe or socket silently produces a headerless file that `history_load` later rejects outright.
- rule: `[spec:libedit:sem:history.history-save-fp-fn]` (step 1) · C: `src/history.c` `history_save_fp`
- class: logic · reach: cold but plausible.
- disposition: reproduce.
- status: open

**ERR-history-21** — write errors are invisible: `history_save` ignores `fclose`'s result, so a failure to flush the final buffer is swallowed and success is still reported; `history_save_fp` ignores every `fprintf` result, so `ENOSPC`, `EIO` and a full pipe go unnoticed.
- rule: `[spec:libedit:sem:history.history-save-fn]` (step 4), `[spec:libedit:sem:history.history-save-fp-fn]` (step 4) · C: `src/history.c`
- class: logic · reach: hot on a full filesystem.
- disposition: reproduce — the return value crosses the ABI.
- status: open

**ERR-history-22** — `history_load`'s cookie check is `strncmp(line, hist_cookie, sz)` where `sz` is the length of the line actually read, not the length of the cookie, so any first line that is a proper prefix of `_HiStOrY_V2_\n` is accepted: a file whose entire content is `_HiS` passes and loads zero entries.
- rule: `[spec:libedit:sem:history.history-load-fn]` (step 3) · C: `src/history.c` `history_load`
- class: logic · reach: cold — truncated files.
- disposition: reproduce — the rule says real files do not exercise it but truncated ones do.
- status: open

**ERR-history-23** — `history_load` returns the number of data *lines read*, which counts lines it silently skipped because they failed to decode in the current locale, and does not subtract entries evicted by the size limit. It is therefore not the number of entries now stored.
- rule: `[spec:libedit:sem:history.history-load-fn]` (return value) · C: `src/history.c` `history_load`
- class: logic · reach: hot with a size cap or a locale mismatch.
- disposition: reproduce.
- status: open

**ERR-history-24** — `history_def_set`'s scan assigns `h->cursor` on every iteration, so a *failed* `H_SET` leaves the cursor parked on the sentinel — the position is invalidated rather than preserved. `history_prev_event`/`history_next_event`/`history_prev_string`/`history_next_string` likewise leave the cursor wherever the scan stopped after an exhausted search.
- rule: `[spec:libedit:sem:history.history-def-set-fn]`, `[spec:libedit:sem:history.history-prev-event-fn]`, `[spec:libedit:sem:history.history-next-event-fn]` · C: `src/history.c`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-history-25** — the `H_ENTER` dispatcher sets `h->h_ent = ev->num` whenever the return is `!= -1`, but a dedup-suppressed enter returns 0 without writing `*ev`, so `h_ent` becomes 0 — the `_HE_OK` prologue's value, which matches no event, since ids start at 1. A following `H_APPEND` then fails with `_HE_NOT_FOUND`.
- rule: `[spec:libedit:sem:history.funw-history-fn]` (`H_ENTER`), `[spec:libedit:sem:history.history-def-enter-fn]` · C: `src/history.c`
- class: logic · reach: only with `H_UNIQUE` enabled.
- disposition: reproduce.
- status: open

**ERR-history-26** — `hist_get`'s branch B has no guard on a negative `eventno`: the walk body never executes, `hp` stays on event 1, and the failure epilogue silently rewrites `eventno` to 1 while the caller believed it was negative.
- rule: `[spec:libedit:sem:hist.hist-get-fn]` (branch B step 3, failure epilogue) · C: `src/hist.c` `hist_get`
- class: logic · reach: cold — callers own the arithmetic.
- disposition: reproduce — the "eventno was fixed by the first call" idiom depends on the epilogue writing `eventno`.
- status: open

**ERR-history-27** — with an empty history, `hist_get`'s step 2 returns `CC_ERROR` **without** resetting `eventno`, and `ed_prev_history` under the emacs keymap keeps the incremented value. The editor then believes it is on event 1 while displaying the stashed line, and anything the user typed in between is lost when they walk back down. The vi keymap escapes this because `ed_prev_history` restores its saved `eventno` on error under `MAP_VI`.
- rule: `[spec:libedit:sem:hist.hist-get-fn]` (state leak), `[spec:libedit:sem:common.ed-prev-history-fn]` · C: `src/hist.c`, `src/common.c`
- class: logic · reach: hot — `^P` with no history configured.
- disposition: reproduce.
- status: open

**ERR-history-28** — `hist_get` step 4 calls `ch_enlargebufs`, which invokes the application's `c_resizefun` callback; a callback that itself uses the history or the scratch buffer invalidates `hp` under the C's feet. The C does not guard against it and the rule does not define the result.
- rule: `[spec:libedit:sem:hist.hist-get-fn]` (step 4) · C: `src/hist.c` `hist_get`
- class: logic · reach: cold — needs an `EL_RESIZE` callback that touches history.
- disposition: define — unspecified in the rule, so the port picks and records a behaviour.
- status: open

**ERR-history-29** — `hist_end` frees `el_history.buf` and NULLs it but leaves `sz` at its last value and `last` pointing into the released memory. A `hist_get` with `eventno == 0` in that state would copy `sz` characters from a NULL `buf` and derive `lastchar` from the dangling `last`.
- rule: `[spec:libedit:sem:hist.hist-end-fn]` · C: `src/hist.c` `hist_end`
- class: logic · reach: unreachable in the C, since `el_end` frees the `EditLine` immediately after.
- disposition: fix — the rule says clearing `sz` and dropping `last` is unobservable and removes the trap.
- status: open

**ERR-history-30** — `vi_to_history_line` stashes the live line with the compile-time constant `EL_BUFSIZ` (1024) as the copy length instead of `el_history.sz`, while still recording the full length in `el_history.last`. Once `ch_enlargebufs` has grown the buffers, a longer line is stashed only in its first 1024 characters and restoring it later yields a tail of stale stash content or NULs.
- rule: `[spec:libedit:sem:vi.vi-to-history-line-fn]`, `[spec:libedit:sem:hist.hist-get-fn]` · C: `src/vi.c` `vi_to_history_line`
- class: logic · reach: cold — needs a line longer than 1024 characters and a vi `G`.
- disposition: reproduce.
- status: open

**ERR-history-31** — `ed_prev_history`'s error path calls `hist_get` a second time and **discards its return value**. If that also fails — for instance when `el_history.ref` is NULL, where both calls fail identically — the line buffer is left untouched, `eventno` keeps the bumped value in emacs mode, and the function still reports success-with-beep. There is no `CC_ERROR` path out of it at all.
- rule: `[spec:libedit:sem:common.ed-prev-history-fn]` (steps 7, 8) · C: `src/common.c` `ed_prev_history`
- class: logic · reach: hot — `^P` with no history installed.
- disposition: reproduce.
- status: open

**ERR-history-32** — `ed_next_history` never saves the current line into `el_history.buf`; only `ed_prev_history` and `ed_search_prev_history` do. Pressing `^N` with `eventno` already 0 and no prior `^P` clamps to 0, `hist_get` copies back whatever `history.buf` holds — a zero-filled buffer after `hist_init`, i.e. an empty line — and the line the user was typing is silently wiped.
- rule: `[spec:libedit:sem:common.ed-next-history-fn]` · C: `src/common.c` `ed_next_history`
- class: logic · reach: hot — a plain `^N` on a fresh line.
- disposition: reproduce.
- status: open

**ERR-history-33** — the one-slot stash is written **only while `eventno` is still 0**, so moving from one recalled entry to another discards whatever the user typed into the first one: the next `hist_get` overwrites the line from the store and the store itself is never modified. Only the original in-progress line is preserved.
- rule: `[spec:libedit:sem:hist.hist-get-fn]` (where the stash comes from) · C: `src/hist.c`, `src/common.c`
- class: logic · reach: hot — the everyday "edit a recalled command, then press up again" case.
- disposition: reproduce — the rule calls it the behaviour to reproduce exactly.
- status: open

**ERR-history-34** — `H_GETSIZE` reports `((history_t *)h_ref)->cur`, the *current number of stored events*, not the maximum configured by `H_SETSIZE`. Despite the symmetry of the opcode names there is no way to query the maximum.
- rule: `[spec:libedit:sem:history.history-getsize-fn]`, `[spec:libedit:sem:histedit.history-fn]` · C: `src/history.c` `history_getsize`
- class: divergence · reach: hot — the readline layer uses it as an entry count.
- disposition: reproduce.
- status: open

**ERR-history-35** — error-string mismatches that are observable through `ev->str`: `history_def_prev` reports `_HE_END_REACHED` ("no next event") from the *previous* function; `history_def_next` reports `_HE_EMPTY_LIST` ("empty list") for a cursor merely parked on the sentinel of a non-empty list.
- rule: `[spec:libedit:sem:history.history-def-prev-fn]`, `[spec:libedit:sem:history.history-def-next-fn]` · C: `src/history.c`
- class: divergence · reach: hot.
- disposition: reproduce — the wording crosses the ABI.
- status: open

**ERR-history-36** — `history_prev_string` walks with `HNEXT` (toward older) while `history_next_string` walks with `HPREV` (toward newer) — the exact opposite pairing from `history_prev_event`/`history_next_event`, which use `HPREV`/`HNEXT`.
- rule: `[spec:libedit:sem:history.history-prev-string-fn]`, `[spec:libedit:sem:history.history-next-string-fn]` · C: `src/history.c`
- class: divergence · reach: hot — `H_PREV_STR`/`H_NEXT_STR`.
- disposition: reproduce — the rule says the inconsistency is real and observable and must not be "corrected".
- status: open

**ERR-history-37** — the source comment above `history_def_enter`'s eviction loop claims it "always keeps at least one entry"; the condition as written does not, and with `max == 0` it deletes the entry just inserted (see ERR-history-01).
- rule: `[spec:libedit:sem:history.history-def-enter-fn]` · C: `src/history.c`
- class: divergence (documentation) · reach: documentation only.
- disposition: fix the comment; the code's behaviour is ERR-history-01.
- status: open

**ERR-history-38** — `history_getsize`'s `if (ev->num < -1)` branch is unreachable because `cur` is never negative.
- rule: `[spec:libedit:sem:history.history-getsize-fn]` (step 3) · C: `src/history.c` `history_getsize`
- class: dead · reach: unreachable.
- disposition: fix — not ported.
- status: open

---

## modes

`src/map.c`, `src/emacs.c`, `src/vi.c`, `src/common.c`, `src/search.c` — the
keymaps, the emacs and vi command sets, and the search machinery.

**ERR-modes-01** — `ed_move_to_beg`'s vi "first non-blank" skip is `while (iswspace(*cursor)) cursor++` with no upper bound: it does not stop at `lastchar` and it does not stop at the end of the allocation. On `"   abc"` after `ed_kill_line` (which sets `lastchar = buffer` while leaving the text physically in the buffer) a vi `^` parks the cursor three positions past `lastchar`; a line of pure whitespace does the same; in the worst case the scan runs off the end of the allocation.
- rule: `[spec:libedit:sem:common.ed-move-to-beg-fn]` · C: `src/common.c` `ed_move_to_beg`
- class: UB · reach: hot — vi `^` after any kill.
- disposition: define — bound the scan at `lastchar`; the rule states the C's behaviour past that point is not a defined semantic to reproduce.
- status: open

**ERR-modes-02** — `ed_prev_line` with a non-positive `argument` runs its backward scan off the front leaving `ptr == buffer - 1` while `argument == 0`, so the guard does not fire; step 5 then leaves `ptr == buffer - 2` and step 6's `ptr++` makes it `buffer - 1`, which the loop condition immediately dereferences — an out-of-bounds read below the line buffer, after which the cursor may be set there.
- rule: `[spec:libedit:sem:common.ed-prev-line-fn]` (undefined edge) · C: `src/common.c` `ed_prev_line`
- class: UB · reach: cold — `ed-prev-line` is unbound by default and needs `ESC 0`.
- disposition: define — treat a non-positive argument as producing no movement.
- status: open

**ERR-modes-03** — `ed_prev_line` step 2 reads `*ptr` at `cursor`; when `cursor == lastchar` that is the slot at `lastchar`, inside the allocation but holding stale data unless something wrote a terminator, so the test's outcome is not reliably defined.
- rule: `[spec:libedit:sem:common.ed-prev-line-fn]` (step 2) · C: `src/common.c` `ed_prev_line`
- class: UB (indeterminate read) · reach: cold.
- disposition: define.
- status: open

**ERR-modes-04** — `em_delete_prev_char` computes `cursor -= el->el_state.argument` with the **unclamped** argument, forming a pointer before the start of the object before the following clamp pulls it back.
- rule: `[spec:libedit:sem:emacs.em-delete-prev-char-fn]` (step 3) · C: `src/emacs.c` `em_delete_prev_char`
- class: UB · reach: hot — any `ESC 9 ^H` near the start of the line.
- disposition: define — saturating arithmetic; the net effect (delete everything before the cursor, land at `buffer`) is the behaviour to reproduce.
- status: open

**ERR-modes-05** — `vi_histedit` does not check `mbstowcs`'s `(size_t)-1` return, so an edited file that is not a valid multibyte string makes `len` `SIZE_MAX`, `buffer[len - 1]` a wild read, and `lastchar` is then set to `buffer + SIZE_MAX`.
- rule: `[spec:libedit:sem:vi.vi-histedit-fn]` (step 7) · C: `src/vi.c` `vi_histedit`
- class: UB · reach: hot — any editor session that saves bytes invalid in the current locale.
- disposition: define — treat a decode failure as an empty result.
- status: open

**ERR-modes-06** — `vi_next_word` and `vi_next_big_word` guard with `cursor >= lastchar - 1`, forming the pointer `buffer - 1` on an empty line.
- rule: `[spec:libedit:sem:vi.vi-next-word-fn]`, `[spec:libedit:sem:vi.vi-next-big-word-fn]` · C: `src/vi.c`
- class: UB · reach: hot — vi `w`/`W` on an empty line.
- disposition: define — express it as "error if fewer than 2 characters remain at or after the cursor"; the observable result is `CC_ERROR`.
- status: open

**ERR-modes-07** — `ce_inc_search`'s `^W` handler advances the cursor by `patlen - 2 - 1`; with `patlen == 2` (`^W` as the very first keystroke of a search) that is computed in `size_t` and wraps to `SIZE_MAX`, which is then added to a `wchar_t *`.
- rule: `[spec:libedit:sem:search.ce-inc-search-fn]` (step 6, defect (a)) · C: `src/search.c` `ce_inc_search`
- class: UB · reach: hot — `^R` then `^W`.
- disposition: define — the sane reading is "start at the cursor".
- status: open

**ERR-modes-08** — the same `^W` handler's append loop is bounded only against `EL_BUFSIZ` for `patbuf` and never against `el->el_line.limit` for the line buffer, while the entry check guaranteed only about three spare slots past the prompt. `^W` on a nearly full line with a long word at the cursor writes past `limit` and past the allocation.
- rule: `[spec:libedit:sem:search.ce-inc-search-fn]` (step 6, defect (b)) · C: `src/search.c` `ce_inc_search`
- class: UB · reach: cold — needs a nearly full line.
- disposition: define — bound the append by the line capacity.
- status: open

**ERR-modes-09** — `ce_search_line`'s backward walk decrements `cp` one past `el->el_line.buffer` before the guard rejects it, and `cv_csearch`'s backward walk does the same at `buffer - 1`.
- rule: `[spec:libedit:sem:search.ce-search-line-fn]`, `[spec:libedit:sem:search.cv-csearch-fn]` · C: `src/search.c`
- class: UB · reach: hot — every failed backward search.
- disposition: define — use bounded indices.
- status: open

**ERR-modes-10** — `cv_csearch` step 4 dereferences `*cp` ("if the character under the cursor already equals the target") **before** any bound check, so a caller with `cursor > lastchar` reads out of range.
- rule: `[spec:libedit:sem:search.cv-csearch-fn]` (fidelity notes) · C: `src/search.c` `cv_csearch`
- class: UB · reach: unreachable in practice — vi command mode keeps the cursor at most `lastchar - 1`, and reading the slot at `lastchar` is in-allocation.
- disposition: define.
- status: open

**ERR-modes-11** — `map_addfunc` checks neither `wcsdup`, so on allocation failure the new entry is left with a NULL `name` and/or `description` while `nfunc` has already been bumped. `parse_cmd` then passes NULL to `wcscmp` and `bind -l` prints `%ls` of NULL.
- rule: `[spec:libedit:sem:map.map-addfunc-fn]`, `[spec:libedit:sem:parse.parse-cmd-fn]` · C: `src/map.c` `map_addfunc`
- class: UB · reach: OOM during `el_set(EL_ADDFN, ...)`.
- disposition: define — the rule says to prefer failing the call, since the downstream C behaviour is UB and not worth matching.
- status: open

**ERR-modes-12** — `map_bind`'s removal path reads `in[1]` even when `in[0]` is the terminating NUL, so `bind -r ""` reads an uninitialised stack slot and then indexes `map[0]`.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (step 6) · C: `src/map.c` `map_bind`
- class: UB · reach: an `.editrc` line or `el_set(EL_BIND, "-r", "", NULL)`.
- disposition: define — treat an empty sequence as the single-element case or reject it.
- status: open

**ERR-modes-13** — `parse__string` performs no bounds checking and takes no length parameter, while `map_bind` hands it two 1024-`wchar_t` stack buffers. A key or value argument that decodes to more than `EL_BUFSIZ` wide characters overflows the stack buffer.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (step 5), `[spec:libedit:sem:parse.parse-string-fn]` · C: `src/map.c` `map_bind`
- class: UB · reach: cold — the tokenizer normally bounds the word length; reachable through `el_set(EL_BIND, ...)` with a long argument.
- disposition: define — bound the decode.
- status: open

**ERR-modes-14** — `bind -k -s <name> "<string>"` calls `terminal_set_arrow` with `keymacro_map_str`'s result, which merely parks the caller's pointer in the shared scratch union; `terminal_set_arrow` copies that union into the function-key table, so the arrow key ends up holding a pointer to `map_bind`'s **stack** buffer, dangling the moment it returns.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (step 9), `[spec:libedit:sem:keymacro.keymacro-map-str-fn]`, `[spec:libedit:sem:terminal.terminal-set-arrow-fn]` · C: `src/map.c` `map_bind`
- class: UB · reach: any `bind -k -s up "..."`.
- disposition: define — own the string.
- status: open

**ERR-modes-15** — `map_set_wordchars` frees the current `el_map.wordchars` *before* duplicating its argument, so `el_set(EL_WORDCHARS, p)` with the `p` obtained from `el_get(EL_WORDCHARS, &p)` — which hands out a borrowed pointer to exactly that field — is a use-after-free.
- rule: `[spec:libedit:sem:map.map-set-wordchars-fn]`, `[spec:libedit:sem:map.map-get-wordchars-fn]` · C: `src/map.c` `map_set_wordchars`
- class: UB · reach: a plausible read-modify-write idiom.
- disposition: define — copy before freeing, or own the string so the aliasing cannot arise.
- status: open

**ERR-modes-16** — `el_match` hands `regcomp` the result of `ct_encode_string`, which is NULL on allocation failure.
- rule: `[spec:libedit:sem:search.el-match-fn]` (step 2) · C: `src/search.c` `el_match`
- class: UB · reach: OOM only.
- disposition: define — return "no match".
- status: open

**ERR-modes-17** — `c_setpat` computes `patlen` as a `size_t` subtraction with no guard for a cursor below `buffer`; the wrap would be clamped to `EL_BUFSIZ - 1` and then 1023 characters read from the line buffer.
- rule: `[spec:libedit:sem:search.c-setpat-fn]` · C: `src/search.c` `c_setpat`
- class: UB · reach: not a reachable state — an absent guard rather than a live bug.
- disposition: define.
- status: open

**ERR-modes-18** — `map_end` frees `el_map.wordchars` but never sets it to NULL, and does not reset `nfunc`, `type` or `current` (which is left pointing into the just-freed `key` block). A second call double-frees `wordchars` and, if any function was added, dereferences the now-NULL `help`.
- rule: `[spec:libedit:sem:map.map-end-fn]` · C: `src/map.c` `map_end`
- class: memory · reach: unobservable in the shipped flow — the only two call sites are `el_end` (immediately before the object is freed) and `map_init`'s failure path (where `nfunc` is still 0).
- disposition: fix — the rule says the observable behaviour is nil.
- status: open

**ERR-modes-19** — `el_map.current` is only ever assigned `el_map.key` or `el_map.alt`, both heap copies, while `el_map.emacs` points at a static const table, so the test `el->el_map.current != el->el_map.emacs` in `c_delafter` and `c_delbefore` is a **tautology**. The guarded `cv_undo` snapshot and `cv_yank` kill-buffer write therefore run on *every* delete, in emacs mode as much as in vi — including from `el_deletestr`, which makes it an ABI-visible side effect of a public API call. The author plainly meant `type != MAP_EMACS` or `current == alt`.
- rule: `[spec:libedit:sem:map.map-init-fn]` (the five fields, consequences) — cross-referenced by `[spec:libedit:sem:chared.c-delafter-fn]`, `[spec:libedit:sem:chared.c-delbefore-fn]`, `[spec:libedit:sem:chared.el-deletestr-fn]`, `[spec:libedit:sem:common.ed-delete-next-char-fn]`, `[spec:libedit:sem:common.ed-delete-prev-char-fn]`, `[spec:libedit:sem:common.ed-delete-prev-word-fn]`, `[spec:libedit:sem:emacs.em-delete-next-word-fn]`, `[spec:libedit:sem:emacs.em-delete-or-list-fn]`, `[spec:libedit:sem:emacs.em-kill-region-fn]`, `[spec:libedit:sem:vi.vi-kill-line-prev-fn]`, `[spec:libedit:sem:vi.vi-substitute-char-fn]` · C: `src/chared.c` `c_delafter`, `c_delbefore`
- class: logic · reach: hot — every deletion in every mode.
- disposition: reproduce — the rule is explicit that the port must keep those calls unconditional in those two functions rather than "fixing" them into a mode test.
- status: open

**ERR-modes-20** — `ed_next_line` with a non-positive `argument` and no newline between the cursor and `lastchar` exits its scan with `ptr == lastchar` and `argument == 0`, so the guard does not fire; step 4's `ptr++` then puts `ptr` at `lastchar + 1` before its own guard rejects the loop, leaving `cursor == lastchar + 1` — a cursor one past the end of the line.
- rule: `[spec:libedit:sem:common.ed-next-line-fn]` (undefined edge) · C: `src/common.c` `ed_next_line`
- class: logic · reach: cold — the slot is inside the allocation, but the cursor value is invalid.
- disposition: define — the rule says the port must not reproduce this; treat a non-positive argument as producing no movement.
- status: open

**ERR-modes-21** — `ed_transpose_chars` advances the cursor in step 1 and does **not** undo it on the step-3 error path. With the cursor at `buffer` on a line of two or more characters it returns `CC_ERROR` with the cursor already moved one to the right, and `el_wgets` only beeps on `CC_ERROR` — it does not refresh — so the internal and displayed cursors disagree until something else forces a redraw.
- rule: `[spec:libedit:sem:common.ed-transpose-chars-fn]` · C: `src/common.c` `ed_transpose_chars`
- class: logic · reach: hot — `^T` at column 0.
- disposition: reproduce — the rule says the port must reproduce the cursor move or treat it as a fix, but cannot silently do neither.
- status: open

**ERR-modes-22** — `em_copy_prev_word`'s copy loop is not conditioned on `c_insert` having succeeded. When `ch_enlargebufs` fails, `c_insert` returns silently without opening the gap, and the loop then overwrites the characters that follow the cursor (stopping at the un-advanced `lastchar`) instead of inserting them; `lastchar` does not move and the function still returns `CC_REFRESH` with no error indication.
- rule: `[spec:libedit:sem:emacs.em-copy-prev-word-fn]` · C: `src/emacs.c` `em_copy_prev_word`
- class: logic · reach: OOM only.
- disposition: reproduce.
- status: open

**ERR-modes-23** — the mark (`c_kill.mark`) is a raw pointer that is never adjusted when the line is edited: inserting or deleting text moves characters out from under it, and `em_kill_line` sets `lastchar = buffer` without touching it, so it can sit above `lastchar`. `em_exchange_mark` then swaps the cursor to a position outside the live line, and `em_kill_region`/`em_copy_region` act on it.
- rule: `[spec:libedit:sem:emacs.em-set-mark-fn]`, `[spec:libedit:sem:emacs.em-exchange-mark-fn]`, `[spec:libedit:sem:emacs.em-kill-line-fn]`, `[spec:libedit:sem:emacs.em-delete-next-word-fn]` · C: `src/emacs.c`
- class: logic · reach: hot — `^@` … `^U` … `^X^X`.
- disposition: reproduce — the rule says to preserve the swap including this case, without introducing a clamp the C does not have.
- status: open

**ERR-modes-24** — `vi_comment_out` calls `c_insert(el, 1)` and then stores `'#'` at the cursor with no check. If the buffers cannot be grown, `c_insert` does nothing and the store *overwrites* the line's first character instead of inserting before it.
- rule: `[spec:libedit:sem:vi.vi-comment-out-fn]` (step 2) · C: `src/vi.c` `vi_comment_out`
- class: logic · reach: OOM only.
- disposition: reproduce.
- status: open

**ERR-modes-25** — `bind -k -r <name>` returns **-1 unconditionally**, even when `terminal_clear_arrow` succeeded, so a successful removal reports failure — and through `el_parse` that -1 is negated into a positive return.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (step 6) · C: `src/map.c` `map_bind`
- class: logic · reach: hot for anyone unbinding a function key.
- disposition: reproduce — the rule states the behaviour is frozen across the C ABI.
- status: open

**ERR-modes-26** — `map_bind`'s `XK_STR` install ends with `map[(unsigned char)in[0]] = ED_SEQUENCE_LEAD_IN` in **both** branches, including under `-k` where `in` is a function-key *name*: `bind -k -s up "x"` therefore sets `map['u'] = ED_SEQUENCE_LEAD_IN` and corrupts the binding of the letter `u`.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (step 9) · C: `src/map.c` `map_bind`
- class: logic · reach: any `bind -k -s`.
- disposition: reproduce — the rule says the keymap clobber is observable and should be kept (unlike the dangling pointer of ERR-modes-14).
- status: open

**ERR-modes-27** — `map_bind` overwrites its own `argc` parameter with 1 as the first statement of the argument loop and iterates to the first NULL element, so `argv` **must** be NULL-terminated and the caller's count is never consulted. `el_wset(EL_BIND, ...)` builds a 20-element array and only terminates it when fewer than 19 arguments were supplied, so with a full 19 `map_bind` reads past the end.
- rule: `[spec:libedit:sem:map.map-bind-fn]`, `[spec:libedit:sem:el.el-wset-fn]` · C: `src/map.c` `map_bind`
- class: logic · reach: cold — needs 19 `EL_BIND` arguments.
- disposition: reproduce the termination convention at the ABI boundary; the over-read itself is ERR-core-api-07.
- status: open

**ERR-modes-28** — `map_bind` stores the resolved command number through an `(el_action_t)` cast, i.e. modulo 256. With `EL_NUM_FCNS == 96` the 160th function registered by `map_addfunc` is the first whose number does not fit; the truncated value may name no help entry, and `map_print_some_keys` then falls through to `EL_ABORT` — a plain `abort()` — on the next bare `bind`.
- rule: `[spec:libedit:sem:map.map-addfunc-fn]`, `[spec:libedit:sem:map.map-bind-fn]` (step 9), `[spec:libedit:sem:map.map-print-some-keys-fn]` (step 4) · C: `src/map.c`
- class: logic · reach: cold — needs 160+ `EL_ADDFN` registrations.
- disposition: needs decision — the rule says the port must not literally abort, but notes that means diverging from the C in a case the C itself calls a bug.
- status: open

**ERR-modes-29** — `map_print_key` prints **nothing** when no help entry matches the action in the slot, while `map_print_some_keys` aborts the process in the same situation.
- rule: `[spec:libedit:sem:map.map-print-key-fn]` (step 3), `[spec:libedit:sem:map.map-print-some-keys-fn]` · C: `src/map.c`
- class: logic · reach: same trigger as ERR-modes-28.
- disposition: reproduce the silent path; see ERR-modes-28 for the abort.
- status: open

**ERR-modes-30** — `map_bind`'s option scan examines only `argv[i][1]`, so clustered flags do not work: `-ar` is read as `-a` and the trailing characters are discarded silently. An unrecognised switch prints a diagnostic and **continues** the scan rather than failing.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (step 3) · C: `src/map.c` `map_bind`
- class: logic · reach: hot for anyone writing `bind -ar`.
- disposition: reproduce.
- status: open

**ERR-modes-31** — every keymap index in `map_bind`, `map_print_key` and `keymacro_clear` is `(unsigned char)in[0]`, so a key whose first wide character is above U+00FF wraps modulo 256 and edits an unrelated slot.
- rule: `[spec:libedit:sem:map.map-bind-fn]` (cross-cutting), `[spec:libedit:sem:map.map-print-key-fn]` · C: `src/map.c`
- class: logic · reach: hot in a UTF-8 locale for `bind` with a non-ASCII key.
- disposition: reproduce.
- status: open

**ERR-modes-32** — `ce_inc_search` dispatches on `el->el_map.current[(unsigned char) ch]`, i.e. the **low byte** of the wide character. The default emacs map holds `ED_UNASSIGNED` for indices 128-255, so typing an accented letter into an incremental search does not extend the pattern: it falls through to the default arm and *terminates* the search, pushing the character back for re-execution. Only U+0000-U+007F can be searched for. (`read_getcmd`, by contrast, refuses to map anything `>= 256` at all.)
- rule: `[spec:libedit:sem:search.ce-inc-search-fn]` (step 6) · C: `src/search.c` `ce_inc_search`
- class: logic · reach: hot in any non-ASCII locale.
- disposition: needs decision — the rule says the port "should specify this behaviour deliberately rather than reproduce the truncation by accident".
- status: open

**ERR-modes-33** — `ce_inc_search`'s end-of-file path returns `ed_end_of_file`'s `CC_EOF` **without stripping the search prompt it appended to the live line** and without restoring the cursor, `patlen` or `eventno` at this or any outer recursion level, leaving the line buffer holding the user's text plus `"\nbck:<pattern>"`.
- rule: `[spec:libedit:sem:search.ce-inc-search-fn]` (step 5) · C: `src/search.c` `ce_inc_search`
- class: logic · reach: input ending mid-search.
- disposition: fix — the rule calls it a bug and says the port should strip the prompt before returning `CC_EOF`.
- status: open

**ERR-modes-34** — `ce_inc_search`'s `pchar` and `endcmd` are function-level statics shared by every recursion level *and* by every `EditLine` instance in the process, so the incremental search is not reentrant across editors or threads.
- rule: `[spec:libedit:sem:search.ce-inc-search-fn]` (statics) · C: `src/search.c` `ce_inc_search`
- class: logic · reach: only with more than one `EditLine`.
- disposition: fix — the rule says threading both through the invocation chain is observationally identical for a single editor and removes the cross-instance hazard.
- status: open

**ERR-modes-35** — `ce_search_line` temporarily overwrites `patbuf[1]` with `'^'` to build its anchored pattern and restores it on every return path, so the shared pattern buffer is corrupt for the duration of the call and nothing reentrant may observe it.
- rule: `[spec:libedit:sem:search.ce-search-line-fn]` (step 2) · C: `src/search.c` `ce_search_line`
- class: logic · reach: hot — every incremental-search keystroke.
- disposition: fix — build the anchored pattern as a separate value; the mutation is not observable to a correct caller.
- status: open

**ERR-modes-36** — `cv_search`'s pattern-reuse path shifts the stored pattern right by two to insert the `".*"` prefix but increments `patlen` only **once**, so the trailing `'.'` is written over the last character of the old pattern: reusing `abc` yields `".*ab.*"`, not `".*abc.*"`. The final character is silently dropped and the search is looser than the user asked for.
- rule: `[spec:libedit:sem:search.cv-search-fn]` (step 6) · C: `src/search.c` `cv_search`
- class: logic · reach: hot — vi `/` followed by an empty pattern after an `M-p`-style search.
- disposition: reproduce.
- status: open

**ERR-modes-37** — `cv_search` returns `CC_REFRESH` when `c_gets` returns -1, which covers both a deliberate cancel and **end of file**; in the EOF case `c_gets` has already called `ed_end_of_file` and thrown away the `CC_EOF`, so `cv_search` silently swallows EOF.
- rule: `[spec:libedit:sem:search.cv-search-fn]` (step 4) · C: `src/search.c` `cv_search`
- class: logic · reach: input ending at the `/` prompt.
- disposition: fix — the rule says the port should propagate the EOF.
- status: open

**ERR-modes-38** — `cv_search` empties the line (`cursor = lastchar = buffer`) *before* running the search, so a failed `/` leaves the line empty: the text the user was editing before pressing `/` is gone, and the "current line" that `ed_search_prev_history` stashes at `eventno == 0` is empty too. `c_gets` has already destroyed the line contents the moment `/` was pressed, whatever happens afterwards.
- rule: `[spec:libedit:sem:search.cv-search-fn]` (steps 3, 9, 10), `[spec:libedit:sem:chared.c-gets-fn]` · C: `src/search.c` `cv_search`
- class: logic · reach: hot — any failed vi `/`.
- disposition: reproduce — the rule says to reproduce it but flags it as worth a note in the port.
- status: open

**ERR-modes-39** — `cv_repeat_srch` sets `lastchar = buffer` to make the history search's prefix comparison vacuous but never moves `cursor`, so on the `CC_ERROR` paths — an unmatched pattern, or a direction that is neither command code — the cursor is left pointing past `lastchar`. The success path hides it because `hist_get` reassigns both.
- rule: `[spec:libedit:sem:search.cv-repeat-srch-fn]` (step 3) · C: `src/search.c` `cv_repeat_srch`
- class: logic · reach: hot — vi `n` with no match.
- disposition: fix — the rule says to set the cursor to `buffer` alongside `lastchar`.
- status: open

**ERR-modes-40** — `cv_csearch` writes `chacha`/`chadir`/`chatflg` **before** running the search, so `;` and `,` are updated even when the search subsequently fails; and on the `CC_ERROR` paths a pending vi operator is left pending, because `c_vcmd.action` is never cleared there.
- rule: `[spec:libedit:sem:search.cv-csearch-fn]` (steps 3, 4) · C: `src/search.c` `cv_csearch`
- class: logic · reach: hot — any failed `f`/`t`, especially after `d`.
- disposition: reproduce.
- status: open

**ERR-modes-41** — `vi_history_word` opens `len + 1` slots with `c_insert`, which advances `lastchar` unconditionally, but its copy loop stops at `el->el_line.limit`. If `c_insert` could not grow the buffers it returns without moving `lastchar` at all and the writes overwrite existing text instead of inserted space. Neither case is detected.
- rule: `[spec:libedit:sem:vi.vi-history-word-fn]` (step 8) · C: `src/vi.c` `vi_history_word`
- class: logic · reach: OOM, or a line at capacity.
- disposition: reproduce.
- status: open

**ERR-modes-42** — `cv_paste`'s `cursor + len > lastchar` error return fires only when `c_insert` silently failed to grow the buffers — by which point `cv_undo` has already run and, for `p`, the cursor may already have been advanced. The error path is not side-effect free.
- rule: `[spec:libedit:sem:vi.cv-paste-fn]` (step 5), `[spec:libedit:sem:vi.vi-paste-next-fn]`, `[spec:libedit:sem:vi.vi-paste-prev-fn]` · C: `src/vi.c` `cv_paste`
- class: logic · reach: OOM only.
- disposition: reproduce.
- status: open

**ERR-modes-43** — `vi_undo` swaps the line and undo buffers wholesale, so afterwards `c_kill.mark` and `c_vcmd.pos` still point into the buffer that has just become the *undo* buffer, and `c_redo` is untouched — so vi `.` still replays the command that `u` just reverted.
- rule: `[spec:libedit:sem:vi.vi-undo-fn]` · C: `src/vi.c` `vi_undo`
- class: logic · reach: hot — every `u`.
- disposition: reproduce — the rule says to reproduce or consciously fix.
- status: open

**ERR-modes-44** — `vi_histedit`'s child calls `exit(0)` rather than `_exit(0)` after a failed `execlp`, flushing the inherited stdio buffers a second time; the parent never inspects the wait status, so an exec failure is indistinguishable from a successful edit; and the `while (waitpid(pid, &status, 0) != pid) continue;` loop spins forever if `waitpid` fails persistently (for example `ECHILD` when `SIGCHLD` is `SIG_IGN`).
- rule: `[spec:libedit:sem:vi.vi-histedit-fn]` (step 7) · C: `src/vi.c` `vi_histedit`
- class: logic · reach: hot for any application that ignores `SIGCHLD`.
- disposition: reproduce the observable outcome; the infinite loop is a hang the port should bound.
- status: open

**ERR-modes-45** — `vi_histedit` reads the edited text back **through the original file descriptor**, so an editor that saves by writing a new file and renaming leaves libedit reading the stale original contents.
- rule: `[spec:libedit:sem:vi.vi-histedit-fn]` (step 7) · C: `src/vi.c` `vi_histedit`
- class: logic · reach: hot — vim's default `backupcopy` behaviour does exactly this.
- disposition: reproduce.
- status: open

**ERR-modes-46** — `vi_histedit` hardcodes the template `/tmp/histedit.XXXXXXXXXX`, ignoring `TMPDIR`, and ignores both `write()` return values, so short writes and `ENOSPC` pass unnoticed. `wcstombs`'s return is likewise unchecked (harmless only because the buffer was zero-filled).
- rule: `[spec:libedit:sem:vi.vi-histedit-fn]` (steps 3, 5, 6) · C: `src/vi.c` `vi_histedit`
- class: logic · reach: hot on systems where `/tmp` is not writable or not private.
- disposition: reproduce the path; the unchecked writes are a robustness gap the port should close.
- status: open

**ERR-modes-47** — four commands yank the same span twice: `ed_delete_prev_word`, `vi_change_to_eol`, `vi_substitute_line` and `vi_kill_line_prev` each copy the doomed text into the kill buffer by hand and then call a `chared` helper (or `ed_kill_line`/`em_kill_line`) that copies byte-identical content over it. The first copy is observably redundant.
- rule: `[spec:libedit:sem:common.ed-delete-prev-word-fn]`, `[spec:libedit:sem:vi.vi-change-to-eol-fn]`, `[spec:libedit:sem:vi.vi-substitute-line-fn]`, `[spec:libedit:sem:vi.vi-kill-line-prev-fn]` · C: `src/common.c`, `src/vi.c`
- class: logic · reach: hot; harmless.
- disposition: reproduce the end state; the duplicate work need not be reproduced.
- status: open

**ERR-modes-48** — `ed_argument_digit` and `ed_digit` compute `c - '0'`, subtracting ASCII `'0'` from a wide character that `iswdigit` accepted. In locales where `iswdigit` is true for non-ASCII decimal digits the result is meaningless, and the C does not guard against it.
- rule: `[spec:libedit:sem:common.ed-argument-digit-fn]`, `[spec:libedit:sem:common.ed-digit-fn]` · C: `src/common.c`
- class: logic · reach: locale-dependent; in C/POSIX only U+0030..U+0039 pass.
- disposition: reproduce.
- status: open

**ERR-modes-49** — the repeat-count overflow cap is tested *before* the multiply in all three accumulators, so `argument` can reach 10000009 through `ed_digit`/`ed_argument_digit` and 4000000 through `em_universal_argument`; there is no overflow check beyond that and none at all on the first digit.
- rule: `[spec:libedit:sem:common.ed-digit-fn]`, `[spec:libedit:sem:common.ed-argument-digit-fn]`, `[spec:libedit:sem:emacs.em-universal-argument-fn]` · C: `src/common.c`, `src/emacs.c`
- class: logic · reach: hot — typing a long digit run.
- disposition: reproduce.
- status: open

**ERR-modes-50** — `ed_next_char` clamps the cursor to `lastchar`, not `lastchar - 1`, even in vi mode, so an overshooting count (`5l` with three characters left) parks the vi cursor one past the last character — exactly the position its own step-1 guard forbids. `vi_to_column` inherits this, so `999|` does the same.
- rule: `[spec:libedit:sem:common.ed-next-char-fn]` (step 3), `[spec:libedit:sem:vi.vi-to-column-fn]` · C: `src/common.c` `ed_next_char`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-51** — `ed_kill_line` (`^K`) does not go through `c_delbefore`/`c_delafter` and takes **no** vi undo snapshot, so it is one of the deletions vi `u` cannot restore. `em_kill_line` (`^U`) likewise records no vi undo state even when bound into a vi keymap.
- rule: `[spec:libedit:sem:common.ed-kill-line-fn]`, `[spec:libedit:sem:emacs.em-kill-line-fn]` · C: `src/common.c`, `src/emacs.c`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-52** — `em_delete_or_list` at the end of a non-empty line rings the bell and returns `CC_ERROR`, and the read loop beeps again for `CC_ERROR`, so the path beeps twice. `vi_list_or_eof`'s two non-EOF branches are behaviourally identical for the same reason.
- rule: `[spec:libedit:sem:emacs.em-delete-or-list-fn]`, `[spec:libedit:sem:vi.vi-list-or-eof-fn]` · C: `src/emacs.c`, `src/vi.c`
- class: logic · reach: hot — `^D` at end of line.
- disposition: reproduce.
- status: open

**ERR-modes-53** — `^W` (`ed_delete_prev_word`), `M-b` and `b` (`ed_prev_word`) use `ce__isword`, the **emacs** word test, even when the vi command map is active, so vi `b` does not split on punctuation the way real vi does and `^W` in vi has emacs word semantics.
- rule: `[spec:libedit:sem:common.ed-delete-prev-word-fn]`, `[spec:libedit:sem:common.ed-prev-word-fn]` · C: `src/common.c`
- class: divergence · reach: hot in vi mode.
- disposition: reproduce.
- status: open

**ERR-modes-54** — every kill and copy command rewrites the kill buffer from index 0, so consecutive kills never accumulate in either direction — a divergence from GNU emacs, where successive kill commands append into one kill-ring entry. Copying or killing a zero-length span therefore *empties* the kill buffer, after which `em_yank` returns `CC_NORM` and inserts nothing.
- rule: `[spec:libedit:sem:emacs.em-copy-region-fn]`, `[spec:libedit:sem:emacs.em-kill-region-fn]`, `[spec:libedit:sem:emacs.em-delete-next-word-fn]`, `[spec:libedit:sem:emacs.em-kill-line-fn]`, `[spec:libedit:sem:emacs.em-yank-fn]` · C: `src/emacs.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-55** — `em_yank`'s numeric argument only chooses where the cursor lands (end of the yanked text for 1, beginning for anything else); it does **not** paste multiple copies, unlike GNU emacs `C-y`. An explicit argument of 1 is indistinguishable from no argument, so there is no way to ask for "cursor at the beginning" with a count of one.
- rule: `[spec:libedit:sem:emacs.em-yank-fn]` (step 6) · C: `src/emacs.c` `em_yank`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-56** — `em_gosmacs_transpose` does not advance point after the swap, unlike GNU emacs `transpose-chars`, and ignores the repeat count entirely.
- rule: `[spec:libedit:sem:emacs.em-gosmacs-transpose-fn]` · C: `src/emacs.c` `em_gosmacs_transpose`
- class: divergence · reach: cold — not bound by default.
- disposition: reproduce.
- status: open

**ERR-modes-57** — `em_kill_line` (`^U`) kills the **entire** line regardless of the cursor, where GNU emacs binds `^U` to `universal-argument` and `kill-line` (`^K`) kills only from point to end of line. libedit's `EM_UNIVERSAL_ARGUMENT` is not bound at all by default.
- rule: `[spec:libedit:sem:emacs.em-kill-line-fn]`, `[spec:libedit:sem:emacs.em-universal-argument-fn]` · C: `src/emacs.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-58** — vi `p`/`P` leave the cursor on the **first** pasted character; real vi leaves it on the last.
- rule: `[spec:libedit:sem:vi.cv-paste-fn]`, `[spec:libedit:sem:vi.vi-paste-next-fn]`, `[spec:libedit:sem:vi.vi-paste-prev-fn]` · C: `src/vi.c` `cv_paste`
- class: divergence · reach: hot.
- disposition: reproduce — the rule calls it deliberate-looking and says it must be preserved.
- status: open

**ERR-modes-59** — vi `Y` yanks only from the cursor to end of line (i.e. behaves as `y$`); real vi's `Y` yanks the whole line.
- rule: `[spec:libedit:sem:vi.vi-yank-end-fn]` · C: `src/vi.c` `vi_yank_end`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-60** — vi `I` moves to column 0 rather than to the first non-blank character.
- rule: `[spec:libedit:sem:vi.vi-insert-at-bol-fn]` · C: `src/vi.c` `vi_insert_at_bol`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-61** — vi `_` appends a word taken from the most recent history entry and enters insert mode; in real vi `_` means "the whole current line". libedit's own comment records this as an invention, and it is why `cc` is documented as a synonym for `c_`.
- rule: `[spec:libedit:sem:vi.vi-history-word-fn]` · C: `src/vi.c` `vi_history_word`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-62** — vi `U` reloads whatever the currently selected history event contains; real vi's `U` restores the line as it was on first entry.
- rule: `[spec:libedit:sem:vi.vi-undo-line-fn]` · C: `src/vi.c` `vi_undo_line`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-63** — vi `n|` counts **characters** (POSIX's definition); NetBSD vi goes to screen column *n*, so wide and control characters that occupy more than one display column make the two definitions differ.
- rule: `[spec:libedit:sem:vi.vi-to-column-fn]` · C: `src/vi.c` `vi_to_column`
- class: divergence · reach: hot in a non-ASCII locale.
- disposition: reproduce.
- status: open

**ERR-modes-64** — `cv_csearch` steps off the current character on **every** iteration of the count loop, including for `t`/`T`. After a `t` the cursor sits one position before the target, so repeating with `;` finds that same target again and the back-off puts the cursor straight back: `t` then `;` does not move. Real vi special-cases this.
- rule: `[spec:libedit:sem:search.cv-csearch-fn]`, `[spec:libedit:sem:vi.vi-to-next-char-fn]`, `[spec:libedit:sem:vi.vi-repeat-next-char-fn]` · C: `src/search.c` `cv_csearch`
- class: divergence · reach: hot.
- disposition: reproduce — the rule says to reproduce libedit's behaviour, not vi's.
- status: open

**ERR-modes-65** — vi `@` does not switch to insert mode after expanding the alias; POSIX implies it should. libedit's own comment records the choice to follow historical precedent instead.
- rule: `[spec:libedit:sem:vi.vi-alias-fn]` · C: `src/vi.c` `vi_alias`
- class: divergence · reach: cold — needs an `EL_ALIAS_TEXT` hook.
- disposition: reproduce.
- status: open

**ERR-modes-66** — vi `%` searches only **forward** from the cursor for the bracket to match, even when the bracket it finds is a closing one, and the operator form anchors at `c_vcmd.pos` rather than at the bracket, so characters between the anchor and the bracket are included. The backward-match case deliberately leaves the matched opening bracket out of the operated range, following POSIX and diverging from NetBSD vi.
- rule: `[spec:libedit:sem:vi.vi-match-fn]` · C: `src/vi.c` `vi_match`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-67** — the vi `/` and `?` prompts are crossed relative to the search direction: `/` invokes `vi_search_prev` (backward through history, prompt `"\n/"`) and `?` invokes `vi_search_next` (forward, prompt `"\n?"`), and `c_gets` treats `ESC`, `CR` and `LF` alike as terminators — with `ESC` accepting and **submitting** the matched entry immediately while `CR`/`LF` leave it for editing, the inverse of the naive expectation.
- rule: `[spec:libedit:sem:search.cv-search-fn]` (steps 3, 11), `[spec:libedit:sem:vi.vi-search-next-fn]`, `[spec:libedit:sem:vi.vi-search-prev-fn]` · C: `src/search.c`, `src/vi.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-68** — the repeat count is silently ignored by a long list of vi commands, so `3dd`, `3cc`, `3yy`, `3p`, `3P`, `3A`, `3a`, `3i`, `3n`, `3N`, `3%`, `3@`, `3s`… all behave as if no count had been typed. `cv_action`'s doubled form discards it explicitly; `vi_to_column` and `vi_history_word` mutate it destructively instead.
- rule: `[spec:libedit:sem:vi.cv-action-fn]`, `[spec:libedit:sem:vi.cv-paste-fn]`, `[spec:libedit:sem:vi.vi-add-fn]`, `[spec:libedit:sem:vi.vi-add-at-eol-fn]`, `[spec:libedit:sem:vi.vi-insert-fn]`, `[spec:libedit:sem:vi.vi-match-fn]`, `[spec:libedit:sem:vi.vi-alias-fn]`, `[spec:libedit:sem:vi.vi-repeat-search-next-fn]`, `[spec:libedit:sem:vi.vi-repeat-search-prev-fn]`, `[spec:libedit:sem:vi.vi-yank-end-fn]` · C: `src/vi.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-modes-69** — `c_setpat` produces the raw typed prefix with no anchors and no metacharacter escaping, and `el_match` tries a literal-substring search *before* compiling it as a POSIX **basic** regular expression. The emacs `M-p`/`M-n` and vi `K`/`J` history search is therefore a substring-or-regex test, not the prefix test `c_hmatch`'s own comment implies; an empty pattern (cursor at column 0) matches everything; and typed `.`, `*`, `[`, `^`, `$`, `\` change the match. Unencodable characters are dropped by the encode step, so in the C locale a pattern of accented text reduces to the empty string that matches everything.
- rule: `[spec:libedit:sem:search.c-setpat-fn]`, `[spec:libedit:sem:search.el-match-fn]`, `[spec:libedit:sem:search.c-hmatch-fn]` · C: `src/search.c`
- class: divergence · reach: hot — every history search.
- disposition: reproduce, including the fast path; the rule notes a Rust port working natively in `str` would diverge for unencodable input and that the divergence must be recorded as an improvement rather than emulated.
- status: open

**ERR-modes-70** — the numeric index comments in `el_map_emacs` are wrong from `M-_` onward: two consecutive entries are both labelled 223 and the labels lag by one thereafter, ending with `254` on the actual index 255. The character comments are correct.
- rule: `[spec:libedit:sem:map.map-init-fn]` (transcription warning) · C: `src/map.c` `el_map_emacs`
- class: divergence (documentation) · reach: transcription hazard only.
- disposition: fix — trust the character comments and the element count.
- status: open

**ERR-modes-71** — dead code in this layer: `ed_delete_next_char`'s non-`KSHVI` branch (`KSHVI` is defined unconditionally in `el.h`); `el_map_vi_insert`'s `#else` block for indices 0..31, for the same reason; `vi_list_or_eof`'s completion-listing branch and `em_delete_or_list`'s listing behaviour, both `#ifdef notyet` or simply unimplemented; `map_bind`'s arity check, inside `#ifdef notyet`, so `bind ^A ed-move-to-beg junk extra` succeeds; the NULL-mark guards in `em_kill_region` and `em_copy_region`, which can never fire because the mark is never NULL; `map_get_editor`'s fall-through for a `type` that is neither `MAP_EMACS` nor `MAP_VI`; `cv_prev_word`'s final clamp, unreachable after its early return; `ed_move_to_beg`'s and `em_capitol_case`/`em_lower_case`/`em_upper_case`'s redundant re-clamps.
- rule: `[spec:libedit:sem:common.ed-delete-next-char-fn]`, `[spec:libedit:sem:map.map-init-fn]`, `[spec:libedit:sem:vi.vi-list-or-eof-fn]`, `[spec:libedit:sem:emacs.em-delete-or-list-fn]`, `[spec:libedit:sem:map.map-bind-fn]`, `[spec:libedit:sem:emacs.em-kill-region-fn]`, `[spec:libedit:sem:emacs.em-copy-region-fn]`, `[spec:libedit:sem:map.map-get-editor-fn]`, `[spec:libedit:sem:chared.cv-prev-word-fn]`, `[spec:libedit:sem:emacs.em-capitol-case-fn]` · C: `src/common.c`, `src/vi.c`, `src/emacs.c`, `src/map.c`, `src/chared.c`
- class: dead · reach: unreachable.
- disposition: fix — not ported.
- status: open

---

## completion

`src/filecomplete.c` — the filename generator, the match list and the shell
escaping applied to an inserted completion.

**ERR-completion-01** — `fn_complete2` computes `single_match` as `matches[2] == NULL && (matches[1] == NULL || strcmp(matches[0], matches[1]) == 0)`. Because `matches[2]` is tested first, an array holding only two elements — which a user-supplied `attempted_completion_function` may legitimately return as `{prefix, NULL}` — is read out of bounds. Arrays from `completion_matches` always carry the slack; a foreign array need not.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 10) · C: `src/filecomplete.c` `fn_complete2`
- class: UB · reach: hot for any application supplying its own completion function.
- disposition: define — test the array length safely.
- status: open

**ERR-completion-02** — `fn_display_match_list` decrements `num` after advancing past element 0 with no lower bound, so `num == 0` wraps to `SIZE_MAX` and the loops walk far off the end of the array.
- rule: `[spec:libedit:sem:filecomplete.fn-display-match-list-fn]` (step 2) · C: `src/filecomplete.c` `fn_display_match_list`
- class: UB · reach: caller error; `rl_display_match_list` forwards a caller-supplied `len` with no range check, so it is reachable across the ABI.
- disposition: define — treat it as a caller error and reject it.
- status: open

**ERR-completion-03** — the same function computes the padding field width as `(int)(width - strlen(string))`; a `width` smaller than the longest string underflows as `size_t` and then casts to a huge or negative `int`.
- rule: `[spec:libedit:sem:filecomplete.fn-display-match-list-fn]` (step 6) · C: `src/filecomplete.c` `fn_display_match_list`
- class: UB · reach: caller error, reachable through `rl_display_match_list`.
- disposition: define.
- status: open

**ERR-completion-04** — `fn_filename_completion_function`'s restart path can fail *after* it has already replaced `filename`/`dirname` but *before* it closes the stale `DIR *` and updates `filename_len`. A following call with a non-zero `state` then finds the stream open, skips the restart entirely, and runs the match loop against a NULL or stale `filename` with a stale length; the NULL case dereferences a null pointer.
- rule: `[spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]` (hazards) · C: `src/filecomplete.c` `fn_filename_completion_function`
- class: UB · reach: OOM during a completion.
- disposition: define — the rule calls it a latent crash, not behaviour to reproduce; reset the whole state atomically.
- status: open

**ERR-completion-05** — `find_word_to_complete` passes `word_break` straight to `wcschr` with no NULL check.
- rule: `[spec:libedit:sem:filecomplete.find-word-to-complete-fn]` · C: `src/filecomplete.c` `find_word_to_complete`
- class: UB · reach: every in-tree caller supplies one; reachable through `fn_complete`/`fn_complete2` with a NULL `word_break`.
- disposition: define.
- status: open

**ERR-completion-06** — `completion_matches` frees only the pointer array on both of its failure paths (the growth realloc and the prefix allocation), leaking every match string collected so far — including the one just generated.
- rule: `[spec:libedit:sem:filecomplete.completion-matches-fn]` (steps 2, 5), `[spec:libedit:sem:readline.completion-matches-fn]` · C: `src/filecomplete.c` `completion_matches`
- class: memory · reach: OOM only.
- disposition: reproduce the observable outcome (NULL, indistinguishable from "no matches"); the rule notes the leak is part of the observed behaviour but must not become a double free.
- status: open

**ERR-completion-07** — `fn_tilde_expand` assigns `len` only in the branch that found a `/`. With no slash present it is still 0 at the final step, so the **entire** original string, tilde included, is appended to the home directory: `fn_tilde_expand("~")` returns `"$HOME/~"` and `fn_tilde_expand("~bob")` returns `"<bob's home>/~bob"`. Observable through the public API and through readline's exported `tilde_expand()`.
- rule: `[spec:libedit:sem:filecomplete.fn-tilde-expand-fn]` · C: `src/filecomplete.c` `fn_tilde_expand`
- class: logic · reach: hot — `append_char_function` and `tilde_expand()` both reach it, which is why a bare `~user` match never stats as a directory. `fn_filename_completion_function` never reaches it, because the string it passes always ends in `/`.
- disposition: reproduce — named explicitly in `[dec:libedit:conformance-policy]` as one of the six forks; the rule says the port must reproduce it.
- status: open

**ERR-completion-08** — `escape_filename`'s quoting scan toggles `d_quoted` on a `"` with **no backslash check**, while the `'` branch does check, so a user-typed `\"` still toggles the double-quote state.
- rule: `[spec:libedit:sem:filecomplete.escape-filename-fn]` (step 2) · C: `src/filecomplete.c` `escape_filename`
- class: logic · reach: hot for anyone completing inside a quoted word.
- disposition: reproduce — the rule says the asymmetry is a bug the port must reproduce.
- status: open

**ERR-completion-09** — the same scan's single-quote rule looks back exactly one character, so an escaped backslash followed by a quote (`\\'`) is misread as an escaped quote.
- rule: `[spec:libedit:sem:filecomplete.escape-filename-fn]` (step 2) · C: `src/filecomplete.c` `escape_filename`
- class: logic · reach: cold.
- disposition: reproduce.
- status: open

**ERR-completion-10** — if `app_func` returns an empty string, `escape_filename`'s "any other byte" branch copies that string's NUL into the buffer and advances past it, so the result carries an **embedded NUL** and its visible length ends one byte before the real end. readline's append hook does exactly this whenever `rl_completion_append_character` is 0.
- rule: `[spec:libedit:sem:filecomplete.escape-filename-fn]` (degenerate case), `[spec:libedit:sem:readline.rl-completion-append-character-function-fn]` · C: `src/filecomplete.c` `escape_filename`
- class: logic · reach: hot for readline consumers that clear the append character.
- disposition: reproduce.
- status: open

**ERR-completion-11** — `fn_complete2` calls `el_deletestr` to remove the partial word *before* building the replacement text; if that allocation fails it jumps to the exit and returns `CC_REFRESH` with the word **not restored**, so a failed completion silently eats what the user typed.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 12c) · C: `src/filecomplete.c` `fn_complete2`
- class: logic · reach: OOM only.
- disposition: reproduce.
- status: open

**ERR-completion-12** — the "Display all %zu possibilities? (y or n) " prompt is answered with `getc(stdin)` — always the C `stdin`, never `el->el_infile` — so completion misbehaves whenever libedit is driven on any other descriptor. Only one character is consumed, so the rest of the user's line, including the newline after the `y`, stays in the stream.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 13c), `[spec:libedit:sem:histedit.el-fn-complete-fn]` · C: `src/filecomplete.c` `fn_complete2`
- class: logic · reach: hot for any application not reading from `stdin`.
- disposition: reproduce — the rule explicitly calls it a real bug and still describes it as the behaviour.
- status: open

**ERR-completion-13** — `fn_complete2` writes `point` and `end` as counts of **wide** characters while the strings it hands the callbacks are multibyte, so in a non-ASCII locale the offsets do not index those strings. `rl_complete` passes `&rl_point` and `&rl_end` here.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 5), `[spec:libedit:sem:readline.rl-complete-fn]` · C: `src/filecomplete.c` `fn_complete2`
- class: logic · reach: hot in a UTF-8 locale.
- disposition: reproduce — the rule calls it a known wart the port must keep.
- status: open

**ERR-completion-14** — the append character is added only on the non-quoting path *and* only when an `attempted_completion_function` was supplied, so a caller passing neither an attempted function nor `FN_QUOTE_MATCH` — which is exactly what `rl_complete` does when the application set no attempted completion function — gets **no append character at all**.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 12e), `[spec:libedit:sem:filecomplete.fn-complete-fn]` · C: `src/filecomplete.c` `fn_complete2`
- class: logic · reach: hot for readline consumers.
- disposition: reproduce.
- status: open

**ERR-completion-15** — the fallback to the built-in generator is taken only when there was no attempted function at all, **or** when `over` is non-NULL and `*over` is 0 and the attempted function returned NULL. With `over == NULL` and a NULL attempted result there is no fallback and completion simply does nothing.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 7) · C: `src/filecomplete.c` `fn_complete2`
- class: logic · reach: hot for `fn_complete` callers that pass no `over`.
- disposition: reproduce.
- status: open

**ERR-completion-16** — `fn_filename_completion_function` restarts when `state == 0` **or when the static stream is NULL**, so calling it again with a non-zero `state` *after* it returned NULL restarts the scan from the first entry instead of continuing to return NULL. A caller that does not stop at the first NULL loops forever.
- rule: `[spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]` · C: `src/filecomplete.c`
- class: logic · reach: cold — `completion_matches` and `rl_completion_matches` both stop at the first NULL.
- disposition: reproduce.
- status: open

**ERR-completion-17** — all of `fn_filename_completion_function`'s state lives in function-level statics (an open `DIR *`, the pattern, its length, the directory prefix and the opened path), so it is not reentrant and not thread-safe: starting a new scan silently destroys any scan in progress, including one belonging to another thread or a nested completion. It also ignores `text` entirely on continuation calls.
- rule: `[spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]` (hazards), `[spec:libedit:sem:readline.completion-matches-fn]` · C: `src/filecomplete.c`
- class: logic · reach: hot under any concurrency or nesting.
- disposition: needs decision — the rule says the port must "decide about explicitly rather than inherit" these hazards.
- status: open

**ERR-completion-18** — `append_char_function` stats the match string exactly as the generator built it, which still carries whatever directory prefix the user typed — including an unexpanded `~`. Combined with ERR-completion-07, a bare `~user` is stat'ed as the nonsense path `<home>/~user` and therefore always reports `" "` rather than `"/"`.
- rule: `[spec:libedit:sem:filecomplete.append-char-function-fn]` · C: `src/filecomplete.c` `append_char_function`
- class: logic · reach: hot for tilde completions.
- disposition: reproduce.
- status: open

**ERR-completion-19** — `unescape_string` skips every `\` unconditionally without looking at what follows, so there is no notion of an escaped backslash: a `\\` pair collapses to nothing at all and a trailing `\` is simply dropped. `find_word_to_complete` reports the *raw* span length through `*length`, which `fn_complete2` hands to `el_deletestr`, so the count of characters deleted from the line can exceed the length of the string returned.
- rule: `[spec:libedit:sem:filecomplete.unescape-string-fn]`, `[spec:libedit:sem:filecomplete.find-word-to-complete-fn]` (steps 5, 6) · C: `src/filecomplete.c`
- class: logic · reach: hot for any word containing backslashes.
- disposition: reproduce.
- status: open

**ERR-completion-20** — `needs_escaping`'s 23-character set contains `[` but not `]`, contains `)` and `}` although the word-break set used by `_el_fn_complete` carries only the opening forms, and omits `!`, `~`, `^`, `%`, `:` and `/`. Any wide character outside ASCII returns 0, so the individual bytes of a multibyte character are never escaped.
- rule: `[spec:libedit:sem:filecomplete.needs-escaping-fn]`, `[spec:libedit:sem:filecomplete.el-fn-complete-fn]` · C: `src/filecomplete.c` `needs_escaping`
- class: divergence · reach: hot.
- disposition: reproduce — the rule says these look like oversights and must be kept as they are.
- status: open

**ERR-completion-21** — `_fn_qsort_string_compare` orders with `strcasecmp`, so two names differing only in case compare equal and `qsort` — which is not required to be stable — leaves their relative order unspecified. The comparison folds case byte by byte with no multibyte awareness.
- rule: `[spec:libedit:sem:filecomplete.fn-qsort-string-compare-fn]` · C: `src/filecomplete.c` `_fn_qsort_string_compare`
- class: divergence · reach: hot — every listed match set.
- disposition: reproduce the case-insensitive ordering; the rule says the tie order may not be relied on and the port should not try.
- status: open

**ERR-completion-22** — `completion_matches` and `rl_completion_matches` are two **separate** implementations with different results, where GNU readline from 4.2 on makes the former a thin alias of the latter. libedit's `completion_matches` does not sort, preserves generation order, and leaves element 0 empty when the matches share no prefix; `rl_completion_matches` sorts, computes element 0 from adjacent pairs, and substitutes a copy of `text` when the prefix is empty. Swapping the names changes both the displayed order and what is inserted.
- rule: `[spec:libedit:sem:filecomplete.completion-matches-fn]`, `[spec:libedit:sem:readline.completion-matches-fn]`, `[spec:libedit:sem:readline.rl-completion-matches-fn]` · C: `src/filecomplete.c`, `src/readline.c`
- class: divergence · reach: hot.
- disposition: reproduce — both must be ported as-is.
- status: open

**ERR-completion-23** — `fn_complete2`'s `what_to_do` is only ever `'\t'` or `'?'`, so the `'!'` tests are dead code and the `'*'` behaviour the header comment documents (insert all matches) is unimplemented. `_el_fn_sh_complete` is an exported symbol with no behaviour of its own.
- rule: `[spec:libedit:sem:filecomplete.fn-complete2-fn]` (step 1), `[spec:libedit:sem:filecomplete.el-fn-sh-complete-fn]` · C: `src/filecomplete.c`
- class: dead · reach: unreachable.
- disposition: fix — not ported; both exported symbols are kept, doing the same thing.
- status: open

---

## core-api

`src/el.c`, `src/eln.c` and the contracts `src/histedit.h` documents — object
construction and teardown, the `el_set`/`el_get` surface and the narrow
wrappers.

**ERR-core-api-01** — `el_init_internal` step 4 is `el->el_prog = wcsdup(ct_decode_string(prog, &el->el_scratch))`, and the NULL test covers only `wcsdup`'s own failure. `ct_decode_string` returns NULL for a NULL `prog` **and** for a `prog` that is not a valid multibyte string in the current `LC_CTYPE`, and that NULL is handed straight to `wcsdup`.
- rule: `[spec:libedit:sem:el.el-init-internal-fn]` (step 4), `[spec:libedit:sem:histedit.el-init-fd-fn]` · C: `src/el.c` `el_init_internal`
- class: UB · reach: hot — constructing an `EditLine` before `setlocale` decodes the program name in the C locale.
- disposition: define — reject a NULL or undecodable `prog` by failing construction; there is no defined C behaviour to preserve.
- status: open

**ERR-core-api-02** — `el_init_internal` discards the return value of `keymacro_init`, `map_init`, `ch_init`, `search_init`, `hist_init`, `prompt_init` and `sig_init`. Each returns -1 on allocation failure, and the object is then handed back to the caller as a **success** with NULL buffers in it — `el_keymacro.buf`, `el_map.alt`/`key`/`help`/`func`, `el_line.buffer` and the undo/redo/kill buffers, `el_search.patbuf`, `el_history.buf`, `el_signal`. Each is dereferenced without a guard the first time the corresponding feature is used, so an out-of-memory during construction becomes a crash somewhere else entirely.
- rule: `[spec:libedit:sem:el.el-init-internal-fn]` (partial-init failure), `[spec:libedit:sem:map.map-init-fn]`, `[spec:libedit:sem:keymacro.keymacro-init-fn]`, `[spec:libedit:sem:sig.sig-init-fn]`, `[spec:libedit:sem:hist.hist-init-fn]` · C: `src/el.c` `el_init_internal`
- class: UB · reach: OOM during construction.
- disposition: define — fail construction instead. Note the exception: `[spec:libedit:sem:hist.hist-init-fn]` is explicit that *its* failure must **not** be made fatal, because `ch_enlargebufs` repairs the state (see ERR-history-11).
- status: open

**ERR-core-api-03** — `read_init` failure is not survivable. It returns -1 either without having allocated `el_read` or after calling `read_end` itself (which frees it and NULLs it); either way `el->el_read` is NULL on entry to the `el_end(el)` cleanup, and `el_end` calls `read_end` unconditionally, whose first act dereferences it. The documented "returns NULL because `read_init` failed" outcome is therefore unreachable — the process faults first — and on one of the two paths it is also a double `read_end`.
- rule: `[spec:libedit:sem:el.el-init-internal-fn]` (CRASH), `[spec:libedit:sem:el.el-end-fn]`, `[spec:libedit:sem:read.read-end-fn]` · C: `src/el.c`
- class: UB · reach: OOM during construction.
- disposition: define — teardown must tolerate an uninitialised read subsystem and the constructor must actually return NULL.
- status: open

**ERR-core-api-04** — `el_wline` is `return (const LineInfoW *)(void *)&el->el_line;` — type punning between two unrelated struct types, which works only because `el_line_t`'s first three members happen to match `LineInfoW`'s. The intermediate cast through `void *` exists only to silence the compiler.
- rule: `[spec:libedit:sem:el.el-wline-fn]`, `[spec:libedit:sem:histedit.el-wline-fn]` · C: `src/el.c` `el_wline`
- class: UB · reach: hot — every `el_wline`, and `el_line`/`fn_complete2` go through it.
- disposition: define — express it as a genuine borrowed view, not a transmute, keeping the field order and representation in the exported struct.
- status: open

**ERR-core-api-05** — `el_wline(NULL)` computes the offset of `el_line` from a null pointer, yielding a small non-null garbage pointer the caller then dereferences. More broadly, `el_beep`, `el_reset`, `el_resize`, `el_source`, `el_wline`, `el_getc`, `el_gets`, `el_push`, `el_insertstr`, `el_replacestr`, `el_parse` and `el_line` have **no NULL check on `el`**; `el_end` is the only NULL-tolerant entry point.
- rule: `[spec:libedit:sem:el.el-wline-fn]`, `[spec:libedit:sem:el.el-beep-fn]`, `[spec:libedit:sem:el.el-reset-fn]`, `[spec:libedit:sem:el.el-resize-fn]`, `[spec:libedit:sem:el.el-source-fn]`, `[spec:libedit:sem:eln.el-gets-fn]`, `[spec:libedit:sem:eln.el-line-fn]` · C: `src/el.c`, `src/eln.c`
- class: UB · reach: caller error.
- disposition: define — the rule says a port should not attempt to reproduce `el_wline(NULL)`'s bogus pointer.
- status: open

**ERR-core-api-06** — `el_init` calls `fileno` on all three streams before any validation, so a NULL stream is a null dereference in practice; and `fileno` on a stream that is not open returns -1, which is stored as the descriptor without diagnosis while construction still reports success.
- rule: `[spec:libedit:sem:el.el-init-fn]`, `[spec:libedit:sem:histedit.el-init-fn]` · C: `src/el.c` `el_init`
- class: UB · reach: caller error; the -1 case is defined-but-wrong and shows up later as a `tty_init` failure setting `NO_TTY`.
- disposition: define for the NULL case; reproduce the -1-descriptor outcome.
- status: open

**ERR-core-api-07** — `el_wset`'s collection loop for `EL_BIND`/`EL_TELLTC`/`EL_SETTC`/`EL_ECHOTC`/`EL_SETTY` runs `for (i = 1; i < 20; i++)` reading varargs until the first NULL. If the caller omits the NULL sentinel with fewer than 19 arguments the loop **reads past the end of the argument list**; if all 19 slots are filled the loop exits with `i == 20` and no terminator is stored, so `argv` reaches the callee unterminated with `argc == 20`, and a 20th and further varargs are silently dropped rather than diagnosed.
- rule: `[spec:libedit:sem:el.el-wset-fn]` (bounds), `[spec:libedit:sem:histedit.el-wset-fn]`, `[spec:libedit:sem:map.map-bind-fn]` · C: `src/el.c` `el_wset`
- class: UB · reach: caller error for the missing sentinel; the 19-argument case is reachable by construction.
- disposition: define — always terminate; the port's ABI shim inherits the requirement.
- status: open

**ERR-core-api-08** — several `el_wset` ops accept NULL and pass it straight on: `EL_EDITOR` reaches `wcscmp`, `EL_WORDCHARS` reaches `wcsdup`, `EL_SETFP` reaches `fileno`, and `EL_GETENV` stores a NULL hook that the next environment lookup calls through.
- rule: `[spec:libedit:sem:el.el-wset-fn]`, `[spec:libedit:sem:el.editline.el-getenv-fn]`, `[spec:libedit:sem:map.map-set-editor-fn]`, `[spec:libedit:sem:map.map-set-wordchars-fn]`, `[spec:libedit:sem:histedit.el-wset-fn]` · C: `src/el.c` `el_wset`
- class: UB · reach: caller error.
- disposition: define — reject NULL.
- status: open

**ERR-core-api-09** — `el_set`'s `EL_EDITOR` and `EL_WORDCHARS` arms pass `ct_decode_string(...)`'s result to `el_wset` **unchecked**, and it is checked on neither side, so `el_set(el, EL_EDITOR, NULL)` — or a string invalid in the current locale — dereferences NULL inside `wcscmp`/`wcsdup`.
- rule: `[spec:libedit:sem:eln.el-set-fn]` · C: `src/eln.c` `el_set`
- class: UB · reach: caller error, or any locale mismatch.
- disposition: define — reject rather than reproduce.
- status: open

**ERR-core-api-10** — `el_line` sets `info->buffer = ct_encode_string(...)`, which is NULL on allocation failure, and then computes `info->cursor` and `info->lastchar` as `NULL + offset`. There is no error return; a caller can only observe a `LineInfo` with a NULL `buffer`.
- rule: `[spec:libedit:sem:eln.el-line-fn]` (step 4), `[spec:libedit:sem:histedit.el-line-fn]` · C: `src/eln.c` `el_line`
- class: UB · reach: OOM only.
- disposition: define — treat the allocation-failure case as unspecified rather than reproducing the arithmetic.
- status: open

**ERR-core-api-11** — `el_gets` dereferences `*nread` in its byte-count rewrite whenever a line was returned, so `el_gets(el, NULL)` faults the moment a line is read — while `el_wgets(el, NULL)` is well defined, because it substitutes a local `int`.
- rule: `[spec:libedit:sem:eln.el-gets-fn]`, `[spec:libedit:sem:histedit.el-gets-fn]` · C: `src/eln.c` `el_gets`
- class: UB · reach: caller error, but an easy one given the wide entry point tolerates it.
- disposition: define.
- status: open

**ERR-core-api-12** — `el_init_internal` leaks `el->el_scratch.wbuff` on both of its early failure paths: step 4 allocates it via `ct_decode_string` and then calls `el_free(el)`, which releases the object but not that buffer, and the `terminal_init` failure path does the same. The buffer is only ever freed by `el_end`, which is not called on either path.
- rule: `[spec:libedit:sem:el.el-init-internal-fn]` (LEAK) · C: `src/el.c` `el_init_internal`
- class: memory · reach: OOM / undecodable `prog`.
- disposition: fix — the rule says the port obviously must not reproduce the leak.
- status: open

**ERR-core-api-13** — `el_end` frees `el_lgcyconv.cbuff` and then the `EditLine`, leaving `el_lgcylinfo`'s three `char *` members dangling into the released buffer for the instant before the object goes away.
- rule: `[spec:libedit:sem:el.el-end-fn]` (step 4) · C: `src/el.c` `el_end`
- class: memory · reach: every `el_end`; unobservable.
- disposition: fix.
- status: open

**ERR-core-api-14** — `prompt_get` selects the left-hand prompt only when `op` is exactly `EL_PROMPT`, so every other op — `EL_PROMPT_ESC` included — selects `el_rprompt`. `el_get(el, EL_PROMPT_ESC, &f, &c)` and `el_wget(...)` therefore report the **right** prompt's function and escape character, asymmetrically with `prompt_set`, which correctly treats `EL_PROMPT_ESC` as the left prompt. Combined with both getters passing `c == NULL` for plain `EL_PROMPT`, there is **no route through the public API to retrieve `el_prompt.p_ignore` at all**.
- rule: `[spec:libedit:sem:prompt.prompt-get-fn]` (BUG), `[spec:libedit:sem:el.el-wget-fn]`, `[spec:libedit:sem:eln.el-get-fn]`, `[spec:libedit:sem:histedit.el-wget-fn]` · C: `src/prompt.c` `prompt_get`
- class: logic · reach: hot — the op most likely to be used for reading an escape character back is the one that reads the wrong record.
- disposition: reproduce — all four rules agree the behaviour is frozen ABI and the port reproduces it rather than fixing it.
- status: open

**ERR-core-api-15** — `el_wget(EL_SIGNAL)` stores the raw `el_flags & HANDLE_SIGNALS` (0 or 0x001) and `el_wget(EL_SAFEREAD)` stores the raw `el_flags & FIXIO` (0 or **0x100**, i.e. 256), neither normalised — unlike `EL_UNBUFFERED` and `EL_EDITMODE`, which are. A caller comparing `EL_SAFEREAD`'s result against 1 gets the wrong answer.
- rule: `[spec:libedit:sem:el.el-wget-fn]`, `[spec:libedit:sem:histedit.el-wget-fn]` · C: `src/el.c` `el_wget`
- class: logic · reach: hot.
- disposition: reproduce — the rules mark it frozen.
- status: open

**ERR-core-api-16** — `el_wset(EL_HIST)` clears `NARROW_HISTORY` only when `MB_CUR_MAX == 1`. The guard is the wrong way round for safety: in a multibyte locale an application that called the narrow `el_set(EL_HIST)` and later the wide `el_wset(EL_HIST)` leaves the flag set, and every subsequent history access then passes the wide callback's `wchar_t *` to `hist_convert`, which reinterprets it as a `char *` and decodes it — type confusion producing garbage or a fault. The narrow `el_set(EL_HIST)` sets the flag unconditionally, in every locale, and never clears it.
- rule: `[spec:libedit:sem:el.el-wset-fn]` (`EL_HIST`), `[spec:libedit:sem:eln.el-set-fn]`, `[spec:libedit:sem:histedit.el-wset-fn]`, `[spec:libedit:sem:hist.hist-convert-fn]` · C: `src/el.c`, `src/eln.c`
- class: logic · reach: cold — needs both entry points used on one `EditLine`.
- disposition: reproduce the flag manipulation as written; the rule says the port should carry the hazard in its own notes.
- status: open

**ERR-core-api-17** — `el_get`'s `EL_PREP_TERM` arm forwards to `el_wget`, which has no such case, so the call consumes the caller's `int *`, stores nothing and always returns -1. It is a set-only op the narrow wrapper pretends to forward.
- rule: `[spec:libedit:sem:eln.el-get-fn]`, `[spec:libedit:sem:histedit.el-get-fn]` · C: `src/eln.c` `el_get`
- class: logic · reach: any caller that believes the header.
- disposition: reproduce.
- status: open

**ERR-core-api-18** — `el_get(EL_PROMPT_ESC)` stores `*c = (char)wc` **unconditionally**, including when `prompt_get` returned -1 (writing `'\0'`), never checks `c` for NULL, and *truncates* the `wchar_t` to its low byte rather than encoding it — which is not the inverse of any multibyte conversion, and is implementation-defined above `CHAR_MAX`. `el_wget` hands back the full `wchar_t`.
- rule: `[spec:libedit:sem:eln.el-get-fn]`, `[spec:libedit:sem:histedit.el-get-fn]` · C: `src/eln.c` `el_get`
- class: logic · reach: hot for narrow-API callers.
- disposition: reproduce.
- status: open

**ERR-core-api-19** — `el_editmode` discards `tty_rawmode`'s and `tty_cookedmode`'s return values, so a failure to change the terminal mode still reports success while the flag has already been updated, leaving `el_flags` and the actual terminal state out of step. `el_wset(EL_PREP_TERM)` and `el_wset(EL_UNBUFFERED)` do the same, resetting `rv` to 0 regardless.
- rule: `[spec:libedit:sem:el.el-editmode-fn]`, `[spec:libedit:sem:el.el-wset-fn]` · C: `src/el.c`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-core-api-20** — a line in an `.editrc` containing **only spaces or tabs** survives `el_source`'s checks (its first byte is not `'\n'`, it decodes, it is not a comment), tokenises to zero words, and `el_wparse` rejects that with -1 — which breaks the read loop, so the rest of the file is silently discarded and `el_source` returns -1. Only a literally empty line is safe.
- rule: `[spec:libedit:sem:el.el-source-fn]`, `[spec:libedit:sem:parse.parse-line-fn]`, `[spec:libedit:sem:histedit.el-source-fn]` · C: `src/el.c` `el_source`
- class: logic · reach: hot — an easy thing to have in a config file.
- disposition: reproduce.
- status: open

**ERR-core-api-21** — `el_source`'s return value is the result of the **last** line that reached dispatch, not an accumulation, and `el_wparse` negates the builtin's status, so a builtin that failed with -1 surfaces as **+1**. A caller testing `!= 0` sees the failure; a caller testing `== -1` does not. There is also no way to distinguish "could not open the file" from "a line failed to parse" — both are -1.
- rule: `[spec:libedit:sem:el.el-source-fn]` (return value), `[spec:libedit:sem:parse.el-wparse-fn]`, `[spec:libedit:sem:histedit.el-source-fn]` · C: `src/el.c` `el_source`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-core-api-22** — two rules disagree about `el_source`'s empty-file-name early return: `[spec:libedit:sem:el.el-source-fn]` says "a constructed path is never empty, so no allocation is leaked here", while `[spec:libedit:sem:histedit.el-source-fn]` says "the allocated path buffer, if any, leaks on this path — it is not freed before the return".
- rule: `[spec:libedit:sem:el.el-source-fn]` (step 5) vs `[spec:libedit:sem:histedit.el-source-fn]` · C: `src/el.c` `el_source`
- class: logic · reach: at most an OOM-adjacent leak; unobservable either way.
- disposition: needs decision — the two rules must be reconciled before a test can assert anything; the el.c rule's reasoning (a constructed path always contains `.editrc`) appears correct.
- status: open

**ERR-core-api-23** — `el_resize` discards `terminal_change_size`'s -1, so an out-of-memory during a resize is silent and leaves the display state inconsistent (both display buffers NULL, saved cursor not restored).
- rule: `[spec:libedit:sem:el.el-resize-fn]` (step 2), `[spec:libedit:sem:terminal.terminal-change-size-fn]` · C: `src/el.c` `el_resize`
- class: logic · reach: OOM during a resize.
- disposition: reproduce the silent return; the port must not leave the display state inconsistent.
- status: open

**ERR-core-api-24** — `el_end` tears the subsystems down in the **same forward order** as construction, not the reverse, with one exception (`read_end` runs sixth here but `read_init` runs eleventh there). Nothing in the current subsystem set depends on the order, but a Rust `Drop` on a struct whose fields are declared in init order would give the opposite.
- rule: `[spec:libedit:sem:el.el-end-fn]` (step 3) · C: `src/el.c` `el_end`
- class: logic · reach: latent.
- disposition: reproduce the order explicitly — the rule says the port should not silently swap it, since a future subsystem interaction would then differ.
- status: open

**ERR-core-api-25** — when the platform supplies neither `secure_getenv`, nor `__secure_getenv`, nor `issetugid`, `issetugid()` is `#define`d to the constant 1 and the fallback `secure_getenv` returns NULL for everything, so libedit reads **no** environment variables at all — `TERM`, `HOME`, `EDITRC` and `EDITOR` are all invisible. The rule states this is a real shipped configuration, not a theoretical one.
- rule: `[spec:libedit:sem:el.secure-getenv-fn]` · C: `src/el.c` `secure_getenv`
- class: logic · reach: platform-dependent.
- disposition: fix — the rule says the always-deny branch is a portability artefact and must **not** be reproduced; the port implements the real privilege test.
- status: open

**ERR-core-api-26** — `el_line`'s `info->lastchar` is the byte width of the wide prefix `[buffer, lastchar)`, not the end of the encoded string: step 3 encodes to the wide string's terminating `L'\0'`, which under the `EL_UNBUFFERED` read path is not at `lastchar`, so `info->lastchar` lands in the middle of a longer, still NUL-terminated byte string. `el_gets` has the matching defect: `*nread` measures exactly `*nread` wide characters while the returned string runs to the terminator, so the byte string can run past `*nread` bytes into characters left over from an earlier, longer line.
- rule: `[spec:libedit:sem:eln.el-line-fn]`, `[spec:libedit:sem:eln.el-gets-fn]`, `[spec:libedit:sem:histedit.el-line-fn]` · C: `src/eln.c`
- class: logic · reach: hot for unbuffered consumers.
- disposition: reproduce.
- status: open

**ERR-core-api-27** — `el_getc` converts with `wctob`, so in a UTF-8 locale every character outside US-ASCII fails with -1/`ERANGE` and the character is **consumed and lost** — `el_wgetc` has already popped it and `el_getc` provides no pushback. `*cp` is also written (`'\0'`) before any early return, clobbering a pre-loaded value even when nothing was read.
- rule: `[spec:libedit:sem:eln.el-getc-fn]`, `[spec:libedit:sem:histedit.el-getc-fn]` · C: `src/eln.c` `el_getc`
- class: logic · reach: hot in any non-ASCII locale; `rl_read_key` and vi `@` both go through it.
- disposition: reproduce.
- status: open

**ERR-core-api-28** — `el_wget`, `el_get` and `el_set` all declare their result variable uninitialised before the dispatch switch, relying on every arm — including `default` — to assign it.
- rule: `[spec:libedit:sem:el.el-wget-fn]`, `[spec:libedit:sem:eln.el-get-fn]`, `[spec:libedit:sem:eln.el-set-fn]`, `[spec:libedit:sem:histedit.el-wget-fn]` · C: `src/el.c`, `src/eln.c`
- class: logic · reach: benign today.
- disposition: fix — return per arm.
- status: open

**ERR-core-api-29** — `EL_GETTC`'s third argument must be a `char **` for a string capability, a `char **` for the four boolean-ish capabilities (receiving a static `"yes"`/`"no"`), and an `int *` for every other numeric capability — and there is **no way to ask which kind a name is**. Passing the wrong pointer type is a type-confusing store. Only two varargs are read despite the header's `..., NULL` annotation, so no sentinel is consumed. `el_wget`'s arm additionally builds `argv[0]` from a function-local `static char name[] = "gettc"`, mutable and shared across calls and threads.
- rule: `[spec:libedit:sem:el.el-wget-fn]` (`EL_GETTC`), `[spec:libedit:sem:terminal.terminal-gettc-fn]`, `[spec:libedit:sem:eln.el-get-fn]`, `[spec:libedit:sem:histedit.el-get-fn]` · C: `src/el.c`, `src/terminal.c`
- class: logic · reach: hot — `rl_get_screen_size` uses it.
- disposition: reproduce the dispatch; the static buffer can be made immutable without observable effect.
- status: open

**ERR-core-api-30** — `map_get_wordchars` reports success while storing NULL: between `map_init` and the mode initialisation that follows it the field is NULL, and `map_set_wordchars` leaves it NULL when its `wcsdup` fails while still returning 0. Callers get `0` with a NULL pointer, which means "the built-in defaults are in use", not "empty".
- rule: `[spec:libedit:sem:map.map-get-wordchars-fn]`, `[spec:libedit:sem:map.map-set-wordchars-fn]`, `[spec:libedit:sem:el.el-wget-fn]` · C: `src/map.c`
- class: logic · reach: OOM, or a very early `el_get`.
- disposition: reproduce — the C makes no guarantee, so the port chooses whether the field can be NULL where a caller can observe it.
- status: open

**ERR-core-api-31** — `EL_GETENV` is missing from both narrow entry points: `el_wset`/`el_wget` support it, but `el_set` and `el_get` fall to their defaults and return -1, even though the hook is narrow in *both* APIs. Through the narrow API the environment hook can be neither set nor read, while `histedit.h` documents the op as `set/get`.
- rule: `[spec:libedit:sem:eln.el-set-fn]`, `[spec:libedit:sem:eln.el-get-fn]`, `[spec:libedit:sem:histedit.el-get-fn]`, `[spec:libedit:sem:histedit.el-set-fn]` · C: `src/eln.c`
- class: divergence · reach: hot for narrow-API applications.
- disposition: reproduce the code; the header is wrong.
- status: open

**ERR-core-api-32** — the list ops have different argument caps in the two APIs: `el_wset` loops `i < 20` and accepts 19 arguments without ever writing a terminator, while `el_set` caps at 18 and always writes one. The limit differs by one between the wide and narrow APIs and only the narrow one is safe.
- rule: `[spec:libedit:sem:eln.el-set-fn]`, `[spec:libedit:sem:el.el-wset-fn]`, `[spec:libedit:sem:histedit.el-set-fn]`, `[spec:libedit:sem:histedit.el-wset-fn]` · C: `src/eln.c`, `src/el.c`
- class: divergence · reach: hot for anyone passing many `EL_BIND` arguments.
- disposition: reproduce both caps.
- status: open

**ERR-core-api-33** — `histedit.h` documents `el_source` as sourcing "$PWD/.editrc or $HOME/.editrc". There is no `$PWD` lookup and no attempt at `./.editrc`; the fallback chain is `$EDITRC` then `$HOME/.editrc`. The vestige of the removed attempt is still visible as a `fp = NULL;` initialisation followed by a redundant `if (fp == NULL)` before the only `fopen`. The manual page matches the implementation.
- rule: `[spec:libedit:sem:el.el-source-fn]`, `[spec:libedit:sem:histedit.el-source-fn]` · C: `src/histedit.h`, `src/el.c`
- class: divergence (documentation) · reach: documentation only.
- disposition: fix the header comment; the code is the contract.
- status: open

**ERR-core-api-34** — further `histedit.h` documentation errors: `H_CURR` is documented as taking `const int` and takes nothing; `H_ADD`, `H_ENTER` and `H_APPEND` are documented as `const wchar_t *` for the *narrow* entry point, where they are `const char *`; `H_NEXT_EVDATA`, `H_DELDATA` and `H_REPLACE` mention a type `histdata_t` that `histedit.h` does not define (it is `void *`, declared only in `editline/readline.h`); `EL_GETTC` is annotated `const Char *` although the argument is `char *` in both APIs; and `editline(3)` describes `EL_EDITMODE` as "only an indication", where the flag actually makes `el_wgets` bypass the editing loop entirely.
- rule: `[spec:libedit:sem:histedit.history-fn]`, `[spec:libedit:sem:histedit.el-wget-fn]`, `[spec:libedit:sem:histedit.el-wset-fn]` · C: `src/histedit.h`, `doc/editline.3`
- class: divergence (documentation) · reach: documentation only.
- disposition: fix the documentation.
- status: open

**ERR-core-api-35** — dead code in this layer: `el_wset`'s inner `default:` arm for the list ops sets `rv = -1` and then `EL_ABORT`s, unreachable because the outer switch already restricted `op`; and `el_source`'s `fp = NULL; if (fp == NULL)` vestige (see ERR-core-api-33).
- rule: `[spec:libedit:sem:el.el-wset-fn]`, `[spec:libedit:sem:el.el-source-fn]` · C: `src/el.c`
- class: dead · reach: unreachable.
- disposition: fix — express it as an unreachable branch rather than a runtime abort.
- status: open

---

## readline-compat

`src/readline.c` and `src/editline/readline.h` — the GNU readline emulation
layer, its history-expansion machinery and its exported globals.

**ERR-readline-01** — `rl_completion_matches` sorts with `qsort(&list[1], len - 1, sizeof(*list), (int (*)(const void *, const void *))strcmp)`. That is wrong twice: calling `strcmp` through an incompatible function-pointer type is undefined behaviour, and `qsort` hands the comparator the *addresses* of the elements, so `strcmp` receives `char **` values and compares the raw bytes of the pointers rather than the strings. The resulting order is effectively allocation-address order, which then also corrupts the adjacent-pair common-prefix computation in the following step. The correctly written comparator `_rl_qsort_string_compare` sits unused in the same file.
- rule: `[spec:libedit:sem:readline.rl-completion-matches-fn]` (step 6), `[spec:libedit:sem:readline.rl-qsort-string-compare-fn]` · C: `src/readline.c` `rl_completion_matches`
- class: UB · reach: hot — every readline-layer completion with more than one match.
- disposition: **needs decision — the sources conflict.** `[dec:libedit:conformance-policy]` names "the pointer-sorting completion comparator" as one of the six forks defaulting to *reproduce*, while `[spec:libedit:sem:readline.rl-completion-matches-fn]` says "The port must sort the strings; that changes observable output, and it is the right change."
- status: open

**ERR-readline-02** — `getfrom`'s growth test is `if (len - 1 >= size)` with `len` a `size_t`, so on the first iteration `len - 1` wraps to `SIZE_MAX` and the buffer is grown from 16 to 32 before the first byte is stored; thereafter the condition only fires at `len >= size + 1`, so the byte at index `size` is written **before** the growth — a one-byte heap overflow each time `len` reaches capacity (32, 64, 128, …). The trailing `what[len] = '\0'` can land one past the end for the same reason.
- rule: `[spec:libedit:sem:readline.getfrom-fn]` · C: `src/readline.c` `getfrom`
- class: UB · reach: hot — any `!!:s/pattern/replacement/` whose pattern reaches a power-of-two length.
- disposition: define — grow when `len == size`.
- status: open

**ERR-readline-03** — `history_list`'s bounds guard is `if (i++ == history_length) abort();`, evaluated *after* the write, so when the real number of entries exceeds the cached `history_length` the code writes one element past the end of `_history_list` and only then calls `abort()` — inside a library.
- rule: `[spec:libedit:sem:readline.history-list-fn]` · C: `src/readline.c` `history_list`
- class: UB · reach: whenever `history_length` is stale, which `read_history` and `remove_history` both leave it.
- disposition: define — the rule says the port must not corrupt memory and should not kill the process.
- status: open

**ERR-readline-04** — `rl_bind_key` writes `e->el_map.key[c]` with **no range check on `c`**, and `el_map.key` is exactly 256 `el_action_t`. Any `c` outside 0..255 — including the negative values a caller might pass for meta keys, and `EOF` — writes out of bounds and corrupts adjacent `EditLine` state. The source acknowledges this in a comment.
- rule: `[spec:libedit:sem:readline.rl-bind-key-fn]` · C: `src/readline.c` `rl_bind_key`
- class: UB · reach: caller error, but an obvious one.
- disposition: define — range-check.
- status: open

**ERR-readline-05** — `replace()` does not check its `strdup` (the source carries `// XXX: check`), so on allocation failure `*tmp` becomes NULL **after** the old buffer has already been freed; `_history_expand_command`'s modifier loop then passes that NULL to `strrchr`, `_rl_compat_sub` or `strdup`.
- rule: `[spec:libedit:sem:readline.replace-fn]` · C: `src/readline.c` `replace`
- class: UB · reach: OOM during a `:t`/`:e` history modifier.
- disposition: define — the rule says the port should decide explicitly between aborting the expansion and leaving the string untouched; either is a deliberate divergence from a crash.
- status: open

**ERR-readline-06** — `rl_copy_text` calls `strlcpy(out, li->buffer + from, len)`, but `strlcpy`'s third argument is the destination **size**, so it copies only `len - 1` bytes plus a NUL and the last requested character is dropped. For `len == 0` (`from == to`) `strlcpy` with size 0 writes nothing at all, so the `el_malloc`'d buffer is returned uninitialised and unterminated.
- rule: `[spec:libedit:sem:readline.rl-copy-text-fn]` · C: `src/readline.c` `rl_copy_text`
- class: UB (the `len == 0` return) / logic (the dropped character) · reach: hot.
- disposition: needs decision — the rule says both defects must be decided about explicitly; the correct call would pass `len + 1`.
- status: open

**ERR-readline-07** — `rl_copy_text` does not check a negative `from`, so a caller passing one reads before the start of the buffer.
- rule: `[spec:libedit:sem:readline.rl-copy-text-fn]` · C: `src/readline.c` `rl_copy_text`
- class: UB · reach: caller error.
- disposition: define.
- status: open

**ERR-readline-08** — `stifle_history`'s eviction loop dereferences `remove_history(0)`'s result without a NULL check (it can be NULL on allocation failure, or if the history runs out before `history_length` says it should), and casts `he->line` through `unsigned long` to strip `const` — which round-trips on LP64 but is not pointer-safe in general. It also frees `he->data`, the application's `histdata_t`, assuming an allocator the readline API never promised.
- rule: `[spec:libedit:sem:readline.stifle-history-fn]` · C: `src/readline.c` `stifle_history`
- class: UB · reach: hot — every `stifle_history` that actually evicts.
- disposition: define — this is nonetheless the only place in the file that disposes of a `remove_history` result correctly, so the port keeps the disposal and drops the unsafe cast.
- status: open

**ERR-readline-09** — `rl_save_prompt`'s entire body is `rl_prompt_saved = strdup(rl_prompt);` with no NULL check on `rl_prompt`, which is NULL until `rl_set_prompt` has succeeded once — so calling it before `rl_initialize()` or `readline()` passes NULL to `strdup`.
- rule: `[spec:libedit:sem:readline.rl-save-prompt-fn]` · C: `src/readline.c` `rl_save_prompt`
- class: UB · reach: caller error, plausible at startup.
- disposition: define.
- status: open

**ERR-readline-10** — `_rl_abort_internal` `longjmp`s through the single module-global `topbuf`. If no `readline()` call is active — in callback mode, or after `readline()` has returned — `topbuf` holds a stale or zero-initialised `jmp_buf` and the jump lands in a dead frame. Nesting `readline()` calls makes only the innermost `setjmp` reachable. It also calls `el_beep(e)` with no NULL guard.
- rule: `[spec:libedit:sem:readline.rl-abort-internal-fn]` · C: `src/readline.c` `_rl_abort_internal`
- class: UB · reach: cold but reachable from a hook.
- disposition: define — the rule says the port must handle it deliberately rather than inherit it.
- status: open

**ERR-readline-11** — many entry points have **no lazy-initialisation guard** and dereference the module statics directly, so calling them before `rl_initialize()` is a NULL dereference: `current_history`, `next_history`, `previous_history`, `history_list`, `history_search`, `history_search_prefix`, `history_search_pos`, `history_total_bytes`, `unstifle_history`, `rl_callback_read_char`, `rl_callback_handler_remove`, `rl_add_defun`, `rl_parse_and_bind`, `rl_read_init_file`, `rl_prep_terminal`, `rl_deprep_terminal`, `rl_resize_terminal`, `rl_redisplay`, `rl_forced_update_display`, `rl_crlf`, `rl_ding`, `rl_echo_signal_char`, `rl_display_match_list`, `rl_get_screen_size`, `rl_set_screen_size`, `rl_variable_bind`, `rl_kill_full_line` and `rl_stuff_char`. `unstifle_history` is called out specifically as "a plausible thing for an application to do at startup".
- rule: `[spec:libedit:sem:readline.unstifle-history-fn]`, `[spec:libedit:sem:readline.current-history-fn]`, `[spec:libedit:sem:readline.history-list-fn]`, `[spec:libedit:sem:readline.rl-add-defun-fn]`, `[spec:libedit:sem:readline.rl-parse-and-bind-fn]`, `[spec:libedit:sem:readline.rl-callback-read-char-fn]`, and the rules for each named function · C: `src/readline.c`
- class: UB · reach: hot — the initialisation contract is undocumented and inconsistent across the file.
- disposition: define — initialise lazily everywhere, or reject; do not fault.
- status: open

**ERR-readline-12** — `rl_parse_and_bind` checks neither `tok_init`'s NULL return (so an allocation failure crashes on the next call) nor `tok_str`'s status (so an incomplete quote leaves `argc`/`argv` in whatever state it left them, which `el_parse` then consumes).
- rule: `[spec:libedit:sem:readline.rl-parse-and-bind-fn]`, `[spec:libedit:sem:tokenizer.fun-tok-str-fn]` · C: `src/readline.c` `rl_parse_and_bind`
- class: UB · reach: hot — one mismatched quote in a configuration line.
- disposition: define — check both.
- status: open

**ERR-readline-13** — `history_expand`'s `ADD_STRING` growth-failure path frees `*output` and the fragment and returns 0, leaving the caller's `*output` pointing at **freed memory** (not NULL) and leaking the `result` accumulator.
- rule: `[spec:libedit:sem:readline.history-expand-fn]` (step 4) · C: `src/readline.c` `history_expand`
- class: UB · reach: OOM only.
- disposition: define.
- status: open

**ERR-readline-14** — `history_truncate_file`'s copy-back phase checks `ferror(fp)` where it should check `ferror(tp)`, so a read error on the temporary file is reported as success; `nlines <= 0` is not handled meaningfully (the first decrement makes it negative, it can never reach 0, and the resulting cut point is arbitrary); the scratch file is always created in `/tmp` regardless of where the history file lives, so private history transits a world-writable directory and the operation fails if `/tmp` is not writable; and the rewrite is not atomic, so a crash between the seek-to-0 and the `ftruncate` leaves a corrupted history file.
- rule: `[spec:libedit:sem:readline.history-truncate-file-fn]` · C: `src/readline.c` `history_truncate_file`
- class: UB (the arbitrary cut point) / logic · reach: hot for anyone calling it with a non-positive count or on a system with a restricted `/tmp`.
- disposition: needs decision — the rule says both the `/tmp` scratch file and the non-atomic rewrite must be "preserved or consciously changed".
- status: open

**ERR-readline-15** — `free_history_entry` frees nothing: its whole body is `return he ? NULL : NULL;`. GNU readline's frees the entry's line and the `HIST_ENTRY` and returns the `histdata_t`, so the documented readline idiom `free_history_entry(remove_history(i))` leaks both the `el_malloc`'d `HIST_ENTRY` and the fresh copy of the line that `H_DELDATA` made.
- rule: `[spec:libedit:sem:readline.free-history-entry-fn]`, `[spec:libedit:sem:readline.remove-history-fn]` · C: `src/readline.c` `free_history_entry`
- class: memory · reach: hot for any readline consumer that removes history entries.
- disposition: reproduce — named explicitly in `[dec:libedit:conformance-policy]` as one of the six forks; the rule says making it actually free would turn today's leak into a **double free** in programs that already free the entry themselves.
- status: open

**ERR-readline-16** — `getto` assigns `*top = NULL` unconditionally right after `el_realloc(*top, 16)`. If that realloc failed, the previous buffer is still allocated and its only pointer has just been discarded, so it leaks. (The unconditional NULL is also what stops the error exit from double-freeing.)
- rule: `[spec:libedit:sem:readline.getto-fn]` · C: `src/readline.c` `getto`
- class: memory · reach: OOM during an `s///` modifier.
- disposition: fix — not observable.
- status: open

**ERR-readline-17** — the history-expansion machinery keeps four permanent allocations: `_history_expand_command`'s function-static `from` and `to` (the mechanism behind the `&` modifier) and `get_history_event`'s `last_search_pat` and `last_search_match`. None is ever freed.
- rule: `[spec:libedit:sem:readline.history-expand-command-fn]`, `[spec:libedit:sem:readline.get-history-event-fn]` · C: `src/readline.c`
- class: memory · reach: hot.
- disposition: reproduce the `&`/`:%` semantics they implement; the permanent allocation itself is not observable and the port owns them properly.
- status: open

**ERR-readline-18** — `rl_restore_prompt` assigns `rl_prompt = rl_prompt_saved` **without freeing the current `rl_prompt`**, so the message buffer `rl_message` installed is leaked on every save/message/restore cycle; and `rl_save_prompt` does not free a previous saved value, so a second save without an intervening restore leaks the first copy.
- rule: `[spec:libedit:sem:readline.rl-restore-prompt-fn]`, `[spec:libedit:sem:readline.rl-save-prompt-fn]`, `[spec:libedit:sem:readline.rl-message-fn]` · C: `src/readline.c`
- class: memory · reach: hot for any application using `rl_message`.
- disposition: reproduce the ownership transfer; the leak is not observable and the port frees properly.
- status: open

**ERR-readline-19** — `rl_initialize` calls `el_end(e)`/`history_end(h)` without clearing the statics first, and both of its failure exits (`e` or `h` NULL after construction; `rl_set_prompt("")` failing) leave the other object allocated and stored, or both statics dangling after it tears them down.
- rule: `[spec:libedit:sem:readline.rl-initialize-fn]` (steps 1, 7, 11) · C: `src/readline.c` `rl_initialize`
- class: memory · reach: OOM, or a second `rl_initialize`.
- disposition: define — a dangling static is UB for every later entry point.
- status: open

**ERR-readline-20** — `history_tokenize`'s array-growth failure frees the array and returns NULL, leaking every token already allocated; `rl_completion_matches`' error exit does the same for every collected match, including on the single-match `strdup` failure.
- rule: `[spec:libedit:sem:readline.history-tokenize-fn]`, `[spec:libedit:sem:readline.rl-completion-matches-fn]` · C: `src/readline.c`
- class: memory · reach: OOM only.
- disposition: reproduce the NULL return; free properly.
- status: open

**ERR-readline-21** — `_default_history_file` caches the computed `$HOME/.history` path in a function-static that is never freed — an intentional once-per-process leak — and derives it from `getpwuid(getuid())`, so `$HOME` is ignored. It uses `getpwuid`'s static storage and an unsynchronised static, so it is not thread-safe.
- rule: `[spec:libedit:sem:readline.default-history-file-fn]` · C: `src/readline.c` `_default_history_file`
- class: memory · reach: hot — every `read_history(NULL)`/`write_history(NULL)`.
- disposition: reproduce the passwd-derived path (it is observable); the leak is not.
- status: open

**ERR-readline-22** — `history_search_pos` calls `history_set_pos(off)`, which only assigns the global `history_offset` and never moves libedit's internal cursor, so the following `H_CURR` re-reads whatever entry the cursor was already on. `pos` therefore acts purely as a range check and the search always begins wherever the cursor happens to be. On a hit it returns the **requested** position, not the position of the match, and `history_offset` is left set to `off` even when the search fails.
- rule: `[spec:libedit:sem:readline.history-search-pos-fn]`, `[spec:libedit:sem:readline.history-set-pos-fn]` · C: `src/readline.c` `history_search_pos`
- class: logic · reach: hot for any consumer using it.
- disposition: reproduce.
- status: open

**ERR-readline-23** — `rl_read_key` writes the character into a scratch buffer and then returns `el_getc`'s **status**, so it yields 1 for every successful read regardless of which key was pressed, 0 at EOF and -1 on error. GNU readline's returns the character. Any application that switches on the return value sees only the constant 1.
- rule: `[spec:libedit:sem:readline.rl-read-key-fn]` · C: `src/readline.c` `rl_read_key`
- class: logic · reach: hot.
- disposition: reproduce — the rule says to record it rather than fix it silently.
- status: open

**ERR-readline-24** — `username_completion_function`'s loop condition is inverted: it *continues* while the entry is exactly equal to `text` and stops at the first entry that differs, so it exits after a single `getpwent()` and returns the next database entry regardless of whether it starts with `text`. **No prefix matching happens at all**, despite the source comment describing exactly that. `endpwent()` is only reached when the database is exhausted, so an abandoned scan leaks the open `passwd` handle.
- rule: `[spec:libedit:sem:readline.username-completion-function-fn]` · C: `src/readline.c` `username_completion_function`
- class: logic · reach: hot for any consumer that wires it up.
- disposition: reproduce.
- status: open

**ERR-readline-25** — `rl_crlf`, `rl_ding` and `rl_echo_signal_char` all use `re_putc(e, c, 0)`, which deposits the character into EditLine's **virtual display array** at the current refresh cursor without advancing it and without writing to the terminal or `rl_outstream`. Nothing is flushed. `rl_ding` is therefore close to a no-op — it is not `el_beep(e)`, which is what actually emits the bell — and if the virtual display is later rendered it carries a stray BEL byte at whatever position the refresh cursor happened to be. GNU readline writes all three straight to `rl_outstream`, and echoes the signal character in caret notation.
- rule: `[spec:libedit:sem:readline.rl-crlf-fn]`, `[spec:libedit:sem:readline.rl-ding-fn]`, `[spec:libedit:sem:readline.rl-echo-signal-char-fn]` · C: `src/readline.c`
- class: logic · reach: hot.
- disposition: reproduce — the rule says libedit's behaviour is what crosses the ABI.
- status: open

**ERR-readline-26** — `rl_redisplay` pushes the terminal's reprint character onto the pending-input queue *and* calls `rl_forced_update_display`, so the redraw happens twice: once immediately through `EL_REFRESH` and once when the pushed character is consumed and runs whatever is bound to it. If the terminal has no reprint character configured a NUL byte is pushed, which `el_push` treats as an empty push.
- rule: `[spec:libedit:sem:readline.rl-redisplay-fn]` · C: `src/readline.c` `rl_redisplay`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-27** — `add_history` bumps `history_base` whenever the event count did not change, on the theory that the oldest event was evicted — but the duplicate-suppression path also leaves the count unchanged, so a suppressed duplicate bumps `history_base` as though an eviction had happened.
- rule: `[spec:libedit:sem:readline.add-history-fn]` · C: `src/readline.c` `add_history`
- class: logic · reach: only with `H_UNIQUE`, which this layer never enables — dormant but wrong.
- disposition: reproduce.
- status: open

**ERR-readline-28** — `clear_history` zeroes `history_offset` and `history_length` but deliberately leaves `history_base` at whatever `add_history`/`stifle_history` last set it to, so after a clear the base can still exceed 1 and `history_get` rejects small indices.
- rule: `[spec:libedit:sem:readline.clear-history-fn]`, `[spec:libedit:sem:readline.history-get-fn]` · C: `src/readline.c` `clear_history`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-29** — `history_expand("")` returns 0 with `*output == NULL` rather than an empty string, because the main loop never runs and the `result` accumulator is still NULL. Several other allocation failures also surface as 0, sometimes with `*output` NULL and sometimes dangling (see ERR-readline-13).
- rule: `[spec:libedit:sem:readline.history-expand-fn]` · C: `src/readline.c` `history_expand`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-30** — `history_arg_extract` replaces a `start` or `end` equal to the integer 36 with `max`, a vestigial hack for the character `'$'` that the only in-tree caller never uses (it passes -1). A genuine word index of 36 is therefore silently reinterpreted as "last word".
- rule: `[spec:libedit:sem:readline.history-arg-extract-fn]` · C: `src/readline.c` `history_arg_extract`
- class: logic · reach: cold — needs a history line with 37+ words and a `:36` designator.
- disposition: reproduce.
- status: open

**ERR-readline-31** — `rl_getc_function` is sampled **once**, at `rl_initialize`, and only for non-NULL-ness; setting it afterwards has no effect until another `rl_initialize()`, which tears down and rebuilds both the editor and the history. Worse, `readline()` installs `_rl_event_read_char` whenever `rl_event_hook` is set and, when the hook is later cleared, restores the **builtin** reader rather than the `rl_getc_function` wrapper — so once an event hook has been used the getc hook stays disabled for the rest of the session. GNU readline consults `rl_getc_function` on every character with no installation step.
- rule: `[spec:libedit:sem:readline.rl-getc-function-fn]`, `[spec:libedit:sem:readline.readline-fn]` (step 10), `[spec:libedit:sem:readline.rl-initialize-fn]` (step 10) · C: `src/readline.c`
- class: logic · reach: hot for any consumer using either hook.
- disposition: reproduce — the rule says a port must reproduce the asymmetry.
- status: open

**ERR-readline-32** — `_getc_function` and `_rl_event_read_char` both widen a value straight to `wchar_t` with **no multibyte decoding**: the former casts the hook's `int`, the latter reads exactly one `char` byte from the descriptor. Under a UTF-8 locale a byte-oriented hook ported from GNU readline produces mojibake for every non-ASCII input, and a signed `char` byte above 0x7F becomes a negative `wchar_t`. `_getc_function` also treats *only* exactly -1 as EOF, storing any other negative value as a character, and dereferences `rl_getc_function` with no NULL check.
- rule: `[spec:libedit:sem:readline.getc-function-fn]`, `[spec:libedit:sem:readline.rl-event-read-char-fn]`, `[spec:libedit:sem:readline.rl-getc-function-fn]` · C: `src/readline.c`
- class: logic · reach: hot in any non-ASCII locale.
- disposition: reproduce.
- status: open

**ERR-readline-33** — `_rl_event_read_char` loops calling `rl_event_hook` and retrying the read with **no sleep, no `select` and no `poll`** — a busy spin that calls the application hook as fast as the CPU allows whenever no input is pending.
- rule: `[spec:libedit:sem:readline.rl-event-read-char-fn]` · C: `src/readline.c` `_rl_event_read_char`
- class: logic · reach: hot for any consumer using `rl_event_hook`.
- disposition: reproduce the polling contract; the port may not change the hook's call pattern, which applications depend on.
- status: open

**ERR-readline-34** — `rl_bind_wrapper` propagates the globals *into* the callback only. Changes the readline command makes to `rl_point`, `rl_end` or `rl_line_buffer` are never written back into EditLine's wide line, so commands that edit through the globals have no effect; only ones that call back into `rl_insert_text`/`rl_delete_text`/`rl_replace_line` change the line. It also discards the callback's return value, so a readline command cannot report failure.
- rule: `[spec:libedit:sem:readline.rl-bind-wrapper-fn]`, `[spec:libedit:sem:readline.rl-update-pos-fn]` · C: `src/readline.c` `rl_bind_wrapper`
- class: logic · reach: hot for any `rl_add_defun` consumer.
- disposition: reproduce.
- status: open

**ERR-readline-35** — `rl_delete_text` forwards to `el_deletestr1`, which works in **wide characters** against `el_line`, while `rl_point`/`rl_end` are byte offsets into the encoded line — so in a multibyte locale the offsets a readline application computes from `rl_line_buffer` do not correspond to what is deleted. `rl_insert_text` mirrors it: it returns `(int)strlen(text)`, a byte count, not the number of characters added. Neither refreshes `rl_point`/`rl_end`.
- rule: `[spec:libedit:sem:readline.rl-delete-text-fn]`, `[spec:libedit:sem:readline.rl-insert-text-fn]`, `[spec:libedit:sem:chared.el-deletestr1-fn]` · C: `src/readline.c`
- class: logic · reach: hot in a UTF-8 locale.
- disposition: reproduce.
- status: open

**ERR-readline-36** — `rl_message` formats into a 160-byte stack buffer with `vsnprintf`, silently truncating at 159 characters, and installs the result *as* `rl_prompt`, overwriting whatever was there — so an application that does not call `rl_save_prompt` first loses the original prompt for good. The 160-byte limit has no readline counterpart.
- rule: `[spec:libedit:sem:readline.rl-message-fn]` · C: `src/readline.c` `rl_message`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-37** — `rl_callback_read_char` calls `el_gets` *before* re-asserting unbuffered mode, so the very first call after `rl_callback_handler_install` behaves like a **blocking** line read; it sets `RL_STATE_DONE` in `rl_readline_state` and never clears it (only `rl_initialize` does); and if `rl_linefunc` is NULL it silently drops a completed line. `rl_callback_handler_remove` restores line-at-a-time reading but does not clear the line, redisplay, or free the prompt, unlike GNU readline.
- rule: `[spec:libedit:sem:readline.rl-callback-read-char-fn]`, `[spec:libedit:sem:readline.rl-callback-handler-remove-fn]` · C: `src/readline.c`
- class: logic · reach: hot for any callback-mode consumer.
- disposition: reproduce.
- status: open

**ERR-readline-38** — `history_is_stifled` returns `max_input_history != INT_MAX`, inspecting only the mirror global rather than the real cap inside the `History` object; the source comment concedes it "cannot return true answer". `stifle_history(INT_MAX)` sets a real cap and is reported as *not* stifled; a cap set directly through `H_SETSIZE` is invisible; and before any initialisation it returns 0 because the static initialiser is 0, not `INT_MAX`. `unstifle_history` returns the raw previous mirror value rather than readline's "previous stifle amount, or negative if not stifled".
- rule: `[spec:libedit:sem:readline.history-is-stifled-fn]`, `[spec:libedit:sem:readline.unstifle-history-fn]` · C: `src/readline.c`
- class: logic · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-39** — `current_history` looks the entry up with `H_PREV_EVENT, history_offset + 1`, an implied identity between a zero-based readline offset and a one-based libedit event number that holds only while event numbering is dense and starts at 1 — which `H_CLEAR` and any `H_DEL` break. `next_history`/`previous_history` reposition with `H_LAST` and then scan, making every navigation step O(n).
- rule: `[spec:libedit:sem:readline.current-history-fn]`, `[spec:libedit:sem:readline.next-history-fn]`, `[spec:libedit:sem:readline.previous-history-fn]` · C: `src/readline.c`
- class: logic · reach: hot after any deletion or clear.
- disposition: reproduce.
- status: open

**ERR-readline-40** — `history_offset` is maintained by only some of the functions that change the list: `read_history`, `history_search`, `history_search_prefix` and `remove_history` all leave it stale, so `where_history()` no longer describes the list after any of them. `read_history` likewise does not adjust `history_base`.
- rule: `[spec:libedit:sem:readline.where-history-fn]`, `[spec:libedit:sem:readline.read-history-fn]`, `[spec:libedit:sem:readline.remove-history-fn]`, `[spec:libedit:sem:readline.history-search-fn]` · C: `src/readline.c`
- class: logic · reach: hot.
- disposition: reproduce — callers are expected to follow with `using_history()`.
- status: open

**ERR-readline-41** — `rl_insert` and `rl_stuff_char` build their one-character string with a lossy `(char)c` narrowing, so no multibyte character can be expressed; `rl_stuff_char` with `c == 0` inserts nothing. `rl_get_previous_history` has the same narrowing.
- rule: `[spec:libedit:sem:readline.rl-insert-fn]`, `[spec:libedit:sem:readline.rl-stuff-char-fn]`, `[spec:libedit:sem:readline.rl-get-previous-history-fn]` · C: `src/readline.c`
- class: logic · reach: hot in a non-ASCII locale.
- disposition: reproduce.
- status: open

**ERR-readline-42** — `rl_insert` and `rl_stuff_char` are **swapped** relative to GNU readline: readline's `rl_insert` inserts into the line buffer and `rl_stuff_char` pushes onto pending input; libedit's `rl_insert` pushes onto the input queue (so the key's current binding runs) and `rl_stuff_char` inserts into the line. Neither reports failure.
- rule: `[spec:libedit:sem:readline.rl-insert-fn]`, `[spec:libedit:sem:readline.rl-stuff-char-fn]` · C: `src/readline.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-43** — `rl_bind_key` supports exactly **one** function: it compares `func` against this library's own `rl_insert` by address and, on a match, binds the key to `ED_INSERT`. Any other function is silently ignored and -1 returned, so `rl_add_defun` — which binds a single byte — is the only working route to a custom binding.
- rule: `[spec:libedit:sem:readline.rl-bind-key-fn]`, `[spec:libedit:sem:readline.rl-add-defun-fn]` · C: `src/readline.c` `rl_bind_key`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-44** — `rl_get_previous_history` pushes the key byte back onto the input queue `count` times rather than moving through history, so it only works if `key` happens to be bound to a history-recall command; it touches no history global and always returns 0. `rl_newline` is `rl_insert(1, '\n')`, so its effect depends on the current binding of `'\n'` rather than accepting the line directly.
- rule: `[spec:libedit:sem:readline.rl-get-previous-history-fn]`, `[spec:libedit:sem:readline.rl-newline-fn]` · C: `src/readline.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-45** — `rl_variable_bind(var, value)` is `el_set(e, EL_BIND, "", var, value, NULL)`, i.e. `bind <var> <value>`: `var` is interpreted as a **key** and `value` as the **command**. This is not readline's variable namespace at all, so `rl_variable_bind("editing-mode", "vi")` and its kin do not do what a readline application expects and most variable names are rejected as unparsable key specifications.
- rule: `[spec:libedit:sem:readline.rl-variable-bind-fn]` · C: `src/readline.c` `rl_variable_bind`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-46** — `rl_parse_and_bind` and `rl_read_init_file` accept libedit's `.editrc` grammar (`bind`, `echotc`, `settc`, `setty`, `telltc`, `history`), not readline's `.inputrc` grammar, so most real `.inputrc` lines are rejected. `rl_initialize` reads `$EDITRC`/`~/.editrc` and never `~/.inputrc`. `rl_read_init_file` returns `el_source`'s -1 where readline returns an errno value.
- rule: `[spec:libedit:sem:readline.rl-parse-and-bind-fn]`, `[spec:libedit:sem:readline.rl-read-init-file-fn]`, `[spec:libedit:sem:readline.rl-initialize-fn]` (step 20) · C: `src/readline.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-47** — `rl_replace_line("")` is a silent no-op (the function returns immediately for a NULL or empty `text`), where GNU readline's clears the line. Applications that clear the line that way get no effect at all.
- rule: `[spec:libedit:sem:readline.rl-replace-line-fn]` · C: `src/readline.c` `rl_replace_line`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-48** — libedit's `HISTORY_STATE` has exactly one member, `int length`, where GNU readline's also carries `entries`, `offset`, `size` and `flags`. A program compiled against the GNU header and run against libedit reads past the end of `history_get_history_state`'s allocation. There is no `history_set_history_state` counterpart.
- rule: `[spec:libedit:sem:readline.history-get-history-state-fn]` · C: `src/editline/readline.h`, `src/readline.c`
- class: divergence · reach: hot for a mismatched build.
- disposition: reproduce — the struct definition is frozen by the drop-in requirement.
- status: open

**ERR-readline-49** — return-convention divergences: `history_search_prefix` returns 0/-1 where GNU readline returns the offset of the matching line; `history_search` returns the byte offset of the match but does not update `history_offset` as readline does; `add_history` returns `int` where readline declares it `void`, and the value carries no information (always 0), so success and allocation failure are indistinguishable; `read_history`/`write_history`/`append_history`/`history_truncate_file` return a positive errno on failure and never -1.
- rule: `[spec:libedit:sem:readline.history-search-prefix-fn]`, `[spec:libedit:sem:readline.history-search-fn]`, `[spec:libedit:sem:readline.add-history-fn]`, `[spec:libedit:sem:readline.read-history-fn]`, `[spec:libedit:sem:readline.write-history-fn]`, `[spec:libedit:sem:readline.append-history-fn]` · C: `src/readline.c`
- class: divergence · reach: hot.
- disposition: reproduce.
- status: open

**ERR-readline-50** — `rl_complete` passes the `rl_completion_word_break_hook` result into `fn_complete2`'s **special-prefixes** slot, not its word-break slot, which always receives `rl_basic_word_break_characters`. Because `find_word_to_complete` stops at a character in *either* set, the hook can only **add** break characters — never remove or replace one, as GNU readline allows. The globals `rl_completer_word_break_characters` and `rl_special_prefixes` are declared but read by no code path at all. The hook also runs *before* `_rl_update_pos`, so `rl_point`/`rl_end` are stale inside it, and its returned pointer is converted through a never-freed function-static buffer and never released.
- rule: `[spec:libedit:sem:readline.rl-completion-word-break-hook-fn]`, `[spec:libedit:sem:readline.rl-complete-fn]` · C: `src/readline.c` `rl_complete`
- class: divergence · reach: hot for any consumer setting the hook.
- disposition: reproduce.
- status: open

**ERR-readline-51** — `_history_expand_command`'s `&` modifier falls through into the `s` case, which immediately takes the *next* character as the delimiter, so `&` does not repeat the previous substitution the way GNU readline's does: a bare `!!:&` reads NUL as the delimiter and errors out. `getfrom` also treats `!!:s/foo/` — nothing after the second delimiter — as an error rather than a deletion.
- rule: `[spec:libedit:sem:readline.history-expand-command-fn]`, `[spec:libedit:sem:readline.getfrom-fn]` · C: `src/readline.c`
- class: divergence · reach: hot for shell-style history expansion.
- disposition: reproduce.
- status: open

**ERR-readline-52** — `history_tokenize` diverges from GNU readline: the shell metacharacters `()<>;&|$` terminate a word but are **not** emitted as words of their own (readline returns them as separate tokens), and the scanner then skips one character so a metacharacter between two words yields an extra empty token; quotes and backslashes are retained verbatim with no unquoting; `""` returns NULL rather than an empty array, while `"   "` returns an array holding one empty string.
- rule: `[spec:libedit:sem:readline.history-tokenize-fn]` · C: `src/readline.c` `history_tokenize`
- class: divergence · reach: hot — every `:n` word designator goes through it.
- disposition: reproduce.
- status: open

**ERR-readline-53** — `completion_matches`' prototype in `editline/readline.h` takes `char *` while the definition in `filecomplete.c` takes `const char *`: formally incompatible across translation units, harmless at the ABI level, but strict C and C++ consumers must cast at the call site. The symbol also has default ELF visibility and libedit's own `fn_complete2` call site is not protected against interposition, so an application defining its own `completion_matches` — which old readline programs sometimes do — preempts libedit's and becomes what libedit's internal TAB completion calls.
- rule: `[spec:libedit:sem:readline.completion-matches-fn]` · C: `src/editline/readline.h`, `src/filecomplete.c`
- class: divergence · reach: hot for old readline consumers.
- disposition: reproduce — the rule warns that a Rust ABI crate hiding the symbol or calling a private copy internally would silently change this.
- status: open

**ERR-readline-54** — inert stubs and unused machinery, all of which report success so a caller cannot detect that nothing happened: the entire Keymap API (`rl_get_keymap` and `rl_make_bare_keymap` return NULL, `rl_set_keymap` does nothing, `rl_generic_bind`, `rl_bind_key_in_map`, `rl_set_key` and `rl_set_keymap_name` ignore their arguments and return 0, and `emacs_standard_keymap`/`emacs_meta_keymap`/`emacs_ctlx_keymap` exist only so programs link); `rl_abort`, `rl_kill_text`, `rl_on_new_line`, `_rl_erase_entire_line`, `rl_cleanup_after_signal`, `rl_free_line_state` and `rl_set_keyboard_input_timeout`; `rl_redisplay_function` and `rl_completion_display_matches_hook`, exported but never consulted; `rl_catch_signals` and `rl_catch_sigwinch`, exported but not honoured; `_rl_qsort_string_compare`, correct and unused (see ERR-readline-01); `#define TAB '\r'`; the commented-out `GDB_411_HACK` block; and `getfrom`'s `search != NULL` arm, which is dead because `_history_expand_command` never assigns `search` — and which, if reached, would free `what` a second time.
- rule: `[spec:libedit:sem:readline.rl-get-keymap-fn]`, `[spec:libedit:sem:readline.rl-make-bare-keymap-fn]`, `[spec:libedit:sem:readline.rl-set-keymap-fn]`, `[spec:libedit:sem:readline.rl-generic-bind-fn]`, `[spec:libedit:sem:readline.rl-bind-key-in-map-fn]`, `[spec:libedit:sem:readline.rl-set-key-fn]`, `[spec:libedit:sem:readline.rl-set-keymap-name-fn]`, `[spec:libedit:sem:readline.rl-abort-fn]`, `[spec:libedit:sem:readline.rl-kill-text-fn]`, `[spec:libedit:sem:readline.rl-on-new-line-fn]`, `[spec:libedit:sem:readline.rl-erase-entire-line-fn]`, `[spec:libedit:sem:readline.rl-cleanup-after-signal-fn]`, `[spec:libedit:sem:readline.rl-free-line-state-fn]`, `[spec:libedit:sem:readline.rl-set-keyboard-input-timeout-fn]`, `[spec:libedit:sem:readline.rl-forced-update-display-fn]`, `[spec:libedit:sem:readline.rl-display-match-list-fn]`, `[spec:libedit:sem:readline.rl-resize-terminal-fn]`, `[spec:libedit:sem:readline.rl-complete-fn]`, `[spec:libedit:sem:readline.history-expand-fn]`, `[spec:libedit:sem:readline.getfrom-fn]`, `[spec:libedit:sem:readline.rl-qsort-string-compare-fn]` · C: `src/readline.c`, `src/editline/readline.h`
- class: dead · reach: the symbols must resolve; the behaviour is nil.
- disposition: reproduce the symbols and their return values (applications link against them and check them); do not port the dead bodies.
- status: open
