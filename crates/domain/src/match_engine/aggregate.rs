use super::{
    AggregateVersion, MatchId, MatchIdentity, MatchLifecycle, MatchState, MatchTransitionError,
    MatchVerificationStatus, MatchVerificationTransitionError,
};
use crate::FixtureId;

/// Domain boundary for one live match.
///
/// The aggregate owns the match identity and lifecycle together. Its mutable
/// behavior is intentionally added by later command and transition
/// increments; callers can inspect the current boundary without reaching into
/// its lifecycle representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchAggregate {
    identity: MatchIdentity,
    lifecycle: MatchLifecycle,
    verification_status: MatchVerificationStatus,
    version: AggregateVersion,
}

impl MatchAggregate {
    pub const fn new(identity: MatchIdentity) -> Self {
        Self {
            identity,
            lifecycle: MatchLifecycle::new(),
            verification_status: MatchVerificationStatus::initial(),
            version: AggregateVersion::initial(),
        }
    }

    pub const fn identity(&self) -> MatchIdentity {
        self.identity
    }

    pub const fn id(&self) -> MatchId {
        self.identity.id()
    }

    pub const fn fixture_id(&self) -> FixtureId {
        self.identity.fixture_id()
    }

    pub const fn state(&self) -> MatchState {
        self.lifecycle.state()
    }

    pub const fn verification_status(&self) -> MatchVerificationStatus {
        self.verification_status
    }

    pub const fn version(&self) -> AggregateVersion {
        self.version
    }

