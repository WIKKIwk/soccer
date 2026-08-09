use super::{
    AggregateVersion, C1EventPayload, C1EventType, CanonicalEventStream, ClockMilliseconds,
    CommandId, DomainEventEnvelope, EventAppendError, EventId, EventIndex, EventTypeError,
    EventVerificationStatus, MatchEventPayload, MatchId, MatchPeriod,
};
use crate::{ActorIdentity, GoalRecordedPayload, UtcInstant};
use std::fmt;

/// The first C1 live-action command vocabulary.
///
/// Each variant represents one operator intention and maps to exactly one
/// canonical source event. Aggregate loading, authorization, and persistence
/// remain application/infrastructure concerns; the append helper only applies
/// this command's expected version to a supplied loaded event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C1LiveAction {
    RecordGoal(GoalRecordedPayload),
    RecordYellowCard(super::YellowCardRecordedPayload),
    RecordSecondYellowCard(super::SecondYellowCardRecordedPayload),
    RecordRedCard(super::RedCardRecordedPayload),
    RecordSubstitution(super::SubstitutionRecordedPayload),
}

impl C1LiveAction {
    pub const fn command_name(&self) -> &'static str {
        match self {
            Self::RecordGoal(_) => "RecordGoal",
            Self::RecordYellowCard(_) => "RecordYellowCard",
            Self::RecordSecondYellowCard(_) => "RecordSecondYellowCard",
            Self::RecordRedCard(_) => "RecordRedCard",
            Self::RecordSubstitution(_) => "RecordSubstitution",
        }
    }

    pub const fn event_type(&self) -> C1EventType {
        match self {
            Self::RecordGoal(_) => C1EventType::GoalRecorded,
            Self::RecordYellowCard(_) => C1EventType::YellowCardRecorded,
            Self::RecordSecondYellowCard(_) => C1EventType::SecondYellowCardRecorded,
            Self::RecordRedCard(_) => C1EventType::RedCardRecorded,
            Self::RecordSubstitution(_) => C1EventType::SubstitutionRecorded,
        }
    }

    fn into_payload(self) -> C1EventPayload {
        match self {
            Self::RecordGoal(payload) => C1EventPayload::GoalRecorded(payload),
            Self::RecordYellowCard(payload) => C1EventPayload::YellowCardRecorded(payload),
            Self::RecordSecondYellowCard(payload) => {
                C1EventPayload::SecondYellowCardRecorded(payload)
            }
            Self::RecordRedCard(payload) => C1EventPayload::RedCardRecorded(payload),
            Self::RecordSubstitution(payload) => C1EventPayload::SubstitutionRecorded(payload),
        }
    }
}

/// Metadata needed to turn one accepted C1 live action into an unsequenced
/// canonical event envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct C1LiveActionCommand {
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
    action: C1LiveAction,
}

