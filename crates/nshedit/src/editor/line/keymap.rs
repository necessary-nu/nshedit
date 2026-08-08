use std::collections::BTreeMap;

use crate::domain::{
    Action, Binding, Direction, EditTarget, InputMode, KeyLookup, KeySequence, KeymapMode, Motion,
    Refresh, Text, WordKind, YankPlacement,
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
        insert(
            map,
            "\u{4}",
            Action::Delete(EditTarget::Character(Direction::Next)),
        );
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
        insert(map, "\u{1b}b", word_motion(Direction::Previous));
        insert(map, "\u{1b}f", word_motion(Direction::Next));
    }

    fn install_vi_insert(&mut self) {
        let map = &mut self.vi_insert;
        insert(
            map,
            "\u{1b}",
            Action::SetModes {
                input: InputMode::Insert,
                keymap: KeymapMode::ViCommand,
            },
        );
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
        insert(map, "\n", Action::AcceptLine);
        insert(map, "\r", Action::AcceptLine);
    }

    fn install_vi_command(&mut self) {
        let map = &mut self.vi_command;
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
        insert(
            map,
            "i",
            Action::SetModes {
                input: InputMode::Insert,
                keymap: KeymapMode::ViInsert,
            },
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
        insert(
            map,
            "R",
            Action::SetModes {
                input: InputMode::Replace,
                keymap: KeymapMode::ViInsert,
            },
        );
        insert(map, "dd", Action::Kill(EditTarget::Line));
        insert(map, "\u{12}", Action::Redo);
        insert(map, "\n", Action::AcceptLine);
        insert(map, "\r", Action::AcceptLine);
    }
}

fn insert(map: &mut BTreeMap<KeySequence, Binding>, sequence: &str, action: Action) {
    let sequence = KeySequence::new(Text::from(sequence))
        .expect("built-in key sequences are statically non-empty");
    map.insert(sequence, Binding::Action(action));
}

fn word_motion(direction: Direction) -> Action {
    Action::Move(Motion::Word {
        direction,
        kind: WordKind::Word,
    })
}
