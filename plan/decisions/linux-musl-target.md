---
id [dec:libedit:linux-musl-target]
epitome "x86_64-unknown-linux-musl is the second supported Linux target: the same transcriptions, asserted separately under each C library, with the compatibility object built dynamically."
state @decided
category @executive
scope {
    elements ([arch:libedit:platform] [arch:libedit:c-abi])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Stay glibc-only on Linux."
        rejected_because "nsh has to run where its users' systems run, and a large share of those are musl: Alpine containers, appliance and embedded images, and static distributions. A line editor that cannot be built there constrains the shell that embeds it. The measured cost of the second target is a target gate, a second set of layout assertions, and a header-path arm — not a port."
    }
    {
        option "Widen the existing glibc cfg arms to any(target_env = \"gnu\", target_env = \"musl\")."
        rejected_because "That makes one assertion carry two ABI claims, which is the reasoning [dec:libedit:platform-targets] already rejected for an any-Linux cfg. The two libraries lay out the same records today; spelling the assertion once per library is what keeps that a measurement the compiler re-checks rather than an assumption inherited by a cfg edit."
    }
    {
        option "Ship the musl artifact the way musl targets link by default, statically."
        rejected_because "rustc emits no cdylib at all while crt-static is on: it drops the crate type with a warning and the build still succeeds. The C compatibility product would silently not exist, and a green build would be evidence of nothing. The drop-in is a shared object or it is not a drop-in."
    }
    {
        option "Support musl as a compile-only target, gated by a cross-check and nothing else."
        rejected_because "The C ABI is why this workspace exists on Linux. A target whose shared object is never built, installed, or loaded by a C consumer is compiled, not supported — the same standard [dec:libedit:platform-targets] applied to Darwin, which is why macOS carries a native acceptance stage."
    }
)
consequences {
    accepted (
        "The supported Linux matrix is x86_64-unknown-linux-gnu and x86_64-unknown-linux-musl, both x86-64. Every other Linux architecture, data model, x32, and Android still fail at the nshedit-plat gate; musl on another architecture is a separate port with its own layout evidence."
        "Every Linux record nshedit-plat transcribes carries one compile-time assertion per C library, spelled separately. Measured on x86-64 with gcc 15 against glibc and musl-gcc against musl 1.2.5, the two agree exactly: struct passwd is 48 bytes at offsets 0/8/16/20/24/32/40, sigset_t is 128, struct sigaction is 152 at 0/8/136/144, struct termios is 60 with NCCS 32, and C char is signed. Agreement is the finding, not the premise: each assertion is evaluated when its own library is the target, so a divergence fails that build."
        "The checks that read expected values out of C headers read the headers of the library the target links — /usr/include/<arch>-linux-musl first for a musl target, /usr/include for the distributions that do not split it — so a musl test binary run on a glibc development machine cannot be answered by glibc's numbers. glibc files these names under bits/termios-*.h, bits/signum-*.h and bits/sigaction.h; musl keeps them in bits/termios.h, bits/signal.h and signal.h. Same names, same required values, different files."
        "rustix selects its linux_raw backend on both targets, so the syscall path is identical and the C library does not change which instruction reaches the kernel. The libc surface of the workspace stays the three extern blocks [dec:libedit:no-c-ffi] enumerates."
        "musl supplies every symbol those blocks and the ABI crate name: __errno_location, the stdin/stdout/stderr data symbols, secure_getenv, getpwnam_r, getpwuid_r, setpwent/getpwent/endpwent, sigaction, pthread_sigmask, raise, strcoll and vsnprintf. No arm of the C ABI skin loses a facility on musl, and no new #[cfg] arm was needed in nshedit-abi."
        "musl has no NSS. Its getpwnam_r reads /etc/passwd directly and falls back to the nscd protocol when a socket is listening, which is how a musl host joined to a directory answers. That is a second reason to call the library rather than parse the file, and the tilde-expansion rule's flattening of every non-zero return is unchanged."
        "The musl compatibility object is built with crt-static off and gated on a musl host by ci/musl-acceptance.sh, which runs the same conformance stages as glibc — export set, SONAME, generated headers, installer, compatibility names, pkg-config metadata, a linked C consumer, and the unsafe-input stage — and additionally requires the object to record a musl C library and no glibc one. Recorded as [spec:nshedit:req:abi.musl-drop-in]."
        "ci/musl-cross-check.sh is the non-regression gate on the glibc development host: it compiles every crate and test target for musl and runs the suite, which crt-static makes statically linked binaries that any Linux host executes. Recorded as [spec:nshedit:req:workspace.musl-cross-check]."
        "The conformance stages, packaging/install.sh, and the Rust conformance wrapper all take the artifact directory from NSHEDIT_TARGET, so one set of stages can inspect either a host build or a cross build. Unset, every path is byte-identical to what it was."
        "The abi crate's conformance wrapper compiles away when crt-static is on, because the artifact it inspects cannot exist in that configuration. A build with no shared object reports no C ABI result rather than a passing one."
    )
    deferred (
        "A musl host is required for the C ABI acceptance. Producing a dynamically linked musl cdylib from a glibc host needs a musl-targeting linker with its own libgcc, and the obvious Debian arrangement resolves -lc against glibc and yields an object that links and cannot load. Containers make a musl host cheap, so this stays a host requirement rather than a cross-link problem to solve."
        "musl on architectures other than x86-64, and the other Linux architectures, x32, and Android, remain unsupported until separately specified and proven."
    )
}
edges {
    requires ([dec:libedit:platform-targets] [dec:libedit:platform-layer])
}
---

