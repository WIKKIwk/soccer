use football_platform::ApplicationError;
use std::{error::Error, fmt};

#[derive(Debug)]
pub(crate) enum LiveCommandError {
    InvalidArgument { field: String, message: String },
    Application(ApplicationError),
    LineupRepository(football_infrastructure::LineupRepositoryError),
    InvalidLineup(football_domain::LineupValidationError),
    InvalidCurrentTime(time::error::ComponentRange),
    ProcessingLeaseOutOfRange,
}

impl LiveCommandError {
    pub(crate) fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            field: field.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for LiveCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument { field, message } => {
                write!(formatter, "{field}: {message}")
            }
            Self::Application(error) => error.fmt(formatter),
            Self::LineupRepository(error) => error.fmt(formatter),
            Self::InvalidLineup(error) => error.fmt(formatter),
            Self::InvalidCurrentTime(error) => {
                write!(
                    formatter,
                    "current UTC time could not be normalized: {error}"
                )
            }
            Self::ProcessingLeaseOutOfRange => {
                formatter.write_str("command processing lease is outside the supported time range")
            }
        }
    }
}

impl Error for LiveCommandError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Application(error) => Some(error),
            Self::LineupRepository(error) => Some(error),
            Self::InvalidLineup(error) => Some(error),
            Self::InvalidCurrentTime(error) => Some(error),
            Self::InvalidArgument { .. } | Self::ProcessingLeaseOutOfRange => None,
        }
    }
}
