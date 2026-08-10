# src/history.c

> [spec:libedit:def:history.fun-history-end-fn]
> void FUN(history,end)(TYPE(History) *h)

> [spec:libedit:sem:history.fun-history-end-fn]
> Destroys a history object. `history_end()` narrow / `history_wend()`
> wide. No return value.
>
> 1. Declare an uninitialised local `TYPE(HistEvent) ev` used only as
>    scratch for the clear callback.
> 2. If `h->h_next == history_def_next` (the built-in function set is
>    still installed), call
>    [spec:libedit:sem:history.history-def-clear-fn] on `h->h_ref`: every
>    entry is unlinked, its string freed and its node freed; `cursor` is
>    reset to the sentinel and `eventid` and `cur` are set to 0.
> 3. `free(h->h_ref)` — **unconditionally**, including when a
>    caller-supplied function set was installed via `H_FUNC`. In that case
>    the pointer freed is whatever `h_ref` currently holds. See
>    [spec:libedit:sem:history.history-set-fun-fn]: because that function
>    never copies the caller's reference into `h_ref`, what actually gets
>    freed here is the built-in `history_t`, not the caller's object — but
>    a port that "fixes" `history_set_fun` inherits a free of caller-owned
>    memory here. Treat the unconditional free as the specified behaviour
>    and keep the two consistent.
> 4. `free(h)`.
>
> `h` is dangling afterwards. `h` is not checked for `NULL`; passing
> `NULL` dereferences it — undefined behaviour, not a defined no-op. Any
> `HistEvent` the caller still holds from an earlier call has a dangling
> `str`, because entry strings are owned by the history and freed in step
> 2. The per-entry `data` pointers set through `H_REPLACE` / read through
> `H_NEXT_EVDATA` are **not** freed: that memory belongs to the caller and
> is simply forgotten.

> [spec:libedit:def:history.fun-history-init-fn]
> TYPE(History) * FUN(history,init)(void)

> [spec:libedit:sem:history.fun-history-init-fn]
> Allocates and returns a new default-backed history object, or `NULL` on
> allocation failure. This is `history_init()` in the narrow build
> (`src/historyn.c`, `NARROWCHAR`, `Char` = `char`, `TYPE(History)` =
> `History`) and `history_winit()` in the wide build (`Char` = `wchar_t`,
> `TYPE(History)` = `HistoryW`); the two are the same code compiled twice
> and hold entirely separate state.
>
> Steps:
>
> 1. `malloc(sizeof(struct history))`. On failure return `NULL` at once.
> 2. Declare an *uninitialised* local `TYPE(HistEvent) ev`. It is passed
>    to `history_def_init` purely as a formal argument and is never read
>    or written; a port needs no such value.
> 3. Call [spec:libedit:sem:history.history-def-init-fn] with `&h->h_ref`,
>    `&ev` and `n = 0`, which allocates the built-in `history_t` backing
>    store. If it returns -1, `free` the `struct history` and return
>    `NULL`.
> 4. Set `h_ent = -1` (the "no event has been entered yet" marker used by
>    `H_APPEND`).
> 5. Install the built-in function set, all ten slots: `h_next =
>    history_def_next`, `h_first = history_def_first`, `h_last =
>    history_def_last`, `h_prev = history_def_prev`, `h_curr =
>    history_def_curr`, `h_set = history_def_set`, `h_clear =
>    history_def_clear`, `h_enter = history_def_enter`, `h_add =
>    history_def_add`, `h_del = history_def_del`.
> 6. Return `h`.
>
> Two consequences a port must preserve. First, the identity test used
> everywhere else in this file to decide "is this the built-in
> implementation?" is `h->h_next == history_def_next` — a single pointer
> comparison, not a flag; `history_setsize`, `history_getsize`,
> `history_setunique`, `history_getunique`, `history_set_fun` and
> `FUN(history,end)` all key off it. Second, the initial maximum size is
> **0**, not unlimited: until the caller issues `H_SETSIZE`, every
> `H_ENTER` inserts an entry and then immediately evicts it (see
> [spec:libedit:sem:history.history-def-enter-fn]), so a freshly
> initialised history retains nothing.

> [spec:libedit:def:history.funw-history-fn]
> int FUNW(history)(TYPE(History) *h, TYPE(HistEvent) *ev, int fun, ...)

