use std::fs;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use super::*;
use crate::testkit::{headless_editor, set_line};

/// Text as the line buffer carries it.
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}

/// A private directory under `TMPDIR`, removed when the value drops.
///
/// Nothing here may assume anything about the machine's filesystem, and
/// every test that reaches one needs entries it can name exactly, so each
/// builds its own tree.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nshedit-filecomplete-{}-{tag}-{n}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        let td = TempDir(path);
        // The tests that drive a whole completion put this path into the
        // line buffer, where a word-break character would end the word
        // early and quietly change what is being measured. `TMPDIR` is
        // honoured, so name the assumption that failed rather than leave
        // a confusing expectation mismatch further down.
        assert!(
            !td.str()
                .chars()
                .any(|c| BREAK_CHARS.contains(&u32::from(c))),
            "TMPDIR must hold no word-break character: {:?}",
            td.0
        );
        td
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn str(&self) -> &str {
        self.0.to_str().expect("TMPDIR must be UTF-8")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A file standing in for the application's output stream.
///
/// `write_outfile` writes through the descriptor rather than through a
/// `FILE *`, so a descriptor of one's own is the only way to read back
/// what the listing path printed.
struct Sink {
    file: fs::File,
    path: PathBuf,
}

impl Sink {
    fn new(td: &TempDir) -> Self {
        let path = td.join("out");
        let file = fs::File::create(&path).unwrap();
        Sink { file, path }
    }

    fn fd(&self) -> i32 {
        self.file.as_raw_fd()
    }

    fn written(&self) -> String {
        fs::read_to_string(&self.path).unwrap()
    }
}

/// An editor whose line holds `s` with the cursor at `at`, on an 80-column
/// screen and writing nowhere.
///
/// The one thing here that is not [`headless_editor`]'s is `thiscmd` being
/// moved off `lastcmd`. A blank editor leaves both at 0, which
/// [`what_to_do`] reads as the *second* consecutive invocation of the same
/// command and answers with `?` — so without this every test would take the
/// listing path instead of the first-tab path.
fn el_with(s: &str, at: usize) -> EditLine {
    let mut el = headless_editor(80, 24);
    set_line(&mut el, s, at);
    el.el_state.thiscmd = 1;
    el
}

fn line_text(el: &EditLine) -> String {
    el.el_line.buffer[..el.el_line.lastchar]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// Two `app_func`s that answer without touching the filesystem, so a test
/// can choose the branch instead of arranging a file to produce it.
fn app_space(_: &str) -> &'static str {
    " "
}
fn app_slash(_: &str) -> &'static str {
    "/"
}

/// The set is 23 characters, and its holes are the interesting part: `[`
/// is a member and `]` is not, `)` and `}` are members although the
/// word-break set carries only their opening forms, and `!`, `~`, `^`,
/// `%`, `:` and `/` are absent — so a completed path is never
/// backslashed at its own separators. ERR-completion-20, reproduced.
// [spec:libedit:sem:filecomplete.needs-escaping-fn/test]
#[test]
fn the_escaping_set_is_the_c_s_twenty_three_characters_holes_included() {
    let members = b"'\"()\\<>$# \n\t?;`@=|{}&*[";
    assert_eq!(members.len(), 23);
    for &c in members {
        assert!(needs_escaping(u32::from(c)), "{:?}", c as char);
    }
    for &c in b"]!~^%:/-_.,+abzAZ09" {
        assert!(!needs_escaping(u32::from(c)), "{:?}", c as char);
    }

    // Anything that is not a single byte falls through to 0, and so does
    // every byte at or above 0x80. That is one defect with two faces: the
    // C compares a `wchar_t` against ASCII literals, so a non-ASCII
    // character is never escaped, and `escape_filename` widens the bytes
    // of a multibyte character one at a time, so its bytes are not
    // escaped either.
    assert!(!needs_escaping(u32::from('é')));
    assert!(!needs_escaping(u32::from('☃')));
    assert!(!needs_escaping(0xC3), "a UTF-8 lead byte");
    assert!(!needs_escaping(0xA9), "a UTF-8 continuation byte");
}

