# src/parse.c

> [spec:libedit:def:parse.el-wparse-fn]
> int el_wparse(EditLine *el, int argc, const wchar_t *argv[])

> [spec:libedit:sem:parse.el-wparse-fn]
> The dispatcher for the editrc command language. Takes an already
> tokenised command line and runs the matching built-in. This is the
> public `el_wparse` declared in `histedit.h`; `parse_line` reaches it
> after tokenising a line, and `el_parse` (eln.c) reaches it after
> widening a `char **` argv.
>
> 1. If `argc < 1`, return -1 at once. `argv` is not touched on this
>    path, so it is the only one on which `argv == NULL` is safe.
> 2. `ptr = wcschr(argv[0], L':')` — find the **first** colon in the
>    command word. Text before that colon is a *program-name qualifier*,
>    the mechanism that lets one editrc file carry lines for several
>    programs.
> 3. No colon: set `ptr = argv[0]` and go to step 5.
> 4. Colon present:
>    a. If the colon is the first character of `argv[0]` (empty
>       qualifier, e.g. `:bind`), return **0** — the line is silently
>       ignored and this is not reported as an error.
>    b. Copy the `l = ptr - argv[0]` characters before the colon into a
>       fresh `el_calloc(l + 1, sizeof(wchar_t))` buffer and NUL
>       terminate it. If that allocation fails, return **0** — an
>       out-of-memory here is reported as "this line is not for us",
>       not as an error.
>    c. Advance `ptr` one past the colon; the remainder is the command
>       word to look up. Only the first colon is split on, so
>       `foo:bar:bind` yields qualifier `foo` and command word
>       `bar:bind`, which matches no command and falls out at step 7.
>    d. Test the qualifier with `el_match(el->el_prog, tprog)`. Note the
>       argument order: the *program name* passed to `el_init` is the
>       subject and the qualifier is the pattern. `el_match` is true if
>       the qualifier occurs anywhere in the program name as a plain
>       substring (`wcsstr`), and failing that if it matches the program
>       name as an unanchored POSIX basic regular expression
>       (`regcomp`/`regexec` with no flags — see
>       [dec:libedit:posix-only-scope], which fixes REGEX as the only
>       live branch). The qualifier is therefore a **substring/regex
>       test, not an equality test**: a line beginning `sh:` applies to
>       a program named `bash`, and a qualifier of `.` applies to every
>       program.
>    e. Free the qualifier copy. If the match failed, return **0** — the
>       line belongs to another program and is skipped silently.
> 5. Linear-scan the static command table (see
>    `[spec:libedit:sem:parse.func-fn]`) from index 0, comparing `ptr`
>    against each entry's name with `wcscmp` — exact and case sensitive,
>    no abbreviation and no locale folding.
> 6. On the first name that compares equal, call that entry's handler as
>    `func(el, argc, argv)`, passing the **original, unmodified** `argc`
>    and `argv` — `argv[0]` still carries the `prog:` prefix, because the
>    qualifier was stripped only into a temporary for the match — and
>    return the **negation** of what the handler returned. Handlers
>    return 0 for success and -1 for failure, so this yields 0 for
>    success and +1 for a handler-reported failure.
> 7. If the scan reaches the `{ NULL, NULL }` sentinel without a match,
>    return -1.
>
> Return-value summary, and it is lossy in both directions:
>
> | value | meanings |
> |-------|----------|
> | 0     | empty qualifier; qualifier did not match this program; the qualifier copy could not be allocated; **or** the command ran and succeeded |
> | +1    | the command ran and reported failure (handler returned -1) |
> | -1    | `argc < 1` (empty line) **or** unknown command name |
>
> The caller cannot tell those apart. It matters because `el_source`
> stops reading the editrc file the moment `parse_line` returns -1 and
> keeps going otherwise: a *failing* command (`settc` with a bad
> capability, +1) does not abort sourcing, but an *unknown* or empty one
> does. `ed_command` (the vi `:` prompt) beeps on -1 only, likewise.
>
> `argc` is forwarded unchanged and `argv` is required to be
> NULL-terminated at `argv[argc]`; several handlers ignore `argc`
> entirely and walk to the NULL themselves, so a C caller that passes an
> exact `argc` over an unterminated array will run off the end. Under
> [dec:libedit:no-c-ffi] this function is part of the exported C ABI, so
> that convention is frozen and the Rust core must accept a
> NULL-terminated argv rather than a plain slice at the boundary.

