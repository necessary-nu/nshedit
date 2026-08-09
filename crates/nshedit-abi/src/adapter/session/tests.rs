use super::*;

static WIDE_PROMPT: [u32; 3] = [b'w' as u32, b'>' as u32, 0];
static mut NARROW_PROMPT: [c_char; 3] = [b'n' as c_char, b'>' as c_char, 0];

fn editor() -> Box<EditLine> {
    EditLine::new(SessionInit::inert("adapter-test"))
        .expect("construct an editor over inert descriptors")
}

unsafe extern "C" fn wide_prompt(_: *mut EditLine) -> *mut u32 {
    WIDE_PROMPT.as_ptr().cast_mut()
}

unsafe extern "C" fn narrow_prompt(_: *mut EditLine) -> *mut c_char {
    (&raw mut NARROW_PROMPT).cast::<c_char>()
}

unsafe extern "C" fn command(_: *mut EditLine, _: u32) -> u8 {
    0
}

unsafe extern "C" fn read(_: *mut EditLine, value: *mut u32) -> c_int {
    if !value.is_null() {
        unsafe { *value = b'x' as u32 };
    }
    1
}

// [spec:nshedit:req:abi.typed-session/test]
#[test]
fn construction_owns_defaults() {
    let editor = editor();
    assert_eq!(editor.program().to_bytes(), b"adapter-test");
    assert!(!editor.editor_is_vi());
    assert!(!editor.handle_signals());
    assert!(editor.editing_enabled());
    assert!(!editor.unbuffered());
    assert!(!editor.safe_read());
    assert_eq!(editor.editor().config().editing_mode(), EditingMode::Emacs);
    assert_eq!(
        editor.editor().config().signal_policy(),
        SignalPolicy::Ignore
    );
    assert_eq!(editor.editor().config().buffering(), Buffering::Line);
}

#[test]
fn policy_reconfigures_the_editor() {
    let mut editor = editor();
    editor.set_handle_signals(true);
    editor.set_unbuffered(true);
    editor.set_safe_read(true);
    editor.set_editing_enabled(false);
    editor.set_history_encoding(HistoryEncoding::Narrow);

    assert!(editor.handle_signals());
    assert!(editor.unbuffered());
    assert!(editor.safe_read());
    assert!(!editor.editing_enabled());
    assert_eq!(editor.history_encoding(), HistoryEncoding::Narrow);
    assert_eq!(
        editor.editor().config().signal_policy(),
        SignalPolicy::Handle
    );
    assert_eq!(editor.editor().config().buffering(), Buffering::Command);
}

#[test]
fn editor_switch_resets_word_policy() {
    let mut editor = editor();
    editor.set_word_characters(&[b'a' as u32, b'b' as u32]);
    assert_eq!(
        editor.word_characters(),
        Some(&[b'a' as u32, b'b' as u32][..])
    );

    editor.set_editor(EditingMode::Vi);
    assert!(editor.editor_is_vi());
    assert_eq!(editor.editor().config().editing_mode(), EditingMode::Vi);
    assert_eq!(editor.word_characters(), None);

    editor.set_editor(EditingMode::Emacs);
    assert!(!editor.editor_is_vi());
}

#[test]
fn prompts_keep_boundary_metadata() {
    let mut editor = editor();
    editor.set_prompt_wide(PromptSide::Left, Some(wide_prompt), 0x1b);
    editor.set_prompt_narrow(PromptSide::Right, Some(narrow_prompt), b'%' as u32);

    let (left, left_escape) = editor.prompt(PromptSide::Left);
    let (right, right_escape) = editor.prompt(PromptSide::Right);
    let PromptCallback::Wide(left) = left else {
        panic!("left prompt should remain wide");
    };
    let PromptCallback::Narrow(right) = right else {
        panic!("right prompt should remain narrow");
    };
    assert!(core::ptr::fn_addr_eq(
        left,
        wide_prompt as WidePromptCallback
    ));
    assert!(core::ptr::fn_addr_eq(
        right,
        narrow_prompt as NarrowPromptCallback
    ));
    assert_eq!(left_escape, 0x1b);
    assert_eq!(right_escape, b'%' as u32);
    assert!(matches!(
        editor.prompt(PromptSide::Left).0,
        PromptCallback::Wide(_)
    ));
    assert!(matches!(
        editor.prompt(PromptSide::Right).0,
        PromptCallback::Narrow(_)
    ));
}

// [spec:nshedit:req:abi.typed-session/test]
#[test]
fn prompt_address_projection_never_invokes() {
    let mut editor = editor();
    editor.set_prompt_narrow(PromptSide::Left, Some(narrow_prompt), 0);
    let mut wide: Option<WidePromptCallback> = None;
    let projected = unsafe {
        editor
            .prompt(PromptSide::Left)
            .0
            .write_wide((&raw mut wide).cast())
    };
    assert!(projected);
    assert_eq!(
        wide.map(|callback| callback as *const ()),
        Some(narrow_prompt as *const ())
    );

    editor.set_prompt_wide(PromptSide::Right, Some(wide_prompt), 0);
    let mut narrow: Option<NarrowPromptCallback> = None;
    let projected = unsafe {
        editor
            .prompt(PromptSide::Right)
            .0
            .write_narrow((&raw mut narrow).cast())
    };
    assert!(projected);
    assert_eq!(
        narrow.map(|callback| callback as *const ()),
        Some(wide_prompt as *const ())
    );
}

#[test]
fn read_callback_round_trips() {
    let mut editor = editor();
    assert!(editor.read_callback().is_none());
    editor.set_read_callback(Some(read));
    assert!(
        editor
            .read_callback()
            .is_some_and(|callback| core::ptr::fn_addr_eq(callback, read as ReadCallback))
    );
    editor.set_read_callback(None);
    assert!(editor.read_callback().is_none());
}

#[test]
fn commands_validate_before_binding() {
    let mut editor = editor();
    let name = [b'm' as u32, b'i' as u32, b'n' as u32, b'e' as u32];
    let help = [b'h' as u32];
    assert!(editor.add_command(&name, &help, command));
    let command_name = CommandName::new("mine").expect("valid command name");
    assert!(
        editor
            .command_callback(&command_name)
            .is_some_and(|callback| core::ptr::fn_addr_eq(callback, command as CommandCallback))
    );

    let bind = [b'b' as u32, b'i' as u32, b'n' as u32, b'd' as u32];
    let key = [b'^' as u32, b'X' as u32];
    assert_eq!(editor.bind_command(&[&bind, &key, &name]), 0);
    let control_x = KeySequence::try_from("\u{18}").expect("control-X is non-empty");
    assert_eq!(
        editor.editor().binding(KeymapMode::Emacs, &control_x),
        Some(&Binding::User(command_name))
    );
    let unknown = [b'n' as u32, b'o' as u32];
    assert_eq!(editor.bind_command(&[&bind, &key, &unknown]), -1);
}

#[test]
fn completion_listing_is_two_step() {
    let mut editor = editor();
    assert_eq!(editor.begin_completion(), CompletionInvocation::Insert);
    assert_eq!(editor.begin_completion(), CompletionInvocation::List);
    editor.clear_completion_pending_listing();
    assert_eq!(editor.begin_completion(), CompletionInvocation::Insert);
}

#[test]
fn reset_keeps_session_policy() {
    let mut editor = editor();
    editor.set_history_depth(7);
    editor.set_safe_read(true);
    editor.reset_line();
    assert_eq!(editor.history_depth(), 0);
    assert!(editor.safe_read());
}
