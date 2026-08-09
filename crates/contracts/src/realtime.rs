use crate::match_snapshot::MatchSnapshotVerificationStatus;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use std::{fmt, str::FromStr};
use uuid::Uuid;

/// The independently versioned WebSocket protocol major.
pub const REALTIME_PROTOCOL_MAJOR_VERSION: u16 = 1;

/// Stable topic namespace for one match's live notifications.
pub const REALTIME_MATCH_TOPIC_PREFIX: &str = "match";
/// Stable topic namespace for one season's standings revision notifications.
pub const REALTIME_STANDINGS_TOPIC_PREFIX: &str = "standings";
pub const REALTIME_MATCH_NOTIFICATION_TYPE: &str = "match.event";
pub const REALTIME_MATCH_NOTIFICATION_SCHEMA_VERSION: u32 = 1;
pub const REALTIME_MATCH_CORRECTION_NOTIFICATION_TYPE: &str = "match.correction";
pub const REALTIME_MATCH_CORRECTION_NOTIFICATION_SCHEMA_VERSION: u32 = 1;
pub const REALTIME_MATCH_VERIFICATION_NOTIFICATION_TYPE: &str = "match.verification";
pub const REALTIME_MATCH_VERIFICATION_NOTIFICATION_SCHEMA_VERSION: u32 = 1;
pub const REALTIME_STANDINGS_REVISION_NOTIFICATION_TYPE: &str = "standings.revision";
pub const REALTIME_STANDINGS_REVISION_NOTIFICATION_SCHEMA_VERSION: u32 = 1;

/// A positive delivery sequence scoped to one realtime topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RealtimeSequence(u64);

impl RealtimeSequence {
    pub const fn first() -> Self {
        Self(1)
    }

    pub const fn try_new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for RealtimeSequence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::try_new(value)
            .ok_or_else(|| de::Error::custom("realtime sequence must be a positive integer"))
    }
}

/// A positive source-results revision emitted when a season's verified result
/// revision advances. A season may start at revision zero, but a revision
/// change notification always carries the post-increment positive value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct RealtimeStandingsResultsRevision(u64);

impl RealtimeStandingsResultsRevision {
    pub const fn try_new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for RealtimeStandingsResultsRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        Self::try_new(value).ok_or_else(|| {
            de::Error::custom("standings source results revision must be a positive integer")
        })
    }
}

/// A versioned match notification ready for post-commit realtime delivery.
///
/// The notification ID is the stable outbox/source-event identity and the
/// consumer deduplication key. Retries of one outbox message reuse the same
/// notification ID, topic, and sequence. The payload remains the versioned
/// match-event envelope rather than a command request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RealtimeMatchNotification {
    #[serde(rename = "type")]
    notification_type: &'static str,
    schema_version: u32,
    notification_id: Uuid,
    topic: RealtimeTopic,
    sequence: RealtimeSequence,
    payload: Value,
}

impl RealtimeMatchNotification {
    pub fn new(
        notification_id: Uuid,
        topic: RealtimeTopic,
        sequence: RealtimeSequence,
        payload: Value,
    ) -> Self {
        Self {
            notification_type: REALTIME_MATCH_NOTIFICATION_TYPE,
            schema_version: REALTIME_MATCH_NOTIFICATION_SCHEMA_VERSION,
            notification_id,
            topic,
            sequence,
            payload,
        }
    }

    pub fn notification_type(&self) -> &str {
        self.notification_type
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn notification_id(&self) -> Uuid {
        self.notification_id
    }

    pub const fn topic(&self) -> RealtimeTopic {
        self.topic
    }

    pub const fn sequence(&self) -> RealtimeSequence {
        self.sequence
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

/// A versioned notification for an immutable canonical correction event.
///
/// Corrections share the match topic and delivery sequence with live events,
/// but use a distinct wire type so clients cannot apply a void marker as if it
/// were a new C1 football event. `voids_event_id` identifies the historical
/// event whose effective-state contribution must be removed; the original
/// event remains immutable and queryable.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RealtimeMatchCorrectionNotification {
    #[serde(rename = "type")]
    notification_type: &'static str,
    schema_version: u32,
    notification_id: Uuid,
    topic: RealtimeTopic,
    sequence: RealtimeSequence,
    match_id: Uuid,
    correction_event_id: Uuid,
    voids_event_id: Uuid,
    reason: String,
}

impl RealtimeMatchCorrectionNotification {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        notification_id: Uuid,
        topic: RealtimeTopic,
        sequence: RealtimeSequence,
        match_id: Uuid,
        correction_event_id: Uuid,
        voids_event_id: Uuid,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            notification_type: REALTIME_MATCH_CORRECTION_NOTIFICATION_TYPE,
            schema_version: REALTIME_MATCH_CORRECTION_NOTIFICATION_SCHEMA_VERSION,
            notification_id,
            topic,
            sequence,
            match_id,
            correction_event_id,
            voids_event_id,
            reason: reason.into(),
        }
    }

