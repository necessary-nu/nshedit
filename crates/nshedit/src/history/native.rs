//! Owned Rust-domain history storage with explicit traversal state.

use std::collections::VecDeque;
use std::fmt;
use std::num::NonZeroUsize;

use crate::domain::{Direction, Text};

/// Stable identity of one native history entry.
///
/// The numeric value is opaque ordering information, not a sentinel or a
/// position in the store. Removing or evicting other entries never changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HistoryId(u64);

impl HistoryId {
    /// The opaque monotonically allocated value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for HistoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// One owned native history record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry<M = ()> {
    id: HistoryId,
    line: Text,
    metadata: M,
}

impl<M> HistoryEntry<M> {
    /// Stable identity allocated when the entry was inserted.
    #[must_use]
    pub const fn id(&self) -> HistoryId {
        self.id
    }

    /// The complete logical line, including preserved raw or wide units.
    #[must_use]
    pub fn line(&self) -> &Text {
        &self.line
    }

    /// Mutably borrow the logical line without changing its identity.
    pub fn line_mut(&mut self) -> &mut Text {
        &mut self.line
    }

    /// Application-owned metadata stored with the line.
    #[must_use]
    pub const fn metadata(&self) -> &M {
        &self.metadata
    }

    /// Mutably borrow the application-owned metadata.
    pub fn metadata_mut(&mut self) -> &mut M {
        &mut self.metadata
    }

    /// Consume the entry without discarding any owned part.
    #[must_use]
    pub fn into_parts(self) -> (HistoryId, Text, M) {
        (self.id, self.line, self.metadata)
    }
}

/// How insertion handles a line equal to the newest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DuplicatePolicy {
    /// Retain every insertion.
    #[default]
    Keep,
    /// Reject only an immediately repeated line.
    IgnoreConsecutive,
}

/// Failed insertion with ownership of the rejected record preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushError<M> {
    /// Every representable typed entry identity has been allocated.
    IdExhausted { line: Text, metadata: M },
}

impl<M> fmt::Display for PushError<M> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdExhausted { .. } => {
                formatter.write_str("history entry identities are exhausted")
            }
        }
    }
}

impl<M: fmt::Debug> std::error::Error for PushError<M> {}

/// Result of inserting an owned line and its metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushResult<M> {
    /// The entry was retained. A bounded store returns an evicted oldest entry
    /// rather than silently discarding its metadata.
    Inserted {
        id: HistoryId,
        evicted: Option<HistoryEntry<M>>,
    },
    /// Consecutive-duplicate policy rejected the entry and returned ownership
    /// of everything the caller supplied.
    Duplicate { line: Text, metadata: M },
}

/// One independent position in a [`HistoryStore`].
///
/// A cursor starts on the live edit line rather than inside history. It holds
/// a typed entry identity, so insertion and removal never turn an old numeric
/// position into a different entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HistoryCursor {
    current: Option<HistoryId>,
}

impl HistoryCursor {
    /// A cursor positioned on the live edit line.
    #[must_use]
    pub const fn new() -> Self {
        Self { current: None }
    }

    /// The selected entry, or `None` while positioned on the live line.
    #[must_use]
    pub const fn current(self) -> Option<HistoryId> {
        self.current
    }

    /// Whether this cursor is positioned on the live edit line.
    #[must_use]
    pub const fn is_live(self) -> bool {
        self.current.is_none()
    }

    /// Return this cursor to the live edit line.
    pub fn reset(&mut self) {
        self.current = None;
    }
}

/// Result of moving an explicit history cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation<'a, M> {
    /// The cursor selected an owned entry in the store.
    Entry(&'a HistoryEntry<M>),
    /// Moving toward newer input left history and returned to the live line.
    Live,
    /// No entry exists in the requested direction; the cursor did not move.
    Boundary,
}

