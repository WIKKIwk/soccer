use super::{
    AggregateVersion, CanonicalEventStream, ClockMilliseconds, CommandId, CorrectionReason,
    DomainEventEnvelope, EventAppendError, EventId, EventIndex, EventSchemaVersion, EventType,
    EventTypeError, EventVerificationStatus, MatchEventPayload, MatchId, MatchPeriod,
};
use crate::{ActorIdentity, UtcInstant};
use std::fmt;

/// Canonical event names owned by the match-verification boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationEventType {
    MatchVerified,
    MatchReopened,
}

impl VerificationEventType {
    pub const ALL: [Self; 2] = [Self::MatchVerified, Self::MatchReopened];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchVerified => "MatchVerified",
            Self::MatchReopened => "MatchReopened",
        }
    }

    pub fn try_as_event_type(self) -> Result<EventType, EventTypeError> {
        EventType::try_new(self.as_str())
    }

    pub const fn schema_version(self) -> EventSchemaVersion {
        EventSchemaVersion::initial()
    }
}

impl fmt::Display for VerificationEventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Empty payload for `MatchVerified`.
///
/// The supervisor identity and event-level verified status live in the
/// canonical envelope. Final-authority evidence fields are intentionally
/// deferred to the source/evidence increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchVerifiedPayload;

impl MatchVerifiedPayload {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for MatchVerifiedPayload {
    fn default() -> Self {
        Self::new()
    }
}

/// Payload for an explicitly reasoned `MatchReopened` decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchReopenedPayload {
    reason: CorrectionReason,
}

impl MatchReopenedPayload {
    pub const fn new(reason: CorrectionReason) -> Self {
        Self { reason }
    }

    pub fn reason(&self) -> &CorrectionReason {
        &self.reason
    }
}

/// Typed payload union for verification events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VerificationEventPayload {
    MatchVerified(MatchVerifiedPayload),
    MatchReopened(MatchReopenedPayload),
}

impl VerificationEventPayload {
    pub const fn event_type(&self) -> VerificationEventType {
        match self {
            Self::MatchVerified(_) => VerificationEventType::MatchVerified,
            Self::MatchReopened(_) => VerificationEventType::MatchReopened,
        }
    }

    pub const fn schema_version(&self) -> EventSchemaVersion {
        self.event_type().schema_version()
    }
}

/// Domain command for appending one supervisor verification event.
///
/// Authorization is enforced by the application policy before this command is
/// admitted. This command owns only canonical metadata, the expected-version
/// check, and the no-repeat verification invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyMatchCommand {
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    expected_version: AggregateVersion,
    period: MatchPeriod,
    clock_ms: ClockMilliseconds,
    occurred_at: UtcInstant,
    recorded_at: UtcInstant,
    actor: ActorIdentity,
}

impl VerifyMatchCommand {
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
    ) -> Result<Self, VerifyMatchCommandError> {
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

    pub const fn command_name(&self) -> &'static str {
        "VerifyMatch"
    }

    pub const fn event_type(&self) -> VerificationEventType {
        VerificationEventType::MatchVerified
    }

    pub const fn verification_status(&self) -> EventVerificationStatus {
        EventVerificationStatus::Verified
    }

    /// Builds the unsequenced canonical event. The stream assigns its
    /// aggregate version and command-local event index.
    pub fn into_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<VerificationEventPayload>, VerifyMatchCommandError> {
        let event_type = self
            .event_type()
            .try_as_event_type()
            .map_err(VerifyMatchCommandError::InvalidEventType)?;

        Ok(DomainEventEnvelope::new(
            self.event_id,
            self.match_id,
            AggregateVersion::initial(),
            self.command_id,
            EventIndex::initial(),
            event_type,
            self.event_type().schema_version(),
            self.period,
            self.clock_ms,
            self.occurred_at,
            self.recorded_at,
            self.actor,
            self.verification_status(),
            VerificationEventPayload::MatchVerified(MatchVerifiedPayload::new()),
            None,
            None,
        ))
    }

    /// Builds the event in the canonical stream union used by C1, correction,
    /// and verification persistence.
    pub fn into_canonical_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<MatchEventPayload>, VerifyMatchCommandError> {
        self.into_unsequenced_event().map(|event| {
            let payload = event.payload().clone();
            event.map_payload(MatchEventPayload::Verification(payload))
        })
    }

    /// Appends the verification event after checking the expected aggregate
    /// version and rejecting a second verification without a reopen event.
    pub fn append_to_stream(
        self,
        stream: CanonicalEventStream<MatchEventPayload>,
    ) -> Result<CanonicalEventStream<MatchEventPayload>, VerifyMatchCommandError> {
        let expected_version = self.expected_version;
        let command_id = self.command_id;
        let actual_version = stream.current_version();
        if expected_version != actual_version {
            return Err(VerifyMatchCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict {
                    expected: expected_version,
                    actual: actual_version,
                },
            ));
        }

        if let Some(event) = last_verification_event(&stream).filter(|event| {
            event.event_type().as_str() == VerificationEventType::MatchVerified.as_str()
        }) {
            return Err(VerifyMatchCommandError::AlreadyVerified {
                event_id: event.event_id(),
            });
        }

        let event = self.into_canonical_unsequenced_event()?;
        stream
            .append_after_expected_version(expected_version, command_id, vec![event])
            .map_err(VerifyMatchCommandError::EventAppend)
    }
}

