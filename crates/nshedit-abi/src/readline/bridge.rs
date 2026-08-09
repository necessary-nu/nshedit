//! Narrow bridge from readline's C-shaped state to safe core/platform calls.

use core::ffi::c_int;

use nshedit::domain::TerminalMode;

use crate::adapter::EditLine;

/// Rebuild the terminal-mode model before a readline operation.
pub(super) fn tty_init(el: *mut EditLine) {
    // A null editor is undefined in C and becomes a no-op at this boundary.
    if let Some(el) = unsafe { el.as_mut() } {
        let _ = el.set_terminal_mode(TerminalMode::Editing);
    }
}

/// Restore terminal modes with readline's requested timing.
pub(super) fn tty_end(el: *mut EditLine, _how: c_int) {
    if let Some(el) = unsafe { el.as_mut() } {
        let _ = el.set_terminal_mode(TerminalMode::Cooked);
    }
}

/// Write into the virtual display without advancing its cursor.
pub(super) fn re_putc(el: *mut EditLine, c: u32) {
    if let Some(el) = unsafe { el.as_mut() } {
        let _ = el.write_wide(c);
    }
}

/// C: `em_kill_line(el, 0)`.
pub(super) fn em_kill_line(el: *mut EditLine) {
    if let Some(el) = unsafe { el.as_mut() } {
        el.kill_line();
    }
}

/// C: `tty_get_signal_character(el, sig)`.
pub(super) fn tty_get_signal_character(_el: *mut EditLine, _sig: c_int) -> c_int {
    -1
}

/// C: `getpwuid(getuid())->pw_dir` through the safe NSS boundary.
pub(super) fn passwd_home_dir() -> Option<Vec<u8>> {
    nshedit_plat::passwd::home_dir_by_uid(nshedit_plat::getuid())
}
