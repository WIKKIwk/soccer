use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ADMIN_COMMAND_RESPONSE_VERSION: u16 = 1;
pub const DEVELOPMENT_ADMIN_ID_HEADER: &str = "x-development-admin-id";
pub const DEVELOPMENT_AUTHORIZATION_HEADER: &str = "x-development-authorization";
pub const DEVELOPMENT_AUTHORIZATION_VALUE: &str = "1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandMetadata {
    pub command_id: String,
    pub client_command_id: String,
    pub event_id: String,
    pub operator_lease_id: String,
    pub expected_version: u64,
    pub clock_ms: u64,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleCommandRequest {
    pub command: CommandMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineupSubmitCommandRequest {
    pub command: CommandMetadata,
    pub lineup_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalAwardKind {
    Regular,
    Penalty,
    OwnGoal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LivePeriodKind {
    FirstHalf,
    SecondHalf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalCommandRequest {
    pub command: CommandMetadata,
    pub period: LivePeriodKind,
    pub team_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scorer_player_id: Option<String>,
    pub award_type: GoalAwardKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Yellow,
    SecondYellow,
    Red,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardCommandRequest {
    pub command: CommandMetadata,
    pub period: LivePeriodKind,
    pub player_id: String,
    pub card: CardKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionCommandRequest {
    pub command: CommandMetadata,
    pub period: LivePeriodKind,
    pub team_id: String,
    pub player_on_id: String,
    pub player_off_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdminCommandResponse {
    pub meta: AdminCommandResponseMeta,
    pub data: AdminCommandResponseData,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdminCommandResponseMeta {
    pub version: u16,
    pub request_id: String,
    pub client_command_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdminCommandResponseData {
    pub command_id: String,
    pub match_id: String,
    pub aggregate_version: u64,
    pub event_count: u64,
    #[serde(flatten)]
    pub outcome: AdminCommandOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AdminCommandOutcome {
    Accepted { result: Value },
    Rejected { rejection: AdminCommandRejection },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdminCommandRejection {
    pub code: String,
    pub details: Value,
}

impl AdminCommandResponse {
    #[allow(clippy::too_many_arguments)]
    pub fn accepted(
        request_id: impl Into<String>,
        client_command_id: impl Into<String>,
        command_id: impl Into<String>,
        match_id: impl Into<String>,
        aggregate_version: u64,
        event_count: u64,
        result: Value,
    ) -> Self {
        Self::new(
            request_id,
            client_command_id,
            command_id,
            match_id,
            aggregate_version,
            event_count,
            AdminCommandOutcome::Accepted { result },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rejected(
        request_id: impl Into<String>,
        client_command_id: impl Into<String>,
        command_id: impl Into<String>,
        match_id: impl Into<String>,
        aggregate_version: u64,
        code: impl Into<String>,
        details: Value,
    ) -> Self {
        Self::new(
            request_id,
            client_command_id,
            command_id,
            match_id,
            aggregate_version,
            0,
            AdminCommandOutcome::Rejected {
                rejection: AdminCommandRejection {
                    code: code.into(),
                    details,
                },
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        request_id: impl Into<String>,
        client_command_id: impl Into<String>,
        command_id: impl Into<String>,
        match_id: impl Into<String>,
        aggregate_version: u64,
        event_count: u64,
        outcome: AdminCommandOutcome,
    ) -> Self {
        Self {
            meta: AdminCommandResponseMeta {
                version: ADMIN_COMMAND_RESPONSE_VERSION,
                request_id: request_id.into(),
                client_command_id: client_command_id.into(),
            },
            data: AdminCommandResponseData {
                command_id: command_id.into(),
                match_id: match_id.into(),
                aggregate_version,
                event_count,
                outcome,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{from_value, json, to_value};

    use super::{
        ADMIN_COMMAND_RESPONSE_VERSION, AdminCommandResponse, CardCommandRequest, CardKind,
        CommandMetadata,
    };

    fn metadata() -> CommandMetadata {
        CommandMetadata {
            command_id: "command-1".to_owned(),
            client_command_id: "client-1".to_owned(),
            event_id: "event-1".to_owned(),
            operator_lease_id: "lease-1".to_owned(),
            expected_version: 4,
            clock_ms: 12_000,
            occurred_at: "2026-08-31T10:00:00Z".to_owned(),
        }
    }

    #[test]
    fn command_requests_are_strict_and_keep_card_meaning() {
        let request: CardCommandRequest = from_value(json!({
            "command": metadata(),
            "period": "first_half",
            "player_id": "player-1",
            "card": "second_yellow"
        }))
        .unwrap();
        assert_eq!(request.card, CardKind::SecondYellow);

        assert!(
            from_value::<CardCommandRequest>(json!({
            "command": metadata(),
            "period": "first_half",
            "player_id": "player-1",
                "card": "blue"
            }))
            .is_err()
        );
    }

    #[test]
    fn serializes_idempotent_terminal_command_shape() {
        let response = AdminCommandResponse::accepted(
            "request-1",
            "client-1",
            "command-1",
            "match-1",
            5,
            1,
            json!({"event_type": "GoalRecorded"}),
        );
        let value = to_value(response).unwrap();
        assert_eq!(value["meta"]["version"], ADMIN_COMMAND_RESPONSE_VERSION);
        assert_eq!(value["data"]["outcome"], "accepted");
        assert_eq!(value["data"]["aggregate_version"], 5);
    }
}
