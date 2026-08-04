//! The `src/editline/` public headers.
//!
//! One submodule per header, mirroring the C directory. The readline
//! compatibility *functions* live in the ABI crate; only the types the
//! header defines are here, because the core needs to name them.

pub mod readline;
