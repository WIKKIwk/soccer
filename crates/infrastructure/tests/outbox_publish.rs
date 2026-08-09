use football_domain::{EventId, UtcInstant};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, OutboxDeliveryStatus, OutboxRepositoryError,
    claim_outbox_messages, connect_database, load_outbox_message, mark_outbox_published,
    run_migrations,
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
    Uuid::parse_str(value).expect("outbox publish fixture UUID should be valid")
}

fn instant(seconds: i64) -> time::OffsetDateTime {
    UtcInstant::from_unix_timestamp(seconds)
        .expect("outbox publish fixture time should be valid")
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
        "outbox publish target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

async fn insert_admin(pool: &PgPool) {
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'outbox-publish-admin', 'Outbox Publish Admin', TRUE)",
    )
    .bind(uuid("019ff0ff-3bc5-7cf0-b3d2-0728c077a930"))
    .execute(pool)
    .await
    .expect("fixture administrator should insert");
}

async fn insert_outbox_fixture(pool: &PgPool) -> EventId {
    let command_id = uuid("019ff020-3bc5-7cf0-b3d2-0728c077a930");
    let client_command_id = uuid("019ff021-3bc5-7cf0-b3d2-0728c077a930");
    let event_id = EventId::from_uuid(uuid("019ff022-3bc5-7cf0-b3d2-0728c077a930"));
    let match_id = uuid("019ff002-3bc5-7cf0-b3d2-0728c077a930");
    let actor_id = uuid("019ff0ff-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(client_command_id)
    .bind(vec![0x81_u8; 32])
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
    .bind(Json(json!({"fixture": "publish"})))
    .execute(pool)
    .await
    .expect("fixture canonical event should insert");

    query(
        "INSERT INTO outbox_messages (
             message_id, aggregate_type, aggregate_id, source_aggregate_version,
             causation_command_id, source_event_id, message_type, schema_version,
             payload, created_at, next_attempt_at
         )
         VALUES ($1, 'match', $2, 1, $3, $1, 'match.event', 1, $4, $5, $5)",
    )
    .bind(event_id.as_uuid())
    .bind(match_id)
    .bind(command_id)
    .bind(Json(json!({"event_id": event_id.to_string()})))
    .bind(instant(900))
    .execute(pool)
    .await
    .expect("fixture outbox message should insert");

    event_id
}

