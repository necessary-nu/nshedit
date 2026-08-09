use std::io;

use super::*;
use crate::domain::{
    CommandName, CommandSequence, EditingMode, KeyLookup, NonScalarWide, Refresh, TerminalMode,
    TextUnit, ViOperator, ViSequence, WordKind,
};
use crate::editor::{Editor, TerminalControl};

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

fn editor() -> Editor<TestTerminal> {
    Editor::new(EditorConfig::default(), TestTerminal).unwrap()
}

fn vi_editor() -> Editor<TestTerminal> {
    let config = EditorConfig::default().with_editing_mode(EditingMode::Vi);
    Editor::new(config, TestTerminal).unwrap()
}

fn apply(editor: &mut Editor<TestTerminal>, action: Action) -> Outcome {
    match editor.execute(action).unwrap() {
        CommandStep::Applied(outcome) => outcome,
        step => panic!("expected an applied command, got {step:?}"),
    }
}

fn sequence(value: &str) -> KeySequence {
    KeySequence::try_from(value).unwrap()
}

// [spec:nshedit:req:core.line-commands/test]
#[test]
fn insertion_modes_use_checked_boundaries() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("abcd")));
    let index = editor.line().index(1).unwrap();
    apply(&mut editor, Action::Move(Motion::Absolute(index)));
    apply(&mut editor, Action::SetInputMode(InputMode::Replace));
    apply(&mut editor, Action::Insert(Text::from("XY")));
    assert_eq!(editor.line(), &Text::from("aXYd"));
    assert_eq!(editor.cursor().get(), 3);

    let index = editor.line().index(1).unwrap();
    apply(&mut editor, Action::Move(Motion::Absolute(index)));
    apply(&mut editor, Action::SetInputMode(InputMode::ReplaceOnce));
    apply(&mut editor, Action::Insert(Text::from("!")));
    apply(&mut editor, Action::Insert(Text::from("?")));
    assert_eq!(editor.line(), &Text::from("a!?Yd"));
    assert_eq!(editor.input_mode(), InputMode::Insert);
}

#[test]
fn motions_preserve_word_and_line_semantics() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("one, two\nx")));
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    assert_eq!(
        apply(
            &mut editor,
            Action::Move(Motion::Word {
                direction: Direction::Next,
                kind: WordKind::Word,
            }),
        ),
        Outcome::CursorMoved(editor.line().index(3).unwrap())
    );
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    assert_eq!(
        apply(
            &mut editor,
            Action::Move(Motion::Word {
                direction: Direction::Next,
                kind: WordKind::BigWord,
            }),
        ),
        Outcome::CursorMoved(editor.line().index(5).unwrap())
    );

    let index = editor.line().index(2).unwrap();
    apply(&mut editor, Action::Move(Motion::Absolute(index)));
    apply(&mut editor, Action::Move(Motion::Line(Direction::Next)));
    assert_eq!(editor.cursor().get(), editor.line().len());
    apply(&mut editor, Action::Move(Motion::Line(Direction::Previous)));
    assert_eq!(editor.cursor().get(), 1);
}

#[test]
fn non_unicode_units_keep_word_class() {
    let mut editor = editor();
    let text: Text = [
        TextUnit::RawByte(0xff),
        TextUnit::Scalar('.'),
        TextUnit::Scalar(' '),
        TextUnit::CompatibilityWide(NonScalarWide::new(0xd800).unwrap()),
    ]
    .into_iter()
    .collect();
    apply(&mut editor, Action::Insert(text));
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    apply(
        &mut editor,
        Action::Move(Motion::Word {
            direction: Direction::Next,
            kind: WordKind::Word,
        }),
    );
    assert_eq!(editor.cursor().get(), 3);
    apply(
        &mut editor,
        Action::Move(Motion::Word {
            direction: Direction::Next,
            kind: WordKind::Word,
        }),
    );
    assert_eq!(editor.cursor().get(), 4);
}

#[test]
fn register_and_mark_own_text() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("alpha beta")));
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    let mark = editor.line().index(5).unwrap();
    apply(&mut editor, Action::Move(Motion::Absolute(mark)));
    apply(&mut editor, Action::SetMark);
    apply(&mut editor, Action::Move(Motion::EndOfBuffer));
    apply(&mut editor, Action::Copy(EditTarget::MarkedRegion));
    assert_eq!(editor.kill_buffer(), Some(&Text::from(" beta")));

    apply(&mut editor, Action::Kill(EditTarget::MarkedRegion));
    assert_eq!(editor.line(), &Text::from("alpha"));
    assert_eq!(editor.mark(), Some(editor.line().index(5).unwrap()));
    apply(&mut editor, Action::Yank(YankPlacement::AtCursor));
    assert_eq!(editor.line(), &Text::from("alpha beta"));
    apply(&mut editor, Action::ExchangeMark);
    assert_eq!(editor.cursor().get(), 5);
}

