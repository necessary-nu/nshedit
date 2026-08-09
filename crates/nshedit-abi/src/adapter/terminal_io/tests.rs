use super::*;

fn editor() -> Box<EditLine> {
    EditLine::new(SessionInit::inert("terminal-adapter-test"))
        .expect("construct an editor over inert descriptors")
}

fn text(value: &str) -> Text {
    value.chars().map(TextUnit::Scalar).collect()
}

fn terminal_command(editor: &mut EditLine, words: &[&str]) -> c_int {
    let storage: Vec<Vec<u32>> = words
        .iter()
        .map(|word| word.chars().map(u32::from).collect())
        .collect();
    let arguments: Vec<&[u32]> = storage.iter().map(Vec::as_slice).collect();
    editor.terminal_command(&arguments)
}

const TTY_FLAG_CASES: &[(&str, TerminalFlag)] = &[
    ("ignbrk", TerminalFlag::IgnoreBreak),
    ("brkint", TerminalFlag::SignalBreak),
    ("ignpar", TerminalFlag::IgnoreParityErrors),
    ("parmrk", TerminalFlag::MarkParityErrors),
    ("inpck", TerminalFlag::CheckInputParity),
    ("istrip", TerminalFlag::StripInputHighBit),
    ("inlcr", TerminalFlag::MapNewlineToCarriageReturn),
    ("igncr", TerminalFlag::IgnoreCarriageReturn),
    ("icrnl", TerminalFlag::MapCarriageReturnToNewline),
    ("iuclc", TerminalFlag::MapUppercaseInputToLowercase),
    ("ixon", TerminalFlag::EnableOutputFlowControl),
    ("ixany", TerminalFlag::AllowAnyCharacterToRestartOutput),
    ("ixoff", TerminalFlag::EnableInputFlowControl),
    ("imaxbel", TerminalFlag::RingBellOnInputOverflow),
    ("opost", TerminalFlag::PostProcessOutput),
    ("olcuc", TerminalFlag::MapLowercaseOutputToUppercase),
    ("onlcr", TerminalFlag::MapNewlineToCarriageReturnNewline),
    ("ocrnl", TerminalFlag::MapCarriageReturnToNewlineOnOutput),
    ("onocr", TerminalFlag::DiscardCarriageReturnAtColumnZero),
    ("onlret", TerminalFlag::NewlinePerformsCarriageReturn),
    ("ofill", TerminalFlag::UseFillCharacters),
    ("ofdel", TerminalFlag::UseDeleteForFill),
    ("nldly", TerminalFlag::NewlineDelay),
    ("crdly", TerminalFlag::CarriageReturnDelay),
    ("tabdly", TerminalFlag::TabDelay),
    ("xtabs", TerminalFlag::ExpandTabs),
    ("bsdly", TerminalFlag::BackspaceDelay),
    ("vtdly", TerminalFlag::VerticalTabDelay),
    ("ffdly", TerminalFlag::FormFeedDelay),
    ("cbaud", TerminalFlag::OutputSpeedBits),
    ("cstopb", TerminalFlag::TwoStopBits),
    ("cread", TerminalFlag::EnableReceiver),
    ("parenb", TerminalFlag::EnableParity),
    ("parodd", TerminalFlag::OddParity),
    ("hupcl", TerminalFlag::HangUpOnClose),
    ("clocal", TerminalFlag::IgnoreModemControl),
    ("cibaud", TerminalFlag::InputSpeedBits),
    ("crtscts", TerminalFlag::HardwareFlowControl),
    ("isig", TerminalFlag::GenerateSignals),
    ("icanon", TerminalFlag::CanonicalInput),
    ("xcase", TerminalFlag::CanonicalUppercase),
    ("echo", TerminalFlag::EchoInput),
    ("echoe", TerminalFlag::EchoErase),
    ("echok", TerminalFlag::EchoKill),
    ("echonl", TerminalFlag::EchoNewline),
    ("noflsh", TerminalFlag::DisableFlush),
    ("tostop", TerminalFlag::StopBackgroundOutput),
    ("echoctl", TerminalFlag::EchoControlCharacters),
    ("echoprt", TerminalFlag::EchoErasedCharacters),
    ("echoke", TerminalFlag::VisuallyEraseKilledLine),
    ("flusho", TerminalFlag::OutputBeingFlushed),
    ("pendin", TerminalFlag::PendingInput),
    ("iexten", TerminalFlag::ExtendedProcessing),
    ("extproc", TerminalFlag::ExternalProcessing),
];

