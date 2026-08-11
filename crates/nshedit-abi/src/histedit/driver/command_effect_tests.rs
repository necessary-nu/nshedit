use core::ffi::{c_char, c_int, c_void};
use std::ffi::{CStr, CString};

use nshedit::domain::{Direction, OpaqueCodePoint, RepeatCount, Text, TextUnit};
use nshedit::editor::effect::{
    AliasEffect, AliasResponse, HistoryLineEffect, HistoryMatch, HistoryPosition,
    HistorySearchEffect, HistorySearchInput, HistorySelection, HistoryWordEffect,
    HistoryWordPosition, HistoryWordResponse,
};

use crate::cdecl::histedit::{H_FIRST, H_NEXT};

use super::*;

struct TestHistory {
    entries: Vec<(c_int, Vec<u32>)>,
    cursor: usize,
}

unsafe extern "C" fn history_callback(
    cookie: *mut c_void,
    event: *mut HistEventW,
    operation: c_int,
    _: ...
) -> c_int {
    let history = unsafe { &mut *cookie.cast::<TestHistory>() };
    match operation {
        H_FIRST => history.cursor = 0,
        H_NEXT => history.cursor += 1,
        _ => return -1,
    }
    let Some((number, line)) = history.entries.get(history.cursor) else {
        return -1;
    };
    unsafe {
        (*event).num = *number;
        (*event).str = line.as_ptr();
    }
    0
}

unsafe extern "C" fn alias_callback(cookie: *mut c_void, name: *const c_char) -> *const c_char {
    let name = unsafe { CStr::from_ptr(name) };
    if name.to_bytes() != b"_g" {
        return core::ptr::null();
    }
    let expansion = unsafe { &*cookie.cast::<CString>() };
    expansion.as_ptr()
}

fn wide_line(line: &str) -> Vec<u32> {
    line.chars().map(u32::from).chain([0]).collect()
}

fn editor() -> Box<EditLine> {
    EditLine::new(SessionInit::inert("effect-test"))
        .expect("construct an editor over inert descriptors")
}

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn history_effect_adapter() {
    let mut editor = editor();
    let history = Box::new(TestHistory {
        entries: vec![(20, wide_line("git status")), (10, wide_line("cargo test"))],
        cursor: 0,
    });
    let cookie = Box::into_raw(history);
    editor
        .set_history_callback(
            Some(history_callback),
            cookie.cast::<c_void>(),
            HistoryEncoding::Wide,
        )
        .expect("register a complete history source");

    let search = HistorySearchEffect {
        input: HistorySearchInput::Pattern(Text::from("car.+test")),
        direction: Direction::Previous,
        matching: HistoryMatch::LiteralOrRegex,
    };
    let response = unsafe { host_history_search(&raw mut *editor, &search) }.unwrap();
    assert_eq!(
        response.history.selection(),
        &HistorySelection::Entry(Text::from("cargo test"))
    );
    assert_eq!(editor.history_depth(), 2);

    editor.set_history_depth(0);
    let selection =
        HistoryLineEffect::select(HistoryPosition::Number(RepeatCount::new(10).unwrap()));
    let response = unsafe { host_history_line(&raw mut *editor, &selection) }.unwrap();
    assert_eq!(
        response.selection(),
        &HistorySelection::Entry(Text::from("cargo test"))
    );

    let word = HistoryWordEffect {
        position: HistoryWordPosition::Last,
    };
    assert_eq!(
        unsafe { host_history_word(&raw mut *editor, &word) },
        Ok(HistoryWordResponse::Word(Text::from("status")))
    );

    drop(unsafe { Box::from_raw(cookie) });
}

// [spec:nshedit:req:abi.history-effects+2/test]
#[test]
fn literal_then_rust_regex() {
    let matching = HistoryMatch::LiteralOrRegex;

    assert!(history_matches(
        &Text::from("cargo test"),
        &Text::from("car.+test"),
        matching,
    ));
    assert!(history_matches(
        &Text::from("cargo [test"),
        &Text::from("[test"),
        matching,
    ));
    assert!(!history_matches(
        &Text::from("cargo test"),
        &Text::from("[test"),
        matching,
    ));

    let opaque = TextUnit::OpaqueCodePoint(OpaqueCodePoint::new(0xd800).unwrap());
    let non_scalar: Text = [TextUnit::RawByte(0xff), opaque].into_iter().collect();
    assert!(history_matches(&non_scalar, &non_scalar, matching));
    assert!(!history_matches(&non_scalar, &Text::from(".+"), matching,));
}

// [spec:nshedit:req:core.command-effects/test]
#[test]
fn alias_effect_adapter() {
    let mut editor = editor();
    let expansion = Box::new(CString::new("git status").unwrap());
    let cookie = Box::into_raw(expansion);
    editor.set_alias_callback(Some(alias_callback), cookie.cast::<c_void>());

    let response = unsafe {
        host_alias(
            &raw mut *editor,
            &AliasEffect {
                name: Text::from("_g"),
            },
        )
    };
    assert_eq!(
        response,
        Ok(AliasResponse::Expansion(Text::from("git status")))
    );

    drop(unsafe { Box::from_raw(cookie) });
}
