use super::{
    AggregateVersion, AggregateVersionError, CommandId, DomainEventEnvelope, EventId, EventIndex,
    EventIndexError, MatchId,
};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// An aggregate's canonical events in persisted version order.
///
/// The storage adapter is responsible for asking the database for ascending
/// aggregate versions. This boundary then verifies identity and contiguity so
/// replay cannot silently accept a partial or mixed-match stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalEventStream<T> {
    match_id: MatchId,
    events: Vec<DomainEventEnvelope<T>>,
}

impl<T> CanonicalEventStream<T> {
    pub fn try_from_ordered(
        match_id: MatchId,
        events: Vec<DomainEventEnvelope<T>>,
    ) -> Result<Self, EventStreamError> {
        Self::validate_appended_batch(match_id, AggregateVersion::initial(), &events)?;

        Ok(Self { match_id, events })
    }

    /// Validates an already-versioned batch that follows `current_version`.
    ///
    /// This is used by persistence adapters after the aggregate operation has
    /// assigned versions and event indexes. It validates only the supplied
    /// batch; proving that the preceding version exists remains the caller's
    /// responsibility when it loads the aggregate stream in the same
    /// transaction.
    pub fn validate_appended_batch(
        match_id: MatchId,
        current_version: AggregateVersion,
        events: &[DomainEventEnvelope<T>],
    ) -> Result<(), EventStreamError> {
        let mut expected_version = current_version;
        let mut seen_event_ids = HashSet::with_capacity(events.len());
        let mut last_event_indexes: HashMap<CommandId, EventIndex> = HashMap::new();

        for event in events {
            if event.match_id() != match_id {
                return Err(EventStreamError::MatchIdMismatch {
                    expected: match_id,
                    actual: event.match_id(),
                    event_id: event.event_id(),
                });
            }

            if !seen_event_ids.insert(event.event_id()) {
                return Err(EventStreamError::DuplicateEventId {
                    event_id: event.event_id(),
                });
            }

            let expected_event_index = match last_event_indexes
                .get(&event.causation_command_id())
                .copied()
            {
                Some(last_index) => last_index
                    .try_next()
                    .map_err(EventStreamError::EventIndexOverflow)?,
                None => EventIndex::initial(),
            };
            if event.event_index() != expected_event_index {
                return Err(EventStreamError::NonContiguousEventIndex {
                    command_id: event.causation_command_id(),
                    event_id: event.event_id(),
                    expected: expected_event_index,
                    actual: event.event_index(),
                });
            }
            last_event_indexes.insert(event.causation_command_id(), event.event_index());

            expected_version = expected_version
                .try_next()
                .map_err(EventStreamError::VersionOverflow)?;
            if event.aggregate_version() != expected_version {
                return Err(EventStreamError::NonContiguousAggregateVersion {
                    expected: expected_version,
                    actual: event.aggregate_version(),
                    event_id: event.event_id(),
                });
            }
        }

        Ok(())
    }

    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }

    pub fn current_version(&self) -> AggregateVersion {
        self.events
            .last()
            .map(DomainEventEnvelope::aggregate_version)
            .unwrap_or_else(AggregateVersion::initial)
    }

    pub fn events(&self) -> &[DomainEventEnvelope<T>] {
        &self.events
    }

    pub fn iter(&self) -> impl Iterator<Item = &DomainEventEnvelope<T>> {
        self.events.iter()
    }

    pub fn into_events(self) -> Vec<DomainEventEnvelope<T>> {
        self.events
    }

    /// Appends one command's events after an explicit optimistic-concurrency
    /// check and assigns aggregate versions and command-local indexes.
    pub fn append_after_expected_version(
        mut self,
        expected_version: AggregateVersion,
        command_id: CommandId,
        events: Vec<DomainEventEnvelope<T>>,
    ) -> Result<Self, EventAppendError> {
        let actual_version = self.current_version();
        if expected_version != actual_version {
            return Err(EventAppendError::ExpectedVersionConflict {
                expected: expected_version,
                actual: actual_version,
            });
        }

        for (index, event) in events.iter().enumerate() {
            if event.match_id() != self.match_id {
                return Err(EventAppendError::MatchIdMismatch {
                    expected: self.match_id,
                    actual: event.match_id(),
                    event_id: event.event_id(),
                });
            }
            if event.causation_command_id() != command_id {
                return Err(EventAppendError::CausationCommandMismatch {
                    expected: command_id,
                    actual: event.causation_command_id(),
                    event_id: event.event_id(),
                });
            }
            if self
                .events
                .iter()
                .chain(events[..index].iter())
                .any(|existing| existing.event_id() == event.event_id())
            {
                return Err(EventAppendError::DuplicateEventId {
                    event_id: event.event_id(),
                });
            }
            u32::try_from(index).map_err(|_| EventAppendError::EventIndexOverflow { index })?;
        }

        for (index, event) in events.into_iter().enumerate() {
            let aggregate_version = self
                .current_version()
                .try_next()
                .map_err(EventAppendError::VersionOverflow)?;
            let event_index = u32::try_from(index)
                .map(EventIndex::from_value)
                .map_err(|_| EventAppendError::EventIndexOverflow { index })?;
            self.events
                .push(event.with_sequence(aggregate_version, event_index));
        }

        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStreamError {
    MatchIdMismatch {
        expected: MatchId,
        actual: MatchId,
        event_id: EventId,
    },
    NonContiguousAggregateVersion {
        expected: AggregateVersion,
        actual: AggregateVersion,
        event_id: EventId,
    },
    DuplicateEventId {
        event_id: EventId,
    },
    NonContiguousEventIndex {
        command_id: CommandId,
        event_id: EventId,
        expected: EventIndex,
        actual: EventIndex,
    },
    EventIndexOverflow(EventIndexError),
    VersionOverflow(AggregateVersionError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventAppendError {
    ExpectedVersionConflict {
        expected: AggregateVersion,
        actual: AggregateVersion,
    },
    MatchIdMismatch {
        expected: MatchId,
        actual: MatchId,
        event_id: EventId,
    },
    CausationCommandMismatch {
        expected: CommandId,
        actual: CommandId,
        event_id: EventId,
    },
    DuplicateEventId {
        event_id: EventId,
    },
    EventIndexOverflow {
        index: usize,
    },
    VersionOverflow(AggregateVersionError),
}

impl fmt::Display for EventStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatchIdMismatch {
                expected,
                actual,
                event_id,
            } => write!(
                formatter,
                "event {event_id} belongs to match {actual}, expected match {expected}"
            ),
            Self::NonContiguousAggregateVersion {
                expected,
                actual,
                event_id,
            } => write!(
                formatter,
                "event {event_id} has aggregate version {}, expected {}",
                actual.value(),
                expected.value()
            ),
            Self::DuplicateEventId { event_id } => {
                write!(
                    formatter,
                    "event {event_id} appears more than once in the stream"
                )
            }
            Self::NonContiguousEventIndex {
                command_id,
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} for command {command_id} has index {}, expected {}",
                actual.value(),
                expected.value()
            ),
            Self::EventIndexOverflow(error) => error.fmt(formatter),
            Self::VersionOverflow(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for EventStreamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::VersionOverflow(error) => Some(error),
            Self::EventIndexOverflow(error) => Some(error),
            Self::MatchIdMismatch { .. }
            | Self::NonContiguousAggregateVersion { .. }
            | Self::DuplicateEventId { .. }
            | Self::NonContiguousEventIndex { .. } => None,
        }
    }
}