    pub fn notification_type(&self) -> &str {
        self.notification_type
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn notification_id(&self) -> Uuid {
        self.notification_id
    }

    pub const fn topic(&self) -> RealtimeTopic {
        self.topic
    }

    pub const fn sequence(&self) -> RealtimeSequence {
        self.sequence
    }

    pub const fn match_id(&self) -> Uuid {
        self.match_id
    }

    pub const fn correction_event_id(&self) -> Uuid {
        self.correction_event_id
    }

    pub const fn voids_event_id(&self) -> Uuid {
        self.voids_event_id
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Canonical event kinds that change match verification state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeVerificationChangeType {
    MatchVerified,
    MatchReopened,
}

impl RealtimeVerificationChangeType {
    pub const fn canonical_event_type(self) -> &'static str {
        match self {
            Self::MatchVerified => "MatchVerified",
            Self::MatchReopened => "MatchReopened",
        }
    }

    pub fn from_canonical_event_type(value: &str) -> Option<Self> {
        match value {
            "MatchVerified" => Some(Self::MatchVerified),
            "MatchReopened" => Some(Self::MatchReopened),
            _ => None,
        }
    }
}

/// A versioned notification for a canonical match verification decision.
///
/// The decision kind is explicit so clients do not have to interpret a
/// generic live-event payload. The status is read from the canonical event
/// envelope, while `payload` remains the versioned event envelope for fields
/// owned by the verification boundary and future evidence metadata.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RealtimeMatchVerificationNotification {
    #[serde(rename = "type")]
    notification_type: &'static str,
    schema_version: u32,
    notification_id: Uuid,
    topic: RealtimeTopic,
    sequence: RealtimeSequence,
    match_id: Uuid,
    verification_event_id: Uuid,
    change_type: RealtimeVerificationChangeType,
    verification_status: MatchSnapshotVerificationStatus,
    payload: Value,
}

/// A versioned notification that tells standings consumers that the source
/// results revision for one season advanced.
///
/// This carries source freshness only. It does not claim that the standings
/// projection has already caught up; clients use the REST standings resource
/// to read the computed revision and table state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RealtimeStandingsRevisionNotification {
    #[serde(rename = "type")]
    notification_type: &'static str,
    schema_version: u32,
    notification_id: Uuid,
    topic: RealtimeTopic,
    sequence: RealtimeSequence,
    season_id: Uuid,
    source_results_revision: RealtimeStandingsResultsRevision,
}

impl RealtimeStandingsRevisionNotification {
    pub fn new(
        notification_id: Uuid,
        topic: RealtimeTopic,
        sequence: RealtimeSequence,
        season_id: Uuid,
        source_results_revision: RealtimeStandingsResultsRevision,
    ) -> Self {
        Self {
            notification_type: REALTIME_STANDINGS_REVISION_NOTIFICATION_TYPE,
            schema_version: REALTIME_STANDINGS_REVISION_NOTIFICATION_SCHEMA_VERSION,
            notification_id,
            topic,
            sequence,
            season_id,
            source_results_revision,
        }
    }

