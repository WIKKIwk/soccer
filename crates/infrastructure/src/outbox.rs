use football_domain::{
    AggregateVersion, CommandId, DomainEventEnvelope, EventId, MatchId, UtcInstant,
};
use serde_json::{Value, json};
use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction, types::Json};
use std::fmt;
use time::Duration;
use uuid::Uuid;

pub const MATCH_EVENT_MESSAGE_TYPE: &str = "match.event";
pub const MATCH_EVENT_ENVELOPE_SCHEMA_VERSION: u32 = 1;
pub const OUTBOX_RETRY_BASE_DELAY_SECONDS: u64 = 1;
pub const OUTBOX_RETRY_MAX_DELAY_SECONDS: u64 = 300;
pub const OUTBOX_MAX_DELIVERY_ATTEMPTS: u32 = 10;
pub const OUTBOX_POISON_REASON_MAX_ATTEMPTS: &str = "max_delivery_attempts_exceeded";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxDeliveryStatus {
    Pending,
    Processing,
    Published,
    Poisoned,
}

impl OutboxDeliveryStatus {
    pub const fn as_persisted_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Published => "published",
            Self::Poisoned => "poisoned",
        }
    }

    fn try_from_persisted(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "published" => Some(Self::Published),
            "poisoned" => Some(Self::Poisoned),
            _ => None,
        }
    }
}

impl fmt::Display for OutboxDeliveryStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_persisted_str())
    }
}

/// The first committed outbox message shape for one canonical match event.
///
/// `message_id` intentionally equals the source event ID in this slice. That
/// gives one stable message identity for one match-event notification while a
/// later delivery increments can add retry metadata and publish completion.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxMessage {
    message_id: EventId,
    aggregate_id: MatchId,
    source_aggregate_version: AggregateVersion,
    causation_command_id: CommandId,
    source_event_id: EventId,
    message_type: String,
    schema_version: u32,
    payload: Value,
    created_at: UtcInstant,
}

impl OutboxMessage {
    pub fn from_encoded_match_event(
        event: &DomainEventEnvelope<Value>,
        created_at: UtcInstant,
    ) -> Result<Self, OutboxMessageError> {
        if !event.payload().is_object() {
            return Err(OutboxMessageError::PayloadNotObject {
                event_id: event.event_id(),
            });
        }

        Ok(Self {
            message_id: event.event_id(),
            aggregate_id: event.match_id(),
            source_aggregate_version: event.aggregate_version(),
            causation_command_id: event.causation_command_id(),
            source_event_id: event.event_id(),
            message_type: MATCH_EVENT_MESSAGE_TYPE.to_owned(),
            schema_version: MATCH_EVENT_ENVELOPE_SCHEMA_VERSION,
            payload: json!({
                "event_id": event.event_id().to_string(),
                "match_id": event.match_id().to_string(),
                "aggregate_version": event.aggregate_version().value(),
                "causation_command_id": event.causation_command_id().to_string(),
                "event_index": event.event_index().value(),
                "event_type": event.event_type().as_str(),
                "event_schema_version": event.event_schema_version().value(),
                "period": event.period().as_persisted_str(),
                "clock_ms": event.clock_ms().value(),
                "occurred_at": event.occurred_at().as_offset_datetime().to_string(),
                "recorded_at": event.recorded_at().as_offset_datetime().to_string(),
                "actor_id": event.actor().admin_user_id().to_string(),
                "verification_status": event.verification_status().as_persisted_str(),
                "payload": event.payload(),
                "supersedes_event_id": event.supersedes_event_id().map(|id| id.to_string()),
                "voids_event_id": event.voids_event_id().map(|id| id.to_string()),
            }),
            created_at,
        })
    }

    pub const fn message_id(&self) -> EventId {
        self.message_id
    }

    pub const fn aggregate_id(&self) -> MatchId {
        self.aggregate_id
    }

