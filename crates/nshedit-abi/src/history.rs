//! The C ABI's history surface.
//!
//! `dec:libedit:idiomatic-core` puts the compatibility artifacts here rather
//! than in the core, and `history.c`'s are the opcode dispatcher and the
//! eleven-slot `H_FUNC` vtable — a store the caller can replace wholesale,
//! reached through a `void *` the library never inspects. Neither is a shape
//! Rust would choose, and both are frozen by the ABI.
//!
//! Only the first piece lives here so far. `history-idiomatize` moves the
//! dispatcher and the vtable across; this is what had to arrive first, because
//! it is the one call the core could neither keep nor make.

use core::ffi::{c_int, c_void};

use nshedit::histedit::{HistEventW, HistoryW};
use nshedit::history::HistoryArg;

/// The `.editrc` `history size` / `history unique` path, which `hist_command`
/// cannot walk itself.
///
/// C: `hist_command` calls `history_w(el->el_history.ref, &ev, op, num)`
/// directly, bypassing the installed dispatcher and reinterpreting the opaque
/// handle as libedit's own wide store (ERR-history-05). Naming that store
/// means naming this crate's dispatcher, and the core cannot depend on this
/// crate, so the core holds a [`HistSettingsT`] instead and the pun happens
/// here.
///
/// Behaviour is the C's, unchanged, including where the C is wrong: for
/// libedit's own store this works, and for a custom store installed through
/// the wide entry point it is type confused. Recognising that case needs the
/// store to identify its own handle and is a separate question — the pun
/// moving does not close it.
///
/// # Safety
///
/// `cookie` is the handle passed to `el_set(EL_HIST, ...)`, and the C's own
/// precondition is that it is a `HistoryW *`.
///
/// [`HistSettingsT`]: nshedit::hist::HistSettingsT
pub(crate) unsafe extern "C" fn hist_settings(cookie: *mut c_void, op: c_int, num: c_int) -> c_int {
    // C: `ev` is an uninitialised local, filled by the callee and discarded,
    // so the error string is thrown away.
    let mut ev = HistEventW {
        num: 0,
        str: core::ptr::null(),
    };
    nshedit::history::history_w(cookie.cast::<HistoryW>(), &mut ev, op, HistoryArg::Num(num))
}