> [spec:libedit:def:parse.func-fn]
> int (*func)(EditLine *, int, const wchar_t **)

> [spec:libedit:sem:parse.func-fn]
> The handler-pointer member of the file-static dispatch table `cmds[]`,
> an array of `{ const wchar_t *name; int (*func)(EditLine *, int, const
> wchar_t **); }` terminated by a `{ NULL, NULL }` sentinel. It is the
> whole of the editrc command vocabulary; `[spec:libedit:sem:parse.el-wparse-fn]`
> is its only reader.
>
> | `name`    | handler           | command |
> |-----------|-------------------|---------|
> | `bind`    | `map_bind`        | add/remove/show key bindings |
> | `echotc`  | `terminal_echotc` | query or emit a terminal capability |
> | `edit`    | `el_editmode`     | `on`/`off` — enable or disable line editing |
> | `history` | `hist_command`    | `list`, `size N`, `unique N` |
> | `telltc`  | `terminal_telltc` | dump the terminal's characteristics |
> | `settc`   | `terminal_settc`  | override a terminal capability |
> | `setty`   | `tty_stty`        | inspect or set tty modes |
>
> Seven commands, all names distinct, looked up by exact `wcscmp`, so
> table order is irrelevant to behaviour; only the sentinel is
> load-bearing, as it ends the scan. The comment block at the head of
> parse.c lists `gettc` and omits `telltc`; that comment is wrong —
> there is no `gettc` command and never was in this table.
>
> The contract every entry obeys:
>
> - **Call shape.** `func(el, argc, argv)` receives exactly the `argc`
>   and `argv` that `el_wparse` received. `argv[0]` is the command word
>   **as written, including any `prog:` qualifier** — `el_wparse` strips
>   the qualifier only into a temporary for its own name match, never in
>   `argv`. Handlers that quote `argv[0]` in diagnostics therefore print
>   the qualified form, e.g. `myprog:settc: Bad capability 'foo'.`, and
>   `tty_stty` adopts `argv[0]` verbatim as the program name it prefixes
>   all its messages with.
> - **Termination.** `argv` must be NULL-terminated at `argv[argc]`.
> - **Return.** 0 on success, -1 on failure. `el_wparse` negates the
>   result, so a handler's -1 surfaces to `parse_line` as +1.
>   `hist_command`'s `size`/`unique` paths return `history_w(...)`
>   directly, which is also 0/-1, so the convention holds throughout.
> - **Diagnostics are the handler's job.** Handlers write their own
>   messages to `el->el_errfile` (or `el->el_outfile` for the listing
>   forms); `el_wparse` prints nothing.
> - **`argc` is advisory for most of them.** `map_bind` reassigns its
>   own `argc` parameter to 1 and walks `argv` to the NULL, ignoring the
>   value passed in; `terminal_echotc`, `terminal_telltc`,
>   `terminal_settc` and `tty_stty` declare the parameter unused and
>   likewise walk `argv`. Only two consult it, and strictly:
>   `el_editmode` rejects anything but `argc == 2` (so `edit on extra`
>   fails), and `hist_command` treats `argc == 1` as an implicit `list`
>   and requires exactly `argc == 3` for `size`/`unique`.
> - **NULL `argv` is defended against, individually.** `map_bind`,
>   `el_editmode`, `terminal_echotc`, `terminal_settc` and `tty_stty`
>   each test `argv == NULL` (and, where they need them, `argv[1]` /
>   `argv[2] == NULL`) and return -1. `terminal_telltc` ignores `argv`
>   completely. `hist_command` does not test `argv` at all — it guards
>   only on `el->el_history.ref == NULL` — so it relies on `argc` being
>   truthful about `argv[1]` and `argv[2]`.
> - **Handlers may reconfigure the editor mid-line and stop early.**
>   `bind -v` and `bind -e` reinitialise the whole keymap and return 0
>   without looking at the rest of the arguments.
>
> A port should model this as a table of `(name, handler)` where the
> handler is a plain function over `(&mut EditLine, argc, argv)` with
> the argv aliasing and NULL-termination rules above preserved, because
> they are visible to C callers of `el_parse`/`el_wparse`
> ([dec:libedit:no-c-ffi]).

