//! C history adaptation over the native owned history store.
//!
//! The core owns logical [`Text`] records and stable identities. This module
//! owns everything that exists only because `histedit.h` does: integer event
//! numbers, one shared traversal cursor, replaceable C callbacks, borrowed
//! NUL-terminated strings, caller-owned deletion strings, opcode dispatch,
//! and the legacy file format's observable quirks.

mod backend;
mod dispatch;
mod persistence;
#[cfg(test)]
mod tests;

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;
use std::path::Path;

use nshedit::domain::Direction;
use nshedit::history::{
    DuplicatePolicy, HistoryCursor, HistoryEntry, HistoryId, HistoryStore, Navigation, PushResult,
};

use crate::adapter::BoundaryChar;
use crate::cdecl::handles::History as OpaqueHistory;
use crate::cdecl::histedit::HistEventGen;
use crate::conversion::{ConversionBuffer, decode_bytes, encode_wide};

pub(crate) use dispatch::dispatch;

const OK: c_int = 0;
const UNKNOWN: c_int = 1;
const ALLOCATION_FAILED: c_int = 2;
const FIRST_NOT_FOUND: c_int = 3;
const LAST_NOT_FOUND: c_int = 4;
const EMPTY_LIST: c_int = 5;
const END_REACHED: c_int = 6;
const START_REACHED: c_int = 7;
const CURRENT_INVALID: c_int = 8;
const NOT_FOUND: c_int = 9;
const HISTORY_READ_FAILED: c_int = 10;
const HISTORY_WRITE_FAILED: c_int = 11;
const PARAMETER_MISSING: c_int = 12;
const NOT_ALLOWED: c_int = 14;
const BAD_PARAMETER: c_int = 15;

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

// [spec:libedit:def:history.history-gfun-t-void-type-hist-event]
pub(crate) type GetCallback<C> = unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>) -> c_int;

// [spec:libedit:def:history.history-efun-t-void-type-hist-event-const-char]
pub(crate) type EnterCallback<C> =
    unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>, *const C) -> c_int;

// [spec:libedit:def:history.history-vfun-t-void-type-hist-event]
pub(crate) type ClearCallback<C> = unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>);

// [spec:libedit:def:history.history-sfun-t-void-type-hist-event-const-int]
pub(crate) type SelectCallback<C> =
    unsafe extern "C" fn(*mut c_void, *mut HistEventGen<C>, c_int) -> c_int;

#[derive(Clone, Copy)]
pub(crate) struct CallbackSet<C> {
    pub(crate) reference: *mut c_void,
    pub(crate) first: Option<GetCallback<C>>,
    pub(crate) next: Option<GetCallback<C>>,
    pub(crate) last: Option<GetCallback<C>>,
    pub(crate) previous: Option<GetCallback<C>>,
    pub(crate) current: Option<GetCallback<C>>,
    pub(crate) select: Option<SelectCallback<C>>,
    pub(crate) clear: Option<ClearCallback<C>>,
    pub(crate) enter: Option<EnterCallback<C>>,
    pub(crate) add: Option<EnterCallback<C>>,
    pub(crate) delete: Option<SelectCallback<C>>,
}

impl<C> CallbackSet<C> {
    fn is_complete(&self) -> bool {
        !self.reference.is_null()
            && self.first.is_some()
            && self.next.is_some()
            && self.last.is_some()
            && self.previous.is_some()
            && self.current.is_some()
            && self.select.is_some()
            && self.clear.is_some()
            && self.enter.is_some()
            && self.add.is_some()
            && self.delete.is_some()
    }
}

/// A caller-owned stream, borrowed for one save operation.
pub(crate) struct SaveStream<'a> {
    pub(crate) at_start: bool,
    pub(crate) output: &'a mut dyn Write,
}

