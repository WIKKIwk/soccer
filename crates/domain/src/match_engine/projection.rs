use super::{
    AggregateVersion, AggregateVersionError, C1EventPayload, C1EventType, CanonicalEventStream,
    ClockMilliseconds, DomainEventEnvelope, EffectiveEventStream, EventId, EventSchemaVersion,
    EventType, MatchEventPayload, MatchId, MatchPeriod,
};
use crate::TeamId;
use std::fmt;

/// The score produced by reducing the effective canonical C1 event stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MatchScore {
    home_goals: u32,
    away_goals: u32,
}

impl MatchScore {
    pub const fn new() -> Self {
        Self {
            home_goals: 0,
            away_goals: 0,
        }
    }

    pub const fn home_goals(self) -> u32 {
        self.home_goals
    }

    pub const fn away_goals(self) -> u32 {
        self.away_goals
    }

    fn increment(&mut self, side: ScoringSide) -> Result<(), ()> {
        let goals = match side {
            ScoringSide::Home => &mut self.home_goals,
            ScoringSide::Away => &mut self.away_goals,
        };

        *goals = goals.checked_add(1).ok_or(())?;
        Ok(())
    }
}

impl Default for MatchScore {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScoringSide {
    Home,
    Away,
}

/// The immutable event metadata needed by the first synchronous timeline.
///
/// The payload remains on the canonical event stream. This row is only a
/// derived, ordered view for current match state and does not become another
/// source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimelineEntry {
    event_id: EventId,
    aggregate_version: AggregateVersion,
    event_index: super::EventIndex,
    event_type: C1EventType,
    period: MatchPeriod,
    clock_ms: ClockMilliseconds,
}

impl TimelineEntry {
    fn from_event(event: &DomainEventEnvelope<C1EventPayload>) -> Self {
        Self {
            event_id: event.event_id(),
            aggregate_version: event.aggregate_version(),
            event_index: event.event_index(),
            event_type: event.payload().event_type(),
            period: event.period(),
            clock_ms: event.clock_ms(),
        }
    }

    pub const fn event_id(self) -> EventId {
        self.event_id
    }

    pub const fn aggregate_version(self) -> AggregateVersion {
        self.aggregate_version
    }

    pub const fn event_index(self) -> super::EventIndex {
        self.event_index
    }

    pub const fn event_type(self) -> C1EventType {
        self.event_type
    }

    pub const fn period(self) -> MatchPeriod {
        self.period
    }

    pub const fn clock_ms(self) -> ClockMilliseconds {
        self.clock_ms
    }
}

/// Synchronous score and timeline state reduced from one canonical match
/// event stream.
///
/// This projection intentionally does not validate lineups, players on the
/// pitch, or card state. It accepts either a contiguous typed C1 canonical
/// stream or a correction-resolved effective view and only derives score plus
/// timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchProjection {
    match_id: MatchId,
    home_team_id: TeamId,
    away_team_id: TeamId,
    score: MatchScore,
    timeline: Vec<TimelineEntry>,
    current_aggregate_version: AggregateVersion,
}

impl MatchProjection {
    pub fn new(
        match_id: MatchId,
        home_team_id: TeamId,
        away_team_id: TeamId,
    ) -> Result<Self, MatchProjectionError> {
        if home_team_id == away_team_id {
            return Err(MatchProjectionError::SameTeams {
                team_id: home_team_id,
            });
        }

        Ok(Self {
            match_id,
            home_team_id,
            away_team_id,
            score: MatchScore::new(),
            timeline: Vec::new(),
            current_aggregate_version: AggregateVersion::initial(),
        })
    }

    /// Reduces an ordered effective stream into a fresh synchronous state.
    pub fn from_stream(
        stream: &CanonicalEventStream<C1EventPayload>,
        home_team_id: TeamId,
        away_team_id: TeamId,
    ) -> Result<Self, MatchProjectionError> {
        let mut projection = Self::new(stream.match_id(), home_team_id, away_team_id)?;
        for event in stream.iter() {
            projection.reduce_event(event)?;
        }
        Ok(projection)
    }

