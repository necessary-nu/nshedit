//! Typed ownership and initialization of readline's private global runtime.

use core::ffi::{c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;

use nshedit::domain::EditingMode;
use nshedit::editor::effect::PromptSide;

use crate::adapter::{EditLine, HistoryEncoding, SessionInit, SessionStreams, StreamEndpoint};
use crate::cdecl::handles::History;
use crate::cdecl::readline::{HistEntry, KEYMAP_SIZE, RlCommandFuncT};
use crate::conversion::ConversionBuffer;
use crate::history::HistoryRequest;

use super::*;

#[derive(Clone, Copy)]
pub(super) enum RuntimeSession {
    Uninitialized,
    Ready {
        editor: NonNull<EditLine>,
        history: NonNull<History>,
    },
}

impl RuntimeSession {
    fn editor(self) -> *mut EditLine {
        match self {
            Self::Uninitialized => ptr::null_mut(),
            Self::Ready { editor, .. } => editor.as_ptr(),
        }
    }

    fn history(self) -> *mut History {
        match self {
            Self::Uninitialized => ptr::null_mut(),
            Self::Ready { history, .. } => history.as_ptr(),
        }
    }

    pub(super) fn is_ready(self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

pub(super) struct ReadlineRuntimeState {
    pub(super) session: RuntimeSession,
    pub(super) commands: [Option<RlCommandFuncT>; KEYMAP_SIZE],
    pub(super) navigation_entry: HistEntry,
    pub(super) lookup_entry: HistEntry,
    pub(super) last_search_pattern: Option<Vec<u8>>,
    pub(super) last_search_match: Option<Vec<u8>>,
    pub(super) history_list: Vec<HistEntry>,
    pub(super) history_list_pointers: Vec<*mut HistEntry>,
    pub(super) passwd_scan: Option<nshedit_plat::passwd::UserNames>,
    pub(super) word_break_conversion: ConversionBuffer,
    pub(super) special_prefix_conversion: ConversionBuffer,
    pub(super) expansion_from: *mut c_char,
    pub(super) expansion_to: *mut c_char,
    pub(super) used_event_hook: bool,
    pub(super) default_history_file: *mut c_char,
}

impl ReadlineRuntimeState {
    const fn new() -> Self {
        Self {
            session: RuntimeSession::Uninitialized,
            commands: [None; KEYMAP_SIZE],
            navigation_entry: HistEntry {
                line: ptr::null(),
                data: ptr::null_mut(),
            },
            lookup_entry: HistEntry {
                line: ptr::null(),
                data: ptr::null_mut(),
            },
            last_search_pattern: None,
            last_search_match: None,
            history_list: Vec::new(),
            history_list_pointers: Vec::new(),
            passwd_scan: None,
            word_break_conversion: ConversionBuffer::new(),
            special_prefix_conversion: ConversionBuffer::new(),
            expansion_from: ptr::null_mut(),
            expansion_to: ptr::null_mut(),
            used_event_hook: false,
            default_history_file: ptr::null_mut(),
        }
    }
}

// [spec:nshedit:req:abi.typed-session]
/// Sole owner of readline's private process-global runtime.
///
/// The public C data symbols remain separate because their addresses are ABI,
/// but every private editor, history, callback registration, conversion
/// buffer, and retained result belongs to this one state object.
pub(super) struct ReadlineRuntime {
    state: UnsafeCell<ReadlineRuntimeState>,
    pub(super) abort_pending: AtomicBool,
}

// SAFETY: readline's C contract is process-global and unsynchronized. Every
// access is serialized by that contract (and by the test harness lock), while
// callback-capable paths copy or take the needed state before invoking C.
unsafe impl Sync for ReadlineRuntime {}

impl ReadlineRuntime {
    const fn new() -> Self {
        Self {
            state: UnsafeCell::new(ReadlineRuntimeState::new()),
            abort_pending: AtomicBool::new(false),
        }
    }

    pub(super) unsafe fn access<R>(
        &self,
        operation: impl FnOnce(&mut ReadlineRuntimeState) -> R,
    ) -> R {
        unsafe { operation(&mut *self.state.get()) }
    }

    pub(super) unsafe fn session(&self) -> RuntimeSession {
        unsafe { (*self.state.get()).session }
    }

    pub(super) unsafe fn install(&self, editor: NonNull<EditLine>, history: NonNull<History>) {
        unsafe {
            (*self.state.get()).session = RuntimeSession::Ready { editor, history };
        }
    }

    pub(super) unsafe fn take_session(&self) -> RuntimeSession {
        unsafe {
            core::mem::replace(
                &mut (*self.state.get()).session,
                RuntimeSession::Uninitialized,
            )
        }
    }
}

pub(super) static READLINE_RUNTIME: ReadlineRuntime = ReadlineRuntime::new();

pub(super) unsafe fn runtime_editor() -> *mut EditLine {
    unsafe { READLINE_RUNTIME.session().editor() }
}

/// Run `operation` on the installed editor, or answer `None` when this layer
/// has none.
///
/// C: every call site reached here hands `el_set`/`el_get` whatever `e` holds,
/// and a NULL editor is rejected without touching any state
/// (ERR-readline-11) — so an uninitialized layer runs no operation.
// [spec:nshedit:req:abi.rust-internals]
pub(super) unsafe fn with_runtime_editor<R>(
    operation: impl FnOnce(&mut EditLine) -> R,
) -> Option<R> {
    // SAFETY: the session owns the editor and hands out no other reference
    // for the duration of the call.
    unsafe { runtime_editor().as_mut() }.map(operation)
}

pub(super) unsafe fn runtime_history() -> *mut History {
    unsafe { READLINE_RUNTIME.session().history() }
}

pub(super) unsafe fn release_runtime_session() {
    if let RuntimeSession::Ready { editor, history } = unsafe { READLINE_RUNTIME.take_session() } {
        unsafe {
            crate::histedit::el_end(editor.as_ptr());
            crate::histedit::history_end(history.as_ptr());
        }
    }
}

#[derive(Debug)]
pub(super) enum ReadlineInitError {
    MissingProgramName,
    InvalidProgramName(std::str::Utf8Error),
    Editor(crate::adapter::SessionInitError),
    History,
    Prompt,
}

impl std::fmt::Display for ReadlineInitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingProgramName => formatter.write_str("readline program name is null"),
            Self::InvalidProgramName(error) => {
                write!(formatter, "invalid readline program name: {error}")
            }
            Self::Editor(error) => write!(formatter, "{error}"),
            Self::History => formatter.write_str("could not allocate readline history"),
            Self::Prompt => formatter.write_str("could not initialize the readline prompt"),
        }
    }
}