    pub fn schedule(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::Scheduled)
    }

    pub fn enter_pre_match(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::PreMatch)
    }

    pub fn submit_lineups(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::LineupsProvisional)
    }

    pub fn confirm_lineups(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::LineupsConfirmed)
    }

    pub fn start_first_half(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::FirstHalf)
    }

    pub fn end_first_half(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::HalfTime)
    }

    pub fn start_second_half(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::SecondHalf)
    }

    pub fn finish(&mut self) -> Result<(), MatchTransitionError> {
        self.transition_to(MatchState::Finished)
    }

    /// Applies only the match-level state invariant. Authorization, evidence,
    /// canonical verification events, and persistence remain outside this
    /// domain value transition until the quality workflow increments.
    pub fn transition_verification_to(
        &mut self,
        next: MatchVerificationStatus,
    ) -> Result<(), MatchVerificationTransitionError> {
        self.verification_status = self.verification_status.transition_to(next)?;
        Ok(())
    }

    fn transition_to(&mut self, next: MatchState) -> Result<(), MatchTransitionError> {
        self.lifecycle.transition_to(next)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::{
        CompetitionId, Fixture, FixtureId, Season, SeasonId, SeasonYear, TeamId, UtcInstant,
    };

    use super::{
        AggregateVersion, MatchAggregate, MatchId, MatchIdentity, MatchState, MatchTransitionError,
        MatchVerificationStatus,
    };

    fn aggregate() -> MatchAggregate {
        let competition_id = CompetitionId::from_uuid(
            Uuid::parse_str("019fe280-3bc5-7cf0-b3d2-0728c077a930")
                .expect("aggregate UUID should be valid"),
        );
        let season = Season::new(
            SeasonId::from_uuid(
                Uuid::parse_str("019fe280-3bc5-7cf1-b3d2-0728c077a930")
                    .expect("aggregate UUID should be valid"),
            ),
            competition_id,
            SeasonYear::try_new(2026).expect("2026 should be valid"),
        );
        let fixture = Fixture::try_new(
            FixtureId::from_uuid(
                Uuid::parse_str("019fe280-3bc5-7cf2-b3d2-0728c077a930")
                    .expect("aggregate UUID should be valid"),
            ),
            season,
            TeamId::from_uuid(
                Uuid::parse_str("019fe280-3bc5-7cf3-b3d2-0728c077a930")
                    .expect("aggregate UUID should be valid"),
            ),
            TeamId::from_uuid(
                Uuid::parse_str("019fe280-3bc5-7cf4-b3d2-0728c077a930")
                    .expect("aggregate UUID should be valid"),
            ),
            UtcInstant::from_unix_timestamp(1_777_000_000)
                .expect("aggregate timestamp should be representable"),
        )
        .expect("distinct teams should create a fixture");
        let identity = MatchIdentity::from_fixture(
            MatchId::from_uuid(
                Uuid::parse_str("019fe280-3bc5-7cf5-b3d2-0728c077a930")
                    .expect("aggregate UUID should be valid"),
            ),
            &fixture,
        );

        MatchAggregate::new(identity)
    }

    #[test]
    fn aggregate_owns_identity_and_starts_in_initial_state() {
        let aggregate = aggregate();

        assert_eq!(aggregate.id(), aggregate.identity().id());
        assert_eq!(aggregate.fixture_id(), aggregate.identity().fixture_id());
        assert_eq!(aggregate.state(), MatchState::initial());
        assert_eq!(
            aggregate.verification_status(),
            MatchVerificationStatus::Provisional
        );
        assert_eq!(aggregate.version(), AggregateVersion::initial());
    }

    #[test]
    fn aggregate_keeps_verification_transition_separate_from_lifecycle() {
        let mut aggregate = aggregate();

        aggregate
            .transition_verification_to(MatchVerificationStatus::Verified)
            .expect("provisional match should become verified");
        assert_eq!(
            aggregate.verification_status(),
            MatchVerificationStatus::Verified
        );
        assert_eq!(aggregate.state(), MatchState::initial());

        aggregate
            .transition_verification_to(MatchVerificationStatus::Provisional)
            .expect("verified match should be reopenable to provisional state");
        assert_eq!(
            aggregate.verification_status(),
            MatchVerificationStatus::Provisional
        );
        assert_eq!(aggregate.state(), MatchState::initial());
    }

    #[test]
    fn aggregate_identity_remains_the_fixture_link() {
        let aggregate = aggregate();

        assert_eq!(aggregate.fixture_id(), aggregate.identity().fixture_id());
    }

    #[test]
    fn aggregate_runs_through_pre_match_to_confirmed_lineups() {
        let mut aggregate = aggregate();

        aggregate.schedule().unwrap();
        aggregate.enter_pre_match().unwrap();
        aggregate.submit_lineups().unwrap();
        aggregate.confirm_lineups().unwrap();

        assert_eq!(aggregate.state(), MatchState::LineupsConfirmed);
        assert_eq!(aggregate.version(), AggregateVersion::initial());
    }

    #[test]
    fn aggregate_rejects_confirming_lineups_before_submission() {
        let mut aggregate = aggregate();
        aggregate.schedule().unwrap();
        aggregate.enter_pre_match().unwrap();

        assert_eq!(
            aggregate.confirm_lineups().unwrap_err(),
            MatchTransitionError::InvalidTransition {
                from: MatchState::PreMatch,
                to: MatchState::LineupsConfirmed,
            }
        );
    }

    #[test]
    fn aggregate_runs_from_confirmed_lineups_to_finished() {
        let mut aggregate = aggregate();
        aggregate.schedule().unwrap();
        aggregate.enter_pre_match().unwrap();
        aggregate.submit_lineups().unwrap();
        aggregate.confirm_lineups().unwrap();

        aggregate.start_first_half().unwrap();
        aggregate.end_first_half().unwrap();
        aggregate.start_second_half().unwrap();
        aggregate.finish().unwrap();

        assert_eq!(aggregate.state(), MatchState::Finished);
        assert_eq!(aggregate.version(), AggregateVersion::initial());
    }

    #[test]
    fn aggregate_rejects_finish_before_second_half() {
        let mut aggregate = aggregate();
        aggregate.schedule().unwrap();
        aggregate.enter_pre_match().unwrap();
        aggregate.submit_lineups().unwrap();
        aggregate.confirm_lineups().unwrap();
        aggregate.start_first_half().unwrap();

        assert_eq!(
            aggregate.finish().unwrap_err(),
            MatchTransitionError::InvalidTransition {
                from: MatchState::FirstHalf,
                to: MatchState::Finished,
            }
        );
    }
}
