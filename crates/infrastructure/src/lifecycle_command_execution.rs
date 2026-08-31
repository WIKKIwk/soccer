use crate::audit::{
    CommandAuditEntry, CommandAuditEntryError, CommandAuditRepositoryError, persist_command_audit,
};
use crate::command_results::{CommandResultRepositoryError, persist_application_command_result};
use crate::lifecycle_execution::{
    LifecycleExecutionError, LifecycleTransactionOutcome, execute_lifecycle_in_transaction,
    lifecycle_command_application_error,
};
use crate::match_event_payloads::{MatchEventPayloadCodecError, encode_match_event_envelope};
use crate::outbox::{
    OutboxMessage, OutboxMessageError, OutboxRepositoryError, persist_outbox_message,
};
use crate::synchronous_match_state::{SynchronousMatchStateError, persist_synchronous_match_state};
use football_application::commands::{
    CommandExecutionContext, CommandExecutionPort, into_persisted_command_result,
};
use football_domain::{
    CommandOutcome, CommandResultEnvelope, EventId, LifecycleEventType, MatchLifecycleCommand,
    MatchLifecycleCommandError, MatchState, UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use serde_json::{Map, Value, json};
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleCommandResult {
    pub event_id: EventId,
    pub event_type: LifecycleEventType,
    pub match_state: MatchState,
}

#[derive(Debug)]
pub enum LifecycleCommandExecutionError {
    Database(SqlxError),
    EventTransaction(LifecycleExecutionError),
    SynchronousState(SynchronousMatchStateError),
    AuditEntry(CommandAuditEntryError),
    AuditRepository(CommandAuditRepositoryError),
    OutboxEncoding(MatchEventPayloadCodecError),
    OutboxEntry(OutboxMessageError),
    OutboxRepository(OutboxRepositoryError),
    TerminalResult(CommandResultRepositoryError),
}

impl fmt::Display for LifecycleCommandExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "lifecycle database failure: {error}"),
            Self::EventTransaction(error) => write!(formatter, "lifecycle event failure: {error}"),
            Self::SynchronousState(error) => write!(formatter, "lifecycle state failure: {error}"),
            Self::AuditEntry(error) => write!(formatter, "lifecycle audit is invalid: {error}"),
            Self::AuditRepository(error) => write!(formatter, "lifecycle audit failed: {error}"),
            Self::OutboxEncoding(error) => {
                write!(formatter, "lifecycle outbox encoding failed: {error}")
            }
            Self::OutboxEntry(error) => write!(formatter, "lifecycle outbox is invalid: {error}"),
            Self::OutboxRepository(error) => write!(formatter, "lifecycle outbox failed: {error}"),
            Self::TerminalResult(error) => {
                write!(formatter, "lifecycle terminal result failed: {error}")
            }
        }
    }
}

impl std::error::Error for LifecycleCommandExecutionError {
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

impl LifecycleCommandExecutionError {
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::Database(_) => dependency_error("the lifecycle command database is unavailable"),
            Self::EventTransaction(error) => error.to_application_error(),
            Self::SynchronousState(SynchronousMatchStateError::Database(_)) => {
                dependency_error("the synchronous match-state database is unavailable")
            }
            Self::SynchronousState(SynchronousMatchStateError::AssignmentNotFound { match_id }) => {
                ApplicationError::new(
                    ErrorCode::ResourceNotFound,
                    "the match assignment was not found",
                )
                .with_detail("match_id", match_id.to_string())
            }
            Self::SynchronousState(_)
            | Self::AuditEntry(_)
            | Self::OutboxEncoding(_)
            | Self::OutboxEntry(_) => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the lifecycle command transaction could not be constructed",
            ),
            Self::AuditRepository(CommandAuditRepositoryError::Database(_))
            | Self::OutboxRepository(OutboxRepositoryError::Database(_))
            | Self::TerminalResult(CommandResultRepositoryError::Database(_)) => {
                dependency_error("a lifecycle command dependency is unavailable")
            }
            Self::AuditRepository(_) | Self::OutboxRepository(_) | Self::TerminalResult(_) => {
                ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the lifecycle command transaction was rejected",
                )
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PostgresLifecycleExecutor<'a> {
    pool: &'a PgPool,
    now_utc: UtcInstant,
}

impl<'a> PostgresLifecycleExecutor<'a> {
    pub const fn new(pool: &'a PgPool, now_utc: UtcInstant) -> Self {
        Self { pool, now_utc }
    }

    async fn execute_inner(
        &self,
        context: &CommandExecutionContext,
        command: &MatchLifecycleCommand,
    ) -> Result<
        CommandResultEnvelope<LifecycleCommandResult, MatchLifecycleCommandError>,
        LifecycleCommandExecutionError,
    > {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(LifecycleCommandExecutionError::Database)?;
        let outcome = match execute_lifecycle_in_transaction(
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
                return Err(LifecycleCommandExecutionError::EventTransaction(error));
            }
        };

