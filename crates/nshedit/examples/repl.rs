//! A complete safe Rust consumer of the nshedit editor API.

#![forbid(unsafe_code)]

use std::error::Error;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(windows)]
use std::os::windows::io::AsHandle;

use nshedit::domain::{
    Action, Binding, Direction, EditTarget, EditorConfig, EffectCommand, KeySequence, KeymapMode,
    Motion, Prompt, ScreenSize, Text, TextUnit,
};
use nshedit::editor::effect::{HistoryResponse, HostFailure, PromptSide};
use nshedit::editor::{
    CompletionCandidate, Editor, IoDescriptors, ReadDriver, ReadResult, ReadStep, SessionIo,
    SystemInput, SystemTerminal, TerminalControl, TerminalProfile,
};
use nshedit::history::{HistoryCursor, HistoryStore, Navigation};

const COMMANDS: [&str; 3] = ["exit", "help", "history"];

// [spec:nshedit:req:core.native-consumer]
fn main() -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    #[cfg(unix)]
    let input_descriptor = stdin.as_fd();
    #[cfg(windows)]
    let input_descriptor = stdin.as_handle();
    #[cfg(unix)]
    let output_descriptor = stdout.as_fd();
    #[cfg(windows)]
    let output_descriptor = stdout.as_handle();
    #[cfg(unix)]
    let diagnostics_descriptor = stderr.as_fd();
    #[cfg(windows)]
    let diagnostics_descriptor = stderr.as_handle();

    let input_source = SystemInput::new(input_descriptor)?;
    let terminal = SystemTerminal::new(input_descriptor, output_descriptor);
    let config = EditorConfig::default();
    let mut editor = Editor::new(config, terminal)?;
    let size = SystemTerminal::screen_size(output_descriptor)
        .unwrap_or_else(|_| ScreenSize::new(24, 80).expect("the fallback size is valid"));
    #[cfg(unix)]
    let profile = terminal_profile();
    #[cfg(windows)]
    let profile = TerminalProfile::ansi();
    editor.configure_display(profile, size);
    install_terminal_bindings(&mut editor)?;

    let mut driver = ReadDriver::default();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut diagnostics = stderr.lock();
    let mut host = Host {
        history: HistoryStore::new(),
        history_cursor: HistoryCursor::new(),
        input_source,
        io: SessionIo {
            input: &mut input,
            output: &mut output,
            diagnostics: &mut diagnostics,
            descriptors: IoDescriptors {
                input: Some(input_descriptor),
                output: Some(output_descriptor),
                diagnostics: Some(diagnostics_descriptor),
            },
        },
    };

    let session = run_repl(&mut editor, &mut driver, &mut host);
    let restoration = editor.finish();
    session?;
    restoration?;
    Ok(())
}

struct Host<'io, 'handle> {
    history: HistoryStore,
    history_cursor: HistoryCursor,
    input_source: SystemInput<'handle>,
    io: SessionIo<'io>,
}

fn run_repl<T: TerminalControl>(
    editor: &mut Editor<T>,
    driver: &mut ReadDriver,
    host: &mut Host<'_, '_>,
) -> Result<(), Box<dyn Error>> {
    while let Some(line) = read_line(editor, driver, host)? {
        writeln!(host.io.output)?;
        let command = scalar_string(&line);
        match command.as_deref() {
            Some("exit") => break,
            Some("help") => writeln!(host.io.output, "commands: {}", COMMANDS.join(", "))?,
            Some("history") => write_history(host.io.output, &host.history)?,
            _ => {
                write!(host.io.output, "accepted: ")?;
                write_text(host.io.output, &line)?;
                writeln!(host.io.output)?;
            }
        }
        host.io.output.flush()?;
        editor.reset_line();
        host.history_cursor.reset();
    }
    Ok(())
}

fn read_line<T: TerminalControl>(
    editor: &mut Editor<T>,
    driver: &mut ReadDriver,
    host: &mut Host<'_, '_>,
) -> Result<Option<Text>, nshedit::editor::DriverError> {
    let mut step = driver.begin(editor)?;
    loop {
        step = match step {
            ReadStep::Prompt(pending) => {
                let prompt = match pending.request().side {
                    PromptSide::Left => Prompt::from("nshedit> "),
                    PromptSide::Right => Prompt::from(format!("[{}]", host.history.len())),
                };
                driver.resume_prompt(editor, &pending, Ok(prompt))?
            }
            ReadStep::Resize(pending) => {
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
            ReadStep::Complete(result) => {
                return Ok(match result {
                    ReadResult::Accepted(line) => Some(line),
                    ReadResult::Character(unit) => Some(std::iter::once(unit).collect()),
                    ReadResult::Command | ReadResult::EndOfInput | ReadResult::Interrupted(_) => {
                        None
                    }
                });
            }
        };
    }
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
            "\u{1b}[B",
            Binding::Effect(EffectCommand::NavigateHistory(Direction::Next)),
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
        (
            "\u{7f}",
            Binding::Action(Action::Delete(EditTarget::Character(Direction::Previous))),
        ),
    ] {
        editor.bind(KeymapMode::Emacs, KeySequence::try_from(sequence)?, binding);
    }
    Ok(())
}

#[cfg(unix)]
fn terminal_profile() -> TerminalProfile {
    std::env::var("TERM")
        .ok()
        .and_then(|name| nshterm::TermInfo::from_name(&name).ok())
        .map(|entry| TerminalProfile::from_terminfo(&entry))
        .unwrap_or_else(TerminalProfile::ansi)
}

fn write_history(output: &mut dyn Write, history: &HistoryStore) -> io::Result<()> {
    for (index, entry) in history.iter().enumerate() {
        write!(output, "{:>4}  ", index + 1)?;
        write_text(output, entry.line())?;
        writeln!(output)?;
    }
    Ok(())
}

fn write_text(output: &mut dyn Write, text: &Text) -> io::Result<()> {
    for unit in text {
        match unit {
            TextUnit::Scalar(character) => {
                let mut encoded = [0; 4];
                output.write_all(character.encode_utf8(&mut encoded).as_bytes())?;
            }
            TextUnit::RawByte(byte) => output.write_all(&[*byte])?,
            TextUnit::OpaqueCodePoint(_) => output.write_all("�".as_bytes())?,
        }
    }
    Ok(())
}

fn scalar_string(text: &Text) -> Option<String> {
    text.as_units()
        .iter()
        .map(|unit| match unit {
            TextUnit::Scalar(character) => Some(*character),
            TextUnit::RawByte(_) | TextUnit::OpaqueCodePoint(_) => None,
        })
        .collect()
}

// [spec:nshedit:req:core.native-consumer/test]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_commands_are_checked() {
        assert_eq!(
            scalar_string(&Text::from("history")),
            Some("history".into())
        );
        assert_eq!(
            scalar_string(&[TextUnit::RawByte(0xff)].into_iter().collect()),
            None
        );
    }

    #[test]
    fn text_output_preserves_raw_bytes() {
        let text = [TextUnit::Scalar('x'), TextUnit::RawByte(0xff)]
            .into_iter()
            .collect();
        let mut output = Vec::new();
        write_text(&mut output, &text).unwrap();
        assert_eq!(output, [b'x', 0xff]);
    }
}
