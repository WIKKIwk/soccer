use crate::{
    FixtureRepositoryError, MatchEventRepositoryError, OperatorAssignmentRepositoryError,
    load_canonical_match_event_stream, load_fixture, load_match_assignment,
};
use football_domain::{
    EffectiveEventStream, EffectiveEventStreamError, MatchEventPayload, MatchId, MatchProjection,
    MatchProjectionError, MatchVerificationStatus, VerificationEventPayload,
};
use sqlx::PgPool;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalMatchQuery {
    projection: MatchProjection,
    verification_status: MatchVerificationStatus,
}

impl CanonicalMatchQuery {
    pub const fn new(
        projection: MatchProjection,
        verification_status: MatchVerificationStatus,
    ) -> Self {
        Self {
            projection,
            verification_status,
        }
    }

    pub const fn projection(&self) -> &MatchProjection {
        &self.projection
    }

    pub const fn verification_status(&self) -> MatchVerificationStatus {
        self.verification_status
    }
}

#[derive(Debug)]
pub enum CanonicalMatchQueryError {
    Assignment(OperatorAssignmentRepositoryError),
    Fixture(FixtureRepositoryError),
    AssignedFixtureMissing { match_id: MatchId },
    Events(MatchEventRepositoryError),
    EffectiveStream(EffectiveEventStreamError),
    Projection(MatchProjectionError),
}

impl fmt::Display for CanonicalMatchQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Assignment(error) => write!(formatter, "match assignment query failed: {error}"),
            Self::Fixture(error) => write!(formatter, "match fixture query failed: {error}"),
            Self::AssignedFixtureMissing { match_id } => {
                write!(formatter, "match {match_id} references a missing fixture")
            }
            Self::Events(error) => write!(formatter, "canonical event query failed: {error}"),
            Self::EffectiveStream(error) => {
                write!(formatter, "effective event stream query failed: {error}")
            }
            Self::Projection(error) => write!(formatter, "match projection query failed: {error}"),
        }
    }
}

impl std::error::Error for CanonicalMatchQueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Assignment(error) => Some(error),
            Self::Fixture(error) => Some(error),
            Self::Events(error) => Some(error),
            Self::EffectiveStream(error) => Some(error),
            Self::Projection(error) => Some(error),
            Self::AssignedFixtureMissing { .. } => None,
        }
    }
}

pub async fn load_canonical_match_query(
    pool: &PgPool,
    match_id: MatchId,
) -> Result<Option<CanonicalMatchQuery>, CanonicalMatchQueryError> {
    let Some(assignment) = load_match_assignment(pool, match_id)
        .await
        .map_err(CanonicalMatchQueryError::Assignment)?
    else {
        return Ok(None);
    };
    let fixture = load_fixture(pool, assignment.fixture_id())
        .await
        .map_err(CanonicalMatchQueryError::Fixture)?
        .ok_or(CanonicalMatchQueryError::AssignedFixtureMissing { match_id })?;
    let canonical = load_canonical_match_event_stream(pool, match_id)
        .await
        .map_err(CanonicalMatchQueryError::Events)?;
    let effective = EffectiveEventStream::try_from_canonical(&canonical)
        .map_err(CanonicalMatchQueryError::EffectiveStream)?;
    let verification_status = effective_verification_status(&effective);
    let projection = MatchProjection::from_effective_stream(
        &effective,
        fixture.home_team_id(),
        fixture.away_team_id(),
    )
    .map_err(CanonicalMatchQueryError::Projection)?;

    Ok(Some(CanonicalMatchQuery::new(
        projection,
        verification_status,
    )))
}

fn effective_verification_status(
    stream: &EffectiveEventStream<MatchEventPayload>,
) -> MatchVerificationStatus {
    stream
        .events()
        .iter()
        .rev()
        .find_map(|event| match event.payload() {
            MatchEventPayload::Verification(VerificationEventPayload::MatchVerified(_)) => {
                Some(MatchVerificationStatus::Verified)
            }
            MatchEventPayload::Verification(VerificationEventPayload::MatchReopened(_)) => {
                Some(MatchVerificationStatus::Provisional)
            }
            MatchEventPayload::Lifecycle(_)
            | MatchEventPayload::C1(_)
            | MatchEventPayload::Correction(_) => None,
        })
        .unwrap_or_else(MatchVerificationStatus::initial)
}

#[cfg(test)]
mod tests {
    use super::{CanonicalMatchQuery, effective_verification_status};
    use football_domain::{
        CanonicalEventStream, EffectiveEventStream, MatchEventPayload, MatchId, MatchProjection,
        MatchVerificationStatus, TeamId,
    };
    use uuid::Uuid;

    fn uuid(value: &str) -> Uuid {
        Uuid::parse_str(value).expect("canonical query fixture UUID should be valid")
    }

    #[test]
    fn empty_stream_is_an_explicit_provisional_zero_projection() {
        let match_id = MatchId::from_uuid(uuid("019ff410-3bc5-7cf0-b3d2-0728c077a930"));
        let stream = CanonicalEventStream::<MatchEventPayload>::try_from_ordered(match_id, vec![])
            .expect("empty canonical stream should be valid");
        let effective = EffectiveEventStream::try_from_canonical(&stream)
            .expect("empty effective stream should be valid");
        let projection = MatchProjection::from_effective_stream(
            &effective,
            TeamId::from_uuid(uuid("019ff411-3bc5-7cf0-b3d2-0728c077a930")),
            TeamId::from_uuid(uuid("019ff412-3bc5-7cf0-b3d2-0728c077a930")),
        )
        .expect("empty match projection should be valid");
        let query = CanonicalMatchQuery::new(projection, effective_verification_status(&effective));

        assert_eq!(
            query.verification_status(),
            MatchVerificationStatus::Provisional
        );
        assert_eq!(query.projection().score().home_goals(), 0);
        assert!(query.projection().timeline().is_empty());
    }
}