// [spec:nshedit:req:core.history+1]
/// Owned logical history with typed identities and external traversal cursors.
///
/// Entries are stored newest first. Capacity is `Option<NonZeroUsize>`, so an
/// unbounded store and a bound cannot be confused through a magic zero. The
/// store contains no encoding state: persistence or locale conversion is an
/// integration concern over the owned [`Text`] and metadata values.
#[derive(Debug)]
pub struct HistoryStore<M = ()> {
    entries: VecDeque<HistoryEntry<M>>,
    capacity: Option<NonZeroUsize>,
    duplicate_policy: DuplicatePolicy,
    next_id: Option<u64>,
}

impl<M> HistoryStore<M> {
    /// Create an unbounded history that retains consecutive duplicates.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: None,
            duplicate_policy: DuplicatePolicy::Keep,
            next_id: Some(0),
        }
    }

    /// Create a bounded history that evicts its oldest entry on insertion.
    #[must_use]
    pub fn bounded(capacity: NonZeroUsize) -> Self {
        Self {
            capacity: Some(capacity),
            ..Self::new()
        }
    }

    /// Number of retained entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no entries are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Current capacity, where `None` means unbounded.
    #[must_use]
    pub const fn capacity(&self) -> Option<NonZeroUsize> {
        self.capacity
    }

    /// Current consecutive-duplicate policy.
    #[must_use]
    pub const fn duplicate_policy(&self) -> DuplicatePolicy {
        self.duplicate_policy
    }

    /// Change the capacity immediately and return evicted entries oldest
    /// first. Passing `None` makes the store unbounded.
    pub fn set_capacity(&mut self, capacity: Option<NonZeroUsize>) -> Vec<HistoryEntry<M>> {
        self.capacity = capacity;
        let mut evicted = Vec::new();
        while self
            .capacity
            .is_some_and(|limit| self.entries.len() > limit.get())
        {
            if let Some(entry) = self.entries.pop_back() {
                evicted.push(entry);
            }
        }
        evicted
    }

    /// Select how following insertions handle an immediate repeated line.
    pub fn set_duplicate_policy(&mut self, policy: DuplicatePolicy) {
        self.duplicate_policy = policy;
    }

    /// Insert a logical line and owned application metadata.
    pub fn push_with(&mut self, line: Text, metadata: M) -> Result<PushResult<M>, PushError<M>> {
        if self.duplicate_policy == DuplicatePolicy::IgnoreConsecutive
            && self.entries.front().is_some_and(|entry| entry.line == line)
        {
            return Ok(PushResult::Duplicate { line, metadata });
        }

        let Some(raw_id) = self.next_id else {
            return Err(PushError::IdExhausted { line, metadata });
        };
        self.next_id = raw_id.checked_add(1);
        let id = HistoryId(raw_id);
        self.entries.push_front(HistoryEntry { id, line, metadata });
        let evicted = self
            .capacity
            .filter(|limit| self.entries.len() > limit.get())
            .and_then(|_| self.entries.pop_back());
        Ok(PushResult::Inserted { id, evicted })
    }

    /// Remove every entry while retaining monotonic identity allocation.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Remove and return one entry by stable identity.
    pub fn remove(&mut self, id: HistoryId) -> Option<HistoryEntry<M>> {
        let position = self.position(id)?;
        self.entries.remove(position)
    }

    /// Borrow one entry by stable identity.
    #[must_use]
    pub fn get(&self, id: HistoryId) -> Option<&HistoryEntry<M>> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Mutably borrow one entry without changing its identity.
    pub fn get_mut(&mut self, id: HistoryId) -> Option<&mut HistoryEntry<M>> {
        self.entries.iter_mut().find(|entry| entry.id == id)
    }

    /// Select an exact entry on an explicit cursor, leaving the cursor alone
    /// when the identity is absent.
    pub fn select(&self, cursor: &mut HistoryCursor, id: HistoryId) -> Option<&HistoryEntry<M>> {
        let entry = self.get(id)?;
        cursor.current = Some(id);
        Some(entry)
    }

    /// Borrow the newest entry.
    #[must_use]
    pub fn newest(&self) -> Option<&HistoryEntry<M>> {
        self.entries.front()
    }

    /// Borrow the oldest entry.
    #[must_use]
    pub fn oldest(&self) -> Option<&HistoryEntry<M>> {
        self.entries.back()
    }

    /// Iterate from newest to oldest.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &HistoryEntry<M>> {
        self.entries.iter()
    }

    /// Move an independent cursor toward older (`Previous`) or newer (`Next`)
    /// input.
    ///
    /// Moving `Previous` from the live line selects the newest entry. Moving
    /// `Next` from that entry returns [`Navigation::Live`]. A cursor whose
    /// entry was removed or evicted is repaired to the live line before the
    /// requested movement is applied.
    pub fn navigate(&self, cursor: &mut HistoryCursor, direction: Direction) -> Navigation<'_, M> {
        let current = cursor.current.and_then(|id| self.position(id));
        if cursor.current.is_some() && current.is_none() {
            cursor.reset();
        }

        match direction {
            Direction::Previous => {
                let position = current.map_or(0, |position| position + 1);
                self.select_position(cursor, position)
            }
            Direction::Next => match current {
                None => Navigation::Boundary,
                Some(0) => {
                    cursor.reset();
                    Navigation::Live
                }
                Some(position) => self.select_position(cursor, position - 1),
            },
        }
    }

    fn position(&self, id: HistoryId) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == id)
    }

    fn select_position(&self, cursor: &mut HistoryCursor, position: usize) -> Navigation<'_, M> {
        let Some(entry) = self.entries.get(position) else {
            return Navigation::Boundary;
        };
        cursor.current = Some(entry.id);
        Navigation::Entry(entry)
    }
}

