use crate::command_results::{CommandResultRepositoryError, load_terminal_command_result};
use football_application::commands::{
    CommandAdmissionPort, CommandAdmissionRequest, CommandAdmissionResult,
};
use football_domain::{CommandId, CommandStatus, UtcInstant};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction};
use std::fmt;
use std::future::Future;
use time::OffsetDateTime;
use uuid::Uuid;

/// PostgreSQL adapter for the application-owned command-admission port.
#[derive(Debug, Clone, Copy)]
pub struct PostgresCommandAdmission<'a> {
    pool: &'a PgPool,
}

impl<'a> PostgresCommandAdmission<'a> {
    pub const fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug)]
pub enum CommandAdmissionError {
    Database(SqlxError),
    TerminalResult(CommandResultRepositoryError),
    InvalidStoredStatus {
        command_id: CommandId,
        value: String,
    },
    InvalidStoredLease {
        command_id: CommandId,
    },
    FingerprintMismatch {
        command_id: CommandId,
    },
    CommandIdentityMismatch {
        expected: CommandId,
        actual: CommandId,
    },
    CommandInProgress {
        command_id: CommandId,
    },
    ReconciliationRequired {
        command_id: CommandId,
    },
}

/// Failure while locking and validating a claimed command before domain
/// execution. This is deliberately separate from admission: admission
/// establishes ownership, while this guard proves that ownership still holds
/// inside the later domain transaction.
#[derive(Debug)]
pub enum CommandExecutionLeaseError {
    Database(SqlxError),
    CommandNotFound {
        command_id: CommandId,
    },
    InvalidStoredStatus {
        command_id: CommandId,
        value: String,
    },
    InvalidStoredLease {
        command_id: CommandId,
    },
    CommandNotProcessing {
        command_id: CommandId,
        status: String,
    },
    CommandNotOwnedOrLeaseExpired {
        command_id: CommandId,
    },
}

/// Failure while recording that an owned command can be retried after an
/// infrastructure failure. The guarded update never changes a command whose
/// processing lease is no longer owned by the caller.
#[derive(Debug)]
pub enum CommandRetryError {
    Database(SqlxError),
    CommandNotOwnedOrLeaseExpired { command_id: CommandId },
}

impl fmt::Display for CommandRetryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "command retry state update failed: {error}")
            }
            Self::CommandNotOwnedOrLeaseExpired { command_id } => write!(
                formatter,
                "command {command_id} is not owned by the active processing lease"
            ),
        }
    }
}

impl std::error::Error for CommandRetryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::CommandNotOwnedOrLeaseExpired { .. } => None,
        }
    }
}

impl CommandRetryError {
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::Database(_) => ApplicationError::new(
                ErrorCode::DependencyDatabaseUnavailable,
                "the command retry dependency is unavailable",
            ),
            Self::CommandNotOwnedOrLeaseExpired { command_id } => ApplicationError::new(
                ErrorCode::ConflictOperatorLease,
                "the command processing lease is not executable",
            )
            .with_detail("command_id", command_id.to_string()),
        }
    }
}

impl fmt::Display for CommandExecutionLeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "command execution lease query failed: {error}")
            }
            Self::CommandNotFound { command_id } => {
                write!(formatter, "command {command_id} was not found")
            }
            Self::InvalidStoredStatus { command_id, value } => write!(
                formatter,
                "command {command_id} has invalid stored status {value:?}"
            ),
            Self::InvalidStoredLease { command_id } => {
                write!(
                    formatter,
                    "command {command_id} has an invalid processing lease"
                )
            }
            Self::CommandNotProcessing { command_id, status } => write!(
                formatter,
                "command {command_id} cannot execute from status {status:?}"
            ),
            Self::CommandNotOwnedOrLeaseExpired { command_id } => write!(
                formatter,
                "command {command_id} is not owned by an unexpired processing lease"
            ),
        }
    }
}

