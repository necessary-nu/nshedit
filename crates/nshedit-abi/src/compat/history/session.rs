//! One editor traversal over a native [`HistoryStore`].

use std::num::NonZeroUsize;

use crate::compat::domain::{Direction, Text, TextUnit};
use crate::compat::hist::{EditorHistory, HistLine, HistText};

use super::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushError,
    PushResult,
};

/// A native history store paired with one independent editor cursor.
///
/// [`HistoryStore`] remains the reusable record owner. This type is the small
/// stateful adapter needed by the compatibility editor, whose history trait
/// remembers one traversal between calls.
#[derive(Debug, Default)]
pub struct HistorySession {
    store: HistoryStore,
    cursor: HistoryCursor,
}

impl HistorySession {
    /// Create a bounded session that evicts the oldest entry on insertion.
    #[must_use]
    pub fn bounded(capacity: NonZeroUsize) -> Self {
        Self {
            store: HistoryStore::bounded(capacity),
            cursor: HistoryCursor::new(),
        }
    }

    /// Borrow the reusable native store.
    #[must_use]
    pub const fn store(&self) -> &HistoryStore {
        &self.store
    }

    /// Mutably borrow the store. Stale cursor identities are repaired by the
    /// next navigation operation.
    pub const fn store_mut(&mut self) -> &mut HistoryStore {
        &mut self.store
    }

    /// Consume the traversal and return the reusable record owner.
    #[must_use]
    pub fn into_store(self) -> HistoryStore {
        self.store
    }

    /// Insert one owned logical line.
    pub fn push(&mut self, line: Text) -> Result<PushResult<()>, PushError<()>> {
        self.cursor.reset();
        self.store.push(line)
    }

    /// Remove all records and return this traversal to the live line.
    pub fn clear(&mut self) {
        self.store.clear();
        self.cursor.reset();
    }

    fn select(&mut self, id: HistoryId) -> Option<HistLine> {
        let entry = self.store.select(&mut self.cursor, id)?;
        Some(compatibility_line(entry))
    }

    fn navigate(&mut self, direction: Direction) -> Option<HistLine> {
        match self.store.navigate(&mut self.cursor, direction) {
            Navigation::Entry(entry) => Some(compatibility_line(entry)),
            Navigation::Live | Navigation::Boundary => None,
        }
    }
}

impl From<HistoryStore> for HistorySession {
    fn from(store: HistoryStore) -> Self {
        Self {
            store,
            cursor: HistoryCursor::new(),
        }
    }
}

impl EditorHistory for HistorySession {
    fn first(&mut self) -> Option<HistLine> {
        let id = self.store.newest()?.id();
        self.select(id)
    }

    fn last(&mut self) -> Option<HistLine> {
        let id = self.store.oldest()?.id();
        self.select(id)
    }

    fn next(&mut self) -> Option<HistLine> {
        self.navigate(Direction::Previous)
    }

    fn prev(&mut self) -> Option<HistLine> {
        self.navigate(Direction::Next)
    }

    fn set_size(&mut self, entries: i32) -> i32 {
        let Ok(entries) = usize::try_from(entries) else {
            return -1;
        };
        let Some(capacity) = NonZeroUsize::new(entries) else {
            return -1;
        };
        self.store.set_capacity(Some(capacity));
        self.cursor.reset();
        0
    }

    fn set_unique(&mut self, on: bool) -> i32 {
        self.store.set_duplicate_policy(if on {
            DuplicatePolicy::IgnoreConsecutive
        } else {
            DuplicatePolicy::Keep
        });
        0
    }
}

fn compatibility_line(entry: &HistoryEntry<()>) -> HistLine {
    let number = entry
        .id()
        .get()
        .checked_add(1)
        .and_then(|number| i32::try_from(number).ok())
        .unwrap_or(i32::MAX);
    let text = entry
        .line()
        .as_units()
        .iter()
        .copied()
        .map(|unit| match unit {
            TextUnit::Scalar(character) => u32::from(character),
            TextUnit::RawByte(byte) => u32::from(byte),
            TextUnit::CompatibilityWide(value) => value.get(),
        })
        .collect();
    HistLine {
        num: number,
        text: HistText::Wide(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inserted(result: Result<PushResult<()>, PushError<()>>) -> bool {
        matches!(result.unwrap(), PushResult::Inserted { .. })
    }

    #[test]
    fn new_session_is_unbounded() {
        let mut history = HistorySession::default();
        assert!(inserted(history.push(Text::from("first"))));
        assert!(inserted(history.push(Text::from("second"))));
        assert_eq!(history.store().len(), 2);
    }

    #[test]
    fn bound_evicts_oldest() {
        let mut history = HistorySession::bounded(NonZeroUsize::new(2).unwrap());
        for line in ["one", "two", "three"] {
            assert!(inserted(history.push(Text::from(line))));
        }
        assert_eq!(
            history.first().unwrap().text,
            HistText::Wide("three".chars().map(u32::from).collect())
        );
        assert_eq!(
            history.next().unwrap().text,
            HistText::Wide("two".chars().map(u32::from).collect())
        );
        assert!(history.next().is_none());
    }

    #[test]
    fn duplicates_are_typed_results() {
        let mut history = HistorySession::default();
        history
            .store_mut()
            .set_duplicate_policy(DuplicatePolicy::IgnoreConsecutive);
        assert!(inserted(history.push(Text::from("same"))));
        assert_eq!(
            history.push(Text::from("same")).unwrap(),
            PushResult::Duplicate {
                line: Text::from("same"),
                metadata: (),
            }
        );
    }

    #[test]
    fn editor_walk_uses_native_cursor() {
        let mut history = HistorySession::default();
        for line in ["oldest", "middle", "newest"] {
            assert!(inserted(history.push(Text::from(line))));
        }
        assert_eq!(history.first().unwrap().num, 3);
        assert_eq!(history.last().unwrap().num, 1);
        assert_eq!(history.prev().unwrap().num, 2);
        assert_eq!(history.next().unwrap().num, 1);
        assert!(history.next().is_none());
    }

    #[test]
    fn store_survives_session() {
        let mut history = HistorySession::default();
        assert!(inserted(history.push(Text::from("retained"))));
        let store = history.into_store();
        assert_eq!(store.newest().unwrap().line(), &Text::from("retained"));
    }
}
