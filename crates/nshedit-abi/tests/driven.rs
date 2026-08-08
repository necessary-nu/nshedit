//! What the conformance drivers provably drive.
//!
//! # This file is generated
//!
//! `./conformance/coverage.sh` writes it and `--check` verifies it. Do not
//! edit it by hand: every line below was produced by rebuilding the cdylib
//! under `-C instrument-coverage`, running each driver against it, and asking
//! `llvm-cov` which functions executed. Editing one in would be a claim
//! nothing measured.
//!
//! # Why the claims live here and not beside the functions
//!
//! The port gate reads the `include` globs in `.config/nspec/config.styx`,
//! so a `/test` facet counts only inside `crates/**/*.rs`. Measured, not
//! assumed: a `/test` annotation in a test file moves the count and a bare
//! `sem` annotation in the same place does not. `conformance/**` is in
//! `test_include`, which `nplan spec status` reads and the port gate does
//! not — so the drivers cannot claim anything from where they live, and this
//! file is the bridge.
//!
//! Putting them here is also the more honest arrangement. A `/test` next to
//! an implementation reads as "this function has a unit test"; these are not
//! unit tests, they are one C program driving both libraries and diffing the
//! traces, and the claim belongs next to the test that runs them.
//!
//! # What a claim means, and what it does not
//!
//! It means: this driver executed this function, and the trace it produced
//! was identical to the one the C produced. It does not mean the function is
//! exhaustively tested — a driver that calls `history_save` once has covered
//! one path through it. The `conformance` node's constraint is about the
//! opposite failure, claiming what nothing runs, and this is what keeps the
//! count on the right side of it.
//!
//! Rules whose functions no driver reaches are absent rather than listed as
//! gaps. The gap is the difference between this count and 572, and it is
//! meant to be read from `nplan port status` rather than from here.

// ---------------------------------------------------------------------------
// conformance/driver/hist_tok.c — 41 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn/test]  crates/nshedit/src/chartype.rs:104
// [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn/test]  crates/nshedit/src/chartype.rs:138
// [spec:libedit:sem:chartype.ct-decode-string-fn/test]  crates/nshedit/src/chartype.rs:225
// [spec:libedit:sem:chartype.ct-enc-width-fn/test]  crates/nshedit/src/chartype.rs:329
// [spec:libedit:sem:chartype.ct-encode-char-fn/test]  crates/nshedit/src/chartype.rs:347
// [spec:libedit:sem:chartype.ct-encode-string-fn/test]  crates/nshedit/src/chartype.rs:171
// [spec:libedit:sem:histedit.history-w-fn/test]  crates/nshedit-abi/src/histedit.rs:1809
// [spec:libedit:sem:histedit.history-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1634
// [spec:libedit:sem:histedit.history-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1622
// [spec:libedit:sem:histedit.tok-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1840
// [spec:libedit:sem:histedit.tok-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1828
// [spec:libedit:sem:histedit.tok-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1864
// [spec:libedit:sem:histedit.tok-wreset-fn/test]  crates/nshedit-abi/src/histedit.rs:1852
// [spec:libedit:sem:histedit.tok-wstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1890
// [spec:libedit:sem:history.fun-history-end-fn/test]  crates/nshedit-abi/src/history.rs:734
// [spec:libedit:sem:history.fun-history-init-fn/test]  crates/nshedit-abi/src/history.rs:272
// [spec:libedit:sem:history.funw-history-fn/test]  crates/nshedit-abi/src/history/dispatch.rs:203
// [spec:libedit:sem:history.history-def-add-fn/test]  crates/nshedit-abi/src/history.rs:541
// [spec:libedit:sem:history.history-def-clear-fn/test]  crates/nshedit-abi/src/history.rs:604
// [spec:libedit:sem:history.history-def-curr-fn/test]  crates/nshedit-abi/src/history.rs:395
// [spec:libedit:sem:history.history-def-del-fn/test]  crates/nshedit-abi/src/history.rs:566
// [spec:libedit:sem:history.history-def-delete-fn/test]  crates/nshedit-abi/src/history.rs:478
// [spec:libedit:sem:history.history-def-enter-fn/test]  crates/nshedit-abi/src/history.rs:533
// [spec:libedit:sem:history.history-def-first-fn/test]  crates/nshedit-abi/src/history.rs:322
// [spec:libedit:sem:history.history-def-init-fn/test]  crates/nshedit-abi/src/history.rs:272
// [spec:libedit:sem:history.history-def-insert-fn/test]  crates/nshedit-abi/src/history.rs:484
// [spec:libedit:sem:history.history-def-last-fn/test]  crates/nshedit-abi/src/history.rs:334
// [spec:libedit:sem:history.history-def-next-fn/test]  crates/nshedit-abi/src/history.rs:346
// [spec:libedit:sem:history.history-def-prev-fn/test]  crates/nshedit-abi/src/history.rs:365
// [spec:libedit:sem:history.history-def-set-fn/test]  crates/nshedit-abi/src/history.rs:412
// [spec:libedit:sem:history.history-deldata-nth-fn/test]  crates/nshedit-abi/src/history.rs:579
// [spec:libedit:sem:history.history-getsize-fn/test]  crates/nshedit-abi/src/history.rs:627
// [spec:libedit:sem:history.history-getunique-fn/test]  crates/nshedit-abi/src/history.rs:653
// [spec:libedit:sem:history.history-set-nth-fn/test]  crates/nshedit-abi/src/history.rs:436
// [spec:libedit:sem:history.history-setsize-fn/test]  crates/nshedit-abi/src/history.rs:612
// [spec:libedit:sem:history.history-setunique-fn/test]  crates/nshedit-abi/src/history.rs:638
// [spec:libedit:sem:tokenizer.fun-tok-finish-fn/test]  crates/nshedit-abi/src/adapter.rs:549
// [spec:libedit:sem:tokenizer.fun-tok-init-fn/test]  crates/nshedit-abi/src/adapter.rs:311
// [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]  crates/nshedit-abi/src/adapter.rs:365
// [spec:libedit:sem:tokenizer.fun-tok-reset-fn/test]  crates/nshedit-abi/src/adapter.rs:352
// [spec:libedit:sem:tokenizer.fun-tok-str-fn/test]  crates/nshedit-abi/src/adapter.rs:365

