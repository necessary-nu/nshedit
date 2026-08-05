# src/map.c, src/map.h

> [spec:libedit:def:map.el-bindings-t]
> typedef struct el_bindings_t

> [spec:libedit:def:map.el-func-t-edit-line-wint-t]
> typedef el_action_t (*el_func_t)(EditLine *, wint_t)

> [spec:libedit:def:map.el-map-t]
> typedef struct el_map_t

> [spec:libedit:def:map.map-addfunc-fn]
> libedit_private int map_addfunc(EditLine *el, const wchar_t *name, const wchar_t *help, el_func_t func)

> [spec:libedit:sem:map.map-addfunc-fn]
> Appends one caller-supplied editor function to the runtime function
> table and gives it a fresh command number plus a `bind`-visible name.
> This is the implementation of `el_set(EL_ADDFN, ...)`.
>
> 1. If any of `name`, `help` or `func` is NULL, return -1 having changed
>    nothing. There is no other validation: a name identical to an
>    existing one is accepted, and because command-name lookup
>    (`parse_cmd`, see `[spec:libedit:sem:parse.parse-cmd-fn]`) scans
>    `el_map.help` from index 0 and returns the first match, the later
>    duplicate becomes unreachable by name — it is not an error.
> 2. Let `nf = el_map.nfunc + 1`, the new length. Reallocate
>    `el_map.func` to `nf * sizeof(el_func_t)`; on failure return -1 with
>    the old array still installed and `nfunc` unchanged. Then reallocate
>    `el_map.help` to `nf * sizeof(el_bindings_t)`; on failure return -1.
>    Note that in this second failure case `el_map.func` has already been
>    replaced by the grown block — that is not a leak (the pointer is
>    stored), but the array is one slot longer than `nfunc` claims and the
>    slot holds indeterminate bytes; nothing ever reads past `nfunc`, so
>    it is benign. The two arrays are therefore only guaranteed to be the
>    same length when the call succeeds.
> 3. Let `nf = el_map.nfunc` again — now the *index* of the new entry,
>    which is also its command number. Store `el_map.func[nf] = func`
>    (the function pointer is stored raw; the map does not own it and the
>    caller must keep it valid for the life of the `EditLine`).
> 4. Fill `el_map.help[nf]`: `.name = wcsdup(name)`,
>    `.func = (int)nf`, `.description = wcsdup(help)`. Both duplicates are
>    owned by `el_map.help` from here on and are freed by `map_end`, which
>    frees exactly the entries with index >= EL_NUM_FCNS.
> 5. Increment `el_map.nfunc`. Return 0.
>
> Neither `wcsdup` is checked. On allocation failure the entry is left
> with a NULL `name` and/or `description` while `nfunc` has already been
> bumped, after which `parse_cmd` (`wcscmp` against NULL) and `bind -l`
> (`%ls` of NULL) are undefined behaviour. A Rust port that cannot
> allocate should either abort or, to stay bug-compatible, store an
> equivalent "absent string" and reproduce the same downstream crash —
> the C behaviour here is UB, so it is not observable behaviour worth
> matching precisely; prefer failing the call.
>
> Adding a function does not bind it to any key. The caller must follow
> up with `bind` / `el_set(EL_BIND, ...)` using the name just registered.
>
> Numbering trap: command numbers handed out here are EL_NUM_FCNS,
> EL_NUM_FCNS+1, ... continuing the generated numbering (see
> `[spec:libedit:sem:map.map-init-fn]`). `el_action_t` is `unsigned char`,
> so a keymap slot cannot hold a command number above 255. With
> EL_NUM_FCNS == 96 the 160th added function is the first whose number
> does not fit, and `map_bind` stores it with a silent truncating cast.
> The command dispatcher independently ignores any command number
> >= `el_map.nfunc`.

> [spec:libedit:def:map.map-bind-fn]
> libedit_private int map_bind(EditLine *el, int argc, const wchar_t **argv)

