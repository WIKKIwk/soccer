use crate::{FixtureId, PlayerId, TeamId};
use std::collections::HashSet;
use std::fmt;
use uuid::Uuid;

/// The two match-specific lineup roles in the first C1 slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineupRole {
    Starter,
    Substitute,
}

impl LineupRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Starter => "starter",
            Self::Substitute => "substitute",
        }
    }

    pub fn try_from_persisted_str(value: &str) -> Option<Self> {
        match value {
            "starter" => Some(Self::Starter),
            "substitute" => Some(Self::Substitute),
            _ => None,
        }
    }
}

impl fmt::Display for LineupRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A match-specific nomination of one player and their lineup role.
///
/// Collection-level uniqueness and team ownership are validated by later
/// lineup increments; this value only defines the entry vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineupEntry {
    player_id: PlayerId,
    role: LineupRole,
}

impl LineupEntry {
    pub const fn new(player_id: PlayerId, role: LineupRole) -> Self {
        Self { player_id, role }
    }

    pub const fn starter(player_id: PlayerId) -> Self {
        Self::new(player_id, LineupRole::Starter)
    }

    pub const fn substitute(player_id: PlayerId) -> Self {
        Self::new(player_id, LineupRole::Substitute)
    }

    pub const fn player_id(self) -> PlayerId {
        self.player_id
    }

    pub const fn role(self) -> LineupRole {
        self.role
    }
}

/// Validates the player-identity invariant for one lineup collection.
pub fn validate_unique_players(entries: &[LineupEntry]) -> Result<(), LineupValidationError> {
    let mut player_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        if !player_ids.insert(entry.player_id()) {
            return Err(LineupValidationError::DuplicatePlayer {
                player_id: entry.player_id(),
            });
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineupValidationError {
    DuplicatePlayer { player_id: PlayerId },
}

impl fmt::Display for LineupValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePlayer { player_id } => {
                write!(
                    formatter,
                    "player {player_id} cannot appear twice in one lineup"
                )
            }
        }
    }
}

impl std::error::Error for LineupValidationError {}

/// Canonical identity of a match-specific lineup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineupId(Uuid);

impl LineupId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for LineupId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<LineupId> for Uuid {
    fn from(value: LineupId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for LineupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A draft lineup owned by one fixture side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftLineup {
    id: LineupId,
    fixture_id: FixtureId,
    team_id: TeamId,
    entries: Vec<LineupEntry>,
}

impl DraftLineup {
    pub fn try_new(
        id: LineupId,
        fixture_id: FixtureId,
        team_id: TeamId,
        entries: impl Into<Vec<LineupEntry>>,
    ) -> Result<Self, LineupValidationError> {
        let entries = entries.into();
        validate_unique_players(&entries)?;

        Ok(Self {
            id,
            fixture_id,
            team_id,
            entries,
        })
    }

    pub const fn id(&self) -> LineupId {
        self.id
    }

    pub const fn fixture_id(&self) -> FixtureId {
        self.fixture_id
    }

    pub const fn team_id(&self) -> TeamId {
        self.team_id
    }

    pub fn entries(&self) -> &[LineupEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::PlayerId;

    use super::{
        DraftLineup, LineupEntry, LineupId, LineupRole, LineupValidationError,
        validate_unique_players,
    };

    fn player_id() -> PlayerId {
        PlayerId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn starter_entry_keeps_typed_player_identity() {
        let entry = LineupEntry::starter(player_id());

        assert_eq!(entry.player_id(), player_id());
        assert_eq!(entry.role(), LineupRole::Starter);
        assert_eq!(entry.role().to_string(), "starter");
    }

    #[test]
    fn substitute_entry_keeps_typed_player_identity() {
        let entry = LineupEntry::substitute(player_id());

        assert_eq!(entry.player_id(), player_id());
        assert_eq!(entry.role(), LineupRole::Substitute);
        assert_eq!(entry.role().to_string(), "substitute");
    }

    #[test]
    fn starter_and_substitute_roles_remain_distinct() {
        assert_ne!(LineupRole::Starter, LineupRole::Substitute);
        assert_eq!(
            LineupRole::try_from_persisted_str(LineupRole::Starter.as_str()),
            Some(LineupRole::Starter)
        );
        assert_eq!(LineupRole::try_from_persisted_str("reserve"), None);
    }

    #[test]
    fn accepts_distinct_players_across_starter_and_substitute_entries() {
        let second_player = PlayerId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        );

        assert_eq!(
            validate_unique_players(&[
                LineupEntry::starter(player_id()),
                LineupEntry::substitute(second_player),
            ]),
            Ok(())
        );
    }

    #[test]
    fn rejects_the_same_player_even_when_roles_differ() {
        assert_eq!(
            validate_unique_players(&[
                LineupEntry::starter(player_id()),
                LineupEntry::substitute(player_id()),
            ]),
            Err(LineupValidationError::DuplicatePlayer {
                player_id: player_id(),
            })
        );
    }

    #[test]
    fn draft_lineup_keeps_fixture_team_and_entries_typed() {
        let fixture_id = crate::FixtureId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        );
        let team_id = crate::TeamId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf2-b3d2-0728c077a930")
                .expect("team UUID should be valid"),
        );
        let lineup_id = LineupId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf3-b3d2-0728c077a930")
                .expect("lineup UUID should be valid"),
        );
        let lineup = DraftLineup::try_new(
            lineup_id,
            fixture_id,
            team_id,
            vec![LineupEntry::starter(player_id())],
        )
        .expect("distinct draft entries should be accepted");

        assert_eq!(lineup.id(), lineup_id);
        assert_eq!(lineup.fixture_id(), fixture_id);
        assert_eq!(lineup.team_id(), team_id);
        assert_eq!(lineup.entries(), &[LineupEntry::starter(player_id())]);
    }

    #[test]
    fn draft_lineup_reuses_duplicate_player_validation() {
        let fixture_id = crate::FixtureId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        );
        let team_id = crate::TeamId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf2-b3d2-0728c077a930")
                .expect("team UUID should be valid"),
        );
        let lineup_id = LineupId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf3-b3d2-0728c077a930")
                .expect("lineup UUID should be valid"),
        );

        assert_eq!(
            DraftLineup::try_new(
                lineup_id,
                fixture_id,
                team_id,
                vec![
                    LineupEntry::starter(player_id()),
                    LineupEntry::substitute(player_id())
                ],
            ),
            Err(LineupValidationError::DuplicatePlayer {
                player_id: player_id(),
            })
        );
    }
}
