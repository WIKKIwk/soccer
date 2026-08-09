use football_application::{
    RealtimeReplayAssessment, RealtimeRetentionPolicy, RealtimeSequenceGap, assess_realtime_replay,
    build_canonical_match_snapshot, realtime_snapshot_required_response,
};
use football_contracts::{
    MatchSnapshotVerificationStatus, RealtimeSequence, RealtimeSnapshotRequiredReason,
    RealtimeSubscriptionResponse, RealtimeTopic,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, C1EventType, ClockMilliseconds,
    CommandId, DomainEventEnvelope, EventId, EventIndex, EventVerificationStatus, GoalAwardType,
    GoalRecordedPayload, MatchId, MatchPeriod, MatchProjection, TeamId, UtcInstant,
};
use serde_json::json;
use uuid::Uuid;

fn uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("reconnect fixture UUID should be valid")
}

fn sequence(value: u64) -> RealtimeSequence {
    RealtimeSequence::try_new(value).expect("reconnect fixture sequence should be positive")
}

fn retention_window() -> football_application::RealtimeRetainedMessageWindow {
    RealtimeRetentionPolicy {
        max_messages_per_topic: 3,
        max_age_millis: 1_000,
    }
    .window_for(sequence(20))
    .expect("reconnect retention window should be valid")
}

fn topic() -> RealtimeTopic {
    RealtimeTopic::match_topic(uuid("019ff300-3bc5-7cf0-b3d2-0728c077a930"))
}

fn canonical_projection() -> MatchProjection {
    let match_id = MatchId::from_uuid(uuid("019ff310-3bc5-7cf0-b3d2-0728c077a930"));
    let home_team_id = TeamId::from_uuid(uuid("019ff311-3bc5-7cf0-b3d2-0728c077a930"));
    let away_team_id = TeamId::from_uuid(uuid("019ff312-3bc5-7cf0-b3d2-0728c077a930"));
    let event_id = EventId::from_uuid(uuid("019ff313-3bc5-7cf0-b3d2-0728c077a930"));
    let actor_id = AdminUserId::from_uuid(uuid("019ff314-3bc5-7cf0-b3d2-0728c077a930"));
    let payload = C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
        home_team_id,
        None,
        GoalAwardType::Regular,
    ));
    let event = DomainEventEnvelope::new(
        event_id,
        match_id,
        AggregateVersion::from_value(1),
        CommandId::from_uuid(uuid("019ff315-3bc5-7cf0-b3d2-0728c077a930")),
        EventIndex::initial(),
        C1EventType::GoalRecorded
            .try_as_event_type()
            .expect("goal event type should be valid"),
        C1EventType::GoalRecorded.schema_version(),
        MatchPeriod::FirstHalf,
        ClockMilliseconds::from_millis(12_000),
        UtcInstant::from_unix_timestamp(1_777_000_000).expect("event time should be valid"),
        UtcInstant::from_unix_timestamp(1_777_000_001).expect("recorded time should be valid"),
        ActorIdentity::administrator(actor_id),
        EventVerificationStatus::Provisional,
        payload,
        None,
        None,
    );

    let mut projection = MatchProjection::new(match_id, home_team_id, away_team_id)
        .expect("reconnect fixture teams should be distinct");
    projection
        .reduce_event(&event)
        .expect("canonical goal should reduce into the projection");
    projection
}

#[test]
fn stale_cursor_requires_snapshot_then_reads_the_canonical_projection() {
    let request_id = uuid("019ff320-3bc5-7cf0-b3d2-0728c077a930");
    let assessment = assess_realtime_replay(Some(sequence(16)), retention_window(), &[])
        .expect("retained storage should be structurally valid");
    let RealtimeReplayAssessment::Gap(gap) = assessment else {
        panic!("cursor before retention must be a gap");
    };
    assert_eq!(
        gap,
        RealtimeSequenceGap::BeforeRetention {
            requested_sequence: sequence(17),
            oldest_retained_sequence: sequence(18),
        }
    );

    let response = realtime_snapshot_required_response(request_id, topic(), gap);
    assert_eq!(
        response,
        RealtimeSubscriptionResponse::SnapshotRequired {
            request_id,
            topic: topic(),
            reason: RealtimeSnapshotRequiredReason::BeforeRetention,
        }
    );
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({
            "type": "snapshot_required",
            "request_id": request_id,
            "topic": topic().to_string(),
            "reason": "before_retention"
        })
    );

    let snapshot = build_canonical_match_snapshot(
        &canonical_projection(),
        "2026-08-09T12:30:00Z",
        MatchSnapshotVerificationStatus::Provisional,
        vec![],
    )
    .expect("canonical projection should map to the REST snapshot");
    assert_eq!(snapshot.data.score.home_goals, 1);
    assert_eq!(snapshot.data.score.away_goals, 0);
    assert_eq!(snapshot.data.aggregate_version, 1);
    assert_eq!(snapshot.data.timeline.len(), 1);
    assert_eq!(
        snapshot.data.timeline[0].event_id,
        uuid("019ff313-3bc5-7cf0-b3d2-0728c077a930")
    );
    assert_eq!(
        snapshot.meta.verification_status,
        MatchSnapshotVerificationStatus::Provisional
    );
}

#[test]
fn complete_retained_cursor_replays_without_snapshot_required() {
    let assessment = assess_realtime_replay(
        Some(sequence(17)),
        retention_window(),
        &[sequence(18), sequence(19), sequence(20)],
    )
    .expect("complete retained sequence should be assessable");

    assert_eq!(
        assessment,
        RealtimeReplayAssessment::Replayable {
            from_sequence: sequence(18),
            through_sequence: sequence(20),
        }
    );
}

#[test]
fn malformed_retained_order_fails_closed_before_snapshot_response() {
    let error = assess_realtime_replay(
        Some(sequence(17)),
        retention_window(),
        &[sequence(19), sequence(18)],
    )
    .expect_err("unordered retained rows must not be converted into a recovery response");

    assert!(matches!(
        error,
        football_application::RealtimeRetainedSequenceError::NotStrictlyIncreasing {
            previous_sequence,
            current_sequence,
        } if previous_sequence == sequence(19) && current_sequence == sequence(18)
    ));
}
