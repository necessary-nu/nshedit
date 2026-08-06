/*
 * conformance-differential, driver 1: history, tokenizer, and the history
 * file round-trip.
 *
 * One source file, compiled twice — once linked against the oracle (the
 * in-tree C, built by conformance/build-oracle.sh) and once against the port
 * (target/debug/libnshedit.so). Both binaries execute this identical
 * sequence and write a deterministic trace to stdout. Any difference between
 * the two traces is either a port defect or a deliberately-frozen one, and
 * docs/errata.md is what says which.
 *
 * This surface first because it is entirely non-interactive — no tty, no
 * terminal capabilities, no signals — and because it settles an open
 * question: the history file goes through vis/unvis, and nothing until now
 * has checked that the port's encoding matches byte for byte.
 *
 * Both binaries include the ORACLE's histedit.h. That is deliberate. The
 * port ships no header of its own yet, and the header is the compile-time
 * half of the drop-in claim: if the port cannot be driven through the C's
 * own declarations, it is not a drop-in.
 *
 * The wide API is what gets driven. The narrow entry points (history_init,
 * history, tok_init, tok_line, tok_str and friends) are exported by the port
 * but abort on call — nshedit-abi routes them to core_gap() because the
 * narrow historyn.c/tokenizern.c instantiations do not exist in nshedit yet.
 * Section 0 probes that in a forked child so the fact is measured without
 * taking the rest of the run down with it.
 *
 * DETERMINISM RULES, which every addition to this file must keep:
 *   - No path, address, pid, time or size-of-anything-on-disk is ever
 *     printed. The trace has to be byte-identical across runs.
 *   - Wide strings are printed through wesc(), which renders every character
 *     outside printable ASCII as \u{HEX}, so the trace is pure ASCII whatever
 *     the locale is.
 *   - Bytes read back from a history file are printed through besc(), which
 *     renders them as \xHH. That is the actual object under test.
 *   - The data slot (histdata_t, which is void * and which histedit.h
 *     mentions in three op comments without ever declaring — it is declared
 *     only in editline/readline.h) carries small integers this file chose,
 *     never a real address.
 *   - Every line is numbered and labelled with its operation, so a diff of
 *     the two traces names the operation that diverged instead of saying
 *     "outputs differ".
 *
 * argv[1] is a writable directory. Files are created inside it and their
 * paths never reach the trace.
 */

#include <errno.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <wchar.h>

#include <histedit.h>

/* --------------------------------------------------------------------- */
/* Trace primitives                                                       */
/* --------------------------------------------------------------------- */

static int seq = 0;
static const char *workdir;

/* Opens a line of the trace: sequence number and operation label. */
static void op(const char *label)
{
	printf("%04d %-26s ", ++seq, label);
}

/* Wide string -> pure ASCII, deterministically and reversibly enough to
 * read. wchar_t is signed on this platform; cast before comparing. */
static void wesc(const wchar_t *s)
{
	if (s == NULL) {
		fputs("(null)", stdout);
		return;
	}
	putchar('<');
	for (; *s; s++) {
		unsigned long c = (unsigned long)(unsigned int)*s;
		if (c >= 0x20 && c < 0x7f && c != '\\' && c != '<' && c != '>')
			putchar((int)c);
		else
			printf("\\u{%lX}", c);
	}
	putchar('>');
}

/* Raw bytes -> pure ASCII. This is how the history file's on-disk form
 * enters the trace, so it must not lose anything. */
static void besc(const unsigned char *b, size_t n)
{
	putchar('<');
	for (size_t i = 0; i < n; i++) {
		unsigned c = b[i];
		if (c >= 0x20 && c < 0x7f && c != '\\' && c != '<' && c != '>')
			putchar((int)c);
		else
			printf("\\x%02X", c);
	}
	putchar('>');
}

/* The result of one history_w call: return code, then the event the call
 * left behind. ev is zeroed before every call, so "the implementation did
 * not write this field" prints as num=0 str=(null) on both sides and a
 * one-sided write shows up as a difference rather than as noise. */
static HistEventW ev;

static void clear_ev(void)
{
	memset(&ev, 0, sizeof(ev));
}

static void pr(int rc)
{
	printf("rc=%d num=%d str=", rc, ev.num);
	wesc(ev.str);
	putchar('\n');
}

static void pr_plain(int rc)
{
	printf("rc=%d\n", rc);
}

/* Builds a path inside the work directory. The path itself never reaches
 * the trace — only the name, and only when it identifies the fixture. */
