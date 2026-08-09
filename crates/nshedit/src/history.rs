//! Owned history storage and editor traversal.

// [spec:nshedit:req:core.history+1]
mod store;

pub use store::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushError,
    PushResult,
};
