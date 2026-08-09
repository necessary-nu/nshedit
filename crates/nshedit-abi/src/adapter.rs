//! Opaque Rust owners behind the incomplete `histedit.h` handle types.
//!
//! A C caller knows only the pointer spelling of these values. The allocation
//! behind an editor handle owns one native editor and only the C-facing state
//! needed to adapt streams, callbacks, encodings, and borrowed result views.

use core::ffi::{c_char, c_int, c_void};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::io::Read;
use std::mem::ManuallyDrop;
use std::os::fd::{BorrowedFd, FromRawFd};
use std::rc::Rc;

use nshedit::domain::{
    Action, Binding, Buffering, CommandName, EditTarget, EditingMode, EditorConfig, KeySequence,
    KeymapMode, Motion, ScreenSize, SignalPolicy, TerminalMode, Text, TextUnit, WordPolicy,
};
use nshedit::editor::{
    BaudRate, Continuation, Editor, QuoteStyle, ReadDriver, TerminalControl, TerminalProfile,
    Tokenization, Tokenizer as NativeTokenizer,
};
use nshedit_plat::signal::SignalHandlers;
use nshedit_plat::terminal::{
    self, ApplyWhen, ControlCharacter, OutputSpeed, TerminalAttributes, TerminalFlag,
};

use crate::cdecl::histedit::{CFile, HistEventWide, LineInfo, LineInfoWide as LineInfoW};
use crate::conversion::ConversionBuffer;

mod binding;
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
    name: CommandName,
    callback: CommandCallback,
    help: Text,
}

/// ABI-owned capability values addressed by terminfo capnames.
///
/// The C surface accepts two-character termcap names, but those names are
/// translated before reaching this state. CString storage preserves the
/// borrowed pointers returned by `EL_GETTC` without putting C strings in the
/// native editor.
struct TerminalCapabilities {
    name: String,
    bools: HashMap<&'static str, bool>,
    numbers: HashMap<&'static str, c_int>,
    strings: HashMap<&'static str, CString>,
    derived_destructive_tabs: bool,
    derived_meta_extension: bool,
    rows: usize,
    columns: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtyOverride {
    Enable,
    Disable,
}

#[derive(Clone, Default)]
struct TtyFlagOverrides {
    flags: HashMap<TerminalFlag, TtyOverride>,
    characters: HashMap<ControlCharacter, TtyOverride>,
}

struct TerminalState {
    input: c_int,
    output: c_int,
    original: Option<TerminalAttributes>,
    editing: Option<TerminalAttributes>,
    quoted: Option<TerminalAttributes>,
    active_mode: TerminalMode,
    overrides: [TtyFlagOverrides; 3],
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
            active_mode: TerminalMode::Cooked,
            overrides: initial_tty_overrides(),
            restoration_due: false,
        }));
        (Self(Rc::clone(&state)), state)
    }
}

impl TerminalControl for AbiTerminal {
    fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        if !matches!(
            with_borrowed_descriptor(state.output, terminal::is_terminal),
            Some(Ok(true))
        ) {
            return Ok(());
        }
        let Some(Ok(original)) = with_borrowed_descriptor(state.input, terminal::read_attributes)
        else {
            return Ok(());
        };
        let editing = original.for_editing();
        let quoted = editing.for_quoted_input();
        state.original = Some(original);
        state.editing = Some(editing);
        state.quoted = Some(quoted);
        state.active_mode = TerminalMode::Cooked;
        state.restoration_due = true;
        apply_terminal_attributes(state.input, ApplyWhen::AfterOutput, &editing)
    }

    fn set_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        let selected = match mode {
            TerminalMode::Cooked => state.original.as_ref(),
            TerminalMode::Editing => state.editing.as_ref(),
            TerminalMode::Quoted => state.quoted.as_ref(),
        };
        let result = match selected {
            Some(attributes) => {
                apply_terminal_attributes(state.input, ApplyWhen::AfterOutput, attributes)
            }
            None => Ok(()),
        };
        if result.is_ok() {
            state.active_mode = mode;
        }
        result
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        if !state.restoration_due {
            return Ok(());
        }
        state.restoration_due = false;
        match state.original.as_ref() {
            Some(original) => apply_terminal_attributes(
                state.input,
                ApplyWhen::AfterOutputAndDiscardInput,
                original,
            ),
            None => Ok(()),
        }
    }
}

