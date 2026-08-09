//! Typed access to an application-supplied history callback.

use core::ffi::{c_int, c_void};
use std::ffi::CStr;

use nshedit::domain::{Text, TextUnit};

use crate::cdecl::histedit::{
    H_FIRST, H_LAST, H_NEXT, H_PREV, H_SETSIZE, H_SETUNIQUE, HistEvent, HistEventWide,
};
use crate::conversion::{ConversionBuffer, decode_bytes};
use crate::history::{EventNumber, HistoryMove};

pub(crate) type HistoryCallback =
    unsafe extern "C" fn(*mut c_void, *mut HistEventWide, c_int, ...) -> c_int;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryEncoding {
    Narrow,
    Wide,
}

#[derive(Clone, Copy)]
pub(crate) struct HistorySource {
    callback: HistoryCallback,
    cookie: *mut c_void,
    encoding: HistoryEncoding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryItem {
    number: EventNumber,
    line: Text,
    boundary_text: Vec<u32>,
}

impl HistoryItem {
    pub(crate) const fn number(&self) -> EventNumber {
        self.number
    }

    pub(crate) fn into_line(self) -> Text {
        self.line
    }

    pub(crate) fn into_boundary_text(self) -> Vec<u32> {
        self.boundary_text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryPolicy {
    Limit(c_int),
    Unique(bool),
}

impl HistorySource {
    pub(crate) const fn new(
        callback: HistoryCallback,
        cookie: *mut c_void,
        encoding: HistoryEncoding,
    ) -> Self {
        Self {
            callback,
            cookie,
            encoding,
        }
    }

    pub(crate) fn is_available(self) -> bool {
        !self.cookie.is_null()
    }

    fn operation(movement: HistoryMove) -> Option<c_int> {
        match movement {
            HistoryMove::Newest => Some(H_FIRST),
            HistoryMove::Older => Some(H_NEXT),
            HistoryMove::Oldest => Some(H_LAST),
            HistoryMove::Newer => Some(H_PREV),
            HistoryMove::Current => None,
        }
    }

    /// Invoke the foreign ABI callback and immediately own its event as a Rust
    /// value. No event record or operation code escapes this boundary adapter.
    pub(crate) unsafe fn item(self, movement: HistoryMove) -> Option<HistoryItem> {
        let operation = Self::operation(movement)?;
        if !self.is_available() {
            return None;
        }
        match self.encoding {
            HistoryEncoding::Narrow => {
                let mut event = HistEvent {
                    num: 0,
                    str: core::ptr::null(),
                };
                // SAFETY: the callback and cookie were installed together by
                // `EL_HIST`; this local record has the selected narrow layout.
                if unsafe {
                    (self.callback)(
                        self.cookie,
                        (&raw mut event).cast(),
                        operation,
                        core::ptr::null_mut::<c_void>(),
                    )
                } == -1
                {
                    return None;
                }
                // SAFETY: a successful callback returns a terminated string.
                let bytes = unsafe { CStr::from_ptr(event.str) }.to_bytes();
                let mut conversion = ConversionBuffer::new();
                let boundary_text = decode_bytes(Some(bytes), &mut conversion)?.to_vec();
                let line = boundary_text
                    .iter()
                    .copied()
                    .map(TextUnit::from_code_point)
                    .collect();
                Some(HistoryItem {
                    number: EventNumber(event.num),
                    line,
                    boundary_text,
                })
            }
            HistoryEncoding::Wide => {
                let mut event = HistEventWide {
                    num: 0,
                    str: core::ptr::null(),
                };
                // SAFETY: as above, using the callback's declared wide event.
                if unsafe {
                    (self.callback)(
                        self.cookie,
                        &raw mut event,
                        operation,
                        core::ptr::null_mut::<c_void>(),
                    )
                } == -1
                {
                    return None;
                }
                if event.str.is_null() {
                    return None;
                }
                let mut length = 0;
                // SAFETY: the successful callback returned a terminated array.
                while unsafe { *event.str.add(length) } != 0 {
                    length += 1;
                }
                // SAFETY: the preceding scan established this live range.
                let boundary_text =
                    unsafe { core::slice::from_raw_parts(event.str, length) }.to_vec();
                let line = boundary_text
                    .iter()
                    .copied()
                    .map(TextUnit::from_code_point)
                    .collect();
                Some(HistoryItem {
                    number: EventNumber(event.num),
                    line,
                    boundary_text,
                })
            }
        }
    }

    pub(crate) unsafe fn set_policy(self, policy: HistoryPolicy) -> bool {
        if !self.is_available()
            || self.encoding == HistoryEncoding::Narrow
            || self.callback as *const () != crate::histedit::history_w as *const ()
        {
            return false;
        }
        let (operation, value) = match policy {
            HistoryPolicy::Limit(value) => (H_SETSIZE, value),
            HistoryPolicy::Unique(value) => (H_SETUNIQUE, c_int::from(value)),
        };
        let mut event = HistEventWide {
            num: 0,
            str: core::ptr::null(),
        };
        // SAFETY: `history_w` accepts this operation/value pair and event type.
        (unsafe { (self.callback)(self.cookie, &raw mut event, operation, value) }) == 0
    }
}
