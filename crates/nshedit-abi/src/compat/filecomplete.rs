//! Ported from `src/filecomplete.c`; rules live in `docs/spec/port/src/filecomplete.md`.
//!
//! Two shapes here depart from the C deliberately, and both are forced by
//! `plan/decisions/idiomatic-core.md`: the core carries no globals, so the
//! match generator's file-statics become an explicit
//! [`FilenameCompletionState`] the caller owns, and the generator callback
//! type widens from a bare function pointer to `&mut dyn FnMut` so a
//! generator that needs state can carry it. The C's stateful face — one
//! process-wide scan, restarted by `state == 0` — is the ABI crate's to
//! present.
//!
//! # Where a `String` cannot hold what the C held
//!
//! Three places in this file traffic in byte strings the C never validates,
//! and the `def`-rule signatures name them `String`. Each is defined here
//! rather than left to the implementation:
//!
//! - **A directory entry whose name is not valid UTF-8** cannot be returned
//!   by [`fn_filename_completion_function`], so it is skipped — never a
//!   candidate, rather than a mangled one. See the note at the match loop.
//! - **A common prefix cut mid-character.** `sem:filecomplete.completion-matches-fn`
//!   step 4 is explicit that the comparison is byte-wise and can cut a
//!   multibyte character in half; [`completion_matches`] floors the cut to
//!   the preceding character boundary, so element 0 stays a genuine (if
//!   occasionally shorter) prefix of element 1 instead of becoming invalid
//!   text or acquiring a replacement character. The unique-match case is
//!   unaffected: there `max_equal` is the whole of element 1.
//! - **An `app_func` whose first byte is a UTF-8 continuation or lead byte.**
//!   [`escape_filename`] copies only `append_char[0]`, as the C does; if that
//!   one byte leaves the buffer invalid the function reports the C's
//!   allocation-failure `None`. Both built-in append strings are ASCII, so
//!   this is reachable only from an application's own `app_func`.
//!
//! # The passwd database
//!
//! `fn_tilde_expand` needs it, and reaches NSS through `nshedit-plat` — so a
//! user that exists only in a directory resolves as it does for the C, and a
//! lookup can block on a network name service. See [`passwd`].

use std::io::Read;

use crate::chared::{el_deletestr, el_winsertstr};
use crate::chartype::{ct_decode_string, ct_encode_string};
use crate::el::{EditLine, el_beep};
use crate::histedit::{CC_NORM, CC_REDISPLAY, CC_REFRESH};

/// C: `#define FN_QUOTE_MATCH 1U` — quote the returned match.
///
/// `filecomplete.h`'s one macro. It has no `def` rule of its own; it is
/// declared here because this module is that header's Rust home, and
/// [`fn_complete`] is the only thing that computes it.
pub const FN_QUOTE_MATCH: u32 = 1;

/// The three ASCII characters this file tests wide input against: the
/// backslash and the two quotes.
///
/// Named because a Rust pattern takes no cast expression, so `'\\' as u32`
/// cannot be written inline in the `match`es that want it.
const BSLASH: u32 = '\\' as u32;
/// C: `'\''` widened. See [`BSLASH`].
const SQUOTE: u32 = '\'' as u32;
/// C: `'"'` widened. See [`BSLASH`].
const DQUOTE: u32 = '"' as u32;

/// C: `static const wchar_t break_chars[] = L" \t\n\"\\'`@$><=;|&{("`.
///
/// The file-static word-break set [`_el_fn_complete`] passes. It carries the
/// opening `{` and `(` but not their closing partners, and it is a different
/// set from `needs_escaping`'s (ERR-completion-20).
const BREAK_CHARS: &[u32] = &[
    ' ' as u32,
    '\t' as u32,
    '\n' as u32,
    DQUOTE,
    BSLASH,
    SQUOTE,
    '`' as u32,
    '@' as u32,
    '$' as u32,
    '>' as u32,
    '<' as u32,
    '=' as u32,
    ';' as u32,
    '|' as u32,
    '&' as u32,
    '{' as u32,
    '(' as u32,
];

/// C: `char *` — the bytes up to the first NUL.
///
/// Every string this file receives is a C string in the original, so every
/// `strlen`, `strcmp`, `strchr` and `%s` stops at the first NUL. A Rust
/// `&str` carries its own length and may hold interior NULs —
/// [`escape_filename`] deliberately produces one (ERR-completion-10) — so
/// the C's view is taken explicitly wherever it matters.
fn cstr(s: &str) -> &str {
    match s.as_bytes().iter().position(|&b| b == 0) {
        // NUL is ASCII, so the split is always on a character boundary.
        Some(i) => &s[..i],
        None => s,
    }
}

/// C: `wcschr(set, c) != NULL`.
///
/// `set` is a NUL-terminated wide string in the C and a slice here, and
/// `wcschr` matches the terminator as well as the members — so a 0 in the
/// line buffer counts as a member of any set. `sem:filecomplete.find-word-to-complete-fn`
/// records that, and that the line buffer never holds one.
fn wcschr(set: &[u32], c: u32) -> bool {
    c == 0 || set.contains(&c)
}

/// C: `getc(stdin)` — always the process `stdin`, never `el->el_infile`.
///
/// ERR-completion-12, disposition *reproduce*: the stream really is `stdin`.
/// `None` stands for the C's `EOF`, and for a read error, neither of which is
/// `'y'`. Exactly one byte is consumed; what a buffered reader pulled in
/// behind it stays buffered, as it does in C stdio.
fn getc_stdin() -> Option<u8> {
    let mut b = [0u8; 1];
    match std::io::stdin().read(&mut b) {
        Ok(1) => Some(b[0]),
        _ => None,
    }
}

/// The passwd-database lookups [`fn_tilde_expand`] is written against.
///
/// `plan/decisions/platform-layer.md` put `getpwnam_r`, `getpwuid_r` and
/// `getuid` in `nshedit-plat`, so the `/etc/passwd` parser that used to stand
/// here is **deleted rather than demoted to a fallback**. The reason is in the
/// rule: `sem:filecomplete.fn-tilde-expand-fn` step 3 requires the POSIX
/// `getpw*_r` shape with a fixed 1024-byte scratch buffer, "treating ANY
/// non-zero return as *no such user*" — which deliberately conflates `ERANGE`
/// with absence. A hand parser has no 1024-byte limit and expands names the C
/// does not, so a parse sitting behind the syscall would disagree in exactly
/// the case the rule pins.
///
/// What that buys, and what it costs, both named by the rule: a user that
/// exists only in LDAP, SSSD, AD, NIS or systemd-homed now resolves exactly as
/// it does for the C — and, because the invoking user on such a host is
/// usually one of them, bare `~` and `~/…` resolve for the person at the
/// keyboard too. The price is that the lookup can block on a network name
/// service, inside a completion keystroke. `nshedit_plat::passwd::PasswdOps`
/// is the seam for a caller that must not pay it.
mod passwd {
    /// C: `getpwnam_r(name, …)->pw_dir`, or `getpwuid_r(getuid(), …)->pw_dir`
    /// when `name` is empty — `sem:filecomplete.fn-tilde-expand-fn` step 3.
    /// `None` is that rule's "the lookup produced nothing", which covers a
    /// genuine absence, a NULL result with a zero return, and `ERANGE` alike.
    pub(super) fn home_dir(name: &str) -> Option<String> {
        let dir = if name.is_empty() {
            // The *real* uid, not the effective one: the C calls `getuid()`.
            nshedit_plat::passwd::home_dir_by_uid(nshedit_plat::getuid())
        } else {
            nshedit_plat::passwd::home_dir_by_name(name)
        }?;
        // The C hands `pw_dir` straight to `strlcpy`, so a non-UTF-8 home
        // directory is bytes there and would have to be bytes here to be
        // reproduced exactly; the port's `fn_tilde_expand` returns a `String`
        // because `def:filecomplete.fn-tilde-expand-fn` fixes that, so an
        // undecodable one reads as no such user rather than being mangled.
        String::from_utf8(dir).ok()
    }
}

