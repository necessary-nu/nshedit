/*	$NetBSD: readline.h,v 1.56 2026/01/09 17:49:12 christos Exp $	*/

/*-
 * Copyright (c) 1997 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Jaromir Dolecek.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
#ifndef _READLINE_H_
#define _READLINE_H_

#include <sys/types.h>
#include <stdio.h>

/* list of readline stuff supported by editline library's readline wrapper */

/* typedefs */
// [spec:libedit:def:readline.rl-linebuf-func-t-const-char-int]
typedef int	  rl_linebuf_func_t(const char *, int);
// [spec:libedit:def:readline.rl-voidfunc-t-void]
typedef void	  rl_voidfunc_t(void);
// [spec:libedit:def:readline.rl-vintfunc-t-int]
typedef void	  rl_vintfunc_t(int);
// [spec:libedit:def:readline.rl-vcpfunc-t-char]
typedef void	  rl_vcpfunc_t(char *);
// [spec:libedit:def:readline.rl-completion-func-t-const-char-int-int]
typedef char	**rl_completion_func_t(const char *, int, int);
// [spec:libedit:def:readline.rl-compentry-func-t-const-char-int]
typedef char     *rl_compentry_func_t(const char *, int);
// [spec:libedit:def:readline.rl-compdisp-func-t-char-int-int]
typedef void	  rl_compdisp_func_t(char **, int, int);
// [spec:libedit:def:readline.rl-command-func-t-int-int]
typedef int	  rl_command_func_t(int, int);
// [spec:libedit:def:readline.rl-hook-func-t-void]
typedef int	  rl_hook_func_t(void);
// [spec:libedit:def:readline.rl-icppfunc-t-char]
typedef int       rl_icppfunc_t(char **);

/* only supports length */
// [spec:libedit:def:readline.history-state]
typedef struct {
	int length;
} HISTORY_STATE;

// [spec:libedit:def:readline.histdata-t]
typedef void *histdata_t;

// [spec:libedit:def:readline.hist-entry]
typedef struct _hist_entry {
	const char	*line;
	histdata_t	 data;
} HIST_ENTRY;

// [spec:libedit:def:readline.keymap-entry]
typedef struct _keymap_entry {
	char type;
#define ISFUNC	0
#define ISKMAP	1
#define ISMACR	2
	rl_linebuf_func_t *function;
} KEYMAP_ENTRY;

#define KEYMAP_SIZE	256

// [spec:libedit:def:readline.keymap-entry-array-keymap-size]
typedef KEYMAP_ENTRY KEYMAP_ENTRY_ARRAY[KEYMAP_SIZE];
// [spec:libedit:def:readline.keymap]
typedef KEYMAP_ENTRY *Keymap;

#define control_character_threshold	0x20
#define control_character_bit		0x40

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

#define RUBOUT		0x7f
#define ABORT_CHAR	CTRL('G')
#define RL_READLINE_VERSION	0x0402
#define RL_PROMPT_START_IGNORE	'\1'
#define RL_PROMPT_END_IGNORE	'\2'

#define RL_STATE_NONE		0x000000
#define RL_STATE_DONE		0x000001

#define RL_SETSTATE(x)		(rl_readline_state |= ((unsigned long) x))
#define RL_UNSETSTATE(x)	(rl_readline_state &= ~((unsigned long) x))
#define RL_ISSTATE(x)		(rl_readline_state & ((unsigned long) x))

