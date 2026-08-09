use football_application::commands::PersistedCommandResult;
use football_domain::{AggregateVersion, CommandId, CommandStatus, MatchId, UtcInstant};
use football_platform::ErrorCode;
use serde_json::Value;
use sqlx::{Error as SqlxError, PgPool, Postgres, Transaction, types::Json};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

/// Terminal result data written together with the claimed command transition.
///
/// The result value is intentionally an opaque JSON value at this adapter
/// boundary. The application/domain layer owns its typed command result, while
/// the database stores the exact already-encoded value for idempotent replay.
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalCommandResult {
    Accepted {
        aggregate_id: MatchId,
        final_aggregate_version: AggregateVersion,
        event_count: u64,
        result_payload: Value,
    },
    Rejected {
        aggregate_id: MatchId,
        aggregate_version: AggregateVersion,
        rejection_code: ErrorCode,
        rejection_details: Value,
    },
}

/// Application terminal-result metadata prepared for the infrastructure
/// transaction writer. The conversion is strict about persisted rejection
/// codes and preserves command/completion metadata separately from the opaque
/// result body.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCommandWrite {
    command_id: CommandId,
    completed_at: UtcInstant,
    result: TerminalCommandResult,
}

impl TerminalCommandWrite {
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub const fn completed_at(&self) -> UtcInstant {
        self.completed_at
    }

    pub const fn result(&self) -> &TerminalCommandResult {
        &self.result
    }
}

/// Raw terminal result loaded from the command store.
///
/// The payload remains JSON until the application knows which command schema
/// owns it. No database value is silently converted into a different command
/// result shape here.
#[derive(Debug, Clone, PartialEq)]
pub enum StoredTerminalCommandResult {
    Accepted {
        command_id: CommandId,
        aggregate_id: MatchId,
        completed_at: UtcInstant,
        final_aggregate_version: AggregateVersion,
        event_count: u64,
        result_payload: Value,
    },
    Rejected {
        command_id: CommandId,
        aggregate_id: MatchId,
        completed_at: UtcInstant,
        aggregate_version: AggregateVersion,
        rejection_code: String,
        rejection_details: Value,
    },
}

impl StoredTerminalCommandResult {
    pub const fn command_id(&self) -> CommandId {
        match self {
            Self::Accepted { command_id, .. } | Self::Rejected { command_id, .. } => *command_id,
        }
    }

    pub const fn aggregate_id(&self) -> MatchId {
        match self {
            Self::Accepted { aggregate_id, .. } | Self::Rejected { aggregate_id, .. } => {
                *aggregate_id
            }
        }
    }

    pub const fn status(&self) -> CommandStatus {
        match self {
            Self::Accepted { .. } => CommandStatus::Accepted,
            Self::Rejected { .. } => CommandStatus::Rejected,
        }
    }
}

impl From<StoredTerminalCommandResult> for PersistedCommandResult {
    fn from(result: StoredTerminalCommandResult) -> Self {
        match result {
            StoredTerminalCommandResult::Accepted {
                command_id,
                aggregate_id,
                completed_at,
                final_aggregate_version,
                event_count,
                result_payload,
            } => Self::Accepted {
                command_id,
                aggregate_id,
                completed_at,
                final_aggregate_version,
                event_count,
                result_payload,
            },
            StoredTerminalCommandResult::Rejected {
                command_id,
                aggregate_id,
                completed_at,
                aggregate_version,
                rejection_code,
                rejection_details,
            } => Self::Rejected {
                command_id,
                aggregate_id,
                completed_at,
                aggregate_version,
                rejection_code,
                rejection_details,
            },
        }
    }
}

#[derive(Debug)]
pub enum CommandResultRepositoryError {
    Database(SqlxError),
    CommandNotFound {
        command_id: CommandId,
    },
    CommandNotTerminal {
        command_id: CommandId,
        status: String,
    },
    InvalidStoredStatus {
        command_id: CommandId,
        value: String,
    },
    InvalidStoredTerminalResult {
        command_id: CommandId,
        field: &'static str,
    },
    InvalidRejectionCode {
        command_id: CommandId,
        value: String,
    },
    RetryableRejectionCode {
        command_id: CommandId,
        value: String,
    },
    CannotPersistAggregateVersion {
        command_id: CommandId,
        value: u64,
    },
    CannotPersistEventCount {
        command_id: CommandId,
        value: u64,
    },
    InvalidRejectionDetails {
        command_id: CommandId,
    },
    TerminalResultEventMismatch {
        command_id: CommandId,
    },
    CommandNotOwnedOrLeaseExpired {
        command_id: CommandId,
    },
}

