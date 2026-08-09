use std::io;

use crate::domain::{
    Action, ArgumentCommand, Binding, CommandName, CommandSequence, EditingMode, EditorConfig,
    KeySequence, KeymapMode, Motion, Prompt, ScreenSize, Signal, TerminalMode, Text, TextUnit,
};
use crate::editor::effect::{HistoryResponse, PromptSide, ReadEffect, ReadOutcome};
use crate::editor::{CompletionCandidate, CompletionCandidates, TerminalProfile};

use super::*;

struct TestTerminal;

impl TerminalControl for TestTerminal {
    fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
        Ok(())
    }

    fn set_mode(&mut self, _mode: TerminalMode) -> io::Result<()> {
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn editor(config: EditorConfig) -> Editor<TestTerminal> {
    let mut editor = Editor::new(config, TestTerminal).unwrap();
    editor.configure_display(TerminalProfile::ansi(), ScreenSize::new(3, 24).unwrap());
    editor
}

fn settle(
    driver: &mut ReadDriver,
    editor: &mut Editor<TestTerminal>,
    mut step: ReadStep,
    output: &mut Vec<u8>,
) -> ReadStep {
    loop {
        step = match step {
            ReadStep::Resize(pending) => driver
                .resume_resize(editor, &pending, Ok(ScreenSize::new(3, 24).unwrap()))
                .unwrap(),
            ReadStep::Prompt(pending) => {
                let prompt = match pending.request().side {
                    PromptSide::Left => Prompt::from("p> "),
                    PromptSide::Right => Prompt::default(),
                };
                driver.resume_prompt(editor, &pending, Ok(prompt)).unwrap()
            }
            ReadStep::Display(display) => driver.display(editor, &display, output).unwrap(),
            other => return other,
        };
    }
}

fn read(step: ReadStep) -> Pending<ReadEffect> {
    match step {
        ReadStep::Read(pending) => pending,
        _ => panic!("driver did not request input"),
    }
}

fn input_unit(
    driver: &mut ReadDriver,
    editor: &mut Editor<TestTerminal>,
    pending: &Pending<ReadEffect>,
    character: char,
) -> ReadStep {
    driver
        .resume_read(
            editor,
            pending,
            Ok(ReadOutcome::Unit(TextUnit::Scalar(character))),
        )
        .unwrap()
}

fn send(
    driver: &mut ReadDriver,
    editor: &mut Editor<TestTerminal>,
    step: ReadStep,
    output: &mut Vec<u8>,
    character: char,
) -> ReadStep {
    let pending = read(settle(driver, editor, step, output));
    input_unit(driver, editor, &pending, character)
}

fn vi_with_line(line: &str) -> Editor<TestTerminal> {
    let config = EditorConfig::default().with_editing_mode(EditingMode::Vi);
    let mut editor = editor(config);
    editor.execute(Action::Insert(Text::from(line))).unwrap();
    editor.execute(Action::Move(Motion::StartOfBuffer)).unwrap();
    editor
        .execute(Action::SetKeymap(KeymapMode::ViCommand))
        .unwrap();
    editor
}

// [spec:nshedit:req:core.read-driver/test]
#[test]
fn driver_decodes_and_accepts_utf8() {
    let mut editor = editor(EditorConfig::default());
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let mut pending = read(settle(&mut driver, &mut editor, begin, &mut output));

    let step = driver
        .resume_read(
            &mut editor,
            &pending,
            Ok(ReadOutcome::Bytes(Box::from([0xc3]))),
        )
        .unwrap();
    pending = read(step);
    let step = driver
        .resume_read(
            &mut editor,
            &pending,
            Ok(ReadOutcome::Bytes(Box::from([0xa9]))),
        )
        .unwrap();
    pending = read(settle(&mut driver, &mut editor, step, &mut output));
    let step = input_unit(&mut driver, &mut editor, &pending, '\n');
    let ReadStep::RecordHistory(record) = step else {
        panic!("acceptance did not request history recording");
    };
    let step = driver
        .resume_history_record(&mut editor, &record, Ok(()))
        .unwrap();
    let step = settle(&mut driver, &mut editor, step, &mut output);

    assert!(
        matches!(step, ReadStep::Complete(ReadResult::Accepted(ref line)) if line == &Text::from("é"))
    );
    assert!(output.ends_with(b"\n"));
    assert_eq!(editor.line(), &Text::from("é"));
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
}

#[test]
fn prefix_timeout_uses_exact_binding() {
    let mut editor = editor(EditorConfig::default());
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("x").unwrap(),
        Binding::Action(Action::Insert(Text::from("A"))),
    );
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("xy").unwrap(),
        Binding::Action(Action::Insert(Text::from("B"))),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));
    let step = input_unit(&mut driver, &mut editor, &pending, 'x');
    let pending = read(step);
    assert_eq!(pending.request(), &ReadEffect::KeySequence);
    let step = driver
        .resume_read(&mut editor, &pending, Ok(ReadOutcome::TimedOut))
        .unwrap();
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("A"));
}