/// Domain command for reopening a previously verified match with an explicit
/// reason. The command changes only the verification marker; it does not
/// delete or rewrite any earlier canonical event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReopenMatchCommand {
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    expected_version: AggregateVersion,
    period: MatchPeriod,
    clock_ms: ClockMilliseconds,
    occurred_at: UtcInstant,
    recorded_at: UtcInstant,
    actor: ActorIdentity,
    reason: CorrectionReason,
}

impl ReopenMatchCommand {
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
        reason: CorrectionReason,
    ) -> Result<Self, ReopenMatchCommandError> {
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

    pub const fn command_name(&self) -> &'static str {
        "ReopenMatch"
    }

    pub const fn event_type(&self) -> VerificationEventType {
        VerificationEventType::MatchReopened
    }

    pub const fn verification_status(&self) -> EventVerificationStatus {
        EventVerificationStatus::Provisional
    }

    pub fn reason(&self) -> &CorrectionReason {
        &self.reason
    }

    pub fn into_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<VerificationEventPayload>, ReopenMatchCommandError> {
        let event_type = self
            .event_type()
            .try_as_event_type()
            .map_err(ReopenMatchCommandError::InvalidEventType)?;

        Ok(DomainEventEnvelope::new(
            self.event_id,
            self.match_id,
            AggregateVersion::initial(),
            self.command_id,
            EventIndex::initial(),
            event_type,
            self.event_type().schema_version(),
            self.period,
            self.clock_ms,
            self.occurred_at,
            self.recorded_at,
            self.actor,
            self.verification_status(),
            VerificationEventPayload::MatchReopened(MatchReopenedPayload::new(self.reason)),
            None,
            None,
        ))
    }

    pub fn into_canonical_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<MatchEventPayload>, ReopenMatchCommandError> {
        self.into_unsequenced_event().map(|event| {
            let payload = event.payload().clone();
            event.map_payload(MatchEventPayload::Verification(payload))
        })
    }

    /// Appends a reopen marker only when the latest verification decision is
    /// `MatchVerified`. The canonical stream remains append-only.
    pub fn append_to_stream(
        self,
        stream: CanonicalEventStream<MatchEventPayload>,
    ) -> Result<CanonicalEventStream<MatchEventPayload>, ReopenMatchCommandError> {
        let expected_version = self.expected_version;
        let command_id = self.command_id;
        let actual_version = stream.current_version();
        if expected_version != actual_version {
            return Err(ReopenMatchCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict {
                    expected: expected_version,
                    actual: actual_version,
                },
            ));
        }

        if last_verification_event(&stream)
            .filter(|event| {
                event.event_type().as_str() == VerificationEventType::MatchVerified.as_str()
            })
            .is_none()
        {
            return Err(ReopenMatchCommandError::MatchNotVerified);
        }

        let event = self.into_canonical_unsequenced_event()?;
        stream
            .append_after_expected_version(expected_version, command_id, vec![event])
            .map_err(ReopenMatchCommandError::EventAppend)
    }
}