impl fmt::Display for CommandResultRepositoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(
                    formatter,
                    "command result database operation failed: {error}"
                )
            }
            Self::CommandNotFound { command_id } => {
                write!(
                    formatter,
                    "terminal result for command {command_id} was not found"
                )
            }
            Self::CommandNotTerminal { command_id, status } => write!(
                formatter,
                "command {command_id} has non-terminal status {status:?}"
            ),
            Self::InvalidStoredStatus { command_id, value } => write!(
                formatter,
                "command {command_id} has invalid stored status {value:?}"
            ),
            Self::InvalidStoredTerminalResult { command_id, field } => write!(
                formatter,
                "command {command_id} has invalid stored terminal result field {field}"
            ),
            Self::InvalidRejectionCode { command_id, value } => write!(
                formatter,
                "command {command_id} has unknown rejection code {value:?}"
            ),
            Self::RetryableRejectionCode { command_id, value } => write!(
                formatter,
                "command {command_id} cannot persist retryable rejection code {value:?}"
            ),
            Self::CannotPersistAggregateVersion { command_id, value } => write!(
                formatter,
                "command {command_id} aggregate version {value} does not fit PostgreSQL BIGINT"
            ),
            Self::CannotPersistEventCount { command_id, value } => write!(
                formatter,
                "command {command_id} event count {value} does not fit PostgreSQL BIGINT"
            ),
            Self::InvalidRejectionDetails { command_id } => write!(
                formatter,
                "command {command_id} rejection details must be a JSON object"
            ),
            Self::TerminalResultEventMismatch { command_id } => write!(
                formatter,
                "command {command_id} terminal result does not match canonical event evidence"
            ),
            Self::CommandNotOwnedOrLeaseExpired { command_id } => write!(
                formatter,
                "command {command_id} is not owned by the active processing lease"
            ),
        }
    }
}

impl std::error::Error for CommandResultRepositoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::CommandNotFound { .. }
            | Self::CommandNotTerminal { .. }
            | Self::InvalidStoredStatus { .. }
            | Self::InvalidStoredTerminalResult { .. }
            | Self::InvalidRejectionCode { .. }
            | Self::RetryableRejectionCode { .. }
            | Self::CannotPersistAggregateVersion { .. }
            | Self::CannotPersistEventCount { .. }
            | Self::InvalidRejectionDetails { .. }
            | Self::TerminalResultEventMismatch { .. }
            | Self::CommandNotOwnedOrLeaseExpired { .. } => None,
        }
    }
}

impl TryFrom<PersistedCommandResult> for TerminalCommandWrite {
    type Error = CommandResultRepositoryError;

    fn try_from(result: PersistedCommandResult) -> Result<Self, Self::Error> {
        match result {
            PersistedCommandResult::Accepted {
                command_id,
                completed_at,
                aggregate_id,
                final_aggregate_version,
                event_count,
                result_payload,
            } => Ok(Self {
                command_id,
                completed_at,
                result: TerminalCommandResult::Accepted {
                    aggregate_id,
                    final_aggregate_version,
                    event_count,
                    result_payload,
                },
            }),
            PersistedCommandResult::Rejected {
                command_id,
                completed_at,
                aggregate_id,
                aggregate_version,
                rejection_code,
                rejection_details,
            } => {
                let rejection_code = ErrorCode::try_from_str(&rejection_code).ok_or_else(|| {
                    CommandResultRepositoryError::InvalidRejectionCode {
                        command_id,
                        value: rejection_code.clone(),
                    }
                })?;
                if rejection_code.retryable() {
                    return Err(CommandResultRepositoryError::RetryableRejectionCode {
                        command_id,
                        value: rejection_code.as_str().to_owned(),
                    });
                }

                Ok(Self {
                    command_id,
                    completed_at,
                    result: TerminalCommandResult::Rejected {
                        aggregate_id,
                        aggregate_version,
                        rejection_code,
                        rejection_details,
                    },
                })
            }
        }
    }
}

#[derive(Debug)]
struct PersistedTerminalResult {
    status: &'static str,
    aggregate_id: Uuid,
    final_aggregate_version: i64,
    event_count: Option<i64>,
    result_payload: Option<Json<Value>>,
    rejection_code: Option<&'static str>,
    rejection_details: Option<Json<Value>>,
}

#[derive(Debug, sqlx::FromRow)]
struct TerminalCommandEventEvidenceRow {
    command_event_count: i64,
    command_match_event_count: i64,
    command_min_event_index: Option<i64>,
    command_max_event_index: Option<i64>,
    command_max_version: Option<i64>,
    command_match_max_version: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalCommandEventEvidence {
    command_event_count: u64,
    command_match_event_count: u64,
    command_min_event_index: Option<u64>,
    command_max_event_index: Option<u64>,
    command_max_version: Option<u64>,
    command_match_max_version: Option<u64>,
    aggregate_max_version: Option<u64>,
}

#[derive(Debug, sqlx::FromRow)]
struct TerminalCommandRow {
    id: Uuid,
    aggregate_id: Option<Uuid>,
    status: String,
    completed_at: Option<OffsetDateTime>,
    final_aggregate_version: Option<i64>,
    event_count: Option<i64>,
    result_payload: Option<Json<Value>>,
    rejection_code: Option<String>,
    rejection_details: Option<Json<Value>>,
}

impl TryFrom<TerminalCommandRow> for StoredTerminalCommandResult {
    type Error = CommandResultRepositoryError;

