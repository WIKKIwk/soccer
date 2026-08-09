use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, run_migrations,
};
use serde_json::json;
use sqlx::{PgPool, query, query_as, query_scalar, types::Json};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("match-timeline schema fixture UUID should be valid")
}

async fn seed_canonical_events(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
    let admin_id = uuid("019ff700-3bc5-7cf0-b3d2-0728c077a930");
    let command_id = uuid("019ff701-3bc5-7cf0-b3d2-0728c077a930");
    let match_id = uuid("019ff702-3bc5-7cf0-b3d2-0728c077a930");
    let other_match_id = uuid("019ff703-3bc5-7cf0-b3d2-0728c077a930");
    let first_event_id = uuid("019ff704-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO admin_users (id, username, display_name) \
         VALUES ($1, 'timeline_schema_operator', 'Timeline Schema Operator')",
    )
    .bind(admin_id)
    .execute(pool)
    .await
    .expect("timeline schema admin should insert");
    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint) \
         VALUES ($1, $2, $3)",
    )
    .bind(command_id)
    .bind(uuid("019ff705-3bc5-7cf0-b3d2-0728c077a930"))
    .bind(vec![7_u8; 32])
    .execute(pool)
    .await
    .expect("timeline schema command should insert");

    for (event_id, event_match_id, aggregate_version, event_index, event_type, period, clock_ms) in [
        (
            first_event_id,
            match_id,
            1_i64,
            0_i64,
            "GoalRecorded",
            "first_half",
            12_000_i64,
        ),
        (
            uuid("019ff706-3bc5-7cf0-b3d2-0728c077a930"),
            match_id,
            2_i64,
            1_i64,
            "YellowCardRecorded",
            "second_half",
            58_000_i64,
        ),
        (
            uuid("019ff707-3bc5-7cf0-b3d2-0728c077a930"),
            other_match_id,
            1_i64,
            2_i64,
            "GoalRecorded",
            "first_half",
            1_000_i64,
        ),
        (
            uuid("019ff708-3bc5-7cf0-b3d2-0728c077a930"),
            match_id,
            3_i64,
            3_i64,
            "SubstitutionRecorded",
            "second_half",
            70_000_i64,
        ),
    ] {
        query(
            "INSERT INTO match_events ( \
                 event_id, match_id, aggregate_version, causation_command_id, event_index, \
                 event_type, event_schema_version, period, clock_ms, occurred_at, recorded_at, \
                 actor_id, verification_status, payload \
             ) VALUES ($1, $2, $3, $4, $5, $6, 1, $7, $8, \
                       '2026-08-09T12:00:00Z', '2026-08-09T12:00:01Z', $9, 'provisional', $10)",
        )
        .bind(event_id)
        .bind(event_match_id)
        .bind(aggregate_version)
        .bind(command_id)
        .bind(event_index)
        .bind(event_type)
        .bind(period)
        .bind(clock_ms)
        .bind(admin_id)
        .bind(Json(json!({})))
        .execute(pool)
        .await
        .expect("timeline schema canonical event should insert");
    }

    (
        match_id,
        other_match_id,
        first_event_id,
        uuid("019ff706-3bc5-7cf0-b3d2-0728c077a930"),
        uuid("019ff708-3bc5-7cf0-b3d2-0728c077a930"),
    )
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
async fn match_timeline_projection_schema_enforces_source_order_and_c1_bounds() {
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
        .expect("clean match-timeline schema target should accept a PostgreSQL connection");
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "match-timeline schema target must be clean"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply before schema assertions");

    let (match_id, other_match_id, first_event_id, second_event_id, fourth_event_id) =
        seed_canonical_events(&pool).await;

    query(
        "INSERT INTO match_timeline_projections ( \
             event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
         ) VALUES ($1, $2, 1, 0, 'GoalRecorded', 'first_half', 12000)",
    )
    .bind(first_event_id)
    .bind(match_id)
    .execute(&pool)
    .await
    .expect("valid first timeline row should insert");
    query(
        "INSERT INTO match_timeline_projections ( \
             event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
         ) VALUES ($1, $2, 2, 1, 'YellowCardRecorded', 'second_half', 58000)",
    )
    .bind(second_event_id)
    .bind(match_id)
    .execute(&pool)
    .await
    .expect("valid second timeline row should insert");

    let rows: Vec<(Uuid, i64, i64, String, String, i64)> = query_as(
        "SELECT event_id, aggregate_version, event_index, event_type, period, clock_ms \
         FROM match_timeline_projections WHERE match_id = $1 \
         ORDER BY aggregate_version",
    )
    .bind(match_id)
    .fetch_all(&pool)
    .await
    .expect("timeline rows should be readable in source order");
    assert_eq!(
        rows,
        vec![
            (
                first_event_id,
                1,
                0,
                "GoalRecorded".to_owned(),
                "first_half".to_owned(),
                12_000,
            ),
            (
                second_event_id,
                2,
                1,
                "YellowCardRecorded".to_owned(),
                "second_half".to_owned(),
                58_000,
            ),
        ]
    );

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 1, 0, 'GoalRecorded', 'first_half', 12000)",
        )
        .bind(first_event_id)
        .bind(match_id)
        .execute(&pool),
        "23505",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 2, 2, 'GoalRecorded', 'first_half', 20000)",
        )
        .bind(fourth_event_id)
        .bind(match_id)
        .execute(&pool),
        "23505",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 4, 2, 'GoalRecorded', 'first_half', 20000)",
        )
        .bind(uuid("019ff709-3bc5-7cf0-b3d2-0728c077a930"))
        .bind(match_id)
        .execute(&pool),
        "23503",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 3, 3, 'GoalRecorded', 'first_half', 20000)",
        )
        .bind(fourth_event_id)
        .bind(other_match_id)
        .execute(&pool),
        "23503",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 3, 3, 'UnknownEvent', 'second_half', 70000)",
        )
        .bind(fourth_event_id)
        .bind(match_id)
        .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 3, 3, 'SubstitutionRecorded', 'half_time', 70000)",
        )
        .bind(fourth_event_id)
        .bind(match_id)
        .execute(&pool),
        "23514",
    )
    .await;

    assert_sqlstate(
        query(
            "INSERT INTO match_timeline_projections ( \
                 event_id, match_id, aggregate_version, event_index, event_type, period, clock_ms \
             ) VALUES ($1, $2, 3, 3, 'SubstitutionRecorded', 'second_half', -1)",
        )
        .bind(fourth_event_id)
        .bind(match_id)
        .execute(&pool),
        "23514",
    )
    .await;

    pool.close().await;
}