static char pathbuf[4096];
static const char *wpath(const char *name)
{
	snprintf(pathbuf, sizeof(pathbuf), "%s/%s", workdir, name);
	return pathbuf;
}

/* Dumps a file's bytes into the trace. This is the vis round-trip's
 * observable half. */
static void dump_file(const char *label, const char *name)
{
	unsigned char buf[65536];
	FILE *fp = fopen(wpath(name), "rb");
	op(label);
	if (fp == NULL) {
		printf("open-failed errno=%d\n", errno);
		return;
	}
	size_t n = fread(buf, 1, sizeof(buf), fp);
	fclose(fp);
	printf("bytes=%zu ", n);
	besc(buf, n);
	putchar('\n');
}

/* Walks the whole history oldest-first and traces every entry. Used on both
 * sides of a save/load round trip. */
static void dump_history(HistoryW *h, const char *label)
{
	int rc;
	clear_ev();
	rc = history_w(h, &ev, H_LAST);
	int i = 0;
	while (rc != -1) {
		op(label);
		printf("[%d] ", i++);
		pr(rc);
		clear_ev();
		rc = history_w(h, &ev, H_PREV);
	}
	op(label);
	printf("end after %d entries, ", i);
	pr(rc);
}

/* --------------------------------------------------------------------- */
/* The corpus. Everything the history file format has to survive.         */
/* --------------------------------------------------------------------- */

static const wchar_t *const corpus[] = {
	L"plain",
	L"with space",
	L"tab\there",
	L"newline\nhere",		/* embedded newline: the record separator */
	L"two\n\nnewlines",
	L"back\\slash",
	L"vis-looking \\012 sequence",	/* already looks encoded */
	L"quote\"and'quote",
	L"ctrl\001\002\037end",
	L"del\177after",
	L"\007bell-first",
	L"latin-e-acute \u00e9",
	L"nbsp\u00a0here",
	L"cjk \u4e2d\u6587",
	L"emoji \U0001F600",
	L"combining a\u0301",
	L"",				/* the empty entry */
	L"trailing space ",
	L" leading space",
	L"_HiStOrY_V2_",		/* the cookie, as data */
};
#define CORPUS_N ((int)(sizeof(corpus) / sizeof(corpus[0])))

/* --------------------------------------------------------------------- */
/* Section 0: the narrow entry points                                     */
/* --------------------------------------------------------------------- */

/*
 * Eight narrow entry points — history_init, history_end, history, tok_init,
 * tok_end, tok_reset, tok_line, tok_str — are exported by both libraries, but
 * the port routes all eight to core_gap(), which aborts
 * (crates/nshedit-abi/src/histedit.rs:585,594,608,633,642,649,656,670). It
 * has nothing to call: nshedit has no narrow instantiation of historyn.c or
 * tokenizern.c yet.
 *
 * Calling them in-process would kill the driver and truncate the trace, so
 * each probe runs in a forked child and the parent reports how the child
 * died. That keeps this measured rather than asserted, and the two
 * constructors are enough — every other narrow entry point needs a handle
 * only they can produce.
 *
 * The child prints nothing, so the duplicated stdio buffer cannot double any
 * output; stdout is flushed before the fork regardless.
 */
static void probe_narrow(const char *label, void (*body)(void))
{
	fflush(stdout);
	pid_t pid = fork();
	if (pid == 0) {
		body();
		_exit(0);
	}
	int st = 0;
	waitpid(pid, &st, 0);
	op(label);
	if (WIFEXITED(st))
		printf("survived, exit=%d\n", WEXITSTATUS(st));
	else if (WIFSIGNALED(st))
		printf("killed by signal %d\n", WTERMSIG(st));
	else
		printf("unknown wait status\n");
}

static void probe_history_init(void) { (void)history_init(); }
static void probe_tok_init(void)     { (void)tok_init(NULL); }

static void section_narrow(void)
{
	probe_narrow("narrow history_init", probe_history_init);
	probe_narrow("narrow tok_init", probe_tok_init);
}

/* --------------------------------------------------------------------- */
/* Section 1: lifecycle and the scalar settings                           */
/* --------------------------------------------------------------------- */

