//! Owned, native tokenization over logical editor text.
//!
//! The implementation lives with the editor because completion consumes its
//! checked token spans and cursor state. This module provides the focused
//! public path without exposing any C storage, integer status protocol, or
//! borrowed scratch buffer.

pub use crate::editor::{
    Continuation, QuoteStyle, Token, TokenCursor, TokenIndex, TokenOffset, Tokenization,
    TokenizedLine, Tokenizer,
};
