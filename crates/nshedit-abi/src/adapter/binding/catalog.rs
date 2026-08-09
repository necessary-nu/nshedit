use nshedit::domain::{
    Action, ArgumentCommand, Binding, CharacterSearch, CharacterSearchLanding, CommandSequence,
    Direction, EditTarget, EffectCommand, HistorySearchCommand, HistorySearchRepetition,
    ImmediateCommand, Motion, Refresh, RepeatCount, SearchRepetition, TextTransform,
    ViInsertPlacement, ViOperator, ViSequence, ViSubstitution, WordKind, WordTraversal,
    YankPlacement,
};

pub(super) struct CommandHelp {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
}

pub(super) struct TerminalKey {
    pub(super) name: &'static str,
    pub(super) capability: &'static str,
    pub(super) fallback: &'static [&'static str],
    pub(super) default_command: &'static str,
}

pub(super) const TERMINAL_KEYS: [TerminalKey; 7] = [
    TerminalKey {
        name: "down",
        capability: "kd",
        fallback: &["\u{1b}[B", "\u{1b}OB"],
        default_command: "ed-next-history",
    },
    TerminalKey {
        name: "up",
        capability: "ku",
        fallback: &["\u{1b}[A", "\u{1b}OA"],
        default_command: "ed-prev-history",
    },
    TerminalKey {
        name: "left",
        capability: "kl",
        fallback: &["\u{1b}[D", "\u{1b}OD"],
        default_command: "ed-prev-char",
    },
    TerminalKey {
        name: "right",
        capability: "kr",
        fallback: &["\u{1b}[C", "\u{1b}OC"],
        default_command: "ed-next-char",
    },
    TerminalKey {
        name: "home",
        capability: "kh",
        fallback: &["\u{1b}[H", "\u{1b}OH"],
        default_command: "ed-move-to-beg",
    },
    TerminalKey {
        name: "end",
        capability: "@7",
        fallback: &["\u{1b}[F", "\u{1b}OF"],
        default_command: "ed-move-to-end",
    },
    TerminalKey {
        name: "delete",
        capability: "kD",
        fallback: &[],
        default_command: "ed-delete-next-char",
    },
];

