/*
 * A fresh C consumer for the installed headers, pkg-config metadata, shared
 * object, and runtime search path. It uses the narrow character surface and
 * touches nothing that needs a terminal.
 *
 * Exit 0 and one line of output on success; non-zero and a reason otherwise.
 */
#include <stdio.h>
#include <string.h>
#include <histedit.h>

static int fail(const char *what)
{
	printf("FAIL: %s\n", what);
	return 1;
}

int main(void)
{
	History *h;
	HistEvent ev;
	Tokenizer *t;
	int argc, rc;
	const char **argv;

	h = history_init();
	if (h == NULL)
		return fail("history_init returned NULL");
	if (history(h, &ev, H_SETSIZE, 8) == -1)
		return fail("H_SETSIZE");
	if (history(h, &ev, H_ENTER, "alpha") == -1)
		return fail("H_ENTER alpha");
	if (history(h, &ev, H_ENTER, "beta") == -1)
		return fail("H_ENTER beta");
	if (history(h, &ev, H_FIRST) == -1)
		return fail("H_FIRST");
	if (ev.str == NULL || strcmp(ev.str, "beta") != 0)
		return fail("H_FIRST is not the newest event");
	if (history(h, &ev, H_NEXT) == -1)
		return fail("H_NEXT");
	if (ev.str == NULL || strcmp(ev.str, "alpha") != 0)
		return fail("H_NEXT is not the older event");
	history_end(h);

	t = tok_init(NULL);
	if (t == NULL)
		return fail("tok_init returned NULL");
	rc = tok_str(t, "one 'two three' four", &argc, &argv);
	if (rc != 0)
		return fail("tok_str did not report a complete line");
	if (argc != 3)
		return fail("tok_str split the line into the wrong number of words");
	if (strcmp(argv[1], "two three") != 0)
		return fail("tok_str did not keep the quoted word together");
	tok_end(t);

	printf("ok: history and tokenizer both behaved\n");
	return 0;
}
