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

/// A capability's bytes up to its first NUL, which is where a C caller's view
/// of it ends.
fn terminated(value: &[u8]) -> CString {
    let end = value
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(value.len());
    CString::new(&value[..end]).expect("the first NUL was removed")
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
            bools.extend(entry.booleans());
            numbers.extend(
                entry
                    .numbers()
                    .map(|(key, value)| (key, c_int::try_from(value).unwrap_or(c_int::MAX))),
            );
            strings.extend(entry.strings().map(|(key, value)| (key, terminated(value))));
            if let Some(value) = entry.string(nshterm::CapabilityName::Termcap("me")) {
                strings.insert("sgr0", terminated(&value));
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
        let mut entry = nshterm::TermInfoBuilder::default().named(self.name.clone());
        for (&capname, &value) in &self.bools {
            entry = entry.boolean(capname, value);
        }
        for (&capname, &value) in &self.numbers {
            if let Ok(value) = u32::try_from(value) {
                entry = entry.number(capname, value);
            }
        }
        for (&capname, value) in &self.strings {
            entry = entry.string(capname, value.as_bytes());
        }
        TerminalProfile::from_terminfo(&entry.build()).with_baud_rate(baud_rate)
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

/// The terminal type was installed but its capability entry could not be
/// loaded, so the hardcoded dumb terminal stands in for it.
///
/// C: `EL_TERMINAL`'s -1, which reports the failed lookup however usable the
/// fallback it installed is (ERR-terminal-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapabilityLookupFailed;

impl EditLine {
    /// Load the terminal type `name` selects, resolving an absent name
    /// through `$TERM` and then the dumb terminal.
    ///
    /// The name is only ever a lookup key, so bytes that are not UTF-8 are
    /// passed on lossily rather than refused — unlike `el_init`'s program
    /// name and `H_LOAD`'s filename, which fail the call. The C's own outcome
    /// for a name the terminfo database has no entry for is the diagnostic,
    /// the hardcoded dumb terminal and -1, and running that path is closer
    /// than refusing to configure anything.
    // [spec:nshedit:req:abi.rust-internals]
    pub(crate) fn set_terminal_type(
        &mut self,
        name: Option<&[u8]>,
    ) -> Result<(), CapabilityLookupFailed> {
        let name = name
            .map(String::from_utf8_lossy)
            .map(std::borrow::Cow::into_owned)
            .or_else(|| {
                secure_environment("TERM").map(|name| String::from_utf8_lossy(&name).into_owned())
            })
            .unwrap_or_else(|| "dumb".to_owned());
        if self.set_terminal_name(&name) == 0 {
            Ok(())
        } else {
            Err(CapabilityLookupFailed)
        }
    }

    pub(crate) fn set_terminal_name(&mut self, name: &str) -> c_int {
        let Ok(name) = CString::new(name) else {
            return -1;
        };
        if name.as_bytes() == b"emacs" {
            self.set_editing_enabled(false);
        }
        let result = nshterm::TermInfo::from_name(name.to_str().unwrap_or("dumb"));
        let window_size =
            with_borrowed_descriptor(self.descriptor(StreamKind::Input), terminal::screen_size)
                .and_then(Result::ok)
                .filter(|(rows, columns)| *rows != 0 && *columns != 0);
        let mut capabilities = TerminalCapabilities::new(
            name.to_str().unwrap_or("dumb"),
            result.as_ref().ok(),
            window_size,
        );
        if let Err(error) = &result {
            capabilities.preserve_failed_lookup_values(&self.boundary.terminal.capabilities);
            self.report_terminal_lookup_failure(name.as_c_str(), error);
        }
        self.boundary.terminal.capabilities = capabilities;
        self.boundary.terminal.name = name;
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
        let _ = crate::cstdio::write(self.stream(StreamKind::Diagnostics), &diagnostic);
    }

    pub(super) fn terminal_baud_rate(&self) -> Option<BaudRate> {
        self.boundary
            .terminal
            .state
            .borrow()
            .original
            .as_ref()
            .and_then(|attributes| match attributes.output_speed() {
                OutputSpeed::BitsPerSecond(rate) => Some(rate),
                OutputSpeed::Custom => None,
            })
            .and_then(BaudRate::new)
    }

    pub(super) fn configure_terminal_display(&mut self) {
        let capabilities = &self.boundary.terminal.capabilities;
        let size = ScreenSize::new(capabilities.rows, capabilities.columns)
            .expect("terminal compatibility sizes are normalized");
        let profile = capabilities.profile(self.terminal_baud_rate());
        self.editor.configure_display(profile, size);
    }
}
