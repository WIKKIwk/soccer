use football_domain::{Team, TeamId, TeamName, TeamNameError};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use uuid::Uuid;

#[derive(Debug)]
pub enum TeamRepositoryError {
    Database(SqlxError),
    InvalidStoredName(TeamNameError),
    SameTeamId,
    ExistingConflict { id: TeamId },
}

impl fmt::Display for TeamRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "team database operation failed: {error}"),
            Self::InvalidStoredName(error) => {
                write!(formatter, "stored team name is invalid: {error}")
            }
            Self::SameTeamId => formatter.write_str("the two teams must have different IDs"),
            Self::ExistingConflict { id } => {
                write!(formatter, "team {id} already exists with a different name")
            }
        }
    }
}

impl std::error::Error for TeamRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredName(error) => Some(error),
            Self::SameTeamId | Self::ExistingConflict { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct TeamRow {
    id: Uuid,
    name: String,
}

impl TryFrom<TeamRow> for Team {
    type Error = TeamRepositoryError;

    fn try_from(row: TeamRow) -> Result<Self, Self::Error> {
        let name = TeamName::try_new(row.name).map_err(TeamRepositoryError::InvalidStoredName)?;
        Ok(Self::new(TeamId::from_uuid(row.id), name))
    }
}

pub async fn persist_team(pool: &PgPool, team: &Team) -> Result<Team, TeamRepositoryError> {
    let row = sqlx::query_as::<_, TeamRow>(
        r#"
        INSERT INTO teams (id, name)
        VALUES ($1, $2)
        RETURNING id, name
        "#,
    )
    .bind(team.id().as_uuid())
    .bind(team.name().as_str())
    .fetch_one(pool)
    .await
    .map_err(TeamRepositoryError::Database)?;

    row.try_into()
}

pub async fn persist_two_teams(
    pool: &PgPool,
    first: &Team,
    second: &Team,
) -> Result<(Team, Team), TeamRepositoryError> {
    if first.id() == second.id() {
        return Err(TeamRepositoryError::SameTeamId);
    }

    let mut transaction = pool.begin().await.map_err(TeamRepositoryError::Database)?;
    let mut persisted = Vec::with_capacity(2);

    for team in [first, second] {
        let existing = sqlx::query_as::<_, TeamRow>(
            r#"
            SELECT id, name
            FROM teams
            WHERE id = $1
            "#,
        )
        .bind(team.id().as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(TeamRepositoryError::Database)?;

        let persisted_team = match existing {
            Some(row) => {
                let existing = row.try_into()?;
                if existing != *team {
                    return Err(TeamRepositoryError::ExistingConflict { id: team.id() });
                }
                existing
            }
            None => {
                let row = sqlx::query_as::<_, TeamRow>(
                    r#"
                    INSERT INTO teams (id, name)
                    VALUES ($1, $2)
                    RETURNING id, name
                    "#,
                )
                .bind(team.id().as_uuid())
                .bind(team.name().as_str())
                .fetch_one(&mut *transaction)
                .await
                .map_err(TeamRepositoryError::Database)?;
                row.try_into()?
            }
        };

        persisted.push(persisted_team);
    }

    transaction
        .commit()
        .await
        .map_err(TeamRepositoryError::Database)?;

    let mut persisted = persisted.into_iter();
    let first = persisted
        .next()
        .expect("two-team persistence should return the first team");
    let second = persisted
        .next()
        .expect("two-team persistence should return the second team");
    Ok((first, second))
}

pub async fn load_team(pool: &PgPool, id: TeamId) -> Result<Option<Team>, TeamRepositoryError> {
    let row = sqlx::query_as::<_, TeamRow>(
        r#"
        SELECT id, name
        FROM teams
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(TeamRepositoryError::Database)?;

    row.map(TryInto::try_into).transpose()
}
