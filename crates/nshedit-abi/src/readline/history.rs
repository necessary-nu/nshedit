//! Typed access to readline's process-global history owner.

use core::ffi::{c_char, c_int};

use crate::history::{
    EntryData, HistoryError, HistoryErrorKind, HistoryEvent, HistoryHandle, HistoryOwner,
    HistoryReply, HistoryRequest, HistoryResult,
};

use super::H;

unsafe fn owner() -> Result<&'static mut HistoryOwner, HistoryErrorKind> {
    // SAFETY: readline serializes access to its process-global history owner.
    unsafe { H.cast::<HistoryHandle<c_char>>().as_mut() }.ok_or(HistoryErrorKind::Unknown)
}

pub(super) unsafe fn execute(request: HistoryRequest<'_, c_char>) -> HistoryResult<c_char> {
    // SAFETY: inherited from this module's serialized global-owner contract.
    unsafe { owner() }
        .map_err(HistoryError::Known)?
        .execute(request)
}

pub(super) unsafe fn boundary_text(event: &HistoryEvent<c_char>) -> *const c_char {
    // SAFETY: the exported caller is borrowing from the live global owner.
    unsafe { owner() }.map_or(core::ptr::null(), |history| history.boundary_text(event))
}

pub(super) fn event(reply: HistoryReply<c_char>) -> Option<HistoryEvent<c_char>> {
    match reply {
        HistoryReply::Event(event)
        | HistoryReply::Insertion {
            event: Some(event), ..
        }
        | HistoryReply::Removed { event, .. }
        | HistoryReply::EventData { event, .. } => Some(event),
        _ => None,
    }
}

pub(super) fn size(reply: HistoryReply<c_char>) -> Option<c_int> {
    match reply {
        HistoryReply::Size(value) => Some(c_int::try_from(value).unwrap_or(c_int::MAX)),
        _ => None,
    }
}

pub(super) fn removed(reply: HistoryReply<c_char>) -> Option<(HistoryEvent<c_char>, EntryData)> {
    match reply {
        HistoryReply::Removed { event, data } => Some((event, data)),
        _ => None,
    }
}

pub(super) fn bytes(event: &HistoryEvent<c_char>) -> Option<&[u8]> {
    let text = event.text.as_deref()?;
    // SAFETY: `c_char` and `u8` have identical layout and every bit pattern is
    // valid for both; this is a borrowed representation change only.
    Some(unsafe { core::slice::from_raw_parts(text.as_ptr().cast::<u8>(), text.len()) })
}
