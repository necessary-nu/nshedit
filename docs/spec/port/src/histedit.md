# src/histedit.h

> [spec:libedit:def:histedit.edit-line]
> typedef struct editline EditLine

> [spec:libedit:def:histedit.el-beep-fn]
> void el_beep(EditLine *)

> [spec:libedit:sem:histedit.el-beep-fn]
> Ring the terminal bell on `el`'s output stream. Delegates unconditionally
> to the terminal layer's beep: if the terminal has a visible-bell string
> capability and it is usable, that is emitted, otherwise the audible bell
> string is emitted; if neither is available nothing is written. Output is
> written into libedit's terminal output buffer and is NOT flushed by this
> call — the caller (or the next flush point, e.g. `el_wgetc`/`el_wgets`)
> flushes it.
> `el` must be a live handle from `el_init`/`el_init_fd`; passing `NULL` is
> undefined behaviour (no NULL check exists). Returns nothing; failure to
> beep is not reported.

> [spec:libedit:def:histedit.el-cursor-fn]
> int el_cursor(EditLine *, int)

> [spec:libedit:sem:histedit.el-cursor-fn]
> Move the edit-buffer cursor by `n` positions (wide characters, not bytes
> or display columns) and return the resulting cursor offset from the start
> of the line buffer.
> Steps: if `n == 0`, skip straight to the return. Otherwise add `n` to the
> cursor pointer (`n` may be negative), then clamp: if the cursor is before
> the buffer start set it to the buffer start; if it is past `lastchar` set
> it to `lastchar`. Return `cursor - buffer` as an `int`.
> Note the clamp is applied to the *already advanced* pointer, so a large
> `n` transiently forms an out-of-range pointer — technically UB in C, but
> the observable result is a saturating move. A Rust port must compute the
> new offset as a saturating signed offset in `0 ..= (lastchar - buffer)`.
> Always succeeds; there is no error return. `el` must be non-NULL (no
> check). The value returned is a character index, and remains meaningful
> only until the buffer is next modified.

> [spec:libedit:def:histedit.el-deletestr-fn]
> void el_deletestr(EditLine *, int)

> [spec:libedit:sem:histedit.el-deletestr-fn]
> Delete `n` characters immediately before the cursor. Also exported under
> the name `el_wdeletestr`, which `histedit.h` `#define`s to `el_deletestr`
> — there is only one function and it counts wide characters in both
> spellings.
> Steps: if `n <= 0`, return without doing anything. If the cursor is
> before `buffer + n` (i.e. there are fewer than `n` characters to the left
> of the cursor) return without doing anything — note this is a hard
> refusal, not a partial delete. Otherwise perform the internal
> "delete before dot" operation: clamp `n` to the number of characters
> actually left of the cursor; if the current keymap is not the emacs map
> (i.e. vi mode) push an undo record and yank the deleted range into the
> kill buffer; then shift the tail `[cursor, lastchar]` left by `n` and
> decrement `lastchar` by `n`. Finally move the cursor back by `n`, and if
> that would place it before the buffer start, set it to the buffer start.
> Returns nothing; the caller cannot tell a refusal from a success.
> `el` must be non-NULL (no check). Invalidates any `LineInfo`/`LineInfoW`
> previously obtained from `el_line`/`el_wline`.

> [spec:libedit:def:histedit.el-deletestr1-fn]
> int el_deletestr1(EditLine *, int, int)

> [spec:libedit:sem:histedit.el-deletestr1-fn]
> Delete the half-open character range `[start, end)` of the current line,
> where both are 0-based character offsets from the buffer start, and
> return the number of positions the caller asked to remove.
> Steps: if `end <= start` return 0. Let `line_length = lastchar - buffer`.
> If `start >= line_length` or `end >= line_length` return 0 — note this
> makes it impossible to delete the final character of the line, since
> `end == line_length` is rejected rather than clamped. Set
> `len = end - start`, then clamp `len` to `line_length - end`. Copy `len`
> characters from `buffer + end` to `buffer + start`, decrementing
> `lastchar` once per character copied. Then if the cursor lies before the
> buffer start, reset it to the buffer start. Return `end - start`.
> Two implementation traps that the port must reproduce exactly, because
> readline's `rl_delete_text` is layered on this call:
> (a) the return value is `end - start`, which is NOT necessarily the number
> of characters actually removed — `lastchar` is decremented `len` times,
> and `len` is the clamped value, so for a range near the end of the line
> the return over-reports;
> (b) the cursor is only clamped at the low end, never against the new
> `lastchar`, so after deleting a range before the cursor the cursor is
> left pointing at a different character than before, and after deleting a
> range at the end it can be left past `lastchar`.
> The moved region is not NUL-terminated by this function. `el` must be
> non-NULL (no check). Invalidates any previously returned `LineInfo`.

> [spec:libedit:def:histedit.el-end-fn]
> void el_end(EditLine *)

> [spec:libedit:sem:histedit.el-end-fn]
> Destroy an `EditLine` handle and release everything it owns.
> If `el` is `NULL`, return immediately — this is the one entry point in
> the editing API that tolerates `NULL`.
> Otherwise: first run `el_reset` (restore the tty to cooked mode and reset
> the character/parser state). Then tear down every module, in this fixed
> order: terminal, keymacro, map, tty (only if the `NO_TTY` flag is clear;
> the tty is drained with `TCSAFLUSH`), chared, read, search, hist, prompt,
> sig, literal. Then free the duplicated program name, the two halves
> (narrow and wide) of each of the three conversion buffers — visual,
> scratch and legacy — and finally the `EditLine` itself.
> After this call every pointer libedit ever handed the caller from this
> handle is dangling: the `el_gets`/`el_wgets` line, the `LineInfo` and
> `LineInfoW` structs, the string returned through `el_get(EL_EDITOR)` or
> `EL_WORDCHARS`, and the terminal name from `EL_TERMINAL`.
> libedit does NOT close or free the three `FILE *` streams or the three
> file descriptors given to `el_init`/`el_init_fd`; those stay the caller's.
> Nor does it touch a `History` handle installed via `EL_HIST` — the caller
> must `history_end` that separately.
> Returns nothing. Calling `el_end` twice on the same handle is a double
> free (undefined behaviour).

> [spec:libedit:def:histedit.el-fn-complete-fn]
> unsigned char _el_fn_complete(EditLine *, int)

> [spec:libedit:sem:histedit.el-fn-complete-fn]
> Editor command that performs filename completion on the word at or
> before the cursor. It exists so an application can bind completion to a
> key with `el_set(el, EL_BIND, "^I", "ed-complete", NULL)`-style wiring or
> pass it to `EL_ADDFN`; the leading underscore marks it as
> readline-compat plumbing rather than a general-purpose entry point, but
> it is an exported symbol and part of the ABI.
> The second parameter (the triggering character) is ignored.
> It calls the internal completion driver with: no application completion
> generator (so the builtin filename generator is used), no application
> "attempted completion" hook, the word-break set
> `" \t\n\"\\'`@$><=;|&{("` (space, tab, newline, double quote, backslash,
> single quote, backtick, at, dollar, greater-than, less-than, equals,
> semicolon, pipe, ampersand, open brace, open paren), no special prefix
> set, the builtin append-character function (which appends `/` after a
> directory and a space otherwise), a query threshold of 100 matches, and
> no out-parameters for completion type, override, point or end. Because
> the attempted-completion hook is NULL, the driver runs in
> quote-and-escape mode (`FN_QUOTE_MATCH`), i.e. the inserted match is
> shell-escaped and the word under the cursor is unescaped before matching.
> Behaviour of the driver, restated because it is observable here: if the
> previous command was this same command, the request is treated as "list
> the possibilities" rather than "complete"; the word before the cursor is
> extracted and unescaped; matches are generated; if there is a non-empty
> common prefix, that many characters are deleted with `el_deletestr` and
> the escaped completion inserted with `el_winsertstr`; if there is more
> than one match and listing was requested, a newline is printed to
> `el`'s output stream, and if the match count exceeds 100 the user is
> prompted `Display all N possibilities? (y or n) ` — that prompt is
> answered by reading a byte from the C `stdin` stream, NOT from `el`'s
> configured input stream or descriptor, which is a real bug when libedit
> is driven on a non-stdin fd.
> Returns the driver's editor return code narrowed to `unsigned char`:
> `CC_NORM` (0) when nothing could be completed or the word could not be
> extracted, `CC_REFRESH` (4) when the line was modified, `CC_REDISPLAY`
> (8) when a match list was printed. `el` must be non-NULL.

> [spec:libedit:def:histedit.el-fn-sh-complete-fn]
> unsigned char _el_fn_sh_complete(EditLine *, int)

> [spec:libedit:sem:histedit.el-fn-sh-complete-fn]
> Exported alias for `_el_fn_complete`: it forwards both arguments
> unchanged and returns its result unchanged. There is no shell-specific
> behaviour despite the name — the two symbols are behaviourally identical
> and must remain distinct exported symbols for ABI compatibility.

> [spec:libedit:def:histedit.el-get-fn]
> int el_get(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-get-fn]
> Narrow (multibyte) variant of the parameter-retrieval varargs entry
> point. Returns 0 on success and -1 on failure, including for any op it
> does not recognise.
> If `el` is `NULL`, return -1 before touching the varargs.
> Then dispatch on `op`. Unlike `el_set`, the "get" direction takes a
> pointer for every value:
> - `EL_PROMPT` (0), `EL_RPROMPT` (12), arg `el_pfunc_t *`: store the
>   installed prompt function. Uses the same underlying getter as
>   `el_wget`, so see the discrepancy noted under
>   `[spec:libedit:sem:histedit.el-wget-fn]`.
> - `EL_PROMPT_ESC` (21), `EL_RPROMPT_ESC` (22), args `el_pfunc_t *` and
>   `char *`: fetch the function and the literal-escape character into a
>   local `wchar_t`, then store that wide character truncated by a C cast
>   to `char`. The truncation is lossy for any escape character outside
>   the single-byte range and is not reported.
> - `EL_EDITOR` (2), `EL_WORDCHARS` (26), arg `const char **`: fetch the
>   wide string via the wide getter, encode it into `el`'s legacy
>   conversion buffer and store the resulting `char *`. If the legacy
>   buffer's byte-capacity field is zero afterwards, force the return value
>   to -1. The stored pointer is owned by libedit, points into the shared
>   legacy conversion buffer, and is invalidated by the next narrow call
>   that uses that buffer (`el_gets`, `el_line`, `el_push`, `el_insertstr`,
>   `el_replacestr`, `el_parse`, and these same `el_set`/`el_get` ops).
> - `EL_TERMINAL` (1), arg `const char **`: store a pointer to the terminal
>   type name libedit is currently using. Owned by libedit, valid until the
>   next `EL_TERMINAL` set or `el_end`. Always returns 0.
> - `EL_SIGNAL` (3), `EL_EDITMODE` (11), `EL_SAFEREAD` (25),
>   `EL_UNBUFFERED` (15), `EL_PREP_TERM` (16), arg `int *`: forwarded to the
>   wide getter. The first four store a boolean and return 0.
>   `EL_PREP_TERM` is not implemented by the wide getter, so
>   `el_get(el, EL_PREP_TERM, &i)` consumes the pointer argument, stores
>   nothing, and returns -1.
> - `EL_GETTC` (17), args `const char *name` and a third pointer whose type
>   depends on the capability: builds the argv `{"gettc", name, out}` and
>   calls the terminal capability getter directly (it is narrow-only in
>   both APIs). For a string capability the third argument must be a
>   `char **` and receives libedit's stored capability string (possibly
>   NULL); for the boolean capabilities `pt`, `km`, `am` and `xn` it must
>   also be a `char **` and receives a pointer to the static string `"yes"`
>   or `"no"`; for every other numeric capability it must be an `int *`.
>   Returns 0 if the name matched, -1 if it did not or if any of the three
>   argv slots is NULL. Passing the wrong pointer type for a capability is
>   a type-confusing store and is undefined behaviour.
> - `EL_GETCFN` (13), arg `el_rfunc_t *`: store the installed character
>   reader, or `EL_BUILTIN_GETCFN` (i.e. `NULL`) if it is still the builtin.
>   Returns 0.
> - `EL_CLIENTDATA` (14), arg `void **`: store the registered client
>   pointer. Returns 0.
> - `EL_GETFP` (18), args `int what` and `FILE **`: store the input stream
>   for `what == 0`, output for 1, error for 2; any other value returns -1
>   without storing. Streams are the caller's; libedit does not own them.
> Every other `op` — notably `EL_GETENV` (27), which the header documents
> as `set/get` — falls through to the default and returns -1. `EL_GETENV`
> is therefore only readable through `el_wget`. This is a
> header/implementation disagreement.
> The varargs list is read only as far as the selected op requires; passing
> the wrong number or type of arguments for an op is undefined behaviour.