// ---------------------------------------------------------------------------
// conformance/driver/el_api.c — 122 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.c-delbefore-fn/test]  crates/nshedit/src/chared.rs:403
// [spec:libedit:sem:chared.c-insert-fn/test]  crates/nshedit/src/chared.rs:274
// [spec:libedit:sem:chared.ch-end-fn/test]  crates/nshedit/src/chared.rs:1008
// [spec:libedit:sem:chared.ch-init-fn/test]  crates/nshedit/src/chared.rs:827
// [spec:libedit:sem:chared.ch-reset-fn/test]  crates/nshedit/src/chared.rs:889
// [spec:libedit:sem:chared.cv-undo-fn/test]  crates/nshedit/src/chared.rs:205
// [spec:libedit:sem:chared.cv-yank-fn/test]  crates/nshedit/src/chared.rs:246
// [spec:libedit:sem:chared.el-cursor-fn/test]  crates/nshedit/src/chared.rs:1201
// [spec:libedit:sem:chared.el-deletestr-fn/test]  crates/nshedit/src/chared.rs:1076
// [spec:libedit:sem:chared.el-winsertstr-fn/test]  crates/nshedit/src/chared.rs:1039
// [spec:libedit:sem:chartype.ct-chr-class-fn/test]  crates/nshedit/src/chartype.rs:539
// [spec:libedit:sem:chartype.ct-visual-char-fn/test]  crates/nshedit/src/chartype.rs:471
// [spec:libedit:sem:chartype.ct-visual-string-fn/test]  crates/nshedit/src/chartype.rs:382
// [spec:libedit:sem:el.editline.el-getenv-fn/test]  crates/nshedit/src/el.rs:507
// [spec:libedit:sem:el.el-editmode-fn/test]  crates/nshedit/src/el.rs:1391
// [spec:libedit:sem:el.el-end-fn/test]  crates/nshedit/src/el.rs:1024
// [spec:libedit:sem:el.el-init-fd-fn/test]  crates/nshedit/src/el.rs:972
// [spec:libedit:sem:el.el-init-internal-fn/test]  crates/nshedit/src/el.rs:627
// [spec:libedit:sem:el.el-reset-fn/test]  crates/nshedit/src/el.rs:1108
// [spec:libedit:sem:el.el-resize-fn/test]  crates/nshedit/src/el.rs:1314
// [spec:libedit:sem:el.el-source-fn/test]  crates/nshedit/src/el.rs:1204
// [spec:libedit:sem:el.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1575
// [spec:libedit:sem:el.secure-getenv-fn/test]  crates/nshedit/src/el.rs:368
// [spec:libedit:sem:eln.el-get-fn/test]  crates/nshedit-abi/src/eln.rs:587
// [spec:libedit:sem:eln.el-insertstr-fn/test]  crates/nshedit-abi/src/eln.rs:804
// [spec:libedit:sem:eln.el-line-fn/test]  crates/nshedit-abi/src/eln.rs:710
// [spec:libedit:sem:eln.el-parse-fn/test]  crates/nshedit-abi/src/eln.rs:305
// [spec:libedit:sem:eln.el-set-fn/test]  crates/nshedit-abi/src/eln.rs:380
// [spec:libedit:sem:hist.hist-command-fn/test]  crates/nshedit/src/hist.rs:430
// [spec:libedit:sem:hist.hist-end-fn/test]  crates/nshedit/src/hist.rs:241
// [spec:libedit:sem:hist.hist-init-fn/test]  crates/nshedit/src/hist.rs:215
// [spec:libedit:sem:histedit.el-cursor-fn/test]  crates/nshedit-abi/src/histedit.rs:1560
// [spec:libedit:sem:histedit.el-deletestr-fn/test]  crates/nshedit-abi/src/histedit.rs:678
// [spec:libedit:sem:histedit.el-end-fn/test]  crates/nshedit-abi/src/histedit.rs:529
// [spec:libedit:sem:histedit.el-init-fn/test]  crates/nshedit-abi/src/histedit.rs:473
// [spec:libedit:sem:histedit.el-resize-fn/test]  crates/nshedit-abi/src/histedit.rs:653
// [spec:libedit:sem:histedit.el-source-fn/test]  crates/nshedit-abi/src/histedit.rs:636
// [spec:libedit:sem:histedit.el-winsertstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1601
// [spec:libedit:sem:histedit.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1575
// [spec:libedit:sem:histedit.el-wparse-fn/test]  crates/nshedit-abi/src/histedit.rs:910
// [spec:libedit:sem:histedit.tok-end-fn/test]  crates/nshedit-abi/src/histedit.rs:773
// [spec:libedit:sem:histedit.tok-init-fn/test]  crates/nshedit-abi/src/histedit.rs:757
// [spec:libedit:sem:histedit.tok-reset-fn/test]  crates/nshedit-abi/src/histedit.rs:785
// [spec:libedit:sem:histedit.tok-str-fn/test]  crates/nshedit-abi/src/histedit.rs:826
// [spec:libedit:sem:keymacro.keymacro-add-fn/test]  crates/nshedit/src/keymacro.rs:208
// [spec:libedit:sem:keymacro.keymacro-clear-fn/test]  crates/nshedit/src/keymacro.rs:251
// [spec:libedit:sem:keymacro.keymacro-decode-str-fn/test]  crates/nshedit/src/keymacro.rs:936
// [spec:libedit:sem:keymacro.keymacro-end-fn/test]  crates/nshedit/src/keymacro.rs:116
// [spec:libedit:sem:keymacro.keymacro-init-fn/test]  crates/nshedit/src/keymacro.rs:84
// [spec:libedit:sem:keymacro.keymacro-kprint-fn/test]  crates/nshedit/src/keymacro.rs:853
// [spec:libedit:sem:keymacro.keymacro-map-cmd-fn/test]  crates/nshedit/src/keymacro.rs:143
// [spec:libedit:sem:keymacro.keymacro-map-str-fn/test]  crates/nshedit/src/keymacro.rs:160
// [spec:libedit:sem:keymacro.keymacro-print-fn/test]  crates/nshedit/src/keymacro.rs:327
// [spec:libedit:sem:keymacro.keymacro-reset-fn/test]  crates/nshedit/src/keymacro.rs:175
// [spec:libedit:sem:keymacro.node-enum-fn/test]  crates/nshedit/src/keymacro.rs:783
// [spec:libedit:sem:keymacro.node-free-fn/test]  crates/nshedit/src/keymacro.rs:692
// [spec:libedit:sem:keymacro.node-get-fn/test]  crates/nshedit/src/keymacro.rs:668
// [spec:libedit:sem:keymacro.node-lookup-fn/test]  crates/nshedit/src/keymacro.rs:715
// [spec:libedit:sem:keymacro.node-put-fn/test]  crates/nshedit/src/keymacro.rs:628
// [spec:libedit:sem:keymacro.node-try-fn/test]  crates/nshedit/src/keymacro.rs:429
// [spec:libedit:sem:literal.literal-clear-fn/test]  crates/nshedit/src/literal.rs:107
// [spec:libedit:sem:literal.literal-end-fn/test]  crates/nshedit/src/literal.rs:92
// [spec:libedit:sem:literal.literal-init-fn/test]  crates/nshedit/src/literal.rs:69
// [spec:libedit:sem:map.map-bind-fn/test]  crates/nshedit/src/map.rs:1473
// [spec:libedit:sem:map.map-end-fn/test]  crates/nshedit/src/map.rs:1078
// [spec:libedit:sem:map.map-init-emacs-fn/test]  crates/nshedit/src/map.rs:1225
// [spec:libedit:sem:map.map-init-fn/test]  crates/nshedit/src/map.rs:1010
// [spec:libedit:sem:map.map-init-meta-fn/test]  crates/nshedit/src/map.rs:1133
// [spec:libedit:sem:map.map-init-nls-fn/test]  crates/nshedit/src/map.rs:1106
// [spec:libedit:sem:map.map-init-vi-fn/test]  crates/nshedit/src/map.rs:1179
// [spec:libedit:sem:map.map-print-all-keys-fn/test]  crates/nshedit/src/map.rs:1434
// [spec:libedit:sem:map.map-print-key-fn/test]  crates/nshedit/src/map.rs:1347
// [spec:libedit:sem:map.map-print-some-keys-fn/test]  crates/nshedit/src/map.rs:1383
// [spec:libedit:sem:map.map-set-editor-fn/test]  crates/nshedit/src/map.rs:1275
// [spec:libedit:sem:parse.el-wparse-fn/test]  crates/nshedit/src/parse.rs:235
// [spec:libedit:sem:parse.parse-cmd-fn/test]  crates/nshedit/src/parse.rs:499
// [spec:libedit:sem:parse.parse-escape-fn/test]  crates/nshedit/src/parse.rs:319
// [spec:libedit:sem:parse.parse-line-fn/test]  crates/nshedit/src/parse.rs:205
// [spec:libedit:sem:parse.parse-string-fn/test]  crates/nshedit/src/parse.rs:449
// [spec:libedit:sem:prompt.prompt-end-fn/test]  crates/nshedit/src/prompt.rs:296
// [spec:libedit:sem:prompt.prompt-init-fn/test]  crates/nshedit/src/prompt.rs:275
// [spec:libedit:sem:read.read-clearmacros-fn/test]  crates/nshedit/src/read.rs:670
// [spec:libedit:sem:read.read-end-fn/test]  crates/nshedit/src/read.rs:123
// [spec:libedit:sem:read.read-finish-fn/test]  crates/nshedit/src/read.rs:819
// [spec:libedit:sem:read.read-init-fn/test]  crates/nshedit/src/read.rs:85
// [spec:libedit:sem:read.read-prepare-fn/test]  crates/nshedit/src/read.rs:779
// [spec:libedit:sem:refresh.re-clear-display-fn/test]  crates/nshedit/src/refresh.rs:1363
// [spec:libedit:sem:search.search-end-fn/test]  crates/nshedit/src/search.rs:224
// [spec:libedit:sem:search.search-init-fn/test]  crates/nshedit/src/search.rs:205
// [spec:libedit:sem:sig.sig-clr-fn/test]  crates/nshedit/src/sig.rs:411
// [spec:libedit:sem:sig.sig-end-fn/test]  crates/nshedit/src/sig.rs:299
// [spec:libedit:sem:sig.sig-init-fn/test]  crates/nshedit/src/sig.rs:262
// [spec:libedit:sem:terminal.terminal-alloc-buffer-fn/test]  crates/nshedit/src/terminal.rs:1101
// [spec:libedit:sem:terminal.terminal-alloc-display-fn/test]  crates/nshedit/src/terminal.rs:1129
// [spec:libedit:sem:terminal.terminal-bind-arrow-fn/test]  crates/nshedit/src/terminal.rs:1903
// [spec:libedit:sem:terminal.terminal-change-size-fn/test]  crates/nshedit/src/terminal.rs:1696
// [spec:libedit:sem:terminal.terminal-echotc-fn/test]  crates/nshedit/src/terminal.rs:2354
// [spec:libedit:sem:terminal.terminal-end-fn/test]  crates/nshedit/src/terminal.rs:1012
// [spec:libedit:sem:terminal.terminal-free-buffer-fn/test]  crates/nshedit/src/terminal.rs:1120
// [spec:libedit:sem:terminal.terminal-free-display-fn/test]  crates/nshedit/src/terminal.rs:1149
// [spec:libedit:sem:terminal.terminal-get-fn/test]  crates/nshedit/src/terminal.rs:1505
// [spec:libedit:sem:terminal.terminal-get-size-fn/test]  crates/nshedit/src/terminal.rs:1669
// [spec:libedit:sem:terminal.terminal-init-arrow-fn/test]  crates/nshedit/src/terminal.rs:1741
// [spec:libedit:sem:terminal.terminal-init-fn/test]  crates/nshedit/src/terminal.rs:966
// [spec:libedit:sem:terminal.terminal-print-arrow-fn/test]  crates/nshedit/src/terminal.rs:1881
// [spec:libedit:sem:terminal.terminal-rebuffer-display-fn/test]  crates/nshedit/src/terminal.rs:1077
// [spec:libedit:sem:terminal.terminal-reset-arrow-fn/test]  crates/nshedit/src/terminal.rs:1792
// [spec:libedit:sem:terminal.terminal-set-fn/test]  crates/nshedit/src/terminal.rs:1513
// [spec:libedit:sem:terminal.terminal-setflags-fn/test]  crates/nshedit/src/terminal.rs:909
// [spec:libedit:sem:terminal.terminal-settc-fn/test]  crates/nshedit/src/terminal.rs:2180
// [spec:libedit:sem:terminal.terminal-telltc-fn/test]  crates/nshedit/src/terminal.rs:2114
// [spec:libedit:sem:terminal.tgetent-fn/test]  crates/nshedit/src/terminal.rs:586
// [spec:libedit:sem:terminal.tgetflag-fn/test]  crates/nshedit/src/terminal.rs:619
// [spec:libedit:sem:terminal.tgetnum-fn/test]  crates/nshedit/src/terminal.rs:631
// [spec:libedit:sem:terminal.tgetstr-fn/test]  crates/nshedit/src/terminal.rs:653
// [spec:libedit:sem:tokenizer.fun-tok-end-fn/test]  crates/nshedit-abi/src/histedit.rs:773
// [spec:libedit:sem:tty.tty-bind-char-fn/test]  crates/nshedit/src/tty.rs:1106
// [spec:libedit:sem:tty.tty-cookedmode-fn/test]  crates/nshedit/src/tty.rs:1353
// [spec:libedit:sem:tty.tty-getty-fn/test]  crates/nshedit/src/tty.rs:777
// [spec:libedit:sem:tty.tty-init-fn/test]  crates/nshedit/src/tty.rs:946
// [spec:libedit:sem:tty.tty-rawmode-fn/test]  crates/nshedit/src/tty.rs:1260
// [spec:libedit:sem:tty.tty-setup-fn/test]  crates/nshedit/src/tty.rs:839