impl std::error::Error for CommandExecutionLeaseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::CommandNotFound { .. }
            | Self::InvalidStoredStatus { .. }
            | Self::InvalidStoredLease { .. }
            | Self::CommandNotProcessing { .. }
            | Self::CommandNotOwnedOrLeaseExpired { .. } => None,
        }
    }
}

impl CommandExecutionLeaseError {
    /// Maps lease-guard failures to stable transport-neutral codes without
    /// exposing SQL errors or malformed persisted values.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::Database(_) => ApplicationError::new(
                ErrorCode::DependencyDatabaseUnavailable,
                "the command execution dependency is unavailable",
            ),
            Self::CommandNotFound { command_id } => {
                ApplicationError::new(ErrorCode::ResourceNotFound, "the command was not found")
                    .with_detail("command_id", command_id.to_string())
            }
            Self::CommandNotOwnedOrLeaseExpired { command_id }
            | Self::CommandNotProcessing { command_id, .. } => ApplicationError::new(
                ErrorCode::ConflictOperatorLease,
                "the command processing lease is not executable",
            )
            .with_detail("command_id", command_id.to_string()),
            Self::InvalidStoredStatus { .. } | Self::InvalidStoredLease { .. } => {
                ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command execution store returned invalid persisted state",
                )
            }
        }
    }
}

impl CommandAdmissionError {
    /// Maps adapter failures to the stable transport-neutral error taxonomy.
    ///
    /// Database errors and malformed persisted rows deliberately lose their
    /// SQL/storage details at this boundary. Only the safe internal command ID
    /// is retained as correlation metadata for idempotency conflicts.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::FingerprintMismatch { command_id } => ApplicationError::new(
                ErrorCode::IdempotencyFingerprintMismatch,
                "the command ID is already bound to a different request",
            )
            .with_detail("command_id", command_id.to_string()),
            Self::CommandIdentityMismatch { .. } => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the command admission identity does not match the stored command",
            ),
            Self::CommandInProgress { command_id } => ApplicationError::new(
                ErrorCode::IdempotencyCommandInProgress,
                "the command is already being processed",
            )
            .with_detail("command_id", command_id.to_string()),
            Self::ReconciliationRequired { command_id } => ApplicationError::new(
                ErrorCode::IdempotencyReconciliationRequired,
                "the command has canonical event evidence and must be reconciled",
            )
            .with_detail("command_id", command_id.to_string()),
            Self::Database(_) => ApplicationError::new(
                ErrorCode::DependencyDatabaseUnavailable,
                "the command admission dependency is unavailable",
            ),
            Self::TerminalResult(error) => match error {
                CommandResultRepositoryError::Database(_) => ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the command result dependency is unavailable",
                ),
                _ => ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command store returned an invalid terminal result",
                ),
            },
            Self::InvalidStoredStatus { .. } | Self::InvalidStoredLease { .. } => {
                ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command admission store returned invalid persisted state",
                )
            }
        }
    }
}

impl fmt::Display for CommandAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(
                    formatter,
                    "command admission database operation failed: {error}"
                )
            }
            Self::TerminalResult(error) => {
                write!(
                    formatter,
                    "command terminal result operation failed: {error}"
                )
            }
            Self::InvalidStoredStatus { command_id, value } => {
                write!(
                    formatter,
                    "command {command_id} has invalid stored status {value:?}"
                )
            }
            Self::InvalidStoredLease { command_id } => {
                write!(
                    formatter,
                    "command {command_id} has an invalid processing lease"
                )
            }
            Self::FingerprintMismatch { command_id } => {
                write!(
                    formatter,
                    "command {command_id} has a different request fingerprint"
                )
            }
            Self::CommandIdentityMismatch { expected, actual } => write!(
                formatter,
                "command admission identity mismatch: expected {expected}, stored {actual}"
            ),
            Self::CommandInProgress { command_id } => {
                write!(formatter, "command {command_id} is already processing")
            }
            Self::ReconciliationRequired { command_id } => write!(
                formatter,
                "command {command_id} has canonical event evidence and requires reconciliation"
            ),
        }
    }
}

