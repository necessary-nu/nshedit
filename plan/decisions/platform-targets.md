---
id [dec:libedit:platform-targets]
epitome "Linux is proven, macOS is a supported target now, Windows is a planned target for the native surface only; the C ABI skin stays ELF."
state @decided
category @executive
scope {
    elements ([arch:libedit:platform] [arch:libedit:c-abi] [arch:libedit:terminal-caps])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Stay Linux-only."
        rejected_because "nsh serves users where they work. macOS is POSIX, ships libedit as a system library, and the workspace is a handful of marked cfg arms away from building there — every gap is already annotated in the tree."
    }
    {
        option "Port the C ABI skin to Windows along with the native surface."
        rejected_because "No Windows program links libedit.so.2; the drop-in contract has no consumers there. On Windows the product is nshedit's native Rust API feeding nsh, not the compatibility skin."
    }
    {
        option "Treat Windows as a separate editor project."
        rejected_because "The core editor and renderer are OS-agnostic Rust. Only the platform seam (nshedit-plat) and the capability source (nshterm) differ; modern Windows consoles speak VT through ConPTY, so the render model transfers. Same crates, new backends."
    }
)
consequences {
    accepted (
        "nshedit-plat grows true Darwin arms: passwd and sigaction/sigset layouts, spelled from the stable Darwin ABI and self-checked with target-gated layout assertions."
        "nshedit-abi grows the Darwin stdio data symbols (__stdinp/__stdoutp/__stderrp) and a Mach-O export/install-name story; the ELF shape gates gain Mach-O counterparts."
        "Final macOS proof (test suite, oracle build, differential traces) requires macOS hardware or CI; until then Darwin support is compile-proven from Linux via a cross-check gate."
        "Windows support is scoped to the native surface: a console backend behind nshedit-plat (VT modes via ConPTY), a builtin VT capability profile in nshterm that bypasses terminfo discovery, and console-event delivery into the driver's signal model. nshedit-abi is explicitly out of scope on Windows."
        "posix-only-scope is unaffected: it governs which C sources were ported, not which platforms the Rust product targets. Its 'the target is POSIX' reading is narrowed to the C ABI skin by this decision."
        "macOS drop-in is source and link compatibility, not replacement of the system library: we ship libnshedit.0.dylib under our own install name with the libedit link names beside it, and never claim /usr/lib/libedit.3.dylib, which is served from the dyld shared cache and protected by System Integrity Protection. Recorded as [spec:nshedit:req:abi.darwin-drop-in]; this is the deferred install-name question, decided in macos-contract."
        "The Darwin termios projection is Darwin's own shape — NCCS 20, a 64-bit tcflag_t, the BSD V* subscripts, separate c_ispeed/c_ospeed — and not glibc's NCCS 32, because a macOS caller's struct termios is Darwin's. A terminal behaviour Darwin does not define leaves the platform representation table rather than resolving to a Linux bit. Recorded as [spec:nshedit:req:platform.darwin-termios]."
    )
    deferred (
        "Windows user-identity and ~user expansion semantics without a passwd database — decided in windows-contract."
    )
}
edges {
    requires ([dec:libedit:platform-layer])
}
---

## Rationale

The workspace's platform commitment was always narrower than its
architecture. The core editor, renderer, history, and completion are
OS-agnostic Rust; everything platform-shaped was pushed behind
nshedit-plat by [dec:libedit:platform-layer], and every Linux-only
assumption that remains is individually marked in the tree — the
`cfg(target_os = "linux")` stdio block in nshedit-abi, the glibc-shaped
`Passwd` and `SigAction` layouts, glibc's `NCCS = 32`. That containment
is what makes target expansion a bounded piece of work rather than a
rewrite.

macOS is POSIX and ships libedit; supporting it is the same product on
a second POSIX. Windows is a different terminal lineage, but the modern
console (ConPTY, Windows Terminal) speaks VT sequences, which is the
render model nshedit already targets — the seam is input, modes, and
events, not the drawing model. The C ABI skin is the one component with
no Windows counterpart, and it stays behind.

## What macOS drop-in means

`packaging/install.sh` installs one object and claims libedit's filenames
beside it — `libedit.so`, `libedit.so.0`, `libedit.so.2` — because on ELF the
SONAME a consumer recorded is a *filename the loader searches for*, so a
symlink is all it takes to serve a binary built before we existed.
`conformance/soname.sh` is the proof: it links a consumer against a real
libedit and runs it against our install alone.

Neither mechanism exists on macOS. Mach-O records the dependency's **install
name**, which for the system library is the absolute path
`/usr/lib/libedit.3.dylib`; dyld resolves `/usr/lib` entries out of the shared
cache without consulting the filesystem, and System Integrity Protection
forbids writing there in any case. A symlink cannot intercept it and a build
that stamped that install name onto our dylib would be stamping an address
nothing will ever ask us for. `crates/nshedit-abi/build.rs` already emits
`@rpath/libnshedit.0.dylib` for the Apple targets, which is the shape that
actually works: the consumer's own rpath decides where we are found.

So macOS drop-in is the compile-and-link claim, not the
already-installed-binary claim: the generated `histedit.h` and
`editline/readline.h` compile unchanged, `-ledit` against our libdir resolves
through the `libedit.dylib` / `libedit.3.dylib` link names we install beside
our own, and the Mach-O export set is gated the way `abi-shape.sh` gates the
ELF one. A consumer that wants us instead of the system library relinks or
sets its rpath; that is the only route macOS offers anyone, Apple included.

## Which termios Darwin gets

`nshedit-plat/src/termios.rs` pins `NCCS = 32` because that is glibc's, and
`def:tty.el-tty-t` describes the `struct termios` libedit compiles against.
The reasoning generalises to the wrong conclusion on Darwin: what that rule
freezes is *the platform's* termios, and Darwin's is a different record —
`NCCS` 20, `tcflag_t` 64 bits wide, the 4.4BSD `V*` subscripts, and the line
speed in `c_ispeed`/`c_ospeed` rather than in `c_cflag`'s `CBAUD` field, which
Darwin does not define at all. Projecting glibc's shape there would put the
user's erase character at a subscript that holds their kill character, and
would read a baud rate out of bits that carry flow control.

Darwin therefore gets its own arm throughout, and the names the two systems do
not share stop being universal: `iuclc`, `olcuc`, `xcase`, `cbaud`, `cibaud`
and `xtabs` leave the table on Darwin, `ccts_oflow`, `crts_iflow`, `cignore`,
`mdmbuf`, `nokerninfo`, `altwerase` and `onoeot` join it, and `setty` reports
an unknown mode for whichever the target system lacks. That is what libedit's
own `#ifdef`-guarded `ttymodes[]` does, arrived at by a table the platform
layer builds rather than by the preprocessor.
