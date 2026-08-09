use crate::{
    AggregateVersion, Fixture, FixtureId, MatchId, MatchIdentity, MatchVerificationStatus,
    SeasonId, TeamId,
};
use std::{cmp::Ordering, fmt};

/// One final match result accepted as an input to the standings reducer.
///
/// This value is intentionally narrower than a match read projection. It is
/// created only after the caller has replayed the canonical match stream,
/// resolved corrections, and proved the match-level status is `verified`.
/// Operator observations and provisional summary rows are not valid sources
/// for this contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VerifiedMatchResult {
    season_id: SeasonId,
    match_id: MatchId,
    fixture_id: FixtureId,
    home_team_id: TeamId,
    away_team_id: TeamId,
    home_goals: u32,
    away_goals: u32,
    source_aggregate_version: AggregateVersion,
}

impl VerifiedMatchResult {
    /// Builds a verified result from one match identity and its fixture.
    ///
    /// The score values must be the result of canonical event replay. This
    /// constructor enforces the boundaries that the standings reducer needs:
    /// the match must be verified, the match identity must point to the same
    /// fixture, and the source stream must contain at least one canonical
    /// event (the verification marker itself advances the version).
    pub fn try_new(
        identity: MatchIdentity,
        fixture: Fixture,
        verification_status: MatchVerificationStatus,
        home_goals: u32,
        away_goals: u32,
        source_aggregate_version: AggregateVersion,
    ) -> Result<Self, VerifiedMatchResultError> {
        if !verification_status.is_verified() {
            return Err(VerifiedMatchResultError::MatchNotVerified {
                status: verification_status,
            });
        }

        if identity.fixture_id() != fixture.id() {
            return Err(VerifiedMatchResultError::MatchIdentityFixtureMismatch {
                match_id: identity.id(),
                expected_fixture_id: identity.fixture_id(),
                actual_fixture_id: fixture.id(),
            });
        }

        if source_aggregate_version.value() == 0 {
            return Err(
                VerifiedMatchResultError::SourceAggregateVersionMustBePositive {
                    value: source_aggregate_version.value(),
                },
            );
        }

        Ok(Self {
            season_id: fixture.season_id(),
            match_id: identity.id(),
            fixture_id: fixture.id(),
            home_team_id: fixture.home_team_id(),
            away_team_id: fixture.away_team_id(),
            home_goals,
            away_goals,
            source_aggregate_version,
        })
    }

    pub const fn season_id(self) -> SeasonId {
        self.season_id
    }

    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    pub const fn fixture_id(self) -> FixtureId {
        self.fixture_id
    }

    pub const fn home_team_id(self) -> TeamId {
        self.home_team_id
    }

    pub const fn away_team_id(self) -> TeamId {
        self.away_team_id
    }

    pub const fn home_goals(self) -> u32 {
        self.home_goals
    }

    pub const fn away_goals(self) -> u32 {
        self.away_goals
    }

    pub const fn source_aggregate_version(self) -> AggregateVersion {
        self.source_aggregate_version
    }

    pub const fn outcome(self) -> MatchOutcome {
        MatchOutcome::from_scores(self.home_goals, self.away_goals)
    }

    pub const fn reduce_played_won_drawn_lost(self) -> MatchOutcomeReduction {
        reduce_played_won_drawn_lost(self)
    }

    pub const fn reduce_goals_and_points(
        self,
        points_rule: PointsRule,
    ) -> MatchGoalsPointsReduction {
        reduce_goals_and_points(self, points_rule)
    }
}

/// The result of comparing the verified home and away scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchOutcome {
    HomeWin,
    Draw,
    AwayWin,
}

impl MatchOutcome {
    pub const fn from_scores(home_goals: u32, away_goals: u32) -> Self {
        if home_goals > away_goals {
            Self::HomeWin
        } else if home_goals < away_goals {
            Self::AwayWin
        } else {
            Self::Draw
        }
    }
}

/// One team's contribution from one verified match.
///
/// These are deltas, not a season aggregate. Every accepted result therefore
/// contributes exactly one `played` and exactly one of `won`, `drawn`, or
/// `lost`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TeamOutcomeDelta {
    team_id: TeamId,
    played: u32,
    won: u32,
    drawn: u32,
    lost: u32,
}