#[test]
fn yank_uses_semantic_placement() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("Xab")));
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    apply(
        &mut editor,
        Action::Copy(EditTarget::Character(Direction::Next)),
    );
    apply(
        &mut editor,
        Action::Delete(EditTarget::Character(Direction::Next)),
    );
    apply(&mut editor, Action::Yank(YankPlacement::AfterCursor));
    assert_eq!(editor.line(), &Text::from("aXb"));
    assert_eq!(editor.cursor().get(), 2);
}

#[test]
fn undo_is_atomic_and_clears_redo() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("a")));
    apply(&mut editor, Action::Insert(Text::from("b")));
    apply(
        &mut editor,
        Action::Delete(EditTarget::Character(Direction::Next)),
    );
    assert!(editor.can_undo());

    apply(&mut editor, Action::Undo);
    assert_eq!(editor.line(), &Text::from("a"));
    assert!(editor.can_redo());
    apply(&mut editor, Action::Redo);
    assert_eq!(editor.line(), &Text::from("ab"));
    apply(&mut editor, Action::Undo);
    apply(&mut editor, Action::Insert(Text::from("c")));
    assert_eq!(editor.line(), &Text::from("ac"));
    assert!(!editor.can_redo());
}

#[test]
fn replacement_and_reset_are_session_operations() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("abcdef")));
    let span = editor.line().span(2..4).unwrap();
    editor.replace(span, Text::from("XY")).unwrap();
    assert_eq!(editor.line(), &Text::from("abXYef"));
    assert_eq!(editor.cursor().get(), 4);
    assert!(editor.can_undo());

    editor.reset_line();
    assert!(editor.line().is_empty());
    assert_eq!(editor.cursor(), TextIndex::START);
    assert!(!editor.can_undo());
    assert!(!editor.can_redo());
}

#[test]
fn transforms_preserve_text_unit_kinds() {
    let mut editor = editor();
    let text: Text = [
        TextUnit::Scalar('ß'),
        TextUnit::RawByte(0xff),
        TextUnit::CompatibilityWide(NonScalarWide::new(0xd800).unwrap()),
    ]
    .into_iter()
    .collect();
    apply(&mut editor, Action::Insert(text));
    apply(
        &mut editor,
        Action::Transform {
            target: EditTarget::Buffer,
            transform: TextTransform::Uppercase,
        },
    );
    let expected: Text = [
        TextUnit::Scalar('S'),
        TextUnit::Scalar('S'),
        TextUnit::RawByte(0xff),
        TextUnit::CompatibilityWide(NonScalarWide::new(0xd800).unwrap()),
    ]
    .into_iter()
    .collect();
    assert_eq!(editor.line(), &expected);

    apply(&mut editor, Action::TransposeCharacters);
    let transposed: Text = [
        TextUnit::Scalar('S'),
        TextUnit::Scalar('S'),
        TextUnit::CompatibilityWide(NonScalarWide::new(0xd800).unwrap()),
        TextUnit::RawByte(0xff),
    ]
    .into_iter()
    .collect();
    assert_eq!(editor.line(), &transposed);
}

#[test]
fn capitalization_and_toggle_are_semantic() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("hELLO")));
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    apply(
        &mut editor,
        Action::Transform {
            target: EditTarget::Buffer,
            transform: TextTransform::Capitalize,
        },
    );
    assert_eq!(editor.line(), &Text::from("Hello"));

    apply(&mut editor, Action::ToggleInputMode);
    assert_eq!(editor.input_mode(), InputMode::Replace);
    apply(&mut editor, Action::ToggleInputMode);
    assert_eq!(editor.input_mode(), InputMode::Insert);
}

#[test]
fn delete_or_eof_is_stateful() {
    let mut editor = editor();
    assert_eq!(
        apply(&mut editor, Action::DeleteOrEndOfInput),
        Outcome::EndOfInput
    );
    apply(&mut editor, Action::Insert(Text::from("a")));
    assert_eq!(
        apply(&mut editor, Action::DeleteOrEndOfInput),
        Outcome::Refresh(Refresh::Beep)
    );
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    assert_eq!(
        apply(&mut editor, Action::DeleteOrEndOfInput),
        Outcome::Continue
    );
    assert!(editor.line().is_empty());
}

