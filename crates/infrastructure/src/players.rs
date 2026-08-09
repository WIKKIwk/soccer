use football_domain::{
    Person, PersonId, PersonName, PersonNameError, Player, PlayerId, TeamId, TeamPlayerAssignment,
};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use uuid::Uuid;

#[derive(Debug)]
pub enum PlayerRepositoryError {
    Database(SqlxError),
    InvalidStoredPersonName(PersonNameError),
    TeamNotFound {
        id: TeamId,
    },
    ExistingPersonConflict {
        id: PersonId,
    },
    ExistingPlayerConflict {
        id: PlayerId,
    },
    ExistingTeamAssignmentConflict {
        player_id: PlayerId,
        team_id: TeamId,
    },
}

impl fmt::Display for PlayerRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "player database operation failed: {error}"),
            Self::InvalidStoredPersonName(error) => {
                write!(formatter, "stored person name is invalid: {error}")
            }
            Self::TeamNotFound { id } => write!(formatter, "team {id} does not exist"),
            Self::ExistingPersonConflict { id } => {
                write!(
                    formatter,
                    "person {id} already exists with a different name"
                )
            }
            Self::ExistingPlayerConflict { id } => {
                write!(
                    formatter,
                    "player {id} already exists for a different person"
                )
            }
            Self::ExistingTeamAssignmentConflict { player_id, team_id } => write!(
                formatter,
                "player {player_id} is already assigned to a different team than {team_id}"
            ),
        }
    }
}

