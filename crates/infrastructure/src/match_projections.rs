use crate::match_events::{
    MatchEventRepositoryError, load_canonical_match_event_stream_in_transaction,
};
use football_domain::{
    EffectiveEventStream, EffectiveEventStreamError, FixtureId, MatchEventPayload, MatchId,
    MatchProjection, MatchProjectionError, MatchVerificationStatus, TeamId, UtcInstant,
    VerificationEventPayload,
};
use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

const REBUILD_FAILURE_CODE: &str = "projection.rebuild_failed";
const SUMMARY_TABLE: &str = "match_summary_projections";
const TIMELINE_HEAD_TABLE: &str = "match_timeline_projection_heads";

/// The result of publishing one complete score/timeline projection generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuiltMatchProjection {
    match_id: MatchId,
    fixture_id: FixtureId,
    source_aggregate_version: u64,
    projection_version: u64,
    home_goals: u32,
    away_goals: u32,
    timeline_entries: usize,
    verification_status: MatchVerificationStatus,
}

impl RebuiltMatchProjection {
    pub const fn match_id(self) -> MatchId {
        self.match_id
    }

    pub const fn fixture_id(self) -> FixtureId {
        self.fixture_id
    }

    pub const fn source_aggregate_version(self) -> u64 {
        self.source_aggregate_version
    }

    pub const fn projection_version(self) -> u64 {
        self.projection_version
    }

    pub const fn home_goals(self) -> u32 {
        self.home_goals
    }

    pub const fn away_goals(self) -> u32 {
        self.away_goals
    }

    pub const fn timeline_entries(self) -> usize {
        self.timeline_entries
    }

    pub const fn verification_status(self) -> MatchVerificationStatus {
        self.verification_status
    }
}

#[derive(Debug)]
pub enum MatchProjectionRebuildError {
    Database(SqlxError),
    FixtureNotFound {
        fixture_id: FixtureId,
    },
    ExistingProjectionFixtureMismatch {
        match_id: MatchId,
        expected: FixtureId,
        actual: FixtureId,
    },
    ProjectionRootMissing {
        match_id: MatchId,
        table: &'static str,
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
    MatchEvent(MatchEventRepositoryError),
    EffectiveStream(EffectiveEventStreamError),
    Projection(MatchProjectionError),
    FailureStatePersistence {
        rebuild: Box<Self>,
        failure: Box<Self>,
    },
}

impl fmt::Display for MatchProjectionRebuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "projection database operation failed: {error}")
            }
            Self::FixtureNotFound { fixture_id } => {
                write!(formatter, "fixture {fixture_id} was not found")
            }
            Self::ExistingProjectionFixtureMismatch {
                match_id,
                expected,
                actual,
            } => write!(
                formatter,
                "match {match_id} projection points to fixture {actual}, expected {expected}"
            ),
            Self::ProjectionRootMissing { match_id, table } => {
                write!(
                    formatter,
                    "projection root {table} for match {match_id} is missing"
                )
            }
            Self::ProjectionVersionMismatch {
                match_id,
                summary_version,
                timeline_version,
            } => write!(
                formatter,
                "match {match_id} projection versions differ: summary {summary_version}, timeline {timeline_version}"
            ),
            Self::InvalidStoredProjectionVersion { table, value } => write!(
                formatter,
                "projection table {table} has invalid stored projection version {value}"
            ),
            Self::ProjectionVersionOverflow { match_id } => {
                write!(
                    formatter,
                    "match {match_id} projection version cannot advance"
                )
            }
            Self::NumericValueOutOfRange { field, value } => {
                write!(
                    formatter,
                    "{field} value {value} does not fit PostgreSQL BIGINT"
                )
            }
            Self::MatchEvent(error) => {
                write!(formatter, "canonical match event load failed: {error}")
            }
            Self::EffectiveStream(error) => {
                write!(formatter, "effective event stream is invalid: {error}")
            }
            Self::Projection(error) => {
                write!(formatter, "match projection reduction failed: {error}")
            }
            Self::FailureStatePersistence { rebuild, failure } => write!(
                formatter,
                "projection rebuild failed ({rebuild}) and its failure state could not be persisted ({failure})"
            ),
        }
    }
}

