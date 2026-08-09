use super::*;

fn editor() -> Box<EditLine> {
    EditLine::new(
        "terminal-adapter-test",
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        core::ptr::null_mut(),
        -1,
        -1,
        -1,
    )
    .expect("construct an editor over inert descriptors")
}

fn text(value: &str) -> Text {
    value.chars().map(TextUnit::Scalar).collect()
}

#[test]
fn line_and_cursor_share_native_state() {
    let mut editor = editor();
    assert!(editor.replace_line(text("abcd")));
    assert_eq!(editor.native().line(), &text("abcd"));
    assert_eq!(editor.native().cursor().get(), 4);

    assert_eq!(editor.move_cursor(-2), 2);
    assert_eq!(editor.native().cursor().get(), 2);
    assert_eq!(editor.move_cursor(99), 4);
    assert_eq!(editor.move_cursor(-99), 0);
}

#[test]
fn wide_edits_are_native() {
    let mut editor = editor();
    assert!(editor.replace_line(text("abcd")));
    assert_eq!(editor.move_cursor(-2), 2);
    assert_eq!(editor.insert_wide(&[b'X' as u32, b'Y' as u32]), 0);
    assert_eq!(editor.native().line(), &text("abXYcd"));
    assert_eq!(editor.native().cursor().get(), 4);

    editor.delete_before_cursor(2);
    assert_eq!(editor.native().line(), &text("abcd"));
    assert_eq!(editor.native().cursor().get(), 2);
    editor.delete_before_cursor(3);
    assert_eq!(editor.native().line(), &text("abcd"));
}

#[test]
fn accepted_line_has_one_newline() {
    let mut editor = editor();
    assert!(editor.finish_accepted_line(text("first")));
    assert_eq!(editor.native().line(), &text("first\n"));
    assert!(editor.finish_accepted_line(text("second\n")));
    assert_eq!(editor.native().line(), &text("second\n"));
}

#[test]
fn range_delete_preserves_legacy_bug() {
    let mut editor = editor();
    assert!(editor.replace_line(text("abcdef")));
    assert_eq!(editor.delete_range(1, 3), 2);
    assert_eq!(editor.native().line(), &text("aded"));

    assert_eq!(editor.delete_range(2, 99), 0);
    assert_eq!(editor.delete_range(-1, 2), 0);
    assert_eq!(editor.native().line(), &text("aded"));
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
    assert!(editor.set_stream(0, input, 17));
    assert!(editor.set_stream(1, output, 23));
    assert!(!editor.set_stream(3, core::ptr::null_mut(), 99));

    assert_eq!(editor.stream(0), Some(input));
    assert_eq!(editor.stream(1), Some(output));
    assert_eq!(editor.descriptor(0), Some(17));
    assert_eq!(editor.descriptor(1), Some(23));
    assert_eq!(editor.boundary.terminal.borrow().input, 17);
    assert_eq!(editor.boundary.terminal.borrow().output, 23);
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
    assert_eq!(editor.control_eof(), termios::CEOF);
    assert_eq!(editor.control_reprint(), termios::CREPRINT);
    assert!(editor.write_output(b"x").is_err());
    assert!(editor.read_input(&mut [0]).is_err());
}
