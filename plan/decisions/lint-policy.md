---
id [dec:libedit:lint-policy]
epitome "First-party Rust uses idiomatic names and live code; blanket lint suppression is forbidden and external constraints use reasoned expectations."
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
        option "Use allow for unavoidable ABI signatures."
        rejected_because "A narrow expect with a reason records the external constraint and fails when the exception stops being necessary."
    }
)
consequences {
    accepted (
        "Crate- and module-wide allow attributes are removed from first-party code."
        "Dead items are deleted, implemented, moved to the boundary that owns them, or accurately gated with cfg; dead_code is never silenced."
        "Rust types, constants, statics, functions, fields, and generated identifiers follow Rust naming conventions. C names are produced with export_name and header-generation metadata."
        "Unsafe exported functions carry real Safety documentation. Unused variables are removed or intentionally named with a leading underscore where the value is contractually present."
        "A lint imposed solely by an external ABI or generated format may use the narrowest expect attribute with a reason. Unfulfilled expectations are lint failures."
        "The final core forbids unsafe code; the ABI and platform crates deny undocumented unsafe operations and keep unsafe blocks local."
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
export and header-generation metadata separate those concerns cleanly. Where
an external signature genuinely forces a lint, `expect` documents why and
alerts the project when refactoring makes the exception obsolete.
