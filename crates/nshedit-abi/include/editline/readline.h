#ifndef _READLINE_H_
#define _READLINE_H_

#include <sys/types.h>
#include <stdio.h>

/* list of readline stuff supported by editline library's readline wrapper */

/* typedefs */
typedef int	  rl_linebuf_func_t(const char *, int);
typedef void	  rl_voidfunc_t(void);
typedef void	  rl_vintfunc_t(int);
typedef void	  rl_vcpfunc_t(char *);
typedef char	**rl_completion_func_t(const char *, int, int);
typedef char     *rl_compentry_func_t(const char *, int);
typedef void	  rl_compdisp_func_t(char **, int, int);
typedef int	  rl_command_func_t(int, int);
typedef int	  rl_hook_func_t(void);
typedef int       rl_icppfunc_t(char **);

#ifndef CTRL
#include <sys/ioctl.h>
#if !defined(__sun) && !defined(__hpux) && !defined(_AIX)
#include <sys/ttydefaults.h>
#endif
#ifndef CTRL
#define CTRL(c)		((c) & 037)
#endif
#endif
#ifndef UNCTRL
#define UNCTRL(c)	(((c) - 'a' + 'A')|control_character_bit)
#endif

#define ABORT_CHAR	CTRL('G')

#define RL_SETSTATE(x)		(rl_readline_state |= ((unsigned long) x))
#define RL_UNSETSTATE(x)	(rl_readline_state &= ~((unsigned long) x))
#define RL_ISSTATE(x)		(rl_readline_state & ((unsigned long) x))


// C: `#define KEYMAP_SIZE 256`.
#define KEYMAP_SIZE 256

// C: `#define ISFUNC 0` — a `KEYMAP_ENTRY` holding a function.
#define ISFUNC 0

// C: `#define ISKMAP 1` — a `KEYMAP_ENTRY` holding a nested keymap.
#define ISKMAP 1

// C: `#define ISMACR 2` — a `KEYMAP_ENTRY` holding a macro.
#define ISMACR 2

// C: `#define control_character_threshold 0x20`.
#define control_character_threshold 32

// C: `#define control_character_bit 0x40`.
#define control_character_bit 64

// C: `#define RUBOUT 0x7f`.
#define RUBOUT 127

// C: `#define RL_READLINE_VERSION 0x0402`.
#define RL_READLINE_VERSION 1026

// C: `#define RL_PROMPT_START_IGNORE '\1'`.
#define RL_PROMPT_START_IGNORE 1

// C: `#define RL_PROMPT_END_IGNORE '\2'`.
#define RL_PROMPT_END_IGNORE 2

// C: `#define RL_STATE_NONE 0x000000`.
#define RL_STATE_NONE 0

// C: `#define RL_STATE_DONE 0x000001`.
#define RL_STATE_DONE 1

// C: `typedef void *histdata_t;` — opaque per-entry application data.
typedef void *histdata_t;

// C: `typedef struct _hist_entry { const char *line; histdata_t data; } HIST_ENTRY;`.
struct _hist_entry {
  const char *line;
  histdata_t data;
};

// C: `typedef struct { int length; } HISTORY_STATE;`.
struct _history_state {
  int length;
};

// C: `typedef struct _keymap_entry { char type; rl_linebuf_func_t *function; } KEYMAP_ENTRY;`.
struct _keymap_entry {
  // `ISFUNC`, `ISKMAP`, or `ISMACR`.
  char type;
  // Nullable `rl_linebuf_func_t *`, expanded so cbindgen renders a C
  int (*function)(const char*, int);
};

// C: `typedef KEYMAP_ENTRY *Keymap;` — a borrowed mutable keymap view.
typedef struct _keymap_entry *Keymap;

// C: `typedef struct _hist_entry { ... } HIST_ENTRY;`.
typedef struct _hist_entry HIST_ENTRY;

// C: `typedef struct { int length; } HISTORY_STATE;`.
typedef struct _history_state HISTORY_STATE;