> [spec:libedit:def:parse.parse-cmd-fn]
> libedit_private int parse_cmd(EditLine *el, const wchar_t *cmd)

> [spec:libedit:sem:parse.parse-cmd-fn]
> Maps an editor-command **name** as written in a `bind` line
> (`ed-insert`, `vi-next-word`, `em-kill-line`, ...) to its numeric
> command index.
>
> 1. Let `b = el->el_map.help` and `n = el->el_map.nfunc`.
> 2. For `i` in `0 .. n-1`, in order, compare `b[i].name` against `cmd`
>    with `wcscmp` — exact, case sensitive, no prefix matching, no
>    abbreviation, no locale folding.
> 3. On the first entry that compares equal, return `b[i].func`.
> 4. If none matches, return -1.
>
> `el->el_map.help` is the editor's command help table: `map_init`
> allocates `EL_NUM_FCNS` entries and copies the generated
> `el_func_help` table into them, setting `el_map.nfunc = EL_NUM_FCNS`.
> `el_wset(EL_ADDFN)` → `map_addfunc` grows both `el_map.help` and
> `el_map.func` by one and appends
> `{ .func = (int)old_nfunc, .name = wcsdup(name), .description = wcsdup(help) }`,
> then increments `nfunc`. So user-registered commands are found by this
> same search, and the number returned for them is their index into
> `el->el_map.func`. For the built-ins the number returned is the
> `ED_*`/`EM_*`/`VI_*` constant, which is likewise the index into
> `el->el_map.func`. The returned value is therefore always a *command
> number*, never a function pointer, and the caller resolves it through
> `el_map.func` or stores it directly in a keymap slot.
>
> The search is first-match by name and `map_addfunc` performs no
> uniqueness check, so registering a function under a name a built-in
> already uses leaves the new function unreachable from `bind`: the
> built-in is always found first.
>
> `map_addfunc` also does not check its two `wcsdup` results, so under
> memory pressure a `help[i].name` can be NULL and this loop will pass
> NULL to `wcscmp` — undefined behaviour. A port should either refuse
> the registration or store the name as an owned string that cannot be
> absent.
>
> -1 is the only error signal and is unambiguous, since no valid command
> number is negative. The sole caller is `map_bind`'s `XK_CMD` path,
> which on -1 prints `"%ls: Invalid command `%ls'.\n"` and fails the
> whole `bind` line.

> [spec:libedit:def:parse.parse-escape-fn]
> libedit_private int parse__escape(const wchar_t **ptr)