    /// Reduces a correction-resolved canonical match view.
    ///
    /// The effective view can have canonical version gaps because source event
    /// envelopes are preserved. The projection therefore requires strictly
    /// increasing canonical versions while retaining the source stream's
    /// final canonical version as its synchronization point.
    pub fn from_effective_stream(
        stream: &EffectiveEventStream<MatchEventPayload>,
        home_team_id: TeamId,
        away_team_id: TeamId,
    ) -> Result<Self, MatchProjectionError> {
        let mut projection = Self::new(stream.match_id(), home_team_id, away_team_id)?;

        for event in stream.iter() {
            let payload = match event.payload() {
                MatchEventPayload::C1(payload) => payload,
                MatchEventPayload::Lifecycle(_) => continue,
                MatchEventPayload::Correction(_) => {
                    return Err(MatchProjectionError::UnsupportedEffectiveCorrection {
                        event_id: event.event_id(),
                        event_type: event.event_type().clone(),
                    });
                }
                MatchEventPayload::Verification(_) => continue,
            };
            let typed_event = event.clone().map_payload(payload.clone());
            projection.reduce_effective_event(&typed_event)?;
        }

        projection.current_aggregate_version = stream.canonical_version();
        Ok(projection)
    }

    /// Reduces one canonical C1 event after the current aggregate version.
    pub fn reduce_event(
        &mut self,
        event: &DomainEventEnvelope<C1EventPayload>,
    ) -> Result<(), MatchProjectionError> {
        self.reduce_c1_event(event, true)
    }

    fn reduce_effective_event(
        &mut self,
        event: &DomainEventEnvelope<C1EventPayload>,
    ) -> Result<(), MatchProjectionError> {
        self.reduce_c1_event(event, false)
    }

    fn reduce_c1_event(
        &mut self,
        event: &DomainEventEnvelope<C1EventPayload>,
        require_contiguous_version: bool,
    ) -> Result<(), MatchProjectionError> {
        if event.match_id() != self.match_id {
            return Err(MatchProjectionError::MatchIdMismatch {
                expected: self.match_id,
                actual: event.match_id(),
                event_id: event.event_id(),
            });
        }

        if require_contiguous_version {
            let expected_version = self
                .current_aggregate_version
                .try_next()
                .map_err(MatchProjectionError::VersionOverflow)?;
            if event.aggregate_version() != expected_version {
                return Err(MatchProjectionError::NonContiguousAggregateVersion {
                    expected: expected_version,
                    actual: event.aggregate_version(),
                    event_id: event.event_id(),
                });
            }
        } else if event.aggregate_version() <= self.current_aggregate_version {
            return Err(MatchProjectionError::NonIncreasingAggregateVersion {
                previous: self.current_aggregate_version,
                actual: event.aggregate_version(),
                event_id: event.event_id(),
            });
        }

        if self
            .timeline
            .iter()
            .any(|entry| entry.event_id() == event.event_id())
        {
            return Err(MatchProjectionError::DuplicateEventId {
                event_id: event.event_id(),
            });
        }

        let expected_event_type = event.payload().event_type();
        if event.event_type().as_str() != expected_event_type.as_str() {
            return Err(MatchProjectionError::EventTypeMismatch {
                event_id: event.event_id(),
                expected: expected_event_type,
                actual: event.event_type().clone(),
            });
        }

        let expected_schema_version = expected_event_type.schema_version();
        if event.event_schema_version() != expected_schema_version {
            return Err(MatchProjectionError::EventSchemaVersionMismatch {
                event_id: event.event_id(),
                expected: expected_schema_version,
                actual: event.event_schema_version(),
            });
        }

        if !event.period().is_live() {
            return Err(MatchProjectionError::NonLiveEventPeriod {
                event_id: event.event_id(),
                period: event.period(),
            });
        }

        let mut next_score = self.score;
        if let C1EventPayload::GoalRecorded(payload) = event.payload() {
            let side = self.scoring_side(event.event_id(), payload.team_id())?;
            next_score
                .increment(side)
                .map_err(|()| MatchProjectionError::ScoreOverflow {
                    event_id: event.event_id(),
                    team_id: payload.team_id(),
                })?;
        }

        self.score = next_score;
        self.current_aggregate_version = event.aggregate_version();
        self.timeline.push(TimelineEntry::from_event(event));
        Ok(())
    }

    pub const fn match_id(&self) -> MatchId {
        self.match_id
    }

    pub const fn home_team_id(&self) -> TeamId {
        self.home_team_id
    }

