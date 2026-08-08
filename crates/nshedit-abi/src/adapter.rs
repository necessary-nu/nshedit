//! Opaque owners behind the five incomplete `histedit.h` handle types.
//!
//! A C caller knows only the pointer spelling of these values. The allocation
//! behind that pointer belongs to this crate and contains both the native Rust
//! object and the boundary-only storage whose pointers the C API lends out.
//! During the cutover, `compatibility` retains the translated state needed by
//! corpus behaviours that have not yet been expressed through the native
//! semantic API.

use core::ffi::{c_char, c_int};
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
use std::ffi::CString;
use std::io;

use nshedit::chartype::CtBufferT;
use nshedit::domain::{EditorConfig, TerminalMode, Text, TextUnit};
use nshedit::editor::{Editor, TerminalControl, Tokenizer as NativeTokenizer};
use nshedit::histedit::{LineInfo, LineInfoW};
use nshedit::history::HistoryStore;

/// Terminal ownership remains with the translated engine during the bounded
/// ABI cutover. This controller lets the native editor own an explicit RAII
/// obligation without touching the same terminal twice.
struct CompatibilityTerminal;

impl TerminalControl for CompatibilityTerminal {
    fn activate(&mut self, _config: EditorConfig) -> io::Result<()> {
        Ok(())
    }

    fn set_mode(&mut self, _mode: TerminalMode) -> io::Result<()> {
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct EditLineBoundary {
    narrow_conversion: CtBufferT,
    narrow_line: Box<LineInfo>,
    wide_line: Box<LineInfoW>,
    terminal_name: Option<CString>,
    word_characters: Option<Vec<u32>>,
}

impl EditLineBoundary {
    fn new() -> Self {
        Self {
            narrow_conversion: CtBufferT {
                cbuff: Vec::new(),
                csize: 0,
                wbuff: Vec::new(),
                wsize: 0,
            },
            narrow_line: Box::new(LineInfo {
                buffer: core::ptr::null(),
                cursor: core::ptr::null(),
                lastchar: core::ptr::null(),
            }),
            wide_line: Box::new(LineInfoW {
                buffer: core::ptr::null(),
                cursor: core::ptr::null(),
                lastchar: core::ptr::null(),
            }),
            terminal_name: None,
            word_characters: None,
        }
    }
}

// [spec:nshedit:req:abi.opaque-owner]
/// Allocation behind C's incomplete `EditLine` handle.
///
/// `compatibility` is deliberately the first field. The translated command
/// engine invokes callbacks with a pointer to that field; offset zero makes
/// the pointer value identical to the enclosing ABI handle, so a callback can
/// safely re-enter an exported function with the handle it received. The
/// native [`Editor`] has a separate representation and is never cast to C.
#[repr(C)]
pub struct EditLine {
    compatibility: ManuallyDrop<nshedit::el::EditLine>,
    native: Editor<CompatibilityTerminal>,
    boundary: EditLineBoundary,
}

impl EditLine {
    pub(crate) fn from_compatibility(
        compatibility: Box<nshedit::el::EditLine>,
    ) -> Option<Box<Self>> {
        let native = match Editor::new(EditorConfig::default(), CompatibilityTerminal) {
            Ok(native) => native,
            Err(_) => {
                nshedit::el::el_end(Some(compatibility));
                return None;
            }
        };
        let owner = Box::new(Self {
            compatibility: ManuallyDrop::new(*compatibility),
            native,
            boundary: EditLineBoundary::new(),
        });
        debug_assert!(owner.native.line().is_empty());
        Some(owner)
    }

    pub(crate) fn compatibility_ptr(&mut self) -> *mut nshedit::el::EditLine {
        core::ptr::from_mut(&mut **self)
    }

    pub(crate) fn narrow_conversion_mut(&mut self) -> &mut CtBufferT {
        &mut self.boundary.narrow_conversion
    }

    pub(crate) fn narrow_line_ptr(&mut self) -> *mut LineInfo {
        core::ptr::from_mut(self.boundary.narrow_line.as_mut())
    }

    pub(crate) fn publish_wide_line(&mut self) -> *const LineInfoW {
        let buffer = self.el_line.buffer.as_ptr();
        *self.boundary.wide_line = LineInfoW {
            buffer,
            // SAFETY: these are the translated line state's checked offsets;
            // `lastchar` may point one element past the used contents.
            cursor: unsafe { buffer.add(self.el_line.cursor) },
            // SAFETY: as above.
            lastchar: unsafe { buffer.add(self.el_line.lastchar) },
        };
        core::ptr::from_ref(self.boundary.wide_line.as_ref())
    }

    pub(crate) fn publish_terminal_name(&mut self, name: Option<&str>) -> *const c_char {
        self.boundary.terminal_name = name.and_then(|name| CString::new(name).ok());
        self.boundary
            .terminal_name
            .as_ref()
            .map_or(core::ptr::null(), |name| name.as_ptr())
    }

    pub(crate) fn publish_word_characters(
        &mut self,
        word_characters: Option<Vec<u32>>,
    ) -> *const u32 {
        self.boundary.word_characters = word_characters.map(|mut characters| {
            characters.push(0);
            characters
        });
        self.boundary
            .word_characters
            .as_ref()
            .map_or(core::ptr::null(), |characters| characters.as_ptr())
    }
}

impl Deref for EditLine {
    type Target = nshedit::el::EditLine;

    fn deref(&self) -> &Self::Target {
        &self.compatibility
    }
}

impl DerefMut for EditLine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.compatibility
    }
}