/// Four characters, every one of them also a `needs_escaping` member.
/// That containment is load-bearing rather than incidental:
/// `escape_filename` tests `needs_escaping` first and copies anything it
/// rejects verbatim, so a character in this set but not in that one could
/// never be escaped at all.
// [spec:libedit:sem:filecomplete.needs-dquote-escaping-fn/test]
#[test]
fn the_double_quote_set_is_four_characters_and_a_subset_of_the_bare_set() {
    for &c in b"\"\\`$" {
        assert!(needs_dquote_escaping(c), "{:?}", c as char);
        assert!(
            needs_escaping(u32::from(c)),
            "{:?} is unreachable in escape_filename unless it is in both",
            c as char
        );
    }
    // Everything else, the rest of the bare set included: inside double
    // quotes a space, a semicolon or a glob character stands for itself.
    for &c in b"'() <>#\n\t?;@=|{}&*[a0/" {
        assert!(!needs_dquote_escaping(c), "{:?}", c as char);
    }
}

/// Every backslash is skipped with no look at what follows, so there is
/// no such thing as an escaped backslash here: a `\\` pair collapses to
/// nothing at all and a trailing `\` is simply dropped. ERR-completion-19,
/// reproduced — and the reason `find_word_to_complete` reports a raw span
/// longer than the word it returns.
// [spec:libedit:sem:filecomplete.unescape-string-fn/test]
#[test]
fn unescaping_drops_every_backslash_including_the_escaped_one() {
    // The allocation stays `length + 1` however much shrinks away, and
    // the unused tail stays zeroed, so the caller sees a NUL-terminated
    // wide string in a buffer longer than its content.
    assert_eq!(
        unescape_string(&wide("a\\ b")).unwrap(),
        [wide("a b"), vec![0, 0]].concat()
    );
    assert_eq!(
        unescape_string(&wide("a\\\\b")).unwrap(),
        [wide("ab"), vec![0, 0, 0]].concat(),
        "the pair collapses to nothing rather than to one backslash"
    );
    assert_eq!(
        unescape_string(&wide("ab\\")).unwrap(),
        [wide("ab"), vec![0, 0]].concat(),
        "a trailing backslash is dropped, not kept"
    );

    // An embedded 0 is copied through as an ordinary character, so the
    // result appears to end early to anything that reads to the first
    // NUL while the buffer still holds what follows.
    let out = unescape_string(&[u32::from(b'a'), 0, u32::from(b'b')]).unwrap();
    assert_eq!(out, [u32::from(b'a'), 0, u32::from(b'b'), 0]);

    assert_eq!(unescape_string(&[]).unwrap(), [0]);
}

/// Outside quotes the whole `needs_escaping` set is backslashed, and the
/// append character goes on unconditionally — a space for a plain name, a
/// `/` for a directory, and nothing at all unless this is the only match.
// [spec:libedit:sem:filecomplete.escape-filename-fn/test]
#[test]
fn an_unquoted_match_is_backslash_escaped_and_gets_its_append_character() {
    let el = el_with("", 0);
    assert_eq!(
        escape_filename(&el, "a b(c)", false, None).unwrap(),
        "a\\ b\\(c\\)"
    );
    assert_eq!(
        escape_filename(&el, "a b(c)", false, Some(app_space)).unwrap(),
        "a\\ b\\(c\\)",
        "the append character needs a single match, not merely an app_func"
    );
    assert_eq!(
        escape_filename(&el, "a b(c)", true, Some(app_space)).unwrap(),
        "a\\ b\\(c\\) "
    );
    assert_eq!(
        escape_filename(&el, "dir", true, Some(app_slash)).unwrap(),
        "dir/"
    );
}