pub(crate) enum DispatchArg<'a, C> {
    None,
    Number(c_int),
    Text(*const C),
    Path(Option<&'a Path>),
    Stream(SaveStream<'a>),
    LimitedStream(usize, SaveStream<'a>),
    EventData(c_int, *mut *mut c_void),
    Replace(*const C, *mut c_void),
    Callbacks(CallbackSet<C>),
}

// [spec:libedit:def:history.hentry-t]
#[derive(Debug)]
struct EntryBoundary<C> {
    event_number: c_int,
    c_string: Vec<C>,
    data: *mut c_void,
}

// [spec:libedit:def:history.hist-event-private]
#[repr(C)]
struct MutableEvent<C> {
    number: c_int,
    string: *mut C,
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
    next_event: c_int,
    last_entered: c_int,
    callbacks: Option<CallbackSet<C>>,
    callback_cookie: Box<CallbackCookie>,
}

pub(crate) type HistoryOwner = HistoryHandle<c_char>;
pub(crate) type HistoryWideOwner = HistoryHandle<u32>;

#[derive(Clone, Copy)]
enum GetOperation {
    First,
    Next,
    Last,
    Previous,
    Current,
}

#[derive(Clone, Copy)]
enum EnterOperation {
    Enter,
    Add,
}

#[derive(Clone, Copy)]
enum SelectOperation {
    Select,
    Delete,
}

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
            next_event: 0,
            last_entered: -1,
            callbacks: None,
            callback_cookie: Box::new(CallbackCookie { marker: 0 }),
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

    fn find_event(&self, number: c_int) -> Option<HistoryId> {
        self.store
            .iter()
            .find(|entry| entry.metadata().event_number == number)
            .map(HistoryEntry::id)
    }

    fn select_id(&mut self, id: HistoryId) -> bool {
        self.store.select(&mut self.cursor, id).is_some()
    }

    fn publish_id(&self, id: HistoryId, event: &mut HistEventGen<C>) -> c_int {
        let Some(entry) = self.store.get(id) else {
            set_error(event, CURRENT_INVALID);
            return -1;
        };
        publish(entry, event);
        0
    }

    // [spec:libedit:def:history.history-def-first-fn]
    // [spec:libedit:sem:history.history-def-first-fn]
    fn first(&mut self, event: &mut HistEventGen<C>) -> c_int {
        let Some(id) = self.store.newest().map(HistoryEntry::id) else {
            self.cursor.reset();
            set_error(event, FIRST_NOT_FOUND);
            return -1;
        };
        self.select_id(id);
        self.publish_id(id, event)
    }

    // [spec:libedit:def:history.history-def-last-fn]
    // [spec:libedit:sem:history.history-def-last-fn]
    fn last(&mut self, event: &mut HistEventGen<C>) -> c_int {
        let Some(id) = self.store.oldest().map(HistoryEntry::id) else {
            self.cursor.reset();
            set_error(event, LAST_NOT_FOUND);
            return -1;
        };
        self.select_id(id);
        self.publish_id(id, event)
    }

    // [spec:libedit:def:history.history-def-next-fn]
    // [spec:libedit:sem:history.history-def-next-fn]
    fn next(&mut self, event: &mut HistEventGen<C>) -> c_int {
        if self.cursor.is_live() {
            set_error(event, EMPTY_LIST);
            return -1;
        }
        match self.store.navigate(&mut self.cursor, Direction::Previous) {
            Navigation::Entry(entry) => {
                publish(entry, event);
                0
            }
            Navigation::Boundary | Navigation::Live => {
                set_error(event, END_REACHED);
                -1
            }
        }
    }

    // [spec:libedit:def:history.history-def-prev-fn]
    // [spec:libedit:sem:history.history-def-prev-fn]
    fn previous(&mut self, event: &mut HistEventGen<C>) -> c_int {
        let Some(id) = self.cursor.current() else {
            set_error(
                event,
                if self.store.is_empty() {
                    EMPTY_LIST
                } else {
                    END_REACHED
                },
            );
            return -1;
        };
        if self.position(id) == Some(0) {
            set_error(event, START_REACHED);
            return -1;
        }
        match self.store.navigate(&mut self.cursor, Direction::Next) {
            Navigation::Entry(entry) => {
                publish(entry, event);
                0
            }
            Navigation::Boundary | Navigation::Live => {
                set_error(event, START_REACHED);
                -1
            }
        }
    }

    // [spec:libedit:def:history.history-def-curr-fn]
    // [spec:libedit:sem:history.history-def-curr-fn]
    fn current(&self, event: &mut HistEventGen<C>) -> c_int {
        let Some(id) = self.cursor.current() else {
            set_error(
                event,
                if self.store.is_empty() {
                    EMPTY_LIST
                } else {
                    CURRENT_INVALID
                },
            );
            return -1;
        };
        self.publish_id(id, event)
    }

    // [spec:libedit:def:history.history-def-set-fn]
    // [spec:libedit:sem:history.history-def-set-fn]
    fn select_event(&mut self, event: &mut HistEventGen<C>, number: c_int) -> c_int {
        if self.store.is_empty() {
            set_error(event, EMPTY_LIST);
            return -1;
        }
        if self
            .cursor
            .current()
            .and_then(|id| self.store.get(id))
            .is_some_and(|entry| entry.metadata().event_number == number)
        {
            return 0;
        }
        let Some(id) = self.find_event(number) else {
            self.cursor.reset();
            set_error(event, NOT_FOUND);
            return -1;
        };
        self.select_id(id);
        0
    }

    // [spec:libedit:def:history.history-set-nth-fn]
    // [spec:libedit:sem:history.history-set-nth-fn]
    fn select_nth(&mut self, event: &mut HistEventGen<C>, number: c_int) -> c_int {
        if self.store.is_empty() {
            set_error(event, EMPTY_LIST);
            return -1;
        }
        let from_oldest = usize::try_from(number).unwrap_or(0);
        let Some(position) = from_oldest
            .checked_add(1)
            .and_then(|offset| self.store.len().checked_sub(offset))
        else {
            self.cursor.reset();
            set_error(event, NOT_FOUND);
            return -1;
        };
        let id = self
            .id_at(position)
            .expect("a checked native history position must exist");
        self.select_id(id);
        0
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
    fn insert(&mut self, event: &mut HistEventGen<C>, input: &[C]) -> c_int {
        let Some(number) = self.next_event.checked_add(1) else {
            set_error(event, ALLOCATION_FAILED);
            return -1;
        };
        let c_string = own_string(input);
        let text = input.iter().copied().map(BoundaryChar::into_unit).collect();
        let metadata = EntryBoundary {
            event_number: number,
            c_string,
            data: ptr::null_mut(),
        };
        let result = match self.store.push_with(text, metadata) {
            Ok(result) => result,
            Err(_) => {
                set_error(event, ALLOCATION_FAILED);
                return -1;
            }
        };
        let PushResult::Inserted { id, evicted } = result else {
            return 0;
        };
        debug_assert!(
            evicted.is_none(),
            "the ABI applies its delayed limit itself"
        );
        self.next_event = number;
        self.select_id(id);
        self.publish_id(id, event);

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
                Vec::leak(boundary.c_string);
            }
        }
        1
    }

    // [spec:libedit:def:history.history-def-enter-fn]
    // [spec:libedit:sem:history.history-def-enter-fn]
    fn enter(&mut self, event: &mut HistEventGen<C>, text: *const C) -> c_int {
        // SAFETY: the public C operation promises a NUL-terminated string;
        // NULL is an undefined C input defined here as the empty string.
        self.insert(event, unsafe { input(text) })
    }

    // [spec:libedit:def:history.history-def-add-fn]
    // [spec:libedit:sem:history.history-def-add-fn]
    fn add(&mut self, event: &mut HistEventGen<C>, text: *const C) -> c_int {
        let Some(id) = self.cursor.current() else {
            return self.enter(event, text);
        };
        // Copy first because a caller can pass a pointer borrowed from an
        // entry whose boundary buffer may reallocate below.
        // SAFETY: as in `enter`.
        let suffix = unsafe { input(text) }.to_vec();
        let Some(entry) = self.store.get_mut(id) else {
            self.cursor.reset();
            return self.insert(event, &suffix);
        };
        entry
            .line_mut()
            .extend(suffix.iter().copied().map(BoundaryChar::into_unit));
        let boundary = entry.metadata_mut();
        boundary.c_string.pop();
        boundary.c_string.extend_from_slice(&suffix);
        boundary.c_string.push(C::NUL);
        publish(entry, event);
        0
    }

    // [spec:libedit:def:history.history-def-del-fn]
    // [spec:libedit:sem:history.history-def-del-fn]
    fn delete_event(&mut self, event: &mut HistEventGen<C>, number: c_int) -> c_int {
        if self.select_event(event, number) != 0 {
            return -1;
        }
        let entry = self
            .delete_selected()
            .expect("successful selection names a retained entry");
        hand_over(&entry, event);
        0
    }

    // [spec:libedit:def:history.history-deldata-nth-fn]
    // [spec:libedit:sem:history.history-deldata-nth-fn]
    fn delete_nth(
        &mut self,
        event: &mut HistEventGen<C>,
        number: c_int,
        data: *mut *mut c_void,
    ) -> c_int {
        if self.select_nth(event, number) != 0 {
            return -1;
        }
        if data.addr() == usize::MAX {
            return 0;
        }
        let entry = self
            .delete_selected()
            .expect("successful positional selection names an entry");
        hand_over(&entry, event);
        if !data.is_null() {
            // SAFETY: this is the caller's non-null out-parameter.
            unsafe { *data = entry.metadata().data };
        }
        0
    }

    // [spec:libedit:def:history.history-def-clear-fn]
    // [spec:libedit:sem:history.history-def-clear-fn]
    fn clear(&mut self) {
        self.store.clear();
        self.cursor.reset();
        self.next_event = 0;
    }

    // [spec:libedit:def:history.history-setsize-fn]
    // [spec:libedit:sem:history.history-setsize-fn]
    fn set_size(&mut self, event: &mut HistEventGen<C>, size: c_int) -> c_int {
        if !self.is_builtin() {
            set_error(event, NOT_ALLOWED);
            return -1;
        }
        let Ok(size) = usize::try_from(size) else {
            set_error(event, BAD_PARAMETER);
            return -1;
        };
        self.limit = size;
        0
    }

    // [spec:libedit:def:history.history-getsize-fn]
    // [spec:libedit:sem:history.history-getsize-fn]
    fn get_size(&self, event: &mut HistEventGen<C>) -> c_int {
        if !self.is_builtin() {
            set_error(event, NOT_ALLOWED);
            return -1;
        }
        event.num = c_int::try_from(self.store.len()).unwrap_or(c_int::MAX);
        0
    }

    // [spec:libedit:def:history.history-setunique-fn]
    // [spec:libedit:sem:history.history-setunique-fn]
    fn set_unique(&mut self, event: &mut HistEventGen<C>, unique: c_int) -> c_int {
        if !self.is_builtin() {
            set_error(event, NOT_ALLOWED);
            return -1;
        }
        self.store.set_duplicate_policy(if unique == 0 {
            DuplicatePolicy::Keep
        } else {
            DuplicatePolicy::IgnoreConsecutive
        });
        0
    }

    // [spec:libedit:def:history.history-getunique-fn]
    // [spec:libedit:sem:history.history-getunique-fn]
    fn get_unique(&self, event: &mut HistEventGen<C>) -> c_int {
        if !self.is_builtin() {
            set_error(event, NOT_ALLOWED);
            return -1;
        }
        event.num =
            c_int::from(self.store.duplicate_policy() == DuplicatePolicy::IgnoreConsecutive);
        0
    }
}

