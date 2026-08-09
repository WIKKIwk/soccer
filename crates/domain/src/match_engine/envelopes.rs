use super::{AggregateVersion, ClockMilliseconds, MatchId, MatchPeriod};
use crate::{ActorIdentity, UtcInstant};
use std::fmt;
use uuid::Uuid;

/// Internal identity of a logical command that caused domain work.
///
/// The caller-facing client command ID and request fingerprint are defined by
/// the later command-admission increment; this ID is the persisted causation
/// reference used by a domain event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(Uuid);

impl CommandId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for CommandId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<CommandId> for Uuid {
    fn from(value: CommandId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Identity of one immutable canonical domain event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(Uuid);

impl EventId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for EventId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<EventId> for Uuid {
    fn from(value: EventId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Zero-based order of events emitted by one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventIndex(u32);

impl EventIndex {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn from_value(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    pub fn try_next(self) -> Result<Self, EventIndexError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(EventIndexError::Overflow { current: self.0 })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventIndexError {
    Overflow { current: u32 },
}

impl fmt::Display for EventIndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overflow { current } => {
                write!(formatter, "event index overflow after {current}")
            }
        }
    }
}

impl std::error::Error for EventIndexError {}

/// Positive schema revision carried by every domain event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventSchemaVersion(u16);

impl EventSchemaVersion {
    pub const fn initial() -> Self {
        Self(1)
    }

    pub fn try_new(value: u16) -> Result<Self, EventSchemaVersionError> {
        if value == 0 {
            return Err(EventSchemaVersionError::Zero);
        }

        Ok(Self(value))
    }

    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSchemaVersionError {
    Zero,
}

impl fmt::Display for EventSchemaVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zero => formatter.write_str("event schema version must be positive"),
        }
    }
}

impl std::error::Error for EventSchemaVersionError {}

/// Stable domain event type identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventType(String);

impl EventType {
    pub fn try_new(value: impl Into<String>) -> Result<Self, EventTypeError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(EventTypeError::Blank);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTypeError {
    Blank,
}

impl fmt::Display for EventTypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => formatter.write_str("event type must not be blank"),
        }
    }
}

impl std::error::Error for EventTypeError {}

/// Verification state attached when an immutable event is recorded.
///
/// This is event-level metadata and is intentionally distinct from the
/// match-level `MatchVerificationStatus`. It does not by itself verify the
/// complete match result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventVerificationStatus {
    Provisional,
    Verified,
}

impl EventVerificationStatus {
    /// Returns the stable lowercase vocabulary persisted with canonical events.
    pub const fn as_persisted_str(self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::Verified => "verified",
        }
    }

    /// Parses only the canonical-event verification vocabulary.
    pub fn try_from_persisted_str(value: &str) -> Option<Self> {
        match value {
            "provisional" => Some(Self::Provisional),
            "verified" => Some(Self::Verified),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandResultStatus {
    Accepted,
    Rejected,
}

/// Terminal result of one logical command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome<R, E> {
    Accepted {
        final_aggregate_version: AggregateVersion,
        event_count: u64,
        result: R,
    },
    Rejected {
        aggregate_version: AggregateVersion,
        error: E,
    },
}

impl<R, E> CommandOutcome<R, E> {
    pub const fn status(&self) -> CommandResultStatus {
        match self {
            Self::Accepted { .. } => CommandResultStatus::Accepted,
            Self::Rejected { .. } => CommandResultStatus::Rejected,
        }
    }

    pub const fn aggregate_version(&self) -> AggregateVersion {
        match self {
            Self::Accepted {
                final_aggregate_version,
                ..
            } => *final_aggregate_version,
            Self::Rejected {
                aggregate_version, ..
            } => *aggregate_version,
        }
    }
}

/// Transport-independent command result envelope for one match aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResultEnvelope<R, E> {
    command_id: CommandId,
    match_id: MatchId,
    outcome: CommandOutcome<R, E>,
}

impl<R, E> CommandResultEnvelope<R, E> {
    pub const fn accepted(
        command_id: CommandId,
        match_id: MatchId,
        final_aggregate_version: AggregateVersion,
        event_count: u64,
        result: R,
    ) -> Self {
        Self {
            command_id,
            match_id,
            outcome: CommandOutcome::Accepted {
                final_aggregate_version,
                event_count,
                result,
            },
        }
    }

