#![forbid(unsafe_code)]

//! Versioned public and admin transport contracts.

pub mod match_snapshot;
pub mod realtime;

use serde::Serialize;
use std::collections::BTreeMap;

pub use match_snapshot::{
    MATCH_SNAPSHOT_RESPONSE_VERSION, MatchSnapshotData, MatchSnapshotMeta, MatchSnapshotMetaError,
    MatchSnapshotResponse, MatchSnapshotScore, MatchSnapshotTimelineEntry,
    MatchSnapshotVerificationStatus,
};
pub use realtime::{
    REALTIME_MATCH_CORRECTION_NOTIFICATION_SCHEMA_VERSION,
    REALTIME_MATCH_CORRECTION_NOTIFICATION_TYPE, REALTIME_MATCH_NOTIFICATION_SCHEMA_VERSION,
    REALTIME_MATCH_NOTIFICATION_TYPE, REALTIME_MATCH_TOPIC_PREFIX,
    REALTIME_MATCH_VERIFICATION_NOTIFICATION_SCHEMA_VERSION,
    REALTIME_MATCH_VERIFICATION_NOTIFICATION_TYPE, REALTIME_PROTOCOL_MAJOR_VERSION,
    REALTIME_STANDINGS_REVISION_NOTIFICATION_SCHEMA_VERSION,
    REALTIME_STANDINGS_REVISION_NOTIFICATION_TYPE, REALTIME_STANDINGS_TOPIC_PREFIX,
    RealtimeMatchCorrectionNotification, RealtimeMatchNotification,
    RealtimeMatchVerificationNotification, RealtimeNotification, RealtimeSequence,
    RealtimeSnapshotRequiredReason, RealtimeStandingsResultsRevision,
    RealtimeStandingsRevisionNotification, RealtimeSubscriptionRequest,
    RealtimeSubscriptionResponse, RealtimeTopic, RealtimeTopicParseError,
    RealtimeVerificationChangeType,
};

pub const PUBLIC_API_MAJOR_VERSION: u16 = 1;
pub const ADMIN_API_MAJOR_VERSION: u16 = 1;
pub const PUBLIC_API_PREFIX: &str = "/v1";
pub const ADMIN_API_PREFIX: &str = "/admin/v1";

pub type ErrorDetails = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
    pub meta: ApiErrorMeta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub category: String,
    pub message: String,
    pub details: ErrorDetails,
    pub retryable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApiErrorMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_command_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ADMIN_API_PREFIX, PUBLIC_API_PREFIX};

    #[test]
    fn exposes_the_frozen_api_prefixes() {
        assert_eq!(PUBLIC_API_PREFIX, "/v1");
        assert_eq!(ADMIN_API_PREFIX, "/admin/v1");
    }
}
