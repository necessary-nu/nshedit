//! Readline completion exports and their typed-provider adaptation.

use super::*;

/// Copy readline's completion suffix into an owned Rust value.
///
/// `rl_completion_append_character == 0` means append nothing. A value outside
/// ASCII also becomes empty because it has no one-byte UTF-8 representation;
/// the C would append one invalid byte.
// [spec:libedit:def:readline.rl-completion-append-character-function-fn]
// [spec:libedit:sem:readline.rl-completion-append-character-function-fn]
pub(super) fn readline_completion_suffix(_name: &str) -> String {
    // SAFETY: a plain read of a module static.
    let c = unsafe { rl_completion_append_character };
    if (1..128).contains(&c) {
        char::from(c as u8).to_string()
    } else {
        String::new()
    }
}

pub(super) unsafe fn display_match_list(matches: *mut *mut c_char, len: c_int, max: c_int) {
    // SAFETY: `matches` is the caller's readline-shaped array: index 0 is the
    // common prefix and 1..=len are the entries shown. Nothing here is freed
    // and ownership stays with the caller.
    unsafe {
        // No lazy-initialization guard in the C, so `e` must already exist.
        if E.is_null() || matches.is_null() {
            return;
        }
        // The C widens both counts to `size_t` with no range check, so a
        // negative argument becomes an enormous count and the display walks
        // off the array (UB). Defined here as doing nothing.
        if len < 0 || max < 0 {
            return;
        }
        // ERR-completion-02's disposition is "treat it as a caller error and
        // reject it", and `len` is the caller's claim about an array only the
        // caller can measure. What actually bounds it is the NULL terminator
        // the readline contract puts there — `rl_completion_matches` and
        // `completion_matches` both produce one — so the walk stops at the
        // first NULL rather than trusting the count.
        //
        // Without this, `rl_display_match_list(matches, 99, 6)` over a
        // two-element array read 100 pointers and died. Measured by
        // `conformance/ub.sh`, where the C dies on it too; this is the half
        // the erratum says must not.
        //
        // The core takes owned strings and sorts them in place, so the
        // caller's array is permuted afterwards to match — which is what the
        // C's in-place `qsort` leaves behind.
        let claimed = len as usize;
        let mut owned: Vec<String> = Vec::with_capacity(claimed + 1);
        for i in 0..=claimed {
            let p = *matches.add(i);
            if p.is_null() {
                // Index 0 is the common prefix and may legitimately be empty;
                // a NULL past it ends the list.
                if i == 0 {
                    owned.push(String::new());
                    continue;
                }
                break;
            }
            owned.push(String::from_utf8_lossy(c_bytes(p)).into_owned());
        }
        let columns = (&*E).screen_size().map_or(80, |size| size.columns());
        let output = filecomplete::format_match_list(
            &mut owned[1..],
            max as usize,
            columns,
            &mut readline_completion_suffix,
        );
        let _ = (&*E).write_output(&output);

        crate::filecomplete::permute_to_match(matches, &owned);
    }
}

