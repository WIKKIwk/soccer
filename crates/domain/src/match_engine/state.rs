use std::fmt;

/// The initial match lifecycle from the roadmap.
///
/// Extra-time and penalty-shootout states are deliberately not present yet;
/// they are scheduled as later extensions after the basic C1 flow is proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchState {
    Draft,
    Scheduled,
    PreMatch,
    LineupsProvisional,
    LineupsConfirmed,
    FirstHalf,
    HalfTime,
    SecondHalf,
    Interrupted,
    Finished,
    Postponed,
    Cancelled,
}

impl MatchState {
    /// Returns the only state from which a new match lifecycle may begin.
    pub const fn initial() -> Self {
        Self::Draft
    }

    pub fn is_live_period(self) -> bool {
        matches!(self, Self::FirstHalf | Self::SecondHalf)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Scheduled)
                | (Self::Scheduled, Self::PreMatch)
                | (Self::Scheduled, Self::Postponed)
                | (Self::Scheduled, Self::Cancelled)
                | (Self::PreMatch, Self::LineupsProvisional)
                | (Self::PreMatch, Self::Postponed)
                | (Self::PreMatch, Self::Cancelled)
                | (Self::LineupsProvisional, Self::LineupsConfirmed)
                | (Self::LineupsConfirmed, Self::FirstHalf)
                | (Self::FirstHalf, Self::HalfTime)
                | (Self::FirstHalf, Self::Interrupted)
                | (Self::HalfTime, Self::SecondHalf)
                | (Self::SecondHalf, Self::Finished)
                | (Self::SecondHalf, Self::Interrupted)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchTransitionError {
    InvalidTransition { from: MatchState, to: MatchState },
    CannotInterruptNonLivePeriod { current: MatchState },
    CannotTransitionWhileInterrupted { to: MatchState },
    NotInterrupted,
}

impl fmt::Display for MatchTransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTransition { from, to } => {
                write!(formatter, "invalid match transition: {from:?} -> {to:?}")
            }
            Self::CannotInterruptNonLivePeriod { current } => {
                write!(formatter, "cannot interrupt match in {current:?}")
            }
            Self::CannotTransitionWhileInterrupted { to } => {
                write!(
                    formatter,
                    "cannot transition an interrupted match directly to {to:?}"
                )
            }
            Self::NotInterrupted => write!(formatter, "match is not interrupted"),
        }
    }
}

impl std::error::Error for MatchTransitionError {}

/// Small stateful wrapper that remembers the live period interrupted by an
/// operational interruption. Resumption must return to that exact period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchLifecycle {
    state: MatchState,
    interrupted_from: Option<MatchState>,
}

impl Default for MatchLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchLifecycle {
    pub const fn new() -> Self {
        Self {
            state: MatchState::initial(),
            interrupted_from: None,
        }
    }

    pub const fn state(self) -> MatchState {
        self.state
    }

