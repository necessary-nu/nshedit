# src/keymacro.c, src/keymacro.h

> [spec:libedit:def:keymacro.el-keymacro-t]
> typedef struct el_keymacro_t

> [spec:libedit:def:keymacro.keymacro-add-fn]
> libedit_private void keymacro_add(EditLine *el, const wchar_t *key, keymacro_value_t *val, int ntype)

> [spec:libedit:sem:keymacro.keymacro-add-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-clear-fn]
> libedit_private void keymacro_clear(EditLine *el, el_action_t *map, const wchar_t *in)

> [spec:libedit:sem:keymacro.keymacro-clear-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-decode-str-fn]
> libedit_private size_t keymacro__decode_str(const wchar_t *str, char *buf, size_t len, const char *sep)

> [spec:libedit:sem:keymacro.keymacro-decode-str-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-delete-fn]
> libedit_private int keymacro_delete(EditLine *el, const wchar_t *key)

> [spec:libedit:sem:keymacro.keymacro-delete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-end-fn]
> libedit_private void keymacro_end(EditLine *el)

> [spec:libedit:sem:keymacro.keymacro-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-get-fn]
> libedit_private int keymacro_get(EditLine *el, wchar_t *ch, keymacro_value_t *val)

> [spec:libedit:sem:keymacro.keymacro-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-init-fn]
> libedit_private int keymacro_init(EditLine *el)

> [spec:libedit:sem:keymacro.keymacro-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-kprint-fn]
> libedit_private void keymacro_kprint(EditLine *el, const wchar_t *key, keymacro_value_t *val, int ntype)

> [spec:libedit:sem:keymacro.keymacro-kprint-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-map-cmd-fn]
> libedit_private keymacro_value_t * keymacro_map_cmd(EditLine *el, int cmd)

> [spec:libedit:sem:keymacro.keymacro-map-cmd-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-map-str-fn]
> libedit_private keymacro_value_t * keymacro_map_str(EditLine *el, wchar_t *str)

> [spec:libedit:sem:keymacro.keymacro-map-str-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-node-t]
> struct keymacro_node_t {
>   wchar_t ch;
>   int type;
>   keymacro_value_t val;
>   struct keymacro_node_t *next;
>   struct keymacro_node_t *sibling;
> }

> [spec:libedit:def:keymacro.keymacro-print-fn]
> libedit_private void keymacro_print(EditLine *el, const wchar_t *key)

> [spec:libedit:sem:keymacro.keymacro-print-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-reset-fn]
> libedit_private void keymacro_reset(EditLine *el)

> [spec:libedit:sem:keymacro.keymacro-reset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.keymacro-value-t]
> typedef union keymacro_value_t

> [spec:libedit:def:keymacro.node-delete-fn]
> static int node__delete(EditLine *el, keymacro_node_t **inptr, const wchar_t *str)

> [spec:libedit:sem:keymacro.node-delete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-enum-fn]
> static int node_enum(EditLine *el, keymacro_node_t *ptr, size_t cnt)

> [spec:libedit:sem:keymacro.node-enum-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-free-fn]
> static void node__free(keymacro_node_t *k)

> [spec:libedit:sem:keymacro.node-free-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-get-fn]
> static keymacro_node_t * node__get(wint_t ch)

> [spec:libedit:sem:keymacro.node-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-lookup-fn]
> static int node_lookup(EditLine *el, const wchar_t *str, keymacro_node_t *ptr, size_t cnt)

> [spec:libedit:sem:keymacro.node-lookup-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-put-fn]
> static void node__put(EditLine *el, keymacro_node_t *ptr)

> [spec:libedit:sem:keymacro.node-put-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-trav-fn]
> static int node_trav(EditLine *el, keymacro_node_t *ptr, wchar_t *ch, keymacro_value_t *val)

> [spec:libedit:sem:keymacro.node-trav-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:keymacro.node-try-fn]
> static int node__try(EditLine *el, keymacro_node_t *ptr, const wchar_t *str, keymacro_value_t *val, int ntype)

> [spec:libedit:sem:keymacro.node-try-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

