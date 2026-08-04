# src/history.c

> [spec:libedit:def:history.fun-history-end-fn]
> void FUN(history,end)(TYPE(History) *h)

> [spec:libedit:sem:history.fun-history-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.fun-history-init-fn]
> TYPE(History) * FUN(history,init)(void)

> [spec:libedit:sem:history.fun-history-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.funw-history-fn]
> int FUNW(history)(TYPE(History) *h, TYPE(HistEvent) *ev, int fun, ...)

> [spec:libedit:sem:history.funw-history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.hentry-t]
> typedef struct hentry_t

> [spec:libedit:def:history.hist-event-private]
> typedef struct

> [spec:libedit:def:history.history-def-add-fn]
> static int history_def_add(void *p, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-def-add-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-clear-fn]
> static void history_def_clear(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-clear-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-curr-fn]
> static int history_def_curr(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-curr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-del-fn]
> static int history_def_del(void *p, TYPE(HistEvent) *ev __attribute__((__unused__)), const int num)

> [spec:libedit:sem:history.history-def-del-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-delete-fn]
> static void history_def_delete(history_t *h, TYPE(HistEvent) *ev __attribute__((__unused__)), hentry_t *hp)

> [spec:libedit:sem:history.history-def-delete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-enter-fn]
> static int history_def_enter(void *p, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-def-enter-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-first-fn]
> static int history_def_first(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-first-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-init-fn]
> static int history_def_init(void **p, TYPE(HistEvent) *ev __attribute__((__unused__)), int n)

> [spec:libedit:sem:history.history-def-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-insert-fn]
> static int history_def_insert(history_t *h, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-def-insert-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-last-fn]
> static int history_def_last(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-last-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-next-fn]
> static int history_def_next(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-next-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-prev-fn]
> static int history_def_prev(void *p, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-def-prev-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-def-set-fn]
> static int history_def_set(void *p, TYPE(HistEvent) *ev, const int n)

> [spec:libedit:sem:history.history-def-set-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-deldata-nth-fn]
> static int history_deldata_nth(history_t *h, TYPE(HistEvent) *ev, int num, void **data)

> [spec:libedit:sem:history.history-deldata-nth-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-efun-t-void-type-hist-event-const-char]
> typedef int (*history_efun_t)(void *, TYPE(HistEvent) *, const Char *)

> [spec:libedit:def:history.history-getsize-fn]
> static int history_getsize(TYPE(History) *h, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-getsize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-getunique-fn]
> static int history_getunique(TYPE(History) *h, TYPE(HistEvent) *ev)

> [spec:libedit:sem:history.history-getunique-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-gfun-t-void-type-hist-event]
> typedef int (*history_gfun_t)(void *, TYPE(HistEvent) *)

> [spec:libedit:def:history.history-load-fn]
> static int history_load(TYPE(History) *h, const char *fname)

> [spec:libedit:sem:history.history-load-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-next-evdata-fn]
> static int history_next_evdata(TYPE(History) *h, TYPE(HistEvent) *ev, int num, void **d)

> [spec:libedit:sem:history.history-next-evdata-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-next-event-fn]
> static int history_next_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)

> [spec:libedit:sem:history.history-next-event-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-next-string-fn]
> static int history_next_string(TYPE(History) *h, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-next-string-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-prev-event-fn]
> static int history_prev_event(TYPE(History) *h, TYPE(HistEvent) *ev, int num)

> [spec:libedit:sem:history.history-prev-event-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-prev-string-fn]
> static int history_prev_string(TYPE(History) *h, TYPE(HistEvent) *ev, const Char *str)

> [spec:libedit:sem:history.history-prev-string-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-save-fn]
> static int history_save(TYPE(History) *h, const char *fname)

> [spec:libedit:sem:history.history-save-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-save-fp-fn]
> static int history_save_fp(TYPE(History) *h, size_t nelem, FILE *fp)

> [spec:libedit:sem:history.history-save-fp-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-set-fun-fn]
> static int history_set_fun(TYPE(History) *h, TYPE(History) *nh)

> [spec:libedit:sem:history.history-set-fun-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-set-nth-fn]
> static int history_set_nth(void *p, TYPE(HistEvent) *ev, int n)

> [spec:libedit:sem:history.history-set-nth-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-setsize-fn]
> static int history_setsize(TYPE(History) *h, TYPE(HistEvent) *ev, int num)

> [spec:libedit:sem:history.history-setsize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-setunique-fn]
> static int history_setunique(TYPE(History) *h, TYPE(HistEvent) *ev, int uni)

> [spec:libedit:sem:history.history-setunique-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:history.history-sfun-t-void-type-hist-event-const-int]
> typedef int (*history_sfun_t)(void *, TYPE(HistEvent) *, const int)

> [spec:libedit:def:history.history-t]
> typedef struct history_t

> [spec:libedit:def:history.history-vfun-t-void-type-hist-event]
> typedef void (*history_vfun_t)(void *, TYPE(HistEvent) *)

