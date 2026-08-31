use crate::{MatchSnapshotMeta, MatchSnapshotResponse, MatchSnapshotTimelineEntry};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchTimelineData {
    pub match_id: Uuid,
    pub aggregate_version: u64,
    pub entries: Vec<MatchSnapshotTimelineEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchTimelineResponse {
    pub data: MatchTimelineData,
    pub meta: MatchSnapshotMeta,
}

impl From<&MatchSnapshotResponse> for MatchTimelineResponse {
    fn from(snapshot: &MatchSnapshotResponse) -> Self {
        Self {
            data: MatchTimelineData {
                match_id: snapshot.data.match_id,
                aggregate_version: snapshot.data.aggregate_version,
                entries: snapshot.data.timeline.clone(),
            },
            meta: snapshot.meta.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MatchTimelineResponse;
    use crate::{
        MatchSnapshotData, MatchSnapshotResponse, MatchSnapshotScore, MatchSnapshotTimelineEntry,
        MatchSnapshotVerificationStatus,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("timeline contract UUID should be valid")
    }

    #[test]
    fn derives_timeline_without_copying_score_or_team_fields() {
        let snapshot = MatchSnapshotResponse::new(
            MatchSnapshotData::new(
                uuid("019ff420-3bc5-7cf0-b3d2-0728c077a930"),
                uuid("019ff421-3bc5-7cf0-b3d2-0728c077a930"),
                uuid("019ff422-3bc5-7cf0-b3d2-0728c077a930"),
                MatchSnapshotScore {
                    home_goals: 1,
                    away_goals: 0,
                },
                1,
                vec![MatchSnapshotTimelineEntry {
                    event_id: uuid("019ff423-3bc5-7cf0-b3d2-0728c077a930"),
                    aggregate_version: 1,
                    event_index: 0,
                    event_type: "GoalRecorded".to_owned(),
                    period: "first_half".to_owned(),
                    clock_ms: 10_000,
                }],
            ),
            "2026-08-31T12:00:00Z",
            MatchSnapshotVerificationStatus::Provisional,
            vec![],
        )
        .expect("snapshot fixture should be valid");

        let timeline = MatchTimelineResponse::from(&snapshot);
        assert_eq!(timeline.data.match_id, snapshot.data.match_id);
        assert_eq!(timeline.data.aggregate_version, 1);
        assert_eq!(timeline.data.entries, snapshot.data.timeline);
        assert_eq!(timeline.meta, snapshot.meta);
    }
}
