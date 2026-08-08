//! Ported from `src/parse.c`; rules live in `docs/spec/port/src/parse.md`.

use crate::domain::{Text, TextUnit};
use crate::editor::{Tokenization, Tokenizer};
use crate::el::{EditLine, el_editmode};
use crate::hist::hist_command;
use crate::map::map_bind;
use crate::search::el_match;
use crate::terminal::{terminal_echotc, terminal_settc, terminal_telltc};
use crate::tty::tty_stty;

/// The C's `s[i]` on a NUL-terminated `const wchar_t *`: one element past the
/// content reads as the terminator.
///
/// Every wide string in this module is a slice of *content*, so the C's NUL
/// is not stored. Reading past the end as 0 makes the translations below read
/// exactly like the C's pointer walks while staying in bounds, and it is also
/// what defines the two out-of-bounds reads the C performs here —
/// `ERR-input-11` (`p[1]` on an empty string) and `ERR-input-10` (the `\U+`
/// run walking off the end). A caller that hands over a slice which does
/// carry its terminator gets the same answers, because a stored 0 and the end
/// of the slice are treated alike.
fn at(s: &[u32], i: usize) -> u32 {
    match s.get(i) {
        Some(&c) => c,
        None => 0,
    }
}

/// The C string inside a slice: the content up to the first NUL, or all of it
/// if there is none. This is `wcslen`, made total.
fn wcs(s: &[u32]) -> &[u32] {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    &s[..n]
}

/// `wcscmp(a, b) == 0`, over slices that may or may not carry a terminator.
fn wcs_eq(a: &[u32], b: &[u32]) -> bool {
    wcs(a) == wcs(b)
}

/// `wcscmp` against a command name. Every name in the table is ASCII, so a
/// byte-wise comparison is exactly the C's wide comparison — exact, case
/// sensitive, no abbreviation and no locale folding.
fn name_eq(name: &'static str, word: &[u32]) -> bool {
    name.len() == word.len() && name.bytes().zip(word).all(|(a, &b)| u32::from(a) == b)
}

/// One digit of the C's `const wchar_t hex[] = L"0123456789ABCDEF"` lookup.
/// Upper case only, because `a`-`f` are not in that table.
///
/// The C uses `wcschr`, which counts the table's own terminating NUL, so an
/// input NUL is accepted as a digit worth sixteen (`ERR-input-10`). That is
/// undefined behaviour — an out-of-bounds read of the input — so it is not
/// reproduced: 0 is not a hex digit here.
fn hexdigit(c: u32) -> Option<u32> {
    match u8::try_from(c) {
        Ok(b @ b'0'..=b'9') => Some(u32::from(b - b'0')),
        Ok(b @ b'A'..=b'F') => Some(u32::from(b - b'A') + 10),
        _ => None,
    }
}

// [spec:libedit:def:parse.func-fn]
// [spec:libedit:sem:parse.func-fn]
/// C: `int (*func)(EditLine *, int, const wchar_t **)` — the handler member
/// of the file-static `cmds[]` dispatch table, which is the whole editrc
/// command vocabulary.
///
/// The C declares it inside an anonymous struct, so there is nothing to name
/// but the pointer type; the table itself is data and belongs with
/// [`el_wparse`], its only reader. Every handler returns 0 on success and -1
/// on failure, and receives `el_wparse`'s own `argc` and `argv` unchanged.
pub(crate) type ParseFuncT = fn(&mut EditLine, i32, &[&[u32]]) -> i32;

/// One row of the C's file-static `cmds[]`.
struct Cmd {
    name: &'static str,
    func: ParseFuncT,
}

