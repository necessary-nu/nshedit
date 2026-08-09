/*
 * Execute every advertised built-in after installing it through EL_BIND.
 *
 * Each command receives the same owned editor, history, invoking unit, and
 * bounded continuation input in an otherwise fresh handle. The trace observes
 * the returned line plus the post-command line and cursor, so resolving a name
 * to a callback miss or unconditional error cannot masquerade as coverage.
 *
 * [spec:nshedit:req:abi.binding-dispatch/test]
 */
#define _XOPEN_SOURCE 700
#define _DEFAULT_SOURCE

#include <fcntl.h>
#include <locale.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include <histedit.h>

static const char *const commands[] = {
	"ed-end-of-file",
	"ed-insert",
	"ed-delete-prev-word",
	"ed-delete-next-char",
	"ed-kill-line",
	"ed-move-to-end",
	"ed-move-to-beg",
	"ed-transpose-chars",
	"ed-next-char",
	"ed-prev-word",
	"ed-prev-char",
	"ed-quoted-insert",
	"ed-digit",
	"ed-argument-digit",
	"ed-unassigned",
	"ed-ignore",
	"ed-newline",
	"ed-delete-prev-char",
	"ed-clear-screen",
	"ed-redisplay",
	"ed-start-over",
	"ed-sequence-lead-in",
	"ed-prev-history",
	"ed-next-history",
	"ed-search-prev-history",
	"ed-search-next-history",
	"ed-prev-line",
	"ed-next-line",
	"ed-command",
	"em-delete-or-list",
	"em-delete-next-word",
	"em-yank",
	"em-kill-line",
	"em-kill-region",
	"em-copy-region",
	"em-gosmacs-transpose",
	"em-next-word",
	"em-upper-case",
	"em-capitol-case",
	"em-lower-case",
	"em-set-mark",
	"em-exchange-mark",
	"em-universal-argument",
	"em-meta-next",
	"em-toggle-overwrite",
	"em-copy-prev-word",
	"em-inc-search-next",
	"em-inc-search-prev",
	"em-delete-prev-char",
	"vi-paste-next",
	"vi-paste-prev",
	"vi-prev-big-word",
	"vi-prev-word",
	"vi-next-big-word",
	"vi-next-word",
	"vi-change-case",
	"vi-change-meta",
	"vi-insert-at-bol",
	"vi-replace-char",
	"vi-replace-mode",
	"vi-substitute-char",
	"vi-substitute-line",
	"vi-change-to-eol",
	"vi-insert",
	"vi-add",
	"vi-add-at-eol",
	"vi-delete-meta",
	"vi-end-big-word",
	"vi-end-word",
	"vi-undo",
	"vi-command-mode",
	"vi-zero",
	"vi-delete-prev-char",
	"vi-list-or-eof",
	"vi-kill-line-prev",
	"vi-search-prev",
	"vi-search-next",
	"vi-repeat-search-next",
	"vi-repeat-search-prev",
	"vi-next-char",
	"vi-prev-char",
	"vi-to-next-char",
	"vi-to-prev-char",
	"vi-repeat-next-char",
	"vi-repeat-prev-char",
	"vi-match",
	"vi-undo-line",
	"vi-to-column",
	"vi-yank-end",
	"vi-yank",
	"vi-comment-out",
	"vi-alias",
	"vi-to-history-line",
	"vi-histedit",
	"vi-history-word",
	"vi-redo",
};

static const char *alias_text(void *argument, const char *name)
{
	(void)argument;
	return name[0] == '_' && name[1] == 'x' ? "alias" : NULL;
}

static char *prompt(EditLine *editor)
{
	(void)editor;
	return (char *)"> ";
}

static struct {
	const unsigned char *bytes;
	size_t length;
	size_t offset;
} scripted_input;

static int read_character(EditLine *editor, wchar_t *character)
{
	(void)editor;
	if (scripted_input.offset == scripted_input.length)
		return 0;
	*character = scripted_input.bytes[scripted_input.offset++];
	return 1;
}

static const char *read_script(EditLine *editor, const char *input, int *count)
{
	scripted_input.bytes = (const unsigned char *)input;
	scripted_input.length = strlen(input);
	scripted_input.offset = 0;
	return el_gets(editor, count);
}