impl std::error::Error for CommandAdmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::TerminalResult(error) => Some(error),
            Self::InvalidStoredStatus { .. }
            | Self::InvalidStoredLease { .. }
            | Self::FingerprintMismatch { .. }
            | Self::CommandIdentityMismatch { .. }
            | Self::CommandInProgress { .. }
            | Self::ReconciliationRequired { .. } => None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct CommandRow {
    id: Uuid,
    request_fingerprint: Vec<u8>,
    status: String,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<OffsetDateTime>,
}

#[derive(Debug, sqlx::FromRow)]
struct ProcessingLeaseRow {
    status: String,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<OffsetDateTime>,
}

/// Locks one claimed command row and verifies that the supplied worker still
/// owns a live processing lease. The caller must invoke this inside the same
/// transaction that will load, execute, and commit the domain command.
pub async fn verify_processing_lease(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
    processing_owner: Uuid,
    now_utc: UtcInstant,
) -> Result<(), CommandExecutionLeaseError> {
    let row = sqlx::query_as::<_, ProcessingLeaseRow>(
        r#"
        SELECT status, processing_owner, processing_expires_at
        FROM commands
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(command_id.as_uuid())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(CommandExecutionLeaseError::Database)?
    .ok_or(CommandExecutionLeaseError::CommandNotFound { command_id })?;

    validate_processing_lease(
        command_id,
        row.status,
        row.processing_owner,
        row.processing_expires_at,
        processing_owner,
        now_utc.as_offset_datetime(),
    )
}

/// Marks an owned command as retryable after an infrastructure failure. The
/// caller owns the transaction and must use this only when no canonical state
/// was written; if canonical state may already have committed, the caller
/// must roll back/reconcile instead of committing this status.
pub async fn mark_command_failed_retryable(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
    processing_owner: Uuid,
    failed_at: UtcInstant,
) -> Result<(), CommandRetryError> {
    let rows = sqlx::query(
        r#"
        UPDATE commands
        SET status = 'failed_retryable',
            processing_owner = NULL,
            processing_expires_at = NULL
        WHERE id = $1
          AND status = 'processing'
          AND processing_owner = $2
          AND processing_expires_at > $3
        "#,
    )
    .bind(command_id.as_uuid())
    .bind(processing_owner)
    .bind(failed_at.as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map_err(CommandRetryError::Database)?;

    if rows.rows_affected() != 1 {
        return Err(CommandRetryError::CommandNotOwnedOrLeaseExpired { command_id });
    }

    Ok(())
}

fn validate_processing_lease(
    command_id: CommandId,
    status: String,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<OffsetDateTime>,
    expected_owner: Uuid,
    now_utc: OffsetDateTime,
) -> Result<(), CommandExecutionLeaseError> {
    match CommandStatus::try_from_persisted_str(&status) {
        Some(CommandStatus::Processing) => {}
        Some(CommandStatus::Received)
        | Some(CommandStatus::Accepted)
        | Some(CommandStatus::Rejected)
        | Some(CommandStatus::FailedRetryable) => {
            return Err(CommandExecutionLeaseError::CommandNotProcessing { command_id, status });
        }
        None => {
            return Err(CommandExecutionLeaseError::InvalidStoredStatus {
                command_id,
                value: status,
            });
        }
    }

    let (Some(actual_owner), Some(expiry)) = (processing_owner, processing_expires_at) else {
        return Err(CommandExecutionLeaseError::InvalidStoredLease { command_id });
    };

    if actual_owner != expected_owner || expiry <= now_utc {
        return Err(CommandExecutionLeaseError::CommandNotOwnedOrLeaseExpired { command_id });
    }

    Ok(())
}

/// Admit or reclaim one command while holding its client-command row lock.
pub async fn admit_command(
    pool: &PgPool,
    request: &CommandAdmissionRequest,
) -> Result<CommandAdmissionResult, CommandAdmissionError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(CommandAdmissionError::Database)?;

    let identity = request.identity();
    let fingerprint = identity.request_fingerprint().as_bytes().to_vec();
    sqlx::query(
        r#"
        INSERT INTO commands (
            id,
            client_command_id,
            request_fingerprint,
            status
        )
        VALUES ($1, $2, $3, 'received')
        ON CONFLICT (client_command_id) DO NOTHING
        "#,
    )
    .bind(identity.command_id().as_uuid())
    .bind(identity.client_command_id().as_uuid())
    .bind(fingerprint)
    .execute(&mut *transaction)
    .await
    .map_err(CommandAdmissionError::Database)?;

    let row = sqlx::query_as::<_, CommandRow>(
        r#"
        SELECT id, request_fingerprint, status, processing_owner, processing_expires_at
        FROM commands
        WHERE client_command_id = $1
        FOR UPDATE
        "#,
    )
    .bind(identity.client_command_id().as_uuid())
    .fetch_one(&mut *transaction)
    .await
    .map_err(CommandAdmissionError::Database)?;

    let command_id = CommandId::from_uuid(row.id);
    let expected_fingerprint = identity.request_fingerprint().as_bytes();
    if row.request_fingerprint.as_slice() != expected_fingerprint {
        return Err(CommandAdmissionError::FingerprintMismatch { command_id });
    }
    if command_id != identity.command_id() {
        return Err(CommandAdmissionError::CommandIdentityMismatch {
            expected: identity.command_id(),
            actual: command_id,
        });
    }
    let status = parse_status(command_id, row.status)?;

    match status {
        CommandStatus::Accepted | CommandStatus::Rejected => {
            transaction
                .commit()
                .await
                .map_err(CommandAdmissionError::Database)?;
            let result = load_terminal_command_result(pool, command_id)
                .await
                .map_err(CommandAdmissionError::TerminalResult)?
                .into();
            Ok(CommandAdmissionResult::ExistingTerminal { result })
        }
        CommandStatus::Processing => {
            let expiry = processing_lease_expiry(
                command_id,
                row.processing_owner,
                row.processing_expires_at,
            )?;
            if expiry > request.now_utc().as_offset_datetime() {
                return Err(CommandAdmissionError::CommandInProgress { command_id });
            }

            let has_canonical_event = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(
                     SELECT 1
                     FROM match_events
                     WHERE causation_command_id = $1
                 )",
            )
            .bind(command_id.as_uuid())
            .fetch_one(&mut *transaction)
            .await
            .map_err(CommandAdmissionError::Database)?;
            if has_canonical_event {
                return Err(CommandAdmissionError::ReconciliationRequired { command_id });
            }

            claim_command(&mut transaction, request, command_id).await?;
            transaction
                .commit()
                .await
                .map_err(CommandAdmissionError::Database)?;
            Ok(CommandAdmissionResult::Claimed { command_id })
        }
        CommandStatus::Received | CommandStatus::FailedRetryable => {
            claim_command(&mut transaction, request, command_id).await?;
            transaction
                .commit()
                .await
                .map_err(CommandAdmissionError::Database)?;
            Ok(CommandAdmissionResult::Claimed { command_id })
        }
    }
}

