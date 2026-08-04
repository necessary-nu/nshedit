# src/readline.c

> [spec:libedit:def:readline.add-history-fn]
> int add_history(const char *line)

> [spec:libedit:sem:readline.add-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.append-history-fn]
> int append_history(int n, const char *filename)

> [spec:libedit:sem:readline.append-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.clear-history-fn]
> void clear_history(void)

> [spec:libedit:sem:readline.clear-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.current-history-fn]
> HIST_ENTRY * current_history(void)

> [spec:libedit:sem:readline.current-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.default-history-file-fn]
> static const char * _default_history_file(void)

> [spec:libedit:sem:readline.default-history-file-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.el-rl-complete-fn]
> static unsigned char _el_rl_complete(EditLine *el __attribute__((__unused__)), int ch)

> [spec:libedit:sem:readline.el-rl-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.el-rl-tstp-fn]
> static unsigned char _el_rl_tstp(EditLine *el __attribute__((__unused__)), int ch __attribute__((__unused__)))

> [spec:libedit:sem:readline.el-rl-tstp-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.filename-completion-function-fn]
> char * filename_completion_function(const char *name, int state)

> [spec:libedit:sem:readline.filename-completion-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.free-history-entry-fn]
> histdata_t free_history_entry(HIST_ENTRY *he)

> [spec:libedit:sem:readline.free-history-entry-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.get-history-event-fn]
> const char * get_history_event(const char *cmd, int *cindex, int qchar)

> [spec:libedit:sem:readline.get-history-event-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.get-prompt-fn]
> static char * _get_prompt(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:readline.get-prompt-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.getc-function-fn]
> static int /*ARGSUSED*/ _getc_function(EditLine *el __attribute__((__unused__)), wchar_t *c)

> [spec:libedit:sem:readline.getc-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.getfrom-fn]
> static int getfrom(const char **cmdp, char **fromp, const char *search, int delim)

> [spec:libedit:sem:readline.getfrom-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.getto-fn]
> static int getto(const char **cmdp, char **top, const char *from, int delim)

> [spec:libedit:sem:readline.getto-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-arg-extract-fn]
> char * history_arg_extract(int start, int end, const char *str)

> [spec:libedit:sem:readline.history-arg-extract-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-expand-command-fn]
> static int _history_expand_command(const char *command, size_t offs, size_t cmdlen, char **result)

> [spec:libedit:sem:readline.history-expand-command-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-expand-fn]
> int history_expand(char *str, char **output)

> [spec:libedit:sem:readline.history-expand-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-get-fn]
> HIST_ENTRY * history_get(int num)

> [spec:libedit:sem:readline.history-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-get-history-state-fn]
> HISTORY_STATE * history_get_history_state(void)

> [spec:libedit:sem:readline.history-get-history-state-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-is-stifled-fn]
> int history_is_stifled(void)

> [spec:libedit:sem:readline.history-is-stifled-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-list-fn]
> HIST_ENTRY ** history_list(void)

> [spec:libedit:sem:readline.history-list-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-search-fn]
> int history_search(const char *str, int direction)

> [spec:libedit:sem:readline.history-search-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-search-pos-fn]
> int history_search_pos(const char *str, int direction __attribute__((__unused__)), int pos)

> [spec:libedit:sem:readline.history-search-pos-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-search-prefix-fn]
> int history_search_prefix(const char *str, int direction)

> [spec:libedit:sem:readline.history-search-prefix-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-set-pos-fn]
> int history_set_pos(int pos)

> [spec:libedit:sem:readline.history-set-pos-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-tokenize-fn]
> char ** history_tokenize(const char *str)

> [spec:libedit:sem:readline.history-tokenize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-total-bytes-fn]
> int history_total_bytes(void)

> [spec:libedit:sem:readline.history-total-bytes-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.history-truncate-file-fn]
> int history_truncate_file (const char *filename, int nlines)

> [spec:libedit:sem:readline.history-truncate-file-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.next-history-fn]
> HIST_ENTRY * next_history(void)

> [spec:libedit:sem:readline.next-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.previous-history-fn]
> HIST_ENTRY * previous_history(void)

> [spec:libedit:sem:readline.previous-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.read-history-fn]
> int read_history(const char *filename)

> [spec:libedit:sem:readline.read-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.readline-fn]
> char * readline(const char *p)

> [spec:libedit:sem:readline.readline-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.remove-history-fn]
> HIST_ENTRY * remove_history(int num)

> [spec:libedit:sem:readline.remove-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.replace-fn]
> static void replace(char **tmp, int c)

> [spec:libedit:sem:readline.replace-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.replace-history-entry-fn]
> HIST_ENTRY * replace_history_entry(int num, const char *line, histdata_t data)

