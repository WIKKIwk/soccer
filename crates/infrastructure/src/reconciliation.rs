use crate::command_results::{
    CommandResultRepositoryError, StoredTerminalCommandResult,
    load_terminal_command_result_in_transaction,
};
use crate::match_events::{
    MatchEventRepositoryError, load_match_event_max_version_in_transaction,
    load_match_events_for_command_in_transaction,
};
use football_domain::{AggregateVersion, CommandId, CommandStatus, DomainEventEnvelope, MatchId};
use football_platform::{ApplicationError, ErrorCode};
use serde_json::Value;
use sqlx::{Error as SqlxError, PgPool};
use std::fmt;
use time::OffsetDateTime;
use uuid::Uuid;

/// The point-in-time disposition of command state and canonical event evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandReconciliationDisposition {
    /// A terminal accepted result is consistent with the command-local events.
    Accepted,
    /// A terminal rejected result is present and no canonical event is visible.
    Rejected,
    /// No canonical event is visible for a non-terminal command at this snapshot.
    NoCanonicalCommitObserved,
    /// Command status, terminal metadata, and event evidence do not agree.
    ReconcileRequired,
}

/// Read-only recovery eligibility derived from a reconciliation snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRecoveryDecision {
    /// Admission may claim the command again, subject to its own transaction.
    SafeToReclaim,
    /// The command has no observed canonical events, but its processing lease
    /// is still active and must not be stolen.
    LeaseStillActive,
    /// The snapshot is terminal or inconsistent and needs no blind recovery.
    NotEligible,
}

/// Read-only command and event evidence captured from one repeatable-read
/// snapshot. This type deliberately does not authorize retry or mutate state.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandReconciliationSnapshot {
    command_id: CommandId,
    status: CommandStatus,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<football_domain::UtcInstant>,
    terminal_result: Option<StoredTerminalCommandResult>,
    canonical_events: Vec<DomainEventEnvelope<Value>>,
    disposition: CommandReconciliationDisposition,
}

impl CommandReconciliationSnapshot {
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    pub const fn status(&self) -> CommandStatus {
        self.status
    }

    pub const fn processing_owner(&self) -> Option<Uuid> {
        self.processing_owner
    }

    pub const fn processing_expires_at(&self) -> Option<football_domain::UtcInstant> {
        self.processing_expires_at
    }

    pub const fn disposition(&self) -> CommandReconciliationDisposition {
        self.disposition
    }

    pub fn terminal_result(&self) -> Option<&StoredTerminalCommandResult> {
        self.terminal_result.as_ref()
    }

    pub fn canonical_events(&self) -> &[DomainEventEnvelope<Value>] {
        &self.canonical_events
    }

    /// Converts only an inconsistent snapshot into a stable application
    /// error. Consistent terminal and no-event observations are state evidence,
    /// not errors and not implicit retry authorization.
    pub fn to_application_error(&self) -> Option<ApplicationError> {
        (self.disposition == CommandReconciliationDisposition::ReconcileRequired).then(|| {
            ApplicationError::new(
                ErrorCode::IdempotencyReconciliationRequired,
                "the command state must be reconciled before another execution attempt",
            )
            .with_detail("command_id", self.command_id.to_string())
        })
    }

    /// Derives a recovery decision without changing command state. A caller
    /// must still run the normal admission transaction before reclaiming.
    pub fn recovery_decision(
        &self,
        now_utc: football_domain::UtcInstant,
    ) -> CommandRecoveryDecision {
        if self.disposition != CommandReconciliationDisposition::NoCanonicalCommitObserved {
            return CommandRecoveryDecision::NotEligible;
        }

        match self.status {
            CommandStatus::Received | CommandStatus::FailedRetryable => {
                CommandRecoveryDecision::SafeToReclaim
            }
            CommandStatus::Processing => match self.processing_expires_at {
                Some(expires_at)
                    if expires_at.as_offset_datetime() <= now_utc.as_offset_datetime() =>
                {
                    CommandRecoveryDecision::SafeToReclaim
                }
                Some(_) => CommandRecoveryDecision::LeaseStillActive,
                None => CommandRecoveryDecision::NotEligible,
            },
            CommandStatus::Accepted | CommandStatus::Rejected => {
                CommandRecoveryDecision::NotEligible
            }
        }
    }
}