    fn try_from(row: TerminalCommandRow) -> Result<Self, Self::Error> {
        let command_id = CommandId::from_uuid(row.id);
        match CommandStatus::try_from_persisted_str(&row.status) {
            Some(CommandStatus::Accepted) => {
                let aggregate_id =
                    required_stored_field(command_id, row.aggregate_id, "aggregate_id")?;
                let completed_at =
                    required_stored_field(command_id, row.completed_at, "completed_at")?;
                let final_aggregate_version = non_negative_stored_value(
                    command_id,
                    required_stored_field(
                        command_id,
                        row.final_aggregate_version,
                        "final_aggregate_version",
                    )?,
                    "final_aggregate_version",
                )?;
                let event_count = non_negative_stored_value(
                    command_id,
                    required_stored_field(command_id, row.event_count, "event_count")?,
                    "event_count",
                )?;
                let result_payload =
                    required_stored_field(command_id, row.result_payload, "result_payload")?;
                if row.rejection_code.is_some() || row.rejection_details.is_some() {
                    return Err(invalid_stored_field(command_id, "rejection_fields"));
                }

                Ok(Self::Accepted {
                    command_id,
                    aggregate_id: MatchId::from_uuid(aggregate_id),
                    completed_at: UtcInstant::from_offset_datetime(completed_at),
                    final_aggregate_version: AggregateVersion::from_value(final_aggregate_version),
                    event_count,
                    result_payload: result_payload.0,
                })
            }
            Some(CommandStatus::Rejected) => {
                let aggregate_id =
                    required_stored_field(command_id, row.aggregate_id, "aggregate_id")?;
                let completed_at =
                    required_stored_field(command_id, row.completed_at, "completed_at")?;
                let aggregate_version = non_negative_stored_value(
                    command_id,
                    required_stored_field(
                        command_id,
                        row.final_aggregate_version,
                        "final_aggregate_version",
                    )?,
                    "final_aggregate_version",
                )?;
                let rejection_code =
                    required_stored_field(command_id, row.rejection_code, "rejection_code")?;
                if rejection_code.trim().is_empty() {
                    return Err(invalid_stored_field(command_id, "rejection_code"));
                }
                let parsed_rejection_code =
                    ErrorCode::try_from_str(&rejection_code).ok_or_else(|| {
                        CommandResultRepositoryError::InvalidRejectionCode {
                            command_id,
                            value: rejection_code.clone(),
                        }
                    })?;
                if parsed_rejection_code.retryable() {
                    return Err(CommandResultRepositoryError::RetryableRejectionCode {
                        command_id,
                        value: rejection_code,
                    });
                }
                let rejection_details =
                    required_stored_field(command_id, row.rejection_details, "rejection_details")?;
                if !rejection_details.0.is_object()
                    || row.event_count.is_some()
                    || row.result_payload.is_some()
                {
                    return Err(invalid_stored_field(command_id, "rejection_shape"));
                }

                Ok(Self::Rejected {
                    command_id,
                    aggregate_id: MatchId::from_uuid(aggregate_id),
                    completed_at: UtcInstant::from_offset_datetime(completed_at),
                    aggregate_version: AggregateVersion::from_value(aggregate_version),
                    rejection_code,
                    rejection_details: rejection_details.0,
                })
            }
            Some(CommandStatus::Received)
            | Some(CommandStatus::Processing)
            | Some(CommandStatus::FailedRetryable) => {
                Err(CommandResultRepositoryError::CommandNotTerminal {
                    command_id,
                    status: row.status,
                })
            }
            None => Err(CommandResultRepositoryError::InvalidStoredStatus {
                command_id,
                value: row.status,
            }),
        }
    }
}

fn required_stored_field<T>(
    command_id: CommandId,
    value: Option<T>,
    field: &'static str,
) -> Result<T, CommandResultRepositoryError> {
    value.ok_or_else(|| invalid_stored_field(command_id, field))
}

fn non_negative_stored_value(
    command_id: CommandId,
    value: i64,
    field: &'static str,
) -> Result<u64, CommandResultRepositoryError> {
    u64::try_from(value).map_err(|_| invalid_stored_field(command_id, field))
}

fn invalid_stored_field(
    command_id: CommandId,
    field: &'static str,
) -> CommandResultRepositoryError {
    CommandResultRepositoryError::InvalidStoredTerminalResult { command_id, field }
}

impl PersistedTerminalResult {
    fn try_from_result(
        command_id: CommandId,
        result: &TerminalCommandResult,
    ) -> Result<Self, CommandResultRepositoryError> {
        match result {
            TerminalCommandResult::Accepted {
                aggregate_id,
                final_aggregate_version,
                event_count,
                result_payload,
            } => Ok(Self {
                status: "accepted",
                aggregate_id: aggregate_id.as_uuid(),
                final_aggregate_version: persist_i64(final_aggregate_version.value(), |value| {
                    CommandResultRepositoryError::CannotPersistAggregateVersion {
                        command_id,
                        value,
                    }
                })?,
                event_count: Some(persist_i64(*event_count, |value| {
                    CommandResultRepositoryError::CannotPersistEventCount { command_id, value }
                })?),
                result_payload: Some(Json(result_payload.clone())),
                rejection_code: None,
                rejection_details: None,
            }),
            TerminalCommandResult::Rejected {
                aggregate_id,
                aggregate_version,
                rejection_code,
                rejection_details,
            } => {
                if !rejection_details.is_object() {
                    return Err(CommandResultRepositoryError::InvalidRejectionDetails {
                        command_id,
                    });
                }

                Ok(Self {
                    status: "rejected",
                    aggregate_id: aggregate_id.as_uuid(),
                    final_aggregate_version: persist_i64(aggregate_version.value(), |value| {
                        CommandResultRepositoryError::CannotPersistAggregateVersion {
                            command_id,
                            value,
                        }
                    })?,
                    event_count: None,
                    result_payload: None,
                    rejection_code: Some(rejection_code.as_str()),
                    rejection_details: Some(Json(rejection_details.clone())),
                })
            }
        }
    }
}

fn persist_i64(
    value: u64,
    error: impl FnOnce(u64) -> CommandResultRepositoryError,
) -> Result<i64, CommandResultRepositoryError> {
    i64::try_from(value).map_err(|_| error(value))
}

async fn load_terminal_command_event_evidence(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
    aggregate_id: MatchId,
) -> Result<TerminalCommandEventEvidence, CommandResultRepositoryError> {
    let command_events = sqlx::query_as::<_, TerminalCommandEventEvidenceRow>(
        r#"
        SELECT
            COUNT(*) AS command_event_count,
            COUNT(*) FILTER (WHERE match_id = $2) AS command_match_event_count,
            MIN(event_index) AS command_min_event_index,
            MAX(event_index) AS command_max_event_index,
            MAX(aggregate_version) AS command_max_version,
            MAX(aggregate_version) FILTER (WHERE match_id = $2) AS command_match_max_version
        FROM match_events
        WHERE causation_command_id = $1
        "#,
    )
    .bind(command_id.as_uuid())
    .bind(aggregate_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(CommandResultRepositoryError::Database)?;

    let aggregate_max_version = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT MAX(aggregate_version) FROM match_events WHERE match_id = $1",
    )
    .bind(aggregate_id.as_uuid())
    .fetch_one(&mut **transaction)
    .await
    .map_err(CommandResultRepositoryError::Database)?;

    Ok(TerminalCommandEventEvidence {
        command_event_count: u64::try_from(command_events.command_event_count).map_err(|_| {
            CommandResultRepositoryError::TerminalResultEventMismatch { command_id }
        })?,
        command_match_event_count: u64::try_from(command_events.command_match_event_count)
            .map_err(
                |_| CommandResultRepositoryError::TerminalResultEventMismatch { command_id },
            )?,
        command_min_event_index: command_events
            .command_min_event_index
            .map(u64::try_from)
            .transpose()
            .map_err(
                |_| CommandResultRepositoryError::TerminalResultEventMismatch { command_id },
            )?,
        command_max_event_index: command_events
            .command_max_event_index
            .map(u64::try_from)
            .transpose()
            .map_err(
                |_| CommandResultRepositoryError::TerminalResultEventMismatch { command_id },
            )?,
        command_max_version: command_events
            .command_max_version
            .map(u64::try_from)
            .transpose()
            .map_err(
                |_| CommandResultRepositoryError::TerminalResultEventMismatch { command_id },
            )?,
        command_match_max_version: command_events
            .command_match_max_version
            .map(u64::try_from)
            .transpose()
            .map_err(
                |_| CommandResultRepositoryError::TerminalResultEventMismatch { command_id },
            )?,
        aggregate_max_version: aggregate_max_version
            .map(u64::try_from)
            .transpose()
            .map_err(
                |_| CommandResultRepositoryError::TerminalResultEventMismatch { command_id },
            )?,
    })
}

fn validate_terminal_command_event_evidence(
    command_id: CommandId,
    result: &TerminalCommandResult,
    evidence: TerminalCommandEventEvidence,
) -> Result<(), CommandResultRepositoryError> {
    if evidence.command_event_count != evidence.command_match_event_count {
        return Err(CommandResultRepositoryError::TerminalResultEventMismatch { command_id });
    }

    let command_event_indexes_are_contiguous = if evidence.command_event_count == 0 {
        evidence.command_min_event_index.is_none() && evidence.command_max_event_index.is_none()
    } else {
        evidence.command_min_event_index == Some(0)
            && evidence.command_max_event_index == Some(evidence.command_event_count - 1)
    };
    if !command_event_indexes_are_contiguous {
        return Err(CommandResultRepositoryError::TerminalResultEventMismatch { command_id });
    }

    let aggregate_max_version = evidence.aggregate_max_version.unwrap_or(0);
    let matches_result = match result {
        TerminalCommandResult::Accepted {
            final_aggregate_version,
            event_count,
            ..
        } if *event_count == 0 => {
            evidence.command_event_count == 0
                && final_aggregate_version.value() <= aggregate_max_version
        }
        TerminalCommandResult::Accepted {
            final_aggregate_version,
            event_count,
            ..
        } => {
            evidence.command_event_count == *event_count
                && evidence.command_max_version == Some(final_aggregate_version.value())
                && evidence.command_match_max_version == Some(final_aggregate_version.value())
                && final_aggregate_version.value() <= aggregate_max_version
        }
        TerminalCommandResult::Rejected {
            aggregate_version, ..
        } => {
            evidence.command_event_count == 0 && aggregate_version.value() <= aggregate_max_version
        }
    };

    if matches_result {
        Ok(())
    } else {
        Err(CommandResultRepositoryError::TerminalResultEventMismatch { command_id })
    }
}

/// Persists a terminal command result and releases its processing lease.
///
/// The caller owns the surrounding transaction and must append canonical
/// events, update critical state, write audit/outbox rows, and call this
/// function before committing the same transaction. The guarded update makes
/// an expired or stolen lease unable to finalize the command.
pub async fn persist_terminal_command_result(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
    processing_owner: Uuid,
    completed_at: UtcInstant,
    result: &TerminalCommandResult,
) -> Result<(), CommandResultRepositoryError> {
    let persisted = PersistedTerminalResult::try_from_result(command_id, result)?;
    let aggregate_id = match result {
        TerminalCommandResult::Accepted { aggregate_id, .. }
        | TerminalCommandResult::Rejected { aggregate_id, .. } => *aggregate_id,
    };
    let evidence =
        load_terminal_command_event_evidence(transaction, command_id, aggregate_id).await?;
    validate_terminal_command_event_evidence(command_id, result, evidence)?;
    let rows = sqlx::query(
        r#"
        UPDATE commands
        SET aggregate_id = $2,
            status = $3,
            completed_at = $4,
            final_aggregate_version = $5,
            event_count = $6,
            result_payload = $7,
            rejection_code = $8,
            rejection_details = $9,
            processing_owner = NULL,
            processing_expires_at = NULL
        WHERE id = $1
          AND status = 'processing'
          AND processing_owner = $10
          AND processing_expires_at > $4
        "#,
    )
    .bind(command_id.as_uuid())
    .bind(persisted.aggregate_id)
    .bind(persisted.status)
    .bind(completed_at.as_offset_datetime())
    .bind(persisted.final_aggregate_version)
    .bind(persisted.event_count)
    .bind(persisted.result_payload)
    .bind(persisted.rejection_code)
    .bind(persisted.rejection_details)
    .bind(processing_owner)
    .execute(&mut **transaction)
    .await
    .map_err(CommandResultRepositoryError::Database)?;

    if rows.rows_affected() != 1 {
        return Err(CommandResultRepositoryError::CommandNotOwnedOrLeaseExpired { command_id });
    }

    Ok(())
}

/// Persists an application-encoded terminal result through the strict
/// infrastructure terminal writer. The caller still owns the transaction and
/// processing-owner value.
pub async fn persist_application_command_result(
    transaction: &mut Transaction<'_, Postgres>,
    processing_owner: Uuid,
    result: PersistedCommandResult,
) -> Result<(), CommandResultRepositoryError> {
    let write = TerminalCommandWrite::try_from(result)?;
    persist_terminal_command_result(
        transaction,
        write.command_id(),
        processing_owner,
        write.completed_at(),
        write.result(),
    )
    .await
}

/// Loads and validates one persisted terminal result without decoding its
/// command-specific JSON payload.
pub async fn load_terminal_command_result(
    pool: &PgPool,
    command_id: CommandId,
) -> Result<StoredTerminalCommandResult, CommandResultRepositoryError> {
    load_terminal_command_result_with_executor(pool, command_id).await
}

/// Loads and validates one persisted terminal result through the caller-owned
/// transaction snapshot.
pub async fn load_terminal_command_result_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    command_id: CommandId,
) -> Result<StoredTerminalCommandResult, CommandResultRepositoryError> {
    load_terminal_command_result_with_executor(&mut **transaction, command_id).await
}

