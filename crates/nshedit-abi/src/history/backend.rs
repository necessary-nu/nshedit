//! Typed built-in and foreign history backend operations.

use core::ptr;

use nshedit::domain::Text;
use nshedit::history::HistoryStore;

use crate::adapter::BoundaryChar;
use crate::cdecl::histedit::HistEventGen;

use super::*;

#[derive(Clone, Copy)]
enum EntryChange {
    Enter,
    Add,
}

#[derive(Clone, Copy)]
enum Selection {
    Select,
    Delete,
}

fn empty_event<C>() -> HistEventGen<C> {
    HistEventGen {
        num: 0,
        str: ptr::null(),
    }
}

impl<C: HistoryChar> HistoryHandle<C> {
    // [spec:libedit:def:history.history-set-fun-fn]
    // [spec:libedit:sem:history.history-set-fun-fn]
    pub(super) fn install(&mut self, callbacks: CallbackSet<C>) -> HistoryResult<C> {
        if !callbacks.is_complete() {
            if !self.is_builtin() {
                self.store = HistoryStore::new();
                self.cursor.reset();
                self.limit = 0;
                self.next_event = EventNumber(0);
                self.callbacks = None;
                self.callback_cookie.marker = 0;
            }
            return Err(HistoryErrorKind::ParameterMissing.into());
        }
        if self.is_builtin() {
            self.clear();
        }
        self.callbacks = Some(callbacks);
        Ok(HistoryReply::Complete)
    }

    fn foreign_event(event: &HistEventGen<C>) -> HistoryEvent<C> {
        let text = if event.str.is_null() {
            None
        } else {
            // SAFETY: a successful callback lends a terminated event string
            // for the duration of this operation. Own it before returning.
            Some(unsafe { input(event.str) }.to_vec())
        };
        HistoryEvent::detached(EventNumber(event.num), text)
    }

