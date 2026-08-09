use crate::commands::{CommandExecutionLeaseError, verify_processing_lease};
use crate::match_events::{
    MatchEventRepositoryError, load_c1_event_stream_in_transaction,
    load_canonical_match_event_stream_in_transaction, persist_c1_event_batch,
    persist_canonical_match_event_batch,
};
use football_application::commands::CommandExecutionContext;
use football_domain::{
    ActorIdentity, AggregateVersion, C1EventPayload, C1LiveActionCommand, C1LiveActionCommandError,
    CanonicalEventStream, CommandId, EventAppendError, MatchEventPayload, UtcInstant,
};
use football_platform::{ApplicationError, ErrorCode};
use sqlx::{Error as SqlxError, Postgres, Transaction};
use std::fmt;

const POSTGRES_UNIQUE_VIOLATION: &str = "23505";
const MATCH_EVENTS_PRIMARY_KEY: &str = "match_events_pkey";
const MATCH_EVENTS_AGGREGATE_ORDER_UNIQUE: &str = "match_events_aggregate_order_unique";
const MATCH_EVENTS_COMMAND_INDEX_UNIQUE: &str = "match_events_command_index_unique";

/// Errors raised while composing one C1 action with the caller-owned domain
/// transaction. Terminal command results, audit rows, outbox messages, and
/// the final commit remain owned by the surrounding execution port.
#[derive(Debug)]
pub enum C1LiveActionExecutionError {
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
    Command(C1LiveActionCommandError),
    Persist(MatchEventRepositoryError),
}

/// Outcome of the C1 event-store portion of one command transaction.
///
/// Deterministic domain rejections are returned with the observed aggregate
/// version so the surrounding application executor can persist a terminal
/// `REJECTED` command result without writing a canonical event. Storage,
/// lease, and malformed-event failures remain execution errors and must roll
/// the transaction back.
#[derive(Debug)]
pub enum C1LiveActionTransactionOutcome<T> {
    Accepted(CanonicalEventStream<T>),
    Rejected {
        match_id: football_domain::MatchId,
        aggregate_version: AggregateVersion,
        error: C1LiveActionCommandError,
    },
}

impl fmt::Display for C1LiveActionExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextCommandIdMismatch { expected, actual } => write!(
                formatter,
                "C1 execution context command {actual} does not match command {expected}"
            ),
            Self::ContextActorMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "C1 command {command_id} actor mismatch: expected {expected:?}, got {actual:?}"
            ),
            Self::ContextExpectedVersionMissing { command_id } => write!(
                formatter,
                "C1 command {command_id} requires an expected aggregate version"
            ),
            Self::ContextExpectedVersionMismatch {
                command_id,
                expected,
                actual,
            } => write!(
                formatter,
                "C1 command {command_id} context expected version {}, command carries {}",
                expected.value(),
                actual.value()
            ),
            Self::Lease(error) => write!(formatter, "C1 command lease validation failed: {error}"),
            Self::Load(error) => write!(formatter, "C1 event stream load failed: {error}"),
            Self::Command(error) => write!(formatter, "C1 command validation failed: {error}"),
            Self::Persist(error) => write!(formatter, "C1 event persistence failed: {error}"),
        }
    }
}

impl std::error::Error for C1LiveActionExecutionError {
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

impl C1LiveActionExecutionError {
    /// Maps execution failures to stable application codes. Known canonical
    /// event-store conflicts retain their stable conflict codes; unknown
    /// database errors, malformed stored streams, and context-integrity
    /// failures remain dependency/internal errors.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::ContextCommandIdMismatch { .. }
            | Self::ContextActorMismatch { .. }
            | Self::ContextExpectedVersionMismatch { .. } => ApplicationError::new(
                ErrorCode::InternalUnexpected,
                "the C1 execution context does not match the command",
            ),
            Self::ContextExpectedVersionMissing { .. } => ApplicationError::new(
                ErrorCode::ValidationRequiredField,
                "C1 live actions require an expected aggregate version",
            )
            .with_detail("field", "expected_version"),
            Self::Lease(error) => error.to_application_error(),
            Self::Load(error) | Self::Persist(error) => map_match_event_repository_error(error),
            Self::Command(error) => map_c1_command_error(error),
        }
    }
}

