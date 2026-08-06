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
// conformance/driver/hist_tok.c — 49 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chartype.ct-conv-cbuff-resize-fn/test]  crates/nshedit/src/chartype.rs:104
// [spec:libedit:sem:chartype.ct-conv-wbuff-resize-fn/test]  crates/nshedit/src/chartype.rs:138
// [spec:libedit:sem:chartype.ct-decode-string-fn/test]  crates/nshedit/src/chartype.rs:227
// [spec:libedit:sem:chartype.ct-enc-width-fn/test]  crates/nshedit/src/chartype.rs:325
// [spec:libedit:sem:chartype.ct-encode-char-fn/test]  crates/nshedit/src/chartype.rs:342
// [spec:libedit:sem:chartype.ct-encode-string-fn/test]  crates/nshedit/src/chartype.rs:171
// [spec:libedit:sem:histedit.history-w-fn/test]  crates/nshedit-abi/src/histedit.rs:1837
// [spec:libedit:sem:histedit.history-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1648
// [spec:libedit:sem:histedit.history-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1637
// [spec:libedit:sem:histedit.tok-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1865
// [spec:libedit:sem:histedit.tok-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1855
// [spec:libedit:sem:histedit.tok-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1929
// [spec:libedit:sem:histedit.tok-wreset-fn/test]  crates/nshedit-abi/src/histedit.rs:1877
// [spec:libedit:sem:histedit.tok-wstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1974
// [spec:libedit:sem:history.fun-history-end-fn/test]  crates/nshedit/src/history.rs:1414
// [spec:libedit:sem:history.fun-history-init-fn/test]  crates/nshedit/src/history.rs:1361
// [spec:libedit:sem:history.funw-history-fn/test]  crates/nshedit/src/history.rs:2234
// [spec:libedit:sem:history.history-def-add-fn/test]  crates/nshedit/src/history.rs:921
// [spec:libedit:sem:history.history-def-clear-fn/test]  crates/nshedit/src/history.rs:1321
// [spec:libedit:sem:history.history-def-curr-fn/test]  crates/nshedit/src/history.rs:803
// [spec:libedit:sem:history.history-def-del-fn/test]  crates/nshedit/src/history.rs:1021
// [spec:libedit:sem:history.history-def-enter-fn/test]  crates/nshedit/src/history.rs:1202
// [spec:libedit:sem:history.history-def-first-fn/test]  crates/nshedit/src/history.rs:654
// [spec:libedit:sem:history.history-def-init-fn/test]  crates/nshedit/src/history.rs:1267
// [spec:libedit:sem:history.history-def-last-fn/test]  crates/nshedit/src/history.rs:689
// [spec:libedit:sem:history.history-def-next-fn/test]  crates/nshedit/src/history.rs:720
// [spec:libedit:sem:history.history-def-prev-fn/test]  crates/nshedit/src/history.rs:759
// [spec:libedit:sem:history.history-def-set-fn/test]  crates/nshedit/src/history.rs:841
// [spec:libedit:sem:history.history-deldata-nth-fn/test]  crates/nshedit/src/history.rs:977
// [spec:libedit:sem:history.history-getsize-fn/test]  crates/nshedit/src/history.rs:1472
// [spec:libedit:sem:history.history-getunique-fn/test]  crates/nshedit/src/history.rs:1517
// [spec:libedit:sem:history.history-load-fn/test]  crates/nshedit/src/history.rs:1628
// [spec:libedit:sem:history.history-next-evdata-fn/test]  crates/nshedit/src/history.rs:2070
// [spec:libedit:sem:history.history-next-event-fn/test]  crates/nshedit/src/history.rs:2111
// [spec:libedit:sem:history.history-next-string-fn/test]  crates/nshedit/src/history.rs:2166
// [spec:libedit:sem:history.history-prev-event-fn/test]  crates/nshedit/src/history.rs:2046
// [spec:libedit:sem:history.history-prev-string-fn/test]  crates/nshedit/src/history.rs:2133
// [spec:libedit:sem:history.history-save-fn/test]  crates/nshedit/src/history.rs:2008
// [spec:libedit:sem:history.history-save-fp-fn/test]  crates/nshedit/src/history.rs:1837
// [spec:libedit:sem:history.history-set-fun-fn/test]  crates/nshedit/src/history.rs:1536
// [spec:libedit:sem:history.history-set-nth-fn/test]  crates/nshedit/src/history.rs:882
// [spec:libedit:sem:history.history-setsize-fn/test]  crates/nshedit/src/history.rs:1450
// [spec:libedit:sem:history.history-setunique-fn/test]  crates/nshedit/src/history.rs:1490
// [spec:libedit:sem:tokenizer.fun-tok-end-fn/test]  crates/nshedit/src/tokenizer.rs:250
// [spec:libedit:sem:tokenizer.fun-tok-finish-fn/test]  crates/nshedit/src/tokenizer.rs:174
// [spec:libedit:sem:tokenizer.fun-tok-init-fn/test]  crates/nshedit/src/tokenizer.rs:202
// [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]  crates/nshedit/src/tokenizer.rs:274
// [spec:libedit:sem:tokenizer.fun-tok-reset-fn/test]  crates/nshedit/src/tokenizer.rs:233
// [spec:libedit:sem:tokenizer.fun-tok-str-fn/test]  crates/nshedit/src/tokenizer.rs:530