static void section_lifecycle(HistoryW *h)
{
	clear_ev(); op("H_GETSIZE default");   pr(history_w(h, &ev, H_GETSIZE));
	clear_ev(); op("H_GETUNIQUE default"); pr(history_w(h, &ev, H_GETUNIQUE));
	clear_ev(); op("H_SETSIZE 5");         pr(history_w(h, &ev, H_SETSIZE, 5));
	clear_ev(); op("H_GETSIZE");           pr(history_w(h, &ev, H_GETSIZE));
	clear_ev(); op("H_SETSIZE 0");         pr(history_w(h, &ev, H_SETSIZE, 0));
	clear_ev(); op("H_GETSIZE after 0");   pr(history_w(h, &ev, H_GETSIZE));
	clear_ev(); op("H_SETSIZE -1");        pr(history_w(h, &ev, H_SETSIZE, -1));
	clear_ev(); op("H_GETSIZE after -1");  pr(history_w(h, &ev, H_GETSIZE));
	clear_ev(); op("H_SETUNIQUE 1");       pr(history_w(h, &ev, H_SETUNIQUE, 1));
	clear_ev(); op("H_GETUNIQUE");         pr(history_w(h, &ev, H_GETUNIQUE));
	clear_ev(); op("H_SETUNIQUE 0");       pr(history_w(h, &ev, H_SETUNIQUE, 0));
	clear_ev(); op("H_GETUNIQUE after 0"); pr(history_w(h, &ev, H_GETUNIQUE));
	clear_ev(); op("unknown op 999");      pr(history_w(h, &ev, 999));
	clear_ev(); op("H_SETSIZE 100");       pr(history_w(h, &ev, H_SETSIZE, 100));
}

/* --------------------------------------------------------------------- */
/* Section 2: entry, traversal and in-place modification                  */
/* --------------------------------------------------------------------- */

static void section_traverse(HistoryW *h)
{
	clear_ev(); op("H_ENTER one");   pr(history_w(h, &ev, H_ENTER, L"one"));
	clear_ev(); op("H_ENTER two");   pr(history_w(h, &ev, H_ENTER, L"two"));
	clear_ev(); op("H_ENTER three"); pr(history_w(h, &ev, H_ENTER, L"three"));
	clear_ev(); op("H_ENTER four");  pr(history_w(h, &ev, H_ENTER, L"four"));

	clear_ev(); op("H_FIRST"); pr(history_w(h, &ev, H_FIRST));
	clear_ev(); op("H_NEXT");  pr(history_w(h, &ev, H_NEXT));
	clear_ev(); op("H_NEXT");  pr(history_w(h, &ev, H_NEXT));
	clear_ev(); op("H_NEXT");  pr(history_w(h, &ev, H_NEXT));
	clear_ev(); op("H_NEXT past end"); pr(history_w(h, &ev, H_NEXT));
	clear_ev(); op("H_LAST");  pr(history_w(h, &ev, H_LAST));
	clear_ev(); op("H_PREV");  pr(history_w(h, &ev, H_PREV));
	clear_ev(); op("H_PREV");  pr(history_w(h, &ev, H_PREV));
	clear_ev(); op("H_PREV");  pr(history_w(h, &ev, H_PREV));
	clear_ev(); op("H_PREV past start"); pr(history_w(h, &ev, H_PREV));
	clear_ev(); op("H_CURR");  pr(history_w(h, &ev, H_CURR));

	clear_ev(); op("H_SET 2");   pr(history_w(h, &ev, H_SET, 2));
	clear_ev(); op("H_CURR");    pr(history_w(h, &ev, H_CURR));
	clear_ev(); op("H_SET 9999"); pr(history_w(h, &ev, H_SET, 9999));
	clear_ev(); op("H_CURR after bad set"); pr(history_w(h, &ev, H_CURR));

	/* H_ADD appends to the most recent entry; H_APPEND appends to the
	 * current one. Both mutate in place, which the following H_LAST and
	 * H_CURR observe. */
	clear_ev(); op("H_ADD suffix");  pr(history_w(h, &ev, H_ADD, L"-added"));
	clear_ev(); op("H_LAST after add"); pr(history_w(h, &ev, H_LAST));
	clear_ev(); op("H_SET 2");       pr(history_w(h, &ev, H_SET, 2));
	clear_ev(); op("H_APPEND suffix"); pr(history_w(h, &ev, H_APPEND, L"-appended"));
	clear_ev(); op("H_CURR after append"); pr(history_w(h, &ev, H_CURR));

	dump_history(h, "walk after modify");
}

/* --------------------------------------------------------------------- */
/* Section 3: search                                                      */
/* --------------------------------------------------------------------- */