fn last_verification_event(
    stream: &CanonicalEventStream<MatchEventPayload>,
) -> Option<&DomainEventEnvelope<MatchEventPayload>> {
    stream
        .events()
        .iter()
        .rev()
        .find(|event| matches!(event.payload(), MatchEventPayload::Verification(_)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyMatchCommandError {
    AlreadyVerified { event_id: EventId },
    InvalidEventType(EventTypeError),
    EventAppend(EventAppendError),
}

impl fmt::Display for VerifyMatchCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyVerified { event_id } => {
                write!(formatter, "match is already verified by event {event_id}")
            }
            Self::InvalidEventType(error) => error.fmt(formatter),
            Self::EventAppend(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for VerifyMatchCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEventType(error) => Some(error),
            Self::EventAppend(error) => Some(error),
            Self::AlreadyVerified { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReopenMatchCommandError {
    MatchNotVerified,
    InvalidEventType(EventTypeError),
    EventAppend(EventAppendError),
}

impl fmt::Display for ReopenMatchCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatchNotVerified => formatter.write_str("match is not currently verified"),
            Self::InvalidEventType(error) => error.fmt(formatter),
            Self::EventAppend(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ReopenMatchCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEventType(error) => Some(error),
            Self::EventAppend(error) => Some(error),
            Self::MatchNotVerified => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MatchReopenedPayload, MatchVerifiedPayload, ReopenMatchCommand, ReopenMatchCommandError,
        VerificationEventPayload, VerificationEventType, VerifyMatchCommand,
        VerifyMatchCommandError,
    };
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, CanonicalEventStream, ClockMilliseconds,
        CommandId, DomainEventEnvelope, EventId, EventIndex, EventVerificationStatus,
        MatchEventPayload, MatchId, MatchPeriod, UtcInstant,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("verification fixture UUID should be valid")
    }

    fn command_id(value: &str) -> CommandId {
        CommandId::from_uuid(uuid(value))
    }

    fn event_id(value: &str) -> EventId {
        EventId::from_uuid(uuid(value))
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019ff500-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019ff500-3bc5-7cf1-b3d2-0728c077a930",
        )))
    }

    fn command(expected_version: AggregateVersion) -> VerifyMatchCommand {
        command_with_ids(
            expected_version,
            command_id("019ff500-3bc5-7cf2-b3d2-0728c077a930"),
            event_id("019ff500-3bc5-7cf3-b3d2-0728c077a930"),
        )
    }

    fn command_with_ids(
        expected_version: AggregateVersion,
        command_id: CommandId,
        event_id: EventId,
    ) -> VerifyMatchCommand {
        VerifyMatchCommand::try_new(
            command_id,
            event_id,
            match_id(),
            expected_version,
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(90).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
        )
        .unwrap()
    }

    fn reason() -> super::super::CorrectionReason {
        super::super::CorrectionReason::try_new("official protocol requires re-verification")
            .expect("reopen reason should be non-empty")
    }

    fn reopen_command(
        expected_version: AggregateVersion,
        emitted_event_id: EventId,
    ) -> ReopenMatchCommand {
        ReopenMatchCommand::try_new(
            command_id("019ff500-3bc5-7cf6-b3d2-0728c077a930"),
            emitted_event_id,
            match_id(),
            expected_version,
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(91).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_002).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_003).unwrap(),
            actor(),
            reason(),
        )
        .unwrap()
    }

    fn existing_verified_event() -> DomainEventEnvelope<MatchEventPayload> {
        DomainEventEnvelope::new(
            event_id("019ff500-3bc5-7cf4-b3d2-0728c077a930"),
            match_id(),
            AggregateVersion::from_value(1),
            command_id("019ff500-3bc5-7cf5-b3d2-0728c077a930"),
            EventIndex::initial(),
            VerificationEventType::MatchVerified
                .try_as_event_type()
                .unwrap(),
            VerificationEventType::MatchVerified.schema_version(),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(90).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Verified,
            MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(
                MatchVerifiedPayload::new(),
            )),
            None,
            None,
        )
    }

    #[test]
    fn freezes_match_verified_event_vocabulary_and_verified_metadata() {
        assert_eq!(
            VerificationEventType::ALL,
            [
                VerificationEventType::MatchVerified,
                VerificationEventType::MatchReopened
            ]
        );
        assert_eq!(
            VerificationEventType::MatchVerified.as_str(),
            "MatchVerified"
        );
        assert_eq!(
            VerificationEventType::MatchVerified
                .schema_version()
                .value(),
            1
        );

        let event = command(AggregateVersion::initial())
            .into_unsequenced_event()
            .unwrap();
        assert_eq!(event.event_type().as_str(), "MatchVerified");
        assert_eq!(
            event.verification_status(),
            EventVerificationStatus::Verified
        );
        assert!(matches!(
            event.payload(),
            VerificationEventPayload::MatchVerified(_)
        ));

        let reopened = reopen_command(
            AggregateVersion::from_value(1),
            event_id("019ff500-3bc5-7cf7-b3d2-0728c077a930"),
        )
        .into_unsequenced_event()
        .unwrap();
        assert_eq!(reopened.event_type().as_str(), "MatchReopened");
        assert_eq!(
            reopened.verification_status(),
            EventVerificationStatus::Provisional
        );
        assert!(matches!(
            reopened.payload(),
            VerificationEventPayload::MatchReopened(MatchReopenedPayload { .. })
        ));
    }

    #[test]
    fn appends_one_verification_event_to_an_empty_canonical_stream() {
        let stream = CanonicalEventStream::try_from_ordered(match_id(), Vec::new()).unwrap();

        let appended = command(AggregateVersion::initial())
            .append_to_stream(stream)
            .unwrap();

        assert_eq!(appended.len(), 1);
        assert_eq!(appended.current_version(), AggregateVersion::from_value(1));
        assert_eq!(appended.events()[0].event_index().value(), 0);
        assert!(matches!(
            appended.events()[0].payload(),
            MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(_))
        ));
    }

    #[test]
    fn rejects_stale_version_and_repeated_verification_without_appending() {
        let stream =
            CanonicalEventStream::try_from_ordered(match_id(), vec![existing_verified_event()])
                .unwrap();
        let stale_error = command(AggregateVersion::initial())
            .append_to_stream(stream.clone())
            .unwrap_err();
        assert_eq!(
            stale_error,
            VerifyMatchCommandError::EventAppend(
                super::super::EventAppendError::ExpectedVersionConflict {
                    expected: AggregateVersion::initial(),
                    actual: AggregateVersion::from_value(1),
                }
            )
        );

        let repeated_error = command(AggregateVersion::from_value(1))
            .append_to_stream(stream.clone())
            .unwrap_err();
        assert_eq!(
            repeated_error,
            VerifyMatchCommandError::AlreadyVerified {
                event_id: event_id("019ff500-3bc5-7cf4-b3d2-0728c077a930"),
            }
        );
        assert_eq!(stream.len(), 1);
    }

    #[test]
    fn reopens_only_a_verified_match_and_allows_a_new_verification_afterward() {
        let empty = CanonicalEventStream::try_from_ordered(match_id(), Vec::new()).unwrap();
        assert_eq!(
            reopen_command(
                AggregateVersion::initial(),
                event_id("019ff500-3bc5-7cf8-b3d2-0728c077a930"),
            )
            .append_to_stream(empty.clone())
            .unwrap_err(),
            ReopenMatchCommandError::MatchNotVerified
        );

        let verified = command(AggregateVersion::initial())
            .append_to_stream(empty)
            .unwrap();
        let reopened = reopen_command(
            AggregateVersion::from_value(1),
            event_id("019ff500-3bc5-7cf8-b3d2-0728c077a930"),
        )
        .append_to_stream(verified)
        .unwrap();

        assert_eq!(reopened.len(), 2);
        assert_eq!(reopened.current_version(), AggregateVersion::from_value(2));
        assert_eq!(reopened.events()[1].event_type().as_str(), "MatchReopened");
        assert_eq!(
            reopened.events()[1].verification_status(),
            EventVerificationStatus::Provisional
        );
        assert!(matches!(
            reopened.events()[1].payload(),
            MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(payload))
                if payload.reason().as_str() == "official protocol requires re-verification"
        ));

        let reverified = command_with_ids(
            AggregateVersion::from_value(2),
            command_id("019ff500-3bc5-7cf9-b3d2-0728c077a930"),
            event_id("019ff500-3bc5-7cfa-b3d2-0728c077a930"),
        )
        .append_to_stream(reopened)
        .unwrap();
        assert_eq!(
            reverified.current_version(),
            AggregateVersion::from_value(3)
        );
        assert_eq!(
            reverified.events()[2].event_type().as_str(),
            "MatchVerified"
        );
    }
}