    pub fn notification_type(&self) -> &str {
        self.notification_type
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn notification_id(&self) -> Uuid {
        self.notification_id
    }

    pub const fn topic(&self) -> RealtimeTopic {
        self.topic
    }

    pub const fn sequence(&self) -> RealtimeSequence {
        self.sequence
    }

    pub const fn season_id(&self) -> Uuid {
        self.season_id
    }

    pub const fn source_results_revision(&self) -> RealtimeStandingsResultsRevision {
        self.source_results_revision
    }
}

impl RealtimeMatchVerificationNotification {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        notification_id: Uuid,
        topic: RealtimeTopic,
        sequence: RealtimeSequence,
        match_id: Uuid,
        verification_event_id: Uuid,
        change_type: RealtimeVerificationChangeType,
        verification_status: MatchSnapshotVerificationStatus,
        payload: Value,
    ) -> Self {
        Self {
            notification_type: REALTIME_MATCH_VERIFICATION_NOTIFICATION_TYPE,
            schema_version: REALTIME_MATCH_VERIFICATION_NOTIFICATION_SCHEMA_VERSION,
            notification_id,
            topic,
            sequence,
            match_id,
            verification_event_id,
            change_type,
            verification_status,
            payload,
        }
    }

    pub fn notification_type(&self) -> &str {
        self.notification_type
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn notification_id(&self) -> Uuid {
        self.notification_id
    }

    pub const fn topic(&self) -> RealtimeTopic {
        self.topic
    }

    pub const fn sequence(&self) -> RealtimeSequence {
        self.sequence
    }

    pub const fn match_id(&self) -> Uuid {
        self.match_id
    }

    pub const fn verification_event_id(&self) -> Uuid {
        self.verification_event_id
    }

    pub const fn change_type(&self) -> RealtimeVerificationChangeType {
        self.change_type
    }

    pub const fn verification_status(&self) -> MatchSnapshotVerificationStatus {
        self.verification_status
    }

    pub fn payload(&self) -> &Value {
        &self.payload
    }
}

/// Transport-ready notification union selected from the canonical outbox
/// event kind. The untagged representation keeps each notification's stable
/// top-level `type` as the wire discriminator.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum RealtimeNotification {
    MatchEvent(RealtimeMatchNotification),
    MatchCorrection(RealtimeMatchCorrectionNotification),
    MatchVerification(RealtimeMatchVerificationNotification),
    StandingsRevision(RealtimeStandingsRevisionNotification),
}

/// A WebSocket topic supported by realtime protocol major version 1.
///
/// Topic names identify a delivery scope only. They do not carry event
/// sequence, authorization, or payload data; those are separate contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RealtimeTopic {
    Match { match_id: Uuid },
    Standings { season_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeTopicParseError {
    UnsupportedTopicName { value: String },
    InvalidMatchId { value: String },
    NonCanonicalMatchId { value: String },
    InvalidSeasonId { value: String },
    NonCanonicalSeasonId { value: String },
}

impl fmt::Display for RealtimeTopicParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedTopicName { value } => {
                write!(formatter, "unsupported realtime topic name {value}")
            }
            Self::InvalidMatchId { value } => {
                write!(
                    formatter,
                    "realtime match topic has invalid match ID {value}"
                )
            }
            Self::NonCanonicalMatchId { value } => write!(
                formatter,
                "realtime match topic must use a lowercase hyphenated UUID: {value}"
            ),
            Self::InvalidSeasonId { value } => {
                write!(
                    formatter,
                    "realtime standings topic has invalid season ID {value}"
                )
            }
            Self::NonCanonicalSeasonId { value } => write!(
                formatter,
                "realtime standings topic must use a lowercase hyphenated UUID: {value}"
            ),
        }
    }
}

impl std::error::Error for RealtimeTopicParseError {}

impl RealtimeTopic {
    pub const fn match_topic(match_id: Uuid) -> Self {
        Self::Match { match_id }
    }

    pub const fn standings_topic(season_id: Uuid) -> Self {
        Self::Standings { season_id }
    }

    pub const fn match_id(self) -> Option<Uuid> {
        match self {
            Self::Match { match_id } => Some(match_id),
            Self::Standings { .. } => None,
        }
    }

    pub const fn season_id(self) -> Option<Uuid> {
        match self {
            Self::Match { .. } => None,
            Self::Standings { season_id } => Some(season_id),
        }
    }

    pub fn as_topic_name(self) -> String {
        self.to_string()
    }
}

impl fmt::Display for RealtimeTopic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Match { match_id } => {
                write!(formatter, "{REALTIME_MATCH_TOPIC_PREFIX}:{match_id}")
            }
            Self::Standings { season_id } => {
                write!(formatter, "{REALTIME_STANDINGS_TOPIC_PREFIX}:{season_id}")
            }
        }
    }
}

