/*	$NetBSD: histedit.h,v 1.64 2025/12/16 02:40:48 kre Exp $	*/

/*-
 * Copyright (c) 1992, 1993
 *	The Regents of the University of California.  All rights reserved.
 *
 * This code is derived from software contributed to Berkeley by
 * Christos Zoulas of Cornell University.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 *
 *	@(#)histedit.h	8.2 (Berkeley) 1/3/94
 */

/*
 * histedit.h: Line editor and history interface.
 */
#ifndef _HISTEDIT_H_
#define	_HISTEDIT_H_

#define	LIBEDIT_MAJOR 2
#define	LIBEDIT_MINOR 11

#include <sys/types.h>
#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * ==== Editing ====
 */

// [spec:libedit:def:histedit.edit-line]
typedef struct editline EditLine;

/*
 * For user-defined function interface
 */
// [spec:libedit:def:histedit.lineinfo]
// [spec:libedit:def:histedit.line-info]
typedef struct lineinfo {
	const char	*buffer;
	const char	*cursor;
	const char	*lastchar;
} LineInfo;

/*
 * EditLine editor function return codes.
 * For user-defined function interface
 */
#define	CC_NORM		0
#define	CC_NEWLINE	1
#define	CC_EOF		2
#define	CC_ARGHACK	3
#define	CC_REFRESH	4
#define	CC_CURSOR	5
#define	CC_ERROR	6
#define	CC_FATAL	7
#define	CC_REDISPLAY	8
#define	CC_REFRESH_BEEP	9

/*
 * Initialization, cleanup, and resetting
 */
// [spec:libedit:def:histedit.el-init-fn]
// [spec:libedit:sem:histedit.el-init-fn]
EditLine	*el_init(const char *, FILE *, FILE *, FILE *);
// [spec:libedit:def:histedit.el-init-fd-fn]
// [spec:libedit:sem:histedit.el-init-fd-fn]
EditLine	*el_init_fd(const char *, FILE *, FILE *, FILE *,
    int, int, int);
// [spec:libedit:def:histedit.el-end-fn]
// [spec:libedit:sem:histedit.el-end-fn]
void		 el_end(EditLine *);
// [spec:libedit:def:histedit.el-reset-fn]
// [spec:libedit:sem:histedit.el-reset-fn]
void		 el_reset(EditLine *);

/*
 * Get a line, a character or push a string back in the input queue
 */
// [spec:libedit:def:histedit.el-gets-fn]
// [spec:libedit:sem:histedit.el-gets-fn]
const char	*el_gets(EditLine *, int *);
// [spec:libedit:def:histedit.el-getc-fn]
// [spec:libedit:sem:histedit.el-getc-fn]
int		 el_getc(EditLine *, char *);
// [spec:libedit:def:histedit.el-push-fn]
// [spec:libedit:sem:histedit.el-push-fn]
void		 el_push(EditLine *, const char *);

/*
 * Beep!
 */
// [spec:libedit:def:histedit.el-beep-fn]
// [spec:libedit:sem:histedit.el-beep-fn]
void		 el_beep(EditLine *);

/*
 * High level function internals control
 * Parses argc, argv array and executes builtin editline commands
 */
// [spec:libedit:def:histedit.el-parse-fn]
// [spec:libedit:sem:histedit.el-parse-fn]
int		 el_parse(EditLine *, int, const char **);

/*
 * Low level editline access functions
 */
// [spec:libedit:def:histedit.el-set-fn]
// [spec:libedit:sem:histedit.el-set-fn]
int		 el_set(EditLine *, int, ...);
// [spec:libedit:def:histedit.el-get-fn]
// [spec:libedit:sem:histedit.el-get-fn]
int		 el_get(EditLine *, int, ...);
// [spec:libedit:def:histedit.el-fn-complete-fn]
// [spec:libedit:sem:histedit.el-fn-complete-fn]
unsigned char	_el_fn_complete(EditLine *, int);
// [spec:libedit:def:histedit.el-fn-sh-complete-fn]
// [spec:libedit:sem:histedit.el-fn-sh-complete-fn]
unsigned char	_el_fn_sh_complete(EditLine *, int);

