use football_domain::{
    C1EventPayload, C1EventType, DomainEventEnvelope, EventId, EventSchemaVersion, EventType,
    GoalAwardType, GoalRecordedPayload, MatchPeriod, PlayerId, RedCardRecordedPayload,
    SecondYellowCardRecordedPayload, SubstitutionRecordedPayload, TeamId,
    YellowCardRecordedPayload,
};
use serde_json::{Map, Value, json};
use std::fmt;
use uuid::Uuid;

/// Error raised when a persisted C1 event payload cannot be validated or
/// converted to the typed domain payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C1EventPayloadCodecError {
    EventTypeMismatch {
        event_id: EventId,
        expected: C1EventType,
        actual: String,
    },
    SchemaVersionMismatch {
        event_id: EventId,
        event_type: C1EventType,
        expected: EventSchemaVersion,
        actual: EventSchemaVersion,
    },
    NonLiveEventPeriod {
        event_id: EventId,
        period: MatchPeriod,
    },
    UnsupportedEventType {
        event_id: EventId,
        actual: String,
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
    InvalidFieldType {
        event_id: EventId,
        field: &'static str,
    },
    InvalidUuid {
        event_id: EventId,
        field: &'static str,
        value: String,
    },
    InvalidAwardType {
        event_id: EventId,
        value: String,
    },
    InvalidSubstitution {
        event_id: EventId,
    },
}

impl fmt::Display for C1EventPayloadCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventTypeMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} payload is {expected}, envelope is {actual}"
            ),
            Self::SchemaVersionMismatch {
                event_id,
                event_type,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} type {event_type} has schema version {}, expected {}",
                actual.value(),
                expected.value()
            ),
            Self::NonLiveEventPeriod { event_id, period } => write!(
                formatter,
                "C1 event {event_id} cannot be encoded or decoded during non-live period {period:?}"
            ),
            Self::UnsupportedEventType { event_id, actual } => {
                write!(
                    formatter,
                    "event {event_id} has unsupported C1 type {actual}"
                )
            }
            Self::PayloadMustBeObject { event_id } => {
                write!(formatter, "event {event_id} payload must be a JSON object")
            }
            Self::MissingField { event_id, field } => {
                write!(
                    formatter,
                    "event {event_id} payload is missing field {field}"
                )
            }
            Self::UnknownField { event_id, field } => {
                write!(
                    formatter,
                    "event {event_id} payload has unknown field {field}"
                )
            }
            Self::InvalidFieldType { event_id, field } => {
                write!(
                    formatter,
                    "event {event_id} payload field {field} has an invalid type"
                )
            }
            Self::InvalidUuid {
                event_id,
                field,
                value,
            } => write!(
                formatter,
                "event {event_id} payload field {field} has invalid UUID {value:?}"
            ),
            Self::InvalidAwardType { event_id, value } => write!(
                formatter,
                "event {event_id} payload has invalid goal award type {value:?}"
            ),
            Self::InvalidSubstitution { event_id } => write!(
                formatter,
                "event {event_id} payload cannot substitute a player for themself"
            ),
        }
    }
}

impl std::error::Error for C1EventPayloadCodecError {}

/// Encodes one typed C1 payload into its version-1 persisted JSON shape.
pub fn encode_c1_event_payload(payload: &C1EventPayload) -> Value {
    match payload {
        C1EventPayload::GoalRecorded(payload) => json!({
            "team_id": payload.team_id().as_uuid().to_string(),
            "scorer_player_id": payload
                .scorer_player_id()
                .map(|player_id| player_id.as_uuid().to_string()),
            "award_type": payload.award_type().as_str(),
        }),
        C1EventPayload::YellowCardRecorded(payload) => json!({
            "player_id": payload.player_id().as_uuid().to_string(),
        }),
        C1EventPayload::SecondYellowCardRecorded(payload) => json!({
            "player_id": payload.player_id().as_uuid().to_string(),
        }),
        C1EventPayload::RedCardRecorded(payload) => json!({
            "player_id": payload.player_id().as_uuid().to_string(),
        }),
        C1EventPayload::SubstitutionRecorded(payload) => json!({
            "team_id": payload.team_id().as_uuid().to_string(),
            "player_on_id": payload.player_on_id().as_uuid().to_string(),
            "player_off_id": payload.player_off_id().as_uuid().to_string(),
        }),
    }
}

