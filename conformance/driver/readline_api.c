/*
 * conformance driver 3: the readline compatibility surface.
 *
 * hist_tok.c covers history and the tokenizer through libedit's own API;
 * el_api.c covers the EditLine object. This covers the third public face —
 * the one a program written against GNU readline links to — and with it
 * src/readline.c, which is 2400 lines and had no differential coverage.
 *
 * # What is here and what is not
 *
 * Everything driven by files or by pure computation. `history_expand` is the
 * centre of it: a parser over `!`-designators with a large, sharp-edged
 * grammar and no I/O at all, which makes it the single richest thing in the
 * library to compare. Around it sit the history list operations, the file
 * round trip, tilde expansion, filename completion over a directory we
 * create, and the binding parsers.
 *
 * `readline()` itself is absent, along with `rl_callback_read_char` and
 * anything that reads a key. Those want a pty and belong to
 * `conformance-pty`.
 *
 * # Determinism
 *
 * Two things here would otherwise vary run to run and both are handled
 * rather than avoided:
 *
 *   - `filename_completion_function` walks a directory, and readdir order is
 *     not defined. Its matches are collected and sorted before printing.
 *   - Several calls return PATHS, which contain the work directory's name.
 *     `pesc` rewrites that prefix to `<WORK>` so the trace carries the shape
 *     of the answer without the accident of where it ran.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <locale.h>
#include <errno.h>
#include <sys/stat.h>

#include <editline/readline.h>

/* --------------------------------------------------------------------- */
/* Trace primitives                                                       */
/* --------------------------------------------------------------------- */

static int seq = 0;
static const char *workdir;

