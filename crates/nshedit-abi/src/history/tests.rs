use core::ffi::{c_char, c_int, c_void};

use crate::cdecl::histedit::HistEventGen;

use super::*;

fn narrow(text: &[u8]) -> Vec<c_char> {
    text.iter().copied().map(|byte| byte as c_char).collect()
}

unsafe fn run(
    history: *mut HistoryOwner,
    request: HistoryRequest<'_, c_char>,
) -> HistoryResult<c_char> {
    // SAFETY: each test owns this live allocation until `end`.
    unsafe { &mut *history }.execute(request)
}

fn event(reply: HistoryReply<c_char>) -> HistoryEvent<c_char> {
    match reply {
        HistoryReply::Event(event)
        | HistoryReply::Insertion {
            event: Some(event), ..
        } => event,
        other => panic!("expected a history event, got {other:?}"),
    }
}

static CALLBACK_LINE: [c_char; 8] = [
    b'f' as c_char,
    b'o' as c_char,
    b'r' as c_char,
    b'e' as c_char,
    b'i' as c_char,
    b'g' as c_char,
    b'n' as c_char,
    0,
];

unsafe extern "C" fn callback_get(_: *mut c_void, event: *mut HistEventGen<c_char>) -> c_int {
    // SAFETY: the history callback contract supplies a writable event.
    unsafe {
        (*event).num = 42;
        (*event).str = CALLBACK_LINE.as_ptr();
    }
    0
}

unsafe extern "C" fn callback_fail(_: *mut c_void, event: *mut HistEventGen<c_char>) -> c_int {
    // SAFETY: the history callback contract supplies a writable event.
    unsafe {
        (*event).num = 73;
        (*event).str = CALLBACK_LINE.as_ptr();
    }
    -1
}

unsafe extern "C" fn callback_enter(
    cookie: *mut c_void,
    event: *mut HistEventGen<c_char>,
    _: *const c_char,
) -> c_int {
    // SAFETY: same event contract as `callback_get`.
    unsafe { callback_get(cookie, event) }
}

unsafe extern "C" fn callback_clear(_: *mut c_void, _: *mut HistEventGen<c_char>) {}

unsafe extern "C" fn callback_select(
    cookie: *mut c_void,
    event: *mut HistEventGen<c_char>,
    _: c_int,
) -> c_int {
    // SAFETY: same event contract as `callback_get`.
    unsafe { callback_get(cookie, event) }
}

// [spec:nshedit:req:abi.typed-history/test]
// [spec:libedit:sem:history.history-def-enter-fn/test]
// [spec:libedit:sem:history.history-def-insert-fn/test]
#[test]
fn typed_requests_return_typed_replies() {
    let history = HistoryOwner::new_raw();
    let line = narrow(b"temporary");
    // SAFETY: this test owns the handle and text for the call.
    let inserted = unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap();
    assert_eq!(event(inserted).number, EventNumber(1));
    // SAFETY: same live handle.
    assert_eq!(
        unsafe { run(history, HistoryRequest::Size) }.unwrap(),
        HistoryReply::Size(0)
    );
    // SAFETY: consuming the test allocation.
    unsafe { end(history) };
}

