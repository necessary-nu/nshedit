# src/keymacro.c, src/keymacro.h

> [spec:libedit:def:keymacro.el-keymacro-t]
> typedef struct el_keymacro_t

> [spec:libedit:def:keymacro.keymacro-add-fn]
> libedit_private void keymacro_add(EditLine *el, const wchar_t *key, keymacro_value_t *val, int ntype)

> [spec:libedit:sem:keymacro.keymacro-add-fn]
> Binds the multi-character sequence `key` to `val` in the trie rooted
> at `el->el_keymacro.map`, replacing any existing binding for that
> exact sequence. `ntype` says which arm of the `keymacro_value_t`
> union is live: `XK_CMD` (0) means `val->cmd` is an editor action,
> `XK_STR` (1) means `val->str` is a wide string to be pushed back into
> the input as a macro expansion.
>
> 1. If `key[0] == L'\0'`, print
>    `"keymacro_add: Null extended-key not allowed.\n"` to
>    `el->el_errfile` and return. Nothing is bound.
> 2. If `ntype == XK_CMD` and `val->cmd == ED_SEQUENCE_LEAD_IN`, print
>    `"keymacro_add: sequence-lead-in command not allowed\n"` to
>    `el->el_errfile` and return. `ED_SEQUENCE_LEAD_IN` is the action
>    that tells `read_getcmd` to enter the trie in the first place;
>    binding it inside the trie would make the lookup re-enter itself.
>    Note the guard is only applied to `XK_CMD`; there is no equivalent
>    check on anything else.
> 3. If `el->el_keymacro.map == NULL`, the trie is empty: create the
>    root node for `key[0]` with `node__get(key[0])` and store it in
>    `el->el_keymacro.map`. The result is NOT checked for NULL, so an
>    allocation failure here becomes a NULL dereference in step 4.
> 4. Call `node__try(el, el->el_keymacro.map, key, val, ntype)`,
>    discarding its return value. That is where the per-character
>    insert, the rebind-frees and both prefix-shadowing rules live —
>    see `[spec:libedit:sem:keymacro.node-try-fn]`.
> 5. Return. `void`: no success or failure is ever reported to the
>    caller, including out-of-memory on the `wcsdup` of an `XK_STR`
>    value, which `node__try` signals with a `-1` that `keymacro_add`
>    drops on the floor.
>
> Ownership. `val` is borrowed for the duration of the call only. For
> `XK_CMD` the action byte is copied into the node. For `XK_STR` the
> string is duplicated with `wcsdup` into node-owned storage; the
> caller keeps ownership of the string it passed and may free it
> immediately afterwards. `key` is never retained — it is consumed one
> `wchar_t` at a time into node `ch` fields.
>
> `ntype` must be `XK_CMD` or `XK_STR`. Anything else — including
> `XK_NOD` (2), the "no binding here" node discriminant — falls through
> to `node__try`'s `EL_ABORT`, which calls `abort(3)` and kills the
> process. This is reachable: `terminal_reset_arrow` passes
> `arrow[i].type` straight through, and `terminal_clear_arrow`
> (`bind -k -r up`) sets that field to `XK_NOD`, so a subsequent
> `map_init_emacs`/`map_init_vi` aborts. A port must decide whether to
> reproduce the abort or reject the call; the C's behaviour is a crash.

> [spec:libedit:def:keymacro.keymacro-clear-fn]
> libedit_private void keymacro_clear(EditLine *el, el_action_t *map, const wchar_t *in)

> [spec:libedit:sem:keymacro.keymacro-clear-fn]
> Conditionally drops the extended binding for the whole sequence `in`,
> but only when its first character is still acting as a sequence
> lead-in in `map` and is not also acting as one in the *other* of the
> two per-EditLine action tables. This is the hook that lets a
> single-character rebind in one editing mode retire the trie entry
> without breaking the other mode.
>
> 1. If `*in > N_KEYS` (i.e. `> 256`), return immediately — "can't be
>    in the map".
> 2. Let `c = (unsigned char)*in`. Delete iff
>    `map[c] == ED_SEQUENCE_LEAD_IN` AND either
>    - `map == el->el_map.key` and
>      `el->el_map.alt[c] != ED_SEQUENCE_LEAD_IN`, or
>    - `map == el->el_map.alt` and
>      `el->el_map.key[c] != ED_SEQUENCE_LEAD_IN`.
>
>    The comparison is pointer identity against `el->el_map.key` and
>    `el->el_map.alt`. If `map` is neither of those two pointers, both
>    disjuncts are false and the function does nothing at all.
> 3. If the condition holds, call `keymacro_delete(el, in)` and discard
>    its result. That removes the node reached by the *entire* string
>    `in` and everything below it — not just the first character.
> 4. Return `void`. Neither action table is modified; callers set
>    `map[c]` themselves afterwards.
>
> Bug, off-by-one. `N_KEYS` is 256 and the tables have exactly 256
> entries, so the guard should be `>= N_KEYS`. A `*in` of exactly 256
> passes the guard and `(unsigned char)256 == 0`, so the decision is
> taken from slot 0 of the tables. Likewise, where `wchar_t` is signed
> (Linux, the POSIX target), a negative `*in` also passes the guard and
> the cast wraps it into range. Both cases read the wrong table slot
> and can delete — or fail to delete — the wrong binding. The accesses
> stay inside the 256-entry arrays, so this is a correctness bug, not a
> memory-safety one. A port should test the whole code point against
> `N_KEYS` and treat anything at or above it as "not in the map".

> [spec:libedit:def:keymacro.keymacro-decode-str-fn]
> libedit_private size_t keymacro__decode_str(const wchar_t *str, char *buf, size_t len, const char *sep)

