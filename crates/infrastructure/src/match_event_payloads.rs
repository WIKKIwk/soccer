use crate::c1_event_payloads::{
    C1EventPayloadCodecError, decode_c1_event_envelope, encode_c1_event_envelope,
};
use football_domain::{
    C1EventType, CorrectionEventPayload, CorrectionEventType, CorrectionReason,
    DomainEventEnvelope, EventId, EventSchemaVersion, EventVerificationStatus, EventVoidedPayload,
    MatchEventPayload, MatchReopenedPayload, MatchVerifiedPayload, VerificationEventPayload,
    VerificationEventType,
};
use serde_json::{Map, Value, json};
use std::fmt;

/// Codec errors for the typed payload union of the canonical match stream.
///
/// C1 payload errors retain the existing C1 codec details. Correction errors
/// additionally validate the envelope references because `EventVoided`
/// carries its target in `voids_event_id`, not in the JSON body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchEventPayloadCodecError {
    C1(C1EventPayloadCodecError),
    UnsupportedEventType {
        event_id: EventId,
        actual: String,
    },
    EventTypeMismatch {
        event_id: EventId,
        expected: &'static str,
        actual: String,
    },
    SchemaVersionMismatch {
        event_id: EventId,
        event_type: &'static str,
        expected: EventSchemaVersion,
        actual: EventSchemaVersion,
    },
    VerificationStatusMismatch {
        event_id: EventId,
        event_type: &'static str,
        expected: EventVerificationStatus,
        actual: EventVerificationStatus,
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
    EmptyReason {
        event_id: EventId,
    },
    MissingCorrectionReference {
        event_id: EventId,
        field: &'static str,
    },
    UnexpectedCorrectionReference {
        event_id: EventId,
        field: &'static str,
    },
    SelfReferentialCorrection {
        event_id: EventId,
    },
}

impl fmt::Display for MatchEventPayloadCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::C1(error) => error.fmt(formatter),
            Self::UnsupportedEventType { event_id, actual } => {
                write!(
                    formatter,
                    "event {event_id} has unsupported match event type {actual:?}"
                )
            }
            Self::EventTypeMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} payload expects {expected}, envelope is {actual}"
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
            Self::VerificationStatusMismatch {
                event_id,
                event_type,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} type {event_type} has verification status {}, expected {}",
                actual.as_persisted_str(),
                expected.as_persisted_str()
            ),
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
            Self::InvalidFieldType { event_id, field } => write!(
                formatter,
                "event {event_id} payload field {field} has an invalid type"
            ),
            Self::EmptyReason { event_id } => {
                write!(
                    formatter,
                    "event {event_id} correction reason must not be empty"
                )
            }
            Self::MissingCorrectionReference { event_id, field } => write!(
                formatter,
                "event {event_id} correction is missing reference {field}"
            ),
            Self::UnexpectedCorrectionReference { event_id, field } => write!(
                formatter,
                "event {event_id} has unexpected correction reference {field}"
            ),
            Self::SelfReferentialCorrection { event_id } => {
                write!(
                    formatter,
                    "event {event_id} correction cannot reference itself"
                )
            }
        }
    }
}

impl std::error::Error for MatchEventPayloadCodecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::C1(error) => Some(error),
            Self::UnsupportedEventType { .. }
            | Self::EventTypeMismatch { .. }
            | Self::SchemaVersionMismatch { .. }
            | Self::VerificationStatusMismatch { .. }
            | Self::PayloadMustBeObject { .. }
            | Self::MissingField { .. }
            | Self::UnknownField { .. }
            | Self::InvalidFieldType { .. }
            | Self::EmptyReason { .. }
            | Self::MissingCorrectionReference { .. }
            | Self::UnexpectedCorrectionReference { .. }
            | Self::SelfReferentialCorrection { .. } => None,
        }
    }
}

