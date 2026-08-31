use crate::synchronous_match_state::SynchronousMatchStateError;
use football_domain::{
    FixtureId, LifecycleProjection, LineupSubmittedPayload, MatchId, MatchState, TeamId, UtcInstant,
};
use sqlx::{Postgres, Transaction};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_lineup_state(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
    fixture_id: FixtureId,
    home_team_id: TeamId,
    away_team_id: TeamId,
    lifecycle: &LifecycleProjection,
    source_version: i64,
    projection_version: i64,
    computed_at_utc: UtcInstant,
) -> Result<(), SynchronousMatchStateError> {
    validate_lineup_teams(match_id, home_team_id, away_team_id, lifecycle)?;
    sqlx::query(
        "INSERT INTO match_lineup_projections \
         (match_id, fixture_id, source_aggregate_version, projection_version, computed_at_utc, is_current) \
         VALUES ($1, $2, $3, $4, $5, TRUE) \
         ON CONFLICT (match_id) DO UPDATE SET \
           source_aggregate_version = EXCLUDED.source_aggregate_version, \
           projection_version = EXCLUDED.projection_version, computed_at_utc = EXCLUDED.computed_at_utc, \
           is_rebuilding = FALSE, is_current = TRUE, last_error_code = NULL, \
           updated_at = CURRENT_TIMESTAMP",
    )
    .bind(match_id.as_uuid())
    .bind(fixture_id.as_uuid())
    .bind(source_version)
    .bind(projection_version)
    .bind(computed_at_utc.as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(SynchronousMatchStateError::Database)?;

    if !lifecycle.has_lifecycle_events() {
        return Ok(());
    }

    sqlx::query("DELETE FROM match_lineup_team_projections WHERE match_id = $1")
        .bind(match_id.as_uuid())
        .execute(&mut **transaction)
        .await
        .map_err(SynchronousMatchStateError::Database)?;

    for lineup in lifecycle.submitted_lineups() {
        let side = team_side(lineup, home_team_id, away_team_id)
            .expect("lineup team identities are validated before publication");
        sqlx::query(
            "INSERT INTO match_lineup_team_projections (match_id, side, lineup_status) \
             VALUES ($1, $2, $3)",
        )
        .bind(match_id.as_uuid())
        .bind(side)
        .bind(lineup_status(lifecycle.state()))
        .execute(&mut **transaction)
        .await
        .map_err(SynchronousMatchStateError::Database)?;

        for entry in lineup.entries() {
            sqlx::query(
                "INSERT INTO match_lineup_entry_projections (match_id, side, player_id, role) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(match_id.as_uuid())
            .bind(side)
            .bind(entry.player_id().as_uuid())
            .bind(entry.role().as_str())
            .execute(&mut **transaction)
            .await
            .map_err(SynchronousMatchStateError::Database)?;
        }
    }
    Ok(())
}

fn validate_lineup_teams(
    match_id: MatchId,
    home_team_id: TeamId,
    away_team_id: TeamId,
    lifecycle: &LifecycleProjection,
) -> Result<(), SynchronousMatchStateError> {
    for lineup in lifecycle.submitted_lineups() {
        if team_side(lineup, home_team_id, away_team_id).is_none() {
            return Err(SynchronousMatchStateError::LineupTeamNotInFixture {
                match_id,
                team_id: lineup.team_id(),
            });
        }
    }
    Ok(())
}

fn team_side(
    lineup: &LineupSubmittedPayload,
    home_team_id: TeamId,
    away_team_id: TeamId,
) -> Option<&'static str> {
    if lineup.team_id() == home_team_id {
        Some("home")
    } else if lineup.team_id() == away_team_id {
        Some("away")
    } else {
        None
    }
}

fn lineup_status(state: MatchState) -> &'static str {
    match state {
        MatchState::LineupsConfirmed
        | MatchState::FirstHalf
        | MatchState::HalfTime
        | MatchState::SecondHalf
        | MatchState::Interrupted
        | MatchState::Finished => "confirmed",
        MatchState::Draft
        | MatchState::Scheduled
        | MatchState::PreMatch
        | MatchState::LineupsProvisional
        | MatchState::Postponed
        | MatchState::Cancelled => "provisional",
    }
}

#[cfg(test)]
mod tests {
    use super::lineup_status;
    use football_domain::MatchState;

    #[test]
    fn lineups_become_confirmed_for_every_later_regular_state() {
        assert_eq!(lineup_status(MatchState::LineupsProvisional), "provisional");
        for state in [
            MatchState::LineupsConfirmed,
            MatchState::FirstHalf,
            MatchState::HalfTime,
            MatchState::SecondHalf,
            MatchState::Finished,
        ] {
            assert_eq!(lineup_status(state), "confirmed");
        }
    }
}