pub(super) const BUILTIN_COMMANDS: &[CommandHelp] = &[
    command("ed-end-of-file", "Indicate end of file"),
    command("ed-insert", "Add character to the line"),
    command(
        "ed-delete-prev-word",
        "Delete from beginning of current word to cursor",
    ),
    command("ed-delete-next-char", "Delete character under cursor"),
    command("ed-kill-line", "Cut to the end of line"),
    command("ed-move-to-end", "Move cursor to the end of line"),
    command("ed-move-to-beg", "Move cursor to the beginning of line"),
    command(
        "ed-transpose-chars",
        "Exchange the character to the left of the cursor with the one under it",
    ),
    command("ed-next-char", "Move to the right one character"),
    command("ed-prev-word", "Move to the beginning of the current word"),
    command("ed-prev-char", "Move to the left one character"),
    command("ed-quoted-insert", "Add the next character typed verbatim"),
    command("ed-digit", "Adds to argument or enters a digit"),
    command("ed-argument-digit", "Digit that starts argument"),
    command("ed-unassigned", "Indicates unbound character"),
    command("ed-ignore", "Input characters that have no effect"),
    command("ed-newline", "Execute command"),
    command(
        "ed-delete-prev-char",
        "Delete the character to the left of the cursor",
    ),
    command(
        "ed-clear-screen",
        "Clear screen leaving current line at the top",
    ),
    command("ed-redisplay", "Redisplay everything"),
    command("ed-start-over", "Erase current line and start from scratch"),
    command("ed-sequence-lead-in", "First character in a bound sequence"),
    command("ed-prev-history", "Move to the previous history line"),
    command("ed-next-history", "Move to the next history line"),
    command(
        "ed-search-prev-history",
        "Search previous in history for a line matching the current",
    ),
    command(
        "ed-search-next-history",
        "Search next in history for a line matching the current",
    ),
    command("ed-prev-line", "Move up one line"),
    command("ed-next-line", "Move down one line"),
    command("ed-command", "Editline extended command"),
    command(
        "em-delete-or-list",
        "Delete character under cursor or list completions if at end of line",
    ),
    command(
        "em-delete-next-word",
        "Cut from cursor to end of current word",
    ),
    command("em-yank", "Paste cut buffer at cursor position"),
    command("em-kill-line", "Cut the entire line and save in cut buffer"),
    command(
        "em-kill-region",
        "Cut area between mark and cursor and save in cut buffer",
    ),
    command(
        "em-copy-region",
        "Copy area between mark and cursor to cut buffer",
    ),
    command(
        "em-gosmacs-transpose",
        "Exchange the two characters before the cursor",
    ),
    command("em-next-word", "Move next to end of current word"),
    command(
        "em-upper-case",
        "Uppercase the characters from cursor to end of current word",
    ),
    command(
        "em-capitol-case",
        "Capitalize the characters from cursor to end of current word",
    ),
    command(
        "em-lower-case",
        "Lowercase the characters from cursor to end of current word",
    ),
    command("em-set-mark", "Set the mark at cursor"),
    command("em-exchange-mark", "Exchange the cursor and mark"),
    command(
        "em-universal-argument",
        "Universal argument (argument times 4)",
    ),
    command("em-meta-next", "Add 8th bit to next character typed"),
    command(
        "em-toggle-overwrite",
        "Switch from insert to overwrite mode or vice versa",
    ),
    command("em-copy-prev-word", "Copy current word to cursor"),
    command("em-inc-search-next", "Emacs incremental next search"),
    command("em-inc-search-prev", "Emacs incremental reverse search"),
    command(
        "em-delete-prev-char",
        "Delete the character to the left of the cursor",
    ),
    command(
        "vi-paste-next",
        "Vi paste previous deletion to the right of the cursor",
    ),
    command(
        "vi-paste-prev",
        "Vi paste previous deletion to the left of the cursor",
    ),
    command(
        "vi-prev-big-word",
        "Vi move to the previous space delimited word",
    ),
    command("vi-prev-word", "Vi move to the previous word"),
    command(
        "vi-next-big-word",
        "Vi move to the next space delimited word",
    ),
    command("vi-next-word", "Vi move to the next word"),
    command(
        "vi-change-case",
        "Vi change case of character under the cursor and advance one character",
    ),
    command("vi-change-meta", "Vi change prefix command"),
    command(
        "vi-insert-at-bol",
        "Vi enter insert mode at the beginning of line",
    ),
    command(
        "vi-replace-char",
        "Vi replace character under the cursor with the next character typed",
    ),
    command("vi-replace-mode", "Vi enter replace mode"),
    command(
        "vi-substitute-char",
        "Vi replace character under the cursor and enter insert mode",
    ),
    command("vi-substitute-line", "Vi substitute entire line"),
    command("vi-change-to-eol", "Vi change to end of line"),
    command("vi-insert", "Vi enter insert mode"),
    command("vi-add", "Vi enter insert mode after the cursor"),
    command("vi-add-at-eol", "Vi enter insert mode at end of line"),
    command("vi-delete-meta", "Vi delete prefix command"),
    command(
        "vi-end-big-word",
        "Vi move to the end of the current space delimited word",
    ),
    command("vi-end-word", "Vi move to the end of the current word"),
    command("vi-undo", "Vi undo last change"),
    command(
        "vi-command-mode",
        "Vi enter command mode (use alternative key bindings)",
    ),
    command("vi-zero", "Vi move to the beginning of line"),
    command(
        "vi-delete-prev-char",
        "Vi move to previous character (backspace)",
    ),
    command(
        "vi-list-or-eof",
        "Vi list choices for completion or indicate end of file if empty line",
    ),
    command(
        "vi-kill-line-prev",
        "Vi cut from beginning of line to cursor",
    ),
    command("vi-search-prev", "Vi search history previous"),
    command("vi-search-next", "Vi search history next"),
    command(
        "vi-repeat-search-next",
        "Vi repeat current search in the same search direction",
    ),
    command(
        "vi-repeat-search-prev",
        "Vi repeat current search in the opposite search direction",
    ),
    command("vi-next-char", "Vi move to the character specified next"),
    command(
        "vi-prev-char",
        "Vi move to the character specified previous",
    ),
    command(
        "vi-to-next-char",
        "Vi move up to the character specified next",
    ),
    command(
        "vi-to-prev-char",
        "Vi move up to the character specified previous",
    ),
    command(
        "vi-repeat-next-char",
        "Vi repeat current character search in the same search direction",
    ),
    command(
        "vi-repeat-prev-char",
        "Vi repeat current character search in the opposite search direction",
    ),
    command("vi-match", "Vi go to matching () {} or []"),
    command("vi-undo-line", "Vi undo all changes to line"),
    command("vi-to-column", "Vi go to specified column"),
    command("vi-yank-end", "Vi yank to end of line"),
    command("vi-yank", "Vi yank"),
    command("vi-comment-out", "Vi comment out current command"),
    command("vi-alias", "Vi include shell alias"),
    command(
        "vi-to-history-line",
        "Vi go to specified history file line.",
    ),
    command("vi-histedit", "Vi edit history line with vi"),
    command("vi-history-word", "Vi append word from previous input line"),
    command("vi-redo", "Vi redo last non-motion command"),
];

