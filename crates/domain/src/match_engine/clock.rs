use crate::UtcInstant;
use std::fmt;

/// Football period represented by the initial C1 clock model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchPeriod {
    PreMatch,
    FirstHalf,
    HalfTime,
    SecondHalf,
}

impl MatchPeriod {
    /// Returns the stable lowercase vocabulary persisted with canonical events.
    pub const fn as_persisted_str(self) -> &'static str {
        match self {
            Self::PreMatch => "pre_match",
            Self::FirstHalf => "first_half",
            Self::HalfTime => "half_time",
            Self::SecondHalf => "second_half",
        }
    }

    /// Parses only the canonical-event period vocabulary.
    pub fn try_from_persisted_str(value: &str) -> Option<Self> {
        match value {
            "pre_match" => Some(Self::PreMatch),
            "first_half" => Some(Self::FirstHalf),
            "half_time" => Some(Self::HalfTime),
            "second_half" => Some(Self::SecondHalf),
            _ => None,
        }
    }

    pub const fn is_live(self) -> bool {
        matches!(self, Self::FirstHalf | Self::SecondHalf)
    }

    pub const fn is_break(self) -> bool {
        matches!(self, Self::PreMatch | Self::HalfTime)
    }
}

/// Non-negative football elapsed time in milliseconds.
///
/// This is match time, not a UTC instant and not a server wall-clock reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClockMilliseconds(u64);

impl ClockMilliseconds {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub const fn from_millis(value: u64) -> Self {
        Self(value)
    }

    pub fn from_seconds(value: u64) -> Result<Self, ClockMillisecondsError> {
        value
            .checked_mul(1_000)
            .map(Self)
            .ok_or(ClockMillisecondsError::SecondsOverflow { seconds: value })
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Result<Self, ClockMillisecondsError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(ClockMillisecondsError::AdditionOverflow {
                current: self.0,
                added: other.0,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockMillisecondsError {
    SecondsOverflow { seconds: u64 },
    AdditionOverflow { current: u64, added: u64 },
}

impl fmt::Display for ClockMillisecondsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecondsOverflow { seconds } => {
                write!(formatter, "clock seconds overflow at {seconds}")
            }
            Self::AdditionOverflow { current, added } => {
                write!(formatter, "clock millisecond overflow: {current} + {added}")
            }
        }
    }
}

impl std::error::Error for ClockMillisecondsError {}

/// Monotonic revision of the stored clock state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClockRevision(u64);

impl ClockRevision {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    fn try_next(self) -> Result<Self, ClockRevisionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(ClockRevisionError::Overflow { current: self.0 })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockRevisionError {
    Overflow { current: u64 },
}

impl fmt::Display for ClockRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow { current } => {
                write!(formatter, "clock revision overflow after {current}")
            }
        }
    }
}

impl std::error::Error for ClockRevisionError {}

/// Stored football-clock state.
///
/// The clock advances from `base_clock_ms` and an explicit UTC server anchor;
/// the database is not updated once per second. All operations accept the
/// server time explicitly so the domain remains deterministic until the clock
/// port is introduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchClock {
    period: MatchPeriod,
    base_clock_ms: ClockMilliseconds,
    anchor_server_time_utc: Option<UtcInstant>,
    is_running: bool,
    added_time_ms: ClockMilliseconds,
    last_corrected_at_utc: Option<UtcInstant>,
    version: ClockRevision,
}

impl MatchClock {
    pub const fn new() -> Self {
        Self {
            period: MatchPeriod::PreMatch,
            base_clock_ms: ClockMilliseconds::zero(),
            anchor_server_time_utc: None,
            is_running: false,
            added_time_ms: ClockMilliseconds::zero(),
            last_corrected_at_utc: None,
            version: ClockRevision::initial(),
        }
    }

    pub const fn period(self) -> MatchPeriod {
        self.period
    }

    pub const fn base_clock_ms(self) -> ClockMilliseconds {
        self.base_clock_ms
    }

    pub const fn anchor_server_time_utc(self) -> Option<UtcInstant> {
        self.anchor_server_time_utc
    }

    pub const fn is_running(self) -> bool {
        self.is_running
    }

    pub const fn added_time_ms(self) -> ClockMilliseconds {
        self.added_time_ms
    }

    pub const fn last_corrected_at_utc(self) -> Option<UtcInstant> {
        self.last_corrected_at_utc
    }

    pub const fn version(self) -> ClockRevision {
        self.version
    }

    pub fn start(
        &mut self,
        period: MatchPeriod,
        server_time_utc: UtcInstant,
    ) -> Result<(), ClockError> {
        if self.is_running {
            return Err(ClockError::AlreadyRunning);
        }

        let valid_transition = matches!(
            (self.period, period),
            (MatchPeriod::PreMatch, MatchPeriod::FirstHalf)
                | (MatchPeriod::HalfTime, MatchPeriod::SecondHalf)
        );
        if !valid_transition {
            return Err(ClockError::InvalidStartPeriod {
                current: self.period,
                requested: period,
            });
        }

        let next_version = self.next_version()?;
        self.period = period;
        self.anchor_server_time_utc = Some(server_time_utc);
        self.is_running = true;
        self.version = next_version;
        Ok(())
    }

