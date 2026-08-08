//! Private translated engine retained behind the C ABI during cutover.
//!
//! These modules preserve the reference implementation's remaining observable
//! behaviour. They are intentionally not part of the Rust API: native callers
//! use `nshedit`, while this crate owns every C-shaped representation and
//! callback obligation.

pub(crate) mod chared;
pub(crate) mod chartype;
pub(crate) mod common;
pub(crate) mod el;
pub(crate) mod emacs;
pub(crate) mod errno;
pub(crate) mod fcns;
pub(crate) mod filecomplete;
pub(crate) mod hist;
pub(crate) mod histedit;
#[cfg(test)]
pub(crate) mod history;
pub(crate) mod keymacro;
pub(crate) mod literal;
pub(crate) mod locale;
pub(crate) mod map;
pub(crate) mod parse;
pub(crate) mod prompt;
pub(crate) mod read;
pub(crate) mod refresh;
pub(crate) mod search;
pub(crate) mod sig;
pub(crate) mod stdio;
pub(crate) mod terminal;
pub(crate) mod tty;
pub(crate) mod vi;
pub(crate) mod vislite;

pub(crate) mod domain {
    pub(crate) use nshedit::domain::*;
}

pub(crate) mod editor {
    pub(crate) use nshedit::editor::*;
}

#[cfg(test)]
pub(crate) mod testkit;
