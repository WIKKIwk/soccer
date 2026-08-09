use crate::{CompetitionId, Season, SeasonId, TeamId};
use std::fmt;
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;

/// Canonical identity of a scheduled fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FixtureId(Uuid);

impl FixtureId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for FixtureId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<FixtureId> for Uuid {
    fn from(value: FixtureId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for FixtureId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A canonical instant whose stored offset is always UTC.
///
/// The application boundary accepts an explicit source offset and normalizes
/// it once. A local date-time without an offset cannot be converted into this
/// type by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcInstant(OffsetDateTime);

impl UtcInstant {
    pub fn from_offset_datetime(value: OffsetDateTime) -> Self {
        Self(value.to_offset(UtcOffset::UTC))
    }

    pub fn from_unix_timestamp(seconds: i64) -> Result<Self, time::error::ComponentRange> {
        OffsetDateTime::from_unix_timestamp(seconds).map(Self::from_offset_datetime)
    }

    pub const fn as_offset_datetime(self) -> OffsetDateTime {
        self.0
    }

    pub const fn unix_timestamp(self) -> i64 {
        self.0.unix_timestamp()
    }
}

/// The minimal fixture contract for the first scheduling slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixture {
    id: FixtureId,
    competition_id: CompetitionId,
    season_id: SeasonId,
    home_team_id: TeamId,
    away_team_id: TeamId,
    kickoff: UtcInstant,
}

impl Fixture {
    pub fn try_new(
        id: FixtureId,
        season: Season,
        home_team_id: TeamId,
        away_team_id: TeamId,
        kickoff: UtcInstant,
    ) -> Result<Self, FixtureError> {
        if home_team_id == away_team_id {
            return Err(FixtureError::SameHomeAndAwayTeam);
        }

        Ok(Self {
            id,
            competition_id: season.competition_id(),
            season_id: season.id(),
            home_team_id,
            away_team_id,
            kickoff,
        })
    }

    pub const fn id(self) -> FixtureId {
        self.id
    }

    pub const fn competition_id(self) -> CompetitionId {
        self.competition_id
    }

    pub const fn season_id(self) -> SeasonId {
        self.season_id
    }

    pub const fn home_team_id(self) -> TeamId {
        self.home_team_id
    }

    pub const fn away_team_id(self) -> TeamId {
        self.away_team_id
    }

    pub fn includes_team(self, team_id: TeamId) -> bool {
        self.home_team_id == team_id || self.away_team_id == team_id
    }

    pub const fn kickoff(self) -> UtcInstant {
        self.kickoff
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureError {
    SameHomeAndAwayTeam,
}

impl fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SameHomeAndAwayTeam => {
                formatter.write_str("fixture home and away teams must be different")
            }
        }
    }
}

impl std::error::Error for FixtureError {}

#[cfg(test)]
mod tests {
    use time::{OffsetDateTime, UtcOffset};
    use uuid::Uuid;

    use crate::{CompetitionId, Season, SeasonId, SeasonYear, TeamId};

    use super::{Fixture, FixtureError, FixtureId, UtcInstant};

    fn competition_id() -> CompetitionId {
        CompetitionId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    fn season() -> Season {
        Season::new(
            SeasonId::from_uuid(
                Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                    .expect("fixture UUID should be valid"),
            ),
            competition_id(),
            SeasonYear::try_new(2026).expect("2026 should be valid"),
        )
    }

    fn team_id(value: &str) -> TeamId {
        TeamId::from_uuid(Uuid::parse_str(value).expect("fixture UUID should be valid"))
    }

    fn fixture_id() -> FixtureId {
        FixtureId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf2-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn fixture_preserves_season_ownership_home_away_and_kickoff() {
        let kickoff = UtcInstant::from_unix_timestamp(1_777_000_000)
            .expect("fixture timestamp should be representable");
        let fixture = Fixture::try_new(
            fixture_id(),
            season(),
            team_id("019fe250-3bc5-7cf3-b3d2-0728c077a930"),
            team_id("019fe250-3bc5-7cf4-b3d2-0728c077a930"),
            kickoff,
        )
        .expect("distinct teams should create a fixture");

        assert_eq!(fixture.id(), fixture_id());
        assert_eq!(fixture.competition_id(), competition_id());
        assert_eq!(fixture.season_id(), season().id());
        assert_eq!(
            fixture.home_team_id(),
            team_id("019fe250-3bc5-7cf3-b3d2-0728c077a930")
        );
        assert_eq!(
            fixture.away_team_id(),
            team_id("019fe250-3bc5-7cf4-b3d2-0728c077a930")
        );
        assert_eq!(fixture.kickoff(), kickoff);
    }

    #[test]
    fn fixture_rejects_the_same_home_and_away_team() {
        let team_id = team_id("019fe250-3bc5-7cf3-b3d2-0728c077a930");
        let kickoff = UtcInstant::from_unix_timestamp(1_777_000_000)
            .expect("fixture timestamp should be representable");

        assert_eq!(
            Fixture::try_new(fixture_id(), season(), team_id, team_id, kickoff),
            Err(FixtureError::SameHomeAndAwayTeam)
        );
    }

    #[test]
    fn utc_instant_normalizes_an_explicit_source_offset_without_host_timezone() {
        let source = OffsetDateTime::from_unix_timestamp(1_777_000_000)
            .expect("fixture timestamp should be representable")
            .to_offset(UtcOffset::from_hms(5, 0, 0).expect("UTC+05 should be valid"));
        let instant = UtcInstant::from_offset_datetime(source);

        assert_eq!(instant.unix_timestamp(), 1_777_000_000);
        assert_eq!(instant.as_offset_datetime().offset(), UtcOffset::UTC);
    }

    #[test]
    fn fixture_id_round_trips_as_a_canonical_uuid() {
        assert_eq!(Uuid::from(fixture_id()), fixture_id().as_uuid());
    }
}
