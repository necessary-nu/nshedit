//! The libedit and readline C ABIs.
//!
//! Two surfaces, both of which consumers compile against today:
//!
//! - `histedit.h` — libedit's own API, the `el_*`, `history*` and `tok_*`
//!   entry points.
//! - `editline/readline.h` — the GNU readline compatibility layer.
//!
//! Nothing here links a C library. This crate *is* the C library: it exports
//! the symbols and builds as `libnshedit.so`, installed with `libedit.so.0`
//! and `libreadline.so.8` symlinked onto it. See
//! `plan/decisions/no-c-ffi.md`.
//!
//! Everything C-shaped lives here and nowhere else — the varargs dispatch,
//! the shared conversion buffers whose returned pointers stay valid exactly
//! until the next call, the exported mutable statics, the live view cast onto
//! the line struct. `nshedit` itself is ordinary Rust and carries none of it.
//! See `plan/decisions/idiomatic-core.md`.
//!
//! Behaviour across this boundary is frozen through translation and test,
//! defects included, then corrected during idiomatization — per
//! `plan/decisions/conformance-policy.md` and the register in
//! `docs/errata.md`. A function here that looks wrong is reproducing
//! something; check its `sem` rule before changing it.

pub mod histedit;
pub mod readline;