const fn command(name: &'static str, description: &'static str) -> CommandHelp {
    CommandHelp { name, description }
}

pub(super) fn is_builtin(name: &str) -> bool {
    BUILTIN_COMMANDS.iter().any(|command| command.name == name)
}

pub(super) fn named_binding(name: &str) -> Option<Binding> {
    named_sequence(name)
        .map(Binding::Sequence)
        .or_else(|| named_effect(name).map(Binding::Effect))
        .or_else(|| named_immediate(name).map(Binding::Immediate))
        .or_else(|| named_action(name).map(Binding::Action))
}

fn named_immediate(name: &str) -> Option<ImmediateCommand> {
    match name {
        "ed-insert" => Some(ImmediateCommand::InsertInvoking),
        "ed-sequence-lead-in" => Some(ImmediateCommand::KeySequenceLeadIn),
        "ed-prev-word" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Move,
        }),
        "ed-delete-prev-word" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Kill,
        }),
        "em-delete-next-word" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Kill,
        }),
        "em-next-word" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Move,
        }),
        "em-copy-prev-word" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Duplicate,
        }),
        "em-upper-case" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Transform(TextTransform::Uppercase),
        }),
        "em-capitol-case" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Transform(TextTransform::Capitalize),
        }),
        "em-lower-case" => Some(ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Transform(TextTransform::Lowercase),
        }),
        "vi-end-word" => Some(ImmediateCommand::EndOfWord(WordKind::Word)),
        "vi-end-big-word" => Some(ImmediateCommand::EndOfWord(WordKind::BigWord)),
        "vi-list-or-eof" => Some(ImmediateCommand::EndOfInputIfEmpty),
        "vi-match" => Some(ImmediateCommand::MatchDelimiter),
        "vi-to-column" => Some(ImmediateCommand::MoveToColumn),
        "vi-comment-out" => Some(ImmediateCommand::CommentAndAccept),
        "em-gosmacs-transpose" => Some(ImmediateCommand::TransposeBeforeCursor),
        "vi-delete-prev-char" => Some(ImmediateCommand::DeletePreviousUnit),
        "vi-paste-prev" => Some(ImmediateCommand::PasteRegister(YankPlacement::AtCursor)),
        "vi-paste-next" => Some(ImmediateCommand::PasteRegister(YankPlacement::AfterCursor)),
        "vi-change-case" => Some(ImmediateCommand::ToggleCaseAndAdvance),
        "vi-zero" => Some(ImmediateCommand::StartOfLineOrArgument),
        "vi-undo" => Some(ImmediateCommand::UndoRequired),
        "em-delete-or-list" => Some(ImmediateCommand::DeleteFollowingOrEndOfInput),
        _ => None,
    }
}

