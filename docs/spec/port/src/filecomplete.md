# src/filecomplete.c

> [spec:libedit:def:filecomplete.append-char-function-fn]
> static const char * append_char_function(const char *name)

> [spec:libedit:sem:filecomplete.append-char-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.completion-matches-fn]
> char ** completion_matches(const char *text, char *(*genfunc)(const char *, int))

> [spec:libedit:sem:filecomplete.completion-matches-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.el-fn-complete-fn]
> unsigned char _el_fn_complete(EditLine *el, int ch __attribute__((__unused__)))

> [spec:libedit:sem:filecomplete.el-fn-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.el-fn-sh-complete-fn]
> unsigned char _el_fn_sh_complete(EditLine *el, int ch)

> [spec:libedit:sem:filecomplete.el-fn-sh-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.escape-filename-fn]
> static char * escape_filename(EditLine * el, const char *filename, int single_match, const char *(*app_func)(const char *))

> [spec:libedit:sem:filecomplete.escape-filename-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.find-word-to-complete-fn]
> static wchar_t * find_word_to_complete(const wchar_t * cursor, const wchar_t * buffer, const wchar_t * word_break, const wchar_t * special_prefixes, size_t * length, int do_unescape)

> [spec:libedit:sem:filecomplete.find-word-to-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.fn-complete-fn]
> int fn_complete(EditLine *el, char *(*complete_func)(const char *, int), char **(*attempted_completion_function)(const char *, int, int), const wchar_t *word_break, const wchar_t *special_prefixes, const char *(*app_func)(const char *), ...

> [spec:libedit:sem:filecomplete.fn-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.fn-complete2-fn]
> int fn_complete2(EditLine *el, char *(*complete_func)(const char *, int), char **(*attempted_completion_function)(const char *, int, int), const wchar_t *word_break, const wchar_t *special_prefixes, const char *(*app_func)(const char *),...

> [spec:libedit:sem:filecomplete.fn-complete2-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.fn-display-match-list-fn]
> void fn_display_match_list(EditLine * el, char **matches, size_t num, size_t width, const char *(*app_func) (const char *))

> [spec:libedit:sem:filecomplete.fn-display-match-list-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.fn-filename-completion-function-fn]
> char * fn_filename_completion_function(const char *text, int state)

> [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.fn-qsort-string-compare-fn]
> static int _fn_qsort_string_compare(const void *i1, const void *i2)

> [spec:libedit:sem:filecomplete.fn-qsort-string-compare-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.fn-tilde-expand-fn]
> char * fn_tilde_expand(const char *txt)

> [spec:libedit:sem:filecomplete.fn-tilde-expand-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.needs-dquote-escaping-fn]
> static int needs_dquote_escaping(char c)

> [spec:libedit:sem:filecomplete.needs-dquote-escaping-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.needs-escaping-fn]
> static int needs_escaping(wchar_t c)

> [spec:libedit:sem:filecomplete.needs-escaping-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:filecomplete.unescape-string-fn]
> static wchar_t * unescape_string(const wchar_t *string, size_t length)

> [spec:libedit:sem:filecomplete.unescape-string-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