async fn load_terminal_command_result_with_executor<'e, E>(
    executor: E,
    command_id: CommandId,
) -> Result<StoredTerminalCommandResult, CommandResultRepositoryError>
where
    E: sqlx::Executor<'e, Database = Postgres>,
{
    let row = sqlx::query_as::<_, TerminalCommandRow>(
        r#"
        SELECT id,
               aggregate_id,
               status,
               completed_at,
               final_aggregate_version,
               event_count,
               result_payload,
               rejection_code,
               rejection_details
        FROM commands
        WHERE id = $1
        "#,
    )
    .bind(command_id.as_uuid())
    .fetch_optional(executor)
    .await
    .map_err(CommandResultRepositoryError::Database)?
    .ok_or(CommandResultRepositoryError::CommandNotFound { command_id })?;

    StoredTerminalCommandResult::try_from(row)
}

#[cfg(test)]
mod tests {
    use football_domain::{AggregateVersion, CommandId, CommandStatus, MatchId, UtcInstant};
    use serde_json::json;
    use sqlx::types::Json;
    use time::OffsetDateTime;
    use uuid::Uuid;

    use super::{
        CommandResultRepositoryError, PersistedTerminalResult, StoredTerminalCommandResult,
        TerminalCommandEventEvidence, TerminalCommandResult, TerminalCommandRow,
        TerminalCommandWrite, validate_terminal_command_event_evidence,
    };
    use football_application::commands::PersistedCommandResult;

