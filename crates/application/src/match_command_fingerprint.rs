use football_domain::{
    C1LiveAction, C1LiveActionCommand, ClientCommandId, LifecycleEventPayload, LineupRole,
    MatchLifecycleCommand, OperatorLeaseId, RequestFingerprint,
};

use crate::commands::fingerprint_canonical_bytes;

pub fn fingerprint_lifecycle_command(
    client_command_id: ClientCommandId,
    operator_lease_id: OperatorLeaseId,
    command: &MatchLifecycleCommand,
) -> RequestFingerprint {
    fingerprint_canonical_bytes(
        canonical_base(
            command.command_name(),
            command.command_id().to_string(),
            client_command_id,
            command.event_id().to_string(),
            operator_lease_id,
            command.match_id().to_string(),
            command.actor().admin_user_id().to_string(),
            command.expected_version().value(),
            command.clock_ms().value(),
            command
                .occurred_at()
                .as_offset_datetime()
                .unix_timestamp_nanos(),
            lifecycle_payload(command.action()),
        )
        .as_bytes(),
    )
}

pub fn fingerprint_c1_command(
    client_command_id: ClientCommandId,
    operator_lease_id: OperatorLeaseId,
    command: &C1LiveActionCommand,
) -> RequestFingerprint {
    fingerprint_canonical_bytes(
        canonical_base(
            command.command_name(),
            command.command_id().to_string(),
            client_command_id,
            command.event_id().to_string(),
            operator_lease_id,
            command.match_id().to_string(),
            command.actor().admin_user_id().to_string(),
            command.expected_version().value(),
            command.clock_ms().value(),
            command
                .occurred_at()
                .as_offset_datetime()
                .unix_timestamp_nanos(),
            c1_payload(command),
        )
        .as_bytes(),
    )
}

#[allow(clippy::too_many_arguments)]
fn canonical_base(
    command_name: &str,
    command_id: String,
    client_command_id: ClientCommandId,
    event_id: String,
    operator_lease_id: OperatorLeaseId,
    match_id: String,
    actor_id: String,
    expected_version: u64,
    clock_ms: u64,
    occurred_at_unix_ns: i128,
    payload: String,
) -> String {
    format!(
        "match-command-v1|name={command_name}|command={command_id}|client={client_command_id}|event={event_id}|operator_lease={operator_lease_id}|match={match_id}|actor={actor_id}|expected={expected_version}|clock_ms={clock_ms}|occurred_ns={occurred_at_unix_ns}|{payload}"
    )
}

