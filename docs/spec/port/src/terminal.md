# src/terminal.c, src/terminal.h

> [spec:libedit:def:terminal.el-terminal-t]
> typedef struct

> [spec:libedit:def:terminal.funckey-t]
> typedef struct

> [spec:libedit:def:terminal.termcapstr]
> struct termcapstr {
>   const char *name;
>   const char *long_name;
> }

> [spec:libedit:def:terminal.termcapval]
> struct termcapval {
>   const char *name;
>   const char *long_name;
> }

> [spec:libedit:def:terminal.terminal-alloc-buffer-fn]
> static wint_t ** terminal_alloc_buffer(EditLine *el)

> [spec:libedit:sem:terminal.terminal-alloc-buffer-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-alloc-display-fn]
> static int terminal_alloc_display(EditLine *el)

> [spec:libedit:sem:terminal.terminal-alloc-display-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-alloc-fn]
> static void terminal_alloc(EditLine *el, const struct termcapstr *t, const char *cap)

> [spec:libedit:sem:terminal.terminal-alloc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-beep-fn]
> libedit_private void terminal_beep(EditLine *el)

> [spec:libedit:sem:terminal.terminal-beep-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-bind-arrow-fn]
> libedit_private void terminal_bind_arrow(EditLine *el)

> [spec:libedit:sem:terminal.terminal-bind-arrow-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-change-size-fn]
> libedit_private int terminal_change_size(EditLine *el, int lins, int cols)

> [spec:libedit:sem:terminal.terminal-change-size-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-clear-arrow-fn]
> libedit_private int terminal_clear_arrow(EditLine *el, const wchar_t *name)

> [spec:libedit:sem:terminal.terminal-clear-arrow-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-clear-eol-fn]
> libedit_private void terminal_clear_EOL(EditLine *el, int num)

> [spec:libedit:sem:terminal.terminal-clear-eol-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-clear-screen-fn]
> libedit_private void terminal_clear_screen(EditLine *el)

> [spec:libedit:sem:terminal.terminal-clear-screen-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-deletechars-fn]
> libedit_private void terminal_deletechars(EditLine *el, int num)

> [spec:libedit:sem:terminal.terminal-deletechars-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-echotc-fn]
> libedit_private int /*ARGSUSED*/ terminal_echotc(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv)

> [spec:libedit:sem:terminal.terminal-echotc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-end-fn]
> libedit_private void terminal_end(EditLine *el)

> [spec:libedit:sem:terminal.terminal-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-flush-fn]
> libedit_private void terminal__flush(EditLine *el)

> [spec:libedit:sem:terminal.terminal-flush-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-free-buffer-fn]
> static void terminal_free_buffer(wint_t ***bp)

> [spec:libedit:sem:terminal.terminal-free-buffer-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-free-display-fn]
> static void terminal_free_display(EditLine *el)

> [spec:libedit:sem:terminal.terminal-free-display-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-get-fn]
> libedit_private void terminal_get(EditLine *el, const char **term)

> [spec:libedit:sem:terminal.terminal-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-get-size-fn]
> libedit_private int terminal_get_size(EditLine *el, int *lins, int *cols)

> [spec:libedit:sem:terminal.terminal-get-size-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-gettc-fn]
> libedit_private int /*ARGSUSED*/ terminal_gettc(EditLine *el, int argc __attribute__((__unused__)), char **argv)

> [spec:libedit:sem:terminal.terminal-gettc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-init-arrow-fn]
> static void terminal_init_arrow(EditLine *el)

> [spec:libedit:sem:terminal.terminal-init-arrow-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-init-fn]
> libedit_private int terminal_init(EditLine *el)

> [spec:libedit:sem:terminal.terminal-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-insertwrite-fn]
> libedit_private void terminal_insertwrite(EditLine *el, wchar_t *cp, int num)

> [spec:libedit:sem:terminal.terminal-insertwrite-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-move-to-char-fn]
> libedit_private void terminal_move_to_char(EditLine *el, int where)

> [spec:libedit:sem:terminal.terminal-move-to-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-move-to-line-fn]
> libedit_private void terminal_move_to_line(EditLine *el, int where)

> [spec:libedit:sem:terminal.terminal-move-to-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-overwrite-fn]
> libedit_private void terminal_overwrite(EditLine *el, const wchar_t *cp, size_t n)

> [spec:libedit:sem:terminal.terminal-overwrite-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-print-arrow-fn]
> libedit_private void terminal_print_arrow(EditLine *el, const wchar_t *name)

> [spec:libedit:sem:terminal.terminal-print-arrow-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-putc-fn]
> libedit_private int terminal__putc(EditLine *el, wint_t c)

> [spec:libedit:sem:terminal.terminal-putc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-rebuffer-display-fn]
> static int terminal_rebuffer_display(EditLine *el)

> [spec:libedit:sem:terminal.terminal-rebuffer-display-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-reset-arrow-fn]
> static void terminal_reset_arrow(EditLine *el)

> [spec:libedit:sem:terminal.terminal-reset-arrow-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-set-arrow-fn]
> libedit_private int terminal_set_arrow(EditLine *el, const wchar_t *name, keymacro_value_t *fun, int type)

> [spec:libedit:sem:terminal.terminal-set-arrow-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-set-fn]
> libedit_private int terminal_set(EditLine *el, const char *term)

> [spec:libedit:sem:terminal.terminal-set-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-setflags-fn]
> static void terminal_setflags(EditLine *el)

> [spec:libedit:sem:terminal.terminal-setflags-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-settc-fn]
> libedit_private int /*ARGSUSED*/ terminal_settc(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv)

> [spec:libedit:sem:terminal.terminal-settc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-telltc-fn]
> libedit_private int /*ARGSUSED*/ terminal_telltc(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv __attribute__((__unused__)))

> [spec:libedit:sem:terminal.terminal-telltc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-tputs-fn]
> static void terminal_tputs(EditLine *el, const char *cap, int affcnt)

> [spec:libedit:sem:terminal.terminal-tputs-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.terminal-writec-fn]
> libedit_private void terminal_writec(EditLine *el, wint_t c)

> [spec:libedit:sem:terminal.terminal-writec-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.tgetent-fn]
> extern int tgetent(char *, const char *)

> [spec:libedit:sem:terminal.tgetent-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.tgetflag-fn]
> extern int tgetflag(char *)

> [spec:libedit:sem:terminal.tgetflag-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.tgetnum-fn]
> extern int tgetnum(char *)

> [spec:libedit:sem:terminal.tgetnum-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.tgetstr-fn]
> extern char* tgetstr(char*, char**)

> [spec:libedit:sem:terminal.tgetstr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.tgoto-fn]
> extern char* tgoto(const char*, int, int)

> [spec:libedit:sem:terminal.tgoto-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:terminal.tputs-fn]
> extern int tputs(const char *, int, int (*)(int))

> [spec:libedit:sem:terminal.tputs-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

