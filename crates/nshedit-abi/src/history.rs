//! C history adaptation over the native owned history store.
//!
//! The core owns logical [`Text`] records and stable identities. This module
//! owns everything that exists only because `histedit.h` does: integer event
//! numbers, one shared traversal cursor, replaceable C callbacks, borrowed
//! NUL-terminated strings, caller-owned deletion strings, opcode dispatch,
//! and the legacy file format's observable quirks.

mod backend;
mod dispatch;
mod model;
mod persistence;
#[cfg(test)]
mod tests;

use core::ffi::{c_char, c_void};
use core::ptr;
use std::alloc::{GlobalAlloc, Layout, System};

use nshedit::domain::Direction;
use nshedit::history::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushResult,
};

use crate::adapter::BoundaryChar;
use crate::cdecl::handles::History as OpaqueHistory;
use crate::conversion::{ConversionBuffer, decode_bytes, encode_wide};

pub(crate) use dispatch::dispatch;
pub(crate) use model::{
    CallbackSet, ClearCallback, DataAccess, DeleteMode, EnterCallback, EntryData, EventNumber,
    GetCallback, HistoryError, HistoryErrorKind, HistoryEvent, HistoryMove, HistoryReply,
    HistoryRequest, HistoryResult, Insertion, SaveStream, SeekDirection, SelectCallback,
};

macro_rules! error_tables {
    ($($message:literal),+ $(,)?) => {
        static WIDE_ERRORS: [&[u32]; 16] = [$({
            const LENGTH: usize = $message.len() + 1;
            const VALUE: [u32; LENGTH] = {
                let bytes = $message.as_bytes();
                let mut value = [0; LENGTH];
                let mut index = 0;
                while index < bytes.len() {
                    value[index] = bytes[index] as u32;
                    index += 1;
                }
                value
            };
            &VALUE
        }),+];

        static NARROW_ERRORS: [&[c_char]; 16] = [$({
            const LENGTH: usize = $message.len() + 1;
            const VALUE: [c_char; LENGTH] = {
                let bytes = $message.as_bytes();
                let mut value = [0; LENGTH];
                let mut index = 0;
                while index < bytes.len() {
                    value[index] = bytes[index] as c_char;
                    index += 1;
                }
                value
            };
            &VALUE
        }),+];
    };
}

error_tables!(
    "OK",
    "unknown error",
    "malloc() failed",
    "first event not found",
    "last event not found",
    "empty list",
    "no next event",
    "no previous event",
    "current event is invalid",
    "event not found",
    "can't read history from file",
    "can't write history",
    "required parameter(s) not supplied",
    "history size negative",
    "function not allowed with other history-functions-set the default",
    "bad parameters",
);

pub(crate) trait HistoryChar: BoundaryChar + 'static {
    fn errors() -> &'static [&'static [Self]; 16];

    fn decode<'a>(bytes: Option<&'a [u8]>, buffer: &'a mut ConversionBuffer) -> Option<&'a [Self]>;

    fn encode<'a>(text: Option<&'a [Self]>, buffer: &'a mut ConversionBuffer) -> Option<&'a [u8]>;
}

impl HistoryChar for c_char {
    fn errors() -> &'static [&'static [Self]; 16] {
        &NARROW_ERRORS
    }

    fn decode<'a>(
        bytes: Option<&'a [u8]>,
        _buffer: &'a mut ConversionBuffer,
    ) -> Option<&'a [Self]> {
        let bytes = bytes?;
        // SAFETY: `c_char` and `u8` have identical layout and every bit
        // pattern is valid for both.
        Some(unsafe { core::slice::from_raw_parts(bytes.as_ptr().cast::<c_char>(), bytes.len()) })
    }

    fn encode<'a>(text: Option<&'a [Self]>, _buffer: &'a mut ConversionBuffer) -> Option<&'a [u8]> {
        let text = text?;
        // SAFETY: as in `decode`.
        Some(unsafe { core::slice::from_raw_parts(text.as_ptr().cast::<u8>(), text.len()) })
    }
}

impl HistoryChar for u32 {
    fn errors() -> &'static [&'static [Self]; 16] {
        &WIDE_ERRORS
    }

    fn decode<'a>(bytes: Option<&'a [u8]>, buffer: &'a mut ConversionBuffer) -> Option<&'a [Self]> {
        decode_bytes(bytes, buffer)
    }

    fn encode<'a>(text: Option<&'a [Self]>, buffer: &'a mut ConversionBuffer) -> Option<&'a [u8]> {
        encode_wide(text, buffer)
    }
}