impl CommandAdmissionPort for PostgresCommandAdmission<'_> {
    type Error = CommandAdmissionError;

    fn admit<'a>(
        &'a self,
        request: &'a CommandAdmissionRequest,
    ) -> impl Future<Output = Result<CommandAdmissionResult, Self::Error>> + Send + 'a {
        admit_command(self.pool, request)
    }
}

fn processing_lease_expiry(
    command_id: CommandId,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<OffsetDateTime>,
) -> Result<OffsetDateTime, CommandAdmissionError> {
    match (processing_owner, processing_expires_at) {
        (Some(_), Some(expiry)) => Ok(expiry),
        _ => Err(CommandAdmissionError::InvalidStoredLease { command_id }),
    }
}

async fn claim_command(
    transaction: &mut Transaction<'_, Postgres>,
    request: &CommandAdmissionRequest,
    command_id: CommandId,
) -> Result<(), CommandAdmissionError> {
    sqlx::query(
        r#"
        UPDATE commands
        SET status = 'processing',
            processing_owner = $2,
            processing_expires_at = $3,
            started_at = $4
        WHERE id = $1
        "#,
    )
    .bind(command_id.as_uuid())
    .bind(request.processing_owner())
    .bind(request.processing_expires_at().as_offset_datetime())
    .bind(request.now_utc().as_offset_datetime())
    .execute(&mut **transaction)
    .await
    .map(|_| ())
    .map_err(CommandAdmissionError::Database)
}

