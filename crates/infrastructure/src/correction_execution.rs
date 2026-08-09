use crate::c1_execution::map_match_event_repository_error;
use crate::commands::{CommandExecutionLeaseError, verify_processing_lease};
use crate::match_events::{
    MatchEventRepositoryError, load_canonical_match_event_stream_in_transaction,
    persist_canonical_match_event_batch,
};
use football_application::commands::CommandExecutionContext;
use football_domain::{
    ActorIdentity, AggregateVersion, CanonicalEventStream, CommandId, EventAppendError,
    MatchEventPayload, UtcInstant, VoidEventCommand, VoidEventCommandError,
};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::{Postgres, Transaction};
use std::fmt;

/// Errors raised while loading, appending, and persisting one correction event
/// inside the caller-owned command transaction. Terminal command results,
/// audit/outbox writes, and the final commit remain outside this adapter.
#[derive(Debug)]
pub enum VoidEventExecutionError {
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
    Command(VoidEventCommandError),
    Persist(MatchEventRepositoryError),
}

impl fmt::Display for VoidEventExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextCommandIdMismatch { expected, actual } => write!(
                formatter,
                "void execution context command {actual} does not match command {expected}"
            ),
            Self::ContextActorMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "void command {command_id} actor mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ContextExpectedVersionMissing { command_id } => write!(
                formatter,
                "void command {command_id} requires an expected aggregate version"
            ),
            Self::ContextExpectedVersionMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "void command {command_id} context expected version {}, command carries {}",
                expected.value(),
                actual.value()
            ),
            Self::Lease(error) => {
                write!(formatter, "void command lease validation failed: {error}")
            }
            Self::Load(error) => write!(formatter, "canonical match event load failed: {error}"),
            Self::Command(error) => write!(formatter, "void command validation failed: {error}"),
            Self::Persist(error) => {
                write!(
                    formatter,
                    "canonical match event persistence failed: {error}"
                )
            }
        }
    }
}

impl std::error::Error for VoidEventExecutionError {
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

impl VoidEventExecutionError {
    /// Maps correction execution failures to stable transport-neutral error
    /// codes without exposing SQL or malformed persisted values.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::ContextCommandIdMismatch { .. }
            | Self::ContextActorMismatch { .. }
            | Self::ContextExpectedVersionMismatch { .. } => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the void execution context does not match the command",
            ),
            Self::ContextExpectedVersionMissing { .. } => ApplicationError::new(
                ErrorCode::ValidationRequiredField,
                "void commands require an expected aggregate version",
            )
            .with_detail("field", "expected_version"),
            Self::Lease(error) => error.to_application_error(),
            Self::Load(error) | Self::Persist(error) => map_match_event_repository_error(error),
            Self::Command(error) => map_void_command_error(error),
        }
    }
}

fn map_void_command_error(error: &VoidEventCommandError) -> ApplicationError {
    match error {
        VoidEventCommandError::TargetEventNotFound { event_id } => ApplicationError::new(
            ErrorCode::ResourceNotFound,
            "the correction target event was not found",
        )
        .with_detail("event_id", event_id.to_string()),
        VoidEventCommandError::EventAppend(EventAppendError::ExpectedVersionConflict {
            expected,
            actual,
        }) => ApplicationError::new(
            ErrorCode::ConflictExpectedVersion,
            "the match changed before the correction was applied",
        )
        .with_detail("expected_version", expected.value().to_string())
        .with_detail("actual_version", actual.value().to_string()),
        VoidEventCommandError::SelfReferentialVoid { .. }
        | VoidEventCommandError::InvalidEventType(_)
        | VoidEventCommandError::EventAppend(_) => ApplicationError::new(
            ErrorCode::InternalUnexpected,
            "the correction could not produce a valid canonical event",
        ),
    }
}

/// Loads the typed canonical union, validates the correction command against
/// the aggregate version and target, persists the immutable correction event,
/// and returns the uncommitted resulting source stream.
pub async fn execute_void_event_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: VoidEventCommand,
    now_utc: UtcInstant,
) -> Result<CanonicalEventStream<MatchEventPayload>, VoidEventExecutionError> {
    validate_void_execution_context(context, &command)?;

    verify_processing_lease(
        transaction,
        command.command_id(),
        context.processing_owner(),
        now_utc,
    )
    .await
    .map_err(VoidEventExecutionError::Lease)?;

    let stream = load_canonical_match_event_stream_in_transaction(transaction, command.match_id())
        .await
        .map_err(VoidEventExecutionError::Load)?;
    let current_version = stream.current_version();
    let existing_event_count = stream.len();
    let appended_stream = command
        .append_to_stream(stream)
        .map_err(VoidEventExecutionError::Command)?;

    persist_canonical_match_event_batch(
        transaction,
        current_version,
        &appended_stream.events()[existing_event_count..],
    )
    .await
    .map_err(VoidEventExecutionError::Persist)?;

    Ok(appended_stream)
}

