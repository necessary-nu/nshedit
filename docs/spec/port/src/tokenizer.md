# src/tokenizer.c

> [spec:libedit:def:tokenizer.fun-tok-end-fn]
> void FUN(tok,end)(TYPE(Tokenizer) *tok)

> [spec:libedit:sem:tokenizer.fun-tok-end-fn]
> Destroys a tokenizer and everything it owns. Four `free` calls, in
> order: the `ifs` copy made by
> `[spec:libedit:sem:tokenizer.fun-tok-init-fn]`, the word buffer
> `wspace`, the `argv` array, then the `tokenizer` struct itself. No
> return value, no failure mode, and no other side effects — nothing is
> zeroed first.
>
> There is no NULL guard: the first statement dereferences `tok` to reach
> `tok->ifs`, so `tok_end(NULL)` is undefined behaviour, not a no-op.
> Calling it twice on the same tokenizer is a double free.
>
> Every pointer the tokenizer has ever handed a caller becomes dangling
> here: the `argv` array published through
> `[spec:libedit:sem:tokenizer.fun-tok-line-fn]`'s `argv` out-parameter,
> and every word string in it, live inside the two blocks freed above.
>
> Compiled twice, as `tok_end` over `Tokenizer` and as `tok_wend` over
> `TokenizerW`; the two differ only in the element type of the word
> buffer and so are byte-for-byte the same logic.

> [spec:libedit:def:tokenizer.fun-tok-finish-fn]
> static void FUN(tok,finish)(TYPE(Tokenizer) *tok)

