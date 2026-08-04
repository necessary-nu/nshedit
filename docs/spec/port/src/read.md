# src/read.c

> [spec:libedit:def:read.el-read-getfn-fn]
> libedit_private el_rfunc_t el_read_getfn(struct el_read_t *el_read)

> [spec:libedit:sem:read.el-read-getfn-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.el-read-setfn-fn]
> libedit_private int el_read_setfn(struct el_read_t *el_read, el_rfunc_t rc)

> [spec:libedit:sem:read.el-read-setfn-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.el-read-t]
> struct el_read_t {
>   struct macros macros;
>   el_rfunc_t read_char;
>   int read_errno;
> }

> [spec:libedit:def:read.el-wgetc-fn]
> int el_wgetc(EditLine *el, wchar_t *cp)

> [spec:libedit:sem:read.el-wgetc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.el-wgets-fn]
> const wchar_t * el_wgets(EditLine *el, int *nread)

> [spec:libedit:sem:read.el-wgets-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.el-wpush-fn]
> void el_wpush(EditLine *el, const wchar_t *str)

> [spec:libedit:sem:read.el-wpush-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.macros]
> struct macros {
>   wchar_t **macro;
>   int level;
>   int offset;
> }

> [spec:libedit:def:read.noedit-wgets-fn]
> static const wchar_t * noedit_wgets(EditLine *el, int *nread)

> [spec:libedit:sem:read.noedit-wgets-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-char-fn]
> static int read_char(EditLine *el, wchar_t *cp)

> [spec:libedit:sem:read.read-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-clearmacros-fn]
> static void read_clearmacros(struct macros *ma)

> [spec:libedit:sem:read.read-clearmacros-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-end-fn]
> libedit_private void read_end(EditLine *el)

> [spec:libedit:sem:read.read-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-finish-fn]
> libedit_private void read_finish(EditLine *el)

> [spec:libedit:sem:read.read-finish-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-fixio-fn]
> static int read__fixio(int fd __attribute__((__unused__)), int e)

> [spec:libedit:sem:read.read-fixio-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-getcmd-fn]
> static int read_getcmd(EditLine *el, el_action_t *cmdnum, wchar_t *ch)

> [spec:libedit:sem:read.read-getcmd-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-init-fn]
> libedit_private int read_init(EditLine *el)

> [spec:libedit:sem:read.read-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-pop-fn]
> static void read_pop(struct macros *ma)

> [spec:libedit:sem:read.read-pop-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:read.read-prepare-fn]
> libedit_private void read_prepare(EditLine *el)

> [spec:libedit:sem:read.read-prepare-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

