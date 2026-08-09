//! Typed history requests and results used behind the C boundary.

use core::ffi::{c_int, c_void};
use core::ptr::NonNull;
use std::io::Write;
use std::path::Path;

use nshedit::history::HistoryId;

use crate::cdecl::histedit::HistEventGen;

/// A history event number, distinct from a retained position or count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EventNumber(pub(crate) c_int);

/// Application data carried opaquely by the compatibility history store.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EntryData(pub(crate) Option<NonNull<c_void>>);

impl EntryData {
    pub(crate) const NONE: Self = Self(None);

    pub(crate) fn as_raw(self) -> *mut c_void {
        self.0.map_or(core::ptr::null_mut(), NonNull::as_ptr)
    }
}

/// A typed event returned by either the built-in or a foreign backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoryEvent<C> {
    pub(crate) number: EventNumber,
    pub(crate) text: Option<Vec<C>>,
    pub(super) retained: Option<HistoryId>,
}

impl<C> HistoryEvent<C> {
    pub(crate) fn detached(number: EventNumber, text: Option<Vec<C>>) -> Self {
        Self {
            number,
            text,
            retained: None,
        }
    }

    pub(super) fn retained(number: EventNumber, text: Vec<C>, id: HistoryId) -> Self {
        Self {
            number,
            text: Some(text),
            retained: Some(id),
        }
    }
}

/// Movement names are stated in user-facing chronology, not libedit opcode
/// direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryMove {
    Newest,
    Older,
    Oldest,
    Newer,
    Current,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SeekDirection {
    Older,
    Newer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DeleteMode {
    SelectOnly,
    Remove,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DataAccess {
    Locate,
    Read,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Insertion {
    Inserted,
    Unchanged,
}

/// The foreign callback table installed by the exported `H_FUNC` operation.
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
    pub(crate) reference: Option<NonNull<c_void>>,
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
    pub(crate) fn is_complete(&self) -> bool {
        self.reference.is_some()
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

/// A caller-owned stream borrowed for one typed save request.
pub(crate) struct SaveStream<'a> {
    pub(crate) at_start: bool,
    pub(crate) output: &'a mut dyn Write,
}

// [spec:nshedit:req:abi.typed-history]
/// Every valid payload is coupled to the operation that consumes it.
pub(crate) enum HistoryRequest<'a, C> {
    Install(CallbackSet<C>),
    Size,
    SetSize(usize),
    Unique,
    SetUnique(bool),
    Clear,
    Enter(&'a [C]),
    Add(&'a [C]),
    Append(&'a [C]),
    Select(EventNumber),
    Delete(EventNumber),
    DeleteAt {
        position_from_oldest: usize,
        mode: DeleteMode,
    },
    Replace {
        text: Option<&'a [C]>,
        data: EntryData,
    },
    Move(HistoryMove),
    Seek {
        direction: SeekDirection,
        number: EventNumber,
    },
    Search {
        direction: SeekDirection,
        prefix: &'a [C],
    },
    FindData {
        number: EventNumber,
        access: DataAccess,
    },
    Load(Option<&'a Path>),
    Save(Option<&'a Path>),
    SaveStream(SaveStream<'a>),
    SaveRecent {
        count: usize,
        stream: SaveStream<'a>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HistoryReply<C> {
    Complete,
    Event(HistoryEvent<C>),
    Insertion {
        state: Insertion,
        event: Option<HistoryEvent<C>>,
    },
    Removed {
        event: HistoryEvent<C>,
        data: EntryData,
    },
    Size(usize),
    Unique(bool),
    Count(usize),
    EventData {
        event: HistoryEvent<C>,
        data: Option<EntryData>,
    },
}

/// Semantic history failures. Their numeric/message representation is applied
/// only while returning through the exported ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryErrorKind {
    Unknown,
    AllocationFailed,
    FirstNotFound,
    LastNotFound,
    Empty,
    EndReached,
    StartReached,
    CurrentInvalid,
    NotFound,
    ReadFailed,
    WriteFailed,
    ParameterMissing,
    NotAllowed,
    BadParameter,
}

impl HistoryErrorKind {
    pub(crate) const fn code(self) -> c_int {
        match self {
            Self::Unknown => 1,
            Self::AllocationFailed => 2,
            Self::FirstNotFound => 3,
            Self::LastNotFound => 4,
            Self::Empty => 5,
            Self::EndReached => 6,
            Self::StartReached => 7,
            Self::CurrentInvalid => 8,
            Self::NotFound => 9,
            Self::ReadFailed => 10,
            Self::WriteFailed => 11,
            Self::ParameterMissing => 12,
            Self::NotAllowed => 14,
            Self::BadParameter => 15,
        }
    }
}

/// A callback may supply its own event while reporting failure. Keeping that
/// event typed lets the boundary preserve it without making it the internal
/// error protocol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HistoryError<C> {
    Known(HistoryErrorKind),
    Foreign(HistoryEvent<C>),
    Silent,
}

pub(crate) type HistoryResult<C> = Result<HistoryReply<C>, HistoryError<C>>;

impl<C> From<HistoryErrorKind> for HistoryError<C> {
    fn from(value: HistoryErrorKind) -> Self {
        Self::Known(value)
    }
}
