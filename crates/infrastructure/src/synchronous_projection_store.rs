use crate::synchronous_match_state::SynchronousMatchStateError;
use football_domain::{
    FixtureId, MatchId, MatchProjection, MatchState, MatchVerificationStatus, UtcInstant,
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(crate) const SUMMARY_TABLE: &str = "match_summary_projections";
const TIMELINE_HEAD_TABLE: &str = "match_timeline_projection_heads";
const LINEUP_TABLE: &str = "match_lineup_projections";

pub(crate) async fn lock_projection_generation(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    fixture_id: FixtureId,
) -> Result<i64, SynchronousMatchStateError> {
    let summary = lock_fixture_root(
        transaction,
        match_id,
        fixture_id,
        SUMMARY_TABLE,
        "SELECT fixture_id, projection_version FROM match_summary_projections \
         WHERE match_id = $1 FOR UPDATE",
    )
    .await?;
    let timeline: Option<i64> = sqlx::query_scalar(
        "SELECT projection_version FROM match_timeline_projection_heads \
         WHERE match_id = $1 FOR UPDATE",
    )
    .bind(match_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?;
    let lineup = lock_fixture_root(
        transaction,
        match_id,
        fixture_id,
        LINEUP_TABLE,
        "SELECT fixture_id, projection_version FROM match_lineup_projections \
         WHERE match_id = $1 FOR UPDATE",
    )
    .await?;
    next_projection_version(match_id, summary, timeline, lineup)
}

async fn lock_fixture_root(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    fixture_id: FixtureId,
    table: &'static str,
    statement: &'static str,
) -> Result<Option<i64>, SynchronousMatchStateError> {
    let row: Option<(Uuid, i64)> = sqlx::query_as(statement)
        .bind(match_id.as_uuid())
        .fetch_optional(&mut **transaction)
        .await
        .map_err(SynchronousMatchStateError::Database)?;
    if let Some((stored_fixture_id, version)) = row {
        let actual = FixtureId::from_uuid(stored_fixture_id);
        if actual != fixture_id {
            return Err(
                SynchronousMatchStateError::ExistingProjectionFixtureMismatch {
                    match_id,
                    table,
                    expected: fixture_id,
                    actual,
                },
            );
        }
        Ok(Some(version))
    } else {
        Ok(None)
    }
}

pub(crate) async fn replace_timeline(
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
pub(crate) async fn publish_summary(
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

pub(crate) async fn publish_timeline_head(
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

pub(crate) fn as_i64(field: &'static str, value: u64) -> Result<i64, SynchronousMatchStateError> {
    i64::try_from(value)
        .map_err(|_| SynchronousMatchStateError::NumericValueOutOfRange { field, value })
}

fn next_projection_version(
    match_id: MatchId,
    summary: Option<i64>,
    timeline: Option<i64>,
    lineup: Option<i64>,
) -> Result<i64, SynchronousMatchStateError> {
    for (table, version) in [
        (SUMMARY_TABLE, summary),
        (TIMELINE_HEAD_TABLE, timeline),
        (LINEUP_TABLE, lineup),
    ] {
        let Some(value) = version else { continue };
        if value < 0 {
            return Err(SynchronousMatchStateError::InvalidStoredProjectionVersion {
                table,
                value,
            });
        }
    }
    if let (Some(expected), Some(actual)) = (summary, timeline)
        && expected != actual
    {
        return Err(SynchronousMatchStateError::ProjectionVersionMismatch {
            match_id,
            expected_table: SUMMARY_TABLE,
            expected,
            actual_table: TIMELINE_HEAD_TABLE,
            actual,
        });
    }
    [summary, timeline, lineup]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(SynchronousMatchStateError::ProjectionVersionOverflow { match_id })
}

#[cfg(test)]
mod tests {
    use super::next_projection_version;
    use crate::SynchronousMatchStateError;
    use football_domain::MatchId;
    use uuid::Uuid;

    fn match_id() -> MatchId {
        MatchId::from_uuid(Uuid::parse_str("019ff550-3bc5-7cf0-b3d2-0728c077a930").unwrap())
    }

    #[test]
    fn projection_generation_starts_advances_and_rejects_split_roots() {
        assert_eq!(
            next_projection_version(match_id(), None, None, None).unwrap(),
            1
        );
        assert_eq!(
            next_projection_version(match_id(), Some(7), Some(7), Some(3)).unwrap(),
            8
        );
        assert!(matches!(
            next_projection_version(match_id(), Some(2), Some(3), Some(8)),
            Err(SynchronousMatchStateError::ProjectionVersionMismatch { .. })
        ));
    }
}
