/*
 * Compatibility behaviours that were documented but not previously driven.
 *
 * This is an ordinary differential driver except that its initial port diff
 * is recorded exactly under conformance/known-gaps/. The next implementation
 * node removes each divergence and then removes that fixture; the harness
 * rejects both an unrecorded change and a stale fixture after equality.
 */
#define _XOPEN_SOURCE 700
#define _DEFAULT_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <locale.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/time.h>
#include <termios.h>
#include <unistd.h>

#include <histedit.h>
#include <editline/readline.h>

static int seq;
static int event_master = -1;
static volatile sig_atomic_t event_calls;

static void op(const char *label)
{
	printf("%04d %-30s ", ++seq, label);
}

static void besc(const char *s)
{
	putchar('<');
	if (s == NULL) {
		fputs("(null)", stdout);
	} else {
		for (; *s != '\0'; s++) {
			unsigned c = (unsigned char)*s;
			if (c >= 0x20 && c < 0x7f && c != '\\' && c != '<' && c != '>')
				putchar((int)c);
			else
				printf("\\x%02X", c);
		}
	}
	putchar('>');
}

static char *test_prompt(EditLine *el)
{
	(void)el;
	return (char *)"prompt> ";
}

static char *test_rprompt(EditLine *el)
{
	(void)el;
	return (char *)" <rprompt";
}

/* [spec:libedit:sem:histedit.el-get-fn/test] */
static void section_narrow_get(void)
{
	FILE *fin = tmpfile();
	FILE *fout = tmpfile();
	FILE *ferr = tmpfile();
	EditLine *el;
	const char *value;
	char *(*prompt)(EditLine *);
	char esc;
	int rc;

	if (fin == NULL || fout == NULL || ferr == NULL) {
		op("tmpfile");
		printf("failed errno=%d\n", errno);
		return;
	}
	el = el_init("abi-gaps", fin, fout, ferr);
	if (el == NULL) {
		op("el_init");
		printf("failed\n");
		return;
	}

	el_set(el, EL_EDITOR, "vi");
	value = NULL;
	rc = el_get(el, EL_EDITOR, &value);
	op("el_get EL_EDITOR");
	printf("rc=%d value=", rc);
	besc(value);
	putchar('\n');

	el_set(el, EL_WORDCHARS, "abc:_");
	value = NULL;
	rc = el_get(el, EL_WORDCHARS, &value);
	op("el_get EL_WORDCHARS");
	printf("rc=%d value=", rc);
	besc(value);
	putchar('\n');

	el_set(el, EL_PROMPT, test_prompt);
	prompt = NULL;
	rc = el_get(el, EL_PROMPT, &prompt);
	op("el_get EL_PROMPT");
	printf("rc=%d same=%d\n", rc, prompt == test_prompt);

	el_set(el, EL_PROMPT_ESC, test_prompt, 0x7f);
	prompt = NULL;
	esc = 0;
	rc = el_get(el, EL_PROMPT_ESC, &prompt, &esc);
	op("el_get EL_PROMPT_ESC");
	printf("rc=%d same=%d esc=%d\n", rc, prompt == test_prompt,
	    (unsigned char)esc);

	el_set(el, EL_RPROMPT, test_rprompt);
	prompt = NULL;
	rc = el_get(el, EL_RPROMPT, &prompt);
	op("el_get EL_RPROMPT");
	printf("rc=%d same=%d\n", rc, prompt == test_rprompt);

	el_set(el, EL_RPROMPT_ESC, test_rprompt, 0x1d);
	prompt = NULL;
	esc = 0;
	rc = el_get(el, EL_RPROMPT_ESC, &prompt, &esc);
	op("el_get EL_RPROMPT_ESC");
	printf("rc=%d same=%d esc=%d\n", rc, prompt == test_rprompt,
	    (unsigned char)esc);

	el_end(el);
	fclose(fin);
	fclose(fout);
	fclose(ferr);
}

/* [spec:libedit:sem:readline.rl-initialize-fn/test] */
static void section_readline_globals(void)
{
	FILE *fin;
	FILE *fout;
	char *envterm;
	int rc;

	rl_catch_signals = 0;
	rl_instream = NULL;
	rl_outstream = NULL;
	rc = rl_initialize();
	envterm = getenv("TERM");
	op("rl_initialize std streams");
	printf("rc=%d in=%d out=%d termenv=%d\n", rc,
	    rl_instream == stdin, rl_outstream == stdout,
	    envterm != NULL && rl_terminal_name == envterm);

	/* The reference keeps getenv("TERM") here.  The port currently lends a
	 * pointer owned by its EditLine adapter, which the next rl_initialize
	 * destroys before reusing the global.  Record the ownership difference
	 * above, then install a stable caller-owned value so the rest of this
	 * oracle does not itself dereference the known dangling pointer. */
	rl_terminal_name = envterm;

	fin = tmpfile();
	fout = tmpfile();
	if (fin == NULL || fout == NULL) {
		op("readline tmpfile");
		printf("failed errno=%d\n", errno);
		return;
	}
	rl_instream = fin;
	rl_outstream = fout;
	rc = rl_initialize();
	op("rl_initialize custom streams");
	printf("rc=%d infd=%d outfd=%d\n", rc,
	    fileno(rl_instream) == fileno(fin),
	    fileno(rl_outstream) == fileno(fout));
}