static void section_search(HistoryW *h)
{
	clear_ev(); op("H_LAST");            pr(history_w(h, &ev, H_LAST));
	clear_ev(); op("H_PREV_STR two");    pr(history_w(h, &ev, H_PREV_STR, L"two"));
	clear_ev(); op("H_NEXT_STR three");  pr(history_w(h, &ev, H_NEXT_STR, L"three"));
	clear_ev(); op("H_PREV_STR absent"); pr(history_w(h, &ev, H_PREV_STR, L"nowhere"));
	clear_ev(); op("H_PREV_STR empty");  pr(history_w(h, &ev, H_PREV_STR, L""));
	clear_ev(); op("H_FIRST");           pr(history_w(h, &ev, H_FIRST));
	clear_ev(); op("H_PREV_EVENT 2");    pr(history_w(h, &ev, H_PREV_EVENT, 2));
	clear_ev(); op("H_NEXT_EVENT 3");    pr(history_w(h, &ev, H_NEXT_EVENT, 3));
	clear_ev(); op("H_PREV_EVENT 9999"); pr(history_w(h, &ev, H_PREV_EVENT, 9999));
	clear_ev(); op("H_NEXT_EVENT 0");    pr(history_w(h, &ev, H_NEXT_EVENT, 0));
}

/* --------------------------------------------------------------------- */
/* Section 4: data slots, replacement and deletion                        */
/* --------------------------------------------------------------------- */

static void section_data(HistoryW *h)
{
	/* The data slot is a void *. These are integers cast to pointers so the
	 * trace can print them without ever printing a real address. */
	void * d1 = (void *)(size_t)0x11;
	void * d2 = (void *)(size_t)0x22;
	void * got = NULL;

	clear_ev(); op("H_LAST");    pr(history_w(h, &ev, H_LAST));
	clear_ev(); op("H_REPLACE"); pr(history_w(h, &ev, H_REPLACE, L"replaced-text", d1));
	clear_ev(); op("H_CURR after replace"); pr(history_w(h, &ev, H_CURR));

	clear_ev(); op("H_NEXT_EVDATA curr");
	{
		int rc = history_w(h, &ev, H_NEXT_EVDATA, ev.num, &got);
		printf("rc=%d num=%d data=0x%lx str=", rc, ev.num,
		       (unsigned long)(size_t)got);
		wesc(ev.str);
		putchar('\n');
	}

	got = NULL;
	clear_ev(); op("H_SET 3"); pr(history_w(h, &ev, H_SET, 3));
	clear_ev(); op("H_REPLACE d2"); pr(history_w(h, &ev, H_REPLACE, L"second-replaced", d2));

	clear_ev(); op("H_DELDATA 3");
	{
		int rc = history_w(h, &ev, H_DELDATA, 3, &got);
		printf("rc=%d num=%d data=0x%lx str=", rc, ev.num,
		       (unsigned long)(size_t)got);
		wesc(ev.str);
		putchar('\n');
	}

	clear_ev(); op("H_DEL 2");        pr(history_w(h, &ev, H_DEL, 2));
	clear_ev(); op("H_DEL 9999");     pr(history_w(h, &ev, H_DEL, 9999));
	dump_history(h, "walk after delete");

	clear_ev(); op("H_CLEAR");        pr(history_w(h, &ev, H_CLEAR));
	clear_ev(); op("H_FIRST empty");  pr(history_w(h, &ev, H_FIRST));
	clear_ev(); op("H_LAST empty");   pr(history_w(h, &ev, H_LAST));
	clear_ev(); op("H_CURR empty");   pr(history_w(h, &ev, H_CURR));
	clear_ev(); op("H_GETSIZE after clear"); pr(history_w(h, &ev, H_GETSIZE));
}

/* --------------------------------------------------------------------- */
/* Section 5: eviction and uniqueness                                     */
/* --------------------------------------------------------------------- */

static void section_evict(void)
{
	HistoryW *h = history_winit();
	clear_ev(); op("evict: H_SETSIZE 3"); pr(history_w(h, &ev, H_SETSIZE, 3));
	for (int i = 1; i <= 5; i++) {
		wchar_t s[16];
		swprintf(s, 16, L"e%d", i);
		clear_ev(); op("evict: H_ENTER"); pr(history_w(h, &ev, H_ENTER, s));
	}
	dump_history(h, "evict: walk");
	history_wend(h);

	h = history_winit();
	clear_ev(); op("uniq: H_SETSIZE 10");  pr(history_w(h, &ev, H_SETSIZE, 10));
	clear_ev(); op("uniq: H_SETUNIQUE 1"); pr(history_w(h, &ev, H_SETUNIQUE, 1));
	clear_ev(); op("uniq: H_ENTER dup");   pr(history_w(h, &ev, H_ENTER, L"dup"));
	clear_ev(); op("uniq: H_ENTER dup");   pr(history_w(h, &ev, H_ENTER, L"dup"));
	clear_ev(); op("uniq: H_ENTER other"); pr(history_w(h, &ev, H_ENTER, L"other"));
	clear_ev(); op("uniq: H_ENTER dup");   pr(history_w(h, &ev, H_ENTER, L"dup"));
	dump_history(h, "uniq: walk");
	history_wend(h);
}