/// `history` under the callback signature the editor's history slot is typed
/// with.
///
/// The slot is declared by the wide API. `readline.c` installs the byte store
/// into it and marks the store narrow, so the record this receives has the
/// narrow layout that mark selects, and the handle is the `History` this
/// layer allocated.
///
/// # Safety
///
/// `handle` must be the live narrow history, and the tail must carry what
/// `op` defines.
// [spec:nshedit:req:abi.rust-internals]
unsafe extern "C" fn narrow_history(
    handle: *mut c_void,
    event: *mut crate::cdecl::histedit::HistEventWide,
    op: c_int,
    ap: ...
) -> c_int {
    // SAFETY: this function's own contract, forwarded unchanged.
    unsafe {
        crate::histedit::history_dispatch::<c_char>(
            handle.cast(),
            event.cast::<crate::cdecl::histedit::HistEvent>(),
            op,
            ap,
        )
    }
}

/// Bind one key sequence to one editor command, discarding the refusal the
/// C's `el_set(EL_BIND, ...)` also discards.
///
/// # Safety
///
/// `editor` must be the live editor this layer owns.
unsafe fn bind_key(editor: *mut EditLine, key_sequence: &[u8], command: &[u8]) {
    // SAFETY: the caller guarantees the editor is live.
    let editor = unsafe { &mut *editor };
    let _ = operations::run_list_command(editor, ListCommand::Bind, &[key_sequence, command]);
}