> [spec:libedit:sem:history.funw-history-fn]
> The public varargs entry point: `history()` in the narrow build,
> `history_w()` in the wide build. Dispatches on the opcode `fun` and
> returns an `int` whose meaning depends on the opcode. Neither `h` nor
> `ev` is checked for `NULL`.
>
> Prologue, always: `va_start`, then `he_seterrev(ev, _HE_OK)`, i.e.
> `ev->num = 0` and `ev->str` = the static string `"OK"`. Epilogue,
> always: `va_end`, then `return retval`. Consequence: any opcode whose
> handler does not itself write `*ev` leaves the caller looking at
> `num = 0`, `str = "OK"` — this is true of `H_SET`, `H_CLEAR`,
> `H_REPLACE` and the successful paths of `H_SETSIZE` / `H_SETUNIQUE`, and
> `H_GETSIZE` / `H_GETUNIQUE` overwrite only `num`.
>
> The error strings reachable through `he_seterrev` are a fixed table
> indexed by code; a port must reproduce the exact text because it is
> observable through `ev->str`:
> `0 "OK"`, `1 "unknown error"`, `2 "malloc() failed"`,
> `3 "first event not found"`, `4 "last event not found"`,
> `5 "empty list"`, `6 "no next event"`, `7 "no previous event"`,
> `8 "current event is invalid"`, `9 "event not found"`,
> `10 "can't read history from file"`, `11 "can't write history"`,
> `12 "required parameter(s) not supplied"`, `13 "history size negative"`,
> `14 "function not allowed with other history-functions-set the default"`,
> `15 "bad parameters"`. These strings are static storage and must never
> be freed by the caller.
>
> The opcode numbers are ABI and are listed here because a port exporting
> the C ABI must match them exactly:
>
> - `H_FUNC` = 0. Eleven varargs, read in this order: `void *ref`, then
>   the ten callbacks `h_first`, `h_next`, `h_last`, `h_prev`, `h_curr`,
>   `h_set`, `h_clear`, `h_enter`, `h_add`, `h_del`. They are collected
>   into a stack-local `TYPE(History) hf` whose remaining field (`h_ent`)
>   is left uninitialised and never read. `h->h_ent = -1` is assigned
>   in the middle of the sequence, immediately after `ref` is read. Then
>   `retval = history_set_fun(h, &hf)`
>   ([spec:libedit:sem:history.history-set-fun-fn]); if that is -1, set
>   `_HE_PARAM_MISSING` (12) on `ev`.
> - `H_SETSIZE` = 1. One `int`. → [spec:libedit:sem:history.history-setsize-fn].
> - `H_GETSIZE` = 2. No vararg. → [spec:libedit:sem:history.history-getsize-fn].
> - `H_FIRST` = 3. No vararg. Calls `h_first(h_ref, ev)`.
> - `H_LAST` = 4. No vararg. Calls `h_last(h_ref, ev)`.
> - `H_PREV` = 5. No vararg. Calls `h_prev(h_ref, ev)`.
> - `H_NEXT` = 6. No vararg. Calls `h_next(h_ref, ev)`.
> - `H_SET` = 7. One `int`. Calls `h_set(h_ref, ev, n)`.
> - `H_CURR` = 8. No vararg. Calls `h_curr(h_ref, ev)`.
> - `H_ADD` = 9. One `const Char *`. Calls `h_add(h_ref, ev, str)`.
> - `H_ENTER` = 10. One `const Char *`. `retval = h_enter(h_ref, ev, str)`;
>   if `retval != -1` then `h->h_ent = ev->num`. Note the built-in enter
>   returns **1** on a real insert and **0** when `H_UNIQUE` suppressed
>   it; both are `!= -1`, so a suppressed enter stores `h_ent = 0` (the
>   event number left over from the `_HE_OK` prologue), and no real event
>   ever has number 0 — event ids start at 1. A subsequent `H_APPEND`
>   therefore fails with `_HE_NOT_FOUND`.
> - `H_APPEND` = 11. One `const Char *`. `retval = h_set(h_ref, ev,
>   h->h_ent)`; if that is `!= -1`, `retval = h_add(h_ref, ev, str)`.
>   With `h_ent == -1` (nothing entered yet) the set fails and the append
>   is skipped.
> - `H_END` = 12. No vararg. Calls `FUN(history,end)(h)`
>   ([spec:libedit:sem:history.fun-history-end-fn]) and sets `retval = 0`.
>   `h` is freed; the caller must not touch it again. `ev` remains valid
>   and reads `0`/`"OK"`.
> - `H_NEXT_STR` = 13. One `const Char *`. → [spec:libedit:sem:history.history-next-string-fn].
> - `H_PREV_STR` = 14. One `const Char *`. → [spec:libedit:sem:history.history-prev-string-fn].
> - `H_NEXT_EVENT` = 15. One `int`. → [spec:libedit:sem:history.history-next-event-fn].
> - `H_PREV_EVENT` = 16. One `int`. → [spec:libedit:sem:history.history-prev-event-fn].
> - `H_LOAD` = 17. One `const char *` path (always narrow `char`, in both
>   builds). → [spec:libedit:sem:history.history-load-fn]. If it returns
>   -1, set `_HE_HIST_READ` (10) on `ev`. Otherwise `retval` is the number
>   of data lines read.
> - `H_SAVE` = 18. One `const char *` path (narrow in both builds).
>   → [spec:libedit:sem:history.history-save-fn]. -1 → `_HE_HIST_WRITE` (11).
> - `H_CLEAR` = 19. No vararg. Calls `h_clear(h_ref, ev)`, which returns
>   `void`; `retval` is set to 0 unconditionally, so `H_CLEAR` can never
>   report failure.
> - `H_SETUNIQUE` = 20. One `int`. → [spec:libedit:sem:history.history-setunique-fn].
> - `H_GETUNIQUE` = 21. No vararg. → [spec:libedit:sem:history.history-getunique-fn].
> - `H_DEL` = 22. One `int` event number. Calls `h_del(h_ref, ev, num)`.
> - `H_NEXT_EVDATA` = 23. `int num`, then `void **d`.
>   → [spec:libedit:sem:history.history-next-evdata-fn].
> - `H_DELDATA` = 24. `int num`, then `void **d`. Calls
>   [spec:libedit:sem:history.history-deldata-nth-fn] with
>   `(history_t *)h->h_ref`. Passing `d == (void **)-1` is a documented
>   magic value meaning "position only, do not delete".
> - `H_REPLACE` = 25. `const Char *line`, then `void *data`. Documented as
>   usable only immediately after `H_NEXT_EVDATA`. If `line` is `NULL`, or
>   `Strdup(line)` returns `NULL`, set `retval = -1` and stop — leaving
>   `ev` at `0`/`"OK"`, so a failed `H_REPLACE` reports no error string.
>   Otherwise store the duplicate into
>   `((history_t *)h->h_ref)->cursor->ev.str` and `data` into
>   `cursor->data`, and return 0. The previous string is **overwritten
>   without being freed** — an unconditional leak of the old entry text.
>   If the cursor happens to be the list sentinel, this writes the
>   duplicate into the sentinel's `ev.str` (normally `NULL`), silently
>   corrupting the list header instead of any entry.
> - `H_SAVE_FP` = 26. One `FILE *`. Calls `history_save_fp(h, (size_t)-1,
>   fp)` ([spec:libedit:sem:history.history-save-fp-fn]). -1 →
>   `_HE_HIST_WRITE`. The stream is neither flushed nor closed.
> - `H_NSAVE_FP` = 27. `size_t nelem`, then `FILE *`. The `size_t` is read
>   into a local first, so argument evaluation order is well defined.
>   Calls `history_save_fp(h, nelem, fp)`. -1 → `_HE_HIST_WRITE`.
> - Anything else: `retval = -1` and `_HE_UNKNOWN` (1) on `ev`.
>
> `H_NEXT_EVDATA`, `H_DELDATA` and `H_REPLACE` all cast `h->h_ref` to
> `history_t *` with no check that the built-in implementation is
> installed. With a caller-supplied function set this is type-confused
> memory access — undefined behaviour, not a diagnosed error. A port
> should keep the operations restricted to the built-in backend.
>
> Varargs typing is the caller's responsibility and is unchecked: default
> argument promotion applies, so the `const int` shown in the header
> comments is read as plain `int`; `H_NSAVE_FP`'s first argument must be
> exactly `size_t`. Passing the wrong type is undefined behaviour.

> [spec:libedit:def:history.hentry-t]
> typedef struct hentry_t

> [spec:libedit:def:history.hist-event-private]
> typedef struct

> [spec:libedit:def:history.history-def-add-fn]
> static int history_def_add(void *p, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-def-add-fn]
> Built-in "append text to the current event" callback. `p` is the
> `history_t`.
>
> The function takes `evp = (HistEventPrivate *)&h->cursor->ev` up front.
> `HistEventPrivate` is a layout-compatible twin of `TYPE(HistEvent)`
> whose `str` member is non-`const`; the cast exists only to get a mutable
> handle on the entry's string. Forming the pointer before the cursor is
> validated is harmless because it is not dereferenced on the early-exit
> path.
>
> 1. If `h->cursor == &h->list` (no current event), delegate entirely:
>    return `history_def_enter(p, ev, str)`
>    ([spec:libedit:sem:history.history-def-enter-fn]). That means
>    `H_ADD` on a fresh or invalidated history creates a new event and
>    returns **1**, or 0 if `H_UNIQUE` suppressed it, not 0-on-success.
> 2. `elen = Strlen(evp->str)`, `slen = Strlen(str)`, `len = elen + slen +
>    1` (counts in `Char`s, not bytes).
> 3. `s = malloc(len * sizeof(Char))`. On failure set `_HE_MALLOC_FAILED`
>    (2) and return -1; the existing entry is left completely unchanged.
> 4. `memcpy` `elen` characters of the old text into `s`, then `slen`
>    characters of `str` at offset `elen`, then store the terminator at
>    `s[len - 1]`. (The old string's own terminator is deliberately not
>    copied.)
> 5. `free` the old `evp->str` and store `s` in its place. Ownership of
>    the new buffer belongs to the history.
> 6. `*ev = h->cursor->ev` — because `evp` aliases that same event, `ev`
>    receives the entry's id and the **new** string pointer, borrowed, not
>    copied.
> 7. Return 0.
>
> The entry does not move in the list and the cursor does not move.
> Eviction is not re-run, so appending can never drop an entry. Any
> `HistEvent` the caller captured before this call now holds a freed
> pointer.

