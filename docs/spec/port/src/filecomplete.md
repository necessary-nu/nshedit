# src/filecomplete.c

> [spec:libedit:def:filecomplete.append-char-function-fn]
> static const char * append_char_function(const char *name)

> [spec:libedit:sem:filecomplete.append-char-function-fn]
> Chooses the string that should follow a completed name: `"/"` if the
> name is a directory, `" "` otherwise. This is the default `app_func`
> for `fn_complete2`, `fn_complete` and `fn_display_match_list`.
>
> 1. If `name[0] == '~'`, set `expname = fn_tilde_expand(name)` (a
>    fresh heap string, or NULL if that allocation failed); otherwise
>    `expname` is NULL.
> 2. Start with the result `rs = " "`.
> 3. `stat()` the path `expname` if it is non-NULL, else `name`. If
>    `stat` returns -1 for any reason — nonexistent, permission denied,
>    a symlink loop, or a failed expansion that left `expname` NULL so
>    the raw `~...` text got stat'ed — leave `rs` as `" "`.
> 4. Otherwise, if `S_ISDIR(st_mode)` is true, set `rs = "/"`.
> 5. Free `expname` if one was allocated, and return `rs`.
>
> Ownership: the return is one of two static string literals. The
> caller must NOT free it and it never expires. Note that the two
> callers consume it differently — `fn_complete2` and
> `fn_display_match_list` insert or print the WHOLE string, while
> `escape_filename` looks only at `append_char[0]`, so an `app_func`
> returning more than one character is silently truncated on that path.
>
> `stat` follows symlinks, so a symlink to a directory yields `"/"`.
> The path is resolved relative to the process working directory, and
> the name handed in is the match string exactly as the generator built
> it, which still carries whatever directory prefix the user typed —
> including an unexpanded `~`. Combined with the `fn_tilde_expand` bug
> for names with no slash, a bare `~user` is stat'ed as the nonsense
> path `<home>/~user` and therefore always reports `" "`.
>
> The answer is inherently a filesystem race: the entry can be created,
> removed or replaced between the moment the generator produced the
> name and this `stat`, and there is no behaviour to preserve beyond
> "whatever `stat` reported at that instant". A port need not make the
> two observations atomic.

> [spec:libedit:def:filecomplete.completion-matches-fn]
> char ** completion_matches(const char *text, char *(*genfunc)(const char *, int))

> [spec:libedit:sem:filecomplete.completion-matches-fn]
> Drives a readline-style generator to exhaustion and packages the
> results as a NULL-terminated array in which element 0 is the longest
> common prefix of the matches and elements 1..n are the matches
> themselves, in generation order. Exported for readline compatibility.
>
> 1. Start with `matches = 0`, `match_list = NULL`, capacity 1.
> 2. Loop, calling `genfunc(text, (int)matches)` — the state argument
>    is the number of matches collected so far, so the first call
>    passes 0, which is the generator's "start a new scan" signal, and
>    later calls pass 1, 2, 3, ... . Stop as soon as the generator
>    returns NULL.
>    - Before storing, if `matches + 3 >= capacity`, double the
>      capacity repeatedly until `matches + 3 < capacity`, then
>      `realloc` `match_list` to that many `char *`. The first
>      iteration therefore lands on 4 slots. On realloc failure: free
>      the ARRAY only — the match strings it already holds and the
>      string just generated all leak — and return NULL.
>    - Store as `match_list[++matches] = retstr`, so index 0 is left
>      reserved and the first match lands at index 1.
> 3. If `match_list` is still NULL — which happens exactly when the
>    very first generator call returned NULL, i.e. there are no matches
>    at all — return NULL without further work.
> 4. Longest common prefix: set `max_equal = strlen(match_list[1])`,
>    then for `which` from 2 to `matches`, advance `i` from 0 while
>    `i < max_equal && match_list[1][i] == match_list[which][i]`, and
>    assign `max_equal = i`. The comparison is always against
>    `match_list[1]` — the local is named `prevstr` but is never
>    reassigned, which happens to be correct for a common prefix. It is
>    byte-wise and case-sensitive with no multibyte awareness, so the
>    prefix can be cut in the middle of a multibyte character. Since
>    `max_equal` only ever shrinks and starts at the length of
>    `match_list[1]`, no read runs past any string's NUL.
> 5. Allocate `max_equal + 1` zeroed bytes, copy the first `max_equal`
>    bytes of `match_list[1]` into it, NUL-terminate, and store it as
>    `match_list[0]`. On allocation failure, free the array — again
>    leaking every match string — and return NULL.
> 6. Set `match_list[matches + 1] = NULL` and return the array. The
>    growth rule in step 2 guarantees at least two spare slots, so this
>    write and `fn_complete2`'s speculative read of `matches[2]` are
>    always in bounds for arrays built here.
>
> With exactly one match, element 0 is a byte-for-byte copy of element
> 1; that equality is precisely what `fn_complete2` tests to recognise
> a unique match. When the matches share no leading bytes, element 0 is
> the empty string, which `fn_complete2` treats as "nothing to insert".
>
> No sorting and no de-duplication happen here: matches appear in the
> order the generator produced them and duplicates are preserved.
> Sorting happens later and only for display, inside
> `fn_display_match_list`, which reorders the caller's array in place.
> Note that readline's own `rl_completion_matches` sorts, uses
> `strcmp`, and computes element 0 by a different rule (the minimum
> adjacent-pair prefix, falling back to a copy of `text`); the two
> functions are NOT interchangeable and both must be ported as-is.
>
> Ownership: on success the caller owns the array and every string in
> it, elements 0 through `matches`, and must free each element and then
> the array — `fn_complete2` does exactly that. NULL means either "no
> matches" or "allocation failed", indistinguishable, and on both
> failure paths every match string collected so far is leaked.

