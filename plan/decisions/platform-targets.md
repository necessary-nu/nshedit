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
    )
    deferred (
        "Whether macOS drop-in means matching the OS libedit.3.dylib install name or shipping alongside it — decided in macos-contract."
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