fn named_effect(name: &str) -> Option<EffectCommand> {
    let search = |command| EffectCommand::SearchHistory(command);
    match name {
        "ed-search-prev-history" => Some(search(HistorySearchCommand::Prefix(Direction::Previous))),
        "ed-search-next-history" => Some(search(HistorySearchCommand::Prefix(Direction::Next))),
        "em-inc-search-prev" => Some(search(HistorySearchCommand::Incremental(
            Direction::Previous,
        ))),
        "em-inc-search-next" => Some(search(HistorySearchCommand::Incremental(Direction::Next))),
        "vi-search-prev" => Some(search(HistorySearchCommand::Prompt(Direction::Previous))),
        "vi-search-next" => Some(search(HistorySearchCommand::Prompt(Direction::Next))),
        "vi-repeat-search-next" => Some(search(HistorySearchCommand::Repeat(
            HistorySearchRepetition::SameDirection,
        ))),
        "vi-repeat-search-prev" => Some(search(HistorySearchCommand::Repeat(
            HistorySearchRepetition::OppositeDirection,
        ))),
        "vi-alias" => Some(EffectCommand::ExpandAlias),
        "vi-to-history-line" => Some(EffectCommand::SelectHistoryLine),
        "vi-undo-line" => Some(EffectCommand::RestoreHistoryLine),
        "vi-history-word" => Some(EffectCommand::InsertHistoryWord),
        "ed-command" => Some(EffectCommand::ReadEditorCommand),
        "vi-histedit" => Some(EffectCommand::EditHistory),
        _ => None,
    }
}

fn named_sequence(name: &str) -> Option<CommandSequence> {
    let vi = |sequence| CommandSequence::Vi(sequence);
    let search = |direction, landing| {
        vi(ViSequence::CharacterSearch(CharacterSearch::new(
            direction, landing,
        )))
    };
    match name {
        "ed-quoted-insert" => Some(CommandSequence::QuotedInsert),
        "ed-digit" => Some(CommandSequence::Argument(ArgumentCommand::DigitOrInsert)),
        "ed-argument-digit" => Some(CommandSequence::Argument(ArgumentCommand::StartDigit)),
        "em-universal-argument" => Some(CommandSequence::Argument(ArgumentCommand::Multiply(
            RepeatCount::new(4).expect("four is non-zero"),
        ))),
        "em-meta-next" => Some(CommandSequence::MetaNext),
        "vi-change-meta" => Some(vi(ViSequence::Operator(ViOperator::Change))),
        "vi-delete-meta" => Some(vi(ViSequence::Operator(ViOperator::Delete))),
        "vi-yank" => Some(vi(ViSequence::Operator(ViOperator::Yank))),
        "vi-insert" => Some(vi(ViSequence::Insert(ViInsertPlacement::AtCursor))),
        "vi-add" => Some(vi(ViSequence::Insert(ViInsertPlacement::AfterCursor))),
        "vi-insert-at-bol" => Some(vi(ViSequence::Insert(ViInsertPlacement::StartOfLine))),
        "vi-add-at-eol" => Some(vi(ViSequence::Insert(ViInsertPlacement::EndOfLine))),
        "vi-command-mode" => Some(vi(ViSequence::CommandMode)),
        "vi-replace-char" => Some(vi(ViSequence::ReplaceNext)),
        "vi-replace-mode" => Some(vi(ViSequence::ReplaceMode)),
        "vi-substitute-char" => Some(vi(ViSequence::Substitute(ViSubstitution::Characters))),
        "vi-substitute-line" => Some(vi(ViSequence::Substitute(ViSubstitution::Line))),
        "vi-change-to-eol" => Some(vi(ViSequence::Substitute(ViSubstitution::ToEndOfLine))),
        "vi-next-char" => Some(search(Direction::Next, CharacterSearchLanding::OnTarget)),
        "vi-prev-char" => Some(search(
            Direction::Previous,
            CharacterSearchLanding::OnTarget,
        )),
        "vi-to-next-char" => Some(search(
            Direction::Next,
            CharacterSearchLanding::BeforeTarget,
        )),
        "vi-to-prev-char" => Some(search(
            Direction::Previous,
            CharacterSearchLanding::BeforeTarget,
        )),
        "vi-repeat-next-char" => Some(vi(ViSequence::RepeatCharacterSearch(
            SearchRepetition::SameDirection,
        ))),
        "vi-repeat-prev-char" => Some(vi(ViSequence::RepeatCharacterSearch(
            SearchRepetition::OppositeDirection,
        ))),
        "vi-redo" => Some(vi(ViSequence::RepeatChange)),
        _ => None,
    }
}

pub(super) fn named_action(name: &str) -> Option<Action> {
    named_common_action(name).or_else(|| named_vi_action(name))
}

