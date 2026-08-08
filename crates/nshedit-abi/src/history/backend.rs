use core::ffi::{c_int, c_void};
use core::ptr;

use nshedit::domain::Text;
use nshedit::history::HistoryStore;

use crate::adapter::BoundaryChar;
use crate::cdecl::histedit::HistEventGen;

use super::{
    CallbackSet, EnterOperation, GetOperation, HistoryChar, HistoryHandle, NOT_ALLOWED, NOT_FOUND,
    SelectOperation, UNKNOWN, input, own_string, set_error,
};

impl<C: HistoryChar> HistoryHandle<C> {
    // [spec:libedit:def:history.history-set-fun-fn]
    // [spec:libedit:sem:history.history-set-fun-fn]
    pub(super) fn install(&mut self, callbacks: CallbackSet<C>) -> c_int {
        if !callbacks.is_complete() {
            if !self.is_builtin() {
                self.store = HistoryStore::new();
                self.cursor.reset();
                self.limit = 0;
                self.next_event = 0;
                self.callbacks = None;
                self.callback_cookie.marker = 0;
            }
            return -1;
        }
        if self.is_builtin() {
            self.clear();
        }
        self.callbacks = Some(callbacks);
        0
    }

    pub(super) fn get_backend(
        &mut self,
        event: &mut HistEventGen<C>,
        operation: GetOperation,
    ) -> c_int {
        let callback = self.callbacks.and_then(|callbacks| match operation {
            GetOperation::First => callbacks.first,
            GetOperation::Next => callbacks.next,
            GetOperation::Last => callbacks.last,
            GetOperation::Previous => callbacks.previous,
            GetOperation::Current => callbacks.current,
        });
        if self.callbacks.is_some() {
            let Some(callback) = callback else {
                set_error(event, UNKNOWN);
                return -1;
            };
            let cookie = self.cookie();
            // SAFETY: `H_FUNC` installed this C callback with this event
            // signature. The deliberately wrong cookie preserves the
            // reference implementation's frozen `H_FUNC` defect.
            return unsafe { callback(cookie, ptr::from_mut(event)) };
        }
        match operation {
            GetOperation::First => self.first(event),
            GetOperation::Next => self.next(event),
            GetOperation::Last => self.last(event),
            GetOperation::Previous => self.previous(event),
            GetOperation::Current => self.current(event),
        }
    }

    pub(super) fn enter_backend(
        &mut self,
        event: &mut HistEventGen<C>,
        operation: EnterOperation,
        text: *const C,
    ) -> c_int {
        let callback = self.callbacks.and_then(|callbacks| match operation {
            EnterOperation::Enter => callbacks.enter,
            EnterOperation::Add => callbacks.add,
        });
        if self.callbacks.is_some() {
            let Some(callback) = callback else {
                set_error(event, UNKNOWN);
                return -1;
            };
            let cookie = self.cookie();
            // SAFETY: as in `get_backend`; `text` is the caller's borrowed C
            // string for the duration of the callback.
            return unsafe { callback(cookie, ptr::from_mut(event), text) };
        }
        match operation {
            EnterOperation::Enter => self.enter(event, text),
            EnterOperation::Add => self.add(event, text),
        }
    }

    pub(super) fn select_backend(
        &mut self,
        event: &mut HistEventGen<C>,
        operation: SelectOperation,
        number: c_int,
    ) -> c_int {
        let callback = self.callbacks.and_then(|callbacks| match operation {
            SelectOperation::Select => callbacks.select,
            SelectOperation::Delete => callbacks.delete,
        });
        if self.callbacks.is_some() {
            let Some(callback) = callback else {
                set_error(event, UNKNOWN);
                return -1;
            };
            let cookie = self.cookie();
            // SAFETY: as in `get_backend`.
            return unsafe { callback(cookie, ptr::from_mut(event), number) };
        }
        match operation {
            SelectOperation::Select => self.select_event(event, number),
            SelectOperation::Delete => self.delete_event(event, number),
        }
    }

    pub(super) fn clear_backend(&mut self, event: &mut HistEventGen<C>) {
        if let Some(callbacks) = self.callbacks {
            if let Some(callback) = callbacks.clear {
                let cookie = self.cookie();
                // SAFETY: as in `get_backend`.
                unsafe { callback(cookie, ptr::from_mut(event)) };
            }
        } else {
            self.clear();
        }
    }

