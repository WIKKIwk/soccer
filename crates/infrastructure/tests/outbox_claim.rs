use football_domain::UtcInstant;
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, OutboxDeliveryStatus, claim_outbox_messages,
    connect_database, load_outbox_message, run_migrations,
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
    Uuid::parse_str(value).expect("outbox claim fixture UUID should be valid")
}

fn instant(seconds: i64) -> time::OffsetDateTime {
    UtcInstant::from_unix_timestamp(seconds)
        .expect("outbox claim fixture time should be valid")
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
        "outbox claim target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

async fn insert_outbox_fixture(pool: &PgPool, index: i64) -> Uuid {
    let command_id = uuid(&format!("019feb{}0-3bc5-7cf0-b3d2-0728c077a930", index));
    let client_command_id = uuid(&format!("019feb{}1-3bc5-7cf0-b3d2-0728c077a930", index));
    let event_id = uuid(&format!("019feb{}2-3bc5-7cf0-b3d2-0728c077a930", index));
    let match_id = uuid("019feb00-3bc5-7cf0-b3d2-0728c077a930");
    let actor_id = uuid("019febff-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(client_command_id)
    .bind(vec![index as u8; 32])
    .execute(pool)
    .await
    .expect("fixture command should insert");

    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload
         )
         VALUES ($1, $2, $3, $4, $5, 'GoalRecorded', 1, 'first_half', 1000,
                 $6, $7, $8, 'provisional', $9)",
    )
    .bind(event_id)
    .bind(match_id)
    .bind(index)
    .bind(command_id)
    .bind(index - 1)
    .bind(instant(1_000 + index))
    .bind(instant(1_001 + index))
    .bind(actor_id)
    .bind(Json(json!({"fixture_index": index})))
    .execute(pool)
    .await
    .expect("fixture canonical event should insert");

    query(
        "INSERT INTO outbox_messages (
             message_id, aggregate_type, aggregate_id, source_aggregate_version,
             causation_command_id, source_event_id, message_type, schema_version,
             payload, created_at, next_attempt_at
         )
         VALUES ($1, 'match', $2, $3, $4, $5, 'match.event', 1, $6, $7, $7)",
    )
    .bind(event_id)
    .bind(match_id)
    .bind(index)
    .bind(command_id)
    .bind(event_id)
    .bind(Json(
        json!({"event_id": event_id.to_string(), "fixture_index": index}),
    ))
    .bind(instant(900 + index))
    .execute(pool)
    .await
    .expect("fixture outbox message should insert");

    event_id
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn outbox_claim_uses_skip_locked_and_reclaims_only_expired_leases() {
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
        .expect("clean outbox claim target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'outbox-claim-admin', 'Outbox Claim Admin', TRUE)",
    )
    .bind(uuid("019febff-3bc5-7cf0-b3d2-0728c077a930"))
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let event_ids = [
        insert_outbox_fixture(&pool, 1).await,
        insert_outbox_fixture(&pool, 2).await,
        insert_outbox_fixture(&pool, 3).await,
    ];
    let worker_a = uuid("019fec00-3bc5-7cf0-b3d2-0728c077a930");
    let worker_b = uuid("019fec01-3bc5-7cf0-b3d2-0728c077a930");
    let worker_c = uuid("019fec02-3bc5-7cf0-b3d2-0728c077a930");

    let now = UtcInstant::from_unix_timestamp(2_000).unwrap();
    let expires = UtcInstant::from_unix_timestamp(2_030).unwrap();
    let mut transaction_a = pool
        .begin()
        .await
        .expect("worker A transaction should begin");
    let claimed_a = claim_outbox_messages(&mut transaction_a, worker_a, now, expires, 2)
        .await
        .expect("worker A should claim two messages");
    assert_eq!(
        claimed_a
            .iter()
            .map(|message| message.source_aggregate_version)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(claimed_a.iter().all(|message| {
        message.delivery_status == OutboxDeliveryStatus::Processing
            && message.attempt_count == 1
            && message.claim_owner == Some(worker_a)
            && message.claim_expires_at == Some(expires)
    }));

    let mut transaction_b = pool
        .begin()
        .await
        .expect("worker B transaction should begin");
    let claimed_b = claim_outbox_messages(&mut transaction_b, worker_b, now, expires, 3)
        .await
        .expect("worker B should skip worker A locks and claim the remaining message");
    assert_eq!(claimed_b.len(), 1);
    assert_eq!(claimed_b[0].source_event_id.as_uuid(), event_ids[2]);
    assert_eq!(claimed_b[0].claim_owner, Some(worker_b));
    transaction_b
        .commit()
        .await
        .expect("worker B claim should commit");
    transaction_a
        .commit()
        .await
        .expect("worker A claim should commit");

    query("UPDATE outbox_messages SET claim_expires_at = $1 WHERE message_id = $2")
        .bind(instant(1_999))
        .bind(event_ids[0])
        .execute(&pool)
        .await
        .expect("worker A claim should become expired");

    let mut transaction_c = pool
        .begin()
        .await
        .expect("worker C transaction should begin");
    let reclaimed = claim_outbox_messages(
        &mut transaction_c,
        worker_c,
        now,
        UtcInstant::from_unix_timestamp(2_040).unwrap(),
        1,
    )
    .await
    .expect("worker C should reclaim the expired message");
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].source_event_id.as_uuid(), event_ids[0]);
    assert_eq!(reclaimed[0].attempt_count, 2);
    assert_eq!(reclaimed[0].claim_owner, Some(worker_c));
    transaction_c
        .commit()
        .await
        .expect("worker C reclaim should commit");

    query(
        "UPDATE outbox_messages
         SET delivery_status = 'published',
             claim_owner = NULL,
             claim_expires_at = NULL,
             published_at = CURRENT_TIMESTAMP",
    )
    .execute(&pool)
    .await
    .expect("fixture messages should become published");
    let mut transaction_d = pool
        .begin()
        .await
        .expect("worker D transaction should begin");
    let claimed_published = claim_outbox_messages(
        &mut transaction_d,
        uuid("019fec03-3bc5-7cf0-b3d2-0728c077a930"),
        UtcInstant::from_unix_timestamp(3_000).unwrap(),
        UtcInstant::from_unix_timestamp(3_030).unwrap(),
        3,
    )
    .await
    .expect("published messages should be queryable without claim errors");
    assert!(claimed_published.is_empty());
    transaction_d
        .rollback()
        .await
        .expect("worker D read-only claim transaction should roll back");

    let stored = load_outbox_message(&pool, event_ids[0].into())
        .await
        .expect("reclaimed outbox message should remain readable");
    assert_eq!(stored.delivery_status, OutboxDeliveryStatus::Published);
    assert_eq!(stored.attempt_count, 2);
    assert_eq!(stored.claim_owner, None);
    assert_eq!(stored.claim_expires_at, None);

    pool.close().await;
}
