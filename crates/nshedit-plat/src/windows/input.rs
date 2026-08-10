//! Structured Windows console input normalized for the native read protocol.

use std::collections::VecDeque;
use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle};
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Console::{
    GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT, KEY_EVENT_RECORD, LEFT_ALT_PRESSED,
    LEFT_CTRL_PRESSED, RIGHT_ALT_PRESSED, RIGHT_CTRL_PRESSED, ReadConsoleInputW, SHIFT_PRESSED,
    WINDOW_BUFFER_SIZE_EVENT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_A, VK_C, VK_CANCEL, VK_DELETE, VK_DOWN, VK_END, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT,
    VK_PRIOR, VK_RIGHT, VK_SPACE, VK_TAB, VK_UP, VK_Z,
};

use super::{HandleKind, handle_kind, wait_for_handle};

const HIGH_SURROGATE_START: u16 = 0xd800;
const HIGH_SURROGATE_END: u16 = 0xdbff;
const LOW_SURROGATE_START: u16 = 0xdc00;
const LOW_SURROGATE_END: u16 = 0xdfff;
const SUPPLEMENTARY_PLANE_START: u32 = 0x1_0000;
const SURROGATE_PAYLOAD_BITS: u32 = 10;

/// One owned result from a real Windows console input buffer.
///
/// Every variant maps directly to the native editor's existing read outcome;
/// no Win32 record, UTF-16 code unit, or virtual-key value crosses this API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleRead {
    /// UTF-8 text or a complete ANSI key sequence.
    Bytes(Box<[u8]>),
    /// An interactive Ctrl-C or Ctrl-Break request.
    Interrupt,
    /// The console window or screen buffer changed size.
    Resize,
    /// The console input source ended.
    EndOfInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Modifiers {
    shift: bool,
    alt: bool,
    control: bool,
}

impl Modifiers {
    fn from_key(key: &KEY_EVENT_RECORD, unicode: u16) -> Self {
        let state = key.dwControlKeyState;
        let left_control = state & LEFT_CTRL_PRESSED != 0;
        let right_alt = state & RIGHT_ALT_PRESSED != 0;
        let alt_gr = left_control && right_alt && unicode != 0;

        Self {
            shift: state & SHIFT_PRESSED != 0,
            alt: state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0 && !alt_gr,
            control: state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0 && !alt_gr,
        }
    }

    fn csi_parameter(self) -> Option<u8> {
        let parameter =
            1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.control);
        (parameter != 1).then_some(parameter)
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingSurrogate {
    high: u16,
    repeat: usize,
    modifiers: Modifiers,
}

/// Exclusive reader for one real Windows console input buffer.
///
/// Use [`super::handle_kind`] to keep stream handles on ordinary
/// [`std::io::Read`]. This reader consumes structured console records only.
// [spec:nshedit:req:platform.windows-input]
pub struct ConsoleReader<'handle> {
    input: BorrowedHandle<'handle>,
    ready: VecDeque<ConsoleRead>,
    surrogate: Option<PendingSurrogate>,
}

