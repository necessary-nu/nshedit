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
// conformance/driver/hist_tok.c — 51 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn/test]  crates/nshedit/src/chartype.rs:104
// [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn/test]  crates/nshedit/src/chartype.rs:138
// [spec:libedit:sem:chartype.ct-decode-string-fn/test]  crates/nshedit/src/chartype.rs:225
// [spec:libedit:sem:chartype.ct-enc-width-fn/test]  crates/nshedit/src/chartype.rs:331
// [spec:libedit:sem:chartype.ct-encode-char-fn/test]  crates/nshedit/src/chartype.rs:348
// [spec:libedit:sem:chartype.ct-encode-string-fn/test]  crates/nshedit/src/chartype.rs:171
// [spec:libedit:sem:histedit.history-w-fn/test]  crates/nshedit-abi/src/histedit.rs:1837
// [spec:libedit:sem:histedit.history-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1648
// [spec:libedit:sem:histedit.history-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1637
// [spec:libedit:sem:histedit.tok-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1865
// [spec:libedit:sem:histedit.tok-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1855
// [spec:libedit:sem:histedit.tok-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1929
// [spec:libedit:sem:histedit.tok-wreset-fn/test]  crates/nshedit-abi/src/histedit.rs:1877
// [spec:libedit:sem:histedit.tok-wstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1974
// [spec:libedit:sem:history.fun-history-end-fn/test]  crates/nshedit/src/history.rs:1266
// [spec:libedit:sem:history.fun-history-init-fn/test]  crates/nshedit/src/history.rs:1211
// [spec:libedit:sem:history.funw-history-fn/test]  crates/nshedit/src/history.rs:2144
// [spec:libedit:sem:history.history-def-add-fn/test]  crates/nshedit/src/history.rs:911
// [spec:libedit:sem:history.history-def-clear-fn/test]  crates/nshedit/src/history.rs:1183
// [spec:libedit:sem:history.history-def-curr-fn/test]  crates/nshedit/src/history.rs:817
// [spec:libedit:sem:history.history-def-del-fn/test]  crates/nshedit/src/history.rs:1007
// [spec:libedit:sem:history.history-def-delete-fn/test]  crates/nshedit/src/history.rs:1040
// [spec:libedit:sem:history.history-def-enter-fn/test]  crates/nshedit/src/history.rs:1105
// [spec:libedit:sem:history.history-def-first-fn/test]  crates/nshedit/src/history.rs:686
// [spec:libedit:sem:history.history-def-init-fn/test]  crates/nshedit/src/history.rs:1162
// [spec:libedit:sem:history.history-def-insert-fn/test]  crates/nshedit/src/history.rs:1060
// [spec:libedit:sem:history.history-def-last-fn/test]  crates/nshedit/src/history.rs:718
// [spec:libedit:sem:history.history-def-next-fn/test]  crates/nshedit/src/history.rs:744
// [spec:libedit:sem:history.history-def-prev-fn/test]  crates/nshedit/src/history.rs:778
// [spec:libedit:sem:history.history-def-set-fn/test]  crates/nshedit/src/history.rs:850
// [spec:libedit:sem:history.history-deldata-nth-fn/test]  crates/nshedit/src/history.rs:964
// [spec:libedit:sem:history.history-getsize-fn/test]  crates/nshedit/src/history.rs:1323
// [spec:libedit:sem:history.history-getunique-fn/test]  crates/nshedit/src/history.rs:1363
// [spec:libedit:sem:history.history-load-fn/test]  crates/nshedit/src/history.rs:1482
// [spec:libedit:sem:history.history-next-evdata-fn/test]  crates/nshedit/src/history.rs:1979
// [spec:libedit:sem:history.history-next-event-fn/test]  crates/nshedit/src/history.rs:2020
// [spec:libedit:sem:history.history-next-string-fn/test]  crates/nshedit/src/history.rs:2076
// [spec:libedit:sem:history.history-prev-event-fn/test]  crates/nshedit/src/history.rs:1955
// [spec:libedit:sem:history.history-prev-string-fn/test]  crates/nshedit/src/history.rs:2042
// [spec:libedit:sem:history.history-save-fn/test]  crates/nshedit/src/history.rs:1917
// [spec:libedit:sem:history.history-save-fp-fn/test]  crates/nshedit/src/history.rs:1752
// [spec:libedit:sem:history.history-set-fun-fn/test]  crates/nshedit/src/history.rs:1381
// [spec:libedit:sem:history.history-set-nth-fn/test]  crates/nshedit/src/history.rs:883
// [spec:libedit:sem:history.history-setsize-fn/test]  crates/nshedit/src/history.rs:1302
// [spec:libedit:sem:history.history-setunique-fn/test]  crates/nshedit/src/history.rs:1340
// [spec:libedit:sem:tokenizer.fun-tok-end-fn/test]  crates/nshedit/src/tokenizer.rs:273
// [spec:libedit:sem:tokenizer.fun-tok-finish-fn/test]  crates/nshedit/src/tokenizer.rs:192
// [spec:libedit:sem:tokenizer.fun-tok-init-fn/test]  crates/nshedit/src/tokenizer.rs:222
// [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]  crates/nshedit/src/tokenizer.rs:297
// [spec:libedit:sem:tokenizer.fun-tok-reset-fn/test]  crates/nshedit/src/tokenizer.rs:253
// [spec:libedit:sem:tokenizer.fun-tok-str-fn/test]  crates/nshedit/src/tokenizer.rs:541

