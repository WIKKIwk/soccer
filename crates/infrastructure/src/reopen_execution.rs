use crate::c1_execution::map_match_event_repository_error;
use crate::commands::{CommandExecutionLeaseError, verify_processing_lease};
use crate::match_events::{
    MatchEventRepositoryError, load_canonical_match_event_stream_in_transaction,
    persist_canonical_match_event_batch,
};
use football_application::commands::CommandExecutionContext;
use football_domain::{
    ActorIdentity, AggregateVersion, CanonicalEventStream, CommandId, EventAppendError,
    MatchEventPayload, ReopenMatchCommand, ReopenMatchCommandError, UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::{Postgres, Transaction};
use std::fmt;

/// Errors raised while loading, appending, and persisting one reasoned reopen
/// event inside the caller-owned command transaction. Terminal command
/// results, audit rows, outbox messages, and the final commit remain owned by
/// the surrounding command executor.
#[derive(Debug)]
pub enum ReopenMatchExecutionError {
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
    Command(ReopenMatchCommandError),
    Persist(MatchEventRepositoryError),
}

impl fmt::Display for ReopenMatchExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextCommandIdMismatch { expected, actual } => write!(
                formatter,
                "reopen command context command {actual} does not match command {expected}"
            ),
            Self::ContextActorMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "reopen command {command_id} actor mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ContextExpectedVersionMissing { command_id } => write!(
                formatter,
                "reopen command {command_id} requires an expected aggregate version"
            ),
            Self::ContextExpectedVersionMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "reopen command {command_id} context expected version {}, command carries {}",
                expected.value(),
                actual.value()
            ),
            Self::Lease(error) => {
                write!(formatter, "reopen command lease validation failed: {error}")
            }
            Self::Load(error) => write!(formatter, "canonical match event load failed: {error}"),
            Self::Command(error) => write!(formatter, "reopen command validation failed: {error}"),
            Self::Persist(error) => write!(
                formatter,
                "canonical match event persistence failed: {error}"
            ),
        }
    }
}

impl std::error::Error for ReopenMatchExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lease(error) => Some(error),
            Self::Load(error) | Self::Persist(error) => Some(error),
            Self::Command(error) => Some(error),
            Self::ContextCommandIdMismatch { .. }
            | Self::ContextActorMismatch { .. }
            | Self::ContextExpectedVersionMissing { .. }
            | Self::ContextExpectedVersionMismatch { .. } => None,
        }
    }
}

impl ReopenMatchExecutionError {
    /// Maps reopen execution failures to stable transport-neutral error codes
    /// without exposing SQL or malformed persisted values.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::ContextCommandIdMismatch { .. }
            | Self::ContextActorMismatch { .. }
            | Self::ContextExpectedVersionMismatch { .. } => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the reopen execution context does not match the command",
            ),
            Self::ContextExpectedVersionMissing { .. } => ApplicationError::new(
                ErrorCode::ValidationRequiredField,
                "reopen commands require an expected aggregate version",
            )
            .with_detail("field", "expected_version"),
            Self::Lease(error) => error.to_application_error(),
            Self::Load(error) | Self::Persist(error) => map_match_event_repository_error(error),
            Self::Command(error) => map_reopen_command_error(error),
        }
    }
}

fn map_reopen_command_error(error: &ReopenMatchCommandError) -> ApplicationError {
    match error {
        ReopenMatchCommandError::MatchNotVerified => ApplicationError::new(
            ErrorCode::DomainInvalidTransition,
            "the match is not currently verified",
        ),
        ReopenMatchCommandError::EventAppend(EventAppendError::ExpectedVersionConflict {
            expected,
            actual,
        }) => ApplicationError::new(
            ErrorCode::ConflictExpectedVersion,
            "the match changed before reopening was applied",
        )
        .with_detail("expected_version", expected.value().to_string())
        .with_detail("actual_version", actual.value().to_string()),
        ReopenMatchCommandError::InvalidEventType(_) | ReopenMatchCommandError::EventAppend(_) => {
            ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "reopening could not produce a valid canonical event",
            )
        }
    }
}

/// Loads the canonical match stream, appends one reasoned `MatchReopened`
/// event, and persists it in the caller-owned transaction. The resulting
/// stream remains uncommitted so command audit, outbox, terminal result, and
/// commit ordering can be composed by the application executor.
pub async fn execute_reopen_match_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: ReopenMatchCommand,
    now_utc: UtcInstant,
) -> Result<CanonicalEventStream<MatchEventPayload>, ReopenMatchExecutionError> {
    validate_reopen_execution_context(context, &command)?;

    verify_processing_lease(
        transaction,
        command.command_id(),
        context.processing_owner(),
        now_utc,
    )
    .await
    .map_err(ReopenMatchExecutionError::Lease)?;

    let stream = load_canonical_match_event_stream_in_transaction(transaction, command.match_id())
        .await
        .map_err(ReopenMatchExecutionError::Load)?;
    let current_version = stream.current_version();
    let existing_event_count = stream.len();
    let appended_stream = command
        .append_to_stream(stream)
        .map_err(ReopenMatchExecutionError::Command)?;

    persist_canonical_match_event_batch(
        transaction,
        current_version,
        &appended_stream.events()[existing_event_count..],
    )
    .await
    .map_err(ReopenMatchExecutionError::Persist)?;

    Ok(appended_stream)
}

