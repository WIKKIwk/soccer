use std::fmt;
use uuid::Uuid;

/// Canonical identity of a sporting participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TeamId(Uuid);

impl TeamId {
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl From<Uuid> for TeamId {
    fn from(value: Uuid) -> Self {
        Self::from_uuid(value)
    }
}

impl From<TeamId> for Uuid {
    fn from(value: TeamId) -> Self {
        value.as_uuid()
    }
}

impl fmt::Display for TeamId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamName(String);

impl TeamName {
    pub fn try_new(value: impl Into<String>) -> Result<Self, TeamNameError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TeamNameError::Empty);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TeamName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamNameError {
    Empty,
}

impl fmt::Display for TeamNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("team name must not be empty"),
        }
    }
}

impl std::error::Error for TeamNameError {}

/// Minimal directory identity for a sporting participant.
///
/// Club relationships, localized names, status, and squad history are
/// deliberately outside this first directory increment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    id: TeamId,
    name: TeamName,
}

impl Team {
    pub const fn new(id: TeamId, name: TeamName) -> Self {
        Self { id, name }
    }

    pub const fn id(&self) -> TeamId {
        self.id
    }

    pub fn name(&self) -> &TeamName {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{Team, TeamId, TeamName, TeamNameError};

    fn team_id() -> TeamId {
        TeamId::from_uuid(
            Uuid::parse_str("019fe250-3bc5-7cf0-b3d2-0728c077a930")
                .expect("fixture UUID should be valid"),
        )
    }

    #[test]
    fn team_preserves_typed_identity_and_source_name() {
        let name = TeamName::try_new(" FC Bunyodkor ").expect("non-empty team name");
        let team = Team::new(team_id(), name.clone());

        assert_eq!(team.id(), team_id());
        assert_eq!(team.name(), &name);
        assert_eq!(team.name().as_str(), " FC Bunyodkor ");
    }

    #[test]
    fn team_id_round_trips_as_a_canonical_uuid() {
        let id = team_id();
        assert_eq!(Uuid::from(id).to_string(), id.to_string());
    }

    #[test]
    fn team_name_rejects_only_whitespace() {
        assert_eq!(TeamName::try_new(" \t\n"), Err(TeamNameError::Empty));
    }
}
