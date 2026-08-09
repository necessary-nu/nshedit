//! The typed editor operations the byte-string entry points drive.
//!
//! `el_set`'s narrow arms and the readline layer ask the editor for the same
//! things, and in both the arguments arrive as C strings. Decoding them
//! through the editor's narrow conversion buffer is part of the operation
//! rather than of either caller, so it lives here and each caller names the
//! operation it wants instead of an operation code and a variadic tail.
//
// [spec:nshedit:req:abi.rust-internals]

use core::ffi::c_int;

use crate::adapter::{CommandCallback, EditLine};
use crate::conversion::decode_bytes;

/// An editor operation the editor refused.
///
/// Every C entry point that can raise it reports the same -1, so the reason
/// is not carried: a caller that needs one asks the editor directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Refused;

/// What a typed editor operation answers.
pub(crate) type Outcome = Result<(), Refused>;

/// The `0`/`-1` an exported entry point returns for `outcome`.
pub(crate) fn status(outcome: Outcome) -> c_int {
    match outcome {
        Ok(()) => 0,
        Err(Refused) => -1,
    }
}

/// The editor operations whose arguments arrive as an argument vector.
///
/// The C reaches all five through `el_set` with a command word written into
/// slot zero of the vector it builds; the word is what the handler dispatches
/// on, so it belongs to the operation and not to the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListCommand {
    /// C: `bind`.
    Bind,
    /// C: `telltc`.
    ReportCapabilities,
    /// C: `settc`.
    SetCapability,
    /// C: `echotc`.
    EchoCapability,
    /// C: `setty`.
    SetTtyModes,
}

impl ListCommand {
    /// The command word the handler dispatches on.
    const fn word(self) -> &'static [u32] {
        const BIND: &[u32] = &[0x62, 0x69, 0x6e, 0x64];
        const TELLTC: &[u32] = &[0x74, 0x65, 0x6c, 0x6c, 0x74, 0x63];
        const SETTC: &[u32] = &[0x73, 0x65, 0x74, 0x74, 0x63];
        const ECHOTC: &[u32] = &[0x65, 0x63, 0x68, 0x6f, 0x74, 0x63];
        const SETTY: &[u32] = &[0x73, 0x65, 0x74, 0x74, 0x79];
        match self {
            Self::Bind => BIND,
            Self::ReportCapabilities => TELLTC,
            Self::SetCapability => SETTC,
            Self::EchoCapability => ECHOTC,
            Self::SetTtyModes => SETTY,
        }
    }
}

/// Decode `arguments` through the editor's narrow conversion and run
/// `command` on the vector they form.
///
/// C: `ct_decode_argv` followed by the handler `el_set`'s arm selects. An
/// argument that does not decode in the current locale refuses the operation
/// without running the handler, as the C's NULL decode does. Each decoded
/// argument is owned before the next one overwrites the shared buffer.
pub(crate) fn run_list_command(
    editor: &mut EditLine,
    command: ListCommand,
    arguments: &[&[u8]],
) -> Outcome {
    let mut decoded: Vec<Vec<u32>> = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let wide = decode_bytes(Some(argument), editor.narrow_conversion_mut()).ok_or(Refused)?;
        decoded.push(wide.to_vec());
    }

    let mut vector: Vec<&[u32]> = Vec::with_capacity(decoded.len() + 1);
    vector.push(command.word());
    vector.extend(decoded.iter().map(Vec::as_slice));

    let refused = match command {
        ListCommand::Bind => editor.bind_command(&vector),
        ListCommand::SetTtyModes => editor.set_tty_modes(&vector),
        ListCommand::ReportCapabilities
        | ListCommand::SetCapability
        | ListCommand::EchoCapability => editor.terminal_command(&vector),
    };
    if refused == 0 { Ok(()) } else { Err(Refused) }
}

/// Register an editor command under a name and help text given as bytes.
///
/// C: `EL_ADDFN`'s narrow arm — both strings are decoded through the editor's
/// conversion buffer, and either failing to decode refuses the operation.
pub(crate) fn add_function(
    editor: &mut EditLine,
    name: &[u8],
    help: &[u8],
    callback: CommandCallback,
) -> Outcome {
    // Owned one at a time: the conversion buffer they share is overwritten by
    // the second decode.
    let name = decode_bytes(Some(name), editor.narrow_conversion_mut())
        .map(<[u32]>::to_vec)
        .ok_or(Refused)?;
    let help = decode_bytes(Some(help), editor.narrow_conversion_mut())
        .map(<[u32]>::to_vec)
        .ok_or(Refused)?;
    if editor.add_command(&name, &help, callback) {
        Ok(())
    } else {
        Err(Refused)
    }
}
