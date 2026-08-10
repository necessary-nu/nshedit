/*
 * C consumer proof for the committed generated headers.
 *
 * This is a real program rather than a link stub: it reads every completed
 * record shape exposed by the headers, calls both compatibility surfaces,
 * and runs against libnshedit. It is compiled with -Werror because a header
 * that consumers cannot include cleanly is not shippable.
 *
 * Headless and deterministic: no terminal, address, path, or clock appears
 * in its output. The three editor streams are /dev/null.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include <editline/readline.h>
#include <histedit.h>

static int failures;

static void ok(const char *what, int cond) {
    printf("%-34s %s\n", what, cond ? "ok" : "FAILED");
    if (!cond) {
        failures++;
    }
}

/*
 * The declarations a consumer writes for themselves. These compile only if
 * the header's typedefs are what libedit's are: `rl_hook_func_t` is a
 * FUNCTION type there, so `rl_hook_func_t *` is a pointer to a function and
 * not a pointer to a pointer, and `el_rfunc_t` is already a pointer.
 */
static int my_hook(void) { return 0; }
static int my_getc(EditLine *el, wchar_t *wc) { (void)el; (void)wc; return 0; }
static char *my_compentry(const char *t, int s) { (void)t; (void)s; return NULL; }

static int wide_line_is(EditLine *el, const wchar_t *expected, size_t cursor) {
    const LineInfoW *line = el_wline(el);
    size_t length = wcslen(expected);
    return line != NULL && line->buffer != NULL &&
           (size_t)(line->lastchar - line->buffer) == length &&
           (size_t)(line->cursor - line->buffer) == cursor &&
           wmemcmp(line->buffer, expected, length) == 0;
}

// [spec:libedit:sem:chared.el-deletestr1-fn+1/test/integration]
// [spec:libedit:sem:histedit.el-deletestr1-fn+1/test/integration]
static void check_delete_range(EditLine *el) {
    ok("el_winsertstr for range delete", el_winsertstr(el, L"abcdefgh") == 0);
    ok("el_deletestr1 removes range", el_deletestr1(el, 1, 3) == 2 &&
                                           wide_line_is(el, L"adefgh", 6));
    ok("el_deletestr1 accepts line end", el_deletestr1(el, 4, 6) == 2 &&
                                             wide_line_is(el, L"adef", 4));
    ok("el_deletestr1 clamps oversized end", el_deletestr1(el, 2, 99) == 2 &&
                                                  wide_line_is(el, L"ad", 2));
    ok("el_deletestr1 rejects negative", el_deletestr1(el, -1, 1) == 0 &&
                                             wide_line_is(el, L"ad", 2));
}

static rl_hook_func_t *hook = my_hook;
static el_rfunc_t getcfn = my_getc;
static rl_compentry_func_t *compentry = my_compentry;