/// C: `char *(*)(const char *, int)` — a match generator.
///
/// The C spells this out at each use and has no typedef for it, so there is
/// no rule to carry here; it is named only because the parameter types are
/// otherwise repeated four times. `FnMut` rather than a bare `fn` pointer
/// because a generator's scan state is its own to carry — see
/// [`FilenameCompletionState`].
pub type CompleteFunc = dyn FnMut(&str, i32) -> Option<String>;

/// C: `char **(*)(const char *, int, int)` — the application's own
/// completion hook, tried before the generator. Stateless in the C, so a
/// plain function pointer. No typedef there, hence no rule here.
pub type AttemptedCompletionFunc = fn(&str, i32, i32) -> Option<Vec<String>>;

/// C: `const char *(*)(const char *)` — chooses the string appended after a
/// completed name. The return is a literal the caller must not free, hence
/// `&'static str`. No typedef in the C, hence no rule here.
pub type AppFunc = fn(&str) -> &'static str;

// [spec:libedit:def:filecomplete.fn-tilde-expand-fn]
// [spec:libedit:sem:filecomplete.fn-tilde-expand-fn]
/// C: `char * fn_tilde_expand(const char *txt)`.
///
/// `None` is the C's NULL, which here means only an allocation failure: a
/// `txt` that does not start with `~`, and an unknown user name, both come
/// back as a copy of `txt`.
pub fn fn_tilde_expand(txt: &str) -> Option<String> {
    // The C walks a `char *`, so everything stops at the first NUL.
    let txt = cstr(txt);

    // Step 1. An empty `txt` reads `txt[0]` as its NUL, which is not `~`.
    if !txt.starts_with('~') {
        return Some(txt.to_owned());
    }

    // Step 2. `strchr(txt + 1, '/')`. `len` is the offset of the rest of the
    // path and is assigned ONLY in the branch that found a slash — that
    // omission is ERR-completion-07, reproduced: see step 5 below.
    let mut len = 0usize;
    let name = match txt.as_bytes()[1..].iter().position(|&b| b == b'/') {
        None => &txt[1..],
        Some(rel) => {
            // `pos - txt + 1`, i.e. tilde, name and slash.
            let k = rel + 1;
            len = k + 1;
            // `strlcpy(temp, txt + 1, len - 1)` copies `k - 1` bytes: the
            // characters strictly between the tilde and the slash. `/` is
            // ASCII, so `k` is a character boundary.
            &txt[1..k]
        }
    };

    // Steps 3 and 4. An empty name is the current user; anything else is a
    // name lookup. A lookup that produced nothing is not an error and is not
    // reported as one — the original text comes back, tilde and all.
    let Some(pw_dir) = passwd::home_dir(name) else {
        return Some(txt.to_owned());
    };

    // Step 5, and ERR-completion-07 with it. `len` is still 0 whenever there
    // was no slash, so the ENTIRE original string — tilde included — is what
    // gets appended: `~` becomes `$HOME/~` and `~bob` becomes
    // `<bob's home>/~bob`. `[dec:libedit:conformance-policy]` names this as
    // one of the six forks that default to reproduce.
    //
    // The `/` goes in unconditionally, so a `pw_dir` of `/` doubles it.
    let rest = &txt[len..];
    let mut out = String::new();
    if out
        .try_reserve_exact(pw_dir.len() + 1 + rest.len() + 1)
        .is_err()
    {
        return None;
    }
    out.push_str(&pw_dir);
    out.push('/');
    out.push_str(rest);
    Some(out)
}

// [spec:libedit:def:filecomplete.needs-escaping-fn]
// [spec:libedit:sem:filecomplete.needs-escaping-fn]
/// C: `static int needs_escaping(wchar_t c)`, a predicate whose `int` is only
/// ever 1 or 0 and only ever read as a condition.
fn needs_escaping(c: u32) -> bool {
    // Exactly 23 characters, and every one of them ASCII. `]` is absent
    // although `[` is present, `)` and `}` are present, and `!`, `~`, `^`,
    // `%`, `:` and `/` are absent — ERR-completion-20, disposition
    // *reproduce*.
    //
    // Anything that is not a single byte falls through to 0, which covers
    // both a wide character outside ASCII and the high-bit bytes
    // `escape_filename` widens one at a time: the C's comparisons are against
    // ASCII code points and match neither, whichever way `char` and `wchar_t`
    // are signed.
    matches!(
        u8::try_from(c),
        Ok(b'\''
            | b'"'
            | b'('
            | b')'
            | b'\\'
            | b'<'
            | b'>'
            | b'$'
            | b'#'
            | b' '
            | b'\n'
            | b'\t'
            | b'?'
            | b';'
            | b'`'
            | b'@'
            | b'='
            | b'|'
            | b'{'
            | b'}'
            | b'&'
            | b'*'
            | b'[')
    )
}

// [spec:libedit:def:filecomplete.needs-dquote-escaping-fn]
// [spec:libedit:sem:filecomplete.needs-dquote-escaping-fn]
/// C: `static int needs_dquote_escaping(char c)`. A byte of a narrow
/// filename, so `u8` and not `u32`; a predicate, so `bool` and not `int`.
fn needs_dquote_escaping(c: u8) -> bool {
    // All four are also members of the `needs_escaping` set, which is
    // load-bearing: escaping is reached only through that set, so a character
    // in this one and not in that one could never be escaped at all.
    matches!(c, b'"' | b'\\' | b'`' | b'$')
}

