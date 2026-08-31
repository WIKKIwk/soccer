use football_domain::{
    ActorIdentity, AdminUserId, LineupId, LineupSubmittedPayload, MatchId, OperatorLeaseId,
    UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::PgPool;
use std::fmt;

use crate::{
    AdminUserRepositoryError, LineupRepositoryError, OperatorAssignmentRepositoryError,
    load_admin_user, load_draft_lineup, load_match_assignment, load_operator_lease,
};

#[derive(Debug)]
pub enum MatchCommandAccessError {
    Administrator(AdminUserRepositoryError),
    Assignment(OperatorAssignmentRepositoryError),
    Lineup(LineupRepositoryError),
    AdministratorMissingOrInactive {
        actor_id: AdminUserId,
    },
    MatchAssignmentMissing {
        match_id: MatchId,
    },
    OperatorNotAssigned {
        match_id: MatchId,
    },
    OperatorLeaseMissingOrInvalid {
        match_id: MatchId,
    },
    DraftLineupMissing {
        lineup_id: LineupId,
    },
    LineupFixtureMismatch {
        lineup_id: LineupId,
        match_id: MatchId,
    },
    InvalidLineup(football_domain::LineupValidationError),
}

impl MatchCommandAccessError {
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::Administrator(AdminUserRepositoryError::Database(_))
            | Self::Assignment(OperatorAssignmentRepositoryError::Database(_))
            | Self::Lineup(LineupRepositoryError::Database(_)) => ApplicationError::new(
                ErrorCode::DependencyDatabaseUnavailable,
                "the match command access dependency is unavailable",
            ),
            Self::AdministratorMissingOrInactive { .. } => ApplicationError::new(
                ErrorCode::AuthenticationInvalidSession,
                "the development administrator is not active",
            ),
            Self::MatchAssignmentMissing { match_id } => ApplicationError::new(
                ErrorCode::ResourceNotFound,
                "the match assignment was not found",
            )
            .with_detail("match_id", match_id.to_string()),
            Self::OperatorNotAssigned { match_id } => ApplicationError::new(
                ErrorCode::AuthorizationForbidden,
                "the administrator is not assigned to this match",
            )
            .with_detail("match_id", match_id.to_string()),
            Self::OperatorLeaseMissingOrInvalid { match_id } => ApplicationError::new(
                ErrorCode::ConflictOperatorLease,
                "the assigned operator does not own an active match lease",
            )
            .with_detail("match_id", match_id.to_string()),
            Self::DraftLineupMissing { lineup_id } => ApplicationError::new(
                ErrorCode::ResourceNotFound,
                "the draft lineup was not found",
            )
            .with_detail("lineup_id", lineup_id.to_string()),
            Self::LineupFixtureMismatch {
                lineup_id,
                match_id,
            } => ApplicationError::new(
                ErrorCode::ValidationInvalidValue,
                "the draft lineup does not belong to the match fixture",
            )
            .with_detail("lineup_id", lineup_id.to_string())
            .with_detail("match_id", match_id.to_string()),
            Self::InvalidLineup(error) => ApplicationError::new(
                ErrorCode::DomainLineupDuplicatePlayer,
                "the submitted lineup is invalid",
            )
            .with_detail("reason", error.to_string()),
            Self::Assignment(_) | Self::Lineup(_) => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the match command access state is invalid",
            ),
        }
    }
}

impl fmt::Display for MatchCommandAccessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Administrator(error) => error.fmt(formatter),
            Self::Assignment(error) => error.fmt(formatter),
            Self::Lineup(error) => error.fmt(formatter),
            Self::AdministratorMissingOrInactive { actor_id } => {
                write!(formatter, "administrator {actor_id} is missing or inactive")
            }
            Self::MatchAssignmentMissing { match_id } => {
                write!(formatter, "match assignment {match_id} is missing")
            }
            Self::OperatorNotAssigned { match_id } => {
                write!(formatter, "actor is not assigned to match {match_id}")
            }
            Self::OperatorLeaseMissingOrInvalid { match_id } => {
                write!(
                    formatter,
                    "match {match_id} has no matching active operator lease"
                )
            }
            Self::DraftLineupMissing { lineup_id } => {
                write!(formatter, "draft lineup {lineup_id} is missing")
            }
            Self::LineupFixtureMismatch {
                lineup_id,
                match_id,
            } => write!(
                formatter,
                "lineup {lineup_id} does not belong to match {match_id}"
            ),
            Self::InvalidLineup(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for MatchCommandAccessError {}

pub async fn require_active_match_operator(
    pool: &PgPool,
    match_id: MatchId,
    actor: ActorIdentity,
    lease_id: OperatorLeaseId,
    now_utc: UtcInstant,
) -> Result<(), MatchCommandAccessError> {
    let actor_id = actor.admin_user_id();
    let administrator = load_admin_user(pool, actor_id)
        .await
        .map_err(MatchCommandAccessError::Administrator)?
        .filter(|administrator| administrator.is_active())
        .ok_or(MatchCommandAccessError::AdministratorMissingOrInactive { actor_id })?;
    debug_assert_eq!(administrator.id(), actor_id);

    let assignment = load_match_assignment(pool, match_id)
        .await
        .map_err(MatchCommandAccessError::Assignment)?
        .ok_or(MatchCommandAccessError::MatchAssignmentMissing { match_id })?;
    if assignment.operator_id() != actor_id {
        return Err(MatchCommandAccessError::OperatorNotAssigned { match_id });
    }

    let lease = load_operator_lease(pool, match_id)
        .await
        .map_err(MatchCommandAccessError::Assignment)?
        .filter(|lease| {
            lease.id() == lease_id && lease.operator_id() == actor_id && lease.is_active_at(now_utc)
        })
        .ok_or(MatchCommandAccessError::OperatorLeaseMissingOrInvalid { match_id })?;
    debug_assert_eq!(lease.match_id(), match_id);
    Ok(())
}

pub async fn load_lineup_submission(
    pool: &PgPool,
    match_id: MatchId,
    lineup_id: LineupId,
) -> Result<LineupSubmittedPayload, MatchCommandAccessError> {
    let assignment = load_match_assignment(pool, match_id)
        .await
        .map_err(MatchCommandAccessError::Assignment)?
        .ok_or(MatchCommandAccessError::MatchAssignmentMissing { match_id })?;
    let lineup = load_draft_lineup(pool, lineup_id)
        .await
        .map_err(MatchCommandAccessError::Lineup)?
        .ok_or(MatchCommandAccessError::DraftLineupMissing { lineup_id })?;
    if lineup.fixture_id() != assignment.fixture_id() {
        return Err(MatchCommandAccessError::LineupFixtureMismatch {
            lineup_id,
            match_id,
        });
    }
    LineupSubmittedPayload::try_new(lineup.id(), lineup.team_id(), lineup.entries().to_vec())
        .map_err(MatchCommandAccessError::InvalidLineup)
}
