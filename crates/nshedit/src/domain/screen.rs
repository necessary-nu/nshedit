use std::num::{NonZeroU8, NonZeroUsize};

use unicode_width::UnicodeWidthChar;

use super::Error;

/// Printable scalar text anchored at one physical terminal column.
///
/// The first scalar has a non-zero terminal width. Following zero-width
/// scalars are retained in the same glyph instead of pretending they occupy
/// independent cells.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScreenGlyph {
    text: String,
    width: NonZeroU8,
}

impl ScreenGlyph {
    /// Construct a glyph from one printable, non-zero-width scalar.
    #[must_use]
    pub fn from_scalar(character: char) -> Option<Self> {
        let width = character
            .width()
            .and_then(|width| u8::try_from(width).ok())
            .and_then(NonZeroU8::new)?;
        Some(Self {
            text: character.to_string(),
            width,
        })
    }

    pub(crate) fn push_zero_width(&mut self, character: char) -> bool {
        if character.width() == Some(0) {
            self.text.push(character);
            true
        } else {
            false
        }
    }

    /// Borrow the complete scalar sequence emitted for this glyph.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Number of physical terminal columns occupied by the glyph.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width.get() as usize
    }
}

/// The state of one physical terminal column.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum ScreenCell {
    /// An unoccupied display column.
    #[default]
    Blank,
    /// A printable glyph anchored at this column.
    Glyph(ScreenGlyph),
    /// A column occupied by the tail of a preceding wide glyph.
    Continuation,
    /// A column deliberately skipped at a terminal edge during layout.
    Padding,
}

/// Non-empty terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenSize {
    rows: NonZeroUsize,
    columns: NonZeroUsize,
}

impl ScreenSize {
    /// Validate terminal dimensions and their flattened area.
    pub fn new(rows: usize, columns: usize) -> Result<Self, Error> {
        let (Some(rows), Some(columns)) = (NonZeroUsize::new(rows), NonZeroUsize::new(columns))
        else {
            return Err(Error::InvalidScreenSize { rows, columns });
        };
        if rows.get().checked_mul(columns.get()).is_none() {
            return Err(Error::ScreenTooLarge {
                rows: rows.get(),
                columns: columns.get(),
            });
        }
        Ok(Self { rows, columns })
    }

    #[must_use]
    pub const fn rows(self) -> usize {
        self.rows.get()
    }

    #[must_use]
    pub const fn columns(self) -> usize {
        self.columns.get()
    }

    #[must_use]
    pub const fn area(self) -> usize {
        self.rows.get() * self.columns.get()
    }

    /// Validate a position against these dimensions.
    pub fn position(self, row: usize, column: usize) -> Result<ScreenPosition, Error> {
        if row < self.rows() && column < self.columns() {
            Ok(ScreenPosition { row, column })
        } else {
            Err(Error::ScreenPositionOutOfBounds {
                row,
                column,
                rows: self.rows(),
                columns: self.columns(),
            })
        }
    }
}

/// A row and column validated against a [`ScreenSize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenPosition {
    row: usize,
    column: usize,
}

impl ScreenPosition {
    #[must_use]
    pub const fn row(self) -> usize {
        self.row
    }

    #[must_use]
    pub const fn column(self) -> usize {
        self.column
    }
}

/// A rectangular physical screen image with checked cell access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    size: ScreenSize,
    cells: Vec<ScreenCell>,
}

impl Screen {
    /// Construct a blank image.
    #[must_use]
    pub fn new(size: ScreenSize) -> Self {
        Self {
            size,
            cells: vec![ScreenCell::Blank; size.area()],
        }
    }

    #[must_use]
    pub const fn size(&self) -> ScreenSize {
        self.size
    }

    /// Borrow every physical cell in row-major order.
    #[must_use]
    pub fn cells(&self) -> &[ScreenCell] {
        &self.cells
    }

    /// Read a position, revalidating it for this image.
    pub fn get(&self, position: ScreenPosition) -> Result<&ScreenCell, Error> {
        let index = self.cell_index(position)?;
        Ok(&self.cells[index])
    }

    /// Replace a position, revalidating it for this image.
    pub fn set(&mut self, position: ScreenPosition, cell: ScreenCell) -> Result<(), Error> {
        let index = self.cell_index(position)?;
        self.cells[index] = cell;
        Ok(())
    }

    fn cell_index(&self, position: ScreenPosition) -> Result<usize, Error> {
        self.size
            .position(position.row, position.column)
            .map(|valid| valid.row * self.size.columns() + valid.column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{NonScalarWide, TerminalLiteral, TextUnit};

    #[test]
    fn display_values_have_distinct_types() {
        let logical = TextUnit::CompatibilityWide(NonScalarWide::new(0xD800).unwrap());
        let literal = TerminalLiteral::from(&b"\x1b[31m"[..]);
        let glyph = ScreenGlyph::from_scalar('x').unwrap();
        let cells = [
            ScreenCell::Glyph(glyph),
            ScreenCell::Continuation,
            ScreenCell::Padding,
            ScreenCell::Blank,
        ];

        assert_eq!(logical, TextUnit::from_wide(0xD800));
        assert_eq!(literal.as_bytes(), b"\x1b[31m");
        assert_ne!(cells[1], cells[2]);
    }

    #[test]
    fn combining_scalars_share_the_anchor() {
        let mut glyph = ScreenGlyph::from_scalar('e').unwrap();
        assert!(glyph.push_zero_width('\u{301}'));
        assert!(!glyph.push_zero_width('x'));
        assert_eq!(glyph.as_str(), "e\u{301}");
        assert_eq!(glyph.width(), 1);
        assert!(ScreenGlyph::from_scalar('\u{301}').is_none());
    }

    #[test]
    fn dimensions_and_positions_are_checked() {
        assert_eq!(
            ScreenSize::new(0, 80),
            Err(Error::InvalidScreenSize {
                rows: 0,
                columns: 80,
            })
        );
        let size = ScreenSize::new(2, 3).unwrap();
        assert_eq!(size.position(1, 2).unwrap().column(), 2);
        assert_eq!(
            size.position(2, 0),
            Err(Error::ScreenPositionOutOfBounds {
                row: 2,
                column: 0,
                rows: 2,
                columns: 3,
            })
        );
    }

    #[test]
    fn positions_are_rechecked_on_use() {
        let large = ScreenSize::new(2, 2).unwrap();
        let small = ScreenSize::new(1, 1).unwrap();
        let position = large.position(1, 1).unwrap();
        let screen = Screen::new(small);
        assert_eq!(
            screen.get(position),
            Err(Error::ScreenPositionOutOfBounds {
                row: 1,
                column: 1,
                rows: 1,
                columns: 1,
            })
        );
    }

    #[test]
    fn screen_updates_use_typed_cells() {
        let size = ScreenSize::new(1, 2).unwrap();
        let mut screen = Screen::new(size);
        let position = size.position(0, 1).unwrap();
        screen.set(position, ScreenCell::Continuation).unwrap();
        assert_eq!(screen.get(position), Ok(&ScreenCell::Continuation));
    }
}