fn parse_status(
    command_id: CommandId,
    value: String,
) -> Result<CommandStatus, CommandAdmissionError> {
    CommandStatus::try_from_persisted_str(&value)
        .ok_or(CommandAdmissionError::InvalidStoredStatus { command_id, value })
}

#[cfg(test)]
mod tests {
    use football_domain::{CommandId, CommandStatus, CommandStatusTransitionError};
    use football_platform::{ErrorCategory, ErrorCode};
    use uuid::Uuid;

    use super::{
        CommandAdmissionError, CommandExecutionLeaseError, CommandResultRepositoryError,
        CommandRetryError, parse_status, processing_lease_expiry, validate_processing_lease,
    };
    use time::OffsetDateTime;

    fn command_id() -> CommandId {
        CommandId::from_uuid(
            Uuid::parse_str("019fe2e0-3bc5-7cf0-b3d2-0728c077a930")
                .expect("command UUID should be valid"),
        )
    }

    #[test]
    fn parses_only_the_persisted_command_status_values() {
        let id = command_id();

        assert_eq!(
            parse_status(id, "received".to_owned()).unwrap(),
            CommandStatus::Received
        );
        assert_eq!(
            parse_status(id, "processing".to_owned()).unwrap(),
            CommandStatus::Processing
        );
        assert_eq!(
            parse_status(id, "accepted".to_owned()).unwrap(),
            CommandStatus::Accepted
        );
        assert_eq!(
            parse_status(id, "rejected".to_owned()).unwrap(),
            CommandStatus::Rejected
        );
        assert_eq!(
            parse_status(id, "failed_retryable".to_owned()).unwrap(),
            CommandStatus::FailedRetryable
        );
    }

    #[test]
    fn rejects_unknown_persisted_status_without_guessing() {
        let error = parse_status(command_id(), "unknown".to_owned()).unwrap_err();

        assert!(matches!(
            error,
            CommandAdmissionError::InvalidStoredStatus { value, .. } if value == "unknown"
        ));
        assert_eq!(
            CommandStatus::Rejected
                .transition_to(CommandStatus::Processing)
                .unwrap_err(),
            CommandStatusTransitionError {
                from: CommandStatus::Rejected,
                to: CommandStatus::Processing,
            }
        );
    }

    #[test]
    fn rejects_a_processing_row_with_an_incomplete_lease() {
        let command_id = command_id();
        let expiry = OffsetDateTime::from_unix_timestamp(2_030).unwrap();
        let missing_owner = processing_lease_expiry(command_id, None, Some(expiry));
        let missing_expiry = processing_lease_expiry(command_id, Some(Uuid::nil()), None);

        assert!(matches!(
            missing_owner,
            Err(CommandAdmissionError::InvalidStoredLease { command_id: actual })
                if actual == command_id
        ));
        assert!(matches!(
            missing_expiry,
            Err(CommandAdmissionError::InvalidStoredLease { command_id: actual })
                if actual == command_id
        ));
        assert_eq!(
            processing_lease_expiry(command_id, Some(Uuid::nil()), Some(expiry)).unwrap(),
            expiry
        );
    }

