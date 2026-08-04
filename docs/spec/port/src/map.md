# src/map.c, src/map.h

> [spec:libedit:def:map.el-bindings-t]
> typedef struct el_bindings_t

> [spec:libedit:def:map.el-func-t-edit-line-wint-t]
> typedef el_action_t (*el_func_t)(EditLine *, wint_t)

> [spec:libedit:def:map.el-map-t]
> typedef struct el_map_t

> [spec:libedit:def:map.map-addfunc-fn]
> libedit_private int map_addfunc(EditLine *el, const wchar_t *name, const wchar_t *help, el_func_t func)

> [spec:libedit:sem:map.map-addfunc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-bind-fn]
> libedit_private int map_bind(EditLine *el, int argc, const wchar_t **argv)

> [spec:libedit:sem:map.map-bind-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-end-fn]
> libedit_private void map_end(EditLine *el)

> [spec:libedit:sem:map.map-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-get-editor-fn]
> libedit_private int map_get_editor(EditLine *el, const wchar_t **editor)

> [spec:libedit:sem:map.map-get-editor-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-get-wordchars-fn]
> libedit_private int map_get_wordchars(EditLine *el, const wchar_t **wordchars)

> [spec:libedit:sem:map.map-get-wordchars-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-init-emacs-fn]
> libedit_private void map_init_emacs(EditLine *el)

> [spec:libedit:sem:map.map-init-emacs-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-init-fn]
> libedit_private int map_init(EditLine *el)

> [spec:libedit:sem:map.map-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-init-meta-fn]
> static void map_init_meta(EditLine *el)

> [spec:libedit:sem:map.map-init-meta-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-init-nls-fn]
> static void map_init_nls(EditLine *el)

> [spec:libedit:sem:map.map-init-nls-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-init-vi-fn]
> libedit_private void map_init_vi(EditLine *el)

> [spec:libedit:sem:map.map-init-vi-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-print-all-keys-fn]
> static void map_print_all_keys(EditLine *el)

> [spec:libedit:sem:map.map-print-all-keys-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-print-key-fn]
> static void map_print_key(EditLine *el, el_action_t *map, const wchar_t *in)

> [spec:libedit:sem:map.map-print-key-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-print-some-keys-fn]
> static void map_print_some_keys(EditLine *el, el_action_t *map, wint_t first, wint_t last)

> [spec:libedit:sem:map.map-print-some-keys-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-set-editor-fn]
> libedit_private int map_set_editor(EditLine *el, wchar_t *editor)

> [spec:libedit:sem:map.map-set-editor-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:map.map-set-wordchars-fn]
> libedit_private int map_set_wordchars(EditLine *el, wchar_t *wordchars)

> [spec:libedit:sem:map.map-set-wordchars-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

