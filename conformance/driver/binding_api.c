/*
 * Binding compatibility through observable state, output, and callbacks.
 *
 * [spec:nshedit:req:abi.bindings/test]
 * [spec:nshedit:req:abi.observational-coverage/test]
 * [spec:libedit:sem:map.map-bind-fn/test]
 */
#include <locale.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <histedit.h>

static int sequence;
static int callback_count;
static int callback_character;

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

static void show_line(const char *label, const char *line, int length)
{
	op(label);
	printf("len=%d bytes=", length);
	escaped(line, length < 0 ? 0 : (size_t)length);
	putchar('\n');
}

static char *stream_bytes(FILE *stream, size_t *length)
{
	long end;
	char *bytes;

	fflush(stream);
	if (fseek(stream, 0, SEEK_END) != 0 || (end = ftell(stream)) < 0 ||
	    fseek(stream, 0, SEEK_SET) != 0)
		return NULL;
	bytes = malloc((size_t)end + 1);
	if (bytes == NULL)
		return NULL;
	*length = fread(bytes, 1, (size_t)end, stream);
	bytes[*length] = '\0';
	return bytes;
}

int main(void)
{
	FILE *input = tmpfile();
	FILE *output = tmpfile();
	FILE *errors = tmpfile();
	EditLine *editor;
	const char *line;
	char *bytes;
	size_t length;
	int count;

	setlocale(LC_ALL, "");
	if (input == NULL || output == NULL || errors == NULL)
		return 70;
	fputs("\030\n\032\n\030\n", input);
	rewind(input);
	editor = el_init("binding-api", input, output, errors);
	if (editor == NULL)
		return 71;
	el_set(editor, EL_SIGNAL, 0);

	op("add user command");
	printf("rc=%d\n", el_set(editor, EL_ADDFN, "record-command",
	    "record callback state", record_command));
	op("bind user command");
	printf("rc=%d\n", el_set(editor, EL_BIND, "^X", "record-command", NULL));
	line = el_gets(editor, &count);
	show_line("callback line", line, count);
	op("callback observation");
	printf("count=%d character=%d\n", callback_count, callback_character);

	op("bind string macro");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-s", "^Z", "macro", NULL));
	line = el_gets(editor, &count);
	show_line("macro line", line, count);

	op("remove user binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-r", "^X", NULL));
	line = el_gets(editor, &count);
	show_line("removed line", line, count);
	op("callback stayed removed");
	printf("count=%d\n", callback_count);

	op("query binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "^Z", NULL));
	op("list commands");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-l", NULL));
	op("set terminal binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "up",
	    "ed-move-to-end", NULL));
	op("query terminal binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "up", NULL));
	bytes = stream_bytes(output, &length);
	op("query output observed");
	printf("macro=%d builtin=%d terminal=%d\n",
	    bytes != NULL && strstr(bytes, "macro") != NULL,
	    bytes != NULL && strstr(bytes, "ed-start-over") != NULL,
	    bytes != NULL && strstr(bytes, "ed-move-to-end") != NULL);
	free(bytes);

	op("remove terminal binding");
	printf("rc=%d\n", el_set(editor, EL_BIND, "-k", "-r", "up", NULL));

	el_end(editor);
	fclose(input);
	fclose(output);
	fclose(errors);
	op("done");
	printf("%d operations\n", sequence);
	return 0;
}