fn set_error<C: HistoryChar>(event: &mut HistEventGen<C>, code: c_int) {
    event.num = code;
    event.str = C::errors()[code as usize].as_ptr();
}

fn publish<C>(entry: &HistoryEntry<EntryBoundary<C>>, event: &mut HistEventGen<C>) {
    event.num = entry.metadata().event_number;
    event.str = entry.metadata().c_string.as_ptr();
}

fn own_string<C: HistoryChar>(input: &[C]) -> Vec<C> {
    let mut owned = Vec::with_capacity(input.len() + 1);
    owned.extend_from_slice(input);
    owned.push(C::NUL);
    owned
}

fn owned_copy<C: HistoryChar>(source: &[C]) -> *mut C {
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

fn hand_over<C: HistoryChar>(entry: &HistoryEntry<EntryBoundary<C>>, event: &mut HistEventGen<C>) {
    let source = &entry.metadata().c_string[..entry.metadata().c_string.len() - 1];
    let owned = MutableEvent {
        number: entry.metadata().event_number,
        string: owned_copy(source),
    };
    event.num = owned.number;
    event.str = owned.string;
}

/// Read the characters before a C string's terminator. NULL is defined as an
/// empty string for the reference implementation's undefined pointer inputs.
///
/// # Safety
///
/// A non-null pointer must name a live NUL-terminated string.
unsafe fn input<'a, C: HistoryChar>(string: *const C) -> &'a [C] {
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
) -> c_int {
    // SAFETY: the caller guarantees that a non-null opaque handle points at
    // the narrow owner allocated by `history_init`.
    let Some(history) = (unsafe { handle.cast::<HistoryOwner>().as_mut() }) else {
        return -1;
    };
    persistence::save_fd(history, count, descriptor)
}