> [spec:libedit:def:histedit.el-getc-fn]
> int el_getc(EditLine *, char *)

> [spec:libedit:sem:histedit.el-getc-fn]
> Read one character and deliver it as a single byte.
> Steps: read a wide character with the wide reader (see
> `[spec:libedit:sem:histedit.el-wgetc-fn]` — this drains the pushed-back
> macro queue first, then reads from the input descriptor through the
> installed `EL_GETCFN` reader). Unconditionally store `'\0'` into `*cp`
> first. If the wide read returned `<= 0` (0 = end of file, -1 = error with
> `errno` set), return that value unchanged with `*cp` left as `'\0'`.
> Otherwise convert the wide character to a single byte with `wctob`. If
> that yields `EOF`, set `errno` to `ERANGE` and return -1, leaving `*cp`
> as `'\0'`. Otherwise store the byte in `*cp` and return 1.
> Note the conversion is `wctob`, i.e. single-byte only: in a UTF-8 locale
> every non-ASCII character fails with `ERANGE`, so this function is only
> useful in the C/POSIX locale or for ASCII input. `cp` must be non-NULL;
> `el` must be non-NULL. Both are dereferenced without checks.
> As a side effect the terminal output buffer is flushed and, unless input
> came from the macro queue, the tty is switched to raw mode.

> [spec:libedit:def:histedit.el-gets-fn]
> const char *el_gets(EditLine *, int *)

> [spec:libedit:sem:histedit.el-gets-fn]
> Narrow (multibyte) variant of the line reader.
> Steps: call the wide line reader with the same `nread` pointer (see
> `[spec:libedit:sem:histedit.el-wgets-fn]` for the full editing loop and
> for the `NULL`/`-1`/`0` conventions). If it returned a non-NULL wide
> line, rewrite `*nread` from a wide-character count into a byte count by
> summing the multibyte encoded width of each of the first `*nread` wide
> characters. Then encode the whole wide line (or `NULL`) into `el`'s
> legacy conversion buffer and return the resulting `char *`.
> Return value: the encoded, NUL-terminated line including its trailing
> `'\n'` when the line was terminated by Return, or `NULL` when nothing was
> read. On `NULL`, `*nread` is 0 if end of file was reached cleanly and -1
> if a read error occurred, in which case `errno` holds the original error
> from the failing read (libedit deliberately restores the errno captured
> at the point of failure, after its own cleanup which may have changed it).
> `nread` may be `NULL`, in which case the wide reader substitutes an
> internal scratch `int` and the count is simply discarded; the byte-count
> rewrite is then applied to that scratch value.
> Ownership: the returned pointer is libedit's, points into the shared
> legacy conversion buffer, must not be freed, and is invalidated by the
> next call that uses that buffer — `el_gets`, `el_line`, `el_push`,
> `el_insertstr`, `el_replacestr`, `el_parse`,
> `el_set(EL_EDITOR|EL_WORDCHARS|EL_BIND|EL_TELLTC|EL_SETTC|EL_ECHOTC|EL_SETTY|EL_ADDFN)`,
> `el_get(EL_EDITOR|EL_WORDCHARS)` — and by `el_end`. If the encode step
> runs out of memory it returns `NULL` while `*nread` still holds a
> positive count, which callers must be prepared for.
> `el` must be non-NULL.

> [spec:libedit:def:histedit.el-init-fd-fn]
> EditLine *el_init_fd(const char *, FILE *, FILE *, FILE *, int, int, int)

> [spec:libedit:sem:histedit.el-init-fd-fn]
> Create an `EditLine` handle with explicitly supplied file descriptors,
> for callers whose streams are not backed by the descriptors `fileno`
> would report (e.g. `funopen`/`fopencookie` streams, or a pty read through
> a wrapper).
> Arguments: `prog` is the program name used to match `prog:command`
> prefixes in `.editrc` and in `el_parse`; `fin`, `fout`, `ferr` are the
> input, output and error streams; `fdin`, `fdout`, `fderr` are the
> descriptors libedit uses for `read(2)`, `ioctl(2)` and `tcsetattr(2)`.
> The streams and descriptors are borrowed, never closed or freed by
> libedit; they must outlive the handle.
> Steps (order is load-bearing):
> 1. Zero-allocate the `EditLine`. Return `NULL` on failure.
> 2. Store the three streams and three descriptors.
> 3. Install the default environment accessor: `secure_getenv` where the
>    platform has it, otherwise a local shim that returns `NULL` when the
>    process is set-user/set-group (`issetugid`) and `getenv` otherwise;
>    where neither is available the shim always returns `NULL`.
> 4. Decode `prog` from the current locale's multibyte encoding into the
>    handle's scratch conversion buffer and duplicate the result. If that
>    allocation fails (or `prog` fails to decode), free the handle and
>    return `NULL`. `prog` may not be `NULL` — the decode helper returns
>    `NULL` for a `NULL` input and the duplication then dereferences it,
>    which is undefined behaviour.
> 5. Set the flag word to 0 (no signal handling, tty enabled, editing
>    enabled, buffered, wide history, no safe-read).
> 6. Initialise the terminal module. On failure free the program name and
>    the handle and return `NULL`.
> 7. Initialise, ignoring their return values, the keymacro and map
>    modules. Initialise the tty module; if that fails set the `NO_TTY`
>    flag rather than failing the call (this is how libedit copes with a
>    non-tty input). Then initialise, ignoring return values, the chared,
>    search, hist, prompt, sig and literal modules.
> 8. Initialise the read module; on failure call `el_end` on the
>    half-built handle and return `NULL`.
> 9. Return the handle.
> The default terminal type is resolved lazily from `$TERM` through the
> installed environment accessor; the default editor map is emacs; the
> default prompt functions produce a fixed built-in prompt; history is not
> configured until `EL_HIST` is set. The handle is not thread-safe and must
> not be used concurrently from two threads.

> [spec:libedit:def:histedit.el-init-fn]
> EditLine *el_init(const char *, FILE *, FILE *, FILE *)

> [spec:libedit:sem:histedit.el-init-fn]
> Create an `EditLine` handle from three streams, deriving the descriptors
> from the streams: it calls `el_init_fd(prog, fin, fout, ferr,
> fileno(fin), fileno(fout), fileno(ferr))` and returns its result. See
> `[spec:libedit:sem:histedit.el-init-fd-fn]` for the full initialisation
> sequence, ownership rules and failure modes.
> Note the three `fileno` calls happen before any validation: passing a
> `NULL` stream is undefined behaviour, and passing a stream with no
> underlying descriptor yields `-1` as the descriptor, which libedit will
> then use for `read`/`ioctl` and which shows up as a tty-init failure and
> hence the `NO_TTY` flag.
> Returns the new handle, or `NULL` on allocation failure or terminal
> initialisation failure. The handle must be released with `el_end`.

> [spec:libedit:def:histedit.el-insertstr-fn]
> int el_insertstr(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-insertstr-fn]
> Narrow variant of "insert string at cursor": decode `str` from the
> current locale's multibyte encoding into `el`'s legacy conversion buffer
> and pass the result to the wide insert (see
> `[spec:libedit:sem:histedit.el-winsertstr-fn]`), returning its result.
> If `str` is `NULL`, or is not a valid multibyte string in the current
> locale, or the conversion buffer cannot be grown, the decode yields
> `NULL` and the wide insert returns -1.
> Returns 0 on success, -1 if the string was NULL/empty/undecodable or did
> not fit and the buffer could not be grown.
> Side effect worth noting: it overwrites the shared legacy conversion
> buffer, invalidating any pointer previously returned by `el_gets` or
> `el_line`.

> [spec:libedit:def:histedit.el-line-fn]
> const LineInfo *el_line(EditLine *)

> [spec:libedit:sem:histedit.el-line-fn]
> Return the narrow view of the line currently being edited.
> Steps: obtain the wide view (a pointer to the internal wide line state).
> Take the address of the handle's cached narrow `LineInfo`. If the
> `FROM_ELLINE` flag is already set — meaning this call is reentrant, i.e.
> it was reached from the `EL_RESIZE` callback that this function itself
> invokes — return the cached struct immediately without recomputing.
> Otherwise set `FROM_ELLINE`, then:
> - encode the whole wide buffer into `el`'s legacy conversion buffer and
>   store the result as `buffer`;
> - compute `cursor` as `buffer` plus the summed multibyte encoded widths
>   of the wide characters in `[wide_buffer, wide_cursor)`;
> - compute `lastchar` as `buffer` plus the summed encoded widths of the
>   wide characters in `[wide_buffer, wide_lastchar)`;
> - if an `EL_RESIZE` callback is installed, invoke it with its registered
>   argument (this is why the reentrancy guard exists);
> - clear `FROM_ELLINE`.
> Return a pointer to the handle's cached `LineInfo`.
> Ownership and lifetime: the `LineInfo` and the bytes it points at are
> libedit's. Do not free either. `buffer` is NOT guaranteed NUL-terminated
> at `lastchar` — the region of interest is `[buffer, lastchar)` — although
> the encode step does write a terminator at the end of whatever it
> encoded. Every field is invalidated by the next call that touches the
> shared legacy conversion buffer (`el_gets`, `el_push`, `el_insertstr`,
> `el_replacestr`, `el_parse`, the string-valued `el_set`/`el_get` ops) and
> by any edit to the line (`el_insertstr`, `el_deletestr`,
> `el_deletestr1`, `el_replacestr`, `el_cursor`, `el_gets`).
> If the encode fails (out of memory) `buffer` is set to `NULL` and
> `cursor`/`lastchar` become `NULL + offset`; there is no error return.
> `el` must be non-NULL. Intended to be called after `el_gets` or from
> within an `EL_ADDFN` editor function.

> [spec:libedit:def:histedit.el-parse-fn]
> int el_parse(EditLine *, int, const char **)

> [spec:libedit:sem:histedit.el-parse-fn]
> Narrow variant of the builtin-command dispatcher. Decodes the whole
> `argv` array (`argc` elements) from multibyte into a freshly allocated
> wide `argv` backed by `el`'s legacy conversion buffer; on decode or
> allocation failure returns -1. Otherwise calls the wide dispatcher (see
> `[spec:libedit:sem:histedit.el-wparse-fn]`), frees the wide argv array
> (the strings themselves live in the conversion buffer and are not freed
> individually), and returns the dispatcher's result.
> Return values, from the wide dispatcher: -1 if `argc < 1` or the command
> name is unknown; 0 if a `prog:` prefix was present and did not match this
> handle's program name, or the prefix was empty (`":cmd"`), or the
> temporary allocation for the prefix failed, or the command ran and
> returned 0; otherwise the negated return of the command, so a command
> that fails with -1 makes `el_parse` return 1.
> `NULL` entries inside `argv` are passed through the decoder as `NULL`
> wide pointers; the dispatcher dereferences `argv[0]`, so `argv[0]` must
> not be `NULL`.
> Side effect: overwrites the shared legacy conversion buffer.

