/*
 * Signal lifecycle through a real terminal and the built-in reader.
 *
 * The prompt marks when handlers are armed. The parent then observes the
 * child blocked in read(2) through /proc before delivering anything. A
 * separate callback gate makes delivery during foreign code deterministic.
 * Together these make installation, EINTR observation, successful-callback
 * preservation, resize state, cooked-mode ordering, rearming, and final
 * restoration observable without timing guesses.
 */
#define _XOPEN_SOURCE 700
#define _DEFAULT_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <locale.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/select.h>
#include <sys/syscall.h>
#include <sys/wait.h>
#include <termios.h>
#include <unistd.h>
#include <wchar.h>

#include <histedit.h>

static int ready_fd = -1;
static int event_fd = -1;
static int release_fd = -1;
static volatile sig_atomic_t ready_announced;
static volatile sig_atomic_t notify_event;
static volatile sig_atomic_t winch_count;
static volatile sig_atomic_t interrupt_count;
static volatile sig_atomic_t interrupt_saw_cooked;

static void caller_handler(int signo)
{
	char marker = signo == SIGWINCH ? 'W' : 'I';

	if (signo == SIGWINCH)
		winch_count++;
	if (signo == SIGINT) {
		struct termios attributes;
		interrupt_count++;
		if (tcgetattr(STDIN_FILENO, &attributes) == 0 &&
		    (attributes.c_lflag & ICANON) != 0 &&
		    (attributes.c_lflag & ECHO) != 0)
			interrupt_saw_cooked = 1;
	}
	if (notify_event)
		(void)write(event_fd, &marker, 1);
}

static char *prompt(EditLine *el)
{
	(void)el;
	if (!ready_announced) {
		ready_announced = 1;
		(void)write(ready_fd, "R", 1);
	}
	return (char *)"signal> ";
}

static int blocking_reader(EditLine *el, wchar_t *value)
{
	char release;
	ssize_t length;

	(void)el;
	(void)write(ready_fd, "C", 1);
	do {
		length = read(release_fd, &release, 1);
	} while (length < 0 && errno == EINTR);
	if (length != 1)
		return -1;
	*value = L'\n';
	return 1;
}

static int disposition_is_caller(int signo)
{
	struct sigaction current;
	return sigaction(signo, NULL, &current) == 0 &&
	    current.sa_handler == caller_handler;
}

static void install_caller_handler(int signo)
{
	struct sigaction action;
	memset(&action, 0, sizeof(action));
	action.sa_handler = caller_handler;
	action.sa_flags = SA_RESTART;
	sigemptyset(&action.sa_mask);
	if (sigaction(signo, &action, NULL) != 0)
		_exit(75);
}

static void report_read(int report, int sequence, const char *name,
	const char *line, int count, int columns, int restored)
{
	dprintf(report,
	    "%04d %-26s line=%d count=%d columns=%d winch=%d restored=%d\n",
	    sequence, name, line != NULL, count, columns, (int)winch_count,
	    restored);
}

