/*
 * Binding compatibility through observable state, output, and callbacks.
 *
 * [spec:libedit:sem:map.map-bind-fn/test]
 */
#define _XOPEN_SOURCE 700
#define _DEFAULT_SOURCE

#include <fcntl.h>
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

#include <histedit.h>

static int sequence;
static int callback_count;
static int callback_character;

static struct {
	const unsigned char *bytes;
	size_t length;
	size_t offset;
} scripted_input;

static void op(const char *label)
{
	printf("%04d %-28s ", ++sequence, label);
}

static void escaped(const char *text, size_t length)
{
	putchar('<');
	for (size_t index = 0; index < length; index++) {
		unsigned character = (unsigned char)text[index];
		if (character >= 0x20 && character < 0x7f &&
		    character != '\\' && character != '<' && character != '>')
			putchar((int)character);
		else
			printf("\\x%02X", character);
	}
	putchar('>');
}

static unsigned char record_command(EditLine *editor, int character)
{
	callback_count++;
	callback_character = character;
	if (el_insertstr(editor, "callback") != 0)
		return CC_ERROR;
	return CC_REFRESH;
}

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

static void show_line(const char *label, const char *line, int length)
{
	op(label);
	printf("len=%d bytes=", length);
	escaped(line, length < 0 ? 0 : (size_t)length);
	putchar('\n');
}

static int line_is(const char *line, int length, const char *expected)
{
	return line != NULL && length == (int)strlen(expected) &&
	    memcmp(line, expected, (size_t)length) == 0;
}

static char *stream_bytes(FILE *stream, size_t *length)
{
	off_t end;
	size_t offset;
	ssize_t amount;
	int descriptor;
	char *bytes;

	if (fflush(stream) != 0 || (descriptor = fileno(stream)) < 0 ||
	    (end = lseek(descriptor, 0, SEEK_END)) < 0 ||
	    lseek(descriptor, 0, SEEK_SET) < 0)
		return NULL;
	bytes = malloc((size_t)end + 1);
	if (bytes == NULL)
		return NULL;
	offset = 0;
	while (offset < (size_t)end) {
		amount = read(descriptor, bytes + offset, (size_t)end - offset);
		if (amount <= 0) {
			free(bytes);
			return NULL;
		}
		offset += (size_t)amount;
	}
	*length = offset;
	bytes[*length] = '\0';
	return bytes;
}

/* [spec:nshedit:req:abi.bindings/test]
 * Direct, parsed-editrc, and sourced-editrc bindings are each followed by an
 * editing read that proves the installed macro or callback actually ran. */