/// Encodes one typed canonical match event into the existing JSONB envelope.
///
/// C1 events cannot carry correction references. `EventVoided` must carry one
/// `voids_event_id` reference and cannot carry a replacement reference.
pub fn encode_match_event_envelope(
    event: &DomainEventEnvelope<MatchEventPayload>,
) -> Result<DomainEventEnvelope<Value>, MatchEventPayloadCodecError> {
    match event.payload() {
        MatchEventPayload::C1(payload) => {
            reject_correction_reference(event, "supersedes_event_id")?;
            reject_correction_reference(event, "voids_event_id")?;

            encode_c1_event_envelope(&event.clone().map_payload(payload.clone()))
                .map_err(MatchEventPayloadCodecError::C1)
        }
        MatchEventPayload::Correction(CorrectionEventPayload::EventVoided(payload)) => {
            encode_event_voided_envelope(event, payload)
        }
        MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(payload)) => {
            encode_match_verified_envelope(event, payload)
        }
        MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(payload)) => {
            encode_match_reopened_envelope(event, payload)
        }
    }
}

/// Decodes one persisted JSONB envelope into the canonical match payload union.
pub fn decode_match_event_envelope(
    event: DomainEventEnvelope<Value>,
) -> Result<DomainEventEnvelope<MatchEventPayload>, MatchEventPayloadCodecError> {
    let event_type = event.event_type().as_str();
    if is_c1_event_type(event_type) {
        reject_correction_reference(&event, "supersedes_event_id")?;
        reject_correction_reference(&event, "voids_event_id")?;

        return decode_c1_event_envelope(event)
            .map(|event| {
                let payload = event.payload().clone();
                event.map_payload(MatchEventPayload::C1(payload))
            })
            .map_err(MatchEventPayloadCodecError::C1);
    }

    if event_type == CorrectionEventType::EventVoided.as_str() {
        return decode_event_voided_envelope(event);
    }

    if event_type == VerificationEventType::MatchVerified.as_str() {
        return decode_match_verified_envelope(event);
    }

    if event_type == VerificationEventType::MatchReopened.as_str() {
        return decode_match_reopened_envelope(event);
    }

    Err(MatchEventPayloadCodecError::UnsupportedEventType {
        event_id: event.event_id(),
        actual: event_type.to_owned(),
    })
}

fn encode_event_voided_envelope(
    event: &DomainEventEnvelope<MatchEventPayload>,
    payload: &EventVoidedPayload,
) -> Result<DomainEventEnvelope<Value>, MatchEventPayloadCodecError> {
    validate_correction_envelope(event, CorrectionEventType::EventVoided)?;

    Ok(event.clone().map_payload(json!({
        "reason": payload.reason().as_str(),
    })))
}

fn decode_event_voided_envelope(
    event: DomainEventEnvelope<Value>,
) -> Result<DomainEventEnvelope<MatchEventPayload>, MatchEventPayloadCodecError> {
    validate_correction_envelope(&event, CorrectionEventType::EventVoided)?;

    let object =
        event
            .payload()
            .as_object()
            .ok_or(MatchEventPayloadCodecError::PayloadMustBeObject {
                event_id: event.event_id(),
            })?;
    ensure_fields(event.event_id(), object, &["reason"])?;
    let reason_value = object
        .get("reason")
        .ok_or(MatchEventPayloadCodecError::MissingField {
            event_id: event.event_id(),
            field: "reason",
        })?;
    let reason = reason_value
        .as_str()
        .ok_or(MatchEventPayloadCodecError::InvalidFieldType {
            event_id: event.event_id(),
            field: "reason",
        })?;
    let reason = CorrectionReason::try_new(reason).map_err(|_| {
        MatchEventPayloadCodecError::EmptyReason {
            event_id: event.event_id(),
        }
    })?;

    Ok(event.map_payload(MatchEventPayload::Correction(
        CorrectionEventPayload::EventVoided(EventVoidedPayload::new(reason)),
    )))
}

fn encode_match_verified_envelope(
    event: &DomainEventEnvelope<MatchEventPayload>,
    _payload: &MatchVerifiedPayload,
) -> Result<DomainEventEnvelope<Value>, MatchEventPayloadCodecError> {
    validate_verification_envelope(
        event,
        VerificationEventType::MatchVerified,
        EventVerificationStatus::Verified,
    )?;

    Ok(event.clone().map_payload(json!({})))
}

