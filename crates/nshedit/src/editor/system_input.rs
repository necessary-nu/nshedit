//! Native host input mapped onto the editor's existing read protocol.

use std::io::{self, Read};
#[cfg(unix)]
use std::os::fd::BorrowedFd as BorrowedIo;
#[cfg(windows)]
use std::os::windows::io::BorrowedHandle as BorrowedIo;

#[cfg(windows)]
use crate::domain::Signal;

use super::effect::{ReadEffect, ReadOutcome};

#[cfg(windows)]
enum Source<'io> {
    Console(nshedit_plat::terminal::ConsoleReader<'io>),
    Stream(BorrowedIo<'io>),
}

/// Platform input state kept by the host while the editor is suspended.
///
/// Unix and Windows stream handles retain incremental byte reads. A real
/// Windows console instead owns a structured record decoder in the platform
/// crate, whose results map directly into [`ReadOutcome`].
// [spec:nshedit:req:core.windows-session]
pub struct SystemInput<'io> {
    #[cfg(unix)]
    input: BorrowedIo<'io>,
    #[cfg(windows)]
    source: Source<'io>,
}

impl<'io> SystemInput<'io> {
    /// Classify and borrow an input descriptor or handle.
    pub fn new(input: BorrowedIo<'io>) -> io::Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self { input })
        }
        #[cfg(windows)]
        {
            let source = match nshedit_plat::terminal::handle_kind(input)? {
                nshedit_plat::terminal::HandleKind::Console => {
                    Source::Console(nshedit_plat::terminal::ConsoleReader::new(input)?)
                }
                nshedit_plat::terminal::HandleKind::Stream => Source::Stream(input),
            };
            Ok(Self { source })
        }
    }

    /// Satisfy one typed read request from the caller-owned input stream.
    ///
    /// `input` must refer to the same operating-system object borrowed by
    /// [`Self::new`]. It is used only for byte streams; real Windows consoles
    /// are read as structured records by the platform layer.
    pub fn read(&mut self, input: &mut dyn Read, purpose: ReadEffect) -> io::Result<ReadOutcome> {
        #[cfg(unix)]
        {
            if purpose == ReadEffect::KeySequence
                && nshedit_plat::terminal::bytes_ready(self.input)? == 0
            {
                return Ok(ReadOutcome::TimedOut);
            }
            read_stream(input)
        }
        #[cfg(windows)]
        {
            match &mut self.source {
                Source::Console(console) => {
                    let read = if purpose == ReadEffect::KeySequence {
                        let Some(read) = console.try_read()? else {
                            return Ok(ReadOutcome::TimedOut);
                        };
                        read
                    } else {
                        console.read()?
                    };
                    Ok(console_outcome(read))
                }
                Source::Stream(handle) => {
                    if purpose == ReadEffect::KeySequence
                        && nshedit_plat::terminal::stream_bytes_ready(*handle)? == 0
                    {
                        return Ok(ReadOutcome::TimedOut);
                    }
                    read_stream(input)
                }
            }
        }
    }
}

fn read_stream(input: &mut dyn Read) -> io::Result<ReadOutcome> {
    let mut byte = [0];
    match input.read(&mut byte)? {
        0 => Ok(ReadOutcome::EndOfInput),
        _ => Ok(ReadOutcome::Bytes(byte.into())),
    }
}

#[cfg(windows)]
fn console_outcome(read: nshedit_plat::terminal::ConsoleRead) -> ReadOutcome {
    match read {
        nshedit_plat::terminal::ConsoleRead::Bytes(bytes) => ReadOutcome::Bytes(bytes),
        nshedit_plat::terminal::ConsoleRead::Interrupt => ReadOutcome::Signal(Signal::Interrupt),
        nshedit_plat::terminal::ConsoleRead::Resize => ReadOutcome::Signal(Signal::Resize),
        nshedit_plat::terminal::ConsoleRead::EndOfInput => ReadOutcome::EndOfInput,
    }
}