// [spec:libedit:def:history.hentry-t]
#[derive(Debug)]
struct EntryBoundary<C> {
    event_number: EventNumber,
    c_string: Vec<C>,
    data: EntryData,
}

#[repr(C, align(16))]
struct CallbackCookie {
    marker: usize,
}

// [spec:nshedit:req:abi.opaque-owner]
// [spec:libedit:def:history.history-t]
/// Allocation behind either incomplete C history handle.
///
/// The native store is the only record store. Every other field exists to
/// reproduce C traversal, callback, numbering, or pointer-lifetime rules.
pub(crate) struct HistoryHandle<C> {
    store: HistoryStore<EntryBoundary<C>>,
    cursor: HistoryCursor,
    limit: usize,
    next_event: EventNumber,
    last_entered: Option<EventNumber>,
    callbacks: Option<CallbackSet<C>>,
    callback_cookie: Box<CallbackCookie>,
    published: Vec<C>,
}

pub(crate) type HistoryOwner = HistoryHandle<c_char>;
pub(crate) type HistoryWideOwner = HistoryHandle<u32>;

impl<C: HistoryChar> HistoryHandle<C> {
    // [spec:libedit:def:history.history-def-init-fn]
    // [spec:libedit:sem:history.history-def-init-fn]
    // [spec:libedit:def:history.fun-history-init-fn]
    // [spec:libedit:sem:history.fun-history-init-fn]
    #[must_use]
    pub(crate) fn new_raw() -> *mut Self {
        Box::into_raw(Box::new(Self {
            store: HistoryStore::new(),
            cursor: HistoryCursor::new(),
            limit: 0,
            next_event: EventNumber(0),
            last_entered: None,
            callbacks: None,
            callback_cookie: Box::new(CallbackCookie { marker: 0 }),
            published: Vec::new(),
        }))
    }

    fn is_builtin(&self) -> bool {
        self.callbacks.is_none()
    }

    fn cookie(&mut self) -> *mut c_void {
        ptr::from_mut(self.callback_cookie.as_mut()).cast()
    }

    fn position(&self, id: HistoryId) -> Option<usize> {
        self.store.iter().position(|entry| entry.id() == id)
    }

    fn id_at(&self, position: usize) -> Option<HistoryId> {
        self.store.iter().nth(position).map(HistoryEntry::id)
    }

    fn find_event(&self, number: EventNumber) -> Option<HistoryId> {
        self.store
            .iter()
            .find(|entry| entry.metadata().event_number == number)
            .map(HistoryEntry::id)
    }

    fn select_id(&mut self, id: HistoryId) -> bool {
        self.store.select(&mut self.cursor, id).is_some()
    }

    fn event_for_id(&self, id: HistoryId) -> Result<HistoryEvent<C>, HistoryErrorKind> {
        let entry = self.store.get(id).ok_or(HistoryErrorKind::CurrentInvalid)?;
        let boundary = entry.metadata();
        Ok(HistoryEvent::retained(
            boundary.event_number,
            boundary.c_string[..boundary.c_string.len() - 1].to_vec(),
            id,
        ))
    }

    /// Project a typed event into the pointer lifetime promised by an exported
    /// compatibility function.
    pub(crate) fn boundary_text(&mut self, event: &HistoryEvent<C>) -> *const C {
        if let Some(id) = event.retained
            && let Some(entry) = self.store.get(id)
        {
            return entry.metadata().c_string.as_ptr();
        }
        let Some(text) = event.text.as_deref() else {
            return ptr::null();
        };
        self.published = own_string(text);
        self.published.as_ptr()
    }