> [spec:libedit:sem:map.map-bind-fn]
> The `bind` builtin: inspect, add, change and remove key bindings. It is
> reached from `el_parse`/`editrc` (as the command named `bind`) and from
> `el_set(EL_BIND, ...)`.
>
> **The `argc` parameter is dead.** The first statement of the argument
> loop is `argc = 1`, overwriting it. Iteration stops on the first NULL
> element of `argv`, so `argv` **must** be NULL-terminated; the caller's
> count is never consulted. (`el_set(EL_BIND, ...)` builds a 20-element
> `argv` and only NUL-terminates it if fewer than 19 arguments were
> passed — with a full 19 the array is unterminated and this function
> reads past its end. That is UB in the caller, but the port's ABI shim
> inherits the requirement: always terminate.)
>
> `argv[0]` is the command name, used only in error messages.
>
> Step 1 — if `argv` is NULL, return -1.
>
> Step 2 — initialise: `map = el_map.key` (the normal keymap),
> `ntype = XK_CMD` (0), `key = 0`, `rem = 0`.
>
> Step 3 — option scan. For `i` from 1 upward while `argv[i] != NULL`:
> if `argv[i][0] != '-'`, stop the scan with `i` pointing at that element.
> Otherwise dispatch on the single character `argv[i][1]`:
>
> - `-a` — `map = el_map.alt`; subsequent lookups/edits target the
>   alternate keymap (vi command mode) instead of the normal one.
> - `-s` — `ntype = XK_STR`; the value argument is a string to push back
>   into the input, not a command name.
> - `-k` — `key = 1`; the key argument is a *function-key name*
>   (`up`, `down`, `left`, `right`, `home`, `end`, `delete`) rather than a
>   key sequence, and is passed through verbatim without escape
>   processing.
> - `-r` — `rem = 1`; remove the binding rather than create one.
> - `-v` — call `map_init_vi(el)` and **return 0 immediately**, ignoring
>   every remaining argument.
> - `-e` — call `map_init_emacs(el)` and **return 0 immediately**,
>   likewise.
> - `-l` — for every entry of `el_map.help[0 .. nfunc)` print
>   `"%ls\n\t%ls\n"` with `name` then `description` to `el_outfile`, then
>   **return 0 immediately**.
> - anything else, including a bare `-` (where `argv[i][1]` is `'\0'`) —
>   print `"%ls: Invalid switch `%lc'.\n"` to `el_errfile` with `argv[0]`
>   and the offending character, then **continue the scan**. This is not
>   an error return; the malformed flag is consumed and skipped.
>
> Only `argv[i][1]` is examined, so clustered flags do not work: `-ar` is
> read as `-a` and the trailing characters are discarded silently. Flags
> must precede the key argument; the first non-`-` argument ends the scan
> and any later `-x` is treated as data.
>
> Step 4 — no key argument. If the scan stopped at the terminating NULL,
> call `map_print_all_keys(el)` and return 0. So a bare `bind`, or
> `bind -a`, dumps the whole binding state.
>
> Step 5 — the key argument, consumed from `argv` and the index advanced.
> If `-k` was given, `in` is the raw argument string. Otherwise
> `in = parse__string(inbuf, arg)` — an in-place decode into a
> 1024-`wchar_t` stack buffer that expands `\` and `^` escapes and turns a
> leading `M-` into ESC (see `[spec:libedit:sem:parse.parse-string-fn]`).
> If it returns NULL (a malformed `\` or `^` escape), print
> `"%ls: Invalid \\ or ^ in instring.\n"` with `argv[0]` to `el_errfile`
> and return -1. `parse__string` performs no bounds checking, so a key
> argument that decodes to more than EL_BUFSIZ wide characters overflows
> the stack buffer — undefined behaviour that the port must not
> reproduce; bound the decode instead.
>
> Step 6 — removal (`-r`).
>
> - With `-k`: call `terminal_clear_arrow(el, in)` (which sets that
>   function key's type to XK_NOD, or returns -1 if the name is unknown)
>   and then **return -1 unconditionally**. This is a bug: a successful
>   `bind -k -r up` still reports failure, and via `el_parse` the failure
>   is negated into a positive return. Behaviour across the C ABI is
>   frozen, so the port must keep returning -1 here.
> - Without `-k`, on a multi-character sequence (`in[1] != 0`): delete the
>   sequence from the keymacro trie with `keymacro_delete(el, in)`; its
>   result is discarded. The lead-in marker left in the keymap is *not*
>   cleared.
> - Without `-k`, on a single character: if `map[(unsigned char)in[0]]` is
>   ED_SEQUENCE_LEAD_IN, call `keymacro_delete(el, in)` (removing the
>   one-character trie entry); otherwise set
>   `map[(unsigned char)in[0]] = ED_UNASSIGNED`.
> - Return 0.
>
> Note `in[1]` is read even when `in[0]` is the terminating NUL — an
> empty key argument (`bind -r ""`) reads an uninitialised stack slot and
> then indexes `map[0]`. Undefined behaviour; the port should treat an
> empty sequence as the single-element case (`in[1]` absent) or reject it.
>
> Step 7 — query. If there is no further argument: with `-k` call
> `terminal_print_arrow(el, in)` (prints that function key's binding, or
> nothing if the name does not match); otherwise call
> `map_print_key(el, map, in)`. Return 0.
>
> Step 8 — extra arguments past the value are silently ignored. The
> arity check exists in the source but is inside `#ifdef notyet` and is
> never compiled, so `bind ^A ed-move-to-beg junk extra` succeeds.
>
> Step 9 — install, on `ntype`:
>
> - **XK_STR** (`-s`): decode the value with `parse__string` into a second
>   1024-`wchar_t` stack buffer, giving `out`. On NULL print
>   `"%ls: Invalid \\ or ^ in outstring.\n"` and return -1. Then, with
>   `-k`, `terminal_set_arrow(el, in, keymacro_map_str(el, out), XK_STR)`;
>   without `-k`, `keymacro_add(el, in, keymacro_map_str(el, out),
>   XK_STR)`. In **both** branches finish with
>   `map[(unsigned char)in[0]] = ED_SEQUENCE_LEAD_IN`.
>
>   Two traps here. First, `keymacro_map_str` merely parks the pointer in
>   the shared scratch `el_keymacro.val`; `keymacro_add` copies the string
>   into the trie (`wcsdup`), but `terminal_set_arrow` stores the raw
>   pointer into the function-key table — so `bind -k -s up "..."` leaves
>   the function key pointing at this function's *stack* buffer, which is
>   dangling the moment it returns. Second, the unconditional keymap write
>   also fires under `-k`, where `in` is a function-key *name*: `bind -k
>   -s up "x"` sets `map['u'] = ED_SEQUENCE_LEAD_IN`, corrupting the
>   binding of the letter `u`. Both are bugs in the C. The dangling
>   pointer is UB and must not be reproduced (own the string); the `map`
>   clobber is observable and should be.
>
> - **XK_CMD** (default): resolve the value to a command number with
>   `parse_cmd(el, arg)`, a linear scan of `el_map.help[0 .. nfunc)`
>   comparing `name` with `wcscmp` and returning that entry's `func`
>   field. Names are the generated hyphenated forms (`ed-move-to-beg`,
>   `vi-paste-next`, ...) plus anything registered by `map_addfunc`;
>   matching is exact and case-sensitive. On -1 print
>   `"%ls: Invalid command `%ls'.\n"` with `argv[0]` and the offending
>   argument, and return -1. Then:
>   - with `-k`: `terminal_set_arrow(el, in, keymacro_map_cmd(el, cmd),
>     XK_CMD)`, whose result is discarded (an unknown function-key name is
>     silently ignored). The keymap is **not** touched in this branch.
>   - without `-k`, multi-character `in` (`in[1] != 0`):
>     `keymacro_add(el, in, keymacro_map_cmd(el, cmd), XK_CMD)` followed by
>     `map[(unsigned char)in[0]] = ED_SEQUENCE_LEAD_IN`, so the first
>     character of the sequence becomes a trie entry point.
>     `keymacro_add` itself rejects an empty sequence and rejects binding
>     a sequence to ED_SEQUENCE_LEAD_IN, printing to `el_errfile` and
>     returning without doing anything — the keymap write still happens.
>   - without `-k`, single-character `in`: `keymacro_clear(el, map, in)`
>     first — which deletes the one-character trie entry only if this map
>     currently has ED_SEQUENCE_LEAD_IN at that index *and the other map
>     does not* — then `map[(unsigned char)in[0]] = (el_action_t)cmd`.
>     The cast truncates modulo 256 (see
>     `[spec:libedit:sem:map.map-addfunc-fn]`).
>
> - any other `ntype`: `EL_ABORT`, i.e. `abort()`. Unreachable, since
>   `ntype` is only ever XK_CMD or XK_STR.
>
> Step 10 — return 0.
>
> Cross-cutting: every keymap index is `(unsigned char)in[0]`, so a key
> whose first wide character is above U+00FF wraps modulo 256 and edits an
> unrelated slot. There is one keymacro trie per `EditLine`, shared by the
> normal and alternate keymaps; which map a sequence is reachable from is
> decided solely by where ED_SEQUENCE_LEAD_IN was written.

