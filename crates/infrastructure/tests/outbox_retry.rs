use football_domain::{EventId, UtcInstant};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, OUTBOX_MAX_DELIVERY_ATTEMPTS,
    OUTBOX_POISON_REASON_MAX_ATTEMPTS, OutboxDeliveryStatus, OutboxPublishFailureDisposition,
    OutboxRepositoryError, claim_outbox_messages, connect_database, handle_outbox_publish_failure,
    load_outbox_message, run_migrations,
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
    Uuid::parse_str(value).expect("outbox retry fixture UUID should be valid")
}

fn instant(seconds: i64) -> time::OffsetDateTime {
    UtcInstant::from_unix_timestamp(seconds)
        .expect("outbox retry fixture time should be valid")
        .as_offset_datetime()
}

async fn apply_migrations(pool: &PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "outbox retry target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

async fn insert_outbox_fixture(pool: &PgPool) -> EventId {
    let command_id = uuid("019fed10-3bc5-7cf0-b3d2-0728c077a930");
    let client_command_id = uuid("019fed11-3bc5-7cf0-b3d2-0728c077a930");
    let event_id = EventId::from_uuid(uuid("019fed12-3bc5-7cf0-b3d2-0728c077a930"));
    let match_id = uuid("019fed00-3bc5-7cf0-b3d2-0728c077a930");
    let actor_id = uuid("019fedff-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(client_command_id)
    .bind(vec![0x71_u8; 32])
    .execute(pool)
    .await
    .expect("fixture command should insert");

    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload
         )
         VALUES ($1, $2, 1, $3, 0, 'GoalRecorded', 1, 'first_half', 1000,
                 $4, $5, $6, 'provisional', $7)",
    )
    .bind(event_id.as_uuid())
    .bind(match_id)
    .bind(command_id)
    .bind(instant(1_001))
    .bind(instant(1_002))
    .bind(actor_id)
    .bind(Json(json!({"fixture": "retry"})))
    .execute(pool)
    .await
    .expect("fixture canonical event should insert");

    query(
        "INSERT INTO outbox_messages (
             message_id, aggregate_type, aggregate_id, source_aggregate_version,
             causation_command_id, source_event_id, message_type, schema_version,
             payload, created_at, next_attempt_at
         )
         VALUES ($1, 'match', $2, 1, $3, $1, 'match.event', 1, $4, $5, $6)",
    )
    .bind(event_id.as_uuid())
    .bind(match_id)
    .bind(command_id)
    .bind(Json(json!({"event_id": event_id.to_string()})))
    .bind(instant(900))
    .bind(instant(900))
    .execute(pool)
    .await
    .expect("fixture outbox message should insert");

    event_id
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn outbox_retry_uses_owned_lease_and_deterministic_backoff() {
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
        .expect("clean outbox retry target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'outbox-retry-admin', 'Outbox Retry Admin', TRUE)",
    )
    .bind(uuid("019fedff-3bc5-7cf0-b3d2-0728c077a930"))
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let event_id = insert_outbox_fixture(&pool).await;
    let worker_a = uuid("019fee00-3bc5-7cf0-b3d2-0728c077a930");
    let worker_b = uuid("019fee01-3bc5-7cf0-b3d2-0728c077a930");
    let now = UtcInstant::from_unix_timestamp(2_000).unwrap();

    let mut claim_transaction = pool
        .begin()
        .await
        .expect("initial outbox claim transaction should begin");
    claim_outbox_messages(
        &mut claim_transaction,
        worker_a,
        now,
        UtcInstant::from_unix_timestamp(2_030).unwrap(),
        1,
    )
    .await
    .expect("initial outbox claim should succeed");
    claim_transaction
        .commit()
        .await
        .expect("initial outbox claim should commit");

    let mut wrong_worker_transaction = pool
        .begin()
        .await
        .expect("wrong-worker retry transaction should begin");
    assert!(matches!(
        handle_outbox_publish_failure(
            &mut wrong_worker_transaction,
            event_id,
            worker_b,
            UtcInstant::from_unix_timestamp(2_005).unwrap(),
        )
        .await,
        Err(OutboxRepositoryError::RetryClaimNotOwnedOrExpired { message_id })
            if message_id == event_id
    ));
    wrong_worker_transaction
        .rollback()
        .await
        .expect("wrong-worker retry transaction should roll back");

    let mut retry_transaction = pool
        .begin()
        .await
        .expect("owned retry transaction should begin");
    let schedule = match handle_outbox_publish_failure(
        &mut retry_transaction,
        event_id,
        worker_a,
        UtcInstant::from_unix_timestamp(2_005).unwrap(),
    )
    .await
    .expect("owned active claim should schedule a retry")
    {
        OutboxPublishFailureDisposition::Retry(schedule) => schedule,
        other => panic!("first failure should be retryable, got {other:?}"),
    };
    assert_eq!(schedule.message_id, event_id);
    assert_eq!(schedule.attempt_count, 1);
    assert_eq!(
        schedule.next_attempt_at,
        UtcInstant::from_unix_timestamp(2_006).unwrap()
    );
    retry_transaction
        .commit()
        .await
        .expect("owned retry schedule should commit");

    let pending = load_outbox_message(&pool, event_id)
        .await
        .expect("scheduled outbox message should load");
    assert_eq!(pending.delivery_status, OutboxDeliveryStatus::Pending);
    assert_eq!(pending.attempt_count, 1);
    assert_eq!(
        pending.next_attempt_at,
        UtcInstant::from_unix_timestamp(2_006).unwrap()
    );
    assert_eq!(pending.claim_owner, None);
    assert_eq!(pending.claim_expires_at, None);

    let mut early_claim_transaction = pool
        .begin()
        .await
        .expect("early claim transaction should begin");
    let early_claim = claim_outbox_messages(
        &mut early_claim_transaction,
        worker_b,
        UtcInstant::from_unix_timestamp(2_005).unwrap(),
        UtcInstant::from_unix_timestamp(2_035).unwrap(),
        1,
    )
    .await
    .expect("early claim should be a valid empty query");
    assert!(early_claim.is_empty());
    early_claim_transaction
        .rollback()
        .await
        .expect("early claim transaction should roll back");

    let mut due_claim_transaction = pool
        .begin()
        .await
        .expect("due claim transaction should begin");
    let due_claim = claim_outbox_messages(
        &mut due_claim_transaction,
        worker_b,
        UtcInstant::from_unix_timestamp(2_006).unwrap(),
        UtcInstant::from_unix_timestamp(2_036).unwrap(),
        1,
    )
    .await
    .expect("due retry should be claimable");
    assert_eq!(due_claim.len(), 1);
    assert_eq!(due_claim[0].attempt_count, 2);
    assert_eq!(due_claim[0].claim_owner, Some(worker_b));
    due_claim_transaction
        .commit()
        .await
        .expect("due claim transaction should commit");

    let mut second_retry_transaction = pool
        .begin()
        .await
        .expect("second retry transaction should begin");
    let second_schedule = match handle_outbox_publish_failure(
        &mut second_retry_transaction,
        event_id,
        worker_b,
        UtcInstant::from_unix_timestamp(2_010).unwrap(),
    )
    .await
    .expect("second owned claim should schedule a retry")
    {
        OutboxPublishFailureDisposition::Retry(schedule) => schedule,
        other => panic!("second failure should be retryable, got {other:?}"),
    };
    assert_eq!(second_schedule.attempt_count, 2);
    assert_eq!(
        second_schedule.next_attempt_at,
        UtcInstant::from_unix_timestamp(2_012).unwrap()
    );
    second_retry_transaction
        .commit()
        .await
        .expect("second retry schedule should commit");

    let mut poison_claim_transaction = pool
        .begin()
        .await
        .expect("poison-threshold claim transaction should begin");
    let poison_claim = claim_outbox_messages(
        &mut poison_claim_transaction,
        worker_b,
        UtcInstant::from_unix_timestamp(2_012).unwrap(),
        UtcInstant::from_unix_timestamp(2_042).unwrap(),
        1,
    )
    .await
    .expect("third due retry should be claimable");
    assert_eq!(poison_claim.len(), 1);
    assert_eq!(poison_claim[0].attempt_count, 3);
    poison_claim_transaction
        .commit()
        .await
        .expect("poison-threshold claim should commit");

    query("UPDATE outbox_messages SET attempt_count = $1 WHERE message_id = $2")
        .bind(OUTBOX_MAX_DELIVERY_ATTEMPTS as i32)
        .bind(event_id.as_uuid())
        .execute(&pool)
        .await
        .expect("fixture should move the message to the poison threshold");

    let poison_time = UtcInstant::from_unix_timestamp(2_015).unwrap();
    let mut poison_transaction = pool
        .begin()
        .await
        .expect("poison transition transaction should begin");
    let poisoned = match handle_outbox_publish_failure(
        &mut poison_transaction,
        event_id,
        worker_b,
        poison_time,
    )
    .await
    .expect("threshold failure should become poison")
    {
        OutboxPublishFailureDisposition::Poisoned(poisoned) => poisoned,
        other => panic!("threshold failure should poison the message, got {other:?}"),
    };
    assert_eq!(poisoned.message_id, event_id);
    assert_eq!(poisoned.attempt_count, OUTBOX_MAX_DELIVERY_ATTEMPTS);
    assert_eq!(poisoned.poisoned_at, poison_time);
    assert_eq!(poisoned.poison_reason, OUTBOX_POISON_REASON_MAX_ATTEMPTS);
    poison_transaction
        .commit()
        .await
        .expect("poison transition should commit");

    let poisoned_stored = load_outbox_message(&pool, event_id)
        .await
        .expect("poisoned outbox message should load");
    assert_eq!(
        poisoned_stored.delivery_status,
        OutboxDeliveryStatus::Poisoned
    );
    assert_eq!(poisoned_stored.attempt_count, OUTBOX_MAX_DELIVERY_ATTEMPTS);
    assert_eq!(poisoned_stored.poisoned_at, Some(poison_time));
    assert_eq!(
        poisoned_stored.poison_reason.as_deref(),
        Some(OUTBOX_POISON_REASON_MAX_ATTEMPTS)
    );
    assert_eq!(poisoned_stored.claim_owner, None);
    assert_eq!(poisoned_stored.claim_expires_at, None);

    let mut post_poison_claim_transaction = pool
        .begin()
        .await
        .expect("post-poison claim transaction should begin");
    let post_poison_claim = claim_outbox_messages(
        &mut post_poison_claim_transaction,
        worker_a,
        UtcInstant::from_unix_timestamp(3_000).unwrap(),
        UtcInstant::from_unix_timestamp(3_030).unwrap(),
        1,
    )
    .await
    .expect("post-poison claim query should succeed");
    assert!(post_poison_claim.is_empty());
    post_poison_claim_transaction
        .rollback()
        .await
        .expect("post-poison claim transaction should roll back");

    pool.close().await;
}
