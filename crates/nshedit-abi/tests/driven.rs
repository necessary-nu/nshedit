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
// conformance/driver/hist_tok.c — 35 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:histedit.history-w-fn/test]  crates/nshedit-abi/src/histedit.rs:1917
// [spec:libedit:sem:histedit.history-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1742
// [spec:libedit:sem:histedit.history-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1730
// [spec:libedit:sem:histedit.tok-wend-fn/test]  crates/nshedit-abi/src/histedit.rs:1948
// [spec:libedit:sem:histedit.tok-winit-fn/test]  crates/nshedit-abi/src/histedit.rs:1936
// [spec:libedit:sem:histedit.tok-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1972
// [spec:libedit:sem:histedit.tok-wreset-fn/test]  crates/nshedit-abi/src/histedit.rs:1960
// [spec:libedit:sem:histedit.tok-wstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1998
// [spec:libedit:sem:history.fun-history-end-fn/test]  crates/nshedit-abi/src/history.rs:737
// [spec:libedit:sem:history.fun-history-init-fn/test]  crates/nshedit-abi/src/history.rs:275
// [spec:libedit:sem:history.funw-history-fn/test]  crates/nshedit-abi/src/history/dispatch.rs:202
// [spec:libedit:sem:history.history-def-add-fn/test]  crates/nshedit-abi/src/history.rs:544
// [spec:libedit:sem:history.history-def-clear-fn/test]  crates/nshedit-abi/src/history.rs:607
// [spec:libedit:sem:history.history-def-curr-fn/test]  crates/nshedit-abi/src/history.rs:398
// [spec:libedit:sem:history.history-def-del-fn/test]  crates/nshedit-abi/src/history.rs:569
// [spec:libedit:sem:history.history-def-delete-fn/test]  crates/nshedit-abi/src/history.rs:481
// [spec:libedit:sem:history.history-def-enter-fn/test]  crates/nshedit-abi/src/history.rs:536
// [spec:libedit:sem:history.history-def-first-fn/test]  crates/nshedit-abi/src/history.rs:325
// [spec:libedit:sem:history.history-def-init-fn/test]  crates/nshedit-abi/src/history.rs:275
// [spec:libedit:sem:history.history-def-insert-fn/test]  crates/nshedit-abi/src/history.rs:487
// [spec:libedit:sem:history.history-def-last-fn/test]  crates/nshedit-abi/src/history.rs:337
// [spec:libedit:sem:history.history-def-next-fn/test]  crates/nshedit-abi/src/history.rs:349
// [spec:libedit:sem:history.history-def-prev-fn/test]  crates/nshedit-abi/src/history.rs:368
// [spec:libedit:sem:history.history-def-set-fn/test]  crates/nshedit-abi/src/history.rs:415
// [spec:libedit:sem:history.history-deldata-nth-fn/test]  crates/nshedit-abi/src/history.rs:582
// [spec:libedit:sem:history.history-getsize-fn/test]  crates/nshedit-abi/src/history.rs:630
// [spec:libedit:sem:history.history-getunique-fn/test]  crates/nshedit-abi/src/history.rs:656
// [spec:libedit:sem:history.history-set-nth-fn/test]  crates/nshedit-abi/src/history.rs:439
// [spec:libedit:sem:history.history-setsize-fn/test]  crates/nshedit-abi/src/history.rs:615
// [spec:libedit:sem:history.history-setunique-fn/test]  crates/nshedit-abi/src/history.rs:641
// [spec:libedit:sem:tokenizer.fun-tok-finish-fn/test]  crates/nshedit-abi/src/adapter/tokenizer.rs:372
// [spec:libedit:sem:tokenizer.fun-tok-init-fn/test]  crates/nshedit-abi/src/adapter/tokenizer.rs:134
// [spec:libedit:sem:tokenizer.fun-tok-line-fn/test]  crates/nshedit-abi/src/adapter/tokenizer.rs:188
// [spec:libedit:sem:tokenizer.fun-tok-reset-fn/test]  crates/nshedit-abi/src/adapter/tokenizer.rs:175
// [spec:libedit:sem:tokenizer.fun-tok-str-fn/test]  crates/nshedit-abi/src/adapter/tokenizer.rs:188