impl TeamOutcomeDelta {
    const fn new(team_id: TeamId, played: u32, won: u32, drawn: u32, lost: u32) -> Self {
        Self {
            team_id,
            played,
            won,
            drawn,
            lost,
        }
    }

    pub const fn team_id(self) -> TeamId {
        self.team_id
    }

    pub const fn played(self) -> u32 {
        self.played
    }

    pub const fn won(self) -> u32 {
        self.won
    }

    pub const fn drawn(self) -> u32 {
        self.drawn
    }

    pub const fn lost(self) -> u32 {
        self.lost
    }
}

/// One verified match's played/won/drawn/lost reduction for both teams.
///
/// The source identity is kept with the deltas so a later standings worker
/// can associate the contribution with one season and one canonical result.
/// This increment does not aggregate multiple matches or deduplicate a
/// correction; those are separate revision/rebuild responsibilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchOutcomeReduction {
    season_id: SeasonId,
    match_id: MatchId,
    fixture_id: FixtureId,
    source_aggregate_version: AggregateVersion,
    outcome: MatchOutcome,
    home: TeamOutcomeDelta,
    away: TeamOutcomeDelta,
}

impl MatchOutcomeReduction {
    pub const fn season_id(self) -> SeasonId {
        self.season_id
    }

    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    pub const fn fixture_id(self) -> FixtureId {
        self.fixture_id
    }

    pub const fn source_aggregate_version(self) -> AggregateVersion {
        self.source_aggregate_version
    }

    pub const fn outcome(self) -> MatchOutcome {
        self.outcome
    }

    pub const fn home(self) -> TeamOutcomeDelta {
        self.home
    }

    pub const fn away(self) -> TeamOutcomeDelta {
        self.away
    }
}

/// Explicit points awarded for one match outcome.
///
/// The values are supplied by the season's rules boundary. This small value
/// object makes STD-003 deterministic without silently inventing a default;
/// versioned and persisted competition rules belong to RUL-001 onward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PointsRule {
    win_points: u32,
    draw_points: u32,
    loss_points: u32,
}

impl PointsRule {
    pub const fn new(win_points: u32, draw_points: u32, loss_points: u32) -> Self {
        Self {
            win_points,
            draw_points,
            loss_points,
        }
    }

    pub const fn win_points(self) -> u32 {
        self.win_points
    }

    pub const fn draw_points(self) -> u32 {
        self.draw_points
    }

    pub const fn loss_points(self) -> u32 {
        self.loss_points
    }
}

/// One team's goals and points contribution from one verified match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TeamGoalsPointsDelta {
    team_id: TeamId,
    goals_for: u32,
    goals_against: u32,
    points: u32,
}

impl TeamGoalsPointsDelta {
    const fn new(team_id: TeamId, goals_for: u32, goals_against: u32, points: u32) -> Self {
        Self {
            team_id,
            goals_for,
            goals_against,
            points,
        }
    }

    pub const fn team_id(self) -> TeamId {
        self.team_id
    }

    pub const fn goals_for(self) -> u32 {
        self.goals_for
    }

    pub const fn goals_against(self) -> u32 {
        self.goals_against
    }

    pub const fn points(self) -> u32 {
        self.points
    }
}

/// One verified match's goals-for/goals-against/points reduction for both
/// teams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchGoalsPointsReduction {
    season_id: SeasonId,
    match_id: MatchId,
    fixture_id: FixtureId,
    source_aggregate_version: AggregateVersion,
    home: TeamGoalsPointsDelta,
    away: TeamGoalsPointsDelta,
}

impl MatchGoalsPointsReduction {
    pub const fn season_id(self) -> SeasonId {
        self.season_id
    }

    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    pub const fn fixture_id(self) -> FixtureId {
        self.fixture_id
    }

    pub const fn source_aggregate_version(self) -> AggregateVersion {
        self.source_aggregate_version
    }

    pub const fn home(self) -> TeamGoalsPointsDelta {
        self.home
    }

    pub const fn away(self) -> TeamGoalsPointsDelta {
        self.away
    }
}

/// The aggregate standings values that can participate in a tiebreak
/// comparison.
///
/// This is a comparison input, not a persisted standings projection. The
/// source revision and projection freshness metadata remain outside this
/// pure ordering operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StandingComparisonRow {
    team_id: TeamId,
    played: u32,
    won: u32,
    drawn: u32,
    lost: u32,
    goals_for: u32,
    goals_against: u32,
    points: u32,
}