## Rationale

[dec:libedit:platform-targets] deferred musl with a specific reason: that
`x86_64-unknown-linux-musl` "drops the `cdylib` needed by the C compatibility
product". That is true of the default configuration and not of the target. A
musl target links `crt-static` by default, and rustc refuses to emit a
`cdylib` for a `crt-static` build — it drops the crate type, warns, and
succeeds. Turn the feature off and the same crate produces an ordinary ELF
shared object with our SONAME on it. The blocker was a target feature, not a
missing capability.

That leaves the question the deferral was really protecting: whether the
x86-64 glibc numbers this workspace transcribes are glibc's or the platform's.
They are the platform's. `struct passwd`, `struct sigaction`, `sigset_t` and
`struct termios` are the kernel's and the psABI's records, and both libraries
spell them the same way on x86-64 — measured, with `offsetof` compiled under
each, not inferred from POSIX. The signal numbers, the `V*` subscripts, the
terminal flag words and `_POSIX_VDISABLE` come out identical for the same
reason: they are Linux's, and a C library that disagreed would be unable to
talk to the kernel either.

## Why the assertions are still written twice

Because "they agree" is a measurement with a date on it, and the cheapest way
to keep a measurement honest is to make the compiler retake it. Each layout
has a `const` block gated on its own `target_env`, so building for musl
evaluates musl's block and building for glibc evaluates glibc's. Widening one
`cfg` to `any(gnu, musl)` would halve the text and delete the property: a
single assertion that passes cannot tell you which library it passed for.

The header-derived tests carry the same shape. They read the expected value
out of the C library's own headers rather than restating the implementation's
constant, and the search root is chosen by `target_env` rather than by
`uname`. That matters precisely because the test binaries are portable: a
static musl build runs on a glibc development machine, and without the arm it
would have quietly checked musl's transcription against glibc's headers and
reported a pass.

## What "supported" costs on musl

The same as on glibc, minus one thing: the C ABI acceptance needs a musl
host. The Rust product does not — `cargo test --target x86_64-unknown-linux-musl`
produces static binaries that run anywhere, which is why the cross-check is a
real execution and not a compile. But the compatibility object is dynamically
linked against the system's musl by definition, and building one on a glibc
distribution means pointing rustc at a musl-targeting linker that also has to
supply `libgcc_s`; the naive arrangement resolves `-lc` against glibc and
produces an object that links cleanly and cannot load on the system it was
built for. A musl container is a smaller and more honest answer than a
cross-linking recipe, and it is what `ci/musl-acceptance.sh` expects.