#[test]
fn vi_counts_and_macros_are_bounded() {
    let config = EditorConfig::default().with_editing_mode(EditingMode::Vi);
    let mut editor = editor(config);
    editor
        .execute(Action::SetKeymap(KeymapMode::ViCommand))
        .unwrap();
    editor.bind(
        KeymapMode::ViCommand,
        KeySequence::try_from("x").unwrap(),
        Binding::Action(Action::Insert(Text::from("z"))),
    );
    editor.bind(
        KeymapMode::ViCommand,
        KeySequence::try_from("q").unwrap(),
        Binding::Macro(Text::from("x")),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));
    let step = input_unit(&mut driver, &mut editor, &pending, '3');
    let pending = read(step);
    let step = input_unit(&mut driver, &mut editor, &pending, 'x');
    let pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("zzz"));

    let step = input_unit(&mut driver, &mut editor, &pending, '2');
    let pending = read(step);
    let step = input_unit(&mut driver, &mut editor, &pending, 'q');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("zzzzz"));
}

#[test]
fn user_command_receives_invoking_unit() {
    let mut editor = editor(EditorConfig::default());
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("xy").unwrap(),
        Binding::Action(Action::User(CommandName::new("host-command").unwrap())),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));
    let step = input_unit(&mut driver, &mut editor, &pending, 'x');
    let pending = read(step);
    let step = input_unit(&mut driver, &mut editor, &pending, 'y');
    let ReadStep::UserCommand(command) = step else {
        panic!("binding did not request the host command");
    };
    assert_eq!(command.request().invoking, TextUnit::Scalar('y'));
}

#[test]
fn echo_uses_human_readable_control_notation() {
    let mut output = Vec::new();

    write_echo(TextUnit::Scalar('\u{4}'), &mut output).unwrap();
    write_echo(TextUnit::RawByte(0x7f), &mut output).unwrap();
    write_echo(TextUnit::Scalar('x'), &mut output).unwrap();

    assert_eq!(output, b"^D^?x");
}

#[test]
fn history_restoration_preserves_live_line() {
    let mut editor = editor(EditorConfig::default());
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));
    editor.execute(Action::Insert(Text::from("draft"))).unwrap();
    assert!(editor.can_undo());

    let step = input_unit(&mut driver, &mut editor, &pending, '\u{10}');
    let ReadStep::History(history) = step else {
        panic!("history binding did not suspend");
    };
    let emitted_before = output.len();
    let step = driver
        .resume_history(
            &mut editor,
            &history,
            Ok(HistoryResponse::entry(Text::from("old")).at_boundary()),
        )
        .unwrap();
    let pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("old"));
    assert_eq!(editor.cursor(), editor.line().index(3).unwrap());
    assert!(!editor.can_undo());
    assert!(output[emitted_before..].contains(&b'\x07'));

    let step = input_unit(&mut driver, &mut editor, &pending, '\u{e}');
    let ReadStep::History(history) = step else {
        panic!("next-history binding did not suspend");
    };
    let step = driver
        .resume_history(&mut editor, &history, Ok(HistoryResponse::live()))
        .unwrap();
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("draft"));
    assert_eq!(editor.cursor(), editor.line().index(5).unwrap());
    assert!(!editor.can_undo());
}

#[test]
fn history_completion_and_signal_resume() {
    let mut editor = editor(EditorConfig::default());
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));
    let step = input_unit(&mut driver, &mut editor, &pending, '\u{10}');
    let ReadStep::History(history) = step else {
        panic!("history binding did not suspend");
    };
    let step = driver
        .resume_history(
            &mut editor,
            &history,
            Ok(HistoryResponse::entry(Text::from("ec"))),
        )
        .unwrap();
    let pending = read(settle(&mut driver, &mut editor, step, &mut output));
    let step = input_unit(&mut driver, &mut editor, &pending, '\t');
    let ReadStep::Completion(completion) = step else {
        panic!("completion binding did not suspend");
    };
    assert_eq!(completion.request().query.stem(), &Text::from("ec"));
    let candidates: CompletionCandidates =
        vec![CompletionCandidate::new("echo").with_suffix(" ")].into();
    let step = driver
        .resume_completion(&mut editor, &completion, Ok(candidates))
        .unwrap();
    let pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("echo "));

    let step = driver
        .resume_read(
            &mut editor,
            &pending,
            Ok(ReadOutcome::Signal(Signal::Interrupt)),
        )
        .unwrap();
    let ReadStep::Signal(signal) = step else {
        panic!("interrupt did not request host propagation");
    };
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
    let step = driver.resume_signal(&mut editor, &signal, Ok(())).unwrap();
    assert!(matches!(
        step,
        ReadStep::Complete(ReadResult::Interrupted(ReadInterrupt::Signal(
            Signal::Interrupt
        )))
    ));
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
}