        let (envelope, accepted_event) = match outcome {
            LifecycleTransactionOutcome::Accepted(stream) => {
                let event = match stream.events().last().cloned() {
                    Some(event) => event,
                    None => {
                        let error = LifecycleCommandExecutionError::OutboxEntry(
                            OutboxMessageError::NoSourceEvent {
                                match_id: command.match_id(),
                            },
                        );
                        let _ = transaction.rollback().await;
                        return Err(error);
                    }
                };
                let state =
                    match persist_synchronous_match_state(&mut transaction, &stream, self.now_utc)
                        .await
                    {
                        Ok(state) => state,
                        Err(error) => {
                            let _ = transaction.rollback().await;
                            return Err(LifecycleCommandExecutionError::SynchronousState(error));
                        }
                    };
                let result = LifecycleCommandResult {
                    event_id: command.event_id(),
                    event_type: command.event_type(),
                    match_state: state.match_state,
                };
                (
                    CommandResultEnvelope::accepted(
                        command.command_id(),
                        command.match_id(),
                        stream.current_version(),
                        1,
                        result,
                    ),
                    Some(event),
                )
            }
            LifecycleTransactionOutcome::Rejected {
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

        let audit = build_audit(context, command, &envelope, self.now_utc)?;
        if let Err(error) = persist_command_audit(&mut transaction, &audit).await {
            let _ = transaction.rollback().await;
            return Err(LifecycleCommandExecutionError::AuditRepository(error));
        }
        if let Some(event) = accepted_event {
            let encoded = encode_match_event_envelope(&event)
                .map_err(LifecycleCommandExecutionError::OutboxEncoding)?;
            let message = OutboxMessage::from_encoded_match_event(&encoded, self.now_utc)
                .map_err(LifecycleCommandExecutionError::OutboxEntry)?;
            if let Err(error) = persist_outbox_message(&mut transaction, &message).await {
                let _ = transaction.rollback().await;
                return Err(LifecycleCommandExecutionError::OutboxRepository(error));
            }
        }

        let result = into_persisted_command_result(
            envelope.clone(),
            self.now_utc,
            |result| {
                json!({
                    "event_id": result.event_id.to_string(),
                    "event_type": result.event_type.as_str(),
                    "match_state": result.match_state.as_persisted_str(),
                })
            },
            lifecycle_rejection_payload,
        );
        if let Err(error) =
            persist_application_command_result(&mut transaction, context.processing_owner(), result)
                .await
        {
            let _ = transaction.rollback().await;
            return Err(LifecycleCommandExecutionError::TerminalResult(error));
        }
        transaction
            .commit()
            .await
            .map_err(LifecycleCommandExecutionError::Database)?;
        Ok(envelope)
    }
}

impl<'a> CommandExecutionPort<MatchLifecycleCommand> for PostgresLifecycleExecutor<'a> {
    type Result = LifecycleCommandResult;
    type DomainError = MatchLifecycleCommandError;
    type Error = LifecycleCommandExecutionError;

    async fn execute_and_commit<'b>(
        &'b self,
        context: &'b CommandExecutionContext,
        command: &'b MatchLifecycleCommand,
    ) -> Result<CommandResultEnvelope<Self::Result, Self::DomainError>, Self::Error> {
        self.execute_inner(context, command).await
    }
}

fn build_audit(
    context: &CommandExecutionContext,
    command: &MatchLifecycleCommand,
    envelope: &CommandResultEnvelope<LifecycleCommandResult, MatchLifecycleCommandError>,
    now_utc: UtcInstant,
) -> Result<CommandAuditEntry, LifecycleCommandExecutionError> {
    match envelope.outcome() {
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
            now_utc,
        ),
        CommandOutcome::Rejected {
            aggregate_version,
            error,
        } => {
            let (code, details) = lifecycle_rejection_payload(error.clone());
            CommandAuditEntry::rejected(
                command.command_id(),
                context.actor(),
                command.match_id(),
                command.command_name(),
                *aggregate_version,
                code,
                details,
                now_utc,
            )
        }
    }
    .map_err(LifecycleCommandExecutionError::AuditEntry)
}

fn lifecycle_rejection_payload(error: MatchLifecycleCommandError) -> (String, Value) {
    let application = lifecycle_command_application_error(&error);
    let details = application
        .details()
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect::<Map<_, _>>();
    (
        application.code().as_str().to_owned(),
        Value::Object(details),
    )
}

fn dependency_error(message: &'static str) -> ApplicationError {
    ApplicationError::new(ErrorCode::DependencyDatabaseUnavailable, message)
}

#[cfg(test)]
mod tests {
    use super::lifecycle_rejection_payload;
    use football_domain::{AggregateVersion, EventAppendError, MatchLifecycleCommandError};

    #[test]
    fn rejection_payload_keeps_expected_version_details() {
        let (code, details) = lifecycle_rejection_payload(MatchLifecycleCommandError::EventAppend(
            EventAppendError::ExpectedVersionConflict {
                expected: AggregateVersion::from_value(2),
                actual: AggregateVersion::from_value(3),
            },
        ));
        assert_eq!(code, "conflict.expected_version");
        assert_eq!(details["expected_version"], "2");
        assert_eq!(details["actual_version"], "3");
    }
}