fn named_common_action(name: &str) -> Option<Action> {
    let word_motion = |direction| {
        Action::Move(Motion::Word {
            direction,
            kind: WordKind::Word,
        })
    };
    let big_word_motion = |direction| {
        Action::Move(Motion::Word {
            direction,
            kind: WordKind::BigWord,
        })
    };
    match name {
        "ed-end-of-file" => Some(Action::EndOfInput),
        "ed-delete-next-char" => Some(Action::Delete(EditTarget::Character(Direction::Next))),
        "ed-kill-line" => Some(Action::Kill(EditTarget::Motion(Motion::EndOfBuffer))),
        "ed-move-to-end" => Some(Action::Move(Motion::EndOfBuffer)),
        "ed-move-to-beg" => Some(Action::Move(Motion::StartOfBuffer)),
        "ed-transpose-chars" => Some(Action::TransposeCharacters),
        "ed-next-char" => Some(Action::Move(Motion::Character(Direction::Next))),
        "ed-prev-char" => Some(Action::Move(Motion::Character(Direction::Previous))),
        "ed-unassigned" => Some(Action::Refresh(Refresh::Beep)),
        "ed-ignore" => Some(Action::Noop),
        "ed-newline" => Some(Action::AcceptLine),
        "ed-delete-prev-char" | "em-delete-prev-char" => {
            Some(Action::Delete(EditTarget::Character(Direction::Previous)))
        }
        "ed-clear-screen" => Some(Action::Refresh(Refresh::Full)),
        "ed-redisplay" => Some(Action::Refresh(Refresh::Redisplay)),
        "ed-start-over" => Some(Action::Delete(EditTarget::Buffer)),
        "ed-prev-history" => Some(Action::History(Direction::Previous)),
        "ed-next-history" => Some(Action::History(Direction::Next)),
        "ed-prev-line" => Some(Action::Move(Motion::Line(Direction::Previous))),
        "ed-next-line" => Some(Action::Move(Motion::Line(Direction::Next))),
        "em-yank" => Some(Action::Yank(YankPlacement::AtCursor)),
        "em-kill-line" => Some(Action::Kill(EditTarget::Buffer)),
        "em-kill-region" => Some(Action::Kill(EditTarget::MarkedRegion)),
        "em-copy-region" => Some(Action::Copy(EditTarget::MarkedRegion)),
        "vi-next-word" => Some(word_motion(Direction::Next)),
        "vi-next-big-word" => Some(big_word_motion(Direction::Next)),
        "vi-prev-word" => Some(word_motion(Direction::Previous)),
        "vi-prev-big-word" => Some(big_word_motion(Direction::Previous)),
        "em-set-mark" => Some(Action::SetMark),
        "em-exchange-mark" => Some(Action::ExchangeMark),
        "em-toggle-overwrite" => Some(Action::ToggleInputMode),
        _ => None,
    }
}

fn named_vi_action(name: &str) -> Option<Action> {
    match name {
        "vi-kill-line-prev" => Some(Action::Kill(EditTarget::Motion(Motion::StartOfLine))),
        "vi-yank-end" => Some(Action::Copy(EditTarget::Motion(Motion::EndOfLine))),
        _ => None,
    }
}