fn validate_reopen_execution_context(
    context: &CommandExecutionContext,
    command: &ReopenMatchCommand,
) -> Result<(), ReopenMatchExecutionError> {
    if context.command_id() != command.command_id() {
        return Err(ReopenMatchExecutionError::ContextCommandIdMismatch {
            expected: command.command_id(),
            actual: context.command_id(),
        });
    }

    if context.actor() != command.actor() {
        return Err(ReopenMatchExecutionError::ContextActorMismatch {
            command_id: command.command_id(),
            expected: context.actor(),
            actual: command.actor(),
        });
    }

    let Some(expected_version) = context.expected_version() else {
        return Err(ReopenMatchExecutionError::ContextExpectedVersionMissing {
            command_id: command.command_id(),
        });
    };
    if expected_version != command.expected_version() {
        return Err(ReopenMatchExecutionError::ContextExpectedVersionMismatch {
            command_id: command.command_id(),
            expected: expected_version,
            actual: command.expected_version(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ReopenMatchExecutionError, map_reopen_command_error, validate_reopen_execution_context,
    };
    use football_application::commands::CommandExecutionContext;
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, ClockMilliseconds, CommandId,
        CorrectionReason, EventId, MatchId, MatchPeriod, ReopenMatchCommand,
        ReopenMatchCommandError, UtcInstant,
    };
    use football_platform::{ErrorCategory, ErrorCode};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("reopen execution fixture UUID should be valid")
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019ff620-3bc5-7cf0-b3d2-0728c077a930",
        )))
    }

    fn command() -> ReopenMatchCommand {
        ReopenMatchCommand::try_new(
            CommandId::from_uuid(uuid("019ff620-3bc5-7cf1-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019ff620-3bc5-7cf2-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019ff620-3bc5-7cf3-b3d2-0728c077a930")),
            AggregateVersion::from_value(2),
            MatchPeriod::SecondHalf,
            ClockMilliseconds::from_seconds(91).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            CorrectionReason::try_new("official protocol requires re-verification").unwrap(),
        )
        .unwrap()
    }

    fn context(command: &ReopenMatchCommand) -> CommandExecutionContext {
        CommandExecutionContext::new(
            command.command_id(),
            actor(),
            Some(command.expected_version()),
            uuid("019ff620-3bc5-7cf4-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        )
    }

    #[test]
    fn validates_a_matching_reopen_execution_context() {
        let command = command();

        assert!(validate_reopen_execution_context(&context(&command), &command).is_ok());
    }

    #[test]
    fn rejects_missing_or_mismatched_reopen_execution_context_values() {
        let command = command();
        let missing = CommandExecutionContext::new(
            command.command_id(),
            actor(),
            None,
            uuid("019ff620-3bc5-7cf4-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        );
        assert!(matches!(
            validate_reopen_execution_context(&missing, &command),
            Err(ReopenMatchExecutionError::ContextExpectedVersionMissing { .. })
        ));

        let wrong_actor = CommandExecutionContext::new(
            command.command_id(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019ff620-3bc5-7cf5-b3d2-0728c077a930",
            ))),
            Some(command.expected_version()),
            uuid("019ff620-3bc5-7cf4-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        );
        assert!(matches!(
            validate_reopen_execution_context(&wrong_actor, &command),
            Err(ReopenMatchExecutionError::ContextActorMismatch { .. })
        ));
    }

    #[test]
    fn maps_reopen_state_and_version_failures_to_stable_codes() {
        let not_verified = map_reopen_command_error(&ReopenMatchCommandError::MatchNotVerified);
        assert_eq!(not_verified.code(), ErrorCode::DomainInvalidTransition);
        assert_eq!(not_verified.category(), ErrorCategory::Domain);

        let version_conflict = map_reopen_command_error(&ReopenMatchCommandError::EventAppend(
            football_domain::EventAppendError::ExpectedVersionConflict {
                expected: AggregateVersion::from_value(2),
                actual: AggregateVersion::from_value(3),
            },
        ));
        assert_eq!(version_conflict.code(), ErrorCode::ConflictExpectedVersion);
        assert_eq!(version_conflict.category(), ErrorCategory::Conflict);
    }
}