/// C: `static const struct { ... } cmds[]` — the seven editrc commands.
///
/// The C's `{ NULL, NULL }` sentinel is the array length here; it is the only
/// load-bearing part of the table's shape, since lookup is an exact `wcscmp`
/// and the order of the seven is therefore irrelevant. (The comment block at
/// the head of `parse.c` lists `gettc` and omits `telltc`; the comment is
/// wrong — this is the table.)
///
/// Two handlers do not have the C's shared `(el, argc, argv)` shape in this
/// port and are reached through the adapters below: `el_editmode` drops
/// `argc`, and `hist_command` keeps the C's raw `const wchar_t **`.
static CMDS: [Cmd; 7] = [
    Cmd {
        name: "bind",
        func: map_bind,
    },
    Cmd {
        name: "echotc",
        func: terminal_echotc,
    },
    Cmd {
        name: "edit",
        func: cmd_editmode,
    },
    Cmd {
        name: "history",
        func: cmd_history,
    },
    Cmd {
        name: "telltc",
        func: terminal_telltc,
    },
    Cmd {
        name: "settc",
        func: terminal_settc,
    },
    Cmd {
        name: "setty",
        func: tty_stty,
    },
];

/// `edit` — [`el_editmode`] with the C's `argc` dropped, which
/// `sem:el.el-editmode-fn` records as observationally equivalent to its
/// `argc != 2` rejection.
fn cmd_editmode(el: &mut EditLine, _argc: i32, argv: &[&[u32]]) -> i32 {
    el_editmode(el, argv)
}

