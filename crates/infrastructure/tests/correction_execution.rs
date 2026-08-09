use football_application::commands::{
    CommandAdmissionRequest, CommandExecutionContext, LogicalCommandIdentity,
    into_persisted_command_result,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1LiveAction, C1LiveActionCommand,
    ClientCommandId, ClockMilliseconds, CommandId, CommandResultEnvelope, CorrectionReason,
    EffectiveEventStream, EventId, EventVerificationStatus, GoalAwardType, GoalRecordedPayload,
    MatchEventPayload, MatchId, MatchPeriod, MatchProjection, PlayerId, RequestFingerprint, TeamId,
    UtcInstant, VoidEventCommand, YellowCardRecordedPayload,
};
use football_infrastructure::{
    CommandAdmissionResult, DatabasePoolConfig, StoredTerminalCommandResult, admit_command,
    connect_database, execute_c1_live_action_in_canonical_stream_transaction,
    execute_void_event_in_transaction, load_canonical_match_event_stream,
    load_terminal_command_result, persist_application_command_result, run_migrations,
};
use serde_json::json;
use sqlx::{PgPool, query, query_scalar};
use std::path::PathBuf;
use uuid::Uuid;

fn repository_migrations_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(football_infrastructure::MIGRATIONS_DIRECTORY)
}

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("correction execution fixture UUID should be valid")
}

async fn admit_claimed_command(
    pool: &PgPool,
    command_id: CommandId,
    client_command_id: ClientCommandId,
    processing_owner: Uuid,
    fingerprint: u8,
    now_utc: UtcInstant,
) -> CommandAdmissionRequest {
    let request = CommandAdmissionRequest::try_new(
        LogicalCommandIdentity::new(
            command_id,
            client_command_id,
            RequestFingerprint::from_bytes([fingerprint; 32]),
        ),
        processing_owner,
        now_utc,
        UtcInstant::from_unix_timestamp(now_utc.unix_timestamp() + 30)
            .expect("fixture lease expiry should be valid"),
    )
    .expect("fixture command admission request should be valid");

    assert!(matches!(
        admit_command(pool, &request).await,
        Ok(CommandAdmissionResult::Claimed { command_id: actual }) if actual == command_id
    ));
    request
}

fn live_goal_command(
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
        UtcInstant::from_unix_timestamp(4_001).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(4_002).expect("fixture time should be valid"),
        actor,
        EventVerificationStatus::Provisional,
        C1LiveAction::RecordGoal(GoalRecordedPayload::new(
            team_id,
            None,
            GoalAwardType::Regular,
        )),
    )
    .expect("fixture goal command should be valid")
}

fn void_command(
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    target_event_id: EventId,
    actor: ActorIdentity,
) -> VoidEventCommand {
    VoidEventCommand::try_new(
        command_id,
        event_id,
        match_id,
        AggregateVersion::from_value(1),
        MatchPeriod::HalfTime,
        ClockMilliseconds::from_seconds(45).expect("fixture correction clock should be valid"),
        UtcInstant::from_unix_timestamp(4_011).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(4_012).expect("fixture time should be valid"),
        actor,
        EventVerificationStatus::Verified,
        target_event_id,
        CorrectionReason::try_new("official protocol removed the provisional goal")
            .expect("fixture correction reason should be valid"),
    )
    .expect("fixture void command should be valid")
}

fn live_yellow_card_command(
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    player_id: PlayerId,
    actor: ActorIdentity,
) -> C1LiveActionCommand {
    C1LiveActionCommand::try_new(
        command_id,
        event_id,
        match_id,
        AggregateVersion::from_value(2),
        MatchPeriod::SecondHalf,
        ClockMilliseconds::from_seconds(58).expect("fixture clock should be valid"),
        UtcInstant::from_unix_timestamp(4_021).expect("fixture time should be valid"),
        UtcInstant::from_unix_timestamp(4_022).expect("fixture time should be valid"),
        actor,
        EventVerificationStatus::Provisional,
        C1LiveAction::RecordYellowCard(YellowCardRecordedPayload::new(player_id)),
    )
    .expect("fixture yellow-card command should be valid")
}

fn accepted_result(
    command_id: CommandId,
    match_id: MatchId,
    final_version: AggregateVersion,
    completed_at: UtcInstant,
) -> football_application::commands::PersistedCommandResult {
    into_persisted_command_result(
        CommandResultEnvelope::<_, ()>::accepted(
            command_id,
            match_id,
            final_version,
            1,
            json!({"aggregate_version": final_version.value()}),
        ),
        completed_at,
        |payload| payload,
        |_: ()| ("domain.invalid_transition".to_owned(), json!({})),
    )
}