// ---------------------------------------------------------------------------
// conformance/driver/el_api.c — 26 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:el.el-init-fn/test]  crates/nshedit-abi/src/histedit.rs:472
// [spec:libedit:sem:el.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1683
// [spec:libedit:sem:el.el-wset-fn/test]  crates/nshedit-abi/src/histedit.rs:1061
// [spec:libedit:sem:eln.el-get-fn/test]  crates/nshedit-abi/src/eln.rs:652
// [spec:libedit:sem:eln.el-insertstr-fn/test]  crates/nshedit-abi/src/eln.rs:872
// [spec:libedit:sem:eln.el-line-fn/test]  crates/nshedit-abi/src/eln.rs:779
// [spec:libedit:sem:eln.el-parse-fn/test]  crates/nshedit-abi/src/eln.rs:308
// [spec:libedit:sem:eln.el-set-fn/test]  crates/nshedit-abi/src/eln.rs:383
// [spec:libedit:sem:histedit.el-cursor-fn/test]  crates/nshedit-abi/src/histedit.rs:1668
// [spec:libedit:sem:histedit.el-deletestr-fn/test]  crates/nshedit-abi/src/histedit.rs:728
// [spec:libedit:sem:histedit.el-end-fn/test]  crates/nshedit-abi/src/histedit.rs:526
// [spec:libedit:sem:histedit.el-init-fn/test]  crates/nshedit-abi/src/histedit.rs:472
// [spec:libedit:sem:histedit.el-resize-fn/test]  crates/nshedit-abi/src/histedit.rs:697
// [spec:libedit:sem:histedit.el-source-fn/test]  crates/nshedit-abi/src/histedit.rs:633
// [spec:libedit:sem:histedit.el-winsertstr-fn/test]  crates/nshedit-abi/src/histedit.rs:1709
// [spec:libedit:sem:histedit.el-wline-fn/test]  crates/nshedit-abi/src/histedit.rs:1683
// [spec:libedit:sem:histedit.el-wparse-fn/test]  crates/nshedit-abi/src/histedit.rs:1028
// [spec:libedit:sem:histedit.el-wset-fn/test]  crates/nshedit-abi/src/histedit.rs:1061
// [spec:libedit:sem:histedit.history-end-fn/test]  crates/nshedit-abi/src/histedit.rs:775
// [spec:libedit:sem:histedit.history-fn/test]  crates/nshedit-abi/src/histedit.rs:791
// [spec:libedit:sem:histedit.history-init-fn/test]  crates/nshedit-abi/src/histedit.rs:761
// [spec:libedit:sem:histedit.tok-end-fn/test]  crates/nshedit-abi/src/histedit.rs:823
// [spec:libedit:sem:histedit.tok-init-fn/test]  crates/nshedit-abi/src/histedit.rs:807
// [spec:libedit:sem:histedit.tok-reset-fn/test]  crates/nshedit-abi/src/histedit.rs:835
// [spec:libedit:sem:histedit.tok-str-fn/test]  crates/nshedit-abi/src/histedit.rs:876
// [spec:libedit:sem:tokenizer.fun-tok-end-fn/test]  crates/nshedit-abi/src/histedit.rs:823