> [spec:libedit:sem:readline.replace-history-entry-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.resize-fun-fn]
> static void _resize_fun(EditLine *el, void *a)

> [spec:libedit:sem:readline.resize-fun-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-abort-fn]
> int rl_abort(int count, int key)

> [spec:libedit:sem:readline.rl-abort-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-abort-internal-fn]
> int _rl_abort_internal(void)

> [spec:libedit:sem:readline.rl-abort-internal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-add-defun-fn]
> int rl_add_defun(const char *name, rl_command_func_t *fun, int c)

> [spec:libedit:sem:readline.rl-add-defun-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-bind-key-fn]
> int rl_bind_key(int c, rl_command_func_t *func)

> [spec:libedit:sem:readline.rl-bind-key-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-bind-key-in-map-fn]
> int /*ARGSUSED*/ rl_bind_key_in_map(int key __attribute__((__unused__)), rl_command_func_t *fun __attribute__((__unused__)), Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-bind-key-in-map-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-bind-wrapper-fn]
> static unsigned char rl_bind_wrapper(EditLine *el __attribute__((__unused__)), unsigned char c)

> [spec:libedit:sem:readline.rl-bind-wrapper-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-callback-handler-install-fn]
> void rl_callback_handler_install(const char *prompt, rl_vcpfunc_t *linefunc)

> [spec:libedit:sem:readline.rl-callback-handler-install-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-callback-handler-remove-fn]
> void rl_callback_handler_remove(void)

> [spec:libedit:sem:readline.rl-callback-handler-remove-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-callback-read-char-fn]
> void rl_callback_read_char(void)

> [spec:libedit:sem:readline.rl-callback-read-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-cleanup-after-signal-fn]
> void rl_cleanup_after_signal(void)

> [spec:libedit:sem:readline.rl-cleanup-after-signal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-compat-sub-fn]
> static char * _rl_compat_sub(const char *str, const char *what, const char *with, int globally)

> [spec:libedit:sem:readline.rl-compat-sub-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-complete-fn]
> int rl_complete(int ignore __attribute__((__unused__)), int invoking_key)

> [spec:libedit:sem:readline.rl-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-completion-append-character-function-fn]
> static const char * /*ARGSUSED*/ _rl_completion_append_character_function(const char *dummy __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-completion-append-character-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-completion-matches-fn]
> char ** rl_completion_matches(const char *str, rl_compentry_func_t *fun)

> [spec:libedit:sem:readline.rl-completion-matches-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-copy-text-fn]
> char * rl_copy_text(int from, int to)

> [spec:libedit:sem:readline.rl-copy-text-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-crlf-fn]
> int rl_crlf(void)

> [spec:libedit:sem:readline.rl-crlf-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-delete-text-fn]
> int rl_delete_text(int start, int end)

> [spec:libedit:sem:readline.rl-delete-text-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-deprep-terminal-fn]
> void rl_deprep_terminal(void)

> [spec:libedit:sem:readline.rl-deprep-terminal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-ding-fn]
> int rl_ding(void)

> [spec:libedit:sem:readline.rl-ding-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-display-match-list-fn]
> void rl_display_match_list(char **matches, int len, int max)

> [spec:libedit:sem:readline.rl-display-match-list-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-echo-signal-char-fn]
> void rl_echo_signal_char(int sig)

> [spec:libedit:sem:readline.rl-echo-signal-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-erase-entire-line-fn]
> void _rl_erase_entire_line(void)

> [spec:libedit:sem:readline.rl-erase-entire-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-event-read-char-fn]
> static int _rl_event_read_char(EditLine *el, wchar_t *wc)

> [spec:libedit:sem:readline.rl-event-read-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-filename-completion-function-fn]
> char * rl_filename_completion_function (const char *text, int state)

> [spec:libedit:sem:readline.rl-filename-completion-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-forced-update-display-fn]
> void rl_forced_update_display(void)

> [spec:libedit:sem:readline.rl-forced-update-display-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-free-line-state-fn]
> void rl_free_line_state(void)

> [spec:libedit:sem:readline.rl-free-line-state-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-generic-bind-fn]
> int /*ARGSUSED*/ rl_generic_bind(int type __attribute__((__unused__)), const char * keyseq __attribute__((__unused__)), const char * data __attribute__((__unused__)), Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-generic-bind-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-get-keymap-fn]
> Keymap rl_get_keymap(void)

> [spec:libedit:sem:readline.rl-get-keymap-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-get-previous-history-fn]
> int rl_get_previous_history(int count, int key)

> [spec:libedit:sem:readline.rl-get-previous-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-get-screen-size-fn]
> void rl_get_screen_size(int *rows, int *cols)

> [spec:libedit:sem:readline.rl-get-screen-size-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-initialize-fn]
> int rl_initialize(void)

