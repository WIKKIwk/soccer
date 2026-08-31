mod support;

use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, CommandOrchestrationResult,
    CommandOrchestrator, LogicalCommandIdentity,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, ClientCommandId, ClockMilliseconds, CommandId,
    EventId, FixtureId, LifecycleEventPayload, LineupId, LineupSubmittedPayload, MatchId,
    MatchLifecycleCommand, RequestFingerprint, TeamId, UtcInstant,
};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, PostgresCommandAdmission, PostgresLifecycleExecutor,
    connect_database, run_migrations,
};
use sqlx::{PgPool, query, query_as, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("lifecycle execution fixture UUID should be valid")
}

fn actor() -> ActorIdentity {
    ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
        "019ff5a0-3bc5-7cf0-b3d2-0728c077a930",
    )))
}

async fn apply_migrations(pool: &PgPool) {
    let clean: bool = query_scalar("SELECT to_regclass('_sqlx_migrations') IS NULL")
        .fetch_one(pool)
        .await
        .expect("migration preflight should succeed");
    assert!(clean, "lifecycle execution target must be clean");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY);
    run_migrations(pool, path)
        .await
        .expect("repository migrations should apply");
}

#[allow(clippy::too_many_arguments)]
async fn execute_action(
    pool: &PgPool,
    match_id: MatchId,
    expected: u64,
    command_id: &str,
    event_id: &str,
    client_id: &str,
    owner: &str,
    action: LifecycleEventPayload,
) {
    let command_id = CommandId::from_uuid(uuid(command_id));
    let now = UtcInstant::from_unix_timestamp(2_000 + expected as i64).unwrap();
    let request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            ClientCommandId::try_from_uuid(uuid(client_id)).unwrap(),
            RequestFingerprint::from_bytes([expected as u8 + 1; 32]),
        ),
        uuid(owner),
        now,
        UtcInstant::from_unix_timestamp(2_100 + expected as i64).unwrap(),
    )
    .unwrap();
    let command = MatchLifecycleCommand::new(
        command_id,
        EventId::from_uuid(uuid(event_id)),
        match_id,
        AggregateVersion::from_value(expected),
        ClockMilliseconds::from_seconds(expected * 10).unwrap(),
        now,
        now,
        actor(),
        action,
    );
    let context = CommandExecutionContext::from_admission_request(
        request,
        actor(),
        Some(AggregateVersion::from_value(expected)),
    );
    let result = CommandOrchestrator::new(
        PostgresCommandAdmission::new(pool),
        PostgresLifecycleExecutor::new(pool, now),
    )
    .execute(&request, &context, &command)
    .await
    .expect("lifecycle command should execute");
    assert!(matches!(result, CommandOrchestrationResult::Executed(_)));
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn lifecycle_commands_publish_state_lineups_audit_and_outbox_atomically() {
    assert_eq!(
        std::env::var("ALLOW_CLEAN_DATABASE_MIGRATION").as_deref(),
        Ok("1"),
        "set ALLOW_CLEAN_DATABASE_MIGRATION=1 to authorize this database operation"
    );
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must point to an approved clean database");
    let pool = connect_database(&database_url, DatabasePoolConfig::default())
        .await
        .expect("clean lifecycle target should connect");
    apply_migrations(&pool).await;

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active) \
         VALUES ($1, 'lifecycle-admin', 'Lifecycle Admin', TRUE)",
    )
    .bind(actor().admin_user_id().as_uuid())
    .execute(&pool)
    .await
    .expect("administrator should insert");
    let match_id = MatchId::from_uuid(uuid("019ff5a0-3bc5-7cf1-b3d2-0728c077a930"));
    let fixture_id = FixtureId::from_uuid(uuid("019ff5a0-3bc5-7cf2-b3d2-0728c077a930"));
    let home = TeamId::from_uuid(uuid("019ff5a0-3bc5-7cf3-b3d2-0728c077a930"));
    let away = TeamId::from_uuid(uuid("019ff5a0-3bc5-7cf4-b3d2-0728c077a930"));
    support::seed_assigned_fixture(
        &pool,
        actor().admin_user_id().as_uuid(),
        match_id,
        fixture_id,
        home,
        away,
    )
    .await;

    let home_lineup = LineupSubmittedPayload::try_new(
        LineupId::from_uuid(uuid("019ff5a0-3bc5-7cf5-b3d2-0728c077a930")),
        home,
        vec![],
    )
    .unwrap();
    let away_lineup = LineupSubmittedPayload::try_new(
        LineupId::from_uuid(uuid("019ff5a0-3bc5-7cf6-b3d2-0728c077a930")),
        away,
        vec![],
    )
    .unwrap();
    let actions = [
        LifecycleEventPayload::MatchScheduled,
        LifecycleEventPayload::LineupSubmitted(home_lineup),
        LifecycleEventPayload::LineupSubmitted(away_lineup),
        LifecycleEventPayload::LineupConfirmed,
        LifecycleEventPayload::FirstHalfStarted,
        LifecycleEventPayload::FirstHalfEnded,
        LifecycleEventPayload::SecondHalfStarted,
        LifecycleEventPayload::MatchFinished,
    ];
    let ids = [
        ("7d00", "7d10", "7d20", "7d30"),
        ("7d01", "7d11", "7d21", "7d31"),
        ("7d02", "7d12", "7d22", "7d32"),
        ("7d03", "7d13", "7d23", "7d33"),
        ("7d04", "7d14", "7d24", "7d34"),
        ("7d05", "7d15", "7d25", "7d35"),
        ("7d06", "7d16", "7d26", "7d36"),
        ("7d07", "7d17", "7d27", "7d37"),
    ];
    for (index, (action, (command, event, client, owner))) in
        actions.into_iter().zip(ids).enumerate()
    {
        let make_id = |middle: &str| format!("019ff5a0-3bc5-{middle}-b3d2-0728c077a930");
        execute_action(
            &pool,
            match_id,
            index as u64,
            &make_id(command),
            &make_id(event),
            &make_id(client),
            &make_id(owner),
            action,
        )
        .await;
    }

    let summary: (String, i64, i64, bool) = query_as(
        "SELECT match_state, source_aggregate_version, projection_version, is_current \
         FROM match_summary_projections WHERE match_id = $1",
    )
    .bind(match_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("finished summary should load");
    assert_eq!(summary, ("finished".to_owned(), 8, 8, true));
    let confirmed: i64 = query_scalar(
        "SELECT COUNT(*) FROM match_lineup_team_projections \
         WHERE match_id = $1 AND lineup_status = 'confirmed'",
    )
    .bind(match_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(confirmed, 2);
    for (table, statement) in [
        ("commands", "SELECT COUNT(*) FROM commands"),
        ("command_audit", "SELECT COUNT(*) FROM command_audit"),
        ("outbox_messages", "SELECT COUNT(*) FROM outbox_messages"),
    ] {
        let count: i64 = query_scalar(statement).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 8, "{table} should contain one row per command");
    }
    pool.close().await;
}
