use football_domain::{ClockError, ClockMilliseconds, MatchClock, MatchPeriod, UtcInstant};

/// Application boundary for obtaining the current UTC server time.
///
/// The domain never calls the operating-system clock. Production adapters and
/// deterministic tests provide this port explicitly.
pub trait ClockPort {
    fn now_utc(&self) -> UtcInstant;
}

/// Coordinates clock operations with an injected UTC clock port.
pub struct MatchClockCoordinator<C> {
    clock: C,
}

impl<C> MatchClockCoordinator<C>
where
    C: ClockPort,
{
    pub const fn new(clock: C) -> Self {
        Self { clock }
    }

    pub fn current_time(&self, match_clock: &MatchClock) -> Result<ClockMilliseconds, ClockError> {
        match_clock.current_at(self.clock.now_utc())
    }

    pub fn start(
        &self,
        match_clock: &mut MatchClock,
        period: MatchPeriod,
    ) -> Result<(), ClockError> {
        match_clock.start(period, self.clock.now_utc())
    }

    pub fn pause(&self, match_clock: &mut MatchClock) -> Result<(), ClockError> {
        match_clock.pause(self.clock.now_utc())
    }

    pub fn resume(&self, match_clock: &mut MatchClock) -> Result<(), ClockError> {
        match_clock.resume(self.clock.now_utc())
    }

    pub fn correct(
        &self,
        match_clock: &mut MatchClock,
        corrected_clock: ClockMilliseconds,
    ) -> Result<(), ClockError> {
        match_clock.correct(self.clock.now_utc(), corrected_clock)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use football_domain::{ClockMilliseconds, MatchClock, MatchPeriod, UtcInstant};

    use super::{ClockPort, MatchClockCoordinator};

    struct FixedClock {
        now: Cell<UtcInstant>,
    }

    impl FixedClock {
        fn new(now: UtcInstant) -> Self {
            Self {
                now: Cell::new(now),
            }
        }

        fn set(&self, now: UtcInstant) {
            self.now.set(now);
        }
    }

    impl ClockPort for FixedClock {
        fn now_utc(&self) -> UtcInstant {
            self.now.get()
        }
    }

    #[test]
    fn coordinator_uses_the_injected_clock_for_display_time() {
        let fixed_clock = FixedClock::new(UtcInstant::from_unix_timestamp(1_000).unwrap());
        let coordinator = MatchClockCoordinator::new(fixed_clock);
        let mut match_clock = MatchClock::new();

        coordinator
            .start(&mut match_clock, MatchPeriod::FirstHalf)
            .unwrap();
        coordinator
            .clock
            .set(UtcInstant::from_unix_timestamp(1_002).unwrap());

        assert_eq!(
            coordinator.current_time(&match_clock).unwrap(),
            ClockMilliseconds::from_seconds(2).unwrap()
        );
    }

    #[test]
    fn coordinator_routes_pause_resume_and_correction_through_the_same_port() {
        let fixed_clock = FixedClock::new(UtcInstant::from_unix_timestamp(2_000).unwrap());
        let coordinator = MatchClockCoordinator::new(fixed_clock);
        let mut match_clock = MatchClock::new();

        coordinator
            .start(&mut match_clock, MatchPeriod::FirstHalf)
            .unwrap();
        coordinator
            .clock
            .set(UtcInstant::from_unix_timestamp(2_005).unwrap());
        coordinator.pause(&mut match_clock).unwrap();
        coordinator
            .clock
            .set(UtcInstant::from_unix_timestamp(2_020).unwrap());
        assert_eq!(
            coordinator.current_time(&match_clock).unwrap(),
            ClockMilliseconds::from_seconds(5).unwrap()
        );

        coordinator.resume(&mut match_clock).unwrap();
        coordinator
            .clock
            .set(UtcInstant::from_unix_timestamp(2_021).unwrap());
        coordinator
            .correct(
                &mut match_clock,
                ClockMilliseconds::from_seconds(30).unwrap(),
            )
            .unwrap();

        assert_eq!(match_clock.base_clock_ms().value(), 30_000);
    }
}
