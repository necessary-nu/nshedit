---
id [dec:libedit:platform-targets]
epitome "Linux support is an enumerated x86-64 matrix — glibc, and musl since [dec:libedit:linux-musl-target]; macOS supports Intel and Apple silicon, and Windows supports the native Rust surface only."
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
    {
        option "Promise support for every target whose operating system is Linux."
        rejected_because "Linux triples disagree on pointer width, libc records and symbols, signal layouts and numbers, C char signedness, and whether Rust can emit the shared library the compatibility product requires. An operating-system cfg is not evidence for an ABI."
    }
)
consequences {
    accepted (
        "The supported Linux matrix is enumerated rather than inferred, and every entry carries its own acceptance evidence. It contained exactly x86_64-unknown-linux-gnu when this was decided — its x86-64 glibc passwd and sigaction layouts, signed C char, ELF cdylib, exports, installer, loader, and unchanged C consumer are all exercised on the target that ships them — and [dec:libedit:linux-musl-target] later added x86_64-unknown-linux-musl on the same terms."
        "nshedit-plat rejects every other Linux ABI and Android at compile time. A new Linux triple is a platform port with its own transcriptions and acceptance evidence, not another value added to a broad cfg arm."
        "nshedit-plat grows true Darwin arms: passwd and sigaction/sigset layouts, spelled from the stable Darwin ABI and self-checked with target-gated layout assertions."
        "nshedit-abi grows the Darwin stdio data symbols (__stdinp/__stdoutp/__stderrp) and a Mach-O export/install-name story; the ELF shape gates gain Mach-O counterparts."
        "Final macOS proof (native test suite, direct C consumer, Mach-O exports, install names, and runtime linking) requires macOS hardware or CI; until then Darwin support is compile-proven from Linux via a cross-check gate."
        "Windows support is scoped to the native surface: nshedit-plat classifies real console and stream handles, decodes structured console input, and enables VT processing for console output; the editor reuses its existing ANSI profile and treats ConPTY as a byte-stream host. nshedit-abi is explicitly out of scope on Windows. [dec:libedit:windows-terminal-boundary] owns the complete boundary."
        "posix-only-scope records the completed source-port boundary, not which platforms the maintained Rust product targets. Its 'the target is POSIX' reading is narrowed to the C ABI by this decision."
        "macOS drop-in is source and link compatibility, not replacement of the system library: we ship libnshedit.0.dylib under our own install name with the libedit link names beside it, and never claim /usr/lib/libedit.3.dylib, which is served from the dyld shared cache and protected by System Integrity Protection. Recorded as [spec:nshedit:req:abi.darwin-drop-in]; this is the deferred install-name question, decided in macos-contract."
        "The Darwin termios projection is Darwin's own shape — NCCS 20, a 64-bit tcflag_t, the BSD V* subscripts, separate c_ispeed/c_ospeed — and not glibc's NCCS 32, because a macOS caller's struct termios is Darwin's. A terminal behaviour Darwin does not define leaves the platform representation table rather than resolving to a Linux bit. Recorded as [spec:nshedit:req:platform.darwin-termios]."
    )
    deferred (
        "Legacy Windows consoles without VT processing and a Windows C ABI remain outside the supported target."
        "Other Linux architectures, x32, and Android remain unsupported until separately specified and proven. musl was deferred here on the same terms and has since been specified and proven, by [dec:libedit:linux-musl-target]."
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

## Why Linux names its triples

The maintained Linux evidence is specifically x86-64 glibc. `Passwd` and
`SigAction` carry sizes and offsets produced by that C compiler and checked
against that system's headers; the conformance harness then verifies the ELF
shared object, generated headers, compatibility names, loader path, and a
direct C consumer on the same target. That evidence cannot be generalized by
changing `target_arch` or `target_env` in a cfg. Adding a triple means
producing that evidence again for it, which is what
[dec:libedit:linux-musl-target] did for musl: its own layout assertions
against musl's own headers, and its own acceptance run on a musl host.

The rejected cases fail for different reasons: armv7 has different pointer-
sized record layouts, and s390x exposes assumptions that C `char` is signed.
Those are separate ports if they ever become valuable. Until then the build
rejects them before an x86-64 transcription can masquerade as their ABI.

**Correction.** This section originally also listed
`x86_64-unknown-linux-musl`, on the grounds that it "drops the `cdylib`
needed by the C compatibility product". That describes the target's default
configuration and not the target: musl targets link `crt-static` by default,
rustc emits no `cdylib` while that feature is on, and turning it off produces
the ordinary ELF shared object. [dec:libedit:linux-musl-target] records the
measurement and the resulting build arrangement.

macOS is POSIX and ships libedit; supporting it is the same product on
a second POSIX. Windows is a different terminal lineage, but modern console
hosts and pseudoterminals accept VT sequences, which is the render model
nshedit already targets — the seam is input, modes, and events, not the
drawing model. The C ABI skin is the one component with no Windows
counterpart, and it stays behind.

## What macOS drop-in means

`packaging/install.sh` installs one object and claims libedit's filenames
beside it — `libedit.so`, `libedit.so.0`, `libedit.so.2` — because on ELF the
SONAME a consumer recorded is a *filename the loader searches for*, so a
symlink is all it takes to serve a binary built before we existed.
`conformance/soname.sh` verifies the installer, compatibility names,
pkg-config metadata, recorded SONAME, and a direct C consumer against the
staged library.

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
