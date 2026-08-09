use core::ffi::c_int;

use crate::cdecl::histedit::{
    H_ADD, H_APPEND, H_CLEAR, H_CURR, H_DEL, H_DELDATA, H_END, H_ENTER, H_FIRST, H_FUNC, H_GETSIZE,
    H_GETUNIQUE, H_LAST, H_LOAD, H_NEXT, H_NEXT_EVDATA, H_NEXT_EVENT, H_NEXT_STR, H_NSAVE_FP,
    H_PREV, H_PREV_EVENT, H_PREV_STR, H_REPLACE, H_SAVE, H_SAVE_FP, H_SET, H_SETSIZE, H_SETUNIQUE,
};

use super::persistence::{load, save, save_stream};
use super::*;

fn missing<C: HistoryChar>(event: &mut HistEventGen<C>) -> c_int {
    set_error(event, PARAMETER_MISSING);
    -1
}

fn report_io<C: HistoryChar>(event: &mut HistEventGen<C>, result: c_int, error: c_int) -> c_int {
    if result == -1 {
        set_error(event, error);
    }
    result
}

fn control<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    operation: c_int,
    argument: DispatchArg<'_, C>,
) -> c_int {
    match operation {
        H_GETSIZE => history.get_size(event),
        H_SETSIZE => match argument {
            DispatchArg::Number(size) => history.set_size(event, size),
            _ => missing(event),
        },
        H_GETUNIQUE => history.get_unique(event),
        H_SETUNIQUE => match argument {
            DispatchArg::Number(unique) => history.set_unique(event, unique),
            _ => missing(event),
        },
        H_CLEAR => {
            history.clear_backend(event);
            0
        }
        H_FUNC => match argument {
            DispatchArg::Callbacks(callbacks) => {
                history.last_entered = -1;
                let result = history.install(callbacks);
                if result == -1 {
                    set_error(event, PARAMETER_MISSING);
                }
                result
            }
            _ => missing(event),
        },
        _ => unreachable!("control dispatch received operation {operation}"),
    }
}

fn edit<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    operation: c_int,
    argument: DispatchArg<'_, C>,
) -> c_int {
    match operation {
        H_ADD => match argument {
            DispatchArg::Text(text) => history.enter_backend(event, EnterOperation::Add, text),
            _ => missing(event),
        },
        H_DEL => match argument {
            DispatchArg::Number(number) => {
                history.select_backend(event, SelectOperation::Delete, number)
            }
            _ => missing(event),
        },
        H_ENTER => match argument {
            DispatchArg::Text(text) => {
                let result = history.enter_backend(event, EnterOperation::Enter, text);
                if result != -1 {
                    history.last_entered = event.num;
                }
                result
            }
            _ => missing(event),
        },
        H_APPEND => match argument {
            DispatchArg::Text(text) => {
                let mut result =
                    history.select_backend(event, SelectOperation::Select, history.last_entered);
                if result != -1 {
                    result = history.enter_backend(event, EnterOperation::Add, text);
                }
                result
            }
            _ => missing(event),
        },
        H_SET => match argument {
            DispatchArg::Number(number) => {
                history.select_backend(event, SelectOperation::Select, number)
            }
            _ => missing(event),
        },
        H_DELDATA => match argument {
            DispatchArg::EventData(number, data) => {
                if !history.is_builtin() {
                    set_error(event, NOT_ALLOWED);
                    -1
                } else {
                    history.delete_nth(event, number, data)
                }
            }
            _ => missing(event),
        },
        H_REPLACE => match argument {
            DispatchArg::Replace(line, data) => history.replace(event, line, data),
            _ => missing(event),
        },
        _ => unreachable!("edit dispatch received operation {operation}"),
    }
}

