use super::{EventSchemaVersion, EventType, EventTypeError};
use crate::{PlayerId, TeamId};
use std::fmt;

/// The canonical C1 event names for live match actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum C1EventType {
    GoalRecorded,
    YellowCardRecorded,
    SecondYellowCardRecorded,
    RedCardRecorded,
    SubstitutionRecorded,
}

impl C1EventType {
    pub const ALL: [Self; 5] = [
        Self::GoalRecorded,
        Self::YellowCardRecorded,
        Self::SecondYellowCardRecorded,
        Self::RedCardRecorded,
        Self::SubstitutionRecorded,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GoalRecorded => "GoalRecorded",
            Self::YellowCardRecorded => "YellowCardRecorded",
            Self::SecondYellowCardRecorded => "SecondYellowCardRecorded",
            Self::RedCardRecorded => "RedCardRecorded",
            Self::SubstitutionRecorded => "SubstitutionRecorded",
        }
    }

    pub fn try_as_event_type(self) -> Result<EventType, EventTypeError> {
        EventType::try_new(self.as_str())
    }

    pub const fn schema_version(self) -> EventSchemaVersion {
        EventSchemaVersion::initial()
    }
}

impl fmt::Display for C1EventType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The score-affecting classification of a C1 goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GoalAwardType {
    Regular,
    Penalty,
    OwnGoal,
}

impl GoalAwardType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Penalty => "penalty",
            Self::OwnGoal => "own_goal",
        }
    }
}

impl fmt::Display for GoalAwardType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed payload for `GoalRecorded`.
///
/// `team_id` is the team credited with the goal. The scorer is optional so a
/// provisional observation can preserve an unknown player without inventing a
/// player identity. For `OwnGoal`, the optional player is the player whose own
/// goal produced the credited team's score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalRecordedPayload {
    team_id: TeamId,
    scorer_player_id: Option<PlayerId>,
    award_type: GoalAwardType,
}

impl GoalRecordedPayload {
    pub const fn new(
        team_id: TeamId,
        scorer_player_id: Option<PlayerId>,
        award_type: GoalAwardType,
    ) -> Self {
        Self {
            team_id,
            scorer_player_id,
            award_type,
        }
    }

    pub const fn team_id(&self) -> TeamId {
        self.team_id
    }

    pub const fn scorer_player_id(&self) -> Option<PlayerId> {
        self.scorer_player_id
    }

    pub const fn award_type(&self) -> GoalAwardType {
        self.award_type
    }
}

/// Typed payload for `YellowCardRecorded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct YellowCardRecordedPayload {
    player_id: PlayerId,
}

impl YellowCardRecordedPayload {
    pub const fn new(player_id: PlayerId) -> Self {
        Self { player_id }
    }

    pub const fn player_id(self) -> PlayerId {
        self.player_id
    }
}

/// Typed payload for `SecondYellowCardRecorded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SecondYellowCardRecordedPayload {
    player_id: PlayerId,
}

impl SecondYellowCardRecordedPayload {
    pub const fn new(player_id: PlayerId) -> Self {
        Self { player_id }
    }

    pub const fn player_id(self) -> PlayerId {
        self.player_id
    }
}

/// Typed payload for `RedCardRecorded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RedCardRecordedPayload {
    player_id: PlayerId,
}

impl RedCardRecordedPayload {
    pub const fn new(player_id: PlayerId) -> Self {
        Self { player_id }
    }

    pub const fn player_id(self) -> PlayerId {
        self.player_id
    }
}

/// Typed payload for `SubstitutionRecorded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubstitutionRecordedPayload {
    team_id: TeamId,
    player_on_id: PlayerId,
    player_off_id: PlayerId,
}

impl SubstitutionRecordedPayload {
    pub fn try_new(
        team_id: TeamId,
        player_on_id: PlayerId,
        player_off_id: PlayerId,
    ) -> Result<Self, SubstitutionPayloadError> {
        if player_on_id == player_off_id {
            return Err(SubstitutionPayloadError::SamePlayer);
        }

        Ok(Self {
            team_id,
            player_on_id,
            player_off_id,
        })
    }

    pub const fn team_id(self) -> TeamId {
        self.team_id
    }

    pub const fn player_on_id(self) -> PlayerId {
        self.player_on_id
    }

    pub const fn player_off_id(self) -> PlayerId {
        self.player_off_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstitutionPayloadError {
    SamePlayer,
}

impl fmt::Display for SubstitutionPayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SamePlayer => formatter.write_str("substitution players must be different"),
        }
    }
}

impl std::error::Error for SubstitutionPayloadError {}

/// The typed payload union for the first C1 live-action event set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C1EventPayload {
    GoalRecorded(GoalRecordedPayload),
    YellowCardRecorded(YellowCardRecordedPayload),
    SecondYellowCardRecorded(SecondYellowCardRecordedPayload),
    RedCardRecorded(RedCardRecordedPayload),
    SubstitutionRecorded(SubstitutionRecordedPayload),
}