fn initial_tty_overrides() -> [TtyFlagOverrides; 3] {
    let mut modes = std::array::from_fn(|_| TtyFlagOverrides::default());
    fn configure(
        modes: &mut [TtyFlagOverrides; 3],
        mode: usize,
        state: TtyOverride,
        flags: &[TerminalFlag],
    ) {
        modes[mode]
            .flags
            .extend(flags.iter().copied().map(|flag| (flag, state)));
    }

    configure(
        &mut modes,
        0,
        TtyOverride::Enable,
        &[
            TerminalFlag::MapCarriageReturnToNewline,
            TerminalFlag::PostProcessOutput,
            TerminalFlag::MapNewlineToCarriageReturnNewline,
            TerminalFlag::GenerateSignals,
            TerminalFlag::CanonicalInput,
            TerminalFlag::EchoInput,
            TerminalFlag::EchoErase,
            TerminalFlag::EchoControlCharacters,
            TerminalFlag::ExtendedProcessing,
        ],
    );
    configure(
        &mut modes,
        0,
        TtyOverride::Disable,
        &[
            TerminalFlag::MapNewlineToCarriageReturn,
            TerminalFlag::IgnoreCarriageReturn,
            TerminalFlag::NewlinePerformsCarriageReturn,
            TerminalFlag::DisableFlush,
            TerminalFlag::EchoNewline,
            TerminalFlag::ExternalProcessing,
            TerminalFlag::OutputBeingFlushed,
        ],
    );
    configure(
        &mut modes,
        1,
        TtyOverride::Enable,
        &[
            TerminalFlag::MapNewlineToCarriageReturn,
            TerminalFlag::MapCarriageReturnToNewline,
            TerminalFlag::PostProcessOutput,
            TerminalFlag::MapNewlineToCarriageReturnNewline,
            TerminalFlag::GenerateSignals,
        ],
    );
    configure(
        &mut modes,
        1,
        TtyOverride::Disable,
        &[
            TerminalFlag::IgnoreCarriageReturn,
            TerminalFlag::NewlinePerformsCarriageReturn,
            TerminalFlag::DisableFlush,
            TerminalFlag::CanonicalInput,
            TerminalFlag::EchoInput,
            TerminalFlag::EchoKill,
            TerminalFlag::EchoNewline,
            TerminalFlag::ExternalProcessing,
            TerminalFlag::ExtendedProcessing,
            TerminalFlag::OutputBeingFlushed,
        ],
    );
    modes[1].characters.extend(
        [
            ControlCharacter::EndOfLine,
            ControlCharacter::Suspend,
            ControlCharacter::Discard,
            ControlCharacter::MinimumBytes,
            ControlCharacter::Timeout,
        ]
        .map(|character| (character, TtyOverride::Enable)),
    );
    configure(
        &mut modes,
        2,
        TtyOverride::Disable,
        &[
            TerminalFlag::EnableOutputFlowControl,
            TerminalFlag::EnableInputFlowControl,
            TerminalFlag::MapNewlineToCarriageReturn,
            TerminalFlag::MapCarriageReturnToNewline,
            TerminalFlag::GenerateSignals,
            TerminalFlag::ExtendedProcessing,
        ],
    );
    modes
}

pub(crate) fn with_borrowed_descriptor<T>(
    descriptor: c_int,
    operation: impl FnOnce(BorrowedFd<'_>) -> T,
) -> Option<T> {
    if descriptor < 0 {
        return None;
    }
    // SAFETY: the ABI handle borrows its C streams for every operation on the
    // handle. The borrow is confined to this callback and is never stored.
    let descriptor = unsafe { BorrowedFd::borrow_raw(descriptor) };
    Some(operation(descriptor))
}

fn apply_terminal_attributes(
    descriptor: c_int,
    when: ApplyWhen,
    attributes: &TerminalAttributes,
) -> io::Result<()> {
    with_borrowed_descriptor(descriptor, |descriptor| {
        terminal::apply_attributes(descriptor, when, attributes)
    })
    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?
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
    commands: Vec<HostCommand>,
    terminal_bindings: [Option<Binding>; 7],
    pushback: VecDeque<VecDeque<TextUnit>>,
    signal_handlers: Option<SignalHandlers>,
    terminal: Rc<RefCell<TerminalState>>,
    terminal_capabilities: TerminalCapabilities,
    narrow_conversion: ConversionBuffer,
    narrow_line: Box<LineInfo>,
    wide_storage: Vec<u32>,
    wide_line: Box<LineInfoW>,
    terminal_name: CString,
    word_characters: Option<Vec<u32>>,
    client_data: *mut c_void,
    history_depth: usize,
    history_live_line: Text,
    completion_pending_listing: bool,
}

impl EditLineBoundary {
    fn new(
        program: CString,
        streams: Streams,
        terminal: Rc<RefCell<TerminalState>>,
        terminal_name: CString,
        terminal_capabilities: TerminalCapabilities,
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
            commands: Vec::new(),
            terminal_bindings: std::array::from_fn(|_| None),
            pushback: VecDeque::new(),
            signal_handlers: None,
            terminal,
            terminal_capabilities,
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
            history_live_line: Text::default(),
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
        TextUnit::OpaqueCodePoint(value) => value.get(),
    }
}

fn wide_string(input: &[u32]) -> Option<String> {
    input.iter().copied().map(char::from_u32).collect()
}

pub(crate) fn secure_environment(name: &str) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt;

    if nshedit_plat::is_elevated() {
        None
    } else {
        std::env::var_os(name).map(|value| value.as_os_str().as_bytes().to_vec())
    }
}

pub(crate) struct DescriptorInput {
    descriptor: c_int,
}

impl DescriptorInput {
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

impl Read for DescriptorInput {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file()?.read(buffer)
    }
}