// [spec:libedit:def:filecomplete.unescape-string-fn]
// [spec:libedit:sem:filecomplete.unescape-string-fn]
/// C: `static wchar_t * unescape_string(const wchar_t *string, size_t
/// length)`. The C's `(pointer, length)` pair is the slice; there is no
/// separate `length`. `None` is the C's allocation failure.
fn unescape_string(string: &[u32]) -> Option<Vec<u32>> {
    // Step 1: `el_calloc(length + 1, …)`. The result keeps that full length —
    // the rule is explicit that the allocation stays `length + 1` and the
    // unused tail stays zeroed — so the caller sees a NUL-terminated wide
    // string whose content may be shorter than the buffer.
    let mut unescaped = Vec::new();
    if unescaped.try_reserve_exact(string.len() + 1).is_err() {
        return None;
    }
    unescaped.resize(string.len() + 1, 0);

    // Steps 2 and 3. The skip is unconditional, with no look at what follows:
    // a `\\` pair collapses to nothing at all and a trailing `\` is dropped
    // (ERR-completion-19). An embedded 0 is copied through as an ordinary
    // character, producing a string that appears to end early.
    let mut j = 0usize;
    for &c in string {
        if c == BSLASH {
            continue;
        }
        unescaped[j] = c;
        j += 1;
    }
    unescaped[j] = 0;
    Some(unescaped)
}

/// The quoting context a completion is inserted into — the C's `s_quoted`
/// and `d_quoted`, which its own rule records can never both be set.
///
/// One value rather than two flags because that exclusion is what the
/// counting and emitting passes rely on: each folds "inside single quotes"
/// and "inside double quotes" into one branch, and both would be wrong if the
/// two could overlap.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quoting {
    /// C: neither flag set.
    Bare,
    /// C: `s_quoted`.
    Single,
    /// C: `d_quoted`.
    Double,
}

/// Step 2 of [`escape_filename`]: the quoting context `typed` leaves its end
/// in.
///
/// `typed` is the line up to but not including the cursor. `fn_complete2` has
/// ALREADY deleted the partial word by the time this runs, so the scan sees
/// the opening quote the user typed but not the word being completed;
/// reordering the two changes the escaping.
fn quoting_before(typed: &[u32]) -> Quoting {
    let mut quoting = Quoting::Bare;
    let mut prev: Option<u32> = None;
    for &c in typed {
        // The look-back is exactly one character, so `\\'` — an escaped
        // backslash followed by a quote — is misread as an escaped quote
        // (ERR-completion-09).
        let bare_quote = prev != Some(BSLASH);
        quoting = match (quoting, c) {
            (Quoting::Bare, SQUOTE) if bare_quote => Quoting::Single,
            (Quoting::Single, SQUOTE) if bare_quote => Quoting::Bare,
            // ERR-completion-08, disposition *reproduce*: no backslash check
            // on these two, so a user-typed `\"` still toggles the
            // double-quote state. The asymmetry with the single-quote rule
            // above is the bug.
            (Quoting::Bare, DQUOTE) => Quoting::Double,
            (Quoting::Double, DQUOTE) => Quoting::Bare,
            // A quote of the other kind, or any ordinary character, leaves
            // the state alone — which is how each kind stays inert inside the
            // other.
            (q, _) => q,
        };
        prev = Some(c);
    }
    quoting
}

/// What one byte of the filename becomes on its way into the line.
///
/// The C asks this twice in two differently-shaped conditionals — step 3 to
/// budget the buffer, step 5 to fill it — and the two have to agree byte for
/// byte or the buffer is the wrong size. Here there is one answer and the
/// budget is read off it.
#[derive(Clone, Copy)]
enum EscapedByte {
    /// The byte itself.
    Verbatim,
    /// A backslash, then the byte.
    Backslashed,
    /// `'\''` — close the single-quoted string, backslash-escape the
    /// apostrophe, reopen it. The only form costing more than one extra byte.
    QuoteBreak,
}

/// Steps 3 and 5 of [`escape_filename`], asked of one byte.
fn escaped_byte(quoting: Quoting, c: u8) -> EscapedByte {
    match quoting {
        // Outside quotes the whole `needs_escaping` set is backslashed.
        Quoting::Bare if needs_escaping(u32::from(c)) => EscapedByte::Backslashed,
        // Inside single quotes nothing but the apostrophe needs anything.
        Quoting::Single if c == b'\'' => EscapedByte::QuoteBreak,
        // Inside double quotes only the four `needs_dquote_escaping` bytes,
        // every one of which is also a `needs_escaping` member — which is what
        // lets this skip the C's outer `needs_escaping` test without changing
        // the answer.
        Quoting::Double if needs_dquote_escaping(c) => EscapedByte::Backslashed,
        _ => EscapedByte::Verbatim,
    }
}

// [spec:libedit:def:filecomplete.escape-filename-fn]
// [spec:libedit:sem:filecomplete.escape-filename-fn]
/// C: `static char * escape_filename(EditLine *el, const char *filename, int
/// single_match, const char *(*app_func)(const char *))`.
///
/// The C's `filename == NULL` guard is unrepresentable — a `&str` is never
/// null — so only the allocation-failure `None` survives. `el` is shared
/// rather than exclusive because the editor is only read here: the line this
/// escapes against is one `fn_complete2` has already finished editing.
fn escape_filename(
    el: &EditLine,
    filename: &str,
    single_match: bool,
    app_func: Option<AppFunc>,
) -> Option<String> {
    // The C walks a `char *`; the emitting pass, the counting pass and the
    // `app_func` call all see the same bytes-up-to-NUL.
    let filename = cstr(filename);

    // Step 2. `cursor` is an offset into `buffer` by the crate's convention,
    // so the clamp never fires; it is here so a broken invariant degrades
    // into a short scan rather than a panic.
    let end = el.el_line.cursor.min(el.el_line.buffer.len());
    let quoting = quoting_before(&el.el_line.buffer[..end]);

    // Step 3: count the extra bytes the escaping will need.
    let bytes = filename.as_bytes();
    let original_len = bytes.len();
    let escaped_character_count: usize = bytes
        .iter()
        .map(|&c| match escaped_byte(quoting, c) {
            EscapedByte::Verbatim => 0,
            EscapedByte::Backslashed => 1,
            EscapedByte::QuoteBreak => 3,
        })
        .sum();

    // Step 4. One byte for a closing quote, one for the append character.
    let mut newlen = original_len + escaped_character_count + 1;
    if quoting != Quoting::Bare {
        newlen += 1;
    }
    if single_match && app_func.is_some() {
        newlen += 1;
    }
    let mut escaped_str: Vec<u8> = Vec::new();
    if escaped_str.try_reserve_exact(newlen).is_err() {
        return None;
    }

    // Step 5: emit exactly what step 3 budgeted.
    for &c in bytes {
        match escaped_byte(quoting, c) {
            EscapedByte::Verbatim => escaped_str.push(c),
            EscapedByte::Backslashed => escaped_str.extend_from_slice(&[b'\\', c]),
            EscapedByte::QuoteBreak => escaped_str.extend_from_slice(b"'\\''"),
        }
    }

    // Step 6. `app_func` receives the ORIGINAL filename, not the escaped
    // string, and only the first byte of its answer is used. The C's
    // transient `escaped_str[offset] = 0` before the call is invisible: every
    // path either overwrites that byte or leaves it where step 8's
    // terminator goes anyway.
    //
    // An empty `app_func` result yields its NUL here, which is then *stored* —
    // the result carries an embedded NUL and its visible length ends one byte
    // before the real end (ERR-completion-10, disposition *reproduce*).
    let appended = app_func
        .filter(|_| single_match)
        .map(|f| f(filename).as_bytes().first().copied().unwrap_or(0));
    if let Some(b) = appended {
        // A space is appended only outside quotes; anything else, typically
        // the `/` of a directory, unconditionally.
        if b != b' ' || quoting == Quoting::Bare {
            escaped_str.push(b);
        }
    }

    // Step 7: close the quote, but only when the append character was a
    // space — the byte step 4 reserved for it is what the quote reuses. A
    // `/` leaves the quote open on purpose: the user keeps typing the path.
    if appended == Some(b' ') {
        match quoting {
            Quoting::Single => escaped_str.push(b'\''),
            Quoting::Double => escaped_str.push(b'"'),
            Quoting::Bare => {}
        }
    }

    // Step 8's terminating NUL is the `String`'s own length here.
    //
    // Every byte above is either copied from `filename` or an ASCII literal,
    // so the only way the result is not UTF-8 is an `app_func` whose first
    // byte is part of a multibyte character. That is a caller error the
    // `String` return cannot carry; it takes the C's allocation-failure
    // `None`, which `fn_complete2` already handles.
    String::from_utf8(escaped_str).ok()
}