/// Inside single quotes only the apostrophe is escaped, and it is escaped
/// by leaving the quote rather than by a backslash. The closing quote is
/// added only when the append character was a space: a `/` means the user
/// is still typing a path, so the quote is deliberately left open.
// [spec:libedit:sem:filecomplete.escape-filename-fn/test]
#[test]
fn inside_single_quotes_only_the_apostrophe_is_escaped() {
    // The scan reads the line up to the cursor, and the C's ordering has
    // `fn_complete2` delete the partial word before calling this — so a
    // lone opening quote is the whole of what it sees.
    let el = el_with("'", 1);
    assert_eq!(
        escape_filename(&el, "a b(c)", false, None).unwrap(),
        "a b(c)",
        "the bare set does not apply inside single quotes"
    );
    assert_eq!(
        escape_filename(&el, "it's", false, None).unwrap(),
        "it'\\''s"
    );
    assert_eq!(
        escape_filename(&el, "a b", true, Some(app_space)).unwrap(),
        "a b'",
        "the space is suppressed inside quotes and the quote is closed"
    );
    assert_eq!(
        escape_filename(&el, "sub", true, Some(app_slash)).unwrap(),
        "sub/",
        "a directory leaves the quote open on purpose"
    );

    // Double quotes escape only the four `needs_dquote_escaping` bytes,
    // so the space stays bare while the `$` is backslashed.
    let el = el_with("\"", 1);
    assert_eq!(
        escape_filename(&el, "a b$c", false, None).unwrap(),
        "a b\\$c"
    );
    assert_eq!(
        escape_filename(&el, "a b", true, Some(app_space)).unwrap(),
        "a b\""
    );
}

/// The quoting scan's two asymmetries, both reproduced. The `"` branch
/// has no backslash check at all, so a user-typed `\"` still opens a
/// double-quoted context (ERR-completion-08); the `'` branch has one but
/// looks back exactly one character, so `\\'` — an escaped backslash and
/// then a quote — is misread as an escaped quote (ERR-completion-09).
// [spec:libedit:sem:filecomplete.escape-filename-fn/test]
#[test]
fn the_quote_scan_mistakes_an_escaped_quote_for_a_quote_and_back_again() {
    // Baseline: an unquoted line escapes the whole bare set.
    let el = el_with("", 0);
    assert_eq!(
        escape_filename(&el, "a b$c", false, None).unwrap(),
        "a\\ b\\$c"
    );

    // ERR-completion-08. The backslash the user typed is ignored, so this
    // escapes as if inside double quotes: the space survives bare.
    let el = el_with("\\\"", 2);
    assert_eq!(
        escape_filename(&el, "a b$c", false, None).unwrap(),
        "a b\\$c"
    );

    // ERR-completion-09. `\\'` really is a backslash followed by an
    // opening quote, but the one-character look-back sees only the
    // backslash, so the quote does not register and the escaping is the
    // unquoted one.
    let el = el_with("\\\\'", 3);
    assert_eq!(
        escape_filename(&el, "a b$c", false, None).unwrap(),
        "a\\ b\\$c"
    );

    // The check does work for the case it was written for.
    let el = el_with("\\'", 2);
    assert_eq!(
        escape_filename(&el, "a b$c", false, None).unwrap(),
        "a\\ b\\$c"
    );
    let el = el_with("'", 1);
    assert_eq!(escape_filename(&el, "a b$c", false, None).unwrap(), "a b$c");
}

/// ERR-completion-10, reproduced: an `app_func` that returns the empty
/// string has its terminating NUL taken as its first character and
/// *stored*, so the result carries an embedded NUL and everything reading
/// it as a C string stops one byte before its real end. readline's append
/// hook does exactly this whenever `rl_completion_append_character` is 0.
// [spec:libedit:sem:filecomplete.escape-filename-fn/test]
#[test]
fn an_empty_append_string_plants_a_nul_inside_the_result() {
    fn app_nothing(_: &str) -> &'static str {
        ""
    }
    let el = el_with("", 0);
    let out = escape_filename(&el, "abc", true, Some(app_nothing)).unwrap();
    assert_eq!(out.as_bytes(), b"abc\0");
    assert_eq!(cstr(&out), "abc", "the C's view ends one byte early");

    // And the quote is not closed either: step 7 asks for a space, and
    // the NUL is not one. A single-quoted context therefore ends up with
    // an unbalanced quote in the line.
    let el = el_with("'", 1);
    let out = escape_filename(&el, "abc", true, Some(app_nothing)).unwrap();
    assert_eq!(out.as_bytes(), b"abc\0");
}

