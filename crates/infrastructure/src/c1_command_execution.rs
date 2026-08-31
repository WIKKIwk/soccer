use crate::audit::{
    CommandAuditEntry, CommandAuditEntryError, CommandAuditRepositoryError, persist_command_audit,
};
use crate::c1_execution::{
    C1LiveActionExecutionError, C1LiveActionTransactionOutcome,
    execute_c1_live_action_in_canonical_stream_transaction_outcome,
};
use crate::command_results::{CommandResultRepositoryError, persist_application_command_result};
use crate::match_event_payloads::{MatchEventPayloadCodecError, encode_match_event_envelope};
use crate::outbox::{
    OutboxMessage, OutboxMessageError, OutboxRepositoryError, persist_outbox_message,
};
use crate::synchronous_match_state::{SynchronousMatchStateError, persist_synchronous_match_state};
use football_application::commands::{
    CommandExecutionContext, CommandExecutionPort, into_persisted_command_result,
};
use football_domain::{
    C1EventType, C1LiveActionCommand, C1LiveActionCommandError, CommandOutcome,
    CommandResultEnvelope, EventId, UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use serde_json::{Map, Value, json};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;

/// Typed accepted payload for one C1 live action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct C1LiveActionResult {
    event_id: EventId,
    event_type: C1EventType,
}

impl C1LiveActionResult {
    pub const fn new(event_id: EventId, event_type: C1EventType) -> Self {
        Self {
            event_id,
            event_type,
        }
    }

    pub const fn event_id(self) -> EventId {
        self.event_id
    }

    pub const fn event_type(self) -> C1EventType {
        self.event_type
    }
}

/// Failure while composing the current C1 application execution boundary.
///
/// The transaction owns canonical event persistence, the command audit, one
/// critical synchronous state, one versioned match-event outbox message, and
/// the terminal command result in this increment.
#[derive(Debug)]
pub enum C1LiveActionCommandExecutionError {
    Database(SqlxError),
    EventTransaction(C1LiveActionExecutionError),
    SynchronousState(SynchronousMatchStateError),
    AuditEntry(CommandAuditEntryError),
    AuditRepository(CommandAuditRepositoryError),
    OutboxEncoding(MatchEventPayloadCodecError),
    OutboxEntry(OutboxMessageError),
    OutboxRepository(OutboxRepositoryError),
    TerminalResult(CommandResultRepositoryError),
}

impl fmt::Display for C1LiveActionCommandExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(
                    formatter,
                    "C1 command transaction database operation failed: {error}"
                )
            }
            Self::EventTransaction(error) => {
                write!(formatter, "C1 command event transaction failed: {error}")
            }
            Self::SynchronousState(error) => {
                write!(formatter, "C1 command synchronous state failed: {error}")
            }
            Self::AuditEntry(error) => {
                write!(formatter, "C1 command audit entry is invalid: {error}")
            }
            Self::AuditRepository(error) => {
                write!(formatter, "C1 command audit persistence failed: {error}")
            }
            Self::OutboxEncoding(error) => {
                write!(
                    formatter,
                    "C1 command outbox envelope encoding failed: {error}"
                )
            }
            Self::OutboxEntry(error) => {
                write!(formatter, "C1 command outbox message is invalid: {error}")
            }
            Self::OutboxRepository(error) => {
                write!(formatter, "C1 command outbox persistence failed: {error}")
            }
            Self::TerminalResult(error) => {
                write!(formatter, "C1 command terminal result failed: {error}")
            }
        }
    }
}

impl std::error::Error for C1LiveActionCommandExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::EventTransaction(error) => Some(error),
            Self::SynchronousState(error) => Some(error),
            Self::AuditEntry(error) => Some(error),
            Self::AuditRepository(error) => Some(error),
            Self::OutboxEncoding(error) => Some(error),
            Self::OutboxEntry(error) => Some(error),
            Self::OutboxRepository(error) => Some(error),
            Self::TerminalResult(error) => Some(error),
        }
    }
}