/// The match generator's scan state.
///
/// The C keeps all five of these in function-level `static`s inside
/// `fn_filename_completion_function`, one set per process; see the hazards
/// in `sem:filecomplete.fn-filename-completion-function-fn`. The core has no
/// globals, so the caller owns the state and hands it in, which is also what
/// makes two interleaved scans possible.
///
/// The real iteration cursor is the open directory stream, not the `state`
/// argument: a scan restarts when `state == 0` **or** when `dir` is `None`.
#[derive(Default)]
pub struct FilenameCompletionState {
    /// C: `static DIR *dir` — the open directory stream, positioned just
    /// past the last entry returned. `None` between scans.
    dir: Option<std::fs::ReadDir>,
    /// C: `static char *filename` — the trailing component of `text` that
    /// entries must be prefixed by. `None` when `text` was empty.
    filename: Option<String>,
    /// C: `static char *dirname` — the directory prefix exactly as the user
    /// typed it, including its trailing slash and any unexpanded `~`. It is
    /// this, not `dirpath`, that is prepended to a match.
    dirname: Option<String>,
    /// C: `static char *dirpath` — the path actually handed to `opendir`,
    /// after tilde expansion.
    dirpath: Option<String>,
    /// C: `static size_t filename_len` — byte length of `filename`.
    filename_len: usize,
}

// [spec:libedit:def:filecomplete.fn-filename-completion-function-fn]
// [spec:libedit:sem:filecomplete.fn-filename-completion-function-fn]
/// C: `char * fn_filename_completion_function(const char *text, int state)`.
///
/// The default match generator. `scan` is the C's file-statics made
/// explicit; `state` keeps its C meaning — a restart flag, not a sequence
/// number — and `text` is ignored entirely on a continuation.
pub fn fn_filename_completion_function(
    scan: &mut FilenameCompletionState,
    text: &str,
    state: i32,
) -> Option<String> {
    // ERR-completion-16, disposition *reproduce*: the restart also triggers
    // whenever the stream is closed, so calling again with a non-zero `state`
    // AFTER this returned `None` restarts the scan from the first entry
    // instead of continuing to return `None`. A caller that does not stop at
    // the first `None` loops forever. `completion_matches` stops.
    //
    // On a continuation `text` is not read at all: a caller that changes it
    // while passing a non-zero `state` gets matches for the old text
    // (ERR-completion-17). The C's own comment — "value of `state` is
    // ignored" — has this exactly backwards.
    if state == 0 || scan.dir.is_none() {
        // The C walks a `char *`.
        let text = cstr(text);

        // Step 1: split at the LAST slash. ERR-completion-04, disposition
        // *define*: the C can fail here after replacing `filename`/`dirname`
        // but before closing the stale stream and updating `filename_len`,
        // leaving a following call to run the match loop against a NULL or
        // stale pattern. Nothing between here and step 5 can fail without
        // Rust's allocator aborting, and the whole state is replaced in one
        // sweep, so the stale-state window does not exist.
        let (filename, dirname) = match text.as_bytes().iter().rposition(|&b| b == b'/') {
            Some(k) => (
                // Everything after the slash — empty when `text` ends in one.
                Some(text[k + 1..].to_owned()),
                // Everything up to AND INCLUDING the slash.
                Some(text[..=k].to_owned()),
            ),
            None => (
                if text.is_empty() {
                    None
                } else {
                    Some(text.to_owned())
                },
                None,
            ),
        };
        scan.filename = filename;
        scan.dirname = dirname;

        // Step 2: close any stream left open by a previous scan. In the C
        // this happens after step 1, which is what makes step 1's failure
        // paths leave a stream behind.
        scan.dir = None;

        // Step 3: the path actually opened. Support for `~user` syntax.
        scan.dirpath = match scan.dirname.as_deref() {
            None => {
                scan.dirname = Some(String::new());
                Some("./".to_owned())
            }
            // `dirname` always ends in `/` here, so the tilde expansion
            // always takes its slash branch and ERR-completion-07 is
            // unreachable from this call site. An unknown user comes back as
            // the literal `~user/`, which then fails to open.
            Some(d) if d.starts_with('~') => fn_tilde_expand(d),
            Some(d) => Some(d.to_owned()),
        };
        let dirpath = scan.dirpath.as_deref()?;

        // Step 4: `opendir`. On failure the stream stays closed, so every
        // later call re-runs this entire restart path and fails identically.
        scan.dir = std::fs::read_dir(dirpath).ok();
        scan.dir.as_ref()?;

        // Step 5.
        scan.filename_len = scan.filename.as_deref().map_or(0, str::len);
    }

    // Step 6: the match loop, run on every call, restart or continuation.
    let mut found: Option<String> = None;
    {
        let filename = scan.filename.as_deref();
        let filename_len = scan.filename_len;
        let dir = scan.dir.as_mut()?;
        for next in dir.by_ref() {
            // `readdir` reports an error by returning NULL, which is also how
            // it reports the end of the stream; the C cannot tell them apart
            // and neither does this.
            let Ok(entry) = next else { break };

            // A name that is not valid UTF-8 cannot be returned as a
            // `String`, so it is not a candidate at all. See the module
            // documentation: skipping is the definition chosen, because a
            // lossy conversion would offer the user a name that does not
            // exist.
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };

            // Skip exactly `.` and `..`; every other dot-file IS a candidate,
            // so hidden files are offered even when the user typed no dot.
            if name == "." || name == ".." {
                continue;
            }
            if filename_len == 0 {
                found = Some(name);
                break;
            }
            // Byte-wise, case-SENSITIVE `strncmp` — no locale folding, no
            // multibyte awareness, no globbing. The C's redundant first-byte
            // test is folded into the prefix test. A `filename_len` above
            // zero implies a pattern; the C would dereference NULL here
            // instead (ERR-completion-04, defined away by the atomic reset).
            let Some(pat) = filename else { break };
            if name.as_bytes().starts_with(&pat.as_bytes()[..filename_len]) {
                found = Some(name);
                break;
            }
        }
    }

    match found {
        // Step 7: `dirname` concatenated with the entry name — the prefix the
        // USER typed, so an unexpanded `~` stays unexpanded and `dirpath`
        // never appears. The stream is deliberately left OPEN and positioned
        // just past this entry; that is what lets the next call continue.
        Some(name) => {
            let dirname = scan.dirname.as_deref().unwrap_or("");
            let mut out = String::new();
            if out
                .try_reserve_exact(dirname.len() + name.len() + 1)
                .is_err()
            {
                // The C abandons the scan mid-way with the stream still open.
                return None;
            }
            out.push_str(dirname);
            out.push_str(&name);
            Some(out)
        }
        // Step 8: close the stream and clear the pointer. `filename`,
        // `dirname`, `dirpath` and `filename_len` are NOT cleared; they
        // survive until the next restart replaces them.
        None => {
            scan.dir = None;
            None
        }
    }
}