#[derive(Debug)]
pub enum CommandReconciliationError {
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
    TerminalResult(CommandResultRepositoryError),
    MatchEvents(MatchEventRepositoryError),
}

impl fmt::Display for CommandReconciliationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => {
                write!(formatter, "command reconciliation query failed: {error}")
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
            Self::TerminalResult(error) => {
                write!(
                    formatter,
                    "command terminal result reconciliation failed: {error}"
                )
            }
            Self::MatchEvents(error) => {
                write!(formatter, "command event reconciliation failed: {error}")
            }
        }
    }
}

impl std::error::Error for CommandReconciliationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::TerminalResult(error) => Some(error),
            Self::MatchEvents(error) => Some(error),
            Self::CommandNotFound { .. }
            | Self::InvalidStoredStatus { .. }
            | Self::InvalidStoredLease { .. } => None,
        }
    }
}

impl CommandReconciliationError {
    /// Maps reconciliation adapter failures to the stable application error
    /// taxonomy without exposing SQL or malformed persisted values.
    pub fn to_application_error(&self) -> ApplicationError {
        match self {
            Self::Database(_) => ApplicationError::new(
                ErrorCode::DependencyDatabaseUnavailable,
                "the command reconciliation dependency is unavailable",
            ),
            Self::CommandNotFound { command_id } => {
                ApplicationError::new(ErrorCode::ResourceNotFound, "the command was not found")
                    .with_detail("command_id", command_id.to_string())
            }
            Self::InvalidStoredStatus { .. } | Self::InvalidStoredLease { .. } => {
                ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command reconciliation store returned invalid persisted state",
                )
            }
            Self::TerminalResult(error) => match error {
                CommandResultRepositoryError::Database(_) => ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the command result dependency is unavailable",
                ),
                _ => ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command reconciliation store returned invalid terminal evidence",
                ),
            },
            Self::MatchEvents(error) => match error {
                MatchEventRepositoryError::Database(_) => ApplicationError::new(
                    ErrorCode::DependencyDatabaseUnavailable,
                    "the match event dependency is unavailable",
                ),
                _ => ApplicationError::new(
                    ErrorCode::InternalUnexpected,
                    "the command reconciliation store returned invalid event evidence",
                ),
            },
        }
    }
}

