use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, LogicalCommandIdentity,
    into_persisted_command_result,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1LiveAction, C1LiveActionCommand,
    ClientCommandId, ClockMilliseconds, CommandId, CommandResultEnvelope, EventId,
    EventVerificationStatus, GoalAwardType, GoalRecordedPayload, MatchId, MatchPeriod,
    MatchProjection, RequestFingerprint, TeamId, UtcInstant,
};
use football_infrastructure::{
    C1LiveActionExecutionError, CommandAdmissionResult, CommandReconciliationDisposition,
    CommandResultRepositoryError, DatabasePoolConfig, MIGRATIONS_DIRECTORY,
    StoredTerminalCommandResult, admit_command, connect_database,
    execute_c1_live_action_in_transaction, load_c1_event_stream,
    load_c1_event_stream_in_transaction, load_command_reconciliation_snapshot,
    load_terminal_command_result, persist_application_command_result, run_migrations,
};
use football_platform::ErrorCode;
use serde_json::json;
use sqlx::{PgPool, query, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("C1 execution fixture UUID should be valid")
}

fn goal_command(
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    team_id: TeamId,
    actor: ActorIdentity,
) -> C1LiveActionCommand {
    C1LiveActionCommand::try_new(
        command_id,
        event_id,
        match_id,
        AggregateVersion::initial(),
        MatchPeriod::FirstHalf,
        ClockMilliseconds::from_seconds(12).expect("fixture clock should be valid"),
        UtcInstant::from_unix_timestamp(3_001).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(3_002).expect("fixture time should be valid"),
        actor,
        EventVerificationStatus::Provisional,
        C1LiveAction::RecordGoal(GoalRecordedPayload::new(
            team_id,
            None,
            GoalAwardType::Regular,
        )),
    )
    .expect("fixture C1 command should be valid")
}

