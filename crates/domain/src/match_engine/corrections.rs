use super::{
    AggregateVersion, CanonicalEventStream, ClockMilliseconds, DomainEventEnvelope,
    EventAppendError, EventId, EventIndex, EventSchemaVersion, EventType, EventTypeError,
    EventVerificationStatus, MatchId, MatchPeriod,
};
use crate::{ActorIdentity, CommandId, UtcInstant};
use std::fmt;

/// Human-supplied explanation required for an event correction, void, or
/// match-verification reopen decision.
///
/// The reason is preserved exactly as supplied after validating that it is not
/// only whitespace. Length and transport-specific body limits belong to the
/// application boundary; the domain must not truncate correction evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorrectionReason(String);

impl CorrectionReason {
    pub fn try_new(value: impl Into<String>) -> Result<Self, CorrectionReasonError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CorrectionReasonError::Empty);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CorrectionReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionReasonError {
    Empty,
}

impl fmt::Display for CorrectionReasonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("correction reason must not be empty"),
        }
    }
}

impl std::error::Error for CorrectionReasonError {}

/// Canonical event names owned by the correction boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CorrectionEventType {
    EventVoided,
}

impl CorrectionEventType {
    pub const ALL: [Self; 1] = [Self::EventVoided];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EventVoided => "EventVoided",
        }
    }

    pub fn try_as_event_type(self) -> Result<EventType, EventTypeError> {
        EventType::try_new(self.as_str())
    }

    pub const fn schema_version(self) -> EventSchemaVersion {
        EventSchemaVersion::initial()
    }
}

impl fmt::Display for CorrectionEventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Payload for an immutable `EventVoided` canonical event.
///
/// The affected event ID is carried by the envelope's `voids_event_id` field,
/// so the reason is not duplicated inside the payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventVoidedPayload {
    reason: CorrectionReason,
}

impl EventVoidedPayload {
    pub const fn new(reason: CorrectionReason) -> Self {
        Self { reason }
    }

    pub fn reason(&self) -> &CorrectionReason {
        &self.reason
    }
}

/// Typed payload union for correction events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CorrectionEventPayload {
    EventVoided(EventVoidedPayload),
}

impl CorrectionEventPayload {
    pub const fn event_type(&self) -> CorrectionEventType {
        match self {
            Self::EventVoided(_) => CorrectionEventType::EventVoided,
        }
    }

    pub const fn schema_version(&self) -> EventSchemaVersion {
        self.event_type().schema_version()
    }
}

/// Domain input for appending one immutable void event.
///
/// This command only builds the unsequenced canonical envelope. Aggregate
/// version checks, sequencing, persistence, authorization, and the effective
/// stream reduction remain owned by their respective boundaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoidEventCommand {
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    expected_version: AggregateVersion,
    period: MatchPeriod,
    clock_ms: ClockMilliseconds,
    occurred_at: UtcInstant,
    recorded_at: UtcInstant,
    actor: ActorIdentity,
    verification_status: EventVerificationStatus,
    voids_event_id: EventId,
    reason: CorrectionReason,
}

