#![forbid(unsafe_code)]

//! PostgreSQL, storage, realtime, and security adapters.
//!
//! Infrastructure implements ports owned by the application/domain boundary;
//! it does not move business rules into database or transport code.

pub mod admin_users;
pub mod audit;
pub mod c1_command_execution;
pub mod c1_event_payloads;
pub mod c1_execution;
pub mod command_results;
pub mod commands;
pub mod competition;
pub mod correction_execution;
pub mod fixtures;
pub mod lineups;
pub mod match_event_payloads;
pub mod match_events;
pub mod match_projections;
pub mod match_queries;
pub mod migrations;
pub mod operations;
pub mod outbox;
pub mod players;
pub mod postgres;
pub mod realtime_latency;
pub mod realtime_notifications;
pub mod realtime_sequence;
pub mod reconciliation;
pub mod reopen_execution;
pub mod synchronous_match_state;
pub mod teams;
pub mod verification_execution;

pub use admin_users::{
    AdminUserRepositoryError, PersistedAdminUser, load_admin_user, persist_active_admin,
};
pub use audit::{
    CommandAuditEntry, CommandAuditEntryError, CommandAuditOutcome, CommandAuditRepositoryError,
    CriticalDecisionAudit, StoredCommandAudit, load_command_audit, persist_command_audit,
};
pub use c1_command_execution::{
    C1LiveActionCommandExecutionError, C1LiveActionResult, PostgresC1LiveActionExecutor,
};
pub use c1_event_payloads::{
    C1EventPayloadCodecError, decode_c1_event_envelope, decode_c1_event_payload,
    encode_c1_event_envelope, encode_c1_event_payload,
};
pub use c1_execution::{
    C1LiveActionExecutionError, C1LiveActionTransactionOutcome,
    execute_c1_live_action_in_canonical_stream_transaction,
    execute_c1_live_action_in_canonical_stream_transaction_outcome,
    execute_c1_live_action_in_transaction, execute_c1_live_action_in_transaction_outcome,
};
pub use command_results::{
    CommandResultRepositoryError, StoredTerminalCommandResult, TerminalCommandResult,
    TerminalCommandWrite, load_terminal_command_result,
    load_terminal_command_result_in_transaction, persist_application_command_result,
    persist_terminal_command_result,
};
pub use commands::{
    CommandAdmissionError, CommandExecutionLeaseError, CommandRetryError, PostgresCommandAdmission,
    admit_command, mark_command_failed_retryable, verify_processing_lease,
};
pub use competition::{
    CompetitionRepositoryError, advance_season_results_revision, load_competition, load_season,
    persist_competition, persist_season,
};
pub use correction_execution::{VoidEventExecutionError, execute_void_event_in_transaction};
pub use fixtures::{FixtureRepositoryError, load_fixture, persist_fixture};
pub use football_application::commands::{CommandAdmissionResult, PersistedCommandResult};
pub use lineups::{LineupRepositoryError, load_draft_lineup, persist_draft_lineup};
pub use match_event_payloads::{
    MatchEventPayloadCodecError, decode_match_event_envelope, encode_match_event_envelope,
};
pub use match_events::{
    MatchEventRepositoryError, load_c1_event_stream, load_c1_event_stream_in_transaction,
    load_c1_events_for_command, load_c1_events_for_command_in_transaction,
    load_canonical_match_event_stream, load_canonical_match_event_stream_in_transaction,
    load_canonical_match_events_for_command,
    load_canonical_match_events_for_command_in_transaction,
    load_match_event_max_version_in_transaction, load_match_event_stream,
    load_match_event_stream_in_transaction, load_match_events_for_command,
    load_match_events_for_command_in_transaction, persist_c1_event_batch,
    persist_canonical_match_event_batch, persist_match_event_batch,
};
pub use match_projections::{
    MatchProjectionRebuildError, RebuiltMatchProjection, rebuild_match_projection,
};
pub use match_queries::{
    CanonicalMatchQuery, CanonicalMatchQueryError, load_canonical_match_query,
};
pub use migrations::{MIGRATIONS_DIRECTORY, load_migrator, run_migrations};
pub use operations::{
    OperatorAssignmentRepositoryError, claim_operator_lease, heartbeat_operator_lease,
    load_match_assignment, load_operator_lease, persist_match_assignment, release_operator_lease,
    takeover_operator_lease,
};
pub use outbox::{
    MATCH_EVENT_ENVELOPE_SCHEMA_VERSION, MATCH_EVENT_MESSAGE_TYPE, OUTBOX_MAX_DELIVERY_ATTEMPTS,
    OUTBOX_POISON_REASON_MAX_ATTEMPTS, OUTBOX_RETRY_BASE_DELAY_SECONDS,
    OUTBOX_RETRY_MAX_DELAY_SECONDS, OutboxDeliveryStatus, OutboxMessage, OutboxMessageError,
    OutboxPoisonedMessage, OutboxPublishFailureDisposition, OutboxPublishedMessage,
    OutboxRepositoryError, OutboxRetrySchedule, StoredOutboxMessage, claim_outbox_messages,
    handle_outbox_publish_failure, load_outbox_message, mark_outbox_published,
    outbox_retry_delay_seconds, persist_outbox_message,
};
pub use players::{
    PersistedPlayerAssignment, PlayerRepositoryError, load_player_assignment,
    persist_player_for_team,
};
pub type DatabasePool = sqlx::PgPool;

pub use postgres::{
    DatabasePoolConfig, DatabasePoolError, PoolConfigError, check_database_readiness,
    connect_database, connect_database_lazy,
};
pub use realtime_latency::{
    REALTIME_FANOUT_LATENCY_MEASUREMENT, RealtimeFanoutLatencyError, RealtimeFanoutLatencySample,
    measure_realtime_fanout_latency,
};
pub use realtime_notifications::{
    RealtimeNotificationBuildError, RealtimeNotificationPreparationError,
    build_realtime_match_correction_notification, build_realtime_match_notification,
    build_realtime_match_verification_notification, build_realtime_notification,
    prepare_realtime_match_correction_notification, prepare_realtime_match_notification,
    prepare_realtime_match_verification_notification, prepare_realtime_notification,
};
pub use realtime_sequence::{
    AllocatedRealtimeTopicSequence, RealtimeSequenceRepositoryError,
    allocate_realtime_topic_sequence, load_realtime_topic_sequence,
};
pub use reconciliation::{
    CommandReconciliationDisposition, CommandReconciliationError, CommandReconciliationSnapshot,
    CommandRecoveryDecision, load_command_reconciliation_snapshot,
};
pub use reopen_execution::{ReopenMatchExecutionError, execute_reopen_match_in_transaction};
pub use synchronous_match_state::{
    SynchronousMatchState, SynchronousMatchStateError, persist_synchronous_match_state,
};
pub use teams::{TeamRepositoryError, load_team, persist_team, persist_two_teams};
pub use verification_execution::{VerifyMatchExecutionError, execute_verify_match_in_transaction};
