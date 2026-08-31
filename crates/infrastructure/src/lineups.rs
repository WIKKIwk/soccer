use football_domain::{
    DraftLineup, Fixture, FixtureId, LineupEntry, LineupId, LineupRole, LineupValidationError,
    PlayerId, TeamId,
};
use sqlx::{Error as SqlxError, PgPool};
use std::{collections::HashSet, fmt};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub enum LineupRepositoryError {
    Database(SqlxError),
    InvalidStoredStatus(String),
    InvalidStoredRole(String),
    InvalidStoredLineup(LineupValidationError),
    FixtureTeamMismatch {
        fixture_id: FixtureId,
        team_id: TeamId,
    },
    LineupFixtureMismatch {
        lineup_id: LineupId,
        expected: FixtureId,
        actual: FixtureId,
    },
    ExistingFixtureConflict {
        id: FixtureId,
    },
    ExistingLineupConflict {
        id: LineupId,
    },
    ExistingFixtureTeamLineupConflict {
        fixture_id: FixtureId,
        team_id: TeamId,
        existing_id: LineupId,
    },
    PlayerNotAssignedToTeam {
        player_id: PlayerId,
        team_id: TeamId,
    },
}

impl fmt::Display for LineupRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "lineup database operation failed: {error}"),
            Self::InvalidStoredStatus(status) => {
                write!(formatter, "stored lineup status is invalid: {status}")
            }
            Self::InvalidStoredRole(role) => {
                write!(formatter, "stored lineup role is invalid: {role}")
            }
            Self::InvalidStoredLineup(error) => {
                write!(formatter, "stored lineup is invalid: {error}")
            }
            Self::FixtureTeamMismatch {
                fixture_id,
                team_id,
            } => write!(
                formatter,
                "team {team_id} is not a home or away side of fixture {fixture_id}"
            ),
            Self::LineupFixtureMismatch {
                lineup_id,
                expected,
                actual,
            } => write!(
                formatter,
                "lineup {lineup_id} belongs to fixture {actual}, expected fixture {expected}"
            ),
            Self::ExistingFixtureConflict { id } => {
                write!(formatter, "fixture {id} already exists with different data")
            }
            Self::ExistingLineupConflict { id } => {
                write!(formatter, "lineup {id} already exists with different data")
            }
            Self::ExistingFixtureTeamLineupConflict {
                fixture_id,
                team_id,
                existing_id,
            } => write!(
                formatter,
                "fixture {fixture_id} already has lineup {existing_id} for team {team_id}"
            ),
            Self::PlayerNotAssignedToTeam { player_id, team_id } => write!(
                formatter,
                "player {player_id} is not assigned to team {team_id}"
            ),
        }
    }
}