> [spec:libedit:def:histedit.el-push-fn]
> void el_push(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-push-fn]
> Narrow variant of "push a string back onto the input queue": decode `str`
> from the current locale's multibyte encoding into `el`'s legacy
> conversion buffer and pass the result to the wide push (see
> `[spec:libedit:sem:histedit.el-wpush-fn]`). This works correctly in
> single-byte locales too, so there is no separate byte path.
> If `str` is `NULL` or is not a valid multibyte string, the decode yields
> `NULL` and the wide push beeps instead of pushing.
> Returns nothing; overflow of the macro stack and decode failure are both
> reported only as a beep. Overwrites the shared legacy conversion buffer,
> invalidating any pointer previously returned by `el_gets`/`el_line`.

> [spec:libedit:def:histedit.el-replacestr-fn]
> int el_replacestr(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-replacestr-fn]
> Narrow variant of "replace the whole line": decode `str` from the current
> locale's multibyte encoding into `el`'s legacy conversion buffer and pass
> the result to the wide replace (see
> `[spec:libedit:sem:histedit.el-wreplacestr-fn]`), returning its result.
> Returns 0 on success, -1 if `str` is `NULL`, empty, undecodable, or does
> not fit and the buffer could not be grown.
> Overwrites the shared legacy conversion buffer.

> [spec:libedit:def:histedit.el-reset-fn]
> void el_reset(EditLine *)

> [spec:libedit:sem:histedit.el-reset-fn]
> Return the terminal and the editor to a known state after the
> application has disturbed them (for instance after running a child
> process, or after an error).
> Two steps, in order: put the tty back into cooked mode (a no-op if it is
> already cooked or if the `NO_TTY` flag is set), then reset the character
> editing state — cursor, `lastchar` and the buffer start pointer are reset
> to an empty line, the undo/redo state, the vi command state and the
> argument/`doingarg` state are cleared, and the kill-buffer mark is
> reset. It does not clear the pushed-back macro queue, does not touch
> history, and does not redraw.
> Returns nothing. `el` must be non-NULL — unlike `el_end`, this function
> has no NULL check, so `el_reset(NULL)` is undefined behaviour.
> Invalidates any previously returned `LineInfo`/`LineInfoW`.

> [spec:libedit:def:histedit.el-resize-fn]
> void el_resize(EditLine *)

> [spec:libedit:sem:histedit.el-resize-fn]
> Re-read the terminal window size and, if it changed, propagate the new
> geometry through libedit's display state. Must be called by the
> application when the terminal is resized unless `EL_SIGNAL` is enabled,
> in which case libedit's own `SIGWINCH` handler arranges it.
> Steps: build a signal set containing exactly `SIGWINCH` and block it with
> `sigprocmask(SIG_BLOCK, ...)`, saving the old mask. Query the terminal
> size (from the descriptor via `TIOCGWINSZ`, falling back to the terminal
> capability values and then to the `LINES`/`COLUMNS` environment variables
> read through the installed environment accessor); if the query reports a
> change, apply it — which reallocates the display and visual line arrays,
> re-derives the wrap behaviour, and invokes the `EL_RESIZE` callback if
> one is installed. Finally restore the saved signal mask with
> `sigprocmask(SIG_SETMASK, ...)`.
> Returns nothing; allocation failures inside the resize are not reported.
> `el` must be non-NULL. Not async-signal-safe: it must be called from
> normal context, not from a signal handler.

> [spec:libedit:def:histedit.el-rfunc-t-edit-line-wchar-t]
> typedef int (*el_rfunc_t)(EditLine *, wchar_t *)

> [spec:libedit:def:histedit.el-set-fn]
> int el_set(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-set-fn]
> Narrow (multibyte) variant of the parameter-setting varargs entry point.
> Returns 0 on success and -1 on failure, including for any op it does not
> recognise.
> If `el` is `NULL`, return -1 before touching the varargs. Then dispatch
> on `op`. The ops it handles, with the narrow argument shapes, are:
> - `EL_PROMPT` (0), `EL_RPROMPT` (12), arg `char *(*)(EditLine *)`:
>   install the (left / right) prompt function with escape character 0 and
>   the prompt marked *narrow*, so libedit will encode the returned
>   multibyte string itself. A `NULL` function restores the built-in
>   default prompt for that slot. Always returns 0.
> - `EL_PROMPT_ESC` (21), `EL_RPROMPT_ESC` (22), args
>   `char *(*)(EditLine *)` and `int`: as above, but the second argument is
>   the character that starts and ends a run of literal (zero-width)
>   prompt text. It is read as `int` (default argument promotion) and
>   stored directly. Always returns 0.
> - `EL_RESIZE` (23), args `void (*)(EditLine *, void *)` and `void *`:
>   record the resize callback and its opaque argument. Always returns 0.
>   The callback is invoked from `el_resize`, from buffer growth, and from
>   `el_line`.
> - `EL_ALIAS_TEXT` (24), args `const char *(*)(void *, const char *)` and
>   `void *`: record the alias-expansion callback and its argument. Always
>   returns 0.
> - `EL_TERMINAL` (1), arg `const char *`: forwarded verbatim (no decoding
>   — the terminal type is bytes) to the wide setter, which reloads the
>   capability database. See `[spec:libedit:sem:histedit.el-wset-fn]`.
> - `EL_EDITOR` (2), `EL_WORDCHARS` (26), arg `const char *`: decoded into
>   the legacy conversion buffer and forwarded to the wide setter.
> - `EL_SIGNAL` (3), `EL_EDITMODE` (11), `EL_SAFEREAD` (25),
>   `EL_UNBUFFERED` (15), `EL_PREP_TERM` (16), arg `int`: forwarded to the
>   wide setter.
> - `EL_BIND` (4), `EL_TELLTC` (5), `EL_SETTC` (6), `EL_ECHOTC` (7),
>   `EL_SETTY` (8), args: a `NULL`-terminated list of `const char *`.
>   At most 18 strings are read (slots 1..18 of a 20-entry array); the
>   scan stops at the first `NULL` or after 18 arguments, whichever comes
>   first, and any further arguments are silently ignored. Slot 0 and the
>   terminating slot are set to `NULL`, the array is decoded into a
>   freshly allocated wide argv (returning -1 if that fails), slot 0 is
>   overwritten with the command name (`L"bind"`, `L"telltc"`, `L"settc"`,
>   `L"echotc"`, `L"setty"` respectively) and the corresponding handler is
>   called with `argc` equal to the number of strings read plus one for
>   slot 0. The wide argv array is then freed. The handler's return value
>   (0 or -1) is the result.
> - `EL_ADDFN` (9), args `const char *name`, `const char *help`,
>   `unsigned char (*)(EditLine *, wint_t)`: the two strings are decoded
>   together into a wide argv (-1 if that fails) and the editor function is
>   registered under `name` with description `help`; the array is then
>   freed. Registration duplicates both strings, so the caller keeps
>   ownership of its own. Returns -1 if any of the three arguments is
>   `NULL` or an allocation fails, else 0.
> - `EL_HIST` (10), args `int (*)(void *, HistEvent *, int, ...)` and
>   `void *`: record the history access function and the handle to pass it
>   (normally `history` and the `History *` from `history_init`), then
>   unconditionally set the `NARROW_HISTORY` flag, which makes libedit call
>   the function with a narrow `HistEvent` and decode the returned strings
>   from multibyte. Always returns 0. Note the asymmetry with the wide
>   setter, which only ever *clears* that flag and only in a single-byte
>   locale — so a narrow `EL_HIST` followed by a wide `EL_HIST` in a
>   multibyte locale leaves the narrow flag set and misinterprets the wide
>   strings.
> - `EL_GETCFN` (13), arg `el_rfunc_t`: forwarded to the wide setter.
>   `EL_BUILTIN_GETCFN` (`NULL`) restores the builtin reader. Always 0.
> - `EL_CLIENTDATA` (14), arg `void *`: forwarded; stored verbatim, not
>   owned by libedit. Always 0.
> - `EL_SETFP` (19), args `int what` and `FILE *`: forwarded. 0 for
>   `what` in {0,1,2}, -1 otherwise.
> - `EL_REFRESH` (20), no further args: clear the recorded display state,
>   redraw the prompt and line, and flush the terminal output buffer.
>   Returns 0. (Handled inline here rather than forwarded, but identical
>   to the wide setter's behaviour.)
> Every other `op` returns -1. In particular `EL_GETENV` (27) is NOT
> handled by the narrow setter even though the header documents it as
> `set/get`; it can only be set through `el_wset`. This is a
> header/implementation disagreement.
> Reading the wrong number or type of varargs for an op is undefined
> behaviour. Note also that on the `EL_BIND` family and `EL_ADDFN` failure
> paths the function jumps to the common exit, so `va_end` is always
> reached.

> [spec:libedit:def:histedit.el-source-fn]
> int el_source(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-source-fn]
> Read an `.editrc`-format file and execute each line as a builtin command.
> Filename resolution: if `fname` is non-`NULL` it is used as given. If it
> is `NULL`, consult `$EDITRC` through the handle's installed environment
> accessor (which returns `NULL` for set-uid/set-gid processes under the
> default accessor) and use its value if set. If `EDITRC` is unset,
> consult `$HOME`; if `HOME` is unset, return -1. Otherwise allocate
> `strlen(HOME) + sizeof("/.editrc")` bytes and format `HOME` followed by
> `"/.editrc"` — except that when `HOME` is the empty string the leading
> `/` is skipped, producing the relative path `.editrc`. Return -1 if that
> allocation fails.
> If the resulting name is the empty string, return -1. Nothing leaks here:
> the return does not free `path`, but it is only reachable with `path`
> still NULL, because a constructed path is never empty — it is either
> `$HOME/.editrc` or, when HOME is empty, the relative `.editrc`. An empty
> name therefore came from the caller or from an empty `EDITRC`, neither of
> which allocates.
> Open the file for reading; if that fails, free the allocated path and
> return -1.
> Then loop with `getline`: skip a line consisting only of `'\n'`; strip a
> single trailing `'\n'`; decode the line from multibyte into the handle's
> *scratch* conversion buffer, skipping the line entirely if the decode
> fails; skip leading whitespace (`iswspace`); skip the line if the first
> non-space character is `'#'`; otherwise dispatch the line through the
> tokenizer and the builtin command dispatcher. If the dispatch returns
> -1 the loop stops immediately.
> Finally free the `getline` buffer, free the allocated path, close the
> file, and return the last dispatch result: 0 if every line was
> dispatched without error (or the file was empty or contained only blanks
> and comments), -1 if a line named an unknown command, or +1 if a
> recognised command reported failure.
> Header/implementation disagreement: the comment above the declaration in
> `histedit.h` says "Source named file or $PWD/.editrc or $HOME/.editrc",
> but the implementation never looks at `$PWD` or the current directory —
> the fallback chain is `$EDITRC` then `$HOME/.editrc` only. The manual
> page matches the implementation.
> `el` must be non-NULL. Uses the *scratch* conversion buffer, not the
> legacy one, so it does not invalidate `el_gets`/`el_line` results.

> [spec:libedit:def:histedit.el-wget-fn]
> int el_wget(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-wget-fn]
> Wide-character parameter-retrieval varargs entry point, and the place
> where the actual state lives — the narrow `el_get` marshals into this for
> most ops. Returns 0 on success and -1 on failure.
> If `el` is `NULL`, return -1 before touching the varargs. Then dispatch:
> - `EL_PROMPT` (0), `EL_RPROMPT` (12), arg `el_pfunc_t *`: store the
>   installed prompt function for that slot. Returns -1 if the pointer is
>   `NULL`, else 0.
> - `EL_PROMPT_ESC` (21), `EL_RPROMPT_ESC` (22), args `el_pfunc_t *` and
>   `wchar_t *`: store the function and the literal-escape character.
>   Returns -1 if the function pointer is `NULL`; the `wchar_t *` may be
>   `NULL`, in which case only the function is stored.
>   **Implementation bug the port must decide about deliberately:** the
>   getter selects the left prompt only when `op == EL_PROMPT` exactly, so
>   `EL_PROMPT_ESC` falls into the `else` branch and reads the *right*
>   prompt's function and escape character. `EL_RPROMPT` and
>   `EL_RPROMPT_ESC` correctly read the right prompt. The setter does not
>   have this bug (it tests both `EL_PROMPT` and `EL_PROMPT_ESC`), so
>   set/get through `EL_PROMPT_ESC` does not round-trip. Since the ABI is
>   frozen this is observable behaviour, not a free fix.
> - `EL_EDITOR` (2), arg `const wchar_t **`: store `L"emacs"` or `L"vi"`
>   according to the active keymap. Returns -1 if the pointer is `NULL` or
>   the map type is neither, else 0. The stored pointer is a static
>   literal, not to be freed.
> - `EL_SIGNAL` (3), arg `int *`: store the raw `HANDLE_SIGNALS` flag bit
>   (i.e. 0 or 1, since that bit is 0x001). Returns 0.
> - `EL_EDITMODE` (11), arg `int *`: store the logical negation of the
>   `EDIT_DISABLED` flag, i.e. 1 when editing is enabled. Returns 0.
> - `EL_SAFEREAD` (25), arg `int *`: store the raw `FIXIO` flag bit, which
>   is 0x100 — so this yields **256**, not 1, when safe-read is on. Callers
>   must treat it as a truth value, not compare against 1. Returns 0.
> - `EL_TERMINAL` (1), arg `const char **`: store libedit's current
>   terminal type name (narrow bytes even in the wide API). Returns 0.
> - `EL_GETTC` (17), args `char *name` and a capability-dependent out
>   pointer: identical to the narrow `el_get(EL_GETTC)` — the arguments are
>   `char *`, not `wchar_t *`, in the wide API too, which contradicts the
>   header comment's `const Char *`. See
>   `[spec:libedit:sem:histedit.el-get-fn]` for the pointer-type rules and
>   the `"yes"`/`"no"` string result for the boolean capabilities.
> - `EL_GETCFN` (13), arg `el_rfunc_t *`: store the installed reader, or
>   `EL_BUILTIN_GETCFN` (`NULL`) if it is the builtin. Returns 0.
> - `EL_CLIENTDATA` (14), arg `void **`: store the client pointer.
>   Returns 0.
> - `EL_UNBUFFERED` (15), arg `int *`: store the `UNBUFFERED` flag
>   normalised to 0 or 1. Returns 0.
> - `EL_GETFP` (18), args `int what` and `FILE **`: store the input (0),
>   output (1) or error (2) stream. Any other `what` returns -1 without
>   storing.
> - `EL_WORDCHARS` (26), arg `const wchar_t **`: store the word-character
>   set string. Returns -1 if the pointer is `NULL`, else 0. The string is
>   libedit's, freed by the next `EL_WORDCHARS` set or by `el_end`.
> - `EL_GETENV` (27), arg `char *(**)(const char *)`: store the installed
>   environment accessor. Returns 0.
> Every other `op` returns -1 — including `EL_PREP_TERM` (16), which is
> set-only.
> Note the local return variable is not initialised before the switch;
> every reachable case assigns it, so this is benign, but a Rust port
> should just return per-arm.

> [spec:libedit:def:histedit.el-wgetc-fn]
> int el_wgetc(EditLine *, wchar_t *)

> [spec:libedit:sem:histedit.el-wgetc-fn]
> Read one wide character, from the pushed-back macro queue if it is
> non-empty, otherwise from the terminal.
> Steps: flush libedit's terminal output buffer first (so any prompt or
> beep is visible before blocking). Then, while the macro stack is
> non-empty (level >= 0): if the current macro's remaining text at the
> current offset is empty, pop the macro (freeing the bottom entry and
> shifting the rest down, resetting the offset to 0) and retry; otherwise
> take one wide character from the macro at the current offset, advance the
> offset, and — if that consumed the last character — pop the macro
> immediately (needed so quote-mode sees the end of the macro), then
> return 1.
> If the macro stack is empty: switch the tty to raw mode; if that fails,
> return 0 (reported as end of file, not as an error). Then call the
> installed character reader — the builtin one, or whatever `EL_GETCFN`
> installed. If it returns a negative value, save the current `errno` in
> the handle so `el_wgets` can restore it after its own cleanup. Return
> the reader's value unchanged.
> Return values: 1 if a character was stored in `*cp`; 0 on end of file (or
> on failure to enter raw mode); -1 on read error, with `errno` set.
> The builtin reader, whose behaviour the manual documents as this
> function's: it reads one byte at a time from the input descriptor and
> feeds them to `mbrtowc` until a complete character is formed. A read
> that fails is retried after `SIGCONT` (which also forces a refresh) and
> after `SIGWINCH` (which re-arms the handlers); if the `EL_SAFEREAD` flag
> is set the first failure additionally attempts to clear non-blocking mode
> on the descriptor and retry, and `EINTR` is treated as retryable. A byte
> that cannot start a valid sequence is discarded and reading restarts; an
> invalid multi-byte sequence discards all but the last byte and restarts
> from it; a sequence that reaches `MB_LEN_MAX` bytes without completing
> sets `errno` to `EILSEQ`, stores `L'\0'` and returns -1. End of file
> stores `L'\0'` and returns 0.
> `el` and `cp` must be non-NULL.

