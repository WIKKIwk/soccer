use football_application::commands::{
    CommandAdmissionRequest, CommandAdmissionResult, LogicalCommandIdentity,
};
use football_domain::{ClientCommandId, CommandId, RequestFingerprint, UtcInstant};
use football_infrastructure::{
    CommandAdmissionError, DatabasePoolConfig, MIGRATIONS_DIRECTORY, admit_command,
    connect_database, run_migrations,
};
use serde_json::json;
use sqlx::{PgPool, query, query_as, query_scalar, types::Json};
use std::path::PathBuf;
use time::OffsetDateTime;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("stale-processing fixture UUID should be valid")
}

fn request(
    command_id: &str,
    client_command_id: &str,
    fingerprint: u8,
    processing_owner: &str,
    now_seconds: i64,
    expires_at_seconds: i64,
) -> CommandAdmissionRequest {
    CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            CommandId::from_uuid(uuid(command_id)),
            ClientCommandId::try_from_uuid(uuid(client_command_id))
                .expect("fixture client command ID should be UUIDv7"),
            RequestFingerprint::from_bytes([fingerprint; 32]),
        ),
        uuid(processing_owner),
        UtcInstant::from_unix_timestamp(now_seconds).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(expires_at_seconds)
            .expect("fixture lease expiry should be valid"),
    )
    .expect("fixture command admission request should be valid")
}

async fn apply_migrations(pool: &PgPool) {
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "stale-processing target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn stale_processing_reclaims_only_without_canonical_event_evidence() {
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
        .expect("clean stale-processing target should accept a PostgreSQL connection");
    apply_migrations(&pool).await;

    let actor_id = uuid("019fe960-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, 'stale-processing-admin', 'Stale Processing Admin', TRUE)",
    )
    .bind(actor_id)
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let no_event = request(
        "019fe960-3bc5-7cf1-b3d2-0728c077a930",
        "019fe960-3bc5-7cf2-b3d2-0728c077a930",
        0x61,
        "019fe960-3bc5-7cf3-b3d2-0728c077a930",
        2_000,
        2_030,
    );
    let no_event_command_id = no_event.identity().command_id();
    assert_eq!(
        admit_command(&pool, &no_event).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: no_event_command_id,
        }
    );
    query("UPDATE commands SET processing_expires_at = $1 WHERE id = $2")
        .bind(
            UtcInstant::from_unix_timestamp(1_999)
                .expect("expired fixture time should be valid")
                .as_offset_datetime(),
        )
        .bind(no_event_command_id.as_uuid())
        .execute(&pool)
        .await
        .expect("no-event lease should become expired");

    let no_event_retry = request(
        "019fe960-3bc5-7cf4-b3d2-0728c077a930",
        "019fe960-3bc5-7cf2-b3d2-0728c077a930",
        0x61,
        "019fe960-3bc5-7cf5-b3d2-0728c077a930",
        2_000,
        2_060,
    );
    assert_eq!(
        admit_command(&pool, &no_event_retry).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: no_event_command_id,
        }
    );

    let event_backed = request(
        "019fe960-3bc5-7cf6-b3d2-0728c077a930",
        "019fe960-3bc5-7cf7-b3d2-0728c077a930",
        0x62,
        "019fe960-3bc5-7cf8-b3d2-0728c077a930",
        3_000,
        3_030,
    );
    let event_backed_command_id = event_backed.identity().command_id();
    assert_eq!(
        admit_command(&pool, &event_backed).await.unwrap(),
        CommandAdmissionResult::Claimed {
            command_id: event_backed_command_id,
        }
    );
    query(
        "INSERT INTO match_events (
             event_id, match_id, aggregate_version, causation_command_id,
             event_index, event_type, event_schema_version, period, clock_ms,
             occurred_at, recorded_at, actor_id, verification_status, payload
         )
         VALUES ($1, $2, 1, $3, 0, 'GoalRecorded', 1, 'first_half', 1000,
                 $4, $5, $6, 'provisional', $7)",
    )
    .bind(uuid("019fe960-3bc5-7cfa-b3d2-0728c077a930"))
    .bind(uuid("019fe960-3bc5-7cfb-b3d2-0728c077a930"))
    .bind(event_backed_command_id.as_uuid())
    .bind(
        UtcInstant::from_unix_timestamp(3_001)
            .expect("fixture event time should be valid")
            .as_offset_datetime(),
    )
    .bind(
        UtcInstant::from_unix_timestamp(3_002)
            .expect("fixture event time should be valid")
            .as_offset_datetime(),
    )
    .bind(actor_id)
    .bind(Json(json!({
        "team_id": "019fe960-3bc5-7cfc-b3d2-0728c077a930",
        "scorer_player_id": null,
        "award_type": "regular"
    })))
    .execute(&pool)
    .await
    .expect("canonical event evidence should insert");
    query("UPDATE commands SET processing_expires_at = $1 WHERE id = $2")
        .bind(
            UtcInstant::from_unix_timestamp(2_999)
                .expect("expired fixture time should be valid")
                .as_offset_datetime(),
        )
        .bind(event_backed_command_id.as_uuid())
        .execute(&pool)
        .await
        .expect("event-backed lease should become expired");

    let event_backed_retry = request(
        "019fe960-3bc5-7cfd-b3d2-0728c077a930",
        "019fe960-3bc5-7cf7-b3d2-0728c077a930",
        0x62,
        "019fe960-3bc5-7cfe-b3d2-0728c077a930",
        3_000,
        3_060,
    );
    assert!(matches!(
        admit_command(&pool, &event_backed_retry).await,
        Err(CommandAdmissionError::ReconciliationRequired { command_id })
            if command_id == event_backed_command_id
    ));

    let (status, processing_owner, processing_expires_at): (String, Uuid, OffsetDateTime) =
        query_as(
            "SELECT status, processing_owner, processing_expires_at
             FROM commands WHERE id = $1",
        )
        .bind(event_backed_command_id.as_uuid())
        .fetch_one(&pool)
        .await
        .expect("event-backed command state should remain readable");
    assert_eq!(status, "processing");
    assert_eq!(processing_owner, event_backed.processing_owner());
    assert_eq!(
        processing_expires_at,
        UtcInstant::from_unix_timestamp(2_999)
            .expect("expired fixture time should be valid")
            .as_offset_datetime()
    );

    pool.close().await;
}
