//! Opaque Rust owners behind the incomplete `histedit.h` handle types.
//!
//! A C caller knows only the pointer spelling of these values. The allocation
//! behind an editor handle owns one native editor and only the C-facing state
//! needed to adapt streams, callbacks, encodings, and borrowed result views.

use core::ffi::{c_char, c_int, c_void};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;
use std::rc::Rc;

use nshedit::domain::{
    Action, Binding, Buffering, CommandName, Direction, EditTarget, EditingMode, EditorConfig,
    InputMode, KeySequence, KeymapMode, Motion, ScreenSize, SignalPolicy, TerminalMode, Text,
    TextUnit, WordKind,
};
use nshedit::editor::{
    Continuation, Editor, QuoteStyle, ReadDriver, TerminalControl, TerminalProfile, Tokenization,
    Tokenizer as NativeTokenizer,
};
use nshedit_plat::termios::{self, Termios};

use crate::cdecl::histedit::{CFile, HistEventWide, LineInfo, LineInfoWide as LineInfoW};
use crate::conversion::ConversionBuffer;

mod session;
mod terminal_io;
mod tokenizer;

pub(crate) use tokenizer::{
    BoundaryChar, BoundaryContinuation, TokenizeOutcome, Tokenizer, TokenizerW,
};

pub(crate) type EnvironmentCallback = unsafe extern "C" fn(*const c_char) -> *mut c_char;
pub(crate) type ResizeCallback = unsafe extern "C" fn(*mut EditLine, *mut c_void);
pub(crate) type AliasCallback = unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char;
pub(crate) type CommandCallback = unsafe extern "C" fn(*mut EditLine, u32) -> u8;
pub(crate) type ReadCallback = unsafe extern "C" fn(*mut EditLine, *mut u32) -> c_int;
pub(crate) type HistoryCallback =
    unsafe extern "C" fn(*mut c_void, *mut HistEventWide, c_int, ...) -> c_int;
pub(crate) type WidePromptCallback = unsafe extern "C" fn(*mut EditLine) -> *mut u32;
pub(crate) type NarrowPromptCallback = unsafe extern "C" fn(*mut EditLine) -> *mut c_char;

#[derive(Clone, Copy)]
struct Streams {
    files: [CFile; 3],
    descriptors: [c_int; 3],
}

struct Policy {
    handle_signals: bool,
    editing_enabled: bool,
    unbuffered: bool,
    safe_read: bool,
    narrow_history: bool,
    publishing_narrow_line: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum PromptCallback {
    Wide(WidePromptCallback),
    Narrow(NarrowPromptCallback),
}

#[derive(Clone, Copy)]
struct PromptSpec {
    callback: PromptCallback,
    escape: u32,
}

#[derive(Clone, Copy)]
struct HostCallbacks {
    resize: Option<(ResizeCallback, *mut c_void)>,
    alias: Option<(AliasCallback, *mut c_void)>,
    read: Option<ReadCallback>,
    history: Option<(HistoryCallback, *mut c_void)>,
    environment: Option<EnvironmentCallback>,
}

struct HostCommand {
    callback: CommandCallback,
    _help: Text,
}

struct TerminalState {
    input: c_int,
    output: c_int,
    original: Option<Termios>,
    editing: Option<Termios>,
    quoted: Option<Termios>,
    restoration_due: bool,
}

#[derive(Clone)]
pub(crate) struct AbiTerminal(Rc<RefCell<TerminalState>>);

impl AbiTerminal {
    fn new(input: c_int, output: c_int) -> (Self, Rc<RefCell<TerminalState>>) {
        let state = Rc::new(RefCell::new(TerminalState {
            input,
            output,
            original: None,
            editing: None,
            quoted: None,
            restoration_due: false,
        }));
        (Self(Rc::clone(&state)), state)
    }
}

impl TerminalControl for AbiTerminal {
    fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        if !termios::isatty(state.output) {
            return Ok(());
        }
        let Some(original) = termios::tcgetattr(state.input) else {
            return Ok(());
        };
        let editing = editing_termios(original);
        let quoted = quoted_termios(editing);
        state.original = Some(original);
        state.editing = Some(editing);
        state.quoted = Some(quoted);
        state.restoration_due = true;
        apply_termios(state.input, termios::TCSADRAIN, &editing)
    }