/// `stat` follows symlinks, so the answer is about the target and not the
/// link, and every failure — a missing entry, a dangling link — is the
/// plain-file `" "` rather than an error.
// [spec:libedit:sem:filecomplete.append-char-function-fn/test]
#[test]
fn the_append_character_is_a_slash_only_for_what_stat_calls_a_directory() {
    let td = TempDir::new("append");
    fs::create_dir(td.join("sub")).unwrap();
    fs::write(td.join("file"), b"x").unwrap();
    std::os::unix::fs::symlink(td.join("sub"), td.join("link")).unwrap();
    std::os::unix::fs::symlink(td.join("gone"), td.join("dangling")).unwrap();

    let at = |name: &str| append_char_function(&format!("{}/{name}", td.str()));
    assert_eq!(at("sub"), "/");
    assert_eq!(at("file"), " ");
    assert_eq!(at("link"), "/", "the link target is what is classified");
    assert_eq!(at("dangling"), " ");
    assert_eq!(at("gone"), " ");

    // The expansion is attempted only for a *leading* tilde, so one
    // anywhere else is an ordinary character of the path.
    fs::create_dir(td.join("~x")).unwrap();
    assert_eq!(at("~x"), "/");
}

/// ERR-completion-18, reproduced. The path handed to `stat` is the match
/// string as the generator built it, and a leading `~` sends that string
/// through `fn_tilde_expand` first — whose ERR-completion-07 appends the
/// whole original text to the home directory. So `~` asks about
/// `<home>/~`, never about the home directory the user meant, and a bare
/// `~user` therefore reports a plain file rather than a directory.
// [spec:libedit:sem:filecomplete.append-char-function-fn/test]
#[test]
fn a_leading_tilde_is_stat_ed_as_its_own_broken_expansion() {
    let expanded = fn_tilde_expand("~").expect("only an allocation failure is None");
    match passwd::home_dir("") {
        Some(home) => assert_eq!(expanded, format!("{home}/~")),
        // No entry for the invoking uid, which the rule requires be
        // silent: the text comes back untouched, tilde and all.
        None => assert_eq!(expanded, "~"),
    }
    // `expanded` has no leading tilde, so this second call skips the
    // expansion and stats the path directly. The two agreeing is what
    // says the first one stat'ed the expansion rather than the name.
    assert_eq!(append_char_function("~"), append_char_function(&expanded));
}

/// ERR-completion-21, reproduced: `strcasecmp` folds ASCII case, so two
/// names differing only in case compare EQUAL and the sort's tie-break is
/// left unspecified. It folds byte by byte with no multibyte awareness,
/// so a case pair outside ASCII is not folded at all.
// [spec:libedit:sem:filecomplete.fn-qsort-string-compare-fn/test]
#[test]
fn the_display_sort_folds_ascii_case_and_nothing_else() {
    assert_eq!(_fn_qsort_string_compare("Foo", "foo"), 0);
    assert_eq!(_fn_qsort_string_compare("README", "readme"), 0);

    // The folding is what puts `apple` before `Banana`; a byte-wise
    // comparison would sort every capital ahead of every lowercase.
    assert!(_fn_qsort_string_compare("apple", "Banana") < 0);
    assert!(_fn_qsort_string_compare("Banana", "apple") > 0);

    // A prefix sorts first: the terminator compares below every byte.
    assert!(_fn_qsort_string_compare("ab", "abc") < 0);
    assert!(_fn_qsort_string_compare("abc", "ab") > 0);

    // U+00C9 and U+00E9 are one case pair, and in UTF-8 they differ in
    // their second byte, which is not an ASCII letter and is left alone.
    assert!(_fn_qsort_string_compare("É", "é") < 0);

    // The C string ends at the first NUL, so nothing past it is read.
    assert_eq!(_fn_qsort_string_compare("a\0z", "a"), 0);
}