impl StandingComparisonRow {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        team_id: TeamId,
        played: u32,
        won: u32,
        drawn: u32,
        lost: u32,
        goals_for: u32,
        goals_against: u32,
        points: u32,
    ) -> Self {
        Self {
            team_id,
            played,
            won,
            drawn,
            lost,
            goals_for,
            goals_against,
            points,
        }
    }

    pub const fn team_id(self) -> TeamId {
        self.team_id
    }

    pub const fn played(self) -> u32 {
        self.played
    }

    pub const fn won(self) -> u32 {
        self.won
    }

    pub const fn drawn(self) -> u32 {
        self.drawn
    }

    pub const fn lost(self) -> u32 {
        self.lost
    }

    pub const fn goals_for(self) -> u32 {
        self.goals_for
    }

    pub const fn goals_against(self) -> u32 {
        self.goals_against
    }

    pub const fn goal_difference(self) -> i64 {
        self.goals_for as i64 - self.goals_against as i64
    }

    pub const fn points(self) -> u32 {
        self.points
    }
}

/// A metric that can be placed in an ordered tiebreak policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TiebreakMetric {
    Points,
    GoalDifference,
    GoalsFor,
    GoalsAgainst,
    Won,
    Drawn,
    Lost,
    Played,
}

/// Direction in which one tiebreak metric ranks teams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TiebreakDirection {
    Ascending,
    Descending,
}

/// One explicit metric/direction entry in an ordered tiebreak policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TiebreakCriterion {
    metric: TiebreakMetric,
    direction: TiebreakDirection,
}

impl TiebreakCriterion {
    pub const fn new(metric: TiebreakMetric, direction: TiebreakDirection) -> Self {
        Self { metric, direction }
    }

    pub const fn metric(self) -> TiebreakMetric {
        self.metric
    }

    pub const fn direction(self) -> TiebreakDirection {
        self.direction
    }
}

/// An ordered, duplicate-free tiebreak policy supplied by a rules boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiebreakPolicy {
    criteria: Vec<TiebreakCriterion>,
}

impl TiebreakPolicy {
    pub fn try_new(
        criteria: impl IntoIterator<Item = TiebreakCriterion>,
    ) -> Result<Self, TiebreakPolicyError> {
        let mut ordered = Vec::new();
        for criterion in criteria {
            if ordered
                .iter()
                .any(|existing: &TiebreakCriterion| existing.metric() == criterion.metric())
            {
                return Err(TiebreakPolicyError::DuplicateMetric {
                    metric: criterion.metric(),
                });
            }
            ordered.push(criterion);
        }

        if ordered.is_empty() {
            return Err(TiebreakPolicyError::Empty);
        }

        Ok(Self { criteria: ordered })
    }

    pub fn criteria(&self) -> &[TiebreakCriterion] {
        &self.criteria
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TiebreakPolicyError {
    Empty,
    DuplicateMetric { metric: TiebreakMetric },
}

impl fmt::Display for TiebreakPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => {
                formatter.write_str("tiebreak policy must contain at least one criterion")
            }
            Self::DuplicateMetric { metric } => {
                write!(formatter, "tiebreak policy repeats metric {metric:?}")
            }
        }
    }
}

impl std::error::Error for TiebreakPolicyError {}

/// Compares two standing rows for ranking under an explicit policy.
///
/// `Ordering::Less` means `left` ranks before `right`, which makes the result
/// directly usable with `slice::sort_by`. If every configured criterion is
/// equal, the result is `Ordering::Equal`; team identity is not a hidden
/// football tiebreaker.
pub fn compare_standing_rows(
    left: &StandingComparisonRow,
    right: &StandingComparisonRow,
    policy: &TiebreakPolicy,
) -> Ordering {
    for criterion in policy.criteria() {
        let natural_ordering = match criterion.metric() {
            TiebreakMetric::Points => left.points().cmp(&right.points()),
            TiebreakMetric::GoalDifference => left.goal_difference().cmp(&right.goal_difference()),
            TiebreakMetric::GoalsFor => left.goals_for().cmp(&right.goals_for()),
            TiebreakMetric::GoalsAgainst => left.goals_against().cmp(&right.goals_against()),
            TiebreakMetric::Won => left.won().cmp(&right.won()),
            TiebreakMetric::Drawn => left.drawn().cmp(&right.drawn()),
            TiebreakMetric::Lost => left.lost().cmp(&right.lost()),
            TiebreakMetric::Played => left.played().cmp(&right.played()),
        };

        let ranking_ordering = match criterion.direction() {
            TiebreakDirection::Ascending => natural_ordering,
            TiebreakDirection::Descending => natural_ordering.reverse(),
        };

        if ranking_ordering != Ordering::Equal {
            return ranking_ordering;
        }
    }

    Ordering::Equal
}

