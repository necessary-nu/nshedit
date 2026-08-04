---
id [dec:libedit:terminal-caps-via-term-crate]
epitome "Terminal capabilities come from terminfo via the term crate; we write tputs ourselves and drop termcap naming."
state @decided
category @existence
scope {
    elements ([arch:libedit:terminal-caps])
    rules (
        [spec:libedit:def:terminal.tgetent-fn]
        [spec:libedit:def:terminal.tgetstr-fn]
        [spec:libedit:def:terminal.tgetflag-fn]
        [spec:libedit:def:terminal.tgetnum-fn]
        [spec:libedit:def:terminal.tgoto-fn]
        [spec:libedit:def:terminal.tputs-fn]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "FFI to libtinfo or libncurses, as the C does."
        rejected_because "Barred by [dec:libedit:no-c-ffi]."
    }
    {
        option "The terminfo crate (meh/rust-terminfo), which has a typed capability API."
        rejected_because "WTFPL, which is not a license we will ship against. Its typed API also fits libedit's table-driven capability access worse than raw maps do."
    }
    {
        option "Hand-roll a terminfo database parser and search path."
        rejected_because "term already does the parsing, the ncurses-compatible database discovery, and the parameterized-string expansion, under MIT OR Apache-2.0. Only tputs is genuinely missing."
    }
)
consequences {
    accepted (
        "We own tputs: term recognises $<...> delays but discards them, so padding must be computed from output baud and emitted by us."
        "Capabilities are addressed by terminfo long name. The C's 39 string and 8 flag/numeric termcap codes are translated once, at the source, not at runtime."
        "term 1.2.x becomes a load-bearing dependency of the terminal layer."
    )
    deferred (
        "Whether term's searcher covers every terminfo database layout we care about; the ncurses hashed terminfo.db is a distinct format from the directory tree."
        "The termcap codes MT, pt and xt have no clean terminfo counterpart and are resolved per-capability during the port."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
}
codifies (
    [spec:libedit:def:terminal.tgetent-fn]
    [spec:libedit:def:terminal.tputs-fn]
)
establishes ([arch:libedit:terminal-caps])
---

## Rationale

libedit's single hard external library was a termcap provider — the C
`configure` demands `tgetent` from ncurses, curses, termcap or tinfo and
refuses to build without one. The entire dependency is six functions:
`tgetent`, `tgetstr`, `tgetflag`, `tgetnum`, `tgoto`, `tputs`. There is
no terminfo API in the C at all; no `setupterm`, no `tigetstr`, no
`tparm`.

Since [[no-c-ffi]] rules out linking, those six become ours. Five of
them are thin over `term`: `TermInfo::from_env` and the `searcher`
module cover `tgetent`, the public `strings`/`bools`/`numbers` maps
cover the three lookups, and `parm::expand` covers `tgoto` with rather
more of the `%` grammar than `tgoto` ever had. The sixth is not there.
`term`'s expander sees `$<`, enters a delay state, and skips to the
closing `>` without reading the number — so capability strings come back
with their padding stripped. We parse `$<N[*][/]>` ourselves, derive the
pad count from the output baud rate and the affected-line count, and
emit the pad character.

That turns out to be a gift rather than a chore. The C wraps `tputs` in
a global mutex under `_REENTRANT` for one reason: `tputs` takes a
`putc`-style callback with no user-data parameter, so the destination
`FILE*` has to be a global. Writing the function ourselves means passing
a writer, and the global and its mutex both cease to exist.

The naming change is the deeper one. libedit's capability tables are
termcap two-letter codes; terminfo is the X/Open Curses standard
interface and the codes are the BSD-era layer over it. We translate once
while authoring, so the Rust tables carry terminfo names and termcap is
simply gone — not preserved and shimmed. This is nominally Wave 4
shaping applied during Wave 1, and it is deliberate: it changes what
the `sem` rules for the terminal layer should say.

The six rules under `[spec:libedit:*:terminal.t*-fn]` are the surviving
trace of the boundary. They were extracted from the C's `#if
defined(__sun)` prototype block, so they describe foreign functions
rather than libedit code — but they are exactly the contract the Rust
capability module must satisfy, so they stay in the work-list and their
impl sites are our module, not ncurses'. The duplicate set that
`src/sys.h` declared is out of scope entirely under
[[posix-only-scope]].