// [spec:nshedit:req:abi.typed-history/test]
#[test]
fn foreign_backend_uses_typed_requests() {
    let history = HistoryOwner::new_raw();
    let callbacks = CallbackSet {
        reference: core::ptr::NonNull::new(core::ptr::without_provenance_mut(1)),
        first: Some(callback_get),
        next: Some(callback_get),
        last: Some(callback_get),
        previous: Some(callback_get),
        current: Some(callback_get),
        select: Some(callback_select),
        clear: Some(callback_clear),
        enter: Some(callback_enter),
        add: Some(callback_enter),
        delete: Some(callback_select),
    };
    // SAFETY: this test owns the live handle and callback table.
    unsafe { run(history, HistoryRequest::Install(callbacks)) }.unwrap();
    // SAFETY: same live handle; the callback result is owned by the reply.
    let reply = unsafe { run(history, HistoryRequest::Move(HistoryMove::Newest)) }.unwrap();
    let moved = event(reply);
    assert_eq!(moved.number, EventNumber(42));
    assert_eq!(moved.text.as_deref(), Some(&CALLBACK_LINE[..7]));
    // SAFETY: same live handle; a foreign selector's event is part of its
    // successful reply even though the built-in selector only moves a cursor.
    let selected = unsafe { run(history, HistoryRequest::Select(EventNumber(7))) }.unwrap();
    let selected = event(selected);
    assert_eq!(selected.number, EventNumber(42));
    assert_eq!(selected.text.as_deref(), Some(&CALLBACK_LINE[..7]));

    // Built-in-only requests remain semantic errors behind a foreign source.
    // SAFETY: same live handle.
    assert_eq!(
        unsafe { run(history, HistoryRequest::Size) },
        Err(HistoryError::Known(HistoryErrorKind::NotAllowed))
    );

    let failing = CallbackSet {
        first: Some(callback_fail),
        ..callbacks
    };
    // SAFETY: same live handle and complete callback table.
    unsafe { run(history, HistoryRequest::Install(failing)) }.unwrap();
    // SAFETY: the callback's borrowed failure payload is owned by the result.
    let failure = unsafe { run(history, HistoryRequest::Move(HistoryMove::Newest)) };
    let Err(HistoryError::Foreign(failure)) = failure else {
        panic!("expected the foreign callback's typed failure event");
    };
    assert_eq!(failure.number, EventNumber(73));
    assert_eq!(failure.text.as_deref(), Some(&CALLBACK_LINE[..7]));
    // SAFETY: consuming the test allocation.
    unsafe { end(history) };
}