    pub const fn away_team_id(&self) -> TeamId {
        self.away_team_id
    }

    pub const fn score(&self) -> MatchScore {
        self.score
    }

    pub fn timeline(&self) -> &[TimelineEntry] {
        &self.timeline
    }

    pub const fn current_aggregate_version(&self) -> AggregateVersion {
        self.current_aggregate_version
    }

    fn scoring_side(
        &self,
        event_id: EventId,
        team_id: TeamId,
    ) -> Result<ScoringSide, MatchProjectionError> {
        if team_id == self.home_team_id {
            Ok(ScoringSide::Home)
        } else if team_id == self.away_team_id {
            Ok(ScoringSide::Away)
        } else {
            Err(MatchProjectionError::TeamNotInMatch { event_id, team_id })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchProjectionError {
    SameTeams {
        team_id: TeamId,
    },
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
    NonIncreasingAggregateVersion {
        previous: AggregateVersion,
        actual: AggregateVersion,
        event_id: EventId,
    },
    VersionOverflow(AggregateVersionError),
    DuplicateEventId {
        event_id: EventId,
    },
    EventTypeMismatch {
        event_id: EventId,
        expected: C1EventType,
        actual: EventType,
    },
    EventSchemaVersionMismatch {
        event_id: EventId,
        expected: EventSchemaVersion,
        actual: EventSchemaVersion,
    },
    NonLiveEventPeriod {
        event_id: EventId,
        period: MatchPeriod,
    },
    TeamNotInMatch {
        event_id: EventId,
        team_id: TeamId,
    },
    ScoreOverflow {
        event_id: EventId,
        team_id: TeamId,
    },
    UnsupportedEffectiveCorrection {
        event_id: EventId,
        event_type: EventType,
    },
}

impl fmt::Display for MatchProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SameTeams { team_id } => {
                write!(formatter, "home and away teams must differ: {team_id}")
            }
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
            Self::NonIncreasingAggregateVersion {
                previous,
                actual,
                event_id,
            } => write!(
                formatter,
                "effective event {event_id} has aggregate version {}, not greater than {}",
                actual.value(),
                previous.value()
            ),
            Self::VersionOverflow(error) => error.fmt(formatter),
            Self::DuplicateEventId { event_id } => {
                write!(
                    formatter,
                    "event {event_id} appears more than once in the projection"
                )
            }
            Self::EventTypeMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} payload is {expected}, envelope is {}",
                actual.as_str()
            ),
            Self::EventSchemaVersionMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "event {event_id} has schema version {}, expected {}",
                actual.value(),
                expected.value()
            ),
            Self::NonLiveEventPeriod { event_id, period } => write!(
                formatter,
                "C1 event {event_id} cannot be reduced during non-live period {period:?}"
            ),
            Self::TeamNotInMatch { event_id, team_id } => write!(
                formatter,
                "event {event_id} credits team {team_id}, which is not in the match"
            ),
            Self::ScoreOverflow { event_id, team_id } => write!(
                formatter,
                "event {event_id} would overflow the score for team {team_id}"
            ),
            Self::UnsupportedEffectiveCorrection {
                event_id,
                event_type,
            } => write!(
                formatter,
                "effective event {event_id} has unsupported correction payload for {}",
                event_type.as_str()
            ),
        }
    }
}

