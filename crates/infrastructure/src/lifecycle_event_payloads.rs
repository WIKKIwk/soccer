use football_domain::{
    DomainEventEnvelope, EventId, EventSchemaVersion, EventVerificationStatus,
    LifecycleEventPayload, LifecycleEventType, LineupEntry, LineupId, LineupRole,
    LineupSubmittedPayload, LineupValidationError, MatchEventPayload, PlayerId, TeamId,
};
use serde_json::{Map, Value, json};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEventPayloadCodecError {
    UnsupportedEventType {
        event_id: EventId,
        actual: String,
    },
    EventTypeMismatch {
        event_id: EventId,
        expected: LifecycleEventType,
        actual: String,
    },
    SchemaVersionMismatch {
        event_id: EventId,
        expected: EventSchemaVersion,
        actual: EventSchemaVersion,
    },
    VerificationStatusMismatch {
        event_id: EventId,
        actual: EventVerificationStatus,
    },
    UnexpectedCorrectionReference {
        event_id: EventId,
    },
    PayloadMustBeObject {
        event_id: EventId,
    },
    MissingField {
        event_id: EventId,
        field: &'static str,
    },
    UnknownField {
        event_id: EventId,
        field: String,
    },
    InvalidField {
        event_id: EventId,
        field: &'static str,
    },
    InvalidLineup {
        event_id: EventId,
        source: LineupValidationError,
    },
}

impl fmt::Display for LifecycleEventPayloadCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEventType { event_id, actual } => {
                write!(
                    formatter,
                    "event {event_id} has unsupported lifecycle type {actual}"
                )
            }
            Self::EventTypeMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} lifecycle payload expects {expected}, envelope is {actual}"
            ),
            Self::SchemaVersionMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} lifecycle schema is {}, expected {}",
                actual.value(),
                expected.value()
            ),
            Self::VerificationStatusMismatch { event_id, actual } => write!(
                formatter,
                "event {event_id} lifecycle verification status is {}",
                actual.as_persisted_str()
            ),
            Self::UnexpectedCorrectionReference { event_id } => {
                write!(
                    formatter,
                    "event {event_id} lifecycle marker has a correction reference"
                )
            }
            Self::PayloadMustBeObject { event_id } => {
                write!(
                    formatter,
                    "event {event_id} lifecycle payload must be an object"
                )
            }
            Self::MissingField { event_id, field } => {
                write!(
                    formatter,
                    "event {event_id} lifecycle payload misses {field}"
                )
            }
            Self::UnknownField { event_id, field } => {
                write!(
                    formatter,
                    "event {event_id} lifecycle payload has unknown {field}"
                )
            }
            Self::InvalidField { event_id, field } => {
                write!(
                    formatter,
                    "event {event_id} lifecycle payload has invalid {field}"
                )
            }
            Self::InvalidLineup { event_id, source } => {
                write!(
                    formatter,
                    "event {event_id} lifecycle lineup is invalid: {source}"
                )
            }
        }
    }
}

