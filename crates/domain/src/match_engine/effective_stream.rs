use super::{
    AggregateVersion, CanonicalEventStream, CorrectionEventType, DomainEventEnvelope, EventId,
    MatchId,
};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// A deterministic derived view of a canonical event stream.
///
/// Effective events keep their original immutable envelopes and canonical
/// aggregate versions. A void or replacement can therefore leave version gaps
/// in this view; the builder never resequences canonical evidence or pretends
/// that the derived view is a new source stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveEventStream<T> {
    match_id: MatchId,
    canonical_version: AggregateVersion,
    events: Vec<DomainEventEnvelope<T>>,
}

impl<T: Clone> EffectiveEventStream<T> {
    /// Builds an effective view without mutating the canonical stream.
    ///
    /// `EventVoided` is a correction marker, not a football source event, so
    /// the marker and its target are excluded. An event with
    /// `supersedes_event_id` is the replacement and remains effective while
    /// the superseded target is excluded. All referenced targets must exist in
    /// this match stream, and correction reference chains must be acyclic.
    pub fn try_from_canonical(
        stream: &CanonicalEventStream<T>,
    ) -> Result<Self, EffectiveEventStreamError> {
        let mut events_by_id = HashMap::with_capacity(stream.len());
        for event in stream.iter() {
            events_by_id.insert(event.event_id(), event);
        }

        let mut excluded_event_ids = HashSet::new();

        for event in stream.iter() {
            let event_id = event.event_id();
            let is_void_event =
                event.event_type().as_str() == CorrectionEventType::EventVoided.as_str();

            if is_void_event {
                if let Some(superseded_event_id) = event.supersedes_event_id() {
                    return Err(EffectiveEventStreamError::VoidEventCannotSupersede {
                        event_id,
                        superseded_event_id,
                    });
                }

                let voided_event_id = event
                    .voids_event_id()
                    .ok_or(EffectiveEventStreamError::VoidEventMissingTarget { event_id })?;
                validate_reference_target(
                    event_id,
                    voided_event_id,
                    CorrectionReferenceKind::Voids,
                    &events_by_id,
                )?;
                excluded_event_ids.insert(event_id);
                excluded_event_ids.insert(voided_event_id);
                continue;
            }

            if let Some(voided_event_id) = event.voids_event_id() {
                return Err(EffectiveEventStreamError::VoidReferenceOnNonVoidEvent {
                    event_id,
                    voided_event_id,
                });
            }

            if let Some(superseded_event_id) = event.supersedes_event_id() {
                validate_reference_target(
                    event_id,
                    superseded_event_id,
                    CorrectionReferenceKind::Supersedes,
                    &events_by_id,
                )?;
                excluded_event_ids.insert(superseded_event_id);
            }
        }

        for event in stream.iter() {
            if event.supersedes_event_id().is_some() || event.voids_event_id().is_some() {
                validate_correction_chain(event.event_id(), &events_by_id)?;
            }
        }

        let events = stream
            .iter()
            .filter(|event| !excluded_event_ids.contains(&event.event_id()))
            .cloned()
            .collect();

        Ok(Self {
            match_id: stream.match_id(),
            canonical_version: stream.current_version(),
            events,
        })
    }
}