    // [spec:libedit:def:history.history-def-first-fn]
    // [spec:libedit:sem:history.history-def-first-fn]
    fn first(&mut self) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let Some(id) = self.store.newest().map(HistoryEntry::id) else {
            self.cursor.reset();
            return Err(HistoryErrorKind::FirstNotFound.into());
        };
        self.select_id(id);
        self.event_for_id(id).map_err(Into::into)
    }

    // [spec:libedit:def:history.history-def-last-fn]
    // [spec:libedit:sem:history.history-def-last-fn]
    fn last(&mut self) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let Some(id) = self.store.oldest().map(HistoryEntry::id) else {
            self.cursor.reset();
            return Err(HistoryErrorKind::LastNotFound.into());
        };
        self.select_id(id);
        self.event_for_id(id).map_err(Into::into)
    }

    // [spec:libedit:def:history.history-def-next-fn]
    // [spec:libedit:sem:history.history-def-next-fn]
    fn next(&mut self) -> Result<HistoryEvent<C>, HistoryError<C>> {
        if self.cursor.is_live() {
            return Err(HistoryErrorKind::Empty.into());
        }
        match self.store.navigate(&mut self.cursor, Direction::Previous) {
            Navigation::Entry(entry) => Ok(event_from_entry(entry)),
            Navigation::Boundary | Navigation::Live => Err(HistoryErrorKind::EndReached.into()),
        }
    }

    // [spec:libedit:def:history.history-def-prev-fn]
    // [spec:libedit:sem:history.history-def-prev-fn]
    fn previous(&mut self) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let Some(id) = self.cursor.current() else {
            return Err(if self.store.is_empty() {
                HistoryErrorKind::Empty
            } else {
                HistoryErrorKind::EndReached
            }
            .into());
        };
        if self.position(id) == Some(0) {
            return Err(HistoryErrorKind::StartReached.into());
        }
        match self.store.navigate(&mut self.cursor, Direction::Next) {
            Navigation::Entry(entry) => Ok(event_from_entry(entry)),
            Navigation::Boundary | Navigation::Live => Err(HistoryErrorKind::StartReached.into()),
        }
    }

    // [spec:libedit:def:history.history-def-curr-fn]
    // [spec:libedit:sem:history.history-def-curr-fn]
    fn current(&self) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let Some(id) = self.cursor.current() else {
            return Err(if self.store.is_empty() {
                HistoryErrorKind::Empty
            } else {
                HistoryErrorKind::CurrentInvalid
            }
            .into());
        };
        self.event_for_id(id).map_err(Into::into)
    }

    // [spec:libedit:def:history.history-def-set-fn]
    // [spec:libedit:sem:history.history-def-set-fn]
    fn select_event(&mut self, number: EventNumber) -> Result<(), HistoryError<C>> {
        if self.store.is_empty() {
            return Err(HistoryErrorKind::Empty.into());
        }
        if self
            .cursor
            .current()
            .and_then(|id| self.store.get(id))
            .is_some_and(|entry| entry.metadata().event_number == number)
        {
            return Ok(());
        }
        let Some(id) = self.find_event(number) else {
            self.cursor.reset();
            return Err(HistoryErrorKind::NotFound.into());
        };
        self.select_id(id);
        Ok(())
    }

    // [spec:libedit:def:history.history-set-nth-fn]
    // [spec:libedit:sem:history.history-set-nth-fn]
    fn select_nth(&mut self, from_oldest: usize) -> Result<(), HistoryError<C>> {
        if self.store.is_empty() {
            return Err(HistoryErrorKind::Empty.into());
        }
        let Some(position) = from_oldest
            .checked_add(1)
            .and_then(|offset| self.store.len().checked_sub(offset))
        else {
            self.cursor.reset();
            return Err(HistoryErrorKind::NotFound.into());
        };
        let id = self
            .id_at(position)
            .expect("a checked native history position must exist");
        self.select_id(id);
        Ok(())
    }

    fn remove_entry(&mut self, id: HistoryId) -> Option<HistoryEntry<EntryBoundary<C>>> {
        let selected = self.cursor.current() == Some(id);
        let replacement = self.position(id).and_then(|position| {
            if position > 0 {
                self.id_at(position - 1)
            } else {
                self.id_at(1)
            }
        });
        let removed = self.store.remove(id)?;
        if selected {
            self.cursor.reset();
            if let Some(replacement) = replacement {
                self.select_id(replacement);
            }
        }
        Some(removed)
    }

    // [spec:libedit:def:history.history-def-delete-fn]
    // [spec:libedit:sem:history.history-def-delete-fn]
    fn delete_selected(&mut self) -> Option<HistoryEntry<EntryBoundary<C>>> {
        self.remove_entry(self.cursor.current()?)
    }

    // [spec:libedit:def:history.history-def-insert-fn]
    // [spec:libedit:sem:history.history-def-insert-fn]
    fn insert(&mut self, input: &[C]) -> HistoryResult<C> {
        let Some(number) = self.next_event.0.checked_add(1).map(EventNumber) else {
            return Err(HistoryErrorKind::AllocationFailed.into());
        };
        let c_string = own_string(input);
        let text = input.iter().copied().map(BoundaryChar::into_unit).collect();
        let metadata = EntryBoundary {
            event_number: number,
            c_string,
            data: EntryData::NONE,
        };
        let result = match self.store.push_with(text, metadata) {
            Ok(result) => result,
            Err(_) => return Err(HistoryErrorKind::AllocationFailed.into()),
        };
        let PushResult::Inserted { id, evicted } = result else {
            return Ok(HistoryReply::Insertion {
                state: Insertion::Unchanged,
                event: None,
            });
        };
        debug_assert!(
            evicted.is_none(),
            "the ABI applies its delayed limit itself"
        );
        self.next_event = number;
        self.select_id(id);
        let mut event = self.event_for_id(id).map_err(HistoryError::from)?;

        while self.store.len() > self.limit {
            let oldest = self
                .store
                .oldest()
                .map(HistoryEntry::id)
                .expect("a non-empty store has an oldest entry");
            let removed = self
                .remove_entry(oldest)
                .expect("the selected oldest entry must remain present");
            if oldest == id {
                let (_, _, boundary) = removed.into_parts();
                event.retained = None;
                self.published = boundary.c_string;
            }
        }
        Ok(HistoryReply::Insertion {
            state: Insertion::Inserted,
            event: Some(event),
        })
    }

    // [spec:libedit:def:history.history-def-enter-fn]
    // [spec:libedit:sem:history.history-def-enter-fn]
    fn enter(&mut self, text: &[C]) -> HistoryResult<C> {
        self.insert(text)
    }

    // [spec:libedit:def:history.history-def-add-fn]
    // [spec:libedit:sem:history.history-def-add-fn]
    fn add(&mut self, text: &[C]) -> HistoryResult<C> {
        let Some(id) = self.cursor.current() else {
            return self.enter(text);
        };
        // Copy first because a caller can pass a pointer borrowed from an
        // entry whose boundary buffer may reallocate below.
        let suffix = text.to_vec();
        let Some(entry) = self.store.get_mut(id) else {
            self.cursor.reset();
            return self.insert(&suffix);
        };
        entry
            .line_mut()
            .extend(suffix.iter().copied().map(BoundaryChar::into_unit));
        let boundary = entry.metadata_mut();
        boundary.c_string.pop();
        boundary.c_string.extend_from_slice(&suffix);
        boundary.c_string.push(C::NUL);
        Ok(HistoryReply::Insertion {
            state: Insertion::Unchanged,
            event: Some(event_from_entry(entry)),
        })
    }

    // [spec:libedit:def:history.history-def-del-fn]
    // [spec:libedit:sem:history.history-def-del-fn]
    fn delete_event(&mut self, number: EventNumber) -> HistoryResult<C> {
        self.select_event(number)?;
        let entry = self
            .delete_selected()
            .expect("successful selection names a retained entry");
        let data = entry.metadata().data;
        Ok(HistoryReply::Removed {
            event: detached_event(&entry),
            data,
        })
    }

    // [spec:libedit:def:history.history-deldata-nth-fn]
    // [spec:libedit:sem:history.history-deldata-nth-fn]
    fn delete_nth(&mut self, position_from_oldest: usize, mode: DeleteMode) -> HistoryResult<C> {
        self.select_nth(position_from_oldest)?;
        if mode == DeleteMode::SelectOnly {
            return Ok(HistoryReply::Complete);
        }
        let entry = self
            .delete_selected()
            .expect("successful positional selection names an entry");
        let data = entry.metadata().data;
        Ok(HistoryReply::Removed {
            event: detached_event(&entry),
            data,
        })
    }

    // [spec:libedit:def:history.history-def-clear-fn]
    // [spec:libedit:sem:history.history-def-clear-fn]
    fn clear(&mut self) {
        self.store.clear();
        self.cursor.reset();
        self.next_event = EventNumber(0);
    }

    // [spec:libedit:def:history.history-setsize-fn]
    // [spec:libedit:sem:history.history-setsize-fn]
    fn set_size(&mut self, size: usize) -> HistoryResult<C> {
        if !self.is_builtin() {
            return Err(HistoryErrorKind::NotAllowed.into());
        }
        self.limit = size;
        Ok(HistoryReply::Complete)
    }

    // [spec:libedit:def:history.history-getsize-fn]
    // [spec:libedit:sem:history.history-getsize-fn]
    fn get_size(&self) -> HistoryResult<C> {
        if !self.is_builtin() {
            return Err(HistoryErrorKind::NotAllowed.into());
        }
        Ok(HistoryReply::Size(self.store.len()))
    }

    // [spec:libedit:def:history.history-setunique-fn]
    // [spec:libedit:sem:history.history-setunique-fn]
    fn set_unique(&mut self, unique: bool) -> HistoryResult<C> {
        if !self.is_builtin() {
            return Err(HistoryErrorKind::NotAllowed.into());
        }
        self.store.set_duplicate_policy(if unique {
            DuplicatePolicy::IgnoreConsecutive
        } else {
            DuplicatePolicy::Keep
        });
        Ok(HistoryReply::Complete)
    }

    // [spec:libedit:def:history.history-getunique-fn]
    // [spec:libedit:sem:history.history-getunique-fn]
    fn get_unique(&self) -> HistoryResult<C> {
        if !self.is_builtin() {
            return Err(HistoryErrorKind::NotAllowed.into());
        }
        Ok(HistoryReply::Unique(
            self.store.duplicate_policy() == DuplicatePolicy::IgnoreConsecutive,
        ))
    }
}

