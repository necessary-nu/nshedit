//! Private native screen state and safe terminal emission.

mod capability;
mod layout;

use std::fmt;
use std::io::{self, Write};

use nshterm::parm::{Error as ExpansionError, Param, Variables};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::domain::{Error, Prompt, Screen, ScreenPosition, ScreenSize, Text, TextIndex, TextUnit};

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
    RegionAddressUnavailable { rows: usize },
    /// A failed region setup left no trustworthy physical origin.
    RegionOriginUnavailable,
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
            Self::RegionAddressUnavailable { rows } => write!(
                formatter,
                "terminal cannot position a frame using {rows} rows"
            ),
            Self::RegionOriginUnavailable => formatter.write_str(
                "terminal region origin is unavailable; reconfigure the display before rendering",
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
            | Self::RegionAddressUnavailable { .. }
            | Self::RegionOriginUnavailable => None,
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
    region: PhysicalRegion,
    redraw: bool,
    damaged: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum RegionOrigin {
    #[default]
    Unanchored,
    Saved,
    Lost,
}

/// Physical rows owned by the editor from its saved current-line anchor.
///
/// `extent` is a high-water mark: shrinking a frame or the reported terminal
/// height does not release rows that may still contain editor output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PhysicalRegion {
    origin: RegionOrigin,
    extent: usize,
}

struct RegionPlan {
    bytes: Vec<u8>,
    variables: Variables,
    region: PhysicalRegion,
    establishes_origin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FinishEcho {
    bytes: Box<[u8]>,
    columns: usize,
}

impl FinishEcho {
    pub(super) fn new(unit: TextUnit) -> Self {
        match unit {
            TextUnit::Scalar(character) if (character as u32) <= 0xff && character.is_control() => {
                let visual = if character == '\u{7f}' {
                    '?'
                } else {
                    char::from_u32((character as u32) | 0x40).unwrap_or('\u{fffd}')
                };
                let mut encoded = [0; 4];
                let visual = visual.encode_utf8(&mut encoded);
                let mut bytes = Vec::with_capacity(1 + visual.len());
                bytes.push(b'^');
                bytes.extend_from_slice(visual.as_bytes());
                Self {
                    bytes: bytes.into_boxed_slice(),
                    columns: 1 + visual.width(),
                }
            }
            TextUnit::Scalar(character) => {
                let mut encoded = [0; 4];
                Self {
                    bytes: character.encode_utf8(&mut encoded).as_bytes().into(),
                    columns: character.width().unwrap_or(1),
                }
            }
            TextUnit::RawByte(byte) if byte.is_ascii_control() => Self {
                bytes: Box::from([b'^', if byte == 0x7f { b'?' } else { byte | 0x40 }]),
                columns: 2,
            },
            TextUnit::RawByte(byte) => Self {
                bytes: Box::from([byte]),
                columns: 1,
            },
            TextUnit::OpaqueCodePoint(_) => Self {
                bytes: "\u{fffd}".as_bytes().into(),
                columns: 1,
            },
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) const fn columns(&self) -> usize {
        self.columns
    }
}

#[derive(Clone, Copy)]
struct Committed<'a> {
    screen: &'a Screen,
    rows: &'a [layout::Row],
    cursor: ScreenPosition,
    region: PhysicalRegion,
    redraw: bool,
    damaged: bool,
}

impl State {
    pub(super) fn configure(&mut self, profile: TerminalProfile, size: ScreenSize) {
        let Some(configured) = &mut self.configured else {
            self.configured = Some(Configured {
                profile,
                screen: Screen::new(size),
                rows: Vec::new(),
                cursor: size
                    .position(0, 0)
                    .expect("a validated screen has an origin"),
                rows_used: 0,
                variables: Variables::new(),
                region: PhysicalRegion::default(),
                redraw: false,
                damaged: false,
            });
            return;
        };

        configured.profile = profile;
        configured.variables = Variables::new();
        if configured.region.origin == RegionOrigin::Lost {
            configured.region = PhysicalRegion::default();
            configured.screen = Screen::new(size);
            configured.rows.clear();
            configured.cursor = size
                .position(0, 0)
                .expect("a validated screen has an origin");
            configured.rows_used = 0;
            configured.redraw = false;
            configured.damaged = false;
            return;
        }
        if configured.screen.size() != size {
            configured.screen = Screen::new(size);
            configured.rows.clear();
            configured.cursor = size
                .position(0, 0)
                .expect("a validated screen has an origin");
            configured.rows_used = 0;
        }
        configured.redraw = false;
        configured.damaged = true;
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
        let mut plan = Self::plan_region(configured, frame.rows_used())?;
        let setup_cursor = plan.establishes_origin.then(|| {
            configured
                .screen
                .size()
                .position(0, 0)
                .expect("a configured screen has an origin")
        });
        let committed = Committed {
            screen: &configured.screen,
            rows: &configured.rows,
            cursor: setup_cursor.unwrap_or(configured.cursor),
            region: plan.region,
            redraw: configured.redraw && !plan.establishes_origin,
            damaged: configured.damaged || plan.establishes_origin,
        };
        let frame_bytes = encode(&configured.profile, &frame, committed, &mut plan.variables)?;
        plan.bytes.extend_from_slice(&frame_bytes);

        if let Err(error) = output.write_all(&plan.bytes).and_then(|()| output.flush()) {
            let configured = self
                .configured
                .as_mut()
                .expect("display configuration cannot disappear during a write");
            if plan.establishes_origin {
                configured.region.origin = RegionOrigin::Lost;
            }
            configured.redraw = false;
            configured.damaged = true;
            return Err(RenderError::Io(error));
        }

        let rows_used = frame.rows_used();
        let summary = RenderSummary {
            size: frame.screen.size(),
            cursor: frame.cursor,
            rows_used,
            bytes_written: plan.bytes.len(),
        };
        let configured = self
            .configured
            .as_mut()
            .expect("display configuration cannot disappear during a write");
        configured.screen = frame.screen;
        configured.rows = frame.rows;
        configured.cursor = frame.cursor;
        configured.rows_used = rows_used;
        configured.variables = plan.variables;
        configured.region = plan.region;
        configured.redraw = false;
        configured.damaged = false;
        Ok(summary)
    }

