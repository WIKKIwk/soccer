use football_domain::{
    AdminUserId, MatchAssignment, MatchId, OperatorLease, OperatorLeaseError, OperatorLeaseId,
    UtcInstant,
};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub enum OperatorAssignmentRepositoryError {
    Database(SqlxError),
    FixtureNotFound {
        id: football_domain::FixtureId,
    },
    OperatorNotFound {
        id: AdminUserId,
    },
    OperatorInactive {
        id: AdminUserId,
    },
    MatchAssignmentNotFound {
        id: MatchId,
    },
    ExistingMatchAssignmentConflict {
        id: MatchId,
    },
    ExistingFixtureAssignmentConflict {
        fixture_id: football_domain::FixtureId,
        existing_match_id: MatchId,
    },
    OperatorNotAssigned {
        match_id: MatchId,
        assigned_operator_id: AdminUserId,
        requested_operator_id: AdminUserId,
    },
    OperatorLeaseNotFound {
        match_id: MatchId,
    },
    OperatorLeaseNotOwned {
        match_id: MatchId,
        expected_lease_id: OperatorLeaseId,
        expected_operator_id: AdminUserId,
        requested_lease_id: OperatorLeaseId,
        requested_operator_id: AdminUserId,
    },
    InvalidLease(OperatorLeaseError),
    InvalidStoredLease(OperatorLeaseError),
    InvalidHeartbeat(OperatorLeaseError),
    ExistingLeaseConflict {
        id: OperatorLeaseId,
    },
    ActiveLeaseConflict {
        match_id: MatchId,
        lease_id: OperatorLeaseId,
        operator_id: AdminUserId,
    },
}

impl fmt::Display for OperatorAssignmentRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(
                    formatter,
                    "operator assignment database operation failed: {error}"
                )
            }
            Self::FixtureNotFound { id } => write!(formatter, "fixture {id} does not exist"),
            Self::OperatorNotFound { id } => write!(formatter, "operator {id} does not exist"),
            Self::OperatorInactive { id } => write!(formatter, "operator {id} is inactive"),
            Self::MatchAssignmentNotFound { id } => {
                write!(formatter, "match assignment for {id} does not exist")
            }
            Self::ExistingMatchAssignmentConflict { id } => write!(
                formatter,
                "match assignment {id} already exists with different data"
            ),
            Self::ExistingFixtureAssignmentConflict {
                fixture_id,
                existing_match_id,
            } => write!(
                formatter,
                "fixture {fixture_id} is already assigned to match {existing_match_id}"
            ),
            Self::OperatorNotAssigned {
                match_id,
                assigned_operator_id,
                requested_operator_id,
            } => write!(
                formatter,
                "operator {requested_operator_id} is not assigned to match {match_id}; assigned operator is {assigned_operator_id}"
            ),
            Self::OperatorLeaseNotFound { match_id } => {
                write!(
                    formatter,
                    "operator lease for match {match_id} does not exist"
                )
            }
            Self::OperatorLeaseNotOwned {
                match_id,
                expected_lease_id,
                expected_operator_id,
                requested_lease_id,
                requested_operator_id,
            } => write!(
                formatter,
                "operator {requested_operator_id} cannot use lease {requested_lease_id} for match {match_id}; current lease is {expected_lease_id} held by operator {expected_operator_id}"
            ),
            Self::InvalidLease(error) => error.fmt(formatter),
            Self::InvalidStoredLease(error) => {
                write!(formatter, "stored operator lease is invalid: {error}")
            }
            Self::InvalidHeartbeat(error) => error.fmt(formatter),
            Self::ExistingLeaseConflict { id } => {
                write!(
                    formatter,
                    "operator lease {id} already exists for another match"
                )
            }
            Self::ActiveLeaseConflict {
                match_id,
                lease_id,
                operator_id,
            } => write!(
                formatter,
                "match {match_id} has active lease {lease_id} held by operator {operator_id}"
            ),
        }
    }
}