> [spec:libedit:def:map.map-end-fn]
> libedit_private void map_end(EditLine *el)

> [spec:libedit:sem:map.map-end-fn]
> Releases everything `map_init` and `map_addfunc` allocated, in this
> exact order:
>
> 1. Free `el_map.alt`, then free `el_map.wordchars`, then set
>    `el_map.alt = NULL`. **`el_map.wordchars` is freed but never set to
>    NULL** — it is left dangling.
> 2. Free `el_map.key`; set `el_map.key = NULL`.
> 3. Set `el_map.emacs`, `el_map.vic`, `el_map.vii` to NULL. These were
>    borrowed pointers to static const tables; nothing is freed for them.
> 4. For each index `nf` from EL_NUM_FCNS up to (but excluding)
>    `el_map.nfunc`, free `el_map.help[nf].name` and
>    `el_map.help[nf].description`, casting away `const`. These are the
>    `wcsdup` copies made by `map_addfunc`. Indices below EL_NUM_FCNS hold
>    pointers to static wide string literals copied in from the generated
>    help table and must **not** be freed — this loop's lower bound is the
>    only thing distinguishing owned from borrowed strings in that array.
> 5. Free `el_map.help`; set it to NULL. Free `el_map.func`; set it to
>    NULL.
>
> Not reset: `el_map.nfunc` keeps its old value, `el_map.type` keeps its
> old value, and `el_map.current` is left pointing into the just-freed
> `key` (or `alt`) block. The function is therefore **not idempotent**: a
> second call double-frees `wordchars`, and if any function was ever added
> it also dereferences the now-NULL `help` in step 4.
>
> Two call sites only: `el_end`, once, just before the `EditLine` itself
> is freed (so the dangling fields are unobservable); and `map_init`'s
> failure path, where `nfunc` is still 0 (the `EditLine` was
> zero-allocated) so step 4 does not run. A Rust port expresses this as
> `Drop` on an owned map struct and the whole hazard disappears; the
> observable behaviour is nil.

> [spec:libedit:def:map.map-get-editor-fn]
> libedit_private int map_get_editor(EditLine *el, const wchar_t **editor)

> [spec:libedit:sem:map.map-get-editor-fn]
> Reports the current editing mode as a name. Backs
> `el_get(EL_EDITOR, &p)`.
>
> - If `editor` (the out-parameter, not the field) is NULL, return -1
>   without touching anything.
> - Switch on `el_map.type`: MAP_EMACS (0) stores `L"emacs"` and returns
>   0; MAP_VI (1) stores `L"vi"` and returns 0.
> - Any other value of `type` falls out of the switch: return -1 with
>   `*editor` left untouched. `type` is only ever set to those two
>   constants by `map_init_emacs`/`map_init_vi`, and a freshly allocated
>   `EditLine` has `type == 0 == MAP_EMACS`, so the fall-through is
>   unreachable in practice.
>
> The stored pointer is a static string literal owned by the library. The
> caller must not free or modify it, and it stays valid for the process
> lifetime — not merely until the next mode switch.
>
> Note the asymmetry with `map_set_editor`: `type` is the sole source of
> truth for the *name*, whereas the actual bindings live in the heap
> tables, so a caller that hand-edits bindings with `bind` still reads
> back the mode name it last selected.

> [spec:libedit:def:map.map-get-wordchars-fn]
> libedit_private int map_get_wordchars(EditLine *el, const wchar_t **wordchars)