    #[test]
    fn validates_only_a_live_lease_owned_by_the_execution_worker() {
        let command_id = command_id();
        let owner = Uuid::parse_str("019fe2e0-3bc5-7cf1-b3d2-0728c077a930").unwrap();
        let other_owner = Uuid::parse_str("019fe2e0-3bc5-7cf2-b3d2-0728c077a930").unwrap();
        let now = OffsetDateTime::from_unix_timestamp(2_000).unwrap();
        let expiry = OffsetDateTime::from_unix_timestamp(2_030).unwrap();

        assert!(
            validate_processing_lease(
                command_id,
                "processing".to_owned(),
                Some(owner),
                Some(expiry),
                owner,
                now,
            )
            .is_ok()
        );

        assert!(matches!(
            validate_processing_lease(
                command_id,
                "processing".to_owned(),
                Some(other_owner),
                Some(expiry),
                owner,
                now,
            ),
            Err(CommandExecutionLeaseError::CommandNotOwnedOrLeaseExpired { command_id: actual })
                if actual == command_id
        ));

        assert!(matches!(
            validate_processing_lease(
                command_id,
                "processing".to_owned(),
                Some(owner),
                Some(now),
                owner,
                now,
            ),
            Err(CommandExecutionLeaseError::CommandNotOwnedOrLeaseExpired { command_id: actual })
                if actual == command_id
        ));
    }

    #[test]
    fn maps_execution_lease_failures_to_stable_codes_without_storage_details() {
        let lease_conflict = CommandExecutionLeaseError::CommandNotOwnedOrLeaseExpired {
            command_id: command_id(),
        }
        .to_application_error();
        assert_eq!(lease_conflict.code(), ErrorCode::ConflictOperatorLease);
        assert_eq!(lease_conflict.category(), ErrorCategory::Conflict);
        assert_eq!(
            lease_conflict.details().get("command_id"),
            Some(&command_id().to_string())
        );

        let invalid_state = CommandExecutionLeaseError::InvalidStoredStatus {
            command_id: command_id(),
            value: "corrupt.persisted.status".to_owned(),
        }
        .to_application_error();
        assert_eq!(invalid_state.code(), ErrorCode::InternalUnexpected);
        assert!(invalid_state.details().is_empty());
        assert!(
            !invalid_state
                .to_string()
                .contains("corrupt.persisted.status")
        );
    }

    #[test]
    fn maps_missing_execution_command_to_a_safe_not_found_error() {
        let error = CommandExecutionLeaseError::CommandNotFound {
            command_id: command_id(),
        }
        .to_application_error();

        assert_eq!(error.code(), ErrorCode::ResourceNotFound);
        assert_eq!(error.category(), ErrorCategory::NotFound);
        assert_eq!(
            error.details().get("command_id"),
            Some(&command_id().to_string())
        );
    }

    #[test]
    fn maps_retryable_state_update_lease_loss_to_a_conflict() {
        let error = CommandRetryError::CommandNotOwnedOrLeaseExpired {
            command_id: command_id(),
        }
        .to_application_error();

        assert_eq!(error.code(), ErrorCode::ConflictOperatorLease);
        assert_eq!(error.category(), ErrorCategory::Conflict);
        assert_eq!(
            error.details().get("command_id"),
            Some(&command_id().to_string())
        );
    }

    #[test]
    fn rejects_non_processing_and_unknown_command_states_before_execution() {
        let command_id = command_id();
        let owner = Uuid::nil();
        let expiry = OffsetDateTime::from_unix_timestamp(2_030).unwrap();
        let now = OffsetDateTime::from_unix_timestamp(2_000).unwrap();

        assert!(matches!(
            validate_processing_lease(
                command_id,
                "accepted".to_owned(),
                None,
                None,
                owner,
                now,
            ),
            Err(CommandExecutionLeaseError::CommandNotProcessing { status, .. })
                if status == "accepted"
        ));
        assert!(matches!(
            validate_processing_lease(
                command_id,
                "unexpected".to_owned(),
                Some(owner),
                Some(expiry),
                owner,
                now,
            ),
            Err(CommandExecutionLeaseError::InvalidStoredStatus { value, .. })
                if value == "unexpected"
        ));
    }