impl std::error::Error for MatchProjectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::VersionOverflow(error) => Some(error),
            Self::SameTeams { .. }
            | Self::MatchIdMismatch { .. }
            | Self::NonContiguousAggregateVersion { .. }
            | Self::NonIncreasingAggregateVersion { .. }
            | Self::DuplicateEventId { .. }
            | Self::EventTypeMismatch { .. }
            | Self::EventSchemaVersionMismatch { .. }
            | Self::NonLiveEventPeriod { .. }
            | Self::TeamNotInMatch { .. }
            | Self::ScoreOverflow { .. }
            | Self::UnsupportedEffectiveCorrection { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MatchProjection, MatchProjectionError};
    use crate::{
        ActorIdentity, AdminUserId, AggregateVersion, C1EventPayload, C1EventType,
        CanonicalEventStream, ClockMilliseconds, CommandId, CorrectionReason, DomainEventEnvelope,
        EffectiveEventStream, EventId, EventIndex, EventSchemaVersion, EventType,
        EventVerificationStatus, GoalAwardType, GoalRecordedPayload, MatchEventPayload, MatchId,
        MatchPeriod, PlayerId, RedCardRecordedPayload, ReopenMatchCommand,
        SecondYellowCardRecordedPayload, SubstitutionRecordedPayload, TeamId, UtcInstant,
        VerifyMatchCommand, VoidEventCommand, YellowCardRecordedPayload,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("projection fixture UUID should be valid")
    }

    fn match_id(value: &str) -> MatchId {
        MatchId::from_uuid(uuid(value))
    }

    fn home_team_id() -> TeamId {
        TeamId::from_uuid(uuid("019fe2e0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn away_team_id() -> TeamId {
        TeamId::from_uuid(uuid("019fe2e0-3bc5-7cf1-b3d2-0728c077a930"))
    }

    fn other_team_id() -> TeamId {
        TeamId::from_uuid(uuid("019fe2e0-3bc5-7cf2-b3d2-0728c077a930"))
    }

    fn player_id(value: &str) -> PlayerId {
        PlayerId::from_uuid(uuid(value))
    }

    fn event_id(value: &str) -> EventId {
        EventId::from_uuid(uuid(value))
    }

    fn event(
        event_id: &str,
        aggregate_version: u64,
        payload: C1EventPayload,
    ) -> DomainEventEnvelope<C1EventPayload> {
        event_in_period(event_id, aggregate_version, MatchPeriod::FirstHalf, payload)
    }

    fn event_in_period(
        event_id: &str,
        aggregate_version: u64,
        period: MatchPeriod,
        payload: C1EventPayload,
    ) -> DomainEventEnvelope<C1EventPayload> {
        event_with_metadata(
            event_id,
            aggregate_version,
            payload.clone().event_type().try_as_event_type().unwrap(),
            payload.schema_version(),
            period,
            payload,
        )
    }

    fn event_with_metadata(
        id: &str,
        aggregate_version: u64,
        event_type: EventType,
        schema_version: EventSchemaVersion,
        period: MatchPeriod,
        payload: C1EventPayload,
    ) -> DomainEventEnvelope<C1EventPayload> {
        DomainEventEnvelope::new(
            event_id(id),
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            AggregateVersion::from_value(aggregate_version),
            CommandId::from_uuid(uuid(id)),
            EventIndex::initial(),
            event_type,
            schema_version,
            period,
            ClockMilliseconds::from_millis(45_000),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe2e0-3bc5-7cf5-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            payload,
            None,
            None,
        )
    }

    fn projection() -> MatchProjection {
        MatchProjection::new(
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            home_team_id(),
            away_team_id(),
        )
        .expect("fixture teams should differ")
    }

    fn match_event(
        event: DomainEventEnvelope<C1EventPayload>,
    ) -> DomainEventEnvelope<MatchEventPayload> {
        let payload = event.payload().clone();
        event.map_payload(MatchEventPayload::C1(payload))
    }

    #[test]
    fn score_reduces_regular_penalty_and_own_goal_by_credited_team() {
        let mut projection = projection();

        projection
            .reduce_event(&event(
                "019fe2e0-3bc5-7cf6-b3d2-0728c077a930",
                1,
                C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                    home_team_id(),
                    Some(player_id("019fe2e0-3bc5-7cf7-b3d2-0728c077a930")),
                    GoalAwardType::Regular,
                )),
            ))
            .unwrap();
        projection
            .reduce_event(&event(
                "019fe2e0-3bc5-7cf8-b3d2-0728c077a930",
                2,
                C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                    home_team_id(),
                    None,
                    GoalAwardType::Penalty,
                )),
            ))
            .unwrap();
        projection
            .reduce_event(&event(
                "019fe2e0-3bc5-7cf9-b3d2-0728c077a930",
                3,
                C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                    away_team_id(),
                    None,
                    GoalAwardType::OwnGoal,
                )),
            ))
            .unwrap();

        assert_eq!(projection.score().home_goals(), 2);
        assert_eq!(projection.score().away_goals(), 1);
        assert_eq!(projection.current_aggregate_version().value(), 3);
        assert_eq!(projection.timeline().len(), 3);
        assert_eq!(
            projection.timeline()[2].event_type().as_str(),
            "GoalRecorded"
        );
    }

    #[test]
    fn cards_and_substitution_are_timeline_events_without_score_changes() {
        let mut projection = projection();
        let player = player_id("019fe2e0-3bc5-7cfa-b3d2-0728c077a930");
        let substitute = player_id("019fe2e0-3bc5-7cfb-b3d2-0728c077a930");

        let events = [
            C1EventPayload::YellowCardRecorded(YellowCardRecordedPayload::new(player)),
            C1EventPayload::SecondYellowCardRecorded(SecondYellowCardRecordedPayload::new(player)),
            C1EventPayload::RedCardRecorded(RedCardRecordedPayload::new(player_id(
                "019fe2e0-3bc5-7cfc-b3d2-0728c077a930",
            ))),
            C1EventPayload::SubstitutionRecorded(
                SubstitutionRecordedPayload::try_new(
                    home_team_id(),
                    substitute,
                    player_id("019fe2e0-3bc5-7cfd-b3d2-0728c077a930"),
                )
                .unwrap(),
            ),
        ];

        for (index, payload) in events.into_iter().enumerate() {
            projection
                .reduce_event(&event(
                    &format!("019fe2e0-3bc5-7d{:02x}-b3d2-0728c077a930", index),
                    (index + 1) as u64,
                    payload,
                ))
                .unwrap();
        }

        assert_eq!(projection.score().home_goals(), 0);
        assert_eq!(projection.score().away_goals(), 0);
        assert_eq!(
            projection
                .timeline()
                .iter()
                .map(|entry| entry.event_type())
                .collect::<Vec<_>>(),
            vec![
                crate::C1EventType::YellowCardRecorded,
                crate::C1EventType::SecondYellowCardRecorded,
                crate::C1EventType::RedCardRecorded,
                crate::C1EventType::SubstitutionRecorded,
            ]
        );
    }

    #[test]
    fn rejects_unknown_goal_team_without_mutating_projection() {
        let mut projection = projection();
        let error = projection
            .reduce_event(&event(
                "019fe2e0-3bc5-7d10-b3d2-0728c077a930",
                1,
                C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                    other_team_id(),
                    None,
                    GoalAwardType::Regular,
                )),
            ))
            .unwrap_err();

        assert_eq!(
            error,
            MatchProjectionError::TeamNotInMatch {
                event_id: event_id("019fe2e0-3bc5-7d10-b3d2-0728c077a930"),
                team_id: other_team_id(),
            }
        );
        assert_eq!(projection.score().home_goals(), 0);
        assert!(projection.timeline().is_empty());
        assert_eq!(
            projection.current_aggregate_version(),
            AggregateVersion::initial()
        );
    }

    #[test]
    fn rejects_c1_events_during_a_break_without_mutating_projection() {
        let mut projection = projection();
        let event_id = event_id("019fe2e0-3bc5-7d1b-b3d2-0728c077a930");
        let error = projection
            .reduce_event(&event_in_period(
                "019fe2e0-3bc5-7d1b-b3d2-0728c077a930",
                1,
                MatchPeriod::HalfTime,
                C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                    home_team_id(),
                    None,
                    GoalAwardType::Regular,
                )),
            ))
            .unwrap_err();

        assert_eq!(
            error,
            MatchProjectionError::NonLiveEventPeriod {
                event_id,
                period: MatchPeriod::HalfTime,
            }
        );
        assert_eq!(projection.score(), crate::MatchScore::new());
        assert!(projection.timeline().is_empty());
        assert_eq!(
            projection.current_aggregate_version(),
            AggregateVersion::initial()
        );
    }

    #[test]
    fn rejects_envelope_payload_type_or_schema_mismatch() {
        let mut projection = projection();
        let payload = C1EventPayload::YellowCardRecorded(YellowCardRecordedPayload::new(
            player_id("019fe2e0-3bc5-7d11-b3d2-0728c077a930"),
        ));
        let type_error = projection
            .reduce_event(&event_with_metadata(
                "019fe2e0-3bc5-7d12-b3d2-0728c077a930",
                1,
                C1EventType::GoalRecorded.try_as_event_type().unwrap(),
                payload.schema_version(),
                MatchPeriod::FirstHalf,
                payload.clone(),
            ))
            .unwrap_err();

        assert!(matches!(
            type_error,
            MatchProjectionError::EventTypeMismatch {
                expected: crate::C1EventType::YellowCardRecorded,
                ..
            }
        ));

        let schema_error = projection
            .reduce_event(&event_with_metadata(
                "019fe2e0-3bc5-7d13-b3d2-0728c077a930",
                1,
                payload.event_type().try_as_event_type().unwrap(),
                EventSchemaVersion::try_new(2).unwrap(),
                MatchPeriod::FirstHalf,
                payload,
            ))
            .unwrap_err();

        assert_eq!(
            schema_error,
            MatchProjectionError::EventSchemaVersionMismatch {
                event_id: event_id("019fe2e0-3bc5-7d13-b3d2-0728c077a930"),
                expected: EventSchemaVersion::initial(),
                actual: EventSchemaVersion::try_new(2).unwrap(),
            }
        );
        assert!(projection.timeline().is_empty());
    }

    #[test]
    fn rejects_non_contiguous_and_duplicate_event_versions() {
        let mut projection = projection();
        let first = event(
            "019fe2e0-3bc5-7d14-b3d2-0728c077a930",
            1,
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                home_team_id(),
                None,
                GoalAwardType::Regular,
            )),
        );
        projection.reduce_event(&first).unwrap();

        let gap = event(
            "019fe2e0-3bc5-7d15-b3d2-0728c077a930",
            3,
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                away_team_id(),
                None,
                GoalAwardType::Regular,
            )),
        );
        assert!(matches!(
            projection.reduce_event(&gap),
            Err(MatchProjectionError::NonContiguousAggregateVersion {
                expected,
                actual,
                ..
            }) if expected.value() == 2 && actual.value() == 3
        ));

        let duplicate = event(
            "019fe2e0-3bc5-7d14-b3d2-0728c077a930",
            2,
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                away_team_id(),
                None,
                GoalAwardType::Regular,
            )),
        );
        assert_eq!(
            projection.reduce_event(&duplicate).unwrap_err(),
            MatchProjectionError::DuplicateEventId {
                event_id: first.event_id(),
            }
        );
        assert_eq!(projection.score().home_goals(), 1);
        assert_eq!(projection.score().away_goals(), 0);
    }

    #[test]
    fn replaying_the_same_canonical_stream_is_deterministic() {
        let events = vec![
            event(
                "019fe2e0-3bc5-7d16-b3d2-0728c077a930",
                1,
                C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                    home_team_id(),
                    None,
                    GoalAwardType::Penalty,
                )),
            ),
            event(
                "019fe2e0-3bc5-7d17-b3d2-0728c077a930",
                2,
                C1EventPayload::SubstitutionRecorded(
                    SubstitutionRecordedPayload::try_new(
                        away_team_id(),
                        player_id("019fe2e0-3bc5-7d18-b3d2-0728c077a930"),
                        player_id("019fe2e0-3bc5-7d19-b3d2-0728c077a930"),
                    )
                    .unwrap(),
                ),
            ),
        ];
        let stream = crate::CanonicalEventStream::try_from_ordered(
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            events,
        )
        .unwrap();

        let first = MatchProjection::from_stream(&stream, home_team_id(), away_team_id()).unwrap();
        let second = MatchProjection::from_stream(&stream, home_team_id(), away_team_id()).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.score().home_goals(), 1);
        assert_eq!(first.score().away_goals(), 0);
        assert_eq!(first.timeline().len(), 2);
    }

    #[test]
    fn rebuilding_from_an_effective_stream_removes_a_voided_goal_from_the_score() {
        let home_goal = event(
            "019fe2e0-3bc5-7d20-b3d2-0728c077a930",
            1,
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                home_team_id(),
                None,
                GoalAwardType::Regular,
            )),
        );
        let home_goal_id = home_goal.event_id();
        let away_goal = event(
            "019fe2e0-3bc5-7d21-b3d2-0728c077a930",
            2,
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                away_team_id(),
                None,
                GoalAwardType::Penalty,
            )),
        );
        let mut canonical = CanonicalEventStream::try_from_ordered(
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            vec![match_event(home_goal), match_event(away_goal)],
        )
        .expect("typed union fixture should be canonical");

        let void_command = VoidEventCommand::try_new(
            CommandId::from_uuid(uuid("019fe2e0-3bc5-7d22-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019fe2e0-3bc5-7d23-b3d2-0728c077a930")),
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            AggregateVersion::from_value(2),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_millis(90_000),
            UtcInstant::from_unix_timestamp(1_777_000_002).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_003).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe2e0-3bc5-7cf5-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Verified,
            home_goal_id,
            CorrectionReason::try_new("official protocol removed the goal")
                .expect("fixture correction reason should be valid"),
        )
        .expect("void command should be valid");
        let void_command_id = void_command.command_id();
        canonical = canonical
            .append_after_expected_version(
                AggregateVersion::from_value(2),
                void_command_id,
                vec![
                    void_command
                        .into_canonical_unsequenced_event()
                        .expect("void event should be representable"),
                ],
            )
            .expect("void event should append after both goals");

        let effective = EffectiveEventStream::try_from_canonical(&canonical)
            .expect("voided stream should build an effective view");
        let projection =
            MatchProjection::from_effective_stream(&effective, home_team_id(), away_team_id())
                .expect("effective C1 stream should reduce");

        assert_eq!(projection.score().home_goals(), 0);
        assert_eq!(projection.score().away_goals(), 1);
        assert_eq!(projection.timeline().len(), 1);
        assert_eq!(
            projection.timeline()[0].event_id(),
            event_id("019fe2e0-3bc5-7d21-b3d2-0728c077a930")
        );
        assert_eq!(projection.timeline()[0].aggregate_version().value(), 2);
        assert_eq!(projection.current_aggregate_version().value(), 3);
    }

    #[test]
    fn rebuilding_from_an_effective_stream_ignores_match_verification_marker() {
        let home_goal = event(
            "019fe2e0-3bc5-7d24-b3d2-0728c077a930",
            1,
            C1EventPayload::GoalRecorded(GoalRecordedPayload::new(
                home_team_id(),
                None,
                GoalAwardType::Regular,
            )),
        );
        let canonical = CanonicalEventStream::try_from_ordered(
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            vec![match_event(home_goal)],
        )
        .expect("typed union fixture should be canonical");
        let canonical = VerifyMatchCommand::try_new(
            CommandId::from_uuid(uuid("019fe2e0-3bc5-7d25-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019fe2e0-3bc5-7d26-b3d2-0728c077a930")),
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            AggregateVersion::from_value(1),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_millis(90_000),
            UtcInstant::from_unix_timestamp(1_777_000_004).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_005).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe2e0-3bc5-7cf5-b3d2-0728c077a930",
            ))),
        )
        .expect("verification command should be valid")
        .append_to_stream(canonical)
        .expect("verification marker should append after the goal");
        let canonical = ReopenMatchCommand::try_new(
            CommandId::from_uuid(uuid("019fe2e0-3bc5-7d27-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019fe2e0-3bc5-7d28-b3d2-0728c077a930")),
            match_id("019fe2e0-3bc5-7cf4-b3d2-0728c077a930"),
            AggregateVersion::from_value(2),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_millis(91_000),
            UtcInstant::from_unix_timestamp(1_777_000_006).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_007).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe2e0-3bc5-7cf5-b3d2-0728c077a930",
            ))),
            CorrectionReason::try_new("official protocol requires re-verification").unwrap(),
        )
        .expect("reopen command should be valid")
        .append_to_stream(canonical)
        .expect("reopen marker should append after verification");

        let effective = EffectiveEventStream::try_from_canonical(&canonical)
            .expect("verification marker should build an effective view");
        let projection =
            MatchProjection::from_effective_stream(&effective, home_team_id(), away_team_id())
                .expect("verification marker should not affect C1 reduction");

        assert_eq!(projection.score().home_goals(), 1);
        assert_eq!(projection.score().away_goals(), 0);
        assert_eq!(projection.timeline().len(), 1);
        assert_eq!(
            projection.timeline()[0].event_id(),
            event_id("019fe2e0-3bc5-7d24-b3d2-0728c077a930")
        );
        assert_eq!(projection.current_aggregate_version().value(), 3);
    }

    #[test]
    fn rejects_a_fixture_with_the_same_home_and_away_team() {
        assert_eq!(
            MatchProjection::new(
                match_id("019fe2e0-3bc5-7d1a-b3d2-0728c077a930"),
                home_team_id(),
                home_team_id(),
            )
            .unwrap_err(),
            MatchProjectionError::SameTeams {
                team_id: home_team_id(),
            }
        );
    }
}
