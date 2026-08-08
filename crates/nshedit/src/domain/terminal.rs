/// The semantic tty mode owned by an active native editor session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalMode {
    /// Canonical host input outside interactive editing.
    Cooked,
    /// Character-at-a-time input used while editing a line.
    Editing,
    /// A temporary mode used to read one character literally.
    Quoted,
}
