use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, ClockMilliseconds, CommandId,
    DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion, EventVerificationStatus,
    GoalAwardType, GoalRecordedPayload, MatchId, MatchPeriod, PlayerId, UtcInstant,
    YellowCardRecordedPayload,
};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, connect_database, load_c1_event_stream,
    load_c1_event_stream_in_transaction, load_c1_events_for_command,
    load_c1_events_for_command_in_transaction, persist_c1_event_batch, run_migrations,
};
use serde_json::json;
use sqlx::{query, query_scalar, types::Json};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("match event fixture UUID should be valid")
}

fn event(
    event_id: &str,
    aggregate_version: u64,
    event_index: u32,
    payload: C1EventPayload,
) -> DomainEventEnvelope<C1EventPayload> {
    let event_type = payload.event_type();
    DomainEventEnvelope::new(
        EventId::from_uuid(uuid(event_id)),
        MatchId::from_uuid(uuid("019fe370-3bc5-7cf0-b3d2-0728c077a930")),
        AggregateVersion::from_value(aggregate_version),
        CommandId::from_uuid(uuid("019fe370-3bc5-7cf1-b3d2-0728c077a930")),
        EventIndex::from_value(event_index),
        event_type
            .try_as_event_type()
            .expect("C1 event type should be valid"),
        EventSchemaVersion::initial(),
        MatchPeriod::FirstHalf,
        ClockMilliseconds::from_millis(45_000),
        UtcInstant::from_unix_timestamp(1_777_000_000).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(1_777_000_001).expect("fixture time should be valid"),
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe370-3bc5-7cf2-b3d2-0728c077a930",
        ))),
        EventVerificationStatus::Provisional,
        payload,
        None,
        None,
    )
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn typed_c1_events_round_trip_through_postgres() {
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
        .expect("clean match-event target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "match-event target is not clean: _sqlx_migrations already exists"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let actor_id = uuid("019fe370-3bc5-7cf2-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("c1-roundtrip-admin")
    .bind("C1 Roundtrip Admin")
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let command_id = uuid("019fe370-3bc5-7cf1-b3d2-0728c077a930");
    query(
        "INSERT INTO commands (id, client_command_id, request_fingerprint, status)
         VALUES ($1, $2, $3, 'received')",
    )
    .bind(command_id)
    .bind(uuid("019fe370-3bc5-7cf3-b3d2-0728c077a930"))
    .bind(vec![0x11_u8; 32])
    .execute(&pool)
    .await
    .expect("fixture command should insert");

    let home_team_id =
        football_domain::TeamId::from_uuid(uuid("019fe370-3bc5-7cf4-b3d2-0728c077a930"));
    let first = event(
        "019fe370-3bc5-7cf5-b3d2-0728c077a930",
        1,
        0,
        C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
            home_team_id,
            None,
            GoalAwardType::Penalty,
        )),
    );
    let second = event(
        "019fe370-3bc5-7cf6-b3d2-0728c077a930",
        2,
        1,
        C1EventPayload::YellowCardRecorded(YellowCardRecordedPayload::new(PlayerId::from_uuid(
            uuid("019fe370-3bc5-7cf7-b3d2-0728c077a930"),
        ))),
    );
    let match_id = first.match_id();

    let mut transaction = pool
        .begin()
        .await
        .expect("match-event transaction should begin");
    persist_c1_event_batch(
        &mut transaction,
        AggregateVersion::initial(),
        &[first.clone(), second.clone()],
    )
    .await
    .expect("typed C1 event batch should persist");

    let loaded_before_commit = load_c1_event_stream_in_transaction(&mut transaction, match_id)
        .await
        .expect("typed C1 event stream should load from the same transaction");
    assert_eq!(loaded_before_commit.len(), 2);
    assert_eq!(loaded_before_commit.events()[0].payload(), first.payload());
    assert_eq!(loaded_before_commit.events()[1].payload(), second.payload());

    let command_events_before_commit =
        load_c1_events_for_command_in_transaction(&mut transaction, first.causation_command_id())
            .await
            .expect("command-local C1 events should load before commit");
    assert_eq!(command_events_before_commit.len(), 2);
    assert_eq!(command_events_before_commit[0].event_id(), first.event_id());
    assert_eq!(
        command_events_before_commit[1].event_id(),
        second.event_id()
    );

    transaction
        .commit()
        .await
        .expect("match-event transaction should commit");

    let stored_payload: Json<serde_json::Value> =
        query_scalar("SELECT payload FROM match_events WHERE event_id = $1")
            .bind(first.event_id().as_uuid())
            .fetch_one(&pool)
            .await
            .expect("stored payload should be readable");
    assert_eq!(
        stored_payload.0,
        json!({
            "team_id": home_team_id.as_uuid().to_string(),
            "scorer_player_id": null,
            "award_type": "penalty",
        })
    );

    let loaded = load_c1_event_stream(&pool, match_id)
        .await
        .expect("typed C1 event stream should load");
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.events()[0].payload(), first.payload());
    assert_eq!(loaded.events()[1].payload(), second.payload());
    assert_eq!(loaded.events()[0].event_index().value(), 0);
    assert_eq!(loaded.events()[1].event_index().value(), 1);
    assert_eq!(
        loaded.events()[1].causation_command_id(),
        first.causation_command_id()
    );

    let command_events = load_c1_events_for_command(&pool, first.causation_command_id())
        .await
        .expect("command-local C1 events should load after commit");
    assert_eq!(command_events.len(), 2);
    assert_eq!(command_events[0].payload(), first.payload());
    assert_eq!(command_events[1].payload(), second.payload());

    pool.close().await;
}