    #[test]
    fn maps_fingerprint_conflict_to_a_final_safe_idempotency_error() {
        let error = CommandAdmissionError::FingerprintMismatch {
            command_id: command_id(),
        }
        .to_application_error();

        assert_eq!(error.code(), ErrorCode::IdempotencyFingerprintMismatch);
        assert_eq!(error.category(), ErrorCategory::Idempotency);
        assert!(!error.retryable());
        assert_eq!(
            error.details().get("command_id"),
            Some(&command_id().to_string())
        );
        assert!(!error.message().contains("fingerprint"));
    }

    #[test]
    fn maps_internal_command_identity_conflicts_to_a_safe_internal_error() {
        let error = CommandAdmissionError::CommandIdentityMismatch {
            expected: command_id(),
            actual: CommandId::from_uuid(
                Uuid::parse_str("019fe2e0-3bc5-7cf1-b3d2-0728c077a930")
                    .expect("stored command UUID should be valid"),
            ),
        }
        .to_application_error();

        assert_eq!(error.code(), ErrorCode::InternalUnexpected);
        assert!(error.details().is_empty());
        assert!(!error.to_string().contains(&command_id().to_string()));
    }

    #[test]
    fn maps_active_lease_to_a_final_safe_idempotency_error() {
        let error = CommandAdmissionError::CommandInProgress {
            command_id: command_id(),
        }
        .to_application_error();

        assert_eq!(error.code(), ErrorCode::IdempotencyCommandInProgress);
        assert_eq!(error.category(), ErrorCategory::Idempotency);
        assert!(!error.retryable());
        assert_eq!(
            error.details().get("command_id"),
            Some(&command_id().to_string())
        );
    }

    #[test]
    fn maps_canonical_event_evidence_to_reconciliation_required() {
        let error = CommandAdmissionError::ReconciliationRequired {
            command_id: command_id(),
        }
        .to_application_error();

        assert_eq!(error.code(), ErrorCode::IdempotencyReconciliationRequired);
        assert_eq!(error.category(), ErrorCategory::Idempotency);
        assert!(!error.retryable());
        assert_eq!(
            error.details().get("command_id"),
            Some(&command_id().to_string())
        );
    }

    #[test]
    fn hides_database_and_persisted_state_details_from_stable_errors() {
        let persisted_state_error = CommandAdmissionError::InvalidStoredLease {
            command_id: command_id(),
        }
        .to_application_error();
        assert_eq!(persisted_state_error.code(), ErrorCode::InternalUnexpected);
        assert!(persisted_state_error.details().is_empty());
        assert!(
            !persisted_state_error
                .to_string()
                .contains(&command_id().to_string())
        );
    }

    #[test]
    fn maps_terminal_result_storage_failures_without_exposing_storage_details() {
        let missing_result =
            CommandAdmissionError::TerminalResult(CommandResultRepositoryError::CommandNotFound {
                command_id: command_id(),
            })
            .to_application_error();
        assert_eq!(missing_result.code(), ErrorCode::InternalUnexpected);
        assert!(missing_result.details().is_empty());
        assert_eq!(
            missing_result.message(),
            "the command store returned an invalid terminal result"
        );

        let database_failure = CommandAdmissionError::TerminalResult(
            CommandResultRepositoryError::Database(sqlx::Error::RowNotFound),
        )
        .to_application_error();
        assert_eq!(
            database_failure.code(),
            ErrorCode::DependencyDatabaseUnavailable
        );
        assert!(database_failure.details().is_empty());
    }
}