static void op(const char *label)
{
	printf("%04d %-28s ", ++seq, label);
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

/*
 * A string that may contain the work directory. The prefix becomes <WORK> so
 * that two runs in two directories produce the same trace, which is what lets
 * the oracle and the port be compared at all.
 */
static void pesc(const char *s)
{
	size_t wlen;

	if (s == NULL) {
		fputs("<(null)>", stdout);
		return;
	}
	wlen = strlen(workdir);
	if (strncmp(s, workdir, wlen) == 0) {
		putchar('<');
		fputs("<WORK>", stdout);
		besc(s + wlen, strlen(s) - wlen);
		printf(">");
		return;
	}
	sesc(s);
}

static char pathbuf[4096];
static const char *wpath(const char *name)
{
	snprintf(pathbuf, sizeof(pathbuf), "%s/%s", workdir, name);
	return pathbuf;
}

/* One history entry, or the fact that there is none. */
static void pr_entry(HIST_ENTRY *e)
{
	if (e == NULL) {
		printf("(null)\n");
		return;
	}
	sesc(e->line);
	printf(" data=%d\n", e->data != NULL);
}

/* --------------------------------------------------------------------- */
/* 1. The history list                                                    */
/* --------------------------------------------------------------------- */

static void section_history_list(void)
{
	HIST_ENTRY **list;
	HISTORY_STATE *st;
	int i;

	op("using_history");        using_history(); printf("void\n");
	op("history_length");       printf("%d\n", history_length);
	op("history_base");         printf("%d\n", history_base);
	op("where_history");        printf("%d\n", where_history());
	op("current_history");      pr_entry(current_history());
	op("history_get(0)");       pr_entry(history_get(0));
	op("history_get(1)");       pr_entry(history_get(1));

	op("add_history alpha");    printf("%d\n", add_history("alpha"));
	op("add_history beta");     printf("%d\n", add_history("beta"));
	op("add_history gamma");    printf("%d\n", add_history("gamma"));
	op("add_history empty");    printf("%d\n", add_history(""));
	op("history_length 2");     printf("%d\n", history_length);

	op("history_list");
	list = history_list();
	if (list == NULL) {
		printf("(null)\n");
	} else {
		printf("[");
		for (i = 0; list[i] != NULL; i++) {
			if (i)
				putchar(' ');
			sesc(list[i]->line);
		}
		printf("] n=%d\n", i);
	}

	op("history_total_bytes");  printf("%d\n", history_total_bytes());
	op("where_history 2");      printf("%d\n", where_history());
	op("history_set_pos 0");    printf("%d\n", history_set_pos(0));
	op("current_history 2");    pr_entry(current_history());
	op("next_history");         pr_entry(next_history());
	op("next_history 2");       pr_entry(next_history());
	op("previous_history");     pr_entry(previous_history());
	op("history_set_pos 99");   printf("%d\n", history_set_pos(99));
	op("history_set_pos -1");   printf("%d\n", history_set_pos(-1));

	op("history_get(base)");    pr_entry(history_get(history_base));
	op("history_get(base+1)");  pr_entry(history_get(history_base + 1));
	op("history_get(-1)");      pr_entry(history_get(-1));
	op("history_get(9999)");    pr_entry(history_get(9999));

	op("history_search beta");  printf("%d\n", history_search("beta", 0));
	op("history_search miss");  printf("%d\n", history_search("nope", 0));
	op("history_search_prefix"); printf("%d\n", history_search_prefix("ga", 0));
	op("history_search_pos");   printf("%d\n", history_search_pos("a", 0, 0));

	op("get_history_state");
	st = history_get_history_state();
	if (st == NULL) {
		printf("(null)\n");
	} else {
		/* libedit's HISTORY_STATE has one field; GNU readline's has
		 * four. The header comment says "only supports length". */
		printf("length=%d\n", st->length);
		free(st);
	}

	op("replace_history_entry"); pr_entry(replace_history_entry(1, "BETA", NULL));
	op("history_get(base+1) 2"); pr_entry(history_get(history_base + 1));
	op("remove_history(0)");     pr_entry(remove_history(0));
	op("history_length 3");      printf("%d\n", history_length);
	op("remove_history(99)");    pr_entry(remove_history(99));
	op("remove_history(-1)");    pr_entry(remove_history(-1));

	op("stifle_history 2");      stifle_history(2); printf("void\n");
	op("history_is_stifled");    printf("%d\n", history_is_stifled());
	op("max_input_history");     printf("%d\n", max_input_history);
	op("add over the limit");    printf("%d\n", add_history("delta"));
	op("history_length 4");      printf("%d\n", history_length);
	op("unstifle_history");      printf("%d\n", unstifle_history());
	op("history_is_stifled 2");  printf("%d\n", history_is_stifled());

	op("clear_history");         clear_history(); printf("void\n");
	op("history_length 5");      printf("%d\n", history_length);
	op("current_history 3");     pr_entry(current_history());
}

/* --------------------------------------------------------------------- */
/* 2. history_expand — the `!` grammar                                    */
/* --------------------------------------------------------------------- */

/*
 * The richest single comparison available. Every line is run against the same
 * seeded history, and both the return code and the produced string are
 * traced: `history_expand` returns 0 (no expansion), 1 (expanded), -1 (error)
 * or 2 (display only, do not execute), and a port that got the code right and
 * the string wrong would otherwise pass.
 */
static const char *expand_corpus[] = {
	/* No designator at all. */
	"echo hello",
	"",
	"!",
	"echo \\!not",

	/* Event designators. */
	"!!",
	"!1",
	"!2",
	"!-1",
	"!-2",
	"!0",
	"!99",
	"!-99",
	"!ec",
	"!echo",
	"!nosuch",
	"!?two?",
	"!?nosuch?",
	"!#",

	/* Word designators. */
	"!!:0",
	"!!:1",
	"!!:$",
	"!!:^",
	"!!:*",
	"!!:1-2",
	"!!:0-$",
	"!!:99",

	/* Modifiers. */
	"!!:p",
	"!!:h",
	"!!:t",
	"!!:r",
	"!!:e",
	"!!:s/two/TWO/",
	"!!:gs/o/0/",
	"!!:q",
	"!!:x",
	"!!:nosuchmodifier",

	/* Quick substitution. */
	"^two^TWO",
	"^two^TWO^",
	"^nosuch^x",
	"^^",

	/* Embedded, quoted and adjacent. */
	"prefix !! suffix",
	"'!!'",
	"\"!!\"",
	"a!!b",
	"!!!!",
	"echo $(!!)",
	"!! | !!",
};

static void section_expand(void)
{
	char line[256];
	char *out;
	int rc;

	/* A known history for the designators to reach. Indices matter, so
	 * this is rebuilt from empty rather than inherited. */
	clear_history();
	using_history();
	add_history("echo one two three");
	add_history("ls -l /tmp/dir/file.txt");
	add_history("grep pattern file");

	op("expand history seeded"); printf("length=%d\n", history_length);

	for (size_t i = 0; i < sizeof(expand_corpus) / sizeof(expand_corpus[0]); i++) {
		/* history_expand takes a writable buffer in the C's signature
		 * and some implementations do write to it. */
		snprintf(line, sizeof(line), "%s", expand_corpus[i]);
		out = NULL;
		rc = history_expand(line, &out);

		op("expand");
		sesc(expand_corpus[i]);
		printf(" rc=%d out=", rc);
		sesc(out);
		putchar('\n');
		free(out);
	}
}

/* --------------------------------------------------------------------- */
/* 3. Tokenizing and argument extraction                                  */
/* --------------------------------------------------------------------- */

static const char *tokenize_corpus[] = {
	"one two three",
	"  leading and   repeated   spaces  ",
	"'single quoted' rest",
	"\"double quoted\" rest",
	"back\\ slash",
	"unterminated 'quote",
	"",
	"a|b;c&d",
	"tab\there",
};

static void section_tokenize(void)
{
	char **toks;
	int i;

	for (size_t k = 0; k < sizeof(tokenize_corpus) / sizeof(tokenize_corpus[0]); k++) {
		op("tokenize");
		sesc(tokenize_corpus[k]);
		toks = history_tokenize(tokenize_corpus[k]);
		if (toks == NULL) {
			printf(" (null)\n");
			continue;
		}
		printf(" [");
		for (i = 0; toks[i] != NULL; i++) {
			if (i)
				putchar(' ');
			sesc(toks[i]);
			free(toks[i]);
		}
		printf("] n=%d\n", i);
		free(toks);
	}

	/* history_arg_extract over a known line: first, last, range, and both
	 * ends of the range out of bounds. */
	{
		static const char *src = "cmd one two three";
		struct { int f, l; const char *label; } cases[] = {
			{ 0, 0, "arg 0..0" },
			{ 1, 1, "arg 1..1" },
			{ 1, 3, "arg 1..3" },
			{ 0, 3, "arg 0..3" },
			{ 2, 1, "arg 2..1 reversed" },
			{ 0, 99, "arg 0..99" },
			{ -1, 1, "arg -1..1" },
		};
		for (size_t k = 0; k < sizeof(cases) / sizeof(cases[0]); k++) {
			char *got = history_arg_extract(cases[k].f, cases[k].l, src);
			op(cases[k].label);
			sesc(got);
			putchar('\n');
			free(got);
		}
	}

	/* get_history_event walks a designator and reports how far it got. */
	{
		static const char *evs[] = { "!!", "!1", "!-1", "!ec", "!?two?", "!nosuch", "!" };
		for (size_t k = 0; k < sizeof(evs) / sizeof(evs[0]); k++) {
			int idx = 0;
			const char *got = get_history_event(evs[k], &idx, 0);
			op("get_history_event");
			sesc(evs[k]);
			printf(" idx=%d got=", idx);
			sesc(got);
			putchar('\n');
		}
	}
}

/* --------------------------------------------------------------------- */
/* 4. The history file, through the readline names                        */
/* --------------------------------------------------------------------- */

static void dump_file(const char *label, const char *name)
{
	FILE *f = fopen(wpath(name), "rb");
	char buf[8192];
	size_t n;

	op(label);
	if (f == NULL) {
		printf("(no file)\n");
		return;
	}
	n = fread(buf, 1, sizeof(buf), f);
	fclose(f);
	besc(buf, n);
	putchar('\n');
}

static void section_file(void)
{
	clear_history();
	using_history();
	add_history("first entry");
	add_history("second with  spaces");
	add_history("third\twith a tab");

	op("write_history");        printf("%d\n", write_history(wpath("rlhist")));
	dump_file("write_history bytes", "rlhist");

	op("clear_history");        clear_history(); printf("void\n");
	op("read_history");         printf("%d\n", read_history(wpath("rlhist")));
	op("history_length");       printf("%d\n", history_length);
	op("read back [0]");        pr_entry(history_get(history_base));
	op("read back [2]");        pr_entry(history_get(history_base + 2));

	op("append_history 1");     printf("%d\n", append_history(1, wpath("rlhist")));
	dump_file("append_history bytes", "rlhist");

	op("truncate_file 2");      printf("%d\n", history_truncate_file(wpath("rlhist"), 2));
	dump_file("truncate bytes", "rlhist");

	op("read_history missing"); printf("%d\n", read_history(wpath("rl-nonexistent")));
	op("errno after");          printf("%d\n", errno == ENOENT);
	op("write_history to dir"); printf("%d\n", write_history(workdir));
	op("read_history NULL");    printf("%d\n", read_history(NULL));
	op("truncate missing");     printf("%d\n", history_truncate_file(wpath("rl-nope"), 1));
}

/* --------------------------------------------------------------------- */
/* 5. Tilde expansion and filename completion                             */
/* --------------------------------------------------------------------- */

static int cmp_str(const void *a, const void *b)
{
	return strcmp(*(const char *const *)a, *(const char *const *)b);
}

static void section_expansion(void)
{
	static const char *tildes[] = {
		"~", "~/", "~/sub", "plain", "", "~nosuchuser", "~nosuchuser/x", "a~b",
	};
	char buf[512];

	for (size_t i = 0; i < sizeof(tildes) / sizeof(tildes[0]); i++) {
		char *got;
		snprintf(buf, sizeof(buf), "%s", tildes[i]);
		got = tilde_expand(buf);
		op("tilde_expand");
		sesc(tildes[i]);
		printf(" -> ");
		pesc(got);
		putchar('\n');
		free(got);
	}

	/* A directory we made, so the answer is ours rather than the host's. */
	{
		char dir[4096];
		char *matches[64];
		int n = 0;
		char *m;

		snprintf(dir, sizeof(dir), "%s/comp", workdir);
		mkdir(dir, 0700);
		for (const char **f = (const char *[]){ "aaa", "aab", "abc", "b", NULL }; *f; f++) {
			char p[4200];
			snprintf(p, sizeof(p), "%s/%s", dir, *f);
			FILE *t = fopen(p, "w");
			if (t)
				fclose(t);
		}

		snprintf(buf, sizeof(buf), "%s/comp/aa", workdir);
		while ((m = filename_completion_function(buf, n)) != NULL && n < 63)
			matches[n++] = m;
		/* readdir order is undefined; the SET is what is being compared. */
		qsort(matches, (size_t)n, sizeof(matches[0]), cmp_str);
		op("filename_completion aa");
		printf("n=%d [", n);
		for (int i = 0; i < n; i++) {
			if (i)
				putchar(' ');
			pesc(matches[i]);
			free(matches[i]);
		}
		printf("]\n");

		snprintf(buf, sizeof(buf), "%s/comp/zzz", workdir);
		m = filename_completion_function(buf, 0);
		op("filename_completion miss");
		pesc(m);
		putchar('\n');
		free(m);
	}
}

/* --------------------------------------------------------------------- */
/* 6. The binding parsers                                                 */
/* --------------------------------------------------------------------- */

static const char *bind_corpus[] = {
	"set editing-mode emacs",
	"set editing-mode vi",
	"set editing-mode nosuch",
	"set horizontal-scroll-mode on",
	"set bell-style none",
	"set nosuchvariable on",
	"set",
	"\"\\C-a\": beginning-of-line",
	"\"\\C-e\": end-of-line",
	"\"\\C-x\": nosuch-command",
	"Control-a: beginning-of-line",
	"nonsense",
	"",
};

static void section_binding(void)
{
	char buf[256];

	for (size_t i = 0; i < sizeof(bind_corpus) / sizeof(bind_corpus[0]); i++) {
		snprintf(buf, sizeof(buf), "%s", bind_corpus[i]);
		op("parse_and_bind");
		sesc(bind_corpus[i]);
		printf(" rc=%d\n", rl_parse_and_bind(buf));
	}

	op("variable_bind emacs");   printf("%d\n", rl_variable_bind("editing-mode", "emacs"));
	op("variable_bind vi");      printf("%d\n", rl_variable_bind("editing-mode", "vi"));
	op("variable_bind bogus");   printf("%d\n", rl_variable_bind("nosuchvar", "x"));
	op("read_init_file missing"); printf("%d\n", rl_read_init_file(wpath("no-inputrc")));
}

/* --------------------------------------------------------------------- */
/* 7. The exported variables a consumer reads                             */
/* --------------------------------------------------------------------- */

static void section_variables(void)
{
	op("rl_library_version");    sesc(rl_library_version); putchar('\n');
	op("rl_readline_version");   printf("%d\n", rl_readline_version);
	op("rl_basic_word_break");   sesc(rl_basic_word_break_characters); putchar('\n');
	op("rl_basic_quote_chars");  sesc(rl_basic_quote_characters); putchar('\n');
	op("rl_completer_quote");    sesc(rl_completer_quote_characters); putchar('\n');
	op("rl_special_prefixes");   sesc(rl_special_prefixes); putchar('\n');
	op("history_expansion_char"); printf("%d\n", history_expansion_char);
	op("history_subst_char");    printf("%d\n", history_subst_char);
	op("history_no_expand");     sesc(history_no_expand_chars); putchar('\n');
	op("rl_completion_query");   printf("%d\n", rl_completion_query_items);
	op("rl_completion_append");  printf("%d\n", rl_completion_append_character);
	op("rl_catch_signals");      printf("%d\n", rl_catch_signals);
	op("rl_point/rl_end");       printf("%d %d\n", rl_point, rl_end);
}


/* --------------------------------------------------------------------- */
/* 8. Completion                                                          */
/* --------------------------------------------------------------------- */

/*
 * A generator of the shape readline defines: called with the same text and an
 * increasing state, returning a fresh string each time and NULL to stop. This
 * is what `completion_matches` and `rl_completion_matches` are built to
 * consume, and driving them through one written here rather than through a
 * real completer keeps the answer independent of what happens to be on disk.
 */
/* `strdup` is POSIX, not C11, and this compiles with -std=c11; two lines
 * here beats a feature-test macro that changes what else is declared. */
static char *dup_str(const char *s)
{
	size_t n = strlen(s) + 1;
	char *p = malloc(n);
	if (p != NULL)
		memcpy(p, s, n);
	return p;
}

static char *fixed_generator(const char *text, int state)
{
	static const char *pool[] = { "alpha", "alphabet", "alpine", "beta", NULL };
	while (pool[state] != NULL) {
		if (strncmp(pool[state], text, strlen(text)) == 0)
			return dup_str(pool[state++]);
		state++;
	}
	return NULL;
}

/* A generator that returns nothing, which is the path where the caller has to
 * cope with a NULL match list rather than an empty one. */
static char *empty_generator(const char *text, int state)
{
	(void)text;
	(void)state;
	return NULL;
}

/*
 * The match SET, sorted, never the order the library left them in.
 *
 * ERR-readline-01: the C sorts with `strcmp` cast to qsort's comparator type,
 * so the comparator is handed the ADDRESSES of the elements and orders them
 * by pointer representation. `plan/decisions/conformance-policy.md` lists it
 * among the forks that default to reproduce, and the port does reproduce it —
 * which makes the resulting order unmatchable by construction, because the
 * two sides sort by addresses from two different allocators.
 *
 * So this compares what the two libraries agree on and can agree on: which
 * strings came back. Index 0 is the common prefix and stays where it is; it
 * is computed from adjacent pairs of the sorted array, so it is part of what
 * the defect corrupts and worth watching.
 */
static void show_matches(const char *label, char **m)
{
	int i, n;
	op(label);
	if (m == NULL) {
		printf("(null)\n");
		return;
	}
	for (n = 0; m[n] != NULL; n++)
		;
	if (n > 2)
		qsort(m + 1, (size_t)(n - 1), sizeof(m[0]), cmp_str);
	printf("[");
	for (i = 0; m[i] != NULL; i++) {
		if (i)
			putchar(' ');
		/* `pesc`, not `sesc`: the filename completer returns absolute
		 * paths, and the two sides run in different work directories,
		 * so a raw path can never compare equal. */
		pesc(m[i]);
		free(m[i]);
	}
	printf("] n=%d\n", i);
	free(m);
}

static void section_completion(void)
{
	/* The common prefix readline puts at index 0 is the interesting part:
	 * "alp" for the three alpha* entries, the word itself for one match. */
	show_matches("completion_matches alp",
	    completion_matches((char *)"alp", fixed_generator));
	show_matches("rl_completion_matches alp",
	    rl_completion_matches("alp", fixed_generator));
	show_matches("rl_completion_matches alpha",
	    rl_completion_matches("alpha", fixed_generator));
	show_matches("rl_completion_matches b",
	    rl_completion_matches("b", fixed_generator));
	show_matches("rl_completion_matches empty",
	    rl_completion_matches("", fixed_generator));
	show_matches("rl_completion_matches miss",
	    rl_completion_matches("zzz", fixed_generator));
	show_matches("rl_completion_matches none",
	    rl_completion_matches("a", empty_generator));

	/* The filename completer through the readline name, over the directory
	 * section 5 made. */
	{
		char buf[512];
		snprintf(buf, sizeof(buf), "%s/comp/a", workdir);
		show_matches("rl_completion_matches file",
		    rl_completion_matches(buf, rl_filename_completion_function));
	}

	/* The display path, with a well-formed list. Output goes to
	 * rl_outstream, which is /dev/null, so only the return matters. */
	{
		char *m[] = { (char *)"alp", (char *)"alpha", (char *)"alpine", NULL };
		op("rl_display_match_list");
		rl_display_match_list(m, 2, 6);
		printf("void\n");
	}

	/* username_completion_function reads the passwd database. The answer
	 * depends on the host, but both libraries read the same one, so the
	 * comparison holds; only its shape enters the trace. */
	{
		char *first = username_completion_function("r", 0);
		op("username_completion r");
		printf("%s\n", first ? "matched" : "(null)");
		free(first);
	}
}

/* --------------------------------------------------------------------- */
/* 9. The line buffer and the keymaps                                     */
/* --------------------------------------------------------------------- */

static int a_command(int count, int key)
{
	(void)count;
	(void)key;
	return 0;
}

static void show_line(const char *label)
{
	op(label);
	printf("point=%d end=%d buf=", rl_point, rl_end);
	sesc(rl_line_buffer);
	putchar('\n');
}

static void section_line_and_keymaps(void)
{
	Keymap km, bare;
	char *copy;

	show_line("line initial");
	op("rl_insert_text abc");   printf("%d\n", rl_insert_text("abc"));
	show_line("after insert");
	op("rl_insert_text def");   printf("%d\n", rl_insert_text("def"));
	show_line("after second insert");

	op("rl_copy_text 0..3");    copy = rl_copy_text(0, 3); sesc(copy); putchar('\n'); free(copy);
	/* ERR-readline-06: `strlcpy(out, src, len)` with `len == 0` writes
	 * NOTHING — strlcpy's third argument is the destination size — so the C
	 * hands back an uninitialised buffer. Only whether it allocated is
	 * comparable; the bytes are indeterminate and differ run to run. */
	op("rl_copy_text 2..2 len");
	copy = rl_copy_text(2, 2);
	printf("%s\n", copy ? "allocated" : "(null)");
	free(copy);
	op("rl_delete_text 1..3");  printf("%d\n", rl_delete_text(1, 3));
	show_line("after delete");
	op("rl_replace_line");      rl_replace_line("replaced", 0); printf("void\n");
	show_line("after replace");
	op("rl_kill_text 0..2");    printf("%d\n", rl_kill_text(0, 2));
	show_line("after kill");
	op("rl_on_new_line");       printf("%d\n", rl_on_new_line());
	op("rl_free_line_state");   rl_free_line_state(); printf("void\n");

	op("rl_set_prompt");        printf("%d\n", rl_set_prompt("> "));
	op("rl_save_prompt");       rl_save_prompt(); printf("void\n");
	op("rl_set_prompt 2");      printf("%d\n", rl_set_prompt("$ "));
	op("rl_restore_prompt");    rl_restore_prompt(); printf("void\n");
	op("rl_prompt after");      sesc(rl_prompt); putchar('\n');

	{
		int rows = -1, cols = -1;
		op("rl_get_screen_size");   rl_get_screen_size(&rows, &cols);
		printf("rows=%d cols=%d\n", rows, cols);
		op("rl_set_screen_size");   rl_set_screen_size(30, 100); printf("void\n");
		rows = cols = -1;
		op("rl_get_screen_size 2");  rl_get_screen_size(&rows, &cols);
		printf("rows=%d cols=%d\n", rows, cols);
		/* Both out-parameters NULL: the C writes through neither. */
		op("rl_get_screen_size NULL"); rl_get_screen_size(NULL, NULL); printf("void\n");
	}

	op("rl_add_defun");         printf("%d\n", rl_add_defun("a-command", a_command, -1));
	op("rl_add_defun bound");   printf("%d\n", rl_add_defun("b-command", a_command, 'B'));
	op("rl_bind_key");          printf("%d\n", rl_bind_key('A', a_command));
	op("rl_bind_key NULL fn");  printf("%d\n", rl_bind_key('C', NULL));

	op("rl_get_keymap");        km = rl_get_keymap(); printf("%d\n", km != NULL);
	op("rl_make_bare_keymap");  bare = rl_make_bare_keymap(); printf("%d\n", bare != NULL);
	op("rl_set_keymap");        rl_set_keymap(km); printf("void\n");
	op("rl_set_keymap_name");   printf("%d\n", rl_set_keymap_name("named", bare));
	op("rl_generic_bind");      printf("%d\n", rl_generic_bind(0, "\\C-x", "text", km));
	op("rl_bind_key_in_map");   printf("%d\n", rl_bind_key_in_map('D', a_command, km));
	op("rl_set_key");           printf("%d\n", rl_set_key("\\C-y", a_command, km));

	op("rl_stuff_char");        printf("%d\n", rl_stuff_char('z'));
	op("rl_message");           rl_message("ignored %d", 1); printf("void\n");
	op("rl_ding");              printf("%d\n", rl_ding());
	op("rl_crlf");              printf("%d\n", rl_crlf());
	/* `_rl_abort_internal` is deliberately absent: in the C it does not
	 * return, so calling it here kills the oracle mid-run and truncates the
	 * trace at whatever operation happened to be next. It sits in
	 * conformance/aux/ub_corpus.c, where a process that dies is the
	 * measurement rather than a lost run. */
}

/* --------------------------------------------------------------------- */

int main(int argc, char **argv)
{
	const char *loc;
	FILE *devnull;

	if (argc != 2) {
		fprintf(stderr, "usage: %s <workdir>\n", argv[0]);
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

	loc = setlocale(LC_ALL, "");
	op("setlocale");
	printf("%s\n", loc ? loc : "(null)");

	/* Point the library at files before anything can initialize an editor
	 * against the real stdin, and swallow any escape sequence it emits so
	 * the trace stays the trace. */
	devnull = fopen("/dev/null", "w");
	if (devnull == NULL)
		return 2;
	rl_instream = stdin;
	rl_outstream = devnull;

	section_history_list();
	section_expand();
	section_tokenize();
	section_file();
	section_expansion();
	section_binding();
	section_completion();
	section_line_and_keymaps();
	section_variables();

	fclose(devnull);
	op("done");
	printf("%d operations\n", seq);
	return 0;
}