    fn set_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
        let state = self.0.borrow();
        let selected = match mode {
            TerminalMode::Cooked => state.original.as_ref(),
            TerminalMode::Editing => state.editing.as_ref(),
            TerminalMode::Quoted => state.quoted.as_ref(),
        };
        match selected {
            Some(attributes) => apply_termios(state.input, termios::TCSADRAIN, attributes),
            None => Ok(()),
        }
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        if !state.restoration_due {
            return Ok(());
        }
        state.restoration_due = false;
        match state.original.as_ref() {
            Some(original) => apply_termios(state.input, termios::TCSAFLUSH, original),
            None => Ok(()),
        }
    }
}

fn apply_termios(descriptor: c_int, action: c_int, attributes: &Termios) -> io::Result<()> {
    if termios::tcsetattr(descriptor, action, attributes) {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn editing_termios(mut attributes: Termios) -> Termios {
    attributes.c_iflag |= termios::INLCR | termios::ICRNL;
    attributes.c_iflag &= !termios::IGNCR;
    attributes.c_oflag |= termios::OPOST | termios::ONLCR;
    attributes.c_oflag &= !termios::ONLRET;
    attributes.c_lflag |= termios::ISIG;
    attributes.c_lflag &= !(termios::NOFLSH
        | termios::ICANON
        | termios::ECHO
        | termios::ECHOK
        | termios::ECHONL
        | termios::EXTPROC
        | termios::IEXTEN
        | termios::FLUSHO);
    attributes.c_cc[termios::VEOF] = termios::VDISABLE;
    attributes.c_cc[termios::VEOL] = termios::VDISABLE;
    attributes.c_cc[termios::VEOL2] = termios::VDISABLE;
    attributes.c_cc[termios::VWERASE] = termios::VDISABLE;
    attributes.c_cc[termios::VREPRINT] = termios::VDISABLE;
    attributes.c_cc[termios::VLNEXT] = termios::VDISABLE;
    attributes.c_cc[termios::VMIN] = 1;
    attributes.c_cc[termios::VTIME] = 0;
    attributes
}

fn quoted_termios(mut attributes: Termios) -> Termios {
    attributes.c_iflag &= !(termios::IXON | termios::IXOFF | termios::INLCR | termios::ICRNL);
    attributes.c_lflag &= !(termios::ISIG | termios::IEXTEN);
    attributes
}

static DEFAULT_LEFT_WIDE: [u32; 3] = [b'?' as u32, b' ' as u32, 0];
static DEFAULT_RIGHT_WIDE: [u32; 1] = [0];

unsafe extern "C" fn default_left_prompt(_: *mut EditLine) -> *mut u32 {
    DEFAULT_LEFT_WIDE.as_ptr().cast_mut()
}

unsafe extern "C" fn default_right_prompt(_: *mut EditLine) -> *mut u32 {
    DEFAULT_RIGHT_WIDE.as_ptr().cast_mut()
}

struct EditLineBoundary {
    program: CString,
    streams: Streams,
    policy: Policy,
    prompts: [PromptSpec; 2],
    callbacks: HostCallbacks,
    commands: HashMap<CommandName, HostCommand>,
    pushback: VecDeque<VecDeque<TextUnit>>,
    terminal: Rc<RefCell<TerminalState>>,
    narrow_conversion: ConversionBuffer,
    narrow_line: Box<LineInfo>,
    wide_storage: Vec<u32>,
    wide_line: Box<LineInfoW>,
    terminal_name: CString,
    word_characters: Option<Vec<u32>>,
    client_data: *mut c_void,
    history_depth: usize,
    completion_pending_listing: bool,
}

impl EditLineBoundary {
    fn new(
        program: CString,
        streams: Streams,
        terminal: Rc<RefCell<TerminalState>>,
        terminal_name: CString,
    ) -> Self {
        Self {
            program,
            streams,
            policy: Policy {
                handle_signals: false,
                editing_enabled: true,
                unbuffered: false,
                safe_read: false,
                narrow_history: false,
                publishing_narrow_line: false,
            },
            prompts: [
                PromptSpec {
                    callback: PromptCallback::Wide(default_left_prompt),
                    escape: 0,
                },
                PromptSpec {
                    callback: PromptCallback::Wide(default_right_prompt),
                    escape: 0,
                },
            ],
            callbacks: HostCallbacks {
                resize: None,
                alias: None,
                read: None,
                history: None,
                environment: None,
            },
            commands: HashMap::new(),
            pushback: VecDeque::new(),
            terminal,
            narrow_conversion: ConversionBuffer::default(),
            narrow_line: Box::new(LineInfo {
                buffer: core::ptr::null(),
                cursor: core::ptr::null(),
                lastchar: core::ptr::null(),
            }),
            wide_storage: Vec::new(),
            wide_line: Box::new(LineInfoW {
                buffer: core::ptr::null(),
                cursor: core::ptr::null(),
                lastchar: core::ptr::null(),
            }),
            terminal_name,
            word_characters: Some(vec![b'_' as u32, 0]),
            client_data: core::ptr::null_mut(),
            history_depth: 0,
            completion_pending_listing: false,
        }
    }
}

// [spec:nshedit:req:abi.opaque-owner]
/// Allocation behind C's incomplete `EditLine` handle.
///
/// The native [`Editor`] is the sole editing and terminal-restoration owner.
/// Every other field exists only to adapt an explicit C ABI obligation.
pub struct EditLine {
    native: Editor<AbiTerminal>,
    driver: ReadDriver,
    boundary: EditLineBoundary,
}

pub(crate) fn unit_to_wide(unit: TextUnit) -> u32 {
    match unit {
        TextUnit::Scalar(character) => u32::from(character),
        TextUnit::RawByte(byte) => u32::from(byte),
        TextUnit::CompatibilityWide(value) => value.get(),
    }
}

fn wide_string(input: &[u32]) -> Option<String> {
    input.iter().copied().map(char::from_u32).collect()
}

fn decode_key_sequence(input: &str) -> Option<Text> {
    let bytes = input.as_bytes();
    let mut output = Text::default();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'^' if index + 1 < bytes.len() => {
                let next = bytes[index + 1];
                let byte = if next == b'?' { 0x7f } else { next & 0x1f };
                output.push(TextUnit::Scalar(char::from(byte)));
                index += 2;
            }
            b'\\' if index + 1 < bytes.len() => {
                let next = bytes[index + 1];
                let byte = match next {
                    b'e' | b'E' => 0x1b,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'b' => 0x08,
                    b'f' => 0x0c,
                    b'v' => 0x0b,
                    b'\\' => b'\\',
                    _ => next,
                };
                output.push(TextUnit::Scalar(char::from(byte)));
                index += 2;
            }
            byte if byte.is_ascii() => {
                output.push(TextUnit::Scalar(char::from(byte)));
                index += 1;
            }
            _ => {
                let remaining = input.get(index..)?;
                let character = remaining.chars().next()?;
                output.push(TextUnit::Scalar(character));
                index += character.len_utf8();
            }
        }
    }
    Some(output)
}