impl C1LiveActionCommand {
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
        action: C1LiveAction,
    ) -> Result<Self, C1LiveActionCommandError> {
        if !period.is_live() {
            return Err(C1LiveActionCommandError::NotLivePeriod {
                command_name: action.command_name(),
                period,
            });
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
            action,
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

    pub const fn action(&self) -> &C1LiveAction {
        &self.action
    }

    pub const fn command_name(&self) -> &'static str {
        self.action.command_name()
    }

    pub const fn event_type(&self) -> C1EventType {
        self.action.event_type()
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

    /// Builds one event before the stream assigns aggregate version and local
    /// event index. No database or aggregate mutation occurs here.
    pub fn into_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<C1EventPayload>, C1LiveActionCommandError> {
        let payload = self.action.into_payload();
        let event_type = payload
            .event_type()
            .try_as_event_type()
            .map_err(C1LiveActionCommandError::InvalidEventType)?;

        Ok(DomainEventEnvelope::new(
            self.event_id,
            self.match_id,
            AggregateVersion::initial(),
            self.command_id,
            EventIndex::initial(),
            event_type,
            payload.schema_version(),
            self.period,
            self.clock_ms,
            self.occurred_at,
            self.recorded_at,
            self.actor,
            self.verification_status,
            payload,
            None,
            None,
        ))
    }

    /// Builds the same event in the union payload used by a canonical match
    /// stream that also contains correction markers.
    pub fn into_canonical_unsequenced_event(
        self,
    ) -> Result<DomainEventEnvelope<MatchEventPayload>, C1LiveActionCommandError> {
        self.into_unsequenced_event().map(|event| {
            let payload = event.payload().clone();
            event.map_payload(MatchEventPayload::C1(payload))
        })
    }

    /// Appends this single live action to a previously loaded canonical stream.
    /// The generic stream boundary performs the optimistic-version check and
    /// assigns the canonical aggregate version and command-local event index.
    /// No database or external state is changed by this operation.
    pub fn append_to_stream(
        self,
        stream: CanonicalEventStream<C1EventPayload>,
    ) -> Result<CanonicalEventStream<C1EventPayload>, C1LiveActionCommandError> {
        let expected_version = self.expected_version;
        let command_id = self.command_id;
        let event = self.into_unsequenced_event()?;

        stream
            .append_after_expected_version(expected_version, command_id, vec![event])
            .map_err(C1LiveActionCommandError::EventAppend)
    }

    /// Appends this live action to a canonical union stream that may already
    /// contain correction markers. The source stream remains immutable from
    /// the caller's perspective; the generic stream boundary assigns the new
    /// aggregate version and command-local event index.
    pub fn append_to_canonical_stream(
        self,
        stream: CanonicalEventStream<MatchEventPayload>,
    ) -> Result<CanonicalEventStream<MatchEventPayload>, C1LiveActionCommandError> {
        let expected_version = self.expected_version;
        let command_id = self.command_id;
        let event = self.into_canonical_unsequenced_event()?;

        stream
            .append_after_expected_version(expected_version, command_id, vec![event])
            .map_err(C1LiveActionCommandError::EventAppend)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C1LiveActionCommandError {
    NotLivePeriod {
        command_name: &'static str,
        period: MatchPeriod,
    },
    InvalidEventType(EventTypeError),
    EventAppend(EventAppendError),
}

impl fmt::Display for C1LiveActionCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLivePeriod {
                command_name,
                period,
            } => write!(
                formatter,
                "{command_name} cannot record a live action during {period:?}"
            ),
            Self::InvalidEventType(error) => {
                write!(
                    formatter,
                    "C1 live action has an invalid event type: {error}"
                )
            }
            Self::EventAppend(error) => {
                write!(
                    formatter,
                    "C1 live action could not append its event: {error}"
                )
            }
        }
    }
}