static void child_session(int report)
{
	EditLine *editor;
	const char *line;
	int count, before, columns;

	install_caller_handler(SIGWINCH);
	install_caller_handler(SIGINT);
	editor = el_init("signals", stdin, stdout, stderr);
	if (editor == NULL)
		_exit(76);
	el_set(editor, EL_PROMPT, prompt);

	ready_announced = 0;
	notify_event = 1;
	el_set(editor, EL_SIGNAL, 0);
	line = el_gets(editor, &count);
	columns = -1;
	el_get(editor, EL_GETTC, "co", &columns);
	report_read(report, 1, "signal disabled", line, count, columns,
	    disposition_is_caller(SIGWINCH));

	ready_announced = 0;
	el_set(editor, EL_SIGNAL, 1);
	line = el_gets(editor, &count);
	columns = -1;
	el_get(editor, EL_GETTC, "co", &columns);
	report_read(report, 2, "resize signal", line, count, columns,
	    disposition_is_caller(SIGWINCH));

	ready_announced = 1;
	notify_event = 1;
	el_set(editor, EL_GETCFN, blocking_reader);
	line = el_gets(editor, &count);
	el_set(editor, EL_GETCFN, EL_BUILTIN_GETCFN);
	dprintf(report,
	    "0003 %-26s line=%d count=%d winch=%d restored=%d\n",
	    "callback signal", line != NULL, count, (int)winch_count,
	    disposition_is_caller(SIGWINCH));

	notify_event = 0;
	before = (int)winch_count;
	raise(SIGWINCH);
	dprintf(report, "0004 %-26s restored=%d raised=%d\n", "between reads",
	    disposition_is_caller(SIGWINCH), (int)winch_count - before);

	ready_announced = 0;
	notify_event = 1;
	el_set(editor, EL_UNBUFFERED, 1);
	line = el_gets(editor, &count);
	dprintf(report,
	    "0005 %-26s line=%d count=%d armed=%d winch=%d\n",
	    "unbuffered signal", line != NULL, count,
	    !disposition_is_caller(SIGWINCH),
	    (int)winch_count);
	el_set(editor, EL_UNBUFFERED, 0);
	dprintf(report, "0006 %-26s restored=%d\n", "unbuffered off",
	    disposition_is_caller(SIGWINCH));

	ready_announced = 0;
	notify_event = 1;
	interrupt_saw_cooked = 0;
	line = el_gets(editor, &count);
	dprintf(report,
	    "0007 %-26s line=%d count=%d handled=%d cooked=%d restored=%d\n",
	    "interrupt signal", line != NULL, count, (int)interrupt_count,
	    (int)interrupt_saw_cooked, disposition_is_caller(SIGINT));

	notify_event = 0;
	el_end(editor);
	before = (int)winch_count;
	raise(SIGWINCH);
	dprintf(report, "0008 %-26s restored=%d raised=%d\n", "after el_end",
	    disposition_is_caller(SIGWINCH), (int)winch_count - before);
	exit(0);
}

static int wait_marker(int fd, char expected)
{
	struct pollfd descriptor = { .fd = fd, .events = POLLIN };
	char marker = 0;
	return poll(&descriptor, 1, 3000) == 1 && read(fd, &marker, 1) == 1 &&
	    marker == expected;
}

static int child_is_reading(pid_t child)
{
	char path[64], state[256], *end;
	ssize_t length;
	long syscall_number;
	int fd;

	snprintf(path, sizeof(path), "/proc/%ld/syscall", (long)child);
	fd = open(path, O_RDONLY | O_CLOEXEC);
	if (fd < 0)
		return 0;
	length = read(fd, state, sizeof(state) - 1);
	close(fd);
	if (length <= 0)
		return 0;
	state[length] = '\0';
	errno = 0;
	syscall_number = strtol(state, &end, 10);
	return errno == 0 && end != state && syscall_number == SYS_read;
}

static int wait_for_read(pid_t child)
{
	for (int attempt = 0; attempt < 3000; attempt++) {
		if (child_is_reading(child))
			return 1;
		if (kill(child, 0) != 0)
			return 0;
		(void)poll(NULL, 0, 1);
	}
	return 0;
}

static int send_scenario(pid_t child, int master, int ready, int events,
	int signo, char event, const char *input, unsigned short columns)
{
	struct winsize size = { .ws_row = 24, .ws_col = columns, 0, 0 };
	int delivered = 0;

	if (!wait_marker(ready, 'R'))
		return 0;
	if (!wait_for_read(child))
		return 0;
	if (columns != 0) {
		struct pollfd descriptor = { .fd = events, .events = POLLIN };
		char marker = 0;
		if (ioctl(master, TIOCSWINSZ, &size) != 0)
			return 0;
		if (poll(&descriptor, 1, 500) == 1 &&
		    read(events, &marker, 1) == 1 && marker == event)
			delivered = 1;
	}
	for (int attempt = 0; attempt < 4 && !delivered; attempt++) {
		struct pollfd descriptor = { .fd = events, .events = POLLIN };
		char marker = 0;
		if (kill(child, signo) != 0)
			return 0;
		if (poll(&descriptor, 1, 500) == 1 &&
		    read(events, &marker, 1) == 1 && marker == event)
			delivered = 1;
	}
	if (!delivered)
		return 0;
	if (input != NULL && write(master, input, strlen(input)) < 0)
		return 0;
	return 1;
}

