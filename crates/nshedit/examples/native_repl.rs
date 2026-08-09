//! A complete safe Rust consumer of the native editor API.

#![forbid(unsafe_code)]

use std::error::Error;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};

use nshedit::domain::{
    Action, Binding, Direction, EditTarget, EditorConfig, KeySequence, KeymapMode, Motion, Prompt,
    ScreenSize, SignalPolicy, Text, TextUnit,
};
use nshedit::editor::effect::{HistoryResponse, HostFailure, PromptSide, ReadEffect, ReadOutcome};
use nshedit::editor::{
    CompletionCandidate, Editor, ReadDriver, ReadResult, ReadStep, SystemTerminal, TerminalControl,
    TerminalProfile,
};
use nshedit::history::{HistoryCursor, HistoryStore, Navigation};

const COMMANDS: [&str; 3] = ["exit", "help", "history"];

// [spec:nshedit:req:core.native-consumer]
fn main() -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let terminal = SystemTerminal::new(stdin.as_fd(), stdout.as_fd());
    let config = EditorConfig::default().with_signal_policy(SignalPolicy::Ignore);
    let mut editor = Editor::new(config, terminal)?;
    let size = SystemTerminal::screen_size(stdout.as_fd())
        .unwrap_or_else(|_| ScreenSize::new(24, 80).expect("the fallback size is valid"));
    editor.configure_display(terminal_profile(), size);
    install_terminal_bindings(&mut editor)?;

    let mut driver = ReadDriver::default();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut host = Host {
        history: HistoryStore::new(),
        history_cursor: HistoryCursor::new(),
        input: &mut input,
        output: &mut output,
        input_fd: stdin.as_fd(),
        output_fd: stdout.as_fd(),
    };

    let session = run_repl(&mut editor, &mut driver, &mut host);
    let restoration = editor.finish();
    session?;
    restoration?;
    Ok(())
}

struct Host<'io, 'fd> {
    history: HistoryStore,
    history_cursor: HistoryCursor,
    input: &'io mut dyn Read,
    output: &'io mut dyn Write,
    input_fd: BorrowedFd<'fd>,
    output_fd: BorrowedFd<'fd>,
}

fn run_repl<T: TerminalControl>(
    editor: &mut Editor<T>,
    driver: &mut ReadDriver,
    host: &mut Host<'_, '_>,
) -> Result<(), Box<dyn Error>> {
    while let Some(line) = read_line(editor, driver, host)? {
        writeln!(host.output)?;
        let command = scalar_string(&line);
        match command.as_deref() {
            Some("exit") => break,
            Some("help") => writeln!(host.output, "commands: {}", COMMANDS.join(", "))?,
            Some("history") => write_history(host.output, &host.history)?,
            _ => {
                write!(host.output, "accepted: ")?;
                write_text(host.output, &line)?;
                writeln!(host.output)?;
            }
        }
        host.output.flush()?;
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
                    PromptSide::Left => Prompt::from("native> "),
                    PromptSide::Right => Prompt::from(format!("[{}]", host.history.len())),
                };
                driver.resume_prompt(editor, &pending, Ok(prompt))?
            }
            ReadStep::Resize(pending) => {
                let response = SystemTerminal::screen_size(host.output_fd)
                    .map_err(|_| HostFailure::Unavailable);
                driver.resume_resize(editor, &pending, response)?
            }
            ReadStep::Read(pending) => {
                let response = read_input(host.input, host.input_fd, *pending.request());
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
            ReadStep::Display(display) => driver.display(editor, &display, host.output)?,
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

fn read_input(
    input: &mut dyn Read,
    input_fd: BorrowedFd<'_>,
    purpose: ReadEffect,
) -> Result<ReadOutcome, HostFailure> {
    if purpose == ReadEffect::KeySequence && SystemTerminal::bytes_ready(input_fd).unwrap_or(0) == 0
    {
        return Ok(ReadOutcome::TimedOut);
    }

    let mut byte = [0];
    match input.read(&mut byte) {
        Ok(0) => Ok(ReadOutcome::EndOfInput),
        Ok(_) => Ok(ReadOutcome::Bytes(byte.into())),
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(HostFailure::Interrupted),
        Err(error) => Err(HostFailure::Failed(error.to_string().into_boxed_str())),
    }
}

fn install_terminal_bindings<T: TerminalControl>(
    editor: &mut Editor<T>,
) -> Result<(), nshedit::domain::Error> {
    for (sequence, action) in [
        ("\u{1b}[A", Action::History(Direction::Previous)),
        ("\u{1b}[B", Action::History(Direction::Next)),
        ("\u{1b}[C", Action::Move(Motion::Character(Direction::Next))),
        (
            "\u{1b}[D",
            Action::Move(Motion::Character(Direction::Previous)),
        ),
        ("\u{1b}[H", Action::Move(Motion::StartOfLine)),
        ("\u{1b}[F", Action::Move(Motion::EndOfLine)),
        (
            "\u{1b}[3~",
            Action::Delete(EditTarget::Character(Direction::Next)),
        ),
        (
            "\u{7f}",
            Action::Delete(EditTarget::Character(Direction::Previous)),
        ),
    ] {
        editor.bind(
            KeymapMode::Emacs,
            KeySequence::try_from(sequence)?,
            Binding::Action(action),
        );
    }
    Ok(())
}

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
            TextUnit::CompatibilityWide(_) => output.write_all("�".as_bytes())?,
        }
    }
    Ok(())
}

fn scalar_string(text: &Text) -> Option<String> {
    text.as_units()
        .iter()
        .map(|unit| match unit {
            TextUnit::Scalar(character) => Some(*character),
            TextUnit::RawByte(_) | TextUnit::CompatibilityWide(_) => None,
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