> [spec:libedit:sem:map.map-get-wordchars-fn]
> Hands out the current word-separator set. Backs
> `el_get(EL_WORDCHARS, &p)`.
>
> - If the out-parameter `wordchars` is NULL, return -1.
> - Otherwise store `el_map.wordchars` into `*wordchars` and return 0.
>
> The pointer is *borrowed*: it is the library's own heap buffer, and the
> caller must neither free it nor keep it across anything that can
> reinstall the set (`map_set_wordchars`, `map_init_vi`,
> `map_init_emacs`, `bind -v`, `bind -e`, `el_end`), each of which frees
> the old buffer.
>
> The stored value may legitimately be NULL: between `map_init` and the
> mode initialisation that follows it the field is NULL, and
> `map_set_wordchars` leaves it NULL if its allocation failed. The
> function reports success either way, so callers get `0` with a NULL
> pointer. Whether the field can be NULL when a caller can observe it is
> the port's choice; the C makes no guarantee.

> [spec:libedit:def:map.map-init-emacs-fn]
> libedit_private void map_init_emacs(EditLine *el)

> [spec:libedit:sem:map.map-init-emacs-fn]
> Resets the editor to emacs mode, discarding all runtime binding edits.
> Reached from `map_init` (only when VIDEFAULT is undefined — it is
> defined in this tree, so not by default), from
> `el_set(EL_EDITOR, "emacs")`, and from `bind -e`.
>
> Steps, in order:
>
> 1. `el_map.type = MAP_EMACS` (0).
> 2. `el_map.current = el_map.key`. `current` therefore points at the
>    heap normal map, never at the static `el_map.emacs` table — see
>    `[spec:libedit:sem:map.map-init-fn]` for why that matters.
> 3. `keymacro_reset(el)` — destroy the entire multi-character keymacro
>    trie, freeing every node and every XK_STR payload, leaving it empty.
>    Every macro added by `bind`, by the arrow-key setup, or by a previous
>    mode init is gone.
> 4. For `i` in `0 .. N_KEYS` (256): `key[i] = emacs[i]` and
>    `alt[i] = ED_UNASSIGNED`. The alternate map is *blanked*, not filled
>    from a table — emacs mode has no second map. `emacs` here is
>    `el_map.emacs`, i.e. the static const default table; if
>    `map_init_emacs` is called after `map_end` it dereferences NULL.
> 5. `map_init_meta(el)` then `map_init_nls(el)`, in that order — see
>    those rules. For the shipped emacs table this converts 34 meta
>    bindings in `key[128..255]` into ESC-prefixed two-character keymacros
>    and sets `key[27]` (ESC) to ED_SEQUENCE_LEAD_IN; `map_init_nls` then
>    overwrites the printable part of `key[128..255]` with ED_INSERT. That
>    printable part begins at 160: the C1 controls 128..159 are not
>    `iswprint` in the C, Latin-1 or UTF-8 locales, so `map_init_nls`
>    never touches them. Of the 34 converted bindings, the 31 at
>    160..255 therefore survive only as the ESC sequences, while the three
>    in the C1 range — `key[136]` ED_DELETE_PREV_WORD, `key[140]`
>    ED_CLEAR_SCREEN and `key[159]` EM_COPY_PREV_WORD — keep their direct
>    8-bit binding as well.
> 6. Add the two-character macro ESC-independent `^X ^X` → EM_EXCHANGE_MARK
>    as XK_CMD: `buf = { CONTROL('X'), CONTROL('X'), 0 }` where
>    `CONTROL(c) == c & 037`, i.e. the sequence 0x18 0x18. The lead-in
>    marker for `^X` is not written here — the static emacs table already
>    has ED_SEQUENCE_LEAD_IN at index 24.
> 7. `tty_bind_char(el, 1)` — force-rebind the tty special characters
>    (interrupt, erase, kill, ...) over the freshly copied tables,
>    restoring defaults at the old positions and installing the editor
>    actions at the current ones. With `dalt == NULL` in emacs mode it
>    touches only `key`.
> 8. `terminal_bind_arrow(el)` — reinstall the arrow/home/end/delete
>    function keys as keymacros over the now-empty trie.
> 9. Free `el_map.wordchars` and replace it with `wcsdup(L"*?_-.[]~=")`.
>    Unchecked: OOM leaves it NULL.
>
> Returns nothing and cannot fail; allocation failures are swallowed.

> [spec:libedit:def:map.map-init-fn]
> libedit_private int map_init(EditLine *el)

