use std::collections::BTreeMap;

use crate::domain::{
    Action, Binding, CharacterSearch, CharacterSearchLanding, CommandSequence, Direction,
    EditTarget, KeyLookup, KeySequence, KeymapMode, Motion, Refresh, SearchRepetition, Text,
    TextTransform, ViInsertPlacement, ViOperator, ViSequence, ViSubstitution, WordKind,
    YankPlacement,
};

#[derive(Debug)]
pub(super) struct Keymaps {
    emacs: BTreeMap<KeySequence, Binding>,
    vi_insert: BTreeMap<KeySequence, Binding>,
    vi_command: BTreeMap<KeySequence, Binding>,
}

impl Default for Keymaps {
    fn default() -> Self {
        let mut maps = Self {
            emacs: BTreeMap::new(),
            vi_insert: BTreeMap::new(),
            vi_command: BTreeMap::new(),
        };
        maps.install_emacs();
        maps.install_vi_insert();
        maps.install_vi_command();
        maps
    }
}

impl Keymaps {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn clear(&mut self, mode: KeymapMode) {
        self.map_mut(mode).clear();
    }

    pub(super) fn bind(
        &mut self,
        mode: KeymapMode,
        sequence: KeySequence,
        binding: Binding,
    ) -> Option<Binding> {
        self.map_mut(mode).insert(sequence, binding)
    }

    pub(super) fn unbind(&mut self, mode: KeymapMode, sequence: &KeySequence) -> Option<Binding> {
        self.map_mut(mode).remove(sequence)
    }

