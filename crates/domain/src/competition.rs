use std::fmt;
use uuid::Uuid;

/// Canonical identity of a competition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompetitionId(Uuid);

impl CompetitionId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for CompetitionId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<CompetitionId> for Uuid {
    fn from(value: CompetitionId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for CompetitionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Source-backed competition name for the minimal competition registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetitionName(String);

impl CompetitionName {
    pub fn try_new(value: impl Into<String>) -> Result<Self, CompetitionNameError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CompetitionNameError::Empty);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CompetitionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompetitionNameError {
    Empty,
}

impl fmt::Display for CompetitionNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("competition name must not be empty"),
        }
    }
}

impl std::error::Error for CompetitionNameError {}

/// Minimal competition identity.
///
/// Competition type, status, rules, and localized names are later concerns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Competition {
    id: CompetitionId,
    name: CompetitionName,
}

impl Competition {
    pub const fn new(id: CompetitionId, name: CompetitionName) -> Self {
        Self { id, name }
    }

    pub const fn id(&self) -> CompetitionId {
        self.id
    }

    pub fn name(&self) -> &CompetitionName {
        &self.name
    }
}

/// Canonical identity of a competition season.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeasonId(Uuid);

impl SeasonId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for SeasonId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<SeasonId> for Uuid {
    fn from(value: SeasonId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for SeasonId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Minimal season label for the current four-digit season contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeasonYear(u16);

impl SeasonYear {
    pub fn try_new(value: u16) -> Result<Self, SeasonYearError> {
        if value == 0 {
            return Err(SeasonYearError::Zero);
        }

        Ok(Self(value))
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

impl fmt::Display for SeasonYear {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonYearError {
    Zero,
}

impl fmt::Display for SeasonYearError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("season year must be positive"),
        }
    }
}

impl std::error::Error for SeasonYearError {}

/// Monotonic source revision of a season's accepted verified results.
///
/// A season starts at zero. Each committed final or corrected verified result
/// advances this value once; standings projections later record which source
/// revision they have computed. The value is deliberately separate from a
/// projection generation or a match aggregate version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeasonResultsRevision(u64);

impl SeasonResultsRevision {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn from_value(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn try_next(self) -> Result<Self, SeasonResultsRevisionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(SeasonResultsRevisionError::Overflow { current: self.0 })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonResultsRevisionError {
    Overflow { current: u64 },
}

impl fmt::Display for SeasonResultsRevisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow { current } => {
                write!(
                    formatter,
                    "season results revision overflow after {current}"
                )
            }
        }
    }
}

impl std::error::Error for SeasonResultsRevisionError {}

/// Minimal season identity owned by one competition.
///
/// Date intervals, status, stages, and versioned rules are later season
/// concerns and are not inferred from the year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Season {
    id: SeasonId,
    competition_id: CompetitionId,
    year: SeasonYear,
    results_revision: SeasonResultsRevision,
}

impl Season {
    pub const fn new(id: SeasonId, competition_id: CompetitionId, year: SeasonYear) -> Self {
        Self {
            id,
            competition_id,
            year,
            results_revision: SeasonResultsRevision::initial(),
        }
    }

    pub const fn with_results_revision(mut self, results_revision: SeasonResultsRevision) -> Self {
        self.results_revision = results_revision;
        self
    }

    pub const fn id(self) -> SeasonId {
        self.id
    }

    pub const fn competition_id(self) -> CompetitionId {
        self.competition_id
    }

    pub const fn year(self) -> SeasonYear {
        self.year
    }

    pub const fn results_revision(self) -> SeasonResultsRevision {
        self.results_revision
    }

    pub fn try_advance_results_revision(
        &mut self,
    ) -> Result<SeasonResultsRevision, SeasonResultsRevisionError> {
        let next = self.results_revision.try_next()?;
        self.results_revision = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        Competition, CompetitionId, CompetitionName, CompetitionNameError, Season, SeasonId,
        SeasonResultsRevision, SeasonResultsRevisionError, SeasonYear, SeasonYearError,
    };

    fn competition_id() -> CompetitionId {
        CompetitionId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    fn season_id() -> SeasonId {
        SeasonId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn competition_preserves_typed_identity_and_source_name() {
        let name = CompetitionName::try_new(" O‘zbekiston Superligasi ")
            .expect("non-empty competition name");
        let competition = Competition::new(competition_id(), name.clone());

        assert_eq!(competition.id(), competition_id());
        assert_eq!(competition.name(), &name);
        assert_eq!(competition.name().as_str(), " O‘zbekiston Superligasi ");
    }

    #[test]
    fn competition_and_season_ids_round_trip_as_canonical_uuids() {
        assert_eq!(Uuid::from(competition_id()), competition_id().as_uuid());
        assert_eq!(Uuid::from(season_id()), season_id().as_uuid());
    }

    #[test]
    fn competition_name_rejects_only_whitespace() {
        assert_eq!(
            CompetitionName::try_new(" \t\n"),
            Err(CompetitionNameError::Empty)
        );
    }

    #[test]
    fn season_keeps_competition_ownership_and_explicit_year() {
        let year = SeasonYear::try_new(2026).expect("2026 is a valid season year");
        let season = Season::new(season_id(), competition_id(), year);

        assert_eq!(season.id(), season_id());
        assert_eq!(season.competition_id(), competition_id());
        assert_eq!(season.year().value(), 2026);
        assert_eq!(season.year().to_string(), "2026");
        assert_eq!(season.results_revision(), SeasonResultsRevision::initial());
    }

    #[test]
    fn season_year_rejects_zero() {
        assert_eq!(SeasonYear::try_new(0), Err(SeasonYearError::Zero));
    }

    #[test]
    fn season_results_revision_advances_from_zero() {
        let mut season = Season::new(
            season_id(),
            competition_id(),
            SeasonYear::try_new(2026).expect("2026 is a valid season year"),
        );

        assert_eq!(season.results_revision().value(), 0);
        assert_eq!(
            season
                .try_advance_results_revision()
                .expect("first result revision should be representable")
                .value(),
            1
        );
        assert_eq!(season.results_revision().value(), 1);
    }

    #[test]
    fn season_results_revision_rejects_overflow_without_mutating_state() {
        let mut season = Season::new(
            season_id(),
            competition_id(),
            SeasonYear::try_new(2026).expect("2026 is a valid season year"),
        )
        .with_results_revision(SeasonResultsRevision::from_value(u64::MAX));

        assert_eq!(
            season.try_advance_results_revision(),
            Err(SeasonResultsRevisionError::Overflow { current: u64::MAX })
        );
        assert_eq!(season.results_revision().value(), u64::MAX);
    }
}