> [spec:libedit:sem:map.map-init-fn]
> Allocates the per-`EditLine` keymap state and installs the default
> editing mode. Called once from `el_init_internal`, after
> `keymacro_init` and before `tty_init`; its return value is **discarded**
> there, so an out-of-memory `EditLine` survives with a half-torn map.
>
> **The five fields and how they relate** (this invariant is relied on by
> `chared.c`, `read.c`, `refresh.c`, `search.c`, `terminal.c`, `tty.c` and
> `vi.c`, and getting it wrong changes observable behaviour):
>
> - `el_map.emacs`, `el_map.vic`, `el_map.vii` are `const el_action_t *`
>   pointers to three **static, immutable, program-lifetime** 256-entry
>   tables — the default emacs map, the default vi *command* map and the
>   default vi *insert* map. They are borrowed, never written through,
>   never freed, and only ever read as the source for a copy or as the
>   "what was the default here" reference in `tty_bind_char` and
>   `terminal_bind_arrow`.
> - `el_map.key` and `el_map.alt` are `el_action_t *` pointers to two
>   **heap** 256-entry arrays owned by the map and freed by `map_end`.
>   `key` is the normal map, `alt` the alternate one. These are the only
>   mutable keymaps; `bind`, `tty_bind_char` and `terminal_bind_arrow`
>   write here.
> - `el_map.current` is the map the reader dispatches through
>   (`cmd = el_map.current[ch]`). It is **only ever assigned
>   `el_map.key` or `el_map.alt`** — by `map_init_vi`, `map_init_emacs`,
>   `ch_reset`, `ch_init`, `cv_delfini`, `ed_command`, and the vi
>   insert/command-mode functions. It is **never** assigned
>   `el_map.emacs`, and cannot be: that is a pointer to static const data
>   while `key`/`alt` are heap blocks.
> - `el_map.type` is the mode tag, MAP_EMACS (0) or MAP_VI (1). It says
>   which *mode* is selected. It is independent of `current`, which says
>   which *sub-map within vi* is live. In emacs mode `current` is always
>   `key` (`alt` is all ED_UNASSIGNED). In vi mode `current == key` means
>   insert mode and `current == alt` means command mode.
>
> Consequences the port must preserve:
>
> - `current == alt` is the correct and only test for "vi command mode",
>   and it is used that way in `refresh.c` and `search.c`.
> - `current != el_map.emacs` in `c_delafter` and `c_delbefore`
>   (`chared.c`) is a **tautology** — it is always true once `map_init`
>   has run, because a heap pointer never equals a static-table pointer.
>   The guarded `cv_undo` + `cv_yank` therefore run on *every* delete,
>   including in emacs mode. This is a bug (the author plainly meant
>   `type != MAP_EMACS` or `current == alt`), but it is observable across
>   the frozen C ABI: the port must keep those calls unconditional in
>   those two functions rather than "fixing" them into a mode test.
> - Nothing outside `map.c` may treat `emacs`/`vic`/`vii` as writable.
>
> **Generated data.** `EL_NUM_FCNS`, `el_func[]` and `el_func_help[]` are
> not in any checked-in header; `src/makelist` produces them at build time
> by scanning the `/* name(): */` doc comments and the derived prototype
> headers of `vi.c`, `emacs.c` and `common.c`:
>
> - `-h` emits `vi.h`/`emacs.h`/`common.h`: one prototype per doc comment
>   whose function name starts `vi`, `em` or `ed`.
> - `-fh` emits `fcns.h`: those names **sorted**, upper-cased, as
>   `#define NAME n` for n = 0.., plus `#define EL_NUM_FCNS count`.
>   In this tree that is 96 commands, numbered 0..95, e.g.
>   ED_INSERT = 9, ED_SEQUENCE_LEAD_IN = 25, ED_UNASSIGNED = 28,
>   EM_META_NEXT = 42.
> - `-fc` emits `func.h`: `static const el_func_t el_func[]`, the function
>   pointers in the **same sorted order**, so `el_func[N]` is the handler
>   for command number `N`. That correspondence is the whole numbering
>   scheme and must be preserved by whatever the port uses instead.
> - `-bh` emits `help.h`: `static const struct el_bindings_t
>   el_func_help[]`, in **source order** (all of `vi.c`, then `emacs.c`,
>   then `common.c`), each entry `{ FCN_MACRO, L"hyphen-ated-name",
>   L"first line of the doc comment" }`. Because this table is *not*
>   sorted, its index is unrelated to the command number — every lookup
>   must compare the `func` field, never index by it.
>
> The port may generate these from the same doc comments in a build
> script or hand-write them, but the command numbers, the `el_func`
> ordering and the help-table *source* ordering are all part of what a C
> caller can observe (through `bind -l` output ordering and through
> `EL_ADDFN` numbering), so they must match.
>
> The three keymap tables themselves (`el_map_emacs`, `el_map_vi_insert`,
> `el_map_vi_command`) are literal 256-entry arrays in `src/map.c` and
> must be transcribed verbatim. Facts other rules depend on:
> `el_map_emacs[24] == ED_SEQUENCE_LEAD_IN` (`^X`),
> `el_map_emacs[27] == EM_META_NEXT` (ESC), and 34 of
> `el_map_emacs[128..255]` are real meta bindings; `el_map_vi_insert` has
> no EM_META_NEXT and no ED_SEQUENCE_LEAD_IN anywhere, `[27] ==
> VI_COMMAND_MODE`, and all of `[128..255]` are ED_INSERT;
> `el_map_vi_command[27] == EM_META_NEXT` and `[128..255]` is 126 ×
> ED_UNASSIGNED plus 2 × ED_SEQUENCE_LEAD_IN. Transcription warning: the
> numeric index comments in `el_map_emacs` are wrong from `M-_` onward —
> two consecutive entries are both labelled 223 and the labels lag by one
> thereafter, ending with `254` on the actual index 255. The character
> comments (`M-_`, `M-\``, ... `M-^?`) are correct; trust those and the
> element count, not the numbers. `el_map_vi_insert` also carries a
> `#ifdef KSHVI` alternative for indices 0..31; `KSHVI` is unconditionally
> defined in `el.h`, so the KSHVI branch is the live one and the `#else`
> block is dead code that is not ported.
>
> Steps:
>
> 1. Under `MAP_DEBUG` (not defined in any shipped build) assert each of
>    the three static tables is exactly `N_KEYS * sizeof(el_action_t)`
>    bytes, aborting otherwise. Not ported; in Rust the array length is
>    the type.
> 2. `el_map.alt = calloc(N_KEYS, sizeof(el_action_t))`. On failure return
>    -1 immediately (nothing to unwind).
> 3. `el_map.key = calloc(N_KEYS, sizeof(el_action_t))`. On failure jump
>    to the cleanup path.
> 4. Point `el_map.emacs`, `el_map.vic`, `el_map.vii` at the three static
>    tables (emacs, vi command, vi insert respectively).
> 5. `el_map.help = calloc(EL_NUM_FCNS, sizeof(el_bindings_t))`; on
>    failure, cleanup path. Then `memcpy` the whole generated
>    `el_func_help[]` over it — a **shallow** copy, so the `name` and
>    `description` pointers in entries 0..EL_NUM_FCNS-1 alias static
>    string literals and are never freed (see
>    `[spec:libedit:sem:map.map-end-fn]`). The copy length is
>    `EL_NUM_FCNS` entries, which is correct only because the help table
>    and the numbering come from the same scan; a doc comment missing its
>    description line would make the generated help table shorter and this
>    `memcpy` would read out of bounds.
> 6. `el_map.func = calloc(EL_NUM_FCNS, sizeof(el_func_t))`; on failure,
>    cleanup path. `memcpy` the generated `el_func[]` over it.
> 7. `el_map.nfunc = EL_NUM_FCNS`; `el_map.wordchars = NULL`.
> 8. Install the default mode: `map_init_vi(el)` if VIDEFAULT is defined,
>    else `map_init_emacs(el)`. **`el.h` defines VIDEFAULT
>    unconditionally, so the shipped default editing mode is vi insert
>    mode**, matching `editline(7)`. That call is what first sets
>    `el_map.type` and `el_map.current`; `map_init` never sets them
>    itself. Programs that want emacs call `el_set(EL_EDITOR, "emacs")`
>    afterwards, as the readline compatibility layer does.
> 9. Return 0.
>
> Cleanup path: call `map_end(el)` and return -1. At that point `nfunc` is
> still 0 (the `EditLine` was zero-allocated), so `map_end`'s user-function
> free loop does not run over the possibly-NULL `help` array.