/// `history` — [`hist_command`], which keeps the C's `const wchar_t **argv`
/// as a raw pointer array (`sem:hist.hist-command-fn`), so the slice of words
/// is rebuilt into that shape: each word NUL terminated, the array NULL
/// terminated, both alive for the duration of the call. `hist_command` reads
/// `argv[1]` and `argv[2]` and retains nothing, so the copy is not observable.
fn cmd_history(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    let words: Vec<Vec<u32>> = argv
        .iter()
        .map(|w| {
            let mut v = wcs(w).to_vec();
            v.push(0);
            v
        })
        .collect();
    let mut ptrs: Vec<*const u32> = words.iter().map(|w| w.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    hist_command(el, argc, ptrs.as_ptr())
}

fn wide_text(text: &Text) -> Vec<u32> {
    text.as_units()
        .iter()
        .map(|unit| match unit {
            TextUnit::Scalar(character) => u32::from(*character),
            TextUnit::RawByte(byte) => u32::from(*byte),
            TextUnit::CompatibilityWide(value) => value.get(),
        })
        .collect()
}

/// Parse one physical editrc line into owned wide words.
fn editrc_words(line: &[u32]) -> Option<Vec<Vec<u32>>> {
    let input: Text = line
        .iter()
        .copied()
        .take_while(|unit| *unit != 0)
        .map(TextUnit::from_wide)
        .collect();
    let cursor = input.index(input.len()).ok()?;
    let Tokenization::Complete(parsed) = Tokenizer::default().tokenize(&input, cursor).ok()? else {
        return None;
    };

    // A final unquoted backslash-newline is a request for another physical
    // line, not a completed logical command. An ordinary newline finishes a
    // token at the newline's index; an escaped one reaches the input end.
    if input.as_units().last() == Some(&TextUnit::Scalar('\n'))
        && parsed
            .tokens()
            .last()
            .is_some_and(|token| token.source().end().get() == input.len())
    {
        return None;
    }

    Some(
        parsed
            .tokens()
            .iter()
            .map(|token| wide_text(token.value()))
            .collect(),
    )
}

// [spec:libedit:def:parse.parse-line-fn]
// [spec:libedit:sem:parse.parse-line-fn]
/// Tokenize one editrc line and dispatch it through [`el_wparse`].
///
/// Native tokenization returns owned words and typed continuation reasons, so
/// neither unchecked C failure is representable here: there is no nullable
/// tokenizer allocation (`ERR-input-13`), and incomplete quote/escape syntax
/// cannot expose uninitialised `argc` or `argv` (`ERR-input-14`). A malformed
/// line is reported as -1, which stops `el_source` just like an unknown
/// command.
pub(crate) fn parse_line(el: &mut EditLine, line: &[u32]) -> i32 {
    let Some(storage) = editrc_words(line) else {
        return -1;
    };
    let words: Vec<&[u32]> = storage.iter().map(Vec::as_slice).collect();
    el_wparse(el, words.len() as i32, &words)
}

// [spec:libedit:def:parse.el-wparse-fn]
// [spec:libedit:sem:parse.el-wparse-fn]
/// Command dispatcher: match `argv[0]` (after any `prog:` qualifier)
/// against the command table and run the handler, negating its result. The
/// C's NULL terminator on `argv` is the slice length here.
///
/// The `prog:` qualifier is a **substring and regex test, not an equality
/// test**: the text before the first colon is the pattern and the program
/// name passed to `el_init` is the subject, so a line beginning `sh:` applies
/// to a program named `bash` and a qualifier of `.` applies to everything.
///
/// The C's `el_calloc` for the qualifier copy cannot fail here, so its
/// "return 0 on out of memory" path is unreachable rather than absent; every
/// other return the rule lists is reproduced, including the lossiness (0 is
/// both "not for this program" and "the command succeeded", -1 is both "empty
/// line" and "unknown command").
///
/// `argc < 1` is the C's guard; an empty `argv` is added to it. The C reads
/// `argv[0]` unconditionally after that guard and would dereference the
/// array's NULL terminator if a caller passed a truthful `argc` over a short
/// array (`ERR-input-14` reaches this with an indeterminate `argc`); the
/// slice makes that reachable in bounds, and it is defined as the same -1.
pub fn el_wparse(el: &mut EditLine, argc: i32, argv: &[&[u32]]) -> i32 {
    if argc < 1 {
        return -1;
    }
    let Some(&arg0) = argv.first() else {
        return -1;
    };
    let arg0 = wcs(arg0);

    // `wcschr(argv[0], L':')` — the *first* colon only, so `foo:bar:bind`
    // yields qualifier `foo` and command word `bar:bind`.
    let ptr: &[u32] = match arg0.iter().position(|&c| c == u32::from(b':')) {
        None => arg0,
        // The colon is the first character (`:bind`): the line is silently
        // ignored, and that is not an error.
        Some(0) => return 0,
        Some(l) => {
            let tprog: Vec<u32> = {
                let mut v = arg0[..l].to_vec();
                v.push(0);
                v
            };
            let prog: Vec<u32> = {
                let mut v = wcs(&el.el_prog).to_vec();
                v.push(0);
                v
            };
            // Note the argument order: the program name is the subject and
            // the qualifier is the pattern.
            if el_match(prog.as_ptr(), tprog.as_ptr()) == 0 {
                return 0;
            }
            &arg0[l + 1..]
        }
    };

    // Exact `wcscmp` against each of the seven, so the table's order does not
    // matter and neither does a duplicate name: there are none.
    let Some(cmd) = CMDS.iter().find(|c| name_eq(c.name, ptr)) else {
        return -1;
    };
    // The handler gets the original, unmodified `argc` and `argv`: `argv[0]`
    // still carries the `prog:` prefix, because the qualifier was stripped
    // only into a temporary for the match.
    let i = (cmd.func)(el, argc, argv);
    -i
}

// [spec:libedit:def:parse.parse-escape-fn]
// [spec:libedit:sem:parse.parse-escape-fn]
/// Decode one `^<char>`, `\<odigit>`, `\<char>` or `\U+xxxx` escape and
/// return its value, or -1 if the escape is malformed. `ptr` is the C's
/// `const wchar_t **`: a cursor the call advances past what it consumed.
///
/// Reproduced defects:
///
/// - `ERR-input-36` — the two-character rule. The blanket "there must be at
///   least two characters here" test runs *before* the form is decided, so an
///   ordinary one-character literal at the end of the string is rejected:
///   `L"a"` is -1 while `L"ax"` is `'a'`. This is why `setty erase=X` stores
///   `(cc_t)-1` = 0xFF and `setty erase=^H` works.
/// - `ERR-input-37` — the `\U+` form consumes **one character more** than the
///   escape text and discards it: `\U+0041x` consumes 8 characters, the `x`
///   included. The rule leaves the choice open; per
///   `plan/decisions/conformance-policy.md` this is defined behaviour that is
///   merely wrong, so it is frozen rather than fixed here.
///
/// Defined, because the C is undefined:
///
/// - `ERR-input-11` — the `p[1]` test reads without checking `p[0]`, so an
///   empty string is read one past the terminator. Defined: an empty input is
///   -1, cursor unchanged.
/// - `ERR-input-10` — the `\U+` hex-digit test accepts the terminating NUL as
///   a digit worth sixteen, so four hex digits at end of string do not fail;
///   they return the value shifted up a nibble with bit 4 forced on
///   (`\U+0041` measures 0x410, not 0x41) and leave the cursor two elements
///   past the terminator. With one to three digits the remaining iterations
///   read past the terminator outright. Defined as the rule directs: four or
///   five upper-case hex digits are required and end of string inside the run
///   is a malformed escape, -1 with the cursor unchanged. Because the frozen
///   `ERR-input-37` over-consumption needs a character to consume, this makes
///   a `\U+` escape at the very end of a string -1 as well, which is the same
///   answer the C already gives for `\U+` and `\U+0` and a deterministic one
///   for the four- and five-digit cases it gets wrong.
pub(crate) fn parse_escape(ptr: &mut &[u32]) -> i32 {
    let p: &[u32] = ptr;

    // The two-character rule: ERR-input-36, and ERR-input-11 defined.
    if at(p, 1) == 0 {
        return -1;
    }

    // The C's `p`, as an index. Every form leaves it on the last character it
    // consumed, and the common tail below advances one past that.
    let mut i = 0usize;

    let c: u32 = if at(p, 0) == u32::from(b'\\') {
        i += 1;
        let e = at(p, i);
        match u8::try_from(e) {
            // A. The named escapes, lower case only: `\E` is not ESC, it
            // falls into form D and yields `'E'`.
            Ok(b'a') => 0x07, // Bell
            Ok(b'b') => 0x08, // Backspace
            Ok(b't') => 0x09, // Horizontal Tab
            Ok(b'n') => 0x0a, // New Line
            Ok(b'v') => 0x0b, // Vertical Tab
            Ok(b'f') => 0x0c, // Form Feed
            Ok(b'r') => 0x0d, // Carriage Return
            Ok(b'e') => 0x1b, // Escape
            // C. `\U+xxxx` / `\U+xxxxx`. Upper case `U` only; `\u` is form D.
            Ok(b'U') => {
                i += 1;
                if at(p, i) != u32::from(b'+') {
                    return -1;
                }
                i += 1;
                let mut v: u32 = 0;
                for k in 0..5 {
                    match hexdigit(at(p, i)) {
                        Some(d) => {
                            v = (v << 4) | d;
                            i += 1;
                        }
                        // The first four are mandatory: `\U+41zz` fails, it
                        // is not read as two digits.
                        None if k < 4 => return -1,
                        // The fifth is optional; it is not consumed here, and
                        // the tail below consumes it as the discarded
                        // character of ERR-input-37.
                        None => break,
                    }
                }
                // The C's only validation — surrogates and noncharacters
                // pass. Five upper-case hex digits cannot exceed 0xFFFFF, so
                // this is unreachable once ERR-input-10 is defined away; it
                // is kept because it is the specified check.
                if v > 0x10FFFF {
                    return -1;
                }
                // ERR-input-10: end of string inside the run is malformed.
                // The escape text is complete but the over-consumed character
                // (ERR-input-37) is not there.
                if at(p, i) == 0 {
                    return -1;
                }
                v
            }
            // B. Octal, at most three digits, stopping at the first character
            // that is not `0`-`7`: `\18` is value 1 then a literal `8`.
            Ok(b'0'..=b'7') => {
                let mut v: u32 = 0;
                for _ in 0..3 {
                    let ch = at(p, i);
                    if !(u32::from(b'0')..=u32::from(b'7')).contains(&ch) {
                        break;
                    }
                    v = (v << 3) | (ch - u32::from(b'0'));
                    i += 1;
                }
                // Three digits reach 0777, so `\400`..`\777` are rejected.
                if v & 0xffff_ff00 != 0 {
                    return -1;
                }
                // The C's `--p`, back onto the last digit consumed; the first
                // digit is always consumed, so this cannot underflow.
                i -= 1;
                v
            }
            // D. Anything else after the backslash is itself: `\\` is 0x5C,
            // `\q` is `'q'`, and `\M` is `'M'`, which is what defeats
            // `parse_string`'s meta form.
            _ => e,
        }
    } else if at(p, 0) == u32::from(b'^') {
        // E. `^X`. The mask clears bits 5 and 6 and keeps bit 7, so `^a` and
        // `^A` are both 1, `^` U+00E9 is 0x89, and everything above bit 7 is
        // discarded.
        i += 1;
        let x = at(p, i);
        if x == u32::from(b'?') {
            0x7f
        } else {
            x & 0o237
        }
    } else {
        // F. A literal, one character — subject to the two-character rule.
        at(p, 0)
    };

    *ptr = &p[i + 1..];
    c as i32
}

// [spec:libedit:def:parse.parse-string-fn]
// [spec:libedit:sem:parse.parse-string-fn]
/// Decode a whole key-binding string from `in` into `out`, returning the
/// written prefix of `out`, or `None` if any escape was malformed. There is
/// no output bound in the C either; `out` must hold `in.len() + 1`.
///
/// The returned slice is the decoded characters. The terminating NUL the C
/// writes is written too, at `out[n]`, so the buffer requirement is the C's —
/// but it is not part of the slice, because the decoded string may contain
/// embedded NULs (`^@`, `\0` and `\U+0000` all decode to 0) and the length is
/// how a caller sees them at all. Read back as a C string it truncates at the
/// first one, which is what `map_bind` does.
///
/// On the `None` path the output buffer is left partially written and
/// unterminated, as in the C; the caller must not read it.
///
/// `ERR-input-12` — the C resumes decoding past the end of its input after a
/// `\U+` escape at end of string left the cursor beyond the terminator, and
/// keeps going through adjacent memory until it happens on a zero. That falls
/// out with `ERR-input-10`: such an escape is -1 here, so the loop stops.
pub(crate) fn parse_string<'a>(out: &'a mut [u32], r#in: &[u32]) -> Option<&'a [u32]> {
    let mut inp: &[u32] = r#in;
    let mut n = 0usize;

    loop {
        let ch = at(inp, 0);
        if ch == 0 {
            // The only success exit.
            out[n] = 0;
            return Some(&out[..n]);
        }
        if ch == u32::from(b'\\') || ch == u32::from(b'^') {
            let v = parse_escape(&mut inp);
            if v == -1 {
                return None;
            }
            out[n] = v as u32;
            n += 1;
        } else if ch == u32::from(b'M') && at(inp, 1) == u32::from(b'-') && at(inp, 2) != 0 {
            // The meta form: emit ESC and step past the `M-` only. The
            // character after it goes round the loop again, so `M-^A` is ESC
            // 0x01 and `M-M-a` is ESC ESC `a`. Capital `M`, literal `-`, and
            // unescaped, only.
            out[n] = 0x1b;
            n += 1;
            inp = &inp[2..];
        } else {
            // Everything else verbatim, including an `M` not followed by `-`
            // and a trailing `M-`, which is not an error.
            out[n] = ch;
            n += 1;
            inp = &inp[1..];
        }
    }
}