pub(super) unsafe fn initialize_readline() -> Result<(), ReadlineInitError> {
    // SAFETY: single-threaded module state; every editor call below is the
    // one `readline.c` makes, in its order.
    unsafe {
        release_runtime_session();

        rl_readline_state &= !RL_STATE_DONE;

        // These must be libc's actual stream objects: a caller can observe
        // both pointer identity and their userspace buffering.
        if rl_instream.is_null() {
            rl_instream = cstdio::standard_input();
        }
        if rl_outstream.is_null() {
            rl_outstream = cstdio::standard_output();
        }
        let error_stream = cstdio::standard_error();
        let fdin = cstdio::fileno_of(rl_instream);
        let fdout = cstdio::fileno_of(rl_outstream);
        let fderr = cstdio::fileno_of(error_stream);

        // A terminal with ECHO disabled is already controlled elsewhere, so
        // readline declines to edit it. Failure retains the C's default 1.
        let editmode = match crate::adapter::with_borrowed_descriptor(
            fdin,
            nshedit_plat::terminal::read_attributes,
        )
        .and_then(Result::ok)
        {
            Some(attributes) => {
                c_int::from(attributes.flag(nshedit_plat::terminal::TerminalFlag::EchoInput))
            }
            None => 1,
        };

        let program_bytes =
            c_bytes_opt(rl_readline_name).ok_or(ReadlineInitError::MissingProgramName)?;
        let program =
            core::str::from_utf8(program_bytes).map_err(ReadlineInitError::InvalidProgramName)?;
        let mut editor_owner = EditLine::new(SessionInit {
            program,
            streams: SessionStreams {
                input: StreamEndpoint {
                    file: rl_instream,
                    descriptor: fdin,
                },
                output: StreamEndpoint {
                    file: rl_outstream,
                    descriptor: fdout,
                },
                diagnostics: StreamEndpoint {
                    file: error_stream,
                    descriptor: fderr,
                },
            },
        })
        .map_err(ReadlineInitError::Editor)?;

        if editmode == 0 {
            editor_owner.set_editing_enabled(false);
        }

        let history =
            NonNull::new(crate::histedit::history_init()).ok_or(ReadlineInitError::History)?;
        let editor = NonNull::from(Box::leak(editor_owner));
        READLINE_RUNTIME.install(editor, history);
        let editor = editor.as_ptr();
        let history = history.as_ptr();

        let _ = history::execute(HistoryRequest::SetSize(c_int::MAX as usize));
        history_length = 0;
        max_input_history = c_int::MAX;
        let _ = (&mut *editor).set_history_callback(
            Some(narrow_history),
            history.cast(),
            HistoryEncoding::Narrow,
        );
        (&mut *editor).set_resize_callback(Some(_resize_fun), (&raw mut rl_line_buffer).cast());

        // Sampled once for non-NULL-ness, exactly as `readline.c` does; later
        // assignments take effect only after reinitialization.
        let getc_hook = rl_getc_function;
        if getc_hook.is_some() {
            (&mut *editor).set_read_callback(Some(_getc_function));
        }

        if rl_set_prompt(c"".as_ptr()) == -1 {
            release_runtime_session();
            return Err(ReadlineInitError::Prompt);
        }
        (&mut *editor).set_prompt_narrow(
            PromptSide::Left,
            Some(_get_prompt),
            RL_PROMPT_START_IGNORE.into(),
        );
        (&mut *editor).set_handle_signals(rl_catch_signals != 0);
        (&mut *editor).set_editor(EditingMode::Emacs);
        if rl_terminal_name.is_null() {
            let term = cenv::get(c"TERM");
            rl_terminal_name = if term.is_null() || *term == 0 {
                c"dumb".as_ptr().cast_mut()
            } else {
                term
            };
        } else {
            let _ = (&mut *editor).set_terminal_type(c_bytes_opt(rl_terminal_name));
        }

        let _ = operations::add_function(
            &mut *editor,
            b"rl_complete",
            b"ReadLine compatible completion function",
            _el_rl_complete,
        );
        bind_key(editor, b"^I", b"rl_complete");
        let _ = operations::add_function(
            &mut *editor,
            b"rl_tstp",
            b"ReadLine compatible suspend function",
            _el_rl_tstp,
        );
        bind_key(editor, b"^Z", b"rl_tstp");
        bind_key(editor, b"^R", b"em-inc-search-prev");

        for (key_sequence, command) in [
            (b"\\e[1~".as_slice(), b"ed-move-to-beg".as_slice()),
            (b"\\e[4~", b"ed-move-to-end"),
            (b"\\e[7~", b"ed-move-to-beg"),
            (b"\\e[8~", b"ed-move-to-end"),
            (b"\\e[H", b"ed-move-to-beg"),
            (b"\\e[F", b"ed-move-to-end"),
            (b"\\e[3~", b"ed-delete-next-char"),
            (b"\\e[2~", b"em-toggle-overwrite"),
            (b"\\e[1;5C", b"em-next-word"),
            (b"\\e[1;5D", b"ed-prev-word"),
            (b"\\e[5C", b"em-next-word"),
            (b"\\e[5D", b"ed-prev-word"),
            (b"\\e\\e[C", b"em-next-word"),
            (b"\\e\\e[D", b"ed-prev-word"),
        ] {
            bind_key(editor, key_sequence, command);
        }

        // readline reads editline's configuration, not GNU's inputrc.
        crate::histedit::el_source(editor, ptr::null());
        _resize_fun(editor, (&raw mut rl_line_buffer).cast());
        _rl_update_pos();
        tty_end(editor, TCSADRAIN);

        Ok(())
    }
}