pub(super) unsafe fn complete(ignore: c_int, invoking_key: c_int) -> c_int {
    let _ = ignore;
    // SAFETY: single-threaded module state.
    unsafe {
        lazy_init();

        if rl_inhibit_completion != 0 {
            // A disabled Tab inserts a literal Tab.
            let arr = [invoking_key as c_char, 0];
            crate::eln::el_insertstr(E, arr.as_ptr());
            return c_int::from(CC_REFRESH);
        }

        // Read fresh on every completion; nothing is cached, the returned
        // pointer is used directly and never freed, and the hook runs
        // *before* `_rl_update_pos`, so `rl_point`/`rl_end` are stale inside
        // it (ERR-readline-50).
        let breakchars = match rl_completion_word_break_hook {
            Some(hook) => hook().cast_const(),
            None => rl_basic_word_break_characters,
        };

        _rl_update_pos();

        if E.is_null() {
            return c_int::from(CC_ERROR);
        }

        // Copy both conversion buffers before any user callback runs. The
        // word-break hook's result remains the special-prefix input, preserving
        // ERR-readline-50 without retaining a dynamic borrow across re-entry.
        let word_break = WBREAK_CONV.with_borrow_mut(|buffer| {
            decode_bytes(c_bytes_opt(rl_basic_word_break_characters), buffer)
                .unwrap_or(&[])
                .to_vec()
        });
        let special = SPREFIX_CONV.with_borrow_mut(|buffer| {
            decode_bytes(c_bytes_opt(breakchars), buffer)
                .unwrap_or(&[])
                .to_vec()
        });
        let separators = word_break
            .into_iter()
            .chain(special)
            .map(nshedit::domain::TextUnit::from_code_point)
            .collect();
        let snapshot = filecomplete::observe_completion(&mut *E, separators);
        let invocation = snapshot.invocation();
        let positions = snapshot.positions();
        rl_completion_type = match invocation {
            filecomplete::CompletionInvocation::Insert => b'\t'.into(),
            filecomplete::CompletionInvocation::List => b'?'.into(),
        };
        rl_point = c_int::try_from(positions.cursor).unwrap_or(c_int::MAX);
        rl_end = c_int::try_from(positions.line_end).unwrap_or(c_int::MAX);

        let mut generator = |text: &str, state: usize| -> Option<String> {
            let f = rl_completion_entry_function?;
            let ctext = c_dup(text.as_bytes());
            if ctext.is_null() {
                return None;
            }
            let state = c_int::try_from(state).unwrap_or(c_int::MAX);
            let m = f(ctext, state);
            c_free_str(ctext);
            if m.is_null() {
                return None;
            }
            let out = String::from_utf8_lossy(c_bytes(m)).into_owned();
            // The C's `fn_complete2` takes ownership of what the generator
            // returns, so the block is released once copied.
            c_free_str(m);
            Some(out)
        };
        let has_generator = { rl_completion_entry_function }.is_some();
        let generator: Option<&mut filecomplete::CandidateGenerator<'_>> = if has_generator {
            Some(&mut generator)
        } else {
            None
        };

        let has_attempted = { rl_attempted_completion_function }.is_some();
        let mut attempted = |text: &str, start: usize, finish: usize| {
            let candidates = rl_attempted_completion_function.and_then(|hook| {
                let ctext = c_dup(text.as_bytes());
                if ctext.is_null() {
                    return None;
                }
                let start = c_int::try_from(start).unwrap_or(c_int::MAX);
                let finish = c_int::try_from(finish).unwrap_or(c_int::MAX);
                let matches = hook(ctext, start, finish);
                c_free_str(ctext);
                if matches.is_null() {
                    return None;
                }
                let count = c_array_len(matches);
                let mut owned = Vec::with_capacity(count);
                for &candidate in core::slice::from_raw_parts(matches, count) {
                    owned.push(String::from_utf8_lossy(c_bytes(candidate)).into_owned());
                    c_free_str(candidate);
                }
                c_free_array(matches, count + 1);
                if owned.len() > 1 {
                    owned.remove(0);
                }
                Some(owned)
            });
            let fallback = if rl_attempted_completion_over == 0 {
                filecomplete::AttemptedFallback::Allow
            } else {
                filecomplete::AttemptedFallback::Suppress
            };
            filecomplete::AttemptedCompletion::new(candidates, fallback)
        };
        let attempted: Option<&mut filecomplete::AttemptedProvider<'_>> = if has_attempted {
            Some(&mut attempted)
        } else {
            None
        };
        let mut suffix = readline_completion_suffix;
        let target = E;
        let mut apply =
            move |query: &nshedit::editor::CompletionQuery,
                  candidates: nshedit::editor::CompletionCandidates| {
                (&mut *target)
                    .native_mut()
                    .apply_completion(query, candidates)
            };
        let providers = filecomplete::CompletionProviders::new(generator)
            .with_attempted(attempted)
            .with_suffix(Some(&mut suffix));
        let report = filecomplete::resolve_completion(filecomplete::CompletionRequest::new(
            snapshot,
            providers,
            filecomplete::CompletionPolicy::new(
                rl_completion_query_items as usize,
                filecomplete::UniqueSuffix::Omit,
            ),
            &mut apply,
        ));
        let invocation = report.invocation();
        let positions = report.positions();
        rl_completion_type = match invocation {
            filecomplete::CompletionInvocation::Insert => b'\t'.into(),
            filecomplete::CompletionInvocation::List => b'?'.into(),
        };
        rl_point = c_int::try_from(positions.cursor).unwrap_or(c_int::MAX);
        rl_end = c_int::try_from(positions.line_end).unwrap_or(c_int::MAX);
        if report.attempted_state() == filecomplete::AttemptedState::Reset {
            rl_attempted_completion_over = 0;
        }
        report.apply_effects(&mut *E);

        // A libedit CC_* code, not readline's 0/non-zero status, because the
        // function doubles as an EditLine command through `_el_rl_complete`.
        match report.command() {
            filecomplete::CompletionCommand::Normal => c_int::from(CC_NORM),
            filecomplete::CompletionCommand::Refresh => c_int::from(CC_REFRESH),
            filecomplete::CompletionCommand::Redisplay => {
                c_int::from(crate::cdecl::histedit::CC_REDISPLAY)
            }
            filecomplete::CompletionCommand::Error => c_int::from(CC_ERROR),
        }
    }
}

/// C: `static unsigned char _el_rl_complete(EditLine *el, int ch);` — the
/// editor command bound to TAB, which calls `rl_complete`.
// [spec:libedit:def:readline.el-rl-complete-fn]
// [spec:libedit:sem:readline.el-rl-complete-fn]
pub(super) fn _el_rl_complete(el: *mut EditLine, ch: c_int) -> c_uchar {
    let _ = el;
    // The first argument, readline's ignored `count`, is hardcoded to 0. Every
    // CC_* value is small, so the narrowing is lossless in practice.
    // SAFETY: `complete` reaches the module statics.
    unsafe { complete(0, ch) as c_uchar }
}