    fn command_id() -> CommandId {
        CommandId::from_uuid(
            Uuid::parse_str("019fe4b0-3bc5-7cf0-b3d2-0728c077a930")
                .expect("command UUID should be valid"),
        )
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(
            Uuid::parse_str("019fe4b0-3bc5-7cf1-b3d2-0728c077a930")
                .expect("match UUID should be valid"),
        )
    }

    fn stored_row(status: &str) -> TerminalCommandRow {
        TerminalCommandRow {
            id: command_id().as_uuid(),
            aggregate_id: Some(match_id().as_uuid()),
            status: status.to_owned(),
            completed_at: Some(
                OffsetDateTime::from_unix_timestamp(2_005)
                    .expect("stored completion time should be valid"),
            ),
            final_aggregate_version: Some(2),
            event_count: Some(2),
            result_payload: Some(Json(json!({"score": {"home": 1, "away": 0}}))),
            rejection_code: None,
            rejection_details: None,
        }
    }

    #[test]
    fn loads_an_accepted_stored_result_without_changing_payload() {
        let result = StoredTerminalCommandResult::try_from(stored_row("accepted")).unwrap();

        assert_eq!(result.command_id(), command_id());
        assert_eq!(result.aggregate_id(), match_id());
        assert_eq!(result.status(), CommandStatus::Accepted);
        assert!(matches!(
            result,
            StoredTerminalCommandResult::Accepted {
                final_aggregate_version,
                event_count: 2,
                result_payload,
                completed_at,
                ..
            } if final_aggregate_version == AggregateVersion::from_value(2)
                && result_payload == json!({"score": {"home": 1, "away": 0}})
                && completed_at == UtcInstant::from_unix_timestamp(2_005).unwrap()
        ));
    }

