//! Native history storage and editor traversal.

// [spec:nshedit:req:core.history+1]
mod native;
mod session;

pub use native::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushError,
    PushResult,
};
pub use session::HistorySession;