> [spec:libedit:sem:readline.rl-initialize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-insert-fn]
> int rl_insert(int count, int c)

> [spec:libedit:sem:readline.rl-insert-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-insert-text-fn]
> int rl_insert_text(const char *text)

> [spec:libedit:sem:readline.rl-insert-text-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-kill-full-line-fn]
> int /*ARGSUSED*/ rl_kill_full_line(int count __attribute__((__unused__)), int key __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-kill-full-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-kill-text-fn]
> int /*ARGSUSED*/ rl_kill_text(int from __attribute__((__unused__)), int to __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-kill-text-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-make-bare-keymap-fn]
> Keymap rl_make_bare_keymap(void)

> [spec:libedit:sem:readline.rl-make-bare-keymap-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-message-fn]
> void rl_message(const char *format, ...)

> [spec:libedit:sem:readline.rl-message-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-newline-fn]
> int rl_newline(int count __attribute__((__unused__)), int c __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-newline-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-on-new-line-fn]
> int rl_on_new_line(void)

> [spec:libedit:sem:readline.rl-on-new-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-parse-and-bind-fn]
> int rl_parse_and_bind(const char *line)

> [spec:libedit:sem:readline.rl-parse-and-bind-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-prep-terminal-fn]
> void /*ARGSUSED*/ rl_prep_terminal(int meta_flag __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-prep-terminal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-qsort-string-compare-fn]
> int _rl_qsort_string_compare(char **s1, char **s2)

> [spec:libedit:sem:readline.rl-qsort-string-compare-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-read-init-file-fn]
> int rl_read_init_file(const char *s)

> [spec:libedit:sem:readline.rl-read-init-file-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-read-key-fn]
> int rl_read_key(void)

> [spec:libedit:sem:readline.rl-read-key-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-redisplay-fn]
> void rl_redisplay(void)

> [spec:libedit:sem:readline.rl-redisplay-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-replace-line-fn]
> void rl_replace_line(const char * text, int clear_undo __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-replace-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-reset-after-signal-fn]
> void rl_reset_after_signal(void)

> [spec:libedit:sem:readline.rl-reset-after-signal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-reset-terminal-fn]
> int rl_reset_terminal(const char *p __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-reset-terminal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-resize-terminal-fn]
> void rl_resize_terminal(void)

> [spec:libedit:sem:readline.rl-resize-terminal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-restore-prompt-fn]
> void rl_restore_prompt(void)

> [spec:libedit:sem:readline.rl-restore-prompt-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-save-prompt-fn]
> void rl_save_prompt(void)

> [spec:libedit:sem:readline.rl-save-prompt-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-set-key-fn]
> int rl_set_key(const char *keyseq __attribute__((__unused__)), rl_command_func_t *function __attribute__((__unused__)), Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-set-key-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-set-keyboard-input-timeout-fn]
> int /*ARGSUSED*/ rl_set_keyboard_input_timeout(int u __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-set-keyboard-input-timeout-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-set-keymap-fn]
> void /*ARGSUSED*/ rl_set_keymap(Keymap k __attribute__((__unused__)))

> [spec:libedit:sem:readline.rl-set-keymap-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-set-keymap-name-fn]
> int rl_set_keymap_name(const char *name, Keymap k)

> [spec:libedit:sem:readline.rl-set-keymap-name-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-set-prompt-fn]
> int rl_set_prompt(const char *prompt)

> [spec:libedit:sem:readline.rl-set-prompt-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-set-screen-size-fn]
> void rl_set_screen_size(int rows, int cols)

> [spec:libedit:sem:readline.rl-set-screen-size-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-stuff-char-fn]
> int rl_stuff_char(int c)

> [spec:libedit:sem:readline.rl-stuff-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-update-pos-fn]
> static void _rl_update_pos(void)

> [spec:libedit:sem:readline.rl-update-pos-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-variable-bind-fn]
> int rl_variable_bind(const char *var, const char *value)

> [spec:libedit:sem:readline.rl-variable-bind-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.stifle-history-fn]
> void stifle_history(int max)

> [spec:libedit:sem:readline.stifle-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.tilde-expand-fn]
> char * tilde_expand(char *name)

> [spec:libedit:sem:readline.tilde-expand-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.unstifle-history-fn]
> int unstifle_history(void)

> [spec:libedit:sem:readline.unstifle-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.username-completion-function-fn]
> char * username_completion_function(const char *text, int state)

> [spec:libedit:sem:readline.username-completion-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.using-history-fn]
> void using_history(void)

> [spec:libedit:sem:readline.using-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.where-history-fn]
> int where_history(void)

> [spec:libedit:sem:readline.where-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.write-history-fn]
> int write_history(const char *filename)

> [spec:libedit:sem:readline.write-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