impl HistoryStore<()> {
    /// Insert a logical line without application metadata.
    pub fn push(&mut self, line: Text) -> Result<PushResult<()>, PushError<()>> {
        self.push_with(line, ())
    }
}

impl<M> Default for HistoryStore<M> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{NonScalarWide, TextUnit};

    fn inserted<M>(result: PushResult<M>) -> HistoryId {
        match result {
            PushResult::Inserted { id, .. } => id,
            PushResult::Duplicate { .. } => panic!("entry was unexpectedly rejected"),
        }
    }

    // [spec:nshedit:req:core.history+1/test]
    #[test]
    fn entries_own_text_and_metadata() {
        let mut history = HistoryStore::new();
        let line: Text = [
            TextUnit::Scalar('x'),
            TextUnit::RawByte(0xff),
            TextUnit::CompatibilityWide(NonScalarWide::new(0xd800).unwrap()),
        ]
        .into_iter()
        .collect();
        let id = inserted(
            history
                .push_with(line.clone(), String::from("status=2"))
                .unwrap(),
        );

        let entry = history.get(id).unwrap();
        assert_eq!(entry.line(), &line);
        assert_eq!(entry.metadata(), "status=2");
    }

    #[test]
    fn typed_ids_never_wrap() {
        let mut history = HistoryStore::new();
        history.next_id = Some(u64::MAX);
        let id = inserted(history.push(Text::from("last")).unwrap());
        assert_eq!(id.get(), u64::MAX);
        assert_eq!(
            history.push(Text::from("overflow")),
            Err(PushError::IdExhausted {
                line: Text::from("overflow"),
                metadata: (),
            })
        );
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn capacity_returns_evicted_entries() {
        let mut history = HistoryStore::bounded(NonZeroUsize::new(2).unwrap());
        history.push_with(Text::from("oldest"), 1).unwrap();
        history.push_with(Text::from("middle"), 2).unwrap();
        let result = history.push_with(Text::from("newest"), 3).unwrap();
        let PushResult::Inserted {
            evicted: Some(evicted),
            ..
        } = result
        else {
            panic!("bounded insertion did not return its eviction");
        };
        assert_eq!(evicted.line(), &Text::from("oldest"));
        assert_eq!(evicted.metadata(), &1);

        let evicted = history.set_capacity(Some(NonZeroUsize::new(1).unwrap()));
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].line(), &Text::from("middle"));
    }

    #[test]
    fn duplicates_return_rejected_ownership() {
        let mut history = HistoryStore::new();
        history.set_duplicate_policy(DuplicatePolicy::IgnoreConsecutive);
        history.push_with(Text::from("same"), vec![1]).unwrap();
        assert_eq!(
            history.push_with(Text::from("same"), vec![2]).unwrap(),
            PushResult::Duplicate {
                line: Text::from("same"),
                metadata: vec![2],
            }
        );
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn cursors_are_independent() {
        let mut history = HistoryStore::new();
        for line in ["old", "middle", "new"] {
            history.push(Text::from(line)).unwrap();
        }
        let mut first = HistoryCursor::new();
        let mut second = HistoryCursor::new();

        assert_eq!(
            entry_line(history.navigate(&mut first, Direction::Previous)),
            "new"
        );
        assert_eq!(
            entry_line(history.navigate(&mut first, Direction::Previous)),
            "middle"
        );
        assert_eq!(
            entry_line(history.navigate(&mut second, Direction::Previous)),
            "new"
        );
    }

    #[test]
    fn navigation_directions_are_semantic() {
        let mut history = HistoryStore::new();
        history.push(Text::from("old")).unwrap();
        history.push(Text::from("new")).unwrap();
        let mut cursor = HistoryCursor::new();

        assert_eq!(
            entry_line(history.navigate(&mut cursor, Direction::Previous)),
            "new"
        );
        assert_eq!(
            entry_line(history.navigate(&mut cursor, Direction::Previous)),
            "old"
        );
        assert_eq!(
            history.navigate(&mut cursor, Direction::Previous),
            Navigation::Boundary
        );
        assert_eq!(
            entry_line(history.navigate(&mut cursor, Direction::Next)),
            "new"
        );
        assert_eq!(
            history.navigate(&mut cursor, Direction::Next),
            Navigation::Live
        );
        assert_eq!(
            history.navigate(&mut cursor, Direction::Next),
            Navigation::Boundary
        );
    }

    #[test]
    fn stale_cursors_recover_after_eviction() {
        let mut history = HistoryStore::new();
        history.push(Text::from("old")).unwrap();
        history.push(Text::from("new")).unwrap();
        let mut cursor = HistoryCursor::new();
        history.navigate(&mut cursor, Direction::Previous);
        history.navigate(&mut cursor, Direction::Previous);
        history.set_capacity(Some(NonZeroUsize::new(1).unwrap()));

        assert_eq!(
            entry_line(history.navigate(&mut cursor, Direction::Previous)),
            "new"
        );
    }

    #[test]
    fn mutable_lookup_preserves_identity() {
        let mut history = HistoryStore::new();
        let id = inserted(history.push_with(Text::from("before"), 1).unwrap());
        let entry = history.get_mut(id).unwrap();
        *entry.line_mut() = Text::from("after");
        *entry.metadata_mut() = 2;

        let entry = history.get(id).unwrap();
        assert_eq!(entry.id(), id);
        assert_eq!(entry.line(), &Text::from("after"));
        assert_eq!(entry.metadata(), &2);
    }

    #[test]
    fn exact_selection_validates_identity() {
        let mut history = HistoryStore::new();
        let removed = inserted(history.push(Text::from("removed")).unwrap());
        let kept = inserted(history.push(Text::from("kept")).unwrap());
        history.remove(removed);
        let mut cursor = HistoryCursor::new();

        assert!(history.select(&mut cursor, removed).is_none());
        assert!(cursor.is_live());
        assert_eq!(
            history.select(&mut cursor, kept).unwrap().line(),
            &Text::from("kept")
        );
        assert_eq!(cursor.current(), Some(kept));
    }

    #[test]
    fn clearing_does_not_reuse_ids() {
        let mut history = HistoryStore::new();
        let first = inserted(history.push(Text::from("first")).unwrap());
        history.clear();
        let second = inserted(history.push(Text::from("second")).unwrap());
        assert!(second > first);
    }

    fn entry_line(navigation: Navigation<'_, ()>) -> String {
        let Navigation::Entry(entry) = navigation else {
            panic!("navigation did not select an entry");
        };
        entry
            .line()
            .as_units()
            .iter()
            .map(|unit| match unit {
                TextUnit::Scalar(character) => *character,
                _ => panic!("test line was not scalar text"),
            })
            .collect()
    }
}