/* --------------------------------------------------------------------- */
/* Section 6: the history file round trip                                 */
/* --------------------------------------------------------------------- */

static void section_roundtrip(void)
{
	HistoryW *h = history_winit();
	clear_ev(); op("save: H_SETSIZE 100"); pr(history_w(h, &ev, H_SETSIZE, 100));

	for (int i = 0; i < CORPUS_N; i++) {
		clear_ev();
		op("save: H_ENTER");
		printf("[%02d] in=", i);
		wesc(corpus[i]);
		printf(" ");
		pr(history_w(h, &ev, H_ENTER, corpus[i]));
	}

	dump_history(h, "save: in-memory walk");

	clear_ev(); op("H_SAVE");  pr(history_w(h, &ev, H_SAVE, wpath("hist")));
	dump_file("H_SAVE bytes", "hist");

	/* H_SAVE again over the same path: the C truncates and rewrites, and
	 * the cookie is only emitted at offset 0. */
	clear_ev(); op("H_SAVE again"); pr(history_w(h, &ev, H_SAVE, wpath("hist")));
	dump_file("H_SAVE again bytes", "hist");

	/* H_NSAVE_FP with a byte budget, and H_SAVE_FP onto a caller's stream.
	 * Both take a FILE * the caller owns. */
	{
		FILE *fp = fopen(wpath("hist_n"), "w");
		clear_ev(); op("H_NSAVE_FP 40"); pr(history_w(h, &ev, H_NSAVE_FP, (size_t)40, fp));
		fclose(fp);
		dump_file("H_NSAVE_FP bytes", "hist_n");
	}
	{
		FILE *fp = fopen(wpath("hist_fp"), "w");
		clear_ev(); op("H_SAVE_FP"); pr(history_w(h, &ev, H_SAVE_FP, fp));
		fclose(fp);
		dump_file("H_SAVE_FP bytes", "hist_fp");
	}

	history_wend(h);

	/* Load it back into a fresh history and walk it. Equal traces here and
	 * on "save: in-memory walk" is the round trip holding. */
	h = history_winit();
	clear_ev(); op("load: H_SETSIZE 100"); pr(history_w(h, &ev, H_SETSIZE, 100));
	clear_ev(); op("H_LOAD");              pr(history_w(h, &ev, H_LOAD, wpath("hist")));
	dump_history(h, "load: walk");
	history_wend(h);

	/* Error paths. */
	h = history_winit();
	clear_ev(); op("load: H_SETSIZE 100"); pr(history_w(h, &ev, H_SETSIZE, 100));
	errno = 0;
	clear_ev(); op("H_LOAD missing file"); pr(history_w(h, &ev, H_LOAD, wpath("no-such-file")));
	op("H_LOAD missing errno"); printf("errno=%d\n", errno);

	{
		FILE *fp = fopen(wpath("bad_cookie"), "w");
		fputs("NOT_A_COOKIE\nentry\n", fp);
		fclose(fp);
	}
	clear_ev(); op("H_LOAD bad cookie"); pr(history_w(h, &ev, H_LOAD, wpath("bad_cookie")));

	{
		FILE *fp = fopen(wpath("empty"), "w");
		fclose(fp);
	}
	clear_ev(); op("H_LOAD empty file"); pr(history_w(h, &ev, H_LOAD, wpath("empty")));

	errno = 0;
	clear_ev(); op("H_SAVE to a directory"); pr(history_w(h, &ev, H_SAVE, workdir));
	op("H_SAVE dir errno"); printf("errno=%d\n", errno);
	history_wend(h);
}

/* --------------------------------------------------------------------- */
/* Section 7: unvis, driven from a hand-written file                      */
/* --------------------------------------------------------------------- */

/*
 * The save direction only ever produces what vis chose to produce. This
 * exercises the load direction against escapes a *different* writer might
 * have produced — libbsd's strvis wrote every history file on a Debian disk
 * — plus raw bytes that no vis would emit.
 */
