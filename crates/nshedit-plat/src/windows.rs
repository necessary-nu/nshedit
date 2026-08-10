//! Safe Windows console lifecycle and handle inspection.

use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_INVALID_HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_PIPE, FILE_TYPE_REMOTE, GetFileType};
use windows_sys::Win32::System::Console::{
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT,
    ENABLE_PROCESSED_INPUT, ENABLE_PROCESSED_OUTPUT, ENABLE_QUICK_EDIT_MODE,
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, ENABLE_WINDOW_INPUT, ENABLE_WRAP_AT_EOL_OUTPUT,
    GetConsoleMode, GetConsoleScreenBufferInfo, SetConsoleMode,
};
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::WaitForSingleObject;

#[path = "windows/input.rs"]
mod input;

pub use input::{ConsoleRead, ConsoleReader};

/// The terminal behavior available through a borrowed Windows handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// A real Windows console input buffer or screen buffer.
    Console,
    /// A file, pipe, pseudoconsole, SSH, or other byte stream.
    Stream,
}

#[derive(Debug, Clone, Copy)]
struct Modes {
    original: u32,
    editing: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveMode {
    Original,
    Editing,
}

/// Classify a borrowed handle without taking ownership of it.
pub fn handle_kind(handle: BorrowedHandle<'_>) -> io::Result<HandleKind> {
    console_mode(handle).map(|mode| {
        if mode.is_some() {
            HandleKind::Console
        } else {
            HandleKind::Stream
        }
    })
}

/// An activated Windows console's exactly-once restoration state.
///
/// Input and output are classified independently. A console input buffer is
/// switched to character-at-a-time records, and a console screen buffer is
/// switched to VT processing. Stream handles are remembered as streams and
/// never passed to mode-setting calls.
// [spec:nshedit:req:platform.windows-console]
pub struct TerminalController<'handle> {
    input: BorrowedHandle<'handle>,
    output: BorrowedHandle<'handle>,
    input_modes: Option<Modes>,
    output_modes: Option<Modes>,
    mode: ActiveMode,
    activated: bool,
    restoration_due: bool,
}

impl<'handle> TerminalController<'handle> {
    /// Borrow the input and output handles without taking ownership of them.
    #[must_use]
    pub const fn new(input: BorrowedHandle<'handle>, output: BorrowedHandle<'handle>) -> Self {
        Self {
            input,
            output,
            input_modes: None,
            output_modes: None,
            mode: ActiveMode::Original,
            activated: false,
            restoration_due: false,
        }
    }

    /// Capture both console modes once and enter interactive editing mode.
    pub fn activate(&mut self) -> io::Result<()> {
        if self.activated {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Windows terminal controller is already activated",
            ));
        }
        self.activated = true;

        self.input_modes = console_mode(self.input)?.map(|original| Modes {
            original,
            editing: editing_input_mode(original),
        });
        self.output_modes = console_mode(self.output)?.map(|original| Modes {
            original,
            editing: editing_output_mode(original),
        });
        self.restoration_due = self.input_modes.is_some() || self.output_modes.is_some();

        self.transition(ActiveMode::Editing)
    }

    /// Restore normal console processing without consuming the obligation.
    pub fn enter_cooked_mode(&mut self) -> io::Result<()> {
        self.transition(ActiveMode::Original)
    }

    /// Re-enter character-at-a-time input and VT output.
    pub fn enter_editing_mode(&mut self) -> io::Result<()> {
        self.transition(ActiveMode::Editing)
    }

    /// Keep the console in editing mode while the decoder quotes one key.
    pub fn enter_quoted_mode(&mut self) -> io::Result<()> {
        self.transition(ActiveMode::Editing)
    }

    /// Consume the restoration obligation and restore every captured mode.
    pub fn restore(&mut self) -> io::Result<()> {
        if !self.restoration_due {
            return Ok(());
        }
        self.restoration_due = false;

        let input = set_optional(self.input, self.input_modes.map(|modes| modes.original));
        let output = set_optional(self.output, self.output_modes.map(|modes| modes.original));
        self.mode = ActiveMode::Original;
        input.and(output)
    }

    fn transition(&mut self, next: ActiveMode) -> io::Result<()> {
        if !self.activated || self.mode == next {
            return Ok(());
        }

        let previous_input = mode_value(self.input_modes, self.mode);
        let next_input = mode_value(self.input_modes, next);
        let next_output = mode_value(self.output_modes, next);

        set_optional(self.input, next_input)?;
        if let Err(error) = set_optional(self.output, next_output) {
            if let Err(rollback) = set_optional(self.input, previous_input) {
                return Err(io::Error::new(
                    error.kind(),
                    format!(
                        "could not change Windows output mode: {error}; input rollback also failed: {rollback}"
                    ),
                ));
            }
            return Err(error);
        }

        self.mode = next;
        Ok(())
    }
}

