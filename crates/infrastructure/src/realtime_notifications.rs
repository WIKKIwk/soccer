use crate::outbox::{OutboxDeliveryStatus, StoredOutboxMessage};
use crate::realtime_sequence::{RealtimeSequenceRepositoryError, allocate_realtime_topic_sequence};
use football_contracts::{
    MatchSnapshotVerificationStatus, REALTIME_MATCH_NOTIFICATION_SCHEMA_VERSION,
    REALTIME_MATCH_NOTIFICATION_TYPE, RealtimeMatchCorrectionNotification,
    RealtimeMatchNotification, RealtimeMatchVerificationNotification, RealtimeNotification,
    RealtimeSequence, RealtimeTopic, RealtimeVerificationChangeType,
};
use football_domain::{CorrectionEventType, EventId, EventVerificationStatus};
use serde_json::Value;
use sqlx::{Error as SqlxError, Postgres, Transaction};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeNotificationBuildError {
    MessageNotProcessing {
        message_id: EventId,
        status: OutboxDeliveryStatus,
    },
    UnsupportedMessageType {
        message_id: EventId,
        value: String,
    },
    UnsupportedSchemaVersion {
        message_id: EventId,
        value: u32,
    },
    UnsupportedEventSchemaVersion {
        message_id: EventId,
        event_type: String,
        value: u32,
    },
    CorrectionRequiresDedicatedNotification {
        message_id: EventId,
    },
    VerificationRequiresDedicatedNotification {
        message_id: EventId,
    },
    ExpectedCorrectionEvent {
        message_id: EventId,
        actual_event_type: String,
    },
    ExpectedVerificationEvent {
        message_id: EventId,
        actual_event_type: String,
    },
    InvalidPayloadField {
        message_id: EventId,
        field: &'static str,
    },
}

impl fmt::Display for RealtimeNotificationBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageNotProcessing { message_id, status } => write!(
                formatter,
                "outbox message {message_id} cannot become a realtime notification from status {status}"
            ),
            Self::UnsupportedMessageType { message_id, value } => write!(
                formatter,
                "outbox message {message_id} has unsupported realtime message type {value}"
            ),
            Self::UnsupportedSchemaVersion { message_id, value } => write!(
                formatter,
                "outbox message {message_id} has unsupported realtime schema version {value}"
            ),
            Self::UnsupportedEventSchemaVersion {
                message_id,
                event_type,
                value,
            } => write!(
                formatter,
                "outbox message {message_id} has unsupported {event_type} event schema version {value}"
            ),
            Self::CorrectionRequiresDedicatedNotification { message_id } => write!(
                formatter,
                "outbox message {message_id} is a correction and cannot become a match.event notification"
            ),
            Self::VerificationRequiresDedicatedNotification { message_id } => write!(
                formatter,
                "outbox message {message_id} is a verification change and cannot become a match.event notification"
            ),
            Self::ExpectedCorrectionEvent {
                message_id,
                actual_event_type,
            } => write!(
                formatter,
                "outbox message {message_id} is {actual_event_type}, not an EventVoided correction"
            ),
            Self::ExpectedVerificationEvent {
                message_id,
                actual_event_type,
            } => write!(
                formatter,
                "outbox message {message_id} is {actual_event_type}, not a verification change"
            ),
            Self::InvalidPayloadField { message_id, field } => write!(
                formatter,
                "outbox message {message_id} has invalid realtime payload field {field}"
            ),
        }
    }
}

impl std::error::Error for RealtimeNotificationBuildError {}

#[derive(Debug)]
pub enum RealtimeNotificationPreparationError {
    Build(RealtimeNotificationBuildError),
    Database(SqlxError),
    ClaimNotCurrent {
        message_id: EventId,
    },
    InvalidStoredTopic {
        message_id: EventId,
        value: String,
    },
    StoredTopicMismatch {
        message_id: EventId,
        expected: RealtimeTopic,
        actual: RealtimeTopic,
    },
    InvalidStoredSequence {
        message_id: EventId,
        value: i64,
    },
    AllocatedSequenceOutOfRange {
        message_id: EventId,
        value: u64,
    },
    Sequence(RealtimeSequenceRepositoryError),
}

