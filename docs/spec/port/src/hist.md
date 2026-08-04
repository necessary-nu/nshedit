# src/hist.c, src/hist.h

> [spec:libedit:def:hist.el-history-t]
> typedef struct el_history_t

> [spec:libedit:def:hist.hist-command-fn]
> libedit_private int hist_command(EditLine *el, int argc, const wchar_t **argv)

> [spec:libedit:sem:hist.hist-command-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:hist.hist-convert-fn]
> libedit_private wchar_t * hist_convert(EditLine *el, int fn, void *arg)

> [spec:libedit:sem:hist.hist-convert-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:hist.hist-end-fn]
> libedit_private void hist_end(EditLine *el)

> [spec:libedit:sem:hist.hist-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:hist.hist-enlargebuf-fn]
> libedit_private int /*ARGSUSED*/ hist_enlargebuf(EditLine *el, size_t newsz)

> [spec:libedit:sem:hist.hist-enlargebuf-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:hist.hist-fun-t-void-hist-event-w-int]
> typedef int (*hist_fun_t)(void *, HistEventW *, int, ...)

> [spec:libedit:def:hist.hist-get-fn]
> libedit_private el_action_t hist_get(EditLine *el)

> [spec:libedit:sem:hist.hist-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:hist.hist-init-fn]
> libedit_private int hist_init(EditLine *el)

> [spec:libedit:sem:hist.hist-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:hist.hist-set-fn]
> libedit_private int hist_set(EditLine *el, hist_fun_t fun, void *ptr)

> [spec:libedit:sem:hist.hist-set-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