// [spec:libedit:sem:history.history-def-clear-fn/test]
// [spec:libedit:sem:history.history-getsize-fn/test]
#[test]
fn clear_keeps_settings() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    let line = narrow(b"one");
    // SAFETY: live handle and borrowed text.
    assert_eq!(
        event(unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap()).number,
        EventNumber(1)
    );
    // SAFETY: live handle.
    unsafe { run(history, HistoryRequest::Clear) }.unwrap();
    // SAFETY: live handle and borrowed text.
    assert_eq!(
        event(unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap()).number,
        EventNumber(1)
    );
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:libedit:sem:history.history-def-delete-fn/test]
#[test]
fn deletion_repairs_the_shared_cursor() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    for line in [narrow(b"old"), narrow(b"new")] {
        // SAFETY: live handle and borrowed text.
        unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap();
    }
    // SAFETY: live handle; event 2 is newest.
    unsafe { run(history, HistoryRequest::Delete(EventNumber(2))) }.unwrap();
    // SAFETY: live handle.
    let current = unsafe { run(history, HistoryRequest::Move(HistoryMove::Current)) }.unwrap();
    assert_eq!(event(current).number, EventNumber(1));
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:libedit:sem:history.history-next-string-fn/test]
// [spec:libedit:sem:history.history-prev-string-fn/test]
#[test]
fn prefix_search_includes_the_current_entry() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    let line = narrow(b"prefix value");
    let prefix = narrow(b"prefix");
    // SAFETY: live handle and borrowed text.
    unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap();
    // SAFETY: live handle and borrowed prefix.
    let found = unsafe {
        run(
            history,
            HistoryRequest::Search {
                direction: SeekDirection::Newer,
                prefix: &prefix,
            },
        )
    }
    .unwrap();
    assert_eq!(event(found).number, EventNumber(1));
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:nshedit:req:abi.typed-history/test]
#[test]
fn navigation_uses_chronology() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    for line in [narrow(b"old"), narrow(b"middle"), narrow(b"new")] {
        // SAFETY: the handle and borrowed line are live for each call.
        unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap();
    }

    // SAFETY: every request uses the same live handle.
    let newest = unsafe { run(history, HistoryRequest::Move(HistoryMove::Newest)) }.unwrap();
    assert_eq!(event(newest).number, EventNumber(3));
    // SAFETY: same live handle and cursor.
    let older = unsafe { run(history, HistoryRequest::Move(HistoryMove::Older)) }.unwrap();
    assert_eq!(event(older).number, EventNumber(2));
    // SAFETY: same live handle and cursor.
    let oldest = unsafe { run(history, HistoryRequest::Move(HistoryMove::Oldest)) }.unwrap();
    assert_eq!(event(oldest).number, EventNumber(1));
    // SAFETY: same live handle and cursor.
    let newer = unsafe { run(history, HistoryRequest::Move(HistoryMove::Newer)) }.unwrap();
    assert_eq!(event(newer).number, EventNumber(2));

    // SAFETY: a typed seek carries both its direction and target number.
    let found = unsafe {
        run(
            history,
            HistoryRequest::Seek {
                direction: SeekDirection::Newer,
                number: EventNumber(3),
            },
        )
    }
    .unwrap();
    assert_eq!(event(found).text.as_deref(), Some(&narrow(b"new")[..]));
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:nshedit:req:abi.typed-history/test]
#[test]
fn duplicate_policy_has_typed_results() {
    let history = HistoryOwner::new_raw();
    let repeated = narrow(b"same");
    // SAFETY: the test owns the live handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    // SAFETY: same handle; this changes a native store policy.
    unsafe { run(history, HistoryRequest::SetUnique(true)) }.unwrap();
    // SAFETY: live handle and borrowed text.
    let first = unsafe { run(history, HistoryRequest::Enter(&repeated)) }.unwrap();
    assert!(matches!(
        first,
        HistoryReply::Insertion {
            state: Insertion::Inserted,
            event: Some(_)
        }
    ));
    // SAFETY: live handle and borrowed text.
    let duplicate = unsafe { run(history, HistoryRequest::Enter(&repeated)) }.unwrap();
    assert_eq!(
        duplicate,
        HistoryReply::Insertion {
            state: Insertion::Unchanged,
            event: None,
        }
    );
    // SAFETY: same live handle.
    assert_eq!(
        unsafe { run(history, HistoryRequest::Unique) }.unwrap(),
        HistoryReply::Unique(true)
    );
    assert_eq!(
        unsafe { run(history, HistoryRequest::Size) }.unwrap(),
        HistoryReply::Size(1)
    );
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:nshedit:req:abi.typed-history/test]
#[test]
fn metadata_remains_opaque() {
    let history = HistoryOwner::new_raw();
    let original = narrow(b"original");
    let replacement = narrow(b"replacement");
    let mut marker = 17_u8;
    let data = EntryData(core::ptr::NonNull::new(
        core::ptr::from_mut(&mut marker).cast(),
    ));
    // SAFETY: the test owns the handle and both borrowed strings.
    unsafe { run(history, HistoryRequest::SetSize(2)) }.unwrap();
    // SAFETY: live handle and borrowed text.
    unsafe { run(history, HistoryRequest::Enter(&original)) }.unwrap();
    // SAFETY: the marker and replacement outlive all operations below.
    unsafe {
        run(
            history,
            HistoryRequest::Replace {
                text: Some(&replacement),
                data,
            },
        )
    }
    .unwrap();

    // Locating an event never exposes its application data.
    // SAFETY: same live handle.
    let located = unsafe {
        run(
            history,
            HistoryRequest::FindData {
                number: EventNumber(1),
                access: DataAccess::Locate,
            },
        )
    }
    .unwrap();
    let HistoryReply::EventData { event, data: None } = located else {
        panic!("locating history must not expose application data");
    };
    assert_eq!(event.text.as_deref(), Some(&replacement[..]));

    // SAFETY: same live handle; typed access returns the opaque value.
    let read = unsafe {
        run(
            history,
            HistoryRequest::FindData {
                number: EventNumber(1),
                access: DataAccess::Read,
            },
        )
    }
    .unwrap();
    let HistoryReply::EventData {
        data: Some(found), ..
    } = read
    else {
        panic!("reading history data must return its opaque value");
    };
    assert_eq!(found, data);

    // SAFETY: same live handle; deletion transfers text and metadata together.
    let removed = unsafe { run(history, HistoryRequest::Delete(EventNumber(1))) }.unwrap();
    let HistoryReply::Removed { event, data: found } = removed else {
        panic!("deletion must return the removed entry");
    };
    assert_eq!(event.text.as_deref(), Some(&replacement[..]));
    assert_eq!(found, data);
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:nshedit:req:abi.typed-history/test]
#[test]
fn positional_delete_names_its_mode() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    for line in [narrow(b"one"), narrow(b"two"), narrow(b"three")] {
        // SAFETY: live handle and borrowed text.
        unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap();
    }

    // SAFETY: select-only moves the cursor without removing ownership.
    let selected = unsafe {
        run(
            history,
            HistoryRequest::DeleteAt {
                position_from_oldest: 1,
                mode: DeleteMode::SelectOnly,
            },
        )
    }
    .unwrap();
    assert_eq!(selected, HistoryReply::Complete);
    // SAFETY: same live handle and selected cursor.
    let current = unsafe { run(history, HistoryRequest::Move(HistoryMove::Current)) }.unwrap();
    assert_eq!(event(current).number, EventNumber(2));

    // SAFETY: remove transfers the selected entry out of the store.
    let removed = unsafe {
        run(
            history,
            HistoryRequest::DeleteAt {
                position_from_oldest: 1,
                mode: DeleteMode::Remove,
            },
        )
    }
    .unwrap();
    let HistoryReply::Removed { event, data } = removed else {
        panic!("remove mode must return an owned entry");
    };
    assert_eq!(event.number, EventNumber(2));
    assert_eq!(event.text.as_deref(), Some(&narrow(b"two")[..]));
    assert_eq!(data, EntryData::NONE);
    // SAFETY: same live handle.
    assert_eq!(
        unsafe { run(history, HistoryRequest::Size) }.unwrap(),
        HistoryReply::Size(2)
    );
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}

