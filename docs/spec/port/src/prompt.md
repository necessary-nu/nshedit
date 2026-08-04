# src/prompt.c, src/prompt.h

> [spec:libedit:def:prompt.el-pfunc-t-edit-line]
> typedef wchar_t *(*el_pfunc_t)(EditLine *)

> [spec:libedit:def:prompt.el-prompt-t]
> typedef struct el_prompt_t

> [spec:libedit:def:prompt.prompt-default-fn]
> static wchar_t * /*ARGSUSED*/ prompt_default(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:prompt.prompt-default-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:prompt.prompt-default-r-fn]
> static wchar_t * /*ARGSUSED*/ prompt_default_r(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:prompt.prompt-default-r-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:prompt.prompt-end-fn]
> libedit_private void /*ARGSUSED*/ prompt_end(EditLine *el __attribute__((__unused__)))

> [spec:libedit:sem:prompt.prompt-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:prompt.prompt-get-fn]
> libedit_private int prompt_get(EditLine *el, el_pfunc_t *prf, wchar_t *c, int op)

> [spec:libedit:sem:prompt.prompt-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:prompt.prompt-init-fn]
> libedit_private int prompt_init(EditLine *el)

> [spec:libedit:sem:prompt.prompt-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:prompt.prompt-print-fn]
> libedit_private void prompt_print(EditLine *el, int op)

> [spec:libedit:sem:prompt.prompt-print-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:prompt.prompt-set-fn]
> libedit_private int prompt_set(EditLine *el, el_pfunc_t prf, wchar_t c, int op, int wide)

> [spec:libedit:sem:prompt.prompt-set-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

