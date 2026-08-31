use super::{EventSchemaVersion, EventType, EventTypeError};
use crate::{LineupEntry, LineupId, LineupValidationError, TeamId, validate_unique_players};
use std::fmt;

/// Canonical event vocabulary for the regular-time lifecycle slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecycleEventType {
    MatchScheduled,
    LineupSubmitted,
    LineupConfirmed,
    FirstHalfStarted,
    FirstHalfEnded,
    SecondHalfStarted,
    MatchFinished,
}

impl LifecycleEventType {
    pub const ALL: [Self; 7] = [
        Self::MatchScheduled,
        Self::LineupSubmitted,
        Self::LineupConfirmed,
        Self::FirstHalfStarted,
        Self::FirstHalfEnded,
        Self::SecondHalfStarted,
        Self::MatchFinished,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MatchScheduled => "MatchScheduled",
            Self::LineupSubmitted => "LineupSubmitted",
            Self::LineupConfirmed => "LineupConfirmed",
            Self::FirstHalfStarted => "FirstHalfStarted",
            Self::FirstHalfEnded => "FirstHalfEnded",
            Self::SecondHalfStarted => "SecondHalfStarted",
            Self::MatchFinished => "MatchFinished",
        }
    }

    pub const fn schema_version(self) -> EventSchemaVersion {
        EventSchemaVersion::initial()
    }

    pub fn try_as_event_type(self) -> Result<EventType, EventTypeError> {
        EventType::try_new(self.as_str())
    }

    pub fn try_from_str(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|event_type| event_type.as_str() == value)
    }
}

impl fmt::Display for LifecycleEventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Immutable lineup snapshot carried by `LineupSubmitted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineupSubmittedPayload {
    lineup_id: LineupId,
    team_id: TeamId,
    entries: Vec<LineupEntry>,
}

impl LineupSubmittedPayload {
    pub fn try_new(
        lineup_id: LineupId,
        team_id: TeamId,
        entries: impl Into<Vec<LineupEntry>>,
    ) -> Result<Self, LineupValidationError> {
        let entries = entries.into();
        validate_unique_players(&entries)?;
        Ok(Self {
            lineup_id,
            team_id,
            entries,
        })
    }

    pub const fn lineup_id(&self) -> LineupId {
        self.lineup_id
    }

    pub const fn team_id(&self) -> TeamId {
        self.team_id
    }

    pub fn entries(&self) -> &[LineupEntry] {
        &self.entries
    }
}

/// Typed payload union for canonical lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleEventPayload {
    MatchScheduled,
    LineupSubmitted(LineupSubmittedPayload),
    LineupConfirmed,
    FirstHalfStarted,
    FirstHalfEnded,
    SecondHalfStarted,
    MatchFinished,
}

impl LifecycleEventPayload {
    pub const fn event_type(&self) -> LifecycleEventType {
        match self {
            Self::MatchScheduled => LifecycleEventType::MatchScheduled,
            Self::LineupSubmitted(_) => LifecycleEventType::LineupSubmitted,
            Self::LineupConfirmed => LifecycleEventType::LineupConfirmed,
            Self::FirstHalfStarted => LifecycleEventType::FirstHalfStarted,
            Self::FirstHalfEnded => LifecycleEventType::FirstHalfEnded,
            Self::SecondHalfStarted => LifecycleEventType::SecondHalfStarted,
            Self::MatchFinished => LifecycleEventType::MatchFinished,
        }
    }

    pub const fn schema_version(&self) -> EventSchemaVersion {
        self.event_type().schema_version()
    }
}

#[cfg(test)]
mod tests {
    use super::{LifecycleEventPayload, LifecycleEventType, LineupSubmittedPayload};
    use crate::{EventSchemaVersion, LineupEntry, LineupId, PlayerId, TeamId};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("lifecycle event fixture UUID should be valid")
    }

    #[test]
    fn lifecycle_names_and_schema_versions_are_stable() {
        let names: Vec<_> = LifecycleEventType::ALL
            .into_iter()
            .map(|value| value.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "MatchScheduled",
                "LineupSubmitted",
                "LineupConfirmed",
                "FirstHalfStarted",
                "FirstHalfEnded",
                "SecondHalfStarted",
                "MatchFinished",
            ]
        );
        assert!(LifecycleEventType::try_from_str("FirstHalfStarted").is_some());
        assert!(LifecycleEventType::try_from_str("KickOff").is_none());
        assert_eq!(
            LifecycleEventType::MatchFinished.schema_version(),
            EventSchemaVersion::initial()
        );
    }

    #[test]
    fn submitted_lineup_keeps_an_immutable_validated_snapshot() {
        let player_id = PlayerId::from_uuid(uuid("019ff560-3bc5-7cf0-b3d2-0728c077a930"));
        let payload = LineupSubmittedPayload::try_new(
            LineupId::from_uuid(uuid("019ff560-3bc5-7cf1-b3d2-0728c077a930")),
            TeamId::from_uuid(uuid("019ff560-3bc5-7cf2-b3d2-0728c077a930")),
            vec![LineupEntry::starter(player_id)],
        )
        .expect("one player should form a valid submitted lineup");

        assert_eq!(payload.entries(), &[LineupEntry::starter(player_id)]);
        assert_eq!(
            LifecycleEventPayload::LineupSubmitted(payload).event_type(),
            LifecycleEventType::LineupSubmitted
        );
    }
}