> [spec:libedit:def:map.map-init-meta-fn]
> static void map_init_meta(EditLine *el)

> [spec:libedit:sem:map.map-init-meta-fn]
> Converts the 8-bit "meta" half of a freshly copied keymap into
> two-character ESC-prefixed keymacros, so that terminals that send
> ESC-x rather than setting the high bit get the same commands. Called by
> `map_init_vi` and `map_init_emacs` immediately after the table copy and
> immediately before `map_init_nls`.
>
> 1. Choose which map to work on and which character acts as the meta
>    prefix. Scan `el_map.key[0 ..= 0377]` for the first index whose
>    action is EM_META_NEXT.
>    - Found at index `i`: work on `key`, prefix character `i`.
>    - Not found: scan `el_map.alt[0 ..= 0377]` the same way.
>      - Found at index `i`: work on `alt`, prefix character `i`.
>      - Not found either: prefix character is 033 (ESC), and the map
>        worked on is `alt` if `el_map.type == MAP_VI`, otherwise `key`.
>    (The C expresses "not found" as the loop counter reaching 0400; the
>    guard is `i > 0377`, so an EM_META_NEXT at index 255 counts as
>    found.)
> 2. Build a three-element buffer `buf` with `buf[0] = prefix` and
>    `buf[2] = 0`.
> 3. For each `i` in `0200 ..= 0377` (128..=255) look at the chosen map's
>    entry. If it is ED_INSERT, ED_UNASSIGNED or ED_SEQUENCE_LEAD_IN,
>    skip it. Otherwise set `buf[1] = i & 0177` (strip the high bit) and
>    add the two-character sequence `buf` to the keymacro trie as an
>    XK_CMD binding for that action, via
>    `keymacro_add(el, buf, keymacro_map_cmd(el, map[i]), XK_CMD)`.
>    `keymacro_map_cmd` only parks the command number in the shared
>    scratch `el_keymacro.val`, which `keymacro_add` copies by value into
>    the trie node — nothing is owned or leaked here.
> 4. Finally set `map[prefix] = ED_SEQUENCE_LEAD_IN` on the chosen map,
>    turning the prefix character itself into a trie entry point. This
>    happens unconditionally, even when step 3 added nothing.
>
> Concretely for the shipped tables. In **emacs** mode EM_META_NEXT is at
> index 27 of the key map, so the chosen map is `key`, the prefix is ESC,
> 34 ESC-`x` macros are created, and `key[27]` becomes
> ED_SEQUENCE_LEAD_IN (replacing EM_META_NEXT). In **vi** mode the insert
> map has no EM_META_NEXT at all, the command map has it at index 27, so
> the chosen map is `alt`: no macros are created (the command map's high
> half is entirely ED_UNASSIGNED and ED_SEQUENCE_LEAD_IN) and the sole
> effect is `alt[27] = ED_SEQUENCE_LEAD_IN`, so ESC in vi command mode
> becomes a lead-in rather than a meta prefix. The vi *insert* map's
> `key[27]` keeps VI_COMMAND_MODE and is untouched.
>
> `buf[1]` is only written inside the loop, so when no entry qualifies it
> is never initialised — harmless only because `keymacro_add` is likewise
> never called in that case.

> [spec:libedit:def:map.map-init-nls-fn]
> static void map_init_nls(EditLine *el)

> [spec:libedit:sem:map.map-init-nls-fn]
> Makes the printable characters of the upper half of the 8-bit range
> self-inserting, so 8-bit locales type through. Called by `map_init_vi`
> and `map_init_emacs` immediately after `map_init_meta`.
>
> For each `i` in `0200 ..= 0377` (128..=255): if `iswprint(i)` is true,
> set `el_map.key[i] = ED_INSERT`.
>
> Only the normal map is touched; `el_map.alt` is never modified here,
> whatever the mode. The test is `iswprint` in the process's current
> LC_CTYPE locale, evaluated on the *integer* 128..255 taken as a wide
> character (U+0080..U+00FF), not on a byte in some multibyte encoding —
> so in the `C` locale nothing changes, while in a Latin-1 or UTF-8 locale
> the printable subset of U+00A0..U+00FF becomes ED_INSERT.
>
> The ordering matters: because this runs *after* `map_init_meta`, in
> emacs mode it overwrites those direct 8-bit meta bindings the default
> table supplied that sit at a printable index — but `map_init_meta` has
> already mirrored each of them into an ESC-prefixed keymacro, so the
> commands remain reachable. The bindings at 128..159 are left alone,
> because the C1 controls are not `iswprint` in the C, Latin-1 or UTF-8
> locales, so those keep their direct 8-bit form on top of the ESC form.
> In vi mode the insert map's high half is already all ED_INSERT, so this
> is a no-op there.
>
> Locale-dependence means the resulting keymap is not deterministic across
> environments; the port must query the same locale-sensitive predicate
> rather than hard-coding a range.