/// Converts a typed C1 envelope to the storage envelope after validating that
/// its envelope type and schema agree with its payload.
pub fn encode_c1_event_envelope(
    event: &DomainEventEnvelope<C1EventPayload>,
) -> Result<DomainEventEnvelope<Value>, C1EventPayloadCodecError> {
    let expected_type = event.payload().event_type();
    if event.event_type().as_str() != expected_type.as_str() {
        return Err(C1EventPayloadCodecError::EventTypeMismatch {
            event_id: event.event_id(),
            expected: expected_type,
            actual: event.event_type().as_str().to_owned(),
        });
    }

    let expected_schema = expected_type.schema_version();
    if event.event_schema_version() != expected_schema {
        return Err(C1EventPayloadCodecError::SchemaVersionMismatch {
            event_id: event.event_id(),
            event_type: expected_type,
            expected: expected_schema,
            actual: event.event_schema_version(),
        });
    }

    if !event.period().is_live() {
        return Err(C1EventPayloadCodecError::NonLiveEventPeriod {
            event_id: event.event_id(),
            period: event.period(),
        });
    }

    Ok(event
        .clone()
        .map_payload(encode_c1_event_payload(event.payload())))
}

/// Decodes a storage envelope into the typed C1 event envelope used by the
/// domain reducer.
pub fn decode_c1_event_envelope(
    event: DomainEventEnvelope<Value>,
) -> Result<DomainEventEnvelope<C1EventPayload>, C1EventPayloadCodecError> {
    if !event.period().is_live() {
        return Err(C1EventPayloadCodecError::NonLiveEventPeriod {
            event_id: event.event_id(),
            period: event.period(),
        });
    }

    let payload = decode_c1_event_payload(
        event.event_id(),
        event.event_type(),
        event.event_schema_version(),
        event.payload().clone(),
    )?;
    Ok(event.map_payload(payload))
}

/// Decodes one persisted C1 payload after resolving its event type and schema.
pub fn decode_c1_event_payload(
    event_id: EventId,
    event_type: &EventType,
    schema_version: EventSchemaVersion,
    payload: Value,
) -> Result<C1EventPayload, C1EventPayloadCodecError> {
    let c1_event_type = C1EventType::ALL
        .into_iter()
        .find(|candidate| candidate.as_str() == event_type.as_str())
        .ok_or_else(|| C1EventPayloadCodecError::UnsupportedEventType {
            event_id,
            actual: event_type.as_str().to_owned(),
        })?;

    let expected_schema = c1_event_type.schema_version();
    if schema_version != expected_schema {
        return Err(C1EventPayloadCodecError::SchemaVersionMismatch {
            event_id,
            event_type: c1_event_type,
            expected: expected_schema,
            actual: schema_version,
        });
    }

    let object = payload
        .as_object()
        .ok_or(C1EventPayloadCodecError::PayloadMustBeObject { event_id })?;

    match c1_event_type {
        C1EventType::GoalRecorded => decode_goal(event_id, object),
        C1EventType::YellowCardRecorded => decode_yellow_card(event_id, object),
        C1EventType::SecondYellowCardRecorded => decode_second_yellow(event_id, object),
        C1EventType::RedCardRecorded => decode_red_card(event_id, object),
        C1EventType::SubstitutionRecorded => decode_substitution(event_id, object),
    }
}

fn decode_goal(
    event_id: EventId,
    object: &Map<String, Value>,
) -> Result<C1EventPayload, C1EventPayloadCodecError> {
    ensure_fields(
        event_id,
        object,
        &["team_id", "scorer_player_id", "award_type"],
    )?;
    let team_id = TeamId::from_uuid(parse_uuid(event_id, object, "team_id")?);
    let scorer_player_id = match required_field(event_id, object, "scorer_player_id")? {
        Value::Null => None,
        Value::String(value) => Some(PlayerId::from_uuid(parse_uuid_value(
            event_id,
            "scorer_player_id",
            value,
        )?)),
        _ => {
            return Err(C1EventPayloadCodecError::InvalidFieldType {
                event_id,
                field: "scorer_player_id",
            });
        }
    };
    let award_type = match required_string(event_id, object, "award_type")? {
        "regular" => GoalAwardType::Regular,
        "penalty" => GoalAwardType::Penalty,
        "own_goal" => GoalAwardType::OwnGoal,
        value => {
            return Err(C1EventPayloadCodecError::InvalidAwardType {
                event_id,
                value: value.to_owned(),
            });
        }
    };

    Ok(C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
        team_id,
        scorer_player_id,
        award_type,
    )))
}

