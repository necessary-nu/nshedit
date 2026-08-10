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
            if let ReadEffect::KeySequence { deadline } = purpose
                && !nshedit_plat::terminal::wait_for_input(self.input, deadline.remaining())?
            {
                return Ok(ReadOutcome::TimedOut);
            }
            read_stream(input)
        }
        #[cfg(windows)]
        {
            match &mut self.source {
                Source::Console(console) => {
                    let read = if let ReadEffect::KeySequence { deadline } = purpose {
                        let Some(read) = console.read_for(deadline.remaining())? else {
                            return Ok(ReadOutcome::TimedOut);
                        };
                        read
                    } else {
                        console.read()?
                    };
                    Ok(console_outcome(read))
                }
                Source::Stream(handle) => {
                    if let ReadEffect::KeySequence { deadline } = purpose
                        && !nshedit_plat::terminal::wait_for_stream_input(
                            *handle,
                            deadline.remaining(),
                        )?
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

#[cfg(all(test, unix))]
mod tests {
    use super::super::effect::ReadDeadline;
    use super::*;
    use std::io::Write;
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    #[test]
    fn deadline_reads_ready_or_times_out() {
        let (mut input, mut writer) = UnixStream::pair().expect("socket pair");
        let descriptor = input.try_clone().expect("descriptor clone");
        let mut system = SystemInput::new(descriptor.as_fd()).expect("system input");
        let expired = ReadEffect::KeySequence {
            deadline: ReadDeadline::after(Duration::ZERO),
        };

        assert_eq!(
            system.read(&mut input, expired).expect("empty read"),
            ReadOutcome::TimedOut
        );

        writer.write_all(b"x").expect("buffer input");
        assert_eq!(
            system.read(&mut input, expired).expect("ready read"),
            ReadOutcome::Bytes(Box::new(*b"x"))
        );
    }
}
