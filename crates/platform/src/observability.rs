use std::fmt;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoggingInitError {
    EmptyFilter,
    InvalidFilter(String),
    AlreadyInitialized,
}

impl fmt::Display for LoggingInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyFilter => formatter.write_str("log filter must not be empty"),
            Self::InvalidFilter(error) => write!(formatter, "invalid log filter: {error}"),
            Self::AlreadyInitialized => {
                formatter.write_str("logging subscriber is already initialized")
            }
        }
    }
}

impl std::error::Error for LoggingInitError {}

fn build_filter(raw_filter: &str) -> Result<EnvFilter, LoggingInitError> {
    if raw_filter.trim().is_empty() {
        return Err(LoggingInitError::EmptyFilter);
    }

    EnvFilter::try_new(raw_filter)
        .map_err(|error| LoggingInitError::InvalidFilter(error.to_string()))
}

/// Initialize one process-wide JSON structured logging subscriber.
pub fn init_logging(raw_filter: &str) -> Result<(), LoggingInitError> {
    let filter = build_filter(raw_filter)?;

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .try_init()
        .map_err(|_| LoggingInitError::AlreadyInitialized)
}

#[cfg(test)]
mod tests {
    use super::{LoggingInitError, build_filter};

    #[test]
    fn accepts_structured_target_filter() {
        assert!(build_filter("info,football_admin_api=debug,sqlx=warn").is_ok());
    }

    #[test]
    fn rejects_empty_filter() {
        assert!(matches!(
            build_filter("  "),
            Err(LoggingInitError::EmptyFilter)
        ));
    }

    #[test]
    fn rejects_malformed_filter() {
        assert!(matches!(
            build_filter("football_admin_api=["),
            Err(LoggingInitError::InvalidFilter(_))
        ));
    }
}
