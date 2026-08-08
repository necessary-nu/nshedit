use std::num::NonZeroUsize;

use super::Error;

/// An index into the renderer's owned table of zero-width terminal bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LiteralId(usize);

/// Terminal byte strings referenced by [`ScreenCell::Literal`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiteralTable(Vec<Box<[u8]>>);

impl LiteralTable {
    /// Own a terminal sequence and return its stable identifier.
    pub fn insert(&mut self, bytes: impl Into<Box<[u8]>>) -> LiteralId {
        let id = LiteralId(self.0.len());
        self.0.push(bytes.into());
        id
    }

    /// Resolve an identifier issued by this table.
    #[must_use]
    pub fn get(&self, id: LiteralId) -> Option<&[u8]> {
        self.0.get(id.0).map(AsRef::as_ref)
    }
}

// [spec:nshedit:req:core.text-screen-model] rendered screen representation
/// One rendered terminal cell. Display bookkeeping is represented by variants
/// and can never alias a text value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenCell {
    /// One visible Unicode scalar.
    Text(char),
    /// A cell occupied by the tail of a preceding wide glyph.
    Continuation,
    /// A cell reserved as terminal-edge or layout padding.
    Padding,
    /// A zero-width terminal byte sequence owned by the renderer.
    Literal(LiteralId),
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

/// A rectangular rendered image with checked cell access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    size: ScreenSize,
    cells: Vec<ScreenCell>,
}

impl Screen {
    /// Construct a blank image. A blank is visible text, not padding.
    #[must_use]
    pub fn new(size: ScreenSize) -> Self {
        Self {
            size,
            cells: vec![ScreenCell::Text(' '); size.area()],
        }
    }

    /// Construct an image with every cell in an explicit state.
    #[must_use]
    pub fn filled(size: ScreenSize, cell: ScreenCell) -> Self {
        Self {
            size,
            cells: vec![cell; size.area()],
        }
    }

    #[must_use]
    pub const fn size(&self) -> ScreenSize {
        self.size
    }

    /// Read a position, revalidating it for this image.
    pub fn get(&self, position: ScreenPosition) -> Result<ScreenCell, Error> {
        let index = self.cell_index(position)?;
        Ok(self.cells[index])
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
    use crate::domain::{NonScalarWide, TextUnit};

    // [spec:nshedit:req:core.text-screen-model/test]
    #[test]
    fn screen_cells_have_explicit_states() {
        let logical = TextUnit::CompatibilityWide(NonScalarWide::new(0xD800).unwrap());
        let mut literals = LiteralTable::default();
        let literal = literals.insert(&b"\x1b[31m"[..]);
        let cells = [
            ScreenCell::Text('x'),
            ScreenCell::Continuation,
            ScreenCell::Padding,
            ScreenCell::Literal(literal),
        ];

        assert_eq!(logical, TextUnit::from_wide(0xD800));
        assert_eq!(cells[3], ScreenCell::Literal(literal));
        assert_eq!(literals.get(literal), Some(&b"\x1b[31m"[..]));
        assert_ne!(cells[1], cells[2]);
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
        assert_eq!(screen.get(position), Ok(ScreenCell::Continuation));
    }
}