> [spec:libedit:def:map.map-init-vi-fn]
> libedit_private void map_init_vi(EditLine *el)

> [spec:libedit:sem:map.map-init-vi-fn]
> Resets the editor to vi mode, discarding all runtime binding edits.
> Reached from `map_init` (the default in this tree, since `el.h` defines
> VIDEFAULT), from `el_set(EL_EDITOR, "vi")`, and from `bind -v`.
>
> Steps, in order:
>
> 1. `el_map.type = MAP_VI` (1).
> 2. `el_map.current = el_map.key` — i.e. start in vi **insert** mode.
>    `current` points at the heap normal map; see
>    `[spec:libedit:sem:map.map-init-fn]` for the full invariant. From
>    here on, `vi_command_mode` sets `current = alt` and the various
>    insert-entering commands set it back to `key`.
> 3. `keymacro_reset(el)` — destroy the entire multi-character keymacro
>    trie, freeing every node and every XK_STR payload. All user macros,
>    arrow-key macros and meta macros are gone.
> 4. For `i` in `0 .. N_KEYS` (256): `key[i] = vii[i]` (vi insert
>    defaults) and `alt[i] = vic[i]` (vi command defaults), reading from
>    the borrowed static tables `el_map.vii` and `el_map.vic`. Unlike
>    emacs mode, both maps are filled from tables. Calling this after
>    `map_end` dereferences NULL.
> 5. `map_init_meta(el)` then `map_init_nls(el)` — see those rules. For
>    the shipped vi tables the net effect is only `alt[27] =
>    ED_SEQUENCE_LEAD_IN`; no ESC macros are created, and `map_init_nls`
>    is a no-op because the insert map's high half is already ED_INSERT.
> 6. `tty_bind_char(el, 1)` — force-rebind the tty special characters over
>    both maps (in vi mode `tty_bind_char` has both a default map and a
>    default alt map, so it restores and rebinds in `key` *and* `alt`).
> 7. `terminal_bind_arrow(el)` — reinstall the arrow/home/end/delete
>    function keys over the now-empty trie. In vi mode it targets `alt`
>    (the command map) and additionally registers the ESC-less forms of
>    the sequences.
> 8. Free `el_map.wordchars` and replace it with `wcsdup(L"_")` — vi's
>    word-constituent set is just underscore. Unchecked: OOM leaves it
>    NULL.
>
> Returns nothing and cannot fail; allocation failures are swallowed.

> [spec:libedit:def:map.map-print-all-keys-fn]
> static void map_print_all_keys(EditLine *el)

> [spec:libedit:sem:map.map-print-all-keys-fn]
> Dumps the complete binding state to `el_outfile`. Reached from `bind`
> with no key argument.
>
> 1. Print `"Standard key bindings\n"`. Then walk the normal map coalescing
>    runs of identical actions: with `prev = 0`, for `i` in `0 .. N_KEYS`,
>    skip while `key[prev] == key[i]`; on the first `i` that differs, call
>    `map_print_some_keys(el, key, prev, i - 1)` and set `prev = i`. After
>    the loop, call it once more for `(prev, N_KEYS - 1)` to flush the
>    final run. (`i == prev` always compares equal, so the run always has
>    at least one element and `i - 1 >= prev`.)
> 2. Print `"Alternative key bindings\n"` and repeat the identical walk
>    over `el_map.alt`. In emacs mode `alt` is uniformly ED_UNASSIGNED, so
>    this yields a single run 0..255 which `map_print_some_keys` prints
>    nothing for — the heading appears with no entries under it.
> 3. Print `"Multi-character bindings\n"` and call
>    `keymacro_print(el, L"")`, which walks the whole keymacro trie (and
>    prints nothing at all when the trie is empty and the key is empty).
> 4. Print `"Arrow key bindings\n"` and call
>    `terminal_print_arrow(el, L"")`, which prints every function key
>    whose type is not XK_NOD.
>
> All four headings are always printed, even when the corresponding
> section is empty. The order of the four sections and the exact heading
> strings are observable output.

> [spec:libedit:def:map.map-print-key-fn]
> static void map_print_key(EditLine *el, el_action_t *map, const wchar_t *in)

> [spec:libedit:sem:map.map-print-key-fn]
> Prints the binding of one key, for `bind <key>` with no value argument.
> `map` is whichever of the normal or alternate map the caller selected.
>
> If `in` is empty or exactly one character long (`in[0] == '\0' ||
> in[1] == '\0'`, short-circuiting so the second read never happens on an
> empty string), it is a single-character binding:
>
> 1. Render `in` into a byte buffer of EL_BUFSIZ (1024) bytes with
>    `keymacro__decode_str(in, buf, sizeof buf, "")` — the empty separator
>    means no surrounding quotes. That helper renders each wide character
>    visually (`^A`, `\033`, ...), renders the empty string as `^@`, and
>    truncates rather than overflowing.
> 2. Scan `el_map.help[0 .. nfunc)` for the first entry whose `func`
>    equals `map[(unsigned char)in[0]]`. On a hit, print
>    `"%s\t->\t%ls\n"` with the rendered key then the entry's `name`, to
>    `el_outfile`, and return.
> 3. If no entry matches, print **nothing** and return silently — unlike
>    `map_print_some_keys`, which aborts in the same situation.
>
> An empty `in` looks up `map[0]`, so `bind ""` reports the binding of NUL
> rendered as `^@`.
>
> Otherwise (two or more characters) delegate to `keymacro_print(el, in)`,
> which walks the trie from that prefix and, if the sequence is unbound,
> prints `Unbound extended key "..."` to `el_errfile`.
>
> Note the key is looked up in the keymap by its first character cast to
> `unsigned char`, so a wide character above U+00FF wraps modulo 256.

