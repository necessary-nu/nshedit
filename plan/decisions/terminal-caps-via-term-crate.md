---
id [dec:libedit:terminal-caps-via-term-crate]
epitome "The in-workspace nshterm crate owns terminfo parsing, padding, secure discovery, and termcap-name compatibility without linking a C provider."
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
        option "Link libtinfo or ncurses as the C implementation does."
        rejected_because "This violates [dec:libedit:no-c-ffi] and reintroduces provider global state and callback constraints."
    }
    {
        option "Continue depending on the unmaintained term crate."
        rejected_because "The needed parser and expander are load-bearing, while its unrelated colour half and unmaintained release cadence are not acceptable dependencies."
    }
    {
        option "Use terminfo capnames internally and drop legacy termcap names."
        rejected_because "The public compatibility operations accept the legacy two-letter namespace, including live obsolete capabilities such as OTpt. Dropping it changes defined C behaviour."
    }
    {
        option "Implement ncurses' optional hashed database now."
        rejected_because "No supported Linux package set ships it, it requires the legacy Berkeley DB 1.85 compatibility format, and carrying an otherwise unused database parser would add input and maintenance surface without closing a supported-platform gap."
    }
)
consequences {
    accepted (
        "nshterm is an in-workspace pure-Rust crate containing the terminfo database parser, searcher, parameter expansion, and compatibility name data."
        "The terminal layer uses typed terminfo capabilities internally while the compatibility boundary resolves the termcap names accepted by libedit."
        "Padding markers survive parameter expansion and are emitted by the Rust tputs implementation according to output speed and affected lines; no global putc destination is required."
        "TERMINFO, TERMINFO_DIRS, and HOME-derived search paths are ignored for a privileged process according to the secure environment guard."
        "Filesystem terminfo trees are the supported database layout. The current Linux package matrix does not require ncurses' opt-in hashed layout, so nshterm does not probe or parse it."
    )
    deferred (
        "A future platform that ships only a hashed terminfo database must specify and verify its database format before entering the support matrix."
    )
}
edges {
    requires ([dec:libedit:no-c-ffi])
    related_to ([dec:libedit:platform-layer] [dec:libedit:conformance-policy])
}
codifies (
    [spec:libedit:def:terminal.tgetent-fn]
    [spec:libedit:def:terminal.tputs-fn]
)
establishes ([arch:libedit:terminal-caps])
---

## Rationale

libedit reaches its terminal database through six termcap-shaped operations,
but linking their C provider is unnecessary. The parsing and parameter
expansion machinery now lives in `nshterm`, which the workspace owns and can
shape around the editor's actual contract.

Internal capability identity and compatibility input spelling are separate
concerns. Typed terminfo names are appropriate inside the renderer; the ABI
must still accept the two-letter termcap names deployed callers pass. Padding
is similarly preserved as structured information until `tputs` knows the
writer speed, avoiding the C provider's global callback destination.

Secure environment discovery and the compatibility name table are settled.
The database-format survey was resolved on 2026-08-09 against the only
supported system ABI, Linux. The pinned packaging recipes for
[Debian](https://salsa.debian.org/debian/ncurses/-/blob/759ec4a183d0c560c638816787fc048a8f4b92f9/debian/rules),
[Fedora](https://src.fedoraproject.org/rpms/ncurses/blob/b2310702e7f62c3fc284bb4da2b5f17834028b0e/f/ncurses.spec),
[Alpine](https://gitlab.alpinelinux.org/alpine/aports/-/blob/dd997c499619e93e6e7bcef26c2482e37d433a04/main/ncurses/APKBUILD),
[Arch](https://gitlab.archlinux.org/archlinux/packaging/packages/ncurses/-/blob/d4793eded7e1af898845a294ea9e7df7617346fa/PKGBUILD), and
[NixOS](https://github.com/NixOS/nixpkgs/blob/ec8f6bc8a8c48a3fff7dd3e26e938766262f2557/pkgs/development/libraries/ncurses/default.nix)
all configure or package filesystem trees. Gentoo goes further: its
[current ebuild](https://github.com/gentoo/gentoo/blob/aa25856d23516a8e5dc3589dd87986c49a94178a/sys-libs/ncurses/ncurses-6.6_p20260411.ebuild)
passes `--without-hashed-db` while documenting that Berkeley DB is being
phased out.

Ncurses leaves the option disabled by default. Its
[installation notes](https://invisible-island.net/ncurses/INSTALL.html#with_hashed_db)
say that enabling it replaces the tree written by `tic` with a database using
the Berkeley DB 1.85 compatibility interface. That makes a reader possible,
but not free: it is a second untrusted binary format and dependency surface.
No current supported deployment needs it, so implementing it would be
speculative rather than compatibility work. The decision is conditional but
not vague: adding a platform that actually ships only that layout reopens the
format as an admission dependency for that platform.