impl fmt::Display for RealtimeNotificationPreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Build(error) => error.fmt(formatter),
            Self::Database(error) => {
                write!(
                    formatter,
                    "realtime notification database operation failed: {error}"
                )
            }
            Self::ClaimNotCurrent { message_id } => write!(
                formatter,
                "realtime notification claim for message {message_id} is no longer current"
            ),
            Self::InvalidStoredTopic { message_id, value } => write!(
                formatter,
                "realtime notification identity for message {message_id} has invalid topic {value}"
            ),
            Self::StoredTopicMismatch {
                message_id,
                expected,
                actual,
            } => write!(
                formatter,
                "realtime notification identity for message {message_id} has topic {actual}, expected {expected}"
            ),
            Self::InvalidStoredSequence { message_id, value } => write!(
                formatter,
                "realtime notification identity for message {message_id} has invalid sequence {value}"
            ),
            Self::AllocatedSequenceOutOfRange { message_id, value } => write!(
                formatter,
                "allocated realtime sequence for message {message_id} does not fit PostgreSQL BIGINT: {value}"
            ),
            Self::Sequence(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RealtimeNotificationPreparationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Build(error) => Some(error),
            Self::Database(error) => Some(error),
            Self::ClaimNotCurrent { .. }
            | Self::InvalidStoredTopic { .. }
            | Self::StoredTopicMismatch { .. }
            | Self::InvalidStoredSequence { .. }
            | Self::AllocatedSequenceOutOfRange { .. } => None,
            Self::Sequence(error) => Some(error),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CurrentOutboxClaimRow {
    delivery_status: String,
    claim_owner: Option<Uuid>,
    claim_expires_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, sqlx::FromRow)]