> [spec:libedit:def:history.history-def-clear-fn]
> static void history_def_clear(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-clear-fn]
> Built-in "clear the history" callback, reached through `H_CLEAR` and
> from `FUN(history,end)` and `history_set_fun`. Returns `void`; it can
> never fail and `H_CLEAR` therefore always reports 0.
>
> 1. `while (h->list.prev != &h->list)` call
>    [spec:libedit:sem:history.history-def-delete-fn] on `h->list.prev` —
>    repeatedly remove the tail (oldest) entry, freeing its string and its
>    node. `ev` is threaded through but never written by either function.
> 2. `h->cursor = &h->list` (invalid).
> 3. `h->eventid = 0` — the id counter restarts, so events entered after a
>    clear reuse numbers 1, 2, 3… that earlier events had.
> 4. `h->cur = 0`.
>
> `h->max` and `h->flags` (`H_UNIQUE`) are deliberately **not** reset, and
> the `history_t` itself is not freed. Per-entry `data` pointers are not
> freed. Every `ev->str` the caller still holds from a previous call is
> dangling afterwards.

> [spec:libedit:def:history.history-def-curr-fn]
> static int history_def_curr(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-curr-fn]
> Built-in "read the current event" callback. Never moves the cursor.
>
> 1. If `h->cursor != &h->list`, copy `*ev = h->cursor->ev` — id plus a
>    borrowed pointer to the entry's own string — and return 0.
> 2. Otherwise set `_HE_CURR_INVALID` (8, `"current event is invalid"`)
>    when `h->cur > 0`, or `_HE_EMPTY_LIST` (5, `"empty list"`) when the
>    list is genuinely empty, and return -1.
>
> This is the only way to read the event a successful `H_SET` positioned
> on, because `history_def_set` does not fill `*ev`.

> [spec:libedit:def:history.history-def-del-fn]
> static int history_def_del(void *p, TYPE(HistEvent) *ev __attribute__((__unused__)), const int num)

> [spec:libedit:sem:history.history-def-del-fn]
> Built-in "delete event number `num`" callback, reached through `H_DEL`.
> The `ev` parameter is annotated `__attribute__((__unused__))` but the
> body does use it; the annotation is simply wrong.
>
> 1. Call [spec:libedit:sem:history.history-def-set-fn] with `num` to
>    position the cursor by event id. If it returns non-zero, return -1;
>    `ev` carries `_HE_EMPTY_LIST` (5) or `_HE_NOT_FOUND` (9), and after a
>    failed search the cursor has been left on the sentinel.
> 2. `ev->str = Strdup(h->cursor->ev.str)` — a caller-owned copy of the
>    deleted text. Not `NULL`-checked. The library never frees it; the
>    public API does not document that the caller must, so in practice
>    this leaks.
> 3. `ev->num = h->cursor->ev.num`.
> 4. Call [spec:libedit:sem:history.history-def-delete-fn] on the cursor
>    node.
> 5. Return 0.
>
> The entry's `data` pointer is discarded without being returned or
> freed — use `H_DELDATA` if you need it.

> [spec:libedit:def:history.history-def-delete-fn]
> static void history_def_delete(history_t *h, TYPE(HistEvent) *ev __attribute__((__unused__)), hentry_t *hp)

> [spec:libedit:sem:history.history-def-delete-fn]
> Unlinks and frees one entry. The lowest-level list operation; every
> removal goes through it. `ev` is accepted and never touched.
>
> 1. `evp = (HistEventPrivate *)&hp->ev`, the non-`const` alias used to
>    free the string.
> 2. If `hp == &h->list`, call `abort()`. Deleting the sentinel is treated
>    as a programming error and terminates the process; it is not an error
>    return.
> 3. Cursor repair, done **before** the unlink: if `h->cursor == hp`, set
>    `h->cursor = hp->prev`; if that landed on the sentinel, set
>    `h->cursor = hp->next`. So deleting the newest entry moves the cursor
>    to the second-newest, and deleting the sole entry leaves the cursor
>    on the sentinel (both `prev` and `next` are the sentinel). Deleting
>    any node the cursor is not on leaves the cursor alone.
> 4. Unlink with two writes: `hp->prev->next = hp->next` and
>    `hp->next->prev = hp->prev`. The list is circular through the
>    sentinel, so no special-casing of the ends is needed.
> 5. `free(evp->str)` then `free(hp)`.
> 6. `h->cur--`.
>
> `hp->data` is deliberately **not** freed — that pointer is the caller's
> property and is simply dropped. `h->eventid` is not adjusted, so ids of
> deleted events are never reused.

> [spec:libedit:def:history.history-def-enter-fn]
> static int history_def_enter(void *p, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-def-enter-fn]
> Built-in "enter a new event" callback, reached through `H_ENTER` and
> through `H_ADD` when there is no current event. Returns **1** on a
> successful insert (not 0), 0 when deduplication suppressed the insert,
> -1 on allocation failure.
>
> 1. **Deduplication.** If `(h->flags & H_UNIQUE) != 0` *and* the list is
>    non-empty (`h->list.next != &h->list`) *and*
>    `Strcmp(h->list.next->ev.str, str) == 0`, return 0 immediately: no
>    insert, no cursor movement, and `*ev` is **not** written — the caller
>    sees the dispatcher's `0`/`"OK"`. The comparison is full string
>    equality against the single most recent entry only; it is not a
>    whole-list uniqueness check, so `a b a` stores all three.
> 2. Call [spec:libedit:sem:history.history-def-insert-fn]. If it returns
>    -1, return -1 immediately, preserving the `_HE_MALLOC_FAILED` message
>    it set.
> 3. **Eviction.** `while (h->cur > h->max && h->cur > 0)` delete
>    `h->list.prev` via
>    [spec:libedit:sem:history.history-def-delete-fn]. Entries are always
>    dropped from the **tail**, i.e. the oldest first, until `cur <= max`.
>    `ev` is passed to the delete but ignored by it.
> 4. Return 1.
>
> The source comment above the eviction loop claims it "always keeps at
> least one entry"; the condition as written does not do that. With
> `max == 0` — which is the state of every history until `H_SETSIZE` is
> issued, see [spec:libedit:sem:history.fun-history-init-fn] — the entry
> just inserted is itself the tail and is deleted, leaving the list empty.
> Because step 2 already wrote `*ev` pointing at that entry's string, the
> caller is handed a **dangling `ev->str`** and a stale `ev->num` for an
> event that no longer exists. This is a genuine use-after-free exposed
> across the API; a port must decide what to do and record the divergence
> rather than assume the comment.
>
> Also note the interaction with the dispatcher: `H_ENTER` sets
> `h->h_ent = ev->num` whenever the return is `!= -1`, so a
> dedup-suppressed enter (return 0, `ev` untouched) sets `h_ent = 0`,
> which matches no event, breaking a following `H_APPEND` with
> `_HE_NOT_FOUND`.

> [spec:libedit:def:history.history-def-first-fn]
> static int history_def_first(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-first-fn]
> Built-in "go to the first event" callback. `p` is the `history_t`
> backing store.
>
> 1. Set `h->cursor = h->list.next` — unconditionally, before any
>    emptiness test. On an empty list this parks the cursor on the
>    sentinel.
> 2. If the new cursor is not the sentinel `&h->list`, copy the whole
>    event by value: `*ev = h->cursor->ev`, i.e. `ev->num` = that entry's
>    id and `ev->str` = a **borrowed** pointer to the entry's own string
>    buffer, owned by the history and valid only until that entry is
>    deleted, replaced or the history is cleared. Return 0.
> 3. Otherwise set `_HE_FIRST_NOTFOUND` (3, `"first event not found"`) on
>    `ev` and return -1.
>
> Direction matters: insertion happens at the head of the list
> ([spec:libedit:sem:history.history-def-insert-fn]), so "first" is the
> **most recently entered** event and "last" is the oldest. `H_NEXT` walks
> from here toward older entries.