const TTY_CHARACTER_CASES: &[(&str, ControlCharacter)] = &[
    ("intr", ControlCharacter::Interrupt),
    ("quit", ControlCharacter::Quit),
    ("erase", ControlCharacter::Erase),
    ("kill", ControlCharacter::Kill),
    ("eof", ControlCharacter::EndOfFile),
    ("eol", ControlCharacter::EndOfLine),
    ("eol2", ControlCharacter::AlternateEndOfLine),
    ("start", ControlCharacter::Start),
    ("stop", ControlCharacter::Stop),
    ("werase", ControlCharacter::WordErase),
    ("susp", ControlCharacter::Suspend),
    ("reprint", ControlCharacter::Reprint),
    ("discard", ControlCharacter::Discard),
    ("lnext", ControlCharacter::LiteralNext),
    ("min", ControlCharacter::MinimumBytes),
    ("time", ControlCharacter::Timeout),
];

#[test]
fn tty_flag_projection() {
    for &(name, flag) in TTY_FLAG_CASES {
        let mut editor = editor();
        let enabled = format!("+{name}");
        let disabled = format!("-{name}");

        assert_eq!(terminal_command(&mut editor, &["setty", "-d", &enabled]), 0);
        assert_eq!(
            editor.boundary.terminal.state.borrow().overrides[1]
                .flags
                .get(&flag),
            Some(&TtyOverride::Enable),
            "{name}"
        );

        assert_eq!(
            terminal_command(&mut editor, &["setty", "-d", &disabled]),
            0
        );
        assert_eq!(
            editor.boundary.terminal.state.borrow().overrides[1]
                .flags
                .get(&flag),
            Some(&TtyOverride::Disable),
            "{name}"
        );

        assert_eq!(terminal_command(&mut editor, &["setty", "-d", name]), 0);
        assert_eq!(
            editor.boundary.terminal.state.borrow().overrides[1]
                .flags
                .get(&flag),
            None,
            "{name}"
        );
    }
}

#[test]
fn tty_character_projection() {
    for &(name, character) in TTY_CHARACTER_CASES {
        let mut editor = editor();
        let enabled = format!("+{name}");
        let disabled = format!("-{name}");

        assert_eq!(terminal_command(&mut editor, &["setty", "-q", &enabled]), 0);
        assert_eq!(
            editor.boundary.terminal.state.borrow().overrides[2]
                .characters
                .get(&character),
            Some(&TtyOverride::Enable),
            "{name}"
        );

        assert_eq!(
            terminal_command(&mut editor, &["setty", "-q", &disabled]),
            0
        );
        assert_eq!(
            editor.boundary.terminal.state.borrow().overrides[2]
                .characters
                .get(&character),
            Some(&TtyOverride::Disable),
            "{name}"
        );

        assert_eq!(terminal_command(&mut editor, &["setty", "-q", name]), 0);
        assert_eq!(
            editor.boundary.terminal.state.borrow().overrides[2]
                .characters
                .get(&character),
            None,
            "{name}"
        );
    }
}

#[test]
fn tty_modes_keep_independent_overrides() {
    let mut editor = editor();
    for (selector, mode, status) in [("-x", 0, -1), ("-d", 1, 0), ("-q", 2, 0)] {
        assert_eq!(
            terminal_command(&mut editor, &["setty", selector, "+echoctl"]),
            status
        );
        let state = editor.boundary.terminal.state.borrow();
        assert_eq!(
            state.overrides[mode]
                .flags
                .get(&TerminalFlag::EchoControlCharacters),
            Some(&TtyOverride::Enable),
            "{selector}"
        );
    }
}

