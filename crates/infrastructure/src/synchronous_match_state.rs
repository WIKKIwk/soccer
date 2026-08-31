use crate::synchronous_lineup_state::publish_lineup_state;
use crate::synchronous_projection_store::{
    SUMMARY_TABLE, as_i64, lock_projection_generation, publish_summary, publish_timeline_head,
    replace_timeline,
};
use football_domain::{
    CanonicalEventStream, EffectiveEventStream, EffectiveEventStreamError, FixtureId,
    LifecycleProjection, LifecycleProjectionError, MatchEventPayload, MatchId, MatchPeriod,
    MatchProjection, MatchProjectionError, MatchState, MatchVerificationStatus, TeamId, UtcInstant,
    VerificationEventPayload,
};
use sqlx::{Error as SqlxError, Postgres, Transaction};
use std::fmt;
use uuid::Uuid;

/// Critical state committed with one canonical match command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SynchronousMatchState {
    pub match_id: MatchId,
    pub fixture_id: FixtureId,
    pub source_aggregate_version: u64,
    pub projection_version: u64,
    pub match_state: MatchState,
    pub home_goals: u32,
    pub away_goals: u32,
    pub timeline_entries: usize,
    pub verification_status: MatchVerificationStatus,
}

#[derive(Debug)]
pub enum SynchronousMatchStateError {
    Database(SqlxError),
    AssignmentNotFound {
        match_id: MatchId,
    },
    ExistingProjectionFixtureMismatch {
        match_id: MatchId,
        table: &'static str,
        expected: FixtureId,
        actual: FixtureId,
    },
    ProjectionVersionMismatch {
        match_id: MatchId,
        expected_table: &'static str,
        expected: i64,
        actual_table: &'static str,
        actual: i64,
    },
    InvalidStoredProjectionVersion {
        table: &'static str,
        value: i64,
    },
    ProjectionVersionOverflow {
        match_id: MatchId,
    },
    NumericValueOutOfRange {
        field: &'static str,
        value: u64,
    },
    EmptyAcceptedTimeline {
        match_id: MatchId,
    },
    UnsupportedTimelinePeriod {
        match_id: MatchId,
        period: MatchPeriod,
    },
    LineupTeamNotInFixture {
        match_id: MatchId,
        team_id: TeamId,
    },
    EffectiveStream(EffectiveEventStreamError),
    Lifecycle(LifecycleProjectionError),
    Projection(MatchProjectionError),
}

impl fmt::Display for SynchronousMatchStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "synchronous state database failure: {error}")
            }
            Self::AssignmentNotFound { match_id } => {
                write!(formatter, "match {match_id} has no operator assignment")
            }
            Self::ExistingProjectionFixtureMismatch {
                match_id,
                table,
                expected,
                actual,
            } => write!(
                formatter,
                "match {match_id} {table} points to fixture {actual}, expected {expected}"
            ),
            Self::ProjectionVersionMismatch {
                match_id,
                expected_table,
                expected,
                actual_table,
                actual,
            } => write!(
                formatter,
                "match {match_id} projection versions differ: {expected_table} {expected}, {actual_table} {actual}"
            ),
            Self::InvalidStoredProjectionVersion { table, value } => {
                write!(formatter, "{table} has invalid projection version {value}")
            }
            Self::ProjectionVersionOverflow { match_id } => {
                write!(formatter, "match {match_id} projection version overflowed")
            }
            Self::NumericValueOutOfRange { field, value } => {
                write!(
                    formatter,
                    "{field} value {value} is outside PostgreSQL BIGINT range"
                )
            }
            Self::EmptyAcceptedTimeline { match_id } => {
                write!(
                    formatter,
                    "accepted legacy C1 stream for match {match_id} is empty"
                )
            }
            Self::UnsupportedTimelinePeriod { match_id, period } => write!(
                formatter,
                "accepted legacy C1 stream for match {match_id} ended in {period:?}"
            ),
            Self::LineupTeamNotInFixture { match_id, team_id } => {
                write!(
                    formatter,
                    "team {team_id} is not a side of match {match_id}"
                )
            }
            Self::EffectiveStream(error) => write!(formatter, "effective stream failed: {error}"),
            Self::Lifecycle(error) => write!(formatter, "lifecycle reduction failed: {error}"),
            Self::Projection(error) => write!(formatter, "score reduction failed: {error}"),
        }
    }
}