static void discard_terminal(FILE *stream, int descriptor)
{
	unsigned char buffer[4096];

	(void)fflush(stream);
	while (read(descriptor, buffer, sizeof(buffer)) > 0)
		continue;
}

static void escaped(const char *text, size_t length)
{
	size_t index;

	putchar('<');
	for (index = 0; index < length; index++) {
		unsigned character = (unsigned char)text[index];
		if (character >= 0x20 && character < 0x7f &&
		    character != '\\' && character != '<' && character != '>')
			putchar((int)character);
		else
			printf("\\x%02X", character);
	}
	putchar('>');
}

static void escaped_file(FILE *stream)
{
	int character;

	if (fflush(stream) != 0 || fseek(stream, 0, SEEK_SET) != 0) {
		printf("<unreadable>");
		return;
	}
	putchar('<');
	while ((character = fgetc(stream)) != EOF) {
		unsigned byte = (unsigned char)character;
		if (byte >= 0x20 && byte < 0x7f && byte != '\\' &&
		    byte != '<' && byte != '>')
			putchar(character);
		else
			printf("\\x%02X", byte);
	}
	putchar('>');
}

static size_t terminal_bells(FILE *stream, int descriptor)
{
	unsigned char buffer[1 << 15];
	size_t length = 0;
	size_t bells = 0;
	ssize_t amount;

	if (fflush(stream) != 0)
		return 0;
	while (length < sizeof(buffer) &&
	    (amount = read(descriptor, buffer + length,
	    sizeof(buffer) - length)) > 0)
		length += (size_t)amount;
	for (size_t index = 0; index < length; index++)
		bells += buffer[index] == '\a';
	return bells;
}

static int run_command(size_t sequence, const char *name, const char *scenario,
    int vi_mode, int counted, const char *initial_line, size_t initial_cursor,
    int pending_operator)
{
	struct winsize size = { .ws_row = 24, .ws_col = 80, 0, 0 };
	int master = -1;
	int slave = -1;
	const char *slave_name;
	FILE *input = NULL;
	FILE *output = NULL;
	FILE *errors = tmpfile();
	History *history_store = NULL;
	EditLine *editor = NULL;
	HistEvent event;
	const LineInfo *state;
	const char *line;
	const char *command_input = pending_operator ?
	    (counted ? "2d2\030x\n\n\n" : "d\030x\n\n\n") :
	    (counted ? "2\030x\n\n\n" : "\030x\n\n\n");
	int bind_result;
	int count = -1;
	int result = 0;

	master = posix_openpt(O_RDWR | O_NOCTTY);
	if (master < 0 || grantpt(master) < 0 || unlockpt(master) < 0 ||
	    ioctl(master, TIOCSWINSZ, &size) < 0) {
		result = 70;
		goto done;
	}
	slave_name = ptsname(master);
	if (slave_name == NULL ||
	    (slave = open(slave_name, O_RDWR | O_NOCTTY)) < 0) {
		result = 71;
		goto done;
	}
	input = fdopen(dup(slave), "r");
	output = fdopen(dup(slave), "w");
	if (input == NULL || output == NULL || errors == NULL) {
		result = 72;
		goto done;
	}
	close(slave);
	slave = -1;
	if (fcntl(master, F_SETFL, O_NONBLOCK) < 0) {
		result = 73;
		goto done;
	}
	editor = el_init("binding-dispatch", input, output, errors);
	history_store = history_init();
	if (editor == NULL || history_store == NULL) {
		result = 74;
		goto done;
	}
	history(history_store, &event, H_SETSIZE, 8);
	history(history_store, &event, H_ENTER, "history one");
	history(history_store, &event, H_ENTER, "history two");
	el_set(editor, EL_SIGNAL, 0);
	el_set(editor, EL_EDITOR, vi_mode ? "vi" : "emacs");
	el_set(editor, EL_GETCFN, read_character);
	el_set(editor, EL_PROMPT, prompt);
	el_set(editor, EL_HIST, history, history_store);
	el_set(editor, EL_ALIAS_TEXT, alias_text, NULL);
	if (counted && !vi_mode)
		el_set(editor, EL_BIND, "2", "ed-argument-digit", NULL);
	bind_result = vi_mode ? el_set(editor, EL_BIND, "-a", "^X", name, NULL) :
	    el_set(editor, EL_BIND, "^X", name, NULL);
	el_set(editor, EL_UNBUFFERED, 1);
	discard_terminal(output, master);
	if (vi_mode) {
		int ignored;
		(void)read_script(editor, "\033", &ignored);
		discard_terminal(output, master);
	}
	if (initial_line[0] != '\0' && el_insertstr(editor, initial_line) != 0) {
		result = 75;
		goto done;
	}
	state = el_line(editor);
	if (state == NULL || initial_cursor > (size_t)(state->lastchar - state->buffer) ||
	    el_cursor(editor, (int)initial_cursor -
	    (int)(state->cursor - state->buffer)) != (int)initial_cursor) {
		result = 76;
		goto done;
	}
	el_set(editor, EL_REFRESH);
	discard_terminal(output, master);
	line = bind_result == 0 ? read_script(editor, command_input, &count) : NULL;
	state = el_line(editor);

	printf("%04zu %-28s scenario=%-8s bind=%d vi=%d counted=%d returned=%d count=%d line=",
	    sequence, name, scenario, bind_result, vi_mode, counted,
	    line != NULL, count);
	if (line != NULL && count > 0)
		escaped(line, (size_t)count);
	else
		escaped("", 0);
	printf(" state=");
	if (state != NULL && state->lastchar >= state->buffer)
		escaped(state->buffer, (size_t)(state->lastchar - state->buffer));
	else
		escaped("", 0);
	printf(" cursor=%td bells=%zu", state == NULL ? (ptrdiff_t)-1 :
	    state->cursor - state->buffer, terminal_bells(output, master));
	printf(" errors=");
	escaped_file(errors);
	putchar('\n');

done:
	if (editor != NULL)
		el_end(editor);
	if (history_store != NULL)
		history_end(history_store);
	if (input != NULL)
		fclose(input);
	if (slave >= 0)
		close(slave);
	if (master >= 0)
		close(master);
	if (output != NULL)
		fclose(output);
	if (errors != NULL)
		fclose(errors);
	return result;
}

