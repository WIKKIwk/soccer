use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, ClockMilliseconds, CommandId,
    CorrectionEventPayload, CorrectionReason, DomainEventEnvelope, EffectiveEventStream, EventId,
    EventIndex, EventVerificationStatus, EventVoidedPayload, GoalAwardType, GoalRecordedPayload,
    MatchEventPayload, MatchId, MatchPeriod, MatchProjection, MatchVerificationStatus,
    MatchVerifiedPayload, TeamId, UtcInstant, VerificationEventPayload, YellowCardRecordedPayload,
};
use football_infrastructure::{
    DatabasePoolConfig, MIGRATIONS_DIRECTORY, MatchProjectionRebuildError, connect_database,
    encode_match_event_envelope, load_canonical_match_event_stream, rebuild_match_projection,
    run_migrations,
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
    Uuid::parse_str(value).expect("projection rebuild fixture UUID should be valid")
}

fn instant(seconds: i64) -> UtcInstant {
    UtcInstant::from_unix_timestamp(seconds).expect("projection rebuild timestamp should be valid")
}

type SummaryProjectionRow = (
    i64,
    i64,
    String,
    String,
    i64,
    i64,
    Option<OffsetDateTime>,
    bool,
    bool,
    Option<String>,
);

type ConsistencySummaryRow = (i64, i64, i64, i64, String, bool, bool, Option<String>);
type ConsistencyHeadRow = (i64, i64, bool, bool, Option<String>);
type ConsistencyTimelineRow = (Uuid, i64, i64, String, String, i64);

#[allow(clippy::too_many_arguments)]
fn envelope(
    event_id: Uuid,
    match_id: Uuid,
    aggregate_version: u64,
    command_id: Uuid,
    period: MatchPeriod,
    clock_ms: u64,
    actor_id: Uuid,
    verification_status: EventVerificationStatus,
    payload: MatchEventPayload,
    voids_event_id: Option<Uuid>,
) -> DomainEventEnvelope<MatchEventPayload> {
    let (event_type, schema_version) = match &payload {
        MatchEventPayload::Lifecycle(payload) => (
            payload
                .event_type()
                .try_as_event_type()
                .expect("lifecycle event type should be valid"),
            payload.schema_version(),
        ),
        MatchEventPayload::C1(payload) => (
            payload
                .event_type()
                .try_as_event_type()
                .expect("C1 event type should be valid"),
            payload.schema_version(),
        ),
        MatchEventPayload::Correction(payload) => (
            payload
                .event_type()
                .try_as_event_type()
                .expect("correction event type should be valid"),
            payload.schema_version(),
        ),
        MatchEventPayload::Verification(payload) => (
            payload
                .event_type()
                .try_as_event_type()
                .expect("verification event type should be valid"),
            payload.schema_version(),
        ),
    };

    DomainEventEnvelope::new(
        EventId::from_uuid(event_id),
        MatchId::from_uuid(match_id),
        AggregateVersion::from_value(aggregate_version),
        CommandId::from_uuid(command_id),
        EventIndex::initial(),
        event_type,
        schema_version,
        period,
        ClockMilliseconds::from_millis(clock_ms),
        instant(1_777_000_000 + i64::try_from(aggregate_version).expect("version fits i64")),
        instant(1_777_000_100 + i64::try_from(aggregate_version).expect("version fits i64")),
        ActorIdentity::administrator(AdminUserId::from_uuid(actor_id)),
        verification_status,
        payload,
        None,
        voids_event_id.map(EventId::from_uuid),
    )
}

