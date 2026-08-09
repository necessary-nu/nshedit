//! Private native screen state and safe terminal emission.

mod capability;
mod layout;

use std::fmt;
use std::io::{self, Write};

use nshterm::parm::{Error as ExpansionError, Param, Variables};
use unicode_width::UnicodeWidthStr;

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
    rows: Vec<layout::Row>,
    cursor: ScreenPosition,
    rows_used: usize,
    variables: Variables,
    redraw: bool,
    damaged: bool,
}

#[derive(Clone, Copy)]
struct Committed<'a> {
    screen: &'a Screen,
    rows: &'a [layout::Row],
    cursor: ScreenPosition,
    redraw: bool,
    damaged: bool,
}

impl State {
    pub(super) fn configure(&mut self, profile: TerminalProfile, size: ScreenSize) {
        self.configured = Some(Configured {
            profile,
            screen: Screen::new(size),
            rows: Vec::new(),
            cursor: size
                .position(0, 0)
                .expect("a validated screen has an origin"),
            rows_used: 0,
            variables: Variables::new(),
            redraw: false,
            damaged: false,
        });
    }

    pub(super) fn resize(&mut self, size: ScreenSize) -> Result<(), RenderError> {
        let configured = self
            .configured
            .as_mut()
            .ok_or(RenderError::DisplayNotConfigured)?;
        configured.screen = Screen::new(size);
        configured.rows.clear();
        configured.cursor = size
            .position(0, 0)
            .expect("a validated screen has an origin");
        configured.rows_used = 0;
        configured.redraw = false;
        configured.damaged = true;
        Ok(())
    }

    pub(super) fn redraw(&mut self) {
        if let Some(configured) = &mut self.configured {
            configured.redraw = true;
        }
    }

    pub(super) fn damage(&mut self) {
        if let Some(configured) = &mut self.configured {
            configured.redraw = false;
            configured.damaged = true;
        }
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
        let committed = Committed {
            screen: &configured.screen,
            rows: &configured.rows,
            cursor: configured.cursor,
            redraw: configured.redraw,
            damaged: configured.damaged,
        };
        let bytes = encode(&configured.profile, &frame, committed, &mut variables)?;

        if let Err(error) = output.write_all(&bytes).and_then(|()| output.flush()) {
            self.configured
                .as_mut()
                .expect("display configuration cannot disappear during a write")
                .redraw = false;
            self.configured
                .as_mut()
                .expect("display configuration cannot disappear during a write")
                .damaged = true;
            return Err(RenderError::Io(error));
        }

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
        configured.rows = frame.rows;
        configured.cursor = frame.cursor;
        configured.rows_used = rows_used;
        configured.variables = variables;
        configured.redraw = false;
        configured.damaged = false;
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

    pub(super) fn finish_line(&mut self, output: &mut dyn Write) -> Result<usize, RenderError> {
        let configured = self
            .configured
            .as_mut()
            .ok_or(RenderError::DisplayNotConfigured)?;
        if let Err(error) = output.write_all(b"\n").and_then(|()| output.flush()) {
            configured.damaged = true;
            return Err(RenderError::Io(error));
        }
        let size = configured.screen.size();
        configured.screen = Screen::new(size);
        configured.rows.clear();
        configured.cursor = size
            .position(0, 0)
            .expect("a validated screen has an origin");
        configured.rows_used = 0;
        configured.redraw = false;
        configured.damaged = false;
        Ok(1)
    }
}

// [spec:nshedit:req:core.incremental-render]
fn encode(
    profile: &TerminalProfile,
    frame: &Frame,
    committed: Committed<'_>,
    variables: &mut Variables,
) -> Result<Vec<u8>, RenderError> {
    let mut output = Vec::new();
    if profile.has_cursor_address() {
        encode_addressed(profile, frame, committed, &mut output, variables)?;
    } else {
        encode_plain(profile, frame, committed, &mut output, variables)?;
    }
    Ok(output)
}

fn encode_addressed(
    profile: &TerminalProfile,
    frame: &Frame,
    committed: Committed<'_>,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    let cleared = committed.damaged
        && append_capability(
            profile,
            output,
            CapabilityKind::ClearScreen,
            &[],
            frame.rows_used(),
            variables,
        )?;
    let columns = frame.screen.size().columns();
    let mut terminal_cursor =
        (!committed.damaged).then_some((committed.cursor.row(), committed.cursor.column()));
    let rows = if cleared {
        frame.rows_used()
    } else if committed.damaged {
        frame.screen.size().rows()
    } else {
        frame.rows_used().max(committed.rows.len())
    };
    for row in 0..rows {
        let current = frame.rows.get(row);
        let previous = committed.rows.get(row);
        let rewrite = cleared || committed.redraw || committed.damaged;
        let start = if rewrite {
            0
        } else {
            let Some(start) = first_changed_column(frame, committed, row, current, previous) else {
                continue;
            };
            if start < columns { start } else { 0 }
        };
        if terminal_cursor != Some((row, start)) {
            append_cursor(profile, output, row, start, variables)?;
            terminal_cursor = Some((row, start));
        }
        let used = match current {
            Some(content) => {
                append_atom_range(output, content, start, content.used(), true);
                if content.used() > start {
                    terminal_cursor = (content.used() < columns).then_some((row, content.used()));
                }
                content.used()
            }
            None => 0,
        };
        let previous_used = previous.map_or(0, layout::Row::used);
        if !cleared && (rewrite || previous_used > used) {
            let cleared_to_end = append_capability(
                profile,
                output,
                CapabilityKind::ClearToEndOfLine,
                &[],
                1,
                variables,
            )?;
            if !cleared_to_end {
                let erase_to = if committed.damaged {
                    columns
                } else {
                    used.max(previous_used)
                };
                let spaces = erase_to.saturating_sub(used);
                output.extend(std::iter::repeat_n(b' ', spaces));
                if spaces > 0 {
                    terminal_cursor = (erase_to < columns).then_some((row, erase_to));
                }
            }
        }
    }
    let target = (frame.cursor.row(), frame.cursor.column());
    if terminal_cursor != Some(target) {
        append_cursor(
            profile,
            output,
            frame.cursor.row(),
            frame.cursor.column(),
            variables,
        )?;
    }
    Ok(())
}

fn first_changed_column(
    frame: &Frame,
    committed: Committed<'_>,
    row: usize,
    current: Option<&layout::Row>,
    previous: Option<&layout::Row>,
) -> Option<usize> {
    if current == previous {
        return None;
    }
    let columns = frame.screen.size().columns();
    let offset = row * columns;
    let cell = committed.screen.cells()[offset..offset + columns]
        .iter()
        .zip(&frame.screen.cells()[offset..offset + columns])
        .position(|(before, after)| before != after);
    let used = match (
        previous.map(layout::Row::used),
        current.map(layout::Row::used),
    ) {
        (Some(before), Some(after)) if before != after => Some(before.min(after)),
        (Some(_), None) | (None, Some(_)) => Some(0),
        _ => None,
    };
    Some(cell.into_iter().chain(used).min().unwrap_or(0))
}

fn encode_plain(
    profile: &TerminalProfile,
    frame: &Frame,
    committed: Committed<'_>,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    if frame.rows_used() > 1 || committed.rows.len() > 1 || frame.cursor.row() > 0 {
        return Err(RenderError::CursorAddressUnavailable {
            rows: frame.rows_used().max(committed.rows.len()),
        });
    }
    let row = &frame.rows[0];
    let Some(previous) = committed.rows.first() else {
        append_atoms(output, &row.atoms);
        return append_plain_reposition(
            profile,
            row,
            row.used(),
            frame.cursor.column(),
            output,
            variables,
        );
    };