/// Reads command status, terminal result, and command-local canonical events
/// from one repeatable-read, read-only transaction. No retry or mutation is
/// performed here; callers must act only on the returned disposition.
pub async fn load_command_reconciliation_snapshot(
    pool: &PgPool,
    command_id: CommandId,
) -> Result<CommandReconciliationSnapshot, CommandReconciliationError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(CommandReconciliationError::Database)?;

    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *transaction)
        .await
        .map_err(CommandReconciliationError::Database)?;

    let row = sqlx::query_as::<_, CommandStateRow>(
        r#"
        SELECT status, processing_owner, processing_expires_at
        FROM commands
        WHERE id = $1
        "#,
    )
    .bind(command_id.as_uuid())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(CommandReconciliationError::Database)?
    .ok_or(CommandReconciliationError::CommandNotFound { command_id })?;

    let status = parse_command_status(command_id, row.status)?;
    let processing_expires_at = validate_lease_shape(
        command_id,
        status,
        row.processing_owner,
        row.processing_expires_at,
    )?;
    let terminal_result = if status.is_terminal() {
        Some(
            load_terminal_command_result_in_transaction(&mut transaction, command_id)
                .await
                .map_err(CommandReconciliationError::TerminalResult)?,
        )
    } else {
        None
    };
    let aggregate_max_version = match terminal_result.as_ref() {
        Some(result) => Some(
            load_match_event_max_version_in_transaction(&mut transaction, result.aggregate_id())
                .await
                .map_err(CommandReconciliationError::MatchEvents)?,
        ),
        None => None,
    };
    let canonical_events =
        load_match_events_for_command_in_transaction(&mut transaction, command_id)
            .await
            .map_err(CommandReconciliationError::MatchEvents)?;

    let disposition = classify_reconciliation(
        command_id,
        status,
        terminal_result.as_ref(),
        &canonical_events,
        aggregate_max_version,
    );

    transaction
        .commit()
        .await
        .map_err(CommandReconciliationError::Database)?;

    Ok(CommandReconciliationSnapshot {
        command_id,
        status,
        processing_owner: row.processing_owner,
        processing_expires_at,
        terminal_result,
        canonical_events,
        disposition,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct CommandStateRow {
    status: String,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<OffsetDateTime>,
}

fn validate_lease_shape(
    command_id: CommandId,
    status: CommandStatus,
    processing_owner: Option<Uuid>,
    processing_expires_at: Option<OffsetDateTime>,
) -> Result<Option<football_domain::UtcInstant>, CommandReconciliationError> {
    match (status, processing_owner, processing_expires_at) {
        (CommandStatus::Processing, Some(_), Some(expires_at)) => Ok(Some(
            football_domain::UtcInstant::from_offset_datetime(expires_at),
        )),
        (CommandStatus::Processing, ..) | (_, Some(_), _) | (_, _, Some(_)) => {
            Err(CommandReconciliationError::InvalidStoredLease { command_id })
        }
        (_, None, None) => Ok(None),
    }
}

fn parse_command_status(
    command_id: CommandId,
    value: String,
) -> Result<CommandStatus, CommandReconciliationError> {
    CommandStatus::try_from_persisted_str(&value)
        .ok_or(CommandReconciliationError::InvalidStoredStatus { command_id, value })
}

fn classify_reconciliation(
    command_id: CommandId,
    status: CommandStatus,
    terminal_result: Option<&StoredTerminalCommandResult>,
    canonical_events: &[DomainEventEnvelope<Value>],
    aggregate_max_version: Option<AggregateVersion>,
) -> CommandReconciliationDisposition {
    match (status, terminal_result) {
        (
            CommandStatus::Accepted,
            Some(StoredTerminalCommandResult::Accepted {
                command_id: result_command_id,
                aggregate_id,
                final_aggregate_version,
                event_count,
                ..
            }),
        ) if *result_command_id == command_id
            && usize::try_from(*event_count).ok() == Some(canonical_events.len())
            && accepted_events_match_result(
                command_id,
                *aggregate_id,
                *final_aggregate_version,
                canonical_events,
            )
            && terminal_version_is_not_ahead(*final_aggregate_version, aggregate_max_version) =>
        {
            CommandReconciliationDisposition::Accepted
        }
        (
            CommandStatus::Rejected,
            Some(StoredTerminalCommandResult::Rejected {
                command_id: result_command_id,
                aggregate_version,
                ..
            }),
        ) if *result_command_id == command_id
            && canonical_events.is_empty()
            && terminal_version_is_not_ahead(*aggregate_version, aggregate_max_version) =>
        {
            CommandReconciliationDisposition::Rejected
        }
        (status, None) if !status.is_terminal() && canonical_events.is_empty() => {
            CommandReconciliationDisposition::NoCanonicalCommitObserved
        }
        _ => CommandReconciliationDisposition::ReconcileRequired,
    }
}

fn terminal_version_is_not_ahead(
    terminal_version: AggregateVersion,
    aggregate_max_version: Option<AggregateVersion>,
) -> bool {
    aggregate_max_version.is_some_and(|max_version| terminal_version <= max_version)
}

fn accepted_events_match_result(
    command_id: CommandId,
    aggregate_id: MatchId,
    final_aggregate_version: AggregateVersion,
    events: &[DomainEventEnvelope<Value>],
) -> bool {
    let mut previous_version: Option<AggregateVersion> = None;

    for (index, event) in events.iter().enumerate() {
        let Ok(expected_index) = u32::try_from(index) else {
            return false;
        };
        if event.causation_command_id() != command_id
            || event.match_id() != aggregate_id
            || event.event_index().value() != expected_index
        {
            return false;
        }

        if let Some(previous_version) = previous_version
            && previous_version.try_next().ok() != Some(event.aggregate_version())
        {
            return false;
        }
        previous_version = Some(event.aggregate_version());
    }

    match events.last() {
        Some(last_event) => last_event.aggregate_version() == final_aggregate_version,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandReconciliationDisposition, CommandReconciliationError,
        CommandReconciliationSnapshot, CommandRecoveryDecision, classify_reconciliation,
        validate_lease_shape,
    };
    use crate::command_results::CommandResultRepositoryError;
    use crate::match_events::MatchEventRepositoryError;
    use football_domain::{
        ActorIdentity, AdminUserId, AggregateVersion, ClockMilliseconds, CommandId, CommandStatus,
        DomainEventEnvelope, EventId, EventIndex, EventSchemaVersion, EventType,
        EventVerificationStatus, MatchId, MatchPeriod, UtcInstant,
    };
    use serde_json::{Value, json};
    use sqlx::Error as SqlxError;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("reconciliation fixture UUID should be valid")
    }

    fn command_id() -> CommandId {
        CommandId::from_uuid(uuid("019fe8b0-3bc5-7cf0-b3d2-0728c077a930"))
    }

    fn match_id() -> MatchId {
        MatchId::from_uuid(uuid("019fe8b0-3bc5-7cf1-b3d2-0728c077a930"))
    }

    fn accepted_result(
        event_count: u64,
        final_aggregate_version: AggregateVersion,
    ) -> super::StoredTerminalCommandResult {
        super::StoredTerminalCommandResult::Accepted {
            command_id: command_id(),
            aggregate_id: match_id(),
            completed_at: UtcInstant::from_unix_timestamp(2_005).unwrap(),
            final_aggregate_version,
            event_count,
            result_payload: json!({"ok": true}),
        }
    }

    fn rejected_result() -> super::StoredTerminalCommandResult {
        super::StoredTerminalCommandResult::Rejected {
            command_id: command_id(),
            aggregate_id: match_id(),
            completed_at: UtcInstant::from_unix_timestamp(2_005).unwrap(),
            aggregate_version: AggregateVersion::initial(),
            rejection_code: "domain.invalid_transition".to_owned(),
            rejection_details: json!({"period": "half_time"}),
        }
    }

    fn canonical_event() -> DomainEventEnvelope<Value> {
        canonical_event_with_index(0)
    }

    fn canonical_event_with_index(event_index: u32) -> DomainEventEnvelope<Value> {
        DomainEventEnvelope::new(
            EventId::from_uuid(uuid("019fe8b0-3bc5-7cf2-b3d2-0728c077a930")),
            match_id(),
            AggregateVersion::from_value(1),
            command_id(),
            EventIndex::from_value(event_index),
            EventType::try_new("GoalRecorded").unwrap(),
            EventSchemaVersion::initial(),
            MatchPeriod::FirstHalf,
            ClockMilliseconds::from_millis(12_000),
            UtcInstant::from_unix_timestamp(2_001).unwrap(),
            UtcInstant::from_unix_timestamp(2_002).unwrap(),
            ActorIdentity::administrator(AdminUserId::from_uuid(uuid(
                "019fe8b0-3bc5-7cf3-b3d2-0728c077a930",
            ))),
            EventVerificationStatus::Provisional,
            json!({"fixture": true}),
            None,
            None,
        )
    }

    #[test]
    fn classifies_consistent_terminal_and_no_event_states() {
        let event = canonical_event();
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Accepted,
                Some(&accepted_result(1, AggregateVersion::from_value(1))),
                &[event],
                Some(AggregateVersion::from_value(1)),
            ),
            CommandReconciliationDisposition::Accepted
        );
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Rejected,
                Some(&rejected_result()),
                &[],
                Some(AggregateVersion::initial()),
            ),
            CommandReconciliationDisposition::Rejected
        );
        assert_eq!(
            classify_reconciliation(command_id(), CommandStatus::Processing, None, &[], None,),
            CommandReconciliationDisposition::NoCanonicalCommitObserved
        );
    }

    #[test]
    fn requires_reconciliation_for_inconsistent_terminal_or_event_evidence() {
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Accepted,
                Some(&accepted_result(0, AggregateVersion::initial())),
                &[canonical_event()],
                Some(AggregateVersion::from_value(1)),
            ),
            CommandReconciliationDisposition::ReconcileRequired
        );
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Accepted,
                Some(&accepted_result(1, AggregateVersion::from_value(1))),
                &[canonical_event_with_index(1)],
                Some(AggregateVersion::from_value(1)),
            ),
            CommandReconciliationDisposition::ReconcileRequired
        );
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Accepted,
                Some(&accepted_result(0, AggregateVersion::from_value(1))),
                &[],
                Some(AggregateVersion::initial()),
            ),
            CommandReconciliationDisposition::ReconcileRequired
        );
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Accepted,
                Some(&accepted_result(0, AggregateVersion::from_value(1))),
                &[],
                Some(AggregateVersion::from_value(2)),
            ),
            CommandReconciliationDisposition::Accepted
        );
        let event = canonical_event();
        assert_eq!(
            classify_reconciliation(
                command_id(),
                CommandStatus::Processing,
                None,
                &[event],
                None,
            ),
            CommandReconciliationDisposition::ReconcileRequired
        );
    }

    #[test]
    fn maps_only_inconsistent_snapshots_to_a_non_retryable_application_error() {
        let inconsistent = CommandReconciliationSnapshot {
            command_id: command_id(),
            status: CommandStatus::Accepted,
            processing_owner: None,
            processing_expires_at: None,
            terminal_result: None,
            canonical_events: Vec::new(),
            disposition: CommandReconciliationDisposition::ReconcileRequired,
        };
        let error = inconsistent
            .to_application_error()
            .expect("inconsistent state should require reconciliation");
        assert_eq!(
            error.code(),
            football_platform::ErrorCode::IdempotencyReconciliationRequired
        );
        assert_eq!(
            error.details().get("command_id"),
            Some(&command_id().to_string())
        );
        assert!(!error.retryable());

        let observed = CommandReconciliationSnapshot {
            command_id: command_id(),
            status: CommandStatus::Processing,
            processing_owner: Some(uuid("019fe8b0-3bc5-7cf4-b3d2-0728c077a930")),
            processing_expires_at: Some(UtcInstant::from_unix_timestamp(2_030).unwrap()),
            terminal_result: None,
            canonical_events: Vec::new(),
            disposition: CommandReconciliationDisposition::NoCanonicalCommitObserved,
        };
        assert!(observed.to_application_error().is_none());
    }

    #[test]
    fn allows_reclaim_only_without_events_and_after_processing_lease_expiry() {
        let now = UtcInstant::from_unix_timestamp(2_000).unwrap();
        let received = CommandReconciliationSnapshot {
            command_id: command_id(),
            status: CommandStatus::Received,
            processing_owner: None,
            processing_expires_at: None,
            terminal_result: None,
            canonical_events: Vec::new(),
            disposition: CommandReconciliationDisposition::NoCanonicalCommitObserved,
        };
        assert_eq!(
            received.recovery_decision(now),
            CommandRecoveryDecision::SafeToReclaim
        );

        let active = CommandReconciliationSnapshot {
            command_id: command_id(),
            status: CommandStatus::Processing,
            processing_owner: Some(uuid("019fe8b0-3bc5-7cf4-b3d2-0728c077a930")),
            processing_expires_at: Some(UtcInstant::from_unix_timestamp(2_030).unwrap()),
            terminal_result: None,
            canonical_events: Vec::new(),
            disposition: CommandReconciliationDisposition::NoCanonicalCommitObserved,
        };
        assert_eq!(
            active.recovery_decision(now),
            CommandRecoveryDecision::LeaseStillActive
        );

        let expired = CommandReconciliationSnapshot {
            command_id: command_id(),
            status: CommandStatus::Processing,
            processing_owner: Some(uuid("019fe8b0-3bc5-7cf4-b3d2-0728c077a930")),
            processing_expires_at: Some(UtcInstant::from_unix_timestamp(1_990).unwrap()),
            terminal_result: None,
            canonical_events: Vec::new(),
            disposition: CommandReconciliationDisposition::NoCanonicalCommitObserved,
        };
        assert_eq!(
            expired.recovery_decision(now),
            CommandRecoveryDecision::SafeToReclaim
        );

        let terminal = CommandReconciliationSnapshot {
            command_id: command_id(),
            status: CommandStatus::Accepted,
            processing_owner: None,
            processing_expires_at: None,
            terminal_result: None,
            canonical_events: Vec::new(),
            disposition: CommandReconciliationDisposition::Accepted,
        };
        assert_eq!(
            terminal.recovery_decision(now),
            CommandRecoveryDecision::NotEligible
        );
    }

    #[test]
    fn rejects_malformed_processing_lease_shapes() {
        assert!(matches!(
            validate_lease_shape(
                command_id(),
                CommandStatus::Processing,
                None,
                Some(
                    UtcInstant::from_unix_timestamp(2_030)
                        .unwrap()
                        .as_offset_datetime()
                ),
            ),
            Err(super::CommandReconciliationError::InvalidStoredLease { .. })
        ));
        assert!(matches!(
            validate_lease_shape(
                command_id(),
                CommandStatus::Received,
                Some(uuid("019fe8b0-3bc5-7cf4-b3d2-0728c077a930")),
                None,
            ),
            Err(super::CommandReconciliationError::InvalidStoredLease { .. })
        ));
    }

    #[test]
    fn maps_reconciliation_failures_without_leaking_storage_details() {
        let database = CommandReconciliationError::Database(SqlxError::RowNotFound);
        assert_eq!(
            database.to_application_error().code(),
            football_platform::ErrorCode::DependencyDatabaseUnavailable
        );
        assert!(database.to_application_error().details().is_empty());

        let missing = CommandReconciliationError::CommandNotFound {
            command_id: command_id(),
        };
        let missing_error = missing.to_application_error();
        assert_eq!(
            missing_error.code(),
            football_platform::ErrorCode::ResourceNotFound
        );
        assert_eq!(
            missing_error.details().get("command_id"),
            Some(&command_id().to_string())
        );

        let invalid_status = CommandReconciliationError::InvalidStoredStatus {
            command_id: command_id(),
            value: "unexpected_status".to_owned(),
        };
        assert_eq!(
            invalid_status.to_application_error().code(),
            football_platform::ErrorCode::InternalUnexpected
        );
        assert!(invalid_status.to_application_error().details().is_empty());

        let terminal_database = CommandReconciliationError::TerminalResult(
            CommandResultRepositoryError::Database(SqlxError::RowNotFound),
        );
        assert_eq!(
            terminal_database.to_application_error().code(),
            football_platform::ErrorCode::DependencyDatabaseUnavailable
        );

        let terminal_corruption = CommandReconciliationError::TerminalResult(
            CommandResultRepositoryError::InvalidStoredTerminalResult {
                command_id: command_id(),
                field: "aggregate_version",
            },
        );
        assert_eq!(
            terminal_corruption.to_application_error().code(),
            football_platform::ErrorCode::InternalUnexpected
        );

        let event_database = CommandReconciliationError::MatchEvents(
            MatchEventRepositoryError::Database(SqlxError::RowNotFound),
        );
        assert_eq!(
            event_database.to_application_error().code(),
            football_platform::ErrorCode::DependencyDatabaseUnavailable
        );

        let event_corruption = CommandReconciliationError::MatchEvents(
            MatchEventRepositoryError::InvalidStoredEventIndex {
                event_id: EventId::from_uuid(uuid("019fe8b0-3bc5-7cf2-b3d2-0728c077a930")),
                value: -1,
            },
        );
        assert_eq!(
            event_corruption.to_application_error().code(),
            football_platform::ErrorCode::InternalUnexpected
        );
    }
}