/// The word runs back to the nearest word-break character, which is the
/// span `fn_complete2` will delete and replace. `/` is not in the set the
/// editor command passes, so a whole path is one word.
// [spec:libedit:sem:filecomplete.find-word-to-complete-fn/test]
#[test]
fn the_word_runs_back_to_the_nearest_break_character() {
    let buf = wide("ls /tm");
    let mut len = 0;
    let w = find_word_to_complete(6, &buf, BREAK_CHARS, None, &mut len, false).unwrap();
    assert_eq!(len, 3);
    assert_eq!(w, [wide("/tm"), vec![0]].concat());

    // With `do_unescape` clear the backslashes stay in and a terminator
    // is appended; the raw span and the returned word are the same length.
    let buf = wide("x a\\ b");
    let mut len = 0;
    let w = find_word_to_complete(6, &buf, BREAK_CHARS, None, &mut len, false).unwrap();
    assert_eq!(len, 4);
    assert_eq!(w, [wide("a\\ b"), vec![0]].concat());
}

/// The escaped-pair test comes before the word-break test, so a
/// backslash-escaped separator does not end the word — which is how
/// completing `a\ b` continues the name rather than starting a new one.
/// ERR-completion-19 shows up here as the length: `*length` is the RAW
/// span, and `fn_complete2` hands that count to `el_deletestr`, so more
/// is removed from the line than the returned word holds.
// [spec:libedit:sem:filecomplete.find-word-to-complete-fn/test]
#[test]
fn an_escaped_break_character_stays_inside_the_word() {
    let buf = wide("x a\\ b");
    let mut len = 0;
    let w = find_word_to_complete(6, &buf, BREAK_CHARS, None, &mut len, true).unwrap();
    assert_eq!(len, 4, "four characters of line for a three-character word");
    assert_eq!(w, [wide("a b"), vec![0, 0]].concat());
}

/// Step 1 steps back over a trailing backslash or quote so the scan
/// carries on through the word before it, and step 3 measures from the
/// ORIGINAL cursor — so the character stepped over is still counted.
/// Step 4 then turns the one-character span a lone quote makes into an
/// empty word starting after it, which is how completion inside a
/// freshly opened quote sees nothing typed yet.
// [spec:libedit:sem:filecomplete.find-word-to-complete-fn/test]
#[test]
fn a_trailing_quote_or_backslash_is_stepped_over_but_still_counted() {
    let buf = wide("ls foo\\");
    let mut len = 0;
    let w = find_word_to_complete(7, &buf, BREAK_CHARS, None, &mut len, true).unwrap();
    assert_eq!(len, 4);
    assert_eq!(w, [wide("foo"), vec![0, 0]].concat());

    let buf = wide("ls '");
    let mut len = 99;
    let w = find_word_to_complete(4, &buf, BREAK_CHARS, None, &mut len, true).unwrap();
    assert_eq!(len, 0);
    assert_eq!(w, [0], "an empty word, not the quote");
}

/// This function treats `special_prefixes` and `word_break` identically —
/// only the caller distinguishes them, which is what lets readline
/// complete `$VAR` by naming `$` a prefix rather than a break. And
/// `wcschr` matches the terminating NUL as well as the members, so a 0 in
/// the line ends the word whatever the sets hold; the line buffer never
/// carries one.
// [spec:libedit:sem:filecomplete.find-word-to-complete-fn/test]
#[test]
fn a_special_prefix_ends_the_word_exactly_as_a_break_character_does() {
    let buf = wide("echo $HO");
    let mut len = 0;
    let sp = [u32::from(b'$')];
    let w = find_word_to_complete(8, &buf, &[], Some(&sp), &mut len, false).unwrap();
    assert_eq!(len, 2);
    assert_eq!(w, [wide("HO"), vec![0]].concat());

    // An empty break set matches nothing but a 0, and here there is one.
    let buf = [u32::from(b'a'), 0, u32::from(b'b')];
    let mut len = 0;
    let w = find_word_to_complete(3, &buf, &[], None, &mut len, false).unwrap();
    assert_eq!(len, 1);
    assert_eq!(w, [u32::from(b'b'), 0]);
}

