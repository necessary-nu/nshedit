//! Ported from `src/refresh.c`; rules live in
//! `docs/spec/port/src/refresh.md`.

use crate::el::CoordT;

// [spec:libedit:def:refresh.el-refresh-t]
/// Where the refresh machinery believes the cursor is, and how tall the
/// display was last time round.
pub struct ElRefreshT {
    /// Refresh cursor position.
    pub r_cursor: CoordT,
    /// Vertical locations: rows used by the previous refresh.
    pub r_oldcv: i32,
    /// Rows used by this refresh.
    pub r_newcv: i32,
}
