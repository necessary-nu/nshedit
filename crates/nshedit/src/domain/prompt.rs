use super::Text;

/// An owned zero-width terminal byte sequence embedded in a prompt.
///
/// A terminal literal is deliberately not a screen cell: emitting it changes
/// terminal state without occupying or replacing a display column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerminalLiteral(Box<[u8]>);

impl TerminalLiteral {
    /// Borrow the exact bytes to emit.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl From<&[u8]> for TerminalLiteral {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.into())
    }
}

impl From<Vec<u8>> for TerminalLiteral {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes.into_boxed_slice())
    }
}

impl From<Box<[u8]>> for TerminalLiteral {
    fn from(bytes: Box<[u8]>) -> Self {
        Self(bytes)
    }
}

/// One typed component of a native prompt.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PromptPart {
    /// Logical text that participates in layout and occupies columns.
    Text(Text),
    /// Terminal bytes that occupy no columns.
    Literal(TerminalLiteral),
}

/// An owned prompt made from logical text and explicit zero-width literals.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Prompt(Vec<PromptPart>);

impl Prompt {
    /// Append logical prompt text.
    pub fn push_text(&mut self, text: impl Into<Text>) {
        let text = text.into();
        if !text.is_empty() {
            self.0.push(PromptPart::Text(text));
        }
    }

    /// Append an explicit zero-width terminal sequence.
    pub fn push_literal(&mut self, literal: TerminalLiteral) {
        if !literal.as_bytes().is_empty() {
            self.0.push(PromptPart::Literal(literal));
        }
    }

    /// Borrow the prompt components in emission order.
    #[must_use]
    pub fn parts(&self) -> &[PromptPart] {
        &self.0
    }

    /// Whether this prompt has no visible text or terminal literals.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Text> for Prompt {
    fn from(text: Text) -> Self {
        if text.is_empty() {
            Self::default()
        } else {
            Self(vec![PromptPart::Text(text)])
        }
    }
}

impl From<&str> for Prompt {
    fn from(text: &str) -> Self {
        Self::from(Text::from(text))
    }
}

impl From<String> for Prompt {
    fn from(text: String) -> Self {
        Self::from(Text::from(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_separates_text_from_literals() {
        let mut prompt = Prompt::from("ready> ");
        prompt.push_literal(TerminalLiteral::from(&b"\x1b[31m"[..]));
        prompt.push_text("red");

        assert_eq!(prompt.parts().len(), 3);
        assert_eq!(
            prompt.parts()[1],
            PromptPart::Literal(TerminalLiteral::from(&b"\x1b[31m"[..]))
        );
    }

    #[test]
    fn empty_parts_do_not_accumulate() {
        let mut prompt = Prompt::default();
        prompt.push_text("");
        prompt.push_literal(TerminalLiteral::from(Box::<[u8]>::default()));
        assert!(prompt.is_empty());
    }
}