> [spec:libedit:def:history.history-def-init-fn]
> static int history_def_init(void **p, TYPE(HistEvent) *ev __attribute__((__unused__)), int n)

> [spec:libedit:sem:history.history-def-init-fn]
> Allocates and initialises the built-in `history_t` backing store,
> storing it through `*p`. `ev` is declared unused and genuinely is
> unused — notably, **the allocation-failure path sets no error event**.
>
> 1. `h = malloc(sizeof(history_t))`. If `NULL`, return -1 leaving `*p`
>    untouched and `*ev` untouched.
> 2. If `n <= 0`, clamp `n` to 0. Negative maxima are impossible from here
>    on.
> 3. `h->eventid = 0` (so the first entry gets id 1), `h->cur = 0`,
>    `h->max = n`.
> 4. `h->list.next = h->list.prev = &h->list` — the empty circular list.
>    The `list` member is an embedded `hentry_t` used as a sentinel, not a
>    real entry; it is never allocated separately and must never be freed.
> 5. `h->list.ev.str = NULL`, `h->list.ev.num = 0`. `h->list.data` is left
>    **uninitialised**; it is never read, because
>    [spec:libedit:sem:history.history-def-delete-fn] aborts rather than
>    process the sentinel.
> 6. `h->cursor = &h->list` — the cursor starts invalid.
> 7. `h->flags = 0`, i.e. `H_UNIQUE` off.
> 8. `*p = h`, return 0.

> [spec:libedit:def:history.history-def-insert-fn]
> static int history_def_insert(history_t *h, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-def-insert-fn]
> Creates a new entry holding a copy of `str` and links it at the head of
> the list. Does not enforce the size limit — that is the caller's job.
>
> 1. `c = malloc(sizeof(hentry_t))`. On failure set `_HE_MALLOC_FAILED`
>    (2) on `ev` and return -1.
> 2. `c->ev.str = Strdup(str)` — the history takes ownership of this copy;
>    the caller's `str` is not retained. On failure `free(c)`, set
>    `_HE_MALLOC_FAILED` and return -1. `str` itself is not checked for
>    `NULL`; passing `NULL` is undefined behaviour.
> 3. `c->data = NULL`.
> 4. `c->ev.num = ++h->eventid` — ids start at **1**, increase strictly
>    monotonically, and are never reused. Only
>    [spec:libedit:sem:history.history-def-clear-fn] resets the counter.
>    No event ever has id 0, which is why the `_HE_OK` prologue's
>    `ev->num = 0` can be used as an "invalid id" sentinel.
> 5. Link at the head, exactly four pointer writes in this order:
>    `c->next = h->list.next`, `c->prev = &h->list`,
>    `h->list.next->prev = c`, `h->list.next = c`. This works unmodified
>    on an empty list because `h->list.next` is then the sentinel itself.
> 6. `h->cur++`.
> 7. `h->cursor = c` — insertion always repositions the cursor onto the
>    new entry.
> 8. `*ev = c->ev`: `ev->num` is the new id and `ev->str` is a **borrowed**
>    pointer to the entry's own buffer, valid only until that entry is
>    deleted or replaced.
> 9. Return 0.

> [spec:libedit:def:history.history-def-last-fn]
> static int history_def_last(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-last-fn]
> Built-in "go to the last event" callback, the mirror of
> [spec:libedit:sem:history.history-def-first-fn].
>
> 1. Set `h->cursor = h->list.prev` unconditionally.
> 2. If that is not the sentinel `&h->list`, `*ev = h->cursor->ev` (id
>    plus a borrowed pointer to the entry's string) and return 0.
> 3. Otherwise set `_HE_LAST_NOTFOUND` (4, `"last event not found"`) and
>    return -1, leaving the cursor parked on the sentinel.
>
> "Last" is the **oldest** event, because new entries go in at the head.

> [spec:libedit:def:history.history-def-next-fn]
> static int history_def_next(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-next-fn]
> Built-in "advance to the next event" callback. "Next" means **toward
> older** entries, since insertion is at the head.
>
> 1. If `h->cursor == &h->list` (the cursor is invalid or the list is
>    empty), set `_HE_EMPTY_LIST` (5, `"empty list"`) and return -1
>    without moving. Note the message is used even when the list is not
>    empty — a cursor left on the sentinel by a failed `H_SET`,
>    `history_set_nth` or a delete-of-the-only-entry produces `"empty
>    list"` here.
> 2. If `h->cursor->next == &h->list` (already on the oldest entry), set
>    `_HE_END_REACHED` (6, `"no next event"`) and return -1. The cursor is
>    **not** moved, so it stays on the oldest entry and a following
>    `H_PREV` walks back correctly.
> 3. Otherwise advance `h->cursor = h->cursor->next`, copy `*ev =
>    h->cursor->ev` (borrowed string), and return 0.

> [spec:libedit:def:history.history-def-prev-fn]
> static int history_def_prev(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-prev-fn]
> Built-in "step to the previous event" callback. "Previous" means
> **toward newer** entries, since insertion is at the head.
>
> 1. If `h->cursor == &h->list`, set `_HE_END_REACHED` (6, `"no next
>    event"`) when `h->cur > 0`, otherwise `_HE_EMPTY_LIST` (5), and
>    return -1 without moving. The `_HE_END_REACHED` choice is a wart —
>    the message says "no next event" in the *previous* function — but it
>    is observable through `ev->str` and must be reproduced.
> 2. If `h->cursor->prev == &h->list` (already on the newest entry), set
>    `_HE_START_REACHED` (7, `"no previous event"`) and return -1 without
>    moving.
> 3. Otherwise `h->cursor = h->cursor->prev`, `*ev = h->cursor->ev`
>    (borrowed string), return 0.

> [spec:libedit:def:history.history-def-set-fn]
> static int history_def_set(void *p, TYPE(HistEvent) *ev, const int n)

> [spec:libedit:sem:history.history-def-set-fn]
> Built-in "make event number `n` current" callback. Positions the cursor
> by matching an event **id**, not an index.
>
> 1. If `h->cur == 0`, set `_HE_EMPTY_LIST` (5) and return -1 with the
>    cursor untouched.
> 2. Fast path: if the cursor is already on a real entry whose
>    `ev.num == n`, skip the search entirely.
> 3. Otherwise scan: start at `h->list.next` (the newest entry) and walk
>    forward through `->next` (toward older) assigning each node to
>    `h->cursor` as you go, stopping at the first node whose `ev.num == n`
>    or at the sentinel. The scan writes `h->cursor` on every iteration,
>    so a failed search leaves the cursor **on the sentinel** — a failed
>    `H_SET` invalidates the position.
> 4. If the scan ended on the sentinel, set `_HE_NOT_FOUND` (9, `"event
>    not found"`) and return -1.
> 5. Otherwise return 0.
>
> Critically, **`*ev` is never written on success**. After a successful
> `H_SET` the caller's `HistEvent` still holds whatever the dispatcher's
> `_HE_OK` prologue put there (`num = 0`, `str = "OK"`). The caller must
> issue `H_CURR` to read the event it just selected. This also means
> `H_APPEND` — which does `H_SET` then `h_add` — relies on `h_add` to fill
> `ev`.

> [spec:libedit:def:history.history-deldata-nth-fn]
> static int history_deldata_nth(history_t *h, TYPE(HistEvent) *ev, int num, void **data)

> [spec:libedit:sem:history.history-deldata-nth-fn]
> Deletes the `n`-th entry counted from the oldest end and hands the
> caller both a copy of its text and its attached `data` pointer. Backs
> `H_DELDATA`.
>
> 1. Call [spec:libedit:sem:history.history-set-nth-fn] with `num`. If it
>    returns non-zero, return -1 immediately; `ev` carries
>    `_HE_EMPTY_LIST` or `_HE_NOT_FOUND` and the cursor may have been left
>    on the sentinel.
> 2. **Magic value:** if `data == (void **)-1`, return 0 right here
>    without deleting anything and without writing `*ev`. The call then
>    means only "move the cursor to the `n`-th entry from the oldest end".
>    `src/readline.c` relies on this to implement positional lookup.
> 3. `ev->str = Strdup(h->cursor->ev.str)` — a freshly allocated copy
>    whose ownership passes to the caller. The result is **not**
>    `NULL`-checked; an allocation failure here stores `NULL` into
>    `ev->str` and the deletion proceeds. The library never frees this
>    copy, so unless the caller frees it, it leaks.
> 4. `ev->num = h->cursor->ev.num`.
> 5. If `data` is non-`NULL` (and not the magic value), `*data =
>    h->cursor->data` — the `void *` previously attached via `H_REPLACE`.
>    Ownership of whatever it points at passes to the caller; the library
>    never frees it.
> 6. Call [spec:libedit:sem:history.history-def-delete-fn] on the cursor
>    node: unlink, free the entry's string, free the node, decrement
>    `cur`, and move the cursor to the deleted node's `prev` (or its
>    `next` when `prev` is the sentinel).
> 7. Return 0.

