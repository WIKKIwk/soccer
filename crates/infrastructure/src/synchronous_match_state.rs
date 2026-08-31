use football_domain::{
    CanonicalEventStream, EffectiveEventStream, EffectiveEventStreamError, FixtureId,
    MatchEventPayload, MatchId, MatchPeriod, MatchProjection, MatchProjectionError, MatchState,
    MatchVerificationStatus, TeamId, UtcInstant, VerificationEventPayload,
};
use sqlx::{Error as SqlxError, Postgres, Transaction};
use std::fmt;
use uuid::Uuid;

const SUMMARY_TABLE: &str = "match_summary_projections";
const TIMELINE_HEAD_TABLE: &str = "match_timeline_projection_heads";

/// Critical score/timeline state committed with one canonical C1 command.
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
        expected: FixtureId,
        actual: FixtureId,
    },
    ProjectionVersionMismatch {
        match_id: MatchId,
        summary_version: i64,
        timeline_version: i64,
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
    EffectiveStream(EffectiveEventStreamError),
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
                expected,
                actual,
            } => write!(
                formatter,
                "match {match_id} projection points to fixture {actual}, expected {expected}"
            ),
            Self::ProjectionVersionMismatch {
                match_id,
                summary_version,
                timeline_version,
            } => write!(
                formatter,
                "match {match_id} projection versions differ: summary {summary_version}, timeline {timeline_version}"
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
                    "accepted C1 stream for match {match_id} has no live timeline"
                )
            }
            Self::UnsupportedTimelinePeriod { match_id, period } => write!(
                formatter,
                "accepted C1 stream for match {match_id} ended in unsupported period {period:?}"
            ),
            Self::EffectiveStream(error) => {
                write!(formatter, "synchronous effective stream failed: {error}")
            }
            Self::Projection(error) => write!(formatter, "synchronous reduction failed: {error}"),
        }
    }
}

