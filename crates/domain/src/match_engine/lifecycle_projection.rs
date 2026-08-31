use super::{
    EffectiveEventStream, EventId, LifecycleEventPayload, LifecycleEventType, MatchEventPayload,
    MatchPeriod, MatchState,
};
use crate::{LineupSubmittedPayload, TeamId};
use std::fmt;

/// Deterministic lifecycle and submitted-lineup view of the effective stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleProjection {
    state: MatchState,
    submitted_lineups: Vec<LineupSubmittedPayload>,
    lifecycle_event_count: usize,
}

impl LifecycleProjection {
    pub fn from_effective_stream(
        stream: &EffectiveEventStream<MatchEventPayload>,
    ) -> Result<Self, LifecycleProjectionError> {
        let mut projection = Self {
            state: MatchState::initial(),
            submitted_lineups: Vec::new(),
            lifecycle_event_count: 0,
        };

        for event in stream.iter() {
            let MatchEventPayload::Lifecycle(payload) = event.payload() else {
                continue;
            };
            projection.validate_envelope(event.event_id(), event.event_type().as_str(), payload)?;
            projection.apply(event.event_id(), payload)?;
            projection.lifecycle_event_count += 1;
        }
        Ok(projection)
    }

    pub const fn state(&self) -> MatchState {
        self.state
    }

    pub const fn has_lifecycle_events(&self) -> bool {
        self.lifecycle_event_count > 0
    }

    pub fn submitted_lineups(&self) -> &[LineupSubmittedPayload] {
        &self.submitted_lineups
    }

    pub fn lineup_for_team(&self, team_id: TeamId) -> Option<&LineupSubmittedPayload> {
        self.submitted_lineups
            .iter()
            .find(|lineup| lineup.team_id() == team_id)
    }

    pub const fn accepts_live_period(&self, period: MatchPeriod) -> bool {
        matches!(
            (self.state, period),
            (MatchState::FirstHalf, MatchPeriod::FirstHalf)
                | (MatchState::SecondHalf, MatchPeriod::SecondHalf)
        )
    }

    pub fn validate_action(
        &self,
        action: &LifecycleEventPayload,
    ) -> Result<(), LifecycleProjectionError> {
        let mut next = self.clone();
        next.apply_without_event_id(action)
    }

    fn validate_envelope(
        &self,
        event_id: EventId,
        actual_type: &str,
        payload: &LifecycleEventPayload,
    ) -> Result<(), LifecycleProjectionError> {
        let expected = payload.event_type();
        if actual_type != expected.as_str() {
            return Err(LifecycleProjectionError::EventTypeMismatch {
                event_id,
                expected,
                actual: actual_type.to_owned(),
            });
        }
        Ok(())
    }

    fn apply(
        &mut self,
        event_id: EventId,
        payload: &LifecycleEventPayload,
    ) -> Result<(), LifecycleProjectionError> {
        self.apply_inner(Some(event_id), payload)
    }

    fn apply_without_event_id(
        &mut self,
        payload: &LifecycleEventPayload,
    ) -> Result<(), LifecycleProjectionError> {
        self.apply_inner(None, payload)
    }

    fn apply_inner(
        &mut self,
        event_id: Option<EventId>,
        payload: &LifecycleEventPayload,
    ) -> Result<(), LifecycleProjectionError> {
        let event_type = payload.event_type();
        match payload {
            LifecycleEventPayload::MatchScheduled => {
                self.require_state(event_id, event_type, &[MatchState::Draft])?;
                self.state = MatchState::Scheduled;
            }
            LifecycleEventPayload::LineupSubmitted(lineup) => {
                self.require_state(
                    event_id,
                    event_type,
                    &[MatchState::Scheduled, MatchState::LineupsProvisional],
                )?;
                if self
                    .submitted_lineups
                    .iter()
                    .any(|existing| existing.team_id() == lineup.team_id())
                {
                    return Err(LifecycleProjectionError::DuplicateTeamLineup {
                        event_id,
                        team_id: lineup.team_id(),
                    });
                }
                if self.submitted_lineups.len() >= 2 {
                    return Err(LifecycleProjectionError::TooManyLineups { event_id });
                }
                self.submitted_lineups.push(lineup.clone());
                self.state = MatchState::LineupsProvisional;
            }
            LifecycleEventPayload::LineupConfirmed => {
                self.require_state(event_id, event_type, &[MatchState::LineupsProvisional])?;
                if self.submitted_lineups.len() != 2 {
                    return Err(LifecycleProjectionError::TwoLineupsRequired {
                        event_id,
                        actual: self.submitted_lineups.len(),
                    });
                }
                self.state = MatchState::LineupsConfirmed;
            }
            LifecycleEventPayload::FirstHalfStarted => {
                self.require_state(event_id, event_type, &[MatchState::LineupsConfirmed])?;
                self.state = MatchState::FirstHalf;
            }
            LifecycleEventPayload::FirstHalfEnded => {
                self.require_state(event_id, event_type, &[MatchState::FirstHalf])?;
                self.state = MatchState::HalfTime;
            }
            LifecycleEventPayload::SecondHalfStarted => {
                self.require_state(event_id, event_type, &[MatchState::HalfTime])?;
                self.state = MatchState::SecondHalf;
            }
            LifecycleEventPayload::MatchFinished => {
                self.require_state(event_id, event_type, &[MatchState::SecondHalf])?;
                self.state = MatchState::Finished;
            }
        }
        Ok(())
    }

