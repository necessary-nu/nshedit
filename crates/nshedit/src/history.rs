//! Native history storage and editor traversal.

// [spec:nshedit:req:core.history+1]
mod native;
#[path = "../../nshedit-abi/src/compat/history/session.rs"]
mod session;

pub use native::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushError,
    PushResult,
};
pub use session::HistorySession;