> [spec:libedit:sem:parse.parse-escape-fn]
> The escape decoder of the key-binding language. Reads **one**
> character specification from `*ptr`, returns its value as a
> non-negative `int`, and advances `*ptr` past exactly what it consumed.
> Returns -1 for a malformed escape and on that path leaves `*ptr`
> **unchanged**.
>
> Let `p = *ptr` on entry.
>
> ### The two-character rule
>
> The first statement is `if (p[1] == 0) return -1;`. This is a blanket
> "there must be at least two characters here" test applied *before*
> deciding which form is present, so it also rejects a perfectly
> ordinary literal at the end of the string: on `L"a"` this returns -1,
> while on `L"ax"` it returns `'a'` and consumes one character. A lone
> trailing `\` or `^` is rejected by the same test. This is observable
> in `setty`: `tty_stty`'s `name=value` path calls this on the text
> after the `=`, so `setty erase=X` (a bare one-character value at the
> end of the token) decodes as -1 and stores `(cc_t)-1` = 0xFF into
> `c_cc`, whereas `setty erase=^H` works.
>
> The test reads `p[1]` without first checking `p[0]`, so calling this
> on an empty string reads one element past the NUL terminator —
> undefined behaviour. Both in-tree callers guarantee `p[0] != 0`
> (`parse__string` only enters on `\` or `^`; `tty_stty` tests `*++p`
> first), so treat "at least one character" as a precondition and the
> two-character rule as the specified behaviour.
>
> ### A. Named backslash escapes
>
> `*p == '\\'`; step past the backslash and dispatch on the next
> character.
>
> | text | value | |
> |------|-------|--|
> | `\a` | 0x07 | BEL |
> | `\b` | 0x08 | BS |
> | `\t` | 0x09 | HT |
> | `\n` | 0x0A | LF |
> | `\v` | 0x0B | VT |
> | `\f` | 0x0C | FF |
> | `\r` | 0x0D | CR |
> | `\e` | 0x1B | ESC |
>
> Each consumes exactly two characters. They are **lower case only** —
> `\E` is not ESC, it falls into form D and yields `'E'`. There is no
> `\s`, no `\d`, no `\x` hex form and no `\0`-means-NUL special case
> (`\0` is form B); `\x41` is form D and yields `'x'`, leaving the
> literal characters `4` and `1` in the input.
>
> ### B. Octal escapes
>
> `\` followed by one of `0`-`7`. Accumulate `c = (c << 3) | (digit -
> '0')` over **at most three** octal digits starting at that first
> digit. The run stops at the first character that is not in `0`-`7`, or
> after the third digit, whichever comes first — `8` and `9` are not
> octal digits, so `\18` is value 1 followed by a literal `8`.
>
> Range check: if the accumulated value has any bit outside 0xFF set
> (`c & 0xffffff00`), return -1 with the cursor unchanged. Three digits
> can reach 0777 = 511, so the accepted range is `\0` .. `\377` and
> `\400` .. `\777` are rejected. There is no other overflow: the digit
> count caps the value.
>
> Cursor: exactly `1 + (digits actually consumed)` characters — the
> backslash plus the digit run and nothing past it. `\101x` → 0x41, 4
> consumed, `x` still ahead; `\10x` → 0x08, 3 consumed; `\1x` → 0x01, 2
> consumed; `\0x` → 0, 2 consumed. The value may legitimately be 0.
>
> ### C. Unicode escapes — `\U+xxxx` / `\U+xxxxx`
>
> `\` followed by `U`. Uppercase `U` only; `\u` is form D and yields
> `'u'`.
>
> 1. The character after the `U` must be `+`. If it is not, return -1,
>    cursor unchanged.
> 2. Then run **five** iterations. Each reads the next character and
>    looks it up in the literal table `L"0123456789ABCDEF"`; a hit
>    contributes `c = (c << 4) | index`. Digits are **uppercase hex
>    only** — `a`-`f` are not in the table, so `\U+00ff` is rejected
>    while `\U+00FF` is accepted.
> 3. Iterations 0-3 (the first four characters after the `+`) must all
>    be hex digits; if any is not, return -1, cursor unchanged. Four hex
>    digits are therefore mandatory: `\U+41zz` fails, it is not read as
>    two digits.
> 4. Iteration 4: if the fifth character is a hex digit it is absorbed
>    (five-digit form); if it is not, the loop steps back over it and
>    the value is the four-digit one.
> 5. If the resulting value exceeds 0x10FFFF, return -1, cursor
>    unchanged. That is the only validation — surrogates and
>    noncharacters pass, so `\U+0D800` is accepted.
>
> **Cursor — the trap.** This form always consumes **one character more
> than the escape text**. The trailing `*ptr = ++p` that the other forms
> need is applied on top of a cursor that already sits just past the last
> accepted digit. `\U+0041x` consumes 8 characters: the seven of
> `\U+0041` *plus the `x`*, which is silently discarded. The five-digit
> `\U+00041x` consumes 9 and discards the `x` the same way. There is no
> way to write a `\U+` escape followed by another character and keep
> that character. This is a defect, not a design; a port must decide
> deliberately whether to freeze it, and the sane reading is to consume
> exactly the escape text.
>
> **End of string is a buffer overrun.** `wcschr` counts the terminating
> NUL as part of its subject, so `wcschr(L"0123456789ABCDEF", L'\0')`
> returns a non-NULL pointer to the table's own terminator and the input
> NUL is accepted as a hex digit **with value 16**. Two consequences,
> both reachable from an ordinary editrc line:
>
> - Exactly four hex digits followed by end of string does **not** fail.
>   Iteration 4 reads the NUL as 16, yielding `(value << 4) | 0x10` —
>   the four-digit value shifted up one nibble with bit 4 forced on. It
>   is far below 0x10FFFF, so the range check passes, and the cursor is
>   left **two elements past the NUL terminator**. Measured:
>   `\U+0041` at end of string returns 0x410 (not 0x41) and `\U+ABCD`
>   returns 0xABCD0 (the `| 0x10` is absorbed there, because the last
>   digit `D` already has its low bit set). So
>   `bind '\U+0041' ed-insert` binds U+0410, not U+0041, and leaves
>   `parse__string` scanning past the end of its input.
> - With one to three hex digits before the end of string, the NUL is
>   consumed as a digit and the remaining iterations dereference past
>   the terminator — out-of-bounds reads whose result depends on
>   adjacent memory. `\U+` and `\U+0` are in this class; they happen to
>   return -1 in practice only because the garbage read usually pushes
>   the value over 0x10FFFF.
>
> These are undefined behaviour and cannot be meaningfully frozen by
> [dec:libedit:no-c-ffi], which pins *defined* observable behaviour. The
> defensible specification for the port: require four or five uppercase
> hex digits, treat end of string inside the run as a malformed escape
> (-1, cursor unchanged), and consume exactly the escape text.
>
> ### D. Unrecognised backslash escapes
>
> Any other character after the backslash: the value is **that character
> itself** and two characters are consumed. So `\\` → 0x5C, `\q` →
> `'q'`, `\8` and `\9` → `'8'` and `'9'`, `\-` → `'-'`. This is also how
> `\M` behaves, which defeats `parse__string`'s meta form: `\M-a`
> decodes to the three characters `M`, `-`, `a`, not to ESC `a`.
>
> ### E. Control form — `^X`
>
> `*p == '^'`; step past the caret and take the next character `x`.
>
> - `x == '?'` → 0x7F (DEL), special-cased before the mask.
> - Otherwise the value is `x & 0237` octal = `x & 0x9F`. That mask
>   **clears bits 5 and 6** (0x60) and keeps bit 7 (0x80) together with
>   bits 0-4 (0x1F).
>
> Consequences: `^A`..`^Z` and `^a`..`^z` both give 1..26, so the letter
> case is irrelevant; `^@` → 0x00, `^[` → 0x1B, `^\` → 0x1C, `^]` →
> 0x1D, `^^` → 0x1E, `^_` → 0x1F, `^` followed by a space (0x20) → 0x00.
> The mask is applied to the whole wide character with no validation and
> everything above bit 7 is discarded, so `^` U+00E9 gives 0x89 and `^`
> U+1234 gives 0x14. Exactly two characters are consumed. `^` at the end
> of the string is caught by the two-character rule and gives -1.
>
> ### F. Literal
>
> Any other leading character: the value is that wide character and one
> character is consumed — subject to the two-character rule, so it fails
> at the very end of a string.
>
> ### Return and cursor discipline
>
> - Every success path ends with `*ptr = ++p`. The per-form consumption
>   counts stated above are the observable contract; implement those,
>   including the `\U+` off-by-one if the decision is to keep bug
>   compatibility.
> - Every -1 path leaves `*ptr` untouched, so the caller's cursor does
>   not move. `parse__string` treats -1 as fatal and returns NULL;
>   `tty_stty` does not check it and stores the -1 straight into a
>   `cc_t`.
> - The value comes from a `wint_t` and is never negative on any defined
>   path, so -1 is unambiguous as the error code. **0 is a valid
>   result** (`^@`, `\0`, `\U+0000`) and must not be confused with end of
>   input.
> - The C's own header comment describes the accepted forms as
>   `^<char> \<odigit> \<char> \U+xxxx`; that is accurate as far as it
>   goes but omits the eight named escapes of form A.

> [spec:libedit:def:parse.parse-line-fn]
> libedit_private int parse_line(EditLine *el, const wchar_t *line)

> [spec:libedit:sem:parse.parse-line-fn]
> Tokenises one editrc line (or one line typed at the `ed_command`
> prompt) and dispatches it.
>
> 1. `tok = tok_winit(NULL)` — a fresh wide tokenizer with the default
>    IFS `L"\t \n"`. All of the sh-like lexing (whitespace-separated
>    words, single and double quotes, backslash escapes,
>    backslash-newline continuation) is the tokenizer's job, not this
>    function's.
> 2. `tok_wstr(tok, line, &argc, &argv)` — split the NUL-terminated
>    `line` into `argc` words and a NULL-terminated `argv`. The words
>    live in the tokenizer's own storage and `argv[argc]` is NULL.
> 3. `argc = el_wparse(el, argc, argv)` — dispatch; see
>    `[spec:libedit:sem:parse.el-wparse-fn]`.
> 4. `tok_wend(tok)` — free the tokenizer, *including the word storage
>    `argv` points into*. Nothing reachable from `argv` may outlive this
>    call; handlers copy whatever they need to keep (`map_bind` copies
>    into its own buffers via `parse__string`, `terminal_settc` into
>    fixed `char[8]` buffers, and so on).
> 5. Return the value `el_wparse` produced, unmodified: 0 = handled or
>    deliberately skipped, +1 = the command ran and failed, -1 = empty
>    line or unknown command.
>
> The caller must have done the line hygiene already — this function
> strips nothing. `el_source` removes the trailing newline, skips
> leading whitespace and drops `#`-comment lines before calling; without
> that a comment line would tokenise to `argv[0] == L"#"`, match no
> command, return -1 and abort the rest of the file. Note the gap that
> leaves: a line consisting only of spaces or tabs survives
> `el_source`'s checks, tokenises to `argc == 0`, comes back -1, and
> **stops the sourcing of the file** — only a literally empty line is
> skipped.
>
> Two unchecked failures a port must not copy blindly:
>
> - `tok_winit` returns NULL on allocation failure and the result is
>   never checked; `tok_wstr` then dereferences NULL. Undefined
>   behaviour.
> - `tok_wstr`'s return value is **discarded**. It returns non-zero for
>   an unmatched single quote (1), an unmatched double quote (2), a
>   dangling backslash-newline continuation (3) and an internal
>   allocation failure (-1), and on *every* one of those paths it
>   returns without writing `*argc` or `*argv`. `parse_line` declares
>   both as uninitialised locals, so an editrc line such as `bind 'foo`
>   hands an indeterminate `argc` and a wild `argv` to `el_wparse` —
>   undefined behaviour, in practice a bad dereference or an absurd
>   argument count. The port must check the tokenizer result; the
>   natural mapping is to report the malformed line as -1.
>
> Callers: `el_source` runs this over every non-empty, non-comment line
> of the editrc file and `break`s out of the read loop on -1, returning
> that -1 as the result of `el_source`; `ed_command` (the vi `:` prompt,
> and whatever `M-X`-style binding a user gives it) runs it over the
> line the user typed and rings the bell on -1.