/// ERR-completion-15, reproduced. The fallback to the generator is taken
/// only when there was no attempted function at all, or when `over` is
/// non-NULL and zero and the attempted one produced nothing. With `over`
/// NULL and a NULL attempted result there is no fallback and completion
/// does nothing whatever: no beep, no refresh, an untouched line. `point`
/// and `end` are still written, because they are written before any
/// callback runs.
// [spec:libedit:sem:filecomplete.fn-complete2-fn/test]
#[test]
fn an_attempted_function_that_declines_and_a_null_over_disable_completion() {
    fn declines(_: &str, _: i32, _: i32) -> Option<Vec<String>> {
        None
    }

    let mut el = el_with("ab", 2);
    let (mut point, mut end) = (-1, -1);
    let r = fn_complete2(
        &mut el,
        None,
        Some(declines),
        BREAK_CHARS,
        None,
        None,
        100,
        None,
        None,
        Some(&mut point),
        Some(&mut end),
        0,
    );
    assert_eq!(r, i32::from(CC_NORM));
    assert_eq!(line_text(&el), "ab", "the partial word is left alone");
    assert_eq!((point, end), (2, 2));

    // The same call with an `over` of 0 does fall back to the generator.
    let mut over = 0;
    let mut generate = |_: &str, s: i32| (s == 0).then(|| "abc".to_owned());
    let mut el = el_with("ab", 2);
    let r = fn_complete2(
        &mut el,
        Some(&mut generate),
        Some(declines),
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        None,
        Some(&mut over),
        None,
        None,
        0,
    );
    assert_eq!(r, i32::from(CC_REFRESH));
    assert_eq!(line_text(&el), "abc ");
}

/// The ordering inside step 12 is load-bearing: the partial word is
/// deleted from the line *before* `escape_filename` scans it, so the
/// quoting scan sees the opening quote the user typed and not the word
/// being completed. Reorder the two and the escaping changes — here the
/// space would acquire a backslash it must not have inside quotes.
// [spec:libedit:sem:filecomplete.fn-complete2-fn/test]
#[test]
fn a_quoted_completion_is_escaped_against_the_line_the_word_has_left() {
    let mut generate = |_: &str, s: i32| (s == 0).then(|| "a b".to_owned());
    let mut el = el_with("'ab", 3);
    let r = fn_complete2(
        &mut el,
        Some(&mut generate),
        None,
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        None,
        None,
        None,
        None,
        FN_QUOTE_MATCH,
    );
    assert_eq!(r, i32::from(CC_REFRESH));
    assert_eq!(line_text(&el), "'a b'");
    assert_eq!(el.el_line.cursor, 5);
}

/// ERR-completion-14, reproduced. Step 12(e) adds the append character
/// only when an `attempted_completion_function` was supplied AND
/// `FN_QUOTE_MATCH` is clear. The quoting path gets one anyway, from
/// `escape_filename`; the third combination — no attempted function and
/// no quoting, which is exactly what `rl_complete` produces when the
/// application set no completion hook of its own — gets none at all.
// [spec:libedit:sem:filecomplete.fn-complete2-fn/test]
#[test]
fn the_append_character_needs_an_attempted_function_and_no_quote_flag() {
    fn declines(_: &str, _: i32, _: i32) -> Option<Vec<String>> {
        None
    }

    let run = |attempted, flags| {
        let mut over = 0;
        let mut generate = |_: &str, s: i32| (s == 0).then(|| "abc".to_owned());
        let mut el = el_with("a", 1);
        let r = fn_complete2(
            &mut el,
            Some(&mut generate),
            attempted,
            BREAK_CHARS,
            None,
            Some(app_space),
            100,
            None,
            Some(&mut over),
            None,
            None,
            flags,
        );
        assert_eq!(r, i32::from(CC_REFRESH));
        line_text(&el)
    };

    assert_eq!(run(Some(declines as AttemptedCompletionFunc), 0), "abc ");
    assert_eq!(run(None, FN_QUOTE_MATCH), "abc ");
    assert_eq!(run(None, 0), "abc", "no attempted function, no append");
}