#[test]
fn line_and_cursor_share_native_state() {
    let mut editor = editor();
    assert!(editor.replace_line(text("abcd")));
    assert_eq!(editor.editor().line(), &text("abcd"));
    assert_eq!(editor.editor().cursor().get(), 4);

    assert_eq!(editor.move_cursor(-2), 2);
    assert_eq!(editor.editor().cursor().get(), 2);
    assert_eq!(editor.move_cursor(99), 4);
    assert_eq!(editor.move_cursor(-99), 0);
}

#[test]
fn wide_edits_are_native() {
    let mut editor = editor();
    assert!(editor.replace_line(text("abcd")));
    assert_eq!(editor.move_cursor(-2), 2);
    assert_eq!(editor.insert_wide(&[b'X' as u32, b'Y' as u32]), 0);
    assert_eq!(editor.editor().line(), &text("abXYcd"));
    assert_eq!(editor.editor().cursor().get(), 4);

    editor.delete_before_cursor(2);
    assert_eq!(editor.editor().line(), &text("abcd"));
    assert_eq!(editor.editor().cursor().get(), 2);
    editor.delete_before_cursor(3);
    assert_eq!(editor.editor().line(), &text("abcd"));
}

#[test]
fn accepted_line_has_one_newline() {
    let mut editor = editor();
    assert!(editor.finish_accepted_line(text("first")));
    assert_eq!(editor.editor().line(), &text("first\n"));
    assert!(editor.finish_accepted_line(text("second\n")));
    assert_eq!(editor.editor().line(), &text("second\n"));
}

#[test]
fn range_delete_preserves_legacy_bug() {
    let mut editor = editor();
    assert!(editor.replace_line(text("abcdef")));
    assert_eq!(editor.delete_range(1, 3), 2);
    assert_eq!(editor.editor().line(), &text("aded"));

    assert_eq!(editor.delete_range(2, 99), 0);
    assert_eq!(editor.delete_range(-1, 2), 0);
    assert_eq!(editor.editor().line(), &text("aded"));
}

#[test]
fn pushback_counts_entries() {
    let mut editor = editor();
    let long = [b'a' as u32; 32];
    assert!(editor.push_input(&long));
    for index in 0..9 {
        assert!(editor.push_input(&[b'0' as u32 + index]));
    }
    assert!(!editor.push_input(&[b'z' as u32]));

    for _ in long {
        assert_eq!(editor.pop_input(), Some(TextUnit::Scalar('a')));
    }
    for expected in '0'..='8' {
        assert_eq!(editor.pop_input(), Some(TextUnit::Scalar(expected)));
    }
    assert_eq!(editor.pop_input(), None);
}

#[test]
fn empty_pushback_entry_is_legal() {
    let mut editor = editor();
    assert!(editor.push_input(&[]));
    assert!(editor.push_input(&[b'x' as u32]));
    assert_eq!(editor.pop_input(), Some(TextUnit::Scalar('x')));
    assert_eq!(editor.pop_input(), None);
}

#[test]
fn streams_update_terminal_descriptors() {
    let mut editor = editor();
    let input = core::ptr::without_provenance_mut::<c_void>(0x1000);
    let output = core::ptr::without_provenance_mut::<c_void>(0x2000);
    editor.set_stream(StreamKind::Input, input, 17);
    editor.set_stream(StreamKind::Output, output, 23);

    assert_eq!(editor.stream(StreamKind::Input), input);
    assert_eq!(editor.stream(StreamKind::Output), output);
    assert_eq!(editor.descriptor(StreamKind::Input), 17);
    assert_eq!(editor.descriptor(StreamKind::Output), 23);
    assert_eq!(editor.boundary.terminal.state.borrow().input, 17);
    assert_eq!(editor.boundary.terminal.state.borrow().output, 23);
}

