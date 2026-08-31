use football_application::commands::fingerprint_canonical_bytes;
use football_contracts::CommandMetadata;
use football_domain::{
    AggregateVersion, ClientCommandId, ClockMilliseconds, CommandId, EventId, MatchId,
    OperatorLeaseId, RequestFingerprint, UtcInstant,
};
use football_infrastructure::MatchCommandDispatch;
use football_platform::RequestId;
use serde::Serialize;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::{Uuid, Version};

use crate::error::ApiCommandError;

const COMMAND_PROCESSING_LEASE_SECONDS: i64 = 30;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParsedCommandMetadata {
    pub(crate) command_id: CommandId,
    pub(crate) client_command_id: ClientCommandId,
    pub(crate) event_id: EventId,
    pub(crate) operator_lease_id: OperatorLeaseId,
    pub(crate) expected_version: AggregateVersion,
    pub(crate) clock_ms: ClockMilliseconds,
    pub(crate) occurred_at: UtcInstant,
}

impl ParsedCommandMetadata {
    pub(crate) fn parse(
        metadata: &CommandMetadata,
        request_id: RequestId,
    ) -> Result<Self, ApiCommandError> {
        let client_meta = Some(metadata.client_command_id.as_str());
        let client_uuid = parse_uuidv7(
            &metadata.client_command_id,
            "client_command_id",
            request_id,
            client_meta,
        )?;
        let client_command_id = ClientCommandId::try_from_uuid(client_uuid).map_err(|_| {
            ApiCommandError::invalid_value(
                request_id,
                client_meta,
                "client_command_id",
                "client_command_id must be UUIDv7",
            )
        })?;
        let occurred_at = OffsetDateTime::parse(&metadata.occurred_at, &Rfc3339)
            .map(UtcInstant::from_offset_datetime)
            .map_err(|_| {
                ApiCommandError::invalid_value(
                    request_id,
                    client_meta,
                    "occurred_at",
                    "occurred_at must be an RFC3339 timestamp with an explicit offset",
                )
            })?;

        Ok(Self {
            command_id: CommandId::from_uuid(parse_uuidv7(
                &metadata.command_id,
                "command_id",
                request_id,
                client_meta,
            )?),
            client_command_id,
            event_id: EventId::from_uuid(parse_uuidv7(
                &metadata.event_id,
                "event_id",
                request_id,
                client_meta,
            )?),
            operator_lease_id: OperatorLeaseId::from_uuid(parse_uuidv7(
                &metadata.operator_lease_id,
                "operator_lease_id",
                request_id,
                client_meta,
            )?),
            expected_version: AggregateVersion::from_value(metadata.expected_version),
            clock_ms: ClockMilliseconds::from_millis(metadata.clock_ms),
            occurred_at,
        })
    }

    pub(crate) fn dispatch(
        self,
        fingerprint: RequestFingerprint,
        now_utc: UtcInstant,
        request_id: RequestId,
    ) -> Result<MatchCommandDispatch, ApiCommandError> {
        let expires_at = now_utc
            .as_offset_datetime()
            .checked_add(Duration::seconds(COMMAND_PROCESSING_LEASE_SECONDS))
            .map(UtcInstant::from_offset_datetime)
            .ok_or_else(|| {
                ApiCommandError::invalid_value(
                    request_id,
                    Some(&self.client_command_id.to_string()),
                    "occurred_at",
                    "command processing time is outside the supported range",
                )
            })?;
        Ok(MatchCommandDispatch::new(
            self.client_command_id,
            fingerprint,
            Uuid::now_v7(),
            now_utc,
            expires_at,
        ))
    }
}

#[derive(Serialize)]
struct FingerprintEnvelope<'a, T> {
    operation: &'static str,
    match_id: String,
    actor_id: String,
    request: &'a T,
}

pub(crate) fn request_fingerprint<T: Serialize>(
    operation: &'static str,
    match_id: MatchId,
    actor_id: football_domain::AdminUserId,
    request: &T,
    request_id: RequestId,
    client_command_id: &str,
) -> Result<RequestFingerprint, ApiCommandError> {
    serde_json::to_vec(&FingerprintEnvelope {
        operation,
        match_id: match_id.to_string(),
        actor_id: actor_id.to_string(),
        request,
    })
    .map(|bytes| fingerprint_canonical_bytes(&bytes))
    .map_err(|_| {
        ApiCommandError::new(
            football_platform::ApplicationError::new(
                football_platform::ErrorCode::InternalUnexpected,
                "the command fingerprint could not be constructed",
            ),
            request_id,
            Some(client_command_id),
        )
    })
}

pub(crate) fn parse_match_id(
    value: &str,
    request_id: RequestId,
    client_command_id: Option<&str>,
) -> Result<MatchId, ApiCommandError> {
    parse_uuidv7(value, "match_id", request_id, client_command_id).map(MatchId::from_uuid)
}

pub(crate) fn parse_uuidv7(
    value: &str,
    field: &'static str,
    request_id: RequestId,
    client_command_id: Option<&str>,
) -> Result<Uuid, ApiCommandError> {
    Uuid::parse_str(value)
        .ok()
        .filter(|uuid| uuid.get_version() == Some(Version::SortRand) && uuid.to_string() == value)
        .ok_or_else(|| {
            ApiCommandError::invalid_value(
                request_id,
                client_command_id,
                field,
                format!("{field} must be a canonical UUIDv7"),
            )
        })
}

pub(crate) fn current_utc_instant(
    request_id: RequestId,
    client_command_id: Option<&str>,
) -> Result<UtcInstant, ApiCommandError> {
    let now = OffsetDateTime::now_utc();
    let microseconds = (now.nanosecond() / 1_000) * 1_000;
    now.replace_nanosecond(microseconds)
        .map(UtcInstant::from_offset_datetime)
        .map_err(|_| {
            ApiCommandError::new(
                football_platform::ApplicationError::new(
                    football_platform::ErrorCode::InternalUnexpected,
                    "the server UTC clock is outside the supported range",
                ),
                request_id,
                client_command_id,
            )
        })
}