fn decode_yellow_card(
    event_id: EventId,
    object: &Map<String, Value>,
) -> Result<C1EventPayload, C1EventPayloadCodecError> {
    ensure_fields(event_id, object, &["player_id"])?;
    Ok(C1EventPayload::YellowCardRecorded(
        YellowCardRecordedPayload::new(PlayerId::from_uuid(parse_uuid(
            event_id,
            object,
            "player_id",
        )?)),
    ))
}

fn decode_second_yellow(
    event_id: EventId,
    object: &Map<String, Value>,
) -> Result<C1EventPayload, C1EventPayloadCodecError> {
    ensure_fields(event_id, object, &["player_id"])?;
    Ok(C1EventPayload::SecondYellowCardRecorded(
        SecondYellowCardRecordedPayload::new(PlayerId::from_uuid(parse_uuid(
            event_id,
            object,
            "player_id",
        )?)),
    ))
}

fn decode_red_card(
    event_id: EventId,
    object: &Map<String, Value>,
) -> Result<C1EventPayload, C1EventPayloadCodecError> {
    ensure_fields(event_id, object, &["player_id"])?;
    Ok(C1EventPayload::RedCardRecorded(
        RedCardRecordedPayload::new(PlayerId::from_uuid(parse_uuid(
            event_id,
            object,
            "player_id",
        )?)),
    ))
}

fn decode_substitution(
    event_id: EventId,
    object: &Map<String, Value>,
) -> Result<C1EventPayload, C1EventPayloadCodecError> {
    ensure_fields(
        event_id,
        object,
        &["team_id", "player_on_id", "player_off_id"],
    )?;
    let payload = SubstitutionRecordedPayload::try_new(
        TeamId::from_uuid(parse_uuid(event_id, object, "team_id")?),
        PlayerId::from_uuid(parse_uuid(event_id, object, "player_on_id")?),
        PlayerId::from_uuid(parse_uuid(event_id, object, "player_off_id")?),
    )
    .map_err(|_| C1EventPayloadCodecError::InvalidSubstitution { event_id })?;
    Ok(C1EventPayload::SubstitutionRecorded(payload))
}

fn ensure_fields(
    event_id: EventId,
    object: &Map<String, Value>,
    expected_fields: &[&'static str],
) -> Result<(), C1EventPayloadCodecError> {
    for field in expected_fields {
        if !object.contains_key(*field) {
            return Err(C1EventPayloadCodecError::MissingField { event_id, field });
        }
    }

    for field in object.keys() {
        if !expected_fields.contains(&field.as_str()) {
            return Err(C1EventPayloadCodecError::UnknownField {
                event_id,
                field: field.clone(),
            });
        }
    }
    Ok(())
}

fn required_field<'a>(
    event_id: EventId,
    object: &'a Map<String, Value>,
    field: &'static str,
) -> Result<&'a Value, C1EventPayloadCodecError> {
    object
        .get(field)
        .ok_or(C1EventPayloadCodecError::MissingField { event_id, field })
}

fn required_string<'a>(
    event_id: EventId,
    object: &'a Map<String, Value>,
    field: &'static str,
) -> Result<&'a str, C1EventPayloadCodecError> {
    required_field(event_id, object, field)?
        .as_str()
        .ok_or(C1EventPayloadCodecError::InvalidFieldType { event_id, field })
}

fn parse_uuid(
    event_id: EventId,
    object: &Map<String, Value>,
    field: &'static str,
) -> Result<Uuid, C1EventPayloadCodecError> {
    let value = required_string(event_id, object, field)?;
    parse_uuid_value(event_id, field, value)
}