pub(crate) fn map_match_event_repository_error(
    error: &MatchEventRepositoryError,
) -> ApplicationError {
    match error {
        MatchEventRepositoryError::Database(error) => map_match_event_database_error(error),
        _ => ApplicationError::new(
            ErrorCode::InternalUnexpected,
            "the match event store returned invalid persisted state",
        ),
    }
}

fn map_match_event_database_error(error: &SqlxError) -> ApplicationError {
    let Some(database_error) = error.as_database_error() else {
        return ApplicationError::new(
            ErrorCode::DependencyDatabaseUnavailable,
            "the match event dependency is unavailable",
        );
    };

    match match_event_conflict_code(
        database_error.code().as_deref(),
        database_error.constraint(),
    ) {
        Some(ErrorCode::ConflictExpectedVersion) => ApplicationError::new(
            ErrorCode::ConflictExpectedVersion,
            "the match changed before this command was applied",
        ),
        Some(ErrorCode::ConflictUniqueConstraint) => ApplicationError::new(
            ErrorCode::ConflictUniqueConstraint,
            "the canonical match event conflicts with an existing event",
        ),
        Some(code) => ApplicationError::new(code, "the match event operation was rejected"),
        None => ApplicationError::new(
            ErrorCode::DependencyDatabaseUnavailable,
            "the match event dependency is unavailable",
        ),
    }
}

/// Narrows only the event-store constraints whose conflict semantics are
/// defined by the canonical event schema. Unknown database constraints remain
/// dependency failures instead of being guessed into a public conflict code.
fn match_event_conflict_code(
    sqlstate: Option<&str>,
    constraint: Option<&str>,
) -> Option<ErrorCode> {
    if sqlstate != Some(POSTGRES_UNIQUE_VIOLATION) {
        return None;
    }

    match constraint {
        Some(MATCH_EVENTS_AGGREGATE_ORDER_UNIQUE) => Some(ErrorCode::ConflictExpectedVersion),
        Some(MATCH_EVENTS_PRIMARY_KEY | MATCH_EVENTS_COMMAND_INDEX_UNIQUE) => {
            Some(ErrorCode::ConflictUniqueConstraint)
        }
        _ => None,
    }
}

fn map_c1_command_error(error: &C1LiveActionCommandError) -> ApplicationError {
    match error {
        C1LiveActionCommandError::NotLivePeriod {
            command_name,
            period,
        } => ApplicationError::new(
            ErrorCode::DomainInvalidTransition,
            "a C1 live action is not valid during this match period",
        )
        .with_detail("command", *command_name)
        .with_detail("period", format!("{period:?}")),
        C1LiveActionCommandError::EventAppend(EventAppendError::ExpectedVersionConflict {
            expected,
            actual,
        }) => ApplicationError::new(
            ErrorCode::ConflictExpectedVersion,
            "the match changed before this command was applied",
        )
        .with_detail("expected_version", expected.value().to_string())
        .with_detail("actual_version", actual.value().to_string()),
        C1LiveActionCommandError::InvalidEventType(_)
        | C1LiveActionCommandError::EventAppend(_) => ApplicationError::new(
            ErrorCode::InternalUnexpected,
            "the C1 command could not produce a valid canonical event",
        ),
    }
}

/// Loads, validates, appends, and persists one C1 live action in the caller's
/// transaction. The function deliberately returns the resulting canonical
/// stream instead of committing it; terminal result and other atomic writes
/// must be composed by the surrounding execution adapter.
pub async fn execute_c1_live_action_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: C1LiveActionCommand,
    now_utc: UtcInstant,
) -> Result<CanonicalEventStream<C1EventPayload>, C1LiveActionExecutionError> {
    match execute_c1_live_action_in_transaction_outcome(transaction, context, command, now_utc)
        .await?
    {
        C1LiveActionTransactionOutcome::Accepted(stream) => Ok(stream),
        C1LiveActionTransactionOutcome::Rejected { error, .. } => {
            Err(C1LiveActionExecutionError::Command(error))
        }
    }
}

