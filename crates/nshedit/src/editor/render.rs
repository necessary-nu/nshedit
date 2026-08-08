//! Private native screen state and safe terminal emission.

mod capability;
mod layout;

use std::fmt;
use std::io::{self, Write};

use nshterm::parm::{Error as ExpansionError, Param, Variables};

use crate::domain::{Error, Prompt, Screen, ScreenPosition, ScreenSize, Text, TextIndex};

pub use capability::{BaudRate, CapabilityKind, TerminalProfile};
use layout::{Atom, Frame};

/// A failure to lay out or emit a native terminal frame.
#[derive(Debug)]
pub enum RenderError {
    /// No terminal profile and screen size have been installed yet.
    DisplayNotConfigured,
    /// Typed editor state failed revalidation while building the frame.
    InvalidState(Error),
    /// The selected terminal cannot position the requested multiline frame.
    CursorAddressUnavailable { rows: usize },
    /// A terminal coordinate cannot be represented by terminfo parameters.
    CoordinateTooLarge { row: usize, column: usize },
    /// A parsed terminal capability contains an invalid expression.
    CapabilityExpansion {
        capability: CapabilityKind,
        source: ExpansionError,
    },
    /// Writing or flushing the caller-supplied destination failed.
    Io(io::Error),
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisplayNotConfigured => formatter.write_str("display is not configured"),
            Self::InvalidState(error) => write!(formatter, "invalid render state: {error}"),
            Self::CursorAddressUnavailable { rows } => write!(
                formatter,
                "terminal cannot position a frame using {rows} rows"
            ),
            Self::CoordinateTooLarge { row, column } => write!(
                formatter,
                "terminal coordinate ({row}, {column}) exceeds terminfo parameters"
            ),
            Self::CapabilityExpansion { capability, source } => write!(
                formatter,
                "could not expand {} capability: {source}",
                capability.description()
            ),
            Self::Io(error) => write!(formatter, "could not write terminal frame: {error}"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidState(error) => Some(error),
            Self::CapabilityExpansion { source, .. } => Some(source),
            Self::Io(error) => Some(error),
            Self::DisplayNotConfigured
            | Self::CursorAddressUnavailable { .. }
            | Self::CoordinateTooLarge { .. } => None,
        }
    }
}

impl From<Error> for RenderError {
    fn from(error: Error) -> Self {
        Self::InvalidState(error)
    }
}

impl From<io::Error> for RenderError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Facts about a frame committed after its writer successfully flushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderSummary {
    size: ScreenSize,
    cursor: ScreenPosition,
    rows_used: usize,
    bytes_written: usize,
}

impl RenderSummary {
    #[must_use]
    pub const fn size(self) -> ScreenSize {
        self.size
    }

    #[must_use]
    pub const fn cursor(self) -> ScreenPosition {
        self.cursor
    }

    #[must_use]
    pub const fn rows_used(self) -> usize {
        self.rows_used
    }

    #[must_use]
    pub const fn bytes_written(self) -> usize {
        self.bytes_written
    }
}

#[derive(Default)]
pub(super) struct State {
    configured: Option<Configured>,
}

struct Configured {
    profile: TerminalProfile,
    screen: Screen,
    cursor: ScreenPosition,
    rows_used: usize,
    variables: Variables,
}

impl State {
    pub(super) fn configure(&mut self, profile: TerminalProfile, size: ScreenSize) {
        self.configured = Some(Configured {
            profile,
            screen: Screen::new(size),
            cursor: size
                .position(0, 0)
                .expect("a validated screen has an origin"),
            rows_used: 0,
            variables: Variables::new(),
        });
    }

    pub(super) fn resize(&mut self, size: ScreenSize) -> Result<(), RenderError> {
        let configured = self
            .configured
            .as_mut()
            .ok_or(RenderError::DisplayNotConfigured)?;
        configured.screen = Screen::new(size);
        configured.cursor = size
            .position(0, 0)
            .expect("a validated screen has an origin");
        configured.rows_used = 0;
        Ok(())
    }

    pub(super) fn profile(&self) -> Option<&TerminalProfile> {
        self.configured
            .as_ref()
            .map(|configured| &configured.profile)
    }

    pub(super) fn screen(&self) -> Option<&Screen> {
        self.configured
            .as_ref()
            .map(|configured| &configured.screen)
    }

    pub(super) fn cursor(&self) -> Option<ScreenPosition> {
        self.configured.as_ref().map(|configured| configured.cursor)
    }