async fn insert_typed_event(pool: &PgPool, event: &DomainEventEnvelope<MatchEventPayload>) {
    let encoded = encode_match_event_envelope(event)
        .expect("typed projection rebuild fixture event should encode");

    query(
        "INSERT INTO match_events ( \
             event_id, match_id, aggregate_version, causation_command_id, event_index, \
             event_type, event_schema_version, period, clock_ms, occurred_at, recorded_at, \
             actor_id, verification_status, payload, supersedes_event_id, voids_event_id \
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(event.event_id().as_uuid())
    .bind(event.match_id().as_uuid())
    .bind(i64::try_from(event.aggregate_version().value()).expect("version fits BIGINT"))
    .bind(event.causation_command_id().as_uuid())
    .bind(i64::from(event.event_index().value()))
    .bind(event.event_type().as_str())
    .bind(i32::from(event.event_schema_version().value()))
    .bind(event.period().as_persisted_str())
    .bind(i64::try_from(event.clock_ms().value()).expect("clock fits BIGINT"))
    .bind(event.occurred_at().as_offset_datetime())
    .bind(event.recorded_at().as_offset_datetime())
    .bind(event.actor().admin_user_id().as_uuid())
    .bind(event.verification_status().as_persisted_str())
    .bind(Json(encoded.payload().clone()))
    .bind(event.supersedes_event_id().map(|value| value.as_uuid()))
    .bind(event.voids_event_id().map(|value| value.as_uuid()))
    .execute(pool)
    .await
    .expect("canonical projection rebuild fixture event should insert");
}

async fn seed_fixture(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, Uuid) {
    let competition_id = uuid("019ffb00-3bc5-7cf0-b3d2-0728c077a930");
    let season_id = uuid("019ffb01-3bc5-7cf0-b3d2-0728c077a930");
    let home_team_id = uuid("019ffb02-3bc5-7cf0-b3d2-0728c077a930");
    let away_team_id = uuid("019ffb03-3bc5-7cf0-b3d2-0728c077a930");
    let fixture_id = uuid("019ffb04-3bc5-7cf0-b3d2-0728c077a930");
    let match_id = uuid("019ffb05-3bc5-7cf0-b3d2-0728c077a930");
    let actor_id = uuid("019ffb06-3bc5-7cf0-b3d2-0728c077a930");

    query("INSERT INTO admin_users (id, username, display_name) VALUES ($1, $2, $3)")
        .bind(actor_id)
        .bind("projection-rebuild-admin")
        .bind("Projection Rebuild Admin")
        .execute(pool)
        .await
        .expect("projection rebuild administrator should insert");

    query("INSERT INTO competitions (id, name) VALUES ($1, $2)")
        .bind(competition_id)
        .bind("O‘zbekiston Superligasi")
        .execute(pool)
        .await
        .expect("projection rebuild competition should insert");
    query("INSERT INTO seasons (id, competition_id, year) VALUES ($1, $2, $3)")
        .bind(season_id)
        .bind(competition_id)
        .bind(2026_i32)
        .execute(pool)
        .await
        .expect("projection rebuild season should insert");
    for (team_id, name) in [
        (home_team_id, "Projection Rebuild Home"),
        (away_team_id, "Projection Rebuild Away"),
    ] {
        query("INSERT INTO teams (id, name) VALUES ($1, $2)")
            .bind(team_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("projection rebuild team should insert");
    }
    query(
        "INSERT INTO fixtures (id, competition_id, season_id, home_team_id, away_team_id, kickoff_at) \
         VALUES ($1, $2, $3, $4, $5, '2026-08-09T12:00:00Z')",
    )
    .bind(fixture_id)
    .bind(competition_id)
    .bind(season_id)
    .bind(home_team_id)
    .bind(away_team_id)
    .execute(pool)
    .await
    .expect("projection rebuild fixture should insert");

    for index in 1..=5_u8 {
        let command_id = uuid(&format!("019ffb1{}-3bc5-7cf0-b3d2-0728c077a930", index));
        let client_command_id = uuid(&format!("019ffb2{}-3bc5-7cf0-b3d2-0728c077a930", index));
        query(
            "INSERT INTO commands (id, client_command_id, request_fingerprint, status) \
             VALUES ($1, $2, $3, 'received')",
        )
        .bind(command_id)
        .bind(client_command_id)
        .bind(vec![index; 32])
        .execute(pool)
        .await
        .expect("projection rebuild command should insert");
    }

    (
        fixture_id,
        match_id,
        home_team_id,
        away_team_id,
        actor_id,
        competition_id,
        season_id,
    )
}

async fn insert_canonical_fixture_events(
    pool: &PgPool,
    match_id: Uuid,
    home_team_id: Uuid,
    actor_id: Uuid,
) -> (Uuid, Uuid, Uuid, Uuid) {
    let goal_event_id = uuid("019ffb30-3bc5-7cf0-b3d2-0728c077a930");
    let yellow_event_id = uuid("019ffb31-3bc5-7cf0-b3d2-0728c077a930");
    let void_event_id = uuid("019ffb32-3bc5-7cf0-b3d2-0728c077a930");
    let verify_event_id = uuid("019ffb33-3bc5-7cf0-b3d2-0728c077a930");

    let goal = envelope(
        goal_event_id,
        match_id,
        1,
        uuid("019ffb11-3bc5-7cf0-b3d2-0728c077a930"),
        MatchPeriod::FirstHalf,
        12_000,
        actor_id,
        EventVerificationStatus::Provisional,
        MatchEventPayload::C1(C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
            TeamId::from_uuid(home_team_id),
            None,
            GoalAwardType::Regular,
        ))),
        None,
    );
    let yellow = envelope(
        yellow_event_id,
        match_id,
        2,
        uuid("019ffb12-3bc5-7cf0-b3d2-0728c077a930"),
        MatchPeriod::SecondHalf,
        58_000,
        actor_id,
        EventVerificationStatus::Provisional,
        MatchEventPayload::C1(C1EventPayload::YellowCardRecorded(
            YellowCardRecordedPayload::new(football_domain::PlayerId::from_uuid(uuid(
                "019ffb40-3bc5-7cf0-b3d2-0728c077a930",
            ))),
        )),
        None,
    );
    let void = envelope(
        void_event_id,
        match_id,
        3,
        uuid("019ffb13-3bc5-7cf0-b3d2-0728c077a930"),
        MatchPeriod::HalfTime,
        45_000,
        actor_id,
        EventVerificationStatus::Verified,
        MatchEventPayload::Correction(CorrectionEventPayload::EventVoided(
            EventVoidedPayload::new(
                CorrectionReason::try_new("official protocol removed the goal")
                    .expect("void reason should be valid"),
            ),
        )),
        Some(goal_event_id),
    );
    let verified = envelope(
        verify_event_id,
        match_id,
        4,
        uuid("019ffb14-3bc5-7cf0-b3d2-0728c077a930"),
        MatchPeriod::SecondHalf,
        90_000,
        actor_id,
        EventVerificationStatus::Verified,
        MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(
            MatchVerifiedPayload::new(),
        )),
        None,
    );

    for event in [&goal, &yellow, &void, &verified] {
        insert_typed_event(pool, event).await;
    }

    (
        goal_event_id,
        yellow_event_id,
        void_event_id,
        verify_event_id,
    )
}