impl fmt::Display for EventAppendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedVersionConflict { expected, actual } => write!(
                formatter,
                "expected aggregate version {}, actual version {}",
                expected.value(),
                actual.value()
            ),
            Self::MatchIdMismatch {
                expected,
                actual,
                event_id,
            } => write!(
                formatter,
                "event {event_id} belongs to match {actual}, expected match {expected}"
            ),
            Self::CausationCommandMismatch {
                expected,
                actual,
                event_id,
            } => write!(
                formatter,
                "event {event_id} has command {actual}, expected command {expected}"
            ),
            Self::DuplicateEventId { event_id } => {
                write!(
                    formatter,
                    "event {event_id} is already present in the stream"
                )
            }
            Self::EventIndexOverflow { index } => {
                write!(formatter, "event index overflow at batch position {index}")
            }
            Self::VersionOverflow(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for EventAppendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::VersionOverflow(error) => Some(error),
            Self::ExpectedVersionConflict { .. }
            | Self::MatchIdMismatch { .. }
            | Self::CausationCommandMismatch { .. }
            | Self::DuplicateEventId { .. }
            | Self::EventIndexOverflow { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CanonicalEventStream, EventAppendError, EventStreamError};
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, AggregateVersionError, ClockMilliseconds,
        CommandId, DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion, EventType,
        EventVerificationStatus, MatchId, MatchPeriod, UtcInstant,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("stream fixture UUID should be valid")
    }

    fn match_id(value: &str) -> MatchId {
        MatchId::from_uuid(uuid(value))
    }

    fn event(match_id: MatchId, event_id: &str, aggregate_version: u64) -> DomainEventEnvelope<()> {
        event_with_command(
            match_id,
            event_id,
            aggregate_version,
            CommandId::from_uuid(uuid(event_id)),
        )
    }

    fn event_with_command(
        match_id: MatchId,
        event_id: &str,
        aggregate_version: u64,
        command_id: CommandId,
    ) -> DomainEventEnvelope<()> {
        event_with_command_and_index(match_id, event_id, aggregate_version, command_id, 0)
    }

    fn event_with_command_and_index(
        match_id: MatchId,
        event_id: &str,
        aggregate_version: u64,
        command_id: CommandId,
        event_index: u32,
    ) -> DomainEventEnvelope<()> {
        DomainEventEnvelope::new(
            EventId::from_uuid(uuid(event_id)),
            match_id,
            AggregateVersion::from_value(aggregate_version),
            command_id,
            EventIndex::from_value(event_index),
            EventType::try_new("GoalRecorded").unwrap(),
            EventSchemaVersion::initial(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(12).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe2b0-3bc5-7cf5-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            (),
            None,
            None,
        )
    }

    #[test]
    fn accepts_an_empty_or_contiguous_stream_and_exposes_current_version() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let empty = CanonicalEventStream::<()>::try_from_ordered(match_id, Vec::new()).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.current_version().value(), 0);

        let stream = CanonicalEventStream::try_from_ordered(
            match_id,
            vec![
                event(match_id, "019fe2b0-3bc5-7cf1-b3d2-0728c077a930", 1),
                event(match_id, "019fe2b0-3bc5-7cf2-b3d2-0728c077a930", 2),
            ],
        )
        .unwrap();

        assert_eq!(stream.match_id(), match_id);
        assert_eq!(stream.len(), 2);
        assert_eq!(stream.current_version().value(), 2);
    }

    #[test]
    fn rejects_mixed_match_events() {
        let expected_match = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let actual_match = match_id("019fe2b0-3bc5-7cf1-b3d2-0728c077a930");
        let event_id = EventId::from_uuid(uuid("019fe2b0-3bc5-7cf2-b3d2-0728c077a930"));

        assert_eq!(
            CanonicalEventStream::try_from_ordered(
                expected_match,
                vec![event(
                    actual_match,
                    "019fe2b0-3bc5-7cf2-b3d2-0728c077a930",
                    1
                )]
            )
            .unwrap_err(),
            EventStreamError::MatchIdMismatch {
                expected: expected_match,
                actual: actual_match,
                event_id,
            }
        );
    }

    #[test]
    fn rejects_gaps_and_out_of_order_versions() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let event_id = EventId::from_uuid(uuid("019fe2b0-3bc5-7cf2-b3d2-0728c077a930"));

        assert_eq!(
            CanonicalEventStream::try_from_ordered(
                match_id,
                vec![event(match_id, "019fe2b0-3bc5-7cf2-b3d2-0728c077a930", 2,)]
            )
            .unwrap_err(),
            EventStreamError::NonContiguousAggregateVersion {
                expected: super::AggregateVersion::initial().try_next().unwrap(),
                actual: super::AggregateVersion::from_value(2),
                event_id,
            }
        );

        assert_eq!(
            CanonicalEventStream::try_from_ordered(
                match_id,
                vec![
                    event(match_id, "019fe2b0-3bc5-7cf1-b3d2-0728c077a930", 1,),
                    event(match_id, "019fe2b0-3bc5-7cf2-b3d2-0728c077a930", 3,),
                ]
            )
            .unwrap_err(),
            EventStreamError::NonContiguousAggregateVersion {
                expected: super::AggregateVersion::from_value(2),
                actual: super::AggregateVersion::from_value(3),
                event_id,
            }
        );
    }

    #[test]
    fn rejects_duplicate_event_ids_even_when_versions_are_contiguous() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let duplicate_event_id = EventId::from_uuid(uuid("019fe2b0-3bc5-7cf1-b3d2-0728c077a930"));

        assert_eq!(
            CanonicalEventStream::try_from_ordered(
                match_id,
                vec![
                    event(match_id, "019fe2b0-3bc5-7cf1-b3d2-0728c077a930", 1,),
                    event(match_id, "019fe2b0-3bc5-7cf1-b3d2-0728c077a930", 2,),
                ]
            )
            .unwrap_err(),
            EventStreamError::DuplicateEventId {
                event_id: duplicate_event_id,
            }
        );
    }

    #[test]
    fn rejects_non_contiguous_event_indexes_per_causation_command() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let command_id = CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf5-b3d2-0728c077a930"));
        let event_id = EventId::from_uuid(uuid("019fe2b0-3bc5-7cf7-b3d2-0728c077a930"));

        assert_eq!(
            CanonicalEventStream::try_from_ordered(
                match_id,
                vec![
                    event_with_command_and_index(
                        match_id,
                        "019fe2b0-3bc5-7cf6-b3d2-0728c077a930",
                        1,
                        command_id,
                        0,
                    ),
                    event_with_command_and_index(
                        match_id,
                        "019fe2b0-3bc5-7cf7-b3d2-0728c077a930",
                        2,
                        command_id,
                        2,
                    ),
                ]
            )
            .unwrap_err(),
            EventStreamError::NonContiguousEventIndex {
                command_id,
                event_id,
                expected: EventIndex::from_value(1),
                actual: EventIndex::from_value(2),
            }
        );
    }

    #[test]
    fn appends_multiple_events_with_contiguous_versions_and_zero_based_indexes() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let command_id = CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf5-b3d2-0728c077a930"));
        let stream = CanonicalEventStream::try_from_ordered(
            match_id,
            vec![event(match_id, "019fe2b0-3bc5-7cf1-b3d2-0728c077a930", 1)],
        )
        .unwrap();

        let appended = stream
            .append_after_expected_version(
                AggregateVersion::from_value(1),
                command_id,
                vec![
                    event_with_command(
                        match_id,
                        "019fe2b0-3bc5-7cf2-b3d2-0728c077a930",
                        99,
                        command_id,
                    ),
                    event_with_command(
                        match_id,
                        "019fe2b0-3bc5-7cf3-b3d2-0728c077a930",
                        100,
                        command_id,
                    ),
                ],
            )
            .unwrap();

        assert_eq!(appended.current_version().value(), 3);
        assert_eq!(appended.events()[1].aggregate_version().value(), 2);
        assert_eq!(appended.events()[2].aggregate_version().value(), 3);
        assert_eq!(appended.events()[1].event_index().value(), 0);
        assert_eq!(appended.events()[2].event_index().value(), 1);
        assert_eq!(appended.events()[1].causation_command_id(), command_id);
        assert_eq!(appended.events()[2].causation_command_id(), command_id);
    }

    #[test]
    fn validates_an_already_versioned_batch_after_a_nonzero_current_version() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let command_id = CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf5-b3d2-0728c077a930"));
        let first = event_with_command_and_index(
            match_id,
            "019fe2b0-3bc5-7cf6-b3d2-0728c077a930",
            6,
            command_id,
            0,
        );
        let second = event_with_command_and_index(
            match_id,
            "019fe2b0-3bc5-7cf7-b3d2-0728c077a930",
            7,
            command_id,
            1,
        );

        CanonicalEventStream::validate_appended_batch(
            match_id,
            AggregateVersion::from_value(5),
            &[first, second],
        )
        .unwrap();

        let gap = event_with_command_and_index(
            match_id,
            "019fe2b0-3bc5-7cf8-b3d2-0728c077a930",
            9,
            command_id,
            0,
        );
        assert!(matches!(
            CanonicalEventStream::validate_appended_batch(
                match_id,
                AggregateVersion::from_value(7),
                &[gap],
            ),
            Err(EventStreamError::NonContiguousAggregateVersion { .. })
        ));
    }

    #[test]
    fn rejects_a_versioned_batch_after_aggregate_version_overflow() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let command_id = CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf5-b3d2-0728c077a930"));

        assert_eq!(
            CanonicalEventStream::validate_appended_batch(
                match_id,
                AggregateVersion::from_value(u64::MAX),
                &[event_with_command_and_index(
                    match_id,
                    "019fe2b0-3bc5-7cf6-b3d2-0728c077a930",
                    1,
                    command_id,
                    0,
                )],
            )
            .unwrap_err(),
            EventStreamError::VersionOverflow(AggregateVersionError::Overflow {
                current: u64::MAX,
                event_count: 1,
            })
        );
    }

    #[test]
    fn rejects_stale_expected_versions_and_inconsistent_batch_identity() {
        let match_id = match_id("019fe2b0-3bc5-7cf0-b3d2-0728c077a930");
        let other_match = MatchId::from_uuid(uuid("019fe2b0-3bc5-7cf4-b3d2-0728c077a930"));
        let command_id = CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf5-b3d2-0728c077a930"));
        let other_command = CommandId::from_uuid(uuid("019fe2b0-3bc5-7cf6-b3d2-0728c077a930"));
        let stream = CanonicalEventStream::try_from_ordered(
            match_id,
            vec![event(match_id, "019fe2b0-3bc5-7cf1-b3d2-0728c077a930", 1)],
        )
        .unwrap();

        assert_eq!(
            stream
                .clone()
                .append_after_expected_version(AggregateVersion::initial(), command_id, Vec::new(),)
                .unwrap_err(),
            EventAppendError::ExpectedVersionConflict {
                expected: AggregateVersion::initial(),
                actual: AggregateVersion::from_value(1),
            }
        );

        assert!(matches!(
            stream
                .clone()
                .append_after_expected_version(
                    AggregateVersion::from_value(1),
                    command_id,
                    vec![event_with_command(
                        other_match,
                        "019fe2b0-3bc5-7cf2-b3d2-0728c077a930",
                        0,
                        command_id,
                    )],
                )
                .unwrap_err(),
            EventAppendError::MatchIdMismatch { .. }
        ));

        assert!(matches!(
            stream
                .clone()
                .append_after_expected_version(
                    AggregateVersion::from_value(1),
                    command_id,
                    vec![event_with_command(
                        match_id,
                        "019fe2b0-3bc5-7cf2-b3d2-0728c077a930",
                        0,
                        other_command,
                    )],
                )
                .unwrap_err(),
            EventAppendError::CausationCommandMismatch {
                expected,
                actual,
                ..
            } if expected == command_id && actual == other_command
        ));
    }
}
