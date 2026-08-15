use std::io;
use std::time::Duration;

use crate::domain::{
    Action, ArgumentCommand, Binding, CommandName, CommandSequence, EditingMode, EditorConfig,
    EffectCommand, HistorySearchCommand, HistorySearchRepetition, ImmediateCommand, KeySequence,
    KeymapMode, Motion, Prompt, ScreenSize, Signal, TerminalMode, Text, TextUnit, WordTraversal,
};
use crate::editor::effect::{
    AliasResponse, HistoryMatch, HistoryPosition, HistoryResponse, HistorySearchInput,
    HistorySearchResponse, HistoryWordPosition, HistoryWordResponse, PromptSide, ReadEffect,
    ReadOutcome,
};
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

fn begin_wrapped_line(
    driver: &mut ReadDriver,
    editor: &mut Editor<TestTerminal>,
    output: &mut Vec<u8>,
    prompt: &str,
) -> Pending<ReadEffect> {
    let ReadStep::Resize(resize) = driver.begin(editor).unwrap() else {
        panic!("a read did not begin with terminal preparation");
    };
    let step = driver
        .resume_resize(editor, &resize, Ok(ScreenSize::new(5, 4).unwrap()))
        .unwrap();
    let ReadStep::Prompt(left) = step else {
        panic!("display preparation did not request the left prompt");
    };
    let step = driver
        .resume_prompt(editor, &left, Ok(Prompt::from(prompt)))
        .unwrap();
    let ReadStep::Prompt(right) = step else {
        panic!("display preparation did not request the right prompt");
    };
    let step = driver
        .resume_prompt(editor, &right, Ok(Prompt::default()))
        .unwrap();
    let ReadStep::Display(display) = step else {
        panic!("prompt preparation did not request a display");
    };
    read(driver.display(editor, &display, output).unwrap())
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

// [spec:nshedit:req:core.read-driver+1/test]
#[test]
fn continuation_token_guards_driver_state() {
    let mut first_editor = editor(EditorConfig::default());
    let mut second_editor = editor(EditorConfig::default());
    let mut first = ReadDriver::default();
    let mut second = ReadDriver::default();

    let ReadStep::Resize(resize) = first.begin(&mut first_editor).unwrap() else {
        panic!("a read did not begin with terminal preparation");
    };
    assert!(matches!(
        first.begin(&mut first_editor),
        Err(DriverError::Busy)
    ));
    assert!(matches!(
        second.resume_resize(
            &mut second_editor,
            &resize,
            Ok(ScreenSize::new(3, 24).unwrap())
        ),
        Err(DriverError::DifferentDriver)
    ));

    let next = first
        .resume_resize(
            &mut first_editor,
            &resize,
            Ok(ScreenSize::new(3, 24).unwrap()),
        )
        .unwrap();
    assert!(matches!(
        first.resume_resize(
            &mut first_editor,
            &resize,
            Ok(ScreenSize::new(3, 24).unwrap())
        ),
        Err(DriverError::StaleStep)
    ));

    let ReadStep::Prompt(prompt) = next else {
        panic!("terminal preparation did not request the left prompt");
    };
    assert_eq!(prompt.request().side, PromptSide::Left);
    assert!(first_editor.line().is_empty());
    assert!(
        first
            .resume_prompt(&mut first_editor, &prompt, Ok(Prompt::from("p> ")))
            .is_ok()
    );
}

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

// [spec:nshedit:req:core.incremental-render+4/test]
#[test]
fn accept_and_eof_finish_below_region() {
    const FINISH: &[u8] = b"\x1b8\x1b[B\x1b[B\n";

    let mut accepted_editor = editor(EditorConfig::default());
    accepted_editor
        .execute(Action::Insert(Text::from("abcde")))
        .unwrap();
    let mut accepted_driver = ReadDriver::default();
    let mut output = Vec::new();
    let pending = begin_wrapped_line(
        &mut accepted_driver,
        &mut accepted_editor,
        &mut output,
        "p\n> ",
    );
    output.clear();
    let step = input_unit(&mut accepted_driver, &mut accepted_editor, &pending, '\n');
    let ReadStep::RecordHistory(record) = step else {
        panic!("acceptance did not request history recording");
    };
    let step = accepted_driver
        .resume_history_record(&mut accepted_editor, &record, Ok(()))
        .unwrap();
    let ReadStep::Display(display) = step else {
        panic!("acceptance did not request final display output");
    };
    let step = accepted_driver
        .display(&mut accepted_editor, &display, &mut output)
        .unwrap();
    assert!(matches!(step, ReadStep::Complete(ReadResult::Accepted(_))));
    assert_eq!(output, FINISH);
    assert_eq!(accepted_editor.terminal_mode(), TerminalMode::Cooked);

    let mut eof_editor = editor(EditorConfig::default());
    eof_editor
        .execute(Action::Insert(Text::from("abcde")))
        .unwrap();
    let mut eof_driver = ReadDriver::default();
    let pending = begin_wrapped_line(&mut eof_driver, &mut eof_editor, &mut output, "p\n> ");
    output.clear();
    let step = eof_driver
        .resume_read(&mut eof_editor, &pending, Ok(ReadOutcome::EndOfInput))
        .unwrap();
    let ReadStep::Display(display) = step else {
        panic!("end of input did not request final display output");
    };
    let step = eof_driver
        .display(&mut eof_editor, &display, &mut output)
        .unwrap();
    assert!(matches!(step, ReadStep::Complete(ReadResult::EndOfInput)));
    assert_eq!(output, FINISH);
    assert_eq!(eof_editor.terminal_mode(), TerminalMode::Cooked);
}

// [spec:nshedit:req:core.incremental-render+4/test]
#[test]
fn eof_echo_wrap_reserves_region() {
    let mut editor = editor(EditorConfig::default());
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let pending = begin_wrapped_line(&mut driver, &mut editor, &mut output, "p\n>>>");
    output.clear();

    let step = input_unit(&mut driver, &mut editor, &pending, '\u{4}');
    let ReadStep::Display(display) = step else {
        panic!("visible end of input did not request final display output");
    };
    let step = driver.display(&mut editor, &display, &mut output).unwrap();

    assert!(matches!(step, ReadStep::Complete(ReadResult::EndOfInput)));
    assert_eq!(
        output,
        b"\x1b8\x1b[B\r\n\x1b[A\x1b[A\x1b7\x1b8\x1b[B\x1b[C\x1b[C\x1b[C^D\x1b8\x1b[B\x1b[B\n"
    );
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
    assert_eq!(
        editor.screen_cursor(),
        Some(ScreenSize::new(5, 4).unwrap().position(0, 0).unwrap())
    );
    assert!(
        editor
            .screen()
            .unwrap()
            .cells()
            .iter()
            .all(|cell| matches!(cell, crate::domain::ScreenCell::Blank))
    );

    // The released region leaves the next prompt inline again, so it reserves
    // nothing and draws only itself.
    let mut next_frame = Vec::new();
    editor
        .render_to(&Prompt::from("n> "), None, &mut next_frame)
        .unwrap();
    assert_eq!(next_frame, b"n> ");
}

#[test]
fn prefix_timeout_uses_exact_binding() {
    let timeout = Duration::from_millis(25);
    let mut first_editor = editor(EditorConfig::default().with_key_sequence_timeout(timeout));
    first_editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("x").unwrap(),
        Binding::Action(Action::Insert(Text::from("A"))),
    );
    first_editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("xy").unwrap(),
        Binding::Action(Action::Insert(Text::from("B"))),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut first_editor).unwrap();
    let pending = read(settle(&mut driver, &mut first_editor, begin, &mut output));
    let step = input_unit(&mut driver, &mut first_editor, &pending, 'x');
    let pending = read(step);
    let ReadEffect::KeySequence { deadline } = *pending.request() else {
        panic!("an ambiguous binding must carry a key-sequence deadline");
    };
    assert_eq!(deadline.timeout(), timeout);

    let step = driver
        .resume_read(
            &mut first_editor,
            &pending,
            Ok(ReadOutcome::Bytes(vec![0xc3].into_boxed_slice())),
        )
        .unwrap();
    let pending = read(step);
    assert_eq!(
        pending.request(),
        &ReadEffect::KeySequence { deadline },
        "an incomplete byte sequence must not restart the prefix deadline"
    );
    let step = driver
        .resume_read(&mut first_editor, &pending, Ok(ReadOutcome::TimedOut))
        .unwrap();
    let _pending = read(settle(&mut driver, &mut first_editor, step, &mut output));
    assert_eq!(first_editor.line(), &Text::from("A"));
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
        Binding::User(CommandName::new("host-command").unwrap()),
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
fn finish_echo_uses_display_columns() {
    let control = FinishEcho::new(TextUnit::Scalar('\u{4}'));
    assert_eq!(control.as_bytes(), b"^D");
    assert_eq!(control.columns(), 2);

    let utf8_control = FinishEcho::new(TextUnit::Scalar('\u{85}'));
    assert_eq!(utf8_control.as_bytes(), "^\u{c5}".as_bytes());
    assert_eq!(utf8_control.columns(), 2);

    let wide = FinishEcho::new(TextUnit::Scalar('界'));
    assert_eq!(wide.as_bytes(), "界".as_bytes());
    assert_eq!(wide.columns(), 2);

    let combining = FinishEcho::new(TextUnit::Scalar('\u{301}'));
    assert_eq!(combining.as_bytes(), "\u{301}".as_bytes());
    assert_eq!(combining.columns(), 0);
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

// [spec:nshedit:req:abi.signal-lifecycle/test]
#[test]
fn signal_transitions_are_typed() {
    let mut editor = editor(EditorConfig::default());
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));

    let step = driver
        .resume_read(
            &mut editor,
            &pending,
            Ok(ReadOutcome::Signal(Signal::Resize)),
        )
        .unwrap();
    let ReadStep::Resize(resize) = step else {
        panic!("resize delivery did not request dimensions");
    };
    assert_eq!(*resize.request(), ResizeEffect::Signal);
    let step = driver
        .resume_resize(&mut editor, &resize, Ok(ScreenSize::new(4, 30).unwrap()))
        .unwrap();
    let ReadStep::Signal(resize) = step else {
        panic!("resize did not request disposition propagation");
    };
    assert_eq!(resize.request().signal, Signal::Resize);
    let step = driver.resume_signal(&mut editor, &resize, Ok(())).unwrap();
    let pending = read(settle(&mut driver, &mut editor, step, &mut output));

    let step = driver
        .resume_read(
            &mut editor,
            &pending,
            Ok(ReadOutcome::Signal(Signal::Suspend)),
        )
        .unwrap();
    let ReadStep::Signal(suspend) = step else {
        panic!("suspend did not request disposition propagation");
    };
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
    let step = driver.resume_signal(&mut editor, &suspend, Ok(())).unwrap();
    let ReadStep::Signal(resume) = step else {
        panic!("suspend resumption did not propagate continue");
    };
    assert_eq!(resume.request().signal, Signal::Continue);
    assert_eq!(editor.terminal_mode(), TerminalMode::Editing);
    let step = driver.resume_signal(&mut editor, &resume, Ok(())).unwrap();
    let ReadStep::Resize(resume) = step else {
        panic!("continue did not request display rebuilding");
    };
    assert_eq!(*resume.request(), ResizeEffect::Resume);
    let step = driver
        .resume_resize(&mut editor, &resume, Ok(ScreenSize::new(4, 30).unwrap()))
        .unwrap();
    assert!(matches!(
        settle(&mut driver, &mut editor, step, &mut output),
        ReadStep::Read(_)
    ));
}

