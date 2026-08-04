# src/vis.c, src/vis.h

> [spec:libedit:def:vis.do-hvis-fn]
> static wchar_t * do_hvis(wchar_t *dst, wint_t c, int flags, wint_t nextc, const wchar_t *extra)

> [spec:libedit:sem:vis.do-hvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.do-mbyte-fn]
> static wchar_t * do_mbyte(wchar_t *dst, wint_t c, int flags, wint_t nextc, int iswextra)

> [spec:libedit:sem:vis.do-mbyte-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.do-mvis-fn]
> static wchar_t * do_mvis(wchar_t *dst, wint_t c, int flags, wint_t nextc, const wchar_t *extra)

> [spec:libedit:sem:vis.do-mvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.do-svis-fn]
> static wchar_t * do_svis(wchar_t *dst, wint_t c, int flags, wint_t nextc, const wchar_t *extra)

> [spec:libedit:sem:vis.do-svis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.getvisfun-fn]
> static visfun_t getvisfun(int flags)

> [spec:libedit:sem:vis.getvisfun-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.iscgraph-fn]
> static int iscgraph(int c)

> [spec:libedit:sem:vis.iscgraph-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.istrsenvisx-fn]
> static int istrsenvisx(char **mbdstp, size_t *dlen, const char *mbsrc, size_t mblength, int flags, const char *mbextra, int *cerr_ptr)

> [spec:libedit:sem:vis.istrsenvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.istrsenvisxl-fn]
> static int istrsenvisxl(char **mbdstp, size_t *dlen, const char *mbsrc, int flags, const char *mbextra, int *cerr_ptr)

> [spec:libedit:sem:vis.istrsenvisxl-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.makeextralist-fn]
> static wchar_t * makeextralist(int flags, const char *src)

> [spec:libedit:sem:vis.makeextralist-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.nvis-fn]
> char * nvis(char *mbdst, size_t dlen, int c, int flags, int nextc)

> [spec:libedit:sem:vis.nvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.snvis-fn]
> char * snvis(char *mbdst, size_t dlen, int c, int flags, int nextc, const char *mbextra)

> [spec:libedit:sem:vis.snvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.stravis-fn]
> int stravis(char **mbdstp, const char *mbsrc, int flags)

> [spec:libedit:sem:vis.stravis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strenvisx-fn]
> int strenvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags, int *cerr_ptr)

> [spec:libedit:sem:vis.strenvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strnunvis-fn]
> int strnunvis(char *, size_t, const char *)

> [spec:libedit:sem:vis.strnunvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strnunvisx-fn]
> int strnunvisx(char *, size_t, const char *, int)

> [spec:libedit:sem:vis.strnunvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strnvis-fn]
> int strnvis(char *mbdst, size_t dlen, const char *mbsrc, int flags)

> [spec:libedit:sem:vis.strnvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strnvisx-fn]
> int strnvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags)

> [spec:libedit:sem:vis.strnvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strsenvisx-fn]
> int strsenvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags, const char *mbextra, int *cerr_ptr)

> [spec:libedit:sem:vis.strsenvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strsnvis-fn]
> int strsnvis(char *mbdst, size_t dlen, const char *mbsrc, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsnvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strsnvisx-fn]
> int strsnvisx(char *mbdst, size_t dlen, const char *mbsrc, size_t len, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsnvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strsvis-fn]
> int strsvis(char *mbdst, const char *mbsrc, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strsvisx-fn]
> int strsvisx(char *mbdst, const char *mbsrc, size_t len, int flags, const char *mbextra)

> [spec:libedit:sem:vis.strsvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strunvis-fn]
> int strunvis(char *, const char *)

> [spec:libedit:sem:vis.strunvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strunvisx-fn]
> int strunvisx(char *, const char *, int)

> [spec:libedit:sem:vis.strunvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strvis-fn]
> int strvis(char *mbdst, const char *mbsrc, int flags)

> [spec:libedit:sem:vis.strvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.strvisx-fn]
> int strvisx(char *mbdst, const char *mbsrc, size_t len, int flags)

> [spec:libedit:sem:vis.strvisx-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.svis-fn]
> char * svis(char *mbdst, int c, int flags, int nextc, const char *mbextra)

> [spec:libedit:sem:vis.svis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.unvis-fn]
> int unvis(char *, int, int *, int)

> [spec:libedit:sem:vis.unvis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.vis-fn]
> char * vis(char *mbdst, int c, int flags, int nextc)

> [spec:libedit:sem:vis.vis-fn]
> TODO(sem): what this does, step by step — precisely enough to
> re-implement from this rule alone, without reading the source.

> [spec:libedit:def:vis.visfun-t-wchar-t-wint-t-int-wint-t-const-wchar-t]
> typedef wchar_t *(*visfun_t)(wchar_t *, wint_t, int, wint_t, const wchar_t *)

