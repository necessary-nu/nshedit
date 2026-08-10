---
id [dec:libedit:history-save-failure-reporting]
epitome "History saves report persistence failures, and named saves replace only complete files."
state @decided
category @executive
scope {
    elements ([arch:libedit:c-abi])
    rules (
        [spec:libedit:sem:history.history-save-fn+1]
        [spec:libedit:sem:history.history-save-fp-fn+1]
        [spec:libedit:sem:readline.append-history-fn+1]
        [spec:libedit:sem:readline.write-history-fn+1]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Continue reproducing successful return values after failed writes or flushes."
        rejected_because "Silent partial persistence is a data-loss defect, not useful compatibility behaviour."
    }
    {
        option "Report failures while continuing to truncate the destination before writing."
        rejected_because "A correct error code does not recover the history that an interrupted or failed save already destroyed."
    }
    {
        option "Check each stdio write but leave a caller-owned stream unflushed."
        rejected_because "Buffered stdio commonly reports the actual storage failure only from fflush, so success before that point is still false."
    }
)
consequences {
    accepted (
        "H_SAVE creates a private same-directory temporary, writes and flushes the complete history, synchronizes it, and atomically replaces the named destination only after every step succeeds."
        "A named-save failure removes its temporary when possible and leaves the prior destination untouched. Temporary creation, permission, write, flush, synchronization, and replacement errors all produce _HE_HIST_WRITE and preserve the originating errno."
        "H_SAVE_FP and H_NSAVE_FP check every entry write and flush the caller-owned FILE before reporting success. The stream remains open and owned by the caller."
        "write_history and append_history retain their 0-or-positive-errno convention and cannot report success after their underlying save failed."
        "Atomic replacement changes inode identity, breaks an existing hard-link relationship for the named directory entry, replaces a symlink rather than truncating its referent, and requires write permission on the containing directory. Those changes are accepted to prevent partial replacement."
        "The detailed history and readline rules are versioned because these return, errno, stream-flush, and filesystem effects are C-visible divergences from the inherited implementation."
    )
    deferred (
        "A release or soname policy for grouping approved behavioural corrections."
    )
}
edges {
    requires ([dec:libedit:conformance-policy] [dec:libedit:native-history] [dec:libedit:no-c-ffi])
    related_to ([dec:libedit:opaque-abi-adapter])
}
codifies (
    [spec:libedit:sem:history.history-save-fn+1]
    [spec:libedit:sem:history.history-save-fp-fn+1]
    [spec:libedit:sem:readline.append-history-fn+1]
    [spec:libedit:sem:readline.write-history-fn+1]
)
---

## Rationale

A save operation promises persistence, not merely successful traversal of the
history records. The inherited implementation discarded per-entry stdio
results and the final flush result, so it could return a positive entry count
after the destination was only partly written. Its named save also truncated
the old file before the first byte of the replacement was known to be
writable.

The compatibility boundary now treats the save as one transaction. Named
saves stage the exact frozen libedit byte format beside the destination and
replace the directory entry only after the staged file is complete. Stream
saves retain the caller's FILE ownership while flushing it before success, the
only point at which buffered I/O can be known to have reached the descriptor.
The exported status conventions stay intact: history operations still use
`-1` and `_HE_HIST_WRITE`, while readline wrappers still return a positive
errno. What changes is that success once again means the promised output was
actually completed.
