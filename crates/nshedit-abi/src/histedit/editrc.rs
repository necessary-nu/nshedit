//! Native tokenization and ABI-owned editrc command dispatch.

use super::*;

pub(super) unsafe fn environment_value(el: *mut EditLine, name: &str) -> Option<Vec<u8>> {
    let callback = unsafe { (&*el).environment_callback() };
    if let Some(callback) = callback {
        let name = CString::new(name).expect("environment keys are static ASCII");
        let value = unsafe { callback(name.as_ptr()) };
        return unsafe { cbytes(value) }.map(<[u8]>::to_vec);
    }
    crate::adapter::secure_environment(name)
}

pub(super) unsafe fn parse_editrc_line(el: *mut EditLine, input: &[TextUnit]) -> c_int {
    let text: Text = input.iter().copied().collect();
    let Ok(cursor) = text.index(text.len()) else {
        return -1;
    };
    let Ok(NativeTokenization::Complete(parsed)) =
        NativeTokenizer::default().tokenize(&text, cursor)
    else {
        return -1;
    };
    let storage: Vec<Vec<u32>> = parsed
        .tokens()
        .iter()
        .map(|token| {
            token
                .value()
                .as_units()
                .iter()
                .copied()
                .map(crate::adapter::unit_to_wide)
                .collect()
        })
        .collect();
    let words: Vec<&[u32]> = storage.iter().map(Vec::as_slice).collect();
    unsafe { dispatch_editrc(el, &words) }
}

fn wide_word(word: &[u32]) -> Option<String> {
    word.iter().copied().map(char::from_u32).collect()
}

pub(super) unsafe fn dispatch_editrc(el: *mut EditLine, words: &[&[u32]]) -> c_int {
    let Some(first) = words.first().and_then(|word| wide_word(word)) else {
        return -1;
    };
    let command = if let Some((qualifier, command)) = first.split_once(':') {
        if qualifier.is_empty() {
            return 0;
        }
        let program = unsafe { (&*el).program() }.to_string_lossy();
        let matches = program.contains(qualifier)
            || regex::Regex::new(qualifier).is_ok_and(|pattern| pattern.is_match(&program));
        if !matches {
            return 0;
        }
        command
    } else {
        first.as_str()
    };

    let status = match command {
        "bind" => unsafe { (&mut *el).bind_command(words) },
        "edit" => match words.get(1).and_then(|word| wide_word(word)).as_deref() {
            Some("on") => {
                unsafe { (&mut *el).set_editing_enabled(true) };
                0
            }
            Some("off") => {
                let _ =
                    unsafe { (&mut *el).set_terminal_mode(nshedit::domain::TerminalMode::Cooked) };
                unsafe { (&mut *el).set_editing_enabled(false) };
                0
            }
            _ => -1,
        },
        "setty" => unsafe { (&mut *el).set_tty_modes(words) },
        "echotc" | "telltc" | "settc" if words.len() > 1 => 0,
        "history" => 0,
        _ => return -1,
    };
    -status
}