impl std::error::Error for PlayerRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredPersonName(error) => Some(error),
            Self::TeamNotFound { .. }
            | Self::ExistingPersonConflict { .. }
            | Self::ExistingPlayerConflict { .. }
            | Self::ExistingTeamAssignmentConflict { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedPlayerAssignment {
    person: Person,
    player: Player,
    assignment: TeamPlayerAssignment,
}

impl PersistedPlayerAssignment {
    pub fn person(&self) -> &Person {
        &self.person
    }

    pub const fn player(&self) -> Player {
        self.player
    }

    pub const fn assignment(&self) -> TeamPlayerAssignment {
        self.assignment
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PersonRow {
    id: Uuid,
    name: String,
}

impl TryFrom<PersonRow> for Person {
    type Error = PlayerRepositoryError;

    fn try_from(row: PersonRow) -> Result<Self, Self::Error> {
        let name = PersonName::try_new(row.name)
            .map_err(PlayerRepositoryError::InvalidStoredPersonName)?;
        Ok(Self::new(PersonId::from_uuid(row.id), name))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PlayerRow {
    id: Uuid,
    person_id: Uuid,
}

impl From<PlayerRow> for Player {
    fn from(row: PlayerRow) -> Self {
        Self::new(
            PlayerId::from_uuid(row.id),
            PersonId::from_uuid(row.person_id),
        )
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PlayerAssignmentRow {
    person_id: Uuid,
    person_name: String,
    player_id: Uuid,
    player_person_id: Uuid,
    team_id: Uuid,
}

impl TryFrom<PlayerAssignmentRow> for PersistedPlayerAssignment {
    type Error = PlayerRepositoryError;

    fn try_from(row: PlayerAssignmentRow) -> Result<Self, Self::Error> {
        let person = Person::new(
            PersonId::from_uuid(row.person_id),
            PersonName::try_new(row.person_name)
                .map_err(PlayerRepositoryError::InvalidStoredPersonName)?,
        );
        let player = Player::new(
            PlayerId::from_uuid(row.player_id),
            PersonId::from_uuid(row.player_person_id),
        );
        let assignment = TeamPlayerAssignment::new(TeamId::from_uuid(row.team_id), player.id());

        Ok(Self {
            person,
            player,
            assignment,
        })
    }
}

pub async fn persist_player_for_team(
    pool: &PgPool,
    person: &Person,
    player: Player,
    team_id: TeamId,
) -> Result<PersistedPlayerAssignment, PlayerRepositoryError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(PlayerRepositoryError::Database)?;

    let team_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM teams WHERE id = $1)")
            .bind(team_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .map_err(PlayerRepositoryError::Database)?;
    if !team_exists {
        return Err(PlayerRepositoryError::TeamNotFound { id: team_id });
    }

    let persisted_person =
        match sqlx::query_as::<_, PersonRow>("SELECT id, name FROM persons WHERE id = $1")
            .bind(person.id().as_uuid())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(PlayerRepositoryError::Database)?
        {
            Some(row) => {
                let existing = row.try_into()?;
                if existing != *person {
                    return Err(PlayerRepositoryError::ExistingPersonConflict { id: person.id() });
                }
                existing
            }
            None => {
                let row = sqlx::query_as::<_, PersonRow>(
                    r#"
                INSERT INTO persons (id, name)
                VALUES ($1, $2)
                RETURNING id, name
                "#,
                )
                .bind(person.id().as_uuid())
                .bind(person.name().as_str())
                .fetch_one(&mut *transaction)
                .await
                .map_err(PlayerRepositoryError::Database)?;
                row.try_into()?
            }
        };

    let persisted_player =
        match sqlx::query_as::<_, PlayerRow>("SELECT id, person_id FROM players WHERE id = $1")
            .bind(player.id().as_uuid())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(PlayerRepositoryError::Database)?
        {
            Some(row) => {
                let existing = Player::from(row);
                if existing != player {
                    return Err(PlayerRepositoryError::ExistingPlayerConflict { id: player.id() });
                }
                existing
            }
            None => {
                let row = sqlx::query_as::<_, PlayerRow>(
                    r#"
                INSERT INTO players (id, person_id)
                VALUES ($1, $2)
                RETURNING id, person_id
                "#,
                )
                .bind(player.id().as_uuid())
                .bind(player.person_id().as_uuid())
                .fetch_one(&mut *transaction)
                .await
                .map_err(PlayerRepositoryError::Database)?;
                Player::from(row)
            }
        };

    let existing_team = sqlx::query_scalar::<_, Uuid>(
        "SELECT team_id FROM team_player_assignments WHERE player_id = $1",
    )
    .bind(player.id().as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(PlayerRepositoryError::Database)?;

    match existing_team.map(TeamId::from_uuid) {
        Some(existing_team) if existing_team != team_id => {
            return Err(PlayerRepositoryError::ExistingTeamAssignmentConflict {
                player_id: player.id(),
                team_id: existing_team,
            });
        }
        Some(_) => {}
        None => {
            sqlx::query("INSERT INTO team_player_assignments (player_id, team_id) VALUES ($1, $2)")
                .bind(player.id().as_uuid())
                .bind(team_id.as_uuid())
                .execute(&mut *transaction)
                .await
                .map_err(PlayerRepositoryError::Database)?;
        }
    }

    transaction
        .commit()
        .await
        .map_err(PlayerRepositoryError::Database)?;

    Ok(PersistedPlayerAssignment {
        person: persisted_person,
        player: persisted_player,
        assignment: TeamPlayerAssignment::new(team_id, player.id()),
    })
}

pub async fn load_player_assignment(
    pool: &PgPool,
    player_id: PlayerId,
    team_id: TeamId,
) -> Result<Option<PersistedPlayerAssignment>, PlayerRepositoryError> {
    let row = sqlx::query_as::<_, PlayerAssignmentRow>(
        r#"
        SELECT
            persons.id AS person_id,
            persons.name AS person_name,
            players.id AS player_id,
            players.person_id AS player_person_id,
            team_player_assignments.team_id AS team_id
        FROM team_player_assignments
        INNER JOIN players ON players.id = team_player_assignments.player_id
        INNER JOIN persons ON persons.id = players.person_id
        WHERE team_player_assignments.player_id = $1
          AND team_player_assignments.team_id = $2
        "#,
    )
    .bind(player_id.as_uuid())
    .bind(team_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(PlayerRepositoryError::Database)?;

    row.map(TryInto::try_into).transpose()
}