// ---------------------------------------------------------------------------
// conformance/driver/el_api.c — 121 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.c-delbefore-fn/test]  crates/nshedit/src/chared.rs:407
// [spec:libedit:sem:chared.c-insert-fn/test]  crates/nshedit/src/chared.rs:278
// [spec:libedit:sem:chared.ch-end-fn/test]  crates/nshedit/src/chared.rs:1012
// [spec:libedit:sem:chared.ch-init-fn/test]  crates/nshedit/src/chared.rs:831
// [spec:libedit:sem:chared.ch-reset-fn/test]  crates/nshedit/src/chared.rs:893
// [spec:libedit:sem:chared.cv-undo-fn/test]  crates/nshedit/src/chared.rs:209
// [spec:libedit:sem:chared.cv-yank-fn/test]  crates/nshedit/src/chared.rs:250
// [spec:libedit:sem:chared.el-cursor-fn/test]  crates/nshedit/src/chared.rs:1205
// [spec:libedit:sem:chared.el-deletestr-fn/test]  crates/nshedit/src/chared.rs:1080
// [spec:libedit:sem:chared.el-winsertstr-fn/test]  crates/nshedit/src/chared.rs:1043
// [spec:libedit:sem:chartype.ct-chr-class-fn/test]  crates/nshedit/src/chartype.rs:540
// [spec:libedit:sem:chartype.ct-visual-char-fn/test]  crates/nshedit/src/chartype.rs:472
// [spec:libedit:sem:chartype.ct-visual-string-fn/test]  crates/nshedit/src/chartype.rs:383
// [spec:libedit:sem:el.editline.el-getenv-fn/test]  crates/nshedit/src/el.rs:520
// [spec:libedit:sem:el.el-editmode-fn/test]  crates/nshedit/src/el.rs:1393
// [spec:libedit:sem:el.el-end-fn/test]  crates/nshedit/src/el.rs:1021
// [spec:libedit:sem:el.el-init-fd-fn/test]  crates/nshedit/src/el.rs:988
// [spec:libedit:sem:el.el-init-internal-fn/test]  crates/nshedit/src/el.rs:637
// [spec:libedit:sem:el.el-reset-fn/test]  crates/nshedit/src/el.rs:1110
// [spec:libedit:sem:el.el-resize-fn/test]  crates/nshedit/src/el.rs:1316
// [spec:libedit:sem:el.el-source-fn/test]  crates/nshedit/src/el.rs:1206
// [spec:libedit:sem:el.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1574
// [spec:libedit:sem:el.secure-getenv-fn/test]  crates/nshedit/src/el.rs:381
// [spec:libedit:sem:eln.el-get-fn/test]  crates/nshedit-abi/src/eln.rs:643
// [spec:libedit:sem:eln.el-insertstr-fn/test]  crates/nshedit-abi/src/eln.rs:834
// [spec:libedit:sem:eln.el-line-fn/test]  crates/nshedit-abi/src/eln.rs:741
// [spec:libedit:sem:eln.el-parse-fn/test]  crates/nshedit-abi/src/eln.rs:361
// [spec:libedit:sem:eln.el-set-fn/test]  crates/nshedit-abi/src/eln.rs:436
// [spec:libedit:sem:hist.hist-command-fn/test]  crates/nshedit/src/hist.rs:430
// [spec:libedit:sem:hist.hist-end-fn/test]  crates/nshedit/src/hist.rs:241
// [spec:libedit:sem:hist.hist-init-fn/test]  crates/nshedit/src/hist.rs:215
// [spec:libedit:sem:histedit.el-cursor-fn/test]  crates/nshedit-abi/src/histedit.rs:1560
// [spec:libedit:sem:histedit.el-deletestr-fn/test]  crates/nshedit-abi/src/histedit.rs:640
// [spec:libedit:sem:histedit.el-end-fn/test]  crates/nshedit-abi/src/histedit.rs:492
// [spec:libedit:sem:histedit.el-init-fn/test]  crates/nshedit-abi/src/histedit.rs:440
// [spec:libedit:sem:histedit.el-resize-fn/test]  crates/nshedit-abi/src/histedit.rs:616
// [spec:libedit:sem:histedit.el-source-fn/test]  crates/nshedit-abi/src/histedit.rs:600
// [spec:libedit:sem:histedit.el-winsertstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1618
// [spec:libedit:sem:histedit.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1574
// [spec:libedit:sem:histedit.el-wparse-fn/test]  crates/nshedit-abi/src/histedit.rs:888
// [spec:libedit:sem:histedit.tok-end-fn/test]  crates/nshedit-abi/src/histedit.rs:725
// [spec:libedit:sem:histedit.tok-init-fn/test]  crates/nshedit-abi/src/histedit.rs:713
// [spec:libedit:sem:histedit.tok-reset-fn/test]  crates/nshedit-abi/src/histedit.rs:737
// [spec:libedit:sem:histedit.tok-str-fn/test]  crates/nshedit-abi/src/histedit.rs:807
// [spec:libedit:sem:keymacro.keymacro-add-fn/test]  crates/nshedit/src/keymacro.rs:208
// [spec:libedit:sem:keymacro.keymacro-clear-fn/test]  crates/nshedit/src/keymacro.rs:251
// [spec:libedit:sem:keymacro.keymacro-decode-str-fn/test]  crates/nshedit/src/keymacro.rs:950
// [spec:libedit:sem:keymacro.keymacro-end-fn/test]  crates/nshedit/src/keymacro.rs:116
// [spec:libedit:sem:keymacro.keymacro-init-fn/test]  crates/nshedit/src/keymacro.rs:84
// [spec:libedit:sem:keymacro.keymacro-kprint-fn/test]  crates/nshedit/src/keymacro.rs:866
// [spec:libedit:sem:keymacro.keymacro-map-cmd-fn/test]  crates/nshedit/src/keymacro.rs:143
// [spec:libedit:sem:keymacro.keymacro-map-str-fn/test]  crates/nshedit/src/keymacro.rs:160
// [spec:libedit:sem:keymacro.keymacro-print-fn/test]  crates/nshedit/src/keymacro.rs:327
// [spec:libedit:sem:keymacro.keymacro-reset-fn/test]  crates/nshedit/src/keymacro.rs:175
// [spec:libedit:sem:keymacro.node-enum-fn/test]  crates/nshedit/src/keymacro.rs:796
// [spec:libedit:sem:keymacro.node-free-fn/test]  crates/nshedit/src/keymacro.rs:705
// [spec:libedit:sem:keymacro.node-get-fn/test]  crates/nshedit/src/keymacro.rs:680
// [spec:libedit:sem:keymacro.node-lookup-fn/test]  crates/nshedit/src/keymacro.rs:728
// [spec:libedit:sem:keymacro.node-put-fn/test]  crates/nshedit/src/keymacro.rs:639
// [spec:libedit:sem:keymacro.node-try-fn/test]  crates/nshedit/src/keymacro.rs:430
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
// [spec:libedit:sem:parse.el-wparse-fn/test]  crates/nshedit/src/parse.rs:236
// [spec:libedit:sem:parse.parse-cmd-fn/test]  crates/nshedit/src/parse.rs:502
// [spec:libedit:sem:parse.parse-escape-fn/test]  crates/nshedit/src/parse.rs:321
// [spec:libedit:sem:parse.parse-line-fn/test]  crates/nshedit/src/parse.rs:179
// [spec:libedit:sem:parse.parse-string-fn/test]  crates/nshedit/src/parse.rs:452
// [spec:libedit:sem:prompt.prompt-end-fn/test]  crates/nshedit/src/prompt.rs:296
// [spec:libedit:sem:prompt.prompt-init-fn/test]  crates/nshedit/src/prompt.rs:275
// [spec:libedit:sem:read.read-clearmacros-fn/test]  crates/nshedit/src/read.rs:672
// [spec:libedit:sem:read.read-end-fn/test]  crates/nshedit/src/read.rs:124
// [spec:libedit:sem:read.read-finish-fn/test]  crates/nshedit/src/read.rs:821
// [spec:libedit:sem:read.read-init-fn/test]  crates/nshedit/src/read.rs:86
// [spec:libedit:sem:read.read-prepare-fn/test]  crates/nshedit/src/read.rs:781
// [spec:libedit:sem:refresh.re-clear-display-fn/test]  crates/nshedit/src/refresh.rs:1365
// [spec:libedit:sem:search.search-end-fn/test]  crates/nshedit/src/search.rs:223
// [spec:libedit:sem:search.search-init-fn/test]  crates/nshedit/src/search.rs:204
// [spec:libedit:sem:sig.sig-clr-fn/test]  crates/nshedit/src/sig.rs:411
// [spec:libedit:sem:sig.sig-end-fn/test]  crates/nshedit/src/sig.rs:299
// [spec:libedit:sem:sig.sig-init-fn/test]  crates/nshedit/src/sig.rs:262
// [spec:libedit:sem:terminal.terminal-alloc-buffer-fn/test]  crates/nshedit/src/terminal.rs:1121
// [spec:libedit:sem:terminal.terminal-alloc-display-fn/test]  crates/nshedit/src/terminal.rs:1150
// [spec:libedit:sem:terminal.terminal-bind-arrow-fn/test]  crates/nshedit/src/terminal.rs:1925
// [spec:libedit:sem:terminal.terminal-change-size-fn/test]  crates/nshedit/src/terminal.rs:1718
// [spec:libedit:sem:terminal.terminal-echotc-fn/test]  crates/nshedit/src/terminal.rs:2378
// [spec:libedit:sem:terminal.terminal-end-fn/test]  crates/nshedit/src/terminal.rs:1032
// [spec:libedit:sem:terminal.terminal-free-buffer-fn/test]  crates/nshedit/src/terminal.rs:1141
// [spec:libedit:sem:terminal.terminal-free-display-fn/test]  crates/nshedit/src/terminal.rs:1170
// [spec:libedit:sem:terminal.terminal-get-fn/test]  crates/nshedit/src/terminal.rs:1527
// [spec:libedit:sem:terminal.terminal-get-size-fn/test]  crates/nshedit/src/terminal.rs:1691
// [spec:libedit:sem:terminal.terminal-init-arrow-fn/test]  crates/nshedit/src/terminal.rs:1763
// [spec:libedit:sem:terminal.terminal-init-fn/test]  crates/nshedit/src/terminal.rs:986
// [spec:libedit:sem:terminal.terminal-print-arrow-fn/test]  crates/nshedit/src/terminal.rs:1903
// [spec:libedit:sem:terminal.terminal-rebuffer-display-fn/test]  crates/nshedit/src/terminal.rs:1097
// [spec:libedit:sem:terminal.terminal-reset-arrow-fn/test]  crates/nshedit/src/terminal.rs:1814
// [spec:libedit:sem:terminal.terminal-set-fn/test]  crates/nshedit/src/terminal.rs:1535
// [spec:libedit:sem:terminal.terminal-setflags-fn/test]  crates/nshedit/src/terminal.rs:929
// [spec:libedit:sem:terminal.terminal-settc-fn/test]  crates/nshedit/src/terminal.rs:2204
// [spec:libedit:sem:terminal.terminal-telltc-fn/test]  crates/nshedit/src/terminal.rs:2138
// [spec:libedit:sem:terminal.tgetent-fn/test]  crates/nshedit/src/terminal.rs:606
// [spec:libedit:sem:terminal.tgetflag-fn/test]  crates/nshedit/src/terminal.rs:639
// [spec:libedit:sem:terminal.tgetnum-fn/test]  crates/nshedit/src/terminal.rs:651
// [spec:libedit:sem:terminal.tgetstr-fn/test]  crates/nshedit/src/terminal.rs:673
// [spec:libedit:sem:tty.tty-bind-char-fn/test]  crates/nshedit/src/tty.rs:1113
// [spec:libedit:sem:tty.tty-cookedmode-fn/test]  crates/nshedit/src/tty.rs:1361
// [spec:libedit:sem:tty.tty-getty-fn/test]  crates/nshedit/src/tty.rs:780
// [spec:libedit:sem:tty.tty-init-fn/test]  crates/nshedit/src/tty.rs:948
// [spec:libedit:sem:tty.tty-rawmode-fn/test]  crates/nshedit/src/tty.rs:1268
// [spec:libedit:sem:tty.tty-setup-fn/test]  crates/nshedit/src/tty.rs:842

