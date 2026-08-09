use super::Direction;

/// How a history-search command obtains or reuses its query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistorySearchCommand {
    /// Search using the current line prefix without reading another unit.
    Prefix(Direction),
    /// Collect a complete query before asking the history host to search.
    Prompt(Direction),
    /// Search again as each query unit is collected.
    Incremental(Direction),
    /// Reuse the most recently completed history search.
    Repeat(HistorySearchRepetition),
}

/// How a repeated history search relates to the stored search direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HistorySearchRepetition {
    /// Keep the stored direction.
    SameDirection,
    /// Search in the opposite direction.
    OppositeDirection,
}

// [spec:nshedit:req:core.command-effects]
/// A closed command whose execution crosses a host-controlled boundary.
///
/// These values contain no callback names, pointers, command numbers, or
/// compatibility status codes. The read driver turns them into owned typed
/// effects after resolving any count or later-input continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectCommand {
    /// Complete the snapshot-bound token at the current cursor.
    Complete,
    /// Navigate the host's independent history cursor.
    NavigateHistory(Direction),
    /// Search host-owned history with a native query protocol.
    SearchHistory(HistorySearchCommand),
    /// Read one alias selector and ask the host for its expansion.
    ExpandAlias,
    /// Select an exact numbered history entry, or the oldest entry when no
    /// count was supplied.
    SelectHistoryLine,
    /// Restore the host's saved snapshot of the current history line.
    RestoreHistoryLine,
    /// Ask for one word from the newest history entry and insert it.
    InsertHistoryWord,
    /// Ask the host to collect and execute one editor configuration command.
    ReadEditorCommand,
    /// Edit the current or counted history line using a host facility.
    EditHistory,
}
