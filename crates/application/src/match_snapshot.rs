use football_contracts::{
    MatchSnapshotData, MatchSnapshotMetaError, MatchSnapshotResponse, MatchSnapshotScore,
    MatchSnapshotTimelineEntry, MatchSnapshotVerificationStatus,
};
use football_domain::MatchProjection;

/// Maps the canonical synchronous projection to the REST snapshot contract.
///
/// The projection is the only source for score, timeline, teams, and aggregate
/// version. Verification metadata is supplied explicitly by the query adapter;
/// this function never guesses it from missing match-level state.
pub fn build_canonical_match_snapshot(
    projection: &MatchProjection,
    generated_at: impl Into<String>,
    verification_status: MatchSnapshotVerificationStatus,
    quality_flags: Vec<String>,
) -> Result<MatchSnapshotResponse, MatchSnapshotMetaError> {
    let score = projection.score();
    let timeline = projection
        .timeline()
        .iter()
        .map(|entry| MatchSnapshotTimelineEntry {
            event_id: entry.event_id().as_uuid(),
            aggregate_version: entry.aggregate_version().value(),
            event_index: entry.event_index().value(),
            event_type: entry.event_type().as_str().to_owned(),
            period: entry.period().as_persisted_str().to_owned(),
            clock_ms: entry.clock_ms().value(),
        })
        .collect();
    let data = MatchSnapshotData::new(
        projection.match_id().as_uuid(),
        projection.home_team_id().as_uuid(),
        projection.away_team_id().as_uuid(),
        MatchSnapshotScore {
            home_goals: score.home_goals(),
            away_goals: score.away_goals(),
        },
        projection.current_aggregate_version().value(),
        timeline,
    );

    MatchSnapshotResponse::new(data, generated_at, verification_status, quality_flags)
}

#[cfg(test)]
mod tests {
    use super::build_canonical_match_snapshot;
    use football_contracts::MatchSnapshotVerificationStatus;
    use football_domain::{MatchId, MatchProjection, TeamId};
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("snapshot builder UUID should be valid")
    }

    #[test]
    fn maps_an_empty_canonical_projection_without_inventing_state() {
        let match_id = MatchId::from_uuid(uuid("019ff210-3bc5-7cf0-b3d2-0728c077a930"));
        let home_team_id = TeamId::from_uuid(uuid("019ff211-3bc5-7cf0-b3d2-0728c077a930"));
        let away_team_id = TeamId::from_uuid(uuid("019ff212-3bc5-7cf0-b3d2-0728c077a930"));
        let projection = MatchProjection::new(match_id, home_team_id, away_team_id)
            .expect("snapshot fixture projection should be valid");

        let response = build_canonical_match_snapshot(
            &projection,
            "2026-08-09T12:00:00Z",
            MatchSnapshotVerificationStatus::Provisional,
            vec![],
        )
        .expect("empty canonical projection should map to a snapshot");

        assert_eq!(response.data.match_id, match_id.as_uuid());
        assert_eq!(response.data.home_team_id, home_team_id.as_uuid());
        assert_eq!(response.data.away_team_id, away_team_id.as_uuid());
        assert_eq!(response.data.score.home_goals, 0);
        assert_eq!(response.data.score.away_goals, 0);
        assert_eq!(response.data.aggregate_version, 0);
        assert!(response.data.timeline.is_empty());
        assert_eq!(
            response.meta.verification_status,
            MatchSnapshotVerificationStatus::Provisional
        );
    }
}