fn event_from_entry<C: Clone>(entry: &HistoryEntry<EntryBoundary<C>>) -> HistoryEvent<C> {
    let boundary = entry.metadata();
    HistoryEvent::retained(
        boundary.event_number,
        boundary.c_string[..boundary.c_string.len() - 1].to_vec(),
        entry.id(),
    )
}

fn detached_event<C: Clone>(entry: &HistoryEntry<EntryBoundary<C>>) -> HistoryEvent<C> {
    let boundary = entry.metadata();
    HistoryEvent::detached(
        boundary.event_number,
        Some(boundary.c_string[..boundary.c_string.len() - 1].to_vec()),
    )
}

fn own_string<C: HistoryChar>(input: &[C]) -> Vec<C> {
    let mut owned = Vec::with_capacity(input.len() + 1);
    owned.extend_from_slice(input);
    owned.push(C::NUL);
    owned
}

pub(super) fn owned_copy<C: HistoryChar>(source: &[C]) -> *mut C {
    let Some(length) = source.len().checked_add(1) else {
        return ptr::null_mut();
    };
    let Ok(layout) = Layout::array::<C>(length) else {
        return ptr::null_mut();
    };
    // SAFETY: `layout` describes `length` elements of `C`. Ownership of a
    // successful allocation crosses to the C caller, which releases it with
    // the platform allocator.
    let allocation = unsafe { System.alloc(layout).cast::<C>() };
    if allocation.is_null() {
        return allocation;
    }
    // SAFETY: the allocation has `length` slots and the two sources are live.
    unsafe {
        ptr::copy_nonoverlapping(source.as_ptr(), allocation, source.len());
        allocation.add(source.len()).write(C::NUL);
    }
    allocation
}