impl<'handle> ConsoleReader<'handle> {
    /// Validate and borrow a real console input handle.
    pub fn new(input: BorrowedHandle<'handle>) -> io::Result<Self> {
        if handle_kind(input)? == HandleKind::Stream {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "editor input is a byte stream, not a Windows console",
            ));
        }
        Ok(Self {
            input,
            ready: VecDeque::new(),
            surrogate: None,
        })
    }

    /// Wait for and decode the next meaningful console result.
    pub fn read(&mut self) -> io::Result<ConsoleRead> {
        loop {
            if let Some(read) = self.ready.pop_front() {
                return Ok(read);
            }
            match self.read_record()? {
                Some(record) => self.decode(record),
                None => self.finish(),
            }
        }
    }

    /// Decode an already-buffered result without blocking.
    ///
    /// `None` means no meaningful console record is currently available. It
    /// lets the editor resolve an ambiguous key-sequence prefix as a timeout.
    pub fn try_read(&mut self) -> io::Result<Option<ConsoleRead>> {
        loop {
            if let Some(read) = self.ready.pop_front() {
                return Ok(Some(read));
            }
            if !self.has_records()? {
                return Ok(None);
            }
            match self.read_record()? {
                Some(record) => self.decode(record),
                None => self.finish(),
            }
        }
    }

    /// Wait up to `timeout` for the next meaningful console result.
    ///
    /// Key-up and other ignored records do not restart the timeout. Buffered
    /// decoded results are returned before the operating-system handle is
    /// consulted.
    pub fn read_for(&mut self, timeout: Duration) -> io::Result<Option<ConsoleRead>> {
        let started_at = Instant::now();
        loop {
            if let Some(read) = self.ready.pop_front() {
                return Ok(Some(read));
            }
            if self.has_records()? {
                match self.read_record()? {
                    Some(record) => self.decode(record),
                    None => self.finish(),
                }
                continue;
            }

            let remaining = timeout.saturating_sub(started_at.elapsed());
            if !wait_for_handle(self.input, remaining)? {
                return Ok(None);
            }
        }
    }

    fn has_records(&self) -> io::Result<bool> {
        let mut count = 0;
        // SAFETY: the borrowed handle remains live and `count` is writable for
        // the duration of the call.
        if unsafe { GetNumberOfConsoleInputEvents(self.input.as_raw_handle(), &mut count) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(count != 0)
        }
    }

    fn read_record(&self) -> io::Result<Option<INPUT_RECORD>> {
        let mut record = INPUT_RECORD::default();
        let mut count = 0;
        // SAFETY: the borrowed handle remains live; `record` and `count` are
        // writable for the duration of this one-record read.
        if unsafe { ReadConsoleInputW(self.input.as_raw_handle(), &mut record, 1, &mut count) } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((count != 0).then_some(record))
    }

    fn decode(&mut self, record: INPUT_RECORD) {
        match u32::from(record.EventType) {
            KEY_EVENT => {
                // SAFETY: `EventType` identifies the active union member.
                let key = unsafe { record.Event.KeyEvent };
                self.decode_key(key);
            }
            WINDOW_BUFFER_SIZE_EVENT => {
                self.flush_surrogate();
                self.ready.push_back(ConsoleRead::Resize);
            }
            _ => {}
        }
    }

    fn decode_key(&mut self, key: KEY_EVENT_RECORD) {
        if key.bKeyDown == 0 {
            return;
        }

        // SAFETY: `ReadConsoleInputW` filled the wide-character member of a
        // `KEY_EVENT_RECORD` returned by its W API.
        let unicode = unsafe { key.uChar.UnicodeChar };
        let modifiers = Modifiers::from_key(&key, unicode);
        let repeat = usize::from(key.wRepeatCount.max(1));

        if modifiers.control && (key.wVirtualKeyCode == VK_C || key.wVirtualKeyCode == VK_CANCEL) {
            self.flush_surrogate();
            self.push_repeated(ConsoleRead::Interrupt, repeat);
            return;
        }
        if modifiers.control && key.wVirtualKeyCode == VK_Z {
            self.flush_surrogate();
            self.push_repeated(ConsoleRead::EndOfInput, repeat);
            return;
        }
        if unicode != 0 {
            self.decode_utf16(unicode, repeat, modifiers);
            return;
        }

        self.flush_surrogate();
        if let Some(sequence) = special_sequence(key.wVirtualKeyCode, modifiers) {
            self.push_bytes(&sequence, repeat);
        } else if modifiers.control
            && let Some(character) = control_character(key.wVirtualKeyCode)
        {
            self.push_character(character, modifiers, repeat);
        }
    }

    fn decode_utf16(&mut self, unit: u16, repeat: usize, modifiers: Modifiers) {
        if (HIGH_SURROGATE_START..=HIGH_SURROGATE_END).contains(&unit) {
            self.flush_surrogate();
            self.surrogate = Some(PendingSurrogate {
                high: unit,
                repeat,
                modifiers,
            });
            return;
        }

        if (LOW_SURROGATE_START..=LOW_SURROGATE_END).contains(&unit) {
            let Some(high) = self.surrogate.take() else {
                self.push_character(char::REPLACEMENT_CHARACTER, modifiers, repeat);
                return;
            };
            if high.modifiers != modifiers {
                self.push_character(char::REPLACEMENT_CHARACTER, high.modifiers, high.repeat);
                self.push_character(char::REPLACEMENT_CHARACTER, modifiers, repeat);
                return;
            }

            let paired = high.repeat.min(repeat);
            self.push_character(surrogate_pair(high.high, unit), modifiers, paired);
            self.push_character(
                char::REPLACEMENT_CHARACTER,
                high.modifiers,
                high.repeat - paired,
            );
            self.push_character(char::REPLACEMENT_CHARACTER, modifiers, repeat - paired);
            return;
        }

        self.flush_surrogate();
        self.push_character(
            char::from_u32(u32::from(unit)).expect("a non-surrogate UTF-16 unit is a scalar"),
            modifiers,
            repeat,
        );
    }

    fn flush_surrogate(&mut self) {
        if let Some(pending) = self.surrogate.take() {
            self.push_character(
                char::REPLACEMENT_CHARACTER,
                pending.modifiers,
                pending.repeat,
            );
        }
    }

    fn finish(&mut self) {
        self.flush_surrogate();
        self.ready.push_back(ConsoleRead::EndOfInput);
    }

    fn push_character(&mut self, character: char, modifiers: Modifiers, repeat: usize) {
        if repeat == 0 {
            return;
        }
        let mut encoded = [0; 4];
        let character = character.encode_utf8(&mut encoded).as_bytes();
        let mut sequence = Vec::with_capacity(character.len() + usize::from(modifiers.alt));
        if modifiers.alt {
            sequence.push(b'\x1b');
        }
        sequence.extend_from_slice(character);
        self.push_bytes(&sequence, repeat);
    }

    fn push_bytes(&mut self, sequence: &[u8], repeat: usize) {
        if repeat != 0 {
            self.ready.push_back(ConsoleRead::Bytes(
                sequence.repeat(repeat).into_boxed_slice(),
            ));
        }
    }

    fn push_repeated(&mut self, read: ConsoleRead, repeat: usize) {
        self.ready.extend(std::iter::repeat_n(read, repeat));
    }
}

