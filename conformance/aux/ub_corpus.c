/*
 * The undefined-behaviour corpus: calls the C has no defined answer for.
 *
 * This is NOT a differential driver, and it deliberately does not live in
 * `driver/`. The other three prove the port and the oracle agree; here they
 * are *expected* to disagree, because the whole point of a `disposition:
 * define` entry in `docs/errata.md` is that the C is undefined and the port
 * is not. Diffing the traces would report every success as a failure.
 *
 * So the shape is one-sided, and `conformance/ub.sh` reads it that way:
 *
 *   the PORT must survive every case.   That is the pass condition.
 *   the ORACLE is run too, and whatever it does is reported.
 *
 * Running the oracle is not decoration. A case the C also survives is a case
 * that proves nothing — it might be an invented hazard rather than a real
 * one — and the report says which is which, so the corpus can be judged
 * rather than trusted.
 *
 * # Every case is forked
 *
 * These calls are expected to crash. A single process would stop at the first
 * one and report nothing about the rest, so each runs in a child and the
 * parent reports how it ended: exited, or killed by a signal, and which. The
 * child also carries an alarm, because "hangs forever" is a failure mode the
 * C has too and a test suite must not inherit.
 *
 * # Provenance
 *
 * Every case cites the errata id it came from. The register is the source of
 * the corpus, not my imagination: `docs/errata.md` holds 120 UB entries, 25
 * of which say in their own `reach:` line that they are reachable across the
 * public C ABI, and these are the ones reachable without a terminal.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <locale.h>
#include <unistd.h>
#include <sys/wait.h>

#include <histedit.h>
#include <editline/readline.h>

static int seq = 0;
static const char *workdir;
static EditLine *el;
static History *hist;

/* --------------------------------------------------------------------- */

/*
 * Runs `body` in a child and reports how it ended.
 *
 * The label carries the errata id, so a failure names the entry whose
 * disposition was not carried out rather than just an operation number.
 */
static void probe(const char *err, const char *label, void (*body)(void))
{
	fflush(stdout);
	pid_t pid = fork();
	if (pid == 0) {
		/* Undefined behaviour includes not terminating. */
		alarm(10);
		/* The child's own output would interleave with the parent's
		 * trace; only the verdict matters here. */
		freopen("/dev/null", "w", stdout);
		freopen("/dev/null", "w", stderr);
		body();
		_exit(0);
	}
	int st = 0;
	waitpid(pid, &st, 0);

	printf("%04d %-18s %-34s ", ++seq, err, label);
	if (WIFEXITED(st))
		printf("survived exit=%d\n", WEXITSTATUS(st));
	else if (WIFSIGNALED(st))
		printf("KILLED signal=%d\n", WTERMSIG(st));
	else
		printf("unknown wait status\n");
}

/* --------------------------------------------------------------------- */
/* The cases                                                              */
/* --------------------------------------------------------------------- */

/* ERR-encoding-05: ct_decode_argv wraps the `argc + 1` allocation count. */
static void ub_parse_negative_argc(void)
{
	const char *argv[] = { "bind", "-e", NULL };
	el_parse(el, -1, argv);
}

/* ERR-core-api-07: the collection loop reads until a NULL that is not there. */
static void ub_settc_no_sentinel(void)
{
	/* Nineteen arguments and no terminator, which is the case the entry
	 * calls "reachable by construction". */
	el_set(el, EL_BIND, "a", "b", "c", "d", "e", "f", "g", "h", "i", "j",
	    "k", "l", "m", "n", "o", "p", "q", "r", "s");
}

/* ERR-terminal-07: an empty argv reaches strlcpy with a NULL. */
static void ub_setty_empty(void)
{
	el_set(el, EL_SETTY, NULL);
}

/* ERR-core-api-08: EL_EDITOR NULL reaches wcscmp. */
static void ub_editor_null(void)
{
	el_set(el, EL_EDITOR, NULL);
}

/* ERR-core-api-09: an argument the locale cannot decode reaches wcsdup. */
static void ub_wordchars_bad_encoding(void)
{
	el_set(el, EL_WORDCHARS, "\xff\xfe\xfd");
}

/* ERR-core-api-05: el_line(NULL) offsets a member from a null pointer. */
static void ub_line_null(void)
{
	const LineInfo *li = el_line(NULL);
	/* Force the read the entry is about. */
	if (li != NULL)
		(void)li->cursor;
}

/* ERR-buffer-11: el_cursor forms the pointer before clamping it. */
static void ub_cursor_far(void)
{
	el_insertstr(el, "abc");
	el_cursor(el, 1 << 30);
	el_cursor(el, -(1 << 30));
}

/* ERR-buffer-24: a negative start is never rejected. */
static void ub_deletestr_negative(void)
{
	el_insertstr(el, "abcdef");
	el_deletestr(el, -3);
}

/* ERR-history-40: an unchecked caller pointer reaches Strlen/Strdup. */
static void ub_history_enter_null(void)
{
	HistEvent ev;
	history(hist, &ev, H_ENTER, NULL);
}

/* Same entry, the other shape: a NULL through H_APPEND. */
static void ub_history_append_null(void)
{
	HistEvent ev;
	history(hist, &ev, H_ENTER, "seed");
	history(hist, &ev, H_APPEND, NULL);
}

/* ERR-history-40's sibling: H_LOAD with a NULL filename. */
static void ub_history_load_null(void)
{
	HistEvent ev;
	history(hist, &ev, H_LOAD, NULL);
}

