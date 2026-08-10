#![cfg(windows)]

use std::error::Error;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use nshedit::domain::{
    Action, Binding, Direction, EditTarget, EditorConfig, EffectCommand, KeySequence, KeymapMode,
    Motion, Prompt, ScreenSize, Signal, Text,
};
use nshedit::editor::effect::{HistoryResponse, HostFailure, PromptSide, ResizeEffect};
use nshedit::editor::{
    CompletionCandidate, Editor, IoDescriptors, ReadDriver, ReadInterrupt, ReadResult, ReadStep,
    SessionIo, SystemInput, SystemTerminal, TerminalControl, TerminalProfile,
};
use nshedit::history::{HistoryCursor, HistoryStore, Navigation};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    AllocConsole, COORD, ClosePseudoConsole, CreatePseudoConsole, ENABLE_ECHO_INPUT,
    ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    ENABLE_WINDOW_INPUT, FlushConsoleInputBuffer, FreeConsole, GetConsoleMode, HPCON, INPUT_RECORD,
    INPUT_RECORD_0, KEY_EVENT, KEY_EVENT_RECORD, KEY_EVENT_RECORD_0, LEFT_CTRL_PRESSED,
    ResizePseudoConsole, WINDOW_BUFFER_SIZE_EVENT, WINDOW_BUFFER_SIZE_RECORD, WriteConsoleInputW,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
    GetExitCodeProcess, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_BACK, VK_C, VK_DELETE, VK_END, VK_HOME, VK_LEFT, VK_RETURN, VK_TAB, VK_UP, VK_Z,
};

const COMMANDS: [&str; 3] = ["exit", "help", "history"];
const CHILD_TIMEOUT_MS: u32 = 30_000;

type AcceptanceResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

// [spec:nshedit:req:workspace.windows-acceptance]
// [spec:nshedit:req:workspace.windows-acceptance/test]
#[test]
fn native_windows_console_and_streams() -> AcceptanceResult {
    let console = AllocatedConsole::new()?;
    let input_handle = console.input()?;
    let output_handle = console.output()?;
    let mut input = input_handle.try_clone()?;
    let mut output = output_handle.try_clone()?;
    let mut diagnostics = Vec::new();

    flush_console_input(&input_handle)?;
    let original_input_mode = console_mode(&input_handle)?;
    let original_output_mode = console_mode(&output_handle)?;

    let terminal = SystemTerminal::new(input_handle.as_handle(), output_handle.as_handle());
    let input_source = SystemInput::new(input_handle.as_handle())?;
    let mut editor = Editor::new(EditorConfig::default(), terminal)?;
    let active_input_mode = console_mode(&input_handle)?;
    let active_output_mode = console_mode(&output_handle)?;
    assert_eq!(
        active_input_mode & (ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT),
        0
    );
    assert_ne!(active_input_mode & ENABLE_WINDOW_INPUT, 0);
    assert_ne!(active_output_mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING, 0);
    editor.configure_display(
        TerminalProfile::ansi(),
        SystemTerminal::screen_size(output_handle.as_handle())?,
    );
    install_terminal_bindings(&mut editor)?;

    write_console_records(&input_handle, &editing_records())?;

    let mut host = Host {
        history: HistoryStore::new(),
        history_cursor: HistoryCursor::new(),
        input_source,
        io: SessionIo {
            input: &mut input,
            output: &mut output,
            diagnostics: &mut diagnostics,
            descriptors: IoDescriptors {
                input: Some(input_handle.as_handle()),
                output: Some(output_handle.as_handle()),
                diagnostics: None,
            },
        },
        signal_resize_count: 0,
        history_count: 0,
        completion_count: 0,
    };
    let mut driver = ReadDriver::default();

    assert_eq!(
        read_and_reset(&mut editor, &mut driver, &mut host)?,
        Text::from("Xab😀")
    );
    assert_eq!(
        read_and_reset(&mut editor, &mut driver, &mut host)?,
        Text::from("help ")
    );
    assert_eq!(
        read_and_reset(&mut editor, &mut driver, &mut host)?,
        Text::from("help ")
    );
    assert_eq!(
        drive_line(&mut editor, &mut driver, &mut host)?,
        ReadResult::Interrupted(ReadInterrupt::Signal(Signal::Interrupt))
    );
    assert_eq!(
        drive_line(&mut editor, &mut driver, &mut host)?,
        ReadResult::EndOfInput
    );
    assert!(host.signal_resize_count >= 1);
    assert!(host.history_count >= 1);
    assert!(host.completion_count >= 1);

    drop(host);
    editor.finish()?;
    assert_eq!(console_mode(&input_handle)?, original_input_mode);
    assert_eq!(console_mode(&output_handle)?, original_output_mode);

    ordinary_error_restores(
        &input_handle,
        &output_handle,
        original_input_mode,
        original_output_mode,
    )?;
    unwinding_restores(
        &input_handle,
        &output_handle,
        original_input_mode,
        original_output_mode,
    );

    drop(console);
    conpty_editor_session()?;
    conpty_end_of_input()?;
    redirected_stream_session()?;
    Ok(())
}