impl std::error::Error for OperatorAssignmentRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidLease(error)
            | Self::InvalidStoredLease(error)
            | Self::InvalidHeartbeat(error) => Some(error),
            Self::FixtureNotFound { .. }
            | Self::OperatorNotFound { .. }
            | Self::OperatorInactive { .. }
            | Self::MatchAssignmentNotFound { .. }
            | Self::ExistingMatchAssignmentConflict { .. }
            | Self::ExistingFixtureAssignmentConflict { .. }
            | Self::OperatorNotAssigned { .. }
            | Self::OperatorLeaseNotFound { .. }
            | Self::OperatorLeaseNotOwned { .. }
            | Self::ExistingLeaseConflict { .. }
            | Self::ActiveLeaseConflict { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct MatchAssignmentRow {
    match_id: Uuid,
    fixture_id: Uuid,
    operator_id: Uuid,
}

impl From<MatchAssignmentRow> for MatchAssignment {
    fn from(row: MatchAssignmentRow) -> Self {
        Self::new(
            MatchId::from_uuid(row.match_id),
            football_domain::FixtureId::from_uuid(row.fixture_id),
            AdminUserId::from_uuid(row.operator_id),
        )
    }
}

#[derive(Debug, sqlx::FromRow)]
struct OperatorLeaseRow {
    lease_id: Uuid,
    match_id: Uuid,
    operator_id: Uuid,
    claimed_at: OffsetDateTime,
    expires_at: OffsetDateTime,
}

impl TryFrom<OperatorLeaseRow> for OperatorLease {
    type Error = OperatorAssignmentRepositoryError;

    fn try_from(row: OperatorLeaseRow) -> Result<Self, Self::Error> {
        Self::try_new(
            OperatorLeaseId::from_uuid(row.lease_id),
            MatchId::from_uuid(row.match_id),
            AdminUserId::from_uuid(row.operator_id),
            UtcInstant::from_offset_datetime(row.claimed_at),
            UtcInstant::from_offset_datetime(row.expires_at),
        )
        .map_err(OperatorAssignmentRepositoryError::InvalidStoredLease)
    }
}

async fn load_assignment_row(
    executor: impl sqlx::PgExecutor<'_>,
    match_id: MatchId,
    for_update: bool,
) -> Result<Option<MatchAssignmentRow>, OperatorAssignmentRepositoryError> {
    let query = if for_update {
        "SELECT match_id, fixture_id, operator_id FROM match_assignments WHERE match_id = $1 FOR UPDATE"
    } else {
        "SELECT match_id, fixture_id, operator_id FROM match_assignments WHERE match_id = $1"
    };
    sqlx::query_as::<_, MatchAssignmentRow>(query)
        .bind(match_id.as_uuid())
        .fetch_optional(executor)
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)
}

async fn load_lease_row(
    executor: impl sqlx::PgExecutor<'_>,
    match_id: MatchId,
    for_update: bool,
) -> Result<Option<OperatorLeaseRow>, OperatorAssignmentRepositoryError> {
    let query = if for_update {
        "SELECT lease_id, match_id, operator_id, claimed_at, expires_at FROM match_operator_leases WHERE match_id = $1 FOR UPDATE"
    } else {
        "SELECT lease_id, match_id, operator_id, claimed_at, expires_at FROM match_operator_leases WHERE match_id = $1"
    };
    sqlx::query_as::<_, OperatorLeaseRow>(query)
        .bind(match_id.as_uuid())
        .fetch_optional(executor)
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)
}

async fn require_active_operator(
    executor: impl sqlx::PgExecutor<'_>,
    operator_id: AdminUserId,
) -> Result<(), OperatorAssignmentRepositoryError> {
    let status = sqlx::query_as::<_, (bool,)>("SELECT is_active FROM admin_users WHERE id = $1")
        .bind(operator_id.as_uuid())
        .fetch_optional(executor)
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;

    match status {
        None => Err(OperatorAssignmentRepositoryError::OperatorNotFound { id: operator_id }),
        Some((false,)) => {
            Err(OperatorAssignmentRepositoryError::OperatorInactive { id: operator_id })
        }
        Some((true,)) => Ok(()),
    }
}