/// Loads, validates, and appends one C1 live action in the caller's
/// transaction, returning deterministic command conflicts as a terminal
/// domain outcome. The accepted stream remains uncommitted.
pub async fn execute_c1_live_action_in_transaction_outcome(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: C1LiveActionCommand,
    now_utc: UtcInstant,
) -> Result<C1LiveActionTransactionOutcome<C1EventPayload>, C1LiveActionExecutionError> {
    validate_c1_execution_context(context, &command)?;

    verify_processing_lease(
        transaction,
        command.command_id(),
        context.processing_owner(),
        now_utc,
    )
    .await
    .map_err(C1LiveActionExecutionError::Lease)?;

    let stream = load_c1_event_stream_in_transaction(transaction, command.match_id())
        .await
        .map_err(C1LiveActionExecutionError::Load)?;
    let current_version = stream.current_version();
    let existing_event_count = stream.len();
    let match_id = command.match_id();
    let appended_stream = match command.append_to_stream(stream) {
        Ok(stream) => stream,
        Err(error) if is_terminal_c1_rejection(&error) => {
            return Ok(C1LiveActionTransactionOutcome::Rejected {
                match_id,
                aggregate_version: current_version,
                error,
            });
        }
        Err(error) => return Err(C1LiveActionExecutionError::Command(error)),
    };

    persist_c1_event_batch(
        transaction,
        current_version,
        &appended_stream.events()[existing_event_count..],
    )
    .await
    .map_err(C1LiveActionExecutionError::Persist)?;

    Ok(C1LiveActionTransactionOutcome::Accepted(appended_stream))
}

/// Loads, validates, appends, and persists one C1 live action against a
/// canonical stream that may contain correction markers. The returned stream
/// remains uncommitted so the surrounding execution adapter can compose the
/// terminal result, audit/outbox writes, and commit decision.
pub async fn execute_c1_live_action_in_canonical_stream_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: C1LiveActionCommand,
    now_utc: UtcInstant,
) -> Result<CanonicalEventStream<MatchEventPayload>, C1LiveActionExecutionError> {
    match execute_c1_live_action_in_canonical_stream_transaction_outcome(
        transaction,
        context,
        command,
        now_utc,
    )
    .await?
    {
        C1LiveActionTransactionOutcome::Accepted(stream) => Ok(stream),
        C1LiveActionTransactionOutcome::Rejected { error, .. } => {
            Err(C1LiveActionExecutionError::Command(error))
        }
    }
}

/// Loads, validates, and appends one C1 live action against the canonical C1
/// plus correction union, returning deterministic command conflicts as a
/// terminal domain outcome. The accepted stream remains uncommitted.
pub async fn execute_c1_live_action_in_canonical_stream_transaction_outcome(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommandExecutionContext,
    command: C1LiveActionCommand,
    now_utc: UtcInstant,
) -> Result<C1LiveActionTransactionOutcome<MatchEventPayload>, C1LiveActionExecutionError> {
    validate_c1_execution_context(context, &command)?;

    verify_processing_lease(
        transaction,
        command.command_id(),
        context.processing_owner(),
        now_utc,
    )
    .await
    .map_err(C1LiveActionExecutionError::Lease)?;

    let stream = load_canonical_match_event_stream_in_transaction(transaction, command.match_id())
        .await
        .map_err(C1LiveActionExecutionError::Load)?;
    let current_version = stream.current_version();
    let existing_event_count = stream.len();
    let match_id = command.match_id();
    let appended_stream = match command.append_to_canonical_stream(stream) {
        Ok(stream) => stream,
        Err(error) if is_terminal_c1_rejection(&error) => {
            return Ok(C1LiveActionTransactionOutcome::Rejected {
                match_id,
                aggregate_version: current_version,
                error,
            });
        }
        Err(error) => return Err(C1LiveActionExecutionError::Command(error)),
    };

    persist_canonical_match_event_batch(
        transaction,
        current_version,
        &appended_stream.events()[existing_event_count..],
    )
    .await
    .map_err(C1LiveActionExecutionError::Persist)?;

    Ok(C1LiveActionTransactionOutcome::Accepted(appended_stream))
}

fn validate_c1_execution_context(
    context: &CommandExecutionContext,
    command: &C1LiveActionCommand,
) -> Result<(), C1LiveActionExecutionError> {
    if context.command_id() != command.command_id() {
        return Err(C1LiveActionExecutionError::ContextCommandIdMismatch {
            expected: command.command_id(),
            actual: context.command_id(),
        });
    }

    if context.actor() != command.actor() {
        return Err(C1LiveActionExecutionError::ContextActorMismatch {
            command_id: command.command_id(),
            expected: context.actor(),
            actual: command.actor(),
        });
    }

    let Some(expected_version) = context.expected_version() else {
        return Err(C1LiveActionExecutionError::ContextExpectedVersionMissing {
            command_id: command.command_id(),
        });
    };
    if expected_version != command.expected_version() {
        return Err(C1LiveActionExecutionError::ContextExpectedVersionMismatch {
            command_id: command.command_id(),
            expected: expected_version,
            actual: command.expected_version(),
        });
    }

    Ok(())
}

