# src/chartype.c, src/chartype.h

> [spec:libedit:def:chartype.ct-buffer-t]
> typedef struct ct_buffer_t

> [spec:libedit:def:chartype.ct-chr-class-fn]
> libedit_private int ct_chr_class(wchar_t c)

> [spec:libedit:sem:chartype.ct-chr-class-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-conv-cbuff-resize-fn]
> static int ct_conv_cbuff_resize(ct_buffer_t *conv, size_t csize)

> [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-conv-wbuff-resize-fn]
> static int ct_conv_wbuff_resize(ct_buffer_t *conv, size_t wsize)

> [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-decode-argv-fn]
> libedit_private wchar_t ** ct_decode_argv(int argc, const char *argv[], ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-decode-argv-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-decode-string-fn]
> wchar_t * ct_decode_string(const char *s, ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-decode-string-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-enc-width-fn]
> libedit_private size_t ct_enc_width(wchar_t c)

> [spec:libedit:sem:chartype.ct-enc-width-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-encode-char-fn]
> libedit_private ssize_t ct_encode_char(char *dst, size_t len, wchar_t c)

> [spec:libedit:sem:chartype.ct-encode-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-encode-string-fn]
> char * ct_encode_string(const wchar_t *s, ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-encode-string-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-visual-char-fn]
> libedit_private ssize_t ct_visual_char(wchar_t *dst, size_t len, wchar_t c)

> [spec:libedit:sem:chartype.ct-visual-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-visual-string-fn]
> libedit_private const wchar_t * ct_visual_string(const wchar_t *s, ct_buffer_t *conv)

> [spec:libedit:sem:chartype.ct-visual-string-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:chartype.ct-visual-width-fn]
> libedit_private int ct_visual_width(wchar_t c)

> [spec:libedit:sem:chartype.ct-visual-width-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

