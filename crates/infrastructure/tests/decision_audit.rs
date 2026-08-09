use football_application::commands::{CommandAdmissionRequest, LogicalCommandIdentity};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, ClientCommandId, CommandId, CorrectionReason,
    EventId, EvidenceReference, MatchId, RequestFingerprint, UtcInstant,
};
use football_infrastructure::{
    CommandAdmissionResult, CommandAuditEntry, CriticalDecisionAudit, DatabasePoolConfig,
    admit_command, connect_database, load_command_audit, persist_command_audit, run_migrations,
};
use serde_json::Value;
use sqlx::{PgPool, query, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(football_infrastructure::MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("decision audit fixture UUID should be valid")
}

fn instant(seconds: i64) -> UtcInstant {
    UtcInstant::from_unix_timestamp(seconds).expect("decision audit fixture time should be valid")
}

async fn admit_claimed_command(
    pool: &PgPool,
    command_id: CommandId,
    client_command_id: ClientCommandId,
    processing_owner: Uuid,
    fingerprint: u8,
) {
    let now_utc = instant(7_000);
    let request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            client_command_id,
            RequestFingerprint::from_bytes([fingerprint; 32]),
        ),
        processing_owner,
        now_utc,
        instant(7_060),
    )
    .expect("decision audit admission request should be valid");

    assert!(matches!(
        admit_command(pool, &request).await,
        Ok(CommandAdmissionResult::Claimed { command_id: actual }) if actual == command_id
    ));
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    pool: &PgPool,
    event_id: EventId,
    match_id: MatchId,
    aggregate_version: i64,
    command_id: CommandId,
    actor_id: Uuid,
    event_type: &str,
    verification_status: &str,
    payload: Value,
    voids_event_id: Option<EventId>,
) {
    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id, event_index,
             event_type, event_schema_version, period, clock_ms, occurred_at, recorded_at,
             actor_id, verification_status, payload, supersedes_event_id, voids_event_id
         ) VALUES ($1, $2, $3, $4, 0, $5, 1, 'second_half', $6, $7, $8, $9, $10, $11, NULL, $12)",
    )
    .bind(event_id.as_uuid())
    .bind(match_id.as_uuid())
    .bind(aggregate_version)
    .bind(command_id.as_uuid())
    .bind(event_type)
    .bind(aggregate_version * 60)
    .bind(instant(7_000 + aggregate_version).as_offset_datetime())
    .bind(instant(7_001 + aggregate_version).as_offset_datetime())
    .bind(actor_id)
    .bind(verification_status)
    .bind(sqlx::types::Json(payload))
    .bind(voids_event_id.map(EventId::as_uuid))
    .execute(pool)
    .await
    .expect("decision audit event fixture should insert");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn persists_correction_and_verification_decision_audits_atomically() {
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
        .expect("clean decision audit target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "decision audit target is not clean: _sqlx_migrations already exists"
    );
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let actor_id = uuid("019ff700-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("decision-audit-supervisor")
    .bind("Decision Audit Supervisor")
    .execute(&pool)
    .await
    .expect("decision audit supervisor fixture should insert");

    let actor = ActorIdentity::administrator(AdminUserId::from_uuid(actor_id));
    let match_id = MatchId::from_uuid(uuid("019ff701-3bc5-7cf0-b3d2-0728c077a930"));
    let goal_command_id = CommandId::from_uuid(uuid("019ff702-3bc5-7cf0-b3d2-0728c077a930"));
    let correction_command_id = CommandId::from_uuid(uuid("019ff703-3bc5-7cf0-b3d2-0728c077a930"));
    let verify_command_id = CommandId::from_uuid(uuid("019ff704-3bc5-7cf0-b3d2-0728c077a930"));
    let reopen_command_id = CommandId::from_uuid(uuid("019ff705-3bc5-7cf0-b3d2-0728c077a930"));
    let goal_event_id = EventId::from_uuid(uuid("019ff706-3bc5-7cf0-b3d2-0728c077a930"));
    let correction_event_id = EventId::from_uuid(uuid("019ff707-3bc5-7cf0-b3d2-0728c077a930"));
    let verify_event_id = EventId::from_uuid(uuid("019ff708-3bc5-7cf0-b3d2-0728c077a930"));
    let reopen_event_id = EventId::from_uuid(uuid("019ff709-3bc5-7cf0-b3d2-0728c077a930"));
    let processing_owner = uuid("019ff70a-3bc5-7cf0-b3d2-0728c077a930");

    for (command_id, client_command_id, fingerprint) in [
        (
            goal_command_id,
            "019ff710-3bc5-7cf0-b3d2-0728c077a930",
            0x71,
        ),
        (
            correction_command_id,
            "019ff711-3bc5-7cf0-b3d2-0728c077a930",
            0x72,
        ),
        (
            verify_command_id,
            "019ff712-3bc5-7cf0-b3d2-0728c077a930",
            0x73,
        ),
        (
            reopen_command_id,
            "019ff713-3bc5-7cf0-b3d2-0728c077a930",
            0x74,
        ),
    ] {
        admit_claimed_command(
            &pool,
            command_id,
            ClientCommandId::try_from_uuid(uuid(client_command_id))
                .expect("decision audit client command ID should be UUIDv7"),
            processing_owner,
            fingerprint,
        )
        .await;
    }

    insert_event(
        &pool,
        goal_event_id,
        match_id,
        1,
        goal_command_id,
        actor_id,
        "GoalRecorded",
        "provisional",
        serde_json::json!({
            "team_id": "019ff720-3bc5-7cf0-b3d2-0728c077a930",
            "scorer_player_id": null,
            "award_type": "regular"
        }),
        None,
    )
    .await;
    insert_event(
        &pool,
        correction_event_id,
        match_id,
        2,
        correction_command_id,
        actor_id,
        "EventVoided",
        "verified",
        serde_json::json!({
            "reason": "official protocol removed the provisional goal"
        }),
        Some(goal_event_id),
    )
    .await;
    insert_event(
        &pool,
        verify_event_id,
        match_id,
        3,
        verify_command_id,
        actor_id,
        "MatchVerified",
        "verified",
        serde_json::json!({}),
        None,
    )
    .await;
    insert_event(
        &pool,
        reopen_event_id,
        match_id,
        4,
        reopen_command_id,
        actor_id,
        "MatchReopened",
        "provisional",
        serde_json::json!({
            "reason": "disciplinary correction requires review"
        }),
        None,
    )
    .await;

    let evidence = EvidenceReference::try_new("PFL protocol ref 2026/001")
        .expect("decision audit evidence reference should be valid");
    let correction_decision = CriticalDecisionAudit::void_event(
        goal_event_id,
        CorrectionReason::try_new("official protocol removed the provisional goal")
            .expect("correction audit reason should be valid"),
        Some(evidence.clone()),
    );
    let verify_decision = CriticalDecisionAudit::verify_match(Some(evidence.clone()));
    let reopen_decision = CriticalDecisionAudit::reopen_match(
        CorrectionReason::try_new("disciplinary correction requires review")
            .expect("reopen audit reason should be valid"),
        Some(evidence),
    );

    let correction_audit = CommandAuditEntry::accepted_decision(
        correction_command_id,
        actor,
        match_id,
        &correction_decision,
        AggregateVersion::from_value(2),
        correction_event_id,
        instant(7_020),
    )
    .expect("correction decision audit entry should be valid");
    let verify_audit = CommandAuditEntry::accepted_decision(
        verify_command_id,
        actor,
        match_id,
        &verify_decision,
        AggregateVersion::from_value(3),
        verify_event_id,
        instant(7_030),
    )
    .expect("verification decision audit entry should be valid");
    let reopen_audit = CommandAuditEntry::accepted_decision(
        reopen_command_id,
        actor,
        match_id,
        &reopen_decision,
        AggregateVersion::from_value(4),
        reopen_event_id,
        instant(7_040),
    )
    .expect("reopen decision audit entry should be valid");

    let mut transaction = pool
        .begin()
        .await
        .expect("decision audit transaction should begin");
    for entry in [&correction_audit, &verify_audit, &reopen_audit] {
        persist_command_audit(&mut transaction, entry)
            .await
            .expect("decision audit should persist in the transaction");
    }
    transaction
        .commit()
        .await
        .expect("decision audit transaction should commit");

    let stored_correction = load_command_audit(&pool, correction_command_id)
        .await
        .expect("correction audit should reload");
    assert_eq!(stored_correction.action, "VoidEvent");
    assert_eq!(stored_correction.outcome, "accepted");
    assert_eq!(stored_correction.event_id, Some(correction_event_id));
    assert_eq!(
        stored_correction.details,
        serde_json::json!({
            "decision": "void_event",
            "evidence_reference": "PFL protocol ref 2026/001",
            "reason": "official protocol removed the provisional goal",
            "voids_event_id": goal_event_id.to_string(),
        })
    );

    let stored_verify = load_command_audit(&pool, verify_command_id)
        .await
        .expect("verification audit should reload");
    assert_eq!(stored_verify.action, "VerifyMatch");
    assert_eq!(stored_verify.event_id, Some(verify_event_id));
    assert_eq!(
        stored_verify.details,
        serde_json::json!({
            "decision": "verify_match",
            "evidence_reference": "PFL protocol ref 2026/001",
        })
    );

    let stored_reopen = load_command_audit(&pool, reopen_command_id)
        .await
        .expect("reopen audit should reload");
    assert_eq!(stored_reopen.action, "ReopenMatch");
    assert_eq!(stored_reopen.event_id, Some(reopen_event_id));
    assert_eq!(
        stored_reopen.details,
        serde_json::json!({
            "decision": "reopen_match",
            "evidence_reference": "PFL protocol ref 2026/001",
            "reason": "disciplinary correction requires review",
        })
    );

    let rollback_command_id = CommandId::from_uuid(uuid("019ff730-3bc5-7cf0-b3d2-0728c077a930"));
    admit_claimed_command(
        &pool,
        rollback_command_id,
        ClientCommandId::try_from_uuid(uuid("019ff731-3bc5-7cf0-b3d2-0728c077a930"))
            .expect("rollback client command ID should be UUIDv7"),
        processing_owner,
        0x75,
    )
    .await;
    let rollback_decision = CriticalDecisionAudit::verify_match(None);
    let rollback_entry = CommandAuditEntry::accepted_decision(
        rollback_command_id,
        actor,
        match_id,
        &rollback_decision,
        AggregateVersion::from_value(3),
        verify_event_id,
        instant(7_050),
    )
    .expect("rollback audit entry should be valid");
    let mut transaction = pool
        .begin()
        .await
        .expect("rollback audit transaction should begin");
    persist_command_audit(&mut transaction, &rollback_entry)
        .await
        .expect("rollback audit should persist before rollback");
    transaction
        .rollback()
        .await
        .expect("rollback audit transaction should roll back");

    let rollback_count: i64 =
        query_scalar("SELECT COUNT(*) FROM command_audit WHERE command_id = $1")
            .bind(rollback_command_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("rollback audit count should query");
    assert_eq!(rollback_count, 0);
}
