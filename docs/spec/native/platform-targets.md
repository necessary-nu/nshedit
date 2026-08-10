# Supported platforms and their ABIs

`plan/decisions/platform-targets.md` makes macOS a supported target and keeps
`x86_64-unknown-linux-gnu` as the only supported Linux target. Linux support
is not inferred from `target_os = "linux"`: another architecture, libc, data
model, or Android requires its own transcriptions and acceptance evidence.
These rules state what "supported" obliges the workspace to spell, assert,
and gate. Behavioural semantics stay in the `libedit` corpus; this corpus
states which platform's numbers those semantics are read against.

## Per-operating-system platform ABI

> [spec:nshedit:req:platform.per-os-layouts]
> Every foreign record layout, constant, and symbol name `nshedit-plat`
> transcribes MUST be spelled per operating system from that system's
> documented stable ABI, and each spelling MUST carry a compile-time
> assertion that the compiler evaluates when that system is the target. One
> layout shared across systems that is correct on only some of them is
> forbidden, as is a layout whose only check runs on a system other than the
> one it describes. Each transcription MUST name the header or published ABI
> it was read from. A system for which a required layout is not spelled MUST
> fail to compile rather than fall through to another system's numbers.

> [spec:nshedit:req:platform.darwin-termios]
> The Darwin termios projection MUST be Darwin's own: its `NCCS`, its
> `tcflag_t` width, its `V*` subscripts, its flag-word bit values, its
> `_POSIX_VDISABLE`, and its separate `c_ispeed`/`c_ospeed` line speeds. The
> glibc shape that the detailed `def:tty.el-tty-t` rule describes MUST NOT be
> projected onto Darwin, because the macOS drop-in target is Darwin's ABI and
> a caller's `struct termios` there is Darwin's. A terminal behaviour or
> control-character slot the target system's termios does not define MUST be
> absent from the platform layer's representation table and MUST NOT resolve
> to another system's bit; the compatibility `setty` vocabulary MUST follow
> that table rather than a fixed list.

## Darwin ABI surface

> [spec:nshedit:req:abi.darwin-runtime]
> The compatibility crate's access to the C runtime MUST be spelled per
> operating system from that system's documented symbols: the standard
> streams through `stdin`/`stdout`/`stderr` on the GNU and Bionic runtimes
> and through the `__stdinp`/`__stdoutp`/`__stderrp` data symbols on Darwin,
> and the thread-local `errno` slot through `__errno_location` on the former
> and `__error` on the latter. A supported system MUST reach the real stream
> and the caller's real `errno`; an accessor that answers a null stream, a
> private slot, or another system's symbol name does not satisfy this rule.
> Any remaining operation the crate offers on one supported system and not
> another MUST state which system ABI supplies it and what the other systems
> answer instead.

> [spec:nshedit:req:abi.darwin-drop-in]
> On macOS the drop-in claim is source and link compatibility, not
> replacement of the system library. The workspace MUST ship its own Mach-O
> dylib carrying its own install name, install the compatibility link names
> beside it, and generate headers a consumer compiles against unchanged. It
> MUST NOT claim the system `/usr/lib/libedit.3.dylib` install name: that
> path is served from the dyld shared cache and protected by System Integrity
> Protection, so a consumer that recorded it can never resolve to this
> library and a build that asserted otherwise would be asserting something
> unreachable. The Mach-O export set, install name, and compatibility link
> names MUST be gated on macOS by the counterparts of the ELF shape and
> soname stages.

## Cross-target non-regression

> [spec:nshedit:req:workspace.darwin-cross-check]
> The workspace MUST compile for the supported Darwin targets from a Linux
> development machine, and that compilation MUST be reachable as one recorded
> command rather than reconstructed by hand. The check MUST build every
> crate and every test target, so that the Darwin-gated layout assertions are
> evaluated by the compiler rather than merely written down. Work performed
> on Linux MUST NOT be able to break the Darwin build without that check
> failing.