#[test]
fn search_remembers_exact_pattern() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("banana")));
    apply(&mut editor, Action::Move(Motion::StartOfBuffer));
    assert_eq!(
        apply(
            &mut editor,
            Action::Search {
                pattern: Text::from("ana"),
                direction: Direction::Next,
            },
        ),
        Outcome::CursorMoved(editor.line().index(1).unwrap())
    );
    assert_eq!(
        apply(&mut editor, Action::RepeatSearch(Direction::Next)),
        Outcome::CursorMoved(editor.line().index(3).unwrap())
    );
    assert_eq!(
        apply(&mut editor, Action::RepeatSearch(Direction::Next)),
        Outcome::Refresh(Refresh::Beep)
    );
    assert_eq!(
        apply(&mut editor, Action::RepeatSearch(Direction::Previous)),
        Outcome::CursorMoved(editor.line().index(1).unwrap())
    );
    assert_eq!(editor.search_pattern(), Some(&Text::from("ana")));
}

#[test]
fn search_rejects_missing_or_empty_patterns() {
    let mut editor = editor();
    assert_eq!(
        editor.execute(Action::RepeatSearch(Direction::Next)),
        Err(Error::SearchPatternNotSet)
    );
    assert_eq!(
        editor.execute(Action::Search {
            pattern: Text::default(),
            direction: Direction::Next,
        }),
        Err(Error::EmptySearchPattern)
    );
}

#[test]
fn keymaps_report_all_match_states() {
    let mut editor = editor();
    assert!(matches!(
        editor.key_binding(&sequence("\u{1}")),
        KeyLookup::Exact(Binding::Action(Action::Move(Motion::StartOfLine)))
    ));
    assert_eq!(
        editor.key_binding(&sequence("\u{1b}")),
        KeyLookup::Ambiguous(&Binding::Sequence(CommandSequence::MetaNext))
    );

    let escape = sequence("\u{1b}");
    editor.bind(
        KeymapMode::Emacs,
        escape.clone(),
        Binding::Macro(Text::from("escape")),
    );
    assert!(matches!(
        editor.key_binding(&escape),
        KeyLookup::Ambiguous(Binding::Macro(text)) if text == &Text::from("escape")
    ));
    assert_eq!(
        editor.unbind(KeymapMode::Emacs, &escape),
        Some(Binding::Macro(Text::from("escape")))
    );
    assert_eq!(editor.key_binding(&escape), KeyLookup::Prefix);
}

#[test]
fn reset_restores_default_bindings() {
    let mut editor = editor();
    let delete = sequence("\u{7f}");
    let capitalise = sequence("\u{1b}c");
    editor.unbind(KeymapMode::Emacs, &delete);
    editor.unbind(KeymapMode::Emacs, &capitalise);
    assert!(editor.binding(KeymapMode::Emacs, &delete).is_none());

    editor.reset_bindings(EditingMode::Emacs);
    assert!(matches!(
        editor.binding(KeymapMode::Emacs, &delete),
        Some(Binding::Action(Action::Delete(EditTarget::Character(
            Direction::Previous
        ))))
    ));
    assert!(matches!(
        editor.binding(KeymapMode::Emacs, &capitalise),
        Some(Binding::Action(Action::Transform {
            transform: TextTransform::Capitalize,
            ..
        }))
    ));
}

#[test]
fn vi_maps_are_mode_typed() {
    let mut editor = vi_editor();
    assert_eq!(editor.keymap_mode(), KeymapMode::ViInsert);
    assert!(matches!(
        editor.key_binding(&sequence("\u{1b}")),
        KeyLookup::Exact(Binding::Sequence(CommandSequence::Vi(
            ViSequence::CommandMode
        )))
    ));
    apply(&mut editor, Action::SetKeymap(KeymapMode::ViCommand));
    assert!(matches!(
        editor.key_binding(&sequence("d")),
        KeyLookup::Exact(Binding::Sequence(CommandSequence::Vi(
            ViSequence::Operator(ViOperator::Delete)
        )))
    ));
}

#[test]
fn host_bound_actions_return_typed_steps() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("echo")));
    assert_eq!(
        editor.execute(Action::AcceptLine).unwrap(),
        CommandStep::Applied(Outcome::Accepted(Text::from("echo")))
    );
    assert_eq!(
        editor.execute(Action::Complete).unwrap(),
        CommandStep::NeedsCompletion
    );
    assert_eq!(
        editor
            .execute(Action::History(Direction::Previous))
            .unwrap(),
        CommandStep::NeedsHistory(Direction::Previous)
    );
    let name = CommandName::new("transpose-words").unwrap();
    assert_eq!(
        editor.execute(Action::User(name.clone())).unwrap(),
        CommandStep::NeedsUserCommand(name)
    );
}

#[test]
fn stale_absolute_positions_are_revalidated() {
    let mut editor = editor();
    apply(&mut editor, Action::Insert(Text::from("abc")));
    let old_end = editor.line().index(3).unwrap();
    apply(&mut editor, Action::Delete(EditTarget::Buffer));
    assert_eq!(
        editor.execute(Action::Move(Motion::Absolute(old_end))),
        Err(Error::TextIndexOutOfBounds { index: 3, len: 0 })
    );
}
