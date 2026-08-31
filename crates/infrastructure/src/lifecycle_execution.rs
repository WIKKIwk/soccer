use crate::c1_execution::map_match_event_repository_error;
use crate::commands::{CommandExecutionLeaseError, verify_processing_lease};
use crate::match_events::{
    MatchEventRepositoryError, load_canonical_match_event_stream_in_transaction,
    persist_canonical_match_event_batch,
};
use football_application::commands::CommandExecutionContext;
use football_domain::{
    ActorIdentity, AggregateVersion, CanonicalEventStream, CommandId, EventAppendError,
    LifecycleProjectionError, MatchEventPayload, MatchId, MatchLifecycleCommand,
    MatchLifecycleCommandError, UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::{Postgres, Transaction};
use std::fmt;

#[derive(Debug)]
pub enum LifecycleTransactionOutcome {
    Accepted(CanonicalEventStream<MatchEventPayload>),
    Rejected {
        match_id: MatchId,
        aggregate_version: AggregateVersion,
        error: MatchLifecycleCommandError,
    },
}

#[derive(Debug)]
pub enum LifecycleExecutionError {
    ContextCommandIdMismatch {
        expected: CommandId,
        actual: CommandId,
    },
    ContextActorMismatch {
        command_id: CommandId,
        expected: ActorIdentity,
        actual: ActorIdentity,
    },
    ContextExpectedVersionMissing {
        command_id: CommandId,
    },
    ContextExpectedVersionMismatch {
        command_id: CommandId,
        expected: AggregateVersion,
        actual: AggregateVersion,
    },
    Lease(CommandExecutionLeaseError),
    Load(MatchEventRepositoryError),
    Command(MatchLifecycleCommandError),
    Persist(MatchEventRepositoryError),
}

impl fmt::Display for LifecycleExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextCommandIdMismatch { expected, actual } => write!(
                formatter,
                "lifecycle context command {actual} does not match {expected}"
            ),
            Self::ContextActorMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "lifecycle command {command_id} actor mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ContextExpectedVersionMissing { command_id } => {
                write!(
                    formatter,
                    "lifecycle command {command_id} requires expected version"
                )
            }
            Self::ContextExpectedVersionMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "lifecycle command {command_id} expected version {}, context carries {}",
                actual.value(),
                expected.value()
            ),
            Self::Lease(error) => write!(formatter, "lifecycle lease validation failed: {error}"),
            Self::Load(error) => write!(formatter, "lifecycle stream load failed: {error}"),
            Self::Command(error) => write!(formatter, "lifecycle command failed: {error}"),
            Self::Persist(error) => {
                write!(formatter, "lifecycle event persistence failed: {error}")
            }
        }
    }
}

impl std::error::Error for LifecycleExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lease(error) => Some(error),
            Self::Load(error) | Self::Persist(error) => Some(error),
            Self::Command(error) => Some(error),
            _ => None,
        }
    }
}

impl LifecycleExecutionError {
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::ContextCommandIdMismatch { .. }
            | Self::ContextActorMismatch { .. }
            | Self::ContextExpectedVersionMismatch { .. } => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the lifecycle execution context does not match the command",
            ),
            Self::ContextExpectedVersionMissing { .. } => ApplicationError::new(
                ErrorCode::ValidationRequiredField,
                "lifecycle commands require an expected aggregate version",
            )
            .with_detail("field", "expected_version"),
            Self::Lease(error) => error.to_application_error(),
            Self::Load(error) | Self::Persist(error) => map_match_event_repository_error(error),
            Self::Command(error) => lifecycle_command_application_error(error),
        }
    }
}

