use football_domain::{FixtureId, MatchId, TeamId, UtcInstant};
use sqlx::{PgPool, query, query_as, query_scalar};
use uuid::Uuid;

pub async fn seed_assigned_fixture(
    pool: &PgPool,
    operator_id: Uuid,
    match_id: MatchId,
    fixture_id: FixtureId,
    home_team_id: TeamId,
    away_team_id: TeamId,
) {
    let competition_id = Uuid::parse_str("019fe951-3bc5-7cf0-b3d2-0728c077a930").unwrap();
    let season_id = Uuid::parse_str("019fe951-3bc5-7cf1-b3d2-0728c077a930").unwrap();
    query("INSERT INTO competitions (id, name) VALUES ($1, 'Application C1 Competition')")
        .bind(competition_id)
        .execute(pool)
        .await
        .expect("fixture competition should insert");
    query("INSERT INTO seasons (id, competition_id, year) VALUES ($1, $2, 2026)")
        .bind(season_id)
        .bind(competition_id)
        .execute(pool)
        .await
        .expect("fixture season should insert");
    query("INSERT INTO teams (id, name) VALUES ($1, 'Application C1 Home'), ($2, 'Application C1 Away')")
        .bind(home_team_id.as_uuid())
        .bind(away_team_id.as_uuid())
        .execute(pool)
        .await
        .expect("fixture teams should insert");
    query(
        "INSERT INTO fixtures \
         (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(fixture_id.as_uuid())
    .bind(competition_id)
    .bind(season_id)
    .bind(home_team_id.as_uuid())
    .bind(away_team_id.as_uuid())
    .bind(
        UtcInstant::from_unix_timestamp(1_900)
            .expect("fixture kickoff should be valid")
            .as_offset_datetime(),
    )
    .execute(pool)
    .await
    .expect("fixture should insert");
    query("INSERT INTO match_assignments (match_id, fixture_id, operator_id) VALUES ($1, $2, $3)")
        .bind(match_id.as_uuid())
        .bind(fixture_id.as_uuid())
        .bind(operator_id)
        .execute(pool)
        .await
        .expect("match assignment should insert");
}

pub async fn assert_first_goal_projection(pool: &PgPool, match_id: MatchId) {
    let summary: (i64, i64, String, String, i64, i64, bool) = query_as(
        "SELECT home_goals, away_goals, match_state, verification_status, \
         source_aggregate_version, projection_version, is_current \
         FROM match_summary_projections WHERE match_id = $1",
    )
    .bind(match_id.as_uuid())
    .fetch_one(pool)
    .await
    .expect("synchronous summary should be readable");
    assert_eq!(
        summary,
        (
            1,
            0,
            "first_half".to_owned(),
            "provisional".to_owned(),
            1,
            1,
            true,
        )
    );

    let head: (i64, i64, bool) = query_as(
        "SELECT source_aggregate_version, projection_version, is_current \
         FROM match_timeline_projection_heads WHERE match_id = $1",
    )
    .bind(match_id.as_uuid())
    .fetch_one(pool)
    .await
    .expect("synchronous timeline head should be readable");
    assert_eq!(head, (1, 1, true));

    let timeline_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_timeline_projections WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .fetch_one(pool)
            .await
            .expect("synchronous timeline should be readable");
    assert_eq!(timeline_count, 1);
}
