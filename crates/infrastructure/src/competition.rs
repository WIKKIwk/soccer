use football_domain::{
    Competition, CompetitionId, CompetitionName, CompetitionNameError, Season, SeasonId,
    SeasonYear, SeasonYearError,
};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use uuid::Uuid;

#[derive(Debug)]
pub enum CompetitionRepositoryError {
    Database(SqlxError),
    InvalidStoredName(CompetitionNameError),
    InvalidStoredSeasonYear(SeasonYearError),
    InvalidStoredSeasonYearRange {
        value: i32,
    },
    CompetitionNotFound {
        id: CompetitionId,
    },
    ExistingCompetitionConflict {
        id: CompetitionId,
    },
    ExistingSeasonConflict {
        id: SeasonId,
    },
    ExistingSeasonYearConflict {
        competition_id: CompetitionId,
        year: SeasonYear,
    },
}

impl fmt::Display for CompetitionRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "competition database operation failed: {error}")
            }
            Self::InvalidStoredName(error) => {
                write!(formatter, "stored competition name is invalid: {error}")
            }
            Self::InvalidStoredSeasonYear(error) => {
                write!(formatter, "stored season year is invalid: {error}")
            }
            Self::InvalidStoredSeasonYearRange { value } => {
                write!(
                    formatter,
                    "stored season year is outside u16 range: {value}"
                )
            }
            Self::CompetitionNotFound { id } => {
                write!(formatter, "competition {id} does not exist")
            }
            Self::ExistingCompetitionConflict { id } => {
                write!(
                    formatter,
                    "competition {id} already exists with a different name"
                )
            }
            Self::ExistingSeasonConflict { id } => {
                write!(formatter, "season {id} already exists with different data")
            }
            Self::ExistingSeasonYearConflict {
                competition_id,
                year,
            } => write!(
                formatter,
                "competition {competition_id} already has a season for year {year}"
            ),
        }
    }
}

impl std::error::Error for CompetitionRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidStoredName(error) => Some(error),
            Self::InvalidStoredSeasonYear(error) => Some(error),
            Self::InvalidStoredSeasonYearRange { .. }
            | Self::CompetitionNotFound { .. }
            | Self::ExistingCompetitionConflict { .. }
            | Self::ExistingSeasonConflict { .. }
            | Self::ExistingSeasonYearConflict { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CompetitionRow {
    id: Uuid,
    name: String,
}

impl TryFrom<CompetitionRow> for Competition {
    type Error = CompetitionRepositoryError;

    fn try_from(row: CompetitionRow) -> Result<Self, Self::Error> {
        let name = CompetitionName::try_new(row.name)
            .map_err(CompetitionRepositoryError::InvalidStoredName)?;
        Ok(Self::new(CompetitionId::from_uuid(row.id), name))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SeasonRow {
    id: Uuid,
    competition_id: Uuid,
    year: i32,
}

impl TryFrom<SeasonRow> for Season {
    type Error = CompetitionRepositoryError;

    fn try_from(row: SeasonRow) -> Result<Self, Self::Error> {
        let year = u16::try_from(row.year).map_err(|_| {
            CompetitionRepositoryError::InvalidStoredSeasonYearRange { value: row.year }
        })?;
        let year = SeasonYear::try_new(year)
            .map_err(CompetitionRepositoryError::InvalidStoredSeasonYear)?;
        Ok(Self::new(
            SeasonId::from_uuid(row.id),
            CompetitionId::from_uuid(row.competition_id),
            year,
        ))
    }
}

pub async fn persist_competition(
    pool: &PgPool,
    competition: &Competition,
) -> Result<Competition, CompetitionRepositoryError> {
    let existing = sqlx::query_as::<_, CompetitionRow>(
        r#"
        SELECT id, name
        FROM competitions
        WHERE id = $1
        "#,
    )
    .bind(competition.id().as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(CompetitionRepositoryError::Database)?;

    if let Some(row) = existing {
        let existing = row.try_into()?;
        if existing != *competition {
            return Err(CompetitionRepositoryError::ExistingCompetitionConflict {
                id: competition.id(),
            });
        }
        return Ok(existing);
    }

    sqlx::query_as::<_, CompetitionRow>(
        r#"
        INSERT INTO competitions (id, name)
        VALUES ($1, $2)
        RETURNING id, name
        "#,
    )
    .bind(competition.id().as_uuid())
    .bind(competition.name().as_str())
    .fetch_one(pool)
    .await
    .map_err(CompetitionRepositoryError::Database)?
    .try_into()
}

pub async fn load_competition(
    pool: &PgPool,
    id: CompetitionId,
) -> Result<Option<Competition>, CompetitionRepositoryError> {
    sqlx::query_as::<_, CompetitionRow>(
        r#"
        SELECT id, name
        FROM competitions
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(CompetitionRepositoryError::Database)?
    .map(TryInto::try_into)
    .transpose()
}

pub async fn persist_season(
    pool: &PgPool,
    season: Season,
) -> Result<Season, CompetitionRepositoryError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(CompetitionRepositoryError::Database)?;

    let competition_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM competitions WHERE id = $1)")
            .bind(season.competition_id().as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .map_err(CompetitionRepositoryError::Database)?;
    if !competition_exists {
        return Err(CompetitionRepositoryError::CompetitionNotFound {
            id: season.competition_id(),
        });
    }

    let existing = sqlx::query_as::<_, SeasonRow>(
        r#"
        SELECT id, competition_id, year
        FROM seasons
        WHERE id = $1
        "#,
    )
    .bind(season.id().as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(CompetitionRepositoryError::Database)?;

    let persisted = match existing {
        Some(row) => {
            let existing = row.try_into()?;
            if existing != season {
                return Err(CompetitionRepositoryError::ExistingSeasonConflict { id: season.id() });
            }
            existing
        }
        None => {
            let existing_year = sqlx::query_scalar::<_, Uuid>(
                r#"
                SELECT id
                FROM seasons
                WHERE competition_id = $1 AND year = $2
                "#,
            )
            .bind(season.competition_id().as_uuid())
            .bind(i32::from(season.year().value()))
            .fetch_optional(&mut *transaction)
            .await
            .map_err(CompetitionRepositoryError::Database)?;
            if existing_year.is_some() {
                return Err(CompetitionRepositoryError::ExistingSeasonYearConflict {
                    competition_id: season.competition_id(),
                    year: season.year(),
                });
            }

            sqlx::query_as::<_, SeasonRow>(
                r#"
                INSERT INTO seasons (id, competition_id, year)
                VALUES ($1, $2, $3)
                RETURNING id, competition_id, year
                "#,
            )
            .bind(season.id().as_uuid())
            .bind(season.competition_id().as_uuid())
            .bind(i32::from(season.year().value()))
            .fetch_one(&mut *transaction)
            .await
            .map_err(CompetitionRepositoryError::Database)?
            .try_into()?
        }
    };

    transaction
        .commit()
        .await
        .map_err(CompetitionRepositoryError::Database)?;
    Ok(persisted)
}

pub async fn load_season(
    pool: &PgPool,
    id: SeasonId,
) -> Result<Option<Season>, CompetitionRepositoryError> {
    sqlx::query_as::<_, SeasonRow>(
        r#"
        SELECT id, competition_id, year
        FROM seasons
        WHERE id = $1
        "#,
    )
    .bind(id.as_uuid())
    .fetch_optional(pool)
    .await
    .map_err(CompetitionRepositoryError::Database)?
    .map(TryInto::try_into)
    .transpose()
}