// C: `typedef struct _keymap_entry { ... } KEYMAP_ENTRY;`.
typedef struct _keymap_entry KEYMAP_ENTRY;

// C: `typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];`.
typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// C: `const char *rl_library_version = "EditLine wrapper";`
extern const char *rl_library_version;

// C: `int rl_readline_version = RL_READLINE_VERSION;`
extern int rl_readline_version;

// C: `const char *rl_readline_name = empty;` — the program name
extern const char *rl_readline_name;

// C: `FILE *rl_instream = NULL;` — NULL means the process's standard
extern FILE* rl_instream;

// C: `FILE *rl_outstream = NULL;` — NULL means the process's standard
extern FILE* rl_outstream;

// C: `int rl_point = 0;` — the cursor position in *bytes*, republished by
extern int rl_point;

// C: `int rl_end = 0;` — the line length in bytes.
extern int rl_end;

// C: `char *rl_line_buffer = NULL;` — borrowed; it points into EditLine's
extern char *rl_line_buffer;

// C: `rl_vcpfunc_t *rl_linefunc = NULL;` — the callback-mode line handler.
extern void (*rl_linefunc)(char*);

// C: `int rl_done = 0;`
extern int rl_done;

// C: `rl_hook_func_t *rl_event_hook = NULL;`
extern int (*rl_event_hook)(void);

// C: `KEYMAP_ENTRY_ARRAY emacs_standard_keymap;` — zero-initialized and
extern struct _keymap_entry emacs_standard_keymap[KEYMAP_SIZE];

// C: `KEYMAP_ENTRY_ARRAY emacs_meta_keymap;` — likewise inert.
extern struct _keymap_entry emacs_meta_keymap[KEYMAP_SIZE];

// C: `KEYMAP_ENTRY_ARRAY emacs_ctlx_keymap;` — likewise inert.
extern struct _keymap_entry emacs_ctlx_keymap[KEYMAP_SIZE];

// C: `int rl_catch_signals = 1;` — read once, by `rl_initialize`, as the
extern int rl_catch_signals;

// C: `int rl_catch_sigwinch = 1;` — exported but never consulted.
extern int rl_catch_sigwinch;

// C: `int history_base = 1;`
extern int history_base;

// C: `int history_length = 0;`
extern int history_length;

// C: `int history_offset = 0;`
extern int history_offset;

// C: `int max_input_history = 0;` — the mirror `history_is_stifled` reads;
extern int max_input_history;

// C: `char history_expansion_char = '!';`
extern char history_expansion_char;

// C: `char history_subst_char = '^';`
extern char history_subst_char;

// C: `char *history_no_expand_chars = expand_chars;`
extern char *history_no_expand_chars;

// C: `rl_linebuf_func_t *history_inhibit_expansion_function = NULL;`
extern int (*history_inhibit_expansion_function)(const char*, int);

// C: `int rl_inhibit_completion = 0;`
extern int rl_inhibit_completion;

// C: `int rl_attempted_completion_over = 0;`
extern int rl_attempted_completion_over;

// C: `const char *rl_basic_word_break_characters = break_chars;` — the only
extern const char *rl_basic_word_break_characters;

// C: `char *rl_completer_word_break_characters = NULL;` — declared, and
extern char *rl_completer_word_break_characters;

// C: `const char *rl_completer_quote_characters = NULL;` — likewise unread.
extern const char *rl_completer_quote_characters;

// C: `const char *rl_basic_quote_characters = "\"'";` — likewise unread.
extern const char *rl_basic_quote_characters;

// C: `rl_compentry_func_t *rl_completion_entry_function = NULL;`
extern char *(*rl_completion_entry_function)(const char*, int);

// C: `extern char *(*rl_completion_word_break_hook)(void);`
extern char *(*rl_completion_word_break_hook)(void);

