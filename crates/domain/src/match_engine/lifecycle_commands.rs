use super::{
    AggregateVersion, CanonicalEventStream, ClockMilliseconds, CommandId, DomainEventEnvelope,
    EffectiveEventStream, EffectiveEventStreamError, EventAppendError, EventId, EventIndex,
    EventTypeError, EventVerificationStatus, LifecycleEventPayload, LifecycleEventType,
    LifecycleProjection, LifecycleProjectionError, MatchEventPayload, MatchId, MatchPeriod,
};
use crate::{ActorIdentity, UtcInstant};
use std::fmt;

/// One idempotent regular-time lifecycle intention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchLifecycleCommand {
    command_id: CommandId,
    event_id: EventId,
    match_id: MatchId,
    expected_version: AggregateVersion,
    clock_ms: ClockMilliseconds,
    occurred_at: UtcInstant,
    recorded_at: UtcInstant,
    actor: ActorIdentity,
    action: LifecycleEventPayload,
}

impl MatchLifecycleCommand {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        command_id: CommandId,
        event_id: EventId,
        match_id: MatchId,
        expected_version: AggregateVersion,
        clock_ms: ClockMilliseconds,
        occurred_at: UtcInstant,
        recorded_at: UtcInstant,
        actor: ActorIdentity,
        action: LifecycleEventPayload,
    ) -> Self {
        Self {
            command_id,
            event_id,
            match_id,
            expected_version,
            clock_ms,
            occurred_at,
            recorded_at,
            actor,
            action,
        }
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

    pub const fn clock_ms(&self) -> ClockMilliseconds {
        self.clock_ms
    }

    pub const fn actor(&self) -> ActorIdentity {
        self.actor
    }

    pub const fn action(&self) -> &LifecycleEventPayload {
        &self.action
    }

    pub const fn event_type(&self) -> LifecycleEventType {
        self.action.event_type()
    }

    pub const fn command_name(&self) -> &'static str {
        match self.action {
            LifecycleEventPayload::MatchScheduled => "ScheduleMatch",
            LifecycleEventPayload::LineupSubmitted(_) => "SubmitLineup",
            LifecycleEventPayload::LineupConfirmed => "ConfirmLineup",
            LifecycleEventPayload::FirstHalfStarted => "StartFirstHalf",
            LifecycleEventPayload::FirstHalfEnded => "EndFirstHalf",
            LifecycleEventPayload::SecondHalfStarted => "StartSecondHalf",
            LifecycleEventPayload::MatchFinished => "EndMatch",
        }
    }

    pub const fn period(&self) -> MatchPeriod {
        match self.action {
            LifecycleEventPayload::MatchScheduled
            | LifecycleEventPayload::LineupSubmitted(_)
            | LifecycleEventPayload::LineupConfirmed => MatchPeriod::PreMatch,
            LifecycleEventPayload::FirstHalfStarted => MatchPeriod::FirstHalf,
            LifecycleEventPayload::FirstHalfEnded => MatchPeriod::HalfTime,
            LifecycleEventPayload::SecondHalfStarted | LifecycleEventPayload::MatchFinished => {
                MatchPeriod::SecondHalf
            }
        }
    }

    pub fn into_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<MatchEventPayload>, MatchLifecycleCommandError> {
        let event_type = self
            .event_type()
            .try_as_event_type()
            .map_err(MatchLifecycleCommandError::InvalidEventType)?;
        let schema_version = self.action.schema_version();
        Ok(DomainEventEnvelope::new(
            self.event_id,
            self.match_id,
            AggregateVersion::initial(),
            self.command_id,
            EventIndex::initial(),
            event_type,
            schema_version,
            self.period(),
            self.clock_ms,
            self.occurred_at,
            self.recorded_at,
            self.actor,
            EventVerificationStatus::Provisional,
            MatchEventPayload::Lifecycle(self.action),
            None,
            None,
        ))
    }

    pub fn append_to_stream(
        self,
        stream: CanonicalEventStream<MatchEventPayload>,
    ) -> Result<CanonicalEventStream<MatchEventPayload>, MatchLifecycleCommandError> {
        if stream.current_version() != self.expected_version {
            return Err(MatchLifecycleCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict {
                    expected: self.expected_version,
                    actual: stream.current_version(),
                },
            ));
        }

        let effective = EffectiveEventStream::try_from_canonical(&stream)
            .map_err(MatchLifecycleCommandError::EffectiveStream)?;
        LifecycleProjection::from_effective_stream(&effective)
            .map_err(MatchLifecycleCommandError::Lifecycle)?
            .validate_action(&self.action)
            .map_err(MatchLifecycleCommandError::Lifecycle)?;

        let expected_version = self.expected_version;
        let command_id = self.command_id;
        let event = self.into_unsequenced_event()?;
        stream
            .append_after_expected_version(expected_version, command_id, vec![event])
            .map_err(MatchLifecycleCommandError::EventAppend)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchLifecycleCommandError {
    InvalidEventType(EventTypeError),
    EffectiveStream(EffectiveEventStreamError),
    Lifecycle(LifecycleProjectionError),
    EventAppend(EventAppendError),
}

