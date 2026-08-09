use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, run_migrations,
};
use sqlx::{PgPool, query, query_as, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("match-summary schema fixture UUID should be valid")
}

async fn seed_fixture(pool: &PgPool) -> (Uuid, Uuid) {
    let competition_id = uuid("019ff600-3bc5-7cf0-b3d2-0728c077a930");
    let season_id = uuid("019ff601-3bc5-7cf0-b3d2-0728c077a930");
    let home_team_id = uuid("019ff602-3bc5-7cf0-b3d2-0728c077a930");
    let away_team_id = uuid("019ff603-3bc5-7cf0-b3d2-0728c077a930");
    let fixture_id = uuid("019ff604-3bc5-7cf0-b3d2-0728c077a930");

    query("INSERT INTO competitions (id, name) VALUES ($1, $2)")
        .bind(competition_id)
        .bind("O‘zbekiston Superligasi")
        .execute(pool)
        .await
        .expect("fixture schema competition should insert");
    query("INSERT INTO seasons (id, competition_id, year) VALUES ($1, $2, $3)")
        .bind(season_id)
        .bind(competition_id)
        .bind(2026_i32)
        .execute(pool)
        .await
        .expect("fixture schema season should insert");
    for (team_id, name) in [
        (home_team_id, "Summary Home"),
        (away_team_id, "Summary Away"),
    ] {
        query("INSERT INTO teams (id, name) VALUES ($1, $2)")
            .bind(team_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("fixture schema team should insert");
    }
    query(
        "INSERT INTO fixtures (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         VALUES ($1, $2, $3, $4, $5, '2026-08-09T12:00:00Z')",
    )
    .bind(fixture_id)
    .bind(competition_id)
    .bind(season_id)
    .bind(home_team_id)
    .bind(away_team_id)
    .execute(pool)
    .await
    .expect("fixture schema fixture should insert");

    (fixture_id, uuid("019ff605-3bc5-7cf0-b3d2-0728c077a930"))
}

async fn assert_sqlstate<F>(operation: F, expected_code: &str)
where
    F: std::future::Future<Output = Result<sqlx::postgres::PgQueryResult, sqlx::Error>>,
{
    let error = operation
        .await
        .expect_err("schema invariant should reject the write");
    let code = match error {
        sqlx::Error::Database(database_error) => database_error.code().map(|code| code.to_string()),
        other => panic!("expected a PostgreSQL constraint error, got {other:?}"),
    };
    assert_eq!(code.as_deref(), Some(expected_code));
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn match_summary_projection_schema_enforces_identity_defaults_and_state_bounds() {
    let confirmation = std::env::var("ALLOW_CLEAN_DATABASE_MIGRATION")
        .expect("set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation");
    assert_eq!(
        confirmation, "1",
        "set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation"
    );

    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to an explicitly approved clean database");
    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("clean match-summary schema target should accept a PostgreSQL connection");
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "match-summary schema target must be clean"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply before schema assertions");

    let (fixture_id, second_match_id) = seed_fixture(&pool).await;
    let match_id = uuid("019ff606-3bc5-7cf0-b3d2-0728c077a930");
    query("INSERT INTO match_summary_projections (match_id, fixture_id) VALUES ($1, $2)")
        .bind(match_id)
        .bind(fixture_id)
        .execute(&pool)
        .await
        .expect("valid summary projection should insert");

    let row: (i64, i64, String, String) = query_as(
        "SELECT home_goals, away_goals, match_state, verification_status \
         FROM match_summary_projections WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .expect("summary projection defaults should be readable");
    assert_eq!(row, (0, 0, "draft".to_owned(), "provisional".to_owned()));

    let valid_second_fixture = uuid("019ff608-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO fixtures (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         SELECT $1, competition_id, season_id, home_team_id, away_team_id, kickoff_at + INTERVAL '1 day' \
         FROM fixtures WHERE id = $2",
    )
    .bind(valid_second_fixture)
    .bind(fixture_id)
    .execute(&pool)
    .await
    .expect("second fixture should insert for independent constraint checks");

    assert_sqlstate(
        query("INSERT INTO match_summary_projections (match_id, fixture_id) VALUES ($1, $2)")
            .bind(second_match_id)
            .bind(fixture_id)
            .execute(&pool),
        "23505",
    )
    .await;

    assert_sqlstate(
        query("INSERT INTO match_summary_projections (match_id, fixture_id) VALUES ($1, $2)")
            .bind(second_match_id)
            .bind(uuid("019ff607-3bc5-7cf0-b3d2-0728c077a930"))
            .execute(&pool),
        "23503",
    )
    .await;

    assert_sqlstate(
        query("INSERT INTO match_summary_projections (match_id, fixture_id, home_goals) VALUES ($1, $2, -1)")
            .bind(second_match_id)
            .bind(valid_second_fixture)
            .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query("INSERT INTO match_summary_projections (match_id, fixture_id, match_state) VALUES ($1, $2, 'unknown')")
            .bind(second_match_id)
            .bind(valid_second_fixture)
            .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query("INSERT INTO match_summary_projections (match_id, fixture_id, verification_status) VALUES ($1, $2, 'operator')")
            .bind(uuid("019ff609-3bc5-7cf0-b3d2-0728c077a930"))
            .bind(valid_second_fixture)
            .execute(&pool),
        "23514",
    )
    .await;

    pool.close().await;
}