    #[test]
    fn loads_a_rejected_stored_result_and_rejects_non_terminal_rows() {
        let mut row = stored_row("rejected");
        row.event_count = None;
        row.result_payload = None;
        row.rejection_code = Some("domain.invalid_transition".to_owned());
        row.rejection_details = Some(Json(json!({"state": "pre_match"})));

        let result = StoredTerminalCommandResult::try_from(row).unwrap();
        assert_eq!(result.status(), CommandStatus::Rejected);
        assert!(matches!(
            result,
            StoredTerminalCommandResult::Rejected {
                aggregate_version,
                rejection_code,
                rejection_details,
                ..
            } if aggregate_version == AggregateVersion::from_value(2)
                && rejection_code == "domain.invalid_transition"
                && rejection_details == json!({"state": "pre_match"})
        ));

        assert!(matches!(
            StoredTerminalCommandResult::try_from(stored_row("processing")),
            Err(CommandResultRepositoryError::CommandNotTerminal { status, .. })
                if status == "processing"
        ));
    }

    #[test]
    fn rejects_unknown_or_retryable_stored_rejection_codes() {
        let mut unknown = stored_row("rejected");
        unknown.event_count = None;
        unknown.result_payload = None;
        unknown.rejection_code = Some("unknown.code".to_owned());
        unknown.rejection_details = Some(Json(json!({"reason": "fixture"})));
        assert!(matches!(
            StoredTerminalCommandResult::try_from(unknown),
            Err(CommandResultRepositoryError::InvalidRejectionCode { value, .. })
                if value == "unknown.code"
        ));

        let mut retryable = stored_row("rejected");
        retryable.event_count = None;
        retryable.result_payload = None;
        retryable.rejection_code = Some("dependency.database_unavailable".to_owned());
        retryable.rejection_details = Some(Json(json!({"retry": true})));
        assert!(matches!(
            StoredTerminalCommandResult::try_from(retryable),
            Err(CommandResultRepositoryError::RetryableRejectionCode { value, .. })
                if value == "dependency.database_unavailable"
        ));
    }