pub(super) fn sequence_name(sequence: CommandSequence) -> &'static str {
    match sequence {
        CommandSequence::Argument(ArgumentCommand::DigitOrInsert) => "ed-digit",
        CommandSequence::Argument(ArgumentCommand::StartDigit) => "ed-argument-digit",
        CommandSequence::Argument(ArgumentCommand::Multiply(_)) => "em-universal-argument",
        CommandSequence::QuotedInsert => "ed-quoted-insert",
        CommandSequence::MetaNext => "em-meta-next",
        CommandSequence::Vi(ViSequence::Operator(ViOperator::Delete)) => "vi-delete-meta",
        CommandSequence::Vi(ViSequence::Operator(ViOperator::Change)) => "vi-change-meta",
        CommandSequence::Vi(ViSequence::Operator(ViOperator::Yank)) => "vi-yank",
        CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::AtCursor)) => "vi-insert",
        CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::AfterCursor)) => "vi-add",
        CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::StartOfLine)) => {
            "vi-insert-at-bol"
        }
        CommandSequence::Vi(ViSequence::Insert(ViInsertPlacement::EndOfLine)) => "vi-add-at-eol",
        CommandSequence::Vi(ViSequence::CommandMode) => "vi-command-mode",
        CommandSequence::Vi(ViSequence::ReplaceNext) => "vi-replace-char",
        CommandSequence::Vi(ViSequence::ReplaceMode) => "vi-replace-mode",
        CommandSequence::Vi(ViSequence::Substitute(ViSubstitution::Characters)) => {
            "vi-substitute-char"
        }
        CommandSequence::Vi(ViSequence::Substitute(ViSubstitution::Line)) => "vi-substitute-line",
        CommandSequence::Vi(ViSequence::Substitute(ViSubstitution::ToEndOfLine)) => {
            "vi-change-to-eol"
        }
        CommandSequence::Vi(ViSequence::CharacterSearch(search)) => {
            match (search.direction(), search.landing()) {
                (Direction::Next, CharacterSearchLanding::OnTarget) => "vi-next-char",
                (Direction::Previous, CharacterSearchLanding::OnTarget) => "vi-prev-char",
                (Direction::Next, CharacterSearchLanding::BeforeTarget) => "vi-to-next-char",
                (Direction::Previous, CharacterSearchLanding::BeforeTarget) => "vi-to-prev-char",
            }
        }
        CommandSequence::Vi(ViSequence::RepeatCharacterSearch(SearchRepetition::SameDirection)) => {
            "vi-repeat-next-char"
        }
        CommandSequence::Vi(ViSequence::RepeatCharacterSearch(
            SearchRepetition::OppositeDirection,
        )) => "vi-repeat-prev-char",
        CommandSequence::Vi(ViSequence::RepeatChange) => "vi-redo",
    }
}

pub(super) fn effect_name(command: EffectCommand) -> &'static str {
    match command {
        EffectCommand::SearchHistory(HistorySearchCommand::Prefix(Direction::Previous)) => {
            "ed-search-prev-history"
        }
        EffectCommand::SearchHistory(HistorySearchCommand::Prefix(Direction::Next)) => {
            "ed-search-next-history"
        }
        EffectCommand::SearchHistory(HistorySearchCommand::Prompt(Direction::Previous)) => {
            "vi-search-prev"
        }
        EffectCommand::SearchHistory(HistorySearchCommand::Prompt(Direction::Next)) => {
            "vi-search-next"
        }
        EffectCommand::SearchHistory(HistorySearchCommand::Incremental(Direction::Previous)) => {
            "em-inc-search-prev"
        }
        EffectCommand::SearchHistory(HistorySearchCommand::Incremental(Direction::Next)) => {
            "em-inc-search-next"
        }
        EffectCommand::SearchHistory(HistorySearchCommand::Repeat(
            HistorySearchRepetition::SameDirection,
        )) => "vi-repeat-search-next",
        EffectCommand::SearchHistory(HistorySearchCommand::Repeat(
            HistorySearchRepetition::OppositeDirection,
        )) => "vi-repeat-search-prev",
        EffectCommand::ExpandAlias => "vi-alias",
        EffectCommand::SelectHistoryLine => "vi-to-history-line",
        EffectCommand::RestoreHistoryLine => "vi-undo-line",
        EffectCommand::InsertHistoryWord => "vi-history-word",
        EffectCommand::ReadEditorCommand => "ed-command",
        EffectCommand::EditHistory => "vi-histedit",
    }
}