    pub(super) fn present(
        &mut self,
        left: &Prompt,
        right: Option<&Prompt>,
        line: &Text,
        cursor: TextIndex,
        output: &mut dyn Write,
    ) -> Result<RenderSummary, RenderError> {
        let configured = self
            .configured
            .as_ref()
            .ok_or(RenderError::DisplayNotConfigured)?;
        let frame = layout::build(configured.screen.size(), left, right, line, cursor)?;
        let mut variables = configured.variables.clone();
        let bytes = encode(
            &configured.profile,
            &frame,
            configured.rows_used,
            &mut variables,
        )?;

        output.write_all(&bytes)?;
        output.flush()?;

        let rows_used = frame.rows_used();
        let summary = RenderSummary {
            size: frame.screen.size(),
            cursor: frame.cursor,
            rows_used,
            bytes_written: bytes.len(),
        };
        let configured = self
            .configured
            .as_mut()
            .expect("display configuration cannot disappear during a write");
        configured.screen = frame.screen;
        configured.cursor = frame.cursor;
        configured.rows_used = rows_used;
        configured.variables = variables;
        Ok(summary)
    }

    pub(super) fn beep(&mut self, output: &mut dyn Write) -> Result<usize, RenderError> {
        let configured = self
            .configured
            .as_mut()
            .ok_or(RenderError::DisplayNotConfigured)?;
        let mut bytes = Vec::new();
        let mut variables = configured.variables.clone();
        append_capability(
            &configured.profile,
            &mut bytes,
            CapabilityKind::Bell,
            &[],
            1,
            &mut variables,
        )?;
        output.write_all(&bytes)?;
        output.flush()?;
        configured.variables = variables;
        Ok(bytes.len())
    }
}

fn encode(
    profile: &TerminalProfile,
    frame: &Frame,
    previous_rows: usize,
    variables: &mut Variables,
) -> Result<Vec<u8>, RenderError> {
    let mut output = Vec::new();
    let cleared = append_capability(
        profile,
        &mut output,
        CapabilityKind::ClearScreen,
        &[],
        frame.rows_used(),
        variables,
    )?;

    if profile.has_cursor_address() {
        encode_addressed(
            profile,
            frame,
            previous_rows,
            cleared,
            &mut output,
            variables,
        )?;
    } else {
        encode_plain(profile, frame, previous_rows, &mut output, variables)?;
    }
    Ok(output)
}

fn encode_addressed(
    profile: &TerminalProfile,
    frame: &Frame,
    previous_rows: usize,
    cleared: bool,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    let rows = if cleared {
        frame.rows_used()
    } else {
        frame.rows_used().max(previous_rows)
    };
    for row in 0..rows {
        append_cursor(profile, output, row, 0, variables)?;
        let used = match frame.rows.get(row) {
            Some(content) => {
                append_atoms(output, &content.atoms);
                content.used()
            }
            None => 0,
        };
        let cleared_to_end = append_capability(
            profile,
            output,
            CapabilityKind::ClearToEndOfLine,
            &[],
            1,
            variables,
        )?;
        if !cleared_to_end && !cleared {
            output.extend(std::iter::repeat_n(
                b' ',
                frame.screen.size().columns() - used,
            ));
        }
    }
    append_cursor(
        profile,
        output,
        frame.cursor.row(),
        frame.cursor.column(),
        variables,
    )
}

fn encode_plain(
    profile: &TerminalProfile,
    frame: &Frame,
    previous_rows: usize,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    if frame.rows_used() > 1 || previous_rows > 1 || frame.cursor.row() > 0 {
        return Err(RenderError::CursorAddressUnavailable {
            rows: frame.rows_used().max(previous_rows),
        });
    }
    append_capability(
        profile,
        output,
        CapabilityKind::CarriageReturn,
        &[],
        1,
        variables,
    )?;
    let row = &frame.rows[0];
    append_atoms(output, &row.atoms);
    let cleared_to_end = append_capability(
        profile,
        output,
        CapabilityKind::ClearToEndOfLine,
        &[],
        1,
        variables,
    )?;
    let end_column = if cleared_to_end {
        row.used()
    } else {
        output.extend(std::iter::repeat_n(
            b' ',
            frame.screen.size().columns() - row.used(),
        ));
        frame.screen.size().columns()
    };
    output.extend(std::iter::repeat_n(
        b'\x08',
        end_column - frame.cursor.column(),
    ));
    Ok(())
}