> [spec:libedit:def:filecomplete.el-fn-complete-fn]
> unsigned char _el_fn_complete(EditLine *el, int ch __attribute__((__unused__)))

> [spec:libedit:sem:filecomplete.el-fn-complete-fn]
> The editor-command wrapper that lets filename completion be bound to
> a key. It calls
> `fn_complete(el, NULL, NULL, break_chars, NULL, NULL, (size_t)100,
> NULL, NULL, NULL, NULL)` and returns the result cast to
> `unsigned char`. The invoking key `ch` is ignored.
>
> `break_chars` is the file-static word-break set
> ``L" \t\n\"\\'`@$><=;|&{("`` — space, tab, newline, double quote,
> backslash, single quote, backtick, `@`, `$`, `>`, `<`, `=`, `;`,
> `|`, `&`, `{`, `(`. It carries the opening `{` and `(` but not their
> closing partners, and it is a different set from `needs_escaping`'s.
>
> Passing NULL for both `complete_func` and
> `attempted_completion_function` means: the generator is
> `fn_filename_completion_function`, the append function is
> `append_char_function`, there are no special prefixes, the
> "Display all N possibilities?" threshold is 100, nothing is reported
> back through `completion_type`/`over`/`point`/`end`, and — by
> `fn_complete`'s flag rule, since there is no attempted function —
> `FN_QUOTE_MATCH` is set, so the inserted match is shell-escaped by
> `escape_filename` and carries its own appended character.
>
> The value returned is one of the `CC_*` editor return codes
> (`CC_NORM` 0, `CC_REFRESH` 4, `CC_REDISPLAY` 8); all fit in an
> `unsigned char`, so the narrowing cast never loses information for
> any value `fn_complete2` can produce.

> [spec:libedit:def:filecomplete.el-fn-sh-complete-fn]
> unsigned char _el_fn_sh_complete(EditLine *el, int ch)

> [spec:libedit:sem:filecomplete.el-fn-sh-complete-fn]
> Behaviourally identical to `_el_fn_complete`: it returns
> `_el_fn_complete(el, ch)`, passing the invoking key through (which
> `_el_fn_complete` then ignores). It has no state and allocates
> nothing.
>
> It exists as a separate exported symbol, declared alongside
> `_el_fn_complete` in `histedit.h`, so that applications and key
> binding tables can name a "shell-style" completion command
> distinctly. In this version there is no difference in behaviour —
> same word-break set, same generator, same quoting, same query
> threshold. The port must keep both symbols, both exported, and both
> doing the same thing.

> [spec:libedit:def:filecomplete.escape-filename-fn]
> static char * escape_filename(EditLine * el, const char *filename, int single_match, const char *(*app_func)(const char *))

