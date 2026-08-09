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
    Action, Binding, Buffering, CommandName, EditTarget, EditingMode, EditorConfig, Error,
    KeySequence, KeymapMode, Motion, ScreenSize, SignalPolicy, TerminalMode, Text, TextUnit,
    WordPolicy,
};
use nshedit::editor::effect::PromptSide;
use nshedit::editor::{
    BaudRate, Continuation, Editor, QuoteStyle, ReadDriver, StartError, TerminalControl,
    TerminalProfile, Tokenization, Tokenizer as EditorTokenizer,
};
use nshedit_plat::signal::SignalHandlers;
use nshedit_plat::terminal::{
    self, ApplyWhen, ControlCharacter, OutputSpeed, TerminalAttributes, TerminalFlag,
};

use crate::cdecl::histedit::{CFile, LineInfo, LineInfoWide as LineInfoW};
use crate::conversion::ConversionBuffer;

mod binding;
mod history;
mod session;
mod terminal_io;
mod tokenizer;

pub(crate) use history::{HistoryCallback, HistoryEncoding, HistoryPolicy, HistorySource};
pub(crate) use tokenizer::{
    BoundaryChar, BoundaryContinuation, TokenizeOutcome, Tokenizer, TokenizerW,
};

pub(crate) type EnvironmentCallback = unsafe extern "C" fn(*const c_char) -> *mut c_char;
pub(crate) type ResizeCallback = unsafe extern "C" fn(*mut EditLine, *mut c_void);
pub(crate) type AliasCallback = unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char;
pub(crate) type CommandCallback = unsafe extern "C" fn(*mut EditLine, u32) -> u8;
pub(crate) type ReadCallback = unsafe extern "C" fn(*mut EditLine, *mut u32) -> c_int;
pub(crate) type WidePromptCallback = unsafe extern "C" fn(*mut EditLine) -> *mut u32;
pub(crate) type NarrowPromptCallback = unsafe extern "C" fn(*mut EditLine) -> *mut c_char;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StreamKind {
    Input,
    Output,
    Diagnostics,
}

impl TryFrom<c_int> for StreamKind {
    type Error = ();

    fn try_from(value: c_int) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Input),
            1 => Ok(Self::Output),
            2 => Ok(Self::Diagnostics),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct StreamEndpoint {
    pub(crate) file: CFile,
    pub(crate) descriptor: c_int,
}

#[derive(Clone, Copy)]
pub(crate) struct SessionStreams {
    pub(crate) input: StreamEndpoint,
    pub(crate) output: StreamEndpoint,
    pub(crate) diagnostics: StreamEndpoint,
}

impl SessionStreams {
    fn endpoint(&self, kind: StreamKind) -> &StreamEndpoint {
        match kind {
            StreamKind::Input => &self.input,
            StreamKind::Output => &self.output,
            StreamKind::Diagnostics => &self.diagnostics,
        }
    }

    fn endpoint_mut(&mut self, kind: StreamKind) -> &mut StreamEndpoint {
        match kind {
            StreamKind::Input => &mut self.input,
            StreamKind::Output => &mut self.output,
            StreamKind::Diagnostics => &mut self.diagnostics,
        }
    }
}

// [spec:nshedit:req:abi.typed-session]
pub(crate) struct SessionInit<'a> {
    pub(crate) program: &'a str,
    pub(crate) streams: SessionStreams,
}

#[cfg(test)]
impl<'a> SessionInit<'a> {
    pub(crate) fn inert(program: &'a str) -> Self {
        let inert = StreamEndpoint {
            file: core::ptr::null_mut(),
            descriptor: -1,
        };
        Self {
            program,
            streams: SessionStreams {
                input: inert,
                output: inert,
                diagnostics: inert,
            },
        }
    }
}

#[derive(Debug)]
pub(crate) enum SessionInitError {
    ProgramName(std::ffi::NulError),
    Terminal(StartError),
    Display(Error),
}