impl C1EventPayload {
    pub const fn event_type(&self) -> C1EventType {
        match self {
            Self::GoalRecorded(_) => C1EventType::GoalRecorded,
            Self::YellowCardRecorded(_) => C1EventType::YellowCardRecorded,
            Self::SecondYellowCardRecorded(_) => C1EventType::SecondYellowCardRecorded,
            Self::RedCardRecorded(_) => C1EventType::RedCardRecorded,
            Self::SubstitutionRecorded(_) => C1EventType::SubstitutionRecorded,
        }
    }

    pub const fn schema_version(&self) -> EventSchemaVersion {
        self.event_type().schema_version()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        C1EventPayload, C1EventType, GoalAwardType, GoalRecordedPayload, RedCardRecordedPayload,
        SecondYellowCardRecordedPayload, SubstitutionPayloadError, SubstitutionRecordedPayload,
        YellowCardRecordedPayload,
    };
    use crate::{EventSchemaVersion, PlayerId, TeamId};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("C1 event fixture UUID should be valid")
    }

    fn team_id() -> TeamId {
        TeamId::from_uuid(uuid("019fe2d0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn player_id(value: &str) -> PlayerId {
        PlayerId::from_uuid(uuid(value))
    }

    #[test]
    fn c1_event_names_and_schema_versions_are_stable() {
        let names: Vec<_> = C1EventType::ALL
            .into_iter()
            .map(|event_type| {
                (
                    event_type.as_str(),
                    event_type.try_as_event_type().unwrap().as_str().to_owned(),
                    event_type.schema_version(),
                )
            })
            .collect();

        assert_eq!(
            names,
            vec![
                (
                    "GoalRecorded",
                    "GoalRecorded".to_owned(),
                    EventSchemaVersion::initial()
                ),
                (
                    "YellowCardRecorded",
                    "YellowCardRecorded".to_owned(),
                    EventSchemaVersion::initial()
                ),
                (
                    "SecondYellowCardRecorded",
                    "SecondYellowCardRecorded".to_owned(),
                    EventSchemaVersion::initial()
                ),
                (
                    "RedCardRecorded",
                    "RedCardRecorded".to_owned(),
                    EventSchemaVersion::initial()
                ),
                (
                    "SubstitutionRecorded",
                    "SubstitutionRecorded".to_owned(),
                    EventSchemaVersion::initial()
                ),
            ]
        );
    }

    #[test]
    fn goal_payload_preserves_award_type_team_and_optional_scorer() {
        let payload = GoalRecordedPayload::new(
            team_id(),
            Some(player_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930")),
            GoalAwardType::OwnGoal,
        );

        assert_eq!(payload.team_id(), team_id());
        assert_eq!(
            payload.scorer_player_id(),
            Some(player_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930"))
        );
        assert_eq!(payload.award_type(), GoalAwardType::OwnGoal);
        assert_eq!(GoalAwardType::Penalty.as_str(), "penalty");
    }

    #[test]
    fn card_payloads_keep_their_event_meanings_distinct() {
        let player_id = player_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930");
        let yellow = C1EventPayload::YellowCardRecorded(YellowCardRecordedPayload::new(player_id));
        let second_yellow = C1EventPayload::SecondYellowCardRecorded(
            SecondYellowCardRecordedPayload::new(player_id),
        );
        let direct_red = C1EventPayload::RedCardRecorded(RedCardRecordedPayload::new(player_id));

        assert_eq!(yellow.event_type(), C1EventType::YellowCardRecorded);
        assert_eq!(
            second_yellow.event_type(),
            C1EventType::SecondYellowCardRecorded
        );
        assert_eq!(direct_red.event_type(), C1EventType::RedCardRecorded);
        assert_eq!(
            second_yellow.schema_version(),
            EventSchemaVersion::initial()
        );
    }

    #[test]
    fn substitution_rejects_using_the_same_player_on_both_sides() {
        let player_id = player_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930");

        assert_eq!(
            SubstitutionRecordedPayload::try_new(team_id(), player_id, player_id),
            Err(SubstitutionPayloadError::SamePlayer)
        );
    }

    #[test]
    fn substitution_payload_keeps_team_and_direction() {
        let player_on = player_id("019fe2d0-3bc5-7cf1-b3d2-0728c077a930");
        let player_off = player_id("019fe2d0-3bc5-7cf2-b3d2-0728c077a930");
        let payload = SubstitutionRecordedPayload::try_new(team_id(), player_on, player_off)
            .expect("different players should form a substitution");

        assert_eq!(payload.team_id(), team_id());
        assert_eq!(payload.player_on_id(), player_on);
        assert_eq!(payload.player_off_id(), player_off);
        assert_eq!(
            C1EventPayload::SubstitutionRecorded(payload).event_type(),
            C1EventType::SubstitutionRecorded
        );
    }
}
