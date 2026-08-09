use football_contracts::{
    RealtimeSequence, RealtimeStandingsResultsRevision, RealtimeStandingsRevisionNotification,
    RealtimeTopic,
};
use std::fmt;
use uuid::Uuid;

/// Errors raised while assembling a standings revision change notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtimeStandingsRevisionBuildError {
    SourceResultsRevisionMustBePositive { value: u64 },
}

impl fmt::Display for RealtimeStandingsRevisionBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceResultsRevisionMustBePositive { value } => write!(
                formatter,
                "standings source results revision must be positive, got {value}"
            ),
        }
    }
}

impl std::error::Error for RealtimeStandingsRevisionBuildError {}

/// Builds the transport contract for one committed season source-revision
/// change. The topic is derived from the season ID so the duplicated
/// `season_id` field and topic scope cannot diverge at this boundary.
pub fn build_realtime_standings_revision_notification(
    notification_id: Uuid,
    season_id: Uuid,
    sequence: RealtimeSequence,
    source_results_revision: u64,
) -> Result<RealtimeStandingsRevisionNotification, RealtimeStandingsRevisionBuildError> {
    let source_results_revision =
        RealtimeStandingsResultsRevision::try_new(source_results_revision).ok_or(
            RealtimeStandingsRevisionBuildError::SourceResultsRevisionMustBePositive {
                value: source_results_revision,
            },
        )?;

    Ok(RealtimeStandingsRevisionNotification::new(
        notification_id,
        RealtimeTopic::standings_topic(season_id),
        sequence,
        season_id,
        source_results_revision,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        RealtimeStandingsRevisionBuildError, build_realtime_standings_revision_notification,
    };
    use football_contracts::{
        REALTIME_STANDINGS_REVISION_NOTIFICATION_TYPE, RealtimeNotification, RealtimeSequence,
        RealtimeTopic,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("standings realtime fixture UUID should be valid")
    }

    #[test]
    fn builds_a_standings_revision_notification_with_a_derived_topic() {
        let notification = build_realtime_standings_revision_notification(
            uuid("019ff190-3bc5-7cf0-b3d2-0728c077a930"),
            uuid("019ff191-3bc5-7cf0-b3d2-0728c077a930"),
            RealtimeSequence::try_new(8).unwrap(),
            57,
        )
        .expect("positive source revision should build");

        assert_eq!(
            notification.topic(),
            RealtimeTopic::standings_topic(uuid("019ff191-3bc5-7cf0-b3d2-0728c077a930"))
        );
        assert_eq!(
            notification.season_id(),
            uuid("019ff191-3bc5-7cf0-b3d2-0728c077a930")
        );
        assert_eq!(notification.source_results_revision().value(), 57);
        assert_eq!(notification.sequence().value(), 8);

        let encoded = serde_json::to_value(&notification).unwrap();
        assert_eq!(
            encoded["type"],
            REALTIME_STANDINGS_REVISION_NOTIFICATION_TYPE
        );
        assert_eq!(encoded["source_results_revision"], 57);
        assert_eq!(
            encoded["topic"],
            "standings:019ff191-3bc5-7cf0-b3d2-0728c077a930"
        );

        let union = RealtimeNotification::StandingsRevision(notification);
        assert_eq!(
            serde_json::to_value(union).unwrap(),
            json!({
                "type": "standings.revision",
                "schema_version": 1,
                "notification_id": "019ff190-3bc5-7cf0-b3d2-0728c077a930",
                "topic": "standings:019ff191-3bc5-7cf0-b3d2-0728c077a930",
                "sequence": 8,
                "season_id": "019ff191-3bc5-7cf0-b3d2-0728c077a930",
                "source_results_revision": 57
            })
        );
    }

    #[test]
    fn rejects_the_initial_zero_revision_as_a_change() {
        assert_eq!(
            build_realtime_standings_revision_notification(
                uuid("019ff192-3bc5-7cf0-b3d2-0728c077a930"),
                uuid("019ff193-3bc5-7cf0-b3d2-0728c077a930"),
                RealtimeSequence::first(),
                0,
            ),
            Err(
                RealtimeStandingsRevisionBuildError::SourceResultsRevisionMustBePositive {
                    value: 0
                }
            )
        );
    }
}
