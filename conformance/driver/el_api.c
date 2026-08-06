/*
 * conformance driver 2: the el_* surface, non-interactively.
 *
 * hist_tok.c covers history and the tokenizer. This covers the other public
 * entry point — the EditLine object — and with it the four source files that
 * had no differential coverage at all: map.c, parse.c, keymacro.c and
 * chared.c. Between them they are the largest untested area in the port.
 *
 * # Why this runs without a terminal
 *
 * Everything here is driven through files. `el_init` takes three streams and
 * we hand it real ones; `TERM` is pinned to `dumb` with a copied terminfo
 * database (conformance/lib.sh); and no call below needs a tty to answer.
 * `el_gets` is deliberately absent: it reads until it has a line, and what it
 * does at EOF on a non-tty is a different question from what this driver is
 * for. That belongs with `conformance-pty`.
 *
 * # What it claims
 *
 * Only what it executes. Every operation prints its own line, so the
 * annotation that eventually cites this driver can point at the operation
 * rather than at the file. `el_parse` is the widest of them: one call reaches
 * the builtin command table in map.c, the argument parser in parse.c, and —
 * for `bind` — the key-macro trie in keymacro.c.
 *
 * # Reading the trace
 *
 * One line per operation: sequence number, label, result. Nothing that varies
 * between runs may appear — no addresses, no paths, no times. The work
 * directory's name is a path, so only file CONTENTS enter the trace, never
 * the name they were read from.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <locale.h>
#include <errno.h>

#include <histedit.h>

/* --------------------------------------------------------------------- */
/* Trace primitives, the same shape hist_tok.c uses                       */
/* --------------------------------------------------------------------- */

static int seq = 0;
static const char *workdir;

static void op(const char *label)
{
	printf("%04d %-30s ", ++seq, label);
}

/* Bytes -> pure ASCII, losing nothing. */
static void besc(const char *b, size_t n)
{
	putchar('<');
	if (b == NULL) {
		fputs("(null)", stdout);
	} else {
		for (size_t i = 0; i < n; i++) {
			unsigned c = (unsigned char)b[i];
			if (c >= 0x20 && c < 0x7f && c != '\\' && c != '<' && c != '>')
				putchar((int)c);
			else
				printf("\\x%02X", c);
		}
	}
	putchar('>');
}

static void sesc(const char *s)
{
	besc(s, s ? strlen(s) : 0);
}

static void pr(int rc)
{
	printf("rc=%d\n", rc);
}

static char pathbuf[4096];
static const char *wpath(const char *name)
{
	snprintf(pathbuf, sizeof(pathbuf), "%s/%s", workdir, name);
	return pathbuf;
}

/* --------------------------------------------------------------------- */
/* 1. Lifecycle and the el_set/el_get mirror                              */
/* --------------------------------------------------------------------- */

/*
 * Every option that can be set without a terminal, set and then read back.
 * A setter that silently does nothing and a getter that invents a default
 * both look like success from one side; comparing the pair is what catches
 * them.
 */
static void section_setget(EditLine *el)
{
	int iv;
	const char *sv;
	void *pv;

	op("EL_EDITOR emacs");   pr(el_set(el, EL_EDITOR, "emacs"));
	op("EL_GET editmode");   iv = -1; pr(el_get(el, EL_EDITMODE, &iv));
	op("  editmode value");  printf("%d\n", iv);

	op("EL_EDITOR vi");      pr(el_set(el, EL_EDITOR, "vi"));
	op("EL_EDITOR bogus");   pr(el_set(el, EL_EDITOR, "no-such-editor"));
	op("EL_EDITOR emacs 2"); pr(el_set(el, EL_EDITOR, "emacs"));

	op("EL_SIGNAL 1");       pr(el_set(el, EL_SIGNAL, 1));
	op("EL_GET signal");     iv = -1; pr(el_get(el, EL_SIGNAL, &iv));
	op("  signal value");    printf("%d\n", iv);
	op("EL_SIGNAL 0");       pr(el_set(el, EL_SIGNAL, 0));

	op("EL_UNBUFFERED 1");   pr(el_set(el, EL_UNBUFFERED, 1));
	op("EL_UNBUFFERED 0");   pr(el_set(el, EL_UNBUFFERED, 0));
	op("EL_SAFEREAD 1");     pr(el_set(el, EL_SAFEREAD, 1));
	op("EL_PREP_TERM 0");    pr(el_set(el, EL_PREP_TERM, 0));

	op("EL_CLIENTDATA set"); pr(el_set(el, EL_CLIENTDATA, (void *)&seq));
	op("EL_CLIENTDATA get"); pv = NULL; pr(el_get(el, EL_CLIENTDATA, &pv));
	op("  clientdata same"); printf("%d\n", pv == (void *)&seq);

	op("EL_EDITMODE 0");     pr(el_set(el, EL_EDITMODE, 0));
	op("EL_EDITMODE 1");     pr(el_set(el, EL_EDITMODE, 1));

	op("EL_GET terminal");   sv = NULL; pr(el_get(el, EL_TERMINAL, &sv));
	op("  terminal value");  sesc(sv); putchar('\n');

	/* An op that does not exist. The C returns -1; what matters is that it
	 * does not walk off its own switch. */
	op("el_set bogus op");   pr(el_set(el, 9999, 0));
	op("el_get bogus op");   pr(el_get(el, 9999, &iv));
}