    pub fn pause(&mut self, server_time_utc: UtcInstant) -> Result<(), ClockError> {
        if !self.is_running {
            return Err(ClockError::NotRunning);
        }

        let current_clock = self.current_at(server_time_utc)?;
        let next_version = self.next_version()?;
        self.base_clock_ms = current_clock;
        self.anchor_server_time_utc = None;
        self.is_running = false;
        self.version = next_version;
        Ok(())
    }

    pub fn resume(&mut self, server_time_utc: UtcInstant) -> Result<(), ClockError> {
        if self.is_running {
            return Err(ClockError::AlreadyRunning);
        }
        if !self.period.is_live() {
            return Err(ClockError::CannotResumeAtPeriod {
                period: self.period,
            });
        }

        let next_version = self.next_version()?;
        self.anchor_server_time_utc = Some(server_time_utc);
        self.is_running = true;
        self.version = next_version;
        Ok(())
    }

    pub fn correct(
        &mut self,
        server_time_utc: UtcInstant,
        corrected_clock: ClockMilliseconds,
    ) -> Result<(), ClockError> {
        if !self.period.is_live() {
            return Err(ClockError::CannotCorrectAtPeriod {
                period: self.period,
            });
        }
        if self.is_running {
            self.current_at(server_time_utc)?;
        }

        let next_version = self.next_version()?;
        self.base_clock_ms = corrected_clock;
        self.anchor_server_time_utc = self.is_running.then_some(server_time_utc);
        self.last_corrected_at_utc = Some(server_time_utc);
        self.version = next_version;
        Ok(())
    }

    pub fn current_at(&self, server_time_utc: UtcInstant) -> Result<ClockMilliseconds, ClockError> {
        if !self.is_running {
            return Ok(self.base_clock_ms);
        }

        let anchor = self
            .anchor_server_time_utc
            .ok_or(ClockError::MissingAnchor)?;
        let elapsed_millis = (server_time_utc.as_offset_datetime() - anchor.as_offset_datetime())
            .whole_milliseconds();
        let elapsed_millis =
            u64::try_from(elapsed_millis).map_err(|_| ClockError::ServerTimeBeforeAnchor);
        let elapsed_millis = elapsed_millis?;

        self.base_clock_ms
            .checked_add(ClockMilliseconds::from_millis(elapsed_millis))
            .map_err(|error| ClockError::ElapsedOverflow { error })
    }

    fn next_version(&self) -> Result<ClockRevision, ClockError> {
        self.version
            .try_next()
            .map_err(|error| ClockError::RevisionOverflow { error })
    }
}

impl Default for MatchClock {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    AlreadyRunning,
    NotRunning,
    InvalidStartPeriod {
        current: MatchPeriod,
        requested: MatchPeriod,
    },
    CannotResumeAtPeriod {
        period: MatchPeriod,
    },
    CannotCorrectAtPeriod {
        period: MatchPeriod,
    },
    MissingAnchor,
    ServerTimeBeforeAnchor,
    ElapsedOverflow {
        error: ClockMillisecondsError,
    },
    RevisionOverflow {
        error: ClockRevisionError,
    },
}

impl fmt::Display for ClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => formatter.write_str("match clock is already running"),
            Self::NotRunning => formatter.write_str("match clock is not running"),
            Self::InvalidStartPeriod { current, requested } => write!(
                formatter,
                "cannot start clock from {current:?} in requested period {requested:?}"
            ),
            Self::CannotResumeAtPeriod { period } => {
                write!(formatter, "cannot resume clock in {period:?}")
            }
            Self::CannotCorrectAtPeriod { period } => {
                write!(formatter, "cannot correct clock in {period:?}")
            }
            Self::MissingAnchor => formatter.write_str("running clock is missing its anchor"),
            Self::ServerTimeBeforeAnchor => {
                formatter.write_str("server time is before the clock anchor")
            }
            Self::ElapsedOverflow { error } => error.fmt(formatter),
            Self::RevisionOverflow { error } => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ClockError {}

#[cfg(test)]
mod tests {
    use time::{Duration, OffsetDateTime};

    use crate::UtcInstant;

    use super::{
        ClockError, ClockMilliseconds, ClockMillisecondsError, ClockRevision, MatchClock,
        MatchPeriod,
    };

    fn instant(milliseconds: i64) -> UtcInstant {
        UtcInstant::from_offset_datetime(
            OffsetDateTime::UNIX_EPOCH + Duration::milliseconds(milliseconds),
        )
    }