fn walk<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    operation: c_int,
    argument: DispatchArg<'_, C>,
) -> c_int {
    match operation {
        H_FIRST => history.get_backend(event, GetOperation::First),
        H_NEXT => history.get_backend(event, GetOperation::Next),
        H_LAST => history.get_backend(event, GetOperation::Last),
        H_PREV => history.get_backend(event, GetOperation::Previous),
        H_CURR => history.get_backend(event, GetOperation::Current),
        H_PREV_EVENT => match argument {
            DispatchArg::Number(number) => history.previous_event(event, number),
            _ => missing(event),
        },
        H_NEXT_EVENT => match argument {
            DispatchArg::Number(number) => history.next_event(event, number),
            _ => missing(event),
        },
        H_PREV_STR => match argument {
            DispatchArg::Text(pattern) => history.previous_string(event, pattern),
            _ => missing(event),
        },
        H_NEXT_STR => match argument {
            DispatchArg::Text(pattern) => history.next_string(event, pattern),
            _ => missing(event),
        },
        H_NEXT_EVDATA => match argument {
            DispatchArg::EventData(number, data) => history.event_data(event, number, data),
            _ => missing(event),
        },
        _ => unreachable!("walk dispatch received operation {operation}"),
    }
}

fn file<C: HistoryChar>(
    history: &mut HistoryHandle<C>,
    event: &mut HistEventGen<C>,
    operation: c_int,
    argument: DispatchArg<'_, C>,
) -> c_int {
    match operation {
        H_LOAD => match argument {
            DispatchArg::Path(Some(path)) => {
                let result = load(history, path);
                report_io(event, result, HISTORY_READ_FAILED)
            }
            DispatchArg::Path(None) => report_io(event, -1, HISTORY_READ_FAILED),
            _ => missing(event),
        },
        H_SAVE => match argument {
            DispatchArg::Path(Some(path)) => {
                let result = save(history, path);
                report_io(event, result, HISTORY_WRITE_FAILED)
            }
            DispatchArg::Path(None) => report_io(event, -1, HISTORY_WRITE_FAILED),
            _ => missing(event),
        },
        H_SAVE_FP => match argument {
            DispatchArg::Stream(stream) => {
                let result = save_stream(history, usize::MAX, stream);
                report_io(event, result, HISTORY_WRITE_FAILED)
            }
            _ => missing(event),
        },
        H_NSAVE_FP => match argument {
            DispatchArg::LimitedStream(count, stream) => {
                let result = save_stream(history, count, stream);
                report_io(event, result, HISTORY_WRITE_FAILED)
            }
            _ => missing(event),
        },
        _ => unreachable!("file dispatch received operation {operation}"),
    }
}

// [spec:libedit:def:history.funw-history-fn]
// [spec:libedit:sem:history.funw-history-fn]
pub(crate) unsafe fn dispatch<C: HistoryChar>(
    handle: *mut HistoryHandle<C>,
    event: *mut HistEventGen<C>,
    operation: c_int,
    argument: DispatchArg<'_, C>,
) -> c_int {
    // SAFETY: the public entry point requires a writable event.
    let event = unsafe { &mut *event };
    set_error(event, OK);
    if handle.is_null() {
        set_error(event, UNKNOWN);
        return -1;
    }
    if operation == H_END {
        // SAFETY: this consuming operation receives the live allocation from
        // `new_raw`; the caller must not use it again.
        drop(unsafe { Box::from_raw(handle) });
        return 0;
    }
    // SAFETY: non-consuming operation through a live opaque owner.
    let history = unsafe { &mut *handle };

    match operation {
        H_GETSIZE | H_SETSIZE | H_GETUNIQUE | H_SETUNIQUE | H_CLEAR | H_FUNC => {
            control(history, event, operation, argument)
        }
        H_ADD | H_DEL | H_ENTER | H_APPEND | H_SET | H_DELDATA | H_REPLACE => {
            edit(history, event, operation, argument)
        }
        H_FIRST | H_NEXT | H_LAST | H_PREV | H_CURR | H_PREV_EVENT | H_NEXT_EVENT | H_PREV_STR
        | H_NEXT_STR | H_NEXT_EVDATA => walk(history, event, operation, argument),
        H_LOAD | H_SAVE | H_SAVE_FP | H_NSAVE_FP => file(history, event, operation, argument),
        _ => {
            set_error(event, UNKNOWN);
            -1
        }
    }
}
