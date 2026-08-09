//! Per-handle capability state and native profile configuration.

use super::*;
use nshterm::parser::names::{
    BOOL_CODES, BOOL_NAMES, NUMBER_CODES, NUMBER_NAMES, STRING_CODES, STRING_NAMES,
};

#[derive(Clone, Copy)]
pub(super) enum CapabilityValueKind {
    Boolean,
    Number,
}

fn capability_name<'a>(code: &str, codes: &[&str], names: &'a [&str]) -> Option<&'a str> {
    codes
        .iter()
        .position(|candidate| *candidate == code)
        .map(|index| names[index])
}

pub(super) fn string_capability_name(code: &str) -> Option<&'static str> {
    capability_name(code, STRING_CODES, STRING_NAMES)
}

pub(super) fn local_value_capability(code: &str) -> Option<(CapabilityValueKind, &'static str)> {
    let kind = match code {
        "am" | "pt" | "km" | "xn" => CapabilityValueKind::Boolean,
        "li" | "co" | "xt" | "MT" => CapabilityValueKind::Number,
        _ => return None,
    };
    let name = match kind {
        CapabilityValueKind::Boolean => capability_name(code, BOOL_CODES, BOOL_NAMES),
        CapabilityValueKind::Number => capability_name(code, NUMBER_CODES, NUMBER_NAMES)
            .or_else(|| capability_name(code, BOOL_CODES, BOOL_NAMES)),
    }?;
    Some((kind, name))
}

impl TerminalCapabilities {
    // [spec:nshedit:req:abi.terminal-session]
    pub(in crate::adapter) fn new(
        name: &str,
        entry: Option<&nshterm::TermInfo>,
        window_size: Option<(usize, usize)>,
    ) -> Self {
        let mut bools = HashMap::new();
        let mut numbers = HashMap::new();
        let mut strings = HashMap::new();
        if let Some(entry) = entry {
            bools.extend(entry.bools.iter().map(|(&key, &value)| (key, value)));
            numbers.extend(
                entry
                    .numbers
                    .iter()
                    .map(|(&key, &value)| (key, c_int::try_from(value).unwrap_or(c_int::MAX))),
            );
            strings.extend(entry.strings.iter().map(|(&key, value)| {
                let end = value
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(value.len());
                let value = CString::new(&value[..end]).expect("the first NUL was removed");
                (key, value)
            }));
            if let Some(value) = entry.termcap_string("me") {
                let end = value
                    .iter()
                    .position(|&byte| byte == 0)
                    .unwrap_or(value.len());
                let value = CString::new(&value[..end]).expect("the first NUL was removed");
                strings.insert("sgr0", value);
            }
        }
        let database_rows = numbers
            .get("lines")
            .copied()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|&value| value >= 1)
            .unwrap_or(24);
        let database_columns = numbers
            .get("cols")
            .copied()
            .and_then(|value| usize::try_from(value).ok())
            .filter(|&value| value >= 2)
            .unwrap_or(80);
        let (rows, columns) = window_size.unwrap_or((database_rows, database_columns));
        numbers.insert("lines", c_int::try_from(rows).unwrap_or(c_int::MAX));
        numbers.insert("cols", c_int::try_from(columns).unwrap_or(c_int::MAX));
        let mut capabilities = Self {
            name: name.to_owned(),
            bools,
            numbers,
            strings,
            derived_destructive_tabs: false,
            derived_meta_extension: false,
            rows,
            columns,
        };
        capabilities.refresh_derived_flags();
        capabilities
    }

    pub(in crate::adapter) fn profile(&self, baud_rate: Option<BaudRate>) -> TerminalProfile {
        let entry = nshterm::TermInfo {
            names: vec![self.name.clone()],
            bools: self.bools.clone(),
            numbers: self
                .numbers
                .iter()
                .filter_map(|(&key, &value)| u32::try_from(value).ok().map(|value| (key, value)))
                .collect(),
            strings: self
                .strings
                .iter()
                .map(|(&key, value)| (key, value.as_bytes().to_vec()))
                .collect(),
        };
        TerminalProfile::from_terminfo(&entry).with_baud_rate(baud_rate)
    }

    pub(super) fn set_size(&mut self, rows: usize, columns: usize) {
        self.rows = rows.max(1);
        self.columns = columns.max(2);
        self.numbers
            .insert("lines", c_int::try_from(self.rows).unwrap_or(c_int::MAX));
        self.numbers
            .insert("cols", c_int::try_from(self.columns).unwrap_or(c_int::MAX));
    }

    pub(super) fn boolean(&self, code: &str) -> bool {
        capability_name(code, BOOL_CODES, BOOL_NAMES)
            .and_then(|name| self.bools.get(name))
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn number(&self, code: &str) -> c_int {
        let Some((_, name)) = local_value_capability(code) else {
            return 0;
        };
        self.numbers
            .get(name)
            .copied()
            .or_else(|| self.bools.get(name).copied().map(c_int::from))
            .unwrap_or(0)
    }

    pub(in crate::adapter) fn string(&self, code: &str) -> Option<&CStr> {
        string_capability_name(code)
            .and_then(|name| self.strings.get(name))
            .map(CString::as_c_str)
    }

    pub(super) fn refresh_derived_flags(&mut self) {
        self.derived_destructive_tabs = self.number("xt") != 0;
        self.derived_meta_extension = self.number("MT") != 0;
    }

    fn preserve_failed_lookup_values(&mut self, previous: &Self) {
        for code in ["am", "xn"] {
            if let Some(name) = capability_name(code, BOOL_CODES, BOOL_NAMES) {
                self.bools.insert(name, previous.boolean(code));
            }
        }
        let meta_extension = previous.number("MT");
        for code in ["MT", "xt"] {
            if let Some((_, name)) = local_value_capability(code) {
                self.numbers.insert(name, meta_extension);
                if BOOL_NAMES.contains(&name) {
                    self.bools.insert(name, meta_extension != 0);
                }
            }
        }
        self.refresh_derived_flags();
    }
}