    // [spec:libedit:def:history.history-prev-event-fn]
    // [spec:libedit:sem:history.history-prev-event-fn]
    pub(super) fn previous_event(&mut self, event: &mut HistEventGen<C>, number: c_int) -> c_int {
        let mut result = self.get_backend(event, GetOperation::Current);
        while result != -1 {
            if event.num == number {
                return 0;
            }
            result = self.get_backend(event, GetOperation::Previous);
        }
        set_error(event, NOT_FOUND);
        -1
    }

    // [spec:libedit:def:history.history-next-event-fn]
    // [spec:libedit:sem:history.history-next-event-fn]
    pub(super) fn next_event(&mut self, event: &mut HistEventGen<C>, number: c_int) -> c_int {
        let mut result = self.get_backend(event, GetOperation::Current);
        while result != -1 {
            if event.num == number {
                return 0;
            }
            result = self.get_backend(event, GetOperation::Next);
        }
        set_error(event, NOT_FOUND);
        -1
    }

    // [spec:libedit:def:history.history-next-evdata-fn]
    // [spec:libedit:sem:history.history-next-evdata-fn]
    pub(super) fn event_data(
        &mut self,
        event: &mut HistEventGen<C>,
        number: c_int,
        output: *mut *mut c_void,
    ) -> c_int {
        let mut result = self.get_backend(event, GetOperation::Current);
        while result != -1 {
            if event.num == number {
                if !output.is_null() {
                    if !self.is_builtin() {
                        set_error(event, NOT_ALLOWED);
                        return -1;
                    }
                    let data = self
                        .cursor
                        .current()
                        .and_then(|id| self.store.get(id))
                        .map_or(ptr::null_mut(), |entry| entry.metadata().data);
                    // SAFETY: `output` is the caller's non-null out-parameter.
                    unsafe { *output = data };
                }
                return 0;
            }
            result = self.get_backend(event, GetOperation::Previous);
        }
        set_error(event, NOT_FOUND);
        -1
    }

    // [spec:libedit:def:history.history-prev-string-fn]
    // [spec:libedit:sem:history.history-prev-string-fn]
    pub(super) fn previous_string(
        &mut self,
        event: &mut HistEventGen<C>,
        pattern: *const C,
    ) -> c_int {
        self.search(event, pattern, GetOperation::Next)
    }

    // [spec:libedit:def:history.history-next-string-fn]
    // [spec:libedit:sem:history.history-next-string-fn]
    pub(super) fn next_string(&mut self, event: &mut HistEventGen<C>, pattern: *const C) -> c_int {
        self.search(event, pattern, GetOperation::Previous)
    }

    fn search(
        &mut self,
        event: &mut HistEventGen<C>,
        pattern: *const C,
        direction: GetOperation,
    ) -> c_int {
        // SAFETY: the public operation promises a NUL-terminated pattern;
        // NULL is defined as the empty prefix.
        let pattern = unsafe { input(pattern) }.to_vec();
        let mut result = self.get_backend(event, GetOperation::Current);
        while result != -1 {
            // SAFETY: a successful backend operation returns a borrowed
            // NUL-terminated event string.
            if unsafe { input(event.str) }.starts_with(&pattern) {
                return 0;
            }
            result = self.get_backend(event, direction);
        }
        set_error(event, NOT_FOUND);
        -1
    }

    pub(super) fn replace(
        &mut self,
        event: &mut HistEventGen<C>,
        line: *const C,
        data: *mut c_void,
    ) -> c_int {
        if !self.is_builtin() {
            set_error(event, NOT_ALLOWED);
            return -1;
        }
        if line.is_null() {
            return -1;
        }
        let Some(id) = self.cursor.current() else {
            return -1;
        };
        // SAFETY: non-null and NUL-terminated by the operation contract.
        let input = unsafe { input(line) }.to_vec();
        let replacement = own_string(&input);
        let Some(entry) = self.store.get_mut(id) else {
            self.cursor.reset();
            return -1;
        };
        *entry.line_mut() = input
            .iter()
            .copied()
            .map(BoundaryChar::into_unit)
            .collect::<Text>();
        let boundary = entry.metadata_mut();
        Vec::leak(core::mem::replace(&mut boundary.c_string, replacement));
        boundary.data = data;
        0
    }
}