    fn backend_move(&mut self, movement: HistoryMove) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let callback = self.callbacks.and_then(|callbacks| match movement {
            HistoryMove::Newest => callbacks.first,
            HistoryMove::Older => callbacks.next,
            HistoryMove::Oldest => callbacks.last,
            HistoryMove::Newer => callbacks.previous,
            HistoryMove::Current => callbacks.current,
        });
        if self.callbacks.is_some() {
            let Some(callback) = callback else {
                return Err(HistoryErrorKind::Unknown.into());
            };
            let cookie = self.cookie();
            let mut event = empty_event();
            // SAFETY: `H_FUNC` installed this callback with this event
            // signature. The deliberately wrong cookie preserves the frozen
            // reference defect while the private protocol remains typed.
            let status = unsafe { callback(cookie, ptr::from_mut(&mut event)) };
            let event = Self::foreign_event(&event);
            return if status == -1 {
                Err(HistoryError::Foreign(event))
            } else {
                Ok(event)
            };
        }
        match movement {
            HistoryMove::Newest => self.first(),
            HistoryMove::Older => self.next(),
            HistoryMove::Oldest => self.last(),
            HistoryMove::Newer => self.previous(),
            HistoryMove::Current => self.current(),
        }
    }

    fn backend_change(&mut self, change: EntryChange, text: &[C]) -> HistoryResult<C> {
        let callback = self.callbacks.and_then(|callbacks| match change {
            EntryChange::Enter => callbacks.enter,
            EntryChange::Add => callbacks.add,
        });
        if self.callbacks.is_some() {
            let Some(callback) = callback else {
                return Err(HistoryErrorKind::Unknown.into());
            };
            let cookie = self.cookie();
            let mut event = empty_event();
            let terminated = own_string(text);
            // SAFETY: the installed callback has this signature; the owned
            // string remains live and terminated for the whole call.
            let status =
                unsafe { callback(cookie, ptr::from_mut(&mut event), terminated.as_ptr()) };
            let event = Self::foreign_event(&event);
            return if status == -1 {
                Err(HistoryError::Foreign(event))
            } else {
                Ok(HistoryReply::Insertion {
                    state: if status == 0 {
                        Insertion::Unchanged
                    } else {
                        Insertion::Inserted
                    },
                    event: Some(event),
                })
            };
        }
        match change {
            EntryChange::Enter => self.enter(text),
            EntryChange::Add => self.add(text),
        }
    }

    fn backend_select(&mut self, selection: Selection, number: EventNumber) -> HistoryResult<C> {
        let callback = self.callbacks.and_then(|callbacks| match selection {
            Selection::Select => callbacks.select,
            Selection::Delete => callbacks.delete,
        });
        if self.callbacks.is_some() {
            let Some(callback) = callback else {
                return Err(HistoryErrorKind::Unknown.into());
            };
            let cookie = self.cookie();
            let mut event = empty_event();
            // SAFETY: the installed callback has this signature.
            let status = unsafe { callback(cookie, ptr::from_mut(&mut event), number.0) };
            let event = Self::foreign_event(&event);
            return if status == -1 {
                Err(HistoryError::Foreign(event))
            } else {
                Ok(match selection {
                    Selection::Select => HistoryReply::Event(event),
                    Selection::Delete => HistoryReply::Removed {
                        event,
                        data: EntryData::NONE,
                    },
                })
            };
        }
        match selection {
            Selection::Select => {
                self.select_event(number)?;
                Ok(HistoryReply::Complete)
            }
            Selection::Delete => self.delete_event(number),
        }
    }

    fn backend_clear(&mut self) -> HistoryResult<C> {
        if let Some(callbacks) = self.callbacks {
            let Some(callback) = callbacks.clear else {
                return Err(HistoryErrorKind::Unknown.into());
            };
            let cookie = self.cookie();
            let mut event = empty_event();
            // SAFETY: the installed callback has this signature.
            unsafe { callback(cookie, ptr::from_mut(&mut event)) };
        } else {
            self.clear();
        }
        Ok(HistoryReply::Complete)
    }

    // [spec:libedit:def:history.history-prev-event-fn]
    // [spec:libedit:sem:history.history-prev-event-fn]
    fn seek(
        &mut self,
        direction: SeekDirection,
        number: EventNumber,
    ) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let movement = match direction {
            SeekDirection::Older => HistoryMove::Older,
            SeekDirection::Newer => HistoryMove::Newer,
        };
        let mut event = self.backend_move(HistoryMove::Current);
        while let Ok(found) = event {
            if found.number == number {
                return Ok(found);
            }
            event = self.backend_move(movement);
        }
        Err(HistoryErrorKind::NotFound.into())
    }

    // [spec:libedit:def:history.history-next-evdata-fn]
    // [spec:libedit:sem:history.history-next-evdata-fn]
    fn find_data(&mut self, number: EventNumber, access: DataAccess) -> HistoryResult<C> {
        let event = self.seek(SeekDirection::Newer, number)?;
        if access == DataAccess::Read && !self.is_builtin() {
            return Err(HistoryErrorKind::NotAllowed.into());
        }
        let data = if access == DataAccess::Read {
            Some(
                self.cursor
                    .current()
                    .and_then(|id| self.store.get(id))
                    .map_or(EntryData::NONE, |entry| entry.metadata().data),
            )
        } else {
            None
        };
        Ok(HistoryReply::EventData { event, data })
    }

    // [spec:libedit:def:history.history-prev-string-fn]
    // [spec:libedit:sem:history.history-prev-string-fn]
    // [spec:libedit:def:history.history-next-string-fn]
    // [spec:libedit:sem:history.history-next-string-fn]
    fn search(
        &mut self,
        direction: SeekDirection,
        pattern: &[C],
    ) -> Result<HistoryEvent<C>, HistoryError<C>> {
        let movement = match direction {
            SeekDirection::Older => HistoryMove::Older,
            SeekDirection::Newer => HistoryMove::Newer,
        };
        let mut event = self.backend_move(HistoryMove::Current);
        while let Ok(found) = event {
            if found
                .text
                .as_deref()
                .is_some_and(|text| text.starts_with(pattern))
            {
                return Ok(found);
            }
            event = self.backend_move(movement);
        }
        Err(HistoryErrorKind::NotFound.into())
    }

    fn replace(&mut self, line: Option<&[C]>, data: EntryData) -> HistoryResult<C> {
        if !self.is_builtin() {
            return Err(HistoryErrorKind::NotAllowed.into());
        }
        let Some(input) = line else {
            return Err(HistoryError::Silent);
        };
        let Some(id) = self.cursor.current() else {
            return Err(HistoryError::Silent);
        };
        let replacement = own_string(input);
        let Some(entry) = self.store.get_mut(id) else {
            self.cursor.reset();
            return Err(HistoryError::Silent);
        };
        *entry.line_mut() = input
            .iter()
            .copied()
            .map(BoundaryChar::into_unit)
            .collect::<Text>();
        let boundary = entry.metadata_mut();
        Vec::leak(core::mem::replace(&mut boundary.c_string, replacement));
        boundary.data = data;
        Ok(HistoryReply::Complete)
    }

    pub(crate) fn execute(&mut self, request: HistoryRequest<'_, C>) -> HistoryResult<C> {
        match request {
            HistoryRequest::Install(callbacks) => {
                self.last_entered = None;
                self.install(callbacks)
            }
            HistoryRequest::Size => self.get_size(),
            HistoryRequest::SetSize(size) => self.set_size(size),
            HistoryRequest::Unique => self.get_unique(),
            HistoryRequest::SetUnique(unique) => self.set_unique(unique),
            HistoryRequest::Clear => self.backend_clear(),
            HistoryRequest::Enter(text) => {
                let reply = self.backend_change(EntryChange::Enter, text)?;
                if let HistoryReply::Insertion { event, .. } = &reply {
                    self.last_entered =
                        Some(event.as_ref().map_or(EventNumber(0), |event| event.number));
                }
                Ok(reply)
            }
            HistoryRequest::Add(text) => self.backend_change(EntryChange::Add, text),
            HistoryRequest::Append(text) => {
                let last_entered = self.last_entered.unwrap_or(EventNumber(-1));
                self.backend_select(Selection::Select, last_entered)?;
                self.backend_change(EntryChange::Add, text)
            }
            HistoryRequest::Select(number) => self.backend_select(Selection::Select, number),
            HistoryRequest::Delete(number) => self.backend_select(Selection::Delete, number),
            HistoryRequest::DeleteAt {
                position_from_oldest,
                mode,
            } => {
                if !self.is_builtin() {
                    Err(HistoryErrorKind::NotAllowed.into())
                } else {
                    self.delete_nth(position_from_oldest, mode)
                }
            }
            HistoryRequest::Replace { text, data } => self.replace(text, data),
            HistoryRequest::Move(movement) => self.backend_move(movement).map(HistoryReply::Event),
            HistoryRequest::Seek { direction, number } => {
                self.seek(direction, number).map(HistoryReply::Event)
            }
            HistoryRequest::Search { direction, prefix } => {
                self.search(direction, prefix).map(HistoryReply::Event)
            }
            HistoryRequest::FindData { number, access } => self.find_data(number, access),
            HistoryRequest::Load(path) => super::persistence::load(self, path),
            HistoryRequest::Save(path) => super::persistence::save(self, path),
            HistoryRequest::SaveStream(stream) => {
                super::persistence::save_stream(self, usize::MAX, stream)
            }
            HistoryRequest::SaveRecent { count, stream } => {
                super::persistence::save_stream(self, count, stream)
            }
        }
    }
}
