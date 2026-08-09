---
id [dec:libedit:lint-policy]
epitome "First-party Rust uses idiomatic names and live code without any lint-suppression attributes."
state @decided
category @ban
scope {
    elements ([arch:libedit:core] [arch:libedit:c-abi] [arch:libedit:platform] [arch:libedit:terminal-caps])
    rules (
        [spec:nshedit:req:workspace.no-legacy-allows]
        [spec:nshedit:req:workspace.lint-policy]
        [spec:nshedit:req:core.unsafe-free]
    )
}
author "brendan@necessary.nu"
alternatives (
    {
        option "Keep broad allows until the greenfield rewrite deletes the translated modules."
        rejected_because "They hide which code is actually unused and let C spelling remain contagious during the migration. The baseline must be truthful before replacement starts."
    }
    {
        option "Use C spelling for Rust items exported under that name."
        rejected_because "Export metadata and cbindgen renames preserve ABI spelling without weakening the Rust source convention."
    }
    {
        option "Use allow or expect for unavoidable ABI signatures."
        rejected_because "An exported signature can remain exact while a typed private helper owns the implementation. A lint on private shape is evidence to improve that shape, not an ABI requirement."
    }
)
consequences {
    accepted (
        "Allow and expect attributes are absent from first-party code at every scope, including conditional attributes that would enable either."
        "Dead items are deleted, implemented, moved to the boundary that owns them, or accurately gated with cfg; dead_code is never silenced."
        "Rust types, constants, statics, functions, fields, and generated identifiers follow Rust naming conventions. C names are produced with export_name and header-generation metadata."
        "Unsafe exported functions carry real Safety documentation. Unused variables are removed or intentionally named with a leading underscore where the value is contractually present."
        "External ABI and generated-format constraints are represented so the lint does not arise, or the generated input is fixed or isolated outside first-party checked source."
        "The final core forbids unsafe code; the ABI and platform crates require Safety documentation on unsafe APIs, deny implicit unsafe operations inside unsafe functions, and keep unsafe blocks local."
    )
    deferred ()
}
edges {
    requires ([dec:libedit:idiomatic-core] [dec:libedit:opaque-abi-adapter])
}
codifies (
    [spec:nshedit:req:workspace.no-legacy-allows]
    [spec:nshedit:req:workspace.lint-policy]
    [spec:nshedit:req:core.unsafe-free]
)
---

## Rationale

Lint suppressions in a literal port are often migration notes disguised as
policy. Once the port is the maintained implementation they instead hide dead
paths, make C-derived spelling look intentional, and allow compatibility
mechanics to spread beyond their boundary.

The ABI determines exported spelling, not Rust identifier spelling. Modern
export and header-generation metadata separate those concerns cleanly. An
external signature constrains the exported wrapper, not the private helper
that implements it. Splitting those responsibilities removes the reason to
suppress a lint and makes the boundary itself visible in the types.