// C: `rl_completion_func_t *rl_attempted_completion_function = NULL;`
extern char **(*rl_attempted_completion_function)(const char*, int, int);

// C: `rl_hook_func_t *rl_pre_input_hook = NULL;`
extern int (*rl_pre_input_hook)(void);

// C: `rl_hook_func_t *rl_startup1_hook = NULL;` — exported, never called.
extern int (*rl_startup1_hook)(void);

// C: `extern int (*rl_getc_function)(FILE *);`
extern int (*rl_getc_function)(FILE*);

// C: `char *rl_terminal_name = NULL;` — written by `rl_initialize` with a
extern char *rl_terminal_name;

// C: `int rl_already_prompted = 0;` — set by `_get_prompt`, cleared by
extern int rl_already_prompted;

// C: `int rl_filename_completion_desired = 0;` — exported, never consulted.
extern int rl_filename_completion_desired;

// C: `int rl_ignore_completion_duplicates = 0;` — exported, never consulted.
extern int rl_ignore_completion_duplicates;

// C: `int readline_echoing_p = 1;` — exported, never consulted.
extern int readline_echoing_p;

// C: `int _rl_print_completions_horizontally = 0;` — exported, never
extern int _rl_print_completions_horizontally;

// C: `rl_voidfunc_t *rl_redisplay_function = NULL;` — readline's
extern void (*rl_redisplay_function)(void);

// C: `rl_hook_func_t *rl_startup_hook = NULL;` — called by `readline()`
extern int (*rl_startup_hook)(void);

// C: `rl_compdisp_func_t *rl_completion_display_matches_hook = NULL;` —
extern void (*rl_completion_display_matches_hook)(char**, int, int);

// C: `rl_vintfunc_t *rl_prep_term_function = (rl_vintfunc_t *)
extern void (*rl_prep_term_function)(int);

// C: `rl_voidfunc_t *rl_deprep_term_function = (rl_voidfunc_t *)
extern void (*rl_deprep_term_function)(void);

// C: `unsigned long rl_readline_state = RL_STATE_NONE;` — only
extern unsigned long rl_readline_state;

// C: `int _rl_complete_mark_directories;` — exported, never consulted.
extern int _rl_complete_mark_directories;

// C: `rl_icppfunc_t *rl_directory_completion_hook;` — exported, never
extern int (*rl_directory_completion_hook)(char**);

// C: `int rl_completion_suppress_append;` — exported, never consulted.
extern int rl_completion_suppress_append;

// C: `int rl_sort_completion_matches;` — exported, never consulted.
extern int rl_sort_completion_matches;

// C: `int _rl_completion_prefix_display_length;` — exported, never
extern int _rl_completion_prefix_display_length;

// C: `int _rl_echoing_p;` — exported, never consulted.
extern int _rl_echoing_p;

// C: `int history_max_entries;` — exported, never consulted.
extern int history_max_entries;

// C: `char *rl_display_prompt;` — exported, never consulted.
extern char *rl_display_prompt;

// C: `int rl_erase_empty_line;` — exported, never consulted.
extern int rl_erase_empty_line;

// C: `char *rl_prompt = NULL;` — the current prompt, owned by this module.
extern char *rl_prompt;

// C: `char *rl_prompt_saved = NULL;` — `rl_save_prompt`'s copy.
extern char *rl_prompt_saved;

// C: `int rl_completion_type = 0;` — written by `fn_complete2`.
extern int rl_completion_type;

// C: `int rl_completion_query_items = 100;` — the "ask before listing this
extern int rl_completion_query_items;

// C: `const char *rl_special_prefixes = NULL;` — declared, and read by no
extern const char *rl_special_prefixes;

// C: `int rl_completion_append_character = ' ';`
extern int rl_completion_append_character;

int rl_set_prompt(const char *prompt);

void rl_save_prompt(void);

void rl_restore_prompt(void);

int rl_initialize(void);

char *readline(const char *p);

void using_history(void);