> [spec:libedit:sem:keymacro.keymacro-decode-str-fn]
> Renders the wide string `str` in human-readable printable form into
> the multibyte buffer `buf` of `len` bytes, optionally wrapped in a
> pair of separator characters, and returns the byte count including
> the terminating NUL.
>
> The model is a write cursor `b` starting at `buf` with a limit
> `eb = buf + len`. The internal `ADDC(c)` macro writes `*b++ = c` only
> while `b < eb`; once `b` has reached `eb` it merely increments `b`
> without writing. So `b` keeps counting past the end of the buffer.
>
> 1. If `sep[0] != '\0'`, `ADDC(sep[0])` — the opening separator.
> 2. If `*str == L'\0'`, the empty sequence renders as the two
>    characters `^@`: `ADDC('^')`, `ADDC('@')`, then jump straight to
>    step 4. (This is the only case where the loop is skipped.)
> 3. Otherwise, for each wide character `*p` of `str` in order:
>    a. Render it with `ct_visual_char` into a scratch
>       `wchar_t dbuf[VISUAL_WIDTH_MAX]` (`VISUAL_WIDTH_MAX` is 8),
>       obtaining `l` wide characters: `^X` for ASCII control
>       characters, tab and newline (`'^'` followed by `c | 0100`),
>       `^?` for DEL, `\U+XXXX` (7 chars) or `\U+XXXXX` (8 chars) for
>       non-printables, and the character itself (1 char) for
>       printables. 8 is always sufficient, so this step never fails.
>    b. Encode each of those `l` wide characters to multibyte with
>       `ct_encode_char(b, (size_t)(eb - b), c)` and advance `b` by the
>       byte count returned. The first call that returns `-1` — meaning
>       the remaining room is smaller than the character's multibyte
>       width — abandons the whole loop and jumps to step 4. The output
>       is therefore truncated on a whole-character boundary, never
>       mid-sequence, and `b` never passes `eb` inside this loop.
> 4. If `sep[0] != '\0'` AND `sep[1] != '\0'`, `ADDC(sep[1])` — the
>    closing separator. A one-character `sep` opens without closing.
> 5. `ADDC('\0')`.
> 6. If `(size_t)(b - buf) >= len`, force `buf[len - 1] = '\0'`, so the
>    buffer is always NUL-terminated when `len > 0`.
> 7. Return `(size_t)(b - buf)`.
>
> The return value is *not* the length the full rendering would have
> needed. It counts every byte actually written by step 3 plus the
> separator and NUL bytes from steps 1, 4 and 5 whether or not those
> were written. On truncation it therefore under-reports. Every
> in-tree caller discards it.
>
> Separator conventions used in tree: `""` for no separators at all
> (`map_print_key`), and `"\"\""` (`STRQQ`) to wrap the result in
> double quotes (`map_print_some_keys`, `keymacro_kprint`).
>
> UB: `len == 0` is not handled. Then `eb == buf == b`, so step 1's
> `ADDC` pushes `b` to `buf + 1`, past `eb`; step 3b then computes
> `(size_t)(eb - b)` as `SIZE_MAX` and `ct_encode_char` writes out of
> bounds, and step 6 writes `buf[-1]`. All in-tree callers pass
> `sizeof` of an `EL_BUFSIZ` (1024) byte array, so it is unreachable
> today. A port should reject `len == 0` explicitly.

> [spec:libedit:def:keymacro.keymacro-delete-fn]
> libedit_private int keymacro_delete(EditLine *el, const wchar_t *key)

> [spec:libedit:sem:keymacro.keymacro-delete-fn]
> Removes the binding for `key` and every longer binding that has `key`
> as a prefix.
>
> 1. If `key[0] == L'\0'`, print
>    `"keymacro_delete: Null extended-key not allowed.\n"` to
>    `el->el_errfile` and return `-1`.
> 2. If `el->el_keymacro.map == NULL`, the trie is empty: return `0`
>    without touching anything.
> 3. Call `node__delete(el, &el->el_keymacro.map, key)` and DISCARD its
>    result. The address of the map field is passed so the root can be
>    replaced when the head of the root sibling chain is the node being
>    removed — including replaced with NULL when the last root node
>    goes.
> 4. Return `0`.
>
> Return protocol: `-1` only for the empty-key rejection, `0` in every
> other case. The "was it actually bound?" answer computed by
> `node__delete` is thrown away, so a caller can never tell whether
> anything was deleted.
>
> Effects, from `node__delete` and `node__put`: the node reached by
> `key` is unlinked from its sibling chain and freed together with its
> entire `next` subtree, and every `XK_STR` payload in that subtree is
> `el_free`d. Interior nodes left with no children are then pruned back
> toward the root — and that pruning is unconditional on node type, so
> a shorter binding that lay on the path and had been shadowed by the
> longer key is destroyed too rather than becoming reachable again.
>
> The `el->el_map.key` / `el->el_map.alt` action tables are not
> touched, so the first character keeps its `ED_SEQUENCE_LEAD_IN`
> action. Callers (`map_bind`, `keymacro_clear`) are responsible for
> that, and if they do not fix it up, a later `read_getcmd` will enter
> `keymacro_get` on an empty or partial trie — see the NULL-map hazard
> in `[spec:libedit:sem:keymacro.keymacro-get-fn]`.

> [spec:libedit:def:keymacro.keymacro-end-fn]
> libedit_private void keymacro_end(EditLine *el)

> [spec:libedit:sem:keymacro.keymacro-end-fn]
> Tears the module down. Called exactly once, from `el_end`, after
> `terminal_end` and before `map_end`.
>
> 1. `el_free(el->el_keymacro.buf)`, then set
>    `el->el_keymacro.buf = NULL`.
> 2. `node__free(el->el_keymacro.map)`, which recursively frees every
>    node in the trie.
> 3. Return `void`.
>
> LEAK. Step 2 uses `node__free`, not `node__put`. `node__free` frees
> node structs only — it never looks at `type` and never frees
> `val.str`. So every macro string bound with `XK_STR` (each one
> `wcsdup`ed by `node__try`) is leaked when the `EditLine` is
> destroyed. `keymacro_reset` and `keymacro_delete`, which go through
> `node__put`, do free them; only the teardown path leaks. A port
> should free the payloads here. The absence of the leak is not
> observable across the C ABI, so fixing it is safe.
>
> DANGLING. `el->el_keymacro.map` is NOT set to NULL after the tree is
> freed. It is harmless in practice only because `el_end` frees the
> whole `EditLine` shortly afterwards and nothing between the two reads
> the field. A second `keymacro_end` on the same `EditLine` would
> double-free the whole trie (the `buf` free is idempotent, the tree
> free is not). A port must not carry a dangling handle here; clearing
> it is unobservable.