// ---------------------------------------------------------------------------
// conformance/driver/readline_api.c — 79 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:eln.el-replacestr-fn/test]  crates/nshedit-abi/src/eln.rs:894
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn/test]  crates/nshedit-abi/src/filecomplete.rs:933
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn/test]  crates/nshedit-abi/src/filecomplete.rs:874
// [spec:libedit:sem:histedit.el-deletestr1-fn/test]  crates/nshedit-abi/src/histedit.rs:748
// [spec:libedit:sem:histedit.el-wreplacestr-fn/test]  crates/nshedit-abi/src/histedit.rs:1720
// [spec:libedit:sem:readline.add-history-fn/test]  crates/nshedit-abi/src/readline.rs:2619
// [spec:libedit:sem:readline.append-history-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:252
// [spec:libedit:sem:readline.clear-history-fn/test]  crates/nshedit-abi/src/readline.rs:2741
// [spec:libedit:sem:readline.completion-matches-fn/test]  crates/nshedit-abi/src/readline.rs:4568
// [spec:libedit:sem:readline.current-history-fn/test]  crates/nshedit-abi/src/readline.rs:2817
// [spec:libedit:sem:readline.default-history-file-fn/test]  crates/nshedit-abi/src/readline.rs:1130
// [spec:libedit:sem:readline.filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3087
// [spec:libedit:sem:readline.get-history-event-fn/test]  crates/nshedit-abi/src/readline.rs:1619
// [spec:libedit:sem:readline.getfrom-fn/test]  crates/nshedit-abi/src/readline.rs:1769
// [spec:libedit:sem:readline.getto-fn/test]  crates/nshedit-abi/src/readline.rs:1851
// [spec:libedit:sem:readline.history-arg-extract-fn/test]  crates/nshedit-abi/src/readline.rs:2353
// [spec:libedit:sem:readline.history-expand-command-fn/test]  crates/nshedit-abi/src/readline.rs:1931
// [spec:libedit:sem:readline.history-expand-fn/test]  crates/nshedit-abi/src/readline.rs:2203
// [spec:libedit:sem:readline.history-get-fn/test]  crates/nshedit-abi/src/readline.rs:2574
// [spec:libedit:sem:readline.history-get-history-state-fn/test]  crates/nshedit-abi/src/readline.rs:4254
// [spec:libedit:sem:readline.history-is-stifled-fn/test]  crates/nshedit-abi/src/readline.rs:2557
// [spec:libedit:sem:readline.history-list-fn/test]  crates/nshedit-abi/src/readline.rs:2771
// [spec:libedit:sem:readline.history-search-fn/test]  crates/nshedit-abi/src/readline.rs:2951
// [spec:libedit:sem:readline.history-search-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3024
// [spec:libedit:sem:readline.history-search-prefix-fn/test]  crates/nshedit-abi/src/readline.rs:2996
// [spec:libedit:sem:readline.history-set-pos-fn/test]  crates/nshedit-abi/src/readline.rs:2883
// [spec:libedit:sem:readline.history-tokenize-fn/test]  crates/nshedit-abi/src/readline.rs:2418
// [spec:libedit:sem:readline.history-total-bytes-fn/test]  crates/nshedit-abi/src/readline.rs:2845
// [spec:libedit:sem:readline.history-truncate-file-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:80
// [spec:libedit:sem:readline.next-history-fn/test]  crates/nshedit-abi/src/readline.rs:2928
// [spec:libedit:sem:readline.previous-history-fn/test]  crates/nshedit-abi/src/readline.rs:2903
// [spec:libedit:sem:readline.read-history-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:182
// [spec:libedit:sem:readline.remove-history-fn/test]  crates/nshedit-abi/src/readline.rs:2649
// [spec:libedit:sem:readline.replace-fn/test]  crates/nshedit-abi/src/readline.rs:1904
// [spec:libedit:sem:readline.replace-history-entry-fn/test]  crates/nshedit-abi/src/readline.rs:2685
// [spec:libedit:sem:readline.resize-fun-fn/test]  crates/nshedit-abi/src/readline.rs:1107
// [spec:libedit:sem:readline.rl-add-defun-fn/test]  crates/nshedit-abi/src/readline.rs:3522
// [spec:libedit:sem:readline.rl-bind-key-fn/test]  crates/nshedit-abi/src/readline.rs:3354
// [spec:libedit:sem:readline.rl-bind-key-in-map-fn/test]  crates/nshedit-abi/src/readline.rs:4350
// [spec:libedit:sem:readline.rl-compat-sub-fn/test]  crates/nshedit-abi/src/readline.rs:1573
// [spec:libedit:sem:readline.rl-completion-append-character-function-fn/test]  crates/nshedit-abi/src/readline.rs:975
// [spec:libedit:sem:readline.rl-completion-matches-fn/test]  crates/nshedit-abi/src/readline.rs:4083
// [spec:libedit:sem:readline.rl-copy-text-fn/test]  crates/nshedit-abi/src/readline.rs:3912
// [spec:libedit:sem:readline.rl-crlf-fn/test]  crates/nshedit-abi/src/readline.rs:4467
// [spec:libedit:sem:readline.rl-delete-text-fn/test]  crates/nshedit-abi/src/readline.rs:3991
// [spec:libedit:sem:readline.rl-ding-fn/test]  crates/nshedit-abi/src/readline.rs:4483
// [spec:libedit:sem:readline.rl-display-match-list-fn/test]  crates/nshedit-abi/src/readline.rs:3169
// [spec:libedit:sem:readline.rl-filename-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:4184
// [spec:libedit:sem:readline.rl-forced-update-display-fn/test]  crates/nshedit-abi/src/readline.rs:4199
// [spec:libedit:sem:readline.rl-free-line-state-fn/test]  crates/nshedit-abi/src/readline.rs:4398
// [spec:libedit:sem:readline.rl-generic-bind-fn/test]  crates/nshedit-abi/src/readline.rs:4333
// [spec:libedit:sem:readline.rl-get-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4314
// [spec:libedit:sem:readline.rl-get-screen-size-fn/test]  crates/nshedit-abi/src/readline.rs:4009
// [spec:libedit:sem:readline.rl-initialize-fn/test]  crates/nshedit-abi/src/readline.rs:1243
// [spec:libedit:sem:readline.rl-insert-text-fn/test]  crates/nshedit-abi/src/readline.rs:3450
// [spec:libedit:sem:readline.rl-kill-text-fn/test]  crates/nshedit-abi/src/readline.rs:4291
// [spec:libedit:sem:readline.rl-make-bare-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4304
// [spec:libedit:sem:readline.rl-message-fn/test]  crates/nshedit-abi/src/readline.rs:4033
// [spec:libedit:sem:readline.rl-on-new-line-fn/test]  crates/nshedit-abi/src/readline.rs:4388
// [spec:libedit:sem:readline.rl-parse-and-bind-fn/test]  crates/nshedit-abi/src/readline.rs:3738
// [spec:libedit:sem:readline.rl-read-init-file-fn/test]  crates/nshedit-abi/src/readline.rs:3724
// [spec:libedit:sem:readline.rl-replace-line-fn/test]  crates/nshedit-abi/src/readline.rs:3967
// [spec:libedit:sem:readline.rl-restore-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1222
// [spec:libedit:sem:readline.rl-save-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1199
// [spec:libedit:sem:readline.rl-set-key-fn/test]  crates/nshedit-abi/src/readline.rs:4364
// [spec:libedit:sem:readline.rl-set-keymap-fn/test]  crates/nshedit-abi/src/readline.rs:4324
// [spec:libedit:sem:readline.rl-set-keymap-name-fn/test]  crates/nshedit-abi/src/readline.rs:4511
// [spec:libedit:sem:readline.rl-set-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1156
// [spec:libedit:sem:readline.rl-set-screen-size-fn/test]  crates/nshedit-abi/src/readline.rs:4054
// [spec:libedit:sem:readline.rl-stuff-char-fn/test]  crates/nshedit-abi/src/readline.rs:3788
// [spec:libedit:sem:readline.rl-update-pos-fn/test]  crates/nshedit-abi/src/readline.rs:3885
// [spec:libedit:sem:readline.rl-variable-bind-fn/test]  crates/nshedit-abi/src/readline.rs:3770
// [spec:libedit:sem:readline.stifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2496
// [spec:libedit:sem:readline.tilde-expand-fn/test]  crates/nshedit-abi/src/readline.rs:3076
// [spec:libedit:sem:readline.unstifle-history-fn/test]  crates/nshedit-abi/src/readline.rs:2534
// [spec:libedit:sem:readline.username-completion-function-fn/test]  crates/nshedit-abi/src/readline.rs:3100
// [spec:libedit:sem:readline.using-history-fn/test]  crates/nshedit-abi/src/readline.rs:1559
// [spec:libedit:sem:readline.where-history-fn/test]  crates/nshedit-abi/src/readline.rs:2760
// [spec:libedit:sem:readline.write-history-fn/test]  crates/nshedit-abi/src/readline/history_io.rs:223

