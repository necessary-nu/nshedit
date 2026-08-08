//! One headless [`EditLine`], for the concerns whose tests drive the editor
//! rather than a single function.
//!
//! Five modules had grown their own version of this and disagreed about which
//! `init` call mattered, which is not a stylistic difference: an editor
//! missing one of them is not a smaller editor, it is an editor in a state no
//! real program can put it in, and three separate suites passed against one.
//! Every `init` below therefore says what it holds up, so that dropping one
//! has to be an argument rather than an omission.
//!
//! What is *not* here is as deliberate. `terminal_init` reads a terminfo entry
//! and `tty_init` wants a tty, neither of which a test runner has, so
//! [`headless_editor`] fills in the screen they would have produced. `sig_init`
//! installs nothing and records nothing a test can see.

use crate::chared::ch_init;
use crate::el::{EditLine, blank_editline};
use crate::hist::hist_init;
use crate::keymacro::keymacro_init;
use crate::literal::literal_init;
use crate::map::map_init;
use crate::prompt::prompt_init;
use crate::read::read_init;
use crate::search::search_init;

/// An editor in the state `el_init` leaves behind, on an `h`-by-`v` screen,
/// with nothing to write to.
///
/// The subsystem order is `el_init_internal`'s, which the C marks "Order is
/// important!!!" — `map_init` needs the key map allocated before it binds the
/// arrow keys, and `ch_init` sets `el_map.current`, so it has to follow
/// `map_init`.
///
/// The editing mode is therefore the shipped default, vi *insert* mode: that
/// is what `map_init`'s last step installs. A test that wants emacs calls
/// `map_init_emacs`, and vi command mode is `el_map.current = Alt`.
pub(crate) fn headless_editor(h: i32, v: i32) -> EditLine {
    let mut el = blank_editline();

    // The macro-binding tree and the buffer `keymacro_kprint` formats into.
    // `map_init` below resets the tree and binds the arrow keys through it.
    keymacro_init(&mut el);
    // The two key maps. Without them every keystroke dispatches as command 0,
    // so a reader loop consumes input and does nothing with it —
    // `ce_inc_search` never grows its pattern, and `vi_redo` finds no function
    // to replay.
    map_init(&mut el);
    // The line, undo, redo and kill buffers, all `EL_BUFSIZ`, with `limit`
    // short of the end so the insert paths have slack to shift into. Without
    // it `cv_undo` and `cv_yank` clamp their copies to a zero-length buffer,
    // so a delete records an *empty* kill buffer and every assertion about
    // killed text passes against `""`.
    ch_init(&mut el);
    // The pattern buffer `c_setpat`, `cv_search`, `chadir` and `chacha` all
    // write through.
    search_init(&mut el);
    // The stash `hist_get` restores the live line out of. Without it the
    // `eventno == 0` branch reads a zero-sized stash and *empties* the line
    // instead of restoring it, so a search rollback looks like the search
    // having deleted the user's text. Its failure is not fatal in the C
    // either (ERR-history-11), so the result is discarded here as it is there.
    let _ = hist_init(&mut el);
    // The two default prompt callbacks. Leaving them unset is a state
    // `el_init` cannot produce, and `re_refresh` draws whatever it finds.
    prompt_init(&mut el);
    // A no-op on a blank editor, as it is in the C; called so that the
    // sequence here stays the same list as `el_init_internal`'s.
    literal_init(&mut el);
    // The macro queue `el_wpush` fills and `el_wgetc` drains, which is the
    // whole of "the user typed this" for a headless test.
    read_init(&mut el);

    // Descriptor 0 is the test runner's own stdout and `write_outfile` takes
    // it literally, so an editor left with the zeroed value sprays escape
    // sequences over the test output. A negative one writes into the void.
    el.el_infd = -1;
    el.el_outfd = -1;
    el.el_errfd = -1;

    // `terminal_init`'s screen, without a terminal. `re_refresh` walks
    // `el_display` under `t_size` and recurses to a stack overflow on a
    // zero-sized one, so both images have to be real and both have to match
    // the size. The extra cell per row is the terminator slot
    // `terminal_alloc_buffer` allocates and every wrap writes.
    el.el_terminal.t_size.h = h;
    el.el_terminal.t_size.v = v;
    let row = usize::try_from(h).unwrap_or(0) + 1;
    let rows = usize::try_from(v).unwrap_or(0);
    el.el_display = vec![vec![0u32; row]; rows];
    el.el_vdisplay = vec![vec![0u32; row]; rows];

    el
}

/// Put `s` in the line buffer with the cursor at `at`, as a reader loop would
/// have left it.
///
/// The buffer keeps `ch_init`'s size rather than being resized to the text:
/// the insert and delete paths shift into the slack past `lastchar`, and a
/// buffer sized to its contents makes them index one past the end — which is
/// the test being wrong about the invariant, not the code breaking it.
pub(crate) fn set_line(el: &mut EditLine, s: &str, at: usize) {
    let text: Vec<u32> = s.chars().map(u32::from).collect();
    el.el_line.buffer[..text.len()].copy_from_slice(&text);
    el.el_line.buffer[text.len()] = 0;
    el.el_line.lastchar = text.len();
    el.el_line.cursor = at;
}

/// The live line as text, bounded by `lastchar` rather than its spare buffer.
pub(crate) fn text(el: &EditLine) -> String {
    el.el_line.buffer[..el.el_line.lastchar]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}

/// The kill buffer as its length field describes it; it has no terminator.
pub(crate) fn killed(el: &EditLine) -> String {
    el.el_chared.c_kill.buf[..el.el_chared.c_kill.last]
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect()
}