> [spec:libedit:def:keymacro.keymacro-get-fn]
> libedit_private int keymacro_get(EditLine *el, wchar_t *ch, keymacro_value_t *val)

> [spec:libedit:sem:keymacro.keymacro-get-fn]
> The lookup entry point. A one-line wrapper: return
> `node_trav(el, el->el_keymacro.map, ch, val)`.
>
> On entry `*ch` holds the character that has already been read and
> that caused the caller to enter the trie. `node_trav` matches it
> against the root sibling chain and, while the matched node still has
> children, BLOCKS reading further characters from `el_wgetc` until the
> sequence resolves. There is therefore no "partial match, come back
> with more input" return code — the ambiguity is resolved inside the
> call.
>
> Return protocol, and how `read_getcmd` (the only caller) drives it:
> - `XK_CMD` (0): `val->cmd` is the editor action to run. `*ch` holds
>   the last character consumed, which the action receives as its
>   argument.
> - `XK_STR` (1) with `val->str != NULL`: a macro expansion. `*ch` has
>   been set to `L'\0'`. The caller pushes the string back onto the
>   input with `el_wpush` (which `wcsdup`s it) and loops. `val->str`
>   aliases storage owned by the trie node — the caller must not free
>   it, and it is invalidated by any later rebind or delete of that
>   key.
> - `XK_STR` (1) with `val->str == NULL`: NO MATCH. The sequence ran
>   into a dead end. Every character consumed on the way in is
>   silently discarded — nothing is pushed back — and `*ch` is left
>   holding the character that failed to match, which the caller also
>   drops (its `cmd` is still `ED_SEQUENCE_LEAD_IN`, so it loops and
>   reads a fresh character). "No match" and "matched a string" share
>   a return code; the caller MUST test `val->str` for NULL.
>   `el_wpush(el, NULL)` is in fact tolerated — it beeps — but the
>   caller does not rely on that.
> - `XK_NOD` (2): end of file or read error while reading a
>   continuation character. `read_getcmd` returns `-1`.
> - Anything else is impossible; the caller `EL_ABORT`s on it.
>
> HAZARD: `node_trav` never checks its node pointer for NULL, so
> calling `keymacro_get` with an empty trie (`el->el_keymacro.map ==
> NULL`) dereferences NULL. The C relies on the invariant that a
> character is only routed here when its action table entry is
> `ED_SEQUENCE_LEAD_IN`, and on the delete paths keeping the table and
> the trie in step. That invariant is maintained by convention, not by
> this function. A port should return "no match" (`XK_STR` with a NULL
> string) for an empty trie.

> [spec:libedit:def:keymacro.keymacro-init-fn]
> libedit_private int keymacro_init(EditLine *el)

> [spec:libedit:sem:keymacro.keymacro-init-fn]
> Brings up the key-macro module on a freshly `calloc`ed `EditLine`.
> Called from `el_init_internal` after `terminal_init` and before
> `map_init`.
>
> 1. `el->el_keymacro.buf = el_calloc(KEY_BUFSIZ, sizeof(wchar_t))`,
>    where `KEY_BUFSIZ` is `EL_BUFSIZ` = 1024. This is the shared
>    scratch buffer in which `keymacro_print` / `node_lookup` /
>    `node_enum` assemble the printable form of a key. If the
>    allocation fails, return `-1` immediately, leaving `buf` NULL and
>    `map` untouched.
> 2. Set `el->el_keymacro.map = NULL` — the trie starts empty.
> 3. Call `keymacro_reset(el)`. Because `map` was just set to NULL this
>    is a no-op (`node__put(el, NULL)` returns at once and `map` is set
>    to NULL again).
> 4. Return `0`.
>
> `el->el_keymacro.val`, the shared scratch union, is not initialised
> here; it is already zero because the whole `EditLine` was `el_calloc`ed.
>
> The function comment ("Initialize the key maps") and
> `keymacro_reset`'s ("Then initializes el->el_keymacro.map with arrow
> keys / [Always bind the ansi arrow keys?]") are STALE. Neither
> function binds anything. The ANSI arrow sequences are installed
> later, by `map_init` → `terminal_bind_arrow` → `terminal_reset_arrow`.
>
> `el_init_internal` ignores the return value, so an out-of-memory
> here leaves `el->el_keymacro.buf` NULL on a live `EditLine`, and the
> first `keymacro_print` then writes through a NULL pointer. A port
> should propagate the failure.

> [spec:libedit:def:keymacro.keymacro-kprint-fn]
> libedit_private void keymacro_kprint(EditLine *el, const wchar_t *key, keymacro_value_t *val, int ntype)