impl EditLine {
    pub(crate) fn set_terminal_name(&mut self, name: &str) -> c_int {
        let Ok(name) = CString::new(name) else {
            return -1;
        };
        if name.as_bytes() == b"emacs" {
            self.set_editing_enabled(false);
        }
        let result = nshterm::TermInfo::from_name(name.to_str().unwrap_or("dumb"));
        let window_size = self
            .descriptor(0)
            .and_then(termios::window_size)
            .map(|(rows, columns)| (usize::from(rows), usize::from(columns)))
            .filter(|(rows, columns)| *rows != 0 && *columns != 0);
        let mut capabilities = TerminalCapabilities::new(
            name.to_str().unwrap_or("dumb"),
            result.as_ref().ok(),
            window_size,
        );
        if let Err(error) = &result {
            capabilities.preserve_failed_lookup_values(&self.boundary.terminal_capabilities);
            self.report_terminal_lookup_failure(name.as_c_str(), error);
        }
        self.boundary.terminal_capabilities = capabilities;
        self.boundary.terminal_name = name;
        self.configure_terminal_display();
        self.install_terminal_bindings();
        if result.is_ok() { 0 } else { -1 }
    }

    pub(in crate::adapter) fn report_terminal_lookup_failure(
        &self,
        name: &CStr,
        error: &nshterm::Error,
    ) {
        let mut diagnostic = if matches!(error, nshterm::Error::TerminfoEntryNotFound) {
            let mut message = b"No entry for terminal type \"".to_vec();
            message.extend_from_slice(name.to_bytes());
            message.extend_from_slice(b"\";\n");
            message
        } else {
            b"Cannot read termcap database;\n".to_vec()
        };
        diagnostic.extend_from_slice(b"using dumb terminal settings.\n");
        let _ = crate::cstdio::write(self.stream(2).unwrap_or(core::ptr::null_mut()), &diagnostic);
    }

    pub(super) fn terminal_baud_rate(&self) -> Option<BaudRate> {
        self.boundary
            .terminal
            .borrow()
            .original
            .as_ref()
            .and_then(termios::baud_rate)
            .and_then(BaudRate::new)
    }

    pub(super) fn configure_terminal_display(&mut self) {
        let capabilities = &self.boundary.terminal_capabilities;
        let size = ScreenSize::new(capabilities.rows, capabilities.columns)
            .expect("terminal compatibility sizes are normalized");
        let profile = capabilities.profile(self.terminal_baud_rate());
        self.native.configure_display(profile, size);
    }
}
