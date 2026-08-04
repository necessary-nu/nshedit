# src/chared.c, src/chared.h

> [spec:libedit:def:chared.c-delafter-fn]
> libedit_private void c_delafter(EditLine *el, int num)

> [spec:libedit:sem:chared.c-delafter-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-delafter1-fn]
> libedit_private void c_delafter1(EditLine *el)

> [spec:libedit:sem:chared.c-delafter1-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-delbefore-fn]
> libedit_private void c_delbefore(EditLine *el, int num)

> [spec:libedit:sem:chared.c-delbefore-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-delbefore1-fn]
> libedit_private void c_delbefore1(EditLine *el)

> [spec:libedit:sem:chared.c-delbefore1-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-gets-fn]
> libedit_private int c_gets(EditLine *el, wchar_t *buf, const wchar_t *prompt)

> [spec:libedit:sem:chared.c-gets-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-hpos-fn]
> libedit_private int c_hpos(EditLine *el)

> [spec:libedit:sem:chared.c-hpos-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-insert-fn]
> libedit_private void c_insert(EditLine *el, int num)

> [spec:libedit:sem:chared.c-insert-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-kill-t]
> typedef struct c_kill_t

> [spec:libedit:def:chared.c-next-word-fn]
> libedit_private wchar_t * c__next_word(EditLine *el, wchar_t *p, wchar_t *high, int n, int (*wtest)(EditLine *, wint_t))

> [spec:libedit:sem:chared.c-next-word-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-prev-word-fn]
> libedit_private wchar_t * c__prev_word(EditLine *el, wchar_t *p, wchar_t *low, int n, int (*wtest)(EditLine *, wint_t))

> [spec:libedit:sem:chared.c-prev-word-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.c-redo-t]
> typedef struct c_redo_t

> [spec:libedit:def:chared.c-undo-t]
> typedef struct c_undo_t

> [spec:libedit:def:chared.c-vcmd-t]
> typedef struct c_vcmd_t

> [spec:libedit:def:chared.ce-isword-fn]
> libedit_private int ce__isword(EditLine *el, wint_t p)

> [spec:libedit:sem:chared.ce-isword-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.ch-aliasfun-fn]
> libedit_private int ch_aliasfun(EditLine *el, el_afunc_t f, void *a)

> [spec:libedit:sem:chared.ch-aliasfun-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.ch-end-fn]
> libedit_private void ch_end(EditLine *el)

> [spec:libedit:sem:chared.ch-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.ch-enlargebufs-fn]
> libedit_private int ch_enlargebufs(EditLine *el, size_t addlen)

> [spec:libedit:sem:chared.ch-enlargebufs-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.ch-init-fn]
> libedit_private int ch_init(EditLine *el)

> [spec:libedit:sem:chared.ch-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.ch-reset-fn]
> libedit_private void ch_reset(EditLine *el)

> [spec:libedit:sem:chared.ch-reset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.ch-resizefun-fn]
> libedit_private int ch_resizefun(EditLine *el, el_zfunc_t f, void *a)

> [spec:libedit:sem:chared.ch-resizefun-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-delfini-fn]
> libedit_private void cv_delfini(EditLine *el)

> [spec:libedit:sem:chared.cv-delfini-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-endword-fn]
> libedit_private wchar_t * cv__endword(EditLine *el, wchar_t *p, wchar_t *high, int n, int (*wtest)(EditLine *, wint_t))

> [spec:libedit:sem:chared.cv-endword-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-is-word-fn]
> libedit_private int cv__isWord(EditLine *el __attribute__((__unused__)), wint_t p)

> [spec:libedit:sem:chared.cv-is-word-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-isword-fn]
> libedit_private int cv__isword(EditLine *el, wint_t p)

> [spec:libedit:sem:chared.cv-isword-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-next-word-fn]
> libedit_private wchar_t * cv_next_word(EditLine *el, wchar_t *p, wchar_t *high, int n, int (*wtest)(EditLine *el, wint_t))

> [spec:libedit:sem:chared.cv-next-word-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-prev-word-fn]
> libedit_private wchar_t * cv_prev_word(EditLine *el, wchar_t *p, wchar_t *low, int n, int (*wtest)(EditLine *el, wint_t))

> [spec:libedit:sem:chared.cv-prev-word-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-undo-fn]
> libedit_private void cv_undo(EditLine *el)

> [spec:libedit:sem:chared.cv-undo-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.cv-yank-fn]
> libedit_private void cv_yank(EditLine *el, const wchar_t *ptr, int size)

> [spec:libedit:sem:chared.cv-yank-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.el-afunc-t-void-const-char]
> typedef const char *(*el_afunc_t)(void *, const char *)

> [spec:libedit:def:chared.el-chared-t]
> typedef struct el_chared_t

> [spec:libedit:def:chared.el-cursor-fn]
> int el_cursor(EditLine *el, int n)

> [spec:libedit:sem:chared.el-cursor-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.el-deletestr-fn]
> void el_deletestr(EditLine *el, int n)

> [spec:libedit:sem:chared.el-deletestr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.el-deletestr1-fn]
> int el_deletestr1(EditLine *el, int start, int end)

> [spec:libedit:sem:chared.el-deletestr1-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.el-winsertstr-fn]
> int el_winsertstr(EditLine *el, const wchar_t *s)

> [spec:libedit:sem:chared.el-winsertstr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.el-wreplacestr-fn]
> int el_wreplacestr(EditLine *el, const wchar_t *s)

> [spec:libedit:sem:chared.el-wreplacestr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chared.el-zfunc-t-edit-line-void]
> typedef void (*el_zfunc_t)(EditLine *, void *)