async fn rollback_outbox_fixture(pool: &PgPool) {
    let command_id = uuid("019ff030-3bc5-7cf0-b3d2-0728c077a930");
    let client_command_id = uuid("019ff031-3bc5-7cf0-b3d2-0728c077a930");
    let event_id = uuid("019ff032-3bc5-7cf0-b3d2-0728c077a930");
    let match_id = uuid("019ff003-3bc5-7cf0-b3d2-0728c077a930");
    let actor_id = uuid("019ff0ff-3bc5-7cf0-b3d2-0728c077a930");

    let mut transaction = pool
        .begin()
        .await
        .expect("rollback fixture transaction should begin");
    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(client_command_id)
    .bind(vec![0x82_u8; 32])
    .execute(&mut *transaction)
    .await
    .expect("rollback fixture command should insert");

    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload
         )
         VALUES ($1, $2, 1, $3, 0, 'GoalRecorded', 1, 'first_half', 1000,
                 $4, $5, $6, 'provisional', $7)",
    )
    .bind(event_id)
    .bind(match_id)
    .bind(command_id)
    .bind(instant(1_101))
    .bind(instant(1_102))
    .bind(actor_id)
    .bind(Json(json!({"fixture": "rollback"})))
    .execute(&mut *transaction)
    .await
    .expect("rollback fixture canonical event should insert");

    query(
        "INSERT INTO outbox_messages (
             message_id, aggregate_type, aggregate_id, source_aggregate_version,
             causation_command_id, source_event_id, message_type, schema_version,
             payload, created_at, next_attempt_at
         )
         VALUES ($1, 'match', $2, 1, $3, $1, 'match.event', 1, $4, $5, $5)",
    )
    .bind(event_id)
    .bind(match_id)
    .bind(command_id)
    .bind(Json(json!({"event_id": event_id.to_string()})))
    .bind(instant(1_090))
    .execute(&mut *transaction)
    .await
    .expect("rollback fixture outbox message should insert");

    transaction
        .rollback()
        .await
        .expect("rollback fixture transaction should roll back");

    let command_count: i64 = query_scalar("SELECT COUNT(*) FROM commands WHERE id = $1")
        .bind(command_id)
        .fetch_one(pool)
        .await
        .expect("rolled-back command count query should succeed");
    let event_count: i64 = query_scalar("SELECT COUNT(*) FROM match_events WHERE event_id = $1")
        .bind(event_id)
        .fetch_one(pool)
        .await
        .expect("rolled-back event count query should succeed");
    let outbox_count: i64 =
        query_scalar("SELECT COUNT(*) FROM outbox_messages WHERE message_id = $1")
            .bind(event_id)
            .fetch_one(pool)
            .await
            .expect("rolled-back outbox count query should succeed");
    assert_eq!(command_count, 0, "rolled-back command row must not remain");
    assert_eq!(event_count, 0, "rolled-back event row must not remain");
    assert_eq!(outbox_count, 0, "rolled-back outbox row must not remain");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn outbox_commit_rollback_and_publish_completion_are_lease_safe() {
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
        .expect("clean outbox publish target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;
    insert_admin(&pool).await;

    rollback_outbox_fixture(&pool).await;
    let event_id = insert_outbox_fixture(&pool).await;
    let worker_a = uuid("019ff040-3bc5-7cf0-b3d2-0728c077a930");
    let worker_b = uuid("019ff041-3bc5-7cf0-b3d2-0728c077a930");

    let claim_time = UtcInstant::from_unix_timestamp(2_000).unwrap();
    let first_expiry = UtcInstant::from_unix_timestamp(2_030).unwrap();
    let mut claim_transaction = pool
        .begin()
        .await
        .expect("initial publish claim transaction should begin");
    let claimed = claim_outbox_messages(
        &mut claim_transaction,
        worker_a,
        claim_time,
        first_expiry,
        1,
    )
    .await
    .expect("initial publish claim should succeed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].source_event_id.as_uuid(), event_id.as_uuid());
    assert_eq!(claimed[0].delivery_status, OutboxDeliveryStatus::Processing);
    assert_eq!(claimed[0].attempt_count, 1);
    assert_eq!(claimed[0].claim_owner, Some(worker_a));
    assert_eq!(claimed[0].claim_expires_at, Some(first_expiry));
    claim_transaction
        .commit()
        .await
        .expect("initial publish claim should commit");

    let mut wrong_worker_transaction = pool
        .begin()
        .await
        .expect("wrong-worker publish transaction should begin");
    assert!(matches!(
        mark_outbox_published(
            &mut wrong_worker_transaction,
            event_id,
            worker_b,
            UtcInstant::from_unix_timestamp(2_005).unwrap(),
        )
        .await,
        Err(OutboxRepositoryError::PublishClaimNotOwnedOrExpired { message_id })
            if message_id == event_id
    ));
    wrong_worker_transaction
        .rollback()
        .await
        .expect("wrong-worker publish transaction should roll back");

    query("UPDATE outbox_messages SET claim_expires_at = $1 WHERE message_id = $2")
        .bind(instant(1_999))
        .bind(event_id.as_uuid())
        .execute(&pool)
        .await
        .expect("fixture claim should become expired");

    let mut expired_worker_transaction = pool
        .begin()
        .await
        .expect("expired-worker publish transaction should begin");
    assert!(matches!(
        mark_outbox_published(
            &mut expired_worker_transaction,
            event_id,
            worker_a,
            UtcInstant::from_unix_timestamp(2_000).unwrap(),
        )
        .await,
        Err(OutboxRepositoryError::PublishClaimNotOwnedOrExpired { message_id })
            if message_id == event_id
    ));
    expired_worker_transaction
        .rollback()
        .await
        .expect("expired-worker publish transaction should roll back");

    let second_expiry = UtcInstant::from_unix_timestamp(2_040).unwrap();
    let mut reclaim_transaction = pool
        .begin()
        .await
        .expect("reclaim transaction should begin");
    let reclaimed = claim_outbox_messages(
        &mut reclaim_transaction,
        worker_b,
        claim_time,
        second_expiry,
        1,
    )
    .await
    .expect("another worker should reclaim the expired publish claim");
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].attempt_count, 2);
    assert_eq!(reclaimed[0].claim_owner, Some(worker_b));
    assert_eq!(reclaimed[0].claim_expires_at, Some(second_expiry));
    reclaim_transaction
        .commit()
        .await
        .expect("reclaimed publish claim should commit");

    let published_at = UtcInstant::from_unix_timestamp(2_010).unwrap();
    let mut publish_transaction = pool
        .begin()
        .await
        .expect("publish completion transaction should begin");
    let published =
        mark_outbox_published(&mut publish_transaction, event_id, worker_b, published_at)
            .await
            .expect("active claim owner should complete publish");
    assert_eq!(published.message_id, event_id);
    assert_eq!(published.attempt_count, 2);
    assert_eq!(published.published_at, published_at);
    publish_transaction
        .rollback()
        .await
        .expect("rolled-back publish completion should roll back");

    let still_processing = load_outbox_message(&pool, event_id)
        .await
        .expect("rolled-back publish completion should leave the claim readable");
    assert_eq!(
        still_processing.delivery_status,
        OutboxDeliveryStatus::Processing
    );
    assert_eq!(still_processing.attempt_count, 2);
    assert_eq!(still_processing.claim_owner, Some(worker_b));
    assert_eq!(still_processing.claim_expires_at, Some(second_expiry));
    assert_eq!(still_processing.published_at, None);

    let mut committed_publish_transaction = pool
        .begin()
        .await
        .expect("committed publish completion transaction should begin");
    let committed_published = mark_outbox_published(
        &mut committed_publish_transaction,
        event_id,
        worker_b,
        published_at,
    )
    .await
    .expect("active claim owner should complete publish again");
    assert_eq!(committed_published, published);
    committed_publish_transaction
        .commit()
        .await
        .expect("publish completion should commit");

    let stored = load_outbox_message(&pool, event_id)
        .await
        .expect("published outbox message should load");
    assert_eq!(stored.delivery_status, OutboxDeliveryStatus::Published);
    assert_eq!(stored.attempt_count, 2);
    assert_eq!(stored.claim_owner, None);
    assert_eq!(stored.claim_expires_at, None);
    assert_eq!(stored.poisoned_at, None);
    assert_eq!(stored.poison_reason, None);
    assert_eq!(stored.published_at, Some(published_at));

    pool.close().await;
}
