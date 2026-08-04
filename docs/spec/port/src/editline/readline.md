# src/editline/readline.h

> [spec:libedit:def:readline.completion-matches-fn]
> char **completion_matches(/* const */ char *, rl_compentry_func_t *)

> [spec:libedit:sem:readline.completion-matches-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.hist-entry]
> typedef struct _hist_entry

> [spec:libedit:def:readline.histdata-t]
> typedef void *histdata_t

> [spec:libedit:def:readline.history-state]
> typedef struct

> [spec:libedit:def:readline.keymap]
> typedef KEYMAP_ENTRY *Keymap

> [spec:libedit:def:readline.keymap-entry]
> typedef struct _keymap_entry

> [spec:libedit:def:readline.keymap-entry-array-keymap-size]
> typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE]

> [spec:libedit:def:readline.rl-command-func-t-int-int]
> typedef int rl_command_func_t(int, int)

> [spec:libedit:def:readline.rl-compdisp-func-t-char-int-int]
> typedef void rl_compdisp_func_t(char **, int, int)

> [spec:libedit:def:readline.rl-compentry-func-t-const-char-int]
> typedef char *rl_compentry_func_t(const char *, int)

> [spec:libedit:def:readline.rl-completion-func-t-const-char-int-int]
> typedef char **rl_completion_func_t(const char *, int, int)

> [spec:libedit:def:readline.rl-completion-word-break-hook-fn]
> extern char *(*rl_completion_word_break_hook)(void)

> [spec:libedit:sem:readline.rl-completion-word-break-hook-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-getc-function-fn]
> extern int (*rl_getc_function)(FILE *)

> [spec:libedit:sem:readline.rl-getc-function-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:readline.rl-hook-func-t-void]
> typedef int rl_hook_func_t(void)

> [spec:libedit:def:readline.rl-icppfunc-t-char]
> typedef int rl_icppfunc_t(char **)

> [spec:libedit:def:readline.rl-linebuf-func-t-const-char-int]
> typedef int rl_linebuf_func_t(const char *, int)

> [spec:libedit:def:readline.rl-vcpfunc-t-char]
> typedef void rl_vcpfunc_t(char *)

> [spec:libedit:def:readline.rl-vintfunc-t-int]
> typedef void rl_vintfunc_t(int)

> [spec:libedit:def:readline.rl-voidfunc-t-void]
> typedef void rl_voidfunc_t(void)