    pub const fn rejected(
        command_id: CommandId,
        match_id: MatchId,
        aggregate_version: AggregateVersion,
        error: E,
    ) -> Self {
        Self {
            command_id,
            match_id,
            outcome: CommandOutcome::Rejected {
                aggregate_version,
                error,
            },
        }
    }

    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }

    pub const fn status(&self) -> CommandResultStatus {
        self.outcome.status()
    }

    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.outcome.aggregate_version()
    }

    pub const fn outcome(&self) -> &CommandOutcome<R, E> {
        &self.outcome
    }

    /// Consumes the envelope and returns its typed terminal outcome without
    /// changing or serializing the business payload.
    pub fn into_outcome(self) -> CommandOutcome<R, E> {
        self.outcome
    }
}

/// Immutable canonical event envelope with a typed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainEventEnvelope<T> {
    event_id: EventId,
    match_id: MatchId,
    aggregate_version: AggregateVersion,
    causation_command_id: CommandId,
    event_index: EventIndex,
    event_type: EventType,
    event_schema_version: EventSchemaVersion,
    period: MatchPeriod,
    clock_ms: ClockMilliseconds,
    occurred_at: UtcInstant,
    recorded_at: UtcInstant,
    actor: ActorIdentity,
    verification_status: EventVerificationStatus,
    payload: T,
    supersedes_event_id: Option<EventId>,
    voids_event_id: Option<EventId>,
}