static int send_callback_scenario(pid_t child, int ready, int events,
	int release)
{
	if (!wait_marker(ready, 'C'))
		return 0;
	if (kill(child, SIGWINCH) != 0 || write(release, "R", 1) != 1)
		return 0;
	return wait_marker(events, 'W');
}

static int copy_report(int report)
{
	struct pollfd descriptor = { .fd = report, .events = POLLIN };
	char buffer[4096];
	ssize_t length;

	for (;;) {
		int result = poll(&descriptor, 1, 3000);
		if (result <= 0)
			return 0;
		if ((descriptor.revents & (POLLIN | POLLHUP)) == 0)
			return 0;
		length = read(report, buffer, sizeof(buffer));
		if (length > 0) {
			fwrite(buffer, 1, (size_t)length, stdout);
			continue;
		}
		if (length == 0)
			return 1;
		if (errno != EINTR)
			return 0;
	}
}

int main(int argc, char **argv)
{
	int master, slave, ready[2], events[2], release[2], report[2], status;
	pid_t child;
	const char *name;
	struct winsize size = { .ws_row = 24, .ws_col = 80, 0, 0 };

	if (argc != 2) {
		fprintf(stderr, "usage: %s <workdir>\n", argv[0]);
		return 2;
	}
	(void)argv;
	setlocale(LC_ALL, "");
	setvbuf(stdout, NULL, _IOLBF, 0);
	if (pipe(ready) != 0 || pipe(events) != 0 || pipe(release) != 0 ||
	    pipe(report) != 0)
		return 3;
	master = posix_openpt(O_RDWR | O_NOCTTY);
	if (master < 0 || grantpt(master) != 0 || unlockpt(master) != 0)
		return 4;
	name = ptsname(master);
	if (name == NULL || ioctl(master, TIOCSWINSZ, &size) != 0)
		return 5;

	child = fork();
	if (child < 0)
		return 6;
	if (child == 0) {
		close(master);
		close(ready[0]);
		close(events[0]);
		close(release[1]);
		close(report[0]);
		ready_fd = ready[1];
		event_fd = events[1];
		release_fd = release[0];
		slave = open(name, O_RDWR);
		if (slave < 0 || setsid() < 0 || ioctl(slave, TIOCSCTTY, 0) < 0)
			_exit(74);
		dup2(slave, STDIN_FILENO);
		dup2(slave, STDOUT_FILENO);
		dup2(slave, STDERR_FILENO);
		if (slave > STDERR_FILENO)
			close(slave);
		child_session(report[1]);
	}

	close(ready[1]);
	close(events[1]);
	close(release[0]);
	close(report[1]);
	if (!send_scenario(child, master, ready[0], events[0], SIGWINCH,
	    'W', "off\n", 0) ||
	    !send_scenario(child, master, ready[0], events[0], SIGWINCH,
	    'W', "on\n", 72) ||
	    !send_callback_scenario(child, ready[0], events[0], release[1]) ||
	    !send_scenario(child, master, ready[0], events[0], SIGWINCH,
	    'W', "u", 0) ||
	    !send_scenario(child, master, ready[0], events[0], SIGINT,
	    'I', NULL, 0)) {
		kill(child, SIGKILL);
		waitpid(child, NULL, 0);
		return 7;
	}

	if (!copy_report(report[0])) {
		kill(child, SIGKILL);
		waitpid(child, NULL, 0);
		return 8;
	}
	waitpid(child, &status, 0);
	close(master);
	printf("0009 %-26s exit=%d\n", "child", WIFEXITED(status) ?
	    WEXITSTATUS(status) : -1);
	printf("0010 %-26s 10 operations\n", "done");
	return WIFEXITED(status) && WEXITSTATUS(status) == 0 ? 0 : 1;
}