    if committed.damaged {
        return append_plain_redraw(profile, frame, previous, true, output, variables);
    }

    if committed.redraw {
        return append_plain_redraw(profile, frame, previous, false, output, variables);
    }

    if row == previous {
        return match frame.cursor.column().cmp(&committed.cursor.column()) {
            std::cmp::Ordering::Less if committed.cursor.column() - frame.cursor.column() == 1 => {
                append_cursor_left(profile, output, 1, variables)
            }
            std::cmp::Ordering::Less => append_plain_left(
                profile,
                row,
                committed.cursor.column(),
                frame.cursor.column(),
                output,
                variables,
            ),
            std::cmp::Ordering::Greater => {
                append_atom_range(
                    output,
                    row,
                    committed.cursor.column(),
                    frame.cursor.column(),
                    true,
                );
                Ok(())
            }
            std::cmp::Ordering::Equal => {
                append_carriage_return(profile, output, variables)?;
                append_atom_range(output, row, 0, frame.cursor.column(), true);
                Ok(())
            }
        };
    }

    let columns = frame.screen.size().columns();
    let common_prefix = committed.screen.cells()[..columns]
        .iter()
        .zip(&frame.screen.cells()[..columns])
        .take_while(|(before, after)| before == after)
        .count();
    if committed.cursor.column() == previous.used()
        && frame.cursor.column() == row.used()
        && row.used() >= previous.used()
        && common_prefix >= previous.used()
    {
        append_atom_range(
            output,
            row,
            committed.cursor.column(),
            frame.cursor.column(),
            true,
        );
        return Ok(());
    }

    append_plain_redraw(profile, frame, previous, false, output, variables)
}

fn append_plain_redraw(
    profile: &TerminalProfile,
    frame: &Frame,
    previous: &layout::Row,
    damaged: bool,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    append_carriage_return(profile, output, variables)?;
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
        let erase_to = if damaged {
            frame.screen.size().columns()
        } else {
            previous.used().max(row.used())
        };
        output.extend(std::iter::repeat_n(
            b' ',
            erase_to.saturating_sub(row.used()),
        ));
        erase_to
    };
    append_plain_reposition(
        profile,
        row,
        end_column,
        frame.cursor.column(),
        output,
        variables,
    )
}

fn append_plain_reposition(
    profile: &TerminalProfile,
    row: &layout::Row,
    from: usize,
    to: usize,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    let distance = from.saturating_sub(to);
    match distance {
        0 => Ok(()),
        1 => append_cursor_left(profile, output, 1, variables),
        _ => append_plain_left(profile, row, from, to, output, variables),
    }
}