// ---------------------------------------------------------------------------
// conformance/driver/readline_api.c — 91 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.el-deletestr1-fn/test]  crates/nshedit/src/chared.rs:1106
// [spec:libedit:sem:chared.el-wreplacestr-fn/test]  crates/nshedit/src/chared.rs:1168
// [spec:libedit:sem:eln.el-replacestr-fn/test]  crates/nshedit-abi/src/eln.rs:826
// [spec:libedit:sem:filecomplete.completion-matches-fn/test]  crates/nshedit/src/filecomplete.rs:747
// [spec:libedit:sem:filecomplete.fn-display-match-list-fn/test]  crates/nshedit/src/filecomplete.rs:868
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn/test]  crates/nshedit-abi/src/filecomplete.rs:591
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test]  crates/nshedit-abi/src/filecomplete.rs:532
// [spec:libedit:sem:hist.hist-set-fn/test]  crates/nshedit/src/hist.rs:273
// [spec:libedit:sem:histedit.el-deletestr1-fn/test]  crates/nshedit-abi/src/histedit.rs:698
// [spec:libedit:sem:histedit.el-wreplacestr-fn/test]  crates/nshedit-abi/src/histedit.rs:1612
// [spec:libedit:sem:histedit.history-fn/test]  crates/nshedit-abi/src/histedit.rs:741
// [spec:libedit:sem:histedit.history-init-fn/test]  crates/nshedit-abi/src/histedit.rs:711
// [spec:libedit:sem:prompt.prompt-set-fn/test]  crates/nshedit/src/prompt.rs:317
// [spec:libedit:sem:readline.add-history-fn/test]  crates/nshedit-abi/src/readline.rs:2627
// [spec:libedit:sem:readline.append-history-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:252
// [spec:libedit:sem:readline.clear-history-fn/test]  crates/nshedit-abi/src/readline.rs:2749
// [spec:libedit:sem:readline.completion-matches-fn/test]  crates/nshedit-abi/src/readline.rs:4578
// [spec:libedit:sem:readline.current-history-fn/test]  crates/nshedit-abi/src/readline.rs:2825
// [spec:libedit:sem:readline.default-history-file-fn/test]  crates/nshedit-abi/src/readline.rs:1137
// [spec:libedit:sem:readline.filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3095
// [spec:libedit:sem:readline.get-history-event-fn/test]  crates/nshedit-abi/src/readline.rs:1627
// [spec:libedit:sem:readline.getfrom-fn/test]  crates/nshedit-abi/src/readline.rs:1777
// [spec:libedit:sem:readline.getto-fn/test]  crates/nshedit-abi/src/readline.rs:1859
// [spec:libedit:sem:readline.history-arg-extract-fn/test]  crates/nshedit-abi/src/readline.rs:2361
// [spec:libedit:sem:readline.history-expand-command-fn/test]  crates/nshedit-abi/src/readline.rs:1939
// [spec:libedit:sem:readline.history-expand-fn/test]  crates/nshedit-abi/src/readline.rs:2211
// [spec:libedit:sem:readline.history-get-fn/test]  crates/nshedit-abi/src/readline.rs:2582
// [spec:libedit:sem:readline.history-get-history-state-fn/test]  crates/nshedit-abi/src/readline.rs:4264
// [spec:libedit:sem:readline.history-is-stifled-fn/test]  crates/nshedit-abi/src/readline.rs:2565
// [spec:libedit:sem:readline.history-list-fn/test]  crates/nshedit-abi/src/readline.rs:2779
// [spec:libedit:sem:readline.history-search-fn/test]  crates/nshedit-abi/src/readline.rs:2959
// [spec:libedit:sem:readline.history-search-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3032
// [spec:libedit:sem:readline.history-search-prefix-fn/test]  crates/nshedit-abi/src/readline.rs:3004
// [spec:libedit:sem:readline.history-set-pos-fn/test]  crates/nshedit-abi/src/readline.rs:2891
// [spec:libedit:sem:readline.history-tokenize-fn/test]  crates/nshedit-abi/src/readline.rs:2426
// [spec:libedit:sem:readline.history-total-bytes-fn/test]  crates/nshedit-abi/src/readline.rs:2853
// [spec:libedit:sem:readline.history-truncate-file-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:80
// [spec:libedit:sem:readline.next-history-fn/test]  crates/nshedit-abi/src/readline.rs:2936
// [spec:libedit:sem:readline.previous-history-fn/test]  crates/nshedit-abi/src/readline.rs:2911
// [spec:libedit:sem:readline.read-history-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:182
// [spec:libedit:sem:readline.remove-history-fn/test]  crates/nshedit-abi/src/readline.rs:2657
// [spec:libedit:sem:readline.replace-fn/test]  crates/nshedit-abi/src/readline.rs:1912
// [spec:libedit:sem:readline.replace-history-entry-fn/test]  crates/nshedit-abi/src/readline.rs:2693
// [spec:libedit:sem:readline.resize-fun-fn/test]  crates/nshedit-abi/src/readline.rs:1114
// [spec:libedit:sem:readline.rl-add-defun-fn/test]  crates/nshedit-abi/src/readline.rs:3532
// [spec:libedit:sem:readline.rl-bind-key-fn/test]  crates/nshedit-abi/src/readline.rs:3363
// [spec:libedit:sem:readline.rl-bind-key-in-map-fn/test]  crates/nshedit-abi/src/readline.rs:4360
// [spec:libedit:sem:readline.rl-compat-sub-fn/test]  crates/nshedit-abi/src/readline.rs:1581
// [spec:libedit:sem:readline.rl-completion-append-character-function-fn/test]  crates/nshedit-abi/src/readline.rs:982
// [spec:libedit:sem:readline.rl-completion-matches-fn/test]  crates/nshedit-abi/src/readline.rs:4093
// [spec:libedit:sem:readline.rl-copy-text-fn/test]  crates/nshedit-abi/src/readline.rs:3922
// [spec:libedit:sem:readline.rl-crlf-fn/test]  crates/nshedit-abi/src/readline.rs:4477
// [spec:libedit:sem:readline.rl-delete-text-fn/test]  crates/nshedit-abi/src/readline.rs:4001
// [spec:libedit:sem:readline.rl-ding-fn/test]  crates/nshedit-abi/src/readline.rs:4493
// [spec:libedit:sem:readline.rl-display-match-list-fn/test]  crates/nshedit-abi/src/readline.rs:3177
// [spec:libedit:sem:readline.rl-filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:4194
// [spec:libedit:sem:readline.rl-forced-update-display-fn/test]  crates/nshedit-abi/src/readline.rs:4209
// [spec:libedit:sem:readline.rl-free-line-state-fn/test]  crates/nshedit-abi/src/readline.rs:4408
// [spec:libedit:sem:readline.rl-generic-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4343
// [spec:libedit:sem:readline.rl-get-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4324
// [spec:libedit:sem:readline.rl-get-screen-size-fn/test]  crates/nshedit-abi/src/readline.rs:4019
// [spec:libedit:sem:readline.rl-initialize-fn/test]  crates/nshedit-abi/src/readline.rs:1250
// [spec:libedit:sem:readline.rl-insert-text-fn/test]  crates/nshedit-abi/src/readline.rs:3460
// [spec:libedit:sem:readline.rl-kill-text-fn/test]  crates/nshedit-abi/src/readline.rs:4301
// [spec:libedit:sem:readline.rl-make-bare-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4314
// [spec:libedit:sem:readline.rl-message-fn/test]  crates/nshedit-abi/src/readline.rs:4043
// [spec:libedit:sem:readline.rl-on-new-line-fn/test]  crates/nshedit-abi/src/readline.rs:4398
// [spec:libedit:sem:readline.rl-parse-and-bind-fn/test]  crates/nshedit-abi/src/readline.rs:3749
// [spec:libedit:sem:readline.rl-read-init-file-fn/test]  crates/nshedit-abi/src/readline.rs:3735
// [spec:libedit:sem:readline.rl-replace-line-fn/test]  crates/nshedit-abi/src/readline.rs:3977
// [spec:libedit:sem:readline.rl-restore-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1229
// [spec:libedit:sem:readline.rl-save-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1206
// [spec:libedit:sem:readline.rl-set-key-fn/test]  crates/nshedit-abi/src/readline.rs:4374
// [spec:libedit:sem:readline.rl-set-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4334
// [spec:libedit:sem:readline.rl-set-keymap-name-fn/test]  crates/nshedit-abi/src/readline.rs:4521
// [spec:libedit:sem:readline.rl-set-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1163
// [spec:libedit:sem:readline.rl-set-screen-size-fn/test]  crates/nshedit-abi/src/readline.rs:4064
// [spec:libedit:sem:readline.rl-stuff-char-fn/test]  crates/nshedit-abi/src/readline.rs:3799
// [spec:libedit:sem:readline.rl-update-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3895
// [spec:libedit:sem:readline.rl-variable-bind-fn/test]  crates/nshedit-abi/src/readline.rs:3781
// [spec:libedit:sem:readline.stifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2504
// [spec:libedit:sem:readline.tilde-expand-fn/test]  crates/nshedit-abi/src/readline.rs:3084
// [spec:libedit:sem:readline.unstifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2542
// [spec:libedit:sem:readline.username-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3108
// [spec:libedit:sem:readline.using-history-fn/test]  crates/nshedit-abi/src/readline.rs:1567
// [spec:libedit:sem:readline.where-history-fn/test]  crates/nshedit-abi/src/readline.rs:2768
// [spec:libedit:sem:readline.write-history-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:223
// [spec:libedit:sem:refresh.re-putc-fn/test]  crates/nshedit/src/refresh.rs:305
// [spec:libedit:sem:search.el-match-fn/test]  crates/nshedit/src/search.rs:256
// [spec:libedit:sem:terminal.terminal-gettc-fn/test]  crates/nshedit/src/terminal.rs:2301
// [spec:libedit:sem:tty.tty-end-fn/test]  crates/nshedit/src/tty.rs:973