impl Drop for EditLine {
    fn drop(&mut self) {
        // SAFETY: this is the only `take`; `compatibility` is never exposed as
        // an owning value and `ManuallyDrop` suppresses the automatic drop.
        let compatibility = unsafe { ManuallyDrop::take(&mut self.compatibility) };
        nshedit::el::el_end(Some(Box::new(compatibility)));
    }
}

// [spec:nshedit:req:abi.opaque-owner]
/// Allocation behind either incomplete C history handle.
pub struct HistoryHandle<C: nshedit::history::HistChar> {
    compatibility: Option<Box<nshedit::history::HistoryGen<C>>>,
    native: HistoryStore,
}

pub type History = HistoryHandle<c_char>;
pub type HistoryW = HistoryHandle<u32>;

impl<C: nshedit::history::HistChar> HistoryHandle<C> {
    pub(crate) fn from_compatibility(
        compatibility: *mut nshedit::history::HistoryGen<C>,
    ) -> *mut Self {
        if compatibility.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: the compatibility constructor returned this allocation and
        // transferred ownership to the ABI boundary.
        let compatibility = unsafe { Box::from_raw(compatibility) };
        let owner = Box::new(Self {
            compatibility: Some(compatibility),
            native: HistoryStore::new(),
        });
        debug_assert!(owner.native.is_empty());
        Box::into_raw(owner)
    }

    pub(crate) fn compatibility_ptr(&mut self) -> *mut nshedit::history::HistoryGen<C> {
        self.compatibility
            .as_deref_mut()
            .map_or(core::ptr::null_mut(), core::ptr::from_mut)
    }

    pub(crate) fn compatibility_mut(&mut self) -> &mut nshedit::history::HistoryGen<C> {
        self.compatibility
            .as_deref_mut()
            .expect("a live history owns its compatibility state")
    }

    pub(crate) fn take_compatibility(&mut self) -> Option<Box<nshedit::history::HistoryGen<C>>> {
        self.compatibility.take()
    }
}

impl<C: nshedit::history::HistChar> Drop for HistoryHandle<C> {
    fn drop(&mut self) {
        if let Some(compatibility) = self.compatibility.take() {
            nshedit::history::history_end_gen(Box::into_raw(compatibility));
        }
    }
}

// [spec:nshedit:req:abi.opaque-owner]
/// Allocation behind either incomplete C tokenizer handle.
pub struct TokenizerHandle<C: nshedit::tokenizer::TokChar> {
    compatibility: Option<Box<nshedit::tokenizer::TokenizerGen<C>>>,
    native: NativeTokenizer,
    argv: Vec<*const C>,
}

