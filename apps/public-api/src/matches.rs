use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use football_application::build_canonical_match_snapshot;
use football_contracts::{
    ApiErrorBody, ApiErrorMeta, ApiErrorResponse, ErrorDetails, MatchSnapshotVerificationStatus,
    MatchTimelineResponse,
};
use football_domain::{MatchId, MatchVerificationStatus};
use football_infrastructure::{DatabasePool, load_canonical_match_query};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::{Uuid, Version};

pub const MATCH_PATH: &str = "/v1/matches/{match_id}";
pub const MATCH_TIMELINE_PATH: &str = "/v1/matches/{match_id}/timeline";

pub async fn detail(
    State(database): State<DatabasePool>,
    Path(match_id): Path<String>,
) -> Response {
    match snapshot(&database, &match_id).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(response) => response,
    }
}

pub async fn timeline(
    State(database): State<DatabasePool>,
    Path(match_id): Path<String>,
) -> Response {
    match snapshot(&database, &match_id).await {
        Ok(snapshot) => Json(MatchTimelineResponse::from(&snapshot)).into_response(),
        Err(response) => response,
    }
}

async fn snapshot(
    database: &DatabasePool,
    match_id: &str,
) -> Result<football_contracts::MatchSnapshotResponse, Response> {
    let match_id = parse_match_id(match_id).map_err(|()| {
        api_error(
            StatusCode::BAD_REQUEST,
            "validation.invalid_match_id",
            "validation",
            "match_id must be a UUIDv7",
            false,
        )
    })?;
    let query = load_canonical_match_query(database, match_id)
        .await
        .map_err(|error| {
            tracing::error!(%match_id, error = %error, "canonical match query failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal.unexpected",
                "internal",
                "the canonical match snapshot could not be loaded",
                true,
            )
        })?
        .ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "resource.not_found",
                "resource",
                "the requested match does not exist",
                false,
            )
        })?;
    let generated_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| {
            tracing::error!(%match_id, error = %error, "snapshot timestamp formatting failed");
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal.unexpected",
                "internal",
                "the canonical match snapshot could not be generated",
                false,
            )
        })?;
    let verification_status = match query.verification_status() {
        MatchVerificationStatus::Provisional => MatchSnapshotVerificationStatus::Provisional,
        MatchVerificationStatus::Verified => MatchSnapshotVerificationStatus::Verified,
    };

    build_canonical_match_snapshot(
        query.projection(),
        generated_at,
        verification_status,
        vec![],
    )
    .map_err(|error| {
        tracing::error!(%match_id, error = %error, "snapshot contract mapping failed");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal.unexpected",
            "internal",
            "the canonical match snapshot could not be generated",
            false,
        )
    })
}

fn parse_match_id(value: &str) -> Result<MatchId, ()> {
    let uuid = Uuid::parse_str(value).map_err(|_| ())?;
    if uuid.get_version() != Some(Version::SortRand) {
        return Err(());
    }
    Ok(MatchId::from_uuid(uuid))
}

fn api_error(
    status: StatusCode,
    code: &str,
    category: &str,
    message: &str,
    retryable: bool,
) -> Response {
    (
        status,
        Json(ApiErrorResponse {
            error: ApiErrorBody {
                code: code.to_owned(),
                category: category.to_owned(),
                message: message.to_owned(),
                details: ErrorDetails::new(),
                retryable,
            },
            meta: ApiErrorMeta::default(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::parse_match_id;

    #[test]
    fn public_match_ids_require_uuidv7() {
        assert!(parse_match_id("019ff430-3bc5-7cf0-b3d2-0728c077a930").is_ok());
        assert!(parse_match_id("550e8400-e29b-41d4-a716-446655440000").is_err());
        assert!(parse_match_id("not-a-uuid").is_err());
    }
}
