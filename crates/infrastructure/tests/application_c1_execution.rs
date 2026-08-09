use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, CommandOrchestrationError,
    CommandOrchestrationResult, CommandOrchestrator, LogicalCommandIdentity,
    PersistedCommandResult,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1LiveAction, C1LiveActionCommand,
    C1LiveActionCommandError, ClientCommandId, ClockMilliseconds, CommandId, CommandOutcome,
    EventAppendError, EventId, EventVerificationStatus, GoalAwardType, GoalRecordedPayload,
    MatchId, MatchPeriod, MatchProjection, RequestFingerprint, TeamId, UtcInstant,
};
use football_infrastructure::{
    C1LiveActionCommandExecutionError, CommandAuditRepositoryError, DatabasePoolConfig,
    MIGRATIONS_DIRECTORY, PostgresC1LiveActionExecutor, PostgresCommandAdmission,
    StoredTerminalCommandResult, connect_database, load_c1_event_stream, load_command_audit,
    load_outbox_message, load_terminal_command_result, run_migrations,
};
use serde_json::json;
use sqlx::{PgPool, query, query_scalar, types::Json};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("application C1 execution fixture UUID should be valid")
}

fn actor() -> ActorIdentity {
    ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
        "019fe950-3bc5-7cf0-b3d2-0728c077a930",
    )))
}

fn request(
    command_id: CommandId,
    client_command_id: Uuid,
    fingerprint: u8,
    processing_owner: Uuid,
    now_seconds: i64,
) -> CommandAdmissionRequest {
    CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            ClientCommandId::try_from_uuid(client_command_id)
                .expect("fixture client command ID should be UUIDv7"),
            RequestFingerprint::from_bytes([fingerprint; 32]),
        ),
        processing_owner,
        UtcInstant::from_unix_timestamp(now_seconds).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(now_seconds + 30)
            .expect("fixture lease expiry should be valid"),
    )
    .expect("fixture command admission request should be valid")
}

fn goal_command(
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    team_id: TeamId,
    expected_version: AggregateVersion,
    occurred_at: i64,
    recorded_at: i64,
) -> C1LiveActionCommand {
    C1LiveActionCommand::try_new(
        command_id,
        event_id,
        match_id,
        expected_version,
        MatchPeriod::FirstHalf,
        ClockMilliseconds::from_seconds(12).expect("fixture clock should be valid"),
        UtcInstant::from_unix_timestamp(occurred_at).expect("fixture event time should be valid"),
        UtcInstant::from_unix_timestamp(recorded_at).expect("fixture event time should be valid"),
        actor(),
        EventVerificationStatus::Provisional,
        C1LiveAction::RecordGoal(GoalRecordedPayload::new(
            team_id,
            None,
            GoalAwardType::Regular,
        )),
    )
    .expect("fixture C1 command should be valid")
}