impl<T> EffectiveEventStream<T> {
    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }

    /// The latest canonical version represented by the source stream.
    pub const fn canonical_version(&self) -> AggregateVersion {
        self.canonical_version
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

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

fn validate_reference_target<T>(
    correction_event_id: EventId,
    target_event_id: EventId,
    kind: CorrectionReferenceKind,
    events_by_id: &HashMap<EventId, &DomainEventEnvelope<T>>,
) -> Result<(), EffectiveEventStreamError> {
    if correction_event_id == target_event_id {
        return Err(EffectiveEventStreamError::SelfReferentialReference {
            event_id: correction_event_id,
            kind,
        });
    }

    if !events_by_id.contains_key(&target_event_id) {
        return Err(EffectiveEventStreamError::ReferenceTargetMissing {
            correction_event_id,
            target_event_id,
            kind,
        });
    }

    Ok(())
}

fn validate_correction_chain<T>(
    start_event_id: EventId,
    events_by_id: &HashMap<EventId, &DomainEventEnvelope<T>>,
) -> Result<(), EffectiveEventStreamError> {
    let mut visited = HashSet::new();
    let mut current_event_id = start_event_id;

    loop {
        if !visited.insert(current_event_id) {
            return Err(EffectiveEventStreamError::CorrectionCycle {
                event_id: current_event_id,
            });
        }

        let event = events_by_id
            .get(&current_event_id)
            .expect("reference targets are validated before chain traversal");
        let next_event_id = event.supersedes_event_id().or(event.voids_event_id());

        match next_event_id {
            Some(next_event_id) => current_event_id = next_event_id,
            None => return Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionReferenceKind {
    Supersedes,
    Voids,
}

impl fmt::Display for CorrectionReferenceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Supersedes => formatter.write_str("supersedes_event_id"),
            Self::Voids => formatter.write_str("voids_event_id"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveEventStreamError {
    VoidEventMissingTarget {
        event_id: EventId,
    },
    VoidEventCannotSupersede {
        event_id: EventId,
        superseded_event_id: EventId,
    },
    VoidReferenceOnNonVoidEvent {
        event_id: EventId,
        voided_event_id: EventId,
    },
    SelfReferentialReference {
        event_id: EventId,
        kind: CorrectionReferenceKind,
    },
    ReferenceTargetMissing {
        correction_event_id: EventId,
        target_event_id: EventId,
        kind: CorrectionReferenceKind,
    },
    CorrectionCycle {
        event_id: EventId,
    },
}

impl fmt::Display for EffectiveEventStreamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VoidEventMissingTarget { event_id } => {
                write!(formatter, "void event {event_id} has no target event")
            }
            Self::VoidEventCannotSupersede {
                event_id,
                superseded_event_id,
            } => write!(
                formatter,
                "void event {event_id} cannot supersede event {superseded_event_id}"
            ),
            Self::VoidReferenceOnNonVoidEvent {
                event_id,
                voided_event_id,
            } => write!(
                formatter,
                "event {event_id} uses voids_event_id for non-void event {voided_event_id}"
            ),
            Self::SelfReferentialReference { event_id, kind } => {
                write!(
                    formatter,
                    "event {event_id} references itself through {kind}"
                )
            }
            Self::ReferenceTargetMissing {
                correction_event_id,
                target_event_id,
                kind,
            } => write!(
                formatter,
                "event {correction_event_id} references missing event {target_event_id} through {kind}"
            ),
            Self::CorrectionCycle { event_id } => {
                write!(formatter, "correction chain cycles at event {event_id}")
            }
        }
    }
}

impl std::error::Error for EffectiveEventStreamError {}

#[cfg(test)]
mod tests {
    use super::{CorrectionReferenceKind, EffectiveEventStream, EffectiveEventStreamError};
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, CanonicalEventStream, ClockMilliseconds,
        CommandId, DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion, EventType,
        EventVerificationStatus, MatchId, MatchPeriod, UtcInstant,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("effective stream fixture UUID should be valid")
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe2d0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn event_id(value: &str) -> EventId {
        EventId::from_uuid(uuid(value))
    }

    fn command_id(value: &str) -> CommandId {
        CommandId::from_uuid(uuid(value))
    }

    fn event(
        id: &str,
        aggregate_version: u64,
        event_type: &str,
        supersedes_event_id: Option<&str>,
        voids_event_id: Option<&str>,
    ) -> DomainEventEnvelope<()> {
        DomainEventEnvelope::new(
            event_id(id),
            match_id(),
            AggregateVersion::from_value(aggregate_version),
            command_id(id),
            EventIndex::initial(),
            EventType::try_new(event_type).expect("fixture event type should be valid"),
            EventSchemaVersion::initial(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(10 * aggregate_version)
                .expect("fixture clock should be valid"),
            UtcInstant::from_unix_timestamp(1_777_000_000)
                .expect("fixture timestamp should be valid"),
            UtcInstant::from_unix_timestamp(1_777_000_001)
                .expect("fixture timestamp should be valid"),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe2d0-3bc5-7cf4-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            (),
            supersedes_event_id.map(event_id),
            voids_event_id.map(event_id),
        )
    }

    fn canonical(events: Vec<DomainEventEnvelope<()>>) -> CanonicalEventStream<()> {
        CanonicalEventStream::try_from_ordered(match_id(), events)
            .expect("effective stream fixture should be canonical")
    }

    #[test]
    fn keeps_source_envelopes_and_canonical_version_when_a_goal_is_voided() {
        let source_event_id = "019fe2d0-3bc5-7cf1-b3d2-0728c077a930";
        let later_event_id = "019fe2d0-3bc5-7cf2-b3d2-0728c077a930";
        let void_event_id = "019fe2d0-3bc5-7cf3-b3d2-0728c077a930";
        let stream = canonical(vec![
            event(source_event_id, 1, "GoalRecorded", None, None),
            event(later_event_id, 2, "YellowCardRecorded", None, None),
            event(void_event_id, 3, "EventVoided", None, Some(source_event_id)),
        ]);

        let effective = EffectiveEventStream::try_from_canonical(&stream).unwrap();

        assert_eq!(effective.match_id(), match_id());
        assert_eq!(
            effective.canonical_version(),
            AggregateVersion::from_value(3)
        );
        assert_eq!(effective.len(), 1);
        assert_eq!(effective.events()[0].event_id(), event_id(later_event_id));
        assert_eq!(
            effective.events()[0].aggregate_version(),
            AggregateVersion::from_value(2)
        );
        assert_eq!(stream.events()[0].event_id(), event_id(source_event_id));
        assert_eq!(
            stream.events()[2].voids_event_id(),
            Some(event_id(source_event_id))
        );
    }

    #[test]
    fn keeps_a_replacement_and_excludes_the_superseded_source() {
        let original_event_id = "019fe2d0-3bc5-7cf1-b3d2-0728c077a930";
        let replacement_event_id = "019fe2d0-3bc5-7cf2-b3d2-0728c077a930";
        let stream = canonical(vec![
            event(original_event_id, 1, "GoalRecorded", None, None),
            event(
                replacement_event_id,
                2,
                "EventCorrected",
                Some(original_event_id),
                None,
            ),
        ]);

        let effective = EffectiveEventStream::try_from_canonical(&stream).unwrap();

        assert_eq!(effective.len(), 1);
        assert_eq!(
            effective.events()[0].event_id(),
            event_id(replacement_event_id)
        );
        assert_eq!(
            effective.events()[0].supersedes_event_id(),
            Some(event_id(original_event_id))
        );
    }

    #[test]
    fn preserves_an_untouched_stream_exactly() {
        let stream = canonical(vec![event(
            "019fe2d0-3bc5-7cf1-b3d2-0728c077a930",
            1,
            "GoalRecorded",
            None,
            None,
        )]);

        let effective = EffectiveEventStream::try_from_canonical(&stream).unwrap();

        assert_eq!(effective.events(), stream.events());
        assert_eq!(effective.canonical_version(), stream.current_version());
    }

    #[test]
    fn rejects_missing_void_target_and_nonvoid_void_reference() {
        let missing_target = canonical(vec![event(
            "019fe2d0-3bc5-7cf1-b3d2-0728c077a930",
            1,
            "EventVoided",
            None,
            Some("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
        )]);
        assert_eq!(
            EffectiveEventStream::try_from_canonical(&missing_target).unwrap_err(),
            EffectiveEventStreamError::ReferenceTargetMissing {
                correction_event_id: event_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
                target_event_id: event_id("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
                kind: CorrectionReferenceKind::Voids,
            }
        );

        let nonvoid_reference = canonical(vec![
            event(
                "019fe2d0-3bc5-7cf1-b3d2-0728c077a930",
                1,
                "GoalRecorded",
                None,
                Some("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
            ),
            event(
                "019fe2d0-3bc5-7cf2-b3d2-0728c077a930",
                2,
                "YellowCardRecorded",
                None,
                None,
            ),
        ]);
        assert_eq!(
            EffectiveEventStream::try_from_canonical(&nonvoid_reference).unwrap_err(),
            EffectiveEventStreamError::VoidReferenceOnNonVoidEvent {
                event_id: event_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
                voided_event_id: event_id("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
            }
        );
    }

    #[test]
    fn rejects_self_reference_and_cyclic_correction_chains() {
        let self_reference = canonical(vec![event(
            "019fe2d0-3bc5-7cf1-b3d2-0728c077a930",
            1,
            "EventVoided",
            None,
            Some("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
        )]);
        assert_eq!(
            EffectiveEventStream::try_from_canonical(&self_reference).unwrap_err(),
            EffectiveEventStreamError::SelfReferentialReference {
                event_id: event_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
                kind: CorrectionReferenceKind::Voids,
            }
        );

        let first_void_id = "019fe2d0-3bc5-7cf1-b3d2-0728c077a930";
        let second_void_id = "019fe2d0-3bc5-7cf2-b3d2-0728c077a930";
        let cyclic = canonical(vec![
            event(first_void_id, 1, "EventVoided", None, Some(second_void_id)),
            event(second_void_id, 2, "EventVoided", None, Some(first_void_id)),
        ]);
        assert_eq!(
            EffectiveEventStream::try_from_canonical(&cyclic).unwrap_err(),
            EffectiveEventStreamError::CorrectionCycle {
                event_id: event_id(first_void_id),
            }
        );
    }

    #[test]
    fn rejects_a_void_event_that_also_claims_to_supersede() {
        let stream = canonical(vec![
            event(
                "019fe2d0-3bc5-7cf1-b3d2-0728c077a930",
                1,
                "GoalRecorded",
                None,
                None,
            ),
            event(
                "019fe2d0-3bc5-7cf2-b3d2-0728c077a930",
                2,
                "EventVoided",
                Some("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
                Some("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
            ),
        ]);

        assert_eq!(
            EffectiveEventStream::try_from_canonical(&stream).unwrap_err(),
            EffectiveEventStreamError::VoidEventCannotSupersede {
                event_id: event_id("019fe2d0-3bc5-7cf2-b3d2-0728c077a930"),
                superseded_event_id: event_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"),
            }
        );
    }
}
