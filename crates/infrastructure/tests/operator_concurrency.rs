use football_domain::{
    AdminUserId, FixtureId, MatchAssignment, MatchId, OperatorLeaseExpiryPolicy, OperatorLeaseId,
    UtcInstant,
};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, OperatorAssignmentRepositoryError,
    claim_operator_lease, connect_database, load_match_assignment, load_operator_lease,
    persist_match_assignment, run_migrations, takeover_operator_lease,
};
use sqlx::{PgPool, query, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("operator concurrency fixture UUID should be valid")
}

async fn apply_migrations(pool: &PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "operator concurrency target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

async fn seed_assignment_with_expired_lease(pool: &PgPool) -> (MatchAssignment, OperatorLeaseId) {
    let first_operator_id = uuid("019fe800-3bc5-7cf0-b3d2-0728c077a930");
    let second_operator_id = uuid("019fe800-3bc5-7cf1-b3d2-0728c077a930");
    let competition_id = uuid("019fe800-3bc5-7cf2-b3d2-0728c077a930");
    let season_id = uuid("019fe800-3bc5-7cf3-b3d2-0728c077a930");
    let home_team_id = uuid("019fe800-3bc5-7cf4-b3d2-0728c077a930");
    let away_team_id = uuid("019fe800-3bc5-7cf5-b3d2-0728c077a930");
    let fixture_id = uuid("019fe800-3bc5-7cf6-b3d2-0728c077a930");
    let match_id = uuid("019fe800-3bc5-7cf7-b3d2-0728c077a930");
    let initial_lease_id = OperatorLeaseId::from_uuid(uuid("019fe800-3bc5-7cf8-b3d2-0728c077a930"));

    for (id, username, display_name) in [
        (first_operator_id, "concurrency-one", "Concurrency One"),
        (second_operator_id, "concurrency-two", "Concurrency Two"),
    ] {
        query(
            "INSERT INTO admin_users (id, username, display_name, is_active)
             VALUES ($1, $2, $3, TRUE)",
        )
        .bind(id)
        .bind(username)
        .bind(display_name)
        .execute(pool)
        .await
        .expect("concurrency operator should insert");
    }

    query("INSERT INTO competitions (id, name) VALUES ($1, $2)")
        .bind(competition_id)
        .bind("Concurrency Competition")
        .execute(pool)
        .await
        .expect("concurrency competition should insert");
    query("INSERT INTO seasons (id, competition_id, year) VALUES ($1, $2, $3)")
        .bind(season_id)
        .bind(competition_id)
        .bind(2026_i32)
        .execute(pool)
        .await
        .expect("concurrency season should insert");

    for (id, name) in [
        (home_team_id, "Concurrency Home"),
        (away_team_id, "Concurrency Away"),
    ] {
        query("INSERT INTO teams (id, name) VALUES ($1, $2)")
            .bind(id)
            .bind(name)
            .execute(pool)
            .await
            .expect("concurrency team should insert");
    }

    query(
        "INSERT INTO fixtures
            (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(fixture_id)
    .bind(competition_id)
    .bind(season_id)
    .bind(home_team_id)
    .bind(away_team_id)
    .bind(
        UtcInstant::from_unix_timestamp(2_000)
            .expect("concurrency kickoff should be valid")
            .as_offset_datetime(),
    )
    .execute(pool)
    .await
    .expect("concurrency fixture should insert");

    let assignment = MatchAssignment::new(
        MatchId::from_uuid(match_id),
        FixtureId::from_uuid(fixture_id),
        AdminUserId::from_uuid(first_operator_id),
    );
    persist_match_assignment(pool, &assignment)
        .await
        .expect("concurrency assignment should persist");

    let claimed_at =
        UtcInstant::from_unix_timestamp(100).expect("initial claim time should be valid");
    let expires_at = OperatorLeaseExpiryPolicy::expiry_at(claimed_at, 1)
        .expect("initial lease expiry should be valid");
    claim_operator_lease(
        pool,
        assignment.match_id(),
        assignment.operator_id(),
        initial_lease_id,
        claimed_at,
        expires_at,
    )
    .await
    .expect("initial expired lease should persist");

    let stored_assignment = load_match_assignment(pool, assignment.match_id())
        .await
        .expect("seeded assignment should load")
        .expect("seeded assignment should exist");
    assert_eq!(stored_assignment, assignment);

    let stored_lease = load_operator_lease(pool, assignment.match_id())
        .await
        .expect("seeded lease should load")
        .expect("seeded lease should exist");
    assert_eq!(stored_lease.id(), initial_lease_id);
    assert_eq!(stored_lease.expires_at(), expires_at);

    let active_lease_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_operator_leases WHERE match_id = $1")
            .bind(assignment.match_id().as_uuid())
            .fetch_one(pool)
            .await
            .expect("seeded lease count should query");
    assert_eq!(active_lease_count, 1);

    (assignment, initial_lease_id)
}

fn assert_active_lease_conflict(
    error: OperatorAssignmentRepositoryError,
    expected_match_id: MatchId,
    expected_lease_id: OperatorLeaseId,
    expected_operator_id: AdminUserId,
) {
    match error {
        OperatorAssignmentRepositoryError::ActiveLeaseConflict {
            match_id,
            lease_id,
            operator_id,
        } => {
            assert_eq!(match_id, expected_match_id);
            assert_eq!(lease_id, expected_lease_id);
            assert_eq!(operator_id, expected_operator_id);
        }
        other => panic!("expected typed active lease conflict, got {other:?}"),
    }
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn concurrent_two_operator_takeovers_allow_only_one_winner() {
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
        .expect("clean operator concurrency target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;
    let (assignment, initial_lease_id) = seed_assignment_with_expired_lease(&pool).await;

    let first_operator_id = AdminUserId::from_uuid(uuid("019fe800-3bc5-7cf0-b3d2-0728c077a930"));
    let second_operator_id = AdminUserId::from_uuid(uuid("019fe800-3bc5-7cf1-b3d2-0728c077a930"));
    let first_takeover_lease_id =
        OperatorLeaseId::from_uuid(uuid("019fe800-3bc5-7cf9-b3d2-0728c077a930"));
    let second_takeover_lease_id =
        OperatorLeaseId::from_uuid(uuid("019fe800-3bc5-7cfa-b3d2-0728c077a930"));
    let takeover_at = UtcInstant::from_unix_timestamp(200).expect("takeover time should be valid");
    let takeover_expires_at = OperatorLeaseExpiryPolicy::expiry_at(takeover_at, 60)
        .expect("takeover expiry should be valid");

    let (first_result, second_result) = tokio::join!(
        takeover_operator_lease(
            &pool,
            assignment.match_id(),
            first_operator_id,
            first_takeover_lease_id,
            takeover_at,
            takeover_expires_at,
        ),
        takeover_operator_lease(
            &pool,
            assignment.match_id(),
            second_operator_id,
            second_takeover_lease_id,
            takeover_at,
            takeover_expires_at,
        ),
    );

    let (winner, losing_lease_id, losing_operator_id) = match (first_result, second_result) {
        (Ok(winner), Err(error)) => {
            assert_active_lease_conflict(
                error,
                assignment.match_id(),
                winner.id(),
                winner.operator_id(),
            );
            (winner, second_takeover_lease_id, second_operator_id)
        }
        (Err(error), Ok(winner)) => {
            assert_active_lease_conflict(
                error,
                assignment.match_id(),
                winner.id(),
                winner.operator_id(),
            );
            (winner, first_takeover_lease_id, first_operator_id)
        }
        (left, right) => panic!(
            "concurrent takeovers must have exactly one winner: left={left:?}, right={right:?}"
        ),
    };

    assert_ne!(winner.id(), losing_lease_id);
    assert_ne!(winner.operator_id(), losing_operator_id);
    assert_eq!(winner.match_id(), assignment.match_id());
    assert_eq!(winner.claimed_at(), takeover_at);
    assert_eq!(winner.expires_at(), takeover_expires_at);

    let persisted_assignment = load_match_assignment(&pool, assignment.match_id())
        .await
        .expect("concurrent assignment should load")
        .expect("concurrent assignment should remain present");
    assert_eq!(persisted_assignment.operator_id(), winner.operator_id());

    let persisted_lease = load_operator_lease(&pool, assignment.match_id())
        .await
        .expect("concurrent lease should load")
        .expect("concurrent lease should remain present");
    assert_eq!(persisted_lease, winner);

    let persisted_lease_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_operator_leases WHERE match_id = $1")
            .bind(assignment.match_id().as_uuid())
            .fetch_one(&pool)
            .await
            .expect("concurrent lease count should query");
    assert_eq!(persisted_lease_count, 1);

    let losing_lease_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_operator_leases WHERE lease_id = $1")
            .bind(losing_lease_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("losing lease count should query");
    assert_eq!(losing_lease_count, 0);

    let initial_lease_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_operator_leases WHERE lease_id = $1")
            .bind(initial_lease_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("initial lease count should query");
    assert_eq!(initial_lease_count, 0);

    pool.close().await;
}