> [spec:libedit:def:parse.parse-string-fn]
> libedit_private wchar_t * parse__string(wchar_t *out, const wchar_t *in)

> [spec:libedit:sem:parse.parse-string-fn]
> Decodes a whole key-binding string: reads the escape syntax from `in`
> and writes the raw wide characters to `out`. Returns `out` (the
> pointer as passed in) on success, or NULL if any escape was malformed.
>
> Loop forever, dispatching on the current input character:
>
> - **`\0`** — write a terminating `\0` at the current output position
>   and return the saved start pointer. This is the only success exit.
> - **`\` or `^`** — call `parse__escape(&in)`, which both yields the
>   value and advances `in` past the escape (see
>   `[spec:libedit:sem:parse.parse-escape-fn]`). On -1 return NULL
>   immediately: no terminator is written and the output buffer is left
>   partially filled. Otherwise cast the `int` to `wchar_t` and append
>   it.
> - **`M` followed by `-` followed by any non-NUL character** — append
>   0x1B (ESC) and advance `in` by **2**, i.e. past the `M-` only. The
>   character after the `M-` is *not* consumed here; it goes round the
>   loop again and is decoded by whatever rule then applies. So `M-a` →
>   ESC `a`; `M-^A` → ESC 0x01; `M-\e` → ESC ESC; `M-M-a` → ESC ESC `a`.
>   This is how editrc spells a meta-prefixed key.
> - **anything else** — copy the character verbatim and advance `in` by
>   one. This includes an `M` not followed by `-` (`Ma` → `M` `a`), an
>   `M-` with nothing after it (`in[2] == '\0'`, so `M-` alone yields
>   the two characters `M` `-`), and every ordinary character. Note
>   `MM-a` → `M` ESC `a`: the fallthrough consumes only the first `M`,
>   and the second is re-examined.
>
> What a re-implementer needs beyond the loop:
>
> - The meta form is recognised only for capital `M`, only for a
>   literal `-`, and only unescaped. `\M-a` decodes `\M` as an
>   unrecognised backslash escape to `M`, then copies `-` and `a`,
>   giving three characters rather than ESC `a`.
> - A trailing `M-` is not an error, but a trailing bare `\` or `^` is:
>   `parse__escape` rejects any escape introducer that is the last
>   character of the string, so `"a\\"` and `"a^"` both yield NULL.
> - **There is no output bound and no length parameter.** `out` must
>   have room for `wcslen(in) + 1` wide characters. That is always
>   sufficient for defined inputs: every rule consumes at least one
>   input character per output character, and the `M-` rule consumes two
>   for one. Callers (`map_bind`, twice — instring and outstring) pass
>   `wchar_t[EL_BUFSIZ]` and rely on the tokenizer having bounded the
>   word length.
> - **The decoded value may be 0** (`^@`, `\0`, `\U+0000`). It is
>   written to the output like any other character and the loop
>   continues, so the result can contain embedded NULs; read back as a C
>   string it is truncated at the first one. `map_bind` measures the
>   binding by testing `in[1]` and indexes `map[(unsigned char)*in]`, so
>   a leading NUL binds keymap slot 0 and any embedded NUL truncates a
>   multi-character binding.
> - `\U+` escapes at the end of `in` leave `parse__escape`'s cursor past
>   the terminator (see its rule); this loop then resumes beyond the end
>   of the input and keeps decoding adjacent memory until it happens on
>   a zero. Undefined behaviour, inherited.
> - On the NULL path the output buffer is partially written and
>   unterminated; the caller must not read it. `map_bind` prints
>   ``"%ls: Invalid \\ or ^ in instring.\n"`` (or `outstring`) and fails
>   the whole `bind` line with -1.
> - Termination of the unbounded `for(;;)` depends on `parse__escape`
>   advancing `in` by at least one on every success, which it does on
>   every defined path.
