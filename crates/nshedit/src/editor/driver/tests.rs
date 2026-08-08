use std::io;

use crate::domain::{
    Action, Binding, EditingMode, EditorConfig, KeySequence, KeymapMode, Prompt, ScreenSize,
    Signal, TerminalMode, Text, TextUnit,
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

    assert!(
        matches!(step, ReadStep::Complete(ReadResult::Accepted(ref line)) if line == &Text::from("é"))
    );
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
            Ok(HistoryResponse::Entry(Text::from("ec"))),
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