static void section_unvis_fixture(void)
{
	static const char fixture[] =
		"_HiStOrY_V2_\n"
		"plain\n"
		"octal \\012 embedded\n"		/* -> newline */
		"octal \\011 tab\n"
		"backslash \\\\ doubled\n"
		"caret \\^A control\n"			/* NetBSD \^ form */
		"meta \\M-A high\n"			/* NetBSD \M- form */
		"meta-ctrl \\M^A\n"
		"named \\n \\r \\t \\b \\a \\v \\f \\s\n"
		"nul \\0 here\n"
		"unicode \\U+00E9 escape\n"
		"raw utf8 \xc3\xa9 here\n"
		"raw high \xff byte\n"
		"raw c1 \x80\x9f bytes\n"
		"unknown \\q escape\n"
		"trailing backslash \\\n";

	FILE *fp = fopen(wpath("fixture"), "wb");
	fwrite(fixture, 1, sizeof(fixture) - 1, fp);
	fclose(fp);
	dump_file("unvis fixture bytes", "fixture");

	HistoryW *h = history_winit();
	clear_ev(); op("unvis: H_SETSIZE 100"); pr(history_w(h, &ev, H_SETSIZE, 100));
	clear_ev(); op("unvis: H_LOAD");        pr(history_w(h, &ev, H_LOAD, wpath("fixture")));
	dump_history(h, "unvis: walk");

	/* And back out again: re-saving what was loaded shows whether the
	 * encode/decode pair is a fixed point. */
	clear_ev(); op("unvis: H_SAVE back"); pr(history_w(h, &ev, H_SAVE, wpath("fixture_out")));
	dump_file("unvis: re-saved bytes", "fixture_out");
	history_wend(h);
}

/* --------------------------------------------------------------------- */
/* Section 8: H_FUNC, the caller-supplied history source                  */
/* --------------------------------------------------------------------- */

/*
 * H_FUNC is the widest op — one ref pointer plus ten function pointers —
 * which makes it the load-bearing case for the port's fixed-arity varargs
 * decision (crates/nshedit-abi/src/histedit.rs, history_w). Eight of the
 * eleven arrive on the stack on x86-64 SysV.
 *
 * docs/errata.md ERR-history-* records that H_FUNC drops the caller's ref
 * pointer; whether that reproduces is visible here, because every callback
 * below reports the ref it was handed.
 */
static const wchar_t *const fake[] = { L"alpha", L"beta", L"gamma" };
#define FAKE_N 3
static int fake_pos;
static void *fake_ref_seen;

static void note_ref(void *ref)
{
	fake_ref_seen = ref;
}

static int fake_fill(HistEventW *e, int pos)
{
	if (pos < 0 || pos >= FAKE_N) {
		e->num = 0;
		e->str = L"no more";
		return -1;
	}
	e->num = pos + 1;
	e->str = fake[pos];
	return 0;
}

static int fake_first(void *ref, HistEventW *e) { note_ref(ref); fake_pos = 0; return fake_fill(e, fake_pos); }
static int fake_last(void *ref, HistEventW *e)  { note_ref(ref); fake_pos = FAKE_N - 1; return fake_fill(e, fake_pos); }
static int fake_next(void *ref, HistEventW *e)  { note_ref(ref); return fake_fill(e, ++fake_pos); }
static int fake_prev(void *ref, HistEventW *e)  { note_ref(ref); return fake_fill(e, --fake_pos); }
static int fake_curr(void *ref, HistEventW *e)  { note_ref(ref); return fake_fill(e, fake_pos); }
static int fake_set(void *ref, HistEventW *e, const int n) { note_ref(ref); fake_pos = n; return fake_fill(e, fake_pos); }
static int fake_del(void *ref, HistEventW *e, const int n) { note_ref(ref); (void)n; e->num = 0; e->str = L"del"; return 0; }
static void fake_clear(void *ref, HistEventW *e) { note_ref(ref); (void)e; fake_pos = 0; }
static int fake_enter(void *ref, HistEventW *e, const wchar_t *s) { note_ref(ref); e->num = 0; e->str = s; return 0; }
static int fake_add(void *ref, HistEventW *e, const wchar_t *s)   { note_ref(ref); e->num = 0; e->str = s; return 0; }