    fn require_state(
        &self,
        event_id: Option<EventId>,
        event_type: LifecycleEventType,
        allowed: &[MatchState],
    ) -> Result<(), LifecycleProjectionError> {
        if allowed.contains(&self.state) {
            Ok(())
        } else {
            Err(LifecycleProjectionError::InvalidTransition {
                event_id,
                from: self.state,
                event_type,
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleProjectionError {
    EventTypeMismatch {
        event_id: EventId,
        expected: LifecycleEventType,
        actual: String,
    },
    InvalidTransition {
        event_id: Option<EventId>,
        from: MatchState,
        event_type: LifecycleEventType,
    },
    DuplicateTeamLineup {
        event_id: Option<EventId>,
        team_id: TeamId,
    },
    TooManyLineups {
        event_id: Option<EventId>,
    },
    TwoLineupsRequired {
        event_id: Option<EventId>,
        actual: usize,
    },
}

impl fmt::Display for LifecycleProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventTypeMismatch {
                event_id,
                expected,
                actual,
            } => write!(
                formatter,
                "lifecycle event {event_id} expects {expected}, envelope is {actual}"
            ),
            Self::InvalidTransition {
                from, event_type, ..
            } => write!(
                formatter,
                "cannot apply {event_type} while match is {from:?}"
            ),
            Self::DuplicateTeamLineup { team_id, .. } => {
                write!(formatter, "team {team_id} already submitted a lineup")
            }
            Self::TooManyLineups { .. } => formatter.write_str("a match accepts two lineups"),
            Self::TwoLineupsRequired { actual, .. } => write!(
                formatter,
                "two submitted lineups are required before confirmation, found {actual}"
            ),
        }
    }
}

impl std::error::Error for LifecycleProjectionError {}

#[cfg(test)]
mod tests {
    use super::{LifecycleProjection, LifecycleProjectionError};
    use crate::{
        CanonicalEventStream, EffectiveEventStream, LifecycleEventPayload, LineupEntry, LineupId,
        LineupSubmittedPayload, MatchEventPayload, MatchPeriod, MatchState, PlayerId, TeamId,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("lifecycle projection fixture UUID should be valid")
    }

    fn lineup(lineup: &str, team: &str, player: &str) -> LifecycleEventPayload {
        LifecycleEventPayload::LineupSubmitted(
            LineupSubmittedPayload::try_new(
                LineupId::from_uuid(uuid(lineup)),
                TeamId::from_uuid(uuid(team)),
                vec![LineupEntry::starter(PlayerId::from_uuid(uuid(player)))],
            )
            .unwrap(),
        )
    }

    fn empty_projection() -> LifecycleProjection {
        let canonical = CanonicalEventStream::<MatchEventPayload>::try_from_ordered(
            crate::MatchId::from_uuid(uuid("019ff570-3bc5-7cf0-b3d2-0728c077a930")),
            vec![],
        )
        .unwrap();
        let effective = EffectiveEventStream::try_from_canonical(&canonical).unwrap();
        LifecycleProjection::from_effective_stream(&effective).unwrap()
    }

    #[test]
    fn validates_the_regular_time_lifecycle_and_two_lineups() {
        let mut projection = empty_projection();
        for action in [
            LifecycleEventPayload::MatchScheduled,
            lineup(
                "019ff570-3bc5-7cf1-b3d2-0728c077a930",
                "019ff570-3bc5-7cf2-b3d2-0728c077a930",
                "019ff570-3bc5-7cf3-b3d2-0728c077a930",
            ),
            lineup(
                "019ff570-3bc5-7cf4-b3d2-0728c077a930",
                "019ff570-3bc5-7cf5-b3d2-0728c077a930",
                "019ff570-3bc5-7cf6-b3d2-0728c077a930",
            ),
            LifecycleEventPayload::LineupConfirmed,
            LifecycleEventPayload::FirstHalfStarted,
            LifecycleEventPayload::FirstHalfEnded,
            LifecycleEventPayload::SecondHalfStarted,
            LifecycleEventPayload::MatchFinished,
        ] {
            projection.validate_action(&action).unwrap();
            projection.apply_without_event_id(&action).unwrap();
        }

        assert_eq!(projection.state(), MatchState::Finished);
        assert_eq!(projection.submitted_lineups().len(), 2);
    }

    #[test]
    fn rejects_early_confirmation_and_matches_live_period_to_state() {
        let mut projection = empty_projection();
        projection
            .apply_without_event_id(&LifecycleEventPayload::MatchScheduled)
            .unwrap();
        assert!(matches!(
            projection.validate_action(&LifecycleEventPayload::LineupConfirmed),
            Err(LifecycleProjectionError::InvalidTransition { .. })
        ));
        projection.state = MatchState::FirstHalf;
        assert!(projection.accepts_live_period(MatchPeriod::FirstHalf));
        assert!(!projection.accepts_live_period(MatchPeriod::SecondHalf));
    }
}