> [spec:libedit:def:histedit.el-wgets-fn]
> const wchar_t *el_wgets(EditLine *, int *)

> [spec:libedit:sem:histedit.el-wgets-fn]
> Read one line, running the full editing loop. This is the core of the
> library.
> Steps:
> 1. If `nread` is `NULL`, substitute a pointer to a local `int`. Set
>    `*nread = 0` and clear the handle's saved read errno.
> 2. If the `NO_TTY` flag is set, reset `lastchar` to the buffer start and
>    fall into the non-editing reader (step 8).
> 3. If `FIONREAD` is available and the tty is currently in cooked mode and
>    the macro stack is empty, query the number of bytes pending on the
>    input descriptor; if zero, switch to raw mode, and if that fails set
>    `errno = 0`, `*nread = 0` and return `NULL`.
> 4. If the `UNBUFFERED` flag is clear, run the read-prepare sequence: arm
>    the signal handlers if `EL_SIGNAL` is enabled; if `NO_TTY` is set stop
>    here; otherwise, when unbuffered-and-editing, enter raw mode; then
>    re-read the window size (`el_resize`), clear the recorded display,
>    reset the char state, and redraw the prompt.
> 5. If the `EDIT_DISABLED` flag is set: when buffered, reset `lastchar` to
>    the buffer start; flush; then fall into the non-editing reader.
> 6. Editing loop, running while `num == -1`: read the next command
>    (`el_wgetc` plus keymap lookup, expanding multi-character key
>    sequences and pushing string bindings back onto the macro queue);
>    on EOF/error break out. Skip commands whose index is out of range.
>    Record the command and character as "this command"; in vi command
>    mode append the character to the redo buffer (or back the redo
>    pointer up one on `vi-delete-prev-char` over a printable). Invoke the
>    bound function and switch on its return code:
>    `CC_CURSOR` (5) refresh the cursor only; `CC_REDISPLAY` (8) clear the
>    drawn lines and the recorded display, then fall through to
>    `CC_REFRESH` (4) which redraws; `CC_REFRESH_BEEP` (9) redraws and
>    beeps; `CC_NORM` (0) does nothing; `CC_ARGHACK` (3) continues the loop
>    *without* resetting the numeric argument state; `CC_EOF` (2) sets
>    `num = 0` when buffered, and when unbuffered and nothing has been read
>    yet appends a literal `^D` to the buffer, moves the cursor after it
>    and sets `num = 1`; `CC_NEWLINE` (1) sets `num` to the current line
>    length in wide characters; `CC_FATAL` (7) clears the display, resets
>    the char state, frees the entire macro stack and redraws the prompt,
>    leaving `num` at -1 so the loop continues; `CC_ERROR` (6) and any
>    unknown value beep and flush. After each command (except the
>    `CC_ARGHACK` path) the numeric argument is reset to 1, `doingarg` is
>    cleared, and the vi pending action is set to NOP. If `UNBUFFERED` is
>    set the loop exits after a single command.
> 7. Flush the terminal output buffer. If buffered, run the read-finish
>    sequence (return the tty to cooked mode and disarm the signal handlers
>    if `EL_SIGNAL` is on) and set `*nread` to `num`, or 0 if `num` is
>    still -1. If unbuffered, set `*nread` to the current line length.
> 8. Non-editing reader (steps 2 and 5): read characters one at a time
>    directly through the installed reader into `lastchar`, growing the
>    buffer as needed, stopping on the first `'\r'` or `'\n'`, immediately
>    if `UNBUFFERED` is set, or on EOF/error. If the read failed with
>    `EINTR`, discard everything read by resetting `lastchar` to the buffer
>    start. Point the cursor at `lastchar`, NUL-terminate there, set
>    `*nread` to the length, and return the buffer if the length is
>    non-zero, else `NULL`.
> 9. Final result: if `*nread` is 0 and the loop ended because of EOF or
>    error (`num == -1`), set `*nread = -1`, restore `errno` from the saved
>    read errno if one was recorded, and return `NULL`. If `*nread` is 0
>    from a clean EOF, return `NULL` with `*nread == 0`. Otherwise return
>    the internal line buffer.
> Return contract: non-`NULL` means a line was produced; a line terminated
> by Return includes the trailing `L'\n'` and is NUL-terminated (the
> newline command appends both). `NULL` with `*nread == 0` means end of
> input; `NULL` with `*nread == -1` means a read error, with `errno` set to
> the original failure.
> Ownership: the returned pointer is libedit's internal line buffer. It
> must not be freed and does not survive the next `el_wgets`, any editing
> call, or any operation that grows the line buffer. Copy it if you need
> it. Not reentrant: an `EL_ADDFN` function must not call `el_wgets`.

> [spec:libedit:def:histedit.el-winsertstr-fn]
> int el_winsertstr(EditLine *, const wchar_t *)

> [spec:libedit:sem:histedit.el-winsertstr-fn]
> Insert a wide string into the line at the cursor.
> Steps: if `s` is `NULL` or has length 0, return -1. If
> `lastchar + len >= limit`, try to grow the line buffer (and, in lockstep,
> the undo, redo, kill and history buffers) by at least `len`; if the grow
> fails, return -1. Note the grow also invokes the `EL_RESIZE` callback and
> moves every internal pointer.
> Then open a gap of `len` characters at the cursor: if the cursor is
> before `lastchar`, copy the range `[cursor, lastchar]` up by `len`
> working backwards; then advance `lastchar` by `len`. (This inner routine
> re-checks the limit and may itself refuse to grow, in which case it
> returns without opening the gap — an over-long insert then writes past
> `lastchar`; the outer check makes this unreachable in practice.)
> Finally copy the characters of `s` into the gap, advancing the cursor
> once per character, so the cursor ends up immediately after the inserted
> text. The buffer is not re-NUL-terminated by this function.
> Returns 0 on success, -1 on `NULL`/empty input or on failure to grow.
> `el` must be non-NULL. Invalidates any previously returned
> `LineInfo`/`LineInfoW`.

> [spec:libedit:def:histedit.el-wline-fn]
> const LineInfoW *el_wline(EditLine *)

> [spec:libedit:sem:histedit.el-wline-fn]
> Return the wide view of the line currently being edited. The
> implementation is a pointer cast: it returns the address of the handle's
> internal `el_line` structure reinterpreted as a `const LineInfoW *`. The
> internal structure's first three members are, in order, `buffer`,
> `cursor` and `lastchar` as `wchar_t *`, which is why the cast is
> layout-compatible; a Rust port must keep that field order and
> representation in the exported struct.
> Consequences the caller can observe:
> - There is no copying and no allocation, so the call is free and always
>   succeeds; there is no error return.
> - The returned struct is *live*: its fields track the internal state, so
>   an edit performed after the call changes what the same pointer reports.
>   This differs from `el_line`, which snapshots into a cached struct.
> - `buffer` is not NUL-terminated at `lastchar`; the valid region is
>   `[buffer, lastchar)` and the cursor satisfies
>   `buffer <= cursor <= lastchar`.
> - The pointer and everything it points at are libedit's; do not free
>   them. The character pointers are invalidated whenever the line buffer
>   is reallocated (buffer growth from `el_winsertstr`,
>   `el_wreplacestr`, insertion during editing) and by `el_end`.
> `el` must be non-NULL; the implementation takes the address of a member
> of `*el` without dereferencing it, so `el_wline(NULL)` returns a small
> bogus pointer rather than crashing — undefined behaviour that a Rust port
> should not attempt to reproduce.
> Intended to be called after `el_wgets` or from an `EL_ADDFN` function.

