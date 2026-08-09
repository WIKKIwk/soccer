use super::c1_event_payloads::{
    C1EventPayloadCodecError, decode_c1_event_envelope, encode_c1_event_envelope,
};
use super::match_event_payloads::{
    MatchEventPayloadCodecError, decode_match_event_envelope, encode_match_event_envelope,
};
use football_domain::{
    ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, CanonicalEventStream,
    ClockMilliseconds, CommandId, DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion,
    EventType, EventTypeError, EventVerificationStatus, MatchEventPayload, MatchId, MatchPeriod,
    UtcInstant,
};
use serde_json::Value;
use sqlx::{Error as SqlxError, PgPool, Postgres, QueryBuilder, Transaction, types::Json};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub enum MatchEventRepositoryError {
    Database(SqlxError),
    InvalidStoredAggregateVersion {
        event_id: EventId,
        value: i64,
    },
    InvalidStoredAggregateMaxVersion {
        match_id: MatchId,
        value: i64,
    },
    InvalidStoredEventIndex {
        event_id: EventId,
        value: i64,
    },
    InvalidStoredSchemaVersion {
        event_id: EventId,
        value: i32,
    },
    InvalidStoredEventType {
        event_id: EventId,
        error: EventTypeError,
    },
    InvalidStoredPeriod {
        event_id: EventId,
        value: String,
    },
    InvalidStoredClock {
        event_id: EventId,
        value: i64,
    },
    InvalidStoredVerificationStatus {
        event_id: EventId,
        value: String,
    },
    InvalidEventStream(football_domain::EventStreamError),
    InvalidC1Payload(C1EventPayloadCodecError),
    InvalidMatchPayload(MatchEventPayloadCodecError),
    CannotPersistAggregateVersion {
        event_id: EventId,
        value: u64,
    },
    CannotPersistClock {
        event_id: EventId,
        value: u64,
    },
}

impl fmt::Display for MatchEventRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "match event database operation failed: {error}")
            }
            Self::InvalidStoredAggregateVersion { event_id, value } => write!(
                formatter,
                "stored event {event_id} has invalid aggregate version {value}"
            ),
            Self::InvalidStoredAggregateMaxVersion { match_id, value } => write!(
                formatter,
                "match {match_id} has invalid aggregate maximum version {value}"
            ),
            Self::InvalidStoredEventIndex { event_id, value } => write!(
                formatter,
                "stored event {event_id} has invalid event index {value}"
            ),
            Self::InvalidStoredSchemaVersion { event_id, value } => write!(
                formatter,
                "stored event {event_id} has invalid schema version {value}"
            ),
            Self::InvalidStoredEventType { event_id, error } => {
                write!(
                    formatter,
                    "stored event {event_id} has invalid event type: {error}"
                )
            }
            Self::InvalidStoredPeriod { event_id, value } => {
                write!(
                    formatter,
                    "stored event {event_id} has invalid period {value}"
                )
            }
            Self::InvalidStoredClock { event_id, value } => {
                write!(
                    formatter,
                    "stored event {event_id} has invalid clock value {value}"
                )
            }
            Self::InvalidStoredVerificationStatus { event_id, value } => write!(
                formatter,
                "stored event {event_id} has invalid verification status {value}"
            ),
            Self::InvalidEventStream(error) => {
                write!(formatter, "stored event stream is invalid: {error}")
            }
            Self::InvalidC1Payload(error) => {
                write!(formatter, "stored C1 event payload is invalid: {error}")
            }
            Self::InvalidMatchPayload(error) => {
                write!(
                    formatter,
                    "stored canonical match payload is invalid: {error}"
                )
            }
            Self::CannotPersistAggregateVersion { event_id, value } => write!(
                formatter,
                "event {event_id} aggregate version {value} does not fit PostgreSQL BIGINT"
            ),
            Self::CannotPersistClock { event_id, value } => write!(
                formatter,
                "event {event_id} clock value {value} does not fit PostgreSQL BIGINT"
            ),
        }
    }
}

