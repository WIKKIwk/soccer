use football_contracts::{
    MatchSnapshotVerificationStatus, RealtimeNotification, RealtimeSequence, RealtimeTopic,
    RealtimeVerificationChangeType,
};
use football_domain::{EventId, UtcInstant};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, OutboxDeliveryStatus,
    OutboxPublishFailureDisposition, OutboxRepositoryError, REALTIME_FANOUT_LATENCY_MEASUREMENT,
    claim_outbox_messages, connect_database, handle_outbox_publish_failure, load_outbox_message,
    load_realtime_topic_sequence, mark_outbox_published, measure_realtime_fanout_latency,
    prepare_realtime_match_notification, prepare_realtime_notification, run_migrations,
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
    Uuid::parse_str(value).expect("realtime notification fixture UUID should be valid")
}

fn instant(seconds: i64) -> time::OffsetDateTime {
    UtcInstant::from_unix_timestamp(seconds)
        .expect("realtime notification fixture time should be valid")
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
        "realtime notification target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

async fn insert_fixture(pool: &PgPool) -> (EventId, Uuid) {
    let admin_id = uuid("01a000ff-3bc5-7cf0-b3d2-0728c077a930");
    let command_id = uuid("01a00010-3bc5-7cf0-b3d2-0728c077a930");
    let client_command_id = uuid("01a00011-3bc5-7cf0-b3d2-0728c077a930");
    let event_id = EventId::from_uuid(uuid("01a00012-3bc5-7cf0-b3d2-0728c077a930"));
    let match_id = uuid("01a00000-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'realtime-notification-admin', 'Realtime Notification Admin', TRUE)",
    )
    .bind(admin_id)
    .execute(pool)
    .await
    .expect("fixture administrator should insert");

    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(client_command_id)
    .bind(vec![0xa1_u8; 32])
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
    .bind(admin_id)
    .bind(Json(json!({"fixture": "realtime_notification"})))
    .execute(pool)
    .await
    .expect("fixture canonical event should insert");

    let payload = json!({
        "event_id": event_id.to_string(),
        "match_id": match_id.to_string(),
        "aggregate_version": 1,
        "causation_command_id": command_id.to_string(),
        "event_index": 0,
        "event_type": "GoalRecorded",
        "event_schema_version": 1,
        "period": "first_half",
        "clock_ms": 1000,
        "occurred_at": instant(1_001).to_string(),
        "recorded_at": instant(1_002).to_string(),
        "actor_id": admin_id.to_string(),
        "verification_status": "provisional",
        "payload": {"fixture": "realtime_notification"},
        "supersedes_event_id": null,
        "voids_event_id": null
    });

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
    .bind(Json(payload))
    .bind(instant(900))
    .execute(pool)
    .await
    .expect("fixture outbox message should insert");

    (event_id, match_id)
}

async fn insert_correction_fixture(pool: &PgPool) -> (EventId, EventId, Uuid) {
    let admin_id = uuid("01a001ff-3bc5-7cf0-b3d2-0728c077a930");
    let target_command_id = uuid("01a00110-3bc5-7cf0-b3d2-0728c077a930");
    let correction_command_id = uuid("01a00114-3bc5-7cf0-b3d2-0728c077a930");
    let target_client_command_id = uuid("01a00111-3bc5-7cf0-b3d2-0728c077a930");
    let correction_client_command_id = uuid("01a00115-3bc5-7cf0-b3d2-0728c077a930");
    let target_event_id = EventId::from_uuid(uuid("01a00112-3bc5-7cf0-b3d2-0728c077a930"));
    let correction_event_id = EventId::from_uuid(uuid("01a00113-3bc5-7cf0-b3d2-0728c077a930"));
    let match_id = uuid("01a00100-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'realtime-correction-admin', 'Realtime Correction Admin', TRUE)",
    )
    .bind(admin_id)
    .execute(pool)
    .await
    .expect("correction fixture administrator should insert");

    for (command_id, client_command_id, fingerprint) in [
        (
            target_command_id,
            target_client_command_id,
            vec![0xb1_u8; 32],
        ),
        (
            correction_command_id,
            correction_client_command_id,
            vec![0xb2_u8; 32],
        ),
    ] {
        query(
            "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
             VALUES ($1, $2, $3, 'received')",
        )
        .bind(command_id)
        .bind(client_command_id)
        .bind(fingerprint)
        .execute(pool)
        .await
        .expect("correction fixture command should insert");
    }

    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload
         )
         VALUES ($1, $2, 1, $3, 0, 'GoalRecorded', 1, 'first_half', 1000,
                 $4, $5, $6, 'provisional', $7)",
    )
    .bind(target_event_id.as_uuid())
    .bind(match_id)
    .bind(target_command_id)
    .bind(instant(1_001))
    .bind(instant(1_002))
    .bind(admin_id)
    .bind(Json(json!({"fixture": "realtime_correction_target"})))
    .execute(pool)
    .await
    .expect("correction fixture target event should insert");

    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload,
             supersedes_event_id, voids_event_id
         )
         VALUES ($1, $2, 2, $3, 1, 'EventVoided', 1, 'second_half', 2000,
                 $4, $5, $6, 'verified', $7, NULL, $8)",
    )
    .bind(correction_event_id.as_uuid())
    .bind(match_id)
    .bind(correction_command_id)
    .bind(instant(1_003))
    .bind(instant(1_004))
    .bind(admin_id)
    .bind(Json(json!({
        "reason": "official protocol removed the goal"
    })))
    .bind(target_event_id.as_uuid())
    .execute(pool)
    .await
    .expect("correction fixture event should insert");

    let payload = json!({
        "event_id": correction_event_id.to_string(),
        "match_id": match_id.to_string(),
        "aggregate_version": 2,
        "causation_command_id": correction_command_id.to_string(),
        "event_index": 1,
        "event_type": "EventVoided",
        "event_schema_version": 1,
        "period": "second_half",
        "clock_ms": 2000,
        "occurred_at": instant(1_003).to_string(),
        "recorded_at": instant(1_004).to_string(),
        "actor_id": admin_id.to_string(),
        "verification_status": "verified",
        "payload": {"reason": "official protocol removed the goal"},
        "supersedes_event_id": null,
        "voids_event_id": target_event_id.to_string()
    });

    query(
        "INSERT INTO outbox_messages (
             message_id, aggregate_type, aggregate_id, source_aggregate_version,
             causation_command_id, source_event_id, message_type, schema_version,
             payload, created_at, next_attempt_at
         )
         VALUES ($1, 'match', $2, 2, $3, $1, 'match.event', 1, $4, $5, $5)",
    )
    .bind(correction_event_id.as_uuid())
    .bind(match_id)
    .bind(correction_command_id)
    .bind(Json(payload))
    .bind(instant(900))
    .execute(pool)
    .await
    .expect("correction fixture outbox message should insert");

    (correction_event_id, target_event_id, match_id)
}