> [spec:libedit:def:history.history-efun-t-void-type-hist-event-const-char]
> typedef int (*history_efun_t)(void *, TYPE(HistEvent) *, const Char *)

> [spec:libedit:def:history.history-getsize-fn]
> static int history_getsize(TYPE(History) *h, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-getsize-fn]
> Backs `H_GETSIZE`.
>
> 1. If `h->h_next != history_def_next`, set `_HE_NOT_ALLOWED` (14) and
>    return -1.
> 2. `ev->num = ((history_t *)h->h_ref)->cur` — the **current number of
>    stored events**, *not* the maximum configured by `H_SETSIZE`. Despite
>    the symmetry of the opcode names, `H_GETSIZE` does not read back what
>    `H_SETSIZE` wrote, and there is no way to query the maximum.
> 3. If `ev->num < -1`, set `_HE_SIZE_NEGATIVE` (13, `"history size
>    negative"`) and return -1. `cur` is never negative, so this branch is
>    unreachable; a port may keep it as dead code or drop it.
> 4. Return 0. `ev->str` is left at the prologue's `"OK"`.

> [spec:libedit:def:history.history-getunique-fn]
> static int history_getunique(TYPE(History) *h, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-getunique-fn]
> Backs `H_GETUNIQUE`.
>
> 1. If `h->h_next != history_def_next`, set `_HE_NOT_ALLOWED` (14) and
>    return -1.
> 2. `ev->num = ((((history_t *)h->h_ref)->flags) & H_UNIQUE) != 0`, i.e.
>    normalised to exactly 1 or 0, not the raw flag word.
> 3. Return 0. `ev->str` keeps the prologue's `"OK"`.

> [spec:libedit:def:history.history-gfun-t-void-type-hist-event]
> typedef int (*history_gfun_t)(void *, TYPE(HistEvent) *)

> [spec:libedit:def:history.history-load-fn]
> static int history_load(TYPE(History) *h, const char *fname)