> [spec:libedit:def:histedit.el-wparse-fn]
> int el_wparse(EditLine *, int, const wchar_t **)

> [spec:libedit:sem:histedit.el-wparse-fn]
> Dispatch one already-tokenized builtin editor command, the same set
> `.editrc` understands.
> Steps: if `argc < 1`, return -1. Search `argv[0]` for a `':'`. If there
> is one:
> - if it is the very first character (`":cmd"`), return 0 — the command is
>   silently ignored;
> - otherwise copy the text before the colon into a temporary
>   zero-initialised buffer (returning 0 if that allocation fails), advance
>   the working pointer past the colon, and glob-match the handle's program
>   name against that prefix. The match is the internal `el_match`, which
>   is a shell-style wildcard match (`*`, `?`, `[...]`) against the `prog`
>   string given to `el_init`. Free the temporary. If it does not match,
>   return 0.
> If there is no colon, the working pointer is `argv[0]` itself.
> Then compare the working pointer against the builtin command table, in
> this order: `bind`, `echotc`, `edit`, `history`, `telltc`, `settc`,
> `setty`. On a match, call the corresponding handler with the full
> `argc`/`argv` (including `argv[0]`) and return the **negated** handler
> result. Handlers return 0 for success and -1 for failure, so this yields
> 0 and +1 respectively. If no name matches, return -1.
> Summary of return values: -1 for `argc < 1` or an unknown command; 0 for
> a non-matching or empty `prog:` prefix, an allocation failure while
> parsing the prefix, or a command that succeeded; +1 for a command that
> reported failure. Note that an allocation failure is indistinguishable
> from a prefix mismatch.
> `argv[0]` must be non-`NULL`; entries beyond `argc - 1` are not read by
> this function but individual handlers scan for a `NULL` terminator, so
> callers should supply one. `el` must be non-NULL.

> [spec:libedit:def:histedit.el-wpush-fn]
> void el_wpush(EditLine *, const wchar_t *)

> [spec:libedit:sem:histedit.el-wpush-fn]
> Push a wide string onto the macro (pushed-back input) stack, so the next
> reads consume it before touching the terminal.
> Steps: if `str` is non-`NULL` and the stack has room (current level + 1
> is below the fixed maximum, `EL_MAXMACRO` = 10), increment the level and
> duplicate the string into that slot; if the duplication succeeds, return.
> Otherwise (str was `NULL`, the stack was full, or the allocation failed)
> undo the level increment if one happened, then beep and flush the
> terminal output buffer.
> Returns nothing: overflow and allocation failure are reported to the
> *user* as a beep, never to the caller. libedit owns the duplicate and
> frees it when the macro is fully consumed, when a `CC_FATAL` command
> clears the stack, or at `el_end`; the caller keeps ownership of `str`.
> Pushes queue, they do not nest: `el_wpush` appends at `macro[++level]`
> while `el_wgetc` always reads `macro[0]` and `read_pop` frees the front
> and shifts the rest down. So a string pushed while an earlier macro is
> still draining is consumed *after* it, not before — first in, first out,
> despite the field being named a level. `el_wgetc` pops each entry as
> soon as its last character is taken. `el` must be non-NULL.

> [spec:libedit:def:histedit.el-wreplacestr-fn]
> int el_wreplacestr(EditLine *, const wchar_t *)

> [spec:libedit:sem:histedit.el-wreplacestr-fn]
> Replace the entire contents of the line with a wide string.
> Steps: if `s` is `NULL` or has length 0, return -1. If
> `buffer + len >= limit`, try to grow the line buffer by at least `len`;
> if that fails, return -1. Copy `len` characters from `s` over the start
> of the buffer, write a NUL at `buffer[len]`, and set `lastchar` to
> `buffer + len`. Then, if the cursor now lies past `lastchar`, move it
> back to `lastchar`; the cursor is otherwise left where it was, so it
> keeps its old *offset*, not its old character.
> Returns 0 on success, -1 on `NULL`/empty input or failure to grow.
> Unlike the rest of the editing API this function does NUL-terminate the
> buffer. `el` must be non-NULL. Invalidates any previously returned
> `LineInfo`/`LineInfoW`.

> [spec:libedit:def:histedit.el-wset-fn]
> int el_wset(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-wset-fn]
> Wide-character parameter-setting varargs entry point, and the place where
> the state actually changes — the narrow `el_set` marshals into this for
> most ops. Returns 0 on success and -1 on failure; the result variable
> starts at 0, so any op that falls through without assigning returns 0.
> If `el` is `NULL`, return -1 before touching the varargs. Then dispatch:
> - `EL_PROMPT` (0), `EL_RPROMPT` (12), arg `el_pfunc_t`
>   (`wchar_t *(*)(EditLine *)`): install the left / right prompt function
>   with escape character 0 and the prompt marked *wide*. A `NULL` function
>   restores the built-in default for that slot (a fixed prompt for the
>   left, an empty string for the right). The cached prompt position is
>   reset to (0,0). Always returns 0.
> - `EL_PROMPT_ESC` (21), `EL_RPROMPT_ESC` (22), args `el_pfunc_t` and
>   `int` (received as `int` because of default argument promotion, then
>   narrowed to `wchar_t`): as above but records the literal-escape
>   character, which brackets runs of prompt text that occupy no columns
>   (terminal escape sequences). Always returns 0.
> - `EL_RESIZE` (23), args `void (*)(EditLine *, void *)` and `void *`:
>   record the resize callback and its argument. Always 0.
> - `EL_ALIAS_TEXT` (24), args `const char *(*)(void *, const char *)` and
>   `void *`: record the alias-expansion callback and its argument.
>   Always 0. Note the callback signature is narrow (`char`) even here.
> - `EL_TERMINAL` (1), arg `char *`: reload the terminal capabilities. A
>   `NULL` argument means "use `$TERM`" through the installed environment
>   accessor; a `NULL` or empty `$TERM` means `"dumb"`. The literal type
>   `"emacs"` additionally sets the `EDIT_DISABLED` flag. `SIGWINCH` is
>   blocked across the reload. If the capability database cannot be read or
>   has no entry for the type, a diagnostic is printed to the error stream
>   and a hardcoded 80-column dumb terminal is configured. Returns -1 if
>   the display arrays could not be allocated, and -1 *also* whenever that
>   capability lookup failed — the fallback terminal is fully usable and
>   the call still reports failure. 0 is returned only when the lookup
>   succeeded. See `[spec:libedit:sem:terminal.terminal-set-fn]` step 14.
> - `EL_EDITOR` (2), arg `wchar_t *`: select the keymap. `L"emacs"` and
>   `L"vi"` are the only accepted values and return 0; anything else
>   returns -1 without changing the map. Switching maps also resets the
>   word-character set to the map's default (`L"_"` for emacs,
>   `L"*?_-.[]~="` for vi).
> - `EL_SIGNAL` (3), arg `int`: set or clear the `HANDLE_SIGNALS` flag.
>   Returns 0. Enabling it makes libedit install handlers for `SIGINT`,
>   `SIGTERM`, `SIGHUP`, `SIGQUIT`, `SIGCONT`, `SIGWINCH` and `SIGTSTP`
>   around each read and restore the tty on receipt.
> - `EL_BIND` (4), `EL_TELLTC` (5), `EL_SETTC` (6), `EL_ECHOTC` (7),
>   `EL_SETTY` (8): a `NULL`-terminated list of `wchar_t *`. Up to 19
>   strings are read into slots 1..19 of a 20-entry array, stopping at the
>   first `NULL`; slot 0 is then set to the command name (`L"bind"`,
>   `L"telltc"`, `L"settc"`, `L"echotc"`, `L"setty"`) and the corresponding
>   handler is called with `argc` equal to the index at which the scan
>   stopped. If the caller supplies 19 non-`NULL` strings the array is not
>   `NULL`-terminated and `argc` is 20; handlers that scan for a terminator
>   then read past the end. Returns the handler's 0 or -1. The argument
>   grammars are those of the corresponding `.editrc` commands
>   (`editrc(5)`).
> - `EL_ADDFN` (9), args `wchar_t *name`, `wchar_t *help`,
>   `unsigned char (*)(EditLine *, wint_t)`: register an editor function.
>   Returns -1 if any argument is `NULL` or if either of the two array
>   reallocations fails, else 0. `name` and `help` are duplicated
>   internally (the duplications are unchecked, so an OOM there leaves
>   `NULL` entries in the help table). The function can then be bound by
>   name with `EL_BIND` and is called with the triggering character; its
>   return value is one of the `CC_*` codes.
> - `EL_HIST` (10), args `hist_fun_t` (`int (*)(void *, HistEventW *, int,
>   ...)`) and `void *`: record the history access function and the opaque
>   handle passed to it — normally `history_w` and the `HistoryW *` from
>   `history_winit`, or `history` and a `History *` if the narrow
>   `el_set(EL_HIST, ...)` was used. Then, **only if `MB_CUR_MAX == 1`**,
>   clear the `NARROW_HISTORY` flag. In a multibyte locale the flag is left
>   untouched, so a previous narrow `el_set(EL_HIST, ...)` is not undone
>   and libedit will keep reinterpreting the wide history strings as
>   multibyte bytes. Always returns 0. libedit does not own the handle and
>   will not free it; the caller must `history_end`/`history_wend` it after
>   `el_end`.
> - `EL_SAFEREAD` (25), arg `int`: set or clear the `FIXIO` flag, which
>   makes the builtin reader tolerate `EINTR` once and attempt to clear
>   non-blocking mode on the input descriptor before giving up. Returns 0.
> - `EL_EDITMODE` (11), arg `int`: non-zero clears `EDIT_DISABLED`
>   (editing on, the default), zero sets it. Returns 0. Note that when the
>   flag is set `el_wgets` bypasses the editing loop entirely and reads a
>   raw line — this is more than the "only an indication" the manual page
>   claims.
> - `EL_GETCFN` (13), arg `el_rfunc_t`: install a replacement character
>   reader; the special value `EL_BUILTIN_GETCFN` (`NULL`) restores the
>   builtin. Always returns 0. The function receives the handle and a
>   `wchar_t *` and must return 1 (stored), 0 (EOF) or -1 (error, `errno`
>   set).
> - `EL_CLIENTDATA` (14), arg `void *`: store an opaque pointer for the
>   application; libedit never dereferences or frees it. Returns 0.
> - `EL_UNBUFFERED` (15), arg `int`: transitioning 0→non-zero sets the
>   `UNBUFFERED` flag and runs the read-prepare sequence (arm signals,
>   enter raw mode when editing is enabled, re-read the window size, reset
>   and redraw); transitioning non-zero→0 clears the flag and runs the
>   read-finish sequence (return to cooked mode, disarm signals). Setting
>   it to the value it already has does nothing. Always returns 0.
> - `EL_PREP_TERM` (16), arg `int`: non-zero puts the tty into raw mode
>   immediately, zero into cooked mode. Errors from the tty layer are
>   discarded. Always returns 0. There is no matching "get".
> - `EL_SETFP` (19), args `int what` and `FILE *`: install a stream and its
>   `fileno` as the input (`what == 0`), output (1) or error (2)
>   stream/descriptor. Any other `what` returns -1. `fileno` is called
>   without a `NULL` check, so passing a `NULL` stream is undefined
>   behaviour. libedit does not take ownership of the stream.
> - `EL_REFRESH` (20), no further args: clear the recorded display state,
>   redraw the prompt and the current line, and flush the terminal output
>   buffer. Returns 0.
> - `EL_WORDCHARS` (26), arg `wchar_t *`: free the previous word-character
>   set and install a duplicate of the argument. Always returns 0 even if
>   the duplication fails (leaving the set `NULL`) or the argument is
>   `NULL` (which is dereferenced by the duplication — undefined
>   behaviour). These characters count as part of a word for the
>   word-motion and word-deletion editor commands.
> - `EL_GETENV` (27), arg `char *(*)(const char *)`: replace the function
>   libedit uses to read environment variables (`TERM`, `HOME`, `EDITRC`,
>   `LINES`, `COLUMNS`). Returns 0. The default is `secure_getenv` or the
>   `issetugid`-guarded shim described under `el_init_fd`. A `NULL`
>   argument installs a `NULL` accessor and every later lookup will call
>   through it — undefined behaviour.
> Every other `op` returns -1.
> Passing the wrong number or type of varargs for an op is undefined
> behaviour.

