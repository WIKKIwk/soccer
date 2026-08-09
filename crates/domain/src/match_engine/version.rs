use std::fmt;

/// Monotonic version of a match aggregate's canonical event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AggregateVersion(u64);

impl AggregateVersion {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn from_value(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    /// Returns the version after appending `event_count` canonical events.
    pub fn try_after_events(self, event_count: u64) -> Result<Self, AggregateVersionError> {
        self.0
            .checked_add(event_count)
            .map(Self)
            .ok_or(AggregateVersionError::Overflow {
                current: self.0,
                event_count,
            })
    }

    pub fn try_next(self) -> Result<Self, AggregateVersionError> {
        self.try_after_events(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateVersionError {
    Overflow { current: u64, event_count: u64 },
}

impl fmt::Display for AggregateVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow {
                current,
                event_count,
            } => write!(
                formatter,
                "aggregate version overflow after {event_count} events at version {current}"
            ),
        }
    }
}

impl std::error::Error for AggregateVersionError {}

#[cfg(test)]
mod tests {
    use super::{AggregateVersion, AggregateVersionError};

    #[test]
    fn starts_at_zero_and_advances_by_event_count() {
        let initial = AggregateVersion::initial();

        assert_eq!(initial.value(), 0);
        assert_eq!(initial.try_after_events(3).unwrap().value(), 3);
        assert_eq!(initial.try_next().unwrap().value(), 1);
    }

    #[test]
    fn zero_events_keep_the_current_version() {
        let current = AggregateVersion::initial().try_after_events(8).unwrap();

        assert_eq!(current.try_after_events(0).unwrap(), current);
    }

    #[test]
    fn rejects_version_overflow() {
        let current = AggregateVersion::initial()
            .try_after_events(u64::MAX)
            .expect("u64::MAX should be representable");

        assert_eq!(
            current.try_next().unwrap_err(),
            AggregateVersionError::Overflow {
                current: u64::MAX,
                event_count: 1,
            }
        );
    }
}