struct RealtimeNotificationIdentityRow {
    topic: String,
    sequence: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedRealtimeMessage {
    topic: RealtimeTopic,
    event_type: String,
    kind: ValidatedRealtimeMessageKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ValidatedRealtimeMessageKind {
    LiveEvent,
    Correction {
        voids_event_id: Uuid,
        reason: String,
    },
    Verification {
        change_type: RealtimeVerificationChangeType,
        verification_status: MatchSnapshotVerificationStatus,
    },
}

fn invalid_payload(
    message: &StoredOutboxMessage,
    field: &'static str,
) -> RealtimeNotificationBuildError {
    RealtimeNotificationBuildError::InvalidPayloadField {
        message_id: message.message_id,
        field,
    }
}

fn payload_uuid(
    message: &StoredOutboxMessage,
    field: &'static str,
) -> Result<Uuid, RealtimeNotificationBuildError> {
    let value = message
        .payload
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_payload(message, field))?;
    Uuid::parse_str(value).map_err(|_| invalid_payload(message, field))
}

fn validate_message(
    message: &StoredOutboxMessage,
) -> Result<ValidatedRealtimeMessage, RealtimeNotificationBuildError> {
    if message.delivery_status != OutboxDeliveryStatus::Processing {
        return Err(RealtimeNotificationBuildError::MessageNotProcessing {
            message_id: message.message_id,
            status: message.delivery_status,
        });
    }
    if message.message_type != REALTIME_MATCH_NOTIFICATION_TYPE {
        return Err(RealtimeNotificationBuildError::UnsupportedMessageType {
            message_id: message.message_id,
            value: message.message_type.clone(),
        });
    }
    if message.schema_version != REALTIME_MATCH_NOTIFICATION_SCHEMA_VERSION {
        return Err(RealtimeNotificationBuildError::UnsupportedSchemaVersion {
            message_id: message.message_id,
            value: message.schema_version,
        });
    }
    if !message.payload.is_object() {
        return Err(invalid_payload(message, "payload"));
    }

    let payload_match_id = payload_uuid(message, "match_id")?;
    if payload_match_id != message.aggregate_id.as_uuid() {
        return Err(invalid_payload(message, "match_id"));
    }
    let payload_event_id = payload_uuid(message, "event_id")?;
    if payload_event_id != message.source_event_id.as_uuid() {
        return Err(invalid_payload(message, "event_id"));
    }
    let event_type = message
        .payload
        .get("event_type")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_payload(message, "event_type"))?;
    if event_type.trim().is_empty() {
        return Err(invalid_payload(message, "event_type"));
    }
    let event_schema_version = message
        .payload
        .get("event_schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid_payload(message, "event_schema_version"))?;
    if event_schema_version == 0 || event_schema_version > u32::MAX as u64 {
        return Err(invalid_payload(message, "event_schema_version"));
    }

    let kind = if event_type == CorrectionEventType::EventVoided.as_str() {
        let expected_schema_version =
            u32::from(CorrectionEventType::EventVoided.schema_version().value());
        let actual_schema_version = u32::try_from(event_schema_version)
            .map_err(|_| invalid_payload(message, "event_schema_version"))?;
        if actual_schema_version != expected_schema_version {
            return Err(
                RealtimeNotificationBuildError::UnsupportedEventSchemaVersion {
                    message_id: message.message_id,
                    event_type: event_type.to_owned(),
                    value: actual_schema_version,
                },
            );
        }

        let voids_event_id = payload_uuid(message, "voids_event_id")?;
        if voids_event_id == message.source_event_id.as_uuid() {
            return Err(invalid_payload(message, "voids_event_id"));
        }
        let supersedes_event_id = message
            .payload
            .get("supersedes_event_id")
            .ok_or_else(|| invalid_payload(message, "supersedes_event_id"))?;
        if !supersedes_event_id.is_null() {
            return Err(invalid_payload(message, "supersedes_event_id"));
        }

        let reason = message
            .payload
            .get("payload")
            .and_then(Value::as_object)
            .and_then(|payload| payload.get("reason"))
            .and_then(Value::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .ok_or_else(|| invalid_payload(message, "payload.reason"))?
            .to_owned();

        ValidatedRealtimeMessageKind::Correction {
            voids_event_id,
            reason,
        }
    } else if let Some(change_type) =
        RealtimeVerificationChangeType::from_canonical_event_type(event_type)
    {
        let verification_status = message
            .payload
            .get("verification_status")
            .and_then(Value::as_str)
            .and_then(EventVerificationStatus::try_from_persisted_str)
            .map(|status| match status {
                EventVerificationStatus::Provisional => {
                    MatchSnapshotVerificationStatus::Provisional
                }
                EventVerificationStatus::Verified => MatchSnapshotVerificationStatus::Verified,
            })
            .ok_or_else(|| invalid_payload(message, "verification_status"))?;

        ValidatedRealtimeMessageKind::Verification {
            change_type,
            verification_status,
        }
    } else {
        ValidatedRealtimeMessageKind::LiveEvent
    };

    Ok(ValidatedRealtimeMessage {
        topic: RealtimeTopic::match_topic(message.aggregate_id.as_uuid()),
        event_type: event_type.to_owned(),
        kind,
    })
}

fn build_validated_realtime_notification(
    message: &StoredOutboxMessage,
    validated: &ValidatedRealtimeMessage,
    sequence: RealtimeSequence,
) -> RealtimeNotification {
    match &validated.kind {
        ValidatedRealtimeMessageKind::LiveEvent => {
            RealtimeNotification::MatchEvent(RealtimeMatchNotification::new(
                message.message_id.as_uuid(),
                validated.topic,
                sequence,
                message.payload.clone(),
            ))
        }
        ValidatedRealtimeMessageKind::Correction {
            voids_event_id,
            reason,
        } => RealtimeNotification::MatchCorrection(RealtimeMatchCorrectionNotification::new(
            message.message_id.as_uuid(),
            validated.topic,
            sequence,
            message.aggregate_id.as_uuid(),
            message.source_event_id.as_uuid(),
            *voids_event_id,
            reason.clone(),
        )),
        ValidatedRealtimeMessageKind::Verification {
            change_type,
            verification_status,
        } => RealtimeNotification::MatchVerification(RealtimeMatchVerificationNotification::new(
            message.message_id.as_uuid(),
            validated.topic,
            sequence,
            message.aggregate_id.as_uuid(),
            message.source_event_id.as_uuid(),
            *change_type,
            *verification_status,
            message.payload.clone(),
        )),
    }
}

/// Converts one claimed match-event outbox row into the appropriate
/// transport-ready notification variant. C1 events use `match.event`; an
/// immutable `EventVoided` marker uses the dedicated `match.correction` type.
pub fn build_realtime_notification(
    message: &StoredOutboxMessage,
    sequence: RealtimeSequence,
) -> Result<RealtimeNotification, RealtimeNotificationBuildError> {
    let validated = validate_message(message)?;
    Ok(build_validated_realtime_notification(
        message, &validated, sequence,
    ))
}

/// Converts one claimed match-event outbox row into a versioned realtime
/// live-event notification. Correction rows use the dedicated builder so
/// their wire type remains distinct.
pub fn build_realtime_match_notification(
    message: &StoredOutboxMessage,
    sequence: RealtimeSequence,
) -> Result<RealtimeMatchNotification, RealtimeNotificationBuildError> {
    let validated = validate_message(message)?;
    match validated.kind {
        ValidatedRealtimeMessageKind::LiveEvent => Ok(RealtimeMatchNotification::new(
            message.message_id.as_uuid(),
            validated.topic,
            sequence,
            message.payload.clone(),
        )),
        ValidatedRealtimeMessageKind::Correction { .. } => Err(
            RealtimeNotificationBuildError::CorrectionRequiresDedicatedNotification {
                message_id: message.message_id,
            },
        ),
        ValidatedRealtimeMessageKind::Verification { .. } => Err(
            RealtimeNotificationBuildError::VerificationRequiresDedicatedNotification {
                message_id: message.message_id,
            },
        ),
    }
}

/// Converts one claimed `EventVoided` outbox row into the dedicated typed
/// correction notification. Live C1 rows are rejected rather than silently
/// changing their wire meaning.
pub fn build_realtime_match_correction_notification(
    message: &StoredOutboxMessage,
    sequence: RealtimeSequence,
) -> Result<RealtimeMatchCorrectionNotification, RealtimeNotificationBuildError> {
    let validated = validate_message(message)?;
    match validated.kind {
        ValidatedRealtimeMessageKind::Correction {
            voids_event_id,
            reason,
        } => Ok(RealtimeMatchCorrectionNotification::new(
            message.message_id.as_uuid(),
            validated.topic,
            sequence,
            message.aggregate_id.as_uuid(),
            message.source_event_id.as_uuid(),
            voids_event_id,
            reason,
        )),
        ValidatedRealtimeMessageKind::LiveEvent => {
            Err(RealtimeNotificationBuildError::ExpectedCorrectionEvent {
                message_id: message.message_id,
                actual_event_type: validated.event_type,
            })
        }
        ValidatedRealtimeMessageKind::Verification { .. } => {
            Err(RealtimeNotificationBuildError::ExpectedCorrectionEvent {
                message_id: message.message_id,
                actual_event_type: validated.event_type,
            })
        }
    }
}

/// Converts one claimed `MatchVerified` or `MatchReopened` outbox row into a
/// dedicated typed verification notification. Live C1 and correction rows are
/// rejected rather than silently changing their wire meaning.
pub fn build_realtime_match_verification_notification(
    message: &StoredOutboxMessage,
    sequence: RealtimeSequence,
) -> Result<RealtimeMatchVerificationNotification, RealtimeNotificationBuildError> {
    let validated = validate_message(message)?;
    match validated.kind {
        ValidatedRealtimeMessageKind::Verification {
            change_type,
            verification_status,
        } => Ok(RealtimeMatchVerificationNotification::new(
            message.message_id.as_uuid(),
            validated.topic,
            sequence,
            message.aggregate_id.as_uuid(),
            message.source_event_id.as_uuid(),
            change_type,
            verification_status,
            message.payload.clone(),
        )),
        ValidatedRealtimeMessageKind::LiveEvent
        | ValidatedRealtimeMessageKind::Correction { .. } => {
            Err(RealtimeNotificationBuildError::ExpectedVerificationEvent {
                message_id: message.message_id,
                actual_event_type: validated.event_type,
            })
        }
    }
}

async fn ensure_realtime_notification_sequence(
    transaction: &mut Transaction<'_, Postgres>,
    message: &StoredOutboxMessage,
    topic: RealtimeTopic,
) -> Result<RealtimeSequence, RealtimeNotificationPreparationError> {
    let current_claim = sqlx::query_as::<_, CurrentOutboxClaimRow>(
        r#"
        SELECT delivery_status, claim_owner, claim_expires_at
        FROM outbox_messages
        WHERE message_id = $1
        FOR UPDATE
        "#,
    )
    .bind(message.message_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RealtimeNotificationPreparationError::Database)?
    .ok_or(RealtimeNotificationPreparationError::ClaimNotCurrent {
        message_id: message.message_id,
    })?;

    if current_claim.delivery_status != OutboxDeliveryStatus::Processing.as_persisted_str()
        || current_claim.claim_owner != message.claim_owner
        || current_claim.claim_expires_at
            != message
                .claim_expires_at
                .map(|value| value.as_offset_datetime())
    {
        return Err(RealtimeNotificationPreparationError::ClaimNotCurrent {
            message_id: message.message_id,
        });
    }

    let existing = sqlx::query_as::<_, RealtimeNotificationIdentityRow>(
        r#"
        SELECT topic, sequence
        FROM realtime_notification_identities
        WHERE message_id = $1
        FOR UPDATE
        "#,
    )
    .bind(message.message_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(RealtimeNotificationPreparationError::Database)?;

    if let Some(existing) = existing {
        let stored_topic = existing.topic.parse::<RealtimeTopic>().map_err(|_| {
            RealtimeNotificationPreparationError::InvalidStoredTopic {
                message_id: message.message_id,
                value: existing.topic.clone(),
            }
        })?;
        if stored_topic != topic {
            return Err(RealtimeNotificationPreparationError::StoredTopicMismatch {
                message_id: message.message_id,
                expected: topic,
                actual: stored_topic,
            });
        }
        let sequence_value = u64::try_from(existing.sequence).map_err(|_| {
            RealtimeNotificationPreparationError::InvalidStoredSequence {
                message_id: message.message_id,
                value: existing.sequence,
            }
        })?;
        return RealtimeSequence::try_new(sequence_value).ok_or(
            RealtimeNotificationPreparationError::InvalidStoredSequence {
                message_id: message.message_id,
                value: existing.sequence,
            },
        );
    }

    let allocated = allocate_realtime_topic_sequence(transaction, topic)
        .await
        .map_err(RealtimeNotificationPreparationError::Sequence)?;
    let sequence_value = i64::try_from(allocated.sequence.value()).map_err(|_| {
        RealtimeNotificationPreparationError::AllocatedSequenceOutOfRange {
            message_id: message.message_id,
            value: allocated.sequence.value(),
        }
    })?;
    sqlx::query(
        r#"
        INSERT INTO realtime_notification_identities (message_id, topic, sequence)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(message.message_id.as_uuid())
    .bind(topic.as_topic_name())
    .bind(sequence_value)
    .execute(&mut **transaction)
    .await
    .map_err(RealtimeNotificationPreparationError::Database)?;

    Ok(allocated.sequence)
}

/// Claims a topic sequence and prepares the notification in the caller's
/// transaction. The first preparation persists the message's topic/sequence
/// identity; retries load and reuse it. The transaction must be committed
/// before network delivery.
pub async fn prepare_realtime_match_notification(
    transaction: &mut Transaction<'_, Postgres>,
    message: &StoredOutboxMessage,
) -> Result<RealtimeMatchNotification, RealtimeNotificationPreparationError> {
    let validated =
        validate_message(message).map_err(RealtimeNotificationPreparationError::Build)?;
    if matches!(
        &validated.kind,
        ValidatedRealtimeMessageKind::Correction { .. }
    ) {
        return Err(RealtimeNotificationPreparationError::Build(
            RealtimeNotificationBuildError::CorrectionRequiresDedicatedNotification {
                message_id: message.message_id,
            },
        ));
    }
    if matches!(
        &validated.kind,
        ValidatedRealtimeMessageKind::Verification { .. }
    ) {
        return Err(RealtimeNotificationPreparationError::Build(
            RealtimeNotificationBuildError::VerificationRequiresDedicatedNotification {
                message_id: message.message_id,
            },
        ));
    }
    let sequence =
        ensure_realtime_notification_sequence(transaction, message, validated.topic).await?;
    Ok(RealtimeMatchNotification::new(
        message.message_id.as_uuid(),
        validated.topic,
        sequence,
        message.payload.clone(),
    ))
}

/// Claims the durable topic sequence and prepares either the live-event or
/// correction notification selected by the claimed outbox payload.
pub async fn prepare_realtime_notification(
    transaction: &mut Transaction<'_, Postgres>,
    message: &StoredOutboxMessage,
) -> Result<RealtimeNotification, RealtimeNotificationPreparationError> {
    let validated =
        validate_message(message).map_err(RealtimeNotificationPreparationError::Build)?;
    let sequence =
        ensure_realtime_notification_sequence(transaction, message, validated.topic).await?;
    Ok(build_validated_realtime_notification(
        message, &validated, sequence,
    ))
}

/// Claims a durable topic sequence and prepares only a typed correction
/// notification. A live event is rejected before sequence allocation.
pub async fn prepare_realtime_match_correction_notification(
    transaction: &mut Transaction<'_, Postgres>,
    message: &StoredOutboxMessage,
) -> Result<RealtimeMatchCorrectionNotification, RealtimeNotificationPreparationError> {
    let validated =
        validate_message(message).map_err(RealtimeNotificationPreparationError::Build)?;
    let ValidatedRealtimeMessage {
        topic,
        event_type,
        kind,
    } = validated;
    let ValidatedRealtimeMessageKind::Correction {
        voids_event_id,
        reason,
    } = kind
    else {
        return Err(RealtimeNotificationPreparationError::Build(
            RealtimeNotificationBuildError::ExpectedCorrectionEvent {
                message_id: message.message_id,
                actual_event_type: event_type,
            },
        ));
    };
    let sequence = ensure_realtime_notification_sequence(transaction, message, topic).await?;
    Ok(RealtimeMatchCorrectionNotification::new(
        message.message_id.as_uuid(),
        topic,
        sequence,
        message.aggregate_id.as_uuid(),
        message.source_event_id.as_uuid(),
        voids_event_id,
        reason,
    ))
}

/// Claims a durable topic sequence and prepares only a typed verification
/// notification. Live and correction events are rejected before sequence
/// allocation.
pub async fn prepare_realtime_match_verification_notification(
    transaction: &mut Transaction<'_, Postgres>,
    message: &StoredOutboxMessage,
) -> Result<RealtimeMatchVerificationNotification, RealtimeNotificationPreparationError> {
    let validated =
        validate_message(message).map_err(RealtimeNotificationPreparationError::Build)?;
    let ValidatedRealtimeMessage {
        topic,
        event_type,
        kind,
    } = validated;
    let ValidatedRealtimeMessageKind::Verification {
        change_type,
        verification_status,
    } = kind
    else {
        return Err(RealtimeNotificationPreparationError::Build(
            RealtimeNotificationBuildError::ExpectedVerificationEvent {
                message_id: message.message_id,
                actual_event_type: event_type,
            },
        ));
    };
    let sequence = ensure_realtime_notification_sequence(transaction, message, topic).await?;
    Ok(RealtimeMatchVerificationNotification::new(
        message.message_id.as_uuid(),
        topic,
        sequence,
        message.aggregate_id.as_uuid(),
        message.source_event_id.as_uuid(),
        change_type,
        verification_status,
        message.payload.clone(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        RealtimeNotificationBuildError, build_realtime_match_correction_notification,
        build_realtime_match_notification, build_realtime_match_verification_notification,
        build_realtime_notification,
    };
    use crate::outbox::{OutboxDeliveryStatus, StoredOutboxMessage};
    use football_contracts::{
        MatchSnapshotVerificationStatus, RealtimeNotification, RealtimeSequence, RealtimeTopic,
        RealtimeVerificationChangeType,
    };
    use football_domain::{CommandId, EventId, MatchId, UtcInstant};
    use serde_json::json;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("realtime notification fixture UUID should be valid")
    }

    fn message(status: OutboxDeliveryStatus, payload: serde_json::Value) -> StoredOutboxMessage {
        StoredOutboxMessage {
            message_id: EventId::from_uuid(uuid("019ff400-3bc5-7cf0-b3d2-0728c077a930")),
            aggregate_id: MatchId::from_uuid(uuid("019ff401-3bc5-7cf0-b3d2-0728c077a930")),
            source_aggregate_version: 1,
            causation_command_id: CommandId::from_uuid(uuid(
                "019ff402-3bc5-7cf0-b3d2-0728c077a930",
            )),
            source_event_id: EventId::from_uuid(uuid("019ff400-3bc5-7cf0-b3d2-0728c077a930")),
            message_type: "match.event".to_owned(),
            schema_version: 1,
            payload,
            created_at: UtcInstant::from_unix_timestamp(1_000).unwrap(),
            next_attempt_at: UtcInstant::from_unix_timestamp(1_000).unwrap(),
            delivery_status: status,
            attempt_count: 1,
            claim_owner: Some(uuid("019ff403-3bc5-7cf0-b3d2-0728c077a930")),
            claim_expires_at: Some(UtcInstant::from_unix_timestamp(1_030).unwrap()),
            poisoned_at: None,
            poison_reason: None,
            published_at: None,
        }
    }

    fn valid_payload() -> serde_json::Value {
        json!({
            "event_id": "019ff400-3bc5-7cf0-b3d2-0728c077a930",
            "match_id": "019ff401-3bc5-7cf0-b3d2-0728c077a930",
            "event_type": "GoalRecorded",
            "event_schema_version": 1,
            "payload": {"award_type": "regular"}
        })
    }

    fn correction_payload() -> serde_json::Value {
        json!({
            "event_id": "019ff400-3bc5-7cf0-b3d2-0728c077a930",
            "match_id": "019ff401-3bc5-7cf0-b3d2-0728c077a930",
            "event_type": "EventVoided",
            "event_schema_version": 1,
            "payload": {"reason": "official protocol removed the goal"},
            "supersedes_event_id": null,
            "voids_event_id": "019ff404-3bc5-7cf0-b3d2-0728c077a930"
        })
    }

    fn verification_payload(event_type: &str, verification_status: &str) -> serde_json::Value {
        json!({
            "event_id": "019ff400-3bc5-7cf0-b3d2-0728c077a930",
            "match_id": "019ff401-3bc5-7cf0-b3d2-0728c077a930",
            "event_type": event_type,
            "event_schema_version": 1,
            "verification_status": verification_status,
            "payload": {"fixture": "verification_change"}
        })
    }

    #[test]
    fn builds_a_notification_only_from_a_claimed_match_event() {
        let message = message(OutboxDeliveryStatus::Processing, valid_payload());
        let notification = build_realtime_match_notification(&message, RealtimeSequence::first())
            .expect("claimed match event should become a realtime notification");

        assert_eq!(notification.notification_id(), message.message_id.as_uuid());
        assert_eq!(
            notification.topic(),
            RealtimeTopic::match_topic(message.aggregate_id.as_uuid())
        );
        assert_eq!(notification.sequence(), RealtimeSequence::first());
        assert_eq!(notification.payload(), &message.payload);
    }

    #[test]
    fn rejects_unclaimed_or_identity_mismatched_messages() {
        let pending = message(OutboxDeliveryStatus::Pending, valid_payload());
        assert!(matches!(
            build_realtime_match_notification(&pending, RealtimeSequence::first()),
            Err(RealtimeNotificationBuildError::MessageNotProcessing { .. })
        ));

        let mut mismatched = message(OutboxDeliveryStatus::Processing, valid_payload());
        mismatched.payload["match_id"] = json!("019ff4ff-3bc5-7cf0-b3d2-0728c077a930");
        assert!(matches!(
            build_realtime_match_notification(&mismatched, RealtimeSequence::first()),
            Err(RealtimeNotificationBuildError::InvalidPayloadField {
                field: "match_id",
                ..
            })
        ));
    }

    #[test]
    fn routes_event_voided_rows_to_a_distinct_correction_notification() {
        let message = message(OutboxDeliveryStatus::Processing, correction_payload());
        let sequence = RealtimeSequence::try_new(2).unwrap();
        let correction = build_realtime_match_correction_notification(&message, sequence)
            .expect("claimed EventVoided row should become a correction notification");

        assert_eq!(correction.notification_type(), "match.correction");
        assert_eq!(correction.schema_version(), 1);
        assert_eq!(correction.notification_id(), message.message_id.as_uuid());
        assert_eq!(
            correction.topic(),
            RealtimeTopic::match_topic(message.aggregate_id.as_uuid())
        );
        assert_eq!(correction.sequence(), sequence);
        assert_eq!(correction.match_id(), message.aggregate_id.as_uuid());
        assert_eq!(
            correction.correction_event_id(),
            message.source_event_id.as_uuid()
        );
        assert_eq!(
            correction.voids_event_id(),
            uuid("019ff404-3bc5-7cf0-b3d2-0728c077a930")
        );
        assert_eq!(correction.reason(), "official protocol removed the goal");

        let wire = serde_json::to_value(&correction)
            .expect("correction notification should serialize for transport");
        assert_eq!(wire["type"], "match.correction");
        assert_eq!(wire["schema_version"], 1);
        assert_eq!(
            wire["notification_id"],
            message.message_id.as_uuid().to_string()
        );
        assert_eq!(wire["topic"], "match:019ff401-3bc5-7cf0-b3d2-0728c077a930");
        assert_eq!(wire["sequence"], 2);
        assert_eq!(
            wire["voids_event_id"],
            "019ff404-3bc5-7cf0-b3d2-0728c077a930"
        );
        assert_eq!(wire["reason"], "official protocol removed the goal");

        let routed = build_realtime_notification(&message, sequence)
            .expect("generic builder should select the correction variant");
        assert!(matches!(routed, RealtimeNotification::MatchCorrection(_)));
        assert!(matches!(
            build_realtime_match_notification(&message, sequence),
            Err(RealtimeNotificationBuildError::CorrectionRequiresDedicatedNotification { .. })
        ));
    }

    #[test]
    fn rejects_malformed_correction_fields_before_distribution() {
        let mut blank_reason = message(OutboxDeliveryStatus::Processing, correction_payload());
        blank_reason.payload["payload"]["reason"] = json!(" \t");
        assert!(matches!(
            build_realtime_notification(&blank_reason, RealtimeSequence::first()),
            Err(RealtimeNotificationBuildError::InvalidPayloadField {
                field: "payload.reason",
                ..
            })
        ));

        let mut self_reference = message(OutboxDeliveryStatus::Processing, correction_payload());
        self_reference.payload["voids_event_id"] = json!("019ff400-3bc5-7cf0-b3d2-0728c077a930");
        assert!(matches!(
            build_realtime_notification(&self_reference, RealtimeSequence::first()),
            Err(RealtimeNotificationBuildError::InvalidPayloadField {
                field: "voids_event_id",
                ..
            })
        ));
    }

    #[test]
    fn routes_verification_events_to_a_distinct_notification_with_status() {
        let verification_message = message(
            OutboxDeliveryStatus::Processing,
            verification_payload("MatchVerified", "verified"),
        );
        let sequence = RealtimeSequence::try_new(3).unwrap();
        let verification =
            build_realtime_match_verification_notification(&verification_message, sequence)
                .expect("claimed MatchVerified row should become a verification notification");

        assert_eq!(verification.notification_type(), "match.verification");
        assert_eq!(verification.schema_version(), 1);
        assert_eq!(
            verification.notification_id(),
            verification_message.message_id.as_uuid()
        );
        assert_eq!(
            verification.topic(),
            RealtimeTopic::match_topic(verification_message.aggregate_id.as_uuid())
        );
        assert_eq!(verification.sequence(), sequence);
        assert_eq!(
            verification.match_id(),
            verification_message.aggregate_id.as_uuid()
        );
        assert_eq!(
            verification.verification_event_id(),
            verification_message.source_event_id.as_uuid()
        );
        assert_eq!(
            verification.change_type(),
            RealtimeVerificationChangeType::MatchVerified
        );
        assert_eq!(
            verification.verification_status(),
            MatchSnapshotVerificationStatus::Verified
        );
        assert_eq!(verification.payload(), &verification_message.payload);

        let wire = serde_json::to_value(&verification)
            .expect("verification notification should serialize for transport");
        assert_eq!(wire["type"], "match.verification");
        assert_eq!(wire["change_type"], "match_verified");
        assert_eq!(wire["verification_status"], "verified");
        assert_eq!(wire["sequence"], 3);
        assert_eq!(wire["payload"], verification_message.payload);

        let routed = build_realtime_notification(&verification_message, sequence)
            .expect("generic builder should select the verification variant");
        assert!(matches!(routed, RealtimeNotification::MatchVerification(_)));
        assert!(matches!(
            build_realtime_match_notification(&verification_message, sequence),
            Err(RealtimeNotificationBuildError::VerificationRequiresDedicatedNotification { .. })
        ));

        let reopened = message(
            OutboxDeliveryStatus::Processing,
            verification_payload("MatchReopened", "provisional"),
        );
        let reopened_notification = build_realtime_match_verification_notification(
            &reopened,
            RealtimeSequence::try_new(4).unwrap(),
        )
        .expect("claimed MatchReopened row should become a verification notification");
        assert_eq!(
            reopened_notification.change_type(),
            RealtimeVerificationChangeType::MatchReopened
        );
        assert_eq!(
            reopened_notification.verification_status(),
            MatchSnapshotVerificationStatus::Provisional
        );
    }

    #[test]
    fn rejects_unknown_verification_status_before_distribution() {
        let message = message(
            OutboxDeliveryStatus::Processing,
            verification_payload("MatchVerified", "operator"),
        );
        assert!(matches!(
            build_realtime_notification(&message, RealtimeSequence::first()),
            Err(RealtimeNotificationBuildError::InvalidPayloadField {
                field: "verification_status",
                ..
            })
        ));
    }
}