    pub const fn source_aggregate_version(&self) -> AggregateVersion {
        self.source_aggregate_version
    }

    pub const fn causation_command_id(&self) -> CommandId {
        self.causation_command_id
    }

    pub const fn source_event_id(&self) -> EventId {
        self.source_event_id
    }

    pub fn message_type(&self) -> &str {
        &self.message_type
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }

    pub const fn created_at(&self) -> UtcInstant {
        self.created_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxMessageError {
    NoSourceEvent { match_id: MatchId },
    PayloadNotObject { event_id: EventId },
}

impl fmt::Display for OutboxMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSourceEvent { match_id } => {
                write!(
                    formatter,
                    "outbox match {match_id} has no appended source event"
                )
            }
            Self::PayloadNotObject { event_id } => {
                write!(
                    formatter,
                    "outbox event {event_id} payload must be a JSON object"
                )
            }
        }
    }
}

impl std::error::Error for OutboxMessageError {}

#[derive(Debug)]
pub enum OutboxRepositoryError {
    Database(SqlxError),
    Entry(OutboxMessageError),
    InvalidClaimBatchSize {
        value: u32,
    },
    InvalidClaimLease,
    RetryClaimNotOwnedOrExpired {
        message_id: EventId,
    },
    RetryAttemptCountZero {
        message_id: EventId,
    },
    PublishClaimNotOwnedOrExpired {
        message_id: EventId,
    },
    CannotPersistSourceVersion {
        event_id: EventId,
        value: u64,
    },
    CannotPersistSchemaVersion {
        event_id: EventId,
        value: u32,
    },
    InvalidStoredDeliveryStatus {
        message_id: EventId,
        value: String,
    },
    InvalidStoredAttemptCount {
        message_id: EventId,
        value: i32,
    },
    InvalidStoredClaimShape {
        message_id: EventId,
        status: OutboxDeliveryStatus,
    },
    InvalidStoredPoisonShape {
        message_id: EventId,
    },
    InvalidStoredPublishedShape {
        message_id: EventId,
    },
    InvalidStoredSourceVersion {
        message_id: EventId,
        value: i64,
    },
    InvalidStoredSchemaVersion {
        message_id: EventId,
        value: i32,
    },
}

impl fmt::Display for OutboxRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "outbox database operation failed: {error}"),
            Self::Entry(error) => write!(formatter, "outbox message is invalid: {error}"),
            Self::InvalidClaimBatchSize { value } => {
                write!(
                    formatter,
                    "outbox claim batch size {value} must be positive"
                )
            }
            Self::InvalidClaimLease => {
                formatter.write_str("outbox claim expiry must be later than claim time")
            }
            Self::RetryClaimNotOwnedOrExpired { message_id } => write!(
                formatter,
                "outbox retry claim for message {message_id} is not owned or has expired"
            ),
            Self::RetryAttemptCountZero { message_id } => write!(
                formatter,
                "outbox retry message {message_id} has no recorded claim attempt"
            ),
            Self::PublishClaimNotOwnedOrExpired { message_id } => write!(
                formatter,
                "outbox publish claim for message {message_id} is not owned or has expired"
            ),
            Self::CannotPersistSourceVersion { event_id, value } => write!(
                formatter,
                "outbox event {event_id} source version {value} does not fit PostgreSQL BIGINT"
            ),
            Self::CannotPersistSchemaVersion { event_id, value } => write!(
                formatter,
                "outbox event {event_id} schema version {value} does not fit PostgreSQL INTEGER"
            ),
            Self::InvalidStoredDeliveryStatus { message_id, value } => write!(
                formatter,
                "stored outbox message {message_id} has invalid delivery status {value}"
            ),
            Self::InvalidStoredAttemptCount { message_id, value } => write!(
                formatter,
                "stored outbox message {message_id} has invalid attempt count {value}"
            ),
            Self::InvalidStoredClaimShape { message_id, status } => write!(
                formatter,
                "stored outbox message {message_id} has invalid claim shape for status {status}"
            ),
            Self::InvalidStoredPoisonShape { message_id } => write!(
                formatter,
                "stored outbox message {message_id} has invalid poison metadata"
            ),
            Self::InvalidStoredPublishedShape { message_id } => write!(
                formatter,
                "stored outbox message {message_id} has invalid published metadata"
            ),
            Self::InvalidStoredSourceVersion { message_id, value } => write!(
                formatter,
                "stored outbox message {message_id} has invalid source version {value}"
            ),
            Self::InvalidStoredSchemaVersion { message_id, value } => write!(
                formatter,
                "stored outbox message {message_id} has invalid schema version {value}"
            ),
        }
    }
}