impl std::error::Error for MatchEventRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredEventType { error, .. } => Some(error),
            Self::InvalidEventStream(error) => Some(error),
            Self::InvalidC1Payload(error) => Some(error),
            Self::InvalidMatchPayload(error) => Some(error),
            Self::InvalidStoredAggregateVersion { .. }
            | Self::InvalidStoredAggregateMaxVersion { .. }
            | Self::InvalidStoredEventIndex { .. }
            | Self::InvalidStoredSchemaVersion { .. }
            | Self::InvalidStoredPeriod { .. }
            | Self::InvalidStoredClock { .. }
            | Self::InvalidStoredVerificationStatus { .. }
            | Self::CannotPersistAggregateVersion { .. }
            | Self::CannotPersistClock { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct MatchEventRow {
    event_id: Uuid,
    match_id: Uuid,
    aggregate_version: i64,
    causation_command_id: Uuid,
    event_index: i64,
    event_type: String,
    event_schema_version: i32,
    period: String,
    clock_ms: i64,
    occurred_at: OffsetDateTime,
    recorded_at: OffsetDateTime,
    actor_id: Uuid,
    verification_status: String,
    payload: Json<Value>,
    supersedes_event_id: Option<Uuid>,
    voids_event_id: Option<Uuid>,
}

const MATCH_EVENT_SELECT: &str = r#"
        SELECT
            event_id,
            match_id,
            aggregate_version,
            causation_command_id,
            event_index,
            event_type,
            event_schema_version,
            period,
            clock_ms,
            occurred_at,
            recorded_at,
            actor_id,
            verification_status,
            payload,
            supersedes_event_id,
            voids_event_id
        FROM match_events
        WHERE match_id = $1
        ORDER BY aggregate_version ASC
        "#;

impl TryFrom<MatchEventRow> for DomainEventEnvelope<Value> {
    type Error = MatchEventRepositoryError;

    fn try_from(row: MatchEventRow) -> Result<Self, Self::Error> {
        let event_id = EventId::from_uuid(row.event_id);
        let aggregate_version = if row.aggregate_version > 0 {
            AggregateVersion::from_value(row.aggregate_version as u64)
        } else {
            return Err(MatchEventRepositoryError::InvalidStoredAggregateVersion {
                event_id,
                value: row.aggregate_version,
            });
        };
        let event_index = u32::try_from(row.event_index).map_err(|_| {
            MatchEventRepositoryError::InvalidStoredEventIndex {
                event_id,
                value: row.event_index,
            }
        })?;
        let event_schema_version = u16::try_from(row.event_schema_version)
            .ok()
            .and_then(|value| EventSchemaVersion::try_new(value).ok())
            .ok_or(MatchEventRepositoryError::InvalidStoredSchemaVersion {
                event_id,
                value: row.event_schema_version,
            })?;
        let event_type = EventType::try_new(row.event_type).map_err(|error| {
            MatchEventRepositoryError::InvalidStoredEventType { event_id, error }
        })?;
        let period = parse_period(event_id, row.period)?;
        let clock_ms = u64::try_from(row.clock_ms).map_err(|_| {
            MatchEventRepositoryError::InvalidStoredClock {
                event_id,
                value: row.clock_ms,
            }
        })?;
        let verification_status = parse_verification_status(event_id, row.verification_status)?;

        Ok(DomainEventEnvelope::new(
            event_id,
            MatchId::from_uuid(row.match_id),
            aggregate_version,
            CommandId::from_uuid(row.causation_command_id),
            EventIndex::from_value(event_index),
            event_type,
            event_schema_version,
            period,
            ClockMilliseconds::from_millis(clock_ms),
            UtcInstant::from_offset_datetime(row.occurred_at),
            UtcInstant::from_offset_datetime(row.recorded_at),
            ActorIdentity::administrator(AdminUserId::from_uuid(row.actor_id)),
            verification_status,
            row.payload.0,
            row.supersedes_event_id.map(EventId::from_uuid),
            row.voids_event_id.map(EventId::from_uuid),
        ))
    }
}

fn parse_period(
    event_id: EventId,
    value: String,
) -> Result<MatchPeriod, MatchEventRepositoryError> {
    MatchPeriod::try_from_persisted_str(&value)
        .ok_or(MatchEventRepositoryError::InvalidStoredPeriod { event_id, value })
}

fn parse_verification_status(
    event_id: EventId,
    value: String,
) -> Result<EventVerificationStatus, MatchEventRepositoryError> {
    EventVerificationStatus::try_from_persisted_str(&value)
        .ok_or(MatchEventRepositoryError::InvalidStoredVerificationStatus { event_id, value })
}

#[derive(Debug)]
struct PersistedEventValues {
    event_id: Uuid,
    match_id: Uuid,
    aggregate_version: i64,
    causation_command_id: Uuid,
    event_index: i64,
    event_type: String,
    event_schema_version: i32,
    period: String,
    clock_ms: i64,
    occurred_at: OffsetDateTime,
    recorded_at: OffsetDateTime,
    actor_id: Uuid,
    verification_status: String,
    payload: Json<Value>,
    supersedes_event_id: Option<Uuid>,
    voids_event_id: Option<Uuid>,
}

impl TryFrom<&DomainEventEnvelope<Value>> for PersistedEventValues {
    type Error = MatchEventRepositoryError;

    fn try_from(event: &DomainEventEnvelope<Value>) -> Result<Self, Self::Error> {
        let event_id = event.event_id();
        let aggregate_version = i64::try_from(event.aggregate_version().value()).map_err(|_| {
            MatchEventRepositoryError::CannotPersistAggregateVersion {
                event_id,
                value: event.aggregate_version().value(),
            }
        })?;
        let clock_ms = i64::try_from(event.clock_ms().value()).map_err(|_| {
            MatchEventRepositoryError::CannotPersistClock {
                event_id,
                value: event.clock_ms().value(),
            }
        })?;

        Ok(Self {
            event_id: event_id.as_uuid(),
            match_id: event.match_id().as_uuid(),
            aggregate_version,
            causation_command_id: event.causation_command_id().as_uuid(),
            event_index: i64::from(event.event_index().value()),
            event_type: event.event_type().as_str().to_owned(),
            event_schema_version: i32::from(event.event_schema_version().value()),
            period: period_to_storage(event.period()).to_owned(),
            clock_ms,
            occurred_at: event.occurred_at().as_offset_datetime(),
            recorded_at: event.recorded_at().as_offset_datetime(),
            actor_id: event.actor().admin_user_id().as_uuid(),
            verification_status: verification_status_to_storage(event.verification_status())
                .to_owned(),
            payload: Json(event.payload().clone()),
            supersedes_event_id: event.supersedes_event_id().map(EventId::as_uuid),
            voids_event_id: event.voids_event_id().map(EventId::as_uuid),
        })
    }
}

fn period_to_storage(period: MatchPeriod) -> &'static str {
    period.as_persisted_str()
}