// [spec:libedit:def:filecomplete.append-char-function-fn]
// [spec:libedit:sem:filecomplete.append-char-function-fn]
/// C: `static const char * append_char_function(const char *name)`.
///
/// The default `app_func`. The return is one of two string literals the
/// caller must not free, hence `&'static str`.
fn append_char_function(name: &str) -> &'static str {
    // Step 1. The expansion is attempted only for a leading `~`.
    let expname = if name.starts_with('~') {
        fn_tilde_expand(name)
    } else {
        None
    };

    // Steps 2 to 4. `stat` follows symlinks, so a symlink to a directory
    // yields `"/"`. Any failure at all — nonexistent, permission denied, a
    // symlink loop, or a failed expansion that left the raw `~…` text to be
    // stat'ed — leaves the result at `" "`.
    //
    // The path stat'ed is the match string exactly as the generator built
    // it, directory prefix and all. Combined with ERR-completion-07 that
    // makes a bare `~user` stat the nonsense path `<home>/~user`, so it
    // always reports `" "` (ERR-completion-18).
    //
    // The answer is inherently a filesystem race and the rule does not ask
    // for it to be made atomic with the generator's observation.
    let path = expname.as_deref().unwrap_or(name);
    match std::fs::metadata(path) {
        Ok(st) if st.is_dir() => "/",
        _ => " ",
    }
}

// [spec:libedit:def:filecomplete.completion-matches-fn]
// [spec:libedit:sem:filecomplete.completion-matches-fn]
/// C: `char ** completion_matches(const char *text, char *(*genfunc)(const
/// char *, int))`.
///
/// The returned vector is the C's array with its NULL terminator dropped:
/// element 0 is the longest common prefix, elements 1.. are the matches in
/// generator order. `None` is the C's NULL — no matches, or an allocation
/// failure.
///
/// `genfunc` is `&mut dyn FnMut` rather than a function pointer because a
/// generator's scan state is now its own to carry; see
/// [`FilenameCompletionState`].
pub fn completion_matches(text: &str, genfunc: &mut CompleteFunc) -> Option<Vec<String>> {
    // Steps 1 and 2. The state argument is the number of matches collected so
    // far, so the first call passes 0 — the generator's "start a new scan"
    // signal — and later calls pass 1, 2, 3, …
    let mut match_list: Vec<String> = Vec::new();
    let mut matches = 0usize;
    let mut match_list_len = 1usize;

    while let Some(retstr) = genfunc(text, matches as i32) {
        if match_list.is_empty() {
            // Index 0 is left reserved for the common prefix and the first
            // match lands at index 1, so the first match seeds a placeholder
            // the tail of this function overwrites.
            match_list.push(String::new());
        }
        // Allow for the list terminator here. The first iteration lands on
        // four slots.
        if matches.saturating_add(3) >= match_list_len {
            while matches.saturating_add(3) >= match_list_len {
                match_list_len = match_list_len.saturating_mul(2);
            }
            if match_list
                .try_reserve_exact(match_list_len.saturating_sub(match_list.len()))
                .is_err()
            {
                // ERR-completion-06: the C frees the ARRAY only, so every
                // match string it already holds and the one just generated
                // all leak. The observable outcome — NULL, indistinguishable
                // from "no matches" — is what is reproduced; the leak is an
                // artifact of the C's representation and has nothing to
                // reproduce in a `Vec<String>`, which drops its strings here.
                return None;
            }
        }
        match_list.push(retstr);
        matches += 1;
    }

    // Step 3. Still empty exactly when the very first call returned NULL, i.e.
    // when there are no matches at all.
    if match_list.is_empty() {
        return None;
    }

    // Step 4: the longest common prefix. The comparison is always against
    // element 1 — the C's local is named `prevstr` and is never reassigned,
    // which happens to be correct. Byte-wise, case-sensitive, no multibyte
    // awareness. `max_equal` only ever shrinks and starts at the length of
    // element 1, so no read runs past any string's NUL: a shorter element
    // stops the inner loop on its own terminator.
    let first = cstr(&match_list[1]).as_bytes();
    let mut max_equal = first.len();
    for other in match_list.iter().skip(2) {
        let other = cstr(other).as_bytes();
        let mut i = 0usize;
        while i < max_equal && first[i] == other.get(i).copied().unwrap_or(0) {
            i += 1;
        }
        max_equal = i;
    }

    // Step 5. The cut is byte-wise and can land inside a multibyte
    // character; a `String` cannot hold that, so it is floored to the
    // preceding character boundary. Element 0 stays a genuine prefix of
    // element 1, at most three bytes shorter than the C's. With exactly one
    // match `max_equal` is the whole of element 1 and nothing is lost, which
    // is what keeps `fn_complete2`'s unique-match test working.
    let cut = match std::str::from_utf8(&first[..max_equal]) {
        Ok(_) => max_equal,
        Err(e) => e.valid_up_to(),
    };
    let prefix_bytes = &first[..cut];
    let mut retstr = String::new();
    if retstr.try_reserve_exact(max_equal + 1).is_err() {
        // Again ERR-completion-06: the C frees the array and leaks every
        // match string, and again the observable part is the NULL.
        return None;
    }
    retstr.push_str(std::str::from_utf8(prefix_bytes).unwrap_or_default());
    match_list[0] = retstr;

    // Step 6. The C's trailing NULL slot has no counterpart in a `Vec`; the
    // slack it guarantees is what makes `fn_complete2`'s speculative read of
    // `matches[2]` in bounds for arrays built here.
    Some(match_list)
}