fn is_terminal_c1_rejection(error: &C1LiveActionCommandError) -> bool {
    matches!(
        error,
        C1LiveActionCommandError::NotLivePeriod { .. }
            | C1LiveActionCommandError::EventAppend(
                EventAppendError::ExpectedVersionConflict { .. }
            )
    )
}

#[cfg(test)]
mod tests {
    use super::{
        C1LiveActionExecutionError, match_event_conflict_code, validate_c1_execution_context,
    };
    use football_application::commands::CommandExecutionContext;
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, C1LiveAction, C1LiveActionCommand,
        C1LiveActionCommandError, ClockMilliseconds, CommandId, EventAppendError, EventId,
        EventVerificationStatus, GoalAwardType, GoalRecordedPayload, MatchId, MatchPeriod, TeamId,
        UtcInstant,
    };
    use football_platform::{ErrorCategory, ErrorCode};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("C1 execution fixture UUID should be valid")
    }

    fn actor() -> ActorIdentity {
        ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe6b0-3bc5-7cf0-b3d2-0728c077a930",
        )))
    }

    fn command() -> C1LiveActionCommand {
        C1LiveActionCommand::try_new(
            CommandId::from_uuid(uuid("019fe6b0-3bc5-7cf1-b3d2-0728c077a930")),
            EventId::from_uuid(uuid("019fe6b0-3bc5-7cf2-b3d2-0728c077a930")),
            MatchId::from_uuid(uuid("019fe6b0-3bc5-7cf3-b3d2-0728c077a930")),
            AggregateVersion::initial(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_seconds(12).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_000).unwrap(),
            UtcInstant::from_unix_timestamp(1_777_000_001).unwrap(),
            actor(),
            EventVerificationStatus::Provisional,
            C1LiveAction::RecordGoal(GoalRecordedPayload::new(
                TeamId::from_uuid(uuid("019fe6b0-3bc5-7cf4-b3d2-0728c077a930")),
                None,
                GoalAwardType::Regular,
            )),
        )
        .unwrap()
    }

    fn context(command: &C1LiveActionCommand) -> CommandExecutionContext {
        CommandExecutionContext::new(
            command.command_id(),
            actor(),
            Some(command.expected_version()),
            uuid("019fe6b0-3bc5-7cf5-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        )
    }

    #[test]
    fn validates_matching_c1_context_before_opening_the_database_transaction() {
        let command = command();

        assert!(validate_c1_execution_context(&context(&command), &command).is_ok());
    }

    #[test]
    fn rejects_missing_or_stale_expected_version_in_the_execution_context() {
        let command = command();
        let mut missing = context(&command);
        missing = CommandExecutionContext::new(
            missing.command_id(),
            missing.actor(),
            None,
            missing.processing_owner(),
            missing.processing_expires_at(),
        );
        assert!(matches!(
            validate_c1_execution_context(&missing, &command),
            Err(C1LiveActionExecutionError::ContextExpectedVersionMissing { command_id })
                if command_id == command.command_id()
        ));

        let stale = CommandExecutionContext::new(
            command.command_id(),
            actor(),
            Some(AggregateVersion::from_value(1)),
            uuid("019fe6b0-3bc5-7cf5-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        );
        assert!(matches!(
            validate_c1_execution_context(&stale, &command),
            Err(C1LiveActionExecutionError::ContextExpectedVersionMismatch {
                command_id,
                expected,
                actual,
            }) if command_id == command.command_id()
                && expected == AggregateVersion::from_value(1)
                && actual == AggregateVersion::initial()
        ));
    }

    #[test]
    fn rejects_context_actor_or_command_identity_mismatch() {
        let command = command();
        let wrong_actor = ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
            "019fe6b0-3bc5-7cf6-b3d2-0728c077a930",
        )));
        let actor_mismatch = CommandExecutionContext::new(
            command.command_id(),
            wrong_actor,
            Some(command.expected_version()),
            uuid("019fe6b0-3bc5-7cf5-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        );
        assert!(matches!(
            validate_c1_execution_context(&actor_mismatch, &command),
            Err(C1LiveActionExecutionError::ContextActorMismatch { command_id, .. })
                if command_id == command.command_id()
        ));

        let identity_mismatch = CommandExecutionContext::new(
            CommandId::from_uuid(uuid("019fe6b0-3bc5-7cf7-b3d2-0728c077a930")),
            actor(),
            Some(command.expected_version()),
            uuid("019fe6b0-3bc5-7cf5-b3d2-0728c077a930"),
            UtcInstant::from_unix_timestamp(1_777_000_030).unwrap(),
        );
        assert!(matches!(
            validate_c1_execution_context(&identity_mismatch, &command),
            Err(C1LiveActionExecutionError::ContextCommandIdMismatch { expected, actual })
                if expected == command.command_id() && actual != expected
        ));
    }

    #[test]
    fn maps_c1_domain_and_version_failures_to_stable_application_codes() {
        let version_conflict = C1LiveActionExecutionError::Command(
            C1LiveActionCommandError::EventAppend(EventAppendError::ExpectedVersionConflict {
                expected: AggregateVersion::initial(),
                actual: AggregateVersion::from_value(1),
            }),
        )
        .to_application_error();
        assert_eq!(version_conflict.code(), ErrorCode::ConflictExpectedVersion);
        assert_eq!(version_conflict.category(), ErrorCategory::Conflict);
        assert_eq!(
            version_conflict.details().get("expected_version"),
            Some(&"0".to_owned())
        );
        assert_eq!(
            version_conflict.details().get("actual_version"),
            Some(&"1".to_owned())
        );

        let period_rejection =
            C1LiveActionExecutionError::Command(C1LiveActionCommandError::NotLivePeriod {
                command_name: "RecordGoal",
                period: MatchPeriod::HalfTime,
            })
            .to_application_error();
        assert_eq!(period_rejection.code(), ErrorCode::DomainInvalidTransition);
        assert_eq!(period_rejection.category(), ErrorCategory::Domain);
        assert_eq!(
            period_rejection.details().get("command"),
            Some(&"RecordGoal".to_owned())
        );

        let missing_version = C1LiveActionExecutionError::ContextExpectedVersionMissing {
            command_id: command().command_id(),
        }
        .to_application_error();
        assert_eq!(missing_version.code(), ErrorCode::ValidationRequiredField);
        assert_eq!(missing_version.category(), ErrorCategory::Validation);
        assert_eq!(
            missing_version.details().get("field"),
            Some(&"expected_version".to_owned())
        );
    }

    #[test]
    fn maps_only_known_match_event_unique_constraints_to_conflict_codes() {
        assert_eq!(
            match_event_conflict_code(Some("23505"), Some("match_events_aggregate_order_unique")),
            Some(ErrorCode::ConflictExpectedVersion)
        );
        assert_eq!(
            match_event_conflict_code(Some("23505"), Some("match_events_pkey")),
            Some(ErrorCode::ConflictUniqueConstraint)
        );
        assert_eq!(
            match_event_conflict_code(Some("23505"), Some("match_events_command_index_unique")),
            Some(ErrorCode::ConflictUniqueConstraint)
        );
    }

    #[test]
    fn leaves_unknown_or_non_unique_database_constraints_unmapped() {
        assert_eq!(
            match_event_conflict_code(Some("23505"), Some("other_unique_constraint")),
            None
        );
        assert_eq!(
            match_event_conflict_code(Some("40001"), Some("match_events_aggregate_order_unique")),
            None
        );
        assert_eq!(match_event_conflict_code(Some("23505"), None), None);
    }

    #[test]
    fn hides_c1_storage_and_context_integrity_details() {
        let storage_error = C1LiveActionExecutionError::Load(
            super::MatchEventRepositoryError::Database(sqlx::Error::RowNotFound),
        )
        .to_application_error();
        assert_eq!(
            storage_error.code(),
            ErrorCode::DependencyDatabaseUnavailable
        );
        assert!(storage_error.details().is_empty());
        assert!(!storage_error.to_string().contains("RowNotFound"));

        let context_error = C1LiveActionExecutionError::ContextCommandIdMismatch {
            expected: command().command_id(),
            actual: CommandId::from_uuid(uuid("019fe6b0-3bc5-7cf7-b3d2-0728c077a930")),
        }
        .to_application_error();
        assert_eq!(context_error.code(), ErrorCode::InternalUnexpected);
        assert!(context_error.details().is_empty());
        assert!(
            !context_error
                .to_string()
                .contains(&command().command_id().to_string())
        );
    }
}
