---
id [dec:libedit:secure-temporary-files]
epitome "History scratch state is anonymous, while external-editor files are private unpredictable names with scoped cleanup."
state @decided
category @executive
scope {
    elements ([arch:libedit:c-abi])
    rules (
        [spec:libedit:sem:readline.history-truncate-file-fn+1]
        [spec:libedit:sem:vi.vi-histedit-fn]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep process ids, counters, clocks, or a private pseudo-random generator in temporary filenames."
        rejected_because "Those inputs are predictable across trust boundaries, and exclusive open alone only converts pre-creation attacks into reliable denial of service."
    }
    {
        option "Keep a named truncation scratch file until copy-back completes."
        rejected_because "No child process needs that path, so leaving private history bytes addressable in a shared directory creates exposure without serving an interface."
    }
    {
        option "Make the external-editor file anonymous too."
        rejected_because "The selected editor is a separate process and its required interface is the pathname passed as argv[1]."
    }
)
consequences {
    accepted (
        "Temporary names and exclusive creation come from the workspace's audited tempfile dependency; first-party code does not implement filename randomness."
        "Every temporary descriptor is created with mode 0600. Exclusive creation refuses existing filesystem objects rather than following a symlink."
        "history_truncate_file creates its scratch in the inherited hardcoded /tmp location, unlinks the name before copying history bytes, and relies only on descriptor lifetime for cleanup. Its in-place target rewrite remains non-atomic."
        "External editing keeps one unpredictable named 0600 file in /tmp only while the host effect is active. NamedTempFile ownership removes it after write, spawn, seek, read, conversion, or normal completion paths."
        "External editing continues to read through the original descriptor after the editor exits, preserving the documented behaviour when an editor replaces the pathname."
        "Named H_SAVE is unchanged: [dec:libedit:history-save-failure-reporting] separately owns secure same-directory staging and atomic destination replacement."
    )
    deferred (
        "Atomic history truncation would change the function's crash, inode, link, symlink, and directory-permission observations and requires a separate decision."
    )
}
edges {
    requires ([dec:libedit:conformance-policy] [dec:libedit:no-c-ffi])
    related_to ([dec:libedit:history-save-failure-reporting] [dec:libedit:native-command-protocols] [dec:libedit:native-read-driver])
}
codifies (
    [spec:libedit:sem:readline.history-truncate-file-fn+1]
    [spec:libedit:sem:vi.vi-histedit-fn]
)
---

## Rationale

Temporary storage is a filesystem capability, not a string-formatting problem.
The previous Rust paths assembled names from wall-clock state or from the
process id plus an incrementing counter. Both then opened with the platform's
ordinary file defaults, so a permissive umask could expose line and history
contents before cleanup. Reimplementing a stronger generator locally would
only create another security primitive to audit.

Truncation and external editing have different pathname requirements. The
truncation algorithm needs only a seekable scratch descriptor, so its random
name is removed before the first private byte is copied and the kernel owns
cleanup thereafter. An external editor cannot consume an anonymous descriptor
through the historical command interface, so that path remains named, private,
and RAII-owned for exactly the child interaction. Both retain `/tmp` as the
documented compatibility location and both keep their existing read and
copy-back semantics.

This decision does not widen the history-save correction. A complete named
save promises replacement and therefore stages beside its destination before
an atomic rename; truncation promises an in-place tail rewrite and external
editing promises a child-visible pathname. Each operation now gets the
smallest temporary-file lifetime its interface actually requires.