#[test]
fn wide_view_tracks_native_line() {
    let mut editor = editor();
    assert!(editor.replace_line(text("wide")));
    assert_eq!(editor.move_cursor(-2), 2);
    let line = editor.publish_wide_line();

    // SAFETY: the view borrows storage owned by `editor`, which remains
    // live and unchanged for every assertion below.
    unsafe {
        assert_eq!((*line).cursor.offset_from((*line).buffer), 2);
        assert_eq!((*line).lastchar.offset_from((*line).buffer), 4);
        assert_eq!(*(*line).lastchar, 0);
    }
}

#[test]
fn inert_descriptors_report_errors() {
    let editor = editor();
    assert!(!editor.is_tty());
    assert_eq!(
        editor.control_eof(),
        ControlCharacter::EndOfFile.default_value()
    );
    assert_eq!(
        editor.control_reprint(),
        ControlCharacter::Reprint.default_value()
    );
    assert!(editor.write_output(b"x").is_err());
    assert!(editor.read_input(&mut [0]).is_err());
}

// [spec:nshedit:req:abi.terminal-controls+1/test]
#[test]
fn mutations_reconfigure_native_profile() {
    let mut editor = editor();

    assert_eq!(terminal_command(&mut editor, &["settc", "co", "132"]), 0);
    assert_eq!(terminal_command(&mut editor, &["settc", "li", "50"]), 0);

    let mut columns = -1;
    let mut rows = -1;
    // SAFETY: numeric capabilities require writable `c_int` output storage.
    unsafe {
        assert_eq!(
            editor.get_terminal_capability(b"co", core::ptr::from_mut(&mut columns).cast()),
            0
        );
        assert_eq!(
            editor.get_terminal_capability(b"li", core::ptr::from_mut(&mut rows).cast()),
            0
        );
    }
    assert_eq!((rows, columns), (50, 132));
    assert_eq!(editor.screen_size(), ScreenSize::new(50, 132).ok());
    assert_eq!(
        editor.editor().screen().map(|screen| screen.size()),
        ScreenSize::new(50, 132).ok()
    );

    assert_eq!(terminal_command(&mut editor, &["settc", "am", "no"]), 0);
    let mut automatic_margins: *const c_char = core::ptr::null();
    // SAFETY: boolean capabilities require writable `char *` output storage.
    unsafe {
        assert_eq!(
            editor
                .get_terminal_capability(b"am", core::ptr::from_mut(&mut automatic_margins).cast()),
            0
        );
        assert_eq!(CStr::from_ptr(automatic_margins), c"no");
    }

    assert_eq!(terminal_command(&mut editor, &["settc", "bl", "B"]), 0);
    let mut bell: *const c_char = core::ptr::null();
    // SAFETY: string capabilities require writable `char *` output storage.
    unsafe {
        assert_eq!(
            editor.get_terminal_capability(b"bl", core::ptr::from_mut(&mut bell).cast()),
            0
        );
        assert_eq!(CStr::from_ptr(bell), c"B");
    }
    let mut native_output = Vec::new();
    assert_eq!(
        editor
            .editor_mut()
            .beep(&mut native_output)
            .expect("the Vec writer cannot fail"),
        1
    );
    assert_eq!(native_output, b"B");
}

// [spec:nshedit:req:abi.terminal-session/test]
#[test]
fn geometry_prefers_kernel_size() {
    let entry = nshterm::TermInfo {
        names: vec!["sized".to_owned()],
        bools: HashMap::new(),
        numbers: HashMap::from([("lines", 17), ("cols", 63)]),
        strings: HashMap::new(),
    };
    let database = TerminalCapabilities::new("sized", Some(&entry), None);
    let kernel = TerminalCapabilities::new("sized", Some(&entry), Some((41, 109)));

    assert_eq!((database.rows, database.columns), (17, 63));
    assert_eq!((kernel.rows, kernel.columns), (41, 109));
}