/* [spec:libedit:sem:readline.rl-message-fn/test] */
static void section_message(void)
{
	char longmsg[201];

	rl_set_prompt("original");
	rl_message("value=%d/%s/%c", 42, "ok", 'Z');
	op("rl_message formatting");
	besc(rl_prompt);
	putchar('\n');

	memset(longmsg, 'm', sizeof(longmsg) - 1);
	longmsg[sizeof(longmsg) - 1] = '\0';
	rl_message("%s", longmsg);
	op("rl_message truncation");
	printf("len=%zu\n", strlen(rl_prompt));
}

/* [spec:libedit:sem:readline.rl-kill-full-line-fn/test] */
static void section_kill_line(void)
{
	char *copy;
	int rc;

	rl_replace_line("alpha beta", 0);
	rc = rl_kill_full_line(9, 'x');
	/* The kill call deliberately does not refresh rl_line_buffer/rl_end.
	 * Insert bytes through the editor and then inspect them: after a real kill
	 * they start the line; after the current no-op they precede old text. */
	rl_insert_text("XY");
	copy = rl_copy_text(0, 4);
	op("rl_kill_full_line");
	printf("rc=%d line=", rc);
	besc(copy);
	putchar('\n');
	free(copy);
}

static int event_hook(void)
{
	event_calls++;
	if (event_calls == 3 && event_master >= 0)
		(void)write(event_master, "x\n", 2);
	return 0;
}

static void alarm_write(int sig)
{
	(void)sig;
	if (event_master >= 0)
		(void)write(event_master, "z\n", 2);
}

static void arm_watchdog(void)
{
	struct sigaction sa;
	struct itimerval timer;

	memset(&sa, 0, sizeof(sa));
	sa.sa_handler = alarm_write;
	sa.sa_flags = SA_RESTART;
	sigemptyset(&sa.sa_mask);
	sigaction(SIGALRM, &sa, NULL);

	memset(&timer, 0, sizeof(timer));
	timer.it_value.tv_usec = 100000;
	setitimer(ITIMER_REAL, &timer, NULL);
}

static void disarm_watchdog(void)
{
	struct itimerval timer;
	memset(&timer, 0, sizeof(timer));
	setitimer(ITIMER_REAL, &timer, NULL);
}

/* [spec:libedit:sem:readline.rl-event-read-char-fn/test] */
static void section_event_poll(void)
{
	int master;
	int slave;
	const char *name;
	FILE *fin;
	FILE *fout;
	char *line;

	master = posix_openpt(O_RDWR | O_NOCTTY);
	if (master < 0 || grantpt(master) < 0 || unlockpt(master) < 0) {
		op("event pty");
		printf("failed errno=%d\n", errno);
		return;
	}
	name = ptsname(master);
	if (name == NULL || (slave = open(name, O_RDWR | O_NOCTTY)) < 0) {
		op("event slave");
		printf("failed errno=%d\n", errno);
		close(master);
		return;
	}
	fin = fdopen(dup(slave), "r");
	fout = fdopen(dup(slave), "w");
	if (fin == NULL || fout == NULL) {
		op("event streams");
		printf("failed errno=%d\n", errno);
		close(master);
		close(slave);
		return;
	}

	event_master = master;
	event_calls = 0;
	rl_instream = fin;
	rl_outstream = fout;
	rl_event_hook = event_hook;
	rl_catch_signals = 0;
	rl_initialize();
	arm_watchdog();
	line = readline("event> ");
	disarm_watchdog();

	op("rl_event_hook polling");
	/* The reference busy-spins until the pty line discipline publishes the
	 * bytes, so its exact call count is scheduling-dependent. The property is
	 * whether it reached the hook's third-call write before the watchdog. */
	printf("spun=%d line=", event_calls >= 3);
	besc(line);
	putchar('\n');

	free(line);
	rl_event_hook = NULL;
	event_master = -1;
	fclose(fin);
	fclose(fout);
	close(slave);
	close(master);
}

/* [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test] */
/* [spec:libedit:sem:readline.tilde-expand-fn/test] */
static void section_tilde_bytes(void)
{
	char input[] = {'x', (char)0xff, 'y', '\0'};
	char *expanded = tilde_expand(input);

	op("fn_tilde_expand bytes");
	besc(expanded);
	putchar('\n');
	free(expanded);
}

/* [spec:nshedit:req:abi.behavioural-conformance/test] */
int main(int argc, char **argv)
{
	(void)argv;
	if (argc != 2) {
		fprintf(stderr, "usage: abi_gaps <workdir>\n");
		return 2;
	}
	setvbuf(stdout, NULL, _IOLBF, 0);
	setlocale(LC_ALL, "");

	section_narrow_get();
	section_readline_globals();
	section_message();
	section_kill_line();
	section_event_poll();
	section_tilde_bytes();

	op("done");
	printf("%d operations\n", seq);
	return 0;
}