impl std::error::Error for SynchronousMatchStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::EffectiveStream(error) => Some(error),
            Self::Lifecycle(error) => Some(error),
            Self::Projection(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AssignedFixtureRow {
    fixture_id: Uuid,
    home_team_id: Uuid,
    away_team_id: Uuid,
}

/// Replays and publishes lifecycle, lineup, score, and timeline state inside
/// the caller-owned command transaction. This function never commits.
pub async fn persist_synchronous_match_state(
    transaction: &mut Transaction<'_, Postgres>,
    stream: &CanonicalEventStream<MatchEventPayload>,
    computed_at_utc: UtcInstant,
) -> Result<SynchronousMatchState, SynchronousMatchStateError> {
    let match_id = stream.match_id();
    let fixture = load_assigned_fixture(transaction, match_id).await?;
    let fixture_id = FixtureId::from_uuid(fixture.fixture_id);
    let home_team_id = TeamId::from_uuid(fixture.home_team_id);
    let away_team_id = TeamId::from_uuid(fixture.away_team_id);
    let effective = EffectiveEventStream::try_from_canonical(stream)
        .map_err(SynchronousMatchStateError::EffectiveStream)?;
    let lifecycle = LifecycleProjection::from_effective_stream(&effective)
        .map_err(SynchronousMatchStateError::Lifecycle)?;
    let projection = MatchProjection::from_effective_stream(&effective, home_team_id, away_team_id)
        .map_err(SynchronousMatchStateError::Projection)?;
    let match_state = if lifecycle.has_lifecycle_events() {
        lifecycle.state()
    } else {
        projected_legacy_c1_state(&projection)?
    };
    let verification_status = verification_status(&effective);
    let projection_version = lock_projection_generation(transaction, match_id, fixture_id).await?;
    let source_version = as_i64(
        "source_aggregate_version",
        projection.current_aggregate_version().value(),
    )?;

    replace_timeline(transaction, match_id, &projection).await?;
    publish_summary(
        transaction,
        match_id,
        fixture_id,
        &projection,
        match_state,
        verification_status,
        source_version,
        projection_version,
        computed_at_utc,
    )
    .await?;
    publish_timeline_head(
        transaction,
        match_id,
        source_version,
        projection_version,
        computed_at_utc,
    )
    .await?;
    publish_lineup_state(
        transaction,
        match_id,
        fixture_id,
        home_team_id,
        away_team_id,
        &lifecycle,
        source_version,
        projection_version,
        computed_at_utc,
    )
    .await?;

    Ok(SynchronousMatchState {
        match_id,
        fixture_id,
        source_aggregate_version: projection.current_aggregate_version().value(),
        projection_version: u64::try_from(projection_version).map_err(|_| {
            SynchronousMatchStateError::InvalidStoredProjectionVersion {
                table: SUMMARY_TABLE,
                value: projection_version,
            }
        })?,
        match_state,
        home_goals: projection.score().home_goals(),
        away_goals: projection.score().away_goals(),
        timeline_entries: projection.timeline().len(),
        verification_status,
    })
}

async fn load_assigned_fixture(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<AssignedFixtureRow, SynchronousMatchStateError> {
    sqlx::query_as(
        "SELECT assignment.fixture_id, fixture.home_team_id, fixture.away_team_id \
         FROM match_assignments AS assignment \
         JOIN fixtures AS fixture ON fixture.id = assignment.fixture_id \
         WHERE assignment.match_id = $1 FOR UPDATE OF assignment, fixture",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?
    .ok_or(SynchronousMatchStateError::AssignmentNotFound { match_id })
}

fn projected_legacy_c1_state(
    projection: &MatchProjection,
) -> Result<MatchState, SynchronousMatchStateError> {
    let period = projection
        .timeline()
        .last()
        .ok_or(SynchronousMatchStateError::EmptyAcceptedTimeline {
            match_id: projection.match_id(),
        })?
        .period();
    match period {
        MatchPeriod::FirstHalf => Ok(MatchState::FirstHalf),
        MatchPeriod::SecondHalf => Ok(MatchState::SecondHalf),
        MatchPeriod::PreMatch | MatchPeriod::HalfTime => {
            Err(SynchronousMatchStateError::UnsupportedTimelinePeriod {
                match_id: projection.match_id(),
                period,
            })
        }
    }
}

fn verification_status(
    stream: &EffectiveEventStream<MatchEventPayload>,
) -> MatchVerificationStatus {
    stream
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(_)) => {
                Some(MatchVerificationStatus::Verified)
            }
            MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(_)) => {
                Some(MatchVerificationStatus::Provisional)
            }
            MatchEventPayload::Lifecycle(_)
            | MatchEventPayload::C1(_)
            | MatchEventPayload::Correction(_) => None,
        })
        .unwrap_or_else(MatchVerificationStatus::initial)
}
