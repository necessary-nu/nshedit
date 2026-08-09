use unicode_width::UnicodeWidthChar;

use crate::domain::{
    Error, Prompt, PromptPart, Screen, ScreenCell, ScreenGlyph, ScreenPosition, ScreenSize, Text,
    TextIndex, TextUnit,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Atom {
    Glyph(String),
    Literal(Box<[u8]>),
    Spaces(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Row {
    pub(super) atoms: Vec<Atom>,
    cells: Vec<ScreenCell>,
    used: usize,
}

impl Row {
    fn new(columns: usize) -> Self {
        Self {
            atoms: Vec::new(),
            cells: vec![ScreenCell::Blank; columns],
            used: 0,
        }
    }

    pub(super) const fn used(&self) -> usize {
        self.used
    }

    fn pad_to(&mut self, column: usize) {
        if column > self.used {
            self.atoms.push(Atom::Spaces(column - self.used));
            self.used = column;
        }
    }

    fn append_at(&mut self, column: usize, other: &Self) {
        self.pad_to(column);
        self.atoms.extend(other.atoms.iter().cloned());
        for (offset, cell) in other.cells[..other.used].iter().enumerate() {
            self.cells[column + offset] = cell.clone();
        }
        self.used = column + other.used;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Frame {
    pub(super) screen: Screen,
    pub(super) cursor: ScreenPosition,
    pub(super) rows: Vec<Row>,
}

impl Frame {
    pub(super) fn rows_used(&self) -> usize {
        self.rows.len()
    }
}

#[derive(Debug, Clone, Copy)]
struct Anchor {
    row: usize,
    column: usize,
    atom: usize,
}

struct Builder {
    columns: usize,
    rows: Vec<Row>,
    row: usize,
    column: usize,
    anchor: Option<Anchor>,
}

impl Builder {
    fn new(columns: usize) -> Self {
        Self {
            columns,
            rows: vec![Row::new(columns)],
            row: 0,
            column: 0,
            anchor: None,
        }
    }

    fn position(&self) -> (usize, usize) {
        (self.row, self.column)
    }

    fn put_prompt(&mut self, prompt: &Prompt) {
        for part in prompt.parts() {
            match part {
                PromptPart::Text(text) => self.put_text(text),
                PromptPart::Literal(literal) => {
                    self.rows[self.row]
                        .atoms
                        .push(Atom::Literal(literal.as_bytes().into()));
                }
            }
        }
    }

    fn put_text(&mut self, text: &Text) {
        for &unit in text.as_units() {
            self.put_unit(unit);
        }
    }

    fn put_unit(&mut self, unit: TextUnit) {
        match unit {
            TextUnit::Scalar('\n') => self.next_row(),
            TextUnit::Scalar('\t') => {
                let spaces = 8 - self.column % 8;
                self.put_spaces(spaces);
            }
            TextUnit::Scalar(character) => self.put_scalar(character),
            TextUnit::RawByte(byte) => self.put_escape(&format!("\\x{byte:02X}")),
            TextUnit::OpaqueCodePoint(value) => {
                self.put_escape(&format!("\\u{{{:X}}}", value.get()));
            }
        }
    }

    fn put_scalar(&mut self, character: char) {
        match character.width() {
            Some(0) if self.append_to_anchor(character) => {}
            Some(0) | None => self.put_escape(&visible_scalar(character)),
            Some(width) if width <= self.columns => self.put_glyph(character, width),
            Some(_) => self.put_escape(&visible_scalar(character)),
        }
    }

    fn put_escape(&mut self, visible: &str) {
        for character in visible.chars() {
            self.put_glyph(character, 1);
        }
    }

    fn put_glyph(&mut self, character: char, width: usize) {
        if self.columns - self.column < width {
            self.put_padding(self.columns - self.column);
            self.next_row();
        }
        let glyph = ScreenGlyph::from_scalar(character)
            .expect("positive-width character must construct a screen glyph");
        let row = &mut self.rows[self.row];
        row.cells[self.column] = ScreenCell::Glyph(glyph);
        for offset in 1..width {
            row.cells[self.column + offset] = ScreenCell::Continuation;
        }
        let atom = row.atoms.len();
        row.atoms.push(Atom::Glyph(character.to_string()));
        row.used = row.used.max(self.column + width);
        self.anchor = Some(Anchor {
            row: self.row,
            column: self.column,
            atom,
        });
        self.column += width;
        if self.column == self.columns {
            self.next_row();
        }
    }

    fn append_to_anchor(&mut self, character: char) -> bool {
        let Some(anchor) = self.anchor else {
            return false;
        };
        let ScreenCell::Glyph(glyph) = &mut self.rows[anchor.row].cells[anchor.column] else {
            return false;
        };
        if !glyph.push_zero_width(character) {
            return false;
        }
        let Atom::Glyph(text) = &mut self.rows[anchor.row].atoms[anchor.atom] else {
            return false;
        };
        text.push(character);
        true
    }

    fn put_spaces(&mut self, mut count: usize) {
        self.anchor = None;
        while count > 0 {
            let available = self.columns - self.column;
            let written = available.min(count);
            if written > 0 {
                let row = &mut self.rows[self.row];
                row.atoms.push(Atom::Spaces(written));
                row.used = row.used.max(self.column + written);
                self.column += written;
                count -= written;
            }
            if self.column == self.columns {
                self.next_row();
            }
        }
    }

    fn put_padding(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        let row = &mut self.rows[self.row];
        for cell in &mut row.cells[self.column..self.column + count] {
            *cell = ScreenCell::Padding;
        }
        row.atoms.push(Atom::Spaces(count));
        row.used = self.column + count;
        self.column += count;
        self.anchor = None;
    }

    fn next_row(&mut self) {
        self.rows.push(Row::new(self.columns));
        self.row += 1;
        self.column = 0;
        self.anchor = None;
    }
}

fn visible_scalar(character: char) -> String {
    match character {
        '\0'..='\u{1f}' => format!("^{}", char::from_u32(u32::from(character) + 0x40).unwrap()),
        '\u{7f}' => "^?".into(),
        _ => format!("\\u{{{:X}}}", u32::from(character)),
    }
}

pub(super) fn build(
    size: ScreenSize,
    left: &Prompt,
    right: Option<&Prompt>,
    line: &Text,
    cursor: TextIndex,
) -> Result<Frame, Error> {
    line.index(cursor.get())?;
    let mut builder = Builder::new(size.columns());
    builder.put_prompt(left);

    let mut cursor_position = None;
    for (index, &unit) in line.as_units().iter().enumerate() {
        if index == cursor.get() {
            cursor_position = Some(builder.position());
        }
        builder.put_unit(unit);
    }
    let cursor_position = cursor_position.unwrap_or_else(|| builder.position());

    if let Some(right) = right {
        add_right_prompt(&mut builder, right);
    }
    viewport(size, builder.rows, cursor_position)
}

fn add_right_prompt(builder: &mut Builder, prompt: &Prompt) {
    let mut right = Builder::new(builder.columns);
    right.put_prompt(prompt);
    if right.rows.len() != 1 {
        return;
    }
    let width = right.rows[0].used;
    if width == 0 {
        builder.rows[0]
            .atoms
            .extend(right.rows[0].atoms.iter().cloned());
        return;
    }
    let start = builder.columns - width;
    if builder.rows[0].used >= start {
        return;
    }
    builder.rows[0].append_at(start, &right.rows[0]);
}

fn viewport(size: ScreenSize, rows: Vec<Row>, cursor: (usize, usize)) -> Result<Frame, Error> {
    let start = cursor.0.saturating_add(1).saturating_sub(size.rows());
    let end = (start + size.rows()).min(rows.len());
    let mut visible = rows[start..end].to_vec();

    if start > 0 {
        let prefix_literals: Vec<_> = rows[..start]
            .iter()
            .flat_map(|row| row.atoms.iter())
            .filter_map(|atom| match atom {
                Atom::Literal(bytes) => Some(Atom::Literal(bytes.clone())),
                Atom::Glyph(_) | Atom::Spaces(_) => None,
            })
            .collect();
        visible[0].atoms.splice(0..0, prefix_literals);
    }

    let mut screen = Screen::new(size);
    for (row_index, row) in visible.iter().enumerate() {
        for (column, cell) in row.cells.iter().enumerate() {
            let position = size.position(row_index, column)?;
            screen.set(position, cell.clone())?;
        }
    }
    let cursor = size.position(cursor.0 - start, cursor.1)?;
    Ok(Frame {
        screen,
        cursor,
        rows: visible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{OpaqueCodePoint, TerminalLiteral};

    fn size(rows: usize, columns: usize) -> ScreenSize {
        ScreenSize::new(rows, columns).unwrap()
    }

    #[test]
    fn literals_do_not_consume_columns() {
        let mut prompt = Prompt::from("a");
        prompt.push_literal(TerminalLiteral::from(&b"\x1b[31m"[..]));
        prompt.push_text("b");
        let frame = build(
            size(2, 8),
            &prompt,
            None,
            &Text::from("c"),
            TextIndex::START,
        )
        .unwrap();
        assert_eq!(frame.cursor.column(), 2);
        assert_eq!(frame.rows[0].used(), 3);
        assert!(matches!(frame.rows[0].atoms[1], Atom::Literal(_)));
    }

    #[test]
    fn raw_units_render_as_escapes() {
        let line: Text = [
            TextUnit::RawByte(0xff),
            TextUnit::OpaqueCodePoint(OpaqueCodePoint::new(0xd800).unwrap()),
        ]
        .into_iter()
        .collect();
        let frame = build(
            size(3, 16),
            &Prompt::default(),
            None,
            &line,
            line.index(2).unwrap(),
        )
        .unwrap();
        assert_eq!(frame.cursor.row(), 0);
        assert_eq!(frame.cursor.column(), 12);
    }

    #[test]
    fn combining_text_shares_one_cell() {
        let line = Text::from("e\u{301}x");
        let frame = build(
            size(2, 8),
            &Prompt::default(),
            None,
            &line,
            line.index(2).unwrap(),
        )
        .unwrap();
        let first = size(2, 8).position(0, 0).unwrap();
        let ScreenCell::Glyph(glyph) = frame.screen.get(first).unwrap() else {
            panic!("first cell was not a glyph");
        };
        assert_eq!(glyph.as_str(), "e\u{301}");
        assert_eq!(frame.cursor.column(), 1);
    }

    #[test]
    fn right_prompt_requires_a_gap() {
        let left = Prompt::from("left");
        let right = Prompt::from("right");
        let empty = Text::default();
        let frame = build(size(1, 9), &left, Some(&right), &empty, TextIndex::START).unwrap();
        assert_eq!(frame.rows[0].used(), 4);

        let frame = build(size(1, 10), &left, Some(&right), &empty, TextIndex::START).unwrap();
        assert_eq!(frame.rows[0].used(), 10);
    }

    #[test]
    fn viewport_keeps_cursor_visible() {
        let line = Text::from("0123456789abcdefghij");
        let frame = build(
            size(2, 5),
            &Prompt::default(),
            None,
            &line,
            line.index(18).unwrap(),
        )
        .unwrap();
        assert_eq!(frame.cursor.row(), 1);
        assert_eq!(frame.cursor.column(), 3);
        assert_eq!(frame.rows.len(), 2);
    }
}