fn verification_status_to_storage(status: EventVerificationStatus) -> &'static str {
    status.as_persisted_str()
}

/// Persists one already-versioned canonical event batch in the caller's
/// transaction. The single INSERT statement keeps the batch atomic; the
/// caller owns the surrounding command transaction and its commit decision.
pub async fn persist_match_event_batch(
    transaction: &mut Transaction<'_, Postgres>,
    current_version: AggregateVersion,
    events: &[DomainEventEnvelope<Value>],
) -> Result<(), MatchEventRepositoryError> {
    if events.is_empty() {
        return Ok(());
    }

    validate_persisted_event_batch(current_version, events)?;

    let values = events
        .iter()
        .map(PersistedEventValues::try_from)
        .collect::<Result<Vec<_>, MatchEventRepositoryError>>()?;

    let mut query = QueryBuilder::<Postgres>::new(
        "INSERT INTO match_events (event_id, match_id, aggregate_version, causation_command_id, event_index, event_type, event_schema_version, period, clock_ms, occurred_at, recorded_at, actor_id, verification_status, payload, supersedes_event_id, voids_event_id) ",
    );
    query.push_values(values, |mut bind, value| {
        bind.push_bind(value.event_id)
            .push_bind(value.match_id)
            .push_bind(value.aggregate_version)
            .push_bind(value.causation_command_id)
            .push_bind(value.event_index)
            .push_bind(value.event_type)
            .push_bind(value.event_schema_version)
            .push_bind(value.period)
            .push_bind(value.clock_ms)
            .push_bind(value.occurred_at)
            .push_bind(value.recorded_at)
            .push_bind(value.actor_id)
            .push_bind(value.verification_status)
            .push_bind(value.payload)
            .push_bind(value.supersedes_event_id)
            .push_bind(value.voids_event_id);
    });
    query
        .build()
        .execute(&mut **transaction)
        .await
        .map_err(MatchEventRepositoryError::Database)?;

    Ok(())
}

