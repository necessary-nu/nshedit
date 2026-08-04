//! The libedit C ABI.
//!
//! Two surfaces, both of which consumers compile against today:
//!
//! - `histedit.h` — libedit's own API, the `el_*`, `history*` and `tok_*`
//!   entry points.
//! - `editline/readline.h` — the GNU readline compatibility layer.
//!
//! Nothing here links a C library. This crate *is* the C library: it exports
//! the symbols and installs as `libedit.so.0.0.78`. See
//! `plan/decisions/no-c-ffi.md`.
//!
//! Behaviour across this boundary is frozen, defects included, per
//! `plan/decisions/conformance-policy.md`. A function here that looks wrong
//! is reproducing something; check its `sem` rule before changing it.

pub mod histedit;
pub mod readline;