> [spec:libedit:def:histedit.hist-event]
> typedef struct HistEvent

> [spec:libedit:def:histedit.hist-event-w]
> typedef struct histeventW

> [spec:libedit:def:histedit.histevent-w]
> struct histeventW {
>   int num;
>   const wchar_t *str;
> }

> [spec:libedit:def:histedit.history]
> typedef struct history History

> [spec:libedit:def:histedit.history-end-fn]
> void history_end(History *)

> [spec:libedit:sem:histedit.history-end-fn]
> Destroy a narrow history handle created by `history_init`.
> Steps: if the handle still uses the builtin implementation — detected by
> comparing its stored "next" function pointer against the builtin one —
> run the builtin clear, which unlinks and frees every entry and its
> duplicated string and resets the cursor, count and event-id counter.
> Then free the implementation state pointer and finally the handle
> itself.
> If a custom implementation was installed with `H_FUNC`, the entries are
> *not* cleared (libedit cannot know how) but the implementation state
> pointer is still freed — and because `H_FUNC` never actually stores the
> caller's `ptr` (see `[spec:libedit:sem:histedit.history-fn]`), what gets
> freed is libedit's own builtin state, not the caller's. The caller's
> custom state is never freed by libedit.
> `h` must be non-`NULL`: there is no NULL check and the first thing the
> function does is dereference it. Calling it twice is a double free.
> After this call every `HistEvent.str` previously returned from this
> handle is dangling, except the ones from `H_DEL`/`H_DELDATA`, which the
> caller owns and must free itself.
> Note this is exactly what `history(h, &ev, H_END)` does, except that the
> `H_END` path additionally returns 0.

> [spec:libedit:def:histedit.history-fn]
> int history(History *, HistEvent *, int, ...)