impl std::error::Error for SynchronousMatchStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::EffectiveStream(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::AssignmentNotFound { .. }
            | Self::ExistingProjectionFixtureMismatch { .. }
            | Self::ProjectionVersionMismatch { .. }
            | Self::InvalidStoredProjectionVersion { .. }
            | Self::ProjectionVersionOverflow { .. }
            | Self::NumericValueOutOfRange { .. }
            | Self::EmptyAcceptedTimeline { .. }
            | Self::UnsupportedTimelinePeriod { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct AssignedFixtureRow {
    fixture_id: Uuid,
    home_team_id: Uuid,
    away_team_id: Uuid,
}

/// Replays and publishes the critical C1 score/timeline snapshot inside the
/// caller-owned command transaction. No commit occurs in this function.
pub async fn persist_synchronous_match_state(
    transaction: &mut Transaction<'_, Postgres>,
    stream: &CanonicalEventStream<MatchEventPayload>,
    computed_at_utc: UtcInstant,
) -> Result<SynchronousMatchState, SynchronousMatchStateError> {
    let match_id = stream.match_id();
    let fixture = load_assigned_fixture(transaction, match_id).await?;
    let fixture_id = FixtureId::from_uuid(fixture.fixture_id);
    let effective = EffectiveEventStream::try_from_canonical(stream)
        .map_err(SynchronousMatchStateError::EffectiveStream)?;
    let projection = MatchProjection::from_effective_stream(
        &effective,
        TeamId::from_uuid(fixture.home_team_id),
        TeamId::from_uuid(fixture.away_team_id),
    )
    .map_err(SynchronousMatchStateError::Projection)?;
    let match_state = projected_match_state(&projection)?;
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

async fn lock_projection_generation(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    fixture_id: FixtureId,
) -> Result<i64, SynchronousMatchStateError> {
    let summary: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT fixture_id, projection_version FROM match_summary_projections \
         WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?;
    if let Some((stored_fixture_id, _)) = summary {
        let actual = FixtureId::from_uuid(stored_fixture_id);
        if actual != fixture_id {
            return Err(
                SynchronousMatchStateError::ExistingProjectionFixtureMismatch {
                    match_id,
                    expected: fixture_id,
                    actual,
                },
            );
        }
    }

    let timeline_version: Option<i64> = sqlx::query_scalar(
        "SELECT projection_version FROM match_timeline_projection_heads \
         WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?;
    next_projection_version(
        match_id,
        summary.map(|(_, version)| version),
        timeline_version,
    )
}

async fn replace_timeline(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    projection: &MatchProjection,
) -> Result<(), SynchronousMatchStateError> {
    sqlx::query("DELETE FROM match_timeline_projections WHERE match_id = $1")
        .bind(match_id.as_uuid())
        .execute(&mut **transaction)
        .await
        .map_err(SynchronousMatchStateError::Database)?;

    for entry in projection.timeline() {
        sqlx::query(
            "INSERT INTO match_timeline_projections \
             (event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(entry.event_id().as_uuid())
        .bind(match_id.as_uuid())
        .bind(as_i64(
            "timeline.aggregate_version",
            entry.aggregate_version().value(),
        )?)
        .bind(i64::from(entry.event_index().value()))
        .bind(entry.event_type().as_str())
        .bind(entry.period().as_persisted_str())
        .bind(as_i64("timeline.clock_ms", entry.clock_ms().value())?)
        .execute(&mut **transaction)
        .await
        .map_err(SynchronousMatchStateError::Database)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn publish_summary(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    fixture_id: FixtureId,
    projection: &MatchProjection,
    match_state: MatchState,
    verification_status: MatchVerificationStatus,
    source_version: i64,
    projection_version: i64,
    computed_at_utc: UtcInstant,
) -> Result<(), SynchronousMatchStateError> {
    sqlx::query(
        "INSERT INTO match_summary_projections \
         (match_id, fixture_id, home_goals, away_goals, match_state, verification_status, \
          source_aggregate_version, projection_version, computed_at_utc, is_current) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, TRUE) \
         ON CONFLICT (match_id) DO UPDATE SET \
           home_goals = EXCLUDED.home_goals, away_goals = EXCLUDED.away_goals, \
           match_state = EXCLUDED.match_state, verification_status = EXCLUDED.verification_status, \
           source_aggregate_version = EXCLUDED.source_aggregate_version, \
           projection_version = EXCLUDED.projection_version, computed_at_utc = EXCLUDED.computed_at_utc, \
           is_rebuilding = FALSE, is_current = TRUE, last_error_code = NULL, \
           updated_at = CURRENT_TIMESTAMP",
    )
    .bind(match_id.as_uuid())
    .bind(fixture_id.as_uuid())
    .bind(i64::from(projection.score().home_goals()))
    .bind(i64::from(projection.score().away_goals()))
    .bind(match_state.as_persisted_str())
    .bind(verification_status.as_persisted_str())
    .bind(source_version)
    .bind(projection_version)
    .bind(computed_at_utc.as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?;
    Ok(())
}

async fn publish_timeline_head(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    source_version: i64,
    projection_version: i64,
    computed_at_utc: UtcInstant,
) -> Result<(), SynchronousMatchStateError> {
    sqlx::query(
        "INSERT INTO match_timeline_projection_heads \
         (match_id, source_aggregate_version, projection_version, computed_at_utc, is_current) \
         VALUES ($1, $2, $3, $4, TRUE) \
         ON CONFLICT (match_id) DO UPDATE SET \
           source_aggregate_version = EXCLUDED.source_aggregate_version, \
           projection_version = EXCLUDED.projection_version, computed_at_utc = EXCLUDED.computed_at_utc, \
           is_rebuilding = FALSE, is_current = TRUE, last_error_code = NULL",
    )
    .bind(match_id.as_uuid())
    .bind(source_version)
    .bind(projection_version)
    .bind(computed_at_utc.as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?;
    Ok(())
}

fn projected_match_state(
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

fn next_projection_version(
    match_id: MatchId,
    summary_version: Option<i64>,
    timeline_version: Option<i64>,
) -> Result<i64, SynchronousMatchStateError> {
    for (table, version) in [
        (SUMMARY_TABLE, summary_version),
        (TIMELINE_HEAD_TABLE, timeline_version),
    ] {
        if let Some(value) = version
            && value < 0
        {
            return Err(SynchronousMatchStateError::InvalidStoredProjectionVersion {
                table,
                value,
            });
        }
    }
    if let (Some(summary_version), Some(timeline_version)) = (summary_version, timeline_version)
        && summary_version != timeline_version
    {
        return Err(SynchronousMatchStateError::ProjectionVersionMismatch {
            match_id,
            summary_version,
            timeline_version,
        });
    }
    summary_version
        .or(timeline_version)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(SynchronousMatchStateError::ProjectionVersionOverflow { match_id })
}

fn as_i64(field: &'static str, value: u64) -> Result<i64, SynchronousMatchStateError> {
    i64::try_from(value)
        .map_err(|_| SynchronousMatchStateError::NumericValueOutOfRange { field, value })
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
            MatchEventPayload::C1(_) | MatchEventPayload::Correction(_) => None,
        })
        .unwrap_or_else(MatchVerificationStatus::initial)
}

#[cfg(test)]
mod tests {
    use super::{SynchronousMatchStateError, next_projection_version};
    use football_domain::MatchId;
    use uuid::Uuid;

    fn match_id() -> MatchId {
        MatchId::from_uuid(
            Uuid::parse_str("019ff550-3bc5-7cf0-b3d2-0728c077a930")
                .expect("synchronous state fixture UUID should be valid"),
        )
    }

    #[test]
    fn projection_generation_starts_at_one_and_advances_shared_roots() {
        assert_eq!(next_projection_version(match_id(), None, None).unwrap(), 1);
        assert_eq!(
            next_projection_version(match_id(), Some(7), Some(7)).unwrap(),
            8
        );
    }

    #[test]
    fn projection_generation_rejects_split_or_invalid_roots() {
        assert!(matches!(
            next_projection_version(match_id(), Some(2), Some(3)),
            Err(SynchronousMatchStateError::ProjectionVersionMismatch { .. })
        ));
        assert!(matches!(
            next_projection_version(match_id(), Some(-1), None),
            Err(SynchronousMatchStateError::InvalidStoredProjectionVersion { .. })
        ));
    }
}
