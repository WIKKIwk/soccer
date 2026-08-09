use crate::directory::TeamId;
use std::fmt;
use uuid::Uuid;

/// Canonical identity of a real-world person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PersonId(Uuid);

impl PersonId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for PersonId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<PersonId> for Uuid {
    fn from(value: PersonId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for PersonId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The source-backed name attached to a person identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonName(String);

impl PersonName {
    pub fn try_new(value: impl Into<String>) -> Result<Self, PersonNameError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PersonNameError::Empty);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PersonName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonNameError {
    Empty,
}

impl fmt::Display for PersonNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("person name must not be empty"),
        }
    }
}

impl std::error::Error for PersonNameError {}

/// Minimal real-world person identity.
///
/// Player profile, team membership, localized names, aliases, and biographical
/// attributes are separate concerns and are deliberately not inferred here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    id: PersonId,
    name: PersonName,
}

impl Person {
    pub const fn new(id: PersonId, name: PersonName) -> Self {
        Self { id, name }
    }

    pub const fn id(&self) -> PersonId {
        self.id
    }

    pub fn name(&self) -> &PersonName {
        &self.name
    }
}

/// Canonical identity of a football participation profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerId(Uuid);

impl PlayerId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for PlayerId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<PlayerId> for Uuid {
    fn from(value: PlayerId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Minimal football player profile attached to one person.
///
/// Team assignment, squad membership, competition registration, and match
/// eligibility are intentionally deferred to later directory increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Player {
    id: PlayerId,
    person_id: PersonId,
}

impl Player {
    pub const fn new(id: PlayerId, person_id: PersonId) -> Self {
        Self { id, person_id }
    }

    pub const fn id(self) -> PlayerId {
        self.id
    }

    pub const fn person_id(self) -> PersonId {
        self.person_id
    }
}

/// The explicit current team assignment used by the first player registry.
///
/// Historical squad intervals and competition registration are different
/// concepts and are deliberately deferred to later directory increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TeamPlayerAssignment {
    team_id: TeamId,
    player_id: PlayerId,
}

impl TeamPlayerAssignment {
    pub const fn new(team_id: TeamId, player_id: PlayerId) -> Self {
        Self { team_id, player_id }
    }

    pub const fn team_id(self) -> TeamId {
        self.team_id
    }

    pub const fn player_id(self) -> PlayerId {
        self.player_id
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        Person, PersonId, PersonName, PersonNameError, Player, PlayerId, TeamPlayerAssignment,
    };
    use crate::TeamId;

    fn person_id() -> PersonId {
        PersonId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    fn player_id() -> PlayerId {
        PlayerId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf1-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn person_preserves_typed_identity_and_source_name() {
        let name = PersonName::try_new(" Otabek Shukurov ").expect("non-empty person name");
        let person = Person::new(person_id(), name.clone());

        assert_eq!(person.id(), person_id());
        assert_eq!(person.name(), &name);
        assert_eq!(person.name().as_str(), " Otabek Shukurov ");
    }

    #[test]
    fn person_and_player_ids_round_trip_as_canonical_uuids() {
        assert_eq!(Uuid::from(person_id()), person_id().as_uuid());
        assert_eq!(Uuid::from(player_id()), player_id().as_uuid());
    }

    #[test]
    fn person_name_rejects_only_whitespace() {
        assert_eq!(PersonName::try_new(" \t\n"), Err(PersonNameError::Empty));
    }

    #[test]
    fn player_profile_keeps_person_ownership_explicit() {
        let player = Player::new(player_id(), person_id());

        assert_eq!(player.id(), player_id());
        assert_eq!(player.person_id(), person_id());
    }

    #[test]
    fn team_player_assignment_keeps_both_typed_owners_explicit() {
        let team_id = TeamId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf2-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        );
        let assignment = TeamPlayerAssignment::new(team_id, player_id());

        assert_eq!(assignment.team_id(), team_id);
        assert_eq!(assignment.player_id(), player_id());
    }
}