pub type Tokenizer = TokenizerHandle<c_char>;
pub type TokenizerW = TokenizerHandle<u32>;

impl TokenizerHandle<c_char> {
    pub(crate) fn from_narrow(
        compatibility: Box<nshedit::tokenizer::Tokenizer>,
        separators: Option<&[c_char]>,
    ) -> Box<Self> {
        let native = separators.map_or_else(NativeTokenizer::default, |separators| {
            NativeTokenizer::new(
                separators
                    .iter()
                    .copied()
                    .map(|byte| {
                        let byte = byte as u8;
                        if byte.is_ascii() {
                            TextUnit::Scalar(char::from(byte))
                        } else {
                            TextUnit::RawByte(byte)
                        }
                    })
                    .collect::<Text>(),
            )
        });
        Self::new(compatibility, native)
    }
}

impl TokenizerHandle<u32> {
    pub(crate) fn from_wide(
        compatibility: Box<nshedit::tokenizer::TokenizerW>,
        separators: Option<&[u32]>,
    ) -> Box<Self> {
        let native = separators.map_or_else(NativeTokenizer::default, |separators| {
            NativeTokenizer::new(
                separators
                    .iter()
                    .copied()
                    .map(TextUnit::from_wide)
                    .collect::<Text>(),
            )
        });
        Self::new(compatibility, native)
    }
}

impl<C: nshedit::tokenizer::TokChar> TokenizerHandle<C> {
    fn new(
        compatibility: Box<nshedit::tokenizer::TokenizerGen<C>>,
        native: NativeTokenizer,
    ) -> Box<Self> {
        let owner = Box::new(Self {
            compatibility: Some(compatibility),
            native,
            argv: Vec::new(),
        });
        debug_assert!(
            owner
                .native
                .separators()
                .as_units()
                .iter()
                .all(|unit| !matches!(unit, TextUnit::Scalar('\n')))
        );
        owner
    }

    pub(crate) fn compatibility(&self) -> &nshedit::tokenizer::TokenizerGen<C> {
        self.compatibility
            .as_deref()
            .expect("a live tokenizer owns its compatibility state")
    }

    pub(crate) fn compatibility_mut(&mut self) -> &mut nshedit::tokenizer::TokenizerGen<C> {
        self.compatibility
            .as_deref_mut()
            .expect("a live tokenizer owns its compatibility state")
    }

    pub(crate) fn publish_argv(&mut self, argc: c_int) -> *mut *const C {
        let count = usize::try_from(argc.max(0)).unwrap_or(0);
        let compatibility = self.compatibility();
        let base = compatibility.wspace.as_ptr();
        let mut argv = Vec::with_capacity(count + 1);
        for index in 0..=count {
            let pointer = match compatibility.argv.get(index).copied().flatten() {
                // SAFETY: published offsets refer to this `wspace` allocation.
                Some(offset) => unsafe { base.add(offset) },
                None => core::ptr::null(),
            };
            argv.push(pointer);
        }
        self.argv = argv;
        self.argv.as_mut_ptr()
    }
}

impl<C: nshedit::tokenizer::TokChar> Deref for TokenizerHandle<C> {
    type Target = nshedit::tokenizer::TokenizerGen<C>;

    fn deref(&self) -> &Self::Target {
        self.compatibility()
    }
}

impl<C: nshedit::tokenizer::TokChar> DerefMut for TokenizerHandle<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.compatibility_mut()
    }
}

impl<C: nshedit::tokenizer::TokChar> Drop for TokenizerHandle<C> {
    fn drop(&mut self) {
        if let Some(compatibility) = self.compatibility.take() {
            nshedit::tokenizer::tok_end_gen(compatibility);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EditLine;

    #[test]
    fn callback_handle_address_is_preserved() {
        assert_eq!(core::mem::offset_of!(EditLine, compatibility), 0);
    }
}