int main(void)
{
	size_t index;
	size_t sequence = 1;
	static const struct {
		const char *name;
		int vi_mode;
		int counted;
		const char *initial_line;
		size_t initial_cursor;
		int pending_operator;
	} scenarios[] = {
		{ "end", 0, 0, "ab cd", 5, 0 },
		{ "end", 0, 1, "ab cd", 5, 0 },
		{ "end", 1, 0, "ab cd", 4, 0 },
		{ "end", 1, 1, "ab cd", 4, 0 },
		{ "empty", 0, 0, "", 0, 0 },
		{ "empty", 0, 1, "", 0, 0 },
		{ "empty", 1, 0, "", 0, 0 },
		{ "empty", 1, 1, "", 0, 0 },
		{ "middle", 0, 0, "ab.! cd", 3, 0 },
		{ "middle", 0, 1, "ab.! cd", 3, 0 },
		{ "middle", 1, 0, "ab.! cd", 3, 0 },
		{ "middle", 1, 1, "ab.! cd", 3, 0 },
		{ "operator", 1, 0, "ab.! cd", 0, 1 },
		{ "operator", 1, 1, "ab.! cd", 0, 1 },
	};

	setlocale(LC_ALL, "");
	setenv("EDITOR", "true", 1);
	for (index = 0; index < sizeof(commands) / sizeof(commands[0]); index++) {
		size_t scenario;

		for (scenario = 0;
		    scenario < sizeof(scenarios) / sizeof(scenarios[0]);
		    scenario++) {
			int result = run_command(sequence++, commands[index],
			    scenarios[scenario].name, scenarios[scenario].vi_mode,
			    scenarios[scenario].counted,
			    scenarios[scenario].initial_line,
			    scenarios[scenario].initial_cursor,
			    scenarios[scenario].pending_operator);
			if (result != 0)
				return result;
		}
	}
	printf("%04zu done                         %zu commands, %zu scenarios\n",
	    sequence, sizeof(commands) / sizeof(commands[0]),
	    sizeof(scenarios) / sizeof(scenarios[0]));
	return 0;
}