fn expected_verification_status(stream: &EffectiveEventStream<MatchEventPayload>) -> &'static str {
    stream
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(_)) => {
                Some("verified")
            }
            MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(_)) => {
                Some("provisional")
            }
            MatchEventPayload::Lifecycle(_)
            | MatchEventPayload::C1(_)
            | MatchEventPayload::Correction(_) => None,
        })
        .unwrap_or("provisional")
}

async fn assert_projection_matches_canonical_effective_stream(
    pool: &PgPool,
    match_id: Uuid,
    home_team_id: Uuid,
    away_team_id: Uuid,
) {
    let canonical = load_canonical_match_event_stream(pool, MatchId::from_uuid(match_id))
        .await
        .expect("canonical stream should load for consistency comparison");
    let effective = EffectiveEventStream::try_from_canonical(&canonical)
        .expect("canonical stream should produce an effective consistency view");
    let reduced = MatchProjection::from_effective_stream(
        &effective,
        TeamId::from_uuid(home_team_id),
        TeamId::from_uuid(away_team_id),
    )
    .expect("effective stream should reduce for consistency comparison");

    let summary: ConsistencySummaryRow = query_as(
        "SELECT source_aggregate_version, projection_version, home_goals, away_goals, \
                verification_status, is_rebuilding, is_current, last_error_code \
         FROM match_summary_projections WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(pool)
    .await
    .expect("summary projection should load for consistency comparison");
    let head: ConsistencyHeadRow = query_as(
        "SELECT source_aggregate_version, projection_version, is_rebuilding, \
                is_current, last_error_code \
         FROM match_timeline_projection_heads WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(pool)
    .await
    .expect("timeline head should load for consistency comparison");
    let timeline: Vec<ConsistencyTimelineRow> = query_as(
        "SELECT event_id, aggregate_version, event_index, event_type, period, clock_ms \
         FROM match_timeline_projections WHERE match_id = $1 ORDER BY aggregate_version",
    )
    .bind(match_id)
    .fetch_all(pool)
    .await
    .expect("timeline projection should load for consistency comparison");

    let canonical_version = i64::try_from(canonical.current_version().value())
        .expect("canonical version should fit projection BIGINT");
    assert_eq!(
        reduced.current_aggregate_version(),
        canonical.current_version()
    );
    assert_eq!(summary.0, canonical_version);
    assert_eq!(summary.0, head.0);
    assert_eq!(summary.1, head.1);
    assert_eq!(summary.2, i64::from(reduced.score().home_goals()));
    assert_eq!(summary.3, i64::from(reduced.score().away_goals()));
    assert_eq!(
        summary.4,
        expected_verification_status(&effective),
        "summary verification status must follow the latest effective marker"
    );
    assert!(!summary.5);
    assert!(summary.6);
    assert!(summary.7.is_none());
    assert!(!head.2);
    assert!(head.3);
    assert!(head.4.is_none());

    assert_eq!(timeline.len(), reduced.timeline().len());
    for (stored, expected) in timeline.iter().zip(reduced.timeline()) {
        assert_eq!(stored.0, expected.event_id().as_uuid());
        assert_eq!(
            stored.1,
            i64::try_from(expected.aggregate_version().value())
                .expect("timeline version should fit projection BIGINT")
        );
        assert_eq!(stored.2, i64::from(expected.event_index().value()));
        assert_eq!(stored.3, expected.event_type().as_str());
        assert_eq!(stored.4, expected.period().as_persisted_str());
        assert_eq!(
            stored.5,
            i64::try_from(expected.clock_ms().value())
                .expect("timeline clock should fit projection BIGINT")
        );
        assert!(
            canonical
                .iter()
                .any(|event| event.event_id().as_uuid() == stored.0),
            "every timeline row must reference a canonical event"
        );
        assert!(
            effective
                .iter()
                .any(|event| event.event_id().as_uuid() == stored.0),
            "every timeline row must reference an effective event"
        );
    }
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn match_projection_rebuilds_effective_score_timeline_and_versions() {
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
        .expect("clean projection rebuild target should accept a PostgreSQL connection");
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("projection rebuild preflight query should succeed");
    assert!(
        !history_table_exists,
        "projection rebuild target must be clean"
    );

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply before projection rebuild");

    let (fixture_id, match_id, home_team_id, _away_team_id, actor_id, _, _) =
        seed_fixture(&pool).await;
    let (goal_event_id, yellow_event_id, _, _) =
        insert_canonical_fixture_events(&pool, match_id, home_team_id, actor_id).await;

    query(
        "INSERT INTO match_summary_projections (match_id, fixture_id, home_goals, away_goals, match_state) \
         VALUES ($1, $2, 9, 8, 'lineups_confirmed')",
    )
    .bind(match_id)
    .bind(fixture_id)
    .execute(&pool)
    .await
    .expect("existing summary projection fixture should insert");
    query(
        "INSERT INTO match_lineup_projections ( \
             match_id, fixture_id, source_aggregate_version, projection_version, \
             computed_at_utc, is_current \
         ) VALUES ($1, $2, 12, 9, '2026-08-09T11:00:00Z', TRUE)",
    )
    .bind(match_id)
    .bind(fixture_id)
    .execute(&pool)
    .await
    .expect("unrelated lineup projection fixture should insert");

    let first_computed_at = instant(1_778_000_000);
    let first = rebuild_match_projection(
        &pool,
        MatchId::from_uuid(match_id),
        football_domain::FixtureId::from_uuid(fixture_id),
        first_computed_at,
    )
    .await
    .expect("first projection rebuild should publish");
    assert_eq!(first.source_aggregate_version(), 4);
    assert_eq!(first.projection_version(), 1);
    assert_eq!(first.home_goals(), 0);
    assert_eq!(first.away_goals(), 0);
    assert_eq!(first.timeline_entries(), 1);
    assert_eq!(
        first.verification_status(),
        MatchVerificationStatus::Verified
    );

    let summary: SummaryProjectionRow = query_as(
        "SELECT home_goals, away_goals, match_state, verification_status, \
                    source_aggregate_version, projection_version, computed_at_utc, \
                    is_rebuilding, is_current, last_error_code \
             FROM match_summary_projections WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .expect("published summary projection should be readable");
    assert_eq!(summary.0, 0);
    assert_eq!(summary.1, 0);
    assert_eq!(summary.2, "lineups_confirmed");
    assert_eq!(summary.3, "verified");
    assert_eq!(summary.4, 4);
    assert_eq!(summary.5, 1);
    assert_eq!(summary.6, Some(first_computed_at.as_offset_datetime()));
    assert!(!summary.7);
    assert!(summary.8);
    assert!(summary.9.is_none());

    let head: (i64, i64, Option<OffsetDateTime>, bool, bool, Option<String>) = query_as(
        "SELECT source_aggregate_version, projection_version, computed_at_utc, \
                is_rebuilding, is_current, last_error_code \
         FROM match_timeline_projection_heads WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .expect("published timeline head should be readable");
    assert_eq!(head.0, 4);
    assert_eq!(head.1, 1);
    assert_eq!(head.2, Some(first_computed_at.as_offset_datetime()));
    assert!(!head.3);
    assert!(head.4);
    assert!(head.5.is_none());

    let timeline: Vec<(Uuid, i64, i64, String, String, i64)> = query_as(
        "SELECT event_id, aggregate_version, event_index, event_type, period, clock_ms \
         FROM match_timeline_projections WHERE match_id = $1 ORDER BY aggregate_version",
    )
    .bind(match_id)
    .fetch_all(&pool)
    .await
    .expect("published timeline should be readable");
    assert_eq!(
        timeline,
        vec![(
            yellow_event_id,
            2,
            0,
            "YellowCardRecorded".to_owned(),
            "second_half".to_owned(),
            58_000,
        )]
    );
    assert!(!timeline.iter().any(|row| row.0 == goal_event_id));

    let lineup_metadata: (i64, i64, bool) = query_as(
        "SELECT source_aggregate_version, projection_version, is_current \
         FROM match_lineup_projections WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .expect("lineup projection should remain readable");
    assert_eq!(lineup_metadata, (12, 9, true));

    let second_computed_at = instant(1_778_000_100);
    let second = rebuild_match_projection(
        &pool,
        MatchId::from_uuid(match_id),
        football_domain::FixtureId::from_uuid(fixture_id),
        second_computed_at,
    )
    .await
    .expect("second projection rebuild should replace one generation atomically");
    assert_eq!(second.projection_version(), 2);
    assert_eq!(second.source_aggregate_version(), 4);
    assert_eq!(second.timeline_entries(), 1);

    let timeline_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_timeline_projections WHERE match_id = $1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .expect("replaced timeline should have one effective event");
    assert_eq!(timeline_count, 1);

    query(
        "INSERT INTO match_events ( \
             event_id, match_id, aggregate_version, causation_command_id, event_index, \
             event_type, event_schema_version, period, clock_ms, occurred_at, recorded_at, \
             actor_id, verification_status, payload \
         ) VALUES ($1, $2, 5, $3, 0, 'GoalRecorded', 1, 'second_half', 65000, \
                   '2026-08-09T12:00:05Z', '2026-08-09T12:00:06Z', $4, 'provisional', $5)",
    )
    .bind(uuid("019ffb34-3bc5-7cf0-b3d2-0728c077a930"))
    .bind(match_id)
    .bind(uuid("019ffb15-3bc5-7cf0-b3d2-0728c077a930"))
    .bind(actor_id)
    .bind(Json(json!({"team_id": home_team_id.to_string()})))
    .execute(&pool)
    .await
    .expect("invalid payload fixture row should insert for rebuild failure test");

    let failure = rebuild_match_projection(
        &pool,
        MatchId::from_uuid(match_id),
        football_domain::FixtureId::from_uuid(fixture_id),
        instant(1_778_000_200),
    )
    .await
    .expect_err("invalid canonical payload should fail the rebuild");
    assert!(matches!(
        failure,
        MatchProjectionRebuildError::MatchEvent(_)
    ));

    let failed_summary: SummaryProjectionRow = query_as(
        "SELECT home_goals, away_goals, match_state, verification_status, \
                    source_aggregate_version, projection_version, computed_at_utc, \
                    is_rebuilding, is_current, last_error_code \
             FROM match_summary_projections WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .expect("failed summary projection should remain readable");
    assert_eq!(failed_summary.0, 0);
    assert_eq!(failed_summary.1, 0);
    assert_eq!(failed_summary.2, "lineups_confirmed");
    assert_eq!(failed_summary.3, "verified");
    assert_eq!(failed_summary.4, 4);
    assert_eq!(failed_summary.5, 2);
    assert_eq!(
        failed_summary.6,
        Some(second_computed_at.as_offset_datetime())
    );
    assert!(!failed_summary.7);
    assert!(!failed_summary.8);
    assert_eq!(
        failed_summary.9.as_deref(),
        Some("projection.rebuild_failed")
    );

    let failed_head: (i64, i64, Option<OffsetDateTime>, bool, bool, Option<String>) = query_as(
        "SELECT source_aggregate_version, projection_version, computed_at_utc, \
                is_rebuilding, is_current, last_error_code \
         FROM match_timeline_projection_heads WHERE match_id = $1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await
    .expect("failed timeline head should remain readable");
    assert_eq!(failed_head.0, 4);
    assert_eq!(failed_head.1, 2);
    assert_eq!(failed_head.2, Some(second_computed_at.as_offset_datetime()));
    assert!(!failed_head.3);
    assert!(!failed_head.4);
    assert_eq!(failed_head.5.as_deref(), Some("projection.rebuild_failed"));

    let preserved_timeline_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_timeline_projections WHERE match_id = $1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .expect("failed rebuild should preserve prior timeline rows");
    assert_eq!(preserved_timeline_count, 1);

    pool.close().await;
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn match_projection_rows_stay_consistent_with_canonical_replay_after_append() {
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
        .expect("clean consistency target should accept a PostgreSQL connection");
    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("consistency preflight query should succeed");
    assert!(!history_table_exists, "consistency target must be clean");

    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply before consistency assertions");

    let (fixture_id, match_id, home_team_id, away_team_id, actor_id, _, _) =
        seed_fixture(&pool).await;
    insert_canonical_fixture_events(&pool, match_id, home_team_id, actor_id).await;

    let first = rebuild_match_projection(
        &pool,
        MatchId::from_uuid(match_id),
        football_domain::FixtureId::from_uuid(fixture_id),
        instant(1_778_001_000),
    )
    .await
    .expect("initial consistency projection should publish");
    assert_eq!(first.source_aggregate_version(), 4);
    assert_projection_matches_canonical_effective_stream(
        &pool,
        match_id,
        home_team_id,
        away_team_id,
    )
    .await;

    let fifth_event = envelope(
        uuid("019ffb35-3bc5-7cf0-b3d2-0728c077a930"),
        match_id,
        5,
        uuid("019ffb15-3bc5-7cf0-b3d2-0728c077a930"),
        MatchPeriod::SecondHalf,
        70_000,
        actor_id,
        EventVerificationStatus::Provisional,
        MatchEventPayload::C1(C1EventPayload::YellowCardRecorded(
            YellowCardRecordedPayload::new(football_domain::PlayerId::from_uuid(uuid(
                "019ffb41-3bc5-7cf0-b3d2-0728c077a930",
            ))),
        )),
        None,
    );
    insert_typed_event(&pool, &fifth_event).await;

    let second = rebuild_match_projection(
        &pool,
        MatchId::from_uuid(match_id),
        football_domain::FixtureId::from_uuid(fixture_id),
        instant(1_778_001_100),
    )
    .await
    .expect("appended consistency projection should publish");
    assert_eq!(second.source_aggregate_version(), 5);
    assert_eq!(second.projection_version(), 2);
    assert_projection_matches_canonical_effective_stream(
        &pool,
        match_id,
        home_team_id,
        away_team_id,
    )
    .await;

    pool.close().await;
}