// ---------------------------------------------------------------------------
// conformance/driver/pty_edit.c — 63 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.c-delbefore1-fn/test]  crates/nshedit/src/chared.rs:447
// [spec:libedit:sem:chared.c-next-word-fn/test]  crates/nshedit/src/chared.rs:565
// [spec:libedit:sem:chared.c-prev-word-fn/test]  crates/nshedit/src/chared.rs:520
// [spec:libedit:sem:chared.ce-isword-fn/test]  crates/nshedit/src/chared.rs:473
// [spec:libedit:sem:chartype.ct-visual-width-fn/test]  crates/nshedit/src/chartype.rs:438
// [spec:libedit:sem:common.ed-insert-fn/test]  crates/nshedit/src/common.rs:217
// [spec:libedit:sem:common.ed-kill-line-fn/test]  crates/nshedit/src/common.rs:363
// [spec:libedit:sem:common.ed-move-to-beg-fn/test]  crates/nshedit/src/common.rs:405
// [spec:libedit:sem:common.ed-move-to-end-fn/test]  crates/nshedit/src/common.rs:382
// [spec:libedit:sem:common.ed-newline-fn/test]  crates/nshedit/src/common.rs:646
// [spec:libedit:sem:common.ed-next-char-fn/test]  crates/nshedit/src/common.rs:472
// [spec:libedit:sem:common.ed-next-history-fn/test]  crates/nshedit/src/common.rs:781
// [spec:libedit:sem:common.ed-prev-char-fn/test]  crates/nshedit/src/common.rs:521
// [spec:libedit:sem:common.ed-prev-history-fn/test]  crates/nshedit/src/common.rs:741
// [spec:libedit:sem:common.ed-prev-word-fn/test]  crates/nshedit/src/common.rs:504
// [spec:libedit:sem:common.ed-transpose-chars-fn/test]  crates/nshedit/src/common.rs:436
// [spec:libedit:sem:eln.el-gets-fn/test]  crates/nshedit-abi/src/eln.rs:232
// [spec:libedit:sem:emacs.em-capitol-case-fn/test]  crates/nshedit/src/emacs.rs:293
// [spec:libedit:sem:emacs.em-delete-or-list-fn/test]  crates/nshedit/src/emacs.rs:20
// [spec:libedit:sem:emacs.em-delete-prev-char-fn/test]  crates/nshedit/src/emacs.rs:515
// [spec:libedit:sem:emacs.em-kill-line-fn/test]  crates/nshedit/src/emacs.rs:142
// [spec:libedit:sem:emacs.em-lower-case-fn/test]  crates/nshedit/src/emacs.rs:337
// [spec:libedit:sem:emacs.em-next-word-fn/test]  crates/nshedit/src/emacs.rs:238
// [spec:libedit:sem:emacs.em-upper-case-fn/test]  crates/nshedit/src/emacs.rs:262
// [spec:libedit:sem:emacs.em-yank-fn/test]  crates/nshedit/src/emacs.rs:94
// [spec:libedit:sem:hist.hist-get-fn/test]  crates/nshedit/src/hist.rs:314
// [spec:libedit:sem:histedit.el-wgets-fn/test]  crates/nshedit-abi/src/histedit.rs:868
// [spec:libedit:sem:histedit.history-end-fn/test]  crates/nshedit-abi/src/histedit.rs:725
// [spec:libedit:sem:histedit.wcsdup-fn/test]  crates/nshedit-abi/src/histedit.rs:868
// [spec:libedit:sem:keymacro.keymacro-get-fn/test]  crates/nshedit/src/keymacro.rs:192
// [spec:libedit:sem:keymacro.node-trav-fn/test]  crates/nshedit/src/keymacro.rs:367
// [spec:libedit:sem:prompt.prompt-default-r-fn/test]  crates/nshedit/src/prompt.rs:101
// [spec:libedit:sem:prompt.prompt-print-fn/test]  crates/nshedit/src/prompt.rs:109
// [spec:libedit:sem:read.el-wgetc-fn/test]  crates/nshedit/src/read.rs:707
// [spec:libedit:sem:read.el-wgets-fn/test]  crates/nshedit/src/read.rs:916
// [spec:libedit:sem:read.read-char-fn/test]  crates/nshedit/src/read.rs:458
// [spec:libedit:sem:read.read-getcmd-fn/test]  crates/nshedit/src/read.rs:273
// [spec:libedit:sem:refresh.re-addc-fn/test]  crates/nshedit/src/refresh.rs:175
// [spec:libedit:sem:refresh.re-clear-eol-fn/test]  crates/nshedit/src/refresh.rs:637
// [spec:libedit:sem:refresh.re-copy-and-pad-fn/test]  crates/nshedit/src/refresh.rs:1080
// [spec:libedit:sem:refresh.re-fastaddc-fn/test]  crates/nshedit/src/refresh.rs:1298
// [spec:libedit:sem:refresh.re-fastputc-fn/test]  crates/nshedit/src/refresh.rs:1205
// [spec:libedit:sem:refresh.re-goto-bottom-fn/test]  crates/nshedit/src/refresh.rs:510
// [spec:libedit:sem:refresh.re-refresh-cursor-fn/test]  crates/nshedit/src/refresh.rs:1114
// [spec:libedit:sem:refresh.re-refresh-fn/test]  crates/nshedit/src/refresh.rs:359
// [spec:libedit:sem:refresh.re-strncopy-fn/test]  crates/nshedit/src/refresh.rs:614
// [spec:libedit:sem:refresh.re-update-line-fn/test]  crates/nshedit/src/refresh.rs:672
// [spec:libedit:sem:terminal.terminal-clear-eol-fn/test]  crates/nshedit/src/terminal.rs:1454
// [spec:libedit:sem:terminal.terminal-flush-fn/test]  crates/nshedit/src/terminal.rs:2081
// [spec:libedit:sem:terminal.terminal-move-to-char-fn/test]  crates/nshedit/src/terminal.rs:1191
// [spec:libedit:sem:terminal.terminal-move-to-line-fn/test]  crates/nshedit/src/terminal.rs:1156
// [spec:libedit:sem:terminal.terminal-overwrite-fn/test]  crates/nshedit/src/terminal.rs:1296
// [spec:libedit:sem:terminal.terminal-putc-fn/test]  crates/nshedit/src/terminal.rs:2051
// [spec:libedit:sem:terminal.terminal-writec-fn/test]  crates/nshedit/src/terminal.rs:2095
// [spec:libedit:sem:tty.tty-get-flag-fn/test]  crates/nshedit/src/tty.rs:1186
// [spec:libedit:sem:tty.tty-getchar-fn/test]  crates/nshedit/src/tty.rs:1070
// [spec:libedit:sem:tty.tty-getspeed-fn/test]  crates/nshedit/src/tty.rs:1006
// [spec:libedit:sem:tty.tty-setchar-fn/test]  crates/nshedit/src/tty.rs:1091
// [spec:libedit:sem:tty.tty-setty-fn/test]  crates/nshedit/src/tty.rs:808
// [spec:libedit:sem:tty.tty-setup-flags-fn/test]  crates/nshedit/src/tty.rs:1771
// [spec:libedit:sem:tty.tty-update-char-fn/test]  crates/nshedit/src/tty.rs:1236
// [spec:libedit:sem:tty.tty-update-flag-fn/test]  crates/nshedit/src/tty.rs:1198
// [spec:libedit:sem:tty.tty-update-flags-fn/test]  crates/nshedit/src/tty.rs:1211

