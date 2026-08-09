use std::fmt;
use std::io;

use crate::domain::Error;

use super::super::RenderError;
use super::super::effect::{EffectStateError, HostFailure};

/// A read-driver failure that leaves the editor safe for `finish` or `Drop`.
#[derive(Debug)]
pub enum DriverError {
    /// A read was started while an earlier step was still live.
    Busy,
    /// A pending effect or display belongs to another driver.
    DifferentDriver,
    /// The supplied step is no longer the active step of this driver.
    StaleStep,
    /// No further unique driver step can be represented.
    SequenceExhausted,
    /// A timeout was returned for a normal blocking input request.
    UnexpectedTimeout,
    /// A repeat count or macro expansion exceeded the configured work bound.
    WorkLimitExceeded { limit: usize },
    /// A recorded semantic replay did not match its typed continuation.
    InvalidSequenceState,
    /// A typed editor-domain operation failed.
    Editor(Error),
    /// The editor rejected an effect suspension or response.
    Effect(EffectStateError),
    /// A required host operation failed.
    Host(HostFailure),
    /// A transactional terminal-mode change failed.
    Terminal(io::Error),
    /// Native display planning or emission failed.
    Render(RenderError),
}

impl fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => formatter.write_str("the read driver already has a live step"),
            Self::DifferentDriver => formatter.write_str("the step belongs to another read driver"),
            Self::StaleStep => formatter.write_str("the read-driver step is no longer current"),
            Self::SequenceExhausted => formatter.write_str("the read-driver sequence is exhausted"),
            Self::UnexpectedTimeout => formatter.write_str("normal input unexpectedly timed out"),
            Self::WorkLimitExceeded { limit } => {
                write!(
                    formatter,
                    "one input exceeded the driver work limit of {limit}"
                )
            }
            Self::InvalidSequenceState => {
                formatter.write_str("a semantic command sequence reached an invalid state")
            }
            Self::Editor(error) => write!(formatter, "editor command failed: {error}"),
            Self::Effect(error) => write!(formatter, "editor effect failed: {error}"),
            Self::Host(error) => error.fmt(formatter),
            Self::Terminal(error) => write!(formatter, "terminal transition failed: {error}"),
            Self::Render(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DriverError {}
