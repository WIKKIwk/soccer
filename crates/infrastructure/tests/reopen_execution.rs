use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, LogicalCommandIdentity,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, ClientCommandId, ClockMilliseconds, CommandId,
    CorrectionReason, EventId, EventVerificationStatus, MatchEventPayload, MatchId, MatchPeriod,
    ReopenMatchCommand, RequestFingerprint, UtcInstant, VerificationEventPayload,
    VerifyMatchCommand,
};
use football_infrastructure::{
    CommandAdmissionResult, DatabasePoolConfig, admit_command, connect_database,
    execute_reopen_match_in_transaction, execute_verify_match_in_transaction,
    load_canonical_match_event_stream, run_migrations,
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
    Uuid::parse_str(value).expect("reopen execution fixture UUID should be valid")
}

async fn admit_claimed_command(
    pool: &PgPool,
    command_id: CommandId,
    client_command_id: ClientCommandId,
    processing_owner: Uuid,
    fingerprint: u8,
    now_utc: UtcInstant,
) -> CommandAdmissionRequest {
    let request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            client_command_id,
            RequestFingerprint::from_bytes([fingerprint; 32]),
        ),
        processing_owner,
        now_utc,
        UtcInstant::from_unix_timestamp(now_utc.unix_timestamp() + 60)
            .expect("reopen lease expiry should be valid"),
    )
    .expect("reopen command admission request should be valid");

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
        UtcInstant::from_unix_timestamp(6_001).expect("verification occurred_at should be valid"),
        UtcInstant::from_unix_timestamp(6_002).expect("verification recorded_at should be valid"),
        actor,
    )
    .expect("verification command should be valid")
}

fn reopen_command(
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    actor: ActorIdentity,
) -> ReopenMatchCommand {
    ReopenMatchCommand::try_new(
        command_id,
        event_id,
        match_id,
        AggregateVersion::from_value(1),
        MatchPeriod::SecondHalf,
        ClockMilliseconds::from_seconds(91).expect("reopen clock should be valid"),
        UtcInstant::from_unix_timestamp(6_011).expect("reopen occurred_at should be valid"),
        UtcInstant::from_unix_timestamp(6_012).expect("reopen recorded_at should be valid"),
        actor,
        CorrectionReason::try_new("official protocol requires re-verification")
            .expect("reopen reason should be valid"),
    )
    .expect("reopen command should be valid")
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn reopens_a_verified_match_with_a_reason_and_reloads_provisional_state() {
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
        .expect("clean reopen execution target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "reopen execution target is not clean: _sqlx_migrations already exists"
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
    .bind("reopen-execution-supervisor")
    .bind("Reopen Execution Supervisor")
    .execute(&pool)
    .await
    .expect("reopen supervisor fixture should insert");

    let actor = ActorIdentity::administrator(AdminUserId::from_uuid(actor_id));
    let match_id = MatchId::from_uuid(uuid("019feae0-3bc5-7cf1-b3d2-0728c077a930"));
    let processing_owner = uuid("019feae0-3bc5-7cf2-b3d2-0728c077a930");
    let verify_command_id = CommandId::from_uuid(uuid("019feae0-3bc5-7cf3-b3d2-0728c077a930"));
    let verify_event_id = EventId::from_uuid(uuid("019feae0-3bc5-7cf4-b3d2-0728c077a930"));
    let verify_request = admit_claimed_command(
        &pool,
        verify_command_id,
        ClientCommandId::try_from_uuid(uuid("019feae0-3bc5-7cf5-b3d2-0728c077a930"))
            .expect("verify client command ID should be UUIDv7"),
        processing_owner,
        0xB1,
        UtcInstant::from_unix_timestamp(6_000).expect("verify fixture time should be valid"),
    )
    .await;
    let verify_context = CommandExecutionContext::from_admission_request(
        verify_request,
        actor,
        Some(AggregateVersion::initial()),
    );
    let mut transaction = pool
        .begin()
        .await
        .expect("verification transaction should begin");
    execute_verify_match_in_transaction(
        &mut transaction,
        &verify_context,
        verification_command(verify_command_id, verify_event_id, match_id, actor),
        UtcInstant::from_unix_timestamp(6_003).expect("verify execution time should be valid"),
    )
    .await
    .expect("match should verify before it can reopen");
    transaction
        .commit()
        .await
        .expect("verification transaction should commit");

    let reopen_command_id = CommandId::from_uuid(uuid("019feae0-3bc5-7cf6-b3d2-0728c077a930"));
    let reopen_event_id = EventId::from_uuid(uuid("019feae0-3bc5-7cf7-b3d2-0728c077a930"));
    let reopen_request = admit_claimed_command(
        &pool,
        reopen_command_id,
        ClientCommandId::try_from_uuid(uuid("019feae0-3bc5-7cf8-b3d2-0728c077a930"))
            .expect("reopen client command ID should be UUIDv7"),
        processing_owner,
        0xB2,
        UtcInstant::from_unix_timestamp(6_010).expect("reopen fixture time should be valid"),
    )
    .await;
    let reopen_context = CommandExecutionContext::from_admission_request(
        reopen_request,
        actor,
        Some(AggregateVersion::from_value(1)),
    );
    let mut transaction = pool.begin().await.expect("reopen transaction should begin");
    let reopened_stream = execute_reopen_match_in_transaction(
        &mut transaction,
        &reopen_context,
        reopen_command(reopen_command_id, reopen_event_id, match_id, actor),
        UtcInstant::from_unix_timestamp(6_013).expect("reopen execution time should be valid"),
    )
    .await
    .expect("verified match should reopen with a reason");
    assert_eq!(
        reopened_stream.current_version(),
        AggregateVersion::from_value(2)
    );
    assert!(matches!(
        reopened_stream.events()[1].payload(),
        MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(payload))
            if payload.reason().as_str() == "official protocol requires re-verification"
    ));
    transaction
        .commit()
        .await
        .expect("reopen transaction should commit");

    let persisted = load_canonical_match_event_stream(&pool, match_id)
        .await
        .expect("reopened stream should reload");
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted.events()[0].event_type().as_str(), "MatchVerified");
    assert_eq!(
        persisted.events()[0].verification_status(),
        EventVerificationStatus::Verified
    );
    assert_eq!(persisted.events()[1].event_type().as_str(), "MatchReopened");
    assert_eq!(
        persisted.events()[1].verification_status(),
        EventVerificationStatus::Provisional
    );

    let stored_payload: serde_json::Value = query_scalar(
        "SELECT payload FROM match_events WHERE match_id = $1 AND event_type = 'MatchReopened'",
    )
    .bind(match_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("persisted reopen payload should query");
    assert_eq!(
        stored_payload,
        serde_json::json!({
            "reason": "official protocol requires re-verification"
        })
    );
}
