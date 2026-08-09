//! Typed ownership and initialization of readline's private global runtime.

use core::ffi::{c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicBool;

use crate::adapter::{EditLine, SessionInit, SessionStreams, StreamEndpoint};
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
        let editor = NonNull::from(editor_owner.as_mut());

        if editmode == 0 {
            el_set_va(editor.as_ptr().cast(), EL_EDITMODE, 0);
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
        el_set_va(
            editor.cast(),
            EL_HIST,
            crate::histedit::history as *const c_void,
            history,
        );
        el_set_va(
            editor.cast(),
            EL_RESIZE,
            _resize_fun as *const c_void,
            &raw mut rl_line_buffer,
        );

        // Sampled once for non-NULL-ness, exactly as the compatibility layer
        // does; later assignments take effect only after reinitialization.
        let getc_hook = rl_getc_function;
        if getc_hook.is_some() {
            el_set_va(editor.cast(), EL_GETCFN, _getc_function as *const c_void);
        }

        if rl_set_prompt(c"".as_ptr()) == -1 {
            release_runtime_session();
            return Err(ReadlineInitError::Prompt);
        }
        el_set_va(
            editor.cast(),
            EL_PROMPT_ESC,
            _get_prompt as *const c_void,
            RL_PROMPT_START_IGNORE as c_int,
        );
        el_set_va(editor.cast(), EL_SIGNAL, rl_catch_signals);
        el_set_va(editor.cast(), EL_EDITOR, c"emacs".as_ptr());
        if !rl_terminal_name.is_null() {
            el_set_va(editor.cast(), EL_TERMINAL, rl_terminal_name);
        } else {
            let term = cenv::get(c"TERM");
            rl_terminal_name = if term.is_null() || *term == 0 {
                c"dumb".as_ptr().cast_mut()
            } else {
                term
            };
        }

        el_set_va(
            editor.cast(),
            EL_ADDFN,
            c"rl_complete".as_ptr(),
            c"ReadLine compatible completion function".as_ptr(),
            _el_rl_complete as *const c_void,
        );
        el_set_va(
            editor.cast(),
            EL_BIND,
            c"^I".as_ptr(),
            c"rl_complete".as_ptr(),
            ptr::null::<c_char>(),
        );
        el_set_va(
            editor.cast(),
            EL_ADDFN,
            c"rl_tstp".as_ptr(),
            c"ReadLine compatible suspend function".as_ptr(),
            _el_rl_tstp as *const c_void,
        );
        el_set_va(
            editor.cast(),
            EL_BIND,
            c"^Z".as_ptr(),
            c"rl_tstp".as_ptr(),
            ptr::null::<c_char>(),
        );
        el_set_va(
            editor.cast(),
            EL_BIND,
            c"^R".as_ptr(),
            c"em-inc-search-prev".as_ptr(),
            ptr::null::<c_char>(),
        );

        for (key_sequence, command) in [
            (c"\\e[1~", c"ed-move-to-beg"),
            (c"\\e[4~", c"ed-move-to-end"),
            (c"\\e[7~", c"ed-move-to-beg"),
            (c"\\e[8~", c"ed-move-to-end"),
            (c"\\e[H", c"ed-move-to-beg"),
            (c"\\e[F", c"ed-move-to-end"),
            (c"\\e[3~", c"ed-delete-next-char"),
            (c"\\e[2~", c"em-toggle-overwrite"),
            (c"\\e[1;5C", c"em-next-word"),
            (c"\\e[1;5D", c"ed-prev-word"),
            (c"\\e[5C", c"em-next-word"),
            (c"\\e[5D", c"ed-prev-word"),
            (c"\\e\\e[C", c"em-next-word"),
            (c"\\e\\e[D", c"ed-prev-word"),
        ] {
            el_set_va(
                editor.cast(),
                EL_BIND,
                key_sequence.as_ptr(),
                command.as_ptr(),
                ptr::null::<c_char>(),
            );
        }

        // readline reads editline's configuration, not GNU's inputrc.
        crate::histedit::el_source(editor, ptr::null());
        _resize_fun(editor, (&raw mut rl_line_buffer).cast());
        _rl_update_pos();
        tty_end(editor, TCSADRAIN);

        Ok(())
    }
}