fn validate_persisted_event_batch(
    current_version: AggregateVersion,
    events: &[DomainEventEnvelope<Value>],
) -> Result<(), MatchEventRepositoryError> {
    let first_event = events
        .first()
        .expect("non-empty event batch should have a first event");

    CanonicalEventStream::validate_appended_batch(first_event.match_id(), current_version, events)
        .map_err(MatchEventRepositoryError::InvalidEventStream)
}

/// Persists a validated typed C1 event batch through the generic JSON event
/// adapter in the caller's transaction.
pub async fn persist_c1_event_batch(
    transaction: &mut Transaction<'_, Postgres>,
    current_version: AggregateVersion,
    events: &[DomainEventEnvelope<C1EventPayload>],
) -> Result<(), MatchEventRepositoryError> {
    let encoded_events = events
        .iter()
        .map(encode_c1_event_envelope)
        .collect::<Result<Vec<_>, C1EventPayloadCodecError>>()
        .map_err(MatchEventRepositoryError::InvalidC1Payload)?;

    persist_match_event_batch(transaction, current_version, &encoded_events).await
}

/// Persists a typed canonical match-event batch containing both C1 live
/// payloads and correction markers through the generic JSON event adapter.
pub async fn persist_canonical_match_event_batch(
    transaction: &mut Transaction<'_, Postgres>,
    current_version: AggregateVersion,
    events: &[DomainEventEnvelope<MatchEventPayload>],
) -> Result<(), MatchEventRepositoryError> {
    let encoded_events = events
        .iter()
        .map(encode_match_event_envelope)
        .collect::<Result<Vec<_>, MatchEventPayloadCodecError>>()
        .map_err(MatchEventRepositoryError::InvalidMatchPayload)?;

    persist_match_event_batch(transaction, current_version, &encoded_events).await
}

/// Loads one match's canonical events in aggregate-version order.
pub async fn load_match_event_stream(
    pool: &PgPool,
    match_id: MatchId,
) -> Result<CanonicalEventStream<Value>, MatchEventRepositoryError> {
    let rows = load_match_event_rows(pool, match_id).await?;

    build_raw_event_stream(match_id, rows)
}

/// Loads the canonical events caused by one internal command in command-local
/// event-index order. This is reconciliation evidence for an uncertain commit,
/// not a replacement for loading the complete aggregate stream.
pub async fn load_match_events_for_command(
    pool: &PgPool,
    command_id: CommandId,
) -> Result<Vec<DomainEventEnvelope<Value>>, MatchEventRepositoryError> {
    let rows = load_match_event_rows_for_command(pool, command_id).await?;

    build_raw_event_batch(rows)
}

/// Loads command-local canonical events through the caller-owned transaction.
pub async fn load_match_events_for_command_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
) -> Result<Vec<DomainEventEnvelope<Value>>, MatchEventRepositoryError> {
    let rows = load_match_event_rows_for_command(&mut **transaction, command_id).await?;

    build_raw_event_batch(rows)
}

/// Loads one match's canonical events through the caller-owned transaction.
///
/// The read uses the transaction snapshot supplied by the caller, so a later
/// command executor can load, append, and persist its terminal result without
/// switching to a separate pool-level read.
pub async fn load_match_event_stream_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<CanonicalEventStream<Value>, MatchEventRepositoryError> {
    let rows = load_match_event_rows(&mut **transaction, match_id).await?;

    build_raw_event_stream(match_id, rows)
}

/// Loads only the highest persisted aggregate version through the caller-owned
/// transaction snapshot. An empty match has the domain's initial version.
pub async fn load_match_event_max_version_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<AggregateVersion, MatchEventRepositoryError> {
    let max_version = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(aggregate_version) FROM match_events WHERE match_id = $1",
    )
    .bind(match_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(MatchEventRepositoryError::Database)?;

    match max_version {
        Some(value) => u64::try_from(value)
            .map(AggregateVersion::from_value)
            .map_err(
                |_| MatchEventRepositoryError::InvalidStoredAggregateMaxVersion { match_id, value },
            ),
        None => Ok(AggregateVersion::initial()),
    }
}

async fn load_match_event_rows<'e, E>(
    executor: E,
    match_id: MatchId,
) -> Result<Vec<MatchEventRow>, MatchEventRepositoryError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, MatchEventRow>(MATCH_EVENT_SELECT)
        .bind(match_id.as_uuid())
        .fetch_all(executor)
        .await
        .map_err(MatchEventRepositoryError::Database)
}

