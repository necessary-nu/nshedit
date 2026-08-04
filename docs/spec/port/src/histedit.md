# src/histedit.h

> [spec:libedit:def:histedit.edit-line]
> typedef struct editline EditLine

> [spec:libedit:def:histedit.el-beep-fn]
> void el_beep(EditLine *)

> [spec:libedit:sem:histedit.el-beep-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-cursor-fn]
> int el_cursor(EditLine *, int)

> [spec:libedit:sem:histedit.el-cursor-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-deletestr-fn]
> void el_deletestr(EditLine *, int)

> [spec:libedit:sem:histedit.el-deletestr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-deletestr1-fn]
> int el_deletestr1(EditLine *, int, int)

> [spec:libedit:sem:histedit.el-deletestr1-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-end-fn]
> void el_end(EditLine *)

> [spec:libedit:sem:histedit.el-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-fn-complete-fn]
> unsigned char _el_fn_complete(EditLine *, int)

> [spec:libedit:sem:histedit.el-fn-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-fn-sh-complete-fn]
> unsigned char _el_fn_sh_complete(EditLine *, int)

> [spec:libedit:sem:histedit.el-fn-sh-complete-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-get-fn]
> int el_get(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-get-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-getc-fn]
> int el_getc(EditLine *, char *)

> [spec:libedit:sem:histedit.el-getc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-gets-fn]
> const char *el_gets(EditLine *, int *)

> [spec:libedit:sem:histedit.el-gets-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-init-fd-fn]
> EditLine *el_init_fd(const char *, FILE *, FILE *, FILE *, int, int, int)

> [spec:libedit:sem:histedit.el-init-fd-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-init-fn]
> EditLine *el_init(const char *, FILE *, FILE *, FILE *)

> [spec:libedit:sem:histedit.el-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-insertstr-fn]
> int el_insertstr(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-insertstr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-line-fn]
> const LineInfo *el_line(EditLine *)

> [spec:libedit:sem:histedit.el-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-parse-fn]
> int el_parse(EditLine *, int, const char **)

> [spec:libedit:sem:histedit.el-parse-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-push-fn]
> void el_push(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-push-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-replacestr-fn]
> int el_replacestr(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-replacestr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-reset-fn]
> void el_reset(EditLine *)

> [spec:libedit:sem:histedit.el-reset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-resize-fn]
> void el_resize(EditLine *)

> [spec:libedit:sem:histedit.el-resize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-rfunc-t-edit-line-wchar-t]
> typedef int (*el_rfunc_t)(EditLine *, wchar_t *)

> [spec:libedit:def:histedit.el-set-fn]
> int el_set(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-set-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-source-fn]
> int el_source(EditLine *, const char *)

> [spec:libedit:sem:histedit.el-source-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wget-fn]
> int el_wget(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-wget-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wgetc-fn]
> int el_wgetc(EditLine *, wchar_t *)

> [spec:libedit:sem:histedit.el-wgetc-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wgets-fn]
> const wchar_t *el_wgets(EditLine *, int *)

> [spec:libedit:sem:histedit.el-wgets-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-winsertstr-fn]
> int el_winsertstr(EditLine *, const wchar_t *)

> [spec:libedit:sem:histedit.el-winsertstr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wline-fn]
> const LineInfoW *el_wline(EditLine *)

> [spec:libedit:sem:histedit.el-wline-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wparse-fn]
> int el_wparse(EditLine *, int, const wchar_t **)

> [spec:libedit:sem:histedit.el-wparse-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wpush-fn]
> void el_wpush(EditLine *, const wchar_t *)

> [spec:libedit:sem:histedit.el-wpush-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wreplacestr-fn]
> int el_wreplacestr(EditLine *, const wchar_t *)

> [spec:libedit:sem:histedit.el-wreplacestr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.el-wset-fn]
> int el_wset(EditLine *, int, ...)

> [spec:libedit:sem:histedit.el-wset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.hist-event]
> typedef struct HistEvent

> [spec:libedit:def:histedit.hist-event-w]
> typedef struct histeventW

> [spec:libedit:def:histedit.histevent-w]
> struct histeventW {
>   int num;
>   const wchar_t *str;
> }

> [spec:libedit:def:histedit.history]
> typedef struct history History

> [spec:libedit:def:histedit.history-end-fn]
> void history_end(History *)

> [spec:libedit:sem:histedit.history-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.history-fn]
> int history(History *, HistEvent *, int, ...)

> [spec:libedit:sem:histedit.history-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.history-init-fn]
> History * history_init(void)

> [spec:libedit:sem:histedit.history-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.history-w]
> typedef struct historyW HistoryW

> [spec:libedit:def:histedit.history-w-fn]
> int history_w(HistoryW *, HistEventW *, int, ...)

> [spec:libedit:sem:histedit.history-w-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.history-wend-fn]
> void history_wend(HistoryW *)

> [spec:libedit:sem:histedit.history-wend-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.history-winit-fn]
> HistoryW * history_winit(void)

> [spec:libedit:sem:histedit.history-winit-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.line-info]
> typedef struct lineinfo

> [spec:libedit:def:histedit.line-info-w]
> typedef struct lineinfow

> [spec:libedit:def:histedit.lineinfo]
> struct lineinfo {
>   const char *buffer;
>   const char *cursor;
>   const char *lastchar;
> }

> [spec:libedit:def:histedit.lineinfow]
> struct lineinfow {
>   const wchar_t *buffer;
>   const wchar_t *cursor;
>   const wchar_t *lastchar;
> }

> [spec:libedit:def:histedit.tok-end-fn]
> void tok_end(Tokenizer *)

> [spec:libedit:sem:histedit.tok-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-init-fn]
> Tokenizer *tok_init(const char *)

> [spec:libedit:sem:histedit.tok-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-line-fn]
> int tok_line(Tokenizer *, const LineInfo *, int *, const char ***, int *, int *)

> [spec:libedit:sem:histedit.tok-line-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-reset-fn]
> void tok_reset(Tokenizer *)

> [spec:libedit:sem:histedit.tok-reset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-str-fn]
> int tok_str(Tokenizer *, const char *, int *, const char ***)

> [spec:libedit:sem:histedit.tok-str-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-wend-fn]
> void tok_wend(TokenizerW *)

> [spec:libedit:sem:histedit.tok-wend-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-winit-fn]
> TokenizerW *tok_winit(const wchar_t *)

> [spec:libedit:sem:histedit.tok-winit-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-wline-fn]
> int tok_wline(TokenizerW *, const LineInfoW *, int *, const wchar_t ***, int *, int *)

> [spec:libedit:sem:histedit.tok-wline-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-wreset-fn]
> void tok_wreset(TokenizerW *)

> [spec:libedit:sem:histedit.tok-wreset-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tok-wstr-fn]
> int tok_wstr(TokenizerW *, const wchar_t *, int *, const wchar_t ***)

> [spec:libedit:sem:histedit.tok-wstr-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:histedit.tokenizer]
> typedef struct tokenizer Tokenizer

> [spec:libedit:def:histedit.tokenizer-w]
> typedef struct tokenizerW TokenizerW

> [spec:libedit:def:histedit.wcsdup-fn]
> wchar_t * wcsdup(const wchar_t *str)

> [spec:libedit:sem:histedit.wcsdup-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

