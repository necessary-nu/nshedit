//! Terminal I/O, line projection, and pointer-lifetime adaptation.

use super::*;

impl EditLine {
    pub(crate) fn set_terminal_name(&mut self, name: &str) -> c_int {
        let Ok(name) = CString::new(name) else {
            return -1;
        };
        let result = nshterm::TermInfo::from_name(name.to_str().unwrap_or("dumb"));
        let profile = result
            .as_ref()
            .map_or_else(|_| TerminalProfile::plain(), TerminalProfile::from_terminfo);
        let (rows, columns) = self
            .descriptor(0)
            .and_then(termios::window_size)
            .map(|(rows, columns)| (usize::from(rows), usize::from(columns)))
            .filter(|(rows, columns)| *rows != 0 && *columns != 0)
            .unwrap_or((24, 80));
        if let Ok(size) = ScreenSize::new(rows, columns) {
            self.native.configure_display(profile, size);
        }
        self.boundary.terminal_name = name;
        if result.is_ok() { 0 } else { -1 }
    }

    pub(crate) fn resize_display(&mut self) {
        let Some((rows, columns)) = self.descriptor(0).and_then(termios::window_size) else {
            return;
        };
        let Ok(size) = ScreenSize::new(usize::from(rows), usize::from(columns)) else {
            return;
        };
        let _ = self.native.resize_display(size);
    }

    pub(crate) fn set_terminal_mode(&mut self, mode: TerminalMode) -> io::Result<()> {
        self.native.set_terminal_mode(mode)
    }

    pub(crate) fn beep(&mut self) {
        let descriptor = self.descriptor(1).unwrap_or(-1);
        let mut output = DescriptorIo::new(descriptor);
        let _ = self.native.beep(&mut output);
    }

    pub(crate) fn write_output(&self, bytes: &[u8]) -> io::Result<()> {
        DescriptorIo::new(self.descriptor(1).unwrap_or(-1)).write_all(bytes)
    }

    pub(crate) fn flush_output(&self) -> io::Result<()> {
        crate::cstdio::flush(self.stream(1).unwrap_or(core::ptr::null_mut()))
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
        DescriptorIo::new(self.descriptor(0).unwrap_or(-1)).read(output)
    }

    pub(crate) fn screen_size(&self) -> Option<ScreenSize> {
        let (rows, columns) = self
            .descriptor(0)
            .and_then(termios::window_size)
            .map(|(rows, columns)| (usize::from(rows), usize::from(columns)))
            .filter(|(rows, columns)| *rows != 0 && *columns != 0)
            .unwrap_or((24, 80));
        ScreenSize::new(rows, columns).ok()
    }