> [spec:libedit:sem:filecomplete.escape-filename-fn]
> Produces a heap copy of `filename` with shell metacharacters escaped
> in the way the text already on the line demands, optionally with the
> append character and a closing quote tacked on. Reached only from the
> `FN_QUOTE_MATCH` path of `fn_complete2`.
>
> 1. If `filename` is NULL, return NULL.
> 2. Work out the quoting context by scanning the line from
>    `el->el_line.buffer` up to but not including `el->el_line.cursor`,
>    one wide character at a time, maintaining two flags that both
>    start false:
>    - a `'` toggles `s_quoted`, but only when `d_quoted` is false and
>      the quote is either the first character of the line or is not
>      immediately preceded by a backslash;
>    - otherwise a `"` toggles `d_quoted`, but only when `s_quoted` is
>      false. There is deliberately NO backslash check on this branch,
>      so a user-typed `\"` still toggles `d_quoted`. That asymmetry
>      with the single-quote rule is a bug the port must reproduce.
>    The two flags are consequently mutually exclusive — at most one
>    can be true at any point in the scan. The single-quote rule looks
>    back exactly one character, so an escaped backslash followed by a
>    quote (`\\'`) is misread as an escaped quote.
>    IMPORTANT: `fn_complete2` calls this AFTER `el_deletestr` has
>    already removed the partial word from the line, so the scan sees
>    the opening quote the user typed but not the word being completed.
>    A port that reorders those two steps changes the escaping.
> 3. Counting pass over the BYTES of `filename` (`original_len` counts
>    all of them), accumulating `escaped_character_count`:
>    - if `s_quoted` and the byte is `'`: add 3, next byte;
>    - else if `d_quoted` and `needs_dquote_escaping(byte)`: add 1,
>      next byte;
>    - else if neither flag is set and `needs_escaping(byte)`: add 1.
> 4. `newlen = original_len + escaped_character_count + 1`; add one
>    more if either quote flag is set (room for a closing quote), and
>    one more if `single_match` is true and `app_func` is non-NULL
>    (room for the append character). Allocate `newlen` uninitialised
>    bytes; return NULL if that fails.
> 5. Emitting pass over the bytes of `filename`, in order:
>    - if `!needs_escaping(byte)`, copy it verbatim and move on. (All
>      four bytes `needs_dquote_escaping` cares about are also in the
>      `needs_escaping` set, so they always reach the rules below.)
>    - else if the byte is `'` and `s_quoted`, emit the four bytes
>      `'\''` — close the quote, backslash-escape the apostrophe,
>      reopen the quote. This is the +3 counted in step 3.
>    - else if `s_quoted`, copy verbatim: nothing else needs escaping
>      inside single quotes.
>    - else if `d_quoted` and `!needs_dquote_escaping(byte)`, copy
>      verbatim.
>    - else emit a backslash followed by the byte.
>    Each branch emits exactly as many bytes as step 3 budgeted, so the
>    buffer can never overflow.
> 6. If `single_match` AND `app_func` is non-NULL: write a NUL at the
>    current offset (without advancing), then call `app_func(filename)`
>    — note it receives the ORIGINAL `filename`, not the escaped string
>    — and keep the returned pointer as `append_char`. Only
>    `append_char[0]` is used. If that byte is a space it is appended
>    only when NEITHER quote flag is set; any other byte, typically `/`
>    for a directory, is appended unconditionally.
> 7. If `single_match` and `append_char` is non-NULL and
>    `append_char[0]` is a space, close the quote: emit `'` when
>    `s_quoted`, else `"` when `d_quoted`. The space was suppressed in
>    step 6 in exactly this case, so the byte reserved for it is reused
>    by the quote and step 4's size always suffices. When the append
>    character is `/` — a directory — the quote is deliberately left
>    open, because the user is expected to keep typing the path.
> 8. Write a terminating NUL at the current offset and return the
>    buffer.
>
> Ownership: the returned buffer is `malloc`'d and the caller frees it;
> `fn_complete2` does so with plain `free`, which is the same allocator
> the `el_*` macros wrap. NULL means either `filename` was NULL or an
> allocation failed, indistinguishable.
>
> Degenerate case: if `app_func` returns an empty string, step 6's
> "any other byte" branch copies that string's NUL into the buffer and
> advances past it, so the result carries an embedded NUL and its
> visible length ends one byte before the real end. readline's
> append-character hook does exactly this whenever
> `rl_completion_append_character` is 0.
>
> `filename` is walked as bytes and each byte is widened to `wchar_t`
> for `needs_escaping`. A byte with the high bit set matches no case
> whichever way `char` and `wchar_t` are signed, so the bytes of
> multibyte characters are neither escaped nor mistaken for
> metacharacters.

> [spec:libedit:def:filecomplete.find-word-to-complete-fn]
> static wchar_t * find_word_to_complete(const wchar_t * cursor, const wchar_t * buffer, const wchar_t * word_break, const wchar_t * special_prefixes, size_t * length, int do_unescape)

> [spec:libedit:sem:filecomplete.find-word-to-complete-fn]
> Walks backwards from the cursor to find where the word being
> completed starts, reports that word's raw length through `*length`,
> and returns a fresh copy of it — with backslashes stripped if
> `do_unescape` is non-zero. Everything here is wide characters.
>
> 1. `ctemp = cursor`. If `ctemp > buffer` and the character
>    immediately before the cursor is `\`, `'` or `"`, step `ctemp`
>    back by one, so the scan carries on through the word that precedes
>    that quote or backslash instead of stopping at it.
> 2. Scan backwards, repeatedly:
>    - stop if `ctemp <= buffer`;
>    - if at least two characters lie behind `ctemp` and `ctemp[-2]` is
>      a backslash and `needs_escaping(ctemp[-1])` is true, this is a
>      backslash-escaped character: step back TWO and continue. This
>      test comes FIRST, so an escaped word-break character does not
>      end the word — in `a\ b` the whole `a\ b` is the word;
>    - stop if `ctemp[-1]` occurs in `word_break`;
>    - stop if `special_prefixes` is non-NULL and `ctemp[-1]` occurs in
>      it. Either way the stopping character itself is not part of the
>      word (the two sets differ only in what the caller does with the
>      information — this function treats them identically);
>    - otherwise step back one.
> 3. `len = cursor - ctemp`, measured from the ORIGINAL cursor, so it
>    includes the trailing quote or backslash that step 1 stepped over.
> 4. If `len == 1` and that single character is `'` or `"`, set
>    `len = 0` and advance `ctemp` past it: a lone quote at the cursor
>    means an empty word starting after the quote.
> 5. Store `len` through `*length`. This is the RAW span in the line,
>    before any unescaping, and it is exactly what `fn_complete2` hands
>    to `el_deletestr` — so the count of characters deleted from the
>    line can exceed the length of the string returned here whenever
>    escapes were removed.
> 6. If `do_unescape` is non-zero, return `unescape_string(ctemp, len)`
>    — every backslash in the span removed. Otherwise allocate
>    `len + 1` wide characters, copy `len` characters from `ctemp`, put
>    a terminating 0 at index `len`, and return that.
>
> Ownership: the returned wide string is heap-allocated and the caller
> frees it (`fn_complete2` does). NULL is returned only on allocation
> failure, and `*length` has already been written by then, so a caller
> must not assume `*length` is untouched on failure.
>
> `word_break` is passed straight to `wcschr` with no NULL check, so a
> NULL `word_break` is undefined behaviour; every in-tree caller
> supplies one. `wcschr` also matches the terminating NUL of the break
> set, so a 0 character inside the line buffer would count as a break
> character — the line buffer never holds one. `special_prefixes` IS
> NULL-checked and NULL simply disables that test.