/*
 * el_set/el_get parameters
 *
 * When using el_wset/el_wget (as opposed to el_set/el_get):
 *   Char is wchar_t, otherwise it is char.
 *   prompt_func is el_wpfunc_t, otherwise it is el_pfunc_t .

 * Prompt function prototypes are:
 *   typedef char    *(*el_pfunct_t)  (EditLine *);
 *   typedef wchar_t *(*el_wpfunct_t) (EditLine *);
 *
 * For operations that support set or set/get, the argument types listed are for
 * the "set" operation. For "get", each listed type must be a pointer.
 * E.g. EL_EDITMODE takes an int when set, but an int* when get.
 *
 * Operations that only support "get" have the correct argument types listed.
 */
#define	EL_PROMPT	0	/* , prompt_func);		      set/get */
#define	EL_TERMINAL	1	/* , const char *);		      set/get */
#define	EL_EDITOR	2	/* , const Char *);		      set/get */
#define	EL_SIGNAL	3	/* , int);			      set/get */
#define	EL_BIND		4	/* , const Char *, ..., NULL);	      set     */
#define	EL_TELLTC	5	/* , const Char *, ..., NULL);	      set     */
#define	EL_SETTC	6	/* , const Char *, ..., NULL);	      set     */
#define	EL_ECHOTC	7	/* , const Char *, ..., NULL);        set     */
#define	EL_SETTY	8	/* , const Char *, ..., NULL);        set     */
#define	EL_ADDFN	9	/* , const Char *, const Char,        set     */
				/*   el_func_t);			      */
#define	EL_HIST		10	/* , hist_fun_t, const void *);	      set     */
#define	EL_EDITMODE	11	/* , int);			      set/get */
#define	EL_RPROMPT	12	/* , prompt_func);		      set/get */
#define	EL_GETCFN	13	/* , el_rfunc_t);		      set/get */
#define	EL_CLIENTDATA	14	/* , void *);			      set/get */
#define	EL_UNBUFFERED	15	/* , int);			      set/get */
#define	EL_PREP_TERM	16	/* , int);			      set     */
#define	EL_GETTC	17	/* , const Char *, ..., NULL);		  get */
#define	EL_GETFP	18	/* , int, FILE **);		          get */
#define	EL_SETFP	19	/* , int, FILE *);		      set     */
#define	EL_REFRESH	20	/* , void);			      set     */
#define	EL_PROMPT_ESC	21	/* , prompt_func, Char);	      set/get */
#define	EL_RPROMPT_ESC	22	/* , prompt_func, Char);	      set/get */
#define	EL_RESIZE	23	/* , el_zfunc_t, void *);	      set     */
#define	EL_ALIAS_TEXT	24	/* , el_afunc_t, void *);	      set     */
#define	EL_SAFEREAD	25	/* , int);			      set/get */
#define	EL_WORDCHARS	26	/* , const Char *);		      set/get */
#define	EL_GETENV	27	/* , char *(*func)(const char *);     set/get */

#define	EL_BUILTIN_GETCFN	(NULL)

/*
 * Source named file or $PWD/.editrc or $HOME/.editrc
 */
// [spec:libedit:def:histedit.el-source-fn]
// [spec:libedit:sem:histedit.el-source-fn]
int		el_source(EditLine *, const char *);

/*
 * Must be called when the terminal changes size; If EL_SIGNAL
 * is set this is done automatically otherwise it is the responsibility
 * of the application
 */
// [spec:libedit:def:histedit.el-resize-fn]
// [spec:libedit:sem:histedit.el-resize-fn]
void		 el_resize(EditLine *);

/*
 * User-defined function interface.
 */
// [spec:libedit:def:histedit.el-line-fn]
// [spec:libedit:sem:histedit.el-line-fn]
const LineInfo	*el_line(EditLine *);
// [spec:libedit:def:histedit.el-insertstr-fn]
// [spec:libedit:sem:histedit.el-insertstr-fn]
int		 el_insertstr(EditLine *, const char *);
// [spec:libedit:def:histedit.el-deletestr-fn]
// [spec:libedit:sem:histedit.el-deletestr-fn]
void		 el_deletestr(EditLine *, int);
// [spec:libedit:def:histedit.el-replacestr-fn]
// [spec:libedit:sem:histedit.el-replacestr-fn]
int		 el_replacestr(EditLine *, const char *);
// [spec:libedit:def:histedit.el-deletestr1-fn]
// [spec:libedit:sem:histedit.el-deletestr1-fn]
int		 el_deletestr1(EditLine *, int, int);