    fn plan_region(
        configured: &Configured,
        required_rows: usize,
    ) -> Result<RegionPlan, RenderError> {
        if !configured.profile.has_relative_region_addressing() {
            return Ok(RegionPlan {
                bytes: Vec::new(),
                variables: configured.variables.clone(),
                region: configured.region,
                establishes_origin: false,
            });
        }
        if configured.region.origin == RegionOrigin::Lost {
            return Err(RenderError::RegionOriginUnavailable);
        }
        if configured.region.origin == RegionOrigin::Saved
            && configured.region.extent >= required_rows
        {
            return Ok(RegionPlan {
                bytes: Vec::new(),
                variables: configured.variables.clone(),
                region: configured.region,
                establishes_origin: false,
            });
        }

        let mut bytes = Vec::new();
        let mut variables = configured.variables.clone();
        let prior_extent = configured.region.extent;
        if configured.region.origin == RegionOrigin::Saved {
            append_required_capability(
                &configured.profile,
                &mut bytes,
                CapabilityKind::RestoreCursor,
                1,
                &mut variables,
            )?;
            for _ in 1..prior_extent {
                append_required_capability(
                    &configured.profile,
                    &mut bytes,
                    CapabilityKind::CursorDown,
                    1,
                    &mut variables,
                )?;
            }
        } else {
            append_carriage_return(&configured.profile, &mut bytes, &mut variables)?;
        }

        let occupied = prior_extent.max(1);
        for _ in occupied..required_rows {
            bytes.extend_from_slice(b"\r\n");
        }
        for _ in 1..required_rows {
            append_required_capability(
                &configured.profile,
                &mut bytes,
                CapabilityKind::CursorUp,
                1,
                &mut variables,
            )?;
        }
        append_required_capability(
            &configured.profile,
            &mut bytes,
            CapabilityKind::SaveCursor,
            1,
            &mut variables,
        )?;

        Ok(RegionPlan {
            bytes,
            variables,
            region: PhysicalRegion {
                origin: RegionOrigin::Saved,
                extent: required_rows.max(prior_extent),
            },
            establishes_origin: true,
        })
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

    // [spec:nshedit:req:core.incremental-render+3]
    pub(super) fn finish_line(
        &mut self,
        echo: Option<&FinishEcho>,
        output: &mut dyn Write,
    ) -> Result<usize, RenderError> {
        let configured = self
            .configured
            .as_ref()
            .ok_or(RenderError::DisplayNotConfigured)?;
        if configured.region.origin == RegionOrigin::Lost {
            return Err(RenderError::RegionOriginUnavailable);
        }

        let occupied_extent = configured.region.extent.max(configured.rows_used);
        let echo_extent = echo.map_or(occupied_extent, |echo| {
            if echo.columns() == 0 {
                return occupied_extent;
            }
            let last_column = configured
                .cursor
                .column()
                .saturating_add(echo.columns().saturating_sub(1));
            configured
                .cursor
                .row()
                .saturating_add(last_column / configured.screen.size().columns())
                .saturating_add(1)
                .max(occupied_extent)
        });

        let mut plan = if configured.profile.has_relative_region_addressing() {
            Self::plan_region(configured, echo_extent.max(1))?
        } else {
            if occupied_extent > 1 {
                return Err(RenderError::RegionAddressUnavailable {
                    rows: occupied_extent,
                });
            }
            RegionPlan {
                bytes: Vec::new(),
                variables: configured.variables.clone(),
                region: configured.region,
                establishes_origin: false,
            }
        };

        if let Some(echo) = echo {
            if configured.profile.has_relative_region_addressing()
                && (plan.establishes_origin || configured.damaged)
            {
                append_required_capability(
                    &configured.profile,
                    &mut plan.bytes,
                    CapabilityKind::RestoreCursor,
                    1,
                    &mut plan.variables,
                )?;
                let mut cursor = Some((0, 0));
                move_addressed_cursor(
                    &configured.profile,
                    &mut plan.bytes,
                    &mut cursor,
                    configured.cursor.row(),
                    configured.cursor.column(),
                    configured.rows.get(configured.cursor.row()),
                    &mut plan.variables,
                )?;
            }
            plan.bytes.extend_from_slice(echo.as_bytes());
        }

        if plan.region.origin == RegionOrigin::Saved
            && configured.profile.has_relative_region_addressing()
        {
            append_required_capability(
                &configured.profile,
                &mut plan.bytes,
                CapabilityKind::RestoreCursor,
                1,
                &mut plan.variables,
            )?;
            for _ in 1..plan.region.extent {
                append_required_capability(
                    &configured.profile,
                    &mut plan.bytes,
                    CapabilityKind::CursorDown,
                    1,
                    &mut plan.variables,
                )?;
            }
        }
        plan.bytes.push(b'\n');

        if let Err(error) = output.write_all(&plan.bytes).and_then(|()| output.flush()) {
            let configured = self
                .configured
                .as_mut()
                .expect("display configuration cannot disappear during a write");
            if plan.establishes_origin {
                configured.region.origin = RegionOrigin::Lost;
            }
            configured.redraw = false;
            configured.damaged = true;
            return Err(RenderError::Io(error));
        }

        let configured = self
            .configured
            .as_mut()
            .expect("display configuration cannot disappear during a write");
        let size = configured.screen.size();
        configured.screen = Screen::new(size);
        configured.rows.clear();
        configured.cursor = size
            .position(0, 0)
            .expect("a validated screen has an origin");
        configured.rows_used = 0;
        configured.variables = plan.variables;
        configured.region = PhysicalRegion::default();
        configured.redraw = false;
        configured.damaged = false;
        Ok(plan.bytes.len())
    }
}

// [spec:nshedit:req:core.incremental-render+3]
fn encode(
    profile: &TerminalProfile,
    frame: &Frame,
    committed: Committed<'_>,
    variables: &mut Variables,
) -> Result<Vec<u8>, RenderError> {
    let mut output = Vec::new();
    if profile.has_relative_region_addressing() {
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
    let columns = frame.screen.size().columns();
    let mut terminal_cursor = if committed.damaged {
        append_required_capability(profile, output, CapabilityKind::RestoreCursor, 1, variables)?;
        Some((0, 0))
    } else {
        Some((committed.cursor.row(), committed.cursor.column()))
    };
    let rows = if committed.damaged {
        committed.region.extent.max(frame.rows_used())
    } else {
        frame.rows_used().max(committed.rows.len())
    };
    for row in 0..rows {
        let current = frame.rows.get(row);
        let previous = committed.rows.get(row);
        let rewrite = committed.redraw || committed.damaged;
        let start = if rewrite {
            0
        } else {
            let Some(start) = first_changed_column(frame, committed, row, current, previous) else {
                continue;
            };
            if start < columns { start } else { 0 }
        };
        move_addressed_cursor(
            profile,
            output,
            &mut terminal_cursor,
            row,
            start,
            current.or(previous),
            variables,
        )?;
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
        if used < columns && (rewrite || previous_used > used) {
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
        move_addressed_cursor(
            profile,
            output,
            &mut terminal_cursor,
            frame.cursor.row(),
            frame.cursor.column(),
            frame.rows.get(frame.cursor.row()),
            variables,
        )?;
    }
    Ok(())
}

fn move_addressed_cursor(
    profile: &TerminalProfile,
    output: &mut Vec<u8>,
    cursor: &mut Option<(usize, usize)>,
    target_row: usize,
    target_column: usize,
    displayed_row: Option<&layout::Row>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    let (mut row, mut column) = if let Some(position) = *cursor {
        position
    } else {
        append_required_capability(profile, output, CapabilityKind::RestoreCursor, 1, variables)?;
        (0, 0)
    };

    if row != target_row {
        if column != 0 {
            append_carriage_return(profile, output, variables)?;
            column = 0;
        }
        let (kind, distance) = if row < target_row {
            (CapabilityKind::CursorDown, target_row - row)
        } else {
            (CapabilityKind::CursorUp, row - target_row)
        };
        for _ in 0..distance {
            append_required_capability(profile, output, kind, 1, variables)?;
        }
        row = target_row;
    }

    match target_column.cmp(&column) {
        std::cmp::Ordering::Less => {
            append_cursor_left(profile, output, column - target_column, variables)?;
        }
        std::cmp::Ordering::Greater if profile.has_cursor_right() => {
            for _ in column..target_column {
                append_required_capability(
                    profile,
                    output,
                    CapabilityKind::CursorRight,
                    1,
                    variables,
                )?;
            }
        }
        std::cmp::Ordering::Greater => {
            if let Some(content) = displayed_row {
                append_atom_range(output, content, column, target_column, false);
            } else {
                output.extend(std::iter::repeat_n(b' ', target_column - column));
            }
        }
        std::cmp::Ordering::Equal => {}
    }
    *cursor = Some((row, target_column));
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
    if frame.rows_used() > 1
        || committed.rows.len() > 1
        || committed.region.extent > 1
        || frame.cursor.row() > 0
    {
        return Err(RenderError::RegionAddressUnavailable {
            rows: frame
                .rows_used()
                .max(committed.rows.len())
                .max(committed.region.extent),
        });
    }
    let row = &frame.rows[0];
    if committed.damaged {
        return append_plain_redraw(profile, frame, 0, true, output, variables);
    }
    let Some(previous) = committed.rows.first() else {
        append_atoms(output, row);
        return append_plain_reposition(
            profile,
            row,
            row.used(),
            frame.cursor.column(),
            output,
            variables,
        );
    };

    if committed.redraw {
        return append_plain_redraw(profile, frame, previous.used(), false, output, variables);
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

    append_plain_redraw(profile, frame, previous.used(), false, output, variables)
}

fn append_plain_redraw(
    profile: &TerminalProfile,
    frame: &Frame,
    previous_used: usize,
    damaged: bool,
    output: &mut Vec<u8>,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    append_carriage_return(profile, output, variables)?;
    let row = &frame.rows[0];
    append_atoms(output, row);
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
            previous_used.max(row.used())
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
    output.extend_from_slice(row.literal_state());
    let mut column = 0usize;
    for atom in &row.atoms {
        match atom {
            Atom::Literal(bytes) => {
                if column < start {
                    output.extend_from_slice(bytes);
                    continue;
                }
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

fn append_required_capability(
    profile: &TerminalProfile,
    output: &mut Vec<u8>,
    capability: CapabilityKind,
    affected_lines: usize,
    variables: &mut Variables,
) -> Result<(), RenderError> {
    if append_capability(profile, output, capability, &[], affected_lines, variables)? {
        Ok(())
    } else {
        Err(RenderError::RegionAddressUnavailable {
            rows: affected_lines,
        })
    }
}

fn append_atoms(output: &mut Vec<u8>, row: &layout::Row) {
    output.extend_from_slice(row.literal_state());
    for atom in &row.atoms {
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
    use crate::domain::{Prompt, ScreenCell, TerminalLiteral};

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

    // [spec:nshedit:req:core.incremental-render+3/test]
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

        assert_eq!(present(&mut state, "", 0), b"\r\x1b7\x1b8> \x1b[K");
        assert_eq!(present(&mut state, "h", 1), b"h");
        assert_eq!(present(&mut state, "hello", 5), b"ello");
        assert_eq!(
            present(&mut state, "hallo", 2),
            b"\x08\x08\x08\x08allo\x08\x08\x08"
        );
        assert_eq!(present(&mut state, "hall", 4), b"\x1b[C\x1b[C\x1b[K");
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn replays_multiline_prompt_literal_state() {
        const RED: &[u8] = b"\x1b[31m";
        const RESET: &[u8] = b"\x1b[0m";

        let prompt = |second_row: &str| {
            let mut prompt = Prompt::default();
            prompt.push_literal(TerminalLiteral::from(RED));
            prompt.push_text(format!("r\n{second_row}"));
            prompt.push_literal(TerminalLiteral::from(RESET));
            prompt.push_text(">");
            prompt
        };
        let mut state = configured(TerminalProfile::ansi(), 3, 8);
        let line = Text::default();
        state
            .present(
                &prompt("ab"),
                None,
                &line,
                TextIndex::START,
                &mut Vec::new(),
            )
            .unwrap();
        assert_eq!(
            state.configured.as_ref().unwrap().rows[1].literal_state(),
            RED
        );

        let mut output = Vec::new();
        state
            .present(&prompt("aB"), None, &line, TextIndex::START, &mut output)
            .unwrap();

        assert_eq!(output, b"\x08\x08\x1b[31mB\x1b[0m>");
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn anchors_region_at_current_line() {
        let mut state = configured(TerminalProfile::ansi(), 3, 4);
        let line = Text::from("abcd");
        let prefix = b"host output on earlier rows\r\n";
        let mut output = prefix.to_vec();
        let summary = state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut output,
            )
            .unwrap();

        assert_eq!(&output[..prefix.len()], prefix);
        assert_eq!(
            &output[prefix.len()..],
            b"\r\r\n\x1b[A\x1b7\x1b8>abc\x1b8\x1b[Bd\x1b[K"
        );
        assert_eq!(summary.rows_used(), 2);
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn finish_descends_past_wrapped_region() {
        let mut state = configured(TerminalProfile::ansi(), 5, 4);
        let line = Text::from("abcde");
        state
            .present(
                &Prompt::from("p\n> "),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();
        assert_eq!(state.configured.as_ref().unwrap().region.extent, 3);

        let mut output = Vec::new();
        assert_eq!(state.finish_line(None, &mut output).unwrap(), 9);

        assert_eq!(output, b"\x1b8\x1b[B\x1b[B\n");
        let configured = state.configured.as_ref().unwrap();
        assert_eq!(configured.region, PhysicalRegion::default());
        assert_eq!(configured.rows_used, 0);
        assert!(configured.rows.is_empty());
        assert!(
            configured
                .screen
                .cells()
                .iter()
                .all(|cell| cell == &ScreenCell::Blank)
        );
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn failed_finish_retains_region() {
        let mut state = configured(TerminalProfile::ansi(), 5, 4);
        let line = Text::from("abcd");
        state
            .present(
                &Prompt::from("p\n> "),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();
        let before_screen = state.screen().unwrap().clone();
        let before_cursor = state.cursor();
        let before_region = state.configured.as_ref().unwrap().region;

        let mut writer = FailingWriter {
            limit: 3,
            written: 0,
            fail_flush: false,
        };
        let echo = FinishEcho::new(TextUnit::Scalar('\u{4}'));
        assert!(matches!(
            state.finish_line(Some(&echo), &mut writer),
            Err(RenderError::Io(_))
        ));

        let configured = state.configured.as_ref().unwrap();
        assert_eq!(configured.screen, before_screen);
        assert_eq!(state.cursor(), before_cursor);
        assert_eq!(configured.region, before_region);
        assert!(configured.damaged);

        let mut output = Vec::new();
        state.finish_line(Some(&echo), &mut output).unwrap();
        assert_eq!(
            output,
            b"\x1b8\x1b[B\x1b[B\x1b[C\x1b[C^D\x1b8\x1b[B\x1b[B\n"
        );
        assert_eq!(
            state.configured.as_ref().unwrap().region,
            PhysicalRegion::default()
        );
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn failed_reanchor_loses_origin() {
        let mut state = configured(TerminalProfile::ansi(), 5, 4);
        let line = Text::default();
        state
            .present(
                &Prompt::from("p\n>>>"),
                None,
                &line,
                line.index(0).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();
        let before_screen = state.screen().unwrap().clone();
        let before_extent = state.configured.as_ref().unwrap().region.extent;

        let mut writer = FailingWriter {
            limit: 3,
            written: 0,
            fail_flush: false,
        };
        let echo = FinishEcho::new(TextUnit::Scalar('\u{4}'));
        assert!(matches!(
            state.finish_line(Some(&echo), &mut writer),
            Err(RenderError::Io(_))
        ));

        let configured = state.configured.as_ref().unwrap();
        assert_eq!(configured.screen, before_screen);
        assert_eq!(configured.region.extent, before_extent);
        assert_eq!(configured.region.origin, RegionOrigin::Lost);
        assert!(configured.damaged);
        assert!(matches!(
            state.finish_line(Some(&echo), &mut Vec::new()),
            Err(RenderError::RegionOriginUnavailable)
        ));
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn damage_stays_within_owned_rows() {
        let mut state = configured(TerminalProfile::ansi(), 4, 4);
        let line = Text::from("abcd");
        state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();

        state.damage();
        let mut output = Vec::new();
        state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut output,
            )
            .unwrap();

        assert_eq!(output, b"\x1b8>abc\x1b8\x1b[Bd\x1b[K");
        assert_eq!(
            output
                .windows(b"\x1b[K".len())
                .filter(|window| *window == b"\x1b[K")
                .count(),
            1
        );
        assert!(!output.windows(4).any(|window| window == b"\x1b[2J"));
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn resize_repairs_high_water_rows() {
        let mut state = configured(TerminalProfile::ansi(), 3, 4);
        let line = Text::from("abcd");
        state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();
        state.resize(ScreenSize::new(1, 8).unwrap()).unwrap();
        assert_eq!(state.configured.as_ref().unwrap().region.extent, 2);

        let mut output = Vec::new();
        state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut output,
            )
            .unwrap();

        assert_eq!(
            output,
            b"\x1b8>abcd\x1b[K\r\x1b[B\x1b[K\x1b[A\x1b[C\x1b[C\x1b[C\x1b[C\x1b[C"
        );
        assert_eq!(
            output
                .windows(b"\x1b[K".len())
                .filter(|window| *window == b"\x1b[K")
                .count(),
            2
        );
        assert!(!output.windows(4).any(|window| window == b"\x1b[2J"));
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn reconfigure_preserves_multiline_region() {
        let size = ScreenSize::new(3, 4).unwrap();
        let mut state = configured(TerminalProfile::ansi(), 3, 4);
        let line = Text::from("abcd");
        state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();

        state.configure(TerminalProfile::plain(), size);
        let region = state.configured.as_ref().unwrap().region;
        assert_eq!(region.origin, RegionOrigin::Saved);
        assert_eq!(region.extent, 2);
        let mut output = Vec::new();
        let error = state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut output,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            RenderError::RegionAddressUnavailable { rows: 2 }
        ));
        assert!(output.is_empty());
        assert_eq!(state.configured.as_ref().unwrap().region.extent, 2);

        state.configure(TerminalProfile::ansi(), size);
        state
            .present(
                &Prompt::from(">"),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut output,
            )
            .unwrap();
        assert_eq!(output, b"\x1b8>abc\x1b8\x1b[Bd\x1b[K");
        assert_eq!(state.configured.as_ref().unwrap().region, region);
    }

    // [spec:nshedit:req:core.incremental-render+3/test]
    #[test]
    fn plain_profile_finishes_saved_row() {
        let size = ScreenSize::new(2, 8).unwrap();
        let mut state = configured(TerminalProfile::ansi(), 2, 8);
        let line = Text::from("line");
        state
            .present(
                &Prompt::from("> "),
                None,
                &line,
                line.index(line.len()).unwrap(),
                &mut Vec::new(),
            )
            .unwrap();
        assert_eq!(state.configured.as_ref().unwrap().region.extent, 1);

        state.configure(TerminalProfile::plain(), size);
        let mut output = Vec::new();
        state.finish_line(None, &mut output).unwrap();

        assert_eq!(output, b"\n");
        assert_eq!(
            state.configured.as_ref().unwrap().region,
            PhysicalRegion::default()
        );
    }

    #[test]
    fn failed_anchor_setup_marks_origin_lost() {
        let mut state = configured(TerminalProfile::ansi(), 2, 10);
        let before = state.screen().unwrap().clone();
        let mut writer = FailingWriter {
            limit: 1,
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
        assert_eq!(
            state.configured.as_ref().unwrap().region.origin,
            RegionOrigin::Lost
        );

        let error = state
            .present(
                &Prompt::default(),
                None,
                &Text::from("changed"),
                TextIndex::START,
                &mut Vec::new(),
            )
            .unwrap_err();
        assert!(matches!(error, RenderError::RegionOriginUnavailable));
    }

    #[test]
    fn failed_frame_preserves_committed_state() {
        let mut state = configured(TerminalProfile::ansi(), 2, 10);
        state
            .present(
                &Prompt::default(),
                None,
                &Text::from("before"),
                TextIndex::START,
                &mut Vec::new(),
            )
            .unwrap();
        let before = state.screen().unwrap().clone();
        let region = state.configured.as_ref().unwrap().region;

        let mut writer = FailingWriter {
            limit: 3,
            written: 0,
            fail_flush: false,
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
        assert_eq!(
            state.configured.as_ref().unwrap().region.origin,
            region.origin
        );

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

    // [spec:nshedit:req:core.incremental-render+3/test]
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
            RenderError::RegionAddressUnavailable { .. }
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
