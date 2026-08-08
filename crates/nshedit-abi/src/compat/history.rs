//! Compatibility editor traversal over the native history store.

pub(crate) use nshedit::history::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushError,
    PushResult,
};

mod session;

pub(crate) use session::HistorySession;
