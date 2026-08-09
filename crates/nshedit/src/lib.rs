//! Safe Rust line editing, history, and tokenization.
//!
//! C and readline compatibility live in `nshedit-abi`; this crate exposes
//! only native Rust values and operations.

// [spec:nshedit:req:core.unsafe-free]
#![forbid(unsafe_code)]

// [spec:nshedit:req:workspace.no-legacy-allows]
// [spec:nshedit:req:workspace.lint-policy]
// [spec:nshedit:req:core.public-surface]
// [spec:nshedit:req:core.typed-domain+1]
/// Rust-native editor values shared by the safe editor shell and its hosts.
pub mod domain;

// [spec:nshedit:req:core.raii-lifecycle]
// [spec:nshedit:req:core.rust-io+1]
// [spec:nshedit:req:core.effect-hooks]
// [spec:nshedit:req:core.line-commands]
// [spec:nshedit:req:core.terminal-render+1]
// [spec:nshedit:req:core.token-completion+1]
// [spec:nshedit:req:core.read-driver]
// [spec:nshedit:req:core.no-compat-internals]
/// Safe native editor sessions and their borrowed I/O capabilities.
pub mod editor;

/// Native and safely decoded legacy history-file formats.
pub mod histfile;

// [spec:nshedit:req:core.history+1]
/// Owned native history storage and traversal.
pub mod history;

/// Owned tokenization over logical editor text.
pub mod tokenizer;