impl std::fmt::Display for SessionInitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProgramName(error) => write!(formatter, "invalid program name: {error}"),
            Self::Terminal(error) => write!(formatter, "terminal initialization failed: {error}"),
            Self::Display(error) => write!(formatter, "invalid initial display: {error:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditingAvailability {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InterruptedRead {
    Report,
    Retry,
}

struct EditingPolicy {
    signals: SignalPolicy,
    availability: EditingAvailability,
    buffering: Buffering,
    interrupted_read: InterruptedRead,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundaryEncoding {
    Narrow,
    Wide,
}

#[derive(Clone, Copy)]
pub(crate) enum PromptCallback {
    Wide(WidePromptCallback),
    Narrow(NarrowPromptCallback),
}

const _: () = {
    assert!(
        core::mem::size_of::<Option<WidePromptCallback>>()
            == core::mem::size_of::<Option<NarrowPromptCallback>>()
    );
    assert!(
        core::mem::align_of::<Option<WidePromptCallback>>()
            == core::mem::align_of::<Option<NarrowPromptCallback>>()
    );
};

impl PromptCallback {
    /// Project the stored callback into the wide C API's function-pointer slot.
    ///
    /// A callback installed through the narrow API remains observable through
    /// the wide getter. In that cross-encoding case only its address is copied;
    /// Rust never constructs or invokes it with the wrong signature.
    pub(crate) unsafe fn write_wide(self, output: *mut c_void) -> bool {
        if output.is_null() {
            return false;
        }
        match self {
            Self::Wide(callback) => unsafe {
                *output.cast::<Option<WidePromptCallback>>() = Some(callback);
            },
            Self::Narrow(callback) => unsafe {
                let callback = Some(callback);
                copy_prompt_address(core::ptr::from_ref(&callback).cast(), output.cast());
            },
        }
        true
    }

    /// Project the stored callback into the narrow C API's function-pointer slot.
    pub(crate) unsafe fn write_narrow(self, output: *mut c_void) -> bool {
        if output.is_null() {
            return false;
        }
        match self {
            Self::Narrow(callback) => unsafe {
                *output.cast::<Option<NarrowPromptCallback>>() = Some(callback);
            },
            Self::Wide(callback) => unsafe {
                let callback = Some(callback);
                copy_prompt_address(core::ptr::from_ref(&callback).cast(), output.cast());
            },
        }
        true
    }
}

unsafe fn copy_prompt_address(source: *const u8, destination: *mut u8) {
    unsafe {
        core::ptr::copy_nonoverlapping(
            source,
            destination,
            core::mem::size_of::<Option<WidePromptCallback>>(),
        );
    }
}

#[derive(Clone, Copy)]
struct PromptSpec {
    callback: PromptCallback,
    escape: u32,
}

struct PromptRegistry {
    left: PromptSpec,
    right: PromptSpec,
}

impl PromptRegistry {
    fn get(&self, side: PromptSide) -> PromptSpec {
        match side {
            PromptSide::Left => self.left,
            PromptSide::Right => self.right,
        }
    }

