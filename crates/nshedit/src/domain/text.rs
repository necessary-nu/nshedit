use std::ops::Range;

use super::Error;

// [spec:nshedit:req:core.text-screen-model+1]
/// An opaque numeric code point that is deliberately not a Unicode scalar.
///
/// Host boundaries can receive values that Rust's [`char`] cannot represent.
/// This type preserves such a value without naming or imposing its transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpaqueCodePoint(u32);

impl OpaqueCodePoint {
    /// Construct an opaque value without duplicating a [`char`].
    #[must_use]
    pub fn new(value: u32) -> Option<Self> {
        if char::from_u32(value).is_some() {
            None
        } else {
            Some(Self(value))
        }
    }

    /// Recover the preserved numeric value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One logical input unit, preserving every representation the boundary can
/// receive without conflating it with display bookkeeping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TextUnit {
    /// A decoded Unicode scalar value.
    Scalar(char),
    /// One byte that could not be decoded under the boundary's active locale.
    RawByte(u8),
    /// An opaque code point that Rust's [`char`] cannot represent.
    OpaqueCodePoint(OpaqueCodePoint),
}

impl TextUnit {
    /// Classify a numeric code point without losing non-scalar values.
    #[must_use]
    pub fn from_code_point(value: u32) -> Self {
        match char::from_u32(value) {
            Some(character) => Self::Scalar(character),
            None => Self::OpaqueCodePoint(OpaqueCodePoint(value)),
        }
    }
}

/// An owned logical input string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Text(Vec<TextUnit>);

impl Text {
    /// Number of logical units, not encoded bytes or display cells.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether no logical units are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Borrow the logical units.
    #[must_use]
    pub fn as_units(&self) -> &[TextUnit] {
        &self.0
    }

    /// Append one logical unit.
    pub fn push(&mut self, unit: TextUnit) {
        self.0.push(unit);
    }

    /// Remove every logical unit.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Validate a cursor boundary in this text. The end boundary is valid.
    pub fn index(&self, index: usize) -> Result<TextIndex, Error> {
        if index <= self.len() {
            Ok(TextIndex(index))
        } else {
            Err(Error::TextIndexOutOfBounds {
                index,
                len: self.len(),
            })
        }
    }

    /// Validate an ordered, half-open span in this text.
    pub fn span(&self, range: Range<usize>) -> Result<TextSpan, Error> {
        if range.start <= range.end && range.end <= self.len() {
            Ok(TextSpan {
                start: TextIndex(range.start),
                end: TextIndex(range.end),
            })
        } else {
            Err(Error::InvalidTextSpan {
                start: range.start,
                end: range.end,
                len: self.len(),
            })
        }
    }

    /// Borrow a span, revalidating it for this particular value.
    pub fn slice(&self, span: TextSpan) -> Result<&[TextUnit], Error> {
        self.span(span.start.get()..span.end.get())?;
        Ok(&self.0[span.start.get()..span.end.get()])
    }

    /// Insert owned logical text at a checked boundary.
    pub fn insert(&mut self, at: TextIndex, inserted: &Self) -> Result<(), Error> {
        self.index(at.get())?;
        self.0
            .splice(at.get()..at.get(), inserted.0.iter().copied());
        Ok(())
    }

    /// Remove a checked span and return what it contained.
    pub fn remove(&mut self, span: TextSpan) -> Result<Self, Error> {
        self.span(span.start.get()..span.end.get())?;
        Ok(Self(
            self.0.drain(span.start.get()..span.end.get()).collect(),
        ))
    }

    // [spec:nshedit:req:core.line-commands]
    /// Replace a checked span and return its previous contents.
    pub fn replace(&mut self, span: TextSpan, replacement: &Self) -> Result<Self, Error> {
        self.span(span.start.get()..span.end.get())?;
        Ok(Self(
            self.0
                .splice(
                    span.start.get()..span.end.get(),
                    replacement.0.iter().copied(),
                )
                .collect(),
        ))
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        value.chars().map(TextUnit::Scalar).collect()
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        value.chars().map(TextUnit::Scalar).collect()
    }
}

impl FromIterator<TextUnit> for Text {
    fn from_iter<T: IntoIterator<Item = TextUnit>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl Extend<TextUnit> for Text {
    fn extend<T: IntoIterator<Item = TextUnit>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl IntoIterator for Text {
    type Item = TextUnit;
    type IntoIter = std::vec::IntoIter<TextUnit>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Text {
    type Item = &'a TextUnit;
    type IntoIter = std::slice::Iter<'a, TextUnit>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// A validated logical-text boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextIndex(usize);

impl TextIndex {
    /// The first boundary, valid for every [`Text`].
    pub const START: Self = Self(0);

    /// The zero-based logical-unit offset.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// A validated, ordered, half-open range of logical text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextSpan {
    start: TextIndex,
    end: TextIndex,
}

impl TextSpan {
    #[must_use]
    pub const fn start(self) -> TextIndex {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> TextIndex {
        self.end
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.end.0 - self.start.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start.0 == self.end.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:nshedit:req:core.text-screen-model+1/test]
    #[test]
    fn code_points_keep_their_kind() {
        assert_eq!(TextUnit::from_code_point(0x41), TextUnit::Scalar('A'));
        assert_eq!(
            TextUnit::from_code_point(0xD800),
            TextUnit::OpaqueCodePoint(OpaqueCodePoint::new(0xD800).unwrap())
        );
        assert_eq!(OpaqueCodePoint::new(0x41), None);
    }

    #[test]
    fn raw_bytes_are_not_scalars() {
        assert_ne!(TextUnit::RawByte(b'A'), TextUnit::Scalar('A'));
    }

    #[test]
    fn indices_and_spans_are_checked() {
        let text = Text::from("abc");
        assert_eq!(text.index(3).unwrap().get(), 3);
        assert_eq!(
            text.index(4),
            Err(Error::TextIndexOutOfBounds { index: 4, len: 3 })
        );
        assert_eq!(text.span(1..3).unwrap().len(), 2);
        let end = text.len() - 2;
        assert_eq!(
            text.span(text.len()..end),
            Err(Error::InvalidTextSpan {
                start: 3,
                end: 1,
                len: 3,
            })
        );
    }

    #[test]
    fn spans_are_rechecked_on_use() {
        let long = Text::from("abcd");
        let short = Text::from("a");
        let span = long.span(2..4).unwrap();
        assert_eq!(
            short.slice(span),
            Err(Error::InvalidTextSpan {
                start: 2,
                end: 4,
                len: 1,
            })
        );
    }

    #[test]
    fn mutations_preserve_logical_units() {
        let mut text = Text::from("ac");
        text.insert(text.index(1).unwrap(), &Text::from("b"))
            .unwrap();
        assert_eq!(text, Text::from("abc"));
        let removed = text.remove(text.span(1..2).unwrap()).unwrap();
        assert_eq!(removed, Text::from("b"));
        assert_eq!(text, Text::from("ac"));
    }
}