async fn apply_migrations(pool: &PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "application C1 execution target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn application_orchestrator_commits_c1_event_and_terminal_rejection_atomically() {
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
        .expect("clean application C1 target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let actor_id = uuid("019fe950-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("application-c1-admin")
    .bind("Application C1 Admin")
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let match_id = MatchId::from_uuid(uuid("019fe950-3bc5-7cf1-b3d2-0728c077a930"));
    let home_team_id = TeamId::from_uuid(uuid("019fe950-3bc5-7cf2-b3d2-0728c077a930"));
    let away_team_id = TeamId::from_uuid(uuid("019fe950-3bc5-7cf3-b3d2-0728c077a930"));
    let first_command_id = CommandId::from_uuid(uuid("019fe950-3bc5-7cf4-b3d2-0728c077a930"));
    let first_request = request(
        first_command_id,
        uuid("019fe950-3bc5-7cf5-b3d2-0728c077a930"),
        0x41,
        uuid("019fe950-3bc5-7cf6-b3d2-0728c077a930"),
        2_000,
    );
    let first_command = goal_command(
        first_command_id,
        EventId::from_uuid(uuid("019fe950-3bc5-7cf7-b3d2-0728c077a930")),
        match_id,
        home_team_id,
        AggregateVersion::initial(),
        2_001,
        2_002,
    );
    let first_context = CommandExecutionContext::from_admission_request(
        first_request,
        actor(),
        Some(AggregateVersion::initial()),
    );
    let orchestrator = CommandOrchestrator::new(
        PostgresCommandAdmission::new(&pool),
        PostgresC1LiveActionExecutor::new(
            &pool,
            UtcInstant::from_unix_timestamp(2_015).expect("fixture execution time should be valid"),
        ),
    );

    let first_result = orchestrator
        .execute(&first_request, &first_context, &first_command)
        .await
        .expect("first C1 application command should execute");
    let first_envelope = match first_result {
        CommandOrchestrationResult::Executed(envelope) => envelope,
        other => panic!("first C1 command should execute, got {other:?}"),
    };
    assert!(matches!(
        first_envelope.outcome(),
        CommandOutcome::Accepted {
            final_aggregate_version,
            event_count: 1,
            result,
        } if *final_aggregate_version == AggregateVersion::from_value(1)
            && result.event_id() == first_command.event_id()
            && result.event_type().as_str() == "GoalRecorded"
    ));

    let first_status: String = query_scalar("SELECT status FROM commands WHERE id = $1")
        .bind(first_command_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("accepted command status query should succeed");
    assert_eq!(first_status, "accepted");

    let stream = load_c1_event_stream(&pool, match_id)
        .await
        .expect("accepted C1 event should be readable");
    assert_eq!(stream.len(), 1);
    let projection = MatchProjection::from_stream(&stream, home_team_id, away_team_id)
        .expect("accepted C1 event should reduce into the projection");
    assert_eq!(projection.score().home_goals(), 1);
    assert_eq!(projection.score().away_goals(), 0);

    let first_terminal = load_terminal_command_result(&pool, first_command_id)
        .await
        .expect("accepted terminal result should be readable");
    assert!(matches!(
        first_terminal,
        StoredTerminalCommandResult::Accepted {
            final_aggregate_version,
            event_count: 1,
            result_payload,
            ..
        } if final_aggregate_version == AggregateVersion::from_value(1)
            && result_payload == json!({
                "event_id": first_command.event_id().to_string(),
                "event_type": "GoalRecorded"
            })
    ));

    let first_audit = load_command_audit(&pool, first_command_id)
        .await
        .expect("accepted command audit should be readable");
    assert_eq!(first_audit.command_id, first_command_id);
    assert_eq!(first_audit.actor_id, actor_id);
    assert_eq!(first_audit.aggregate_id, match_id);
    assert_eq!(first_audit.action, "RecordGoal");
    assert_eq!(first_audit.outcome, "accepted");
    assert_eq!(first_audit.aggregate_version, 1);
    assert_eq!(first_audit.event_count, 1);
    assert_eq!(first_audit.event_id, Some(first_command.event_id()));
    assert_eq!(first_audit.rejection_code, None);
    assert_eq!(first_audit.details, json!({}));

    let first_outbox = load_outbox_message(&pool, first_command.event_id())
        .await
        .expect("accepted command outbox message should be readable");
    assert_eq!(first_outbox.message_id, first_command.event_id());
    assert_eq!(first_outbox.aggregate_id, match_id);
    assert_eq!(first_outbox.source_aggregate_version, 1);
    assert_eq!(first_outbox.causation_command_id, first_command_id);
    assert_eq!(first_outbox.source_event_id, first_command.event_id());
    assert_eq!(first_outbox.message_type, "match.event");
    assert_eq!(first_outbox.schema_version, 1);
    assert_eq!(first_outbox.payload["event_type"], "GoalRecorded");
    assert_eq!(first_outbox.payload["aggregate_version"], 1);
    assert_eq!(first_outbox.payload["payload"]["award_type"], "regular");
    assert!(first_outbox.payload.get("request_fingerprint").is_none());

    let second_command_id = CommandId::from_uuid(uuid("019fe950-3bc5-7cf8-b3d2-0728c077a930"));
    let second_request = request(
        second_command_id,
        uuid("019fe950-3bc5-7cf9-b3d2-0728c077a930"),
        0x42,
        uuid("019fe950-3bc5-7cfa-b3d2-0728c077a930"),
        2_010,
    );
    let second_command = goal_command(
        second_command_id,
        EventId::from_uuid(uuid("019fe950-3bc5-7cfb-b3d2-0728c077a930")),
        match_id,
        home_team_id,
        AggregateVersion::initial(),
        2_011,
        2_012,
    );
    let second_context = CommandExecutionContext::from_admission_request(
        second_request,
        actor(),
        Some(AggregateVersion::initial()),
    );
    let second_result = orchestrator
        .execute(&second_request, &second_context, &second_command)
        .await
        .expect("expected-version conflict should become a terminal rejection");
    let second_envelope = match second_result {
        CommandOrchestrationResult::Executed(envelope) => envelope,
        other => panic!("second C1 command should produce a result, got {other:?}"),
    };
    assert!(matches!(
        second_envelope.outcome(),
        CommandOutcome::Rejected {
            aggregate_version,
            error: C1LiveActionCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict { expected, actual }
            ),
        } if *aggregate_version == AggregateVersion::from_value(1)
            && *expected == AggregateVersion::initial()
            && *actual == AggregateVersion::from_value(1)
    ));

    let event_count: i64 = query_scalar("SELECT COUNT(*) FROM match_events WHERE match_id = $1")
        .bind(match_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("canonical event count query should succeed");
    assert_eq!(event_count, 1);

    let rejected_outbox_count: i64 =
        query_scalar("SELECT COUNT(*) FROM outbox_messages WHERE message_id = $1")
            .bind(second_command.event_id().as_uuid())
            .fetch_one(&pool)
            .await
            .expect("rejected command outbox count query should succeed");
    assert_eq!(rejected_outbox_count, 0);

    let second_terminal = load_terminal_command_result(&pool, second_command_id)
        .await
        .expect("rejected terminal result should be readable");
    assert!(matches!(
        second_terminal,
        StoredTerminalCommandResult::Rejected {
            aggregate_version,
            rejection_code,
            rejection_details,
            ..
        } if aggregate_version == AggregateVersion::from_value(1)
            && rejection_code == "conflict.expected_version"
            && rejection_details == json!({
                "actual_version": "1",
                "expected_version": "0"
            })
    ));

    let second_audit = load_command_audit(&pool, second_command_id)
        .await
        .expect("rejected command audit should be readable");
    assert_eq!(second_audit.command_id, second_command_id);
    assert_eq!(second_audit.actor_id, actor_id);
    assert_eq!(second_audit.aggregate_id, match_id);
    assert_eq!(second_audit.action, "RecordGoal");
    assert_eq!(second_audit.outcome, "rejected");
    assert_eq!(second_audit.aggregate_version, 1);
    assert_eq!(second_audit.event_count, 0);
    assert_eq!(second_audit.event_id, None);
    assert_eq!(
        second_audit.rejection_code.as_deref(),
        Some("conflict.expected_version")
    );
    assert_eq!(
        second_audit.details,
        json!({
            "actual_version": "1",
            "expected_version": "0"
        })
    );

    let replayed = orchestrator
        .execute(&second_request, &second_context, &second_command)
        .await
        .expect("same rejected command retry should replay its terminal result");
    assert!(matches!(
        replayed,
        CommandOrchestrationResult::ExistingTerminal {
            result: PersistedCommandResult::Rejected {
                command_id,
                aggregate_version,
                rejection_code,
                ..
            }
        } if command_id == second_command_id
            && aggregate_version == AggregateVersion::from_value(1)
            && rejection_code == "conflict.expected_version"
    ));

    let third_command_id = CommandId::from_uuid(uuid("019fe950-3bc5-7cfc-b3d2-0728c077a930"));
    let third_request = request(
        third_command_id,
        uuid("019fe950-3bc5-7cfd-b3d2-0728c077a930"),
        0x43,
        uuid("019fe950-3bc5-7cfe-b3d2-0728c077a930"),
        2_020,
    );
    let third_command = goal_command(
        third_command_id,
        EventId::from_uuid(uuid("019fe950-3bc5-7cff-b3d2-0728c077a930")),
        match_id,
        home_team_id,
        AggregateVersion::from_value(1),
        2_021,
        2_022,
    );
    let third_context = CommandExecutionContext::from_admission_request(
        third_request,
        actor(),
        Some(AggregateVersion::from_value(1)),
    );
    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(third_command_id.as_uuid())
    .bind(third_request.identity().client_command_id().as_uuid())
    .bind(
        third_request
            .identity()
            .request_fingerprint()
            .as_bytes()
            .to_vec(),
    )
    .execute(&pool)
    .await
    .expect("third command should exist before the rollback fixture");
    query(
        "INSERT INTO command_audit (
             command_id, actor_id, aggregate_id, action, outcome,
             aggregate_version, event_count, rejection_code, details
         )
         VALUES ($1, $2, $3, 'preexisting', 'rejected', 1, 0, 'test.preexisting', $4)",
    )
    .bind(third_command_id.as_uuid())
    .bind(actor_id)
    .bind(match_id.as_uuid())
    .bind(Json(json!({})))
    .execute(&pool)
    .await
    .expect("preexisting audit row should create the duplicate-key rollback fixture");

    let failed_execution = orchestrator
        .execute(&third_request, &third_context, &third_command)
        .await;
    assert!(matches!(
        failed_execution,
        Err(CommandOrchestrationError::Execution(
            C1LiveActionCommandExecutionError::AuditRepository(
                CommandAuditRepositoryError::Database(_)
            )
        ))
    ));

    let event_count_after_audit_failure: i64 =
        query_scalar("SELECT COUNT(*) FROM match_events WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("post-rollback event count query should succeed");
    assert_eq!(event_count_after_audit_failure, 1);
    let third_status: String = query_scalar("SELECT status FROM commands WHERE id = $1")
        .bind(third_command_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("rolled-back command status query should succeed");
    assert_eq!(third_status, "processing");
    let third_audit_count: i64 =
        query_scalar("SELECT COUNT(*) FROM command_audit WHERE command_id = $1")
            .bind(third_command_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("preexisting audit count query should succeed");
    assert_eq!(third_audit_count, 1);
    let third_outbox_count: i64 =
        query_scalar("SELECT COUNT(*) FROM outbox_messages WHERE message_id = $1")
            .bind(third_command.event_id().as_uuid())
            .fetch_one(&pool)
            .await
            .expect("rolled-back outbox count query should succeed");
    assert_eq!(third_outbox_count, 0);

    pool.close().await;
}