const char *get_history_event(const char *cmd, int *cindex, int qchar);

int history_expand(char *str_, char **output);

char *history_arg_extract(int start, int end, const char *str_);

char **history_tokenize(const char *str_);

void stifle_history(int max);

int unstifle_history(void);

int history_is_stifled(void);

struct _hist_entry *history_get(int num);

int add_history(const char *line);

struct _hist_entry *remove_history(int num);

struct _hist_entry *replace_history_entry(int num, const char *line, histdata_t data);

void clear_history(void);

int where_history(void);

struct _hist_entry **history_list(void);

struct _hist_entry *current_history(void);

int history_total_bytes(void);

int history_set_pos(int pos);

struct _hist_entry *previous_history(void);

struct _hist_entry *next_history(void);

int history_search(const char *str_, int direction);

int history_search_prefix(const char *str_, int direction);

int history_search_pos(const char *str_, int direction, int pos);

char *tilde_expand(char *name);

char *filename_completion_function(const char *name, int state);

char *username_completion_function(const char *text, int state);

void rl_display_match_list(char **matches, int len, int max);

int rl_complete(int ignore, int invoking_key);

int rl_bind_key(int c, int (*func)(int, int));

int rl_read_key(void);

int rl_reset_terminal(const char *p);

int rl_insert(int count, int c);

int rl_insert_text(const char *text);

int rl_newline(int count, int c);

int rl_add_defun(const char *name, int (*fun)(int, int), int c);

void rl_callback_read_char(void);

void rl_callback_handler_install(const char *prompt, void (*linefunc)(char*));

void rl_callback_handler_remove(void);

void rl_redisplay(void);

int rl_get_previous_history(int count, int key);

void rl_prep_terminal(int meta_flag);

void rl_deprep_terminal(void);

int rl_read_init_file(const char *s);

int rl_parse_and_bind(const char *line);

int rl_variable_bind(const char *var, const char *value);

int rl_stuff_char(int c);

char *rl_copy_text(int from, int to);

void rl_replace_line(const char *text, int clear_undo);

int rl_delete_text(int start, int end);

void rl_get_screen_size(int *rows, int *cols);

// C: `void rl_message(const char *format, ...);`
void rl_message(const char *format, ...);

void rl_set_screen_size(int rows, int cols);

char **rl_completion_matches(const char *str_, char *(*fun)(const char*, int));

char *rl_filename_completion_function(const char *text, int state);

void rl_forced_update_display(void);

int _rl_abort_internal(void);

int _rl_qsort_string_compare(char **s1, char **s2);

struct _history_state *history_get_history_state(void);

int rl_kill_full_line(int count, int key);

int rl_kill_text(int from, int to);

Keymap rl_make_bare_keymap(void);

Keymap rl_get_keymap(void);

void rl_set_keymap(Keymap k);

int rl_generic_bind(int type_, const char *keyseq, const char *data, Keymap k);

int rl_bind_key_in_map(int key, int (*fun)(int, int), Keymap k);

int rl_set_key(const char *keyseq, int (*function)(int, int), Keymap k);

void rl_cleanup_after_signal(void);

int rl_on_new_line(void);

void rl_free_line_state(void);

int rl_set_keyboard_input_timeout(int u);

void rl_resize_terminal(void);

void rl_reset_after_signal(void);

void rl_echo_signal_char(int sig);

int rl_crlf(void);

int rl_ding(void);

int rl_abort(int count, int key);

int rl_set_keymap_name(const char *name, Keymap k);

histdata_t free_history_entry(struct _hist_entry *he);

void _rl_erase_entire_line(void);

// C: `char **completion_matches(/* const */ char *, rl_compentry_func_t *);`
char **completion_matches(char *text, char *(*genfunc)(const char*, int));

int history_truncate_file(const char *filename, int nlines);

int read_history(const char *filename);

int write_history(const char *filename);

int append_history(int n, const char *filename);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* _READLINE_H_ */
