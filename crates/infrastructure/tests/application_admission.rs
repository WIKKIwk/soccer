use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, CommandExecutionPort,
    CommandOrchestrationError, CommandOrchestrationResult, CommandOrchestrator,
    LogicalCommandIdentity, PersistedCommandResult, into_persisted_command_result,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, CommandId, CommandResultEnvelope, MatchId,
    RequestFingerprint, UtcInstant,
};
use football_infrastructure::{
    CommandAdmissionError, CommandAdmissionResult, DatabasePoolConfig, PostgresCommandAdmission,
    admit_command, connect_database, persist_application_command_result, run_migrations,
};
use football_platform::ErrorCode;
use serde_json::json;
use sqlx::{PgPool, query_scalar};
use std::future::{Future, ready};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(football_infrastructure::MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("application admission fixture UUID should be valid")
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
            football_domain::ClientCommandId::try_from_uuid(client_command_id)
                .expect("fixture client command ID should be UUIDv7"),
            RequestFingerprint::from_bytes([fingerprint; 32]),
        ),
        processing_owner,
        UtcInstant::from_unix_timestamp(now_seconds).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(now_seconds + 30)
            .expect("fixture lease expiry should be valid"),
    )
    .expect("fixture admission request should be valid")
}

fn actor() -> ActorIdentity {
    ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
        "019fe410-3bc5-7cf0-b3d2-0728c077a930",
    )))
}

#[derive(Debug, Clone, Copy)]
struct FixtureCommand;

#[derive(Debug, Clone)]
struct CountingExecution {
    calls: Arc<AtomicUsize>,
    match_id: MatchId,
}

impl CommandExecutionPort<FixtureCommand> for CountingExecution {
    type Result = &'static str;
    type DomainError = &'static str;
    type Error = &'static str;

    fn execute_and_commit<'a>(
        &'a self,
        context: &'a CommandExecutionContext,
        _command: &'a FixtureCommand,
    ) -> impl Future<
        Output = Result<CommandResultEnvelope<Self::Result, Self::DomainError>, Self::Error>,
    > + Send
    + 'a {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ready(Ok(CommandResultEnvelope::accepted(
            context.command_id(),
            self.match_id,
            AggregateVersion::initial(),
            0,
            "executed",
        )))
    }
}