/* ERR-readline-04: rl_bind_key indexes el_map.key with no range check. */
static void ub_bind_key_out_of_range(void)
{
	rl_bind_key(-1, rl_insert);
	rl_bind_key(1 << 20, rl_insert);
}

/* ERR-readline-07: rl_copy_text reads before the buffer. */
static void ub_copy_text_negative(void)
{
	char *s = rl_copy_text(-5, 3);
	free(s);
}

/* ERR-readline-09: rl_save_prompt strdups rl_prompt without a NULL check. */
static void ub_save_prompt_null(void)
{
	rl_prompt = NULL;
	rl_save_prompt();
	rl_restore_prompt();
}

/* ERR-completion-02 and -03: a negative len reaches the display loop. */
static void ub_display_match_list_negative(void)
{
	char *matches[] = { (char *)"pre", (char *)"prefix", NULL };
	rl_display_match_list(matches, -1, 6);
}

/* ERR-completion-02: and a count that overruns the array. */
static void ub_display_match_list_overrun(void)
{
	char *matches[] = { (char *)"pre", (char *)"prefix", NULL };
	rl_display_match_list(matches, 99, 6);
}

/* ERR-buffer-10: el_insertstr past the buffer limit. */
static void ub_insertstr_enormous(void)
{
	static char big[1 << 20];
	memset(big, 'x', sizeof(big) - 1);
	big[sizeof(big) - 1] = '\0';
	el_insertstr(el, big);
}

/* ERR-core-api-11: el_gets with a NULL count. Reading from an empty stdin,
 * so this returns immediately at EOF and the entry's dereference is the
 * only interesting part. */
static void ub_gets_null_count(void)
{
	(void)el_gets(el, NULL);
}

/* No erratum: the register has no entry for a NULL out-parameter here, and
 * the C stores through it unconditionally. Found by running this corpus. */
static void ub_history_expand_null_out(void)
{
	char line[] = "!!";
	(void)history_expand(line, NULL);
}

/* No erratum either. `tilde_expand(NULL)` reaches `strlen` in the C. */
static void ub_tilde_null(void)
{
	free(tilde_expand(NULL));
}

/* --------------------------------------------------------------------- */

int main(int argc, char **argv)
{
	FILE *devnull;
	HistEvent ev;

	if (argc != 2) {
		fprintf(stderr, "usage: %s <workdir>\n", argv[0]);
		return 2;
	}
	workdir = argv[1];
	setvbuf(stdout, NULL, _IOLBF, 0);
	setlocale(LC_ALL, "");

	devnull = fopen("/dev/null", "w");
	if (devnull == NULL)
		return 2;

	/* A live editor and history for the cases that need one. Built in the
	 * parent so every child inherits the same state, which is what makes
	 * the cases independent of each other. */
	el = el_init("ub", stdin, devnull, devnull);
	if (el == NULL)
		return 2;
	el_set(el, EL_PREP_TERM, 0);
	el_set(el, EL_SIGNAL, 0);

	hist = history_init();
	if (hist == NULL)
		return 2;
	history(hist, &ev, H_SETSIZE, 10);

	rl_instream = stdin;
	rl_outstream = devnull;
	rl_initialize();

	probe("ERR-encoding-05", "el_parse argc=-1", ub_parse_negative_argc);
	probe("ERR-core-api-07", "EL_BIND 19 args, no NULL", ub_settc_no_sentinel);
	probe("ERR-terminal-07", "EL_SETTY empty argv", ub_setty_empty);
	probe("ERR-core-api-08", "EL_EDITOR NULL", ub_editor_null);
	probe("ERR-core-api-09", "EL_WORDCHARS undecodable", ub_wordchars_bad_encoding);
	probe("ERR-core-api-05", "el_line(NULL)", ub_line_null);
	probe("ERR-buffer-11", "el_cursor +/-2^30", ub_cursor_far);
	probe("ERR-buffer-24", "el_deletestr(-3)", ub_deletestr_negative);
	probe("ERR-history-40", "H_ENTER NULL", ub_history_enter_null);
	probe("ERR-history-40", "H_APPEND NULL", ub_history_append_null);
	probe("ERR-history-40", "H_LOAD NULL", ub_history_load_null);
	probe("ERR-readline-04", "rl_bind_key out of range", ub_bind_key_out_of_range);
	probe("ERR-readline-07", "rl_copy_text(-5,3)", ub_copy_text_negative);
	probe("ERR-readline-09", "rl_save_prompt NULL prompt", ub_save_prompt_null);
	probe("ERR-completion-02", "match_list len=-1", ub_display_match_list_negative);
	probe("ERR-completion-02", "match_list len=99", ub_display_match_list_overrun);
	probe("ERR-buffer-10", "el_insertstr 1MB", ub_insertstr_enormous);
	probe("ERR-core-api-11", "el_gets NULL count", ub_gets_null_count);
	probe("(unregistered)", "history_expand NULL out", ub_history_expand_null_out);
	probe("(unregistered)", "tilde_expand(NULL)", ub_tilde_null);

	/* `++seq` and `seq - 1` in one argument list is unsequenced, which is
	 * the same class of defect this file exists to find. Count first. */
	int cases = seq;
	printf("%04d %-18s %-34s %d cases\n", ++seq, "-", "done", cases);
	history_end(hist);
	el_end(el);
	fclose(devnull);
	return 0;
}