fn encode_match_reopened_envelope(
    event: &DomainEventEnvelope<MatchEventPayload>,
    payload: &MatchReopenedPayload,
) -> Result<DomainEventEnvelope<Value>, MatchEventPayloadCodecError> {
    validate_verification_envelope(
        event,
        VerificationEventType::MatchReopened,
        EventVerificationStatus::Provisional,
    )?;

    Ok(event.clone().map_payload(json!({
        "reason": payload.reason().as_str(),
    })))
}

fn decode_match_verified_envelope(
    event: DomainEventEnvelope<Value>,
) -> Result<DomainEventEnvelope<MatchEventPayload>, MatchEventPayloadCodecError> {
    validate_verification_envelope(
        &event,
        VerificationEventType::MatchVerified,
        EventVerificationStatus::Verified,
    )?;

    let object =
        event
            .payload()
            .as_object()
            .ok_or(MatchEventPayloadCodecError::PayloadMustBeObject {
                event_id: event.event_id(),
            })?;
    ensure_fields(event.event_id(), object, &[])?;

    Ok(event.map_payload(MatchEventPayload::Verification(
        VerificationEventPayload::MatchVerified(MatchVerifiedPayload::new()),
    )))
}

fn decode_match_reopened_envelope(
    event: DomainEventEnvelope<Value>,
) -> Result<DomainEventEnvelope<MatchEventPayload>, MatchEventPayloadCodecError> {
    validate_verification_envelope(
        &event,
        VerificationEventType::MatchReopened,
        EventVerificationStatus::Provisional,
    )?;

    let object =
        event
            .payload()
            .as_object()
            .ok_or(MatchEventPayloadCodecError::PayloadMustBeObject {
                event_id: event.event_id(),
            })?;
    ensure_fields(event.event_id(), object, &["reason"])?;
    let reason_value = object
        .get("reason")
        .ok_or(MatchEventPayloadCodecError::MissingField {
            event_id: event.event_id(),
            field: "reason",
        })?;
    let reason = reason_value
        .as_str()
        .ok_or(MatchEventPayloadCodecError::InvalidFieldType {
            event_id: event.event_id(),
            field: "reason",
        })?;
    let reason = CorrectionReason::try_new(reason).map_err(|_| {
        MatchEventPayloadCodecError::EmptyReason {
            event_id: event.event_id(),
        }
    })?;

    Ok(event.map_payload(MatchEventPayload::Verification(
        VerificationEventPayload::MatchReopened(MatchReopenedPayload::new(reason)),
    )))
}

fn validate_verification_envelope(
    event: &DomainEventEnvelope<impl Sized>,
    expected_type: VerificationEventType,
    expected_status: EventVerificationStatus,
) -> Result<(), MatchEventPayloadCodecError> {
    let event_id = event.event_id();
    if event.event_type().as_str() != expected_type.as_str() {
        return Err(MatchEventPayloadCodecError::EventTypeMismatch {
            event_id,
            expected: expected_type.as_str(),
            actual: event.event_type().as_str().to_owned(),
        });
    }

    let expected_schema = expected_type.schema_version();
    if event.event_schema_version() != expected_schema {
        return Err(MatchEventPayloadCodecError::SchemaVersionMismatch {
            event_id,
            event_type: expected_type.as_str(),
            expected: expected_schema,
            actual: event.event_schema_version(),
        });
    }

    if event.verification_status() != expected_status {
        return Err(MatchEventPayloadCodecError::VerificationStatusMismatch {
            event_id,
            event_type: expected_type.as_str(),
            expected: expected_status,
            actual: event.verification_status(),
        });
    }

    reject_correction_reference(event, "supersedes_event_id")?;
    reject_correction_reference(event, "voids_event_id")?;
    Ok(())
}