impl C1LiveActionCommandExecutionError {
    /// Maps execution failures to the stable application error taxonomy.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::Database(_) => ApplicationError::new(
                ErrorCode::DependencyDatabaseUnavailable,
                "the C1 command execution dependency is unavailable",
            ),
            Self::EventTransaction(error) => error.to_application_error(),
            Self::SynchronousState(SynchronousMatchStateError::Database(_)) => {
                ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the synchronous match-state dependency is unavailable",
                )
            }
            Self::SynchronousState(SynchronousMatchStateError::AssignmentNotFound { match_id }) => {
                ApplicationError::new(
                    ErrorCode::ResourceNotFound,
                    "the match assignment was not found",
                )
                .with_detail("match_id", match_id.to_string())
            }
            Self::SynchronousState(_) => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the synchronous match state could not be published",
            ),
            Self::AuditEntry(_) => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the command audit entry could not be constructed",
            ),
            Self::AuditRepository(error) => match error {
                CommandAuditRepositoryError::Database(_) => ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the command audit dependency is unavailable",
                ),
                _ => ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command audit store rejected the audit entry",
                ),
            },
            Self::OutboxEncoding(_) | Self::OutboxEntry(_) => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the command outbox message could not be constructed",
            ),
            Self::OutboxRepository(error) => match error {
                OutboxRepositoryError::Database(_) => ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the command outbox dependency is unavailable",
                ),
                _ => ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command outbox store rejected the message",
                ),
            },
            Self::TerminalResult(error) => match error {
                CommandResultRepositoryError::Database(_) => ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the command result dependency is unavailable",
                ),
                _ => ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command result store rejected the terminal result",
                ),
            },
        }
    }
}

/// PostgreSQL command execution adapter for one C1 live-action command.
///
/// Admission remains a separate short transaction owned by
/// `PostgresCommandAdmission`. Once admission has claimed the command, this
/// adapter performs lease validation, canonical event append, synchronous
/// state publication, command-audit/outbox/terminal-result persistence, and
/// commit in one caller-independent PostgreSQL transaction.
#[derive(Debug, Clone, Copy)]
pub struct PostgresC1LiveActionExecutor<'a> {
    pool: &'a PgPool,
    now_utc: UtcInstant,
}

impl<'a> PostgresC1LiveActionExecutor<'a> {
    pub const fn new(pool: &'a PgPool, now_utc: UtcInstant) -> Self {
        Self { pool, now_utc }
    }

    async fn execute_and_commit_inner(
        &self,
        context: &CommandExecutionContext,
        command: &C1LiveActionCommand,
    ) -> Result<
        CommandResultEnvelope<C1LiveActionResult, C1LiveActionCommandError>,
        C1LiveActionCommandExecutionError,
    > {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(C1LiveActionCommandExecutionError::Database)?;

        let accepted_result = C1LiveActionResult::new(command.event_id(), command.event_type());
        let outcome = match execute_c1_live_action_in_canonical_stream_transaction_outcome(
            &mut transaction,
            context,
            command.clone(),
            self.now_utc,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(C1LiveActionCommandExecutionError::EventTransaction(error));
            }
        };

        let (envelope, accepted_event) = match outcome {
            C1LiveActionTransactionOutcome::Accepted(stream) => {
                let accepted_event = stream.events().last().cloned().ok_or_else(|| {
                    C1LiveActionCommandExecutionError::OutboxEntry(
                        OutboxMessageError::NoSourceEvent {
                            match_id: command.match_id(),
                        },
                    )
                });
                let accepted_event = match accepted_event {
                    Ok(event) => event,
                    Err(error) => {
                        let _ = transaction.rollback().await;
                        return Err(error);
                    }
                };
                if let Err(error) =
                    persist_synchronous_match_state(&mut transaction, &stream, self.now_utc).await
                {
                    let _ = transaction.rollback().await;
                    return Err(C1LiveActionCommandExecutionError::SynchronousState(error));
                }
                (
                    CommandResultEnvelope::accepted(
                        command.command_id(),
                        command.match_id(),
                        stream.current_version(),
                        1,
                        accepted_result,
                    ),
                    Some(accepted_event),
                )
            }
            C1LiveActionTransactionOutcome::Rejected {
                match_id,
                aggregate_version,
                error,
            } => (
                CommandResultEnvelope::rejected(
                    command.command_id(),
                    match_id,
                    aggregate_version,
                    error,
                ),
                None,
            ),
        };

        let audit_entry = match envelope.outcome() {
            CommandOutcome::Accepted {
                final_aggregate_version,
                event_count,
                ..
            } => CommandAuditEntry::accepted(
                command.command_id(),
                context.actor(),
                command.match_id(),
                command.command_name(),
                *final_aggregate_version,
                *event_count,
                command.event_id(),
                self.now_utc,
            )
            .map_err(C1LiveActionCommandExecutionError::AuditEntry),
            CommandOutcome::Rejected {
                aggregate_version,
                error,
            } => {
                let (rejection_code, rejection_details) = c1_rejection_payload(*error);
                CommandAuditEntry::rejected(
                    command.command_id(),
                    context.actor(),
                    command.match_id(),
                    command.command_name(),
                    *aggregate_version,
                    rejection_code,
                    rejection_details,
                    self.now_utc,
                )
                .map_err(C1LiveActionCommandExecutionError::AuditEntry)
            }
        };
        let audit_entry = match audit_entry {
            Ok(entry) => entry,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        };