impl std::error::Error for LifecycleEventPayloadCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLineup { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn encode_lifecycle_event_envelope(
    event: &DomainEventEnvelope<MatchEventPayload>,
    payload: &LifecycleEventPayload,
) -> Result<DomainEventEnvelope<Value>, LifecycleEventPayloadCodecError> {
    validate_envelope(event, payload.event_type())?;
    let value = match payload {
        LifecycleEventPayload::LineupSubmitted(lineup) => json!({
            "lineup_id": lineup.lineup_id().to_string(),
            "team_id": lineup.team_id().to_string(),
            "entries": lineup.entries().iter().map(|entry| json!({
                "player_id": entry.player_id().to_string(),
                "role": entry.role().as_str(),
            })).collect::<Vec<_>>(),
        }),
        LifecycleEventPayload::MatchScheduled
        | LifecycleEventPayload::LineupConfirmed
        | LifecycleEventPayload::FirstHalfStarted
        | LifecycleEventPayload::FirstHalfEnded
        | LifecycleEventPayload::SecondHalfStarted
        | LifecycleEventPayload::MatchFinished => json!({}),
    };
    Ok(event.clone().map_payload(value))
}

pub fn decode_lifecycle_event_envelope(
    event: DomainEventEnvelope<Value>,
) -> Result<DomainEventEnvelope<MatchEventPayload>, LifecycleEventPayloadCodecError> {
    let event_type =
        LifecycleEventType::try_from_str(event.event_type().as_str()).ok_or_else(|| {
            LifecycleEventPayloadCodecError::UnsupportedEventType {
                event_id: event.event_id(),
                actual: event.event_type().as_str().to_owned(),
            }
        })?;
    validate_envelope(&event, event_type)?;
    let object = event.payload().as_object().ok_or(
        LifecycleEventPayloadCodecError::PayloadMustBeObject {
            event_id: event.event_id(),
        },
    )?;
    let payload = match event_type {
        LifecycleEventType::LineupSubmitted => decode_lineup(event.event_id(), object)?,
        LifecycleEventType::MatchScheduled
        | LifecycleEventType::LineupConfirmed
        | LifecycleEventType::FirstHalfStarted
        | LifecycleEventType::FirstHalfEnded
        | LifecycleEventType::SecondHalfStarted
        | LifecycleEventType::MatchFinished => {
            ensure_fields(event.event_id(), object, &[])?;
            empty_payload(event_type)
        }
    };
    Ok(event.map_payload(MatchEventPayload::Lifecycle(payload)))
}

fn decode_lineup(
    event_id: EventId,
    object: &Map<String, Value>,
) -> Result<LifecycleEventPayload, LifecycleEventPayloadCodecError> {
    ensure_fields(event_id, object, &["lineup_id", "team_id", "entries"])?;
    let lineup_id = LineupId::from_uuid(parse_uuid(event_id, object, "lineup_id")?);
    let team_id = TeamId::from_uuid(parse_uuid(event_id, object, "team_id")?);
    let entries = object
        .get("entries")
        .and_then(Value::as_array)
        .ok_or(LifecycleEventPayloadCodecError::InvalidField {
            event_id,
            field: "entries",
        })?
        .iter()
        .map(|value| decode_entry(event_id, value))
        .collect::<Result<Vec<_>, _>>()?;
    let payload = LineupSubmittedPayload::try_new(lineup_id, team_id, entries)
        .map_err(|source| LifecycleEventPayloadCodecError::InvalidLineup { event_id, source })?;
    Ok(LifecycleEventPayload::LineupSubmitted(payload))
}

fn decode_entry(
    event_id: EventId,
    value: &Value,
) -> Result<LineupEntry, LifecycleEventPayloadCodecError> {
    let object = value
        .as_object()
        .ok_or(LifecycleEventPayloadCodecError::InvalidField {
            event_id,
            field: "entries",
        })?;
    ensure_fields(event_id, object, &["player_id", "role"])?;
    let player_id = PlayerId::from_uuid(parse_uuid(event_id, object, "player_id")?);
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .and_then(LineupRole::try_from_persisted_str)
        .ok_or(LifecycleEventPayloadCodecError::InvalidField {
            event_id,
            field: "role",
        })?;
    Ok(LineupEntry::new(player_id, role))
}

fn parse_uuid(
    event_id: EventId,
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Uuid, LifecycleEventPayloadCodecError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(LifecycleEventPayloadCodecError::InvalidField { event_id, field })
}

fn empty_payload(event_type: LifecycleEventType) -> LifecycleEventPayload {
    match event_type {
        LifecycleEventType::MatchScheduled => LifecycleEventPayload::MatchScheduled,
        LifecycleEventType::LineupConfirmed => LifecycleEventPayload::LineupConfirmed,
        LifecycleEventType::FirstHalfStarted => LifecycleEventPayload::FirstHalfStarted,
        LifecycleEventType::FirstHalfEnded => LifecycleEventPayload::FirstHalfEnded,
        LifecycleEventType::SecondHalfStarted => LifecycleEventPayload::SecondHalfStarted,
        LifecycleEventType::MatchFinished => LifecycleEventPayload::MatchFinished,
        LifecycleEventType::LineupSubmitted => {
            unreachable!("lineup payload is decoded separately")
        }
    }
}

fn validate_envelope(
    event: &DomainEventEnvelope<impl Sized>,
    expected_type: LifecycleEventType,
) -> Result<(), LifecycleEventPayloadCodecError> {
    if event.event_type().as_str() != expected_type.as_str() {
        return Err(LifecycleEventPayloadCodecError::EventTypeMismatch {
            event_id: event.event_id(),
            expected: expected_type,
            actual: event.event_type().as_str().to_owned(),
        });
    }
    if event.event_schema_version() != expected_type.schema_version() {
        return Err(LifecycleEventPayloadCodecError::SchemaVersionMismatch {
            event_id: event.event_id(),
            expected: expected_type.schema_version(),
            actual: event.event_schema_version(),
        });
    }
    if event.verification_status() != EventVerificationStatus::Provisional {
        return Err(
            LifecycleEventPayloadCodecError::VerificationStatusMismatch {
                event_id: event.event_id(),
                actual: event.verification_status(),
            },
        );
    }
    if event.supersedes_event_id().is_some() || event.voids_event_id().is_some() {
        return Err(
            LifecycleEventPayloadCodecError::UnexpectedCorrectionReference {
                event_id: event.event_id(),
            },
        );
    }
    Ok(())
}

fn ensure_fields(
    event_id: EventId,
    object: &Map<String, Value>,
    expected: &[&'static str],
) -> Result<(), LifecycleEventPayloadCodecError> {
    for field in expected {
        if !object.contains_key(*field) {
            return Err(LifecycleEventPayloadCodecError::MissingField { event_id, field });
        }
    }
    if let Some(field) = object
        .keys()
        .find(|field| !expected.contains(&field.as_str()))
    {
        return Err(LifecycleEventPayloadCodecError::UnknownField {
            event_id,
            field: field.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{decode_lifecycle_event_envelope, encode_lifecycle_event_envelope};
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, ClockMilliseconds, CommandId,
        DomainEventEnvelope, EventId, EventIndex, EventVerificationStatus, LifecycleEventPayload,
        LifecycleEventType, MatchEventPayload, MatchId, MatchPeriod, UtcInstant,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("lifecycle codec fixture UUID should be valid")
    }

    #[test]
    fn empty_lifecycle_payload_round_trips_strictly() {
        let payload = LifecycleEventPayload::FirstHalfStarted;
        let event = DomainEventEnvelope::new(
            EventId::from_uuid(uuid("019ff590-3bc5-7cf0-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019ff590-3bc5-7cf1-b3d2-0728c077a930")),
            AggregateVersion::from_value(1),
            CommandId::from_uuid(uuid("019ff590-3bc5-7cf2-b3d2-0728c077a930")),
            EventIndex::initial(),
            LifecycleEventType::FirstHalfStarted
                .try_as_event_type()
                .unwrap(),
            LifecycleEventType::FirstHalfStarted.schema_version(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::zero(),
            UtcInstant::from_unix_timestamp(1_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_001).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019ff590-3bc5-7cf3-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            MatchEventPayload::Lifecycle(payload),
            None,
            None,
        );
        let encoded = encode_lifecycle_event_envelope(
            &event,
            match event.payload() {
                MatchEventPayload::Lifecycle(payload) => payload,
                _ => unreachable!(),
            },
        )
        .unwrap();
        assert_eq!(encoded.payload(), &serde_json::json!({}));
        assert_eq!(decode_lifecycle_event_envelope(encoded).unwrap(), event);
    }
}