async fn execute_and_commit_c1_event(
    pool: &PgPool,
    context: CommandExecutionContext,
    command: C1LiveActionCommand,
    now_utc: UtcInstant,
) -> Result<AggregateVersion, C1LiveActionExecutionError> {
    let mut transaction = pool
        .begin()
        .await
        .expect("concurrent C1 execution transaction should begin");
    match execute_c1_live_action_in_transaction(&mut transaction, &context, command, now_utc).await
    {
        Ok(stream) => {
            let version = stream.current_version();
            transaction
                .commit()
                .await
                .expect("successful concurrent C1 execution should commit");
            Ok(version)
        }
        Err(error) => {
            transaction
                .rollback()
                .await
                .expect("failed concurrent C1 execution should roll back");
            Err(error)
        }
    }
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn c1_action_loads_appends_and_persists_within_one_domain_transaction() {
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
        .expect("clean C1 execution target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "C1 execution target is not clean: _sqlx_migrations already exists"
    );
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let actor_id = uuid("019fe710-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("c1-execution-admin")
    .bind("C1 Execution Admin")
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let command_id = CommandId::from_uuid(uuid("019fe710-3bc5-7cf1-b3d2-0728c077a930"));
    let client_command_id =
        ClientCommandId::try_from_uuid(uuid("019fe710-3bc5-7cf2-b3d2-0728c077a930"))
            .expect("fixture client command ID should be UUIDv7");
    let processing_owner = uuid("019fe710-3bc5-7cf3-b3d2-0728c077a930");
    let now = UtcInstant::from_unix_timestamp(2_000).expect("fixture time should be valid");
    let expires_at = UtcInstant::from_unix_timestamp(2_030).expect("fixture time should be valid");
    let request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            client_command_id,
            RequestFingerprint::from_bytes([0x31; 32]),
        ),
        processing_owner,
        now,
        expires_at,
    )
    .expect("fixture processing lease should be valid");
    admit_command(&pool, &request)
        .await
        .expect("C1 command should be admitted");

    let match_id = MatchId::from_uuid(uuid("019fe710-3bc5-7cf4-b3d2-0728c077a930"));
    let home_team_id = TeamId::from_uuid(uuid("019fe710-3bc5-7cf5-b3d2-0728c077a930"));
    let actor = ActorIdentity::administrator(AdminUserId::from_uuid(actor_id));
    let command = C1LiveActionCommand::try_new(
        command_id,
        EventId::from_uuid(uuid("019fe710-3bc5-7cf6-b3d2-0728c077a930")),
        match_id,
        AggregateVersion::initial(),
        MatchPeriod::FirstHalf,
        ClockMilliseconds::from_seconds(12).expect("fixture clock should be valid"),
        UtcInstant::from_unix_timestamp(2_001).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(2_002).expect("fixture time should be valid"),
        actor,
        EventVerificationStatus::Provisional,
        C1LiveAction::RecordGoal(GoalRecordedPayload::new(
            home_team_id,
            None,
            GoalAwardType::Regular,
        )),
    )
    .expect("fixture C1 command should be valid");
    let context = CommandExecutionContext::from_admission_request(
        request,
        actor,
        Some(AggregateVersion::initial()),
    );

    let mut transaction = pool
        .begin()
        .await
        .expect("C1 domain transaction should begin");
    let appended = execute_c1_live_action_in_transaction(
        &mut transaction,
        &context,
        command,
        UtcInstant::from_unix_timestamp(2_005).expect("fixture time should be valid"),
    )
    .await
    .expect("C1 action should append inside the domain transaction");
    assert_eq!(appended.len(), 1);
    assert_eq!(appended.current_version(), AggregateVersion::from_value(1));

    let loaded_before_commit = load_c1_event_stream_in_transaction(&mut transaction, match_id)
        .await
        .expect("C1 event should be readable before the transaction commits");
    assert_eq!(loaded_before_commit.len(), 1);
    assert_eq!(
        loaded_before_commit.events()[0].event_id(),
        appended.events()[0].event_id()
    );

    let persisted_result = into_persisted_command_result(
        CommandResultEnvelope::<_, ()>::accepted(
            command_id,
            match_id,
            appended.current_version(),
            1,
            json!({"score": {"home": 1, "away": 0}}),
        ),
        UtcInstant::from_unix_timestamp(2_005).expect("fixture time should be valid"),
        |payload| payload,
        |_: ()| ("domain.invalid_transition".to_owned(), json!({})),
    );
    let mismatched_result = into_persisted_command_result(
        CommandResultEnvelope::<_, ()>::accepted(
            command_id,
            match_id,
            AggregateVersion::initial(),
            0,
            json!({"score": {"home": 0, "away": 0}}),
        ),
        UtcInstant::from_unix_timestamp(2_005).expect("fixture time should be valid"),
        |payload| payload,
        |_: ()| ("domain.invalid_transition".to_owned(), json!({})),
    );
    assert!(matches!(
        persist_application_command_result(&mut transaction, processing_owner, mismatched_result)
            .await,
        Err(CommandResultRepositoryError::TerminalResultEventMismatch { command_id: actual })
            if actual == command_id
    ));
    let status_before_valid_result: String =
        query_scalar("SELECT status FROM commands WHERE id = $1")
            .bind(command_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .expect("mismatched terminal result must leave the command row readable");
    assert_eq!(status_before_valid_result, "processing");
    persist_application_command_result(&mut transaction, processing_owner, persisted_result)
        .await
        .expect("terminal result should persist in the same transaction");
    transaction
        .commit()
        .await
        .expect("C1 domain transaction should commit");

    let loaded_after_commit = load_c1_event_stream(&pool, match_id)
        .await
        .expect("C1 event should remain readable after commit");
    assert_eq!(loaded_after_commit.len(), 1);
    assert_eq!(
        loaded_after_commit.events()[0].payload(),
        appended.events()[0].payload()
    );
    let away_team_id = TeamId::from_uuid(uuid("019fe710-3bc5-7cf7-b3d2-0728c077a930"));
    let replayed = MatchProjection::from_stream(&loaded_after_commit, home_team_id, away_team_id)
        .expect("committed C1 stream should replay into the synchronous projection");
    assert_eq!(replayed.match_id(), match_id);
    assert_eq!(
        replayed.current_aggregate_version(),
        appended.current_version()
    );
    assert_eq!(replayed.score().home_goals(), 1);
    assert_eq!(replayed.score().away_goals(), 0);
    assert_eq!(replayed.timeline().len(), 1);
    assert_eq!(
        replayed.timeline()[0].event_id(),
        appended.events()[0].event_id()
    );

    let stored_terminal_result = load_terminal_command_result(&pool, command_id)
        .await
        .expect("committed terminal result should be readable after commit");
    assert!(matches!(
        stored_terminal_result,
        StoredTerminalCommandResult::Accepted {
            command_id: stored_command_id,
            aggregate_id,
            final_aggregate_version,
            event_count: 1,
            result_payload,
            ..
        } if stored_command_id == command_id
            && aggregate_id == match_id
            && final_aggregate_version == appended.current_version()
            && result_payload == json!({"score": {"home": 1, "away": 0}})
    ));

    let reconciliation = load_command_reconciliation_snapshot(&pool, command_id)
        .await
        .expect("committed command should be reconcilable");
    assert_eq!(
        reconciliation.disposition(),
        CommandReconciliationDisposition::Accepted
    );
    assert_eq!(reconciliation.canonical_events().len(), 1);

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn concurrent_c1_commands_with_one_expected_version_persist_only_one_event() {
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
        .expect("clean concurrent C1 target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "concurrent C1 target is not clean: _sqlx_migrations already exists"
    );
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let actor_id = uuid("019fe720-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("concurrent-c1-admin")
    .bind("Concurrent C1 Admin")
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let actor = ActorIdentity::administrator(AdminUserId::from_uuid(actor_id));
    let match_id = MatchId::from_uuid(uuid("019fe720-3bc5-7cf9-b3d2-0728c077a930"));
    let home_team_id = TeamId::from_uuid(uuid("019fe720-3bc5-7cfa-b3d2-0728c077a930"));

    let left_request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            CommandId::from_uuid(uuid("019fe720-3bc5-7cf1-b3d2-0728c077a930")),
            ClientCommandId::try_from_uuid(uuid("019fe720-3bc5-7cf3-b3d2-0728c077a930"))
                .expect("left client command ID should be UUIDv7"),
            RequestFingerprint::from_bytes([0x61; 32]),
        ),
        uuid("019fe720-3bc5-7cf5-b3d2-0728c077a930"),
        UtcInstant::from_unix_timestamp(3_000).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(3_030).expect("fixture time should be valid"),
    )
    .expect("left processing lease should be valid");
    let right_request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            CommandId::from_uuid(uuid("019fe720-3bc5-7cf2-b3d2-0728c077a930")),
            ClientCommandId::try_from_uuid(uuid("019fe720-3bc5-7cf4-b3d2-0728c077a930"))
                .expect("right client command ID should be UUIDv7"),
            RequestFingerprint::from_bytes([0x62; 32]),
        ),
        uuid("019fe720-3bc5-7cf6-b3d2-0728c077a930"),
        UtcInstant::from_unix_timestamp(3_000).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(3_030).expect("fixture time should be valid"),
    )
    .expect("right processing lease should be valid");

    for request in [&left_request, &right_request] {
        assert!(matches!(
            admit_command(&pool, request).await,
            Ok(CommandAdmissionResult::Claimed { .. })
        ));
    }

    let left_command = goal_command(
        left_request.identity().command_id(),
        EventId::from_uuid(uuid("019fe720-3bc5-7cf7-b3d2-0728c077a930")),
        match_id,
        home_team_id,
        actor,
    );
    let right_command = goal_command(
        right_request.identity().command_id(),
        EventId::from_uuid(uuid("019fe720-3bc5-7cf8-b3d2-0728c077a930")),
        match_id,
        home_team_id,
        actor,
    );
    let left_event_id = left_command.event_id();
    let right_event_id = right_command.event_id();
    let left_context = CommandExecutionContext::from_admission_request(
        left_request,
        actor,
        Some(AggregateVersion::initial()),
    );
    let right_context = CommandExecutionContext::from_admission_request(
        right_request,
        actor,
        Some(AggregateVersion::initial()),
    );
    let now_utc = UtcInstant::from_unix_timestamp(3_005).expect("fixture time should be valid");

    let (left_result, right_result) = tokio::join!(
        execute_and_commit_c1_event(&pool, left_context, left_command, now_utc),
        execute_and_commit_c1_event(&pool, right_context, right_command, now_utc),
    );
    let successful_results = [&left_result, &right_result]
        .into_iter()
        .filter(|result| result.is_ok())
        .count();
    assert_eq!(
        successful_results, 1,
        "exactly one command may append version 1"
    );

    for result in [&left_result, &right_result] {
        match result {
            Ok(version) => assert_eq!(*version, AggregateVersion::from_value(1)),
            Err(error) => assert_eq!(
                error.to_application_error().code(),
                ErrorCode::ConflictExpectedVersion
            ),
        }
    }

    let stream = load_c1_event_stream(&pool, match_id)
        .await
        .expect("the concurrent C1 stream should remain readable");
    assert_eq!(stream.len(), 1);
    assert_eq!(stream.current_version(), AggregateVersion::from_value(1));
    let persisted_event_id = stream.events()[0].event_id();
    assert!(persisted_event_id == left_event_id || persisted_event_id == right_event_id);

    let persisted_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_events WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("persisted event count query should succeed");
    assert_eq!(persisted_count, 1);

    pool.close().await;
}