> [spec:libedit:def:filecomplete.fn-complete-fn]
> int fn_complete(EditLine *el, char *(*complete_func)(const char *, int), char **(*attempted_completion_function)(const char *, int, int), const wchar_t *word_break, const wchar_t *special_prefixes, const char *(*app_func)(const char *), ...

> [spec:libedit:sem:filecomplete.fn-complete-fn]
> Thin wrapper: forwards every argument unchanged to `fn_complete2` and
> supplies the one argument `fn_complete2` has and it does not, the
> `flags` word, computed as
> `attempted_completion_function ? 0 : FN_QUOTE_MATCH` (that macro is
> 1). It returns `fn_complete2`'s return value verbatim and does
> nothing else — no allocation, no state.
>
> The rule it encodes: when the application supplies its own
> `attempted_completion_function`, the match it produced is inserted
> verbatim and the application is trusted to have quoted it; when it
> does not, the built-in filename path runs and the inserted match is
> put through `escape_filename`. That same choice decides which code
> path appends the trailing space or `/`: see step 12e of the
> `fn_complete2` rule, where the non-quoting path appends only when an
> attempted function was supplied.

> [spec:libedit:def:filecomplete.fn-complete2-fn]
> int fn_complete2(EditLine *el, char *(*complete_func)(const char *, int), char **(*attempted_completion_function)(const char *, int, int), const wchar_t *word_break, const wchar_t *special_prefixes, const char *(*app_func)(const char *),...

> [spec:libedit:sem:filecomplete.fn-complete2-fn]
> The completion command proper. It finds the word before the cursor,
> obtains candidate matches, replaces the word with the matches' common
> prefix (escaped, and with a trailing character when the match is
> unique), and on a repeated invocation lists the candidates. Returns a
> `CC_*` editor return code.
>
> 1. `what_to_do` is `'\t'`, or `'?'` when
>    `el->el_state.lastcmd == el->el_state.thiscmd`, i.e. when the
>    previous editor command was this same command. That is the entire
>    mechanism by which a second consecutive Tab becomes "list the
>    possibilities". The header comment also documents `'*'` and `'!'`,
>    but `what_to_do` is never set to either, so the `'!'` tests below
>    are dead code and `'*'` (insert all matches) is unimplemented.
> 2. If `completion_type` is non-NULL, store `what_to_do` through it —
>    this is readline's `rl_completion_type`.
> 3. Default the callbacks: a NULL `complete_func` becomes
>    `fn_filename_completion_function`, a NULL `app_func` becomes
>    `append_char_function`. `do_unescape` is `flags & FN_QUOTE_MATCH`
>    (`FN_QUOTE_MATCH` is 1).
> 4. Take the wide line view `li = el_wline(el)` and call
>    `find_word_to_complete(li->cursor, li->buffer, word_break,
>    special_prefixes, &len, do_unescape)`. If it returns NULL, jump
>    to the exit and return `CC_NORM`, having changed nothing.
> 5. If `point` is non-NULL store `li->cursor - li->buffer`; if `end`
>    is non-NULL store `li->lastchar - li->buffer`. These are written
>    BEFORE any user callback runs, because readline callbacks read
>    `rl_point` and `rl_end`. Both are counts of WIDE characters while
>    the strings handed to the callbacks are multibyte, so in a
>    non-ASCII locale the offsets do not index those strings — a
>    known wart that the port must keep.
> 6. If `attempted_completion_function` is non-NULL, call it with the
>    word encoded to multibyte through `el->el_scratch`, a start offset
>    of `cursor_off - len` and an end offset of `cursor_off` (both wide
>    offsets, per the caveat above). Otherwise `matches = NULL`. The
>    encoded string points into `el->el_scratch` and is invalidated by
>    the next encode, so a callback must not retain it.
> 7. Fall back to the built-in path when there was no attempted
>    function at all, OR when `over` is non-NULL and `*over` is 0 and
>    the attempted function returned NULL: `matches =
>    completion_matches(<word re-encoded to multibyte>,
>    complete_func)`. Note the asymmetry — if `over` is NULL and the
>    attempted function returned NULL there is NO fallback, and
>    completion simply does nothing.
> 8. If `over` is non-NULL, store 0 through it.
> 9. If `matches` is NULL, jump to the exit and return `CC_NORM`.
> 10. `single_match` is true when `matches[2] == NULL` AND
>     (`matches[1] == NULL` or `strcmp(matches[0], matches[1]) == 0`).
>     Because `matches[2]` is tested first, an array holding only two
>     elements — which a user-supplied `attempted_completion_function`
>     may legitimately return as `{prefix, NULL}` — is read out of
>     bounds. Arrays from `completion_matches` always carry the slack
>     for it; a foreign array need not, and that read is undefined
>     behaviour.
> 11. Set the return value to `CC_REFRESH`.
> 12. If `matches[0]` is not the empty string:
>     a. `el_deletestr(el, (int)len)` — remove the raw word span from
>        the line (`len == 0` is a no-op).
>     b. Build the replacement text: with `FN_QUOTE_MATCH` set that is
>        `escape_filename(el, matches[0], single_match, app_func)`,
>        otherwise `strdup(matches[0])`. The ordering with (a) is
>        load-bearing: the deletion happens first, so
>        `escape_filename` scans a line from which the partial word has
>        already been removed.
>     c. If that allocation failed, jump to the match-freeing exit and
>        return `CC_REFRESH`. The word has already been deleted from
>        the line and is NOT restored — a failed completion silently
>        eats the user's partial word.
>     d. Decode the replacement back to wide characters and insert it
>        at the cursor with `el_winsertstr`.
>     e. If `single_match` AND `attempted_completion_function` is
>        non-NULL AND `FN_QUOTE_MATCH` is NOT set, also insert
>        `app_func(completion)` — the trailing space, or `/` for a
>        directory. Note `app_func` is applied to the inserted string,
>        not to `matches[0]`. This is the only place the append string
>        is added on the non-quoting path; on the `FN_QUOTE_MATCH`
>        path `escape_filename` already appended its first character.
>        A caller passing neither an attempted function nor
>        `FN_QUOTE_MATCH` — which is exactly what readline's
>        `rl_complete` does when the application set no attempted
>        completion function — gets NO append character at all.
>     f. Free the replacement string.
> 13. If NOT `single_match` and `what_to_do` is `'?'` (or the
>     unreachable `'!'`):
>     a. Walk `matches[1]` onward to the NULL terminator, computing
>        `maxlen` (the longest match's byte length) and `matches_num`
>        (the number of real matches).
>     b. Print a newline to `el->el_outfile`.
>     c. If `matches_num > query_items`, print
>        `"Display all %zu possibilities? (y or n) "`, flush, then read
>        ONE character with `getc(stdin)` — always `stdin`, never
>        `el->el_infile` — and suppress the listing unless that
>        character is exactly `'y'`. Then print a newline. Only one
>        character is consumed, so the rest of the user's input line,
>        the newline after the `y` included, stays in the stream.
>     d. Unless suppressed, call `fn_display_match_list(el, matches,
>        matches_num + 1, maxlen, app_func)`; the `+ 1` restores the
>        1-based convention that function expects. This SORTS
>        `matches[1..]` in place.
>     e. The return value becomes `CC_REDISPLAY`.
>     Else if `matches[0]` is non-empty — a common prefix was inserted
>     but is not a complete match — `el_beep(el)` and keep
>     `CC_REFRESH`.
>     Else — empty common prefix, so nothing was inserted —
>     `el_beep(el)` and drop the return value back to `CC_NORM`.
> 14. Free every element of `matches` up to the NULL terminator, then
>     the array itself. This frees element 0, the prefix, as well, and
>     it frees an array returned by a user-supplied
>     `attempted_completion_function` too: ownership of that array and
>     all its strings transfers to `fn_complete2` unconditionally, so
>     the callback must return individually heap-allocated strings in a
>     heap-allocated array and must not keep a reference. The
>     permutation applied by `fn_display_match_list` does not change
>     which pointers get freed.
> 15. Free the word from step 4 and return.
>
> Return values: `CC_NORM` (0) when no word was found, no matches were
> produced, or the common prefix was empty — line unchanged, possibly
> after a beep; `CC_REFRESH` (4) when the line was modified;
> `CC_REDISPLAY` (8) when a match list was printed. `CC_ERROR` is never
> returned, and allocation failures surface as ordinary `CC_REFRESH` or
> `CC_NORM` outcomes rather than as errors.

> [spec:libedit:def:filecomplete.fn-display-match-list-fn]
> void fn_display_match_list(EditLine * el, char **matches, size_t num, size_t width, const char *(*app_func) (const char *))

> [spec:libedit:sem:filecomplete.fn-display-match-list-fn]
> Prints the match list to `el->el_outfile`, sorted, in column-major
> columns sized to the terminal width.
>
> The interface is 1-based for readline compatibility: `matches[0]` is
> the common prefix and is NOT a match, but it IS counted in `num`, so
> the strings actually printed are `matches[1]` through
> `matches[num - 1]`. `width` is the caller's promise about the longest
> of those strings.
>
> 1. Read the screen width from `el->el_terminal.t_size.h`. If
>    `app_func` is NULL, substitute `append_char_function`.
> 2. Advance `matches` by one and decrement `num`, so from here on the
>    strings are `matches[0 .. num-1]`.
> 3. `cols = (size_t)screenwidth / (width + 2)` — integer division,
>    where the `+ 2` covers the one-space column separator and the
>    single appended character. If the result is 0, use 1.
> 4. `lines = (num + cols - 1) / cols`, i.e. ceiling division.
> 5. Sort `matches[0 .. num-1]` in place with `qsort` and
>    `_fn_qsort_string_compare` (case-insensitive). This MUTATES the
>    caller's array. `fn_complete2` depends on the set of pointers
>    being merely permuted, so that its later free loop still covers
>    every string. The original element 0, the common prefix, is
>    excluded from the sort by step 2 and keeps its position.
> 6. For `line` from 0 to `lines - 1`, and inside it `col` from 0 to
>    `cols - 1`, compute `thisguy = line + col * lines` — column-major,
>    so reading DOWN a column gives sorted order — and break out of the
>    inner loop as soon as `thisguy >= num`. For each entry, `fprintf`
>    to `el->el_outfile`: a single space unless this is the first
>    column, then the string, then `app_func(string)`, then a `"%-*s"`
>    of an empty string with field width `(int)(width -
>    strlen(string))`, i.e. that many padding spaces. After each line,
>    print a newline.
>
> Nothing is returned and nothing is freed. Padding is emitted after
> the last column too, so every line carries trailing whitespace.
> `app_func` is called once per entry printed, which for the default
> means one `stat` per entry per listing — the listing is therefore
> O(n) filesystem calls and can show a `/` for an entry that has since
> been deleted.
>
> Two caller obligations go unchecked, and a port should treat both as
> caller errors rather than defining behaviour for them:
> - `num` must be at least 1. Passing 0 makes step 2's decrement wrap
>   to `SIZE_MAX`, after which the loops walk far off the end of the
>   array — an out-of-bounds read in the C.
> - `width` must be at least the longest string's length. Otherwise
>   `width - strlen(string)` underflows as `size_t` and is then cast to
>   `int` for the field width, producing a huge or negative width and a
>   corresponding flood of spaces.
> A non-positive `screenwidth` is not defended either, but is benign: a
> negative value cast to `size_t` makes `cols` enormous, hence
> `lines == 1`, and everything prints on one long line.

> [spec:libedit:def:filecomplete.fn-filename-completion-function-fn]
> char * fn_filename_completion_function(const char *text, int state)

> [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]
> The default match generator: a stateful, readline-compatible iterator
> that returns the next filename whose name starts with the trailing
> component of `text`, or NULL when there are no more. Every piece of
> its state lives in function-level `static` variables — an open
> `DIR *`, the pattern `filename`, its byte length `filename_len`, the
> directory prefix `dirname` exactly as the user typed it, and the
> actually-opened path `dirpath`. There is one such set per process.
>
> Generator protocol. The scan is (re)started when `state == 0` OR when
> the static `DIR *` is NULL; otherwise the call CONTINUES the scan
> already in progress and the `text` argument is ignored entirely — a
> caller that changes `text` while passing a non-zero `state` gets
> matches for the old text. So `state` is not a sequence number, only a
> "restart" flag; the real iteration cursor is the read position of the
> open directory stream. (The comment above the C claims `state` is
> ignored, which is wrong; it is the restart trigger.)
>
> Restart path — run when `state == 0` or no stream is open:
> 1. Split `text` at its LAST `/`:
>    - if there is one, `filename` becomes a copy of everything after
>      it (the empty string when `text` ends in `/`), and `dirname`
>      becomes a copy of everything up to AND INCLUDING that slash.
>      Both reuse the previous call's buffers via `realloc`; if either
>      reallocation fails, that one static is freed and set to NULL and
>      the function returns NULL, leaving the remaining state stale —
>      see the hazards below;
>    - if there is none, `filename` becomes a copy of `text`, or NULL
>      when `text` is the empty string, and `dirname` is freed and set
>      to NULL.
> 2. If a stream is still open from a previous scan, `closedir` it and
>    clear the pointer. This happens AFTER step 1, which is what makes
>    step 1's failure paths leave an open stream behind.
> 3. Free `dirpath` and recompute it:
>    - `dirname == NULL` (no slash in `text`): `dirname` becomes a
>      freshly allocated empty string and `dirpath` becomes `"./"`;
>    - `dirname` begins with `~`: `dirpath = fn_tilde_expand(dirname)`.
>      `dirname` always ends in `/` here, so the tilde expansion always
>      takes its slash branch and the `~`-with-no-slash bug is
>      unreachable from here; an unknown user makes `fn_tilde_expand`
>      hand back the literal `~user/`, which then fails to open;
>    - otherwise `dirpath` is a plain copy of `dirname`.
>    Return NULL if any of those allocations fail.
> 4. `opendir(dirpath)`. On failure return NULL — the scan never
>    starts, and because the static stream stays NULL every later call
>    re-runs this entire restart path and fails identically.
> 5. `filename_len = filename ? strlen(filename) : 0`.
>
> Match loop — run on every call, restart or continuation:
> 6. Call `readdir` until an entry is accepted or the stream is
>    exhausted:
>    - skip entries named exactly `.` and `..`; every other dot-file IS
>      a candidate, so hidden files are offered even when the user
>      typed no leading dot;
>    - if `filename_len == 0`, accept the first surviving entry;
>    - otherwise accept the first entry whose name is at least
>      `filename_len` bytes long and whose first `filename_len` bytes
>      equal `filename`. That is a byte-wise, case-SENSITIVE `strncmp`
>      (preceded by a redundant first-byte check) with no locale
>      folding and no multibyte awareness, so a prefix can split a
>      multibyte character. There is no glob or fuzzy matching of any
>      kind.
> 7. On a match, return a newly allocated string holding `dirname`
>    concatenated with the entry name — the prefix the USER typed, so
>    an unexpanded `~` stays unexpanded in the result and `dirpath`
>    never appears in it. The stream is deliberately left OPEN and
>    positioned just past this entry; that is what lets the next call
>    continue. If the allocation fails, return NULL with the stream
>    still open, abandoning the scan mid-way.
> 8. When `readdir` returns NULL, `closedir` the stream, clear the
>    pointer and return NULL. `filename`, `dirname`, `dirpath` and
>    `filename_len` are NOT freed here; they survive until the next
>    restart replaces them, a bounded but permanent retention of the
>    last scan's strings.
>
> Ownership: every non-NULL return is a fresh heap string the caller
> frees (`completion_matches` stores it in the match array and
> `fn_complete2` frees it there). NULL means "no more matches", and is
> equally what an allocation failure, an unopenable directory and a
> `readdir` error return; `errno` is never consulted, so the four are
> indistinguishable to the caller.
>
> Hazards a port must decide about explicitly rather than inherit:
> - The statics make this non-reentrant and not thread-safe, and they
>   make two interleaved scans impossible: starting a new scan silently
>   destroys any scan in progress, including one belonging to another
>   thread or to a nested completion.
> - Because the restart path also triggers whenever the stream is NULL,
>   calling again with a non-zero `state` AFTER the generator returned
>   NULL restarts the scan from the first entry instead of continuing
>   to return NULL. Callers that stop at the first NULL
>   (`completion_matches`, `rl_completion_matches`) never notice; a
>   caller that keeps calling loops forever.
> - The failure paths in step 1 return NULL without closing the stream
>   and without updating `filename_len`. A following call with a
>   non-zero `state` then finds the stream open, skips the restart, and
>   runs the match loop against a NULL or stale `filename` with a stale
>   `filename_len`; the NULL case dereferences a null pointer. That is
>   a latent crash in the C, not behaviour to reproduce — a port should
>   reset the whole state atomically.
> - The directory is read incrementally across many calls, so the
>   result set is whatever the filesystem happened to contain at each
>   `readdir`. Entries created or removed mid-scan may be reported
>   once, twice or never; this is unspecified, per POSIX `readdir`.
> - Results come back in raw directory order: unsorted, and not
>   de-duplicated. Neither this function nor `completion_matches`
>   orders them; only `fn_display_match_list` sorts, and only for
>   display.

> [spec:libedit:def:filecomplete.fn-qsort-string-compare-fn]
> static int _fn_qsort_string_compare(const void *i1, const void *i2)

> [spec:libedit:sem:filecomplete.fn-qsort-string-compare-fn]
> The `qsort` comparison callback used by `fn_display_match_list`. Its
> two arguments are pointers to array ELEMENTS, i.e. `char *const *`;
> it loads the `char *` out of each and returns `strcasecmp(s1, s2)`.
>
> The ordering is therefore case-insensitive. `strcasecmp` folds case
> byte by byte according to `LC_CTYPE`, with no multibyte awareness, so
> bytes above 0x7f effectively compare by value. Two names differing
> only in case compare equal, and since `qsort` is not required to be
> stable their relative order is unspecified — a port may not be relied
> on to reproduce it and should not try. NULL elements are not handled;
> passing one dereferences it.

> [spec:libedit:def:filecomplete.fn-tilde-expand-fn]
> char * fn_tilde_expand(const char *txt)

> [spec:libedit:sem:filecomplete.fn-tilde-expand-fn]
> Expands a leading `~` or `~user` to the corresponding home directory
> and returns a fresh heap string. Anything that cannot be expanded
> comes back as a plain copy of the input.
>
> 1. If `txt[0]` is not `~`, return `strdup(txt)` (NULL if that fails).
> 2. Search for the first `/` at or after `txt + 1`:
>    - if there is none, the user name is a copy of `txt + 1`
>      (everything after the tilde) and the "rest of the path" offset
>      `len` stays 0;
>    - if there is one at index `k`, set `len = k + 1` (tilde, name and
>      slash) and take the `k - 1` characters strictly between the
>      tilde and the slash as the user name. For `~/foo`, `k` is 1 and
>      the name is empty.
>    Return NULL if either copy fails to allocate.
> 3. Look the user up:
>    - if the extracted name is empty (`~` alone, or a path beginning
>      `~/`), look up the CURRENT user by `getuid()`;
>    - otherwise look the name up by name.
>    The C picks one of three call shapes at configure time; a port
>    should behave as the POSIX one, `getpwnam_r(name, &pwres, buf,
>    sizeof buf, &result)` / `getpwuid_r(uid, &pwres, buf, sizeof buf,
>    &result)` with a fixed 1024-byte scratch buffer, treating ANY
>    non-zero return as "no such user" — which lumps a passwd entry too
>    large for 1024 bytes (`ERANGE`) together with a genuinely absent
>    one, and also treats a zero return with a NULL result as absent.
>    The alternatives it probes for are the Solaris draft form, which
>    returns the `struct passwd *` directly, and plain
>    `getpwnam`/`getpwuid`, which is not thread-safe and hands back a
>    pointer into static storage that the next `getpw*` call may
>    overwrite. For a successful lookup of an ordinary entry all three
>    are observationally identical.
> 4. Free the user-name copy. If the lookup produced nothing, return
>    `strdup(txt)`: the ORIGINAL text, tilde and all, unexpanded. An
>    unknown user is not an error and is not reported as one, so the
>    caller cannot distinguish "user missing" from "no tilde present".
> 5. Otherwise advance `txt` by `len` and return a freshly allocated
>    `"<pw_dir>/<txt>"` — the home directory, one literal `/`, then the
>    remainder — sized exactly `strlen(pw_dir) + 1 + strlen(txt) + 1`.
>    The `/` is inserted unconditionally, so a home directory that is
>    itself `/` yields a doubled slash (`~/x` becomes `//x`), and a
>    `pw_dir` with a trailing slash likewise doubles.
>
> BUG, observable through the public API and through readline's
> exported `tilde_expand()`: `len` is assigned only in the slash branch
> of step 2. With no slash present it is still 0 at step 5, so the
> ENTIRE original string, tilde included, is appended to the home
> directory. `fn_tilde_expand("~")` returns `"$HOME/~"` and
> `fn_tilde_expand("~bob")` returns `"<bob's home>/~bob"` — not
> `"$HOME"` and not `"<bob's home>"`. The port must reproduce this:
> `~name` without a trailing slash does not expand to a home
> directory. `fn_filename_completion_function` never reaches the bug
> because the string it passes always ends in `/`;
> `append_char_function` and `tilde_expand()` do reach it, which is why
> a bare `~user` match never stats as a directory.
>
> Ownership: the return is always a freshly heap-allocated string (via
> `strdup` or `calloc`) that the caller frees. NULL is returned only on
> allocation failure and never means "no such user". The passwd lookup
> reads the system password database, so this can block on a network
> name service.

> [spec:libedit:def:filecomplete.needs-dquote-escaping-fn]
> static int needs_dquote_escaping(char c)

> [spec:libedit:sem:filecomplete.needs-dquote-escaping-fn]
> Returns 1 for the four bytes that still need a backslash inside a
> double-quoted shell word — `"`, `\`, `` ` `` and `$` — and 0 for
> every other byte. Pure byte test, no locale involvement; bytes above
> 0x7f return 0.
>
> All four members are also members of the `needs_escaping` set. That
> containment is load-bearing: `escape_filename` tests `needs_escaping`
> first and copies anything it rejects verbatim, so if one of these
> four were missing from that set it would never reach the
> double-quote rule.

> [spec:libedit:def:filecomplete.needs-escaping-fn]
> static int needs_escaping(wchar_t c)

> [spec:libedit:sem:filecomplete.needs-escaping-fn]
> Returns 1 if the wide character is one that must be backslash-escaped
> when a completed word is inserted into an unquoted shell command
> line, and 0 otherwise. The set is exactly these 23 characters:
>
> `'` `"` `(` `)` `\` `<` `>` `$` `#` space newline tab `?` `;`
> `` ` `` `@` `=` `|` `{` `}` `&` `*` `[`
>
> Points that look like oversights and must be kept as they are: `]` is
> NOT in the set although `[` is; `)` and `}` ARE in it, although the
> word-break set used by `_el_fn_complete` carries only the opening
> forms; and `!`, `~`, `^`, `%`, `:` and `/` are absent.
>
> The comparisons are against ASCII code points, so any wide character
> outside ASCII returns 0. `escape_filename` calls this with a single
> BYTE widened to `wchar_t`; a byte with the high bit set becomes
> either a negative value or a large positive one depending on the
> signedness of `char` and `wchar_t`, and matches nothing either way,
> so the individual bytes of a multibyte character are never escaped.
>
> The same predicate is reused by `find_word_to_complete` to decide
> whether a backslash in the line is escaping something, so this set
> also defines which `\X` pairs are treated as a single unit when
> scanning backwards for the start of a word.

> [spec:libedit:def:filecomplete.unescape-string-fn]
> static wchar_t * unescape_string(const wchar_t *string, size_t length)

> [spec:libedit:sem:filecomplete.unescape-string-fn]
> Returns a fresh wide string holding the first `length` characters of
> `string` with every backslash removed.
>
> 1. Allocate `length + 1` wide characters, zero-filled. Return NULL if
>    the allocation fails.
> 2. Copy `string[0 .. length-1]` into it in order, skipping any
>    character equal to `\` — unconditionally, without looking at what
>    follows it.
> 3. Write a terminating 0 immediately after the last character copied.
>
> The result may be shorter than `length`; the allocation is still
> `length + 1` wide characters and the unused tail remains zeroed. The
> input need NOT be NUL-terminated — only `length` bounds the read —
> and an embedded 0 would be copied through as an ordinary character,
> producing a string that appears to end early.
>
> Because the skip is unconditional there is no notion of an escaped
> backslash: a `\\` pair collapses to nothing at all, and a trailing
> `\` is simply dropped. The only caller is `find_word_to_complete`,
> which applies it to the raw word span when `do_unescape` is set.
>
> Ownership: the returned buffer is heap-allocated and the caller frees
> it. NULL is returned only on allocation failure.