#[test]
fn signal_failure_is_not_accepted() {
    let mut editor = editor(EditorConfig::default());
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let pending = read(settle(&mut driver, &mut editor, begin, &mut output));
    let step = driver
        .resume_read(
            &mut editor,
            &pending,
            Ok(ReadOutcome::Signal(Signal::Interrupt)),
        )
        .unwrap();
    let ReadStep::Signal(signal) = step else {
        panic!("interrupt did not request propagation");
    };
    assert!(matches!(
        driver.resume_signal(&mut editor, &signal, Err(HostFailure::Unavailable)),
        Err(DriverError::Host(HostFailure::Unavailable))
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

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn history_search_effect() {
    let mut editor = editor(EditorConfig::default());
    editor.execute(Action::Insert(Text::from("car"))).unwrap();
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("q").unwrap(),
        Binding::Effect(EffectCommand::SearchHistory(HistorySearchCommand::Prefix(
            crate::domain::Direction::Previous,
        ))),
    );
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("r").unwrap(),
        Binding::Effect(EffectCommand::SearchHistory(HistorySearchCommand::Repeat(
            HistorySearchRepetition::OppositeDirection,
        ))),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, 'q');
    let ReadStep::HistorySearch(search) = step else {
        panic!("search command did not suspend");
    };
    assert_eq!(
        search.request().input,
        HistorySearchInput::Pattern(Text::from("car"))
    );
    assert_eq!(search.request().matching, HistoryMatch::LiteralOrRegex);
    let step = driver
        .resume_history_search(
            &mut editor,
            &search,
            Ok(HistorySearchResponse {
                history: HistoryResponse::entry(Text::from("cargo test")),
                pattern: Text::from("car"),
            }),
        )
        .unwrap();
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert_eq!(editor.line(), &Text::from("cargo test"));

    let step = send(&mut driver, &mut editor, step, &mut output, 'r');
    let ReadStep::HistorySearch(repeated) = step else {
        panic!("repeat search did not suspend");
    };
    assert_eq!(repeated.request().direction, crate::domain::Direction::Next);
    assert_eq!(
        repeated.request().input,
        HistorySearchInput::Pattern(Text::from("car"))
    );
    assert_eq!(repeated.request().matching, HistoryMatch::LiteralOrRegex);
}

// [spec:libedit:sem:search.cv-search-fn/test]
#[test]
fn prompted_search_propagates_eof() {
    let mut editor = editor(EditorConfig::default());
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("q").unwrap(),
        Binding::Effect(EffectCommand::SearchHistory(HistorySearchCommand::Prompt(
            crate::domain::Direction::Previous,
        ))),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, 'q');
    let ReadStep::HistorySearch(search) = step else {
        panic!("prompted search command did not suspend");
    };
    assert_eq!(search.request().input, HistorySearchInput::Prompted);

    let step = driver
        .resume_history_search(&mut editor, &search, Err(HostFailure::EndOfInput))
        .unwrap();
    assert!(matches!(step, ReadStep::Complete(ReadResult::EndOfInput)));
}

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn alias_expansion_effect() {
    let mut editor = vi_with_line("a");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '@');
    let step = send(&mut driver, &mut editor, step, &mut output, 'g');
    let ReadStep::Alias(alias) = step else {
        panic!("alias selector did not suspend");
    };
    assert_eq!(alias.request().name, Text::from("_g"));
    let step = driver
        .resume_alias(
            &mut editor,
            &alias,
            Ok(AliasResponse::Expansion(Text::from("iX\u{1b}"))),
        )
        .unwrap();
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("Xa"));
    assert_eq!(editor.keymap_mode(), KeymapMode::ViCommand);
}

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn history_selection_chain() {
    let mut editor = vi_with_line("draft");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut editor, step, &mut output, 'v');
    let ReadStep::HistoryLine(selection) = step else {
        panic!("counted external edit did not select history first");
    };
    assert_eq!(
        selection.request().position(),
        HistoryPosition::Number(crate::domain::RepeatCount::new(2).unwrap())
    );
    let step = driver
        .resume_history_line(
            &mut editor,
            &selection,
            Ok(HistoryResponse::entry(Text::from("old"))),
        )
        .unwrap();
    let ReadStep::ExternalEdit(external) = step else {
        panic!("history selection did not continue to external editing");
    };
    assert_eq!(external.request().line, Text::from("old"));
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
    let step = driver
        .resume_external_edit(&mut editor, &external, Ok(Text::from("edited")))
        .unwrap();
    let ReadStep::RecordHistory(record) = step else {
        panic!("external editing did not accept the edited line");
    };
    let step = driver
        .resume_history_record(&mut editor, &record, Ok(()))
        .unwrap();
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert!(matches!(
        step,
        ReadStep::Complete(ReadResult::Accepted(ref line)) if line == &Text::from("edited")
    ));
}

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn history_word_effect() {
    let mut editor = vi_with_line("one");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();

    let step = send(&mut driver, &mut editor, begin, &mut output, '$');
    let step = send(&mut driver, &mut editor, step, &mut output, '_');
    let ReadStep::HistoryWord(word) = step else {
        panic!("history-word command did not suspend");
    };
    assert_eq!(word.request().position, HistoryWordPosition::Last);
    let step = driver
        .resume_history_word(
            &mut editor,
            &word,
            Ok(HistoryWordResponse::Word(Text::from("two"))),
        )
        .unwrap();
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("one two"));
    assert_eq!(editor.keymap_mode(), KeymapMode::ViInsert);
}

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn command_failure_is_typed() {
    let mut editor = editor(EditorConfig::default());
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("q").unwrap(),
        Binding::Effect(EffectCommand::ReadEditorCommand),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, 'q');
    let ReadStep::EditorCommand(command) = step else {
        panic!("editor command did not suspend");
    };
    assert_eq!(command.request().prompt, Text::from("\n: "));
    let error = driver.resume_editor_command(
        &mut editor,
        &command,
        Err(HostFailure::Failed("parse failed".into())),
    );
    assert!(matches!(
        error,
        Err(DriverError::Host(HostFailure::Failed(ref message)))
            if message.as_ref() == "parse failed"
    ));
    assert_eq!(editor.terminal_mode(), TerminalMode::Cooked);
}

