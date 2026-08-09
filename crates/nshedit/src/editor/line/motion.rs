use crate::domain::{
    Direction, EditTarget, Error, Motion, Text, TextIndex, TextSpan, TextUnit, WordKind,
};

pub(super) fn destination(
    line: &Text,
    cursor: TextIndex,
    motion: Motion,
) -> Result<TextIndex, Error> {
    line.index(cursor.get())?;
    let units = line.as_units();
    let position = match motion {
        Motion::Character(Direction::Previous) => cursor.get().saturating_sub(1),
        Motion::Character(Direction::Next) => cursor.get().saturating_add(1).min(line.len()),
        Motion::Word { direction, kind } => word_boundary(units, cursor.get(), direction, kind),
        Motion::Line(direction) => adjacent_line(units, cursor.get(), direction),
        Motion::StartOfLine => line_start(units, cursor.get()),
        Motion::EndOfLine => line_end(units, cursor.get()),
        Motion::StartOfBuffer => 0,
        Motion::EndOfBuffer => line.len(),
        Motion::Absolute(index) => return line.index(index.get()),
    };
    line.index(position)
}

pub(super) fn repeated_destination(
    line: &Text,
    cursor: TextIndex,
    motion: Motion,
    count: usize,
) -> Result<TextIndex, Error> {
    let mut destination = cursor;
    for _ in 0..count {
        let next = self::destination(line, destination, motion)?;
        if next == destination {
            break;
        }
        destination = next;
    }
    Ok(destination)
}

pub(super) fn target_span(
    line: &Text,
    cursor: TextIndex,
    mark: Option<TextIndex>,
    target: EditTarget,
) -> Result<TextSpan, Error> {
    line.index(cursor.get())?;
    let range = match target {
        EditTarget::Character(Direction::Previous) => cursor.get().saturating_sub(1)..cursor.get(),
        EditTarget::Character(Direction::Next) => {
            cursor.get()..cursor.get().saturating_add(1).min(line.len())
        }
        EditTarget::Word { direction, kind } => ordered(
            cursor.get(),
            word_boundary(line.as_units(), cursor.get(), direction, kind),
        ),
        EditTarget::Motion(motion) => {
            ordered(cursor.get(), destination(line, cursor, motion)?.get())
        }
        EditTarget::Line => {
            line_start(line.as_units(), cursor.get())..line_end(line.as_units(), cursor.get())
        }
        EditTarget::Buffer => 0..line.len(),
        EditTarget::Span(span) => span.start().get()..span.end().get(),
        EditTarget::MarkedRegion => {
            let mark = mark.ok_or(Error::MarkNotSet)?;
            line.index(mark.get())?;
            ordered(cursor.get(), mark.get())
        }
    };
    line.span(range)
}

pub(super) fn find_pattern(
    line: &Text,
    pattern: &Text,
    cursor: TextIndex,
    direction: Direction,
    include_cursor: bool,
) -> Option<usize> {
    let units = line.as_units();
    let needle = pattern.as_units();
    if needle.is_empty() || needle.len() > units.len() {
        return None;
    }

    match direction {
        Direction::Next => {
            let first = cursor.get().saturating_add(usize::from(!include_cursor));
            let last = units.len() - needle.len();
            (first..=last).find(|&position| units[position..].starts_with(needle))
        }
        Direction::Previous => {
            let last = (units.len() - needle.len()).min(cursor.get());
            (0..=last)
                .rev()
                .filter(|&position| include_cursor || position < cursor.get())
                .find(|&position| units[position..].starts_with(needle))
        }
    }
}

fn ordered(first: usize, second: usize) -> std::ops::Range<usize> {
    first.min(second)..first.max(second)
}

fn word_boundary(units: &[TextUnit], cursor: usize, direction: Direction, kind: WordKind) -> usize {
    match direction {
        Direction::Previous => previous_word(units, cursor, kind),
        Direction::Next => next_word(units, cursor, kind),
    }
}

fn next_word(units: &[TextUnit], mut position: usize, kind: WordKind) -> usize {
    if position >= units.len() {
        return units.len();
    }
    if !is_space(units[position]) {
        let class = word_class(units[position], kind);
        while position < units.len() && word_class(units[position], kind) == class {
            position += 1;
        }
    }
    while position < units.len() && is_space(units[position]) {
        position += 1;
    }
    position
}

fn previous_word(units: &[TextUnit], mut position: usize, kind: WordKind) -> usize {
    while position > 0 && is_space(units[position - 1]) {
        position -= 1;
    }
    if position == 0 {
        return 0;
    }
    let class = word_class(units[position - 1], kind);
    while position > 0 && word_class(units[position - 1], kind) == class {
        position -= 1;
    }
    position
}

fn adjacent_line(units: &[TextUnit], cursor: usize, direction: Direction) -> usize {
    let start = line_start(units, cursor);
    let column = cursor - start;
    match direction {
        Direction::Previous if start > 0 => {
            let previous_end = start - 1;
            let previous_start = line_start(units, previous_end);
            previous_start + column.min(previous_end - previous_start)
        }
        Direction::Next => {
            let end = line_end(units, cursor);
            if end == units.len() {
                cursor
            } else {
                let next_start = end + 1;
                let next_end = line_end(units, next_start);
                next_start + column.min(next_end - next_start)
            }
        }
        Direction::Previous => cursor,
    }
}

fn line_start(units: &[TextUnit], cursor: usize) -> usize {
    units[..cursor]
        .iter()
        .rposition(|unit| is_newline(*unit))
        .map_or(0, |position| position + 1)
}

fn line_end(units: &[TextUnit], cursor: usize) -> usize {
    units[cursor..]
        .iter()
        .position(|unit| is_newline(*unit))
        .map_or(units.len(), |position| cursor + position)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Space,
    Word,
    Other,
}

fn word_class(unit: TextUnit, kind: WordKind) -> WordClass {
    if is_space(unit) {
        WordClass::Space
    } else if kind == WordKind::BigWord || is_word(unit) {
        WordClass::Word
    } else {
        WordClass::Other
    }
}

fn is_space(unit: TextUnit) -> bool {
    matches!(unit, TextUnit::Scalar(character) if character.is_whitespace())
}

fn is_word(unit: TextUnit) -> bool {
    matches!(unit, TextUnit::Scalar(character) if character.is_alphanumeric() || character == '_')
}

fn is_newline(unit: TextUnit) -> bool {
    unit == TextUnit::Scalar('\n')
}