> [spec:libedit:def:map.map-print-some-keys-fn]
> static void map_print_some_keys(EditLine *el, el_action_t *map, wint_t first, wint_t last)

> [spec:libedit:sem:map.map-print-some-keys-fn]
> Prints one run of consecutive key codes `first ..= last` that all share
> the same action. Only called from `map_print_all_keys`, which guarantees
> `first <= last`, both within 0..255, and `map[first] == map[last]`.
>
> 1. Build one-character strings `firstbuf = { first, 0 }` and
>    `lastbuf = { last, 0 }`.
> 2. If `map[first] == ED_UNASSIGNED`: when the run is a single key
>    (`first == last`), render `firstbuf` with
>    `keymacro__decode_str(..., STRQQ)` — STRQQ is the two-character
>    separator `"\"\""`, so the rendering is wrapped in double quotes —
>    and print `"%-15s->  is undefined\n"` to `el_outfile`. When the run
>    is longer, print nothing. Return either way. Unassigned *ranges* are
>    thus invisible in `bind` output while unassigned single keys are
>    reported.
> 3. Otherwise scan `el_map.help[0 .. nfunc)` for the first entry whose
>    `func` equals `map[first]`. On a hit:
>    - single key: print `"%-15s->  %ls\n"` with the quoted rendering of
>      `first` and the entry's `name`;
>    - a range: render both ends and print `"%-4s to %-7s->  %ls\n"` with
>      the quoted rendering of `first`, of `last`, and the `name`.
>    Return.
> 4. If the scan finds nothing — a keymap slot holding an action number
>    that no help entry claims — fall through to `EL_ABORT`, which is
>    plain `abort()`. (A `MAP_DEBUG` block prints diagnostics first; it is
>    not compiled in shipped builds.) This is reachable in principle:
>    `map_bind` writes command numbers through an `unsigned char` cast, so
>    a function registered with `map_addfunc` past number 255 truncates to
>    a number that may not correspond to any help entry, and the next
>    `bind` with no arguments aborts the process. The port must not
>    literally abort; treat it as an internal invariant violation, but
>    note that means diverging from the C in a case the C itself calls a
>    bug.
>
> Two rendering buffers of EL_BUFSIZ bytes each are used; the helper
> truncates rather than overflowing. The `%-15s`, `%-4s`, `%-7s` field
> widths count bytes of the rendered form and are observable output.

> [spec:libedit:def:map.map-set-editor-fn]
> libedit_private int map_set_editor(EditLine *el, wchar_t *editor)

> [spec:libedit:sem:map.map-set-editor-fn]
> Selects an editing mode by name. Backs `el_set(EL_EDITOR, name)`.
>
> - If `editor` compares equal to `L"emacs"` under `wcscmp`, call
>   `map_init_emacs(el)` and return 0.
> - Else if it compares equal to `L"vi"`, call `map_init_vi(el)` and
>   return 0.
> - Otherwise return -1, having changed nothing.
>
> Matching is exact and case-sensitive; there are no aliases, no
> abbreviations, and no third mode. A NULL argument is undefined
> behaviour (`wcscmp` on NULL); the port should reject it.
>
> Because this delegates to the mode initialisers, a successful call is
> destructive: it re-copies both keymaps from the static defaults, wipes
> the entire keymacro trie, re-runs the meta/NLS/tty/arrow setup and
> replaces `el_map.wordchars`. Calling `el_set(EL_EDITOR, "vi")` while
> already in vi mode is not a no-op — it discards every binding the
> program or the user's `editrc` established.

> [spec:libedit:def:map.map-set-wordchars-fn]
> libedit_private int map_set_wordchars(EditLine *el, wchar_t *wordchars)

> [spec:libedit:sem:map.map-set-wordchars-fn]
> Replaces the set of characters that count as word constituents (used by
> the word-motion and word-deletion commands). Backs
> `el_set(EL_WORDCHARS, s)`.
>
> 1. Free the current `el_map.wordchars`.
> 2. Set `el_map.wordchars = wcsdup(wordchars)` — an owned heap copy; the
>    caller's string is not retained.
> 3. Return 0, always.
>
> The duplication is unchecked, so on allocation failure the field is left
> NULL and the function still reports success. There is no way for a
> caller to detect that.
>
> A NULL argument is undefined behaviour (`wcsdup` of NULL). So is passing
> the library's own buffer back in — `map_get_wordchars` hands out a
> borrowed pointer to exactly this field, and step 1 frees it before step
> 2 reads it, so `el_set(EL_WORDCHARS, p)` with the `p` obtained from
> `el_get(EL_WORDCHARS, &p)` is a use-after-free. The port must copy
> before freeing (or own the string such that the aliasing cannot arise).
>
> The value set here survives until the next call, the next mode switch
> (`map_init_vi`/`map_init_emacs`, including via `bind -v`/`bind -e`,
> each of which installs its own mode default) or `map_end`. Mode
> defaults are `L"_"` for vi and `L"*?_-.[]~="` for emacs.
