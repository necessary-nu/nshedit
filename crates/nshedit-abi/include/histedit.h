#ifndef _HISTEDIT_H_
#define _HISTEDIT_H_

#include <sys/types.h>
#include <stdio.h>
#include <wchar.h>
#include <wctype.h>

/* Restore the built-in character reader after `EL_GETCFN`. */
#define EL_BUILTIN_GETCFN (NULL)

/* The wide name for `el_deletestr`, which takes no character type. There is
   no second symbol: `libedit.so` exports `el_deletestr` only. */
#define el_wdeletestr el_deletestr


// C: `#define LIBEDIT_MAJOR 2`.
#define LIBEDIT_MAJOR 2

// C: `#define LIBEDIT_MINOR 11`.
#define LIBEDIT_MINOR 11

// C: `#define CC_NORM 0` — command completed, no redraw needed.
#define CC_NORM 0

// C: `#define CC_NEWLINE 1` — the line is complete.
#define CC_NEWLINE 1

// C: `#define CC_EOF 2` — end of input.
#define CC_EOF 2

// C: `#define CC_ARGHACK 3` — preserve the pending argument or vi action.
#define CC_ARGHACK 3

// C: `#define CC_REFRESH 4` — redraw the line.
#define CC_REFRESH 4

// C: `#define CC_CURSOR 5` — move the cursor only.
#define CC_CURSOR 5

// C: `#define CC_ERROR 6` — beep, no redraw.
#define CC_ERROR 6

// C: `#define CC_FATAL 7` — unrecoverable; the editor resets.
#define CC_FATAL 7

// C: `#define CC_REDISPLAY 8` — full redisplay.
#define CC_REDISPLAY 8

// C: `#define CC_REFRESH_BEEP 9` — redraw and beep.
#define CC_REFRESH_BEEP 9

// C: `history()` operation codes.
#define H_FUNC 0

#define H_SETSIZE 1

#define H_GETSIZE 2

#define H_FIRST 3

#define H_LAST 4

#define H_PREV 5

#define H_NEXT 6

#define H_SET 7

#define H_CURR 8

#define H_ADD 9

#define H_ENTER 10

#define H_APPEND 11

#define H_END 12

#define H_NEXT_STR 13

#define H_PREV_STR 14

#define H_NEXT_EVENT 15

#define H_PREV_EVENT 16

#define H_LOAD 17

#define H_SAVE 18

#define H_CLEAR 19

#define H_SETUNIQUE 20

#define H_GETUNIQUE 21

#define H_DEL 22

#define H_NEXT_EVDATA 23

#define H_DELDATA 24

#define H_REPLACE 25

#define H_SAVE_FP 26

#define H_NSAVE_FP 27

// `, prompt_func);` — set/get. The prompt callback.
#define EL_PROMPT 0

// `, const char *);` — set/get. The terminal type.
#define EL_TERMINAL 1

// `, const Char *);` — set/get. `"emacs"` or `"vi"`.
#define EL_EDITOR 2

// `, int);` — set/get. Whether libedit installs signal handlers.
#define EL_SIGNAL 3

// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
#define EL_BIND 4

// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
#define EL_TELLTC 5

// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
#define EL_SETTC 6

// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
#define EL_ECHOTC 7

// `, const Char *, ..., NULL);` — set. A NULL-terminated argument list.
#define EL_SETTY 8

// `, const Char *name, const Char *help, el_func_t);` — set. `help` is a STRING, not the single `const Char` `histedit.h` annotates: ERR-core-api-34.
#define EL_ADDFN 9

// `, hist_fun_t, const void *);` — set. The history callback and its cookie.
#define EL_HIST 10

// `, int);` — set/get. Zero makes `el_gets` bypass the editing loop.
#define EL_EDITMODE 11

// `, prompt_func);` — set/get. The right-hand prompt callback.
#define EL_RPROMPT 12

// `, el_rfunc_t);` — set/get. `EL_BUILTIN_GETCFN` restores the default.
#define EL_GETCFN 13

// `, void *);` — set/get. Application data, stored and handed back.
#define EL_CLIENTDATA 14

// `, int);` — set/get. Return each character as it arrives.
#define EL_UNBUFFERED 15

// `, int);` — set. Put the terminal in or out of editing mode.
#define EL_PREP_TERM 16

// `, char *, ..., NULL);` — get only. `char *` in BOTH APIs, not the `const Char *` `histedit.h` annotates: ERR-core-api-34.
#define EL_GETTC 17

// `, int, FILE **);` — get only. The stream for one of the three fds.
#define EL_GETFP 18

// `, int, FILE *);` — set. The stream for one of the three fds.
#define EL_SETFP 19

// `, void);` — set. Redraw the line.
#define EL_REFRESH 20

// `, prompt_func, Char);` — set/get. Prompt, plus its literal-run marker.
#define EL_PROMPT_ESC 21

// `, prompt_func, Char);` — set/get. As `EL_PROMPT_ESC`, right-hand side.
#define EL_RPROMPT_ESC 22

// `, el_zfunc_t, void *);` — set. The window-size-change callback.
#define EL_RESIZE 23

// `, el_afunc_t, void *);` — set. The line-alias callback.
#define EL_ALIAS_TEXT 24

// `, int);` — set/get. Restart reads interrupted by a signal.
#define EL_SAFEREAD 25

// `, const Char *);` — set/get. The word-constituent character set.
#define EL_WORDCHARS 26

// `, char *(*func)(const char *));` — set/get. The environment accessor.
#define EL_GETENV 27

// C: `struct editline` — the editor, `def:el.editline`.
struct editline;

// C: `struct history` — the narrow history, `historyn.c`.
struct history;