fn parse_uuid_value(
    event_id: EventId,
    field: &'static str,
    value: &str,
) -> Result<Uuid, C1EventPayloadCodecError> {
    Uuid::parse_str(value).map_err(|_| C1EventPayloadCodecError::InvalidUuid {
        event_id,
        field,
        value: value.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        C1EventPayloadCodecError, decode_c1_event_envelope, decode_c1_event_payload,
        encode_c1_event_envelope, encode_c1_event_payload,
    };
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, C1EventType,
        ClockMilliseconds, CommandId, DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion,
        EventType, EventVerificationStatus, GoalAwardType, GoalRecordedPayload, MatchId,
        MatchPeriod, PlayerId, RedCardRecordedPayload, SecondYellowCardRecordedPayload,
        SubstitutionRecordedPayload, TeamId, UtcInstant, YellowCardRecordedPayload,
    };
    use serde_json::{Value, json};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("C1 payload fixture UUID should be valid")
    }

    fn event_id() -> EventId {
        EventId::from_uuid(uuid("019fe360-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn team_id() -> TeamId {
        TeamId::from_uuid(uuid("019fe360-3bc5-7cf1-b3d2-0728c077a930"))
    }

    fn player_id(value: &str) -> PlayerId {
        PlayerId::from_uuid(uuid(value))
    }

    fn event(payload: C1EventPayload) -> DomainEventEnvelope<C1EventPayload> {
        event_in_period(payload, MatchPeriod::FirstHalf)
    }

    fn event_in_period(
        payload: C1EventPayload,
        period: MatchPeriod,
    ) -> DomainEventEnvelope<C1EventPayload> {
        let event_type = payload.event_type();
        DomainEventEnvelope::new(
            event_id(),
            MatchId::from_uuid(uuid("019fe360-3bc5-7cf2-b3d2-0728c077a930")),
            AggregateVersion::from_value(1),
            CommandId::from_uuid(uuid("019fe360-3bc5-7cf3-b3d2-0728c077a930")),
            EventIndex::initial(),
            event_type.try_as_event_type().unwrap(),
            event_type.schema_version(),
            period,
            ClockMilliseconds::from_millis(12_000),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe360-3bc5-7cf4-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            payload,
            None,
            None,
        )
    }

    #[test]
    fn encodes_and_decodes_every_c1_payload_without_losing_meaning() {
        let substitution = SubstitutionRecordedPayload::try_new(
            team_id(),
            player_id("019fe360-3cf0-7cf0-b3d2-0728c077a930"),
            player_id("019fe360-3cf1-7cf0-b3d2-0728c077a930"),
        )
        .unwrap();
        let payloads = vec![
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                team_id(),
                Some(player_id("019fe360-3cf2-7cf0-b3d2-0728c077a930")),
                GoalAwardType::Regular,
            )),
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::Penalty,
            )),
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::OwnGoal,
            )),
            C1EventPayload::YellowCardRecorded(YellowCardRecordedPayload::new(player_id(
                "019fe360-3cf3-7cf0-b3d2-0728c077a930",
            ))),
            C1EventPayload::SecondYellowCardRecorded(SecondYellowCardRecordedPayload::new(
                player_id("019fe360-3cf4-7cf0-b3d2-0728c077a930"),
            )),
            C1EventPayload::RedCardRecorded(RedCardRecordedPayload::new(player_id(
                "019fe360-3cf5-7cf0-b3d2-0728c077a930",
            ))),
            C1EventPayload::SubstitutionRecorded(substitution),
        ];

        for payload in payloads {
            let encoded = encode_c1_event_payload(&payload);
            let event_type = payload.event_type().try_as_event_type().unwrap();
            let decoded =
                decode_c1_event_payload(event_id(), &event_type, payload.schema_version(), encoded)
                    .unwrap();
            assert_eq!(decoded, payload);
        }
    }

    #[test]
    fn envelope_codec_preserves_metadata_and_rejects_mismatched_metadata() {
        let typed = event(C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
            team_id(),
            None,
            GoalAwardType::Penalty,
        )));
        let encoded = encode_c1_event_envelope(&typed).unwrap();
        let decoded = decode_c1_event_envelope(encoded).unwrap();

        assert_eq!(decoded, typed);
        assert_eq!(decoded.event_id(), typed.event_id());
        assert_eq!(decoded.aggregate_version(), typed.aggregate_version());
        assert_eq!(decoded.causation_command_id(), typed.causation_command_id());
        assert_eq!(decoded.event_index(), typed.event_index());

        let mismatched = DomainEventEnvelope::new(
            typed.event_id(),
            typed.match_id(),
            typed.aggregate_version(),
            typed.causation_command_id(),
            typed.event_index(),
            C1EventType::YellowCardRecorded.try_as_event_type().unwrap(),
            typed.event_schema_version(),
            typed.period(),
            typed.clock_ms(),
            typed.occurred_at(),
            typed.recorded_at(),
            typed.actor(),
            typed.verification_status(),
            typed.payload().clone(),
            None,
            None,
        );
        assert!(matches!(
            encode_c1_event_envelope(&mismatched),
            Err(C1EventPayloadCodecError::EventTypeMismatch { .. })
        ));
    }

    #[test]
    fn rejects_c1_envelopes_outside_live_periods_before_persistence_or_replay() {
        let typed = event_in_period(
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::Regular,
            )),
            MatchPeriod::HalfTime,
        );

        assert!(matches!(
            encode_c1_event_envelope(&typed),
            Err(C1EventPayloadCodecError::NonLiveEventPeriod {
                period: MatchPeriod::HalfTime,
                ..
            })
        ));

        let raw = typed
            .clone()
            .map_payload(encode_c1_event_payload(typed.payload()));
        assert!(matches!(
            decode_c1_event_envelope(raw),
            Err(C1EventPayloadCodecError::NonLiveEventPeriod {
                period: MatchPeriod::HalfTime,
                ..
            })
        ));
    }

    #[test]
    fn rejects_unknown_fields_missing_fields_and_invalid_values() {
        let event_type = C1EventType::GoalRecorded.try_as_event_type().unwrap();
        let schema = EventSchemaVersion::initial();

        assert!(matches!(
            decode_c1_event_payload(
                event_id(),
                &event_type,
                schema,
                json!({
                    "team_id": team_id().as_uuid().to_string(),
                    "scorer_player_id": null,
                    "award_type": "regular",
                    "extra": true,
                }),
            ),
            Err(C1EventPayloadCodecError::UnknownField { field, .. }) if field == "extra"
        ));

        assert!(matches!(
            decode_c1_event_payload(
                event_id(),
                &event_type,
                schema,
                json!({"team_id": team_id().as_uuid().to_string()}),
            ),
            Err(C1EventPayloadCodecError::MissingField {
                field: "scorer_player_id",
                ..
            })
        ));

        assert!(matches!(
            decode_c1_event_payload(
                event_id(),
                &event_type,
                schema,
                json!({
                    "team_id": "not-a-uuid",
                    "scorer_player_id": null,
                    "award_type": "regular",
                }),
            ),
            Err(C1EventPayloadCodecError::InvalidUuid {
                field: "team_id",
                ..
            })
        ));

        assert!(matches!(
            decode_c1_event_payload(event_id(), &event_type, schema, Value::Null),
            Err(C1EventPayloadCodecError::PayloadMustBeObject { .. })
        ));
    }

    #[test]
    fn rejects_unknown_event_type_schema_and_invalid_substitution() {
        let unknown_type = EventType::try_new("UnknownC1Event").unwrap();
        assert!(matches!(
            decode_c1_event_payload(
                event_id(),
                &unknown_type,
                EventSchemaVersion::initial(),
                json!({}),
            ),
            Err(C1EventPayloadCodecError::UnsupportedEventType { .. })
        ));

        let goal_type = C1EventType::GoalRecorded.try_as_event_type().unwrap();
        let schema = EventSchemaVersion::try_new(2).unwrap();
        assert!(matches!(
            decode_c1_event_payload(event_id(), &goal_type, schema, json!({})),
            Err(C1EventPayloadCodecError::SchemaVersionMismatch { .. })
        ));

        let substitution_type = C1EventType::SubstitutionRecorded
            .try_as_event_type()
            .unwrap();
        let same_player = player_id("019fe360-3cf6-7cf0-b3d2-0728c077a930")
            .as_uuid()
            .to_string();
        assert!(matches!(
            decode_c1_event_payload(
                event_id(),
                &substitution_type,
                EventSchemaVersion::initial(),
                json!({
                    "team_id": team_id().as_uuid().to_string(),
                    "player_on_id": same_player,
                    "player_off_id": same_player,
                }),
            ),
            Err(C1EventPayloadCodecError::InvalidSubstitution { .. })
        ));
    }
}