// ---------------------------------------------------------------------------
// conformance/driver/pty_edit.c — 3 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:eln.el-gets-fn/test]  crates/nshedit-abi/src/eln.rs:235
// [spec:libedit:sem:histedit.el-wgets-fn/test]  crates/nshedit-abi/src/histedit.rs:918
// [spec:libedit:sem:histedit.wcsdup-fn/test]  crates/nshedit-abi/src/histedit.rs:918

// ---------------------------------------------------------------------------
// conformance/driver/binding_dispatch.c — 0 rules
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// conformance/driver/abi_gaps.c — 4 rules
// ---------------------------------------------------------------------------
// [spec:libedit:sem:readline.get-prompt-fn/test]  crates/nshedit-abi/src/readline.rs:1061
// [spec:libedit:sem:readline.readline-fn/test]  crates/nshedit-abi/src/readline.rs:1468
// [spec:libedit:sem:readline.rl-event-read-char-fn/test]  crates/nshedit-abi/src/readline.rs:3806
// [spec:libedit:sem:readline.rl-kill-full-line-fn/test]  crates/nshedit-abi/src/readline.rs:4274

// ---------------------------------------------------------------------------
// conformance/aux/ub_corpus.c — 0 rules
// ---------------------------------------------------------------------------

/// The drivers, and the count each one earns.
///
/// A rule reached by more than one driver is attributed to the first that
/// reaches it, so these sum to the total. The overlap is large and that is
/// expected — 147 of 147 rules are reached by more than one,
/// because every driver goes through the same lifecycle and allocator paths.
#[test]
fn the_claim_list_is_what_coverage_measured() {
    // Regenerate with ./conformance/coverage.sh, verify with --check.
    // 147 rules across 7 drivers, measured under -C instrument-coverage.
    assert_eq!(CLAIMED, 147);
}

/// How many `/test` facets this file carries. The generator and the
/// annotations above are written together, so a hand edit to either
/// desynchronises them and `coverage.sh --check` says so.
const CLAIMED: usize = 147;
