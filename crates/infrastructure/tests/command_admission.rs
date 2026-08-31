use football_application::commands::{
    CommandAdmissionRequest, LogicalCommandIdentity, into_persisted_command_result,
};
use football_domain::{
    AggregateVersion, ClientCommandId, CommandId, CommandResultEnvelope, MatchId,
    RequestFingerprint, UtcInstant,
};
use football_infrastructure::{
    CommandAdmissionError, CommandAdmissionResult, DatabasePoolConfig, MIGRATIONS_DIRECTORY,
    admit_command, connect_database, load_migrator, load_terminal_command_result,
    mark_command_failed_retryable, persist_application_command_result, run_migrations,
    verify_processing_lease,
};
use serde_json::json;
use sqlx::query_scalar;
use std::path::PathBuf;
use time::OffsetDateTime;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixture UUID should be valid")
}

fn command_id(value: &str) -> CommandId {
    CommandId::from_uuid(uuid(value))
}

fn client_command_id(value: &str) -> ClientCommandId {
    ClientCommandId::try_from_uuid(uuid(value)).expect("fixture client command ID should be UUIDv7")
}

fn request(
    command_id_value: &str,
    client_command_id_value: &str,
    fingerprint: [u8; 32],
    owner: &str,
    now_seconds: i64,
    expiry_seconds: i64,
) -> CommandAdmissionRequest {
    let identity = LogicalCommandIdentity::new(
        command_id(command_id_value),
        client_command_id(client_command_id_value),
        RequestFingerprint::from_bytes(fingerprint),
    );
    CommandAdmissionRequest::try_new(
        identity,
        uuid(owner),
        UtcInstant::from_unix_timestamp(now_seconds).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(expiry_seconds).expect("fixture time should be valid"),
    )
    .expect("fixture processing lease should be in the future")
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn admit_command_enforces_idempotency_and_processing_lease_rules() {
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
        .expect("clean admission target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "admission target is not clean: _sqlx_migrations already exists"
    );

    let migrator = load_migrator(repository_migrations_path())
        .await
        .expect("repository migrations should resolve");
    assert!(
        migrator.iter().any(|migration| migration.version == 5),
        "command admission migration should be present"
    );
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let first = request(
        "019fe310-3bc5-7cf0-b3d2-0728c077a930",
        "019fe310-3bc5-7cf1-b3d2-0728c077a930",
        [0x11; 32],
        "019fe310-3bc5-7cf2-b3d2-0728c077a930",
        2_000,
        2_030,
    );
    let first_command_id = first.identity().command_id();

    assert_eq!(
        admit_command(&pool, &first).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: first_command_id,
        }
    );

    let mut execution_transaction = pool
        .begin()
        .await
        .expect("execution lease transaction should begin");
    verify_processing_lease(
        &mut execution_transaction,
        first_command_id,
        first.processing_owner(),
        first.now_utc(),
    )
    .await
    .expect("claimed command lease should be valid inside the execution transaction");
    execution_transaction
        .rollback()
        .await
        .expect("execution lease transaction should roll back without writes");

    let same_active_request = request(
        "019fe310-3bc5-7cf0-b3d2-0728c077a930",
        "019fe310-3bc5-7cf1-b3d2-0728c077a930",
        [0x11; 32],
        "019fe310-3bc5-7cf3-b3d2-0728c077a930",
        2_005,
        2_035,
    );
    assert!(matches!(
        admit_command(&pool, &same_active_request).await,
        Err(CommandAdmissionError::CommandInProgress { command_id })
            if command_id == first_command_id
    ));

    let accepted_completed_at =
        UtcInstant::from_unix_timestamp(2_005).expect("completion time should be valid");
    let accepted_result = into_persisted_command_result(
        CommandResultEnvelope::<_, ()>::accepted(
            first_command_id,
            MatchId::from_uuid(uuid("019fe310-3bc5-7cfb-b3d2-0728c077a930")),
            AggregateVersion::initial(),
            0,
            json!({"score": {"home": 0, "away": 0}}),
        ),
        accepted_completed_at,
        |payload| payload,
        |_: ()| ("domain.invalid_transition".to_owned(), json!({})),
    );
    let mut accepted_transaction = pool
        .begin()
        .await
        .expect("accepted command transaction should begin");
    persist_application_command_result(
        &mut accepted_transaction,
        first.processing_owner(),
        accepted_result,
    )
    .await
    .expect("accepted terminal result should persist");
    accepted_transaction
        .commit()
        .await
        .expect("accepted command transaction should commit");
    let stored_accepted = load_terminal_command_result(&pool, first_command_id)
        .await
        .expect("accepted terminal result should load");
    assert!(matches!(
        stored_accepted,
        football_infrastructure::StoredTerminalCommandResult::Accepted {
            final_aggregate_version,
            event_count: 0,
            result_payload,
            ..
        } if final_aggregate_version == AggregateVersion::initial()
            && result_payload == json!({"score": {"home": 0, "away": 0}})
    ));

    let terminal_retry = request(
        "019fe310-3bc5-7cf0-b3d2-0728c077a930",
        "019fe310-3bc5-7cf1-b3d2-0728c077a930",
        [0x11; 32],
        "019fe310-3bc5-7cf3-b3d2-0728c077a930",
        2_010,
        2_040,
    );
    assert!(matches!(
        admit_command(&pool, &terminal_retry).await.unwrap(),
        CommandAdmissionResult::ExistingTerminal {
            result: football_application::commands::PersistedCommandResult::Accepted {
                command_id,
                result_payload,
                ..
            }
        } if command_id == first_command_id
            && result_payload == json!({"score": {"home": 0, "away": 0}})
    ));

    let rejectable = request(
        "019fe310-3bc5-7cfc-b3d2-0728c077a930",
        "019fe310-3bc5-7cfd-b3d2-0728c077a930",
        [0x44; 32],
        "019fe310-3bc5-7cfe-b3d2-0728c077a930",
        2_000,
        2_030,
    );
    let rejected_command_id = rejectable.identity().command_id();
    assert_eq!(
        admit_command(&pool, &rejectable).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: rejected_command_id,
        }
    );

    let rejected_result = into_persisted_command_result(
        CommandResultEnvelope::<(), football_platform::ErrorCode>::rejected(
            rejected_command_id,
            MatchId::from_uuid(uuid("019fe310-3c05-7cfb-b3d2-0728c077a930")),
            AggregateVersion::initial(),
            football_platform::ErrorCode::DomainInvalidTransition,
        ),
        UtcInstant::from_unix_timestamp(2_005).expect("completion time should be valid"),
        |_: ()| json!({}),
        |error| (error.as_str().to_owned(), json!({"state": "pre_match"})),
    );
    let mut rejected_transaction = pool
        .begin()
        .await
        .expect("rejected command transaction should begin");
    persist_application_command_result(
        &mut rejected_transaction,
        rejectable.processing_owner(),
        rejected_result,
    )
    .await
    .expect("rejected terminal result should persist");
    rejected_transaction
        .commit()
        .await
        .expect("rejected command transaction should commit");
    let stored_rejected = load_terminal_command_result(&pool, rejected_command_id)
        .await
        .expect("rejected terminal result should load");
    assert!(matches!(
        stored_rejected,
        football_infrastructure::StoredTerminalCommandResult::Rejected {
            aggregate_version,
            rejection_code,
            rejection_details,
            ..
        } if aggregate_version == AggregateVersion::initial()
            && rejection_code == "domain.invalid_transition"
            && rejection_details == json!({"state": "pre_match"})
    ));

    let rejected_retry = request(
        "019fe310-3bc5-7cff-b3d2-0728c077a930",
        "019fe310-3bc5-7cfd-b3d2-0728c077a930",
        [0x44; 32],
        "019fe310-3bc5-7cfe-b3d2-0728c077a930",
        2_010,
        2_040,
    );
    assert!(matches!(
        admit_command(&pool, &rejected_retry).await.unwrap(),
        CommandAdmissionResult::ExistingTerminal {
            result: football_application::commands::PersistedCommandResult::Rejected {
                command_id,
                rejection_code,
                rejection_details,
                ..
            }
        } if command_id == rejected_command_id
            && rejection_code == "domain.invalid_transition"
            && rejection_details == json!({"state": "pre_match"})
    ));

    let mismatched_request = request(
        "019fe310-3bc5-7cf4-b3d2-0728c077a930",
        "019fe310-3bc5-7cf1-b3d2-0728c077a930",
        [0x22; 32],
        "019fe310-3bc5-7cf3-b3d2-0728c077a930",
        2_010,
        2_040,
    );
    assert!(matches!(
        admit_command(&pool, &mismatched_request).await,
        Err(CommandAdmissionError::FingerprintMismatch { command_id })
            if command_id == first_command_id
    ));

    let identity_mismatch_request = request(
        "019fe310-3bc5-7d00-b3d2-0728c077a930",
        "019fe310-3bc5-7cf1-b3d2-0728c077a930",
        [0x11; 32],
        "019fe310-3bc5-7d01-b3d2-0728c077a930",
        2_010,
        2_040,
    );
    let identity_mismatch_error = match admit_command(&pool, &identity_mismatch_request).await {
        Err(error) => error,
        other => panic!("stored command identity mismatch should be rejected: {other:?}"),
    };
    assert!(matches!(
        &identity_mismatch_error,
        CommandAdmissionError::CommandIdentityMismatch { expected, actual }
            if *expected == identity_mismatch_request.identity().command_id()
                && *actual == first_command_id
    ));
    let safe_error = identity_mismatch_error.to_application_error();
    assert_eq!(
        safe_error.code(),
        football_platform::ErrorCode::InternalUnexpected
    );
    assert!(safe_error.details().is_empty());

    let expiring = request(
        "019fe310-3bc5-7cf5-b3d2-0728c077a930",
        "019fe310-3bc5-7cf6-b3d2-0728c077a930",
        [0x33; 32],
        "019fe310-3bc5-7cf7-b3d2-0728c077a930",
        2_000,
        2_030,
    );
    let expiring_command_id = expiring.identity().command_id();
    let expiring_client_command_id = expiring.identity().client_command_id();
    assert_eq!(
        admit_command(&pool, &expiring).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: expiring_command_id,
        }
    );

    let expired_at = UtcInstant::from_unix_timestamp(1_999)
        .expect("expired fixture time should be valid")
        .as_offset_datetime();
    let expired_rows =
        sqlx::query("UPDATE commands SET processing_expires_at = $1 WHERE client_command_id = $2")
            .bind(expired_at)
            .bind(expiring_client_command_id.as_uuid())
            .execute(&pool)
            .await
            .expect("expired lease fixture update should succeed");
    assert_eq!(expired_rows.rows_affected(), 1);

    let recovery = request(
        "019fe310-3bc5-7cf8-b3d2-0728c077a930",
        "019fe310-3bc5-7cf6-b3d2-0728c077a930",
        [0x33; 32],
        "019fe310-3bc5-7cf9-b3d2-0728c077a930",
        2_000,
        2_060,
    );
    assert_eq!(
        admit_command(&pool, &recovery).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: expiring_command_id,
        }
    );

    let mut retryable_transaction = pool
        .begin()
        .await
        .expect("retryable command transaction should begin");
    mark_command_failed_retryable(
        &mut retryable_transaction,
        expiring_command_id,
        recovery.processing_owner(),
        UtcInstant::from_unix_timestamp(2_005).expect("failure time should be valid"),
    )
    .await
    .expect("owned command should become retryable");
    retryable_transaction
        .commit()
        .await
        .expect("retryable command transaction should commit");

    let retryable_recovery = request(
        "019fe310-3bc5-7cfa-b3d2-0728c077a930",
        "019fe310-3bc5-7cf6-b3d2-0728c077a930",
        [0x33; 32],
        "019fe310-3bc5-7cfb-b3d2-0728c077a930",
        2_000,
        2_090,
    );
    assert_eq!(
        admit_command(&pool, &retryable_recovery).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: expiring_command_id,
        }
    );

    let (status, owner, processing_expires_at): (String, Uuid, OffsetDateTime) = sqlx::query_as(
        "SELECT status, processing_owner, processing_expires_at
         FROM commands WHERE client_command_id = $1",
    )
    .bind(expiring_client_command_id.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("recovered command row should be readable");
    assert_eq!(status, "processing");
    assert_eq!(owner, retryable_recovery.processing_owner());
    assert_eq!(
        processing_expires_at,
        retryable_recovery
            .processing_expires_at()
            .as_offset_datetime()
    );

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn concurrent_duplicate_admission_allows_only_one_claim() {
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
        .expect("clean admission target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "admission target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let left = request(
        "019fe320-3bc5-7cf0-b3d2-0728c077a930",
        "019fe320-3bc5-7cf1-b3d2-0728c077a930",
        [0x55; 32],
        "019fe320-3bc5-7cf2-b3d2-0728c077a930",
        3_000,
        3_030,
    );
    let right = request(
        "019fe320-3bc5-7cf3-b3d2-0728c077a930",
        "019fe320-3bc5-7cf1-b3d2-0728c077a930",
        [0x55; 32],
        "019fe320-3bc5-7cf4-b3d2-0728c077a930",
        3_000,
        3_030,
    );

    let (left_result, right_result) =
        tokio::join!(admit_command(&pool, &left), admit_command(&pool, &right));

    let mut claimed_command_id = None;
    let mut in_progress_command_id = None;
    for result in [left_result, right_result] {
        match result {
            Ok(CommandAdmissionResult::Claimed { command_id }) => {
                assert!(
                    claimed_command_id.replace(command_id).is_none(),
                    "concurrent duplicate requests must not both claim"
                );
            }
            Err(CommandAdmissionError::CommandInProgress { command_id }) => {
                assert!(
                    in_progress_command_id.replace(command_id).is_none(),
                    "one duplicate request must observe the active claim"
                );
            }
            other => panic!("unexpected concurrent admission result: {other:?}"),
        }
    }

    assert_eq!(claimed_command_id, in_progress_command_id);
    pool.close().await;
}