int main(int argc, char **argv) {
    HistoryW *h;
    HistEventW ev;
    TokenizerW *tok;
    const wchar_t **wargv;
    int wargc;
    EditLine *el;
    const LineInfoW *li;
    FILE *devnull;
    void *clientdata;
    HIST_ENTRY *he;
    HISTORY_STATE *hs;
    KEYMAP_ENTRY *km;

    (void)argc;
    (void)argv;

    printf("LIBEDIT %d.%d\n", LIBEDIT_MAJOR, LIBEDIT_MINOR);

    /* The opcodes are #defines, not enumerators. Consumers write this. */
#ifdef EL_PROMPT
    printf("EL_PROMPT is a macro, value %d\n", EL_PROMPT);
#else
#error "EL_PROMPT is not a macro: #ifdef EL_PROMPT is what consumers write"
#endif
#ifdef H_SETSIZE
    printf("H_SETSIZE is a macro, value %d\n", H_SETSIZE);
#else
#error "H_SETSIZE is not a macro"
#endif
#ifdef RL_PROMPT_START_IGNORE
    printf("RL_PROMPT_START_IGNORE is a macro, value %d\n", RL_PROMPT_START_IGNORE);
#else
#error "RL_PROMPT_START_IGNORE is not a macro"
#endif

    ok("hook typedefs are usable", hook == my_hook && getcfn == my_getc &&
                                       compentry == my_compentry);

    /* ---- histedit.h: history, and HistEventW's layout ---- */
    h = history_winit();
    ok("history_winit", h != NULL);
    ok("H_SETSIZE", history_w(h, &ev, H_SETSIZE, 10) != -1);
    ok("H_ENTER", history_w(h, &ev, H_ENTER, L"one") != -1);
    ok("H_ENTER", history_w(h, &ev, H_ENTER, L"two") != -1);
    ok("H_FIRST reads ev.str", history_w(h, &ev, H_FIRST) != -1 &&
                                   ev.str != NULL && wcscmp(ev.str, L"two") == 0);
    ok("H_FIRST reads ev.num", ev.num > 0);
    history_wend(h);

    /* ---- histedit.h: tokenizer, and the argv out-parameter ---- */
    tok = tok_winit(NULL);
    ok("tok_winit", tok != NULL);
    wargc = -1;
    wargv = NULL;
    ok("tok_wstr", tok_wstr(tok, L"a bb ccc", &wargc, &wargv) == 0);
    ok("tok_wstr argc", wargc == 3);
    ok("tok_wstr argv", wargv != NULL && wcscmp(wargv[2], L"ccc") == 0);
    tok_wend(tok);

    /* ---- histedit.h: the editor, and LineInfoW's layout ---- */
    devnull = fopen("/dev/null", "r+");
    ok("fopen /dev/null", devnull != NULL);
    el = el_init("header_consumer", devnull, devnull, devnull);
    ok("el_init", el != NULL);
    /* Exercise a variadic operation and its pointer-valued result through
     * the exact declarations shipped to C consumers. */
    ok("el_wset(EL_CLIENTDATA)", el_wset(el, EL_CLIENTDATA, (void *)el) == 0);
    clientdata = NULL;
    ok("el_wget(EL_CLIENTDATA)", el_wget(el, EL_CLIENTDATA, &clientdata) == 0 &&
                                     clientdata == (void *)el);
    li = el_wline(el);
    ok("el_wline", li != NULL);
    ok("LineInfoW fields are ordered", li != NULL && li->buffer != NULL &&
                                           li->cursor >= li->buffer &&
                                           li->lastchar >= li->buffer);
    check_delete_range(el);
    el_end(el);
    if (devnull != NULL) {
        fclose(devnull);
    }

    /* ---- editline/readline.h: HIST_ENTRY, HISTORY_STATE, KEYMAP_ENTRY ---- */
    using_history();
    ok("add_history", add_history("alpha") == 0);
    ok("add_history", add_history("beta") == 0);
    ok("history_length", history_length == 2);
    he = history_get(history_base);
    ok("history_get reads ->line", he != NULL && he->line != NULL &&
                                       strcmp(he->line, "alpha") == 0);
    ok("history_get reads ->data", he != NULL && he->data == NULL);
    hs = history_get_history_state();
    ok("HISTORY_STATE reads ->length", hs != NULL && hs->length == 2);
    free(hs);

    km = &emacs_standard_keymap[3];
    ok("KEYMAP_ENTRY reads ->type", km->type == ISFUNC);
    ok("KEYMAP_ENTRY reads ->function", km->function == NULL);
    ok("Keymap is a KEYMAP_ENTRY *", (Keymap)km == km);

    ok("rl_library_version", rl_library_version != NULL);
    ok("CTRL/UNCTRL/RUBOUT", CTRL('G') == ABORT_CHAR && RUBOUT == 0x7f &&
                                 UNCTRL('a') == ('A' | control_character_bit));
    RL_SETSTATE(RL_STATE_DONE);
    ok("RL_SETSTATE/RL_ISSTATE", RL_ISSTATE(RL_STATE_DONE) != 0);
    RL_UNSETSTATE(RL_STATE_DONE);
    ok("RL_UNSETSTATE", RL_ISSTATE(RL_STATE_DONE) == 0);
    clear_history();

    printf("%d check(s) failed\n", failures);
    return failures != 0;
}