/// Read the visible Windows console window's `(rows, columns)` dimensions.
pub fn screen_size(output: BorrowedHandle<'_>) -> io::Result<(usize, usize)> {
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    // SAFETY: `BorrowedHandle` guarantees a live borrowed handle, and `info`
    // is writable for the duration of the call.
    if unsafe { GetConsoleScreenBufferInfo(output.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }

    let rows = i32::from(info.srWindow.Bottom) - i32::from(info.srWindow.Top) + 1;
    let columns = i32::from(info.srWindow.Right) - i32::from(info.srWindow.Left) + 1;
    let rows = usize::try_from(rows)
        .ok()
        .filter(|rows| *rows > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid console height"))?;
    let columns = usize::try_from(columns)
        .ok()
        .filter(|columns| *columns > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid console width"))?;
    Ok((rows, columns))
}

/// Report whether a previously classified stream can be read without waiting.
///
/// Pipes are inspected with the pipe API. A disconnected pipe is readable as
/// end-of-input; files and other non-console streams are treated as immediately
/// readable so their ordinary [`std::io::Read`] implementation remains the
/// authority for data and EOF.
fn stream_is_ready(input: BorrowedHandle<'_>) -> io::Result<bool> {
    // SAFETY: `BorrowedHandle` guarantees a live borrowed handle.
    let file_type = unsafe { GetFileType(input.as_raw_handle()) } & !FILE_TYPE_REMOTE;
    if file_type != FILE_TYPE_PIPE {
        return Ok(true);
    }

    let mut available = 0;
    // SAFETY: `BorrowedHandle` guarantees a live borrowed handle; all optional
    // output pointers except `available` are null as permitted by the API.
    if unsafe {
        PeekNamedPipe(
            input.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        if matches!(
            error.kind(),
            io::ErrorKind::BrokenPipe | io::ErrorKind::NotConnected
        ) {
            Ok(true)
        } else {
            Err(error)
        }
    } else {
        Ok(available != 0)
    }
}

const STREAM_READ_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Wait for a byte-stream handle to become readable for at most `timeout`.
///
/// Console handles have a kernel-signalled readiness state and are handled by
/// [`ConsoleReader`]. Synchronous pipe handles do not expose a separate
/// waitable read event, so this path uses bounded, sleeping peeks without
/// changing the caller's pipe mode or starting an uninterruptible reader.
pub fn wait_for_stream_input(input: BorrowedHandle<'_>, timeout: Duration) -> io::Result<bool> {
    let started_at = Instant::now();
    loop {
        if stream_is_ready(input)? {
            return Ok(true);
        }
        let remaining = timeout.saturating_sub(started_at.elapsed());
        if remaining.is_zero() {
            return Ok(false);
        }
        std::thread::sleep(remaining.min(STREAM_READ_POLL_INTERVAL));
    }
}

pub(super) fn wait_for_handle(input: BorrowedHandle<'_>, timeout: Duration) -> io::Result<bool> {
    let milliseconds = wait_milliseconds(timeout);
    // SAFETY: `BorrowedHandle` proves the handle remains live for the wait.
    match unsafe { WaitForSingleObject(input.as_raw_handle(), milliseconds) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        result => Err(io::Error::other(format!(
            "unexpected input wait result {result:#x}"
        ))),
    }
}

fn wait_milliseconds(timeout: Duration) -> u32 {
    u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1)
}

fn editing_input_mode(original: u32) -> u32 {
    let disabled =
        ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT | ENABLE_QUICK_EDIT_MODE;
    (original | ENABLE_EXTENDED_FLAGS | ENABLE_WINDOW_INPUT) & !disabled
}

fn editing_output_mode(original: u32) -> u32 {
    original
        | ENABLE_PROCESSED_OUTPUT
        | ENABLE_VIRTUAL_TERMINAL_PROCESSING
        | ENABLE_WRAP_AT_EOL_OUTPUT
}

fn mode_value(modes: Option<Modes>, active: ActiveMode) -> Option<u32> {
    modes.map(|modes| match active {
        ActiveMode::Original => modes.original,
        ActiveMode::Editing => modes.editing,
    })
}

fn console_mode(handle: BorrowedHandle<'_>) -> io::Result<Option<u32>> {
    let mut mode = 0;
    // SAFETY: `BorrowedHandle` guarantees a live borrowed handle, and `mode`
    // is writable for the duration of the call.
    if unsafe { GetConsoleMode(handle.as_raw_handle(), &mut mode) } != 0 {
        return Ok(Some(mode));
    }

    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_INVALID_HANDLE.cast_signed()) {
        Ok(None)
    } else {
        Err(error)
    }
}

fn set_optional(handle: BorrowedHandle<'_>, mode: Option<u32>) -> io::Result<()> {
    mode.map_or(Ok(()), |mode| set_console_mode(handle, mode))
}

fn set_console_mode(handle: BorrowedHandle<'_>, mode: u32) -> io::Result<()> {
    // SAFETY: `BorrowedHandle` guarantees a live borrowed handle. The mode is
    // a value captured from or derived for that same console handle.
    if unsafe { SetConsoleMode(handle.as_raw_handle(), mode) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