> [spec:libedit:sem:keymacro.keymacro-kprint-fn]
> Prints one binding as a single line to `el->el_outfile`, using the
> fixed format `"%-15s->  %s\n"` — the key text left-justified in 15
> columns, then `->` and two spaces, then the binding's description.
>
> The `key` argument is printed as-is; callers hand it something
> already made printable. `node_lookup` and `node_enum` pass
> `el->el_keymacro.buf`, which they have filled with the visual form of
> the sequence surrounded by double quotes; `terminal_print_arrow`
> passes a plain arrow name such as `L"up"`.
>
> 1. If `val == NULL`, print the line with the literal string
>    `"no input"` on the right and return. `ntype` is not consulted.
> 2. Otherwise switch on `ntype`:
>    - `XK_STR`: render the macro string with
>      `keymacro__decode_str(val->str, unparsbuf, sizeof(unparsbuf), "\"\"")`
>      into a local `char unparsbuf[EL_BUFSIZ]` — i.e. the expansion
>      wrapped in double quotes — then print. (The source writes the
>      separator as `ntype == XK_STR ? "\"\"" : "[]"` *inside* the
>      `XK_STR` case, so the condition is always true and the `"[]"`
>      arm is dead code, a leftover from a removed `XK_EXE` type.
>      Implement it as the constant `"\"\""`.)
>    - `XK_CMD`: linear-scan `el->el_map.help[]` from index 0 for the
>      first entry whose `func` equals `val->cmd`. On a hit, convert
>      that entry's wide `name` to multibyte with
>      `wcstombs(unparsbuf, fp->name, sizeof(unparsbuf))`, force
>      `unparsbuf[sizeof(unparsbuf) - 1] = '\0'`, print, and stop
>      scanning. If no entry matches, NOTHING IS PRINTED at all — only
>      a `DEBUG_KEY` build emits `"BUG! Command not found.\n"`.
>      `wcstombs`'s return value is not checked; on an unconvertible
>      name the buffer contents are unspecified apart from the forced
>      terminator.
>    - Any other `ntype`, INCLUDING `XK_NOD`: `EL_ABORT` — the process
>      calls `abort(3)`.
> 3. Return `void`.
>
> The key is converted for printing with
> `ct_encode_string(key, &el->el_scratch)`, which renders the wide key
> into the `EditLine`'s scratch multibyte buffer. It returns NULL on
> allocation failure, and that NULL is handed straight to `%s` —
> undefined behaviour (glibc prints `(null)`).
>
> BUG, out-of-bounds read. The `XK_CMD` scan is written as
> `for (fp = el->el_map.help; fp->name; fp++)`, i.e. it terminates on a
> NULL `name` sentinel. But `el->el_map.help` is an exactly-sized array
> of `el->el_map.nfunc` entries (`el_calloc(EL_NUM_FCNS, ...)` memcpy'd
> from the generated `el_func_help[]`, later `realloc`ed by
> `map_addfunc`), and the generator emits no terminating entry. Every
> element has a non-NULL `name`, so an unmatched command walks off the
> end of the allocation and keeps reading until it happens upon a zero
> word. Not reachable with a legitimately bound command — every valid
> `el_action_t` has a help entry — but a port must bound the scan by
> `el_map.nfunc`.

> [spec:libedit:def:keymacro.keymacro-map-cmd-fn]
> libedit_private keymacro_value_t * keymacro_map_cmd(EditLine *el, int cmd)

> [spec:libedit:sem:keymacro.keymacro-map-cmd-fn]
> Packages an editor command number as a `keymacro_value_t` so it can
> be handed to `keymacro_add` or `terminal_set_arrow`.
>
> 1. Store `(el_action_t)cmd` into `el->el_keymacro.val.cmd`.
>    `el_action_t` is `unsigned char`, so the cast truncates anything
>    outside 0..255 silently; callers get `cmd` from `parse_cmd` or
>    from an existing action table entry, both already in range.
> 2. Return `&el->el_keymacro.val`.
>
> No allocation, no copying, and the trie is not touched.
> `el->el_keymacro.val` is a single per-`EditLine` scratch slot shared
> with `keymacro_map_str`, so the returned pointer is always the same
> address and the value it holds survives only until the next
> `keymacro_map_cmd` or `keymacro_map_str` call on that `EditLine`.
> The idiom is always
> `keymacro_add(el, key, keymacro_map_cmd(el, cmd), XK_CMD)` — build
> and consume in one expression. Note that `keymacro_add` also accepts
> pointers that did NOT come from here (`terminal.c` passes
> `&arrow[i].fun` directly), so a port must not assume the value
> argument is the scratch slot.

> [spec:libedit:def:keymacro.keymacro-map-str-fn]
> libedit_private keymacro_value_t * keymacro_map_str(EditLine *el, wchar_t *str)

> [spec:libedit:sem:keymacro.keymacro-map-str-fn]
> Packages a macro expansion string as a `keymacro_value_t`.
>
> 1. Store the pointer `str` — NOT a copy — into
>    `el->el_keymacro.val.str`.
> 2. Return `&el->el_keymacro.val`.
>
> Nothing is allocated, duplicated or freed, and the trie is not
> touched. The union member is a non-owning alias of the caller's
> buffer: `map_bind` passes a `wchar_t outbuf[EL_BUFSIZ]` living on its
> own stack frame, so `el->el_keymacro.val.str` dangles the moment
> `map_bind` returns. That is safe only because the value is consumed
> immediately, within the same statement, by `keymacro_add` (which
> `wcsdup`s it into the node) or `terminal_set_arrow` (which copies the
> union into `arrow[i].fun`, and so inherits the same aliasing —
> arrow-key `XK_STR` bindings hold a borrowed pointer). As with
> `keymacro_map_cmd`, the slot is shared per `EditLine` and is
> overwritten by the next `keymacro_map_*` call.

> [spec:libedit:def:keymacro.keymacro-node-t]
> struct keymacro_node_t {
>   wchar_t ch;
>   int type;
>   keymacro_value_t val;
>   struct keymacro_node_t *next;
>   struct keymacro_node_t *sibling;
> }

> [spec:libedit:def:keymacro.keymacro-print-fn]
> libedit_private void keymacro_print(EditLine *el, const wchar_t *key)

> [spec:libedit:sem:keymacro.keymacro-print-fn]
> Prints the binding for `key`, or the whole map when `key` is empty.
>
> 1. If `el->el_keymacro.map == NULL` AND `*key == 0`, return
>    immediately — nothing to enumerate.
> 2. Write the opening quote: `el->el_keymacro.buf[0] = L'"'`. This is
>    the shared 1024-`wchar_t` print buffer; the rest of the key text
>    is appended from offset 1 by `node_lookup` / `node_enum`, and the
>    closing quote and terminator are written by whichever of them
>    reaches the end of the key. If `keymacro_init`'s allocation failed
>    this dereferences NULL.
> 3. Call `node_lookup(el, key, el->el_keymacro.map, (size_t)1)` — the
>    `1` is the count of `wchar_t` already in the buffer, i.e. the
>    opening quote.
> 4. If the result is `<= -1`, print
>    `"Unbound extended key \"%ls\"\n"` with `key` to
>    `el->el_errfile`. `node_lookup` only ever returns `0` or `-1`, so
>    this is exactly the "not bound / did not fit" case.
> 5. Return `void`.
>
> Behaviour by key shape, inherited from `node_lookup`:
> - empty key with a non-empty map: enumerate EVERY binding in the map,
>   walking the whole root sibling chain (this is how
>   `map_print_all_keys` dumps the extended bindings);
> - a key that is a proper prefix of bindings: enumerate all its
>   completions;
> - a complete key: print just that binding;
> - a key with no path in the trie, or one longer than any binding:
>   the "Unbound extended key" message.
>
> Note that `map == NULL` with a non-empty key still executes step 2
> before `node_lookup` returns `-1`, so the buffer is written even on
> the unbound path.