/// Reduces one verified result into goals-for, goals-against, and points
/// deltas using an explicitly supplied outcome-points rule.
pub const fn reduce_goals_and_points(
    result: VerifiedMatchResult,
    points_rule: PointsRule,
) -> MatchGoalsPointsReduction {
    let (home_points, away_points) = match result.outcome() {
        MatchOutcome::HomeWin => (points_rule.win_points(), points_rule.loss_points()),
        MatchOutcome::Draw => (points_rule.draw_points(), points_rule.draw_points()),
        MatchOutcome::AwayWin => (points_rule.loss_points(), points_rule.win_points()),
    };

    MatchGoalsPointsReduction {
        season_id: result.season_id(),
        match_id: result.match_id(),
        fixture_id: result.fixture_id(),
        source_aggregate_version: result.source_aggregate_version(),
        home: TeamGoalsPointsDelta::new(
            result.home_team_id(),
            result.home_goals(),
            result.away_goals(),
            home_points,
        ),
        away: TeamGoalsPointsDelta::new(
            result.away_team_id(),
            result.away_goals(),
            result.home_goals(),
            away_points,
        ),
    }
}

/// Reduces one verified result into the two team outcome deltas required by
/// the first standings slice.
pub const fn reduce_played_won_drawn_lost(result: VerifiedMatchResult) -> MatchOutcomeReduction {
    let outcome = result.outcome();
    let (home, away) = match outcome {
        MatchOutcome::HomeWin => (
            TeamOutcomeDelta::new(result.home_team_id(), 1, 1, 0, 0),
            TeamOutcomeDelta::new(result.away_team_id(), 1, 0, 0, 1),
        ),
        MatchOutcome::Draw => (
            TeamOutcomeDelta::new(result.home_team_id(), 1, 0, 1, 0),
            TeamOutcomeDelta::new(result.away_team_id(), 1, 0, 1, 0),
        ),
        MatchOutcome::AwayWin => (
            TeamOutcomeDelta::new(result.home_team_id(), 1, 0, 0, 1),
            TeamOutcomeDelta::new(result.away_team_id(), 1, 1, 0, 0),
        ),
    };

    MatchOutcomeReduction {
        season_id: result.season_id(),
        match_id: result.match_id(),
        fixture_id: result.fixture_id(),
        source_aggregate_version: result.source_aggregate_version(),
        outcome,
        home,
        away,
    }
}

/// Rejection raised when a value cannot be admitted as a verified standings
/// input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedMatchResultError {
    MatchNotVerified {
        status: MatchVerificationStatus,
    },
    MatchIdentityFixtureMismatch {
        match_id: MatchId,
        expected_fixture_id: FixtureId,
        actual_fixture_id: FixtureId,
    },
    SourceAggregateVersionMustBePositive {
        value: u64,
    },
}

impl fmt::Display for VerifiedMatchResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatchNotVerified { status } => {
                write!(formatter, "match status {status} cannot enter standings")
            }
            Self::MatchIdentityFixtureMismatch {
                match_id,
                expected_fixture_id,
                actual_fixture_id,
            } => write!(
                formatter,
                "match {match_id} points to fixture {expected_fixture_id}, not {actual_fixture_id}"
            ),
            Self::SourceAggregateVersionMustBePositive { value } => write!(
                formatter,
                "verified standings result requires a positive source aggregate version, got {value}"
            ),
        }
    }
}