> [spec:libedit:sem:histedit.history-fn]
> The narrow history operation dispatcher. `h` is a handle from
> `history_init`; `ev` is an out-parameter the operation fills in; `fun`
> selects the operation and determines the remaining arguments.
> Before dispatching, `ev` is set to `{num = 0, str = "OK"}`. On any error
> `ev` is overwritten with an error code in `num` and a static English
> message in `str`; the codes are: 0 OK, 1 "unknown error", 2 "malloc()
> failed", 3 "first event not found", 4 "last event not found", 5 "empty
> list", 6 "no next event", 7 "no previous event", 8 "current event is
> invalid", 9 "event not found", 10 "can't read history from file", 11
> "can't write history", 12 "required parameter(s) not supplied", 13
> "history size negative", 14 "function not allowed with other
> history-functions-set the default", 15 "bad parameters".
> Neither `h` nor `ev` may be `NULL`; both are dereferenced unchecked.
> The list model of the builtin implementation, which the operation names
> are relative to: entries are kept in a doubly linked circular list with a
> sentinel; the *newest* entry is at the head. "First" therefore means
> newest, "next" means one step older, "last" means oldest, and "previous"
> means one step newer. Each entry has a monotonically increasing event
> number starting at 1 and an associated `void *` data slot. The maximum
> retained count starts at **0**, so until `H_SETSIZE` is called every
> `H_ENTER` inserts an entry and immediately evicts it — history stays
> empty, and the `HistEvent` returned by that `H_ENTER` holds a pointer to
> the just-freed string (a use-after-free the caller can observe).
> Operations, with their extra arguments:
> - `H_FUNC` (0), args `void *ptr` then ten function pointers in this
>   order: `first`, `next`, `last`, `prev`, `curr` (each
>   `int (*)(void *, HistEvent *)`), `set` (`int (*)(void *, HistEvent *,
>   const int)`), `clear` (`void (*)(void *, HistEvent *)`), `enter` and
>   `add` (each `int (*)(void *, HistEvent *, const char *)`), and `del`
>   (`int (*)(void *, HistEvent *, const int)`). Install a custom history
>   implementation. The "append" anchor is reset to -1 first, then: if any
>   of the eleven values is `NULL`, the call fails — and if a custom
>   implementation was already installed, the builtin one is reinstalled
>   with a fresh empty state first — returning -1 with `ev` set to code 12.
>   Otherwise, if the builtin implementation is currently in use its
>   entries are cleared, and the ten function pointers are copied into the
>   handle. Returns 0.
>   **Implementation bug, load-bearing:** `ptr` is read from the varargs
>   and validated, but is never stored into the handle. The installed
>   functions are therefore called with libedit's own builtin state pointer
>   as their `void *` argument, not the caller's `ptr`. Additionally, the
>   builtin state block itself is leaked (only its entries are freed).
>   The manual page also documents only ten arguments, omitting `del`;
>   the implementation reads eleven.
> - `H_SETSIZE` (1), arg `int`: set the maximum retained entry count.
>   Returns -1 with code 14 if a custom implementation is installed, -1
>   with code 15 if the size is negative, else 0. Existing surplus entries
>   are not evicted until the next `H_ENTER`.
> - `H_GETSIZE` (2), no arg: store the *current* number of entries in
>   `ev->num` — not the configured maximum, despite the symmetry of the
>   names. Returns -1 with code 14 for a custom implementation, -1 with
>   code 13 if the count is below -1 (unreachable), else 0.
> - `H_FIRST` (3): move the cursor to the newest entry and copy it into
>   `ev`. -1 with code 3 if the list is empty.
> - `H_LAST` (4): move the cursor to the oldest entry and copy it into
>   `ev`. -1 with code 4 if empty.
> - `H_PREV` (5): move one step *newer*. -1 with code 6 if the cursor is
>   on the sentinel and the list is non-empty, code 5 if it is empty, code
>   7 if already at the newest.
> - `H_NEXT` (6): move one step *older*. -1 with code 5 if the cursor is on
>   the sentinel, code 6 if already at the oldest.
> - `H_SET` (7), arg `int`: position the cursor on the entry with that
>   event number, searching from the newest if the cursor is not already
>   there. -1 with code 5 if empty, code 9 if not found. `ev` is only
>   written on error.
> - `H_CURR` (8), **no argument** — the header comment `, const int)` is
>   wrong: copy the entry under the cursor into `ev`. -1 with code 8 if the
>   cursor is on the sentinel and the list is non-empty, code 5 if empty.
> - `H_ADD` (9), arg `const char *`: append the string to the entry under
>   the cursor, replacing that entry's string with a newly allocated
>   concatenation and freeing the old one, then copy the entry into `ev`.
>   If the cursor is on the sentinel this degrades to `H_ENTER`. Returns 0,
>   or -1 with code 2 on allocation failure.
> - `H_ENTER` (10), arg `const char *`: insert the string as a new newest
>   entry. If the unique flag is set and the current newest entry has an
>   identical string, do nothing and return 0 — note it compares against
>   the *newest* entry, not the entry under the cursor as the manual claims.
>   Otherwise duplicate the string, assign the next event number, link it
>   at the head, make it the cursor, then evict oldest entries while the
>   count exceeds the maximum. On success the handle's "append anchor" is
>   set to the new event number and 1 is returned; -1 with code 2 on
>   allocation failure.
> - `H_APPEND` (11), arg `const char *`: position the cursor on the entry
>   recorded by the last `H_ENTER` (the append anchor, -1 before any
>   `H_ENTER`, which will not match any entry) and then perform `H_ADD`.
>   Returns the `H_SET` failure (-1, code 5 or 9) or the `H_ADD` result.
> - `H_END` (12): destroy the handle exactly as `history_end` does and
>   return 0. The handle is invalid afterwards.
> - `H_NEXT_STR` (13), arg `const char *`: starting from the cursor and
>   walking towards *older* entries, stop at the first whose string starts
>   with the given prefix, leaving the cursor there and the entry in `ev`.
>   Returns 0, or -1 with code 9 if none matches. (The direction is the
>   opposite of what the name suggests — the function named "next string"
>   walks with `prev`-style steps in the code's own vocabulary; what
>   matters is that `H_NEXT_STR` searches older entries and `H_PREV_STR`
>   searches newer ones.)
> - `H_PREV_STR` (14), arg `const char *`: as above but walking towards
>   *newer* entries.
> - `H_NEXT_EVENT` (15), arg `int`: starting from the cursor, walk towards
>   older entries until one has that event number. 0 or -1 with code 9.
> - `H_PREV_EVENT` (16), arg `int`: as above towards newer entries.
> - `H_LOAD` (17), arg `const char *filename`: read a history file. Opens
>   the file, reads the first line and requires it to be exactly the cookie
>   `"_HiStOrY_V2_\n"` (compared over as many bytes as were read), then for
>   each remaining line strips a trailing newline, decodes it with
>   `strunvis` (the BSD visual-encoding inverse), and enters it with the
>   handle's `enter` function. Returns the number of lines read, or -1 —
>   with `ev` set to code 10 — if the file cannot be opened, is empty, has
>   the wrong cookie, or an allocation or `enter` fails. Because entries
>   are appended in file order and the newest entry is at the head, the
>   file's last line ends up newest.
> - `H_SAVE` (18), arg `const char *filename`: write the whole history.
>   Opens the file with `O_WRONLY|O_CREAT|O_TRUNC` and mode 0600, wraps it
>   in a stream, writes all entries, closes the stream and returns the
>   number written; -1 with code 11 on failure. Note a descriptor leak: if
>   the `fdopen` fails the raw descriptor is returned to the OS unclosed.
> - `H_SAVE_FP` (26), arg `FILE *`: write the whole history to an already
>   open stream, which the caller keeps and must close. Returns the number
>   of entries written, -1 with code 11 on failure.
> - `H_NSAVE_FP` (27), args `size_t n` and `FILE *`: write only the most
>   recent entries. It walks `n` steps from the newest towards the oldest
>   and then writes from there back towards the newest, so it emits
>   **`n + 1`** entries, not `n` — `n == 0` writes one entry. If the
>   history is shorter than that, the walk falls off the end and it writes
>   everything. Returns the number written, -1 with code 11 on failure.
> The three write operations share one routine: if the stream is at
> offset 0 it first writes the cookie line `"_HiStOrY_V2_\n"`; each entry
> is then `strvis`-encoded with `VIS_WHITE` (so whitespace, control and
> non-printable bytes become `\`-escapes and the entry can never contain a
> raw newline) and written followed by `'\n'`. Entries are emitted oldest
> first. The temporary encoding buffer starts at 1024 bytes and grows to
> `(4 * strlen + 1 + 1024)` rounded down to a multiple of 1024 as needed.
> As a side effect the cursor is left on the newest entry. **This encoding
> is the on-disk history format and is frozen by
> `[dec:libedit:no-c-ffi]`;** `[dec:libedit:posix-only-scope]` keeps
> `vis`/`unvis` in the port for exactly this reason.
> - `H_CLEAR` (19): call the implementation's clear function, which for the
>   builtin frees every entry and resets the cursor, the count and the
>   event-id counter to 0 — so event numbers restart at 1. Always
>   returns 0.
> - `H_SETUNIQUE` (20), arg `int`: set or clear the "suppress consecutive
>   duplicates" flag. -1 with code 14 for a custom implementation, else 0.
> - `H_GETUNIQUE` (21): store the flag as 0 or 1 in `ev->num`. -1 with
>   code 14 for a custom implementation, else 0.
> - `H_DEL` (22), arg `int`: delete the entry with that event number.
>   Positions the cursor with the same search as `H_SET` (returning -1 with
>   code 5 or 9 on failure), then **duplicates** the entry's string into
>   `ev->str`, copies its number into `ev->num`, and unlinks and frees the
>   entry. Returns 0. **The caller owns `ev->str` after this call and must
>   free it** — this is the only operation with that ownership, and the
>   duplication is unchecked, so `ev->str` can be `NULL` on OOM. Provided
>   for readline compatibility.
> - `H_NEXT_EVDATA` (23), args `int` and `void **`: like `H_NEXT_EVENT` —
>   walk from the cursor towards older entries to the entry with that
>   number — and additionally, if the pointer is non-`NULL`, store that
>   entry's associated data slot through it. 0, or -1 with code 9.
>   Reaches into the builtin state unconditionally, so it is undefined
>   behaviour with a custom implementation.
> - `H_DELDATA` (24), args `int` and `void **`: position the cursor on the
>   `n`-th entry counting from the **oldest** (0-based; this is a positional
>   index, not an event number, unlike every other numbered operation),
>   then delete it: duplicate its string into `ev->str`, copy its number,
>   store its data slot through the pointer if non-`NULL`, and unlink and
>   free it. Returns 0, or -1 with code 5 (empty) or 9 (out of range). The
>   caller owns `ev->str` and must free it. The pointer value `(void **)-1`
>   is a magic marker meaning "just position the cursor, do not delete";
>   it returns 0 immediately after positioning. Reaches into the builtin
>   state unconditionally — undefined behaviour with a custom
>   implementation.
> - `H_REPLACE` (25), args `const char *line` and `void *data`: overwrite
>   the string and data slot of the entry currently under the cursor.
>   Documented as valid only immediately after `H_NEXT_EVDATA`. Returns -1
>   (with `ev` left as OK) if `line` is `NULL` or the duplication fails,
>   else 0. **It does not free the previous string, so every call leaks
>   it**, and it reaches into the builtin state unconditionally, so it is
>   undefined behaviour with a custom implementation. It also does not
>   check that the cursor is off the sentinel.
> Any other `fun` returns -1 with `ev` set to code 1, "unknown error".
> General return contract: `>= 0` means success (`H_ENTER` returns 1 for an
> insertion and 0 for a suppressed duplicate; the load/save operations
> return counts; everything else returns 0); -1 means failure with details
> in `ev`.
> General ownership: except for `H_DEL` and `H_DELDATA`, `ev->str` points
> either into libedit's storage or at a static error message and must not
> be freed; it is invalidated by any operation that deletes or replaces
> that entry, by `H_CLEAR`, and by `history_end`/`H_END`. Strings passed
> in (`H_ADD`, `H_ENTER`, `H_APPEND`, `H_REPLACE`) are duplicated, so the
> caller keeps ownership.
> Type safety: `History *` and `HistoryW *` are distinct incomplete types
> backed by separately compiled implementations. Passing a `HistoryW *`
> here, or the wrong `HistEvent` variant, is undefined behaviour.
> The header comments for `H_ADD`, `H_ENTER` and `H_APPEND` say
> `const wchar_t *`; for this narrow entry point they are `const char *`.
> The header comments for `H_NEXT_EVDATA`, `H_DELDATA` and `H_REPLACE`
> mention a type `histdata_t` that `histedit.h` does not define — it is
> `void *`, and is only declared in `editline/readline.h`.

> [spec:libedit:def:histedit.history-init-fn]
> History * history_init(void)

> [spec:libedit:sem:histedit.history-init-fn]
> Create a narrow history handle.
> Steps: allocate the handle; return `NULL` on failure. Allocate and
> initialise the builtin implementation state — an empty circular list with
> its sentinel pointing at itself, cursor on the sentinel, current count 0,
> **maximum count 0**, event-id counter 0, unique flag clear — and store it
> in the handle; if that allocation fails, free the handle and return
> `NULL`. Set the "append anchor" (the event number `H_APPEND` targets) to
> -1, and install the ten builtin operation functions. Return the handle.
> Because the maximum starts at 0, a caller must issue
> `history(h, &ev, H_SETSIZE, n)` before any `H_ENTER` will retain
> anything; see `[spec:libedit:sem:histedit.history-fn]`.
> The handle is released with `history_end` or `history(h, &ev, H_END)`.
> It is independent of any `EditLine`: installing it with
> `el_set(el, EL_HIST, history, h)` does not transfer ownership, and
> `el_end` will not free it.
> Not thread-safe; a handle must not be used concurrently.

> [spec:libedit:def:histedit.history-w]
> typedef struct historyW HistoryW

> [spec:libedit:def:histedit.history-w-fn]
> int history_w(HistoryW *, HistEventW *, int, ...)

> [spec:libedit:sem:histedit.history-w-fn]
> The wide-character history operation dispatcher. It is generated from the
> same source as the narrow `history` with the character type set to
> `wchar_t`, so every operation code, argument shape, return value, error
> code, error message, cursor model, ownership rule and quirk described in
> `[spec:libedit:sem:histedit.history-fn]` applies here unchanged, with
> these substitutions and additions:
> - The handle is `HistoryW *` from `history_winit` and the event is
>   `HistEventW *`. Mixing wide and narrow handles or events is undefined
>   behaviour — they are separate implementations with separate state
>   layouts.
> - Every string argument (`H_ADD`, `H_ENTER`, `H_APPEND`, `H_NEXT_STR`,
>   `H_PREV_STR`, `H_REPLACE`) is `const wchar_t *`, and `ev->str` is
>   `const wchar_t *`. Duplication is `wcsdup`, comparison is `wcscmp` /
>   `wcsncmp`, length is `wcslen`. The header comments that say
>   `const wchar_t *` are correct for this entry point.
> - The static error message strings are wide literals with the same
>   wording.
> - `H_LOAD` and the three save operations still read and write **bytes**:
>   the file format is unchanged. On load, each line is `strunvis`-decoded
>   and then decoded from the current locale's multibyte encoding into wide
>   characters; a line that fails to decode is silently skipped rather than
>   aborting the load. On save, each entry is encoded from wide characters
>   to multibyte and then `strvis`-escaped with `VIS_WHITE`. The cookie is
>   the same narrow `"_HiStOrY_V2_\n"`. A wide history file is therefore
>   byte-identical to a narrow one for the same content in the same locale,
>   which is what makes the format portable between the two APIs.
> - The multibyte conversion in load and save uses a **file-static**
>   conversion buffer shared by all `HistoryW` handles in the process. It
>   is never freed (a deliberate one-time leak) and makes `H_LOAD`,
>   `H_SAVE`, `H_SAVE_FP` and `H_NSAVE_FP` non-thread-safe across handles,
>   not merely non-reentrant per handle. A Rust port must reproduce the
>   observable behaviour but should not reproduce the shared mutable
>   global.
> - The `H_FUNC` callback signatures take `HistEventW *` and
>   `const wchar_t *`, and the `H_FUNC` bug — the caller's `ptr` is
>   validated but never stored, so the callbacks receive libedit's builtin
>   state pointer — is present here too.

> [spec:libedit:def:histedit.history-wend-fn]
> void history_wend(HistoryW *)

> [spec:libedit:sem:histedit.history-wend-fn]
> Destroy a wide history handle created by `history_winit`. Behaviourally
> identical to `[spec:libedit:sem:histedit.history-end-fn]` with `wchar_t`
> strings: if the builtin implementation is still installed, clear and free
> every entry and its `wcsdup`ed string; then free the implementation state
> pointer, then the handle.
> `h` must be non-`NULL` (no check). Calling it twice is a double free.
> Invalidates every `HistEventW.str` previously returned from the handle
> except those from `H_DEL`/`H_DELDATA`, which the caller owns.
> Equivalent to `history_w(h, &ev, H_END)` except that `H_END` also
> returns 0.
> Must be given a `HistoryW *`; passing a narrow `History *` is undefined
> behaviour.

> [spec:libedit:def:histedit.history-winit-fn]
> HistoryW * history_winit(void)

> [spec:libedit:sem:histedit.history-winit-fn]
> Create a wide history handle. Behaviourally identical to
> `[spec:libedit:sem:histedit.history-init-fn]` — allocate the handle,
> allocate and initialise an empty builtin state with **maximum count 0**,
> set the `H_APPEND` anchor to -1, install the ten builtin wide operation
> functions — with `wchar_t` strings throughout. Returns the handle, or
> `NULL` if either allocation fails.
> As with the narrow handle, `history_w(h, &ev, H_SETSIZE, n)` must be
> called before `H_ENTER` retains anything.
> Released with `history_wend` or `history_w(h, &ev, H_END)`. Install it
> into an editor with `el_wset(el, EL_HIST, history_w, h)`; that does not
> transfer ownership.

> [spec:libedit:def:histedit.line-info]
> typedef struct lineinfo

> [spec:libedit:def:histedit.line-info-w]
> typedef struct lineinfow

> [spec:libedit:def:histedit.lineinfo]
> struct lineinfo {
>   const char *buffer;
>   const char *cursor;
>   const char *lastchar;
> }

> [spec:libedit:def:histedit.lineinfow]
> struct lineinfow {
>   const wchar_t *buffer;
>   const wchar_t *cursor;
>   const wchar_t *lastchar;
> }

> [spec:libedit:def:histedit.tok-end-fn]
> void tok_end(Tokenizer *)

> [spec:libedit:sem:histedit.tok-end-fn]
> Destroy a narrow tokenizer created by `tok_init`. Frees, in order, the
> duplicated IFS string, the word-space buffer and the argument-vector
> array, then the tokenizer itself.
> `tok` must be non-`NULL`: there is no check and the first action
> dereferences it. Calling it twice is a double free.
> Every `argv` array and every word pointer previously returned by
> `tok_line`/`tok_str` on this tokenizer is dangling afterwards — they all
> point into the freed word-space and argv allocations.
> Returns nothing.

> [spec:libedit:def:histedit.tok-init-fn]
> Tokenizer *tok_init(const char *)

> [spec:libedit:sem:histedit.tok-init-fn]
> Create a narrow tokenizer.
> Steps: allocate the tokenizer; return `NULL` on failure. Duplicate the
> `ifs` string — or the default `"\t \n"` (tab, space, newline) if `ifs` is
> `NULL` — and store it; on failure free the tokenizer and return `NULL`.
> Set the argument count to 0 and the argv capacity to 10, allocate that
> many argv slots and store `NULL` in slot 0; on failure free the IFS copy
> and the tokenizer and return `NULL`. Allocate a 20-character word-space
> buffer and set the write pointer, the current-word start pointer and the
> limit pointer accordingly; on failure free the argv array, the IFS copy
> and the tokenizer and return `NULL`. Clear the flags and set the quoting
> state to "none". Return the tokenizer.
> `ifs` is copied, so the caller keeps ownership of it; the copy is freed
> by `tok_end`. The tokenizer owns everything it later hands back through
> `tok_line`/`tok_str`.
> The IFS characters are the field separators used only in unquoted state;
> they have no effect inside quotes.

> [spec:libedit:def:histedit.tok-line-fn]
> int tok_line(Tokenizer *, const LineInfo *, int *, const char ***, int *, int *)

> [spec:libedit:sem:histedit.tok-line-fn]
> Split a line into words using simplified `sh(1)` quoting, and report
> which word the cursor is in.
> Input is a `LineInfo`: `buffer` is the text, `lastchar` bounds it (the
> text need not be NUL-terminated), and `cursor` marks the cursor. Note
> `tok_line` does **not** reset the tokenizer — it appends to whatever
> state is there, which is how multi-line continuation works. The caller
> must call `tok_reset` before starting a fresh line unless it is
> continuing one.
> Algorithm: walk a pointer from `buffer` forward one character at a time.
> At the top of each iteration, if the pointer has reached or passed
> `lastchar`, replace it with a pointer to a static empty string, so the
> loop then sees a `'\0'` (and keeps seeing one, since the replacement is
> redone every iteration). If the pointer equals `cursor`, record the
> current argument index and the current offset within the word being
> built.
> A five-state quoting machine drives the character handling. The states
> are: none; single (inside `'...'`); double (inside `"..."`); one
> (backslash seen outside quotes — quote exactly the next character); and
> doubleone (backslash seen inside double quotes).
> - `'` — set the "keep this word even if empty" flag and clear the "eat
>   the newline" flag. In state none enter single; in single return to
>   none; in one emit `'` and return to none; in double emit `'`; in
>   doubleone emit `'` and return to double.
> - `"` — same flag updates. In none enter double; in double return to
>   none; in one emit `"` and return to none; in single emit `"`; in
>   doubleone emit `"` and return to double.
> - `\` — same flag updates. In none enter one; in double enter doubleone;
>   in one emit `\` and return to none; in single emit `\`; in doubleone
>   emit `\` and return to double.
> - `\n` — clear "eat". In none, finish successfully. In single or double
>   emit the newline (a quoted newline is literal). In doubleone set
>   "eat", return to double. In one set "eat", return to none.
> - `\0` (end of text) — in none: if "eat" is set, clear it and return 3
>   ("backslash-quoted newline, read another line and call again"); else
>   finish successfully. In single return 1 (unmatched single quote). In
>   double return 2 (unmatched double quote). In doubleone emit `\0` and
>   return to double. In one emit `\0` and return to none.
> - anything else — clear "eat". In none: if the character is in the IFS
>   set, finish the current word; else emit it. In single or double emit
>   it. In doubleone emit a literal `\` *and then* the character, and
>   return to double (so `\x` inside double quotes keeps its backslash
>   unless `x` is one of `'`, `"`, `\`, newline). In one emit it and return
>   to none.
> After each character, grow the word-space buffer by 20 characters if
> fewer than 4 remain, relocating every already-recorded argv pointer, the
> write pointer and the word start; and grow the argv array by 10 slots if
> fewer than 4 remain. Either reallocation failing returns -1.
> Finishing a word means: write a `'\0'` at the write pointer, and if the
> "keep" flag is set or the word is non-empty, record the word start in
> `argv[argc++]`, store `NULL` in the next argv slot, and set the next word
> start to one past the terminator. Then clear the "keep" flag.
> Successful completion: if the cursor was never seen, set the recorded
> argument index and offset to the current ones. Store them through
> `cursorc` and `cursoro` if those are non-`NULL`. Finish the last word,
> store the argv array through `argv` and the count through `argc`, and
> return 0.
> Return values: 0 success; -1 internal error (reallocation failure, or an
> unreachable invalid quoting state); 1 unterminated single quote; 2
> unterminated double quote; 3 line ended with a backslash-escaped newline.
> A positive result means the line is incomplete: read another line and
> call `tok_line` again on the *same* tokenizer without resetting, and the
> words continue accumulating. On any non-zero return, `argc`, `argv`,
> `cursorc` and `cursoro` are left untouched.
> Ownership: `*argv` and the strings it points at belong to the tokenizer.
> Do not free them. They are invalidated by the next `tok_line`, `tok_str`
> or `tok_reset` on the same tokenizer — and note that a *successful*
> continuation call can relocate the whole word-space, so pointers must be
> re-read after every call. `tok_end` frees them all.
> `tok` and `line` must be non-`NULL` and `line->buffer` must be non-`NULL`;
> none is checked. `cursor` outside `[buffer, lastchar]` simply never
> matches, and the cursor is then reported as the end of the line.

> [spec:libedit:def:histedit.tok-reset-fn]
> void tok_reset(Tokenizer *)

> [spec:libedit:sem:histedit.tok-reset-fn]
> Reset a narrow tokenizer to its just-initialised state so a new,
> unrelated line can be tokenized: set the argument count to 0, point both
> the current-word start and the write pointer at the beginning of the
> word-space buffer, clear the flags, and set the quoting state to "none".
> The IFS string, the word-space buffer and the argv array are kept
> allocated at their current (possibly grown) sizes; nothing is freed and
> nothing is zeroed, so stale contents remain readable through previously
> returned pointers although they are logically dead.
> Call this after a `tok_line`/`tok_str` that returned 0, before tokenizing
> the next line — but *not* between the calls that make up a multi-line
> continuation (returns 1, 2 or 3), since resetting would discard the
> partial words.
> Returns nothing. `tok` must be non-`NULL` (no check). Invalidates the
> `argv` array and word pointers from the previous call.

> [spec:libedit:def:histedit.tok-str-fn]
> int tok_str(Tokenizer *, const char *, int *, const char ***)

> [spec:libedit:sem:histedit.tok-str-fn]
> Convenience wrapper over `tok_line` for a NUL-terminated string with no
> cursor. It builds a zeroed `LineInfo` on the stack, sets `buffer` to
> `line`, sets both `cursor` and `lastchar` to the address of the string's
> terminating NUL, and calls `tok_line` with `NULL` for both cursor
> out-parameters.
> Return values, argument semantics, ownership and reset requirements are
> exactly those of `[spec:libedit:sem:histedit.tok-line-fn]`: 0 on success
> with `*argc`/`*argv` filled in, -1 internal error, 1 unmatched single
> quote, 2 unmatched double quote, 3 backslash-escaped newline at end.
> `line` must be non-`NULL` and NUL-terminated; the terminator is located
> with `strchr`, so an unterminated string reads out of bounds.
> Because `cursor == lastchar`, the cursor is always reported as
> end-of-line internally, and the tokenizer state is again *not* reset by
> this call.

> [spec:libedit:def:histedit.tok-wend-fn]
> void tok_wend(TokenizerW *)

> [spec:libedit:sem:histedit.tok-wend-fn]
> Destroy a wide tokenizer created by `tok_winit`. Identical to
> `[spec:libedit:sem:histedit.tok-end-fn]` with `wchar_t` storage: frees
> the duplicated IFS string, the word-space buffer, the argv array and the
> tokenizer, in that order.
> `tok` must be non-`NULL` (no check); double destruction is a double free;
> all previously returned `argv` arrays and word pointers become dangling.
> Must be given a `TokenizerW *` — the narrow and wide tokenizers are
> distinct incomplete types over separately compiled state, and mixing them
> is undefined behaviour.

> [spec:libedit:def:histedit.tok-winit-fn]
> TokenizerW *tok_winit(const wchar_t *)

> [spec:libedit:sem:histedit.tok-winit-fn]
> Create a wide tokenizer. Identical to
> `[spec:libedit:sem:histedit.tok-init-fn]` with `wchar_t` storage: the
> default IFS when the argument is `NULL` is `L"\t \n"`, the IFS is
> duplicated with `wcsdup`, the argv array starts at 10 slots with slot 0
> set to `NULL`, and the word-space buffer starts at 20 `wchar_t`. Returns
> the tokenizer, or `NULL` if any of the three allocations fails, having
> freed whatever had already been allocated.
> The caller keeps ownership of the `ifs` argument; the tokenizer owns
> everything it hands back.

> [spec:libedit:def:histedit.tok-wline-fn]
> int tok_wline(TokenizerW *, const LineInfoW *, int *, const wchar_t ***, int *, int *)

> [spec:libedit:sem:histedit.tok-wline-fn]
> Wide-character line tokenizer. Generated from the same source as
> `tok_line`, so the quoting state machine, the IFS handling, the buffer
> growth policy (20 `wchar_t` at a time with 4 characters of headroom, 10
> argv slots at a time with 4 slots of headroom, relocating every recorded
> pointer), the cursor reporting, the continuation protocol and the
> ownership rules are exactly those of
> `[spec:libedit:sem:histedit.tok-line-fn]`, with these substitutions:
> - the line is a `LineInfoW` of `const wchar_t *` fields, so the input is
>   the wide edit buffer that `el_wline` returns;
> - words are `wchar_t` sequences, `*argv` is `const wchar_t **`, and the
>   IFS membership test is `wcschr`;
> - the sentinel used when the scan passes `lastchar` is the wide empty
>   string `L""`;
> - the quoting metacharacters are still the ASCII `'`, `"`, `\`, `\n` and
>   `\0` code points, compared as wide characters.
> Return values are identical: 0 success, -1 internal error, 1 unmatched
> single quote, 2 unmatched double quote, 3 backslash-escaped newline. On
> any non-zero return none of the four out-parameters is written.
> Does not reset the tokenizer; call `tok_wreset` between unrelated lines.

> [spec:libedit:def:histedit.tok-wreset-fn]
> void tok_wreset(TokenizerW *)

> [spec:libedit:sem:histedit.tok-wreset-fn]
> Reset a wide tokenizer. Identical to
> `[spec:libedit:sem:histedit.tok-reset-fn]`: argument count to 0, word
> start and write pointer back to the beginning of the word-space buffer,
> flags cleared, quoting state set to "none". No memory is freed or
> zeroed and the grown buffer capacities are retained.
> Call it after a successful `tok_wline`/`tok_wstr`, never in the middle of
> a multi-line continuation. `tok` must be non-`NULL` (no check).
> Invalidates the previous call's `argv` and word pointers.

> [spec:libedit:def:histedit.tok-wstr-fn]
> int tok_wstr(TokenizerW *, const wchar_t *, int *, const wchar_t ***)

> [spec:libedit:sem:histedit.tok-wstr-fn]
> Wide-character convenience wrapper over `tok_wline`. Builds a zeroed
> `LineInfoW` on the stack, sets `buffer` to `line`, sets `cursor` and
> `lastchar` to the address of the string's terminating `L'\0'` (located
> with `wcschr`), and calls `tok_wline` with `NULL` for both cursor
> out-parameters.
> Return values, argument semantics, ownership and reset requirements are
> those of `[spec:libedit:sem:histedit.tok-wline-fn]`.
> `line` must be non-`NULL` and NUL-terminated. Does not reset the
> tokenizer.
> This is the function `el_source` and the `.editrc` parser use to split
> each configuration line before dispatching it — note they call it on a
> tokenizer that is created and destroyed per line, so the
> "reset between lines" obligation does not arise there.

> [spec:libedit:def:histedit.tokenizer]
> typedef struct tokenizer Tokenizer

> [spec:libedit:def:histedit.tokenizer-w]
> typedef struct tokenizerW TokenizerW

> [spec:libedit:def:histedit.wcsdup-fn]
> wchar_t * wcsdup(const wchar_t *str)

> [spec:libedit:sem:histedit.wcsdup-fn]
> Return a newly allocated copy of the wide string `str`, terminator
> included, or `NULL` on allocation failure with `errno` set to the
> allocation error (`ENOMEM`, or `EOVERFLOW` if the size computation
> overflows). The caller owns the result and frees it with `free`.
> `str` must be non-`NULL`; the implementation only asserts this under a
> debug build and otherwise dereferences it, so `NULL` is undefined
> behaviour. The length is `wcslen(str) + 1` wide characters, and the copy
> is made with `wmemcpy`, so embedded content is copied verbatim up to the
> first `L'\0'`.
> The declaration is conditional; the definition is not, and the two
> disagree. `histedit.h` guards the prototype with `#ifndef HAVE_WCSDUP`,
> which is correct for a translation unit that has already seen
> `config.h`: on a platform whose libc has `wcsdup` the prototype
> disappears. `wcsdup.c` opens with the same `#ifndef HAVE_WCSDUP`
> *before* its own `#include "config.h"`, so the macro does not exist yet
> when the guard is evaluated and the guard is unconditionally true. The
> bundled definition is therefore compiled into every build, with default
> visibility (nothing here is `libedit_private`), and libedit exports
> `wcsdup` even on platforms that already provide it — so any program
> that links libedit ahead of libc resolves `wcsdup` to this copy rather
> than the system's, for every caller in the process. It is still a libc
> gap-filler with no libedit-specific behaviour, and under
> `[dec:libedit:posix-only-scope]` it leaves the port's scope entirely —
> the Rust port targets POSIX platforms whose libc provides `wcsdup`, so
> neither the declaration nor the definition is reproduced. The rule is
> retained only to record what the symbol meant when it was present, so
> that a consumer that compiled against a `wcsdup`-less build is
> understood.