> [spec:libedit:def:keymacro.keymacro-reset-fn]
> libedit_private void keymacro_reset(EditLine *el)

> [spec:libedit:sem:keymacro.keymacro-reset-fn]
> Empties the trie completely.
>
> 1. `node__put(el, el->el_keymacro.map)` — recursively frees every
>    node reachable from the root, following both `next` and `sibling`
>    links, and `el_free`s the `val.str` of every `XK_STR` node on the
>    way. Safe when `map` is already NULL (`node__put` returns at
>    once).
> 2. `el->el_keymacro.map = NULL`.
> 3. Return `void`.
>
> Unlike `keymacro_end`'s `node__free`, this path DOES release the
> macro strings, so a reset does not leak.
>
> It does not touch `el->el_map.key` / `el->el_map.alt`, so any
> `ED_SEQUENCE_LEAD_IN` entries in those tables survive and now point
> at nothing. The two callers, `map_init_vi` and `map_init_emacs`,
> immediately overwrite both tables from the static defaults and then
> re-bind through `map_init_meta`, `tty_bind_char` and
> `terminal_bind_arrow`, so the inconsistency never escapes.
>
> The function comment ("Then initializes el->el_keymacro.map with
> arrow keys / [Always bind the ansi arrow keys?]") is STALE — no
> binding of any kind happens here.

> [spec:libedit:def:keymacro.keymacro-value-t]
> typedef union keymacro_value_t

> [spec:libedit:def:keymacro.node-delete-fn]
> static int node__delete(EditLine *el, keymacro_node_t **inptr, const wchar_t *str)

> [spec:libedit:sem:keymacro.node-delete-fn]
> Removes the node reached by `str` from the level whose sibling chain
> starts at `*inptr`, together with everything below it, then prunes
> emptied ancestors on the way back out.
>
> `inptr` is the ADDRESS of the slot holding this level's head node —
> either `&el->el_keymacro.map` at the top or `&parent->next` in a
> recursive call — so the head can be rewritten, including to NULL,
> when the head itself is what gets removed.
>
> 1. `ptr = *inptr` (never NULL-checked; `keymacro_delete` guarantees a
>    non-NULL root and the recursion only descends into non-NULL
>    `next`). `prev_ptr = NULL`.
> 2. If `ptr->ch != *str`, search the sibling chain: walk `xm` from
>    `ptr` while `xm->sibling != NULL`, breaking early when
>    `xm->sibling->ch == *str`. If the walk ran off the end with
>    `xm->sibling == NULL`, no node matches this character — return `0`
>    with nothing changed. Otherwise set `prev_ptr = xm` and
>    `ptr = xm->sibling`. `prev_ptr` therefore stays NULL exactly when
>    the head of the chain was the match.
> 3. Advance `str` by one (pre-increment). If `*str` is now `L'\0'`,
>    this node terminates the key:
>    a. Unlink it: if `prev_ptr == NULL`, `*inptr = ptr->sibling`;
>       otherwise `prev_ptr->sibling = ptr->sibling`.
>    b. `ptr->sibling = NULL`. THIS IS LOAD-BEARING — `node__put`
>       follows `sibling` links, so without this it would free the rest
>       of the chain that was just relinked around this node.
>    c. `node__put(el, ptr)`, which frees this node, its entire `next`
>       subtree and every `XK_STR` payload therein.
>    d. Return `1`.
>
>    Consequence: deleting a key deletes every longer key that has it
>    as a prefix, since they all hang off this node's `next`.
> 4. Otherwise, if `ptr->next != NULL` AND the recursive
>    `node__delete(el, &ptr->next, str)` returns `1`:
>    - If `ptr->next` is still non-NULL (the child level had other
>      siblings and the recursion re-pointed the slot at one of them),
>      return `0`. Returning `0` here is what stops the prune from
>      propagating any further up.
>    - Otherwise the child level is now empty, so this node is dead
>      too: unlink it exactly as in step 3a, `ptr->sibling = NULL`,
>      `node__put(el, ptr)`, return `1`, and the prune continues one
>      level further out.
> 5. In every other case — no `next` to descend into, or the recursion
>    returned `0` — return `0`.
>
> Return protocol: `1` means "a node was removed at this level, and the
> level may now be empty — check your `next`"; `0` means "nothing was
> removed here". Only `keymacro_delete` sees the outermost value, and
> it discards it.
>
> IMPORTANT, shadowed bindings are destroyed by the prune. Step 4 never
> looks at `ptr->type`. If a node carries a complete binding of its own
> and also has children (which happens whenever a longer key was added
> over a shorter one — see `node__try` step 3), deleting the longer key
> empties the child level and then frees the node holding the shorter
> binding as well. Binding `"ab"`, then `"abc"`, then deleting `"abc"`
> leaves NEITHER bound, and the prune runs all the way back to the root
> so the `"a"` node goes too. A port must reproduce this; "unshadowing"
> the shorter binding would be a behaviour change.
>
> Recursion depth is the key length plus the sibling-chain lengths
> walked; there is no bound.

> [spec:libedit:def:keymacro.node-enum-fn]
> static int node_enum(EditLine *el, keymacro_node_t *ptr, size_t cnt)

> [spec:libedit:sem:keymacro.node-enum-fn]
> Prints every complete binding at and below `ptr`, and across `ptr`'s
> whole sibling chain, assembling each key's printable text in
> `el->el_keymacro.buf`. `cnt` is the number of `wchar_t` already
> placed in that buffer by the callers above (the opening quote plus
> the prefix matched so far).
>
> 1. Buffer-exhaustion guard first: if `cnt >= KEY_BUFSIZ - 5` (i.e.
>    `>= 1019`), write `buf[++cnt] = L'"'` then `buf[++cnt] = L'\0'`,
>    print `"Some extended keys too long for internal print buffer"`
>    to `el->el_errfile` followed by a second `fprintf` of
>    `" \"%ls...\"\n"` with the buffer, and return `0` without
>    descending or visiting siblings.
>    BUG: both increments are PRE-increments, so the quote lands at
>    `cnt + 1` and the terminator at `cnt + 2`, leaving `buf[cnt]`
>    holding whatever was there before — a stale character from a
>    previously printed key, or an uninitialised zero from the
>    `el_calloc`. The intent was plainly `buf[cnt]` and `buf[cnt + 1]`.
>    A port should write the two at `cnt` and `cnt + 1`.
> 2. If `ptr == NULL`, return `-1` (a `DEBUG_EDIT` build first prints
>    `"node_enum: BUG!! Null ptr passed\n!"`). `node_enum` never
>    recurses on a NULL pointer, so this only fires for a bad caller.
> 3. Append this node's character:
>    `used = ct_visual_char(el->el_keymacro.buf + cnt, KEY_BUFSIZ - cnt, ptr->ch)`.
>    BUG: the return value is NOT checked. Step 1 only guarantees six
>    free `wchar_t`, but a non-BMP non-printable needs eight, so
>    `ct_visual_char` can return `-1`; then `cnt + (size_t)used` is
>    `cnt - 1` and the writes in step 4 land one slot early, over the
>    previous character, with nothing having been appended. A port
>    should check for `-1` and bail out the way `node_lookup` does.
> 4. If `ptr->next == NULL` this is a complete binding: write
>    `buf[cnt + used] = L'"'` and `buf[cnt + used + 1] = L'\0'`, then
>    call `keymacro_kprint(el, buf, &ptr->val, ptr->type)` to print the
>    key and its action. Otherwise recurse
>    `node_enum(el, ptr->next, cnt + used)` to print everything below.
> 5. Then, if `ptr->sibling != NULL`, recurse
>    `node_enum(el, ptr->sibling, cnt)` — with the ORIGINAL `cnt`, so
>    the sibling's rendering overwrites this node's character in the
>    buffer. Output order is depth-first, children before siblings.
> 6. Return `0`.
>
> The "is this a binding" test is `ptr->next == NULL`, NOT
> `ptr->type != XK_NOD`. So an interior node that also carries a
> complete binding — a key shadowed by a longer one — is never printed.
> Shadowed bindings are invisible to `bind` output exactly as they are
> invisible to `node_trav`. Conversely, if a node ever ended up a leaf
> with `type == XK_NOD`, `keymacro_kprint` would `abort()`; the insert
> paths do not produce that state.
>
> Returns `0` normally, `-1` only for the NULL-pointer case; both
> callers discard the value.

> [spec:libedit:def:keymacro.node-free-fn]
> static void node__free(keymacro_node_t *k)

> [spec:libedit:sem:keymacro.node-free-fn]
> Frees a whole trie, node structs only.
>
> 1. If `k == NULL`, return.
> 2. `node__free(k->sibling)` — the sibling chain first.
> 3. `node__free(k->next)` — then the children.
> 4. `el_free(k)`.
>
> Takes no `EditLine`; returns `void`. Every node reachable through
> either link is freed, so the ordering only matters in that both links
> must be followed before the node itself is released.
>
> It differs from `node__put` in two ways, and both are deliberate to
> the point of being the reason both functions exist — except that one
> of them is a bug:
> - It does NOT free `val.str` for `XK_STR` nodes. Since
>   `keymacro_end` is its only caller, every macro string bound during
>   the `EditLine`'s life leaks at teardown. A port should free them.
> - It does NOT `EL_ABORT` on an unrecognised `type`; it never reads
>   `type` at all.
>
> It also does not NULL the links it follows, but as every node is
> freed there is no reader left to observe them.

> [spec:libedit:def:keymacro.node-get-fn]
> static keymacro_node_t * node__get(wint_t ch)

> [spec:libedit:sem:keymacro.node-get-fn]
> Allocates and initialises one trie node for the character `ch`.
>
> 1. `ptr = el_malloc(sizeof(keymacro_node_t))`. If it returns NULL,
>    return NULL.
> 2. `ptr->ch = ch` — the `wint_t` argument narrowed to `wchar_t`.
> 3. `ptr->type = XK_NOD` (2), the "no binding here" discriminant: this
>    node exists only because it lies on the path of some longer key.
>    `node__try` overwrites it with `XK_CMD` or `XK_STR` if and when
>    the node becomes the last character of a bound sequence.
> 4. `ptr->val.str = NULL`. Writing the pointer member zeroes the union
>    on every layout of interest, so `val.cmd` reads back as 0 too.
> 5. `ptr->next = NULL`, `ptr->sibling = NULL` — unlinked.
> 6. Return the node.
>
> The caller is responsible for linking it in; `node__get` touches no
> `EditLine` state and does not know where the node will go.
>
> HAZARD: neither of the two call sites in `node__try`, nor the one in
> `keymacro_add`, checks the result for NULL — each immediately stores
> it into a link field and then dereferences it. Out of memory during a
> bind is therefore a NULL dereference, not an error return. A port
> should propagate the failure.

> [spec:libedit:def:keymacro.node-lookup-fn]
> static int node_lookup(EditLine *el, const wchar_t *str, keymacro_node_t *ptr, size_t cnt)

> [spec:libedit:sem:keymacro.node-lookup-fn]
> Walks `str` down the trie from `ptr`, appending the printable
> rendering of each matched character to `el->el_keymacro.buf` at
> offset `cnt`, and prints when the key is exhausted. `cnt` is the
> number of `wchar_t` already in the buffer — `keymacro_print` seeds
> `buf[0]` with the opening quote and passes `1`.
>
> 1. If `ptr == NULL`, return `-1` ("cannot have null ptr"). This is
>    the empty-map case as well as the recursion's guard.
> 2. If `str` is NULL or `*str == 0` — no key characters left to
>    match — call `node_enum(el, ptr, cnt)` to print every binding at
>    and below `ptr` INCLUDING its whole sibling chain, and return `0`.
>    This is how an empty key dumps the entire map and how a prefix
>    key lists all its completions.
> 3. Otherwise, if `ptr->ch == *str` (match at this position):
>    a. `used = ct_visual_char(el->el_keymacro.buf + cnt, KEY_BUFSIZ - cnt, ptr->ch)`
>       appends the visual form of the character (`^X` for controls,
>       `^?` for DEL, `\U+XXXX` / `\U+XXXXX` for non-printables, the
>       character itself otherwise). If it returns `-1` there was not
>       enough room; return `-1`.
>    b. If `ptr->next != NULL`, recurse
>       `node_lookup(el, str + 1, ptr->next, (size_t)used + cnt)`.
>       Note that if `str + 1` is the terminator the recursion takes
>       step 2 and enumerates the whole subtree — the prefix case.
>    c. Otherwise this node is a leaf. If `str[1] == 0` the key is
>       complete: let `px = cnt + used`, write `buf[px] = L'"'` and
>       `buf[px + 1] = L'\0'`, call
>       `keymacro_kprint(el, el->el_keymacro.buf, &ptr->val, ptr->type)`,
>       and return `0`. If `str[1] != 0` the caller's key is longer
>       than any binding on this path: return `-1`.
> 4. Otherwise (no match at this position): if `ptr->sibling != NULL`,
>    recurse with the SAME `str` and the SAME `cnt`; otherwise return
>    `-1`.
>
> Returns `0` when something was printed or enumerated, `-1` for "not
> bound" or "ran out of buffer". No other values.
>
> BUG, buffer overflow. Step 3c writes two `wchar_t` at `px` and
> `px + 1` with no bounds check. `ct_visual_char` only guarantees
> `cnt + used <= KEY_BUFSIZ`, so a key whose rendering exactly fills
> the 1024-`wchar_t` buffer writes one or two `wchar_t` past its end.
> `node_enum` has the same unchecked pattern behind a (leaky) size
> guard; `node_lookup` has no guard at all. A port must bound-check
> before writing the closing quote and terminator.
>
> Recursion depth is the key length plus the sibling chains walked. The
> buffer is the single shared `el->el_keymacro.buf`, so this is not
> re-entrant.

> [spec:libedit:def:keymacro.node-put-fn]
> static void node__put(EditLine *el, keymacro_node_t *ptr)

> [spec:libedit:sem:keymacro.node-put-fn]
> Frees `ptr` and EVERYTHING reachable from it — children and the
> remaining sibling chain — including the `XK_STR` payloads.
>
> Despite the name and the comment ("Puts a tree of nodes onto free
> list"), there is no free list and no reuse: it is plain `el_free` on
> every node. A port should implement it as a drop, not a pool return.
>
> 1. If `ptr == NULL`, return.
> 2. If `ptr->next != NULL`: `node__put(el, ptr->next)`, then
>    `ptr->next = NULL`. The store is dead — `ptr` is freed in step 5 —
>    but it is harmless.
> 3. `node__put(el, ptr->sibling)`. `sibling` is NOT nulled. This is
>    the behaviour callers depend on in both directions:
>    - `node__try` calls `node__put(ptr->next)` precisely to drop the
>      whole child level, all its siblings and their subtrees, in one
>      call;
>    - `node__delete` must therefore unlink its victim AND set
>      `victim->sibling = NULL` before calling, or it would take the
>      rest of the chain with it.
> 4. Release this node's payload by `type`:
>    - `XK_CMD` and `XK_NOD`: nothing to free.
>    - `XK_STR`: if `ptr->val.str != NULL`, `el_free(ptr->val.str)`.
>    - anything else: `EL_ABORT((el->el_errfile, "Bad XK_ type %d\n", ptr->type))`,
>      which calls `abort(3)`. This is the only use of the `el`
>      parameter.
> 5. `el_free(ptr)`.
>
> Order: children, then siblings, then own string, then own node.
> Returns `void`. This is the ONLY path that frees `XK_STR` payloads —
> `node__free`, used by `keymacro_end`, does not.
>
> Recursion depth is unbounded in principle: it nests once per trie
> level and once per sibling in a chain, and sibling chains at the root
> can be as long as the number of distinct first characters bound.

> [spec:libedit:def:keymacro.node-trav-fn]
> static int node_trav(EditLine *el, keymacro_node_t *ptr, wchar_t *ch, keymacro_value_t *val)

> [spec:libedit:sem:keymacro.node-trav-fn]
> The lookup engine. Recursively matches `*ch` against the sibling
> chain starting at `ptr`, reading further characters from the input
> whenever the matched node still has children, until the sequence
> resolves to a binding or hits a dead end.
>
> `ptr` is dereferenced without a NULL check on entry.
>
> 1. If `ptr->ch == *ch` — this node matches the current character:
>    a. If `ptr->next != NULL` the key is not complete. Read one more
>       wide character with `el_wgetc(el, ch)`. If that does not return
>       exactly `1` (end of file, read error, or the tty could not be
>       put in raw mode), return `XK_NOD` at once; `*ch` holds whatever
>       `el_wgetc` left in it and every character consumed so far is
>       lost. Otherwise recurse `node_trav(el, ptr->next, ch, val)`
>       with the new character. Only `next` is followed — searching the
>       child level's sibling chain is the recursive call's own job.
>    b. Otherwise this node is a leaf and the sequence is complete:
>       copy the node's union with `*val = ptr->val`, then, if
>       `ptr->type != XK_CMD`, set `*ch = L'\0'`. Return `ptr->type`.
> 2. Otherwise — this node does not match:
>    a. If `ptr->sibling != NULL`, recurse
>       `node_trav(el, ptr->sibling, ch, val)` with the same `*ch`.
>    b. Otherwise this is a dead end: set `val->str = NULL` and return
>       `XK_STR`.
>
> Return values and their meanings:
> - `XK_CMD` — `val->cmd` is the bound action; `*ch` keeps the last
>   character read (this is why the `*ch = '\0'` in 1b is conditional:
>   command bindings want the character, macro expansions do not).
> - `XK_STR` with `val->str != NULL` — a macro expansion; `*ch` is
>   `L'\0'`. The pointer ALIASES the node's own `wcsdup`ed string. The
>   caller must not free it and must not hold it across any rebind or
>   delete of that key, both of which `el_free` it.
> - `XK_STR` with `val->str == NULL` — NO MATCH. Note this shares its
>   return code with a successful string binding; the caller must test
>   the pointer. `*ch` is left holding the character that failed to
>   match, and neither it nor any earlier character of the attempted
>   sequence is pushed back — the input is simply consumed and
>   discarded.
> - `XK_NOD` — EOF or read error mid-sequence (step 1a). It would also
>   be returned by 1b for a leaf whose type was never set, and for a
>   leaf left as `XK_STR` with a NULL string by a failed `wcsdup` in
>   `node__try` the caller cannot distinguish that from no-match.
>
> There is deliberately NO "partial match, need more input" code. A
> partial match blocks inside `el_wgetc`, which first drains any pushed
> macro text and only then reads the tty. Recursion depth is the
> sequence length plus the sibling chains walked.
>
> Shadowing, from the reader's side: the test at step 1 is
> `ptr->next != NULL`, checked BEFORE `type` is ever consulted. So if a
> node carries a complete binding and also has children, the shorter
> binding is unreachable — the traversal always demands another
> character. The shorter binding's value is still allocated and still
> occupies the node; it is simply never returned. See `node__try`
> step 3.

> [spec:libedit:def:keymacro.node-try-fn]
> static int node__try(EditLine *el, keymacro_node_t *ptr, const wchar_t *str, keymacro_value_t *val, int ntype)

> [spec:libedit:sem:keymacro.node-try-fn]
> The insert engine. Places the character `*str` at the level whose
> sibling chain begins at `ptr`, then either finishes the binding (if
> this was the last character) or recurses into the child level.
>
> 1. Locate or create the node for `*str` at this level. If
>    `ptr->ch != *str`:
>    a. Walk `xm` from `ptr` while `xm->sibling != NULL`, breaking
>       early when `xm->sibling->ch == *str`.
>    b. If the walk ran to the end of the chain with
>       `xm->sibling == NULL`, no sibling matched: append a fresh node,
>       `xm->sibling = node__get(*str)`. New siblings therefore go on
>       the END of the chain, and chains are in insertion order — which
>       is also the order `node_enum` prints them in.
>    c. `ptr = xm->sibling`.
>    `node__get`'s NULL return is not checked, so an allocation failure
>    here becomes a NULL dereference immediately below.
> 2. Advance `str` by one (pre-increment). If `*str` is now `L'\0'`,
>    this node is the last character of the key:
>    a. If `ptr->next != NULL`, call `node__put(el, ptr->next)` and
>       then set `ptr->next = NULL`. THIS IS THE SHADOWING RULE IN THE
>       DESTRUCTIVE DIRECTION: `node__put` frees the entire child
>       level, all its siblings, all their subtrees and every `XK_STR`
>       payload in them. Binding a key that is a proper prefix of
>       existing longer keys silently DESTROYS all of them. This is the
>       behaviour the file header warns about: with `"abcd"` and
>       `"abcef"` bound, adding `"abc"` loses both.
>    b. Release the old payload, switching on the CURRENT `ptr->type`:
>       `XK_CMD` and `XK_NOD` need nothing; `XK_STR` `el_free`s
>       `ptr->val.str` if it is non-NULL; any other value hits
>       `EL_ABORT` and aborts the process.
>    c. Install the new payload, switching on `ptr->type = ntype` (note
>       the type field is written BEFORE the switch, so even the abort
>       path has already mutated the node):
>       - `XK_CMD`: `ptr->val = *val`, copying the action by value.
>       - `XK_STR`: `ptr->val.str = wcsdup(val->str)`; if that returns
>         NULL, RETURN `-1` IMMEDIATELY, leaving the node with
>         `type == XK_STR` and `val.str == NULL` — a state
>         indistinguishable at lookup time from `node_trav`'s no-match
>         answer. `wcsdup(NULL)` if `val->str` is NULL is undefined
>         behaviour; no caller does it.
>       - anything else, including `XK_NOD`: `EL_ABORT`, i.e.
>         `abort(3)`.
> 3. Otherwise there are more characters to place: if
>    `ptr->next == NULL`, create the child with
>    `ptr->next = node__get(*str)` (again unchecked for NULL), then
>    recurse `node__try(el, ptr->next, str, val, ntype)` and DISCARD
>    its return value.
>    THIS IS THE SHADOWING RULE IN THE OTHER DIRECTION: this branch
>    does not touch `ptr->type` or `ptr->val`. A node that already held
>    a complete binding keeps it and merely gains a child. The shorter
>    binding stays allocated but becomes UNREACHABLE, because
>    `node_trav` tests `next` before it looks at `type`. Adding a
>    longer key over a shorter one shadows the shorter one instead of
>    deleting it — and if the longer key is later deleted, the prune in
>    `node__delete` frees the shadowed one rather than restoring it.
> 4. Return `0`.
>
> Return protocol: `0` normally, `-1` only from the `wcsdup` failure in
> step 2c. The `-1` cannot escape: step 3 discards the recursive
> result, and `keymacro_add` discards the outermost one. Allocation
> failure during a bind is silently swallowed.
>
> Ownership summary. The key is consumed one character at a time into
> node `ch` fields and never retained as a string. `XK_CMD` values are
> copied by value. `XK_STR` values are duplicated into node-owned
> storage, freed by step 2b on rebind and by `node__put` on delete or
> reset (but NOT by `node__free` on `keymacro_end` — see that rule).