fn append_plain_left(
    profile: &TerminalProfile,
    row: &layout::Row,
    from: usize,
    to: usize,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    let mut left = Vec::new();
    let mut left_variables = variables.clone();
    append_cursor_left(
        profile,
        &mut left,
        from.saturating_sub(to),
        &mut left_variables,
    )?;

    let mut restart = Vec::new();
    let mut restart_variables = variables.clone();
    append_carriage_return(profile, &mut restart, &mut restart_variables)?;
    append_atom_range(&mut restart, row, 0, to, true);

    if left.len() <= restart.len() {
        output.extend_from_slice(&left);
        *variables = left_variables;
    } else {
        output.extend_from_slice(&restart);
        *variables = restart_variables;
    }
    Ok(())
}

fn append_carriage_return(
    profile: &TerminalProfile,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    append_capability(
        profile,
        output,
        CapabilityKind::CarriageReturn,
        &[],
        1,
        variables,
    )?;
    Ok(())
}

fn append_cursor_left(
    profile: &TerminalProfile,
    output: &mut Vec<u8>,
    count: usize,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    for _ in 0..count {
        append_capability(
            profile,
            output,
            CapabilityKind::CursorLeft,
            &[],
            1,
            variables,
        )?;
    }
    Ok(())
}

fn append_atom_range(
    output: &mut Vec<u8>,
    row: &layout::Row,
    start: usize,
    end: usize,
    include_end_literals: bool,
) {
    let mut column = 0usize;
    for atom in &row.atoms {
        match atom {
            Atom::Literal(bytes) => {
                if column >= start && (column < end || (include_end_literals && column == end)) {
                    output.extend_from_slice(bytes);
                }
            }
            Atom::Glyph(text) => {
                let width = text.width();
                if column < end && column.saturating_add(width) > start {
                    output.extend_from_slice(text.as_bytes());
                }
                column = column.saturating_add(width);
            }
            Atom::Spaces(count) => {
                let atom_end = column.saturating_add(*count);
                let overlap_start = column.max(start);
                let overlap_end = atom_end.min(end);
                output.extend(std::iter::repeat_n(
                    b' ',
                    overlap_end.saturating_sub(overlap_start),
                ));
                column = atom_end;
            }
        }
    }
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

        assert!(!output.windows(4).any(|window| window == b"\x1b[2J"));
        assert!(output.windows(6).any(|window| window == b"p> abc"));
        assert_eq!(summary.cursor().column(), 5);
        assert_eq!(state.cursor(), Some(summary.cursor()));
        let first = summary.size().position(0, 0).unwrap();
        assert!(matches!(
            state.screen().unwrap().get(first),
            Ok(ScreenCell::Glyph(_))
        ));
    }

    // [spec:nshedit:req:core.incremental-render/test]
    #[test]
    fn addressed_frames_diff_from_first_change() {
        let mut state = configured(TerminalProfile::ansi(), 2, 20);
        let prompt = Prompt::from("> ");
        let mut output = Vec::new();

        let mut present = |state: &mut State, line: &str, cursor: usize| {
            output.clear();
            let line = Text::from(line);
            state
                .present(
                    &prompt,
                    None,
                    &line,
                    line.index(cursor).unwrap(),
                    &mut output,
                )
                .unwrap();
            output.clone()
        };

        assert_eq!(present(&mut state, "", 0), b"> ");
        assert_eq!(present(&mut state, "h", 1), b"h");
        assert_eq!(present(&mut state, "hello", 5), b"ello");
        assert_eq!(present(&mut state, "hallo", 2), b"\x1b[1;4Hallo\x1b[1;5H");
        assert_eq!(present(&mut state, "hall", 4), b"\x1b[1;7H\x1b[K");
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

    // [spec:nshedit:req:core.incremental-render/test]
    #[test]
    fn plans_plain_incremental_transitions() {
        let mut state = configured(TerminalProfile::plain(), 1, 20);
        let prompt = Prompt::from("> ");
        let mut output = Vec::new();

        let mut present = |state: &mut State, line: &str, cursor: usize| {
            output.clear();
            let line = Text::from(line);
            state
                .present(
                    &prompt,
                    None,
                    &line,
                    line.index(cursor).unwrap(),
                    &mut output,
                )
                .unwrap();
            output.clone()
        };

        assert_eq!(present(&mut state, "", 0), b"> ");
        assert_eq!(present(&mut state, "h", 1), b"h");
        assert_eq!(present(&mut state, "hello", 5), b"ello");
        assert_eq!(present(&mut state, "hello", 0), b"\r> ");
        assert_eq!(present(&mut state, "hello", 5), b"hello");
        assert_eq!(present(&mut state, "hello", 4), b"\x08");
        assert_eq!(present(&mut state, "hell", 4), b"\r> hell \x08");
        assert_eq!(present(&mut state, "hell", 4), b"\r> hell");
        assert_eq!(present(&mut state, "hellworld", 9), b"world");
        assert_eq!(present(&mut state, "", 0), b"\r>          \r> ");
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