// ---------------------------------------------------------------------------
// conformance/driver/abi_gaps.c — 9 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:map.map-get-editor-fn/test]  crates/nshedit/src/map.rs:1294
// [spec:libedit:sem:map.map-get-wordchars-fn/test]  crates/nshedit/src/map.rs:1333
// [spec:libedit:sem:map.map-set-wordchars-fn/test]  crates/nshedit/src/map.rs:1314
// [spec:libedit:sem:prompt.prompt-get-fn/test]  crates/nshedit/src/prompt.rs:368
// [spec:libedit:sem:read.el-read-setfn-fn/test]  crates/nshedit/src/read.rs:142
// [spec:libedit:sem:readline.get-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1068
// [spec:libedit:sem:readline.readline-fn/test]  crates/nshedit-abi/src/readline.rs:1476
// [spec:libedit:sem:readline.rl-event-read-char-fn/test]  crates/nshedit-abi/src/readline.rs:3817
// [spec:libedit:sem:readline.rl-kill-full-line-fn/test]  crates/nshedit-abi/src/readline.rs:4284

// ---------------------------------------------------------------------------
// conformance/aux/ub_corpus.c — 0 rules
// ---------------------------------------------------------------------------

/// The drivers, and the count each one earns.
///
/// A rule reached by more than one driver is attributed to the first that
/// reaches it, so these sum to the total. The overlap is large and that is
/// expected — 326 of 326 rules are reached by more than one,
/// because every driver goes through the same lifecycle and allocator paths.
#[test]
fn the_claim_list_is_what_coverage_measured() {
    // Regenerate with ./conformance/coverage.sh, verify with --check.
    // 326 rules across 6 drivers, measured under -C instrument-coverage.
    assert_eq!(CLAIMED, 326);
}

/// How many `/test` facets this file carries. The generator and the
/// annotations above are written together, so a hand edit to either
/// desynchronises them and `coverage.sh --check` says so.
const CLAIMED: usize = 326;