/*
 * ==== History ====
 */

// [spec:libedit:def:histedit.history]
typedef struct history History;

// [spec:libedit:def:histedit.hist-event]
typedef struct HistEvent {
	int		 num;
	const char	*str;
} HistEvent;

/*
 * History access functions.
 */
// [spec:libedit:def:histedit.history-init-fn]
// [spec:libedit:sem:histedit.history-init-fn]
History *	history_init(void);
// [spec:libedit:def:histedit.history-end-fn]
// [spec:libedit:sem:histedit.history-end-fn]
void		history_end(History *);

// [spec:libedit:def:histedit.history-fn]
// [spec:libedit:sem:histedit.history-fn]
int		history(History *, HistEvent *, int, ...);

#define	H_FUNC		 0	/* , UTSL		*/
#define	H_SETSIZE	 1	/* , const int);	*/
#define	H_GETSIZE	 2	/* , void);		*/
#define	H_FIRST		 3	/* , void);		*/
#define	H_LAST		 4	/* , void);		*/
#define	H_PREV		 5	/* , void);		*/
#define	H_NEXT		 6	/* , void);		*/
#define	H_CURR		 8	/* , const int);	*/
#define	H_SET		 7	/* , int);		*/
#define	H_ADD		 9	/* , const wchar_t *);	*/
#define	H_ENTER		10	/* , const wchar_t *);	*/
#define	H_APPEND	11	/* , const wchar_t *);	*/
#define	H_END		12	/* , void);		*/
#define	H_NEXT_STR	13	/* , const wchar_t *);	*/
#define	H_PREV_STR	14	/* , const wchar_t *);	*/
#define	H_NEXT_EVENT	15	/* , const int);	*/
#define	H_PREV_EVENT	16	/* , const int);	*/
#define	H_LOAD		17	/* , const char *);	*/
#define	H_SAVE		18	/* , const char *);	*/
#define	H_CLEAR		19	/* , void);		*/
#define	H_SETUNIQUE	20	/* , int);		*/
#define	H_GETUNIQUE	21	/* , void);		*/
#define	H_DEL		22	/* , int);		*/
#define	H_NEXT_EVDATA	23	/* , const int, histdata_t *);	*/
#define	H_DELDATA	24	/* , int, histdata_t *);*/
#define	H_REPLACE	25	/* , const char *, histdata_t);	*/
#define	H_SAVE_FP	26	/* , FILE *);		*/
#define	H_NSAVE_FP	27	/* , size_t, FILE *);	*/



/*
 * ==== Tokenization ====
 */

// [spec:libedit:def:histedit.tokenizer]
typedef struct tokenizer Tokenizer;

/*
 * String tokenization functions, using simplified sh(1) quoting rules
 */
// [spec:libedit:def:histedit.tok-init-fn]
// [spec:libedit:sem:histedit.tok-init-fn]
Tokenizer	*tok_init(const char *);
// [spec:libedit:def:histedit.tok-end-fn]
// [spec:libedit:sem:histedit.tok-end-fn]
void		 tok_end(Tokenizer *);
// [spec:libedit:def:histedit.tok-reset-fn]
// [spec:libedit:sem:histedit.tok-reset-fn]
void		 tok_reset(Tokenizer *);
// [spec:libedit:def:histedit.tok-line-fn]
// [spec:libedit:sem:histedit.tok-line-fn]
int		 tok_line(Tokenizer *, const LineInfo *,
		    int *, const char ***, int *, int *);
// [spec:libedit:def:histedit.tok-str-fn]
// [spec:libedit:sem:histedit.tok-str-fn]
int		 tok_str(Tokenizer *, const char *,
		    int *, const char ***);

/*
 * Begin Wide Character Support
 */
#include <wchar.h>
#include <wctype.h>

#ifndef HAVE_WCSDUP
// [spec:libedit:def:histedit.wcsdup-fn]
// [spec:libedit:sem:histedit.wcsdup-fn]
wchar_t * wcsdup(const wchar_t *str);
#endif

/*
 * ==== Editing ====
 */
// [spec:libedit:def:histedit.lineinfow]
// [spec:libedit:def:histedit.line-info-w]
typedef struct lineinfow {
	const wchar_t	*buffer;
	const wchar_t	*cursor;
	const wchar_t	*lastchar;
} LineInfoW;