async fn load_match_event_rows_for_command<'e, E>(
    executor: E,
    command_id: CommandId,
) -> Result<Vec<MatchEventRow>, MatchEventRepositoryError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    sqlx::query_as::<_, MatchEventRow>(
        r#"
        SELECT
            event_id,
            match_id,
            aggregate_version,
            causation_command_id,
            event_index,
            event_type,
            event_schema_version,
            period,
            clock_ms,
            occurred_at,
            recorded_at,
            actor_id,
            verification_status,
            payload,
            supersedes_event_id,
            voids_event_id
        FROM match_events
        WHERE causation_command_id = $1
        ORDER BY event_index ASC
        "#,
    )
    .bind(command_id.as_uuid())
    .fetch_all(executor)
    .await
    .map_err(MatchEventRepositoryError::Database)
}

fn build_raw_event_batch(
    rows: Vec<MatchEventRow>,
) -> Result<Vec<DomainEventEnvelope<Value>>, MatchEventRepositoryError> {
    rows.into_iter()
        .map(DomainEventEnvelope::try_from)
        .collect()
}

fn build_raw_event_stream(
    match_id: MatchId,
    rows: Vec<MatchEventRow>,
) -> Result<CanonicalEventStream<Value>, MatchEventRepositoryError> {
    let events = rows
        .into_iter()
        .map(DomainEventEnvelope::try_from)
        .collect::<Result<Vec<_>, MatchEventRepositoryError>>()?;

    CanonicalEventStream::try_from_ordered(match_id, events)
        .map_err(MatchEventRepositoryError::InvalidEventStream)
}

/// Loads one match's canonical events and decodes the first typed C1 payload
/// set for domain reduction.
pub async fn load_c1_event_stream(
    pool: &PgPool,
    match_id: MatchId,
) -> Result<CanonicalEventStream<C1EventPayload>, MatchEventRepositoryError> {
    let raw_stream = load_match_event_stream(pool, match_id).await?;
    decode_c1_event_stream(raw_stream)
}

/// Loads and decodes the C1 events caused by one internal command. The result
/// preserves event metadata and command-local ordering while intentionally not
/// pretending to be a complete aggregate stream.
pub async fn load_c1_events_for_command(
    pool: &PgPool,
    command_id: CommandId,
) -> Result<Vec<DomainEventEnvelope<C1EventPayload>>, MatchEventRepositoryError> {
    let raw_events = load_match_events_for_command(pool, command_id).await?;

    decode_c1_event_batch(raw_events)
}

/// Loads and decodes one match's canonical C1 events through the caller-owned
/// transaction snapshot.
pub async fn load_c1_event_stream_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<CanonicalEventStream<C1EventPayload>, MatchEventRepositoryError> {
    let raw_stream = load_match_event_stream_in_transaction(transaction, match_id).await?;
    decode_c1_event_stream(raw_stream)
}

/// Loads and decodes command-local C1 events through the caller-owned
/// transaction snapshot.
pub async fn load_c1_events_for_command_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
) -> Result<Vec<DomainEventEnvelope<C1EventPayload>>, MatchEventRepositoryError> {
    let raw_events = load_match_events_for_command_in_transaction(transaction, command_id).await?;

    decode_c1_event_batch(raw_events)
}

/// Loads and decodes one match's canonical event stream into the typed C1 and
/// correction payload union used by effective-stream reduction.
pub async fn load_canonical_match_event_stream(
    pool: &PgPool,
    match_id: MatchId,
) -> Result<CanonicalEventStream<MatchEventPayload>, MatchEventRepositoryError> {
    let raw_stream = load_match_event_stream(pool, match_id).await?;
    decode_match_event_stream(raw_stream)
}

/// Loads and decodes the canonical match-event union through the caller-owned
/// transaction snapshot.
pub async fn load_canonical_match_event_stream_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<CanonicalEventStream<MatchEventPayload>, MatchEventRepositoryError> {
    let raw_stream = load_match_event_stream_in_transaction(transaction, match_id).await?;
    decode_match_event_stream(raw_stream)
}