> [spec:libedit:sem:tokenizer.fun-tok-finish-fn]
> Terminates the word currently under construction and, if the word
> counts as present, publishes it into `argv`. Static; the only two call
> sites are inside `[spec:libedit:sem:tokenizer.fun-tok-line-fn]` — the
> separator branch of its main loop, and its success exit.
>
> The word under construction occupies `wstart .. wptr` in the single
> flat word buffer `wspace`. Steps:
>
> 1. Store a NUL element at `*wptr`, without advancing `wptr`. This NUL
>    terminates the pending word in place.
> 2. If `TOK_KEEP` is set in `flags`, **or** `wptr != wstart` (the word
>    produced at least one element), publish it:
>    - `argv[argc] = wstart`;
>    - `argc += 1`;
>    - `argv[argc] = NULL`;
>    - advance `wptr` by one (past the NUL just written) and set
>      `wstart = wptr`.
>    Words are therefore packed back to back in one allocation, each NUL
>    terminated, and the next word begins immediately after the previous
>    word's terminator.
> 3. Otherwise — empty word with `TOK_KEEP` clear — publish nothing.
>    `argc`, `wstart` and `wptr` are all unchanged, and the NUL written in
>    step 1 is inert (the next element emitted overwrites it). This is what
>    makes a run of separators collapse rather than yield empty arguments.
> 4. Unconditionally clear `TOK_KEEP` in `flags`. `TOK_EAT` is not touched.
>
> `TOK_KEEP` is the "this word exists even though it produced no
> elements" flag. `tok_line` sets it on every `'`, `"` and `\` it reads,
> in every quote state, so `''`, `""` and a lone `\` each yield a
> zero-length argument, while whitespace alone yields none. Because the
> flag is cleared here and nowhere else, it is scoped to one word, and it
> survives across a `tok_line` continuation return (1, 2 or 3) along with
> the rest of the state.
>
> There is no bounds check and no allocation. The function relies on its
> caller having reserved at least one free element in the word buffer and
> two free slots in `argv`; `tok_line`'s growth checks maintain exactly
> that headroom. `argv[argc] = NULL` is written only on the publishing
> path, so the array's NUL terminator is maintained only when a word is
> actually added — see
> `[spec:libedit:sem:tokenizer.fun-tok-reset-fn]` for the case where that
> invariant is observably broken.

> [spec:libedit:def:tokenizer.fun-tok-init-fn]
> TYPE(Tokenizer) * FUN(tok,init)(const Char *ifs)

> [spec:libedit:sem:tokenizer.fun-tok-init-fn]
> Allocates and initialises a tokenizer. Returns the new tokenizer, or
> NULL if any of the four allocations fails — every partial allocation is
> released before returning NULL, so failure leaks nothing.
>
> Steps, in order, each aborting to the NULL return on failure:
>
> 1. `malloc` the `tokenizer` struct. On failure return NULL.
> 2. Set `ifs` to a fresh duplicate (`strdup` narrow / `wcsdup` wide) of
>    the caller's `ifs`, or of the default separator set `"\t \n"` — tab,
>    space, newline — when the caller passes NULL. On failure free the
>    struct and return NULL. The caller's string is copied, not retained;
>    it need only live for the duration of the call, and it must be NUL
>    terminated or the duplication is undefined. An empty `ifs` is legal
>    and means nothing separates words, so a whole line becomes one
>    argument.
> 3. `argc = 0`; `amax = AINCR = 10`; `argv = malloc(10 * sizeof(const
>    Char *))`. On failure free `ifs` and the struct, and return NULL.
>    Then set `argv[0] = NULL`, so a tokenizer that has never produced a
>    word still presents a NULL-terminated array.
> 4. `wspace = malloc(WINCR * sizeof(Char))`, i.e. room for 20 elements.
>    On failure free `argv`, `ifs` and the struct, and return NULL. The
>    contents are left uninitialised.
> 5. `wmax = wspace + 20` (one past the end of the word buffer);
>    `wstart = wspace`; `wptr = wspace`; `flags = 0` (both `TOK_KEEP` and
>    `TOK_EAT` clear); `quote = Q_none`.
>
> Both buffers grow on demand inside
> `[spec:libedit:sem:tokenizer.fun-tok-line-fn]`; the initial sizes are not
> limits.
>
> The whole file is compiled twice from one source. The narrow
> instantiation is `tok_init`, over `char`, `Tokenizer`, `LineInfo`, with
> `strchr`/`strdup`; the wide one is `tok_winit`, over `wchar_t`,
> `TokenizerW`, `LineInfoW`, with `wcschr`/`wcsdup`. Nothing else differs
> — in particular the wide build does no multibyte decoding of its own and
> the narrow build is purely byte-oriented. All rules in this file
> describe both instantiations.

> [spec:libedit:def:tokenizer.fun-tok-line-fn]
> int FUN(tok,line)(TYPE(Tokenizer) *tok, const TYPE(LineInfo) *line, int *argc, const Char ***argv, int *cursorc, int *cursoro)

> [spec:libedit:sem:tokenizer.fun-tok-line-fn]
> The tokenizer proper: a Bourne-shell-like word splitter over the region
> `line->buffer .. line->lastchar`, appending the words it finds to the
> tokenizer's accumulated `argv`. Returns 0 for a complete parse, 1, 2 or
> 3 when the input ran out mid-construct and a continuation line is
> expected, and -1 on internal error.
>
> **Input extent.** `line->buffer` is the first element and
> `line->lastchar` points one past the last. The buffer need **not** be
> NUL terminated; `lastchar` alone delimits it, and no element at or past
> `lastchar` is ever read. A NUL element occurring *before* `lastchar` is
> treated as end of input, so an embedded NUL truncates the line.
> `line->cursor` is consulted only for the `cursorc`/`cursoro`
> out-parameters; it may be NULL, or point outside `buffer..lastchar`,
> without that being an error.
>
> **No implicit reset.** This function never resets the tokenizer. It
> resumes from whatever the previous call left behind: the accumulated
> `argc`, the partially built word between `wstart` and `wptr`, the
> `quote` state, and `flags`. That is the entire continuation mechanism;
> see `[spec:libedit:sem:tokenizer.fun-tok-reset-fn]`.
>
> **Locals.** `cc` and `co` both start at -1. A cursor `ptr` starts at
> `line->buffer`.
>
> **Main loop.** Repeat forever; `ptr` advances by one element at the
> bottom of each pass. Each pass does (a), (b), (c) and then, only if (c)
> fell through rather than returning or exiting, (d).
>
> *(a) End-of-input substitution.* If `ptr >= line->lastchar`, replace
> `ptr` with a pointer to a static empty string, so `*ptr` reads as NUL.
> The intent is that the region past the input looks like an unlimited
> supply of NULs; see "The end-of-input overrun" below for where that
> intent is not actually delivered.
>
> *(b) Cursor capture.* If `ptr == line->cursor` — pointer identity,
> tested *after* (a) — record `cc = (int)argc` and
> `co = (int)(wptr - wstart)`. Both are captured *before* the element
> under the cursor is processed, so `cc` is the number of words completed
> so far and `co` the number of elements already emitted for the word in
> progress. Both casts are from `size_t`/`ptrdiff_t` and truncate past
> `INT_MAX`. Because (a) runs first, a cursor at or past `lastchar` — or
> NULL, or before `buffer` — never matches here and is handled by the
> fallback at the success exit.
>
> *(c) Dispatch on `*ptr`.* Five cases. Note first the flag housekeeping,
> which is per-case and not uniform: `'`, `"` and `\` each set `TOK_KEEP`
> and clear `TOK_EAT`; newline and any ordinary element clear `TOK_EAT`
> and leave `TOK_KEEP` alone; NUL touches neither flag on entry. "Emit x"
> below means `*wptr++ = x`.
>
> `'` (apostrophe) — set `TOK_KEEP`, clear `TOK_EAT`, then by `quote`:
>
> | `quote` | new `quote` | emits |
> | --- | --- | --- |
> | `Q_none` | `Q_single` | nothing (opens a single-quoted section) |
> | `Q_single` | `Q_none` | nothing (closes it) |
> | `Q_double` | `Q_double` | `'` (literal inside double quotes) |
> | `Q_one` | `Q_none` | `'` (`\'` yields `'`) |
> | `Q_doubleone` | `Q_double` | `'` |
> | other | — | `return -1` |
>
> The `Q_doubleone` row means `"\'"` yields a bare `'` — libedit **drops**
> the backslash there, where sh(1) would keep it and yield `\'`. This is
> a deliberate-looking divergence and must be preserved.
>
> `"` (double quote) — set `TOK_KEEP`, clear `TOK_EAT`:
>
> | `quote` | new `quote` | emits |
> | --- | --- | --- |
> | `Q_none` | `Q_double` | nothing (opens a double-quoted section) |
> | `Q_double` | `Q_none` | nothing (closes it) |
> | `Q_single` | `Q_single` | `"` (literal inside single quotes) |
> | `Q_one` | `Q_none` | `"` |
> | `Q_doubleone` | `Q_double` | `"` |
> | other | — | `return -1` |
>
> `\` (backslash) — set `TOK_KEEP`, clear `TOK_EAT`:
>
> | `quote` | new `quote` | emits |
> | --- | --- | --- |
> | `Q_none` | `Q_one` | nothing (quotes the next element) |
> | `Q_double` | `Q_doubleone` | nothing (quotes the next element) |
> | `Q_single` | `Q_single` | `\` (backslash is literal in single quotes) |
> | `Q_one` | `Q_none` | `\` (`\\` yields `\`) |
> | `Q_doubleone` | `Q_double` | `\` (`"\\"` yields `\`) |
> | other | — | `return -1` |
>
> Newline — clear `TOK_EAT`; `TOK_KEEP` is **not** set:
>
> | `quote` | action |
> | --- | --- |
> | `Q_none` | leave the loop at the success exit — the line is complete |
> | `Q_single` | emit the newline into the word, stay `Q_single` |
> | `Q_double` | emit the newline into the word, stay `Q_double` |
> | `Q_one` | set `TOK_EAT`, go to `Q_none`, emit nothing |
> | `Q_doubleone` | set `TOK_EAT`, go to `Q_double`, emit nothing |
> | other | `return 0` |
>
> Two things here. A newline inside either kind of quote is an ordinary
> element of the word and **does not end the line**: parsing continues
> with the rest of the buffer, so `a'b<newline>c'` is the single word
> `ab<newline>c`. And a backslash-newline pair is a line continuation:
> the newline is discarded and `TOK_EAT` records that the input stopped
> only because a continuation was requested. The `other` arm returns 0,
> not -1 as the other four switches do, and returns it without writing
> any out-parameter; it is unreachable given the five-valued `quote_t`,
> but it is an inconsistency rather than a designed behaviour and must
> not be mistaken for a success path.
>
> NUL — either an element inside the buffer or the end-of-input NUL
> synthesised by (a). Neither flag is touched on entry:
>
> | `quote` | action |
> | --- | --- |
> | `Q_none` | if `TOK_EAT` is set, clear it and `return 3`; otherwise leave the loop at the success exit |
> | `Q_single` | `return 1` |
> | `Q_double` | `return 2` |
> | `Q_one` | go to `Q_none`, emit a NUL element into the word, continue |
> | `Q_doubleone` | go to `Q_double`, emit a NUL element into the word, continue |
> | other | `return -1` |
>
> The two continuing rows are the only `'\0'` arms that do not return,
> and they are the source of the overrun described at the end of this
> rule.
>
> Any other element — clear `TOK_EAT`; `TOK_KEEP` is not set:
>
> | `quote` | action |
> | --- | --- |
> | `Q_none` | if the element occurs anywhere in `ifs` (a plain `strchr`/`wcschr` search), call `[spec:libedit:sem:tokenizer.fun-tok-finish-fn]`; otherwise emit it |
> | `Q_single` | emit it |
> | `Q_double` | emit it |
> | `Q_one` | go to `Q_none`, emit it |
> | `Q_doubleone` | emit `\`, go to `Q_double`, then emit it |
> | other | `return -1` |
>
> The `Q_doubleone` row is the rule that backslash inside double quotes
> is preserved before anything other than `'`, `"`, `\`, newline or NUL:
> `"\x"` yields the two elements `\x`. It is also the only arm that emits
> two elements in one pass, which is what the growth slack in (d) is
> sized for.
>
> **Separators.** Because the five switch cases are matched first, `'`,
> `"`, `\`, newline and NUL can *never* act as separators however `ifs`
> is set. The converse also bites: the default `ifs` is `"\t \n"` and
> does contain a newline, but that newline is consumed by the newline
> case, so in `Q_none` it terminates the line instead of separating a
> word — `tok_str("a\nb")` yields the single word `a` and silently
> discards `b`. An empty `ifs` means no element ever separates, so the
> whole line becomes one word. `ifs` is matched element-wise with no
> multibyte or locale awareness.
>
> *(d) Growth.* Reached only when (c) fell through; the `return` and
> success-exit paths skip it, which is safe because the previous pass left
> the required slack.
>
> - Word buffer: if `wptr >= wmax - 4`, `realloc` `wspace` to its current
>   capacity plus `WINCR = 20` elements. On failure `return -1`
>   immediately, leaving all tokenizer state untouched. On success, if the
>   block moved, rebase by the same displacement every published
>   `argv[i]` for `i < argc`, plus `wptr` and `wstart`, and set `wspace`
>   to the new block; either way set `wmax` to new block + new capacity.
> - `argv` array: if `argc >= amax - 4`, add `AINCR = 10` to `amax` and
>   `realloc` `argv`. On failure subtract the 10 back and `return -1`.
>
> Growth is linear, not doubling. The `- 4` slack covers the two elements
> the `Q_doubleone` ordinary-element arm can emit plus the one
> `tok_finish` writes, and the two `argv` slots `tok_finish` writes. These
> two reallocations are the reason `argv` and its strings have the
> lifetime stated below.
>
> **Success exit.** Reached from newline in `Q_none`, and from NUL in
> `Q_none` with `TOK_EAT` clear. In this order:
>
> 1. If `cc` and `co` are *both* still -1 — the cursor was never matched
>    — set `cc = (int)argc` and `co = (int)(wptr - wstart)` now, treating
>    the cursor as sitting at the current end of input. (They are always
>    both set or both unset, since (b) writes them together and neither
>    can be negative once written.)
> 2. If `cursorc` is non-NULL store `cc`; if `cursoro` is non-NULL store
>    `co`.
> 3. Call `[spec:libedit:sem:tokenizer.fun-tok-finish-fn]` exactly once, to
>    terminate and possibly publish the word still under construction.
> 4. `*argv = tok->argv`; `*argc = (int)tok->argc`.
> 5. Return 0.
>
> Step 3 running *after* step 2 is load-bearing: `cc` can therefore equal
> the final `*argc`.
>
> **Return values.**
>
> - **0** — complete parse. `*argc`, `*argv`, and `*cursorc`/`*cursoro`
>   where non-NULL, are all written. `argv[*argc]` is NULL, subject to the
>   `tok_reset` caveat in
>   `[spec:libedit:sem:tokenizer.fun-tok-reset-fn]`.
> - **1** — the input ended inside a single-quoted section. Unmatched `'`.
> - **2** — the input ended inside a double-quoted section. Unmatched `"`.
>   This also covers a backslash-newline that occurred inside double
>   quotes, because that returns the state to `Q_double` and it is
>   `Q_double` that the end-of-input NUL then meets — so `echo "ab\`
>   followed by a newline returns 2, never 3.
> - **3** — "quoted return": the input ended in `Q_none` with `TOK_EAT`
>   set, i.e. the last thing consumed was a backslash immediately followed
>   by a newline. `TOK_EAT` is cleared on the way out.
> - **-1** — internal error: a failed `realloc` in either growth step, or
>   an out-of-range `quote` value (unreachable).
>
> For 1, 2, 3 and -1 **none** of `*argc`, `*argv`, `*cursorc`, `*cursoro`
> is written; the caller's variables keep whatever they held. All four
> non-zero returns preserve the accumulated words, the partial word, the
> quote state and `TOK_KEEP`, so the intended response to 1, 2 or 3 is to
> prompt for more input and call `tok_line` again with the next line
> without resetting. Words then continue across the boundary: `echo
> "hello<newline>` returns 2, and feeding `world"<newline>` next yields
> the two arguments `echo` and `hello<newline>world` — the newline that
> sat inside the quotes is part of the word, whereas a backslash-newline
> pair contributes nothing (`echo "ab\<newline>` then `cd"<newline>`
> yields `echo` and `abcd`). Returns 1 and 2 leave `TOK_EAT` as they found
> it; only the return-3 path and the arrival of any element clear it.
>
> Note also that `TOK_KEEP` surviving a return-3 is observable: `\`
> followed by a newline, then an empty continuation line, yields one
> zero-length argument.
>
> **Cursor mapping.** `co` counts elements of the *produced* word, not
> input columns. Quote characters and the backslash of an escape
> contribute nothing, so a cursor before an opening `'` and a cursor just
> after it both map to offset 0, and a cursor on a backslash and on the
> element it escapes map to the same offset. Concretely, for `ab  cd`
> (two separators) with the cursor at input offset *n*:
>
> | *n* | `cc` | `co` | meaning |
> | --- | --- | --- | --- |
> | 0, 1, 2 | 0 | 0, 1, 2 | inside `ab`, or on the first separator after it |
> | 3, 4 | 1 | 0 | at the start of `cd` (which is not yet published) |
> | 6 (`== lastchar`) | 1 | 2 | end of `cd`, via the fallback |
>
> The asymmetry at *n* = 2 versus *n* = 3 is exact and intended to be
> reproduced: the *first* separator element after a word is captured
> before that word is finished, so it reports the end of the preceding
> word; every *later* separator in the run is captured after, so it
> reports offset 0 of the next word. In a run of trailing separators, or
> with the cursor at or past `lastchar`, `cc` equals the number of words
> finished and `co` is 0 — and since step 3 of the success exit publishes
> nothing for an empty unkept word, `cc == *argc` there, naming a word
> that does not exist. Callers must treat `cc == *argc` as valid and
> `argv[cc]` as absent. When the last word *is* kept — `ab ""` with the
> cursor at the end — `cc` is 1 and `*argc` is 2, so `cc` correctly names
> the empty word.
>
> Across a continuation the values are re-initialised to -1 on every call
> and reported in accumulated terms: `foo bar\<newline>` then `baz` gives
> `cc = 1`, `co = 6`, naming offset 6 of the word `barbaz`. The values are
> meaningful only when 0 is returned.
>
> **Ownership and lifetime.** `*argv` is the tokenizer's own `argv` array
> and its entries point into the tokenizer's own word buffer. The caller
> must neither free nor modify either, and must not retain them. Both are
> invalidated by any subsequent `tok_line` or `tok_str` on the same
> tokenizer — the array can be reallocated and the word buffer can move,
> and both were observed to move in practice — and by
> `[spec:libedit:sem:tokenizer.fun-tok-end-fn]`. `tok_reset` frees nothing
> but makes the next parse overwrite the contents from index 0. The rule
> for a caller is: read what you need out of `argv` before the next call.
>
> **The end-of-input overrun.** Substitution (a) points `ptr` at a
> one-element object and the loop then increments `ptr` past it. Whether
> (a) fires again on the following pass depends on comparing that
> out-of-object pointer against `line->lastchar` — a comparison between
> pointers into unrelated objects, which C does not define. This only
> matters when the input ends while in `Q_one` or `Q_doubleone`, i.e. the
> last element of the buffer is a backslash with no newline after it,
> because those are the only NUL arms that continue the loop.
>
> - If the comparison happens to hold, the next pass sees a NUL again and
>   the parse ends normally: the trailing backslash is silently swallowed,
>   an extra NUL element has been appended to the word buffer, and 0 is
>   returned. That extra NUL is not visible in `argv` (the word string is
>   already terminated by it) but *is* visible in `co`, which counts one
>   more than the word's length.
> - If it does not hold — the ordinary case on Linux/x86-64 whenever the
>   line buffer is on the heap or the stack, both of which sit above the
>   literal in the address space — the loop walks off the literal and
>   keeps tokenizing whatever read-only memory follows, appending it to
>   the word and growing the word buffer, until it happens to meet an
>   element that ends the loop, or it faults, or it meets a quote and
>   returns 1 or 2. Measured on gcc/glibc x86-64: `tok_str` on a
>   heap-allocated `"abc\"` reads a few hundred bytes past the literal
>   before stopping, and still returns 0 with `argv[0] == "abc"`.
>
> This is a genuine out-of-bounds read, reachable from `tok_str` and
> therefore from `rl_parse_and_bind` on any inputrc line ending in a
> backslash. A re-implementation must not reproduce it. Take the first
> branch as the intended semantics — a trailing backslash at end of input
> is dropped, the parse completes, 0 is returned — and treat the extra
> embedded NUL, whose only observable effect is on `co`, as the frozen
> part of the behaviour.
>
> Narrow instantiation `tok_line`, wide instantiation `tok_wline`. The
> five special elements are compared against the ASCII code points for
> `'`, `"`, `\`, newline and NUL in both builds, and the backslash the
> `Q_doubleone` arm re-emits is likewise ASCII. The narrow build compares
> bytes and performs no multibyte decoding, so under an encoding whose
> trailing bytes overlap ASCII a continuation byte can be mistaken for a
> quote or a separator; this cannot happen in UTF-8.

> [spec:libedit:def:tokenizer.fun-tok-reset-fn]
> void FUN(tok,reset)(TYPE(Tokenizer) *tok)

> [spec:libedit:sem:tokenizer.fun-tok-reset-fn]
> Returns the tokenizer to the state
> `[spec:libedit:sem:tokenizer.fun-tok-init-fn]` left it in, without
> reallocating anything. Exactly five assignments:
>
> - `argc = 0`
> - `wstart = wspace`
> - `wptr = wspace`
> - `flags = 0` (clears both `TOK_KEEP` and `TOK_EAT`)
> - `quote = Q_none`
>
> No return value and no failure mode. There is no NULL guard, so
> `tok_reset(NULL)` is undefined behaviour.
>
> What it deliberately does **not** do: it does not free or shrink
> `wspace` or `argv`, does not touch `ifs`, `amax` or `wmax`, and — the
> one that matters — does not restore `argv[0] = NULL`. The `argv` array
> keeps the pointers the previous parse wrote. They still address live
> storage inside `wspace` and remain dereferenceable, but they name bytes
> the next parse will overwrite from the front.
>
> That omission is an observable defect. If the next
> `[spec:libedit:sem:tokenizer.fun-tok-line-fn]` publishes zero words —
> an empty or all-separator line — then
> `[spec:libedit:sem:tokenizer.fun-tok-finish-fn]` never runs its
> publishing branch, `argv[0]` is still the stale non-NULL pointer from
> before the reset, and the array is not NULL terminated. A caller that
> walks `argv` looking for NULL instead of bounding by `*argc` will read
> the previous line's words. Confirmed: `tok_str(t, "x y", ...)`,
> `tok_reset(t)`, `tok_str(t, "   ", ...)` yields `argc == 0` with a
> non-NULL `argv[0]`. Only a tokenizer fresh from `tok_init` is
> guaranteed NULL-terminated at index 0.
>
> Reset is the boundary between independent lines. Skipping it is not an
> error — it is the continuation protocol: a subsequent `tok_line`
> appends its words to the same `argv` and continues the same partial
> word. Callers therefore reset after a `tok_line` that returned 0, and
> do not reset after one that returned 1, 2 or 3.

> [spec:libedit:def:tokenizer.fun-tok-str-fn]
> int FUN(tok,str)(TYPE(Tokenizer) *tok, const Char *line, int *argc, const Char ***argv)

> [spec:libedit:sem:tokenizer.fun-tok-str-fn]
> Convenience wrapper over
> `[spec:libedit:sem:tokenizer.fun-tok-line-fn]` for a NUL-terminated
> string, with no cursor tracking. Four steps:
>
> 1. Declare a `LineInfo` on the stack and `memset` it to zero. The
>    memset is redundant — all three of its fields are assigned
>    immediately after — and is not observable.
> 2. `li.buffer = line`.
> 3. `li.cursor = li.lastchar = Strchr(line, '\0')`, i.e. both are set to
>    the address of `line`'s terminating NUL, which is one past its last
>    character. `line` must be non-NULL and NUL terminated; otherwise the
>    search runs off the end and the behaviour is undefined.
> 4. Return `tok_line(tok, &li, argc, argv, NULL, NULL)` — its return
>    value verbatim.
>
> Everything else is `tok_line`'s: the quoting grammar, the separator
> rules, the growth and ownership rules for `argv`, the accumulate-until-
> reset protocol, and the full return-code set 0, 1, 2, 3 and -1. In
> particular `tok_str` is *not* restricted to complete lines: given `"abc`
> it returns 2 and leaves the tokenizer inside a double-quoted word,
> exactly as `tok_line` would.
>
> Two consequences of step 3 are worth stating. First, because
> `lastchar` is the terminating NUL rather than one past it, `tok_line`
> never reads that NUL — it substitutes its own end-of-input NUL at the
> same position — so the parse cannot see an embedded NUL and stops at
> the first one either way. Second, because `cursor == lastchar`,
> `tok_line`'s in-loop cursor match can never fire; its cursor
> bookkeeping falls through to the end-of-input fallback and is then
> discarded, since both cursor out-parameters are NULL.
>
> Both in-tree callers (`parse_line` in `src/parse.c`, `rl_parse_and_bind`
> in `src/readline.c`) discard the return value and read `*argc`/`*argv`
> unconditionally. On any non-zero return those out-parameters were never
> written, so those callers consume indeterminate values — a caller-side
> bug that the return-code contract here does not excuse.
>
> Narrow instantiation `tok_str`, wide instantiation `tok_wstr`.

> [spec:libedit:def:tokenizer.quote-t]
> typedef enum