/* global variables used by readline enabled applications */
#ifdef __cplusplus
extern "C" {
#endif
// [spec:libedit:def:readline.rl-library-version]
// [spec:libedit:sem:readline.rl-library-version]
extern const char	*rl_library_version;
// [spec:libedit:def:readline.rl-readline-version]
// [spec:libedit:sem:readline.rl-readline-version]
extern int		rl_readline_version;
// [spec:libedit:def:readline.rl-readline-name]
// [spec:libedit:sem:readline.rl-readline-name]
extern const char	*rl_readline_name;
// [spec:libedit:def:readline.rl-instream]
// [spec:libedit:sem:readline.rl-instream]
extern FILE		*rl_instream;
// [spec:libedit:def:readline.rl-outstream]
// [spec:libedit:sem:readline.rl-outstream]
extern FILE		*rl_outstream;
// [spec:libedit:def:readline.rl-line-buffer]
// [spec:libedit:sem:readline.rl-line-buffer]
extern char		*rl_line_buffer;
// [spec:libedit:def:readline.rl-point]
// [spec:libedit:sem:readline.rl-point]
// [spec:libedit:def:readline.rl-end]
// [spec:libedit:sem:readline.rl-end]
extern int		rl_point, rl_end;
// [spec:libedit:def:readline.rl-basic-quote-characters]
// [spec:libedit:sem:readline.rl-basic-quote-characters]
extern const char	*rl_basic_quote_characters;
// [spec:libedit:def:readline.rl-basic-word-break-characters]
// [spec:libedit:sem:readline.rl-basic-word-break-characters]
extern const char	*rl_basic_word_break_characters;
// [spec:libedit:def:readline.rl-completer-word-break-characters]
// [spec:libedit:sem:readline.rl-completer-word-break-characters]
extern char		*rl_completer_word_break_characters;
// [spec:libedit:def:readline.rl-completer-quote-characters]
// [spec:libedit:sem:readline.rl-completer-quote-characters]
extern const char	*rl_completer_quote_characters;
// [spec:libedit:def:readline.rl-completion-entry-function]
// [spec:libedit:sem:readline.rl-completion-entry-function]
extern rl_compentry_func_t *rl_completion_entry_function;
// [spec:libedit:def:readline.rl-completion-word-break-hook-fn]
// [spec:libedit:sem:readline.rl-completion-word-break-hook-fn]
extern char		*(*rl_completion_word_break_hook)(void);
// [spec:libedit:def:readline.rl-attempted-completion-function]
// [spec:libedit:sem:readline.rl-attempted-completion-function]
extern rl_completion_func_t *rl_attempted_completion_function;
// [spec:libedit:def:readline.rl-attempted-completion-over]
// [spec:libedit:sem:readline.rl-attempted-completion-over]
extern int		 rl_attempted_completion_over;
// [spec:libedit:def:readline.rl-completion-type]
// [spec:libedit:sem:readline.rl-completion-type]
extern int		rl_completion_type;
// [spec:libedit:def:readline.rl-completion-query-items]
// [spec:libedit:sem:readline.rl-completion-query-items]
extern int		rl_completion_query_items;
// [spec:libedit:def:readline.rl-special-prefixes]
// [spec:libedit:sem:readline.rl-special-prefixes]
extern const char	*rl_special_prefixes;
// [spec:libedit:def:readline.rl-completion-append-character]
// [spec:libedit:sem:readline.rl-completion-append-character]
extern int		rl_completion_append_character;
// [spec:libedit:def:readline.rl-inhibit-completion]
// [spec:libedit:sem:readline.rl-inhibit-completion]
extern int		rl_inhibit_completion;
// [spec:libedit:def:readline.rl-pre-input-hook]
// [spec:libedit:sem:readline.rl-pre-input-hook]
extern rl_hook_func_t	*rl_pre_input_hook;
// [spec:libedit:def:readline.rl-startup-hook]
// [spec:libedit:sem:readline.rl-startup-hook]
extern rl_hook_func_t	*rl_startup_hook;
// [spec:libedit:def:readline.rl-terminal-name]
// [spec:libedit:sem:readline.rl-terminal-name]
extern char		*rl_terminal_name;
// [spec:libedit:def:readline.rl-already-prompted]
// [spec:libedit:sem:readline.rl-already-prompted]
extern int		rl_already_prompted;
// [spec:libedit:def:readline.rl-prompt]
// [spec:libedit:sem:readline.rl-prompt]
extern char		*rl_prompt;
// [spec:libedit:def:readline.rl-done]
// [spec:libedit:sem:readline.rl-done]
extern int		rl_done;
// [spec:libedit:def:readline.rl-linefunc]
// [spec:libedit:sem:readline.rl-linefunc]
extern rl_vcpfunc_t	*rl_linefunc;
// [spec:libedit:def:readline.rl-startup1-hook]
// [spec:libedit:sem:readline.rl-startup1-hook]
extern rl_hook_func_t   *rl_startup1_hook;
// [spec:libedit:def:readline.rl-prompt-saved]
// [spec:libedit:sem:readline.rl-prompt-saved]
extern char             *rl_prompt_saved;
// [spec:libedit:def:readline.history-base]
// [spec:libedit:sem:readline.history-base]
// [spec:libedit:def:readline.history-length]
// [spec:libedit:sem:readline.history-length]
extern int		history_base, history_length;
// [spec:libedit:def:readline.history-offset]
// [spec:libedit:sem:readline.history-offset]
extern int		history_offset;
// [spec:libedit:def:readline.history-expansion-char]
// [spec:libedit:sem:readline.history-expansion-char]
extern char		history_expansion_char;
// [spec:libedit:def:readline.history-subst-char]
// [spec:libedit:sem:readline.history-subst-char]
extern char		history_subst_char;
// [spec:libedit:def:readline.history-no-expand-chars]
// [spec:libedit:sem:readline.history-no-expand-chars]
extern char		*history_no_expand_chars;
// [spec:libedit:def:readline.history-inhibit-expansion-function]
// [spec:libedit:sem:readline.history-inhibit-expansion-function]
extern rl_linebuf_func_t *history_inhibit_expansion_function;
// [spec:libedit:def:readline.max-input-history]
// [spec:libedit:sem:readline.max-input-history]
extern int		max_input_history;

/*
 * The following is not implemented
 */
// [spec:libedit:def:readline.rl-readline-state]
// [spec:libedit:sem:readline.rl-readline-state]
extern unsigned long	rl_readline_state;
// [spec:libedit:def:readline.rl-catch-signals]
// [spec:libedit:sem:readline.rl-catch-signals]
extern int		rl_catch_signals;
// [spec:libedit:def:readline.rl-catch-sigwinch]
// [spec:libedit:sem:readline.rl-catch-sigwinch]
extern int		rl_catch_sigwinch;
// [spec:libedit:def:readline.emacs-standard-keymap]
// [spec:libedit:sem:readline.emacs-standard-keymap]
// [spec:libedit:def:readline.emacs-meta-keymap]
// [spec:libedit:sem:readline.emacs-meta-keymap]
// [spec:libedit:def:readline.emacs-ctlx-keymap]
// [spec:libedit:sem:readline.emacs-ctlx-keymap]
extern KEYMAP_ENTRY_ARRAY emacs_standard_keymap,
			emacs_meta_keymap,
			emacs_ctlx_keymap;
// [spec:libedit:def:readline.rl-filename-completion-desired]
// [spec:libedit:sem:readline.rl-filename-completion-desired]
extern int		rl_filename_completion_desired;
// [spec:libedit:def:readline.rl-ignore-completion-duplicates]
// [spec:libedit:sem:readline.rl-ignore-completion-duplicates]
extern int		rl_ignore_completion_duplicates;
// [spec:libedit:def:readline.rl-getc-function-fn]
// [spec:libedit:sem:readline.rl-getc-function-fn]
extern int		(*rl_getc_function)(FILE *);
// [spec:libedit:def:readline.rl-redisplay-function]
// [spec:libedit:sem:readline.rl-redisplay-function]
extern rl_voidfunc_t	*rl_redisplay_function;
// [spec:libedit:def:readline.rl-completion-display-matches-hook]
// [spec:libedit:sem:readline.rl-completion-display-matches-hook]
extern rl_compdisp_func_t *rl_completion_display_matches_hook;
// [spec:libedit:def:readline.rl-prep-term-function]
// [spec:libedit:sem:readline.rl-prep-term-function]
extern rl_vintfunc_t	*rl_prep_term_function;
// [spec:libedit:def:readline.rl-deprep-term-function]
// [spec:libedit:sem:readline.rl-deprep-term-function]
extern rl_voidfunc_t	*rl_deprep_term_function;
// [spec:libedit:def:readline.rl-event-hook]
// [spec:libedit:sem:readline.rl-event-hook]
extern rl_hook_func_t	*rl_event_hook;
// [spec:libedit:def:readline.readline-echoing-p]
// [spec:libedit:sem:readline.readline-echoing-p]
extern int		readline_echoing_p;
// [spec:libedit:def:readline.rl-print-completions-horizontally]
// [spec:libedit:sem:readline.rl-print-completions-horizontally]
extern int		_rl_print_completions_horizontally;
// [spec:libedit:def:readline.rl-complete-mark-directories]
// [spec:libedit:sem:readline.rl-complete-mark-directories]
extern int		_rl_complete_mark_directories;
// [spec:libedit:def:readline.rl-directory-completion-hook]
// [spec:libedit:sem:readline.rl-directory-completion-hook]
extern rl_icppfunc_t	*rl_directory_completion_hook;
// [spec:libedit:def:readline.rl-completion-suppress-append]
// [spec:libedit:sem:readline.rl-completion-suppress-append]
extern int		rl_completion_suppress_append;
// [spec:libedit:def:readline.rl-sort-completion-matches]
// [spec:libedit:sem:readline.rl-sort-completion-matches]
extern int		rl_sort_completion_matches;
// [spec:libedit:def:readline.rl-completion-prefix-display-length]
// [spec:libedit:sem:readline.rl-completion-prefix-display-length]
extern int		_rl_completion_prefix_display_length;
// [spec:libedit:def:readline.rl-echoing-p]
// [spec:libedit:sem:readline.rl-echoing-p]
extern int		_rl_echoing_p;
// [spec:libedit:def:readline.history-max-entries]
// [spec:libedit:sem:readline.history-max-entries]
extern int		history_max_entries;
// [spec:libedit:def:readline.rl-display-prompt]
// [spec:libedit:sem:readline.rl-display-prompt]
extern char		*rl_display_prompt;
// [spec:libedit:def:readline.rl-erase-empty-line]
// [spec:libedit:sem:readline.rl-erase-empty-line]
extern int		rl_erase_empty_line;

/* supported functions */
char		*readline(const char *);
int		 rl_initialize(void);

void		 using_history(void);
int		 add_history(const char *);
void		 clear_history(void);
int		 append_history(int, const char *);
void		 stifle_history(int);
int		 unstifle_history(void);
int		 history_is_stifled(void);
int		 where_history(void);
HIST_ENTRY	*current_history(void);
HIST_ENTRY	*history_get(int);
HIST_ENTRY	*remove_history(int);
HIST_ENTRY	*replace_history_entry(int, const char *, histdata_t);
int		 history_total_bytes(void);
int		 history_set_pos(int);
HIST_ENTRY	*previous_history(void);
HIST_ENTRY	*next_history(void);
HIST_ENTRY     **history_list(void);
int		 history_search(const char *, int);
int		 history_search_prefix(const char *, int);
int		 history_search_pos(const char *, int, int);
int		 read_history(const char *);
int		 write_history(const char *);
int		 history_truncate_file(const char *, int);
int		 history_expand(char *, char **);
char	       **history_tokenize(const char *);
const char	*get_history_event(const char *, int *, int);
char		*history_arg_extract(int, int, const char *);

char		*tilde_expand(char *);
char		*filename_completion_function(const char *, int);
char		*username_completion_function(const char *, int);
int		 rl_complete(int, int);
int		 rl_read_key(void);
// [spec:libedit:def:readline.completion-matches-fn]
// [spec:libedit:sem:readline.completion-matches-fn]
char	       **completion_matches(/* const */ char *, rl_compentry_func_t *);
void		 rl_display_match_list(char **, int, int);

int		 rl_insert(int, int);
int		 rl_insert_text(const char *);
int		 rl_reset_terminal(const char *);
void		 rl_resize_terminal(void);
int		 rl_bind_key(int, rl_command_func_t *);
int		 rl_newline(int, int);
void		 rl_callback_read_char(void);
void		 rl_callback_handler_install(const char *, rl_vcpfunc_t *);
void		 rl_callback_handler_remove(void);
void		 rl_redisplay(void);
int		 rl_get_previous_history(int, int);
int		 rl_kill_full_line(int, int);
void		 rl_prep_terminal(int);
void		 rl_deprep_terminal(void);
int		 rl_read_init_file(const char *);
int		 rl_parse_and_bind(const char *);
int		 rl_variable_bind(const char *, const char *);
int		 rl_stuff_char(int);
int		 rl_add_defun(const char *, rl_command_func_t *, int);
HISTORY_STATE	*history_get_history_state(void);
void		 rl_get_screen_size(int *, int *);
void		 rl_set_screen_size(int, int);
char		*rl_filename_completion_function(const char *, int);
int		 _rl_abort_internal(void);
int		 _rl_qsort_string_compare(char **, char **);
char	       **rl_completion_matches(const char *, rl_compentry_func_t *);
void		 rl_forced_update_display(void);
int		 rl_set_prompt(const char *);
int		 rl_on_new_line(void);
void		 rl_reset_after_signal(void);
void		 rl_echo_signal_char(int);
int		 rl_crlf(void);
int		 rl_ding(void);
char 		*rl_copy_text(int, int);
void		 rl_replace_line(const char *, int);
int		 rl_delete_text(int, int);
void 		 rl_message(const char *format, ...)
    __attribute__((__format__(__printf__, 1, 2)));
void		 rl_save_prompt(void);
void		 rl_restore_prompt(void);

/*
 * The following are not implemented
 */
int		 rl_kill_text(int, int);
Keymap		 rl_get_keymap(void);
void		 rl_set_keymap(Keymap);
Keymap		 rl_make_bare_keymap(void);
int		 rl_generic_bind(int, const char *, const char *, Keymap);
int		 rl_bind_key_in_map(int, rl_command_func_t *, Keymap);
int		 rl_set_key(const char *, rl_command_func_t *, Keymap);
void		 rl_cleanup_after_signal(void);
void		 rl_free_line_state(void);
int		 rl_set_keyboard_input_timeout(int);
int		 rl_abort(int, int);
int	         rl_set_keymap_name(const char *, Keymap);
histdata_t	 free_history_entry(HIST_ENTRY *);
void		 _rl_erase_entire_line(void);

#ifdef __cplusplus
}
#endif

#endif /* _READLINE_H_ */