#[tokio::test]
#[ignore = "requires an explicitly approved clean TEST_DATABASE_URL"]
async fn correction_then_live_action_replays_the_effective_projection() {
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
        .expect("clean correction execution target should accept a PostgreSQL connection");

    let history_table_exists: bool =
        query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("migration history preflight query should succeed");
    assert!(
        !history_table_exists,
        "correction execution target is not clean: _sqlx_migrations already exists"
    );
    run_migrations(&pool, repository_migrations_path())
        .await
        .expect("all repository migrations should apply to the clean target");

    let actor_id = uuid("019fe730-3bc5-7cf0-b3d2-0728c077a930");
    query(
        "INSERT INTO admin_users (id, username, display_name, is_active)
         VALUES ($1, $2, $3, TRUE)",
    )
    .bind(actor_id)
    .bind("correction-execution-admin")
    .bind("Correction Execution Admin")
    .execute(&pool)
    .await
    .expect("fixture administrator should insert");

    let actor = ActorIdentity::administrator(AdminUserId::from_uuid(actor_id));
    let match_id = MatchId::from_uuid(uuid("019fe730-3bc5-7cf1-b3d2-0728c077a930"));
    let home_team_id = TeamId::from_uuid(uuid("019fe730-3bc5-7cf2-b3d2-0728c077a930"));
    let away_team_id = TeamId::from_uuid(uuid("019fe730-3bc5-7cf3-b3d2-0728c077a930"));
    let player_id = PlayerId::from_uuid(uuid("019fe730-3bc5-7cf4-b3d2-0728c077a930"));

    let goal_command_id = CommandId::from_uuid(uuid("019fe730-3bc5-7cf5-b3d2-0728c077a930"));
    let goal_event_id = EventId::from_uuid(uuid("019fe730-3bc5-7cf6-b3d2-0728c077a930"));
    let goal_request = admit_claimed_command(
        &pool,
        goal_command_id,
        ClientCommandId::try_from_uuid(uuid("019fe730-3bc5-7cf7-b3d2-0728c077a930"))
            .expect("goal client command ID should be UUIDv7"),
        uuid("019fe730-3bc5-7cf8-b3d2-0728c077a930"),
        0x71,
        UtcInstant::from_unix_timestamp(4_000).expect("fixture time should be valid"),
    )
    .await;
    let goal_command = live_goal_command(
        goal_command_id,
        goal_event_id,
        match_id,
        home_team_id,
        actor,
    );
    let goal_context = CommandExecutionContext::from_admission_request(
        goal_request,
        actor,
        Some(AggregateVersion::initial()),
    );
    let mut transaction = pool
        .begin()
        .await
        .expect("goal domain transaction should begin");
    let goal_stream = execute_c1_live_action_in_canonical_stream_transaction(
        &mut transaction,
        &goal_context,
        goal_command,
        UtcInstant::from_unix_timestamp(4_005).expect("fixture time should be valid"),
    )
    .await
    .expect("goal should append to the canonical union stream");
    assert_eq!(
        goal_stream.current_version(),
        AggregateVersion::from_value(1)
    );
    persist_application_command_result(
        &mut transaction,
        goal_request.processing_owner(),
        accepted_result(
            goal_command_id,
            match_id,
            goal_stream.current_version(),
            UtcInstant::from_unix_timestamp(4_005).expect("fixture time should be valid"),
        ),
    )
    .await
    .expect("goal terminal result should persist");
    transaction
        .commit()
        .await
        .expect("goal domain transaction should commit");

    let correction_command_id = CommandId::from_uuid(uuid("019fe730-3bc5-7cf9-b3d2-0728c077a930"));
    let correction_event_id = EventId::from_uuid(uuid("019fe730-3bc5-7cfa-b3d2-0728c077a930"));
    let correction_request = admit_claimed_command(
        &pool,
        correction_command_id,
        ClientCommandId::try_from_uuid(uuid("019fe730-3bc5-7cfb-b3d2-0728c077a930"))
            .expect("correction client command ID should be UUIDv7"),
        uuid("019fe730-3bc5-7cfc-b3d2-0728c077a930"),
        0x72,
        UtcInstant::from_unix_timestamp(4_010).expect("fixture time should be valid"),
    )
    .await;
    let correction_command = void_command(
        correction_command_id,
        correction_event_id,
        match_id,
        goal_event_id,
        actor,
    );
    let correction_context = CommandExecutionContext::from_admission_request(
        correction_request,
        actor,
        Some(AggregateVersion::from_value(1)),
    );
    let mut transaction = pool
        .begin()
        .await
        .expect("correction domain transaction should begin");
    let correction_stream = execute_void_event_in_transaction(
        &mut transaction,
        &correction_context,
        correction_command,
        UtcInstant::from_unix_timestamp(4_015).expect("fixture time should be valid"),
    )
    .await
    .expect("void correction should append to the canonical union stream");
    assert_eq!(
        correction_stream.current_version(),
        AggregateVersion::from_value(2)
    );
    assert_eq!(correction_stream.len(), 2);
    persist_application_command_result(
        &mut transaction,
        correction_request.processing_owner(),
        accepted_result(
            correction_command_id,
            match_id,
            correction_stream.current_version(),
            UtcInstant::from_unix_timestamp(4_015).expect("fixture time should be valid"),
        ),
    )
    .await
    .expect("correction terminal result should persist");
    transaction
        .commit()
        .await
        .expect("correction domain transaction should commit");

    let card_command_id = CommandId::from_uuid(uuid("019fe730-3bc5-7cfd-b3d2-0728c077a930"));
    let card_event_id = EventId::from_uuid(uuid("019fe730-3bc5-7cfe-b3d2-0728c077a930"));
    let card_request = admit_claimed_command(
        &pool,
        card_command_id,
        ClientCommandId::try_from_uuid(uuid("019fe730-3bc5-7cff-b3d2-0728c077a930"))
            .expect("card client command ID should be UUIDv7"),
        uuid("019fe731-3bc5-7cf0-b3d2-0728c077a930"),
        0x73,
        UtcInstant::from_unix_timestamp(4_020).expect("fixture time should be valid"),
    )
    .await;
    let card_command =
        live_yellow_card_command(card_command_id, card_event_id, match_id, player_id, actor);
    let card_context = CommandExecutionContext::from_admission_request(
        card_request,
        actor,
        Some(AggregateVersion::from_value(2)),
    );
    let mut transaction = pool
        .begin()
        .await
        .expect("card domain transaction should begin");
    let card_stream = execute_c1_live_action_in_canonical_stream_transaction(
        &mut transaction,
        &card_context,
        card_command,
        UtcInstant::from_unix_timestamp(4_025).expect("fixture time should be valid"),
    )
    .await
    .expect("live action after correction should append to the union stream");
    assert_eq!(
        card_stream.current_version(),
        AggregateVersion::from_value(3)
    );
    persist_application_command_result(
        &mut transaction,
        card_request.processing_owner(),
        accepted_result(
            card_command_id,
            match_id,
            card_stream.current_version(),
            UtcInstant::from_unix_timestamp(4_025).expect("fixture time should be valid"),
        ),
    )
    .await
    .expect("card terminal result should persist");
    transaction
        .commit()
        .await
        .expect("card domain transaction should commit");

    let canonical_stream = load_canonical_match_event_stream(&pool, match_id)
        .await
        .expect("committed mixed canonical stream should load");
    assert_eq!(canonical_stream.len(), 3);
    assert_eq!(
        canonical_stream.current_version(),
        AggregateVersion::from_value(3)
    );
    assert!(matches!(
        canonical_stream.events()[0].payload(),
        MatchEventPayload::C1(_)
    ));
    assert!(matches!(
        canonical_stream.events()[1].payload(),
        MatchEventPayload::Correction(_)
    ));
    assert!(matches!(
        canonical_stream.events()[2].payload(),
        MatchEventPayload::C1(_)
    ));

    let effective_stream = EffectiveEventStream::try_from_canonical(&canonical_stream)
        .expect("canonical correction should build an effective stream");
    assert_eq!(effective_stream.len(), 1);
    assert_eq!(
        effective_stream.canonical_version(),
        AggregateVersion::from_value(3)
    );
    assert_eq!(effective_stream.events()[0].event_id(), card_event_id);

    let projection =
        MatchProjection::from_effective_stream(&effective_stream, home_team_id, away_team_id)
            .expect("effective canonical stream should replay into match projection");
    assert_eq!(projection.score().home_goals(), 0);
    assert_eq!(projection.score().away_goals(), 0);
    assert_eq!(projection.timeline().len(), 1);
    assert_eq!(projection.timeline()[0].event_id(), card_event_id);
    assert_eq!(
        projection.current_aggregate_version(),
        AggregateVersion::from_value(3)
    );

    for (command_id, final_version) in [
        (goal_command_id, 1),
        (correction_command_id, 2),
        (card_command_id, 3),
    ] {
        let terminal_result = load_terminal_command_result(&pool, command_id)
            .await
            .expect("committed terminal result should load");
        assert!(matches!(
            terminal_result,
            StoredTerminalCommandResult::Accepted {
                aggregate_id,
                final_aggregate_version,
                event_count: 1,
                ..
            } if aggregate_id == match_id
                && final_aggregate_version == AggregateVersion::from_value(final_version)
        ));
    }

    let persisted_event_count: i64 =
        query_scalar("SELECT COUNT(*) FROM match_events WHERE match_id = $1")
            .bind(match_id.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("canonical event count query should succeed");
    assert_eq!(persisted_event_count, 3);

    pool.close().await;
}