// C: `struct historyW` — the wide history, `history.c`.
struct historyW;

// C: `struct tokenizer` — the narrow tokenizer, `tokenizern.c`.
struct tokenizer;

// C: `struct tokenizerW` — the wide tokenizer, `tokenizer.c`.
struct tokenizerW;

// C: `typedef struct editline EditLine;` — `def:histedit.edit-line`.
typedef struct editline EditLine;

// C: `typedef struct history History;` — `def:histedit.history`.
typedef struct history History;

// C: `struct HistEvent` and `struct histeventW`, differing only in character
struct HistEvent {
  int num;
  const char *str;
};

// C: `typedef struct HistEvent { ... } HistEvent;`.
typedef struct HistEvent HistEvent;

// C: `typedef struct tokenizer Tokenizer;` — `def:histedit.tokenizer`.
typedef struct tokenizer Tokenizer;

// C: `struct lineinfo` and `struct lineinfow`, differing only in character
struct lineinfo {
  const char *buffer;
  const char *cursor;
  const char *lastchar;
};

// C: `typedef struct lineinfo { ... } LineInfo;`.
typedef struct lineinfo LineInfo;

// C: `typedef struct historyW HistoryW;` — `def:histedit.history-w`.
typedef struct historyW HistoryW;

// C: `typedef struct tokenizerW TokenizerW;` — `def:histedit.tokenizer-w`.
typedef struct tokenizerW TokenizerW;

// C: `typedef int (*el_rfunc_t)(EditLine *, wchar_t *);` —
typedef int (*el_rfunc_t)(EditLine*, wchar_t*);

// C: `struct lineinfo` and `struct lineinfow`, differing only in character
struct lineinfow {
  const wchar_t *buffer;
  const wchar_t *cursor;
  const wchar_t *lastchar;
};

// C: `typedef struct lineinfow { ... } LineInfoW;` — `def:histedit.lineinfow`.
typedef struct lineinfow LineInfoW;

// C: `struct HistEvent` and `struct histeventW`, differing only in character
struct histeventW {
  int num;
  const wchar_t *str;
};

// C: `typedef struct histeventW { ... } HistEventW;` — `def:histedit.hist-event-w`.
typedef struct histeventW HistEventW;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

EditLine *el_init(const char *prog, FILE* fin, FILE* fout, FILE* ferr);

EditLine *el_init_fd(const char *prog,
                     FILE* fin,
                     FILE* fout,
                     FILE* ferr,
                     int fdin,
                     int fdout,
                     int fderr);

void el_end(EditLine *el);

void el_reset(EditLine *el);

void el_beep(EditLine *el);

// C: `unsigned char _el_fn_complete(EditLine *, int);` — the built-in
unsigned char _el_fn_complete(EditLine *el, int ch);

// C: `unsigned char _el_fn_sh_complete(EditLine *, int);` — the
unsigned char _el_fn_sh_complete(EditLine *el, int ch);

int el_source(EditLine *el, const char *fname);

void el_resize(EditLine *el);

void el_deletestr(EditLine *el, int count);

int el_deletestr1(EditLine *el, int start, int end);

History *history_init(void);

void history_end(History *h);

// C: `int history(History *, HistEvent *, int, ...);`
int history(History *h, HistEvent *ev, int op, ...);

Tokenizer *tok_init(const char *ifs);

void tok_end(Tokenizer *tok);

void tok_reset(Tokenizer *tok);

int tok_line(Tokenizer *tok,
             const LineInfo *line,
             int *argc,
             const char ***argv,
             int *cursorc,
             int *cursoro);

int tok_str(Tokenizer *tok, const char *line, int *argc, const char ***argv);

const wchar_t *el_wgets(EditLine *el, int *nread);

int el_wgetc(EditLine *el, wchar_t *wc);

void el_wpush(EditLine *el, const wchar_t *str_);

int el_wparse(EditLine *el, int argc, const wchar_t **argv);

// C: `int el_wset(EditLine *, int, ...);`
int el_wset(EditLine *el, int op, ...);

// C: `int el_wget(EditLine *, int, ...);`
int el_wget(EditLine *el, int op, ...);

int el_cursor(EditLine *el, int n);

const LineInfoW *el_wline(EditLine *el);

int el_winsertstr(EditLine *el, const wchar_t *str_);

int el_wreplacestr(EditLine *el, const wchar_t *str_);

HistoryW *history_winit(void);

void history_wend(HistoryW *h);

// C: `int history_w(HistoryW *, HistEventW *, int, ...);`
int history_w(HistoryW *h, HistEventW *ev, int op, ...);

TokenizerW *tok_winit(const wchar_t *ifs);

void tok_wend(TokenizerW *tok);

void tok_wreset(TokenizerW *tok);

int tok_wline(TokenizerW *tok,
              const LineInfoW *line,
              int *argc,
              const wchar_t ***argv,
              int *cursorc,
              int *cursoro);

int tok_wstr(TokenizerW *tok, const wchar_t *line, int *argc, const wchar_t ***argv);

int el_getc(EditLine *el, char *cp);

void el_push(EditLine *el, const char *str_);

const char *el_gets(EditLine *el, int *nread);

int el_parse(EditLine *el, int argc, const char **argv);

// C: `int el_set(EditLine *el, int op, ...);`
int el_set(EditLine *el, int op, ...);

// C: `int el_get(EditLine *el, int op, ...);`
int el_get(EditLine *el, int op, ...);

const LineInfo *el_line(EditLine *el);

int el_insertstr(EditLine *el, const char *str_);

int el_replacestr(EditLine *el, const char *str_);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* _HISTEDIT_H_ */