    pub fn transition_to(&mut self, next: MatchState) -> Result<(), MatchTransitionError> {
        if self.state == MatchState::Interrupted {
            return Err(MatchTransitionError::CannotTransitionWhileInterrupted { to: next });
        }

        if next == MatchState::Interrupted {
            if !self.state.is_live_period() {
                return Err(MatchTransitionError::CannotInterruptNonLivePeriod {
                    current: self.state,
                });
            }

            self.interrupted_from = Some(self.state);
            self.state = next;
            return Ok(());
        }

        if !self.state.can_transition_to(next) {
            return Err(MatchTransitionError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }

        self.state = next;
        Ok(())
    }

    pub fn resume_interrupted(&mut self) -> Result<(), MatchTransitionError> {
        let Some(previous_state) = self.interrupted_from.take() else {
            return Err(MatchTransitionError::NotInterrupted);
        };

        debug_assert_eq!(self.state, MatchState::Interrupted);
        self.state = previous_state;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MatchLifecycle, MatchState, MatchTransitionError};

    #[test]
    fn new_lifecycle_starts_at_the_canonical_initial_state() {
        assert_eq!(MatchState::initial(), MatchState::Draft);
        assert_eq!(MatchLifecycle::new().state(), MatchState::initial());
    }

    fn confirmed_lineups() -> MatchLifecycle {
        let mut lifecycle = MatchLifecycle::new();
        lifecycle.transition_to(MatchState::Scheduled).unwrap();
        lifecycle.transition_to(MatchState::PreMatch).unwrap();
        lifecycle
            .transition_to(MatchState::LineupsProvisional)
            .unwrap();
        lifecycle
            .transition_to(MatchState::LineupsConfirmed)
            .unwrap();
        lifecycle
    }

    #[test]
    fn follows_the_initial_match_flow_to_finished() {
        let mut lifecycle = confirmed_lineups();
        lifecycle.transition_to(MatchState::FirstHalf).unwrap();
        lifecycle.transition_to(MatchState::HalfTime).unwrap();
        lifecycle.transition_to(MatchState::SecondHalf).unwrap();
        lifecycle.transition_to(MatchState::Finished).unwrap();

        assert_eq!(lifecycle.state(), MatchState::Finished);
    }

    #[test]
    fn postponement_and_cancellation_are_pre_match_extensions() {
        let mut scheduled = MatchLifecycle::new();
        scheduled.transition_to(MatchState::Scheduled).unwrap();
        scheduled.transition_to(MatchState::Postponed).unwrap();

        let mut pre_match = MatchLifecycle::new();
        pre_match.transition_to(MatchState::Scheduled).unwrap();
        pre_match.transition_to(MatchState::PreMatch).unwrap();
        pre_match.transition_to(MatchState::Cancelled).unwrap();

        assert_eq!(scheduled.state(), MatchState::Postponed);
        assert_eq!(pre_match.state(), MatchState::Cancelled);
    }

    #[test]
    fn interruption_resumes_the_same_live_period() {
        let mut first_half = confirmed_lineups();
        first_half.transition_to(MatchState::FirstHalf).unwrap();
        first_half.transition_to(MatchState::Interrupted).unwrap();
        assert_eq!(first_half.state(), MatchState::Interrupted);
        first_half.resume_interrupted().unwrap();
        assert_eq!(first_half.state(), MatchState::FirstHalf);

        let mut second_half = confirmed_lineups();
        second_half.transition_to(MatchState::FirstHalf).unwrap();
        second_half.transition_to(MatchState::HalfTime).unwrap();
        second_half.transition_to(MatchState::SecondHalf).unwrap();
        second_half.transition_to(MatchState::Interrupted).unwrap();
        second_half.resume_interrupted().unwrap();
        assert_eq!(second_half.state(), MatchState::SecondHalf);
    }

    #[test]
    fn rejects_invalid_transitions() {
        let mut lifecycle = MatchLifecycle::new();
        let error = lifecycle.transition_to(MatchState::FirstHalf).unwrap_err();

        assert_eq!(
            error,
            MatchTransitionError::InvalidTransition {
                from: MatchState::Draft,
                to: MatchState::FirstHalf,
            }
        );
    }

    #[test]
    fn interrupted_match_cannot_accept_direct_lifecycle_transition() {
        let mut lifecycle = confirmed_lineups();
        lifecycle.transition_to(MatchState::FirstHalf).unwrap();
        lifecycle.transition_to(MatchState::Interrupted).unwrap();

        let error = lifecycle.transition_to(MatchState::SecondHalf).unwrap_err();

        assert_eq!(
            error,
            MatchTransitionError::CannotTransitionWhileInterrupted {
                to: MatchState::SecondHalf,
            }
        );
    }

    #[test]
    fn interruption_is_only_valid_during_a_live_period() {
        let mut lifecycle = MatchLifecycle::new();
        let error = lifecycle
            .transition_to(MatchState::Interrupted)
            .unwrap_err();

        assert_eq!(
            error,
            MatchTransitionError::CannotInterruptNonLivePeriod {
                current: MatchState::Draft,
            }
        );
    }
}