fn validate_correction_envelope(
    event: &DomainEventEnvelope<impl Sized>,
    expected_type: CorrectionEventType,
) -> Result<(), MatchEventPayloadCodecError> {
    let event_id = event.event_id();
    if event.event_type().as_str() != expected_type.as_str() {
        return Err(MatchEventPayloadCodecError::EventTypeMismatch {
            event_id,
            expected: expected_type.as_str(),
            actual: event.event_type().as_str().to_owned(),
        });
    }

    let expected_schema = expected_type.schema_version();
    if event.event_schema_version() != expected_schema {
        return Err(MatchEventPayloadCodecError::SchemaVersionMismatch {
            event_id,
            event_type: expected_type.as_str(),
            expected: expected_schema,
            actual: event.event_schema_version(),
        });
    }

    let Some(voided_event_id) = event.voids_event_id() else {
        return Err(MatchEventPayloadCodecError::MissingCorrectionReference {
            event_id,
            field: "voids_event_id",
        });
    };
    if voided_event_id == event_id {
        return Err(MatchEventPayloadCodecError::SelfReferentialCorrection { event_id });
    }
    reject_correction_reference(event, "supersedes_event_id")
}

fn reject_correction_reference(
    event: &DomainEventEnvelope<impl Sized>,
    field: &'static str,
) -> Result<(), MatchEventPayloadCodecError> {
    let present = match field {
        "supersedes_event_id" => event.supersedes_event_id().is_some(),
        "voids_event_id" => event.voids_event_id().is_some(),
        _ => false,
    };
    if present {
        return Err(MatchEventPayloadCodecError::UnexpectedCorrectionReference {
            event_id: event.event_id(),
            field,
        });
    }
    Ok(())
}

fn is_c1_event_type(value: &str) -> bool {
    C1EventType::ALL
        .into_iter()
        .any(|event_type| event_type.as_str() == value)
}