static void section_hfunc(void)
{
	void *ref = (void *)(size_t)0x5150;
	HistoryW *h = history_winit();

	clear_ev();
	op("H_FUNC install");
	pr(history_w(h, &ev, H_FUNC, ref,
	    fake_first, fake_next, fake_last, fake_prev, fake_curr,
	    fake_set, fake_clear, fake_enter, fake_add, fake_del));

	fake_ref_seen = (void *)(size_t)0xdead;
	clear_ev(); op("H_FUNC H_FIRST"); pr(history_w(h, &ev, H_FIRST));
	/* Only the verdict is printed. The C hands the callbacks something
	 * other than the installed ref (docs/errata.md, H_FUNC's dropped ref),
	 * and what it hands them is an address, which cannot go in a
	 * deterministic trace. "is-installed-ref" is the observable fact and it
	 * is stable. */
	op("H_FUNC ref seen"); printf("is-installed-ref=%d unchanged=%d\n",
	    fake_ref_seen == ref, fake_ref_seen == (void *)(size_t)0xdead);

	clear_ev(); op("H_FUNC H_NEXT");  pr(history_w(h, &ev, H_NEXT));
	clear_ev(); op("H_FUNC H_LAST");  pr(history_w(h, &ev, H_LAST));
	clear_ev(); op("H_FUNC H_PREV");  pr(history_w(h, &ev, H_PREV));
	clear_ev(); op("H_FUNC H_CURR");  pr(history_w(h, &ev, H_CURR));
	clear_ev(); op("H_FUNC H_SET 1"); pr(history_w(h, &ev, H_SET, 1));
	clear_ev(); op("H_FUNC H_ENTER"); pr(history_w(h, &ev, H_ENTER, L"pushed"));
	clear_ev(); op("H_FUNC H_ADD");   pr(history_w(h, &ev, H_ADD, L"appended"));
	clear_ev(); op("H_FUNC H_DEL 1"); pr(history_w(h, &ev, H_DEL, 1));
	clear_ev(); op("H_FUNC H_CLEAR"); pr(history_w(h, &ev, H_CLEAR));

	/* H_FUNC with a NULL in the middle of the pointer run: the C's
	 * history_set_fun rejects the whole set and restores the built-in
	 * source. */
	clear_ev();
	op("H_FUNC with NULL h_next");
	pr(history_w(h, &ev, H_FUNC, ref,
	    fake_first, NULL, fake_last, fake_prev, fake_curr,
	    fake_set, fake_clear, fake_enter, fake_add, fake_del));
	clear_ev(); op("after rejected H_FUNC"); pr(history_w(h, &ev, H_FIRST));

	history_wend(h);
}

/* --------------------------------------------------------------------- */
/* Section 9: the tokenizer                                               */
/* --------------------------------------------------------------------- */

static void tok_case(TokenizerW *t, const char *label, const wchar_t *line)
{
	int argc = -1;
	const wchar_t **argv = NULL;
	int rc;

	tok_wreset(t);
	op(label);
	printf("in=");
	wesc(line);
	rc = tok_wstr(t, line, &argc, &argv);
	printf(" rc=%d argc=%d", rc, argc);
	if (argv != NULL) {
		for (int i = 0; i < argc; i++) {
			printf(" [%d]=", i);
			wesc(argv[i]);
		}
		/* The C guarantees a NULL terminator past the last argument. */
		printf(" [%d]=%s", argc, argv[argc] == NULL ? "NULL" : "NOT-NULL");
	} else {
		printf(" argv=NULL");
	}
	putchar('\n');
}

