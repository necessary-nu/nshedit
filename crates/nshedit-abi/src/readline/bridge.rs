//! Narrow bridge from readline's C-shaped state to safe core/platform calls.

use core::ffi::c_int;

use crate::adapter::EditLine;

/// C: `#define NO_TTY 0x002` (`el.h`).
pub(super) const NO_TTY: i32 = 0x002;

/// Rebuild the terminal-mode model before a readline operation.
pub(super) fn tty_init(el: *mut EditLine) {
    // A null editor is undefined in C and becomes a no-op at this boundary.
    if let Some(el) = unsafe { el.as_mut() } {
        let _ = nshedit::tty::tty_init(el);
    }
}

/// Restore terminal modes with readline's requested timing.
pub(super) fn tty_end(el: *mut EditLine, how: c_int) {
    if let Some(el) = unsafe { el.as_mut() } {
        nshedit::tty::tty_end(el, how);
    }
}

/// Write into the virtual display without advancing its cursor.
pub(super) fn re_putc(el: *mut EditLine, c: u32) {
    if let Some(el) = unsafe { el.as_mut() } {
        nshedit::refresh::re_putc(el, c, 0);
    }
}

/// C: `em_kill_line(el, 0)`.
pub(super) fn em_kill_line(el: *mut EditLine) {
    if let Some(el) = unsafe { el.as_mut() } {
        let _ = nshedit::emacs::em_kill_line(el, 0);
    }
}

/// C: `tty_get_signal_character(el, sig)`.
pub(super) fn tty_get_signal_character(el: *mut EditLine, sig: c_int) -> c_int {
    unsafe { el.as_mut() }.map_or(-1, |el| nshedit::tty::tty_get_signal_character(el, sig))
}

/// C: `getpwuid(getuid())->pw_dir` through the safe NSS boundary.
pub(super) fn passwd_home_dir() -> Option<Vec<u8>> {
    nshedit_plat::passwd::home_dir_by_uid(nshedit_plat::getuid())
}