    pub(super) fn lookup(&self, mode: KeymapMode, sequence: &KeySequence) -> KeyLookup<'_> {
        let map = self.map(mode);
        let binding = map.get(sequence);
        let is_prefix = map
            .keys()
            .any(|candidate| candidate != sequence && candidate.starts_with(sequence));
        match (binding, is_prefix) {
            (Some(binding), true) => KeyLookup::Ambiguous(binding),
            (Some(binding), false) => KeyLookup::Exact(binding),
            (None, true) => KeyLookup::Prefix,
            (None, false) => KeyLookup::Unbound,
        }
    }

    pub(super) fn binding(&self, mode: KeymapMode, sequence: &KeySequence) -> Option<&Binding> {
        self.map(mode).get(sequence)
    }

    pub(super) fn bindings(
        &self,
        mode: KeymapMode,
    ) -> impl Iterator<Item = (&KeySequence, &Binding)> {
        self.map(mode).iter()
    }

    fn map(&self, mode: KeymapMode) -> &BTreeMap<KeySequence, Binding> {
        match mode {
            KeymapMode::Emacs => &self.emacs,
            KeymapMode::ViInsert => &self.vi_insert,
            KeymapMode::ViCommand => &self.vi_command,
        }
    }

    fn map_mut(&mut self, mode: KeymapMode) -> &mut BTreeMap<KeySequence, Binding> {
        match mode {
            KeymapMode::Emacs => &mut self.emacs,
            KeymapMode::ViInsert => &mut self.vi_insert,
            KeymapMode::ViCommand => &mut self.vi_command,
        }
    }

    fn install_emacs(&mut self) {
        let map = &mut self.emacs;
        insert(map, "\u{1}", Action::Move(Motion::StartOfLine));
        insert(
            map,
            "\u{2}",
            Action::Move(Motion::Character(Direction::Previous)),
        );
        insert(map, "\u{4}", Action::DeleteOrEndOfInput);
        insert(map, "\u{5}", Action::Move(Motion::EndOfLine));
        insert(
            map,
            "\u{6}",
            Action::Move(Motion::Character(Direction::Next)),
        );
        insert(
            map,
            "\u{8}",
            Action::Delete(EditTarget::Character(Direction::Previous)),
        );
        insert(
            map,
            "\u{b}",
            Action::Kill(EditTarget::Motion(Motion::EndOfLine)),
        );
        insert(map, "\u{c}", Action::Refresh(Refresh::Full));
        insert(map, "\u{e}", Action::History(Direction::Next));
        insert(map, "\u{10}", Action::History(Direction::Previous));
        insert(map, "\u{14}", Action::TransposeCharacters);
        insert(
            map,
            "\u{15}",
            Action::Kill(EditTarget::Motion(Motion::StartOfLine)),
        );
        insert(
            map,
            "\u{17}",
            Action::Kill(EditTarget::Word {
                direction: Direction::Previous,
                kind: WordKind::Word,
            }),
        );
        insert(map, "\u{19}", Action::Yank(YankPlacement::AtCursor));
        insert(map, "\u{1f}", Action::Undo);
        insert(map, "\t", Action::Complete);
        insert(map, "\n", Action::AcceptLine);
        insert(map, "\r", Action::AcceptLine);
        insert(
            map,
            "\u{7f}",
            Action::Delete(EditTarget::Character(Direction::Previous)),
        );
        insert_sequence(map, "\u{16}", CommandSequence::QuotedInsert);
        insert_sequence(map, "\u{1b}", CommandSequence::MetaNext);
        insert(map, "\u{1b}b", word_motion(Direction::Previous));
        insert(
            map,
            "\u{1b}c",
            Action::Transform {
                target: EditTarget::Word {
                    direction: Direction::Next,
                    kind: WordKind::Word,
                },
                transform: TextTransform::Capitalize,
            },
        );
        insert(map, "\u{1b}f", word_motion(Direction::Next));
        insert(
            map,
            "\u{1b}l",
            Action::Transform {
                target: EditTarget::Word {
                    direction: Direction::Next,
                    kind: WordKind::Word,
                },
                transform: TextTransform::Lowercase,
            },
        );
        insert(
            map,
            "\u{1b}u",
            Action::Transform {
                target: EditTarget::Word {
                    direction: Direction::Next,
                    kind: WordKind::Word,
                },
                transform: TextTransform::Uppercase,
            },
        );
        insert(map, "\u{e2}", word_motion(Direction::Previous));
        insert(map, "\u{e6}", word_motion(Direction::Next));
    }

    fn install_vi_insert(&mut self) {
        let map = &mut self.vi_insert;
        insert_sequence(map, "\u{1b}", CommandSequence::Vi(ViSequence::CommandMode));
        insert(
            map,
            "\u{8}",
            Action::Delete(EditTarget::Character(Direction::Previous)),
        );
        insert(
            map,
            "\u{17}",
            Action::Kill(EditTarget::Word {
                direction: Direction::Previous,
                kind: WordKind::Word,
            }),
        );
        insert_sequence(map, "\u{16}", CommandSequence::QuotedInsert);
        insert(map, "\n", Action::AcceptLine);
        insert(map, "\r", Action::AcceptLine);
    }

    fn install_vi_command(&mut self) {
        let map = &mut self.vi_command;
        insert_sequence(map, "\u{1b}", CommandSequence::MetaNext);
        insert_sequence(map, "\u{16}", CommandSequence::QuotedInsert);
        insert(map, "0", Action::Move(Motion::StartOfLine));
        insert(map, "$", Action::Move(Motion::EndOfLine));
        insert(
            map,
            "b",
            Action::Move(Motion::Word {
                direction: Direction::Previous,
                kind: WordKind::Word,
            }),
        );
        insert(
            map,
            "B",
            Action::Move(Motion::Word {
                direction: Direction::Previous,
                kind: WordKind::BigWord,
            }),
        );
        insert(
            map,
            "h",
            Action::Move(Motion::Character(Direction::Previous)),
        );
        insert_sequence(
            map,
            "i",
            CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::AtCursor)),
        );
        insert_sequence(
            map,
            "a",
            CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::AfterCursor)),
        );
        insert_sequence(
            map,
            "I",
            CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::StartOfLine)),
        );
        insert_sequence(
            map,
            "A",
            CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::EndOfLine)),
        );
        insert(map, "l", Action::Move(Motion::Character(Direction::Next)));
        insert(map, "p", Action::Yank(YankPlacement::AfterCursor));
        insert(map, "u", Action::Undo);
        insert(
            map,
            "w",
            Action::Move(Motion::Word {
                direction: Direction::Next,
                kind: WordKind::Word,
            }),
        );
        insert(
            map,
            "W",
            Action::Move(Motion::Word {
                direction: Direction::Next,
                kind: WordKind::BigWord,
            }),
        );
        insert(
            map,
            "x",
            Action::Delete(EditTarget::Character(Direction::Next)),
        );
        insert(
            map,
            "X",
            Action::Delete(EditTarget::Character(Direction::Previous)),
        );
        insert(
            map,
            "D",
            Action::Kill(EditTarget::Motion(Motion::EndOfLine)),
        );
        insert(map, "P", Action::Yank(YankPlacement::AtCursor));
        insert_sequence(map, "R", CommandSequence::Vi(ViSequence::ReplaceMode));
        insert_sequence(
            map,
            "d",
            CommandSequence::Vi(ViSequence::Operator(ViOperator::Delete)),
        );
        insert_sequence(
            map,
            "c",
            CommandSequence::Vi(ViSequence::Operator(ViOperator::Change)),
        );
        insert_sequence(
            map,
            "y",
            CommandSequence::Vi(ViSequence::Operator(ViOperator::Yank)),
        );
        insert_sequence(map, "r", CommandSequence::Vi(ViSequence::ReplaceNext));
        insert_sequence(
            map,
            "s",
            CommandSequence::Vi(ViSequence::Substitute(ViSubstitution::Characters)),
        );
        insert_sequence(
            map,
            "S",
            CommandSequence::Vi(ViSequence::Substitute(ViSubstitution::Line)),
        );
        insert_sequence(
            map,
            "C",
            CommandSequence::Vi(ViSequence::Substitute(ViSubstitution::ToEndOfLine)),
        );
        insert_sequence(
            map,
            "f",
            character_search(Direction::Next, CharacterSearchLanding::OnTarget),
        );
        insert_sequence(
            map,
            "F",
            character_search(Direction::Previous, CharacterSearchLanding::OnTarget),
        );
        insert_sequence(
            map,
            "t",
            character_search(Direction::Next, CharacterSearchLanding::BeforeTarget),
        );
        insert_sequence(
            map,
            "T",
            character_search(Direction::Previous, CharacterSearchLanding::BeforeTarget),
        );
        insert_sequence(
            map,
            ";",
            CommandSequence::Vi(ViSequence::RepeatCharacterSearch(
                SearchRepetition::SameDirection,
            )),
        );
        insert_sequence(
            map,
            ",",
            CommandSequence::Vi(ViSequence::RepeatCharacterSearch(
                SearchRepetition::OppositeDirection,
            )),
        );
        insert_sequence(map, ".", CommandSequence::Vi(ViSequence::RepeatChange));
        insert(map, "\u{12}", Action::Refresh(Refresh::Redisplay));
        insert(map, "\n", Action::AcceptLine);
        insert(map, "\r", Action::AcceptLine);
    }
}

fn insert(map: &mut BTreeMap<KeySequence, Binding>, sequence: &str, action: Action) {
    let sequence = KeySequence::new(Text::from(sequence))
        .expect("built-in key sequences are statically non-empty");
    map.insert(sequence, Binding::Action(action));
}

fn insert_sequence(map: &mut BTreeMap<KeySequence, Binding>, key: &str, sequence: CommandSequence) {
    let key =
        KeySequence::new(Text::from(key)).expect("built-in key sequences are statically non-empty");
    map.insert(key, Binding::Sequence(sequence));
}

fn character_search(direction: Direction, landing: CharacterSearchLanding) -> CommandSequence {
    CommandSequence::Vi(ViSequence::CharacterSearch(CharacterSearch::new(
        direction, landing,
    )))
}

fn word_motion(direction: Direction) -> Action {
    Action::Move(Motion::Word {
        direction,
        kind: WordKind::Word,
    })
}