fn ensure_fields(
    event_id: EventId,
    object: &Map<String, Value>,
    expected_fields: &[&'static str],
) -> Result<(), MatchEventPayloadCodecError> {
    for field in expected_fields {
        if !object.contains_key(*field) {
            return Err(MatchEventPayloadCodecError::MissingField { event_id, field });
        }
    }

    for field in object.keys() {
        if !expected_fields.contains(&field.as_str()) {
            return Err(MatchEventPayloadCodecError::UnknownField {
                event_id,
                field: field.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MatchEventPayloadCodecError, decode_match_event_envelope, encode_match_event_envelope,
    };
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, ClockMilliseconds, CommandId,
        CorrectionReason, DomainEventEnvelope, EventId, EventIndex, EventVerificationStatus,
        GoalAwardType, GoalRecordedPayload, MatchEventPayload, MatchId, MatchPeriod,
        MatchReopenedPayload, MatchVerifiedPayload, TeamId, UtcInstant, VerificationEventPayload,
        VerificationEventType, VoidEventCommand,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("match event codec fixture UUID should be valid")
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019feae0-3bc5-7cf0-b3d2-0728c077a930",
        )))
    }

    fn c1_event() -> DomainEventEnvelope<MatchEventPayload> {
        DomainEventEnvelope::new(
            EventId::from_uuid(uuid("019feae0-3bc5-7cf1-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019feae0-3bc5-7cf2-b3d2-0728c077a930")),
            AggregateVersion::from_value(1),
            CommandId::from_uuid(uuid("019feae0-3bc5-7cf3-b3d2-0728c077a930")),
            EventIndex::initial(),
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                TeamId::from_uuid(uuid("019feae0-3bc5-7cf4-b3d2-0728c077a930")),
                None,
                GoalAwardType::Regular,
            ))
            .event_type()
            .try_as_event_type()
            .unwrap(),
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                TeamId::from_uuid(uuid("019feae0-3bc5-7cf4-b3d2-0728c077a930")),
                None,
                GoalAwardType::Regular,
            ))
            .schema_version(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(12).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            MatchEventPayload::C1(C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                TeamId::from_uuid(uuid("019feae0-3bc5-7cf4-b3d2-0728c077a930")),
                None,
                GoalAwardType::Regular,
            ))),
            None,
            None,
        )
    }

    fn correction_event() -> DomainEventEnvelope<MatchEventPayload> {
        VoidEventCommand::try_new(
            CommandId::from_uuid(uuid("019feae0-3bc5-7cf5-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019feae0-3bc5-7cf6-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019feae0-3bc5-7cf2-b3d2-0728c077a930")),
            AggregateVersion::from_value(2),
            MatchPeriod::HalfTime,
            ClockMilliseconds::from_seconds(45).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_002).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_003).unwrap(),
            actor(),
            EventVerificationStatus::Verified,
            EventId::from_uuid(uuid("019feae0-3bc5-7cf1-b3d2-0728c077a930")),
            CorrectionReason::try_new("official protocol removed the goal").unwrap(),
        )
        .unwrap()
        .into_canonical_unsequenced_event()
        .unwrap()
        .with_test_sequence(AggregateVersion::from_value(2), EventIndex::from_value(1))
    }

    fn verification_event() -> DomainEventEnvelope<MatchEventPayload> {
        DomainEventEnvelope::new(
            EventId::from_uuid(uuid("019feae0-3bc5-7cf8-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019feae0-3bc5-7cf2-b3d2-0728c077a930")),
            AggregateVersion::from_value(3),
            CommandId::from_uuid(uuid("019feae0-3bc5-7cf9-b3d2-0728c077a930")),
            EventIndex::initial(),
            VerificationEventType::MatchVerified
                .try_as_event_type()
                .unwrap(),
            VerificationEventType::MatchVerified.schema_version(),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(90).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_004).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_005).unwrap(),
            actor(),
            EventVerificationStatus::Verified,
            MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(
                MatchVerifiedPayload::new(),
            )),
            None,
            None,
        )
    }

    fn reopened_event() -> DomainEventEnvelope<MatchEventPayload> {
        DomainEventEnvelope::new(
            EventId::from_uuid(uuid("019feae0-3bc5-7cfb-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019feae0-3bc5-7cf2-b3d2-0728c077a930")),
            AggregateVersion::from_value(4),
            CommandId::from_uuid(uuid("019feae0-3bc5-7cfc-b3d2-0728c077a930")),
            EventIndex::initial(),
            VerificationEventType::MatchReopened
                .try_as_event_type()
                .unwrap(),
            VerificationEventType::MatchReopened.schema_version(),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(91).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_006).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_007).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(
                MatchReopenedPayload::new(
                    CorrectionReason::try_new("official protocol requires re-verification")
                        .unwrap(),
                ),
            )),
            None,
            None,
        )
    }

    trait TestSequence {
        fn with_test_sequence(
            self,
            aggregate_version: AggregateVersion,
            event_index: EventIndex,
        ) -> Self;
    }

    impl TestSequence for DomainEventEnvelope<MatchEventPayload> {
        fn with_test_sequence(
            self,
            aggregate_version: AggregateVersion,
            event_index: EventIndex,
        ) -> Self {
            DomainEventEnvelope::new(
                self.event_id(),
                self.match_id(),
                aggregate_version,
                self.causation_command_id(),
                event_index,
                self.event_type().clone(),
                self.event_schema_version(),
                self.period(),
                self.clock_ms(),
                self.occurred_at(),
                self.recorded_at(),
                self.actor(),
                self.verification_status(),
                self.payload().clone(),
                self.supersedes_event_id(),
                self.voids_event_id(),
            )
        }
    }

    #[test]
    fn round_trips_match_payload_union_without_losing_envelope_metadata() {
        for event in [
            c1_event(),
            correction_event(),
            verification_event(),
            reopened_event(),
        ] {
            let encoded = encode_match_event_envelope(&event).unwrap();
            let decoded = decode_match_event_envelope(encoded).unwrap();

            assert_eq!(decoded, event);
        }
    }

    #[test]
    fn encodes_match_verified_as_an_empty_verified_payload() {
        let encoded = encode_match_event_envelope(&verification_event()).unwrap();

        assert_eq!(encoded.payload(), &json!({}));
        assert_eq!(encoded.event_type().as_str(), "MatchVerified");
        assert_eq!(encoded.event_schema_version().value(), 1);
        assert_eq!(
            encoded.verification_status(),
            EventVerificationStatus::Verified
        );
        assert_eq!(encoded.supersedes_event_id(), None);
        assert_eq!(encoded.voids_event_id(), None);
    }

    #[test]
    fn encodes_match_reopened_with_a_provisional_status_and_reason() {
        let encoded = encode_match_event_envelope(&reopened_event()).unwrap();

        assert_eq!(
            encoded.payload(),
            &json!({
                "reason": "official protocol requires re-verification"
            })
        );
        assert_eq!(encoded.event_type().as_str(), "MatchReopened");
        assert_eq!(encoded.event_schema_version().value(), 1);
        assert_eq!(
            encoded.verification_status(),
            EventVerificationStatus::Provisional
        );
        assert_eq!(encoded.supersedes_event_id(), None);
        assert_eq!(encoded.voids_event_id(), None);
    }

    #[test]
    fn rejects_non_object_or_extra_fields_in_persisted_match_verified_payload() {
        let event = verification_event();

        assert!(matches!(
            decode_match_event_envelope(event.clone().map_payload(json!(null))),
            Err(MatchEventPayloadCodecError::PayloadMustBeObject { .. })
        ));
        assert!(matches!(
            decode_match_event_envelope(event.map_payload(json!({"evidence": "protocol"}))),
            Err(MatchEventPayloadCodecError::UnknownField { field, .. }) if field == "evidence"
        ));

        let reopened = reopened_event();
        assert!(matches!(
            decode_match_event_envelope(reopened.clone().map_payload(json!({}))),
            Err(MatchEventPayloadCodecError::MissingField {
                field: "reason",
                ..
            })
        ));
        assert!(matches!(
            decode_match_event_envelope(reopened.clone().map_payload(json!({
                "reason": " \t"
            }))),
            Err(MatchEventPayloadCodecError::EmptyReason { .. })
        ));
        assert!(matches!(
            decode_match_event_envelope(reopened.map_payload(json!({
                "reason": "official protocol requires re-verification",
                "extra": true
            }))),
            Err(MatchEventPayloadCodecError::UnknownField { field, .. }) if field == "extra"
        ));
    }

    #[test]
    fn rejects_correction_references_on_match_verified_events() {
        let event = verification_event();
        let invalid = DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            event.aggregate_version(),
            event.causation_command_id(),
            event.event_index(),
            event.event_type().clone(),
            event.event_schema_version(),
            event.period(),
            event.clock_ms(),
            event.occurred_at(),
            event.recorded_at(),
            event.actor(),
            event.verification_status(),
            event.payload().clone(),
            None,
            Some(EventId::from_uuid(uuid(
                "019feae0-3bc5-7cfa-b3d2-0728c077a930",
            ))),
        );

        assert!(matches!(
            encode_match_event_envelope(&invalid),
            Err(MatchEventPayloadCodecError::UnexpectedCorrectionReference {
                field: "voids_event_id",
                ..
            })
        ));

        let reopened = reopened_event();
        let invalid_reopened = DomainEventEnvelope::new(
            reopened.event_id(),
            reopened.match_id(),
            reopened.aggregate_version(),
            reopened.causation_command_id(),
            reopened.event_index(),
            reopened.event_type().clone(),
            reopened.event_schema_version(),
            reopened.period(),
            reopened.clock_ms(),
            reopened.occurred_at(),
            reopened.recorded_at(),
            reopened.actor(),
            reopened.verification_status(),
            reopened.payload().clone(),
            Some(EventId::from_uuid(uuid(
                "019feae0-3bc5-7cfd-b3d2-0728c077a930",
            ))),
            None,
        );
        assert!(matches!(
            encode_match_event_envelope(&invalid_reopened),
            Err(MatchEventPayloadCodecError::UnexpectedCorrectionReference {
                field: "supersedes_event_id",
                ..
            })
        ));
    }

    #[test]
    fn rejects_verification_events_with_the_wrong_resulting_status() {
        let event = verification_event();
        let invalid = DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            event.aggregate_version(),
            event.causation_command_id(),
            event.event_index(),
            event.event_type().clone(),
            event.event_schema_version(),
            event.period(),
            event.clock_ms(),
            event.occurred_at(),
            event.recorded_at(),
            event.actor(),
            EventVerificationStatus::Provisional,
            event.payload().clone(),
            None,
            None,
        );
        assert!(matches!(
            encode_match_event_envelope(&invalid),
            Err(MatchEventPayloadCodecError::VerificationStatusMismatch {
                expected: EventVerificationStatus::Verified,
                actual: EventVerificationStatus::Provisional,
                ..
            })
        ));

        let event = reopened_event();
        let invalid = DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            event.aggregate_version(),
            event.causation_command_id(),
            event.event_index(),
            event.event_type().clone(),
            event.event_schema_version(),
            event.period(),
            event.clock_ms(),
            event.occurred_at(),
            event.recorded_at(),
            event.actor(),
            EventVerificationStatus::Verified,
            event.payload().clone(),
            None,
            None,
        );
        assert!(matches!(
            encode_match_event_envelope(&invalid),
            Err(MatchEventPayloadCodecError::VerificationStatusMismatch {
                expected: EventVerificationStatus::Provisional,
                actual: EventVerificationStatus::Verified,
                ..
            })
        ));
    }

    #[test]
    fn encodes_void_reason_in_json_and_keeps_target_in_envelope_reference() {
        let encoded = encode_match_event_envelope(&correction_event()).unwrap();

        assert_eq!(
            encoded.payload(),
            &json!({
                "reason": "official protocol removed the goal"
            })
        );
        assert_eq!(
            encoded.voids_event_id(),
            Some(EventId::from_uuid(uuid(
                "019feae0-3bc5-7cf1-b3d2-0728c077a930"
            )))
        );
    }

    #[test]
    fn rejects_missing_or_self_referential_void_targets() {
        let event = correction_event();
        let missing_target = DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            event.aggregate_version(),
            event.causation_command_id(),
            event.event_index(),
            event.event_type().clone(),
            event.event_schema_version(),
            event.period(),
            event.clock_ms(),
            event.occurred_at(),
            event.recorded_at(),
            event.actor(),
            event.verification_status(),
            event.payload().clone(),
            None,
            None,
        );
        assert!(matches!(
            encode_match_event_envelope(&missing_target),
            Err(MatchEventPayloadCodecError::MissingCorrectionReference {
                field: "voids_event_id",
                ..
            })
        ));

        let self_reference = DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            event.aggregate_version(),
            event.causation_command_id(),
            event.event_index(),
            event.event_type().clone(),
            event.event_schema_version(),
            event.period(),
            event.clock_ms(),
            event.occurred_at(),
            event.recorded_at(),
            event.actor(),
            event.verification_status(),
            event.payload().clone(),
            None,
            Some(event.event_id()),
        );
        assert!(matches!(
            encode_match_event_envelope(&self_reference),
            Err(MatchEventPayloadCodecError::SelfReferentialCorrection { .. })
        ));
    }

    #[test]
    fn rejects_blank_or_extra_fields_in_persisted_void_payload() {
        let event = correction_event();
        let blank_reason = event.clone().map_payload(json!({ "reason": " \t" }));
        assert!(matches!(
            decode_match_event_envelope(blank_reason),
            Err(MatchEventPayloadCodecError::EmptyReason { .. })
        ));

        let extra_field = event.map_payload(json!({
            "reason": "official protocol removed the goal",
            "extra": true
        }));
        assert!(matches!(
            decode_match_event_envelope(extra_field),
            Err(MatchEventPayloadCodecError::UnknownField { field, .. }) if field == "extra"
        ));
    }

    #[test]
    fn rejects_c1_events_that_carry_correction_references() {
        let event = c1_event();
        let invalid = DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            event.aggregate_version(),
            event.causation_command_id(),
            event.event_index(),
            event.event_type().clone(),
            event.event_schema_version(),
            event.period(),
            event.clock_ms(),
            event.occurred_at(),
            event.recorded_at(),
            event.actor(),
            event.verification_status(),
            event.payload().clone(),
            None,
            Some(EventId::from_uuid(uuid(
                "019feae0-3bc5-7cf7-b3d2-0728c077a930",
            ))),
        );

        assert!(matches!(
            encode_match_event_envelope(&invalid),
            Err(MatchEventPayloadCodecError::UnexpectedCorrectionReference {
                field: "voids_event_id",
                ..
            })
        ));
    }
}