impl fmt::Display for MatchLifecycleCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEventType(error) => write!(formatter, "invalid lifecycle type: {error}"),
            Self::EffectiveStream(error) => {
                write!(formatter, "lifecycle effective stream failed: {error}")
            }
            Self::Lifecycle(error) => write!(formatter, "lifecycle transition failed: {error}"),
            Self::EventAppend(error) => write!(formatter, "lifecycle append failed: {error}"),
        }
    }
}

impl std::error::Error for MatchLifecycleCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEventType(error) => Some(error),
            Self::EffectiveStream(error) => Some(error),
            Self::Lifecycle(error) => Some(error),
            Self::EventAppend(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MatchLifecycleCommand, MatchLifecycleCommandError};
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, CanonicalEventStream, ClockMilliseconds,
        CommandId, EventAppendError, EventId, LifecycleEventPayload, MatchEventPayload, MatchId,
        MatchPeriod, UtcInstant,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("lifecycle command fixture UUID should be valid")
    }

    fn command(expected_version: AggregateVersion) -> MatchLifecycleCommand {
        MatchLifecycleCommand::new(
            CommandId::from_uuid(uuid("019ff580-3bc5-7cf0-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019ff580-3bc5-7cf1-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019ff580-3bc5-7cf2-b3d2-0728c077a930")),
            expected_version,
            ClockMilliseconds::zero(),
            UtcInstant::from_unix_timestamp(1_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_001).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019ff580-3bc5-7cf3-b3d2-0728c077a930",
            ))),
            LifecycleEventPayload::MatchScheduled,
        )
    }

    #[test]
    fn schedule_command_appends_one_canonical_lifecycle_event() {
        let command = command(AggregateVersion::initial());
        let match_id = command.match_id();
        let stream = CanonicalEventStream::try_from_ordered(match_id, vec![]).unwrap();
        let appended = command.append_to_stream(stream).unwrap();

        assert_eq!(appended.current_version(), AggregateVersion::from_value(1));
        assert!(matches!(
            appended.events()[0].payload(),
            MatchEventPayload::Lifecycle(LifecycleEventPayload::MatchScheduled)
        ));
        assert_eq!(appended.events()[0].period(), MatchPeriod::PreMatch);
    }

    #[test]
    fn stale_lifecycle_command_is_rejected_before_transition_reduction() {
        let stream = CanonicalEventStream::try_from_ordered(
            command(AggregateVersion::initial()).match_id(),
            vec![],
        )
        .unwrap();
        assert!(matches!(
            command(AggregateVersion::from_value(1)).append_to_stream(stream),
            Err(MatchLifecycleCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict { .. }
            ))
        ));
    }
}