// [spec:nshedit:req:core.command-sequences/test]
#[test]
fn quoted_and_meta() {
    let mut editor = editor(EditorConfig::default());
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("4").unwrap(),
        Binding::Sequence(CommandSequence::Argument(ArgumentCommand::StartDigit)),
    );
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("m").unwrap(),
        Binding::Sequence(CommandSequence::MetaNext),
    );
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("á").unwrap(),
        Binding::Action(Action::Insert(Text::from("Z"))),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '4');
    let step = send(&mut driver, &mut editor, step, &mut output, 'm');
    let step = send(&mut driver, &mut editor, step, &mut output, 'a');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("ZZZZ"));

    let step = send(&mut driver, &mut editor, step, &mut output, '\u{16}');
    assert!(matches!(step, ReadStep::Read(_)));
    assert_eq!(editor.terminal_mode(), TerminalMode::Quoted);
    let step = send(&mut driver, &mut editor, step, &mut output, '\u{4}');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.terminal_mode(), TerminalMode::Editing);
    assert_eq!(
        editor.line().as_units(),
        &[
            TextUnit::Scalar('Z'),
            TextUnit::Scalar('Z'),
            TextUnit::Scalar('Z'),
            TextUnit::Scalar('Z'),
            TextUnit::Scalar('\u{4}'),
        ]
    );
}

// [spec:nshedit:req:core.command-sequences/test]
#[test]
fn vi_operator_composition() {
    let mut editor = vi_with_line("one two three");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut editor, step, &mut output, 'd');
    let step = send(&mut driver, &mut editor, step, &mut output, 'w');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("three"));
    assert_eq!(editor.kill_buffer(), Some(&Text::from("one two ")));

    let step = send(&mut driver, &mut editor, step, &mut output, 'u');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("one two three"));

    let step = send(&mut driver, &mut editor, step, &mut output, 'd');
    let step = send(&mut driver, &mut editor, step, &mut output, 'd');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert!(editor.line().is_empty());
    assert_eq!(editor.kill_buffer(), Some(&Text::from("one two three")));
}

// [spec:nshedit:req:core.command-sequences/test]
#[test]
fn vi_character_search() {
    let mut editor = vi_with_line("abacad");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut editor, step, &mut output, 'f');
    let step = send(&mut driver, &mut editor, step, &mut output, 'a');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.cursor().get(), 4);

    let step = send(&mut driver, &mut editor, step, &mut output, ',');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.cursor().get(), 2);

    let step = send(&mut driver, &mut editor, step, &mut output, '0');
    let step = send(&mut driver, &mut editor, step, &mut output, 'd');
    let step = send(&mut driver, &mut editor, step, &mut output, 'f');
    let step = send(&mut driver, &mut editor, step, &mut output, 'c');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("ad"));
    assert_eq!(editor.kill_buffer(), Some(&Text::from("abac")));
}

// [spec:nshedit:req:core.command-sequences/test]
#[test]
fn vi_substitution_replay() {
    let mut editor = vi_with_line("abcdef");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut editor, step, &mut output, 's');
    let step = send(&mut driver, &mut editor, step, &mut output, 'X');
    let step = send(&mut driver, &mut editor, step, &mut output, 'Y');
    let step = send(&mut driver, &mut editor, step, &mut output, '\u{1b}');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("XYcdef"));
    assert_eq!(editor.keymap_mode(), KeymapMode::ViCommand);

    let step = send(&mut driver, &mut editor, step, &mut output, 'l');
    let step = send(&mut driver, &mut editor, step, &mut output, '.');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("XYXYef"));

    let step = send(&mut driver, &mut editor, step, &mut output, 'u');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("XYcdef"));
    let step = send(&mut driver, &mut editor, step, &mut output, 'u');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("abcdef"));
}

// [spec:nshedit:req:core.command-sequences/test]
#[test]
fn vi_replacement_replay() {
    let mut editor = vi_with_line("abcdef");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '3');
    let step = send(&mut driver, &mut editor, step, &mut output, 'r');
    let step = send(&mut driver, &mut editor, step, &mut output, 'X');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("XXXdef"));
    assert_eq!(editor.cursor().get(), 2);

    let step = send(&mut driver, &mut editor, step, &mut output, 'l');
    let step = send(&mut driver, &mut editor, step, &mut output, '.');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("XXXXXX"));
    assert_eq!(editor.cursor().get(), 5);
}

// [spec:nshedit:req:core.command-sequences/test]
#[test]
fn semantic_replay_bound() {
    let mut editor = vi_with_line("abcdef");
    let mut driver = ReadDriver::default().with_work_limit(NonZeroUsize::new(3).unwrap());
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut editor, step, &mut output, 'x');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("cdef"));

    let step = send(&mut driver, &mut editor, step, &mut output, '2');
    let pending = read(settle(&mut driver, &mut editor, step, &mut output));
    let result = driver.resume_read(
        &mut editor,
        &pending,
        Ok(ReadOutcome::Unit(TextUnit::Scalar('.'))),
    );
    assert!(matches!(
        result,
        Err(DriverError::WorkLimitExceeded { limit: 3 })
    ));
    assert_eq!(editor.line(), &Text::from("cdef"));
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
}
