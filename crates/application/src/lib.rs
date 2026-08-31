#![forbid(unsafe_code)]

//! Use-case orchestration and ports between transport adapters and the domain.
//!
//! Business rules remain in `football-domain`; this crate coordinates commands,
//! queries, authorization, and transaction boundaries.

pub mod commands;
pub mod identity;
pub mod match_command_fingerprint;
pub mod match_snapshot;
pub mod ports;
pub mod realtime_authorization;
pub mod realtime_gap;
pub mod realtime_heartbeat;
pub mod realtime_retention;
pub mod realtime_snapshot;
pub mod realtime_standings;
pub mod verification_authorization;

pub use commands::{
    CommandAdmissionPort, CommandAdmissionRequest, CommandAdmissionRequestError,
    CommandAdmissionResult, CommandExecutionContext, CommandExecutionPort,
    CommandOrchestrationError, CommandOrchestrationResult, CommandOrchestrator,
    LogicalCommandIdentity, PersistedCommandResult, fingerprint_canonical_bytes,
    into_persisted_command_result,
};
pub use match_snapshot::build_canonical_match_snapshot;
pub use ports::{ClockPort, MatchClockCoordinator};
pub use realtime_authorization::{
    RealtimePrincipal, RealtimeSubscriptionOperation, RealtimeTopicAuthorizationError,
    RealtimeTopicAuthorizationFacts, authorize_realtime_subscription,
};
pub use realtime_gap::{
    RealtimeReplayAssessment, RealtimeRetainedSequenceError, RealtimeSequenceGap,
    assess_realtime_replay,
};
pub use realtime_heartbeat::{
    DEFAULT_REALTIME_HEARTBEAT_INTERVAL_MILLIS, DEFAULT_REALTIME_MAX_INBOUND_FRAME_BYTES,
    DEFAULT_REALTIME_MAX_PENDING_OUTBOUND_NOTIFICATIONS, DEFAULT_REALTIME_MAX_SUBSCRIPTIONS,
    DEFAULT_REALTIME_PONG_TIMEOUT_MILLIS, RealtimeConnectionLimitError, RealtimeConnectionLimits,
    RealtimeConnectionLimitsError, RealtimeHeartbeatAction, RealtimeHeartbeatError,
    RealtimeHeartbeatState, RealtimePongDisposition,
};
pub use realtime_retention::{
    DEFAULT_REALTIME_RETAINED_MESSAGE_MAX_AGE_MILLIS, DEFAULT_REALTIME_RETAINED_MESSAGES_PER_TOPIC,
    RealtimeRetainedMessageWindow, RealtimeRetentionPolicy, RealtimeRetentionPolicyError,
};
pub use realtime_snapshot::realtime_snapshot_required_response;
pub use realtime_standings::{
    RealtimeStandingsRevisionBuildError, build_realtime_standings_revision_notification,
};
pub use verification_authorization::{
    MatchVerificationAuthorizationError, MatchVerificationAuthorizationFacts,
    MatchVerificationOperation, MatchVerificationPrincipal, authorize_match_verification,
};