/// ERR-completion-01, defined rather than reproduced. The C tests
/// `matches[2]` before `matches[1]`, so a caller's own `{prefix, NULL}`
/// array — a one-element `Vec` here — is read out of bounds. Reading past
/// the end as the C's NULL terminator would gives the same answer without
/// the fault: a lone element 0 is a single match.
// [spec:libedit:sem:filecomplete.fn-complete2-fn/test]
#[test]
fn a_one_element_match_array_from_a_caller_is_a_single_match() {
    fn one(_: &str, _: i32, _: i32) -> Option<Vec<String>> {
        Some(vec!["abc".to_owned()])
    }
    let mut el = el_with("a", 1);
    let r = fn_complete2(
        &mut el,
        None,
        Some(one),
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        None,
        None,
        None,
        None,
        0,
    );
    assert_eq!(r, i32::from(CC_REFRESH));
    // Single, so step 12(e) runs and the append character goes in.
    assert_eq!(line_text(&el), "abc ");
}

/// Matches with no common prefix leave element 0 empty, and an empty
/// element 0 means step 12 is skipped entirely — nothing is deleted and
/// nothing is inserted, the partial word survives, and the result drops
/// back to CC_NORM with a beep. That is a first tab over a genuinely
/// ambiguous word.
// [spec:libedit:sem:filecomplete.fn-complete2-fn/test]
#[test]
fn an_empty_common_prefix_leaves_the_line_alone_and_reports_cc_norm() {
    let mut generate = |_: &str, s: i32| match s {
        0 => Some("abc".to_owned()),
        1 => Some("xyz".to_owned()),
        _ => None,
    };
    let mut el = el_with("a", 1);
    let r = fn_complete2(
        &mut el,
        Some(&mut generate),
        None,
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        None,
        None,
        None,
        None,
        0,
    );
    assert_eq!(r, i32::from(CC_NORM));
    assert_eq!(line_text(&el), "a");
}

/// A second consecutive invocation of the same command lists the
/// possibilities rather than completing again: `what_to_do` becomes `?`,
/// the common prefix is still inserted first, and the return is
/// CC_REDISPLAY. The listing is column-major — reading DOWN a column
/// gives sorted order — and every line carries trailing padding, since
/// the pad is emitted after the last column too.
// [spec:libedit:sem:filecomplete.fn-complete2-fn/test]
#[test]
fn a_repeated_completion_lists_the_possibilities_and_redisplays() {
    let td = TempDir::new("list");
    let sink = Sink::new(&td);

    let mut generate = |_: &str, s: i32| match s {
        0 => Some("abc".to_owned()),
        1 => Some("abd".to_owned()),
        _ => None,
    };
    let mut el = el_with("a", 1);
    el.el_outfd = sink.fd();
    el.el_terminal.t_size.h = 80;
    // The repeat: the command that ran last is the command running now.
    el.el_state.lastcmd = el.el_state.thiscmd;

    let mut completion_type = 0;
    let r = fn_complete2(
        &mut el,
        Some(&mut generate),
        None,
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        Some(&mut completion_type),
        None,
        None,
        None,
        0,
    );
    assert_eq!(r, i32::from(CC_REDISPLAY));
    assert_eq!(completion_type, i32::from(b'?'));
    assert_eq!(line_text(&el), "ab", "the common prefix, and only that");
    // One newline to get off the command line, then one row of two
    // columns. `width` is 3 and both names are 3 long, so the padding is
    // empty — and yet the gap is two spaces, because the column separator
    // is emitted on TOP of the append character rather than instead of
    // it. The trailing space is that append character on the last column,
    // which is why every line ends in whitespace.
    assert_eq!(sink.written(), "\nabc  abd \n");
}