impl std::error::Error for LineupRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredLineup(error) => Some(error),
            Self::InvalidStoredStatus(_)
            | Self::InvalidStoredRole(_)
            | Self::FixtureTeamMismatch { .. }
            | Self::LineupFixtureMismatch { .. }
            | Self::ExistingFixtureConflict { .. }
            | Self::ExistingLineupConflict { .. }
            | Self::ExistingFixtureTeamLineupConflict { .. }
            | Self::PlayerNotAssignedToTeam { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct FixtureRow {
    id: Uuid,
    competition_id: Uuid,
    season_id: Uuid,
    home_team_id: Uuid,
    away_team_id: Uuid,
    kickoff_at: OffsetDateTime,
}

#[derive(Debug, sqlx::FromRow)]
struct LineupRow {
    id: Uuid,
    fixture_id: Uuid,
    team_id: Uuid,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LineupEntryRow {
    player_id: Uuid,
    role: String,
}

fn fixture_matches(row: &FixtureRow, fixture: &Fixture) -> bool {
    row.id == fixture.id().as_uuid()
        && row.competition_id == fixture.competition_id().as_uuid()
        && row.season_id == fixture.season_id().as_uuid()
        && row.home_team_id == fixture.home_team_id().as_uuid()
        && row.away_team_id == fixture.away_team_id().as_uuid()
        && row.kickoff_at.unix_timestamp_nanos()
            == fixture
                .kickoff()
                .as_offset_datetime()
                .unix_timestamp_nanos()
}

fn parse_role(role: String) -> Result<LineupRole, LineupRepositoryError> {
    match role.as_str() {
        "starter" => Ok(LineupRole::Starter),
        "substitute" => Ok(LineupRole::Substitute),
        _ => Err(LineupRepositoryError::InvalidStoredRole(role)),
    }
}

fn build_draft_lineup(
    row: LineupRow,
    entries: Vec<LineupEntryRow>,
) -> Result<DraftLineup, LineupRepositoryError> {
    if row.status != "draft" {
        return Err(LineupRepositoryError::InvalidStoredStatus(row.status));
    }

    let entries = entries
        .into_iter()
        .map(|entry| {
            Ok(LineupEntry::new(
                PlayerId::from_uuid(entry.player_id),
                parse_role(entry.role)?,
            ))
        })
        .collect::<Result<Vec<_>, LineupRepositoryError>>()?;

    DraftLineup::try_new(
        LineupId::from_uuid(row.id),
        FixtureId::from_uuid(row.fixture_id),
        TeamId::from_uuid(row.team_id),
        entries,
    )
    .map_err(LineupRepositoryError::InvalidStoredLineup)
}

fn lineups_match(existing: &DraftLineup, requested: &DraftLineup) -> bool {
    existing.id() == requested.id()
        && existing.fixture_id() == requested.fixture_id()
        && existing.team_id() == requested.team_id()
        && existing.entries().iter().copied().collect::<HashSet<_>>()
            == requested.entries().iter().copied().collect::<HashSet<_>>()
}

async fn load_entries(
    pool: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    lineup_id: LineupId,
) -> Result<Vec<LineupEntryRow>, LineupRepositoryError> {
    sqlx::query_as::<_, LineupEntryRow>(
        r#"
        SELECT player_id, role
        FROM lineup_entries
        WHERE lineup_id = $1
        ORDER BY player_id
        "#,
    )
    .bind(lineup_id.as_uuid())
    .fetch_all(&mut **pool)
    .await
    .map_err(LineupRepositoryError::Database)
}

async fn load_lineup_row(
    pool: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    lineup_id: LineupId,
) -> Result<Option<LineupRow>, LineupRepositoryError> {
    sqlx::query_as::<_, LineupRow>(
        r#"
        SELECT id, fixture_id, team_id, status
        FROM lineups
        WHERE id = $1
        "#,
    )
    .bind(lineup_id.as_uuid())
    .fetch_optional(&mut **pool)
    .await
    .map_err(LineupRepositoryError::Database)
}

pub async fn persist_draft_lineup(
    pool: &PgPool,
    fixture: &Fixture,
    lineup: &DraftLineup,
) -> Result<DraftLineup, LineupRepositoryError> {
    if lineup.fixture_id() != fixture.id() {
        return Err(LineupRepositoryError::LineupFixtureMismatch {
            lineup_id: lineup.id(),
            expected: fixture.id(),
            actual: lineup.fixture_id(),
        });
    }
    if !fixture.includes_team(lineup.team_id()) {
        return Err(LineupRepositoryError::FixtureTeamMismatch {
            fixture_id: fixture.id(),
            team_id: lineup.team_id(),
        });
    }

    let mut transaction = pool
        .begin()
        .await
        .map_err(LineupRepositoryError::Database)?;

    let existing_fixture = sqlx::query_as::<_, FixtureRow>(
        r#"
        SELECT id, competition_id, season_id, home_team_id, away_team_id, kickoff_at
        FROM fixtures
        WHERE id = $1
        "#,
    )
    .bind(fixture.id().as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(LineupRepositoryError::Database)?;

    match existing_fixture {
        Some(row) if !fixture_matches(&row, fixture) => {
            return Err(LineupRepositoryError::ExistingFixtureConflict { id: fixture.id() });
        }
        Some(_) => {}
        None => {
            sqlx::query(
                r#"
                INSERT INTO fixtures (
                    id, competition_id, season_id, home_team_id, away_team_id, kickoff_at
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(fixture.id().as_uuid())
            .bind(fixture.competition_id().as_uuid())
            .bind(fixture.season_id().as_uuid())
            .bind(fixture.home_team_id().as_uuid())
            .bind(fixture.away_team_id().as_uuid())
            .bind(fixture.kickoff().as_offset_datetime())
            .execute(&mut *transaction)
            .await
            .map_err(LineupRepositoryError::Database)?;
        }
    }

    if let Some(row) = load_lineup_row(&mut transaction, lineup.id()).await? {
        let entries = load_entries(&mut transaction, lineup.id()).await?;
        let existing = build_draft_lineup(row, entries)?;

        if !lineups_match(&existing, lineup) {
            return Err(LineupRepositoryError::ExistingLineupConflict { id: lineup.id() });
        }

        transaction
            .commit()
            .await
            .map_err(LineupRepositoryError::Database)?;
        return Ok(lineup.clone());
    }

    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM lineups WHERE fixture_id = $1 AND team_id = $2",
    )
    .bind(fixture.id().as_uuid())
    .bind(lineup.team_id().as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(LineupRepositoryError::Database)?
    {
        return Err(LineupRepositoryError::ExistingFixtureTeamLineupConflict {
            fixture_id: fixture.id(),
            team_id: lineup.team_id(),
            existing_id: LineupId::from_uuid(existing_id),
        });
    }

    for entry in lineup.entries() {
        let assigned = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM team_player_assignments WHERE player_id = $1 AND team_id = $2)",
        )
        .bind(entry.player_id().as_uuid())
        .bind(lineup.team_id().as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(LineupRepositoryError::Database)?;
        if !assigned {
            return Err(LineupRepositoryError::PlayerNotAssignedToTeam {
                player_id: entry.player_id(),
                team_id: lineup.team_id(),
            });
        }
    }

    sqlx::query(
        r#"
        INSERT INTO lineups (id, fixture_id, team_id, status)
        VALUES ($1, $2, $3, 'draft')
        "#,
    )
    .bind(lineup.id().as_uuid())
    .bind(lineup.fixture_id().as_uuid())
    .bind(lineup.team_id().as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(LineupRepositoryError::Database)?;

    for entry in lineup.entries() {
        sqlx::query("INSERT INTO lineup_entries (lineup_id, player_id, role) VALUES ($1, $2, $3)")
            .bind(lineup.id().as_uuid())
            .bind(entry.player_id().as_uuid())
            .bind(entry.role().to_string())
            .execute(&mut *transaction)
            .await
            .map_err(LineupRepositoryError::Database)?;
    }

    transaction
        .commit()
        .await
        .map_err(LineupRepositoryError::Database)?;

    Ok(lineup.clone())
}

pub async fn load_draft_lineup(
    pool: &PgPool,
    lineup_id: LineupId,
) -> Result<Option<DraftLineup>, LineupRepositoryError> {
    let row = sqlx::query_as::<_, LineupRow>(
        r#"
        SELECT id, fixture_id, team_id, status
        FROM lineups
        WHERE id = $1
        "#,
    )
    .bind(lineup_id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(LineupRepositoryError::Database)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let entries = sqlx::query_as::<_, LineupEntryRow>(
        r#"
        SELECT player_id, role
        FROM lineup_entries
        WHERE lineup_id = $1
        ORDER BY player_id
        "#,
    )
    .bind(lineup_id.as_uuid())
    .fetch_all(pool)
    .await
    .map_err(LineupRepositoryError::Database)?;

    build_draft_lineup(row, entries).map(Some)
}

#[cfg(test)]
mod tests {
    use super::parse_role;
    use football_domain::LineupRole;

    #[test]
    fn parses_only_the_two_persisted_lineup_roles() {
        assert_eq!(
            parse_role("starter".to_owned()).unwrap(),
            LineupRole::Starter
        );
        assert_eq!(
            parse_role("substitute".to_owned()).unwrap(),
            LineupRole::Substitute
        );
        assert!(parse_role("captain".to_owned()).is_err());
    }
}