fn special_sequence(virtual_key: u16, modifiers: Modifiers) -> Option<Vec<u8>> {
    match virtual_key {
        VK_UP => Some(csi_final(b'A', modifiers)),
        VK_DOWN => Some(csi_final(b'B', modifiers)),
        VK_RIGHT => Some(csi_final(b'C', modifiers)),
        VK_LEFT => Some(csi_final(b'D', modifiers)),
        VK_HOME => Some(csi_final(b'H', modifiers)),
        VK_END => Some(csi_final(b'F', modifiers)),
        VK_INSERT => Some(csi_tilde(2, modifiers)),
        VK_DELETE => Some(csi_tilde(3, modifiers)),
        VK_PRIOR => Some(csi_tilde(5, modifiers)),
        VK_NEXT => Some(csi_tilde(6, modifiers)),
        VK_TAB if modifiers.shift => Some(if modifiers.csi_parameter() == Some(2) {
            b"\x1b[Z".to_vec()
        } else {
            csi_final(b'Z', modifiers)
        }),
        _ => None,
    }
}

fn csi_final(final_byte: u8, modifiers: Modifiers) -> Vec<u8> {
    match modifiers.csi_parameter() {
        Some(parameter) => format!("\x1b[1;{parameter}{}", char::from(final_byte)).into_bytes(),
        None => vec![b'\x1b', b'[', final_byte],
    }
}

fn csi_tilde(number: u8, modifiers: Modifiers) -> Vec<u8> {
    match modifiers.csi_parameter() {
        Some(parameter) => format!("\x1b[{number};{parameter}~").into_bytes(),
        None => format!("\x1b[{number}~").into_bytes(),
    }
}

fn control_character(virtual_key: u16) -> Option<char> {
    if (VK_A..=VK_Z).contains(&virtual_key) {
        char::from_u32(u32::from(virtual_key - VK_A + 1))
    } else if virtual_key == VK_SPACE {
        Some('\0')
    } else {
        None
    }
}

fn surrogate_pair(high: u16, low: u16) -> char {
    let high = u32::from(high - HIGH_SURROGATE_START);
    let low = u32::from(low - LOW_SURROGATE_START);
    char::from_u32(SUPPLEMENTARY_PLANE_START + (high << SURROGATE_PAYLOAD_BITS) + low)
        .expect("a UTF-16 surrogate pair is a scalar")
}
