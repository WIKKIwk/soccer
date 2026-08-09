use serde::Serialize;
use std::fmt;
use uuid::Uuid;

/// Version of the canonical match snapshot response contract.
pub const MATCH_SNAPSHOT_RESPONSE_VERSION: u16 = 1;

/// Public REST data reconstructed from the canonical synchronous match
/// projection. It deliberately contains no persistence-row fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchSnapshotData {
    pub match_id: Uuid,
    pub home_team_id: Uuid,
    pub away_team_id: Uuid,
    pub score: MatchSnapshotScore,
    pub aggregate_version: u64,
    pub timeline: Vec<MatchSnapshotTimelineEntry>,
}

impl MatchSnapshotData {
    pub fn new(
        match_id: Uuid,
        home_team_id: Uuid,
        away_team_id: Uuid,
        score: MatchSnapshotScore,
        aggregate_version: u64,
        timeline: Vec<MatchSnapshotTimelineEntry>,
    ) -> Self {
        Self {
            match_id,
            home_team_id,
            away_team_id,
            score,
            aggregate_version,
            timeline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MatchSnapshotScore {
    pub home_goals: u32,
    pub away_goals: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchSnapshotTimelineEntry {
    pub event_id: Uuid,
    pub aggregate_version: u64,
    pub event_index: u32,
    pub event_type: String,
    pub period: String,
    pub clock_ms: u64,
}

/// Verification state supplied by the canonical query adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchSnapshotVerificationStatus {
    Provisional,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchSnapshotMeta {
    pub version: u16,
    pub generated_at: String,
    pub verification_status: MatchSnapshotVerificationStatus,
    pub quality_flags: Vec<String>,
}

impl MatchSnapshotMeta {
    pub fn new(
        generated_at: impl Into<String>,
        verification_status: MatchSnapshotVerificationStatus,
        quality_flags: Vec<String>,
    ) -> Result<Self, MatchSnapshotMetaError> {
        let generated_at = generated_at.into();
        if generated_at.trim().is_empty() {
            return Err(MatchSnapshotMetaError::GeneratedAtBlank);
        }
        if quality_flags
            .iter()
            .any(|quality_flag| quality_flag.trim().is_empty())
        {
            return Err(MatchSnapshotMetaError::QualityFlagBlank);
        }

        Ok(Self {
            version: MATCH_SNAPSHOT_RESPONSE_VERSION,
            generated_at,
            verification_status,
            quality_flags,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchSnapshotMetaError {
    GeneratedAtBlank,
    QualityFlagBlank,
}

impl fmt::Display for MatchSnapshotMetaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GeneratedAtBlank => {
                formatter.write_str("snapshot generated_at must not be blank")
            }
            Self::QualityFlagBlank => {
                formatter.write_str("snapshot quality flags must not be blank")
            }
        }
    }
}

impl std::error::Error for MatchSnapshotMetaError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchSnapshotResponse {
    pub data: MatchSnapshotData,
    pub meta: MatchSnapshotMeta,
}

impl MatchSnapshotResponse {
    pub fn new(
        data: MatchSnapshotData,
        generated_at: impl Into<String>,
        verification_status: MatchSnapshotVerificationStatus,
        quality_flags: Vec<String>,
    ) -> Result<Self, MatchSnapshotMetaError> {
        Ok(Self {
            data,
            meta: MatchSnapshotMeta::new(generated_at, verification_status, quality_flags)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MATCH_SNAPSHOT_RESPONSE_VERSION, MatchSnapshotData, MatchSnapshotMeta,
        MatchSnapshotMetaError, MatchSnapshotResponse, MatchSnapshotScore,
        MatchSnapshotTimelineEntry, MatchSnapshotVerificationStatus,
    };
    use serde_json::json;
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("snapshot contract UUID should be valid")
    }

    #[test]
    fn serializes_the_canonical_snapshot_envelope() {
        let data = MatchSnapshotData::new(
            uuid("019ff200-3bc5-7cf0-b3d2-0728c077a930"),
            uuid("019ff201-3bc5-7cf0-b3d2-0728c077a930"),
            uuid("019ff202-3bc5-7cf0-b3d2-0728c077a930"),
            MatchSnapshotScore {
                home_goals: 1,
                away_goals: 0,
            },
            4,
            vec![MatchSnapshotTimelineEntry {
                event_id: uuid("019ff203-3bc5-7cf0-b3d2-0728c077a930"),
                aggregate_version: 4,
                event_index: 0,
                event_type: "GoalRecorded".to_owned(),
                period: "first_half".to_owned(),
                clock_ms: 12_000,
            }],
        );
        let response = MatchSnapshotResponse::new(
            data,
            "2026-08-09T12:00:00Z",
            MatchSnapshotVerificationStatus::Provisional,
            vec!["projection.current".to_owned()],
        )
        .expect("snapshot response metadata should be valid");

        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({
                "data": {
                    "match_id": "019ff200-3bc5-7cf0-b3d2-0728c077a930",
                    "home_team_id": "019ff201-3bc5-7cf0-b3d2-0728c077a930",
                    "away_team_id": "019ff202-3bc5-7cf0-b3d2-0728c077a930",
                    "score": {"home_goals": 1, "away_goals": 0},
                    "aggregate_version": 4,
                    "timeline": [{
                        "event_id": "019ff203-3bc5-7cf0-b3d2-0728c077a930",
                        "aggregate_version": 4,
                        "event_index": 0,
                        "event_type": "GoalRecorded",
                        "period": "first_half",
                        "clock_ms": 12000
                    }]
                },
                "meta": {
                    "version": MATCH_SNAPSHOT_RESPONSE_VERSION,
                    "generated_at": "2026-08-09T12:00:00Z",
                    "verification_status": "provisional",
                    "quality_flags": ["projection.current"]
                }
            })
        );
    }

    #[test]
    fn rejects_blank_snapshot_metadata() {
        assert_eq!(
            MatchSnapshotMeta::new("  ", MatchSnapshotVerificationStatus::Provisional, vec![]),
            Err(MatchSnapshotMetaError::GeneratedAtBlank)
        );
        assert_eq!(
            MatchSnapshotMeta::new(
                "2026-08-09T12:00:00Z",
                MatchSnapshotVerificationStatus::Verified,
                vec!["".to_owned()]
            ),
            Err(MatchSnapshotMetaError::QualityFlagBlank)
        );
    }
}