        if let Err(error) = persist_command_audit(&mut transaction, &audit_entry).await {
            let _ = transaction.rollback().await;
            return Err(C1LiveActionCommandExecutionError::AuditRepository(error));
        }

        if let Some(accepted_event) = accepted_event {
            let encoded_event = match encode_match_event_envelope(&accepted_event) {
                Ok(event) => event,
                Err(error) => {
                    let _ = transaction.rollback().await;
                    return Err(C1LiveActionCommandExecutionError::OutboxEncoding(error));
                }
            };
            let outbox_message =
                match OutboxMessage::from_encoded_match_event(&encoded_event, self.now_utc) {
                    Ok(message) => message,
                    Err(error) => {
                        let _ = transaction.rollback().await;
                        return Err(C1LiveActionCommandExecutionError::OutboxEntry(error));
                    }
                };
            if let Err(error) = persist_outbox_message(&mut transaction, &outbox_message).await {
                let _ = transaction.rollback().await;
                return Err(C1LiveActionCommandExecutionError::OutboxRepository(error));
            }
        }

        let persisted_result = into_persisted_command_result(
            envelope.clone(),
            self.now_utc,
            |result| {
                json!({
                    "event_id": result.event_id().to_string(),
                    "event_type": result.event_type().as_str(),
                })
            },
            c1_rejection_payload,
        );

        if let Err(error) = persist_application_command_result(
            &mut transaction,
            context.processing_owner(),
            persisted_result,
        )
        .await
        {
            let _ = transaction.rollback().await;
            return Err(C1LiveActionCommandExecutionError::TerminalResult(error));
        }

        transaction
            .commit()
            .await
            .map_err(C1LiveActionCommandExecutionError::Database)?;

        Ok(envelope)
    }
}

impl<'a> CommandExecutionPort<C1LiveActionCommand> for PostgresC1LiveActionExecutor<'a> {
    type Result = C1LiveActionResult;
    type DomainError = C1LiveActionCommandError;
    type Error = C1LiveActionCommandExecutionError;

    async fn execute_and_commit<'b>(
        &'b self,
        context: &'b CommandExecutionContext,
        command: &'b C1LiveActionCommand,
    ) -> Result<CommandResultEnvelope<Self::Result, Self::DomainError>, Self::Error> {
        self.execute_and_commit_inner(context, command).await
    }
}

fn application_error_details(error: &ApplicationError) -> Value {
    let details = error
        .details()
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect::<Map<String, Value>>();
    Value::Object(details)
}

fn c1_rejection_payload(error: C1LiveActionCommandError) -> (String, Value) {
    let application_error = C1LiveActionExecutionError::Command(error).to_application_error();
    (
        application_error.code().as_str().to_owned(),
        application_error_details(&application_error),
    )
}

#[cfg(test)]
mod tests {
    use super::{C1LiveActionResult, application_error_details};
    use football_domain::{C1EventType, EventId};
    use football_platform::{ApplicationError, ErrorCode};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("C1 command execution fixture UUID should be valid")
    }

    #[test]
    fn encodes_only_safe_stable_details_for_terminal_rejections() {
        let error = ApplicationError::new(ErrorCode::ConflictExpectedVersion, "stale version")
            .with_detail("expected_version", "0")
            .with_detail("actual_version", "1");
        let details = application_error_details(&error);

        assert_eq!(
            details,
            serde_json::json!({
                "expected_version": "0",
                "actual_version": "1"
            })
        );
    }

    #[test]
    fn keeps_typed_c1_result_metadata() {
        let result = C1LiveActionResult::new(
            EventId::from_uuid(uuid("019fe930-3bc5-7cf0-b3d2-0728c077a930")),
            C1EventType::GoalRecorded,
        );

        assert_eq!(result.event_type(), C1EventType::GoalRecorded);
        assert_eq!(
            result.event_id(),
            EventId::from_uuid(uuid("019fe930-3bc5-7cf0-b3d2-0728c077a930"))
        );
    }
}