impl std::error::Error for MatchProjectionRebuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::MatchEvent(error) => Some(error),
            Self::EffectiveStream(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::FailureStatePersistence { failure, .. } => Some(failure.as_ref()),
            Self::FixtureNotFound { .. }
            | Self::ExistingProjectionFixtureMismatch { .. }
            | Self::ProjectionRootMissing { .. }
            | Self::ProjectionVersionMismatch { .. }
            | Self::InvalidStoredProjectionVersion { .. }
            | Self::ProjectionVersionOverflow { .. }
            | Self::NumericValueOutOfRange { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FixtureTeamsRow {
    home_team_id: Uuid,
    away_team_id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectionMetadataRow {
    source_aggregate_version: i64,
    projection_version: i64,
    computed_at_utc: Option<OffsetDateTime>,
}

/// Rebuilds the currently implemented match read model from canonical events.
///
/// The summary and timeline roots are first marked as rebuilding in a committed
/// transaction. A second transaction replaces all timeline rows and publishes
/// both roots atomically. If reduction or publication fails, the prior rows
/// remain intact and both roots receive a visible failure state.
pub async fn rebuild_match_projection(
    pool: &PgPool,
    match_id: MatchId,
    fixture_id: FixtureId,
    computed_at_utc: UtcInstant,
) -> Result<RebuiltMatchProjection, MatchProjectionRebuildError> {
    prepare_rebuild(pool, match_id, fixture_id).await?;

    match publish_rebuilt_projection(pool, match_id, fixture_id, computed_at_utc).await {
        Ok(result) => Ok(result),
        Err(rebuild_error) => {
            if let Err(failure) = persist_rebuild_failure(pool, match_id, fixture_id).await {
                Err(MatchProjectionRebuildError::FailureStatePersistence {
                    rebuild: Box::new(rebuild_error),
                    failure: Box::new(failure),
                })
            } else {
                Err(rebuild_error)
            }
        }
    }
}

async fn prepare_rebuild(
    pool: &PgPool,
    match_id: MatchId,
    fixture_id: FixtureId,
) -> Result<(), MatchProjectionRebuildError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(MatchProjectionRebuildError::Database)?;

    lock_fixture(&mut transaction, fixture_id).await?;

    let summary: Option<(Uuid, i64, i64, Option<OffsetDateTime>)> = sqlx::query_as(
        "SELECT fixture_id, source_aggregate_version, projection_version, computed_at_utc \
         FROM match_summary_projections WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if let Some((stored_fixture_id, _, projection_version, _)) = &summary {
        validate_fixture_identity(
            match_id,
            fixture_id,
            FixtureId::from_uuid(*stored_fixture_id),
        )?;
        validate_stored_projection_version(SUMMARY_TABLE, *projection_version)?;
    }

    let timeline_head: Option<ProjectionMetadataRow> = sqlx::query_as(
        "SELECT source_aggregate_version, projection_version, computed_at_utc \
         FROM match_timeline_projection_heads WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if let Some(metadata) = &timeline_head {
        validate_stored_projection_version(TIMELINE_HEAD_TABLE, metadata.projection_version)?;
    }

    if let (Some((_, _, summary_version, _)), Some(timeline_metadata)) = (&summary, &timeline_head)
    {
        validate_projection_versions(
            match_id,
            *summary_version,
            timeline_metadata.projection_version,
        )?;
    }

    if summary.is_none() {
        let metadata = timeline_head.as_ref();
        sqlx::query(
            "INSERT INTO match_summary_projections \
             (match_id, fixture_id, source_aggregate_version, projection_version, computed_at_utc) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(match_id.as_uuid())
        .bind(fixture_id.as_uuid())
        .bind(metadata.map_or(0, |value| value.source_aggregate_version))
        .bind(metadata.map_or(0, |value| value.projection_version))
        .bind(metadata.and_then(|value| value.computed_at_utc))
        .execute(&mut *transaction)
        .await
        .map_err(MatchProjectionRebuildError::Database)?;
    }

    if timeline_head.is_none() {
        sqlx::query(
            "INSERT INTO match_timeline_projection_heads \
             (match_id, source_aggregate_version, projection_version, computed_at_utc) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(match_id.as_uuid())
        .bind(summary.as_ref().map_or(0, |(_, source, _, _)| *source))
        .bind(summary.as_ref().map_or(0, |(_, _, version, _)| *version))
        .bind(
            summary
                .as_ref()
                .and_then(|(_, _, _, computed_at)| *computed_at),
        )
        .execute(&mut *transaction)
        .await
        .map_err(MatchProjectionRebuildError::Database)?;
    }

    let summary_updated = sqlx::query(
        "UPDATE match_summary_projections \
         SET is_rebuilding = TRUE, is_current = FALSE, last_error_code = NULL \
         WHERE match_id = $1",
    )
    .bind(match_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if summary_updated.rows_affected() != 1 {
        return Err(MatchProjectionRebuildError::ProjectionRootMissing {
            match_id,
            table: SUMMARY_TABLE,
        });
    }

    let timeline_updated = sqlx::query(
        "UPDATE match_timeline_projection_heads \
         SET is_rebuilding = TRUE, is_current = FALSE, last_error_code = NULL \
         WHERE match_id = $1",
    )
    .bind(match_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if timeline_updated.rows_affected() != 1 {
        return Err(MatchProjectionRebuildError::ProjectionRootMissing {
            match_id,
            table: TIMELINE_HEAD_TABLE,
        });
    }

    transaction
        .commit()
        .await
        .map_err(MatchProjectionRebuildError::Database)
}

async fn publish_rebuilt_projection(
    pool: &PgPool,
    match_id: MatchId,
    fixture_id: FixtureId,
    computed_at_utc: UtcInstant,
) -> Result<RebuiltMatchProjection, MatchProjectionRebuildError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(MatchProjectionRebuildError::Database)?;
    let (home_team_id, away_team_id) = lock_fixture(&mut transaction, fixture_id).await?;

    let canonical_stream =
        load_canonical_match_event_stream_in_transaction(&mut transaction, match_id)
            .await
            .map_err(MatchProjectionRebuildError::MatchEvent)?;
    let effective_stream = EffectiveEventStream::try_from_canonical(&canonical_stream)
        .map_err(MatchProjectionRebuildError::EffectiveStream)?;
    let projection =
        MatchProjection::from_effective_stream(&effective_stream, home_team_id, away_team_id)
            .map_err(MatchProjectionRebuildError::Projection)?;
    let verification_status = verification_status(&effective_stream);

    let summary = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT fixture_id, projection_version FROM match_summary_projections \
         WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?
    .ok_or(MatchProjectionRebuildError::ProjectionRootMissing {
        match_id,
        table: SUMMARY_TABLE,
    })?;
    validate_fixture_identity(match_id, fixture_id, FixtureId::from_uuid(summary.0))?;

    let timeline_version: i64 = sqlx::query_scalar(
        "SELECT projection_version FROM match_timeline_projection_heads \
         WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?
    .ok_or(MatchProjectionRebuildError::ProjectionRootMissing {
        match_id,
        table: TIMELINE_HEAD_TABLE,
    })?;
    let projection_version = next_projection_version(match_id, summary.1, timeline_version)?;
    let source_version = as_i64(
        "source_aggregate_version",
        projection.current_aggregate_version().value(),
    )?;
    let projection_source_version = projection.current_aggregate_version().value();

    sqlx::query("DELETE FROM match_timeline_projections WHERE match_id = $1")
        .bind(match_id.as_uuid())
        .execute(&mut *transaction)
        .await
        .map_err(MatchProjectionRebuildError::Database)?;

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
        .execute(&mut *transaction)
        .await
        .map_err(MatchProjectionRebuildError::Database)?;
    }

    let summary_updated = sqlx::query(
        "UPDATE match_summary_projections \
         SET home_goals = $1, away_goals = $2, verification_status = $3, \
             source_aggregate_version = $4, projection_version = $5, \
             computed_at_utc = $6, is_rebuilding = FALSE, is_current = TRUE, \
             last_error_code = NULL, updated_at = CURRENT_TIMESTAMP \
         WHERE match_id = $7",
    )
    .bind(i64::from(projection.score().home_goals()))
    .bind(i64::from(projection.score().away_goals()))
    .bind(verification_status.as_persisted_str())
    .bind(source_version)
    .bind(projection_version)
    .bind(computed_at_utc.as_offset_datetime())
    .bind(match_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if summary_updated.rows_affected() != 1 {
        return Err(MatchProjectionRebuildError::ProjectionRootMissing {
            match_id,
            table: SUMMARY_TABLE,
        });
    }

    let timeline_updated = sqlx::query(
        "UPDATE match_timeline_projection_heads \
         SET source_aggregate_version = $1, projection_version = $2, \
             computed_at_utc = $3, is_rebuilding = FALSE, is_current = TRUE, \
             last_error_code = NULL \
         WHERE match_id = $4",
    )
    .bind(source_version)
    .bind(projection_version)
    .bind(computed_at_utc.as_offset_datetime())
    .bind(match_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if timeline_updated.rows_affected() != 1 {
        return Err(MatchProjectionRebuildError::ProjectionRootMissing {
            match_id,
            table: TIMELINE_HEAD_TABLE,
        });
    }

    transaction
        .commit()
        .await
        .map_err(MatchProjectionRebuildError::Database)?;

    Ok(RebuiltMatchProjection {
        match_id,
        fixture_id,
        source_aggregate_version: projection_source_version,
        projection_version: u64::try_from(projection_version).map_err(|_| {
            MatchProjectionRebuildError::InvalidStoredProjectionVersion {
                table: SUMMARY_TABLE,
                value: projection_version,
            }
        })?,
        home_goals: projection.score().home_goals(),
        away_goals: projection.score().away_goals(),
        timeline_entries: projection.timeline().len(),
        verification_status,
    })
}

async fn persist_rebuild_failure(
    pool: &PgPool,
    match_id: MatchId,
    fixture_id: FixtureId,
) -> Result<(), MatchProjectionRebuildError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(MatchProjectionRebuildError::Database)?;
    lock_fixture(&mut transaction, fixture_id).await?;

    let summary_updated = sqlx::query(
        "UPDATE match_summary_projections \
         SET is_rebuilding = FALSE, is_current = FALSE, last_error_code = $1, \
             updated_at = CURRENT_TIMESTAMP WHERE match_id = $2",
    )
    .bind(REBUILD_FAILURE_CODE)
    .bind(match_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if summary_updated.rows_affected() != 1 {
        return Err(MatchProjectionRebuildError::ProjectionRootMissing {
            match_id,
            table: SUMMARY_TABLE,
        });
    }

    let timeline_updated = sqlx::query(
        "UPDATE match_timeline_projection_heads \
         SET is_rebuilding = FALSE, is_current = FALSE, last_error_code = $1 \
         WHERE match_id = $2",
    )
    .bind(REBUILD_FAILURE_CODE)
    .bind(match_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?;
    if timeline_updated.rows_affected() != 1 {
        return Err(MatchProjectionRebuildError::ProjectionRootMissing {
            match_id,
            table: TIMELINE_HEAD_TABLE,
        });
    }

    transaction
        .commit()
        .await
        .map_err(MatchProjectionRebuildError::Database)
}

async fn lock_fixture(
    transaction: &mut Transaction<'_, Postgres>,
    fixture_id: FixtureId,
) -> Result<(TeamId, TeamId), MatchProjectionRebuildError> {
    let row = sqlx::query_as::<_, FixtureTeamsRow>(
        "SELECT home_team_id, away_team_id FROM fixtures WHERE id = $1 FOR UPDATE",
    )
    .bind(fixture_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(MatchProjectionRebuildError::Database)?
    .ok_or(MatchProjectionRebuildError::FixtureNotFound { fixture_id })?;

    Ok((
        TeamId::from_uuid(row.home_team_id),
        TeamId::from_uuid(row.away_team_id),
    ))
}

fn validate_fixture_identity(
    match_id: MatchId,
    expected: FixtureId,
    actual: FixtureId,
) -> Result<(), MatchProjectionRebuildError> {
    if expected != actual {
        return Err(
            MatchProjectionRebuildError::ExistingProjectionFixtureMismatch {
                match_id,
                expected,
                actual,
            },
        );
    }
    Ok(())
}

fn validate_stored_projection_version(
    table: &'static str,
    value: i64,
) -> Result<(), MatchProjectionRebuildError> {
    if value < 0 {
        return Err(MatchProjectionRebuildError::InvalidStoredProjectionVersion { table, value });
    }
    Ok(())
}

fn validate_projection_versions(
    match_id: MatchId,
    summary_version: i64,
    timeline_version: i64,
) -> Result<(), MatchProjectionRebuildError> {
    if summary_version != timeline_version {
        return Err(MatchProjectionRebuildError::ProjectionVersionMismatch {
            match_id,
            summary_version,
            timeline_version,
        });
    }
    Ok(())
}

fn next_projection_version(
    match_id: MatchId,
    summary_version: i64,
    timeline_version: i64,
) -> Result<i64, MatchProjectionRebuildError> {
    validate_stored_projection_version(SUMMARY_TABLE, summary_version)?;
    validate_stored_projection_version(TIMELINE_HEAD_TABLE, timeline_version)?;
    validate_projection_versions(match_id, summary_version, timeline_version)?;
    summary_version
        .checked_add(1)
        .ok_or(MatchProjectionRebuildError::ProjectionVersionOverflow { match_id })
}

fn as_i64(field: &'static str, value: u64) -> Result<i64, MatchProjectionRebuildError> {
    i64::try_from(value)
        .map_err(|_| MatchProjectionRebuildError::NumericValueOutOfRange { field, value })
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