    #[test]
    fn rejects_malformed_stored_terminal_shapes_without_guessing() {
        let mut missing_payload = stored_row("accepted");
        missing_payload.result_payload = None;
        assert!(matches!(
            StoredTerminalCommandResult::try_from(missing_payload),
            Err(CommandResultRepositoryError::InvalidStoredTerminalResult { field, .. })
                if field == "result_payload"
        ));

        let mut negative_version = stored_row("accepted");
        negative_version.final_aggregate_version = Some(-1);
        assert!(matches!(
            StoredTerminalCommandResult::try_from(negative_version),
            Err(CommandResultRepositoryError::InvalidStoredTerminalResult { field, .. })
                if field == "final_aggregate_version"
        ));

        let unknown_status = stored_row("not-a-status");
        assert!(matches!(
            StoredTerminalCommandResult::try_from(unknown_status),
            Err(CommandResultRepositoryError::InvalidStoredStatus { value, .. })
                if value == "not-a-status"
        ));
    }

    #[test]
    fn encodes_an_accepted_result_with_bigint_safe_metadata() {
        let result = TerminalCommandResult::Accepted {
            aggregate_id: match_id(),
            final_aggregate_version: AggregateVersion::from_value(2),
            event_count: 2,
            result_payload: json!({"score": {"home": 1, "away": 0}}),
        };

        let persisted = PersistedTerminalResult::try_from_result(command_id(), &result).unwrap();

        assert_eq!(persisted.status, "accepted");
        assert_eq!(persisted.aggregate_id, match_id().as_uuid());
        assert_eq!(persisted.final_aggregate_version, 2);
        assert_eq!(persisted.event_count, Some(2));
        assert_eq!(
            persisted.result_payload.map(|value| value.0),
            Some(json!({"score": {"home": 1, "away": 0}}))
        );
        assert_eq!(persisted.rejection_code, None);
        assert_eq!(persisted.rejection_details, None);
    }

