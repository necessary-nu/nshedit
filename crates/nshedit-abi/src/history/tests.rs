use core::ffi::{c_char, c_int};
use core::ptr;

use crate::cdecl::histedit::{
    H_CLEAR, H_CURR, H_DEL, H_ENTER, H_GETSIZE, H_NEXT_STR, H_SETSIZE, HistEventGen,
};

use super::{DispatchArg, HistoryOwner, dispatch, end};

fn narrow(text: &[u8]) -> Vec<c_char> {
    text.iter()
        .copied()
        .map(|byte| byte as c_char)
        .chain([0])
        .collect()
}

unsafe fn run(
    history: *mut HistoryOwner,
    operation: c_int,
    argument: DispatchArg<'_, c_char>,
) -> (c_int, HistEventGen<c_char>) {
    let mut event = HistEventGen {
        num: 0,
        str: ptr::null(),
    };
    // SAFETY: the test owns the live handle and supplies the typed tail.
    let result = unsafe { dispatch(history, &mut event, operation, argument) };
    (result, event)
}

// [spec:libedit:sem:history.history-def-enter-fn/test]
// [spec:libedit:sem:history.history-def-insert-fn/test]
#[test]
fn zero_limit_discards_insert() {
    let history = HistoryOwner::new_raw();
    let line = narrow(b"temporary");
    // SAFETY: this test owns the handle and string.
    let (result, event) = unsafe { run(history, H_ENTER, DispatchArg::Text(line.as_ptr())) };
    assert_eq!(result, 1);
    assert_eq!(event.num, 1);
    // SAFETY: same live handle.
    assert_eq!(
        unsafe { run(history, H_GETSIZE, DispatchArg::None) }.1.num,
        0
    );
    // SAFETY: consuming the test allocation.
    unsafe { end(history) };
}

// [spec:libedit:sem:history.history-def-clear-fn/test]
// [spec:libedit:sem:history.history-getsize-fn/test]
#[test]
fn clear_keeps_settings() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, H_SETSIZE, DispatchArg::Number(4)) };
    let line = narrow(b"one");
    // SAFETY: live handle and string.
    assert_eq!(
        unsafe { run(history, H_ENTER, DispatchArg::Text(line.as_ptr())) }
            .1
            .num,
        1
    );
    // SAFETY: live handle.
    unsafe { run(history, H_CLEAR, DispatchArg::None) };
    // SAFETY: live handle and string.
    assert_eq!(
        unsafe { run(history, H_ENTER, DispatchArg::Text(line.as_ptr())) }
            .1
            .num,
        1
    );
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:libedit:sem:history.history-def-delete-fn/test]
#[test]
fn deletion_repairs_the_shared_cursor() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, H_SETSIZE, DispatchArg::Number(4)) };
    for line in [narrow(b"old"), narrow(b"new")] {
        // SAFETY: live handle and string.
        unsafe { run(history, H_ENTER, DispatchArg::Text(line.as_ptr())) };
    }
    // SAFETY: live handle; event 2 is newest.
    unsafe { run(history, H_DEL, DispatchArg::Number(2)) };
    // SAFETY: live handle.
    let (_, event) = unsafe { run(history, H_CURR, DispatchArg::None) };
    assert_eq!(event.num, 1);
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:libedit:sem:history.history-next-string-fn/test]
// [spec:libedit:sem:history.history-prev-string-fn/test]
#[test]
fn prefix_search_includes_the_current_entry() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, H_SETSIZE, DispatchArg::Number(4)) };
    let line = narrow(b"prefix value");
    let prefix = narrow(b"prefix");
    // SAFETY: live handle and strings.
    unsafe { run(history, H_ENTER, DispatchArg::Text(line.as_ptr())) };
    // SAFETY: live handle and string.
    let (result, event) = unsafe { run(history, H_NEXT_STR, DispatchArg::Text(prefix.as_ptr())) };
    assert_eq!(result, 0);
    assert_eq!(event.num, 1);
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}