async fn insert_verification_fixture(pool: &PgPool) -> (EventId, Uuid) {
    let admin_id = uuid("01a002ff-3bc5-7cf0-b3d2-0728c077a930");
    let command_id = uuid("01a00210-3bc5-7cf0-b3d2-0728c077a930");
    let client_command_id = uuid("01a00211-3bc5-7cf0-b3d2-0728c077a930");
    let event_id = EventId::from_uuid(uuid("01a00212-3bc5-7cf0-b3d2-0728c077a930"));
    let match_id = uuid("01a00200-3bc5-7cf0-b3d2-0728c077a930");

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'realtime-verification-admin', 'Realtime Verification Admin', TRUE)",
    )
    .bind(admin_id)
    .execute(pool)
    .await
    .expect("verification fixture administrator should insert");

    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(client_command_id)
    .bind(vec![0xc1_u8; 32])
    .execute(pool)
    .await
    .expect("verification fixture command should insert");

    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload
         )
         VALUES ($1, $2, 1, $3, 0, 'MatchVerified', 1, 'second_half', 3000,
                 $4, $5, $6, 'verified', $7)",
    )
    .bind(event_id.as_uuid())
    .bind(match_id)
    .bind(command_id)
    .bind(instant(1_005))
    .bind(instant(1_006))
    .bind(admin_id)
    .bind(Json(json!({
        "source": "official_pfl_match_protocol"
    })))
    .execute(pool)
    .await
    .expect("verification fixture event should insert");

    let payload = json!({
        "event_id": event_id.to_string(),
        "match_id": match_id.to_string(),
        "aggregate_version": 1,
        "causation_command_id": command_id.to_string(),
        "event_index": 0,
        "event_type": "MatchVerified",
        "event_schema_version": 1,
        "period": "second_half",
        "clock_ms": 3000,
        "occurred_at": instant(1_005).to_string(),
        "recorded_at": instant(1_006).to_string(),
        "actor_id": admin_id.to_string(),
        "verification_status": "verified",
        "payload": {"source": "official_pfl_match_protocol"},
        "supersedes_event_id": null,
        "voids_event_id": null
    });

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
    .bind(Json(payload))
    .bind(instant(900))
    .execute(pool)
    .await
    .expect("verification fixture outbox message should insert");

    (event_id, match_id)
}