async fn apply_migrations(pool: &PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "application admission target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn application_orchestrator_preserves_fingerprint_conflict_and_skips_execution() {
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
        .expect("clean application admission target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let first_command_id = CommandId::from_uuid(uuid("019fe410-3bc5-7cf1-b3d2-0728c077a930"));
    let shared_client_command_id = uuid("019fe410-3bc5-7cf2-b3d2-0728c077a930");
    let first_request = request(
        first_command_id,
        shared_client_command_id,
        0x11,
        uuid("019fe410-3bc5-7cf3-b3d2-0728c077a930"),
        5_000,
    );
    let mismatch_request = request(
        CommandId::from_uuid(uuid("019fe410-3bc5-7cf4-b3d2-0728c077a930")),
        shared_client_command_id,
        0x22,
        uuid("019fe410-3bc5-7cf5-b3d2-0728c077a930"),
        5_005,
    );
    let match_id = MatchId::from_uuid(uuid("019fe410-3bc5-7cf6-b3d2-0728c077a930"));
    let calls = Arc::new(AtomicUsize::new(0));
    let orchestrator = CommandOrchestrator::new(
        PostgresCommandAdmission::new(&pool),
        CountingExecution {
            calls: Arc::clone(&calls),
            match_id,
        },
    );
    let command = FixtureCommand;
    let first_context =
        CommandExecutionContext::from_admission_request(first_request, actor(), None);

    let first_result = orchestrator
        .execute(&first_request, &first_context, &command)
        .await
        .expect("the first application command should be admitted and executed");
    assert!(matches!(
        first_result,
        CommandOrchestrationResult::Executed(result)
            if result.command_id() == first_command_id
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let mismatch_context =
        CommandExecutionContext::from_admission_request(mismatch_request, actor(), None);
    let mismatch_result = orchestrator
        .execute(&mismatch_request, &mismatch_context, &command)
        .await;
    let admission_error = match mismatch_result {
        Err(CommandOrchestrationError::Admission(error)) => error,
        other => panic!("fingerprint mismatch should stop at admission: {other:?}"),
    };
    assert!(matches!(
        admission_error,
        CommandAdmissionError::FingerprintMismatch { command_id }
            if command_id == first_command_id
    ));

    let application_error = admission_error.to_application_error();
    assert_eq!(
        application_error.code(),
        ErrorCode::IdempotencyFingerprintMismatch
    );
    assert!(!application_error.retryable());
    assert_eq!(
        application_error.details().get("command_id"),
        Some(&first_command_id.to_string())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let command_count: i64 = query_scalar("SELECT COUNT(*) FROM commands")
        .fetch_one(&pool)
        .await
        .expect("command count query should succeed");
    assert_eq!(command_count, 1);

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn application_orchestrator_returns_existing_terminal_without_execution() {
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
        .expect("clean terminal replay target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let command_id = CommandId::from_uuid(uuid("019fe420-3bc5-7cf1-b3d2-0728c077a930"));
    let client_command_id = uuid("019fe420-3bc5-7cf2-b3d2-0728c077a930");
    let match_id = MatchId::from_uuid(uuid("019fe420-3bc5-7cf3-b3d2-0728c077a930"));
    let first_request = request(
        command_id,
        client_command_id,
        0x31,
        uuid("019fe420-3bc5-7cf4-b3d2-0728c077a930"),
        6_000,
    );
    assert!(matches!(
        admit_command(&pool, &first_request).await,
        Ok(CommandAdmissionResult::Claimed { command_id: actual }) if actual == command_id
    ));

    let persisted_result = into_persisted_command_result(
        CommandResultEnvelope::<_, ()>::accepted(
            command_id,
            match_id,
            AggregateVersion::initial(),
            0,
            json!({"score": {"home": 0, "away": 0}}),
        ),
        UtcInstant::from_unix_timestamp(6_005).expect("fixture completion time should be valid"),
        |payload| payload,
        |_: ()| ("domain.invalid_transition".to_owned(), json!({})),
    );
    let mut transaction = pool
        .begin()
        .await
        .expect("terminal replay transaction should begin");
    persist_application_command_result(
        &mut transaction,
        first_request.processing_owner(),
        persisted_result,
    )
    .await
    .expect("terminal result should persist before replay");
    transaction
        .commit()
        .await
        .expect("terminal replay transaction should commit");

    let retry_request = request(
        command_id,
        client_command_id,
        0x31,
        uuid("019fe420-3bc5-7cf5-b3d2-0728c077a930"),
        6_010,
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let orchestrator = CommandOrchestrator::new(
        PostgresCommandAdmission::new(&pool),
        CountingExecution {
            calls: Arc::clone(&calls),
            match_id,
        },
    );
    let retry_context =
        CommandExecutionContext::from_admission_request(retry_request, actor(), None);
    let replayed = orchestrator
        .execute(&retry_request, &retry_context, &FixtureCommand)
        .await
        .expect("same command retry should return its terminal result");
    assert!(matches!(
        replayed,
        CommandOrchestrationResult::ExistingTerminal {
            result: PersistedCommandResult::Accepted {
                command_id: actual,
                aggregate_id,
                event_count: 0,
                result_payload,
                ..
            }
        } if actual == command_id
            && aggregate_id == match_id
            && result_payload == json!({"score": {"home": 0, "away": 0}})
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let command_count: i64 = query_scalar("SELECT COUNT(*) FROM commands")
        .fetch_one(&pool)
        .await
        .expect("command count query should succeed");
    assert_eq!(command_count, 1);

    pool.close().await;
}