// [spec:libedit:def:filecomplete.fn-qsort-string-compare-fn]
// [spec:libedit:sem:filecomplete.fn-qsort-string-compare-fn]
/// C: `static int _fn_qsort_string_compare(const void *i1, const void *i2)`.
///
/// The `qsort` callback of [`fn_display_match_list`], which loads a
/// `char *` out of each array element and returns `strcasecmp`. Typed here
/// as the elements themselves; the `strcasecmp` sign convention is kept.
fn _fn_qsort_string_compare(i1: &str, i2: &str) -> i32 {
    // `strcasecmp` folds case byte by byte through `LC_CTYPE` and has no
    // multibyte awareness, so bytes above 0x7f effectively compare by value:
    // in every locale this port models, a single byte at or above 0x80 is
    // never a letter, so only ASCII folds (ERR-completion-21). The C string
    // ends at the first NUL, which then compares below every other byte.
    let s1 = cstr(i1).as_bytes();
    let s2 = cstr(i2).as_bytes();
    // Past the end is the terminator, which is why the shorter of two strings
    // sharing a prefix sorts first. Index `n` is past the end of at most one
    // of them, so the pair is compared one position beyond the overlap and no
    // further.
    let folded = |s: &[u8], i: usize| s.get(i).map_or(0, |&b| i32::from(b.to_ascii_lowercase()));
    (0..=s1.len().min(s2.len()))
        .map(|i| (folded(s1, i), folded(s2, i)))
        .find(|(c1, c2)| c1 != c2)
        .map_or(0, |(c1, c2)| c1 - c2)
}

// [spec:libedit:def:filecomplete.fn-display-match-list-fn]
// [spec:libedit:sem:filecomplete.fn-display-match-list-fn]
/// C: `void fn_display_match_list(EditLine *el, char **matches, size_t num,
/// size_t width, const char *(*app_func)(const char *))`.
///
/// `matches` is `&mut` because the C sorts it in place. `num` stays a
/// parameter: it counts `matches[0]`, which is not one of the strings
/// printed, so it is not the slice length.
pub fn fn_display_match_list(
    el: &mut EditLine,
    matches: &mut [String],
    num: usize,
    width: usize,
    app_func: Option<AppFunc>,
) {
    // Step 1.
    let screenwidth = el.el_terminal.t_size.h;
    let app_func = app_func.unwrap_or(append_char_function);

    // Step 2: ignore `matches[0]` and avoid the 1-based array logic below.
    //
    // ERR-completion-02, disposition *define — treat it as a caller error and
    // reject it*: the C's `num--` has no lower bound, so `num == 0` wraps to
    // `SIZE_MAX` and the loops walk far off the end of the array. Here a
    // `num` of 0, an empty `matches`, or a `num` beyond what `matches` holds
    // all print nothing.
    if num == 0 {
        return;
    }
    let Some(matches) = matches.get_mut(1..) else {
        return;
    };
    let num = (num - 1).min(matches.len());

    // Step 3: how many entries fit on a line, counting one space between
    // strings and the single appended character. A non-positive
    // `screenwidth` is not defended against and is benign: it sign-extends
    // into an enormous `cols`, hence one line holding everything.
    let mut cols = (screenwidth as usize) / width.saturating_add(2);
    if cols == 0 {
        cols = 1;
    }

    // Step 4: lines of output, rounded up. `div_ceil` is the C's
    // `(num + cols - 1) / cols` without its overflow when `cols` is enormous.
    let lines = num.div_ceil(cols);

    // Step 5: sort in place, which MUTATES the caller's array.
    // `fn_complete2` depends on the pointers being merely permuted so that
    // its free loop still covers every string; here the strings are owned by
    // the slice, so a permutation is all a sort can be. The original element
    // 0 was excluded by step 2 and keeps its position. Ties — names
    // differing only in case — are left wherever the sort puts them; the
    // rule says their order is unspecified and must not be relied on.
    matches[..num].sort_by(|a, b| _fn_qsort_string_compare(a, b).cmp(&0));

    // Step 6: on the ith line print elements i, i+lines, i+lines*2, … —
    // column-major, so reading DOWN a column gives sorted order.
    let mut out: Vec<u8> = Vec::new();
    for line in 0..lines {
        for col in 0..cols {
            let thisguy = line + col * lines;
            if thisguy >= num {
                break;
            }
            let s = cstr(&matches[thisguy]);
            if col != 0 {
                out.push(b' ');
            }
            out.extend_from_slice(s.as_bytes());
            out.extend_from_slice(app_func(s).as_bytes());
            // C: `fprintf(el->el_outfile, "%-*s", (int)(width -
            // strlen(...)), "")` — that many padding spaces, emitted after
            // the last column too, so every line carries trailing
            // whitespace. ERR-completion-03, disposition *define*: a `width`
            // smaller than the longest string underflows in the C and casts
            // to a huge or negative field width; here it saturates to no
            // padding at all.
            out.resize(out.len() + width.saturating_sub(s.len()), b' ');
        }
        out.push(b'\n');
    }
    el.write_outfile(&out);
}

// [spec:libedit:def:filecomplete.find-word-to-complete-fn]
// [spec:libedit:sem:filecomplete.find-word-to-complete-fn]
/// C: `static wchar_t * find_word_to_complete(const wchar_t *cursor, const
/// wchar_t *buffer, const wchar_t *word_break, const wchar_t
/// *special_prefixes, size_t *length, int do_unescape)`.
///
/// `cursor` is an offset into `buffer`, per the pointer-into-a-buffer
/// convention in the crate docs. `word_break` is never NULL in the C —
/// `wcschr` is called on it unguarded — while `special_prefixes` is
/// NULL-checked.
fn find_word_to_complete(
    cursor: usize,
    buffer: &[u32],
    word_break: &[u32],
    special_prefixes: Option<&[u32]>,
    length: &mut usize,
    do_unescape: bool,
) -> Option<Vec<u32>> {
    // ERR-completion-05 (a NULL `word_break` handed to `wcschr`) cannot be
    // expressed: `&[u32]` is never null. An empty slice is the closest thing
    // and simply matches nothing but a 0 character.

    // Step 1: if the cursor sits just after a backslash or a quote, step back
    // over it so the scan carries on through the word that precedes it.
    let mut ctemp = cursor;
    if ctemp > 0 && matches!(buffer[ctemp - 1], BSLASH | SQUOTE | DQUOTE) {
        ctemp -= 1;
    }

    // Step 2: scan backwards.
    loop {
        if ctemp == 0 {
            break;
        }
        // This test comes FIRST, so a backslash-escaped word-break character
        // does not end the word: in `a\ b` the whole `a\ b` is the word.
        if ctemp >= 2 && buffer[ctemp - 2] == BSLASH && needs_escaping(buffer[ctemp - 1]) {
            ctemp -= 2;
            continue;
        }
        if wcschr(word_break, buffer[ctemp - 1]) {
            break;
        }
        // `special_prefixes` IS NULL-checked, and this function treats the
        // two sets identically — only the caller distinguishes them.
        if let Some(sp) = special_prefixes
            && wcschr(sp, buffer[ctemp - 1])
        {
            break;
        }
        ctemp -= 1;
    }

    // Step 3: measured from the ORIGINAL cursor, so it includes the trailing
    // quote or backslash step 1 stepped over.
    let mut len = cursor - ctemp;

    // Step 4: a lone quote at the cursor means an empty word starting after
    // the quote.
    if len == 1 && matches!(buffer[ctemp], SQUOTE | DQUOTE) {
        len = 0;
        ctemp += 1;
    }

    // Step 5: the RAW span in the line, before any unescaping. This is what
    // `fn_complete2` hands to `el_deletestr`, so the count of characters
    // deleted can exceed the length of the string returned whenever escapes
    // were removed (ERR-completion-19). It is written before the allocation
    // below, so a caller must not assume it is untouched on failure.
    *length = len;

    let span = &buffer[ctemp..ctemp + len];

    // Step 6.
    if do_unescape {
        return unescape_string(span);
    }
    let mut temp = Vec::new();
    temp.try_reserve_exact(len + 1).ok()?;
    temp.extend_from_slice(span);
    temp.push(0);
    Some(temp)
}