#[test]
fn numeric_flags_refresh_on_string_mutation() {
    let mut editor = editor();
    assert!(
        !editor
            .boundary
            .terminal
            .capabilities
            .derived_destructive_tabs
    );

    assert_eq!(terminal_command(&mut editor, &["settc", "xt", "1"]), 0);
    assert!(
        !editor
            .boundary
            .terminal
            .capabilities
            .derived_destructive_tabs
    );

    assert_eq!(terminal_command(&mut editor, &["settc", "bl", "B"]), 0);
    assert!(
        editor
            .boundary
            .terminal
            .capabilities
            .derived_destructive_tabs
    );
}

#[test]
fn missing_terminal_uses_dumb_fallback() {
    let mut editor = editor();
    assert_eq!(terminal_command(&mut editor, &["settc", "am", "yes"]), 0);
    assert_eq!(terminal_command(&mut editor, &["settc", "xn", "yes"]), 0);
    assert_eq!(terminal_command(&mut editor, &["settc", "MT", "1"]), 0);
    assert_eq!(terminal_command(&mut editor, &["settc", "bl", "B"]), 0);

    assert_eq!(
        editor.set_terminal_name("nshedit-no-such-terminal-entry"),
        -1
    );
    let capabilities = &editor.boundary.terminal.capabilities;
    assert!(capabilities.boolean("am"));
    assert!(capabilities.boolean("xn"));
    assert_eq!(capabilities.number("MT"), 1);
    assert_eq!(capabilities.number("xt"), 1);
    assert!(capabilities.string("bl").is_none());
    assert_eq!((capabilities.rows, capabilities.columns), (24, 80));
}

#[test]
fn emacs_name_disables_editing() {
    let mut editor = editor();
    assert!(editor.editing_enabled());
    let _ = editor.set_terminal_name("emacs");
    assert!(!editor.editing_enabled());
    let _ = editor.set_terminal_name("dumb");
    assert!(!editor.editing_enabled());
}

// [spec:nshedit:req:abi.tty-modes/test]
#[test]
fn tty_commands_change_selected_masks() {
    let mut editor = editor();
    assert_eq!(
        terminal_command(&mut editor, &["setty", "-d", "+echo", "-isig"]),
        0
    );

    let state = editor.boundary.terminal.state.borrow();
    let editing = &state.overrides[tty::tty_mode_index(TerminalMode::Editing)];
    assert_eq!(
        editing.flags.get(&TerminalFlag::EchoInput),
        Some(&TtyOverride::Enable)
    );
    assert_eq!(
        editing.flags.get(&TerminalFlag::GenerateSignals),
        Some(&TtyOverride::Disable)
    );
}

#[test]
fn expands_legacy_coordinates() {
    assert_eq!(commands::required_parameters(b"\x1b[%i%d;%dH"), 2);
    assert_eq!(
        commands::expand_legacy_sequence(b"\x1b[%i%d;%dH", 12, 4),
        b"\x1b[5;13H"
    );
    assert_eq!(
        commands::expand_legacy_sequence(b"%r%2,%3", 7, 42),
        b"07,042"
    );
}

#[test]
fn parses_legacy_tty_characters() {
    assert_eq!(
        tty::parse_tty_character(""),
        ControlCharacter::EndOfLine.default_value()
    );
    assert_eq!(tty::parse_tty_character("X"), u8::MAX);
    assert_eq!(tty::parse_tty_character("XY"), b'X');
    assert_eq!(tty::parse_tty_character("^H"), 0x08);
    assert_eq!(tty::parse_tty_character("\\377"), 0xff);
    assert_eq!(tty::parse_tty_character("\\400"), u8::MAX);
    assert_eq!(tty::parse_tty_character("\\U+0041"), b'A');
    assert_eq!(tty::parse_tty_character("\\U+00ff"), u8::MAX);
}
