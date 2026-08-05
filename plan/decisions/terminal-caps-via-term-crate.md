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
        "We own tputs: term's expander enters its delay state on a bare $ and discards everything through the next >, so padding must be re-parsed, computed from output baud, and emitted by us — and tgoto must cut the string at every $ so the runs survive expansion."
        "Capabilities are addressed by terminfo capname — the two-to-five-character code, not the long name. Every TermInfo constructor parses with longnames false, so the strings/bools/numbers maps are keyed that way. The C's 39 string and 8 flag/numeric termcap codes are translated to capnames once, at the source, not at runtime."
        "term's searcher walks the filesystem terminfo tree only; it does not read the ncurses hashed terminfo.db. On a host that ships only the hashed database no entry is found and the port behaves as it does for an unknown TERM."
        "term 1.2.x becomes a load-bearing dependency of the terminal layer."
        "Padding is only as good as the line speed the tty layer reports, so tputs is downstream of the platform layer: with no tcgetattr, t_speed is 0 and every delay collapses to nothing. See [dec:libedit:platform-layer]."
    )
    deferred (
        "term reads TERMINFO, TERMINFO_DIRS and $HOME/.terminfo straight from the process environment, behind libedit's own secure_getenv guard. Whether a set-uid consumer needs that suppressed — and whether the port must supply its own searcher to do it — is open."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
    related_to ([dec:libedit:platform-layer])
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
more of the `%` grammar than `tgoto` ever had. The sixth is not there,
and the way it is missing matters. `term`'s expander enters its delay
state on a bare `$` — not on `$<` — and then discards every byte through
the next `>`. So it strips padding, and it will also swallow ordinary
text out of any capability that happens to contain a `$`. `tgoto`
therefore cuts its input at every `$`, expands the runs between, and
copies the padding runs through untouched; `tputs` then parses
`$<N[*][/]>` itself, derives the pad count from the output baud rate and
the affected-line count, and emits the pad character.

That turns out to be a gift rather than a chore. The C wraps `tputs` in
a global mutex under `_REENTRANT` for one reason: `tputs` takes a
`putc`-style callback with no user-data parameter, so the destination
`FILE*` has to be a global. Writing the function ourselves means passing
a writer, and the global and its mutex both cease to exist.

The naming change is the deeper one. libedit's capability tables are
termcap two-letter codes; terminfo is the X/Open Curses standard
interface and the codes are the BSD-era layer over it. We translate once
while authoring, so the Rust tables carry terminfo **capnames** — `il1`,
`bel`, `clear`, `lines`, `cols`, `xenl` — and termcap is simply gone,
not preserved and shimmed. Capname and not long name: every `TermInfo`
constructor `term` exposes parses with `longnames` false, so `strings`,
`bools` and `numbers` are keyed by the short code, and the ANSI fallback
entry `from_name` synthesises is keyed the same way. This is nominally
Wave 4 shaping applied during Wave 1, and it is deliberate: it changes
what the `sem` rules for the terminal layer should say.

Three of the eight flag and numeric codes are worth naming, because the
obvious reading of each is wrong. `xt` needs no translation at all:
termcap `xt` and terminfo `xt` (`dest_tabs_magic_smso`) are the same
capname. `pt` is not absent from terminfo either — it is `OTpt`
(`has_hardware_tabs`), one of the obsolete termcap-compatibility
booleans, and ncurses does store it: 139 of the 1843 entries in a
current Debian `/usr/share/terminfo` set it, `screen` among them. So
`tgetflag("pt")` is live under ncurses, not dead, and a table carrying
the literal `"pt"` reads 0 where the C reads 1. `MT` is `OTMT`
(`gnu_has_meta_key`), which no entry in that database sets, so it reads
0 either way and the C agrees. The registered defects that describe
these — ERR-terminal-61 and ERR-terminal-62 — are wrong on the same
points and need correcting with them.

The six rules under `[spec:libedit:*:terminal.t*-fn]` are the surviving
trace of the boundary. They were extracted from the C's `#if
defined(__sun)` prototype block, so they describe foreign functions
rather than libedit code — but they are exactly the contract the Rust
capability module must satisfy, so they stay in the work-list and their
impl sites are our module, not ncurses'. The duplicate set that
`src/sys.h` declared is out of scope entirely under
[[posix-only-scope]].
