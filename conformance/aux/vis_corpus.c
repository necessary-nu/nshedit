/*
 * The vis question, measured rather than assumed.
 *
 * The history file's on-disk grammar is whatever vis(3) produces, and there
 * are two vis(3)s in play:
 *
 *   - src/vis.c, NetBSD-derived and multibyte-aware, which this tree compiles
 *     in whenever configure finds no system vis (src/Makefile.am, !HAVE_VIS).
 *     That is what the port translated.
 *   - libbsd's, which Debian's libedit.so.2 imports (strvis@LIBBSD_0.0), and
 *     which therefore wrote every history file already sitting on a Debian
 *     user's disk.
 *
 * If those two disagree, "drop-in" has a data-migration problem that no
 * amount of matching the in-tree C would reveal. This program is compiled
 * twice — once against the oracle's vis, once against libbsd's — over one
 * corpus of raw byte strings, and the traces are diffed.
 *
 * strvis is byte-oriented in its signature but the NetBSD implementation
 * consults LC_CTYPE, so the caller's locale is part of the answer and
 * conformance/vis-cross.sh pins it.
 *
 * Not part of the port-vs-oracle differential: neither side of this is the
 * port. It is a fact about the format the port has to interoperate with.
 */

#include <locale.h>
#include <stdio.h>
#include <string.h>

/* Declared here rather than included: the oracle installs no vis.h, and
 * libbsd-dev's <bsd/vis.h> is not present on every host. Both libraries
 * export exactly this signature. */
int strvis(char *, const char *, int);
int strvisx(char *, const char *, size_t, int);
int strunvis(char *, const char *);

#define VIS_SP    0x0004
#define VIS_TAB   0x0008
#define VIS_NL    0x0010
#define VIS_WHITE (VIS_SP | VIS_TAB | VIS_NL)

/* Byte strings, with their lengths, so an embedded NUL could be added later.
 * These are the shapes a history entry actually takes. */
static const char *const corpus[] = {
	"plain",
	"with space",
	"tab\there",
	"newline\nhere",
	"back\\slash",
	"already \\012 encoded",
	"ctrl\001\002\037end",
	"del\177after",
	"\007bell",
	"\xc3\xa9 utf8 e-acute",
	"\xc2\xa0 utf8 nbsp",
	"\xe4\xb8\xad\xe6\x96\x87 cjk",
	"\xf0\x9f\x98\x80 emoji",
	"\xff invalid byte",
	"\x80\x9f c1 bytes",
	"\xc3 truncated utf8",
	"",
	"trailing space ",
	"_HiStOrY_V2_",
};
#define N ((int)(sizeof(corpus) / sizeof(corpus[0])))

static void besc(const char *s)
{
	putchar('<');
	for (const unsigned char *p = (const unsigned char *)s; *p; p++) {
		if (*p >= 0x20 && *p < 0x7f && *p != '\\' && *p != '<' && *p != '>')
			putchar((int)*p);
		else
			printf("\\x%02X", *p);
	}
	putchar('>');
}

int main(void)
{
	char enc[4096], dec[4096];

	printf("%04d %-26s %s\n", 0, "setlocale", setlocale(LC_ALL, ""));

	for (int i = 0; i < N; i++) {
		int n = strvis(enc, corpus[i], VIS_WHITE);
		printf("%04d %-26s in=", i * 2 + 1, "strvis");
		besc(corpus[i]);
		printf(" rc=%d out=", n);
		besc(enc);
		putchar('\n');

		int m = strunvis(dec, enc);
		printf("%04d %-26s rc=%d back=", i * 2 + 2, "strunvis", m);
		besc(dec);
		printf(" roundtrip=%d\n",
		    m >= 0 && (size_t)m == strlen(corpus[i]) &&
		    memcmp(dec, corpus[i], (size_t)m) == 0);
	}

	/* VIS_NL on its own, which is a different question from VIS_WHITE
	 * above. It is what hist_command's `history` listing passes, and the
	 * port implements it without vis(3) at all — crates/nshedit/src/
	 * vislite.rs, whose differential measures it against libbsd. That
	 * differential is only evidence about the C in this tree if the two
	 * agree here, so this is what makes the shorter measurement count.
	 *
	 * strvisx and not strvis, so the sweep can include a NUL: a NUL is in
	 * the extra list whatever the flags say, because the C's membership
	 * test is wcschr and searching for L'\0' finds the list's own
	 * terminator. */
	int seq = N * 2 + 1;
	for (int i = 0; i < N; i++) {
		int n = strvisx(enc, corpus[i], strlen(corpus[i]), VIS_NL);
		printf("%04d %-26s in=", seq++, "strvisx VIS_NL");
		besc(corpus[i]);
		printf(" rc=%d out=", n);
		besc(enc);
		putchar('\n');
	}
	for (int b = 0; b <= 255; b++) {
		char one = (char)b;
		int n = strvisx(enc, &one, 1, VIS_NL);
		printf("%04d %-26s in=%02X rc=%d out=", seq++, "strvisx VIS_NL byte",
		    (unsigned)b, n);
		besc(enc);
		putchar('\n');
	}
	return 0;
}