// [spec:nshedit:req:abi.binding-dispatch/test]
#[test]
fn immediate_insertion_and_word_duplication() {
    let mut first_editor = editor(EditorConfig::default());
    first_editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("2").unwrap(),
        Binding::Sequence(CommandSequence::Argument(ArgumentCommand::StartDigit)),
    );
    first_editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("q").unwrap(),
        Binding::Immediate(ImmediateCommand::InsertInvoking),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut first_editor).unwrap();
    let step = send(&mut driver, &mut first_editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut first_editor, step, &mut output, 'q');
    let _pending = read(settle(&mut driver, &mut first_editor, step, &mut output));
    assert_eq!(first_editor.line(), &Text::from("qq"));

    let mut second_editor = editor(EditorConfig::default());
    second_editor
        .execute(Action::Insert(Text::from("one two")))
        .unwrap();
    second_editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("2").unwrap(),
        Binding::Sequence(CommandSequence::Argument(ArgumentCommand::StartDigit)),
    );
    second_editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("q").unwrap(),
        Binding::Immediate(ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Duplicate,
        }),
    );
    let mut driver = ReadDriver::default();
    let begin = driver.begin(&mut second_editor).unwrap();
    let step = send(&mut driver, &mut second_editor, begin, &mut output, '2');
    let step = send(&mut driver, &mut second_editor, step, &mut output, 'q');
    let _pending = read(settle(&mut driver, &mut second_editor, step, &mut output));
    assert_eq!(second_editor.line(), &Text::from("one twoone two"));
}