fn lifecycle_payload(action: &LifecycleEventPayload) -> String {
    match action {
        LifecycleEventPayload::LineupSubmitted(lineup) => {
            let entries = lineup
                .entries()
                .iter()
                .map(|entry| {
                    let role = match entry.role() {
                        LineupRole::Starter => "starter",
                        LineupRole::Substitute => "substitute",
                    };
                    format!("{}:{role}", entry.player_id())
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "lineup_id={}|team_id={}|entries={entries}",
                lineup.lineup_id(),
                lineup.team_id()
            )
        }
        _ => "payload=empty".to_owned(),
    }
}

fn c1_payload(command: &C1LiveActionCommand) -> String {
    let period = command.period().as_persisted_str();
    match command.action() {
        C1LiveAction::RecordGoal(goal) => format!(
            "period={period}|team_id={}|scorer_id={}|award_type={}",
            goal.team_id(),
            goal.scorer_player_id()
                .map_or_else(|| "none".to_owned(), |id| id.to_string()),
            goal.award_type().as_str()
        ),
        C1LiveAction::RecordYellowCard(card) => {
            format!("period={period}|player_id={}|card=yellow", card.player_id())
        }
        C1LiveAction::RecordSecondYellowCard(card) => format!(
            "period={period}|player_id={}|card=second_yellow",
            card.player_id()
        ),
        C1LiveAction::RecordRedCard(card) => {
            format!("period={period}|player_id={}|card=red", card.player_id())
        }
        C1LiveAction::RecordSubstitution(substitution) => format!(
            "period={period}|team_id={}|player_on_id={}|player_off_id={}",
            substitution.team_id(),
            substitution.player_on_id(),
            substitution.player_off_id()
        ),
    }
}

#[cfg(test)]
mod tests {
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, C1LiveAction, C1LiveActionCommand,
        ClientCommandId, ClockMilliseconds, CommandId, EventId, EventVerificationStatus,
        GoalAwardType, GoalRecordedPayload, LifecycleEventPayload, MatchId, MatchLifecycleCommand,
        MatchPeriod, OperatorLeaseId, TeamId, UtcInstant,
    };
    use uuid::Uuid;

    use super::{fingerprint_c1_command, fingerprint_lifecycle_command};

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).unwrap()
    }

    #[test]
    fn canonical_fingerprint_is_deterministic_and_action_sensitive() {
        let command = |action| {
            MatchLifecycleCommand::new(
                CommandId::from_uuid(uuid("019ff900-3bc5-7cf0-b3d2-0728c077a930")),
                EventId::from_uuid(uuid("019ff900-3bc5-7cf1-b3d2-0728c077a930")),
                MatchId::from_uuid(uuid("019ff900-3bc5-7cf2-b3d2-0728c077a930")),
                AggregateVersion::initial(),
                ClockMilliseconds::zero(),
                UtcInstant::from_unix_timestamp(1_000).unwrap(),
                UtcInstant::from_unix_timestamp(1_001).unwrap(),
                ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                    "019ff900-3bc5-7cf3-b3d2-0728c077a930",
                ))),
                action,
            )
        };
        let client =
            ClientCommandId::try_from_uuid(uuid("019ff900-3bc5-7cf4-b3d2-0728c077a930")).unwrap();
        let lease = OperatorLeaseId::from_uuid(uuid("019ff900-3bc5-7cf5-b3d2-0728c077a930"));
        let scheduled = command(LifecycleEventPayload::MatchScheduled);
        assert_eq!(
            fingerprint_lifecycle_command(client, lease, &scheduled),
            fingerprint_lifecycle_command(client, lease, &scheduled)
        );
        assert_ne!(
            fingerprint_lifecycle_command(client, lease, &scheduled),
            fingerprint_lifecycle_command(
                client,
                lease,
                &command(LifecycleEventPayload::LineupConfirmed)
            )
        );
    }

    #[test]
    fn c1_fingerprint_ignores_recording_time_and_captures_payload() {
        let command = |recorded_at, team_id| {
            C1LiveActionCommand::try_new(
                CommandId::from_uuid(uuid("019ff901-3bc5-7cf0-b3d2-0728c077a930")),
                EventId::from_uuid(uuid("019ff901-3bc5-7cf1-b3d2-0728c077a930")),
                MatchId::from_uuid(uuid("019ff901-3bc5-7cf2-b3d2-0728c077a930")),
                AggregateVersion::from_value(4),
                MatchPeriod::FirstHalf,
                ClockMilliseconds::from_millis(2_000),
                UtcInstant::from_unix_timestamp(1_000).unwrap(),
                UtcInstant::from_unix_timestamp(recorded_at).unwrap(),
                ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                    "019ff901-3bc5-7cf3-b3d2-0728c077a930",
                ))),
                EventVerificationStatus::Provisional,
                C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                    TeamId::from_uuid(uuid(team_id)),
                    None,
                    GoalAwardType::Regular,
                )),
            )
            .unwrap()
        };
        let client =
            ClientCommandId::try_from_uuid(uuid("019ff901-3bc5-7cf4-b3d2-0728c077a930")).unwrap();
        let lease = OperatorLeaseId::from_uuid(uuid("019ff901-3bc5-7cf5-b3d2-0728c077a930"));
        let first = command(1_001, "019ff901-3bc5-7cf6-b3d2-0728c077a930");
        let later_recording = command(1_100, "019ff901-3bc5-7cf6-b3d2-0728c077a930");
        let different_team = command(1_001, "019ff901-3bc5-7cf7-b3d2-0728c077a930");

        assert_eq!(
            fingerprint_c1_command(client, lease, &first),
            fingerprint_c1_command(client, lease, &later_recording)
        );
        assert_ne!(
            fingerprint_c1_command(client, lease, &first),
            fingerprint_c1_command(client, lease, &different_team)
        );
    }
}