impl std::error::Error for VerifiedMatchResultError {}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use uuid::Uuid;

    use crate::{
        AggregateVersion, CompetitionId, Fixture, FixtureId, MatchId, MatchIdentity, Season,
        SeasonId, SeasonYear, TeamId, UtcInstant,
    };

    use super::{
        MatchOutcome, MatchVerificationStatus, PointsRule, StandingComparisonRow,
        TeamGoalsPointsDelta, TeamOutcomeDelta, TiebreakCriterion, TiebreakDirection,
        TiebreakMetric, TiebreakPolicy, TiebreakPolicyError, VerifiedMatchResult,
        VerifiedMatchResultError, compare_standing_rows, reduce_goals_and_points,
        reduce_played_won_drawn_lost,
    };

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("standings fixture UUID should be valid")
    }

    fn competition_id() -> CompetitionId {
        CompetitionId::from_uuid(uuid("019ff300-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn season_id() -> SeasonId {
        SeasonId::from_uuid(uuid("019ff301-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn fixture_id() -> FixtureId {
        FixtureId::from_uuid(uuid("019ff302-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019ff303-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn home_team_id() -> TeamId {
        TeamId::from_uuid(uuid("019ff304-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn away_team_id() -> TeamId {
        TeamId::from_uuid(uuid("019ff305-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn fixture(id: FixtureId) -> Fixture {
        Fixture::try_new(
            id,
            Season::new(
                season_id(),
                competition_id(),
                SeasonYear::try_new(2026).expect("2026 should be valid"),
            ),
            home_team_id(),
            away_team_id(),
            UtcInstant::from_unix_timestamp(1_777_000_000)
                .expect("standings fixture timestamp should be valid"),
        )
        .expect("distinct teams should create a fixture")
    }

    fn identity(fixture: &Fixture) -> MatchIdentity {
        MatchIdentity::from_fixture(match_id(), fixture)
    }

    fn verified_result(home_goals: u32, away_goals: u32) -> VerifiedMatchResult {
        let fixture = fixture(fixture_id());
        VerifiedMatchResult::try_new(
            identity(&fixture),
            fixture,
            MatchVerificationStatus::Verified,
            home_goals,
            away_goals,
            AggregateVersion::from_value(7),
        )
        .expect("verified canonical result should be accepted")
    }

    fn assert_delta(
        delta: TeamOutcomeDelta,
        team_id: TeamId,
        played: u32,
        won: u32,
        drawn: u32,
        lost: u32,
    ) {
        assert_eq!(delta.team_id(), team_id);
        assert_eq!(delta.played(), played);
        assert_eq!(delta.won(), won);
        assert_eq!(delta.drawn(), drawn);
        assert_eq!(delta.lost(), lost);
        assert_eq!(won + drawn + lost, played);
    }

    fn assert_goals_points_delta(
        delta: TeamGoalsPointsDelta,
        team_id: TeamId,
        goals_for: u32,
        goals_against: u32,
        points: u32,
    ) {
        assert_eq!(delta.team_id(), team_id);
        assert_eq!(delta.goals_for(), goals_for);
        assert_eq!(delta.goals_against(), goals_against);
        assert_eq!(delta.points(), points);
    }

    fn comparison_row(
        team_id: TeamId,
        points: u32,
        goals_for: u32,
        goals_against: u32,
    ) -> StandingComparisonRow {
        StandingComparisonRow::new(team_id, 10, 5, 2, 3, goals_for, goals_against, points)
    }

    #[test]
    fn preserves_the_minimal_verified_result_contract() {
        let fixture = fixture(fixture_id());
        let result = VerifiedMatchResult::try_new(
            identity(&fixture),
            fixture,
            MatchVerificationStatus::Verified,
            2,
            1,
            AggregateVersion::from_value(7),
        )
        .expect("verified canonical result should be accepted");

        assert_eq!(result.season_id(), season_id());
        assert_eq!(result.match_id(), match_id());
        assert_eq!(result.fixture_id(), fixture_id());
        assert_eq!(result.home_team_id(), home_team_id());
        assert_eq!(result.away_team_id(), away_team_id());
        assert_eq!(result.home_goals(), 2);
        assert_eq!(result.away_goals(), 1);
        assert_eq!(result.source_aggregate_version().value(), 7);
    }

    #[test]
    fn rejects_provisional_operator_state_before_standings_reduction() {
        let fixture = fixture(fixture_id());

        assert_eq!(
            VerifiedMatchResult::try_new(
                identity(&fixture),
                fixture,
                MatchVerificationStatus::Provisional,
                2,
                1,
                AggregateVersion::from_value(7),
            ),
            Err(VerifiedMatchResultError::MatchNotVerified {
                status: MatchVerificationStatus::Provisional,
            })
        );
    }

    #[test]
    fn rejects_an_unpublished_zero_source_version() {
        let fixture = fixture(fixture_id());

        assert_eq!(
            VerifiedMatchResult::try_new(
                identity(&fixture),
                fixture,
                MatchVerificationStatus::Verified,
                0,
                0,
                AggregateVersion::initial(),
            ),
            Err(VerifiedMatchResultError::SourceAggregateVersionMustBePositive { value: 0 })
        );
    }

    #[test]
    fn rejects_a_match_identity_linked_to_another_fixture() {
        let first_fixture = fixture(fixture_id());
        let second_fixture = fixture(FixtureId::from_uuid(uuid(
            "019ff306-3bc5-7cf0-b3d2-0728c077a930",
        )));

        assert_eq!(
            VerifiedMatchResult::try_new(
                identity(&first_fixture),
                second_fixture,
                MatchVerificationStatus::Verified,
                0,
                0,
                AggregateVersion::from_value(1),
            ),
            Err(VerifiedMatchResultError::MatchIdentityFixtureMismatch {
                match_id: match_id(),
                expected_fixture_id: fixture_id(),
                actual_fixture_id: second_fixture.id(),
            })
        );
    }

    #[test]
    fn accepts_a_verified_goalless_result_without_inventing_an_outcome() {
        let fixture = fixture(fixture_id());
        let result = VerifiedMatchResult::try_new(
            identity(&fixture),
            fixture,
            MatchVerificationStatus::Verified,
            0,
            0,
            AggregateVersion::from_value(1),
        )
        .expect("verified 0-0 result should be accepted");

        assert_eq!(result.home_goals(), 0);
        assert_eq!(result.away_goals(), 0);
    }

    #[test]
    fn reduces_a_home_win_to_one_played_win_and_loss() {
        let result = verified_result(2, 1);
        let reduction = reduce_played_won_drawn_lost(result);

        assert_eq!(result.outcome(), MatchOutcome::HomeWin);
        assert_eq!(reduction.outcome(), MatchOutcome::HomeWin);
        assert_eq!(reduction.season_id(), season_id());
        assert_eq!(reduction.match_id(), match_id());
        assert_eq!(reduction.fixture_id(), fixture_id());
        assert_eq!(reduction.source_aggregate_version().value(), 7);
        assert_delta(reduction.home(), home_team_id(), 1, 1, 0, 0);
        assert_delta(reduction.away(), away_team_id(), 1, 0, 0, 1);
    }

    #[test]
    fn reduces_an_away_win_to_one_played_loss_and_win() {
        let reduction = verified_result(0, 3).reduce_played_won_drawn_lost();

        assert_eq!(reduction.outcome(), MatchOutcome::AwayWin);
        assert_delta(reduction.home(), home_team_id(), 1, 0, 0, 1);
        assert_delta(reduction.away(), away_team_id(), 1, 1, 0, 0);
    }

    #[test]
    fn reduces_a_draw_to_one_played_draw_for_both_teams() {
        let reduction = verified_result(0, 0).reduce_played_won_drawn_lost();

        assert_eq!(reduction.outcome(), MatchOutcome::Draw);
        assert_delta(reduction.home(), home_team_id(), 1, 0, 1, 0);
        assert_delta(reduction.away(), away_team_id(), 1, 0, 1, 0);
    }

    #[test]
    fn reduces_home_win_goals_and_explicit_three_one_zero_points() {
        let result = verified_result(2, 1);
        let reduction = reduce_goals_and_points(result, PointsRule::new(3, 1, 0));

        assert_eq!(reduction.season_id(), season_id());
        assert_eq!(reduction.match_id(), match_id());
        assert_eq!(reduction.fixture_id(), fixture_id());
        assert_eq!(reduction.source_aggregate_version().value(), 7);
        assert_goals_points_delta(reduction.home(), home_team_id(), 2, 1, 3);
        assert_goals_points_delta(reduction.away(), away_team_id(), 1, 2, 0);
    }

    #[test]
    fn reduces_a_draw_to_equal_goals_and_draw_points() {
        let reduction = verified_result(0, 0).reduce_goals_and_points(PointsRule::new(3, 1, 0));

        assert_goals_points_delta(reduction.home(), home_team_id(), 0, 0, 1);
        assert_goals_points_delta(reduction.away(), away_team_id(), 0, 0, 1);
    }

    #[test]
    fn applies_the_explicit_points_rule_without_hard_coding_three_one_zero() {
        let reduction = verified_result(1, 4).reduce_goals_and_points(PointsRule::new(4, 2, 1));

        assert_goals_points_delta(reduction.home(), home_team_id(), 1, 4, 1);
        assert_goals_points_delta(reduction.away(), away_team_id(), 4, 1, 4);
    }

    #[test]
    fn compares_by_the_first_different_metric_in_policy_order() {
        let policy = TiebreakPolicy::try_new([
            TiebreakCriterion::new(TiebreakMetric::Points, TiebreakDirection::Descending),
            TiebreakCriterion::new(
                TiebreakMetric::GoalDifference,
                TiebreakDirection::Descending,
            ),
            TiebreakCriterion::new(TiebreakMetric::GoalsFor, TiebreakDirection::Descending),
        ])
        .expect("ordered tiebreak policy should be valid");
        let left = comparison_row(home_team_id(), 10, 12, 4);
        let right = comparison_row(away_team_id(), 10, 10, 5);

        assert_eq!(
            compare_standing_rows(&left, &right, &policy),
            Ordering::Less
        );
        assert_eq!(
            compare_standing_rows(&right, &left, &policy),
            Ordering::Greater
        );
    }

    #[test]
    fn changing_policy_order_changes_the_selected_tiebreak_metric() {
        let points_first = TiebreakPolicy::try_new([
            TiebreakCriterion::new(TiebreakMetric::Points, TiebreakDirection::Descending),
            TiebreakCriterion::new(TiebreakMetric::GoalsFor, TiebreakDirection::Descending),
        ])
        .expect("points-first policy should be valid");
        let goals_first = TiebreakPolicy::try_new([
            TiebreakCriterion::new(TiebreakMetric::GoalsFor, TiebreakDirection::Descending),
            TiebreakCriterion::new(TiebreakMetric::Points, TiebreakDirection::Descending),
        ])
        .expect("goals-first policy should be valid");
        let left = comparison_row(home_team_id(), 9, 20, 20);
        let right = comparison_row(away_team_id(), 10, 1, 1);

        assert_eq!(
            compare_standing_rows(&left, &right, &points_first),
            Ordering::Greater
        );
        assert_eq!(
            compare_standing_rows(&left, &right, &goals_first),
            Ordering::Less
        );
    }

    #[test]
    fn respects_ascending_direction_for_a_metric() {
        let policy = TiebreakPolicy::try_new([TiebreakCriterion::new(
            TiebreakMetric::GoalsAgainst,
            TiebreakDirection::Ascending,
        )])
        .expect("ascending tiebreak policy should be valid");
        let left = comparison_row(home_team_id(), 10, 8, 2);
        let right = comparison_row(away_team_id(), 10, 8, 4);

        assert_eq!(
            compare_standing_rows(&left, &right, &policy),
            Ordering::Less
        );
    }

    #[test]
    fn leaves_equal_rows_equal_without_using_team_identity_as_a_fallback() {
        let policy = TiebreakPolicy::try_new([
            TiebreakCriterion::new(TiebreakMetric::Points, TiebreakDirection::Descending),
            TiebreakCriterion::new(
                TiebreakMetric::GoalDifference,
                TiebreakDirection::Descending,
            ),
        ])
        .expect("tiebreak policy should be valid");
        let left = comparison_row(home_team_id(), 10, 8, 2);
        let right = comparison_row(away_team_id(), 10, 8, 2);

        assert_eq!(
            compare_standing_rows(&left, &right, &policy),
            Ordering::Equal
        );
    }

    #[test]
    fn rejects_empty_and_duplicate_tiebreak_policies() {
        assert_eq!(TiebreakPolicy::try_new([]), Err(TiebreakPolicyError::Empty));

        assert_eq!(
            TiebreakPolicy::try_new([
                TiebreakCriterion::new(TiebreakMetric::Points, TiebreakDirection::Descending),
                TiebreakCriterion::new(TiebreakMetric::Points, TiebreakDirection::Ascending),
            ]),
            Err(TiebreakPolicyError::DuplicateMetric {
                metric: TiebreakMetric::Points,
            })
        );
    }
}