pub(crate) fn transfer_text<C: HistoryChar>(event: &HistoryEvent<C>) -> *mut C {
    event.text.as_deref().map_or(ptr::null_mut(), owned_copy)
}

/// Read the characters before a C string's terminator. NULL is defined as an
/// empty string for the reference implementation's undefined pointer inputs.
///
/// # Safety
///
/// A non-null pointer must name a live NUL-terminated string.
pub(super) unsafe fn input<'a, C: HistoryChar>(string: *const C) -> &'a [C] {
    if string.is_null() {
        return &[];
    }
    let mut length = 0;
    // SAFETY: guaranteed by this function's contract.
    while unsafe { *string.add(length) } != C::NUL {
        length += 1;
    }
    // SAFETY: the preceding scan established the live range.
    unsafe { core::slice::from_raw_parts(string, length) }
}

// [spec:libedit:def:history.fun-history-end-fn]
// [spec:libedit:sem:history.fun-history-end-fn]
pub(crate) unsafe fn end<C: HistoryChar>(handle: *mut HistoryHandle<C>) {
    if !handle.is_null() {
        // SAFETY: the caller transfers its live allocation exactly once.
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Persist the narrow owner behind readline's declaration-only C handle.
///
/// # Safety
///
/// `handle` must be null or the live allocation returned by `history_init`.
pub(crate) unsafe fn save_fd(
    handle: *mut OpaqueHistory,
    count: usize,
    descriptor: std::os::fd::RawFd,
) -> HistoryResult<c_char> {
    // SAFETY: the caller guarantees that a non-null opaque handle points at
    // the narrow owner allocated by `history_init`.
    let Some(history) = (unsafe { handle.cast::<HistoryOwner>().as_mut() }) else {
        return Err(HistoryErrorKind::WriteFailed.into());
    };
    persistence::save_fd(history, count, descriptor)
}