impl VoidEventCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        command_id: CommandId,
        event_id: EventId,
        match_id: MatchId,
        expected_version: AggregateVersion,
        period: MatchPeriod,
        clock_ms: ClockMilliseconds,
        occurred_at: UtcInstant,
        recorded_at: UtcInstant,
        actor: ActorIdentity,
        verification_status: EventVerificationStatus,
        voids_event_id: EventId,
        reason: CorrectionReason,
    ) -> Result<Self, VoidEventCommandError> {
        if event_id == voids_event_id {
            return Err(VoidEventCommandError::SelfReferentialVoid { event_id });
        }

        Ok(Self {
            command_id,
            event_id,
            match_id,
            expected_version,
            period,
            clock_ms,
            occurred_at,
            recorded_at,
            actor,
            verification_status,
            voids_event_id,
            reason,
        })
    }

    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub const fn event_id(&self) -> EventId {
        self.event_id
    }

    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }

    pub const fn expected_version(&self) -> AggregateVersion {
        self.expected_version
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

    pub const fn voids_event_id(&self) -> EventId {
        self.voids_event_id
    }

    pub fn reason(&self) -> &CorrectionReason {
        &self.reason
    }

    /// Builds the correction envelope before aggregate version and event index
    /// are assigned by the canonical stream operation.
    pub fn into_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<CorrectionEventPayload>, VoidEventCommandError> {
        let event_type = CorrectionEventType::EventVoided
            .try_as_event_type()
            .map_err(VoidEventCommandError::InvalidEventType)?;

        Ok(DomainEventEnvelope::new(
            self.event_id,
            self.match_id,
            AggregateVersion::initial(),
            self.command_id,
            EventIndex::initial(),
            event_type,
            CorrectionEventType::EventVoided.schema_version(),
            self.period,
            self.clock_ms,
            self.occurred_at,
            self.recorded_at,
            self.actor,
            self.verification_status,
            CorrectionEventPayload::EventVoided(EventVoidedPayload::new(self.reason)),
            None,
            Some(self.voids_event_id),
        ))
    }

    /// Builds the same event in the union payload used by a canonical match
    /// stream that also contains live C1 actions.
    pub fn into_canonical_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<super::MatchEventPayload>, VoidEventCommandError> {
        self.into_unsequenced_event().map(|event| {
            let payload = event.payload().clone();
            event.map_payload(super::MatchEventPayload::Correction(payload))
        })
    }

    /// Appends this correction to a loaded canonical match stream after the
    /// explicit optimistic-version check. The target must already exist in
    /// the source stream; the effective-stream builder remains responsible
    /// for deriving the correction-resolved view.
    pub fn append_to_stream(
        self,
        stream: CanonicalEventStream<super::MatchEventPayload>,
    ) -> Result<CanonicalEventStream<super::MatchEventPayload>, VoidEventCommandError> {
        let expected_version = self.expected_version;
        let command_id = self.command_id;
        let target_event_id = self.voids_event_id;
        let actual_version = stream.current_version();
        if expected_version != actual_version {
            return Err(VoidEventCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict {
                    expected: expected_version,
                    actual: actual_version,
                },
            ));
        }
        if !stream
            .events()
            .iter()
            .any(|event| event.event_id() == target_event_id)
        {
            return Err(VoidEventCommandError::TargetEventNotFound {
                event_id: target_event_id,
            });
        }

        let event = self.into_canonical_unsequenced_event()?;
        stream
            .append_after_expected_version(expected_version, command_id, vec![event])
            .map_err(VoidEventCommandError::EventAppend)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoidEventCommandError {
    SelfReferentialVoid { event_id: EventId },
    TargetEventNotFound { event_id: EventId },
    InvalidEventType(EventTypeError),
    EventAppend(EventAppendError),
}

impl fmt::Display for VoidEventCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SelfReferentialVoid { event_id } => {
                write!(formatter, "void event {event_id} cannot void itself")
            }
            Self::TargetEventNotFound { event_id } => {
                write!(
                    formatter,
                    "void target event {event_id} was not found in the stream"
                )
            }
            Self::InvalidEventType(error) => error.fmt(formatter),
            Self::EventAppend(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for VoidEventCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEventType(error) => Some(error),
            Self::EventAppend(error) => Some(error),
            Self::SelfReferentialVoid { .. } | Self::TargetEventNotFound { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CorrectionEventPayload, CorrectionEventType, CorrectionReason, CorrectionReasonError,
        EventVoidedPayload, VoidEventCommand, VoidEventCommandError,
    };
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, C1EventType,
        CanonicalEventStream, ClockMilliseconds, CommandId, DomainEventEnvelope, EventId,
        EventIndex, EventVerificationStatus, GoalAwardType, GoalRecordedPayload, MatchEventPayload,
        MatchId, MatchPeriod, TeamId, UtcInstant,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("correction fixture UUID should be valid")
    }

    fn command_id() -> CommandId {
        CommandId::from_uuid(uuid("019fe2c0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn event_id(value: &str) -> EventId {
        EventId::from_uuid(uuid(value))
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe2c0-3bc5-7cf2-b3d2-0728c077a930"))
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe2c0-3bc5-7cf3-b3d2-0728c077a930",
        )))
    }

    fn reason() -> CorrectionReason {
        CorrectionReason::try_new("official protocol corrected the scorer")
            .expect("fixture reason should be non-empty")
    }

    fn command_with_version(
        expected_version: AggregateVersion,
        emitted_event_id: EventId,
        voids_event_id: EventId,
    ) -> VoidEventCommand {
        VoidEventCommand::try_new(
            command_id(),
            emitted_event_id,
            match_id(),
            expected_version,
            MatchPeriod::HalfTime,
            ClockMilliseconds::from_seconds(45).expect("fixture clock should be valid"),
            UtcInstant::from_unix_timestamp(1_777_000_000).expect("timestamp should be valid"),
            UtcInstant::from_unix_timestamp(1_777_000_001).expect("timestamp should be valid"),
            actor(),
            EventVerificationStatus::Verified,
            voids_event_id,
            reason(),
        )
        .expect("void command fixture should be valid")
    }

    fn command(voids_event_id: EventId) -> VoidEventCommand {
        command_with_version(
            AggregateVersion::from_value(4),
            event_id("019fe2c0-3bc5-7cf4-b3d2-0728c077a930"),
            voids_event_id,
        )
    }

    fn source_stream() -> CanonicalEventStream<MatchEventPayload> {
        let payload = C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
            TeamId::from_uuid(uuid("019fe2c0-3bc5-7cf6-b3d2-0728c077a930")),
            None,
            GoalAwardType::Regular,
        ));
        let event_id = event_id("019fe2c0-3bc5-7cf5-b3d2-0728c077a930");
        let event = DomainEventEnvelope::new(
            event_id,
            match_id(),
            AggregateVersion::from_value(1),
            CommandId::from_uuid(uuid("019fe2c0-3bc5-7cf6-b3d2-0728c077a930")),
            EventIndex::initial(),
            C1EventType::GoalRecorded.try_as_event_type().unwrap(),
            C1EventType::GoalRecorded.schema_version(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(12).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            MatchEventPayload::C1(payload),
            None,
            None,
        );
        CanonicalEventStream::try_from_ordered(match_id(), vec![event]).unwrap()
    }

    #[test]
    fn correction_reason_rejects_blank_text_and_preserves_nonblank_evidence() {
        assert_eq!(
            CorrectionReason::try_new(" \t\n"),
            Err(CorrectionReasonError::Empty)
        );

        let reason = CorrectionReason::try_new(" official protocol differs ")
            .expect("nonblank reason should be accepted");
        assert_eq!(reason.as_str(), " official protocol differs ");
        assert_eq!(reason.to_string(), " official protocol differs ");
    }

    #[test]
    fn correction_event_type_and_payload_have_stable_versioned_meaning() {
        assert_eq!(CorrectionEventType::ALL, [CorrectionEventType::EventVoided]);
        assert_eq!(CorrectionEventType::EventVoided.as_str(), "EventVoided");
        assert_eq!(
            CorrectionEventType::EventVoided
                .try_as_event_type()
                .unwrap()
                .as_str(),
            "EventVoided"
        );

        let payload = CorrectionEventPayload::EventVoided(EventVoidedPayload::new(reason()));
        assert_eq!(payload.event_type(), CorrectionEventType::EventVoided);
        assert_eq!(
            payload.schema_version(),
            CorrectionEventType::EventVoided.schema_version()
        );
        let CorrectionEventPayload::EventVoided(payload) = payload;
        assert_eq!(
            payload.reason().as_str(),
            "official protocol corrected the scorer"
        );
    }

    #[test]
    fn void_command_rejects_voiding_the_event_it_is_emitting() {
        let emitted_event_id = event_id("019fe2c0-3bc5-7cf4-b3d2-0728c077a930");

        assert_eq!(
            VoidEventCommand::try_new(
                command_id(),
                emitted_event_id,
                match_id(),
                AggregateVersion::from_value(4),
                MatchPeriod::FirstHalf,
                ClockMilliseconds::from_seconds(44).expect("fixture clock should be valid"),
                UtcInstant::from_unix_timestamp(1_777_000_000).expect("timestamp should be valid"),
                UtcInstant::from_unix_timestamp(1_777_000_001).expect("timestamp should be valid"),
                actor(),
                EventVerificationStatus::Verified,
                emitted_event_id,
                reason(),
            )
            .unwrap_err(),
            VoidEventCommandError::SelfReferentialVoid {
                event_id: emitted_event_id,
            }
        );
    }

    #[test]
    fn void_command_builds_an_immutable_reference_with_reason_and_metadata() {
        let voided_event_id = event_id("019fe2c0-3bc5-7cf5-b3d2-0728c077a930");
        let command = command(voided_event_id);
        let expected_event_id = command.event_id();
        let event = command
            .into_unsequenced_event()
            .expect("void event type should be valid");

        assert_eq!(event.event_id(), expected_event_id);
        assert_eq!(event.match_id(), match_id());
        assert_eq!(event.aggregate_version(), AggregateVersion::initial());
        assert_eq!(event.causation_command_id(), command_id());
        assert_eq!(event.event_index(), crate::EventIndex::initial());
        assert_eq!(event.event_type().as_str(), "EventVoided");
        assert_eq!(
            event.event_schema_version(),
            CorrectionEventType::EventVoided.schema_version()
        );
        assert_eq!(event.period(), MatchPeriod::HalfTime);
        assert_eq!(
            event.clock_ms(),
            ClockMilliseconds::from_seconds(45).unwrap()
        );
        assert_eq!(event.actor(), actor());
        assert_eq!(
            event.verification_status(),
            EventVerificationStatus::Verified
        );
        assert_eq!(event.supersedes_event_id(), None);
        assert_eq!(event.voids_event_id(), Some(voided_event_id));

        let CorrectionEventPayload::EventVoided(payload) = event.payload();
        assert_eq!(payload.reason().as_str(), reason().as_str());
    }

    #[test]
    fn void_command_appends_to_a_canonical_union_stream_and_preserves_target_reference() {
        let stream = source_stream();
        let target_event_id = stream.events()[0].event_id();
        let command = command_with_version(
            AggregateVersion::from_value(1),
            event_id("019fe2c0-3bc5-7cf7-b3d2-0728c077a930"),
            target_event_id,
        );

        let appended = command.append_to_stream(stream).unwrap();

        assert_eq!(appended.len(), 2);
        assert_eq!(appended.current_version(), AggregateVersion::from_value(2));
        let correction = &appended.events()[1];
        assert_eq!(
            correction.aggregate_version(),
            AggregateVersion::from_value(2)
        );
        assert_eq!(correction.event_index(), EventIndex::initial());
        assert_eq!(correction.voids_event_id(), Some(target_event_id));
        assert!(matches!(
            correction.payload(),
            MatchEventPayload::Correction(CorrectionEventPayload::EventVoided(_))
        ));
    }

    #[test]
    fn void_command_rejects_stale_version_before_target_lookup() {
        let stream = source_stream();
        let command = command_with_version(
            AggregateVersion::initial(),
            event_id("019fe2c0-3bc5-7cf7-b3d2-0728c077a930"),
            event_id("019fe2c0-3bc5-7cf8-b3d2-0728c077a930"),
        );

        assert!(matches!(
            command.append_to_stream(stream),
            Err(VoidEventCommandError::EventAppend(
                crate::EventAppendError::ExpectedVersionConflict { expected, actual }
            )) if expected == AggregateVersion::initial()
                && actual == AggregateVersion::from_value(1)
        ));
    }

    #[test]
    fn void_command_rejects_a_target_missing_from_the_loaded_stream() {
        let stream = source_stream();
        let missing_event_id = event_id("019fe2c0-3bc5-7cf8-b3d2-0728c077a930");
        let command = command_with_version(
            AggregateVersion::from_value(1),
            event_id("019fe2c0-3bc5-7cf7-b3d2-0728c077a930"),
            missing_event_id,
        );

        assert_eq!(
            command.append_to_stream(stream).unwrap_err(),
            VoidEventCommandError::TargetEventNotFound {
                event_id: missing_event_id
            }
        );
    }
}