fn append_cursor(
    profile: &TerminalProfile,
    output: &mut Vec<u8>,
    row: usize,
    column: usize,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    let (Ok(row_param), Ok(column_param)) = (i32::try_from(row), i32::try_from(column)) else {
        return Err(RenderError::CoordinateTooLarge { row, column });
    };
    append_capability(
        profile,
        output,
        CapabilityKind::CursorAddress,
        &[Param::Number(row_param), Param::Number(column_param)],
        1,
        variables,
    )?;
    Ok(())
}

fn append_capability(
    profile: &TerminalProfile,
    output: &mut Vec<u8>,
    capability: CapabilityKind,
    params: &[Param],
    affected_lines: usize,
    variables: &mut Variables,
) -> Result<bool, RenderError> {
    profile
        .append(output, capability, params, affected_lines, variables)
        .map_err(|source| RenderError::CapabilityExpansion { capability, source })
}

fn append_atoms(output: &mut Vec<u8>, atoms: &[Atom]) {
    for atom in atoms {
        match atom {
            Atom::Glyph(text) => output.extend_from_slice(text.as_bytes()),
            Atom::Literal(bytes) => output.extend_from_slice(bytes),
            Atom::Spaces(count) => output.extend(std::iter::repeat_n(b' ', *count)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Prompt, ScreenCell};

    struct FailingWriter {
        limit: usize,
        written: usize,
        fail_flush: bool,
    }

    impl Write for FailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.written >= self.limit {
                return Err(io::Error::other("write failed"));
            }
            let count = bytes.len().min(self.limit - self.written);
            self.written += count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                Err(io::Error::other("flush failed"))
            } else {
                Ok(())
            }
        }
    }

    fn configured(profile: TerminalProfile, rows: usize, columns: usize) -> State {
        let mut state = State::default();
        state.configure(profile, ScreenSize::new(rows, columns).unwrap());
        state
    }

    // [spec:nshedit:req:core.terminal-render+1/test]
    #[test]
    fn ansi_frame_commits_after_flush() {
        let mut state = configured(TerminalProfile::ansi(), 2, 10);
        let line = Text::from("abc");
        let mut output = Vec::new();
        let summary = state
            .present(
                &Prompt::from("p> "),
                None,
                &line,
                line.index(2).unwrap(),
                &mut output,
            )
            .unwrap();

        assert!(output.starts_with(b"\x1b[H\x1b[2J"));
        assert!(output.windows(6).any(|window| window == b"p> abc"));
        assert_eq!(summary.cursor().column(), 5);
        assert_eq!(state.cursor(), Some(summary.cursor()));
        let first = summary.size().position(0, 0).unwrap();
        assert!(matches!(
            state.screen().unwrap().get(first),
            Ok(ScreenCell::Glyph(_))
        ));
    }

    #[test]
    fn failed_write_keeps_committed_screen() {
        let mut state = configured(TerminalProfile::ansi(), 2, 10);
        let before = state.screen().unwrap().clone();
        let mut writer = FailingWriter {
            limit: 3,
            written: 0,
            fail_flush: false,
        };
        let error = state
            .present(
                &Prompt::default(),
                None,
                &Text::from("changed"),
                TextIndex::START,
                &mut writer,
            )
            .unwrap_err();
        assert!(matches!(error, RenderError::Io(_)));
        assert_eq!(state.screen(), Some(&before));

        let mut writer = FailingWriter {
            limit: usize::MAX,
            written: 0,
            fail_flush: true,
        };
        assert!(
            state
                .present(
                    &Prompt::default(),
                    None,
                    &Text::from("changed"),
                    TextIndex::START,
                    &mut writer,
                )
                .is_err()
        );
        assert_eq!(state.screen(), Some(&before));
    }

    #[test]
    fn plain_profile_rejects_multiline() {
        let mut state = configured(TerminalProfile::plain(), 2, 4);
        let line = Text::from("long line");
        let error = state
            .present(
                &Prompt::default(),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RenderError::CursorAddressUnavailable { .. }
        ));
    }

    #[test]
    fn resize_discards_the_old_image() {
        let mut state = configured(TerminalProfile::ansi(), 2, 4);
        state.resize(ScreenSize::new(3, 5).unwrap()).unwrap();
        assert_eq!(state.screen().unwrap().size().rows(), 3);
        assert!(
            state
                .screen()
                .unwrap()
                .cells()
                .iter()
                .all(|cell| cell == &ScreenCell::Blank)
        );
    }

    #[test]
    fn beep_uses_the_safe_writer() {
        let mut state = configured(TerminalProfile::plain(), 1, 8);
        let mut output = Vec::new();
        assert_eq!(state.beep(&mut output).unwrap(), 1);
        assert_eq!(output, b"\x07");
    }
}