    #[test]
    fn distinguishes_live_periods_from_breaks() {
        assert!(MatchPeriod::FirstHalf.is_live());
        assert!(MatchPeriod::SecondHalf.is_live());
        assert!(MatchPeriod::PreMatch.is_break());
        assert!(MatchPeriod::HalfTime.is_break());
        assert!(!MatchPeriod::HalfTime.is_live());
    }

    #[test]
    fn match_period_persisted_vocabulary_round_trips_without_guessing() {
        let periods = [
            (MatchPeriod::PreMatch, "pre_match"),
            (MatchPeriod::FirstHalf, "first_half"),
            (MatchPeriod::HalfTime, "half_time"),
            (MatchPeriod::SecondHalf, "second_half"),
        ];

        for (period, persisted) in periods {
            assert_eq!(period.as_persisted_str(), persisted);
            assert_eq!(MatchPeriod::try_from_persisted_str(persisted), Some(period));
        }
        assert_eq!(MatchPeriod::try_from_persisted_str("full_time"), None);
    }

    #[test]
    fn stores_non_negative_elapsed_time_in_milliseconds() {
        assert_eq!(ClockMilliseconds::zero().value(), 0);
        assert_eq!(ClockMilliseconds::from_millis(90_000).value(), 90_000);
        assert_eq!(ClockMilliseconds::from_seconds(90).unwrap().value(), 90_000);
        assert_eq!(
            ClockMilliseconds::from_millis(45_000)
                .checked_add(ClockMilliseconds::from_millis(45_000))
                .unwrap()
                .value(),
            90_000
        );
    }

    #[test]
    fn rejects_clock_arithmetic_overflow() {
        assert_eq!(
            ClockMilliseconds::from_seconds(u64::MAX).unwrap_err(),
            ClockMillisecondsError::SecondsOverflow { seconds: u64::MAX }
        );
        assert_eq!(
            ClockMilliseconds::from_millis(u64::MAX)
                .checked_add(ClockMilliseconds::from_millis(1))
                .unwrap_err(),
            ClockMillisecondsError::AdditionOverflow {
                current: u64::MAX,
                added: 1,
            }
        );
    }

    #[test]
    fn starts_pauses_resumes_and_reports_anchored_elapsed_time() {
        let mut clock = MatchClock::new();
        let start = instant(1_000_000);

        clock.start(MatchPeriod::FirstHalf, start).unwrap();
        assert!(clock.is_running());
        assert_eq!(clock.version().value(), 1);
        assert_eq!(clock.current_at(instant(1_002_500)).unwrap().value(), 2_500);

        clock.pause(instant(1_005_000)).unwrap();
        assert!(!clock.is_running());
        assert_eq!(clock.base_clock_ms().value(), 5_000);
        assert_eq!(clock.current_at(instant(1_009_000)).unwrap().value(), 5_000);

        clock.resume(instant(1_010_000)).unwrap();
        assert_eq!(clock.current_at(instant(1_012_500)).unwrap().value(), 7_500);
        assert_eq!(clock.version().value(), 3);
    }

    #[test]
    fn correction_reanchors_a_running_clock_and_records_utc_correction_time() {
        let mut clock = MatchClock::new();
        let correction_time = instant(2_015_000);
        clock
            .start(MatchPeriod::FirstHalf, instant(2_000_000))
            .unwrap();

        clock
            .correct(correction_time, ClockMilliseconds::from_millis(30_000))
            .unwrap();

        assert_eq!(clock.base_clock_ms().value(), 30_000);
        assert_eq!(
            clock.current_at(instant(2_016_000)).unwrap().value(),
            31_000
        );
        assert_eq!(clock.last_corrected_at_utc(), Some(correction_time));
        assert_eq!(clock.version().value(), 2);
        assert_eq!(clock.added_time_ms(), ClockMilliseconds::zero());
    }

    #[test]
    fn rejects_invalid_clock_operations_without_changing_state() {
        let mut clock = MatchClock::new();
        let start = instant(3_000_000);

        assert_eq!(
            clock.start(MatchPeriod::SecondHalf, start).unwrap_err(),
            ClockError::InvalidStartPeriod {
                current: MatchPeriod::PreMatch,
                requested: MatchPeriod::SecondHalf,
            }
        );
        assert_eq!(clock.version(), ClockRevision::initial());
        assert_eq!(clock.pause(start).unwrap_err(), ClockError::NotRunning);
        assert_eq!(
            clock.resume(start).unwrap_err(),
            ClockError::CannotResumeAtPeriod {
                period: MatchPeriod::PreMatch,
            }
        );

        clock.start(MatchPeriod::FirstHalf, start).unwrap();
        assert_eq!(
            clock.start(MatchPeriod::FirstHalf, start).unwrap_err(),
            ClockError::AlreadyRunning
        );
        assert_eq!(
            clock.current_at(instant(2_999_999)).unwrap_err(),
            ClockError::ServerTimeBeforeAnchor
        );
    }
}