    pub(crate) fn move_cursor(&mut self, delta: c_int) -> c_int {
        let current = self.native.cursor().get();
        let destination = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current
                .saturating_add(delta as usize)
                .min(self.native.line().len())
        };
        let index = self
            .native
            .line()
            .index(destination)
            .expect("the cursor destination is clamped to the line");
        let _ = self.native.execute(Action::Move(Motion::Absolute(index)));
        c_int::try_from(destination).unwrap_or(c_int::MAX)
    }

    pub(crate) fn insert_wide(&mut self, input: &[u32]) -> c_int {
        if input.is_empty() {
            return -1;
        }
        let text = input.iter().copied().map(TextUnit::from_wide).collect();
        match self.native.execute(Action::Insert(text)) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }

    pub(crate) fn replace_wide(&mut self, input: &[u32]) -> c_int {
        if input.is_empty() {
            return -1;
        }
        let text = input.iter().copied().map(TextUnit::from_wide).collect();
        let span = self
            .native
            .line()
            .span(0..self.native.line().len())
            .expect("the complete line is a valid span");
        match self.native.replace(span, text) {
            Ok(()) => 0,
            Err(_) => -1,
        }
    }

    pub(crate) fn replace_line(&mut self, line: Text) -> bool {
        let span = self
            .native
            .line()
            .span(0..self.native.line().len())
            .expect("the complete line is a valid span");
        self.native.replace(span, line).is_ok()
    }

    pub(crate) fn finish_accepted_line(&mut self, mut line: Text) -> bool {
        if line.as_units().last() != Some(&TextUnit::Scalar('\n')) {
            line.push(TextUnit::Scalar('\n'));
        }
        self.replace_line(line)
    }

    pub(crate) fn delete_before_cursor(&mut self, count: c_int) {
        let Ok(count) = usize::try_from(count) else {
            return;
        };
        let cursor = self.native.cursor().get();
        let Some(start) = cursor.checked_sub(count) else {
            return;
        };
        let span = self
            .native
            .line()
            .span(start..cursor)
            .expect("cursor-derived deletion is within the line");
        let _ = self.native.replace(span, Text::default());
    }

    pub(crate) fn kill_line(&mut self) {
        let _ = self.native.execute(Action::Kill(EditTarget::Buffer));
    }

    pub(crate) fn delete_range(&mut self, start: c_int, end: c_int) -> c_int {
        let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
            return 0;
        };
        if end <= start || end >= self.native.line().len() {
            return 0;
        }
        let requested = end - start;
        let copied = requested.min(self.native.line().len() - end);
        let mut result = self.native.line().as_units().to_vec();
        result.copy_within(end..end + copied, start);
        result.truncate(result.len() - copied);
        let span = self
            .native
            .line()
            .span(0..self.native.line().len())
            .expect("the complete line is a valid span");
        let _ = self.native.replace(span, result.into_iter().collect());
        c_int::try_from(requested).unwrap_or(c_int::MAX)
    }

    pub(crate) fn narrow_conversion_mut(&mut self) -> &mut ConversionBuffer {
        &mut self.boundary.narrow_conversion
    }

    pub(crate) fn narrow_line_ptr(&mut self) -> *mut LineInfo {
        core::ptr::from_mut(self.boundary.narrow_line.as_mut())
    }

    pub(crate) fn publish_wide_line(&mut self) -> *const LineInfoW {
        self.boundary.wide_storage.clear();
        self.boundary.wide_storage.extend(
            self.native
                .line()
                .as_units()
                .iter()
                .copied()
                .map(unit_to_wide),
        );
        let used = self.boundary.wide_storage.len();
        self.boundary.wide_storage.push(0);
        let buffer = self.boundary.wide_storage.as_ptr();
        *self.boundary.wide_line = LineInfoW {
            buffer,
            // SAFETY: the cursor is a checked logical boundary and the
            // storage has one element for every logical unit.
            cursor: unsafe { buffer.add(self.native.cursor().get()) },
            // SAFETY: `used` is the one-past-used boundary and a terminator
            // was appended at that index.
            lastchar: unsafe { buffer.add(used) },
        };
        core::ptr::from_ref(self.boundary.wide_line.as_ref())
    }

    pub(crate) fn terminal_name_ptr(&self) -> *const c_char {
        self.boundary.terminal_name.as_ptr()
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
        self.boundary
            .pushback
            .push_back(input.iter().copied().map(TextUnit::from_wide).collect());
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

    pub(crate) fn stream(&self, index: usize) -> Option<CFile> {
        self.boundary.streams.files.get(index).copied()
    }

    pub(crate) fn descriptor(&self, index: usize) -> Option<c_int> {
        self.boundary.streams.descriptors.get(index).copied()
    }

    pub(crate) fn set_stream(&mut self, index: usize, stream: CFile, descriptor: c_int) -> bool {
        let (Some(file), Some(fd)) = (
            self.boundary.streams.files.get_mut(index),
            self.boundary.streams.descriptors.get_mut(index),
        ) else {
            return false;
        };
        *file = stream;
        *fd = descriptor;
        let mut terminal = self.boundary.terminal.borrow_mut();
        match index {
            0 => terminal.input = descriptor,
            1 => terminal.output = descriptor,
            _ => {}
        }
        true
    }

    pub(crate) fn is_tty(&self) -> bool {
        self.boundary.terminal.borrow().original.is_some()
    }

    pub(crate) fn control_eof(&self) -> u8 {
        self.boundary
            .terminal
            .borrow()
            .original
            .as_ref()
            .map_or(termios::CEOF, |attributes| attributes.c_cc[termios::VEOF])
    }

    pub(crate) fn control_reprint(&self) -> u8 {
        self.boundary
            .terminal
            .borrow()
            .original
            .as_ref()
            .map_or(termios::CREPRINT, |attributes| {
                attributes.c_cc[termios::VREPRINT]
            })
    }
}

#[cfg(test)]
mod tests;