> [spec:libedit:sem:history.history-load-fn]
> Reads a history file and appends its entries to `h`. Backs `H_LOAD`.
> This function and
> [spec:libedit:sem:history.history-save-fp-fn] together define the
> on-disk format, which the no-c-ffi decision freezes: a port must read
> and write files that the C library reads and writes, byte for byte.
>
> ### On-disk grammar
>
> ```
> file    := cookie entry*
> cookie  := "_HiStOrY_V2_" LF          ; exactly 13 bytes
> entry   := vis-encoded-text LF        ; LF on the last entry is optional on read
> ```
>
> - The cookie is the 13 bytes `5f 48 69 53 74 4f 72 59 5f 56 32 5f 0a`,
>   i.e. `_HiStOrY_V2_` followed by a single `LF`. It is the only header;
>   there is no length field, no entry count, no byte-order mark and no
>   trailing footer.
> - Every following line is exactly one history entry. Lines are
>   terminated by a single `LF` (`0x0A`). `CR` is not a terminator and is
>   not special in any way.
> - The entry text is the raw entry bytes passed through
>   `strvis(dst, src, VIS_WHITE)` — see
>   [spec:libedit:sem:vis.strvis-fn]. The effect of `VIS_WHITE`
>   (`VIS_SP|VIS_TAB|VIS_NL`) with no other flags is that the escape set
>   is `{' ', '\t', '\n', '\\'}`: printable/graphic characters are
>   written literally, and space, tab, newline and backslash are written
>   as three-digit octal escapes `\040`, `\011`, `\012`, `\134`. Any
>   other byte whose low seven bits are `0x20` (so `0xA0`) also takes the
>   octal form, `\240`. Every remaining non-graphic byte comes out in the
>   meta/control forms: `\^A` for `0x01`, `\^?` for `0x7f`, `\M-i` for
>   `0xE9`, `\M^@` for `0x80`. No C-style `\n`/`\t` escapes are
>   ever produced, because `VIS_CSTYLE` is not set. The guarantee that
>   matters structurally: **an encoded entry can never contain a literal
>   `LF`, so one line is always exactly one entry.**
> - **There is no timestamp, no per-entry count, no event id and no flag
>   byte.** Nothing but the text is persisted. Event numbers are
>   regenerated by the insertion counter on load, and the per-entry
>   `data` pointer is not saved at all. A port must not add fields; a file
>   with extra columns is not this format.
> - Entries appear oldest first, newest last.
> - The encoding is **locale-sensitive on write**: `strvis` classifies
>   through `iswgraph`/`mbrtowc`, so in a UTF-8 locale non-ASCII text is
>   written as raw UTF-8 bytes, while the same text written under `LC_ALL=C`
>   comes out as `\M-` escapes. Both decode back to the same bytes; the
>   wide build then converts those bytes with `mbstowcs`, which *is*
>   locale-sensitive on read.
>
> ### Procedure
>
> 1. `fp = fopen(fname, "r")`. On failure return -1 at once (`i` is
>    pre-initialised to -1). The dispatcher turns -1 into
>    `_HE_HIST_READ`.
> 2. Read the first line with `getline` into a heap buffer. If it returns
>    -1 (empty file, or read error), return -1.
> 3. Cookie check: `strncmp(line, hist_cookie, (size_t)sz) != 0` → return
>    -1. `sz` is the byte count `getline` returned, i.e. the line
>    *including* its `LF` but excluding the terminating NUL. **The
>    comparison length is the length of the line read, not the length of
>    the cookie**, so this is lenient in one direction: any first line
>    that is a proper prefix of `_HiStOrY_V2_\n` is accepted — a file
>    whose entire content is `_HiS` (no newline, `sz == 4`) passes the
>    check and then loads zero entries. A first line longer than the
>    cookie can never match. A port must reproduce the prefix leniency;
>    real files do not exercise it, but truncated ones do.
> 4. Allocate a scratch decode buffer `ptr` of 1024 bytes. On failure
>    return -1. **It is never zeroed and is reused for every line** — see
>    the malformed-line rule below.
> 5. For each subsequent `getline`, with `i` starting at 0 and
>    incrementing once per line read (including lines that end up
>    skipped):
>    a. If `sz > 0` and the final byte is `LF`, drop it: `line[--sz] =
>       '\0'`. A final line with no `LF` is processed unchanged, so a
>       truncated file still contributes its last partial entry.
>    b. If `max_size <= (size_t)sz`, grow `ptr` to `((size_t)sz + 1024) &
>       ~(size_t)1023` bytes. `realloc` failure sets `i = -1` and stops
>       the loop. The grown size is always at least `sz + 1`, which is
>       sufficient because unvis output is never longer than its input.
>    c. `strunvis(ptr, line)` — decode in place into `ptr`; see
>       [spec:libedit:sem:unvis.strunvis-fn]. **The return value is
>       discarded.**
>    d. `decode_result = ct_decode_string(ptr, &conv)`. In the wide build
>       this is `mbstowcs` into a **function-`static`** `ct_buffer_t`
>       (hence not thread-safe, and the returned buffer is reused on every
>       call); it returns `NULL` when `ptr` is not a valid multibyte
>       string in the current locale, and the line is then silently
>       skipped by `continue` — but the `for`-loop increment still runs,
>       so it is still counted in `i`. In the narrow build
>       `ct_decode_string` is the identity macro and never fails.
>    e. `HENTER(h, &ev, decode_result)`: enter the decoded string as a new
>       newest event. The history takes its own copy. A return of -1 sets
>       `i = -1` and stops the loop. A successful enter runs the normal
>       size-limit eviction, so **loading a file with more lines than the
>       configured maximum keeps only the last `max` lines** — the newest
>       ones, since entries are read top to bottom.
> 6. Free `ptr`, free the `getline` buffer, `fclose(fp)`, return `i`.
>
> ### Return value
>
> -1 for: `fopen` failure, empty file, cookie mismatch, either allocation
> failure, or an `HENTER` failure. Otherwise the **number of data lines
> read** — which is not necessarily the number of entries now stored,
> because skipped lines are counted and evicted entries are not
> subtracted. A file containing only the cookie returns 0.
>
> ### Leniency and malformed input — must be reproduced
>
> - The existing history is **not cleared first**. Entries are appended to
>   whatever `h` already holds, and `h->h_ent` is left unchanged.
> - **Empty lines are not skipped.** After the `LF` is stripped the line
>   is `""`, which decodes to `""` and is entered as an empty history
>   event (unless `H_UNIQUE` suppresses a run of them).
> - A line containing an embedded NUL byte is silently truncated at that
>   NUL: `getline` keeps the byte, `strunvis` stops there, and the rest of
>   the line is lost with no diagnostic.
> - A second `_HiStOrY_V2_` line anywhere but line 1 is ordinary entry
>   text, not a header. Concatenating two history files therefore yields a
>   file with a stray entry, not an error.
> - An escape sequence cut short by end of line (a trailing `\`, a
>   partial `\12`) is **not** an error: `strunvis` flushes it and NUL
>   terminates normally.
> - **A genuinely invalid escape is a hazard, not an error, and the result
>   is undefined.** `strunvis` returns -1 on the first bad sequence and
>   returns *without writing the terminating NUL*. The return value is
>   ignored, so `ptr` is then read as a C string containing: the bytes
>   decoded from this line so far, followed by whatever was left in the
>   reused buffer from the previous line — or, on the first malformed
>   line, uninitialised heap — up to the first NUL byte that happens to be
>   present. If no NUL is present within the allocation the read runs off
>   the end of the buffer. There is no defined behaviour here to port. A
>   Rust port must (a) pick a policy — keeping the successfully decoded
>   prefix is the least surprising — and record the divergence, and (b)
>   **keep going**: the C does not abort the load, it enters the garbage
>   string and continues with the next line. A port that treats a
>   malformed line as fatal will refuse files the C accepts.
>   Sequences that trigger this include `\` followed by a non-graphic
>   character that is not a recognised escape (e.g. `\` + space), `\M`
>   not followed by `-` or `^`, `\x` not followed by a hex digit, and an
>   octal escape that overflows 8 bits such as `\400`.

> [spec:libedit:def:history.history-next-evdata-fn]
> static int history_next_evdata(TYPE(History) *h, TYPE(HistEvent) *ev, int num, void **d)

> [spec:libedit:sem:history.history-next-evdata-fn]
> Identical to [spec:libedit:sem:history.history-prev-event-fn] except
> that it also hands back the found event's attached `data` pointer. Backs
> `H_NEXT_EVDATA`.
>
> 1. `retval = HCURR(h, ev)`, then while `retval != -1`: if `ev->num ==
>    num`, then — if `d` is non-`NULL` — store `*d = ((history_t
>    *)h->h_ref)->cursor->data`, and return 0. Otherwise `retval =
>    HPREV(h, ev)` and repeat.
> 2. On exhaustion set `_HE_NOT_FOUND` (9) and return -1; `*d` is left
>    untouched.
>
> Two things to carry over. Despite the name it does **not** advance to a
> "next" event: `HPREV` walks from the current position toward *newer*
> entries, exactly as `history_prev_event` does. And it casts `h->h_ref`
> to `history_t *` unconditionally, so with a caller-supplied function set
> this reads through a type-confused pointer — undefined behaviour, and
> the reason `H_NEXT_EVDATA` is only meaningful on the built-in backend.
> The `data` pointer is borrowed, not copied; ownership stays with
> whoever set it via `H_REPLACE`.

> [spec:libedit:def:history.history-next-event-fn]
> static int history_next_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)

> [spec:libedit:sem:history.history-next-event-fn]
> Finds the event with a given id by scanning from the current position
> **toward older** entries. Backs `H_NEXT_EVENT`.
>
> 1. `retval = HCURR(h, ev)`.
> 2. While `retval != -1`: if `ev->num == num`, return 0 with the cursor
>    on that event and `*ev` filled (borrowed string). Otherwise `retval =
>    HNEXT(h, ev)` — one step toward the tail, i.e. toward older entries —
>    and repeat.
> 3. On exhaustion set `_HE_NOT_FOUND` (9) and return -1.
>
> The cursor is left where the scan stopped (on the oldest entry after an
> exhausted search), not restored.

> [spec:libedit:def:history.history-next-string-fn]
> static int history_next_string(TYPE(History) *h, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-next-string-fn]
> Finds the first event whose text **begins with** `str`, scanning from
> the current position **toward newer** entries. Backs `H_NEXT_STR`.
>
> 1. `len = Strlen(str)`, computed once.
> 2. `retval = HCURR(h, ev)`.
> 3. While `retval != -1`: if `Strncmp(str, ev->str, len) == 0`, return 0
>    with the cursor on that event and `*ev` filled (borrowed string).
>    Otherwise `retval = HPREV(h, ev)` — toward the head, i.e. toward more
>    recently entered events — and repeat.
> 4. On exhaustion set `_HE_NOT_FOUND` (9) and return -1.
>
> Prefix semantics as in
> [spec:libedit:sem:history.history-prev-string-fn]: an empty `str`
> matches the current event immediately, and the scan includes the current
> event. And as noted there, the `HPREV`/`HNEXT` pairing of the two string
> searches is the reverse of the two event-id searches; this is a quirk of
> the C, not a typo in this rule.

> [spec:libedit:def:history.history-prev-event-fn]
> static int history_prev_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)

> [spec:libedit:sem:history.history-prev-event-fn]
> Finds the event with a given id by scanning from the current position
> **toward newer** entries. Backs `H_PREV_EVENT`.
>
> 1. `retval = HCURR(h, ev)` — read the current event. If the cursor is
>    invalid this fails immediately and the loop body never runs.
> 2. While `retval != -1`: if `ev->num == num`, return 0 with the cursor
>    sitting on that event and `*ev` describing it (id plus a borrowed
>    string pointer). Otherwise `retval = HPREV(h, ev)` and repeat.
>    `HPREV` moves toward the head of the list, i.e. toward more recently
>    entered events.
> 3. When the walk runs out (`HPREV` fails at the newest entry, or `HCURR`
>    failed at the start), set `_HE_NOT_FOUND` (9, `"event not found"`)
>    and return -1.
>
> The cursor is left wherever the scan stopped — on the newest entry after
> an exhausted search, or unchanged if `HCURR` failed. It is *not* reset,
> so a failed search silently repositions the history. All callbacks are
> reached through the function-pointer table, so this works with a
> caller-supplied backend.

> [spec:libedit:def:history.history-prev-string-fn]
> static int history_prev_string(TYPE(History) *h, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-prev-string-fn]
> Finds the first event whose text **begins with** `str`, scanning from
> the current position **toward older** entries. Backs `H_PREV_STR`.
>
> 1. `len = Strlen(str)` — computed once, in `Char`s.
> 2. `retval = HCURR(h, ev)`.
> 3. While `retval != -1`: if `Strncmp(str, ev->str, len) == 0`, return 0
>    with the cursor on that event and `*ev` filled (borrowed string
>    pointer). Otherwise `retval = HNEXT(h, ev)` and repeat.
> 4. On exhaustion set `_HE_NOT_FOUND` (9) and return -1.
>
> This is a prefix test, not a substring or equality test, and an empty
> `str` (`len == 0`) matches the very first candidate — i.e. the current
> event — immediately. The comparison starts at the current event, so an
> event already selected can match itself.
>
> Note the direction: `history_prev_string` walks with `HNEXT` (toward
> older) while [spec:libedit:sem:history.history-next-string-fn] walks
> with `HPREV` (toward newer) — the exact opposite pairing from
> `history_prev_event`/`history_next_event`, which use `HPREV`/`HNEXT`
> respectively. This inconsistency is real and is observable through
> `H_PREV_STR`/`H_NEXT_STR`; reproduce it rather than "correcting" it.

> [spec:libedit:def:history.history-save-fn+1]
> static int history_save(TYPE(History) *h, const char *fname)

> [spec:libedit:sem:history.history-save-fn+1]
> Writes the entire history to a named file and backs `H_SAVE`. The complete
> replacement MUST be staged before the destination changes; this is the
> C-visible correction authorized by
> [dec:libedit:history-save-failure-reporting].
>
> 1. Create an exclusive **0600** temporary in the destination's directory.
>    If the destination is an existing regular file, copy its permissions to
>    the temporary. A creation, metadata, or permission failure returns -1
>    and the dispatcher reports `_HE_HIST_WRITE` with the originating errno.
> 2. Write the `_HiStOrY_V2_` cookie and every history entry, oldest first,
>    using the byte grammar in [spec:libedit:sem:history.history-load-fn].
>    Every write MUST be checked.
> 3. Flush and synchronize the temporary. Any failure returns -1, preserves
>    its errno, cleans up the temporary when possible, and leaves the prior
>    destination untouched.
> 4. Atomically replace the destination with the completed temporary. A
>    replacement failure likewise returns -1 and leaves the prior destination
>    untouched.
> 5. Return the number of entries written only after replacement succeeds.
>
> Replacement acts on the named directory entry: it changes inode identity,
> breaks that name's previous hard-link relationship, replaces a symlink
> rather than truncating its referent, and requires write permission on the
> containing directory. These effects are intentional consequences of making
> partial named saves impossible.

> [spec:libedit:def:history.history-save-fp-fn+1]
> static int history_save_fp(TYPE(History) *h, size_t nelem, FILE *fp)

> [spec:libedit:sem:history.history-save-fp-fn+1]
> Writes the history to an already-open stream. Backs `H_SAVE_FP`
> (`nelem == (size_t)-1`) and `H_NSAVE_FP`, and supplies the encoding and
> write phase of [spec:libedit:sem:history.history-save-fn]. Returns the
> number of
> entries written, or -1. The on-disk grammar it produces is specified in
> [spec:libedit:sem:history.history-load-fn]; `i` is pre-initialised to
> -1.
>
> 1. **Header.** If `ftell(fp) == 0`, write the cookie with
>    `fputs(hist_cookie, fp)`; if that returns `EOF`, return -1 at once.
>    If `ftell(fp) != 0` the cookie is **not** written — that covers both
>    the intended case (appending to a stream already positioned past a
>    header) and a trap: on a non-seekable stream such as a pipe or socket
>    `ftell` returns -1, so `H_SAVE_FP` to a pipe silently produces a
>    headerless file that `history_load` will later reject outright.
> 2. Allocate a 1024-byte scratch buffer `ptr`; on failure return -1.
> 3. **Positioning.**
>    - If `nelem == (size_t)-1`, skip straight to the fallback: `retval =
>      HLAST(h, &ev)`, i.e. start at the **oldest** entry.
>    - Otherwise walk forward from the newest:
>      `for (retval = HFIRST(h,&ev); retval != -1 && nelem-- > 0; retval =
>      HNEXT(h,&ev)) continue;`. If the list runs out first, `retval ==
>      -1` and the code falls back to `HLAST`, i.e. saves everything.
>    - **Off-by-one, and it is observable.** The `nelem--` is a
>      post-decrement in the loop condition, so the walk stops *on* the
>      entry at index `nelem` counted from the newest, and the write loop
>      below then emits that entry **plus every newer one** — `nelem + 1`
>      entries. `H_NSAVE_FP` with `n` therefore writes
>      `min(n + 1, size)` entries, not `n`. `n == 0` writes one entry (the
>      newest), not zero. On an empty history `HFIRST` fails immediately,
>      the `HLAST` fallback also fails, and 0 is returned.
> 4. **Write loop.** Starting at the current position and stepping with
>    `HPREV` (toward newer) until it fails, with `i` counting iterations:
>    - `str = ct_encode_string(ev.str, &conv)`. Wide build: convert the
>      `wchar_t` string to the locale's multibyte encoding through a
>      **function-`static`** `ct_buffer_t`, so this is not thread-safe and
>      the buffer is reused each iteration. Narrow build: the macro is the
>      identity and `str` is `ev.str` itself. A `NULL` return —
>      allocation failure, or a `NULL` `ev.str` from a caller-supplied
>      function set — is **not checked**, and `strlen(NULL)` follows:
>      undefined behaviour.
>    - `len = strlen(str) * 4 + 1`. Four is the worst-case `vis` expansion
>      per input byte (`\ddd`, `\M-x`, `\M^X`), so this bound is exact
>      and never short. If `len > max_size`, grow `ptr` to `(len + 1024) &
>      ~(size_t)1023`; a `realloc` failure sets `i = -1` and stops the
>      loop.
>    - `strvis(ptr, str, VIS_WHITE)` — encode; see
>      [spec:libedit:sem:vis.strvis-fn].
>    - Write the encoded line and newline to `fp`. The result MUST be checked;
>      `ENOSPC`, `EIO`, a full pipe, or any short write returns -1 with
>      `_HE_HIST_WRITE` and the originating errno.
> 5. Flush `fp`. A flush failure returns -1 with `_HE_HIST_WRITE` and its
>    errno; success is not reported while output remains only in an unchecked
>    stdio buffer.
> 6. Free `ptr` and return `i`: the number of entries written, 0 for an
>    empty history, or -1 for any header, allocation, write, or flush failure.
>
> The stream is flushed but never closed — the caller owns it. Because
> the walk runs from oldest toward newest, the resulting file is
> **oldest first, newest last**, which is what makes `history_load`'s
> top-to-bottom `H_ENTER` restore the original ordering.

> [spec:libedit:def:history.history-set-fun-fn]
> static int history_set_fun(TYPE(History) *h, TYPE(History) *nh)

> [spec:libedit:sem:history.history-set-fun-fn]
> Installs a caller-supplied history implementation. Reached only from the
> `H_FUNC` opcode, which builds `nh` on the stack from eleven varargs.
> Returns 0 on success, -1 on rejection.
>
> 1. **Validation.** If *any* of `nh->h_first`, `h_next`, `h_last`,
>    `h_prev`, `h_curr`, `h_set`, `h_enter`, `h_add`, `h_clear`, `h_del`
>    or `h_ref` is `NULL`, the request is rejected — and, as a side
>    effect, `h` is forced back onto the built-in implementation if it was
>    not already on it:
>    - If `h->h_next != history_def_next`, call
>      [spec:libedit:sem:history.history-def-init-fn] with `&h->h_ref` to
>      allocate a **fresh, empty** `history_t` with `max = 0`. Whatever
>      `h_ref` previously pointed at is overwritten without being freed.
>      If that allocation fails, return -1 with `h` completely unchanged.
>      Otherwise install the ten built-in callbacks (`history_def_first`,
>      `_next`, `_last`, `_prev`, `_curr`, `_set`, `_clear`, `_enter`,
>      `_add`, `_del`).
>    - If `h` was already on the built-in set, nothing is changed.
>    - Return -1 either way. `h_ent` is *not* reset on this path. The
>      dispatcher turns the -1 into `_HE_PARAM_MISSING` (12).
> 2. **Acceptance.** All eleven are non-`NULL`. If `h->h_next ==
>    history_def_next`, call
>    [spec:libedit:sem:history.history-def-clear-fn] on the current
>    `h_ref`, deleting and freeing every stored entry. The `history_t`
>    struct itself is **not** freed, so it leaks.
> 3. `h->h_ent = -1`.
> 4. Copy exactly ten fields from `nh` into `h`: `h_first`, `h_next`,
>    `h_last`, `h_prev`, `h_curr`, `h_set`, `h_clear`, `h_enter`, `h_add`,
>    `h_del`.
> 5. Return 0.
>
> **The critical defect: `h->h_ref` is never assigned from `nh->h_ref`.**
> It is read once, in step 1, only to test it against `NULL`. Every
> callback invocation in this file goes through macros of the form
> `(*(h)->h_next)((h)->h_ref, ev)`, so after a successful `H_FUNC` the
> caller's ten functions are invoked with the *old* reference — the
> built-in `history_t` that step 2 just emptied — instead of the object
> the caller supplied. `H_FUNC` as shipped therefore cannot work for any
> non-trivial custom backend. Nothing in the C tree uses `H_FUNC`, so the
> defect is dormant in practice. A port must make a deliberate choice
> here: reproducing the C exactly preserves a broken feature, while
> assigning `h_ref` changes observable behaviour *and* makes
> [spec:libedit:sem:history.fun-history-end-fn]'s unconditional
> `free(h->h_ref)` start freeing caller-owned memory. Record whichever is
> chosen; do not silently "fix" it.
>
> After a custom set is installed, several operations become unsafe or
> refused: `history_setsize`, `history_getsize`, `history_setunique` and
> `history_getunique` all fail with `_HE_NOT_ALLOWED`; `H_NEXT_EVDATA`,
> `H_DELDATA` and `H_REPLACE` still cast `h_ref` to `history_t *`
> regardless (undefined behaviour); and `FUN(history,end)` still calls
> `free(h->h_ref)`.

> [spec:libedit:def:history.history-set-nth-fn]
> static int history_set_nth(void *p, TYPE(HistEvent) *ev, int n)

> [spec:libedit:sem:history.history-set-nth-fn]
> Positions the cursor on the `n`-th entry counted from the **oldest**
> end. This is index addressing, unlike
> [spec:libedit:sem:history.history-def-set-fn] which matches by event id.
> It is not part of the callback table; it exists for
> `history_deldata_nth` and hence for `H_DELDATA`.
>
> 1. If `h->cur == 0`, set `_HE_EMPTY_LIST` (5) and return -1.
> 2. Walk: `h->cursor = h->list.prev` (the oldest entry), and while the
>    cursor is not the sentinel, evaluate `if (n-- <= 0) break;` then step
>    `h->cursor = h->cursor->prev` (toward newer). So the loop stops on
>    the entry `n` positions from the oldest end: `n == 0` selects the
>    oldest, `n == 1` the second-oldest, and so on. Any negative `n`
>    behaves exactly like 0 and selects the oldest entry.
> 3. If the walk fell off the end onto the sentinel, set `_HE_NOT_FOUND`
>    (9) and return -1 — with the cursor left on the sentinel, i.e.
>    invalidated.
> 4. Otherwise return 0. `*ev` is **not** written on success.

> [spec:libedit:def:history.history-setsize-fn]
> static int history_setsize(TYPE(History) *h, TYPE(HistEvent) *ev, int num)

> [spec:libedit:sem:history.history-setsize-fn]
> Backs `H_SETSIZE`. Sets the maximum number of retained events.
>
> 1. If `h->h_next != history_def_next` — a caller-supplied function set
>    is installed — set `_HE_NOT_ALLOWED` (14, `"function not allowed with
>    other history-functions-set the default"`) and return -1.
> 2. If `num < 0`, set `_HE_BAD_PARAM` (15, `"bad parameters"`) and return
>    -1.
> 3. Otherwise assign `((history_t *)h->h_ref)->max = num` and return 0.
>    `*ev` is not written, so the caller still sees `0`/`"OK"`.
>
> Two behaviours a port must keep. **No immediate eviction**: shrinking
> the size does not delete anything now; the list is trimmed only on the
> next [spec:libedit:sem:history.history-def-enter-fn], so a history can
> sit above its configured maximum indefinitely if nothing is entered.
> And `num == 0` is legal and means "retain nothing" — every subsequent
> enter inserts and immediately evicts.

> [spec:libedit:def:history.history-setunique-fn]
> static int history_setunique(TYPE(History) *h, TYPE(HistEvent) *ev, int uni)

> [spec:libedit:sem:history.history-setunique-fn]
> Backs `H_SETUNIQUE`. Controls whether an entry identical to the most
> recent one is suppressed.
>
> 1. If `h->h_next != history_def_next`, set `_HE_NOT_ALLOWED` (14) and
>    return -1.
> 2. If `uni` is non-zero, set the `H_UNIQUE` bit (value 1) in
>    `((history_t *)h->h_ref)->flags`; if `uni` is zero, clear it.
> 3. Return 0 without writing `*ev`.
>
> Turning the flag on removes nothing that is already stored; it only
> affects future calls to
> [spec:libedit:sem:history.history-def-enter-fn], and even then only
> compares against the single newest entry.

> [spec:libedit:def:history.history-sfun-t-void-type-hist-event-const-int]
> typedef int (*history_sfun_t)(void *, TYPE(HistEvent) *, const int)

> [spec:libedit:def:history.history-t]
> typedef struct history_t

> [spec:libedit:def:history.history-vfun-t-void-type-hist-event]
> typedef void (*history_vfun_t)(void *, TYPE(HistEvent) *)