// ---------------------------------------------------------------------------
// conformance/driver/el_api.c — 121 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:chared.c-delbefore-fn/test]  crates/nshedit/src/chared.rs:407
// [spec:libedit:sem:chared.c-insert-fn/test]  crates/nshedit/src/chared.rs:278
// [spec:libedit:sem:chared.ch-end-fn/test]  crates/nshedit/src/chared.rs:995
// [spec:libedit:sem:chared.ch-init-fn/test]  crates/nshedit/src/chared.rs:814
// [spec:libedit:sem:chared.ch-reset-fn/test]  crates/nshedit/src/chared.rs:876
// [spec:libedit:sem:chared.cv-undo-fn/test]  crates/nshedit/src/chared.rs:209
// [spec:libedit:sem:chared.cv-yank-fn/test]  crates/nshedit/src/chared.rs:250
// [spec:libedit:sem:chared.el-cursor-fn/test]  crates/nshedit/src/chared.rs:1188
// [spec:libedit:sem:chared.el-deletestr-fn/test]  crates/nshedit/src/chared.rs:1063
// [spec:libedit:sem:chared.el-winsertstr-fn/test]  crates/nshedit/src/chared.rs:1026
// [spec:libedit:sem:chartype.ct-chr-class-fn/test]  crates/nshedit/src/chartype.rs:547
// [spec:libedit:sem:chartype.ct-visual-char-fn/test]  crates/nshedit/src/chartype.rs:470
// [spec:libedit:sem:chartype.ct-visual-string-fn/test]  crates/nshedit/src/chartype.rs:377
// [spec:libedit:sem:el.editline.el-getenv-fn/test]  crates/nshedit/src/el.rs:446
// [spec:libedit:sem:el.el-editmode-fn/test]  crates/nshedit/src/el.rs:1294
// [spec:libedit:sem:el.el-end-fn/test]  crates/nshedit/src/el.rs:933
// [spec:libedit:sem:el.el-init-fd-fn/test]  crates/nshedit/src/el.rs:900
// [spec:libedit:sem:el.el-init-internal-fn/test]  crates/nshedit/src/el.rs:547
// [spec:libedit:sem:el.el-reset-fn/test]  crates/nshedit/src/el.rs:1022
// [spec:libedit:sem:el.el-resize-fn/test]  crates/nshedit/src/el.rs:1217
// [spec:libedit:sem:el.el-source-fn/test]  crates/nshedit/src/el.rs:1053
// [spec:libedit:sem:el.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1574
// [spec:libedit:sem:el.secure-getenv-fn/test]  crates/nshedit/src/el.rs:304
// [spec:libedit:sem:eln.el-get-fn/test]  crates/nshedit-abi/src/eln.rs:520
// [spec:libedit:sem:eln.el-insertstr-fn/test]  crates/nshedit-abi/src/eln.rs:698
// [spec:libedit:sem:eln.el-line-fn/test]  crates/nshedit-abi/src/eln.rs:605
// [spec:libedit:sem:eln.el-parse-fn/test]  crates/nshedit-abi/src/eln.rs:357
// [spec:libedit:sem:eln.el-set-fn/test]  crates/nshedit-abi/src/eln.rs:432
// [spec:libedit:sem:hist.hist-command-fn/test]  crates/nshedit/src/hist.rs:272
// [spec:libedit:sem:hist.hist-end-fn/test]  crates/nshedit/src/hist.rs:96
// [spec:libedit:sem:hist.hist-init-fn/test]  crates/nshedit/src/hist.rs:69
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
// [spec:libedit:sem:keymacro.keymacro-add-fn/test]  crates/nshedit/src/keymacro.rs:209
// [spec:libedit:sem:keymacro.keymacro-clear-fn/test]  crates/nshedit/src/keymacro.rs:252
// [spec:libedit:sem:keymacro.keymacro-decode-str-fn/test]  crates/nshedit/src/keymacro.rs:971
// [spec:libedit:sem:keymacro.keymacro-end-fn/test]  crates/nshedit/src/keymacro.rs:117
// [spec:libedit:sem:keymacro.keymacro-init-fn/test]  crates/nshedit/src/keymacro.rs:85
// [spec:libedit:sem:keymacro.keymacro-kprint-fn/test]  crates/nshedit/src/keymacro.rs:887
// [spec:libedit:sem:keymacro.keymacro-map-cmd-fn/test]  crates/nshedit/src/keymacro.rs:144
// [spec:libedit:sem:keymacro.keymacro-map-str-fn/test]  crates/nshedit/src/keymacro.rs:161
// [spec:libedit:sem:keymacro.keymacro-print-fn/test]  crates/nshedit/src/keymacro.rs:328
// [spec:libedit:sem:keymacro.keymacro-reset-fn/test]  crates/nshedit/src/keymacro.rs:176
// [spec:libedit:sem:keymacro.node-enum-fn/test]  crates/nshedit/src/keymacro.rs:812
// [spec:libedit:sem:keymacro.node-free-fn/test]  crates/nshedit/src/keymacro.rs:706
// [spec:libedit:sem:keymacro.node-get-fn/test]  crates/nshedit/src/keymacro.rs:681
// [spec:libedit:sem:keymacro.node-lookup-fn/test]  crates/nshedit/src/keymacro.rs:729
// [spec:libedit:sem:keymacro.node-put-fn/test]  crates/nshedit/src/keymacro.rs:640
// [spec:libedit:sem:keymacro.node-try-fn/test]  crates/nshedit/src/keymacro.rs:431
// [spec:libedit:sem:literal.literal-clear-fn/test]  crates/nshedit/src/literal.rs:98
// [spec:libedit:sem:literal.literal-end-fn/test]  crates/nshedit/src/literal.rs:83
// [spec:libedit:sem:literal.literal-init-fn/test]  crates/nshedit/src/literal.rs:60
// [spec:libedit:sem:map.map-bind-fn/test]  crates/nshedit/src/map.rs:1477
// [spec:libedit:sem:map.map-end-fn/test]  crates/nshedit/src/map.rs:1063
// [spec:libedit:sem:map.map-init-emacs-fn/test]  crates/nshedit/src/map.rs:1215
// [spec:libedit:sem:map.map-init-fn/test]  crates/nshedit/src/map.rs:996
// [spec:libedit:sem:map.map-init-meta-fn/test]  crates/nshedit/src/map.rs:1118
// [spec:libedit:sem:map.map-init-nls-fn/test]  crates/nshedit/src/map.rs:1091
// [spec:libedit:sem:map.map-init-vi-fn/test]  crates/nshedit/src/map.rs:1169
// [spec:libedit:sem:map.map-print-all-keys-fn/test]  crates/nshedit/src/map.rs:1438
// [spec:libedit:sem:map.map-print-key-fn/test]  crates/nshedit/src/map.rs:1337
// [spec:libedit:sem:map.map-print-some-keys-fn/test]  crates/nshedit/src/map.rs:1379
// [spec:libedit:sem:map.map-set-editor-fn/test]  crates/nshedit/src/map.rs:1265
// [spec:libedit:sem:parse.el-wparse-fn/test]  crates/nshedit/src/parse.rs:236
// [spec:libedit:sem:parse.parse-cmd-fn/test]  crates/nshedit/src/parse.rs:502
// [spec:libedit:sem:parse.parse-escape-fn/test]  crates/nshedit/src/parse.rs:321
// [spec:libedit:sem:parse.parse-line-fn/test]  crates/nshedit/src/parse.rs:179
// [spec:libedit:sem:parse.parse-string-fn/test]  crates/nshedit/src/parse.rs:452
// [spec:libedit:sem:prompt.prompt-end-fn/test]  crates/nshedit/src/prompt.rs:281
// [spec:libedit:sem:prompt.prompt-init-fn/test]  crates/nshedit/src/prompt.rs:260
// [spec:libedit:sem:read.read-clearmacros-fn/test]  crates/nshedit/src/read.rs:703
// [spec:libedit:sem:read.read-end-fn/test]  crates/nshedit/src/read.rs:155
// [spec:libedit:sem:read.read-finish-fn/test]  crates/nshedit/src/read.rs:852
// [spec:libedit:sem:read.read-init-fn/test]  crates/nshedit/src/read.rs:117
// [spec:libedit:sem:read.read-prepare-fn/test]  crates/nshedit/src/read.rs:812
// [spec:libedit:sem:refresh.re-clear-display-fn/test]  crates/nshedit/src/refresh.rs:1333
// [spec:libedit:sem:search.search-end-fn/test]  crates/nshedit/src/search.rs:223
// [spec:libedit:sem:search.search-init-fn/test]  crates/nshedit/src/search.rs:204
// [spec:libedit:sem:sig.sig-clr-fn/test]  crates/nshedit/src/sig.rs:411
// [spec:libedit:sem:sig.sig-end-fn/test]  crates/nshedit/src/sig.rs:299
// [spec:libedit:sem:sig.sig-init-fn/test]  crates/nshedit/src/sig.rs:262
// [spec:libedit:sem:terminal.terminal-alloc-buffer-fn/test]  crates/nshedit/src/terminal.rs:1134
// [spec:libedit:sem:terminal.terminal-alloc-display-fn/test]  crates/nshedit/src/terminal.rs:1163
// [spec:libedit:sem:terminal.terminal-bind-arrow-fn/test]  crates/nshedit/src/terminal.rs:1942
// [spec:libedit:sem:terminal.terminal-change-size-fn/test]  crates/nshedit/src/terminal.rs:1734
// [spec:libedit:sem:terminal.terminal-echotc-fn/test]  crates/nshedit/src/terminal.rs:2396
// [spec:libedit:sem:terminal.terminal-end-fn/test]  crates/nshedit/src/terminal.rs:1045
// [spec:libedit:sem:terminal.terminal-free-buffer-fn/test]  crates/nshedit/src/terminal.rs:1154
// [spec:libedit:sem:terminal.terminal-free-display-fn/test]  crates/nshedit/src/terminal.rs:1183
// [spec:libedit:sem:terminal.terminal-get-fn/test]  crates/nshedit/src/terminal.rs:1540
// [spec:libedit:sem:terminal.terminal-get-size-fn/test]  crates/nshedit/src/terminal.rs:1707
// [spec:libedit:sem:terminal.terminal-init-arrow-fn/test]  crates/nshedit/src/terminal.rs:1779
// [spec:libedit:sem:terminal.terminal-init-fn/test]  crates/nshedit/src/terminal.rs:999
// [spec:libedit:sem:terminal.terminal-print-arrow-fn/test]  crates/nshedit/src/terminal.rs:1922
// [spec:libedit:sem:terminal.terminal-rebuffer-display-fn/test]  crates/nshedit/src/terminal.rs:1110
// [spec:libedit:sem:terminal.terminal-reset-arrow-fn/test]  crates/nshedit/src/terminal.rs:1830
// [spec:libedit:sem:terminal.terminal-set-fn/test]  crates/nshedit/src/terminal.rs:1548
// [spec:libedit:sem:terminal.terminal-setflags-fn/test]  crates/nshedit/src/terminal.rs:942
// [spec:libedit:sem:terminal.terminal-settc-fn/test]  crates/nshedit/src/terminal.rs:2221
// [spec:libedit:sem:terminal.terminal-telltc-fn/test]  crates/nshedit/src/terminal.rs:2152
// [spec:libedit:sem:terminal.tgetent-fn/test]  crates/nshedit/src/terminal.rs:619
// [spec:libedit:sem:terminal.tgetflag-fn/test]  crates/nshedit/src/terminal.rs:652
// [spec:libedit:sem:terminal.tgetnum-fn/test]  crates/nshedit/src/terminal.rs:664
// [spec:libedit:sem:terminal.tgetstr-fn/test]  crates/nshedit/src/terminal.rs:686
// [spec:libedit:sem:tty.tty-bind-char-fn/test]  crates/nshedit/src/tty.rs:1153
// [spec:libedit:sem:tty.tty-cookedmode-fn/test]  crates/nshedit/src/tty.rs:1409
// [spec:libedit:sem:tty.tty-getty-fn/test]  crates/nshedit/src/tty.rs:806
// [spec:libedit:sem:tty.tty-init-fn/test]  crates/nshedit/src/tty.rs:988
// [spec:libedit:sem:tty.tty-rawmode-fn/test]  crates/nshedit/src/tty.rs:1308
// [spec:libedit:sem:tty.tty-setup-fn/test]  crates/nshedit/src/tty.rs:868