async fn insert_live_correction_live_fixture(pool: &PgPool) -> ([EventId; 3], Uuid) {
    let admin_id = uuid("01a004ff-3bc5-7cf0-b3d2-0728c077a930");
    let match_id = uuid("01a00400-3bc5-7cf0-b3d2-0728c077a930");
    let goal_command_id = uuid("01a00410-3bc5-7cf0-b3d2-0728c077a930");
    let correction_command_id = uuid("01a00411-3bc5-7cf0-b3d2-0728c077a930");
    let card_command_id = uuid("01a00412-3bc5-7cf0-b3d2-0728c077a930");
    let goal_client_command_id = uuid("01a00420-3bc5-7cf0-b3d2-0728c077a930");
    let correction_client_command_id = uuid("01a00421-3bc5-7cf0-b3d2-0728c077a930");
    let card_client_command_id = uuid("01a00422-3bc5-7cf0-b3d2-0728c077a930");
    let goal_event_id = EventId::from_uuid(uuid("01a00430-3bc5-7cf0-b3d2-0728c077a930"));
    let correction_event_id = EventId::from_uuid(uuid("01a00431-3bc5-7cf0-b3d2-0728c077a930"));
    let card_event_id = EventId::from_uuid(uuid("01a00432-3bc5-7cf0-b3d2-0728c077a930"));

    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'realtime-live-correction-admin', 'Realtime Live Correction Admin', TRUE)",
    )
    .bind(admin_id)
    .execute(pool)
    .await
    .expect("live correction fixture administrator should insert");

    for (command_id, client_command_id, fingerprint) in [
        (goal_command_id, goal_client_command_id, vec![0xd1_u8; 32]),
        (
            correction_command_id,
            correction_client_command_id,
            vec![0xd2_u8; 32],
        ),
        (card_command_id, card_client_command_id, vec![0xd3_u8; 32]),
    ] {
        query(
            "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
             VALUES ($1, $2, $3, 'received')",
        )
        .bind(command_id)
        .bind(client_command_id)
        .bind(fingerprint)
        .execute(pool)
        .await
        .expect("live correction fixture command should insert");
    }

    let events = [
        (
            goal_event_id,
            goal_command_id,
            1_i64,
            0_i64,
            "GoalRecorded",
            "first_half",
            1_000_i64,
            1_001_i64,
            "provisional",
            json!({"team_id": "01a00440-3bc5-7cf0-b3d2-0728c077a930", "scorer_player_id": null, "award_type": "regular"}),
            None,
        ),
        (
            correction_event_id,
            correction_command_id,
            2,
            1,
            "EventVoided",
            "second_half",
            45_000,
            45_001,
            "verified",
            json!({"reason": "official protocol removed the provisional goal"}),
            Some(goal_event_id.as_uuid()),
        ),
        (
            card_event_id,
            card_command_id,
            3,
            2,
            "YellowCardRecorded",
            "second_half",
            58_000,
            58_001,
            "provisional",
            json!({"player_id": "01a00441-3bc5-7cf0-b3d2-0728c077a930"}),
            None,
        ),
    ];

    for (
        event_id,
        command_id,
        aggregate_version,
        event_index,
        event_type,
        period,
        clock_ms,
        recorded_seconds,
        verification_status,
        event_payload,
        voids_event_id,
    ) in events
    {
        let occurred_at = instant(recorded_seconds - 1_000);
        let recorded_at = instant(recorded_seconds);
        query(
            "INSERT INTO match_events (
                 event_id, match_id, aggregate_version, causation_command_id,
                 event_index, event_type, event_schema_version, period, clock_ms,
                 occurred_at, recorded_at, actor_id, verification_status, payload,
                 supersedes_event_id, voids_event_id
             )
             VALUES ($1, $2, $3, $4, $5, $6, 1, $7, $8,
                     $9, $10, $11, $12, $13, NULL, $14)",
        )
        .bind(event_id.as_uuid())
        .bind(match_id)
        .bind(aggregate_version)
        .bind(command_id)
        .bind(event_index)
        .bind(event_type)
        .bind(period)
        .bind(clock_ms)
        .bind(occurred_at)
        .bind(recorded_at)
        .bind(admin_id)
        .bind(verification_status)
        .bind(Json(event_payload.clone()))
        .bind(voids_event_id)
        .execute(pool)
        .await
        .expect("live correction fixture canonical event should insert");

        let payload = json!({
            "event_id": event_id.to_string(),
            "match_id": match_id.to_string(),
            "aggregate_version": aggregate_version,
            "causation_command_id": command_id.to_string(),
            "event_index": event_index,
            "event_type": event_type,
            "event_schema_version": 1,
            "period": period,
            "clock_ms": clock_ms,
            "occurred_at": occurred_at.to_string(),
            "recorded_at": recorded_at.to_string(),
            "actor_id": admin_id.to_string(),
            "verification_status": verification_status,
            "payload": event_payload,
            "supersedes_event_id": null,
            "voids_event_id": voids_event_id.map(|id| id.to_string()),
        });
        let created_at = instant(899 + aggregate_version);
        query(
            "INSERT INTO outbox_messages (
                 message_id, aggregate_type, aggregate_id, source_aggregate_version,
                 causation_command_id, source_event_id, message_type, schema_version,
                 payload, created_at, next_attempt_at
             )
             VALUES ($1, 'match', $2, $3, $4, $1, 'match.event', 1, $5, $6, $6)",
        )
        .bind(event_id.as_uuid())
        .bind(match_id)
        .bind(aggregate_version)
        .bind(command_id)
        .bind(Json(payload))
        .bind(created_at)
        .execute(pool)
        .await
        .expect("live correction fixture outbox message should insert");
    }

    (
        [goal_event_id, correction_event_id, card_event_id],
        match_id,
    )
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn claimed_match_event_reuses_one_notification_identity_across_retries() {
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
        .expect("clean realtime notification target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let (event_id, match_id) = insert_fixture(&pool).await;
    let worker_id = uuid("01a00020-3bc5-7cf0-b3d2-0728c077a930");
    let now = UtcInstant::from_unix_timestamp(2_000).unwrap();
    let expires_at = UtcInstant::from_unix_timestamp(2_030).unwrap();

    let mut rollback_transaction = pool
        .begin()
        .await
        .expect("rollback preparation transaction should begin");
    let rollback_claim =
        claim_outbox_messages(&mut rollback_transaction, worker_id, now, expires_at, 1)
            .await
            .expect("rollback preparation claim should succeed");
    assert_eq!(rollback_claim.len(), 1);
    let rollback_notification =
        prepare_realtime_match_notification(&mut rollback_transaction, &rollback_claim[0])
            .await
            .expect("rollback preparation should allocate a notification sequence");
    assert_eq!(rollback_notification.sequence(), RealtimeSequence::first());
    rollback_transaction
        .rollback()
        .await
        .expect("notification preparation transaction should roll back");

    assert!(
        load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
            .await
            .expect("rolled-back sequence lookup should succeed")
            .is_none(),
        "rollback must not leave an uncommitted realtime sequence"
    );
    let pending_after_rollback = load_outbox_message(&pool, event_id)
        .await
        .expect("rolled-back outbox message should load");
    assert_eq!(
        pending_after_rollback.delivery_status,
        OutboxDeliveryStatus::Pending
    );
    assert_eq!(pending_after_rollback.attempt_count, 0);

    let mut commit_transaction = pool
        .begin()
        .await
        .expect("committed preparation transaction should begin");
    let claimed = claim_outbox_messages(&mut commit_transaction, worker_id, now, expires_at, 1)
        .await
        .expect("committed preparation claim should succeed");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].delivery_status, OutboxDeliveryStatus::Processing);
    assert_eq!(claimed[0].claim_owner, Some(worker_id));

    let notification = prepare_realtime_match_notification(&mut commit_transaction, &claimed[0])
        .await
        .expect("claimed match event should become a realtime notification");
    assert_eq!(notification.notification_type(), "match.event");
    assert_eq!(notification.schema_version(), 1);
    assert_eq!(notification.notification_id(), event_id.as_uuid());
    assert_eq!(notification.topic(), RealtimeTopic::match_topic(match_id));
    assert_eq!(notification.sequence(), RealtimeSequence::first());
    assert_eq!(notification.payload(), &claimed[0].payload);

    let wire = serde_json::to_value(&notification)
        .expect("realtime notification should serialize for transport");
    assert_eq!(wire["type"], "match.event");
    assert_eq!(wire["schema_version"], 1);
    assert_eq!(wire["topic"], format!("match:{match_id}"));
    assert_eq!(wire["sequence"], 1);
    assert_eq!(wire["payload"], claimed[0].payload);

    commit_transaction
        .commit()
        .await
        .expect("committed notification preparation should commit");

    let stored_sequence = load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
        .await
        .expect("committed sequence lookup should succeed")
        .expect("committed notification should persist its topic sequence");
    assert_eq!(stored_sequence.sequence, RealtimeSequence::first());

    let processing_after_commit = load_outbox_message(&pool, event_id)
        .await
        .expect("claimed outbox message should load after notification preparation");
    assert_eq!(
        processing_after_commit.delivery_status,
        OutboxDeliveryStatus::Processing
    );
    assert_eq!(processing_after_commit.attempt_count, 1);
    assert_eq!(processing_after_commit.claim_owner, Some(worker_id));
    assert_eq!(processing_after_commit.published_at, None);

    let mut failure_transaction = pool
        .begin()
        .await
        .expect("retry scheduling transaction should begin");
    let retry_schedule = match handle_outbox_publish_failure(
        &mut failure_transaction,
        event_id,
        worker_id,
        UtcInstant::from_unix_timestamp(2_005).unwrap(),
    )
    .await
    .expect("first transport failure should schedule a retry")
    {
        OutboxPublishFailureDisposition::Retry(schedule) => schedule,
        other => panic!("first transport failure should be retryable, got {other:?}"),
    };
    assert_eq!(
        retry_schedule.next_attempt_at,
        UtcInstant::from_unix_timestamp(2_006).unwrap()
    );
    failure_transaction
        .commit()
        .await
        .expect("retry schedule should commit");

    let retry_worker_id = uuid("01a00021-3bc5-7cf0-b3d2-0728c077a930");
    let mut retry_transaction = pool
        .begin()
        .await
        .expect("retry claim transaction should begin");
    let retry_claim = claim_outbox_messages(
        &mut retry_transaction,
        retry_worker_id,
        UtcInstant::from_unix_timestamp(2_006).unwrap(),
        UtcInstant::from_unix_timestamp(2_036).unwrap(),
        1,
    )
    .await
    .expect("due retry should be claimable");
    assert_eq!(retry_claim.len(), 1);
    assert_eq!(retry_claim[0].attempt_count, 2);
    assert_eq!(retry_claim[0].claim_owner, Some(retry_worker_id));

    let retry_notification =
        prepare_realtime_match_notification(&mut retry_transaction, &retry_claim[0])
            .await
            .expect("retry should reuse the committed notification identity");
    assert_eq!(retry_notification, notification);

    let repeated_notification =
        prepare_realtime_match_notification(&mut retry_transaction, &retry_claim[0])
            .await
            .expect("repeated preparation should remain idempotent");
    assert_eq!(repeated_notification, notification);
    retry_transaction
        .commit()
        .await
        .expect("retry notification preparation should commit");

    let mut completion_transaction = pool
        .begin()
        .await
        .expect("retry publish completion transaction should begin");
    mark_outbox_published(
        &mut completion_transaction,
        event_id,
        retry_worker_id,
        UtcInstant::from_unix_timestamp(2_010).unwrap(),
    )
    .await
    .expect("retry worker should complete the published notification");
    completion_transaction
        .commit()
        .await
        .expect("retry publish completion should commit");

    let final_sequence = load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
        .await
        .expect("final sequence lookup should succeed")
        .expect("notification topic sequence should remain present");
    assert_eq!(final_sequence.sequence, RealtimeSequence::first());
    let identity_count: i64 =
        query_scalar("SELECT COUNT(*) FROM realtime_notification_identities WHERE message_id = $1")
            .bind(event_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("notification identity count should load");
    assert_eq!(identity_count, 1);

    let published_after_retry = load_outbox_message(&pool, event_id)
        .await
        .expect("published retry outbox message should load");
    assert_eq!(
        published_after_retry.delivery_status,
        OutboxDeliveryStatus::Published
    );
    assert_eq!(published_after_retry.attempt_count, 2);
    assert_eq!(published_after_retry.claim_owner, None);
    assert!(published_after_retry.published_at.is_some());

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn worker_restart_after_lease_expiry_reuses_the_same_notification_identity() {
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
        .expect("clean worker restart target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let (event_id, match_id) = insert_fixture(&pool).await;
    let worker_a = uuid("01a00320-3bc5-7cf0-b3d2-0728c077a930");
    let worker_b = uuid("01a00321-3bc5-7cf0-b3d2-0728c077a930");
    let now = UtcInstant::from_unix_timestamp(2_000).unwrap();
    let first_expiry = UtcInstant::from_unix_timestamp(2_030).unwrap();

    let mut first_transaction = pool
        .begin()
        .await
        .expect("first worker transaction should begin");
    let first_claim = claim_outbox_messages(&mut first_transaction, worker_a, now, first_expiry, 1)
        .await
        .expect("first worker should claim the outbox row");
    assert_eq!(first_claim.len(), 1);
    assert_eq!(first_claim[0].attempt_count, 1);
    let first_notification = prepare_realtime_notification(&mut first_transaction, &first_claim[0])
        .await
        .expect("first worker should prepare the notification before its restart");
    let first_wire = serde_json::to_value(&first_notification)
        .expect("first notification should serialize for duplicate comparison");
    first_transaction
        .commit()
        .await
        .expect("first worker claim and identity should commit");

    let first_state = load_outbox_message(&pool, event_id)
        .await
        .expect("first worker state should load");
    assert_eq!(
        first_state.delivery_status,
        OutboxDeliveryStatus::Processing
    );
    assert_eq!(first_state.claim_owner, Some(worker_a));
    assert_eq!(first_state.attempt_count, 1);

    query("UPDATE outbox_messages SET claim_expires_at = $1 WHERE message_id = $2")
        .bind(instant(1_999))
        .bind(event_id.as_uuid())
        .execute(&pool)
        .await
        .expect("worker A lease should become expired for restart simulation");

    let mut restart_transaction = pool
        .begin()
        .await
        .expect("restarted worker transaction should begin");
    let restart_claim = claim_outbox_messages(
        &mut restart_transaction,
        worker_b,
        now,
        UtcInstant::from_unix_timestamp(2_030).unwrap(),
        1,
    )
    .await
    .expect("restarted worker should reclaim the expired row");
    assert_eq!(restart_claim.len(), 1);
    assert_eq!(restart_claim[0].attempt_count, 2);
    assert_eq!(restart_claim[0].claim_owner, Some(worker_b));

    let restarted_notification =
        prepare_realtime_notification(&mut restart_transaction, &restart_claim[0])
            .await
            .expect("restarted worker should reuse the durable notification identity");
    assert_eq!(
        restarted_notification, first_notification,
        "a worker restart must not allocate a second notification identity"
    );
    let duplicate_notification =
        prepare_realtime_notification(&mut restart_transaction, &restart_claim[0])
            .await
            .expect("duplicate preparation should remain idempotent after restart");
    assert_eq!(duplicate_notification, first_notification);
    assert_eq!(
        serde_json::to_value(&restarted_notification)
            .expect("restarted notification should serialize"),
        first_wire
    );
    restart_transaction
        .commit()
        .await
        .expect("restarted worker preparation should commit");

    let mut stale_completion_transaction = pool
        .begin()
        .await
        .expect("stale worker completion transaction should begin");
    assert!(matches!(
        mark_outbox_published(
            &mut stale_completion_transaction,
            event_id,
            worker_a,
            UtcInstant::from_unix_timestamp(2_010).unwrap(),
        )
        .await,
        Err(OutboxRepositoryError::PublishClaimNotOwnedOrExpired { message_id })
            if message_id == event_id
    ));
    stale_completion_transaction
        .rollback()
        .await
        .expect("stale worker completion transaction should roll back");

    let mut completion_transaction = pool
        .begin()
        .await
        .expect("restarted worker completion transaction should begin");
    mark_outbox_published(
        &mut completion_transaction,
        event_id,
        worker_b,
        UtcInstant::from_unix_timestamp(2_010).unwrap(),
    )
    .await
    .expect("restarted worker should complete publish");
    completion_transaction
        .commit()
        .await
        .expect("restarted worker publish completion should commit");

    let final_state = load_outbox_message(&pool, event_id)
        .await
        .expect("final restarted worker state should load");
    assert_eq!(final_state.delivery_status, OutboxDeliveryStatus::Published);
    assert_eq!(final_state.attempt_count, 2);
    assert_eq!(final_state.claim_owner, None);
    assert!(final_state.published_at.is_some());

    let stored_sequence = load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
        .await
        .expect("restarted worker topic sequence lookup should succeed")
        .expect("restarted worker topic sequence should exist");
    assert_eq!(stored_sequence.sequence, RealtimeSequence::first());
    let identity_count: i64 =
        query_scalar("SELECT COUNT(*) FROM realtime_notification_identities WHERE message_id = $1")
            .bind(event_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("restarted worker identity count should load");
    assert_eq!(identity_count, 1);

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn correction_during_live_match_preserves_realtime_order_and_continuation() {
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
        .expect("clean live correction target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let (event_ids, match_id) = insert_live_correction_live_fixture(&pool).await;
    let worker_id = uuid("01a00450-3bc5-7cf0-b3d2-0728c077a930");
    let mut transaction = pool
        .begin()
        .await
        .expect("live correction preparation transaction should begin");
    let claimed = claim_outbox_messages(
        &mut transaction,
        worker_id,
        UtcInstant::from_unix_timestamp(2_000).unwrap(),
        UtcInstant::from_unix_timestamp(2_030).unwrap(),
        3,
    )
    .await
    .expect("live correction worker should claim all three messages");
    assert_eq!(claimed.len(), 3);
    assert_eq!(
        claimed
            .iter()
            .map(|message| message.source_event_id)
            .collect::<Vec<_>>(),
        event_ids.to_vec()
    );

    let live_before_correction = prepare_realtime_notification(&mut transaction, &claimed[0])
        .await
        .expect("live event before correction should prepare");
    let correction = prepare_realtime_notification(&mut transaction, &claimed[1])
        .await
        .expect("correction during live match should prepare");
    let live_after_correction = prepare_realtime_notification(&mut transaction, &claimed[2])
        .await
        .expect("live event after correction should continue to prepare");

    let RealtimeNotification::MatchEvent(live_before_correction) = live_before_correction else {
        panic!("GoalRecorded must remain a match.event notification");
    };
    let RealtimeNotification::MatchCorrection(correction) = correction else {
        panic!("EventVoided must become a match.correction notification");
    };
    let RealtimeNotification::MatchEvent(live_after_correction) = live_after_correction else {
        panic!("YellowCardRecorded must remain a match.event notification");
    };

    assert_eq!(
        live_before_correction.topic(),
        RealtimeTopic::match_topic(match_id)
    );
    assert_eq!(correction.topic(), RealtimeTopic::match_topic(match_id));
    assert_eq!(
        live_after_correction.topic(),
        RealtimeTopic::match_topic(match_id)
    );
    assert_eq!(
        live_before_correction.notification_id(),
        event_ids[0].as_uuid()
    );
    assert_eq!(correction.notification_id(), event_ids[1].as_uuid());
    assert_eq!(
        live_after_correction.notification_id(),
        event_ids[2].as_uuid()
    );
    assert_eq!(live_before_correction.sequence(), RealtimeSequence::first());
    assert_eq!(correction.sequence().value(), 2);
    assert_eq!(live_after_correction.sequence().value(), 3);
    assert_eq!(correction.match_id(), match_id);
    assert_eq!(correction.correction_event_id(), event_ids[1].as_uuid());
    assert_eq!(correction.voids_event_id(), event_ids[0].as_uuid());
    assert_eq!(
        correction.reason(),
        "official protocol removed the provisional goal"
    );
    assert_eq!(
        live_before_correction.payload()["event_type"],
        "GoalRecorded"
    );
    assert_eq!(
        live_after_correction.payload()["event_type"],
        "YellowCardRecorded"
    );

    let before_wire = serde_json::to_value(&live_before_correction)
        .expect("live-before correction notification should serialize");
    let correction_wire =
        serde_json::to_value(&correction).expect("correction notification should serialize");
    let after_wire = serde_json::to_value(&live_after_correction)
        .expect("live-after correction notification should serialize");
    assert_eq!(before_wire["type"], "match.event");
    assert_eq!(before_wire["sequence"], 1);
    assert_eq!(correction_wire["type"], "match.correction");
    assert_eq!(correction_wire["sequence"], 2);
    assert_eq!(after_wire["type"], "match.event");
    assert_eq!(after_wire["sequence"], 3);

    transaction
        .commit()
        .await
        .expect("live correction notification preparation should commit");

    let mut completion_transaction = pool
        .begin()
        .await
        .expect("live correction publish completion transaction should begin");
    for event_id in event_ids {
        mark_outbox_published(
            &mut completion_transaction,
            event_id,
            worker_id,
            UtcInstant::from_unix_timestamp(2_010).unwrap(),
        )
        .await
        .expect("live correction worker should complete each published message");
    }
    completion_transaction
        .commit()
        .await
        .expect("live correction publish completions should commit");

    let stored_sequence = load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
        .await
        .expect("live correction topic sequence lookup should succeed")
        .expect("live correction topic sequence should exist");
    assert_eq!(stored_sequence.sequence.value(), 3);
    let identity_count: i64 = query_scalar("SELECT COUNT(*) FROM realtime_notification_identities")
        .fetch_one(&pool)
        .await
        .expect("live correction identity count should load");
    assert_eq!(identity_count, 3);

    for event_id in event_ids {
        let state = load_outbox_message(&pool, event_id)
            .await
            .expect("published live correction outbox message should load");
        assert_eq!(state.delivery_status, OutboxDeliveryStatus::Published);
        assert_eq!(state.claim_owner, None);
        assert!(state.published_at.is_some());
    }

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn measures_outbox_to_publish_ack_latency_after_notification_commit() {
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
        .expect("clean realtime latency target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let (event_id, _) = insert_fixture(&pool).await;
    let worker_id = uuid("01a00510-3bc5-7cf0-b3d2-0728c077a930");
    let mut preparation_transaction = pool
        .begin()
        .await
        .expect("latency preparation transaction should begin");
    let claimed = claim_outbox_messages(
        &mut preparation_transaction,
        worker_id,
        UtcInstant::from_unix_timestamp(900).unwrap(),
        UtcInstant::from_unix_timestamp(930).unwrap(),
        1,
    )
    .await
    .expect("latency fixture message should claim");
    assert_eq!(claimed.len(), 1);
    let outbox_created_at = claimed[0].created_at;
    let _notification = prepare_realtime_notification(&mut preparation_transaction, &claimed[0])
        .await
        .expect("latency fixture notification should prepare");
    preparation_transaction
        .commit()
        .await
        .expect("latency preparation should commit before publish acknowledgement");

    let published_at = UtcInstant::from_offset_datetime(
        time::OffsetDateTime::UNIX_EPOCH + time::Duration::milliseconds(901_750),
    );
    let mut completion_transaction = pool
        .begin()
        .await
        .expect("latency completion transaction should begin");
    let published = mark_outbox_published(
        &mut completion_transaction,
        event_id,
        worker_id,
        published_at,
    )
    .await
    .expect("latency fixture publish acknowledgement should persist");
    completion_transaction
        .commit()
        .await
        .expect("latency publish acknowledgement should commit");

    let sample = measure_realtime_fanout_latency(event_id, outbox_created_at, published)
        .expect("committed outbox and publish timestamps should produce a latency sample");
    assert_eq!(
        REALTIME_FANOUT_LATENCY_MEASUREMENT,
        "outbox_created_to_publish_ack"
    );
    assert_eq!(sample.message_id(), event_id);
    assert_eq!(sample.elapsed_millis(), 1_750);
    assert_eq!(sample.published_at(), published_at);

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn claimed_correction_uses_a_distinct_notification_variant_and_identity() {
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
        .expect("clean realtime correction target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let (correction_event_id, target_event_id, match_id) = insert_correction_fixture(&pool).await;
    let worker_id = uuid("01a00120-3bc5-7cf0-b3d2-0728c077a930");
    let mut transaction = pool
        .begin()
        .await
        .expect("correction preparation transaction should begin");
    let claimed = claim_outbox_messages(
        &mut transaction,
        worker_id,
        UtcInstant::from_unix_timestamp(2_000).unwrap(),
        UtcInstant::from_unix_timestamp(2_030).unwrap(),
        1,
    )
    .await
    .expect("correction outbox message should be claimable");
    assert_eq!(claimed.len(), 1);

    let notification = prepare_realtime_notification(&mut transaction, &claimed[0])
        .await
        .expect("claimed correction should become a typed realtime notification");
    let RealtimeNotification::MatchCorrection(correction) = notification else {
        panic!("EventVoided outbox row should not become a live-event notification");
    };
    assert_eq!(correction.notification_type(), "match.correction");
    assert_eq!(correction.notification_id(), correction_event_id.as_uuid());
    assert_eq!(correction.topic(), RealtimeTopic::match_topic(match_id));
    assert_eq!(correction.sequence(), RealtimeSequence::first());
    assert_eq!(correction.match_id(), match_id);
    assert_eq!(
        correction.correction_event_id(),
        correction_event_id.as_uuid()
    );
    assert_eq!(correction.voids_event_id(), target_event_id.as_uuid());
    assert_eq!(correction.reason(), "official protocol removed the goal");

    let wire = serde_json::to_value(&correction)
        .expect("prepared correction should serialize for transport");
    assert_eq!(wire["type"], "match.correction");
    assert_eq!(wire["sequence"], 1);
    assert_eq!(
        wire["voids_event_id"],
        target_event_id.as_uuid().to_string()
    );

    transaction
        .commit()
        .await
        .expect("correction notification preparation should commit");

    let stored_sequence = load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
        .await
        .expect("correction topic sequence lookup should succeed")
        .expect("correction notification should persist its topic sequence");
    assert_eq!(stored_sequence.sequence, RealtimeSequence::first());
    let identity_count: i64 =
        query_scalar("SELECT COUNT(*) FROM realtime_notification_identities WHERE message_id = $1")
            .bind(correction_event_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("correction notification identity count should load");
    assert_eq!(identity_count, 1);

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn claimed_verification_uses_a_distinct_notification_variant_and_identity() {
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
        .expect("clean realtime verification target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let (event_id, match_id) = insert_verification_fixture(&pool).await;
    let worker_id = uuid("01a00220-3bc5-7cf0-b3d2-0728c077a930");
    let mut transaction = pool
        .begin()
        .await
        .expect("verification preparation transaction should begin");
    let claimed = claim_outbox_messages(
        &mut transaction,
        worker_id,
        UtcInstant::from_unix_timestamp(2_000).unwrap(),
        UtcInstant::from_unix_timestamp(2_030).unwrap(),
        1,
    )
    .await
    .expect("verification outbox message should be claimable");
    assert_eq!(claimed.len(), 1);

    let notification = prepare_realtime_notification(&mut transaction, &claimed[0])
        .await
        .expect("claimed verification should become a typed realtime notification");
    let RealtimeNotification::MatchVerification(verification) = notification else {
        panic!("MatchVerified outbox row should not become a live-event notification");
    };
    assert_eq!(verification.notification_type(), "match.verification");
    assert_eq!(verification.notification_id(), event_id.as_uuid());
    assert_eq!(verification.topic(), RealtimeTopic::match_topic(match_id));
    assert_eq!(verification.sequence(), RealtimeSequence::first());
    assert_eq!(verification.match_id(), match_id);
    assert_eq!(verification.verification_event_id(), event_id.as_uuid());
    assert_eq!(
        verification.change_type(),
        RealtimeVerificationChangeType::MatchVerified
    );
    assert_eq!(
        verification.verification_status(),
        MatchSnapshotVerificationStatus::Verified
    );

    let wire = serde_json::to_value(&verification)
        .expect("prepared verification should serialize for transport");
    assert_eq!(wire["type"], "match.verification");
    assert_eq!(wire["change_type"], "match_verified");
    assert_eq!(wire["verification_status"], "verified");
    assert_eq!(wire["sequence"], 1);

    transaction
        .commit()
        .await
        .expect("verification notification preparation should commit");

    let stored_sequence = load_realtime_topic_sequence(&pool, RealtimeTopic::match_topic(match_id))
        .await
        .expect("verification topic sequence lookup should succeed")
        .expect("verification notification should persist its topic sequence");
    assert_eq!(stored_sequence.sequence, RealtimeSequence::first());
    let identity_count: i64 =
        query_scalar("SELECT COUNT(*) FROM realtime_notification_identities WHERE message_id = $1")
            .bind(event_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("verification notification identity count should load");
    assert_eq!(identity_count, 1);

    pool.close().await;
}