impl FromStr for RealtimeTopic {
    type Err = RealtimeTopicParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(match_id_value) = value.strip_prefix("match:") {
            let match_id = Uuid::parse_str(match_id_value).map_err(|_| {
                RealtimeTopicParseError::InvalidMatchId {
                    value: match_id_value.to_owned(),
                }
            })?;
            let topic = Self::match_topic(match_id);
            if topic.to_string() != value {
                return Err(RealtimeTopicParseError::NonCanonicalMatchId {
                    value: value.to_owned(),
                });
            }
            return Ok(topic);
        }
        if let Some(season_id_value) = value.strip_prefix("standings:") {
            let season_id = Uuid::parse_str(season_id_value).map_err(|_| {
                RealtimeTopicParseError::InvalidSeasonId {
                    value: season_id_value.to_owned(),
                }
            })?;
            let topic = Self::standings_topic(season_id);
            if topic.to_string() != value {
                return Err(RealtimeTopicParseError::NonCanonicalSeasonId {
                    value: value.to_owned(),
                });
            }
            return Ok(topic);
        }
        Err(RealtimeTopicParseError::UnsupportedTopicName {
            value: value.to_owned(),
        })
    }
}

impl Serialize for RealtimeTopic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RealtimeTopic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
    }
}

/// Client control frame for changing one connection's topic subscriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeSubscriptionRequest {
    Subscribe {
        request_id: Uuid,
        topic: RealtimeTopic,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_known_sequence: Option<RealtimeSequence>,
    },
    Unsubscribe {
        request_id: Uuid,
        topic: RealtimeTopic,
    },
}

/// Server response for a subscription change or reconnect recovery decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RealtimeSubscriptionResponse {
    Subscribed {
        request_id: Uuid,
        topic: RealtimeTopic,
    },
    Unsubscribed {
        request_id: Uuid,
        topic: RealtimeTopic,
    },
    SnapshotRequired {
        request_id: Uuid,
        topic: RealtimeTopic,
        reason: RealtimeSnapshotRequiredReason,
    },
}

/// Why a subscribe request cannot safely replay the client's cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeSnapshotRequiredReason {
    BeforeRetention,
    InsideRetention,
    ClientAheadOfNewest,
}