// ---------------------------------------------------------------------------
// conformance/driver/readline_api.c — 49 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn/test]  crates/nshedit-abi/src/filecomplete.rs:554
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test]  crates/nshedit-abi/src/filecomplete.rs:527
// [spec:libedit:sem:histedit.el-init-fd-fn/test]  crates/nshedit-abi/src/histedit.rs:472
// [spec:libedit:sem:histedit.history-fn/test]  crates/nshedit-abi/src/histedit.rs:698
// [spec:libedit:sem:histedit.history-init-fn/test]  crates/nshedit-abi/src/histedit.rs:671
// [spec:libedit:sem:readline.add-history-fn/test]  crates/nshedit-abi/src/readline.rs:3052
// [spec:libedit:sem:readline.append-history-fn/test]  crates/nshedit-abi/src/readline.rs:2917
// [spec:libedit:sem:readline.clear-history-fn/test]  crates/nshedit-abi/src/readline.rs:3180
// [spec:libedit:sem:readline.current-history-fn/test]  crates/nshedit-abi/src/readline.rs:3259
// [spec:libedit:sem:readline.default-history-file-fn/test]  crates/nshedit-abi/src/readline.rs:1217
// [spec:libedit:sem:readline.filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3541
// [spec:libedit:sem:readline.get-history-event-fn/test]  crates/nshedit-abi/src/readline.rs:1705
// [spec:libedit:sem:readline.getfrom-fn/test]  crates/nshedit-abi/src/readline.rs:1858
// [spec:libedit:sem:readline.getto-fn/test]  crates/nshedit-abi/src/readline.rs:1940
// [spec:libedit:sem:readline.history-arg-extract-fn/test]  crates/nshedit-abi/src/readline.rs:2440
// [spec:libedit:sem:readline.history-expand-command-fn/test]  crates/nshedit-abi/src/readline.rs:2020
// [spec:libedit:sem:readline.history-expand-fn/test]  crates/nshedit-abi/src/readline.rs:2291
// [spec:libedit:sem:readline.history-get-fn/test]  crates/nshedit-abi/src/readline.rs:3005
// [spec:libedit:sem:readline.history-get-history-state-fn/test]  crates/nshedit-abi/src/readline.rs:4715
// [spec:libedit:sem:readline.history-is-stifled-fn/test]  crates/nshedit-abi/src/readline.rs:2655
// [spec:libedit:sem:readline.history-list-fn/test]  crates/nshedit-abi/src/readline.rs:3211
// [spec:libedit:sem:readline.history-search-fn/test]  crates/nshedit-abi/src/readline.rs:3400
// [spec:libedit:sem:readline.history-search-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3477
// [spec:libedit:sem:readline.history-search-prefix-fn/test]  crates/nshedit-abi/src/readline.rs:3447
// [spec:libedit:sem:readline.history-set-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3329
// [spec:libedit:sem:readline.history-tokenize-fn/test]  crates/nshedit-abi/src/readline.rs:2509
// [spec:libedit:sem:readline.history-total-bytes-fn/test]  crates/nshedit-abi/src/readline.rs:3289
// [spec:libedit:sem:readline.history-truncate-file-fn/test]  crates/nshedit-abi/src/readline.rs:2732
// [spec:libedit:sem:readline.next-history-fn/test]  crates/nshedit-abi/src/readline.rs:3375
// [spec:libedit:sem:readline.previous-history-fn/test]  crates/nshedit-abi/src/readline.rs:3348
// [spec:libedit:sem:readline.read-history-fn/test]  crates/nshedit-abi/src/readline.rs:2833
// [spec:libedit:sem:readline.remove-history-fn/test]  crates/nshedit-abi/src/readline.rs:3084
// [spec:libedit:sem:readline.replace-fn/test]  crates/nshedit-abi/src/readline.rs:1993
// [spec:libedit:sem:readline.replace-history-entry-fn/test]  crates/nshedit-abi/src/readline.rs:3122
// [spec:libedit:sem:readline.resize-fun-fn/test]  crates/nshedit-abi/src/readline.rs:1194
// [spec:libedit:sem:readline.rl-compat-sub-fn/test]  crates/nshedit-abi/src/readline.rs:1660
// [spec:libedit:sem:readline.rl-initialize-fn/test]  crates/nshedit-abi/src/readline.rs:1326
// [spec:libedit:sem:readline.rl-parse-and-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4198
// [spec:libedit:sem:readline.rl-read-init-file-fn/test]  crates/nshedit-abi/src/readline.rs:4185
// [spec:libedit:sem:readline.rl-set-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1242
// [spec:libedit:sem:readline.rl-update-pos-fn/test]  crates/nshedit-abi/src/readline.rs:4339
// [spec:libedit:sem:readline.rl-variable-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4229
// [spec:libedit:sem:readline.stifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2590
// [spec:libedit:sem:readline.tilde-expand-fn/test]  crates/nshedit-abi/src/readline.rs:3531
// [spec:libedit:sem:readline.unstifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2630
// [spec:libedit:sem:readline.using-history-fn/test]  crates/nshedit-abi/src/readline.rs:1646
// [spec:libedit:sem:readline.where-history-fn/test]  crates/nshedit-abi/src/readline.rs:3201
// [spec:libedit:sem:readline.write-history-fn/test]  crates/nshedit-abi/src/readline.rs:2881
// [spec:libedit:sem:search.el-match-fn/test]  crates/nshedit/src/search.rs:254

// ---------------------------------------------------------------------------
// conformance/aux/ub_corpus.c — 1 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:histedit.history-end-fn/test]  crates/nshedit-abi/src/histedit.rs:684

/// The drivers, and the count each one earns.
///
/// A rule reached by more than one driver is attributed to the first that
/// reaches it, so these sum to the total. The overlap is large and that is
/// expected — 220 of 220 rules are reached by more than one,
/// because every driver goes through the same lifecycle and allocator paths.
#[test]
fn the_claim_list_is_what_coverage_measured() {
    // Regenerate with ./conformance/coverage.sh, verify with --check.
    // 220 rules across 4 drivers, measured under -C instrument-coverage.
    assert_eq!(CLAIMED, 220);
}

/// How many `/test` facets this file carries. The generator and the
/// annotations above are written together, so a hand edit to either
/// desynchronises them and `coverage.sh --check` says so.
const CLAIMED: usize = 220;
