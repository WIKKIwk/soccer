use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, run_migrations,
};
use sqlx::{AssertSqlSafe, PgPool, query, query_as, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("projection version fixture UUID should be valid")
}

async fn seed_fixtures(pool: &PgPool) -> (Uuid, Uuid) {
    let competition_id = uuid("019ff900-3bc5-7cf0-b3d2-0728c077a930");
    let season_id = uuid("019ff901-3bc5-7cf0-b3d2-0728c077a930");
    let home_team_id = uuid("019ff902-3bc5-7cf0-b3d2-0728c077a930");
    let away_team_id = uuid("019ff903-3bc5-7cf0-b3d2-0728c077a930");
    let summary_fixture_id = uuid("019ff904-3bc5-7cf0-b3d2-0728c077a930");
    let lineup_fixture_id = uuid("019ff905-3bc5-7cf0-b3d2-0728c077a930");

    query("INSERT INTO competitions (id, name) VALUES ($1, $2)")
        .bind(competition_id)
        .bind("O‘zbekiston Superligasi")
        .execute(pool)
        .await
        .expect("projection version competition should insert");
    query("INSERT INTO seasons (id, competition_id, year) VALUES ($1, $2, $3)")
        .bind(season_id)
        .bind(competition_id)
        .bind(2026_i32)
        .execute(pool)
        .await
        .expect("projection version season should insert");
    for (team_id, name) in [
        (home_team_id, "Projection Version Home"),
        (away_team_id, "Projection Version Away"),
    ] {
        query("INSERT INTO teams (id, name) VALUES ($1, $2)")
            .bind(team_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("projection version team should insert");
    }
    query(
        "INSERT INTO fixtures (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         VALUES ($1, $2, $3, $4, $5, '2026-08-09T12:00:00Z'), \
                ($6, $2, $3, $4, $5, '2026-08-10T12:00:00Z')",
    )
    .bind(summary_fixture_id)
    .bind(competition_id)
    .bind(season_id)
    .bind(home_team_id)
    .bind(away_team_id)
    .bind(lineup_fixture_id)
    .execute(pool)
    .await
    .expect("projection version fixtures should insert");

    (summary_fixture_id, lineup_fixture_id)
}

async fn projection_metadata(
    pool: &PgPool,
    table: &str,
    match_id: Uuid,
) -> (
    i64,
    i64,
    Option<time::OffsetDateTime>,
    bool,
    bool,
    Option<String>,
) {
    query_as(AssertSqlSafe(format!(
        "SELECT source_aggregate_version, projection_version, computed_at_utc, \
                is_rebuilding, is_current, last_error_code \
         FROM {table} WHERE match_id = $1"
    )))
    .bind(match_id)
    .fetch_one(pool)
    .await
    .expect("projection metadata should be readable")
}

async fn assert_sqlstate<F>(operation: F, expected_code: &str)
where
    F: std::future::Future<Output = Result<sqlx::postgres::PgQueryResult, sqlx::Error>>,
{
    let error = operation
        .await
        .expect_err("projection metadata invariant should reject the write");
    let code = match error {
        sqlx::Error::Database(database_error) => database_error.code().map(|code| code.to_string()),
        other => panic!("expected a PostgreSQL constraint error, got {other:?}"),
    };
    assert_eq!(code.as_deref(), Some(expected_code));
}

async fn assert_metadata_constraints(pool: &PgPool, table: &str, match_id: Uuid) {
    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET source_aggregate_version = -1 WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET projection_version = -1 WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET projection_version = 0 WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET computed_at_utc = NULL, is_current = TRUE, \
             is_rebuilding = FALSE, last_error_code = NULL WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET is_rebuilding = TRUE, is_current = TRUE WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET is_current = TRUE, last_error_code = 'projection.failed' \
             WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(AssertSqlSafe(format!(
            "UPDATE {table} SET last_error_code = '  ' WHERE match_id = $1"
        )))
        .bind(match_id)
        .execute(pool),
        "23514",
    )
    .await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn projection_versions_store_shared_freshness_metadata_for_all_match_read_models() {
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
        .expect("clean projection version target should accept a PostgreSQL connection");
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "projection version target must be clean"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply before projection version assertions");

    let (summary_fixture_id, lineup_fixture_id) = seed_fixtures(&pool).await;
    let summary_match_id = uuid("019ff906-3bc5-7cf0-b3d2-0728c077a930");
    let lineup_match_id = uuid("019ff907-3bc5-7cf0-b3d2-0728c077a930");
    let timeline_match_id = uuid("019ff908-3bc5-7cf0-b3d2-0728c077a930");

    query("INSERT INTO match_summary_projections (match_id, fixture_id) VALUES ($1, $2)")
        .bind(summary_match_id)
        .bind(summary_fixture_id)
        .execute(&pool)
        .await
        .expect("summary projection with version metadata should insert");
    query("INSERT INTO match_lineup_projections (match_id, fixture_id) VALUES ($1, $2)")
        .bind(lineup_match_id)
        .bind(lineup_fixture_id)
        .execute(&pool)
        .await
        .expect("lineup projection with version metadata should insert");
    query("INSERT INTO match_timeline_projection_heads (match_id) VALUES ($1)")
        .bind(timeline_match_id)
        .execute(&pool)
        .await
        .expect("timeline projection head with version metadata should insert");

    for (table, match_id) in [
        ("match_summary_projections", summary_match_id),
        ("match_lineup_projections", lineup_match_id),
        ("match_timeline_projection_heads", timeline_match_id),
    ] {
        let metadata = projection_metadata(&pool, table, match_id).await;
        assert_eq!(metadata.0, 0, "{table} should start at source version zero");
        assert_eq!(
            metadata.1, 0,
            "{table} should start at unpublished generation zero"
        );
        assert!(
            metadata.2.is_none(),
            "{table} should have no compute time before publication"
        );
        assert!(!metadata.3, "{table} should not be rebuilding on insertion");
        assert!(
            !metadata.4,
            "{table} must not claim current before publication"
        );
        assert!(
            metadata.5.is_none(),
            "{table} should have no error on insertion"
        );
    }

    for (table, match_id, source_version, projection_version) in [
        ("match_summary_projections", summary_match_id, 7_i64, 3_i64),
        ("match_lineup_projections", lineup_match_id, 4_i64, 2_i64),
        (
            "match_timeline_projection_heads",
            timeline_match_id,
            9_i64,
            5_i64,
        ),
    ] {
        query(AssertSqlSafe(format!(
            "UPDATE {table} \
             SET source_aggregate_version = $1, projection_version = $2, \
                 computed_at_utc = '2026-08-09T12:30:00Z', \
                 is_rebuilding = FALSE, is_current = TRUE, last_error_code = NULL \
             WHERE match_id = $3"
        )))
        .bind(source_version)
        .bind(projection_version)
        .bind(match_id)
        .execute(&pool)
        .await
        .expect("published projection metadata should update");

        let metadata = projection_metadata(&pool, table, match_id).await;
        assert_eq!(metadata.0, source_version);
        assert_eq!(metadata.1, projection_version);
        assert!(metadata.2.is_some(), "{table} should record compute time");
        assert!(!metadata.3);
        assert!(metadata.4);
        assert!(metadata.5.is_none());
    }

    query(
        "UPDATE match_summary_projections \
         SET is_rebuilding = TRUE, is_current = FALSE, last_error_code = NULL \
         WHERE match_id = $1",
    )
    .bind(summary_match_id)
    .execute(&pool)
    .await
    .expect("projection rebuild state should be representable");
    let rebuilding_metadata =
        projection_metadata(&pool, "match_summary_projections", summary_match_id).await;
    assert!(rebuilding_metadata.3);
    assert!(!rebuilding_metadata.4);

    query(
        "UPDATE match_summary_projections \
         SET is_rebuilding = FALSE, is_current = FALSE, last_error_code = 'projection.failed' \
         WHERE match_id = $1",
    )
    .bind(summary_match_id)
    .execute(&pool)
    .await
    .expect("projection error state should be representable");
    let failed_metadata =
        projection_metadata(&pool, "match_summary_projections", summary_match_id).await;
    assert_eq!(failed_metadata.5.as_deref(), Some("projection.failed"));
    assert!(!failed_metadata.4);

    query(
        "UPDATE match_summary_projections \
         SET computed_at_utc = '2026-08-09T12:31:00Z', is_rebuilding = FALSE, \
             is_current = TRUE, last_error_code = NULL \
         WHERE match_id = $1",
    )
    .bind(summary_match_id)
    .execute(&pool)
    .await
    .expect("projection should be publishable again after an error");

    for (table, match_id) in [
        ("match_summary_projections", summary_match_id),
        ("match_lineup_projections", lineup_match_id),
        ("match_timeline_projection_heads", timeline_match_id),
    ] {
        assert_metadata_constraints(&pool, table, match_id).await;
    }

    let duplicate_head_match =
        query("INSERT INTO match_timeline_projection_heads (match_id) VALUES ($1)")
            .bind(timeline_match_id)
            .execute(&pool);
    assert_sqlstate(duplicate_head_match, "23505").await;

    pool.close().await;
}
