use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, LogicalCommandIdentity,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, ClientCommandId, ClockMilliseconds, CommandId,
    EventId, EventVerificationStatus, MatchEventPayload, MatchId, MatchPeriod, RequestFingerprint,
    UtcInstant, VerifyMatchCommand,
};
use football_infrastructure::{
    CommandAdmissionResult, DatabasePoolConfig, admit_command, connect_database,
    execute_verify_match_in_transaction, load_canonical_match_event_stream, run_migrations,
};
use sqlx::{PgPool, query, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(football_infrastructure::MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("verification execution fixture UUID should be valid")
}

async fn admit_claimed_command(
    pool: &PgPool,
    command_id: CommandId,
    client_command_id: ClientCommandId,
    processing_owner: Uuid,
    now_utc: UtcInstant,
) -> CommandAdmissionRequest {
    let request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            client_command_id,
            RequestFingerprint::from_bytes([0xA3; 32]),
        ),
        processing_owner,
        now_utc,
        UtcInstant::from_unix_timestamp(now_utc.unix_timestamp() + 60)
            .expect("verification lease expiry should be valid"),
    )
    .expect("verification command admission request should be valid");

    assert!(matches!(
        admit_command(pool, &request).await,
        Ok(CommandAdmissionResult::Claimed { command_id: actual }) if actual == command_id
    ));
    request
}

fn verification_command(
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    actor: ActorIdentity,
) -> VerifyMatchCommand {
    VerifyMatchCommand::try_new(
        command_id,
        event_id,
        match_id,
        AggregateVersion::initial(),
        MatchPeriod::SecondHalf,
        ClockMilliseconds::from_seconds(90).expect("verification clock should be valid"),
        UtcInstant::from_unix_timestamp(5_001).expect("verification occurred_at should be valid"),
        UtcInstant::from_unix_timestamp(5_002).expect("verification recorded_at should be valid"),
        actor,
    )
    .expect("verification command should be valid")
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn verifies_a_match_in_the_owned_command_transaction_and_reloads_the_marker() {
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
        .expect("clean verification execution target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "verification execution target is not clean: _sqlx_migrations already exists"
    );
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let actor_id = uuid("019feae0-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("verification-execution-supervisor")
    .bind("Verification Execution Supervisor")
    .execute(&pool)
    .await
    .expect("verification supervisor fixture should insert");

    let actor = ActorIdentity::administrator(AdminUserId::from_uuid(actor_id));
    let match_id = MatchId::from_uuid(uuid("019feae0-3bc5-7cf1-b3d2-0728c077a930"));
    let command_id = CommandId::from_uuid(uuid("019feae0-3bc5-7cf2-b3d2-0728c077a930"));
    let event_id = EventId::from_uuid(uuid("019feae0-3bc5-7cf3-b3d2-0728c077a930"));
    let processing_owner = uuid("019feae0-3bc5-7cf4-b3d2-0728c077a930");
    let now_utc = UtcInstant::from_unix_timestamp(5_000).expect("fixture time should be valid");
    let request = admit_claimed_command(
        &pool,
        command_id,
        ClientCommandId::try_from_uuid(uuid("019feae0-3bc5-7cf5-b3d2-0728c077a930"))
            .expect("client command ID should be UUIDv7"),
        processing_owner,
        now_utc,
    )
    .await;
    let context = CommandExecutionContext::from_admission_request(
        request,
        actor,
        Some(AggregateVersion::initial()),
    );
    let command = verification_command(command_id, event_id, match_id, actor);

    let mut transaction = pool
        .begin()
        .await
        .expect("verification execution transaction should begin");
    let stream = execute_verify_match_in_transaction(
        &mut transaction,
        &context,
        command,
        UtcInstant::from_unix_timestamp(5_003).expect("execution time should be valid"),
    )
    .await
    .expect("verification marker should persist in the command transaction");

    assert_eq!(stream.current_version(), AggregateVersion::from_value(1));
    assert_eq!(stream.len(), 1);
    assert!(matches!(
        stream.events()[0].payload(),
        MatchEventPayload::Verification(_)
    ));
    transaction
        .commit()
        .await
        .expect("verification transaction should commit");

    let persisted = load_canonical_match_event_stream(&pool, match_id)
        .await
        .expect("persisted verification stream should reload");
    assert_eq!(persisted.current_version(), AggregateVersion::from_value(1));
    assert_eq!(persisted.events()[0].event_type().as_str(), "MatchVerified");
    assert_eq!(
        persisted.events()[0].verification_status(),
        EventVerificationStatus::Verified
    );
    assert!(matches!(
        persisted.events()[0].payload(),
        MatchEventPayload::Verification(_)
    ));

    let stored_event_count: i64 = query_scalar(
        "SELECT COUNT(*) FROM match_events WHERE match_id = $1 AND event_type = 'MatchVerified'",
    )
    .bind(match_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("persisted verification event count should query");
    assert_eq!(stored_event_count, 1);

    let stored_payload: serde_json::Value = query_scalar(
        "SELECT payload FROM match_events WHERE match_id = $1 AND event_type = 'MatchVerified'",
    )
    .bind(match_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("persisted verification payload should query");
    assert_eq!(stored_payload, serde_json::json!({}));
}
