use crate::{Fixture, FixtureId};
use std::fmt;
use uuid::Uuid;

/// Canonical identity of a live match aggregate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchId(Uuid);

impl MatchId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for MatchId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<MatchId> for Uuid {
    fn from(value: MatchId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for MatchId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The identity link between a fixture and the future match aggregate.
///
/// Lifecycle state, aggregate version, clock, lineups, events, and replay
/// behavior are intentionally introduced by later match-engine increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchIdentity {
    id: MatchId,
    fixture_id: FixtureId,
}

impl MatchIdentity {
    pub const fn from_fixture(id: MatchId, fixture: &Fixture) -> Self {
        Self {
            id,
            fixture_id: fixture.id(),
        }
    }

    pub const fn id(self) -> MatchId {
        self.id
    }

    pub const fn fixture_id(self) -> FixtureId {
        self.fixture_id
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::{
        CompetitionId, Fixture, FixtureId, Season, SeasonId, SeasonYear, TeamId, UtcInstant,
    };

    use super::{MatchId, MatchIdentity};

    fn fixture() -> Fixture {
        let competition_id = CompetitionId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        );
        let season = Season::new(
            SeasonId::from_uuid(
                Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                    .expect("fixture UUID should be valid"),
            ),
            competition_id,
            SeasonYear::try_new(2026).expect("2026 should be valid"),
        );
        Fixture::try_new(
            FixtureId::from_uuid(
                Uuid::parse_str("019fe250-3bc5-7cf2-b3d2-0728c077a930")
                    .expect("fixture UUID should be valid"),
            ),
            season,
            TeamId::from_uuid(
                Uuid::parse_str("019fe250-3bc5-7cf3-b3d2-0728c077a930")
                    .expect("fixture UUID should be valid"),
            ),
            TeamId::from_uuid(
                Uuid::parse_str("019fe250-3bc5-7cf4-b3d2-0728c077a930")
                    .expect("fixture UUID should be valid"),
            ),
            UtcInstant::from_unix_timestamp(1_777_000_000)
                .expect("fixture timestamp should be representable"),
        )
        .expect("distinct teams should create a fixture")
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf5-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn creates_match_identity_from_one_fixture() {
        let fixture = fixture();
        let identity = MatchIdentity::from_fixture(match_id(), &fixture);

        assert_eq!(identity.id(), match_id());
        assert_eq!(identity.fixture_id(), fixture.id());
    }

    #[test]
    fn match_id_round_trips_as_a_canonical_uuid() {
        assert_eq!(Uuid::from(match_id()), match_id().as_uuid());
    }
}