fn validate_void_execution_context(
    context: &CommandExecutionContext,
    command: &VoidEventCommand,
) -> Result<(), VoidEventExecutionError> {
    if context.command_id() != command.command_id() {
        return Err(VoidEventExecutionError::ContextCommandIdMismatch {
            expected: command.command_id(),
            actual: context.command_id(),
        });
    }

    if context.actor() != command.actor() {
        return Err(VoidEventExecutionError::ContextActorMismatch {
            command_id: command.command_id(),
            expected: context.actor(),
            actual: command.actor(),
        });
    }

    let Some(expected_version) = context.expected_version() else {
        return Err(VoidEventExecutionError::ContextExpectedVersionMissing {
            command_id: command.command_id(),
        });
    };
    if expected_version != command.expected_version() {
        return Err(VoidEventExecutionError::ContextExpectedVersionMismatch {
            command_id: command.command_id(),
            expected: expected_version,
            actual: command.expected_version(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{VoidEventExecutionError, map_void_command_error, validate_void_execution_context};
    use football_application::commands::CommandExecutionContext;
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, ClockMilliseconds, CommandId,
        CorrectionReason, EventId, EventVerificationStatus, MatchId, MatchPeriod, UtcInstant,
        VoidEventCommand,
    };
    use football_platform::{ErrorCategory, ErrorCode};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("void execution fixture UUID should be valid")
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019feb20-3bc5-7cf0-b3d2-0728c077a930",
        )))
    }

    fn command() -> VoidEventCommand {
        VoidEventCommand::try_new(
            CommandId::from_uuid(uuid("019feb20-3bc5-7cf1-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019feb20-3bc5-7cf2-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019feb20-3bc5-7cf3-b3d2-0728c077a930")),
            AggregateVersion::from_value(1),
            MatchPeriod::HalfTime,
            ClockMilliseconds::from_seconds(45).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Verified,
            EventId::from_uuid(uuid("019feb20-3bc5-7cf4-b3d2-0728c077a930")),
            CorrectionReason::try_new("official protocol removed the goal").unwrap(),
        )
        .unwrap()
    }

    fn context(command: &VoidEventCommand) -> CommandExecutionContext {
        CommandExecutionContext::new(
            command.command_id(),
            actor(),
            Some(command.expected_version()),
            uuid("019feb20-3bc5-7cf5-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        )
    }

    #[test]
    fn validates_a_matching_void_execution_context() {
        let command = command();

        assert!(validate_void_execution_context(&context(&command), &command).is_ok());
    }

    #[test]
    fn rejects_missing_or_mismatched_void_execution_context_values() {
        let command = command();
        let valid = context(&command);
        let missing = CommandExecutionContext::new(
            valid.command_id(),
            valid.actor(),
            None,
            valid.processing_owner(),
            valid.processing_expires_at(),
        );
        assert!(matches!(
            validate_void_execution_context(&missing, &command),
            Err(VoidEventExecutionError::ContextExpectedVersionMissing { command_id })
                if command_id == command.command_id()
        ));

        let wrong_actor = CommandExecutionContext::new(
            command.command_id(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019feb20-3bc5-7cf6-b3d2-0728c077a930",
            ))),
            Some(command.expected_version()),
            valid.processing_owner(),
            valid.processing_expires_at(),
        );
        assert!(matches!(
            validate_void_execution_context(&wrong_actor, &command),
            Err(VoidEventExecutionError::ContextActorMismatch { command_id, .. })
                if command_id == command.command_id()
        ));
    }

    #[test]
    fn maps_void_target_and_version_failures_to_stable_codes() {
        let target_not_found = map_void_command_error(
            &football_domain::VoidEventCommandError::TargetEventNotFound {
                event_id: command().voids_event_id(),
            },
        );
        assert_eq!(target_not_found.code(), ErrorCode::ResourceNotFound);
        assert_eq!(target_not_found.category(), ErrorCategory::NotFound);

        let version_conflict =
            map_void_command_error(&football_domain::VoidEventCommandError::EventAppend(
                football_domain::EventAppendError::ExpectedVersionConflict {
                    expected: AggregateVersion::initial(),
                    actual: AggregateVersion::from_value(1),
                },
            ));
        assert_eq!(version_conflict.code(), ErrorCode::ConflictExpectedVersion);
        assert_eq!(version_conflict.category(), ErrorCategory::Conflict);
    }
}