// [spec:nshedit:req:abi.binding-dispatch/test]
#[test]
fn vi_immediate_motions_compose_with_operators() {
    let mut editor = vi_with_line("foo bar");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, 'e');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.cursor().get(), 2);

    let mut editor = vi_with_line("foo bar");
    let mut driver = ReadDriver::default();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, 'd');
    let step = send(&mut driver, &mut editor, step, &mut output, 'e');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from(" bar"));

    let mut editor = vi_with_line("a(bc)d");
    let mut driver = ReadDriver::default();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, '%');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.cursor().get(), 4);

    let mut editor = vi_with_line("abcdef");
    let mut driver = ReadDriver::default();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, '5');
    let step = send(&mut driver, &mut editor, step, &mut output, '|');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.cursor().get(), 4);
}

// [spec:nshedit:req:abi.binding-dispatch/test]
#[test]
fn vi_immediate_terminal_outcomes_are_typed() {
    let mut editor = vi_with_line("");
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, '\u{4}');
    let step = settle(&mut driver, &mut editor, step, &mut output);
    assert!(matches!(step, ReadStep::Complete(ReadResult::EndOfInput)));
    assert!(output.ends_with(b"^D\n"));

    let mut editor = vi_with_line("echo ok");
    let mut driver = ReadDriver::default();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, '#');
    let ReadStep::RecordHistory(record) = step else {
        panic!("comment command did not accept the line");
    };
    assert_eq!(record.request().line, Text::from("#echo ok"));
}

// [spec:nshedit:req:abi.binding-dispatch/test]
#[test]
fn sequence_lead_in_dispatches_next_unit() {
    let mut editor = editor(EditorConfig::default());
    editor.bind(
        KeymapMode::Emacs,
        KeySequence::try_from("q").unwrap(),
        Binding::Immediate(ImmediateCommand::KeySequenceLeadIn),
    );
    let mut driver = ReadDriver::default();
    let mut output = Vec::new();
    let begin = driver.begin(&mut editor).unwrap();
    let step = send(&mut driver, &mut editor, begin, &mut output, 'q');
    let step = send(&mut driver, &mut editor, step, &mut output, 'x');
    let _pending = read(settle(&mut driver, &mut editor, step, &mut output));
    assert_eq!(editor.line(), &Text::from("x"));
    assert!(output.contains(&b'\x07'));
}
