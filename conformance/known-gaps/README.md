# Exact known-gap fixtures

This directory is for short-lived, reviewed differences between the in-tree C
oracle and the Rust port. A fixture is the exact `diff -U0` emitted by
`differential.sh`; patterns, ignored fields, and port-only golden traces are
not accepted.

The differential fails if a byte changes unexpectedly. It also fails when the
two traces become equal while a fixture remains, so closing a gap requires
deleting its fixture. Reference defects deliberately reproduced by both sides
belong in `docs/errata.md`, not here.