/* --------------------------------------------------------------------- */
/* 2. el_parse — the .editrc command surface                              */
/* --------------------------------------------------------------------- */

/*
 * One el_parse call is a whole .editrc line. This is the widest reach in the
 * driver: `bind` goes through map.c into keymacro.c's trie, `settc`/`echotc`
 * into terminal.c, `setty` into tty.c, and the argument splitting is parse.c
 * throughout.
 *
 * The corpus is deliberately half malformed. A parser is defined as much by
 * what it rejects as by what it accepts, and the C's rejections are the part
 * a port is most likely to get wrong.
 */
static const char *editrc_lines[] = {
	/* Well formed. */
	"bind -e",
	"bind -v",
	"bind ^A ed-move-to-beg",
	"bind ^E ed-move-to-end",
	"bind \\^X ed-start-over",
	"bind -s ^Z \"hello\"",
	"bind -a ^A ed-move-to-end",
	"bind ^A",
	"bind -l",
	"echotc co",
	"echotc li",
	"echotc am",
	"settc co 132",
	"settc li 50",
	"telltc",
	"edit on",
	"edit off",
	"editmode on",
	"history size 42",
	"history unique 1",

	/* Malformed, or naming things that do not exist. */
	"",
	"bind",
	"bind ^A no-such-command",
	"bind -q ^A ed-move-to-beg",
	"echotc no-such-capability",
	"settc no-such-capability 1",
	"settc co",
	"history",
	"history size",
	"history size not-a-number",
	"no-such-builtin arg",
	"edit maybe",
	"bind \"unterminated",
	"bind ^A ed-move-to-beg extra arguments here",
};

static void section_parse(EditLine *el)
{
	char line[256];
	const char **argv;
	int argc;
	Tokenizer *tok = tok_init(NULL);

	for (size_t i = 0; i < sizeof(editrc_lines) / sizeof(editrc_lines[0]); i++) {
		snprintf(line, sizeof(line), "%s", editrc_lines[i]);

		/* el_parse takes an already-split argv, so split it the way
		 * el_source does — through the tokenizer, which is itself under
		 * test in hist_tok.c and so is a known quantity here. */
		tok_reset(tok);
		argc = 0;
		argv = NULL;
		int rc_tok = tok_str(tok, line, &argc, &argv);

		op("parse:");
		sesc(editrc_lines[i]);
		printf(" tok=%d argc=%d", rc_tok, argc);
		if (rc_tok == 0 && argc > 0 && argv != NULL)
			printf(" parse=%d", el_parse(el, argc, argv));
		else
			printf(" parse=skipped");
		putchar('\n');
	}
	tok_end(tok);
}

/* --------------------------------------------------------------------- */
/* 3. el_source — the same surface, reached from a file                   */
/* --------------------------------------------------------------------- */

static void write_editrc(const char *name, const char *body)
{
	FILE *f = fopen(wpath(name), "w");
	if (f == NULL)
		return;
	fputs(body, f);
	fclose(f);
}