impl<T> DomainEventEnvelope<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_id: EventId,
        match_id: MatchId,
        aggregate_version: AggregateVersion,
        causation_command_id: CommandId,
        event_index: EventIndex,
        event_type: EventType,
        event_schema_version: EventSchemaVersion,
        period: MatchPeriod,
        clock_ms: ClockMilliseconds,
        occurred_at: UtcInstant,
        recorded_at: UtcInstant,
        actor: ActorIdentity,
        verification_status: EventVerificationStatus,
        payload: T,
        supersedes_event_id: Option<EventId>,
        voids_event_id: Option<EventId>,
    ) -> Self {
        Self {
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
            actor,
            verification_status,
            payload,
            supersedes_event_id,
            voids_event_id,
        }
    }

    pub const fn event_id(&self) -> EventId {
        self.event_id
    }

    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }

    pub const fn aggregate_version(&self) -> AggregateVersion {
        self.aggregate_version
    }

    pub const fn causation_command_id(&self) -> CommandId {
        self.causation_command_id
    }

    pub const fn event_index(&self) -> EventIndex {
        self.event_index
    }

    pub fn event_type(&self) -> &EventType {
        &self.event_type
    }

    pub const fn event_schema_version(&self) -> EventSchemaVersion {
        self.event_schema_version
    }

    pub const fn period(&self) -> MatchPeriod {
        self.period
    }

    pub const fn clock_ms(&self) -> ClockMilliseconds {
        self.clock_ms
    }

    pub const fn occurred_at(&self) -> UtcInstant {
        self.occurred_at
    }

    pub const fn recorded_at(&self) -> UtcInstant {
        self.recorded_at
    }

    pub const fn actor(&self) -> ActorIdentity {
        self.actor
    }

    pub const fn verification_status(&self) -> EventVerificationStatus {
        self.verification_status
    }

    pub fn payload(&self) -> &T {
        &self.payload
    }

    /// Rebuilds the envelope with a transformed payload while preserving all
    /// canonical identity and ordering metadata.
    pub fn map_payload<U>(self, payload: U) -> DomainEventEnvelope<U> {
        DomainEventEnvelope::new(
            self.event_id,
            self.match_id,
            self.aggregate_version,
            self.causation_command_id,
            self.event_index,
            self.event_type,
            self.event_schema_version,
            self.period,
            self.clock_ms,
            self.occurred_at,
            self.recorded_at,
            self.actor,
            self.verification_status,
            payload,
            self.supersedes_event_id,
            self.voids_event_id,
        )
    }

    pub const fn supersedes_event_id(&self) -> Option<EventId> {
        self.supersedes_event_id
    }

    pub const fn voids_event_id(&self) -> Option<EventId> {
        self.voids_event_id
    }

    pub(crate) fn with_sequence(
        mut self,
        aggregate_version: AggregateVersion,
        event_index: EventIndex,
    ) -> Self {
        self.aggregate_version = aggregate_version;
        self.event_index = event_index;
        self
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::{ActorIdentity, AdminUserId, ClockMilliseconds, MatchId, MatchPeriod, UtcInstant};

    use super::{
        AggregateVersion, CommandId, CommandResultEnvelope, CommandResultStatus,
        DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion, EventType, EventTypeError,
        EventVerificationStatus,
    };

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("envelope UUID should be valid")
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe2b0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn command_id() -> CommandId {
        CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf1-b3d2-0728c077a930"))
    }

    #[test]
    fn accepted_command_result_exposes_terminal_version_and_event_count() {
        let result = CommandResultEnvelope::<_, &str>::accepted(
            command_id(),
            match_id(),
            AggregateVersion::initial().try_after_events(2).unwrap(),
            2,
            "accepted-value",
        );

        assert_eq!(result.command_id(), command_id());
        assert_eq!(result.match_id(), match_id());
        assert_eq!(result.status(), CommandResultStatus::Accepted);
        assert_eq!(result.aggregate_version().value(), 2);
        assert!(matches!(
            result.outcome(),
            super::CommandOutcome::Accepted {
                event_count: 2,
                result: "accepted-value",
                ..
            }
        ));
    }

    #[test]
    fn rejected_command_result_keeps_observed_version_and_error() {
        let result = CommandResultEnvelope::<(), _>::rejected(
            command_id(),
            match_id(),
            AggregateVersion::initial(),
            "domain.invalid_transition",
        );

        assert_eq!(result.status(), CommandResultStatus::Rejected);
        assert_eq!(result.aggregate_version(), AggregateVersion::initial());
        assert!(matches!(
            result.outcome(),
            super::CommandOutcome::Rejected {
                error: "domain.invalid_transition",
                ..
            }
        ));
    }

    #[test]
    fn command_result_can_move_its_typed_outcome_to_an_application_encoder() {
        let result = CommandResultEnvelope::<_, ()>::accepted(
            command_id(),
            match_id(),
            AggregateVersion::initial().try_next().unwrap(),
            1,
            vec!["typed-result"],
        );

        assert!(matches!(
            result.into_outcome(),
            super::CommandOutcome::Accepted {
                final_aggregate_version,
                event_count: 1,
                result,
            } if final_aggregate_version == AggregateVersion::from_value(1)
                && result == vec!["typed-result"]
        ));
    }

    #[test]
    fn event_envelope_preserves_causation_order_timing_actor_and_payload() {
        let event_id = EventId::from_uuid(uuid("019fe2b0-3bc5-7cf2-b3d2-0728c077a930"));
        let voided_id = EventId::from_uuid(uuid("019fe2b0-3bc5-7cf3-b3d2-0728c077a930"));
        let occurred_at = UtcInstant::from_unix_timestamp(1_777_000_000).unwrap();
        let recorded_at = UtcInstant::from_unix_timestamp(1_777_000_005).unwrap();
        let actor = ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe2b0-3bc5-7cf4-b3d2-0728c077a930",
        )));
        let event = DomainEventEnvelope::new(
            event_id,
            match_id(),
            AggregateVersion::initial().try_next().unwrap(),
            command_id(),
            EventIndex::initial(),
            EventType::try_new("GoalRecorded").unwrap(),
            EventSchemaVersion::initial(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(12).unwrap(),
            occurred_at,
            recorded_at,
            actor,
            EventVerificationStatus::Provisional,
            "typed-payload",
            None,
            Some(voided_id),
        );

        assert_eq!(event.event_id(), event_id);
        assert_eq!(event.match_id(), match_id());
        assert_eq!(event.aggregate_version().value(), 1);
        assert_eq!(event.causation_command_id(), command_id());
        assert_eq!(event.event_index().value(), 0);
        assert_eq!(event.event_type().as_str(), "GoalRecorded");
        assert_eq!(event.event_schema_version().value(), 1);
        assert_eq!(event.period(), MatchPeriod::FirstHalf);
        assert_eq!(event.clock_ms().value(), 12_000);
        assert_eq!(event.occurred_at(), occurred_at);
        assert_eq!(event.recorded_at(), recorded_at);
        assert_eq!(event.actor(), actor);
        assert_eq!(
            event.verification_status(),
            EventVerificationStatus::Provisional
        );
        assert_eq!(event.payload(), &"typed-payload");
        assert_eq!(event.supersedes_event_id(), None);
        assert_eq!(event.voids_event_id(), Some(voided_id));
    }

    #[test]
    fn envelope_value_objects_reject_invalid_schema_and_event_type() {
        assert_eq!(
            EventSchemaVersion::try_new(0).unwrap_err().to_string(),
            "event schema version must be positive"
        );
        assert_eq!(EventType::try_new("  ").unwrap_err(), EventTypeError::Blank);
        assert_eq!(EventIndex::initial().try_next().unwrap().value(), 1);
    }

    #[test]
    fn event_verification_status_persisted_vocabulary_round_trips_without_guessing() {
        let statuses = [
            (EventVerificationStatus::Provisional, "provisional"),
            (EventVerificationStatus::Verified, "verified"),
        ];

        for (status, persisted) in statuses {
            assert_eq!(status.as_persisted_str(), persisted);
            assert_eq!(
                EventVerificationStatus::try_from_persisted_str(persisted),
                Some(status)
            );
        }
        assert_eq!(
            EventVerificationStatus::try_from_persisted_str("operator"),
            None
        );
    }
}