#[cfg(test)]
mod tests {
    use super::{
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
    use crate::match_snapshot::MatchSnapshotVerificationStatus;
    use uuid::Uuid;

    #[test]
    fn formats_the_stable_match_topic_name() {
        let match_id = Uuid::parse_str("019ff100-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(match_id);

        assert_eq!(REALTIME_PROTOCOL_MAJOR_VERSION, 1);
        assert_eq!(REALTIME_MATCH_TOPIC_PREFIX, "match");
        assert_eq!(topic.match_id(), Some(match_id));
        assert_eq!(topic.season_id(), None);
        assert_eq!(
            topic.as_topic_name(),
            "match:019ff100-3bc5-7cf0-b3d2-0728c077a930"
        );
        assert_eq!(
            topic.to_string(),
            "match:019ff100-3bc5-7cf0-b3d2-0728c077a930"
        );
    }

    #[test]
    fn formats_and_round_trips_the_stable_standings_topic_name() {
        let season_id = Uuid::parse_str("019ff105-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::standings_topic(season_id);

        assert_eq!(REALTIME_STANDINGS_TOPIC_PREFIX, "standings");
        assert_eq!(topic.match_id(), None);
        assert_eq!(topic.season_id(), Some(season_id));
        assert_eq!(
            topic.as_topic_name(),
            "standings:019ff105-3bc5-7cf0-b3d2-0728c077a930"
        );
        assert_eq!(
            "standings:019ff105-3bc5-7cf0-b3d2-0728c077a930"
                .parse::<RealtimeTopic>()
                .unwrap(),
            topic
        );
    }

    #[test]
    fn keeps_realtime_sequences_positive() {
        assert_eq!(RealtimeSequence::first().value(), 1);
        assert_eq!(RealtimeSequence::try_new(0), None);
        assert_eq!(RealtimeSequence::try_new(7).unwrap().value(), 7);
    }

    #[test]
    fn serializes_subscription_control_frames_with_stable_topic_names() {
        let request_id = Uuid::parse_str("019ff110-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(
            Uuid::parse_str("019ff111-3bc5-7cf0-b3d2-0728c077a930").unwrap(),
        );
        let subscribe = RealtimeSubscriptionRequest::Subscribe {
            request_id,
            topic,
            last_known_sequence: None,
        };
        let encoded = serde_json::to_string(&subscribe).unwrap();
        assert_eq!(
            encoded,
            r#"{"type":"subscribe","request_id":"019ff110-3bc5-7cf0-b3d2-0728c077a930","topic":"match:019ff111-3bc5-7cf0-b3d2-0728c077a930"}"#
        );
        let decoded: RealtimeSubscriptionRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, subscribe);

        let unsubscribe = RealtimeSubscriptionResponse::Unsubscribed { request_id, topic };
        let decoded: RealtimeSubscriptionResponse =
            serde_json::from_value(serde_json::to_value(unsubscribe).unwrap()).unwrap();
        assert_eq!(decoded, unsubscribe);
    }

    #[test]
    fn serializes_standings_topics_in_subscription_control_frames() {
        let request_id = Uuid::parse_str("019ff115-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::standings_topic(
            Uuid::parse_str("019ff116-3bc5-7cf0-b3d2-0728c077a930").unwrap(),
        );
        let request = RealtimeSubscriptionRequest::Subscribe {
            request_id,
            topic,
            last_known_sequence: Some(RealtimeSequence::try_new(9).unwrap()),
        };

        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::json!({
                "type": "subscribe",
                "request_id": request_id,
                "topic": "standings:019ff116-3bc5-7cf0-b3d2-0728c077a930",
                "last_known_sequence": 9
            })
        );
    }

    #[test]
    fn serializes_and_round_trips_snapshot_required_response() {
        let request_id = Uuid::parse_str("019ff150-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(
            Uuid::parse_str("019ff151-3bc5-7cf0-b3d2-0728c077a930").unwrap(),
        );
        let response = RealtimeSubscriptionResponse::SnapshotRequired {
            request_id,
            topic,
            reason: RealtimeSnapshotRequiredReason::BeforeRetention,
        };

        let encoded = serde_json::to_value(response).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "type": "snapshot_required",
                "request_id": request_id,
                "topic": topic.to_string(),
                "reason": "before_retention"
            })
        );
        assert_eq!(
            serde_json::from_value::<RealtimeSubscriptionResponse>(encoded).unwrap(),
            response
        );
    }

    #[test]
    fn keeps_snapshot_required_reason_names_stable() {
        let cases = [
            (
                RealtimeSnapshotRequiredReason::BeforeRetention,
                "before_retention",
            ),
            (
                RealtimeSnapshotRequiredReason::InsideRetention,
                "inside_retention",
            ),
            (
                RealtimeSnapshotRequiredReason::ClientAheadOfNewest,
                "client_ahead_of_newest",
            ),
        ];

        for (reason, encoded_name) in cases {
            assert_eq!(
                serde_json::to_value(reason).unwrap(),
                serde_json::json!(encoded_name)
            );
        }
    }

    #[test]
    fn accepts_a_positive_last_known_sequence_without_breaking_old_subscribe_json() {
        let request_id = Uuid::parse_str("019ff130-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(
            Uuid::parse_str("019ff131-3bc5-7cf0-b3d2-0728c077a930").unwrap(),
        );
        let request = RealtimeSubscriptionRequest::Subscribe {
            request_id,
            topic,
            last_known_sequence: Some(RealtimeSequence::try_new(7).unwrap()),
        };
        let encoded = serde_json::to_string(&request).unwrap();
        assert_eq!(
            encoded,
            r#"{"type":"subscribe","request_id":"019ff130-3bc5-7cf0-b3d2-0728c077a930","topic":"match:019ff131-3bc5-7cf0-b3d2-0728c077a930","last_known_sequence":7}"#
        );
        assert_eq!(
            serde_json::from_str::<RealtimeSubscriptionRequest>(&encoded).unwrap(),
            request
        );

        let old_json = r#"{"type":"subscribe","request_id":"019ff130-3bc5-7cf0-b3d2-0728c077a930","topic":"match:019ff131-3bc5-7cf0-b3d2-0728c077a930"}"#;
        assert_eq!(
            serde_json::from_str::<RealtimeSubscriptionRequest>(old_json).unwrap(),
            RealtimeSubscriptionRequest::Subscribe {
                request_id,
                topic,
                last_known_sequence: None,
            }
        );
    }

    #[test]
    fn rejects_zero_last_known_sequence() {
        let json = r#"{"type":"subscribe","request_id":"019ff140-3bc5-7cf0-b3d2-0728c077a930","topic":"match:019ff141-3bc5-7cf0-b3d2-0728c077a930","last_known_sequence":0}"#;
        assert!(serde_json::from_str::<RealtimeSubscriptionRequest>(json).is_err());
    }

    #[test]
    fn rejects_unsupported_or_noncanonical_topic_names() {
        assert!(matches!(
            "competition:019ff111-3bc5-7cf0-b3d2-0728c077a930".parse::<RealtimeTopic>(),
            Err(RealtimeTopicParseError::UnsupportedTopicName { .. })
        ));
        assert!(matches!(
            "match:019FF111-3bc5-7cf0-b3d2-0728c077a930".parse::<RealtimeTopic>(),
            Err(RealtimeTopicParseError::NonCanonicalMatchId { .. })
        ));
        assert!(matches!(
            "standings:019FF111-3bc5-7cf0-b3d2-0728c077a930".parse::<RealtimeTopic>(),
            Err(RealtimeTopicParseError::NonCanonicalSeasonId { .. })
        ));
        assert!(matches!(
            "standings:not-a-uuid".parse::<RealtimeTopic>(),
            Err(RealtimeTopicParseError::InvalidSeasonId { .. })
        ));
    }

    #[test]
    fn serializes_a_versioned_match_notification_with_topic_sequence() {
        let notification_id = Uuid::parse_str("019ff120-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(
            Uuid::parse_str("019ff121-3bc5-7cf0-b3d2-0728c077a930").unwrap(),
        );
        let notification = RealtimeMatchNotification::new(
            notification_id,
            topic,
            RealtimeSequence::try_new(4).unwrap(),
            serde_json::json!({"event_type": "GoalRecorded"}),
        );
        let encoded = serde_json::to_value(&notification).unwrap();

        assert_eq!(encoded["type"], REALTIME_MATCH_NOTIFICATION_TYPE);
        assert_eq!(
            encoded["schema_version"],
            REALTIME_MATCH_NOTIFICATION_SCHEMA_VERSION
        );
        assert_eq!(encoded["notification_id"], notification_id.to_string());
        assert_eq!(encoded["topic"], topic.to_string());
        assert_eq!(encoded["sequence"], 4);
        assert_eq!(encoded["payload"]["event_type"], "GoalRecorded");
        assert_eq!(notification.notification_id(), notification_id);
        assert_eq!(notification.topic(), topic);
        assert_eq!(notification.sequence().value(), 4);
    }

    #[test]
    fn serializes_corrections_as_a_distinct_typed_notification() {
        let notification_id = Uuid::parse_str("019ff160-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let match_id = Uuid::parse_str("019ff161-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let correction_event_id = Uuid::parse_str("019ff162-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let voids_event_id = Uuid::parse_str("019ff163-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(match_id);
        let notification = RealtimeMatchCorrectionNotification::new(
            notification_id,
            topic,
            RealtimeSequence::try_new(5).unwrap(),
            match_id,
            correction_event_id,
            voids_event_id,
            "official protocol removed the goal",
        );

        let encoded = serde_json::to_value(&notification).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "type": REALTIME_MATCH_CORRECTION_NOTIFICATION_TYPE,
                "schema_version": REALTIME_MATCH_CORRECTION_NOTIFICATION_SCHEMA_VERSION,
                "notification_id": notification_id,
                "topic": topic.to_string(),
                "sequence": 5,
                "match_id": match_id,
                "correction_event_id": correction_event_id,
                "voids_event_id": voids_event_id,
                "reason": "official protocol removed the goal"
            })
        );
        assert_eq!(notification.notification_type(), "match.correction");
        assert_eq!(notification.schema_version(), 1);
        assert_eq!(notification.notification_id(), notification_id);
        assert_eq!(notification.topic(), topic);
        assert_eq!(notification.sequence().value(), 5);
        assert_eq!(notification.match_id(), match_id);
        assert_eq!(notification.correction_event_id(), correction_event_id);
        assert_eq!(notification.voids_event_id(), voids_event_id);
        assert_eq!(notification.reason(), "official protocol removed the goal");

        let union = RealtimeNotification::MatchCorrection(notification);
        assert_eq!(serde_json::to_value(union).unwrap(), encoded);
    }

    #[test]
    fn serializes_verification_changes_with_explicit_kind_and_status() {
        let notification_id = Uuid::parse_str("019ff170-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let match_id = Uuid::parse_str("019ff171-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::match_topic(match_id);
        let notification = RealtimeMatchVerificationNotification::new(
            notification_id,
            topic,
            RealtimeSequence::try_new(6).unwrap(),
            match_id,
            notification_id,
            RealtimeVerificationChangeType::MatchVerified,
            MatchSnapshotVerificationStatus::Verified,
            serde_json::json!({
                "event_type": "MatchVerified",
                "verification_status": "verified"
            }),
        );

        let encoded = serde_json::to_value(&notification).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "type": REALTIME_MATCH_VERIFICATION_NOTIFICATION_TYPE,
                "schema_version": REALTIME_MATCH_VERIFICATION_NOTIFICATION_SCHEMA_VERSION,
                "notification_id": notification_id,
                "topic": topic.to_string(),
                "sequence": 6,
                "match_id": match_id,
                "verification_event_id": notification_id,
                "change_type": "match_verified",
                "verification_status": "verified",
                "payload": {
                    "event_type": "MatchVerified",
                    "verification_status": "verified"
                }
            })
        );
        assert_eq!(
            RealtimeVerificationChangeType::MatchVerified.canonical_event_type(),
            "MatchVerified"
        );
        assert_eq!(
            RealtimeVerificationChangeType::from_canonical_event_type("MatchReopened"),
            Some(RealtimeVerificationChangeType::MatchReopened)
        );

        let union = RealtimeNotification::MatchVerification(notification);
        assert_eq!(serde_json::to_value(union).unwrap(), encoded);
    }

    #[test]
    fn serializes_standings_source_revision_changes_as_a_distinct_notification() {
        let notification_id = Uuid::parse_str("019ff180-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let season_id = Uuid::parse_str("019ff181-3bc5-7cf0-b3d2-0728c077a930").unwrap();
        let topic = RealtimeTopic::standings_topic(season_id);
        let source_results_revision = RealtimeStandingsResultsRevision::try_new(57).unwrap();
        let notification = RealtimeStandingsRevisionNotification::new(
            notification_id,
            topic,
            RealtimeSequence::try_new(7).unwrap(),
            season_id,
            source_results_revision,
        );

        let encoded = serde_json::to_value(&notification).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({
                "type": REALTIME_STANDINGS_REVISION_NOTIFICATION_TYPE,
                "schema_version": REALTIME_STANDINGS_REVISION_NOTIFICATION_SCHEMA_VERSION,
                "notification_id": notification_id,
                "topic": "standings:019ff181-3bc5-7cf0-b3d2-0728c077a930",
                "sequence": 7,
                "season_id": season_id,
                "source_results_revision": 57
            })
        );
        assert_eq!(notification.notification_type(), "standings.revision");
        assert_eq!(notification.schema_version(), 1);
        assert_eq!(notification.notification_id(), notification_id);
        assert_eq!(notification.topic(), topic);
        assert_eq!(notification.sequence().value(), 7);
        assert_eq!(notification.season_id(), season_id);
        assert_eq!(notification.source_results_revision().value(), 57);

        let union = RealtimeNotification::StandingsRevision(notification);
        assert_eq!(serde_json::to_value(union).unwrap(), encoded);
    }

    #[test]
    fn standings_source_results_revision_must_be_positive() {
        assert_eq!(RealtimeStandingsResultsRevision::try_new(0), None);
        assert_eq!(
            RealtimeStandingsResultsRevision::try_new(57)
                .unwrap()
                .value(),
            57
        );
        assert!(serde_json::from_str::<RealtimeStandingsResultsRevision>("0").is_err());
    }
}