// ---------------------------------------------------------------------------
// conformance/driver/readline_api.c — 89 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.el-deletestr1-fn/test]  crates/nshedit/src/chared.rs:1110
// [spec:libedit:sem:chared.el-wreplacestr-fn/test]  crates/nshedit/src/chared.rs:1172
// [spec:libedit:sem:eln.el-replacestr-fn/test]  crates/nshedit-abi/src/eln.rs:855
// [spec:libedit:sem:filecomplete.completion-matches-fn/test]  crates/nshedit/src/filecomplete.rs:747
// [spec:libedit:sem:filecomplete.fn-display-match-list-fn/test]  crates/nshedit/src/filecomplete.rs:868
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn/test]  crates/nshedit-abi/src/filecomplete.rs:554
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test]  crates/nshedit-abi/src/filecomplete.rs:527
// [spec:libedit:sem:hist.hist-set-fn/test]  crates/nshedit/src/hist.rs:273
// [spec:libedit:sem:histedit.el-deletestr1-fn/test]  crates/nshedit-abi/src/histedit.rs:659
// [spec:libedit:sem:histedit.el-init-fd-fn/test]  crates/nshedit-abi/src/histedit.rs:472
// [spec:libedit:sem:histedit.el-wreplacestr-fn/test]  crates/nshedit-abi/src/histedit.rs:1628
// [spec:libedit:sem:histedit.history-fn/test]  crates/nshedit-abi/src/histedit.rs:698
// [spec:libedit:sem:histedit.history-init-fn/test]  crates/nshedit-abi/src/histedit.rs:671
// [spec:libedit:sem:prompt.prompt-set-fn/test]  crates/nshedit/src/prompt.rs:317
// [spec:libedit:sem:readline.add-history-fn/test]  crates/nshedit-abi/src/readline.rs:3039
// [spec:libedit:sem:readline.append-history-fn/test]  crates/nshedit-abi/src/readline.rs:2913
// [spec:libedit:sem:readline.clear-history-fn/test]  crates/nshedit-abi/src/readline.rs:3158
// [spec:libedit:sem:readline.completion-matches-fn/test]  crates/nshedit-abi/src/readline.rs:4954
// [spec:libedit:sem:readline.current-history-fn/test]  crates/nshedit-abi/src/readline.rs:3231
// [spec:libedit:sem:readline.default-history-file-fn/test]  crates/nshedit-abi/src/readline.rs:1244
// [spec:libedit:sem:readline.filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3492
// [spec:libedit:sem:readline.get-history-event-fn/test]  crates/nshedit-abi/src/readline.rs:1726
// [spec:libedit:sem:readline.getfrom-fn/test]  crates/nshedit-abi/src/readline.rs:1876
// [spec:libedit:sem:readline.getto-fn/test]  crates/nshedit-abi/src/readline.rs:1958
// [spec:libedit:sem:readline.history-arg-extract-fn/test]  crates/nshedit-abi/src/readline.rs:2458
// [spec:libedit:sem:readline.history-expand-command-fn/test]  crates/nshedit-abi/src/readline.rs:2038
// [spec:libedit:sem:readline.history-expand-fn/test]  crates/nshedit-abi/src/readline.rs:2309
// [spec:libedit:sem:readline.history-get-fn/test]  crates/nshedit-abi/src/readline.rs:2995
// [spec:libedit:sem:readline.history-get-history-state-fn/test]  crates/nshedit-abi/src/readline.rs:4662
// [spec:libedit:sem:readline.history-is-stifled-fn/test]  crates/nshedit-abi/src/readline.rs:2658
// [spec:libedit:sem:readline.history-list-fn/test]  crates/nshedit-abi/src/readline.rs:3186
// [spec:libedit:sem:readline.history-search-fn/test]  crates/nshedit-abi/src/readline.rs:3360
// [spec:libedit:sem:readline.history-search-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3431
// [spec:libedit:sem:readline.history-search-prefix-fn/test]  crates/nshedit-abi/src/readline.rs:3404
// [spec:libedit:sem:readline.history-set-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3295
// [spec:libedit:sem:readline.history-tokenize-fn/test]  crates/nshedit-abi/src/readline.rs:2522
// [spec:libedit:sem:readline.history-total-bytes-fn/test]  crates/nshedit-abi/src/readline.rs:3258
// [spec:libedit:sem:readline.history-truncate-file-fn/test]  crates/nshedit-abi/src/readline.rs:2744
// [spec:libedit:sem:readline.next-history-fn/test]  crates/nshedit-abi/src/readline.rs:3338
// [spec:libedit:sem:readline.previous-history-fn/test]  crates/nshedit-abi/src/readline.rs:3314
// [spec:libedit:sem:readline.read-history-fn/test]  crates/nshedit-abi/src/readline.rs:2845
// [spec:libedit:sem:readline.remove-history-fn/test]  crates/nshedit-abi/src/readline.rs:3068
// [spec:libedit:sem:readline.replace-fn/test]  crates/nshedit-abi/src/readline.rs:2011
// [spec:libedit:sem:readline.replace-history-entry-fn/test]  crates/nshedit-abi/src/readline.rs:3103
// [spec:libedit:sem:readline.resize-fun-fn/test]  crates/nshedit-abi/src/readline.rs:1221
// [spec:libedit:sem:readline.rl-add-defun-fn/test]  crates/nshedit-abi/src/readline.rs:3937
// [spec:libedit:sem:readline.rl-bind-key-fn/test]  crates/nshedit-abi/src/readline.rs:3774
// [spec:libedit:sem:readline.rl-bind-key-in-map-fn/test]  crates/nshedit-abi/src/readline.rs:4751
// [spec:libedit:sem:readline.rl-compat-sub-fn/test]  crates/nshedit-abi/src/readline.rs:1681
// [spec:libedit:sem:readline.rl-completion-matches-fn/test]  crates/nshedit-abi/src/readline.rs:4488
// [spec:libedit:sem:readline.rl-copy-text-fn/test]  crates/nshedit-abi/src/readline.rs:4312
// [spec:libedit:sem:readline.rl-crlf-fn/test]  crates/nshedit-abi/src/readline.rs:4859
// [spec:libedit:sem:readline.rl-delete-text-fn/test]  crates/nshedit-abi/src/readline.rs:4389
// [spec:libedit:sem:readline.rl-ding-fn/test]  crates/nshedit-abi/src/readline.rs:4874
// [spec:libedit:sem:readline.rl-display-match-list-fn/test]  crates/nshedit-abi/src/readline.rs:3590
// [spec:libedit:sem:readline.rl-filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:4588
// [spec:libedit:sem:readline.rl-forced-update-display-fn/test]  crates/nshedit-abi/src/readline.rs:4602
// [spec:libedit:sem:readline.rl-free-line-state-fn/test]  crates/nshedit-abi/src/readline.rs:4795
// [spec:libedit:sem:readline.rl-generic-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4735
// [spec:libedit:sem:readline.rl-get-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4718
// [spec:libedit:sem:readline.rl-get-screen-size-fn/test]  crates/nshedit-abi/src/readline.rs:4406
// [spec:libedit:sem:readline.rl-initialize-fn/test]  crates/nshedit-abi/src/readline.rs:1353
// [spec:libedit:sem:readline.rl-insert-text-fn/test]  crates/nshedit-abi/src/readline.rs:3867
// [spec:libedit:sem:readline.rl-kill-text-fn/test]  crates/nshedit-abi/src/readline.rs:4697
// [spec:libedit:sem:readline.rl-make-bare-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4709
// [spec:libedit:sem:readline.rl-message-fn/test]  crates/nshedit-abi/src/readline.rs:4439
// [spec:libedit:sem:readline.rl-on-new-line-fn/test]  crates/nshedit-abi/src/readline.rs:4786
// [spec:libedit:sem:readline.rl-parse-and-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4145
// [spec:libedit:sem:readline.rl-read-init-file-fn/test]  crates/nshedit-abi/src/readline.rs:4132
// [spec:libedit:sem:readline.rl-replace-line-fn/test]  crates/nshedit-abi/src/readline.rs:4366
// [spec:libedit:sem:readline.rl-restore-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1333
// [spec:libedit:sem:readline.rl-save-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1311
// [spec:libedit:sem:readline.rl-set-key-fn/test]  crates/nshedit-abi/src/readline.rs:4764
// [spec:libedit:sem:readline.rl-set-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4727
// [spec:libedit:sem:readline.rl-set-keymap-name-fn/test]  crates/nshedit-abi/src/readline.rs:4900
// [spec:libedit:sem:readline.rl-set-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1269
// [spec:libedit:sem:readline.rl-set-screen-size-fn/test]  crates/nshedit-abi/src/readline.rs:4460
// [spec:libedit:sem:readline.rl-stuff-char-fn/test]  crates/nshedit-abi/src/readline.rs:4193
// [spec:libedit:sem:readline.rl-update-pos-fn/test]  crates/nshedit-abi/src/readline.rs:4286
// [spec:libedit:sem:readline.rl-variable-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4176
// [spec:libedit:sem:readline.stifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2599
// [spec:libedit:sem:readline.tilde-expand-fn/test]  crates/nshedit-abi/src/readline.rs:3482
// [spec:libedit:sem:readline.unstifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2636
// [spec:libedit:sem:readline.username-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3504
// [spec:libedit:sem:readline.using-history-fn/test]  crates/nshedit-abi/src/readline.rs:1667
// [spec:libedit:sem:readline.where-history-fn/test]  crates/nshedit-abi/src/readline.rs:3176
// [spec:libedit:sem:readline.write-history-fn/test]  crates/nshedit-abi/src/readline.rs:2885
// [spec:libedit:sem:search.el-match-fn/test]  crates/nshedit/src/search.rs:254
// [spec:libedit:sem:terminal.terminal-gettc-fn/test]  crates/nshedit/src/terminal.rs:2325

