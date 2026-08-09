//! Terminal I/O, line projection, and pointer-lifetime adaptation.

use super::*;

mod commands;
mod profile;
mod tty;

impl EditLine {
    pub(crate) fn set_terminal_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
        self.editor.set_terminal_mode(mode)
    }

    pub(crate) fn resize_display(&mut self) {
        let Some((rows, columns)) =
            with_borrowed_descriptor(self.descriptor(StreamKind::Input), terminal::screen_size)
                .and_then(Result::ok)
        else {
            return;
        };
        let Ok(size) = ScreenSize::new(rows, columns) else {
            return;
        };
        self.boundary
            .terminal
            .capabilities
            .set_size(size.rows(), size.columns());
        let _ = self.editor.resize_display(size);
    }

    pub(crate) fn beep(&mut self) {
        let mut bytes = Vec::new();
        if self.editor.beep(&mut bytes).is_ok() {
            let _ = crate::cstdio::write(self.stream(StreamKind::Output), &bytes);
        }
    }

    pub(crate) fn write_output(&self, bytes: &[u8]) -> io::Result<()> {
        crate::cstdio::write(self.stream(StreamKind::Output), bytes)
    }

    pub(crate) fn flush_output(&self) -> io::Result<()> {
        crate::cstdio::flush(self.stream(StreamKind::Output))
    }

    pub(crate) fn write_wide(&self, value: u32) -> io::Result<()> {
        match char::from_u32(value) {
            Some(character) => {
                let mut encoded = [0; 4];
                self.write_output(character.encode_utf8(&mut encoded).as_bytes())
            }
            None => self.write_output("\u{fffd}".as_bytes()),
        }
    }

    pub(crate) fn read_input(&self, output: &mut [u8]) -> io::Result<usize> {
        DescriptorInput::new(self.descriptor(StreamKind::Input)).read(output)
    }

    pub(crate) fn screen_size(&self) -> Option<ScreenSize> {
        let capabilities = &self.boundary.terminal.capabilities;
        ScreenSize::new(capabilities.rows, capabilities.columns).ok()
    }

    pub(crate) fn write_stream(&self, stream: StreamKind, bytes: &[u8]) {
        let _ = crate::cstdio::write(self.stream(stream), bytes);
    }

    pub(crate) fn move_cursor(&mut self, delta: c_int) -> c_int {
        let current = self.editor.cursor().get();
        let destination = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current
                .saturating_add(delta as usize)
                .min(self.editor.line().len())
        };
        let index = self
            .editor
            .line()
            .index(destination)
            .expect("the cursor destination is clamped to the line");
        let _ = self.editor.execute(Action::Move(Motion::Absolute(index)));
        c_int::try_from(destination).unwrap_or(c_int::MAX)
    }

    pub(crate) fn insert_wide(&mut self, input: &[u32]) -> c_int {
        if input.is_empty() {
            return -1;
        }
        let text = input
            .iter()
            .copied()
            .map(TextUnit::from_code_point)
            .collect();
        match self.editor.insert_untracked(text) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    pub(crate) fn replace_wide(&mut self, input: &[u32]) -> c_int {
        if input.is_empty() {
            return -1;
        }
        let text = input
            .iter()
            .copied()
            .map(TextUnit::from_code_point)
            .collect();
        match self.editor.replace_line_untracked(text) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    pub(crate) fn replace_line(&mut self, line: Text) -> bool {
        let span = self
            .editor
            .line()
            .span(0..self.editor.line().len())
            .expect("the complete line is a valid span");
        self.editor.replace(span, line).is_ok()
    }

    pub(crate) fn finish_accepted_line(&mut self, mut line: Text) -> bool {
        let cursor = self.editor.cursor().get();
        if line.as_units().last() != Some(&TextUnit::Scalar('\n')) {
            line.push(TextUnit::Scalar('\n'));
        }
        if !self.replace_line(line) {
            return false;
        }
        let position = self
            .editor
            .line()
            .index(cursor.min(self.editor.line().len()))
            .expect("a clamped accepted-line cursor is valid");
        self.editor
            .execute(Action::Move(Motion::Absolute(position)))
            .is_ok()
    }

    pub(crate) fn append_end_of_input(&mut self) -> bool {
        let mut line = self.editor.line().clone();
        line.push(TextUnit::Scalar('\u{4}'));
        self.replace_line(line)
    }

    pub(crate) fn delete_before_cursor(&mut self, count: c_int) {
        let Ok(count) = usize::try_from(count) else {
            return;
        };
        let cursor = self.editor.cursor().get();
        let Some(start) = cursor.checked_sub(count) else {
            return;
        };
        let span = self
            .editor
            .line()
            .span(start..cursor)
            .expect("cursor-derived deletion is within the line");
        let _ = self.editor.replace(span, Text::default());
    }

    pub(crate) fn kill_line(&mut self) {
        let _ = self.editor.execute(Action::Kill(EditTarget::Buffer));
    }

    pub(crate) fn delete_range(&mut self, start: c_int, end: c_int) -> c_int {
        let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
            return 0;
        };
        if end <= start || end >= self.editor.line().len() {
            return 0;
        }
        let requested = end - start;
        let copied = requested.min(self.editor.line().len() - end);
        let mut result = self.editor.line().as_units().to_vec();
        result.copy_within(end..end + copied, start);
        result.truncate(result.len() - copied);
        let span = self
            .editor
            .line()
            .span(0..self.editor.line().len())
            .expect("the complete line is a valid span");
        let _ = self.editor.replace(span, result.into_iter().collect());
        c_int::try_from(requested).unwrap_or(c_int::MAX)
    }

    pub(crate) fn narrow_conversion_mut(&mut self) -> &mut ConversionBuffer {
        &mut self.boundary.lines.narrow_conversion
    }

    pub(crate) fn narrow_line_ptr(&mut self) -> *mut LineInfo {
        core::ptr::from_mut(self.boundary.lines.narrow_line.as_mut())
    }

    pub(crate) fn publish_wide_line(&mut self) -> *const LineInfoW {
        self.boundary.lines.wide_storage.clear();
        self.boundary.lines.wide_storage.extend(
            self.editor
                .line()
                .as_units()
                .iter()
                .copied()
                .map(unit_to_wide),
        );
        let used = self.boundary.lines.wide_storage.len();
        self.boundary.lines.wide_storage.push(0);
        let buffer = self.boundary.lines.wide_storage.as_ptr();
        *self.boundary.lines.wide_line = LineInfoW {
            buffer,
            // SAFETY: the cursor is a checked logical boundary and the
            // storage has one element for every logical unit.
            cursor: unsafe { buffer.add(self.editor.cursor().get()) },
            // SAFETY: `used` is the one-past-used boundary and a terminator
            // was appended at that index.
            lastchar: unsafe { buffer.add(used) },
        };
        core::ptr::from_ref(self.boundary.lines.wide_line.as_ref())
    }

    pub(crate) fn terminal_name_ptr(&self) -> *const c_char {
        self.boundary.terminal.name.as_ptr()
    }

    pub(crate) fn publish_word_characters(&mut self) -> *const u32 {
        self.boundary
            .word_characters
            .as_ref()
            .map_or(core::ptr::null(), |characters| characters.as_ptr())
    }

    pub(crate) fn push_input(&mut self, input: &[u32]) -> bool {
        if self.boundary.pushback.len() >= 10 {
            return false;
        }
        self.boundary.pushback.push_back(
            input
                .iter()
                .copied()
                .map(TextUnit::from_code_point)
                .collect(),
        );
        true
    }

    pub(crate) fn pop_input(&mut self) -> Option<TextUnit> {
        loop {
            let entry = self.boundary.pushback.front_mut()?;
            let unit = entry.pop_front();
            if entry.is_empty() {
                self.boundary.pushback.pop_front();
            }
            if unit.is_some() {
                return unit;
            }
        }
    }

    pub(crate) fn stream(&self, kind: StreamKind) -> CFile {
        self.boundary.streams.endpoint(kind).file
    }

    pub(crate) fn descriptor(&self, kind: StreamKind) -> c_int {
        self.boundary.streams.endpoint(kind).descriptor
    }

    pub(crate) fn set_stream(&mut self, kind: StreamKind, stream: CFile, descriptor: c_int) {
        *self.boundary.streams.endpoint_mut(kind) = StreamEndpoint {
            file: stream,
            descriptor,
        };
        let mut terminal = self.boundary.terminal.state.borrow_mut();
        match kind {
            StreamKind::Input => terminal.input = descriptor,
            StreamKind::Output => terminal.output = descriptor,
            StreamKind::Diagnostics => {}
        }
    }

    pub(crate) fn is_tty(&self) -> bool {
        self.boundary.terminal.state.borrow().original.is_some()
    }

    pub(crate) fn control_eof(&self) -> u8 {
        self.boundary
            .terminal
            .state
            .borrow()
            .original
            .as_ref()
            .map_or_else(
                || ControlCharacter::EndOfFile.default_value(),
                |attributes| attributes.control_character(ControlCharacter::EndOfFile),
            )
    }

    pub(crate) fn control_reprint(&self) -> u8 {
        self.boundary
            .terminal
            .state
            .borrow()
            .original
            .as_ref()
            .map_or_else(
                || ControlCharacter::Reprint.default_value(),
                |attributes| attributes.control_character(ControlCharacter::Reprint),
            )
    }
}

#[cfg(test)]
mod tests;