fn named_action(name: &str) -> Option<Action> {
    match name {
        "ed-move-to-beg" => Some(Action::Move(Motion::StartOfBuffer)),
        "ed-move-to-end" => Some(Action::Move(Motion::EndOfBuffer)),
        "ed-delete-next-char" => Some(Action::Delete(EditTarget::Character(Direction::Next))),
        "ed-delete-prev-char" => Some(Action::Delete(EditTarget::Character(Direction::Previous))),
        "em-next-word" => Some(Action::Move(Motion::Word {
            direction: Direction::Next,
            kind: WordKind::Word,
        })),
        "ed-prev-word" => Some(Action::Move(Motion::Word {
            direction: Direction::Previous,
            kind: WordKind::Word,
        })),
        "em-toggle-overwrite" => Some(Action::SetInputMode(InputMode::Replace)),
        "ed-insert" => Some(Action::SetModes {
            input: InputMode::Insert,
            keymap: KeymapMode::Emacs,
        }),
        "em-inc-search-prev" => Some(Action::Refresh(nshedit::domain::Refresh::Redisplay)),
        _ => None,
    }
}

pub(crate) fn secure_environment(name: &str) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;

    if nshedit_plat::is_elevated() {
        None
    } else {
        std::env::var_os(name).map(|value| value.as_os_str().as_bytes().to_vec())
    }
}

pub(crate) struct DescriptorIo {
    descriptor: c_int,
}

impl DescriptorIo {
    pub(crate) const fn new(descriptor: c_int) -> Self {
        Self { descriptor }
    }

    fn file(&self) -> io::Result<ManuallyDrop<File>> {
        if self.descriptor < 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        // SAFETY: the descriptor belongs to the C caller and remains open for
        // the call. `ManuallyDrop` prevents Rust from taking ownership.
        Ok(ManuallyDrop::new(unsafe {
            File::from_raw_fd(self.descriptor)
        }))
    }
}

impl Read for DescriptorIo {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file()?.read(buffer)
    }
}

impl Write for DescriptorIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.file()?.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file()?.flush()
    }
}