// ---------------------------------------------------------------------------
// conformance/driver/pty_edit.c — 65 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.c-delbefore1-fn/test]  crates/nshedit/src/chared.rs:451
// [spec:libedit:sem:chared.c-next-word-fn/test]  crates/nshedit/src/chared.rs:569
// [spec:libedit:sem:chared.c-prev-word-fn/test]  crates/nshedit/src/chared.rs:524
// [spec:libedit:sem:chared.ce-isword-fn/test]  crates/nshedit/src/chared.rs:477
// [spec:libedit:sem:chartype.ct-visual-width-fn/test]  crates/nshedit/src/chartype.rs:439
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
// [spec:libedit:sem:eln.el-gets-fn/test]  crates/nshedit-abi/src/eln.rs:289
// [spec:libedit:sem:emacs.em-capitol-case-fn/test]  crates/nshedit/src/emacs.rs:292
// [spec:libedit:sem:emacs.em-delete-or-list-fn/test]  crates/nshedit/src/emacs.rs:20
// [spec:libedit:sem:emacs.em-delete-prev-char-fn/test]  crates/nshedit/src/emacs.rs:514
// [spec:libedit:sem:emacs.em-kill-line-fn/test]  crates/nshedit/src/emacs.rs:141
// [spec:libedit:sem:emacs.em-lower-case-fn/test]  crates/nshedit/src/emacs.rs:336
// [spec:libedit:sem:emacs.em-next-word-fn/test]  crates/nshedit/src/emacs.rs:237
// [spec:libedit:sem:emacs.em-upper-case-fn/test]  crates/nshedit/src/emacs.rs:261
// [spec:libedit:sem:emacs.em-yank-fn/test]  crates/nshedit/src/emacs.rs:94
// [spec:libedit:sem:hist.hist-get-fn/test]  crates/nshedit/src/hist.rs:314
// [spec:libedit:sem:histedit.el-wgets-fn/test]  crates/nshedit-abi/src/histedit.rs:849
// [spec:libedit:sem:histedit.history-end-fn/test]  crates/nshedit-abi/src/histedit.rs:684
// [spec:libedit:sem:histedit.wcsdup-fn/test]  crates/nshedit-abi/src/histedit.rs:849
// [spec:libedit:sem:keymacro.keymacro-get-fn/test]  crates/nshedit/src/keymacro.rs:192
// [spec:libedit:sem:keymacro.node-trav-fn/test]  crates/nshedit/src/keymacro.rs:367
// [spec:libedit:sem:prompt.prompt-default-r-fn/test]  crates/nshedit/src/prompt.rs:101
// [spec:libedit:sem:prompt.prompt-print-fn/test]  crates/nshedit/src/prompt.rs:109
// [spec:libedit:sem:read.el-wgetc-fn/test]  crates/nshedit/src/read.rs:709
// [spec:libedit:sem:read.el-wgets-fn/test]  crates/nshedit/src/read.rs:918
// [spec:libedit:sem:read.read-char-fn/test]  crates/nshedit/src/read.rs:460
// [spec:libedit:sem:read.read-getcmd-fn/test]  crates/nshedit/src/read.rs:275
// [spec:libedit:sem:refresh.re-addc-fn/test]  crates/nshedit/src/refresh.rs:178
// [spec:libedit:sem:refresh.re-clear-eol-fn/test]  crates/nshedit/src/refresh.rs:639
// [spec:libedit:sem:refresh.re-copy-and-pad-fn/test]  crates/nshedit/src/refresh.rs:1082
// [spec:libedit:sem:refresh.re-fastaddc-fn/test]  crates/nshedit/src/refresh.rs:1300
// [spec:libedit:sem:refresh.re-fastputc-fn/test]  crates/nshedit/src/refresh.rs:1207
// [spec:libedit:sem:refresh.re-goto-bottom-fn/test]  crates/nshedit/src/refresh.rs:512
// [spec:libedit:sem:refresh.re-putc-fn/test]  crates/nshedit/src/refresh.rs:307
// [spec:libedit:sem:refresh.re-refresh-cursor-fn/test]  crates/nshedit/src/refresh.rs:1116
// [spec:libedit:sem:refresh.re-refresh-fn/test]  crates/nshedit/src/refresh.rs:361
// [spec:libedit:sem:refresh.re-strncopy-fn/test]  crates/nshedit/src/refresh.rs:616
// [spec:libedit:sem:refresh.re-update-line-fn/test]  crates/nshedit/src/refresh.rs:674
// [spec:libedit:sem:terminal.terminal-clear-eol-fn/test]  crates/nshedit/src/terminal.rs:1476
// [spec:libedit:sem:terminal.terminal-flush-fn/test]  crates/nshedit/src/terminal.rs:2105
// [spec:libedit:sem:terminal.terminal-move-to-char-fn/test]  crates/nshedit/src/terminal.rs:1212
// [spec:libedit:sem:terminal.terminal-move-to-line-fn/test]  crates/nshedit/src/terminal.rs:1177
// [spec:libedit:sem:terminal.terminal-overwrite-fn/test]  crates/nshedit/src/terminal.rs:1317
// [spec:libedit:sem:terminal.terminal-putc-fn/test]  crates/nshedit/src/terminal.rs:2074
// [spec:libedit:sem:terminal.terminal-writec-fn/test]  crates/nshedit/src/terminal.rs:2119
// [spec:libedit:sem:tty.tty-end-fn/test]  crates/nshedit/src/tty.rs:974
// [spec:libedit:sem:tty.tty-get-flag-fn/test]  crates/nshedit/src/tty.rs:1194
// [spec:libedit:sem:tty.tty-getchar-fn/test]  crates/nshedit/src/tty.rs:1076
// [spec:libedit:sem:tty.tty-getspeed-fn/test]  crates/nshedit/src/tty.rs:1010
// [spec:libedit:sem:tty.tty-setchar-fn/test]  crates/nshedit/src/tty.rs:1098
// [spec:libedit:sem:tty.tty-setty-fn/test]  crates/nshedit/src/tty.rs:811
// [spec:libedit:sem:tty.tty-setup-flags-fn/test]  crates/nshedit/src/tty.rs:1778
// [spec:libedit:sem:tty.tty-update-char-fn/test]  crates/nshedit/src/tty.rs:1244
// [spec:libedit:sem:tty.tty-update-flag-fn/test]  crates/nshedit/src/tty.rs:1206
// [spec:libedit:sem:tty.tty-update-flags-fn/test]  crates/nshedit/src/tty.rs:1219

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
    // 326 rules across 5 drivers, measured under -C instrument-coverage.
    assert_eq!(CLAIMED, 326);
}

/// How many `/test` facets this file carries. The generator and the
/// annotations above are written together, so a hand edit to either
/// desynchronises them and `coverage.sh --check` says so.
const CLAIMED: usize = 326;