// [spec:libedit:def:histedit.el-rfunc-t-edit-line-wchar-t]
typedef int	(*el_rfunc_t)(EditLine *, wchar_t *);

// [spec:libedit:def:histedit.el-wgets-fn]
// [spec:libedit:sem:histedit.el-wgets-fn]
const wchar_t	*el_wgets(EditLine *, int *);
// [spec:libedit:def:histedit.el-wgetc-fn]
// [spec:libedit:sem:histedit.el-wgetc-fn]
int		 el_wgetc(EditLine *, wchar_t *);
// [spec:libedit:def:histedit.el-wpush-fn]
// [spec:libedit:sem:histedit.el-wpush-fn]
void		 el_wpush(EditLine *, const wchar_t *);

// [spec:libedit:def:histedit.el-wparse-fn]
// [spec:libedit:sem:histedit.el-wparse-fn]
int		 el_wparse(EditLine *, int, const wchar_t **);

// [spec:libedit:def:histedit.el-wset-fn]
// [spec:libedit:sem:histedit.el-wset-fn]
int		 el_wset(EditLine *, int, ...);
// [spec:libedit:def:histedit.el-wget-fn]
// [spec:libedit:sem:histedit.el-wget-fn]
int		 el_wget(EditLine *, int, ...);

// [spec:libedit:def:histedit.el-cursor-fn]
// [spec:libedit:sem:histedit.el-cursor-fn]
int		 el_cursor(EditLine *, int);
// [spec:libedit:def:histedit.el-wline-fn]
// [spec:libedit:sem:histedit.el-wline-fn]
const LineInfoW	*el_wline(EditLine *);
// [spec:libedit:def:histedit.el-winsertstr-fn]
// [spec:libedit:sem:histedit.el-winsertstr-fn]
int		 el_winsertstr(EditLine *, const wchar_t *);
#define          el_wdeletestr  el_deletestr
// [spec:libedit:def:histedit.el-wreplacestr-fn]
// [spec:libedit:sem:histedit.el-wreplacestr-fn]
int		 el_wreplacestr(EditLine *, const wchar_t *);

/*
 * ==== History ====
 */
// [spec:libedit:def:histedit.histevent-w]
// [spec:libedit:def:histedit.hist-event-w]
typedef struct histeventW {
	int		 num;
	const wchar_t	*str;
} HistEventW;

// [spec:libedit:def:histedit.history-w]
typedef struct historyW HistoryW;

// [spec:libedit:def:histedit.history-winit-fn]
// [spec:libedit:sem:histedit.history-winit-fn]
HistoryW *	history_winit(void);
// [spec:libedit:def:histedit.history-wend-fn]
// [spec:libedit:sem:histedit.history-wend-fn]
void		history_wend(HistoryW *);

// [spec:libedit:def:histedit.history-w-fn]
// [spec:libedit:sem:histedit.history-w-fn]
int		history_w(HistoryW *, HistEventW *, int, ...);

/*
 * ==== Tokenization ====
 */
// [spec:libedit:def:histedit.tokenizer-w]
typedef struct tokenizerW TokenizerW;

/* Wide character tokenizer support */
// [spec:libedit:def:histedit.tok-winit-fn]
// [spec:libedit:sem:histedit.tok-winit-fn]
TokenizerW	*tok_winit(const wchar_t *);
// [spec:libedit:def:histedit.tok-wend-fn]
// [spec:libedit:sem:histedit.tok-wend-fn]
void		 tok_wend(TokenizerW *);
// [spec:libedit:def:histedit.tok-wreset-fn]
// [spec:libedit:sem:histedit.tok-wreset-fn]
void		 tok_wreset(TokenizerW *);
// [spec:libedit:def:histedit.tok-wline-fn]
// [spec:libedit:sem:histedit.tok-wline-fn]
int		 tok_wline(TokenizerW *, const LineInfoW *,
		    int *, const wchar_t ***, int *, int *);
// [spec:libedit:def:histedit.tok-wstr-fn]
// [spec:libedit:sem:histedit.tok-wstr-fn]
int		 tok_wstr(TokenizerW *, const wchar_t *,
		    int *, const wchar_t ***);

#ifdef __cplusplus
}
#endif

#endif /* _HISTEDIT_H_ */