pub async fn execute_lifecycle_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: MatchLifecycleCommand,
    now_utc: UtcInstant,
) -> Result<LifecycleTransactionOutcome, LifecycleExecutionError> {
    validate_context(context, &command)?;
    verify_processing_lease(
        transaction,
        command.command_id(),
        context.processing_owner(),
        now_utc,
    )
    .await
    .map_err(LifecycleExecutionError::Lease)?;

    let stream = load_canonical_match_event_stream_in_transaction(transaction, command.match_id())
        .await
        .map_err(LifecycleExecutionError::Load)?;
    let current_version = stream.current_version();
    let existing_event_count = stream.len();
    let match_id = command.match_id();
    let appended = match command.append_to_stream(stream) {
        Ok(stream) => stream,
        Err(error) if is_terminal_rejection(&error) => {
            return Ok(LifecycleTransactionOutcome::Rejected {
                match_id,
                aggregate_version: current_version,
                error,
            });
        }
        Err(error) => return Err(LifecycleExecutionError::Command(error)),
    };
    persist_canonical_match_event_batch(
        transaction,
        current_version,
        &appended.events()[existing_event_count..],
    )
    .await
    .map_err(LifecycleExecutionError::Persist)?;
    Ok(LifecycleTransactionOutcome::Accepted(appended))
}

fn validate_context(
    context: &CommandExecutionContext,
    command: &MatchLifecycleCommand,
) -> Result<(), LifecycleExecutionError> {
    if context.command_id() != command.command_id() {
        return Err(LifecycleExecutionError::ContextCommandIdMismatch {
            expected: command.command_id(),
            actual: context.command_id(),
        });
    }
    if context.actor() != command.actor() {
        return Err(LifecycleExecutionError::ContextActorMismatch {
            command_id: command.command_id(),
            expected: context.actor(),
            actual: command.actor(),
        });
    }
    let expected = context.expected_version().ok_or(
        LifecycleExecutionError::ContextExpectedVersionMissing {
            command_id: command.command_id(),
        },
    )?;
    if expected != command.expected_version() {
        return Err(LifecycleExecutionError::ContextExpectedVersionMismatch {
            command_id: command.command_id(),
            expected,
            actual: command.expected_version(),
        });
    }
    Ok(())
}

fn is_terminal_rejection(error: &MatchLifecycleCommandError) -> bool {
    matches!(
        error,
        MatchLifecycleCommandError::EventAppend(EventAppendError::ExpectedVersionConflict { .. })
            | MatchLifecycleCommandError::Lifecycle(_)
    )
}

pub(crate) fn lifecycle_command_application_error(
    error: &MatchLifecycleCommandError,
) -> ApplicationError {
    match error {
        MatchLifecycleCommandError::EventAppend(EventAppendError::ExpectedVersionConflict {
            expected,
            actual,
        }) => ApplicationError::new(
            ErrorCode::ConflictExpectedVersion,
            "the match changed before the lifecycle command was applied",
        )
        .with_detail("expected_version", expected.value().to_string())
        .with_detail("actual_version", actual.value().to_string()),
        MatchLifecycleCommandError::Lifecycle(error) => lifecycle_projection_error(error),
        MatchLifecycleCommandError::InvalidEventType(_)
        | MatchLifecycleCommandError::EffectiveStream(_)
        | MatchLifecycleCommandError::EventAppend(_) => ApplicationError::new(
            ErrorCode::InternalUnexpected,
            "the lifecycle command could not produce a canonical event",
        ),
    }
}

fn lifecycle_projection_error(error: &LifecycleProjectionError) -> ApplicationError {
    let mut application = ApplicationError::new(
        ErrorCode::DomainInvalidTransition,
        "the lifecycle transition is not valid for the current match state",
    );
    if let LifecycleProjectionError::InvalidTransition {
        from, event_type, ..
    } = error
    {
        application = application
            .with_detail("state", from.as_persisted_str())
            .with_detail("event_type", event_type.as_str());
    }
    application
}

#[cfg(test)]
mod tests {
    use super::lifecycle_command_application_error;
    use football_domain::{
        LifecycleEventType, LifecycleProjectionError, MatchLifecycleCommandError, MatchState,
    };
    use football_platform::ErrorCode;

    #[test]
    fn invalid_transition_maps_to_stable_domain_error() {
        let error = lifecycle_command_application_error(&MatchLifecycleCommandError::Lifecycle(
            LifecycleProjectionError::InvalidTransition {
                event_id: None,
                from: MatchState::Draft,
                event_type: LifecycleEventType::FirstHalfStarted,
            },
        ));
        assert_eq!(error.code(), ErrorCode::DomainInvalidTransition);
        assert_eq!(
            error.details().get("state").map(String::as_str),
            Some("draft")
        );
    }
}
