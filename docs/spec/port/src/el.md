# src/el.c, src/el.h

> [spec:libedit:def:el.coord-t]
> typedef struct coord_t

> [spec:libedit:def:el.editline]
> struct editline {
>   wchar_t *el_prog;
>   FILE *el_infile;
>   FILE *el_outfile;
>   FILE *el_errfile;
>   int el_infd;
>   int el_outfd;
>   int el_errfd;
>   int el_flags;
>   coord_t el_cursor;
>   wint_t **el_display;
>   wint_t **el_vdisplay;
>   void *el_data;
>   el_line_t el_line;
>   el_state_t el_state;
>   el_terminal_t el_terminal;
>   el_tty_t el_tty;
>   el_refresh_t el_refresh;
>   el_prompt_t el_prompt;
>   el_prompt_t el_rprompt;
>   el_literal_t el_literal;
>   el_chared_t el_chared;
>   el_map_t el_map;
>   el_keymacro_t el_keymacro;
>   el_history_t el_history;
>   el_search_t el_search;
>   el_signal_t el_signal;
>   struct el_read_t *el_read;
>   ct_buffer_t el_visual;
>   ct_buffer_t el_scratch;
>   ct_buffer_t el_lgcyconv;
>   LineInfo el_lgcylinfo;
> }

> [spec:libedit:def:el.editline.el-getenv-fn]
> char * (*el_getenv)(const char *)

> [spec:libedit:sem:el.editline.el-getenv-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-action-t]
> typedef unsigned char el_action_t

> [spec:libedit:def:el.el-beep-fn]
> void el_beep(EditLine *el)

> [spec:libedit:sem:el.el-beep-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-editmode-fn]
> libedit_private int /*ARGSUSED*/ el_editmode(EditLine *el, int argc, const wchar_t **argv)

> [spec:libedit:sem:el.el-editmode-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-end-fn]
> void el_end(EditLine *el)

> [spec:libedit:sem:el.el-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-init-fd-fn]
> EditLine * el_init_fd(const char *prog, FILE *fin, FILE *fout, FILE *ferr, int fdin, int fdout, int fderr)

> [spec:libedit:sem:el.el-init-fd-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-init-fn]
> EditLine * el_init(const char *prog, FILE *fin, FILE *fout, FILE *ferr)

> [spec:libedit:sem:el.el-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-init-internal-fn]
> libedit_private EditLine * el_init_internal(const char *prog, FILE *fin, FILE *fout, FILE *ferr, int fdin, int fdout, int fderr, int flags)

> [spec:libedit:sem:el.el-init-internal-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-line-t]
> typedef struct el_line_t

> [spec:libedit:def:el.el-reset-fn]
> void el_reset(EditLine *el)

> [spec:libedit:sem:el.el-reset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-resize-fn]
> void el_resize(EditLine *el)

> [spec:libedit:sem:el.el-resize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-source-fn]
> int el_source(EditLine *el, const char *fname)

> [spec:libedit:sem:el.el-source-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-state-t]
> typedef struct el_state_t

> [spec:libedit:def:el.el-wget-fn]
> int el_wget(EditLine *el, int op, ...)

> [spec:libedit:sem:el.el-wget-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-wline-fn]
> const LineInfoW * el_wline(EditLine *el)

> [spec:libedit:sem:el.el-wline-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.el-wset-fn]
> int el_wset(EditLine *el, int op, ...)

> [spec:libedit:sem:el.el-wset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:el.func-t-const-char]
> typedef char * (*func_t)(const char *)

> [spec:libedit:def:el.secure-getenv-fn]
> char *secure_getenv(char const *name)

> [spec:libedit:sem:el.secure-getenv-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