/// C: `char what_to_do = el->el_state.lastcmd == el->el_state.thiscmd ? '?' :
/// '\t'` — step 1 of [`fn_complete2`].
///
/// "A second consecutive invocation of this same command lists the
/// possibilities" is the whole protocol, and it is carried by two fields
/// named for neither it nor each other. Named here so the consequence is
/// visible at the reading rather than at the debugging: an editor whose two
/// command slots happen to agree — a freshly zeroed one, say — takes the
/// listing path on its very first invocation.
///
/// ERR-completion-23: the result is never `*` or `!`, so the header comment's
/// `*` behaviour is unimplemented and the `!` tests are dead code — neither is
/// ported.
fn what_to_do(el: &EditLine) -> char {
    if el.el_state.lastcmd == el.el_state.thiscmd {
        '?'
    } else {
        '\t'
    }
}

// [spec:libedit:def:filecomplete.fn-complete2-fn]
// [spec:libedit:sem:filecomplete.fn-complete2-fn]
/// C: `int fn_complete2(EditLine *el, char *(*complete_func)(const char *,
/// int), char **(*attempted_completion_function)(const char *, int, int),
/// const wchar_t *word_break, const wchar_t *special_prefixes, const char
/// *(*app_func)(const char *), size_t query_items, int *completion_type, int
/// *over, int *point, int *end, unsigned int flags)`.
///
/// Every NULL-checked parameter is an `Option`; the return is the C's
/// `CC_*` code, so it stays an `i32`.
#[expect(
    clippy::too_many_arguments,
    reason = "the compatibility completion operation has twelve independently optional inputs"
)]
pub fn fn_complete2(
    el: &mut EditLine,
    complete_func: Option<&mut CompleteFunc>,
    attempted_completion_function: Option<AttemptedCompletionFunc>,
    word_break: &[u32],
    special_prefixes: Option<&[u32]>,
    app_func: Option<AppFunc>,
    query_items: usize,
    completion_type: Option<&mut i32>,
    over: Option<&mut i32>,
    point: Option<&mut i32>,
    end: Option<&mut i32>,
    flags: u32,
) -> i32 {
    let mut retval = i32::from(CC_NORM);
    let do_unescape = flags & FN_QUOTE_MATCH != 0;

    // Step 1.
    let what_to_do = what_to_do(el);

    // Step 2: readline's `rl_completion_type` has to be told what we did.
    if let Some(ct) = completion_type {
        *ct = what_to_do as i32;
    }

    // Step 3: default the callbacks. The generator's default is deferred to
    // its use below, because in this port it needs a scan state to go with
    // it.
    let app_func: AppFunc = app_func.unwrap_or(append_char_function);

    // Step 4. `el_wline` hands out a live view of `el->el_line`; the three
    // members it exposes are read directly here.
    let mut len = 0usize;
    let Some(temp) = find_word_to_complete(
        el.el_line.cursor,
        &el.el_line.buffer,
        word_break,
        special_prefixes,
        &mut len,
        do_unescape,
    ) else {
        // Nothing changed.
        return retval;
    };

    // Step 5: written BEFORE any user callback runs, because readline
    // callbacks read `rl_point` and `rl_end`. ERR-completion-13, disposition
    // *reproduce*: both are counts of WIDE characters while the strings the
    // callbacks receive are multibyte, so in a non-ASCII locale the offsets
    // do not index those strings.
    let cur_off = el.el_line.cursor as i32;
    if let Some(p) = point {
        *p = cur_off;
    }
    if let Some(e) = end {
        *e = el.el_line.lastchar as i32;
    }

    // The word, encoded to multibyte through `el->el_scratch`. The C hands
    // the callbacks a pointer into that buffer which the next encode
    // invalidates; this owns a copy instead, which no caller can tell apart
    // because retaining it was never allowed. `None` covers both the C's NULL
    // return and a byte string that is not UTF-8, neither of which can be
    // handed to a `&str` callback; the C would pass NULL and the callee would
    // fault.
    let encoded = ct_encode_string(Some(&temp), &mut el.el_scratch)
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(str::to_owned);

    // Step 6.
    let mut matches: Option<Vec<String>> = None;
    if let Some(acf) = attempted_completion_function
        && let Some(word) = encoded.as_deref()
    {
        matches = acf(word, cur_off - len as i32, cur_off);
    }

    // Step 7: fall back to the built-in path. ERR-completion-15, disposition
    // *reproduce*: with `over == NULL` and a NULL attempted result there is
    // NO fallback and completion simply does nothing.
    let over_permits_fallback = over.as_deref().is_some_and(|v| *v == 0);
    if (attempted_completion_function.is_none() || (over_permits_fallback && matches.is_none()))
        && let Some(word) = encoded.as_deref()
    {
        matches = match complete_func {
            Some(f) => completion_matches(word, f),
            None => {
                // The C's default generator reads one process-wide set of
                // statics (ERR-completion-17). The core has no globals,
                // so the scan state is created here and lives exactly as
                // long as this one drive-to-exhaustion — which is what
                // makes a nested or concurrent completion safe, the
                // hazard the rule asks the port to decide about
                // explicitly.
                // `move` because [`CompleteFunc`] is an unparameterised
                // `dyn`, hence `'static`: the state has to live inside
                // the closure rather than beside it.
                let mut scan = FilenameCompletionState::default();
                let mut default_gen =
                    move |t: &str, s: i32| fn_filename_completion_function(&mut scan, t, s);
                completion_matches(word, &mut default_gen)
            }
        };
    }

    // Step 8.
    if let Some(o) = over {
        *o = 0;
    }

    // Step 9.
    let Some(mut matches) = matches else {
        return retval;
    };

    // Step 10. ERR-completion-01, disposition *define — test the array length
    // safely*: because `matches[2]` is tested first, a two-element array from
    // a caller's own completion function (`{prefix, NULL}`, i.e. a one-element
    // `Vec` here) is read out of bounds in the C. An element past the end
    // reads as the NULL terminator would, which is also what an array from
    // `completion_matches` really holds there.
    let single_match = matches.get(2).is_none()
        && matches
            .get(1)
            .is_none_or(|m1| cstr(&matches[0]) == cstr(m1));

    // Step 11.
    retval = i32::from(CC_REFRESH);

    // C: `matches[0][0] != '\0'` — so an element 0 that merely *starts* with a
    // NUL is empty too, which is what taking the C's view of it says. A
    // missing element 0 — an empty array from a caller's completion function,
    // which the C would dereference as NULL — reads as the empty string.
    let lcd_empty = matches.first().is_none_or(|s| cstr(s).is_empty());

    // Step 12.
    if !lcd_empty {
        // (a) Remove the raw word span. `len == 0` is a no-op.
        el_deletestr(el, len as i32);

        // (b) The ordering with (a) is load-bearing: the deletion happens
        // first, so `escape_filename` scans a line the partial word has
        // already left.
        let completion = if flags & FN_QUOTE_MATCH != 0 {
            escape_filename(el, &matches[0], single_match, Some(app_func))
        } else {
            let mut s = String::new();
            match s.try_reserve_exact(matches[0].len() + 1) {
                Ok(()) => {
                    s.push_str(&matches[0]);
                    Some(s)
                }
                Err(_) => None,
            }
        };

        // (c) ERR-completion-11, disposition *reproduce*: the word has
        // already been deleted from the line and is NOT restored, so a failed
        // completion silently eats the user's partial word, and the failure
        // surfaces as an ordinary `CC_REFRESH`.
        let Some(completion) = completion else {
            return retval;
        };

        // (d) Replace the completed string with the common part of all
        // possible matches. A `None` decode is the C's NULL, which
        // `el_winsertstr` rejects exactly as it rejects an empty string. The
        // copy is what lets the scratch borrow end before `el` is handed on.
        let decoded = ct_decode_string(Some(completion.as_bytes()), &mut el.el_scratch)
            .map(<[u32]>::to_vec)
            .unwrap_or_default();
        el_winsertstr(el, &decoded);

        // (e) ERR-completion-14, disposition *reproduce*: the append string
        // goes in only here, so a caller passing neither an attempted
        // function nor `FN_QUOTE_MATCH` gets no append character at all. Note
        // `app_func` is applied to the INSERTED string, not to `matches[0]`.
        if single_match && attempted_completion_function.is_some() && flags & FN_QUOTE_MATCH == 0 {
            let appended = app_func(&completion);
            let decoded = ct_decode_string(Some(appended.as_bytes()), &mut el.el_scratch)
                .map(<[u32]>::to_vec)
                .unwrap_or_default();
            el_winsertstr(el, &decoded);
        }
        // (f) `completion` drops here.
    }

    // Step 13.
    if !single_match && what_to_do == '?' {
        // (a) Walk `matches[1]` onward. The C's walk stops at the NULL
        // terminator the `Vec` does not carry, so it stops at the end.
        let maxlen = matches
            .iter()
            .skip(1)
            .map(|m| cstr(m).len())
            .max()
            .unwrap_or(0);
        let matches_num = matches.len() - 1;

        // (b) Get onto the next line from the command line.
        el.write_outfile(b"\n");

        // (c) Too many items: ask for confirmation. The prompt must be on the
        // wire before the read, which is the C's `fflush`.
        let mut match_display = true;
        if matches_num > query_items {
            el.write_outfile(
                format!("Display all {matches_num} possibilities? (y or n) ").as_bytes(),
            );
            if getc_stdin() != Some(b'y') {
                match_display = false;
            }
            el.write_outfile(b"\n");
        }

        // (d) The `+ 1` restores the 1-based convention that function
        // expects. This SORTS `matches[1..]` in place.
        if match_display {
            fn_display_match_list(el, &mut matches, matches_num + 1, maxlen, Some(app_func));
        }

        // (e)
        retval = i32::from(CC_REDISPLAY);
    } else if !lcd_empty {
        // Some common match, but not complete enough. The next tab prints the
        // possibilities.
        el_beep(el);
    } else {
        // The common prefix is empty, so nothing was inserted and further
        // specification is needed.
        el_beep(el);
        retval = i32::from(CC_NORM);
    }

    // Steps 14 and 15: the C frees every element up to the NULL terminator
    // and then the array, element 0 included — and it does that to an array a
    // caller's own `attempted_completion_function` returned, too. Ownership
    // of that array and all its strings transfers here unconditionally, which
    // is exactly what taking the `Vec<String>` by value says. `matches` and
    // `temp` drop here.
    retval
}