pub async fn persist_match_assignment(
    pool: &PgPool,
    assignment: &MatchAssignment,
) -> Result<MatchAssignment, OperatorAssignmentRepositoryError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;

    let fixture_exists =
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM fixtures WHERE id = $1)")
            .bind(assignment.fixture_id().as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .map_err(OperatorAssignmentRepositoryError::Database)?;
    if !fixture_exists {
        return Err(OperatorAssignmentRepositoryError::FixtureNotFound {
            id: assignment.fixture_id(),
        });
    }

    require_active_operator(&mut *transaction, assignment.operator_id()).await?;

    if let Some(row) = load_assignment_row(&mut *transaction, assignment.match_id(), false).await? {
        let existing = MatchAssignment::from(row);
        if existing == *assignment {
            transaction
                .commit()
                .await
                .map_err(OperatorAssignmentRepositoryError::Database)?;
            return Ok(existing);
        }
        return Err(
            OperatorAssignmentRepositoryError::ExistingMatchAssignmentConflict {
                id: assignment.match_id(),
            },
        );
    }

    let existing_fixture_match = sqlx::query_as::<_, (Uuid,)>(
        "SELECT match_id FROM match_assignments WHERE fixture_id = $1",
    )
    .bind(assignment.fixture_id().as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;
    if let Some((existing_match_id,)) = existing_fixture_match {
        return Err(
            OperatorAssignmentRepositoryError::ExistingFixtureAssignmentConflict {
                fixture_id: assignment.fixture_id(),
                existing_match_id: MatchId::from_uuid(existing_match_id),
            },
        );
    }

    sqlx::query(
        r#"
        INSERT INTO match_assignments (match_id, fixture_id, operator_id)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(assignment.match_id().as_uuid())
    .bind(assignment.fixture_id().as_uuid())
    .bind(assignment.operator_id().as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;

    transaction
        .commit()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    Ok(*assignment)
}

pub async fn load_match_assignment(
    pool: &PgPool,
    match_id: MatchId,
) -> Result<Option<MatchAssignment>, OperatorAssignmentRepositoryError> {
    load_assignment_row(pool, match_id, false)
        .await
        .map(|row| row.map(MatchAssignment::from))
}

pub async fn claim_operator_lease(
    pool: &PgPool,
    match_id: MatchId,
    operator_id: AdminUserId,
    lease_id: OperatorLeaseId,
    claimed_at: UtcInstant,
    expires_at: UtcInstant,
) -> Result<OperatorLease, OperatorAssignmentRepositoryError> {
    let requested = OperatorLease::try_new(lease_id, match_id, operator_id, claimed_at, expires_at)
        .map_err(OperatorAssignmentRepositoryError::InvalidLease)?;

    let mut transaction = pool
        .begin()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    let assignment = load_assignment_row(&mut *transaction, match_id, true)
        .await?
        .map(MatchAssignment::from)
        .ok_or(OperatorAssignmentRepositoryError::MatchAssignmentNotFound { id: match_id })?;

    if assignment.operator_id() != operator_id {
        return Err(OperatorAssignmentRepositoryError::OperatorNotAssigned {
            match_id,
            assigned_operator_id: assignment.operator_id(),
            requested_operator_id: operator_id,
        });
    }
    require_active_operator(&mut *transaction, operator_id).await?;

    let existing_by_id = sqlx::query_as::<_, (Uuid,)>(
        "SELECT match_id FROM match_operator_leases WHERE lease_id = $1",
    )
    .bind(lease_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;
    if let Some((existing_match_id,)) = existing_by_id {
        let existing_match_id = MatchId::from_uuid(existing_match_id);
        if existing_match_id != match_id {
            return Err(OperatorAssignmentRepositoryError::ExistingLeaseConflict { id: lease_id });
        }
    }

    let existing = load_lease_row(&mut *transaction, match_id, true).await?;
    if let Some(row) = existing {
        let existing = OperatorLease::try_from(row)?;
        if existing.is_active_at(claimed_at) {
            if existing.id() == requested.id()
                && existing.match_id() == requested.match_id()
                && existing.operator_id() == requested.operator_id()
            {
                transaction
                    .commit()
                    .await
                    .map_err(OperatorAssignmentRepositoryError::Database)?;
                return Ok(existing);
            }
            return Err(OperatorAssignmentRepositoryError::ActiveLeaseConflict {
                match_id,
                lease_id: existing.id(),
                operator_id: existing.operator_id(),
            });
        }

        let replaced = sqlx::query_as::<_, OperatorLeaseRow>(
            r#"
            UPDATE match_operator_leases
            SET lease_id = $1, operator_id = $2, claimed_at = $3, expires_at = $4
            WHERE match_id = $5
            RETURNING lease_id, match_id, operator_id, claimed_at, expires_at
            "#,
        )
        .bind(requested.id().as_uuid())
        .bind(requested.operator_id().as_uuid())
        .bind(requested.claimed_at().as_offset_datetime())
        .bind(requested.expires_at().as_offset_datetime())
        .bind(requested.match_id().as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
        let replaced = OperatorLease::try_from(replaced)?;
        transaction
            .commit()
            .await
            .map_err(OperatorAssignmentRepositoryError::Database)?;
        return Ok(replaced);
    }

    let inserted = sqlx::query_as::<_, OperatorLeaseRow>(
        r#"
        INSERT INTO match_operator_leases (
            lease_id, match_id, operator_id, claimed_at, expires_at
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING lease_id, match_id, operator_id, claimed_at, expires_at
        "#,
    )
    .bind(requested.id().as_uuid())
    .bind(requested.match_id().as_uuid())
    .bind(requested.operator_id().as_uuid())
    .bind(requested.claimed_at().as_offset_datetime())
    .bind(requested.expires_at().as_offset_datetime())
    .fetch_one(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;
    let inserted = OperatorLease::try_from(inserted)?;

    transaction
        .commit()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    Ok(inserted)
}

pub async fn load_operator_lease(
    pool: &PgPool,
    match_id: MatchId,
) -> Result<Option<OperatorLease>, OperatorAssignmentRepositoryError> {
    load_lease_row(pool, match_id, false)
        .await?
        .map(OperatorLease::try_from)
        .transpose()
}

pub async fn heartbeat_operator_lease(
    pool: &PgPool,
    match_id: MatchId,
    operator_id: AdminUserId,
    lease_id: OperatorLeaseId,
    heartbeat_at: UtcInstant,
    expires_at: UtcInstant,
) -> Result<OperatorLease, OperatorAssignmentRepositoryError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    let assignment = load_assignment_row(&mut *transaction, match_id, true)
        .await?
        .map(MatchAssignment::from)
        .ok_or(OperatorAssignmentRepositoryError::MatchAssignmentNotFound { id: match_id })?;

    if assignment.operator_id() != operator_id {
        return Err(OperatorAssignmentRepositoryError::OperatorNotAssigned {
            match_id,
            assigned_operator_id: assignment.operator_id(),
            requested_operator_id: operator_id,
        });
    }
    require_active_operator(&mut *transaction, operator_id).await?;

    let existing = load_lease_row(&mut *transaction, match_id, true)
        .await?
        .map(OperatorLease::try_from)
        .transpose()?
        .ok_or(OperatorAssignmentRepositoryError::OperatorLeaseNotFound { match_id })?;
    if existing.id() != lease_id || existing.operator_id() != operator_id {
        return Err(OperatorAssignmentRepositoryError::OperatorLeaseNotOwned {
            match_id,
            expected_lease_id: existing.id(),
            expected_operator_id: existing.operator_id(),
            requested_lease_id: lease_id,
            requested_operator_id: operator_id,
        });
    }

    let renewed = existing
        .try_heartbeat(heartbeat_at, expires_at)
        .map_err(OperatorAssignmentRepositoryError::InvalidHeartbeat)?;
    let renewed = sqlx::query_as::<_, OperatorLeaseRow>(
        r#"
        UPDATE match_operator_leases
        SET expires_at = $1
        WHERE match_id = $2 AND lease_id = $3 AND operator_id = $4
        RETURNING lease_id, match_id, operator_id, claimed_at, expires_at
        "#,
    )
    .bind(renewed.expires_at().as_offset_datetime())
    .bind(renewed.match_id().as_uuid())
    .bind(renewed.id().as_uuid())
    .bind(renewed.operator_id().as_uuid())
    .fetch_one(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;
    let renewed = OperatorLease::try_from(renewed)?;

    transaction
        .commit()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    Ok(renewed)
}

pub async fn release_operator_lease(
    pool: &PgPool,
    match_id: MatchId,
    operator_id: AdminUserId,
    lease_id: OperatorLeaseId,
) -> Result<(), OperatorAssignmentRepositoryError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    let assignment = load_assignment_row(&mut *transaction, match_id, true)
        .await?
        .map(MatchAssignment::from)
        .ok_or(OperatorAssignmentRepositoryError::MatchAssignmentNotFound { id: match_id })?;

    if assignment.operator_id() != operator_id {
        return Err(OperatorAssignmentRepositoryError::OperatorNotAssigned {
            match_id,
            assigned_operator_id: assignment.operator_id(),
            requested_operator_id: operator_id,
        });
    }
    require_active_operator(&mut *transaction, operator_id).await?;

    let existing = load_lease_row(&mut *transaction, match_id, true).await?;
    let Some(existing) = existing else {
        transaction
            .commit()
            .await
            .map_err(OperatorAssignmentRepositoryError::Database)?;
        return Ok(());
    };
    let existing = OperatorLease::try_from(existing)?;
    if existing.id() != lease_id || existing.operator_id() != operator_id {
        return Err(OperatorAssignmentRepositoryError::OperatorLeaseNotOwned {
            match_id,
            expected_lease_id: existing.id(),
            expected_operator_id: existing.operator_id(),
            requested_lease_id: lease_id,
            requested_operator_id: operator_id,
        });
    }

    sqlx::query(
        "DELETE FROM match_operator_leases WHERE match_id = $1 AND lease_id = $2 AND operator_id = $3",
    )
    .bind(match_id.as_uuid())
    .bind(lease_id.as_uuid())
    .bind(operator_id.as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;

    transaction
        .commit()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    Ok(())
}

pub async fn takeover_operator_lease(
    pool: &PgPool,
    match_id: MatchId,
    operator_id: AdminUserId,
    lease_id: OperatorLeaseId,
    claimed_at: UtcInstant,
    expires_at: UtcInstant,
) -> Result<OperatorLease, OperatorAssignmentRepositoryError> {
    let requested = OperatorLease::try_new(lease_id, match_id, operator_id, claimed_at, expires_at)
        .map_err(OperatorAssignmentRepositoryError::InvalidLease)?;

    let mut transaction = pool
        .begin()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    let _assignment = load_assignment_row(&mut *transaction, match_id, true)
        .await?
        .map(MatchAssignment::from)
        .ok_or(OperatorAssignmentRepositoryError::MatchAssignmentNotFound { id: match_id })?;
    require_active_operator(&mut *transaction, operator_id).await?;

    let existing_by_id = sqlx::query_as::<_, (Uuid,)>(
        "SELECT match_id FROM match_operator_leases WHERE lease_id = $1",
    )
    .bind(lease_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;
    if existing_by_id.is_some() {
        return Err(OperatorAssignmentRepositoryError::ExistingLeaseConflict { id: lease_id });
    }

    let existing = load_lease_row(&mut *transaction, match_id, true)
        .await?
        .map(OperatorLease::try_from)
        .transpose()?;
    let persisted = if let Some(existing) = existing {
        if existing.is_active_at(claimed_at) {
            return Err(OperatorAssignmentRepositoryError::ActiveLeaseConflict {
                match_id,
                lease_id: existing.id(),
                operator_id: existing.operator_id(),
            });
        }

        let row = sqlx::query_as::<_, OperatorLeaseRow>(
            r#"
            UPDATE match_operator_leases
            SET lease_id = $1, operator_id = $2, claimed_at = $3, expires_at = $4
            WHERE match_id = $5
            RETURNING lease_id, match_id, operator_id, claimed_at, expires_at
            "#,
        )
        .bind(requested.id().as_uuid())
        .bind(requested.operator_id().as_uuid())
        .bind(requested.claimed_at().as_offset_datetime())
        .bind(requested.expires_at().as_offset_datetime())
        .bind(requested.match_id().as_uuid())
        .fetch_one(&mut *transaction)
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
        OperatorLease::try_from(row)?
    } else {
        let row = sqlx::query_as::<_, OperatorLeaseRow>(
            r#"
            INSERT INTO match_operator_leases (
                lease_id, match_id, operator_id, claimed_at, expires_at
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING lease_id, match_id, operator_id, claimed_at, expires_at
            "#,
        )
        .bind(requested.id().as_uuid())
        .bind(requested.match_id().as_uuid())
        .bind(requested.operator_id().as_uuid())
        .bind(requested.claimed_at().as_offset_datetime())
        .bind(requested.expires_at().as_offset_datetime())
        .fetch_one(&mut *transaction)
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
        OperatorLease::try_from(row)?
    };

    sqlx::query(
        "UPDATE match_assignments SET operator_id = $1, updated_at = CURRENT_TIMESTAMP WHERE match_id = $2",
    )
    .bind(requested.operator_id().as_uuid())
    .bind(requested.match_id().as_uuid())
    .execute(&mut *transaction)
    .await
    .map_err(OperatorAssignmentRepositoryError::Database)?;

    transaction
        .commit()
        .await
        .map_err(OperatorAssignmentRepositoryError::Database)?;
    Ok(persisted)
}