struct AllocatedConsole;

impl AllocatedConsole {
    fn new() -> io::Result<Self> {
        // A test runner may already have a console. Its standard streams are
        // pipes, so detaching it does not disturb the captured test report.
        unsafe {
            FreeConsole();
        }
        // SAFETY: this process owns no console after the best-effort detach.
        if unsafe { AllocConsole() } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self)
        }
    }

    fn input(&self) -> io::Result<File> {
        OpenOptions::new().read(true).write(true).open("CONIN$")
    }

    fn output(&self) -> io::Result<File> {
        OpenOptions::new().read(true).write(true).open("CONOUT$")
    }
}

impl Drop for AllocatedConsole {
    fn drop(&mut self) {
        // SAFETY: this guard represents the console allocated by `new`.
        unsafe {
            FreeConsole();
        }
    }
}

struct Host<'io, 'handle> {
    history: HistoryStore,
    history_cursor: HistoryCursor,
    input_source: SystemInput<'handle>,
    io: SessionIo<'io>,
    signal_resize_count: usize,
    history_count: usize,
    completion_count: usize,
}

fn drive_line<T: TerminalControl>(
    editor: &mut Editor<T>,
    driver: &mut ReadDriver,
    host: &mut Host<'_, '_>,
) -> Result<ReadResult, nshedit::editor::DriverError> {
    let mut step = driver.begin(editor)?;
    loop {
        step = match step {
            ReadStep::Prompt(pending) => {
                let prompt = match pending.request().side {
                    PromptSide::Left => Prompt::from("nsh> "),
                    PromptSide::Right => Prompt::from(format!("[{}]", host.history.len())),
                };
                driver.resume_prompt(editor, &pending, Ok(prompt))?
            }
            ReadStep::Resize(pending) => {
                if *pending.request() == ResizeEffect::Signal {
                    host.signal_resize_count += 1;
                }
                let response = host
                    .io
                    .descriptors
                    .output
                    .ok_or(HostFailure::Unavailable)
                    .and_then(|output| {
                        SystemTerminal::screen_size(output).map_err(|_| HostFailure::Unavailable)
                    });
                driver.resume_resize(editor, &pending, response)?
            }
            ReadStep::Read(pending) => {
                let response = host
                    .input_source
                    .read(host.io.input, *pending.request())
                    .map_err(host_failure);
                driver.resume_read(editor, &pending, response)?
            }
            ReadStep::History(pending) => {
                host.history_count += 1;
                let response = match host
                    .history
                    .navigate(&mut host.history_cursor, pending.request().direction)
                {
                    Navigation::Entry(entry) => HistoryResponse::entry(entry.line().clone()),
                    Navigation::Live => HistoryResponse::live(),
                    Navigation::Boundary => HistoryResponse::boundary(),
                };
                driver.resume_history(editor, &pending, Ok(response))?
            }
            ReadStep::HistorySearch(pending) => {
                driver.resume_history_search(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::HistoryLine(pending) => {
                driver.resume_history_line(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::HistoryWord(pending) => {
                driver.resume_history_word(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::Alias(pending) => {
                driver.resume_alias(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::EditorCommand(pending) => {
                driver.resume_editor_command(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::ExternalEdit(pending) => {
                driver.resume_external_edit(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::RecordHistory(pending) => {
                let response = host
                    .history
                    .push(pending.request().line.clone())
                    .map(|_| ())
                    .map_err(|error| HostFailure::Failed(error.to_string().into_boxed_str()));
                host.history_cursor.reset();
                driver.resume_history_record(editor, &pending, response)?
            }
            ReadStep::Completion(pending) => {
                host.completion_count += 1;
                let candidates = COMMANDS
                    .into_iter()
                    .map(|command| CompletionCandidate::new(command).with_suffix(" "))
                    .collect();
                driver.resume_completion(editor, &pending, Ok(candidates))?
            }
            ReadStep::UserCommand(pending) => {
                driver.resume_user_command(editor, &pending, Err(HostFailure::Unavailable))?
            }
            ReadStep::Signal(pending) => driver.resume_signal(editor, &pending, Ok(()))?,
            ReadStep::Display(display) => driver.display(editor, &display, host.io.output)?,
            ReadStep::Complete(result) => return Ok(result),
        };
    }
}

fn read_and_reset<T: TerminalControl>(
    editor: &mut Editor<T>,
    driver: &mut ReadDriver,
    host: &mut Host<'_, '_>,
) -> AcceptanceResult<Text> {
    let ReadResult::Accepted(line) = drive_line(editor, driver, host)? else {
        return Err("the console session did not accept a line".into());
    };
    editor.reset_line();
    host.history_cursor.reset();
    Ok(line)
}

fn host_failure(error: io::Error) -> HostFailure {
    if error.kind() == io::ErrorKind::Interrupted {
        HostFailure::Interrupted
    } else {
        HostFailure::Failed(error.to_string().into_boxed_str())
    }
}

fn install_terminal_bindings<T: TerminalControl>(
    editor: &mut Editor<T>,
) -> Result<(), nshedit::domain::Error> {
    for (sequence, binding) in [
        (
            "\u{1b}[A",
            Binding::Effect(EffectCommand::NavigateHistory(Direction::Previous)),
        ),
        (
            "\u{1b}[C",
            Binding::Action(Action::Move(Motion::Character(Direction::Next))),
        ),
        (
            "\u{1b}[D",
            Binding::Action(Action::Move(Motion::Character(Direction::Previous))),
        ),
        (
            "\u{1b}[H",
            Binding::Action(Action::Move(Motion::StartOfLine)),
        ),
        ("\u{1b}[F", Binding::Action(Action::Move(Motion::EndOfLine))),
        (
            "\u{1b}[3~",
            Binding::Action(Action::Delete(EditTarget::Character(Direction::Next))),
        ),
    ] {
        editor.bind(KeymapMode::Emacs, KeySequence::try_from(sequence)?, binding);
    }
    Ok(())
}

fn editing_records() -> Vec<INPUT_RECORD> {
    let mut records = vec![resize_record(100, 30)];
    records.extend(text_records("ac"));
    records.push(special_key(VK_LEFT));
    records.extend(text_records("b"));
    records.push(special_key(VK_HOME));
    records.extend(text_records("X"));
    records.push(special_key(VK_END));
    records.push(character_key('\u{8}', VK_BACK));
    records.extend(text_records("c"));
    records.push(special_key(VK_LEFT));
    records.push(special_key(VK_DELETE));
    records.push(special_key(VK_END));
    records.extend(text_records("😀"));
    records.push(character_key('\r', VK_RETURN));
    records.extend(text_records("he"));
    records.push(character_key('\t', VK_TAB));
    records.push(character_key('\r', VK_RETURN));
    records.push(special_key(VK_UP));
    records.push(character_key('\r', VK_RETURN));
    records.push(control_key(VK_C, '\u{3}'));
    records.push(control_key(VK_Z, '\u{1a}'));
    records
}

fn text_records(text: &str) -> impl Iterator<Item = INPUT_RECORD> + '_ {
    text.encode_utf16().map(|unit| key_record(unit, 0, 0))
}

fn character_key(character: char, virtual_key: u16) -> INPUT_RECORD {
    key_record(character as u16, virtual_key, 0)
}

fn special_key(virtual_key: u16) -> INPUT_RECORD {
    key_record(0, virtual_key, 0)
}

fn control_key(virtual_key: u16, character: char) -> INPUT_RECORD {
    key_record(character as u16, virtual_key, LEFT_CTRL_PRESSED)
}

fn key_record(unicode: u16, virtual_key: u16, control_state: u32) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: KEY_EVENT as u16,
        Event: INPUT_RECORD_0 {
            KeyEvent: KEY_EVENT_RECORD {
                bKeyDown: 1,
                wRepeatCount: 1,
                wVirtualKeyCode: virtual_key,
                wVirtualScanCode: 0,
                uChar: KEY_EVENT_RECORD_0 {
                    UnicodeChar: unicode,
                },
                dwControlKeyState: control_state,
            },
        },
    }
}

fn resize_record(columns: i16, rows: i16) -> INPUT_RECORD {
    INPUT_RECORD {
        EventType: WINDOW_BUFFER_SIZE_EVENT as u16,
        Event: INPUT_RECORD_0 {
            WindowBufferSizeEvent: WINDOW_BUFFER_SIZE_RECORD {
                dwSize: COORD {
                    X: columns,
                    Y: rows,
                },
            },
        },
    }
}

fn write_console_records(input: &File, records: &[INPUT_RECORD]) -> io::Result<()> {
    let mut written = 0;
    // SAFETY: the file is an open console-input handle and the slice remains
    // live for the duration of the call.
    if unsafe {
        WriteConsoleInputW(
            input.as_raw_handle(),
            records.as_ptr(),
            u32::try_from(records.len()).expect("the acceptance input is small"),
            &mut written,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    if written as usize != records.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "Windows accepted only part of the console input",
        ));
    }
    Ok(())
}

fn flush_console_input(input: &File) -> io::Result<()> {
    // SAFETY: the file is an open console-input handle.
    if unsafe { FlushConsoleInputBuffer(input.as_raw_handle()) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn console_mode(handle: &File) -> io::Result<u32> {
    let mut mode = 0;
    // SAFETY: the file is an open console handle and `mode` is writable.
    if unsafe { GetConsoleMode(handle.as_raw_handle(), &mut mode) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(mode)
    }
}

fn ordinary_error_restores(
    input: &File,
    output: &File,
    original_input: u32,
    original_output: u32,
) -> AcceptanceResult {
    let terminal = SystemTerminal::new(input.as_handle(), output.as_handle());
    let mut editor = Editor::new(EditorConfig::default(), terminal)?;
    editor.configure_display(TerminalProfile::ansi(), ScreenSize::new(24, 80)?);
    let mut driver = ReadDriver::default();
    let mut step = driver.begin(&mut editor)?;
    loop {
        step = match step {
            ReadStep::Resize(pending) => {
                driver.resume_resize(&mut editor, &pending, Ok(ScreenSize::new(24, 80)?))?
            }
            ReadStep::Prompt(pending) => {
                driver.resume_prompt(&mut editor, &pending, Ok(Prompt::from("nsh> ")))?
            }
            ReadStep::Display(display) => {
                let Err(error) = driver.display(&mut editor, &display, &mut FailingWriter) else {
                    return Err("the failing writer unexpectedly succeeded".into());
                };
                assert!(error.to_string().contains("acceptance writer failure"));
                break;
            }
            _ => return Err("display preparation requested an unexpected host effect".into()),
        };
    }
    assert_eq!(console_mode(input)?, original_input);
    assert_eq!(console_mode(output)?, original_output);
    drop(editor);
    Ok(())
}

fn unwinding_restores(input: &File, output: &File, original_input: u32, original_output: u32) {
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        let terminal = SystemTerminal::new(input.as_handle(), output.as_handle());
        let _editor = Editor::new(EditorConfig::default(), terminal).unwrap();
        panic!("acceptance unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(console_mode(input).unwrap(), original_input);
    assert_eq!(console_mode(output).unwrap(), original_output);
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("acceptance writer failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn conpty_editor_session() -> AcceptanceResult {
    let mut input =
        b"ac\x1b[Db\x1b[HX\x1b[F\x08c\x1b[D\x1b[3~\x1b[F\xf0\x9f\x98\x80\rhe\t\r\x1b[A\r".to_vec();
    input.extend(conpty_control_key(VK_C, '\u{3}'));
    let output = run_in_conpty(&input, Some(COORD { X: 100, Y: 30 }))?;
    let output = String::from_utf8_lossy(&output);
    assert!(
        output.contains("accepted: Xab😀"),
        "ConPTY output: {output:?}"
    );
    assert!(
        output.matches("accepted: help ").count() >= 2,
        "ConPTY output: {output:?}"
    );
    Ok(())
}

fn conpty_end_of_input() -> AcceptanceResult {
    let output = run_in_conpty(&conpty_control_key(VK_Z, '\u{1a}'), None)?;
    assert!(String::from_utf8_lossy(&output).contains("nshedit> "));
    Ok(())
}

fn conpty_control_key(virtual_key: u16, character: char) -> Vec<u8> {
    format!(
        "\u{1b}[{virtual_key};0;{};1;{LEFT_CTRL_PRESSED};1_",
        u32::from(character)
    )
    .into_bytes()
}

fn redirected_stream_session() -> AcceptanceResult {
    let mut child = Command::new(repl_executable()?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("redirected child has no input pipe")?
        .write_all(b"streamed\r")?;
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "redirected child failed: {output:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("accepted: streamed"),
        "redirected output: {stdout:?}"
    );
    assert!(
        output.stderr.is_empty(),
        "unexpected diagnostics: {:?}",
        output.stderr
    );
    Ok(())
}

fn run_in_conpty(input: &[u8], resize: Option<COORD>) -> AcceptanceResult<Vec<u8>> {
    let (pseudo_input, host_input) = pipe()?;
    let (host_output, pseudo_output) = pipe()?;
    let pseudo_console = PseudoConsole::new(COORD { X: 80, Y: 24 }, &pseudo_input, &pseudo_output)?;

    let process = pseudo_console.spawn(&repl_executable()?)?;
    drop(pseudo_input);
    drop(pseudo_output);
    let (prompt_sender, prompt_receiver) = mpsc::channel();
    let output_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let mut output_file = File::from(host_output);
        let mut buffer = [0; 4096];
        let mut prompt_seen = false;
        loop {
            let read = output_file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..read]);
            if !prompt_seen
                && output
                    .windows(b"nshedit> ".len())
                    .any(|window| window == b"nshedit> ")
            {
                prompt_seen = true;
                let _ = prompt_sender.send(());
            }
        }
        Ok::<_, io::Error>(output)
    });
    prompt_receiver
        .recv_timeout(Duration::from_millis(u64::from(CHILD_TIMEOUT_MS)))
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!("ConPTY child did not render its prompt: {error}"),
            )
        })?;
    let mut input_file = File::from(host_input);
    if let Some(size) = resize {
        pseudo_console.resize(size)?;
    }
    input_file.write_all(input)?;
    input_file.flush()?;
    drop(input_file);

    let exit_code = process.wait(CHILD_TIMEOUT_MS)?;
    drop(process);
    drop(pseudo_console);
    let output = output_reader
        .join()
        .map_err(|_| "ConPTY output reader panicked")??;
    if exit_code != 0 {
        return Err(format!("ConPTY child exited with status {exit_code}: {output:?}").into());
    }
    Ok(output)
}

fn pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read = null_mut();
    let mut write = null_mut();
    // SAFETY: both output slots are writable and null security attributes
    // request ordinary non-inheritable anonymous-pipe handles.
    if unsafe { CreatePipe(&mut read, &mut write, null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `CreatePipe` returned two fresh owned handles.
    let read = unsafe { OwnedHandle::from_raw_handle(read) };
    // SAFETY: `CreatePipe` returned two fresh owned handles.
    let write = unsafe { OwnedHandle::from_raw_handle(write) };
    Ok((read, write))
}

struct PseudoConsole(HPCON);

impl PseudoConsole {
    fn new(size: COORD, input: &OwnedHandle, output: &OwnedHandle) -> io::Result<Self> {
        let mut pseudo_console = 0;
        // SAFETY: the two handles are the correct live pipe ends and the
        // output slot remains writable for this call.
        let result = unsafe {
            CreatePseudoConsole(
                size,
                input.as_raw_handle(),
                output.as_raw_handle(),
                0,
                &mut pseudo_console,
            )
        };
        if result < 0 {
            Err(io::Error::other(format!(
                "CreatePseudoConsole failed with HRESULT {result:#x}"
            )))
        } else {
            Ok(Self(pseudo_console))
        }
    }

    fn resize(&self, size: COORD) -> io::Result<()> {
        // SAFETY: this guard owns a live pseudoconsole handle.
        let result = unsafe { ResizePseudoConsole(self.0, size) };
        if result < 0 {
            Err(io::Error::other(format!(
                "ResizePseudoConsole failed with HRESULT {result:#x}"
            )))
        } else {
            Ok(())
        }
    }

    fn spawn(&self, executable: &Path) -> io::Result<ChildProcess> {
        let attributes = AttributeList::new(self.0)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.lpAttributeList = attributes.pointer;
        let executable = wide_null(executable.as_os_str());
        let mut process = PROCESS_INFORMATION::default();
        // SAFETY: all pointers refer to live initialized values for the call;
        // `CreateProcessW` copies the executable path and startup attributes.
        if unsafe {
            CreateProcessW(
                executable.as_ptr(),
                null_mut(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT,
                null(),
                null(),
                &startup.StartupInfo,
                &mut process,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the successful process creation returned two fresh handles.
        let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
        // SAFETY: the successful process creation returned two fresh handles.
        let thread_handle = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
        drop(thread_handle);
        Ok(ChildProcess(process_handle))
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        // SAFETY: this guard owns the pseudoconsole handle exactly once.
        unsafe {
            ClosePseudoConsole(self.0);
        }
    }
}

struct AttributeList {
    _storage: Vec<usize>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new(pseudo_console: HPCON) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: a null first call asks Windows for the required byte count.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0usize; words];
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: `storage` is aligned and at least the requested size.
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `pointer` names the initialized list, and Windows copies the
        // pseudoconsole handle value before this function returns.
        if unsafe {
            UpdateProcThreadAttribute(
                pointer,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                pseudo_console as *const std::ffi::c_void,
                size_of::<HPCON>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            // SAFETY: the list was initialized successfully above.
            unsafe {
                DeleteProcThreadAttributeList(pointer);
            }
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            _storage: storage,
            pointer,
        })
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        // SAFETY: `pointer` remains backed by `_storage` and is deleted once.
        unsafe {
            DeleteProcThreadAttributeList(self.pointer);
        }
    }
}

struct ChildProcess(OwnedHandle);

impl ChildProcess {
    fn wait(&self, timeout_ms: u32) -> io::Result<u32> {
        // SAFETY: this guard owns a live process handle.
        match unsafe { WaitForSingleObject(self.0.as_raw_handle(), timeout_ms) } {
            WAIT_OBJECT_0 => {
                let mut exit_code = 0;
                // SAFETY: the process has exited and `exit_code` is writable.
                if unsafe { GetExitCodeProcess(self.0.as_raw_handle(), &mut exit_code) } == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(exit_code)
                }
            }
            WAIT_TIMEOUT => {
                // SAFETY: the process handle is live; termination prevents a
                // wedged acceptance child from escaping the test timeout.
                unsafe {
                    TerminateProcess(self.0.as_raw_handle(), 1);
                    WaitForSingleObject(self.0.as_raw_handle(), CHILD_TIMEOUT_MS);
                }
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "ConPTY child did not exit",
                ))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}

fn repl_executable() -> AcceptanceResult<PathBuf> {
    if let Some(path) = std::env::var_os("NSHEDIT_REPL_EXE") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }
    let executable = std::env::current_exe()?;
    let path = executable
        .parent()
        .and_then(Path::parent)
        .ok_or("cannot locate the Cargo target directory")?
        .join("examples")
        .join("repl.exe");
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "{} does not exist; build the repl example before this test",
            path.display()
        )
        .into())
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
