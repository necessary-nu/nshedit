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

// Exported C symbols keep their C names, which are not Rust's casing.
#![allow(non_upper_case_globals)]
// Every function in this crate is a C entry point and its safety contract is
// its `sem` rule, quoted per function in `docs/spec/port/`. Repeating a
// boilerplate `# Safety` section on all ~160 of them would say less than the
// rule already does.
#![allow(clippy::missing_safety_doc)]
// Both of these go away as the bodies land: until then every function is
// `todo!()`, so its parameters are unread, and the private helpers translated
// from `readline.c`'s `static` functions have no callers yet.
#![allow(dead_code, unused_variables)]

pub mod eln;
pub mod histedit;
pub mod readline;