static void section_tokenizer(void)
{
	TokenizerW *t = tok_winit(NULL);

	op("tok_winit(NULL)"); printf("non-null=%d\n", t != NULL);

	tok_case(t, "tok empty",           L"");
	tok_case(t, "tok spaces only",     L"   ");
	tok_case(t, "tok simple",          L"one two three");
	tok_case(t, "tok tabs",            L"one\ttwo\t\tthree");
	tok_case(t, "tok leading space",   L"   one two");
	tok_case(t, "tok trailing space",  L"one two   ");
	tok_case(t, "tok squote",          L"a 'b c' d");
	tok_case(t, "tok dquote",          L"a \"b c\" d");
	tok_case(t, "tok quote adjacent",  L"a'b'c");
	tok_case(t, "tok empty squote",    L"a '' b");
	tok_case(t, "tok empty dquote",    L"a \"\" b");
	tok_case(t, "tok backslash space", L"a b\\ c d");
	tok_case(t, "tok backslash quote", L"a \\'b c");
	tok_case(t, "tok backslash in dq", L"a \"b\\\"c\" d");
	tok_case(t, "tok backslash in sq", L"a 'b\\'c' d");
	tok_case(t, "tok unmatched squote", L"a 'b c");
	tok_case(t, "tok unmatched dquote", L"a \"b c");
	tok_case(t, "tok trailing bslash", L"a b\\");
	tok_case(t, "tok bslash newline",  L"a b\\\nc");
	tok_case(t, "tok newline",         L"a\nb");
	tok_case(t, "tok semicolon",       L"a;b");
	tok_case(t, "tok multibyte",       L"caf\u00e9 \u4e2d\u6587 \U0001F600");
	tok_case(t, "tok control chars",   L"a\001b \002c");
	tok_case(t, "tok many args",       L"1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21");

	/* tok_wline reads the cursor position out of a LineInfoW and reports
	 * which argument the cursor is in and where inside it. */
	{
		static const wchar_t buf[] = L"alpha 'beta gamma' delta";
		LineInfoW li;
		int argc = -1, cursorc = -1, cursoro = -1;
		const wchar_t **argv = NULL;

		for (int at = 0; at <= 24; at += 6) {
			tok_wreset(t);
			li.buffer = buf;
			li.lastchar = buf + wcslen(buf);
			li.cursor = buf + at;
			argc = cursorc = cursoro = -1;
			argv = NULL;
			op("tok_wline cursor");
			printf("at=%2d rc=%d", at,
			       tok_wline(t, &li, &argc, &argv, &cursorc, &cursoro));
			printf(" argc=%d cursorc=%d cursoro=%d\n", argc, cursorc, cursoro);
		}
	}

	tok_wend(t);

	/* A custom delimiter set. */
	t = tok_winit(L"|;");
	op("tok_winit(|;)"); printf("non-null=%d\n", t != NULL);
	tok_case(t, "tokdelim pipe",   L"a|b;c d");
	tok_case(t, "tokdelim spaces", L"a b|c");
	tok_wend(t);

	/* tok_wreset between two uses must not leak state from the first. */
	t = tok_winit(NULL);
	tok_case(t, "reuse first",  L"a 'unterminated");
	tok_case(t, "reuse second", L"clean line");
	tok_wend(t);
}

/* --------------------------------------------------------------------- */

int main(int argc, char **argv)
{
	if (argc < 2) {
		fprintf(stderr, "usage: %s <writable-work-directory>\n", argv[0]);
		return 2;
	}
	workdir = argv[1];

	/*
	 * Line-buffered, so an abort cannot take the trace with it.
	 *
	 * stdout to a file is block-buffered by default, and a driver that dies
	 * mid-run then loses every line still in the buffer — which made a panic
	 * at operation 153 look like a failure at 146, the last one that happened
	 * to have been flushed. A harness whose job is to say WHICH operation
	 * diverged cannot afford that; the cost is one write per line on a run
	 * that is already dominated by process startup.
	 */
	setvbuf(stdout, NULL, _IOLBF, 0);

	/* Line buffered: if one side aborts, the trace up to that point is
	 * still on stdout and the diff can name the operation that killed it. */
	setvbuf(stdout, NULL, _IOLBF, 0);

	/* The C reaches LC_CTYPE through here. The port reads the same
	 * variables itself. conformance/lib.sh pins them. */
	const char *loc = setlocale(LC_ALL, "");
	op("setlocale"); printf("%s\n", loc ? loc : "(null)");

	section_narrow();

	HistoryW *h = history_winit();
	op("history_winit"); printf("non-null=%d\n", h != NULL);
	if (h == NULL)
		return 1;

	section_lifecycle(h);
	section_traverse(h);
	section_search(h);
	section_data(h);

	history_wend(h);
	op("history_wend"); printf("done\n");

	/* H_END is history_wend by another name — src/history.c:1212 is
	 * `FUN(history,end)(h)` — so it gets its own handle and is never
	 * followed by history_wend, which would be a double free on both
	 * sides. */
	h = history_winit();
	clear_ev(); op("H_SETSIZE 4 (H_END handle)"); pr(history_w(h, &ev, H_SETSIZE, 4));
	clear_ev(); op("H_ENTER (H_END handle)");     pr(history_w(h, &ev, H_ENTER, L"doomed"));
	clear_ev(); op("H_END"); pr_plain(history_w(h, &ev, H_END));

	section_evict();
	section_roundtrip();
	section_unvis_fixture();
	section_hfunc();
	section_tokenizer();

	op("driver complete"); printf("ops=%d\n", seq);
	return 0;
}