static void section_source(EditLine *el)
{
	/* Comments, blank lines, continuation and a bad line in the middle:
	 * the C keeps going past a line it cannot parse, and a port that
	 * stopped would silently drop the rest of someone's .editrc. */
	write_editrc("editrc.good",
		     "# a comment\n"
		     "\n"
		     "bind -e\n"
		     "bind ^A ed-move-to-beg\n"
		     "no-such-builtin\n"
		     "bind ^E ed-move-to-end\n");
	op("el_source good");     pr(el_source(el, wpath("editrc.good")));

	write_editrc("editrc.empty", "");
	op("el_source empty");    pr(el_source(el, wpath("editrc.empty")));

	/* A file that is not there. The C consults $EDITRC and then $HOME,
	 * both pinned by the harness, so this is deterministic. */
	op("el_source missing");  pr(el_source(el, wpath("editrc.nonexistent")));
	op("el_source NULL");     pr(el_source(el, NULL));
}

/* --------------------------------------------------------------------- */
/* 4. The line buffer — chared.c, without a keystroke in sight            */
/* --------------------------------------------------------------------- */

static void dump_line(EditLine *el, const char *label)
{
	const LineInfo *li = el_line(el);
	op(label);
	if (li == NULL) {
		printf("(null)\n");
		return;
	}
	printf("len=%td cursor=%td buf=",
	       li->lastchar - li->buffer, li->cursor - li->buffer);
	besc(li->buffer, (size_t)(li->lastchar - li->buffer));
	putchar('\n');
}

static void section_line(EditLine *el)
{
	dump_line(el, "line initial");

	op("insertstr abc");      pr(el_insertstr(el, "abc"));
	dump_line(el, "line after abc");

	op("insertstr def");      pr(el_insertstr(el, "def"));
	dump_line(el, "line after def");

	op("cursor -3");          printf("%d\n", el_cursor(el, -3));
	dump_line(el, "line after cursor -3");

	op("insertstr X");        pr(el_insertstr(el, "X"));
	dump_line(el, "line after X");

	op("deletestr 2");        el_deletestr(el, 2); printf("void\n");
	dump_line(el, "line after deletestr");

	/* Past both ends, and zero. The C clamps rather than refusing, and
	 * where it clamps to is the observable part. */
	op("cursor +1000");       printf("%d\n", el_cursor(el, 1000));
	dump_line(el, "line after cursor +1000");
	op("cursor -1000");       printf("%d\n", el_cursor(el, -1000));
	dump_line(el, "line after cursor -1000");
	op("cursor 0");           printf("%d\n", el_cursor(el, 0));

	op("deletestr 0");        el_deletestr(el, 0); printf("void\n");
	op("deletestr 1000");     el_deletestr(el, 1000); printf("void\n");
	dump_line(el, "line after big deletestr");

	op("insertstr empty");    pr(el_insertstr(el, ""));
	dump_line(el, "line after empty insert");
}

/* --------------------------------------------------------------------- */
/* 5. Geometry                                                            */
/* --------------------------------------------------------------------- */

static void section_geometry(EditLine *el)
{
	/* COLUMNS and LINES are pinned by the harness, so what el_resize
	 * reads is fixed and the answer is comparable. */
	op("el_resize");          el_resize(el); printf("void\n");
	dump_line(el, "line after resize");
}

/* --------------------------------------------------------------------- */

int main(int argc, char **argv)
{
	const char *loc;
	EditLine *el;
	FILE *devnull;

	if (argc != 2) {
		fprintf(stderr, "usage: %s <workdir>\n", argv[0]);
		return 2;
	}
	workdir = argv[1];

	loc = setlocale(LC_ALL, "");
	op("setlocale");
	printf("%s\n", loc ? loc : "(null)");

	/* Output goes to a real stream the library may write escape sequences
	 * to; the trace is stdout and must not be polluted by them. */
	devnull = fopen("/dev/null", "w");
	if (devnull == NULL)
		return 2;

	el = el_init("conformance", stdin, devnull, devnull);
	op("el_init");
	printf("%d\n", el != NULL);
	if (el == NULL) {
		fclose(devnull);
		return 1;
	}

	/* No terminal is prepared and no signals are taken: this driver never
	 * reads a key, and both would make the run depend on its environment. */
	el_set(el, EL_PREP_TERM, 0);
	el_set(el, EL_SIGNAL, 0);

	section_setget(el);
	section_parse(el);
	section_source(el);
	section_line(el);
	section_geometry(el);

	op("el_end");
	el_end(el);
	printf("void\n");

	fclose(devnull);
	op("done");
	printf("%d operations\n", seq);
	return 0;
}
