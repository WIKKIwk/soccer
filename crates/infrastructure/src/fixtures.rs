use football_domain::{
    CompetitionId, Fixture, FixtureError, FixtureId, Season, SeasonId, SeasonYear, SeasonYearError,
    TeamId,
};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub enum FixtureRepositoryError {
    Database(SqlxError),
    SeasonNotFound {
        id: SeasonId,
    },
    TeamNotFound {
        id: TeamId,
    },
    InvalidStoredSeasonYear(SeasonYearError),
    InvalidStoredSeasonYearRange {
        value: i32,
    },
    InvalidStoredFixture(FixtureError),
    FixtureSeasonOwnershipMismatch {
        fixture_id: FixtureId,
        expected_season_id: SeasonId,
        actual_season_id: SeasonId,
    },
    FixtureCompetitionOwnershipMismatch {
        fixture_id: FixtureId,
        expected: CompetitionId,
        actual: CompetitionId,
    },
    ExistingFixtureConflict {
        id: FixtureId,
    },
}

impl fmt::Display for FixtureRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "fixture database operation failed: {error}")
            }
            Self::SeasonNotFound { id } => write!(formatter, "season {id} does not exist"),
            Self::TeamNotFound { id } => write!(formatter, "team {id} does not exist"),
            Self::InvalidStoredSeasonYear(error) => {
                write!(formatter, "stored fixture season year is invalid: {error}")
            }
            Self::InvalidStoredSeasonYearRange { value } => {
                write!(
                    formatter,
                    "stored fixture season year is outside u16 range: {value}"
                )
            }
            Self::InvalidStoredFixture(error) => {
                write!(formatter, "stored fixture is invalid: {error}")
            }
            Self::FixtureSeasonOwnershipMismatch {
                fixture_id,
                expected_season_id,
                actual_season_id,
            } => write!(
                formatter,
                "fixture {fixture_id} points to season {actual_season_id}, expected {expected_season_id}"
            ),
            Self::FixtureCompetitionOwnershipMismatch {
                fixture_id,
                expected,
                actual,
            } => write!(
                formatter,
                "fixture {fixture_id} has competition {actual}, expected {expected}"
            ),
            Self::ExistingFixtureConflict { id } => {
                write!(formatter, "fixture {id} already exists with different data")
            }
        }
    }
}

impl std::error::Error for FixtureRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredSeasonYear(error) => Some(error),
            Self::InvalidStoredFixture(error) => Some(error),
            Self::SeasonNotFound { .. }
            | Self::TeamNotFound { .. }
            | Self::InvalidStoredSeasonYearRange { .. }
            | Self::FixtureSeasonOwnershipMismatch { .. }
            | Self::FixtureCompetitionOwnershipMismatch { .. }
            | Self::ExistingFixtureConflict { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SeasonRow {
    id: Uuid,
    competition_id: Uuid,
    year: i32,
}

impl TryFrom<SeasonRow> for Season {
    type Error = FixtureRepositoryError;

    fn try_from(row: SeasonRow) -> Result<Self, Self::Error> {
        let year = u16::try_from(row.year).map_err(|_| {
            FixtureRepositoryError::InvalidStoredSeasonYearRange { value: row.year }
        })?;
        let year =
            SeasonYear::try_new(year).map_err(FixtureRepositoryError::InvalidStoredSeasonYear)?;
        Ok(Self::new(
            SeasonId::from_uuid(row.id),
            CompetitionId::from_uuid(row.competition_id),
            year,
        ))
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

fn build_fixture(row: FixtureRow, season: Season) -> Result<Fixture, FixtureRepositoryError> {
    let fixture_id = FixtureId::from_uuid(row.id);
    if row.season_id != season.id().as_uuid() {
        return Err(FixtureRepositoryError::FixtureSeasonOwnershipMismatch {
            fixture_id,
            expected_season_id: season.id(),
            actual_season_id: SeasonId::from_uuid(row.season_id),
        });
    }
    if row.competition_id != season.competition_id().as_uuid() {
        return Err(
            FixtureRepositoryError::FixtureCompetitionOwnershipMismatch {
                fixture_id,
                expected: season.competition_id(),
                actual: CompetitionId::from_uuid(row.competition_id),
            },
        );
    }

    Fixture::try_new(
        fixture_id,
        season,
        TeamId::from_uuid(row.home_team_id),
        TeamId::from_uuid(row.away_team_id),
        football_domain::UtcInstant::from_offset_datetime(row.kickoff_at),
    )
    .map_err(FixtureRepositoryError::InvalidStoredFixture)
}

async fn load_season_row(
    executor: impl sqlx::PgExecutor<'_>,
    id: SeasonId,
) -> Result<Option<Season>, FixtureRepositoryError> {
    sqlx::query_as::<_, SeasonRow>(
        r#"
        SELECT id, competition_id, year
        FROM seasons
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(executor)
    .await
    .map_err(FixtureRepositoryError::Database)?
    .map(TryInto::try_into)
    .transpose()
}

pub async fn persist_fixture(
    pool: &PgPool,
    fixture: &Fixture,
) -> Result<Fixture, FixtureRepositoryError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(FixtureRepositoryError::Database)?;
    let season = load_season_row(
        &mut *transaction,
        SeasonId::from_uuid(fixture.season_id().as_uuid()),
    )
    .await?
    .ok_or(FixtureRepositoryError::SeasonNotFound {
        id: SeasonId::from_uuid(fixture.season_id().as_uuid()),
    })?;

    if fixture.competition_id() != season.competition_id() {
        return Err(
            FixtureRepositoryError::FixtureCompetitionOwnershipMismatch {
                fixture_id: fixture.id(),
                expected: season.competition_id(),
                actual: fixture.competition_id(),
            },
        );
    }

    for team_id in [fixture.home_team_id(), fixture.away_team_id()] {
        let exists =
            sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM teams WHERE id = $1)")
                .bind(team_id.as_uuid())
                .fetch_one(&mut *transaction)
                .await
                .map_err(FixtureRepositoryError::Database)?;
        if !exists {
            return Err(FixtureRepositoryError::TeamNotFound { id: team_id });
        }
    }

    let existing = sqlx::query_as::<_, FixtureRow>(
        r#"
        SELECT id, competition_id, season_id, home_team_id, away_team_id, kickoff_at
        FROM fixtures
        WHERE id = $1
        "#,
    )
    .bind(fixture.id().as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(FixtureRepositoryError::Database)?;

    if let Some(row) = existing {
        let existing = build_fixture(row, season)?;
        if existing != *fixture {
            return Err(FixtureRepositoryError::ExistingFixtureConflict { id: fixture.id() });
        }
        transaction
            .commit()
            .await
            .map_err(FixtureRepositoryError::Database)?;
        return Ok(existing);
    }

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
    .map_err(FixtureRepositoryError::Database)?;

    transaction
        .commit()
        .await
        .map_err(FixtureRepositoryError::Database)?;
    Ok(*fixture)
}

pub async fn load_fixture(
    pool: &PgPool,
    id: FixtureId,
) -> Result<Option<Fixture>, FixtureRepositoryError> {
    let row = sqlx::query_as::<_, FixtureRow>(
        r#"
        SELECT id, competition_id, season_id, home_team_id, away_team_id, kickoff_at
        FROM fixtures
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(FixtureRepositoryError::Database)?;

    let Some(row) = row else {
        return Ok(None);
    };
    let season_id = SeasonId::from_uuid(row.season_id);
    let season = load_season_row(pool, season_id)
        .await?
        .ok_or(FixtureRepositoryError::SeasonNotFound { id: season_id })?;
    build_fixture(row, season).map(Some)
}