/// `fn_complete`'s whole contribution is the flag, and it is observable in
/// the line: an application that supplied its own attempted function is
/// trusted to have quoted what it produced, so the match goes in verbatim;
/// without one the built-in filename path runs and the match is escaped.
// [spec:libedit:sem:filecomplete.fn-complete-fn/test]
#[test]
fn fn_complete_sets_the_quote_flag_exactly_when_there_is_no_attempted_function() {
    let mut generate = |_: &str, s: i32| (s == 0).then(|| "a b".to_owned());
    let mut el = el_with("a", 1);
    let r = fn_complete(
        &mut el,
        Some(&mut generate),
        None,
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        None,
        None,
        None,
        None,
    );
    assert_eq!(r, i32::from(CC_REFRESH));
    assert_eq!(line_text(&el), "a\\ b ", "escaped, because FN_QUOTE_MATCH");

    fn attempted(_: &str, _: i32, _: i32) -> Option<Vec<String>> {
        Some(vec!["a b".to_owned(), "a b".to_owned()])
    }
    let mut el = el_with("a", 1);
    let r = fn_complete(
        &mut el,
        None,
        Some(attempted),
        BREAK_CHARS,
        None,
        Some(app_space),
        100,
        None,
        None,
        None,
        None,
    );
    assert_eq!(r, i32::from(CC_REFRESH));
    assert_eq!(
        line_text(&el),
        "a b ",
        "verbatim, because the flag is clear"
    );
}

/// The editor command's whole contribution is its fixed arguments: the
/// built-in filename generator, `append_char_function`, the file-static
/// word-break set — which carries no `/`, so a whole path is one word —
/// no special prefixes, a query threshold of 100, nothing reported back,
/// and, through `fn_complete`'s flag rule, `FN_QUOTE_MATCH`. So a
/// filename with a space in it reaches the line backslash-escaped.
// [spec:libedit:sem:filecomplete.el-fn-complete-fn/test]
#[test]
fn the_editor_command_completes_a_filename_and_escapes_it() {
    let td = TempDir::new("elfn");
    fs::write(td.join("only one"), b"x").unwrap();

    let typed = format!("{}/only", td.str());
    let mut el = el_with(&typed, typed.chars().count());
    assert_eq!(_el_fn_complete(&mut el, 0), CC_REFRESH);
    assert_eq!(line_text(&el), format!("{}/only\\ one ", td.str()));
}

/// ERR-completion-23, reproduced: `_el_fn_sh_complete` is an exported
/// symbol with no behaviour of its own, so that a key-binding table can
/// name a "shell-style" completion distinctly. Both symbols must be kept
/// and both must stay observably identical, `ch` included — it is unused
/// on either side.
// [spec:libedit:sem:filecomplete.el-fn-sh-complete-fn/test]
#[test]
fn the_shell_style_command_is_the_same_command_under_another_name() {
    let td = TempDir::new("shfn");
    fs::create_dir(td.join("dir name")).unwrap();
    let typed = format!("{}/dir", td.str());

    let mut plain = el_with(&typed, typed.chars().count());
    let plain_rv = _el_fn_complete(&mut plain, 0);

    let mut shell = el_with(&typed, typed.chars().count());
    let shell_rv = _el_fn_sh_complete(&mut shell, 0x7f);

    assert_eq!(plain_rv, shell_rv);
    assert_eq!(line_text(&plain), line_text(&shell));
    assert_eq!(plain.el_line.cursor, shell.el_line.cursor);
    // A directory, so the append character is a `/` and no space closes
    // the word off.
    assert_eq!(line_text(&shell), format!("{}/dir\\ name/", td.str()));
}