/// Loads command-local canonical events with typed C1/correction payloads.
/// The result remains command-local evidence and is not treated as an
/// aggregate stream.
pub async fn load_canonical_match_events_for_command(
    pool: &PgPool,
    command_id: CommandId,
) -> Result<Vec<DomainEventEnvelope<MatchEventPayload>>, MatchEventRepositoryError> {
    let raw_events = load_match_events_for_command(pool, command_id).await?;
    decode_match_event_batch(raw_events)
}

/// Loads command-local canonical events with typed C1/correction payloads
/// through the caller-owned transaction snapshot.
pub async fn load_canonical_match_events_for_command_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
) -> Result<Vec<DomainEventEnvelope<MatchEventPayload>>, MatchEventRepositoryError> {
    let raw_events = load_match_events_for_command_in_transaction(transaction, command_id).await?;
    decode_match_event_batch(raw_events)
}

fn decode_c1_event_batch(
    raw_events: Vec<DomainEventEnvelope<Value>>,
) -> Result<Vec<DomainEventEnvelope<C1EventPayload>>, MatchEventRepositoryError> {
    raw_events
        .into_iter()
        .map(decode_c1_event_envelope)
        .collect::<Result<Vec<_>, C1EventPayloadCodecError>>()
        .map_err(MatchEventRepositoryError::InvalidC1Payload)
}

fn decode_c1_event_stream(
    raw_stream: CanonicalEventStream<Value>,
) -> Result<CanonicalEventStream<C1EventPayload>, MatchEventRepositoryError> {
    let match_id = raw_stream.match_id();
    let events = decode_c1_event_batch(raw_stream.into_events())?;

    CanonicalEventStream::try_from_ordered(match_id, events)
        .map_err(MatchEventRepositoryError::InvalidEventStream)
}

fn decode_match_event_batch(
    raw_events: Vec<DomainEventEnvelope<Value>>,
) -> Result<Vec<DomainEventEnvelope<MatchEventPayload>>, MatchEventRepositoryError> {
    raw_events
        .into_iter()
        .map(decode_match_event_envelope)
        .collect::<Result<Vec<_>, MatchEventPayloadCodecError>>()
        .map_err(MatchEventRepositoryError::InvalidMatchPayload)
}

fn decode_match_event_stream(
    raw_stream: CanonicalEventStream<Value>,
) -> Result<CanonicalEventStream<MatchEventPayload>, MatchEventRepositoryError> {
    let match_id = raw_stream.match_id();
    let events = decode_match_event_batch(raw_stream.into_events())?;

    CanonicalEventStream::try_from_ordered(match_id, events)
        .map_err(MatchEventRepositoryError::InvalidEventStream)
}