    #[test]
    fn requires_terminal_result_metadata_to_match_canonical_event_evidence() {
        let accepted = TerminalCommandResult::Accepted {
            aggregate_id: match_id(),
            final_aggregate_version: AggregateVersion::from_value(2),
            event_count: 2,
            result_payload: json!({"score": {"home": 1, "away": 0}}),
        };
        let exact_evidence = TerminalCommandEventEvidence {
            command_event_count: 2,
            command_match_event_count: 2,
            command_min_event_index: Some(0),
            command_max_event_index: Some(1),
            command_max_version: Some(2),
            command_match_max_version: Some(2),
            aggregate_max_version: Some(2),
        };
        assert!(
            validate_terminal_command_event_evidence(command_id(), &accepted, exact_evidence,)
                .is_ok()
        );
        assert!(
            validate_terminal_command_event_evidence(
                command_id(),
                &accepted,
                TerminalCommandEventEvidence {
                    aggregate_max_version: Some(3),
                    ..exact_evidence
                },
            )
            .is_ok()
        );

        let mut wrong_count = exact_evidence;
        wrong_count.command_event_count = 1;
        assert!(matches!(
            validate_terminal_command_event_evidence(command_id(), &accepted, wrong_count),
            Err(CommandResultRepositoryError::TerminalResultEventMismatch { command_id: actual })
                if actual == command_id()
        ));

        let mut gapped_indexes = exact_evidence;
        gapped_indexes.command_max_event_index = Some(2);
        assert!(matches!(
            validate_terminal_command_event_evidence(command_id(), &accepted, gapped_indexes),
            Err(CommandResultRepositoryError::TerminalResultEventMismatch { command_id: actual })
                if actual == command_id()
        ));

        let accepted_without_events = TerminalCommandResult::Accepted {
            aggregate_id: match_id(),
            final_aggregate_version: AggregateVersion::from_value(2),
            event_count: 0,
            result_payload: json!({"unchanged": true}),
        };
        assert!(
            validate_terminal_command_event_evidence(
                command_id(),
                &accepted_without_events,
                TerminalCommandEventEvidence {
                    command_event_count: 0,
                    command_match_event_count: 0,
                    command_min_event_index: None,
                    command_max_event_index: None,
                    command_max_version: None,
                    command_match_max_version: None,
                    aggregate_max_version: Some(3),
                },
            )
            .is_ok()
        );

        let rejected = TerminalCommandResult::Rejected {
            aggregate_id: match_id(),
            aggregate_version: AggregateVersion::from_value(2),
            rejection_code: football_platform::ErrorCode::DomainInvalidTransition,
            rejection_details: json!({"state": "pre_match"}),
        };
        assert!(
            validate_terminal_command_event_evidence(
                command_id(),
                &rejected,
                TerminalCommandEventEvidence {
                    command_event_count: 0,
                    command_match_event_count: 0,
                    command_min_event_index: None,
                    command_max_event_index: None,
                    command_max_version: None,
                    command_match_max_version: None,
                    aggregate_max_version: Some(3),
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn bridges_an_application_result_without_losing_terminal_metadata() {
        let completed_at = UtcInstant::from_unix_timestamp(2_005).unwrap();
        let result = PersistedCommandResult::Accepted {
            command_id: command_id(),
            aggregate_id: match_id(),
            completed_at,
            final_aggregate_version: AggregateVersion::from_value(2),
            event_count: 2,
            result_payload: json!({"score": {"home": 1, "away": 0}}),
        };

        let write = TerminalCommandWrite::try_from(result).unwrap();

        assert_eq!(write.command_id(), command_id());
        assert_eq!(write.completed_at(), completed_at);
        assert!(matches!(
            write.result(),
            TerminalCommandResult::Accepted {
                aggregate_id,
                final_aggregate_version,
                event_count: 2,
                result_payload,
            } if *aggregate_id == match_id()
                && *final_aggregate_version == AggregateVersion::from_value(2)
                && result_payload == &json!({"score": {"home": 1, "away": 0}})
        ));
    }

    #[test]
    fn rejects_unknown_application_rejection_codes_before_persistence() {
        let result = PersistedCommandResult::Rejected {
            command_id: command_id(),
            aggregate_id: match_id(),
            completed_at: UtcInstant::from_unix_timestamp(2_005).unwrap(),
            aggregate_version: AggregateVersion::initial(),
            rejection_code: "unknown.code".to_owned(),
            rejection_details: json!({"reason": "fixture"}),
        };

        assert!(matches!(
            TerminalCommandWrite::try_from(result),
            Err(CommandResultRepositoryError::InvalidRejectionCode { command_id: actual, value })
                if actual == command_id() && value == "unknown.code"
        ));
    }

    #[test]
    fn rejects_retryable_application_codes_as_terminal_rejections() {
        let result = PersistedCommandResult::Rejected {
            command_id: command_id(),
            aggregate_id: match_id(),
            completed_at: UtcInstant::from_unix_timestamp(2_005).unwrap(),
            aggregate_version: AggregateVersion::initial(),
            rejection_code: "dependency.database_unavailable".to_owned(),
            rejection_details: json!({"retry": true}),
        };

        assert!(matches!(
            TerminalCommandWrite::try_from(result),
            Err(CommandResultRepositoryError::RetryableRejectionCode {
                command_id: actual,
                value,
            }) if actual == command_id() && value == "dependency.database_unavailable"
        ));
    }

    #[test]
    fn encodes_a_rejection_only_with_object_details() {
        let result = TerminalCommandResult::Rejected {
            aggregate_id: match_id(),
            aggregate_version: AggregateVersion::from_value(4),
            rejection_code: football_platform::ErrorCode::DomainInvalidTransition,
            rejection_details: json!({"state": "pre_match"}),
        };

        let persisted = PersistedTerminalResult::try_from_result(command_id(), &result).unwrap();

        assert_eq!(persisted.status, "rejected");
        assert_eq!(persisted.final_aggregate_version, 4);
        assert_eq!(persisted.event_count, None);
        assert_eq!(persisted.result_payload, None);
        assert_eq!(persisted.rejection_code, Some("domain.invalid_transition"));
        assert_eq!(
            persisted.rejection_details.map(|value| value.0),
            Some(json!({"state": "pre_match"}))
        );

        let invalid = TerminalCommandResult::Rejected {
            aggregate_id: match_id(),
            aggregate_version: AggregateVersion::from_value(4),
            rejection_code: football_platform::ErrorCode::DomainInvalidTransition,
            rejection_details: json!("not-an-object"),
        };
        assert!(matches!(
            PersistedTerminalResult::try_from_result(command_id(), &invalid),
            Err(CommandResultRepositoryError::InvalidRejectionDetails { command_id: actual })
                if actual == command_id()
        ));
    }

    #[test]
    fn rejects_values_that_do_not_fit_postgresql_bigint() {
        let version_overflow = TerminalCommandResult::Accepted {
            aggregate_id: match_id(),
            final_aggregate_version: AggregateVersion::from_value(u64::MAX),
            event_count: 0,
            result_payload: json!(null),
        };
        assert!(matches!(
            PersistedTerminalResult::try_from_result(command_id(), &version_overflow),
            Err(CommandResultRepositoryError::CannotPersistAggregateVersion { value, .. })
                if value == u64::MAX
        ));

        let event_count_overflow = TerminalCommandResult::Accepted {
            aggregate_id: match_id(),
            final_aggregate_version: AggregateVersion::initial(),
            event_count: u64::MAX,
            result_payload: json!(null),
        };
        assert!(matches!(
            PersistedTerminalResult::try_from_result(command_id(), &event_count_overflow),
            Err(CommandResultRepositoryError::CannotPersistEventCount { value, .. })
                if value == u64::MAX
        ));
    }
}