impl std::error::Error for OutboxRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Entry(error) => Some(error),
            Self::InvalidClaimBatchSize { .. }
            | Self::InvalidClaimLease
            | Self::RetryClaimNotOwnedOrExpired { .. }
            | Self::RetryAttemptCountZero { .. }
            | Self::PublishClaimNotOwnedOrExpired { .. }
            | Self::CannotPersistSourceVersion { .. }
            | Self::CannotPersistSchemaVersion { .. }
            | Self::InvalidStoredDeliveryStatus { .. }
            | Self::InvalidStoredAttemptCount { .. }
            | Self::InvalidStoredClaimShape { .. }
            | Self::InvalidStoredPoisonShape { .. }
            | Self::InvalidStoredPublishedShape { .. }
            | Self::InvalidStoredSourceVersion { .. }
            | Self::InvalidStoredSchemaVersion { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredOutboxMessage {
    pub message_id: EventId,
    pub aggregate_id: MatchId,
    pub source_aggregate_version: u64,
    pub causation_command_id: CommandId,
    pub source_event_id: EventId,
    pub message_type: String,
    pub schema_version: u32,
    pub payload: Value,
    pub created_at: UtcInstant,
    pub next_attempt_at: UtcInstant,
    pub delivery_status: OutboxDeliveryStatus,
    pub attempt_count: u32,
    pub claim_owner: Option<Uuid>,
    pub claim_expires_at: Option<UtcInstant>,
    pub poisoned_at: Option<UtcInstant>,
    pub poison_reason: Option<String>,
    pub published_at: Option<UtcInstant>,
}

#[derive(Debug, sqlx::FromRow)]
struct StoredOutboxMessageRow {
    message_id: Uuid,
    aggregate_id: Uuid,
    source_aggregate_version: i64,
    causation_command_id: Uuid,
    source_event_id: Uuid,
    message_type: String,
    schema_version: i32,
    payload: Json<Value>,
    created_at: time::OffsetDateTime,
    next_attempt_at: time::OffsetDateTime,
    delivery_status: String,
    attempt_count: i32,
    claim_owner: Option<Uuid>,
    claim_expires_at: Option<time::OffsetDateTime>,
    poisoned_at: Option<time::OffsetDateTime>,
    poison_reason: Option<String>,
    published_at: Option<time::OffsetDateTime>,
}

impl TryFrom<StoredOutboxMessageRow> for StoredOutboxMessage {
    type Error = OutboxRepositoryError;

    fn try_from(row: StoredOutboxMessageRow) -> Result<Self, Self::Error> {
        let message_id = EventId::from_uuid(row.message_id);
        let delivery_status = OutboxDeliveryStatus::try_from_persisted(&row.delivery_status)
            .ok_or_else(|| OutboxRepositoryError::InvalidStoredDeliveryStatus {
                message_id,
                value: row.delivery_status.clone(),
            })?;
        let attempt_count = u32::try_from(row.attempt_count).map_err(|_| {
            OutboxRepositoryError::InvalidStoredAttemptCount {
                message_id,
                value: row.attempt_count,
            }
        })?;
        let source_aggregate_version =
            u64::try_from(row.source_aggregate_version).map_err(|_| {
                OutboxRepositoryError::InvalidStoredSourceVersion {
                    message_id,
                    value: row.source_aggregate_version,
                }
            })?;
        let schema_version = u32::try_from(row.schema_version).map_err(|_| {
            OutboxRepositoryError::InvalidStoredSchemaVersion {
                message_id,
                value: row.schema_version,
            }
        })?;
        let claim_expires_at = row.claim_expires_at.map(UtcInstant::from_offset_datetime);
        let claim_shape_is_valid = match delivery_status {
            OutboxDeliveryStatus::Processing => {
                row.claim_owner.is_some() && claim_expires_at.is_some()
            }
            OutboxDeliveryStatus::Pending
            | OutboxDeliveryStatus::Published
            | OutboxDeliveryStatus::Poisoned => {
                row.claim_owner.is_none() && claim_expires_at.is_none()
            }
        };
        if !claim_shape_is_valid {
            return Err(OutboxRepositoryError::InvalidStoredClaimShape {
                message_id,
                status: delivery_status,
            });
        }
        let poison_shape_is_valid = match delivery_status {
            OutboxDeliveryStatus::Poisoned => {
                row.poisoned_at.is_some()
                    && row
                        .poison_reason
                        .as_deref()
                        .is_some_and(|reason| !reason.trim().is_empty())
            }
            OutboxDeliveryStatus::Pending
            | OutboxDeliveryStatus::Processing
            | OutboxDeliveryStatus::Published => {
                row.poisoned_at.is_none() && row.poison_reason.is_none()
            }
        };
        if !poison_shape_is_valid {
            return Err(OutboxRepositoryError::InvalidStoredPoisonShape { message_id });
        }
        let published_shape_is_valid = match delivery_status {
            OutboxDeliveryStatus::Published => row.published_at.is_some(),
            OutboxDeliveryStatus::Pending
            | OutboxDeliveryStatus::Processing
            | OutboxDeliveryStatus::Poisoned => row.published_at.is_none(),
        };
        if !published_shape_is_valid {
            return Err(OutboxRepositoryError::InvalidStoredPublishedShape { message_id });
        }

        Ok(Self {
            message_id,
            aggregate_id: MatchId::from_uuid(row.aggregate_id),
            source_aggregate_version,
            causation_command_id: CommandId::from_uuid(row.causation_command_id),
            source_event_id: EventId::from_uuid(row.source_event_id),
            message_type: row.message_type,
            schema_version,
            payload: row.payload.0,
            created_at: UtcInstant::from_offset_datetime(row.created_at),
            next_attempt_at: UtcInstant::from_offset_datetime(row.next_attempt_at),
            delivery_status,
            attempt_count,
            claim_owner: row.claim_owner,
            claim_expires_at,
            poisoned_at: row.poisoned_at.map(UtcInstant::from_offset_datetime),
            poison_reason: row.poison_reason,
            published_at: row.published_at.map(UtcInstant::from_offset_datetime),
        })
    }
}

pub async fn persist_outbox_message(
    transaction: &mut Transaction<'_, Postgres>,
    message: &OutboxMessage,
) -> Result<(), OutboxRepositoryError> {
    let source_aggregate_version = i64::try_from(message.source_aggregate_version().value())
        .map_err(|_| OutboxRepositoryError::CannotPersistSourceVersion {
            event_id: message.source_event_id(),
            value: message.source_aggregate_version().value(),
        })?;
    let schema_version = i32::try_from(message.schema_version()).map_err(|_| {
        OutboxRepositoryError::CannotPersistSchemaVersion {
            event_id: message.source_event_id(),
            value: message.schema_version(),
        }
    })?;

    sqlx::query(
        r#"
        INSERT INTO outbox_messages (
            message_id,
            aggregate_type,
            aggregate_id,
            source_aggregate_version,
            causation_command_id,
            source_event_id,
            message_type,
            schema_version,
            payload,
            created_at,
            next_attempt_at,
            delivery_status,
            attempt_count
        )
        VALUES ($1, 'match', $2, $3, $4, $5, $6, $7, $8, $9, $9, 'pending', 0)
        "#,
    )
    .bind(message.message_id().as_uuid())
    .bind(message.aggregate_id().as_uuid())
    .bind(source_aggregate_version)
    .bind(message.causation_command_id().as_uuid())
    .bind(message.source_event_id().as_uuid())
    .bind(message.message_type())
    .bind(schema_version)
    .bind(Json(message.payload().clone()))
    .bind(message.created_at().as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(OutboxRepositoryError::Database)?;

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxRetrySchedule {
    pub message_id: EventId,
    pub attempt_count: u32,
    pub next_attempt_at: UtcInstant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxPoisonedMessage {
    pub message_id: EventId,
    pub attempt_count: u32,
    pub poisoned_at: UtcInstant,
    pub poison_reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxPublishFailureDisposition {
    Retry(OutboxRetrySchedule),
    Poisoned(OutboxPoisonedMessage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboxPublishedMessage {
    pub message_id: EventId,
    pub attempt_count: u32,
    pub published_at: UtcInstant,
}

/// Returns the deterministic exponential delay for a failed claim.
/// Attempt one waits one second; the delay doubles until the five-minute cap.
pub const fn outbox_retry_delay_seconds(attempt_count: u32) -> u64 {
    if attempt_count == 0 {
        return 0;
    }

    let mut shift = attempt_count.saturating_sub(1);
    if shift > 63 {
        shift = 63;
    }
    let exponential = OUTBOX_RETRY_BASE_DELAY_SECONDS << shift;
    if exponential > OUTBOX_RETRY_MAX_DELAY_SECONDS {
        OUTBOX_RETRY_MAX_DELAY_SECONDS
    } else {
        exponential
    }
}

fn next_outbox_retry_at(failed_at: UtcInstant, attempt_count: u32) -> UtcInstant {
    UtcInstant::from_offset_datetime(
        failed_at.as_offset_datetime()
            + Duration::seconds(outbox_retry_delay_seconds(attempt_count) as i64),
    )
}

#[derive(Debug, sqlx::FromRow)]
struct OutboxRetryClaimRow {
    attempt_count: i32,
    claim_owner: Option<Uuid>,
    claim_expires_at: Option<time::OffsetDateTime>,
    delivery_status: String,
}

/// Resolves a worker-owned processing claim after a publish failure.
/// Below the attempt threshold the message returns to `pending` with a
/// deterministic delay; at the threshold it becomes terminally `poisoned`.
/// The caller owns the transaction and decides when this state is committed.
pub async fn handle_outbox_publish_failure(
    transaction: &mut Transaction<'_, Postgres>,
    message_id: EventId,
    worker_id: Uuid,
    failed_at: UtcInstant,
) -> Result<OutboxPublishFailureDisposition, OutboxRepositoryError> {
    let claim = sqlx::query_as::<_, OutboxRetryClaimRow>(
        r#"
        SELECT attempt_count, claim_owner, claim_expires_at, delivery_status
        FROM outbox_messages
        WHERE message_id = $1
        FOR UPDATE
        "#,
    )
    .bind(message_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(OutboxRepositoryError::Database)?
    .ok_or(OutboxRepositoryError::RetryClaimNotOwnedOrExpired { message_id })?;

    let claim_expires_at = claim.claim_expires_at.map(UtcInstant::from_offset_datetime);
    let is_owned_active_claim = claim.delivery_status == "processing"
        && claim.claim_owner == Some(worker_id)
        && claim_expires_at
            .map(|expires_at| expires_at > failed_at)
            .unwrap_or(false);
    if !is_owned_active_claim {
        return Err(OutboxRepositoryError::RetryClaimNotOwnedOrExpired { message_id });
    }

    let attempt_count = u32::try_from(claim.attempt_count).map_err(|_| {
        OutboxRepositoryError::InvalidStoredAttemptCount {
            message_id,
            value: claim.attempt_count,
        }
    })?;
    if attempt_count == 0 {
        return Err(OutboxRepositoryError::RetryAttemptCountZero { message_id });
    }

    if attempt_count >= OUTBOX_MAX_DELIVERY_ATTEMPTS {
        sqlx::query(
            r#"
            UPDATE outbox_messages
            SET delivery_status = 'poisoned',
                next_attempt_at = $2,
                claim_owner = NULL,
                claim_expires_at = NULL,
                poisoned_at = $2,
                poison_reason = $3,
                published_at = NULL
            WHERE message_id = $1
            "#,
        )
        .bind(message_id.as_uuid())
        .bind(failed_at.as_offset_datetime())
        .bind(OUTBOX_POISON_REASON_MAX_ATTEMPTS)
        .execute(&mut **transaction)
        .await
        .map_err(OutboxRepositoryError::Database)?;

        return Ok(OutboxPublishFailureDisposition::Poisoned(
            OutboxPoisonedMessage {
                message_id,
                attempt_count,
                poisoned_at: failed_at,
                poison_reason: OUTBOX_POISON_REASON_MAX_ATTEMPTS,
            },
        ));
    }

    let next_attempt_at = next_outbox_retry_at(failed_at, attempt_count);
    sqlx::query(
        r#"
        UPDATE outbox_messages
        SET delivery_status = 'pending',
            next_attempt_at = $2,
            claim_owner = NULL,
            claim_expires_at = NULL,
            poisoned_at = NULL,
            poison_reason = NULL,
            published_at = NULL
        WHERE message_id = $1
        "#,
    )
    .bind(message_id.as_uuid())
    .bind(next_attempt_at.as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(OutboxRepositoryError::Database)?;

    Ok(OutboxPublishFailureDisposition::Retry(
        OutboxRetrySchedule {
            message_id,
            attempt_count,
            next_attempt_at,
        },
    ))
}

/// Completes a successful publish for the worker's active claim.
///
/// The caller owns the transaction and decides when the acknowledgement is
/// committed. A worker cannot publish a message after its lease has expired or
/// after another worker has reclaimed the claim.
pub async fn mark_outbox_published(
    transaction: &mut Transaction<'_, Postgres>,
    message_id: EventId,
    worker_id: Uuid,
    published_at: UtcInstant,
) -> Result<OutboxPublishedMessage, OutboxRepositoryError> {
    let claim = sqlx::query_as::<_, OutboxRetryClaimRow>(
        r#"
        SELECT attempt_count, claim_owner, claim_expires_at, delivery_status
        FROM outbox_messages
        WHERE message_id = $1
        FOR UPDATE
        "#,
    )
    .bind(message_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(OutboxRepositoryError::Database)?
    .ok_or(OutboxRepositoryError::PublishClaimNotOwnedOrExpired { message_id })?;

    let claim_expires_at = claim.claim_expires_at.map(UtcInstant::from_offset_datetime);
    let is_owned_active_claim = claim.delivery_status == "processing"
        && claim.claim_owner == Some(worker_id)
        && claim_expires_at
            .map(|expires_at| expires_at > published_at)
            .unwrap_or(false);
    if !is_owned_active_claim {
        return Err(OutboxRepositoryError::PublishClaimNotOwnedOrExpired { message_id });
    }

    let attempt_count = u32::try_from(claim.attempt_count).map_err(|_| {
        OutboxRepositoryError::InvalidStoredAttemptCount {
            message_id,
            value: claim.attempt_count,
        }
    })?;
    if attempt_count == 0 {
        return Err(OutboxRepositoryError::RetryAttemptCountZero { message_id });
    }

    sqlx::query(
        r#"
        UPDATE outbox_messages
        SET delivery_status = 'published',
            claim_owner = NULL,
            claim_expires_at = NULL,
            poisoned_at = NULL,
            poison_reason = NULL,
            published_at = $2
        WHERE message_id = $1
        "#,
    )
    .bind(message_id.as_uuid())
    .bind(published_at.as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(OutboxRepositoryError::Database)?;

    Ok(OutboxPublishedMessage {
        message_id,
        attempt_count,
        published_at,
    })
}

/// Claims pending or expired-processing messages in the caller's transaction.
/// PostgreSQL row locks and `SKIP LOCKED` ensure concurrent workers receive
/// disjoint rows while the caller retains the transaction until the claim is
/// committed.
pub async fn claim_outbox_messages(
    transaction: &mut Transaction<'_, Postgres>,
    worker_id: Uuid,
    now_utc: UtcInstant,
    claim_expires_at: UtcInstant,
    batch_size: u32,
) -> Result<Vec<StoredOutboxMessage>, OutboxRepositoryError> {
    if batch_size == 0 {
        return Err(OutboxRepositoryError::InvalidClaimBatchSize { value: batch_size });
    }
    if claim_expires_at <= now_utc {
        return Err(OutboxRepositoryError::InvalidClaimLease);
    }

    let rows = sqlx::query_as::<_, StoredOutboxMessageRow>(
        r#"
        WITH candidates AS (
            SELECT message_id
            FROM outbox_messages
            WHERE (
                   delivery_status = 'pending'
                   AND next_attempt_at <= $2
               )
               OR (
                   delivery_status = 'processing'
                   AND claim_expires_at <= $2
               )
            ORDER BY created_at, message_id
            FOR UPDATE SKIP LOCKED
            LIMIT $1
        ), claimed AS (
            UPDATE outbox_messages AS outbox
            SET delivery_status = 'processing',
                attempt_count = outbox.attempt_count + 1,
                claim_owner = $3,
                claim_expires_at = $4
            FROM candidates
            WHERE outbox.message_id = candidates.message_id
            RETURNING outbox.*
        )
        SELECT message_id,
               aggregate_id,
               source_aggregate_version,
               causation_command_id,
               source_event_id,
               message_type,
               schema_version,
               payload,
               created_at,
               next_attempt_at,
               delivery_status,
               attempt_count,
               claim_owner,
               claim_expires_at,
               poisoned_at,
               poison_reason,
               published_at
        FROM claimed
        ORDER BY created_at, message_id
        "#,
    )
    .bind(i64::from(batch_size))
    .bind(now_utc.as_offset_datetime())
    .bind(worker_id)
    .bind(claim_expires_at.as_offset_datetime())
    .fetch_all(&mut **transaction)
    .await
    .map_err(OutboxRepositoryError::Database)?;

    rows.into_iter().map(TryInto::try_into).collect()
}

pub async fn load_outbox_message(
    pool: &PgPool,
    message_id: EventId,
) -> Result<StoredOutboxMessage, OutboxRepositoryError> {
    let row = sqlx::query_as::<_, StoredOutboxMessageRow>(
        r#"
        SELECT message_id,
               aggregate_id,
               source_aggregate_version,
               causation_command_id,
               source_event_id,
               message_type,
               schema_version,
               payload,
               created_at,
               next_attempt_at,
               delivery_status,
               attempt_count,
               claim_owner,
               claim_expires_at,
               poisoned_at,
               poison_reason,
               published_at
        FROM outbox_messages
        WHERE message_id = $1
        "#,
    )
    .bind(message_id.as_uuid())
    .fetch_one(pool)
    .await
    .map_err(OutboxRepositoryError::Database)?;

    row.try_into()
}

#[cfg(test)]
mod tests {
    use super::{
        MATCH_EVENT_ENVELOPE_SCHEMA_VERSION, MATCH_EVENT_MESSAGE_TYPE,
        OUTBOX_RETRY_BASE_DELAY_SECONDS, OUTBOX_RETRY_MAX_DELAY_SECONDS, OutboxDeliveryStatus,
        OutboxMessage, outbox_retry_delay_seconds,
    };
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, ClockMilliseconds, CommandId,
        DomainEventEnvelope, EventId, EventIndex, EventVerificationStatus, GoalAwardType,
        GoalRecordedPayload, MatchId, MatchPeriod, TeamId, UtcInstant,
    };
    use serde_json::{Value, json};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("outbox fixture UUID should be valid")
    }

    fn event() -> DomainEventEnvelope<Value> {
        let event_id = EventId::from_uuid(uuid("019fe990-3bc5-7cf0-b3d2-0728c077a930"));
        let payload = C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
            TeamId::from_uuid(uuid("019fe990-3bc5-7cf1-b3d2-0728c077a930")),
            None,
            GoalAwardType::Regular,
        ));
        DomainEventEnvelope::new(
            event_id,
            MatchId::from_uuid(uuid("019fe990-3bc5-7cf2-b3d2-0728c077a930")),
            AggregateVersion::from_value(1),
            CommandId::from_uuid(uuid("019fe990-3bc5-7cf3-b3d2-0728c077a930")),
            EventIndex::initial(),
            payload.event_type().try_as_event_type().unwrap(),
            payload.schema_version(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(12).unwrap(),
            UtcInstant::from_unix_timestamp(1_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_001).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe990-3bc5-7cf4-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            json!({
                "team_id": "019fe990-3bc5-7cf1-b3d2-0728c077a930",
                "scorer_player_id": null,
                "award_type": "regular"
            }),
            None,
            None,
        )
    }

    #[test]
    fn builds_a_versioned_message_without_copying_command_request_data() {
        let message = OutboxMessage::from_encoded_match_event(
            &event(),
            UtcInstant::from_unix_timestamp(1_002).unwrap(),
        )
        .unwrap();

        assert_eq!(message.message_id(), event().event_id());
        assert_eq!(message.source_event_id(), event().event_id());
        assert_eq!(message.message_type(), MATCH_EVENT_MESSAGE_TYPE);
        assert_eq!(
            message.schema_version(),
            MATCH_EVENT_ENVELOPE_SCHEMA_VERSION
        );
        assert_eq!(message.payload()["event_type"], "GoalRecorded");
        assert_eq!(message.payload()["payload"]["award_type"], "regular");
        assert!(message.payload().get("request_fingerprint").is_none());
        assert!(message.payload().get("client_command_id").is_none());
    }

    #[test]
    fn delivery_status_has_a_closed_persisted_vocabulary() {
        for (status, persisted) in [
            (OutboxDeliveryStatus::Pending, "pending"),
            (OutboxDeliveryStatus::Processing, "processing"),
            (OutboxDeliveryStatus::Published, "published"),
            (OutboxDeliveryStatus::Poisoned, "poisoned"),
        ] {
            assert_eq!(status.as_persisted_str(), persisted);
        }
    }

    #[test]
    fn retry_delay_is_deterministic_exponential_and_capped() {
        assert_eq!(outbox_retry_delay_seconds(0), 0);
        assert_eq!(
            outbox_retry_delay_seconds(1),
            OUTBOX_RETRY_BASE_DELAY_SECONDS
        );
        assert_eq!(outbox_retry_delay_seconds(2), 2);
        assert_eq!(outbox_retry_delay_seconds(3), 4);
        assert_eq!(outbox_retry_delay_seconds(9), 256);
        assert_eq!(
            outbox_retry_delay_seconds(10),
            OUTBOX_RETRY_MAX_DELAY_SECONDS
        );
        assert_eq!(
            outbox_retry_delay_seconds(u32::MAX),
            OUTBOX_RETRY_MAX_DELAY_SECONDS
        );
    }
}
