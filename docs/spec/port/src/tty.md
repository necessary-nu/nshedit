# src/tty.c, src/tty.h

> [spec:libedit:def:tty.el-tty-t]
> typedef struct

> [spec:libedit:def:tty.tty-bind-char-fn]
> libedit_private void tty_bind_char(EditLine *el, int force)

> [spec:libedit:sem:tty.tty-bind-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-cookedmode-fn]
> libedit_private int tty_cookedmode(EditLine *el)

> [spec:libedit:sem:tty.tty-cookedmode-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-end-fn]
> libedit_private void /*ARGSUSED*/ tty_end(EditLine *el, int how)

> [spec:libedit:sem:tty.tty-end-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-get-flag-fn]
> static tcflag_t * tty__get_flag(struct termios *t, int kind)

> [spec:libedit:sem:tty.tty-get-flag-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-get-signal-character-fn]
> libedit_private int tty_get_signal_character(EditLine *el, int sig)

> [spec:libedit:sem:tty.tty-get-signal-character-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-getchar-fn]
> static void tty__getchar(struct termios *td, unsigned char *s)

> [spec:libedit:sem:tty.tty-getchar-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-getcharindex-fn]
> static int tty__getcharindex(int i)

> [spec:libedit:sem:tty.tty-getcharindex-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-getspeed-fn]
> static speed_t tty__getspeed(struct termios *td)

> [spec:libedit:sem:tty.tty-getspeed-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-getty-fn]
> static int tty_getty(EditLine *el, struct termios *t)

> [spec:libedit:sem:tty.tty-getty-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-init-fn]
> libedit_private int tty_init(EditLine *el)

> [spec:libedit:sem:tty.tty-init-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-noquotemode-fn]
> libedit_private int tty_noquotemode(EditLine *el)

> [spec:libedit:sem:tty.tty-noquotemode-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-printchar-fn]
> static void tty_printchar(EditLine *el, unsigned char *s)

> [spec:libedit:sem:tty.tty-printchar-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-quotemode-fn]
> libedit_private int tty_quotemode(EditLine *el)

> [spec:libedit:sem:tty.tty-quotemode-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-rawmode-fn]
> libedit_private int tty_rawmode(EditLine *el)

> [spec:libedit:sem:tty.tty-rawmode-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-setchar-fn]
> static void tty__setchar(struct termios *td, unsigned char *s)

> [spec:libedit:sem:tty.tty-setchar-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-setty-fn]
> static int tty_setty(EditLine *el, int action, const struct termios *t)

> [spec:libedit:sem:tty.tty-setty-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-setup-flags-fn]
> static void tty_setup_flags(EditLine *el, struct termios *tios, int mode)

> [spec:libedit:sem:tty.tty-setup-flags-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-setup-fn]
> static int tty_setup(EditLine *el)

> [spec:libedit:sem:tty.tty-setup-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-stty-fn]
> libedit_private int /*ARGSUSED*/ tty_stty(EditLine *el, int argc __attribute__((__unused__)), const wchar_t **argv)

> [spec:libedit:sem:tty.tty-stty-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-update-char-fn]
> static void tty_update_char(EditLine *el, int mode, int c)

> [spec:libedit:sem:tty.tty-update-char-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-update-flag-fn]
> static tcflag_t tty_update_flag(EditLine *el, tcflag_t f, int mode, int kind)

> [spec:libedit:sem:tty.tty-update-flag-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.tty-update-flags-fn]
> static void tty_update_flags(EditLine *el, int kind)

> [spec:libedit:sem:tty.tty-update-flags-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:tty.ttychar-t-nn-io-c-ncc]
> typedef unsigned char ttychar_t[NN_IO][C_NCC]

> [spec:libedit:def:tty.ttymap-t]
> typedef struct ttymap_t

> [spec:libedit:def:tty.ttymodes-t]
> typedef struct ttymodes_t

> [spec:libedit:def:tty.ttyperm-t-nn-io-md-nn]
> typedef struct