pub(super) const fn immediate_name(command: ImmediateCommand) -> &'static str {
    match command {
        ImmediateCommand::InsertInvoking => "ed-insert",
        ImmediateCommand::KeySequenceLeadIn => "ed-sequence-lead-in",
        ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Move,
        } => "ed-prev-word",
        ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Kill,
        } => "ed-delete-prev-word",
        ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Kill,
        } => "em-delete-next-word",
        ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Move,
        } => "em-next-word",
        ImmediateCommand::TraverseWords {
            direction: Direction::Previous,
            operation: WordTraversal::Duplicate,
        } => "em-copy-prev-word",
        ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Transform(TextTransform::Uppercase),
        } => "em-upper-case",
        ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Transform(TextTransform::Capitalize),
        } => "em-capitol-case",
        ImmediateCommand::TraverseWords {
            direction: Direction::Next,
            operation: WordTraversal::Transform(TextTransform::Lowercase),
        } => "em-lower-case",
        ImmediateCommand::TraverseWords { .. } => "ed-unassigned",
        ImmediateCommand::EndOfWord(WordKind::Word) => "vi-end-word",
        ImmediateCommand::EndOfWord(WordKind::BigWord) => "vi-end-big-word",
        ImmediateCommand::EndOfInputIfEmpty => "vi-list-or-eof",
        ImmediateCommand::MatchDelimiter => "vi-match",
        ImmediateCommand::MoveToColumn => "vi-to-column",
        ImmediateCommand::CommentAndAccept => "vi-comment-out",
        ImmediateCommand::TransposeBeforeCursor => "em-gosmacs-transpose",
        ImmediateCommand::DeletePreviousUnit => "vi-delete-prev-char",
        ImmediateCommand::PasteRegister(YankPlacement::AtCursor) => "vi-paste-prev",
        ImmediateCommand::PasteRegister(YankPlacement::AfterCursor) => "vi-paste-next",
        ImmediateCommand::ToggleCaseAndAdvance => "vi-change-case",
        ImmediateCommand::StartOfLineOrArgument => "vi-zero",
        ImmediateCommand::UndoRequired => "vi-undo",
        ImmediateCommand::DeleteFollowingOrEndOfInput => "em-delete-or-list",
    }
}

pub(super) fn action_name(action: &Action) -> Option<&str> {
    match action {
        Action::Noop => Some("ed-ignore"),
        Action::Insert(_) => Some("ed-insert"),
        Action::Move(Motion::StartOfLine | Motion::StartOfBuffer) => Some("ed-move-to-beg"),
        Action::Move(Motion::EndOfLine | Motion::EndOfBuffer) => Some("ed-move-to-end"),
        Action::Move(Motion::Character(Direction::Previous)) => Some("ed-prev-char"),
        Action::Move(Motion::Character(Direction::Next)) => Some("ed-next-char"),
        Action::Move(Motion::Word {
            direction: Direction::Previous,
            kind: WordKind::Word,
        }) => Some("ed-prev-word"),
        Action::Move(Motion::Word {
            direction: Direction::Next,
            kind: WordKind::Word,
        }) => Some("em-next-word"),
        Action::Delete(EditTarget::Character(Direction::Next)) => Some("ed-delete-next-char"),
        Action::Delete(EditTarget::Character(Direction::Previous)) => Some("ed-delete-prev-char"),
        Action::Delete(EditTarget::Buffer) => Some("ed-start-over"),
        Action::DeleteOrEndOfInput => Some("em-delete-or-list"),
        Action::Kill(EditTarget::Motion(Motion::EndOfLine | Motion::EndOfBuffer)) => {
            Some("ed-kill-line")
        }
        Action::Kill(EditTarget::Buffer) => Some("em-kill-line"),
        Action::Kill(EditTarget::MarkedRegion) => Some("em-kill-region"),
        Action::Copy(EditTarget::MarkedRegion) => Some("em-copy-region"),
        Action::Yank(YankPlacement::AtCursor) => Some("em-yank"),
        Action::TransposeCharacters => Some("ed-transpose-chars"),
        Action::Transform {
            transform: TextTransform::Uppercase,
            ..
        } => Some("em-upper-case"),
        Action::Transform {
            transform: TextTransform::Lowercase,
            ..
        } => Some("em-lower-case"),
        Action::Transform {
            transform: TextTransform::Capitalize,
            ..
        } => Some("em-capitol-case"),
        Action::Transform {
            transform: TextTransform::ToggleCase,
            ..
        } => Some("vi-change-case"),
        Action::SetMark => Some("em-set-mark"),
        Action::ExchangeMark => Some("em-exchange-mark"),
        Action::ToggleInputMode => Some("em-toggle-overwrite"),
        Action::AcceptLine => Some("ed-newline"),
        Action::EndOfInput => Some("ed-end-of-file"),
        Action::Complete => Some("em-delete-or-list"),
        Action::History(Direction::Previous) => Some("ed-prev-history"),
        Action::History(Direction::Next) => Some("ed-next-history"),
        Action::Undo => Some("vi-undo"),
        Action::Refresh(Refresh::Full) => Some("ed-clear-screen"),
        Action::Refresh(Refresh::Redisplay) => Some("ed-redisplay"),
        Action::Refresh(Refresh::Beep) => Some("ed-unassigned"),
        _ => None,
    }
}
