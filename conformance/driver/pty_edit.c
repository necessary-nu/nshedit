/*
 * conformance driver 4: a real terminal.
 *
 * The other three drive libedit through files, which reaches everything that
 * does not need a keystroke and nothing that does. That leaves the largest
 * single block of the library untested — the editor command sets (vi.c,
 * emacs.c, common.c), the screen refresh (refresh.c) and the terminal writer
 * (terminal.c, tty.c) — because reaching those IS pressing a key.
 *
 * So this one opens a pty, forks, and lets the child run a genuine editing
 * session on it while the parent plays the part of the person typing.
 *
 * # It still looks like the other drivers
 *
 * The trace goes to this program's own stdout, not to the pty, so
 * `differential.sh` diffs it the same way it diffs the rest and needs no
 * special case. What crosses the pty is the editor's output, which the parent
 * collects and escapes into that trace — so the comparison is over the BYTES
 * A TERMINAL WOULD HAVE RECEIVED, which is the one thing no other stage sees.
 *
 * # Determinism
 *
 * A pty is a timing surface, and three things here make the answer not depend
 * on scheduling:
 *
 *   - The parent accumulates every byte and prints once. How a read happens
 *     to split is invisible; only the total matters.
 *   - Each key is followed by a drain that waits for quiet rather than for a
 *     fixed time, so a slow child produces the same bytes as a fast one. The
 *     threshold is short (30ms) because it is a threshold for SILENCE, not a
 *     budget for the child: a reaction that has started keeps the drain
 *     going. determinism.sh runs this driver three times per library per
 *     locale and is what proves the number is not too small.
 *   - The window size is set explicitly, TERM is the harness's pinned `dumb`,
 *     and nothing consults the clock.
 *
 * `dumb` is deliberate rather than convenient: its terminfo entry has almost
 * no capabilities, so refresh has to fall back to its own arithmetic instead
 * of delegating to the terminal, and the emitted bytes stay small enough to
 * read in a diff. A richer TERM belongs in a later pass, once this one is
 * quiet.
 */
/*
 * `posix_openpt`, `grantpt`, `unlockpt` and `ptsname` are XSI, and the
 * harness compiles with -std=c11, which hides them. `_DEFAULT_SOURCE` comes
 * with it because <sys/ioctl.h>'s TIOCSCTTY and TIOCSWINSZ are outside any
 * standard at all.
 */
#define _XOPEN_SOURCE 700
#define _DEFAULT_SOURCE

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <locale.h>
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <poll.h>
#include <termios.h>
#include <sys/ioctl.h>
#include <sys/wait.h>

#include <histedit.h>

static int seq = 0;
static const char *workdir;

static void op(const char *label)
{
	printf("%04d %-26s ", ++seq, label);
}

/* Bytes -> pure ASCII, losing nothing. The editor's output is escape
 * sequences and control characters, so this is most of the trace. */
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

/* --------------------------------------------------------------------- */
/* The child: a real editing session on the pty                           */
/* --------------------------------------------------------------------- */

static char *prompt(EditLine *el)
{
	(void)el;
	return (char *)"> ";
}

/*
 * Runs in the child, with `slave` already its controlling terminal. Never
 * returns: the parent reads what it wrote and reaps it.
 */
static void child_session(void)
{
	EditLine *el;
	History *h;
	HistEvent ev;
	const char *line;
	int count;

	el = el_init("pty", stdin, stdout, stderr);
	if (el == NULL)
		_exit(70);

	h = history_init();
	if (h == NULL)
		_exit(71);
	history(h, &ev, H_SETSIZE, 32);

	el_set(el, EL_EDITOR, "emacs");
	el_set(el, EL_PROMPT, prompt);
	el_set(el, EL_HIST, history, h);
	el_set(el, EL_SIGNAL, 0);

	/* Each returned line is echoed back with a marker, so the trace shows
	 * what the editor decided the line WAS as well as what it drew. */
	while ((line = el_gets(el, &count)) != NULL && count > 0) {
		fputs("[line]", stdout);
		fputs(line, stdout);
		fflush(stdout);
		if (line[0] != '\n')
			history(h, &ev, H_ENTER, line);
	}

	history_end(h);
	el_end(el);
	/* `exit`, not `_exit`: the editing all happens in this child, and
	 * `_exit` skips the atexit handlers — including the one that writes the
	 * coverage profile, which made `conformance/coverage.sh` credit this
	 * driver with zero rules while it was plainly driving the editor.
	 * Everything that would be flushed here has been flushed already. */
	exit(0);
}

/* --------------------------------------------------------------------- */
/* The parent: types, and collects                                        */
/* --------------------------------------------------------------------- */

static unsigned char collected[1 << 16];
static size_t collected_len;

/*
 * Reads until the pty has been quiet for `quiet_ms`, or until it closes.
 *
 * Waiting for silence rather than for a fixed delay is what makes the byte
 * stream independent of how fast the child runs: a slow child simply takes
 * more polls to say the same thing.
 */