    fn set(&mut self, side: PromptSide, prompt: PromptSpec) {
        match side {
            PromptSide::Left => self.left = prompt,
            PromptSide::Right => self.right = prompt,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct CallbackRegistration<F> {
    pub(crate) callback: F,
    pub(crate) cookie: *mut c_void,
}

#[derive(Clone, Copy)]
struct HostCallbacks {
    resize: Option<CallbackRegistration<ResizeCallback>>,
    alias: Option<CallbackRegistration<AliasCallback>>,
    read: Option<ReadCallback>,
    environment: Option<EnvironmentCallback>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HistoryRegistrationError {
    CallbackMissing,
}

struct HistoryBridge {
    source: Option<HistorySource>,
    encoding: HistoryEncoding,
    depth: usize,
    live_line: Text,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompletionInvocation {
    Insert,
    List,
}

struct CompletionBridge {
    next: CompletionInvocation,
}

struct LineViews {
    published: BoundaryEncoding,
    narrow_conversion: ConversionBuffer,
    narrow_line: Box<LineInfo>,
    wide_storage: Vec<u32>,
    wide_line: Box<LineInfoW>,
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

struct TerminalBoundary {
    state: Rc<RefCell<TerminalState>>,
    capabilities: TerminalCapabilities,
    name: CString,
    bindings: [Option<Binding>; 7],
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
static DEFAULT_LEFT_NARROW: [c_char; 3] = [b'?' as c_char, b' ' as c_char, 0];
static DEFAULT_RIGHT_NARROW: [c_char; 1] = [0];

unsafe extern "C" fn default_left_prompt(_: *mut EditLine) -> *mut u32 {
    DEFAULT_LEFT_WIDE.as_ptr().cast_mut()
}

unsafe extern "C" fn default_right_prompt(_: *mut EditLine) -> *mut u32 {
    DEFAULT_RIGHT_WIDE.as_ptr().cast_mut()
}

unsafe extern "C" fn default_left_prompt_narrow(_: *mut EditLine) -> *mut c_char {
    DEFAULT_LEFT_NARROW.as_ptr().cast_mut()
}

unsafe extern "C" fn default_right_prompt_narrow(_: *mut EditLine) -> *mut c_char {
    DEFAULT_RIGHT_NARROW.as_ptr().cast_mut()
}

struct EditLineBoundary {
    program: CString,
    streams: SessionStreams,
    policy: EditingPolicy,
    prompts: PromptRegistry,
    callbacks: HostCallbacks,
    commands: Vec<HostCommand>,
    pushback: VecDeque<VecDeque<TextUnit>>,
    signal_handlers: Option<SignalHandlers>,
    terminal: TerminalBoundary,
    lines: LineViews,
    word_characters: Option<Vec<u32>>,
    client_data: *mut c_void,
    history: HistoryBridge,
    completion: CompletionBridge,
}

impl EditLineBoundary {
    fn new(
        program: CString,
        streams: SessionStreams,
        terminal: Rc<RefCell<TerminalState>>,
        terminal_name: CString,
        terminal_capabilities: TerminalCapabilities,
    ) -> Self {
        Self {
            program,
            streams,
            policy: EditingPolicy {
                signals: SignalPolicy::Ignore,
                availability: EditingAvailability::Enabled,
                buffering: Buffering::Line,
                interrupted_read: InterruptedRead::Report,
            },
            prompts: PromptRegistry {
                left: PromptSpec {
                    callback: PromptCallback::Wide(default_left_prompt),
                    escape: 0,
                },
                right: PromptSpec {
                    callback: PromptCallback::Wide(default_right_prompt),
                    escape: 0,
                },
            },
            callbacks: HostCallbacks {
                resize: None,
                alias: None,
                read: None,
                environment: None,
            },
            commands: Vec::new(),
            pushback: VecDeque::new(),
            signal_handlers: None,
            terminal: TerminalBoundary {
                state: terminal,
                capabilities: terminal_capabilities,
                name: terminal_name,
                bindings: std::array::from_fn(|_| None),
            },
            lines: LineViews {
                published: BoundaryEncoding::Wide,
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
            },
            word_characters: Some(vec![b'_' as u32, 0]),
            client_data: core::ptr::null_mut(),
            history: HistoryBridge {
                source: None,
                encoding: HistoryEncoding::Wide,
                depth: 0,
                live_line: Text::default(),
            },
            completion: CompletionBridge {
                next: CompletionInvocation::Insert,
            },
        }
    }
}

// [spec:nshedit:req:abi.opaque-owner]
/// Allocation behind C's incomplete `EditLine` handle.
///
/// The native [`Editor`] is the sole editing and terminal-restoration owner.
/// Every other field exists only to adapt an explicit C ABI obligation.
pub struct EditLine {
    editor: Editor<AbiTerminal>,
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