// [spec:nshedit:req:abi.typed-history/test]
#[test]
fn stream_save_returns_count() {
    let history = HistoryOwner::new_raw();
    // SAFETY: the test owns the handle.
    unsafe { run(history, HistoryRequest::SetSize(4)) }.unwrap();
    for line in [narrow(b"old"), narrow(b"new")] {
        // SAFETY: live handle and borrowed text.
        unsafe { run(history, HistoryRequest::Enter(&line)) }.unwrap();
    }

    let mut output = Vec::new();
    // SAFETY: the output borrow lasts for this request only.
    let saved = unsafe {
        run(
            history,
            HistoryRequest::SaveStream(SaveStream {
                at_start: true,
                output: &mut output,
            }),
        )
    }
    .unwrap();
    assert_eq!(saved, HistoryReply::Count(2));
    let mut expected = nshedit::history_file::LIBEDIT_V2_HEADER.to_vec();
    expected.extend_from_slice(b"old\nnew\n");
    assert_eq!(output, expected);

    let mut appended = b"prefix:".to_vec();
    // SAFETY: a non-starting stream receives entries without another header.
    let saved = unsafe {
        run(
            history,
            HistoryRequest::SaveStream(SaveStream {
                at_start: false,
                output: &mut appended,
            }),
        )
    }
    .unwrap();
    assert_eq!(saved, HistoryReply::Count(2));
    assert_eq!(appended, b"prefix:old\nnew\n");
    // SAFETY: consuming the allocation.
    unsafe { end(history) };
}