static void drain(int master, int quiet_ms)
{
	for (;;) {
		struct pollfd p = { .fd = master, .events = POLLIN };
		int r = poll(&p, 1, quiet_ms);
		if (r <= 0)
			return;
		unsigned char buf[4096];
		ssize_t n = read(master, buf, sizeof(buf));
		if (n <= 0)
			return;
		if (collected_len + (size_t)n <= sizeof(collected)) {
			memcpy(collected + collected_len, buf, (size_t)n);
			collected_len += (size_t)n;
		}
	}
}

/* Types one key sequence and waits for the editor to finish reacting. */
static void type(int master, const char *keys)
{
	size_t n = strlen(keys);
	if (write(master, keys, n) < 0)
		return;
	drain(master, 30);
}

/*
 * The script. Every entry is a label and the bytes to send; the label is what
 * a divergence will be reported against, so it names the editing operation
 * rather than the control character.
 */
static const struct {
	const char *label;
	const char *keys;
} script[] = {
	{ "type hello",              "hello" },
	{ "move to beginning ^A",    "\001" },
	{ "move to end ^E",          "\005" },
	{ "back one ^B",             "\002" },
	{ "forward one ^F",          "\006" },
	{ "delete prev char DEL",    "\177" },
	{ "kill to end ^K",          "\013" },
	{ "type again",              "world" },
	{ "kill whole line ^U",      "\025" },
	{ "type a first line",       "alpha beta" },
	{ "accept it",               "\n" },
	{ "type a second line",      "gamma delta" },
	{ "accept it too",           "\n" },
	{ "previous history ^P",     "\020" },
	{ "previous again",          "\020" },
	{ "next history ^N",         "\016" },
	{ "back a word ESC-b",       "\033b" },
	{ "forward a word ESC-f",    "\033f" },
	{ "upcase word ESC-u",       "\033u" },
	{ "downcase word ESC-l",     "\033l" },
	{ "capitalise ESC-c",        "\033c" },
	{ "transpose ^T",            "\024" },
	{ "yank ^Y",                 "\031" },
	{ "clear and retype",        "\025typed" },
	{ "accept the last line",    "\n" },
	{ "end of input ^D",         "\004" },
};

int main(int argc, char **argv)
{
	int master, slave;
	pid_t pid;
	const char *name;
	struct winsize ws = { .ws_row = 24, .ws_col = 80, 0, 0 };
	int st = 0;

	if (argc != 2) {
		fprintf(stderr, "usage: %s <workdir>\n", argv[0]);
		return 2;
	}
	workdir = argv[1];
	(void)workdir;
	setvbuf(stdout, NULL, _IOLBF, 0);
	setlocale(LC_ALL, "");

	/* POSIX pty allocation, so nothing links against libutil. */
	master = posix_openpt(O_RDWR | O_NOCTTY);
	if (master < 0 || grantpt(master) < 0 || unlockpt(master) < 0) {
		op("posix_openpt");
		printf("failed errno=%d\n", errno);
		return 1;
	}
	name = ptsname(master);
	if (name == NULL) {
		op("ptsname");
		printf("failed\n");
		return 1;
	}
	/* The size is fixed rather than inherited: it decides every wrap
	 * decision refresh makes, and an inherited one would make the trace
	 * depend on the window the harness happened to run in. */
	ioctl(master, TIOCSWINSZ, &ws);

	op("pty opened");
	printf("yes\n");

	fflush(stdout);
	pid = fork();
	if (pid < 0) {
		op("fork");
		printf("failed\n");
		return 1;
	}
	if (pid == 0) {
		close(master);
		slave = open(name, O_RDWR);
		if (slave < 0)
			_exit(72);
		if (setsid() < 0)
			_exit(73);
		if (ioctl(slave, TIOCSCTTY, 0) < 0)
			_exit(74);
		dup2(slave, 0);
		dup2(slave, 1);
		dup2(slave, 2);
		if (slave > 2)
			close(slave);
		child_session();
	}

	/* The initial prompt, before anything is typed. */
	drain(master, 150);
	op("prompt drawn");
	besc(collected, collected_len);
	putchar('\n');
	collected_len = 0;

	for (size_t i = 0; i < sizeof(script) / sizeof(script[0]); i++) {
		type(master, script[i].keys);
		op(script[i].label);
		besc(collected, collected_len);
		putchar('\n');
		collected_len = 0;
	}

	/* Whatever the child says on its way out. */
	drain(master, 200);
	op("after end of input");
	besc(collected, collected_len);
	putchar('\n');

	close(master);
	waitpid(pid, &st, 0);
	op("child exit");
	if (WIFEXITED(st))
		printf("exit=%d\n", WEXITSTATUS(st));
	else if (WIFSIGNALED(st))
		printf("signal=%d\n", WTERMSIG(st));
	else
		printf("unknown\n");

	op("done");
	printf("%d operations\n", seq);
	return 0;
}