// [spec:libedit:def:parse.parse-cmd-fn]
// [spec:libedit:sem:parse.parse-cmd-fn]
/// Return the command number for a command name, or -1 if there is none.
///
/// First match wins and `map_addfunc` performs no uniqueness check, so a
/// user function registered under a built-in's name stays unreachable from
/// `bind` — reproduced, since the search order is the C's.
///
/// `ERR-modes-11` — `map_addfunc` does not check its `wcsdup`s, so the C can
/// leave a NULL `name` in the help table for this loop to hand to `wcscmp`.
/// The rule's own advice is taken structurally: `ElBindingsT::name` is an
/// owned string that cannot be absent, so there is nothing to guard here.
/// `nfunc` is likewise bounded by the table it counts rather than trusted, so
/// an over-large `nfunc` cannot read past the end.
pub(crate) fn parse_cmd(el: &mut EditLine, cmd: &[u32]) -> i32 {
    for b in el.el_map.help.iter().take(el.el_map.nfunc) {
        if wcs_eq(&b.name, cmd) {
            return b.func;
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::el::blank_editline;

    /// A wide string from ASCII source text, as every caller of this module
    /// has after tokenising an editrc line.
    fn w(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }

    /// [`parse_escape`] over `s`: the value it returned and how many
    /// characters it consumed. The consumed count is half the contract — the
    /// cursor is how `parse_string` finds the next escape — and it is where
    /// both of this function's frozen defects show.
    fn esc(s: &str) -> (i32, usize) {
        let v = w(s);
        let mut p: &[u32] = &v;
        let rv = parse_escape(&mut p);
        (rv, v.len() - p.len())
    }

    /// [`parse_string`] over `s`, with the output buffer the C requires.
    fn dec(s: &str) -> Option<Vec<u32>> {
        let r#in = w(s);
        let mut out = vec![0u32; r#in.len() + 1];
        parse_string(&mut out, &r#in).map(<[u32]>::to_vec)
    }

    // [spec:libedit:sem:parse.parse-line-fn/test]
    #[test]
    fn native_editrc_words() {
        assert_eq!(
            editrc_words(&w("bind 'x y'")),
            Some(vec![w("bind"), w("x y")])
        );
        assert_eq!(editrc_words(&w("bind 'x y")), None);
        assert_eq!(editrc_words(&w("bind x\\\n")), None);
        assert_eq!(
            editrc_words(&w("bind x\nignored")),
            Some(vec![w("bind"), w("x")])
        );

        let non_scalar = [u32::from(b'x'), 0xd800];
        assert_eq!(editrc_words(&non_scalar), Some(vec![non_scalar.to_vec()]));
    }

    /// The dispatch table is the entire editrc vocabulary, and a name paired
    /// with the wrong handler survives any differential that sources a file
    /// whose lines all parse: `settc` and `telltc` transposed would still be
    /// seven names looked up by exact `wcscmp`. Pinning the pairing is the
    /// only way to catch that.
    ///
    /// The comment block at the head of `parse.c` lists `gettc` and omits
    /// `telltc`. The comment is wrong; the table is what runs.
    // [spec:libedit:sem:parse.func-fn/test]
    #[test]
    fn each_command_name_reaches_its_own_handler() {
        let expected: [(&str, ParseFuncT); 7] = [
            ("bind", map_bind),
            ("echotc", terminal_echotc),
            ("edit", cmd_editmode),
            ("history", cmd_history),
            ("telltc", terminal_telltc),
            ("settc", terminal_settc),
            ("setty", tty_stty),
        ];
        assert_eq!(CMDS.len(), expected.len(), "seven commands, no more");
        for (row, (name, func)) in CMDS.iter().zip(expected) {
            assert_eq!(row.name, name);
            assert!(std::ptr::fn_addr_eq(row.func, func), "{name}");
        }
        assert!(
            !CMDS.iter().any(|c| c.name == "gettc"),
            "there is no gettc command and never was"
        );

        // The handler protocol, which is the rest of what this type is: 0 for
        // success and -1 for failure, negated on the way out. `edit` with no
        // operand is the cheapest -1 in the table — `el_editmode` rejects any
        // `argv` that is not exactly two words before it touches anything.
        let mut el = blank_editline();
        // The three descriptors a `calloc`ed `EditLine` leaves at 0 are the
        // process's standard input; -1 is this crate's "no stream", so a
        // handler that reports an error writes nowhere.
        el.el_errfd = -1;
        el.el_outfd = -1;
        let edit = w("edit");
        assert_eq!(
            el_wparse(&mut el, 1, &[&edit]),
            1,
            "a handler's -1 reaches the caller as +1"
        );
        let gettc = w("gettc");
        assert_eq!(
            el_wparse(&mut el, 1, &[&gettc]),
            -1,
            "an unknown name is -1, which is what stops el_source"
        );
    }

    /// The blanket "there must be at least two characters here" test runs
    /// before the form is decided, so a one-character literal at the end of a
    /// string is rejected while the same character followed by anything is
    /// not (ERR-input-36). This is why `setty erase=X` stores `(cc_t)-1` and
    /// `setty erase=^H` works, and it is invisible to any differential that
    /// only feeds well-formed editrc lines.
    ///
    /// The empty input is the C's `p[1]` read past the terminator
    /// (ERR-input-11), defined here as the same -1 with the cursor unmoved.
    // [spec:libedit:sem:parse.parse-escape-fn/test]
    #[test]
    fn a_lone_trailing_character_is_a_malformed_escape() {
        assert_eq!(esc(""), (-1, 0));
        assert_eq!(esc("a"), (-1, 0));
        assert_eq!(esc("ax"), (i32::from(b'a'), 1));
        assert_eq!(esc("^"), (-1, 0));
        assert_eq!(esc("\\"), (-1, 0));
    }

    /// The named escapes are lower case only, so `\E` is not ESC — it falls
    /// through to "anything else after a backslash is itself" and yields
    /// `'E'`. The same fall-through is what makes `\M` a literal `M`, which
    /// is the documented way to defeat [`parse_string`]'s meta form.
    #[test]
    fn the_named_escapes_are_lower_case_only() {
        assert_eq!(esc("\\a").0, 0x07);
        assert_eq!(esc("\\e").0, 0x1b);
        assert_eq!(esc("\\r").0, 0x0d);
        assert_eq!(esc("\\E"), (i32::from(b'E'), 2));
        assert_eq!(esc("\\M"), (i32::from(b'M'), 2));
        assert_eq!(esc("\\\\"), (0x5c, 2));
    }

    /// At most three octal digits, stopping at the first character that is
    /// not `0`-`7`; three digits reach 0777, and everything above 0377 is
    /// rejected outright rather than truncated. `\18` is value 1 followed by
    /// a literal `8`, so the cursor must stop after two characters.
    #[test]
    fn an_octal_escape_stops_at_three_digits_and_rejects_the_overflow() {
        assert_eq!(esc("\\101"), (0x41, 4));
        assert_eq!(esc("\\377"), (0xff, 4));
        assert_eq!(esc("\\18"), (1, 2));
        assert_eq!(esc("\\400"), (-1, 0), "0400 is above a byte");
        assert_eq!(esc("\\777"), (-1, 0));
        // Four digits are three digits and a literal: 0101 then `1`.
        assert_eq!(esc("\\1011"), (0x41, 4));
    }

    /// The `^` form masks with 0237, which clears bits 5 and 6 and keeps bit
    /// 7 — so `^a` and `^A` are the same control character, and a character
    /// above U+007F keeps its high bit rather than being rejected.
    #[test]
    fn the_control_form_masks_rather_than_validates() {
        assert_eq!(esc("^A"), (0x01, 2));
        assert_eq!(esc("^a"), (0x01, 2));
        assert_eq!(esc("^?"), (0x7f, 2), "DEL is the one special case");
        assert_eq!(esc("^@"), (0x00, 2), "^@ is an embedded NUL");
        assert_eq!(esc("^\u{e9}"), (0x89, 2));
    }

    /// `\U+` needs four upper-case hex digits and takes an optional fifth,
    /// and it consumes **one character more than the escape text** — the
    /// character after the digits is eaten and discarded (ERR-input-37). That
    /// is defined C behaviour that is merely wrong, so it is frozen rather
    /// than fixed, and nothing but a unit test looks at it.
    ///
    /// End of string inside the digit run is -1 (ERR-input-10 defined): the C
    /// counts its lookup table's own terminator as a digit worth sixteen and
    /// walks off the input, which is undefined and is not reproduced.
    #[test]
    fn a_unicode_escape_eats_the_character_after_it() {
        assert_eq!(esc("\\U+0041x"), (0x41, 8), "the trailing x is consumed");
        assert_eq!(esc("\\U+00041x"), (0x41, 9), "five digits, then the x");
        assert_eq!(esc("\\U+0041"), (-1, 0), "no character left to discard");
        assert_eq!(esc("\\U+41zz"), (-1, 0), "the first four are mandatory");
        assert_eq!(esc("\\U0041x"), (-1, 0), "the plus is mandatory");
        assert_eq!(esc("\\u+0041x"), (i32::from(b'u'), 2), "upper case only");
        assert_eq!(esc("\\U+abcdx"), (-1, 0), "the hex table has no lower case");
        assert_eq!(
            esc("\\U+D800x"),
            (0xd800, 8),
            "the only validation is the 0x10FFFF ceiling, so surrogates pass"
        );
    }

    /// The meta form is capital `M`, a literal `-`, and a character after it:
    /// it emits ESC and steps past the `M-` only, so what follows goes round
    /// the loop again and may be another escape or another `M-`. An `M` that
    /// is not part of one is verbatim, and a trailing `M-` is not an error.
    // [spec:libedit:sem:parse.parse-string-fn/test]
    #[test]
    fn the_meta_form_prefixes_an_escape_and_rescans_the_rest() {
        assert_eq!(dec("M-a"), Some(vec![0x1b, u32::from(b'a')]));
        assert_eq!(dec("M-^A"), Some(vec![0x1b, 0x01]));
        assert_eq!(dec("M-M-a"), Some(vec![0x1b, 0x1b, u32::from(b'a')]));
        assert_eq!(dec("M"), Some(vec![u32::from(b'M')]));
        assert_eq!(
            dec("M-"),
            Some(vec![u32::from(b'M'), u32::from(b'-')]),
            "a trailing M- decodes verbatim rather than failing"
        );
        assert_eq!(
            dec("\\M-a"),
            Some(vec![u32::from(b'M'), u32::from(b'-'), u32::from(b'a')]),
            "an escaped M is a literal M, which defeats the meta form"
        );
    }

    /// The decoded string is a length and not a C string: `^@`, `\0` and
    /// `\U+0000` all decode to 0, and only the returned length distinguishes
    /// an embedded NUL from the end of the text. The terminator the C writes
    /// is still there, one past the last character, which is why `map_bind`
    /// reading the result back as a C string truncates.
    #[test]
    fn an_embedded_nul_is_kept_and_only_the_length_reveals_it() {
        let r#in = w("^@a");
        let mut out = vec![0xdead_beefu32; r#in.len() + 1];
        let got = parse_string(&mut out, &r#in).expect("well formed");
        assert_eq!(got, [0, u32::from(b'a')]);
        assert_eq!(out[2], 0, "the C's terminator, past the returned slice");
        assert_eq!(dec("\\0x"), Some(vec![0, u32::from(b'x')]));
    }

    /// One malformed escape fails the whole string, and the loop stops there.
    /// That is what closes ERR-input-12: with `\U+` at end of string defined
    /// as -1 rather than left consuming past the terminator, there is no
    /// resumption through adjacent memory to reproduce.
    #[test]
    fn a_malformed_escape_fails_the_whole_string() {
        assert_eq!(dec("\\"), None);
        assert_eq!(dec("ab\\"), None);
        assert_eq!(dec("a\\U+0041"), None);
        assert_eq!(dec("\\400"), None);
    }
}