int main(int argc, char **argv)
{
	int master;
	int slave;
	int output_descriptor;
	const char *slave_name;
	FILE *input;
	FILE *output;
	FILE *errors = tmpfile();
	FILE *editrc;
	FILE *query_output;
	FILE *removed_output;
	EditLine *editor;
	const char *parse_macro[] = { "bind", "-s", "^V", "parsed" };
	const char *line;
	char *bytes;
	char editrc_path[4096];
	size_t length = 0;
	int path_length;
	int has_builtin;
	int has_macro;
	int has_terminal;
	int count;

	if (argc != 2)
		return 2;
	setlocale(LC_ALL, "");
	master = posix_openpt(O_RDWR | O_NOCTTY);
	if (master < 0 || grantpt(master) < 0 || unlockpt(master) < 0)
		return 70;
	slave_name = ptsname(master);
	if (slave_name == NULL)
		return 71;
	slave = open(slave_name, O_RDWR | O_NOCTTY);
	if (slave < 0)
		return 72;
	output_descriptor = dup(slave);
	if (output_descriptor < 0)
		return 73;
	input = fdopen(slave, "r");
	output = fdopen(output_descriptor, "w");
	if (input == NULL || output == NULL || errors == NULL)
		return 74;
	editor = el_init("binding-api", input, output, errors);
	if (editor == NULL)
		return 75;
	el_set(editor, EL_SIGNAL, 0);
	el_set(editor, EL_GETCFN, read_character);

	op("add user command");
	printf("rc=%d\n", el_set(editor, EL_ADDFN, "record-command",
	    "record callback state", record_command));
	op("bind user command");
	printf("rc=%d\n", el_set(editor, EL_BIND, "^X", "record-command", NULL));
	line = read_script(editor, "\030\n", &count);
	show_line("callback line", line, count);
	op("callback observation");
	printf("count=%d character=%d\n", callback_count, callback_character);
	if (!line_is(line, count, "callback\n") || callback_count != 1 ||
	    callback_character != 24)
		return 78;

	op("bind string macro");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-s", "^Zx", "macro", NULL));
	line = read_script(editor, "\032x\n", &count);
	show_line("macro line", line, count);
	if (!line_is(line, count, "macro\n"))
		return 79;

	op("remove user binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-r", "^X", NULL));
	line = read_script(editor, "\030\n", &count);
	show_line("removed line", line, count);
	op("callback stayed removed");
	printf("count=%d\n", callback_count);
	if (!line_is(line, count, "\n") || callback_count != 1)
		return 80;

	op("parse macro binding");
	printf("rc=%d\n", el_parse(editor, 4, parse_macro));
	line = read_script(editor, "\026\n", &count);
	show_line("parsed macro line", line, count);
	if (!line_is(line, count, "parsed\n"))
		return 81;

	path_length = snprintf(editrc_path, sizeof(editrc_path),
	    "%s/binding.editrc", argv[1]);
	if (path_length < 0 || path_length >= (int)sizeof(editrc_path))
		return 72;
	editrc = fopen(editrc_path, "w");
	if (editrc == NULL)
		return 73;
	if (fputs("bind -s ^W sourced\n", editrc) == EOF) {
		fclose(editrc);
		return 74;
	}
	if (fclose(editrc) != 0)
		return 74;
	op("source macro binding");
	printf("rc=%d\n", el_source(editor, editrc_path));
	line = read_script(editor, "\027\n", &count);
	show_line("sourced macro line", line, count);
	if (!line_is(line, count, "sourced\n"))
		return 82;

	query_output = tmpfile();
	if (query_output == NULL)
		return 76;
	op("capture query output");
	printf("rc=%d\n", el_set(editor, EL_SETFP, 1, query_output));
	op("query binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "^Zx", NULL));
	op("list commands");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-l", NULL));
	op("set terminal binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "up",
	    "ed-move-to-end", NULL));
	op("query terminal binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "up", NULL));
	bytes = stream_bytes(query_output, &length);
	if (bytes == NULL)
		return 83;
	has_macro = strstr(bytes, "macro") != NULL;
	has_builtin = strstr(bytes, "ed-start-over") != NULL;
	has_terminal = strstr(bytes, "ed-move-to-end") != NULL;
	op("query output observed");
	printf("bytes=%zu macro=%d builtin=%d terminal=%d\n", length,
	    has_macro, has_builtin, has_terminal);
	free(bytes);
	if (!has_macro || !has_builtin || !has_terminal)
		return 84;

	op("remove terminal binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "-r", "up", NULL));
	removed_output = tmpfile();
	if (removed_output == NULL)
		return 77;
	op("capture removed output");
	printf("rc=%d\n", el_set(editor, EL_SETFP, 1, removed_output));
	fclose(query_output);
	op("query removed terminal");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "up", NULL));
	bytes = stream_bytes(removed_output, &length);
	if (bytes == NULL)
		return 85;
	has_terminal = strstr(bytes, "ed-move-to-end") != NULL;
	op("removed output observed");
	printf("still-bound=%d\n", has_terminal);
	free(bytes);
	if (has_terminal)
		return 86;

	el_end(editor);
	fclose(input);
	fclose(output);
	fclose(errors);
	fclose(removed_output);
	close(master);
	op("done");
	printf("%d operations\n", sequence);
	return 0;
}