#[cfg(test)]
mod tests {
    use super::{
        MatchEventRepositoryError, MatchEventRow, PersistedEventValues, build_raw_event_batch,
        build_raw_event_stream, decode_match_event_stream, parse_period, parse_verification_status,
        validate_persisted_event_batch,
    };
    use football_domain::{
        AggregateVersion, CommandId, EventId, EventVerificationStatus, MatchEventPayload, MatchId,
        MatchPeriod, UtcInstant,
    };
    use serde_json::json;
    use sqlx::types::Json;
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("match event fixture UUID should be valid")
    }

    fn event_id() -> EventId {
        EventId::from_uuid(uuid("019fe2c0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn valid_row() -> MatchEventRow {
        MatchEventRow {
            event_id: uuid("019fe2c0-3bc5-7cf0-b3d2-0728c077a930"),
            match_id: uuid("019fe2c0-3bc5-7cf1-b3d2-0728c077a930"),
            aggregate_version: 1,
            causation_command_id: uuid("019fe2c0-3bc5-7cf2-b3d2-0728c077a930"),
            event_index: 0,
            event_type: "GoalRecorded".to_owned(),
            event_schema_version: 1,
            period: "first_half".to_owned(),
            clock_ms: 12_000,
            occurred_at: OffsetDateTime::from_unix_timestamp(1_777_000_000).unwrap(),
            recorded_at: OffsetDateTime::from_unix_timestamp(1_777_000_001).unwrap(),
            actor_id: uuid("019fe2c0-3bc5-7cf3-b3d2-0728c077a930"),
            verification_status: "provisional".to_owned(),
            payload: Json(json!({ "side": "home" })),
            supersedes_event_id: None,
            voids_event_id: None,
        }
    }

    #[test]
    fn builds_command_event_evidence_without_requiring_a_full_aggregate_stream() {
        let first = valid_row();
        let mut second = valid_row();
        second.event_id = uuid("019fe2c0-3bc5-7cf4-b3d2-0728c077a930");
        second.aggregate_version = 8;
        second.event_index = 1;

        let events = build_raw_event_batch(vec![first, second]).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_index().value(), 0);
        assert_eq!(events[1].event_index().value(), 1);
        assert_eq!(events[0].aggregate_version().value(), 1);
        assert_eq!(events[1].aggregate_version().value(), 8);
        assert_eq!(
            events[0].causation_command_id(),
            CommandId::from_uuid(uuid("019fe2c0-3bc5-7cf2-b3d2-0728c077a930"))
        );
    }

    #[test]
    fn maps_a_valid_row_to_the_domain_event_envelope() {
        let event: football_domain::DomainEventEnvelope<serde_json::Value> =
            valid_row().try_into().unwrap();

        assert_eq!(event.event_id(), event_id());
        assert_eq!(event.aggregate_version().value(), 1);
        assert_eq!(event.event_index().value(), 0);
        assert_eq!(event.event_type().as_str(), "GoalRecorded");
        assert_eq!(event.period(), MatchPeriod::FirstHalf);
        assert_eq!(event.clock_ms().value(), 12_000);
        assert_eq!(
            event.verification_status(),
            EventVerificationStatus::Provisional
        );
        assert_eq!(
            event.occurred_at(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap()
        );
        assert_eq!(event.payload(), &json!({ "side": "home" }));
    }

    #[test]
    fn decodes_a_canonical_stream_with_c1_and_voided_events_into_the_union() {
        let mut first = valid_row();
        first.payload = Json(json!({
            "team_id": uuid("019fe2c0-3bc5-7cf5-b3d2-0728c077a930").to_string(),
            "scorer_player_id": null,
            "award_type": "regular"
        }));
        let mut second = valid_row();
        second.event_id = uuid("019fe2c0-3bc5-7cf4-b3d2-0728c077a930");
        second.aggregate_version = 2;
        second.event_index = 1;
        second.event_type = "EventVoided".to_owned();
        second.period = "half_time".to_owned();
        second.payload = Json(json!({
            "reason": "official protocol removed the goal"
        }));
        second.supersedes_event_id = None;
        second.voids_event_id = Some(first.event_id);

        let mut third = valid_row();
        third.event_id = uuid("019fe2c0-3bc5-7cf6-b3d2-0728c077a930");
        third.causation_command_id = uuid("019fe2c0-3bc5-7cf7-b3d2-0728c077a930");
        third.aggregate_version = 3;
        third.event_index = 0;
        third.event_type = "MatchVerified".to_owned();
        third.event_schema_version = 1;
        third.period = "second_half".to_owned();
        third.clock_ms = 90_000;
        third.verification_status = "verified".to_owned();
        third.payload = Json(json!({}));
        third.supersedes_event_id = None;
        third.voids_event_id = None;

        let mut fourth = valid_row();
        fourth.event_id = uuid("019fe2c0-3bc5-7cf8-b3d2-0728c077a930");
        fourth.causation_command_id = uuid("019fe2c0-3bc5-7cf9-b3d2-0728c077a930");
        fourth.aggregate_version = 4;
        fourth.event_index = 0;
        fourth.event_type = "MatchReopened".to_owned();
        fourth.event_schema_version = 1;
        fourth.period = "second_half".to_owned();
        fourth.clock_ms = 91_000;
        fourth.verification_status = "provisional".to_owned();
        fourth.payload = Json(json!({
            "reason": "official protocol requires re-verification"
        }));
        fourth.supersedes_event_id = None;
        fourth.voids_event_id = None;

        let stream = build_raw_event_stream(
            MatchId::from_uuid(first.match_id),
            vec![first, second, third, fourth],
        )
        .unwrap();
        let typed = decode_match_event_stream(stream).unwrap();

        assert!(matches!(
            typed.events()[0].payload(),
            MatchEventPayload::C1(_)
        ));
        assert!(matches!(
            typed.events()[1].payload(),
            MatchEventPayload::Correction(_)
        ));
        assert!(matches!(
            typed.events()[2].payload(),
            MatchEventPayload::Verification(_)
        ));
        assert_eq!(
            typed.events()[1].voids_event_id(),
            Some(typed.events()[0].event_id())
        );
        assert_eq!(typed.events()[2].event_type().as_str(), "MatchVerified");
        assert_eq!(
            typed.events()[2].verification_status(),
            EventVerificationStatus::Verified
        );
        assert!(matches!(
            typed.events()[3].payload(),
            MatchEventPayload::Verification(_)
        ));
        assert_eq!(typed.events()[3].event_type().as_str(), "MatchReopened");
        assert_eq!(
            typed.events()[3].verification_status(),
            EventVerificationStatus::Provisional
        );
    }

    #[test]
    fn rejects_invalid_stored_period_and_verification_status() {
        assert!(matches!(
            parse_period(event_id(), "full_time".to_owned()),
            Err(MatchEventRepositoryError::InvalidStoredPeriod { .. })
        ));
        assert!(matches!(
            parse_verification_status(event_id(), "operator".to_owned()),
            Err(MatchEventRepositoryError::InvalidStoredVerificationStatus { .. })
        ));
    }

    #[test]
    fn rejects_negative_persisted_numeric_values() {
        let mut row = valid_row();
        row.aggregate_version = 0;
        assert!(matches!(
            football_domain::DomainEventEnvelope::<serde_json::Value>::try_from(row),
            Err(MatchEventRepositoryError::InvalidStoredAggregateVersion { value: 0, .. })
        ));

        let mut row = valid_row();
        row.clock_ms = -1;
        assert!(matches!(
            football_domain::DomainEventEnvelope::<serde_json::Value>::try_from(row),
            Err(MatchEventRepositoryError::InvalidStoredClock { value: -1, .. })
        ));
    }

    #[test]
    fn preserves_causation_command_and_event_index_when_encoding_for_insert() {
        let event: football_domain::DomainEventEnvelope<serde_json::Value> =
            valid_row().try_into().unwrap();
        let values = PersistedEventValues::try_from(&event).unwrap();

        assert_eq!(
            values.causation_command_id,
            uuid("019fe2c0-3bc5-7cf2-b3d2-0728c077a930")
        );
        assert_eq!(values.event_index, 0);
        assert_eq!(values.aggregate_version, 1);
        assert_eq!(values.period, "first_half");
        assert_eq!(values.verification_status, "provisional");
        assert_eq!(values.payload.0, json!({ "side": "home" }));
    }

    #[test]
    fn rejects_invalid_versioned_batches_before_the_insert_adapter_runs() {
        let first: football_domain::DomainEventEnvelope<serde_json::Value> =
            valid_row().try_into().unwrap();
        let mut second_row = valid_row();
        second_row.event_id = uuid("019fe2c0-3bc5-7cf4-b3d2-0728c077a930");
        second_row.aggregate_version = 3;
        second_row.event_index = 1;
        let second: football_domain::DomainEventEnvelope<serde_json::Value> =
            second_row.try_into().unwrap();

        assert!(matches!(
            validate_persisted_event_batch(AggregateVersion::initial(), &[first, second]),
            Err(MatchEventRepositoryError::InvalidEventStream(_))
        ));
    }

    #[test]
    fn requires_the_supplied_current_version_before_a_non_initial_batch() {
        let mut row = valid_row();
        row.aggregate_version = 3;
        let event: football_domain::DomainEventEnvelope<serde_json::Value> =
            row.try_into().unwrap();

        assert!(matches!(
            validate_persisted_event_batch(
                AggregateVersion::initial(),
                std::slice::from_ref(&event),
            ),
            Err(MatchEventRepositoryError::InvalidEventStream(_))
        ));
        assert!(validate_persisted_event_batch(AggregateVersion::from_value(2), &[event]).is_ok());
    }

    #[test]
    fn rejects_values_that_cannot_fit_postgresql_bigint() {
        let mut event: football_domain::DomainEventEnvelope<serde_json::Value> =
            valid_row().try_into().unwrap();
        event = football_domain::DomainEventEnvelope::new(
            event.event_id(),
            event.match_id(),
            football_domain::AggregateVersion::from_value(i64::MAX as u64 + 1),
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
            event.supersedes_event_id(),
            event.voids_event_id(),
        );

        assert!(matches!(
            PersistedEventValues::try_from(&event),
            Err(MatchEventRepositoryError::CannotPersistAggregateVersion { .. })
        ));
    }
}
