//! Projection of typed history results into the exported C protocol.

use core::ffi::{c_int, c_void};

use crate::cdecl::histedit::HistEventGen;

use super::*;

fn set_error<C: HistoryChar>(event: &mut HistEventGen<C>, error: HistoryErrorKind) {
    let code = error.code();
    event.num = code;
    event.str = C::errors()[code as usize].as_ptr();
}

fn set_ok<C: HistoryChar>(event: &mut HistEventGen<C>) {
    event.num = 0;
    event.str = C::errors()[0].as_ptr();
}

fn publish<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    value: HistoryEvent<C>,
) {
    event.num = value.number.0;
    event.str = history.boundary_text(&value);
}

fn publish_removed<C: HistoryChar>(event: &mut HistEventGen<C>, value: HistoryEvent<C>) {
    event.num = value.number.0;
    event.str = transfer_text(&value).cast_const();
}

fn write_data(output: Option<*mut *mut c_void>, data: EntryData) {
    if let Some(output) = output
        && !output.is_null()
    {
        // SAFETY: the exported operation supplied this writable out-parameter.
        unsafe { output.write(data.as_raw()) };
    }
}

fn publish_reply<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    reply: HistoryReply<C>,
    data_output: Option<*mut *mut c_void>,
) -> c_int {
    match reply {
        HistoryReply::Complete => 0,
        HistoryReply::Event(value) => {
            publish(history, event, value);
            0
        }
        HistoryReply::Insertion {
            state,
            event: value,
        } => {
            if let Some(value) = value {
                publish(history, event, value);
            }
            c_int::from(state == Insertion::Inserted)
        }
        HistoryReply::Removed { event: value, data } => {
            publish_removed(event, value);
            write_data(data_output, data);
            0
        }
        HistoryReply::Size(size) => {
            event.num = c_int::try_from(size).unwrap_or(c_int::MAX);
            0
        }
        HistoryReply::Unique(unique) => {
            event.num = c_int::from(unique);
            0
        }
        HistoryReply::Count(count) => c_int::try_from(count).unwrap_or(c_int::MAX),
        HistoryReply::EventData { event: value, data } => {
            publish(history, event, value);
            if let Some(data) = data {
                write_data(data_output, data);
            }
            0
        }
    }
}

fn publish_error<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    error: HistoryError<C>,
) -> c_int {
    match error {
        HistoryError::Known(error) => set_error(event, error),
        HistoryError::Foreign(value) => publish(history, event, value),
        HistoryError::Silent => {}
    }
    -1
}

// [spec:libedit:def:history.funw-history-fn]
// [spec:libedit:sem:history.funw-history-fn]
pub(crate) unsafe fn dispatch<C: HistoryChar>(
    handle: *mut HistoryHandle<C>,
    event: *mut HistEventGen<C>,
    request: Result<HistoryRequest<'_, C>, HistoryErrorKind>,
    data_output: Option<*mut *mut c_void>,
) -> c_int {
    // SAFETY: the exported entry point requires a writable event.
    let event = unsafe { &mut *event };
    set_ok(event);
    let Some(history) = (unsafe { handle.as_mut() }) else {
        set_error(event, HistoryErrorKind::Unknown);
        return -1;
    };
    let request = match request {
        Ok(request) => request,
        Err(error) => {
            set_error(event, error);
            return -1;
        }
    };
    match history.execute(request) {
        Ok(reply) => publish_reply(history, event, reply, data_output),
        Err(error) => publish_error(history, event, error),
    }
}