impl std::error::Error for C1LiveActionCommandError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidEventType(error) => Some(error),
            Self::EventAppend(error) => Some(error),
            Self::NotLivePeriod { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{C1LiveAction, C1LiveActionCommand, C1LiveActionCommandError};
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, C1EventType,
        CanonicalEventStream, ClockMilliseconds, CommandId, CorrectionReason, EventAppendError,
        EventId, EventVerificationStatus, GoalAwardType, GoalRecordedPayload, MatchEventPayload,
        MatchId, MatchPeriod, PlayerId, TeamId, UtcInstant, VoidEventCommand,
        YellowCardRecordedPayload,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("C1 action fixture UUID should be valid")
    }

    fn command_id() -> CommandId {
        CommandId::from_uuid(uuid("019fe5a0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn event_id() -> EventId {
        EventId::from_uuid(uuid("019fe5a0-3bc5-7cf1-b3d2-0728c077a930"))
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe5a0-3bc5-7cf2-b3d2-0728c077a930"))
    }

    fn team_id() -> TeamId {
        TeamId::from_uuid(uuid("019fe5a0-3bc5-7cf3-b3d2-0728c077a930"))
    }

    fn player_id() -> PlayerId {
        PlayerId::from_uuid(uuid("019fe5a0-3bc5-7cf4-b3d2-0728c077a930"))
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe5a0-3bc5-7cf5-b3d2-0728c077a930",
        )))
    }

    fn command(action: C1LiveAction) -> C1LiveActionCommand {
        command_with_expected_version(action, AggregateVersion::from_value(3))
    }

    fn command_with_expected_version(
        action: C1LiveAction,
        expected_version: AggregateVersion,
    ) -> C1LiveActionCommand {
        C1LiveActionCommand::try_new(
            command_id(),
            event_id(),
            match_id(),
            expected_version,
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(45).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            action,
        )
        .unwrap()
    }

    #[test]
    fn maps_each_c1_live_command_to_its_single_canonical_event() {
        let actions = vec![
            (
                C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                    team_id(),
                    Some(player_id()),
                    GoalAwardType::Penalty,
                )),
                "RecordGoal",
                "GoalRecorded",
            ),
            (
                C1LiveAction::RecordYellowCard(YellowCardRecordedPayload::new(player_id())),
                "RecordYellowCard",
                "YellowCardRecorded",
            ),
            (
                C1LiveAction::RecordSecondYellowCard(
                    super::super::SecondYellowCardRecordedPayload::new(player_id()),
                ),
                "RecordSecondYellowCard",
                "SecondYellowCardRecorded",
            ),
            (
                C1LiveAction::RecordRedCard(super::super::RedCardRecordedPayload::new(player_id())),
                "RecordRedCard",
                "RedCardRecorded",
            ),
            (
                C1LiveAction::RecordSubstitution(
                    super::super::SubstitutionRecordedPayload::try_new(
                        team_id(),
                        player_id(),
                        PlayerId::from_uuid(uuid("019fe5a0-3bc5-7cf6-b3d2-0728c077a930")),
                    )
                    .unwrap(),
                ),
                "RecordSubstitution",
                "SubstitutionRecorded",
            ),
        ];

        for (action, command_name, event_name) in actions {
            assert_eq!(action.command_name(), command_name);
            assert_eq!(action.event_type().as_str(), event_name);
            let event = command(action).into_unsequenced_event().unwrap();
            assert_eq!(event.event_id(), event_id());
            assert_eq!(event.match_id(), match_id());
            assert_eq!(event.aggregate_version(), AggregateVersion::initial());
            assert_eq!(event.event_index().value(), 0);
            assert_eq!(event.causation_command_id(), command_id());
            assert_eq!(event.event_type().as_str(), event_name);
            assert_eq!(event.period(), MatchPeriod::FirstHalf);
        }
    }

    #[test]
    fn preserves_metadata_and_typed_payload_meaning() {
        let action = C1LiveAction::RecordGoal(GoalRecordedPayload::new(
            team_id(),
            None,
            GoalAwardType::OwnGoal,
        ));
        let command = command(action);
        let expected_version = command.expected_version();
        assert_eq!(command.command_name(), "RecordGoal");
        assert_eq!(command.event_type(), C1EventType::GoalRecorded);
        assert_eq!(command.actor(), actor());
        assert_eq!(
            command.verification_status(),
            EventVerificationStatus::Provisional
        );
        assert_eq!(
            command.occurred_at(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap()
        );
        assert_eq!(
            command.recorded_at(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap()
        );
        let event = command.into_unsequenced_event().unwrap();

        assert_eq!(expected_version, AggregateVersion::from_value(3));
        assert_eq!(event.actor(), actor());
        assert_eq!(
            event.verification_status(),
            EventVerificationStatus::Provisional
        );
        assert_eq!(event.clock_ms().value(), 45_000);
        assert_eq!(
            event.occurred_at(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap()
        );
        assert_eq!(
            event.recorded_at(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap()
        );
        assert!(matches!(
            event.payload(),
            C1EventPayload::GoalRecorded(payload)
                if payload.award_type() == GoalAwardType::OwnGoal
                    && payload.scorer_player_id().is_none()
        ));
    }

    #[test]
    fn appends_one_live_action_with_expected_version_and_canonical_sequence() {
        let command = command_with_expected_version(
            C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::Regular,
            )),
            AggregateVersion::initial(),
        );
        let stream =
            CanonicalEventStream::<C1EventPayload>::try_from_ordered(match_id(), Vec::new())
                .unwrap();

        let appended = command.append_to_stream(stream).unwrap();

        assert_eq!(appended.len(), 1);
        assert_eq!(appended.current_version(), AggregateVersion::from_value(1));
        assert_eq!(appended.events()[0].aggregate_version().value(), 1);
        assert_eq!(appended.events()[0].event_index().value(), 0);
        assert_eq!(appended.events()[0].causation_command_id(), command_id());
        assert!(matches!(
            appended.events()[0].payload(),
            C1EventPayload::GoalRecorded(payload)
                if payload.award_type() == GoalAwardType::Regular
        ));
    }

    #[test]
    fn appends_a_live_action_after_a_correction_in_the_canonical_union_stream() {
        let initial =
            CanonicalEventStream::<MatchEventPayload>::try_from_ordered(match_id(), Vec::new())
                .unwrap();
        let source = command_with_expected_version(
            C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::Regular,
            )),
            AggregateVersion::initial(),
        )
        .append_to_canonical_stream(initial)
        .unwrap();
        let correction = VoidEventCommand::try_new(
            CommandId::from_uuid(uuid("019fe5a0-3bc5-7cf6-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019fe5a0-3bc5-7cf7-b3d2-0728c077a930")),
            match_id(),
            AggregateVersion::from_value(1),
            MatchPeriod::HalfTime,
            ClockMilliseconds::from_seconds(45).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_002).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_003).unwrap(),
            actor(),
            EventVerificationStatus::Verified,
            source.events()[0].event_id(),
            CorrectionReason::try_new("official protocol removed the goal").unwrap(),
        )
        .unwrap()
        .append_to_stream(source)
        .unwrap();
        let next_live_action = C1LiveActionCommand::try_new(
            CommandId::from_uuid(uuid("019fe5a0-3bc5-7cf8-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019fe5a0-3bc5-7cf9-b3d2-0728c077a930")),
            match_id(),
            AggregateVersion::from_value(2),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(47).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_004).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_005).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            C1LiveAction::RecordYellowCard(YellowCardRecordedPayload::new(player_id())),
        )
        .unwrap();

        let appended = next_live_action
            .append_to_canonical_stream(correction)
            .unwrap();

        assert_eq!(appended.len(), 3);
        assert_eq!(appended.current_version(), AggregateVersion::from_value(3));
        assert!(matches!(
            appended.events()[1].payload(),
            MatchEventPayload::Correction(_)
        ));
        assert_eq!(
            appended.events()[2].aggregate_version(),
            AggregateVersion::from_value(3)
        );
        assert!(matches!(
            appended.events()[2].payload(),
            MatchEventPayload::C1(C1EventPayload::YellowCardRecorded(_))
        ));
    }

    #[test]
    fn rejects_stale_expected_version_without_appending() {
        let initial_stream =
            CanonicalEventStream::<C1EventPayload>::try_from_ordered(match_id(), Vec::new())
                .unwrap();
        let stream = command_with_expected_version(
            C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::Regular,
            )),
            AggregateVersion::initial(),
        )
        .append_to_stream(initial_stream)
        .unwrap();

        let stale_command = command_with_expected_version(
            C1LiveAction::RecordYellowCard(YellowCardRecordedPayload::new(player_id())),
            AggregateVersion::initial(),
        );
        let error = stale_command.append_to_stream(stream.clone()).unwrap_err();

        assert_eq!(
            error,
            C1LiveActionCommandError::EventAppend(EventAppendError::ExpectedVersionConflict {
                expected: AggregateVersion::initial(),
                actual: AggregateVersion::from_value(1),
            })
        );
        assert_eq!(stream.len(), 1);
        assert_eq!(stream.current_version(), AggregateVersion::from_value(1));
    }

    #[test]
    fn rejects_break_periods_without_fabricating_an_event() {
        let result = C1LiveActionCommand::try_new(
            command_id(),
            event_id(),
            match_id(),
            AggregateVersion::initial(),
            MatchPeriod::HalfTime,
            ClockMilliseconds::zero(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                team_id(),
                None,
                GoalAwardType::Regular,
            )),
        );

        assert_eq!(
            result.unwrap_err(),
            C1LiveActionCommandError::NotLivePeriod {
                command_name: "RecordGoal",
                period: MatchPeriod::HalfTime,
            }
        );
    }
}