// [spec:libedit:def:filecomplete.fn-complete-fn]
// [spec:libedit:sem:filecomplete.fn-complete-fn]
/// C: `int fn_complete(EditLine *el, ...)` — [`fn_complete2`] with `flags`
/// derived from whether an `attempted_completion_function` was supplied.
#[expect(
    clippy::too_many_arguments,
    reason = "this compatibility wrapper forwards the completion operation without hiding any input"
)]
pub fn fn_complete(
    el: &mut EditLine,
    complete_func: Option<&mut CompleteFunc>,
    attempted_completion_function: Option<AttemptedCompletionFunc>,
    word_break: &[u32],
    special_prefixes: Option<&[u32]>,
    app_func: Option<AppFunc>,
    query_items: usize,
    completion_type: Option<&mut i32>,
    over: Option<&mut i32>,
    point: Option<&mut i32>,
    end: Option<&mut i32>,
) -> i32 {
    // When the application supplies its own attempted function the match it
    // produced is inserted verbatim and the application is trusted to have
    // quoted it; otherwise the built-in filename path runs and the inserted
    // match goes through `escape_filename`.
    let flags = if attempted_completion_function.is_some() {
        0
    } else {
        FN_QUOTE_MATCH
    };
    fn_complete2(
        el,
        complete_func,
        attempted_completion_function,
        word_break,
        special_prefixes,
        app_func,
        query_items,
        completion_type,
        over,
        point,
        end,
        flags,
    )
}

// [spec:libedit:def:filecomplete.el-fn-complete-fn]
// [spec:libedit:sem:filecomplete.el-fn-complete-fn]
/// C: `unsigned char _el_fn_complete(EditLine *el, int ch)` — the editor
/// command wrapper, bound as a key action. `ch` is unused, as in the C.
pub fn _el_fn_complete(el: &mut EditLine, ch: i32) -> u8 {
    let _ = ch;
    // NULL for both `complete_func` and `attempted_completion_function`
    // means: the built-in generator, `append_char_function`, no special
    // prefixes, a query threshold of 100, nothing reported back, and — by
    // `fn_complete`'s flag rule — `FN_QUOTE_MATCH`.
    //
    // Every `CC_*` code `fn_complete2` can produce fits in an `unsigned
    // char`, so the narrowing cast never loses information.
    fn_complete(
        el,
        None,
        None,
        BREAK_CHARS,
        None,
        None,
        100,
        None,
        None,
        None,
        None,
    ) as u8
}

// [spec:libedit:def:filecomplete.el-fn-sh-complete-fn]
// [spec:libedit:sem:filecomplete.el-fn-sh-complete-fn]
/// C: `unsigned char _el_fn_sh_complete(EditLine *el, int ch)`.
pub fn _el_fn_sh_complete(el: &mut EditLine, ch: i32) -> u8 {
    // A separate exported symbol with no behaviour of its own
    // (ERR-completion-23): applications and key-binding tables can name a
    // "shell-style" completion command distinctly, and in this version it
    // does the same thing. Both symbols must be kept.
    _el_fn_complete(el, ch)
}

#[cfg(test)]
#[path = "filecomplete/test.rs"]
mod test;
